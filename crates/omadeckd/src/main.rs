//! omadeckd — owns the Stream Deck: paints key faces, runs actions on press, follows
//! config and Omarchy theme changes live, and survives unplug/replug.

mod hub;
mod nav;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use omadeck_core::device::{self, DeviceStateReader, DeviceStateUpdate, HidApi, ImageRect, Kind, StreamDeck};
use omadeck_core::ipc::{Command, Event};
use omadeck_core::paths::FileStamp;
use omadeck_core::{Action, Config, DeckInfo, DialStatus, PANEL_W, Renderer, action, paths, tap_to_dial};

use hub::Hub;
use nav::Nav;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const READ_TIMEOUT: Duration = Duration::from_millis(100);
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
/// How often to double-check the config by stat, for changes inotify can't see.
const STAMP_INTERVAL: Duration = Duration::from_secs(1);
/// How long a panel stays highlighted after a tap on the strip.
const DIAL_FLASH: Duration = Duration::from_millis(150);
/// The Plus's LCD can ignore the first writes right after connect; repaint once after this.
const POST_CONNECT_REPAINT: Duration = Duration::from_secs(1);

struct Deck {
    dev: Arc<StreamDeck>,
    reader: Arc<DeviceStateReader>,
    kind: Kind,
    info: DeckInfo,
    /// Whether each dial is physically pressed down.
    dial_pressed: Vec<bool>,
    /// Until when each dial's panel stays highlighted after a tap on the strip.
    flash_until: Vec<Option<Instant>>,
}

