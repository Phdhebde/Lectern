//! Sessions, CSRF protection and the `CurrentUser` extractor.
//!
//! Sessions are opaque random tokens kept in an `HttpOnly`, `SameSite=Lax` cookie; only
//! their SHA-256 hash is stored. Every state-changing request must carry the session's
//! CSRF token in the `X-CSRF-Token` header (synchronizer token pattern), and requests
//! whose `Origin` does not match the instance are rejected outright.

pub mod email;
pub mod oidc;

use axum::extract::{FromRef, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use axum_extra::extract::CookieJar;
use axum_extra::extract::cookie::{Cookie, SameSite};
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::state::AppState;

pub const SESSION_COOKIE: &str = "lectern_session";
pub const CSRF_HEADER: &str = "x-csrf-token";

/// 256-bit random token, URL-safe base64.
pub fn random_token() -> String {
    let bytes: [u8; 32] = rand::random();
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

pub fn hash_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    ChannelManager,
    Trainer,
    Admin,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::ChannelManager => "channel_manager",
            Role::Trainer => "trainer",
            Role::Admin => "admin",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "channel_manager" => Some(Role::ChannelManager),
            "trainer" => Some(Role::Trainer),
            "admin" => Some(Role::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Membership {
    pub org_id: Uuid,
    pub org_name: String,
    pub org_kind: String,
    pub status: String,
    pub org_role: String,
}

impl Membership {
    pub fn approved(&self) -> bool {
        self.status == "approved"
    }
}

/// The authenticated user of the current request.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
    pub public_profile: bool,
    pub roles: Vec<Role>,
    pub membership: Option<Membership>,
    pub csrf_token: String,
    pub mfa: bool,
    pub session_hash: Vec<u8>,
    require_mfa_for: Vec<String>,
}

impl CurrentUser {
    pub fn has_role(&self, role: Role) -> bool {
        self.roles.contains(&role)
    }

    /// Requires a platform role. Roles listed in `auth.require_mfa_for` additionally
    /// require a session that was authenticated with MFA.
    pub fn require(&self, role: Role) -> AppResult<()> {
        if !self.has_role(role) {
            return Err(AppError::Forbidden("forbidden"));
        }
        if self.require_mfa_for.iter().any(|r| r == role.as_str()) && !self.mfa {
            return Err(AppError::Forbidden("mfa_required"));
        }
        Ok(())
    }

    /// Requires one of several roles (the first one held is checked for MFA).
    pub fn require_any(&self, roles: &[Role]) -> AppResult<Role> {
        let held: Vec<Role> = roles.iter().copied().filter(|r| self.has_role(*r)).collect();
        if held.is_empty() {
            return Err(AppError::Forbidden("forbidden"));
        }
        // Accept if any held role is usable with the current session.
        for r in &held {
            if self.require(*r).is_ok() {
                return Ok(*r);
            }
        }
        Err(AppError::Forbidden("mfa_required"))
    }

    /// Organization the user manages, if they are an approved training manager.
    pub fn managed_org(&self) -> Option<Uuid> {
        self.membership.as_ref().filter(|m| m.approved() && m.org_role == "training_manager").map(|m| m.org_id)
    }

    /// Kind of organization the user is an approved member of.
    pub fn org_kind(&self) -> Option<&str> {
        self.membership.as_ref().filter(|m| m.approved()).map(|m| m.org_kind.as_str())
    }
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    user_id: Uuid,
    csrf_token: String,
    mfa: bool,
    email: String,
    display_name: String,
    public_profile: bool,
}

pub async fn load_user(db: &PgPool, session_hash: &[u8], require_mfa_for: &[String]) -> AppResult<Option<CurrentUser>> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT s.user_id, s.csrf_token, s.mfa, u.email, u.display_name, u.public_profile
         FROM sessions s JOIN users u ON u.id = s.user_id
         WHERE s.token_hash = $1 AND s.expires_at > now()",
    )
    .bind(session_hash)
    .fetch_optional(db)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let roles: Vec<String> =
        sqlx::query_scalar("SELECT role FROM user_roles WHERE user_id = $1").bind(row.user_id).fetch_all(db).await?;
    let membership = load_membership(db, row.user_id).await?;
    Ok(Some(CurrentUser {
        id: row.user_id,
        email: row.email,
        display_name: row.display_name,
        public_profile: row.public_profile,
        roles: roles.iter().filter_map(|r| Role::parse(r)).collect(),
        membership,
        csrf_token: row.csrf_token,
        mfa: row.mfa,
        session_hash: session_hash.to_vec(),
        require_mfa_for: require_mfa_for.to_vec(),
    }))
}

