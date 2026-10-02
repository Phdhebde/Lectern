//! Exam attempt lifecycle: who may start, drawing the paper, saving answers,
//! grading section by section and issuing the certification.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

use crate::audit;
use crate::auth::CurrentUser;
use crate::domain::catalog::{self, Track};
use crate::domain::certs;
use crate::domain::exam::{
    self, Answer, AnswerKey, CandidateQuestion, Eligibility, ExamDefinition, Ineligible, Paper, PastAttempt,
    QuestionPool, SectionResult, SectionSource,
};
use crate::error::{AppError, AppResult};
use crate::mail;
use crate::state::AppState;

/// Network grace period after a section deadline.
pub const GRACE_SECONDS: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Certification,
    Recertification,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Certification => "certification",
            Purpose::Recertification => "recertification",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SectionSummary {
    pub title: String,
    pub duration_minutes: u32,
    pub pass_percent: u32,
    pub items: String,
}

#[derive(Debug, Serialize)]
pub struct ExamStatus {
    /// What the next attempt would be; `None` when the learner holds a valid
    /// certification outside its recertification window.
    pub purpose: Option<Purpose>,
    pub sections: Vec<SectionSummary>,
    /// Conditions not yet met (modules, scenarios, prerequisite, exam not ready).
    pub blockers: Vec<String>,
    pub eligibility: Option<Result<&'static str, Ineligible>>,
    pub active_attempt: Option<Uuid>,
    pub recert_opens_at: Option<DateTime<Utc>>,
    pub attempts_used: usize,
    pub free_attempts: Option<u32>,
    pub credits: i64,
}

pub fn summarize(def: &ExamDefinition) -> Vec<SectionSummary> {
    def.sections
        .iter()
        .map(|s| SectionSummary {
            title: s.title.clone(),
            duration_minutes: s.duration_minutes,
            pass_percent: s.pass_percent,
            items: match &s.source {
                SectionSource::Questions { question_count, .. } => format!("{question_count} questions"),
                SectionSource::CaseStudy { scenario_count } => format!("{scenario_count} case studies"),
            },
        })
        .collect()
}

/// Definition used for the next attempt of the given purpose.
pub fn definition_for(track: &Track, purpose: Purpose, provisional_holder: bool) -> AppResult<ExamDefinition> {
    match purpose {
        Purpose::Recertification if !provisional_holder => Ok(track.recert_def()?.unwrap_or(track.exam_def()?)),
        _ => track.exam_def(),
    }
}

async fn past_attempts(db: &PgPool, user_id: Uuid, track_id: Uuid, purpose: Purpose) -> AppResult<Vec<PastAttempt>> {
    let rows: Vec<(String, DateTime<Utc>, Option<DateTime<Utc>>)> = sqlx::query_as(
        "SELECT status, started_at, finished_at FROM exam_attempts a
         WHERE a.user_id = $1 AND a.track_id = $2 AND (a.purpose = $3 OR a.status IN ('in_progress', 'pending_review'))
           AND a.started_at > COALESCE((SELECT max(started_at) FROM exam_attempts p
                WHERE p.user_id = $1 AND p.track_id = $2 AND p.status = 'passed'), '-infinity')
         ORDER BY started_at",
    )
    .bind(user_id)
    .bind(track_id)
    .bind(purpose.as_str())
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(status, started_at, finished_at)| PastAttempt { status, started_at, finished_at })
        .collect())
}

async fn credits(db: &PgPool, user_id: Uuid, track_id: Uuid) -> AppResult<i64> {
    Ok(sqlx::query_scalar(
        "SELECT count(*) FROM attempt_credits WHERE user_id = $1 AND track_id = $2 AND consumed_by IS NULL",
    )
    .bind(user_id)
    .bind(track_id)
    .fetch_one(db)
    .await?)
}

