//! The watcher: one thread that reads system state off the deck loop (volume levels and
//! toggle state commands; PR 9 adds clocks and meters) and runs the volume changes the
//! deck asks for, in order, each followed by a fresh read. It sends back only values that
//! changed, or that the deck asked for again. Each read-back carries the change the
//! watcher just applied (0 for a poll), so the deck can tell how much of what it sent is
//! still in flight and keep the bar from snapping back to a stale read.
//!
//! State commands never run on the watcher thread: a slow one would stall the volume
//! reads (and PR 9's clock). Each run happens on its own short-lived runner thread, which
//! sends its result back as `WatchCmd::Done` on a clone of the watcher's own command
//! sender. The watcher only keeps track of what is in flight, so one probe never has two
//! runs. The thread therefore stays fully blocking (`recv()` / `recv_timeout(next_due -
//! now)`), with no reap tick.

use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use omadeck_core::probe::{Probe, Toggle, Value, is_missing, toggle_from_exit};
use omadeck_core::proc::Job;
use omadeck_core::{AudioDevice, action, audio};

/// A message the deck loop sends the watcher.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchCmd {
    /// The complete set to read from now on (replaces the previous set).
    Watch(Vec<Probe>),
    /// Something changed it elsewhere: schedule a forced re-read at each of
    /// `Probe::refresh_after()` and send the result even if unchanged.
    Refresh(Probe),
    /// Change a device's volume here, after any earlier change, then re-read it at once
    /// and send the result even if unchanged (the deck is showing a guess).
    Volume { device: AudioDevice, change: i32, mute: bool },
    /// A slow toggle was pressed while showing `from`: poll every `FAST_INTERVAL` until a
    /// read is neither busy nor `from`, or `FAST_WINDOW` passes.
    Leave { probe: Probe, from: Toggle },
    /// A runner thread finished. Sent by the runner, not by the daemon.
    Done { probe: Probe, result: RunResult },
    /// End the thread (shutdown), cancelling the runners. Explicit, because the thread
    /// holds a sender itself (for the runners), so its `recv()` never sees a
    /// disconnect.
    Stop,
}

/// The result of one run of a state command.
#[derive(Debug, Clone, PartialEq)]
pub enum RunResult {
    /// The child exited; `first_line` is its first output line (the busy progress).
    Exit { code: Option<i32>, first_line: Option<String> },
    /// It ran too long, or was cancelled: the key keeps its face.
    TimedOut,
    /// It couldn't be started.
    SpawnFailed(String),
}

/// Runner threads at once.
const MAX_IN_FLIGHT: usize = 8;
/// A runner's `try_wait` step.
const RUN_POLL: Duration = Duration::from_millis(20);
/// How long a state command may run. Longer than the 2 s `COMMAND_TIMEOUT`: a D-Bus
/// query can take ~2 s when slow, and a health `curl -m 2` needs headroom.
#[cfg(not(test))]
const STATE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]
const STATE_TIMEOUT: Duration = Duration::from_millis(300);

pub struct Watcher {
    pub cmd: mpsc::Sender<WatchCmd>,
    /// `(probe, value, applied)`: `applied` is the change the read-back confirms (0 for a
    /// poll). The deck adds what's still in flight on top, so the bar never drops to a
    /// value the queued changes haven't reached yet.
    pub values: mpsc::Receiver<(Probe, Value, i32)>,
    thread: thread::JoinHandle<()>,
}

impl Watcher {
    /// Stop the thread and wait for it (shutdown). A volume change in progress finishes
    /// first; anything queued behind `Stop` is dropped.
    pub fn stop(self) {
        let _ = self.cmd.send(WatchCmd::Stop);
        let _ = self.thread.join();
    }
}

/// Read a volume device. Blocking (it runs `wpctl`/`pactl`, each under
/// `proc::COMMAND_TIMEOUT`), so only ever on the watcher.
fn read_volume(dev: AudioDevice) -> Option<Value> {
    audio::volume(dev).map(|l| Value::Level(l.to_panel()))
}

/// Change a volume and wait for it.
fn change_volume(dev: AudioDevice, change: i32, mute: bool) {
    action::apply_volume(dev, change.clamp(-100, 100) as i8, mute);
}

