# PR 8: `feat/toggle-keys`: keys with on/off faces that follow real state

Part of [PLAN.md](PLAN.md). Base: **`feat/groundwork`** ([groundwork.md](groundwork.md)),
which sits on `fix/review` → `fix/bugs` (`d7bd448`). This PR extends the **state watcher**
that groundwork introduces; PRs 9, 10 and 14 extend it further.

**Rewritten 2026-10-17** against `d7bd448` plus the groundwork contents fixed in the
2026-10-17 cross-plan decisions (D1–D15). Every open decision is settled below
("Decided"); three choices are left to me under "Ask before building". Tasks A–K are
~150–300 lines each, in order, each with its files, signatures, one test written out and an
exact check.

**Outcome:** any key can get a second face, `[key.on]`, with its own label, icon or glyph
and colours. Which face shows comes from the real system: a shell command's exit status
polled in the background (night light, stay awake, Bluetooth, a systemd service, …), or
the mute the daemon already reads for a volume mute key. A key with no source flips on
each press, as the Elgato app's two-state actions do. A command can also report **busy**
(exit 75) with a progress line, for things that take a while to come up or go down (a
local model server, a VM, a Docker stack): the key shows the progress text, optionally a
bar, and the watcher polls faster until it settles. Separately, **any** key running a
plain command or script can opt in to looking busy for as long as that command runs
(`busy_while_running`). The configurator mirrors what the deck shows and gets a Toggle
group and toggle presets. Old configs load and save byte-identical.

## What this PR takes from groundwork

Use these names exactly (decisions D3–D10). If groundwork.md named one differently, use
groundwork's name and don't add a second copy.

- Core `probe.rs`: `Probe { Volume(AudioDevice) }`, `Value { Level(Level) }` (`Level` is
  `render::Level`, with `muted`; `Value` derives `Serialize, Deserialize`),
  `Values { probes: HashMap<Probe, Value>, brightness }`, `dial_status(..)`,
  `watched_probes(cfg, _page, n_dials: usize)` (PR 8 uses the page),
  `Probe::interval()`, `Probe::refresh_after()`.
- Core `proc.rs`: `output_timeout(cmd, timeout) -> Option<Output>`, `COMMAND_TIMEOUT = 2 s`.
- Daemon `watch.rs`: `WatchCmd { Watch(Vec<Probe>), Refresh(Probe), Volume { device, change, mute }, Stop }`
  (`Refresh` carries `#[allow(dead_code)]` until PR 8 sends it; drop the allow),
  `Watcher { cmd, values, thread }` with `Watcher::stop(self)` (sends `Stop`, joins),
  `start()`, the thread loop `run(cmd_rx, val_tx, poller)` with `coalesce`, and the
  `Poller { watched, last, next_due, forced, read_volume, change_volume }` with
  `handle`, `run_due`, `record` and `next_due()`. The thread blocks on `recv()` with
  nothing watched, else `recv_timeout(next_due - now)`.
- Daemon `daemon.rs`: `struct Daemon { cfg, renderer, nav, wanted_page, hub, watcher,
  saver, rt, deck, published, reload_at, repaint_at }`, `Runtime { values, state }`
  (derives `Default`, outlives replug; the brightness is `rt.values.brightness`),
  `enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick }`,
  `Daemon::run_input(&mut self, src, act, steps) -> bool`, `Daemon::apply_value(probe,
  value)` (the one place watcher values and guesses land), `Daemon::watch()` (sends
  `Watch`; called from `attach` and `reload`), `Daemon::next_deadline()`, and
  `Daemon::flush()` at the end of every pass, which calls `deck::flush_dirty` and then
  publishes each panel's `DialStatus` that differs from `published`.
- Daemon `deck.rs`: `Deck` (hardware only: `dev, reader, kind, info, dial_pressed,
  flash_until, dirty_panels`), `paint_all`, `flush_dirty`, `paint_key` and the private
  `write_key`. `saver.rs`: `StateSaver`.
- Hub: per-client writer threads; a `Replay { status, page, brightness, panels }` struct
  replayed to new clients.
- IPC: `Event::Brightness { value }`, `Event::DialStatus { index, status: PanelStatus }`
  (`PanelStatus` is `DialStatus` minus `pressed`); the GUI mirrors them and no longer polls.
- Render: `draw_bar(img, rect, value, fill, track)` (clamps `value`), extracted from
  `draw_b1`, which takes separate title and value colours. PR 8 calls it; it adds no
  second bar drawer.
- GUI: `ActionSlot { target, dir, get, set, … }` with `ActionTarget { Key(KeyRef),
  DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8) }`, `App::action_at` /
  `App::set_action_at`, `action_editor(app, group, combo_title, &slot)`,
  `Action::default_for(kind, dir)`, `Action::is_deck_owned()` (D11).

## How the Elgato app does it (researched 2026-10-16)

