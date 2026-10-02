//! Instance configuration: one TOML file, overridable by environment variables.
//!
//! Any key can be overridden with `LECTERN__<SECTION>__<KEY>=value` (double underscores
//! separate path segments, case-insensitive). Values are parsed as TOML literals when
//! possible (`true`, `42`, `["a","b"]`) and fall back to plain strings. Secrets should
//! always come from the environment, never from the file.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub instance: InstanceConfig,
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub mail: MailConfig,
    #[serde(default)]
    pub certificates: CertificateConfig,
    #[serde(default)]
    pub alerts: AlertConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InstanceConfig {
    /// Name of the academy, e.g. "Acme Academy".
    pub name: String,
    /// Name of the product being taught, used in interface texts.
    #[serde(default)]
    pub product_name: String,
    /// Current major version of the product. Certifications record it; bumping it
    /// shortens the validity of older certifications (see `alerts.major_version_grace_days`).
    #[serde(default)]
    pub product_major_version: Option<String>,
    /// Public base URL, without trailing slash. Used in e-mails, badges and redirects.
    pub public_url: String,
    pub contact_email: String,
    #[serde(default)]
    pub legal_notice_url: Option<String>,
    #[serde(default)]
    pub privacy_policy_url: Option<String>,
    /// Documentation site, for deep links from modules.
    #[serde(default)]
    pub documentation_url: Option<String>,
    /// Logo file name inside `server.branding_dir` (SVG or PNG).
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub favicon: Option<String>,
    #[serde(default = "default_locale")]
    pub default_locale: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeConfig {
    /// Design tokens, emitted as CSS custom properties `--<name>`.
    /// Missing tokens fall back to the defaults in `theme.rs`.
    #[serde(default)]
    pub tokens: BTreeMap<String, String>,
    /// Font faces served from `server.branding_dir`.
    #[serde(default)]
    pub fonts: Vec<FontFace>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FontFace {
    pub family: String,
    /// File name inside the branding directory (woff2, woff, ttf).
    pub src: String,
    #[serde(default = "default_font_weight")]
    pub weight: String,
    #[serde(default = "default_font_style")]
    pub style: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    #[serde(default = "default_bind")]
    pub bind: String,
    /// Built front-end (index.html + assets). Optional in development.
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,
    /// Uploaded assets (screenshots, attachments).
    #[serde(default = "default_data_dir")]
    pub data_dir: PathBuf,
    /// Logo, favicon, fonts, certificate artwork.
    #[serde(default = "default_branding_dir")]
    pub branding_dir: PathBuf,
    /// The instance is served over HTTPS: enables HSTS and the `__Host-` session cookie
    /// prefix. Only disable for local development on http://localhost (session cookies
    /// are always `Secure`, which browsers accept on localhost).
    #[serde(default = "default_true")]
    pub secure_cookies: bool,
    /// Extra origins allowed to serve media (video CDN / object storage), for the CSP.
    #[serde(default)]
    pub media_origins: Vec<String>,
    /// Maximum upload size in megabytes.
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_mb: usize,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            bind: default_bind(),
            static_dir: default_static_dir(),
            data_dir: default_data_dir(),
            branding_dir: default_branding_dir(),
            secure_cookies: true,
            media_origins: Vec::new(),
            max_upload_mb: default_max_upload_mb(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseConfig {
    pub url: String,
    #[serde(default = "default_pool_size")]
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthConfig {
    #[serde(default = "default_session_hours")]
    pub session_hours: i64,
    /// Passwordless sign-in by e-mail link.
    #[serde(default = "default_true")]
    pub email_login: bool,
    #[serde(default)]
    pub oidc: Option<OidcConfig>,
    /// Platform roles that require a session authenticated with MFA.
    #[serde(default = "default_mfa_roles")]
    pub require_mfa_for: Vec<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            session_hours: default_session_hours(),
            email_login: true,
            oidc: None,
            require_mfa_for: default_mfa_roles(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    /// Label of the sign-in button, e.g. "Company account".
    #[serde(default = "default_oidc_label")]
    pub label: String,
    pub issuer_url: String,
    pub client_id: String,
    #[serde(default)]
    pub client_secret: Option<String>,
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,
    /// `acr` values that denote a multi-factor authentication.
    #[serde(default)]
    pub mfa_acr_values: Vec<String>,
    /// `amr` values that denote a multi-factor authentication.
    #[serde(default = "default_mfa_amr")]
    pub mfa_amr_values: Vec<String>,
    /// Optional ID-token claim holding platform roles (e.g. Keycloak client roles mapped to a flat claim).
    #[serde(default)]
    pub roles_claim: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MailConfig {
    /// Sender, e.g. "Acme Academy <academy@example.com>". Defaults to the contact address.
    #[serde(default)]
    pub from: Option<String>,
    /// `smtps://user:pass@host:465` or `smtp://host:587` (STARTTLS). When absent,
    /// e-mails are written to the log instead of being sent.
    #[serde(default)]
    pub smtp_url: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CertificateConfig {
    /// TrueType/OpenType font inside the branding directory. Defaults to the bundled font.
    #[serde(default)]
    pub font: Option<String>,
    #[serde(default)]
    pub font_bold: Option<String>,
    /// PNG or JPEG logo inside the branding directory.
    #[serde(default)]
    pub logo: Option<String>,
    #[serde(default)]
    pub signatory_name: Option<String>,
    #[serde(default)]
    pub signatory_title: Option<String>,
    /// PNG image of the signature inside the branding directory.
    #[serde(default)]
    pub signature_image: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AlertConfig {
    #[serde(default = "default_expiry_days")]
    pub expiry_days: Vec<i64>,
    /// Recertification opens this many days before expiry.
    #[serde(default = "default_recert_window")]
    pub recert_window_days: i64,
    /// When a new major product version is declared, older certifications expire
    /// at most this many days later.
    #[serde(default = "default_recert_window")]
    pub major_version_grace_days: i64,
}

impl Default for AlertConfig {
    fn default() -> Self {
        Self {
            expiry_days: default_expiry_days(),
            recert_window_days: default_recert_window(),
            major_version_grace_days: default_recert_window(),
        }
    }
}

fn default_locale() -> String {
    "fr".into()
}
fn default_font_weight() -> String {
    "400".into()
}
fn default_font_style() -> String {
    "normal".into()
}
fn default_bind() -> String {
    "0.0.0.0:8080".into()
}
fn default_static_dir() -> PathBuf {
    "web/dist".into()
}
fn default_data_dir() -> PathBuf {
    "data".into()
}
fn default_branding_dir() -> PathBuf {
    "branding".into()
}
fn default_true() -> bool {
    true
}
fn default_max_upload_mb() -> usize {
    50
}
fn default_pool_size() -> u32 {
    10
}
fn default_session_hours() -> i64 {
    12
}
fn default_mfa_roles() -> Vec<String> {
    vec!["trainer".into(), "admin".into()]
}
fn default_oidc_label() -> String {
    "SSO".into()
}
fn default_scopes() -> Vec<String> {
    vec!["openid".into(), "email".into(), "profile".into()]
}
fn default_mfa_amr() -> Vec<String> {
    ["mfa", "otp", "hwk", "swk", "webauthn"].into_iter().map(String::from).collect()
}
fn default_expiry_days() -> Vec<i64> {
    vec![90, 30, 7]
}
fn default_recert_window() -> i64 {
    90
}

impl Config {
    pub fn load(path: Option<&Path>) -> anyhow::Result<Self> {
        let mut table: toml::Table = match path {
            Some(p) => {
                let raw = std::fs::read_to_string(p)
                    .with_context(|| format!("reading configuration file {}", p.display()))?;
                raw.parse().with_context(|| format!("parsing {}", p.display()))?
            }
            None => toml::Table::new(),
        };
        apply_env_overrides(&mut table, std::env::vars())?;
        // DATABASE_URL is the de-facto standard and, like any environment value, wins over the file.
        if let Ok(url) = std::env::var("DATABASE_URL") {
            let db = table.entry("database").or_insert_with(|| toml::Value::Table(toml::Table::new()));
            if let Some(db) = db.as_table_mut() {
                db.insert("url".into(), toml::Value::String(url));
            }
        }
        let config: Config = toml::Value::Table(table).try_into().context("invalid configuration")?;
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> anyhow::Result<()> {
        if !self.instance.public_url.starts_with("http") {
            bail!("instance.public_url must be an absolute URL");
        }
        if self.instance.public_url.ends_with('/') {
            bail!("instance.public_url must not end with a slash");
        }
        if !self.auth.email_login && self.auth.oidc.is_none() {
            bail!("at least one sign-in method (auth.email_login or auth.oidc) must be enabled");
        }
        for name in self.theme.tokens.keys() {
            if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
                bail!("invalid theme token name {name:?}");
            }
        }
        for value in self.theme.tokens.values() {
            if value.contains(['{', '}', ';', '<']) {
                bail!("invalid theme token value {value:?}");
            }
        }
        Ok(())
    }

    pub fn public_url(&self, path: &str) -> String {
        format!("{}{}", self.instance.public_url, path)
    }

    pub fn mail_from(&self) -> String {
        self.mail.from.clone().unwrap_or_else(|| format!("{} <{}>", self.instance.name, self.instance.contact_email))
    }
}

fn apply_env_overrides(table: &mut toml::Table, vars: impl Iterator<Item = (String, String)>) -> anyhow::Result<()> {
    for (key, raw) in vars {
        let Some(path) = key.strip_prefix("LECTERN__") else { continue };
        let segments: Vec<String> = path.split("__").map(|s| s.to_ascii_lowercase()).collect();
        if segments.iter().any(String::is_empty) {
            bail!("invalid configuration variable {key}");
        }
        let value = parse_env_value(&raw);
        let (last, parents) = segments.split_last().expect("non-empty");
        let mut cursor = &mut *table;
        for seg in parents {
            let entry = cursor.entry(seg.clone()).or_insert_with(|| toml::Value::Table(toml::Table::new()));
            cursor = entry.as_table_mut().with_context(|| format!("{key}: {seg} is not a table"))?;
        }
        cursor.insert(last.clone(), value);
    }
    Ok(())
}

fn parse_env_value(raw: &str) -> toml::Value {
    let wrapped = format!("v = {raw}");
    match wrapped.parse::<toml::Table>() {
        Ok(mut t) => t.remove("v").unwrap_or_else(|| toml::Value::String(raw.into())),
        Err(_) => toml::Value::String(raw.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_overrides_nested_keys() {
        let mut t: toml::Table = "[auth]\nsession_hours = 12".parse().unwrap();
        apply_env_overrides(
            &mut t,
            vec![
                ("LECTERN__AUTH__SESSION_HOURS".into(), "4".into()),
                ("LECTERN__AUTH__OIDC__CLIENT_SECRET".into(), "s3cr#t value".into()),
                ("UNRELATED".into(), "x".into()),
            ]
            .into_iter(),
        )
        .unwrap();
        assert_eq!(t["auth"]["session_hours"].as_integer(), Some(4));
        assert_eq!(t["auth"]["oidc"]["client_secret"].as_str(), Some("s3cr#t value"));
    }

    #[test]
    fn parses_example_config() {
        let raw = include_str!("../../config/lectern.example.toml");
        let table: toml::Table = raw.parse().unwrap();
        let config: Config = toml::Value::Table(table).try_into().unwrap();
        config.validate().unwrap();
    }
}
