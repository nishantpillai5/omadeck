//! Pages that follow the focused app: window-class matching and the open-window list.

use serde::Deserialize;

/// The configurator's window class (its GTK application id). The daemon ignores focus on
/// it, so editing never moves the deck.
pub const CONFIGURATOR_CLASS: &str = "dev.omadeck.Omadeck";

/// Whether a Hyprland window class matches `pattern`: ASCII case-insensitive, `*` matches
/// any run. An empty pattern or class never matches.
pub fn class_matches(pattern: &str, class: &str) -> bool {
    let (p, c) = (pattern.trim().to_ascii_lowercase(), class.to_ascii_lowercase());
    if p.is_empty() || c.is_empty() {
        return false;
    }
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == c;
    }
    let Some(mut rest) = c.strip_prefix(parts[0]) else { return false };
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    rest.ends_with(parts[parts.len() - 1])
}

/// `(class, title)` for each open window in `hyprctl -j clients` output, one per class,
/// sorted. Empty classes are dropped; anything unparsable gives an empty list.
pub fn parse_clients(json: &str) -> Vec<(String, String)> {
    #[derive(Deserialize)]
    struct Client {
        #[serde(default)]
        class: String,
        #[serde(default)]
        title: String,
    }
    let mut v: Vec<(String, String)> = serde_json::from_str::<Vec<Client>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| !c.class.is_empty())
        .map(|c| (c.class, c.title))
        .collect();
    v.sort();
    v.dedup_by(|a, b| a.0 == b.0);
    v
}

/// The window class an installed app's windows get, from its desktop id and, if found, its
/// desktop entry: the entry's `StartupWMClass`, else the id without `.desktop` (GTK and
/// Flatpak apps use their app id; matching is case-insensitive, which covers most others).
pub fn desktop_class(id: &str, entry: Option<&str>) -> String {
    let mut in_main = false;
    for line in entry.unwrap_or_default().lines().map(str::trim) {
        if line.starts_with('[') {
            in_main = line == "[Desktop Entry]";
        } else if in_main
            && let Some((key, value)) = line.split_once('=')
            && key.trim() == "StartupWMClass"
            && !value.trim().is_empty()
        {
            return value.trim().to_string();
        }
    }
    id.strip_suffix(".desktop").unwrap_or(id).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_matches_globs() {
        assert!(class_matches("spotify", "Spotify"), "case-insensitive");
        assert!(!class_matches("spot", "spotify"), "no star: the whole class");
        assert!(class_matches("chrome-*-Default", "chrome-music.youtube.com__-Default"));
        assert!(class_matches("*spotify*", "com.spotify.Client"));
        assert!(class_matches("*", "kitty"));
        assert!(!class_matches("a*b*a", "ab"), "parts can't overlap");
        assert!(!class_matches("", "kitty") && !class_matches("*", ""), "empty never matches");
    }

    #[test]
    fn desktop_class_prefers_startup_wm_class() {
        let entry = "[Desktop Entry]\nName=Spotify\nStartupWMClass = spotify \n[Desktop Action x]\nStartupWMClass=no\n";
        assert_eq!(desktop_class("spotify.desktop", Some(entry)), "spotify");
        let only_action = "[Desktop Entry]\nName=X\n[Desktop Action x]\nStartupWMClass=no\n";
        assert_eq!(desktop_class("org.gnome.Nautilus.desktop", Some(only_action)), "org.gnome.Nautilus");
        assert_eq!(desktop_class("kitty.desktop", None), "kitty");
    }

    #[test]
    fn parse_clients_dedups_and_sorts() {
        let json = r#"[
            {"address": "0x1", "class": "spotify", "title": "Spotify Premium"},
            {"address": "0x2", "class": "kitty", "title": "~"},
            {"address": "0x3", "class": "spotify", "title": "Song - Artist"},
            {"address": "0x4", "class": "", "title": "a layer"}
        ]"#;
        let got = parse_clients(json);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0], ("kitty".to_string(), "~".to_string()));
        assert_eq!(got[1].0, "spotify");
        assert!(parse_clients("not json").is_empty());
    }

    #[test]
    fn parse_clients_on_a_real_capture() {
        let got = parse_clients(include_str!("../tests/fixtures/hyprctl-clients.json"));
        let classes: Vec<&str> = got.iter().map(|(c, _)| c.as_str()).collect();
        assert_eq!(classes, vec!["brave-browser", "dev.omadeck.Omadeck", "kitty"]);
    }
}
