//! The daemon's state and everything the deck loop does with it: inputs, actions, page
//! moves, brightness, values from the watcher, reloads and painting.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use anyhow::Result;
use omadeck_core::config::apply_brightness;
use omadeck_core::device::{DeviceStateUpdate, StreamDeckError};
use omadeck_core::ipc::{Command, Event, Reply};
use omadeck_core::probe::{self, FAST_WINDOW, Probe, SETTLE, Toggle, Value, Values};
use omadeck_core::proc;
use omadeck_core::state::State;
use omadeck_core::{
    Action, AudioDevice, Config, DialConfig, Direction, KeyConfig, KeyStatus, Level, LevelSource, Live, PanelStatus,
    Renderer, action, audio, tap_to_dial,
};

use crate::deck::{self, BackHold, Deck};
use crate::hub::{Hub, Request};
use crate::hypr::{self, HyprEvent};
use crate::hypr_worker::{HyprJob, HyprWorker};
use crate::nav::Nav;
use crate::saver::StateSaver;
use crate::sequence::{RunOf, Sequencer};
use crate::watch::{WatchCmd, Watcher};

/// The longest a pass waits for the deck; also the latency of hub commands and values.
pub const READ_TIMEOUT: Duration = Duration::from_millis(100);
/// The Plus's LCD can ignore the first writes right after connect; repaint once after this.
const POST_CONNECT_REPAINT: Duration = Duration::from_secs(1);
/// Minimum horizontal distance, in strip pixels, for a swipe to count as a flick.
const FLICK_MIN_PX: i32 = 50;
/// After a read that returned input, at most this many more queued HID reports are taken
/// in the same pass (fix/review's), so a fast spin is handled together and painted once.
const MAX_DRAIN: usize = 32;

/// A key's place: its page (`None` is home) and index.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId {
    pub page: Option<String>,
    pub index: u8,
}

/// A tracked command running for a key (`busy_while_running`): the job and its latest
/// output line.
pub struct Running {
    pub job: proc::Job,
    /// The latest output line (the busy face's text).
    pub text: String,
}

impl fmt::Debug for Running {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Running").field("text", &self.text).finish_non_exhaustive()
    }
}

/// An optimistic guess waiting for the real state to confirm it. Presses build them
/// and `apply_value` consults them.
#[derive(Debug, Clone, PartialEq)]
enum Settle {
    /// A fast toggle was pressed and shows `value`: ignore reads that contradict it until
    /// `until`.
    Expect { value: Value, until: Instant },
    /// A slow toggle was pressed while showing `from`: ignore reads still equal to it
    /// until `until`.
    Leaving { from: Toggle, until: Instant },
}

impl Settle {
    fn accepts(&self, v: &Value, now: Instant) -> bool {
        match self {
            Settle::Expect { value, until } => now >= *until || v == value,
            Settle::Leaving { from, until } => now >= *until || *v != Value::Toggle(from.clone()),
        }
    }
}

/// What outlives the deck: an unplug and replug keeps it all.
#[derive(Debug, Default)]
pub struct Runtime {
    /// What the panels show: probe values and the live brightness.
    pub values: Values,
    /// The persisted state. Changed here, then sent whole to the saver.
    pub state: State,
    /// Press toggles that are on (they have no source of truth, so they reset on a
    /// daemon restart).
    pub flipped: HashSet<KeyId>,
    /// Optimistic guesses waiting for the real state to confirm them.
    settle: HashMap<Probe, Settle>,
    /// `busy_while_running` commands, per key.
    pub jobs: HashMap<KeyId, Running>,
    /// The focused Hyprland window, from the event listener.
    #[allow(dead_code)] // read by PR 10 and PR 15
    pub window: Option<hypr::ActiveWindow>,
}

/// What key `i` on `page` shows now: core's `key_status` (the ONE builder) with the two
/// per-key things `Values` can't hold laid over it. A running job wins; a press toggle
/// reads `flipped`.
pub fn key_state(cfg: &Config, rt: &Runtime, page: Option<&str>, i: u8, pressed: bool) -> KeyStatus {
    let key = cfg.key(page, i);
    let mut st = probe::key_status(key, &rt.values, pressed);
    let id = KeyId { page: page.map(str::to_string), index: i };
    if let Some(r) = rt.jobs.get(&id) {
        st.state = Toggle::Busy(r.text.clone());
    } else if key.is_some_and(KeyConfig::is_press_toggle) && rt.flipped.contains(&id) {
        st.state = Toggle::On;
    }
    st
}

/// Whether a press of `key` waits for its release: the action `press_key` would run
/// (the on action while on) is `Back`.
fn waits_for_release(key: Option<&KeyConfig>, on: bool) -> bool {
    key.is_some_and(|k| matches!(k.action_for(on), Action::Back))
}

/// What follows a finished job. Pure apart from `rt`, so it is testable without a deck.
struct JobEnd {
    failed: Option<String>,
    refresh: Option<Probe>,
}

fn finish_job(rt: &mut Runtime, cfg: &Config, id: &KeyId, status: std::process::ExitStatus) -> JobEnd {
    rt.jobs.remove(id);
    let ok = status.success();
    let failed = (!ok).then(|| match status.code() {
        Some(c) => format!("exited with status {c}"),
        None => "was killed".to_string(),
    });
    let Some(k) = cfg.key(id.page.as_deref(), id.index) else {
        return JobEnd { failed, refresh: None };
    };
    let refresh = match k.state_probe() {
        Some(p @ Probe::Command { .. }) => Some(p), // ask the real state now; no guessing
        Some(Probe::Volume(_)) => None,
        Some(_) => None, // a state probe is only ever a volume or a command
        None => {
            // A press toggle flips only when its command succeeded.
            if ok && k.on.is_some() && !rt.flipped.remove(id) {
                rt.flipped.insert(id.clone());
            }
            None
        }
    };
    JobEnd { failed, refresh }
}

impl Runtime {
    /// The runtime at start, from the saved state and the config's `brightness`
    /// (`None` when config.toml didn't load: a fallback default is not an edit). The
    /// config wins when its value differs from the one the state last saw; either way the
    /// state now records the config's value.
    pub fn start(mut state: State, cfg_brightness: Option<u8>) -> Self {
        let brightness = match cfg_brightness {
            Some(c) => {
                let b = state.start_brightness(c);
                state.config_brightness = Some(c);
                b
            }
            None => state.brightness.unwrap_or_else(|| Config::default().brightness),
        };
        state.brightness = Some(brightness);
        Runtime { values: Values { brightness, ..Values::default() }, state, ..Self::default() }
    }
}

/// Where an input came from, so a failure is reported against the right key or dial.
/// PR 16 adds `LongTap(u8)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Key(u8),
    Turn(u8),
    Press(u8),
    Tap(u8),
    Flick,
    /// A control command run as itself (`omadeckd page`, `brightness`, Try it now): its
    /// reply carries a failure, so nothing is published for it.
    Ctl,
    /// A multi action's step: `run_due_steps` reports its failure against the run.
    Step,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Key(i) => write!(f, "key {i}"),
            Source::Turn(i) | Source::Press(i) | Source::Tap(i) => write!(f, "dial {i}"),
            Source::Flick => write!(f, "strip flick"),
            Source::Ctl => write!(f, "omadeckd"),
            Source::Step => write!(f, "step"),
        }
    }
}

pub struct Daemon {
    pub cfg: Config,
    pub renderer: Renderer,
    pub nav: Nav,
    /// A page the configurator asked for that isn't in the config yet (it saves a moment later).
    pub wanted_page: Option<String>,
    pub hub: Hub,
    pub watcher: Watcher,
    pub saver: StateSaver,
    /// Runs every Hyprland dispatch off the loop, in order.
    pub hypr: HyprWorker,
    /// Hyprland's events (the active workspace and window); `None` outside Hyprland.
    /// Read only; dispatches go through `hypr`.
    pub hypr_events: Option<std::sync::mpsc::Receiver<HyprEvent>>,
    pub rt: Runtime,
    pub deck: Option<Deck>,
    /// What was last published per strip panel (`Event::DialStatus`), so only changes go out.
    published: HashMap<u8, PanelStatus>,
    /// What was last published per key (`Event::KeyState`), so only changes go out.
    /// `publish_keys` (in `flush`) reads and writes it.
    published_keys: HashMap<KeyId, KeyStatus>,
    /// Volume changes sent to the watcher but not yet confirmed by its read-backs, per
    /// device. The bar shows confirmed + in-flight, so it never drops to a value the
    /// queued changes haven't reached yet (a fast spin would otherwise snap back as the
    /// read-backs catch up).
    in_flight: HashMap<AudioDevice, i32>,
    pub reload_at: Option<Instant>,
    pub repaint_at: Option<Instant>,
    /// The multi actions running, timed by the loop.
    seq: Sequencer,
}

