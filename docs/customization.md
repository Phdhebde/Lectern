# Customization

Everything that identifies a brand is instance configuration. The code contains no brand
name — CI fails if one appears (`scripts/check-brand.sh` with the `BRAND_DENYLIST`
repository variable).

## Colours, radii, fonts: design tokens

`[theme.tokens]` in the configuration. The server renders them as CSS custom properties in
`/theme.css`; the interface, server pages, e-mails, badges and certificates all read them.
Components never contain a colour (an ESLint rule rejects hexadecimal literals).

| Token | Used for |
| --- | --- |
| `color-primary`, `color-primary-contrast` | Buttons, links, header of e-mails and certificates, default badge colour |
| `color-accent` | Highlights, default badge accent, stars |
| `color-bg`, `color-surface`, `color-border` | Page background, cards, separators |
| `color-text`, `color-muted` | Text |
| `color-success`, `color-warning`, `color-danger` | Statuses |
| `color-annotation`, `color-annotation-contrast` | Screenshot annotations |
| `radius`, `radius-small`, `space`, `max-width` | Shapes and layout |
| `font-body`, `font-heading`, `font-mono` | Font stacks |

Fonts are served by the instance: put the files in the branding directory and declare them:

```toml
[[theme.fonts]]
family = "Inter"
src = "fonts/Inter-Variable.woff2"
weight = "100 900"

[theme.tokens]
font-body = "'Inter', system-ui, sans-serif"
```

## Name, logo, links

`[instance]`: `name`, `logo`, `favicon`, `public_url`, `contact_email`, `legal_notice_url`,
`privacy_policy_url`, `documentation_url`.

## Texts

- Interface: bundled in `web/src/i18n/<lang>.json`. Override any key with
  `<branding_dir>/locales/<lang>.json` (same structure, only the keys you change).
  `{instance}` and `{product}` placeholders are replaced everywhere.
- Server texts (e-mails, verification page, certificates): `server/locales/<lang>.toml`,
  overridden by `<branding_dir>/locales/<lang>.toml`. Values are templates
  (`{{ instance.name }}`, `{% if … %}`).

## E-mails, verification page, badges

Templates `email.html`, `verify.html`, `badge.svg` and `server.css` are embedded and can
be replaced by files of the same name in `<branding_dir>/templates/`. They receive the
instance configuration (`instance`) and the theme tokens (`theme`).

Each track's badge is generated from `[badge]` in `track.toml` (label, ribbon, colours).
Badge images are SVG for the web and PNG (600×600) for Open Badges and link previews.

## Certificates (PDF)

```toml
[certificates]
font = "fonts/Certificate-Regular.ttf"   # TTF/OTF, defaults to the bundled DejaVu Sans
font_bold = "fonts/Certificate-Bold.ttf"
logo = "logo.png"                        # PNG or JPEG
signature_image = "signature.png"
signatory_name = "Jane Doe"
signatory_title = "Head of Academy"
```

Colours come from the theme tokens; texts from the `certificate.*` server strings.
