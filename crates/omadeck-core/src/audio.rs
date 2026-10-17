//! Reading and changing the system volume, through Omarchy's own commands when they're
//! installed (so its on-screen pop-up shows) and through `wpctl` otherwise. Also one
//! app's playback streams (PR 10): read with `pactl`, changed with `wpctl`.

use std::collections::{HashMap, HashSet};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};

use crate::config::AudioDevice;

/// A device's current level, as read from the audio stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// 0–100 (may exceed 100 if set outside omadeck).
    pub percent: u8,
    pub muted: bool,
}

impl Level {
    /// The display model for the `$B1` strip panel: the bar position and the text next
    /// to it. Mute reads `Muted`; the bar is left empty by the renderer.
    pub fn to_panel(&self) -> crate::render::Level {
        crate::render::Level {
            value: (self.percent as f32 / 100.0).min(1.0),
            text: if self.muted { "Muted".into() } else { format!("{}%", self.percent) },
            muted: self.muted,
        }
    }
}

/// One playback stream (a sink input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    /// PipeWire node id (`properties."object.id"`), what wpctl takes.
    pub id: u32,
    /// `application.process.id`.
    pub pid: Option<u32>,
    /// `application.process.binary`, lowercased ("" if absent).
    pub binary: String,
    /// `application.name`, else `media.name`, else `node.name` ("" if none).
    pub name: String,
    /// `pipewire.access.portal.app_id`, else `application.id`, lowercased.
    pub app_id: Option<String>,
    /// `application.icon_name`.
    pub icon_name: Option<String>,
    /// The loudest channel's percent, plus mute.
    pub level: Level,
    /// Paused but present: still controllable.
    pub corked: bool,
}

/// What an app volume action or panel targets. The probe key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppQuery {
    /// A chosen app, as the user typed or picked it (trimmed).
    Named(String),
    /// Whichever app has the focused window (or the last focused one that had audio).
    Focused,
}

/// One app's streams added up, as the watcher reports them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppLevel {
    /// Display name. For a focused dial with no audio: the window's class ("" if none).
    pub name: String,
    /// What `select_named` finds the app by again: the first stream's binary, else its
    /// app id, else its lowercased name. "" when nothing matched. The sticky app.
    pub key: String,
    /// Filled by the watcher from `icon_names`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    /// Names to look the icon up by, in order. Never sent.
    #[serde(skip)]
    pub icon_names: Vec<String>,
    /// `percent` = max over streams, `muted` = all muted. `None`: no stream now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// Every stream's node id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<u32>,
}

/// The focused window, as matching needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub pid: Option<u32>,
    pub class: String,
}

/// Every playback stream now. `None`: `pactl` is missing, failed or gave something else.
/// Blocking: the watcher, a helper thread or `gio::spawn_blocking` only.
pub fn streams() -> Option<Vec<Stream>> {
    parse_pactl_streams(&capture(&["pactl", "-f", "json", "list", "sink-inputs"])?)
}

/// A JSON number, or a string holding one (`pactl` writes properties as strings).
fn json_u64(v: Option<&serde_json::Value>) -> Option<u64> {
    let v = v?;
    v.as_u64().or_else(|| v.as_str()?.trim().parse().ok())
}

fn json_str(props: &serde_json::Value, key: &str) -> Option<String> {
    props.get(key)?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string)
}

