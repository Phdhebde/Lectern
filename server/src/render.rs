//! Server-side rendering: translated strings, Markdown and HTML templates.
//!
//! Strings live in `locales/<lang>.toml` (embedded) and can be overridden per instance
//! with `<branding_dir>/locales/<lang>.toml`. Templates are embedded too and can be
//! overridden with `<branding_dir>/templates/<name>`. No brand name appears in code:
//! templates receive the instance configuration and theme as context.

use std::collections::BTreeMap;

use minijinja::{Environment, Value as JValue, context};
use pulldown_cmark::{Options, Parser, html};
use serde::Serialize;

use crate::config::Config;
use crate::theme::Theme;

const EMBEDDED_LOCALES: &[(&str, &str)] =
    &[("fr", include_str!("../locales/fr.toml")), ("en", include_str!("../locales/en.toml"))];

const EMBEDDED_TEMPLATES: &[(&str, &str)] = &[
    ("email.html", include_str!("../templates/email.html")),
    ("verify.html", include_str!("../templates/verify.html")),
    ("badge.svg", include_str!("../templates/badge.svg")),
    ("server.css", include_str!("../templates/server.css")),
];

pub struct Renderer {
    env: Environment<'static>,
    strings: BTreeMap<String, String>,
    pub locale: String,
}

impl Renderer {
    pub fn new(config: &Config, theme: &Theme) -> anyhow::Result<Self> {
        let locale = config.instance.default_locale.clone();
        let mut strings = BTreeMap::new();
        // English is the fallback for keys missing in the instance locale.
        for lang in ["en", locale.as_str()] {
            if let Some((_, raw)) = EMBEDDED_LOCALES.iter().find(|(l, _)| *l == lang) {
                flatten("", &raw.parse::<toml::Table>()?, &mut strings);
            }
            let override_path = config.server.branding_dir.join("locales").join(format!("{lang}.toml"));
            if override_path.exists() {
                let raw = std::fs::read_to_string(&override_path)?;
                flatten("", &raw.parse::<toml::Table>()?, &mut strings);
            }
        }

        let mut env = Environment::new();
        env.set_auto_escape_callback(|name| {
            if name.ends_with(".html") || name.ends_with(".svg") {
                minijinja::AutoEscape::Html
            } else {
                minijinja::AutoEscape::None
            }
        });
        for (name, src) in EMBEDDED_TEMPLATES {
            let custom = config.server.branding_dir.join("templates").join(name);
            let source = if custom.exists() { std::fs::read_to_string(&custom)? } else { (*src).to_string() };
            env.add_template_owned(*name, source)?;
        }
        env.add_global("instance", JValue::from_serialize(&config.instance));
        env.add_global("theme", JValue::from_serialize(&theme.tokens));
        env.add_global("theme_css", theme.css.clone());
        let table = strings.clone();
        env.add_function("t", move |key: String| -> String { table.get(&key).cloned().unwrap_or(key) });
        env.add_filter("markdown", |s: String| JValue::from_safe_string(markdown(&s)));
        Ok(Self { env, strings, locale })
    }

    /// Translated string, with `{{ var }}` placeholders rendered from `ctx`.
    pub fn text(&self, key: &str, ctx: &impl Serialize) -> anyhow::Result<String> {
        let raw = self.strings.get(key).map(String::as_str).unwrap_or(key);
        Ok(self.env.render_str(raw, ctx)?)
    }

    pub fn raw<'a>(&'a self, key: &'a str) -> &'a str {
        self.strings.get(key).map(String::as_str).unwrap_or(key)
    }

    pub fn template(&self, name: &str, ctx: impl Serialize) -> anyhow::Result<String> {
        Ok(self.env.get_template(name)?.render(ctx)?)
    }

    /// Renders an e-mail: subject, HTML (themed layout) and plain-text bodies.
    pub fn email(&self, kind: &str, ctx: &serde_json::Value) -> anyhow::Result<(String, String, String)> {
        let subject = self.text(&format!("email.{kind}.subject"), ctx)?;
        let body_md = self.text(&format!("email.{kind}.body"), ctx)?;
        let html = self.template(
            "email.html",
            context! { subject => &subject, body => JValue::from_safe_string(markdown(&body_md)) },
        )?;
        Ok((subject, html, body_md))
    }
}

fn flatten(prefix: &str, table: &toml::Table, out: &mut BTreeMap<String, String>) {
    for (k, v) in table {
        let key = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
        match v {
            toml::Value::Table(t) => flatten(&key, t, out),
            toml::Value::String(s) => {
                out.insert(key, s.clone());
            }
            other => {
                out.insert(key, other.to_string());
            }
        }
    }
}

/// Markdown to sanitized HTML. Content is written by staff, but sanitizing keeps a
/// compromised author account or a malicious content pack from injecting scripts.
pub fn markdown(src: &str) -> String {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(src, opts));
    ammonia::Builder::default()
        .link_rel(Some("noopener noreferrer"))
        .url_schemes(["http", "https", "mailto"].into())
        .clean(&out)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_is_sanitized() {
        let html = markdown("Hello <script>alert(1)</script> [x](javascript:alert(1)) **bold**");
        assert!(!html.contains("<script"));
        assert!(!html.contains("javascript:"));
        assert!(html.contains("<strong>bold</strong>"));
    }

    #[test]
    fn locales_have_same_keys() {
        let mut fr = BTreeMap::new();
        let mut en = BTreeMap::new();
        flatten("", &EMBEDDED_LOCALES[0].1.parse().unwrap(), &mut fr);
        flatten("", &EMBEDDED_LOCALES[1].1.parse().unwrap(), &mut en);
        let fr_keys: Vec<_> = fr.keys().collect();
        let en_keys: Vec<_> = en.keys().collect();
        assert_eq!(fr_keys, en_keys);
    }
}
