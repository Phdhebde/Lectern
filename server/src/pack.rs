//! Content packs: the documented import/export format for tracks, modules, scenarios,
//! question banks and certification requirements. See `docs/content-pack.md`.
//!
//! A pack is a directory (or a zip of it):
//!
//! ```text
//! pack.toml                                  format version, requirement levels
//! tracks/<track>/track.toml                  track, badge, exam definitions
//! tracks/<track>/modules/<NN>-<module>.md    TOML front matter + recap sheet
//! tracks/<track>/files/...                   module attachments
//! tracks/<track>/questions/*.toml            question banks
//! tracks/<track>/scenarios/<scenario>/scenario.toml + screenshots
//! ```
//!
//! Imports are idempotent: entities are matched by slug (tracks, modules, scenarios)
//! or by `ref` (questions). A pack is authoritative for the tracks it contains:
//! modules and scenarios missing from it are removed, questions are deactivated
//! (kept for the history of past exam papers).

use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::credentials::BadgeDesign;
use crate::domain::exam::ExamDefinition;

pub const FORMAT_VERSION: u32 = 1;

// ---------------------------------------------------------------------------
// File model
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    pub format: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub levels: Vec<LevelFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelFile {
    pub slug: String,
    pub org_kind: String,
    pub name: String,
    pub rank: i32,
    /// track slug -> number of valid certifications required
    #[serde(default)]
    pub requirements: BTreeMap<String, i64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrackFile {
    pub title: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub description: String,
    pub audiences: Vec<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub prerequisite: Option<String>,
    #[serde(default)]
    pub prerequisites: String,
    #[serde(default)]
    pub estimated_minutes: i32,
    #[serde(default)]
    pub scenarios_required: bool,
    #[serde(default)]
    pub validity_months: Option<i32>,
    #[serde(default = "default_quiz_pass")]
    pub module_quiz_pass_percent: i32,
    #[serde(default = "default_true")]
    pub published: bool,
    #[serde(default)]
    pub badge: BadgeDesign,
    pub exam: ExamDefinition,
    #[serde(default)]
    pub recert_exam: Option<ExamDefinition>,
}

fn default_quiz_pass() -> i32 {
    70
}
fn default_true() -> bool {
    true
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleFrontMatter {
    pub title: String,
    #[serde(default)]
    pub video: Option<String>,
    #[serde(default)]
    pub captions: Option<String>,
    #[serde(default)]
    pub duration_minutes: i32,
    #[serde(default)]
    pub doc_url: Option<String>,
    #[serde(default)]
    pub attachments: Vec<AttachmentFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentFile {
    /// Path relative to the track directory.
    pub file: String,
    pub label: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionsFile {
    #[serde(default)]
    pub questions: Vec<QuestionFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QuestionFile {
    #[serde(rename = "ref")]
    pub reference: String,
    pub pool: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    #[serde(default = "default_format")]
    pub format: String,
    pub prompt: String,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub choices: Vec<ChoiceFile>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_format() -> String {
    "choice".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChoiceFile {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub text: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub correct: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioFile {
    pub title: String,
    pub kind: String,
    #[serde(default)]
    pub exam_only: bool,
    #[serde(default)]
    pub family: Option<String>,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub context: String,
    #[serde(default)]
    pub pitfalls: String,
    #[serde(default)]
    pub steps: Vec<StepFile>,
    /// Verification questions (pool "quiz") or case questions (pool "case").
    #[serde(default)]
    pub questions: Vec<QuestionFile>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepFile {
    pub action: String,
    /// Screenshot file name inside the scenario directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    #[serde(default)]
    pub alt: String,
    #[serde(default)]
    pub expected: String,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
}

/// Annotation drawn over a screenshot. Coordinates are percentages (0-100).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    #[serde(rename = "type")]
    pub kind: String,
    pub x: f64,
    pub y: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y2: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Annotation {
    pub fn validate(&self) -> anyhow::Result<()> {
        if !matches!(self.kind.as_str(), "box" | "arrow" | "marker") {
            bail!("annotation type must be box, arrow or marker");
        }
        let in_range = |v: f64| (0.0..=100.0).contains(&v);
        let coords = [Some(self.x), Some(self.y), self.w, self.h, self.x2, self.y2];
        if !coords.into_iter().flatten().all(in_range) {
            bail!("annotation coordinates are percentages between 0 and 100");
        }
        match self.kind.as_str() {
            "box" if self.w.is_none() || self.h.is_none() => bail!("box annotations need w and h"),
            "arrow" if self.x2.is_none() || self.y2.is_none() => bail!("arrow annotations need x2 and y2"),
            _ => Ok(()),
        }
    }
}

// ---------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------

/// In-memory view of a pack: relative path -> bytes.
pub struct PackFiles {
    files: BTreeMap<String, Vec<u8>>,
}

impl PackFiles {
    pub fn from_dir(root: &Path) -> anyhow::Result<Self> {
        let mut files = BTreeMap::new();
        let mut stack = vec![root.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).with_context(|| format!("reading {}", dir.display()))? {
                let entry = entry?;
                let path = entry.path();
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                if entry.file_type()?.is_dir() {
                    stack.push(path);
                } else {
                    let rel = path.strip_prefix(root)?.to_string_lossy().replace('\\', "/");
                    files.insert(rel, std::fs::read(&path)?);
                }
            }
        }
        Ok(Self { files })
    }

    pub fn from_zip(bytes: &[u8], max_total: u64) -> anyhow::Result<Self> {
        let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))?;
        let mut files = BTreeMap::new();
        let mut total = 0u64;
        for i in 0..archive.len() {
            let mut f = archive.by_index(i)?;
            if f.is_dir() {
                continue;
            }
            // enclosed_name rejects absolute paths and `..` (zip-slip).
            let Some(name) = f.enclosed_name() else { bail!("unsafe path in archive: {}", f.name()) };
            let name = name.to_string_lossy().replace('\\', "/");
            if name.split('/').any(|seg| seg.starts_with('.') || seg == "__MACOSX") {
                continue;
            }
            // Count the bytes actually inflated: sizes declared in the archive can lie.
            let mut buf = Vec::new();
            f.by_ref().take(max_total - total + 1).read_to_end(&mut buf)?;
            total += buf.len() as u64;
            if total > max_total {
                bail!("archive too large once uncompressed");
            }
            files.insert(name, buf);
        }
        // Accept archives whose content sits in a single top-level folder.
        if !files.contains_key("pack.toml") {
            let prefix = files
                .keys()
                .find(|k| k.ends_with("/pack.toml") && k.matches('/').count() == 1)
                .map(|k| k.trim_end_matches("pack.toml").to_string());
            if let Some(prefix) = prefix {
                files = files
                    .into_iter()
                    .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|s| (s.to_string(), v)))
                    .collect();
            }
        }
        Ok(Self { files })
    }

    fn text(&self, path: &str) -> anyhow::Result<&str> {
        let bytes = self.files.get(path).with_context(|| format!("missing file {path}"))?;
        std::str::from_utf8(bytes).with_context(|| format!("{path} is not UTF-8"))
    }

    fn toml<T: for<'de> Deserialize<'de>>(&self, path: &str) -> anyhow::Result<T> {
        toml::from_str(self.text(path)?).with_context(|| format!("invalid {path}"))
    }

    fn list(&self, prefix: &str) -> Vec<&str> {
        self.files.keys().filter(|k| k.starts_with(prefix)).map(String::as_str).collect()
    }
}

/// Parsed and validated pack, ready to be written to the database.
pub struct ParsedPack {
    pub manifest: PackManifest,
    pub tracks: Vec<ParsedTrack>,
}

pub struct ParsedTrack {
    pub slug: String,
    pub track: TrackFile,
    pub modules: Vec<ParsedModule>,
    pub scenarios: Vec<ParsedScenario>,
    pub questions: Vec<QuestionFile>,
}

pub struct ParsedModule {
    pub slug: String,
    pub position: i32,
    pub front: ModuleFrontMatter,
    pub body: String,
}

pub struct ParsedScenario {
    pub slug: String,
    pub dir: String,
    pub file: ScenarioFile,
}

pub fn is_slug(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 80
        && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !s.starts_with('-')
}

pub fn parse(files: &PackFiles) -> anyhow::Result<ParsedPack> {
    let manifest: PackManifest = files.toml("pack.toml")?;
    if manifest.format != FORMAT_VERSION {
        bail!("unsupported pack format {} (expected {FORMAT_VERSION})", manifest.format);
    }
    for level in &manifest.levels {
        if !is_slug(&level.slug) || !matches!(level.org_kind.as_str(), "partner" | "customer") {
            bail!("invalid level {:?}", level.slug);
        }
    }
    let mut track_slugs: Vec<String> = files
        .list("tracks/")
        .iter()
        .filter_map(|p| p.strip_prefix("tracks/")?.strip_suffix("/track.toml"))
        .filter(|s| !s.contains('/'))
        .map(String::from)
        .collect();
    track_slugs.sort();

    let mut tracks = Vec::new();
    for slug in track_slugs {
        let base = format!("tracks/{slug}");
        if !is_slug(&slug) {
            bail!("{base}: directory names must be lowercase slugs");
        }
        let track: TrackFile = files.toml(&format!("{base}/track.toml"))?;
        track.exam.validate().map_err(|e| anyhow::anyhow!("{base}: exam: {e}"))?;
        if let Some(r) = &track.recert_exam {
            r.validate().map_err(|e| anyhow::anyhow!("{base}: recert_exam: {e}"))?;
        }
        if track.audiences.is_empty()
            || !track.audiences.iter().all(|a| matches!(a.as_str(), "public" | "partner" | "customer"))
        {
            bail!("{base}: audiences must be a non-empty subset of public, partner, customer");
        }

        let mut modules = Vec::new();
        let mut module_files: Vec<&str> =
            files.list(&format!("{base}/modules/")).into_iter().filter(|p| p.ends_with(".md")).collect();
        module_files.sort();
        for (i, path) in module_files.iter().enumerate() {
            let stem = path.rsplit('/').next().unwrap_or_default().trim_end_matches(".md");
            // "01-architecture" -> "architecture"
            let slug = stem
                .split_once('-')
                .filter(|(n, _)| n.chars().all(|c| c.is_ascii_digit()))
                .map(|(_, s)| s)
                .unwrap_or(stem);
            if !is_slug(slug) {
                bail!("{path}: invalid module slug {slug:?}");
            }
            let (front, body) = split_front_matter(files.text(path)?).with_context(|| path.to_string())?;
            let front: ModuleFrontMatter = toml::from_str(front).with_context(|| format!("{path}: front matter"))?;
            for a in &front.attachments {
                files
                    .files
                    .get(&format!("{base}/{}", a.file))
                    .with_context(|| format!("{path}: missing attachment {}", a.file))?;
            }
            modules.push(ParsedModule {
                slug: slug.to_string(),
                position: i as i32 + 1,
                front,
                body: body.trim().to_string(),
            });
        }

        let mut scenarios = Vec::new();
        let mut questions = Vec::new();
        for path in files.list(&format!("{base}/scenarios/")) {
            let Some(rest) = path.strip_prefix(&format!("{base}/scenarios/")) else { continue };
            let Some(sslug) = rest.strip_suffix("/scenario.toml") else { continue };
            if !is_slug(sslug) {
                bail!("{path}: invalid scenario slug");
            }
            let mut file: ScenarioFile = files.toml(path)?;
            if !matches!(file.kind.as_str(), "implementation" | "diagnostic") {
                bail!("{path}: kind must be implementation or diagnostic");
            }
            let dir = format!("{base}/scenarios/{sslug}");
            for (i, step) in file.steps.iter().enumerate() {
                if let Some(img) = &step.image {
                    files
                        .files
                        .get(&format!("{dir}/{img}"))
                        .with_context(|| format!("{path}: step {}: missing image {img}", i + 1))?;
                }
                for a in &step.annotations {
                    a.validate().with_context(|| format!("{path}: step {}", i + 1))?;
                }
            }
            for mut q in std::mem::take(&mut file.questions) {
                q.scenario = Some(sslug.to_string());
                if q.pool != "quiz" && q.pool != "case" {
                    bail!("{path}: question {}: scenario questions use pool quiz or case", q.reference);
                }
                questions.push(q);
            }
            scenarios.push(ParsedScenario { slug: sslug.to_string(), dir, file });
        }
        scenarios.sort_by(|a, b| a.file.position.cmp(&b.file.position).then(a.slug.cmp(&b.slug)));

        let mut qfiles = files.list(&format!("{base}/questions/"));
        qfiles.retain(|p| p.ends_with(".toml"));
        for path in qfiles {
            let qf: QuestionsFile = files.toml(path)?;
            questions.extend(qf.questions);
        }
        let module_slugs: Vec<&str> = modules.iter().map(|m| m.slug.as_str()).collect();
        let scenario_slugs: Vec<&str> = scenarios.iter().map(|s| s.slug.as_str()).collect();
        for q in &mut questions {
            validate_question(q, &module_slugs, &scenario_slugs)
                .with_context(|| format!("{base}: question {}", q.reference))?;
        }
        tracks.push(ParsedTrack { slug, track, modules, scenarios, questions });
    }

    let mut refs = HashMap::new();
    for t in &tracks {
        for q in &t.questions {
            if let Some(other) = refs.insert(q.reference.clone(), t.slug.clone()) {
                bail!("duplicate question ref {} (tracks {other} and {})", q.reference, t.slug);
            }
        }
    }
    Ok(ParsedPack { manifest, tracks })
}

fn split_front_matter(src: &str) -> anyhow::Result<(&str, &str)> {
    let src = src.trim_start_matches('\u{feff}');
    let rest = src.strip_prefix("+++").context("modules start with a +++ TOML front matter")?;
    let end = rest.find("\n+++").context("unterminated front matter")?;
    Ok((&rest[..end], &rest[end + 4..]))
}

pub fn validate_question(q: &mut QuestionFile, modules: &[&str], scenarios: &[&str]) -> anyhow::Result<()> {
    if q.reference.trim().is_empty() || q.reference.len() > 120 {
        bail!("ref is required");
    }
    if !matches!(q.pool.as_str(), "quiz" | "exam" | "recert" | "case") {
        bail!("pool must be quiz, exam, recert or case");
    }
    if !matches!(q.format.as_str(), "choice" | "written") {
        bail!("format must be choice or written");
    }
    if q.prompt.trim().is_empty() {
        bail!("prompt is required");
    }
    if let Some(m) = &q.module
        && !modules.contains(&m.as_str())
    {
        bail!("unknown module {m}");
    }
    if let Some(s) = &q.scenario
        && !scenarios.contains(&s.as_str())
    {
        bail!("unknown scenario {s}");
    }
    if q.pool == "quiz" && q.module.is_none() && q.scenario.is_none() {
        bail!("quiz questions belong to a module or a scenario");
    }
    if q.pool == "case" && q.scenario.is_none() {
        bail!("case questions belong to a scenario");
    }
    if q.format == "written" {
        if !q.choices.is_empty() {
            bail!("written questions have no choices");
        }
        return Ok(());
    }
    if q.choices.len() < 2 {
        bail!("choice questions need at least two choices");
    }
    if !q.choices.iter().any(|c| c.correct) {
        bail!("at least one choice must be correct");
    }
    for (i, c) in q.choices.iter_mut().enumerate() {
        if c.id.is_none() {
            c.id = Some(((b'a' + i as u8) as char).to_string());
        }
    }
    let mut ids: Vec<&String> = q.choices.iter().filter_map(|c| c.id.as_ref()).collect();
    ids.sort();
    ids.dedup();
    if ids.len() != q.choices.len() {
        bail!("choice ids must be unique");
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Writing to the database
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
pub struct ImportReport {
    pub tracks: usize,
    pub modules: usize,
    pub scenarios: usize,
    pub questions: usize,
    pub assets: usize,
    pub levels: usize,
}

pub async fn store_asset<'c>(
    tx: &mut Transaction<'c, Postgres>,
    data_dir: &Path,
    name: &str,
    bytes: &[u8],
) -> anyhow::Result<Uuid> {
    let sha = hex::encode(Sha256::digest(bytes));
    if let Some(id) = sqlx::query_scalar::<_, Uuid>("SELECT id FROM assets WHERE sha256 = $1")
        .bind(&sha)
        .fetch_optional(&mut **tx)
        .await?
    {
        return Ok(id);
    }
    let dir = data_dir.join("assets");
    tokio::fs::create_dir_all(&dir).await?;
    let path = asset_path(data_dir, &sha);
    if !tokio::fs::try_exists(&path).await? {
        tokio::fs::write(&path, bytes).await?;
    }
    let id: Uuid = sqlx::query_scalar(
        "INSERT INTO assets (id, sha256, content_type, size_bytes, original_name) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (sha256) DO UPDATE SET sha256 = EXCLUDED.sha256 RETURNING id",
    )
    .bind(Uuid::new_v4())
    .bind(&sha)
    .bind(content_type_for(name))
    .bind(bytes.len() as i64)
    .bind(name.rsplit('/').next().unwrap_or(name))
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

pub fn asset_path(data_dir: &Path, sha: &str) -> PathBuf {
    data_dir.join("assets").join(sha)
}

/// Allow-list of content types; anything else is served as a download.
pub fn content_type_for(name: &str) -> &'static str {
    match name.rsplit('.').next().map(|e| e.to_ascii_lowercase()).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("pdf") => "application/pdf",
        Some("vtt") => "text/vtt",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("zip") => "application/zip",
        Some("txt" | "log" | "conf" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub async fn import(db: &PgPool, data_dir: &Path, files: &PackFiles) -> anyhow::Result<ImportReport> {
    let pack = parse(files)?;
    let mut report = ImportReport::default();
    let mut tx = db.begin().await?;

    for level in &pack.manifest.levels {
        sqlx::query(
            "INSERT INTO requirement_levels (slug, org_kind, name, rank, requirements) VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (slug) DO UPDATE SET org_kind = $2, name = $3, rank = $4, requirements = $5",
        )
        .bind(&level.slug)
        .bind(&level.org_kind)
        .bind(&level.name)
        .bind(level.rank)
        .bind(serde_json::to_value(&level.requirements)?)
        .execute(&mut *tx)
        .await?;
        report.levels += 1;
    }

    for t in &pack.tracks {
        let tf = &t.track;
        let track_id: Uuid = sqlx::query_scalar(
            "INSERT INTO tracks (id, slug, title, summary, description_md, audiences, position, prerequisite_slug,
                prerequisites_md, estimated_minutes, scenarios_required, validity_months, module_quiz_pass_percent,
                exam, recert_exam, badge, published, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17, now())
             ON CONFLICT (slug) DO UPDATE SET title=$3, summary=$4, description_md=$5, audiences=$6, position=$7,
                prerequisite_slug=$8, prerequisites_md=$9, estimated_minutes=$10, scenarios_required=$11,
                validity_months=$12, module_quiz_pass_percent=$13, exam=$14, recert_exam=$15, badge=$16,
                published=$17, updated_at=now()
             RETURNING id",
        )
        .bind(Uuid::new_v4())
        .bind(&t.slug)
        .bind(&tf.title)
        .bind(&tf.summary)
        .bind(&tf.description)
        .bind(&tf.audiences)
        .bind(tf.position)
        .bind(&tf.prerequisite)
        .bind(&tf.prerequisites)
        .bind(tf.estimated_minutes)
        .bind(tf.scenarios_required)
        .bind(tf.validity_months)
        .bind(tf.module_quiz_pass_percent)
        .bind(serde_json::to_value(&tf.exam)?)
        .bind(tf.recert_exam.as_ref().map(serde_json::to_value).transpose()?)
        .bind(serde_json::to_value(&tf.badge)?)
        .bind(tf.published)
        .fetch_one(&mut *tx)
        .await?;
        report.tracks += 1;

        // Modules
        let mut module_ids: HashMap<String, Uuid> = HashMap::new();
        for m in &t.modules {
            let mut attachments = Vec::new();
            for a in &m.front.attachments {
                let path = format!("tracks/{}/{}", t.slug, a.file);
                let id = store_asset(&mut tx, data_dir, &a.file, &files.files[&path]).await?;
                report.assets += 1;
                attachments.push(json!({ "asset_id": id, "label": a.label, "name": a.file.rsplit('/').next() }));
            }
            // Positions are offset first to avoid transient clashes when reordering.
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO modules (id, track_id, slug, position, title, video_url, captions_url, duration_minutes,
                    body_md, attachments, doc_url, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now())
                 ON CONFLICT (track_id, slug) DO UPDATE SET position=$4, title=$5, video_url=$6, captions_url=$7,
                    duration_minutes=$8, body_md=$9, attachments=$10, doc_url=$11, updated_at=now()
                 RETURNING id",
            )
            .bind(Uuid::new_v4())
            .bind(track_id)
            .bind(&m.slug)
            .bind(m.position)
            .bind(&m.front.title)
            .bind(&m.front.video)
            .bind(&m.front.captions)
            .bind(m.front.duration_minutes)
            .bind(&m.body)
            .bind(serde_json::Value::Array(attachments))
            .bind(&m.front.doc_url)
            .fetch_one(&mut *tx)
            .await?;
            module_ids.insert(m.slug.clone(), id);
            report.modules += 1;
        }
        let keep: Vec<Uuid> = module_ids.values().copied().collect();
        sqlx::query("DELETE FROM modules WHERE track_id = $1 AND NOT (id = ANY($2))")
            .bind(track_id)
            .bind(&keep)
            .execute(&mut *tx)
            .await?;

        // Scenarios
        let mut scenario_ids: HashMap<String, Uuid> = HashMap::new();
        for s in &t.scenarios {
            let f = &s.file;
            let id: Uuid = sqlx::query_scalar(
                "INSERT INTO scenarios (id, track_id, slug, position, title, kind, exam_only, context_md, pitfalls_md, family, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10, now())
                 ON CONFLICT (track_id, slug) DO UPDATE SET position=$4, title=$5, kind=$6, exam_only=$7,
                    context_md=$8, pitfalls_md=$9, family=$10, updated_at=now()
                 RETURNING id",
            )
            .bind(Uuid::new_v4())
            .bind(track_id)
            .bind(&s.slug)
            .bind(f.position)
            .bind(&f.title)
            .bind(&f.kind)
            .bind(f.exam_only)
            .bind(&f.context)
            .bind(&f.pitfalls)
            .bind(&f.family)
            .fetch_one(&mut *tx)
            .await?;
            sqlx::query("DELETE FROM scenario_steps WHERE scenario_id = $1").bind(id).execute(&mut *tx).await?;
            for (i, step) in f.steps.iter().enumerate() {
                let image = match &step.image {
                    Some(img) => {
                        let bytes = &files.files[&format!("{}/{img}", s.dir)];
                        report.assets += 1;
                        Some(store_asset(&mut tx, data_dir, img, bytes).await?)
                    }
                    None => None,
                };
                sqlx::query(
                    "INSERT INTO scenario_steps (scenario_id, position, action_md, image_asset, image_alt, annotations, expected_md)
                     VALUES ($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(id)
                .bind(i as i32 + 1)
                .bind(&step.action)
                .bind(image)
                .bind(&step.alt)
                .bind(serde_json::to_value(&step.annotations)?)
                .bind(&step.expected)
                .execute(&mut *tx)
                .await?;
            }
            scenario_ids.insert(s.slug.clone(), id);
            report.scenarios += 1;
        }
        let keep: Vec<Uuid> = scenario_ids.values().copied().collect();
        sqlx::query("DELETE FROM scenarios WHERE track_id = $1 AND NOT (id = ANY($2))")
            .bind(track_id)
            .bind(&keep)
            .execute(&mut *tx)
            .await?;

        // Questions
        let mut refs = Vec::new();
        for q in &t.questions {
            let choices: Vec<serde_json::Value> =
                q.choices.iter().map(|c| json!({ "id": c.id, "text": c.text, "correct": c.correct })).collect();
            sqlx::query(
                "INSERT INTO questions (id, ref, track_id, pool, module_id, scenario_id, format, prompt_md, choices,
                    explanation_md, active, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11, now())
                 ON CONFLICT (ref) DO UPDATE SET track_id=$3, pool=$4, module_id=$5, scenario_id=$6, format=$7,
                    prompt_md=$8, choices=$9, explanation_md=$10, active=$11, updated_at=now()",
            )
            .bind(Uuid::new_v4())
            .bind(&q.reference)
            .bind(track_id)
            .bind(&q.pool)
            .bind(q.module.as_ref().and_then(|m| module_ids.get(m)))
            .bind(q.scenario.as_ref().and_then(|s| scenario_ids.get(s)))
            .bind(&q.format)
            .bind(&q.prompt)
            .bind(serde_json::Value::Array(choices))
            .bind(&q.explanation)
            .bind(q.active)
            .execute(&mut *tx)
            .await?;
            refs.push(q.reference.clone());
            report.questions += 1;
        }
        sqlx::query("UPDATE questions SET active = FALSE WHERE track_id = $1 AND NOT (ref = ANY($2))")
            .bind(track_id)
            .bind(&refs)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(report)
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct TrackRow {
    id: Uuid,
    slug: String,
    title: String,
    summary: String,
    description_md: String,
    audiences: Vec<String>,
    position: i32,
    prerequisite_slug: Option<String>,
    prerequisites_md: String,
    estimated_minutes: i32,
    scenarios_required: bool,
    validity_months: Option<i32>,
    module_quiz_pass_percent: i32,
    exam: serde_json::Value,
    recert_exam: Option<serde_json::Value>,
    badge: serde_json::Value,
    published: bool,
}

/// Exports every track as a zip pack, re-importable as is.
#[allow(clippy::type_complexity)]
pub async fn export(db: &PgPool, data_dir: &Path) -> anyhow::Result<Vec<u8>> {
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    let levels: Vec<(String, String, String, i32, serde_json::Value)> = sqlx::query_as(
        "SELECT slug, org_kind, name, rank, requirements FROM requirement_levels ORDER BY org_kind, rank",
    )
    .fetch_all(db)
    .await?;
    let manifest = PackManifest {
        format: FORMAT_VERSION,
        name: "export".into(),
        levels: levels
            .into_iter()
            .map(|(slug, org_kind, name, rank, req)| -> anyhow::Result<LevelFile> {
                Ok(LevelFile { slug, org_kind, name, rank, requirements: serde_json::from_value(req)? })
            })
            .collect::<anyhow::Result<_>>()?,
    };
    out.insert("pack.toml".into(), toml::to_string_pretty(&manifest)?.into_bytes());

    let tracks: Vec<TrackRow> = sqlx::query_as("SELECT * FROM tracks ORDER BY position, slug").fetch_all(db).await?;
    let read_asset = |id: Uuid| async move {
        let sha: String = sqlx::query_scalar("SELECT sha256 FROM assets WHERE id = $1").bind(id).fetch_one(db).await?;
        anyhow::Ok(tokio::fs::read(asset_path(data_dir, &sha)).await?)
    };
    for t in tracks {
        let base = format!("tracks/{}", t.slug);
        let tf = TrackFile {
            title: t.title,
            summary: t.summary,
            description: t.description_md,
            audiences: t.audiences,
            position: t.position,
            prerequisite: t.prerequisite_slug,
            prerequisites: t.prerequisites_md,
            estimated_minutes: t.estimated_minutes,
            scenarios_required: t.scenarios_required,
            validity_months: t.validity_months,
            module_quiz_pass_percent: t.module_quiz_pass_percent,
            published: t.published,
            badge: serde_json::from_value(t.badge)?,
            exam: serde_json::from_value(t.exam)?,
            recert_exam: t.recert_exam.map(serde_json::from_value).transpose()?,
        };
        out.insert(format!("{base}/track.toml"), toml::to_string_pretty(&tf)?.into_bytes());

        let modules: Vec<(
            Uuid,
            String,
            i32,
            String,
            Option<String>,
            Option<String>,
            i32,
            String,
            serde_json::Value,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT id, slug, position, title, video_url, captions_url, duration_minutes, body_md, attachments, doc_url
                 FROM modules WHERE track_id = $1 ORDER BY position",
        )
        .bind(t.id)
        .fetch_all(db)
        .await?;
        let mut module_slugs = HashMap::new();
        for (id, slug, position, title, video, captions, duration, body, attachments, doc_url) in modules {
            let mut atts = Vec::new();
            for a in attachments.as_array().cloned().unwrap_or_default() {
                let Some(asset_id) = a["asset_id"].as_str().and_then(|s| s.parse().ok()) else { continue };
                let name = a["name"].as_str().unwrap_or("file").to_string();
                let file = format!("files/{name}");
                out.insert(format!("{base}/{file}"), read_asset(asset_id).await?);
                atts.push(AttachmentFile { file, label: a["label"].as_str().unwrap_or_default().to_string() });
            }
            let front =
                ModuleFrontMatter { title, video, captions, duration_minutes: duration, doc_url, attachments: atts };
            let content = format!("+++\n{}+++\n\n{}\n", toml::to_string_pretty(&front)?, body);
            out.insert(format!("{base}/modules/{position:02}-{slug}.md"), content.into_bytes());
            module_slugs.insert(id, slug);
        }

        let scenarios: Vec<(Uuid, String, i32, String, String, bool, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, slug, position, title, kind, exam_only, context_md, pitfalls_md, family
             FROM scenarios WHERE track_id = $1 ORDER BY position, slug",
        )
        .bind(t.id)
        .fetch_all(db)
        .await?;
        let mut scenario_slugs = HashMap::new();
        for (id, slug, position, title, kind, exam_only, context, pitfalls, family) in &scenarios {
            scenario_slugs.insert(*id, slug.clone());
            let steps: Vec<(i32, String, Option<Uuid>, String, serde_json::Value, String)> = sqlx::query_as(
                "SELECT position, action_md, image_asset, image_alt, annotations, expected_md
                 FROM scenario_steps WHERE scenario_id = $1 ORDER BY position",
            )
            .bind(id)
            .fetch_all(db)
            .await?;
            let dir = format!("{base}/scenarios/{slug}");
            let mut step_files = Vec::new();
            for (pos, action, image, alt, annotations, expected) in steps {
                let image_name = match image {
                    Some(asset) => {
                        let ext: String = sqlx::query_scalar("SELECT original_name FROM assets WHERE id = $1")
                            .bind(asset)
                            .fetch_one(db)
                            .await?;
                        let ext = ext.rsplit('.').next().unwrap_or("png").to_string();
                        let name = format!("step-{pos:02}.{ext}");
                        out.insert(format!("{dir}/{name}"), read_asset(asset).await?);
                        Some(name)
                    }
                    None => None,
                };
                step_files.push(StepFile {
                    action,
                    image: image_name,
                    alt,
                    expected,
                    annotations: serde_json::from_value(annotations)?,
                });
            }
            let sf = ScenarioFile {
                title: title.clone(),
                kind: kind.clone(),
                exam_only: *exam_only,
                family: family.clone(),
                position: *position,
                context: context.clone(),
                pitfalls: pitfalls.clone(),
                steps: step_files,
                questions: Vec::new(),
            };
            out.insert(format!("{dir}/scenario.toml"), toml::to_string_pretty(&sf)?.into_bytes());
        }

        let questions: Vec<(
            String,
            String,
            Option<Uuid>,
            Option<Uuid>,
            String,
            String,
            serde_json::Value,
            String,
            bool,
        )> = sqlx::query_as(
            "SELECT ref, pool, module_id, scenario_id, format, prompt_md, choices, explanation_md, active
                 FROM questions WHERE track_id = $1 ORDER BY pool, ref",
        )
        .bind(t.id)
        .fetch_all(db)
        .await?;
        let mut by_pool: BTreeMap<String, Vec<QuestionFile>> = BTreeMap::new();
        for (reference, pool, module, scenario, format, prompt, choices, explanation, active) in questions {
            let choices: Vec<ChoiceFile> = choices
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .map(|c| ChoiceFile {
                    id: c["id"].as_str().map(String::from),
                    text: c["text"].as_str().unwrap_or_default().to_string(),
                    correct: c["correct"].as_bool().unwrap_or(false),
                })
                .collect();
            by_pool.entry(pool.clone()).or_default().push(QuestionFile {
                reference,
                pool,
                module: module.and_then(|m| module_slugs.get(&m).cloned()),
                scenario: scenario.and_then(|s| scenario_slugs.get(&s).cloned()),
                format,
                prompt,
                explanation,
                choices,
                active,
            });
        }
        for (pool, questions) in by_pool {
            let content = toml::to_string_pretty(&QuestionsFile { questions })?;
            out.insert(format!("{base}/questions/{pool}.toml"), content.into_bytes());
        }
    }

    let mut buf = std::io::Cursor::new(Vec::new());
    {
        let mut zip = zip::ZipWriter::new(&mut buf);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        for (path, bytes) in out {
            zip.start_file(path, opts)?;
            zip.write_all(&bytes)?;
        }
        zip.finish()?;
    }
    Ok(buf.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_demo_pack() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/demo-pack");
        let files = PackFiles::from_dir(&root).unwrap();
        let pack = parse(&files).unwrap();
        assert!(!pack.tracks.is_empty());
        for t in &pack.tracks {
            assert!(!t.modules.is_empty(), "{} has modules", t.slug);
        }
    }

    #[test]
    fn question_validation() {
        let mut q = QuestionFile {
            reference: "q1".into(),
            pool: "exam".into(),
            module: None,
            scenario: None,
            format: "choice".into(),
            prompt: "?".into(),
            explanation: String::new(),
            choices: vec![
                ChoiceFile { id: None, text: "a".into(), correct: true },
                ChoiceFile { id: None, text: "b".into(), correct: false },
            ],
            active: true,
        };
        validate_question(&mut q, &[], &[]).unwrap();
        assert_eq!(q.choices[1].id.as_deref(), Some("b"));
        q.choices[0].correct = false;
        assert!(validate_question(&mut q, &[], &[]).is_err());
        q.choices[0].correct = true;
        q.pool = "quiz".into();
        assert!(validate_question(&mut q, &[], &[]).is_err(), "quiz needs a module");
    }

    #[test]
    fn annotations_are_bounded() {
        let a = Annotation {
            kind: "box".into(),
            x: 10.0,
            y: 10.0,
            w: Some(20.0),
            h: Some(5.0),
            x2: None,
            y2: None,
            label: None,
        };
        a.validate().unwrap();
        let bad = Annotation { x: 120.0, ..a.clone() };
        assert!(bad.validate().is_err());
        let no_size = Annotation { w: None, ..a };
        assert!(no_size.validate().is_err());
    }

    #[test]
    fn front_matter() {
        let (f, b) = split_front_matter("+++\ntitle = \"x\"\n+++\n\nBody").unwrap();
        assert_eq!(f.trim(), "title = \"x\"");
        assert_eq!(b.trim(), "Body");
    }
}
