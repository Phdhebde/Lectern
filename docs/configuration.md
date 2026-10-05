# Installation and configuration

## Requirements

- PostgreSQL 14 or later, UTF-8 (CI tests on 16; the Docker Compose stack ships 18).
- An SMTP relay for e-mails (sign-in links, alerts).
- Optionally an OpenID Connect provider (e.g. Keycloak) — recommended, and required to
  grant MFA-protected roles (trainer, admin).
- A volume for uploaded assets (screenshots, attachments).
- Storage serving videos (object storage or a streaming server; MP4 or HLS).

## Configuration file

One TOML file per instance (see [`config/lectern.example.toml`](../config/lectern.example.toml)),
passed with `--config` or `LECTERN_CONFIG`. Every key can be overridden by an environment
variable `LECTERN__<SECTION>__<KEY>` (double underscores; values parsed as TOML when
possible), e.g. `LECTERN__AUTH__SESSION_HOURS=8`. `DATABASE_URL` sets `database.url`.

**Secrets never go in the file:** `DATABASE_URL`, `LECTERN__AUTH__OIDC__CLIENT_SECRET`,
`LECTERN__MAIL__SMTP_URL`.

| Section | Key | Meaning |
| --- | --- | --- |
| `instance` | `name`, `product_name` | Academy name; name of the product being taught (texts) |
| | `public_url` | Absolute URL without trailing slash (links, badges, CSRF origin check) |
| | `product_major_version` | Recorded on certifications (see [certification](certification.md)) |
| | `contact_email`, `legal_notice_url`, `privacy_policy_url`, `documentation_url` | Footer and e-mails; modules' `doc_url` are relative to `documentation_url` |
| | `logo`, `favicon` | Files in the branding directory |
| | `default_locale` | `fr` or `en` |
| `theme.tokens` | `color-…`, `radius`, `font-…` | Design tokens, see [customization](customization.md) |
| `theme.fonts` | `family`, `src`, `weight`, `style` | Self-hosted web fonts |
| `server` | `bind` | Listen address (default `0.0.0.0:8080`) |
| | `static_dir`, `data_dir`, `branding_dir` | Built front-end, uploaded assets, branding files |
| | `secure_cookies` | `true` in production (HTTPS): HSTS and `__Host-` session cookie. Session cookies are always `Secure`; for development use `http://localhost` |
| | `media_origins` | Origins serving videos, added to the CSP |
| | `max_upload_mb` | Upload limit (default 50) |
| `auth` | `email_login` | Passwordless e-mail sign-in |
| | `session_hours` | Session lifetime (default 12) |
| | `require_mfa_for` | Roles that need an MFA session (default trainer, admin) |
| `auth.oidc` | `issuer_url`, `client_id`, `client_secret`, `scopes`, `label` | OIDC client (authorization code + PKCE). Redirect URI: `<public_url>/auth/oidc/callback` |
| | `mfa_acr_values`, `mfa_amr_values` | ID-token `acr`/`amr` values meaning MFA |
| | `roles_claim` | Dotted path of a claim with platform roles, e.g. `resource_access.lectern.roles`; when set, roles are synchronized at each sign-in |
| `mail` | `from`, `smtp_url` | `smtps://user:pass@host:465` or `smtp://host:587` (STARTTLS). Unset: e-mails are logged |
| `certificates` | `font`, `font_bold`, `logo`, `signature_image`, `signatory_name`, `signatory_title` | PDF certificate template |
| `alerts` | `expiry_days` | Alert thresholds before expiry (default 90, 30, 7) |
| | `recert_window_days` | Recertification opens this many days before expiry (90) |
| | `major_version_grace_days` | Grace period after a new major version (90) |

Check a configuration without starting: `lectern --config lectern.toml check-config`.

## Keycloak

1. Create a confidential client `lectern`, standard flow only, redirect URI
   `https://academy.example.com/auth/oidc/callback`, PKCE S256.
2. To enforce MFA for staff, use an authentication flow with OTP/WebAuthn and map it to an
   `acr` level (e.g. `gold`), then set `mfa_acr_values = ["gold"]`; or rely on the `amr`
   claim if your setup emits it.
3. Optionally create client roles `admin`, `trainer`, `channel_manager`, add a mapper
   exposing them in the ID token, and set `roles_claim`.

The e-mail claim must be verified by the provider (`email_verified`): an identity is
linked to an existing account by e-mail on first sign-in.

## Commands

```
lectern serve                     # default: migrations, background jobs, HTTP server
lectern migrate
lectern import-pack <dir|zip>
lectern export-pack <out.zip>
lectern grant-role <email> <admin|trainer|channel_manager>
lectern check-config
```

## First start

1. Deploy (see [operations](operations.md)), then `lectern grant-role you@company.com admin`.
2. Import your content pack.
3. In *Administration › Organizations*, create organizations and add a training manager
   to each; they share their join code with their colleagues.
