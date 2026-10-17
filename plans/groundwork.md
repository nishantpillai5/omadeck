# `feat/groundwork`: the restructuring the later PRs need, done once

Part of [PLAN.md](PLAN.md). Branch `feat/groundwork`, based on **`fix/review`**
([review-fixes.md](review-fixes.md)). PR 8 is based on this branch. It does decisions D2,
D3, D4 (`next_deadline`), D5, D7, D8, D9 (the cache part), D10, D11, the GUI part of D1,
and gives PRs 8–16 the names they build on. No new feature.

**Written 2026-10-17 against `d7bd448` plus `fix/review` as review-fixes.md describes it.**
The core and daemon code below was built in a scratch clone of `d7bd448` with
fix/review's API stubbed in: `cargo clippy -p omadeck-core -p omadeckd --all-targets --
-D warnings` clean, `cargo fmt` clean, and every core and daemon test below passes except
the two hub tests, which need a Unix socket the sandbox didn't allow (they run on Linux;
run them first in Task 8). The GUI code (Tasks 9–10) was written against the tree but not
compiled: gtk4 doesn't build on macOS.

**Outcome:** the daemon is a `Daemon` struct with one entry point for every input
(`run_input`), its state split into the device (`Deck`) and what outlives it (`Runtime`).
One watcher thread reads system state and runs volume changes in order. The hub never
blocks the deck loop. The configurator mirrors the daemon's brightness and panel levels
instead of polling `pactl`/`wpctl` itself. The GUI's action editor takes one `ActionSlot`,
so editors can nest (PR 12).

**Why** (the 2026-10-17 review, summarised in [review-2026-10-17.md](review-2026-10-17.md)):
- Four copies of the input block, and `after_volume` glued on at three call sites.
  PR 11, 12 and 16 would each add another.
- Daemon state lives in `Deck`, so it's lost on replug and unreachable with no deck.
- Volume changes run unordered, each on its own thread, and the read-back can be stale.
  The audio worker wakes 20 times a second forever, and `pactl` has no timeout.
- The hub writes to every client on the deck loop with a 200 ms timeout.
- The loop only wakes for HID input, so a tap flash ends up to 100 ms late.
- The GUI polls `pactl`/`wpctl` on its own thread for the strip preview, and its
  brightness slider writes `config.toml` for a value the daemon ignores.
- `action_editor` takes nine `impl Fn + Clone` generics, so PR 12's recursion can't
  compile. The "deck-owned actions" list is copied three times.
- The state saver writes a `State` with only `brightness`, and the background cache
  stats the file on every paint.

**What changes for the user:** almost nothing. The configurator's strip preview and
brightness slider follow the deck (a dial turn moves the slider; the preview shows the
deck's levels within a pass, not after a second). Volume changes from a fast spin apply in
order and the bar no longer jumps back. A tap flash ends on time. A busy configurator can't
stall the deck. "Try it now" tooltips for deck-only actions read "<kind> only works on the
deck".

## Where things run

- `omadeck-core` and `omadeckd` tests run anywhere (`cargo test -p omadeck-core`,
  `cargo test -p omadeckd`). No test needs a deck.
- `omadeck-gui` builds **on the Omarchy machine only** (gtk4, libadwaita). Tasks 9–10 are
  checked there: `cargo clippy --all-targets -- -D warnings` and the manual checks.
- Hardware checks need the Stream Deck + (`0fd9:0084`). Before running a dev build:
  `systemctl --user stop omadeckd`.

## Capture on the machine first

Nothing. The one hardware fact this plan relies on was checked in the crate sources:

- **Zero-timeout reads** (elgato-streamdeck 0.13.1, hidapi 2.6.7, default
  `linux-static-hidraw` backend). `DeviceStateReader::read(Some(t))` →
  `StreamDeck::read_input` → `util::read_data`, which calls
  `device.read_timeout(buf, t.as_millis() as i32)`. hidapi's Linux `hid_read_timeout` does
  `poll(fd, ms)`: with 0 it returns at once, 0 bytes, so `data[0] == 0` →
  `StreamDeckInput::NoData` → `Ok(vec![])`. A disconnect still shows (`POLLHUP` → error).
  fix/review already drains with `Duration::ZERO`. Two things follow: never pass `None`
  (that blocks for ever), and a timeout under 1 ms truncates to 0 ms, so `wait_for` rounds
  up to whole milliseconds.

## How the Elgato app does it

Nothing here is user-facing design, so there's little to copy. Two points the code follows:
- A dial action receives one `dialRotate` event with a `ticks` count per report, not one
  event per detent (SDK `dialRotate`), which is why a turn runs one folded volume command.
- The app's property inspector shows what the plugin last sent (`setFeedback`); it doesn't
  read the system itself. That's D2: only the daemon reads, the configurator mirrors.

## Decided

- **Module layout.** Core gains `proc.rs` (timeouts) and `probe.rs` (what is watched and
  what panels show). The daemon splits `main.rs` into `daemon.rs` (`Daemon`, `Runtime`,
  `Source`), `deck.rs` (`Deck`, painting), `watch.rs` (the watcher), `saver.rs`
  (`StateSaver`); `main.rs` keeps the CLI, `snapshot` and the `run()` driver (signals, file
  watching, scanning).
- **`Value::Level` holds `render::Level`** (it has `muted`; PR 8 reads it for mute keys).
  The watcher converts with `audio::Level::to_panel()` when it reads. There are still two
  `Level` types: `audio::Level { percent, muted }` is the raw read inside `audio.rs`; every
  other `Level` in this plan is `render::Level`.
- **Brightness lives once**, in `Runtime.values.brightness` (what panels draw), with
  `Runtime.state: State` (what is saved). D3's `Runtime { brightness, values }` would have
  held it twice, since D5 puts `brightness` in `Values`.
- **`DialStatus` = `pressed` + `PanelStatus`.** `PanelStatus { level }` is what a panel
  shows apart from being pressed, and is what `Event::DialStatus { index, status }`
  carries. PR 9 adds `live`, PR 10 `app`, PR 14 `media`, each
  `#[serde(default, skip_serializing_if = ...)]`.
- **The daemon publishes `DialStatus` by comparison**, at the end of every pass: it builds
  each panel's `PanelStatus` and publishes the ones that differ from what it last sent.
  That covers watcher values, guesses, brightness, reloads and connect in one place.
- **Watcher semantics.** `Watch(set)` keeps the last value of probes that stay watched
  (the deck kept them too) and reads new ones at once; `Watch(vec![])` on unplug forgets
  everything, so a replug re-sends all. `Volume` runs the change, then reads that device at
  once and sends it even if unchanged (the deck shows a guess). Back-to-back level changes
  of one device queued together are merged into one command (clamped ±100).
  `Refresh(Probe)` exists with `Probe::refresh_after()` (150 ms for volume) but nothing
  sends it until PR 8, so it carries `#[allow(dead_code)]`. `Stop` ends the thread.
- **`WatchCmd::Volume.change` is `i32`** (as PR 8 and D5 write it); the watcher clamps to
  ±100 and converts to `apply_volume`'s `i8`.
