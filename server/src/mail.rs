//! E-mail delivery through a transactional outbox.
//!
//! Handlers only insert rows into `email_outbox` (in the same transaction as the
//! change that triggers them when possible); a background worker sends them with
//! retries. Without SMTP configuration, messages are written to the log.

use std::time::Duration;

use chrono::Utc;
use lettre::message::MultiPart;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use serde_json::Value;
use sqlx::PgExecutor;
use uuid::Uuid;

use crate::error::AppResult;
use crate::state::AppState;

const MAX_ATTEMPTS: i32 = 8;

/// Renders a localized e-mail and queues it.
pub async fn send_template(state: &AppState, to: &str, kind: &str, ctx: Value) -> AppResult<()> {
    queue_template(&state.db, state, to, kind, ctx).await
}

pub async fn queue_template<'e>(
    db: impl PgExecutor<'e>,
    state: &AppState,
    to: &str,
    kind: &str,
    ctx: Value,
) -> AppResult<()> {
    let (subject, html, text) = state.renderer.email(kind, &ctx)?;
    sqlx::query("INSERT INTO email_outbox (id, to_address, subject, html_body, text_body) VALUES ($1, $2, $3, $4, $5)")
        .bind(Uuid::new_v4())
        .bind(to)
        .bind(subject)
        .bind(html)
        .bind(text)
        .execute(db)
        .await?;
    Ok(())
}

#[derive(sqlx::FromRow)]
struct Pending {
    id: Uuid,
    to_address: String,
    subject: String,
    html_body: String,
    text_body: String,
    attempts: i32,
}

pub fn spawn_worker(state: AppState) {
    tokio::spawn(async move {
        let transport = match &state.config.mail.smtp_url {
            Some(url) => match AsyncSmtpTransport::<Tokio1Executor>::from_url(url) {
                Ok(b) => Some(b.build()),
                Err(e) => {
                    tracing::error!(error = %e, "invalid SMTP URL; e-mails will only be logged");
                    None
                }
            },
            None => None,
        };
        loop {
            if let Err(e) = deliver_batch(&state, transport.as_ref()).await {
                tracing::error!(error = ?e, "e-mail worker failed");
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

async fn deliver_batch(state: &AppState, transport: Option<&AsyncSmtpTransport<Tokio1Executor>>) -> anyhow::Result<()> {
    // SKIP LOCKED lets several replicas run the worker without sending twice.
    let mut tx = state.db.begin().await?;
    let batch: Vec<Pending> = sqlx::query_as(
        "SELECT id, to_address, subject, html_body, text_body, attempts FROM email_outbox
         WHERE sent_at IS NULL AND send_after <= now() AND attempts < $1
         ORDER BY created_at LIMIT 20 FOR UPDATE SKIP LOCKED",
    )
    .bind(MAX_ATTEMPTS)
    .fetch_all(&mut *tx)
    .await?;
    for mail in batch {
        let result = match transport {
            Some(t) => send(state, t, &mail).await,
            None => {
                tracing::info!(to = %mail.to_address, subject = %mail.subject, body = %mail.text_body, "e-mail (SMTP not configured)");
                Ok(())
            }
        };
        match result {
            Ok(()) => {
                sqlx::query("UPDATE email_outbox SET sent_at = now(), attempts = attempts + 1 WHERE id = $1")
                    .bind(mail.id)
                    .execute(&mut *tx)
                    .await?;
            }
            Err(e) => {
                let backoff = 60 * 2i64.pow(mail.attempts.clamp(0, 10) as u32);
                tracing::warn!(error = %e, to = %mail.to_address, "e-mail delivery failed");
                sqlx::query(
                    "UPDATE email_outbox SET attempts = attempts + 1, last_error = $2, send_after = $3 WHERE id = $1",
                )
                .bind(mail.id)
                .bind(e.to_string())
                .bind(Utc::now() + chrono::Duration::seconds(backoff))
                .execute(&mut *tx)
                .await?;
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn send(state: &AppState, transport: &AsyncSmtpTransport<Tokio1Executor>, mail: &Pending) -> anyhow::Result<()> {
    let message = Message::builder()
        .from(state.config.mail_from().parse()?)
        .to(mail.to_address.parse()?)
        .subject(&mail.subject)
        .multipart(MultiPart::alternative_plain_html(mail.text_body.clone(), mail.html_body.clone()))?;
    transport.send(message).await?;
    Ok(())
}
