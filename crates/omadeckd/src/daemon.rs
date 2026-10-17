//! The daemon's state and everything the deck loop does with it: inputs, actions, page
//! moves, brightness, values from the watcher, reloads and painting.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::time::{Duration, Instant};

use anyhow::Result;
use omadeck_core::config::apply_brightness;
use omadeck_core::device::{DeviceStateUpdate, StreamDeckError};
use omadeck_core::ipc::{Command, Event};
use omadeck_core::probe::{self, FAST_WINDOW, Probe, SETTLE, Toggle, Value, Values};
use omadeck_core::proc;
use omadeck_core::state::State;
use omadeck_core::{
    Action, AudioDevice, Config, DialConfig, KeyConfig, KeyStatus, Level, LevelSource, PanelStatus, Renderer, action,
    tap_to_dial,
};

use crate::deck::{self, Deck};
use crate::hub::Hub;
use crate::nav::Nav;
use crate::saver::StateSaver;
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
    // Task F polls the job; until then only the text is read (by `key_state`'s tests).
    #[allow(dead_code)]
    pub job: proc::Job,
    /// The latest output line (the busy face's text).
    pub text: String,
}

impl fmt::Debug for Running {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Running").field("text", &self.text).finish_non_exhaustive()
    }
}

/// An optimistic guess waiting for the real state to confirm it. Task F's presses build
/// them and `apply_value` consults them.
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
enum Settle {
    /// A fast toggle was pressed and shows `value`: ignore reads that contradict it until
    /// `until`.
    Expect { value: Value, until: Instant },
    /// A slow toggle was pressed while showing `from`: ignore reads still equal to it
    /// until `until`.
    Leaving { from: Toggle, until: Instant },
}

impl Settle {
    #[allow(dead_code)]
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
    /// daemon restart). Task F reads and writes them.
    #[allow(dead_code)]
    pub flipped: HashSet<KeyId>,
    /// Optimistic guesses waiting for the real state to confirm them.
    #[allow(dead_code)]
    settle: HashMap<Probe, Settle>,
    /// `busy_while_running` commands, per key. Task F fills it.
    #[allow(dead_code)]
    pub jobs: HashMap<KeyId, Running>,
}

