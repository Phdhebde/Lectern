//! Taking an exam, and the evaluator's manual review.

use std::collections::HashMap;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::audit;
use crate::auth::{CurrentUser, Role};
use crate::domain::attempts::{self, AttemptRow};
use crate::domain::catalog;
use crate::domain::exam::{self, Answer};
use crate::error::{AppError, AppResult};
use crate::pack::asset_path;
use crate::render::markdown;
use crate::routes::learning::scenario_steps;
use crate::state::AppState;

pub async fn start(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let track = catalog::accessible_track(&state.db, &slug, Some(&user)).await?;
    let id = attempts::start(&state, &user, &track).await?;
    Ok(Json(json!({ "attempt_id": id })))
}

async fn attempt_view(state: &AppState, attempt: &AttemptRow, include_keys: bool) -> AppResult<Value> {
    let paper = attempt.paper()?;
    let track = catalog::load_track_by_id(&state.db, attempt.track_id).await?;
    let sections: Vec<Value> = paper
        .sections
        .iter()
        .map(|s| json!({ "title": s.title, "duration_minutes": s.duration_minutes, "pass_percent": s.pass_percent, "items": s.items.len() }))
        .collect();
    // Learners see only the current section; reviewers see the whole paper.
    let visible: Vec<usize> = if include_keys {
        (0..paper.sections.len()).collect()
    } else if attempt.status == "in_progress" {
        vec![attempt.current_section as usize]
    } else {
        Vec::new()
    };
    let answers = attempt.answers();
    let mut out_sections = Vec::new();
    for idx in visible {
        let s = &paper.sections[idx];
        let ids: Vec<Uuid> = s.items.iter().map(|i| i.question_id).collect();
        let rows: HashMap<Uuid, (String, String, Value, String)> =
            sqlx::query_as::<_, (Uuid, String, String, Value, String)>(
                "SELECT id, prompt_md, format, choices, explanation_md FROM questions WHERE id = ANY($1)",
            )
            .bind(&ids)
            .fetch_all(&state.db)
            .await?
            .into_iter()
            .map(|(id, p, f, c, e)| (id, (p, f, c, e)))
            .collect();
        let items: Vec<Value> = s
            .items
            .iter()
            .filter_map(|item| {
                let (prompt, format, choices, explanation) = rows.get(&item.question_id)?;
                let by_id: HashMap<&str, &Value> =
                    choices.as_array()?.iter().filter_map(|c| Some((c["id"].as_str()?, c))).collect();
                let ordered: Vec<Value> = item
                    .choice_order
                    .iter()
                    .filter_map(|cid| {
                        let c = by_id.get(cid.as_str())?;
                        let mut v = json!({ "id": cid, "html": markdown(c["text"].as_str().unwrap_or_default()) });
                        if include_keys {
                            v["correct"] = c["correct"].clone();
                        }
                        Some(v)
                    })
                    .collect();
                let mut v = json!({
                    "question_id": item.question_id,
                    "prompt_html": markdown(prompt),
                    "format": format,
                    "choices": ordered,
                    "answer": answers.get(&item.question_id),
                });
                if include_keys {
                    v["explanation_html"] = json!(markdown(explanation));
                }
                Some(v)
            })
            .collect();
        let mut scenarios = Vec::new();
        for sid in &s.scenarios {
            let (title, context): (String, String) =
                sqlx::query_as("SELECT title, context_md FROM scenarios WHERE id = $1")
                    .bind(sid)
                    .fetch_one(&state.db)
                    .await?;
            scenarios.push(json!({
                "id": sid,
                "title": title,
                "context_html": markdown(&context),
                "steps": scenario_steps(state, *sid, include_keys).await?,
            }));
        }
        out_sections.push(json!({ "index": idx, "title": s.title, "items": items, "scenarios": scenarios }));
    }
    Ok(json!({
        "id": attempt.id,
        "track": { "slug": track.slug, "title": track.title },
        "purpose": attempt.purpose,
        "status": attempt.status,
        "current_section": attempt.current_section,
        "section_deadline": attempt.section_deadline,
        "server_now": Utc::now(),
        "sections": sections,
        "visible_sections": out_sections,
        "results": attempt.results,
        "started_at": attempt.started_at,
        "finished_at": attempt.finished_at,
        "review": attempt.review.as_ref().map(|r| json!({ "comment": r.get("comment"), "decision": r.get("decision") })),
    }))
}

pub async fn get_attempt(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    let attempt = attempts::own_attempt(&state, &user, id).await?;
    Ok(Json(attempt_view(&state, &attempt, false).await?))
}

#[derive(Deserialize)]
pub struct AnswerReq {
    question_id: Uuid,
    answer: Answer,
}

