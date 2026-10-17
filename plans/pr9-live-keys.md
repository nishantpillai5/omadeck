# PR 9: `feat/live-keys`: keys and panels that update themselves

Part of [PLAN.md](PLAN.md). Roadmap item: "**Live keys** that show a clock, CPU usage or
the active workspace". Base branch: `fix/review-2` ([review-fixes-2.md](review-fixes-2.md)),
which sits on `feat/toggle-keys` (PR 8, `55d0d62`). This PR reuses groundwork's **state
watcher** (core `probe.rs`, daemon `watch.rs`) and PR 8's key status, and it owns the
**Hyprland event listener** (`omadeckd/src/hypr.rs`), which PR 10 (focused app volume)
and PR 15 (pages follow the focused app) reuse.

**Status (2026-10-17): built, not verified.** Built on the Mac and pushed to the fork as
`feat/live-keys`: 837c11e … ab0df79 (8 commits, tasks A–H). Core and daemon tests, clippy and fmt pass; the GUI type-checks
(not linked or run; GUI clippy warns, BUGS.md 78), and nothing was run on the deck or with the captures below. Where the code
differs from this plan, the code wins: the deviations, the defaults taken in place of
captures, and every check still to do are in its `feat/live-keys` section of [VERIFY.md](VERIFY.md). The review
of what was built is [review-2026-10-17c.md](review-2026-10-17c.md).

**Rewritten 2026-10-17 (second time)** against the code as merged on `feat/toggle-keys`
(`55d0d62`) plus what `fix/review-2` delivers (R1–R9 in "What this PR builds on"). Every
name below is copied from the code. Before Task A, run the `grep`s in "Check before
Task A" on the `fix/review-2` tip. If a name differs, fix this plan first; don't
reconcile in code.

**Outcome:** any key, and any dial's strip panel, can show a live value: the time or date
(strftime format), CPU %, memory %, battery % (with charging), the active Hyprland
workspace, or the first line of a command's output, re-run at an interval. On a key the
value is drawn the way Elgato's plugins set a key's title. On the strip it uses Elgato's
`$A1` (text) or `$B1` (text and bar) layout. A live key still runs its action on press.
Nothing polls faster than it needs to: the clock wakes on the minute (or second)
boundary, the workspace is event-driven, and only the keys and panels whose value changed
are repainted. The configurator shows the same values, mirrored from daemon events; it
never reads them itself. Old configs load and save byte-identical.

## How the Elgato app does it (researched 2026-10-16)

- **No built-in clock or system monitor.** Both are free Marketplace plugins (absence
  confirmed from docs and search only: unconfirmed against a current install).
