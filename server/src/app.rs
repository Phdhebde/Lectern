//! HTTP router and cross-cutting middleware (security headers, origin check, limits).

use std::sync::Arc;

use axum::Router;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use tower_http::compression::CompressionLayer;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::TraceLayer;

use crate::auth::{self, email, oidc};
use crate::routes::{admin, exams, learning, me, org, public};
use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let api = Router::new()
        // Instance and catalog
        .route("/instance", get(public::instance))
        .route("/i18n/{lang}", get(public::translations))
        .route("/catalog", get(public::catalog))
        // Authentication
        .route("/auth/email/request", post(email::request_link))
        .route("/auth/email/verify", post(email::verify_link))
        .route("/auth/logout", post(me::logout))
        // Account
        .route("/me", get(me::me).patch(me::update_profile).delete(me::delete_account))
        .route("/me/export", get(me::export))
        .route("/me/organization", post(me::join_org).delete(me::leave_org))
        .route("/me/certifications", get(me::certifications))
        .route("/me/attempts", get(me::attempts))
        .route("/certifications/{id}/certificate.pdf", get(me::certificate_pdf))
        // Learning
        .route("/tracks/{slug}", get(learning::track_detail))
        .route("/tracks/{slug}/enroll", post(learning::enroll))
        .route("/tracks/{slug}/rating", post(learning::rate))
        .route("/tracks/{slug}/modules/{module}", get(learning::module_detail))
        .route("/tracks/{slug}/scenarios/{scenario}", get(learning::scenario_detail))
        .route("/modules/{id}/progress", post(learning::module_progress))
        .route("/modules/{id}/quiz", post(learning::submit_module_quiz))
        .route("/scenarios/{id}/progress", post(learning::scenario_progress))
        .route("/scenarios/{id}/check", post(learning::scenario_check))
        .route("/assets/{id}", get(exams::asset))
        // Exams
        .route("/tracks/{slug}/exam", post(exams::start))
        .route("/attempts/{id}", get(exams::get_attempt))
        .route("/attempts/{id}/answers", put(exams::save_answer))
        .route("/attempts/{id}/submit", post(exams::submit_section))
        .route("/reviews", get(exams::pending_reviews))
        .route("/reviews/{id}", get(exams::review_detail).post(exams::submit_review))
        // Training managers
        .route("/organization", get(org::my_org))
        .route("/organization/join-code", post(org::rotate_join_code))
        .route("/organization/members/{id}/decision", post(org::decide_member))
        .route("/organization/members/{id}/role", post(org::set_member_role))
        .route("/organization/members/{id}", delete(org::remove_member))
        // Channel managers
        .route("/partners", get(org::list_orgs))
        .route("/partners/{id}", get(org::org_detail))
        .route("/partners/export.csv", get(org::export_csv))
        .route("/v1/certified", get(org::api_certified))
        // Administration
        .route("/admin/organizations", post(admin::create_org))
        .route("/admin/organizations/{id}", put(admin::update_org).delete(admin::delete_org))
        .route("/admin/organizations/{id}/managers", post(admin::add_manager))
        .route("/admin/users", get(admin::list_users))
        .route("/admin/users/{id}/roles", post(admin::set_role))
        .route("/admin/users/{id}/credits", post(admin::grant_credit))
        .route("/admin/certifications/{id}/revoke", post(admin::revoke_cert))
        .route("/admin/product-version", post(admin::declare_major_version))
        .route("/admin/api-tokens", get(admin::list_tokens).post(admin::create_token))
        .route("/admin/api-tokens/{id}", delete(admin::revoke_token))
        .route("/admin/stats", get(admin::stats))
        .route("/admin/audit", get(admin::audit_log))
        .route("/admin/levels", get(admin::list_levels))
        .route("/admin/levels/{slug}", put(admin::put_level).delete(admin::delete_level))
        .route("/admin/pack", get(admin::export_pack).post(admin::import_pack))
        .route("/admin/tracks", get(admin::list_tracks))
        .route("/admin/tracks/{slug}", get(admin::get_track).put(admin::put_track).delete(admin::delete_track))
        .route("/admin/tracks/{slug}/announce", post(admin::announce))
        .route("/admin/tracks/{slug}/modules/{module}", put(admin::put_module).delete(admin::delete_module))
        .route("/admin/tracks/{slug}/scenarios/{scenario}", put(admin::put_scenario).delete(admin::delete_scenario))
        .route("/admin/questions/{reference}", put(admin::put_question).delete(admin::deactivate_question))
        .route("/admin/assets", post(admin::upload_asset))
        .fallback(|| async { crate::error::AppError::NotFound })
        .layer(middleware::from_fn_with_state(state.clone(), no_store));

    let static_dir = state.config.server.static_dir.clone();
    let index = static_dir.join("index.html");
    let spa = ServeDir::new(&static_dir).not_found_service(ServeFile::new(index));

    Router::new()
        .nest("/api", api)
        .route("/auth/oidc/login", get(oidc::login))
        .route("/auth/oidc/callback", get(oidc::callback))
        .route("/theme.css", get(public::theme_css))
        .route("/server.css", get(public::server_css))
        .route("/verify/{id}", get(public::verify_page))
        .route("/verify/{id}/badge.svg", get(public::cert_badge_svg))
        .route("/verify/{id}/badge.png", get(public::cert_badge_png))
        .route("/ob/issuer", get(public::ob_issuer))
        .route("/ob/badges/{slug}", get(public::ob_badge))
        .route("/ob/badges/{slug}/image.svg", get(public::track_badge_svg))
        .route("/ob/badges/{slug}/image.png", get(public::track_badge_png))
        .route("/ob/assertions/{id}", get(public::ob_assertion))
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(readyz))
        .nest_service("/branding", ServeDir::new(&state.config.server.branding_dir))
        .fallback_service(spa)
        .layer(DefaultBodyLimit::max(state.config.server.max_upload_mb * 1024 * 1024))
        .layer(middleware::from_fn_with_state(state.clone(), security_headers))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

