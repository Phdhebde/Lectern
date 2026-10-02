use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::oidc::OidcProvider;
use crate::config::Config;
use crate::render::Renderer;
use crate::theme::Theme;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub config: Arc<Config>,
    pub theme: Arc<Theme>,
    pub renderer: Arc<Renderer>,
    pub oidc: Option<Arc<OidcProvider>>,
}
