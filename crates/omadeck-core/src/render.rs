//! Renders a key face (background + icon + label) to an RGBA bitmap.
//!
//! The same renderer drives both the physical deck and the previews in the configurator,
//! so what you see in the app is exactly what lands on the hardware.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use image::Rgba;
pub use image::RgbaImage;
use image::imageops::{self, FilterType};

use crate::config::{AudioDevice, Config, DialConfig, KeyConfig, Style};
use crate::paths;
use crate::theme::{Rgb, Theme};

/// The look fields a face (key or dial panel) is drawn from, so the drawing code is
/// written once. Built with `KeyConfig::face()` / `DialConfig::face()`.
pub struct Face<'a> {
    pub label: &'a str,
    pub show_label: bool,
    pub icon: Option<&'a str>,
    pub glyph: Option<&'a str>,
    pub background: Option<&'a str>,
    pub label_color: Option<&'a str>,
}

/// Live state of one dial for its strip panel.
#[derive(Debug, Clone, Default)]
pub struct DialStatus {
    /// The dial is pressed (or a tap is still flashing).
    pub pressed: bool,
    /// A value to show in the `$B1` panel layout (volume, PR 4).
    pub level: Option<Level>,
}

/// A value for the `$B1` layout: a bar position plus the text next to it (e.g. `40%`).
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    /// 0.0..=1.0, for the bar.
    pub value: f32,
    pub text: String,
    /// Mute: the text reads `Muted` and the bar is left empty.
    pub muted: bool,
}

/// One dial panel of the Stream Deck + touch strip, in native 200×100 units.
///
/// The rectangles are Elgato's built-in panel layouts, `$X1` "Icon" and `$B1` "Indicator":
/// <https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout>
pub const PANEL_W: u32 = 200;
pub const PANEL_H: u32 = 100;
/// `$X1`/`$B1` title: 16 px, weight 600, left-aligned in both (per the Elgato SDK).
const X1_TITLE: (u32, u32, u32, u32) = (16, 10, 136, 24);
/// `$X1` icon: centred in the panel.
const X1_ICON: (u32, u32, u32, u32) = (76, 40, 48, 48);
// The `$B1` "Indicator" layout, drawn by a volume dial.
/// Icon: on the left, room for the value and bar to its right.
const B1_ICON: (u32, u32, u32, u32) = (16, 40, 48, 48);
/// Value text: 24 px, weight 600, right-aligned (e.g. `40%`).
const B1_VALUE: (u32, u32, u32, u32) = (76, 40, 108, 32);
/// Indicator: groove bar, rounded ends, range 0–100.
const B1_BAR: (u32, u32, u32, u32) = (76, 74, 108, 12);
/// The panel icon box, in native pixels.
const STRIP_ICON_PX: u32 = 48;
/// The panel title size, in native pixels.
const STRIP_TITLE_PX: f32 = 16.0;
/// The `$B1` value text size, in native pixels.
const B1_VALUE_PX: f32 = 24.0;
/// Default panel glyphs for a volume dial with no icon or glyph of its own.
const OUTPUT_GLYPH: &str = "f057e"; // 󰕾 speaker
const INPUT_GLYPH: &str = "f036c"; // 󰎬 microphone
const MUTE_GLYPH: &str = "f075f"; // 󰝟 muted

/// Which dial panel a touch at `x` falls in, for a strip `width` pixels wide with
/// `dials` panels. Out of range (or no dials) is `None`.
pub fn tap_to_dial(x: u32, width: u32, dials: usize) -> Option<usize> {
    (dials > 0 && width > 0 && x < width).then(|| (x * dials as u32 / width) as usize)
}

/// Horizontal alignment inside a text rectangle.
#[derive(Debug, Clone, Copy)]
pub enum Align {
    Left,
    Center,
    Right,
}

pub struct Renderer {
    font: Option<FontVec>,
    font_pattern: Option<String>,
    pub theme: Theme,
    icons: HashMap<(PathBuf, u32), Option<RgbaImage>>,
}

impl Renderer {
    pub fn new(style: &Style) -> Self {
        let mut r = Self { font: None, font_pattern: None, theme: Theme::default(), icons: HashMap::new() };
        r.update_style(style);
        r.theme = Theme::load_omarchy();
        r
    }

    /// Re-resolve the font if the style asks for a different one.
    pub fn update_style(&mut self, style: &Style) {
        if self.font.is_none() || self.font_pattern != style.font {
            self.font_pattern = style.font.clone();
            self.font = load_font(style.font.as_deref().unwrap_or("monospace"));
            if self.font.is_none() {
                log::warn!("no usable font found; labels will not be drawn");
            }
        }
    }

