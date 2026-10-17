//! Hyprland's event socket, on its own thread. Live keys use the active workspace; app
//! volume and app pages use the focused window.
//!
//! Events come from `.socket2.sock` (`EVENT>>DATA` lines); the window and workspace
//! themselves from `.socket.sock` with the `j/…` requests `hyprctl -j` sends, so there is
//! no process per focus change. This only reads: Hyprland dispatches go through the
//! `hypr` worker (`hypr_worker.rs`).

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The focused window, as `j/activewindow` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWindow {
    /// Without a `0x` prefix, lowercase, so event and JSON forms compare equal.
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprEvent {
    /// The focused workspace changed, moved monitor, or was renamed.
    Workspace {
        #[allow(dead_code)]
        id: i64,
        name: String,
    },
    /// The focused window changed or was retitled; `None` when nothing has focus.
    ActiveWindow(Option<ActiveWindow>),
}

/// Reconnect backoff cap.
const MAX_BACKOFF: Duration = Duration::from_secs(30);
/// Read and write timeout of one request on `.socket.sock`.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(1);

/// Starts the `hypr-events` thread. `None` outside Hyprland (no thread).
pub fn start() -> Option<mpsc::Receiver<HyprEvent>> {
    let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from) else {
        log::debug!("no XDG_RUNTIME_DIR: not listening to Hyprland");
        return None;
    };
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() && !runtime.join("hypr").is_dir() {
        log::debug!("not running under Hyprland: not listening to its events");
        return None;
    }
    let (tx, rx) = mpsc::channel();
    match thread::Builder::new().name("hypr-events".into()).spawn(move || run(tx, runtime)) {
        Ok(_) => Some(rx),
        Err(e) => {
            log::warn!("could not start the Hyprland event thread: {e}");
            None
        }
    }
}

/// What one event line asks for. Pure.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Workspace {
        id: i64,
        name: String,
    },
    /// focusedmon, renameworkspace
    QueryWorkspace,
    /// activewindowv2 with an address
    QueryWindow,
    /// activewindowv2 with no address
    NoWindow,
    Title {
        address: String,
        title: String,
    },
    /// configreloaded
    Reseed,
}

/// What the thread last sent, so it sends only changes.
#[derive(Debug, Default)]
struct Seen {
    workspace: Option<(i64, String)>,
    window: Option<ActiveWindow>,
}

fn parse(line: &str) -> Option<Parsed> {
    let (event, data) = line.split_once(">>")?;
    let mut parts = data.splitn(2, ',');
    let first = parts.next().unwrap_or("");
    let rest = parts.next();
    match event {
        "workspacev2" => Some(Parsed::Workspace { id: first.trim().parse().ok()?, name: rest?.to_string() }),
        "focusedmon" | "renameworkspace" => Some(Parsed::QueryWorkspace),
        "activewindowv2" => {
            if first.trim().is_empty() {
                Some(Parsed::NoWindow)
            } else {
                Some(Parsed::QueryWindow)
            }
        }
        "windowtitlev2" => Some(Parsed::Title { address: normalise_address(first), title: rest?.to_string() }),
        "configreloaded" => Some(Parsed::Reseed),
        _ => None,
    }
}