- **No guess before the first read** (fix/review's rule), and the guess is a pure core
  function, `probe::guess_volume`, so it's tested.
- **`watched_probes(cfg, page, n_dials)`** takes the page although only dials are watched
  today: PR 8 adds the page's keys. The parameter is `_page` until then.
- **Hub:** a per-client writer thread fed by a `sync_channel(256)`; a full queue drops the
  event, a closed one drops the client. The 200 ms write timeout goes: only the writer
  waits now. A client that falls 256 lines behind misses those events until the state
  changes again (D8's choice). Replay lives in a `Replay` struct (`status`, `page`,
  `brightness`, `panels`), so PR 8 adds `keys` beside `panels`.
- **Loop timing (D4):** fix/review's drain stays, moved into `Daemon::pass`. The first
  read waits `wait_for(now, next_deadline())`; `next_deadline` covers flash expiry, the
  post-connect repaint and the reload debounce. With no deck, the loop sleeps the same.
- **The configurator with a daemon** sends `SetBrightness` and never writes
  `config.brightness`; without one it edits `config.brightness` as upstream did. The
  slider follows `Event::Brightness`, except within 500 ms of the user moving it (so a
  late echo can't pull it back mid-drag).
- **The preview without a daemon** builds panels with `dial_status` from an empty
  `Values` whose brightness is the config's: volume dials show a dash, brightness dials the
  config's value. With a daemon, panels it has described show what it said; others fall
  back the same way.
- **`ActionTarget`**, not D10's `Target`: `app::Target { Key, Dial, Strip }` already
  exists for faces. `ActionTarget { Key(KeyRef), DialTurnLeft(u8), DialTurnRight(u8),
  DialPress(u8) }`; PR 8 adds `KeyOn`, PR 12 `Step`, PR 16 `DialLongPress`. Async results
  (a picker, a file dialog, a fetch) write through `App::set_action_at(&ActionTarget, ..)`.
- **`ActionSlot.get/set` take `&Rc<App>`** (D10 wrote `Fn() -> Action`): every existing
  closure has that shape, and it keeps the slot free of a strong `App` capture.
- **Kind defaults:** fix/review's `Action::default_for(kind, Direction)` for dial slots;
  keys keep `Action::empty_of_kind`. `ActionSlot.dir: Option<Direction>` (`None` = key).
- **Deck-owned actions (D11):** `is_deck_owned()` = `Page | Back | PageStep |
  Brightness`. `action::run` refuses exactly those (one check before its `match`; the
  `match` still lists them in one arm because Rust needs it exhaustive, and a test pins
  that the two agree). The GUI's ▶ button is off for them, except Open page, which shows
  the page.
- **Background cache (D9):** a cached image is never re-checked on paint; `clear_cache`
  (called on every reload) compares a `(mtime, len)` stamp and drops what changed. Images
  are `Arc<RgbaImage>`, shared by every paint that uses them.

## Ask before building

1. **Merging queued volume changes.** During a hard spin, changes queued behind a running
   command are merged into one, so Omarchy's pop-up shows fewer steps (the final level is
   the same). **Default:** merge. The alternative is one command per report, which lags a
   fast spin.
2. **Dropped hub events.** A configurator 256 lines behind (stuck for seconds while a dial
   spins) misses those events and may show a stale level until the next change.
   **Default:** accept it (D8). The alternative is re-sending the replay when its queue
   drains, about 20 more lines.

## Known traps

- **BUGS.md 4 (GUI):** never `if let Action::X { .. } = get(&a.config.borrow()) { edit(..) }`.
  Clone the action out (`let mut act = (slot.get)(&a);`), let the borrow end, then edit.
  The same in the daemon: `run_input` takes `&mut self`, so clone the action out of the
  config first (`let act = self.cfg.key(..).map(|k| k.action.clone())`), then call it.
- **Split borrows in `Daemon` methods.** `if let Some(d) = &mut self.deck { deck::paint_all(d,
  &self.cfg, .., &mut self.renderer, &self.rt.values) }` compiles: those are different
  fields. Calling a `self.method()` while `d` is still used afterwards doesn't. Finish with
  `d` first (NLL ends the borrow at its last use), or clone what you need. `pass` clones
  the reader `Arc` before the read loop for this reason.
- **`Rc<dyn Fn>` fields are called with parentheses:** `(slot.get)(&a)`, never
  `slot.get(&a)` (that looks for a method and fails).
- **`notify::Watcher` vs `watch::Watcher`.** `main.rs` imports notify's trait only for its
  methods: `use notify::{RecursiveMode, Watcher as _};`. The notify watcher variable is
  `fs_watch`.
- **Nothing slow on the deck loop.** No `audio::volume`, no `Command::output`, no
  `State::load` in a pass. `State::load` runs once in `Runtime::load`. Volume changes and
  reads only on the watcher; `apply_volume` blocks and is never called from the loop.
- **Hot reload must re-send `Watch`.** `Daemon::reload` calls `self.watch()`. So do
  `attach` (connect) and `detach` (`Watch(vec![])`). PR 8 adds page changes.
- **Replug must keep `Runtime`.** `detach` drops only the `Deck`. Nothing in `Deck` may
  hold a value the next connect needs.
- **Shutdown order:** `deck.reset()`, `hub.shutdown()`, `watcher.stop()` (sends `Stop`,
  joins), `saver.finish()` (drops its sender, joins; writes a pending state).
- **Dead code fails clippy.** A `pub fn` in a daemon module that nothing calls is a
  warning, and `-D warnings` makes it an error. Add each function in the task that first
  calls it (this plan does), and remove what a task replaces in the same task.
- **Goldens:** fix/review's goldens must not change in Task 4. A golden diff there is a
  bug in the refactor; don't regenerate.

## Types and signatures (final, after Task 10)

Core:
```rust
// proc.rs
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
pub fn output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output>;

// probe.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe { Volume(AudioDevice) }
impl Probe {
    pub fn interval(&self) -> Duration;                 // Volume: 1 s
    pub fn refresh_after(&self) -> &'static [Duration]; // Volume: [150 ms]
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value { Level(Level) }                         // render::Level
#[derive(Debug, Clone, Default)]
pub struct Values { pub probes: HashMap<Probe, Value>, pub brightness: u8 }
impl Values { pub fn level(&self, device: AudioDevice) -> Option<&Level>; }
pub fn brightness_level(value: u8) -> Level;
pub fn dial_status(dial: Option<&DialConfig>, values: &Values, pressed: bool) -> DialStatus;
pub fn guess_volume(cur: &Level, change: i32, mute: bool) -> Level;
pub fn shows(dial: Option<&DialConfig>, probe: &Probe) -> bool;
pub fn watched_probes(cfg: &Config, _page: Option<&str>, n_dials: usize) -> Vec<Probe>;

// render.rs
#[derive(Debug, Clone, Default)]
pub struct DialStatus { pub pressed: bool, pub panel: PanelStatus }
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PanelStatus { #[serde(default, skip_serializing_if = "Option::is_none")] pub level: Option<Level> }
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level { pub value: f32, pub text: String, pub muted: bool }
struct B1Colors { title: Rgb, value: Rgb, fill: Rgb, track: Rgb }   // private
fn draw_bar(img: &mut RgbaImage, rect: (u32, u32, u32, u32), value: f32, fill: Rgb, track: Rgb); // clamps value
fn backdrop_image(&mut self, path: &Path, w: u32, h: u32) -> Option<Arc<RgbaImage>>;

// ipc.rs, new events
Event::Brightness { value: u8 }
Event::DialStatus { index: u8, status: PanelStatus }

// state.rs
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct State { pub brightness: Option<u8> }
impl State {
    pub fn load() -> Self; pub fn load_from(path: &Path) -> Self;
    pub fn save(&self) -> anyhow::Result<()>; pub fn save_to(&self, path: &Path) -> anyhow::Result<()>;
}

// config.rs
impl Action { pub fn is_deck_owned(&self) -> bool; }   // Page | Back | PageStep | Brightness
impl DialConfig { pub fn action(&self, dir: Direction) -> &Action; pub fn action_mut(&mut self, dir: Direction) -> &mut Action; }

// action.rs
pub fn folded_volume(change: i8, steps: u8) -> i8;
pub fn apply_volume(device: AudioDevice, change: i8, mute: bool);   // blocking

// device.rs re-exports StreamDeckError
```

Daemon:
```rust
// watch.rs
pub enum WatchCmd { Watch(Vec<Probe>), Refresh(Probe), Volume { device: AudioDevice, change: i32, mute: bool }, Stop }
pub struct Watcher { pub cmd: mpsc::Sender<WatchCmd>, pub values: mpsc::Receiver<(Probe, Value)>, thread: JoinHandle<()> }
impl Watcher { pub fn stop(self); }
pub fn start() -> Watcher;
struct Poller { watched, last, next_due, forced, read_volume: fn(..), change_volume: fn(..) }
impl Poller {
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Value)>);
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Value)>);
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Value)>);
    fn next_due(&self) -> Option<Instant>;
}
fn read_volume(dev: AudioDevice) -> Option<Value>;

// saver.rs
pub struct StateSaver { tx: mpsc::Sender<State>, thread: JoinHandle<()> }
pub fn start_saver() -> StateSaver;  pub fn start_saver_at(path: PathBuf) -> StateSaver;
impl StateSaver { pub fn send(&self, state: State); pub fn finish(self); }

// deck.rs
pub struct Deck { pub dev, pub reader, pub kind, pub info, pub dial_pressed: Vec<bool>,
                  pub flash_until: Vec<Option<Instant>>, pub dirty_panels: Vec<bool> }
impl Deck { pub fn pressed(&self, i: u8, now: Instant) -> bool; pub fn mark_dirty(&mut self, i: u8);
            pub fn dirty_where(&mut self, cfg: &Config, pred: impl Fn(Option<&DialConfig>) -> bool);
            pub fn flash_deadline(&self) -> Option<Instant>; pub fn expire_flashes(&mut self, now: Instant); }
pub fn open(hid: &mut HidApi) -> Option<Deck>;
pub fn panel_status(d: &Deck, cfg: &Config, values: &Values, i: u8, now: Instant) -> DialStatus;
pub fn paint_all(d: &mut Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, values: &Values) -> Result<()>;
pub fn flush_dirty(d: &mut Deck, cfg: &Config, renderer: &mut Renderer, values: &Values);
pub fn paint_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool);

// daemon.rs
#[derive(Debug, Default)] pub struct Runtime { pub values: Values, pub state: State }
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick }   // + Display
pub struct Daemon { pub cfg, pub renderer, pub nav, pub wanted_page, pub hub, pub watcher, pub saver,
                    pub rt: Runtime, pub deck: Option<Deck>, published: HashMap<u8, PanelStatus>,
                    pub reload_at: Option<Instant>, pub repaint_at: Option<Instant> }
impl Daemon {
    pub fn new(cfg: Config, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self;
    pub fn attach(&mut self, d: Deck);  pub fn detach(&mut self, e: &StreamDeckError);
    pub fn reload(&mut self);  pub fn handle_commands(&mut self);
    pub fn apply_values(&mut self);  pub fn apply_value(&mut self, probe: Probe, value: Value);
    pub fn next_deadline(&self) -> Option<Instant>;
    pub fn pass(&mut self) -> Result<(), StreamDeckError>;
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool;
    fn dispatch(&mut self, act: &Action, steps: u8) -> Result<bool>;
    pub fn set_brightness(&mut self, set: Option<u8>, change: i32);
    pub fn flush(&mut self);  pub fn shutdown(self);
}
pub fn wait_for(now: Instant, deadline: Option<Instant>) -> Duration;
```

GUI (`app.rs` / `editor.rs`):
```rust
#[derive(Clone)]
pub enum ActionTarget { Key(KeyRef), DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8) }
impl ActionTarget { pub fn dial(i: u8, dir: Direction) -> Self; pub fn dial_slot(&self) -> Option<(u8, Direction)>; }
impl App {
    pub fn action_at(&self, at: &ActionTarget) -> Option<Action>;
    pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool;
}
#[derive(Clone)]
pub(crate) struct ActionSlot {
    pub target: ActionTarget, pub dir: Option<Direction>,
    pub get: Rc<dyn Fn(&Rc<App>) -> Action>, pub set: Rc<dyn Fn(&Rc<App>, Action)>,
    pub own_page: Option<String>,
    pub on_app_picked: Rc<dyn Fn(&Rc<App>, &ActionTarget, String, String, Option<String>)>,
    pub on_new_page: Rc<dyn Fn(&Rc<App>) -> String>,
    pub on_favicon: Option<FaviconHandler>,
}
impl ActionSlot { pub fn dial(i: u8, dir: Direction) -> Self; pub fn default_action(&self, kind: usize) -> Action; }
fn action_editor(app: &Rc<App>, group: &adw::PreferencesGroup, combo_title: &str, slot: &ActionSlot);
fn action_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot);
fn test_button(app: &Rc<App>, slot: &ActionSlot) -> gtk::Button;
```

## Tasks

Do them in order, one commit each. Before each commit: `cargo fmt`, then the task's check.
Sizes count added and changed lines, tests included. Code blocks marked **listing** are
the finished file or function: copy them.

### Task 1: core: commands with a timeout, volume changes you can wait for (~220 lines)

Files: `crates/omadeck-core/src/proc.rs` (new), `lib.rs`, `audio.rs`, `action.rs`.

1. `proc.rs`, listing:
```rust
//! Running a short command and waiting for it, with a time limit, so a hung tool (a
//! `pactl` while PipeWire restarts, a stalled Hyprland) can never block whoever waits.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// How long any command we wait on may take before it's killed.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// How often to check whether the child has exited.
const POLL: Duration = Duration::from_millis(5);

/// Run `cmd` with stdin closed and stdout/stderr captured, and wait at most `timeout`.
/// `None` if it couldn't start or ran too long (it is killed then). A non-zero exit is
/// still `Some`: look at `status`.
pub fn output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("failed to start {cmd:?}: {e}");
            return None;
        }
    };
    let deadline = Instant::now() + timeout;
    // Read both pipes on their own threads, so a chatty command can't fill a pipe and
    // block before it exits.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("{cmd:?} took longer than {timeout:?}; killed it");
                return None;
            }
        }
    };
    // A background grandchild can keep a pipe open after the child exits; don't wait
    // for it past the deadline.
    let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(50));
    Some(Output {
        status,
        stdout: stdout.recv_timeout(left).unwrap_or_default(),
        stderr: stderr.recv_timeout(left).unwrap_or_default(),
    })
}

/// Read a pipe to the end on a helper thread; the bytes arrive on the returned channel.
fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    match pipe {
        Some(mut p) => {
            thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                let _ = tx.send(buf);
            });
        }
        None => {
            let _ = tx.send(Vec::new());
        }
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn output_timeout_returns_output_and_status() {
        let out = output_timeout(&mut sh("echo hi; echo oops >&2; exit 3"), Duration::from_secs(2)).unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"oops\n");
    }

    #[test]
    fn output_timeout_kills_a_hung_command() {
        let start = Instant::now();
        assert!(output_timeout(&mut sh("sleep 5"), Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(2), "took {:?}", start.elapsed());
    }

    #[test]
    fn output_timeout_missing_command_is_none() {
        assert!(output_timeout(&mut Command::new("omadeck-no-such-command"), Duration::from_secs(1)).is_none());
    }

    #[test]
    fn output_timeout_big_output_does_not_block() {
        // More than a pipe buffer (64 KiB): would deadlock if we read only after exit.
        let out = output_timeout(&mut sh("head -c 200000 /dev/zero"), Duration::from_secs(2)).unwrap();
        assert_eq!(out.stdout.len(), 200_000);
    }
}
```
2. `lib.rs`: `pub mod proc;` after `pub mod paths;`.
3. `audio.rs` `capture`: the command goes through `output_timeout` (stdin is set there):
   ```rust
   /// Run a command and return its stdout, `None` if it is missing, fails or hangs.
   fn capture(args: &[&str]) -> Option<String> {
       let mut cmd = Command::new(args[0]);
       cmd.args(&args[1..]).env("LC_ALL", "C");
       let out = crate::proc::output_timeout(&mut cmd, crate::proc::COMMAND_TIMEOUT)?;
       out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
   }
   ```
   The import becomes `use std::process::Command;` (`Stdio` is unused now).
4. `action.rs`: `use crate::{paths, proc};`. Add `folded_volume`, make `run_steps`' Volume
   block use it (keep fix/review's Hyprland block as it is), and replace `run_volume` and
   `spawn_seq` (delete `spawn_seq`; nothing else uses it once `run_volume` changes):
   ```rust
   // in run_steps, the Volume block becomes:
   if let Action::Volume { device, change, mute } = action {
       return run_volume(*device, if *mute { 0 } else { folded_volume(*change, steps) }, *mute);
   }
   ```
```rust
/// The volume change `steps` detents of `change` make, as one command: clamped to ±100.
pub fn folded_volume(change: i8, steps: u8) -> i8 {
    (change as i32 * steps as i32).clamp(-100, 100) as i8
}

/// Change the system volume without waiting for it (`omadeckd press`, the configurator's
/// "Try it now"). The daemon calls [`apply_volume`] on its watcher thread instead.
fn run_volume(device: AudioDevice, change: i8, mute: bool) -> Result<()> {
    thread::Builder::new().name("volume".into()).spawn(move || apply_volume(device, change, mute))?;
    Ok(())
}

/// Change the system volume and wait until it's done. It goes through Omarchy's own
/// commands when they're installed, so its on-screen pop-up shows just like the volume
/// keys; otherwise it falls back to `wpctl`. Blocking: never call it on the deck loop.
pub fn apply_volume(device: AudioDevice, change: i8, mute: bool) {
    let backend = audio::backend();
    // Omarchy 3's pop-up needs the focused monitor, which costs a `hyprctl` run.
    let monitor = (backend == audio::Backend::Omarchy3).then(audio::focused_monitor).flatten();
    let cmds = volume_command(backend, device, change, mute, monitor.as_deref());
    log::info!("exec {}", cmds.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ; "));
    for argv in cmds {
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]).process_group(0);
        if proc::output_timeout(&mut c, proc::COMMAND_TIMEOUT).is_none() {
            log::warn!("{argv:?} failed to start or didn't finish");
        }
    }
    // Omarchy 4 has no command to change the mic's volume, so show its pop-up ourselves.
    // The change above has finished, so this reads the new level.
    if backend == audio::Backend::Omarchy4
        && device == AudioDevice::Input
        && !mute
        && let Some(lv) = audio::volume(AudioDevice::Input)
    {
        let mut c = Command::new("omarchy-osd");
        c.args(["-i", "microphone", "-p", &lv.percent.to_string()]);
        let _ = spawn(c);
    }
}
```
   The 80 ms sleep before Omarchy 4's mic pop-up goes: the change has
   finished when the level is read.
5. `hyprctl_dispatch` waits through `output_timeout`:
   ```rust
   let mut c = Command::new("hyprctl");
   c.arg("dispatch").args(args);
   let out = proc::output_timeout(&mut c, proc::COMMAND_TIMEOUT)
       .ok_or_else(|| anyhow::anyhow!("hyprctl didn't start or didn't answer"))?;
   ```
   replacing the `Command::new("hyprctl")…output()?` line. Remove imports that are now
   unused (`std::time::Duration` if nothing else uses it).

Test to copy: `output_timeout_kills_a_hung_command` (in the listing). Also add to
`action.rs` tests:
```rust
#[test]
fn folded_volume_clamps() {
    assert_eq!(folded_volume(5, 1), 5);
    assert_eq!(folded_volume(-5, 3), -15);
    assert_eq!(folded_volume(25, 8), 100);
    assert_eq!(folded_volume(-25, 8), -100);
    assert_eq!(folded_volume(i8::MIN, 8), -100);
}
```

**Check:** `cargo test -p omadeck-core proc::` → `test result: ok. 4 passed`;
`cargo test -p omadeck-core folded` → `1 passed`; `cargo test -p omadeck-core` all pass;
`cargo clippy --all-targets -- -D warnings` clean (no unused `spawn_seq`).

### Task 2: core: probes, values and the one panel builder (~300 lines)

Files: `crates/omadeck-core/src/probe.rs` (new), `render.rs`, `lib.rs`; call sites in
`crates/omadeckd/src/main.rs` and `crates/omadeck-gui/src/app.rs`.

1. `render.rs`: `DialStatus` gets a `PanelStatus`, and `Level` derives serde (it goes on
   the wire in Task 8):
```rust
/// Live state of one dial for its strip panel.
#[derive(Debug, Clone, Default)]
pub struct DialStatus {
    /// The dial is pressed (or a tap is still flashing).
    pub pressed: bool,
    /// What the panel shows, as the daemon tells the configurator.
    pub panel: PanelStatus,
}

/// What a dial's panel shows, apart from being pressed: the part the daemon sends the
/// configurator in `Event::DialStatus`. PR 9 adds `live`, PR 10 `app`, PR 14 `media`,
/// each `#[serde(default, skip_serializing_if = ...)]`.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelStatus {
    /// A value for the `$B1` layout (volume, brightness); `None` before the first read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}

/// A value for the `$B1` layout: a bar position plus the text next to it (e.g. `40%`).
/// Sent to the configurator in `Event::DialStatus`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Level {
    /// 0.0..=1.0, for the bar.
    pub value: f32,
    pub text: String,
    /// Mute: the text reads `Muted` and the bar is left empty.
    pub muted: bool,
}
```
   In `render_strip_panel`, `st.level` becomes `st.panel.level` (twice: the `muted` line
   and the `draw_b1` call). Fix the render tests that build a `DialStatus` the same way
   (`DialStatus { pressed: false, panel: PanelStatus { level: Some(..) } }`; import
   `PanelStatus` in the tests' `use super::{..}`).
2. `lib.rs`: `pub mod probe;` and `PanelStatus` in the `render` re-export:
   `pub use render::{Align, DialStatus, Face, Level, PANEL_W, PanelStatus, Renderer, tap_to_dial};`
3. `probe.rs`, listing:
```rust
//! System state the daemon watches (a probe), the values it reads, and what a dial's
//! strip panel shows for them. The daemon's watcher thread reads probes; the daemon, the
//! configurator and `snapshot` all build panels through [`dial_status`].

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::config::{AudioDevice, Config, DialConfig, LevelSource};
use crate::render::{DialStatus, Level, PanelStatus};

/// A volume set from the keyboard or `pavucontrol` lands in well under this.
const VOLUME_REFRESH: [Duration; 1] = [Duration::from_millis(150)];

/// Something the daemon reads from the system. PR 8 adds `Command { .. }`, PR 9 clocks
/// and meters, PR 10 `AppVolume(..)`, PR 14 `Player`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    /// An audio device's level and mute.
    Volume(AudioDevice),
}