/// Checks that each section has enough content (bank at least `bank_factor` times the draw).
pub async fn content_blockers(db: &PgPool, track: &Track, def: &ExamDefinition) -> AppResult<Vec<String>> {
    let mut out = Vec::new();
    for s in &def.sections {
        let (available, needed): (i64, i64) = match &s.source {
            SectionSource::Questions { pool, question_count } => (
                sqlx::query_scalar("SELECT count(*) FROM questions WHERE track_id = $1 AND pool = $2 AND active")
                    .bind(track.id)
                    .bind(pool.as_str())
                    .fetch_one(db)
                    .await?,
                i64::from(*question_count) * i64::from(def.bank_factor.max(1)),
            ),
            SectionSource::CaseStudy { scenario_count } => (
                sqlx::query_scalar(
                    "SELECT count(*) FROM scenarios s WHERE s.track_id = $1 AND s.exam_only
                     AND EXISTS (SELECT 1 FROM questions q WHERE q.scenario_id = s.id AND q.pool = 'case' AND q.active)",
                )
                .bind(track.id)
                .fetch_one(db)
                .await?,
                // Case studies must differ between attempts: at least two sets.
                i64::from(*scenario_count) * 2,
            ),
        };
        if available < needed {
            out.push(format!("exam_not_ready:{}:{available}/{needed}", s.title));
        }
    }
    Ok(out)
}

pub async fn exam_status(state: &AppState, user: &CurrentUser, track: &Track) -> AppResult<ExamStatus> {
    let db = &state.db;
    let now = Utc::now();
    let cert = certs::current(db, user.id, track.id).await?;
    let window = Duration::days(state.config.alerts.recert_window_days);
    let mut recert_opens_at = None;
    let purpose = match &cert {
        Some(c) if c.status(now) == "valid" => match c.expires_at {
            Some(exp) => {
                recert_opens_at = Some(exp - window);
                (now >= exp - window).then_some(Purpose::Recertification)
            }
            None => None,
        },
        _ => Some(Purpose::Certification),
    };
    let provisional = cert.as_ref().is_some_and(|c| c.provisional);
    let def = definition_for(track, purpose.unwrap_or(Purpose::Certification), provisional)?;

    let mut blockers = Vec::new();
    if purpose == Some(Purpose::Certification) {
        if let Some(pre) = &track.prerequisite_slug
            && !certs::has_valid(db, user.id, pre).await?
        {
            blockers.push(format!("prerequisite:{pre}"));
        }
        let modules = catalog::module_progress(db, track.id, Some(user.id)).await?;
        let missing = modules.iter().filter(|m| !m.completed).count();
        if missing > 0 {
            blockers.push(format!("modules:{missing}"));
        }
        if track.scenarios_required {
            let scenarios = catalog::scenario_progress(db, track.id, Some(user.id)).await?;
            let missing = scenarios.iter().filter(|s| !s.completed).count();
            if missing > 0 {
                blockers.push(format!("scenarios:{missing}"));
            }
        }
    }
    if purpose.is_some() {
        blockers.extend(content_blockers(db, track, &def).await?);
    }

    let attempts = match purpose {
        Some(p) => past_attempts(db, user.id, track.id, p).await?,
        None => Vec::new(),
    };
    let credits = credits(db, user.id, track.id).await?;
    let eligibility = purpose.map(|_| {
        exam::check_eligibility(&def, &attempts, credits as usize, now).map(|e| match e {
            Eligibility::Free => "free",
            Eligibility::UsesCredit => "uses_credit",
        })
    });
    let active_attempt: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM exam_attempts WHERE user_id = $1 AND track_id = $2 AND status = 'in_progress'",
    )
    .bind(user.id)
    .bind(track.id)
    .fetch_optional(db)
    .await?;
    Ok(ExamStatus {
        purpose,
        sections: summarize(&def),
        blockers,
        eligibility,
        active_attempt,
        recert_opens_at,
        attempts_used: attempts.iter().filter(|a| a.status != "in_progress").count(),
        free_attempts: def.free_attempts,
        credits,
    })
}

