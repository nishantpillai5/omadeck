//! First-run layout: a useful Omarchy deck out of the box, exercising every action type.

use std::path::Path;

use gtk::{gdk, gio, prelude::*};
use omadeck_core::{Action, Config, KeyConfig, Keys};

use crate::media;

/// Find a regular (non-symbolic) icon in the user's icon theme and import it.
fn themed_icon(names: &[&str]) -> Option<String> {
    let display = gdk::Display::default()?;
    let theme = gtk::IconTheme::for_display(&display);
    let name = names.iter().find(|n| theme.has_icon(n))?;
    let paintable = theme.lookup_icon(name, &[], 256, 1, gtk::TextDirection::None, gtk::IconLookupFlags::FORCE_REGULAR);
    let path = paintable.file()?.path()?;
    let bytes = std::fs::read(&path).ok()?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
    media::store_icon(name, ext, &bytes).ok()
}

fn default_app_icon(mime: &str) -> Option<String> {
    gio::AppInfo::default_for_type(mime, false).as_ref().and_then(media::import_app_icon)
}

fn key(index: u8, label: &str, icon: Option<String>, action: Action) -> KeyConfig {
    KeyConfig { label: label.into(), icon, action, ..KeyConfig::new(index) }
}

fn app(cmd: &str) -> Action {
    Action::App { command: cmd.into() }
}

pub fn layout() -> Config {
    let mut keys = vec![
        key(0, "Terminal", themed_icon(&["utilities-terminal", "terminal"]), app("xdg-terminal-exec")),
        key(
            1,
            "Browser",
            default_app_icon("x-scheme-handler/https").or_else(|| themed_icon(&["web-browser"])),
            app("omarchy launch browser"),
        ),
        key(
            2,
            "Files",
            default_app_icon("inode/directory").or_else(|| themed_icon(&["system-file-manager", "folder"])),
            app("omarchy launch nautilus"),
        ),
        key(
            3,
            "Screenshot",
            themed_icon(&["applets-screenshooter", "org.gnome.Screenshot", "camera-photo"]),
            app("omarchy capture screenshot"),
        ),
        key(
            4,
            "Omarchy",
            Path::new("/usr/share/pixmaps/omarchy.png").exists().then(|| "/usr/share/pixmaps/omarchy.png".into()),
            Action::Url { url: "https://omarchy.org".into() },
        ),
    ];
    for n in 1..=5u8 {
        keys.push(key(4 + n, &n.to_string(), None, Action::Hyprland { dispatch: format!("workspace {n}") }));
    }
    // The two presets that work everywhere, by name: the output mute toggle (the on
    // face follows the mute) and the night light (a real state command, when jq is
    // there; a plain key otherwise, or its state command fails every poll).
    let preset = |name: &str| omadeck_core::PRESETS.iter().find(|p| p.name == name).expect("a built-in preset");
    let mut mute = KeyConfig::new(10);
    (preset("Output mute").apply)(&mut mute);
    let night_light = preset("Night light");
    let night = if night_light.needs.is_none_or(omadeck_core::on_path) {
        let mut k = KeyConfig::new(11);
        (night_light.apply)(&mut k);
        k
    } else {
        key(11, "Night light", None, app("omarchy-toggle-nightlight"))
    };
    keys.extend([
        mute,
        night,
        key(
            12,
            "Wallpaper",
            themed_icon(&["preferences-desktop-wallpaper", "preferences-desktop-theme"]),
            app("omarchy theme bg next"),
        ),
        key(
            13,
            "Ding!",
            themed_icon(&["audio-x-generic", "multimedia-volume-control"]),
            Action::Sound { file: "/usr/share/sounds/freedesktop/stereo/complete.oga".into(), volume: None },
        ),
        key(14, "Lock", themed_icon(&["system-lock-screen", "changes-prevent"]), app("omarchy system lock")),
    ]);
    Config { keys: Keys(keys), ..Config::default() }
}
