//! Administration and content authoring (back-office). Every write is audited.

use axum::Json;
use axum::body::Bytes;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit;
use crate::auth::{self, CurrentUser, Role};
use crate::domain::attempts;
use crate::domain::catalog;
use crate::domain::certs;
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::pack::{self, Annotation, ChoiceFile, QuestionFile, TrackFile, is_slug};
use crate::routes::org::new_join_code;
use crate::state::AppState;

fn admin(user: &CurrentUser) -> AppResult<()> {
    user.require(Role::Admin)
}

/// Content authors: administrators and trainers.
fn author(user: &CurrentUser) -> AppResult<()> {
    user.require_any(&[Role::Admin, Role::Trainer]).map(|_| ())
}

fn bad(code: &'static str, e: impl std::fmt::Display) -> AppError {
    AppError::bad_request(code, e.to_string())
}

// ---------------------------------------------------------------------------
// Organizations and users
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct OrgReq {
    name: String,
    kind: String,
    #[serde(default)]
    level_slug: Option<String>,
}

pub async fn create_org(State(state): State<AppState>, user: CurrentUser, Json(r): Json<OrgReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    if r.name.trim().is_empty() || !matches!(r.kind.as_str(), "partner" | "customer") {
        return Err(AppError::bad_request("invalid_org", "name required; kind is partner or customer"));
    }
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, kind, level_slug, join_code) VALUES ($1, $2, $3, $4, $5)")
        .bind(id)
        .bind(r.name.trim())
        .bind(&r.kind)
        .bind(&r.level_slug)
        .bind(new_join_code())
        .execute(&state.db)
        .await?;
    audit::log(&state.db, Some(user.id), "org.create", Some(id.to_string()), json!({ "name": r.name, "kind": r.kind })).await?;
    Ok(Json(json!({ "id": id })))
}