/// The live state of dial `i` for the strip renderer.
fn dial_status(d: &Deck, i: u8, now: Instant) -> DialStatus {
    let n = i as usize;
    let flashed = d.flash_until.get(n).and_then(|t| *t).is_some_and(|t| now.duration_since(t) < DIAL_FLASH);
    DialStatus { pressed: d.dial_pressed.get(n).copied().unwrap_or(false) || flashed, level: None }
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).format_timestamp(None).init();

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        None | Some("run") => {}
        Some("list") => {
            let decks = device::detect();
            if decks.is_empty() {
                println!("No Stream Deck found.");
            }
            for d in decks {
                let mut line =
                    format!("{}  serial={}  {}x{} keys  {}px", d.model, d.serial, d.cols, d.rows, d.key_size);
                if d.dials > 0 {
                    line.push_str(&format!("  · {} dials", d.dials));
                }
                if let Some((w, h)) = d.strip {
                    line.push_str(&format!("  · {w}x{h} strip"));
                }
                println!("{line}");
            }
            return;
        }
        Some("press") => {
            let Some(i) = args.next().and_then(|v| v.parse::<u8>().ok()) else {
                eprintln!("usage: omadeckd press KEY [PAGE]   (keys are numbered from 0)");
                std::process::exit(2);
            };
            let page = args.next();
            let cfg = load_config();
            let Some(keys) = cfg.page(page.as_deref()) else {
                eprintln!("no page named {}", page.unwrap_or_default());
                std::process::exit(1);
            };
            let act = keys.get(i).map(|k| k.action.clone()).unwrap_or_default();
            println!("key {i}: {}", act.summary());
            if let Err(e) = action::run(&act) {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            // Give the detached child a moment to start before we exit.
            std::thread::sleep(Duration::from_millis(300));
            return;
        }
        Some("dial") => {
            let (Some(n), Some(which)) = (args.next(), args.next()) else {
                eprintln!("usage: omadeckd dial N left|right|press");
                std::process::exit(2);
            };
            let Ok(i) = n.parse::<u8>() else {
                eprintln!("usage: omadeckd dial N left|right|press");
                std::process::exit(2);
            };
            let cfg = load_config();
            let act = match which.as_str() {
                "left" => cfg.dial(i).map(|d| d.turn_left.clone()).unwrap_or_default(),
                "right" => cfg.dial(i).map(|d| d.turn_right.clone()).unwrap_or_default(),
                "press" => cfg.dial(i).map(|d| d.press.clone()).unwrap_or_default(),
                _ => {
                    eprintln!("usage: omadeckd dial N left|right|press");
                    std::process::exit(2);
                }
            };
            println!("dial {i} {which}: {}", act.summary());
            if let Err(e) = action::run(&act) {
                eprintln!("{e:#}");
                std::process::exit(1);
            }
            std::thread::sleep(Duration::from_millis(300));
            return;
        }
        Some("snapshot") => {
            let out = args.next().unwrap_or_else(|| "omadeck.png".into());
            if let Err(e) = snapshot(Path::new(&out), args.next().as_deref()) {
                eprintln!("snapshot failed: {e:#}");
                std::process::exit(1);
            }
            println!("wrote {out}");
            return;
        }
        Some("-V" | "--version") => {
            println!("omadeckd {}", env!("CARGO_PKG_VERSION"));
            return;
        }
        Some(_) => {
            eprintln!(
                "usage: omadeckd [run | list | press KEY [PAGE] | dial N left|right|press | snapshot FILE.png [PAGE] | --version]\n\nRuns the omadeck Stream Deck daemon."
            );
            std::process::exit(2);
        }
    }

    if let Err(e) = run() {
        log::error!("{e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let socket = paths::socket_path()?;
    if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
        anyhow::bail!("another omadeckd is already running ({})", socket.display());
    }

    let term = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(sig, term.clone())?;
    }

    let hub = Hub::start(socket)?;

    // Watch the config dir and Omarchy's theme state for changes.
    std::fs::create_dir_all(paths::config_dir())?;
    let (tx, rx) = mpsc::channel::<()>();
    let mut watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res
            && !matches!(ev.kind, notify::EventKind::Access(_))
            && ev.paths.iter().any(|p| relevant(p))
        {
            let _ = tx.send(());
        }
    })?;
    watcher.watch(&paths::config_dir(), RecursiveMode::NonRecursive)?;
    let mut dir_stamp = FileStamp::of(&paths::config_dir());
    let mut cfg_stamp = FileStamp::of(&paths::config_file());
    let mut last_stamp_check = Instant::now();
    let omarchy = paths::omarchy_current_dir();
    if omarchy.is_dir() {
        watcher.watch(&omarchy, RecursiveMode::NonRecursive)?;
    }
    if paths::icons_dir().is_dir() {
        let _ = watcher.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
    }

    let mut cfg = load_config();
    let mut renderer = Renderer::new(&cfg.style);
    let mut nav = Nav::default();
    // A page the configurator asked for that isn't in the config yet (it saves a moment later).
    let mut wanted_page: Option<String> = None;
    log::info!("theme: {}", renderer.theme.name);

    let mut hid = device::new_hidapi().context("initialising hidapi")?;
    let mut deck: Option<Deck> = None;
    let mut last_scan: Option<Instant> = None;
    let mut reload_at: Option<Instant> = None;
    let mut repaint_at: Option<Instant> = None;

    log::info!("omadeckd {} ready; config {}", env!("CARGO_PKG_VERSION"), paths::config_file().display());

    while !term.load(Ordering::Relaxed) {
        // Coalesce bursts of filesystem events into one reload.
        while rx.try_recv().is_ok() {
            reload_at = Some(Instant::now() + RELOAD_DEBOUNCE);
        }
        // inotify misses a symlinked config's target, and its watch dies with a config
        // directory that's deleted and recreated (restoring a backup), so also compare stamps.
        if last_stamp_check.elapsed() >= STAMP_INTERVAL {
            last_stamp_check = Instant::now();
            let d = FileStamp::of(&paths::config_dir());
            // The directory's mtime moves on every save; only its identity matters here.
            if !FileStamp::same_file(d, dir_stamp) {
                dir_stamp = d;
                let _ = watcher.unwatch(&paths::config_dir());
                if d.is_some() {
                    log::info!("config directory was replaced; watching the new one");
                    let _ = watcher.watch(&paths::config_dir(), RecursiveMode::NonRecursive);
                    if paths::icons_dir().is_dir() {
                        let _ = watcher.unwatch(&paths::icons_dir());
                        let _ = watcher.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
                    }
                }
            }
            let c = FileStamp::of(&paths::config_file());
            if c != cfg_stamp {
                cfg_stamp = c;
                reload_at.get_or_insert(Instant::now());
            }
        }
        if reload_at.is_some_and(|t| Instant::now() >= t) {
            reload_at = None;
            // A typo while hand-editing shouldn't wipe the deck: keep the last good config.
            match Config::load() {
                Ok(new_cfg) => cfg = new_cfg,
                Err(e) => log::error!("{e:#}; keeping the previous config"),
            }
            renderer.update_style(&cfg.style);
            renderer.reload_theme();
            renderer.clear_cache();
            log::info!("reloaded config (theme: {})", renderer.theme.name);
            let mut moved = nav.retain_existing(&cfg);
            if let Some(p) = wanted_page.take_if(|p| cfg.has_page(p)) {
                moved |= nav.go(Some(&p));
            }
            if moved {
                hub.publish(Event::PageChanged { page: nav.current().map(str::to_string) });
            }
            if let Some(d) = &deck
                && let Err(e) = paint_all(d, &cfg, nav.current(), &mut renderer)
            {
                log::warn!("repaint failed: {e}");
            }
        }

        // Only the latest page request counts, so flicking through pages in the configurator
        // doesn't echo a trail of stale page changes back to it.
        let mut show = None;
        for cmd in hub.commands() {
            match cmd {
                Command::ShowPage { page } => show = Some(page),
            }
        }
        if let Some(page) = show {
            wanted_page = None;
            match page {
                Some(p) if !cfg.has_page(&p) => wanted_page = Some(p),
                page => {
                    if nav.go(page.as_deref()) {
                        show_page(deck.as_ref(), &cfg, &nav, &mut renderer, &hub);
                    }
                }
            }
        }

        let Some(d) = &mut deck else {
            if last_scan.is_none_or(|t| t.elapsed() >= SCAN_INTERVAL) {
                last_scan = Some(Instant::now());
                deck = connect(&mut hid, &cfg, nav.current(), &mut renderer);
                hub.publish(match &deck {
                    Some(d) => Event::Connected { deck: d.info.clone() },
                    None => Event::Disconnected,
                });
                if deck.is_some() {
                    repaint_at = Some(Instant::now() + POST_CONNECT_REPAINT);
                }
            } else {
                std::thread::sleep(READ_TIMEOUT);
            }
            continue;
        };

        let now = Instant::now();

        // The Plus's LCD can ignore the first writes right after connect; repaint once.
        if repaint_at.is_some_and(|t| now >= t) {
            repaint_at = None;
            if let Err(e) = paint_all(d, &cfg, nav.current(), &mut renderer) {
                log::warn!("post-connect repaint failed: {e}");
            }
        }

        // Expire tap flashes so a panel only stays highlighted for a moment.
        let expired: Vec<u8> = d
            .flash_until
            .iter_mut()
            .enumerate()
            .filter(|(_, t)| t.is_some_and(|t| now.duration_since(t) >= DIAL_FLASH))
            .map(|(i, t)| {
                *t = None;
                i as u8
            })
            .collect();
        for i in expired {
            paint_dial(d, &cfg, &mut renderer, i);
        }

        match d.reader.read(Some(READ_TIMEOUT)) {
            Ok(updates) => {
                for u in updates {
                    match u {
                        DeviceStateUpdate::ButtonDown(i) => {
                            paint(d, &cfg, nav.current(), &mut renderer, i, true);
                            hub.publish(Event::KeyDown { index: i, page: nav.current().map(str::to_string) });
                            let Some(k) = cfg.key(nav.current(), i) else { continue };
                            log::info!("key {i}: {}", k.action.summary());
                            let moved = match &k.action {
                                Action::Page { page } if cfg.has_page(page) => Ok(nav.go(Some(page))),
                                Action::Page { page } => Err(anyhow::anyhow!("there's no page named “{page}”")),
                                Action::Back => Ok(nav.back()),
                                a => action::run(a).map(|_| false),
                            };
                            match moved {
                                Ok(true) => show_page(Some(d), &cfg, &nav, &mut renderer, &hub),
                                Ok(false) => {}
                                Err(e) => {
                                    log::warn!("key {i}: {e:#}");
                                    hub.publish(Event::ActionFailed { index: i, message: format!("{e:#}") });
                                }
                            }
                        }
                        DeviceStateUpdate::ButtonUp(i) => {
                            paint(d, &cfg, nav.current(), &mut renderer, i, false);
                            hub.publish(Event::KeyUp { index: i });
                        }
                        DeviceStateUpdate::EncoderTwist(i, delta) => {
                            log::debug!("dial {i} turned {delta}");
                            hub.publish(Event::DialTurn { index: i, delta });
                            if let Some(d) = cfg.dial(i) {
                                let (act, dir) =
                                    if delta >= 0 { (&d.turn_right, "right") } else { (&d.turn_left, "left") };
                                let steps = delta.unsigned_abs().min(8);
                                if !act.is_none() {
                                    log::info!("dial {i} turned {dir} x{steps}: {}", act.summary());
                                    if let Err(e) = action::run_steps(act, steps) {
                                        log::warn!("dial {i}: {e:#}");
                                        hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                    }
                                }
                            }
                        }
                        DeviceStateUpdate::EncoderDown(i) => {
                            log::info!("dial {i} pressed");
                            hub.publish(Event::DialDown { index: i });
                            if let Some(p) = d.dial_pressed.get_mut(i as usize) {
                                *p = true;
                            }
                            paint_dial(d, &cfg, &mut renderer, i);
                            if let Some(dc) = cfg.dial(i)
                                && !dc.press.is_none()
                            {
                                log::info!("dial {i}: {}", dc.press.summary());
                                if let Err(e) = action::run(&dc.press) {
                                    log::warn!("dial {i}: {e:#}");
                                    hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                }
                            }
                        }
                        DeviceStateUpdate::EncoderUp(i) => {
                            log::info!("dial {i} released");
                            hub.publish(Event::DialUp { index: i });
                            if let Some(p) = d.dial_pressed.get_mut(i as usize) {
                                *p = false;
                            }
                            paint_dial(d, &cfg, &mut renderer, i);
                        }
                        DeviceStateUpdate::TouchScreenPress(x, y) => {
                            log::info!("strip tap at ({x}, {y})");
                            hub.publish(Event::StripTap { x, y });
                            // A tap on a panel does the same as pressing its dial, as in
                            // the Elgato app: run the dial's press and flash the title.
                            if let Some((w, _)) = d.info.strip
                                && let Some(i) = tap_to_dial(x as u32, w, d.info.dials as usize)
                            {
                                log::info!("strip tap on dial {i}");
                                if let Some(t) = d.flash_until.get_mut(i) {
                                    // The tap time, not a deadline: `dial_status` and the
                                    // expiry sweep both measure elapsed time since it.
                                    *t = Some(Instant::now());
                                }
                                paint_dial(d, &cfg, &mut renderer, i as u8);
                                if let Some(dc) = cfg.dial(i as u8)
                                    && !dc.press.is_none()
                                {
                                    log::info!("dial {i}: {}", dc.press.summary());
                                    if let Err(e) = action::run(&dc.press) {
                                        log::warn!("dial {i}: {e:#}");
                                        hub.publish(Event::DialActionFailed {
                                            index: i as u8,
                                            message: format!("{e:#}"),
                                        });
                                    }
                                }
                            }
                        }
                        DeviceStateUpdate::TouchScreenLongPress(x, y) => {
                            // Published for now; a `long_press` action may use it later.
                            log::info!("strip long press at ({x}, {y})");
                            hub.publish(Event::StripLongPress { x, y });
                        }
                        DeviceStateUpdate::TouchScreenSwipe(from, to) => {
                            // Published for now; PR 5 uses it to change page, as the
                            // Elgato app does.
                            log::info!("strip swipe {from:?} to {to:?}");
                            hub.publish(Event::StripSwipe { from, to });
                        }
                        // Neo touch points; nothing reacts to them yet.
                        _ => {}
                    }
                }
            }
            Err(_) if term.load(Ordering::Relaxed) => break,
            Err(e) => {
                log::warn!("lost {} ({e}); waiting for it to come back", d.info.model);
                deck = None;
                last_scan = None;
                hub.publish(Event::Disconnected);
            }
        }
    }

    log::info!("shutting down");
    if let Some(d) = deck {
        // Hand the deck back to its idle logo screen.
        let _ = d.dev.reset();
    }
    hub.shutdown();
    Ok(())
}

