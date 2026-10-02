//! Lectern: white-label academy server.

pub mod app;
pub mod audit;
pub mod auth;
pub mod config;
pub mod credentials;
pub mod domain;
pub mod error;
pub mod jobs;
pub mod mail;
pub mod pack;
pub mod render;
pub mod routes;
pub mod state;
pub mod theme;

use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

/// SQL assembled from compile-time constant fragments only (never user input);
/// values are always bound as parameters.
#[macro_export]
macro_rules! const_sql {
    ($($t:tt)*) => { sqlx::AssertSqlSafe(format!($($t)*)) };
}

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// Builds the shared application state (database pool, theme, templates, OIDC client).
pub async fn build_state(config: config::Config) -> anyhow::Result<state::AppState> {
    let db = PgPoolOptions::new()
        .max_connections(config.database.max_connections)
        .connect(&config.database.url)
        .await?;
    state_with_pool(config, db)
}

pub fn state_with_pool(config: config::Config, db: sqlx::PgPool) -> anyhow::Result<state::AppState> {
    let theme = theme::Theme::from_config(&config);
    let renderer = render::Renderer::new(&config, &theme)?;
    let oidc = match &config.auth.oidc {
        Some(o) => Some(Arc::new(auth::oidc::OidcProvider::new(o.clone(), &config.instance.public_url)?)),
        None => None,
    };
    Ok(state::AppState { db, config: Arc::new(config), theme: Arc::new(theme), renderer: Arc::new(renderer), oidc })
}
