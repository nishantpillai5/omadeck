//! Running multi actions, timed by the daemon's loop. A delay is a deadline here, never a
//! sleep, so a multi never blocks the deck.

use std::fmt;
use std::time::{Duration, Instant};

use omadeck_core::Action;

use crate::daemon::KeyId;

/// Which input started a multi: what "ignored while running" compares, and where a
/// failed step is reported.
#[derive(Debug, Clone, PartialEq)]
pub enum RunOf {
    Key(KeyId),
    /// Either direction: a left and a right multi on one dial don't overlap.
    Turn(u8),
    /// A dial press or a tap on its panel (the same action).
    Press(u8),
    /// `omadeckd` (a `Command::Run`: a nested or dial ▶, `page next`; or a press on a page
    /// the deck isn't showing). A key's own ▶ is a `Press`, so it is `Key`.
    Ctl,
}

impl fmt::Display for RunOf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunOf::Key(KeyId { page: None, index }) => write!(f, "key {index}"),
            RunOf::Key(KeyId { page: Some(page), index }) => write!(f, "key {index} on {page}"),
            RunOf::Turn(i) | RunOf::Press(i) => write!(f, "dial {i}"),
            RunOf::Ctl => write!(f, "omadeckd"),
        }
    }
}

/// Running multi actions, timed by whoever polls it (the daemon's loop). Pure: no
/// threads, no clock of its own, so it's unit-tested with made-up instants.
#[derive(Debug, Default)]
pub struct Sequencer {
    runs: Vec<Run>,
}

#[derive(Debug)]
struct Run {
    of: RunOf,
    steps: Vec<Action>,
    /// The next step to look at.
    next: usize,
    /// When it may run.
    due: Instant,
}

impl Sequencer {
    /// Start `steps` for `of`. False (and nothing starts) while that input's last run is
    /// still going. An empty list starts nothing and returns true.
    pub fn start(&mut self, of: RunOf, steps: Vec<Action>, now: Instant) -> bool {
        if self.is_running(&of) {
            return false;
        }
        if !steps.is_empty() {
            self.runs.push(Run { of, steps, next: 0, due: now });
        }
        true
    }

    /// Every step whose time has come, in order, as (run, step number from 0, action).
    /// A delay moves its run's deadline on from the previous deadline (so lateness doesn't
    /// add up) and isn't returned. A run is dropped once its last step, or its trailing
    /// delay, is done.
    pub fn due(&mut self, now: Instant) -> Vec<(RunOf, usize, Action)> {
        let mut out = Vec::new();
        self.runs.retain_mut(|run| {
            while run.due <= now {
                let Some(step) = run.steps.get(run.next) else { return false };
                let n = run.next;
                run.next += 1;
                match step {
                    Action::Delay { ms } => {
                        run.due += Duration::from_millis(u64::from((*ms).min(omadeck_core::config::MAX_DELAY_MS)))
                    }
                    _ => out.push((run.of.clone(), n, step.clone())),
                }
            }
            true
        });
        out
    }

    /// The earliest deadline of a waiting run, for the loop's read timeout.
    pub fn next_due(&self) -> Option<Instant> {
        self.runs.iter().map(|r| r.due).min()
    }

    pub fn is_running(&self, of: &RunOf) -> bool {
        self.runs.iter().any(|r| r.of == *of)
    }

    pub fn len(&self) -> usize {
        self.runs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.runs.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(c: &str) -> Action {
        Action::App { command: c.into() }
    }

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn delay(ms: u32) -> Action {
        Action::Delay { ms }
    }

    #[test]
    fn start_runs_leading_steps_now() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        assert!(s.start(RunOf::Press(0), vec![app("a"), app("b"), delay(100), app("c")], t0));
        assert_eq!(s.due(t0), vec![(RunOf::Press(0), 0, app("a")), (RunOf::Press(0), 1, app("b"))]);
        assert!(s.is_running(&RunOf::Press(0)));
        assert_eq!(s.len(), 1);
    }

