//! Talks to configurator windows over the Unix socket: broadcasts daemon events to all of
//! them and collects the commands they send.

use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use anyhow::{Context, Result};
use omadeck_core::ipc::{self, Command, Event};

pub struct Hub {
    path: PathBuf,
    inner: Arc<Mutex<Inner>>,
    commands: mpsc::Receiver<Command>,
}

struct Inner {
    clients: Vec<UnixStream>,
    /// Last connect/disconnect event, replayed to new clients.
    status: Event,
    /// Last page change, replayed to new clients.
    page: Event,
}

impl Hub {
    pub fn start(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        let inner = Arc::new(Mutex::new(Inner {
            clients: Vec::new(),
            status: Event::Disconnected,
            page: Event::PageChanged { page: None },
        }));
        let (tx, commands) = mpsc::channel();
        let shared = inner.clone();
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                let _ = stream.set_write_timeout(Some(Duration::from_millis(200)));
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
                let mut stream = stream;
                let hello = g.status.to_line() + &g.page.to_line();
                if stream.write_all(hello.as_bytes()).is_ok() {
                    g.clients.push(stream);
                }
            }
        })?;
        Ok(Self { path, inner, commands })
    }

    pub fn publish(&self, ev: Event) {
        let line = ev.to_line();
        let mut g = self.inner.lock().unwrap();
        match ev {
            Event::Connected { .. } | Event::Disconnected => g.status = ev,
            Event::PageChanged { .. } => g.page = ev,
            _ => {}
        }
        g.clients.retain_mut(|c| c.write_all(line.as_bytes()).is_ok());
    }

    /// Commands received since the last call.
    pub fn commands(&self) -> mpsc::TryIter<'_, Command> {
        self.commands.try_iter()
    }

    pub fn shutdown(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}