/// What key `i` on `page` shows now: core's `key_status` (the ONE builder) with the two
/// per-key things `Values` can't hold laid over it. A running job wins; a press toggle
/// reads `flipped`. Task F's paint path calls it.
#[allow(dead_code)]
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
/// PR 11 adds `Ctl`, PR 12 `Step`, PR 16 `LongTap(u8)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Key(u8),
    Turn(u8),
    Press(u8),
    Tap(u8),
    Flick,
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Key(i) => write!(f, "key {i}"),
            Source::Turn(i) | Source::Press(i) | Source::Tap(i) => write!(f, "dial {i}"),
            Source::Flick => write!(f, "strip flick"),
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
    pub rt: Runtime,
    pub deck: Option<Deck>,
    /// What was last published per strip panel (`Event::DialStatus`), so only changes go out.
    published: HashMap<u8, PanelStatus>,
    /// What was last published per key (`Event::KeyState`), so only changes go out.
    /// Task F's `publish_keys` (in `flush`) reads and writes it.
    #[allow(dead_code)]
    published_keys: HashMap<KeyId, KeyStatus>,
    /// Volume changes sent to the watcher but not yet confirmed by its read-backs, per
    /// device. The bar shows confirmed + in-flight, so it never drops to a value the
    /// queued changes haven't reached yet (a fast spin would otherwise snap back as the
    /// read-backs catch up).
    in_flight: HashMap<AudioDevice, i32>,
    pub reload_at: Option<Instant>,
    pub repaint_at: Option<Instant>,
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
            rt,
            deck: None,
            published: HashMap::new(),
            published_keys: HashMap::new(),
            in_flight: HashMap::new(),
            reload_at: None,
            repaint_at: None,
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
        self.published.clear();
        self.repaint_at = Some(Instant::now() + POST_CONNECT_REPAINT);
        self.watch();
    }

    /// The deck went away. `rt` stays: the values and brightness are there on replug.
    pub fn detach(&mut self, e: &StreamDeckError) {
        if let Some(d) = self.deck.take() {
            log::warn!("lost {} ({e}); waiting for it to come back", d.info.model);
        }
        self.published.clear();
        self.repaint_at = None;
        // `in_flight` stays: the watcher still acknowledges every queued change.
        let _ = self.watcher.cmd.send(WatchCmd::Watch(Vec::new()));
        self.hub.publish(Event::Disconnected);
    }

    /// Tell the watcher what the deck's dials and the current page's keys show. Sent on
    /// connect, on every reload and on every page change.
    fn watch(&mut self) {
        let dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        let probes = probe::watched_probes(&self.cfg, self.nav.current(), dials);
        let _ = self.watcher.cmd.send(WatchCmd::Watch(probes));
    }

    // --- one pass of the loop --------------------------------------------------------------

    /// The config changed on disk.
    pub fn reload(&mut self) {
        // A typo while hand-editing shouldn't wipe the deck: keep the last good config.
        let mut hand_edit = None;
        match Config::load() {
            Ok(new_cfg) => {
                hand_edit = self.rt.state.config_edit(new_cfg.brightness);
                self.cfg = new_cfg;
                self.keep_toggle_state();
            }
            Err(e) => log::error!("{e:#}; keeping the previous config"),
        }
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
    }

    /// Run what the configurators asked for. Only the latest page request counts, so
    /// flicking through pages there doesn't echo a trail of stale page changes back.
    pub fn handle_commands(&mut self) {
        let mut show = None;
        let mut bright = None;
        for cmd in self.hub.commands() {
            match cmd {
                Command::ShowPage { page } => show = Some(page),
                Command::SetBrightness { value } => bright = Some(value),
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

    /// Take the watcher's new values.
    pub fn apply_values(&mut self) {
        while let Ok((p, v, applied)) = self.watcher.values.try_recv() {
            self.apply_value(p, v, applied);
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
            if self.cfg.key(page.as_deref(), i).and_then(KeyConfig::state_probe).as_ref() == Some(probe) {
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

    /// What each of the current page's first `n` keys shows now (not pressed), for
    /// painting.
    fn key_states(&self, n: u8) -> Vec<KeyStatus> {
        (0..n).map(|i| key_state(&self.cfg, &self.rt, self.nav.current(), i, false)).collect()
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
            if self.cfg.key(page.as_deref(), i).is_some_and(|k| k.on.is_some() || k.busy_while_running) {
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
    /// toggles, settles and values of probes the config still shows.
    fn keep_toggle_state(&mut self) {
        retain_flips(&self.cfg, &mut self.rt.flipped);
        let n_dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        let watched = probe::watched_probes(&self.cfg, self.nav.current(), n_dials);
        self.rt.settle.retain(|p, _| watched.contains(p));
        // Values: the dials' probes plus every key's, on any page, so a page you return
        // to shows the last known faces at once.
        let all = config_probes(&self.cfg, n_dials);
        self.rt.values.probes.retain(|p, _| all.contains(p));
    }

    /// The next moment something is due: a tap flash ending, the post-connect repaint, a
    /// debounced reload. PR 12's sequencer and PR 16's timers add theirs here.
    pub fn next_deadline(&self) -> Option<Instant> {
        let flash = self.deck.as_ref().and_then(Deck::flash_deadline);
        [flash, self.repaint_at, self.reload_at].into_iter().flatten().min()
    }

    /// One pass with a deck: read and handle its input until it's quiet, then paint what
    /// changed, once. An error means the deck is gone.
    pub fn pass(&mut self) -> Result<(), StreamDeckError> {
        let now = Instant::now();
        if self.repaint_at.is_some_and(|t| now >= t) {
            self.repaint_at = None;
            self.paint_all();
        }
        let Some(d) = &mut self.deck else { return Ok(()) };
        d.expire_flashes(now);
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
                self.paint_key(i, true);
                self.hub.publish(Event::KeyDown { index: i, page: self.nav.current().map(str::to_string) });
                self.press_key(i);
            }
            DeviceStateUpdate::ButtonUp(i) => {
                self.paint_key(i, false);
                self.hub.publish(Event::KeyUp { index: i });
            }
            DeviceStateUpdate::EncoderTwist(i, delta) => {
                log::debug!("dial {i} turned {delta}");
                self.hub.publish(Event::DialTurn { index: i, delta });
                let Some(dial) = self.cfg.dial(i) else { return };
                let act = if delta >= 0 { dial.turn_right.clone() } else { dial.turn_left.clone() };
                if !act.is_none() {
                    self.run_input(Source::Turn(i), &act, delta.unsigned_abs().min(8));
                }
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
                d.flash_until[i] = Some(Instant::now());
                d.mark_dirty(i as u8);
                let act = self.cfg.dial(i as u8).map(|d| d.press.clone()).unwrap_or_default();
                if !act.is_none() {
                    self.run_input(Source::Tap(i as u8), &act, 1);
                }
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
    /// as the new state at once and let the real state confirm it.
    fn press_key(&mut self, i: u8) {
        let page = self.nav.current().map(str::to_string);
        // Clone: everything below takes &mut self, and cfg.key() borrows self.cfg.
        let Some(k) = self.cfg.key(page.as_deref(), i).cloned() else { return };
        let before = key_state(&self.cfg, &self.rt, page.as_deref(), i, false).state;
        let act = k.action_for(before == Toggle::On).clone();
        if !self.run_input(Source::Key(i), &act, 1) || k.on.is_none() {
            return;
        }
        self.after_toggle_press(&KeyId { page, index: i }, &k, before);
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
        if steps > 1 {
            log::info!("{src} x{steps}: {}", act.summary());
        } else {
            log::info!("{src}: {}", act.summary());
        }
        match self.dispatch(act, steps) {
            Ok(moved) => {
                if moved {
                    self.show_page();
                }
                true
            }
            Err(e) => {
                log::warn!("{src}: {e:#}");
                let message = format!("{e:#}");
                match src {
                    Source::Key(index) => {
                        let page = self.nav.current().map(str::to_string);
                        self.hub.publish(Event::ActionFailed { index, page, message })
                    }
                    Source::Turn(index) | Source::Press(index) | Source::Tap(index) => {
                        self.hub.publish(Event::DialActionFailed { index, message })
                    }
                    Source::Flick => {}
                }
                false
            }
        }
    }

    /// Run an action: page moves, brightness and volume are handled here, everything else
    /// goes to `action::run_steps`. Returns whether the page moved.
    fn dispatch(&mut self, act: &Action, steps: u8) -> Result<bool> {
        match act {
            Action::Page { page } if self.cfg.has_page(page) => Ok(self.nav.go(Some(page))),
            Action::Page { page } => Err(anyhow::anyhow!("there's no page named “{page}”")),
            Action::Back => Ok(self.nav.back()),
            Action::PageStep { change } => {
                // From inside a folder, step from the folder's parent, leaving the folder.
                let in_folder =
                    self.cfg.pages.iter().any(|p| p.folder && self.nav.current().is_some_and(|c| p.name == c));
                let origin =
                    if in_folder { self.nav.parent().flatten() } else { self.nav.current().map(str::to_string) };
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
        if let Some(d) = self.deck {
            let _ = d.dev.reset();
        }
        self.hub.shutdown();
        self.watcher.stop();
        self.saver.finish();
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
    for i in 0..n_dials {
        if let Some(LevelSource::Volume(dev)) = cfg.dial(i as u8).and_then(DialConfig::level_source) {
            let p = Probe::Volume(dev);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    let mut pages: Vec<Option<&str>> = vec![None];
    pages.extend(cfg.pages.iter().map(|p| Some(p.name.as_str())));
    for page in pages {
        if let Some(ks) = cfg.page(page) {
            for k in ks.iter() {
                if let Some(p) = k.state_probe()
                    && !out.contains(&p)
                {
                    out.push(p);
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
    use omadeck_core::probe::{FAST_WINDOW, SETTLE};

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
}
