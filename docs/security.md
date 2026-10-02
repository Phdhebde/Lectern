# Security

The academy is published by a security vendor: it applies the practices it teaches.

## Authentication and sessions

- OpenID Connect, authorization code + PKCE, nonce and access-token hash verified; the
  HTTP client never follows redirects (SSRF).
- Passwordless e-mail sign-in: single-use 256-bit tokens, stored hashed (SHA-256), valid
  15 minutes, rate-limited per address; the link carries the token in the URL fragment and
  the page POSTs it, so mail scanners prefetching links cannot consume it. The response
  is identical whether the address exists or not.
- Sessions: random 256-bit tokens in an `HttpOnly`, `Secure`, `SameSite=Lax` cookie; only
  their hash is stored; server-side expiry and logout.
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

## Development

- CI: `cargo fmt`, `clippy -D warnings`, unit and API tests on PostgreSQL, ESLint,
  TypeScript, Vitest, `cargo audit` (RustSec) and `npm audit`, brand check, image build.
- Code review on every change (see `CONTRIBUTING.md`).
- Before each production release: run the test suite, review the dependency audit, and run
  a dynamic scan (e.g. OWASP ZAP baseline) against a staging instance.

Report vulnerabilities as described in [SECURITY.md](../SECURITY.md).
