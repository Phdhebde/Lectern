//! Exam definitions, paper drawing, eligibility and grading.
//!
//! Everything here is pure (no I/O) so the rules that decide who may sit an exam and
//! whether they pass are unit-tested in isolation.

use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use rand::seq::SliceRandom;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Exam definition stored on a track (`tracks.exam` / `tracks.recert_exam`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ExamDefinition {
    pub sections: Vec<SectionDefinition>,
    /// Attempts included before extra attempts must be granted or paid. `None` = unlimited.
    #[serde(default)]
    pub free_attempts: Option<u32>,
    /// Waiting period between attempts once the free attempts are used.
    #[serde(default)]
    pub cooldown_days: u32,
    /// Minimum bank size relative to the number of questions drawn (integrity rule).
    #[serde(default = "default_bank_factor")]
    pub bank_factor: u32,
    /// Certifications issued with this definition are provisional (e.g. a transition
    /// period with a reduced exam): their holders take the full exam at recertification.
    #[serde(default)]
    pub provisional: bool,
}

fn default_bank_factor() -> u32 {
    3
}

// No `deny_unknown_fields` here: serde does not support it together with `flatten`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionDefinition {
    pub title: String,
    #[serde(flatten)]
    pub source: SectionSource,
    pub duration_minutes: u32,
    /// Passing threshold in percent. Ignored for sections graded by an evaluator.
    pub pass_percent: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SectionSource {
    /// Questions drawn at random from a pool of the track (`exam` or `recert`).
    Questions { pool: QuestionPool, question_count: u32 },
    /// Exam-only scenarios drawn at random, with all their case questions.
    CaseStudy { scenario_count: u32 },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum QuestionPool {
    Exam,
    Recert,
}

impl QuestionPool {
    pub fn as_str(self) -> &'static str {
        match self {
            QuestionPool::Exam => "exam",
            QuestionPool::Recert => "recert",
        }
    }
}

impl ExamDefinition {
    pub fn validate(&self) -> Result<(), String> {
        if self.sections.is_empty() {
            return Err("an exam needs at least one section".into());
        }
        for s in &self.sections {
            if s.duration_minutes == 0 {
                return Err(format!("section {:?}: duration must be positive", s.title));
            }
            if s.pass_percent > 100 {
                return Err(format!("section {:?}: pass_percent must be ≤ 100", s.title));
            }
            match s.source {
                SectionSource::Questions { question_count: 0, .. } => {
                    return Err(format!("section {:?}: question_count must be positive", s.title));
                }
                SectionSource::CaseStudy { scenario_count: 0 } => {
                    return Err(format!("section {:?}: scenario_count must be positive", s.title));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Paper
// ---------------------------------------------------------------------------

/// The frozen exam paper of one attempt, stored in `exam_attempts.paper`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Paper {
    pub sections: Vec<PaperSection>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaperSection {
    pub title: String,
    pub duration_minutes: u32,
    pub pass_percent: u32,
    #[serde(default)]
    pub scenarios: Vec<Uuid>,
    pub items: Vec<PaperItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaperItem {
    pub question_id: Uuid,
    /// Choice ids in the order they are displayed (shuffled per attempt).
    pub choice_order: Vec<String>,
}

/// A question as seen by the paper builder.
#[derive(Debug, Clone)]
pub struct CandidateQuestion {
    pub id: Uuid,
    pub choice_ids: Vec<String>,
    pub scenario_id: Option<Uuid>,
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PaperError {
    #[error("section {section:?} needs {needed} items but only {available} are available")]
    NotEnoughContent { section: String, needed: usize, available: usize },
}

/// Draws a paper. `avoid_scenarios` holds the case studies of the learner's previous
/// attempt: they are only reused when there are not enough other scenarios.
pub fn draw_paper(
    def: &ExamDefinition,
    pools: &HashMap<QuestionPool, Vec<CandidateQuestion>>,
    case_scenarios: &[Uuid],
    case_questions: &[CandidateQuestion],
    avoid_scenarios: &HashSet<Uuid>,
    rng: &mut impl rand::Rng,
) -> Result<Paper, PaperError> {
    let mut sections = Vec::with_capacity(def.sections.len());
    let mut used_scenarios: HashSet<Uuid> = HashSet::new();
    for s in &def.sections {
        let (items, scenarios) = match &s.source {
            SectionSource::Questions { pool, question_count } => {
                let mut candidates: Vec<&CandidateQuestion> =
                    pools.get(pool).map(|v| v.iter().collect()).unwrap_or_default();
                let needed = *question_count as usize;
                if candidates.len() < needed {
                    return Err(PaperError::NotEnoughContent {
                        section: s.title.clone(),
                        needed,
                        available: candidates.len(),
                    });
                }
                candidates.shuffle(rng);
                let items = candidates[..needed].iter().map(|q| shuffled_item(q, rng)).collect();
                (items, Vec::new())
            }
            SectionSource::CaseStudy { scenario_count } => {
                let needed = *scenario_count as usize;
                let mut fresh: Vec<Uuid> = case_scenarios
                    .iter()
                    .copied()
                    .filter(|id| !avoid_scenarios.contains(id) && !used_scenarios.contains(id))
                    .collect();
                let mut reused: Vec<Uuid> = case_scenarios
                    .iter()
                    .copied()
                    .filter(|id| avoid_scenarios.contains(id) && !used_scenarios.contains(id))
                    .collect();
                if fresh.len() + reused.len() < needed {
                    return Err(PaperError::NotEnoughContent {
                        section: s.title.clone(),
                        needed,
                        available: fresh.len() + reused.len(),
                    });
                }
                fresh.shuffle(rng);
                reused.shuffle(rng);
                fresh.extend(reused);
                let picked: Vec<Uuid> = fresh.into_iter().take(needed).collect();
                used_scenarios.extend(picked.iter().copied());
                let items = picked
                    .iter()
                    .flat_map(|sid| case_questions.iter().filter(move |q| q.scenario_id == Some(*sid)))
                    .map(|q| shuffled_item(q, rng))
                    .collect();
                (items, picked)
            }
        };
        sections.push(PaperSection {
            title: s.title.clone(),
            duration_minutes: s.duration_minutes,
            pass_percent: s.pass_percent,
            scenarios,
            items,
        });
    }
    Ok(Paper { sections })
}

fn shuffled_item(q: &CandidateQuestion, rng: &mut impl rand::Rng) -> PaperItem {
    let mut order = q.choice_ids.clone();
    order.shuffle(rng);
    PaperItem { question_id: q.id, choice_order: order }
}

// ---------------------------------------------------------------------------
// Eligibility
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct PastAttempt {
    pub status: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Eligibility {
    /// Within the free allowance.
    Free,
    /// Allowed by consuming an attempt credit.
    UsesCredit,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub enum Ineligible {
    AttemptInProgress,
    AwaitingReview,
    Cooldown { retry_at: DateTime<Utc> },
    NoAttemptLeft,
}

/// Decides whether a new attempt may start.
///
/// `attempts` are the learner's attempts for this track and purpose since their last
/// successful one (oldest first). The first `free_attempts` are free; every further
/// attempt needs a credit and must respect the cooldown after the previous attempt.
pub fn check_eligibility(
    def: &ExamDefinition,
    attempts: &[PastAttempt],
    available_credits: usize,
    now: DateTime<Utc>,
) -> Result<Eligibility, Ineligible> {
    if attempts.iter().any(|a| a.status == "in_progress") {
        return Err(Ineligible::AttemptInProgress);
    }
    if attempts.iter().any(|a| a.status == "pending_review") {
        return Err(Ineligible::AwaitingReview);
    }
    let Some(free) = def.free_attempts else {
        return Ok(Eligibility::Free);
    };
    if (attempts.len() as u32) < free {
        return Ok(Eligibility::Free);
    }
    if let Some(last) = attempts.last() {
        let ended = last.finished_at.unwrap_or(last.started_at);
        let retry_at = ended + Duration::days(i64::from(def.cooldown_days));
        if now < retry_at {
            return Err(Ineligible::Cooldown { retry_at });
        }
    }
    if available_credits == 0 {
        return Err(Ineligible::NoAttemptLeft);
    }
    Ok(Eligibility::UsesCredit)
}

// ---------------------------------------------------------------------------
// Grading
// ---------------------------------------------------------------------------

/// A learner's answer: selected choice ids, or free text for written questions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Answer {
    Choices(Vec<String>),
    Text(String),
}

#[derive(Debug, Clone)]
pub struct AnswerKey {
    pub written: bool,
    pub correct: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SectionResult {
    /// Percentage of correct choice questions (0-100). `None` when the section only has
    /// written questions.
    pub score: Option<u32>,
    pub correct: u32,
    pub total: u32,
    /// Written answers waiting for an evaluator.
    pub needs_review: bool,
    pub passed: Option<bool>,
}

/// Exact-match grading: a choice question is right when the selected set equals the
/// set of correct choices. Unanswered questions count as wrong.
pub fn is_correct(key: &AnswerKey, answer: Option<&Answer>) -> bool {
    match answer {
        Some(Answer::Choices(selected)) => {
            let selected: HashSet<&String> = selected.iter().collect();
            !key.correct.is_empty()
                && selected.len() == key.correct.len()
                && key.correct.iter().all(|c| selected.contains(c))
        }
        _ => false,
    }
}

pub fn grade_section(
    section: &PaperSection,
    keys: &HashMap<Uuid, AnswerKey>,
    answers: &BTreeMap<Uuid, Answer>,
) -> SectionResult {
    let mut correct = 0u32;
    let mut total = 0u32;
    let mut needs_review = false;
    for item in &section.items {
        let Some(key) = keys.get(&item.question_id) else { continue };
        if key.written {
            needs_review = true;
            continue;
        }
        total += 1;
        if is_correct(key, answers.get(&item.question_id)) {
            correct += 1;
        }
    }
    let score = (total > 0).then(|| percent(correct, total));
    let passed = if needs_review { None } else { Some(score.unwrap_or(0) >= section.pass_percent) };
    SectionResult { score, correct, total, needs_review, passed }
}

pub fn percent(correct: u32, total: u32) -> u32 {
    if total == 0 {
        return 0;
    }
    // Floor so that 74.9 % never rounds up to a 75 % threshold.
    correct * 100 / total
}

/// Overall outcome once every section is graded.
pub fn overall_status(results: &[SectionResult]) -> &'static str {
    if results.iter().any(|r| r.passed == Some(false)) {
        "failed"
    } else if results.iter().any(|r| r.needs_review) {
        "pending_review"
    } else {
        "passed"
    }
}

/// Expiry of a new certification: validity period, capped by nothing else.
pub fn expiry_date(issued: DateTime<Utc>, validity_months: Option<i32>) -> Option<DateTime<Utc>> {
    let months = u32::try_from(validity_months?).ok()?;
    issued.checked_add_months(chrono::Months::new(months))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rand::SeedableRng;

    fn def(free: Option<u32>, cooldown: u32) -> ExamDefinition {
        ExamDefinition {
            sections: vec![SectionDefinition {
                title: "QCM".into(),
                source: SectionSource::Questions { pool: QuestionPool::Exam, question_count: 2 },
                duration_minutes: 10,
                pass_percent: 75,
            }],
            free_attempts: free,
            cooldown_days: cooldown,
            bank_factor: 3,
            provisional: false,
        }
    }

    fn at(day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 1, day, 12, 0, 0).unwrap()
    }

    fn failed(day: u32) -> PastAttempt {
        PastAttempt { status: "failed".into(), started_at: at(day), finished_at: Some(at(day)) }
    }

    #[test]
    fn definition_json_roundtrip() {
        let json = r#"{
            "sections": [
                {"title": "QCM", "kind": "questions", "pool": "exam", "question_count": 40, "duration_minutes": 60, "pass_percent": 75},
                {"title": "Case study", "kind": "case_study", "scenario_count": 2, "duration_minutes": 90, "pass_percent": 75}
            ],
            "free_attempts": 2,
            "cooldown_days": 14
        }"#;
        let d: ExamDefinition = serde_json::from_str(json).unwrap();
        assert_eq!(d.sections.len(), 2);
        assert_eq!(d.bank_factor, 3);
        assert!(matches!(d.sections[1].source, SectionSource::CaseStudy { scenario_count: 2 }));
        d.validate().unwrap();
        let back: ExamDefinition = serde_json::from_value(serde_json::to_value(&d).unwrap()).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn unlimited_attempts_are_always_free() {
        let attempts: Vec<_> = (1..10).map(failed).collect();
        assert_eq!(check_eligibility(&def(None, 0), &attempts, 0, at(10)), Ok(Eligibility::Free));
    }

    #[test]
    fn free_attempts_then_cooldown_then_credit() {
        let d = def(Some(2), 7);
        assert_eq!(check_eligibility(&d, &[], 0, at(1)), Ok(Eligibility::Free));
        assert_eq!(check_eligibility(&d, &[failed(1)], 0, at(1)), Ok(Eligibility::Free));
        let two = [failed(1), failed(2)];
        assert_eq!(
            check_eligibility(&d, &two, 1, at(5)),
            Err(Ineligible::Cooldown { retry_at: at(9) })
        );
        assert_eq!(check_eligibility(&d, &two, 0, at(9)), Err(Ineligible::NoAttemptLeft));
        assert_eq!(check_eligibility(&d, &two, 1, at(9)), Ok(Eligibility::UsesCredit));
    }

    #[test]
    fn in_progress_or_pending_blocks() {
        let d = def(None, 0);
        let active = PastAttempt { status: "in_progress".into(), started_at: at(1), finished_at: None };
        assert_eq!(check_eligibility(&d, &[active], 0, at(1)), Err(Ineligible::AttemptInProgress));
        let pending = PastAttempt { status: "pending_review".into(), started_at: at(1), finished_at: Some(at(1)) };
        assert_eq!(check_eligibility(&d, &[pending], 0, at(1)), Err(Ineligible::AwaitingReview));
    }

    fn key(correct: &[&str]) -> AnswerKey {
        AnswerKey { written: false, correct: correct.iter().map(|s| s.to_string()).collect() }
    }

    #[test]
    fn exact_match_grading() {
        let k = key(&["a", "c"]);
        let ans = |v: &[&str]| Answer::Choices(v.iter().map(|s| s.to_string()).collect());
        assert!(is_correct(&k, Some(&ans(&["c", "a"]))));
        assert!(!is_correct(&k, Some(&ans(&["a"]))));
        assert!(!is_correct(&k, Some(&ans(&["a", "b", "c"]))));
        assert!(!is_correct(&k, None));
        assert!(!is_correct(&k, Some(&Answer::Text("a".into()))));
    }

    #[test]
    fn threshold_is_not_rounded_up() {
        assert_eq!(percent(299, 400), 74);
        assert_eq!(percent(3, 4), 75);
    }

    #[test]
    fn grading_and_overall_status() {
        let q1 = Uuid::new_v4();
        let q2 = Uuid::new_v4();
        let w = Uuid::new_v4();
        let section = PaperSection {
            title: "s".into(),
            duration_minutes: 10,
            pass_percent: 50,
            scenarios: vec![],
            items: [q1, q2]
                .iter()
                .map(|id| PaperItem { question_id: *id, choice_order: vec![] })
                .collect(),
        };
        let mut keys = HashMap::new();
        keys.insert(q1, key(&["a"]));
        keys.insert(q2, key(&["b"]));
        let mut answers = BTreeMap::new();
        answers.insert(q1, Answer::Choices(vec!["a".into()]));
        let r = grade_section(&section, &keys, &answers);
        assert_eq!((r.correct, r.total, r.score, r.passed), (1, 2, Some(50), Some(true)));
        assert_eq!(overall_status(std::slice::from_ref(&r)), "passed");

        let mut written = section.clone();
        written.items.push(PaperItem { question_id: w, choice_order: vec![] });
        keys.insert(w, AnswerKey { written: true, correct: HashSet::new() });
        let r2 = grade_section(&written, &keys, &answers);
        assert!(r2.needs_review);
        assert_eq!(overall_status(&[r.clone(), r2]), "pending_review");

        let failing = SectionResult { score: Some(10), correct: 1, total: 10, needs_review: false, passed: Some(false) };
        assert_eq!(overall_status(&[r, failing]), "failed");
    }

    #[test]
    fn paper_draw_respects_counts_and_avoids_previous_case_studies() {
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let bank: Vec<CandidateQuestion> = (0..6)
            .map(|_| CandidateQuestion {
                id: Uuid::new_v4(),
                choice_ids: vec!["a".into(), "b".into(), "c".into()],
                scenario_id: None,
            })
            .collect();
        let scenarios: Vec<Uuid> = (0..3).map(|_| Uuid::new_v4()).collect();
        let case_questions: Vec<CandidateQuestion> = scenarios
            .iter()
            .map(|s| CandidateQuestion { id: Uuid::new_v4(), choice_ids: vec!["a".into()], scenario_id: Some(*s) })
            .collect();
        let mut d = def(None, 0);
        d.sections.push(SectionDefinition {
            title: "Case".into(),
            source: SectionSource::CaseStudy { scenario_count: 2 },
            duration_minutes: 10,
            pass_percent: 75,
        });
        let pools = HashMap::from([(QuestionPool::Exam, bank)]);
        let avoid: HashSet<Uuid> = [scenarios[0]].into();
        let paper = draw_paper(&d, &pools, &scenarios, &case_questions, &avoid, &mut rng).unwrap();
        assert_eq!(paper.sections[0].items.len(), 2);
        let ids: HashSet<_> = paper.sections[0].items.iter().map(|i| i.question_id).collect();
        assert_eq!(ids.len(), 2, "no duplicates");
        assert_eq!(paper.sections[1].scenarios.len(), 2);
        assert!(!paper.sections[1].scenarios.contains(&scenarios[0]));
        assert_eq!(paper.sections[1].items.len(), 2);

        // Not enough content.
        let mut big = d.clone();
        big.sections[0].source = SectionSource::Questions { pool: QuestionPool::Exam, question_count: 7 };
        assert!(matches!(
            draw_paper(&big, &pools, &scenarios, &case_questions, &avoid, &mut rng),
            Err(PaperError::NotEnoughContent { needed: 7, available: 6, .. })
        ));
    }

    #[test]
    fn expiry_adds_months() {
        let issued = Utc.with_ymd_and_hms(2026, 10, 2, 0, 0, 0).unwrap();
        assert_eq!(expiry_date(issued, Some(24)), Some(Utc.with_ymd_and_hms(2028, 10, 2, 0, 0, 0).unwrap()));
        assert_eq!(expiry_date(issued, None), None);
    }
}