- **CPU** (`com.elgato.cpu`, open source, <https://github.com/elgatosf/streamdeck-cpu>):
  one key action. A static image, `"TitleAlignment": "middle"`, `"FontSize": "17"`; a
  **1000 ms** timer calls `SetTitle(value + "%")`. So the live value is just the key's
  **title** over the icon. Press opens Task Manager / Activity Monitor.
- **Clocks** plugin
  (<https://www.elgato.com/us/en/explorer/products/stream-deck/clocks-plugin-for-stream-deck-track-multiple-time-zones-at-a-glance/>):
  many analog and digital styles, 12/24 h, date and weekday; on the Plus a dial cycles
  time zones. Whether Elgato wrote it: unconfirmed.
- **User title wins** (<https://docs.elgato.com/streamdeck/sdk/guides/keys>):
  `setTitle`/`setImage` only take effect "when the user has not specified a custom title".
- **Strip** (<https://docs.elgato.com/streamdeck/sdk/guides/dials>): `setFeedback` on a
  built-in layout. `$A1` = title `[16,10,136,24]`, icon `[16,40,48,48]`, value
  `[76,40,108,32]` 24 pt right aligned, **no bar**. `$B1` = the same plus the bar
  `[76,74,108,12]`, which PR 4 draws.
- Workspace and window state: no Elgato equivalent.

## What this PR builds on

All of it is in the tree at `55d0d62`, except the lines marked **R1–R9**, which
`fix/review-2` adds (see [review-fixes-2.md](review-fixes-2.md)).

**Core `probe.rs`:**

```rust
pub enum Probe {                       // #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    Volume(AudioDevice),
    Command { command: String, interval_ms: u32 },
}
impl Probe {
    pub fn interval(&self) -> Duration;               // Volume 1 s; Command max(interval_ms, 1 s)
    pub fn refresh_after(&self) -> &'static [Duration]; // VOLUME_REFRESH / COMMAND_REFRESH
}
pub enum Toggle { Off, On, Busy(String) }             // #[serde(rename_all = "snake_case")]
pub enum Value { Level(Level), Toggle(Toggle) }       // R4: #[serde(rename_all = "snake_case")]
pub struct Values { pub probes: HashMap<Probe, Value>, pub brightness: u8 }
impl Values { pub fn level(&self, device: AudioDevice) -> Option<&Level>; }
pub fn dial_status(dial: Option<&DialConfig>, values: &Values, pressed: bool) -> DialStatus;
pub fn shows(dial: Option<&DialConfig>, probe: &Probe) -> bool;
pub fn key_status(key: Option<&KeyConfig>, values: &Values, pressed: bool) -> KeyStatus;
pub fn toggle_from_exit(code: Option<i32>, first_line: Option<String>) -> Toggle;
pub fn is_missing(code: Option<i32>) -> bool;
pub fn progress(text: &str) -> Option<u8>;             // R7: the first explicit `N%` only
pub fn watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize, n_keys: u8) -> Vec<Probe>;
                                                       // R9: keys at or past n_keys aren't polled
pub const FAST_INTERVAL: Duration; pub const FAST_WINDOW: Duration; pub const SETTLE: Duration;
```

**Core `render.rs`** (exported from `lib.rs`: `Align, DialStatus, Face, KeyStatus, Level,
PANEL_W, PanelStatus, Renderer, tap_to_dial`):

```rust
pub struct Face<'a> { pub label: &'a str, pub show_label: bool, pub icon: Option<&'a str>,
    pub glyph: Option<&'a str>, pub background: Option<&'a str>,
    pub background_image: Option<&'a str>, pub label_color: Option<&'a str> }   // no derives
#[derive(Debug, Clone, Default)]
pub struct DialStatus { pub pressed: bool, pub panel: PanelStatus }
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}
pub struct Level { pub value: f32, pub text: String, pub muted: bool }
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KeyStatus {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pressed: bool,
    #[serde(default, skip_serializing_if = "crate::probe::Toggle::is_off")]
    pub state: crate::probe::Toggle,
}
impl Renderer {
    pub fn render(&mut self, key: Option<&KeyConfig>, style: &Style, size: u32, st: &KeyStatus) -> RgbaImage;
    fn render_face(&mut self, face: &Face, style: &Style, size: u32, bg: Rgb) -> RgbaImage;
    pub fn render_strip_panel(&mut self, cfg: &Config, i: u8, panels: u32, st: DialStatus) -> RgbaImage;
    fn brightness_icon(&mut self, face: &Face, color: Rgb) -> Option<RgbaImage>;
    fn draw_b1(&self, img: &mut RgbaImage, face: &Face, colors: B1Colors, want_label: bool,
               icon: Option<RgbaImage>, level: Option<&Level>);
    fn glyph(&self, text: &str, box_px: u32, color: Rgb) -> Option<RgbaImage>;
    fn draw_line(&self, img: &mut RgbaImage, text: &str, px: f32, cy: f32, color: Rgb, shadow: bool);
    fn draw_block(&self, img: &mut RgbaImage, text: &str, color: Rgb);
}
// private: B1Colors { title, value, fill, track } (R8: a press recolours `title` only),
// B1_ICON, B1_VALUE, B1_BAR, X1_TITLE, STRIP_ICON_PX, BRIGHTNESS_GLYPH, BAR_ZONE, wrap().
```

`render` has **two paths** that draw the face: the busy-bar path (`render_face` at
`inner = s·(1 − BAR_ZONE)`, then `draw_bar`) and the plain path (`render_face` at full
size). The face is `k.face_for(&st.state)` (R1: `face_for(On)` with no `[key.on]` returns
the off face). Test helpers in `render::tests`: `test_style()`, `renderer_with_font()` (R9: it is
`Renderer::with_font_bytes(&test_style(), TEST_FONT)`), `golden(img, name)`, `in_rect(..)`,
`volume_dial()`, `level(v, muted)`. R9: `Renderer::with_font_bytes(style: &Style, data:
&[u8]) -> Renderer` and `omadeck_core::render::TEST_FONT: &[u8]` are public under
`#[cfg(any(test, feature = "test-support"))]`; the daemon and the GUI take
`omadeck-core = { workspace = true, features = ["test-support"] }` as a dev-dependency. Fixture font: printable ASCII plus `–` (U+2013) and `…`; test texts
use `A`. Goldens: `tests/fixtures/strip-{empty,x1,b1}.png`.

**Core `config.rs`:** `KeyConfig` ends `…, busy_while_running, action, on: Option<OnFace>,
busy: Option<BusyFace>`; `KeyConfig::new(index)`, `is_blank()`, `face()`, `face_for(&Toggle)`,
`state_probe() -> Option<Probe>`, `action_for(on: bool) -> &Action`. `OnFace.interval: u16`
(seconds, `two`/`is_two`). `DialConfig` ends `…, turn_left, turn_right, press`;
`DialConfig::new(index)`, `is_blank()`, `face()`, `level_source() -> Option<LevelSource>`,
`volume_device()`. `Config::parse(&str)`, `Config::key(page, i)`, `Config::dial(i)`,
`Config::dial_mut(i)`, `Keys::get_mut(i)`. R2: one walker, `Action::walk`/`walk_mut`, under
`KeyConfig::actions(&self) -> Vec<&Action>`, `DialConfig::actions`, `Config::all_actions`
(and their `_mut` forms, which take `f: &mut dyn FnMut(&mut Action)`);
`Direction::ALL: [Direction; 3]`. This PR adds no action, so it doesn't touch them.

**Core `proc.rs`:** `output_timeout(cmd, timeout) -> Option<Output>`, `Job::{spawn, started,
lines, try_finish, kill_group}`; output lines are trimmed, split on `\n`/`\r`, empty lines
skipped, cut to 64 chars.

**Core `ipc.rs`:** `Event::DialStatus { index: u8, status: PanelStatus }` and
`Event::KeyState { page: Option<String>, index: u8, status: KeyStatus }`, both replayed to
new clients by the hub's `Replay` (a default `KeyStatus` removes its entry).

**Daemon `watch.rs`:**

```rust
pub enum WatchCmd {
    Watch(Vec<Probe>),
    Refresh(Probe),
    Volume { device: AudioDevice, change: i32, mute: bool },   // R5: always acknowledged
    Leave { probe: Probe, from: Toggle },
    Done { probe: Probe, run: u64, result: RunResult },         // R5: `run` is run_due's run id
    Stop,
}
pub enum RunResult { Exit { code: Option<i32>, first_line: Option<String> }, TimedOut, SpawnFailed(String) }
const MAX_IN_FLIGHT: usize = 8;
const STATE_TIMEOUT: Duration = 5 s (300 ms under cfg(test));
// R5: `value` is `None` only for a volume change that wasn't read back (it still confirms
// its change).
pub struct Watcher { pub cmd: mpsc::Sender<WatchCmd>, pub values: mpsc::Receiver<(Probe, Option<Value>, i32)>, thread }
impl Watcher { pub fn stop(self); }
struct Poller { watched, last, next_due, forced, refresh_at,
    in_flight: HashMap<Probe, (u64, Arc<AtomicBool>)>,   // R5: (run id, cancel)
    next_run: u64,                                      // R5
    fast_until, failing,
    read_volume: fn(AudioDevice) -> Option<Value>, change_volume: fn(AudioDevice, i32, bool) }
impl Default for Poller { … }   // by hand: it fills the two fn fields
impl Poller {
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>);
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>) -> Vec<(Probe, u64, Arc<AtomicBool>)>;
                                                  // R5: (probe, run id, cancel) per command to start
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>, applied: i32);
                                                  // pushes (p, Some(v), applied)
    fn next_due(&self) -> Option<Instant>;
    fn warn_once(&mut self, p: &Probe, what: &str);
    fn cancel_all(&mut self);
}
fn state_command(command: &str) -> Command;      // sh -c, LC_ALL=C, cwd $HOME
fn run_state(command: &str, timeout: Duration, cancel: &AtomicBool) -> RunResult;
pub fn start() -> Watcher;
fn run(cmd_rx, val_tx, poller, runner_tx);      // the thread loop
// tests: `Poller::run_of(&self, p: &Probe) -> u64` (R5), the id a test's `Done` passes:
// `p.handle(WatchCmd::Done { probe: probe.clone(), run: p.run_of(&probe), result }, now, &mut out)`.
```

`run_due` reads `Probe::Volume` inline (through `read_volume`) and returns the
`Probe::Command`s to start; `run` gives each a `watch-run` thread that calls
`run_state(&command, STATE_TIMEOUT, &cancel)` and sends `WatchCmd::Done { probe, run,
result }` back on `runner_tx` (R5: a runner that fails to spawn takes its probe out of
`in_flight` and reschedules it). `record` schedules the next read as `now + FAST_INTERVAL` (busy or in a
`Leave` window) else `now + p.interval()`, and sends only changes (or forced reads).
`Volume` changes run inline on the watcher thread, each followed by a read.

**Daemon `daemon.rs`:**

```rust
pub struct KeyId { pub page: Option<String>, pub index: u8 }
#[derive(Debug, Default)]
pub struct Runtime { pub values: Values, pub state: State, pub flipped: HashSet<KeyId>,
    settle: HashMap<Probe, Settle>, pub jobs: HashMap<KeyId, Running> }
impl Runtime { pub fn start(state: State, cfg_brightness: Option<u8>) -> Runtime; }   // R3 (Runtime::load is gone)
pub fn key_state(cfg: &Config, rt: &Runtime, page: Option<&str>, i: u8, pressed: bool) -> KeyStatus;
fn finish_job(rt: &mut Runtime, cfg: &Config, id: &KeyId, status: ExitStatus) -> JobEnd;
pub struct Daemon { pub cfg, pub renderer, pub nav, pub wanted_page, pub hub, pub watcher,
    pub saver,
    pub hypr: HyprWorker,                  // R6: the Hyprland dispatch worker (hypr_worker.rs)
    pub rt: Runtime, pub deck: Option<Deck>, published: HashMap<u8, PanelStatus>,
    published_keys: HashMap<KeyId, KeyStatus>, in_flight: HashMap<AudioDevice, i32>,
    pub reload_at, pub repaint_at }
impl Daemon {
    pub fn new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self;   // R3
    pub fn attach(&mut self, d: Deck); pub fn detach(&mut self, e: &StreamDeckError);
                                           // R5: both clear published_keys
    fn watch(&mut self);                   // probe::watched_probes(.., dials, self.key_count()) → WatchCmd::Watch
    pub fn reload(&mut self);              // → keep_toggle_state → config_probes
    pub fn apply_values(&mut self);
    pub fn apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32);
                                           // R5: settles in_flight first; `None` stops there
    fn store_value(&mut self, probe: Probe, value: Value);   // insert + mark_probe
    fn mark_probe(&mut self, probe: &Probe);                // dials via probe::shows, keys via state_probe
    fn mark_key(&mut self, page: Option<&str>, i: u8);
    fn key_count(&self) -> u8;
    fn publish_keys(&mut self);            // in flush; candidates: on.is_some() || busy_while_running
    fn keep_toggle_state(&mut self);
    fn press_key(&mut self, i: u8);
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool;
    fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>;   // R6: takes src
    fn report_failure(&self, src: Source, page: Option<String>, message: String);   // R6
    pub fn poll_jobs(&mut self);
    pub fn poll_hypr(&mut self);           // R6: publishes the worker's failures
    pub fn flush(&mut self);               // publish_keys, flush_dirty, DialStatus by comparison
}
fn config_probes(cfg: &Config, n_dials: usize) -> Vec<Probe>;   // values a reload keeps
```

**Daemon `main.rs` `run()`** each iteration (as `fix/review-2` leaves it): drain notify →
1 s stamp check → `reload()` when due → `handle_commands()` → `apply_values()` →
`poll_jobs()` → `poll_hypr()` → no deck: `flush()`, then every 2 s `deck::open` → `attach`
(nothing is published on a miss), else sleep `wait_for(now, next_deadline())`; deck:
`pass()`, `Err` → `detach`. This PR adds `apply_hypr()` right after `apply_values()`.
The loop blocks in the HID read for up to `READ_TIMEOUT` (100 ms), so a
watcher value reaches the deck within ~100 ms.

**GUI:** `App.key_states: RefCell<HashMap<(Option<String>, u8), KeyStatus>>` and
`App.panel_status: RefCell<HashMap<u8, PanelStatus>>`, filled from `KeyState` /
`DialStatus`. `refresh_key(i)` renders the mirrored status (the selected key uses
`preview_status(i)`); `refresh_strip()` renders mirrored panels, else
`probe::dial_status(cfg.dial(i), &values, false)`. Edits: `a.key(i)` (a clone),
`a.edit(i, f)`, `a.edit_dial(i, f)`, `a.rebuild_editor()`. `build_key` builds the key
group, the Toggle group, then Appearance; `build_dial` builds the dial group, the three
`dial_action_group`s, then Appearance.

**R6:** every Hyprland *dispatch* goes to one ordered background worker in
`fix/review-2`: module `omadeckd/src/hypr_worker.rs`, `HyprWorker` (thread `hypr`), held in
`Daemon.hypr`, its failures drained by `Daemon::poll_hypr`. That worker sends commands;
this PR's listener only reads events. They are separate modules and threads: the
listener is `omadeckd/src/hypr.rs`, its thread is `hypr-events`, and its receiver is
`Daemon.hypr_events` (not `hypr`, which the worker has).

### Check before Task A

```bash
git checkout fix/review-2
grep -n "pub enum Probe\|pub enum Value\|rename_all\|pub fn watched_probes" crates/omadeck-core/src/probe.rs
grep -n "Done {\|fn run_due\|fn run_of\|in_flight:\|pub values" crates/omadeckd/src/watch.rs
grep -n "test-support\|pub fn with_font_bytes\|pub const TEST_FONT" crates/omadeck-core/Cargo.toml crates/omadeck-core/src/render.rs
grep -n "struct B1Colors" -A6 crates/omadeck-core/src/render.rs
grep -n "pub hypr\|pub fn new\|pub fn apply_value\|pub fn poll_hypr\|fn finish_job" crates/omadeckd/src/daemon.rs
grep -n "mod hypr_worker\|poll_hypr" crates/omadeckd/src/main.rs
```

They must show: `Done { probe: Probe, run: u64, result: RunResult }`; `run_due` returning
`Vec<(Probe, u64, Arc<AtomicBool>)>`; `values: mpsc::Receiver<(Probe, Option<Value>, i32)>`;
`watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize, n_keys: u8)`;
`pub hypr: HyprWorker`; `apply_value(&mut self, probe: Probe, value: Option<Value>,
applied: i32)`; `daemon.poll_hypr()` in `run()`. If any differs, fix this plan first.

## Config (`omadeck-core/src/config.rs`)

A new optional `live` table on `[[key]]` and `[[dial]]`. It says what the face *shows*,
separate from the `action` it *does*.

```rust
/// A value a key or a dial's strip panel shows and keeps up to date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Live {
    /// Local time and date, strftime format (jiff's strtime: %H %M %S %a %d %b %n …).
    Clock {
        #[serde(default = "default_clock_format", skip_serializing_if = "is_default_clock_format")]
        format: String,
    },
    /// Average over all cores, %.
    Cpu,
    /// (MemTotal − MemAvailable) / MemTotal, %.
    Memory,
    /// System battery %, and whether it is charging.
    Battery,
    /// Hyprland's active workspace name.
    Workspace,
    /// The first line of a command's stdout (`sh -c`), re-run every `interval` seconds.
    Command {
        command: String,
        #[serde(default = "default_live_interval", skip_serializing_if = "is_default_live_interval")]
        interval: u16,
    },
}

fn default_clock_format() -> String {
    "%H:%M".into()
}
fn is_default_clock_format(f: &String) -> bool {
    f == "%H:%M"
}
fn default_live_interval() -> u16 {
    5
}
fn is_default_live_interval(n: &u16) -> bool {
    *n == 5
}

impl Live {
    /// For the configurator's "Shows" combo; index 0 is "no live value".
    pub const KINDS: [&'static str; 7] =
        ["Nothing", "Clock", "CPU", "Memory", "Battery", "Active workspace", "Command output"];
    /// None → 0, Clock → 1 … Command → 6.
    pub fn kind_index(live: Option<&Live>) -> usize;
    /// 0 → None; 1 → Clock { "%H:%M" }; 2 → Cpu; … 6 → Command { "", 5 }.
    pub fn of_kind(i: usize) -> Option<Live>;
    /// Whether the strip panel draws `$B1` (with a bar) rather than `$A1`.
    pub fn has_bar(&self) -> bool;   // Cpu | Memory | Battery
    /// The probe that feeds it.
    pub fn probe(&self) -> Probe;
}
```

`Live::probe`: `Clock { format }` → `Probe::Clock { format: format.clone() }`; `Cpu`,
`Memory`, `Battery`, `Workspace` → the same-named probe; `Command { command, interval }` →
`Probe::Output { command: command.clone(), interval_ms: interval.max(1) as u32 * 1000 }`
(the same form as `state_probe`'s `on.interval.max(1) as u32 * 1000`). The clamp happens
here, not on load, so a hand-written `0` saves as written.

- `KeyConfig.live` and `DialConfig.live`: `Option<Live>`, the **last** field (after
  `busy`, and after `press`), `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  TOML writes tables after plain values, and `on`/`busy` are tables already, so `live`
  goes after them. `KeyConfig::new` and `DialConfig::new` set `None`. Both `is_blank()`s
  also require `self.live.is_none()`.
- `DialConfig::level_source()` returns `None` when `live` is set: an explicit live value
  wins over the volume or brightness value the dial's actions imply. Add
  `if self.live.is_some() { return None; }` as its first line.
- `pub use config::Live;` in `lib.rs`.

```toml
[[key]]
index = 7
label = "CPU"                # drawn as a small caption above the value
action = { type = "app", command = "omarchy launch-or-focus-tui btop" }
[key.live]
source = "cpu"

[[key]]
index = 4
[key.live]
source = "clock"
format = "%H:%M%n%a %d"      # time, and the day under it

[[key]]
index = 6
[key.live]
source = "command"
command = "curl -s 'wttr.in/?format=%t'"
interval = 900

[[dial]]
index = 3
label = "Battery"
[dial.live]
source = "battery"
```

No new `Action` variant and no `Action::KINDS` change: a live key's press runs its
ordinary `action` (or `on.action` while on).

## Probes and values (`omadeck-core/src/probe.rs`)

New variants, appended after the existing ones:

```rust
pub enum Probe {
    Volume(AudioDevice),
    Command { command: String, interval_ms: u32 },
    /// Local time in this strftime format.
    Clock { format: String },
    Cpu,
    Memory,
    Battery,
    /// Fed by the Hyprland listener, never polled.
    Workspace,
    /// `sh -c command`'s first output line, every `interval_ms`. Both fields are the hash
    /// key, as for `Command`.
    Output { command: String, interval_ms: u32 },
}

pub enum Value {
    Level(Level),
    Toggle(Toggle),
    /// A live text: the clock, the workspace, a command's line.
    Text(String),
    /// 0–100: CPU, memory.
    Percent(u8),
    Battery { percent: u8, charging: bool },
}
```

`Probe::interval()` arms: `Clock { .. }` → `live::CLOCK_CAP` (only a fallback; the
watcher schedules clocks on the boundary), `Cpu` → 1 s, `Memory` → 2 s, `Battery` → 5 s,
`Workspace` → 1 s (never used), `Output { interval_ms, .. }` → the same as `Command`
(`max(interval_ms, 1000)`). `Probe::refresh_after()`: `Output { .. }` → `&COMMAND_REFRESH`;
`Clock`, `Cpu`, `Memory`, `Battery`, `Workspace` → `&[]`. Both stay exhaustive matches.

```rust
impl Probe {
    /// False for probes something else feeds (the Hyprland listener): `watched_probes`
    /// leaves them out.
    pub fn is_polled(&self) -> bool {
        !matches!(self, Probe::Workspace)
    }
}

/// What the live key or panel for `live` shows now; `None` until a value arrives.
pub fn live_show(live: &Live, values: &Values) -> Option<LiveShow> {
    match values.probes.get(&live.probe())? {
        Value::Text(s) => Some(LiveShow { text: s.clone(), bar: None, charging: false }),
        Value::Percent(p) => Some(LiveShow { text: format!("{p}%"), bar: Some(*p as f32 / 100.0), charging: false }),
        Value::Battery { percent, charging } => {
            Some(LiveShow { text: format!("{percent}%"), bar: Some(*percent as f32 / 100.0), charging: *charging })
        }
        Value::Level(_) | Value::Toggle(_) => None,
    }
}
```

`Values::level`'s `Value::Toggle(_) => None` arm becomes `_ => None`.

`LiveShow` lives in `render.rs`, next to `Level`, because the renderer draws it and the
events carry it:

```rust
/// What a live key or strip panel shows (PR 9).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LiveShow {
    /// "14:05", "42%", "3"; may hold `\n` (a clock's `%n`).
    pub text: String,
    /// 0.0..=1.0 for the strip's `$B1` bar; `None` for text sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar: Option<f32>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub charging: bool,
}
```

- `KeyStatus` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub live:
  Option<LiveShow>` (last field). `probe::key_status` fills it:
  `live: key.and_then(|k| k.live.as_ref()).and_then(|l| live_show(l, values))`.
- `PanelStatus` gains the same field after `level`. `dial_status` fills it from `dial.live`
  the same way; `level` stays `None` for a live dial (because `level_source()` is `None`).
- `pub use render::LiveShow;` in `lib.rs` (add it to the existing `render::{..}` list).
- No new event: `live` rides inside `status` on `Event::KeyState` and `Event::DialStatus`.
  The daemon's `flush` publishes them by comparison, as today. Update `Event::KeyState`'s
  doc comment to "What a key with a toggle, a running command or a live value shows now".

## Clock and system readers (core, pure where possible)

New `omadeck-core/src/live.rs`:

```rust
//! Clock formatting and scheduling for live keys (PR 9).

use std::time::Duration;

/// Never sleep longer than this before re-checking the clock. Monotonic sleeps stop
/// during suspend, so an uncapped minute sleep would leave a stale time after resume.
pub const CLOCK_CAP: Duration = Duration::from_secs(5);

/// `now` formatted with `format`; "?" for a bad format.
pub fn format_clock(format: &str, now: &jiff::Zoned) -> String {
    jiff::fmt::strtime::format(format, now).unwrap_or_else(|_| "?".into())
}

/// The local time now, formatted. The watcher and the configurator's example row call
/// this, so neither needs jiff itself.
pub fn clock_text(format: &str) -> String {
    format_clock(format, &jiff::Zoned::now())
}

/// Unix time now, in milliseconds.
pub fn wall_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// Whether `format` shows seconds: %S %T %s %r %X %c %+ (not `%%S`).
pub fn needs_seconds(format: &str) -> bool;

/// Time from `now_ms` (Unix milliseconds) to just after the next minute (or second)
/// boundary, plus 10 ms so the format runs after the rollover; capped at `CLOCK_CAP`.
pub fn next_tick(now_ms: i64, seconds: bool) -> Duration {
    let period: i64 = if seconds { 1_000 } else { 60_000 };
    let to_next = period - now_ms.rem_euclid(period);
    Duration::from_millis((to_next + 10) as u64).min(CLOCK_CAP)
}
```

`needs_seconds` walks the chars: on `%`, look at the next char; `%%` skips both; any of
`S T s r X c +` returns true.

New `omadeck-core/src/sysinfo.rs` (no `sysinfo` crate):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes {
    pub busy: u64,
    pub total: u64,
}

/// The `cpu ` line of /proc/stat: user nice system idle iowait irq softirq steal [guest
/// guest_nice]. total = the first eight summed (guest is already inside user);
/// busy = total − idle − iowait.
pub fn parse_proc_stat(stat: &str) -> Option<CpuTimes>;
/// Δbusy·100 / Δtotal, rounded; 0 when Δtotal is 0.
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> u8;

/// Keeps the previous sample. The first sample only primes it (returns None).
#[derive(Debug, Default)]
pub struct CpuMeter {
    prev: Option<CpuTimes>,
}
impl CpuMeter {
    pub fn sample(&mut self, stat: &str) -> Option<u8>;
}

/// (MemTotal − MemAvailable) · 100 / MemTotal, rounded; None if either is missing.
pub fn parse_meminfo(meminfo: &str) -> Option<u8>;

/// Battery % and charging from a power_supply directory. Entries with `type` = Battery
/// and no `scope` = Device (that's a mouse or headset). Several batteries combine as
/// Σenergy_now / Σenergy_full (else Σcharge_now / Σcharge_full), else the mean
/// `capacity`. charging = any `status` is "Charging". No battery → None.
pub fn battery_in(root: &Path) -> Option<(u8, bool)>;
pub fn battery() -> Option<(u8, bool)> {
    battery_in(Path::new("/sys/class/power_supply"))
}
```

Read each sysfs file with `fs::read_to_string(..).ok()` and `trim()`.

**jiff.** `jiff 0.2.37` is already in `Cargo.lock` (through `env_logger`). Core adds
`jiff = { version = "0.2", default-features = false, features = ["std", "tz-system",
"tzdb-zoneinfo"] }`. Only core depends on jiff; the daemon and the GUI call
`live::clock_text` and `live::wall_ms`. That's a go/no-go in Task A: after adding it,
`git diff --stat Cargo.lock` must show only `Cargo.lock | 1 +` (the new `"jiff",` line
under `omadeck-core`). If any `[[package]]` is added, take the fallback instead:
`libc::localtime_r` + `libc::strftime` (`libc` is already a core dependency), with
`format_clock(format: &str, unix_secs: i64) -> String` and `wall_ms` from
`SystemTime::now()`.

## Watcher additions (`omadeckd/src/watch.rs`)

| Probe | Read | Next read |
|---|---|---|
| `Clock { format }` | `Value::Text(live::clock_text(format))`, inline | `live::next_tick(live::wall_ms(), live::needs_seconds(format))` |
| `Cpu` | `/proc/stat` → `cpu.sample(..)` → `Value::Percent` | 1 s |
| `Memory` | `/proc/meminfo` → `parse_meminfo` → `Value::Percent` | 2 s |
| `Battery` | `sysinfo::battery()` → `Value::Battery` | 5 s |
| `Output { command, .. }` | a runner thread (PR 8's path) | its interval, after the result |
| `Workspace` | never (not in the watched set) | – |

- **Inline reads, with a seam like `read_volume`.** `Poller` gains
  `cpu: sysinfo::CpuMeter` and `read_live: fn(&Probe, &mut sysinfo::CpuMeter) ->
  Option<Value>`, both set in `impl Default for Poller` (`CpuMeter::default()`,
  `read_live`). The free function:

  ```rust
  /// Read a built-in live source. A clock and three small file reads: cheap enough for
  /// the watcher thread.
  fn read_live(p: &Probe, cpu: &mut sysinfo::CpuMeter) -> Option<Value> {
      match p {
          Probe::Clock { format } => Some(Value::Text(live::clock_text(format))),
          Probe::Cpu => cpu.sample(&std::fs::read_to_string("/proc/stat").ok()?).map(Value::Percent),
          Probe::Memory => sysinfo::parse_meminfo(&std::fs::read_to_string("/proc/meminfo").ok()?).map(Value::Percent),
          Probe::Battery => sysinfo::battery().map(|(percent, charging)| Value::Battery { percent, charging }),
          _ => None,
      }
  }
  ```

- `run_due`'s match gains `Probe::Clock { .. } | Probe::Cpu | Probe::Memory |
  Probe::Battery => match (self.read_live)(&p, &mut self.cpu) { Some(v) => self.record(p,
  v, now, out, 0), None => { self.next_due.insert(p.clone(), now + p.interval()); } }`,
  next to the `Probe::Volume` arm. `None` sends nothing, as for volume: the CPU's first
  sample only primes, so the key shows `–` for one second.
- `Probe::Workspace => {}` in `run_due` (it's never watched; the arm keeps the match
  exhaustive).
- `handle(Watch(..))`: when the new set has no `Probe::Cpu`, reset
  `self.cpu = CpuMeter::default()`, so the next first read primes again.
- **Scheduling.** Pull it into a pure function, and have `record` call it in its non-fast
  case (where it wrote `now + p.interval()`):

  ```rust
  /// When `probe` should be read next, after a read at `now` (`wall_ms`: Unix ms).
  fn next_read(probe: &Probe, now: Instant, wall_ms: i64) -> Instant {
      match probe {
          Probe::Clock { format } => now + live::next_tick(wall_ms, live::needs_seconds(format)),
          _ => now + probe.interval(),
      }
  }
  ```

  `record` becomes `let next = if fast { now + FAST_INTERVAL } else { next_read(&p, now,
  live::wall_ms()) }; self.next_due.insert(p.clone(), next);`.
- **Command output goes through PR 8's runner path.** A command must never hold up the
  clock or the other probes. No new `WatchCmd` variant and no second runner:
  - `run_due`: the `Probe::Command { .. }` arm becomes `Probe::Command { .. } |
    Probe::Output { .. }` (same `MAX_IN_FLIGHT` check, same `in_flight` entry with R5's
    run id). A due probe already in flight is skipped, so never two runs at once.
  - `run` (the thread loop): the `let Probe::Command { command, .. } = &probe else
    { continue };` line becomes a match that also picks the timeout, and the runner calls
    `run_state(&command, timeout, &cancel)`:

    ```rust
    let (command, timeout) = match &probe {
        Probe::Command { command, .. } => (command.clone(), STATE_TIMEOUT),
        Probe::Output { command, .. } => (command.clone(), probe.interval().min(LIVE_COMMAND_TIMEOUT)),
        _ => continue,
    };
    ```

    `probe.interval()` is at least 1 s, so the timeout is 1–10 s.
  - `/// Upper bound for one live command run.`
    `const LIVE_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);`
  - `handle`'s `Done` arm, right after R5's stale-run check (the `in_flight` removal that
    returns for a run that isn't current): answer a `Probe::Output` here and return,
    before PR 8's toggle mapping:

    ```rust
    if matches!(probe, Probe::Output { .. }) {
        match result {
            RunResult::Exit { code, first_line } => {
                if is_missing(code) {
                    self.warn_once(&probe, "live command not found or not executable");
                } else {
                    self.failing.remove(&probe);
                }
                // Any exit status counts; `–` when it printed nothing.
                let text = first_line.unwrap_or_else(|| "–".into());
                self.record(probe, Value::Text(text), now, out, 0);
            }
            RunResult::TimedOut | RunResult::SpawnFailed(_) => {
                // Keep the last value; try again next interval.
                self.warn_once(&probe, "live command failed or timed out");
                self.next_due.insert(probe.clone(), now + probe.interval());
            }
        }
        return;
    }
    ```
- A `Done` for a probe no longer watched is dropped (the existing check).
- **The clock can wait behind a volume command (decided, documented).** `Volume` changes
  and reads run inline on this thread. A clock read due while one runs waits for it:
  normally under 100 ms, at most ~2 s per call on a hung PipeWire (`COMMAND_TIMEOUT`).
  The minute still turns over; it may be late by that much once. No extra thread. Add a
  sentence saying so to the module doc comment (it already promises "PR 9 adds clocks
  and meters").

## Hyprland event listener (`omadeckd/src/hypr.rs`): this PR owns it

Facts (Hyprland wiki, `hyprwm/hyprland-wiki` `content/ipc/_index.md`, read 2026-10-17):
- Request socket `$XDG_RUNTIME_DIR/hypr/$HIS/.socket.sock`: write `[flags]/command args`
  (so `j/activeworkspace` is JSON). "Hyprland evaluates connections to this socket
  completely synchronously … any unclosed connections will cause Hyprland to freeze until
  the five-second timeout": open, write, read, close at once.
- Event socket `.socket2.sock`: lines `EVENT>>DATA\n`. Used here:
  `workspacev2>>WORKSPACEID,WORKSPACENAME` (sent "ONLY when a user requests a workspace
  change, and is not emitted on mouse movements (see `focusedmon`)"),
  `focusedmon>>MONNAME,WORKSPACENAME`, `renameworkspace>>WORKSPACEID,NEWNAME`,
  `activewindowv2>>WINDOWADDRESS`, `windowtitlev2>>WINDOWADDRESS,WINDOWTITLE`,
  `configreloaded>>` (empty).
- Not on the wiki, so captured first (below): the address form in events vs JSON (`0x`
  prefix or not), what `activewindowv2` sends with no focused window, and the JSON of
  `j/activeworkspace` / `j/activewindow`.

```rust
//! Hyprland's event socket, on its own thread. Live keys use the active workspace;
//! app volume (PR 10) and app pages (PR 15) use the focused window.

/// The focused window, as `j/activewindow` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWindow {
    /// Without a `0x` prefix, lowercase, so event and JSON forms compare equal.
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprEvent {
    /// The focused workspace changed, moved monitor, or was renamed.
    Workspace { id: i64, name: String },
    /// The focused window changed or was retitled; `None` when nothing has focus.
    ActiveWindow(Option<ActiveWindow>),
}

/// Starts the `hypr-events` thread. `None` outside Hyprland (no thread).
pub fn start() -> Option<mpsc::Receiver<HyprEvent>>;

/// What one event line asks for. Pure.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Workspace { id: i64, name: String },
    QueryWorkspace, // focusedmon, renameworkspace
    QueryWindow,    // activewindowv2 with an address
    NoWindow,       // activewindowv2 with no address
    Title { address: String, title: String },
    Reseed, // configreloaded
}

/// What the thread last sent, so it sends only changes.
#[derive(Debug, Default)]
struct Seen {
    workspace: Option<(i64, String)>,
    window: Option<ActiveWindow>,
}

fn parse(line: &str) -> Option<Parsed>;
/// Turns a parsed line into the events to send, running `query` (the request socket)
/// when the line needs it. Pure but for `query`, so tests pass a closure.
fn handle(p: Parsed, seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent>;
/// The events for a fresh connection: both queries, through `handle`'s dedup.
fn seed(seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent>;
fn parse_workspace_json(json: &str) -> Option<(i64, String)>; // {"id","name"}
fn parse_window_json(json: &str) -> Option<ActiveWindow>;     // {} or no "address" → None
fn normalise_address(a: &str) -> String;                       // trim, strip "0x", lowercase
/// The instance dir: `runtime/hypr/<sig>/` if it has `.socket2.sock`, else the newest
/// `runtime/hypr/*/` that has one.
fn find_instance(runtime: &Path, sig: Option<&str>) -> Option<PathBuf>;
/// One request on `.socket.sock`: connect, write, read to EOF, close. 1 s timeouts.
fn request(dir: &Path, cmd: &str) -> Option<String>;
```

`parse`: split at the first `>>`; the data is split with `splitn(2, ',')`, so names and
titles may contain commas.

| Line | `Parsed` |
|---|---|
| `workspacev2>>ID,NAME` | `Workspace { id, name }` (`id` must parse as `i64`, else `None`) |
| `focusedmon>>…`, `renameworkspace>>…` | `QueryWorkspace` |
| `activewindowv2>>ADDR` | `QueryWindow`; empty or `,` → `NoWindow` |
| `windowtitlev2>>ADDR,TITLE` | `Title { address: normalise_address(ADDR), title }` |
| `configreloaded>>` | `Reseed` |
| anything else (`workspace>>`, `activewindow>>`, `focusedmonv2>>`, `activespecial*`, …) | `None` |

`handle`: `Workspace` → send if `seen.workspace` differs. `QueryWorkspace` →
`query("j/activeworkspace")` → `parse_workspace_json` → as `Workspace`. `QueryWindow` →
`query("j/activewindow")` → `parse_window_json` → send `ActiveWindow(w)` if it differs
from `seen.window`. `NoWindow` → `ActiveWindow(None)` if `seen.window` was `Some`.
`Title` → only if it matches `seen.window`'s address: a copy with the new title. `Reseed`
→ `seed`. The `activewindow>>CLASS,TITLE` line is not needed: the query gives class, title
and pid at once.

The thread (`name("hypr-events")`; `hypr` is R6's dispatch worker):

```rust
const MAX_BACKOFF: Duration = Duration::from_secs(30);

fn run(tx: mpsc::Sender<HyprEvent>, runtime: PathBuf) {
    let mut backoff = Duration::from_secs(1);
    let mut seen = Seen::default();
    loop {
        let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok();
        if let Some(dir) = find_instance(&runtime, sig.as_deref())
            && let Ok(stream) = UnixStream::connect(dir.join(".socket2.sock"))
        {
            let started = Instant::now();
            let mut query = |c: &str| request(&dir, c);
            for ev in seed(&mut seen, &mut query) {
                if tx.send(ev).is_err() {
                    return;
                }
            }
            let mut reader = BufReader::new(stream);
            let mut buf = Vec::new();
            loop {
                buf.clear();
                match reader.read_until(b'\n', &mut buf) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {}
                }
                // Titles can hold invalid UTF-8; `lines()` would end the connection.
                let line = String::from_utf8_lossy(&buf);
                let Some(p) = parse(line.trim_end_matches('\n')) else { continue };
                for ev in handle(p, &mut seen, &mut query) {
                    if tx.send(ev).is_err() {
                        return;
                    }
                }
            }
            if started.elapsed() > Duration::from_secs(10) {
                backoff = Duration::from_secs(1);
            }
            log::info!("Hyprland event socket closed; reconnecting");
        }
        thread::sleep(backoff);
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}
```

- `start()`: `runtime = $XDG_RUNTIME_DIR`. If `HYPRLAND_INSTANCE_SIGNATURE` is unset
  **and** `runtime/hypr` doesn't exist, log once at debug and return `None`. Otherwise
  spawn the thread and return the receiver. The fallback in `find_instance` covers a
  service started without the variable, and a Hyprland restart (new signature, stale env).
  The pre-0.40 `/tmp/hypr/` location is not supported.
- `serde_json` is added to `omadeckd`'s `[dependencies]` (`serde_json.workspace = true`);
  it isn't there today.
- `Seen` survives a reconnect, so a reconnect resends only what changed. That's why the
  daemon must never drop the workspace value (below).

**In the daemon (Task F):**
- `Daemon` gains `hypr_events: Option<mpsc::Receiver<hypr::HyprEvent>>`, set by
  `hypr::start()` in `Daemon::new` (no test builds a `Daemon`). Not `hypr`: that field is
  R6's `HyprWorker`.
- `Runtime` gains `pub window: Option<hypr::ActiveWindow>` (it still derives `Default`).
- `Daemon::apply_hypr(&mut self)` drains `hypr_events` with `try_iter` (collect first,
  then apply, so the receiver borrow ends): `HyprEvent::Workspace { name, .. }` →
  `self.apply_value(Probe::Workspace, Some(Value::Text(name)), 0)` (it lands in
  `rt.values` even with no deck); `HyprEvent::ActiveWindow(w)` → `self.rt.window = w`
  (PR 10 and 15 hook in here).
- `main.rs`'s `run()` calls `daemon.apply_hypr()` right after `daemon.apply_values()`,
  so it runs **before** the no-deck branch. The order is then: drain notify → 1 s stamp
  check → `reload()` when due → `handle_commands()` → `apply_values()` → `apply_hypr()` →
  `poll_jobs()` → `poll_hypr()` → no deck: `flush()`, then every 2 s `deck::open` →
  `attach`, else sleep `wait_for(now, next_deadline())`; deck: `pass()`, `Err` → `detach`.

## Render (`omadeck-core/src/render.rs`)

**Keys: one live layout, called from both paths of `render`.**

```rust
/// Draw `face` for key `k`: the live layout when the key has `live` and isn't busy,
/// else the ordinary face. The one place the live layout lives; both of `render`'s
/// paths (the busy-bar one and the plain one) call it in place of `render_face`.
fn render_body(&mut self, k: &KeyConfig, face: &Face, st: &KeyStatus, style: &Style, size: u32, bg: Rgb) -> RgbaImage;
```

- **The busy face wins over live text.** If `k.live.is_none()` or `st.state` is
  `Toggle::Busy(_)`, `render_body` returns `self.render_face(face, style, size, bg)`
  unchanged. So a busy live key looks exactly like any busy key.
- Otherwise `text = st.live.as_ref().map_or("–", |l| l.text.as_str())` and
  `charging = st.live.as_ref().is_some_and(|l| l.charging)`.
- **Icon or glyph** (`face.icon.is_some()`, or a non-blank `face.glyph`, or `charging`):
  build a new `Face { label: text, show_label: true, icon: face.icon, glyph, background:
  face.background, background_image: None, label_color: face.label_color }` (`Face` has
  no `Clone`), where `glyph` is `Some(CHARGING_GLYPH)` while charging with no icon and no
  glyph of its own, else `face.glyph`; then `render_face` it. The value takes the label's
  place; the configured label isn't drawn.
- **No icon or glyph:** `solid(size, bg)`, `fg = self.label_color(Some(face), style, bg)`.
  If `face.show_label && !face.label.is_empty()`, the label is a small caption
  (`self.draw_line(&mut img, face.label, s * 0.14, s * 0.16, fg, true)`) and the value
  fills the area below (`self.draw_block_in(&mut img, s * 0.28, s * 0.72, text, fg)`).
  Without a label: `self.draw_block(&mut img, text, fg)`.
- On and pressed keep working: the face comes from `face_for(&st.state)` before
  `render_body`, and `render` draws the pressed ring over the result.
- In `render`, replace both `self.render_face(&f, style, …, bg)` calls in the
  `(Some(k), Some(f))` arm with `self.render_body(k, &f, st, style, …, bg)`.

`draw_block` becomes `draw_block_in(&self, img: &mut RgbaImage, top: f32, height: f32,
text: &str, color: Rgb)`, with `draw_block(img, text, color)` =
`draw_block_in(img, 0.0, img.width() as f32, text, color)`. It splits `text` on `\n`
first and wraps each part (`text.split('\n').flat_map(|l| wrap(font, px, l, max_w))`),
so `%n` puts the date under the time. The fit check uses `height * 0.9` where it had
`s * 0.9`, and `top + (height − lh·n) / 2` where it had `(s − lh·n) / 2`; `max_w`, the
start size `s * 0.26` and the floor `s * 0.12` stay on the width `s`. For text without
`\n` and the full area, every pixel must stay the same (the goldens and existing key
tests check).

**Strip panels** (`render_strip_panel`): before the `level_source()` arm, a dial with
`live` draws `$B1` when `live.has_bar()`, else `$A1`. `$A1` is `draw_b1` without the bar:
add `bar: bool` as `draw_b1`'s last parameter; the existing caller passes `true`, and
`draw_b1` calls `draw_bar` only when `bar`.

```rust
if let Some(live) = dial.and_then(|d| d.live.as_ref()) {
    let shown = st.panel.live.as_ref();
    let icon = self.live_icon(&face, live, shown.is_some_and(|l| l.charging), glyph_color);
    let level = shown.map(|l| Level { value: l.bar.unwrap_or(0.0), text: l.text.clone(), muted: false });
    let colors = /* exactly the level arm's B1Colors, as R8 left it */;
    self.draw_b1(&mut img, &face, colors, want_label, icon, level.as_ref(), live.has_bar());
    return img;
}
```

`None` before the first value: `draw_b1` already draws `–`.

```rust
/// The icon for a live dial's panel: the user's icon, else their glyph, else the
/// source's own (a charging battery while charging).
fn live_icon(&mut self, face: &Face, live: &Live, charging: bool, color: Rgb) -> Option<RgbaImage>;
```

Like `brightness_icon`: `face.icon` → `self.icon(&paths::resolve(p), STRIP_ICON_PX)`; else
the non-blank `face.glyph`, else the source glyph, through `self.glyph(g, (STRIP_ICON_PX
as f32 * 0.86) as u32, color)`.

```rust
const CLOCK_GLYPH: &str = "f0150";     // nf-md clock-outline
const CPU_GLYPH: &str = "f061a";       // nf-md cpu-64-bit
const MEMORY_GLYPH: &str = "f035b";    // nf-md memory
const BATTERY_GLYPH: &str = "f0079";   // nf-md battery
const CHARGING_GLYPH: &str = "f0084";  // nf-md battery-charging
const WORKSPACE_GLYPH: &str = "f056e"; // check on the machine (Capture list)
const COMMAND_GLYPH: &str = "f018d";   // nf-md console
```

Use these as-is; the Capture list checks each renders. Change a constant only if it
shows the wrong symbol there.

## Daemon (`omadeckd/src/daemon.rs`)

- `probe::watched_probes(cfg, page, n_dials, n_keys)` (core): after a dial's level probe,
  its `live` probe; after a key's `state_probe()`, its `live` probe. Keep only
  `is_polled()` probes, each once, in that order.
- `probe::shows(dial, probe)`: first line
  `if let Some(l) = dial.and_then(|d| d.live.as_ref()) { return l.probe() == *probe; }`.
  `mark_probe`'s dial side then needs no change.
- `mark_probe`'s key loop matches the key's `live` probe too:
  `k.and_then(KeyConfig::state_probe).as_ref() == Some(probe) ||
  k.and_then(|k| k.live.as_ref()).map(Live::probe).as_ref() == Some(probe)`.
- `publish_keys`: the candidate test becomes `k.on.is_some() || k.busy_while_running ||
  k.live.is_some()`. `flush` then publishes `KeyState` and `DialStatus` with `live` by
  comparison; nothing is published from `apply_value`.
- `config_probes(cfg, n_dials)`: add each dial's and every key's (any page) `live` probe.
  `keep_toggle_state` keeps `Probe::Workspace` always:
  `retain(|p, _| *p == Probe::Workspace || all.contains(p))`. Nothing re-reads the
  workspace, and the listener sends only changes, so a dropped value would never return.
- `press_key`: after `run_input` for the key, if it ran and the key's `live` is
  `Live::Command`, send `WatchCmd::Refresh(probe)` so a key showing e.g. `playerctl
  status` updates right after the press. Shape:
  `let ran = self.run_input(..); if ran && let Some(p @ Probe::Output { .. }) =
  k.live.as_ref().map(Live::probe) { let _ = self.watcher.cmd.send(WatchCmd::Refresh(p)); }
  if !ran || k.on.is_none() { return; }`.
- `finish_job`'s `match k.state_probe()` is exhaustive over `Probe` today: Task B gives it
  a `Some(_) => None` arm (only `Volume` and `Command` matter there). R5 removed `watch()`'s
  `kept` filter, so `watch()` has no `Probe` match left.
- Disconnect: unchanged (`Watch(vec![])`); the listener keeps running. Page change: unchanged
  (`show_page` → `watch()`).
- `snapshot` (main.rs): unchanged. It passes `Values::default()`, so live keys and panels
  show `–`.

## GUI (`omadeck-gui`)

- `editor.rs`: a **Live value** group in `build_key`, after the Toggle group and before
  Appearance; a **Panel value** group in `build_dial`, after the three
  `dial_action_group`s and before Appearance. Each holds an `adw::ComboRow` "Shows" over
  `Live::KINDS` (`gtk::StringList::new(&Live::KINDS)`, `set_selected(Live::kind_index(..)
  as u32)` before connecting; on change `a.edit(i, |k| k.live = Live::of_kind(sel))` or
  `a.edit_dial(i, |d| d.live = Live::of_kind(sel))`, then `a.rebuild_editor()`). Per
  source:
  - Clock: `EntryRow` "Format", with a dim suffix label "strftime, e.g. %H:%M or %a %d
    %b"; under it an `ActionRow` titled with `live::clock_text(format)`, updated in the
    entry's `connect_changed` (one format per keystroke, not a timer).
  - Command: `EntryRow` "Command" and a `SpinRow` "Every (seconds)" over
    `gtk::Adjustment::new(interval as f64, 1.0, 3600.0, 1.0, 10.0, 0.0)`, the same
    pattern as the Toggle group's "Every (seconds)" (value `round() as u16`).
  - Keys only, any source: a hint row "The label shows as a small caption; with an icon,
    the value takes the label's place."
- Preview: `refresh_key` and `refresh_strip` already render the mirrored `KeyStatus` /
  `PanelStatus`, which now carry `live`. With no daemon there's no value, so `–` (the GUI
  never reads system state or runs the command). One change: `preview_status(i)` with a
  pinned On/Busy tab copies `live` from the mirrored status into `st`, so designing the
  On face still shows the live value.
- *Try it now* still runs the key's action.
- **Starter layout: unchanged.** It already fills all 15 keys, and keys 5–9 switch
  workspaces 1–5.

## Docs

README: a "Live keys" feature bullet (sources, the strftime link
<https://docs.rs/jiff/latest/jiff/fmt/strtime/>, the `command` example); roadmap
`- [x] **Live keys** that show a clock, CPU usage or the active workspace`.
`examples/config.toml`: the clock, CPU and command keys above and a battery dial,
commented. CHANGELOG under Unreleased/Added: "Live keys: a key or a dial's strip panel can
show the time, CPU, memory, battery, the active Hyprland workspace or a command's output,
kept up to date by the daemon."

## Capture on the machine first

None of these is recorded yet. Paste the output into this plan (or the named fixture
file) before the task that needs it. Items 2–5 and 7 can be run from a shell; item 1 needs
you to do the listed actions while it records; item 6 needs you to look at the glyphs.

1. **socket2 transcript** (Task E fixture `crates/omadeckd/tests/fixtures/socket2.txt`):
   `timeout 30 socat -U - UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock | tee ~/socket2.txt`
   while you: press `SUPER+2`, `SUPER+1`; click a window on another monitor (if any);
   focus a terminal; focus Firefox; change a tab in Firefox (title change); go to an empty
   workspace; `hyprctl dispatch renameworkspace 1 web`; `hyprctl reload`. This answers
   the address form, and what `activewindowv2` sends with nothing focused.
2. **Request socket JSON:**
   `printf 'j/activeworkspace' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket.sock`
   and the same with `j/activewindow`, with a window focused and on an empty workspace.
   Compare with `hyprctl -j activeworkspace` / `hyprctl -j activewindow`.
3. **Service environment:** `systemctl --user show-environment | grep -E 'HYPRLAND|XDG_RUNTIME'`
   and `ls -la $XDG_RUNTIME_DIR/hypr/`.
4. **Battery sysfs:** `grep . /sys/class/power_supply/*/{type,scope,status,capacity,energy_now,energy_full,charge_now,charge_full} 2>/dev/null`
   (on battery and on the charger). Copy the layout into the `battery_in` tests if it
   differs from theirs.
5. **/proc samples:** `head -1 /proc/stat; grep -E 'MemTotal|MemAvailable' /proc/meminfo`.
6. **Glyphs:** `for g in F0150 F061A F035B F0079 F0084 F056E F018D; do printf "%s \U000$g\n" $g; done`
   in the Omarchy terminal (bash), and `fc-list ":charset=f056e" family | head -3`.
   Each should show a clock, chip, memory stick, battery, charging battery, a
   workspace-like symbol and a console prompt.
7. **jiff `%n`:** in Task A's test run; if `format_clock("%H%n%M", ..)` doesn't give
   `"12\n00"`, replace `%n` with `\n` in `format_clock` before formatting.

## Tasks

Each task is one session of ~150–300 lines including tests, in this order. Each ends
with its check and a commit. Run `cargo fmt` before every commit. Paths are from the
repo root.

### Task A — `live.rs` and `sysinfo.rs` (pure, ~250 lines)

**Files:** `crates/omadeck-core/Cargo.toml` (jiff, the go/no-go above),
`crates/omadeck-core/src/lib.rs` (`pub mod live; pub mod sysinfo;`), new
`crates/omadeck-core/src/live.rs`, new `crates/omadeck-core/src/sysinfo.rs`.

**Signatures:** as in "Clock and system readers": `CLOCK_CAP`, `format_clock`,
`clock_text`, `wall_ms`, `needs_seconds`, `next_tick`; `CpuTimes`, `parse_proc_stat`,
`cpu_percent`, `CpuMeter::sample`, `parse_meminfo`, `battery_in`, `battery`.

**Tests** (in each file's `mod tests`): `clock_formats_and_ticks` (below),
`needs_seconds_formats` (`%H:%M` no; `%T`, `%S`, `%r` yes; `%%S` no),
`next_tick_second_boundary`, `next_tick_capped`, `bad_format_is_question_mark`;
`cpu_percent_from_two_samples`, `cpu_no_delta_is_zero`, `cpu_meter_primes_first`,
`meminfo_used_percent`, `meminfo_missing_field_is_none`, `battery_charging_status`
(Charging / Full / Discharging / Not charging), `battery_skips_device_scope`,
`battery_combines_two`. The battery tests build a temp dir
(`std::env::temp_dir().join(format!("omadeck-bat-{name}-{}", std::process::id()))`) with
one subdirectory per supply and the files `type`, `scope`, `status`, `capacity`,
`energy_now`, `energy_full`.

```rust
#[test]
fn clock_formats_and_ticks() {
    let ts: jiff::Timestamp = "2026-10-15T12:00:59.5Z".parse().unwrap();
    let now = ts.to_zoned(jiff::tz::TimeZone::UTC);
    assert_eq!(format_clock("%H:%M", &now), "12:00");
    assert_eq!(format_clock("%H:%M%n%a %d", &now), "12:00\nThu 15");
    // 500 ms to the minute, plus the 10 ms margin.
    assert_eq!(next_tick(ts.as_millisecond(), false), Duration::from_millis(510));
    assert_eq!(next_tick(ts.as_millisecond(), true), Duration::from_millis(510));
    assert_eq!(next_tick(ts.as_millisecond() + 1, false), Duration::from_millis(509));
}
```

**Check:** `git diff --stat Cargo.lock` → `Cargo.lock | 1 +`; then
`cargo test -p omadeck-core -- live:: sysinfo::` → `test result: ok. 13 passed; 0 failed`.

**Traps:** don't add jiff to the daemon or the GUI. `next_tick` uses `rem_euclid`, not
`%` (negative times). `parse_proc_stat` reads only the line starting with `"cpu "` (with
the space), not `cpu0`.

### Task B — config, probes, values, wire types (~240 lines)

**Files:** `crates/omadeck-core/src/config.rs` (`Live`, the two fields, `new`,
`is_blank`, `level_source`'s first line, `Live::{KINDS, kind_index, of_kind, has_bar,
probe}`), `crates/omadeck-core/src/probe.rs` (the new `Probe` and `Value` variants, the
`interval`/`refresh_after` arms, `is_polled`, `live_show`, `Values::level`'s `_` arm,
`key_status` and `dial_status` filling `live`), `crates/omadeck-core/src/render.rs`
(`LiveShow`, `KeyStatus.live`, `PanelStatus.live` only), `crates/omadeck-core/src/ipc.rs`
(the `KeyState` doc line; tests), `crates/omadeck-core/src/lib.rs` (`Live`, `LiveShow`).
The new variants break exhaustive matches outside core; fix each one
`cargo build --workspace` lists, doing nothing new yet:
- `omadeckd/src/watch.rs` `run_due`: a temporary arm for the new probes that only
  reschedules (`self.next_due.insert(p.clone(), now + p.interval());`). Task D replaces
  it. Nothing watches them before Task F.
- `omadeckd/src/daemon.rs`: `finish_job`'s `match k.state_probe()` gets `Some(_) => None`
  (after the `Volume` arm). `watch()` has no `Probe` match after R5.
- Any `KeyStatus { .. }` / `PanelStatus { .. }` literal without `..` gets `live: None`.

**Tests:** `live_key_round_trips` (below), `clock_default_format_not_written`,
`command_interval_default_not_written`, `old_config_saves_byte_identical` (load
`examples/config.toml` through `toml::from_str`, save with `toml::to_string_pretty`, load
and save again, compare the two saves), `key_with_only_live_is_not_blank`,
`dial_live_wins_over_level_source`, `live_show_forms` (Text, Percent, Battery),
`workspace_is_not_polled`, `command_live_probe_clamps_interval` (`interval = 0` → 1000
ms, and `Live::Command { interval: 0 }` saves `interval = 0`), and in `ipc.rs`
`key_state_live_wire_shape` (a `KeyStatus` with `live: Some(LiveShow { text: "AA".into(),
bar: None, charging: false })` writes `"status":{"live":{"text":"AA"}}`; with `live: None`
the field is absent).

```rust
#[test]
fn live_key_round_trips() {
    for live in [
        Live::Clock { format: "%H:%M%n%a".into() },
        Live::Cpu,
        Live::Memory,
        Live::Battery,
        Live::Workspace,
        Live::Command { command: "date +%s".into(), interval: 30 },
    ] {
        let mut c = Config::default();
        c.keys.get_mut(3).live = Some(live.clone());
        let s = toml::to_string_pretty(&c).unwrap();
        assert!(s.contains("[key.live]"), "{s}");
        assert_eq!(toml::from_str::<Config>(&s).unwrap().key(None, 3).unwrap().live, Some(live));
    }
}
```

**Check:** `cargo test -p omadeck-core -- config:: probe:: ipc::` → `test result: ok.`,
0 failed; `cargo build --workspace` clean.

**Traps:** `live` must be the last field of both structs, or `toml` fails with "values
must be emitted before tables". `Probe::interval` and `refresh_after` stay exhaustive (no
`_`), so PR 10 and 14 get a compile error when they add probes. R4's
`rename_all = "snake_case"` on `Value` covers the new variants; don't add a second one.

### Task C — render (~260 lines)

**Files:** `crates/omadeck-core/src/render.rs`. `draw_block_in` with `\n`; `render_body`
and the two call sites in `render`; `draw_b1(.., bar: bool)`; `live_icon` and the glyph
constants; the live arm in `render_strip_panel`.

**Tests** (in `render::tests`; add `B1_VALUE` and `LiveShow` to its `use super::{..}`,
and `Live` to the `crate::config` import): `dial_live_text_is_a1_without_bar` (below),
`dial_live_level_is_b1_with_bar` (a `Live::Cpu` dial with `bar: Some(0.5)` has bar pixels
that differ from the background), `live_key_value_centred_without_icon`,
`live_key_label_is_caption` (pixels in the top `s × 0.28` band differ from the
background, and so do pixels below it), `live_key_with_glyph_puts_value_in_label_slot`
(glyph `"A"`: ink below `s × 0.70`), `busy_live_key_shows_busy_face` (a live key with
`state: Toggle::Busy("AA".into())` renders the same image as the same key without
`live`), `draw_block_honours_newline` (`"A\nA"` gives two separate ink bands),
`live_without_value_shows_dash` (`KeyStatus { live: None, .. }` renders the same as a
`LiveShow` with text `"–"`), and the existing goldens and key tests unchanged.

```rust
#[test]
fn dial_live_text_is_a1_without_bar() {
    let mut r = renderer_with_font();
    let mut cfg = Config { style: test_style(), ..Default::default() };
    cfg.dial_mut(0).live = Some(Live::Clock { format: "%H:%M".into() });
    let st = DialStatus {
        panel: PanelStatus {
            // Test texts use "A": the fixture font is ASCII plus – and ….
            live: Some(LiveShow { text: "AA".into(), bar: None, charging: false }),
            ..Default::default()
        },
        ..Default::default()
    };
    let panel = r.render_strip_panel(&cfg, 0, 4, st);
    let bg = *panel.get_pixel(2, 2);
    let (bx, by, bw, bh) = B1_BAR;
    // $A1: nothing is drawn where $B1's bar would be.
    for y in by..by + bh {
        for x in bx..bx + bw {
            assert_eq!(*panel.get_pixel(x, y), bg, "bar pixel at {x},{y}");
        }
    }
    // The value is drawn.
    let (vx, vy, vw, vh) = B1_VALUE;
    assert!((vy..vy + vh).any(|y| (vx..vx + vw).any(|x| *panel.get_pixel(x, y) != bg)));
}
```

**Check:** `cargo test -p omadeck-core render::` → `test result: ok.`, 0 failed (the
goldens included).

**Traps:** the live layout exists only in `render_body`; don't copy it into the busy-bar
branch. `draw_block_in` must not change a single pixel of a one-line full-size block (the
golden and `key_glyph_stays_in_its_box` tests catch it). The fixture font has no Nerd Font
glyphs, so `self.glyph(CLOCK_GLYPH, ..)` returns `None` in tests: assert on text, not on
the source glyph.

### Task D — watcher reads and command output (~230 lines)

**Files:** `crates/omadeckd/src/watch.rs`. `read_live`, the `Poller.cpu` and
`Poller.read_live` fields (and `impl Default for Poller`), the inline arm in `run_due`
(replacing Task B's temporary one), the `Cpu` reset in `handle(Watch)`, `next_read` and
`record`'s call to it, the `Output` cases in `run_due`, `run` and the `Done` arm,
`LIVE_COMMAND_TIMEOUT`, the module doc sentence about the clock waiting behind volume.

**Tests** (in `watch::tests`): `clock_scheduled_on_boundary` (below),
`live_read_is_inline` (a `Poller` with `read_live` set to a stub returning
`Some(Value::Percent(42))`: `Watch([Probe::Cpu])`, `run_due` starts nothing and `out` has
`(Probe::Cpu, Some(Value::Percent(42)), 0)`), `cpu_meter_resets_when_unwatched`,
`output_runs_on_a_runner` (`Watch` a `Probe::Output`, `run_due` returns it, a second
`run_due` returns nothing), `output_done_records_text` (the same, then
`p.handle(WatchCmd::Done { probe: probe.clone(), run: p.run_of(&probe), result: RunResult::Exit
{ code: Some(1), first_line: Some("x".into()) } }, now, &mut out)` sends
`(probe, Some(Value::Text("x".into())), 0)`; with `first_line: None` it sends `"–"`).

```rust
#[test]
fn clock_scheduled_on_boundary() {
    let now = Instant::now();
    // 2026-10-15T12:00:59.500Z
    let wall: i64 = 1_792_065_659_500;
    let p = Probe::Clock { format: "%H:%M".into() };
    assert_eq!(next_read(&p, now, wall), now + Duration::from_millis(510));
    let p = Probe::Clock { format: "%H:%M:%S".into() };
    assert_eq!(next_read(&p, now, wall), now + Duration::from_millis(510));
    assert_eq!(next_read(&Probe::Cpu, now, wall), now + Duration::from_secs(1));
}
```

**Check:** `cargo test -p omadeckd watch::` → `test result: ok.`, 0 failed.

**Traps:** never call `read_live` for `Output`: commands only run on runner threads.
`(self.read_live)(&p, &mut self.cpu)` needs `p` cloned out of `self.watched` first, as
the loop already does (`for p in self.watched.clone()`). Keep the `Leave`/busy fast path
in `record` as it is; only its non-fast branch calls `next_read`.

### Task E — `hypr.rs` (~300 lines)

**Files:** new `crates/omadeckd/src/hypr.rs`, `crates/omadeckd/Cargo.toml`
(`serde_json.workspace = true`), `crates/omadeckd/src/main.rs` (`mod hypr;` only),
fixture `crates/omadeckd/tests/fixtures/socket2.txt` from Capture 1. Code as in the
listener section.

**Tests:** `parse_workspacev2`, `parse_workspace_name_with_comma` (`5,my,ws` → name
`my,ws`), `parse_unknown_ignored` (`workspace>>2`, `activewindow>>a,b`,
`activespecial>>,DP-1` → `None`), `empty_activewindowv2_is_no_window`,
`title_only_for_focused` (below), `seed_parses_both_queries`,
`window_json_empty_object_is_none`, `find_instance_prefers_env`,
`find_instance_falls_back_to_newest` (temp dir with two instance dirs and files named
`.socket2.sock`), `transcript_gives_events` (feeds each line of the fixture through
`parse`/`handle` with a canned `query`; expected events written after Capture 1).

```rust
#[test]
fn title_only_for_focused() {
    let mut seen = Seen::default();
    let mut query = |c: &str| match c {
        "j/activewindow" => Some(r#"{"address":"0xabc","class":"firefox","title":"One","pid":42}"#.into()),
        _ => None,
    };
    let evs = handle(Parsed::QueryWindow, &mut seen, &mut query);
    let w = ActiveWindow { address: "abc".into(), class: "firefox".into(), title: "One".into(), pid: Some(42) };
    assert_eq!(evs, vec![HyprEvent::ActiveWindow(Some(w.clone()))]);
    // Another window's title: nothing.
    let other = Parsed::Title { address: "def".into(), title: "Two".into() };
    assert!(handle(other, &mut seen, &mut query).is_empty());
    // The focused one's: the same window, new title.
    let own = Parsed::Title { address: "abc".into(), title: "Two".into() };
    let evs = handle(own, &mut seen, &mut query);
    assert_eq!(evs, vec![HyprEvent::ActiveWindow(Some(ActiveWindow { title: "Two".into(), ..w }))]);
}
```

**Check:** `cargo test -p omadeckd hypr::` → `test result: ok.`, 0 failed;
`cargo clippy -p omadeckd --all-targets -- -D warnings` clean.

**Traps:** `start` is unused until Task F, so clippy needs `#[allow(dead_code)]` on it in
this commit; Task F removes it. The request socket must be closed after each request
(Hyprland freezes on open ones): `request` opens a new `UnixStream` each call and drops
it. Read the event socket with `read_until`, not `lines()`.

### Task F — daemon wiring (~220 lines)

**Files:** `crates/omadeckd/src/daemon.rs` (`Daemon.hypr_events`, `Runtime.window`,
`apply_hypr`, `mark_probe`'s key match, `publish_keys`' candidates, `config_probes`,
`keep_toggle_state`'s `Workspace` rule, the `Refresh` after a press),
`crates/omadeckd/src/main.rs` (the `apply_hypr` call; remove Task E's `allow`),
`crates/omadeck-core/src/probe.rs` (`watched_probes`, `shows`).

**Tests:** in `probe.rs`: `watched_probes_only_current_page` (below),
`watched_probes_includes_dial_live`, `shows_live_dial` (a `Live::Battery` dial shows
`Probe::Battery`, not `Probe::Volume(Output)`), `key_status_fills_live`; in `daemon.rs`:
`config_probes_include_live_on_every_page`.

```rust
#[test]
fn watched_probes_only_current_page() {
    let cfg = Config::parse(
        r#"
[[key]]
index = 0
live = { source = "cpu" }

[[page]]
name = "Other"
[[page.key]]
index = 0
live = { source = "memory" }

[[dial]]
index = 0
live = { source = "workspace" }
"#,
    )
    .unwrap();
    let home = watched_probes(&cfg, None, 4, 15);
    assert!(home.contains(&Probe::Cpu));
    assert!(!home.contains(&Probe::Memory));
    assert!(!home.contains(&Probe::Workspace), "fed by the listener, never polled");
    assert!(watched_probes(&cfg, Some("Other"), 4, 15).contains(&Probe::Memory));
}
```

**Check:** `cargo test --workspace` → every `test result: ok.`, 0 failed;
`cargo clippy --all-targets -- -D warnings` clean. Then on the hardware: a clock key, a
CPU key, a command key, a workspace key.

**Traps:** `Runtime.window`, `HyprEvent::Workspace`'s `id` and `ActiveWindow`'s fields are
written but not read until PR 10 and 15, and derived impls don't count as reads: add
`#[allow(dead_code)] // read by PR 10 and PR 15` on each, not a crate-wide allow.
`apply_hypr` must collect the events before applying them (`apply_value` takes
`&mut self` while `self.hypr_events` is borrowed). Never drop `Probe::Workspace` from
`rt.values`.

### Task G — GUI (~200 lines)

**Files:** `crates/omadeck-gui/src/editor.rs` (the two groups),
`crates/omadeck-gui/src/app.rs` (`preview_status` copies `live`).

**Traps:** never `if let Some(k) = app.key(i) { app.edit(i, ..) }` while a `config`
borrow is alive (BUGS.md 4: `a.key(i)` already clones, so drop it before editing); no
`rebuild_editor()` from an `EntryRow`'s `connect_changed` (focus is lost); set the
combo's initial value before `connect_selected_notify`; closures shared between rows are
`Rc<dyn Fn…>`, not `impl Fn + Clone`. The Format row's example title updates in
`connect_changed`, from the entry text, never from the config.

No test (GTK). **Check:** `cargo clippy --all-targets -- -D warnings` clean; by hand:
pick each source on a key and on a dial, the preview shows `–` with the daemon stopped
and the live value with it running; the clock example row follows typing; the On tab of
a live toggle key still shows the value.

### Task H — docs (~90 lines)

**Files:** `README.md`, `examples/config.toml`, `CHANGELOG.md`,
`crates/omadeck-core/src/config.rs` (the example tests).

**Traps:** `example_dial_parses` asserts `blocks.len() == 3` dial examples and checks
them by index. Adding the commented battery dial makes it four: change the count to 4
and add `assert!(c.dials[3].live.is_some());`. Put the live keys after the toggle keys,
commented, so `example_parses` still finds a toggle key.

**Check:** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → all
green (`example_parses` and `example_dial_parses` included).

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] A `%H:%M` clock key turns over within ~0.15 s of the minute (compare with Waybar);
      a `%H:%M:%S` key ticks every second without skipping.
- [ ] After suspend and resume the clock is right within 5 s.
- [ ] `timedatectl set-timezone …` moves the clock (how fast jiff notices: unconfirmed).
- [ ] CPU follows btop roughly; memory and battery match `free` and Waybar; plugging the
      charger shows the charging glyph within 5 s.
- [ ] Workspace key follows `SUPER+1..5`, a click on another monitor's window, and a
      rename; `–` when the daemon runs outside Hyprland (TTY).
- [ ] Restarting Hyprland reconnects the listener with the new signature.
- [ ] A command key with `sleep 30` doesn't delay the clock; its run is killed at 5 s
      (default interval).
- [ ] Spinning a volume dial hard while a seconds clock runs: the clock may stall for a
      moment but catches up (the accepted wait behind volume).
- [ ] Pressing a live key runs its action; the pressed ring shows over the value. A live
      key with `busy_while_running` shows the busy face while its command runs, then the
      value again.
- [ ] Battery and clock dials draw `$B1` and `$A1` correctly on the strip.
- [ ] The configurator's preview shows the same values as the deck, within a second.
- [ ] No visible CPU cost from the daemon with a seconds clock (`top`).

## Decided (2026-10-17)

- **Decided: `live` is a key and dial field, not an `Action`.** What a key shows and what
  it does are separate, so a clock can launch a calendar.
- **Decided: values are `Text`, `Percent` and `Battery`,** not `Level` (which carries a
  `muted` that means nothing here). The renderer gets a `LiveShow { text, bar, charging }`.
- **Decided: the command probe is `Probe::Output { command: String, interval_ms: u32 }`,**
  the same shape as PR 8's `Probe::Command`. The config keeps `source = "command"` with
  `interval` in seconds (`u16`, like `on.interval`).
- **Decided: the busy face wins over live text.** A live key that is busy (a busy toggle
  or a running command) shows its busy face. The live layout is one function
  (`render_body`) that both of `render`'s paths call.
- **Decided: `$A1` is `draw_b1` without the bar,** same rects, a `bar: bool` parameter.
- **Decided: CPU 1 s** (Elgato's plugin rate), **memory 2 s, battery 5 s.**
- **Decided: the clock wakes on the boundary, capped at 5 s,** because monotonic sleeps
  stop during suspend.
- **Decided: the clock may wait behind a volume command on the watcher** (normally under
  100 ms, at most ~2 s on a hung PipeWire). Documented in `watch.rs`; no extra thread.
- **Decided: jiff** (already in the build), in core only, with the go/no-go and libc
  fallback in Task A. No `sysinfo` crate.
- **Decided: commands run on PR 8's runner threads** with a timeout of the interval,
  clamped to 1–10 s; one run at a time per probe; the first non-empty stdout line, any
  exit status, `–` when empty; a failed run keeps the last value.
- **Decided: the Hyprland listener is part of this PR** (Task E), not its own PR: the
  branches chain anyway, and PR 10 and 15 build on this one. It is separate from R6's
  dispatch worker.
- **Decided: sockets, not `hyprctl`.** Events from `.socket2.sock`; the window and
  workspace from `.socket.sock` with the same `j/…` requests `hyprctl -j` sends, so no
  process per focus change. `activewindowv2` triggers one `j/activewindow` request, which
  gives class, title and pid together, so `activewindow>>` isn't paired up.
- **Decided: the workspace also follows `focusedmon` and `renameworkspace`,** since
  `workspacev2` isn't sent when focus moves to another monitor by mouse. Special
  workspaces are ignored.
- **Decided: the listener always runs inside Hyprland,** even with no workspace key,
  because PR 10 and 15 need the focused window; it's one idle blocking read.
- **Decided: the GUI mirrors live values from `KeyState`/`DialStatus` events only;**
  without the daemon it shows `–`. `snapshot` shows `–`.
- **Decided: the glyph list above,** checked on the machine before Task C.
- **Decided: starter layout unchanged.**
- **Decided: a live key's label is a small caption above the value.** Hiding it, as
  Elgato does, would leave a label the user set with nowhere to show.

## Ask before building

None; all settled 2026-10-17.

## Decisions to explain

- **`live` is a key field, not an `Action`.** Elgato ties the display to the plugin's
  action. Here a key's look and its action are separate, and dials get the same field.
- **The value wins over the label.** Elgato draws a user's title *instead of* the
  plugin's; that would hide the value, so omadeck keeps the label as a caption.
- **Busy wins over the value.** A key that is busy says so first, as every other busy
  key does.
- **Text on keys, no bar.** Elgato's CPU key is just `N%` over an image; bars are a strip
  thing (`$B1`).
- **`$A1` for text, `$B1` for levels** on the strip, Elgato's own layouts. This also
  delivers `$A1` from PLAN.md's "Later".
- **CPU every 1 s** like Elgato's plugin; memory 2 s and battery 5 s are our own.
- **Clock wakes on the boundary, capped at 5 s,** cheaper than a fixed poll and right
  after resume.
- **jiff for time:** already in the build through `env_logger`; it reads the system zone.
- **The command source** has no Elgato built-in counterpart (Marketplace plugins do it).
- **Hyprland via sockets,** no process per focus change; one listener reused by PR 10
  and 15.

## Later

- Highlight the active workspace's key in the starter (PR 8 states fed by
  `Probe::Workspace`), and special workspaces (`activespecialv2`).
- A `window` source (the focused window's title or class): the event is already there.
- Analog clock faces and time zones; a dial turn to cycle zones (Elgato's Clocks plugin).
- A level bar on keys, and per-source format templates (`"CPU {}"`).
- Temperature, network rate, GPU, per-core CPU.
- Refresh a live command key's value when its `busy_while_running` job ends.
- Clocks off the watcher (a daemon deadline) if the wait behind volume ever shows.

**Size:** about 1,790 lines including tests over 8 tasks (A 250, B 240, C 260, D 230,
E 300, F 220, G 200, H 90).

## Names later PRs use

Final names, from this PR (PR 10 and PR 15 copy them):

```rust
// crates/omadeckd/src/hypr.rs
pub struct ActiveWindow { pub address: String, pub class: String, pub title: String, pub pid: Option<u32> }
pub enum HyprEvent { Workspace { id: i64, name: String }, ActiveWindow(Option<ActiveWindow>) }
pub fn start() -> Option<mpsc::Receiver<HyprEvent>>;

// crates/omadeckd/src/daemon.rs
Daemon.hypr_events: Option<mpsc::Receiver<hypr::HyprEvent>>   // set in Daemon::new; `Daemon.hypr` is R6's HyprWorker
Runtime.window: Option<hypr::ActiveWindow>
pub fn apply_hypr(&mut self);   // main.rs run(): right after apply_values(), before poll_jobs()
                                // and the no-deck branch; its HyprEvent::ActiveWindow(w) arm
                                // sets rt.window = w

// core
pub enum Live { Clock { format }, Cpu, Memory, Battery, Workspace, Command { command, interval: u16 } }
Probe::{Clock { format: String }, Cpu, Memory, Battery, Workspace, Output { command: String, interval_ms: u32 }}
Probe::is_polled(&self) -> bool
Value::{Text(String), Percent(u8), Battery { percent: u8, charging: bool }}
pub struct LiveShow { pub text: String, pub bar: Option<f32>, pub charging: bool }   // render.rs
KeyStatus.live: Option<LiveShow>;  PanelStatus.live: Option<LiveShow>   // PR 10's `app`, PR 14's `media` go beside it
probe::live_show(live: &Live, values: &Values) -> Option<LiveShow>
live::{CLOCK_CAP, format_clock, clock_text, wall_ms, needs_seconds, next_tick}
Renderer: fn render_body(..), fn live_icon(..), draw_b1(.., bar: bool)
```

## Status (2026-10-17)

Rewritten against `feat/toggle-keys` as merged (`55d0d62`) and `fix/review-2`'s R1–R9:
every predicted name replaced with the real one (`apply_value(probe, Some(value), applied)`,
`config_probes`, `Poller.read_volume`'s seam pattern, the two render paths, the test
helpers), `Probe::Output` for command output, the busy-over-live rule, and the accepted
clock wait behind volume. Not started.

- **Settled (2026-10-17):** a live key's label is a small caption above the value.
- **Captures pending** ("Capture on the machine first"): items 2–5 and 7 the assistant
  can run from the shell; item 1 (the 30 s socket2 transcript) needs the user to perform
  the listed actions while it records; item 6 (the seven Nerd Font glyphs) needs the user
  to confirm they render.
