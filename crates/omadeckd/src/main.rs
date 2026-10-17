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
use omadeck_core::ipc::{self, Reply};
use omadeck_core::paths::FileStamp;
use omadeck_core::probe::{self, Values};
use omadeck_core::state::State;
use omadeck_core::{Action, Config, DeckInfo, Direction, HOME, Renderer, action, paths};

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
        Some(verb @ ("page" | "brightness" | "reload" | "state" | "press" | "dial")) => {
            let rest: Vec<String> = args.collect();
            let cmd = match parse_ctl(verb, &rest) {
                Ok(cmd) => cmd,
                Err(usage) => {
                    eprintln!("usage: omadeckd {usage}");
                    std::process::exit(2);
                }
            };
            std::process::exit(ctl(cmd));
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
                "usage: omadeckd [run | list | page next|prev|home|NAME | brightness N|+N|-N | press KEY [PAGE] | dial N left|right|press [STEPS] | reload | state | snapshot FILE.png [PAGE] | --version]\n\nRuns the omadeck Stream Deck daemon; the other verbs drive the running one."
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
        }
        if daemon.reload_at.is_some_and(|t| Instant::now() >= t) {
            daemon.reload_at = None;
            if let Err(e) = daemon.reload() {
                log::error!("{e:#}; keeping the previous config");
            }
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

const PAGE_USAGE: &str = "page next|prev|home|NAME";
const BRIGHTNESS_USAGE: &str = "brightness N|+N|-N";
const PRESS_USAGE: &str = "press KEY [PAGE]   (keys are numbered from 0)";
const DIAL_USAGE: &str = "dial N left|right|press [STEPS]";
const NOT_RUNNING: &str = "omadeckd is not running (systemctl --user start omadeckd)";

/// A control verb's arguments as the command to send. `Err` is the usage line.
fn parse_ctl(verb: &str, args: &[String]) -> Result<ipc::Command, &'static str> {
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    match verb {
        "page" => {
            let [page] = args[..] else { return Err(PAGE_USAGE) };
            Ok(match page {
                // Like a flick: folders are skipped.
                "next" => ipc::Command::Run { action: Action::PageStep { change: 1 } },
                "prev" | "previous" => ipc::Command::Run { action: Action::PageStep { change: -1 } },
                p if p.eq_ignore_ascii_case(HOME) => ipc::Command::Page { page: None },
                p => ipc::Command::Page { page: Some(p.to_string()) },
            })
        }
        "brightness" => {
            let [value] = args[..] else { return Err(BRIGHTNESS_USAGE) };
            let action = if value.starts_with(['+', '-']) {
                let change: i64 = value.parse().map_err(|_| BRIGHTNESS_USAGE)?;
                Action::Brightness { change: change.clamp(-100, 100) as i8, set: None }
            } else {
                let set: u64 = value.parse().map_err(|_| BRIGHTNESS_USAGE)?;
                Action::Brightness { change: 0, set: Some(set.min(100) as u8) }
            };
            Ok(ipc::Command::Run { action })
        }
        "press" => {
            let (key, page) = match args[..] {
                [key] => (key, None),
                [key, page] => (key, Some(page.to_string())),
                _ => return Err(PRESS_USAGE),
            };
            let index = key.parse().map_err(|_| PRESS_USAGE)?;
            Ok(ipc::Command::Press { index, page })
        }
        "dial" => {
            let (n, which, steps) = match args[..] {
                [n, which] => (n, which, "1"),
                [n, which, steps] => (n, which, steps),
                _ => return Err(DIAL_USAGE),
            };
            let index = n.parse().map_err(|_| DIAL_USAGE)?;
            let dir = match which {
                "left" => Direction::Left,
                "right" => Direction::Right,
                "press" => Direction::Press,
                _ => return Err(DIAL_USAGE),
            };
            let steps = steps.parse::<u8>().ok().filter(|s| *s > 0).ok_or(DIAL_USAGE)?;
            Ok(ipc::Command::Dial { index, dir, steps })
        }
        "reload" if args.is_empty() => Ok(ipc::Command::Reload),
        "reload" => Err("reload"),
        "state" if args.is_empty() => Ok(ipc::Command::State),
        _ => Err("state"),
    }
}

/// Send `cmd`, print the answer, and fall back to running press/dial here. The exit code.
fn ctl(cmd: ipc::Command) -> i32 {
    match ipc::request(&cmd) {
        Ok(Some(Reply::Ok { message })) => {
            if !message.is_empty() {
                println!("{message}");
            }
            0
        }
        Ok(Some(r @ Reply::State { .. })) => {
            print!("{}", r.to_line());
            0
        }
        Ok(Some(Reply::Error { message })) => {
            eprintln!("{message}");
            1
        }
        // No daemon, or no deck on it: press and dial run here, as they always did.
        Ok(Some(Reply::NoDeck) | None) => match cmd {
            ipc::Command::Press { index, page } => press_locally(index, page.as_deref()),
            ipc::Command::Dial { index, dir, steps } => dial_locally(index, dir, steps),
            _ => {
                eprintln!("{NOT_RUNNING}");
                1
            }
        },
        // "daemon busy" shows here: press and dial don't fall back, the daemon may have run it.
        Err(e) => {
            eprintln!("{e:#}");
            1
        }
    }
}

