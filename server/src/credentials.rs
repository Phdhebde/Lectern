//! Badges (SVG/PNG), PDF certificates and Open Badges 2.0 documents.
//!
//! All artwork derives from the instance theme and configuration: colours from the
//! design tokens, fonts and logo from the branding directory.

use std::path::Path;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Utc};
use krilla::color::rgb;
use krilla::geom::{PathBuilder, Point, Rect, Size, Transform};
use krilla::image::Image;
use krilla::page::PageSettings;
use krilla::paint::{Fill, Stroke};
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::{Data, Document};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use uuid::Uuid;

use crate::state::AppState;
use crate::theme::parse_hex;

const DEFAULT_FONT: &[u8] = include_bytes!("../assets/fonts/DejaVuSans.ttf");
const DEFAULT_FONT_BOLD: &[u8] = include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf");

/// Per-track badge design (`tracks.badge`).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BadgeDesign {
    /// Large text in the middle of the badge. Defaults to the track title.
    #[serde(default)]
    pub label: Option<String>,
    /// Text in the ribbon.
    #[serde(default)]
    pub subtitle: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub accent: Option<String>,
    /// Ready-made artwork (SVG/PNG asset id) replacing the generated badge.
    #[serde(default)]
    pub image_asset: Option<Uuid>,
}

pub struct BadgeInput<'a> {
    pub track_title: &'a str,
    pub design: &'a BadgeDesign,
    pub year: Option<i32>,
}

pub fn badge_svg(state: &AppState, input: &BadgeInput) -> anyhow::Result<String> {
    let theme = &state.theme;
    let color = input.design.color.clone().unwrap_or_else(|| theme.token("color-primary").to_string());
    let accent = input.design.accent.clone().unwrap_or_else(|| theme.token("color-accent").to_string());
    let label = input.design.label.clone().unwrap_or_else(|| input.track_title.to_string());
    let title_size = match label.chars().count() {
        0..=8 => 52,
        9..=12 => 40,
        13..=16 => 32,
        _ => 24,
    };
    state.renderer.template(
        "badge.svg",
        json!({
            "track": input.track_title,
            "label": label,
            "subtitle": input.design.subtitle.clone().unwrap_or_default(),
            "issuer": state.config.instance.name,
            "color": color,
            "color_dark": darken(&color, 0.72),
            "accent": accent,
            "accent_text": readable_on(&accent),
            "text_color": readable_on(&color),
            "font": "DejaVu Sans, sans-serif",
            "title_size": title_size,
            "year": input.year,
        }),
    )
}

pub fn svg_to_png(svg: &str, size: u32) -> anyhow::Result<Vec<u8>> {
    let mut opt = resvg::usvg::Options::default();
    let db = opt.fontdb_mut();
    db.load_font_data(DEFAULT_FONT.to_vec());
    db.load_font_data(DEFAULT_FONT_BOLD.to_vec());
    db.set_sans_serif_family("DejaVu Sans");
    let tree = resvg::usvg::Tree::from_str(svg, &opt)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).ok_or_else(|| anyhow::anyhow!("bad size"))?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Ok(pixmap.encode_png()?)
}

fn darken(hex: &str, factor: f32) -> String {
    match parse_hex(hex) {
        Some((r, g, b)) => {
            let f = |c: u8| (c as f32 * factor).round() as u8;
            format!("#{:02x}{:02x}{:02x}", f(r), f(g), f(b))
        }
        None => hex.to_string(),
    }
}

/// Black or white, whichever reads better on the given background (WCAG luminance).
pub fn readable_on(hex: &str) -> &'static str {
    let Some((r, g, b)) = parse_hex(hex) else { return "#ffffff" };
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let l = 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
    if l > 0.45 { "#111111" } else { "#ffffff" }
}

// ---------------------------------------------------------------------------
// PDF certificate
// ---------------------------------------------------------------------------

pub struct CertificateInput<'a> {
    pub id: Uuid,
    pub holder: &'a str,
    pub track_title: &'a str,
    pub issued: DateTime<Utc>,
    pub expires: Option<DateTime<Utc>>,
}

