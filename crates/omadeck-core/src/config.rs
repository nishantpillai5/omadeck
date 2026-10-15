//! The on-disk configuration (`~/.config/omadeck/config.toml`).

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Panel brightness, 0–100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    /// Global look; per-key settings override it.
    #[serde(default)]
    pub style: Style,
    /// The home page: what the deck shows at startup.
    #[serde(default, rename = "key", skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// More pages, opened from keys with a `page` action.
    #[serde(default, rename = "page", skip_serializing_if = "Vec::is_empty")]
    pub pages: Vec<Page>,
}

impl Default for Config {
    fn default() -> Self {
        Self { brightness: default_brightness(), style: Style::default(), keys: Keys::default(), pages: Vec::new() }
    }
}

/// A named page of keys. Pages are flat: a "folder" is just a page with a Back key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    #[serde(default, rename = "key", skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
}

/// The keys on one page. Keys that aren't listed are blank.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Keys(pub Vec<KeyConfig>);

impl Keys {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn iter(&self) -> std::slice::Iter<'_, KeyConfig> {
        self.0.iter()
    }

    pub fn get(&self, index: u8) -> Option<&KeyConfig> {
        self.0.iter().find(|k| k.index == index)
    }

    /// Get a key for editing, creating it if needed.
    pub fn get_mut(&mut self, index: u8) -> &mut KeyConfig {
        if let Some(pos) = self.0.iter().position(|k| k.index == index) {
            &mut self.0[pos]
        } else {
            self.0.push(KeyConfig::new(index));
            self.0.last_mut().unwrap()
        }
    }

    pub fn clear(&mut self, index: u8) {
        self.0.retain(|k| k.index != index);
    }

    pub fn swap(&mut self, a: u8, b: u8) {
        for k in &mut self.0 {
            if k.index == a {
                k.index = b;
            } else if k.index == b {
                k.index = a;
            }
        }
    }

    /// Drop blank keys and sort by index, for a tidy file.
    fn tidy(&mut self) {
        self.0.retain(|k| !k.is_blank());
        self.0.sort_by_key(|k| k.index);
    }
}

fn default_brightness() -> u8 {
    70
}

fn yes() -> bool {
    true
}

fn is_true(b: &bool) -> bool {
    *b
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Take default colours from the active Omarchy theme (re-rendered when the theme changes).
    #[serde(default = "yes")]
    pub follow_theme: bool,
    /// Default key background, `#rrggbb`. Overrides the theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    /// Default label colour, `#rrggbb`. Overrides the theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    /// fontconfig pattern for labels. Defaults to the Omarchy monospace font.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font: Option<String>,
}

