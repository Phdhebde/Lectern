//! Instance theme: design tokens rendered as CSS custom properties.
//!
//! Components never hard-code colours; they use `var(--color-primary)` and friends.
//! The same tokens feed the e-mails, badges and PDF certificates.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::config::Config;

/// Defaults for every token the interface relies on. Instances override any subset.
pub const DEFAULT_TOKENS: &[(&str, &str)] = &[
    ("color-primary", "#2f5bea"),
    ("color-primary-contrast", "#ffffff"),
    ("color-accent", "#f59e0b"),
    ("color-bg", "#f7f8fa"),
    ("color-surface", "#ffffff"),
    ("color-text", "#1b1f29"),
    ("color-muted", "#5b6475"),
    ("color-border", "#dde1e8"),
    ("color-success", "#1a7f4b"),
    ("color-warning", "#a15c00"),
    ("color-danger", "#c62828"),
    ("color-annotation", "#e5007d"),
    ("color-annotation-contrast", "#ffffff"),
    ("radius", "10px"),
    ("radius-small", "6px"),
    ("space", "16px"),
    ("max-width", "1100px"),
    ("font-body", "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"),
    ("font-heading", "system-ui, -apple-system, 'Segoe UI', Roboto, sans-serif"),
    ("font-mono", "ui-monospace, 'SFMono-Regular', Menlo, Consolas, monospace"),
];

#[derive(Debug, Clone, Serialize)]
pub struct Theme {
    pub tokens: BTreeMap<String, String>,
    pub css: String,
}

impl Theme {
    pub fn from_config(config: &Config) -> Self {
        let mut tokens: BTreeMap<String, String> =
            DEFAULT_TOKENS.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        for (k, v) in &config.theme.tokens {
            tokens.insert(k.clone(), v.clone());
        }
        let mut css = String::new();
        for font in &config.theme.fonts {
            let format = match font.src.rsplit('.').next() {
                Some("woff2") => "woff2",
                Some("woff") => "woff",
                Some("otf") => "opentype",
                _ => "truetype",
            };
            css.push_str(&format!(
                "@font-face{{font-family:\"{}\";src:url(\"/branding/{}\") format(\"{}\");font-weight:{};font-style:{};font-display:swap}}\n",
                css_escape(&font.family),
                css_escape(&font.src),
                format,
                css_escape(&font.weight),
                css_escape(&font.style)
            ));
        }
        css.push_str(":root{");
        for (k, v) in &tokens {
            css.push_str(&format!("--{k}:{v};"));
        }
        css.push_str("}\n");
        Self { tokens, css }
    }

    pub fn token(&self, name: &str) -> &str {
        self.tokens.get(name).map(String::as_str).unwrap_or("#000000")
    }

    /// Parses a `#rrggbb` token into RGB components (for PDFs). Falls back to black.
    pub fn rgb(&self, name: &str) -> (u8, u8, u8) {
        parse_hex(self.token(name)).unwrap_or((0, 0, 0))
    }
}

pub fn parse_hex(value: &str) -> Option<(u8, u8, u8)> {
    let hex = value.trim().strip_prefix('#')?;
    let hex = match hex.len() {
        3 => hex.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => hex.to_string(),
        _ => return None,
    };
    let n = u32::from_str_radix(&hex, 16).ok()?;
    Some(((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

fn css_escape(s: &str) -> String {
    s.chars().filter(|c| !matches!(c, '"' | '\\' | '\n' | '{' | '}' | ';' | '<')).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_colors() {
        assert_eq!(parse_hex("#2f5bea"), Some((0x2f, 0x5b, 0xea)));
        assert_eq!(parse_hex("#fff"), Some((255, 255, 255)));
        assert_eq!(parse_hex("red"), None);
    }
}
