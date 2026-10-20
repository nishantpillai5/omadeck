//! Runs the actions that wait for an answer (Hyprland dispatchers) off the deck loop, one at
//! a time and in the order they were pressed, so a slow `hyprctl` can't freeze the deck.

use std::sync::mpsc;

use omadeck_core::{Action, action};

pub struct Worker {
    jobs: mpsc::Sender<(u8, Action)>,
    failures: mpsc::Receiver<(u8, String)>,
}

impl Worker {
    pub fn start() -> std::io::Result<Self> {
        let (jobs, rx) = mpsc::channel::<(u8, Action)>();
        let (tx, failures) = mpsc::channel();
        std::thread::Builder::new().name("actions".into()).spawn(move || {
            for (key, act) in rx {
                if let Err(e) = action::run(&act)
                    && tx.send((key, format!("{e:#}"))).is_err()
                {
                    break;
                }
            }
        })?;
        Ok(Self { jobs, failures })
    }

    /// Queue key `key`'s action.
    pub fn run(&self, key: u8, act: Action) {
        let _ = self.jobs.send((key, act));
    }

    /// Actions that failed since the last call, with their key.
    pub fn failures(&self) -> mpsc::TryIter<'_, (u8, String)> {
        self.failures.try_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn failures_come_back_with_their_key() {
        let w = Worker::start().unwrap();
        w.run(4, Action::Hyprland { dispatch: String::new() });
        let deadline = Instant::now() + Duration::from_secs(2);
        let got = loop {
            if let Some(f) = w.failures().next() {
                break f;
            }
            assert!(Instant::now() < deadline, "no failure reported");
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(got, (4, "no dispatcher set".to_string()));
    }

    #[test]
    fn actions_run_in_the_order_they_were_pressed() {
        let w = Worker::start().unwrap();
        for key in 0..20 {
            w.run(key, Action::Hyprland { dispatch: String::new() });
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut keys = Vec::new();
        while keys.len() < 20 {
            assert!(Instant::now() < deadline, "only {} of 20 came back", keys.len());
            keys.extend(w.failures().map(|(k, _)| k));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(keys, (0..20).collect::<Vec<u8>>());
    }
}
