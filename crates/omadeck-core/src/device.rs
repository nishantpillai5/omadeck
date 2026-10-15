//! Device discovery helpers on top of the `elgato-streamdeck` crate.

use serde::{Deserialize, Serialize};

pub use elgato_streamdeck::info::Kind;
pub use elgato_streamdeck::{
    DeviceStateReader, DeviceStateUpdate, StreamDeck, list_devices, new_hidapi, refresh_device_list,
};
pub use hidapi::HidApi;

/// What the configurator needs to know to draw a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckInfo {
    pub model: String,
    pub serial: String,
    pub rows: u8,
    pub cols: u8,
    pub key_size: u32,
    /// Rotary dials under the keys (Stream Deck +).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dials: u8,
    /// Touch strip above the dials, in pixels (width, height).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strip: Option<(u32, u32)>,
}

impl DeckInfo {
    pub fn new(kind: Kind, serial: &str) -> Self {
        let fmt = kind.key_image_format();
        Self {
            model: model_name(kind).into(),
            serial: serial.into(),
            rows: kind.row_count(),
            cols: kind.column_count(),
            key_size: fmt.size.0 as u32,
            dials: kind.encoder_count(),
            // "strip" means the strip above the dials, so exclude the Neo's info bar.
            strip: kind.lcd_strip_size().filter(|_| kind.encoder_count() > 0).map(|(w, h)| (w as u32, h as u32)),
        }
    }

    pub fn key_count(&self) -> u8 {
        self.rows * self.cols
    }

    /// Layout used when no deck is around: the classic 15-key Stream Deck.
    pub fn fallback() -> Self {
        Self {
            model: "Stream Deck".into(),
            serial: String::new(),
            rows: 3,
            cols: 5,
            key_size: 72,
            dials: 0,
            strip: None,
        }
    }
}

fn is_zero(v: &u8) -> bool {
    *v == 0
}

pub fn model_name(kind: Kind) -> &'static str {
    match kind {
        Kind::Original => "Stream Deck",
        Kind::OriginalV2 => "Stream Deck (V2)",
        Kind::Mini | Kind::MiniMk2 | Kind::MiniMk2Module => "Stream Deck Mini",
        Kind::MiniDiscord => "Stream Deck Mini (Discord)",
        Kind::Xl | Kind::XlV2 | Kind::XlV2Module => "Stream Deck XL",
        Kind::Mk2 | Kind::Mk2Module => "Stream Deck MK.2",
        Kind::Mk2Scissor => "Stream Deck MK.2 (Scissor)",
        Kind::Neo => "Stream Deck Neo",
        Kind::Pedal => "Stream Deck Pedal",
        Kind::Plus => "Stream Deck +",
        Kind::PlusXl => "Stream Deck + XL",
    }
}

/// Enumerate attached decks without opening them (safe while the daemon owns the device).
pub fn detect() -> Vec<DeckInfo> {
    match new_hidapi() {
        Ok(hid) => list_devices(&hid).into_iter().map(|(k, s)| DeckInfo::new(k, &s)).collect(),
        Err(e) => {
            log::warn!("hidapi: {e}");
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_json_parses_without_dials_and_strip() {
        let old = r#"{"model":"Stream Deck +","serial":"ABC","rows":2,"cols":4,"key_size":120}"#;
        let info: DeckInfo = serde_json::from_str(old).unwrap();
        assert_eq!(info.dials, 0);
        assert_eq!(info.strip, None);
    }

    #[test]
    fn plus_roundtrips() {
        let info = DeckInfo::new(Kind::Plus, "ABC");
        assert_eq!(info.dials, 4);
        assert_eq!(info.strip, Some((800, 100)));
        let json = serde_json::to_string(&info).unwrap();
        assert_eq!(serde_json::from_str::<DeckInfo>(&json).unwrap(), info);
    }

    #[test]
    fn neo_has_no_strip() {
        // The Neo's info bar is not the strip above dials.
        let info = DeckInfo::new(Kind::Neo, "ABC");
        assert_eq!(info.dials, 0);
        assert_eq!(info.strip, None);
    }

    #[test]
    fn fallback_has_no_dials_or_strip() {
        let info = DeckInfo::fallback();
        assert_eq!(info.dials, 0);
        assert_eq!(info.strip, None);
    }
}