impl Daemon {
    /// `cfg_loaded` is false when config.toml didn't parse and `cfg` is the empty
    /// fallback: its `brightness` is then no edit.
    pub fn new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self {
        let renderer = Renderer::new(&cfg.style);
        let saved = State::load();
        let rt = Runtime::start(saved.clone(), cfg_loaded.then_some(cfg.brightness));
        if rt.state != saved {
            saver.send(rt.state.clone());
        }
        hub.publish(Event::Brightness { value: rt.values.brightness });
        Daemon {
            cfg,
            renderer,
            nav: Nav::default(),
            wanted_page: None,
            hub,
            watcher,
            saver,
            hypr: HyprWorker::start(),
            hypr_events: hypr::start(),
            rt,
            deck: None,
            published: HashMap::new(),
            published_keys: HashMap::new(),
            in_flight: HashMap::new(),
            reload_at: None,
            repaint_at: None,
            seq: Sequencer::default(),
        }
    }

    // --- the deck coming and going ---------------------------------------------------------

    /// A deck was found: paint it, watch what its dials and keys show, tell the
    /// configurators.
    pub fn attach(&mut self, mut d: Deck) {
        let keys = self.key_states(d.info.key_count());
        if let Err(e) =
            deck::paint_all(&mut d, &self.cfg, self.nav.current(), &mut self.renderer, &self.rt.values, &keys)
        {
            log::warn!("initial paint failed: {e}");
        }
        self.hub.publish(Event::Connected { deck: d.info.clone() });
        self.deck = Some(d);
        // The configurators forgot every panel and key state on `Disconnected` (so did
        // the hub's replay): send them all again.
        self.published.clear();
        self.published_keys.clear();
        self.repaint_at = Some(Instant::now() + POST_CONNECT_REPAINT);
        self.watch();
    }

    /// The deck went away. `rt` stays: the values and brightness are there on replug.
    pub fn detach(&mut self, e: &StreamDeckError) {
        if let Some(d) = self.deck.take() {
            log::warn!("lost {} ({e}); waiting for it to come back", d.info.model);
        }
        self.published.clear();
        self.published_keys.clear();
        self.repaint_at = None;
        // `in_flight` stays: the watcher still acknowledges every queued change.
        let _ = self.watcher.cmd.send(WatchCmd::Watch(Vec::new()));
        self.hub.publish(Event::Disconnected);
    }

    /// Tell the watcher what the deck's dials and the current page's keys show. Sent on
    /// connect, on every reload and on every page change.
    fn watch(&mut self) {
        let dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        // No deck: no dials and no keys, so nothing is polled until one is attached.
        let probes = probe::watched_probes(&self.cfg, self.nav.current(), dials, self.key_count());
        let _ = self.watcher.cmd.send(WatchCmd::Watch(probes));
    }

    // --- one pass of the loop --------------------------------------------------------------

    /// The config changed on disk, or `omadeckd reload` asked. A typo while hand-editing
    /// shouldn't wipe the deck: on a bad file it keeps the last good config, restyles and
    /// repaints as usual, then returns `Config::load`'s error for the caller to report.
    pub fn reload(&mut self) -> Result<()> {
        let mut hand_edit = None;
        let loaded = match Config::load() {
            Ok(new_cfg) => {
                hand_edit = self.rt.state.config_edit(new_cfg.brightness);
                self.cfg = new_cfg;
                self.keep_toggle_state();
                Ok(())
            }
            Err(e) => Err(e),
        };
        self.renderer.update_style(&self.cfg.style);
        self.renderer.reload_theme();
        self.renderer.clear_cache();
        log::info!("reloaded config (theme: {})", self.renderer.theme.name);
        let mut moved = self.nav.retain_existing(&self.cfg);
        if let Some(p) = self.wanted_page.take_if(|p| self.cfg.has_page(p)) {
            moved |= self.nav.go(Some(&p));
        }
        if moved {
            self.hub.publish(Event::PageChanged { page: self.nav.current().map(str::to_string) });
        }
        // A hand edit of `brightness` wins over the saved value, like any other field.
        // The state records the value it has now seen, so the next start doesn't apply
        // it again over a later dial turn.
        if let Some(v) = hand_edit {
            self.rt.state.config_brightness = Some(v);
            self.saver.send(self.rt.state.clone());
            self.set_brightness(Some(v), 0);
        }
        self.watch();
        self.paint_all();
        loaded
    }

    /// Run what the configurators and `omadeckd` verbs asked for. Only the latest page
    /// request and brightness from a configurator count, so flicking through pages there
    /// doesn't echo a trail of stale page changes back. Every other command is answered
    /// at once, in arrival order.
    pub fn handle_commands(&mut self) {
        let mut show = None;
        let mut bright = None;
        // Collected first: `handle_command` needs `&mut self`, the iterator borrows `self.hub`.
        let requests: Vec<Request> = self.hub.commands().collect();
        for req in requests {
            match req.cmd {
                Command::ShowPage { page } => show = Some(page),
                Command::SetBrightness { value } => bright = Some(value),
                cmd => {
                    let r = self.handle_command(cmd);
                    req.reply.send(&r);
                }
            }
        }
        // With no deck too: it's saved and applied on connect.
        if let Some(v) = bright {
            self.set_brightness(Some(v), 0);
        }
        if let Some(page) = show {
            self.wanted_page = None;
            match page {
                Some(p) if !self.cfg.has_page(&p) => self.wanted_page = Some(p),
                page => {
                    if self.nav.go(page.as_deref()) {
                        self.show_page();
                    }
                }
            }
        }
    }

    /// Answer one control command. Never blocks: it does what a key press does.
    fn handle_command(&mut self, cmd: Command) -> Reply {
        match cmd {
            Command::Press { index, page } => {
                let Some(n) = self.deck.as_ref().map(|d| d.info.key_count()) else { return Reply::NoDeck };
                if index >= n {
                    return Reply::error(format!("no key {index}; this deck has {n} keys"));
                }
                let page = match page {
                    Some(p) if p.eq_ignore_ascii_case(omadeck_core::HOME) => None,
                    Some(p) if !self.cfg.has_page(&p) => {
                        return Reply::error(format!("there's no page named “{p}”"));
                    }
                    page => page,
                };
                let shown = self.nav.current().map(str::to_string);
                let act = self.key_action_now(page.as_deref(), index).unwrap_or_default();
                let message = format!("key {index}: {}", act.summary());
                if page != shown {
                    // Not on the deck: run what the key would run now, no flash, no flip.
                    return match self.try_input(Source::Ctl, &act, 1) {
                        Ok(()) => Reply::ok(message),
                        Err(e) => Reply::error(format!("{e:#}")),
                    };
                }
                let res = self.key_down(index, false);
                // No `ButtonUp` follows: the key flashes instead. Not when the press moved
                // the page: the new page's key at that place wasn't pressed.
                if self.nav.current() == shown.as_deref()
                    && let Some(f) = self.deck.as_mut().and_then(|d| d.key_flash.get_mut(index as usize))
                {
                    *f = Some(Instant::now());
                }
                match res {
                    Ok(()) => Reply::ok(message),
                    Err(e) => Reply::error(format!("{e:#}")),
                }
            }
            Command::Dial { index, dir, steps } => {
                let Some(n) = self.deck.as_ref().map(|d| d.info.dials) else { return Reply::NoDeck };
                if index >= n {
                    return Reply::error(format!("no dial {index}; this deck has {n} dials"));
                }
                let steps = steps.clamp(1, 8);
                let summary = self.cfg.dial(index).map(|d| d.action(dir).summary()).unwrap_or_else(|| {
                    // No `[[dial]]` for it: what an empty slot says.
                    Action::None.summary()
                });
                let (res, what) = match dir {
                    Direction::Left => (self.turn_dial(index, -(steps as i8), false), "left"),
                    Direction::Right => (self.turn_dial(index, steps as i8, false), "right"),
                    Direction::Press => (self.tap_dial(index), "press"),
                };
                let times = if steps > 1 && dir != Direction::Press { format!(" x{steps}") } else { String::new() };
                match res {
                    Ok(()) => Reply::ok(format!("dial {index} {what}{times}: {summary}")),
                    Err(e) => Reply::error(format!("{e:#}")),
                }
            }
            Command::Run { action } => match self.try_input(Source::Ctl, &action, 1) {
                Ok(()) => Reply::ok(run_message(&action, self.nav.current(), self.rt.values.brightness)),
                Err(e) => Reply::error(format!("{e:#}")),
            },
            Command::Page { page } => {
                if let Some(p) = &page
                    && !self.cfg.has_page(p)
                {
                    return Reply::error(format!("there's no page named “{p}”"));
                }
                if self.nav.go(page.as_deref()) {
                    self.show_page();
                }
                Reply::ok(format!("page {}", self.nav.current().unwrap_or(omadeck_core::HOME)))
            }
            Command::Reload => {
                self.reload_at = None;
                match self.reload() {
                    Ok(()) => Reply::ok(format!("reloaded config (theme: {})", self.renderer.theme.name)),
                    Err(e) => Reply::error(format!("{e:#}; keeping the previous config")),
                }
            }
            Command::State => Reply::State {
                deck: self.deck.as_ref().map(|d| d.info.clone()),
                page: self.nav.current().map(str::to_string),
                pages: self.cfg.page_names(),
                brightness: self.rt.values.brightness,
            },
            // `handle_commands` takes these and never answers them.
            Command::ShowPage { .. } | Command::SetBrightness { .. } => Reply::ok(""),
        }
    }

