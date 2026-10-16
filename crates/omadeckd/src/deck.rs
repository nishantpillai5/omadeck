//! The connected Stream Deck: its handle, per-device scratch state, and painting.
//! Nothing here outlives an unplug; the daemon's `Runtime` holds what must.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use omadeck_core::device::{self, DeviceStateReader, HidApi, ImageRect, Kind, StreamDeck};
use omadeck_core::probe::{self, Values};
use omadeck_core::{Config, DeckInfo, DialStatus, PANEL_W, Renderer};

/// How long a panel stays highlighted after a tap on the strip.
pub const DIAL_FLASH: Duration = Duration::from_millis(150);

pub struct Deck {
    pub dev: Arc<StreamDeck>,
    pub reader: Arc<DeviceStateReader>,
    pub kind: Kind,
    pub info: DeckInfo,
    /// Whether each dial is physically pressed down.
    pub dial_pressed: Vec<bool>,
    /// When each dial's panel was tapped (the tap time, not a deadline), while it flashes.
    pub flash_until: Vec<Option<Instant>>,
    /// Panels to repaint at the end of this pass (`flush_dirty`), however many changes
    /// dirtied them. PR 8 adds `dirty_keys` next to it.
    pub dirty_panels: Vec<bool>,
}

impl Deck {
    /// Whether dial `i` is drawn pressed: held down, or tapped less than `DIAL_FLASH` ago.
    pub fn pressed(&self, i: u8, now: Instant) -> bool {
        let n = i as usize;
        let flashed = self.flash_until.get(n).and_then(|t| *t).is_some_and(|t| now.duration_since(t) < DIAL_FLASH);
        self.dial_pressed.get(n).copied().unwrap_or(false) || flashed
    }

    /// Mark dial `i`'s panel for the end-of-pass paint.
    pub fn mark_dirty(&mut self, i: u8) {
        if let Some(f) = self.dirty_panels.get_mut(i as usize) {
            *f = true;
        }
    }

    /// Mark the panels whose dial `pred` picks for repainting at the end of the pass.
    pub fn dirty_where(&mut self, cfg: &Config, pred: impl Fn(Option<&omadeck_core::DialConfig>) -> bool) {
        for i in 0..self.info.dials {
            if pred(cfg.dial(i)) {
                self.dirty_panels[i as usize] = true;
            }
        }
    }

    /// When the earliest tap flash ends, if any is showing.
    pub fn flash_deadline(&self) -> Option<Instant> {
        self.flash_until.iter().flatten().map(|t| *t + DIAL_FLASH).min()
    }

    /// End the tap flashes that have run their time; their panels repaint this pass.
    pub fn expire_flashes(&mut self, now: Instant) {
        for i in 0..self.flash_until.len() {
            if self.flash_until[i].is_some_and(|t| now.duration_since(t) >= DIAL_FLASH) {
                self.flash_until[i] = None;
                self.dirty_panels[i] = true;
            }
        }
    }
}

/// Open the first Stream Deck found. Doesn't paint it: the daemon does, with its values.
// elgato-streamdeck hands out its reader via `Arc<Self>`; everything stays on this thread.
#[allow(clippy::arc_with_non_send_sync)]
pub fn open(hid: &mut HidApi) -> Option<Deck> {
    if let Err(e) = device::refresh_device_list(hid) {
        log::warn!("hidapi refresh: {e}");
        return None;
    }
    let (kind, serial) = device::list_devices(hid).into_iter().find(|(k, _)| k.is_visual())?;
    let dev = match StreamDeck::connect(hid, kind, &serial) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            log::warn!("found {} but could not open it: {e} (is the udev rule installed?)", device::model_name(kind));
            return None;
        }
    };
    let reader = dev.get_reader();
    let info = DeckInfo::new(kind, &serial);
    let n_dials = info.dials as usize;
    let deck = Deck {
        dev,
        reader,
        kind,
        info,
        dial_pressed: vec![false; n_dials],
        flash_until: vec![None; n_dials],
        dirty_panels: vec![false; n_dials],
    };
    log::info!(
        "connected {} (serial {}, firmware {})",
        deck.info.model,
        serial,
        deck.dev.firmware_version().unwrap_or_default()
    );
    Some(deck)
}

/// What panel `i` shows now.
pub fn panel_status(d: &Deck, cfg: &Config, values: &Values, i: u8, now: Instant) -> DialStatus {
    probe::dial_status(cfg.dial(i), values, d.pressed(i, now))
}

/// Repaint everything: brightness, every key, every strip panel, one flush. Clears the
/// dirty panels, which it has just painted.
pub fn paint_all(
    d: &mut Deck,
    cfg: &Config,
    page: Option<&str>,
    renderer: &mut Renderer,
    values: &Values,
) -> Result<()> {
    d.dev.set_brightness(values.brightness)?;
    for i in 0..d.kind.key_count() {
        write_key(d, cfg, page, renderer, i, false)?;
    }
    // The touch strip, panel by panel (the same 200×100 writes a single-panel update
    // uses); it repaints on connect, reload and theme change, so follow_theme and hot
    // reload come for free. The first writes right after connect can be dropped by the
    // Plus's LCD, so the daemon repaints once more a moment later (POST_CONNECT_REPAINT).
    if d.info.strip.is_some() {
        let now = Instant::now();
        for i in 0..d.info.dials {
            write_panel(d, cfg, renderer, values, i, now)?;
        }
    }
    d.dirty_panels.fill(false);
    d.dev.flush()?;
    Ok(())
}

/// Paint what this pass dirtied, with one flush, and clear it. The only place panels are
/// painted outside `paint_all`. PR 8 adds `dirty_keys` here (and to `paint_all`'s clear).
pub fn flush_dirty(d: &mut Deck, cfg: &Config, renderer: &mut Renderer, values: &Values) {
    if d.info.strip.is_none() || !d.dirty_panels.contains(&true) {
        d.dirty_panels.fill(false);
        return;
    }
    let now = Instant::now();
    for i in 0..d.info.dials {
        if d.dirty_panels[i as usize]
            && let Err(e) = write_panel(d, cfg, renderer, values, i, now)
        {
            log::warn!("paint dial {i}: {e}");
        }
    }
    d.dirty_panels.fill(false);
    if let Err(e) = d.dev.flush() {
        log::warn!("flush: {e}");
    }
}

/// Paint one key at once (presses must show without waiting for the pass to end).
pub fn paint_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) {
    if let Err(e) = write_key(d, cfg, page, renderer, i, pressed).and_then(|_| Ok(d.dev.flush()?)) {
        log::warn!("paint key {i}: {e}");
    }
}

/// Queue one 200×100 dial panel for the next `flush`.
fn write_panel(d: &Deck, cfg: &Config, renderer: &mut Renderer, values: &Values, i: u8, now: Instant) -> Result<()> {
    let st = panel_status(d, cfg, values, i, now);
    let panel = renderer.render_strip_panel(cfg, i, d.info.dials as u32, st);
    let rect = ImageRect::from_image(image::DynamicImage::ImageRgba8(panel))?;
    d.dev.write_lcd(i as u16 * PANEL_W as u16, 0, &rect)?;
    Ok(())
}

fn write_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) -> Result<()> {
    let face = renderer.render(cfg.key(page, i), &cfg.style, d.info.key_size, pressed);
    d.dev.set_button_image(i, image::DynamicImage::ImageRgba8(face))?;
    Ok(())
}