/// Parse `pactl -f json list sink-inputs`. Not an array: `None`. A stream without a
/// usable `object.id` is skipped (wpctl couldn't change it); a missing optional key never
/// fails a stream.
pub fn parse_pactl_streams(json: &str) -> Option<Vec<Stream>> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = v.as_array()?;
    let mut out = Vec::with_capacity(arr.len());
    let null = serde_json::Value::Null;
    for s in arr {
        let props = s.get("properties").unwrap_or(&null);
        let Some(id) = json_u64(props.get("object.id")).and_then(|n| u32::try_from(n).ok()) else {
            log::warn!("pactl: a sink input without a usable object.id, skipped (index {:?})", s.get("index"));
            continue;
        };
        let percent = s
            .get("volume")
            .and_then(|v| v.as_object())
            .map(|chans| {
                chans
                    .values()
                    .filter_map(|c| {
                        c.get("value_percent")?.as_str()?.trim().trim_end_matches('%').trim().parse::<u32>().ok()
                    })
                    .max()
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        let name =
            ["application.name", "media.name", "node.name"].iter().find_map(|k| json_str(props, k)).unwrap_or_default();
        out.push(Stream {
            id,
            pid: json_u64(props.get("application.process.id")).and_then(|n| u32::try_from(n).ok()),
            binary: json_str(props, "application.process.binary").unwrap_or_default().to_lowercase(),
            name,
            app_id: ["pipewire.access.portal.app_id", "application.id"]
                .iter()
                .find_map(|k| json_str(props, k))
                .map(|s| s.to_lowercase()),
            icon_name: json_str(props, "application.icon_name"),
            level: Level {
                percent: percent.min(255) as u8,
                muted: s.get("mute").and_then(|m| m.as_bool()).unwrap_or(false),
            },
            corked: s.get("corked").and_then(|m| m.as_bool()).unwrap_or(false),
        });
    }
    Some(out)
}

/// The canonical form of an app as a user may write it: trimmed, lowercased, the last
/// `/` segment (`/usr/bin/spotify`), without `.desktop`.
pub fn normalise_app(q: &str) -> String {
    let q = q.trim().to_lowercase();
    let last = q.rsplit('/').next().unwrap_or("");
    last.strip_suffix(".desktop").unwrap_or(last).trim().to_string()
}

/// A readable name for an app: a few well-known ones, else the normalised form with its
/// first letter upper-cased.
pub fn display_app(q: &str) -> String {
    let n = normalise_app(q);
    let known = match n.as_str() {
        "spotify" | "com.spotify.client" => Some("Spotify"),
        "firefox" => Some("Firefox"),
        "chromium" => Some("Chromium"),
        "google-chrome" => Some("Chrome"),
        "brave" => Some("Brave"),
        "mpv" => Some("mpv"),
        "vlc" => Some("VLC"),
        "discord" => Some("Discord"),
        _ => None,
    };
    if let Some(k) = known {
        return k.to_string();
    }
    let mut c = n.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// The streams of the app named `name` (any form `normalise_app` takes): its binary, its
/// name or its app id.
pub fn select_named<'a>(s: &'a [Stream], name: &str) -> Vec<&'a Stream> {
    let n = normalise_app(name);
    if n.is_empty() {
        return vec![];
    }
    s.iter().filter(|st| st.binary == n || st.name.to_lowercase() == n || st.app_id.as_deref() == Some(&n)).collect()
}

/// The streams of the focused window's app. By pid first: the window's process or any
/// of its children (Chromium and Electron play from a child, a terminal runs `mpv`). By
/// class only when no pid matched (Flatpak apps, whose pids are in a sandbox).
pub fn select_window<'a>(s: &'a [Stream], w: &Window, parent: impl Fn(u32) -> Option<u32>) -> Vec<&'a Stream> {
    if let Some(wpid) = w.pid {
        let by_pid: Vec<&Stream> =
            s.iter().filter(|st| st.pid.is_some_and(|p| descends_from(p, wpid, &parent))).collect();
        if !by_pid.is_empty() {
            return by_pid;
        }
    }
    let class = w.class.trim().to_lowercase();
    if class.is_empty() {
        return vec![];
    }
    s.iter()
        .filter(|st| st.app_id.as_deref() == Some(&class) || st.binary == class || st.name.to_lowercase() == class)
        .collect()
}

/// Whether `pid` is `ancestor` or one of its descendants. Stops at pid 1, at a repeat
/// and after 32 hops.
fn descends_from(pid: u32, ancestor: u32, parent: &impl Fn(u32) -> Option<u32>) -> bool {
    let mut seen = HashSet::new();
    let mut cur = pid;
    for _ in 0..=32 {
        if cur == ancestor {
            return true;
        }
        if cur <= 1 || !seen.insert(cur) {
            return false;
        }
        match parent(cur) {
            Some(p) => cur = p,
            None => return false,
        }
    }
    false
}

