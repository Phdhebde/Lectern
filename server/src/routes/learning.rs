//! Following a track: modules (video + recap sheet + quiz), scenarios, ratings.

use std::collections::{BTreeMap, HashMap};

use axum::Json;
use axum::extract::{Path, State};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{Value, json};
use uuid::Uuid;

use crate::auth::{CurrentUser, MaybeUser};
use crate::domain::attempts;
use crate::domain::catalog::{self, Track};
use crate::domain::certs;
use crate::domain::exam::{self, Answer, percent};
use crate::error::{AppError, AppResult};
use crate::render::markdown;
use crate::state::AppState;

pub async fn track_detail(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let track = catalog::accessible_track(&state.db, &slug, user.as_ref()).await?;
    let uid = user.as_ref().map(|u| u.id);
    let modules = catalog::module_progress(&state.db, track.id, uid).await?;
    let scenarios = catalog::scenario_progress(&state.db, track.id, uid).await?;
    let (exam_status, certification, enrollment) = match &user {
        Some(u) => {
            let status = attempts::exam_status(&state, u, &track).await?;
            let cert = certs::current(&state.db, u.id, track.id).await?.map(|c| {
                json!({ "id": c.id, "status": c.status(Utc::now()), "issued_at": c.issued_at, "expires_at": c.expires_at, "provisional": c.provisional })
            });
            let enrollment: Option<Value> = sqlx::query_scalar(
                "SELECT jsonb_build_object('enrolled_at', enrolled_at, 'last_module', last_module, 'completed_at', completed_at, 'rating', rating)
                 FROM enrollments WHERE user_id = $1 AND track_id = $2",
            )
            .bind(u.id)
            .bind(track.id)
            .fetch_optional(&state.db)
            .await?;
            (Some(status), cert, enrollment)
        }
        None => (None, None, None),
    };
    let def = track.exam_def()?;
    Ok(Json(json!({
        "slug": track.slug,
        "title": track.title,
        "summary": track.summary,
        "description_html": markdown(&track.description_md),
        "prerequisites_html": markdown(&track.prerequisites_md),
        "prerequisite": track.prerequisite_slug,
        "audiences": track.audiences,
        "estimated_minutes": track.estimated_minutes,
        "validity_months": track.validity_months,
        "scenarios_required": track.scenarios_required,
        "quiz_pass_percent": track.module_quiz_pass_percent,
        "exam": {
            "sections": attempts::summarize(&def),
            "free_attempts": def.free_attempts,
            "cooldown_days": def.cooldown_days,
        },
        "modules": modules,
        "scenarios": scenarios,
        "exam_status": exam_status,
        "certification": certification,
        "enrollment": enrollment,
    })))
}

