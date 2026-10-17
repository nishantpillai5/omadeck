//! System state the daemon watches (a probe), the values it reads, and what a dial's
//! strip panel shows for them. The daemon's watcher thread reads probes; the daemon, the
//! configurator and `snapshot` all build panels through [`dial_status`].

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::audio::{AppLevel, AppQuery};
use crate::config::{AudioDevice, Config, DialConfig, KeyConfig, LevelSource, Live};
use crate::player::{self, PlayStatus, PlayerRead};
use crate::render::{AppShow, DialStatus, KeyStatus, Level, LiveShow, MediaShow, PanelStatus};

/// A volume set from the keyboard or `pavucontrol` lands in well under this.
const VOLUME_REFRESH: [Duration; 1] = [Duration::from_millis(150)];
/// A state command's answer: at 300 ms the optimistic guess is confirmed or corrected;
/// at 1.2 s a fast toggle that really failed shows its true state (outside the 1 s
/// `SETTLE` window).
const COMMAND_REFRESH: [Duration; 2] = [Duration::from_millis(300), Duration::from_millis(1200)];
/// How long a player takes to report a new status after a command.
pub const MEDIA_REFRESH_DELAY: Duration = Duration::from_millis(600);
/// After a media command: the first read usually confirms the guess; the second lands
/// after `SETTLE`, so a failed command shows the truth.
const PLAYER_REFRESH: [Duration; 2] = [MEDIA_REFRESH_DELAY, Duration::from_millis(1500)];

/// While a toggle is busy, or a slow press is waiting for its state to move.
pub const FAST_INTERVAL: Duration = Duration::from_millis(500);
/// How long a slow press keeps polling fast before the normal interval resumes.
pub const FAST_WINDOW: Duration = Duration::from_secs(15);
/// How long a fast toggle's optimistic guess wins over a contradicting read.
pub const SETTLE: Duration = Duration::from_secs(1);

/// Something the daemon reads from the system.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    /// An audio device's level and mute.
    Volume(AudioDevice),
    /// `sh -c command` every `interval_ms`. Both fields are the hash key **on purpose**:
    /// the same command at two intervals is two probes, so each key polls at the rate it
    /// asked for. Keys with identical command and interval share one probe.
    Command { command: String, interval_ms: u32 },
    /// Local time in this strftime format.
    Clock { format: String },
    /// CPU use, %.
    Cpu,
    /// Memory use, %.
    Memory,
    /// The system battery.
    Battery,
    /// The active Hyprland workspace. Fed by the Hyprland listener, never polled.
    Workspace,
    /// `sh -c command`'s first output line, every `interval_ms`. Both fields are the hash
    /// key, as for `Command`.
    Output { command: String, interval_ms: u32 },
    /// One app's playback streams. Every app probe is answered from one stream read.
    AppVolume(AppQuery),
    /// The media player: the wanted player (`playerctl --player` name), `None` for
    /// whichever plays. Read inline by the watcher, like volume.
    Player(Option<String>),
}

impl Probe {
    /// How often it's re-read while watched. Volume: 1 s; a command: its own, never
    /// under 1 s.
    pub fn interval(&self) -> Duration {
        match self {
            Probe::Volume(_) => Duration::from_secs(1),
            Probe::Command { interval_ms, .. } | Probe::Output { interval_ms, .. } => {
                Duration::from_millis((*interval_ms).max(1000) as u64)
            }
            // Only a fallback: the watcher schedules clocks on the minute or second.
            Probe::Clock { .. } => crate::live::CLOCK_CAP,
            Probe::Cpu => Duration::from_secs(1),
            Probe::Memory => Duration::from_secs(2),
            Probe::Battery => Duration::from_secs(5),
            // Never used: the Hyprland listener feeds it.
            Probe::Workspace => Duration::from_secs(1),
            Probe::AppVolume(_) => Duration::from_secs(1),
            Probe::Player(_) => Duration::from_secs(1),
        }
    }

    /// When to re-read after a change made elsewhere (`WatchCmd::Refresh`). A volume
    /// change made by the deck is read back at once instead.
    pub fn refresh_after(&self) -> &'static [Duration] {
        match self {
            Probe::Volume(_) => &VOLUME_REFRESH,
            Probe::Command { .. } | Probe::Output { .. } => &COMMAND_REFRESH,
            Probe::Player(_) => &PLAYER_REFRESH,
            // Nothing sends `Refresh` for an app: its own changes are read back at once.
            Probe::Clock { .. }
            | Probe::Cpu
            | Probe::Memory
            | Probe::Battery
            | Probe::Workspace
            | Probe::AppVolume(_) => &[],
        }
    }

    /// False for probes something else feeds (the Hyprland listener): `watched_probes`
    /// leaves them out.
    pub fn is_polled(&self) -> bool {
        !matches!(self, Probe::Workspace)
    }
}

/// What a toggle key shows. On the wire: `"off"`, `"on"`, `{"busy":"Loading 43%"}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Toggle {
    #[default]
    Off,
    On,
    /// The text is the state command's progress line, or a running command's latest line.
    Busy(String),
}

impl Toggle {
    pub fn is_off(&self) -> bool {
        matches!(self, Toggle::Off)
    }
}

/// What a probe read. It goes on the wire in later events, hence serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Value {
    /// A volume device's level, as the panel draws it (`audio::Level::to_panel`).
    Level(Level),
    /// A toggle key's state. A `Toggle` on a dial shows no level.
    Toggle(Toggle),
    /// A live text: the clock, the workspace, a command's line.
    Text(String),
    /// 0–100: CPU, memory.
    Percent(u8),
    /// The battery's charge, 0–100, and whether it is charging.
    Battery { percent: u8, charging: bool },
    /// One app's streams added up.
    App(AppLevel),
    /// The media player's track, nothing playing, or why it can't be read.
    Player(PlayerRead),
}

impl Value {
    /// Whether `newer` says the same as `self`, for "send / store only on change".
    /// Players compare with `player::same` (a moving position is not a change); all
    /// other values compare with `==`.
    pub fn same_as(&self, newer: &Value) -> bool {
        match (self, newer) {
            (Value::Player(Ok(a)), Value::Player(Ok(b))) => player::same(a.as_ref(), b.as_ref()),
            (a, b) => a == b,
        }
    }
}

