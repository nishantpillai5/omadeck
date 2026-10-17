//! System state the daemon watches (a probe), the values it reads, and what a dial's
//! strip panel shows for them. The daemon's watcher thread reads probes; the daemon, the
//! configurator and `snapshot` all build panels through [`dial_status`].

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{AudioDevice, Config, DialConfig, KeyConfig, LevelSource};
use crate::render::{DialStatus, KeyStatus, Level, PanelStatus};

/// A volume set from the keyboard or `pavucontrol` lands in well under this.
const VOLUME_REFRESH: [Duration; 1] = [Duration::from_millis(150)];
/// A state command's answer: at 300 ms the optimistic guess is confirmed or corrected;
/// at 1.2 s a fast toggle that really failed shows its true state (outside the 1 s
/// `SETTLE` window).
const COMMAND_REFRESH: [Duration; 2] = [Duration::from_millis(300), Duration::from_millis(1200)];

/// While a toggle is busy, or a slow press is waiting for its state to move.
pub const FAST_INTERVAL: Duration = Duration::from_millis(500);
/// How long a slow press keeps polling fast before the normal interval resumes.
pub const FAST_WINDOW: Duration = Duration::from_secs(15);
/// How long a fast toggle's optimistic guess wins over a contradicting read.
pub const SETTLE: Duration = Duration::from_secs(1);

/// Something the daemon reads from the system. PR 9 adds clocks and meters, PR 10
/// `AppVolume(..)`, PR 14 `Player`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    /// An audio device's level and mute.
    Volume(AudioDevice),
    /// `sh -c command` every `interval_ms`. Both fields are the hash key **on purpose**:
    /// the same command at two intervals is two probes, so each key polls at the rate it
    /// asked for. Keys with identical command and interval share one probe.
    Command { command: String, interval_ms: u32 },
}

impl Probe {
    /// How often it's re-read while watched. Volume: 1 s; a command: its own, never
    /// under 1 s.
    pub fn interval(&self) -> Duration {
        match self {
            Probe::Volume(_) => Duration::from_secs(1),
            Probe::Command { interval_ms, .. } => Duration::from_millis((*interval_ms).max(1000) as u64),
        }
    }

    /// When to re-read after a change made elsewhere (`WatchCmd::Refresh`). A volume
    /// change made by the deck is read back at once instead.
    pub fn refresh_after(&self) -> &'static [Duration] {
        match self {
            Probe::Volume(_) => &VOLUME_REFRESH,
            Probe::Command { .. } => &COMMAND_REFRESH,
        }
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

/// What a probe read. Later PRs add variants (PR 9 `Text`, …). It goes on the wire in
/// later events, hence serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// A volume device's level, as the panel draws it (`audio::Level::to_panel`).
    Level(Level),
    /// A toggle key's state. A `Toggle` on a dial shows no level.
    Toggle(Toggle),
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
            Value::Toggle(_) => None,
        }
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
    let level = dial.and_then(DialConfig::level_source).and_then(|src| match src {
        LevelSource::Volume(dev) => values.level(dev).cloned(),
        LevelSource::Brightness => Some(brightness_level(values.brightness)),
    });
    DialStatus { pressed, panel: PanelStatus { level } }
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
    match (dial.and_then(DialConfig::level_source), probe) {
        (Some(LevelSource::Volume(dev)), Probe::Volume(p)) => dev == *p,
        _ => false,
    }
}

/// The ONE builder of a key's status from the watcher's values, like `dial_status`. The
/// daemon, the GUI and `snapshot` call it; PR 9 adds `live`. It covers what `Values`
/// knows (a command's state, a mute); the two per-(page, key) things `Values` can't key
/// (a press toggle's flip and a running job) are laid over it by the daemon's
/// `key_state`.
pub fn key_status(key: Option<&KeyConfig>, values: &Values, pressed: bool) -> KeyStatus {
    let state = match key.filter(|k| k.on.is_some()).and_then(KeyConfig::state_probe) {
        Some(p) => match values.probes.get(&p) {
            Some(Value::Toggle(t)) => t.clone(),
            Some(Value::Level(l)) if l.muted => Toggle::On, // "on" for a mute key is muted
            _ => Toggle::Off,
        },
        None => Toggle::Off,
    };
    KeyStatus { pressed, state }
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

/// The last `NN%` in a busy text, clamped to 100 (`"shard 2/4 43%"` → 43,
/// `"43.5 %"` → 43, `"120%"` → 100, none → `None`).
pub fn progress(text: &str) -> Option<u8> {
    let mut last = None;
    for part in text.split('%') {
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
        if start < end
            && b[start..].iter().any(u8::is_ascii_digit)
            && let Ok(n) = t[start..].parse::<f32>()
        {
            last = Some(n.clamp(0.0, 100.0) as u8);
        }
    }
    last
}

/// Everything the watcher should read while `page` is shown on a deck with `n_dials`
/// dials, each once: today the dials' volume levels, in dial order. PR 8 adds the probes
/// of the keys on `page` (hence the parameter), PR 9 live keys, PR 10 app volumes.
pub fn watched_probes(cfg: &Config, _page: Option<&str>, n_dials: usize) -> Vec<Probe> {
    let mut out = Vec::new();
    for i in 0..n_dials {
        if let Some(LevelSource::Volume(dev)) = cfg.dial(i as u8).and_then(DialConfig::level_source) {
            let p = Probe::Volume(dev);
            if !out.contains(&p) {
                out.push(p);
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

    #[test]
    fn dial_status_brightness_and_plain() {
        let values = Values { brightness: 70, ..Values::default() };
        assert_eq!(dial_status(Some(&brightness_dial(0)), &values, false).panel.level, Some(brightness_level(70)));
        assert_eq!(dial_status(Some(&DialConfig::new(0)), &values, false).panel, PanelStatus::default());
        assert_eq!(dial_status(None, &values, false).panel, PanelStatus::default());
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
        assert_eq!(watched_probes(&cfg, None, 4), both);
        // Dials aren't per page.
        assert_eq!(watched_probes(&cfg, Some("Media"), 4), both);
        // Only the dials the deck has; a deck without dials watches nothing.
        assert_eq!(watched_probes(&cfg, None, 2), vec![Probe::Volume(AudioDevice::Input)]);
        assert_eq!(watched_probes(&cfg, None, 0), vec![]);
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
    fn progress_parses_last_percentage() {
        assert_eq!(progress("Loading 43%"), Some(43));
        assert_eq!(progress("shard 2/4 43%"), Some(43));
        assert_eq!(progress("43.5 %"), Some(43));
        assert_eq!(progress("120%"), Some(100));
        assert_eq!(progress("10% then 20%"), Some(20));
        assert_eq!(progress("50 % done"), Some(50));
        assert_eq!(progress("no number here"), None);
        assert_eq!(progress(""), None);
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
        let deadline = Instant::now() + crate::proc::GRACE + Duration::from_secs(1);
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
}
