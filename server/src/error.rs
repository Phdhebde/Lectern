use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub type AppResult<T> = Result<T, AppError>;

/// Errors returned by handlers. The `code` is stable and meant for the front-end
/// (translated there); the message is a developer hint and never contains internals.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("authentication required")]
    Unauthorized,
    #[error("forbidden: {0}")]
    Forbidden(&'static str),
    #[error("not found")]
    NotFound,
    #[error("{message}")]
    BadRequest { code: &'static str, message: String },
    #[error("{message}")]
    Conflict { code: &'static str, message: String, details: Option<serde_json::Value> },
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl AppError {
    pub fn bad_request(code: &'static str, message: impl Into<String>) -> Self {
        Self::BadRequest { code, message: message.into() }
    }

    pub fn conflict(code: &'static str, message: impl Into<String>) -> Self {
        Self::Conflict { code, message: message.into(), details: None }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            other => AppError::Internal(other.into()),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code, message, details) = match &self {
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized", self.to_string(), None),
            AppError::Forbidden(code) => (StatusCode::FORBIDDEN, *code, self.to_string(), None),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found", self.to_string(), None),
            AppError::BadRequest { code, message } => (StatusCode::BAD_REQUEST, *code, message.clone(), None),
            AppError::Conflict { code, message, details } => {
                (StatusCode::CONFLICT, *code, message.clone(), details.clone())
            }
            AppError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal", "internal error".to_string(), None)
            }
        };
        (status, Json(json!({ "error": code, "message": message, "details": details }))).into_response()
    }
}