pub async fn enroll(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(slug): Path<String>,
) -> AppResult<Json<Value>> {
    let track = catalog::accessible_track(&state.db, &slug, Some(&user)).await?;
    sqlx::query("INSERT INTO enrollments (user_id, track_id) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(user.id)
        .bind(track.id)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct Rating {
    rating: i32,
}

pub async fn rate(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(slug): Path<String>,
    Json(r): Json<Rating>,
) -> AppResult<Json<Value>> {
    if !(1..=5).contains(&r.rating) {
        return Err(AppError::bad_request("invalid_rating", "rating must be 1-5"));
    }
    let track = catalog::accessible_track(&state.db, &slug, Some(&user)).await?;
    sqlx::query("UPDATE enrollments SET rating = $3 WHERE user_id = $1 AND track_id = $2")
        .bind(user.id)
        .bind(track.id)
        .bind(r.rating)
        .execute(&state.db)
        .await?;
    Ok(Json(json!({ "ok": true })))
}

#[derive(sqlx::FromRow)]
struct ModuleRow {
    id: Uuid,
    track_id: Uuid,
    slug: String,
    position: i32,
    title: String,
    video_url: Option<String>,
    captions_url: Option<String>,
    duration_minutes: i32,
    body_md: String,
    attachments: Value,
    doc_url: Option<String>,
}

async fn module_in_track(state: &AppState, user: &CurrentUser, module_id: Uuid) -> AppResult<(ModuleRow, Track)> {
    let m: ModuleRow = sqlx::query_as(
        "SELECT id, track_id, slug, position, title, video_url, captions_url, duration_minutes, body_md, attachments, doc_url
         FROM modules WHERE id = $1",
    )
    .bind(module_id)
    .fetch_one(&state.db)
    .await?;
    let track = catalog::load_track_by_id(&state.db, m.track_id).await?;
    if !catalog::can_access(&track, Some(user)) {
        return Err(AppError::NotFound);
    }
    Ok((m, track))
}

/// Quiz questions without their answers.
fn public_question(prompt: &str, format: &str, choices: &Value, id: Uuid) -> Value {
    let choices: Vec<Value> = choices
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|c| json!({ "id": c["id"], "html": markdown(c["text"].as_str().unwrap_or_default()) }))
        .collect();
    json!({ "id": id, "prompt_html": markdown(prompt), "format": format, "choices": choices })
}

pub async fn module_detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, module_slug)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let track = catalog::accessible_track(&state.db, &slug, Some(&user)).await?;
    let id: Uuid = sqlx::query_scalar("SELECT id FROM modules WHERE track_id = $1 AND slug = $2")
        .bind(track.id)
        .bind(&module_slug)
        .fetch_one(&state.db)
        .await?;
    let (m, _) = module_in_track(&state, &user, id).await?;
    sqlx::query(
        "INSERT INTO enrollments (user_id, track_id, last_module) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, track_id) DO UPDATE SET last_module = $3",
    )
    .bind(user.id)
    .bind(track.id)
    .bind(m.id)
    .execute(&state.db)
    .await?;
    let questions: Vec<(Uuid, String, String, Value)> = sqlx::query_as(
        "SELECT id, prompt_md, format, choices FROM questions WHERE module_id = $1 AND pool = 'quiz' AND active ORDER BY ref",
    )
    .bind(m.id)
    .fetch_all(&state.db)
    .await?;
    let progress: Option<(i32, bool, Option<i32>, bool)> = sqlx::query_as(
        "SELECT video_position, video_completed, quiz_best_score, completed_at IS NOT NULL FROM module_progress
         WHERE user_id = $1 AND module_id = $2",
    )
    .bind(user.id)
    .bind(m.id)
    .fetch_optional(&state.db)
    .await?;
    let siblings: Vec<(String, String)> =
        sqlx::query_as("SELECT slug, title FROM modules WHERE track_id = $1 ORDER BY position")
            .bind(track.id)
            .fetch_all(&state.db)
            .await?;
    let idx = siblings.iter().position(|(s, _)| *s == m.slug);
    let prev = idx.and_then(|i| i.checked_sub(1)).and_then(|i| siblings.get(i));
    let next = idx.and_then(|i| siblings.get(i + 1));
    let doc_url = m.doc_url.as_ref().map(|u| {
        if u.starts_with("http") {
            u.clone()
        } else {
            format!("{}{}", state.config.instance.documentation_url.clone().unwrap_or_default(), u)
        }
    });
    Ok(Json(json!({
        "id": m.id,
        "slug": m.slug,
        "position": m.position,
        "title": m.title,
        "track": { "slug": track.slug, "title": track.title },
        "video_url": m.video_url,
        "captions_url": m.captions_url,
        "duration_minutes": m.duration_minutes,
        "body_html": markdown(&m.body_md),
        "attachments": m.attachments,
        "doc_url": doc_url,
        "quiz": questions.iter().map(|(id, p, f, c)| public_question(p, f, c, *id)).collect::<Vec<_>>(),
        "quiz_pass_percent": track.module_quiz_pass_percent,
        "progress": progress.map(|(pos, done, best, completed)| json!({ "video_position": pos, "content_completed": done, "quiz_best_score": best, "completed": completed })),
        "previous": prev.map(|(s, t)| json!({ "slug": s, "title": t })),
        "next": next.map(|(s, t)| json!({ "slug": s, "title": t })),
    })))
}

#[derive(Deserialize)]
pub struct ProgressUpdate {
    #[serde(default)]
    video_position: Option<i32>,
    #[serde(default)]
    content_completed: Option<bool>,
}

pub async fn module_progress(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<ProgressUpdate>,
) -> AppResult<Json<Value>> {
    let (m, track) = module_in_track(&state, &user, id).await?;
    sqlx::query(
        "INSERT INTO module_progress (user_id, module_id, video_position, video_completed)
         VALUES ($1, $2, COALESCE($3, 0), COALESCE($4, FALSE))
         ON CONFLICT (user_id, module_id) DO UPDATE SET
            video_position = COALESCE($3, module_progress.video_position),
            video_completed = module_progress.video_completed OR COALESCE($4, FALSE),
            updated_at = now()",
    )
    .bind(user.id)
    .bind(m.id)
    .bind(req.video_position.map(|p| p.max(0)))
    .bind(req.content_completed)
    .execute(&state.db)
    .await?;
    let completed = refresh_completion(&state, user.id, m.id, track.module_quiz_pass_percent).await?;
    Ok(Json(json!({ "completed": completed })))
}