pub async fn start(state: &AppState, user: &CurrentUser, track: &Track) -> AppResult<Uuid> {
    let status = exam_status(state, user, track).await?;
    let purpose = status
        .purpose
        .ok_or_else(|| AppError::conflict("already_certified", "you already hold a valid certification"))?;
    if !status.blockers.is_empty() {
        return Err(AppError::Conflict {
            code: "exam_locked",
            message: "prerequisites not met".into(),
            details: Some(json!(status.blockers)),
        });
    }
    let eligibility = match status.eligibility {
        Some(Ok(e)) => e,
        Some(Err(reason)) => {
            return Err(AppError::Conflict {
                code: "not_eligible",
                message: "attempt not allowed now".into(),
                details: Some(serde_json::to_value(reason).map_err(anyhow::Error::from)?),
            });
        }
        None => return Err(AppError::conflict("not_eligible", "attempt not allowed")),
    };
    let cert = certs::current(&state.db, user.id, track.id).await?;
    let def = definition_for(track, purpose, cert.as_ref().is_some_and(|c| c.provisional))?;
    let paper = draw(&state.db, track, &def, user.id).await?;
    let first = paper.sections.first().ok_or_else(|| AppError::conflict("exam_not_ready", "empty exam"))?;
    let deadline = Utc::now() + Duration::minutes(i64::from(first.duration_minutes));

    let mut tx = state.db.begin().await?;
    let id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO exam_attempts (id, user_id, track_id, purpose, status, paper, section_deadline, paid)
         VALUES ($1, $2, $3, $4, 'in_progress', $5, $6, $7)",
    )
    .bind(id)
    .bind(user.id)
    .bind(track.id)
    .bind(purpose.as_str())
    .bind(serde_json::to_value(&paper).map_err(anyhow::Error::from)?)
    .bind(deadline)
    .bind(eligibility == "uses_credit")
    .execute(&mut *tx)
    .await;
    match inserted {
        Err(sqlx::Error::Database(e)) if e.is_unique_violation() => {
            return Err(AppError::conflict("attempt_in_progress", "another exam is already in progress"));
        }
        other => {
            other?;
        }
    }
    if eligibility == "uses_credit" {
        let consumed = sqlx::query(
            "UPDATE attempt_credits SET consumed_by = $1 WHERE id = (
                SELECT id FROM attempt_credits WHERE user_id = $2 AND track_id = $3 AND consumed_by IS NULL
                ORDER BY created_at LIMIT 1 FOR UPDATE SKIP LOCKED)",
        )
        .bind(id)
        .bind(user.id)
        .bind(track.id)
        .execute(&mut *tx)
        .await?;
        if consumed.rows_affected() != 1 {
            return Err(AppError::conflict("not_eligible", "no attempt credit left"));
        }
    }
    audit::log(
        &mut *tx,
        Some(user.id),
        "exam.start",
        Some(id.to_string()),
        json!({ "track": track.slug, "purpose": purpose.as_str() }),
    )
    .await?;
    tx.commit().await?;
    Ok(id)
}

async fn draw(db: &PgPool, track: &Track, def: &ExamDefinition, user_id: Uuid) -> AppResult<Paper> {
    let rows: Vec<(Uuid, String, Option<Uuid>, Value)> = sqlx::query_as(
        "SELECT id, pool, scenario_id, choices FROM questions
         WHERE track_id = $1 AND active AND pool IN ('exam', 'recert', 'case')",
    )
    .bind(track.id)
    .fetch_all(db)
    .await?;
    let mut pools: HashMap<QuestionPool, Vec<CandidateQuestion>> = HashMap::new();
    let mut case_questions = Vec::new();
    for (id, pool, scenario_id, choices) in rows {
        let choice_ids = choice_ids(&choices);
        let c = CandidateQuestion { id, choice_ids, scenario_id };
        match pool.as_str() {
            "exam" => pools.entry(QuestionPool::Exam).or_default().push(c),
            "recert" => pools.entry(QuestionPool::Recert).or_default().push(c),
            _ => case_questions.push(c),
        }
    }
    let case_scenarios: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM scenarios WHERE track_id = $1 AND exam_only")
        .bind(track.id)
        .fetch_all(db)
        .await?;
    let previous: Option<Value> = sqlx::query_scalar(
        "SELECT paper FROM exam_attempts WHERE user_id = $1 AND track_id = $2 ORDER BY started_at DESC LIMIT 1",
    )
    .bind(user_id)
    .bind(track.id)
    .fetch_optional(db)
    .await?;
    let avoid: HashSet<Uuid> = previous
        .and_then(|p| serde_json::from_value::<Paper>(p).ok())
        .map(|p| p.sections.into_iter().flat_map(|s| s.scenarios).collect())
        .unwrap_or_default();
    let mut rng = rand::rng();
    exam::draw_paper(def, &pools, &case_scenarios, &case_questions, &avoid, &mut rng)
        .map_err(|e| AppError::conflict("exam_not_ready", e.to_string()))
}

