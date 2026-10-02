//! Passwordless sign-in: a single-use link sent by e-mail.
//!
//! The link opens a front-end page that POSTs the token, so mail scanners that
//! prefetch links (GET) cannot consume it.

use axum::Json;
use axum::extract::State;
use axum_extra::extract::CookieJar;
use chrono::{Duration, Utc};
use serde::Deserialize;
use serde_json::{Value, json};

use super::{create_session, hash_token, random_token, upsert_user_by_email};
use crate::audit;
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::state::AppState;

const TOKEN_MINUTES: i64 = 15;
const MAX_REQUESTS_PER_HOUR: i64 = 5;
/// Instance-wide cap, so the sign-in form cannot be used to flood many inboxes.
const MAX_REQUESTS_PER_MINUTE_GLOBAL: i64 = 60;

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub return_to: Option<String>,
}

pub fn is_plausible_email(email: &str) -> bool {
    let email = email.trim();
    let Some((local, domain)) = email.split_once('@') else { return false };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && email.len() <= 254
        && !email.chars().any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
}

/// Only same-site relative paths are accepted as post-login destinations.
pub fn safe_return_to(value: Option<&str>) -> String {
    match value {
        Some(p) if p.starts_with('/') && !p.starts_with("//") && !p.contains('\\') => p.to_string(),
        _ => "/".to_string(),
    }
}

pub async fn request_link(State(state): State<AppState>, Json(req): Json<LoginRequest>) -> AppResult<Json<Value>> {
    if !state.config.auth.email_login {
        return Err(AppError::Forbidden("email_login_disabled"));
    }
    let email = req.email.trim().to_string();
    if !is_plausible_email(&email) {
        return Err(AppError::bad_request("invalid_email", "invalid e-mail address"));
    }
    let (recent, global): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE lower(email) = lower($1) AND created_at > now() - interval '1 hour'),
                count(*) FILTER (WHERE created_at > now() - interval '1 minute')
         FROM login_tokens WHERE created_at > now() - interval '1 hour'",
    )
    .bind(&email)
    .fetch_one(&state.db)
    .await?;
    if global >= MAX_REQUESTS_PER_MINUTE_GLOBAL {
        tracing::warn!("sign-in link requests throttled instance-wide");
    }
    // Same response whether or not the limit is hit, to avoid account enumeration;
    // the limit protects inboxes from being flooded.
    if recent < MAX_REQUESTS_PER_HOUR && global < MAX_REQUESTS_PER_MINUTE_GLOBAL {
        let token = random_token();
        sqlx::query(
            "INSERT INTO login_tokens (token_hash, email, display_name, return_to, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(hash_token(&token))
        .bind(&email)
        .bind(super::clean_name(&req.display_name))
        .bind(safe_return_to(req.return_to.as_deref()))
        .bind(Utc::now() + Duration::minutes(TOKEN_MINUTES))
        .execute(&state.db)
        .await?;
        let link = state.config.public_url(&format!("/login/email#token={token}"));
        mail::send_template(&state, &email, "login", json!({ "link": link, "minutes": TOKEN_MINUTES })).await?;
    }
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct VerifyRequest {
    pub token: String,
}

#[derive(sqlx::FromRow)]
struct TokenRow {
    email: String,
    display_name: String,
    return_to: String,
}

pub async fn verify_link(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(req): Json<VerifyRequest>,
) -> AppResult<(CookieJar, Json<Value>)> {
    if !state.config.auth.email_login {
        return Err(AppError::Forbidden("email_login_disabled"));
    }
    let row: Option<TokenRow> = sqlx::query_as(
        "UPDATE login_tokens SET used_at = now()
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > now()
         RETURNING email, display_name, return_to",
    )
    .bind(hash_token(req.token.trim()))
    .fetch_optional(&state.db)
    .await?;
    let row = row.ok_or_else(|| AppError::bad_request("invalid_token", "link invalid or expired"))?;
    let user_id = upsert_user_by_email(&state.db, &row.email, &row.display_name).await?;
    let cookie = create_session(&state, user_id, "email", false).await?;
    audit::log(&state.db, Some(user_id), "auth.login", None, json!({ "method": "email" })).await?;
    Ok((jar.add(cookie), Json(json!({ "ok": true, "return_to": row.return_to }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_plausibility() {
        assert!(is_plausible_email("jane.doe@example.com"));
        assert!(!is_plausible_email("jane"));
        assert!(!is_plausible_email("jane@localhost"));
        assert!(!is_plausible_email("a b@example.com"));
        assert!(!is_plausible_email("<a@example.com>"));
    }

    #[test]
    fn return_to_is_relative_only() {
        assert_eq!(safe_return_to(Some("/tracks/x")), "/tracks/x");
        assert_eq!(safe_return_to(Some("//evil.test")), "/");
        assert_eq!(safe_return_to(Some("https://evil.test")), "/");
        assert_eq!(safe_return_to(Some("/\\evil.test")), "/");
        assert_eq!(safe_return_to(None), "/");
    }
}