    /// The action key `i` on `page` runs now: the on action while it's on.
    fn key_action_now(&self, page: Option<&str>, i: u8) -> Option<Action> {
        let k = self.cfg.key(page, i)?;
        let on = key_state(&self.cfg, &self.rt, page, i, false).state == Toggle::On;
        Some(k.action_for(on).clone())
    }

    /// Take the watcher's new values.
    pub fn apply_values(&mut self) {
        while let Ok((p, v, applied)) = self.watcher.values.try_recv() {
            self.apply_value(p, v, applied);
        }
    }

    /// Take the Hyprland listener's events. With no deck too: the workspace lands in
    /// `rt.values`, ready for the next attach.
    pub fn apply_hypr(&mut self) {
        // Collect first: `apply_value` takes `&mut self`.
        let events: Vec<HyprEvent> = match &self.hypr_events {
            Some(rx) => rx.try_iter().collect(),
            None => return,
        };
        for ev in events {
            match ev {
                HyprEvent::Workspace { name, .. } => self.apply_value(Probe::Workspace, Some(Value::Text(name)), 0),
                // PR 15 hooks in here too.
                HyprEvent::ActiveWindow(w) => {
                    // Always, nothing focused included: the watcher only remembers it
                    // when no focused dial is watched, and a focused dial with no window
                    // falls back to its sticky app.
                    let focus = w.as_ref().map(|w| audio::Window { pid: w.pid, class: w.class.clone() });
                    let _ = self.watcher.cmd.send(WatchCmd::Focus(focus));
                    self.rt.window = w;
                }
            }
        }
    }

