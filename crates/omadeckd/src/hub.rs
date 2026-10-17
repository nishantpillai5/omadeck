//! Talks to configurator windows over the Unix socket: broadcasts daemon events to all of
//! them, collects the commands they send, and carries each control command's reply back
//! to the client that sent it.

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use omadeck_core::KeyStatus;
use omadeck_core::ipc::{self, Command, Event, Reply};

/// Lines queued per client. A client this far behind loses events, never the deck loop's
/// time and never its connection.
const CLIENT_QUEUE: usize = 256;

pub struct Hub {
    /// The socket file, removed at shutdown; `None` for a detached hub (tests).
    path: Option<PathBuf>,
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
    /// Commands handed to the loop and not answered yet, and whether the client hung up:
    /// its writer ends after the last answer.
    pending: Arc<(AtomicUsize, AtomicBool)>,
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

    /// The loop's answer to one of this client's commands.
    pub fn answer(&self, r: &Reply) {
        self.send(r);
        self.done();
    }

    /// One of this client's commands was handled, answered or not. After the last one, a
    /// client that hung up has its writer ended.
    pub fn done(&self) {
        if self.pending.0.fetch_sub(1, Ordering::SeqCst) == 1 && self.pending.1.load(Ordering::SeqCst) {
            self.end();
        }
    }