pub async fn update_org(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(r): Json<OrgReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    if r.name.trim().is_empty() || !matches!(r.kind.as_str(), "partner" | "customer") {
        return Err(AppError::bad_request("invalid_org", "name required; kind is partner or customer"));
    }
    let res = sqlx::query("UPDATE organizations SET name = $2, kind = $3, level_slug = $4 WHERE id = $1")
        .bind(id)
        .bind(r.name.trim())
        .bind(&r.kind)
        .bind(&r.level_slug)
        .execute(&state.db)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    audit::log(&state.db, Some(user.id), "org.update", Some(id.to_string()), json!({ "name": r.name, "kind": r.kind, "level": r.level_slug })).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_org(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    admin(&user)?;
    sqlx::query("DELETE FROM organizations WHERE id = $1").bind(id).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "org.delete", Some(id.to_string()), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ManagerReq {
    email: String,
    #[serde(default)]
    display_name: String,
}

/// Attaches a user (created if needed) to the organization as an approved training manager.
pub async fn add_manager(State(state): State<AppState>, user: CurrentUser, Path(org): Path<Uuid>, Json(r): Json<ManagerReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    if !auth::email::is_plausible_email(&r.email) {
        return Err(AppError::bad_request("invalid_email", "invalid e-mail address"));
    }
    let uid = auth::upsert_user_by_email(&state.db, &r.email, &r.display_name).await?;
    sqlx::query(
        "INSERT INTO memberships (user_id, org_id, status, org_role, decided_at, decided_by) VALUES ($1, $2, 'approved', 'training_manager', now(), $3)
         ON CONFLICT (user_id) DO UPDATE SET org_id = $2, status = 'approved', org_role = 'training_manager', decided_at = now(), decided_by = $3",
    )
    .bind(uid)
    .bind(org)
    .bind(user.id)
    .execute(&state.db)
    .await?;
    audit::log(&state.db, Some(user.id), "org.add_manager", Some(org.to_string()), json!({ "user": uid })).await?;
    Ok(Json(json!({ "user_id": uid })))
}

#[derive(Deserialize)]
pub struct UserQuery {
    #[serde(default)]
    q: String,
}

pub async fn list_users(State(state): State<AppState>, user: CurrentUser, Query(q): Query<UserQuery>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let pattern = format!("%{}%", q.q.trim().replace(['%', '_'], ""));
    let rows: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x) FROM (
            SELECT u.id, u.email, u.display_name, u.created_at, u.last_login_at,
                (SELECT coalesce(jsonb_agg(role), '[]') FROM user_roles r WHERE r.user_id = u.id) AS roles,
                o.name AS organization, m.status AS membership_status, m.org_role
            FROM users u LEFT JOIN memberships m ON m.user_id = u.id LEFT JOIN organizations o ON o.id = m.org_id
            WHERE u.email ILIKE $1 OR u.display_name ILIKE $1
            ORDER BY u.created_at DESC LIMIT 100) x",
    )
    .bind(pattern)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

#[derive(Deserialize)]
pub struct RoleReq {
    role: String,
    grant: bool,
}

pub async fn set_role(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(r): Json<RoleReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let role = Role::parse(&r.role).ok_or_else(|| AppError::bad_request("invalid_role", "unknown role"))?;
    if id == user.id && role == Role::Admin && !r.grant {
        return Err(AppError::conflict("cannot_demote_self", "another administrator must do this"));
    }
    if r.grant {
        sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(id)
            .bind(role.as_str())
            .execute(&state.db)
            .await?;
    } else {
        sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND role = $2").bind(id).bind(role.as_str()).execute(&state.db).await?;
    }
    audit::log(&state.db, Some(user.id), "user.role", Some(id.to_string()), json!({ "role": r.role, "grant": r.grant })).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct CreditReq {
    track_slug: String,
    #[serde(default)]
    reference: Option<String>,
}

/// Grants an extra exam attempt (until online payment is enabled).
pub async fn grant_credit(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(r): Json<CreditReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let track = catalog::load_track(&state.db, &r.track_slug).await?;
    sqlx::query("INSERT INTO attempt_credits (id, user_id, track_id, source, reference) VALUES ($1, $2, $3, 'grant', $4)")
        .bind(Uuid::new_v4())
        .bind(id)
        .bind(track.id)
        .bind(&r.reference)
        .execute(&state.db)
        .await?;
    audit::log(&state.db, Some(user.id), "credit.grant", Some(id.to_string()), json!({ "track": r.track_slug, "reference": r.reference })).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct RevokeReq {
    reason: String,
}

pub async fn revoke_cert(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>, Json(r): Json<RevokeReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    sqlx::query("UPDATE certifications SET revoked_at = now(), revoke_reason = $2 WHERE id = $1 AND revoked_at IS NULL")
        .bind(id)
        .bind(&r.reason)
        .execute(&state.db)
        .await?;
    audit::log(&state.db, Some(user.id), "cert.revoke", Some(id.to_string()), json!({ "reason": r.reason })).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct VersionReq {
    version: String,
}

/// Declares a new major product version: certifications obtained on older versions
/// expire at the latest after the configured grace period.
pub async fn declare_major_version(State(state): State<AppState>, user: CurrentUser, Json(r): Json<VersionReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let version = r.version.trim();
    if version.is_empty() {
        return Err(AppError::bad_request("invalid_version", "version required"));
    }
    let deadline = Utc::now() + Duration::days(state.config.alerts.major_version_grace_days);
    let res = sqlx::query(crate::const_sql!(
        "UPDATE certifications c SET expires_at = LEAST(c.expires_at, $2)
         FROM tracks t WHERE t.id = c.track_id AND t.validity_months IS NOT NULL
           AND c.product_major IS DISTINCT FROM $1 AND {}",
        certs::VALID
    ))
    .bind(version)
    .bind(deadline)
    .execute(&state.db)
    .await?;
    sqlx::query("INSERT INTO settings (key, value) VALUES ('declared_major_version', $1) ON CONFLICT (key) DO UPDATE SET value = $1")
        .bind(json!(version))
        .execute(&state.db)
        .await?;
    audit::log(&state.db, Some(user.id), "product.major_version", None, json!({ "version": version, "affected": res.rows_affected() })).await?;
    Ok(Json(json!({ "affected_certifications": res.rows_affected(), "expires_at_latest": deadline,
        "note": "set instance.product_major_version to the new version so new certifications record it" })))
}

// ---------------------------------------------------------------------------
// API tokens
// ---------------------------------------------------------------------------

pub async fn list_tokens(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    admin(&user)?;
    let rows: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.created_at DESC) FROM (SELECT id, name, created_at, last_used_at, revoked_at FROM api_tokens) x",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

#[derive(Deserialize)]
pub struct TokenReq {
    name: String,
}

pub async fn create_token(State(state): State<AppState>, user: CurrentUser, Json(r): Json<TokenReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let token = format!("lct_{}", auth::random_token());
    let id = Uuid::new_v4();
    sqlx::query("INSERT INTO api_tokens (id, name, token_hash, created_by) VALUES ($1, $2, $3, $4)")
        .bind(id)
        .bind(r.name.trim())
        .bind(auth::hash_token(&token))
        .bind(user.id)
        .execute(&state.db)
        .await?;
    audit::log(&state.db, Some(user.id), "api_token.create", Some(id.to_string()), json!({ "name": r.name })).await?;
    // Shown once; only the hash is stored.
    Ok(Json(json!({ "id": id, "token": token })))
}

pub async fn revoke_token(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Json<Value>> {
    admin(&user)?;
    sqlx::query("UPDATE api_tokens SET revoked_at = now() WHERE id = $1").bind(id).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "api_token.revoke", Some(id.to_string()), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Statistics and audit
// ---------------------------------------------------------------------------

pub async fn stats(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require_any(&[Role::Admin, Role::Trainer])?;
    let db = &state.db;
    let tracks: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.position) FROM (
            SELECT t.slug, t.title, t.position,
              (SELECT count(*) FROM enrollments e WHERE e.track_id = t.id) AS enrollments,
              (SELECT count(*) FROM enrollments e WHERE e.track_id = t.id AND e.completed_at IS NOT NULL) AS completions,
              (SELECT round(avg(rating), 2) FROM enrollments e WHERE e.track_id = t.id) AS average_rating,
              (SELECT count(*) FROM exam_attempts a WHERE a.track_id = t.id AND a.status IN ('passed', 'failed')) AS attempts,
              (SELECT count(*) FROM exam_attempts a WHERE a.track_id = t.id AND a.status = 'passed') AS passed,
              (SELECT count(*) FROM certifications c WHERE c.track_id = t.id AND c.revoked_at IS NULL AND c.superseded_by IS NULL
                 AND (c.expires_at IS NULL OR c.expires_at > now())) AS valid_certifications
            FROM tracks t) x",
    )
    .fetch_one(db)
    .await?;
    let questions: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.success_rate) FROM (
            SELECT q.ref, t.slug AS track, q.pool, left(q.prompt_md, 160) AS prompt, s.answered, s.correct,
                round(100.0 * s.correct / NULLIF(s.answered, 0), 1) AS success_rate,
                (s.answered >= 20 AND (s.correct::float / s.answered < 0.3 OR s.correct::float / s.answered > 0.97)) AS flagged
            FROM question_stats s JOIN questions q ON q.id = s.question_id JOIN tracks t ON t.id = q.track_id) x",
    )
    .fetch_one(db)
    .await?;
    let signups: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.month) FROM (
            SELECT to_char(date_trunc('month', created_at), 'YYYY-MM') AS month, count(*) AS users
            FROM users GROUP BY 1 ORDER BY 1 DESC LIMIT 12) x",
    )
    .fetch_one(db)
    .await?;
    let users: i64 = sqlx::query_scalar("SELECT count(*) FROM users").fetch_one(db).await?;
    Ok(Json(json!({ "users": users, "signups": signups, "tracks": tracks, "questions": questions })))
}