    pub fn reload_theme(&mut self) {
        self.theme = Theme::load_omarchy();
    }

    /// Drop cached icons (call after the config changes; files may have been replaced).
    pub fn clear_cache(&mut self) {
        self.icons.clear();
    }

    pub fn background(&self, face: Option<&Face>, style: &Style) -> Rgb {
        face.and_then(|f| f.background)
            .and_then(Rgb::parse)
            .or_else(|| style.background.as_deref().and_then(Rgb::parse))
            .unwrap_or(if style.follow_theme { self.theme.background } else { Rgb(0, 0, 0) })
    }

    pub fn label_color(&self, face: Option<&Face>, style: &Style, bg: Rgb) -> Rgb {
        if let Some(c) = face.and_then(|f| f.label_color).and_then(Rgb::parse) {
            return c;
        }
        if let Some(c) = style.label_color.as_deref().and_then(Rgb::parse) {
            return c;
        }
        let fg = if style.follow_theme { self.theme.foreground } else { Rgb(255, 255, 255) };
        // Keep the label readable on custom backgrounds.
        if (fg.luminance() - bg.luminance()).abs() < 0.35 {
            if bg.luminance() > 0.5 { Rgb(16, 16, 16) } else { Rgb(245, 245, 245) }
        } else {
            fg
        }
    }

    /// Render one key face at `size`×`size`.
    pub fn render(&mut self, key: Option<&KeyConfig>, style: &Style, size: u32, pressed: bool) -> RgbaImage {
        let key = key.filter(|k| !k.is_blank());
        let face = key.as_ref().map(|k| k.face());
        let bg = self.background(face.as_ref(), style);
        let out = match key {
            None => solid(size, bg.mix(Rgb(0, 0, 0), 0.55)),
            Some(k) => self.render_face(&k.face(), style, size, bg),
        };
        if pressed { self.pressed(out, bg, size) } else { out }
    }

    fn render_face(&mut self, face: &Face, style: &Style, size: u32, bg: Rgb) -> RgbaImage {
        let mut img = solid(size, bg);
        let s = size as f32;
        let want_label = face.show_label && !face.label.is_empty();
        let fg = self.label_color(Some(face), style, bg);

        let box_px = if want_label { (s * 0.62) as u32 } else { (s * 0.80) as u32 };
        let icon = match (face.icon, face.glyph.filter(|g| !g.trim().is_empty())) {
            (Some(p), _) => self.icon(&paths::resolve(p), box_px),
            (None, Some(g)) => {
                let color = face.label_color.and_then(Rgb::parse).unwrap_or(if style.follow_theme {
                    self.theme.accent
                } else {
                    fg
                });
                self.glyph(g, (box_px as f32 * 0.86) as u32, color)
            }
            _ => None,
        };

        match (&icon, want_label) {
            (Some(icon), true) => {
                let x = (size - icon.width()) / 2;
                let area = s * 0.70;
                let y = ((area - icon.height() as f32) / 2.0 + s * 0.04).max(0.0) as u32;
                imageops::overlay(&mut img, icon, x as i64, y as i64);
                self.draw_line(&mut img, face.label, s * 0.17, s * 0.86, fg, true);
            }
            (Some(icon), false) => {
                let x = (size - icon.width()) / 2;
                let y = (size - icon.height()) / 2;
                imageops::overlay(&mut img, icon, x as i64, y as i64);
            }
            (None, true) => self.draw_block(&mut img, face.label, fg),
            (None, false) => {}
        }
        img
    }

    // --- touch strip ----------------------------------------------------------------------

    /// Render the whole touch strip (Stream Deck +: 800×100, four 200×100 dial panels),
    /// scaled to `size`. `status` has one entry per physical dial; unconfigured dials
    /// draw as empty slots. A tall `size` (the Plus XL) is not supported yet: a plain
    /// background is returned and the limitation logged once.
    pub fn render_strip(&mut self, cfg: &Config, status: &[DialStatus], size: (u32, u32)) -> RgbaImage {
        if size.1 > size.0 {
            static TALL: std::sync::Once = std::sync::Once::new();
            TALL.call_once(|| log::warn!("tall touch strips (Stream Deck + XL) are not drawn yet"));
            return solid_rect(size.0, size.1, self.theme.background);
        }
        let panels = status.len().max(1);
        let native = (PANEL_W * panels as u32, PANEL_H);
        let mut img = RgbaImage::new(native.0, native.1);
        for i in 0..panels {
            let panel =
                self.render_strip_panel(cfg.dial(i as u8), &cfg.style, status.get(i).cloned().unwrap_or_default());
            imageops::overlay(&mut img, &panel, (i as u32 * PANEL_W) as i64, 0);
        }
        if size == native { img } else { imageops::resize(&img, size.0, size.1, FilterType::Triangle) }
    }