pub fn choice_ids(choices: &Value) -> Vec<String> {
    choices
        .as_array()
        .map(|a| a.iter().filter_map(|c| c["id"].as_str().map(String::from)).collect())
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// Taking the exam
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AttemptRow {
    pub id: Uuid,
    pub user_id: Uuid,
    pub track_id: Uuid,
    pub purpose: String,
    pub status: String,
    pub paper: Value,
    pub answers: Value,
    pub current_section: i32,
    pub section_deadline: DateTime<Utc>,
    pub results: Value,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub review: Option<Value>,
}

impl AttemptRow {
    pub fn paper(&self) -> AppResult<Paper> {
        serde_json::from_value(self.paper.clone()).map_err(|e| AppError::Internal(e.into()))
    }

    pub fn answers(&self) -> BTreeMap<Uuid, Answer> {
        serde_json::from_value(self.answers.clone()).unwrap_or_default()
    }

    pub fn results(&self) -> Vec<SectionResult> {
        serde_json::from_value(self.results.clone()).unwrap_or_default()
    }
}

pub async fn load_attempt(db: &PgPool, id: Uuid) -> AppResult<AttemptRow> {
    Ok(sqlx::query_as(
        "SELECT id, user_id, track_id, purpose, status, paper, answers, current_section, section_deadline, results,
            started_at, finished_at, review FROM exam_attempts WHERE id = $1",
    )
    .bind(id)
    .fetch_one(db)
    .await?)
}

/// Loads the learner's own attempt, closing the current section first if its time is up.
pub async fn own_attempt(state: &AppState, user: &CurrentUser, id: Uuid) -> AppResult<AttemptRow> {
    let attempt = load_attempt(&state.db, id).await?;
    if attempt.user_id != user.id {
        return Err(AppError::NotFound);
    }
    if attempt.status == "in_progress" && Utc::now() > attempt.section_deadline + Duration::seconds(GRACE_SECONDS) {
        submit_section(state, id, attempt.current_section).await?;
        return load_attempt(&state.db, id).await;
    }
    Ok(attempt)
}

pub async fn save_answer(
    state: &AppState,
    user: &CurrentUser,
    id: Uuid,
    question_id: Uuid,
    answer: Answer,
) -> AppResult<()> {
    let attempt = own_attempt(state, user, id).await?;
    if attempt.status != "in_progress" {
        return Err(AppError::conflict("attempt_closed", "this attempt is closed"));
    }
    let paper = attempt.paper()?;
    let section = &paper.sections[attempt.current_section as usize];
    let item = section
        .items
        .iter()
        .find(|i| i.question_id == question_id)
        .ok_or_else(|| AppError::bad_request("not_in_section", "question not in the current section"))?;
    match &answer {
        Answer::Choices(c) => {
            if c.len() > item.choice_order.len() || !c.iter().all(|x| item.choice_order.contains(x)) {
                return Err(AppError::bad_request("invalid_answer", "unknown choice"));
            }
        }
        Answer::Text(t) => {
            if !item.choice_order.is_empty() || t.len() > 20_000 {
                return Err(AppError::bad_request("invalid_answer", "invalid written answer"));
            }
        }
    }
    let updated = sqlx::query(
        "UPDATE exam_attempts SET answers = jsonb_set(answers, ARRAY[$2::text], $3)
         WHERE id = $1 AND status = 'in_progress' AND current_section = $4",
    )
    .bind(id)
    .bind(question_id.to_string())
    .bind(serde_json::to_value(&answer).map_err(anyhow::Error::from)?)
    .bind(attempt.current_section)
    .execute(&state.db)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::conflict("attempt_closed", "this section is closed"));
    }
    Ok(())
}

