//! Background jobs: expiry alerts, closing abandoned exam sections, housekeeping.
//! Safe to run on several replicas: alerts are deduplicated by a primary key.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use crate::domain::{attempts, certs};
use crate::mail;
use crate::state::AppState;

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        let mut minutes = 0u64;
        loop {
            tick.tick().await;
            if let Err(e) = attempts::close_overdue(&state).await {
                tracing::error!(error = ?e, "closing overdue attempts failed");
            }
            if minutes.is_multiple_of(15) {
                if let Err(e) = expiry_alerts(&state).await {
                    tracing::error!(error = ?e, "expiry alerts failed");
                }
                if let Err(e) = housekeeping(&state).await {
                    tracing::error!(error = ?e, "housekeeping failed");
                }
            }
            minutes += 1;
        }
    });
}

/// Smallest alert threshold (in days) the expiry falls within, if any.
pub fn alert_threshold(thresholds: &[i64], expires: DateTime<Utc>, now: DateTime<Utc>) -> Option<i64> {
    let mut sorted = thresholds.to_vec();
    sorted.sort_unstable();
    sorted.into_iter().find(|d| expires <= now + chrono::Duration::days(*d))
}

#[derive(sqlx::FromRow)]
struct Expiring {
    id: Uuid,
    expires_at: DateTime<Utc>,
    email: String,
    name: String,
    track_slug: String,
    track_title: String,
    org_id: Option<Uuid>,
    org_name: Option<String>,
}

pub async fn expiry_alerts(state: &AppState) -> anyhow::Result<usize> {
    let thresholds = &state.config.alerts.expiry_days;
    let Some(max) = thresholds.iter().max().copied() else { return Ok(0) };
    let now = Utc::now();
    let rows: Vec<Expiring> = sqlx::query_as(crate::const_sql!(
        "SELECT c.id, c.expires_at, u.email, u.display_name AS name, t.slug AS track_slug, t.title AS track_title,
            m.org_id, o.name AS org_name
         FROM certifications c JOIN users u ON u.id = c.user_id JOIN tracks t ON t.id = c.track_id
         LEFT JOIN memberships m ON m.user_id = u.id AND m.status = 'approved'
         LEFT JOIN organizations o ON o.id = m.org_id
         WHERE {} AND c.expires_at IS NOT NULL AND c.expires_at <= now() + make_interval(days => $1)",
        certs::VALID
    ))
    .bind(max as i32)
    .fetch_all(&state.db)
    .await?;
    let fmt = state.renderer.raw("date.format").to_string();
    let mut sent = 0;
    for c in rows {
        let Some(days) = alert_threshold(thresholds, c.expires_at, now) else { continue };
        let mut tx = state.db.begin().await?;
        // Record this threshold and every larger one, so a late first alert is not followed by stale ones.
        let mut fresh = false;
        for d in thresholds.iter().filter(|d| **d >= days) {
            let res = sqlx::query(
                "INSERT INTO expiry_alerts (certification_id, days_before) VALUES ($1, $2) ON CONFLICT DO NOTHING",
            )
            .bind(c.id)
            .bind(*d as i32)
            .execute(&mut *tx)
            .await?;
            if *d == days && res.rows_affected() == 1 {
                fresh = true;
            }
        }
        if !fresh {
            tx.rollback().await?;
            continue;
        }
        let expires = c.expires_at.format(&fmt).to_string();
        let remaining = (c.expires_at - now).num_days().max(0);
        mail::queue_template(
            &mut *tx,
            state,
            &c.email,
            "expiry_learner",
            json!({ "name": c.name, "track": c.track_title, "days": remaining, "expires": expires,
                    "link": state.config.public_url(&format!("/tracks/{}", c.track_slug)) }),
        )
        .await?;
        if let Some(org) = c.org_id {
            let managers: Vec<String> = sqlx::query_scalar(
                "SELECT u.email FROM memberships m JOIN users u ON u.id = m.user_id
                 WHERE m.org_id = $1 AND m.org_role = 'training_manager' AND m.status = 'approved' AND lower(u.email) <> lower($2)",
            )
            .bind(org)
            .bind(&c.email)
            .fetch_all(&mut *tx)
            .await?;
            for m in managers {
                mail::queue_template(
                    &mut *tx,
                    state,
                    &m,
                    "expiry_manager",
                    json!({ "learner": c.name, "org": c.org_name, "track": c.track_title, "days": remaining,
                            "expires": expires, "link": state.config.public_url("/organization") }),
                )
                .await?;
            }
        }
        tx.commit().await?;
        sent += 1;
    }
    Ok(sent)
}

async fn housekeeping(state: &AppState) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM sessions WHERE expires_at < now()").execute(&state.db).await?;
    sqlx::query("DELETE FROM login_tokens WHERE expires_at < now() - interval '1 day'").execute(&state.db).await?;
    sqlx::query("DELETE FROM oidc_flows WHERE created_at < now() - interval '1 hour'").execute(&state.db).await?;
    sqlx::query("DELETE FROM email_outbox WHERE sent_at < now() - interval '30 days'").execute(&state.db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn picks_smallest_matching_threshold() {
        let now = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let t = [90, 30, 7];
        assert_eq!(alert_threshold(&t, now + chrono::Duration::days(100), now), None);
        assert_eq!(alert_threshold(&t, now + chrono::Duration::days(60), now), Some(90));
        assert_eq!(alert_threshold(&t, now + chrono::Duration::days(20), now), Some(30));
        assert_eq!(alert_threshold(&t, now + chrono::Duration::days(3), now), Some(7));
    }
}
