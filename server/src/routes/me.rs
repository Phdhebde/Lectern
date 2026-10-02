//! The signed-in user's account: profile, organization membership, certifications
//! and GDPR rights (access/export, rectification, erasure).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit;
use crate::auth::{self, CurrentUser, Role};
use crate::credentials::{self, CertificateInput};
use crate::domain::certs;
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::state::AppState;

pub async fn me(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let pending_reviews: i64 = if user.has_role(Role::Trainer) {
        sqlx::query_scalar("SELECT count(*) FROM exam_attempts WHERE status = 'pending_review'")
            .fetch_one(&state.db)
            .await?
    } else {
        0
    };
    Ok(Json(json!({
        "id": user.id,
        "email": user.email,
        "display_name": user.display_name,
        "public_profile": user.public_profile,
        "roles": user.roles,
        "mfa": user.mfa,
        "mfa_required_for": state.config.auth.require_mfa_for,
        "membership": user.membership,
        "csrf_token": user.csrf_token,
        "pending_reviews": pending_reviews,
    })))
}

#[derive(Deserialize)]
pub struct ProfileUpdate {
    display_name: Option<String>,
    public_profile: Option<bool>,
}

pub async fn update_profile(State(state): State<AppState>, user: CurrentUser, Json(req): Json<ProfileUpdate>) -> AppResult<Json<Value>> {
    if let Some(name) = &req.display_name {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(AppError::bad_request("invalid_name", "name must be 1-120 characters"));
        }
        sqlx::query("UPDATE users SET display_name = $2 WHERE id = $1").bind(user.id).bind(name).execute(&state.db).await?;
    }
    if let Some(public) = req.public_profile {
        sqlx::query("UPDATE users SET public_profile = $2 WHERE id = $1").bind(user.id).bind(public).execute(&state.db).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

pub async fn logout(State(state): State<AppState>, jar: CookieJar, user: CurrentUser) -> AppResult<(CookieJar, Json<Value>)> {
    sqlx::query("DELETE FROM sessions WHERE token_hash = $1").bind(&user.session_hash).execute(&state.db).await?;
    Ok((jar.add(auth::clear_session_cookie(&state)), Json(json!({ "ok": true }))))
}

/// Right of access / portability: everything stored about the user, as JSON.
pub async fn export(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    let db = &state.db;
    let profile: Value = sqlx::query_scalar(
        "SELECT to_jsonb(u) - 'id' FROM (SELECT email, display_name, public_profile, created_at, last_login_at FROM users WHERE id = $1) u",
    )
    .bind(user.id)
    .fetch_one(db)
    .await?;
    let q = |sql: &'static str| sqlx::query_scalar::<_, Option<Value>>(sql).bind(user.id).fetch_one(db);
    let data = json!({
        "exported_at": Utc::now(),
        "profile": profile,
        "membership": user.membership,
        "roles": user.roles,
        "enrollments": q("SELECT jsonb_agg(to_jsonb(e) - 'user_id') FROM (SELECT t.slug, e.enrolled_at, e.completed_at, e.rating FROM enrollments e JOIN tracks t ON t.id = e.track_id WHERE e.user_id = $1) e").await?,
        "module_progress": q("SELECT jsonb_agg(x) FROM (SELECT t.slug AS track, m.slug AS module, p.video_completed, p.quiz_best_score, p.completed_at FROM module_progress p JOIN modules m ON m.id = p.module_id JOIN tracks t ON t.id = m.track_id WHERE p.user_id = $1) x").await?,
        "scenario_progress": q("SELECT jsonb_agg(x) FROM (SELECT s.slug, p.current_step, p.completed_at FROM scenario_progress p JOIN scenarios s ON s.id = p.scenario_id WHERE p.user_id = $1) x").await?,
        "exam_attempts": q("SELECT jsonb_agg(x) FROM (SELECT a.id, t.slug AS track, a.purpose, a.status, a.started_at, a.finished_at, a.results, a.review->'comment' AS reviewer_comment FROM exam_attempts a JOIN tracks t ON t.id = a.track_id WHERE a.user_id = $1) x").await?,
        "certifications": q("SELECT jsonb_agg(x) FROM (SELECT c.id, t.slug AS track, c.issued_at, c.expires_at, c.revoked_at FROM certifications c JOIN tracks t ON t.id = c.track_id WHERE c.user_id = $1) x").await?,
    });
    let body = serde_json::to_string_pretty(&data).map_err(anyhow::Error::from)?;
    Ok((
        [(header::CONTENT_TYPE, "application/json"), (header::CONTENT_DISPOSITION, "attachment; filename=\"my-data.json\"")],
        body,
    )
        .into_response())
}

/// Right to erasure. Anonymous per-question statistics are kept; the audit log keeps
/// event records without the link to the person.
pub async fn delete_account(State(state): State<AppState>, jar: CookieJar, user: CurrentUser) -> AppResult<(CookieJar, Json<Value>)> {
    let mut tx = state.db.begin().await?;
    audit::log(&mut *tx, None, "account.deleted", None, json!({})).await?;
    sqlx::query("UPDATE audit_log SET details = details - 'user' WHERE details->>'user' = $1")
        .bind(user.id.to_string())
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM email_outbox WHERE lower(to_address) = lower($1)").bind(&user.email).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM login_tokens WHERE lower(email) = lower($1)").bind(&user.email).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM users WHERE id = $1").bind(user.id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok((jar.add(auth::clear_session_cookie(&state)), Json(json!({ "ok": true }))))
}