    /// A read-back from the watcher: `applied` of what we sent is confirmed by this read.
    /// What's still in flight hasn't happened yet, so it's shown on top of the read — the
    /// bar only ever moves forward while a spin is in flight, and settles as the queue
    /// drains, instead of snapping back to each stale read. A read the daemon's own
    /// guess is waiting to be confirmed by is dropped until it expires.
    pub fn apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32) {
        // Every volume change is acknowledged exactly once, read or not: take its part
        // off the in-flight total first, so the total can't drift.
        if let Probe::Volume(dev) = &probe
            && applied != 0
        {
            let f = self.in_flight.entry(*dev).or_insert(0);
            *f -= applied;
            if *f == 0 {
                self.in_flight.remove(dev);
            }
        }
        // An acknowledgement without a read: nothing to show.
        let Some(mut value) = value else { return };
        if let Some(s) = self.rt.settle.get(&probe) {
            if !s.accepts(&value, Instant::now()) {
                return;
            }
            self.rt.settle.remove(&probe);
        }
        if let Probe::Volume(dev) = &probe
            && let Value::Level(read) = &value
        {
            value = Value::Level(settle_volume(self.in_flight.get(dev).copied().unwrap_or(0), read));
        }
        if self.rt.values.probes.get(&probe) == Some(&value) {
            return;
        }
        self.store_value(probe, value);
    }

    /// The one place a value is stored and what shows it (the panels and the keys that
    /// read the probe) marked for repainting at the end of the pass.
    fn store_value(&mut self, probe: Probe, value: Value) {
        self.rt.values.probes.insert(probe.clone(), value);
        self.mark_probe(&probe);
    }

    /// Everything on screen that shows `probe`: the dials and the current page's keys.
    /// Nothing is published here: `flush` compares and publishes, for panels and keys
    /// alike.
    fn mark_probe(&mut self, probe: &Probe) {
        if let Some(d) = &mut self.deck {
            d.dirty_where(&self.cfg, |dial| probe::shows(dial, probe));
        }
        let page = self.nav.current().map(str::to_string);
        let n = self.key_count();
        for i in 0..n {
            let k = self.cfg.key(page.as_deref(), i);
            if k.and_then(KeyConfig::state_probe).as_ref() == Some(probe)
                || k.and_then(|k| k.live.as_ref()).map(Live::probe).as_ref() == Some(probe)
            {
                self.mark_key(page.as_deref(), i);
            }
        }
    }

    /// Repaint key `i` at the end of this pass, if it is on the deck now.
    fn mark_key(&mut self, page: Option<&str>, i: u8) {
        if page == self.nav.current()
            && let Some(d) = &mut self.deck
        {
            d.mark_key_dirty(i);
        }
    }

    /// How many keys the deck has (0 with none).
    fn key_count(&self) -> u8 {
        self.deck.as_ref().map_or(0, |d| d.info.key_count())
    }

    /// What each of the current page's first `n` keys shows now, for painting. A key held
    /// down, or flashing after a ctl press, is drawn pressed, so the end-of-pass paint
    /// keeps it pressed (BUGS.md 65).
    fn key_states(&self, n: u8) -> Vec<KeyStatus> {
        (0..n)
            .map(|i| {
                let held = self.deck.as_ref().is_some_and(|d| d.key_held(i));
                key_state(&self.cfg, &self.rt, self.nav.current(), i, held)
            })
            .collect()
    }

    /// Groundwork's pattern for panels, for keys: build each key's status and publish
    /// the ones that differ from what was last sent. Called first thing in `flush`,
    /// before its deck check, so a job's progress reaches the configurator with no deck
    /// too. That covers watcher values, guesses, jobs, connect, reload and page change
    /// in one place.
    fn publish_keys(&mut self) {
        let page = self.nav.current().map(str::to_string);
        let mut ids: HashSet<KeyId> = self.published_keys.keys().chain(self.rt.jobs.keys()).cloned().collect();
        for i in 0..self.key_count() {
            if self
                .cfg
                .key(page.as_deref(), i)
                .is_some_and(|k| k.on.is_some() || k.busy_while_running || k.live.is_some())
            {
                ids.insert(KeyId { page: page.clone(), index: i });
            }
        }
        for id in ids {
            let status = key_state(&self.cfg, &self.rt, id.page.as_deref(), id.index, false);
            let last = self.published_keys.get(&id).cloned().unwrap_or_default();
            if last != status {
                self.hub.publish(Event::KeyState { page: id.page.clone(), index: id.index, status: status.clone() });
            }
            if status == KeyStatus::default() {
                self.published_keys.remove(&id);
            } else {
                self.published_keys.insert(id, status);
            }
        }
    }

    /// What a reload keeps of the toggle state: flips of keys that are still press
    /// toggles, settles and values of probes the config still shows, and always the
    /// workspace (nothing re-reads it, and the listener sends only changes).
    fn keep_toggle_state(&mut self) {
        retain_flips(&self.cfg, &mut self.rt.flipped);
        let n_dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        let watched = probe::watched_probes(&self.cfg, self.nav.current(), n_dials, self.key_count());
        self.rt.settle.retain(|p, _| watched.contains(p));
        // Values: the dials' probes plus every key's, on any page, so a page you return
        // to shows the last known faces at once.
        let all = config_probes(&self.cfg, n_dials);
        self.rt.values.probes.retain(|p, _| *p == Probe::Workspace || all.contains(p));
    }

    /// The next moment something is due: a tap flash ending, a held Back key going Home,
    /// the post-connect repaint, a debounced reload, a multi action's next step. PR 16's
    /// timers add theirs here.
    pub fn next_deadline(&self) -> Option<Instant> {
        let flash = self.deck.as_ref().and_then(Deck::flash_deadline);
        let hold = self.deck.as_ref().and_then(|d| d.back_held).map(|h| h.deadline());
        [flash, hold, self.repaint_at, self.reload_at, self.seq.next_due()].into_iter().flatten().min()
    }

    /// One pass with a deck: read and handle its input until it's quiet, then paint what
    /// changed, once. An error means the deck is gone.
    pub fn pass(&mut self) -> Result<(), StreamDeckError> {
        let now = Instant::now();
        if self.repaint_at.is_some_and(|t| now >= t) {
            self.repaint_at = None;
            self.paint_all();
        }
        if let Some(h) = self.deck.as_mut().and_then(|d| d.back_held.take_if(|h| now >= h.deadline())) {
            log::info!("key {}: held Back, going Home", h.key);
            if self.nav.home() {
                self.show_page();
            }
        }
        let Some(d) = &mut self.deck else { return Ok(()) };
        d.expire_flashes(now);
        // No `KeyUp`: only a ctl press flashes a key, and it published no `KeyDown`.
        d.expire_key_flashes(now);
        // A clone of the `Arc`, so reading doesn't hold a borrow of `self` while the
        // updates are handled.
        let reader = d.reader.clone();
        let mut timeout = wait_for(now, self.next_deadline());
        let mut result = Ok(());
        for _ in 0..=MAX_DRAIN {
            match reader.read(Some(timeout)) {
                Ok(updates) if updates.is_empty() => break,
                Ok(updates) => {
                    for u in updates {
                        self.handle_update(u);
                    }
                }
                Err(e) => {
                    result = Err(e);
                    break;
                }
            }
            // The rest of a burst (a spun dial sends a report per detent) is drained
            // without waiting, and painted once below.
            timeout = Duration::ZERO;
        }
        self.flush();
        result
    }

    fn handle_update(&mut self, u: DeviceStateUpdate) {
        match u {
            DeviceStateUpdate::ButtonDown(i) => {
                self.clear_key_flash(i);
                self.set_key_pressed(i, true);
                let page = self.nav.current();
                let on = key_state(&self.cfg, &self.rt, page, i, false).state == Toggle::On;
                if waits_for_release(self.cfg.key(page, i), on) {
                    // Back waits for the release (a hold goes Home instead); show the press now.
                    self.paint_key(i, true);
                    self.hub.publish(Event::KeyDown { index: i, page: self.nav.current().map(str::to_string) });
                    if let Some(d) = &mut self.deck {
                        d.back_held = Some(BackHold { key: i, since: Instant::now() });
                    }
                } else {
                    let _ = self.key_down(i, true);
                }
            }
            DeviceStateUpdate::ButtonUp(i) => {
                self.clear_key_flash(i);
                self.set_key_pressed(i, false);
                self.paint_key(i, false);
                self.hub.publish(Event::KeyUp { index: i });
                let held = self.deck.as_mut().and_then(|d| d.back_held.take_if(|h| h.key == i));
                if held.is_some() {
                    let _ = self.press_key(i); // a tap: Back, through the normal press path
                }
            }
            DeviceStateUpdate::EncoderTwist(i, delta) => {
                let _ = self.turn_dial(i, delta, true);
            }
            DeviceStateUpdate::EncoderDown(i) => {
                log::info!("dial {i} pressed");
                self.hub.publish(Event::DialDown { index: i });
                self.set_dial_pressed(i, true);
                let act = self.cfg.dial(i).map(|d| d.press.clone()).unwrap_or_default();
                if !act.is_none() {
                    self.run_input(Source::Press(i), &act, 1);
                }
            }
            DeviceStateUpdate::EncoderUp(i) => {
                log::info!("dial {i} released");
                self.hub.publish(Event::DialUp { index: i });
                self.set_dial_pressed(i, false);
            }
            DeviceStateUpdate::TouchScreenPress(x, y) => {
                log::info!("strip tap at ({x}, {y})");
                self.hub.publish(Event::StripTap { x, y });
                // A tap on a panel does the same as pressing its dial, as in the Elgato
                // app: run the dial's press and flash the title.
                let Some(d) = &mut self.deck else { return };
                let Some((w, _)) = d.info.strip else { return };
                let Some(i) = tap_to_dial(x as u32, w, d.info.dials as usize) else { return };
                log::info!("strip tap on dial {i}");
                let _ = self.tap_dial(i as u8);
            }
            DeviceStateUpdate::TouchScreenLongPress(x, y) => {
                // Published for now; PR 16's long press uses it.
                log::info!("strip long press at ({x}, {y})");
                self.hub.publish(Event::StripLongPress { x, y });
            }
            DeviceStateUpdate::TouchScreenSwipe(from, to) => {
                log::info!("strip swipe {from:?} to {to:?}");
                self.hub.publish(Event::StripSwipe { from, to });
                // As in the Elgato app: a horizontal flick pages through, left is next,
                // right is previous. Short or mostly vertical swipes don't.
                let (dx, dy) = (to.0 as i32 - from.0 as i32, to.1 as i32 - from.1 as i32);
                if dx.abs() >= FLICK_MIN_PX && dx.abs() > dy.abs() {
                    let act = Action::PageStep { change: if dx < 0 { 1 } else { -1 } };
                    self.run_input(Source::Flick, &act, 1);
                }
            }
            // Neo touch points; nothing reacts to them yet.
            _ => {}
        }
    }

    /// A key pressed on the deck (and ctl's `Press`): paint it pressed, publish `KeyDown`
    /// only when `publish` (the hardware arm passes true, ctl false, so an open
    /// configurator's selection doesn't move), then `press_key(i)`.
    fn key_down(&mut self, i: u8, publish: bool) -> Result<()> {
        self.paint_key(i, true);
        if publish {
            self.hub.publish(Event::KeyDown { index: i, page: self.nav.current().map(str::to_string) });
        }
        self.press_key(i)
    }

    /// A dial turned (and ctl's `Dial` left/right): publish `DialTurn` only when `publish`
    /// (hardware true, ctl false), run its turn right (`delta >= 0`) or turn left with
    /// `|delta|` steps, at most 8. An empty action does nothing and is Ok.
    fn turn_dial(&mut self, i: u8, delta: i8, publish: bool) -> Result<()> {
        log::debug!("dial {i} turned {delta}");
        if publish {
            self.hub.publish(Event::DialTurn { index: i, delta });
        }
        let Some(dial) = self.cfg.dial(i) else { return Ok(()) };
        let act = if delta >= 0 { dial.turn_right.clone() } else { dial.turn_left.clone() };
        if act.is_none() {
            return Ok(());
        }
        self.try_input(Source::Turn(i), &act, delta.unsigned_abs().min(8))
    }

    /// A tap on panel `i` (a strip tap, and ctl's `Dial { dir: Press }`): flash the panel
    /// and run the dial's press. Publishes nothing: the strip arm publishes `StripTap`.
    fn tap_dial(&mut self, i: u8) -> Result<()> {
        if let Some(d) = &mut self.deck
            && let Some(f) = d.flash_until.get_mut(i as usize)
        {
            *f = Some(Instant::now());
            d.mark_dirty(i);
        }
        let act = self.cfg.dial(i).map(|d| d.press.clone()).unwrap_or_default();
        if act.is_none() {
            return Ok(());
        }
        self.try_input(Source::Tap(i), &act, 1)
    }

    /// A hardware press or release ends a ctl press's flash on that key.
    fn clear_key_flash(&mut self, i: u8) {
        if let Some(f) = self.deck.as_mut().and_then(|d| d.key_flash.get_mut(i as usize)) {
            *f = None;
        }
    }

    /// Record key `i` as held down or released. Doesn't mark it dirty: the button arms
    /// paint it themselves.
    fn set_key_pressed(&mut self, i: u8, pressed: bool) {
        if let Some(p) = self.deck.as_mut().and_then(|d| d.key_pressed.get_mut(i as usize)) {
            *p = pressed;
        }
    }

    fn set_dial_pressed(&mut self, i: u8, pressed: bool) {
        if let Some(d) = &mut self.deck
            && let Some(p) = d.dial_pressed.get_mut(i as usize)
        {
            *p = pressed;
            d.mark_dirty(i);
        }
    }

    // --- actions ---------------------------------------------------------------------------

    /// A key was pressed: run its action (the on action while on), then show the press
    /// as the new state at once and let the real state confirm it. The error is the
    /// action's (already published against the key).
    fn press_key(&mut self, i: u8) -> Result<()> {
        let page = self.nav.current().map(str::to_string);
        // Clone: everything below takes &mut self, and cfg.key() borrows self.cfg.
        let Some(k) = self.cfg.key(page.as_deref(), i).cloned() else { return Ok(()) };
        let id = KeyId { page: page.clone(), index: i };
        if self.rt.jobs.contains_key(&id) {
            log::info!("key {i}: still running, press ignored");
            return Ok(());
        }
        let before = key_state(&self.cfg, &self.rt, page.as_deref(), i, false).state;
        let act = k.action_for(before == Toggle::On).clone();
        // Like a running job: an ignored press never flips a toggle.
        if matches!(act, Action::Multi { .. }) && self.seq.is_running(&RunOf::Key(id.clone())) {
            log::info!("key {i}: multi action still running, press ignored");
            return Ok(());
        }
        if k.busy_while_running && act.is_trackable() {
            match action::run_tracked(&act) {
                Ok(job) => {
                    self.rt.jobs.insert(id.clone(), Running { job, text: String::new() });
                    self.mark_key(page.as_deref(), i);
                }
                Err(e) => {
                    let message = format!("{e:#}");
                    self.hub.publish(Event::ActionFailed { index: i, page, message });
                    return Err(e);
                }
            }
            return Ok(()); // the toggle's state catches up when the job ends (`finish_job`)
        }
        self.try_input(Source::Key(i), &act, 1)?;
        // A live command key shows what the press changed (e.g. `playerctl status`) at once.
        if let Some(p @ Probe::Output { .. }) = k.live.as_ref().map(Live::probe) {
            let _ = self.watcher.cmd.send(WatchCmd::Refresh(p));
        }
        if k.on.is_some() {
            self.after_toggle_press(&id, &k, before);
        }
        Ok(())
    }

    /// Every pass, with or without a deck: read the tracked jobs' output and reap the
    /// finished ones.
    pub fn poll_jobs(&mut self) {
        let mut changed = Vec::new();
        let mut done = Vec::new();
        for (id, r) in self.rt.jobs.iter_mut() {
            if let Some(line) = r.job.lines().last()
                && line != r.text.as_str()
            {
                r.text = line;
                changed.push(id.clone());
            }
            if let Some(status) = r.job.try_finish() {
                done.push((id.clone(), status));
            }
        }
        for id in changed {
            self.mark_key(id.page.as_deref(), id.index);
        }
        for (id, status) in done {
            let end = finish_job(&mut self.rt, &self.cfg, &id, status);
            if let Some(message) = end.failed {
                self.hub.publish(Event::ActionFailed { index: id.index, page: id.page.clone(), message });
            }
            if let Some(p) = end.refresh {
                let _ = self.watcher.cmd.send(WatchCmd::Refresh(p));
            }
            self.mark_key(id.page.as_deref(), id.index);
        }
    }

    /// Show the press at once, as Elgato does, then let the real state confirm it.
    fn after_toggle_press(&mut self, id: &KeyId, k: &KeyConfig, before: Toggle) {
        let now = Instant::now();
        match k.state_probe() {
            None => {
                // A press toggle flips on each press.
                if !self.rt.flipped.remove(id) {
                    self.rt.flipped.insert(id.clone());
                }
                self.mark_key(id.page.as_deref(), id.index);
            }
            Some(Probe::Volume(_)) => {} // the watcher runs the change and re-reads (D5)
            Some(p) if matches!(before, Toggle::Busy(_)) => {
                // No guess while busy: just ask for the real state now.
                let _ = self.watcher.cmd.send(WatchCmd::Refresh(p));
            }
            Some(p) if k.is_slow_toggle() => {
                // A slow toggle shows busy at once and polls fast until the state moves.
                self.rt.values.probes.insert(p.clone(), Value::Toggle(Toggle::Busy(String::new())));
                self.rt.settle.insert(p.clone(), Settle::Leaving { from: before.clone(), until: now + FAST_WINDOW });
                let _ = self.watcher.cmd.send(WatchCmd::Leave { probe: p.clone(), from: before });
                self.mark_probe(&p);
            }
            Some(p) => {
                // A fast toggle flips at once; the 300 ms read-back confirms or corrects.
                let guess = Value::Toggle(if before == Toggle::On { Toggle::Off } else { Toggle::On });
                self.rt.values.probes.insert(p.clone(), guess.clone());
                self.rt.settle.insert(p.clone(), Settle::Expect { value: guess, until: now + SETTLE });
                let _ = self.watcher.cmd.send(WatchCmd::Refresh(p.clone()));
                self.mark_probe(&p);
            }
        }
    }

    /// Run `act` for an input. The one entry point for keys, dials, strip taps and
    /// flicks (and later ctl, multi-action steps and long presses): it logs, dispatches,
    /// moves the page if the action did, and reports a failure against its source.
    /// Returns whether the action ran (a toggle flips only then).
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool {
        self.try_input(src, act, steps).is_ok()
    }

    /// What `run_input` does, with the error: log, dispatch, show the page if it moved,
    /// and on failure report it against `src` (nothing is published for `Ctl`: its reply
    /// carries it).
    pub fn try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()> {
        match act {
            // Only meaningful inside a multi, where the sequencer times it.
            Action::Delay { .. } => return Ok(()),
            Action::Multi { steps } => return self.start_multi(src, steps),
            _ => {}
        }
        if steps > 1 {
            log::info!("{src} x{steps}: {}", act.summary());
        } else {
            log::info!("{src}: {}", act.summary());
        }
        match self.dispatch(src, act, steps) {
            Ok(moved) => {
                if moved {
                    self.show_page();
                }
                Ok(())
            }
            Err(e) => {
                let page = self.nav.current().map(str::to_string);
                self.report_failure(src, page, format!("{e:#}"));
                Err(e)
            }
        }
    }

    /// Tell the configurators an input's action failed, against the key (on `page`) or
    /// dial it came from.
    fn report_failure(&self, src: Source, page: Option<String>, message: String) {
        log::warn!("{src}: {message}");
        match src {
            Source::Key(index) => self.hub.publish(Event::ActionFailed { index, page, message }),
            Source::Turn(index) | Source::Press(index) | Source::Tap(index) => {
                self.hub.publish(Event::DialActionFailed { index, message })
            }
            // Only logged: the reply carries it (or, for a Hyprland dispatch that failed
            // later on the worker, the reply has already gone).
            Source::Flick | Source::Ctl => {}
            // A step's failure is reported against its run by `run_due_steps`; a Hyprland
            // step that fails later on the worker is only logged.
            Source::Step => {}
        }
    }

    /// The run a multi started by `src` belongs to: the key (on the shown page), the dial
    /// (a press and a tap are one), or ctl. `None` for a flick and for a step: a step
    /// can't start a multi.
    fn run_of(&self, src: Source) -> Option<RunOf> {
        match src {
            Source::Key(index) => Some(RunOf::Key(KeyId { page: self.nav.current().map(str::to_string), index })),
            Source::Turn(i) => Some(RunOf::Turn(i)),
            Source::Press(i) | Source::Tap(i) => Some(RunOf::Press(i)),
            Source::Ctl => Some(RunOf::Ctl),
            Source::Flick | Source::Step => None,
        }
    }

    /// Start a multi for `src` and run its leading steps now (so a page step shows at
    /// once). Ignored, and Ok, while that input's last run is still going. A turn runs
    /// it once per turn event, whatever its steps.
    fn start_multi(&mut self, src: Source, steps: &[Action]) -> Result<()> {
        let Some(of) = self.run_of(src) else { anyhow::bail!("multi-actions can't be nested") };
        log::info!("{src}: {}", Action::Multi { steps: steps.to_vec() }.summary());
        let now = Instant::now();
        if !self.seq.start(of.clone(), steps.to_vec(), now) {
            log::info!("{of}: multi action still running; ignored");
            return Ok(());
        }
        self.run_due_steps(now);
        Ok(())
    }

    /// Run the steps whose time has come. `main.rs`'s `run()` calls it on every
    /// iteration, right after `handle_commands`, deck or not. A failed step is reported
    /// against the key or dial whose multi it is, with its number; the rest still run.
    pub fn run_due_steps(&mut self, now: Instant) {
        for (of, n, step) in self.seq.due(now) {
            log::info!("{of} step {}: {}", n + 1, step.summary());
            if let Err(e) = self.try_input(Source::Step, &step, 1) {
                let message = format!("step {}: {e:#}", n + 1);
                log::warn!("{of}: {message}");
                match of {
                    RunOf::Key(KeyId { page, index }) => self.hub.publish(Event::ActionFailed { index, page, message }),
                    RunOf::Turn(index) | RunOf::Press(index) => {
                        self.hub.publish(Event::DialActionFailed { index, message })
                    }
                    RunOf::Ctl => {}
                }
            }
        }
    }

    /// Every pass: report the Hyprland dispatches that failed on the worker.
    pub fn poll_hypr(&mut self) {
        while let Ok(f) = self.hypr.failed.try_recv() {
            self.report_failure(f.src, f.page, f.message);
        }
    }

    /// Run an action: page moves, brightness and volume are handled here, everything else
    /// goes to `action::run_steps`. Returns whether the page moved.
    fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool> {
        match act {
            Action::Page { page } if self.cfg.has_page(page) => Ok(self.nav.go(Some(page))),
            Action::Page { page } => Err(anyhow::anyhow!("there's no page named “{page}”")),
            Action::Back => Ok(self.nav.back()),
            Action::PageStep { change } => {
                // From inside a folder (however deep), step from the page the folders hang off.
                let origin = self.nav.nearest(|p| !self.cfg.is_folder(p)).map(str::to_string);
                let by = i32::from(*change) * steps as i32;
                Ok(match self.cfg.step_page(origin.as_deref(), by) {
                    Some(page) => self.nav.switch(page.as_deref()),
                    None => false,
                })
            }
            Action::Brightness { change, set } => {
                // A relative turn changes the brightness per detent; `set` applies once.
                let change = if set.is_none() { i32::from(*change) * steps as i32 } else { 0 };
                self.set_brightness(*set, change);
                Ok(false)
            }
            Action::Volume { device, change, mute } => {
                let change = if *mute { 0 } else { i32::from(action::folded_volume(*change, steps)) };
                self.guess_volume(*device, change, *mute);
                // Runs on the watcher, after any earlier change, and is read back there.
                let _ = self.watcher.cmd.send(WatchCmd::Volume { device: *device, change, mute: *mute });
                Ok(false)
            }
            Action::AppVolume { change, mute, .. } => {
                let Some(query) = act.app_query() else { anyhow::bail!("no app set") };
                let change = if *mute { 0 } else { i32::from(action::folded_volume(*change, steps)) };
                // Runs on the watcher, after any earlier change, and is read back there.
                let _ = self.watcher.cmd.send(WatchCmd::AppVolume { query, change, mute: *mute });
                Ok(false)
            }
            Action::Hyprland { dispatch } => {
                // Queued on the worker, in order behind earlier ones: `hyprctl` waits for
                // Hyprland, which must never stall the deck. Only an empty dispatcher fails
                // here; anything else comes back through `poll_hypr`.
                if dispatch.trim().is_empty() {
                    anyhow::bail!("no dispatcher set");
                }
                let page = self.nav.current().map(str::to_string);
                self.hypr.send(HyprJob { src, page, dispatch: dispatch.clone(), steps });
                Ok(false)
            }
            a => action::run_steps(a, steps).map(|_| false),
        }
    }

    /// Move the volume bar at once, from the last known level plus the change, so it
    /// doesn't wait for the read-back (the brightness bar is instant for the same reason).
    /// The watcher's read right after the change corrects it. No guess before the first
    /// read: that read is moments away.
    fn guess_volume(&mut self, device: AudioDevice, change: i32, mute: bool) {
        // The change goes to the watcher right after: it's in flight until the watcher
        // acknowledges it (always, read or not). Not clamped: the acknowledgement isn't.
        if !mute && change != 0 {
            *self.in_flight.entry(device).or_insert(0) += change;
        }
        let Some(cur) = self.rt.values.level(device) else { return };
        let next = probe::guess_volume(cur, change, mute);
        self.store_value(Probe::Volume(device), Value::Level(next));
    }

    /// Set or change the panel brightness: the deck (if any), the state file, the
    /// brightness panels and the configurators. With no deck it applies on connect.
    pub fn set_brightness(&mut self, set: Option<u8>, change: i32) {
        let next = apply_brightness(self.rt.values.brightness, set, change);
        if next == self.rt.values.brightness {
            return;
        }
        log::info!("brightness {next}%");
        self.rt.values.brightness = next;
        self.rt.state.brightness = Some(next);
        self.saver.send(self.rt.state.clone());
        self.hub.publish(Event::Brightness { value: next });
        if let Some(d) = &mut self.deck {
            if let Err(e) = d.dev.set_brightness(next) {
                log::warn!("set_brightness: {e}");
            }
            d.dirty_where(&self.cfg, |dial| {
                matches!(dial.and_then(|dc| dc.level_source()), Some(LevelSource::Brightness))
            });
        }
    }

    // --- painting ----------------------------------------------------------------------------

    /// Repaint the deck after `nav` moved to another page, watch the new page's keys,
    /// and tell the configurators.
    fn show_page(&mut self) {
        // The key under your finger on the new page is drawn released; its release only
        // repaints it.
        if let Some(d) = &mut self.deck {
            d.key_pressed.fill(false);
            d.back_held = None;
        }
        log::info!("page: {}", self.nav.current().unwrap_or(omadeck_core::HOME));
        self.paint_all();
        self.watch();
        self.hub.publish(Event::PageChanged { page: self.nav.current().map(str::to_string) });
    }

    fn paint_all(&mut self) {
        let n = self.key_count();
        let keys = self.key_states(n);
        if let Some(d) = &mut self.deck
            && let Err(e) =
                deck::paint_all(d, &self.cfg, self.nav.current(), &mut self.renderer, &self.rt.values, &keys)
        {
            log::warn!("repaint failed: {e}");
        }
    }

    fn paint_key(&mut self, i: u8, pressed: bool) {
        if let Some(d) = &self.deck {
            let st = key_state(&self.cfg, &self.rt, self.nav.current(), i, pressed);
            deck::paint_key(d, &self.cfg, self.nav.current(), &mut self.renderer, i, &st);
        }
    }

    /// End of a pass: publish the key states that changed, paint the dirty panels and
    /// keys once, and tell the configurators about any panel whose value changed.
    pub fn flush(&mut self) {
        self.publish_keys();
        let n = self.key_count();
        let keys = self.key_states(n);
        let Some(d) = &mut self.deck else { return };
        deck::flush_dirty(d, &self.cfg, self.nav.current(), &mut self.renderer, &self.rt.values, &keys);
        for i in 0..d.info.dials {
            let status = probe::dial_status(self.cfg.dial(i), &self.rt.values, false).panel;
            if self.published.get(&i) != Some(&status) {
                self.published.insert(i, status.clone());
                self.hub.publish(Event::DialStatus { index: i, status });
            }
        }
    }

    /// Hand the deck back to its logo, close the socket, write any pending state.
    pub fn shutdown(self) {
        log::info!("shutting down");
        if !self.seq.is_empty() {
            log::info!("dropping {} running multi actions", self.seq.len());
        }
        if let Some(d) = self.deck {
            let _ = d.dev.reset();
        }
        self.hub.shutdown();
        self.watcher.stop();
        self.saver.finish();
    }
}