    /// Render one 200×100 dial panel at native size. A volume dial uses Elgato's `$B1`
    /// "Indicator" layout (icon left, value and groove bar right); everything else uses
    /// `$X1` "Icon" (title left, icon centred).
    pub fn render_strip_panel(&mut self, dial: Option<&DialConfig>, style: &Style, st: DialStatus) -> RgbaImage {
        let dial = dial.filter(|d| !d.is_blank());
        let face = dial.map(|d| d.face());
        let bg = self.background(face.as_ref(), style);
        let mut img = solid_rect(PANEL_W, PANEL_H, if face.is_none() { bg.mix(Rgb(0, 0, 0), 0.55) } else { bg });
        let Some(face) = face else { return img };

        let want_label = face.show_label && !face.label.is_empty();
        // A pressed dial (or a tap, for a moment) turns the title into the accent colour,
        // as the only feedback: the layout stays the same as the Elgato app's.
        let fg = if st.pressed { self.theme.accent } else { self.label_color(Some(&face), style, bg) };
        let glyph_color =
            face.label_color.and_then(Rgb::parse).unwrap_or(if style.follow_theme { self.theme.accent } else { fg });

        // A volume dial shows its level in the $B1 layout.
        if let Some(device) = dial.and_then(|d| d.volume_device()) {
            let muted = st.level.as_ref().is_some_and(|l| l.muted);
            let icon = self.volume_icon(&face, device, muted, glyph_color);
            self.draw_b1(&mut img, &face, bg, fg, want_label, icon, st.level);
            return img;
        }

        let icon = match (face.icon, face.glyph.filter(|g| !g.trim().is_empty())) {
            (Some(p), _) => self.icon(&paths::resolve(p), STRIP_ICON_PX),
            (None, Some(g)) => self.glyph(g, (STRIP_ICON_PX as f32 * 0.86) as u32, glyph_color),
            _ => None,
        };
        if let Some(icon) = icon {
            let (x0, y0, w, h) = X1_ICON;
            let x = x0 + (w - icon.width()) / 2;
            let y = y0 + (h - icon.height()) / 2;
            imageops::overlay(&mut img, &icon, x as i64, y as i64);
        }
        if want_label {
            self.draw_text_in(&mut img, X1_TITLE, STRIP_TITLE_PX, Align::Left, face.label, fg, true);
        }
        img
    }

    /// The icon for a volume dial's `$B1` panel: the user's icon if set, else the mute
    /// glyph when muted, else their glyph, else a default speaker (output) or mic (input).
    fn volume_icon(&mut self, face: &Face, device: AudioDevice, muted: bool, color: Rgb) -> Option<RgbaImage> {
        if let Some(p) = face.icon {
            return self.icon(&paths::resolve(p), STRIP_ICON_PX);
        }
        let glyph = if muted {
            MUTE_GLYPH
        } else if let Some(g) = face.glyph.filter(|g| !g.trim().is_empty()) {
            g
        } else if device.is_output() {
            OUTPUT_GLYPH
        } else {
            INPUT_GLYPH
        };
        self.glyph(glyph, (STRIP_ICON_PX as f32 * 0.86) as u32, color)
    }

    /// Draw the `$B1` "Indicator" layout: title (right), icon (left), value and groove bar.
    #[allow(clippy::too_many_arguments)]
    fn draw_b1(
        &self,
        img: &mut RgbaImage,
        face: &Face,
        bg: Rgb,
        fg: Rgb,
        want_label: bool,
        icon: Option<RgbaImage>,
        level: Option<Level>,
    ) {
        if want_label {
            self.draw_text_in(img, X1_TITLE, STRIP_TITLE_PX, Align::Left, face.label, fg, true);
        }
        if let Some(icon) = icon {
            let (x0, y0, w, h) = B1_ICON;
            let x = x0 + (w - icon.width()) / 2;
            let y = y0 + (h - icon.height()) / 2;
            imageops::overlay(img, &icon, x as i64, y as i64);
        }
        // Value text, right-aligned; a dash until the first level is known, so the panel
        // doesn't jump between layouts. Mute reads "Muted" and leaves the bar empty.
        let (text, fill) = match &level {
            Some(l) => (l.text.as_str(), if l.muted { 0.0 } else { l.value }),
            None => ("–", 0.0),
        };
        self.draw_text_in(img, B1_VALUE, B1_VALUE_PX, Align::Right, text, fg, true);
        let (bx, by, bw, bh) = B1_BAR;
        let track = bg.mix(fg, 0.22);
        fill_pill(img, bx as i64, by as i64, bw as i64, bh as i64, track);
        if fill > 0.0 {
            let fw = ((bw as f32 * fill).round() as i64).max(bh as i64);
            fill_pill(img, bx as i64, by as i64, fw, bh as i64, fg);
        }
    }

