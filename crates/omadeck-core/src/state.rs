//! Values the daemon owns at runtime, kept out of the hand-edited config.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;

/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's
/// `brightness` is the start value; once the deck has run, this file wins (a hand edit
/// of the config's value wins again). Later PRs that persist something add a field here,
/// with `#[serde(default, skip_serializing_if = ...)]`.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl State {
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
        let s = State { brightness: Some(42) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }

    #[test]
    fn save_to_and_load_from_a_file() {
        let dir = std::env::temp_dir().join(format!("omadeck-state-test-{}", std::process::id()));
        let path = dir.join("state.toml");
        assert_eq!(State::load_from(&path), State::default()); // absent
        State { brightness: Some(30) }.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), State { brightness: Some(30) });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "brightness = 30\n");
        std::fs::write(&path, "brightness = \"oops\"").unwrap();
        assert_eq!(State::load_from(&path), State::default()); // unreadable
        let _ = std::fs::remove_dir_all(&dir);
    }
}