pub async fn answer_keys(db: &PgPool, ids: &[Uuid]) -> AppResult<HashMap<Uuid, AnswerKey>> {
    let rows: Vec<(Uuid, String, Value)> =
        sqlx::query_as("SELECT id, format, choices FROM questions WHERE id = ANY($1)").bind(ids).fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|(id, format, choices)| {
            let correct = choices
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter(|c| c["correct"].as_bool() == Some(true))
                        .filter_map(|c| c["id"].as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            (id, AnswerKey { written: format == "written", correct })
        })
        .collect())
}

/// Grades the given section (idempotent: does nothing if the attempt moved on).
pub async fn submit_section(state: &AppState, id: Uuid, section_index: i32) -> AppResult<()> {
    let mut tx = state.db.begin().await?;
    let attempt: AttemptRow = sqlx::query_as(
        "SELECT id, user_id, track_id, purpose, status, paper, answers, current_section, section_deadline, results,
            started_at, finished_at, review FROM exam_attempts WHERE id = $1 FOR UPDATE",
    )
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    if attempt.status != "in_progress" || attempt.current_section != section_index {
        return Ok(());
    }
    let paper = attempt.paper()?;
    let section = &paper.sections[section_index as usize];
    let ids: Vec<Uuid> = section.items.iter().map(|i| i.question_id).collect();
    let keys = answer_keys(&state.db, &ids).await?;
    let answers = attempt.answers();
    let result = exam::grade_section(section, &keys, &answers);

    for item in &section.items {
        if let Some(key) = keys.get(&item.question_id).filter(|k| !k.written) {
            let ok = exam::is_correct(key, answers.get(&item.question_id));
            sqlx::query(
                "INSERT INTO question_stats (question_id, answered, correct) VALUES ($1, 1, $2)
                 ON CONFLICT (question_id) DO UPDATE SET answered = question_stats.answered + 1,
                    correct = question_stats.correct + $2",
            )
            .bind(item.question_id)
            .bind(i64::from(ok))
            .execute(&mut *tx)
            .await?;
        }
    }

    let mut results = attempt.results();
    results.push(result.clone());
    let last = section_index as usize + 1 >= paper.sections.len();
    let failed = result.passed == Some(false);
    if last || failed {
        let status = exam::overall_status(&results);
        sqlx::query("UPDATE exam_attempts SET status = $2, results = $3, finished_at = now() WHERE id = $1")
            .bind(id)
            .bind(status)
            .bind(serde_json::to_value(&results).map_err(anyhow::Error::from)?)
            .execute(&mut *tx)
            .await?;
        let track = catalog::load_track_by_id(&state.db, attempt.track_id).await?;
        finalize(state, &mut tx, &attempt, &track, status).await?;
    } else {
        let next = &paper.sections[section_index as usize + 1];
        sqlx::query(
            "UPDATE exam_attempts SET results = $2, current_section = current_section + 1, section_deadline = $3 WHERE id = $1",
        )
        .bind(id)
        .bind(serde_json::to_value(&results).map_err(anyhow::Error::from)?)
        .bind(Utc::now() + Duration::minutes(i64::from(next.duration_minutes)))
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Issues the certification or notifies the learner, inside the grading transaction.
pub async fn finalize(
    state: &AppState,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    attempt: &AttemptRow,
    track: &Track,
    status: &str,
) -> AppResult<()> {
    let (email, name): (String, String) = sqlx::query_as("SELECT email, display_name FROM users WHERE id = $1")
        .bind(attempt.user_id)
        .fetch_one(&mut **tx)
        .await?;
    let date_fmt = state.renderer.raw("date.format").to_string();
    let track_link = state.config.public_url(&format!("/tracks/{}", track.slug));
    audit::log(
        &mut **tx,
        None,
        "exam.result",
        Some(attempt.id.to_string()),
        json!({ "user": attempt.user_id, "track": track.slug, "status": status }),
    )
    .await?;
    match status {
        "passed" => {
            let purpose =
                if attempt.purpose == "recertification" { Purpose::Recertification } else { Purpose::Certification };
            let previous: Option<(Uuid, Option<DateTime<Utc>>, bool)> = sqlx::query_as(crate::const_sql!(
                "SELECT c.id, c.expires_at, c.provisional FROM certifications c
                 WHERE c.user_id = $1 AND c.track_id = $2 AND {} ORDER BY c.issued_at DESC LIMIT 1",
                certs::VALID
            ))
            .bind(attempt.user_id)
            .bind(track.id)
            .fetch_optional(&mut **tx)
            .await?;
            let def = definition_for(track, purpose, previous.as_ref().is_some_and(|p| p.2))?;
            let renewal = previous.map(|(id, exp, _)| (id, exp));
            let (cert_id, expires) = certs::issue(
                tx,
                attempt.user_id,
                track.id,
                attempt.id,
                track.validity_months,
                state.config.instance.product_major_version.as_deref(),
                def.provisional,
                renewal,
            )
            .await?;
            sqlx::query("UPDATE enrollments SET completed_at = COALESCE(completed_at, now()) WHERE user_id = $1 AND track_id = $2")
                .bind(attempt.user_id)
                .bind(track.id)
                .execute(&mut **tx)
                .await?;
            mail::queue_template(
                &mut **tx,
                state,
                &email,
                "exam_passed",
                json!({
                    "name": name,
                    "track": track.title,
                    "expires": expires.map(|e| e.format(&date_fmt).to_string()),
                    "certificate_link": state.config.public_url(&format!("/api/certifications/{cert_id}/certificate.pdf")),
                    "verify_link": state.config.public_url(&format!("/verify/{cert_id}")),
                }),
            )
            .await?;
        }
        "failed" => {
            let def = track.exam_def()?;
            let used: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM exam_attempts a WHERE a.user_id = $1 AND a.track_id = $2 AND a.purpose = $3
                   AND a.status = 'failed'
                   AND a.started_at > COALESCE((SELECT max(started_at) FROM exam_attempts p
                        WHERE p.user_id = $1 AND p.track_id = $2 AND p.status = 'passed'), '-infinity')",
            )
            .bind(attempt.user_id)
            .bind(track.id)
            .bind(&attempt.purpose)
            .fetch_one(&mut **tx)
            .await?;
            let cooldown_applies = def.free_attempts.is_some_and(|f| used >= i64::from(f)) && def.cooldown_days > 0;
            let retry_at = cooldown_applies.then(|| Utc::now() + Duration::days(i64::from(def.cooldown_days)));
            mail::queue_template(
                &mut **tx,
                state,
                &email,
                "exam_failed",
                json!({ "name": name, "track": track.title, "link": track_link, "retry_at": retry_at.map(|d| d.format(&date_fmt).to_string()) }),
            )
            .await?;
        }
        _ => {
            mail::queue_template(
                &mut **tx,
                state,
                &email,
                "exam_review",
                json!({ "name": name, "track": track.title }),
            )
            .await?;
        }
    }
    Ok(())
}

/// Closes sections whose time ran out (learner closed the tab, lost connection...).
pub async fn close_overdue(state: &AppState) -> anyhow::Result<usize> {
    let overdue: Vec<(Uuid, i32)> = sqlx::query_as(
        "SELECT id, current_section FROM exam_attempts
         WHERE status = 'in_progress' AND section_deadline < now() - make_interval(secs => $1)",
    )
    .bind(GRACE_SECONDS as f64)
    .fetch_all(&state.db)
    .await?;
    for (id, section) in &overdue {
        if let Err(e) = submit_section(state, *id, *section).await {
            tracing::error!(attempt = %id, error = ?e, "failed to close overdue attempt");
        }
    }
    Ok(overdue.len())
}
