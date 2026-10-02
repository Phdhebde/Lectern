//! Unauthenticated endpoints: instance configuration, theme, catalog, verification
//! pages and Open Badges documents.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{Html, IntoResponse, Response};
use chrono::{Datelike, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::MaybeUser;
use crate::credentials::{self, BadgeDesign, BadgeInput};
use crate::domain::catalog::{self, Track};
use crate::domain::certs;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub async fn instance(State(state): State<AppState>) -> Json<Value> {
    let c = &state.config.instance;
    let branding = |f: &Option<String>| f.as_ref().map(|f| format!("/branding/{f}"));
    Json(json!({
        "name": c.name,
        "product_name": c.product_name,
        "product_major_version": c.product_major_version,
        "public_url": c.public_url,
        "contact_email": c.contact_email,
        "legal_notice_url": c.legal_notice_url,
        "privacy_policy_url": c.privacy_policy_url,
        "documentation_url": c.documentation_url,
        "logo": branding(&c.logo),
        "favicon": branding(&c.favicon),
        "locale": c.default_locale,
        "auth": {
            "email": state.config.auth.email_login,
            "oidc": state.oidc.as_ref().map(|o| json!({ "label": o.label(), "url": "/auth/oidc/login" })),
        },
        "theme": state.theme.tokens,
    }))
}

/// Instance overrides of the front-end translations (`<branding>/locales/<lang>.json`).
pub async fn translations(State(state): State<AppState>, Path(lang): Path<String>) -> AppResult<Response> {
    if !lang.chars().all(|c| c.is_ascii_alphabetic() || c == '-') || lang.len() > 10 {
        return Err(AppError::NotFound);
    }
    let path = state.config.server.branding_dir.join("locales").join(format!("{lang}.json"));
    let body = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(_) => "{}".to_string(),
    };
    Ok(([(header::CONTENT_TYPE, "application/json")], body).into_response())
}

pub async fn theme_css(State(state): State<AppState>) -> Response {
    css(state.theme.css.clone())
}

pub async fn server_css(State(state): State<AppState>) -> AppResult<Response> {
    Ok(css(state.renderer.template("server.css", ())?))
}

fn css(body: String) -> Response {
    (
        [(header::CONTENT_TYPE, HeaderValue::from_static("text/css; charset=utf-8")), (header::CACHE_CONTROL, HeaderValue::from_static("public, max-age=300"))],
        body,
    )
        .into_response()
}

#[derive(Serialize)]
pub struct CatalogEntry {
    slug: String,
    title: String,
    summary: String,
    audiences: Vec<String>,
    estimated_minutes: i32,
    prerequisite: Option<String>,
    certifying: bool,
    module_count: i64,
    enrolled: bool,
    completed_modules: i64,
    certification: Option<Value>,
}

pub async fn catalog(State(state): State<AppState>, MaybeUser(user): MaybeUser) -> AppResult<Json<Vec<CatalogEntry>>> {
    let tracks: Vec<Track> = sqlx::query_as("SELECT * FROM tracks ORDER BY position, title").fetch_all(&state.db).await?;
    let user_id = user.as_ref().map(|u| u.id);
    let mut out = Vec::new();
    let user_certs = match user_id {
        Some(id) => certs::for_user(&state.db, id).await?,
        None => Vec::new(),
    };
    let now = Utc::now();
    for t in tracks.into_iter().filter(|t| catalog::can_access(t, user.as_ref())) {
        let (module_count, completed, enrolled): (i64, i64, bool) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM modules WHERE track_id = $1),
                    (SELECT count(*) FROM module_progress p JOIN modules m ON m.id = p.module_id
                        WHERE m.track_id = $1 AND p.user_id = $2 AND p.completed_at IS NOT NULL),
                    EXISTS (SELECT 1 FROM enrollments WHERE track_id = $1 AND user_id = $2)",
        )
        .bind(t.id)
        .bind(user_id)
        .fetch_one(&state.db)
        .await?;
        let cert = user_certs
            .iter()
            .find(|c| c.track_id == t.id && !c.superseded)
            .map(|c| json!({ "id": c.id, "status": c.status(now), "expires_at": c.expires_at }));
        out.push(CatalogEntry {
            certifying: t.validity_months.is_some(),
            slug: t.slug,
            title: t.title,
            summary: t.summary,
            audiences: t.audiences,
            estimated_minutes: t.estimated_minutes,
            prerequisite: t.prerequisite_slug,
            module_count,
            enrolled,
            completed_modules: completed,
            certification: cert,
        });
    }
    Ok(Json(out))
}

// ---------------------------------------------------------------------------
// Verification and badges
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct PublicCert {
    id: Uuid,
    holder: String,
    email: String,
    public_profile: bool,
    track_slug: String,
    track_title: String,
    badge: Value,
    issued_at: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
    revoked_at: Option<chrono::DateTime<Utc>>,
    superseded: bool,
}