/// The watcher's state, apart from the thread, so its logic is tested without threads.
struct Poller {
    watched: Vec<Probe>,
    /// The last value sent per probe; a read equal to it isn't sent again.
    last: HashMap<Probe, Value>,
    /// When each watched probe is next read.
    next_due: HashMap<Probe, Instant>,
    /// Send the next read of these even if it equals `last` (the deck shows a guess).
    forced: HashSet<Probe>,
    /// Scheduled forced re-reads (`Refresh`), at their due times.
    refresh_at: Vec<(Instant, Probe)>,
    /// Running probes → their cancel flag. A probe in here is never started again until
    /// its `Done` (or a `Watch` that drops it).
    in_flight: HashMap<Probe, Arc<AtomicBool>>,
    /// A `Leave` window: (the state the press left, the deadline).
    fast_until: HashMap<Probe, (Toggle, Instant)>,
    /// Probes that failed and were warned about; cleared on a good read.
    failing: HashSet<Probe>,
    /// The I/O, swapped out in tests.
    read_volume: fn(AudioDevice) -> Option<Value>,
    change_volume: fn(AudioDevice, i32, bool),
}

impl Default for Poller {
    fn default() -> Self {
        Poller {
            watched: Vec::new(),
            last: HashMap::new(),
            next_due: HashMap::new(),
            forced: HashSet::new(),
            refresh_at: Vec::new(),
            in_flight: HashMap::new(),
            fast_until: HashMap::new(),
            failing: HashSet::new(),
            read_volume,
            change_volume,
        }
    }
}