/// The reply line for a `Run` that went through: where the deck is now, or what ran.
fn run_message(act: &Action, page: Option<&str>, brightness: u8) -> String {
    match act {
        Action::Page { .. } | Action::Back | Action::PageStep { .. } => {
            format!("page {}", page.unwrap_or(omadeck_core::HOME))
        }
        Action::Brightness { .. } => format!("brightness {brightness}%"),
        a => a.summary(),
    }
}

/// What a volume panel shows for a read: the read plus what is still in flight (`f`,
/// already net of what this read confirmed), so the bar never drops to a value the
/// queued changes haven't reached yet. The mute is the read's: a level change doesn't
/// unmute everywhere (`wpctl set-volume` doesn't).
fn settle_volume(f: i32, read: &Level) -> Level {
    if f == 0 {
        return read.clone();
    }
    let guess = probe::guess_volume(read, f, false);
    if read.muted { Level { value: guess.value, ..read.clone() } } else { guess }
}

/// The flip state a reload keeps: only the keys that are still press toggles.
fn retain_flips(cfg: &Config, flipped: &mut HashSet<KeyId>) {
    flipped.retain(|id| cfg.key(id.page.as_deref(), id.index).is_some_and(KeyConfig::is_press_toggle));
}

/// Every probe the config can show: the dials' and every key's, on any page.
fn config_probes(cfg: &Config, n_dials: usize) -> Vec<Probe> {
    let mut out = Vec::new();
    let mut add = |p: Probe| {
        if !out.contains(&p) {
            out.push(p);
        }
    };
    for i in 0..n_dials {
        let dial = cfg.dial(i as u8);
        match dial.and_then(DialConfig::level_source) {
            Some(LevelSource::Volume(dev)) => add(Probe::Volume(dev)),
            Some(LevelSource::App) => {
                if let Some(q) = dial.and_then(DialConfig::app_query) {
                    add(Probe::AppVolume(q));
                }
            }
            Some(LevelSource::Brightness | LevelSource::Player) | None => {}
        }
        if let Some(l) = dial.and_then(|d| d.live.as_ref()) {
            add(l.probe());
        }
    }
    let mut pages: Vec<Option<&str>> = vec![None];
    pages.extend(cfg.pages.iter().map(|p| Some(p.name.as_str())));
    for page in pages {
        if let Some(ks) = cfg.page(page) {
            for k in ks.iter() {
                if let Some(p) = k.state_probe() {
                    add(p);
                }
                if let Some(l) = &k.live {
                    add(l.probe());
                }
            }
        }
    }
    out
}

