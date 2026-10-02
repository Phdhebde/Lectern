# Contributing

Thanks for helping! Every change is reviewed before merge, external contributions included.

## Ground rules

- **No brand in the code.** Names, colours and texts belong to instance configuration,
  translation files and content packs. CI rejects brand names (`scripts/check-brand.sh`).
- **No colour in components.** Use theme tokens (`var(--color-…)`); add a token in
  `server/src/theme.rs` if needed.
- **Every interface text goes through translations** (`web/src/i18n/*.json`,
  `server/locales/*.toml`), in every bundled language.
- **Security first**: bound SQL parameters, no `dangerouslySetInnerHTML` except for
  server-sanitized HTML, authorization checked in every handler, tests for access rules.

## Workflow

1. Open an issue to discuss significant changes.
2. Branch, commit with clear messages, open a pull request.
3. Make sure these pass locally:
   ```sh
   cargo fmt --all --check && cargo clippy --all-targets -- -D warnings && cargo test
   cd web && npm run lint && npm run typecheck && npm test && npm run build
   ```
4. A maintainer reviews; security-sensitive changes get a second review.

Database changes are new files in `server/migrations/` (never edit a released migration).
