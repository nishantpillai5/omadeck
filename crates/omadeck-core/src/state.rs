//! Values the daemon owns at runtime, kept out of the hand-edited config.

use serde::{Deserialize, Serialize};

use crate::paths;

/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's
/// `brightness` is the start value; once the deck has run, this file wins.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl State {
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
        let s = State { brightness: Some(42) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }
}