struct LoadedFont {
    font: Font,
    face_data: Arc<Vec<u8>>,
}

impl LoadedFont {
    fn load(branding: &Path, file: Option<&str>, fallback: &'static [u8]) -> anyhow::Result<Self> {
        let data = match file {
            Some(f) => std::fs::read(branding.join(f))?,
            None => fallback.to_vec(),
        };
        let data = Arc::new(data);
        let font = Font::new(Data::from(data.clone()), 0).ok_or_else(|| anyhow::anyhow!("unsupported font"))?;
        Ok(Self { font, face_data: data })
    }

    /// Lays out `text` on one line: one glyph per character from the font's character
    /// map, with its horizontal advance (no kerning or ligatures, which certificates
    /// do not need). Advances are normalized to the em, as krilla expects.
    fn layout(&self, text: &str) -> Vec<KrillaGlyph> {
        use skrifa::MetadataProvider;
        let Ok(face) = skrifa::FontRef::new(&self.face_data) else { return Vec::new() };
        let (size, location) = (skrifa::instance::Size::unscaled(), skrifa::instance::LocationRef::default());
        let upem = f32::from(face.metrics(size, location).units_per_em.max(1));
        let charmap = face.charmap();
        let metrics = face.glyph_metrics(size, location);
        text.char_indices()
            .map(|(i, c)| {
                let gid = charmap.map(c).unwrap_or_default();
                let advance = metrics.advance_width(gid).unwrap_or(upem / 2.0) / upem;
                KrillaGlyph::new(GlyphId::new(gid.to_u32()), advance, 0.0, 0.0, 0.0, i..i + c.len_utf8(), None)
            })
            .collect()
    }

    /// Width of `text` at `size` points.
    fn width(&self, text: &str, size: f32) -> f32 {
        self.layout(text).iter().map(|g| g.x_advance).sum::<f32>() * size
    }

    fn draw(&self, s: &mut krilla::surface::Surface, at: Point, size: f32, text: &str) {
        let glyphs = self.layout(text);
        s.draw_glyphs(at, &glyphs, self.font.clone(), text, size, false);
    }
}