    #[test]
    fn delay_holds_the_rest() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        assert!(s.start(RunOf::Turn(1), vec![app("a"), delay(500), app("b"), app("c")], t0));
        assert_eq!(s.due(t0), vec![(RunOf::Turn(1), 0, app("a"))]);
        assert_eq!(s.next_due(), Some(t0 + ms(500)));
        assert!(s.due(t0 + ms(499)).is_empty());
        assert_eq!(s.due(t0 + ms(500)), vec![(RunOf::Turn(1), 2, app("b")), (RunOf::Turn(1), 3, app("c"))]);
        assert!(!s.is_running(&RunOf::Turn(1)));
        assert_eq!(s.next_due(), None);
    }

    #[test]
    fn delays_do_not_drift() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        s.start(RunOf::Ctl, vec![app("a"), delay(500), app("b"), delay(500), app("c")], t0);
        assert_eq!(s.due(t0).len(), 1);
        // Polled 300 ms late: b runs now, but c is still due 1000 ms after the start.
        assert_eq!(s.due(t0 + ms(800)), vec![(RunOf::Ctl, 2, app("b"))]);
        assert_eq!(s.next_due(), Some(t0 + ms(1000)));
        assert_eq!(s.due(t0 + ms(1000)), vec![(RunOf::Ctl, 4, app("c"))]);
        assert!(s.is_empty());
    }

    #[test]
    fn restart_ignored_while_running() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        let of = RunOf::Key(KeyId { page: None, index: 5 });
        assert!(s.start(of.clone(), vec![app("a"), delay(100), app("b")], t0));
        s.due(t0);
        assert!(!s.start(of.clone(), vec![app("x")], t0 + ms(50)));
        assert_eq!(s.len(), 1);
        assert_eq!(s.due(t0 + ms(100)), vec![(of.clone(), 2, app("b"))]);
        assert!(s.start(of.clone(), vec![app("x")], t0 + ms(150)));
        assert_eq!(s.due(t0 + ms(150)), vec![(of, 0, app("x"))]);
    }

    #[test]
    fn trailing_delay_keeps_it_running() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        s.start(RunOf::Press(2), vec![app("a"), delay(1000)], t0);
        assert_eq!(s.due(t0).len(), 1);
        assert!(s.is_running(&RunOf::Press(2)));
        assert!(s.due(t0 + ms(999)).is_empty());
        assert!(s.is_running(&RunOf::Press(2)));
        assert!(s.due(t0 + ms(1000)).is_empty());
        assert!(!s.is_running(&RunOf::Press(2)));
    }

    #[test]
    fn two_inputs_run_independently() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        let k = RunOf::Key(KeyId { page: Some("Media".into()), index: 1 });
        assert!(s.start(k.clone(), vec![delay(200), app("k")], t0));
        assert!(s.start(RunOf::Turn(0), vec![delay(100), app("t")], t0));
        assert!(s.due(t0).is_empty());
        assert_eq!(s.due(t0 + ms(100)), vec![(RunOf::Turn(0), 1, app("t"))]);
        assert_eq!(s.due(t0 + ms(200)), vec![(k, 1, app("k"))]);
        assert!(s.is_empty());
    }

    #[test]
    fn next_due_is_earliest() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        assert_eq!(s.next_due(), None);
        s.start(RunOf::Turn(0), vec![delay(300), app("a")], t0);
        s.start(RunOf::Press(0), vec![delay(100), app("b")], t0);
        s.due(t0);
        assert_eq!(s.next_due(), Some(t0 + ms(100)));
    }

    #[test]
    fn empty_multi_finishes_at_once() {
        let t0 = Instant::now();
        let mut s = Sequencer::default();
        assert!(s.start(RunOf::Ctl, vec![], t0));
        assert!(s.is_empty());
        assert!(!s.is_running(&RunOf::Ctl));
        assert!(s.due(t0).is_empty());
    }

    #[test]
    fn run_of_display() {
        assert_eq!(RunOf::Key(KeyId { page: None, index: 5 }).to_string(), "key 5");
        assert_eq!(RunOf::Key(KeyId { page: Some("Media".into()), index: 5 }).to_string(), "key 5 on Media");
        assert_eq!(RunOf::Turn(2).to_string(), "dial 2");
        assert_eq!(RunOf::Press(2).to_string(), "dial 2");
        assert_eq!(RunOf::Ctl.to_string(), "omadeckd");
    }
}
