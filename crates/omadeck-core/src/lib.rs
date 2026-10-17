//! Shared core of omadeck: configuration, Omarchy theme integration, key rendering,
//! action execution, device discovery and the daemon event protocol.

pub mod action;
pub mod audio;
pub mod config;
pub mod device;
pub mod ipc;
pub mod live;
pub mod paths;
pub mod presets;
pub mod probe;
pub mod proc;
pub mod render;
pub mod state;
pub mod sysinfo;
pub mod theme;

pub use audio::{AppLevel, AppQuery};
pub use config::{
    Action, AudioDevice, BusyFace, Config, DialConfig, Direction, FaceTab, HOME, KeyConfig, Keys, LevelSource, Live,
    LookField, MediaCommand, OnFace, Page, StateChoice, Style,
};
pub use device::DeckInfo;
pub use presets::{PRESETS, Preset, apply_service, on_path};
pub use render::{
    Align, AppShow, DialStatus, Face, KeyStatus, Level, LiveShow, PANEL_W, PanelStatus, Renderer, tap_to_dial,
};
pub use theme::{Rgb, Theme};