    /// Pressed state: face shrinks inside an accent ring for tactile feedback.
    fn pressed(&self, face: RgbaImage, bg: Rgb, size: u32) -> RgbaImage {
        let mut out = solid(size, bg.mix(Rgb(0, 0, 0), 0.4));
        let inner = (size as f32 * 0.84) as u32;
        let small = imageops::resize(&face, inner, inner, FilterType::Triangle);
        let off = ((size - inner) / 2) as i64;
        imageops::overlay(&mut out, &small, off, off);
        let ring = (size / 24).max(2);
        let a = self.theme.accent;
        let px = Rgba([a.0, a.1, a.2, 255]);
        for y in 0..size {
            for x in 0..size {
                if x < ring || y < ring || x >= size - ring || y >= size - ring {
                    out.put_pixel(x, y, px);
                }
            }
        }
        out
    }

    fn icon(&mut self, path: &Path, box_px: u32) -> Option<RgbaImage> {
        let k = (path.to_path_buf(), box_px);
        if let Some(hit) = self.icons.get(&k) {
            return hit.clone();
        }
        let loaded = load_icon(path, box_px);
        if loaded.is_none() {
            log::warn!("could not load icon {}", path.display());
        }
        self.icons.insert(k, loaded.clone());
        loaded
    }

    /// Rasterise a single (Nerd Font) symbol, scaled to fit a `box_px` square.
    fn glyph(&self, text: &str, box_px: u32, color: Rgb) -> Option<RgbaImage> {
        let font = self.font.as_ref()?;
        let id = font.glyph_id(glyph_char(text)?);
        if id.0 == 0 {
            return None;
        }
        let probe = font.outline_glyph(id.with_scale(PxScale::from(100.0)))?.px_bounds();
        let k = box_px as f32 / probe.width().max(probe.height()).max(1.0);
        let outline = font.outline_glyph(id.with_scale_and_position(PxScale::from(100.0 * k), point(0.0, 0.0)))?;
        let b = outline.px_bounds();
        let mut img = RgbaImage::new((b.width().ceil() as u32).max(1), (b.height().ceil() as u32).max(1));
        outline.draw(|x, y, cov| {
            if x < img.width() && y < img.height() {
                img.put_pixel(x, y, Rgba([color.0, color.1, color.2, (cov.clamp(0.0, 1.0) * 255.0) as u8]));
            }
        });
        Some(img)
    }

    // --- text -------------------------------------------------------------------------

    fn text_width(font: &FontVec, px: f32, text: &str) -> f32 {
        let sf = font.as_scaled(PxScale::from(px));
        let mut w = 0.0;
        let mut prev = None;
        for c in text.chars() {
            let id = sf.glyph_id(c);
            if let Some(p) = prev {
                w += sf.kern(p, id);
            }
            w += sf.h_advance(id);
            prev = Some(id);
        }
        w
    }

    /// Draw one centred line with its baseline-centre at `cy`, shrinking/ellipsizing to fit.
    fn draw_line(&self, img: &mut RgbaImage, text: &str, px: f32, cy: f32, color: Rgb, shadow: bool) {
        let Some(font) = &self.font else { return };
        let max_w = img.width() as f32 * 0.92;
        let mut px = px;
        let min_px = img.width() as f32 * 0.11;
        while px > min_px && Self::text_width(font, px, text) > max_w {
            px -= 0.5;
        }
        let text = Self::ellipsize(font, px, text, max_w);
        let w = Self::text_width(font, px, &text);
        let sf = font.as_scaled(PxScale::from(px));
        let baseline = cy + (sf.ascent() + sf.descent()) / 2.0;
        let x = (img.width() as f32 - w) / 2.0;
        if shadow {
            draw_text(img, font, px, x + 1.0, baseline + 1.0, &text, Rgb(0, 0, 0), 0.6);
        }
        draw_text(img, font, px, x, baseline, &text, color, 1.0);
    }

