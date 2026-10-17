# PR 9: `feat/live-keys`: keys and panels that update themselves

Part of [PLAN.md](PLAN.md). Roadmap item: "**Live keys** that show a clock, CPU usage or
the active workspace". Base branch: `feat/toggle-keys` (PR 8), which sits on
`feat/groundwork`. This PR reuses groundwork's **state watcher** (core `probe.rs`, daemon
`watch.rs`) and PR 8's key status, and it owns the **Hyprland event listener**
(`omadeckd/src/hypr.rs`), which PR 10 (focused app volume) and PR 15 (pages follow the
focused app) reuse.

**Rewritten 2026-10-17** against the cross-plan decisions (D1–D15) and the code as merged
on `fix/bugs` (`d7bd448`), with the names groundwork and PR 8 deliver. Before Task A,
re-read [groundwork.md](groundwork.md) and [pr8-toggle-keys.md](pr8-toggle-keys.md) as
merged and check the names in "What this PR builds on" against the tree; fix this plan
first if any differ. Do not reconcile in code.

**Outcome:** any key, and any dial's strip panel, can show a live value: the time or date
(strftime format), CPU %, memory %, battery % (with charging), the active Hyprland
workspace, or the first line of a command's output, re-run at an interval. On a key the
value is drawn the way Elgato's plugins set a key's title. On the strip it uses Elgato's
`$A1` (text) or `$B1` (text and bar) layout. A live key still runs its action on press.
Nothing polls faster than it needs to: the clock wakes on the minute (or second)
boundary, the workspace is event-driven, and only the keys and panels whose value changed
are repainted. The configurator shows the same values, mirrored from daemon events (D2);
it never reads them itself. Old configs load and save byte-identical.

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

## What this PR builds on (groundwork and PR 8, as planned)

Check each against the merged tree before Task A (`grep -n` the name):

- Core `probe.rs` (D5): `Probe` (`Hash + Eq + Clone`), `Value`, `Values { probes:
  HashMap<Probe, Value>, brightness: u8 }`, and `dial_status(dial: Option<&DialConfig>,
  values: &Values, pressed: bool) -> DialStatus`, the one builder the daemon, the GUI and
  `snapshot` call.
- PR 8 adds `Probe::Command { command: String, interval_ms: u32 }`,
  `Value::Toggle(Toggle)`, `KeyStatus { pressed, state }` (in `render.rs`, passed to
  `Renderer::render(key, style, size, &status)`), the core builder
  `probe::key_status(key, values, pressed) -> KeyStatus`, the daemon's
  `key_state(cfg, rt, page, i, pressed)`, and `Event::KeyState { page: Option<String>,
  index: u8, status: KeyStatus }`.
- Daemon `watch.rs` (groundwork, extended by PR 8): `WatchCmd::{Watch(Vec<Probe>),
  Refresh(Probe), Volume {..}, Leave {..}, Done { probe, result: RunResult }, Stop}`,
  `Watcher { cmd, values, thread }`, `Watcher::stop(self)`, `start()`. The state lives
  in `Poller` (`handle`, `run_due`, `record`, `next_due()`, `in_flight`, `failing`,
  `warn_once`); commands run on PR 8's runner threads (`run_state(command, timeout,
  cancel) -> RunResult`, `state_command`), which send `WatchCmd::Done` back. The thread
  blocks on `recv()` with nothing due, else `recv_timeout(next_due - now)`. It sends only
  changes.
- Core `proc::output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output>`.
- Daemon `daemon.rs`: `struct Daemon { cfg, renderer, nav, wanted_page, hub, watcher,
  saver, rt: Runtime, deck, published, published_keys, reload_at, repaint_at }`,
  `Runtime { values, state }` plus PR 8's `flipped`, `settle`, `jobs`;
  `Daemon::apply_value(probe, value)` (the one place a value lands; it marks what shows
  the probe through `mark_probe`), and core `probe::watched_probes(cfg, page, n_dials:
  usize)`, which builds the `WatchCmd::Watch` set. `Daemon::flush` publishes each
  panel's `DialStatus` and (PR 8's `publish_keys`) each key's `KeyState` that differs
  from what was last sent.
- `render::DialStatus { pressed, panel: PanelStatus }`, `render::PanelStatus { level }`,
  `render::Level` (`Serialize, Deserialize`, D2), `Event::DialStatus { index: u8,
  status: PanelStatus }`, replayed to new clients (D8). The GUI keeps the latest
  `PanelStatus` per panel and PR 8's key states, and refreshes from them.
- Renderer test constructor `Renderer::with_font_bytes(..)` and the bundled OFL fixture
  font (D9), through fix/review's test helper `renderer_with_font() -> Renderer` in
  `render.rs`'s `mod tests`.

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
        interval: u32,
    },
}

fn default_clock_format() -> String { "%H:%M".into() }
fn is_default_clock_format(f: &String) -> bool { f == "%H:%M" }
fn default_live_interval() -> u32 { 5 }
fn is_default_live_interval(n: &u32) -> bool { *n == 5 }

impl Live {
    /// For the GUI's "Shows" dropdown; index 0 is "no live value".
    pub const KINDS: [&'static str; 7] =
        ["Nothing", "Clock", "CPU", "Memory", "Battery", "Active workspace", "Command output"];
    pub fn kind_index(live: Option<&Live>) -> usize;   // None → 0, Clock → 1 … Command → 6
    pub fn of_kind(i: usize) -> Option<Live>;           // 0 → None; 1 → Clock { "%H:%M" }; 6 → Command { "", 5 }
    /// Whether the strip panel draws `$B1` (with a bar) rather than `$A1`.
    pub fn has_bar(&self) -> bool;                       // Cpu | Memory | Battery
    /// The probe that feeds it (defined here, the type lives in `probe.rs`).
    pub fn probe(&self) -> crate::probe::Probe;
}
```

