# Architecture

```
            browser (React SPA, same origin)
                     │  HTTPS, session cookie + CSRF header
                     ▼
┌──────────────────────────────────────────────┐
│ lectern (Rust, axum)                          │
│  /api/*        JSON API                       │
│  /auth/oidc/*  OpenID Connect redirects       │──── OIDC provider (Keycloak…)
│  /verify/*     public verification pages      │
│  /ob/*         Open Badges documents          │
│  /theme.css    design tokens                  │
│  /branding/*   logo, fonts (instance files)   │
│  /*            built SPA (web/dist)           │
│  background:   e-mail outbox worker,          │──── SMTP
│                expiry alerts, exam timeouts   │
└──────────────┬───────────────────────────────┘
               │ sqlx
               ▼
          PostgreSQL           data dir: uploaded assets (content-addressed)
```

## Choices

- **Rust + axum + sqlx** for the server: memory safety, a small attack surface, a single
  static binary. Queries are plain SQL with bound parameters; dynamic SQL is only
  assembled from compile-time constants (`const_sql!`).
- **React + TypeScript (Vite)** for the front-end, served by the server from the same
  origin: no CORS, strict CSP (`script-src 'self'`, no inline script or style).
- **One process** runs the API and the background jobs. Jobs are safe on several replicas
  (row locks with `SKIP LOCKED`, idempotent alerts), but uploaded assets live on a
  `ReadWriteOnce` volume, so the default deployment uses one replica.
- **Transactional outbox** for e-mails: a result and its notification are committed
  together; a worker delivers with retries and backoff.
- **No third-party runtime dependency** for learners: fonts, logo, badges and certificates
  are served by the instance; videos come from storage you control (MP4 or HLS).

## Code map

| Path | Role |
| --- | --- |
| `server/src/config.rs` | Instance configuration (TOML + `LECTERN__…` environment overrides) |
| `server/src/theme.rs` | Design tokens → `/theme.css` |
| `server/src/auth/` | Sessions, CSRF, e-mail links, OIDC |
| `server/src/domain/exam.rs` | Pure exam rules: paper draw, eligibility, grading (unit-tested) |
| `server/src/domain/attempts.rs` | Exam lifecycle in the database |
| `server/src/domain/certs.rs` | Certifications, validity, requirement levels |
| `server/src/credentials.rs` | Badge SVG/PNG, PDF certificate, Open Badges JSON |
| `server/src/pack.rs` | Content pack parsing, import and export |
| `server/src/jobs.rs` | Expiry alerts, overdue exam sections, housekeeping |
| `server/src/routes/` | HTTP handlers |
| `server/migrations/` | Database schema |
| `web/src/pages/` | Screens; `web/src/i18n/` translations |

## Data model (main tables)

`organizations`, `users`, `memberships` (one organization per user, approved by a
training manager), `user_roles` (platform roles), `tracks` → `modules`, `scenarios` →
`scenario_steps`, `questions` (pools quiz/exam/recert/case), progress tables,
`exam_attempts` (frozen paper, answers, per-section results), `attempt_credits`,
`certifications` (expiry, superseded/revoked), `requirement_levels`, `email_outbox`,
`audit_log`.