impl Poller {
    /// Act on one command; values to send go to `out`. `Stop` is the thread loop's.
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Value, i32)>) {
        match cmd {
            WatchCmd::Watch(set) => {
                let mut set_d: Vec<Probe> = Vec::new();
                for p in set {
                    if !set_d.contains(&p) {
                        set_d.push(p);
                    }
                }
                let keep = |p: &Probe| set_d.contains(p);
                // Dropped probes: tell their runners to kill the command; their `Done` is
                // ignored.
                self.in_flight.retain(|p, cancel| {
                    keep(p) || {
                        cancel.store(true, Ordering::Relaxed);
                        false
                    }
                });
                // What stays watched keeps its last value (the deck kept it too); what is
                // new is read now.
                self.last.retain(|p, _| keep(p));
                self.next_due.retain(|p, _| keep(p));
                self.forced.retain(|p| keep(p));
                self.refresh_at.retain(|(_, p)| keep(p));
                self.fast_until.retain(|p, _| keep(p));
                self.failing.retain(|p| keep(p));
                for p in &set_d {
                    self.next_due.entry(p.clone()).or_insert(now);
                }
                self.watched = set_d;
            }
            WatchCmd::Refresh(p) => {
                if !self.watched.contains(&p) {
                    return;
                }
                for d in p.refresh_after() {
                    self.refresh_at.push((now + *d, p.clone()));
                }
            }
            WatchCmd::Leave { probe, from } => {
                if !self.watched.contains(&probe) {
                    return;
                }
                self.fast_until.insert(probe.clone(), (from, now + omadeck_core::probe::FAST_WINDOW));
                self.next_due.insert(probe, now + omadeck_core::probe::FAST_INTERVAL);
            }
            WatchCmd::Volume { device, change, mute } => {
                // Runs here, in order: a change never races the one before it, and the
                // read right after sees it applied. The read-back confirms this much of
                // what the deck sent (a mute changes no level).
                (self.change_volume)(device, change, mute);
                let p = Probe::Volume(device);
                if self.watched.contains(&p) {
                    self.forced.insert(p.clone());
                    if let Some(v) = (self.read_volume)(device) {
                        self.record(p, v, now, out, if mute { 0 } else { change });
                    }
                }
            }
            WatchCmd::Stop => {} // the thread loop handles it
            WatchCmd::Done { probe, result } => {
                if self.in_flight.remove(&probe).is_none() {
                    return; // unwatched meanwhile
                }
                match result {
                    RunResult::Exit { code, .. } if is_missing(code) => {
                        self.warn_once(&probe, "state command not found or not executable");
                        self.record(probe, Value::Toggle(Toggle::Off), now, out, 0);
                    }
                    RunResult::Exit { code, first_line } => {
                        self.failing.remove(&probe);
                        self.record(probe, Value::Toggle(toggle_from_exit(code, first_line)), now, out, 0);
                    }
                    RunResult::TimedOut => {
                        // Send nothing: the key keeps its face.
                        self.warn_once(&probe, "state command timed out");
                        self.next_due.insert(probe.clone(), now + probe.interval());
                    }
                    RunResult::SpawnFailed(e) => {
                        self.warn_once(&probe, &format!("could not run state command: {e}"));
                        self.next_due.insert(probe.clone(), now + probe.interval());
                    }
                }
            }
        }
    }

    /// Do what is due. Volume reads happen inline; the commands to start are returned,
    /// and the thread loop gives each a runner.
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Value, i32)>) -> Vec<(Probe, Arc<AtomicBool>)> {
        // 1. Due refreshes become forced reads, but not while a run is going: its answer
        //    may predate the change. They wait for its `Done`.
        let mut i = 0;
        while i < self.refresh_at.len() {
            let (at, p) = &self.refresh_at[i];
            if *at <= now && !self.in_flight.contains_key(p) {
                let (_, p) = self.refresh_at.swap_remove(i);
                self.forced.insert(p.clone());
                self.next_due.insert(p, now);
            } else {
                i += 1;
            }
        }
        // 2. Start what is due.
        let mut start = Vec::new();
        for p in self.watched.clone() {
            if self.in_flight.contains_key(&p) || self.next_due.get(&p).is_some_and(|t| *t > now) {
                continue;
            }
            match &p {
                Probe::Volume(dev) => match (self.read_volume)(*dev) {
                    // A poll confirms nothing: the deck's in-flight total is untouched.
                    Some(v) => self.record(p, v, now, out, 0),
                    // No `wpctl`, or it timed out: send nothing, try again next interval.
                    None => {
                        self.next_due.insert(p.clone(), now + p.interval());
                    }
                },
                Probe::Command { .. } => {
                    if self.in_flight.len() >= MAX_IN_FLIGHT {
                        // Not due again until something finishes: a `Done` wakes the
                        // thread.
                        self.next_due.insert(p, now + omadeck_core::probe::FAST_INTERVAL);
                        continue;
                    }
                    let cancel = Arc::new(AtomicBool::new(false));
                    self.in_flight.insert(p.clone(), cancel.clone());
                    start.push((p, cancel));
                }
            }
        }
        start
    }

    /// A read landed: end a `Leave` window if settled, schedule the next read, send on
    /// change (or when asked). `applied` is the change this read confirms (0 for a poll
    /// and for every command result).
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Value, i32)>, applied: i32) {
        let busy = matches!(v, Value::Toggle(Toggle::Busy(_)));
        // Decide first, then remove: `from` borrows `fast_until`.
        let window_over = self
            .fast_until
            .get(&p)
            .is_some_and(|(from, until)| (!busy && v != Value::Toggle(from.clone())) || now >= *until);
        if window_over {
            self.fast_until.remove(&p);
        }
        let fast = busy || self.fast_until.contains_key(&p);
        self.next_due.insert(p.clone(), now + if fast { omadeck_core::probe::FAST_INTERVAL } else { p.interval() });
        let forced = self.forced.remove(&p);
        if forced || self.last.get(&p) != Some(&v) {
            self.last.insert(p.clone(), v.clone());
            out.push((p, v, applied));
        }
    }

    /// When the thread must wake next; `None` = block until a command (or a `Done`) comes.
    /// Probes in flight don't count: their `Done` wakes the thread.
    fn next_due(&self) -> Option<Instant> {
        let due = self
            .watched
            .iter()
            .filter(|p| !self.in_flight.contains_key(*p))
            .filter_map(|p| self.next_due.get(p).copied());
        let refresh = self.refresh_at.iter().filter(|(_, p)| !self.in_flight.contains_key(p)).map(|(t, _)| *t);
        due.chain(refresh).min()
    }

    /// Warn once per failing probe, not per poll.
    fn warn_once(&mut self, p: &Probe, what: &str) {
        if self.failing.insert(p.clone()) {
            log::warn!("{what}: {p:?}");
        }
    }

    /// Tell every runner to kill its command (unwatched, or the thread is ending).
    fn cancel_all(&mut self) {
        for c in self.in_flight.values() {
            c.store(true, Ordering::Relaxed);
        }
        self.in_flight.clear();
    }
}

/// `sh -c command` with `LC_ALL=C` (stable tool output) in `$HOME`.
fn state_command(command: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(command).env("LC_ALL", "C").current_dir(std::env::var_os("HOME").unwrap_or_else(|| "/".into()));
    c
}

