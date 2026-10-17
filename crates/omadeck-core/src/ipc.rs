//! Tiny protocol between the daemon and the configurator: newline-delimited JSON over a
//! Unix socket at `$XDG_RUNTIME_DIR/omadeck.sock`. The daemon sends events (tagged
//! `"event"`); clients may send commands back (tagged `"command"`). The control commands
//! (`omadeckd page`, `press`, …) each get exactly one reply (tagged `"reply"`) on the
//! connection that sent them.

use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};

use crate::config::{Action, Direction};
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
    /// What a key with a toggle, a running command or a live value shows now. Published on each
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
    /// Press key `index` as if by hand, but quietly: no `KeyDown`/`KeyUp`. `page: None`
    /// is the page the deck shows; a name (or "Home") that isn't the shown page runs that
    /// page's key without flashing anything.
    Press {
        index: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<String>,
    },
    /// Turn dial `index` by `steps` detents, or press it (like a tap on its panel).
    Dial {
        index: u8,
        dir: Direction,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        steps: u8,
    },
    /// Run an action on the daemon, deck-owned ones included (Try it now, `page next`).
    Run { action: Action },
    /// Show page `page` (`None` is home). Unlike `ShowPage`, a missing page is an error.
    Page { page: Option<String> },
    /// Re-read config and theme now.
    Reload,
    /// Report the daemon's state.
    State,
}

/// The daemon's answer to one control command (not to `ShowPage` or `SetBrightness`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Ok {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        message: String,
    },
    Error {
        message: String,
    },
    /// No deck is attached; `press`/`dial` callers run the action themselves.
    NoDeck,
    State {
        #[serde(default)]
        deck: Option<DeckInfo>,
        #[serde(default)]
        page: Option<String>,
        #[serde(default)]
        pages: Vec<String>,
        brightness: u8,
    },
}

fn one() -> u8 {
    1
}

fn is_one(n: &u8) -> bool {
    *n == 1
}

/// How long `request` waits for the daemon's answer.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// The longest command line the daemon reads; longer ones are refused. Roomy, so a
/// multi with dozens of steps or a long command still fits (BUGS.md 86).
pub const MAX_LINE: usize = 64 * 1024;

const BUSY: &str = "daemon busy: omadeckd closed the connection without answering; try again";

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

impl Command {
    pub fn to_line(&self) -> String {
        line(self)
    }
}

impl Reply {
    pub fn to_line(&self) -> String {
        line(self)
    }

    pub fn ok(message: impl Into<String>) -> Self {
        Reply::Ok { message: message.into() }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Reply::Error { message: message.into() }
    }
}

/// Parse one command line from a client. Unknown or malformed commands are an error
/// message (the hub sends it back as `Reply::Error`), so a newer client never hangs on an
/// older daemon.
pub fn parse_command(line: &str) -> Result<Command, String> {
    let line = line.trim();
    serde_json::from_str::<Command>(line).map_err(|_| {
        let v = serde_json::from_str::<serde_json::Value>(line).ok();
        match v.as_ref().and_then(|v| v.get("command")).and_then(|c| c.as_str()) {
            Some(name) => format!("unknown command “{name}”"),
            None => "not a command".into(),
        }
    })
}

/// Send one command and wait for its reply. `Ok(None)` when no daemon is running (no
/// runtime dir, no socket, or a stale one).
pub fn request(cmd: &Command) -> Result<Option<Reply>> {
    let Ok(path) = paths::socket_path() else { return Ok(None) };
    let mut stream = match UnixStream::connect(&path) {
        Ok(s) => s,
        Err(e) if matches!(e.kind(), ErrorKind::NotFound | ErrorKind::ConnectionRefused) => return Ok(None),
        Err(e) => return Err(anyhow!(e).context(format!("connecting to {}", path.display()))),
    };
    stream.set_read_timeout(Some(REPLY_TIMEOUT))?;
    stream.set_write_timeout(Some(Duration::from_millis(200)))?;
    stream.write_all(cmd.to_line().as_bytes())?;
    read_reply(BufReader::new(stream)).map(Some)
}