/// Everything the panels show: the last value of each probe, and the panel brightness.
#[derive(Debug, Clone, Default)]
pub struct Values {
    pub probes: HashMap<Probe, Value>,
    /// 0–100.
    pub brightness: u8,
}

impl Values {
    /// The last known level of a volume device.
    pub fn level(&self, device: AudioDevice) -> Option<&Level> {
        match self.probes.get(&Probe::Volume(device))? {
            Value::Level(l) => Some(l),
            _ => None,
        }
    }

    /// The last read of an app's streams.
    pub fn app(&self, q: &AppQuery) -> Option<&AppLevel> {
        match self.probes.get(&Probe::AppVolume(q.clone()))? {
            Value::App(lv) => Some(lv),
            _ => None,
        }
    }
}

/// What the live key or panel for `live` shows now; `None` until a value arrives.
pub fn live_show(live: &Live, values: &Values) -> Option<LiveShow> {
    match values.probes.get(&live.probe())? {
        Value::Text(s) => Some(LiveShow { text: s.clone(), bar: None, charging: false }),
        Value::Percent(p) => Some(LiveShow { text: format!("{p}%"), bar: Some(*p as f32 / 100.0), charging: false }),
        Value::Battery { percent, charging } => {
            Some(LiveShow { text: format!("{percent}%"), bar: Some(*percent as f32 / 100.0), charging: *charging })
        }
        Value::Level(_) | Value::Toggle(_) | Value::App(_) | Value::Player(_) => None,
    }
}

/// The `$B1` value for the panel brightness.
pub fn brightness_level(value: u8) -> Level {
    Level { value: value as f32 / 100.0, text: format!("{value}%"), muted: false }
}

/// What dial `dial`'s strip panel shows. The one place that turns values into a panel:
/// the daemon, the configurator's preview and `snapshot` all call it. A dial that shows a
/// level that isn't known yet gets `level: None` (the panel draws a dash).
pub fn dial_status(dial: Option<&DialConfig>, values: &Values, pressed: bool) -> DialStatus {
    let mut panel = PanelStatus {
        live: dial.and_then(|d| d.live.as_ref()).and_then(|l| live_show(l, values)),
        ..PanelStatus::default()
    };
    match dial.and_then(DialConfig::level_source) {
        Some(LevelSource::Volume(dev)) => panel.level = values.level(dev).cloned(),
        Some(LevelSource::Brightness) => panel.level = Some(brightness_level(values.brightness)),
        Some(LevelSource::App) => {
            if let Some((d, q)) = dial.zip(dial.and_then(DialConfig::app_query)) {
                let (show, level) = app_show(d, &q, values.app(&q));
                panel.app = Some(show);
                panel.level = level;
            }
        }
        // Anchored at the read: no clock here, so the panel only changes with the value.
        Some(LevelSource::Player) => panel.media = media_show(dial, values),
        None => {}
    }
    DialStatus { pressed, panel }
}

/// What a media dial's panel shows: the track the dial's player probe last read, a
/// "nothing playing" or a note, or `None` before the first read.
pub fn media_show(dial: Option<&DialConfig>, values: &Values) -> Option<MediaShow> {
    let wanted = Probe::Player(dial.and_then(DialConfig::media_player).map(str::to_string));
    match values.probes.get(&wanted) {
        Some(Value::Player(Ok(Some(t)))) => Some(t.show()),
        Some(Value::Player(Ok(None))) => Some(MediaShow::default()),
        Some(Value::Player(Err(note))) => Some(MediaShow { note: note.clone(), ..MediaShow::default() }),
        _ => None,
    }
}

/// The player a media action names: its own `player`, trimmed, when not blank; else, for an
/// action on a dial, the player the dial's panel follows (`DialConfig::media_player`), so
/// the panel and the dial's actions follow the same player. `None`: whatever plays.
pub fn media_wanted<'a>(player: Option<&'a str>, dial: Option<&'a DialConfig>) -> Option<&'a str> {
    player.map(str::trim).filter(|p| !p.is_empty()).or_else(|| dial.and_then(DialConfig::media_player))
}

/// Where a media action goes, from [`media_target`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MediaSend {
    /// Send it to this `playerctl --player` instance or name; `None`: untargeted
    /// (Omarchy's own command, or playerctl's choice).
    To(Option<String>),
    /// A seek while the player probe, watched, read "nothing playing": dropped.
    NothingToSeek,
}

/// Where a media action goes. The one targeting rule; the daemon follows it for every
/// media action, and the offline path (no values, nothing watched) gets rule 3.
///
/// 1. `wanted` (from [`media_wanted`]) names the player: the action's own, or its dial's.
/// 2. If `Probe::Player(wanted)` is in `watched` and has read a track, the action goes to
///    the instance that read shows (the one its panel or key shows). A value of a probe no
///    longer watched is kept for faces but never targeted: it was read on another page and
///    may name an instance that's gone.
/// 3. Otherwise it goes to `wanted` as given, untargeted when `None`.
/// 4. A seek (`seek`) is dropped only when that watched probe read "nothing playing"; with
///    nothing known it is sent per 2–3, as without the daemon.
pub fn media_target(values: &Values, watched: &[Probe], wanted: Option<&str>, seek: bool) -> MediaSend {
    let probe = Probe::Player(wanted.map(str::to_string));
    let read = if watched.contains(&probe) { values.probes.get(&probe) } else { None };
    match read {
        Some(Value::Player(Ok(Some(t)))) if !t.player.is_empty() => MediaSend::To(Some(t.player.clone())),
        Some(Value::Player(Ok(None))) if seek => MediaSend::NothingToSeek,
        _ => MediaSend::To(wanted.map(str::to_string)),
    }
}