impl Probe {
    /// How often it's re-read while watched.
    pub fn interval(&self) -> Duration {
        match self {
            Probe::Volume(_) => Duration::from_secs(1),
        }
    }

    /// When to re-read after a change made elsewhere (`WatchCmd::Refresh`; PR 8's
    /// toggles send it). A volume change made by the deck is read back at once instead.
    pub fn refresh_after(&self) -> &'static [Duration] {
        match self {
            Probe::Volume(_) => &VOLUME_REFRESH,
        }
    }
}

/// What a probe read. Later PRs add variants (PR 8 `Toggle`, PR 9 `Text`, …). It goes on
/// the wire in later events, hence serde.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// A volume device's level, as the panel draws it (`audio::Level::to_panel`).
    Level(Level),
}

/// Everything the panels show: the last value of each probe, and the panel brightness.
#[derive(Debug, Clone, Default)]
pub struct Values {
    pub probes: HashMap<Probe, Value>,
    /// 0–100.
    pub brightness: u8,
}

impl Values {
    /// The last known level of a volume device.
    pub fn level(&self, device: AudioDevice) -> Option<&Level> {
        match self.probes.get(&Probe::Volume(device))? {
            Value::Level(l) => Some(l),
        }
    }
}

/// The `$B1` value for the panel brightness.
pub fn brightness_level(value: u8) -> Level {
    Level { value: value as f32 / 100.0, text: format!("{value}%"), muted: false }
}

/// What dial `dial`'s strip panel shows. The one place that turns values into a panel:
/// the daemon, the configurator's preview and `snapshot` all call it. A dial that shows a
/// level that isn't known yet gets `level: None` (the panel draws a dash).
pub fn dial_status(dial: Option<&DialConfig>, values: &Values, pressed: bool) -> DialStatus {
    let level = dial.and_then(DialConfig::level_source).and_then(|src| match src {
        LevelSource::Volume(dev) => values.level(dev).cloned(),
        LevelSource::Brightness => Some(brightness_level(values.brightness)),
    });
    DialStatus { pressed, panel: PanelStatus { level } }
}

/// The level a volume change will leave, guessed from the current one, so the bar moves
/// at once; the watcher's read-back a moment later corrects it. A mute toggles; a change
/// adds `change` percent, clamped to 0–100, and unmutes.
pub fn guess_volume(cur: &Level, change: i32, mute: bool) -> Level {
    let pct = (cur.value * 100.0).round() as i32;
    if mute {
        let muted = !cur.muted;
        Level { value: cur.value, text: if muted { "Muted".into() } else { format!("{pct}%") }, muted }
    } else {
        let p = (pct + change).clamp(0, 100);
        Level { value: p as f32 / 100.0, text: format!("{p}%"), muted: false }
    }
}

/// Whether `dial`'s panel shows `probe`, so a new value for it repaints the panel.
pub fn shows(dial: Option<&DialConfig>, probe: &Probe) -> bool {
    match (dial.and_then(DialConfig::level_source), probe) {
        (Some(LevelSource::Volume(dev)), Probe::Volume(p)) => dev == *p,
        _ => false,
    }
}

/// Everything the watcher should read while `page` is shown on a deck with `n_dials`
/// dials, each once: today the dials' volume levels, in dial order. PR 8 adds the probes
/// of the keys on `page` (hence the parameter), PR 9 live keys, PR 10 app volumes.
pub fn watched_probes(cfg: &Config, _page: Option<&str>, n_dials: usize) -> Vec<Probe> {
    let mut out = Vec::new();
    for i in 0..n_dials {
        if let Some(LevelSource::Volume(dev)) = cfg.dial(i as u8).and_then(DialConfig::level_source) {
            let p = Probe::Volume(dev);
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Action;

    fn volume_dial(index: u8, device: AudioDevice) -> DialConfig {
        DialConfig { turn_right: Action::Volume { device, change: 5, mute: false }, ..DialConfig::new(index) }
    }

    fn brightness_dial(index: u8) -> DialConfig {
        DialConfig { turn_right: Action::Brightness { change: 5, set: None }, ..DialConfig::new(index) }
    }

    #[test]
    fn dial_status_volume_known_and_unknown() {
        let dial = volume_dial(0, AudioDevice::Output);
        let mut values = Values::default();
        // Not read yet: the panel shows a dash.
        assert_eq!(dial_status(Some(&dial), &values, false).panel.level, None);
        let forty = Level { value: 0.4, text: "40%".into(), muted: false };
        values.probes.insert(Probe::Volume(AudioDevice::Output), Value::Level(forty));
        let st = dial_status(Some(&dial), &values, true);
        assert!(st.pressed);
        assert_eq!(st.panel.level.unwrap().text, "40%");
        // The input's level doesn't leak into an output dial.
        let mic = volume_dial(1, AudioDevice::Input);
        assert_eq!(dial_status(Some(&mic), &values, false).panel.level, None);
    }

    #[test]
    fn dial_status_brightness_and_plain() {
        let values = Values { brightness: 70, ..Values::default() };
        assert_eq!(dial_status(Some(&brightness_dial(0)), &values, false).panel.level, Some(brightness_level(70)));
        assert_eq!(dial_status(Some(&DialConfig::new(0)), &values, false).panel, PanelStatus::default());
        assert_eq!(dial_status(None, &values, false).panel, PanelStatus::default());
    }

    #[test]
    fn watched_probes_in_dial_order_each_once() {
        let cfg = Config {
            dials: vec![
                volume_dial(0, AudioDevice::Input),
                brightness_dial(1),
                volume_dial(2, AudioDevice::Output),
                volume_dial(3, AudioDevice::Input),
            ],
            ..Config::default()
        };
        let both = vec![Probe::Volume(AudioDevice::Input), Probe::Volume(AudioDevice::Output)];
        assert_eq!(watched_probes(&cfg, None, 4), both);
        // Dials aren't per page.
        assert_eq!(watched_probes(&cfg, Some("Media"), 4), both);
        // Only the dials the deck has; a deck without dials watches nothing.
        assert_eq!(watched_probes(&cfg, None, 2), vec![Probe::Volume(AudioDevice::Input)]);
        assert_eq!(watched_probes(&cfg, None, 0), vec![]);
    }

    #[test]
    fn guess_volume_moves_and_mutes() {
        let fifty = Level { value: 0.5, text: "50%".into(), muted: false };
        assert_eq!(guess_volume(&fifty, 15, false), Level { value: 0.65, text: "65%".into(), muted: false });
        assert_eq!(guess_volume(&fifty, -80, false).text, "0%");
        assert_eq!(guess_volume(&fifty, 80, false).text, "100%");
        let muted = guess_volume(&fifty, 0, true);
        assert_eq!(muted, Level { value: 0.5, text: "Muted".into(), muted: true });
        assert_eq!(guess_volume(&muted, 0, true), fifty);
    }

    #[test]
    fn shows_matches_the_device() {
        let out = volume_dial(0, AudioDevice::Output);
        assert!(shows(Some(&out), &Probe::Volume(AudioDevice::Output)));
        assert!(!shows(Some(&out), &Probe::Volume(AudioDevice::Input)));
        assert!(!shows(Some(&brightness_dial(1)), &Probe::Volume(AudioDevice::Output)));
        assert!(!shows(None, &Probe::Volume(AudioDevice::Output)));
    }
}
```
4. Call sites, so the tree builds (Task 7 replaces the daemon's):
   - `omadeckd` `dial_status`: its last line becomes
     `DialStatus { pressed: …, panel: omadeck_core::PanelStatus { level } }`.
   - `omadeckd` `snapshot`: the brightness `Level` built by hand and the `DialStatus`
     loop become:
     ```rust
     // Brightness dials show the brightness the deck runs at; volume dials show a dash,
     // as their level is live and only the running daemon reads it.
     let values = Values { brightness: State::load().brightness.unwrap_or(cfg.brightness), ..Values::default() };
     let status: Vec<_> = (0..info.dials).map(|i| probe::dial_status(cfg.dial(i), &values, false)).collect();
     ```
     with `use omadeck_core::probe::{self, Values};`.
   - GUI `refresh_strip`: `DialStatus { pressed: false, level }` becomes
     `DialStatus { pressed: false, panel: PanelStatus { level } }` (import `PanelStatus`).
     Task 9 rewrites the function.

Test to copy: `dial_status_volume_known_and_unknown` (in the listing).

**Check:** `cargo test -p omadeck-core probe::` → `test result: ok. 5 passed`;
`cargo test -p omadeck-core render` all pass, goldens unchanged; `cargo clippy
--all-targets -- -D warnings` clean (on the Omarchy machine, for the GUI call site).

### Task 3: core: deck-owned actions in one place, dial slots by direction (~110 lines)

Files: `crates/omadeck-core/src/config.rs`, `action.rs`.

1. `Action::is_deck_owned` (D11):
```rust
/// Run by the daemon itself (page moves and the panel brightness), never by
/// `action::run`. The one list: `action::run` refuses exactly these, and the
/// configurator's "Try it now" is off for them.
pub fn is_deck_owned(&self) -> bool {
    matches!(self, Action::Page { .. } | Action::Back | Action::PageStep { .. } | Action::Brightness { .. })
}
```
2. `impl DialConfig`, before `level_source`:
```rust
/// The action in slot `dir`.
pub fn action(&self, dir: Direction) -> &Action {
    match dir {
        Direction::Left => &self.turn_left,
        Direction::Right => &self.turn_right,
        Direction::Press => &self.press,
    }
}

pub fn action_mut(&mut self, dir: Direction) -> &mut Action {
    match dir {
        Direction::Left => &mut self.turn_left,
        Direction::Right => &mut self.turn_right,
        Direction::Press => &mut self.press,
    }
}
```
3. `action::run` refuses exactly the deck-owned kinds. At the top of `run`:
   ```rust
   if action.is_deck_owned() {
       bail!("{} only works on the deck", Action::KINDS[action.kind_index()]);
   }
   ```
   and the two old bail arms (`Action::Page { .. } | Action::Back | Action::PageStep { .. }
   => bail!(..)` and `Action::Brightness { .. } => bail!(..)`) become one:
   ```rust
   // Refused above; the daemon runs these itself.
   Action::Page { .. } | Action::Back | Action::PageStep { .. } | Action::Brightness { .. } => Ok(()),
   ```

Tests to copy (`config.rs` tests, then `action.rs` tests):
```rust
#[test]
fn deck_owned_kinds() {
    let owned: Vec<usize> =
        (0..Action::KINDS.len()).filter(|&k| Action::empty_of_kind(k).is_deck_owned()).collect();
    assert_eq!(owned, vec![6, 7, 9, 10]); // Open page, Go back, Next/previous page, Brightness
}

#[test]
fn dial_action_by_direction() {
    let mut d = DialConfig::new(0);
    *d.action_mut(Direction::Left) = Action::Back;
    *d.action_mut(Direction::Press) = Action::PageStep { change: 1 };
    assert_eq!(d.turn_left, Action::Back);
    assert_eq!(d.action(Direction::Left), &Action::Back);
    assert_eq!(d.action(Direction::Right), &Action::None);
    assert_eq!(d.action(Direction::Press), &d.press);
}
```
```rust
#[test]
fn run_refuses_exactly_the_deck_owned_kinds() {
    for kind in 0..Action::KINDS.len() {
        // Volume would really change the volume; it isn't deck-owned anyway.
        if kind == 8 {
            continue;
        }
        let a = Action::empty_of_kind(kind);
        let refused = run(&a).is_err_and(|e| e.to_string().ends_with("only works on the deck"));
        assert_eq!(refused, a.is_deck_owned(), "{a:?}");
    }
}
```

**Check:** `cargo test -p omadeck-core deck_owned` → `2 passed`;
`cargo test -p omadeck-core dial_action_by` → `1 passed`; clippy clean. The GUI still
builds (its inline copies go in Task 10).

### Task 4: core: the bar drawer, and a cache that doesn't stat on paint (~180 lines)

File: `crates/omadeck-core/src/render.rs`. No pixel may change.

1. `draw_b1` takes its colours from the caller, and the bar is its own function (PR 8
   draws key bars with it, PR 10 tones each part separately). Next to `fill_pill`:
```rust
/// The colours of a `$B1` panel's parts, chosen by the caller (PR 10 tones each part
/// against the image under it).
#[derive(Debug, Clone, Copy)]
struct B1Colors {
    title: Rgb,
    value: Rgb,
    fill: Rgb,
    track: Rgb,
}

/// A groove bar in `rect` (x, y, w, h): the track, then the fill up to `value`, clamped
/// to 0–1 here (a NaN draws no fill). A fill never shrinks below a round end cap.
fn draw_bar(img: &mut RgbaImage, rect: (u32, u32, u32, u32), value: f32, fill: Rgb, track: Rgb) {
    let (bx, by, bw, bh) = rect;
    fill_pill(img, bx as i64, by as i64, bw as i64, bh as i64, track);
    let value = if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) };
    if value > 0.0 {
        let fw = ((bw as f32 * value).round() as i64).max(bh as i64);
        fill_pill(img, bx as i64, by as i64, fw, bh as i64, fill);
    }
}
```
   `draw_b1`'s signature and its two colour uses:
   ```rust
   fn draw_b1(&self, img: &mut RgbaImage, face: &Face, colors: B1Colors, want_label: bool,
              icon: Option<RgbaImage>, level: Option<&Level>) {
       if want_label {
           self.draw_text_in(img, X1_TITLE, STRIP_TITLE_PX, Align::Left, face.label, colors.title, true);
       }
       // … icon unchanged …
       let (text, value) = match level {
           Some(l) => (l.text.as_str(), if l.muted { 0.0 } else { l.value }),
           None => ("–", 0.0),
       };
       self.draw_text_in(img, B1_VALUE, B1_VALUE_PX, Align::Right, text, colors.value, true);
       draw_bar(img, B1_BAR, value, colors.fill, colors.track);
   }
   ```
   (`#[allow(clippy::too_many_arguments)]` goes: six arguments now.) The caller in
   `render_strip_panel` passes the colours `draw_b1` used before. At `d7bd448` that is
   one colour for everything:
   ```rust
   let bar = B1Colors { title: fg, value: fg, fill: fg, track: tone.mix(fg, 0.22) };
   self.draw_b1(&mut img, &face, bar, want_label, icon, st.panel.level.as_ref());
   ```
   If fix/review's `draw_b1` already picks different colours for the title, value, fill
   or track, put exactly those into `B1Colors` instead. The goldens decide: they must not
   change.
