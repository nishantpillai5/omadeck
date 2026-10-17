//! Renders a key face (background + icon + label) to an RGBA bitmap.
//!
//! The same renderer drives both the physical deck and the previews in the configurator,
//! so what you see in the app is exactly what lands on the hardware.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use ab_glyph::{Font, FontVec, PxScale, ScaleFont, point};
use image::Rgba;
pub use image::RgbaImage;
use image::imageops::{self, FilterType};

use crate::config::{AudioDevice, Config, KeyConfig, LevelSource, Style};
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
    /// A background image (dials only; keys pass `None`).
    pub background_image: Option<&'a str>,
    pub label_color: Option<&'a str>,
}

/// Live state of one dial for its strip panel.
#[derive(Debug, Clone, Default)]
pub struct DialStatus {
    /// The dial is pressed (or a tap is still flashing).
    pub pressed: bool,
    /// What the panel shows, as the daemon tells the configurator.
    pub panel: PanelStatus,
}

/// What a dial's panel shows, apart from being pressed: the part the daemon sends the
/// configurator in `Event::DialStatus`. PR 9 adds `live`, PR 10 `app`, PR 14 `media`,
/// each `#[serde(default, skip_serializing_if = ...)]`.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelStatus {
    /// A value for the `$B1` layout (volume, brightness); `None` before the first read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// A live dial's value (PR 9): `$B1` when it has a bar, else `$A1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveShow>,
}

/// What a live key or strip panel shows (PR 9).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveShow {
    /// "14:05", "42%", "3"; may hold `\n` (a clock's `%n`).
    pub text: String,
    /// 0.0..=1.0 for the strip's `$B1` bar; `None` for text sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar: Option<f32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub charging: bool,
}

/// A value for the `$B1` layout: a bar position plus the text next to it (e.g. `40%`).
/// Sent to the configurator in `Event::DialStatus`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// 0.0..=1.0, for the bar.
    pub value: f32,
    pub text: String,
    /// Mute: the text reads `Muted` and the bar is left empty.
    pub muted: bool,
}

/// What a key looks like right now, beside its config. Mirrors `DialStatus`; built only
/// by `probe::key_status`. Serializable for `Event::KeyState`; every field is
/// `serde(default)` and skipped when default, so fields can be added.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeyStatus {
    /// Drawn with the press ring.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pressed: bool,
    /// Off, on or busy; drawn by `KeyConfig::face_for`.
    #[serde(default, skip_serializing_if = "crate::probe::Toggle::is_off")]
    pub state: crate::probe::Toggle,
    /// A live key's value (PR 9); `None` before the first read (drawn as `–`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<LiveShow>,
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
/// Default panel glyphs for a volume or brightness dial with no icon or glyph of its own.
const OUTPUT_GLYPH: &str = "f057e"; // 󰕾 speaker
const INPUT_GLYPH: &str = "f036c"; // 󰎬 microphone
const MUTE_GLYPH: &str = "f075f"; // 󰝟 muted
const BRIGHTNESS_GLYPH: &str = "f00de"; // 󰃞 brightness

/// The glyph a volume dial's `$B1` panel shows when it has no icon: the mute glyph when
/// muted, else the dial's own glyph, else a speaker (output) or a microphone (input).
fn volume_glyph<'a>(face: &Face<'a>, device: AudioDevice, muted: bool) -> &'a str {
    if muted {
        MUTE_GLYPH
    } else if let Some(g) = face.glyph.filter(|g| !g.trim().is_empty()) {
        g
    } else if device.is_output() {
        OUTPUT_GLYPH
    } else {
        INPUT_GLYPH
    }
}

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
    /// Fitted background images (strip and dial), each with the file's stamp at load.
    backdrops: HashMap<(PathBuf, u32, u32), Backdropped>,
    #[cfg(test)]
    pub decode_count: std::cell::Cell<u32>,
    /// Every `load_cover` call, failed or not.
    #[cfg(test)]
    pub load_attempts: std::cell::Cell<u32>,
}

/// The bundled test font: a JetBrains Mono Bold subset (OFL, printable ASCII plus `–`
/// and `…`), for [`Renderer::with_font_bytes`]. Draw `"A"` where a test needs a glyph.
#[cfg(any(test, feature = "test-support"))]
pub const TEST_FONT: &[u8] = include_bytes!("../tests/fixtures/test-font.ttf");

/// A file's identity for the background cache: its mtime and length, `None` when it
/// can't be stat'ed. Taken when an image is loaded and compared only on `clear_cache`.
type Stamp = Option<(std::time::SystemTime, u64)>;

/// A cached background image: the file's stamp when it was loaded, and the fitted image
/// (`None` when the load failed), shared with every paint that uses it.
type Backdropped = (Stamp, Option<Arc<RgbaImage>>);

fn stamp(path: &Path) -> Stamp {
    let m = fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}

impl Renderer {
    pub fn new(style: &Style) -> Self {
        let mut r = Self {
            font: None,
            font_pattern: None,
            theme: Theme::default(),
            icons: HashMap::new(),
            backdrops: HashMap::new(),
            #[cfg(test)]
            decode_count: std::cell::Cell::new(0),
            #[cfg(test)]
            load_attempts: std::cell::Cell::new(0),
        };
        r.update_style(style);
        r.theme = Theme::load_omarchy();
        r
    }