    /// Draw one line inside a fixed `[x, y, w, h]` rectangle, aligned and ellipsized to
    /// the width (the text never shrinks; Elgato clips, we end with `…`).
    #[allow(clippy::too_many_arguments)]
    fn draw_text_in(
        &self,
        img: &mut RgbaImage,
        rect: (u32, u32, u32, u32),
        px: f32,
        align: Align,
        text: &str,
        color: Rgb,
        shadow: bool,
    ) {
        let Some(font) = &self.font else { return };
        let (x0, y0, w, h) = rect;
        let text = Self::ellipsize(font, px, text, w as f32);
        let sf = font.as_scaled(PxScale::from(px));
        let baseline = y0 as f32 + h as f32 / 2.0 + (sf.ascent() + sf.descent()) / 2.0;
        let tw = Self::text_width(font, px, &text);
        let x = match align {
            Align::Left => x0 as f32,
            Align::Center => x0 as f32 + (w as f32 - tw) / 2.0,
            Align::Right => x0 as f32 + w as f32 - tw,
        };
        if shadow {
            draw_text(img, font, px, x + 1.0, baseline + 1.0, &text, Rgb(0, 0, 0), 0.6);
        }
        draw_text(img, font, px, x, baseline, &text, color, 1.0);
    }

    /// Shorten `text` with a trailing ellipsis so it fits `max_w` at `px`.
    fn ellipsize(font: &FontVec, px: f32, text: &str, max_w: f32) -> String {
        if Self::text_width(font, px, text) <= max_w {
            return text.to_string();
        }
        let chars: Vec<char> = text.chars().collect();
        for n in (1..chars.len()).rev() {
            let cand = format!("{}…", chars[..n].iter().collect::<String>().trim_end());
            if Self::text_width(font, px, &cand) <= max_w || n == 1 {
                return cand;
            }
        }
        text.to_string()
    }

    /// Label without an icon: large, centred, wrapped onto up to three lines.
    fn draw_block(&self, img: &mut RgbaImage, text: &str, color: Rgb) {
        let Some(font) = &self.font else { return };
        let s = img.width() as f32;
        let max_w = s * 0.90;
        let mut px = s * 0.26;
        let lines = loop {
            let lines = wrap(font, px, text, max_w);
            let fits = lines.iter().all(|l| Self::text_width(font, px, l) <= max_w)
                && lines.len() as f32 * px * 1.1 <= s * 0.9;
            if fits || px <= s * 0.12 {
                break lines;
            }
            px -= 0.5;
        };
        let lh = px * 1.1;
        let top = (s - lh * lines.len() as f32) / 2.0;
        for (i, line) in lines.iter().enumerate() {
            self.draw_line(img, line, px, top + lh * (i as f32 + 0.5), color, false);
        }
    }
}

/// A pasted symbol, or a codepoint written as `f075f`, `U+F075F`, `0xf075f` or `\\u{f075f}`.
pub fn glyph_char(text: &str) -> Option<char> {
    let t = text.trim();
    let hex = t
        .trim_start_matches("\\u{")
        .trim_end_matches('}')
        .trim_start_matches("U+")
        .trim_start_matches("u+")
        .trim_start_matches("0x");
    if hex.len() >= 4 && hex.len() <= 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return u32::from_str_radix(hex, 16).ok().and_then(char::from_u32);
    }
    t.chars().next()
}