pub async fn audit_log(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    admin(&user)?;
    let rows: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.at DESC) FROM (
            SELECT a.id, a.at, a.action, a.target, a.details, u.email AS actor
            FROM audit_log a LEFT JOIN users u ON u.id = a.actor_id ORDER BY a.at DESC LIMIT 300) x",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

// ---------------------------------------------------------------------------
// Content packs
// ---------------------------------------------------------------------------

pub async fn import_pack(State(state): State<AppState>, user: CurrentUser, body: Bytes) -> AppResult<Json<Value>> {
    admin(&user)?;
    let limit = (state.config.server.max_upload_mb as u64) * 1024 * 1024 * 4;
    let files = pack::PackFiles::from_zip(&body, limit).map_err(|e| bad("invalid_pack", e))?;
    let report = pack::import(&state.db, &state.config.server.data_dir, &files)
        .await
        .map_err(|e| bad("invalid_pack", format!("{e:#}")))?;
    audit::log(&state.db, Some(user.id), "pack.import", None, serde_json::to_value(&report).map_err(anyhow::Error::from)?).await?;
    Ok(Json(json!(report)))
}

pub async fn export_pack(State(state): State<AppState>, user: CurrentUser) -> AppResult<Response> {
    admin(&user)?;
    let zip = pack::export(&state.db, &state.config.server.data_dir).await?;
    audit::log(&state.db, Some(user.id), "pack.export", None, json!({})).await?;
    Ok((
        [(header::CONTENT_TYPE, "application/zip"), (header::CONTENT_DISPOSITION, "attachment; filename=\"content-pack.zip\"")],
        zip,
    )
        .into_response())
}