- An action declares up to two `States` in its manifest; with two, "the action will act as
  a toggle" ([keys guide](https://docs.elgato.com/streamdeck/sdk/guides/keys),
  [manifest](https://docs.elgato.com/streamdeck/sdk/references/manifest)).
- **The state flips on press automatically.** A plugin can opt out
  (`DisableAutomaticStates`) and drive it with `setState(0|1)`, which is how plugins show
  real state (a mute that follows the system).
- **Each state has its own image, title, title colour, alignment and font.** Whether users
  set a per-state *title* in the app is **unconfirmed**.
- **One action per key; the state advances.** Built-in exceptions: **Hotkey Switch** (two
  hotkeys, used alternately) and **Multi Action Switch** (two multi-actions)
  ([hotkeys how-to](https://www.elgato.com/eu/el/explorer/products/stream-deck/how-to-set-up-stream-deck-hotkeys/),
  [4.4 notes](https://help.elgato.com/hc/en-us/articles/5162934218637-Elgato-Stream-Deck-4-4-Release-Notes)).
  So a second action for the second state is Elgato behaviour too.
- **No busy state.** Plugins show progress by calling `setTitle` repeatedly ("Loading
  43%") and flash `showAlert`/`showOk` when done. omadeck's busy face is the same idea,
  driven by the state command instead of plugin code.
- **Unconfirmed:** whether the app remembers a state across restarts.
- Dials have no `States`; nothing to copy for them.

## Config (`omadeck-core/src/config.rs`)

A key becomes a toggle by having an `[key.on]` table. The busy look is `[key.busy]`.

```rust
pub struct KeyConfig {
    // … existing fields up to label_color, unchanged …
    /// Show the busy face until this key's command exits. Only `app` (a plain command,
    /// not a `.desktop` id) and `script` actions; ignored for others.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub busy_while_running: bool,
    #[serde(default)]
    pub action: Action,
    /// A second face shown while the key is on, and where on/off comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<OnFace>,
    /// The look while busy (a state command's exit 75, or a running command).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy: Option<BusyFace>,
}
```

Field order matters: the scalar `busy_while_running` goes **before** `action`, the tables
`on` and `busy` after it, so TOML values come before tables.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OnFace {
    // The look while on; each unset field falls back to the key's own.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label_color: Option<String>,
    /// A shell command: exit 0 = on, 75 = busy (its first output line is the progress),
    /// anything else = off. Unset: a volume mute key follows the mute; any other key
    /// flips on each press.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub state: Option<String>,
    /// Seconds between runs of `state`; at least 1.
    #[serde(default = "two", skip_serializing_if = "is_two")] pub interval: u16,
    /// A slow toggle: a press shows busy at once and polls fast until the state moves.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub slow: bool,
    /// Run this instead of the key's action while on (Elgato's Hotkey Switch).
    #[serde(default, skip_serializing_if = "Action::is_none")] pub action: Action,
}

impl Default for OnFace {   // by hand: a derived Default would give interval 0
    fn default() -> Self {
        Self { label: None, icon: None, glyph: None, background: None, label_color: None,
               state: None, interval: 2, slow: false, action: Action::None }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BusyFace {
    /// Unset: the progress text, else "…".
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label_color: Option<String>,
    /// Draw a progress bar when the busy text has a percentage (the last `NN%`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub bar: bool,
}

fn two() -> u16 { 2 }
fn is_two(v: &u16) -> bool { *v == 2 }
```

New methods on `KeyConfig` (all pure):

```rust
/// What the watcher polls for this key, if anything. A non-blank `on.state` wins; else a
/// volume mute action follows its device; else `None` (a press toggle, or not a toggle).
pub fn state_probe(&self) -> Option<Probe>;
/// `on` is set and nothing is polled: flips on each press.
pub fn is_press_toggle(&self) -> bool;
/// `on.slow` and the source is a command.
pub fn is_slow_toggle(&self) -> bool;
/// `on.action` while on and set, else `action`.
pub fn action_for(&self, on: bool) -> &Action;
/// The face for a state. Off: `face()`. On: the on face merged over it. Busy: the busy
/// face merged over the *off* face; label = busy.label, else the text, else "…";
/// `show_label` forced on.
pub fn face_for<'a>(&'a self, t: &'a Toggle) -> Face<'a>;
```

```rust
pub fn state_probe(&self) -> Option<Probe> {
    let on = self.on.as_ref()?;
    if let Some(cmd) = on.state.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        return Some(Probe::Command { command: cmd.to_string(), interval_ms: on.interval.max(1) as u32 * 1000 });
    }
    match &self.action {
        Action::Volume { device, mute: true, .. } => Some(Probe::Volume(*device)),
        _ => None,
    }
}

/// Merge one state's look over a face. Setting `icon` or `glyph` replaces both (one image
/// per state, as in Elgato; otherwise a base icon would always beat an on glyph).
fn merge<'a>(mut f: Face<'a>, label: &'a Option<String>, icon: &'a Option<String>, glyph: &'a Option<String>,
             background: &'a Option<String>, label_color: &'a Option<String>) -> Face<'a> {
    if let Some(l) = label { f.label = l.trim(); }      // Some("") = no label while on
    if icon.is_some() || glyph.is_some() { f.icon = icon.as_deref(); f.glyph = glyph.as_deref(); }
    if let Some(b) = background { f.background = Some(b); }
    if let Some(c) = label_color { f.label_color = Some(c); }
    f
}
```

`is_blank` adds `&& self.on.is_none() && self.busy.is_none() && !self.busy_while_running`;
`KeyConfig::new` sets all three to their defaults.

`Action::is_trackable(&self) -> bool`: `App { command }` whose trimmed command isn't a
`.desktop` id (same test as `launch_app`), and `Script { .. }`.

"On" for a mute key means **muted**: the toggle being on is the thing the key does.

```toml
[[key]]
index = 10
label = "Mic"
glyph = "f036c"
[key.action]
type = "volume"
device = "input"
mute = true
[key.on]                       # no state: follows the mic's mute
label = "Muted"
glyph = "f036d"

[[key]]                        # a slow toggle: a systemd user service (a model server)
index = 4
label = "LLM off"
glyph = "f048b"
[key.action]
type = "app"
command = "systemctl --user is-active -q llm.service && systemctl --user stop --no-block llm.service || systemctl --user start --no-block llm.service"
[key.on]
label = "LLM on"
slow = true
state = """
s=$(systemctl --user is-active llm.service)
case $s in
  active) curl -sf -o /dev/null -m 2 http://127.0.0.1:8000/health && exit 0
          echo Loading; exit 75 ;;
  activating|deactivating|reloading) echo "$s"; exit 75 ;;
  *) exit 1 ;;
esac"""

[[key]]                        # busy while its own script runs
index = 6
label = "Backup"
glyph = "f0bc8"
busy_while_running = true
[key.action]
type = "script"
path = "~/bin/backup"          # prints "Copying 43%" lines as it goes
[key.busy]
bar = true
```

No new `Action` variant and no `KINDS` change: a toggle is a key property.

## Core: `probe.rs` and `proc.rs` additions

```rust
// probe.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    Volume(AudioDevice),
    /// `sh -c command` every `interval_ms`. Both fields are the hash key **on purpose**:
    /// the same command at two intervals is two probes, so each key polls at the rate it
    /// asked for. Keys with identical command and interval share one probe.
    Command { command: String, interval_ms: u32 },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value { Level(Level), Toggle(Toggle) }

/// What a toggle key shows. On the wire: `"off"`, `"on"`, `{"busy":"Loading 43%"}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Toggle { #[default] Off, On, Busy(String) }

pub const FAST_INTERVAL: Duration = Duration::from_millis(500); // while busy, or after a slow press
pub const FAST_WINDOW: Duration = Duration::from_secs(15);      // how long a slow press polls fast
pub const SETTLE: Duration = Duration::from_secs(1);            // how long a fast guess wins

impl Probe {
    /// Volume: groundwork's poll (1 s). Command: its own, never under 1 s.
    pub fn interval(&self) -> Duration;
    /// Forced re-reads after a change. Volume: groundwork's (150 ms). Command: 300 ms and 1200 ms.
    pub fn refresh_after(&self) -> &'static [Duration];
}

impl Toggle { pub fn is_off(&self) -> bool; }

/// The ONE builder of a key's status from the watcher's values, like `dial_status`. The
/// daemon, the GUI and `snapshot` call it; PR 9 adds `live`. It covers what `Values`
/// knows (a command's state, a mute); the two per-(page, key) things `Values` can't key
/// (a press toggle's flip and a running job) are laid over it by the daemon's
/// `key_state` (below).
pub fn key_status(key: Option<&KeyConfig>, values: &Values, pressed: bool) -> KeyStatus {
    let state = match key.filter(|k| k.on.is_some()).and_then(KeyConfig::state_probe) {
        Some(p) => match values.probes.get(&p) {
            Some(Value::Toggle(t)) => t.clone(),
            Some(Value::Level(l)) if l.muted => Toggle::On,
            _ => Toggle::Off,
        },
        None => Toggle::Off,
    };
    KeyStatus { pressed, state }
}

/// A state command's exit code: 0 on, 75 busy (with its first output line), anything
/// else (or killed by a signal) off.
pub fn toggle_from_exit(code: Option<i32>, first_line: Option<String>) -> Toggle;
/// 126/127: not executable / not found. Off, but worth one warning (likely a typo).
pub fn is_missing(code: Option<i32>) -> bool;
/// The last `NN%` in a busy text, clamped to 100 (`"shard 2/4 43%"` → 43,
/// `"43.5 %"` → 43, `"120%"` → 100, none → `None`).
pub fn progress(text: &str) -> Option<u8>;
```

`proc.rs` gains a job: a child whose output is read on its own thread, for callers that
must not block (the watcher's state commands and `busy_while_running`). `libc = "0.2"` is
added to `omadeck-core`'s `[dependencies]` for `killpg`; it is already in `Cargo.lock`
(0.2.189), so no new package.

```rust
// imports: std::os::unix::process::{CommandExt, ExitStatusExt} (process_group, from_raw)
pub struct Job {
    child: Child,
    lines: mpsc::Receiver<String>,
    reader_done: Arc<AtomicBool>,
    exited: Option<(ExitStatus, Instant)>,
    started: Instant,
}

const LINE_MAX: usize = 64;                          // chars kept per line
const PIECE_MAX: usize = 4096;                       // bytes buffered per line before cutting
const GRACE: Duration = Duration::from_millis(100);  // wait for output after exit

impl Job {
    /// Spawn in its own process group; stdin and stderr null; stdout split into lines on a
    /// helper thread ("job-out").
    pub fn spawn(mut cmd: Command) -> io::Result<Job> {
        cmd.stdin(Stdio::null()).stderr(Stdio::null()).stdout(Stdio::piped()).process_group(0);
        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, lines) = mpsc::channel();
        let reader_done = Arc::new(AtomicBool::new(false));
        let done = reader_done.clone();
        thread::Builder::new().name("job-out".into()).spawn(move || {
            read_lines(stdout, |l| { let _ = tx.send(l); });
            done.store(true, Ordering::Release);
        })?;
        Ok(Job { child, lines, reader_done, exited: None, started: Instant::now() })
    }
    pub fn started(&self) -> Instant { self.started }
    /// Lines that arrived since the last call.
    pub fn lines(&self) -> mpsc::TryIter<'_, String> { self.lines.try_iter() }
    /// The exit status once the child has exited and its output is read, or GRACE after
    /// exit (a background grandchild can hold the pipe open forever).
    pub fn try_finish(&mut self) -> Option<ExitStatus> {
        if self.exited.is_none() {
            match self.child.try_wait() {
                Ok(Some(s)) => self.exited = Some((s, Instant::now())),
                Ok(None) => return None,
                Err(_) => self.exited = Some((ExitStatus::from_raw(1 << 8), Instant::now())),
            }
        }
        let (s, at) = self.exited?;
        (self.reader_done.load(Ordering::Acquire) || at.elapsed() >= GRACE).then_some(s)
    }
    /// SIGKILL the whole process group, so a pipeline dies too, then reap.
    pub fn kill_group(&mut self) {
        if self.exited.is_none() {
            unsafe { libc::killpg(self.child.id() as libc::pid_t, libc::SIGKILL) };
            let _ = self.child.wait();
            self.exited = Some((ExitStatus::from_raw(libc::SIGKILL), Instant::now()));
        }
    }
}

/// Split on `\n` **and** `\r` (progress bars redraw with `\r`), trim, skip empty pieces,
/// keep LINE_MAX chars. A piece is cut at PIECE_MAX bytes, so nothing grows unbounded.
fn read_lines(mut r: impl Read, mut each: impl FnMut(String)) {
    let mut buf = [0u8; 4096];
    let mut cur: Vec<u8> = Vec::new();
    let mut emit = |cur: &mut Vec<u8>| {
        let s = String::from_utf8_lossy(cur);
        let t = s.trim();
        if !t.is_empty() { each(t.chars().take(LINE_MAX).collect()); }
        cur.clear();
    };
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => for &b in &buf[..n] {
                if b == b'\n' || b == b'\r' { emit(&mut cur) } else if cur.len() < PIECE_MAX { cur.push(b) }
            },
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    emit(&mut cur);
}
```

## The watcher (`omadeckd/src/watch.rs`)

Groundwork's public API stays: `WatchCmd`, `Watcher`, `Watcher::stop`, `start()`. **State commands never
run on the watcher thread**: a slow one would stall the volume reads (and PR 9's clock).
Each run happens on its own short-lived runner thread, which sends its result back as
`WatchCmd::Done` on a clone of the watcher's own command sender. The watcher only keeps
track of what is in flight, so one probe never has two runs. The thread therefore stays
fully blocking (`recv()` / `recv_timeout(next_due - now)`), with no reap tick.

Groundwork already keeps the thread's state in `Poller`, so its logic is tested with
synthetic `Done` messages and no threads. PR 8 adds variants to `WatchCmd`, fields to
`Poller`, and changes `run_due`, `record` and `next_due()` as below.

```rust
pub enum WatchCmd {
    Watch(Vec<Probe>),
    Refresh(Probe),                                            // groundwork's; drop its allow
    Volume { device: AudioDevice, change: i32, mute: bool },   // groundwork's, unchanged
    /// A slow toggle was pressed while showing `from`: poll every FAST_INTERVAL until a
    /// read is neither busy nor `from`, or FAST_WINDOW passes.
    Leave { probe: Probe, from: Toggle },
    /// A runner thread finished. Sent by the runner, not by the daemon.
    Done { probe: Probe, result: RunResult },
    /// Groundwork's: daemon shutdown. Now it also cancels the runners. Needed because the
    /// thread holds a sender itself (for the runners), so its `recv()` never sees a disconnect.
    Stop,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RunResult {
    Exit { code: Option<i32>, first_line: Option<String> },
    TimedOut,
    SpawnFailed(String),
}

const MAX_IN_FLIGHT: usize = 8;                      // runner threads at once
const RUN_POLL: Duration = Duration::from_millis(20); // a runner's try_wait step
#[cfg(not(test))] const STATE_TIMEOUT: Duration = Duration::from_secs(5);
#[cfg(test)]      const STATE_TIMEOUT: Duration = Duration::from_millis(300);

struct Poller {
    // groundwork's fields, unchanged:
    watched: Vec<Probe>,
    last: HashMap<Probe, Value>,                   // last value sent, for change detection
    next_due: HashMap<Probe, Instant>,
    forced: HashSet<Probe>,                        // send the next read even if unchanged
    read_volume: fn(AudioDevice) -> Option<Value>,
    change_volume: fn(AudioDevice, i32, bool),
    // PR 8 adds (each `Default::default()` in groundwork's hand-written `impl Default`):
    refresh_at: Vec<(Instant, Probe)>,             // scheduled forced re-reads (Refresh)
    in_flight: HashMap<Probe, Arc<AtomicBool>>,    // running probes → their cancel flag
    fast_until: HashMap<Probe, (Toggle, Instant)>, // Leave: (state the press left, deadline)
    failing: HashSet<Probe>,                       // warned once; cleared on a good read
}
```

`Refresh` is a *scheduled forced re-read*: the old plan's "`last.remove(&p)` at each
`refresh_after()`" means exactly this. The result is sent even if equal to the last one,
because the deck may be showing an optimistic guess.

```rust
impl Poller {
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Value)>) {
        match cmd {
            WatchCmd::Watch(set) => {
                let mut set_d: Vec<Probe> = Vec::new();
                for p in set { if !set_d.contains(&p) { set_d.push(p); } }
                let keep = |p: &Probe| set_d.contains(p);
                // Dropped probes: tell their runners to kill the command; their Done is ignored.
                self.in_flight.retain(|p, cancel| keep(p) || { cancel.store(true, Ordering::Relaxed); false });
                self.last.retain(|p, _| keep(p));
                self.next_due.retain(|p, _| keep(p));
                self.forced.retain(|p| keep(p));
                self.refresh_at.retain(|(_, p)| keep(p));
                self.fast_until.retain(|p, _| keep(p));
                self.failing.retain(|p| keep(p));
                for p in &set_d { self.next_due.entry(p.clone()).or_insert(now); }
                self.watched = set_d;
            }
            WatchCmd::Refresh(p) => {
                if !self.watched.contains(&p) { return; }
                for d in p.refresh_after() { self.refresh_at.push((now + *d, p.clone())); }
            }
            WatchCmd::Leave { probe, from } => {
                if !self.watched.contains(&probe) { return; }
                self.fast_until.insert(probe.clone(), (from, now + FAST_WINDOW));
                self.next_due.insert(probe, now + FAST_INTERVAL);
            }
            WatchCmd::Volume { .. } => { /* groundwork's arm, unchanged; its read goes through self.record */ }
            WatchCmd::Stop => {}   // groundwork's: the thread loop handles Stop
            WatchCmd::Done { probe, result } => {
                if self.in_flight.remove(&probe).is_none() { return; }   // unwatched meanwhile
                match result {
                    RunResult::Exit { code, .. } if is_missing(code) => {
                        self.warn_once(&probe, "state command not found or not executable");
                        self.record(probe, Value::Toggle(Toggle::Off), now, out);
                    }
                    RunResult::Exit { code, first_line } => {
                        self.failing.remove(&probe);
                        self.record(probe, Value::Toggle(toggle_from_exit(code, first_line)), now, out);
                    }
                    RunResult::TimedOut => {   // send nothing: the key keeps its face
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

    /// Do what is due. Volume reads happen inline (groundwork's); the commands to start
    /// are returned, and the thread loop gives each a runner. (Groundwork's `run_due`,
    /// which returned nothing; PR 8 adds the return value.)
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Value)>) -> Vec<(Probe, Arc<AtomicBool>)> {
        // 1. Due refreshes become forced reads, but not while a run is going: its answer
        //    may predate the change. They wait for its Done.
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
                Probe::Volume(dev) => match (self.read_volume)(*dev) {   // groundwork's inline read
                    Some(v) => self.record(p, v, now, out),
                    None => { self.next_due.insert(p.clone(), now + p.interval()); }
                },
                Probe::Command { .. } => {
                    if self.in_flight.len() >= MAX_IN_FLIGHT {
                        // Not due again until something finishes: a Done wakes the thread.
                        self.next_due.insert(p, now + FAST_INTERVAL);
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

    /// A read landed: end a Leave window if settled, schedule the next read, send on change.
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Value)>) {
        let busy = matches!(v, Value::Toggle(Toggle::Busy(_)));
        // Decide first, then remove: `from` borrows `fast_until`.
        let window_over = self.fast_until.get(&p).is_some_and(|(from, until)| {
            (!busy && v != Value::Toggle(from.clone())) || now >= *until
        });
        if window_over { self.fast_until.remove(&p); }
        let fast = busy || self.fast_until.contains_key(&p);
        self.next_due.insert(p.clone(), now + if fast { FAST_INTERVAL } else { p.interval() });
        let forced = self.forced.remove(&p);
        if forced || self.last.get(&p) != Some(&v) {   // a new busy text counts as a change
            self.last.insert(p.clone(), v.clone());
            out.push((p, v));
        }
    }

    /// When the thread must wake next; `None` = block until a command (or a Done) comes.
    /// Probes in flight don't count: their Done wakes the thread. (Groundwork's
    /// `next_due()`, which took the minimum of every `next_due` entry.)
    fn next_due(&self) -> Option<Instant> {
        let due = self.watched.iter()
            .filter(|p| !self.in_flight.contains_key(*p))
            .filter_map(|p| self.next_due.get(p).copied());
        let refresh = self.refresh_at.iter()
            .filter(|(_, p)| !self.in_flight.contains_key(p))
            .map(|(t, _)| *t);
        due.chain(refresh).min()
    }

    fn warn_once(&mut self, p: &Probe, what: &str) {
        if self.failing.insert(p.clone()) { log::warn!("{what}: {p:?}"); }
    }

    fn cancel_all(&mut self) {
        for c in self.in_flight.values() { c.store(true, Ordering::Relaxed); }
        self.in_flight.clear();
    }
}

/// `sh -c command` with `LC_ALL=C` (stable tool output) in `$HOME`.
fn state_command(command: &str) -> Command {
    let mut c = Command::new("sh");
    c.arg("-c").arg(command).env("LC_ALL", "C")
        .current_dir(std::env::var_os("HOME").unwrap_or_else(|| "/".into()));
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
```

The thread loop is groundwork's `run`, with three changes: it takes a fourth parameter
`runner_tx: mpsc::Sender<WatchCmd>` (`start()` passes a clone of `cmd_tx`), every
`return` calls `poller.cancel_all()` first, and the runners are started after `run_due`.
The rest (`recv`/`recv_timeout` on `poller.next_due()`, the `try_recv` batch, `coalesce`)
stays as groundwork wrote it:

```rust
fn run(cmd_rx: &mpsc::Receiver<WatchCmd>, val_tx: &mpsc::Sender<(Probe, Value)>, mut poller: Poller,
       runner_tx: mpsc::Sender<WatchCmd>) {
    // … groundwork's wait and batch, unchanged, with `poller.cancel_all();` before each `return` …
    // after `poller.run_due(..)`, which now returns the runs to start:
    for (probe, cancel) in poller.run_due(Instant::now(), &mut out) {
        let Probe::Command { command, .. } = &probe else { continue };
        let (command, tx) = (command.clone(), runner_tx.clone());
        let spawned = thread::Builder::new().name("watch-run".into()).spawn(move || {
            let result = run_state(&command, STATE_TIMEOUT, &cancel);
            let _ = tx.send(WatchCmd::Done { probe, result });   // watcher gone: nothing to do
        });
        if let Err(e) = spawned { log::warn!("could not start a runner thread: {e}"); }
    }
    // … groundwork's send of `out`, with `poller.cancel_all();` before its `return` …
}
```

`runner_tx` is a clone of the sender the daemon holds, kept by the thread. So the loop
ends on `WatchCmd::Stop` (groundwork's `Watcher::stop(self)` sends it in
`Daemon::shutdown`, before `saver.finish()`), or when a value send fails. A thread spawn that fails leaves the probe in `in_flight`; that
can only happen when the system is out of threads, and the next `Watch` clears it.

Nothing watched → the thread sleeps in `recv()`. Commands are watched for the current
page only, so a 10-page config runs at most one probe per key on screen plus the dials.

## Daemon (`omadeckd/src/daemon.rs`, `deck.rs`)

### Runtime and the pure helpers

Toggle state lives in `Runtime`, so it survives an unplug and replug, and the jobs keep
being reaped while no deck is attached. Everything in this section goes in `daemon.rs`
unless it says `deck.rs`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId { pub page: Option<String>, pub index: u8 }

pub struct Running { pub job: proc::Job, pub text: String }   // text: the latest output line

#[derive(Debug, Clone, PartialEq)]
enum Settle {
    /// A fast toggle was pressed and shows `value`: ignore reads that contradict it until `until`.
    Expect { value: Value, until: Instant },
    /// A slow toggle was pressed while showing `from`: ignore reads still equal to it until `until`.
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

// Runtime gains (groundwork's `values` and `state` unchanged; it still derives Default):
    pub flipped: HashSet<KeyId>,           // press toggles that are on
    settle: HashMap<Probe, Settle>,        // optimistic guesses waiting for confirmation
    pub jobs: HashMap<KeyId, Running>,     // busy_while_running commands

// Daemon gains, next to groundwork's `published` (panels):
    published_keys: HashMap<KeyId, KeyStatus>,   // what was last sent as Event::KeyState

/// What key `i` on `page` shows now: core's `key_status` (the ONE builder) with the two
/// per-key things `Values` can't hold laid over it. A running job wins; a press toggle
/// reads `flipped`.
fn key_state(cfg: &Config, rt: &Runtime, page: Option<&str>, i: u8, pressed: bool) -> KeyStatus {
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

// core probe.rs, groundwork's function; its `_page` parameter becomes `page`:
/// Everything to poll now: the dials' probes (groundwork's list) then the current page's
/// key probes, without repeats.
pub fn watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize) -> Vec<Probe>;

/// What follows a finished job. Pure apart from `rt`, so it's tested without a deck.
struct JobEnd { failed: Option<String>, refresh: Option<Probe> }
fn finish_job(rt: &mut Runtime, cfg: &Config, id: &KeyId, status: ExitStatus) -> JobEnd {
    rt.jobs.remove(id);
    let ok = status.success();
    let failed = (!ok).then(|| match status.code() {
        Some(c) => format!("exited with status {c}"),
        None => "was killed".to_string(),
    });
    let Some(k) = cfg.key(id.page.as_deref(), id.index) else { return JobEnd { failed, refresh: None } };
    let refresh = match k.state_probe() {
        Some(p @ Probe::Command { .. }) => Some(p),   // ask the real state now; no guess
        Some(Probe::Volume(_)) => None,
        None => {
            // A press toggle flips only when its command succeeded.
            if ok && k.on.is_some() && !rt.flipped.remove(id) { rt.flipped.insert(id.clone()); }
            None
        }
    };
    JobEnd { failed, refresh }
}
```

### Values, repaint and the configurator: `apply_value`

One place maps a `(Probe, Value)` to what it changes: groundwork's
`Daemon::apply_value`, called by `apply_values` (the drain of `watcher.values`) and by
`guess_volume`. PR 8 extends it. (PR 14 later swaps the `==` here and in
`Settle::accepts` for `Value::same_as`; write `==` now.)

```rust
impl Daemon {
    pub fn apply_value(&mut self, probe: Probe, value: Value) {
        if let Some(s) = self.rt.settle.get(&probe) {
            if !s.accepts(&value, Instant::now()) { return; }
            self.rt.settle.remove(&probe);
        }
        if self.rt.values.probes.get(&probe) == Some(&value) { return; }
        self.rt.values.probes.insert(probe.clone(), value);
        self.mark_probe(&probe);
    }

    /// Everything on screen that shows `probe`: the dials (groundwork's marking, moved
    /// here from `apply_value`) and the current page's keys. Nothing is published here:
    /// `flush` compares and publishes, for panels and keys alike.
    fn mark_probe(&mut self, probe: &Probe) {
        if let Some(d) = &mut self.deck {
            d.dirty_where(&self.cfg, |dial| probe::shows(dial, probe));
        }
        let page = self.nav.current().map(str::to_string);   // owned: the calls below take &mut self
        for i in 0..self.key_count() {
            if self.cfg.key(page.as_deref(), i).and_then(KeyConfig::state_probe).as_ref() == Some(probe) {
                self.mark_key(page.as_deref(), i);
            }
        }
    }

    /// Repaint key `i` at the end of this pass, if it is on the deck now. The
    /// configurator hears of it from `publish_keys` in `flush`.
    fn mark_key(&mut self, page: Option<&str>, i: u8) {
        if page == self.nav.current()
            && let Some(d) = &mut self.deck
            && let Some(f) = d.dirty_keys.get_mut(i as usize)
        {
            *f = true;
        }
    }

    fn key_count(&self) -> u8 { self.deck.as_ref().map_or(0, |d| d.info.key_count()) }

    /// What each key of the current page shows now (not pressed), for painting.
    fn key_states(&self) -> Vec<KeyStatus> {
        (0..self.key_count()).map(|i| key_state(&self.cfg, &self.rt, self.nav.current(), i, false)).collect()
    }

    /// Groundwork's pattern for panels, for keys: build each key's status and publish the
    /// ones that differ from what was last sent. Called first thing in `flush`, before its
    /// `let Some(d) = &mut self.deck else { return };`, so a job's progress reaches the
    /// configurator with no deck too. That covers watcher values, guesses, jobs, connect,
    /// reload and page change in one place.
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
}
```

`deck.rs`: `Deck` gains `dirty_keys: Vec<bool>` (len `info.key_count()`), painted by
`flush_dirty` next to `dirty_panels` (one device flush for both); `paint_all` clears it.
`key_state` needs `Runtime` (flips and jobs), which `deck.rs` doesn't see, so the daemon
passes the statuses in: `write_key` takes `st: &KeyStatus` in place of `pressed: bool`;
`paint_all` and `flush_dirty` gain a last parameter `keys: &[KeyStatus]` (from
`self.key_states()`, indexed by key); `paint_key` takes `st: &KeyStatus` in place of
`pressed` (the daemon's `paint_key(i, pressed)` builds it with
`key_state(&self.cfg, &self.rt, self.nav.current(), i, pressed)`). Panels keep
`values: &Values`.

### A press

The `ButtonDown` arm keeps groundwork's pressed paint and `KeyDown` publish, and calls
`press_key(i)` where it called `run_input(Source::Key(i), ..)`.

```rust
fn press_key(&mut self, i: u8) {
    let page = self.nav.current().map(str::to_string);
    // Clone: everything below takes &mut self, and cfg.key() borrows self.cfg.
    let Some(k) = self.cfg.key(page.as_deref(), i).cloned() else { return };
    let id = KeyId { page: page.clone(), index: i };
    if self.rt.jobs.contains_key(&id) {
        log::info!("key {i}: still running, press ignored");
        return;
    }
    let before = key_state(&self.cfg, &self.rt, page.as_deref(), i, false).state;
    let act = k.action_for(before == Toggle::On).clone();   // busy runs the base action
    if k.busy_while_running && act.is_trackable() {
        match action::run_tracked(&act) {
            Ok(job) => {
                self.rt.jobs.insert(id, Running { job, text: String::new() });
                self.mark_key(page.as_deref(), i);
            }
            Err(e) => self.hub.publish(Event::ActionFailed { index: i, page, message: format!("{e:#}") }),
        }
        return;   // a toggle's state catches up when the job ends (finish_job)
    }
    if !self.run_input(Source::Key(i), &act, 1) || k.on.is_none() { return; }
    self.after_toggle_press(&id, &k, before);
}

/// Show the press at once, as Elgato does, then let the real state confirm it.
fn after_toggle_press(&mut self, id: &KeyId, k: &KeyConfig, before: Toggle) {
    let now = Instant::now();
    match k.state_probe() {
        None => {
            if !self.rt.flipped.remove(id) { self.rt.flipped.insert(id.clone()); }
            self.mark_key(id.page.as_deref(), id.index);
        }
        Some(Probe::Volume(_)) => {}   // the watcher runs the change and re-reads (D5)
        Some(p) if matches!(before, Toggle::Busy(_)) => {
            let _ = self.watcher.cmd.send(WatchCmd::Refresh(p));   // no guess while busy
        }
        Some(p) if k.is_slow_toggle() => {
            self.rt.values.probes.insert(p.clone(), Value::Toggle(Toggle::Busy(String::new())));
            self.rt.settle.insert(p.clone(), Settle::Leaving { from: before.clone(), until: now + FAST_WINDOW });
            let _ = self.watcher.cmd.send(WatchCmd::Leave { probe: p.clone(), from: before });
            self.mark_probe(&p);
        }
        Some(p) => {
            let guess = Value::Toggle(if before == Toggle::On { Toggle::Off } else { Toggle::On });
            self.rt.values.probes.insert(p.clone(), guess.clone());
            self.rt.settle.insert(p.clone(), Settle::Expect { value: guess, until: now + SETTLE });
            let _ = self.watcher.cmd.send(WatchCmd::Refresh(p.clone()));
            self.mark_probe(&p);
        }
    }
}
```

The second refresh read (1.2 s) is outside the 1 s `Expect` window, so a fast toggle that
really failed shows its true state within ~1.2 s. A slow toggle that never starts shows
its true state after 15 s.

### Jobs each pass

`poll_jobs()` runs every pass, **with or without a deck**: in `main.rs`'s `run()`, call
`daemon.poll_jobs()` right after `daemon.apply_values()`, and in the no-deck branch call
`daemon.flush()` before it scans or sleeps (with no deck, `flush` only runs
`publish_keys`, so a job's progress still reaches the configurator):

```rust
fn poll_jobs(&mut self) {
    let mut changed = Vec::new();
    let mut done = Vec::new();
    for (id, r) in self.rt.jobs.iter_mut() {
        if let Some(line) = r.job.lines().last() && line != r.text {
            r.text = line;
            changed.push(id.clone());
        }
        if let Some(status) = r.job.try_finish() { done.push((id.clone(), status)); }
    }
    for id in changed { self.mark_key(id.page.as_deref(), id.index); }
    for (id, status) in done {
        let end = finish_job(&mut self.rt, &self.cfg, &id, status);
        if let Some(message) = end.failed {
            self.hub.publish(Event::ActionFailed { index: id.index, page: id.page.clone(), message });
        }
        if let Some(p) = end.refresh { let _ = self.watcher.cmd.send(WatchCmd::Refresh(p)); }
        self.mark_key(id.page.as_deref(), id.index);
    }
}
```

No change to `next_deadline()`: passes already run at least every `READ_TIMEOUT`
(100 ms), which is the job poll rate.

### Connect, reload, page change

- **Groundwork's `watch(&mut self)`** already sends
  `WatchCmd::Watch(probe::watched_probes(&self.cfg, self.nav.current(), dials))` from
  `attach` and `reload`; PR 8 also calls it from `show_page` (**every page change**; new,
  since groundwork's set was page-independent). Its `rt.values.probes.retain` goes (the
  reload rule below replaces it, and page changes must keep values). `detach` still sends
  `Watch(vec![])`.
- **The configurator** hears about keys from `publish_keys` in `flush` (above): after
  connect, reload and page change the next pass publishes whatever differs, so it is
  never stale and nothing extra is called.
- **Reload**: `rt.flipped.retain(|id| cfg.key(id.page.as_deref(), id.index).is_some_and(KeyConfig::is_press_toggle))`;
  `rt.settle.retain` to probes in the new watched set; `rt.values.probes.retain` to the
  dials' probes plus every key's `state_probe()` on **any** page; jobs are kept (a job
  whose key lost the flag is still reaped, its `KeyState` just shows the key's face
  again when it ends).
- **Page change**: values are not pruned, so a page you return to shows the last known
  faces at once and corrects on the first read (no flash to off).

`omadeckd press KEY` with no daemon keeps running the base action detached (D5); it can't
know the state.

## Busy while running (core `action.rs`)

```rust
/// Start `action` as a job the daemon watches. Only for `is_trackable()` kinds.
/// Not through `uwsm-app`: it hands the command to systemd and returns at once, so there
/// would be nothing to wait for. Instead the job gets its own scope (systemd-run --scope
/// execs the command, so the child *is* the job), so stopping the daemon doesn't kill
/// it; SIGPIPE is ignored so it keeps running when the daemon (the pipe's reader) goes.
pub fn run_tracked(action: &Action) -> Result<proc::Job>;

/// The argv `run_tracked` runs, pure for tests. `scope`: systemd-run is on PATH.
pub fn tracked_argv(argv: &[String], scope: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if scope {
        out.extend(["systemd-run", "--user", "--scope", "--collect", "--quiet", "--"].map(String::from));
    }
    out.extend(["sh", "-c", "trap '' PIPE; exec \"$@\"", "sh"].map(String::from));
    out.extend(argv.iter().cloned());
    out
}
```

`run_tracked` builds `argv` the way `launch_app` (plain command: `sh -c` if
`needs_shell`, else `shell_words::split`; cwd `$HOME`) and `run_script` (executable check,
`terminal = true` wraps in the terminal; cwd the script's directory) do today, then
`proc::Job::spawn(Command::new(&a[0]).args(&a[1..]))` on `tracked_argv(&argv,
which("systemd-run"))`. Cache the `which` in a `OnceLock` as `detached` does. A spawn
on the loop costs what today's `spawn` costs.

## IPC (`omadeck-core/src/ipc.rs`)

```rust
    /// An action failed to start, or a tracked command exited non-zero. `page` is where
    /// the key is: a job can end after the deck moved on.
    ActionFailed {
        index: u8,
        #[serde(default)]
        page: Option<String>,
        message: String,
    },
    /// What a key with a toggle, or with a running command, shows now. Published on each
    /// change (each new progress text too) and for the current page after connect,
    /// reload and page change; replayed to new clients.
    KeyState {
        #[serde(default)]
        page: Option<String>,
        index: u8,
        status: KeyStatus,   // `pressed` is always false here, so it is never written
    },
```

Decided: `ActionFailed` gains `page`, because `busy_while_running` jobs outlive a page
change. Old configurators ignore the extra field; a new configurator reading an old
daemon gets `None`. `run_input` fills it from `nav.current()`.

Wire: `{"event":"key_state","page":null,"index":3,"status":{"state":"on"}}`, and
`"status":{"state":{"busy":"Loading 43%"}}`. `KeyStatus` is a struct so PR 9's `live`
(and later fields) join it with `serde(default, skip_serializing_if)` and old
configurators ignore them.

Hub (`hub.rs`): groundwork's `Replay` gains `keys: BTreeMap<(Option<String>, u8), Event>`
next to `panels`. `Replay::remember` of a `KeyState` inserts it, or removes the entry
when `status == KeyStatus::default()`; `Replay::lines` appends every entry after the
panels.

## Render (`omadeck-core/src/render.rs`)

```rust
/// What a key looks like right now, beside its config. Mirrors `DialStatus`; built only
/// by `probe::key_status`. Serializable for `Event::KeyState`; every field is
/// `serde(default)` and skipped when default, so fields can be added (PR 9: `live`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct KeyStatus {
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pressed: bool,
    #[serde(default, skip_serializing_if = "Toggle::is_off")]
    pub state: Toggle,
}

pub fn render(&mut self, key: Option<&KeyConfig>, style: &Style, size: u32, st: &KeyStatus) -> RgbaImage;
```

Re-export `KeyStatus` from `lib.rs` next to `DialStatus`.

Every caller changes (check with `grep -rn '\.render(' crates`; fix any extra the same way):

| Caller | New argument |
|---|---|
| daemon `write_key` (groundwork's paint path) | `&key_state(&self.cfg, &self.rt, page, i, pressed)` |
| daemon `snapshot` | `&probe::key_status(key, &Values::default(), false)` (off faces; see Later) |
| GUI `App::refresh_key` | `&self.preview_status(i)` (Task H) |
| render test `key_glyph_stays_in_its_box` | `&KeyStatus::default()` |
| render test `strip_empty_slot_matches_empty_key` | `&KeyStatus::default()` |
| groundwork's golden key tests, if any | `&KeyStatus::default()` |

Inside `render`:
- The face is `k.face_for(&st.state)`.
- **On** with no `on.background`: the background is mixed `ON_TINT = 0.30` towards the
  accent (`theme.accent` with `follow_theme`, else the label colour), so a bare
  `[key.on]` is still visibly different.
- **Busy** with no `busy.label_color` and no key `label_color`: the label is
  `theme.accent` under `follow_theme` (put `self.theme.accent.hex()` in a local `String`
  and point `face.label_color` at it; `Face` is covariant, so the shorter borrow is fine).
- **Bar**: when busy, `busy.bar` is set and `progress(text)` is `Some(p)`: render the face
  at `inner = size × (1 − BAR_ZONE)` (`BAR_ZONE = 0.22`), overlay it at the top centre of
  a `solid(size, bg)`, then draw a bar at `x = 0.10·s, w = 0.80·s, h = max(4, 0.07·s)`,
  bottom margin `0.08·s`, with groundwork's `draw_bar(img, rect, p as f32 / 100.0, fill,
  track)`. Colours: the fill and track `draw_b1` passes to `draw_bar` at the base, with
  the busy label colour in the place of `$B1`'s value colour. No second bar drawer.

- The pressed ring draws over any state, as today. No spinner: it would need a repaint
  timer for nothing the text doesn't already say.

## GUI (`omadeck-gui`)

- `App.key_states: RefCell<HashMap<(Option<String>, u8), KeyStatus>>`, filled from
  `Event::KeyState` (insert, or remove when it equals `KeyStatus::default()`). Cleared on `Msg::DaemonDown`, then
  `refresh_all()`: with no daemon, no states (D2).
- `App.face_tab: Cell<FaceTab>`: which face the editor edits; reset to `Off` in `select()`
  when the selection changes.
- `App::preview_status(&self, i) -> KeyStatus`: the selected key with a tab other than Off
  previews that tab (`FaceTab::preview()`: On → `On`, Busy → `Busy("Loading 42%")`);
  otherwise the mirrored `key_states` entry for `(current_page, i)`; with no entry,
  `probe::key_status(key, &Values::default(), false)` (the one builder; no daemon means
  no values, D2).
- `ActionFailed` toasts "Key N: …" on the current page, "Key N on Page: …" otherwise.
- Key editor, **Appearance** group: linked **Off | On | Busy** toggle buttons in the
  header (On for toggles; Busy for slow toggles and `busy_while_running` keys). The rows
  (`appearance_rows`) edit the chosen tab through `KeyConfig::look`/`set_look`; "Show
  label on key" only on Off. On/Busy row titles say what an empty value means ("Label
  while on (empty: the key's label)", "Label while busy (empty: the command's output)").
- **Toggle** group under the action group:
  - **Two faces (toggle)** switch, adding or removing `[key.on]`;
  - **State from** combo, offering `KeyConfig::state_choices()`: *Flips on each press* /
    *Shell command* for most keys, *Follows mute* / *Shell command* for a volume mute key;
  - for *Shell command*: a command `EntryRow` (hint "exit 0 = on, 75 = busy, else off"),
    an **Every (seconds)** spin 1–60, a **Slow (shows progress)** switch (`on.slow`);
  - **Progress bar** switch (`busy.bar`, creating `[key.busy]`), shown for slow toggles
    and `busy_while_running` keys;
  - **When pressed while on**: `action_editor(app, &group, "When pressed while on", &slot)`
    with `slot.target = ActionTarget::KeyOn(KeyRef)` (default Nothing = same action).
- **Busy while running** switch in the key's action group, shown only when
  `action.is_trackable()`, with the hint "show the busy face until the command finishes".
- **Presets** menu button in the key group header, beside ▶ and 🗑 (Task J).
- ▶ *Try it now* runs `action_for(face_tab == On)`.

Core helpers for the GUI (pure, tested in core):

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum FaceTab { #[default] Off, On, Busy }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LookField { Label, Icon, Glyph, Background, LabelColor }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChoice { Press, Command, Mute }

impl FaceTab { pub fn preview(self) -> Toggle; }
impl KeyConfig {
    /// The field as set on that tab's face; `None` when unset there.
    pub fn look(&self, tab: FaceTab, field: LookField) -> Option<String>;
    /// Set it; `None` or blank clears it (Off's label becomes ""). On/Busy create their
    /// table if missing.
    pub fn set_look(&mut self, tab: FaceTab, field: LookField, v: Option<String>);
    pub fn state_choices(&self) -> &'static [StateChoice];
    /// `on.state` set (even blank) → Command; else Mute for a volume mute action; else Press.
    pub fn state_choice(&self) -> StateChoice;
    /// Command keeps any old text, else `Some("")`; Press/Mute clear `state` and `slow`.
    pub fn set_state_choice(&mut self, c: StateChoice);
}
```

Known GUI traps: clone out of `config.borrow()` before calling `edit`/`edit_ref` (BUGS.md
4); every closure handed to `action_editor` is an `Rc<dyn Fn>` in an `ActionSlot`, never
`impl Fn`; `rebuild_editor()` only from switches and combos, never from an `EntryRow`'s
`connect_changed`; drop `key_states.borrow_mut()` before `refresh_key` (it borrows again).

## Presets (`omadeck-core/src/presets.rs`, new; GUI menu)

```rust
pub struct Preset {
    pub name: &'static str,
    /// A binary that must be on PATH, else the menu item is insensitive with the tooltip
    /// "needs <bin>".
    pub needs: Option<&'static str>,
    pub apply: fn(&mut KeyConfig),
}
pub const PRESETS: &[Preset];
pub fn on_path(bin: &str) -> bool;          // the same test as action.rs `which`
/// The Service preset: a slow toggle for a systemd user unit, optional health URL.
/// `Err` when `unit` has characters outside `[A-Za-z0-9@._:-]`.
pub fn apply_service(k: &mut KeyConfig, unit: &str, health_url: Option<&str>) -> Result<()>;
```

- **Mic mute**: action `Volume { Input, mute }`, label "Mic", glyph `f036c`; on: "Muted",
  glyph `f036d`. No `needs`.
- **Output mute**: action `Volume { Output, mute }`, label "Sound", glyph `f057e`; on:
  "Muted", glyph `f075f`. No `needs`.
- **Service…**: a small `adw::AlertDialog` with two entries (unit, optional health URL);
  writes the slow-toggle example above (`slow = true`). The URL is quoted with
  `shell_words::quote`. Without a URL, `active) exit 0 ;;`.
- **Night light, Stay awake, Do Not Disturb, Bluetooth**: only from the commands captured
  on the machine (below). Until captured, they are not in `PRESETS`.

## Starter layout (`omadeck-gui/src/starter.rs`)

Key 10 "Mute" becomes `Volume { Output, mute }` with the Output mute preset's on face. Key
11 "Night light" gets the night light preset's on face and state once captured (else it
stays as it is). New installs only.

## Docs

README Features ("Toggle keys: a second face that follows real state, with progress for
slow toggles, and a busy face while a key's command runs"), tick the roadmap's **Toggle
keys**, a short README section on writing a state command (exit 0 / other / 75 and the
progress line, `NN%` for the bar), `examples/config.toml` gets the mic and backup
examples plus commented service and VPN (`nmcli -t -f TYPE connection show --active |
grep -q vpn`) examples, a CHANGELOG line.

## Capture on the machine first

Run these on the Omarchy machine and paste the output into a "Captured" section here
before Task J. Don't guess the output.

1. Which Omarchy commands exist (the starter already uses the `omarchy toggle nightlight`
   form): `compgen -c | grep -E '^omarchy' | sort -u | grep -Ei 'toggle|nightlight|idle|notif|bluetooth'`
   and `omarchy --help 2>&1 | head -40`.
2. Night light state, with it on and then off:
   `omarchy-toggle-nightlight --status; echo "exit=$?"` and
   `hyprctl hyprsunset temperature; echo "exit=$?"`.
3. Stay awake (idle) state, on and off: `omarchy-toggle-idle --status; echo "exit=$?"`
   and `pgrep -x hypridle; echo "exit=$?"`.
4. Do Not Disturb, on and off: `makoctl mode; echo "exit=$?"`,
   `cat ~/.local/state/omarchy/notifications.json 2>&1`, `ls ~/.local/state/omarchy/`.
5. Bluetooth, on and off: `time omarchy-bluetooth-power is-on; echo "exit=$?"` and
   `bluetoothctl show | grep Powered`.
6. Generic flags: `ls ~/.local/state/omarchy/toggles/ 2>&1` and
   `command -v omarchy-toggle omarchy-toggle-enabled`.
7. Tools the presets use: `command -v curl jq systemd-run uwsm-app`.
8. A scope passes the exit status and stdout through:
   `systemd-run --user --scope --collect --quiet -- sh -c 'echo hi; exit 3'; echo "exit=$?"`
   (expect `hi` then `exit=3`).
9. Whether the terminal waits for its child (for `terminal = true` jobs):
   `time xdg-terminal-exec sh -c 'sleep 3'; echo "exit=$?"` (≈3 s = waits; ≈0 s = hands off).

From these, write the preset rows as `(name, needs, action command, state command)`.
Rule for the state command: exit 0 exactly when on; prefer a file test or a fast query
over anything that can take seconds.

## Implementation steps

Each task is one session, ~150–300 lines with tests, and ends with its check. Commit after
each. `cargo fmt` before every commit.

1. [ ] **Task A — core probe + proc (~220).** Files: `omadeck-core/src/probe.rs`,
   `proc.rs`, `Cargo.toml` (`libc = "0.2"`). Add `Probe::Command`, `Value::Toggle`,
   `Toggle` (with serde), the three constants, `Probe::interval`/`refresh_after` arms,
   `toggle_from_exit`, `is_missing`, `progress`, `key_status` (with `KeyStatus` in
   `render.rs`, moved here from Task C); `Job` and `read_lines`.
   Every `match` on `Probe`/`Value` elsewhere gains an arm (the compiler lists them; a
   `Toggle` value on a dial shows no level). Tests: `progress_parses_last_percentage`,
   `key_status_wire_shape` (`KeyStatus::default()` → `{}`, On → `{"state":"on"}`,
   Busy → `{"state":{"busy":"x"}}`, and `{}` parses back), `exit_codes_map_to_toggles`, `read_lines_splits_on_cr_and_lf`,
   `job_reports_exit_and_lines` (`sh -c 'echo a; exit 75'`, poll `try_finish` ≤ 2 s),
   `job_with_background_child_still_finishes` (`sh -c 'sleep 5 & echo x'` finishes within
   GRACE + 1 s), `kill_group_kills_a_pipeline` (`sleep 30 | cat`, `kill_group`, then
   `try_finish` is `Some`).

   ```rust
   #[test]
   fn read_lines_splits_on_cr_and_lf() {
       let mut got = Vec::new();
       read_lines(&b"one\r two \n\n\rthree"[..], |l| got.push(l));
       assert_eq!(got, ["one", "two", "three"]);
       let mut long = Vec::new();
       read_lines(&[b'x'; 10_000][..], |l| long.push(l));
       assert_eq!(long.len(), 1);
       assert_eq!(long[0].chars().count(), 64, "cut to LINE_MAX");
   }
   ```

   **Check:** `cargo test -p omadeck-core -- probe proc` → `test result: ok`, the seven
   tests above listed as `ok`; `cargo build --workspace` clean.

2. [ ] **Task B — config (~260).** File: `config.rs`. `OnFace` (manual `Default`),
   `BusyFace`, the three `KeyConfig` fields in the order given, `new`, `is_blank`,
   `state_probe`, `is_press_toggle`, `is_slow_toggle`, `action_for`, `merge`, `face_for`,
   `Action::is_trackable`. Tests: `toggle_roundtrip_and_defaults`,
   `old_config_writes_no_toggle_fields` (`examples/config.toml` before this PR's docs
   task: parse, `toml::to_string_pretty`, parse again, equal, and no `[key.on]`,
   `[key.busy]`, `slow`, `interval` or `busy_while_running` in the output),
   `key_with_only_on_is_not_blank`,
   `state_probe_command_wins_over_mute`, `state_probe_mute_from_volume_action`,
   `blank_state_is_press_toggle`, `interval_clamped_to_one_second` (`interval = 0` →
   `interval_ms: 1000`), `face_on_image_replaces_both`, `face_on_empty_label_hides_label`,
   `face_for_busy_label_order` (busy.label, then text, then "…"),
   `action_for_uses_on_action_only_when_on`, `slow_without_state_is_not_slow`,
   `trackable_kinds` (plain app and script yes; `.desktop`, url, volume no).

   ```rust
   #[test]
   fn toggle_roundtrip_and_defaults() {
       let src = r#"
   [[key]]
   index = 11
   label = "Night light"
   [key.action]
   type = "app"
   command = "night-toggle"
   [key.on]
   label = "Night on"
   state = "test -f /tmp/night"
   slow = true
   "#;
       let c = Config::parse(src).unwrap();
       let k = c.key(None, 11).unwrap();
       assert_eq!(k.on.as_ref().unwrap().interval, 2);
       assert_eq!(k.state_probe(), Some(Probe::Command { command: "test -f /tmp/night".into(), interval_ms: 2000 }));
       assert!(k.is_slow_toggle());
       assert!(!k.busy_while_running);
       let out = toml::to_string_pretty(&c).unwrap();
       assert!(!out.contains("interval"), "the default interval is not written");
       assert!(!out.contains("busy"), "no busy table or flag when unset");
       assert_eq!(Config::parse(&out).unwrap(), c);
   }
   ```

   **Check:** `cargo test -p omadeck-core config` → `test result: ok`, all of the above
   plus the existing `example_parses` and `roundtrip`.

3. [ ] **Task C — render (~230).** Files: `render.rs`, `lib.rs`, the callers in the
   table (daemon and GUI mechanically: `&KeyStatus { pressed, state: Toggle::Off }` for
   now; Tasks F and H fill the state). The new `render`, `ON_TINT`, the busy accent,
   `BAR_ZONE` and the key bar through groundwork's `draw_bar`. Tests:
   `off_face_ignores_on_table`, `on_face_without_background_is_tinted`,
   `busy_face_uses_accent_label_by_default` (with `follow_theme`, the label colour
   resolves to `theme.accent`), `busy_bar_only_when_enabled_and_percent_present` (bar off
   + `43%` → bar row equals background; bar on + `Starting` → no bar; bar on + `50%` →
   the pixel at 30% of the bar's width is the fill colour and at 80% the track colour,
   size 100, rows 85–91).

   ```rust
   #[test]
   fn off_face_ignores_on_table() {
       let style = test_style();
       let mut r = Renderer::new(&style);
       let plain = KeyConfig { label: "Mic".into(), glyph: Some("A".into()), ..KeyConfig::new(0) };
       let toggle = KeyConfig {
           on: Some(OnFace { label: Some("Muted".into()), ..OnFace::default() }),
           ..plain.clone()
       };
       let off = KeyStatus::default();
       assert_eq!(r.render(Some(&plain), &style, 72, &off), r.render(Some(&toggle), &style, 72, &off));
       let on = KeyStatus { state: Toggle::On, ..KeyStatus::default() };
       assert_ne!(r.render(Some(&toggle), &style, 72, &on), r.render(Some(&plain), &style, 72, &off),
                  "the on face is tinted even without a font");
   }
   ```

   **Check:** `cargo test -p omadeck-core render` → `test result: ok`, including the
   four new tests and the existing strip tests; `cargo build --workspace` clean.

4. [ ] **Task D — watcher (~300).** File: `omadeckd/src/watch.rs`. `WatchCmd::Leave`,
   `Done`, `RunResult`, the constants, the new `Poller` fields, `handle`, `run_due` (now
   returning the runs to start), `record`, `next_due`, `warn_once`, `cancel_all`,
   `state_command`, `run_state`, and `run`'s `runner_tx` and `cancel_all` calls.
   Groundwork's `Stop`, `Watcher::stop`, `coalesce` and volume code stay unchanged.
   Poller tests use synthetic `Done` messages, no threads: `exit_75_is_busy_with_first_line`,
   `exit_0_on_other_off`, `busy_text_change_is_resent`, `unchanged_value_not_resent`,
   `refresh_resends_unchanged`, `refresh_waits_for_the_run_in_flight`,
   `leave_polls_fast_until_settled`, `timeout_sends_nothing_and_warns_once`,
   `done_for_an_unwatched_probe_is_dropped`, `in_flight_is_never_started_twice`,
   `nothing_watched_means_no_wake`. Runner tests with real `sh`:
   `run_state_reads_exit_and_first_line`, `run_state_kills_a_hung_command`
   (`sleep 30 | cat`, STATE_TIMEOUT is 300 ms under test, returns `TimedOut` in < 1 s),
   `run_state_cancel_kills`, `large_output_does_not_block`
   (`yes | head -c 1000000; exit 0` → `Exit { code: Some(0), .. }`).

   ```rust
   fn cmd(c: &str) -> Probe { Probe::Command { command: c.into(), interval_ms: 2000 } }

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
       let result = RunResult::Exit { code: Some(75), first_line: Some("Loading 43%".into()) };
       p.handle(WatchCmd::Done { probe: probe.clone(), result }, t0, &mut out);
       assert_eq!(out, vec![(probe.clone(), Value::Toggle(Toggle::Busy("Loading 43%".into())))]);
       assert_eq!(p.next_due(), Some(t0 + FAST_INTERVAL), "busy polls fast");
   }
   ```

   **Check:** `cargo test -p omadeckd watch` → `test result: ok`, the fifteen tests listed;
   `cargo clippy -p omadeckd --all-targets -- -D warnings` clean. Re-run groundwork's
   volume hardware checks (levels, mute, external changes within ~1 s).

5. [ ] **Task E — IPC, hub and daemon types (~220).** Files: `ipc.rs`, `hub.rs`,
   `omadeckd/src/daemon.rs`. `Event::KeyState`, `ActionFailed.page` (fix its publishers:
   `run_input` fills `page` from `nav.current()`; in the GUI's exhaustive `App::handle`
   match, the `ActionFailed` arm gains `page: _` and a `Event::KeyState { .. } => {}` arm
   is added, both filled in Task H), the hub's `Replay.keys`, `KeyId`, `Running`, `Settle` +
   `accepts`, the `Runtime` fields, `key_state`. Tests: `key_state_wire_shape`,
   `settle_expect_ignores_contradiction_until_expiry`,
   `settle_leaving_takes_busy_and_new_state`, `key_state_precedence`.

   ```rust
   // ipc.rs
   #[test]
   fn key_state_wire_shape() {
       let on = KeyStatus { state: Toggle::On, ..KeyStatus::default() };
       assert_eq!(
           Event::KeyState { page: None, index: 3, status: on }.to_line(),
           "{\"event\":\"key_state\",\"page\":null,\"index\":3,\"status\":{\"state\":\"on\"}}\n"
       );
       let busy = KeyStatus { state: Toggle::Busy("Loading 43%".into()), ..KeyStatus::default() };
       roundtrip(&Event::KeyState { page: Some("Media".into()), index: 0, status: busy });
       let old: Event = serde_json::from_str("{\"event\":\"action_failed\",\"index\":2,\"message\":\"x\"}").unwrap();
       assert_eq!(old, Event::ActionFailed { index: 2, page: None, message: "x".into() });
   }

   // daemon.rs
   #[test]
   fn key_state_precedence() {
       let cfg = Config::parse("[[key]]\nindex = 0\n[key.on]\nstate = \"true\"\n\n[[key]]\nindex = 1\n[key.on]\n").unwrap();
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
   ```

   **Check:** `cargo test -p omadeck-core ipc` and `cargo test -p omadeckd -- key_state settle`
   → `test result: ok`; `cargo build --workspace` clean (the GUI compiles with the
   placeholder arms).

6. [ ] **Task F — daemon wiring (~280).** Files: `omadeck-core/src/probe.rs`,
   `omadeckd/src/daemon.rs`, `deck.rs`. `probe::watched_probes` (page parameter), `watch()`
   from `show_page`, `published_keys` and `publish_keys` in `flush`, `apply_value`,
   `mark_probe`, `mark_key`, `key_count`, `key_states`, `Deck.dirty_keys` and its flush,
   `write_key`/`paint_key`/`paint_all`/`flush_dirty` with the real state, `press_key`,
   `after_toggle_press`, the reload and page-change hooks. Tests (the first in
   `probe.rs`, the second in `daemon.rs`): `watched_probes_current_page_dials_and_dedup`,
   `reload_drops_flips_of_keys_that_are_no_longer_press_toggles` (pure: the `retain`
   expression as a helper `fn retain_flips(cfg, &mut HashSet<KeyId>)`).

   ```rust
   #[test]
   fn watched_probes_current_page_dials_and_dedup() {
       let cfg = Config::parse(r#"
   [[key]]
   index = 0
   [key.action]
   type = "volume"
   mute = true
   [key.on]

   [[key]]
   index = 1
   [key.on]
   state = "true"

   [[key]]
   index = 2
   [key.on]
   state = "true"

   [[page]]
   name = "Other"
   [[page.key]]
   index = 0
   [page.key.on]
   state = "false"

   [[dial]]
   index = 0
   [dial.turn_right]
   type = "volume"
   change = 5
   "#).unwrap();
       let cmd = |c: &str| Probe::Command { command: c.into(), interval_ms: 2000 };
       assert_eq!(watched_probes(&cfg, None, 4), vec![Probe::Volume(AudioDevice::Output), cmd("true")]);
       assert_eq!(watched_probes(&cfg, Some("Other"), 4), vec![Probe::Volume(AudioDevice::Output), cmd("false")]);
       assert_eq!(watched_probes(&cfg, Some("Other"), 0), vec![cmd("false")], "no dials, no dial probes");
   }
   ```

   **Check:** `cargo test -p omadeckd` → `test result: ok`;
   `cargo clippy --all-targets -- -D warnings` clean. Hardware: mic mute key follows the
   mute from the key, the dial and the keyboard; a `test -f /tmp/t` toggle flips at once
   on press and follows `touch`/`rm` within ~2 s.

7. [ ] **Task G — busy while running (~230).** Files: `action.rs`, `omadeckd/src/daemon.rs`.
   `run_tracked`, `tracked_argv`, the `press_key` branch (already written in Task F; now
   reachable), `poll_jobs`, `finish_job`, `JobEnd`. Tests:
   `tracked_argv_uses_a_scope_never_uwsm`, `run_tracked_streams_lines`
   (`for i in 1 2 3; do echo "$i%"; done` → lines in order, exit 0),
   `job_on_toggle_key_settles_after_exit`.

   ```rust
   // daemon.rs: busy_while_running on toggle keys
   #[test]
   fn job_on_toggle_key_settles_after_exit() {
       use std::os::unix::process::ExitStatusExt;
       let cfg = Config::parse(r#"
   [[key]]
   index = 0
   busy_while_running = true
   [key.action]
   type = "app"
   command = "sh -c 'exit 0'"
   [key.on]

   [[key]]
   index = 1
   busy_while_running = true
   [key.action]
   type = "app"
   command = "make-it-so"
   [key.on]
   state = "test -f /tmp/so"
   "#).unwrap();
       let mut rt = Runtime::default();
       let press = KeyId { page: None, index: 0 };
       // Exit 1: reported, no flip.
       let end = finish_job(&mut rt, &cfg, &press, ExitStatus::from_raw(1 << 8));
       assert_eq!(end.failed.as_deref(), Some("exited with status 1"));
       assert!(!rt.flipped.contains(&press));
       // Exit 0: a press toggle flips; a second success flips back.
       assert!(finish_job(&mut rt, &cfg, &press, ExitStatus::from_raw(0)).failed.is_none());
       assert!(rt.flipped.contains(&press));
       finish_job(&mut rt, &cfg, &press, ExitStatus::from_raw(0));
       assert!(!rt.flipped.contains(&press));
       // A command toggle: no guess, no settle, just a forced re-read.
       let cmd = KeyId { page: None, index: 1 };
       let end = finish_job(&mut rt, &cfg, &cmd, ExitStatus::from_raw(0));
       assert_eq!(end.refresh, Some(Probe::Command { command: "test -f /tmp/so".into(), interval_ms: 2000 }));
       assert!(rt.settle.is_empty());
       assert!(rt.values.probes.is_empty());
   }
   ```

   **Check:** `cargo test -p omadeck-core action` and `cargo test -p omadeckd job` →
   `test result: ok`; clippy clean. Hardware: the `busy_while_running` checks below.

8. [ ] **Task H — GUI mirror and face tabs (~280).** Files: `config.rs` (`FaceTab`,
   `LookField`, `look`, `set_look`, `FaceTab::preview`), `omadeck-gui/src/app.rs`,
   `editor.rs`. `key_states`, the `KeyState` arm, `DaemonDown` clearing, the
   `ActionFailed` toast with page, `face_tab`, `preview_status`, `refresh_key` with the
   state; extract `appearance_rows(app, group, i, tab)` from `build_key`'s Appearance
   rows, retargeted through `set_look`; the Off | On | Busy buttons. Test (core):
   `set_look_targets_each_tab`.

   ```rust
   #[test]
   fn set_look_targets_each_tab() {
       let mut k = KeyConfig { label: "Mic".into(), ..KeyConfig::new(0) };
       k.set_look(FaceTab::On, LookField::Label, Some("Muted".into()));
       assert_eq!(k.on.as_ref().unwrap().label.as_deref(), Some("Muted"), "On creates [key.on]");
       assert_eq!(k.label, "Mic", "the off face is untouched");
       k.set_look(FaceTab::Busy, LookField::Glyph, Some("f0bc8".into()));
       assert_eq!(k.look(FaceTab::Busy, LookField::Glyph).as_deref(), Some("f0bc8"));
       k.set_look(FaceTab::On, LookField::Label, Some("  ".into()));
       assert_eq!(k.look(FaceTab::On, LookField::Label), None, "blank clears");
       k.set_look(FaceTab::Off, LookField::Label, None);
       assert_eq!(k.label, "");
   }
   ```

   **Check:** `cargo test -p omadeck-core set_look` → `ok`;
   `cargo clippy --all-targets -- -D warnings` clean. Manual: with the daemon running,
   pressing a toggle on the deck changes the preview; stopping the daemon shows all off;
   On/Busy tabs pin the preview while the key is selected.

9. [ ] **Task I — GUI Toggle group (~280).** Files: `config.rs` (`StateChoice`,
   `state_choices`, `state_choice`, `set_state_choice`), `editor.rs`, and `app.rs`:
   groundwork's `ActionTarget` gains `KeyOn(KeyRef)` (`dial_slot()` → `None`; explicit
   arms in `action_at` / `set_action_at` that read and write `on.action` through
   `edit_ref`). The Toggle group rows, the on-action `ActionSlot` (get =
   `on.action`, set = `edit_ref(.., |k| if let Some(on) = &mut k.on { on.action = a })`),
   the Busy while running switch, the Progress bar switch, ▶ using `action_for`. Test
   (core): `state_choice_follows_the_action_and_survives_an_empty_command`.

   ```rust
   #[test]
   fn state_choice_follows_the_action_and_survives_an_empty_command() {
       let mut k = KeyConfig { on: Some(OnFace::default()), ..KeyConfig::new(0) };
       assert_eq!(k.state_choices(), &[StateChoice::Press, StateChoice::Command]);
       assert_eq!(k.state_choice(), StateChoice::Press);
       k.set_state_choice(StateChoice::Command);
       assert_eq!(k.state_choice(), StateChoice::Command, "an empty command is still the choice");
       assert_eq!(k.state_probe(), None, "but there is nothing to poll yet");
       k.on.as_mut().unwrap().slow = true;
       k.set_state_choice(StateChoice::Press);
       assert_eq!(k.on.as_ref().unwrap().state, None);
       assert!(!k.on.as_ref().unwrap().slow, "slow needs a command");
       k.action = Action::Volume { device: AudioDevice::Input, change: 0, mute: true };
       assert_eq!(k.state_choices(), &[StateChoice::Mute, StateChoice::Command]);
       assert_eq!(k.state_choice(), StateChoice::Mute);
   }
   ```

   **Check:** `cargo test -p omadeck-core state_choice` → `ok`; clippy clean. Manual:
   make each kind of toggle from the GUI, save, and compare the TOML with the examples
   above; a config without `on` saves byte-identical.

10. [ ] **Task J — presets (~250).** Gate: the "Capture on the machine first" output is
    pasted above; if not, build only Mic mute, Output mute and Service now and add the
    others in a follow-up commit. Files: `omadeck-core/src/presets.rs`, `lib.rs`,
    `editor.rs` (the menu button and the Service dialog). Tests:
    `every_preset_makes_a_toggle_that_roundtrips`, `service_preset_is_a_slow_toggle`,
    `service_rejects_odd_unit_names` (`"x; rm -rf ~"` → `Err`).

    ```rust
    #[test]
    fn every_preset_makes_a_toggle_that_roundtrips() {
        for p in PRESETS {
            let mut k = KeyConfig::new(3);
            (p.apply)(&mut k);
            assert!(k.on.is_some(), "{} makes a toggle", p.name);
            let mut c = Config::default();
            *c.keys.get_mut(3) = k.clone();
            let s = toml::to_string_pretty(&c).unwrap();
            assert_eq!(Config::parse(&s).unwrap().key(None, 3), Some(&k), "{} survives a save", p.name);
        }
    }
    ```

    **Check:** `cargo test -p omadeck-core presets` → `ok`; clippy clean. Manual: each
    preset fills a working key; a preset whose binary is missing is greyed out.

11. [ ] **Task K — starter and docs (~150).** Files: `starter.rs`, `README.md`,
    `examples/config.toml`, `CHANGELOG.md`, `config.rs` (`example_parses` also asserts
    `c.keys.iter().any(|k| k.on.is_some())`). **Check:**
    `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` all green.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy 4)

- [ ] Volume dials and the mic dial behave as on groundwork (level, mute, external
      changes within ~1 s).
- [ ] Mic mute key: follows the mute from the key, the dial and the keyboard's mic key;
      a mic dial and the key agree; the face changes within ~300 ms of a press.
- [ ] Night light key: press flips the face at once and it stays flipped (no flicker),
      including the first press after login.
- [ ] Stay awake, DND and Bluetooth presets follow changes made from the Omarchy menu.
- [ ] A state command of `sleep 30`: the deck stays responsive, one warning in the log,
      no `sleep` processes pile up (`pgrep -c sleep`).
- [ ] A typo'd command: off face, one warning, no repeat.
- [ ] Progress bar: a state command counting `10%`…`100%` over 20 s with `bar` on fills
      in steps about twice a second; with `bar` off, text only; no percentage, no bar.
- [ ] Busy while running: a script printing `Step 10%` … `Step 100%` once a second, with
      `[key.busy] bar = true`: steps and bar show, then the normal face. A second press
      while it runs does nothing.
- [ ] A script that exits 1: busy, then the normal face and a toast naming the key.
- [ ] Start a long job, change page and come back: still busy. Unplug and replug
      mid-job: busy again after reconnect, normal when it ends.
- [ ] `systemctl --user restart omadeckd` during a job: the job keeps running (check
      with `pgrep -f`), and finishes on its own.
- [ ] A press toggle with an on action (a `togglefloating` pair) alternates actions, and
      stays on across an unplug and replug.
- [ ] Page away and back: faces show the last state immediately.
- [ ] Idle CPU of `omadeckd` with three command toggles on screen stays ~0%.
- [ ] Service preset with a test unit (`ExecStart=sh -c 'sleep 20; exec python -m
      http.server 8765'`, health URL `http://127.0.0.1:8765/`): press while off → "…" at
      once, then "Loading" or "activating", then on once it answers. Press again →
      "deactivating" if the stop takes time, then off. No flash of the old face.
- [ ] The same with a real model server that takes minutes: fast polling continues for
      the whole load, CPU stays low.
- [ ] A unit started at login: the key shows Loading at startup and turns on by itself.
- [ ] The service stops from a terminal while the key shows on: off within one interval.
- [ ] The configurator's preview matches the deck, progress text included.

## Decisions to explain

- **`[key.on]` rather than Elgato's numbered states.** Elgato caps usable states at two;
  "off face = the key, on face = overrides" keeps existing keys unchanged and the TOML
  minimal, and "on" reads better than "state 1".
- **Real state is the point, a press flip the fallback.** Elgato's automatic flip only
  tracks the last press, which goes wrong as soon as you toggle from the keyboard or
  bar. omadeck flips on press first (instant, Elgato-like), then trusts the command.
- **A shell command's exit status as the source**, not built-in knowledge of each tool:
  it works with any Omarchy version and any script, and mirrors how Omarchy's scripts
  answer. The presets supply good commands.
- **The mute source is derived from the action**, like the strip's `level_source()`.
- **An optional on action** copies Elgato's Hotkey Switch rather than inventing
  `on_press`/`off_press`.
- **Exit 75 means busy.** `EX_TEMPFAIL` ("temporary failure, try again"). Exit 2 would be
  the obvious third value, but `grep` and `jq` return 2 on errors, so a missing file
  would read as busy.
- **A busy state, which Elgato doesn't have.** Some toggles take minutes; on/off alone is
  wrong for that whole time. Elgato plugins retitle the key with progress; omadeck has
  no plugins, so the state command reports it.
- **`slow = true` marks a slow toggle**: a press shows busy at once and polls fast until
  the state moves, instead of flipping and confirming within a second.
- **Fast polling only while it matters**: every 500 ms while busy or for up to 15 s after
  a slow press, the normal interval otherwise. The numbers come from my earlier Stream
  Deck plugin, which tracked a local model server this way.
- **The progress bar is opt-in**, read from the last `NN%` in the text: one output
  convention, and an empty bar stuck at 0% would mislead.
- **Busy while running is opt-in**, for plain commands and scripts only; other actions
  finish at once or start an app that keeps running on purpose.
- **A tracked command runs in its own scope, not through `uwsm-app`**: `uwsm-app` returns
  at once, so there'd be nothing to wait for; the scope keeps a daemon restart from
  killing it, as `detached` does for launched apps.
- **A second press while running is ignored**; a non-zero exit is reported like a failed
  launch.
- **Polling, not events**: a fork every 2 s per toggle on screen; event sources are
  per-tool work and can join as new `Probe` variants later.
- **Only the current page is watched**; values are kept across page changes and replugs
  to avoid a flash.
- **One warning per failing probe**, not per poll.

## Decided (2026-10-17)

- **Decided: `Probe` lives in core `probe.rs`** (D5); `Command { command, interval_ms }`
  hashes both fields on purpose (same command at two intervals = two probes).
- **Decided: toggle state lives in `Runtime`** (`flipped`, `settle`, `jobs`), so it
  survives an unplug and replug. Press toggles are not persisted across a daemon
  restart (see Ask 1).
- **Decided: state commands time out after 5 s** (`STATE_TIMEOUT`), not groundwork's 2 s
  `COMMAND_TIMEOUT`: `omarchy-bluetooth-power is-on` can take ~2 s when D-Bus is slow,
  and a health `curl -m 2` needs headroom. Each run is on its own runner thread, so the
  length never holds up the watcher.
- **Decided: `SETTLE` 1 s, `FAST_INTERVAL` 500 ms, `FAST_WINDOW` 15 s, `MAX_IN_FLIGHT` 8
  runner threads, `RUN_POLL` 20 ms (on the runner, not the watcher).**
- **Decided: state commands run on short-lived runner threads** reporting
  `WatchCmd::Done`; the watcher never waits on a child (agreed with the PR 9/10 plans).
- **Decided: no optimistic flip for mute keys.** Groundwork runs the volume change on the
  watcher and re-reads right after (D5), so the real answer lands in well under 300 ms;
  the volume stale-read bug (daemon review 7) is fixed there, not with `Settle`.
- **Decided: a press on a busy key runs the base action** and asks for a re-read, with no
  guess (see Ask 3).
- **Decided: `ActionFailed` gains `page`**; `KeyState` is `{ page, index, status:
  KeyStatus }` and is replayed to new clients.
- **Decided: busy look** = the off face with the progress text as an accent label; a bar
  only with `bar = true`. **On look** without a background = the key's background 30%
  towards the accent.
- **Decided: a tracked job ignores SIGPIPE and runs in a `systemd-run --user --scope`**
  when available (capture item 8 confirms the exit status passes through), so it survives
  the daemon going away.
- **Decided: `curl` for the Service preset's health check.** It's always on Arch
  (`pacman` depends on it); capture item 7 confirms.
- **Decided: no Wi-Fi or VPN presets** (no Omarchy command; Wi-Fi's "connected" isn't
  "radio on"). VPN is a commented example in the docs.
- **Decided: the Service dialog takes a typed unit name**; listing units is Later (it
  would run `systemctl` on the GTK thread).
- **Decided: `snapshot` draws off faces.**
- **Decided: one branch.** At PR time, if it's over ~1,000 lines, Tasks H–J can go out as
  a second PR; that's a PR-time call.

## Ask before building

1. **Press toggles across a daemon restart.** Planned: kept in `Runtime` only (survive a
   replug, reset to off when the daemon restarts), because a press toggle has no source of
   truth and after a reboot the thing it toggled has usually reset too. If you want them
   kept: `State` gains `#[serde(default, skip_serializing_if = "Vec::is_empty")] pub
   flipped: Vec<FlippedKey>` (`FlippedKey { page: Option<String>, index: u8 }`), loaded
   into `rt.flipped` at start and sent whole through `StateSaver` on every flip (D7).
2. **The field name `busy_while_running`.** It's permanent once in people's TOML.
   Alternatives: `track`, `show_running`.
3. **A press on a slow toggle that's busy.** Planned: run the base action (the script
   knows best what a press during loading means: cancel, or nothing). Alternative: ignore
   the press while busy.

## Later

- Toggle faces on dial panels (a dial press that toggles, showing on/off on the strip).
- `snapshot` running each state command once to show real faces.
- Event-driven probes: inotify on Omarchy's flag files, D-Bus for Bluetooth and
  NetworkManager (Wi-Fi radio, VPN presets).
- Multi-action steps that set a toggle's state (Elgato's `userDesiredState`, PR 12), and
  busy while running for multi-actions (PR 12's sequencer reports when the last step ends).
- Cancelling a running job from the deck (a long press kills its process group, PR 16).
- Elgato's `showOk`/`showAlert` flash when a slow toggle finishes or never starts.
- A busy indicator on a dial's strip panel.
- Listing systemd user units in the Service dialog (off the GTK thread).

## What PR 9 builds on

- `KeyStatus { pressed, state }` passed by reference to `render`; PR 9 adds
  `live: Option<LiveShow>` (owned, a serializable view of the value, like
  `PanelStatus.level`).
- `Daemon::apply_value(probe, value)` and `mark_probe`: PR 9's `Value::Text` and new
  probes land through the same two functions.
- `Poller`'s runner path (`run_due` returns what to start, `WatchCmd::Done` brings it back)
  and `proc::Job`: PR 9's `CommandOutput` reuses `state_command` and `run_state` (with
  its own timeout; `RunResult::Exit.first_line` is the output line, so `RunResult` is
  unchanged).
- `probe::key_status(key, values, pressed)`: PR 9 fills `live` there; `Event::KeyState`
  carries it with no wire change for old fields.
- `probe::watched_probes(cfg, page, n_dials: usize)`: PR 9 adds `k.live` and `dc.live` probes there.
- `Daemon::publish_keys` (in `flush`) and `published_keys`: `KeyState` goes out by
  comparison, so PR 9's `live` changes are published with no extra call.

## Groundwork provides

Everything PR 8 needs is in groundwork.md: `run_input -> bool`, `apply_value`, the
end-of-pass `flush` with `published`, `Runtime: Default`, the `Poller` with
`handle`/`run_due`/`record`/`next_due()`, `WatchCmd::Stop` with `Watcher::stop(self)`,
`Probe::interval`/`refresh_after`, `read_volume` through `output_timeout`, `Value::Level`
holding `render::Level`, `ActionTarget` (not `app::Target`), and the hub's `Replay`.

**Size:** about 2,500 lines with tests across the eleven tasks.
