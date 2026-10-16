//! omadeckd — owns the Stream Deck: paints key faces, runs actions on press, follows
//! config and Omarchy theme changes live, and survives unplug/replug.

mod hub;
mod nav;

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use omadeck_core::config::apply_brightness;
use omadeck_core::device::{self, DeviceStateReader, DeviceStateUpdate, HidApi, ImageRect, Kind, StreamDeck};
use omadeck_core::ipc::{Command, Event};
use omadeck_core::paths::FileStamp;
use omadeck_core::state::State;
use omadeck_core::{
    Action, AudioDevice, Config, DeckInfo, DialStatus, Level, LevelSource, PANEL_W, Renderer, action, audio, paths,
    tap_to_dial,
};

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
/// Minimum horizontal distance, in strip pixels, for a swipe to count as a flick.
const FLICK_MIN_PX: i32 = 50;
/// How often the audio worker re-reads a watched device's level.
const AUDIO_POLL: Duration = Duration::from_millis(1000);
/// Wait this long after our own volume change before re-reading, so the command finishes.
const AUDIO_REFRESH_DELAY: Duration = Duration::from_millis(150);

/// At most this many more HID reports are taken in one pass after the first, so a fast
/// spin is handled together and its panels are painted once.
const MAX_DRAIN: usize = 32;

/// A message the main loop sends the audio worker.
enum AudioCmd {
    /// Which devices some dial shows; polled every second while non-empty.
    Watch(Vec<AudioDevice>),
    /// We just changed this device; re-read it soon and send the result even if it's
    /// unchanged, so an optimistic guess that was wrong gets corrected.
    Refresh(AudioDevice),
}

/// The audio worker: reads the watched devices' levels off the deck loop and sends
/// `(device, level)` when a value changes, so a fast spin never blocks the deck.
struct AudioWorker {
    cmd: mpsc::Sender<AudioCmd>,
    levels: mpsc::Receiver<(AudioDevice, Level)>,
}

