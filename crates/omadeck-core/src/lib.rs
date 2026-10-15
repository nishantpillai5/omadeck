//! Shared core of omadeck: configuration, Omarchy theme integration, key rendering,
//! action execution, device discovery and the daemon event protocol.

pub mod action;
pub mod audio;
pub mod config;
pub mod device;
pub mod ipc;
pub mod paths;
pub mod render;
pub mod state;
pub mod theme;

pub use config::{Action, AudioDevice, Config, DialConfig, HOME, KeyConfig, Keys, LevelSource, Page, Style};
pub use device::DeckInfo;
pub use render::{Align, DialStatus, Face, Level, PANEL_W, Renderer, tap_to_dial};
pub use theme::{Rgb, Theme};
