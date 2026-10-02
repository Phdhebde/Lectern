//! OpenID Connect sign-in (authorization code + PKCE), e.g. against Keycloak.
//!
//! MFA is derived from the ID token: the session is marked `mfa` when the token's
//! `acr` is one of `mfa_acr_values` or its `amr` contains one of `mfa_amr_values`.
//! Platform roles can optionally be synchronised from a claim (`roles_claim`).

use axum::extract::{Query, State};
use axum::response::{IntoResponse, Redirect, Response};
use axum_extra::extract::CookieJar;
use base64::Engine;
use openidconnect::core::{CoreAuthenticationFlow, CoreClient, CoreProviderMetadata};
use openidconnect::{
    AccessTokenHash, AuthorizationCode, ClientId, ClientSecret, CsrfToken, EndpointMaybeSet, EndpointNotSet,
    EndpointSet, IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    TokenResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::RwLock;
use uuid::Uuid;

use super::{Role, create_session, email::safe_return_to};
use crate::audit;
use crate::config::OidcConfig;
use crate::error::{AppError, AppResult};
use crate::state::AppState;

type Client =
    CoreClient<EndpointSet, EndpointNotSet, EndpointNotSet, EndpointNotSet, EndpointMaybeSet, EndpointMaybeSet>;

pub struct OidcProvider {
    config: OidcConfig,
    redirect_url: String,
    http: openidconnect::reqwest::Client,
    client: RwLock<Option<Client>>,
}

impl OidcProvider {
    pub fn new(config: OidcConfig, public_url: &str) -> anyhow::Result<Self> {
        let http = openidconnect::reqwest::ClientBuilder::new()
            // Following redirects would expose the server to SSRF.
            .redirect(openidconnect::reqwest::redirect::Policy::none())
            .build()?;
        Ok(Self { config, redirect_url: format!("{public_url}/auth/oidc/callback"), http, client: RwLock::new(None) })
    }

    pub fn label(&self) -> &str {
        &self.config.label
    }

    /// Discovers the provider lazily so the server starts even if the IdP is down.
    async fn client(&self) -> anyhow::Result<Client> {
        if let Some(c) = self.client.read().await.as_ref() {
            return Ok(c.clone());
        }
        let metadata =
            CoreProviderMetadata::discover_async(IssuerUrl::new(self.config.issuer_url.clone())?, &self.http).await?;
        let client = CoreClient::from_provider_metadata(
            metadata,
            ClientId::new(self.config.client_id.clone()),
            self.config.client_secret.clone().map(ClientSecret::new),
        )
        .set_redirect_uri(RedirectUrl::new(self.redirect_url.clone())?);
        *self.client.write().await = Some(client.clone());
        Ok(client)
    }
}

#[derive(Deserialize)]
pub struct LoginQuery {
    return_to: Option<String>,
}

pub async fn login(State(state): State<AppState>, Query(q): Query<LoginQuery>) -> AppResult<Response> {
    let provider = state.oidc.as_ref().ok_or(AppError::NotFound)?;
    let client = provider.client().await?;
    let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
    let mut req =
        client.authorize_url(CoreAuthenticationFlow::AuthorizationCode, CsrfToken::new_random, Nonce::new_random);
    for scope in &provider.config.scopes {
        if scope != "openid" {
            req = req.add_scope(Scope::new(scope.clone()));
        }
    }
    let (url, csrf, nonce) = req.set_pkce_challenge(challenge).url();
    sqlx::query("DELETE FROM oidc_flows WHERE created_at < now() - interval '15 minutes'").execute(&state.db).await?;
    sqlx::query("INSERT INTO oidc_flows (state, pkce_verifier, nonce, return_to) VALUES ($1, $2, $3, $4)")
        .bind(csrf.secret())
        .bind(verifier.secret())
        .bind(nonce.secret())
        .bind(safe_return_to(q.return_to.as_deref()))
        .execute(&state.db)
        .await?;
    Ok(Redirect::to(url.as_str()).into_response())
}

#[derive(Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

#[derive(sqlx::FromRow)]
struct Flow {
    pkce_verifier: String,
    nonce: String,
    return_to: String,
}

pub async fn callback(
    State(state): State<AppState>,
    jar: CookieJar,
    Query(q): Query<CallbackQuery>,
) -> AppResult<Response> {
    let provider = state.oidc.as_ref().ok_or(AppError::NotFound)?;
    if let Some(err) = q.error {
        tracing::warn!(%err, "OIDC provider returned an error");
        return Ok(Redirect::to("/login?error=oidc").into_response());
    }
    let (Some(code), Some(flow_state)) = (q.code, q.state) else {
        return Err(AppError::bad_request("oidc_invalid_callback", "missing code or state"));
    };
    let flow: Flow = sqlx::query_as(
        "DELETE FROM oidc_flows WHERE state = $1 AND created_at > now() - interval '15 minutes'
         RETURNING pkce_verifier, nonce, return_to",
    )
    .bind(&flow_state)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| AppError::bad_request("oidc_invalid_state", "unknown or expired sign-in attempt"))?;

    let client = provider.client().await?;
    let token_response = client
        .exchange_code(AuthorizationCode::new(code))
        .map_err(|e| anyhow::anyhow!("OIDC provider has no token endpoint: {e}"))?
        .set_pkce_verifier(PkceCodeVerifier::new(flow.pkce_verifier))
        .request_async(&provider.http)
        .await
        .map_err(|e| anyhow::anyhow!("token exchange failed: {e}"))?;
    let id_token = token_response.id_token().ok_or_else(|| anyhow::anyhow!("no ID token returned"))?;
    let verifier = client.id_token_verifier();
    let claims =
        id_token.claims(&verifier, &Nonce::new(flow.nonce)).map_err(|e| anyhow::anyhow!("invalid ID token: {e}"))?;
    if let Some(expected) = claims.access_token_hash() {
        let actual = AccessTokenHash::from_token(
            token_response.access_token(),
            id_token.signing_alg().map_err(|e| anyhow::anyhow!("{e}"))?,
            id_token.signing_key(&verifier).map_err(|e| anyhow::anyhow!("{e}"))?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?;
        if actual != *expected {
            return Err(AppError::bad_request("oidc_invalid_token", "access token hash mismatch"));
        }
    }

    // The token is verified; read the raw payload for acr/amr/custom role claims.
    let payload = decode_jwt_payload(&id_token.to_string()).unwrap_or(Value::Null);
    let mfa = is_mfa(&payload, &provider.config);
    let issuer = claims.issuer().as_str().to_string();
    let subject = claims.subject().as_str().to_string();
    let email = claims
        .email()
        .map(|e| e.as_str().to_string())
        .ok_or_else(|| AppError::bad_request("oidc_no_email", "the identity provider did not return an e-mail"))?;
    if claims.email_verified() == Some(false) {
        return Err(AppError::bad_request("oidc_email_unverified", "e-mail not verified by the identity provider"));
    }
    let name = claims.name().and_then(|n| n.get(None)).map(|n| n.as_str().to_string()).unwrap_or_default();

    let user_id = link_identity(&state, &issuer, &subject, &email, &name).await?;
    if let Some(claim) = &provider.config.roles_claim {
        sync_roles(&state, user_id, &roles_from_claim(&payload, claim)).await?;
    }
    let cookie = create_session(&state, user_id, "oidc", mfa).await?;
    audit::log(&state.db, Some(user_id), "auth.login", None, json!({ "method": "oidc", "mfa": mfa })).await?;
    Ok((jar.add(cookie), Redirect::to(&flow.return_to)).into_response())
}

async fn link_identity(state: &AppState, issuer: &str, subject: &str, email: &str, name: &str) -> AppResult<Uuid> {
    let existing: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM user_identities WHERE issuer = $1 AND subject = $2")
            .bind(issuer)
            .bind(subject)
            .fetch_optional(&state.db)
            .await?;
    if let Some(id) = existing {
        return Ok(id);
    }
    // The IdP verified the e-mail: attach to the account using it, or create one.
    let id = super::upsert_user_by_email(&state.db, email, name).await?;
    sqlx::query("INSERT INTO user_identities (issuer, subject, user_id) VALUES ($1, $2, $3) ON CONFLICT DO NOTHING")
        .bind(issuer)
        .bind(subject)
        .bind(id)
        .execute(&state.db)
        .await?;
    Ok(id)
}

async fn sync_roles(state: &AppState, user_id: Uuid, roles: &[Role]) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    sqlx::query("DELETE FROM user_roles WHERE user_id = $1").bind(user_id).execute(&mut *tx).await?;
    for r in roles {
        sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1, $2) ON CONFLICT DO NOTHING")
            .bind(user_id)
            .bind(r.as_str())
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

fn decode_jwt_payload(jwt: &str) -> Option<Value> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub fn is_mfa(payload: &Value, config: &OidcConfig) -> bool {
    let acr_ok =
        payload.get("acr").and_then(Value::as_str).is_some_and(|acr| config.mfa_acr_values.iter().any(|v| v == acr));
    let amr_ok = payload
        .get("amr")
        .and_then(Value::as_array)
        .is_some_and(|amr| amr.iter().filter_map(Value::as_str).any(|m| config.mfa_amr_values.iter().any(|v| v == m)));
    acr_ok || amr_ok
}

/// Reads roles from a claim given as a dotted path (e.g. `resource_access.lectern.roles`).
pub fn roles_from_claim(payload: &Value, path: &str) -> Vec<Role> {
    let mut cursor = payload;
    for seg in path.split('.') {
        match cursor.get(seg) {
            Some(v) => cursor = v,
            None => return Vec::new(),
        }
    }
    cursor.as_array().map(|a| a.iter().filter_map(Value::as_str).filter_map(Role::parse).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> OidcConfig {
        OidcConfig {
            label: "SSO".into(),
            issuer_url: "https://idp.test".into(),
            client_id: "x".into(),
            client_secret: None,
            scopes: vec![],
            mfa_acr_values: vec!["gold".into()],
            mfa_amr_values: vec!["otp".into()],
            roles_claim: None,
        }
    }

    #[test]
    fn mfa_detection() {
        assert!(is_mfa(&json!({"acr": "gold"}), &cfg()));
        assert!(is_mfa(&json!({"amr": ["pwd", "otp"]}), &cfg()));
        assert!(!is_mfa(&json!({"acr": "1", "amr": ["pwd"]}), &cfg()));
        assert!(!is_mfa(&Value::Null, &cfg()));
    }

    #[test]
    fn roles_claim_path() {
        let p = json!({"resource_access": {"lectern": {"roles": ["admin", "unknown", "trainer"]}}});
        assert_eq!(roles_from_claim(&p, "resource_access.lectern.roles"), vec![Role::Admin, Role::Trainer]);
        assert!(roles_from_claim(&p, "missing").is_empty());
    }

    #[test]
    fn decodes_payload() {
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(br#"{"acr":"gold"}"#);
        let jwt = format!("h.{payload}.s");
        assert_eq!(decode_jwt_payload(&jwt), Some(json!({"acr": "gold"})));
    }
}