pub async fn save_answer(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<AnswerReq>,
) -> AppResult<Json<Value>> {
    attempts::save_answer(&state, &user, id, req.question_id, req.answer).await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct SubmitReq {
    section: i32,
}

pub async fn submit_section(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SubmitReq>,
) -> AppResult<Json<Value>> {
    let attempt = attempts::own_attempt(&state, &user, id).await?;
    if attempt.status == "in_progress" {
        attempts::submit_section(&state, id, req.section).await?;
    }
    let attempt = attempts::load_attempt(&state.db, id).await?;
    Ok(Json(attempt_view(&state, &attempt, false).await?))
}

// ---------------------------------------------------------------------------
// Assets (screenshots, attachments)
// ---------------------------------------------------------------------------

/// Serves an uploaded asset. Screenshots of exam case studies are only served to
/// staff and to learners whose open attempt contains that case study.
pub async fn asset(State(state): State<AppState>, user: CurrentUser, Path(id): Path<Uuid>) -> AppResult<Response> {
    let (sha, content_type, name): (String, String, String) =
        sqlx::query_as("SELECT sha256, content_type, original_name FROM assets WHERE id = $1")
            .bind(id)
            .fetch_one(&state.db)
            .await?;
    let exam_scenarios: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT s.id FROM scenario_steps st JOIN scenarios s ON s.id = st.scenario_id
         WHERE st.image_asset = $1 AND s.exam_only",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    if !exam_scenarios.is_empty() && user.require_any(&[Role::Admin, Role::Trainer]).is_err() {
        let papers: Vec<Value> =
            sqlx::query_scalar("SELECT paper FROM exam_attempts WHERE user_id = $1 AND status = 'in_progress'")
                .bind(user.id)
                .fetch_all(&state.db)
                .await?;
        let allowed = papers.iter().any(|p| {
            serde_json::from_value::<exam::Paper>(p.clone())
                .map(|paper| paper.sections.iter().flat_map(|s| &s.scenarios).any(|s| exam_scenarios.contains(s)))
                .unwrap_or(false)
        });
        if !allowed {
            return Err(AppError::NotFound);
        }
    }
    let bytes = tokio::fs::read(asset_path(&state.config.server.data_dir, &sha))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let inline = content_type.starts_with("image/") || content_type == "text/vtt" || content_type.starts_with("video/");
    let disposition = if inline {
        "inline".to_string()
    } else {
        format!("attachment; filename=\"{}\"", name.replace(['"', '\\', '\r', '\n'], "_"))
    };
    Ok((
        [
            (header::CONTENT_TYPE, content_type),
            (header::CONTENT_DISPOSITION, disposition),
            (header::CACHE_CONTROL, "private, max-age=3600".to_string()),
        ],
        bytes,
    )
        .into_response())
}

// ---------------------------------------------------------------------------
// Manual review (evaluators)
// ---------------------------------------------------------------------------

pub async fn pending_reviews(State(state): State<AppState>, user: CurrentUser) -> AppResult<Json<Value>> {
    user.require(Role::Trainer)?;
    let rows: Option<Value> = sqlx::query_scalar(
        "SELECT jsonb_agg(x ORDER BY x.finished_at) FROM (
            SELECT a.id, a.finished_at, t.slug AS track_slug, t.title AS track_title, u.display_name AS learner,
                   o.name AS organization
            FROM exam_attempts a JOIN tracks t ON t.id = a.track_id JOIN users u ON u.id = a.user_id
            LEFT JOIN memberships m ON m.user_id = u.id LEFT JOIN organizations o ON o.id = m.org_id
            WHERE a.status = 'pending_review') x",
    )
    .fetch_one(&state.db)
    .await?;
    Ok(Json(rows.unwrap_or(json!([]))))
}

pub async fn review_detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Value>> {
    user.require(Role::Trainer)?;
    let attempt = attempts::load_attempt(&state.db, id).await?;
    let mut view = attempt_view(&state, &attempt, true).await?;
    let learner: (String, String) = sqlx::query_as("SELECT display_name, email FROM users WHERE id = $1")
        .bind(attempt.user_id)
        .fetch_one(&state.db)
        .await?;
    view["learner"] = json!({ "name": learner.0, "email": learner.1 });
    view["review_full"] = attempt.review.clone().unwrap_or(Value::Null);
    Ok(Json(view))
}

#[derive(Deserialize)]
pub struct ReviewReq {
    decision: String,
    #[serde(default)]
    comment: String,
    /// Evaluation grid: [{"criterion": "...", "score": 3, "max": 5}]
    #[serde(default)]
    grid: Vec<Value>,
}

pub async fn submit_review(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ReviewReq>,
) -> AppResult<Json<Value>> {
    user.require(Role::Trainer)?;
    let status = match req.decision.as_str() {
        "pass" => "passed",
        "fail" => "failed",
        _ => return Err(AppError::bad_request("invalid_decision", "decision must be pass or fail")),
    };
    let mut tx = state.db.begin().await?;
    let attempt: AttemptRow = sqlx::query_as(
        "SELECT id, user_id, track_id, purpose, status, paper, answers, current_section, section_deadline, results,
            started_at, finished_at, review FROM exam_attempts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if attempt.status != "pending_review" {
        return Err(AppError::conflict("not_pending", "this attempt is not awaiting review"));
    }
    if attempt.user_id == user.id {
        return Err(AppError::Forbidden("cannot_review_own_attempt"));
    }
    let review =
        json!({ "decision": req.decision, "comment": req.comment, "grid": req.grid, "reviewed_at": Utc::now() });
    sqlx::query("UPDATE exam_attempts SET status = $2, review = $3, reviewer_id = $4 WHERE id = $1")
        .bind(id)
        .bind(status)
        .bind(&review)
        .bind(user.id)
        .execute(&mut *tx)
        .await?;
    let track = catalog::load_track_by_id(&state.db, attempt.track_id).await?;
    attempts::finalize(&state, &mut tx, &attempt, &track, status).await?;
    audit::log(&mut *tx, Some(user.id), "exam.review", Some(id.to_string()), json!({ "decision": req.decision }))
        .await?;
    tx.commit().await?;
    Ok(Json(json!({ "ok": true, "status": status })))
}