/// The first reply line from the daemon. Skips the status replay and events that arrive
/// first. Split out of `request` so it can be tested on a `&[u8]`.
pub fn read_reply(mut r: impl BufRead) -> Result<Reply> {
    let mut buf = String::new();
    loop {
        buf.clear();
        match r.read_line(&mut buf) {
            Ok(0) => return Err(anyhow!(BUSY)),
            Ok(_) => {}
            // Linux reports a socket read timeout as `WouldBlock`.
            Err(e) if matches!(e.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {
                return Err(anyhow!(
                    "omadeckd did not answer (is it older than this omadeckd? systemctl --user restart omadeckd)"
                ));
            }
            Err(e) => return Err(e.into()),
        }
        let line = buf.trim();
        let Ok(v) = serde_json::from_str::<serde_json::Value>(line) else { continue };
        if v.get("reply").is_none() {
            continue;
        }
        return serde_json::from_value::<Reply>(v).map_err(|_| anyhow!("unexpected reply from omadeckd: {line}"));
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

    fn command_roundtrips(cmd: Command) {
        assert_eq!(parse_command(&cmd.to_line()), Ok(cmd));
    }

    #[test]
    fn command_roundtrip() {
        command_roundtrips(Command::ShowPage { page: Some("Media".into()) });
        command_roundtrips(Command::ShowPage { page: None });
        command_roundtrips(Command::SetBrightness { value: 40 });
        command_roundtrips(Command::Press { index: 3, page: None });
        command_roundtrips(Command::Press { index: 3, page: Some("Media".into()) });
        command_roundtrips(Command::Dial { index: 1, dir: Direction::Left, steps: 1 });
        command_roundtrips(Command::Dial { index: 1, dir: Direction::Press, steps: 5 });
        command_roundtrips(Command::Run { action: Action::Brightness { change: 10, set: None } });
        command_roundtrips(Command::Page { page: None });
        command_roundtrips(Command::Page { page: Some("Apps".into()) });
        command_roundtrips(Command::Reload);
        command_roundtrips(Command::State);
        // No `steps` on the wire is one step.
        assert_eq!(
            parse_command("{\"command\":\"dial\",\"index\":2,\"dir\":\"left\"}"),
            Ok(Command::Dial { index: 2, dir: Direction::Left, steps: 1 })
        );
        assert_eq!(
            Command::Dial { index: 0, dir: Direction::Left, steps: 4 }.to_line(),
            "{\"command\":\"dial\",\"index\":0,\"dir\":\"left\",\"steps\":4}\n"
        );
    }

    #[test]
    fn command_wire_shape() {
        // The socket protocol is a contract with scripts; pin it.
        assert_eq!(
            Command::Run { action: Action::PageStep { change: 1 } }.to_line(),
            "{\"command\":\"run\",\"action\":{\"type\":\"page_step\"}}\n"
        );
        assert_eq!(Command::Press { index: 3, page: None }.to_line(), "{\"command\":\"press\",\"index\":3}\n");
        assert_eq!(
            Command::Dial { index: 0, dir: Direction::Right, steps: 1 }.to_line(),
            "{\"command\":\"dial\",\"index\":0,\"dir\":\"right\"}\n"
        );
        assert_eq!(Command::State.to_line(), "{\"command\":\"state\"}\n");
        assert_eq!(Reply::ok("").to_line(), "{\"reply\":\"ok\"}\n");
        assert_eq!(Reply::NoDeck.to_line(), "{\"reply\":\"no_deck\"}\n");
    }

    fn reply_roundtrips(r: Reply) {
        assert_eq!(serde_json::from_str::<Reply>(&r.to_line()).unwrap(), r);
    }

    #[test]
    fn reply_roundtrip() {
        reply_roundtrips(Reply::ok(""));
        reply_roundtrips(Reply::ok("page Media"));
        reply_roundtrips(Reply::error("there's no page named “Nope”"));
        reply_roundtrips(Reply::NoDeck);
        reply_roundtrips(Reply::State {
            deck: None,
            page: Some("Media".into()),
            pages: vec!["Media".into(), "Apps".into()],
            brightness: 40,
        });
    }

    #[test]
    fn reply_skips_events_and_names_eof() {
        let lines = "{\"event\":\"disconnected\"}\n{\"event\":\"page_changed\",\"page\":null}\n\
                     {\"reply\":\"error\",\"message\":\"no key 40; this deck has 8 keys\"}\n";
        let r = read_reply(lines.as_bytes()).unwrap();
        assert_eq!(r, Reply::error("no key 40; this deck has 8 keys"));
        // The daemon closed the connection before answering: its queue for us was full.
        let e = read_reply("{\"event\":\"disconnected\"}\n".as_bytes()).unwrap_err();
        assert!(e.to_string().starts_with("daemon busy"), "{e}");
    }

    #[test]
    fn unknown_command_is_an_error() {
        assert_eq!(parse_command(r#"{"command":"explode"}"#), Err("unknown command “explode”".into()));
        assert_eq!(parse_command("nonsense"), Err("not a command".into()));
    }

    fn roundtrip(ev: &Event) {
        let line = ev.to_line();
        assert_eq!(serde_json::from_str::<Event>(&line).unwrap(), *ev);
    }

    #[test]
    fn state_events_roundtrip_and_wire_shape() {
        roundtrip(&Event::Brightness { value: 40 });
        roundtrip(&Event::DialStatus { index: 1, status: PanelStatus::default() });
        let level = Some(crate::Level { value: 0.5, text: "50%".into(), muted: false });
        roundtrip(&Event::DialStatus { index: 2, status: PanelStatus { level, ..Default::default() } });
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

    #[test]
    fn key_state_live_wire_shape() {
        let live = crate::LiveShow { text: "AA".into(), bar: None, charging: false };
        let st = KeyStatus { live: Some(live), ..KeyStatus::default() };
        assert_eq!(
            Event::KeyState { page: None, index: 1, status: st.clone() }.to_line(),
            "{\"event\":\"key_state\",\"page\":null,\"index\":1,\"status\":{\"live\":{\"text\":\"AA\"}}}\n"
        );
        roundtrip(&Event::KeyState { page: None, index: 1, status: st });
        let none = Event::KeyState { page: None, index: 1, status: KeyStatus::default() }.to_line();
        assert!(!none.contains("live"), "{none}");
        let bat = crate::LiveShow { text: "50%".into(), bar: Some(0.5), charging: true };
        roundtrip(&Event::DialStatus { index: 0, status: PanelStatus { live: Some(bat), ..Default::default() } });
    }

    #[test]
    fn dial_status_event_carries_app() {
        let app = crate::AppShow { title: "Spotify".into(), icon: None, dimmed: false };
        let st = PanelStatus { app: Some(app), ..Default::default() };
        let line = Event::DialStatus { index: 1, status: st.clone() }.to_line();
        assert!(line.contains("\"status\":{\"app\":{\"title\":\"Spotify\"}}"), "{line}");
        roundtrip(&Event::DialStatus { index: 1, status: st });
        let dimmed = crate::AppShow { title: "A".into(), icon: Some("/i.png".into()), dimmed: true };
        roundtrip(&Event::DialStatus { index: 1, status: PanelStatus { app: Some(dimmed), ..Default::default() } });
        let none = Event::DialStatus { index: 1, status: PanelStatus::default() }.to_line();
        assert!(!none.contains("app"), "{none}");
    }
}
