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
    /// A folder: opened from a key, with a Back key. Folders are skipped by page steps,
    /// so a flick can't land inside one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
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

fn is_zero_i8(v: &i8) -> bool {
    *v == 0
}

fn default_step() -> i8 {
    1
}

fn is_default_step(v: &i8) -> bool {
    *v == 1
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

/// Which audio device a volume action works on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioDevice {
    #[default]
    Output,
    Input,
}

impl AudioDevice {
    pub fn is_output(&self) -> bool {
        matches!(self, AudioDevice::Output)
    }
}

/// What a dial's strip panel shows a value for, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource {
    /// The level of an audio device, polled by the daemon's audio worker.
    Volume(AudioDevice),
    /// The deck's panel brightness, kept in the daemon itself.
    Brightness,
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
    /// Change the system volume, through Omarchy's volume commands (so its pop-up shows)
    /// or `wpctl`.
    Volume {
        /// The output (default) or the input (microphone).
        #[serde(default, skip_serializing_if = "AudioDevice::is_output")]
        device: AudioDevice,
        /// Percent to change by, signed. Ignored when `mute` is set.
        #[serde(default, skip_serializing_if = "is_zero_i8")]
        change: i8,
        /// Toggle mute instead of changing the level.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        mute: bool,
    },
    /// Move to the next or previous page; on a dial, per turn.
    PageStep {
        /// How many pages to move, signed. 1 is next.
        #[serde(default = "default_step", skip_serializing_if = "is_default_step")]
        change: i8,
    },
    /// Change the deck's panel brightness, 0–100. The daemon owns the current value and
    /// persists it to the config, so this is never "run" like the other actions.
    Brightness {
        /// Percent to change by, signed (per detent on a dial).
        #[serde(default, skip_serializing_if = "is_zero_i8")]
        change: i8,
        /// Set this absolute brightness instead of changing it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<u8>,
    },
}

impl Action {
    pub const KINDS: [&'static str; 11] = [
        "Nothing",
        "Launch app",
        "Run script",
        "Play sound",
        "Open website",
        "Hyprland",
        "Open page",
        "Go back",
        "Volume",
        "Next/previous page",
        "Brightness",
    ];

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
            Action::Volume { .. } => 8,
            Action::PageStep { .. } => 9,
            Action::Brightness { .. } => 10,
        }
    }

    /// Handled by whoever owns the deck (page moves, later brightness), not by `action::run`.
    pub fn is_deck_owned(&self) -> bool {
        matches!(self, Action::Page { .. } | Action::Back | Action::PageStep { .. })
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
            8 => Action::Volume { device: AudioDevice::Output, change: 5, mute: false },
            9 => Action::PageStep { change: 1 },
            10 => Action::Brightness { change: 5, set: None },
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
            Action::Volume { device, change, mute } => volume_summary(*device, *change, *mute),
            Action::PageStep { change } => match *change {
                1 => "Next page".into(),
                -1 => "Previous page".into(),
                c if c > 0 => format!("Page +{c}"),
                c => format!("Page {c}"),
            },
            Action::Brightness { change, set } => brightness_summary(*change, *set),
        }
    }
}

fn volume_summary(device: AudioDevice, change: i8, mute: bool) -> String {
    let mic = !device.is_output();
    if mute {
        return if mic { "Toggle mute (mic)".into() } else { "Toggle mute".into() };
    }
    let sign = if change >= 0 { "+" } else { "" };
    if mic { format!("Mic {sign}{change}%") } else { format!("Volume {sign}{change}%") }
}