async fn refresh_completion(state: &AppState, user_id: Uuid, module_id: Uuid, pass: i32) -> AppResult<bool> {
    let (content, best, quiz_count): (bool, Option<i32>, i64) = sqlx::query_as(
        "SELECT p.video_completed, p.quiz_best_score,
            (SELECT count(*) FROM questions q WHERE q.module_id = $2 AND q.pool = 'quiz' AND q.active)
         FROM module_progress p WHERE p.user_id = $1 AND p.module_id = $2",
    )
    .bind(user_id)
    .bind(module_id)
    .fetch_one(&state.db)
    .await?;
    let done = catalog::module_completed(content, quiz_count, best, pass);
    if done {
        sqlx::query("UPDATE module_progress SET completed_at = COALESCE(completed_at, now()) WHERE user_id = $1 AND module_id = $2")
            .bind(user_id)
            .bind(module_id)
            .execute(&state.db)
            .await?;
    }
    Ok(done)
}

#[derive(Deserialize)]
pub struct QuizSubmission {
    answers: BTreeMap<Uuid, Answer>,
}

/// Grades a set of quiz questions and returns the correction with explanations.
async fn grade_quiz(
    state: &AppState,
    question_ids: &[Uuid],
    answers: &BTreeMap<Uuid, Answer>,
) -> AppResult<(u32, Vec<Value>)> {
    let keys = attempts::answer_keys(&state.db, question_ids).await?;
    let explanations: HashMap<Uuid, (String, Value)> = sqlx::query_as::<_, (Uuid, String, Value)>(
        "SELECT id, explanation_md, choices FROM questions WHERE id = ANY($1)",
    )
    .bind(question_ids)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(|(id, e, c)| (id, (e, c)))
    .collect();
    let mut correct = 0u32;
    let mut details = Vec::new();
    for id in question_ids {
        let Some(key) = keys.get(id) else { continue };
        let ok = exam::is_correct(key, answers.get(id));
        correct += u32::from(ok);
        let (explanation, choices) = explanations.get(id).cloned().unwrap_or_default();
        details.push(json!({
            "question_id": id,
            "correct": ok,
            "correct_choices": correct_choice_ids(&choices),
            "explanation_html": markdown(&explanation),
        }));
    }
    Ok((percent(correct, question_ids.len() as u32), details))
}

fn correct_choice_ids(choices: &Value) -> Vec<&str> {
    choices
        .as_array()
        .map(|a| a.iter().filter(|c| c["correct"].as_bool() == Some(true)).filter_map(|c| c["id"].as_str()).collect())
        .unwrap_or_default()
}

pub async fn submit_module_quiz(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<QuizSubmission>,
) -> AppResult<Json<Value>> {
    let (m, track) = module_in_track(&state, &user, id).await?;
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM questions WHERE module_id = $1 AND pool = 'quiz' AND active")
            .bind(m.id)
            .fetch_all(&state.db)
            .await?;
    if ids.is_empty() {
        return Err(AppError::bad_request("no_quiz", "this module has no quiz"));
    }
    let (score, details) = grade_quiz(&state, &ids, &req.answers).await?;
    sqlx::query(
        "INSERT INTO module_progress (user_id, module_id, quiz_best_score) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, module_id) DO UPDATE SET
            quiz_best_score = GREATEST(COALESCE(module_progress.quiz_best_score, 0), $3), updated_at = now()",
    )
    .bind(user.id)
    .bind(m.id)
    .bind(score as i32)
    .execute(&state.db)
    .await?;
    let completed = refresh_completion(&state, user.id, m.id, track.module_quiz_pass_percent).await?;
    Ok(Json(json!({
        "score": score,
        "passed": score as i32 >= track.module_quiz_pass_percent,
        "completed": completed,
        "results": details,
    })))
}

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

pub async fn scenario_steps(state: &AppState, scenario_id: Uuid, include_expected: bool) -> AppResult<Vec<Value>> {
    let steps: Vec<(i32, String, Option<Uuid>, String, Value, String)> = sqlx::query_as(
        "SELECT position, action_md, image_asset, image_alt, annotations, expected_md
         FROM scenario_steps WHERE scenario_id = $1 ORDER BY position",
    )
    .bind(scenario_id)
    .fetch_all(&state.db)
    .await?;
    Ok(steps
        .into_iter()
        .map(|(pos, action, image, alt, annotations, expected)| {
            json!({
                "position": pos,
                "action_html": markdown(&action),
                "image_url": image.map(|i| format!("/api/assets/{i}")),
                "image_alt": alt,
                "annotations": annotations,
                "expected_html": if include_expected { Some(markdown(&expected)) } else { None },
            })
        })
        .collect())
}