/// How long to wait for input: until `deadline`, at most `READ_TIMEOUT`. Rounded up to
/// whole milliseconds, which is what hidapi takes, so a deadline 0.4 ms away doesn't
/// become a busy loop of zero-length reads.
pub fn wait_for(now: Instant, deadline: Option<Instant>) -> Duration {
    let d = deadline.map_or(READ_TIMEOUT, |t| t.saturating_duration_since(now).min(READ_TIMEOUT));
    Duration::from_millis(d.as_micros().div_ceil(1000) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use omadeck_core::{
        probe::{FAST_WINDOW, SETTLE},
        proc,
    };

    #[test]
    fn only_a_back_press_waits_for_release() {
        let cfg = Config::parse(
            "[[key]]\nindex = 0\n[key.action]\ntype = \"back\"\n\n\
             [[key]]\nindex = 1\n[key.action]\ntype = \"page\"\npage = \"A\"\n\n\
             [[key]]\nindex = 2\n[key.action]\ntype = \"app\"\ncommand = \"true\"\n[key.on]\n[key.on.action]\ntype = \"back\"\n",
        )
        .unwrap();
        assert!(waits_for_release(cfg.key(None, 0), false), "a Back key");
        assert!(!waits_for_release(cfg.key(None, 1), false), "Open page acts on key-down");
        assert!(!waits_for_release(cfg.key(None, 2), false), "a toggle runs its off action while off");
        assert!(waits_for_release(cfg.key(None, 2), true), "and its Back on action while on");
        assert!(!waits_for_release(None, false), "no key");
    }

    #[test]
    fn config_probes_keeps_app_dials() {
        let cfg = Config::parse(
            r#"
[[dial]]
index = 0
[dial.turn_right]
type = "app_volume"
app = "spotify"
change = 5

[[dial]]
index = 1
[dial.turn_right]
type = "app_volume"
focused = true
change = 5
"#,
        )
        .unwrap();
        use omadeck_core::audio::AppQuery;
        let all = config_probes(&cfg, 4);
        assert!(all.contains(&Probe::AppVolume(AppQuery::Named("spotify".into()))));
        assert!(all.contains(&Probe::AppVolume(AppQuery::Focused)));
        // Only the dials the deck has.
        assert_eq!(config_probes(&cfg, 1), vec![Probe::AppVolume(AppQuery::Named("spotify".into()))]);
    }

    #[test]
    fn app_volume_without_app_is_an_error() {
        // What `dispatch` bails on: no app and not following focus.
        let a = Action::AppVolume { app: "  ".into(), focused: false, change: 5, mute: false };
        assert_eq!(a.app_query(), None);
        let a = Action::AppVolume { app: String::new(), focused: true, change: 5, mute: false };
        assert!(a.app_query().is_some());
    }

    #[test]
    fn settle_expect_ignores_contradiction_until_expiry() {
        let now = Instant::now();
        let guess = Value::Toggle(Toggle::On);
        let s = Settle::Expect { value: guess.clone(), until: now + SETTLE };
        // A contradicting read is refused while the window is open…
        assert!(!s.accepts(&Value::Toggle(Toggle::Off), now + Duration::from_millis(100)));
        // …but the guess itself is accepted, and so is anything after the window.
        assert!(s.accepts(&guess, now + Duration::from_millis(100)));
        assert!(s.accepts(&Value::Toggle(Toggle::Off), now + SETTLE));
    }

    #[test]
    fn settle_leaving_takes_busy_and_new_state() {
        let now = Instant::now();
        let s = Settle::Leaving { from: Toggle::On, until: now + FAST_WINDOW };
        // Still on: refused…
        assert!(!s.accepts(&Value::Toggle(Toggle::On), now + Duration::from_millis(100)));
        // …but busy and a new state are accepted.
        assert!(s.accepts(&Value::Toggle(Toggle::Busy("starting".into())), now + Duration::from_millis(100)));
        assert!(s.accepts(&Value::Toggle(Toggle::Off), now + Duration::from_millis(100)));
        // And everything is accepted after the window.
        assert!(s.accepts(&Value::Toggle(Toggle::On), now + FAST_WINDOW));
    }

    #[test]
    fn reload_drops_flips_of_keys_that_are_no_longer_press_toggles() {
        let cfg =
            Config::parse("[[key]]\nindex = 0\n[key.on]\n\n[[key]]\nindex = 1\n[key.on]\nstate = \"true\"\n").unwrap();
        let mut flipped = HashSet::from([
            KeyId { page: None, index: 0 }, // still a press toggle: kept
            KeyId { page: None, index: 1 }, // now a command toggle: dropped
            KeyId { page: None, index: 2 }, // no such key: dropped
        ]);
        retain_flips(&cfg, &mut flipped);
        assert_eq!(flipped, HashSet::from([KeyId { page: None, index: 0 }]));
    }

    #[test]
    fn job_on_toggle_key_settles_after_exit() {
        let cfg = Config::parse(
            "[[key]]\nindex = 0\n[key.action]\ntype = \"app\"\ncommand = \"sleep 1\"\n[key.on]\nstate = \"true\"\nbusy_while_running = true\n",
        )
        .unwrap();
        let mut rt = Runtime::default();
        let id = KeyId { page: None, index: 0 };
        let mut job = proc::Job::spawn(std::process::Command::new("true")).unwrap();
        // Wait for the job to finish so we have a real ExitStatus.
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        rt.jobs.insert(id.clone(), Running { job, text: "".into() });
        // Exit 0: no failure, and the command probe is refreshed.
        let end = finish_job(&mut rt, &cfg, &id, status);
        assert!(end.failed.is_none());
        assert!(matches!(end.refresh, Some(Probe::Command { .. })));
        assert!(rt.jobs.is_empty());
    }

    #[test]
    fn job_failure_reports_and_does_not_flip() {
        let cfg =
            Config::parse("[[key]]\nindex = 0\n[key.action]\ntype = \"app\"\ncommand = \"false\"\n[key.on]\n").unwrap();
        let mut rt = Runtime::default();
        let id = KeyId { page: None, index: 0 };
        let mut job = proc::Job::spawn(std::process::Command::new("false")).unwrap();
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        rt.jobs.insert(id.clone(), Running { job, text: "".into() });
        let end = finish_job(&mut rt, &cfg, &id, status);
        assert!(end.failed.is_some(), "a failing command reports");
        assert!(end.refresh.is_none(), "no state probe on this key");
        assert!(!rt.flipped.contains(&id), "a failed press toggle does not flip");
    }

    #[test]
    fn key_state_precedence() {
        let cfg =
            Config::parse("[[key]]\nindex = 0\n[key.on]\nstate = \"true\"\n\n[[key]]\nindex = 1\n[key.on]\n").unwrap();
        let mut rt = Runtime::default();
        let probe = cfg.key(None, 0).unwrap().state_probe().unwrap();
        assert_eq!(key_state(&cfg, &rt, None, 0, false).state, Toggle::Off, "nothing read yet");
        rt.values.probes.insert(probe, Value::Toggle(Toggle::On));
        assert_eq!(key_state(&cfg, &rt, None, 0, false).state, Toggle::On);
        let mut sleep = std::process::Command::new("sleep");
        sleep.arg("5");
        let id = KeyId { page: None, index: 0 };
        rt.jobs.insert(id.clone(), Running { job: proc::Job::spawn(sleep).unwrap(), text: "Copying 3%".into() });
        assert_eq!(key_state(&cfg, &rt, None, 0, false).state, Toggle::Busy("Copying 3%".into()), "a running job wins");
        rt.jobs.get_mut(&id).unwrap().job.kill_group();
        assert_eq!(key_state(&cfg, &rt, None, 1, false).state, Toggle::Off);
        rt.flipped.insert(KeyId { page: None, index: 1 });
        assert_eq!(key_state(&cfg, &rt, None, 1, false).state, Toggle::On, "a press toggle reads `flipped`");
    }

    #[test]
    fn a_flipped_press_toggle_paints_its_on_face() {
        // What `paint_key` draws: `key_state` through the renderer, with the bundled font
        // (core's `test-support`), so the pixels don't depend on this machine.
        let cfg = Config::parse("[[key]]\nindex = 0\nlabel = \"A\"\n[key.on]\nlabel = \"B\"\n").unwrap();
        let mut r = Renderer::with_font_bytes(&cfg.style, omadeck_core::render::TEST_FONT);
        let mut rt = Runtime::default();
        let paint = |rt: &Runtime, r: &mut Renderer| {
            r.render(cfg.key(None, 0), &cfg.style, 72, &key_state(&cfg, rt, None, 0, false))
        };
        let off = paint(&rt, &mut r);
        rt.flipped.insert(KeyId { page: None, index: 0 });
        assert_ne!(paint(&rt, &mut r), off);
    }

    #[test]
    fn wait_for_caps_and_rounds_up() {
        let now = Instant::now();
        assert_eq!(wait_for(now, None), READ_TIMEOUT);
        assert_eq!(wait_for(now, Some(now + Duration::from_secs(5))), READ_TIMEOUT);
        assert_eq!(wait_for(now, Some(now + Duration::from_millis(40))), Duration::from_millis(40));
        assert_eq!(wait_for(now, Some(now + Duration::from_micros(400))), Duration::from_millis(1));
        // Overdue: don't wait at all.
        assert_eq!(wait_for(now + Duration::from_secs(1), Some(now)), Duration::ZERO);
    }

    #[test]
    fn start_brightness_follows_a_changed_config_value_only() {
        // Dimmed to 20 with the dial; the configurator later rewrote config.toml with
        // the same 70: the deck comes back at 20, and the state now records 70.
        let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, Some(70));
        assert_eq!(rt.values.brightness, 20);
        // The value was edited to 35 while the daemon was stopped: 35, saved as seen.
        let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, Some(35));
        assert_eq!((rt.values.brightness, rt.state.config_brightness), (35, Some(35)));
        assert_eq!(rt.state.brightness, Some(35));
        // config.toml didn't parse: its fallback default is not an edit.
        let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, None);
        assert_eq!((rt.values.brightness, rt.state.config_brightness), (20, Some(70)));
    }

    #[test]
    fn runtime_outlives_the_deck() {
        // What a replug keeps lives in Runtime; Deck has only the device and its scratch.
        let mut rt = Runtime::default();
        rt.values.brightness = 40;
        rt.values.probes.insert(
            Probe::Volume(AudioDevice::Output),
            Value::Level(omadeck_core::Level { value: 0.5, text: "50%".into(), muted: false }),
        );
        assert_eq!(rt.values.level(AudioDevice::Output).map(|l| l.text.as_str()), Some("50%"));
        assert_eq!(rt.state, State::default());
    }

    #[test]
    fn source_names_its_key_or_dial() {
        assert_eq!(Source::Key(3).to_string(), "key 3");
        assert_eq!(Source::Turn(1).to_string(), "dial 1");
        assert_eq!(Source::Flick.to_string(), "strip flick");
        assert_eq!(Source::Ctl.to_string(), "omadeckd");
    }

    #[test]
    fn step_and_ctl_sources_name_themselves() {
        assert_eq!(Source::Step.to_string(), "step");
        assert_eq!(Source::Ctl.to_string(), "omadeckd");
        assert_eq!(Source::Tap(2).to_string(), "dial 2");
    }

    #[test]
    fn run_messages() {
        assert_eq!(run_message(&Action::PageStep { change: 1 }, Some("Media"), 70), "page Media");
        assert_eq!(run_message(&Action::Back, None, 70), "page Home");
        assert_eq!(run_message(&Action::Brightness { change: 10, set: None }, None, 40), "brightness 40%");
        assert_eq!(run_message(&Action::Url { url: "omarchy.org".into() }, None, 40), "Open omarchy.org");
    }

    fn level(percent: i32) -> Level {
        Level { value: percent as f32 / 100.0, text: format!("{percent}%"), muted: false }
    }

    #[test]
    fn fast_spin_does_not_snap_back() {
        // Two turns of +5 are in flight; the first read-back confirms only 5 (net 5
        // left): the bar stays at 60 (55 read + 5), not the stale read.
        assert_eq!(settle_volume(5, &level(55)).text, "60%");
        // Everything confirmed: the read as it is.
        assert_eq!(settle_volume(0, &level(60)).text, "60%");
        // A muted device stays muted while changes are in flight.
        let muted = Level { muted: true, text: "Muted".into(), ..level(40) };
        let shown = settle_volume(10, &muted);
        assert!(shown.muted);
        assert_eq!(shown.text, "Muted");
    }

    #[test]
    fn config_probes_include_live_on_every_page() {
        let cfg = Config::parse(
            r#"
[[key]]
index = 0
live = { source = "cpu" }

[[page]]
name = "Other"
[[page.key]]
index = 3
live = { source = "command", command = "date" }

[[dial]]
index = 1
live = { source = "battery" }
"#,
        )
        .unwrap();
        let all = config_probes(&cfg, 4);
        assert!(all.contains(&Probe::Cpu));
        assert!(all.contains(&Probe::Battery));
        assert!(all.contains(&Probe::Output { command: "date".into(), interval_ms: 5000 }));
        assert!(!config_probes(&cfg, 0).contains(&Probe::Battery), "only the dials the deck has");
    }
}
