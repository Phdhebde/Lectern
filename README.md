# Lectern

White-label academy for software vendors: train and certify partners and customers with
courses, hands-on scenarios, exams and verifiable badges.

One instance = one brand. Everything a learner sees — name, colours, fonts, logo, texts,
e-mails, certificates, badges — comes from the instance configuration. The platform ships
without any content: tracks, scenarios and question banks are imported as a
[content pack](docs/content-pack.md).

## Features

| Area | What you get |
| --- | --- |
| Accounts | Passwordless e-mail sign-in, OpenID Connect (e.g. Keycloak) with MFA detection, organizations (partner / customer / individual) joined with a code and approved by a training manager, platform roles (admin, trainer, channel manager) |
| Catalog | Tracks with audiences (public / partner / customer), prerequisites, modules (video, recap sheet, attachments, quiz with correction), resume where you left off |
| Hands-on scenarios | Step-by-step reader with annotated screenshots (boxes, arrows, numbered markers), expected results, pitfalls, verification questions |
| Exams | Randomized draw from question banks (minimum bank size enforced), shuffled answers, timed sections, case studies that differ between attempts, attempts policy (free attempts, cooldown, extra credits), one exam at a time, written answers graded by an evaluator |
| Credentials | Certifications with expiry, recertification window, PDF certificate, badge (SVG/PNG), public verification page, Open Badges 2.0 hosted assertions, LinkedIn "add to profile" |
| Alerts | E-mails at 90/30/7 days before expiry to the learner and their training manager, sign-in links, results, membership decisions, new content |
| Organizations | Training manager dashboard (members, progress, valid certifications, gap with requirement levels), channel manager view, CSV export and API for the partner portal |
| Administration | Content editor (tracks, modules, scenarios with visual annotation editor, questions), pack import/export, users and roles, extra attempts, statistics (completion, pass rate per question with anomaly flag), audit log, major-version declaration |
| Privacy | Data export and account deletion from the profile, private verification pages, anonymous per-question statistics |

## Architecture

```
web/      React + TypeScript single-page app (Vite)
server/   Rust API server (axum, PostgreSQL via sqlx), serves the built web app
config/   Example instance configuration
examples/ Fictional demo content pack and branding directory
deploy/   Kubernetes manifests (Kustomize) for K3s / GitOps
docs/     Documentation
```

See [docs/architecture.md](docs/architecture.md).

## Quick start (development)

Requirements: Rust (stable), Node.js 22, PostgreSQL 16 (or `docker compose up -d`).

```sh
# 1. Database
docker compose up -d db          # or any local PostgreSQL
export DATABASE_URL=postgres://lectern:lectern@localhost/lectern

# 2. Demo content and an administrator
cargo run -p lectern-server -- --config config/lectern.example.toml import-pack examples/demo-pack
cargo run -p lectern-server -- --config config/lectern.example.toml grant-role you@example.com admin

# 3. Server (http://localhost:8080). MFA is not available with e-mail sign-in, so relax it locally:
LECTERN__AUTH__REQUIRE_MFA_FOR='[]' LECTERN__SERVER__BRANDING_DIR=examples/branding \
  cargo run -p lectern-server -- --config config/lectern.example.toml

# 4. Front-end with hot reload (http://localhost:5173, proxies the API)
cd web && npm install && npm run dev
```

Without SMTP configuration, e-mails (including sign-in links) are printed in the server log.

## Tests

```sh
cargo test                      # unit tests + API tests (needs DATABASE_URL)
cd web && npm run lint && npm run typecheck && npm test
```

## Documentation

- [Installation and configuration](docs/configuration.md)
- [Customization (theme, texts, e-mails, certificates, badges)](docs/customization.md)
- [Content pack format](docs/content-pack.md)
- [Exams, certifications and requirements](docs/certification.md)
- [Operations: deployment, backups, monitoring](docs/operations.md)
- [Security](docs/security.md) and [personal data](docs/privacy.md)
- [HTTP API](docs/api.md)

## License

See [LICENSE](LICENSE). Security issues: see [SECURITY.md](SECURITY.md).