fn start_audio_worker() -> AudioWorker {
    let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCmd>();
    let (lvl_tx, lvl_rx) = mpsc::channel::<(AudioDevice, Level)>();
    std::thread::Builder::new()
        .name("audio".into())
        .spawn(move || {
            let mut watched: Vec<AudioDevice> = Vec::new();
            let mut last: HashMap<AudioDevice, audio::Level> = HashMap::new();
            let mut next_poll = Instant::now();
            loop {
                loop {
                    match cmd_rx.try_recv() {
                        Ok(AudioCmd::Watch(devs)) => {
                            watched = devs;
                            last.clear();
                            next_poll = Instant::now();
                        }
                        Ok(AudioCmd::Refresh(dev)) => {
                            // Drop the last known value so the next read is sent even
                            // when it equals what we already knew.
                            last.remove(&dev);
                            next_poll = Instant::now() + AUDIO_REFRESH_DELAY;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let now = Instant::now();
                if now >= next_poll && !watched.is_empty() {
                    for dev in &watched {
                        if let Some(lv) = audio::volume(*dev)
                            && last.get(dev) != Some(&lv)
                        {
                            last.insert(*dev, lv);
                            if lvl_tx.send((*dev, lv.to_panel())).is_err() {
                                return;
                            }
                        }
                    }
                    next_poll = now + AUDIO_POLL;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        })
        .expect("spawn audio thread");
    AudioWorker { cmd: cmd_tx, levels: lvl_rx }
}

/// The volume devices any dial shows, so the audio worker only reads what's on screen.
fn watched_devices(cfg: &Config, n_dials: usize) -> Vec<AudioDevice> {
    let mut devs = Vec::new();
    for dev in [AudioDevice::Output, AudioDevice::Input] {
        if (0..n_dials).any(
            |i| matches!(cfg.dial(i as u8).and_then(|d| d.level_source()), Some(LevelSource::Volume(d)) if d == dev),
        ) {
            devs.push(dev);
        }
    }
    devs
}

struct Deck {
    dev: Arc<StreamDeck>,
    reader: Arc<DeviceStateReader>,
    kind: Kind,
    info: DeckInfo,
    /// Whether each dial is physically pressed down.
    dial_pressed: Vec<bool>,
    /// Until when each dial's panel stays highlighted after a tap on the strip.
    flash_until: Vec<Option<Instant>>,
    /// The value each dial shows on its panel, if any (a volume device or the brightness).
    dial_levels: Vec<Option<LevelSource>>,
    /// The last known level per volume device, for the strip panels.
    device_levels: HashMap<AudioDevice, Level>,
    /// Panels to repaint at the end of this pass (`flush_dirty`), so a fast spin can't
    /// queue a write per detent on the loop.
    dirty_panels: Vec<bool>,
    /// The panel brightness, 0–100: the config value at start, changed by brightness actions.
    brightness: u8,
}

/// The live state of dial `i` for the strip renderer.
fn dial_status(d: &Deck, i: u8, now: Instant) -> DialStatus {
    let n = i as usize;
    let flashed = d.flash_until.get(n).and_then(|t| *t).is_some_and(|t| now.duration_since(t) < DIAL_FLASH);
    let level = d.dial_levels.get(n).and_then(|src| match src {
        Some(LevelSource::Volume(dev)) => d.device_levels.get(dev).cloned(),
        // The brightness is in-process state: no worker, no polling.
        Some(LevelSource::Brightness) => {
            Some(Level { value: d.brightness as f32 / 100.0, text: format!("{}%", d.brightness), muted: false })
        }
        None => None,
    });
    DialStatus { pressed: d.dial_pressed.get(n).copied().unwrap_or(false) || flashed, level }
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
    let audio = start_audio_worker();
    let (brightness_tx, brightness_rx) = mpsc::channel::<u8>();
    let saver = std::thread::Builder::new()
        .name("brightness".into())
        .spawn(move || brightness_saver(brightness_rx, save_brightness))
        .expect("spawning the brightness saver");
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
            let mut hand_edit = None;
            match Config::load() {
                Ok(new_cfg) => {
                    hand_edit = hand_edited_brightness(&cfg, &new_cfg);
                    cfg = new_cfg;
                }
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
            if let Some(d) = &mut deck {
                d.dial_levels = (0..d.info.dials).map(|i| cfg.dial(i).and_then(|dc| dc.level_source())).collect();
                // Keep the levels of devices that are still watched, so the repaint below
                // draws their bars as they are instead of blanking them for a moment (the
                // worker re-polls and refreshes them). Drop only what is no longer shown.
                let watched = watched_devices(&cfg, d.info.dials as usize);
                d.device_levels.retain(|dev, _| watched.contains(dev));
                let _ = audio.cmd.send(AudioCmd::Watch(watched));
            }
            // A hand edit of `brightness` in config.toml wins over the state file, like
            // any other hot-reloaded field (it is also saved there).
            if let Some(v) = hand_edit {
                set_brightness(deck.as_mut(), &brightness_tx, v);
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
        let mut bright = None;
        for cmd in hub.commands() {
            match cmd {
                Command::ShowPage { page } => show = Some(page),
                Command::SetBrightness { value } => bright = Some(value),
            }
        }
        if let Some(v) = bright {
            set_brightness(deck.as_mut(), &brightness_tx, v);
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
                if let Some(d) = &deck {
                    repaint_at = Some(Instant::now() + POST_CONNECT_REPAINT);
                    let _ = audio.cmd.send(AudioCmd::Watch(watched_devices(&cfg, d.info.dials as usize)));
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
            mark_dirty(d, i);
        }

        // Volume levels from the audio worker: update the device and repaint its panels.
        while let Ok((dev, level)) = audio.levels.try_recv() {
            d.device_levels.insert(dev, level.clone());
            for (i, src) in d.dial_levels.iter().enumerate().take(d.info.dials as usize) {
                if *src == Some(LevelSource::Volume(dev)) {
                    d.dirty_panels[i] = true;
                }
            }
        }

        match d.reader.read(Some(READ_TIMEOUT)) {
            Ok(mut updates) => {
                // A read returns one HID report. Take the reports already queued behind
                // it too (a zero timeout doesn't wait), so a fast spin is one pass.
                if !updates.is_empty() {
                    for _ in 0..MAX_DRAIN {
                        match d.reader.read(Some(Duration::ZERO)) {
                            Ok(more) if !more.is_empty() => updates.extend(more),
                            _ => break,
                        }
                    }
                }
                for u in updates {
                    match u {
                        DeviceStateUpdate::ButtonDown(i) => {
                            paint(d, &cfg, nav.current(), &mut renderer, i, true);
                            hub.publish(Event::KeyDown { index: i, page: nav.current().map(str::to_string) });
                            let Some(k) = cfg.key(nav.current(), i) else { continue };
                            log::info!("key {i}: {}", k.action.summary());
                            if let Err(e) =
                                run_deck_action(d, &cfg, &mut nav, &mut renderer, &hub, &brightness_tx, &k.action, 1)
                            {
                                log::warn!("key {i}: {e:#}");
                                hub.publish(Event::ActionFailed { index: i, message: format!("{e:#}") });
                            }
                            after_volume(d, &audio, &k.action, 1);
                        }
                        DeviceStateUpdate::ButtonUp(i) => {
                            paint(d, &cfg, nav.current(), &mut renderer, i, false);
                            hub.publish(Event::KeyUp { index: i });
                        }
                        DeviceStateUpdate::EncoderTwist(i, delta) => {
                            log::debug!("dial {i} turned {delta}");
                            hub.publish(Event::DialTurn { index: i, delta });
                            if let Some(dial) = cfg.dial(i) {
                                let (act, dir) =
                                    if delta >= 0 { (&dial.turn_right, "right") } else { (&dial.turn_left, "left") };
                                let steps = delta.unsigned_abs().min(8);
                                if !act.is_none() {
                                    log::info!("dial {i} turned {dir} x{steps}: {}", act.summary());
                                    if let Err(e) = run_deck_action(
                                        d,
                                        &cfg,
                                        &mut nav,
                                        &mut renderer,
                                        &hub,
                                        &brightness_tx,
                                        act,
                                        steps,
                                    ) {
                                        log::warn!("dial {i}: {e:#}");
                                        hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                    }
                                    after_volume(d, &audio, act, steps);
                                }
                            }
                        }
                        DeviceStateUpdate::EncoderDown(i) => {
                            log::info!("dial {i} pressed");
                            hub.publish(Event::DialDown { index: i });
                            if let Some(p) = d.dial_pressed.get_mut(i as usize) {
                                *p = true;
                            }
                            mark_dirty(d, i);
                            if let Some(dc) = cfg.dial(i)
                                && !dc.press.is_none()
                            {
                                log::info!("dial {i}: {}", dc.press.summary());
                                if let Err(e) = run_deck_action(
                                    d,
                                    &cfg,
                                    &mut nav,
                                    &mut renderer,
                                    &hub,
                                    &brightness_tx,
                                    &dc.press,
                                    1,
                                ) {
                                    log::warn!("dial {i}: {e:#}");
                                    hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                }
                                after_volume(d, &audio, &dc.press, 1);
                            }
                        }
                        DeviceStateUpdate::EncoderUp(i) => {
                            log::info!("dial {i} released");
                            hub.publish(Event::DialUp { index: i });
                            if let Some(p) = d.dial_pressed.get_mut(i as usize) {
                                *p = false;
                            }
                            mark_dirty(d, i);
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
                                mark_dirty(d, i as u8);
                                if let Some(dc) = cfg.dial(i as u8)
                                    && !dc.press.is_none()
                                {
                                    log::info!("dial {i}: {}", dc.press.summary());
                                    if let Err(e) = run_deck_action(
                                        d,
                                        &cfg,
                                        &mut nav,
                                        &mut renderer,
                                        &hub,
                                        &brightness_tx,
                                        &dc.press,
                                        1,
                                    ) {
                                        log::warn!("dial {i}: {e:#}");
                                        hub.publish(Event::DialActionFailed {
                                            index: i as u8,
                                            message: format!("{e:#}"),
                                        });
                                    }
                                    after_volume(d, &audio, &dc.press, 1);
                                }
                            }
                        }
                        DeviceStateUpdate::TouchScreenLongPress(x, y) => {
                            // Published for now; a `long_press` action may use it later.
                            log::info!("strip long press at ({x}, {y})");
                            hub.publish(Event::StripLongPress { x, y });
                        }
                        DeviceStateUpdate::TouchScreenSwipe(from, to) => {
                            log::info!("strip swipe {from:?} to {to:?}");
                            hub.publish(Event::StripSwipe { from, to });
                            // As in the Elgato app: a horizontal flick pages through, left is
                            // next, right is previous. Short or mostly vertical swipes don't.
                            let (dx, dy) = (to.0 as i32 - from.0 as i32, to.1 as i32 - from.1 as i32);
                            if dx.abs() >= FLICK_MIN_PX && dx.abs() > dy.abs() {
                                let act = Action::PageStep { change: if dx < 0 { 1 } else { -1 } };
                                if let Err(e) =
                                    run_deck_action(d, &cfg, &mut nav, &mut renderer, &hub, &brightness_tx, &act, 1)
                                {
                                    log::warn!("strip flick: {e:#}");
                                }
                            }
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
                let _ = audio.cmd.send(AudioCmd::Watch(Vec::new()));
                hub.publish(Event::Disconnected);
            }
        }

        // Everything this pass dirtied (input, levels, flashes, a GUI brightness change),
        // painted once.
        if let Some(d) = &mut deck {
            flush_dirty(d, &cfg, &mut renderer);
        }
    }

    log::info!("shutting down");
    if let Some(d) = deck {
        // Hand the deck back to its idle logo screen.
        let _ = d.dev.reset();
    }
    hub.shutdown();
    // Let the saver write a brightness that is still settling.
    drop(brightness_tx);
    let _ = saver.join();
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
    let deck = Deck {
        dev,
        reader,
        kind,
        info,
        dial_pressed: vec![false; n_dials],
        flash_until: vec![None; n_dials],
        dial_levels: (0..n_dials).map(|i| cfg.dial(i as u8).and_then(|d| d.level_source())).collect(),
        device_levels: HashMap::new(),
        dirty_panels: vec![false; n_dials],
        // The state file wins over the config's start value once the deck has run.
        brightness: State::load().brightness.unwrap_or(cfg.brightness),
    };
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

/// Run an action from the deck: page moves and brightness are handled by the daemon,
/// everything else goes to `action::run`. Returns whether the deck moved.
fn dispatch(
    d: &mut Deck,
    cfg: &Config,
    nav: &mut Nav,
    brightness: &mpsc::Sender<u8>,
    act: &Action,
    steps: u8,
) -> Result<bool> {
    match act {
        Action::Page { page } if cfg.has_page(page) => Ok(nav.go(Some(page))),
        Action::Page { page } => Err(anyhow::anyhow!("there's no page named “{page}”")),
        Action::Back => Ok(nav.back()),
        Action::PageStep { change } => {
            // From inside a folder, step from the folder's parent, leaving the folder.
            let in_folder = cfg.pages.iter().any(|p| p.folder && nav.current().is_some_and(|c| p.name == c));
            let origin = if in_folder { nav.parent().flatten() } else { nav.current().map(str::to_string) };
            let by = i32::from(*change) * steps as i32;
            match cfg.step_page(origin.as_deref(), by) {
                Some(page) => Ok(nav.switch(page.as_deref())),
                None => Ok(false),
            }
        }
        Action::Brightness { change, set } => {
            // A relative turn changes the brightness per detent; `set` applies once.
            let change = if set.is_none() { i32::from(*change) * steps as i32 } else { 0 };
            do_brightness(d, brightness, *set, change);
            Ok(false)
        }
        a => action::run_steps(a, steps).map(|_| false),
    }
}

/// The new start brightness when `new` changes the config's `brightness` (a hand edit,
/// or the configurator with no daemon), else `None`.
fn hand_edited_brightness(old: &Config, new: &Config) -> Option<u8> {
    (new.brightness != old.brightness).then_some(new.brightness)
}

/// Set the brightness from outside the deck's own input (the configurator's slider, a
/// config edit). With no deck it is only saved, and the next connect starts from it.
fn set_brightness(deck: Option<&mut Deck>, saver: &mpsc::Sender<u8>, value: u8) {
    match deck {
        Some(d) => do_brightness(d, saver, Some(value), 0),
        None => {
            let _ = saver.send(value.min(100));
        }
    }
}

/// Apply a brightness action: set the panel, remember it in the state file (so it
/// survives a restart, as in the Elgato app), and repaint the brightness dials' panels.
fn do_brightness(d: &mut Deck, brightness: &mpsc::Sender<u8>, set: Option<u8>, change: i32) {
    let next = apply_brightness(d.brightness, set, change);
    if next == d.brightness {
        return;
    }
    log::info!("brightness {next}%");
    d.brightness = next;
    if let Err(e) = d.dev.set_brightness(next) {
        log::warn!("set_brightness: {e}");
    }
    let _ = brightness.send(next);
    for (i, src) in d.dial_levels.iter().enumerate().take(d.info.dials as usize) {
        if *src == Some(LevelSource::Brightness) {
            d.dirty_panels[i] = true;
        }
    }
}

/// After a volume action has run: move the bar optimistically, so it doesn't wait on
/// the read-back the way an external value must (the brightness bar is instant for the
/// same reason). The value is the last known level plus the change, clamped 0–100; the
/// audio worker's refresh re-reads the true value a moment later and corrects it, which
/// for a relative change is the same number.
fn after_volume(d: &mut Deck, audio: &AudioWorker, act: &Action, steps: u8) {
    let Action::Volume { device, change, mute } = act else { return };
    let Some(cur) = d.device_levels.get(device).cloned() else {
        // Nothing read yet (just connected): a guess from 0 would show a wrong bar; the
        // refresh brings the real value.
        let _ = audio.cmd.send(AudioCmd::Refresh(*device));
        return;
    };
    let pct = (cur.value * 100.0).round() as i32;
    let next = if *mute {
        // A press toggles mute; guess the new state from the current one.
        let muted = !cur.muted;
        Level { value: cur.value, text: if muted { "Muted".into() } else { format!("{}%", pct) }, muted }
    } else {
        let p = (pct + *change as i32 * steps as i32).clamp(0, 100);
        Level { value: p as f32 / 100.0, text: format!("{p}%"), muted: false }
    };
    d.device_levels.insert(*device, next);
    for (i, src) in d.dial_levels.iter().enumerate().take(d.info.dials as usize) {
        if *src == Some(LevelSource::Volume(*device)) {
            d.dirty_panels[i] = true;
        }
    }
    let _ = audio.cmd.send(AudioCmd::Refresh(*device));
}

/// Write the brightness to the state file, debounced: one write per burst of turns,
/// off the deck loop, so a hard spin doesn't serialize the file on every detent. The
/// user's config.toml is never touched.
fn brightness_saver(rx: mpsc::Receiver<u8>, save: impl Fn(u8)) {
    let settle = Duration::from_millis(200);
    let mut pending = rx.recv().ok();
    while let Some(first) = pending {
        // A hard spin sends several in a row; take the last of the burst.
        let mut v = first;
        while let Ok(next) = rx.recv_timeout(settle) {
            v = next;
        }
        save(v);
        // Blocks for the next turn; ends when the daemon drops the sender.
        pending = rx.recv().ok();
    }
}

fn save_brightness(v: u8) {
    if let Err(e) = (State { brightness: Some(v) }).save() {
        log::warn!("saving brightness: {e:#}");
    }
}

/// Run `act` from the deck and repaint if it moved the page.
#[allow(clippy::too_many_arguments)]
fn run_deck_action(
    d: &mut Deck,
    cfg: &Config,
    nav: &mut Nav,
    renderer: &mut Renderer,
    hub: &Hub,
    brightness: &mpsc::Sender<u8>,
    act: &Action,
    steps: u8,
) -> Result<bool> {
    let moved = dispatch(d, cfg, nav, brightness, act, steps)?;
    if moved {
        show_page(Some(d), cfg, nav, renderer, hub);
    }
    Ok(moved)
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
    d.dev.set_brightness(d.brightness)?;
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
    let panel = renderer.render_strip_panel(cfg, i, d.info.dials as u32, dial_status(d, i, Instant::now()));
    let rect = ImageRect::from_image(image::DynamicImage::ImageRgba8(panel))?;
    d.dev.write_lcd(i as u16 * PANEL_W as u16, 0, &rect)?;
    Ok(())
}

/// Redraw one 200×100 dial panel. Volume and brightness changes mark the panel dirty
/// instead of calling this directly, so a fast spin coalesces to one write per loop pass.
/// Mark dial `i`'s panel for the end-of-pass paint.
fn mark_dirty(d: &mut Deck, i: u8) {
    if let Some(f) = d.dirty_panels.get_mut(i as usize) {
        *f = true;
    }
}

/// Paint every dirty dial panel and flush once. Runs at the end of every loop pass, the
/// only place panels are painted outside `paint_all`, so a fast spin, a tap flash and a
/// level change in the same pass cost one write per panel.
fn flush_dirty(d: &mut Deck, cfg: &Config, renderer: &mut Renderer) {
    if !d.dirty_panels.contains(&true) {
        return;
    }
    if d.info.strip.is_some() {
        for i in 0..d.info.dials {
            if d.dirty_panels.get(i as usize).copied().unwrap_or(false)
                && let Err(e) = write_panel(d, cfg, renderer, i)
            {
                log::warn!("paint dial {i}: {e}");
            }
        }
        if let Err(e) = d.dev.flush() {
            log::warn!("flush: {e}");
        }
    }
    d.dirty_panels.fill(false);
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
    // The deck runs on the state file's brightness once it has run, as `connect` does.
    let brightness = State::load().brightness.unwrap_or(cfg.brightness);
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
        // Brightness dials show the live start value; volume dials show a dash, as their
        // level is read by the running daemon.
        let status: Vec<DialStatus> = (0..info.dials as usize)
            .map(|i| {
                let level = matches!(cfg.dial(i as u8).and_then(|d| d.level_source()), Some(LevelSource::Brightness))
                    .then_some(Level {
                        value: brightness as f32 / 100.0,
                        text: format!("{}%", brightness),
                        muted: false,
                    });
                DialStatus { pressed: false, level }
            })
            .collect();
        let strip = renderer.render_strip(&cfg, &status, (w, w * sh / sw));
        image::imageops::overlay(&mut canvas, &strip, 0, y as i64);
        y += strip_h;
    }
    if dial_r > 0 {
        for i in 0..info.dials {
            // The panel's average colour (the backdrop's tone), so a background image
            // tints the knob instead of a corner pixel deciding.
            let tone = renderer.panel_backdrop(&cfg, i, info.dials as u32).tone;
            let col = image::Rgba([tone.0, tone.1, tone.2, 255]);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn saver_writes_the_last_value_when_dropped() {
        let saved = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (tx, rx) = mpsc::channel::<u8>();
        let out = saved.clone();
        let saver = std::thread::spawn(move || brightness_saver(rx, move |v| out.lock().unwrap().push(v)));
        for v in [30, 40, 50] {
            tx.send(v).unwrap();
        }
        // Shutdown: drop the sender and join, as `run` does; the burst is saved once.
        drop(tx);
        saver.join().unwrap();
        assert_eq!(*saved.lock().unwrap(), vec![50]);
    }

    #[test]
    fn hand_edit_is_a_changed_brightness() {
        let old = Config { brightness: 70, ..Default::default() };
        let new = Config { brightness: 30, ..Default::default() };
        assert_eq!(hand_edited_brightness(&old, &new), Some(30));
        assert_eq!(hand_edited_brightness(&old, &old), None);
    }
}