/// Turns a parsed line into the events to send, running `query` (the request socket)
/// when the line needs it. Pure but for `query`, so tests pass a closure.
fn handle(p: Parsed, seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent> {
    match p {
        Parsed::Workspace { id, name } => workspace(seen, id, name),
        Parsed::QueryWorkspace => match query("j/activeworkspace").as_deref().and_then(parse_workspace_json) {
            Some((id, name)) => workspace(seen, id, name),
            None => Vec::new(),
        },
        Parsed::QueryWindow => match query("j/activewindow") {
            Some(json) => window(seen, parse_window_json(&json)),
            // The request failed: keep what was sent.
            None => Vec::new(),
        },
        Parsed::NoWindow => window(seen, None),
        Parsed::Title { address, title } => match &seen.window {
            Some(w) if w.address == address => {
                let w = ActiveWindow { title, ..w.clone() };
                window(seen, Some(w))
            }
            _ => Vec::new(),
        },
        Parsed::Reseed => seed(seen, query),
    }
}

fn workspace(seen: &mut Seen, id: i64, name: String) -> Vec<HyprEvent> {
    if seen.workspace.as_ref().is_some_and(|(i, n)| *i == id && *n == name) {
        return Vec::new();
    }
    seen.workspace = Some((id, name.clone()));
    vec![HyprEvent::Workspace { id, name }]
}

fn window(seen: &mut Seen, w: Option<ActiveWindow>) -> Vec<HyprEvent> {
    if seen.window == w {
        return Vec::new();
    }
    seen.window = w.clone();
    vec![HyprEvent::ActiveWindow(w)]
}

/// The events for a fresh connection: both queries, through `handle`'s dedup.
fn seed(seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent> {
    let mut out = handle(Parsed::QueryWorkspace, seen, query);
    out.extend(handle(Parsed::QueryWindow, seen, query));
    out
}

/// `{"id": 1, "name": "1", …}` → `(1, "1")`.
fn parse_workspace_json(json: &str) -> Option<(i64, String)> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    Some((v.get("id")?.as_i64()?, v.get("name")?.as_str()?.to_string()))
}

/// `j/activewindow`: `{}` (nothing focused) or no `"address"` → `None`.
fn parse_window_json(json: &str) -> Option<ActiveWindow> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let address = normalise_address(v.get("address")?.as_str()?);
    if address.is_empty() {
        return None;
    }
    let text = |k: &str| v.get(k).and_then(|s| s.as_str()).unwrap_or_default().to_string();
    let pid = v.get("pid").and_then(|p| p.as_i64()).and_then(|p| u32::try_from(p).ok()).filter(|p| *p > 0);
    Some(ActiveWindow { address, class: text("class"), title: text("title"), pid })
}

/// Trim, strip `0x`, lowercase.
fn normalise_address(a: &str) -> String {
    let a = a.trim();
    let a = a.strip_prefix("0x").or_else(|| a.strip_prefix("0X")).unwrap_or(a);
    a.to_ascii_lowercase()
}

/// The instance dirs that have a `.socket2.sock`, in the order to try them:
/// `runtime/hypr/<sig>/` first, then the rest newest first. A crashed Hyprland can leave
/// its socket file behind, so the env's dir isn't always the live one.
fn instances(runtime: &Path, sig: Option<&str>) -> Vec<PathBuf> {
    let hypr = runtime.join("hypr");
    let env = sig.filter(|s| !s.is_empty()).map(|s| hypr.join(s));
    let mut rest: Vec<(std::time::SystemTime, PathBuf)> = std::fs::read_dir(&hypr)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|d| Some(d) != env.as_ref())
        .filter_map(|d| {
            let modified = std::fs::metadata(d.join(".socket2.sock")).ok()?.modified().ok()?;
            Some((modified, d))
        })
        .collect();
    rest.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    let env = env.filter(|d| d.join(".socket2.sock").exists());
    env.into_iter().chain(rest.into_iter().map(|(_, d)| d)).collect()
}

/// The first of `dirs` whose event socket `connect` accepts, with its connection.
fn connect_first<T>(dirs: Vec<PathBuf>, mut connect: impl FnMut(&Path) -> std::io::Result<T>) -> Option<(PathBuf, T)> {
    dirs.into_iter().find_map(|dir| match connect(&dir.join(".socket2.sock")) {
        Ok(stream) => Some((dir, stream)),
        Err(e) => {
            log::debug!("Hyprland at {}: {e}", dir.display());
            None
        }
    })
}

/// One request on `.socket.sock`: connect, write, read to EOF, close. 1 s timeouts.
/// Hyprland serves this socket synchronously and freezes on a connection left open, so
/// each request opens its own stream and drops it at once.
fn request(dir: &Path, cmd: &str) -> Option<String> {
    let mut stream = UnixStream::connect(dir.join(".socket.sock")).ok()?;
    stream.set_read_timeout(Some(REQUEST_TIMEOUT)).ok()?;
    stream.set_write_timeout(Some(REQUEST_TIMEOUT)).ok()?;
    stream.write_all(cmd.as_bytes()).ok()?;
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).ok()?;
    let out = String::from_utf8_lossy(&buf).into_owned();
    (!out.trim().is_empty()).then_some(out)
}

