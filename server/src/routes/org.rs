//! Training manager dashboard and channel manager views of organizations.
//!
//! Data is strictly partitioned: a training manager only ever queries rows of the
//! organization they manage (`CurrentUser::managed_org`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit;
use crate::auth::{self, CurrentUser, Role};
use crate::domain::certs;
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::state::AppState;

pub async fn org_overview(state: &AppState, org_id: Uuid, include_join_code: bool) -> AppResult<Value> {
    let (name, kind, level, join_code): (String, String, Option<String>, String) =
        sqlx::query_as("SELECT name, kind, level_slug, join_code FROM organizations WHERE id = $1")
            .bind(org_id)
            .fetch_one(&state.db)
            .await?;
    let members: Option<Value> = sqlx::query_scalar(crate::const_sql!(
        "SELECT jsonb_agg(x ORDER BY x.status DESC, x.name) FROM (
            SELECT u.id, u.display_name AS name, u.email, m.status, m.org_role, m.requested_at,
              (SELECT jsonb_agg(jsonb_build_object('track_slug', t.slug, 'track_title', t.title,
                    'modules_total', (SELECT count(*) FROM modules mo WHERE mo.track_id = t.id),
                    'modules_completed', (SELECT count(*) FROM module_progress p JOIN modules mo ON mo.id = p.module_id
                        WHERE mo.track_id = t.id AND p.user_id = u.id AND p.completed_at IS NOT NULL),
                    'enrolled_at', e.enrolled_at) ORDER BY t.position)
               FROM enrollments e JOIN tracks t ON t.id = e.track_id WHERE e.user_id = u.id) AS progress,
              (SELECT jsonb_agg(jsonb_build_object('id', c.id, 'track_slug', t.slug, 'track_title', t.title,
                    'issued_at', c.issued_at, 'expires_at', c.expires_at, 'provisional', c.provisional))
               FROM certifications c JOIN tracks t ON t.id = c.track_id WHERE c.user_id = u.id AND {}) AS certifications
            FROM memberships m JOIN users u ON u.id = m.user_id WHERE m.org_id = $1 AND m.status <> 'rejected') x",
        certs::VALID
    ))
    .bind(org_id)
    .fetch_one(&state.db)
    .await?;
    let levels = certs::level_statuses(&state.db, org_id, &kind).await?;
    let counts = certs::valid_counts(&state.db, org_id).await?;
    Ok(json!({
        "id": org_id,
        "name": name,
        "kind": kind,
        "level": level,
        "join_code": if include_join_code { Some(join_code) } else { None },
        "members": members.unwrap_or(json!([])),
        "valid_certifications": counts,
        "levels": levels,
    }))
}

fn managed(user: &CurrentUser) -> AppResult<Uuid> {
    user.managed_org().ok_or(AppError::Forbidden("not_training_manager"))
}

pub async fn my_org(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let org = managed(&user)?;
    Ok(Json(org_overview(&state, org, true).await?))
}

#[derive(Deserialize)]
pub struct Decision {
    approve: bool,
}

pub async fn decide_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(member): Path<Uuid>,
    Json(d): Json<Decision>,
) -> AppResult<Json<Value>> {
    let org = managed(&user)?;
    let mut tx = state.db.begin().await?;
    let row: Option<(String, String, String)> = sqlx::query_as(
        "UPDATE memberships m SET status = $3, decided_at = now(), decided_by = $4
         FROM users u, organizations o
         WHERE m.user_id = $1 AND m.org_id = $2 AND u.id = m.user_id AND o.id = m.org_id
         RETURNING u.email, u.display_name, o.name",
    )
    .bind(member)
    .bind(org)
    .bind(if d.approve { "approved" } else { "rejected" })
    .bind(user.id)
    .fetch_optional(&mut *tx)
    .await?;
    let (email, name, org_name) = row.ok_or(AppError::NotFound)?;
    mail::queue_template(
        &mut *tx,
        &state,
        &email,
        "membership_decided",
        json!({ "name": name, "org": org_name, "approved": d.approve, "link": state.config.public_url("/") }),
    )
    .await?;
    audit::log(
        &mut *tx,
        Some(user.id),
        "membership.decide",
        Some(member.to_string()),
        json!({ "org": org, "approve": d.approve }),
    )
    .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct OrgRole {
    org_role: String,
}

pub async fn set_member_role(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(member): Path<Uuid>,
    Json(r): Json<OrgRole>,
) -> AppResult<Json<Value>> {
    let org = managed(&user)?;
    if !matches!(r.org_role.as_str(), "learner" | "training_manager") {
        return Err(AppError::bad_request("invalid_role", "invalid organization role"));
    }
    if member == user.id && r.org_role != "training_manager" {
        return Err(AppError::conflict("cannot_demote_self", "ask another manager to change your role"));
    }
    let res =
        sqlx::query("UPDATE memberships SET org_role = $3 WHERE user_id = $1 AND org_id = $2 AND status = 'approved'")
            .bind(member)
            .bind(org)
            .bind(&r.org_role)
            .execute(&state.db)
            .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::log(
        &state.db,
        Some(user.id),
        "membership.role",
        Some(member.to_string()),
        json!({ "org": org, "role": r.org_role }),
    )
    .await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn remove_member(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(member): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let org = managed(&user)?;
    if member == user.id {
        return Err(AppError::conflict("cannot_remove_self", "leave the organization from your profile"));
    }
    let res = sqlx::query("DELETE FROM memberships WHERE user_id = $1 AND org_id = $2")
        .bind(member)
        .bind(org)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::log(&state.db, Some(user.id), "membership.remove", Some(member.to_string()), json!({ "org": org })).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn rotate_join_code(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let org = managed(&user)?;
    let code = new_join_code();
    sqlx::query("UPDATE organizations SET join_code = $2 WHERE id = $1")
        .bind(org)
        .bind(&code)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "join_code": code })))
}