impl Default for Style {
    fn default() -> Self {
        Self { follow_theme: true, background: None, label_color: None, font: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyConfig {
    /// Zero-based key index, left-to-right, top-to-bottom.
    pub index: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub show_label: bool,
    /// PNG/JPEG/SVG/… path. Relative paths resolve against the config dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// A Nerd Font symbol drawn as the icon in the theme accent colour (used when `icon` is unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    #[serde(default)]
    pub action: Action,
}

impl KeyConfig {
    pub fn new(index: u8) -> Self {
        Self {
            index,
            label: String::new(),
            show_label: true,
            icon: None,
            glyph: None,
            background: None,
            label_color: None,
            action: Action::None,
        }
    }

    /// True when the key carries nothing worth saving.
    pub fn is_blank(&self) -> bool {
        self.label.is_empty()
            && self.icon.is_none()
            && self.glyph.is_none()
            && self.background.is_none()
            && self.label_color.is_none()
            && self.action == Action::None
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Action {
    #[default]
    None,
    /// Launch an application: a desktop entry id (`firefox.desktop`) or a command line.
    App { command: String },
    /// Run a script or executable.
    Script {
        path: String,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        terminal: bool,
    },
    /// Play a sound file through PipeWire.
    Sound {
        file: String,
        /// 0.0–1.0
        #[serde(default, skip_serializing_if = "Option::is_none")]
        volume: Option<f32>,
    },
    /// Open a website in the default browser.
    Url { url: String },
    /// Run a Hyprland dispatcher, e.g. `workspace 3` or `togglefloating`.
    Hyprland { dispatch: String },
    /// Show another page of keys on the deck.
    Page { page: String },
    /// Return to the page shown before this one (Home if there is none).
    Back,
}

impl Action {
    pub const KINDS: [&'static str; 8] =
        ["Nothing", "Launch app", "Run script", "Play sound", "Open website", "Hyprland", "Open page", "Go back"];

    pub fn kind_index(&self) -> usize {
        match self {
            Action::None => 0,
            Action::App { .. } => 1,
            Action::Script { .. } => 2,
            Action::Sound { .. } => 3,
            Action::Url { .. } => 4,
            Action::Hyprland { .. } => 5,
            Action::Page { .. } => 6,
            Action::Back => 7,
        }
    }

    /// Page navigation is handled by whoever owns the deck, not by `action::run`.
    pub fn is_navigation(&self) -> bool {
        matches!(self, Action::Page { .. } | Action::Back)
    }

    pub fn empty_of_kind(i: usize) -> Self {
        match i {
            1 => Action::App { command: String::new() },
            2 => Action::Script { path: String::new(), terminal: false },
            3 => Action::Sound { file: String::new(), volume: None },
            4 => Action::Url { url: String::new() },
            5 => Action::Hyprland { dispatch: String::new() },
            6 => Action::Page { page: String::new() },
            7 => Action::Back,
            _ => Action::None,
        }
    }

    pub fn summary(&self) -> String {
        match self {
            Action::None => "Does nothing".into(),
            Action::App { command } => format!("Launch {command}"),
            Action::Script { path, .. } => format!("Run {path}"),
            Action::Sound { file, .. } => format!("Play {file}"),
            Action::Url { url } => format!("Open {url}"),
            Action::Hyprland { dispatch } => format!("hyprctl dispatch {dispatch}"),
            Action::Page { page } => format!("Open page {page}"),
            Action::Back => "Go back".into(),
        }
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&paths::config_file())
    }

    /// Missing file → default config.
    pub fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&paths::config_file())
    }

    /// Atomic write (temp file + rename) so the daemon never reads a half-written file.
    pub fn save_to(&self, path: &Path) -> Result<()> {
        let mut clean = self.clone();
        clean.keys.tidy();
        for p in &mut clean.pages {
            p.keys.tidy();
        }
        let body = toml::to_string_pretty(&clean)?;
        let dir = path.parent().context("config path has no parent")?;
        fs::create_dir_all(dir)?;
        let tmp = dir.join(".config.toml.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(b"# omadeck configuration - edit by hand or with the `omadeck` app.\n\n")?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok(())
    }

    /// The keys of a page: `None` is the home page.
    pub fn page(&self, page: Option<&str>) -> Option<&Keys> {
        match page {
            None => Some(&self.keys),
            Some(name) => self.pages.iter().find(|p| p.name == name).map(|p| &p.keys),
        }
    }

    pub fn page_mut(&mut self, page: Option<&str>) -> Option<&mut Keys> {
        match page {
            None => Some(&mut self.keys),
            Some(name) => self.pages.iter_mut().find(|p| p.name == name).map(|p| &mut p.keys),
        }
    }

    pub fn key(&self, page: Option<&str>, index: u8) -> Option<&KeyConfig> {
        self.page(page)?.get(index)
    }

    pub fn has_page(&self, name: &str) -> bool {
        self.pages.iter().any(|p| p.name == name)
    }

    pub fn page_names(&self) -> Vec<String> {
        self.pages.iter().map(|p| p.name.clone()).collect()
    }

    /// Why `name` can't be used for a new or renamed page, if it can't.
    pub fn page_name_problem(&self, name: &str) -> Option<&'static str> {
        let name = name.trim();
        if name.is_empty() {
            Some("Give the page a name")
        } else if name.eq_ignore_ascii_case(HOME) {
            Some("“Home” is the main page's name")
        } else if self.has_page(name) {
            Some("There's already a page with that name")
        } else {
            None
        }
    }

    /// `base` if it's free, otherwise `base 2`, `base 3`, …
    pub fn unique_page_name(&self, base: &str) -> String {
        let base = base.trim();
        let base = if base.is_empty() || base.eq_ignore_ascii_case(HOME) { "Page" } else { base };
        (1..)
            .map(|n| if n == 1 { base.to_string() } else { format!("{base} {n}") })
            .find(|n| !self.has_page(n))
            .unwrap()
    }

    fn all_keys_mut(&mut self) -> impl Iterator<Item = &mut KeyConfig> {
        self.keys.0.iter_mut().chain(self.pages.iter_mut().flat_map(|p| p.keys.0.iter_mut()))
    }

    /// How many keys open the page `name`.
    pub fn links_to(&self, name: &str) -> usize {
        self.keys
            .iter()
            .chain(self.pages.iter().flat_map(|p| p.keys.iter()))
            .filter(|k| matches!(&k.action, Action::Page { page } if page == name))
            .count()
    }

    /// Rename a page and every key that opens it.
    pub fn rename_page(&mut self, old: &str, new: &str) {
        let new = new.trim();
        for p in &mut self.pages {
            if p.name == old {
                p.name = new.to_string();
            }
        }
        for k in self.all_keys_mut() {
            if let Action::Page { page } = &mut k.action
                && page == old
            {
                *page = new.to_string();
            }
        }
    }

    /// Delete a page. Keys that opened it keep their look but no longer do anything.
    pub fn remove_page(&mut self, name: &str) {
        self.pages.retain(|p| p.name != name);
        for k in self.all_keys_mut() {
            if matches!(&k.action, Action::Page { page } if page == name) {
                k.action = Action::None;
            }
        }
    }
}

/// What the home page is called in the UI. It has no name in the config file.
pub const HOME: &str = "Home";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let mut c = Config::default();
        c.keys.get_mut(0).label = "Term".into();
        c.keys.get_mut(0).action = Action::App { command: "Alacritty.desktop".into() };
        c.keys.get_mut(3).action = Action::Sound { file: "~/a.wav".into(), volume: Some(0.5) };
        c.keys.get_mut(4).action = Action::Script { path: "x.sh".into(), terminal: true };
        c.keys.get_mut(5).action = Action::Page { page: "Media".into() };
        let mut media = Page { name: "Media".into(), keys: Keys::default() };
        media.keys.get_mut(0).action = Action::Back;
        media.keys.get_mut(1).label = "Play".into();
        c.pages.push(media);
        c.pages.push(Page { name: "Empty".into(), keys: Keys::default() });
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn swap() {
        let mut c = Config::default();
        c.keys.get_mut(1).label = "a".into();
        c.keys.get_mut(2).label = "b".into();
        c.keys.swap(1, 2);
        assert_eq!(c.key(None, 1).unwrap().label, "b");
        assert_eq!(c.key(None, 2).unwrap().label, "a");
    }

    #[test]
    fn pages() {
        let mut c = Config::default();
        c.pages.push(Page { name: "Media".into(), keys: Keys::default() });
        c.keys.get_mut(0).action = Action::Page { page: "Media".into() };
        c.page_mut(Some("Media")).unwrap().get_mut(2).action = Action::Page { page: "Media".into() };
        assert_eq!(c.links_to("Media"), 2);
        assert!(c.key(Some("Nope"), 0).is_none());

        assert_eq!(c.unique_page_name("Media"), "Media 2");
        assert_eq!(c.unique_page_name("home"), "Page");
        assert!(c.page_name_problem(" Media ").is_some());
        assert!(c.page_name_problem("HOME").is_some());
        assert!(c.page_name_problem("Games").is_none());

        c.rename_page("Media", "Music");
        assert!(c.has_page("Music") && !c.has_page("Media"));
        assert_eq!(c.key(None, 0).unwrap().action, Action::Page { page: "Music".into() });
        assert_eq!(c.links_to("Music"), 2);

        c.remove_page("Music");
        assert!(c.pages.is_empty());
        assert_eq!(c.key(None, 0).unwrap().action, Action::None);
    }

    #[test]
    fn save_tidies_every_page() {
        let dir = std::env::temp_dir().join(format!("omadeck-test-{}", std::process::id()));
        let path = dir.join("config.toml");
        let mut c = Config::default();
        c.keys.get_mut(4).label = "b".into();
        c.keys.get_mut(1).label = "a".into();
        c.pages.push(Page { name: "P".into(), keys: Keys::default() });
        c.page_mut(Some("P")).unwrap().get_mut(3);
        c.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(back.keys.iter().map(|k| k.index).collect::<Vec<_>>(), [1, 4]);
        assert!(back.page(Some("P")).unwrap().is_empty(), "blank keys are dropped, empty pages kept");
    }

    #[test]
    fn example_parses() {
        let s = include_str!("../../../examples/config.toml");
        let c: Config = toml::from_str(s).unwrap();
        assert!(!c.keys.is_empty());
    }
}