fn wrap(font: &FontVec, px: f32, text: &str, max_w: f32) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for word in text.split_whitespace() {
        match lines.last_mut() {
            Some(l) if Renderer::text_width(font, px, &format!("{l} {word}")) <= max_w => {
                l.push(' ');
                l.push_str(word);
            }
            _ => lines.push(word.to_string()),
        }
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

#[allow(clippy::too_many_arguments)]
fn draw_text(
    img: &mut RgbaImage,
    font: &FontVec,
    px: f32,
    x: f32,
    baseline: f32,
    text: &str,
    color: Rgb,
    opacity: f32,
) {
    let scale = PxScale::from(px);
    let sf = font.as_scaled(scale);
    let mut caret = x;
    let mut prev = None;
    let (w, h) = img.dimensions();
    for c in text.chars() {
        let id = sf.glyph_id(c);
        if let Some(p) = prev {
            caret += sf.kern(p, id);
        }
        let glyph = id.with_scale_and_position(scale, point(caret, baseline));
        caret += sf.h_advance(id);
        prev = Some(id);
        let Some(outline) = font.outline_glyph(glyph) else { continue };
        let b = outline.px_bounds();
        outline.draw(|gx, gy, cov| {
            let x = b.min.x as i32 + gx as i32;
            let y = b.min.y as i32 + gy as i32;
            if x < 0 || y < 0 || x as u32 >= w || y as u32 >= h {
                return;
            }
            let a = (cov * opacity).clamp(0.0, 1.0);
            let p = img.get_pixel_mut(x as u32, y as u32);
            let blend = |d: u8, s: u8| (d as f32 * (1.0 - a) + s as f32 * a).round() as u8;
            p.0 = [blend(p.0[0], color.0), blend(p.0[1], color.1), blend(p.0[2], color.2), 255];
        });
    }
}

fn solid(size: u32, c: Rgb) -> RgbaImage {
    RgbaImage::from_pixel(size, size, Rgba([c.0, c.1, c.2, 255]))
}

fn solid_rect(w: u32, h: u32, c: Rgb) -> RgbaImage {
    RgbaImage::from_pixel(w, h, Rgba([c.0, c.1, c.2, 255]))
}

/// Fill a horizontal pill (a rect with fully rounded ends) with `color`. Used for the
/// `$B1` groove bar and its fill; `w` must be at least `h`.
fn fill_pill(img: &mut RgbaImage, x: i64, y: i64, w: i64, h: i64, color: Rgb) {
    let (img_w, img_h) = img.dimensions();
    let r = h as f32 / 2.0;
    let cy = y as f32 + r;
    let y0 = y.max(0).min(img_h as i64);
    let y1 = (y + h).max(0).min(img_h as i64);
    let x0 = x.max(0).min(img_w as i64);
    let x1 = (x + w).max(0).min(img_w as i64);
    for py in y0..y1 {
        for px in x0..x1 {
            let fx = px as f32 + 0.5;
            let fy = py as f32 + 0.5;
            let inside = if fx < x as f32 + r {
                let dx = fx - (x as f32 + r);
                dx * dx + (fy - cy) * (fy - cy) <= r * r
            } else if fx >= x as f32 + w as f32 - r {
                let dx = fx - (x as f32 + w as f32 - r);
                dx * dx + (fy - cy) * (fy - cy) <= r * r
            } else {
                true
            };
            if inside {
                img.put_pixel(px as u32, py as u32, Rgba([color.0, color.1, color.2, 255]));
            }
        }
    }
}

/// Load a raster or SVG icon and fit it inside a `box_px` square, preserving aspect ratio.
pub fn load_icon(path: &Path, box_px: u32) -> Option<RgbaImage> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    if is_svg {
        return load_svg(path, box_px);
    }
    let img = image::open(path).ok()?.to_rgba8();
    let (w, h) = img.dimensions();
    let scale = box_px as f32 / w.max(h) as f32;
    let (nw, nh) = (((w as f32 * scale).round() as u32).max(1), ((h as f32 * scale).round() as u32).max(1));
    Some(imageops::resize(&img, nw, nh, FilterType::Lanczos3))
}

fn load_svg(path: &Path, box_px: u32) -> Option<RgbaImage> {
    use resvg::{tiny_skia, usvg};
    let data = fs::read(path).ok()?;
    let tree = usvg::Tree::from_data(&data, &usvg::Options::default()).ok()?;
    let size = tree.size();
    let scale = box_px as f32 / size.width().max(size.height());
    let (w, h) = (((size.width() * scale).ceil() as u32).max(1), ((size.height() * scale).ceil() as u32).max(1));
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    let mut out = RgbaImage::new(w, h);
    for (dst, src) in out.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Some(out)
}

