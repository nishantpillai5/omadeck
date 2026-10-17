//! Reading and changing the system volume, through Omarchy's own commands when they're
//! installed (so its on-screen pop-up shows) and through `wpctl` otherwise. Also one app's
//! playback streams: read with `pactl`, changed with `wpctl`.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

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
        // Real pactl output.
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
        // Hand-written in the shape `pactl -f json list sink-inputs` gives. The pid is a
        // number here, the id a string: both forms must parse.
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
