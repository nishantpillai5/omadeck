//! Toggle key presets: one click makes a key a working toggle. The state commands are
//! captured on the machine (see plans/pr8-toggle-keys.md, "Captured"): each exits 0
//! exactly when on, in single-digit milliseconds.

use anyhow::{Result, bail};

use crate::action::which;
use crate::config::{Action, AudioDevice, KeyConfig, OnFace};

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
];

/// The same test as `action::which`: on PATH and a file.
pub fn on_path(bin: &str) -> bool {
    which(bin)
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
    k.label = "Mic".into();
    k.glyph = Some("f036c".into());
    k.action = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
    k.on = Some(on_face("Muted", Some("f036d"), None));
}

fn output_mute(k: &mut KeyConfig) {
    k.label = "Sound".into();
    k.glyph = Some("f057e".into());
    k.action = Action::Volume { device: AudioDevice::Output, change: 0, mute: true };
    k.on = Some(on_face("Muted", Some("f075f"), None));
}

fn night_light(k: &mut KeyConfig) {
    k.label = "Night light".into();
    k.glyph = Some("f18e".into());
    k.action = Action::App { command: "omarchy-toggle-nightlight".into() };
    k.on = Some(on_face("Night on", Some("f18e"), Some("omarchy-toggle-nightlight --status | jq -e .enabled")));
}

fn stay_awake(k: &mut KeyConfig) {
    k.label = "Stay awake".into();
    k.action = Action::App { command: "omarchy-toggle-idle".into() };
    k.on = Some(on_face("Awake", None, Some("omarchy-toggle-idle --status | jq -e .enabled")));
}

fn do_not_disturb(k: &mut KeyConfig) {
    k.label = "Do Not Disturb".into();
    k.action = Action::App { command: "omarchy-toggle-notification-silencing".into() };
    // The command has no --status; the state is the file it writes on every change.
    k.on = Some(on_face("DND on", None, Some("jq -e .dnd ~/.local/state/omarchy/notifications.json")));
}

fn bluetooth(k: &mut KeyConfig) {
    k.label = "Bluetooth".into();
    k.action = Action::App { command: "omarchy-bluetooth-power".into() };
    k.on = Some(on_face("BT on", None, Some("omarchy-bluetooth-power is-on")));
}

/// The Service preset: a slow toggle for a systemd user unit, optional health URL.
/// `Err` when `unit` has characters outside `[A-Za-z0-9@._:-]`.
pub fn apply_service(k: &mut KeyConfig, unit: &str, health_url: Option<&str>) -> Result<()> {
    if unit.is_empty() || !unit.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '@' | '.' | '_' | ':' | '-')) {
        bail!("invalid unit name: {unit:?}");
    }
    k.label = unit.into();
    k.glyph = Some("f048b".into());
    k.action = Action::App {
        command: format!(
            "systemctl --user is-active -q {unit} && systemctl --user stop --no-block {unit} || systemctl --user start --no-block {unit}"
        ),
    };
    let state = match health_url {
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
    fn service_rejects_odd_unit_names() {
        assert!(apply_service(&mut KeyConfig::new(0), "x; rm -rf ~", None).is_err());
        assert!(apply_service(&mut KeyConfig::new(0), "", None).is_err());
        assert!(apply_service(&mut KeyConfig::new(0), "a b", None).is_err());
    }
}
