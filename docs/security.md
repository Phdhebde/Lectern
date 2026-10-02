# Security

The academy is published by a security vendor: it applies the practices it teaches.

## Authentication and sessions

- OpenID Connect, authorization code + PKCE, nonce and access-token hash verified; the
  HTTP client never follows redirects (SSRF).
- Passwordless e-mail sign-in: single-use 256-bit tokens, stored hashed (SHA-256), valid
  15 minutes, rate-limited per address; the link carries the token in the URL fragment and
  the page POSTs it, so mail scanners prefetching links cannot consume it. The response
  is identical whether the address exists or not.
- Sessions: random 256-bit tokens in an `HttpOnly`, `Secure`, `SameSite=Lax` cookie named
  `__Host-lectern_session` over HTTPS (the prefix forbids `Domain` and requires `Secure` and
  `Path=/`, so a sibling sub-domain cannot plant a session); only their hash is stored;
  server-side expiry and logout.
- Rate limiting: per address and instance-wide limits on sign-in e-mails in the
  application; Traefik limits for the whole site and stricter ones for sign-in routes
  (`deploy/k8s/overlays/example/ratelimit.yaml`).
- Display names are stripped of Markdown/HTML syntax characters before being stored, as
  they are interpolated into e-mails.
- MFA is required for the roles listed in `auth.require_mfa_for` (trainer, admin by
  default): the session must come from an OIDC sign-in whose `acr`/`amr` denotes MFA.

## Request forgery and injection

- CSRF: synchronizer token (`X-CSRF-Token` header, compared in constant time) on every
  state-changing request, plus rejection of cross-origin `Origin` headers.
- SQL: bound parameters only; dynamic SQL limited to compile-time constant fragments.
- XSS: Markdown is rendered and sanitized server-side (ammonia: allow-listed tags and URL
  schemes, `rel="noopener noreferrer"`); React escapes everything else; strict CSP
  without `unsafe-inline`; templates auto-escape.
- Content packs: zip-slip protection, uncompressed size limit, all-or-nothing validation.
- Uploads: content-type allow-list, content-addressed storage, non-inline types served as
  attachments, `X-Content-Type-Options: nosniff`.
- CSV exports neutralize spreadsheet formulas.

## Headers

`Content-Security-Policy` (`default-src 'self'`, `frame-ancestors 'none'`,
`object-src 'none'`, `base-uri 'none'`), `Strict-Transport-Security` (when
`secure_cookies`), `X-Frame-Options: DENY`, `Referrer-Policy`, `Permissions-Policy`,
`Cross-Origin-Opener-Policy`, `Cache-Control: no-store` on the API.

## Authorization and isolation

- Every handler checks the role it needs; training managers only query rows of the
  organization they manage; restricted tracks answer 404 to non-members.
- Exam answer keys and explanations never reach learners; exam-only screenshots are only
  served to staff and to the candidate whose open attempt contains them.
- Evaluators cannot review their own attempt; administrators cannot remove their own
  admin role.
- API tokens for integrations are random, shown once, stored hashed, revocable.

## Audit

`audit_log` records sign-ins, administration actions (organizations, roles, credits,
revocations, content changes, pack imports/exports, API tokens), membership decisions,
exam starts and results, evaluations.

## Supply chain

- `cargo-deny` (`deny.toml`) fails the build on any RustSec vulnerability, unmaintained
  or unsound crate (direct or transitive), yanked release, non-permissive license, crate
  from outside crates.io, or OpenSSL/native-tls. The only exception (RUSTSEC-2023-0071,
  unreachable code path) is justified in the file.
- Minimal features: e.g. the zip reader only supports deflate (no bzip2, lzma, zstd,
  ppmd, AES code exposed to uploaded archives); PDF text uses `skrifa` instead of
  unmaintained shaping crates.
- `npm audit` (all dependencies, moderate and above) and `npm audit signatures`;
  `npm ci --ignore-scripts` everywhere (no install scripts run).
- GitHub Actions pinned to commit SHAs, read-only `GITHUB_TOKEN`, no persisted
  credentials; container base images pinned by digest; Dependabot updates Cargo, npm,
  Actions and Docker weekly.
- Runtime image: distroless (no shell, no package manager), non-root user 65532.
- CI scans the built image (Trivy, high/critical fail the build), scans the Dockerfile and
  Kubernetes manifests for misconfigurations, and produces an SPDX SBOM. CodeQL
  (security-extended) analyses Rust, TypeScript and the workflows.

## Development

- CI: `cargo fmt`, `clippy -D warnings`, unit and API tests on PostgreSQL, ESLint,
  TypeScript, Vitest, supply-chain checks above, brand check, image build and scan.
- Code review on every change (see `CONTRIBUTING.md`).
- Before each production release: run the test suite, review the dependency audit, and run
  a dynamic scan (e.g. OWASP ZAP baseline) against a staging instance.

Report vulnerabilities as described in [SECURITY.md](../SECURITY.md).