- `KeyConfig.live` and `DialConfig.live`: `Option<Live>`, last field,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. `KeyConfig::new` and
  `DialConfig::new` set `None`. Both `is_blank()`s also require `live.is_none()`.
- `DialConfig::level_source()` returns `None` when `live` is set: an explicit live value
  wins over the volume or brightness value the dial's actions imply. Add the check as the
  first line.
- `interval` is clamped to at least 1 where it becomes a probe (`Live::probe`), not on
  load, so a hand-written `0` still saves as written.

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

No new `Action` variant and no `KINDS` change (D14): a live key's press runs its ordinary
`action`.

## Probes and values (`omadeck-core/src/probe.rs`)

New variants (append, keep groundwork's and PR 8's):

```rust
pub enum Probe {
    // … Volume(AudioDevice) (groundwork), Command { .. } (PR 8)
    Clock { format: String },
    Cpu,
    Memory,
    Battery,
    /// Fed by the Hyprland listener, never polled.
    Workspace,
    CommandOutput { cmd: String, interval: Duration },
}

pub enum Value {
    // … Level(..) (groundwork), Toggle(..) (PR 8)
    Text(String),
    Percent(u8),
    Battery { percent: u8, charging: bool },
}

impl Probe {
    /// False for probes something else feeds (the Hyprland listener). `watched_probes`
    /// drops them before sending `WatchCmd::Watch`.
    pub fn is_polled(&self) -> bool { !matches!(self, Probe::Workspace) }
}

/// What the live key or panel for `live` shows now; `None` until a value arrives.
pub fn live_show(live: &Live, values: &Values) -> Option<LiveShow>;
```

`live_show` maps `Text(s)` → `LiveShow { text: s, bar: None, charging: false }`,
`Percent(p)` → `{ text: "{p}%", bar: Some(p as f32 / 100.0), charging: false }`,
`Battery { percent, charging }` → the same with `charging`. Any other `Value` → `None`.

`LiveShow` lives in `render.rs`, next to `Level`, because the renderer draws it and the
IPC events carry it (D2):

```rust
/// What a live key or strip panel shows (PR 9).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

- `KeyStatus` (PR 8) gains `#[serde(default, skip_serializing_if = "Option::is_none")]
  pub live: Option<LiveShow>` (owned). `probe::key_status` fills it with
  `key.and_then(|k| k.live.as_ref()).and_then(|l| live_show(l, values))`.
- `render::PanelStatus` gains `#[serde(default, skip_serializing_if = "Option::is_none")]
  pub live: Option<LiveShow>` (groundwork's decision: panel fields go on `PanelStatus`,
  not `DialStatus`). `dial_status` fills it the same way from `dial.live`; `level` stays
  `None` for a live dial (because `level_source()` is `None`).
- No event changes: `Event::DialStatus { index, status: PanelStatus }` and
  `Event::KeyState { page, index, status: KeyStatus }` carry `live` inside `status`. The
  daemon's `flush` publishes them whenever the shown value changes (it compares, as for
  levels and toggle state), and the hub replays them the same way. Old configurators
  ignore the field.

## Clock and system readers (core, pure where possible)

New `omadeck-core/src/live.rs`:

```rust
/// Never sleep longer than this before re-checking the clock. Monotonic sleeps stop
/// during suspend, so an uncapped minute sleep would leave a stale time after resume.
pub const CLOCK_CAP: Duration = Duration::from_secs(5);

