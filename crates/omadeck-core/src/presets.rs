//! Toggle key presets: one click makes a key a working toggle. The state commands are
//! captured on the machine (see plans/pr8-toggle-keys.md, "Captured"): each exits 0
//! exactly when on, in single-digit milliseconds.

use anyhow::{Result, bail};

use crate::action::which;
use crate::config::{Action, AudioDevice, KeyConfig, MediaCommand, OnFace};

pub struct Preset {
    pub name: &'static str,
    /// A binary that must be on PATH, else the menu item is insensitive with the
    /// tooltip "needs <bin>".
    pub needs: Option<&'static str>,
    pub apply: fn(&mut KeyConfig),
}

pub const PRESETS: &[Preset] = &[
    Preset { name: "Mic mute", needs: None, apply: mic_mute },
    Preset { name: "Output mute", needs: None, apply: output_mute },
    Preset { name: "Night light", needs: Some("jq"), apply: night_light },
    Preset { name: "Stay awake", needs: Some("jq"), apply: stay_awake },
    Preset { name: "Do Not Disturb", needs: Some("jq"), apply: do_not_disturb },
    Preset { name: "Bluetooth", needs: None, apply: bluetooth },
    // Works through Omarchy's media commands without playerctl.
    Preset { name: "Play/Pause", needs: None, apply: play_pause },
];

/// The same test as `action::which`: on PATH and a file.
pub fn on_path(bin: &str) -> bool {
    which(bin)
}

/// What every preset does first, applied in place: drop what would fight the new
/// toggle. A live value would still decide the key's text, and a leftover busy look or
/// `busy_while_running` would come back on the new action (BUGS.md 90). The index,
/// background and colours stay.
fn start(k: &mut KeyConfig) {
    k.live = None;
    k.busy = None;
    k.busy_while_running = false;
}

/// A preset's glyph replaces the key's icon too: an icon would beat the glyph on the off
/// face (and the on face's glyph would then be the only one shown).
fn set_glyph(k: &mut KeyConfig, glyph: &str) {
    k.icon = None;
    k.glyph = Some(glyph.into());
}

fn on_face(label: &str, glyph: Option<&str>, state: Option<&str>) -> OnFace {
    OnFace {
        label: Some(label.into()),
        glyph: glyph.map(str::to_string),
        state: state.map(str::to_string),
        ..OnFace::default()
    }
}

/// The mute presets use the built-in volume action: the key toggles the mute, and `on`
/// with no state follows it.
fn mic_mute(k: &mut KeyConfig) {
    start(k);
    k.label = "Mic".into();
    set_glyph(k, "f036c");
    k.action = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
    k.on = Some(on_face("Muted", Some("f036d"), None));
}

fn output_mute(k: &mut KeyConfig) {
    start(k);
    k.label = "Sound".into();
    set_glyph(k, "f057e");
    k.action = Action::Volume { device: AudioDevice::Output, change: 0, mute: true };
    k.on = Some(on_face("Muted", Some("f075f"), None));
}

fn night_light(k: &mut KeyConfig) {
    start(k);
    k.label = "Night light".into();
    set_glyph(k, "f18e");
    k.action = Action::App { command: "omarchy-toggle-nightlight".into() };
    k.on = Some(on_face("Night on", Some("f18e"), Some("omarchy-toggle-nightlight --status | jq -e .enabled")));
}

fn stay_awake(k: &mut KeyConfig) {
    start(k);
    k.label = "Stay awake".into();
    k.action = Action::App { command: "omarchy-toggle-idle".into() };
    k.on = Some(on_face("Awake", None, Some("omarchy-toggle-idle --status | jq -e .enabled")));
}

fn do_not_disturb(k: &mut KeyConfig) {
    start(k);
    k.label = "Do Not Disturb".into();
    k.action = Action::App { command: "omarchy-toggle-notification-silencing".into() };
    // The command has no --status; the state is the file it writes on every change.
    k.on = Some(on_face("DND on", None, Some("jq -e .dnd ~/.local/state/omarchy/notifications.json")));
}

fn bluetooth(k: &mut KeyConfig) {
    start(k);
    k.label = "Bluetooth".into();
    k.action = Action::App { command: "omarchy-bluetooth-power".into() };
    k.on = Some(on_face("BT on", None, Some("omarchy-bluetooth-power is-on")));
}

/// The face shows what a press will do: play while paused, pause while playing. The on
/// face follows the player (no state command).
fn play_pause(k: &mut KeyConfig) {
    k.label = "Play".into();
    set_glyph(k, "f040a");
    k.action = Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None };
    k.on = Some(on_face("Pause", Some("f03e4"), None));
}

