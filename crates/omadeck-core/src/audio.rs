//! Reading and changing the system volume, through Omarchy's own commands when they're
//! installed (so its on-screen pop-up shows) and through `wpctl` otherwise.

use std::process::{Command, Stdio};
use std::sync::OnceLock;

use crate::config::AudioDevice;

/// A device's current level, as read from the audio stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Run a command and return its stdout, `None` if it is missing or fails.
fn capture(args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(args[0]);
    cmd.args(&args[1..]).env("LC_ALL", "C").stdin(Stdio::null());
    let out = cmd.output().ok()?;
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