pub async fn load_membership(db: &PgPool, user_id: Uuid) -> AppResult<Option<Membership>> {
    Ok(sqlx::query_as(
        "SELECT m.org_id, o.name AS org_name, o.kind AS org_kind, m.status, m.org_role
         FROM memberships m JOIN organizations o ON o.id = m.org_id WHERE m.user_id = $1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?)
}

fn is_safe_method(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

/// Rejects cross-origin state-changing requests. Browsers always send `Origin` on
/// cross-origin POST/PUT/PATCH/DELETE; a mismatch means a forged request.
pub fn check_origin(headers: &HeaderMap, method: &Method, public_url: &str) -> AppResult<()> {
    if is_safe_method(method) {
        return Ok(());
    }
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok())
        && origin != "null"
        && !origin.eq_ignore_ascii_case(origin_of(public_url))
    {
        return Err(AppError::Forbidden("bad_origin"));
    }
    Ok(())
}

fn origin_of(url: &str) -> &str {
    // public_url is validated as scheme://host[:port][/path]; keep scheme://host[:port].
    let after_scheme = url.find("://").map(|i| i + 3).unwrap_or(0);
    match url[after_scheme..].find('/') {
        Some(i) => &url[..after_scheme + i],
        None => url,
    }
}

impl<S> FromRequestParts<S> for CurrentUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state = AppState::from_ref(state);
        if let Some(user) = parts.extensions.get::<CurrentUser>() {
            return Ok(user.clone());
        }
        let jar = CookieJar::from_headers(&parts.headers);
        let token = jar.get(SESSION_COOKIE).map(|c| c.value().to_string()).ok_or(AppError::Unauthorized)?;
        let user = load_user(&state.db, &hash_token(&token), &state.config.auth.require_mfa_for)
            .await?
            .ok_or(AppError::Unauthorized)?;
        if !is_safe_method(&parts.method) {
            let sent = parts.headers.get(CSRF_HEADER).and_then(|v| v.to_str().ok()).unwrap_or("");
            if !constant_time_eq(sent.as_bytes(), user.csrf_token.as_bytes()) {
                return Err(AppError::Forbidden("csrf"));
            }
        }
        parts.extensions.insert(user.clone());
        Ok(user)
    }
}

/// Optional authentication: `None` for anonymous visitors.
pub struct MaybeUser(pub Option<CurrentUser>);

impl<S> FromRequestParts<S> for MaybeUser
where
    AppState: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        match CurrentUser::from_request_parts(parts, state).await {
            Ok(u) => Ok(MaybeUser(Some(u))),
            Err(AppError::Unauthorized) => Ok(MaybeUser(None)),
            Err(e) => Err(e),
        }
    }
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Creates a session and returns the cookie to set.
pub async fn create_session(state: &AppState, user_id: Uuid, method: &str, mfa: bool) -> AppResult<Cookie<'static>> {
    let token = random_token();
    let expires: DateTime<Utc> = Utc::now() + Duration::hours(state.config.auth.session_hours);
    sqlx::query(
        "INSERT INTO sessions (token_hash, user_id, csrf_token, mfa, method, expires_at)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(hash_token(&token))
    .bind(user_id)
    .bind(random_token())
    .bind(mfa)
    .bind(method)
    .bind(expires)
    .execute(&state.db)
    .await?;
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1").bind(user_id).execute(&state.db).await?;
    Ok(session_cookie(state, token, state.config.auth.session_hours))
}

pub fn session_cookie(state: &AppState, value: String, hours: i64) -> Cookie<'static> {
    Cookie::build((SESSION_COOKIE, value))
        .http_only(true)
        .secure(state.config.server.secure_cookies)
        .same_site(SameSite::Lax)
        .path("/")
        .max_age(time::Duration::hours(hours))
        .build()
}

pub fn clear_session_cookie(state: &AppState) -> Cookie<'static> {
    session_cookie(state, String::new(), 0)
}

/// Finds or creates a user by e-mail (case-insensitive).
pub async fn upsert_user_by_email(db: &PgPool, email: &str, display_name: &str) -> AppResult<Uuid> {
    let existing: Option<Uuid> = sqlx::query_scalar("SELECT id FROM users WHERE lower(email) = lower($1)")
        .bind(email)
        .fetch_optional(db)
        .await?;
    if let Some(id) = existing {
        // Accounts pre-created by an administrator carry a placeholder name (the
        // e-mail local part) until their owner gives one at first sign-in.
        if !display_name.trim().is_empty() {
            sqlx::query(
                "UPDATE users SET display_name = $2 WHERE id = $1 AND display_name = split_part(email, '@', 1)",
            )
            .bind(id)
            .bind(display_name.trim())
            .execute(db)
            .await?;
        }
        return Ok(id);
    }
    let name = if display_name.trim().is_empty() {
        email.split('@').next().unwrap_or(email).to_string()
    } else {
        display_name.trim().to_string()
    };
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO users (id, email, display_name) VALUES ($1, $2, $3)
         ON CONFLICT ((lower(email))) DO UPDATE SET email = users.email RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(email.trim())
    .bind(name)
    .fetch_one(db)
    .await?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origin_extraction() {
        assert_eq!(origin_of("https://academy.example.com"), "https://academy.example.com");
        assert_eq!(origin_of("https://example.com/academy"), "https://example.com");
        assert_eq!(origin_of("http://localhost:8080"), "http://localhost:8080");
    }

    #[test]
    fn origin_check() {
        let mut h = HeaderMap::new();
        h.insert("origin", "https://evil.test".parse().unwrap());
        assert!(check_origin(&h, &Method::POST, "https://academy.example.com").is_err());
        assert!(check_origin(&h, &Method::GET, "https://academy.example.com").is_ok());
        h.insert("origin", "https://academy.example.com".parse().unwrap());
        assert!(check_origin(&h, &Method::POST, "https://academy.example.com").is_ok());
        assert!(check_origin(&HeaderMap::new(), &Method::POST, "https://academy.example.com").is_ok());
    }

    #[test]
    fn tokens_are_unique_and_hashed() {
        let a = random_token();
        assert_ne!(a, random_token());
        assert_eq!(a.len(), 43);
        assert_eq!(hash_token(&a).len(), 32);
        assert!(constant_time_eq(b"abc", b"abc"));
        assert!(!constant_time_eq(b"abc", b"abd"));
    }
}