/// The face and level of an app volume panel. `lv` is the watcher's value, `None`
/// before the first read. A chosen app keeps the dial's label as its title; a focused
/// dial names the app it would change (the label only when there is none).
pub fn app_show(dial: &DialConfig, q: &AppQuery, lv: Option<&AppLevel>) -> (AppShow, Option<Level>) {
    let label = dial.label.trim();
    let or_label = |x: &str| if label.is_empty() { x.to_string() } else { label.to_string() };
    let level = lv.and_then(|lv| lv.level).map(|l| l.to_panel());
    let dimmed = level.is_none();
    let show = match (q, lv) {
        (AppQuery::Named(app), None) => {
            AppShow { title: or_label(&crate::audio::display_app(app)), icon: None, dimmed }
        }
        (AppQuery::Named(_), Some(lv)) => AppShow { title: or_label(&lv.name), icon: lv.icon.clone(), dimmed },
        (AppQuery::Focused, Some(lv)) if lv.level.is_some() => {
            AppShow { title: lv.name.clone(), icon: lv.icon.clone(), dimmed }
        }
        (AppQuery::Focused, Some(lv)) if !lv.name.trim().is_empty() => {
            AppShow { title: lv.name.clone(), icon: None, dimmed }
        }
        (AppQuery::Focused, _) => AppShow { title: or_label("Focused app"), icon: None, dimmed },
    };
    (show, level)
}

/// The level a volume change will leave, guessed from the current one, so the bar moves
/// at once; the watcher's read-back a moment later corrects it. A mute toggles; a change
/// adds `change` percent, clamped to 0–100, and unmutes.
pub fn guess_volume(cur: &Level, change: i32, mute: bool) -> Level {
    let pct = (cur.value * 100.0).round() as i32;
    if mute {
        let muted = !cur.muted;
        Level { value: cur.value, text: if muted { "Muted".into() } else { format!("{pct}%") }, muted }
    } else {
        let p = (pct + change).clamp(0, 100);
        Level { value: p as f32 / 100.0, text: format!("{p}%"), muted: false }
    }
}

/// Whether `dial`'s panel shows `probe`, so a new value for it repaints the panel.
pub fn shows(dial: Option<&DialConfig>, probe: &Probe) -> bool {
    if let Some(l) = dial.and_then(|d| d.live.as_ref()) {
        return l.probe() == *probe;
    }
    match (dial.and_then(DialConfig::level_source), probe) {
        (Some(LevelSource::Volume(dev)), Probe::Volume(p)) => dev == *p,
        (Some(LevelSource::App), Probe::AppVolume(q)) => dial.and_then(DialConfig::app_query).as_ref() == Some(q),
        (Some(LevelSource::Player), Probe::Player(w)) => dial.and_then(DialConfig::media_player) == w.as_deref(),
        _ => false,
    }
}

/// The ONE builder of a key's status from the watcher's values, like `dial_status`. It
/// covers what `Values` knows (a command's state, a mute); the two per-(page, key) things
/// `Values` can't key (a press toggle's flip and a running job) are laid over it by the
/// daemon's `key_state`.
pub fn key_status(key: Option<&KeyConfig>, values: &Values, pressed: bool) -> KeyStatus {
    let state = match key.filter(|k| k.on.is_some()).and_then(KeyConfig::state_probe) {
        Some(p) => match values.probes.get(&p) {
            Some(Value::Toggle(t)) => t.clone(),
            Some(Value::Level(l)) if l.muted => Toggle::On, // "on" for a mute key is muted
            // "on" for a play/pause key is playing: its on face shows the pause glyph.
            Some(Value::Player(Ok(Some(t)))) if t.status == PlayStatus::Playing => Toggle::On,
            _ => Toggle::Off,
        },
        None => Toggle::Off,
    };
    let live = key.and_then(|k| k.live.as_ref()).and_then(|l| live_show(l, values));
    KeyStatus { pressed, state, live }
}

/// A state command's exit code: 0 on, 75 busy (with its first output line as the
/// progress), anything else (or killed by a signal) off.
pub fn toggle_from_exit(code: Option<i32>, first_line: Option<String>) -> Toggle {
    match code {
        Some(0) => Toggle::On,
        Some(75) => Toggle::Busy(first_line.unwrap_or_default()),
        _ => Toggle::Off,
    }
}

/// 126/127: not executable / not found. Off, but worth one warning (likely a typo).
pub fn is_missing(code: Option<i32>) -> bool {
    matches!(code, Some(126) | Some(127))
}

/// The first explicit `NN%` in a busy text, clamped to 100 (`"shard 2/4 43%"` → 43,
/// `"43.5 %"` → 43, `"120%"` → 100). Only a number a `%` follows counts, so `"1/4"`,
/// `"3 of 7"` or `"v2"` give no bar.
pub fn progress(text: &str) -> Option<u8> {
    let parts: Vec<&str> = text.split('%').collect();
    // Every part but the last is followed by a `%`.
    parts[..parts.len() - 1].iter().find_map(|part| number_at_end(part))
}

/// The number (digits, at most one `.`) a text ends with, ignoring trailing spaces,
/// clamped to 0–100.
fn number_at_end(part: &str) -> Option<u8> {
    let t = part.trim_end();
    let b = t.as_bytes();
    let end = b.len();
    let mut start = end;
    let mut dot = false;
    while start > 0 {
        match b[start - 1] {
            b'0'..=b'9' => start -= 1,
            b'.' if !dot => {
                dot = true;
                start -= 1;
            }
            _ => break,
        }
    }
    if start < end && b[start..].iter().any(u8::is_ascii_digit) {
        t[start..].parse::<f32>().ok().map(|n| n.clamp(0.0, 100.0) as u8)
    } else {
        None
    }
}

