//! Values the daemon owns at runtime, kept out of the hand-edited config.

use serde::{Deserialize, Serialize};

use crate::paths;

/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's `brightness`
/// is the start value; once the deck has run, this file wins (a hand edit of the config's
/// value wins again).
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
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
        std::fs::read_to_string(paths::state_file()).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    /// Atomic write (temp file + rename), so the daemon never reads a half-written file.
    pub fn save(&self) -> anyhow::Result<()> {
        let path = paths::state_file();
        let Some(dir) = path.parent() else { anyhow::bail!("state path has no parent") };
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(".state.toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)?;
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
    fn a_config_brightness_edited_since_wins_at_start() {
        let s = State { brightness: Some(20), config_brightness: Some(70) };
        assert_eq!(s.start_brightness(70), 20);
        assert_eq!(s.start_brightness(35), 35);
        let old = State { brightness: Some(20), config_brightness: None };
        assert_eq!(old.start_brightness(70), 20);
        assert_eq!(State::default().start_brightness(70), 70);
    }
}
