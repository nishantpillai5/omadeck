//! The on-disk configuration (`~/.config/omadeck/config.toml`).

use std::fs;
use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::paths;
use crate::probe::Probe;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    /// Panel brightness, 0–100.
    #[serde(default = "default_brightness")]
    pub brightness: u8,
    /// Global look; per-key settings override it.
    #[serde(default)]
    pub style: Style,
    /// The Stream Deck + touch strip as a whole.
    #[serde(default, skip_serializing_if = "StripConfig::is_default")]
    pub strip: StripConfig,
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
            strip: StripConfig::default(),
            keys: Keys::default(),
            pages: Vec::new(),
            dials: Vec::new(),
        }
    }
}

/// The Stream Deck + touch strip as a whole (`[strip]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StripConfig {
    /// An image across the whole strip (800 × 100 on a Plus). Any size: it fills the
    /// strip and is centre-cropped. Relative paths resolve against the config dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
}

impl StripConfig {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// A named page of keys. A `folder` page is one you open from a key (it starts with a
/// Back key); flicks and `page_step` skip it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    #[serde(default, rename = "key", skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// Skipped by flicks and `page_step`.
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
    /// Show the busy face until this key's command exits. Only `app` (a plain command,
    /// not a `.desktop` id) and `script` actions; ignored for others.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub busy_while_running: bool,
    #[serde(default)]
    pub action: Action,
    /// A second face shown while the key is on, and where on/off comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<OnFace>,
    /// The look while busy (a state command's exit 75, or a running command).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy: Option<BusyFace>,
    /// A value the key shows and keeps up to date (PR 9). Last: TOML writes tables last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
}

/// The look and the source of a key's on state. Each unset look field falls back to the
/// key's own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnFace {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    /// A shell command: exit 0 = on, 75 = busy (its first output line is the progress),
    /// anything else = off. Unset: a volume mute key follows the mute; any other key
    /// flips on each press.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Seconds between runs of `state`; at least 1.
    #[serde(default = "two", skip_serializing_if = "is_two")]
    pub interval: u16,
    /// A slow toggle: a press shows busy at once and polls fast until the state moves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub slow: bool,
    /// Run this instead of the key's action while on (Elgato's Hotkey Switch).
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub action: Action,
}

impl Default for OnFace {
    // By hand: a derived Default would give `interval` 0.
    fn default() -> Self {
        Self {
            label: None,
            icon: None,
            glyph: None,
            background: None,
            label_color: None,
            state: None,
            interval: 2,
            slow: false,
            action: Action::None,
        }
    }
}

/// The look while busy.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BusyFace {
    /// Unset: the progress text, else "…".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label_color: Option<String>,
    /// Draw a progress bar when the busy text has a percentage (the last `NN%`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bar: bool,
}

fn two() -> u16 {
    2
}

/// A value a key or a dial's strip panel shows and keeps up to date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Live {
    /// Local time and date, strftime format (jiff's strtime: %H %M %S %a %d %b %n …).
    Clock {
        #[serde(default = "default_clock_format", skip_serializing_if = "is_default_clock_format")]
        format: String,
    },
    /// Average over all cores, %.
    Cpu,
    /// (MemTotal − MemAvailable) / MemTotal, %.
    Memory,
    /// System battery %, and whether it is charging.
    Battery,
    /// Hyprland's active workspace name.
    Workspace,
    /// The first line of a command's stdout (`sh -c`), re-run every `interval` seconds.
    Command {
        command: String,
        #[serde(default = "default_live_interval", skip_serializing_if = "is_default_live_interval")]
        interval: u16,
    },
}

fn default_clock_format() -> String {
    "%H:%M".into()
}
fn is_default_clock_format(f: &String) -> bool {
    f == "%H:%M"
}
fn default_live_interval() -> u16 {
    5
}
fn is_default_live_interval(n: &u16) -> bool {
    *n == 5
}