/// Everything the watcher should read while `page` is shown on a deck with `n_dials` dials
/// and `n_keys` keys, each once: per dial in dial order its volume level, then its live
/// value; then per key of the page the deck has, its toggle's state command (or the mute it
/// follows), then its live value. A key past `n_keys` (a config made for a bigger deck) is
/// not polled. Probes something else feeds (the workspace) are left out. No deck: both are
/// 0, so nothing is. An app volume dial's level is its app's streams.
pub fn watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize, n_keys: u8) -> Vec<Probe> {
    let mut out = Vec::new();
    let mut add = |p: Probe| {
        if p.is_polled() && !out.contains(&p) {
            out.push(p);
        }
    };
    for i in 0..n_dials {
        let dial = cfg.dial(i as u8);
        match dial.and_then(DialConfig::level_source) {
            Some(LevelSource::Volume(dev)) => add(Probe::Volume(dev)),
            Some(LevelSource::App) => {
                if let Some(q) = dial.and_then(DialConfig::app_query) {
                    add(Probe::AppVolume(q));
                }
            }
            Some(LevelSource::Player) => {
                add(Probe::Player(dial.and_then(DialConfig::media_player).map(str::to_string)))
            }
            Some(LevelSource::Brightness) | None => {}
        }
        if let Some(l) = dial.and_then(|d| d.live.as_ref()) {
            add(l.probe());
        }
    }
    if let Some(ks) = cfg.page(page) {
        for k in ks.iter().filter(|k| k.index < n_keys) {
            if let Some(p) = k.state_probe() {
                add(p);
            }
            if let Some(l) = &k.live {
                add(l.probe());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;
    use crate::render::KeyStatus;
    use std::os::unix::process::ExitStatusExt;
    use std::thread;
    use std::time::Instant;

    fn volume_dial(index: u8, device: AudioDevice) -> DialConfig {
        DialConfig { turn_right: Action::Volume { device, change: 5, mute: false }, ..DialConfig::new(index) }
    }

    fn brightness_dial(index: u8) -> DialConfig {
        DialConfig { turn_right: Action::Brightness { change: 5, set: None }, ..DialConfig::new(index) }
    }

    #[test]
    fn dial_status_volume_known_and_unknown() {
        let dial = volume_dial(0, AudioDevice::Output);
        let mut values = Values::default();
        // Not read yet: the panel shows a dash.
        assert_eq!(dial_status(Some(&dial), &values, false).panel.level, None);
        let forty = Level { value: 0.4, text: "40%".into(), muted: false };
        values.probes.insert(Probe::Volume(AudioDevice::Output), Value::Level(forty));
        let st = dial_status(Some(&dial), &values, true);
        assert!(st.pressed);
        assert_eq!(st.panel.level.unwrap().text, "40%");
        // The input's level doesn't leak into an output dial.
        let mic = volume_dial(1, AudioDevice::Input);
        assert_eq!(dial_status(Some(&mic), &values, false).panel.level, None);
    }

    fn app_dial(index: u8, app: &str, focused: bool) -> DialConfig {
        DialConfig {
            turn_right: Action::AppVolume { app: app.into(), focused, change: 5, mute: false },
            ..DialConfig::new(index)
        }
    }

    fn app_level(name: &str, level: Option<u8>) -> AppLevel {
        AppLevel {
            name: name.into(),
            key: name.to_lowercase(),
            icon: Some("/i.svg".into()),
            icon_names: vec![],
            level: level.map(|p| crate::audio::Level { percent: p, muted: false }),
            ids: if level.is_some() { vec![87] } else { vec![] },
        }
    }

    #[test]
    fn app_show_cases() {
        let named = AppQuery::Named("spotify".into());
        let plain = app_dial(0, "spotify", false);
        let labelled = DialConfig { label: " Music ".into(), ..plain.clone() };
        let show = |title: &str, icon: Option<&str>, dimmed| AppShow {
            title: title.into(),
            icon: icon.map(Into::into),
            dimmed,
        };
        let forty = Some(crate::audio::Level { percent: 40, muted: false }.to_panel());
        // Named, playing.
        assert_eq!(
            app_show(&plain, &named, Some(&app_level("Spotify", Some(40)))),
            (show("Spotify", Some("/i.svg"), false), forty.clone())
        );
        assert_eq!(app_show(&labelled, &named, Some(&app_level("Spotify", Some(40)))).0.title, "Music");
        // Named, no stream.
        assert_eq!(
            app_show(&plain, &named, Some(&app_level("Spotify", None))),
            (show("Spotify", Some("/i.svg"), true), None)
        );
        assert_eq!(app_show(&labelled, &named, Some(&app_level("Spotify", None))).0.title, "Music");
        // Named, no value yet.
        assert_eq!(app_show(&plain, &named, None), (show("Spotify", None, true), None));
        assert_eq!(app_show(&labelled, &named, None).0.title, "Music");
        // Focused, playing: the app's name, the label ignored.
        let focused = DialConfig { label: "Here".into(), ..app_dial(0, "", true) };
        assert_eq!(
            app_show(&focused, &AppQuery::Focused, Some(&app_level("Firefox", Some(40)))),
            (show("Firefox", Some("/i.svg"), false), forty)
        );
        // Focused, no audio: the window's class, else the label, else "Focused app".
        let mut silent = app_level("Alacritty", None);
        assert_eq!(app_show(&focused, &AppQuery::Focused, Some(&silent)), (show("Alacritty", None, true), None));
        silent.name = String::new();
        assert_eq!(app_show(&focused, &AppQuery::Focused, Some(&silent)).0, show("Here", None, true));
        assert_eq!(
            app_show(&app_dial(0, "", true), &AppQuery::Focused, Some(&silent)).0,
            show("Focused app", None, true)
        );
        // Focused, no value yet.
        assert_eq!(app_show(&focused, &AppQuery::Focused, None), (show("Here", None, true), None));
        assert_eq!(app_show(&app_dial(0, "", true), &AppQuery::Focused, None).0.title, "Focused app");
    }

    #[test]
    fn dial_status_app_named() {
        let dial = DialConfig {
            turn_right: Action::AppVolume { app: "spotify".into(), focused: false, change: 5, mute: false },
            ..DialConfig::new(0)
        };
        let q = AppQuery::Named("spotify".into());
        let mut values = Values::default();
        // No value yet: dimmed, the app's display name, a dash.
        let st = dial_status(Some(&dial), &values, false).panel;
        assert_eq!(st.app, Some(AppShow { title: "Spotify".into(), icon: None, dimmed: true }));
        assert_eq!(st.level, None);
        // Playing: lit, its level.
        let lv = AppLevel {
            name: "Spotify".into(),
            key: "spotify".into(),
            icon: None,
            icon_names: vec![],
            level: Some(crate::audio::Level { percent: 40, muted: false }),
            ids: vec![87],
        };
        values.probes.insert(Probe::AppVolume(q), Value::App(lv));
        let st = dial_status(Some(&dial), &values, false).panel;
        assert!(!st.app.unwrap().dimmed);
        assert_eq!(st.level.unwrap().text, "40%");
        // Another dial type has no app.
        assert_eq!(dial_status(Some(&volume_dial(0, AudioDevice::Output)), &values, false).panel.app, None);
    }

    #[test]
    fn shows_app_matches_its_query() {
        let spotify = app_dial(0, "spotify", false);
        assert!(shows(Some(&spotify), &Probe::AppVolume(AppQuery::Named("spotify".into()))));
        assert!(!shows(Some(&spotify), &Probe::AppVolume(AppQuery::Named("mpv".into()))));
        assert!(!shows(Some(&spotify), &Probe::AppVolume(AppQuery::Focused)));
        assert!(!shows(Some(&spotify), &Probe::Volume(AudioDevice::Output)));
        assert!(shows(Some(&app_dial(1, "", true)), &Probe::AppVolume(AppQuery::Focused)));
        assert!(!shows(Some(&volume_dial(0, AudioDevice::Output)), &Probe::AppVolume(AppQuery::Focused)));
    }

    #[test]
    fn watched_probes_adds_app_dials() {
        let cfg = Config {
            dials: vec![
                app_dial(0, "spotify", false),
                app_dial(1, "", true),
                app_dial(2, "spotify", false),
                app_dial(3, "", true),
            ],
            ..Config::default()
        };
        assert_eq!(
            watched_probes(&cfg, None, 4, 32),
            vec![Probe::AppVolume(AppQuery::Named("spotify".into())), Probe::AppVolume(AppQuery::Focused)]
        );
        // No app set: nothing to read.
        let cfg = Config { dials: vec![app_dial(0, " ", false)], ..Config::default() };
        assert_eq!(watched_probes(&cfg, None, 4, 32), vec![]);
    }

    #[test]
    fn dial_status_brightness_and_plain() {
        let values = Values { brightness: 70, ..Values::default() };
        assert_eq!(dial_status(Some(&brightness_dial(0)), &values, false).panel.level, Some(brightness_level(70)));
        assert_eq!(dial_status(Some(&DialConfig::new(0)), &values, false).panel, PanelStatus::default());
        assert_eq!(dial_status(None, &values, false).panel, PanelStatus::default());
    }

    #[test]
    fn watched_probes_current_page_dials_and_dedup() {
        let cfg = Config::parse(
            r#"
[[key]]
index = 0
[key.action]
type = "volume"
mute = true
[key.on]

[[key]]
index = 1
[key.on]
state = "true"

[[key]]
index = 2
[key.on]
state = "true"

[[page]]
name = "Other"
[[page.key]]
index = 0
[page.key.on]
state = "false"

[[dial]]
index = 0
[dial.turn_right]
type = "volume"
change = 5
"#,
        )
        .unwrap();
        let cmd = |c: &str| Probe::Command { command: c.into(), interval_ms: 2000 };
        // The mute key's probe (the output's) is the dial's: deduped. The two "true" keys
        // share one probe.
        assert_eq!(watched_probes(&cfg, None, 4, 32), vec![Probe::Volume(AudioDevice::Output), cmd("true")]);
        // The other page watches its own keys, not the home page's.
        assert_eq!(watched_probes(&cfg, Some("Other"), 4, 32), vec![Probe::Volume(AudioDevice::Output), cmd("false")]);
        assert_eq!(watched_probes(&cfg, Some("Other"), 0, 32), vec![cmd("false")], "no dials, no dial probes");
    }

    #[test]
    fn watched_probes_skip_keys_the_deck_lacks() {
        // A key 10 made on an XL, on a Plus (8 keys): its command is not polled.
        let cfg = Config::parse(
            "[[key]]\nindex = 1\n[key.on]\nstate = \"a\"\n\n[[key]]\nindex = 10\n[key.on]\nstate = \"b\"\n",
        )
        .unwrap();
        let cmd = |c: &str| Probe::Command { command: c.into(), interval_ms: 2000 };
        assert_eq!(watched_probes(&cfg, None, 4, 8), vec![cmd("a")]);
        assert_eq!(watched_probes(&cfg, None, 4, 32), vec![cmd("a"), cmd("b")]);
        assert_eq!(watched_probes(&cfg, None, 0, 0), vec![], "no deck: nothing");
    }

    #[test]
    fn watched_probes_in_dial_order_each_once() {
        let cfg = Config {
            dials: vec![
                volume_dial(0, AudioDevice::Input),
                brightness_dial(1),
                volume_dial(2, AudioDevice::Output),
                volume_dial(3, AudioDevice::Input),
            ],
            ..Config::default()
        };
        let both = vec![Probe::Volume(AudioDevice::Input), Probe::Volume(AudioDevice::Output)];
        assert_eq!(watched_probes(&cfg, None, 4, 32), both);
        // Dials aren't per page.
        assert_eq!(watched_probes(&cfg, Some("Media"), 4, 32), both);
        // Only the dials the deck has; a deck without dials watches nothing.
        assert_eq!(watched_probes(&cfg, None, 2, 32), vec![Probe::Volume(AudioDevice::Input)]);
        assert_eq!(watched_probes(&cfg, None, 0, 32), vec![]);
    }

    #[test]
    fn guess_volume_moves_and_mutes() {
        let fifty = Level { value: 0.5, text: "50%".into(), muted: false };
        assert_eq!(guess_volume(&fifty, 15, false), Level { value: 0.65, text: "65%".into(), muted: false });
        assert_eq!(guess_volume(&fifty, -80, false).text, "0%");
        assert_eq!(guess_volume(&fifty, 80, false).text, "100%");
        let muted = guess_volume(&fifty, 0, true);
        assert_eq!(muted, Level { value: 0.5, text: "Muted".into(), muted: true });
        assert_eq!(guess_volume(&muted, 0, true), fifty);
    }

    #[test]
    fn shows_matches_the_device() {
        let out = volume_dial(0, AudioDevice::Output);
        assert!(shows(Some(&out), &Probe::Volume(AudioDevice::Output)));
        assert!(!shows(Some(&out), &Probe::Volume(AudioDevice::Input)));
        assert!(!shows(Some(&brightness_dial(1)), &Probe::Volume(AudioDevice::Output)));
        assert!(!shows(None, &Probe::Volume(AudioDevice::Output)));
    }

    #[test]
    fn command_probe_uses_its_own_interval_never_under_one_second() {
        let p = Probe::Command { command: "true".into(), interval_ms: 2000 };
        assert_eq!(p.interval(), Duration::from_millis(2000));
        assert_eq!(Probe::Command { command: "true".into(), interval_ms: 0 }.interval(), Duration::from_secs(1));
        assert_eq!(p.refresh_after(), &[Duration::from_millis(300), Duration::from_millis(1200)]);
        // The same command at two intervals is two probes.
        let q = Probe::Command { command: "true".into(), interval_ms: 5000 };
        assert_ne!(p, q);
    }

    #[test]
    fn exit_codes_map_to_toggles() {
        assert_eq!(toggle_from_exit(Some(0), None), Toggle::On);
        assert_eq!(toggle_from_exit(Some(75), Some("Loading 43%".into())), Toggle::Busy("Loading 43%".into()));
        assert_eq!(toggle_from_exit(Some(75), None), Toggle::Busy(String::new()));
        assert_eq!(toggle_from_exit(Some(1), None), Toggle::Off);
        assert_eq!(toggle_from_exit(None, None), Toggle::Off, "killed by a signal");
        assert!(is_missing(Some(126)) && is_missing(Some(127)));
        assert!(!is_missing(Some(0)) && !is_missing(Some(1)) && !is_missing(None));
    }

    #[test]
    fn progress_takes_the_first_explicit_percentage() {
        assert_eq!(progress("Loading 43%"), Some(43));
        assert_eq!(progress("shard 2/4 43%"), Some(43));
        assert_eq!(progress("43.5 %"), Some(43));
        assert_eq!(progress("120%"), Some(100));
        assert_eq!(progress("10% then 20%"), Some(10));
        assert_eq!(progress("50 % done"), Some(50));
        assert_eq!(progress("Loading 43% step 5"), Some(43));
        // Numbers no `%` follows are not progress.
        assert_eq!(progress("1/4"), None);
        assert_eq!(progress("Downloading 3 of 7"), None);
        assert_eq!(progress("v2"), None);
        assert_eq!(progress("no number here"), None);
        assert_eq!(progress("%"), None);
        assert_eq!(progress(""), None);
    }

    #[test]
    fn value_wire_names_are_snake_case() {
        assert_eq!(serde_json::to_string(&Value::Toggle(Toggle::On)).unwrap(), r#"{"toggle":"on"}"#);
        let level = Value::Level(Level { value: 0.5, text: "50%".into(), muted: false });
        assert_eq!(serde_json::to_string(&level).unwrap(), r#"{"level":{"value":0.5,"text":"50%","muted":false}}"#);
    }

    #[test]
    fn key_status_wire_shape() {
        assert_eq!(serde_json::to_string(&KeyStatus::default()).unwrap(), "{}");
        let on = KeyStatus { state: Toggle::On, ..KeyStatus::default() };
        assert_eq!(serde_json::to_string(&on).unwrap(), "{\"state\":\"on\"}");
        let busy = KeyStatus { state: Toggle::Busy("x".into()), ..KeyStatus::default() };
        assert_eq!(serde_json::to_string(&busy).unwrap(), "{\"state\":{\"busy\":\"x\"}}");
        let pressed = KeyStatus { pressed: true, ..KeyStatus::default() };
        assert_eq!(serde_json::to_string(&pressed).unwrap(), "{\"pressed\":true}");
        let back: KeyStatus = serde_json::from_str("{}").unwrap();
        assert_eq!(back, KeyStatus::default());
    }

    #[test]
    fn job_reports_exit_and_lines() {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "echo a; exit 75"]);
        let mut job = crate::proc::Job::spawn(c).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(status.code(), Some(75));
        assert_eq!(job.lines().collect::<Vec<_>>(), vec!["a"]);
    }

    #[test]
    fn job_with_background_child_still_finishes() {
        // A background grandchild holds the pipe open; the GRACE must end the wait.
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 5 & echo x"]);
        let mut job = crate::proc::Job::spawn(c).unwrap();
        // Well under the `sleep 5`, with a margin for a loaded machine.
        let deadline = Instant::now() + crate::proc::GRACE + Duration::from_secs(3);
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success());
    }

    #[test]
    fn kill_group_kills_a_pipeline() {
        let mut c = std::process::Command::new("sh");
        c.args(["-c", "sleep 30 | cat"]);
        let mut job = crate::proc::Job::spawn(c).unwrap();
        thread::sleep(Duration::from_millis(100)); // let the pipeline start
        job.kill_group();
        let deadline = Instant::now() + Duration::from_secs(2);
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(10));
        };
        assert!(status.signal().is_some());
    }

    #[test]
    fn live_show_forms() {
        let mut values = Values::default();
        assert_eq!(live_show(&Live::Cpu, &values), None, "nothing read yet");
        values.probes.insert(Probe::Clock { format: "%H".into() }, Value::Text("12".into()));
        values.probes.insert(Probe::Cpu, Value::Percent(42));
        values.probes.insert(Probe::Battery, Value::Battery { percent: 80, charging: true });
        assert_eq!(
            live_show(&Live::Clock { format: "%H".into() }, &values),
            Some(LiveShow { text: "12".into(), bar: None, charging: false })
        );
        assert_eq!(
            live_show(&Live::Cpu, &values),
            Some(LiveShow { text: "42%".into(), bar: Some(0.42), charging: false })
        );
        assert_eq!(
            live_show(&Live::Battery, &values),
            Some(LiveShow { text: "80%".into(), bar: Some(0.8), charging: true })
        );
    }

    #[test]
    fn workspace_is_not_polled() {
        assert!(!Probe::Workspace.is_polled());
        assert!(Probe::Cpu.is_polled() && Probe::Clock { format: "%H".into() }.is_polled());
        assert!(Probe::Output { command: "x".into(), interval_ms: 1000 }.is_polled());
        assert_eq!(Probe::Memory.interval(), Duration::from_secs(2));
        assert_eq!(Probe::Battery.interval(), Duration::from_secs(5));
        assert!(Probe::Cpu.refresh_after().is_empty());
    }

    #[test]
    fn value_live_wire_names() {
        assert_eq!(serde_json::to_string(&Value::Text("a".into())).unwrap(), r#"{"text":"a"}"#);
        assert_eq!(serde_json::to_string(&Value::Percent(5)).unwrap(), r#"{"percent":5}"#);
        assert_eq!(
            serde_json::to_string(&Value::Battery { percent: 5, charging: false }).unwrap(),
            r#"{"battery":{"percent":5,"charging":false}}"#
        );
    }

    #[test]
    fn watched_probes_only_current_page() {
        let cfg = Config::parse(
            r#"
[[key]]
index = 0
live = { source = "cpu" }

[[page]]
name = "Other"
[[page.key]]
index = 0
live = { source = "memory" }

[[dial]]
index = 0
live = { source = "workspace" }
"#,
        )
        .unwrap();
        let home = watched_probes(&cfg, None, 4, 15);
        assert!(home.contains(&Probe::Cpu));
        assert!(!home.contains(&Probe::Memory));
        assert!(!home.contains(&Probe::Workspace), "fed by the listener, never polled");
        assert!(watched_probes(&cfg, Some("Other"), 4, 15).contains(&Probe::Memory));
    }

    #[test]
    fn watched_probes_includes_dial_live() {
        let cfg = Config {
            dials: vec![
                DialConfig { live: Some(Live::Battery), ..volume_dial(0, AudioDevice::Output) },
                volume_dial(1, AudioDevice::Input),
                DialConfig { live: Some(Live::Battery), ..DialConfig::new(2) },
            ],
            ..Config::default()
        };
        // Dial 0's live value wins over its volume action; dial 2's battery is deduped.
        assert_eq!(watched_probes(&cfg, None, 4, 15), vec![Probe::Battery, Probe::Volume(AudioDevice::Input)]);
        assert_eq!(watched_probes(&cfg, None, 0, 15), vec![]);
    }

    #[test]
    fn shows_live_dial() {
        let dial = DialConfig { live: Some(Live::Battery), ..volume_dial(0, AudioDevice::Output) };
        assert!(shows(Some(&dial), &Probe::Battery));
        assert!(!shows(Some(&dial), &Probe::Volume(AudioDevice::Output)));
    }

    #[test]
    fn key_status_fills_live() {
        let mut values = Values::default();
        let k = KeyConfig { live: Some(Live::Workspace), ..KeyConfig::new(0) };
        assert_eq!(key_status(Some(&k), &values, false).live, None);
        values.probes.insert(Probe::Workspace, Value::Text("3".into()));
        let st = key_status(Some(&k), &values, true);
        assert_eq!(st.live, Some(LiveShow { text: "3".into(), bar: None, charging: false }));
        assert!(st.pressed);
        assert_eq!(key_status(Some(&KeyConfig::new(0)), &values, false).live, None);
        let d = DialConfig { live: Some(Live::Workspace), ..DialConfig::new(0) };
        assert_eq!(dial_status(Some(&d), &values, false).panel.live.unwrap().text, "3");
    }

    fn media_track(status: player::PlayStatus, position_ms: u64, read_ms: u64) -> player::Track {
        player::Track {
            player: "spotify".into(),
            name: "spotify".into(),
            status,
            position_ms,
            length_ms: Some(225_000),
            title: "Song".into(),
            artist: "Band".into(),
            art: None,
            art_img: None,
            read_ms,
        }
    }

    #[test]
    fn dial_status_shows_paused_track() {
        let dial = DialConfig {
            label: "Media".into(),
            press: Action::Media { command: crate::MediaCommand::PlayPause, seek: 0, player: None },
            ..DialConfig::new(0)
        };
        let track = player::Track {
            player: "spotify".into(),
            name: "spotify".into(),
            status: player::PlayStatus::Paused,
            position_ms: 83_000,
            length_ms: Some(225_000),
            title: "Song".into(),
            artist: "Band".into(),
            art: None,
            art_img: None,
            read_ms: 1_000,
        };
        let mut values = Values::default();
        values.probes.insert(Probe::Player(None), Value::Player(Ok(Some(track))));
        let st = dial_status(Some(&dial), &values, false);
        let m = st.panel.media.expect("a media panel");
        assert_eq!((m.title.as_str(), m.artist.as_str(), m.playing), ("Song", "Band", false));
        assert_eq!((m.position_ms, m.as_of_ms), (83_000, 1_000), "anchored at the read, not advanced");
        assert!(st.panel.level.is_none());
        assert!(shows(Some(&dial), &Probe::Player(None)));
        assert!(!shows(Some(&dial), &Probe::Player(Some("mpv".into()))));
        // No value yet: no media model, the renderer draws the dashes.
        assert!(dial_status(Some(&dial), &Values::default(), false).panel.media.is_none());
        // Nothing playing, and no playerctl.
        values.probes.insert(Probe::Player(None), Value::Player(Ok(None)));
        assert_eq!(dial_status(Some(&dial), &values, false).panel.media, Some(MediaShow::default()));
        values.probes.insert(Probe::Player(None), Value::Player(Err("Needs playerctl".into())));
        assert_eq!(dial_status(Some(&dial), &values, false).panel.media.unwrap().note, "Needs playerctl");
        // Other dials carry no media.
        assert!(dial_status(Some(&volume_dial(0, AudioDevice::Output)), &values, false).panel.media.is_none());
    }

    #[test]
    fn value_same_as_player_and_others() {
        let l = |v: f32| Value::Level(Level { value: v, text: String::new(), muted: false });
        assert!(l(0.5).same_as(&l(0.5)));
        assert!(!l(0.5).same_as(&l(0.6)), "a level change is a change");
        let playing = |pos, read| Value::Player(Ok(Some(media_track(player::PlayStatus::Playing, pos, read))));
        assert!(playing(10_000, 1_000).same_as(&playing(11_000, 2_000)), "a playing tick is not a change");
        assert!(playing(10_000, 1_000) != playing(11_000, 2_000), "though == says it is");
        assert!(!playing(10_000, 1_000).same_as(&playing(60_000, 2_000)), "a seek is");
        assert!(!playing(10_000, 1_000).same_as(&Value::Player(Ok(None))));
        assert!(Value::Player(Ok(None)).same_as(&Value::Player(Ok(None))));
        assert!(Value::Player(Err("x".into())).same_as(&Value::Player(Err("x".into()))));
        assert!(!Value::Player(Err("x".into())).same_as(&Value::Player(Ok(None))));
    }

    #[test]
    fn media_target_prefers_shown_player() {
        use MediaSend::*;
        let mut values = Values::default();
        let any = Probe::Player(None);
        let spot = Probe::Player(Some("spotify".into()));
        let watched = [any.clone(), spot.clone()];
        assert_eq!(media_target(&values, &watched, None, false), To(None), "nothing known: untargeted");
        assert_eq!(media_target(&values, &watched, Some("spotify"), false), To(Some("spotify".into())));
        let mut t = media_track(player::PlayStatus::Playing, 0, 0);
        t.player = "chromium.instance4821".into();
        values.probes.insert(any.clone(), Value::Player(Ok(Some(t.clone()))));
        assert_eq!(media_target(&values, &watched, None, false), To(Some("chromium.instance4821".into())));
        assert_eq!(media_target(&values, &watched, None, true), To(Some("chromium.instance4821".into())));
        // A pinned dial reads its own probe.
        assert_eq!(media_target(&values, &watched, Some("spotify"), false), To(Some("spotify".into())));
        t.player = "spotify.instance2".into();
        values.probes.insert(spot, Value::Player(Ok(Some(t))));
        assert_eq!(media_target(&values, &watched, Some("spotify"), false), To(Some("spotify.instance2".into())));
        values.probes.insert(any, Value::Player(Ok(None)));
        assert_eq!(media_target(&values, &watched, None, false), To(None), "nothing playing: untargeted");
        assert_eq!(media_target(&values, &watched, None, true), NothingToSeek);
    }

    // A value read on another page (its probe no longer watched) is never the target.
    #[test]
    fn media_target_ignores_a_probe_no_longer_watched() {
        let mut values = Values::default();
        let mut t = media_track(player::PlayStatus::Playing, 0, 0);
        t.player = "chromium.instance4821".into();
        values.probes.insert(Probe::Player(None), Value::Player(Ok(Some(t))));
        assert_eq!(media_target(&values, &[], None, false), MediaSend::To(None));
        values.probes.insert(Probe::Player(None), Value::Player(Ok(None)));
        assert_eq!(media_target(&values, &[], None, true), MediaSend::To(None), "and doesn't drop a seek");
    }

    // A seek key with nothing watching the player seeks untargeted.
    #[test]
    fn media_seek_with_nothing_watched_is_sent_untargeted() {
        let values = Values::default();
        assert_eq!(media_target(&values, &[], None, true), MediaSend::To(None));
        assert_eq!(media_target(&values, &[Probe::Player(None)], None, true), MediaSend::To(None), "not read yet");
        assert_eq!(media_target(&values, &[], Some("mpv"), true), MediaSend::To(Some("mpv".into())));
    }

    // A dial's action with no player follows the dial's panel.
    #[test]
    fn media_wanted_falls_back_to_the_dial() {
        let dial = DialConfig {
            turn_right: Action::Media { command: crate::MediaCommand::Next, seek: 5, player: Some("spotify".into()) },
            press: Action::Media { command: crate::MediaCommand::PlayPause, seek: 0, player: None },
            ..DialConfig::new(0)
        };
        assert_eq!(media_wanted(None, Some(&dial)), Some("spotify"));
        assert_eq!(media_wanted(Some("  "), Some(&dial)), Some("spotify"));
        assert_eq!(media_wanted(Some(" mpv "), Some(&dial)), Some("mpv"));
        assert_eq!(media_wanted(None, None), None, "a key: whatever plays");
        assert_eq!(media_wanted(Some(" mpv "), None), Some("mpv"));
    }

    #[test]
    fn watched_probes_include_media_dials() {
        let media = |index: u8, player: Option<&str>| DialConfig {
            press: Action::Media { command: crate::MediaCommand::PlayPause, seek: 0, player: player.map(Into::into) },
            ..DialConfig::new(index)
        };
        let cfg = Config {
            dials: vec![
                media(0, None),
                volume_dial(1, AudioDevice::Output),
                media(2, Some(" spotify ")),
                media(3, None),
            ],
            ..Config::default()
        };
        assert_eq!(
            watched_probes(&cfg, None, 4, 8),
            vec![Probe::Player(None), Probe::Volume(AudioDevice::Output), Probe::Player(Some("spotify".into())),]
        );
    }

    #[test]
    fn play_pause_key_follows_player() {
        let k = KeyConfig {
            action: Action::Media { command: crate::MediaCommand::PlayPause, seek: 0, player: None },
            on: Some(crate::OnFace { label: Some("Pause".into()), ..Default::default() }),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.state_probe(), Some(Probe::Player(None)));
        assert!(!k.is_press_toggle());
        let mut values = Values::default();
        assert_eq!(key_status(Some(&k), &values, false).state, Toggle::Off);
        values
            .probes
            .insert(Probe::Player(None), Value::Player(Ok(Some(media_track(player::PlayStatus::Playing, 0, 0)))));
        assert_eq!(key_status(Some(&k), &values, false).state, Toggle::On);
        values
            .probes
            .insert(Probe::Player(None), Value::Player(Ok(Some(media_track(player::PlayStatus::Paused, 0, 0)))));
        assert_eq!(key_status(Some(&k), &values, false).state, Toggle::Off);
        // Next, or a seek, is not a toggle source.
        let next = KeyConfig {
            action: Action::Media { command: crate::MediaCommand::Next, seek: 0, player: None },
            ..k.clone()
        };
        assert_eq!(next.state_probe(), None);
        // Without an on face there's nothing to follow.
        assert_eq!(KeyConfig { on: None, ..k }.state_probe(), None);
    }
}
