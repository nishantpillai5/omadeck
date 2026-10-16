//! Values the daemon owns at runtime, kept out of the hand-edited config.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;

/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's `brightness`
/// is the start value; once the deck has run, this file wins (a hand edit of the config's
/// value wins again).
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// The config's `brightness` when the daemon last read it. A config value that
    /// differs from it was edited since (by hand, or by the configurator with no daemon
    /// running), and wins. Compared by value, not by file times: the configurator
    /// rewrites config.toml on every edit without touching `brightness`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_brightness: Option<u8>,
}

impl State {
    /// The config's `brightness` when it was edited since the daemon last saw it, else
    /// `None`. A state file from before `config_brightness` existed has seen nothing, so
    /// its saved brightness still wins (as it did before).
    pub fn config_edit(&self, cfg: u8) -> Option<u8> {
        match self.config_brightness {
            Some(seen) if seen == cfg => None,
            None if self.brightness.is_some() => None,
            _ => Some(cfg),
        }
    }

    /// The brightness the deck starts at: a config edit, else the saved value, else the
    /// config's.
    pub fn start_brightness(&self, cfg: u8) -> u8 {
        self.config_edit(cfg).or(self.brightness).unwrap_or(cfg)
    }

    /// The state as saved, or empty when the file is absent or unreadable.
    pub fn load() -> Self {
        Self::load_from(&paths::state_file())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&paths::state_file())
    }

    /// Atomic write (temp file, fsync, rename), so a reader never sees a half-written
    /// file and a crash can't leave an empty one. The temp name carries the pid, so a
    /// second daemon can't race on it.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        let Some(dir) = path.parent() else { anyhow::bail!("state path has no parent") };
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".state.toml.{}.tmp", std::process::id()));
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_absent_is_empty() {
        let s = State { brightness: Some(42), config_brightness: Some(70) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None, config_brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }

    #[test]
    fn a_config_edit_is_a_changed_value_not_a_newer_file() {
        // Dimmed to 20 with the dial while the config said 70.
        let s = State { brightness: Some(20), config_brightness: Some(70) };
        // The configurator saved config.toml later (relabelled a key): same 70, no edit.
        assert_eq!(s.config_edit(70), None);
        assert_eq!(s.start_brightness(70), 20);
        // The value itself changed: a hand edit (or the slider with no daemon) wins.
        assert_eq!(s.config_edit(35), Some(35));
        assert_eq!(s.start_brightness(35), 35);
        // A state file from before this field: the saved value still wins.
        let old = State { brightness: Some(20), config_brightness: None };
        assert_eq!(old.start_brightness(70), 20);
        // Nothing saved yet: the config.
        assert_eq!(State::default().start_brightness(70), 70);
    }

    #[test]
    fn save_to_and_load_from_a_file() {
        let dir = std::env::temp_dir().join(format!("omadeck-state-test-{}", std::process::id()));
        let path = dir.join("state.toml");
        assert_eq!(State::load_from(&path), State::default()); // absent
        State { brightness: Some(30), ..State::default() }.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), State { brightness: Some(30), ..State::default() });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "brightness = 30\n");
        std::fs::write(&path, "brightness = \"oops\"").unwrap();
        assert_eq!(State::load_from(&path), State::default()); // unreadable
        let _ = std::fs::remove_dir_all(&dir);
    }
}