fn brightness_summary(change: i8, set: Option<u8>) -> String {
    if let Some(v) = set {
        return format!("Brightness {v}%");
    }
    format!("Brightness {}{}%", if change >= 0 { "+" } else { "" }, change)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialConfig {
    /// Zero-based dial index, left to right.
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
        Self {
            index,
            label: String::new(),
            show_label: true,
            icon: None,
            glyph: None,
            background: None,
            label_color: None,
            turn_left: Action::None,
            turn_right: Action::None,
            press: Action::None,
        }
    }

    /// True when the dial carries nothing worth saving.
    pub fn is_blank(&self) -> bool {
        self.label.is_empty()
            && self.show_label
            && self.icon.is_none()
            && self.glyph.is_none()
            && self.background.is_none()
            && self.label_color.is_none()
            && self.turn_left.is_none()
            && self.turn_right.is_none()
            && self.press.is_none()
    }

    /// The audio device this dial shows a level for: the device of the first `Volume`
    /// action among turn_right, turn_left and press. `None` for a non-volume dial.
    pub fn volume_device(&self) -> Option<AudioDevice> {
        self.level_source().and_then(|s| match s {
            LevelSource::Volume(dev) => Some(dev),
            LevelSource::Brightness => None,
        })
    }

    /// What this dial's strip panel shows: the first `Volume` action's device (which
    /// wins), else `Brightness` when any of its actions is a brightness action.
    pub fn level_source(&self) -> Option<LevelSource> {
        let actions = [&self.turn_right, &self.turn_left, &self.press];
        if let Some(dev) = actions.into_iter().find_map(|a| match a {
            Action::Volume { device, .. } => Some(*device),
            _ => None,
        }) {
            return Some(LevelSource::Volume(dev));
        }
        actions.iter().any(|a| matches!(a, Action::Brightness { .. })).then_some(LevelSource::Brightness)
    }

    /// The look fields, shared with the key face renderer.
    pub fn face(&self) -> crate::render::Face<'_> {
        crate::render::Face {
            label: self.label.trim(),
            show_label: self.show_label,
            icon: self.icon.as_deref(),
            glyph: self.glyph.as_deref(),
            background: self.background.as_deref(),
            label_color: self.label_color.as_deref(),
        }
    }
}