/// The Service preset: a slow toggle for a systemd user unit, optional health URL.
/// `Err` when `unit` has characters outside `[A-Za-z0-9@._:-]`, or the URL has a control
/// character; then `k` is untouched, so it can be applied to the key in place. The key's
/// index, background and colours are kept.
pub fn apply_service(k: &mut KeyConfig, unit: &str, health_url: Option<&str>) -> Result<()> {
    if unit.is_empty() || !unit.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | ':' | '-')) {
        bail!("invalid unit name: {unit:?}");
    }
    if let Some(url) = health_url
        && url.chars().any(char::is_control)
    {
        bail!("the health URL has a control character in it");
    }
    start(k);
    k.label = unit.into();
    set_glyph(k, "f048b");
    k.action = Action::App {
        command: format!(
            "systemctl --user is-active -q {unit} && systemctl --user stop --no-block {unit} || systemctl --user start --no-block {unit}"
        ),
    };
    let state = match health_url.map(shell_words::quote) {
        Some(url) => format!(
            "s=$(systemctl --user is-active {unit})\ncase $s in\n  active) curl -sf -o /dev/null -m 2 {url} && exit 0\n          echo Loading; exit 75 ;;\n  activating|deactivating|reloading) echo \"$s\"; exit 75 ;;\n  *) exit 1 ;;\nesac"
        ),
        None => format!(
            "s=$(systemctl --user is-active {unit})\ncase $s in\n  active) exit 0 ;;\n  activating|deactivating|reloading) echo \"$s\"; exit 75 ;;\n  *) exit 1 ;;\nesac"
        ),
    };
    k.on = Some(OnFace { label: Some(format!("{unit} on")), state: Some(state), slow: true, ..OnFace::default() });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Config;

    #[test]
    fn every_preset_makes_a_toggle_that_roundtrips() {
        for p in PRESETS {
            let mut k = KeyConfig::new(3);
            (p.apply)(&mut k);
            assert!(k.on.is_some(), "{} makes a toggle", p.name);
            let mut c = Config::default();
            *c.keys.get_mut(3) = k.clone();
            let s = toml::to_string_pretty(&c).unwrap();
            assert_eq!(Config::parse(&s).unwrap().key(None, 3), Some(&k), "{} survives a save", p.name);
        }
    }

    #[test]
    fn service_preset_is_a_slow_toggle() {
        let mut k = KeyConfig::new(0);
        apply_service(&mut k, "llm.service", Some("http://127.0.0.1:8000/health")).unwrap();
        assert!(k.is_slow_toggle());
        match k.state_probe().unwrap() {
            crate::probe::Probe::Command { interval_ms, .. } => assert_eq!(interval_ms, 2000),
            other => panic!("expected a command probe, got {other:?}"),
        }
        let mut k = KeyConfig::new(0);
        apply_service(&mut k, "backup.timer", None).unwrap();
        assert!(k.is_slow_toggle());
        assert!(!k.on.as_ref().unwrap().state.as_deref().unwrap().contains("curl"));
    }

    #[test]
    fn service_quotes_the_health_url() {
        let mut k = KeyConfig::new(5);
        apply_service(&mut k, "llm.service", Some("http://127.0.0.1:8000/h?a=1&b=2")).unwrap();
        assert_eq!(k.index, 5, "applied in place, the key keeps its slot");
        let state = k.on.unwrap().state.unwrap();
        assert!(state.contains("-m 2 'http://127.0.0.1:8000/h?a=1&b=2' && exit 0"), "{state}");
        // The script still parses as shell.
        let ok = std::process::Command::new("sh").args(["-n", "-c", &state]).status().unwrap();
        assert!(ok.success());
        assert!(apply_service(&mut KeyConfig::new(0), "llm.service", Some("http://x/\nrm -rf ~")).is_err());
    }

    #[test]
    fn presets_keep_the_key_and_replace_its_icon() {
        // A CPU key with a busy look, made a preset in place: the live value and the busy
        // settings go (BUGS.md 90), the slot and colours stay.
        let old = || KeyConfig {
            icon: Some("old.png".into()),
            background: Some("#112233".into()),
            busy_while_running: true,
            busy: Some(crate::config::BusyFace { label: Some("Wait".into()), ..Default::default() }),
            live: Some(crate::config::Live::Cpu),
            ..KeyConfig::new(7)
        };
        let service = |k: &mut KeyConfig| apply_service(k, "llm.service", None).unwrap();
        for (name, apply) in PRESETS.iter().map(|p| (p.name, p.apply)).chain([("Service", service as fn(&mut _))]) {
            let mut k = old();
            apply(&mut k);
            assert_eq!(k.index, 7, "{name}");
            assert_eq!(k.background.as_deref(), Some("#112233"), "{name}");
            if k.glyph.is_some() {
                assert_eq!(k.icon, None, "{name}: its glyph would hide behind the old icon");
            }
            assert_eq!(k.live, None, "{name}: the live value would still decide the face");
            assert_eq!(k.busy, None, "{name}");
            assert!(!k.busy_while_running, "{name}");
        }
    }

    #[test]
    fn service_rejects_odd_unit_names() {
        assert!(apply_service(&mut KeyConfig::new(0), "x; rm -rf ~", None).is_err());
        assert!(apply_service(&mut KeyConfig::new(0), "", None).is_err());
        assert!(apply_service(&mut KeyConfig::new(0), "a b", None).is_err());
    }
}
