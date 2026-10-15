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
    /// An action failed to start.
    ActionFailed {
        index: u8,
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Show a page on the deck (`None` is home), e.g. the one being edited.
    ShowPage { page: Option<String> },
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