impl KeyConfig {
    /// The look fields, shared with the dial strip renderer.
    pub fn face(&self) -> crate::render::Face<'_> {
        crate::render::Face {
            label: self.label.trim(),
            show_label: self.show_label,
            icon: self.icon.as_deref(),
            glyph: self.glyph.as_deref(),
            background: self.background.as_deref(),
            label_color: self.label_color.as_deref(),
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

    /// The pages a flick or `page_step` moves through: Home first, then the non-folder
    /// pages in file order.
    fn steppable_pages(&self) -> Vec<Option<String>> {
        let mut pages = vec![None];
        pages.extend(self.pages.iter().filter(|p| !p.folder).map(|p| Some(p.name.clone())));
        pages
    }

    /// Step from `current` by `change` pages, wrapping both ways. `None` when there's
    /// nothing to step through. A `current` that isn't steppable (a folder, or gone)
    /// steps from Home.
    pub fn step_page(&self, current: Option<&str>, change: i32) -> Option<Option<String>> {
        let pages = self.steppable_pages();
        if pages.len() < 2 {
            return None;
        }
        let pos = current.and_then(|c| pages.iter().position(|p| p.as_deref() == Some(c))).unwrap_or(0);
        let next = (pos as i32 + change).rem_euclid(pages.len() as i32) as usize;
        Some(pages[next].clone())
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

/// What the home page is called in the UI. It has no name in the config file.
pub const HOME: &str = "Home";

/// The brightness a brightness action lands on, clamped to 0–100. An absolute `set`
/// wins over the relative `change`.
pub fn apply_brightness(current: u8, set: Option<u8>, change: i32) -> u8 {
    match set {
        Some(v) => v.min(100),
        None => (current as i32 + change).clamp(0, 100) as u8,
    }
}

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
        let mut media = Page { name: "Media".into(), keys: Keys::default(), folder: false };
        media.keys.get_mut(0).action = Action::Back;
        media.keys.get_mut(1).label = "Play".into();
        c.pages.push(media);
        c.pages.push(Page { name: "Empty".into(), keys: Keys::default(), folder: false });
        let s = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn step_page() {
        let mut c = Config::default();
        c.pages.push(Page { name: "A".into(), keys: Keys::default(), folder: false });
        c.pages.push(Page { name: "F".into(), keys: Keys::default(), folder: true });
        c.pages.push(Page { name: "B".into(), keys: Keys::default(), folder: false });

        // Home, A, B: folders are skipped.
        assert_eq!(c.step_page(None, 1), Some(Some("A".into())));
        assert_eq!(c.step_page(Some("A"), 1), Some(Some("B".into())));
        // Wraps both ways.
        assert_eq!(c.step_page(Some("B"), 1), Some(None));
        assert_eq!(c.step_page(None, -1), Some(Some("B".into())));
        // Multi-step.
        assert_eq!(c.step_page(None, 2), Some(Some("B".into())));
        assert_eq!(c.step_page(Some("B"), -3), Some(Some("B".into())));
        // A folder (or a gone page) steps from Home.
        assert_eq!(c.step_page(Some("F"), 1), Some(Some("A".into())));
        assert_eq!(c.step_page(Some("Gone"), -1), Some(Some("B".into())));

        // Nothing to step through: Home only, or Home with only folders.
        assert_eq!(Config::default().step_page(None, 1), None);
        let only_folders =
            Config { pages: vec![Page { name: "F".into(), keys: Keys::default(), folder: true }], ..Config::default() };
        assert_eq!(only_folders.step_page(None, 1), None);
    }

    #[test]
    fn page_step_roundtrip() {
        let mut c = Config::default();
        c.keys.get_mut(0).action = Action::PageStep { change: -2 };
        c.keys.get_mut(1).action = Action::PageStep { change: 1 };
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains(r#"type = "page_step""#));
        assert!(!s.contains("change = 1"), "the default step is not written");
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
        assert_eq!(Action::PageStep { change: 1 }.summary(), "Next page");
        assert_eq!(Action::PageStep { change: -1 }.summary(), "Previous page");
        assert_eq!(Action::PageStep { change: 3 }.summary(), "Page +3");
    }

    #[test]
    fn folder_flag_is_optional() {
        let c: Config = toml::from_str("[[page]]\nname = \"A\"").unwrap();
        assert!(!c.pages[0].folder, "a hand-written page without the flag is a normal page");
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(!s.contains("folder"), "the flag is not written when false");
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
        c.pages.push(Page { name: "Media".into(), keys: Keys::default(), folder: false });
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
        c.pages.push(Page { name: "P".into(), keys: Keys::default(), folder: false });
        c.page_mut(Some("P")).unwrap().get_mut(3);
        c.save_to(&path).unwrap();
        let back = Config::load_from(&path).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(back.keys.iter().map(|k| k.index).collect::<Vec<_>>(), [1, 4]);
        assert!(back.page(Some("P")).unwrap().is_empty(), "blank keys are dropped, empty pages kept");
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
    fn volume_action_roundtrip_and_defaults() {
        let mut c = Config::default();
        let d = c.dial_mut(0);
        d.label = "Volume".into();
        d.turn_left = Action::Volume { device: AudioDevice::Output, change: -5, mute: false };
        d.turn_right = Action::Volume { device: AudioDevice::Output, change: 5, mute: false };
        d.press = Action::Volume { device: AudioDevice::Output, change: 0, mute: true };
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains("type = \"volume\""));
        // The default device and the zero/false fields are left out.
        assert!(!s.contains("device"));
        assert!(!s.contains("mute = false"));
        assert!(s.contains("change = -5") && s.contains("change = 5") && s.contains("mute = true"));
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn volume_input_device_is_saved() {
        let mut c = Config::default();
        c.dial_mut(1).press = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains("device = \"input\""));
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn volume_device_picks_first() {
        let d = DialConfig {
            turn_right: Action::Volume { device: AudioDevice::Output, change: 5, mute: false },
            press: Action::Volume { device: AudioDevice::Input, change: 0, mute: true },
            ..DialConfig::new(0)
        };
        assert_eq!(d.volume_device(), Some(AudioDevice::Output));
        let d = DialConfig {
            press: Action::Volume { device: AudioDevice::Input, change: 0, mute: true },
            ..DialConfig::new(0)
        };
        assert_eq!(d.volume_device(), Some(AudioDevice::Input));
        assert_eq!(DialConfig::new(0).volume_device(), None);
    }

    #[test]
    fn brightness_action_roundtrip_and_summaries() {
        let a = Action::Brightness { change: -5, set: None };
        let s = toml::to_string_pretty(&a).unwrap();
        assert!(s.contains("type = \"brightness\""));
        assert!(s.contains("change = -5"));
        assert!(!s.contains("set"), "{s}");
        assert_eq!(toml::from_str::<Action>(&s).unwrap(), a);

        let a = Action::Brightness { change: 0, set: Some(40) };
        let s = toml::to_string_pretty(&a).unwrap();
        assert!(s.contains("set = 40"));
        assert!(!s.contains("change"), "zero change is omitted: {s}");
        assert_eq!(toml::from_str::<Action>(&s).unwrap(), a);

        assert_eq!(Action::empty_of_kind(10), Action::Brightness { change: 5, set: None });
        assert_eq!(Action::Brightness { change: 0, set: Some(40) }.summary(), "Brightness 40%");
        assert_eq!(Action::Brightness { change: -5, set: None }.summary(), "Brightness -5%");
        assert_eq!(Action::Brightness { change: 5, set: None }.summary(), "Brightness +5%");
    }

    #[test]
    fn apply_brightness_clamps_and_set_wins() {
        assert_eq!(apply_brightness(70, None, 5), 75);
        assert_eq!(apply_brightness(70, None, -5), 65);
        assert_eq!(apply_brightness(95, None, 10), 100, "clamps at the top");
        assert_eq!(apply_brightness(5, None, -10), 0, "clamps at the bottom");
        assert_eq!(apply_brightness(70, Some(40), 5), 40, "set wins over change");
        assert_eq!(apply_brightness(70, Some(200), 0), 100, "set is clamped too");
        assert_eq!(apply_brightness(70, None, 0), 70, "no change, no move");
    }

    #[test]
    fn level_source_volume_wins_over_brightness() {
        let d = DialConfig {
            turn_right: Action::Brightness { change: 5, set: None },
            press: Action::Volume { device: AudioDevice::Input, change: 0, mute: true },
            ..DialConfig::new(0)
        };
        assert_eq!(d.level_source(), Some(LevelSource::Volume(AudioDevice::Input)));
        let d = DialConfig { turn_right: Action::Brightness { change: 5, set: None }, ..DialConfig::new(0) };
        assert_eq!(d.level_source(), Some(LevelSource::Brightness));
        let d = DialConfig::new(0);
        assert_eq!(d.level_source(), None);
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
        let first = s.find("# [[dial]]").expect("a commented [[dial]] block");
        let (before, rest) = s.split_at(first);
        // Each dial block runs from its `# [[dial]]` to the next one or a blank comment line.
        let mut blocks: Vec<&str> = Vec::new();
        let mut idx = 0;
        while let Some(rel) = rest[idx..].find("# [[dial]]") {
            let start = idx + rel;
            let after = start + "# [[dial]]".len();
            let end_blank = rest[after..].find("\n#\n").map(|p| after + p);
            let end_next = rest[after..].find("# [[dial]]").map(|p| after + p);
            let end = match (end_blank, end_next) {
                (Some(b), Some(n)) => b.min(n),
                (b, n) => b.or(n).unwrap_or(rest.len()),
            };
            blocks.push(&rest[start..end]);
            idx = end;
        }
        assert_eq!(blocks.len(), 3, "three dial examples");
        let uncommented = blocks
            .iter()
            .map(|b| b.lines().map(|l| l.strip_prefix("# ").unwrap_or(l)).collect::<Vec<_>>().join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        let c: Config = toml::from_str(&format!("{before}{uncommented}"))
            .expect("the commented dial blocks, uncommented, are valid config");
        assert_eq!(c.dials.len(), 3);
        assert!(!c.dials[0].turn_left.is_none());
        assert!(matches!(c.dials[1].turn_right, Action::Volume { .. }));
        assert!(matches!(c.dials[2].turn_right, Action::Brightness { .. }));
    }
}
