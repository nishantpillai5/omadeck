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
}