impl Live {
    /// For the configurator's "Shows" combo; index 0 is "no live value".
    pub const KINDS: [&'static str; 7] =
        ["Nothing", "Clock", "CPU", "Memory", "Battery", "Active workspace", "Command output"];

    /// None → 0, Clock → 1 … Command → 6.
    pub fn kind_index(live: Option<&Live>) -> usize {
        match live {
            None => 0,
            Some(Live::Clock { .. }) => 1,
            Some(Live::Cpu) => 2,
            Some(Live::Memory) => 3,
            Some(Live::Battery) => 4,
            Some(Live::Workspace) => 5,
            Some(Live::Command { .. }) => 6,
        }
    }

    /// 0 → None; 1 → Clock { "%H:%M" }; 2 → Cpu; … 6 → Command { "", 5 }.
    pub fn of_kind(i: usize) -> Option<Live> {
        match i {
            1 => Some(Live::Clock { format: default_clock_format() }),
            2 => Some(Live::Cpu),
            3 => Some(Live::Memory),
            4 => Some(Live::Battery),
            5 => Some(Live::Workspace),
            6 => Some(Live::Command { command: String::new(), interval: default_live_interval() }),
            _ => None,
        }
    }

    /// Whether the strip panel draws `$B1` (with a bar) rather than `$A1`.
    pub fn has_bar(&self) -> bool {
        matches!(self, Live::Cpu | Live::Memory | Live::Battery)
    }

    /// The probe that feeds it. A command's interval is clamped to 1 s here, not on
    /// load, so a hand-written `0` saves as written.
    pub fn probe(&self) -> Probe {
        match self {
            Live::Clock { format } => Probe::Clock { format: format.clone() },
            Live::Cpu => Probe::Cpu,
            Live::Memory => Probe::Memory,
            Live::Battery => Probe::Battery,
            Live::Workspace => Probe::Workspace,
            Live::Command { command, interval } => {
                Probe::Output { command: command.clone(), interval_ms: (*interval).max(1) as u32 * 1000 }
            }
        }
    }
}
fn is_two(v: &u16) -> bool {
    *v == 2
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
            busy_while_running: false,
            action: Action::None,
            on: None,
            busy: None,
            live: None,
        }
    }

    /// True when the key carries nothing worth saving.
    pub fn is_blank(&self) -> bool {
        self.label.is_empty()
            && self.icon.is_none()
            && self.glyph.is_none()
            && self.background.is_none()
            && self.label_color.is_none()
            && !self.busy_while_running
            && self.action == Action::None
            && self.on.is_none()
            && self.busy.is_none()
            && self.live.is_none()
    }

    /// What the watcher polls for this key, if anything. A non-blank `on.state` wins;
    /// else a volume mute action follows its device; else `None` (a press toggle, or
    /// not a toggle).
    pub fn state_probe(&self) -> Option<Probe> {
        let on = self.on.as_ref()?;
        if let Some(cmd) = on.state.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            return Some(Probe::Command { command: cmd.to_string(), interval_ms: on.interval.max(1) as u32 * 1000 });
        }
        match &self.action {
            Action::Volume { device, mute: true, .. } => Some(Probe::Volume(*device)),
            _ => None,
        }
    }

    /// `on` is set and nothing is polled: flips on each press.
    pub fn is_press_toggle(&self) -> bool {
        self.on.is_some() && self.state_probe().is_none()
    }

    /// `on.slow` and the source is a command.
    pub fn is_slow_toggle(&self) -> bool {
        self.on.as_ref().is_some_and(|on| on.slow)
            && self.state_probe().is_some_and(|p| matches!(p, Probe::Command { .. }))
    }

    /// `on.action` while on and set, else `action`.
    pub fn action_for(&self, on: bool) -> &Action {
        match (on, self.on.as_ref()) {
            (true, Some(f)) if !f.action.is_none() => &f.action,
            _ => &self.action,
        }
    }

    /// Every action this key owns, nested ones included: `action`, then `on.action`.
    pub fn actions(&self) -> Vec<&Action> {
        let mut out = Vec::new();
        self.action.walk(&mut |a| out.push(a));
        if let Some(on) = &self.on {
            on.action.walk(&mut |a| out.push(a));
        }
        out
    }

    /// `actions`, mutably: `f` sees each one in the same order.
    pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
        self.action.walk_mut(f);
        if let Some(on) = &mut self.on {
            on.action.walk_mut(f);
        }
    }

    /// The face for a state. Off: `face()`. On: the on face merged over it. Busy: the
    /// busy face merged over the *off* face; label = busy.label, else the text, else
    /// "…"; `show_label` forced on.
    pub fn face_for<'a>(&'a self, t: &'a crate::probe::Toggle) -> crate::render::Face<'a> {
        use crate::probe::Toggle;
        let f = self.face();
        match t {
            Toggle::Off => f,
            Toggle::On => {
                // Only a key with `[key.on]` is ever on, but the configurator can draw a
                // stale state for a moment (a mirrored state, a face tab) after the on
                // face is removed: draw the off face then, never panic.
                let Some(on) = self.on.as_ref() else { return f };
                let mut f = merge_looks(
                    f,
                    on.icon.as_deref(),
                    on.glyph.as_deref(),
                    on.background.as_deref(),
                    on.label_color.as_deref(),
                );
                if let Some(l) = &on.label {
                    f.label = l.trim(); // Some("") = no label while on
                }
                f
            }
            Toggle::Busy(text) => {
                let busy = self.busy.as_ref();
                let mut f = merge_looks(
                    f,
                    busy.and_then(|b| b.icon.as_deref()),
                    busy.and_then(|b| b.glyph.as_deref()),
                    busy.and_then(|b| b.background.as_deref()),
                    busy.and_then(|b| b.label_color.as_deref()),
                );
                f.label = busy
                    .and_then(|b| b.label.as_deref())
                    .filter(|l| !l.trim().is_empty())
                    .unwrap_or(if text.trim().is_empty() { "…" } else { text.as_str() });
                f.show_label = true;
                f
            }
        }
    }
}

/// Where a toggle key's state comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChoice {
    /// No source of truth: it flips on each press.
    Press,
    /// A shell command's exit status, polled.
    Command,
    /// The mute a volume mute key already changes.
    Mute,
}

impl KeyConfig {
    /// The choices the State-from combo offers, in order.
    pub fn state_choices(&self) -> &'static [StateChoice] {
        match &self.action {
            Action::Volume { mute: true, .. } => &[StateChoice::Mute, StateChoice::Command],
            _ => &[StateChoice::Press, StateChoice::Command],
        }
    }

    /// `on.state` set (even blank) → Command; else Mute for a volume mute action; else
    /// Press. Only called for keys with an on face.
    pub fn state_choice(&self) -> StateChoice {
        let on = self.on.as_ref().expect("a state choice only exists with an on face");
        if on.state.is_some() {
            StateChoice::Command
        } else if matches!(self.action, Action::Volume { mute: true, .. }) {
            StateChoice::Mute
        } else {
            StateChoice::Press
        }
    }

    /// Command keeps any old text (an empty command is still the choice, just nothing to
    /// poll yet); Press and Mute clear `state` and `slow` (slow needs a command).
    pub fn set_state_choice(&mut self, c: StateChoice) {
        let on = self.on.get_or_insert_with(OnFace::default);
        match c {
            StateChoice::Command => {
                if on.state.as_deref().map(str::trim).is_none_or(str::is_empty) {
                    on.state = Some(String::new());
                }
            }
            StateChoice::Press | StateChoice::Mute => {
                on.state = None;
                on.slow = false;
            }
        }
    }
}

/// Which face of a key the configurator's editor is showing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FaceTab {
    #[default]
    Off,
    On,
    Busy,
}

impl FaceTab {
    /// The state a preview of this tab shows (Busy with a sample percentage, so the bar
    /// is visible too).
    pub fn preview(self) -> crate::probe::Toggle {
        use crate::probe::Toggle;
        match self {
            FaceTab::Off => Toggle::Off,
            FaceTab::On => Toggle::On,
            FaceTab::Busy => Toggle::Busy("Loading 42%".into()),
        }
    }
}

/// One of the look fields a face can carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookField {
    Label,
    Icon,
    Glyph,
    Background,
    LabelColor,
}