fn relevant(p: &Path) -> bool {
    let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if p.starts_with(paths::omarchy_current_dir()) {
        return name == "theme.name" || name == "theme";
    }
    !name.starts_with('.')
}

fn load_config() -> Config {
    match Config::load() {
        Ok(c) => c,
        Err(e) => {
            log::error!("{e:#}; using an empty config");
            Config::default()
        }
    }
}

// elgato-streamdeck hands out its reader via `Arc<Self>`; everything stays on this thread.
#[allow(clippy::arc_with_non_send_sync)]
fn connect(hid: &mut HidApi, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Option<Deck> {
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
    let deck = Deck { dev, reader, kind, info, dial_pressed: vec![false; n_dials], flash_until: vec![None; n_dials] };
    log::info!(
        "connected {} (serial {}, firmware {})",
        deck.info.model,
        serial,
        deck.dev.firmware_version().unwrap_or_default()
    );
    if let Err(e) = paint_all(&deck, cfg, page, renderer) {
        log::warn!("initial paint failed: {e}");
    }
    Some(deck)
}

/// Repaint the deck after `nav` moved to another page, and tell the configurators.
fn show_page(d: Option<&Deck>, cfg: &Config, nav: &Nav, renderer: &mut Renderer, hub: &Hub) {
    log::info!("page: {}", nav.current().unwrap_or(omadeck_core::HOME));
    if let Some(d) = d
        && let Err(e) = paint_all(d, cfg, nav.current(), renderer)
    {
        log::warn!("repaint failed: {e}");
    }
    hub.publish(Event::PageChanged { page: nav.current().map(str::to_string) });
}

fn paint_all(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Result<()> {
    d.dev.set_brightness(cfg.brightness)?;
    for i in 0..d.kind.key_count() {
        write_key(d, cfg, page, renderer, i, false)?;
    }
    // The touch strip, panel by panel (the same 200×100 writes a single-panel update
    // uses); it repaints on connect, reload and theme change, so follow_theme and hot
    // reload come for free. The first writes right after connect can be dropped by the
    // Plus's LCD, so `run` repaints once more a moment later (POST_CONNECT_REPAINT).
    if d.info.strip.is_some() {
        for i in 0..d.info.dials {
            write_panel(d, cfg, renderer, i)?;
        }
    }
    d.dev.flush()?;
    Ok(())
}

fn paint(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) {
    if let Err(e) = write_key(d, cfg, page, renderer, i, pressed).and_then(|_| Ok(d.dev.flush()?)) {
        log::warn!("paint key {i}: {e}");
    }
}

/// Queue one 200×100 dial panel for the next `flush`.
fn write_panel(d: &Deck, cfg: &Config, renderer: &mut Renderer, i: u8) -> Result<()> {
    let panel = renderer.render_strip_panel(cfg.dial(i), &cfg.style, dial_status(d, i, Instant::now()));
    let rect = ImageRect::from_image(image::DynamicImage::ImageRgba8(panel))?;
    d.dev.write_lcd(i as u16 * PANEL_W as u16, 0, &rect)?;
    Ok(())
}

/// Redraw one 200×100 dial panel. Fast spins never reach this: turns don't change the
/// panel in this PR, so a spin can't queue a write per detent.
fn paint_dial(d: &Deck, cfg: &Config, renderer: &mut Renderer, i: u8) {
    if d.info.strip.is_none() || i >= d.info.dials {
        return;
    }
    if let Err(e) = write_panel(d, cfg, renderer, i).and_then(|_| Ok(d.dev.flush()?)) {
        log::warn!("paint dial {i}: {e}");
    }
}

fn write_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) -> Result<()> {
    let face = renderer.render(cfg.key(page, i), &cfg.style, d.info.key_size, pressed);
    d.dev.set_button_image(i, image::DynamicImage::ImageRgba8(face))?;
    Ok(())
}

/// Render the configured layout to a PNG, the way it looks on the hardware.
fn snapshot(out: &Path, page: Option<&str>) -> Result<()> {
    let cfg = Config::load()?;
    let keys = cfg.page(page).with_context(|| format!("no page named {}", page.unwrap_or_default()))?;
    let info = device::detect().into_iter().next().unwrap_or_else(DeckInfo::fallback);
    let mut renderer = Renderer::new(&cfg.style);
    let size = 144u32;
    let gap = 18u32;
    let w = info.cols as u32 * (size + gap) + gap;
    let grid_h = info.rows as u32 * (size + gap) + gap;
    // Decks with a touch strip show it under the key grid, scaled to the grid's width,
    // then a row of dial circles, so bug reports show the whole deck.
    let strip_h = info.strip.map(|(sw, sh)| w * sh / sw).unwrap_or(0);
    let dial_r = if info.dials > 0 { size / 4 } else { 0 };
    let h = grid_h + if strip_h > 0 { gap + strip_h } else { 0 } + if dial_r > 0 { gap + dial_r * 2 } else { 0 };
    let mut canvas = image::RgbaImage::from_pixel(w, h, image::Rgba([10, 10, 12, 255]));
    for i in 0..info.key_count() {
        let face = renderer.render(keys.get(i), &cfg.style, size, false);
        let (r, c) = (i as u32 / info.cols as u32, i as u32 % info.cols as u32);
        image::imageops::overlay(&mut canvas, &face, (gap + c * (size + gap)) as i64, (gap + r * (size + gap)) as i64);
    }
    let mut y = grid_h + gap;
    if let Some((sw, sh)) = info.strip {
        let status = vec![DialStatus::default(); info.dials as usize];
        let strip = renderer.render_strip(&cfg, &status, (w, w * sh / sw));
        image::imageops::overlay(&mut canvas, &strip, 0, y as i64);
        y += strip_h;
    }
    if dial_r > 0 {
        for i in 0..info.dials {
            // The panel's background colour, sampled from a corner (never icon or text).
            let panel = renderer.render_strip_panel(cfg.dial(i), &cfg.style, DialStatus::default());
            let col = *panel.get_pixel(2, 2);
            let cx = gap + (i as u32) * (size + gap) + size / 2;
            draw_circle(&mut canvas, cx, y + dial_r, dial_r, col);
        }
    }
    canvas.save(out)?;
    Ok(())
}

/// A filled circle, good enough for the snapshot's dial knobs.
fn draw_circle(img: &mut image::RgbaImage, cx: u32, cy: u32, r: u32, c: image::Rgba<u8>) {
    let r2 = r as i32 * r as i32;
    for y in cy.saturating_sub(r)..(cy + r + 1).min(img.height()) {
        for x in cx.saturating_sub(r)..(cx + r + 1).min(img.width()) {
            let (dx, dy) = (x as i32 - cx as i32, y as i32 - cy as i32);
            if dx * dx + dy * dy <= r2 {
                img.put_pixel(x, y, c);
            }
        }
    }
}
