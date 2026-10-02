# Example branding directory

Point `server.branding_dir` at a directory like this one. It holds everything visual
that belongs to an instance and is served by the instance itself under `/branding/`:

- `logo.svg`, `favicon.svg` — referenced by `instance.logo` / `instance.favicon`
- `fonts/` — web fonts declared in `[[theme.fonts]]`, and TTF files for certificates
- `locales/<lang>.json` — overrides of interface texts (front-end)
- `locales/<lang>.toml` — overrides of server texts (e-mails, certificates, verification page)
- `templates/` — optional replacements for `email.html`, `verify.html`, `badge.svg`, `server.css`
- certificate artwork: `logo.png`, `signature.png` (see `[certificates]`)