2. The cache (D9). Replace fix/review's `type Backdropped` and add the stamp:
```rust
/// A file's identity for the background cache: its mtime and length, `None` when it
/// can't be stat'ed. Taken when an image is loaded and compared only on `clear_cache`.
type Stamp = Option<(std::time::SystemTime, u64)>;

/// A cached background image: the file's stamp when it was loaded, and the fitted image
/// (`None` when the load failed), shared with every paint that uses it.
type Backdropped = (Stamp, Option<Arc<RgbaImage>>);

fn stamp(path: &Path) -> Stamp {
    let m = fs::metadata(path).ok()?;
    Some((m.modified().ok()?, m.len()))
}
```
   The field: `backdrops: HashMap<(PathBuf, u32, u32), Backdropped>,`. Add
   `use std::sync::Arc;`. `clear_cache`'s backdrop line:
   ```rust
   // The only place backdrop files are stat'ed: a paint never touches the disk.
   self.backdrops.retain(|(path, _, _), (st, _)| stamp(path) == *st);
   ```
   `backdrop_image` (keep fix/review's `load_attempts` counter line where it was, before
   the `load_cover` call):
```rust
/// A fitted background image (strip or dial), decoded once per file and shared. A
/// hit costs no file access; `clear_cache` (on reload) drops entries whose file
/// changed. A missing or unreadable file is cached as `None` too, so it warns once.
fn backdrop_image(&mut self, path: &Path, w: u32, h: u32) -> Option<Arc<RgbaImage>> {
    let k = (path.to_path_buf(), w, h);
    if let Some((_, img)) = self.backdrops.get(&k) {
        return img.clone();
    }
    let st = stamp(path);
    let loaded = st.is_some().then(|| load_cover(path, w, h)).flatten().map(Arc::new);
    if loaded.is_none() {
        log::warn!("could not load background image {}", path.display());
    }
    #[cfg(test)]
    if loaded.is_some() {
        self.decode_count.set(self.decode_count.get() + 1);
    }
    self.backdrops.insert(k, (st, loaded.clone()));
    loaded
}
```
   Its two callers in `panel_backdrop` deref the `Arc`: `imageops::crop_imm(&*full, ..)`
   and `imageops::overlay(&mut img, &*image, 0, 0)`. fix/review's `failed_decode_is_cached`
   still holds: a failed load is cached as `(stamp, None)`.

