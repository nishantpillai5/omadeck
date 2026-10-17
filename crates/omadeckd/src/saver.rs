//! Writes the daemon's state file off the deck loop, debounced: one write per burst of
//! changes, so a hard spin of the brightness dial doesn't serialize the file per detent.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use omadeck_core::paths;
use omadeck_core::state::State;

/// Wait this long after a change for more before writing.
const SETTLE: Duration = Duration::from_millis(200);

pub struct StateSaver {
    tx: mpsc::Sender<State>,
    thread: thread::JoinHandle<()>,
}

/// The saver for `$XDG_STATE_HOME/omadeck/state.toml`.
pub fn start_saver() -> StateSaver {
    start_saver_at(paths::state_file())
}

/// The saver for `path` (tests use a temp file).
pub fn start_saver_at(path: PathBuf) -> StateSaver {
    let (tx, rx) = mpsc::channel::<State>();
    let thread =
        thread::Builder::new().name("state".into()).spawn(move || run(&rx, &path)).expect("spawning the state saver");
    StateSaver { tx, thread }
}

impl StateSaver {
    /// Save `state`, whole, soon. Every field is always written together, so no field
    /// can clobber another.
    pub fn send(&self, state: State) {
        let _ = self.tx.send(state);
    }

    /// Write anything still pending and wait for it (shutdown).
    pub fn finish(self) {
        let StateSaver { tx, thread } = self;
        drop(tx);
        let _ = thread.join();
    }
}

fn run(rx: &mpsc::Receiver<State>, path: &std::path::Path) {
    // Blocks for the first change; ends when the daemon drops the sender.
    while let Ok(first) = rx.recv() {
        // A hard spin sends several in a row; keep the last of the burst. A dropped
        // sender ends the wait at once, so `finish` writes without waiting `SETTLE`.
        let mut state = first;
        while let Ok(next) = rx.recv_timeout(SETTLE) {
            state = next;
        }
        if let Err(e) = state.save_to(path) {
            log::warn!("saving {}: {e:#}", path.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saver_keeps_the_last_state_and_finish_writes_it() {
        let dir = std::env::temp_dir().join(format!("omadeck-saver-test-{}", std::process::id()));
        let path = dir.join("state.toml");
        let saver = start_saver_at(path.clone());
        saver.send(State { brightness: Some(10), ..State::default() });
        saver.send(State { brightness: Some(20), ..State::default() });
        saver.finish();
        assert_eq!(State::load_from(&path), State { brightness: Some(20), ..State::default() });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