pub async fn scenario_detail(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((slug, scenario_slug)): Path<(String, String)>,
) -> AppResult<Json<Value>> {
    let track = catalog::accessible_track(&state.db, &slug, Some(&user)).await?;
    let s: (Uuid, String, String, String, String, Option<String>) = sqlx::query_as(
        "SELECT id, title, kind, context_md, pitfalls_md, family FROM scenarios
         WHERE track_id = $1 AND slug = $2 AND NOT exam_only",
    )
    .bind(track.id)
    .bind(&scenario_slug)
    .fetch_one(&state.db)
    .await?;
    let (id, title, kind, context, pitfalls, family) = s;
    let questions: Vec<(Uuid, String, String, Value)> = sqlx::query_as(
        "SELECT id, prompt_md, format, choices FROM questions WHERE scenario_id = $1 AND pool = 'quiz' AND active ORDER BY ref",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await?;
    let progress: Option<(i32, bool)> = sqlx::query_as(
        "SELECT current_step, completed_at IS NOT NULL FROM scenario_progress WHERE user_id = $1 AND scenario_id = $2",
    )
    .bind(user.id)
    .bind(id)
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(json!({
        "id": id,
        "slug": scenario_slug,
        "title": title,
        "kind": kind,
        "family": family,
        "track": { "slug": track.slug, "title": track.title },
        "context_html": markdown(&context),
        "pitfalls_html": markdown(&pitfalls),
        "steps": scenario_steps(&state, id, true).await?,
        "questions": questions.iter().map(|(qid, p, f, c)| public_question(p, f, c, *qid)).collect::<Vec<_>>(),
        "progress": progress.map(|(step, done)| json!({ "current_step": step, "completed": done })),
    })))
}

async fn learning_scenario(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<Uuid> {
    let (track_id, exam_only): (Uuid, bool) =
        sqlx::query_as("SELECT track_id, exam_only FROM scenarios WHERE id = $1").bind(id).fetch_one(&state.db).await?;
    let track = catalog::load_track_by_id(&state.db, track_id).await?;
    if exam_only || !catalog::can_access(&track, Some(user)) {
        return Err(AppError::NotFound);
    }
    Ok(track_id)
}

#[derive(Deserialize)]
pub struct StepUpdate {
    step: i32,
}

pub async fn scenario_progress(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<StepUpdate>,
) -> AppResult<Json<Value>> {
    learning_scenario(&state, &user, id).await?;
    sqlx::query(
        "INSERT INTO scenario_progress (user_id, scenario_id, current_step) VALUES ($1, $2, $3)
         ON CONFLICT (user_id, scenario_id) DO UPDATE SET current_step = GREATEST(scenario_progress.current_step, $3), updated_at = now()",
    )
    .bind(user.id)
    .bind(id)
    .bind(req.step.max(0))
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "ok": true })))
}

/// Verification questions at the end of a scenario. The scenario is complete when all
/// answers are right (or immediately if it has no question once the last step is reached).
pub async fn scenario_check(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<Uuid>,
    Json(req): Json<QuizSubmission>,
) -> AppResult<Json<Value>> {
    learning_scenario(&state, &user, id).await?;
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM questions WHERE scenario_id = $1 AND pool = 'quiz' AND active")
            .bind(id)
            .fetch_all(&state.db)
            .await?;
    let (score, details) =
        if ids.is_empty() { (100, Vec::new()) } else { grade_quiz(&state, &ids, &req.answers).await? };
    let passed = score == 100;
    sqlx::query(
        "INSERT INTO scenario_progress (user_id, scenario_id, current_step, completed_at) VALUES ($1, $2, 0, CASE WHEN $3 THEN now() END)
         ON CONFLICT (user_id, scenario_id) DO UPDATE SET
            completed_at = COALESCE(scenario_progress.completed_at, CASE WHEN $3 THEN now() END), updated_at = now()",
    )
    .bind(user.id)
    .bind(id)
    .bind(passed)
    .execute(&state.db)
    .await?;
    Ok(Json(json!({ "score": score, "passed": passed, "results": details })))
}