    /// End the writer once what is queued is written.
    fn end(&self) {
        let _ = self.queue.try_send(Arc::from(""));
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

impl Inner {
    /// No clients yet, and the replay of a daemon that just started.
    fn shared() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Inner {
            clients: Vec::new(),
            replay: Replay {
                status: Event::Disconnected,
                page: Event::PageChanged { page: None },
                brightness: None,
                panels: BTreeMap::new(),
                keys: BTreeMap::new(),
            },
        }))
    }
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
        let inner = Inner::shared();
        let (tx, commands) = mpsc::channel();
        let shared = inner.clone();
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            loop {
                let stream = match listener.accept() {
                    Ok((stream, _)) => stream,
                    // Out of fds (EMFILE) or similar: wait instead of spinning on `accept`.
                    Err(e) => {
                        log::warn!("accepting a client: {e}");
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        continue;
                    }
                };
                // The queue first: the reader sends replies into it.
                let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
                if let (Ok(reader), Ok(closer)) = (stream.try_clone(), stream.try_clone()) {
                    let tx = tx.clone();
                    let reply = ReplyTo { queue: queue.clone(), stream: Arc::new(closer), pending: Arc::default() };
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
        Ok(Self { path: Some(path), inner, commands })
    }

    /// A hub with no socket and no listener thread, for tests: what it publishes lands
    /// only in its replay, and commands come from the returned sender.
    #[cfg(test)]
    pub fn detached() -> (Self, mpsc::Sender<Request>) {
        let (tx, commands) = mpsc::channel();
        (Self { path: None, inner: Inner::shared(), commands }, tx)
    }

    /// The lines a client connecting now would be sent first.
    #[cfg(test)]
    pub fn replay_lines(&self) -> Vec<String> {
        self.inner.lock().unwrap().replay.lines().iter().map(|l| l.trim_end().to_string()).collect()
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
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The `ipc-client` thread's body: read command lines, hand them to the loop, answer
/// parse errors itself. Ends when the client hangs up, sends a line longer than
/// `ipc::MAX_LINE`, or the loop is gone.
fn read_requests(stream: UnixStream, reply: ReplyTo, tx: mpsc::Sender<Request>) {
    serve_requests(stream, &reply, tx);
    // The client hung up (or was cut off): end its writer now, not at the next event, which
    // an idle daemon may not publish for hours. Otherwise every short-lived ctl client
    // would keep a thread and an fd. Anything queued before this is still written first. If
    // the queue is full, the writer is busy writing and ends on the failed write instead.
    reply.pending.1.store(true, Ordering::SeqCst);
    if reply.pending.0.load(Ordering::SeqCst) == 0 {
        reply.end();
    }
}

fn serve_requests(stream: UnixStream, reply: &ReplyTo, tx: mpsc::Sender<Request>) {
    let mut reader = BufReader::new(stream);
    let mut bytes = Vec::new();
    loop {
        bytes.clear();
        // Bytes, decoded after: `read_line` fails on a cap that cuts a character in two,
        // and that client got no answer at all.
        match (&mut reader).take(ipc::MAX_LINE as u64 + 1).read_until(b'\n', &mut bytes) {
            Ok(0) | Err(_) => return,
            Ok(_) => {}
        }
        if bytes.len() > ipc::MAX_LINE {
            reply.send(&Reply::error("command too long"));
            return;
        }
        let line = String::from_utf8_lossy(&bytes);
        if line.trim().is_empty() {
            continue;
        }
        match ipc::parse_command(&line) {
            Ok(cmd) => {
                reply.pending.0.fetch_add(1, Ordering::SeqCst);
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
/// reading stalls this thread alone. Ends when a write fails (the client closed) or on the
/// empty line the reader queues when the client hangs up.
fn spawn_writer(mut stream: UnixStream, lines: mpsc::Receiver<Arc<str>>) {
    let _ = std::thread::Builder::new().name("ipc-write".into()).spawn(move || {
        for line in lines {
            if line.is_empty() || stream.write_all(line.as_bytes()).is_err() {
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
        let reply = ReplyTo { queue, stream: Arc::new(ours), pending: Arc::default() };
        reply.send(&Reply::ok("page Media"));
        theirs.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
        let mut buf = String::new();
        assert_eq!(theirs.read_to_string(&mut buf).unwrap(), 0, "the client sees EOF at once, not a timeout");
    }

    #[test]
    fn a_client_that_hangs_up_ends_its_writer() {
        // An idle daemon publishes nothing: the writer must end when the reader sees the
        // hang-up, not at the next event.
        let (ours, theirs) = UnixStream::pair().unwrap();
        let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
        let reply =
            ReplyTo { queue: queue.clone(), stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() };
        let (tx, _rx) = mpsc::channel();
        let reader = ours.try_clone().unwrap();
        let read = std::thread::spawn(move || read_requests(reader, reply, tx));
        spawn_writer(ours, lines);
        drop(theirs);
        read.join().unwrap();
        let start = Instant::now();
        // Once the writer is gone, its queue reports Disconnected.
        while !matches!(queue.try_send("{}\n".into()), Err(TrySendError::Disconnected(_))) {
            assert!(start.elapsed() < Duration::from_secs(2), "the writer is still alive");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_client_that_closes_its_write_side_still_gets_its_reply() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
        let reply = ReplyTo { queue, stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() };
        let (tx, rx) = mpsc::channel();
        let reader = ours.try_clone().unwrap();
        let read = std::thread::spawn(move || read_requests(reader, reply, tx));
        spawn_writer(ours, lines);
        std::io::Write::write_all(&mut &theirs, b"{\"command\":\"state\"}\n").unwrap();
        theirs.shutdown(std::net::Shutdown::Write).unwrap();
        let req: Request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        read.join().unwrap();
        req.reply.answer(&Reply::ok("answered"));
        let mut line = String::new();
        BufReader::new(&theirs).read_line(&mut line).unwrap();
        assert!(line.contains("answered"), "{line:?}");
    }

    #[test]
    fn long_and_odd_lines_are_read_or_answered() {
        // A command past 4 KiB was refused, and a cap cut through a UTF-8 character got no
        // answer at all.
        let (ours, theirs) = UnixStream::pair().unwrap();
        let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
        let reply = ReplyTo { queue, stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() };
        let (tx, rx) = mpsc::channel();
        let reader = ours.try_clone().unwrap();
        std::thread::spawn(move || read_requests(reader, reply, tx));
        spawn_writer(ours, lines);
        theirs.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        let mut client = theirs.try_clone().unwrap();
        let mut replies = BufReader::new(theirs);

        // A 10 KB command goes through.
        let run = Command::Run { action: omadeck_core::Action::App { command: "x".repeat(10_000) } };
        client.write_all(run.to_line().as_bytes()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap().cmd, run);

        // Bad UTF-8 is answered, not dropped.
        client.write_all(b"{\"command\":\"\xff\"}\n").unwrap();
        assert!(matches!(ipc::read_reply(&mut replies).unwrap(), Reply::Error { .. }));

        // A line past the cap with the cap inside a two-byte character: "command too long".
        let mut long = b"{\"command\":\"".to_vec();
        long.extend("é".repeat(ipc::MAX_LINE).as_bytes());
        long.extend(b"\"}\n");
        client.write_all(&long[..ipc::MAX_LINE + 1]).unwrap();
        let r = ipc::read_reply(&mut replies).unwrap();
        assert_eq!(r, Reply::error("command too long"));
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
    fn a_detached_hub_keeps_the_replay() {
        let (hub, _tx) = Hub::detached();
        hub.publish(Event::Brightness { value: 40 });
        hub.publish(Event::DialStatus { index: 1, status: Default::default() });
        hub.publish(Event::DialTurn { index: 0, delta: 1 }); // not state: not replayed
        assert_eq!(
            hub.replay_lines(),
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