Tests to copy:
```rust
#[test]
fn draw_bar_clamps_its_value() {
    let (fill, track) = (Rgb(255, 0, 0), Rgb(0, 0, 255));
    let bar = |v: f32| {
        let mut img = RgbaImage::new(200, 100);
        super::draw_bar(&mut img, B1_BAR, v, fill, track);
        img
    };
    assert_eq!(bar(1.7), bar(1.0), "above 1 is full");
    assert_eq!(bar(-0.5), bar(0.0), "below 0 is empty");
    assert_eq!(bar(f32::NAN), bar(0.0), "NaN is empty");
    // Half full: red at a quarter of the bar, blue at three quarters.
    let (bx, by, bw, bh) = B1_BAR;
    let half = bar(0.5);
    assert_eq!(half.get_pixel(bx + bw / 4, by + bh / 2).0[..3], [255, 0, 0]);
    assert_eq!(half.get_pixel(bx + bw * 3 / 4, by + bh / 2).0[..3], [0, 0, 255]);
}

#[test]
fn backdrop_is_shared_and_only_rechecked_on_clear_cache() {
    let mut r = Renderer::new(&test_style());
    let path = tmp_image("stamp", &solid_img(4, 2, Rgba([9, 9, 9, 255])));
    let first = r.backdrop_image(&path, 4, 2).unwrap();
    // A bigger file under the same name: a paint doesn't look, so the cached one stays.
    image::DynamicImage::ImageRgba8(RgbaImage::from_pixel(8, 4, Rgba([200, 1, 1, 255]))).save(&path).unwrap();
    let again = r.backdrop_image(&path, 4, 2).unwrap();
    assert!(std::sync::Arc::ptr_eq(&first, &again), "a hit shares the decoded image");
    // A reload sees the new length and loads it.
    r.clear_cache();
    assert_eq!(*r.backdrop_image(&path, 4, 2).unwrap().get_pixel(0, 0), Rgba([200, 1, 1, 255]));
    let _ = std::fs::remove_dir_all(path.parent().unwrap());
}
```
(`Renderer::new`, as fix/review's neighbouring cache tests use: no text is drawn.)

**Check:** `cargo test -p omadeck-core render` → all pass, including fix/review's goldens,
`clear_cache_keeps_unchanged_backdrop`, `touched_file_is_reloaded` and
`failed_decode_is_cached`; clippy clean.

### Task 5: the state file, saved whole (~170 lines)

Files: `crates/omadeck-core/src/state.rs`, `crates/omadeckd/src/saver.rs` (new),
`crates/omadeckd/src/main.rs`.

1. `state.rs`, listing:
```rust
//! Values the daemon owns at runtime, kept out of the hand-edited config.

use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::paths;

/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's
/// `brightness` is the start value; once the deck has run, this file wins (a hand edit
/// of the config's value wins again). Later PRs that persist something add a field here,
/// with `#[serde(default, skip_serializing_if = ...)]`.
#[derive(Serialize, Deserialize, Default, Debug, Clone, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl State {
    /// The state as saved, or empty when the file is absent or unreadable.
    pub fn load() -> Self {
        Self::load_from(&paths::state_file())
    }

    pub fn load_from(path: &Path) -> Self {
        std::fs::read_to_string(path).ok().and_then(|s| toml::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save(&self) -> anyhow::Result<()> {
        self.save_to(&paths::state_file())
    }

    /// Atomic write (temp file, fsync, rename), so a reader never sees a half-written
    /// file and a crash can't leave an empty one. The temp name carries the pid, so a
    /// second daemon can't race on it.
    pub fn save_to(&self, path: &Path) -> anyhow::Result<()> {
        let Some(dir) = path.parent() else { anyhow::bail!("state path has no parent") };
        std::fs::create_dir_all(dir)?;
        let tmp = dir.join(format!(".state.toml.{}.tmp", std::process::id()));
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(toml::to_string_pretty(self)?.as_bytes())?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_absent_is_empty() {
        let s = State { brightness: Some(42) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }

    #[test]
    fn save_to_and_load_from_a_file() {
        let dir = std::env::temp_dir().join(format!("omadeck-state-test-{}", std::process::id()));
        let path = dir.join("state.toml");
        assert_eq!(State::load_from(&path), State::default()); // absent
        State { brightness: Some(30) }.save_to(&path).unwrap();
        assert_eq!(State::load_from(&path), State { brightness: Some(30) });
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "brightness = 30\n");
        std::fs::write(&path, "brightness = \"oops\"").unwrap();
        assert_eq!(State::load_from(&path), State::default()); // unreadable
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```
2. `saver.rs`, listing:
```rust
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
        saver.send(State { brightness: Some(10) });
        saver.send(State { brightness: Some(20) });
        saver.finish();
        assert_eq!(State::load_from(&path), State { brightness: Some(20) });
        let _ = std::fs::remove_dir_all(&dir);
    }
}
```
3. `main.rs`: `mod saver;`. Replace fix/review's saver: delete `brightness_saver`,
   `save_brightness` and their test `saver_writes_the_last_value_when_dropped`; the
   `(brightness_tx, brightness_rx)` channel and thread become
   `let saver = saver::start_saver();`. Until Task 7 makes `Runtime`, keep a local
   `let mut state = State::load();` next to it. Where a brightness was sent
   (`brightness.send(next)` in `do_brightness`, and fix/review's no-deck
   `saver.send(value.min(100))` in `set_brightness`), set the field and send the whole
   state: pass `saver: &StateSaver` and `state: &mut State` instead of the
   `&mpsc::Sender<u8>`, and write
   ```rust
   state.brightness = Some(next);
   saver.send(state.clone());
   ```
   `connect` reads `state.brightness` (pass `&State`) instead of `State::load()`, so the
   file is read once at start. At shutdown, fix/review's `drop(brightness_tx); let _ =
   saver.join();` becomes `saver.finish();`. `run_deck_action` and `dispatch` gain the
   extra parameter; `run_deck_action` already has `#[allow(clippy::too_many_arguments)]`.

Test to copy: `saver_keeps_the_last_state_and_finish_writes_it` (in the listing).

**Check:** `cargo test -p omadeck-core state::` → `2 passed`; `cargo test -p omadeckd` →
all pass (fix/review's count minus one, plus one); clippy clean. On the deck: turn the
brightness dial and `systemctl --user restart omadeckd` at once: it comes back at the new
value.

### Task 6: the watcher replaces the audio worker (~330 lines)

Files: `crates/omadeckd/src/watch.rs` (new), `main.rs`.

1. `watch.rs`, listing:
```rust
//! The watcher: one thread that reads system state off the deck loop (today the volume
//! levels; PR 8 adds state commands, PR 9 clocks and meters, …) and runs the volume
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
    // PR 8's toggles are the first to send it; drop the allow then.
    #[allow(dead_code)]
    Refresh(Probe),
    /// Change a device's volume here, after any earlier change, then re-read it at once
    /// and send the result even if unchanged (the deck is showing a guess).
    Volume { device: AudioDevice, change: i32, mute: bool },
    /// End the thread (shutdown). Explicit, because PR 8 gives the thread a sender of its
    /// own (for its runners), so the channel never disconnects by itself.
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
```
2. `main.rs`:
   - `mod watch;` and `use watch::{WatchCmd, Watcher};`. The notify import becomes
     `use notify::{RecursiveMode, Watcher as _};` and the notify watcher variable
     `watcher` is renamed `fs_watch` everywhere in `run()`.
   - Delete `AudioCmd`, `AudioWorker`, `start_audio_worker`, `watched_devices`,
     `AUDIO_POLL`, `AUDIO_REFRESH_DELAY`.
   - `let audio = start_audio_worker();` → `let watcher = watch::start();`.
   - Each `audio.cmd.send(AudioCmd::Watch(watched_devices(&cfg, n)))` →
     `watcher.cmd.send(WatchCmd::Watch(probe::watched_probes(&cfg, nav.current(), n)))`
     (`n` as `usize`); `AudioCmd::Watch(Vec::new())` → `WatchCmd::Watch(Vec::new())`. In
     the reload block, `d.device_levels.retain(|dev, _| watched.contains(dev))` becomes
     `d.device_levels.retain(|dev, _| watched.contains(&Probe::Volume(*dev)))`.
   - The levels drain:
     ```rust
     while let Ok((p, v)) = watcher.values.try_recv() {
         let (Probe::Volume(dev), Value::Level(level)) = (p, v);
         d.device_levels.insert(dev, level);
         for (i, src) in d.dial_levels.iter().enumerate().take(d.info.dials as usize) {
             if *src == Some(LevelSource::Volume(dev)) {
                 d.dirty_panels[i] = true;
             }
         }
     }
     ```
     (The `let` is irrefutable while `Probe` and `Value` have one variant each; Task 7
     replaces it with `apply_value`.)
   - `dispatch`: before the catch-all `a => action::run_steps(..)` arm:
     `Action::Volume { .. } => Ok(false), // after_volume hands it to the watcher`.
   - `after_volume(d, &watcher, act, steps)`:
     ```rust
     fn after_volume(d: &mut Deck, watcher: &Watcher, act: &Action, steps: u8) {
         let Action::Volume { device, change, mute } = act else { return };
         let change = if *mute { 0 } else { i32::from(action::folded_volume(*change, steps)) };
         // Runs on the watcher, after any earlier change, and is read back there.
         let _ = watcher.cmd.send(WatchCmd::Volume { device: *device, change, mute: *mute });
         // No guess before the first read: that read is moments away.
         let Some(cur) = d.device_levels.get(device) else { return };
         let next = probe::guess_volume(cur, change, *mute);
         d.device_levels.insert(*device, next);
         for (i, src) in d.dial_levels.iter().enumerate().take(d.info.dials as usize) {
             if *src == Some(LevelSource::Volume(*device)) {
                 d.dirty_panels[i] = true;
             }
         }
     }
     ```
   - Shutdown, before `saver.finish()`: `watcher.stop();`.

Test to copy: `volume_changes_run_in_order_and_are_read_back` (in the listing).

**Check:** `cargo test -p omadeckd watch::` → `test result: ok. 4 passed`; `cargo test
-p omadeckd` all pass; clippy clean; `grep -n "AudioCmd\|audio\.cmd\|audio\.levels"
crates/omadeckd/src/main.rs` → nothing. On the deck: the PR 4 checks under "Hardware
checks" below.

### Task 7: the daemon split: `Deck`, `Runtime`, `Daemon`, one input path, deadlines (~600 lines, mostly moved)

The largest task, because every piece of `run()` moves at once; the code is all here to
copy, and the behaviour is unchanged apart from D4's deadlines. If the session runs long,
stop after step 2 compiles (`deck.rs` unused gives dead-code errors, so don't commit
then), and finish in a second session.

Files: `crates/omadeckd/src/deck.rs` (new), `daemon.rs` (new), `main.rs`,
`crates/omadeck-core/src/device.rs`.

1. `device.rs`: add `StreamDeckError` to the `elgato_streamdeck` re-export:
   `DeviceStateReader, DeviceStateUpdate, StreamDeck, StreamDeckError, list_devices, new_hidapi, refresh_device_list,`.
2. `deck.rs`, listing (fix/review's `mark_dirty` and `flush_dirty` move here as `Deck`'s
   method and a function that takes the values):
```rust
//! The connected Stream Deck: its handle, per-device scratch state, and painting.
//! Nothing here outlives an unplug; the daemon's `Runtime` holds what must.

use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use omadeck_core::device::{self, DeviceStateReader, HidApi, ImageRect, Kind, StreamDeck};
use omadeck_core::probe::{self, Values};
use omadeck_core::{Config, DeckInfo, DialStatus, PANEL_W, Renderer};

/// How long a panel stays highlighted after a tap on the strip.
pub const DIAL_FLASH: Duration = Duration::from_millis(150);

pub struct Deck {
    pub dev: Arc<StreamDeck>,
    pub reader: Arc<DeviceStateReader>,
    pub kind: Kind,
    pub info: DeckInfo,
    /// Whether each dial is physically pressed down.
    pub dial_pressed: Vec<bool>,
    /// When each dial's panel was tapped (the tap time, not a deadline), while it flashes.
    pub flash_until: Vec<Option<Instant>>,
    /// Panels to repaint at the end of this pass (`flush_dirty`), however many changes
    /// dirtied them. PR 8 adds `dirty_keys` next to it.
    pub dirty_panels: Vec<bool>,
}

impl Deck {
    /// Whether dial `i` is drawn pressed: held down, or tapped less than `DIAL_FLASH` ago.
    pub fn pressed(&self, i: u8, now: Instant) -> bool {
        let n = i as usize;
        let flashed = self.flash_until.get(n).and_then(|t| *t).is_some_and(|t| now.duration_since(t) < DIAL_FLASH);
        self.dial_pressed.get(n).copied().unwrap_or(false) || flashed
    }

    /// Mark dial `i`'s panel for the end-of-pass paint.
    pub fn mark_dirty(&mut self, i: u8) {
        if let Some(f) = self.dirty_panels.get_mut(i as usize) {
            *f = true;
        }
    }

    /// Mark the panels whose dial `pred` picks for repainting at the end of the pass.
    pub fn dirty_where(&mut self, cfg: &Config, pred: impl Fn(Option<&omadeck_core::DialConfig>) -> bool) {
        for i in 0..self.info.dials {
            if pred(cfg.dial(i)) {
                self.dirty_panels[i as usize] = true;
            }
        }
    }

    /// When the earliest tap flash ends, if any is showing.
    pub fn flash_deadline(&self) -> Option<Instant> {
        self.flash_until.iter().flatten().map(|t| *t + DIAL_FLASH).min()
    }

    /// End the tap flashes that have run their time; their panels repaint this pass.
    pub fn expire_flashes(&mut self, now: Instant) {
        for i in 0..self.flash_until.len() {
            if self.flash_until[i].is_some_and(|t| now.duration_since(t) >= DIAL_FLASH) {
                self.flash_until[i] = None;
                self.dirty_panels[i] = true;
            }
        }
    }
}

/// Open the first Stream Deck found. Doesn't paint it: the daemon does, with its values.
// elgato-streamdeck hands out its reader via `Arc<Self>`; everything stays on this thread.
#[allow(clippy::arc_with_non_send_sync)]
pub fn open(hid: &mut HidApi) -> Option<Deck> {
    if let Err(e) = device::refresh_device_list(hid) {
        log::warn!("hidapi refresh: {e}");
        return None;
    }
    let (kind, serial) = device::list_devices(hid).into_iter().find(|(k, _)| k.is_visual())?;
    let dev = match StreamDeck::connect(hid, kind, &serial) {
        Ok(d) => Arc::new(d),
        Err(e) => {
            log::warn!("found {} but could not open it: {e} (is the udev rule installed?)", device::model_name(kind));
            return None;
        }
    };
    let reader = dev.get_reader();
    let info = DeckInfo::new(kind, &serial);
    let n_dials = info.dials as usize;
    let deck = Deck {
        dev,
        reader,
        kind,
        info,
        dial_pressed: vec![false; n_dials],
        flash_until: vec![None; n_dials],
        dirty_panels: vec![false; n_dials],
    };
    log::info!(
        "connected {} (serial {}, firmware {})",
        deck.info.model,
        serial,
        deck.dev.firmware_version().unwrap_or_default()
    );
    Some(deck)
}

/// What panel `i` shows now.
pub fn panel_status(d: &Deck, cfg: &Config, values: &Values, i: u8, now: Instant) -> DialStatus {
    probe::dial_status(cfg.dial(i), values, d.pressed(i, now))
}

/// Repaint everything: brightness, every key, every strip panel, one flush. Clears the
/// dirty panels, which it has just painted.
pub fn paint_all(
    d: &mut Deck,
    cfg: &Config,
    page: Option<&str>,
    renderer: &mut Renderer,
    values: &Values,
) -> Result<()> {
    d.dev.set_brightness(values.brightness)?;
    for i in 0..d.kind.key_count() {
        write_key(d, cfg, page, renderer, i, false)?;
    }
    // The touch strip, panel by panel (the same 200×100 writes a single-panel update
    // uses); it repaints on connect, reload and theme change, so follow_theme and hot
    // reload come for free. The first writes right after connect can be dropped by the
    // Plus's LCD, so the daemon repaints once more a moment later (POST_CONNECT_REPAINT).
    if d.info.strip.is_some() {
        let now = Instant::now();
        for i in 0..d.info.dials {
            write_panel(d, cfg, renderer, values, i, now)?;
        }
    }
    d.dirty_panels.fill(false);
    d.dev.flush()?;
    Ok(())
}

/// Paint what this pass dirtied, with one flush, and clear it. The only place panels are
/// painted outside `paint_all`. PR 8 adds `dirty_keys` here (and to `paint_all`'s clear).
pub fn flush_dirty(d: &mut Deck, cfg: &Config, renderer: &mut Renderer, values: &Values) {
    if d.info.strip.is_none() || !d.dirty_panels.contains(&true) {
        d.dirty_panels.fill(false);
        return;
    }
    let now = Instant::now();
    for i in 0..d.info.dials {
        if d.dirty_panels[i as usize]
            && let Err(e) = write_panel(d, cfg, renderer, values, i, now)
        {
            log::warn!("paint dial {i}: {e}");
        }
    }
    d.dirty_panels.fill(false);
    if let Err(e) = d.dev.flush() {
        log::warn!("flush: {e}");
    }
}

/// Paint one key at once (presses must show without waiting for the pass to end).
pub fn paint_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) {
    if let Err(e) = write_key(d, cfg, page, renderer, i, pressed).and_then(|_| Ok(d.dev.flush()?)) {
        log::warn!("paint key {i}: {e}");
    }
}

/// Queue one 200×100 dial panel for the next `flush`.
fn write_panel(d: &Deck, cfg: &Config, renderer: &mut Renderer, values: &Values, i: u8, now: Instant) -> Result<()> {
    let st = panel_status(d, cfg, values, i, now);
    let panel = renderer.render_strip_panel(cfg, i, d.info.dials as u32, st);
    let rect = ImageRect::from_image(image::DynamicImage::ImageRgba8(panel))?;
    d.dev.write_lcd(i as u16 * PANEL_W as u16, 0, &rect)?;
    Ok(())
}

fn write_key(d: &Deck, cfg: &Config, page: Option<&str>, renderer: &mut Renderer, i: u8, pressed: bool) -> Result<()> {
    let face = renderer.render(cfg.key(page, i), &cfg.style, d.info.key_size, pressed);
    d.dev.set_button_image(i, image::DynamicImage::ImageRgba8(face))?;
    Ok(())
}
```
3. `daemon.rs`, listing, **leaving out what Task 8 adds**: the `published` field (and its
   `HashMap::new()`, its two `clear()` calls), the `for i in 0..d.info.dials { … }` loop at
   the end of `flush` (keep its first line, the `flush_dirty` call), and the two
   `self.hub.publish(Event::Brightness { .. })` lines (in `new` and `set_brightness`).
   Remove `PanelStatus` from its imports for now.
```rust
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
use omadeck_core::{Action, AudioDevice, Config, LevelSource, PanelStatus, Renderer, action, tap_to_dial};

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
    pub reload_at: Option<Instant>,
    pub repaint_at: Option<Instant>,
}

impl Daemon {
    pub fn new(cfg: Config, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self {
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
            rt,
            deck: None,
            published: HashMap::new(),
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
        let _ = self.watcher.cmd.send(WatchCmd::Watch(Vec::new()));
        self.hub.publish(Event::Disconnected);
    }

    /// Tell the watcher what the deck's dials show. Sent on connect and on every reload.
    fn watch(&mut self) {
        let dials = self.deck.as_ref().map_or(0, |d| d.info.dials as usize);
        let probes = probe::watched_probes(&self.cfg, self.nav.current(), dials);
        // Keep what is still shown, so a reload's repaint draws it as it is.
        self.rt.values.probes.retain(|p, _| probes.contains(p));
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
        while let Ok((p, v)) = self.watcher.values.try_recv() {
            self.apply_value(p, v);
        }
    }

    /// The one place a value lands, from the watcher or a guess: store it and mark what
    /// shows it (today strip panels; PR 8 adds keys) for repainting at the end of the pass.
    pub fn apply_value(&mut self, probe: Probe, value: Value) {
        if let Some(d) = &mut self.deck {
            d.dirty_where(&self.cfg, |dial| probe::shows(dial, &probe));
        }
        self.rt.values.probes.insert(probe, value);
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

    /// Run `act` for an input. The one entry point for keys, dials, strip taps and
    /// flicks (and later ctl, multi-action steps and long presses): it logs, dispatches,
    /// moves the page if the action did, and reports a failure against its source.
    /// Returns whether the action ran (PR 8 flips a toggle only then).
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
        let Some(cur) = self.rt.values.level(device) else { return };
        let next = probe::guess_volume(cur, change, mute);
        self.apply_value(Probe::Volume(device), Value::Level(next));
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

/// The new start brightness when `new` changes the config's `brightness` (a hand edit,
/// or the configurator with no daemon), else `None`. (fix/review's, moved here.)
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
}
```
4. `main.rs`: the module list and imports, then `run()`, listing. Everything else that
   was in `main.rs` between `run()` and `snapshot` (`Deck`, `dial_status`, `connect`,
   `dispatch`, `do_brightness`, fix/review's `set_brightness` and
   `hand_edited_brightness` (its test moves to `daemon.rs`, in the listing),
   `after_volume`, `run_deck_action`, `show_page`, `paint_all`, `paint`, `write_panel`,
   `mark_dirty`, `flush_dirty`, `write_key`, the constants other than the three below)
   is deleted: it now lives in `deck.rs` and `daemon.rs`.
```rust
//! omadeckd — owns the Stream Deck: paints key faces, runs actions on press, follows
//! config and Omarchy theme changes live, and survives unplug/replug.

mod daemon;
mod deck;
mod hub;
mod nav;
mod saver;
mod watch;

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
// Only the trait's methods: `Watcher` here is the daemon's own (watch.rs).
use notify::{RecursiveMode, Watcher as _};
use omadeck_core::device;
use omadeck_core::paths::FileStamp;
use omadeck_core::probe::{self, Values};
use omadeck_core::state::State;
use omadeck_core::{Config, DeckInfo, Renderer, action, paths};

use daemon::Daemon;
use hub::Hub;

const SCAN_INTERVAL: Duration = Duration::from_secs(2);
const RELOAD_DEBOUNCE: Duration = Duration::from_millis(150);
/// How often to double-check the config by stat, for changes inotify can't see.
const STAMP_INTERVAL: Duration = Duration::from_secs(1);
```
```rust
fn run() -> Result<()> {
    let socket = paths::socket_path()?;
    if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
        anyhow::bail!("another omadeckd is already running ({})", socket.display());
    }

    let term = Arc::new(AtomicBool::new(false));
    for sig in [signal_hook::consts::SIGTERM, signal_hook::consts::SIGINT, signal_hook::consts::SIGHUP] {
        signal_hook::flag::register(sig, term.clone())?;
    }

    let hub = Hub::start(socket)?;

    // Watch the config dir and Omarchy's theme state for changes.
    std::fs::create_dir_all(paths::config_dir())?;
    let (tx, rx) = mpsc::channel::<()>();
    let mut fs_watch = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res
            && !matches!(ev.kind, notify::EventKind::Access(_))
            && ev.paths.iter().any(|p| relevant(p))
        {
            let _ = tx.send(());
        }
    })?;
    fs_watch.watch(&paths::config_dir(), RecursiveMode::NonRecursive)?;
    let mut dir_stamp = FileStamp::of(&paths::config_dir());
    let mut cfg_stamp = FileStamp::of(&paths::config_file());
    let mut last_stamp_check = Instant::now();
    let omarchy = paths::omarchy_current_dir();
    if omarchy.is_dir() {
        fs_watch.watch(&omarchy, RecursiveMode::NonRecursive)?;
    }
    if paths::icons_dir().is_dir() {
        let _ = fs_watch.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
    }

    let mut hid = device::new_hidapi().context("initialising hidapi")?;
    let mut daemon = Daemon::new(load_config(), hub, watch::start(), saver::start_saver());
    log::info!("theme: {}", daemon.renderer.theme.name);
    let mut last_scan: Option<Instant> = None;

    log::info!("omadeckd {} ready; config {}", env!("CARGO_PKG_VERSION"), paths::config_file().display());

    while !term.load(Ordering::Relaxed) {
        // Coalesce bursts of filesystem events into one reload.
        while rx.try_recv().is_ok() {
            daemon.reload_at = Some(Instant::now() + RELOAD_DEBOUNCE);
        }
        // inotify misses a symlinked config's target, and its watch dies with a config
        // directory that's deleted and recreated (restoring a backup), so also compare stamps.
        if last_stamp_check.elapsed() >= STAMP_INTERVAL {
            last_stamp_check = Instant::now();
            let d = FileStamp::of(&paths::config_dir());
            // The directory's mtime moves on every save; only its identity matters here.
            if !FileStamp::same_file(d, dir_stamp) {
                dir_stamp = d;
                let _ = fs_watch.unwatch(&paths::config_dir());
                if d.is_some() {
                    log::info!("config directory was replaced; watching the new one");
                    let _ = fs_watch.watch(&paths::config_dir(), RecursiveMode::NonRecursive);
                    if paths::icons_dir().is_dir() {
                        let _ = fs_watch.unwatch(&paths::icons_dir());
                        let _ = fs_watch.watch(&paths::icons_dir(), RecursiveMode::NonRecursive);
                    }
                }
            }
            let c = FileStamp::of(&paths::config_file());
            if c != cfg_stamp {
                cfg_stamp = c;
                daemon.reload_at.get_or_insert(Instant::now());
            }
        }
        if daemon.reload_at.is_some_and(|t| Instant::now() >= t) {
            daemon.reload_at = None;
            daemon.reload();
        }
        daemon.handle_commands();
        daemon.apply_values();

        if daemon.deck.is_none() {
            if last_scan.is_none_or(|t| t.elapsed() >= SCAN_INTERVAL) {
                last_scan = Some(Instant::now());
                match deck::open(&mut hid) {
                    Some(d) => daemon.attach(d),
                    None => daemon.hub.publish(omadeck_core::ipc::Event::Disconnected),
                }
            } else {
                std::thread::sleep(daemon::wait_for(Instant::now(), daemon.next_deadline()));
            }
            continue;
        }

        match daemon.pass() {
            Ok(()) => {}
            Err(_) if term.load(Ordering::Relaxed) => break,
            Err(e) => {
                daemon.detach(&e);
                last_scan = None;
            }
        }
    }

    daemon.shutdown();
    Ok(())
}
```
   The `snapshot` keeps Task 2's version.

Tests to copy (both in the `daemon.rs` listing): `wait_for_caps_and_rounds_up` and
`runtime_outlives_the_deck`.

**Check:** `cargo test -p omadeckd` → all pass, including `daemon::tests::*` (4) and
`nav`, `watch`, `saver`; clippy clean; `grep -c "fn " crates/omadeckd/src/main.rs` is
about 6 (`main`, `run`, `relevant`, `load_config`, `snapshot`, `draw_circle`). On the
deck: every check under "Hardware checks".

### Task 8: the hub never blocks; brightness and panels go to the configurator (~280 lines)

Files: `crates/omadeckd/src/hub.rs`, `daemon.rs`, `crates/omadeck-core/src/ipc.rs`,
`crates/omadeck-gui/src/app.rs`.

1. `ipc.rs`: `use crate::render::PanelStatus;` and two events at the end of `Event`:
   ```rust
   /// The panel brightness, 0–100: on connect and whenever it changes.
   Brightness {
       value: u8,
   },
   /// What a dial's strip panel shows: on connect and whenever it changes.
   DialStatus {
       index: u8,
       status: PanelStatus,
   },
   ```
2. `hub.rs`, listing:
```rust
//! Talks to configurator windows over the Unix socket: broadcasts daemon events to all of
//! them and collects the commands they send.

use std::collections::BTreeMap;
use std::io::Write;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use omadeck_core::ipc::{self, Command, Event};

/// Lines queued per client. A client this far behind loses events, never the deck loop's
/// time and never its connection.
const CLIENT_QUEUE: usize = 256;

pub struct Hub {
    path: PathBuf,
    inner: Arc<Mutex<Inner>>,
    commands: mpsc::Receiver<Command>,
}

struct Inner {
    /// One queue per client; its writer thread empties it into the socket.
    clients: Vec<SyncSender<Arc<str>>>,
    replay: Replay,
}

/// What a new client is told first, so it starts in sync: the latest of each kind of
/// state event. PR 8 adds `keys: BTreeMap<(Option<String>, u8), Event>` (`KeyState` per
/// page and key) next to `panels`, remembered and replayed the same way.
struct Replay {
    /// Last connect/disconnect event.
    status: Event,
    /// Last page change.
    page: Event,
    /// Last brightness.
    brightness: Option<Event>,
    /// Last `DialStatus` per panel; cleared on disconnect.
    panels: BTreeMap<u8, Event>,
}

impl Replay {
    fn remember(&mut self, ev: &Event) {
        match ev {
            Event::Connected { .. } => self.status = ev.clone(),
            Event::Disconnected => {
                self.status = ev.clone();
                self.panels.clear();
            }
            Event::PageChanged { .. } => self.page = ev.clone(),
            Event::Brightness { .. } => self.brightness = Some(ev.clone()),
            Event::DialStatus { index, .. } => {
                self.panels.insert(*index, ev.clone());
            }
            _ => {}
        }
    }

    /// The lines a new client is sent first, in this order.
    fn lines(&self) -> Vec<Arc<str>> {
        let mut out: Vec<Arc<str>> = vec![self.status.to_line().into(), self.page.to_line().into()];
        out.extend(self.brightness.iter().map(|e| e.to_line().into()));
        out.extend(self.panels.values().map(|e| e.to_line().into()));
        out
    }
}

impl Hub {
    pub fn start(path: PathBuf) -> Result<Self> {
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).with_context(|| format!("binding {}", path.display()))?;
        let inner = Arc::new(Mutex::new(Inner {
            clients: Vec::new(),
            replay: Replay {
                status: Event::Disconnected,
                page: Event::PageChanged { page: None },
                brightness: None,
                panels: BTreeMap::new(),
            },
        }));
        let (tx, commands) = mpsc::channel();
        let shared = inner.clone();
        std::thread::Builder::new().name("ipc".into()).spawn(move || {
            for stream in listener.incoming().flatten() {
                // The queue first: PR 11's reader sends replies into it.
                let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
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
                // Queued under the lock, so nothing published meanwhile can overtake it.
                for line in g.replay.lines() {
                    let _ = queue.try_send(line);
                }
                g.clients.push(queue);
                drop(g);
                spawn_writer(stream, lines);
            }
        })?;
        Ok(Self { path, inner, commands })
    }

    /// Send `ev` to every client. Never waits on a client: it only queues the line.
    pub fn publish(&self, ev: Event) {
        let line: Arc<str> = ev.to_line().into();
        let mut g = self.inner.lock().unwrap();
        g.replay.remember(&ev);
        g.clients.retain(|c| match c.try_send(line.clone()) {
            Ok(()) | Err(TrySendError::Full(_)) => true,
            // Its writer stopped: the client went away.
            Err(TrySendError::Disconnected(_)) => false,
        });
    }

    /// Commands received since the last call.
    pub fn commands(&self) -> mpsc::TryIter<'_, Command> {
        self.commands.try_iter()
    }

    pub fn shutdown(self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// A client's writer: the only place that writes to its socket, so a client that stops
/// reading stalls this thread alone. Ends when a write fails (the client closed).
fn spawn_writer(mut stream: UnixStream, lines: mpsc::Receiver<Arc<str>>) {
    let _ = std::thread::Builder::new().name("ipc-write".into()).spawn(move || {
        for line in lines {
            if stream.write_all(line.as_bytes()).is_err() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::time::{Duration, Instant};

    fn socket(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("omadeck-{name}-{}.sock", std::process::id()))
    }

    #[test]
    fn new_client_gets_the_replay() {
        let path = socket("hub-replay");
        let hub = Hub::start(path.clone()).unwrap();
        hub.publish(Event::Brightness { value: 40 });
        hub.publish(Event::DialStatus { index: 1, status: Default::default() });
        let stream = UnixStream::connect(&path).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let lines: Vec<String> = BufReader::new(stream).lines().take(4).map(|l| l.unwrap()).collect();
        assert_eq!(
            lines,
            vec![
                r#"{"event":"disconnected"}"#,
                r#"{"event":"page_changed","page":null}"#,
                r#"{"event":"brightness","value":40}"#,
                r#"{"event":"dial_status","index":1,"status":{}}"#,
            ]
        );
        hub.shutdown();
    }

    #[test]
    fn a_client_that_never_reads_does_not_block_publish() {
        let path = socket("hub-stuck");
        let hub = Hub::start(path.clone()).unwrap();
        let _stuck = UnixStream::connect(&path).unwrap(); // connected, never read
        std::thread::sleep(Duration::from_millis(100)); // let the hub accept it
        let start = Instant::now();
        for i in 0..20_000 {
            hub.publish(Event::DialTurn { index: 0, delta: (i % 2) as i8 });
        }
        assert!(start.elapsed() < Duration::from_secs(1), "publish waited on the client: {:?}", start.elapsed());
        hub.shutdown();
    }
}
```
3. `daemon.rs`: add what Task 7 left out (the `published` field, its clears, the loop at
   the end of `flush`, the two `Event::Brightness` publishes), as in Task 7's listing.
4. GUI `App::handle`: the `match` is exhaustive, so add the new events to the ignored arm
   for now (Task 9 handles them): `| Event::Brightness { .. } | Event::DialStatus { .. }`.

Tests to copy: `a_client_that_never_reads_does_not_block_publish` (in the listing), and in
`ipc.rs`:
```rust
#[test]
fn state_events_roundtrip_and_wire_shape() {
    roundtrip(&Event::Brightness { value: 40 });
    roundtrip(&Event::DialStatus { index: 1, status: PanelStatus::default() });
    let level = Some(crate::Level { value: 0.5, text: "50%".into(), muted: false });
    roundtrip(&Event::DialStatus { index: 2, status: PanelStatus { level } });
    assert_eq!(Event::Brightness { value: 40 }.to_line(), "{\"event\":\"brightness\",\"value\":40}\n");
    assert_eq!(
        Event::DialStatus { index: 1, status: PanelStatus::default() }.to_line(),
        "{\"event\":\"dial_status\",\"index\":1,\"status\":{}}\n"
    );
}
```

**Check:** `cargo test -p omadeckd hub::` → `test result: ok. 2 passed` (these weren't run
when this plan was written; if `new_client_gets_the_replay` times out, the hub thread hadn't
accepted yet: the replay is queued at accept, so it should still arrive; look at the
order of the lines first); `cargo test -p omadeck-core ipc` all pass; clippy clean. With
the daemon running: `socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/omadeck.sock` prints
`connected`, `page_changed`, `brightness` and one `dial_status` per panel, then a
`dial_status` line per volume change.

### Task 9: GUI: mirror the daemon, no poller (~180 lines)

File: `crates/omadeck-gui/src/app.rs`. Omarchy machine only.

1. Fields: delete `strip_levels`; add after `locked`:
   ```rust
   /// What the daemon last said each strip panel shows (`Event::DialStatus`). Empty with
   /// no daemon or no deck; the preview then builds panels itself.
   panel_status: RefCell<std::collections::HashMap<u8, PanelStatus>>,
   /// The daemon's live brightness (`Event::Brightness`); `None` with no daemon.
   live_brightness: Cell<Option<u8>>,
   /// The settings slider, so `Event::Brightness` can move it.
   brightness_scale: RefCell<Option<gtk::Scale>>,
   /// Set while the slider is moved from code, so it doesn't send the value back.
   syncing_brightness: Cell<bool>,
   /// When the user last moved the slider: a late echo mustn't pull it back mid-drag.
   slider_moved: Cell<Option<std::time::Instant>>,
   ```
   with `RefCell::default()` / `Cell::new(None)` / `Cell::new(false)` in the `Self { … }`
   literal. A constant at the top: `const SLIDER_HOLD: Duration = Duration::from_millis(500);`
2. Delete `listen_levels` and its call in `build`. Remove imports that are now unused
   (`AudioDevice`, `Level`, `LevelSource` in `app.rs`, if nothing else there uses them;
   clippy says which) and add `PanelStatus` and `probe::{self, Values}`.
3. `refresh_strip`:
   ```rust
   /// Redraw the strip picture: one panel per dial, the same picture the daemon paints.
   pub fn refresh_strip(&self) {
       let Some((w, h)) = self.deck.borrow().strip else { return };
       let width = self.strip_width();
       if width <= 0 {
           return;
       }
       let cfg = self.config.borrow();
       let n = self.deck.borrow().dials;
       // Panels the daemon described show what the deck shows. The rest (no daemon, or
       // it hasn't said yet) are built here: brightness from the daemon's value or the
       // config's, volume as a dash. Only the daemon reads the system.
       let values = Values { brightness: self.live_brightness.get().unwrap_or(cfg.brightness), ..Values::default() };
       let mirrored = self.panel_status.borrow();
       let status: Vec<DialStatus> = (0..n)
           .map(|i| match mirrored.get(&i) {
               Some(panel) => DialStatus { pressed: false, panel: panel.clone() },
               None => probe::dial_status(cfg.dial(i), &values, false),
           })
           .collect();
       let img = self.renderer.borrow_mut().render_strip(&cfg, &status, (width as u32, width as u32 * h / w));
       self.strip.set_paintable(Some(&media::texture(img)));
   }
   ```
   (Keep the render size fix/review left, if it changed the last argument.)
4. `settings_popover`: the slider starts at `self.live_brightness.get().unwrap_or(cfg.brightness)`
   and its handler becomes:
   ```rust
   scale.connect_value_changed(move |s| {
       let Some(app) = weak.upgrade() else { return };
       if app.syncing_brightness.get() {
           return;
       }
       let value = s.value().round() as u8;
       app.slider_moved.set(Some(std::time::Instant::now()));
       if app.daemon_up.get() {
           // The daemon owns the brightness: it applies and saves it, and its
           // `Event::Brightness` updates the preview. config.toml isn't touched.
           app.send(DaemonCommand::SetBrightness { value });
       } else {
           // No daemon: the config's value is the start value, as upstream.
           app.config.borrow_mut().brightness = value;
           app.schedule_save();
           app.refresh_strip();
       }
   });
   *self.brightness_scale.borrow_mut() = Some(scale.clone());
   ```
5. A helper next to `refresh_strip`:
   ```rust
   /// Move the brightness slider to `value` without sending it back, unless the user
   /// moved it a moment ago (then this is the echo of an older value).
   fn sync_slider(&self, value: u8) {
       if self.slider_moved.get().is_some_and(|t| t.elapsed() < SLIDER_HOLD) {
           return;
       }
       // Cloned out, so no borrow is held while GTK runs the handler.
       let scale = self.brightness_scale.borrow().clone();
       if let Some(scale) = scale {
           self.syncing_brightness.set(true);
           scale.set_value(value as f64);
           self.syncing_brightness.set(false);
       }
   }
   ```
6. `handle`: take the two events out of the ignored arm and add:
   ```rust
   Msg::Event(Event::Brightness { value }) => {
       self.live_brightness.set(Some(value));
       self.sync_slider(value);
       self.refresh_strip();
   }
   Msg::Event(Event::DialStatus { index, status }) => {
       self.panel_status.borrow_mut().insert(index, status);
       self.refresh_strip();
   }
   ```
   `Disconnected` also forgets the panels (the deck that showed them is gone):
   ```rust
   Msg::Event(Event::Disconnected) => {
       self.connected.set(false);
       self.panel_status.borrow_mut().clear();
       self.refresh_strip();
   }
   ```
   and `Msg::DaemonDown`, after its existing lines:
   ```rust
   self.live_brightness.set(None);
   self.panel_status.borrow_mut().clear();
   let b = self.config.borrow().brightness;
   self.sync_slider(b);
   self.refresh_strip();
   ```

No GUI-free test: `dial_status` is tested in core. **Check:** `cargo clippy --all-targets
-- -D warnings` clean; `grep -n "strip_levels\|listen_levels\|audio::volume"
crates/omadeck-gui/src/*.rs` → nothing. In the app:
- [ ] Turn the volume dial: the preview's bar follows within a pass; `pgrep -fa wpctl`
      while the app is open and the daemon stopped shows nothing.
- [ ] Turn the brightness dial: the slider and the preview follow. Drag the slider: the
      deck follows, the slider doesn't jump back, and `config.toml` doesn't change
      (`stat -c %Y ~/.config/omadeck/config.toml` before and after).
- [ ] Stop the daemon: volume panels show a dash, brightness the config's value; the
      slider then edits `brightness` in `config.toml`.

### Task 10: GUI: `ActionSlot`, `ActionTarget`, and one ▶ button (~300 lines)

Files: `crates/omadeck-gui/src/app.rs`, `editor.rs`. Omarchy machine only.

1. `app.rs`, after `enum Target`:
   ```rust
   /// Which action an editor edits, for work that finishes later (a picker, a file
   /// dialog, a fetch): the result lands on this action even if the window has moved on.
   /// PR 8 adds `KeyOn(KeyRef)`, PR 12 `Step(Box<ActionTarget>, usize)`, PR 16
   /// `DialLongPress(u8)`.
   #[derive(Clone)]
   pub enum ActionTarget {
       /// A key's action.
       Key(KeyRef),
       DialTurnLeft(u8),
       DialTurnRight(u8),
       DialPress(u8),
   }

   impl ActionTarget {
       /// The target for dial `i`'s slot `dir`.
       pub fn dial(i: u8, dir: Direction) -> Self {
           match dir {
               Direction::Left => ActionTarget::DialTurnLeft(i),
               Direction::Right => ActionTarget::DialTurnRight(i),
               Direction::Press => ActionTarget::DialPress(i),
           }
       }

       /// The dial and slot, for a dial's action.
       pub fn dial_slot(&self) -> Option<(u8, Direction)> {
           match self {
               ActionTarget::Key(_) => None,
               ActionTarget::DialTurnLeft(i) => Some((*i, Direction::Left)),
               ActionTarget::DialTurnRight(i) => Some((*i, Direction::Right)),
               ActionTarget::DialPress(i) => Some((*i, Direction::Press)),
           }
       }
   }
   ```
   and in `impl App`, after `edit_dial`:
   ```rust
   /// The action `at` names; `None` if its page has gone.
   pub fn action_at(&self, at: &ActionTarget) -> Option<Action> {
       let cfg = self.config.borrow();
       match at.dial_slot() {
           Some((i, dir)) => Some(cfg.dial(i).map(|d| d.action(dir).clone()).unwrap_or_default()),
           None => {
               let ActionTarget::Key(k) = at else { return None };
               cfg.page(k.page()).map(|keys| keys.get(k.index).map(|kc| kc.action.clone()).unwrap_or_default())
           }
       }
   }

   /// Replace the action `at` names. False when it can't: config.toml is broken (both
   /// edit paths drop late results then) or the key's page has gone.
   pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool {
       match at.dial_slot() {
           Some((i, dir)) => {
               let ok = self.locked.borrow().is_none();
               self.edit_dial(i, |d| *d.action_mut(dir) = act);
               ok
           }
           None => match at {
               ActionTarget::Key(k) => self.edit_ref(k, |kc| kc.action = act),
               _ => false,
           },
       }
   }
   ```
   Import `Direction` from `omadeck_core`.
2. `editor.rs`, the slot (above `action_editor`; `FaviconHandler` now takes an
   `ActionTarget`):
   ```rust
   /// Handles a fetched website icon: (app, target, imported icon path, site name).
   type FaviconHandler = Rc<dyn Fn(&Rc<App>, ActionTarget, String, String) + 'static>;

   /// One action an editor edits: a key's action, one of a dial's three, and later a
   /// toggle's on action (PR 8), a multi-action step (PR 12) or a long press (PR 16).
   /// Every action editor takes one, so editors nest: PR 12's step editor builds a slot
   /// for a step and calls `action_editor` again.
   #[derive(Clone)]
   pub(crate) struct ActionSlot {
       /// Where the action lives. Late results write through `App::set_action_at`.
       pub target: ActionTarget,
       /// The dial slot it is, for the defaults a kind pick starts from; `None` for a key.
       pub dir: Option<Direction>,
       /// Read the action now.
       pub get: Rc<dyn Fn(&Rc<App>) -> Action>,
       /// Replace the action (the rows' own edits).
       pub set: Rc<dyn Fn(&Rc<App>, Action)>,
       /// The page an "Open page" list leaves out: a key's own page; `None` for dials.
       pub own_page: Option<String>,
       /// Apply an app pick: (app, target, command, name, icon).
       pub on_app_picked: Rc<dyn Fn(&Rc<App>, &ActionTarget, String, String, Option<String>)>,
       /// Make the page a "New page" button points the action at; returns its name.
       pub on_new_page: Rc<dyn Fn(&Rc<App>) -> String>,
       /// Handle a fetched website icon; `None` hides the Fetch row.
       pub on_favicon: Option<FaviconHandler>,
   }

   impl ActionSlot {
       /// A dial slot: reads and writes through its target and only changes the action.
       pub fn dial(i: u8, dir: Direction) -> Self {
           let target = ActionTarget::dial(i, dir);
           let (t1, t2) = (target.clone(), target.clone());
           ActionSlot {
               target,
               dir: Some(dir),
               get: Rc::new(move |a| a.action_at(&t1).unwrap_or_default()),
               set: Rc::new(move |a, act| {
                   a.set_action_at(&t2, act);
               }),
               own_page: None,
               // Only the slot's action changes: the dial's label is shared by its slots.
               on_app_picked: Rc::new(|a, at, command, _, _| {
                   a.set_action_at(at, Action::App { command });
               }),
               on_new_page: Rc::new(move |a| {
                   let label = a.config.borrow().dial(i).map(|d| d.label.clone()).unwrap_or_default();
                   a.add_page(if label.trim().is_empty() { "Page" } else { &label }, true)
               }),
               on_favicon: None,
           }
       }

       /// The action a kind pick starts from: the slot's direction for a dial (a left
       /// turn lowers, a press mutes), the empty action for a key.
       pub fn default_action(&self, kind: usize) -> Action {
           match self.dir {
               Some(dir) => Action::default_for(kind, dir),
               None => Action::empty_of_kind(kind),
           }
       }
   }
   ```
   (`add_page(name, true)`: fix/review's folder flag; a page made from an Open-page action
   is a folder.)
3. `test_button` replaces the two inline ▶ buttons (in `build_key` and
   `dial_action_group`) and their copies of the deck-owned list:
   ```rust
   /// The ▶ "Try it now" button for a slot. Off for an empty action and for actions only
   /// the deck runs (`Action::is_deck_owned`), except Open page, which shows the page here.
   fn test_button(app: &Rc<App>, slot: &ActionSlot) -> gtk::Button {
       let act = (slot.get)(app);
       let (tip, sensitive) = match &act {
           Action::Page { page } => ("Try it now".to_string(), app.config.borrow().has_page(page)),
           a if a.is_deck_owned() => (format!("{} only works on the deck", Action::KINDS[a.kind_index()]), false),
           a => ("Try it now".to_string(), !a.is_none()),
       };
       let test = gtk::Button::builder()
           .icon_name("media-playback-start-symbolic")
           .tooltip_text(tip)
           .valign(gtk::Align::Center)
           .sensitive(sensitive)
           .css_classes(["flat"])
           .build();
       let a = app.clone();
       let get = slot.get.clone();
       test.connect_clicked(move |_| {
           let act = get(&a);
           if let Action::Page { page } = act {
               a.show_page(Some(page), true);
           } else if let Err(e) = action::run(&act) {
               a.toast(&format!("{e:#}"));
           }
       });
       test
   }
   ```
4. `action_editor(app, group, combo_title, slot: &ActionSlot)` and
   `action_rows(app, group, slot: &ActionSlot)` replace the generic parameter lists (and
   lose `#[allow(clippy::too_many_arguments)]`). The kind combo:
   ```rust
   let action = (slot.get)(app);
   // … kind_row as before …
   let a = app.clone();
   kind_row.connect_selected_notify({
       let slot = slot.clone();
       move |row| {
           let sel = row.selected() as usize;
           if sel != (slot.get)(&a).kind_index() {
               (slot.set)(&a, slot.default_action(sel));
               a.rebuild_editor();
           }
       }
   });
   group.add(&kind_row);
   action_rows(app, group, slot);
   ```
   In `action_rows`, mechanically: each closure that cloned `get`/`edit`/`target`/… now
   does `let slot = slot.clone();` and calls `(slot.get)(&a)` and `(slot.set)(&a, act)`.
   Specific rows:
   - App pick: `let at = slot.target.clone();` before `apps::pick`, then
     `(slot.on_app_picked)(&a2, &at, command, name, icon);`.
   - Script and Sound browse: `browse_button` loses its `target` parameter, and its
     `on_file` becomes `impl Fn(&Rc<App>, std::path::PathBuf) + Clone + 'static` (it is
     only used here). The callers capture the target:
     ```rust
     row.add_suffix(&browse_button(app, "Choose a script", None, {
         let at = slot.target.clone();
         move |a, p| {
             a.set_action_at(&at, Action::Script { path: paths::portable(&p), terminal: t });
         }
     }));
     ```
     and the same for Sound (`Action::Sound { file: paths::portable(&p), volume: v }`).
     Inside `browse_button`, drop `let at = target(&a);` and call `on_file(&a, p)`.
   - Url Fetch: `if let Some(on_favicon) = slot.on_favicon.clone()`, and
     `let at = slot.target.clone();` where it took `target(&a)`.
   - Open page: fix/review's `own` computed from `target(app)` becomes
     `let own = slot.own_page.clone();`. "New page" calls `(slot.on_new_page)(&a)`.
   - Brightness Mode, back to "change": `_ => slot.default_action(Action::Brightness { change: 0, set: None }.kind_index()),`
     (fix/review's `default_for` parameter goes; the slot has it).
5. `build_key`: build the slot first, then the ▶ button from it, then
   `action_editor(app, &group, "When pressed", &slot)`:
   ```rust
   let slot = ActionSlot {
       target: ActionTarget::Key(app.key_ref(i)),
       dir: None,
       get: Rc::new(move |a| a.key(i).map(|k| k.action).unwrap_or_default()),
       set: Rc::new(move |a, act| {
           // … the body of the old `edit` closure, unchanged (the Open page → new folder
           // rule, then `a.edit(i, |k| k.action = action)`) …
       }),
       own_page: app.current_page(),
       on_app_picked: Rc::new(|a, at, command, name, icon| {
           if let ActionTarget::Key(k) = at {
               a.edit_ref(k, |k| {
                   k.action = Action::App { command };
                   k.label = name;
                   if icon.is_some() {
                       k.icon = icon;
                   }
               });
           }
       }),
       on_new_page: Rc::new(move |a| new_folder(a, i)),
       on_favicon: Some(Rc::new(move |a, at, icon, name| {
           if let ActionTarget::Key(k) = at {
               a.edit_ref(&k, |k| {
                   k.icon = Some(icon);
                   if k.label.is_empty() {
                       k.label = name;
                   }
               });
           }
       })),
   };
   buttons.append(&test_button(app, &slot));
   ```
   The old `edit_at` closure goes (async writes use `set_action_at`).
6. `dial_action_group(app, i, dir: Direction)`:
   ```rust
   /// One dial slot: a "Turn left" / "Turn right" / "Press" group with a ▶ test button.
   fn dial_action_group(app: &Rc<App>, i: u8, dir: Direction) -> adw::PreferencesGroup {
       let title = match dir {
           Direction::Left => "Turn left",
           Direction::Right => "Turn right",
           Direction::Press => "Press",
       };
       let group = adw::PreferencesGroup::builder().title(title).build();
       let slot = ActionSlot::dial(i, dir);
       group.set_header_suffix(Some(&test_button(app, &slot)));
       action_editor(app, &group, "Action", &slot);
       group
   }
   ```
   and in `build_dial`:
   ```rust
   // A kind pick starts from what the slot implies: turning left lowers or goes back, a
   // Volume press mutes (`Action::default_for`).
   for dir in [Direction::Left, Direction::Right, Direction::Press] {
       page.add(&dial_action_group(app, i, dir));
   }
   ```

No GUI-free test: the core parts (`default_for`, `is_deck_owned`, `DialConfig::action`)
are tested there. **Check:** `cargo clippy --all-targets -- -D warnings` clean (no
`too_many_arguments` allow left in `editor.rs`); `grep -n "PageStep { .. } | Action::Brightness"
crates/omadeck-gui/src/editor.rs` → nothing. In the app, every row still works:
- [ ] Key: each kind's rows edit and save; Choose… app sets the label and icon; Fetch sets
      the icon; Script/Sound browse land on the key even after switching page while the
      dialog is open.
- [ ] Dial: Turn left Volume reads −5, Press Volume is "Toggle mute"; Choose… on a dial
      changes only that slot; browse lands on the right slot.
- [ ] ▶ is off for Go back, Next/previous page and Brightness, with the tooltip
      "<kind> only works on the deck"; on for Open page when the page exists.
- [ ] Brightness Mode "Set" then back to "Change per turn" on Turn left: −5.

## Hardware checks (Stream Deck +, `0fd9:0084`, Omarchy 4), after Task 8

The PR 4 and PR 6 checks again, since their code moved:
- [ ] Turn = ±5 % per detent; a fast spin runs folded commands (`RUST_LOG=info omadeckd`
      shows `exec ["omarchy-audio-output-volume", "+10"]`-style lines), capped at 100 %.
- [ ] Omarchy's volume pop-up appears on turn and on press (mute), on the focused monitor.
- [ ] Press toggles mute; the panel shows "Muted" and an empty bar within a pass.
- [ ] Volume changed with the keyboard keys or `pavucontrol` shows on the panel within
      about a second.
- [ ] Mic dial: turn ±5 % with the mic pop-up showing the **new** level, press mutes.
- [ ] Spin hard for two seconds and stop: the bar stops with the knob and doesn't snap
      back (D7); `wpctl get-volume @DEFAULT_AUDIO_SINK@` agrees with the panel.
- [ ] Brightness dial dims and brightens live; the panel shows the percent.
- [ ] `systemctl --user restart omadeckd` right after a brightness turn: it comes back at
      the new value.
- [ ] Unplug and replug: brightness and the volume bars come back as they were at once
      (the bars without a dash).
- [ ] Edit `brightness = 30` in `config.toml`: the deck dims, the configurator's slider
      moves to 30.
- [ ] A strip tap flashes the title for about 150 ms (not up to 250 ms); a held dial stays
      highlighted.
- [ ] A flick changes page; Open page / Back keys still work; a dial on Next/previous page
      steps.
- [ ] Freeze the configurator (`kill -STOP $(pgrep -x omadeck)`), spin a dial for ten
      seconds: the deck stays responsive. `kill -CONT` it: it carries on (it may show a
      stale level until the next change).
- [ ] Idle CPU of `omadeckd` with no volume dials: `top -p $(pgrep omadeckd)` ~0 %, and
      `strace -f -p $(pgrep omadeckd) -e trace=clock_nanosleep,futex` shows no 50 ms tick.
- [ ] Stop the service: the deck returns to its logo, and the stop takes under a second
      (`time systemctl --user stop omadeckd`).

## Decisions to explain (for the PR description)

- **Why a `Daemon` struct.** Upstream's loop keeps its state in locals; with dials, the
  strip, brightness and the watcher, `run()` had grown to ~350 lines and helpers to eight
  arguments. One struct with methods keeps upstream's single-threaded loop and makes every
  input go through one `run_input`.
- **`Runtime` vs `Deck`.** What a replug must keep (brightness, last values) is separate
  from the USB handle, so a replug is just "open the device, paint".
- **One watcher thread, not one per source.** The audio worker becomes the watcher; later
  sources (state commands, clocks, app volumes) add match arms, not threads. It sleeps
  until something is due instead of ticking.
- **Volume changes run on the watcher.** Two dial events no longer race (`wpctl
  set-volume 5%+` is read-modify-write), and the read after a change sees it.
- **Everything we wait on has a 2 s timeout** (`proc::output_timeout`), so a hung
  `pactl` or Hyprland can't freeze the watcher or the loop.
- **The configurator mirrors the daemon.** It no longer reads audio itself (six process
  spawns a second on Omarchy 4), and its brightness slider no longer rewrites
  `config.toml` for a value the daemon ignored.
- **The hub never blocks the deck.** A per-client writer thread with a bounded queue;
  a stuck configurator loses events, not the deck's responsiveness.
- **The wire gains two events** (`brightness`, `dial_status`); older configurators ignore
  unknown events, so the protocol stays compatible.
- **Deck-only actions say so.** "Try it now" for one says "<kind> only works on the deck",
  from one list (`is_deck_owned`), the same one `action::run` refuses.
- **Background images aren't stat'ed on paint**; a reload (which every config save
  triggers) re-checks them.

## After the branch

- Hand PR 8 the names above: `apply_value`, `watched_probes`, `Poller::handle/run_due/record`,
  `Replay`, `deck::flush_dirty` (it adds `dirty_keys` there and in `paint_all`),
  `run_input -> bool`, `Runtime: Default`, `ActionTarget`, `ActionSlot`.
- Note in PR 8's plan: `Daemon::show_page` must call `self.watch()` once key probes exist
  (today's set doesn't depend on the page).

## Verification notes (2026-10-17, hardware pass on the Stream Deck +)

Two places where the implemented design misbehaved and was fixed on the branch after
the ten task commits:

1. **The volume bar snapped back on a fast spin.** The plan's guess (last known level +
   change) reached the end of the spin at once, while the sequential ~150 ms commands
   caught up one read-back at a time — each read-back, reflecting only what had
   finished, visibly pulled the bar back down. (fix/review's parallel worker + one
   debounced re-read had hidden this.) Fixed: the watcher's read-backs now carry the
   change they confirm, and the daemon keeps an in-flight total per device (a turn
   adds to it, a read-back takes its part off); the bar shows the read plus what is
   still in flight, so it only ever moves forward while a spin is in flight. The total
   is clamped like the watcher's coalesce, so the two stay in step. `settle_volume`
   is unit-tested.
2. **A config `brightness` edited while the daemon was stopped was ignored at start.**
   The state file's docs promise a hand edit wins again, but that only happened on
   reload. The settings slider with no daemon writes `config.toml` (the no-daemon
   fallback), and a start afterwards came back at the saved value. Fixed: at start,
   the config wins when its mtime is newer than the state file's (`start_brightness`).

All hardware checks passed: ±5 per detent, folded fast spins (no snap-back), mute +
"Muted" panel, keyboard volume on the panel in ~1 s, mic dial, brightness dial live,
restart after a turn (new value), unplug/replug (values back), hand-edited config
brightness (deck + slider follow), tap flash + held highlight (title to accent),
flicks and Open page/Back, frozen GUI (deck responsive, `kill -STOP`), stop under a
second back to the logo, idle 0.1 % CPU with the volume dial watched.
