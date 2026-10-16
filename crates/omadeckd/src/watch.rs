//! The watcher: one thread that reads system state off the deck loop and runs the volume
//! changes the deck asks for, in order, each followed by a fresh read. It sends back only
//! values that changed, or that the deck asked for again.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::thread;
use std::time::Instant;

use omadeck_core::probe::{Probe, Value};
use omadeck_core::{AudioDevice, action, audio};

/// A message the deck loop sends the watcher.
#[derive(Debug, Clone, PartialEq)]
pub enum WatchCmd {
    /// The complete set to read from now on (replaces the previous set).
    Watch(Vec<Probe>),
    /// Something changed it elsewhere: re-read after `Probe::refresh_after()` and send the
    /// result even if unchanged.
    #[allow(dead_code)]
    Refresh(Probe),
    /// Change a device's volume here, after any earlier change, then re-read it at once
    /// and send the result even if unchanged (the deck is showing a guess).
    Volume { device: AudioDevice, change: i32, mute: bool },
    /// End the thread (shutdown).
    Stop,
}

pub struct Watcher {
    pub cmd: mpsc::Sender<WatchCmd>,
    pub values: mpsc::Receiver<(Probe, Value)>,
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
            read_volume,
            change_volume,
        }
    }
}

impl Poller {
    /// Act on one command; values to send go to `out`. `Stop` is the thread loop's.
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Value)>) {
        match cmd {
            WatchCmd::Watch(set) => {
                let mut set_d: Vec<Probe> = Vec::new();
                for p in set {
                    if !set_d.contains(&p) {
                        set_d.push(p);
                    }
                }
                // What stays watched keeps its last value (the deck kept it too); what is
                // new is read now.
                self.last.retain(|p, _| set_d.contains(p));
                self.next_due.retain(|p, _| set_d.contains(p));
                self.forced.retain(|p| set_d.contains(p));
                for p in &set_d {
                    self.next_due.entry(p.clone()).or_insert(now);
                }
                self.watched = set_d;
            }
            WatchCmd::Refresh(p) => {
                if let Some(after) = p.refresh_after().first()
                    && self.watched.contains(&p)
                {
                    self.forced.insert(p.clone());
                    self.next_due.insert(p, now + *after);
                }
            }
            WatchCmd::Volume { device, change, mute } => {
                // Runs here, in order: a change never races the one before it, and the
                // read right after sees it applied.
                (self.change_volume)(device, change, mute);
                let p = Probe::Volume(device);
                if self.watched.contains(&p) {
                    self.forced.insert(p.clone());
                    if let Some(v) = (self.read_volume)(device) {
                        self.record(p, v, now, out);
                    }
                }
            }
            WatchCmd::Stop => {}
        }
    }

    /// Read every probe that is due.
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Value)>) {
        for p in self.watched.clone() {
            if self.next_due.get(&p).is_some_and(|t| *t > now) {
                continue;
            }
            let read = match &p {
                Probe::Volume(dev) => (self.read_volume)(*dev),
            };
            match read {
                Some(v) => self.record(p, v, now, out),
                // No `wpctl`, or it timed out: send nothing, try again next interval.
                None => {
                    let next = now + p.interval();
                    self.next_due.insert(p, next);
                }
            }
        }
    }

    /// A read came back: send it if it changed (or was asked for), and schedule the next.
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Value)>) {
        let forced = self.forced.remove(&p);
        self.next_due.insert(p.clone(), now + p.interval());
        if forced || self.last.get(&p) != Some(&v) {
            self.last.insert(p.clone(), v.clone());
            out.push((p, v));
        }
    }

    /// When the next read is due; `None` with nothing watched.
    fn next_due(&self) -> Option<Instant> {
        self.next_due.values().min().copied()
    }
}

/// Start the watcher thread.
pub fn start() -> Watcher {
    let (cmd_tx, cmd_rx) = mpsc::channel::<WatchCmd>();
    let (val_tx, val_rx) = mpsc::channel::<(Probe, Value)>();
    let thread = thread::Builder::new()
        .name("watch".into())
        .spawn(move || run(&cmd_rx, &val_tx, Poller::default()))
        .expect("spawn watch thread");
    Watcher { cmd: cmd_tx, values: val_rx, thread }
}

fn run(cmd_rx: &mpsc::Receiver<WatchCmd>, val_tx: &mpsc::Sender<(Probe, Value)>, mut poller: Poller) {
    let mut out = Vec::new();
    loop {
        // Sleep until a command comes or a read is due; with nothing watched, until a
        // command comes. No fixed tick.
        let first = match poller.next_due() {
            None => match cmd_rx.recv() {
                Ok(c) => Some(c),
                Err(_) => return,
            },
            Some(due) => match cmd_rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(c) => Some(c),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        // Take everything queued, so a burst of volume changes runs as one batch.
        let mut cmds: Vec<WatchCmd> = first.into_iter().collect();
        loop {
            match cmd_rx.try_recv() {
                Ok(c) => cmds.push(c),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        for c in coalesce(cmds) {
            if c == WatchCmd::Stop {
                return;
            }
            poller.handle(c, Instant::now(), &mut out);
        }
        poller.run_due(Instant::now(), &mut out);
        for v in out.drain(..) {
            if val_tx.send(v).is_err() {
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

    fn level(percent: i32) -> Value {
        Value::Level(Level { value: percent as f32 / 100.0, text: format!("{percent}%"), muted: false })
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
        assert_eq!(out, vec![(out_dev.clone(), level(50))]);
        out.clear();
        // Each change is read back right after it, in order.
        for _ in 0..2 {
            p.handle(WatchCmd::Volume { device: AudioDevice::Output, change: 5, mute: false }, now, &mut out);
        }
        assert_eq!(out, vec![(out_dev.clone(), level(55)), (out_dev.clone(), level(60))]);
        out.clear();
        // Not due again before the interval.
        p.run_due(now + Duration::from_millis(500), &mut out);
        assert!(out.is_empty());
        // A change that leaves the level as it was is still sent: the deck shows a guess.
        p.handle(WatchCmd::Volume { device: AudioDevice::Output, change: 0, mute: false }, now, &mut out);
        assert_eq!(out, vec![(out_dev, level(60))]);
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
