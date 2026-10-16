//! The daemon's state and everything the deck loop does with it: inputs, actions, page
//! moves, brightness, values from the watcher, reloads and painting.

use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};

use anyhow::Result;
use omadeck_core::config::apply_brightness;
use omadeck_core::device::{DeviceStateUpdate, StreamDeckError};
use omadeck_core::ipc::{Command, Event};
use omadeck_core::probe::{self, Probe, Value, Values};
use omadeck_core::state::State;
use omadeck_core::{Action, AudioDevice, Config, Level, LevelSource, PanelStatus, Renderer, action, tap_to_dial};

use crate::deck::{self, Deck};
use crate::hub::Hub;
use crate::nav::Nav;
use crate::saver::StateSaver;
use crate::watch::{WatchCmd, Watcher};
use crate::worker::Worker;

/// The longest a pass waits for the deck; also the latency of hub commands and values.
pub const READ_TIMEOUT: Duration = Duration::from_millis(100);
/// The Plus's LCD can ignore the first writes right after connect; repaint once after this.
const POST_CONNECT_REPAINT: Duration = Duration::from_secs(1);
/// Minimum horizontal distance, in strip pixels, for a swipe to count as a flick.
const FLICK_MIN_PX: i32 = 50;
/// After a read that returned input, at most this many more queued HID reports are taken in
/// the same pass, so a fast spin is handled together and painted once.
const MAX_DRAIN: usize = 32;

/// What outlives the deck: an unplug and replug keeps it all.
#[derive(Debug, Default)]
pub struct Runtime {
    /// What the panels show: probe values and the live brightness.
    pub values: Values,
    /// The persisted state. Changed here, then sent whole to the saver.
    pub state: State,
}

impl Runtime {
    /// Read the state file once, at start.
    pub fn load(cfg: &Config) -> Self {
        let state = State::load();
        let brightness = state.brightness.unwrap_or(cfg.brightness);
        Runtime { values: Values { brightness, ..Values::default() }, state }
    }
}

/// Where an input came from, so a failure is reported against the right key or dial.
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
    /// Actions that wait for an answer (Hyprland dispatchers) run here, off the deck loop.
    pub worker: Worker,
    pub rt: Runtime,
    pub deck: Option<Deck>,
    /// What was last published per strip panel (`Event::DialStatus`), so only changes go out.
    published: HashMap<u8, PanelStatus>,
    /// Volume changes sent to the watcher but not yet confirmed by its read-backs, per
    /// device. The bar shows confirmed + in-flight, so it never drops to a value the
    /// queued changes haven't reached yet (a fast spin would otherwise snap back as the
    /// read-backs catch up).
    in_flight: HashMap<AudioDevice, i32>,
    pub reload_at: Option<Instant>,
    pub repaint_at: Option<Instant>,
}

impl Daemon {
    pub fn new(cfg: Config, hub: Hub, watcher: Watcher, saver: StateSaver, worker: Worker) -> Self {
        let renderer = Renderer::new(&cfg.style);
        let rt = Runtime::load(&cfg);
        hub.publish(Event::Brightness { value: rt.values.brightness });
        Daemon {
            cfg,
            renderer,
            nav: Nav::default(),
            wanted_page: None,
            hub,
            watcher,
            saver,
            worker,
            rt,
            deck: None,
            published: HashMap::new(),
            in_flight: HashMap::new(),
            reload_at: None,
            repaint_at: None,
        }
    }

    // --- the deck coming and going ---------------------------------------------------------

