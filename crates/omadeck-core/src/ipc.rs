//! Tiny protocol between the daemon and the configurator: newline-delimited JSON over a
//! Unix socket at `$XDG_RUNTIME_DIR/omadeck.sock`. The daemon sends events; clients may
//! send commands back.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::device::DeckInfo;
use crate::paths;
use crate::render::{KeyStatus, PanelStatus};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Sent on connect and whenever a deck is attached.
    Connected {
        deck: DeckInfo,
    },
    /// Sent on connect when no deck is attached, and on unplug.
    Disconnected,
    /// Sent on connect and whenever the deck shows another page (`None` is home).
    PageChanged {
        page: Option<String>,
    },
    KeyDown {
        index: u8,
        /// The page the key was on.
        #[serde(default)]
        page: Option<String>,
    },
    KeyUp {
        index: u8,
    },
    /// An action failed to start, or a tracked command exited non-zero. `page` is where
    /// the key is: a job can end after the deck moved on.
    ActionFailed {
        index: u8,
        #[serde(default)]
        page: Option<String>,
        message: String,
    },
    /// A dial turned; `delta` is signed (+ is clockwise) and may exceed ±1 on a fast spin.
    DialTurn {
        index: u8,
        delta: i8,
    },
    /// A dial is pressed down.
    DialDown {
        index: u8,
    },
    /// A dial is released.
    DialUp {
        index: u8,
    },
    /// The touch strip was tapped.
    StripTap {
        x: u16,
        y: u16,
    },
    /// The touch strip was pressed and held.
    StripLongPress {
        x: u16,
        y: u16,
    },
    /// The touch strip was swiped.
    StripSwipe {
        from: (u16, u16),
        to: (u16, u16),
    },
    /// A dial action failed to start. Separate from `ActionFailed` so key and dial
    /// indices never get mixed up; older configurators ignore it.
    DialActionFailed {
        index: u8,
        message: String,
    },
    /// The panel brightness, 0–100: on connect and whenever it changes.
    Brightness {
        value: u8,
    },
    /// What a dial's strip panel shows: on connect and whenever it changes.
    DialStatus {
        index: u8,
        status: PanelStatus,
    },
    /// What a key with a toggle, or with a running command, shows now. Published on each
    /// change (each new progress text too) and for the current page after connect,
    /// reload and page change; replayed to new clients. `pressed` is always false here,
    /// so it is never written.
    KeyState {
        #[serde(default)]
        page: Option<String>,
        index: u8,
        status: KeyStatus,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Show a page on the deck (`None` is home), e.g. the one being edited.
    ShowPage { page: Option<String> },
    /// Set the panel brightness, e.g. from the configurator's settings.
    SetBrightness { value: u8 },
}

fn line<T: Serialize>(v: &T) -> String {
    let mut s = serde_json::to_string(v).expect("message serializes");
    s.push('\n');
    s
}

impl Event {
    pub fn to_line(&self) -> String {
        line(self)
    }
}

/// The sending half of a connection to the daemon.
pub struct Sender(UnixStream);

impl Sender {
    pub fn send(&mut self, cmd: &Command) -> Result<()> {
        Ok(self.0.write_all(line(cmd).as_bytes())?)
    }
}

/// Connect to a running daemon: a sender for commands, and its events until it goes away.
pub fn connect() -> Result<(Sender, impl Iterator<Item = Event>)> {
    let stream = UnixStream::connect(paths::socket_path()?)?;
    let writer = stream.try_clone()?;
    writer.set_write_timeout(Some(Duration::from_millis(200)))?;
    let events = BufReader::new(stream).lines().map_while(|l| l.ok()).filter_map(|l| serde_json::from_str(&l).ok());
    Ok((Sender(writer), events))
}

/// Read commands sent by a client until it disconnects.
pub fn commands(stream: UnixStream) -> impl Iterator<Item = Command> {
    BufReader::new(stream).lines().map_while(|l| l.ok()).filter_map(|l| serde_json::from_str(&l).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(ev: &Event) {
        let line = ev.to_line();
        assert_eq!(serde_json::from_str::<Event>(&line).unwrap(), *ev);
    }

    #[test]
    fn state_events_roundtrip_and_wire_shape() {
        roundtrip(&Event::Brightness { value: 40 });
        roundtrip(&Event::DialStatus { index: 1, status: PanelStatus::default() });
        let level = Some(crate::Level { value: 0.5, text: "50%".into(), muted: false });
        roundtrip(&Event::DialStatus { index: 2, status: PanelStatus { level } });
        assert_eq!(Event::Brightness { value: 40 }.to_line(), "{\"event\":\"brightness\",\"value\":40}\n");
        assert_eq!(
            Event::DialStatus { index: 1, status: PanelStatus::default() }.to_line(),
            "{\"event\":\"dial_status\",\"index\":1,\"status\":{}}\n"
        );
    }

    #[test]
    fn key_state_wire_shape() {
        let on = KeyStatus { state: crate::probe::Toggle::On, ..KeyStatus::default() };
        assert_eq!(
            Event::KeyState { page: None, index: 3, status: on }.to_line(),
            "{\"event\":\"key_state\",\"page\":null,\"index\":3,\"status\":{\"state\":\"on\"}}\n"
        );
        let busy = KeyStatus { state: crate::probe::Toggle::Busy("Loading 43%".into()), ..KeyStatus::default() };
        roundtrip(&Event::KeyState { page: Some("Media".into()), index: 0, status: busy });
        // An old daemon's ActionFailed (no page) parses as page: None.
        let old: Event = serde_json::from_str("{\"event\":\"action_failed\",\"index\":2,\"message\":\"x\"}").unwrap();
        assert_eq!(old, Event::ActionFailed { index: 2, page: None, message: "x".into() });
    }

    #[test]
    fn dial_events_roundtrip() {
        roundtrip(&Event::DialTurn { index: 0, delta: -2 });
        roundtrip(&Event::DialDown { index: 3 });
        roundtrip(&Event::DialUp { index: 3 });
    }

    #[test]
    fn strip_events_roundtrip() {
        roundtrip(&Event::StripTap { x: 210, y: 40 });
        roundtrip(&Event::StripLongPress { x: 600, y: 50 });
        roundtrip(&Event::StripSwipe { from: (700, 50), to: (100, 48) });
    }

    #[test]
    fn dial_turn_wire_shape() {
        // The socket protocol is part of the contract with the configurator; pin it.
        assert_eq!(
            Event::DialTurn { index: 0, delta: -2 }.to_line(),
            "{\"event\":\"dial_turn\",\"index\":0,\"delta\":-2}\n"
        );
    }
}