#[derive(Deserialize)]
pub struct JoinRequest {
    join_code: String,
}

pub async fn join_org(State(state): State<AppState>, user: CurrentUser, Json(req): Json<JoinRequest>) -> AppResult<Json<Value>> {
    if user.membership.as_ref().is_some_and(|m| m.status != "rejected") {
        return Err(AppError::conflict("already_member", "leave your current organization first"));
    }
    let org: Option<(Uuid, String)> = sqlx::query_as("SELECT id, name FROM organizations WHERE join_code = $1")
        .bind(req.join_code.trim())
        .fetch_optional(&state.db)
        .await?;
    let (org_id, org_name) = org.ok_or_else(|| AppError::bad_request("invalid_join_code", "unknown code"))?;
    let mut tx = state.db.begin().await?;
    sqlx::query(
        "INSERT INTO memberships (user_id, org_id, status) VALUES ($1, $2, 'pending')
         ON CONFLICT (user_id) DO UPDATE SET org_id = $2, status = 'pending', org_role = 'learner',
            requested_at = now(), decided_at = NULL, decided_by = NULL",
    )
    .bind(user.id)
    .bind(org_id)
    .execute(&mut *tx)
    .await?;
    let managers: Vec<String> = sqlx::query_scalar(
        "SELECT u.email FROM memberships m JOIN users u ON u.id = m.user_id
         WHERE m.org_id = $1 AND m.org_role = 'training_manager' AND m.status = 'approved'",
    )
    .bind(org_id)
    .fetch_all(&mut *tx)
    .await?;
    for m in managers {
        mail::queue_template(
            &mut *tx,
            &state,
            &m,
            "membership_requested",
            json!({ "learner": user.display_name, "email": user.email, "org": org_name, "link": state.config.public_url("/organization") }),
        )
        .await?;
    }
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "organization": org_name })))
}

pub async fn leave_org(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    sqlx::query("DELETE FROM memberships WHERE user_id = $1").bind(user.id).execute(&state.db).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn certifications(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let now = Utc::now();
    let list = certs::for_user(&state.db, user.id).await?;
    let out: Vec<Value> = list
        .into_iter()
        .map(|c| {
            json!({
                "id": c.id,
                "track_slug": c.track_slug,
                "track_title": c.track_title,
                "issued_at": c.issued_at,
                "expires_at": c.expires_at,
                "status": c.status(now),
                "provisional": c.provisional,
                "verify_url": state.config.public_url(&format!("/verify/{}", c.id)),
                "linkedin_add_url": credentials::linkedin_add_url(&state, c.id, &c.track_title, c.issued_at, c.expires_at),
            })
        })
        .collect();
    Ok(Json(json!(out)))
}

pub async fn attempts(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    let rows: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.started_at DESC) FROM (
            SELECT a.id, t.slug AS track_slug, t.title AS track_title, a.purpose, a.status, a.started_at, a.finished_at,
                   a.results, a.review->'comment' AS reviewer_comment
            FROM exam_attempts a JOIN tracks t ON t.id = a.track_id WHERE a.user_id = $1) x",
    )
    .bind(user.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

/// PDF certificate. Available to the holder, their training manager and staff.
#[allow(clippy::type_complexity)]
pub async fn certificate_pdf(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    let row: (Uuid, String, String, chrono::DateTime<Utc>, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>) = sqlx::query_as(
        "SELECT c.user_id, u.display_name, t.title, c.issued_at, c.expires_at, c.revoked_at
         FROM certifications c JOIN users u ON u.id = c.user_id JOIN tracks t ON t.id = c.track_id WHERE c.id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await?;
    let (owner, holder, track, issued, expires, revoked) = row;
    let allowed = owner == user.id
        || user.require_any(&[Role::Admin, Role::ChannelManager]).is_ok()
        || match user.managed_org() {
            Some(org) => sqlx::query_scalar::<_, bool>(
                "SELECT EXISTS (SELECT 1 FROM memberships WHERE user_id = $1 AND org_id = $2 AND status = 'approved')",
            )
            .bind(owner)
            .bind(org)
            .fetch_one(&state.db)
            .await?,
            None => false,
        };
    if !allowed {
        return Err(AppError::NotFound);
    }
    if revoked.is_some() {
        return Err(AppError::conflict("revoked", "this certification was revoked"));
    }
    let st = state.clone();
    let pdf = tokio::task::spawn_blocking(move || {
        credentials::certificate_pdf(&st, &CertificateInput { id, holder: &holder, track_title: &track, issued, expires })
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"certificate-{id}.pdf\"")),
        ],
        pdf,
    )
        .into_response())
}