    /// A deck was found: paint it, watch what its dials show, tell the configurators.
    pub fn attach(&mut self, mut d: Deck) {
        if let Err(e) = deck::paint_all(&mut d, &self.cfg, self.nav.current(), &mut self.renderer, &self.rt.values) {
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
        self.in_flight.clear();
        let _ = self.watcher.cmd.send(WatchCmd::Watch(Vec::new()));
        self.hub.publish(Event::Disconnected);
    }

    /// Tell the watcher what the deck's dials show. Sent on connect and on every reload.
    fn watch(&mut self) {
        let dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        let probes = probe::watched_probes(&self.cfg, self.nav.current(), dials);
        // Keep what is still shown, so a reload's repaint draws it as it is.
        self.rt.values.probes.retain(|p, _| probes.contains(p));
        // The watch set changed: what was in flight is no longer accounted for.
        self.in_flight.clear();
        let _ = self.watcher.cmd.send(WatchCmd::Watch(probes));
    }

    // --- one pass of the loop --------------------------------------------------------------

    /// The config changed on disk.
    pub fn reload(&mut self) {
        // A typo while hand-editing shouldn't wipe the deck: keep the last good config.
        let mut hand_edit = None;
        match Config::load() {
            Ok(new_cfg) => {
                hand_edit = hand_edited_brightness(&self.cfg, &new_cfg);
                self.cfg = new_cfg;
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
        if let Some(v) = hand_edit {
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
    /// drains, instead of snapping back to each stale read.
    pub fn apply_value(&mut self, probe: Probe, mut value: Value, applied: i32) {
        #[allow(irrefutable_let_patterns)]
        if let Probe::Volume(dev) = &probe
            && let Value::Level(read) = &value
        {
            let f = self.in_flight.entry(*dev).or_insert(0);
            let (nf, shown) = settle_volume(*f, applied, read);
            *f = nf;
            value = Value::Level(shown);
        }
        self.store_value(probe, value);
    }

    /// The one place a value is stored and what shows it marked for repainting at the end
    /// of the pass.
    fn store_value(&mut self, probe: Probe, value: Value) {
        if let Some(d) = &mut self.deck {
            d.dirty_where(&self.cfg, |dial| probe::shows(dial, &probe));
        }
        self.rt.values.probes.insert(probe, value);
    }

    /// The next moment something is due: a tap flash ending, the post-connect repaint, a
    /// debounced reload.
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
                // Cloned out of the config: `run_input` needs `&mut self`.
                let Some(act) = self.cfg.key(self.nav.current(), i).map(|k| k.action.clone()) else { return };
                self.run_input(Source::Key(i), &act, 1);
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

    /// Run `act` for an input. The one entry point for keys, dials, strip taps and flicks
    /// (and later ctl, multi-action steps and long presses): it logs, dispatches, moves the
    /// page if the action did, and reports a failure against its source. Returns whether
    /// the action ran.
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool {
        if steps > 1 {
            log::info!("{src} x{steps}: {}", act.summary());
        } else {
            log::info!("{src}: {}", act.summary());
        }
        // hyprctl waits for Hyprland's answer; the deck shouldn't. These run on the
        // worker, one at a time, in the order they were pressed.
        if matches!(act, Action::Hyprland { .. }) {
            for _ in 0..steps {
                self.worker.run(Self::index_of(src), act.clone());
            }
            return true;
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
                    Source::Key(index) => self.hub.publish(Event::ActionFailed { index, message }),
                    Source::Turn(index) | Source::Press(index) | Source::Tap(index) => {
                        self.hub.publish(Event::DialActionFailed { index, message })
                    }
                    Source::Flick => {}
                }
                false
            }
        }
    }

    /// The key or dial a failure is reported against; a flick has none.
    fn index_of(src: Source) -> u8 {
        match src {
            Source::Key(i) | Source::Turn(i) | Source::Press(i) | Source::Tap(i) => i,
            Source::Flick => 0,
        }
    }

    /// Publish the worker's failures since the last pass, against their keys.
    pub fn drain_worker_failures(&mut self) {
        for (i, message) in self.worker.failures() {
            log::warn!("key {i}: {message}");
            self.hub.publish(Event::ActionFailed { index: i, message });
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
        // The change goes to the watcher right after: it's in flight until a read-back
        // confirms it. Clamped like the watcher's coalesce, so the two stay in step.
        if !mute {
            let f = self.in_flight.entry(device).or_insert(0);
            *f = (*f + change).clamp(-100, 100);
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

    /// Repaint the deck after `nav` moved to another page, and tell the configurators.
    fn show_page(&mut self) {
        log::info!("page: {}", self.nav.current().unwrap_or(omadeck_core::HOME));
        self.paint_all();
        self.hub.publish(Event::PageChanged { page: self.nav.current().map(str::to_string) });
    }

    fn paint_all(&mut self) {
        if let Some(d) = &mut self.deck
            && let Err(e) = deck::paint_all(d, &self.cfg, self.nav.current(), &mut self.renderer, &self.rt.values)
        {
            log::warn!("repaint failed: {e}");
        }
    }

    fn paint_key(&mut self, i: u8, pressed: bool) {
        if let Some(d) = &self.deck {
            deck::paint_key(d, &self.cfg, self.nav.current(), &mut self.renderer, i, pressed);
        }
    }

    /// End of a pass: paint the dirty panels once, and tell the configurators about any
    /// panel whose value changed.
    pub fn flush(&mut self) {
        let Some(d) = &mut self.deck else { return };
        deck::flush_dirty(d, &self.cfg, &mut self.renderer, &self.rt.values);
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

/// The new start brightness when `new` changes the config's `brightness` (a hand edit, or
/// the configurator with no daemon), else `None`. A read-back confirms `applied` of the
/// in-flight change: take it off the total, and show what's still in flight on top of the
/// read, so the bar never drops to a value the queued changes haven't reached yet. The
/// total stays clamped like the watcher's coalesce, so the two stay in step.
fn settle_volume(f: i32, applied: i32, read: &Level) -> (i32, Level) {
    let f = (f - applied).clamp(-100, 100);
    let shown = if f != 0 { probe::guess_volume(read, f, false) } else { read.clone() };
    (f, shown)
}

fn hand_edited_brightness(old: &Config, new: &Config) -> Option<u8> {
    (new.brightness != old.brightness).then_some(new.brightness)
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
    fn hand_edit_is_a_changed_brightness() {
        let old = Config { brightness: 70, ..Config::default() };
        assert_eq!(hand_edited_brightness(&old, &Config { brightness: 30, ..Config::default() }), Some(30));
        assert_eq!(hand_edited_brightness(&old, &Config { brightness: 70, ..Config::default() }), None);
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
        // Two turns of +5 are in flight (total 10). The watcher confirms them at once,
        // coalesced: the bar stays where the turns put it, the total goes to zero.
        let (f, shown) = settle_volume(10, 10, &level(60));
        assert_eq!((f, shown.text.as_str()), (0, "60%"));
        // Not coalesced: the first read-back confirms only 5. The bar must stay 60
        // (55 read + 5 still in flight), not drop to the stale read.
        let (f, shown) = settle_volume(10, 5, &level(55));
        assert_eq!((f, shown.text.as_str()), (5, "60%"));
        // A poll confirms nothing: the in-flight total is untouched.
        let (f, shown) = settle_volume(10, 0, &level(55));
        assert_eq!((f, shown.text.as_str()), (10, "65%"));
        // The total stays clamped like the watcher's coalesce, so a spin past the
        // boundary doesn't leave a stale total behind.
        let (f, shown) = settle_volume(100, 100, &level(100));
        assert_eq!((f, shown.text.as_str()), (0, "100%"));
        let (f, shown) = settle_volume(-100, -100, &level(0));
        assert_eq!((f, shown.text.as_str()), (0, "0%"));
    }
}
