//! Talks to configurator windows over the Unix socket: broadcasts daemon events to all of
//! them, collects the commands they send, and carries each control command's reply back
//! to the client that sent it.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use omadeck_core::KeyStatus;
use omadeck_core::ipc::{self, Command, Event, Reply};

/// Lines queued per client. A client this far behind loses events, never the deck loop's
/// time and never its connection.
const CLIENT_QUEUE: usize = 256;

pub struct Hub {
    path: PathBuf,
    inner: Arc<Mutex<Inner>>,
    commands: mpsc::Receiver<Request>,
}

/// One command from a client, with the way back to that client.
pub struct Request {
    pub cmd: Command,
    pub reply: ReplyTo,
}

/// A client's queue to its writer thread (the same one events go into), and a handle to
/// close its socket.
#[derive(Clone)]
pub struct ReplyTo {
    queue: SyncSender<Arc<str>>,
    stream: Arc<UnixStream>,
}

impl ReplyTo {
    /// Queue a reply line. Never blocks the loop, and never drops a reply silently: if the
    /// client's queue is full, the client is disconnected (its CLI then says "daemon busy").
    pub fn send(&self, r: &Reply) {
        match self.queue.try_send(r.to_line().into()) {
            Ok(()) | Err(TrySendError::Disconnected(_)) => {}
            Err(TrySendError::Full(_)) => {
                log::warn!("a client isn't reading; closing its connection so the reply isn't lost");
                let _ = self.stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}

struct Inner {
    /// One queue per client; its writer thread empties it into the socket.
    clients: Vec<SyncSender<Arc<str>>>,
    replay: Replay,
}

/// What a new client is told first, so it starts in sync: the latest of each kind of
/// state event.
struct Replay {
    /// Last connect/disconnect event.
    status: Event,
    /// Last page change.
    page: Event,
    /// Last brightness.
    brightness: Option<Event>,
    /// Last `DialStatus` per panel; cleared on disconnect.
    panels: BTreeMap<u8, Event>,
    /// Last `KeyState` per (page, key); cleared on disconnect.
    keys: BTreeMap<(Option<String>, u8), Event>,
}

impl Replay {
    fn remember(&mut self, ev: &Event) {
        match ev {
            Event::Connected { .. } => self.status = ev.clone(),
            Event::Disconnected => {
                self.status = ev.clone();
                self.panels.clear();
                self.keys.clear();
            }
            Event::PageChanged { .. } => self.page = ev.clone(),
            Event::Brightness { .. } => self.brightness = Some(ev.clone()),
            Event::DialStatus { index, .. } => {
                self.panels.insert(*index, ev.clone());
            }
            Event::KeyState { page, index, status } => {
                if *status == KeyStatus::default() {
                    self.keys.remove(&(page.clone(), *index));
                } else {
                    self.keys.insert((page.clone(), *index), ev.clone());
                }
            }
            _ => {}
        }
    }

    /// The lines a new client is sent first, in this order.
    fn lines(&self) -> Vec<Arc<str>> {
        let mut out: Vec<Arc<str>> = vec![self.status.to_line().into(), self.page.to_line().into()];
        out.extend(self.brightness.iter().map(|e| e.to_line().into()));
        out.extend(self.panels.values().map(|e| e.to_line().into()));
        out.extend(self.keys.values().map(|e| e.to_line().into()));
        out
    }
}

impl Hub {
    pub fn start(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        // The runtime dir is already 0700; the socket accepts commands, so make it explicit.
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("restricting {}", path.display()))?;
        let inner = Arc::new(Mutex::new(Inner {
            clients: Vec::new(),
            replay: Replay {
                status: Event::Disconnected,
                page: Event::PageChanged { page: None },
                brightness: None,
                panels: BTreeMap::new(),
                keys: BTreeMap::new(),
            },
        }));
        let (tx, commands) = mpsc::channel();
        let shared = inner.clone();
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                // The queue first: the reader sends replies into it.
                let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
                if let (Ok(reader), Ok(closer)) = (stream.try_clone(), stream.try_clone()) {
                    let tx = tx.clone();
                    let reply = ReplyTo { queue: queue.clone(), stream: Arc::new(closer) };
                    let _ = std::thread::Builder::new()
                        .name("ipc-client".into())
                        .spawn(move || read_requests(reader, reply, tx));
                }
                let mut g = shared.lock().unwrap();
                // Queued under the lock, so nothing published meanwhile can overtake it.
                for line in g.replay.lines() {
                    let _ = queue.try_send(line);
                }
                g.clients.push(queue);
                drop(g);
                spawn_writer(stream, lines);
            }
        })?;
        Ok(Self { path, inner, commands })
    }

    /// Send `ev` to every client. Never waits on a client: it only queues the line.
    pub fn publish(&self, ev: Event) {
        let line: Arc<str> = ev.to_line().into();
        let mut g = self.inner.lock().unwrap();
        g.replay.remember(&ev);
        g.clients.retain(|c| match c.try_send(line.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) => true,
            // Its writer stopped: the client went away.
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    /// Commands received since the last call, each with the way back to its client.
    pub fn commands(&self) -> mpsc::TryIter<'_, Request> {
        self.commands.try_iter()
    }

    pub fn shutdown(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// The `ipc-client` thread's body: read command lines, hand them to the loop, answer
/// parse errors itself. Ends when the client hangs up, sends a line longer than
/// `ipc::MAX_LINE`, or the loop is gone.
fn read_requests(stream: UnixStream, reply: ReplyTo, tx: mpsc::Sender<Request>) {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    loop {
        line.clear();
        match (&mut reader).take(ipc::MAX_LINE as u64 + 1).read_line(&mut line) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if line.len() > ipc::MAX_LINE {
            reply.send(&Reply::error("command too long"));
            return;
        }
        if line.trim().is_empty() {
            continue;
        }
        match ipc::parse_command(&line) {
            Ok(cmd) => {
                if tx.send(Request { cmd, reply: reply.clone() }).is_err() {
                    return;
                }
            }
            // Through the writer like any reply: only it writes to the socket.
            Err(msg) => reply.send(&Reply::error(msg)),
        }
    }
}

/// A client's writer: the only place that writes to its socket, so a client that stops
/// reading stalls this thread alone. Ends when a write fails (the client closed).
fn spawn_writer(mut stream: UnixStream, lines: mpsc::Receiver<Arc<str>>) {
    let _ = std::thread::Builder::new().name("ipc-write".into()).spawn(move || {
        for line in lines {
            if stream.write_all(line.as_bytes()).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    #[test]
    fn a_full_queue_disconnects_instead_of_dropping_the_reply() {
        // No socket file needed: a connected pair stands in for the client.
        let (ours, mut theirs) = UnixStream::pair().unwrap();
        let (queue, _lines) = mpsc::sync_channel::<Arc<str>>(1);
        queue.try_send("{\"event\":\"disconnected\"}\n".into()).unwrap(); // full now
        let reply = ReplyTo { queue, stream: Arc::new(ours) };
        reply.send(&Reply::ok("page Media"));
        theirs.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut buf = String::new();
        assert_eq!(theirs.read_to_string(&mut buf).unwrap(), 0, "the client sees EOF at once, not a timeout");
    }

    /// Wait up to 3 s for the next request to reach the loop.
    fn next_request(hub: &Hub) -> Request {
        let start = Instant::now();
        loop {
            if let Some(r) = hub.commands().next() {
                return r;
            }
            assert!(start.elapsed() < Duration::from_secs(3), "no command arrived");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn command_reply_over_socket() {
        let path = socket("reply");
        let hub = Hub::start(path.clone()).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let mut client = UnixStream::connect(&path).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        client.write_all(Command::State.to_line().as_bytes()).unwrap();
        let req = next_request(&hub);
        assert_eq!(req.cmd, Command::State);
        let state = Reply::State { deck: None, page: None, pages: vec![], brightness: 40 };
        req.reply.send(&state);
        // The replay comes first and is skipped.
        assert_eq!(ipc::read_reply(BufReader::new(client)).unwrap(), state);
        // A command this daemon doesn't know is answered by the reader itself.
        let mut other = UnixStream::connect(&path).unwrap();
        other.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        other.write_all(b"{\"command\":\"explode\"}\n").unwrap();
        let r = ipc::read_reply(BufReader::new(other)).unwrap();
        assert!(matches!(r, Reply::Error { .. }), "{r:?}");
        hub.shutdown();
    }

    fn socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omadeck-{name}-{}.sock", std::process::id()))
    }

    #[test]
    fn new_client_gets_the_replay() {
        let path = socket("hub-replay");
        let hub = Hub::start(path.clone()).unwrap();
        hub.publish(Event::Brightness { value: 40 });
        hub.publish(Event::DialStatus { index: 1, status: Default::default() });
        let stream = UnixStream::connect(&path).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let lines: Vec<String> = BufReader::new(stream).lines().take(4).map(|l| l.unwrap()).collect();
        assert_eq!(
            lines,
            vec![
                r#"{"event":"disconnected"}"#,
                r#"{"event":"page_changed","page":null}"#,
                r#"{"event":"brightness","value":40}"#,
                r#"{"event":"dial_status","index":1,"status":{}}"#,
            ]
        );
        hub.shutdown();
    }

    #[test]
    fn a_client_that_never_reads_does_not_block_publish() {
        let path = socket("hub-stuck");
        let hub = Hub::start(path.clone()).unwrap();
        let _stuck = UnixStream::connect(&path).unwrap(); // connected, never read
        std::thread::sleep(Duration::from_millis(100)); // let the hub accept it
        let start = Instant::now();
        for i in 0..20_000 {
            hub.publish(Event::DialTurn { index: 0, delta: (i % 2) as i8 });
        }
        assert!(start.elapsed() < Duration::from_secs(1), "publish waited on the client: {:?}", start.elapsed());
        hub.shutdown();
    }
}
