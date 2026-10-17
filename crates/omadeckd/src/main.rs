//! omadeckd — owns the Stream Deck: paints key faces, runs actions on press, follows
//! config and Omarchy theme changes live, and survives unplug/replug.

mod daemon;
mod deck;
mod hub;
mod hypr;
mod hypr_worker;
mod nav;
mod saver;
mod watch;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
// Only the trait's methods: `Watcher` here is the daemon's own (watch.rs).
use notify::{RecursiveMode, Watcher as _};
use omadeck_core::device;
use omadeck_core::paths::FileStamp;
use omadeck_core::probe::{self, Values};
use omadeck_core::state::State;
use omadeck_core::{Config, DeckInfo, Renderer, action, paths};

use daemon::Daemon;
use hub::Hub;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
/// How often to double-check the config by stat, for changes inotify can't see.
const STAMP_INTERVAL: Duration = Duration::from_secs(1);

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
    let mut fs_watch = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res
            && !matches!(ev.kind, notify::EventKind::Access(_))
            && ev.paths.iter().any(|p| relevant(p))
        {
            let _ = tx.send(());
        }
    })?;
    fs_watch.watch(&paths::config_dir(), RecursiveMode::NonRecursive)?;
    let mut dir_stamp = FileStamp::of(&paths::config_dir());
    let mut cfg_stamp = FileStamp::of(&paths::config_file());
    let mut last_stamp_check = Instant::now();
    let omarchy = paths::omarchy_current_dir();
    if omarchy.is_dir() {
        fs_watch.watch(&omarchy, RecursiveMode::NonRecursive)?;
    }
    if paths::icons_dir().is_dir() {
        let _ = fs_watch.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
    }

    let mut hid = device::new_hidapi().context("initialising hidapi")?;
    let (cfg, cfg_loaded) = match Config::load() {
        Ok(c) => (c, true),
        Err(e) => {
            log::error!("{e:#}; using an empty config");
            (Config::default(), false)
        }
    };
    let mut daemon = Daemon::new(cfg, cfg_loaded, hub, watch::start(), saver::start_saver());
    log::info!("theme: {}", daemon.renderer.theme.name);
    let mut last_scan: Option<Instant> = None;

    log::info!("omadeckd {} ready; config {}", env!("CARGO_PKG_VERSION"), paths::config_file().display());

    while !term.load(Ordering::Relaxed) {
        // Coalesce bursts of filesystem events into one reload.
        while rx.try_recv().is_ok() {
            daemon.reload_at = Some(Instant::now() + RELOAD_DEBOUNCE);
        }
        // inotify misses a symlinked config's target, and its watch dies with a config
        // directory that's deleted and recreated (restoring a backup), so also compare stamps.
        if last_stamp_check.elapsed() >= STAMP_INTERVAL {
            last_stamp_check = Instant::now();
            let d = FileStamp::of(&paths::config_dir());
            // The directory's mtime moves on every save; only its identity matters here.
            if !FileStamp::same_file(d, dir_stamp) {
                dir_stamp = d;
                let _ = fs_watch.unwatch(&paths::config_dir());
                if d.is_some() {
                    log::info!("config directory was replaced; watching the new one");
                    let _ = fs_watch.watch(&paths::config_dir(), RecursiveMode::NonRecursive);
                    if paths::icons_dir().is_dir() {
                        let _ = fs_watch.unwatch(&paths::icons_dir());
                        let _ = fs_watch.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
                    }
                }
            }
            let c = FileStamp::of(&paths::config_file());
            if c != cfg_stamp {
                cfg_stamp = c;
                daemon.reload_at.get_or_insert(Instant::now());
            }
            // `omarchy font set` only rewrites fontconfig's user config.
            if daemon.renderer.font_changed() {
                daemon.reload_at.get_or_insert(Instant::now());
            }
        }
        if daemon.reload_at.is_some_and(|t| Instant::now() >= t) {
            daemon.reload_at = None;
            daemon.reload();
        }
        daemon.handle_commands();
        daemon.apply_values();
        daemon.apply_hypr();
        daemon.poll_jobs();
        daemon.poll_hypr();

        if daemon.deck.is_none() {
            // With no deck, `flush` only runs `publish_keys`: a job's progress and a
            // reload's new states still reach the configurators.
            daemon.flush();
            if last_scan.is_none_or(|t| t.elapsed() >= SCAN_INTERVAL) {
                last_scan = Some(Instant::now());
                // `Disconnected` goes out once, from `detach` (and the hub's replay
                // starts with it), not on every scan.
                if let Some(d) = deck::open(&mut hid) {
                    daemon.attach(d);
                }
            } else {
                std::thread::sleep(daemon::wait_for(Instant::now(), daemon.next_deadline()));
            }
            continue;
        }

        match daemon.pass() {
            Ok(()) => {}
            Err(_) if term.load(Ordering::Relaxed) => break,
            Err(e) => {
                daemon.detach(&e);
                last_scan = None;
            }
        }
    }

    daemon.shutdown();
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
        // Off faces: a snapshot can't run the state commands.
        let face =
            renderer.render(keys.get(i), &cfg.style, size, &probe::key_status(keys.get(i), &Values::default(), false));
        let (r, c) = (i as u32 / info.cols as u32, i as u32 % info.cols as u32);
        image::imageops::overlay(&mut canvas, &face, (gap + c * (size + gap)) as i64, (gap + r * (size + gap)) as i64);
    }
    let mut y = grid_h + gap;
    if let Some((sw, sh)) = info.strip {
        // The brightness the daemon would start at (the state file's, unless the
        // config's value was edited since).
        // Brightness dials show the brightness the deck runs at; volume dials show a dash,
        // as their level is live and only the running daemon reads it.
        let values = Values { brightness: State::load().start_brightness(cfg.brightness), ..Values::default() };
        let status: Vec<_> = (0..info.dials).map(|i| probe::dial_status(cfg.dial(i), &values, false)).collect();
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