#[derive(Deserialize)]
pub struct LevelReq {
    org_kind: String,
    name: String,
    rank: i32,
    requirements: std::collections::BTreeMap<String, i64>,
}

pub async fn list_levels(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require_any(&[Role::Admin, Role::ChannelManager])?;
    let rows: Option<Value> = sqlx::query_scalar("SELECT jsonb_agg(to_jsonb(l) ORDER BY l.org_kind, l.rank) FROM requirement_levels l")
        .fetch_one(&state.db)
        .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

pub async fn put_level(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>, Json(r): Json<LevelReq>) -> AppResult<Json<Value>> {
    admin(&user)?;
    if !is_slug(&slug) || !matches!(r.org_kind.as_str(), "partner" | "customer") {
        return Err(AppError::bad_request("invalid_level", "invalid slug or organization kind"));
    }
    sqlx::query(
        "INSERT INTO requirement_levels (slug, org_kind, name, rank, requirements) VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (slug) DO UPDATE SET org_kind = $2, name = $3, rank = $4, requirements = $5",
    )
    .bind(&slug)
    .bind(&r.org_kind)
    .bind(&r.name)
    .bind(r.rank)
    .bind(json!(r.requirements))
    .execute(&state.db)
    .await?;
    audit::log(&state.db, Some(user.id), "level.put", Some(slug), json!(r.requirements)).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_level(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>) -> AppResult<Json<Value>> {
    admin(&user)?;
    sqlx::query("DELETE FROM requirement_levels WHERE slug = $1").bind(&slug).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "level.delete", Some(slug), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

// ---------------------------------------------------------------------------
// Authoring: tracks, modules, scenarios, questions, assets
// ---------------------------------------------------------------------------

pub async fn list_tracks(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    author(&user)?;
    let tracks: Vec<catalog::Track> = sqlx::query_as("SELECT * FROM tracks ORDER BY position, title").fetch_all(&state.db).await?;
    let mut out = Vec::new();
    for t in tracks {
        let def = t.exam_def()?;
        let mut readiness = attempts::content_blockers(&state.db, &t, &def).await?;
        if let Some(r) = t.recert_def()? {
            readiness.extend(attempts::content_blockers(&state.db, &t, &r).await?.into_iter().map(|b| format!("recert:{b}")));
        }
        let (modules, scenarios, questions): (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM modules WHERE track_id = $1), (SELECT count(*) FROM scenarios WHERE track_id = $1),
                    (SELECT count(*) FROM questions WHERE track_id = $1 AND active)",
        )
        .bind(t.id)
        .fetch_one(&state.db)
        .await?;
        out.push(json!({
            "slug": t.slug, "title": t.title, "published": t.published, "audiences": t.audiences,
            "modules": modules, "scenarios": scenarios, "questions": questions, "readiness": readiness,
        }));
    }
    Ok(Json(json!(out)))
}

pub async fn get_track(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>) -> AppResult<Json<Value>> {
    author(&user)?;
    let t = catalog::load_track(&state.db, &slug).await?;
    let modules: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(jsonb_build_object('slug', slug, 'position', position, 'title', title, 'video', video_url,
            'captions', captions_url, 'duration_minutes', duration_minutes, 'doc_url', doc_url, 'body', body_md,
            'attachments', attachments) ORDER BY position) FROM modules WHERE track_id = $1",
    )
    .bind(t.id)
    .fetch_one(&state.db)
    .await?;
    let scenarios: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(jsonb_build_object('slug', s.slug, 'position', s.position, 'title', s.title, 'kind', s.kind,
            'exam_only', s.exam_only, 'family', s.family, 'context', s.context_md, 'pitfalls', s.pitfalls_md,
            'steps', COALESCE((SELECT jsonb_agg(jsonb_build_object('action', st.action_md, 'image_asset', st.image_asset,
                'alt', st.image_alt, 'expected', st.expected_md, 'annotations', st.annotations) ORDER BY st.position)
                FROM scenario_steps st WHERE st.scenario_id = s.id), '[]'::jsonb)) ORDER BY s.position, s.slug)
         FROM scenarios s WHERE s.track_id = $1",
    )
    .bind(t.id)
    .fetch_one(&state.db)
    .await?;
    let questions: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(jsonb_build_object('ref', q.ref, 'pool', q.pool, 'module', m.slug, 'scenario', s.slug,
            'format', q.format, 'prompt', q.prompt_md, 'explanation', q.explanation_md, 'choices', q.choices,
            'active', q.active, 'answered', st.answered, 'correct', st.correct) ORDER BY q.pool, q.ref)
         FROM questions q LEFT JOIN modules m ON m.id = q.module_id LEFT JOIN scenarios s ON s.id = q.scenario_id
         LEFT JOIN question_stats st ON st.question_id = q.id WHERE q.track_id = $1",
    )
    .bind(t.id)
    .fetch_one(&state.db)
    .await?;
    Ok(Json(json!({
        "slug": t.slug,
        "track": {
            "title": t.title, "summary": t.summary, "description": t.description_md, "audiences": t.audiences,
            "position": t.position, "prerequisite": t.prerequisite_slug, "prerequisites": t.prerequisites_md,
            "estimated_minutes": t.estimated_minutes, "scenarios_required": t.scenarios_required,
            "validity_months": t.validity_months, "module_quiz_pass_percent": t.module_quiz_pass_percent,
            "published": t.published, "badge": t.badge, "exam": t.exam, "recert_exam": t.recert_exam,
        },
        "modules": modules.unwrap_or(json!([])),
        "scenarios": scenarios.unwrap_or(json!([])),
        "questions": questions.unwrap_or(json!([])),
    })))
}