    /// A renderer that draws with this font file instead of fontconfig's choice, so pixel
    /// tests give the same result on every machine. The theme is the built-in default.
    /// For tests only: other crates get it with the `test-support` feature (pass
    /// [`TEST_FONT`]).
    #[cfg(any(test, feature = "test-support"))]
    pub fn with_font_bytes(style: &Style, data: &[u8]) -> Self {
        Self {
            font: Some(FontVec::try_from_vec(data.to_vec()).expect("a valid test font")),
            font_pattern: style.font.clone(),
            theme: Theme::default(),
            icons: HashMap::new(),
            backdrops: HashMap::new(),
            #[cfg(test)]
            decode_count: std::cell::Cell::new(0),
            #[cfg(test)]
            load_attempts: std::cell::Cell::new(0),
        }
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
    /// Backdrop images are expensive to decode, so entries whose file is unchanged
    /// (same mtime and length) are kept; the rest are dropped and re-decoded on next use.
    pub fn clear_cache(&mut self) {
        self.icons.clear();
        // The only place backdrop files are stat'ed: a paint never touches the disk.
        self.backdrops.retain(|(path, _, _), (st, _)| stamp(path) == *st);
    }

    /// A fitted background image (strip or dial), decoded once per file and shared. A
    /// hit costs no file access; `clear_cache` (on reload) drops entries whose file
    /// changed. A missing or unreadable file is cached as `None` too, so it warns once.
    fn backdrop_image(&mut self, path: &Path, w: u32, h: u32) -> Option<Arc<RgbaImage>> {
        let k = (path.to_path_buf(), w, h);
        if let Some((_, img)) = self.backdrops.get(&k) {
            return img.clone();
        }
        let st = stamp(path);
        #[cfg(test)]
        self.load_attempts.set(self.load_attempts.get() + 1);
        let loaded = st.is_some().then(|| load_cover(path, w, h)).flatten().map(Arc::new);
        if loaded.is_none() {
            log::warn!("could not load background image {}", path.display());
        }
        #[cfg(test)]
        if loaded.is_some() {
            self.decode_count.set(self.decode_count.get() + 1);
        }
        self.backdrops.insert(k, (st, loaded.clone()));
        loaded
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

    /// Render one key face at `size`×`size`, in the state `st` says it is in (off, on,
    /// busy — and pressed over any of them).
    pub fn render(&mut self, key: Option<&KeyConfig>, style: &Style, size: u32, st: &KeyStatus) -> RgbaImage {
        use crate::probe::{Toggle, progress};
        let key = key.filter(|k| !k.is_blank());
        let mut face = key.as_ref().map(|k| k.face_for(&st.state));
        let bg = self.background(face.as_ref(), style);
        // An on face without its own background is tinted towards the accent, so a bare
        // [key.on] is still visibly different.
        let bg = if matches!(st.state, Toggle::On)
            && key.is_some_and(|k| k.on.as_ref().is_some_and(|on| on.background.is_none()))
        {
            let a = if style.follow_theme { self.theme.accent } else { self.label_color(face.as_ref(), style, bg) };
            bg.mix(a, ON_TINT)
        } else {
            bg
        };
        // A busy key with no label colour of its own takes the accent, so the progress
        // reads as a state, not as the off label. The hex is held in a local for the
        // whole of the function: the (owned) face borrows it.
        let accent;
        if matches!(st.state, Toggle::Busy(_))
            && face.as_ref().is_some_and(|f| f.label_color.is_none())
            && style.label_color.as_deref().is_none_or(|s| s.trim().is_empty())
            && style.follow_theme
        {
            accent = self.theme.accent.hex();
            if let Some(f) = face.as_mut() {
                f.label_color = Some(&accent);
            }
        }
        let out = match (key, face) {
            (None, None) => solid(size, bg.mix(Rgb(0, 0, 0), 0.55)),
            (Some(k), Some(f)) => {
                // A busy face with a percentage gets a progress bar in a zone at the
                // bottom; the face moves up to make room.
                let bar = match &st.state {
                    Toggle::Busy(t) if k.busy.as_ref().is_some_and(|b| b.bar) => progress(t),
                    _ => None,
                };
                match bar {
                    Some(p) => {
                        let s = size as f32;
                        let inner = (s * (1.0 - BAR_ZONE)) as u32;
                        let mut img = solid(size, bg);
                        imageops::overlay(
                            &mut img,
                            &self.render_face(&f, style, inner, bg),
                            ((size - inner) / 2) as i64,
                            0,
                        );
                        let fg = self.label_color(Some(&f), style, bg);
                        let bh = (s * 0.07).max(4.0) as u32;
                        let bx = (s * 0.10) as u32;
                        let by = size - bh - (s * 0.08) as u32;
                        draw_bar(&mut img, (bx, by, (s * 0.80) as u32, bh), p as f32 / 100.0, fg, bg.mix(fg, 0.22));
                        img
                    }
                    None => self.render_face(&f, style, size, bg),
                }
            }
            _ => unreachable!("a face is built exactly when a key is"),
        };
        if st.pressed { self.pressed(out, bg, size) } else { out }
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
                self.render_strip_panel(cfg, i as u8, panels as u32, status.get(i).cloned().unwrap_or_default());
            imageops::overlay(&mut img, &panel, (i as u32 * PANEL_W) as i64, 0);
        }
        if size == native { img } else { imageops::resize(&img, size.0, size.1, FilterType::Triangle) }
    }

    /// Layers 1–4 for dial `i` of `panels`: the base colour, the strip image slice,
    /// the dial colour and the dial image.
    pub fn panel_backdrop(&mut self, cfg: &Config, i: u8, panels: u32) -> Backdrop {
        let dial = cfg.dial(i).filter(|d| !d.is_blank());
        let mut img = solid_rect(PANEL_W, PANEL_H, self.background(None, &cfg.style));
        // Layer 2: the strip image, fitted once per file and sliced per panel.
        let mut has_image = false;
        if let Some(src) = cfg.strip.background_image.as_deref()
            && let Some(full) = self.backdrop_image(&paths::resolve(src), PANEL_W * panels, PANEL_H)
            && (i as u32) < panels
        {
            let slice = imageops::crop_imm(&*full, i as u32 * PANEL_W, 0, PANEL_W, PANEL_H).to_image();
            imageops::overlay(&mut img, &slice, 0, 0);
            has_image = true;
        }
        if let Some(d) = &dial {
            // Layer 3: the dial's own colour, over the strip image.
            if let Some(c) = d.background.as_deref().and_then(Rgb::parse) {
                imageops::overlay(&mut img, &solid_rect(PANEL_W, PANEL_H, c), 0, 0);
            }
            // Layer 4: the dial image, alpha-composited over everything under it.
            if let Some(src) = d.background_image.as_deref()
                && let Some(image) = self.backdrop_image(&paths::resolve(src), PANEL_W, PANEL_H)
            {
                imageops::overlay(&mut img, &*image, 0, 0);
                has_image = true;
            }
        }
        let tone = mean_rgb(&img);
        Backdrop { tone, img, has_image }
    }

    /// Render one 200×100 dial panel at native size. A volume dial uses Elgato's `$B1`
    /// "Indicator" layout (icon left, value and groove bar right); everything else uses
    /// `$X1` "Icon" (title left, icon centred). `i` is the panel's index of `panels`,
    /// needed to slice the strip image.
    pub fn render_strip_panel(&mut self, cfg: &Config, i: u8, panels: u32, st: DialStatus) -> RgbaImage {
        let style = &cfg.style;
        let dial = cfg.dial(i).filter(|d| !d.is_blank());
        let face = dial.map(|d| d.face());
        let backdrop = self.panel_backdrop(cfg, i, panels);
        let tone = backdrop.tone;
        // An empty slot with no image is the dimmed base colour, as before; with an
        // image the empty panels show their slice undimmed, so the picture stays
        // continuous, as Elgato's strip background does.
        let mut img = if face.is_none() && !backdrop.has_image {
            solid_rect(PANEL_W, PANEL_H, self.background(None, style).mix(Rgb(0, 0, 0), 0.55))
        } else {
            backdrop.img
        };
        let Some(face) = face else { return img };

        let want_label = face.show_label && !face.label.is_empty();
        // A pressed dial (or a tap, for a moment) turns the title into the accent colour,
        // as the only feedback: the layout stays the same as the Elgato app's. The value
        // and the bar keep their colours (PR 3's decision).
        let fg = self.label_color(Some(&face), style, tone);
        let title = if st.pressed { self.theme.accent } else { fg };
        let glyph_color =
            face.label_color.and_then(Rgb::parse).unwrap_or(if style.follow_theme { self.theme.accent } else { fg });

        // A volume or brightness dial shows its value in the $B1 layout.
        if let Some(src) = dial.and_then(|d| d.level_source()) {
            let icon = match src {
                LevelSource::Volume(device) => {
                    let muted = st.panel.level.as_ref().is_some_and(|l| l.muted);
                    self.volume_icon(&face, device, muted, glyph_color)
                }
                LevelSource::Brightness => self.brightness_icon(&face, glyph_color),
            };
            let bar = B1Colors { title, value: fg, fill: fg, track: tone.mix(fg, 0.22) };
            self.draw_b1(&mut img, &face, bar, want_label, icon, st.panel.level.as_ref());
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
            self.draw_text_in(&mut img, X1_TITLE, STRIP_TITLE_PX, Align::Left, face.label, title, true);
        }
        img
    }

    /// The icon for a volume dial's `$B1` panel: the user's icon if set, else
    /// [`volume_glyph`].
    fn volume_icon(&mut self, face: &Face, device: AudioDevice, muted: bool, color: Rgb) -> Option<RgbaImage> {
        if let Some(p) = face.icon {
            return self.icon(&paths::resolve(p), STRIP_ICON_PX);
        }
        self.glyph(volume_glyph(face, device, muted), (STRIP_ICON_PX as f32 * 0.86) as u32, color)
    }

    /// The icon for a brightness dial's `$B1` panel: the user's icon if set, else their
    /// glyph, else a default sun.
    fn brightness_icon(&mut self, face: &Face, color: Rgb) -> Option<RgbaImage> {
        if let Some(p) = face.icon {
            return self.icon(&paths::resolve(p), STRIP_ICON_PX);
        }
        let glyph = face.glyph.filter(|g| !g.trim().is_empty()).unwrap_or(BRIGHTNESS_GLYPH);
        self.glyph(glyph, (STRIP_ICON_PX as f32 * 0.86) as u32, color)
    }

    /// Draw the `$B1` "Indicator" layout: title (right), icon (left), value and groove bar.
    fn draw_b1(
        &self,
        img: &mut RgbaImage,
        face: &Face,
        colors: B1Colors,
        want_label: bool,
        icon: Option<RgbaImage>,
        level: Option<&Level>,
    ) {
        if want_label {
            self.draw_text_in(img, X1_TITLE, STRIP_TITLE_PX, Align::Left, face.label, colors.title, true);
        }
        if let Some(icon) = icon {
            let (x0, y0, w, h) = B1_ICON;
            let x = x0 + (w - icon.width()) / 2;
            let y = y0 + (h - icon.height()) / 2;
            imageops::overlay(img, &icon, x as i64, y as i64);
        }
        // Value text, right-aligned; a dash until the first level is known, so the panel
        // doesn't jump between layouts. Mute reads "Muted" and leaves the bar empty.
        let (text, value) = match level {
            Some(l) => (l.text.as_str(), if l.muted { 0.0 } else { l.value }),
            None => ("–", 0.0),
        };
        self.draw_text_in(img, B1_VALUE, B1_VALUE_PX, Align::Right, text, colors.value, true);
        draw_bar(img, B1_BAR, value, colors.fill, colors.track);
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

/// The colours of a `$B1` panel's parts, chosen by the caller (PR 10 tones each part
/// against the image under it).
#[derive(Debug, Clone, Copy)]
struct B1Colors {
    title: Rgb,
    value: Rgb,
    fill: Rgb,
    track: Rgb,
}

/// How far an on face without its own background is mixed towards the accent.
const ON_TINT: f32 = 0.30;
/// The zone at the bottom of a busy key face that the progress bar sits in.
const BAR_ZONE: f32 = 0.22;

/// A groove bar in `rect` (x, y, w, h): the track, then the fill up to `value`, clamped
/// to 0–1 here (a NaN draws no fill). A fill never shrinks below a round end cap.
fn draw_bar(img: &mut RgbaImage, rect: (u32, u32, u32, u32), value: f32, fill: Rgb, track: Rgb) {
    let (bx, by, bw, bh) = rect;
    fill_pill(img, bx as i64, by as i64, bw as i64, bh as i64, track);
    let value = if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) };
    if value > 0.0 {
        let fw = ((bw as f32 * value).round() as i64).max(bh as i64);
        fill_pill(img, bx as i64, by as i64, fw, bh as i64, fill);
    }
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

/// Everything under a strip panel's layout: the base colour, the strip image slice,
/// the dial colour and the dial image (layers 1–4).
pub struct Backdrop {
    pub img: RgbaImage,
    /// The colour layouts treat as "the background" for contrast: the mean of `img`.
    pub tone: Rgb,
    /// Whether a strip or dial image actually landed (a missing file counts as none).
    pub has_image: bool,
}

/// The mean RGB of an image, alpha ignored.
fn mean_rgb(img: &RgbaImage) -> Rgb {
    let (mut r, mut g, mut b) = (0u64, 0u64, 0u64);
    for px in img.pixels() {
        r += u64::from(px[0]);
        g += u64::from(px[1]);
        b += u64::from(px[2]);
    }
    let n = u64::from(img.width() * img.height()).max(1);
    Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
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

/// Cover `w × h`: centre-crop the source to the target's aspect ratio, then resize
/// (`Lanczos3`) to exactly `w × h`. Exact-size input is returned unchanged. Cropping
/// first keeps the work bounded by the source and the target, whatever the aspect.
pub fn fit_cover(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.dimensions() == (w, h) {
        return img.clone();
    }
    let (iw, ih) = img.dimensions();
    if iw == 0 || ih == 0 || w == 0 || h == 0 {
        return RgbaImage::new(w, h);
    }
    let s = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let cw = ((w as f32 / s).round() as u32).clamp(1, iw);
    let ch = ((h as f32 / s).round() as u32).clamp(1, ih);
    let src = imageops::crop_imm(img, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image();
    if src.dimensions() == (w, h) { src } else { imageops::resize(&src, w, h, FilterType::Lanczos3) }
}

/// Load a raster or SVG image fitted to cover `w × h`. An animated GIF loads its first
/// frame, which `image::open` already does.
pub fn load_cover(path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    if is_svg {
        return load_svg_cover(path, w, h);
    }
    image::open(path).ok().map(|img| fit_cover(&img.to_rgba8(), w, h))
}

/// Rasterise an SVG straight at the cover scale into a `w × h` pixmap, centred, so it
/// stays sharp (no raster resize) and a huge intrinsic size allocates nothing extra.
fn load_svg_cover(path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
    use resvg::tiny_skia;
    let tree = svg_tree(path)?;
    let size = tree.size();
    let (sw, sh) = (size.width(), size.height());
    if w == 0 || h == 0 || sw <= 0.0 || sh <= 0.0 {
        return None;
    }
    let s = (w as f32 / sw).max(h as f32 / sh);
    let tx = -(sw * s - w as f32) / 2.0;
    let ty = -(sh * s - h as f32) / 2.0;
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s).post_translate(tx, ty), &mut pixmap.as_mut());
    Some(pixmap_to_rgba(&pixmap))
}

/// Rasterise an SVG scaled so its longer side is `box_px` (fit inside, no crop).
fn load_svg(path: &Path, box_px: u32) -> Option<RgbaImage> {
    let tree = svg_tree(path)?;
    let size = tree.size();
    let scale = box_px as f32 / size.width().max(size.height());
    rasterize_svg(&tree, scale)
}

fn svg_tree(path: &Path) -> Option<resvg::usvg::Tree> {
    let data = fs::read(path).ok()?;
    resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()
}

fn rasterize_svg(tree: &resvg::usvg::Tree, scale: f32) -> Option<RgbaImage> {
    use resvg::tiny_skia;
    let size = tree.size();
    let w = (size.width() * scale).ceil() as u32;
    let h = (size.height() * scale).ceil() as u32;
    let mut pixmap = tiny_skia::Pixmap::new(w.max(1), h.max(1))?;
    resvg::render(tree, tiny_skia::Transform::from_scale(scale, scale), &mut pixmap.as_mut());
    Some(pixmap_to_rgba(&pixmap))
}

/// Copy a premultiplied tiny-skia pixmap into a straight-alpha `RgbaImage`.
fn pixmap_to_rgba(pixmap: &resvg::tiny_skia::Pixmap) -> RgbaImage {
    let mut out = RgbaImage::new(pixmap.width(), pixmap.height());
    for (dst, src) in out.pixels_mut().zip(pixmap.pixels()) {
        let c = src.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    out
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
    use super::{
        B1_BAR, DialStatus, Level, PanelStatus, Renderer, RgbaImage, TEST_FONT, X1_ICON, X1_TITLE, fit_cover,
        glyph_char, load_cover, tap_to_dial,
    };
    use crate::KeyStatus;
    use crate::config::{Action, AudioDevice, BusyFace, Config, DialConfig, KeyConfig, OnFace, Style};
    use crate::probe::Toggle;
    use crate::render::ON_TINT;
    use crate::theme::Rgb;
    use image::Rgba;

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

    /// A renderer with the bundled test font (an OFL subset, `tests/fixtures/test-font.ttf`),
    /// so text pixels don't depend on the machine's fonts. Never skips.
    fn renderer_with_font() -> Renderer {
        Renderer::with_font_bytes(&test_style(), TEST_FONT)
    }

    /// Compare `img` with the golden PNG `name` in `tests/fixtures`, or rewrite the golden
    /// when `OMADECK_UPDATE_GOLDEN=1` is set (then review the PNG diff before committing).
    fn golden(img: &RgbaImage, name: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
        if std::env::var_os("OMADECK_UPDATE_GOLDEN").is_some() {
            img.save(&path).unwrap();
            return;
        }
        let want = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).to_rgba8();
        assert!(*img == want, "{name} differs; rerun with OMADECK_UPDATE_GOLDEN=1 and review the PNG diff");
    }

    fn in_rect(x: u32, y: u32, rect: (u32, u32, u32, u32)) -> bool {
        let (x0, y0, w, h) = rect;
        x >= x0 && x < x0 + w && y >= y0 && y < y0 + h
    }

    #[test]
    fn key_glyph_stays_in_its_box() {
        let mut r = renderer_with_font();
        let style = test_style();
        let key = KeyConfig { label: String::new(), glyph: Some("A".into()), ..KeyConfig::new(0) };
        let img = r.render(Some(&key), &style, 72, &KeyStatus::default());
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
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let cfg = Config::default();
        let status = vec![DialStatus::default(); 4];
        let img = r.render_strip(&cfg, &status, (800, 100));
        assert_eq!(img.dimensions(), (800, 100));
    }

    #[test]
    fn strip_scaled() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let cfg = Config::default();
        let status = vec![DialStatus::default(); 4];
        assert_eq!(r.render_strip(&cfg, &status, (400, 50)).dimensions(), (400, 50));
    }

    #[test]
    fn strip_panel_layout() {
        let mut r = renderer_with_font();
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
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let style = test_style();
        let cfg = Config { style: test_style(), ..Default::default() };
        let panel = r.render_strip_panel(&cfg, 0, 4, DialStatus::default());
        let key = r.render(None, &style, 100, &KeyStatus::default());
        assert_eq!(*panel.get_pixel(100, 50), *key.get_pixel(50, 50));
    }

    #[test]
    fn pressed_b1_recolours_only_the_title() {
        let mut r = renderer_with_font();
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(DialConfig { glyph: Some("A".into()), ..volume_dial() });
        let up = r.render_strip_panel(&cfg, 0, 4, level(0.5, false));
        let down = r.render_strip_panel(&cfg, 0, 4, DialStatus { pressed: true, ..level(0.5, false) });
        let mut title_changed = false;
        for (x, y, p) in down.enumerate_pixels() {
            if in_rect(x, y, X1_TITLE) {
                title_changed |= p != up.get_pixel(x, y);
            } else {
                assert_eq!(p, up.get_pixel(x, y), "({x}, {y}) is outside the title");
            }
        }
        assert!(title_changed, "the title turns accent");
    }

    #[test]
    fn strip_panel_isolation() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
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
            panel: PanelStatus {
                level: Some(Level { value: v, text: if muted { "Muted".into() } else { "50%".into() }, muted }),
                live: None,
            },
        }
    }

    #[test]
    fn strip_volume_bar_fills_to_level() {
        let mut r = renderer_with_font();
        let dial = volume_dial();
        let (bx, by, _bw, bh) = B1_BAR;
        let mid_y = by + bh / 2;
        let x = bx + 90; // the right part of the bar
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(dial);
        let full = r.render_strip_panel(&cfg, 0, 4, level(1.0, false));
        let empty = r.render_strip_panel(&cfg, 0, 4, level(0.0, false));
        assert_ne!(full.get_pixel(x, mid_y), empty.get_pixel(x, mid_y), "the bar fill should change with the level");
    }

    #[test]
    fn strip_volume_muted_leaves_bar_empty() {
        let mut r = renderer_with_font();
        let dial = volume_dial();
        let (bx, by, _bw, bh) = B1_BAR;
        let mid_y = by + bh / 2;
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(dial);
        let muted = r.render_strip_panel(&cfg, 0, 4, level(0.8, true));
        let empty = r.render_strip_panel(&cfg, 0, 4, level(0.0, false));
        // Muted: the bar is the empty track, the same as level 0.
        for x in bx..bx + 108 {
            assert_eq!(*muted.get_pixel(x, mid_y), *empty.get_pixel(x, mid_y), "muted bar should be empty at x = {x}");
        }
    }

    #[test]
    fn strip_volume_without_level_stays_b1() {
        // Before the first poll (level None) the panel is still $B1, not $X1: it draws its
        // groove bar and does not draw the centred $X1 icon.
        let mut r = renderer_with_font();
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(volume_dial());
        let panel = r.render_strip_panel(&cfg, 0, 4, DialStatus::default());
        // No centred $X1 icon (x 76..124) in the icon's vertical band.
        let x1_icon_ink = (76..124).any(|x| (40..72).any(|y| panel.get_pixel(x, y)[0] > 120));
        assert!(!x1_icon_ink, "a $B1 panel should not draw the centred $X1 icon");
        // The $B1 groove bar is visible on the background.
        assert_ne!(*panel.get_pixel(5, 5), *panel.get_pixel(100, 80), "the $B1 groove bar should be visible");
    }

    #[test]
    fn label_color_contrast_kept() {
        let r = Renderer::with_font_bytes(&Style { follow_theme: false, ..Default::default() }, TEST_FONT);
        // White label on a white background flips dark; on black it keeps its contrast.
        assert_eq!(r.label_color(None, &test_style(), Rgb(255, 255, 255)), Rgb(16, 16, 16));
        assert_eq!(r.label_color(None, &test_style(), Rgb(0, 0, 0)), Rgb(255, 255, 255));
    }

    #[test]
    fn fit_cover_exact_size_is_identity() {
        let img = RgbaImage::from_pixel(4, 2, Rgba([1, 2, 3, 255]));
        assert_eq!(fit_cover(&img, 4, 2), img);
    }

    /// A 120 × 10 image in three vertical thirds: red, green, blue.
    fn thirds_wide() -> RgbaImage {
        let mut img = RgbaImage::new(120, 10);
        for row in img.rows_mut() {
            for (x, px) in row.enumerate() {
                *px = if x < 40 {
                    Rgba([255, 0, 0, 255])
                } else if x < 80 {
                    Rgba([0, 255, 0, 255])
                } else {
                    Rgba([0, 0, 255, 255])
                };
            }
        }
        img
    }

    /// A 10 × 120 image in three horizontal thirds: red, green, blue.
    fn thirds_tall() -> RgbaImage {
        let mut img = RgbaImage::new(10, 120);
        for (y, row) in img.rows_mut().enumerate() {
            for px in row {
                *px = if y < 40 {
                    Rgba([255, 0, 0, 255])
                } else if y < 80 {
                    Rgba([0, 255, 0, 255])
                } else {
                    Rgba([0, 0, 255, 255])
                };
            }
        }
        img
    }

    #[test]
    fn fit_cover_wide_crops_centre() {
        let out = fit_cover(&thirds_wide(), 80, 10);
        assert_eq!(out.dimensions(), (80, 10));
        assert_eq!(*out.get_pixel(40, 5), Rgba([0, 255, 0, 255]), "centre is green");
        assert_eq!(*out.get_pixel(79, 5), Rgba([0, 0, 255, 255]), "right edge is blue");
    }

    #[test]
    fn fit_cover_tall_crops_centre() {
        let out = fit_cover(&thirds_tall(), 10, 80);
        assert_eq!(out.dimensions(), (10, 80));
        assert_eq!(*out.get_pixel(5, 40), Rgba([0, 255, 0, 255]), "centre is green");
    }

    /// A PNG in a per-test temp dir.
    fn tmp_image(name: &str, img: &RgbaImage) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("omadeck-cover-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("img.png");
        image::DynamicImage::ImageRgba8(img.clone()).save(&path).unwrap();
        path
    }

    fn solid_img(w: u32, h: u32, c: Rgba<u8>) -> RgbaImage {
        RgbaImage::from_pixel(w, h, c)
    }

    #[test]
    fn clear_cache_keeps_unchanged_backdrop() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let path = tmp_image("keep", &solid_img(4, 2, Rgba([9, 9, 9, 255])));
        assert!(r.backdrop_image(&path, 4, 2).is_some());
        let decodes = r.decode_count.get();
        r.clear_cache();
        assert!(r.backdrop_image(&path, 4, 2).is_some());
        assert_eq!(r.decode_count.get(), decodes, "an unchanged file is not re-decoded");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn failed_decode_is_cached() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let dir = std::env::temp_dir().join(format!("omadeck-cover-garbage-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("broken.png");
        std::fs::write(&path, b"not a png").unwrap();
        assert!(r.backdrop_image(&path, 800, 100).is_none());
        assert!(r.backdrop_image(&path, 800, 100).is_none());
        assert_eq!(r.load_attempts.get(), 1, "a broken file is tried once, not on every paint");
        r.clear_cache();
        assert!(r.backdrop_image(&path, 800, 100).is_none());
        assert_eq!(r.load_attempts.get(), 1, "an unchanged broken file stays cached over a reload");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fit_cover_scales_and_crops() {
        // 100 × 400, red, with a green band at rows 194–206: the centre crop keeps it.
        let mut img = RgbaImage::from_pixel(100, 400, Rgba([255, 0, 0, 255]));
        for y in 194..=206 {
            for px in img.rows_mut().nth(y).unwrap() {
                *px = Rgba([0, 255, 0, 255]);
            }
        }
        let out = fit_cover(&img, 800, 100);
        assert_eq!(out.dimensions(), (800, 100));
        assert_eq!(*out.get_pixel(400, 50), Rgba([0, 255, 0, 255]), "centre is the green band");
        // Odd and exact aspects all come out exactly w × h.
        for (w, h, tw, th) in [(3200, 200, 800, 100), (799, 99, 800, 100), (801, 101, 200, 100), (16, 1000, 800, 100)] {
            let src = RgbaImage::from_pixel(w, h, Rgba([1, 2, 3, 255]));
            assert_eq!(fit_cover(&src, tw, th).dimensions(), (tw, th), "from {w} × {h}");
        }
        assert_eq!(fit_cover(&RgbaImage::new(0, 0), 8, 4).dimensions(), (8, 4));
    }

    #[test]
    fn svg_cover_stays_sharp() {
        let dir = std::env::temp_dir().join(format!("omadeck-svg-sharp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("half-half.svg");
        std::fs::write(
            &path,
            "<svg xmlns='http://www.w3.org/2000/svg' width='16' height='2'><rect x='0' y='0' width='8' height='2' fill='red'/><rect x='8' y='0' width='8' height='2' fill='blue'/></svg>",
        )
        .unwrap();
        let img = load_cover(&path, 800, 100).unwrap();
        let blended = img.rows().nth(50).unwrap().filter(|p| p[0] > 10 && p[2] > 10).count();
        assert!(blended <= 2, "the red/blue boundary is crisp, not a {blended}-px blend");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn touched_file_is_reloaded() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let path = tmp_image("touch", &solid_img(4, 2, Rgba([9, 9, 9, 255])));
        assert!(r.backdrop_image(&path, 4, 2).is_some());
        let decodes = r.decode_count.get();
        // Different pixels and a newer mtime; coarse filesystems may need a wait.
        let old = std::fs::metadata(&path).unwrap().modified().unwrap();
        loop {
            image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(4, 2, Rgba([200, 1, 1, 255]))).save(&path).unwrap();
            if std::fs::metadata(&path).unwrap().modified().unwrap() != old {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(1100));
        }
        r.clear_cache();
        let img = r.backdrop_image(&path, 4, 2).unwrap();
        assert_eq!(r.decode_count.get(), decodes + 1, "a touched file is re-decoded");
        assert_eq!(*img.get_pixel(0, 0), Rgba([200, 1, 1, 255]));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn off_face_ignores_on_table() {
        let style = test_style();
        let mut r = Renderer::with_font_bytes(&style, TEST_FONT);
        let plain = KeyConfig { label: "Mic".into(), glyph: Some("A".into()), ..KeyConfig::new(0) };
        let toggle =
            KeyConfig { on: Some(OnFace { label: Some("Muted".into()), ..OnFace::default() }), ..plain.clone() };
        let off = KeyStatus::default();
        assert_eq!(r.render(Some(&plain), &style, 72, &off), r.render(Some(&toggle), &style, 72, &off));
        let on = KeyStatus { state: Toggle::On, ..KeyStatus::default() };
        assert_ne!(
            r.render(Some(&toggle), &style, 72, &on),
            r.render(Some(&plain), &style, 72, &off),
            "the on face is tinted even without a font"
        );
    }

    #[test]
    fn on_face_without_background_is_tinted() {
        let style = test_style(); // follow_theme false, background #334455
        let mut r = Renderer::with_font_bytes(&style, TEST_FONT);
        let k = KeyConfig { label: "Mic".into(), on: Some(OnFace::default()), ..KeyConfig::new(0) };
        let on = r.render(Some(&k), &style, 72, &KeyStatus { state: Toggle::On, ..KeyStatus::default() });
        // No on background: the key's background mixed 30% towards the label colour
        // (white, the default for an unfollowed style on this dark background).
        let want = Rgb::parse("#334455").unwrap().mix(Rgb(255, 255, 255), ON_TINT);
        assert_eq!(on.get_pixel(0, 0).0[..3], [want.0, want.1, want.2]);
        // A key without an on face is not tinted.
        let plain = KeyConfig { label: "Mic".into(), ..KeyConfig::new(0) };
        let off = r.render(Some(&plain), &style, 72, &KeyStatus::default());
        assert_eq!(off.get_pixel(0, 0).0[..3], [0x33, 0x44, 0x55]);
    }

    #[test]
    fn busy_face_uses_accent_label_by_default() {
        let style = Style { follow_theme: true, ..Default::default() };
        let mut r = Renderer::with_font_bytes(&style, TEST_FONT);
        let k = KeyConfig { label: "LLM".into(), ..KeyConfig::new(0) };
        let busy = KeyStatus { state: Toggle::Busy("Loading 43%".into()), ..KeyStatus::default() };
        let img = r.render(Some(&k), &style, 72, &busy);
        // The label is drawn in the theme accent; check a pixel of the label area
        // against a re-render with the accent forced as the key's label colour.
        let forced = KeyConfig { label: "LLM".into(), label_color: Some(r.theme.accent.hex()), ..KeyConfig::new(0) };
        let forced_img = r.render(Some(&forced), &style, 72, &busy);
        let with_color = KeyConfig { label: "LLM".into(), label_color: Some("#ff0000".into()), ..KeyConfig::new(0) };
        let colored = r.render(Some(&with_color), &style, 72, &busy);
        assert_ne!(img, colored, "a key label colour wins over the accent");
        assert_eq!(img, forced_img, "the default busy label is the accent");
    }

    #[test]
    fn busy_bar_only_when_enabled_and_percent_present() {
        let style = test_style();
        let mut r = Renderer::with_font_bytes(&style, TEST_FONT);
        let key = |bar: bool, label: Option<String>| KeyConfig {
            label: "LLM".into(),
            busy: Some(BusyFace { bar, label, ..BusyFace::default() }),
            ..KeyConfig::new(0)
        };
        let status = |t: &str| KeyStatus { state: Toggle::Busy(t.into()), ..KeyStatus::default() };
        let plain = KeyConfig { label: "LLM".into(), ..KeyConfig::new(0) };
        let off = r.render(Some(&plain), &style, 100, &KeyStatus::default());

        // Bar off, with a percentage: no bar zone, the rows where a bar would be are
        // plain background.
        let no_bar = r.render(Some(&key(false, None)), &style, 100, &status("Loading 43%"));
        let want_bg = Rgb::parse("#334455").unwrap();
        for y in 85..92 {
            assert_eq!(no_bar.get_pixel(0, y).0[..3], [want_bg.0, want_bg.1, want_bg.2], "row {y}");
        }
        // Bar on, no percentage: no bar either.
        let no_pct = r.render(Some(&key(true, None)), &style, 100, &status("Starting"));
        for y in 85..92 {
            assert_eq!(no_pct.get_pixel(0, y).0[..3], [want_bg.0, want_bg.1, want_bg.2], "row {y}");
        }
        // Bar on, 50%: the bar's row shows the fill at 30% of its width and the track
        // at 80%. The bar is x 10..90, y 85..91 (h 6, bottom margin 8).
        let half = r.render(Some(&key(true, None)), &style, 100, &status("Loading 50%"));
        assert_ne!(half, off, "the bar is drawn");
        // fill = the label colour (white for this style), track = bg mixed 22% towards it.
        let fill = Rgb(255, 255, 255);
        let track = want_bg.mix(fill, 0.22);
        assert_eq!(half.get_pixel(20, 88).0[..3], [fill.0, fill.1, fill.2], "inside the fill");
        assert_eq!(half.get_pixel(80, 88).0[..3], [track.0, track.1, track.2], "inside the track");
        // A corner of the bar zone (outside the bar) is plain background.
        assert_eq!(half.get_pixel(0, 99).0[..3], [want_bg.0, want_bg.1, want_bg.2]);
    }

    #[test]
    fn draw_bar_clamps_its_value() {
        let (fill, track) = (Rgb(255, 0, 0), Rgb(0, 0, 255));
        let bar = |v: f32| {
            let mut img = RgbaImage::new(200, 100);
            super::draw_bar(&mut img, B1_BAR, v, fill, track);
            img
        };
        assert_eq!(bar(1.7), bar(1.0), "above 1 is full");
        assert_eq!(bar(-0.5), bar(0.0), "below 0 is empty");
        assert_eq!(bar(f32::NAN), bar(0.0), "NaN is empty");
        // Half full: red at a quarter of the bar, blue at three quarters.
        let (bx, by, bw, bh) = B1_BAR;
        let half = bar(0.5);
        assert_eq!(half.get_pixel(bx + bw / 4, by + bh / 2).0[..3], [255, 0, 0]);
        assert_eq!(half.get_pixel(bx + bw * 3 / 4, by + bh / 2).0[..3], [0, 0, 255]);
    }

    #[test]
    fn backdrop_is_shared_and_only_rechecked_on_clear_cache() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let path = tmp_image("stamp", &solid_img(4, 2, Rgba([9, 9, 9, 255])));
        let first = r.backdrop_image(&path, 4, 2).unwrap();
        // A bigger file under the same name: a paint doesn't look, so the cached one stays.
        image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 4, Rgba([200, 1, 1, 255]))).save(&path).unwrap();
        let again = r.backdrop_image(&path, 4, 2).unwrap();
        assert!(std::sync::Arc::ptr_eq(&first, &again), "a hit shares the decoded image");
        // A reload sees the new length and loads it.
        r.clear_cache();
        assert_eq!(*r.backdrop_image(&path, 4, 2).unwrap().get_pixel(0, 0), Rgba([200, 1, 1, 255]));
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    /// An 800 × 100 strip image in four vertical quarters, one colour each.
    fn quarters_strip() -> RgbaImage {
        let cols = [Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255]), Rgba([0, 0, 255, 255]), Rgba([255, 255, 0, 255])];
        let mut img = RgbaImage::new(800, 100);
        for row in img.rows_mut() {
            for (x, px) in row.enumerate() {
                *px = cols[x / 200];
            }
        }
        img
    }

    #[test]
    fn strip_image_slices_per_panel() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let path = tmp_image("quarters", &quarters_strip());
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(path.to_string_lossy().into());
        let cols = [Rgba([255, 0, 0, 255]), Rgba([0, 255, 0, 255]), Rgba([0, 0, 255, 255]), Rgba([255, 255, 0, 255])];
        for i in 0..4 {
            let panel = r.render_strip_panel(&cfg, i, 4, DialStatus::default());
            assert_eq!(*panel.get_pixel(5, 5), cols[i as usize], "panel {i} shows its quarter");
        }
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn dial_image_wins_over_strip() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let strip = tmp_image("win-strip", &quarters_strip());
        let dial = tmp_image("win-dial", &solid_img(200, 100, Rgba([1, 2, 3, 255])));
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        cfg.dials.push(DialConfig { background_image: Some(dial.to_string_lossy().into()), ..DialConfig::new(1) });
        assert_eq!(*r.render_strip_panel(&cfg, 1, 4, DialStatus::default()).get_pixel(5, 5), Rgba([1, 2, 3, 255]));
        assert_eq!(*r.render_strip_panel(&cfg, 0, 4, DialStatus::default()).get_pixel(5, 5), Rgba([255, 0, 0, 255]));
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    #[test]
    fn transparent_dial_image_shows_strip() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let strip = tmp_image("trans-strip", &quarters_strip());
        let dial = tmp_image("trans-dial", &solid_img(200, 100, Rgba([0, 0, 0, 0])));
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        cfg.dials.push(DialConfig { background_image: Some(dial.to_string_lossy().into()), ..DialConfig::new(2) });
        assert_eq!(*r.render_strip_panel(&cfg, 2, 4, DialStatus::default()).get_pixel(5, 5), Rgba([0, 0, 255, 255]));
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    #[test]
    fn dial_colour_covers_strip_image() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let strip = tmp_image("colour-strip", &quarters_strip());
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        cfg.dials.push(DialConfig { background: Some("#0a0b0c".into()), ..DialConfig::new(3) });
        assert_eq!(*r.render_strip_panel(&cfg, 3, 4, DialStatus::default()).get_pixel(5, 5), Rgba([10, 11, 12, 255]));
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    #[test]
    fn empty_slot_shows_strip_undimmed() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let strip = tmp_image("empty-strip", &quarters_strip());
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        // No dial at all: the panel shows its quarter, not the dimmed base colour.
        assert_eq!(*r.render_strip_panel(&cfg, 0, 4, DialStatus::default()).get_pixel(5, 5), Rgba([255, 0, 0, 255]));
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    #[test]
    fn missing_image_falls_back_to_colour() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let mut cfg = Config { style: test_style(), ..Default::default() };
        let with = r.render_strip_panel(&cfg, 0, 4, DialStatus::default());
        cfg.strip.background_image = Some("/nonexistent/omadeck-strip-test.png".into());
        let without = r.render_strip_panel(&cfg, 0, 4, DialStatus::default());
        assert_eq!(with, without, "a missing image looks like no image");
    }

    #[test]
    fn bright_image_flips_label_dark() {
        let mut r = renderer_with_font();
        let strip = tmp_image("bright", &solid_img(800, 100, Rgba([255, 255, 255, 255])));
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        cfg.dials.push(DialConfig { label: "Volume".into(), glyph: Some("A".into()), ..DialConfig::new(0) });
        let panel = r.render_strip_panel(&cfg, 0, 4, DialStatus::default());
        // The title ink (in X1_TITLE, text and its shadow) must be dark on the white
        // image; on the old dark background the same title would be near-white.
        let (x0, y0, w, h) = X1_TITLE;
        let mut ink = Vec::new();
        for dx in 0..w {
            for dy in 0..h {
                let p = *panel.get_pixel(x0 + dx, y0 + dy);
                if p[0] < 200 {
                    ink.push(p);
                }
            }
        }
        assert!(!ink.is_empty(), "no title ink");
        // Antialiased edges blend toward white, so judge the ink by its mean.
        let mean: u32 =
            ink.iter().map(|p| p[0] as u32 + p[1] as u32 + p[2] as u32).sum::<u32>() / (3 * ink.len() as u32);
        assert!(mean < 128, "the title must flip dark, mean was {mean}");
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    #[test]
    fn panel_isolation_with_strip_image() {
        let mut r = Renderer::with_font_bytes(&test_style(), TEST_FONT);
        let strip = tmp_image("iso", &quarters_strip());
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.strip.background_image = Some(strip.to_string_lossy().into());
        cfg.dials.push(DialConfig { label: "One".into(), ..DialConfig::new(0) });
        cfg.dials.push(DialConfig { label: "Two".into(), ..DialConfig::new(2) });
        let status = vec![DialStatus::default(); 4];
        let before = r.render_strip(&cfg, &status, (800, 100));
        cfg.dials[1].label = "Changed".into();
        let after = r.render_strip(&cfg, &status, (800, 100));
        for (x0, changed) in [(0u32, false), (200, false), (400, true), (600, false)] {
            let same = (0..200).all(|dx| (0..100).all(|y| before.get_pixel(x0 + dx, y) == after.get_pixel(x0 + dx, y)));
            assert_eq!(same, !changed, "panel at x = {x0} should {changed}have changed");
        }
        let _ = std::fs::remove_dir_all(strip.parent().unwrap());
    }

    /// The no-image panels must be pixel-identical to the goldens. They use the bundled
    /// font and the glyph "A", so they don't depend on the machine's fonts.
    #[test]
    fn no_images_pixels_unchanged() {
        let mut r = renderer_with_font();
        let cfg = Config { style: test_style(), ..Default::default() };
        golden(&r.render_strip_panel(&cfg, 0, 4, DialStatus::default()), "strip-empty.png");
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(DialConfig { label: "Volume".into(), glyph: Some("A".into()), ..DialConfig::new(0) });
        golden(&r.render_strip_panel(&cfg, 0, 4, DialStatus::default()), "strip-x1.png");
        let mut cfg = Config { style: test_style(), ..Default::default() };
        cfg.dials.push(DialConfig { glyph: Some("A".into()), ..volume_dial() });
        let st = DialStatus {
            pressed: false,
            panel: PanelStatus { level: Some(Level { value: 0.4, text: "40%".into(), muted: false }), live: None },
        };
        golden(&r.render_strip_panel(&cfg, 0, 4, st), "strip-b1.png");
    }

    #[test]
    fn volume_glyph_defaults() {
        use super::{INPUT_GLYPH, MUTE_GLYPH, OUTPUT_GLYPH, volume_glyph};
        let plain = volume_dial();
        let face = plain.face();
        assert_eq!(volume_glyph(&face, AudioDevice::Output, false), OUTPUT_GLYPH);
        assert_eq!(volume_glyph(&face, AudioDevice::Input, false), INPUT_GLYPH);
        assert_eq!(volume_glyph(&face, AudioDevice::Input, true), MUTE_GLYPH);
        let own = DialConfig { glyph: Some("A".into()), ..volume_dial() };
        assert_eq!(volume_glyph(&own.face(), AudioDevice::Output, false), "A");
        assert_eq!(INPUT_GLYPH, "f036c");
    }
}