impl KeyConfig {
    /// The face tabs the editor offers for this key, in order: Off always, On for a
    /// toggle (a key with `[key.on]`), Busy for a slow toggle or a busy-while-running
    /// key. A tab not in here must never be shown or written to.
    pub fn face_tabs(&self) -> Vec<FaceTab> {
        let mut tabs = vec![FaceTab::Off];
        if self.on.is_some() {
            tabs.push(FaceTab::On);
        }
        if self.is_slow_toggle() || self.busy_while_running {
            tabs.push(FaceTab::Busy);
        }
        tabs
    }

    /// `tab` if this key offers it, else Off (the editor's tab outlived the face it
    /// showed: another key was selected, or "Two faces" was switched off).
    pub fn clamp_face_tab(&self, tab: FaceTab) -> FaceTab {
        if self.face_tabs().contains(&tab) { tab } else { FaceTab::Off }
    }

    /// The field as set on that tab's face; `None` when unset there.
    pub fn look(&self, tab: FaceTab, field: LookField) -> Option<String> {
        let on = self.on.as_ref();
        let busy = self.busy.as_ref();
        match (tab, field) {
            (FaceTab::Off, LookField::Label) => Some(self.label.clone()),
            (FaceTab::Off, LookField::Icon) => self.icon.clone(),
            (FaceTab::Off, LookField::Glyph) => self.glyph.clone(),
            (FaceTab::Off, LookField::Background) => self.background.clone(),
            (FaceTab::Off, LookField::LabelColor) => self.label_color.clone(),
            (FaceTab::On, LookField::Label) => on.and_then(|f| f.label.clone()),
            (FaceTab::On, LookField::Icon) => on.and_then(|f| f.icon.clone()),
            (FaceTab::On, LookField::Glyph) => on.and_then(|f| f.glyph.clone()),
            (FaceTab::On, LookField::Background) => on.and_then(|f| f.background.clone()),
            (FaceTab::On, LookField::LabelColor) => on.and_then(|f| f.label_color.clone()),
            (FaceTab::Busy, LookField::Label) => busy.and_then(|f| f.label.clone()),
            (FaceTab::Busy, LookField::Icon) => busy.and_then(|f| f.icon.clone()),
            (FaceTab::Busy, LookField::Glyph) => busy.and_then(|f| f.glyph.clone()),
            (FaceTab::Busy, LookField::Background) => busy.and_then(|f| f.background.clone()),
            (FaceTab::Busy, LookField::LabelColor) => busy.and_then(|f| f.label_color.clone()),
        }
    }

    /// Set the field on that tab's face; `None` or blank clears it (Off's label becomes
    /// ""). Setting a field on On or Busy creates that table if missing; clearing never
    /// does, and a busy face left with nothing set is dropped.
    pub fn set_look(&mut self, tab: FaceTab, field: LookField, v: Option<String>) {
        let Some(v) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string) else {
            // Blank clears.
            match (tab, field) {
                (FaceTab::Off, LookField::Label) => self.label.clear(),
                (FaceTab::Off, LookField::Icon) => self.icon = None,
                (FaceTab::Off, LookField::Glyph) => self.glyph = None,
                (FaceTab::Off, LookField::Background) => self.background = None,
                (FaceTab::Off, LookField::LabelColor) => self.label_color = None,
                // Clearing never creates a face: an `[key.on]` would make the key a
                // toggle, and an empty `[key.busy]` is only noise in the file.
                (FaceTab::On, f) => {
                    if let Some(on) = &mut self.on {
                        on.clear(f);
                    }
                }
                (FaceTab::Busy, f) => {
                    if let Some(busy) = &mut self.busy {
                        busy.clear(f);
                    }
                    if self.busy.as_ref().is_some_and(|b| *b == BusyFace::default()) {
                        self.busy = None;
                    }
                }
            }
            return;
        };
        match (tab, field) {
            (FaceTab::Off, LookField::Label) => self.label = v,
            (FaceTab::Off, LookField::Icon) => self.icon = Some(v),
            (FaceTab::Off, LookField::Glyph) => self.glyph = Some(v),
            (FaceTab::Off, LookField::Background) => self.background = Some(v),
            (FaceTab::Off, LookField::LabelColor) => self.label_color = Some(v),
            (FaceTab::On, f) => {
                let on = self.on.get_or_insert_with(OnFace::default);
                on.set(f, v);
            }
            (FaceTab::Busy, f) => {
                let busy = self.busy.get_or_insert_with(BusyFace::default);
                busy.set(f, v);
            }
        }
    }
}

impl OnFace {
    pub fn clear(&mut self, f: LookField) {
        match f {
            LookField::Label => self.label = None,
            LookField::Icon => self.icon = None,
            LookField::Glyph => self.glyph = None,
            LookField::Background => self.background = None,
            LookField::LabelColor => self.label_color = None,
        }
    }
    pub fn set(&mut self, f: LookField, v: String) {
        match f {
            LookField::Label => self.label = Some(v),
            LookField::Icon => self.icon = Some(v),
            LookField::Glyph => self.glyph = Some(v),
            LookField::Background => self.background = Some(v),
            LookField::LabelColor => self.label_color = Some(v),
        }
    }
}

impl BusyFace {
    pub fn clear(&mut self, f: LookField) {
        match f {
            LookField::Label => self.label = None,
            LookField::Icon => self.icon = None,
            LookField::Glyph => self.glyph = None,
            LookField::Background => self.background = None,
            LookField::LabelColor => self.label_color = None,
        }
    }
    pub fn set(&mut self, f: LookField, v: String) {
        match f {
            LookField::Label => self.label = Some(v),
            LookField::Icon => self.icon = Some(v),
            LookField::Glyph => self.glyph = Some(v),
            LookField::Background => self.background = Some(v),
            LookField::LabelColor => self.label_color = Some(v),
        }
    }
}

/// Merge one state's look over a face. Setting `icon` or `glyph` replaces both (one
/// image per state, as in Elgato; otherwise a base icon would always beat an on glyph).
fn merge_looks<'a>(
    mut f: crate::render::Face<'a>,
    icon: Option<&'a str>,
    glyph: Option<&'a str>,
    background: Option<&'a str>,
    label_color: Option<&'a str>,
) -> crate::render::Face<'a> {
    if icon.is_some() || glyph.is_some() {
        f.icon = icon;
        f.glyph = glyph;
    }
    if background.is_some() {
        f.background = background;
    }
    if label_color.is_some() {
        f.label_color = label_color;
    }
    f
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