/// Press key `index` here, with no daemon: on Home, or on `page` ("home" in any case is
/// Home).
fn press_locally(index: u8, page: Option<&str>) -> i32 {
    let page = page.filter(|p| !p.eq_ignore_ascii_case(HOME));
    let cfg = load_config();
    let Some(keys) = cfg.page(page) else {
        eprintln!("no page named {}", page.unwrap_or_default());
        return 1;
    };
    let act = keys.get(index).map(|k| k.action.clone()).unwrap_or_default();
    println!("key {index}: {}", act.summary());
    if let Err(e) = action::run(&act) {
        eprintln!("{e:#}");
        return 1;
    }
    // Give the detached child a moment to start before we exit.
    std::thread::sleep(Duration::from_millis(300));
    0
}

/// Turn or press dial `index` here, with no daemon.
fn dial_locally(index: u8, dir: Direction, steps: u8) -> i32 {
    let cfg = load_config();
    let act = cfg.dial(index).map(|d| d.action(dir).clone()).unwrap_or_default();
    let which = match dir {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Press => "press",
    };
    let steps = if dir == Direction::Press { 1 } else { steps };
    let times = if steps > 1 { format!(" x{steps}") } else { String::new() };
    println!("dial {index} {which}{times}: {}", act.summary());
    // Hyprland steps > 1 run on run_steps' helper thread: the sleep below lets them go.
    if let Err(e) = action::run_steps(&act, steps) {
        eprintln!("{e:#}");
        return 1;
    }
    std::thread::sleep(Duration::from_millis(300));
    0
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
        // Off faces: a snapshot can't run the state commands (PR 8's Later).
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

#[cfg(test)]
mod tests {
    use super::*;
    use omadeck_core::{Action, Direction};

    fn args(s: &str) -> Vec<String> {
        s.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn parse_ctl_page() {
        let run = |action| Ok(ipc::Command::Run { action });
        assert_eq!(parse_ctl("page", &args("next")), run(Action::PageStep { change: 1 }));
        assert_eq!(parse_ctl("page", &args("prev")), run(Action::PageStep { change: -1 }));
        assert_eq!(parse_ctl("page", &args("previous")), run(Action::PageStep { change: -1 }));
        assert_eq!(parse_ctl("page", &args("home")), Ok(ipc::Command::Page { page: None }));
        assert_eq!(parse_ctl("page", &args("HOME")), Ok(ipc::Command::Page { page: None }));
        assert_eq!(parse_ctl("page", &args("Media")), Ok(ipc::Command::Page { page: Some("Media".into()) }));
        assert!(parse_ctl("page", &[]).is_err());
        assert!(parse_ctl("page", &args("Media Apps")).is_err());
    }

    #[test]
    fn parse_ctl_brightness() {
        let run = |action| Ok(ipc::Command::Run { action });
        assert_eq!(parse_ctl("brightness", &args("30")), run(Action::Brightness { change: 0, set: Some(30) }));
        assert_eq!(parse_ctl("brightness", &args("+10")), run(Action::Brightness { change: 10, set: None }));
        assert_eq!(parse_ctl("brightness", &args("-10")), run(Action::Brightness { change: -10, set: None }));
        assert_eq!(parse_ctl("brightness", &args("-500")), run(Action::Brightness { change: -100, set: None }));
        assert_eq!(parse_ctl("brightness", &args("250")), run(Action::Brightness { change: 0, set: Some(100) }));
        assert!(parse_ctl("brightness", &args("abc")).is_err());
        assert!(parse_ctl("brightness", &[]).is_err());
    }

    #[test]
    fn parse_ctl_press_and_dial() {
        assert_eq!(parse_ctl("press", &args("3")), Ok(ipc::Command::Press { index: 3, page: None }));
        assert_eq!(
            parse_ctl("press", &args("3 Media")),
            Ok(ipc::Command::Press { index: 3, page: Some("Media".into()) })
        );
        assert!(parse_ctl("press", &args("x")).is_err());
        assert_eq!(
            parse_ctl("dial", &args("1 left")),
            Ok(ipc::Command::Dial { index: 1, dir: Direction::Left, steps: 1 })
        );
        assert_eq!(
            parse_ctl("dial", &args("1 right 4")),
            Ok(ipc::Command::Dial { index: 1, dir: Direction::Right, steps: 4 })
        );
        assert_eq!(
            parse_ctl("dial", &args("1 press")),
            Ok(ipc::Command::Dial { index: 1, dir: Direction::Press, steps: 1 })
        );
        assert!(parse_ctl("dial", &args("1 up")).is_err());
        assert!(parse_ctl("dial", &args("1 left 0")).is_err());
    }
}
