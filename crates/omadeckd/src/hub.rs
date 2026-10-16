//! Talks to configurator windows over the Unix socket: broadcasts daemon events to all of
//! them and collects the commands they send.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use omadeck_core::ipc::{self, Command, Event};

/// Lines queued per client. A client this far behind loses events, never the deck loop's
/// time and never its connection.
const CLIENT_QUEUE: usize = 256;

pub struct Hub {
    path: PathBuf,
    inner: Arc<Mutex<Inner>>,
    commands: mpsc::Receiver<Command>,
}

struct Inner {
    /// One queue per client; its writer thread empties it into the socket.
    clients: Vec<SyncSender<Arc<str>>>,
    replay: Replay,
}

/// What a new client is told first, so it starts in sync: the latest of each kind of
/// state event. PR 8 adds `keys: BTreeMap<(Option<String>, u8), Event>` (`KeyState` per
/// page and key) next to `panels`, remembered and replayed the same way.
struct Replay {
    /// Last connect/disconnect event.
    status: Event,
    /// Last page change.
    page: Event,
    /// Last brightness.
    brightness: Option<Event>,
    /// Last `DialStatus` per panel; cleared on disconnect.
    panels: BTreeMap<u8, Event>,
}

impl Replay {
    fn remember(&mut self, ev: &Event) {
        match ev {
            Event::Connected { .. } => self.status = ev.clone(),
            Event::Disconnected => {
                self.status = ev.clone();
                self.panels.clear();
            }
            Event::PageChanged { .. } => self.page = ev.clone(),
            Event::Brightness { .. } => self.brightness = Some(ev.clone()),
            Event::DialStatus { index, .. } => {
                self.panels.insert(*index, ev.clone());
            }
            _ => {}
        }
    }

    /// The lines a new client is sent first, in this order.
    fn lines(&self) -> Vec<Arc<str>> {
        let mut out: Vec<Arc<str>> = vec![self.status.to_line().into(), self.page.to_line().into()];
        out.extend(self.brightness.iter().map(|e| e.to_line().into()));
        out.extend(self.panels.values().map(|e| e.to_line().into()));
        out
    }
}

impl Hub {
    pub fn start(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        let inner = Arc::new(Mutex::new(Inner {
            clients: Vec::new(),
            replay: Replay {
                status: Event::Disconnected,
                page: Event::PageChanged { page: None },
                brightness: None,
                panels: BTreeMap::new(),
            },
        }));
        let (tx, commands) = mpsc::channel();
        let shared = inner.clone();
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                // The queue first: PR 11's reader sends replies into it.
                let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
                if let Ok(reader) = stream.try_clone() {
                    let tx = tx.clone();
                    let _ = std::thread::Builder::new().name("ipc-client".into()).spawn(move || {
                        for cmd in ipc::commands(reader) {
                            if tx.send(cmd).is_err() {
                                break;
                            }
                        }
                    });
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

    /// Commands received since the last call.
    pub fn commands(&self) -> mpsc::TryIter<'_, Command> {
        self.commands.try_iter()
    }

    pub fn shutdown(self) {
        let _ = std::fs::remove_file(&self.path);
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
