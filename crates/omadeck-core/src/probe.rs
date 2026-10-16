//! System state the daemon watches (a probe), the values it reads, and what a dial's
//! strip panel shows for them. The daemon's watcher thread reads probes; the daemon, the
//! configurator and `snapshot` all build panels through [`dial_status`].

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{AudioDevice, Config, DialConfig, LevelSource};
use crate::render::{DialStatus, Level, PanelStatus};

/// A volume set from the keyboard or `pavucontrol` lands in well under this.
const VOLUME_REFRESH: [Duration; 1] = [Duration::from_millis(150)];

/// Something the daemon reads from the system. PR 8 adds `Command { .. }`, PR 9 clocks
/// and meters, PR 10 `AppVolume(..)`, PR 14 `Player`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    /// An audio device's level and mute.
    Volume(AudioDevice),
}

impl Probe {
    /// How often it's re-read while watched.
    pub fn interval(&self) -> Duration {
        match self {
            Probe::Volume(_) => Duration::from_secs(1),
        }
    }

    /// When to re-read after a change made elsewhere (`WatchCmd::Refresh`; PR 8's
    /// toggles send it). A volume change made by the deck is read back at once instead.
    pub fn refresh_after(&self) -> &'static [Duration] {
        match self {
            Probe::Volume(_) => &VOLUME_REFRESH,
        }
    }
}

/// What a probe read. Later PRs add variants (PR 8 `Toggle`, PR 9 `Text`, …). It goes on
/// the wire in later events, hence serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// A volume device's level, as the panel draws it (`audio::Level::to_panel`).
    Level(Level),
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
}
