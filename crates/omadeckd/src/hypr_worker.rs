//! The Hyprland worker: one background thread that runs every Hyprland dispatch the deck
//! asks for, in the order asked. `hyprctl dispatch` waits for Hyprland's answer (up to
//! 2 s, twice on a Lua config, when Hyprland is stalled), so it never runs on the deck
//! loop. One ordered worker, not a thread per call: a spin's dispatches stay in order
//! and a stalled Hyprland can't pile up threads. Failures come back to the loop, which
//! reports them against the key or dial that asked.

use std::sync::mpsc;
use std::thread;

use anyhow::Result;
use omadeck_core::action;

use crate::daemon::Source;

/// One input's dispatch: `dispatch` run `steps` times (a dial's detents), stopping at
/// the first failure.
#[derive(Debug, Clone, PartialEq)]
pub struct HyprJob {
    pub src: Source,
    /// The page the deck showed when the input came in (for `ActionFailed`).
    pub page: Option<String>,
    pub dispatch: String,
    pub steps: u8,
}

/// A job that failed, for the loop to publish.
#[derive(Debug, Clone, PartialEq)]
pub struct HyprFailed {
    pub src: Source,
    pub page: Option<String>,
    pub message: String,
}

pub struct HyprWorker {
    tx: mpsc::Sender<HyprJob>,
    /// Failures, in order; the loop drains them each pass (`Daemon::poll_hypr`).
    pub failed: mpsc::Receiver<HyprFailed>,
}

impl HyprWorker {
    pub fn start() -> Self {
        Self::start_with(action::hypr_dispatch)
    }

    /// With the dispatch function swapped out, for tests.
    fn start_with(run: fn(&str) -> Result<()>) -> Self {
        let (tx, jobs) = mpsc::channel::<HyprJob>();
        let (failed_tx, failed) = mpsc::channel();
        // Not joined at shutdown: a queued dispatch may wait 2 s on a stalled Hyprland,
        // and nothing is lost by leaving it. The thread ends when `tx` is dropped.
        thread::Builder::new()
            .name("hypr".into())
            .spawn(move || {
                for job in jobs {
                    for _ in 0..job.steps.max(1) {
                        if let Err(e) = run(&job.dispatch) {
                            let message = format!("{e:#}");
                            let _ = failed_tx.send(HyprFailed { src: job.src, page: job.page.clone(), message });
                            break;
                        }
                    }
                }
            })
            .expect("spawn hypr thread");
        HyprWorker { tx, failed }
    }

    /// Queue a job behind the ones already sent. Never blocks.
    pub fn send(&self, job: HyprJob) {
        let _ = self.tx.send(job);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn fake(dispatch: &str) -> Result<()> {
        RAN.lock().unwrap().push(dispatch.to_string());
        if dispatch == "bad" { anyhow::bail!("hyprctl: invalid dispatcher") } else { Ok(()) }
    }

    #[test]
    fn jobs_run_in_order_and_failures_come_back() {
        let w = HyprWorker::start_with(fake);
        let job = |src, dispatch: &str, steps| HyprJob { src, page: None, dispatch: dispatch.into(), steps };
        w.send(job(Source::Turn(0), "workspace e+1", 2));
        w.send(job(Source::Key(3), "bad", 3)); // stops at the first failure
        w.send(job(Source::Key(4), "togglefloating", 1));
        let failed = w.failed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            failed,
            HyprFailed { src: Source::Key(3), page: None, message: "hyprctl: invalid dispatcher".into() }
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while RAN.lock().unwrap().len() < 4 {
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*RAN.lock().unwrap(), ["workspace e+1", "workspace e+1", "bad", "togglefloating"]);
        assert!(w.failed.try_recv().is_err(), "one failure");
    }
}