/// One of a dial's three action slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Left,
    Right,
    Press,
}

impl Direction {
    /// Every slot, in the order the editor shows them. PR 16's long press is not a
    /// `Direction`; it joins `DialConfig::actions` on its own.
    pub const ALL: [Direction; 3] = [Direction::Left, Direction::Right, Direction::Press];
}

/// What a dial's strip panel shows a value for, if anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource {
    /// The level of an audio device, read by the daemon.
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
    /// keeps it in the state file, so this is never "run" like the other actions.
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

    /// Run by the daemon itself (page moves and the panel brightness), never by
    /// `action::run`. The one list: `action::run` refuses exactly these, and the
    /// configurator's "Try it now" is off for them.
    pub fn is_deck_owned(&self) -> bool {
        matches!(self, Action::Page { .. } | Action::Back | Action::PageStep { .. } | Action::Brightness { .. })
    }

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

    pub fn is_none(&self) -> bool {
        matches!(self, Action::None)
    }

    /// Visit this action and every action nested in it. The one recursive walk: PR 12
    /// adds an arm for a multi action's steps here, and every walker built on it
    /// (`KeyConfig::actions`, `Config::all_actions`, …) follows.
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Action)) {
        f(self);
    }

    /// `walk`, mutably. `f` sees the outer action first, then what's nested in it.
    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
        f(self);
    }

    /// True for actions that run a command we can watch while it runs (`busy_while_running`):
    /// a plain `app` command (not a `.desktop` id, which hands off to the desktop and
    /// returns at once) or a `script`.
    pub fn is_trackable(&self) -> bool {
        match self {
            Action::App { command } => {
                let c = command.trim();
                !c.ends_with(".desktop") || c.contains(char::is_whitespace)
            }
            Action::Script { .. } => true,
            _ => false,
        }
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

    /// The action a kind pick starts from in one of a dial's slots. Turning left gets
    /// the opposite sign of turning right (lower the volume or brightness, the previous
    /// page), and a Volume press mutes, as on Elgato's volume dial. Everything else is
    /// `empty_of_kind`.
    pub fn default_for(kind: usize, dir: Direction) -> Self {
        match (Self::empty_of_kind(kind), dir) {
            (Action::Volume { device, .. }, Direction::Press) => Action::Volume { device, change: 0, mute: true },
            (Action::Volume { device, change, mute }, Direction::Left) => {
                Action::Volume { device, change: -change, mute }
            }
            (Action::Brightness { change, set }, Direction::Left) => Action::Brightness { change: -change, set },
            (Action::PageStep { change }, Direction::Left) => Action::PageStep { change: -change },
            (act, _) => act,
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
    /// An image behind this dial's panel (200 × 100), over the strip image. Fills the
    /// panel and is centre-cropped. Relative paths resolve against the config dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
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
    /// A value the strip panel shows and keeps up to date (PR 9). Last: TOML writes
    /// tables last.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub live: Option<Live>,
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
            background_image: None,
            label_color: None,
            turn_left: Action::None,
            turn_right: Action::None,
            press: Action::None,
            live: None,
        }
    }

    /// True when the dial carries nothing worth saving.
    pub fn is_blank(&self) -> bool {
        self.label.is_empty()
            && self.show_label
            && self.icon.is_none()
            && self.glyph.is_none()
            && self.background.is_none()
            && self.background_image.is_none()
            && self.label_color.is_none()
            && self.turn_left.is_none()
            && self.turn_right.is_none()
            && self.press.is_none()
            && self.live.is_none()
    }

    /// The audio device this dial shows a level for: the device of the first `Volume`
    /// action among turn_right, turn_left and press. `None` for a non-volume dial.
    pub fn volume_device(&self) -> Option<AudioDevice> {
        self.level_source().and_then(|s| match s {
            LevelSource::Volume(dev) => Some(dev),
            LevelSource::Brightness => None,
        })
    }

    /// The action in slot `dir`.
    pub fn action(&self, dir: Direction) -> &Action {
        match dir {
            Direction::Left => &self.turn_left,
            Direction::Right => &self.turn_right,
            Direction::Press => &self.press,
        }
    }

    pub fn action_mut(&mut self, dir: Direction) -> &mut Action {
        match dir {
            Direction::Left => &mut self.turn_left,
            Direction::Right => &mut self.turn_right,
            Direction::Press => &mut self.press,
        }
    }

    /// Every action this dial owns, nested ones included, slot by slot in
    /// `Direction::ALL` order. PR 16 adds the long press here.
    pub fn actions(&self) -> Vec<&Action> {
        let mut out = Vec::new();
        for dir in Direction::ALL {
            self.action(dir).walk(&mut |a| out.push(a));
        }
        out
    }

    /// `actions`, mutably.
    pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
        for dir in Direction::ALL {
            self.action_mut(dir).walk_mut(f);
        }
    }

    /// What this dial's strip panel shows: the first `Volume` action's device (which
    /// wins), else `Brightness` when any of its actions is a brightness action. `None`
    /// when `live` is set: an explicit live value wins.
    pub fn level_source(&self) -> Option<LevelSource> {
        if self.live.is_some() {
            return None;
        }
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
            background_image: self.background_image.as_deref(),
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
            background_image: None,
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

    /// Add a page named after `base` (made unique) that starts with `first`, and return
    /// its name. A `folder` page is skipped by page steps and flicks; the configurator's
    /// **+** makes normal pages, "New page" on an Open-page action makes folders.
    pub fn add_page(&mut self, base: &str, folder: bool, first: KeyConfig) -> String {
        let name = self.unique_page_name(base);
        self.pages.push(Page { name: name.clone(), keys: Keys(vec![first]), folder });
        name
    }

    /// Whether the page `name` is a folder (skipped by page steps).
    pub fn is_folder(&self, name: &str) -> bool {
        self.pages.iter().any(|p| p.folder && p.name == name)
    }

    /// Mark the page `name` as a folder or a normal page.
    pub fn set_folder(&mut self, name: &str, folder: bool) {
        if let Some(p) = self.pages.iter_mut().find(|p| p.name == name) {
            p.folder = folder;
        }
    }

    /// Every action in the config, nested ones included: each key's (its action and its
    /// on action) on every page, then each dial's. Built on `KeyConfig::actions` and
    /// `DialConfig::actions`, so a new place an action can live is added there once.
    pub fn all_actions(&self) -> Vec<&Action> {
        let keys = self.keys.iter().chain(self.pages.iter().flat_map(|p| p.keys.iter()));
        keys.flat_map(KeyConfig::actions).chain(self.dials.iter().flat_map(DialConfig::actions)).collect()
    }

    /// `all_actions`, mutably: `f` sees each one in the same order.
    pub fn all_actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
        for k in self.keys.0.iter_mut().chain(self.pages.iter_mut().flat_map(|p| p.keys.0.iter_mut())) {
            k.actions_mut(f);
        }
        for d in &mut self.dials {
            d.actions_mut(f);
        }
    }

    /// How many actions (keys, their on actions, and dial turns and presses) open the
    /// page `name`.
    pub fn links_to(&self, name: &str) -> usize {
        self.all_actions().into_iter().filter(|a| matches!(a, Action::Page { page } if page == name)).count()
    }

    /// Rename a page and every action (key or dial) that opens it.
    pub fn rename_page(&mut self, old: &str, new: &str) {
        let new = new.trim();
        for p in &mut self.pages {
            if p.name == old {
                p.name = new.to_string();
            }
        }
        self.all_actions_mut(&mut |a| {
            if let Action::Page { page } = a
                && page == old
            {
                *page = new.to_string();
            }
        });
    }

    /// Delete a page. Keys and dials that opened it keep their look but no longer do
    /// anything.
    pub fn remove_page(&mut self, name: &str) {
        self.pages.retain(|p| p.name != name);
        self.all_actions_mut(&mut |a| {
            if matches!(a, Action::Page { page } if page == name) {
                *a = Action::None;
            }
        });
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
    fn deck_owned_kinds() {
        let owned: Vec<usize> =
            (0..Action::KINDS.len()).filter(|&k| Action::empty_of_kind(k).is_deck_owned()).collect();
        assert_eq!(owned, vec![6, 7, 9, 10]); // Open page, Go back, Next/previous page, Brightness
    }

    #[test]
    fn toggle_roundtrip_and_defaults() {
        let src = r#"
[[key]]
index = 11
label = "Night light"
[key.action]
type = "app"
command = "night-toggle"
[key.on]
label = "Night on"
state = "test -f /tmp/night"
slow = true
"#;
        let c = Config::parse(src).unwrap();
        let k = c.key(None, 11).unwrap();
        assert_eq!(k.on.as_ref().unwrap().interval, 2);
        assert_eq!(k.state_probe(), Some(Probe::Command { command: "test -f /tmp/night".into(), interval_ms: 2000 }));
        assert!(k.is_slow_toggle());
        assert!(!k.busy_while_running);
        let out = toml::to_string_pretty(&c).unwrap();
        assert!(!out.contains("interval"), "the default interval is not written");
        assert!(!out.contains("busy"), "no busy table or flag when unset");
        assert_eq!(Config::parse(&out).unwrap(), c);
    }

    #[test]
    fn old_config_writes_no_toggle_fields() {
        // The example's first toggle key is a live example; "old" means the part of the
        // file before it (what the config looked like before this PR).
        let src = include_str!("../../../examples/config.toml");
        let c = Config::parse(&src[..src.find("[[key]]\nindex = 10").unwrap()]).unwrap();
        let out = toml::to_string_pretty(&c).unwrap();
        assert!(!out.contains("[key.on]"), "{}", out);
        assert!(!out.contains("[key.busy]"));
        assert!(!out.contains("slow"));
        assert!(!out.contains("busy_while_running"));
        assert_eq!(Config::parse(&out).unwrap(), c, "old configs save unchanged");
    }

    #[test]
    fn key_with_only_on_is_not_blank() {
        let k = KeyConfig { on: Some(OnFace::default()), ..KeyConfig::new(0) };
        assert!(!k.is_blank());
        assert!(k.is_press_toggle(), "on with nothing to poll flips on each press");
        assert!(!k.is_slow_toggle());
    }

    #[test]
    fn state_probe_command_wins_over_mute() {
        let mut k = KeyConfig::new(0);
        k.action = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
        k.on = Some(OnFace { state: Some("test -f /tmp/x".into()), ..OnFace::default() });
        assert_eq!(k.state_probe(), Some(Probe::Command { command: "test -f /tmp/x".into(), interval_ms: 2000 }));
    }

    #[test]
    fn state_probe_mute_from_volume_action() {
        let k = KeyConfig {
            action: Action::Volume { device: AudioDevice::Input, change: 0, mute: true },
            on: Some(OnFace::default()),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.state_probe(), Some(Probe::Volume(AudioDevice::Input)));
        // A volume action that changes the level, not the mute: nothing to follow.
        let k = KeyConfig {
            action: Action::Volume { device: AudioDevice::Input, change: 5, mute: false },
            on: Some(OnFace::default()),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.state_probe(), None);
    }

    #[test]
    fn blank_state_is_press_toggle() {
        let k = KeyConfig { on: Some(OnFace { state: Some("   ".into()), ..OnFace::default() }), ..KeyConfig::new(0) };
        assert_eq!(k.state_probe(), None);
        assert!(k.is_press_toggle());
    }

    #[test]
    fn interval_clamped_to_one_second() {
        let k = KeyConfig {
            on: Some(OnFace { state: Some("true".into()), interval: 0, ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.state_probe(), Some(Probe::Command { command: "true".into(), interval_ms: 1000 }));
    }

    #[test]
    fn face_on_image_replaces_both() {
        let k = KeyConfig {
            label: "Mic".into(),
            icon: Some("a.png".into()),
            on: Some(OnFace { glyph: Some("f036d".into()), ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        let f = k.face_for(&crate::probe::Toggle::On);
        assert_eq!(f.icon, None);
        assert_eq!(f.glyph, Some("f036d"));
        // The base glyph survives when the on face sets no image at all.
        let k = KeyConfig {
            label: "Mic".into(),
            glyph: Some("f036c".into()),
            on: Some(OnFace { label: Some("Muted".into()), ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        let f = k.face_for(&crate::probe::Toggle::On);
        assert_eq!(f.glyph, Some("f036c"));
        assert_eq!(f.label, "Muted");
    }

    #[test]
    fn face_on_empty_label_hides_label() {
        let k = KeyConfig {
            label: "Mic".into(),
            on: Some(OnFace { label: Some(String::new()), ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.face_for(&crate::probe::Toggle::On).label, "");
    }

    #[test]
    fn face_for_busy_label_order() {
        let base = || KeyConfig { label: "LLM".into(), ..KeyConfig::new(0) };
        // No busy face: the text.
        let k = base();
        assert_eq!(k.face_for(&crate::probe::Toggle::Busy("Loading 43%".into())).label, "Loading 43%");
        // Empty text: "…".
        assert_eq!(k.face_for(&crate::probe::Toggle::Busy(String::new())).label, "…");
        // busy.label wins over the text.
        let k = KeyConfig { busy: Some(BusyFace { label: Some("Working".into()), ..BusyFace::default() }), ..base() };
        assert_eq!(k.face_for(&crate::probe::Toggle::Busy("43%".into())).label, "Working");
        // A blank busy.label falls through to the text.
        let k = KeyConfig { busy: Some(BusyFace { label: Some("  ".into()), ..BusyFace::default() }), ..base() };
        assert_eq!(k.face_for(&crate::probe::Toggle::Busy("43%".into())).label, "43%");
    }

    #[test]
    fn state_choice_follows_the_action_and_survives_an_empty_command() {
        let mut k = KeyConfig { on: Some(OnFace::default()), ..KeyConfig::new(0) };
        assert_eq!(k.state_choices(), &[StateChoice::Press, StateChoice::Command]);
        assert_eq!(k.state_choice(), StateChoice::Press);
        k.set_state_choice(StateChoice::Command);
        assert_eq!(k.state_choice(), StateChoice::Command, "an empty command is still the choice");
        assert_eq!(k.state_probe(), None, "but there is nothing to poll yet");
        k.on.as_mut().unwrap().slow = true;
        k.set_state_choice(StateChoice::Press);
        assert_eq!(k.on.as_ref().unwrap().state, None);
        assert!(!k.on.as_ref().unwrap().slow, "slow needs a command");
        k.action = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
        assert_eq!(k.state_choices(), &[StateChoice::Mute, StateChoice::Command]);
        assert_eq!(k.state_choice(), StateChoice::Mute);
    }

    #[test]
    fn set_look_targets_each_tab() {
        let mut k = KeyConfig { label: "Mic".into(), ..KeyConfig::new(0) };
        k.set_look(FaceTab::On, LookField::Label, Some("Muted".into()));
        assert_eq!(k.on.as_ref().unwrap().label.as_deref(), Some("Muted"), "On creates [key.on]");
        assert_eq!(k.label, "Mic", "the off face is untouched");
        k.set_look(FaceTab::Busy, LookField::Glyph, Some("f0bc8".into()));
        assert_eq!(k.look(FaceTab::Busy, LookField::Glyph).as_deref(), Some("f0bc8"));
        k.set_look(FaceTab::On, LookField::Label, Some("  ".into()));
        assert_eq!(k.look(FaceTab::On, LookField::Label), None, "blank clears");
        k.set_look(FaceTab::Off, LookField::Label, None);
        assert_eq!(k.label, "");
    }

    #[test]
    fn face_tabs_follow_the_key() {
        let plain = KeyConfig::new(0);
        assert_eq!(plain.face_tabs(), vec![FaceTab::Off]);
        assert_eq!(plain.clamp_face_tab(FaceTab::On), FaceTab::Off, "a stale On tab falls back");
        let toggle = KeyConfig { on: Some(OnFace::default()), ..KeyConfig::new(0) };
        assert_eq!(toggle.face_tabs(), vec![FaceTab::Off, FaceTab::On]);
        assert_eq!(toggle.clamp_face_tab(FaceTab::On), FaceTab::On);
        assert_eq!(toggle.clamp_face_tab(FaceTab::Busy), FaceTab::Off);
        let job = KeyConfig { busy_while_running: true, ..KeyConfig::new(0) };
        assert_eq!(job.face_tabs(), vec![FaceTab::Off, FaceTab::Busy]);
    }

    #[test]
    fn clearing_a_look_never_creates_a_face() {
        let mut k = KeyConfig::new(0);
        k.set_look(FaceTab::On, LookField::Glyph, None);
        k.set_look(FaceTab::Busy, LookField::Label, Some("  ".into()));
        assert_eq!((k.on.is_some(), k.busy.is_some()), (false, false), "a plain key stays plain");
        k.set_look(FaceTab::Busy, LookField::Label, Some("Wait".into()));
        k.set_look(FaceTab::Busy, LookField::Label, None);
        assert!(k.busy.is_none(), "an empty busy face is dropped");
        let mut k = KeyConfig { busy: Some(BusyFace { bar: true, ..BusyFace::default() }), ..KeyConfig::new(0) };
        k.set_look(FaceTab::Busy, LookField::Glyph, None);
        assert!(k.busy.is_some_and(|b| b.bar), "a busy face with a bar stays");
    }

    #[test]
    fn action_for_uses_on_action_only_when_on() {
        let k = KeyConfig {
            action: Action::Hyprland { dispatch: "togglefloating".into() },
            on: Some(OnFace { action: Action::Hyprland { dispatch: "workspace 1".into() }, ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.action_for(false), &Action::Hyprland { dispatch: "togglefloating".into() });
        assert_eq!(k.action_for(true), &Action::Hyprland { dispatch: "workspace 1".into() });
        // An unset on action falls back to the key's own.
        let k = KeyConfig {
            action: Action::Hyprland { dispatch: "togglefloating".into() },
            on: Some(OnFace::default()),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.action_for(true), &Action::Hyprland { dispatch: "togglefloating".into() });
    }

    #[test]
    fn slow_without_state_is_not_slow() {
        let k = KeyConfig { on: Some(OnFace { slow: true, ..OnFace::default() }), ..KeyConfig::new(0) };
        assert!(!k.is_slow_toggle(), "slow needs a command");
        assert!(k.is_press_toggle());
    }

    #[test]
    fn trackable_kinds() {
        assert!(Action::App { command: "firefox".into() }.is_trackable());
        assert!(Action::App { command: "sh -c 'a | b'".into() }.is_trackable());
        assert!(!Action::App { command: "org.gnome.Nautilus.desktop".into() }.is_trackable());
        assert!(Action::Script { path: "x.sh".into(), terminal: false }.is_trackable());
        assert!(!Action::Url { url: "x".into() }.is_trackable());
        assert!(!Action::Volume { device: AudioDevice::Output, change: 5, mute: false }.is_trackable());
        assert!(!Action::None.is_trackable());
    }

    #[test]
    fn dial_action_by_direction() {
        let mut d = DialConfig::new(0);
        *d.action_mut(Direction::Left) = Action::Back;
        *d.action_mut(Direction::Press) = Action::PageStep { change: 1 };
        assert_eq!(d.turn_left, Action::Back);
        assert_eq!(d.action(Direction::Left), &Action::Back);
        assert_eq!(d.action(Direction::Right), &Action::None);
        assert_eq!(d.action(Direction::Press), &d.press);
    }

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
    fn page_links_include_dials() {
        let mut c = Config::default();
        c.add_page("Media", false, KeyConfig::new(0));
        c.dials.push(DialConfig { press: Action::Page { page: "Media".into() }, ..DialConfig::new(0) });
        c.dials.push(DialConfig { turn_left: Action::Page { page: "Media".into() }, ..DialConfig::new(1) });
        assert_eq!(c.links_to("Media"), 2);
        c.rename_page("Media", "Music");
        assert_eq!(c.dial(0).unwrap().press, Action::Page { page: "Music".into() });
        assert_eq!(c.dial(1).unwrap().turn_left, Action::Page { page: "Music".into() });
        c.remove_page("Music");
        assert_eq!(c.dial(0).unwrap().press, Action::None);
        assert_eq!(c.links_to("Music"), 0);
    }

    #[test]
    fn page_links_include_on_actions() {
        let mut c = Config::default();
        c.add_page("Media", false, KeyConfig::new(0));
        let on = OnFace { action: Action::Page { page: "Media".into() }, ..OnFace::default() };
        *c.keys.get_mut(3) = KeyConfig { on: Some(on), ..KeyConfig::new(3) };
        assert_eq!(c.links_to("Media"), 1, "the on action counts");
        c.rename_page("Media", "Music");
        let on_action = |c: &Config| c.key(None, 3).and_then(|k| k.on.as_ref()).map(|o| o.action.clone());
        assert_eq!(on_action(&c), Some(Action::Page { page: "Music".into() }));
        c.remove_page("Music");
        assert_eq!(on_action(&c), Some(Action::None));
        assert_eq!(c.links_to("Music"), 0);
    }

    #[test]
    fn walkers_visit_every_slot_in_order() {
        let d = DialConfig { turn_left: Action::Back, press: Action::PageStep { change: 1 }, ..DialConfig::new(0) };
        assert_eq!(d.actions(), vec![&Action::Back, &Action::None, &Action::PageStep { change: 1 }]);
        let k = KeyConfig {
            action: Action::Back,
            on: Some(OnFace { action: Action::PageStep { change: -1 }, ..OnFace::default() }),
            ..KeyConfig::new(0)
        };
        assert_eq!(k.actions(), vec![&Action::Back, &Action::PageStep { change: -1 }]);
        assert_eq!(KeyConfig::new(0).actions(), vec![&Action::None], "no on face, no on action");
    }

    #[test]
    fn on_state_without_on_face_draws_the_off_face() {
        let k = KeyConfig { label: "Plain".into(), glyph: Some("A".into()), ..KeyConfig::new(0) };
        let on = k.face_for(&crate::probe::Toggle::On);
        let off = k.face_for(&crate::probe::Toggle::Off);
        assert_eq!((on.label, on.glyph, on.background), (off.label, off.glyph, off.background));
    }

    #[test]
    fn default_for_follows_direction() {
        let volume = Action::Volume { device: AudioDevice::Output, change: 5, mute: false }.kind_index();
        assert_eq!(
            Action::default_for(volume, Direction::Left),
            Action::Volume { device: AudioDevice::Output, change: -5, mute: false }
        );
        assert_eq!(
            Action::default_for(volume, Direction::Right),
            Action::Volume { device: AudioDevice::Output, change: 5, mute: false }
        );
        assert_eq!(
            Action::default_for(volume, Direction::Press),
            Action::Volume { device: AudioDevice::Output, change: 0, mute: true }
        );
        let step = Action::PageStep { change: 1 }.kind_index();
        assert_eq!(Action::default_for(step, Direction::Left), Action::PageStep { change: -1 });
        let bright = Action::Brightness { change: 5, set: None }.kind_index();
        assert_eq!(Action::default_for(bright, Direction::Left), Action::Brightness { change: -5, set: None });
        for kind in 0..Action::KINDS.len() {
            assert_eq!(Action::default_for(kind, Direction::Right), Action::empty_of_kind(kind));
        }
    }

    #[test]
    fn added_pages_step_unless_folder() {
        let mut c = Config::default();
        assert_eq!(c.add_page("Work", false, KeyConfig::new(0)), "Work");
        assert_eq!(c.add_page("Work", true, KeyConfig::new(0)), "Work 2");
        assert!(c.is_folder("Work 2") && !c.is_folder("Work"));
        assert_eq!(c.step_page(None, 1), Some(Some("Work".into())));
        assert_eq!(c.step_page(Some("Work"), 1), Some(None), "the folder is skipped");
        c.set_folder("Work 2", false);
        assert_eq!(c.step_page(Some("Work"), 1), Some(Some("Work 2".into())));
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
    fn strip_background_roundtrip() {
        let mut c = Config::default();
        c.strip.background_image = Some("icons/aurora-strip.png".into());
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains("[strip]"), "{s}");
        assert!(s.contains("background_image = \"icons/aurora-strip.png\""), "{s}");
        assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    }

    #[test]
    fn old_config_saves_without_strip() {
        let mut c = Config::default();
        c.keys.get_mut(0).label = "Term".into();
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(!s.contains("[strip]"), "{s}");
        assert!(!s.contains("background_image"), "{s}");
        // Load, then save, is byte-identical for a config written before this field.
        let back: Config = toml::from_str(&s).unwrap();
        assert_eq!(toml::to_string_pretty(&back).unwrap(), s);
    }

    #[test]
    fn dial_background_image_roundtrip() {
        let d = DialConfig { background_image: Some("icons/waves.png".into()), ..DialConfig::new(1) };
        let s = toml::to_string_pretty(&d).unwrap();
        assert!(s.contains("background_image = \"icons/waves.png\""), "{s}");
        assert_eq!(toml::from_str::<DialConfig>(&s).unwrap(), d);
    }

    #[test]
    fn dial_with_only_background_image_is_not_blank() {
        let d = DialConfig { background_image: Some("icons/waves.png".into()), ..DialConfig::new(0) };
        assert!(!d.is_blank());
        assert!(DialConfig::new(0).is_blank());
    }

    #[test]
    fn example_parses() {
        let s = include_str!("../../../examples/config.toml");
        let c: Config = toml::from_str(s).unwrap();
        assert!(!c.keys.is_empty());
        assert!(c.keys.iter().any(|k| k.on.is_some()), "the example shows a toggle key");
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
        assert_eq!(blocks.len(), 4, "four dial examples");
        let uncommented = blocks
            .iter()
            .map(|b| b.lines().map(|l| l.strip_prefix("# ").unwrap_or(l)).collect::<Vec<_>>().join("\n"))
            .collect::<Vec<_>>()
            .join("\n");
        let c: Config = toml::from_str(&format!("{before}{uncommented}"))
            .expect("the commented dial blocks, uncommented, are valid config");
        assert_eq!(c.dials.len(), 4);
        assert!(!c.dials[0].turn_left.is_none());
        assert!(matches!(c.dials[1].turn_right, Action::Volume { .. }));
        assert!(matches!(c.dials[2].turn_right, Action::Brightness { .. }));
        assert!(c.dials[3].live.is_some());
    }

    #[test]
    fn live_key_round_trips() {
        for live in [
            Live::Clock { format: "%H:%M%n%a".into() },
            Live::Cpu,
            Live::Memory,
            Live::Battery,
            Live::Workspace,
            Live::Command { command: "date +%s".into(), interval: 30 },
        ] {
            let mut c = Config::default();
            c.keys.get_mut(3).live = Some(live.clone());
            let s = toml::to_string_pretty(&c).unwrap();
            assert!(s.contains("[key.live]"), "{s}");
            assert_eq!(toml::from_str::<Config>(&s).unwrap().key(None, 3).unwrap().live, Some(live));
        }
    }

    #[test]
    fn live_after_on_and_busy_tables() {
        let k = KeyConfig {
            on: Some(OnFace::default()),
            busy: Some(BusyFace::default()),
            live: Some(Live::Cpu),
            ..KeyConfig::new(0)
        };
        let s = toml::to_string_pretty(&Config { keys: Keys(vec![k]), ..Config::default() }).unwrap();
        assert_eq!(Config::parse(&s).unwrap().key(None, 0).unwrap().live, Some(Live::Cpu));
    }

    #[test]
    fn clock_default_format_not_written() {
        let d = DialConfig { live: Some(Live::Clock { format: "%H:%M".into() }), ..DialConfig::new(0) };
        let s = toml::to_string_pretty(&d).unwrap();
        assert!(s.contains("source = \"clock\""), "{s}");
        assert!(!s.contains("format"), "{s}");
        let back: DialConfig = toml::from_str("index = 0\nlive = { source = \"clock\" }\n").unwrap();
        assert_eq!(back.live, Some(Live::Clock { format: "%H:%M".into() }));
    }

    #[test]
    fn command_interval_default_not_written() {
        let k = KeyConfig { live: Some(Live::Command { command: "x".into(), interval: 5 }), ..KeyConfig::new(0) };
        let s = toml::to_string_pretty(&k).unwrap();
        assert!(!s.contains("interval"), "{s}");
        let back: KeyConfig = toml::from_str("index = 0\nlive = { source = \"command\", command = \"x\" }\n").unwrap();
        assert_eq!(back.live, Some(Live::Command { command: "x".into(), interval: 5 }));
    }

    #[test]
    fn old_config_saves_byte_identical() {
        let src = include_str!("../../../examples/config.toml");
        let once = toml::to_string_pretty(&toml::from_str::<Config>(src).unwrap()).unwrap();
        assert!(!once.contains("live"), "{once}");
        let twice = toml::to_string_pretty(&toml::from_str::<Config>(&once).unwrap()).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn key_with_only_live_is_not_blank() {
        assert!(!KeyConfig { live: Some(Live::Cpu), ..KeyConfig::new(0) }.is_blank());
        assert!(!DialConfig { live: Some(Live::Cpu), ..DialConfig::new(0) }.is_blank());
    }

    #[test]
    fn dial_live_wins_over_level_source() {
        let mut d = DialConfig {
            turn_right: Action::Volume { device: AudioDevice::Output, change: 5, mute: false },
            ..DialConfig::new(0)
        };
        assert_eq!(d.level_source(), Some(LevelSource::Volume(AudioDevice::Output)));
        d.live = Some(Live::Battery);
        assert_eq!(d.level_source(), None);
        assert_eq!(d.volume_device(), None);
    }

    #[test]
    fn command_live_probe_clamps_interval() {
        let live = Live::Command { command: "x".into(), interval: 0 };
        assert_eq!(live.probe(), Probe::Output { command: "x".into(), interval_ms: 1000 });
        let k = KeyConfig { live: Some(live.clone()), ..KeyConfig::new(0) };
        let s = toml::to_string_pretty(&k).unwrap();
        assert!(s.contains("interval = 0"), "{s}");
        assert_eq!(toml::from_str::<KeyConfig>(&s).unwrap().live, Some(live));
    }

    #[test]
    fn live_kinds_round_trip() {
        for i in 0..Live::KINDS.len() {
            assert_eq!(Live::kind_index(Live::of_kind(i).as_ref()), i);
        }
        assert!(Live::Battery.has_bar() && Live::Cpu.has_bar() && Live::Memory.has_bar());
        assert!(!Live::Workspace.has_bar() && !Live::of_kind(1).unwrap().has_bar());
    }
}