async fn readyz(State(state): State<AppState>) -> Response {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => "ready".into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

/// API responses carry personal data: never cache them.
async fn no_store(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    res.headers_mut().entry(header::CACHE_CONTROL).or_insert(HeaderValue::from_static("no-store"));
    res
}

pub fn content_security_policy(media_origins: &[String]) -> String {
    let media = media_origins.join(" ");
    format!(
        "default-src 'self'; script-src 'self'; style-src 'self'; img-src 'self' data: blob:; \
         font-src 'self'; media-src 'self' blob: {media}; connect-src 'self' {media}; \
         frame-ancestors 'none'; form-action 'self'; base-uri 'none'; object-src 'none'"
    )
}

async fn security_headers(State(state): State<AppState>, req: Request, next: Next) -> Response {
    if let Err(e) = auth::check_origin(req.headers(), req.method(), &state.config.instance.public_url) {
        return e.into_response();
    }
    let csp = Arc::new(content_security_policy(&state.config.server.media_origins));
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    let set = |h: &mut axum::http::HeaderMap, name: &'static str, value: &str| {
        if let Ok(v) = HeaderValue::from_str(value) {
            h.insert(HeaderName::from_static(name), v);
        }
    };
    set(h, "content-security-policy", &csp);
    set(h, "x-content-type-options", "nosniff");
    set(h, "x-frame-options", "DENY");
    set(h, "referrer-policy", "strict-origin-when-cross-origin");
    set(h, "permissions-policy", "camera=(), microphone=(), geolocation=(), payment=()");
    set(h, "cross-origin-opener-policy", "same-origin");
    if state.config.server.secure_cookies {
        set(h, "strict-transport-security", "max-age=31536000; includeSubDomains");
    }
    res
}