/// One run of a state command, on a runner thread. Kills the whole process group on
/// timeout or cancel. Blocking, so never call it on the watcher thread.
fn run_state(command: &str, timeout: Duration, cancel: &AtomicBool) -> RunResult {
    let mut job = match Job::spawn(state_command(command)) {
        Ok(j) => j,
        Err(e) => return RunResult::SpawnFailed(e.to_string()),
    };
    loop {
        if let Some(s) = job.try_finish() {
            return RunResult::Exit { code: s.code(), first_line: job.lines().next() };
        }
        if cancel.load(Ordering::Relaxed) || job.started().elapsed() >= timeout {
            job.kill_group();
            return RunResult::TimedOut;
        }
        thread::sleep(RUN_POLL);
    }
}

/// Start the watcher thread.
pub fn start() -> Watcher {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WatchCmd>();
    let (val_tx, val_rx) = mpsc::channel::<(Probe, Value, i32)>();
    // The runners send their `Done` back through the command channel.
    let runner_tx = cmd_tx.clone();
    let thread = thread::Builder::new()
        .name("watch".into())
        .spawn(move || run(&cmd_rx, &val_tx, Poller::default(), runner_tx))
        .expect("spawn watch thread");
    Watcher { cmd: cmd_tx, values: val_rx, thread }
}

fn run(
    cmd_rx: &mpsc::Receiver<WatchCmd>,
    val_tx: &mpsc::Sender<(Probe, Value, i32)>,
    mut poller: Poller,
    runner_tx: mpsc::Sender<WatchCmd>,
) {
    let mut out = Vec::new();
    loop {
        // Sleep until a command comes or a read is due; with nothing watched, until a
        // command comes. No fixed tick.
        let first = match poller.next_due() {
            None => match cmd_rx.recv() {
                Ok(c) => Some(c),
                Err(_) => {
                    poller.cancel_all();
                    return;
                }
            },
            Some(due) => match cmd_rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => {
                    poller.cancel_all();
                    return;
                }
            },
        };
        // Take everything queued, so a burst of volume changes runs as one batch.
        let mut cmds: Vec<WatchCmd> = first.into_iter().collect();
        loop {
            match cmd_rx.try_recv() {
                Ok(c) => cmds.push(c),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    poller.cancel_all();
                    return;
                }
            }
        }
        for c in coalesce(cmds) {
            if c == WatchCmd::Stop {
                poller.cancel_all();
                return;
            }
            poller.handle(c, Instant::now(), &mut out);
        }
        // The commands `run_due` returned each get a runner thread.
        for (probe, cancel) in poller.run_due(Instant::now(), &mut out) {
            let Probe::Command { command, .. } = &probe else { continue };
            let (command, tx) = (command.clone(), runner_tx.clone());
            let spawned = thread::Builder::new().name("watch-run".into()).spawn(move || {
                let result = run_state(&command, STATE_TIMEOUT, &cancel);
                let _ = tx.send(WatchCmd::Done { probe, result }); // watcher gone: nothing to do
            });
            if let Err(e) = spawned {
                log::warn!("could not start a runner thread: {e}");
            }
        }
        for v in out.drain(..) {
            if val_tx.send(v).is_err() {
                poller.cancel_all();
                return;
            }
        }
    }
}

