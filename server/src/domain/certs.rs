//! Certifications: issuance, validity and organization requirement status.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use crate::domain::exam::expiry_date;
use crate::error::AppResult;

/// SQL predicate (alias `c`) for a certification that currently counts.
pub const VALID: &str =
    "c.revoked_at IS NULL AND c.superseded_by IS NULL AND (c.expires_at IS NULL OR c.expires_at > now())";

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Certification {
    pub id: Uuid,
    pub track_id: Uuid,
    pub track_slug: String,
    pub track_title: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub superseded: bool,
    pub provisional: bool,
}

impl Certification {
    pub fn status(&self, now: DateTime<Utc>) -> &'static str {
        if self.revoked_at.is_some() {
            "revoked"
        } else if self.superseded {
            "superseded"
        } else if self.expires_at.is_some_and(|e| e <= now) {
            "expired"
        } else {
            "valid"
        }
    }
}

const SELECT: &str = "SELECT c.id, c.track_id, t.slug AS track_slug, t.title AS track_title, c.issued_at, c.expires_at,
    c.revoked_at, (c.superseded_by IS NOT NULL) AS superseded, c.provisional
    FROM certifications c JOIN tracks t ON t.id = c.track_id";

pub async fn for_user(db: &PgPool, user_id: Uuid) -> AppResult<Vec<Certification>> {
    Ok(sqlx::query_as(crate::const_sql!("{SELECT} WHERE c.user_id = $1 ORDER BY c.issued_at DESC"))
        .bind(user_id)
        .fetch_all(db)
        .await?)
}

/// The certification a learner currently holds for a track (valid or most recent).
pub async fn current(db: &PgPool, user_id: Uuid, track_id: Uuid) -> AppResult<Option<Certification>> {
    Ok(sqlx::query_as(crate::const_sql!(
        "{SELECT} WHERE c.user_id = $1 AND c.track_id = $2 AND c.superseded_by IS NULL ORDER BY c.issued_at DESC LIMIT 1"
    ))
    .bind(user_id)
    .bind(track_id)
    .fetch_optional(db)
    .await?)
}

pub async fn has_valid(db: &PgPool, user_id: Uuid, track_slug: &str) -> AppResult<bool> {
    Ok(sqlx::query_scalar(crate::const_sql!(
        "SELECT EXISTS (SELECT 1 FROM certifications c JOIN tracks t ON t.id = c.track_id
         WHERE c.user_id = $1 AND t.slug = $2 AND {VALID})"
    ))
    .bind(user_id)
    .bind(track_slug)
    .fetch_one(db)
    .await?)
}

/// Issues a certification, superseding the previous one for the same track.
/// A renewal extends from the previous expiry date so early recertification is not penalized.
#[allow(clippy::too_many_arguments)]
pub async fn issue<'e>(
    tx: &mut sqlx::Transaction<'e, sqlx::Postgres>,
    user_id: Uuid,
    track_id: Uuid,
    attempt_id: Uuid,
    validity_months: Option<i32>,
    product_major: Option<&str>,
    provisional: bool,
    renewal_of: Option<(Uuid, Option<DateTime<Utc>>)>,
) -> AppResult<(Uuid, Option<DateTime<Utc>>)> {
    let now = Utc::now();
    let base = match renewal_of {
        Some((_, Some(prev_expiry))) if prev_expiry > now => prev_expiry,
        _ => now,
    };
    let expires = expiry_date(base, validity_months);
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO certifications (id, user_id, track_id, attempt_id, issued_at, expires_at, product_major, provisional)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(id)
    .bind(user_id)
    .bind(track_id)
    .bind(attempt_id)
    .bind(now)
    .bind(expires)
    .bind(product_major)
    .bind(provisional)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "UPDATE certifications SET superseded_by = $1
         WHERE user_id = $2 AND track_id = $3 AND id <> $1 AND superseded_by IS NULL AND revoked_at IS NULL",
    )
    .bind(id)
    .bind(user_id)
    .bind(track_id)
    .execute(&mut **tx)
    .await?;
    Ok((id, expires))
}

#[derive(Debug, Serialize)]
pub struct LevelStatus {
    pub slug: String,
    pub name: String,
    pub rank: i32,
    pub requirements: Vec<RequirementStatus>,
    pub met: bool,
}

#[derive(Debug, Serialize)]
pub struct RequirementStatus {
    pub track_slug: String,
    pub track_title: String,
    pub required: i64,
    pub valid: i64,
    pub missing: i64,
}

/// Valid certifications per track for an organization's approved members.
pub async fn valid_counts<'e>(db: impl PgExecutor<'e>, org_id: Uuid) -> AppResult<BTreeMap<String, i64>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(crate::const_sql!(
        "SELECT t.slug, count(DISTINCT c.user_id) FROM certifications c
         JOIN tracks t ON t.id = c.track_id
         JOIN memberships m ON m.user_id = c.user_id AND m.org_id = $1 AND m.status = 'approved'
         WHERE {VALID} GROUP BY t.slug"
    ))
    .bind(org_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().collect())
}

/// Requirement levels of the organization kind, with the gap for each.
pub async fn level_statuses(db: &PgPool, org_id: Uuid, org_kind: &str) -> AppResult<Vec<LevelStatus>> {
    let counts = valid_counts(db, org_id).await?;
    let levels: Vec<(String, String, i32, serde_json::Value)> = sqlx::query_as(
        "SELECT slug, name, rank, requirements FROM requirement_levels WHERE org_kind = $1 ORDER BY rank",
    )
    .bind(org_kind)
    .fetch_all(db)
    .await?;
    let titles: BTreeMap<String, String> = sqlx::query_as::<_, (String, String)>("SELECT slug, title FROM tracks")
        .fetch_all(db)
        .await?
        .into_iter()
        .collect();
    Ok(levels
        .into_iter()
        .map(|(slug, name, rank, req)| {
            let req: BTreeMap<String, i64> = serde_json::from_value(req).unwrap_or_default();
            let requirements: Vec<RequirementStatus> = req
                .into_iter()
                .map(|(track_slug, required)| {
                    let valid = counts.get(&track_slug).copied().unwrap_or(0);
                    RequirementStatus {
                        track_title: titles.get(&track_slug).cloned().unwrap_or_else(|| track_slug.clone()),
                        track_slug,
                        required,
                        valid,
                        missing: (required - valid).max(0),
                    }
                })
                .collect();
            let met = requirements.iter().all(|r| r.missing == 0);
            LevelStatus { slug, name, rank, requirements, met }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn status_precedence() {
        let now = Utc::now();
        let mut c = Certification {
            id: Uuid::nil(),
            track_id: Uuid::nil(),
            track_slug: "x".into(),
            track_title: "X".into(),
            issued_at: now - Duration::days(10),
            expires_at: Some(now + Duration::days(1)),
            revoked_at: None,
            superseded: false,
            provisional: false,
        };
        assert_eq!(c.status(now), "valid");
        c.expires_at = Some(now - Duration::days(1));
        assert_eq!(c.status(now), "expired");
        c.revoked_at = Some(now);
        assert_eq!(c.status(now), "revoked");
    }
}