pub fn new_join_code() -> String {
    auth::random_token()[..12].to_string()
}

// ---------------------------------------------------------------------------
// Channel managers: all organizations, export
// ---------------------------------------------------------------------------

pub async fn list_orgs(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require_any(&[Role::ChannelManager, Role::Admin])?;
    let orgs: Vec<(Uuid, String, String, Option<String>, i64, i64)> = sqlx::query_as(
        "SELECT o.id, o.name, o.kind, o.level_slug,
            (SELECT count(*) FROM memberships m WHERE m.org_id = o.id AND m.status = 'approved'),
            (SELECT count(*) FROM memberships m WHERE m.org_id = o.id AND m.status = 'pending')
         FROM organizations o ORDER BY o.kind, o.name",
    )
    .fetch_all(&state.db)
    .await?;
    let mut out = Vec::new();
    for (id, name, kind, level, members, pending) in orgs {
        let levels = certs::level_statuses(&state.db, id, &kind).await?;
        let current = level.as_ref().and_then(|l| levels.iter().find(|s| &s.slug == l));
        out.push(json!({
            "id": id, "name": name, "kind": kind, "level": level, "members": members, "pending": pending,
            "valid_certifications": certs::valid_counts(&state.db, id).await?,
            "level_met": current.map(|c| c.met),
            "highest_level_met": levels.iter().filter(|l| l.met).max_by_key(|l| l.rank).map(|l| &l.slug),
        }));
    }
    Ok(Json(json!(out)))
}

pub async fn org_detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require_any(&[Role::ChannelManager, Role::Admin])?;
    Ok(Json(org_overview(&state, id, user.has_role(Role::Admin)).await?))
}

pub async fn certified_rows(state: &AppState) -> AppResult<Vec<(String, String, String, String, i64)>> {
    Ok(sqlx::query_as(crate::const_sql!(
        "SELECT o.id::text, o.name, o.kind, t.slug, count(DISTINCT c.user_id)
         FROM organizations o
         JOIN memberships m ON m.org_id = o.id AND m.status = 'approved'
         JOIN certifications c ON c.user_id = m.user_id
         JOIN tracks t ON t.id = c.track_id
         WHERE {} GROUP BY o.id, o.name, o.kind, t.slug ORDER BY o.name, t.slug",
        certs::VALID
    ))
    .fetch_all(&state.db)
    .await?)
}

/// CSV of valid certifications per organization and track, for tier reviews.
pub async fn export_csv(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    user.require_any(&[Role::ChannelManager, Role::Admin])?;
    let rows = certified_rows(&state).await?;
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(["organization_id", "organization", "kind", "track", "valid_certifications"])
        .map_err(anyhow::Error::from)?;
    for (id, name, kind, track, count) in rows {
        w.write_record([id, csv_safe(&name), kind, track, count.to_string()]).map_err(anyhow::Error::from)?;
    }
    let body = w.into_inner().map_err(|e| anyhow::anyhow!("{e}"))?;
    audit::log(&state.db, Some(user.id), "export.certified_csv", None, json!({})).await?;
    Ok((
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8"),
            (header::CONTENT_DISPOSITION, "attachment; filename=\"certified.csv\""),
        ],
        body,
    )
        .into_response())
}

/// Neutralizes spreadsheet formula injection in exported text fields.
pub fn csv_safe(s: &str) -> String {
    if s.starts_with(['=', '+', '-', '@', '\t', '\r']) { format!("'{s}") } else { s.to_string() }
}

/// Machine API for the partner portal: `Authorization: Bearer <api token>`.
pub async fn api_certified(State(state): State<AppState>, headers: axum::http::HeaderMap) -> AppResult<Json<Value>> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(AppError::Unauthorized)?;
    let id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE api_tokens SET last_used_at = now() WHERE token_hash = $1 AND revoked_at IS NULL RETURNING id",
    )
    .bind(auth::hash_token(token.trim()))
    .fetch_optional(&state.db)
    .await?;
    id.ok_or(AppError::Unauthorized)?;
    let rows = certified_rows(&state).await?;
    let mut orgs: std::collections::BTreeMap<String, Value> = std::collections::BTreeMap::new();
    for (id, name, kind, track, count) in rows {
        let e = orgs
            .entry(id.clone())
            .or_insert_with(|| json!({ "id": id, "name": name, "kind": kind, "valid_certifications": {} }));
        e["valid_certifications"][track] = json!(count);
    }
    Ok(Json(json!({ "generated_at": chrono::Utc::now(), "organizations": orgs.into_values().collect::<Vec<_>>() })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_injection_neutralized() {
        assert_eq!(csv_safe("=HYPERLINK(1)"), "'=HYPERLINK(1)");
        assert_eq!(csv_safe("Acme"), "Acme");
    }
}