pub fn certificate_pdf(state: &AppState, input: &CertificateInput) -> anyhow::Result<Vec<u8>> {
    let cfg = &state.config.certificates;
    let branding = &state.config.server.branding_dir;
    let regular = LoadedFont::load(branding, cfg.font.as_deref(), DEFAULT_FONT)?;
    let bold = LoadedFont::load(branding, cfg.font_bold.as_deref().or(cfg.font.as_deref()), DEFAULT_FONT_BOLD)?;
    let r = &state.renderer;
    let date_fmt = r.raw("date.format").to_string();
    let fmt = |d: DateTime<Utc>| d.format(&date_fmt).to_string();

    let (w, h) = (842.0f32, 595.0f32); // A4 landscape, points
    let primary = state.theme.rgb("color-primary");
    let accent = state.theme.rgb("color-accent");
    let text = state.theme.rgb("color-text");
    let muted = state.theme.rgb("color-muted");
    let fill = |c: (u8, u8, u8)| Fill { paint: rgb::Color::new(c.0, c.1, c.2).into(), ..Default::default() };

    let mut doc = Document::new();
    let mut page = doc.start_page_with(PageSettings::new(Size::from_wh(w, h).expect("valid size")));
    let mut s = page.surface();

    // Frame
    s.set_fill(None);
    s.set_stroke(Some(Stroke {
        paint: rgb::Color::new(primary.0, primary.1, primary.2).into(),
        width: 6.0,
        ..Default::default()
    }));
    s.draw_path(&rect_path(18.0, 18.0, w - 36.0, h - 36.0));
    s.set_stroke(Some(Stroke {
        paint: rgb::Color::new(accent.0, accent.1, accent.2).into(),
        width: 1.5,
        ..Default::default()
    }));
    s.draw_path(&rect_path(30.0, 30.0, w - 60.0, h - 60.0));
    s.set_stroke(None);

    // Header band
    s.set_fill(Some(fill(primary)));
    s.draw_path(&rect_path(30.0, 30.0, w - 60.0, 70.0));

    if let Some(logo) = cfg.logo.as_deref()
        && let Ok(bytes) = std::fs::read(branding.join(logo))
    {
        let image = if logo.to_ascii_lowercase().ends_with(".png") {
            Image::from_png(Data::from(bytes), true).ok()
        } else {
            Image::from_jpeg(Data::from(bytes), true).ok()
        };
        if let Some(image) = image {
            let (iw, ih) = image.size();
            let lh = 44.0;
            let lw = lh * iw as f32 / ih.max(1) as f32;
            s.push_transform(&Transform::from_translate(50.0, 43.0));
            s.draw_image(image, Size::from_wh(lw, lh).expect("valid size"));
            s.pop();
        }
    }

    let contrast = parse_hex(state.theme.token("color-primary-contrast")).unwrap_or((255, 255, 255));
    s.set_fill(Some(fill(contrast)));
    let name = &state.config.instance.name;
    let size = 20.0;
    bold.draw(&mut s, Point::from_xy(w - 50.0 - bold.width(name, size), 72.0), size, name);

    let centered =
        |s: &mut krilla::surface::Surface, f: &LoadedFont, size: f32, y: f32, txt: &str, color: (u8, u8, u8)| {
            let mut size = size;
            while f.width(txt, size) > w - 120.0 && size > 8.0 {
                size -= 1.0;
            }
            s.set_fill(Some(fill(color)));
            f.draw(s, Point::from_xy((w - f.width(txt, size)) / 2.0, y), size, txt);
        };

    centered(&mut s, &bold, 40.0, 175.0, &r.raw("certificate.title").to_uppercase(), primary);
    centered(&mut s, &regular, 16.0, 225.0, r.raw("certificate.intro"), muted);
    centered(&mut s, &bold, 34.0, 280.0, input.holder, text);
    centered(&mut s, &regular, 16.0, 325.0, r.raw("certificate.achievement"), muted);
    centered(&mut s, &bold, 28.0, 372.0, input.track_title, primary);

    let mut dates = format!("{} {}", r.raw("certificate.issued"), fmt(input.issued));
    if let Some(exp) = input.expires {
        dates.push_str(&format!("   ·   {} {}", r.raw("certificate.expires"), fmt(exp)));
    }
    centered(&mut s, &regular, 13.0, 412.0, &dates, text);

    // Signature block
    let sig_x = w - 290.0;
    if let Some(sig) = cfg.signature_image.as_deref()
        && let Ok(bytes) = std::fs::read(branding.join(sig))
        && let Ok(image) = Image::from_png(Data::from(bytes), true)
    {
        let (iw, ih) = image.size();
        let sh = 50.0;
        let sw = (sh * iw as f32 / ih.max(1) as f32).min(220.0);
        s.push_transform(&Transform::from_translate(sig_x, 440.0));
        s.draw_image(image, Size::from_wh(sw, sh).expect("valid size"));
        s.pop();
    }
    s.set_fill(Some(fill(muted)));
    s.draw_path(&rect_path(sig_x, 495.0, 220.0, 0.8));
    if let Some(n) = &cfg.signatory_name {
        s.set_fill(Some(fill(text)));
        bold.draw(&mut s, Point::from_xy(sig_x, 512.0), 12.0, n);
    }
    if let Some(t) = &cfg.signatory_title {
        s.set_fill(Some(fill(muted)));
        regular.draw(&mut s, Point::from_xy(sig_x, 528.0), 10.0, t);
    }

    // Verification link
    let url = state.config.public_url(&format!("/verify/{}", input.id));
    s.set_fill(Some(fill(muted)));
    regular.draw(&mut s, Point::from_xy(50.0, 512.0), 10.0, r.raw("certificate.verify"));
    regular.draw(&mut s, Point::from_xy(50.0, 528.0), 10.0, &url);

    s.finish();
    page.finish();
    doc.finish().map_err(|e| anyhow::anyhow!("PDF generation failed: {e:?}"))
}

