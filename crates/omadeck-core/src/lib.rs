//! Shared core of omadeck: configuration, Omarchy theme integration, key rendering,
//! action execution, device discovery and the daemon event protocol.

pub mod action;
pub mod audio;
pub mod config;
pub mod device;
pub mod ipc;
pub mod paths;
pub mod probe;
pub mod proc;
pub mod render;
pub mod state;
pub mod theme;

pub use config::{
    Action, AudioDevice, BusyFace, Config, DialConfig, Direction, FaceTab, HOME, KeyConfig, Keys, LevelSource,
    LookField, OnFace, Page, StateChoice, Style,
};
pub use device::DeckInfo;
pub use render::{Align, DialStatus, Face, KeyStatus, Level, PANEL_W, PanelStatus, Renderer, tap_to_dial};
pub use theme::{Rgb, Theme};