async fn public_cert(state: &AppState, id: Uuid) -> AppResult<Option<PublicCert>> {
    Ok(sqlx::query_as(
        "SELECT c.id, u.display_name AS holder, u.email, u.public_profile, t.slug AS track_slug, t.title AS track_title,
            t.badge, c.issued_at, c.expires_at, c.revoked_at,
            (c.superseded_by IS NOT NULL) AS superseded
         FROM certifications c JOIN users u ON u.id = c.user_id JOIN tracks t ON t.id = c.track_id
         WHERE c.id = $1",
    )
    .bind(id)
    .fetch_optional(&state.db)
    .await?)
}

fn cert_status(c: &PublicCert) -> &'static str {
    if c.revoked_at.is_some() {
        "revoked"
    } else if c.superseded {
        "superseded"
    } else if c.expires_at.is_some_and(|e| e <= Utc::now()) {
        "expired"
    } else {
        "valid"
    }
}

pub async fn verify_page(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let cert = match id.parse::<Uuid>() {
        Ok(id) => public_cert(&state, id).await?,
        Err(_) => None,
    };
    let fmt = state.renderer.raw("date.format").to_string();
    let ctx = cert.as_ref().map(|c| {
        json!({
            "id": c.id,
            "public": c.public_profile,
            "holder": c.holder,
            "track": c.track_title,
            "issued": c.issued_at.format(&fmt).to_string(),
            "expires": c.expires_at.map(|e| e.format(&fmt).to_string()),
            "status": cert_status(c),
        })
    });
    let html = state.renderer.template("verify.html", json!({ "cert": ctx }))?;
    let status = if cert.is_some() { StatusCode::OK } else { StatusCode::NOT_FOUND };
    Ok((status, Html(html)).into_response())
}

async fn badge_svg_for(state: &AppState, title: &str, badge: &Value, year: Option<i32>) -> AppResult<String> {
    let design: BadgeDesign = serde_json::from_value(badge.clone()).unwrap_or_default();
    Ok(credentials::badge_svg(state, &BadgeInput { track_title: title, design: &design, year })?)
}

fn svg_response(svg: String) -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "public, max-age=3600")], svg).into_response()
}

fn png_response(png: Vec<u8>) -> Response {
    ([(header::CONTENT_TYPE, "image/png"), (header::CACHE_CONTROL, "public, max-age=3600")], png).into_response()
}

async fn visible_cert(state: &AppState, id: &str) -> AppResult<PublicCert> {
    let id: Uuid = id.parse().map_err(|_| AppError::NotFound)?;
    let cert = public_cert(state, id).await?.ok_or(AppError::NotFound)?;
    if !cert.public_profile {
        return Err(AppError::NotFound);
    }
    Ok(cert)
}

pub async fn cert_badge_svg(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let c = visible_cert(&state, &id).await?;
    Ok(svg_response(badge_svg_for(&state, &c.track_title, &c.badge, Some(c.issued_at.year())).await?))
}

pub async fn cert_badge_png(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let c = visible_cert(&state, &id).await?;
    let svg = badge_svg_for(&state, &c.track_title, &c.badge, Some(c.issued_at.year())).await?;
    let png = tokio::task::spawn_blocking(move || credentials::svg_to_png(&svg, 600))
        .await
        .map_err(anyhow::Error::from)??;
    Ok(png_response(png))
}

pub async fn track_badge_svg(State(state): State<AppState>, Path(slug): Path<String>) -> AppResult<Response> {
    let t = catalog::load_track(&state.db, &slug).await?;
    if !t.published {
        return Err(AppError::NotFound);
    }
    Ok(svg_response(badge_svg_for(&state, &t.title, &t.badge, None).await?))
}

pub async fn track_badge_png(State(state): State<AppState>, Path(slug): Path<String>) -> AppResult<Response> {
    let t = catalog::load_track(&state.db, &slug).await?;
    if !t.published {
        return Err(AppError::NotFound);
    }
    let svg = badge_svg_for(&state, &t.title, &t.badge, None).await?;
    let png = tokio::task::spawn_blocking(move || credentials::svg_to_png(&svg, 600))
        .await
        .map_err(anyhow::Error::from)??;
    Ok(png_response(png))
}

fn ld_json(v: Value) -> Response {
    ([(header::CONTENT_TYPE, "application/ld+json")], v.to_string()).into_response()
}

pub async fn ob_issuer(State(state): State<AppState>) -> Response {
    ld_json(credentials::ob_issuer(&state))
}

pub async fn ob_badge(State(state): State<AppState>, Path(slug): Path<String>) -> AppResult<Response> {
    let t = catalog::load_track(&state.db, &slug).await?;
    if !t.published {
        return Err(AppError::NotFound);
    }
    Ok(ld_json(credentials::ob_badge_class(&state, &t.slug, &t.title, &t.summary)))
}

pub async fn ob_assertion(State(state): State<AppState>, Path(id): Path<String>) -> AppResult<Response> {
    let c = visible_cert(&state, &id).await?;
    let status = cert_status(&c);
    Ok(ld_json(credentials::ob_assertion(
        &state,
        c.id,
        &c.track_slug,
        &c.email,
        c.issued_at,
        c.expires_at,
        status == "revoked",
    )))
}