pub async fn put_track(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>, Json(tf): Json<TrackFile>) -> AppResult<Json<Value>> {
    author(&user)?;
    if !is_slug(&slug) {
        return Err(AppError::bad_request("invalid_slug", "lowercase letters, digits and dashes"));
    }
    tf.exam.validate().map_err(|e| bad("invalid_exam", e))?;
    if let Some(r) = &tf.recert_exam {
        r.validate().map_err(|e| bad("invalid_exam", e))?;
    }
    if tf.audiences.is_empty() || !tf.audiences.iter().all(|a| matches!(a.as_str(), "public" | "partner" | "customer")) {
        return Err(AppError::bad_request("invalid_audiences", "audiences: public, partner, customer"));
    }
    sqlx::query(
        "INSERT INTO tracks (id, slug, title, summary, description_md, audiences, position, prerequisite_slug, prerequisites_md,
            estimated_minutes, scenarios_required, validity_months, module_quiz_pass_percent, exam, recert_exam, badge, published, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17, now())
         ON CONFLICT (slug) DO UPDATE SET title=$3, summary=$4, description_md=$5, audiences=$6, position=$7, prerequisite_slug=$8,
            prerequisites_md=$9, estimated_minutes=$10, scenarios_required=$11, validity_months=$12, module_quiz_pass_percent=$13,
            exam=$14, recert_exam=$15, badge=$16, published=$17, updated_at=now()",
    )
    .bind(Uuid::new_v4())
    .bind(&slug)
    .bind(&tf.title)
    .bind(&tf.summary)
    .bind(&tf.description)
    .bind(&tf.audiences)
    .bind(tf.position)
    .bind(&tf.prerequisite)
    .bind(&tf.prerequisites)
    .bind(tf.estimated_minutes)
    .bind(tf.scenarios_required)
    .bind(tf.validity_months)
    .bind(tf.module_quiz_pass_percent)
    .bind(json!(tf.exam))
    .bind(tf.recert_exam.as_ref().map(|r| json!(r)))
    .bind(json!(tf.badge))
    .bind(tf.published)
    .execute(&state.db)
    .await?;
    audit::log(&state.db, Some(user.id), "content.track.put", Some(slug), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_track(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>) -> AppResult<Json<Value>> {
    admin(&user)?;
    let has_certs: bool = sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM certifications c JOIN tracks t ON t.id = c.track_id WHERE t.slug = $1)")
        .bind(&slug)
        .fetch_one(&state.db)
        .await?;
    if has_certs {
        return Err(AppError::conflict("track_has_certifications", "unpublish the track instead of deleting it"));
    }
    sqlx::query("DELETE FROM tracks WHERE slug = $1").bind(&slug).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "content.track.delete", Some(slug), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct ModuleReq {
    title: String,
    position: i32,
    #[serde(default)]
    video: Option<String>,
    #[serde(default)]
    captions: Option<String>,
    #[serde(default)]
    duration_minutes: i32,
    #[serde(default)]
    doc_url: Option<String>,
    #[serde(default)]
    body: String,
    #[serde(default)]
    attachments: Vec<Value>,
}

fn check_url(u: &Option<String>) -> AppResult<()> {
    match u {
        Some(u) if !(u.starts_with("https://") || u.starts_with("http://") || u.starts_with('/')) => {
            Err(AppError::bad_request("invalid_url", "URLs must be http(s) or site-relative"))
        }
        _ => Ok(()),
    }
}

pub async fn put_module(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, mslug)): Path<(String, String)>,
    Json(r): Json<ModuleReq>,
) -> AppResult<Json<Value>> {
    author(&user)?;
    if !is_slug(&mslug) || r.title.trim().is_empty() {
        return Err(AppError::bad_request("invalid_module", "slug and title required"));
    }
    check_url(&r.video)?;
    check_url(&r.captions)?;
    check_url(&r.doc_url)?;
    let track = catalog::load_track(&state.db, &slug).await?;
    let attachments: Vec<Value> = r
        .attachments
        .into_iter()
        .filter_map(|a| {
            let id: Uuid = a["asset_id"].as_str()?.parse().ok()?;
            Some(json!({ "asset_id": id, "label": a["label"].as_str().unwrap_or_default(), "name": a["name"].as_str().unwrap_or("file") }))
        })
        .collect();
    sqlx::query(
        "INSERT INTO modules (id, track_id, slug, position, title, video_url, captions_url, duration_minutes, body_md, attachments, doc_url, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now())
         ON CONFLICT (track_id, slug) DO UPDATE SET position=$4, title=$5, video_url=$6, captions_url=$7, duration_minutes=$8,
            body_md=$9, attachments=$10, doc_url=$11, updated_at=now()",
    )
    .bind(Uuid::new_v4())
    .bind(track.id)
    .bind(&mslug)
    .bind(r.position)
    .bind(r.title.trim())
    .bind(&r.video)
    .bind(&r.captions)
    .bind(r.duration_minutes)
    .bind(&r.body)
    .bind(Value::Array(attachments))
    .bind(&r.doc_url)
    .execute(&state.db)
    .await?;
    audit::log(&state.db, Some(user.id), "content.module.put", Some(format!("{slug}/{mslug}")), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_module(State(state): State<AppState>, user: CurrentUser, Path((slug, mslug)): Path<(String, String)>) -> AppResult<Json<Value>> {
    author(&user)?;
    let track = catalog::load_track(&state.db, &slug).await?;
    sqlx::query("DELETE FROM modules WHERE track_id = $1 AND slug = $2").bind(track.id).bind(&mslug).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "content.module.delete", Some(format!("{slug}/{mslug}")), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct StepReq {
    action: String,
    #[serde(default)]
    image_asset: Option<Uuid>,
    #[serde(default)]
    alt: String,
    #[serde(default)]
    expected: String,
    #[serde(default)]
    annotations: Vec<Annotation>,
}

#[derive(Deserialize)]
pub struct ScenarioReq {
    title: String,
    kind: String,
    #[serde(default)]
    exam_only: bool,
    #[serde(default)]
    family: Option<String>,
    #[serde(default)]
    position: i32,
    #[serde(default)]
    context: String,
    #[serde(default)]
    pitfalls: String,
    #[serde(default)]
    steps: Vec<StepReq>,
}

pub async fn put_scenario(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, sslug)): Path<(String, String)>,
    Json(r): Json<ScenarioReq>,
) -> AppResult<Json<Value>> {
    author(&user)?;
    if !is_slug(&sslug) || r.title.trim().is_empty() || !matches!(r.kind.as_str(), "implementation" | "diagnostic") {
        return Err(AppError::bad_request("invalid_scenario", "slug, title and kind (implementation|diagnostic) required"));
    }
    for (i, s) in r.steps.iter().enumerate() {
        for a in &s.annotations {
            a.validate().map_err(|e| bad("invalid_annotation", format!("step {}: {e}", i + 1)))?;
        }
    }
    let track = catalog::load_track(&state.db, &slug).await?;
    let mut tx = state.db.begin().await?;
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO scenarios (id, track_id, slug, position, title, kind, exam_only, context_md, pitfalls_md, family, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10, now())
         ON CONFLICT (track_id, slug) DO UPDATE SET position=$4, title=$5, kind=$6, exam_only=$7, context_md=$8,
            pitfalls_md=$9, family=$10, updated_at=now()
         RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(track.id)
    .bind(&sslug)
    .bind(r.position)
    .bind(r.title.trim())
    .bind(&r.kind)
    .bind(r.exam_only)
    .bind(&r.context)
    .bind(&r.pitfalls)
    .bind(&r.family)
    .fetch_one(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM scenario_steps WHERE scenario_id = $1").bind(id).execute(&mut *tx).await?;
    for (i, s) in r.steps.iter().enumerate() {
        sqlx::query(
            "INSERT INTO scenario_steps (scenario_id, position, action_md, image_asset, image_alt, annotations, expected_md)
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(id)
        .bind(i as i32 + 1)
        .bind(&s.action)
        .bind(s.image_asset)
        .bind(&s.alt)
        .bind(json!(s.annotations))
        .bind(&s.expected)
        .execute(&mut *tx)
        .await?;
    }
    audit::log(&mut *tx, Some(user.id), "content.scenario.put", Some(format!("{slug}/{sslug}")), json!({})).await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn delete_scenario(State(state): State<AppState>, user: CurrentUser, Path((slug, sslug)): Path<(String, String)>) -> AppResult<Json<Value>> {
    author(&user)?;
    let track = catalog::load_track(&state.db, &slug).await?;
    sqlx::query("DELETE FROM scenarios WHERE track_id = $1 AND slug = $2").bind(track.id).bind(&sslug).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "content.scenario.delete", Some(format!("{slug}/{sslug}")), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct QuestionReq {
    track: String,
    pool: String,
    #[serde(default)]
    module: Option<String>,
    #[serde(default)]
    scenario: Option<String>,
    #[serde(default = "default_format")]
    format: String,
    prompt: String,
    #[serde(default)]
    explanation: String,
    #[serde(default)]
    choices: Vec<ChoiceFile>,
    #[serde(default = "default_true")]
    active: bool,
}

fn default_format() -> String {
    "choice".into()
}
fn default_true() -> bool {
    true
}

pub async fn put_question(State(state): State<AppState>, user: CurrentUser, Path(reference): Path<String>, Json(r): Json<QuestionReq>) -> AppResult<Json<Value>> {
    author(&user)?;
    let track = catalog::load_track(&state.db, &r.track).await?;
    let module_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM modules WHERE track_id = $1").bind(track.id).fetch_all(&state.db).await?;
    let scenario_slugs: Vec<String> = sqlx::query_scalar("SELECT slug FROM scenarios WHERE track_id = $1").bind(track.id).fetch_all(&state.db).await?;
    let mut q = QuestionFile {
        reference: reference.clone(),
        pool: r.pool,
        module: r.module.filter(|s| !s.is_empty()),
        scenario: r.scenario.filter(|s| !s.is_empty()),
        format: r.format,
        prompt: r.prompt,
        explanation: r.explanation,
        choices: r.choices,
        active: r.active,
    };
    let ms: Vec<&str> = module_slugs.iter().map(String::as_str).collect();
    let ss: Vec<&str> = scenario_slugs.iter().map(String::as_str).collect();
    pack::validate_question(&mut q, &ms, &ss).map_err(|e| bad("invalid_question", e))?;
    let module_id: Option<Uuid> = match &q.module {
        Some(m) => sqlx::query_scalar("SELECT id FROM modules WHERE track_id = $1 AND slug = $2").bind(track.id).bind(m).fetch_optional(&state.db).await?,
        None => None,
    };
    let scenario_id: Option<Uuid> = match &q.scenario {
        Some(s) => sqlx::query_scalar("SELECT id FROM scenarios WHERE track_id = $1 AND slug = $2").bind(track.id).bind(s).fetch_optional(&state.db).await?,
        None => None,
    };
    let choices: Vec<Value> = q.choices.iter().map(|c| json!({ "id": c.id, "text": c.text, "correct": c.correct })).collect();
    sqlx::query(
        "INSERT INTO questions (id, ref, track_id, pool, module_id, scenario_id, format, prompt_md, choices, explanation_md, active, updated_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now())
         ON CONFLICT (ref) DO UPDATE SET track_id=$3, pool=$4, module_id=$5, scenario_id=$6, format=$7, prompt_md=$8,
            choices=$9, explanation_md=$10, active=$11, updated_at=now()",
    )
    .bind(Uuid::new_v4())
    .bind(&reference)
    .bind(track.id)
    .bind(&q.pool)
    .bind(module_id)
    .bind(scenario_id)
    .bind(&q.format)
    .bind(&q.prompt)
    .bind(Value::Array(choices))
    .bind(&q.explanation)
    .bind(q.active)
    .execute(&state.db)
    .await?;
    audit::log(&state.db, Some(user.id), "content.question.put", Some(reference), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn deactivate_question(State(state): State<AppState>, user: CurrentUser, Path(reference): Path<String>) -> AppResult<Json<Value>> {
    author(&user)?;
    sqlx::query("UPDATE questions SET active = FALSE, updated_at = now() WHERE ref = $1").bind(&reference).execute(&state.db).await?;
    audit::log(&state.db, Some(user.id), "content.question.deactivate", Some(reference), json!({})).await?;
    Ok(Json(json!({ "ok": true })))
}

pub async fn upload_asset(State(state): State<AppState>, user: CurrentUser, mut multipart: Multipart) -> AppResult<Json<Value>> {
    author(&user)?;
    let mut out = Vec::new();
    while let Some(field) = multipart.next_field().await.map_err(|e| bad("invalid_upload", e))? {
        let name = field.file_name().unwrap_or("upload").to_string();
        let bytes = field.bytes().await.map_err(|e| bad("invalid_upload", e))?;
        if pack::content_type_for(&name) == "application/octet-stream" {
            return Err(AppError::bad_request("unsupported_file", "unsupported file type"));
        }
        let mut tx = state.db.begin().await?;
        let id = pack::store_asset(&mut tx, &state.config.server.data_dir, &name, &bytes).await?;
        tx.commit().await?;
        out.push(json!({ "asset_id": id, "name": name, "url": format!("/api/assets/{id}") }));
    }
    audit::log(&state.db, Some(user.id), "content.asset.upload", None, json!({ "count": out.len() })).await?;
    Ok(Json(json!(out)))
}

#[derive(Deserialize)]
pub struct AnnounceReq {
    message: String,
}

/// E-mails every learner enrolled in a track about new or updated content.
pub async fn announce(State(state): State<AppState>, user: CurrentUser, Path(slug): Path<String>, Json(r): Json<AnnounceReq>) -> AppResult<Json<Value>> {
    author(&user)?;
    let track = catalog::load_track(&state.db, &slug).await?;
    let learners: Vec<(String, String)> = sqlx::query_as(
        "SELECT u.email, u.display_name FROM enrollments e JOIN users u ON u.id = e.user_id WHERE e.track_id = $1",
    )
    .bind(track.id)
    .fetch_all(&state.db)
    .await?;
    let link = state.config.public_url(&format!("/tracks/{slug}"));
    for (email, name) in &learners {
        mail::send_template(&state, email, "new_content", json!({ "name": name, "track": track.title, "message": r.message, "link": link })).await?;
    }
    audit::log(&state.db, Some(user.id), "content.announce", Some(slug), json!({ "recipients": learners.len() })).await?;
    Ok(Json(json!({ "recipients": learners.len() })))
}
