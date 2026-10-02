//! End-to-end API tests against a real PostgreSQL database.
//! `#[sqlx::test]` creates a fresh database per test from `DATABASE_URL`.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use lectern_server::config::Config;
use lectern_server::{app, pack, state::AppState, state_with_pool};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;

const ORIGIN: &str = "http://localhost:8080";

struct TestApp {
    router: Router,
    state: AppState,
    _data: tempdir::TempDir,
}

mod tempdir {
    use std::path::PathBuf;
    pub struct TempDir(pub PathBuf);
    impl TempDir {
        pub fn new() -> Self {
            let p = std::env::temp_dir().join(format!("lectern-test-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

fn manifest(p: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(p)
}

async fn setup(db: PgPool) -> TestApp {
    setup_with(db, false).await
}

async fn setup_with(db: PgPool, secure_cookies: bool) -> TestApp {
    let data = tempdir::TempDir::new();
    let raw = std::fs::read_to_string(manifest("../config/lectern.example.toml")).unwrap();
    let mut table: toml::Table = raw.parse().unwrap();
    table["server"]["data_dir"] = toml::Value::String(data.0.to_string_lossy().into());
    table["server"]["branding_dir"] = toml::Value::String(data.0.join("branding").to_string_lossy().into());
    table["auth"]["require_mfa_for"] = toml::Value::Array(vec![]);
    table["server"]["secure_cookies"] = toml::Value::Boolean(secure_cookies);
    let config: Config = toml::Value::Table(table).try_into().unwrap();
    let state = state_with_pool(config, db).unwrap();
    let files = pack::PackFiles::from_dir(&manifest("../examples/demo-pack")).unwrap();
    pack::import(&state.db, &data.0, &files).await.unwrap();
    TestApp { router: app::router(state.clone()), state, _data: data }
}

struct Session {
    cookie: String,
    csrf: String,
    id: String,
}

impl TestApp {
    async fn call(
        &self,
        method: Method,
        uri: &str,
        session: Option<&Session>,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        let mut req = Request::builder().method(method.clone()).uri(uri).header(header::ORIGIN, ORIGIN);
        if let Some(s) = session {
            req = req.header(header::COOKIE, &s.cookie);
            if method != Method::GET {
                req = req.header("x-csrf-token", &s.csrf);
            }
        }
        let req = match body {
            Some(b) => req.header(header::CONTENT_TYPE, "application/json").body(Body::from(b.to_string())).unwrap(),
            None => req.body(Body::empty()).unwrap(),
        };
        let res = self.router.clone().oneshot(req).await.unwrap();
        let status = res.status();
        let bytes = axum::body::to_bytes(res.into_body(), 50_000_000).await.unwrap();
        let v = serde_json::from_slice(&bytes).unwrap_or_else(
            |_| json!({ "raw_len": bytes.len(), "head": String::from_utf8_lossy(&bytes[..bytes.len().min(8)]) }),
        );
        (status, v)
    }

    async fn get(&self, uri: &str, s: &Session) -> Value {
        let (status, v) = self.call(Method::GET, uri, Some(s), None).await;
        assert_eq!(status, StatusCode::OK, "GET {uri}: {v}");
        v
    }

    async fn post(&self, uri: &str, s: &Session, body: Value) -> Value {
        let (status, v) = self.call(Method::POST, uri, Some(s), Some(body)).await;
        assert_eq!(status, StatusCode::OK, "POST {uri}: {v}");
        v
    }

    /// Signs in through the real e-mail link flow, reading the link from the outbox.
    async fn login(&self, email: &str) -> Session {
        let (status, _) = self
            .call(
                Method::POST,
                "/api/auth/email/request",
                None,
                Some(json!({ "email": email, "display_name": "Test User" })),
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        let body: String = sqlx::query_scalar(
            "SELECT text_body FROM email_outbox WHERE to_address = $1 ORDER BY created_at DESC LIMIT 1",
        )
        .bind(email)
        .fetch_one(&self.state.db)
        .await
        .unwrap();
        let token = body.split("token=").nth(1).unwrap().split(')').next().unwrap().to_string();
        let req = Request::post("/api/auth/email/verify")
            .header(header::CONTENT_TYPE, "application/json")
            .header(header::ORIGIN, ORIGIN)
            .body(Body::from(json!({ "token": token }).to_string()))
            .unwrap();
        let res = self.router.clone().oneshot(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let cookie = res.headers()[header::SET_COOKIE].to_str().unwrap().split(';').next().unwrap().to_string();
        assert!(res.headers()[header::SET_COOKIE].to_str().unwrap().contains("HttpOnly"));
        let mut s = Session { cookie, csrf: String::new(), id: String::new() };
        let me = self.get("/api/me", &s).await;
        s.csrf = me["csrf_token"].as_str().unwrap().to_string();
        s.id = me["id"].as_str().unwrap().to_string();
        s
    }

    async fn grant(&self, s: &Session, role: &str) {
        sqlx::query("INSERT INTO user_roles (user_id, role) VALUES ($1::uuid, $2)")
            .bind(&s.id)
            .bind(role)
            .execute(&self.state.db)
            .await
            .unwrap();
    }

    async fn correct_choices(&self, question_id: &str) -> Vec<String> {
        let choices: Value = sqlx::query_scalar("SELECT choices FROM questions WHERE id = $1::uuid")
            .bind(question_id)
            .fetch_one(&self.state.db)
            .await
            .unwrap();
        choices
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c["correct"] == json!(true))
            .map(|c| c["id"].as_str().unwrap().to_string())
            .collect()
    }

    async fn complete_modules(&self, s: &Session, track: &str) {
        let detail = self.get(&format!("/api/tracks/{track}"), s).await;
        for m in detail["modules"].as_array().unwrap() {
            let slug = m["slug"].as_str().unwrap();
            let module = self.get(&format!("/api/tracks/{track}/modules/{slug}"), s).await;
            let id = module["id"].as_str().unwrap();
            self.post(&format!("/api/modules/{id}/progress"), s, json!({ "content_completed": true })).await;
            if !module["quiz"].as_array().unwrap().is_empty() {
                let mut answers = serde_json::Map::new();
                for q in module["quiz"].as_array().unwrap() {
                    let qid = q["id"].as_str().unwrap();
                    answers.insert(qid.into(), json!(self.correct_choices(qid).await));
                }
                let r = self.post(&format!("/api/modules/{id}/quiz"), s, json!({ "answers": answers })).await;
                assert_eq!(r["completed"], json!(true), "{r}");
                assert!(r["results"][0]["explanation_html"].is_string());
            }
        }
    }

    /// Answers the current section (correctly or not) and submits it.
    async fn answer_section(&self, s: &Session, attempt: &str, correct: bool) -> Value {
        let view = self.get(&format!("/api/attempts/{attempt}"), s).await;
        let section = &view["visible_sections"][0];
        for item in section["items"].as_array().unwrap() {
            assert!(item["choices"].as_array().unwrap().iter().all(|c| c.get("correct").is_none()), "answers leaked");
            let qid = item["question_id"].as_str().unwrap();
            let answer = if item["format"] == "written" {
                json!("My detailed analysis")
            } else if correct {
                json!(self.correct_choices(qid).await)
            } else {
                json!([])
            };
            let (st, v) = self
                .call(
                    Method::PUT,
                    &format!("/api/attempts/{attempt}/answers"),
                    Some(s),
                    Some(json!({ "question_id": qid, "answer": answer })),
                )
                .await;
            assert_eq!(st, StatusCode::OK, "{v}");
        }
        self.post(&format!("/api/attempts/{attempt}/submit"), s, json!({ "section": view["current_section"] })).await
    }
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn discovery_path_to_verified_badge(db: PgPool) {
    let app = setup(db).await;

    // Anonymous visitors see public tracks only.
    let (st, catalog) = app.call(Method::GET, "/api/catalog", None, None).await;
    assert_eq!(st, StatusCode::OK);
    let slugs: Vec<&str> = catalog.as_array().unwrap().iter().map(|t| t["slug"].as_str().unwrap()).collect();
    assert_eq!(slugs, vec!["discovery"]);

    let s = app.login("learner@example.com").await;

    // The exam is locked until every module is completed.
    let (st, v) = app.call(Method::POST, "/api/tracks/discovery/exam", Some(&s), None).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(v["error"], "exam_locked");

    app.complete_modules(&s, "discovery").await;
    let started = app.post("/api/tracks/discovery/exam", &s, json!({})).await;
    let attempt = started["attempt_id"].as_str().unwrap().to_string();

    // One exam session at a time.
    let (st, _) = app.call(Method::POST, "/api/tracks/discovery/exam", Some(&s), None).await;
    assert_eq!(st, StatusCode::CONFLICT);

    let result = app.answer_section(&s, &attempt, true).await;
    assert_eq!(result["status"], "passed", "{result}");

    let certs = app.get("/api/me/certifications", &s).await;
    let cert = &certs[0];
    assert_eq!(cert["status"], "valid");
    assert!(cert["expires_at"].is_null(), "discovery never expires");
    assert!(cert["linkedin_add_url"].as_str().unwrap().starts_with("https://www.linkedin.com/profile/add"));
    let id = cert["id"].as_str().unwrap();

    // Public verification page and Open Badges assertion.
    let res =
        app.router.clone().oneshot(Request::get(format!("/verify/{id}")).body(Body::empty()).unwrap()).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(res.headers()["content-security-policy"].to_str().unwrap().contains("default-src 'self'"));
    let html = String::from_utf8(axum::body::to_bytes(res.into_body(), 1_000_000).await.unwrap().to_vec()).unwrap();
    assert!(html.contains("Test User"));
    let (st, ob) = app.call(Method::GET, &format!("/ob/assertions/{id}"), None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(ob["recipient"]["hashed"], json!(true));
    assert!(!ob.to_string().contains("learner@example.com"), "e-mail must not be published");
    let (st, png) = app.call(Method::GET, &format!("/verify/{id}/badge.png"), None, None).await;
    assert_eq!(st, StatusCode::OK);
    assert!(png["raw_len"].as_u64().unwrap() > 1000);

    // PDF certificate.
    let (st, pdf) = app.call(Method::GET, &format!("/api/certifications/{id}/certificate.pdf"), Some(&s), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(pdf["head"], "%PDF-1.7");

    // A private profile hides the verification page.
    app.call(Method::PATCH, "/api/me", Some(&s), Some(json!({ "public_profile": false }))).await;
    let (st, _) = app.call(Method::GET, &format!("/ob/assertions/{id}"), None, None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // Success e-mail queued.
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM email_outbox WHERE subject LIKE '%Discovery%'")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 1);
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn csrf_and_origin_are_enforced(db: PgPool) {
    let app = setup(db).await;
    let s = app.login("csrf@example.com").await;
    let no_csrf = Session { cookie: s.cookie.clone(), csrf: "wrong".into(), id: s.id.clone() };
    let (st, v) = app.call(Method::POST, "/api/tracks/discovery/enroll", Some(&no_csrf), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    assert_eq!(v["error"], "csrf");

    let req = Request::post("/api/tracks/discovery/enroll")
        .header(header::COOKIE, &s.cookie)
        .header("x-csrf-token", &s.csrf)
        .header(header::ORIGIN, "https://evil.example")
        .body(Body::empty())
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // A login link works only once.
    let (st, _) = app.call(Method::POST, "/api/auth/email/verify", None, Some(json!({ "token": "forged" }))).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn organizations_are_isolated_and_requirements_counted(db: PgPool) {
    let app = setup(db).await;
    let admin = app.login("admin@example.com").await;
    app.grant(&admin, "admin").await;

    let a = app
        .post(
            "/api/admin/organizations",
            &admin,
            json!({ "name": "Partner A", "kind": "partner", "level_slug": "partner-silver" }),
        )
        .await;
    let b = app.post("/api/admin/organizations", &admin, json!({ "name": "Partner B", "kind": "partner" })).await;
    let (a, b) = (a["id"].as_str().unwrap(), b["id"].as_str().unwrap());
    app.post(&format!("/api/admin/organizations/{a}/managers"), &admin, json!({ "email": "manager-a@example.com" }))
        .await;
    app.post(&format!("/api/admin/organizations/{b}/managers"), &admin, json!({ "email": "manager-b@example.com" }))
        .await;

    let manager_a = app.login("manager-a@example.com").await;
    let org_a = app.get("/api/organization", &manager_a).await;
    let code_a = org_a["join_code"].as_str().unwrap().to_string();
    let code_b: String = sqlx::query_scalar("SELECT join_code FROM organizations WHERE id = $1::uuid")
        .bind(b)
        .fetch_one(&app.state.db)
        .await
        .unwrap();

    // A learner joins B: manager A cannot see nor approve them.
    let learner = app.login("tech@example.com").await;
    app.post("/api/me/organization", &learner, json!({ "join_code": code_b })).await;
    let (st, _) = app
        .call(
            Method::POST,
            &format!("/api/organization/members/{}/decision", learner.id),
            Some(&manager_a),
            Some(json!({ "approve": true })),
        )
        .await;
    assert_eq!(st, StatusCode::NOT_FOUND);
    let org_a = app.get("/api/organization", &manager_a).await;
    assert!(!org_a.to_string().contains("tech@example.com"));

    // Partner tracks stay hidden until the membership is approved.
    let (st, _) = app.call(Method::GET, "/api/tracks/associate", Some(&learner), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    // A learner of A, approved, certified Associate: counted for A's Silver requirements.
    let seller = app.login("seller@example.com").await;
    app.post("/api/me/organization", &seller, json!({ "join_code": code_a })).await;
    app.post(&format!("/api/organization/members/{}/decision", seller.id), &manager_a, json!({ "approve": true }))
        .await;
    let seller = app.login("seller@example.com").await;
    app.complete_modules(&seller, "associate").await;
    let attempt = app.post("/api/tracks/associate/exam", &seller, json!({})).await;
    let r = app.answer_section(&seller, attempt["attempt_id"].as_str().unwrap(), true).await;
    assert_eq!(r["status"], "passed");

    let org_a = app.get("/api/organization", &manager_a).await;
    assert_eq!(org_a["valid_certifications"]["associate"], json!(1));
    let silver = org_a["levels"].as_array().unwrap().iter().find(|l| l["slug"] == "partner-silver").unwrap();
    let engineer = silver["requirements"].as_array().unwrap().iter().find(|r| r["track_slug"] == "engineer").unwrap();
    assert_eq!(engineer["missing"], json!(2));

    // Channel manager export and machine API.
    let cm = app.login("cm@example.com").await;
    app.grant(&cm, "channel_manager").await;
    let partners = app.get("/api/partners", &cm).await;
    assert_eq!(partners.as_array().unwrap().len(), 2);
    let token = app.post("/api/admin/api-tokens", &admin, json!({ "name": "portal" })).await;
    let req = Request::get("/api/v1/certified")
        .header(header::AUTHORIZATION, format!("Bearer {}", token["token"].as_str().unwrap()))
        .body(Body::empty())
        .unwrap();
    let res = app.router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&axum::body::to_bytes(res.into_body(), 100_000).await.unwrap()).unwrap();
    assert_eq!(body["organizations"][0]["valid_certifications"]["associate"], json!(1));

    // Learners cannot reach staff endpoints.
    let (st, _) = app.call(Method::GET, "/api/partners", Some(&seller), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn engineer_exam_attempt_policy_and_case_study_protection(db: PgPool) {
    let app = setup(db).await;
    let admin = app.login("admin@example.com").await;
    app.grant(&admin, "admin").await;
    let org = app.post("/api/admin/organizations", &admin, json!({ "name": "Partner", "kind": "partner" })).await;
    let org = org["id"].as_str().unwrap();
    app.post(&format!("/api/admin/organizations/{org}/managers"), &admin, json!({ "email": "eng@example.com" })).await;
    let s = app.login("eng@example.com").await;

    // Case-study screenshots are not reachable outside an attempt.
    let case_asset: String = sqlx::query_scalar(
        "SELECT st.image_asset::text FROM scenario_steps st JOIN scenarios s ON s.id = st.scenario_id WHERE s.exam_only AND st.image_asset IS NOT NULL LIMIT 1",
    )
    .fetch_one(&app.state.db)
    .await
    .unwrap();
    let (st, _) = app.call(Method::GET, &format!("/api/assets/{case_asset}"), Some(&s), None).await;
    assert_eq!(st, StatusCode::NOT_FOUND);

    app.complete_modules(&s, "engineer").await;
    let (_, v) = app.call(Method::POST, "/api/tracks/engineer/exam", Some(&s), None).await;
    assert_eq!(v["error"], "exam_locked");
    assert!(v["details"].to_string().contains("scenarios"), "{v}");

    let detail = app.get("/api/tracks/engineer", &s).await;
    for sc in detail["scenarios"].as_array().unwrap() {
        let sc = app.get(&format!("/api/tracks/engineer/scenarios/{}", sc["slug"].as_str().unwrap()), &s).await;
        let mut answers = serde_json::Map::new();
        for q in sc["questions"].as_array().unwrap() {
            let qid = q["id"].as_str().unwrap();
            answers.insert(qid.into(), json!(app.correct_choices(qid).await));
        }
        let r = app
            .post(&format!("/api/scenarios/{}/check", sc["id"].as_str().unwrap()), &s, json!({ "answers": answers }))
            .await;
        assert_eq!(r["passed"], json!(true));
    }

    // Two free attempts, both failed on the first section.
    for _ in 0..2 {
        let a = app.post("/api/tracks/engineer/exam", &s, json!({})).await;
        let r = app.answer_section(&s, a["attempt_id"].as_str().unwrap(), false).await;
        assert_eq!(r["status"], "failed");
    }
    // Third attempt: cooldown applies.
    let (st, v) = app.call(Method::POST, "/api/tracks/engineer/exam", Some(&s), None).await;
    assert_eq!(st, StatusCode::CONFLICT);
    assert_eq!(v["details"]["reason"], "cooldown", "{v}");

    // After the cooldown, a granted credit allows a new attempt.
    sqlx::query("UPDATE exam_attempts SET started_at = started_at - interval '20 days', finished_at = finished_at - interval '20 days'")
        .execute(&app.state.db)
        .await
        .unwrap();
    let (_, v) = app.call(Method::POST, "/api/tracks/engineer/exam", Some(&s), None).await;
    assert_eq!(v["details"]["reason"], "no_attempt_left");
    app.post(&format!("/api/admin/users/{}/credits", s.id), &admin, json!({ "track_slug": "engineer" })).await;
    let a = app.post("/api/tracks/engineer/exam", &s, json!({})).await;
    let attempt = a["attempt_id"].as_str().unwrap();

    let r = app.answer_section(&s, attempt, true).await;
    assert_eq!(r["status"], "in_progress");
    assert_eq!(r["current_section"], json!(1));
    // During the case study, its screenshots are served to the candidate.
    let case = &r["visible_sections"][0]["scenarios"][0];
    let img = case["steps"][0]["image_url"].as_str().unwrap();
    let (st, _) = app.call(Method::GET, img, Some(&s), None).await;
    assert_eq!(st, StatusCode::OK);

    let r = app.answer_section(&s, attempt, true).await;
    assert_eq!(r["status"], "passed", "{r}");
    let certs = app.get("/api/me/certifications", &s).await;
    assert!(certs[0]["expires_at"].is_string(), "engineer certifications expire");
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn account_export_and_erasure(db: PgPool) {
    let app = setup(db).await;
    let s = app.login("gdpr@example.com").await;
    app.post("/api/tracks/discovery/enroll", &s, json!({})).await;
    let (st, export) = app.call(Method::GET, "/api/me/export", Some(&s), None).await;
    assert_eq!(st, StatusCode::OK);
    assert_eq!(export["profile"]["email"], "gdpr@example.com");
    assert_eq!(export["enrollments"][0]["slug"], "discovery");

    let (st, _) = app.call(Method::DELETE, "/api/me", Some(&s), None).await;
    assert_eq!(st, StatusCode::OK);
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM users WHERE email = 'gdpr@example.com'")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    assert_eq!(n, 0);
    let (st, _) = app.call(Method::GET, "/api/me", Some(&s), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn pack_export_reimports_identically(db: PgPool) {
    let app = setup(db).await;
    let zip = pack::export(&app.state.db, &app._data.0).await.unwrap();
    let files = pack::PackFiles::from_zip(&zip, 100_000_000).unwrap();
    let report = pack::import(&app.state.db, &app._data.0, &files).await.unwrap();
    assert_eq!(report.tracks, 5);
    let active: i64 =
        sqlx::query_scalar("SELECT count(*) FROM questions WHERE active").fetch_one(&app.state.db).await.unwrap();
    let total: i64 = sqlx::query_scalar("SELECT count(*) FROM questions").fetch_one(&app.state.db).await.unwrap();
    assert_eq!(active, total, "re-import keeps every question active");
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn expert_written_case_needs_manual_review(db: PgPool) {
    let app = setup(db).await;
    let admin = app.login("admin@example.com").await;
    app.grant(&admin, "admin").await;
    let trainer = app.login("trainer@example.com").await;
    app.grant(&trainer, "trainer").await;
    let org = app.post("/api/admin/organizations", &admin, json!({ "name": "P", "kind": "partner" })).await;
    let org = org["id"].as_str().unwrap();
    app.post(&format!("/api/admin/organizations/{org}/managers"), &admin, json!({ "email": "arch@example.com" })).await;
    let s = app.login("arch@example.com").await;

    // Prerequisite: a valid Engineer certification.
    let (_, v) = app.call(Method::POST, "/api/tracks/expert/exam", Some(&s), None).await;
    assert!(v["details"].to_string().contains("prerequisite:engineer"), "{v}");
    let engineer: String = sqlx::query_scalar("SELECT id::text FROM tracks WHERE slug = 'engineer'")
        .fetch_one(&app.state.db)
        .await
        .unwrap();
    sqlx::query("INSERT INTO certifications (id, user_id, track_id, expires_at) VALUES (gen_random_uuid(), $1::uuid, $2::uuid, now() + interval '1 year')")
        .bind(&s.id)
        .bind(&engineer)
        .execute(&app.state.db)
        .await
        .unwrap();

    app.complete_modules(&s, "expert").await;
    let a = app.post("/api/tracks/expert/exam", &s, json!({})).await;
    let attempt = a["attempt_id"].as_str().unwrap();
    let r = app.answer_section(&s, attempt, true).await;
    assert_eq!(r["status"], "pending_review");

    let pending = app.get("/api/reviews", &trainer).await;
    assert_eq!(pending[0]["id"], attempt);
    let detail = app.get(&format!("/api/reviews/{attempt}"), &trainer).await;
    assert_eq!(detail["visible_sections"][0]["items"][0]["answer"], "My detailed analysis");
    let r = app.post(&format!("/api/reviews/{attempt}"), &trainer, json!({ "decision": "pass", "comment": "Solid", "grid": [{ "criterion": "Diagnosis", "score": 4, "max": 5 }] })).await;
    assert_eq!(r["status"], "passed");
    let certs = app.get("/api/me/certifications", &s).await;
    assert!(certs.as_array().unwrap().iter().any(|c| c["track_slug"] == "expert" && c["status"] == "valid"));
}

#[sqlx::test(migrator = "lectern_server::MIGRATOR")]
async fn https_instances_use_host_prefixed_secure_cookie(db: PgPool) {
    let app = setup_with(db, true).await;
    let s = app.login("secure@example.com").await;
    assert!(s.cookie.starts_with("__Host-lectern_session="), "{}", s.cookie);
    // A cookie without the prefix (e.g. planted by a sibling sub-domain) is ignored.
    let token = s.cookie.split_once('=').unwrap().1;
    let planted = Session { cookie: format!("lectern_session={token}"), csrf: s.csrf.clone(), id: s.id.clone() };
    let (st, _) = app.call(Method::GET, "/api/me", Some(&planted), None).await;
    assert_eq!(st, StatusCode::UNAUTHORIZED);
    // Security headers specific to HTTPS.
    let res = app.router.clone().oneshot(Request::get("/api/instance").body(Body::empty()).unwrap()).await.unwrap();
    assert!(res.headers()["strict-transport-security"].to_str().unwrap().contains("max-age"));
    assert_eq!(res.headers()["cache-control"], "no-store");
}