/// `now` formatted with `format`; "?" for a bad format.
pub fn format_clock(format: &str, now: &jiff::Zoned) -> String {
    jiff::fmt::strtime::format(format, now).unwrap_or_else(|_| "?".into())
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

`needs_seconds` walks the string: on `%`, look at the next char; `%%` skips both; any of
`S T s r X c +` returns true.

New `omadeck-core/src/sysinfo.rs` (no `sysinfo` crate):

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CpuTimes { pub busy: u64, pub total: u64 }

/// The `cpu ` line of /proc/stat: user nice system idle iowait irq softirq steal [guest
/// guest_nice]. total = the first eight summed (guest is already inside user);
/// busy = total − idle − iowait.
pub fn parse_proc_stat(stat: &str) -> Option<CpuTimes>;
/// Δbusy·100 / Δtotal, rounded; 0 when Δtotal is 0.
pub fn cpu_percent(prev: CpuTimes, now: CpuTimes) -> u8;

/// Keeps the previous sample. The first sample only primes it (returns None).
#[derive(Debug, Default)]
pub struct CpuMeter { prev: Option<CpuTimes> }
impl CpuMeter { pub fn sample(&mut self, stat: &str) -> Option<u8> }

/// (MemTotal − MemAvailable) · 100 / MemTotal, rounded; None if either is missing.
pub fn parse_meminfo(meminfo: &str) -> Option<u8>;

/// Battery % and charging from a power_supply directory. Entries with `type` = Battery
/// and no `scope` = Device (that's a mouse or headset). Several batteries combine as
/// Σenergy_now / Σenergy_full (else Σcharge_now / Σcharge_full), else the mean
/// `capacity`. charging = any `status` is "Charging". No battery → None.
pub fn battery_in(root: &Path) -> Option<(u8, bool)>;
pub fn battery() -> Option<(u8, bool)> { battery_in(Path::new("/sys/class/power_supply")) }
```

Read each sysfs file with `fs::read_to_string(..).ok()` and `trim()`.

**jiff.** `jiff 0.2.37` is already in `Cargo.lock` (through `env_logger`). Core adds
`jiff = { version = "0.2", default-features = false, features = ["std", "tz-system",
"tzdb-zoneinfo"] }`. That's a go/no-go in Task A: after adding it,
`git diff --stat Cargo.lock` must show only `Cargo.lock | 1 +` (the new `"jiff",` line
under `omadeck-core`). If any `[[package]]` is added, take the fallback instead:
`libc::localtime_r` + `libc::strftime` (`libc` is also in `Cargo.lock`), same
`format_clock` signature with `i64` seconds instead of `&Zoned`.

## Watcher additions (`omadeckd/src/watch.rs`)

| Probe | Read | Next read |
|---|---|---|
| `Clock { format }` | `Value::Text(live::format_clock(format, &jiff::Zoned::now()))`, on the thread | `live::next_tick(jiff::Timestamp::now().as_millisecond(), live::needs_seconds(format))` |
| `Cpu` | `fs::read_to_string("/proc/stat")` → `cpu.sample(..)` → `Value::Percent` | 1 s |
| `Memory` | `/proc/meminfo` → `parse_meminfo` → `Value::Percent` | 2 s |
| `Battery` | `sysinfo::battery()` → `Value::Battery` | 5 s |
| `CommandOutput { cmd, interval }` | a runner thread (below) | `interval` after the result arrives |
| `Workspace` | never (not in the watched set) | – |

- `cpu: CpuMeter` is a new `Poller` field. A `Watch` that drops `Cpu` resets it to
  `CpuMeter::default()`, so the next first read primes again. Until then the key shows
  `–` (no value).
- Clock, Cpu, Memory and Battery are read inline in `Poller::run_due`, next to
  groundwork's `Probe::Volume` arm. A reader returning `None` sends nothing (as
  groundwork does for `Volume`).
- `Probe::interval()` gains the arms of the table: Clock `CLOCK_CAP` (only a fallback),
  Cpu 1 s, Memory 2 s, Battery 5 s, `CommandOutput` its `interval` (at least 1 s),
  Workspace 1 s (never used). `Probe::refresh_after()`: `CommandOutput` as PR 8's
  `Command` (300 ms and 1200 ms), the others `&[]`. The clock's boundary comes from
  `next_read` (Task D), which `Poller::record` calls where it wrote `now + p.interval()`.
- **Runner threads: PR 8's.** A command must never hold up the clock or the other
  probes, so `CommandOutput` goes through the path PR 8 built for state commands. No new
  `WatchCmd` variant and no second runner:
  - `Poller::run_due`: PR 8's `Probe::Command { .. }` arm becomes
    `Probe::Command { .. } | Probe::CommandOutput { .. }` (same `in_flight` check, same
    `MAX_IN_FLIGHT`). A due probe already in flight is skipped (never two at once).
  - `run` (the thread loop): the `let Probe::Command { command, .. } = &probe else
    { continue };` line becomes a match that takes `(command, timeout)` from either:
    `Command` → `STATE_TIMEOUT`, `CommandOutput { cmd, interval }` →
    `(*interval).clamp(Duration::from_secs(1), LIVE_COMMAND_TIMEOUT)`, then PR 8's
    `run_state(&command, timeout, &cancel)`, which sends `WatchCmd::Done { probe, result }`.
  - `Poller::handle`'s `Done` arm, right after PR 8's `in_flight.remove` check: a
    `Probe::CommandOutput` is answered here and the arm returns, before PR 8's toggle
    mapping:
    ```rust
    if matches!(probe, Probe::CommandOutput { .. }) {
        match result {
            // Any exit status counts; read_lines already skipped empty lines.
            RunResult::Exit { first_line, .. } => {
                self.failing.remove(&probe);
                let text = first_line.unwrap_or_else(|| "–".into());
                self.record(probe, Value::Text(text), now, out);
            }
            RunResult::TimedOut | RunResult::SpawnFailed(_) => {
                // Keep the last value; schedule the next run.
                self.warn_once(&probe, "live command failed or timed out");
                self.next_due.insert(probe.clone(), now + probe.interval());
            }
        }
        return;
    }
    ```
  - `/// Upper bound for one live command run.`
    `const LIVE_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);` in `watch.rs`.
- A `Done` for a probe no longer watched is dropped (PR 8's `in_flight` check).
- Any exit status counts: the first non-empty stdout line is the value (`read_lines`
  keeps 64 chars), `–` when empty.
- The thread already holds a clone of its own `cmd` sender for the runners (PR 8) and
  ends on `WatchCmd::Stop`; nothing changes there.
- **Refresh after a press:** when a key whose `live` is `Command` runs its action, the
  daemon sends `WatchCmd::Refresh(probe)` (as PR 8 does for toggles), so a key showing
  e.g. `playerctl status` updates right after the press.

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

/// Starts the `hypr` thread. `None` outside Hyprland (no thread).
pub fn start() -> Option<mpsc::Receiver<HyprEvent>>;

/// What one event line asks for. Pure.
#[derive(Debug, PartialEq, Eq)]
enum Parsed {
    Workspace { id: i64, name: String },
    QueryWorkspace,                       // focusedmon, renameworkspace
    QueryWindow,                          // activewindowv2 with an address
    NoWindow,                             // activewindowv2 with no address
    Title { address: String, title: String },
    Reseed,                               // configreloaded
}

/// What the thread last sent, so it sends only changes.
#[derive(Debug, Default)]
struct Seen { workspace: Option<(i64, String)>, window: Option<ActiveWindow> }

fn parse(line: &str) -> Option<Parsed>;
/// Turns a parsed line into the events to send, running `query` (the request socket)
/// when the line needs it. Pure but for `query`, so tests pass a closure.
fn handle(p: Parsed, seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent>;
/// The events for a fresh connection: both queries, through `handle`'s dedup.
fn seed(seen: &mut Seen, query: &mut dyn FnMut(&str) -> Option<String>) -> Vec<HyprEvent>;
fn parse_workspace_json(json: &str) -> Option<(i64, String)>;     // {"id","name"}
fn parse_window_json(json: &str) -> Option<ActiveWindow>;          // {} or no "address" → None
fn normalise_address(a: &str) -> String;                           // trim, strip "0x", lowercase
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
`query("j/activewindow")` → `parse_window_json` → send `ActiveWindow(w)` if it differs from
`seen.window`. `NoWindow` → `ActiveWindow(None)` if `seen.window` was `Some`. `Title` →
only if it matches `seen.window`'s address: a copy with the new title. `Reseed` → `seed`.
The `activewindow>>CLASS,TITLE` line is not needed: the query gives class, title and pid
at once.

The thread (`name("hypr")`):

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
                if tx.send(ev).is_err() { return; }
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
                    if tx.send(ev).is_err() { return; }
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
- `serde_json` is added to `omadeckd`'s `[dependencies]` (`serde_json.workspace = true`).
- **In the daemon:** `Daemon` gains `hypr: Option<mpsc::Receiver<HyprEvent>>` (set by
  `hypr::start()` in `Daemon::new`), and `Runtime` gains `window: Option<hypr::ActiveWindow>`
  (a new field; `Runtime` still derives `Default`). A new `Daemon::apply_hypr(&mut self)`
  drains it with `try_recv`; `main.rs`'s `run()` calls it right after
  `daemon.apply_values()`, so it runs **before** the no-deck branch, with the other
  worker drains (D4). `Workspace { name, .. }` → `apply_value(Probe::Workspace,
  Value::Text(name))` (it lands in `rt.values` even with no deck). `ActiveWindow(w)` →
  `rt.window = w` (PR 10 and 15 hook in here). Values of `Probe::Workspace` are never
  pruned on reload or page change, since nothing re-reads them.

## Render (`omadeck-core/src/render.rs`)

**Keys** (`render` with `KeyStatus.live`, only when `key.live.is_some()`). The value is
drawn like Elgato's title. `text = status.live.map(|l| l.text).unwrap_or("–")`.
- Battery while charging, with no icon or glyph of its own: the face's glyph becomes
  `CHARGING_GLYPH`, so charging is visible. Otherwise no default glyph on keys.
- **No icon or glyph:** the value is the face, large and centred. If the key has a label
  and `show_label`, the label is a small caption at the top (`s × 0.14` px, centre
  `s × 0.16`, `draw_line`) and the value block fills the area below (`top = s × 0.28`,
  `height = s × 0.72`).
- **Icon or glyph:** the icon as today, and the value **in the label's place**:
  `render_face` with a copy of the face whose `label` is the value and `show_label` is
  true. The configured label isn't drawn.
- Pressed and toggle state are unchanged (PR 8's face choice comes first; the value
  replaces only the label).
- `draw_block` becomes `draw_block_in(img, top: f32, height: f32, text, color)`, with
  `draw_block(img, text, color)` = `draw_block_in(img, 0.0, s, text, color)`. It splits
  on `\n` first and wraps each line, so `%n` puts the date under the time. For text
  without `\n` and the full area, every pixel must stay the same (the golden tests
  check).

**Strip panels** (`render_strip_panel`): before the `level_source()` arm, a dial with
`live` draws `$B1` when `live.has_bar()`, else `$A1`. `$A1` is `draw_b1` without the bar:
add `bar: bool` as `draw_b1`'s last parameter (the existing callers pass `true`). The
level passed is `st.live.map(|l| Level { value: l.bar.unwrap_or(0.0), text: l.text,
muted: false })`, `None` before the first value (`–`). The icon: the user's icon, else
their glyph, else the source's glyph (battery charging: `CHARGING_GLYPH`), in
`glyph_color`, `STRIP_ICON_PX`, like `brightness_icon`; write `fn live_icon(&mut self,
face: &Face, live: &Live, charging: bool, color: Rgb) -> Option<RgbaImage>`.

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

- `Daemon::new` sets `hypr: hypr::start()`.
- `probe::watched_probes` (core): add `k.live.as_ref().map(Live::probe)` for the current page's keys
  and the same for each dial, keeping only `is_polled()` probes, deduplicated.
- `apply_value` → `mark_probe`: the keys and dials it marks for `probe` now include
  those whose `live.probe() == probe` (for dials, `probe::shows` gains the `live` case;
  for keys, `mark_probe`'s key loop also matches `k.live.as_ref().map(Live::probe)`).
  `flush` then publishes `KeyState` / `DialStatus` with `live` by comparison; nothing is
  published from `apply_value`. `publish_keys` also takes keys with `live` on the
  current page.
- Key press: after the action runs, a key with `Live::Command` sends
  `WatchCmd::Refresh(probe)`.
- Reload: values of probes still watched are kept (no blanking), the rest dropped,
  `Probe::Workspace` always kept. Disconnect: unchanged (`Watch(vec![])`); `hypr` keeps
  running. Page change: unchanged.
- `snapshot`: passes `Values::default()` as today, so live keys and panels show `–`.
- No new IPC event types; `live` rides on `KeyState` and `DialStatus`.

## GUI (`omadeck-gui`)

- `editor.rs`: a **Live value** group in `build_key` and **Panel value** in `build_dial`,
  after the action groups: an `adw::ComboRow` "Shows" over `Live::KINDS`
  (`set_selected(Live::kind_index(..))` before connecting; on change set
  `live = Live::of_kind(sel)` and `rebuild_editor()`). Per source:
  - Clock: `EntryRow` "Format", subtitle "strftime, e.g. %H:%M or %a %d %b"; under it an
    `ActionRow` showing `live::format_clock(text, &jiff::Zoned::now())`, updated in the
    entry's `connect_changed` (one format per keystroke, not a timer).
  - Command: `EntryRow` "Command" and `SpinRow` "Every (seconds)" 1–3600.
  - A hint row: "The label shows as a small caption; with an icon, the value takes the
    label's place."
- Preview: `refresh_key` and `refresh_strip` already build their status from the mirrored
  `KeyState` / `DialStatus` (PR 8, groundwork); they now carry `live` too. No daemon: no
  value, so `–` (D2: the GUI never reads system state or runs the command).
  *Try it now* still runs the key's action.
- **Starter layout: unchanged.** It already fills all 15 keys, and keys 5–9 switch
  workspaces 1–5.

## Docs

README: a "Live keys" feature bullet (sources, the strftime link
<https://docs.rs/jiff/latest/jiff/fmt/strtime/>, the `command` example); roadmap
`- [x] **Live keys** …`. `examples/config.toml`: the clock, CPU and command keys above and
a battery dial, commented. CHANGELOG under Unreleased/Added: "Live keys: a key or a dial's
strip panel can show the time, CPU, memory, battery, the active Hyprland workspace or a
command's output, kept up to date by the daemon."

## Capture on the machine first

Paste the output into this plan (or the named fixture file) before the task that needs it.

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
   (on battery and on the charger). Copy the layout into the `battery_in` test if it
   differs from the test's.
5. **/proc samples:** `head -1 /proc/stat; grep -E 'MemTotal|MemAvailable' /proc/meminfo`.
6. **Glyphs:** `for g in F0150 F061A F035B F0079 F0084 F056E F018D; do printf "%s \U000$g\n" $g; done`
   in the Omarchy terminal (bash), and `fc-list ":charset=f056e" family | head -3`.
   Each should show a clock, chip, memory stick, battery, charging battery, a
   workspace-like symbol and a console prompt.
7. **jiff `%n`:** in Task A's test run; if `format_clock("%H%n%M", ..)` doesn't give
   `"12\n00"`, use `\n` handling in `format_clock` (replace `%n` before formatting).

## Tasks

Each task is one session of ~150–300 lines including tests, in this order. Each ends
with its check and a commit. Run `cargo fmt` before every commit.

### Task A — `live.rs` and `sysinfo.rs` (pure, ~250 lines)

Files: `crates/omadeck-core/Cargo.toml` (jiff, the go/no-go above),
`crates/omadeck-core/src/lib.rs` (`pub mod live; pub mod sysinfo;`), new
`crates/omadeck-core/src/live.rs`, new `crates/omadeck-core/src/sysinfo.rs`. Signatures as
in "Clock and system readers". Tests: `clock_formats_and_ticks` (below),
`needs_seconds_formats` (`%H:%M` no; `%T`, `%S`, `%r` yes; `%%S` no),
`next_tick_second_boundary`, `next_tick_capped`, `bad_format_is_question_mark`,
`cpu_percent_from_two_samples`, `cpu_no_delta_is_zero`, `cpu_meter_primes_first`,
`meminfo_used_percent`, `meminfo_missing_field_is_none`, `battery_charging_status`
(Charging / Full / Discharging / Not charging), `battery_skips_device_scope`,
`battery_combines_two` (temp dirs).

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
`cargo test -p omadeck-core -- live:: sysinfo::` → `test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; … filtered out`.

### Task B — config and probes (~200 lines)

Files: `config.rs` (`Live`, the fields, `new`, `is_blank`, `level_source`'s first line,
`KINDS`/`kind_index`/`of_kind`/`has_bar`/`probe`), `probe.rs` (the new `Probe` and
`Value` variants, `is_polled`, `live_show`), `render.rs` (`LiveShow` only, plus
`PanelStatus.live` and `KeyStatus.live`; fix every struct literal `cargo build` flags with
`live: None`), `ipc.rs` (tests only: `live` inside `KeyState`'s and `DialStatus`'s
`status`), `lib.rs`
(`pub use config::Live; pub use render::LiveShow;`). The new `Value` variants make
`match`es on `Value` non-exhaustive: `cargo build --workspace` lists each; add an arm
that does what the `_` case would.

Tests: `live_key_round_trips` (below), `clock_default_format_not_written`,
`command_interval_default_not_written`, `old_config_saves_byte_identical` (load
`examples/config.toml`, save, compare), `key_with_only_live_is_not_blank`,
`dial_live_wins_over_level_source`, `live_show_forms` (Text, Percent, Battery),
`workspace_is_not_polled`, `key_state_live_omitted_when_none` (wire shape).

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

**Check:** `cargo test -p omadeck-core -- config:: probe:: ipc::` → `test result: ok.`, 0 failed;
`cargo build --workspace` clean.

### Task C — render (~250 lines)

Files: `render.rs`. `draw_block_in` with `\n`; the key layouts; `draw_b1(.., bar: bool)`;
`live_icon` and the glyph constants; the live arm in `render_strip_panel`. Tests:
`live_key_value_centred_without_icon`, `live_key_label_is_caption` (pixels in the top
`s × 0.28` band differ from the background, and so do pixels below it),
`live_key_with_glyph_puts_value_in_label_slot`, `draw_block_honours_newline` (two text
bands), `dial_live_text_is_a1_without_bar` (below), `dial_live_level_is_b1_with_bar`,
`live_without_value_shows_dash` (the fixture font has `–`, U+2013: assert the value
rect isn't all background and that `KeyStatus { live: None, .. }` renders the same as a
`LiveShow` with text `–`), and the existing goldens unchanged. The fixture font is
printable ASCII plus `–` and `…` (fix/review); the texts in these tests use `A`, as the
goldens do.

```rust
#[test]
fn dial_live_text_is_a1_without_bar() {
    let mut r = renderer_with_font();
    let mut cfg = Config::default();
    cfg.dial_mut(0).live = Some(Live::Clock { format: "%H:%M".into() });
    let st = DialStatus {
        panel: PanelStatus {
            // Test texts use "A", as fix/review's goldens do (D9).
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

**Check:** `cargo test -p omadeck-core render::` → all pass (the goldens included),
0 failed.

### Task D — watcher probes (~250 lines)

Files: `crates/omadeckd/src/watch.rs`, `crates/omadeck-core/src/probe.rs`
(`Probe::interval` arms). The new read arms in `run_due`, the `Poller.cpu` field, the
`CommandOutput` cases in `run_due`, `run` and the `Done` arm (PR 8's runner path),
`LIVE_COMMAND_TIMEOUT`. Pull the scheduling into a pure function so it can be tested
(`record` calls `next_read(&p, now, jiff::Timestamp::now().as_millisecond())` in place
of `now + p.interval()` in its non-fast case):

```rust
/// When `probe` should be read next, after a read at `now` (`wall_ms`: Unix ms).
fn next_read(probe: &Probe, now: Instant, wall_ms: i64) -> Instant;
```

Tests (in `watch.rs`): `clock_scheduled_on_boundary` (below), `cpu_every_second`,
`command_interval_from_config`, `command_output_done_records_text` (PR 8's `Poller`
test pattern: `Watch` a `CommandOutput`, `run_due` hands it out, a synthetic
`Done { probe, result: RunResult::Exit { code: Some(1), first_line: Some("x".into()) } }`
sends `Value::Text("x")`).

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

**Check:** `cargo test -p omadeckd watch::` → all pass, 0 failed.

### Task E — `hypr.rs` (~300 lines)

Files: new `crates/omadeckd/src/hypr.rs`, `crates/omadeckd/Cargo.toml` (`serde_json`),
`main.rs` (`mod hypr;` only), fixture `crates/omadeckd/tests/fixtures/socket2.txt` from
Capture 1. Code as in the listener section. Tests: `parse_workspacev2`,
`parse_workspace_name_with_comma` (`5,my,ws` → name `my,ws`), `parse_unknown_ignored`
(`workspace>>2`, `activewindow>>a,b`, `activespecial>>,DP-1` → `None`),
`empty_activewindowv2_is_no_window`, `title_only_for_focused` (below),
`seed_parses_both_queries`, `window_json_empty_object_is_none`,
`find_instance_prefers_env`, `find_instance_falls_back_to_newest` (temp dir with two
instance dirs and files named `.socket2.sock`), `transcript_gives_events` (feeds each
line of the fixture through `parse`/`handle` with a canned `query`; expected events
written after Capture 1).

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

**Check:** `cargo test -p omadeckd hypr::` → all pass, 0 failed;
`cargo clippy -p omadeckd --all-targets -- -D warnings` clean (allow `dead_code` on
`start` until Task F only if clippy needs it; remove it in Task F).

### Task F — daemon wiring (~200 lines)

Files: `crates/omadeckd/src/daemon.rs`, `main.rs` (the `apply_hypr` call),
`crates/omadeck-core/src/probe.rs`. `Daemon.hypr`, `Runtime.window`, `apply_hypr`,
`probe::watched_probes`, `probe::shows` and `mark_probe`'s key match, `publish_keys`'
live keys, the `Refresh` after a press, reload retention for `Workspace`. Tests (in
`probe.rs`): `watched_probes_skips_workspace`,
`watched_probes_includes_dial_live`, `watched_probes_only_current_page` (below).

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
    let home = watched_probes(&cfg, None, 4);
    assert!(home.contains(&Probe::Cpu));
    assert!(!home.contains(&Probe::Memory));
    assert!(!home.contains(&Probe::Workspace), "fed by the listener, never polled");
    assert!(watched_probes(&cfg, Some("Other"), 4).contains(&Probe::Memory));
}
```

**Check:** `cargo test -p omadeckd` → all pass; `cargo clippy --all-targets -- -D warnings`
clean. Then on the hardware: a clock key, a CPU key, a command key, a workspace key.

### Task G — GUI (~200 lines)

Files: `crates/omadeck-gui/src/editor.rs` (the two groups), `app.rs` only if the mirrored
state doesn't already carry `live` through. Traps: never
`if let Some(k) = app.key(i) { app.edit(i, ..) }` while a `config` borrow is alive (BUGS.md
4: clone, drop the borrow, then edit); no `rebuild_editor()` from an `EntryRow`'s
`connect_changed` (focus is lost); closures shared between rows are `Rc<dyn Fn…>`, not
`impl Fn + Clone`. No test (GTK); **check:** `cargo clippy --all-targets -- -D warnings`
clean; by hand: pick each source on a key and a dial, the preview shows `–` with the
daemon stopped and the live value with it running; the clock example row follows typing.

### Task H — docs (~80 lines)

README, `examples/config.toml`, CHANGELOG. **Check:**
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → all green
(`example_parses` included).

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] A `%H:%M` clock key turns over within ~0.1 s of the minute (compare with Waybar);
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
- [ ] Pressing a live key runs its action; the pressed ring shows over the value.
- [ ] Battery and clock dials draw `$B1` and `$A1` correctly on the strip.
- [ ] The configurator's preview shows the same values as the deck, within a second.
- [ ] No visible CPU cost from the daemon with a seconds clock (`top`).

## Decided (2026-10-17)

- **Decided: `live` is a key and dial field, not an `Action`.** What a key shows and what
  it does are separate, so a clock can launch a calendar.
- **Decided: values are `Text`, `Percent` and `Battery`,** not `Level` (which carries a
  `muted` that means nothing here). The renderer gets a `LiveShow { text, bar, charging }`.
- **Decided: `$A1` is `draw_b1` without the bar,** same rects, a `bar: bool` parameter.
- **Decided: CPU 1 s** (Elgato's plugin rate), **memory 2 s, battery 5 s.**
- **Decided: the clock wakes on the boundary, capped at 5 s,** because monotonic sleeps
  stop during suspend.
- **Decided: jiff** (already in the build), with the go/no-go and libc fallback in
  Task A. No `sysinfo` crate.
- **Decided: commands run on runner threads** with a timeout of the interval, clamped to
  1–10 s; one run at a time per probe; the first non-empty stdout line, any exit status,
  `–` when empty; a failed run keeps the last value.
- **Decided: the Hyprland listener is part of this PR** (Task E), not its own PR: the
  branches chain anyway, and PR 10 and 15 build on this one.
- **Decided: sockets, not `hyprctl`.** Events from `.socket2.sock`; the window and
  workspace from `.socket.sock` with the same `j/…` requests `hyprctl -j` sends, so no
  process per focus change. `activewindowv2` triggers one `j/activewindow` request, which
  gives class, title and pid together, so `activewindow>>` isn't paired up.
- **Decided: the workspace also follows `focusedmon` and `renameworkspace`,** since
  `workspacev2` isn't sent when focus moves to another monitor by mouse. Special
  workspaces are ignored.
- **Decided: the listener always runs inside Hyprland,** even with no workspace key,
  because PR 10 and 15 need the focused window; it's one idle blocking read.
- **Decided: the GUI mirrors live values from `KeyState`/`DialStatus` events only** (D2);
  without the daemon it shows `–`. `snapshot` shows `–`.
- **Decided: the glyph list above,** checked on the machine before Task C.
- **Decided: starter layout unchanged.**

## Ask before building

1. When a live key has a label: draw it as a small caption above the value (planned), or
   hide it, as Elgato hides the plugin's title behind a user title? Only Task C's key
   layout changes.

## Decisions to explain

- **`live` is a key field, not an `Action`.** Elgato ties the display to the plugin's
  action. Here a key's look and its action are separate, and dials get the same field.
- **The value wins over the label.** Elgato draws a user's title *instead of* the
  plugin's; that would hide the value, so omadeck keeps the label as a caption.
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

**Size:** about 1,650 lines including tests over 8 tasks (A 250, B 200, C 250, D 250,
E 300, F 200, G 200, H 80).

## Status (2026-10-17)

Reviewed against `feat/toggle-keys` as merged (tip `55d0d62`): the names in "What this
PR builds on" match the tree (`Probe::Command`, `Value::Toggle`, `KeyStatus`,
`probe::key_status`, `Event::KeyState`, the watcher's `Refresh`/`Leave`/`Done` and
runner threads, `apply_value`/`mark_probe`, `publish_keys`, `watched_probes(cfg, page,
n_dials)`). Not started.

- **Open decision (awaiting the user):** a live key that also has a label — caption
  above the value (the plan's default) or hidden as in Elgato. Only Task C's key layout
  changes.
- **Captures pending** ("Capture on the machine first"): items 2–5 and 7 the assistant
  can run from the shell; item 1 (the 30 s socket2 transcript) needs the user to perform
  the listed actions while it records; item 6 (the seven Nerd Font glyphs) needs the user
  to confirm they render.