fn rect_path(x: f32, y: f32, w: f32, h: f32) -> krilla::geom::Path {
    let mut pb = PathBuilder::new();
    pb.push_rect(Rect::from_xywh(x, y, w, h).expect("valid rect"));
    pb.finish().expect("non-empty path")
}

// ---------------------------------------------------------------------------
// Open Badges 2.0 (hosted verification)
// ---------------------------------------------------------------------------

pub fn ob_issuer(state: &AppState) -> Value {
    let c = &state.config.instance;
    json!({
        "@context": "https://w3id.org/openbadges/v2",
        "type": "Issuer",
        "id": state.config.public_url("/ob/issuer"),
        "name": c.name,
        "url": c.public_url,
        "email": c.contact_email,
    })
}

pub fn ob_badge_class(state: &AppState, slug: &str, title: &str, summary: &str) -> Value {
    json!({
        "@context": "https://w3id.org/openbadges/v2",
        "type": "BadgeClass",
        "id": state.config.public_url(&format!("/ob/badges/{slug}")),
        "name": title,
        "description": if summary.is_empty() { title } else { summary },
        "image": state.config.public_url(&format!("/ob/badges/{slug}/image.png")),
        "criteria": { "narrative": summary, "id": state.config.public_url(&format!("/tracks/{slug}")) },
        "issuer": state.config.public_url("/ob/issuer"),
    })
}

/// Assertion with a hashed recipient e-mail, so the e-mail is never published.
pub fn ob_assertion(
    state: &AppState,
    cert_id: Uuid,
    slug: &str,
    email: &str,
    issued: DateTime<Utc>,
    expires: Option<DateTime<Utc>>,
    revoked: bool,
) -> Value {
    use sha2::{Digest, Sha256};
    let salt = cert_id.simple().to_string();
    let identity = format!("sha256${}", hex::encode(Sha256::digest(format!("{}{}", email.to_lowercase(), salt))));
    let mut v = json!({
        "@context": "https://w3id.org/openbadges/v2",
        "type": "Assertion",
        "id": state.config.public_url(&format!("/ob/assertions/{cert_id}")),
        "recipient": { "type": "email", "hashed": true, "salt": salt, "identity": identity },
        "badge": state.config.public_url(&format!("/ob/badges/{slug}")),
        "verification": { "type": "hosted" },
        "issuedOn": issued.to_rfc3339(),
        "image": state.config.public_url(&format!("/verify/{cert_id}/badge.png")),
        "evidence": state.config.public_url(&format!("/verify/{cert_id}")),
    });
    if let Some(e) = expires {
        v["expires"] = json!(e.to_rfc3339());
    }
    if revoked {
        v["revoked"] = json!(true);
    }
    v
}

/// "Add to profile" URL for LinkedIn certifications.
pub fn linkedin_add_url(
    state: &AppState,
    cert_id: Uuid,
    track_title: &str,
    issued: DateTime<Utc>,
    expires: Option<DateTime<Utc>>,
) -> String {
    let mut url = url::Url::parse("https://www.linkedin.com/profile/add").expect("static URL");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("startTask", "CERTIFICATION_NAME")
            .append_pair("name", track_title)
            .append_pair("organizationName", &state.config.instance.name)
            .append_pair("issueYear", &issued.year().to_string())
            .append_pair("issueMonth", &issued.month().to_string())
            .append_pair("certUrl", &state.config.public_url(&format!("/verify/{cert_id}")))
            .append_pair("certId", &cert_id.to_string());
        if let Some(e) = expires {
            q.append_pair("expirationYear", &e.year().to_string())
                .append_pair("expirationMonth", &e.month().to_string());
        }
    }
    url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_color() {
        assert_eq!(readable_on("#ffffff"), "#111111");
        assert_eq!(readable_on("#1b1f29"), "#ffffff");
        assert_eq!(darken("#ffffff", 0.5), "#808080");
    }
}