/// A process's parent, from `/proc/<pid>/stat` (the second field after the `)` that
/// ends the command name).
pub fn proc_parent(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

/// A stream name that says nothing about the app.
fn is_generic_name(n: &str) -> bool {
    n.is_empty() || n == "audio-src" || n.starts_with("ALSA plug-in")
}

/// Several streams of one app (a tab each) as one: the loudest percent, muted only when
/// all are, every id. `None` for no streams. The icon is left for the watcher.
pub fn app_level(matched: &[&Stream]) -> Option<AppLevel> {
    let first = matched.first()?;
    let name = if is_generic_name(first.name.trim()) { display_app(&first.binary) } else { first.name.clone() };
    let key = if !first.binary.is_empty() {
        first.binary.clone()
    } else if let Some(id) = first.app_id.as_ref().filter(|s| !s.is_empty()) {
        id.clone()
    } else {
        first.name.to_lowercase()
    };
    let mut icon_names: Vec<String> = Vec::new();
    for n in [
        first.icon_name.clone().unwrap_or_default(),
        key.clone(),
        first.name.clone(),
        first.app_id.clone().unwrap_or_default(),
    ] {
        if !n.is_empty() && !icon_names.contains(&n) {
            icon_names.push(n);
        }
    }
    Some(AppLevel {
        name,
        key,
        icon: None,
        icon_names,
        level: Some(Level {
            percent: matched.iter().map(|s| s.level.percent).max().unwrap_or(0),
            muted: matched.iter().all(|s| s.level.muted),
        }),
        ids: matched.iter().map(|s| s.id).collect(),
    })
}

/// A chosen app. Not playing: no level, named and iconed after what was chosen, so a
/// stopped Spotify still shows Spotify.
pub fn resolve_named(s: &[Stream], name: &str) -> AppLevel {
    app_level(&select_named(s, name)).unwrap_or_else(|| {
        let n = normalise_app(name);
        AppLevel {
            name: display_app(name),
            key: String::new(),
            icon: None,
            icon_names: if n.is_empty() { vec![] } else { vec![n] },
            level: None,
            ids: vec![],
        }
    })
}

/// What a focused dial controls: the window's app if it has a stream, else the sticky
/// app if it has one, else nothing (a `level: None` AppLevel named after the class).
/// Updates `sticky`: a window with audio replaces it; a silent one never clears it.
pub fn resolve_focused(
    s: &[Stream],
    w: Option<&Window>,
    sticky: &mut Option<String>,
    parent: impl Fn(u32) -> Option<u32>,
) -> AppLevel {
    if let Some(lv) = w.and_then(|w| app_level(&select_window(s, w, &parent))) {
        if !lv.key.is_empty() {
            *sticky = Some(lv.key.clone());
        }
        return lv;
    }
    if let Some(k) = sticky.as_deref() {
        let lv = resolve_named(s, k);
        if lv.level.is_some() {
            return lv;
        }
    }
    AppLevel {
        name: w.map(|w| w.class.clone()).unwrap_or_default(),
        key: String::new(),
        icon: None,
        icon_names: vec![],
        level: None,
        ids: vec![],
    }
}

/// `resolve_named` or `resolve_focused`, by the query.
pub fn resolve(
    s: &[Stream],
    q: &AppQuery,
    w: Option<&Window>,
    sticky: &mut Option<String>,
    parent: impl Fn(u32) -> Option<u32>,
) -> AppLevel {
    match q {
        AppQuery::Named(n) => resolve_named(s, n),
        AppQuery::Focused => resolve_focused(s, w, sticky, parent),
    }
}

const ICON_DIRS: [&str; 6] = [
    "icons/hicolor/scalable/apps",
    "icons/hicolor/256x256/apps",
    "icons/hicolor/128x128/apps",
    "icons/hicolor/64x64/apps",
    "icons/hicolor/48x48/apps",
    "pixmaps",
];

/// An app icon file by its icon name, under the data dirs `roots`.
fn icon_file_in(roots: &[PathBuf], name: &str) -> Option<PathBuf> {
    if name.is_empty() || name.contains('/') || name.starts_with('.') {
        return None;
    }
    for root in roots {
        for dir in ICON_DIRS {
            for ext in ["svg", "png"] {
                let p = root.join(dir).join(format!("{name}.{ext}"));
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    None
}

/// An app's icon without GTK: each name as an icon name (hicolor, then pixmaps, svg
/// before png), then each name's desktop entry's `Icon=` (an absolute path as is).
pub fn app_icon_in(roots: &[PathBuf], names: &[String]) -> Option<PathBuf> {
    if let Some(p) = names.iter().find_map(|n| icon_file_in(roots, n)) {
        return Some(p);
    }
    for name in names.iter().filter(|n| !n.is_empty() && !n.contains('/') && !n.starts_with('.')) {
        for root in roots {
            let Ok(text) = std::fs::read_to_string(root.join("applications").join(format!("{name}.desktop"))) else {
                continue;
            };
            let Some(icon) = text.lines().find_map(|l| l.trim().strip_prefix("Icon=")).map(str::trim) else {
                continue;
            };
            let path = Path::new(icon);
            if path.is_absolute() {
                if path.is_file() {
                    return Some(path.to_path_buf());
                }
            } else if let Some(p) = icon_file_in(roots, icon) {
                return Some(p);
            }
        }
    }
    None
}

/// `app_icon_in` over the real data dirs, cached (misses too). Blocking file reads: the
/// watcher thread only.
pub fn app_icon(names: &[String]) -> Option<PathBuf> {
    static CACHE: OnceLock<Mutex<HashMap<Vec<String>, Option<PathBuf>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(names) {
        return hit.clone();
    }
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut roots: Vec<PathBuf> = Vec::new();
    match std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        Some(d) => roots.push(PathBuf::from(d)),
        None => roots.extend(home.as_ref().map(|h| h.join(".local/share"))),
    }
    let dirs = std::env::var("XDG_DATA_DIRS").ok().filter(|v| !v.is_empty());
    roots.extend(
        dirs.as_deref()
            .unwrap_or("/usr/local/share:/usr/share")
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    roots.extend(home.as_ref().map(|h| h.join(".local/share/flatpak/exports/share")));
    roots.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let found = app_icon_in(&roots, names);
    if let Ok(mut c) = cache.lock() {
        c.insert(names.to_vec(), found.clone());
    }
    found
}

/// The `wpctl` runs that apply an app volume change to its streams `ids`, in order. A
/// change is relative per stream (tabs keep their balance) and capped at 100%; a mute
/// sets one state for all (`mute_to`, which the caller picks). Nothing to do: none.
pub fn app_volume_commands(ids: &[u32], change: i32, mute_to: Option<bool>) -> Vec<Vec<String>> {
    if let Some(m) = mute_to {
        return ids
            .iter()
            .map(|id| vec!["wpctl".into(), "set-mute".into(), id.to_string(), if m { "1" } else { "0" }.into()])
            .collect();
    }
    if change == 0 {
        return vec![];
    }
    let step = format!("{}%{}", change.unsigned_abs().min(100), if change > 0 { '+' } else { '-' });
    ids.iter()
        .map(|id| vec!["wpctl".into(), "set-volume".into(), "-l".into(), "1.0".into(), id.to_string(), step.clone()])
        .collect()
}

/// Run each argv in order, each in its own process group, waiting for each (up to
/// `COMMAND_TIMEOUT`). Blocking: the watcher and `action::run`'s helper threads only.
pub fn run_commands(cmds: &[Vec<String>]) {
    if cmds.is_empty() {
        return;
    }
    log::info!("exec {}", cmds.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ; "));
    for argv in cmds.iter().filter(|a| !a.is_empty()) {
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]).process_group(0);
        if crate::proc::output_timeout(&mut c, crate::proc::COMMAND_TIMEOUT).is_none() {
            log::warn!("{argv:?} failed to start or didn't finish");
        }
    }
}

/// The focused window from `hyprctl -j activewindow`, for `action::run` outside the
/// daemon (the daemon has the Hyprland listener). Blocking.
pub fn focused_window() -> Option<Window> {
    parse_hyprctl_window(&capture(&["hyprctl", "-j", "activewindow"])?)
}

/// Parse `hyprctl -j activewindow`: `{}` (nothing focused) or no `class` gives `None`.
pub fn parse_hyprctl_window(json: &str) -> Option<Window> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    let class = v.get("class")?.as_str()?.to_string();
    let pid = v.get("pid").and_then(|p| p.as_i64()).filter(|p| *p > 0).and_then(|p| u32::try_from(p).ok());
    Some(Window { pid, class })
}

/// A stream for tests (here and in the daemon's): 40%, unmuted, named after `binary`.
#[cfg(any(test, feature = "test-support"))]
pub fn test_stream(id: u32, pid: u32, binary: &str) -> Stream {
    Stream {
        id,
        pid: Some(pid),
        binary: binary.to_string(),
        name: binary.to_string(),
        app_id: None,
        icon_name: None,
        level: Level { percent: 40, muted: false },
        corked: false,
    }
}

/// Which volume backend to drive, chosen once from what's on `PATH`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    /// Omarchy 4: `omarchy-audio-output-volume` / `omarchy-audio-input-mute` on `PATH`.
    Omarchy4,
    /// Omarchy 3: `swayosd-client` on `PATH`.
    Omarchy3,
    /// Neither: plain `wpctl`.
    Plain,
}

pub fn backend() -> Backend {
    static B: OnceLock<Backend> = OnceLock::new();
    *B.get_or_init(|| {
        if which("omarchy-audio-output-volume") {
            Backend::Omarchy4
        } else if which("swayosd-client") {
            Backend::Omarchy3
        } else {
            Backend::Plain
        }
    })
}

/// Read the level of `device`, from the same sink the backend changes.
pub fn volume(device: AudioDevice) -> Option<Level> {
    match (backend(), device) {
        // Omarchy 4's script resolves through any DSP sink (speaker tuning, EasyEffects)
        // to the physical one; read that same sink so the strip and its pop-up agree.
        (Backend::Omarchy4, AudioDevice::Output) => {
            let sink = sink_name()?;
            let vol = capture(&["pactl", "get-sink-volume", &sink])?;
            let mute = capture(&["pactl", "get-sink-mute", &sink])?;
            parse_pactl(&vol, &mute)
        }
        _ => {
            let target = if device.is_output() { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" };
            parse_wpctl(&capture(&["wpctl", "get-volume", target])?)
        }
    }
}

/// The physical sink Omarchy 4 controls, for the current default output.
fn sink_name() -> Option<String> {
    let out = capture(&["omarchy-audio-output-sink"])?;
    let s = out.trim().to_string();
    (!s.is_empty()).then_some(s)
}

/// The focused monitor's name, for Omarchy 3's `swayosd-client --monitor`.
pub fn focused_monitor() -> Option<String> {
    let out = capture(&["hyprctl", "monitors", "-j"])?;
    let v: serde_json::Value = serde_json::from_str(&out).ok()?;
    let arr = v.as_array()?;
    for m in arr {
        if m.get("focused")?.as_bool() == Some(true) {
            return m.get("name")?.as_str().map(str::to_string);
        }
    }
    None
}

/// Parse `wpctl get-volume` output: `Volume: 0.40` or `Volume: 0.40 [MUTED]`.
pub fn parse_wpctl(out: &str) -> Option<Level> {
    let line = out.lines().next()?;
    let mut parts = line.split_whitespace();
    let _ = parts.next()?; // "Volume:"
    let frac: f32 = parts.next()?.parse().ok()?;
    let percent = (frac * 100.0).round().clamp(0.0, 255.0) as u8;
    Some(Level { percent, muted: line.contains("MUTED") })
}

/// Parse `pactl get-sink-volume` (first `NN%` on the first line) and `get-sink-mute`.
pub fn parse_pactl(volume_out: &str, mute_out: &str) -> Option<Level> {
    let percent = volume_out
        .lines()
        .next()?
        .split_whitespace()
        .find(|t| t.ends_with('%'))
        .and_then(|t| t.trim_end_matches('%').parse::<u32>().ok())?;
    // Clamped like `parse_wpctl`, so a level above 255% can't fail the whole read.
    let muted = mute_out.split_whitespace().last() == Some("yes");
    Some(Level { percent: percent.min(255) as u8, muted })
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// Run a command and return its stdout, `None` if it is missing, fails or hangs.
fn capture(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(args[0]);
    cmd.args(&args[1..]).env("LC_ALL", "C");
    let out = crate::proc::output_timeout(&mut cmd, crate::proc::COMMAND_TIMEOUT)?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpctl_forms() {
        assert_eq!(parse_wpctl("Volume: 0.40"), Some(Level { percent: 40, muted: false }));
        assert_eq!(parse_wpctl("Volume: 0.40 [MUTED]"), Some(Level { percent: 40, muted: true }));
        assert_eq!(parse_wpctl("Volume: 1.00"), Some(Level { percent: 100, muted: false }));
        // Above 100% (set outside omadeck) is reported as the real number.
        assert_eq!(parse_wpctl("Volume: 1.53"), Some(Level { percent: 153, muted: false }));
        assert_eq!(parse_wpctl("garbage"), None);
        assert_eq!(parse_wpctl(""), None);
    }

    #[test]
    fn pactl_forms() {
        // Real output captured on an Omarchy machine.
        let vol =
            "Volume: front-left: 56359 /  86% / -3.93 dB,   front-right: 56359 /  86% / -3.93 dB\n        balance 0.00";
        assert_eq!(parse_pactl(vol, "Mute: no"), Some(Level { percent: 86, muted: false }));
        assert_eq!(parse_pactl(vol, "Mute: yes"), Some(Level { percent: 86, muted: true }));
        assert_eq!(parse_pactl("Volume: 100%\n", "Mute: no"), Some(Level { percent: 100, muted: false }));
        // Above 100% parses (the old `u8` gave up at 256 and froze the bar).
        assert_eq!(parse_pactl("Volume: 150%\n", "Mute: no"), Some(Level { percent: 150, muted: false }));
        assert_eq!(parse_pactl("Volume: 260%\n", "Mute: no"), Some(Level { percent: 255, muted: false }));
        assert_eq!(parse_pactl("", "Mute: no"), None);
        assert_eq!(parse_pactl("no percent here", "Mute: no"), None);
    }

    #[test]
    fn pactl_streams_parse() {
        // Hand-written in the shape `pactl -f json list sink-inputs` gives (nothing has
        // been captured yet). The pid is a number here, the id a string: both forms must
        // parse.
        let json = r#"[{"index":91,"corked":false,"mute":false,
            "volume":{"front-left":{"value":45875,"value_percent":"70%","db":"-9.29 dB"},
                      "front-right":{"value":52428,"value_percent":"80%","db":"-5.81 dB"}},
            "properties":{"object.id":"87","application.name":"Spotify",
                          "application.process.id":4242,"application.process.binary":"spotify",
                          "application.icon_name":"spotify-client"}}]"#;
        let s = parse_pactl_streams(json).unwrap();
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].id, 87);
        assert_eq!(s[0].pid, Some(4242));
        assert_eq!(s[0].binary, "spotify");
        assert_eq!(s[0].name, "Spotify");
        assert_eq!(s[0].icon_name.as_deref(), Some("spotify-client"));
        assert_eq!(s[0].level, Level { percent: 80, muted: false });
        assert!(!s[0].corked);
    }

    #[test]
    fn pactl_stream_fallbacks() {
        // The other number forms, a muted corked stream, the name and app id fallbacks.
        let json = r#"[{"mute":true,"corked":true,
            "volume":{"mono":{"value_percent":"260%"}},
            "properties":{"object.id":12,"application.process.id":"77","media.name":"audio-src",
                          "pipewire.access.portal.app_id":"com.Spotify.Client","application.id":"x"}},
            {"properties":{"object.id":"13","node.name":"alsa_playback.mpv"}}]"#;
        let s = parse_pactl_streams(json).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].id, s[0].pid), (12, Some(77)));
        assert_eq!(s[0].name, "audio-src");
        assert_eq!(s[0].app_id.as_deref(), Some("com.spotify.client"));
        assert_eq!(s[0].level, Level { percent: 255, muted: true });
        assert!(s[0].corked);
        assert_eq!(s[0].binary, "");
        assert_eq!(s[1].name, "alsa_playback.mpv");
        assert_eq!((s[1].pid, s[1].level.percent, s[1].app_id.clone()), (None, 0, None));
    }

    #[test]
    fn pactl_garbage_is_none() {
        assert_eq!(parse_pactl_streams("x"), None);
        assert_eq!(parse_pactl_streams("{}"), None);
        assert_eq!(parse_pactl_streams("[]"), Some(vec![]));
    }

    #[test]
    fn pactl_skips_stream_without_object_id() {
        let json = r#"[{"properties":{"application.name":"A"}},
                       {"properties":{"object.id":"nope","application.name":"B"}},
                       {"properties":{"object.id":"5","application.name":"C"}}]"#;
        let s = parse_pactl_streams(json).unwrap();
        assert_eq!(s.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["C"]);
    }

    fn st(id: u32, pid: u32, binary: &str) -> Stream {
        test_stream(id, pid, binary)
    }

    #[test]
    fn select_named_forms() {
        let mut spot = st(87, 100, "spotify");
        spot.app_id = Some("com.spotify.client".into());
        let s = vec![spot, st(90, 200, "firefox")];
        for q in ["spotify", "Spotify", "spotify.desktop", "/usr/bin/spotify", "com.spotify.Client", " spotify "] {
            let hit = select_named(&s, q);
            assert_eq!(hit.iter().map(|s| s.id).collect::<Vec<_>>(), vec![87], "{q}");
        }
        assert!(select_named(&s, "spot").is_empty());
        assert!(select_named(&s, "").is_empty());
        assert_eq!(normalise_app(" /usr/share/applications/Firefox.desktop "), "firefox");
    }

    #[test]
    fn select_window_pid_and_children() {
        // 100 is the browser window; 101 its audio child; 102 a grandchild. 300 and 301
        // point at each other.
        let parents: HashMap<u32, u32> = HashMap::from([(101, 100), (102, 101), (100, 1), (300, 301), (301, 300)]);
        let parent = |p: u32| parents.get(&p).copied();
        let s = vec![st(1, 101, "chromium"), st(2, 102, "chromium"), st(3, 300, "loop"), st(4, 1, "init")];
        let w = Window { pid: Some(100), class: "chromium".into() };
        let ids: Vec<u32> = select_window(&s, &w, parent).iter().map(|s| s.id).collect();
        assert_eq!(ids, vec![1, 2]);
        // A cycle and pid 1 end the walk without a match (then class: none).
        let w = Window { pid: Some(999), class: "nothing".into() };
        assert!(select_window(&s, &w, parent).is_empty());
    }

    #[test]
    fn select_window_class_fallback() {
        let mut flat = st(5, 4000, "");
        flat.app_id = Some("com.spotify.client".into());
        let s = vec![flat, st(6, 50, "mpv")];
        let none = |_| None;
        // The window's pid is outside the sandbox: no pid match, so its class matches.
        let w = Window { pid: Some(10), class: "com.spotify.Client".into() };
        assert_eq!(select_window(&s, &w, none).iter().map(|s| s.id).collect::<Vec<_>>(), vec![5]);
        // A pid match wins over the class.
        let w = Window { pid: Some(50), class: "com.spotify.Client".into() };
        assert_eq!(select_window(&s, &w, none).iter().map(|s| s.id).collect::<Vec<_>>(), vec![6]);
        let w = Window { pid: None, class: String::new() };
        assert!(select_window(&s, &w, none).is_empty());
    }

    #[test]
    fn app_level_aggregates() {
        let mut a = st(1, 10, "firefox");
        a.level = Level { percent: 30, muted: true };
        let mut b = st(2, 10, "firefox");
        b.level = Level { percent: 70, muted: false };
        let lv = app_level(&[&a, &b]).unwrap();
        assert_eq!(lv.level, Some(Level { percent: 70, muted: false }));
        assert_eq!(lv.ids, vec![1, 2]);
        assert_eq!(lv.key, "firefox");
        b.level.muted = true;
        assert!(app_level(&[&a, &b]).unwrap().level.unwrap().muted);
        assert_eq!(app_level(&[]), None);
    }

    #[test]
    fn display_name_rules() {
        let mut a = st(1, 10, "brave");
        a.name = "audio-src".into();
        assert_eq!(app_level(&[&a]).unwrap().name, "Brave");
        a.name = "ALSA plug-in [mpv]".into();
        a.binary = "foo-player".into();
        assert_eq!(app_level(&[&a]).unwrap().name, "Foo-player");
        a.name = "Music".into();
        assert_eq!(app_level(&[&a]).unwrap().name, "Music");
        assert_eq!(display_app("com.spotify.Client"), "Spotify");
        assert_eq!(display_app("vlc.desktop"), "VLC");
        assert_eq!(display_app("  "), "");
        // Not playing: named and iconed after what was chosen.
        let lv = resolve_named(&[], "Spotify.desktop");
        assert_eq!((lv.name.as_str(), lv.key.as_str(), lv.level), ("Spotify", "", None));
        assert_eq!(lv.icon_names, vec!["spotify".to_string()]);
    }

    #[test]
    fn sticky_focus() {
        let s = vec![test_stream(87, 100, "firefox")];
        let no_parent = |_| None;
        let mut sticky = None;
        // Firefox focused: it, and it becomes sticky.
        let w = Window { pid: Some(100), class: "firefox".into() };
        let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
        assert_eq!(lv.ids, vec![87]);
        assert_eq!(sticky.as_deref(), Some("firefox"));
        // A silent terminal focused: still Firefox.
        let w = Window { pid: Some(200), class: "Alacritty".into() };
        let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
        assert_eq!(lv.ids, vec![87]);
        // Firefox stops: the terminal's class, no level.
        let lv = resolve_focused(&[], Some(&w), &mut sticky, no_parent);
        assert_eq!((lv.name.as_str(), lv.level), ("Alacritty", None));
        assert_eq!(sticky.as_deref(), Some("firefox"), "kept until another app plays");
        // Nothing focused: no class.
        let lv = resolve_focused(&[], None, &mut sticky, no_parent);
        assert_eq!(lv.name, "");
    }

    #[test]
    fn app_icon_lookup() {
        let root = std::env::temp_dir().join(format!("omadeck-icons-{}", std::process::id()));
        let apps = root.join("icons/hicolor/scalable/apps");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::create_dir_all(root.join("applications")).unwrap();
        std::fs::create_dir_all(root.join("pixmaps")).unwrap();
        std::fs::write(apps.join("foo.svg"), "<svg/>").unwrap();
        std::fs::write(root.join("pixmaps/baz.png"), "png").unwrap();
        std::fs::write(root.join("applications/bar.desktop"), "[Desktop Entry]\nName=Bar\nIcon=foo\n").unwrap();
        let roots = vec![root.join("missing"), root.clone()];
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(app_icon_in(&roots, &names(&["bar"])), Some(apps.join("foo.svg")));
        assert_eq!(app_icon_in(&roots, &names(&["nope", "baz"])), Some(root.join("pixmaps/baz.png")));
        assert_eq!(app_icon_in(&roots, &names(&["nope", "../x", ""])), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn app_volume_commands_change_and_mute() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            app_volume_commands(&[87, 90], 5, None),
            vec![
                v(&["wpctl", "set-volume", "-l", "1.0", "87", "5%+"]),
                v(&["wpctl", "set-volume", "-l", "1.0", "90", "5%+"])
            ]
        );
        assert_eq!(
            app_volume_commands(&[87], -250, None),
            vec![v(&["wpctl", "set-volume", "-l", "1.0", "87", "100%-"])]
        );
        assert_eq!(
            app_volume_commands(&[87, 90], 0, Some(true)),
            vec![v(&["wpctl", "set-mute", "87", "1"]), v(&["wpctl", "set-mute", "90", "1"])]
        );
        assert_eq!(app_volume_commands(&[87], 5, Some(false)), vec![v(&["wpctl", "set-mute", "87", "0"])]);
        assert!(app_volume_commands(&[87], 0, None).is_empty());
        assert!(app_volume_commands(&[], 5, None).is_empty());
    }

    #[test]
    fn hyprctl_window_parses() {
        // Hand-written in the documented shape (not captured yet): Firefox focused, and
        // an empty workspace's `{}`.
        let w = parse_hyprctl_window(r#"{"address":"0x5612","class":"firefox","title":"Mozilla Firefox","pid":4321}"#);
        assert_eq!(w, Some(Window { pid: Some(4321), class: "firefox".into() }));
        assert_eq!(parse_hyprctl_window("{}"), None);
        assert_eq!(parse_hyprctl_window(r#"{"class":"x","pid":-1}"#), Some(Window { pid: None, class: "x".into() }));
        assert_eq!(parse_hyprctl_window("garbage"), None);
    }

    #[test]
    fn panel_model() {
        assert_eq!(Level { percent: 40, muted: false }.to_panel().text, "40%");
        assert!((Level { percent: 40, muted: false }.to_panel().value - 0.4).abs() < f32::EPSILON);
        let muted = Level { percent: 40, muted: true }.to_panel();
        assert_eq!(muted.text, "Muted");
        assert!(muted.muted);
        // Above 100%: the real number, a full bar.
        let over = Level { percent: 153, muted: false }.to_panel();
        assert_eq!(over.text, "153%");
        assert_eq!(over.value, 1.0);
    }
}
