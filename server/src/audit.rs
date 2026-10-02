//! Append-only audit log for administration actions and exam results.

use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppResult;

pub async fn log<'e>(
    db: impl PgExecutor<'e>,
    actor: Option<Uuid>,
    action: &str,
    target: Option<String>,
    details: Value,
) -> AppResult<()> {
    sqlx::query("INSERT INTO audit_log (actor_id, action, target, details) VALUES ($1, $2, $3, $4)")
        .bind(actor)
        .bind(action)
        .bind(target)
        .bind(details)
        .execute(db)
        .await?;
    Ok(())
}