/// Merge back-to-back level changes of the same device into one command (a hard spin
/// queues several while the previous one runs), keeping everything else in order.
fn coalesce(cmds: Vec<WatchCmd>) -> Vec<WatchCmd> {
    let mut out: Vec<WatchCmd> = Vec::with_capacity(cmds.len());
    for c in cmds {
        if let (
            Some(WatchCmd::Volume { device: d1, change: c1, mute: false }),
            WatchCmd::Volume { device: d2, change: c2, mute: false },
        ) = (out.last_mut(), &c)
            && *d1 == *d2
        {
            *c1 = (*c1 + *c2).clamp(-100, 100);
            continue;
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::time::Duration;

    use omadeck_core::Level;
    use omadeck_core::probe::FAST_INTERVAL;

    fn level(percent: i32) -> Value {
        Value::Level(Level { value: percent as f32 / 100.0, text: format!("{percent}%"), muted: false })
    }

    fn cmd(c: &str) -> Probe {
        Probe::Command { command: c.into(), interval_ms: 2000 }
    }

    fn exit(code: i32, first_line: &str) -> RunResult {
        RunResult::Exit { code: Some(code), first_line: (!first_line.is_empty()).then(|| first_line.into()) }
    }

    /// Watch a command probe and start its first run; returns the probe.
    fn watched(p: &mut Poller, probe: Probe, now: Instant, out: &mut Vec<(Probe, Value, i32)>) {
        p.handle(WatchCmd::Watch(vec![probe.clone()]), now, out);
        assert_eq!(p.run_due(now, out).len(), 1, "due at once, handed to a runner");
    }

    #[test]
    fn exit_75_is_busy_with_first_line() {
        let probe = cmd("anything");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        p.handle(WatchCmd::Watch(vec![probe.clone()]), t0, &mut out);
        let start = p.run_due(t0, &mut out);
        assert_eq!(start.len(), 1, "due at once, handed to a runner");
        assert!(p.run_due(t0, &mut out).is_empty(), "never two runs of one probe");
        assert_eq!(p.next_due(), None, "in flight: its Done wakes the thread");
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(75, "Loading 43%") }, t0, &mut out);
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::Busy("Loading 43%".into())), 0)]);
        assert_eq!(p.next_due(), Some(t0 + FAST_INTERVAL), "busy polls fast");
    }

    #[test]
    fn exit_0_on_other_off() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0, &mut out);
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::On), 0)]);
        out.clear();
        // The next interval: a new run, and this time off.
        assert_eq!(p.run_due(t0 + Duration::from_millis(2000), &mut out).len(), 1);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(1, "") },
            t0 + Duration::from_millis(2000),
            &mut out,
        );
        assert_eq!(out, vec![(probe, Value::Toggle(Toggle::Off), 0)]);
    }

    #[test]
    fn busy_text_change_is_resent() {
        let probe = cmd("load");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(75, "10%") }, t0, &mut out);
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::Busy("10%".into())), 0)]);
        out.clear();
        // Busy polls fast; a new busy text counts as a change.
        p.run_due(t0 + FAST_INTERVAL, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(75, "20%") }, t0 + FAST_INTERVAL, &mut out);
        assert_eq!(out, vec![(probe, Value::Toggle(Toggle::Busy("20%".into())), 0)]);
    }

    #[test]
    fn unchanged_value_not_resent() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0, &mut out);
        out.clear();
        p.run_due(t0 + Duration::from_millis(2000), &mut out);
        p.handle(WatchCmd::Done { probe, result: exit(0, "") }, t0 + Duration::from_millis(2000), &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn refresh_resends_unchanged() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0, &mut out);
        out.clear();
        // A refresh schedules forced re-reads at 300 ms and 1.2 s.
        p.handle(WatchCmd::Refresh(probe.clone()), t0, &mut out);
        assert_eq!(p.next_due(), Some(t0 + Duration::from_millis(300)));
        p.run_due(t0 + Duration::from_millis(300), &mut out);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(0, "") },
            t0 + Duration::from_millis(300),
            &mut out,
        );
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::On), 0)], "the unchanged value is resent");
        out.clear();
        // The second scheduled re-read (1.2 s from the refresh) is still there.
        assert_eq!(p.next_due(), Some(t0 + Duration::from_millis(1200)));
        p.run_due(t0 + Duration::from_millis(1200), &mut out);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(0, "") },
            t0 + Duration::from_millis(1200),
            &mut out,
        );
        assert_eq!(out, vec![(probe, Value::Toggle(Toggle::On), 0)]);
    }

    #[test]
    fn refresh_waits_for_the_run_in_flight() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        // A refresh while the run is in flight: its answer may predate the change, so
        // the scheduled re-reads wait for the `Done`.
        p.handle(WatchCmd::Refresh(probe.clone()), t0, &mut out);
        assert!(p.run_due(t0 + Duration::from_millis(300), &mut out).is_empty(), "no second run while one is going");
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(0, "") },
            t0 + Duration::from_millis(300),
            &mut out,
        );
        // The 300 ms re-read is now due and starts a run.
        assert_eq!(p.run_due(t0 + Duration::from_millis(300), &mut out).len(), 1);
    }

    #[test]
    fn leave_polls_fast_until_settled() {
        let probe = cmd("state");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0, &mut out);
        out.clear();
        // A slow press while showing on: fast polling until a read is neither busy nor on.
        p.handle(WatchCmd::Leave { probe: probe.clone(), from: Toggle::On }, t0, &mut out);
        assert_eq!(p.next_due(), Some(t0 + FAST_INTERVAL));
        p.run_due(t0 + FAST_INTERVAL, &mut out);
        // Still on: not resent (unchanged), and the window continues fast.
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0 + FAST_INTERVAL, &mut out);
        assert!(out.is_empty());
        assert_eq!(p.next_due(), Some(t0 + 2 * FAST_INTERVAL));
        // A busy read inside the window keeps it fast too.
        p.run_due(t0 + 2 * FAST_INTERVAL, &mut out);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(75, "starting") },
            t0 + 2 * FAST_INTERVAL,
            &mut out,
        );
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::Busy("starting".into())), 0)]);
        assert_eq!(p.next_due(), Some(t0 + 3 * FAST_INTERVAL));
        // Now off: the value is sent and the normal interval resumes.
        p.run_due(t0 + 3 * FAST_INTERVAL, &mut out);
        out.clear();
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(1, "") }, t0 + 3 * FAST_INTERVAL, &mut out);
        assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::Off), 0)]);
        assert_eq!(p.next_due(), Some(t0 + 3 * FAST_INTERVAL + Duration::from_millis(2000)));
        // An expired window ends even if the state never moved: the normal interval
        // resumes.
        p.fast_until.insert(probe.clone(), (Toggle::On, t0)); // already expired
        let t_due = t0 + 3 * FAST_INTERVAL + Duration::from_millis(2000);
        p.run_due(t_due, &mut out);
        p.handle(WatchCmd::Done { probe, result: exit(0, "") }, t_due, &mut out);
        assert_eq!(p.next_due(), Some(t_due + Duration::from_millis(2000)));
    }

    #[test]
    fn timeout_sends_nothing_and_warns_once() {
        let probe = cmd("sleep 30");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: RunResult::TimedOut }, t0, &mut out);
        assert!(out.is_empty(), "a timeout sends nothing: the key keeps its face");
        assert!(p.failing.contains(&probe), "warned once");
        assert_eq!(p.next_due(), Some(t0 + Duration::from_millis(2000)));
        // A second failure is not warned again.
        p.run_due(t0 + Duration::from_millis(2000), &mut out);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: RunResult::TimedOut },
            t0 + Duration::from_millis(2000),
            &mut out,
        );
        assert!(out.is_empty());
        // A good read clears the failure.
        p.run_due(t0 + 2 * Duration::from_millis(2000), &mut out);
        p.handle(
            WatchCmd::Done { probe: probe.clone(), result: exit(0, "") },
            t0 + 2 * Duration::from_millis(2000),
            &mut out,
        );
        assert!(!p.failing.contains(&probe));
    }

    #[test]
    fn done_for_an_unwatched_probe_is_dropped() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        // Unwatched while the run is in flight: the runner is killed, its `Done` ignored.
        p.handle(WatchCmd::Watch(vec![]), t0, &mut out);
        p.handle(WatchCmd::Done { probe: probe.clone(), result: exit(0, "") }, t0, &mut out);
        assert!(out.is_empty());
        assert!(p.last.is_empty());
    }

    #[test]
    fn in_flight_is_never_started_twice() {
        let probe = cmd("true");
        let mut p = Poller::default();
        let mut out = Vec::new();
        let t0 = Instant::now();
        watched(&mut p, probe.clone(), t0, &mut out);
        assert!(p.run_due(t0, &mut out).is_empty(), "never two runs of one probe");
        assert_eq!(p.next_due(), None, "in flight: its Done wakes the thread");
    }

    #[test]
    fn nothing_watched_means_no_wake() {
        let mut p = Poller::default();
        let mut out = Vec::new();
        assert_eq!(p.next_due(), None);
        assert!(p.run_due(Instant::now(), &mut out).is_empty());
    }

    #[test]
    fn run_state_reads_exit_and_first_line() {
        let done = Arc::new(AtomicBool::new(false));
        let r = run_state("echo Loading 43%; exit 75", Duration::from_secs(2), &done);
        assert_eq!(r, RunResult::Exit { code: Some(75), first_line: Some("Loading 43%".into()) });
    }

    #[test]
    fn run_state_kills_a_hung_command() {
        let done = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let r = run_state("sleep 30 | cat", STATE_TIMEOUT, &done);
        assert_eq!(r, RunResult::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1), "took {:?}", start.elapsed());
    }

    #[test]
    fn run_state_cancel_kills() {
        let cancel = Arc::new(AtomicBool::new(true));
        let r = run_state("sleep 30", Duration::from_secs(10), &cancel);
        assert_eq!(r, RunResult::TimedOut);
    }

    #[test]
    fn large_output_does_not_block() {
        let done = Arc::new(AtomicBool::new(false));
        let r = run_state("yes | head -c 1000000; exit 0", Duration::from_secs(5), &done);
        assert_eq!(r, RunResult::Exit { code: Some(0), first_line: Some("y".into()) });
    }

    // A fake output device: changes add up, reads return the sum. Only the test below
    // uses it, so tests running at once don't share it.
    static FAKE: AtomicI32 = AtomicI32::new(50);
    fn fake_read(_: AudioDevice) -> Option<Value> {
        Some(level(FAKE.load(Ordering::SeqCst)))
    }
    fn fake_change(_: AudioDevice, change: i32, _: bool) {
        FAKE.fetch_add(change, Ordering::SeqCst);
    }

    #[test]
    fn volume_changes_run_in_order_and_are_read_back() {
        let mut p = Poller { read_volume: fake_read, change_volume: fake_change, ..Poller::default() };
        let out_dev = Probe::Volume(AudioDevice::Output);
        let now = Instant::now();
        let mut out = Vec::new();
        p.handle(WatchCmd::Watch(vec![out_dev.clone()]), now, &mut out);
        p.run_due(now, &mut out);
        assert_eq!(out, vec![(out_dev.clone(), level(50), 0)]);
        out.clear();
        // Each change is read back right after it, in order, confirming the change.
        for _ in 0..2 {
            p.handle(WatchCmd::Volume { device: AudioDevice::Output, change: 5, mute: false }, now, &mut out);
        }
        assert_eq!(out, vec![(out_dev.clone(), level(55), 5), (out_dev.clone(), level(60), 5)]);
        out.clear();
        // Not due again before the interval.
        p.run_due(now + Duration::from_millis(500), &mut out);
        assert!(out.is_empty());
        // A change that leaves the level as it was is still sent: the deck shows a guess.
        p.handle(WatchCmd::Volume { device: AudioDevice::Output, change: 0, mute: false }, now, &mut out);
        assert_eq!(out, vec![(out_dev, level(60), 0)]);
    }

    #[test]
    fn unchanged_value_is_not_resent_until_watched_anew() {
        let mut p = Poller { read_volume: |_| Some(level(40)), ..Poller::default() };
        let out_dev = Probe::Volume(AudioDevice::Output);
        let now = Instant::now();
        let mut out = Vec::new();
        p.handle(WatchCmd::Watch(vec![out_dev.clone()]), now, &mut out);
        p.run_due(now, &mut out);
        assert_eq!(out.len(), 1);
        out.clear();
        // The next poll reads the same level: nothing is sent.
        p.run_due(now + Duration::from_secs(1), &mut out);
        assert!(out.is_empty());
        assert_eq!(p.next_due(), Some(now + Duration::from_secs(2)));
        // Unwatched (unplug) and watched again (replug): sent again.
        p.handle(WatchCmd::Watch(vec![]), now, &mut out);
        assert_eq!(p.next_due(), None);
        p.handle(WatchCmd::Watch(vec![out_dev]), now, &mut out);
        p.run_due(now, &mut out);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn stop_ends_the_thread() {
        let w = start();
        let begin = Instant::now();
        w.stop(); // returns only once the thread has ended
        assert!(begin.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn coalesce_merges_only_adjacent_level_changes_of_one_device() {
        use AudioDevice::*;
        let v = |device, change, mute| WatchCmd::Volume { device, change, mute };
        assert_eq!(coalesce(vec![v(Output, 5, false), v(Output, 5, false)]), vec![v(Output, 10, false)]);
        assert_eq!(coalesce(vec![v(Output, 60, false), v(Output, 60, false)]), vec![v(Output, 100, false)]);
        // A mute, another device or another command in between keeps them apart.
        let kept = vec![v(Output, 5, false), v(Output, 0, true), v(Output, 5, false)];
        assert_eq!(coalesce(kept.clone()), kept);
        let kept = vec![v(Output, 5, false), v(Input, 5, false)];
        assert_eq!(coalesce(kept.clone()), kept);
        let kept = vec![v(Output, 5, false), WatchCmd::Stop, v(Output, 5, false)];
        assert_eq!(coalesce(kept.clone()), kept);
    }
}