fn run(tx: mpsc::Sender<HyprEvent>, runtime: PathBuf) {
    let mut backoff = Duration::from_secs(1);
    // Survives a reconnect, so a reconnect resends only what changed.
    let mut seen = Seen::default();
    loop {
        // Read each time: a restarted Hyprland has a new signature (`instances` finds it
        // when the environment is stale or names a crashed instance).
        let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        if let Some((dir, stream)) = connect_first(instances(&runtime, sig.as_deref()), |p| UnixStream::connect(p)) {
            let started = Instant::now();
            let mut query = |c: &str| request(&dir, c);
            for ev in seed(&mut seen, &mut query) {
                if tx.send(ev).is_err() {
                    return;
                }
            }
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                // Titles can hold invalid UTF-8; `lines()` would end the connection.
                let line = String::from_utf8_lossy(&buf);
                let Some(p) = parse(line.trim_end_matches('\n')) else { continue };
                for ev in handle(p, &mut seen, &mut query) {
                    if tx.send(ev).is_err() {
                        return;
                    }
                }
            }
            if started.elapsed() > Duration::from_secs(10) {
                backoff = Duration::from_secs(1);
            }
            log::info!("Hyprland event socket closed; reconnecting");
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_query(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn parse_workspacev2() {
        assert_eq!(parse("workspacev2>>2,2"), Some(Parsed::Workspace { id: 2, name: "2".into() }));
        assert_eq!(
            parse("workspacev2>>-98,special:magic"),
            Some(Parsed::Workspace { id: -98, name: "special:magic".into() })
        );
        assert_eq!(parse("workspacev2>>x,2"), None, "the id must be a number");
        assert_eq!(parse("focusedmon>>DP-1,3"), Some(Parsed::QueryWorkspace));
        assert_eq!(parse("renameworkspace>>1,web"), Some(Parsed::QueryWorkspace));
        assert_eq!(parse("configreloaded>>"), Some(Parsed::Reseed));
        assert_eq!(parse("activewindowv2>>55d0a1b2c3d4"), Some(Parsed::QueryWindow));
    }

    #[test]
    fn parse_workspace_name_with_comma() {
        assert_eq!(parse("workspacev2>>5,my,ws"), Some(Parsed::Workspace { id: 5, name: "my,ws".into() }));
        assert_eq!(
            parse("windowtitlev2>>0xABC,a, b"),
            Some(Parsed::Title { address: "abc".into(), title: "a, b".into() })
        );
    }

    #[test]
    fn parse_unknown_ignored() {
        assert_eq!(parse("workspace>>2"), None);
        assert_eq!(parse("activewindow>>a,b"), None);
        assert_eq!(parse("activespecial>>,DP-1"), None);
        assert_eq!(parse("focusedmonv2>>DP-1,3"), None);
        assert_eq!(parse("no separator"), None);
    }

    #[test]
    fn empty_activewindowv2_is_no_window() {
        assert_eq!(parse("activewindowv2>>"), Some(Parsed::NoWindow));
        assert_eq!(parse("activewindowv2>>,"), Some(Parsed::NoWindow));
        // Only sent when something was focused.
        let mut seen = Seen::default();
        assert!(handle(Parsed::NoWindow, &mut seen, &mut no_query).is_empty());
        seen.window = Some(ActiveWindow { address: "a".into(), class: "c".into(), title: "t".into(), pid: None });
        assert_eq!(handle(Parsed::NoWindow, &mut seen, &mut no_query), vec![HyprEvent::ActiveWindow(None)]);
        assert_eq!(seen.window, None);
    }

    #[test]
    fn title_only_for_focused() {
        let mut seen = Seen::default();
        let mut query = |c: &str| match c {
            "j/activewindow" => Some(r#"{"address":"0xabc","class":"firefox","title":"One","pid":42}"#.into()),
            _ => None,
        };
        let evs = handle(Parsed::QueryWindow, &mut seen, &mut query);
        let w = ActiveWindow { address: "abc".into(), class: "firefox".into(), title: "One".into(), pid: Some(42) };
        assert_eq!(evs, vec![HyprEvent::ActiveWindow(Some(w.clone()))]);
        // Another window's title: nothing.
        let other = Parsed::Title { address: "def".into(), title: "Two".into() };
        assert!(handle(other, &mut seen, &mut query).is_empty());
        // The focused one's: the same window, new title.
        let own = Parsed::Title { address: "abc".into(), title: "Two".into() };
        let evs = handle(own, &mut seen, &mut query);
        assert_eq!(evs, vec![HyprEvent::ActiveWindow(Some(ActiveWindow { title: "Two".into(), ..w }))]);
    }

    #[test]
    fn seed_parses_both_queries() {
        let mut seen = Seen::default();
        let mut query = |c: &str| match c {
            "j/activeworkspace" => Some(r#"{"id":3,"name":"3","monitor":"DP-1","windows":1}"#.into()),
            "j/activewindow" => Some(r#"{"address":"0x55d0","class":"kitty","title":"~","pid":7}"#.into()),
            _ => None,
        };
        let evs = seed(&mut seen, &mut query);
        let w = ActiveWindow { address: "55d0".into(), class: "kitty".into(), title: "~".into(), pid: Some(7) };
        assert_eq!(evs, vec![HyprEvent::Workspace { id: 3, name: "3".into() }, HyprEvent::ActiveWindow(Some(w))]);
        // A reconnect (or `configreloaded`) with nothing changed sends nothing.
        assert!(handle(Parsed::Reseed, &mut seen, &mut query).is_empty());
        // A workspace event for the same workspace: nothing.
        assert!(handle(Parsed::Workspace { id: 3, name: "3".into() }, &mut seen, &mut query).is_empty());
    }

    #[test]
    fn window_json_empty_object_is_none() {
        assert_eq!(parse_window_json("{}"), None);
        assert_eq!(parse_window_json(r#"{"class":"x"}"#), None);
        assert_eq!(parse_window_json("not json"), None);
        let w = parse_window_json(r#"{"address":"0xAB","class":"c","title":"t"}"#).unwrap();
        assert_eq!((w.address.as_str(), w.pid), ("ab", None));
        assert_eq!(parse_workspace_json(r#"{"id":1}"#), None);
    }

    fn instance_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("omadeck-hypr-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    /// An instance dir with a `.socket2.sock` file, modified `age` ago.
    fn instance(root: &Path, sig: &str, age: Duration) -> PathBuf {
        let dir = root.join("hypr").join(sig);
        std::fs::create_dir_all(&dir).unwrap();
        let f = std::fs::File::create(dir.join(".socket2.sock")).unwrap();
        f.set_modified(std::time::SystemTime::now() - age).unwrap();
        dir
    }

    #[test]
    fn instances_try_env_first_then_newest() {
        let root = instance_root("env");
        let old = instance(&root, "old", Duration::from_secs(600));
        let mid = instance(&root, "mid", Duration::from_secs(60));
        let new = instance(&root, "new", Duration::ZERO);
        assert_eq!(instances(&root, Some("old")), vec![old.clone(), new.clone(), mid.clone()]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn instances_fall_back_to_newest() {
        let root = instance_root("newest");
        let old = instance(&root, "old", Duration::from_secs(600));
        let new = instance(&root, "new", Duration::ZERO);
        // No dir without a socket counts.
        std::fs::create_dir_all(root.join("hypr").join("empty")).unwrap();
        assert_eq!(instances(&root, None), vec![new.clone(), old.clone()]);
        assert_eq!(instances(&root, Some("gone")), vec![new, old], "a stale signature");
        assert_eq!(instances(&root.join("nothing"), None), Vec::<PathBuf>::new());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_crashed_instance_in_the_env_falls_through_to_a_live_one() {
        // The env's dir keeps a dead `.socket2.sock` after a crash.
        let root = instance_root("crashed");
        let dead = instance(&root, "dead", Duration::from_secs(600));
        let live = instance(&root, "live", Duration::ZERO);
        let mut tried = Vec::new();
        let got = connect_first(instances(&root, Some("dead")), |p: &Path| {
            tried.push(p.to_path_buf());
            if p.starts_with(&dead) { Err(std::io::ErrorKind::ConnectionRefused.into()) } else { Ok(()) }
        });
        assert_eq!(got.map(|(d, ())| d), Some(live.clone()));
        assert_eq!(tried, vec![dead.join(".socket2.sock"), live.join(".socket2.sock")]);
        // The real connect refuses a socket file nobody listens on.
        assert!(connect_first(vec![dead], |p| UnixStream::connect(p)).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A recorded transcript (Hyprland 0.56.2): the terminal and the browser, workspaces
    /// 1–3, tab changes, and empty workspaces. Event addresses have no `0x` prefix; the
    /// query answers with the terminal's window (with `0x`, as the request socket does), so
    /// a focus event comes only on the first focus and after each empty workspace, and the
    /// browser's title lines never match the seen window.
    #[test]
    fn committed_transcript_gives_expected_events() {
        let lines = std::fs::read_to_string("tests/fixtures/socket2.txt").expect("the socket2.txt fixture");
        let mut seen = Seen::default();
        let mut query = |c: &str| match c {
            "j/activewindow" => {
                Some(r#"{"address":"0x55fc9d652d80","class":"kitty","title":"shell","pid":30218}"#.into())
            }
            _ => None,
        };
        let evs: Vec<HyprEvent> =
            lines.lines().filter_map(parse).flat_map(|p| handle(p, &mut seen, &mut query)).collect();
        let w = ActiveWindow {
            address: "55fc9d652d80".into(),
            class: "kitty".into(),
            title: "shell".into(),
            pid: Some(30218),
        };
        assert_eq!(
            evs,
            vec![
                HyprEvent::ActiveWindow(Some(w.clone())),
                HyprEvent::ActiveWindow(None),
                HyprEvent::Workspace { id: 2, name: "2".into() },
                HyprEvent::ActiveWindow(Some(w.clone())),
                HyprEvent::Workspace { id: 1, name: "1".into() },
                HyprEvent::ActiveWindow(None),
                HyprEvent::Workspace { id: 2, name: "2".into() },
                HyprEvent::ActiveWindow(Some(w.clone())),
                HyprEvent::Workspace { id: 1, name: "1".into() },
                HyprEvent::ActiveWindow(None),
                HyprEvent::Workspace { id: 3, name: "3".into() },
                HyprEvent::ActiveWindow(Some(w.clone())),
                HyprEvent::Workspace { id: 1, name: "1".into() },
            ]
        );
    }

    /// The event forms the Hyprland wiki documents, through `parse` and `handle`.
    #[test]
    fn documented_lines_give_events() {
        let mut seen = Seen::default();
        let mut query = |c: &str| match c {
            "j/activeworkspace" => Some(r#"{"id":2,"name":"web"}"#.into()),
            "j/activewindow" => Some(r#"{"address":"0x1","class":"firefox","title":"Tab","pid":9}"#.into()),
            _ => None,
        };
        let lines = [
            "workspace>>2",
            "workspacev2>>2,2",
            "activewindow>>firefox,Tab",
            "activewindowv2>>1",
            "windowtitlev2>>1,Tab 2",
            "renameworkspace>>2,web",
            "activewindowv2>>",
        ];
        let evs: Vec<HyprEvent> =
            lines.iter().filter_map(|l| parse(l)).flat_map(|p| handle(p, &mut seen, &mut query)).collect();
        let w = ActiveWindow { address: "1".into(), class: "firefox".into(), title: "Tab".into(), pid: Some(9) };
        assert_eq!(
            evs,
            vec![
                HyprEvent::Workspace { id: 2, name: "2".into() },
                HyprEvent::ActiveWindow(Some(w.clone())),
                HyprEvent::ActiveWindow(Some(ActiveWindow { title: "Tab 2".into(), ..w })),
                HyprEvent::Workspace { id: 2, name: "web".into() },
                HyprEvent::ActiveWindow(None),
            ]
        );
    }
}
