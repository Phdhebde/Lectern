//! Track visibility and learner progress.

use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{CurrentUser, Role};
use crate::domain::exam::ExamDefinition;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Track {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub summary: String,
    pub description_md: String,
    pub audiences: Vec<String>,
    pub position: i32,
    pub prerequisite_slug: Option<String>,
    pub prerequisites_md: String,
    pub estimated_minutes: i32,
    pub scenarios_required: bool,
    pub validity_months: Option<i32>,
    pub module_quiz_pass_percent: i32,
    pub exam: serde_json::Value,
    pub recert_exam: Option<serde_json::Value>,
    pub badge: serde_json::Value,
    pub published: bool,
}

impl Track {
    pub fn exam_def(&self) -> AppResult<ExamDefinition> {
        serde_json::from_value(self.exam.clone()).map_err(|e| AppError::Internal(e.into()))
    }

    pub fn recert_def(&self) -> AppResult<Option<ExamDefinition>> {
        self.recert_exam.clone().map(serde_json::from_value).transpose().map_err(|e| AppError::Internal(e.into()))
    }
}

pub fn is_staff(user: Option<&CurrentUser>) -> bool {
    user.is_some_and(|u| u.has_role(Role::Admin) || u.has_role(Role::Trainer) || u.has_role(Role::ChannelManager))
}

/// Whether the viewer may see and follow the track.
pub fn can_access(track: &Track, user: Option<&CurrentUser>) -> bool {
    if is_staff(user) {
        return true;
    }
    if !track.published {
        return false;
    }
    if track.audiences.iter().any(|a| a == "public") {
        return true;
    }
    let Some(kind) = user.and_then(|u| u.org_kind()) else { return false };
    track.audiences.iter().any(|a| a == kind)
}

pub async fn load_track(db: &PgPool, slug: &str) -> AppResult<Track> {
    Ok(sqlx::query_as("SELECT * FROM tracks WHERE slug = $1").bind(slug).fetch_one(db).await?)
}

pub async fn load_track_by_id(db: &PgPool, id: Uuid) -> AppResult<Track> {
    Ok(sqlx::query_as("SELECT * FROM tracks WHERE id = $1").bind(id).fetch_one(db).await?)
}

pub async fn accessible_track(db: &PgPool, slug: &str, user: Option<&CurrentUser>) -> AppResult<Track> {
    let track = load_track(db, slug).await?;
    if !can_access(&track, user) {
        // Do not reveal the existence of restricted tracks.
        return Err(AppError::NotFound);
    }
    Ok(track)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ModuleProgress {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub position: i32,
    pub duration_minutes: i32,
    pub has_video: bool,
    pub quiz_questions: i64,
    pub content_completed: bool,
    pub quiz_best_score: Option<i32>,
    pub completed: bool,
}

pub async fn module_progress(db: &PgPool, track_id: Uuid, user_id: Option<Uuid>) -> AppResult<Vec<ModuleProgress>> {
    Ok(sqlx::query_as(
        "SELECT m.id, m.slug, m.title, m.position, m.duration_minutes, (m.video_url IS NOT NULL) AS has_video,
            (SELECT count(*) FROM questions q WHERE q.module_id = m.id AND q.pool = 'quiz' AND q.active) AS quiz_questions,
            COALESCE(p.video_completed, FALSE) AS content_completed,
            p.quiz_best_score,
            (p.completed_at IS NOT NULL) AS completed
         FROM modules m
         LEFT JOIN module_progress p ON p.module_id = m.id AND p.user_id = $2
         WHERE m.track_id = $1 ORDER BY m.position",
    )
    .bind(track_id)
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ScenarioProgress {
    pub id: Uuid,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub family: Option<String>,
    pub steps: i64,
    pub current_step: i32,
    pub completed: bool,
}

pub async fn scenario_progress(db: &PgPool, track_id: Uuid, user_id: Option<Uuid>) -> AppResult<Vec<ScenarioProgress>> {
    Ok(sqlx::query_as(
        "SELECT s.id, s.slug, s.title, s.kind, s.family,
            (SELECT count(*) FROM scenario_steps st WHERE st.scenario_id = s.id) AS steps,
            COALESCE(p.current_step, 0) AS current_step,
            (p.completed_at IS NOT NULL) AS completed
         FROM scenarios s
         LEFT JOIN scenario_progress p ON p.scenario_id = s.id AND p.user_id = $2
         WHERE s.track_id = $1 AND NOT s.exam_only ORDER BY s.position, s.slug",
    )
    .bind(track_id)
    .bind(user_id)
    .fetch_all(db)
    .await?)
}

/// Module completion rule: content viewed (video watched or sheet read) and, when the
/// module has a quiz, the quiz passed.
pub fn module_completed(
    content_completed: bool,
    quiz_questions: i64,
    quiz_best: Option<i32>,
    pass_percent: i32,
) -> bool {
    content_completed && (quiz_questions == 0 || quiz_best.is_some_and(|s| s >= pass_percent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_completion_rule() {
        assert!(module_completed(true, 0, None, 70));
        assert!(!module_completed(false, 0, None, 70));
        assert!(!module_completed(true, 3, Some(66), 70));
        assert!(module_completed(true, 3, Some(70), 70));
    }
}