/// Find a bold face for a fontconfig pattern (defaults to Omarchy's monospace font).
fn load_font(pattern: &str) -> Option<FontVec> {
    let candidates = [format!("{pattern}:weight=bold"), pattern.to_string(), "sans:bold".into()];
    for pat in &candidates {
        let out = Command::new("fc-match").args(["-f", "%{file}", pat]).output().ok()?;
        let file = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if file.is_empty() {
            continue;
        }
        if let Ok(data) = fs::read(&file)
            && let Ok(font) = FontVec::try_from_vec(data)
        {
            log::debug!("label font: {file}");
            return Some(font);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{B1_BAR, DialStatus, Level, Renderer, X1_ICON, X1_TITLE, glyph_char, tap_to_dial};
    use crate::config::{Action, AudioDevice, Config, DialConfig, KeyConfig, Style};
    use crate::theme::Rgb;

    #[test]
    fn glyph_forms() {
        assert_eq!(glyph_char("\u{f075f}"), Some('\u{f075f}'));
        assert_eq!(glyph_char("f075f"), Some('\u{f075f}'));
        assert_eq!(glyph_char("U+F075F"), Some('\u{f075f}'));
        assert_eq!(glyph_char("\\u{f075f}"), Some('\u{f075f}'));
        assert_eq!(glyph_char("A"), Some('A'));
        assert_eq!(glyph_char(""), None);
    }

    /// Deterministic style: no theme, a fixed background, black-on-`#334455`.
    fn test_style() -> Style {
        Style { follow_theme: false, background: Some("#334455".into()), ..Default::default() }
    }

    /// The renderer needs a font to draw ink; skip the pixel assertions without one.
    fn renderer_with_font() -> Option<Renderer> {
        let r = Renderer::new(&test_style());
        r.font.is_some().then_some(r)
    }

    fn in_rect(x: u32, y: u32, rect: (u32, u32, u32, u32)) -> bool {
        let (x0, y0, w, h) = rect;
        x >= x0 && x < x0 + w && y >= y0 && y < y0 + h
    }

    #[test]
    fn key_glyph_stays_in_its_box() {
        let Some(mut r) = renderer_with_font() else { return };
        let style = test_style();
        let key = KeyConfig { label: String::new(), glyph: Some("A".into()), ..KeyConfig::new(0) };
        let img = r.render(Some(&key), &style, 72, false);
        let box_px = (72.0 * 0.80) as u32;
        let off = (72 - box_px) / 2;
        for y in 0..72 {
            for x in 0..72 {
                if img.get_pixel(x, y)[0] > 120 {
                    assert!(x >= off && x < off + box_px && y >= off && y < off + box_px, "ink at ({x},{y})");
                }
            }
        }
    }

    #[test]
    fn strip_is_800x100_for_a_plus() {
        let mut r = Renderer::new(&test_style());
        let cfg = Config::default();
        let status = vec![DialStatus::default(); 4];
        let img = r.render_strip(&cfg, &status, (800, 100));
        assert_eq!(img.dimensions(), (800, 100));
    }

    #[test]
    fn strip_scaled() {
        let mut r = Renderer::new(&test_style());
        let cfg = Config::default();
        let status = vec![DialStatus::default(); 4];
        assert_eq!(r.render_strip(&cfg, &status, (400, 50)).dimensions(), (400, 50));
    }

    #[test]
    fn strip_panel_layout() {
        let Some(mut r) = renderer_with_font() else { return };
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(DialConfig { label: "Workspace".into(), glyph: Some("A".into()), ..DialConfig::new(0) });
        let status = vec![DialStatus::default(); 4];
        let img = r.render_strip(&cfg, &status, (800, 100));
        let (title, icon) = (X1_TITLE, X1_ICON);
        let mut title_ink = 0;
        for y in 0..100 {
            for x in 0..800 {
                if img.get_pixel(x, y)[0] <= 120 {
                    continue;
                }
                let lx = x % 200;
                if x < 200 {
                    // Panel 0's ink sits in the title or icon rect, never elsewhere.
                    assert!(in_rect(lx, y, title) || in_rect(lx, y, icon), "ink at ({x},{y})");
                    if in_rect(lx, y, title) {
                        title_ink += 1;
                    }
                } else {
                    // The other three panels are empty slots: no ink at all.
                    panic!("ink in empty panel at ({x},{y})");
                }
            }
        }
        // The title is left-aligned: its ink starts near x = 16.
        assert!(title_ink > 0, "no title ink");
        let leftmost =
            (0..200u32).find(|x| (0..100).any(|y| img.get_pixel(*x, y)[0] > 120 && in_rect(*x, y, title))).unwrap();
        assert!((14..20).contains(&leftmost), "title starts at x = {leftmost}");
    }

    #[test]
    fn strip_empty_slot_matches_empty_key() {
        let mut r = Renderer::new(&test_style());
        let style = test_style();
        let panel = r.render_strip_panel(None, &style, DialStatus::default());
        let key = r.render(None, &style, 100, false);
        assert_eq!(*panel.get_pixel(100, 50), *key.get_pixel(50, 50));
    }

    #[test]
    fn strip_panel_isolation() {
        let mut r = Renderer::new(&test_style());
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(DialConfig { label: "One".into(), glyph: Some("A".into()), ..DialConfig::new(0) });
        cfg.dials.push(DialConfig { label: "Two".into(), glyph: Some("B".into()), ..DialConfig::new(2) });
        let status = vec![DialStatus::default(); 4];
        let before = r.render_strip(&cfg, &status, (800, 100));
        cfg.dials[1].label = "Changed".into();
        let after = r.render_strip(&cfg, &status, (800, 100));
        for (x0, changed) in [(0u32, false), (200, false), (400, true), (600, false)] {
            let same = (0..200).all(|dx| (0..100).all(|y| before.get_pixel(x0 + dx, y) == after.get_pixel(x0 + dx, y)));
            assert_eq!(same, !changed, "panel at x = {x0} should {changed}have changed");
        }
    }

    #[test]
    fn tap_edges() {
        assert_eq!(tap_to_dial(0, 800, 4), Some(0));
        assert_eq!(tap_to_dial(199, 800, 4), Some(0));
        assert_eq!(tap_to_dial(200, 800, 4), Some(1));
        assert_eq!(tap_to_dial(599, 800, 4), Some(2));
        assert_eq!(tap_to_dial(600, 800, 4), Some(3));
        assert_eq!(tap_to_dial(799, 800, 4), Some(3));
        assert_eq!(tap_to_dial(800, 800, 4), None);
        assert_eq!(tap_to_dial(10, 0, 4), None);
        assert_eq!(tap_to_dial(10, 800, 0), None);
    }

    /// A dial whose turn action is `Volume`.
    fn volume_dial() -> DialConfig {
        DialConfig {
            label: "Volume".into(),
            turn_right: Action::Volume { device: AudioDevice::Output, change: 5, mute: false },
            ..DialConfig::new(0)
        }
    }

    fn level(v: f32, muted: bool) -> DialStatus {
        DialStatus {
            pressed: false,
            level: Some(Level { value: v, text: if muted { "Muted".into() } else { "50%".into() }, muted }),
        }
    }

    #[test]
    fn strip_volume_bar_fills_to_level() {
        let Some(mut r) = renderer_with_font() else { return };
        let style = test_style();
        let dial = volume_dial();
        let (bx, by, _bw, bh) = B1_BAR;
        let mid_y = by + bh / 2;
        let x = bx + 90; // the right part of the bar
        let full = r.render_strip_panel(Some(&dial), &style, level(1.0, false));
        let empty = r.render_strip_panel(Some(&dial), &style, level(0.0, false));
        assert_ne!(full.get_pixel(x, mid_y), empty.get_pixel(x, mid_y), "the bar fill should change with the level");
    }

    #[test]
    fn strip_volume_muted_leaves_bar_empty() {
        let Some(mut r) = renderer_with_font() else { return };
        let style = test_style();
        let dial = volume_dial();
        let (bx, by, _bw, bh) = B1_BAR;
        let mid_y = by + bh / 2;
        let muted = r.render_strip_panel(Some(&dial), &style, level(0.8, true));
        let empty = r.render_strip_panel(Some(&dial), &style, level(0.0, false));
        // Muted: the bar is the empty track, the same as level 0.
        for x in bx..bx + 108 {
            assert_eq!(*muted.get_pixel(x, mid_y), *empty.get_pixel(x, mid_y), "muted bar should be empty at x = {x}");
        }
    }

    #[test]
    fn strip_volume_without_level_stays_b1() {
        // Before the first poll (level None) the panel is still $B1, not $X1: it draws its
        // groove bar and does not draw the centred $X1 icon.
        let Some(mut r) = renderer_with_font() else { return };
        let style = test_style();
        let dial = volume_dial();
        let panel = r.render_strip_panel(Some(&dial), &style, DialStatus::default());
        // No centred $X1 icon (x 76..124) in the icon's vertical band.
        let x1_icon_ink = (76..124).any(|x| (40..72).any(|y| panel.get_pixel(x, y)[0] > 120));
        assert!(!x1_icon_ink, "a $B1 panel should not draw the centred $X1 icon");
        // The $B1 groove bar is visible on the background.
        assert_ne!(*panel.get_pixel(5, 5), *panel.get_pixel(100, 80), "the $B1 groove bar should be visible");
    }

    #[test]
    fn label_color_contrast_kept() {
        let r = Renderer::new(&Style { follow_theme: false, ..Default::default() });
        // White label on a white background flips dark; on black it keeps its contrast.
        assert_eq!(r.label_color(None, &test_style(), Rgb(255, 255, 255)), Rgb(16, 16, 16));
        assert_eq!(r.label_color(None, &test_style(), Rgb(0, 0, 0)), Rgb(255, 255, 255));
    }
}
