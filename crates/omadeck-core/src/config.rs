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
    /// Dials on the Stream Deck +, one `[[dial]]` table per dial.
    #[serde(default, rename = "dial", skip_serializing_if = "Vec::is_empty")]
    pub dials: Vec<DialConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            brightness: default_brightness(),
            style: Style::default(),
            keys: Keys::default(),
            pages: Vec::new(),
            dials: Vec::new(),
        }
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

    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialConfig {
    /// Zero-based dial index, left to right.
    pub index: u8,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Runs once per detent when the dial is turned left.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub turn_left: Action,
    /// Runs once per detent when the dial is turned right.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub turn_right: Action,
    /// Runs when the dial is pressed down.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub press: Action,
}

impl DialConfig {
    pub fn new(index: u8) -> Self {
        Self { index, label: String::new(), turn_left: Action::None, turn_right: Action::None, press: Action::None }
    }

    /// True when the dial carries nothing worth saving.
    pub fn is_blank(&self) -> bool {
        self.label.is_empty() && self.turn_left.is_none() && self.turn_right.is_none() && self.press.is_none()
    }
}

impl Config {
    pub fn load() -> Result<Self> {
        Self::load_from(&paths::config_file())
    }

    /// Missing file → default config.
    pub fn load_from(path: &Path) -> Result<Self> {
        match fs::read_to_string(path) {
            Ok(s) => Self::parse(&s).with_context(|| format!("parsing {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    pub fn parse(src: &str) -> Result<Self> {
        Ok(toml::from_str(src)?)
    }

    /// Saves and returns exactly what was written.
    pub fn save(&self) -> Result<String> {
        self.save_to(&paths::config_file())
    }

    /// Atomic write (temp file + rename) so the daemon never reads a half-written file.
    /// A symlinked config (dotfile managers) is written through, keeping the link.
    pub fn save_to(&self, path: &Path) -> Result<String> {
        let mut clean = self.clone();
        clean.keys.tidy();
        for p in &mut clean.pages {
            p.keys.tidy();
        }
        clean.dials.retain(|d| !d.is_blank());
        clean.dials.sort_by_key(|d| d.index);
        let body = format!(
            "# omadeck configuration - edit by hand or with the `omadeck` app.\n\n{}",
            toml::to_string_pretty(&clean)?
        );
        let link = fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
        let path = if link {
            fs::canonicalize(path).with_context(|| format!("following {}", path.display()))?
        } else {
            path.to_path_buf()
        };
        let dir = path.parent().context("config path has no parent")?;
        fs::create_dir_all(dir)?;
        let tmp = dir.join(".config.toml.tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, &path)?;
        Ok(body)
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
    /// Returns what [`Config::restore_page`] needs to undo it.
    pub fn remove_page(&mut self, name: &str) -> Option<RemovedPage> {
        let pos = self.pages.iter().position(|p| p.name == name)?;
        let page = self.pages.remove(pos);
        let mut links = Vec::new();
        let home = self.keys.0.iter_mut().map(|k| (None, k));
        let others = self.pages.iter_mut().flat_map(|p| p.keys.0.iter_mut().map(|k| (Some(p.name.clone()), k)));
        for (on, k) in home.chain(others) {
            if matches!(&k.action, Action::Page { page } if page == name) {
                k.action = Action::None;
                links.push((on, k.index));
            }
        }
        Some(RemovedPage { pos, page, links })
    }

    /// Undo [`Config::remove_page`]: put the page back where it was and re-link the keys
    /// that opened it, unless they've been given another job since.
    pub fn restore_page(&mut self, removed: RemovedPage) -> bool {
        let RemovedPage { pos, page, links } = removed;
        if self.has_page(&page.name) {
            return false;
        }
        let name = page.name.clone();
        self.pages.insert(pos.min(self.pages.len()), page);
        for (on, index) in links {
            if let Some(k) = self.page_mut(on.as_deref()).and_then(|keys| keys.0.iter_mut().find(|k| k.index == index))
                && k.action == Action::None
            {
                k.action = Action::Page { page: name.clone() };
            }
        }
        true
    }

    /// Whether the deck can leave `page`: it needs a Back key or a key that opens a page.
    /// Home never needs one.
    pub fn has_way_out(&self, page: Option<&str>) -> bool {
        page.is_none()
            || self
                .page(page)
                .is_none_or(|keys| keys.iter().any(|k| matches!(k.action, Action::Back | Action::Page { .. })))
    }

    pub fn dial(&self, index: u8) -> Option<&DialConfig> {
        self.dials.iter().find(|d| d.index == index)
    }

    /// Get a dial for editing, creating it if needed.
    pub fn dial_mut(&mut self, index: u8) -> &mut DialConfig {
        if let Some(pos) = self.dials.iter().position(|d| d.index == index) {
            &mut self.dials[pos]
        } else {
            self.dials.push(DialConfig::new(index));
            self.dials.last_mut().unwrap()
        }
    }

    pub fn clear_dial(&mut self, index: u8) {
        self.dials.retain(|d| d.index != index);
    }
}

/// A deleted page, kept so the deletion can be undone.
#[derive(Debug, Clone)]
pub struct RemovedPage {
    pos: usize,
    pub page: Page,
    /// Keys (page, index) that opened it.
    links: Vec<(Option<String>, u8)>,
}

/// How many backups of config.toml to keep.
const BACKUPS: usize = 20;

/// Copy config.toml into the backups folder (`~/.local/state/omadeck/backups`), unless the
/// newest backup already matches it, and prune old ones. Returns the new backup's path.
pub fn backup() -> Result<Option<std::path::PathBuf>> {
    let Ok(body) = fs::read(paths::config_file()) else { return Ok(None) };
    let dir = paths::backups_dir();
    fs::create_dir_all(&dir)?;
    let mut old: Vec<_> = fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with("config-") && n.ends_with(".toml"))
        })
        .collect();
    old.sort();
    let mut made = None;
    if !old.last().is_some_and(|p| fs::read(p).is_ok_and(|b| b == body)) {
        let path = dir.join(format!("config-{}.toml", paths::timestamp()));
        fs::write(&path, &body)?;
        old.push(path.clone());
        made = Some(path);
    }
    let excess = old.len().saturating_sub(BACKUPS);
    for p in &old[..excess] {
        let _ = fs::remove_file(p);
    }
    Ok(made)
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
    fn undo_page_delete() {
        let mut c = Config::default();
        for n in ["A", "B", "C"] {
            c.pages.push(Page { name: n.into(), keys: Keys::default() });
        }
        c.keys.get_mut(0).action = Action::Page { page: "B".into() };
        c.page_mut(Some("A")).unwrap().get_mut(2).action = Action::Page { page: "B".into() };
        c.page_mut(Some("B")).unwrap().get_mut(1).label = "inside".into();
        let before = c.clone();

        let removed = c.remove_page("B").unwrap();
        assert!(!c.has_page("B"));
        assert_eq!(c.links_to("B"), 0);
        assert!(c.restore_page(removed));
        assert_eq!(c, before, "page back in place with its keys and links");

        // A link that was given another job meanwhile is left alone.
        let removed = c.remove_page("B").unwrap();
        c.keys.get_mut(0).action = Action::Back;
        c.restore_page(removed);
        assert_eq!(c.keys.get(0).unwrap().action, Action::Back);
        assert_eq!(c.links_to("B"), 1);
        assert!(c.remove_page("nope").is_none());
    }

    #[test]
    fn way_out() {
        let mut c = Config::default();
        c.pages.push(Page { name: "P".into(), keys: Keys::default() });
        assert!(c.has_way_out(None));
        assert!(!c.has_way_out(Some("P")));
        c.page_mut(Some("P")).unwrap().get_mut(3).action = Action::Page { page: "P".into() };
        assert!(c.has_way_out(Some("P")));
    }

    #[test]
    fn save_keeps_a_symlinked_config() {
        let dir = std::env::temp_dir().join(format!("omadeck-link-{}", std::process::id()));
        let real = dir.join("dotfiles/omadeck.toml");
        let link = dir.join("config.toml");
        std::fs::create_dir_all(real.parent().unwrap()).unwrap();
        std::fs::write(&real, "").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut c = Config::default();
        c.keys.get_mut(0).label = "via link".into();
        c.save_to(&link).unwrap();
        let still_link = std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink();
        let back = Config::load_from(&real).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(still_link);
        assert_eq!(back.keys.iter().next().unwrap().label, "via link");
    }

    #[test]
    fn old_config_without_dials_loads() {
        let old = r#"
            brightness = 70
            [[key]]
            index = 0
            label = "Term"
            [key.action]
            type = "app"
            command = "Alacritty.desktop"
        "#;
        let c: Config = toml::from_str(old).unwrap();
        assert!(c.dials.is_empty());
        assert_eq!(c.keys.iter().count(), 1);
    }

    #[test]
    fn keys_only_config_saves_without_dials() {
        let mut c = Config::default();
        c.keys.get_mut(0).label = "Term".into();
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(!s.contains("dial"));
    }

    #[test]
    fn dials_roundtrip() {
        let mut c = Config::default();
        let d = c.dial_mut(0);
        d.label = "Workspace".into();
        d.turn_left = Action::Hyprland { dispatch: "workspace e-1".into() };
        d.turn_right = Action::Hyprland { dispatch: "workspace e+1".into() };
        d.press = Action::App { command: "walker".into() };
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains("[[dial]]"));
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn dial_with_only_press_saves_minimally() {
        let mut c = Config::default();
        c.dial_mut(2).press = Action::App { command: "walker".into() };
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(!s.contains("turn_left"));
        assert!(!s.contains("turn_right"));
        assert!(s.contains("index = 2"));
    }

    #[test]
    fn example_parses() {
        let s = include_str!("../../../examples/config.toml");
        let c: Config = toml::from_str(s).unwrap();
        assert!(!c.keys.is_empty());
    }

    #[test]
    fn example_dial_parses() {
        let s = include_str!("../../../examples/config.toml");
        let start = s.find("# [[dial]]").expect("a commented [[dial]] block");
        let (before, block) = s.split_at(start);
        let uncommented = block.lines().map(|l| l.strip_prefix("# ").unwrap_or(l)).collect::<Vec<_>>().join("\n");
        let c: Config = toml::from_str(&format!("{before}{uncommented}"))
            .expect("the commented dial block, uncommented, is valid config");
        assert_eq!(c.dials.len(), 1);
        assert!(!c.dials[0].turn_left.is_none());
    }
}
