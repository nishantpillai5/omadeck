# PR 8: `feat/toggle-keys`: keys with on/off faces that follow real state

Part of [PLAN.md](PLAN.md). Base: `feat/strip-backgrounds` (PR 7), which itself sits on
`feat/brightness`. Nothing from PR 7 is used; the base is only for a clean merge order.
This PR introduces the **state watcher** building block that PRs 9, 10 and 14 extend.

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Rewrite against the code as merged (upstream's named pages: `Option<&str>` / `Config::key(page, index)` / `Nav`), not `ba0eced`'s `page: u8`.
- [ ] Replace every "assumption about PR N" with that PR's real, merged signatures, copied in full.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.
- [ ] `Renderer::render` takes `Option<&KeyConfig>` today. List every caller that gains the toggle parameter: the daemon's `write_key`, the snapshot, the GUI's `refresh_key`, and the tests.
- [ ] `Event::ActionFailed { index, message }` has no page, but `busy_while_running` jobs outlive a page change. Add the page, or say why not.
- [ ] Define what PR 9 builds on, in PR 9's shape: one `apply_value(d, cfg, renderer, probe, value)` and a `KeyStatus { pressed, state }` struct passed to `render` (PR 9 adds `live`).
- [ ] Write the watcher loop as code, not prose: the struct fields (`watched`, `last`, `next_due: HashMap<Probe, Instant>`, `in_flight`, `fast_until`, …) and pseudo-code for one tick. "`last.remove(&p)` at each `refresh_after()`" means scheduled forced re-reads; say so.
- [ ] Say that `Probe::Command { interval }` being part of the `Hash` key is intended (same command at two intervals = two probes).
- [ ] Add a test for a toggle key that is also `busy_while_running`: how the busy face and the settle step after the job interact.
- [ ] Keep the Omarchy presets with unconfirmed flags (`--status`, the DND file) out of the model's tasks, or check them on the machine and give exact commands.
- [ ] Split into sessions: (a) watcher swap only (`state.rs` + `watch.rs` with `Volume`, no behaviour change; PR 4/6 checks pass), (b) `Probe::Command` (spawn, reap, killpg), (c) config `OnFace`/`face_on`/`action_for` + the render parameter, (d) fast toggles (Press flip, `Expect` settle), (e) slow toggles (busy, `Leave`, `[key.busy]`, `draw_bar`), (f) `busy_while_running`, (g) GUI toggle group, (h) GUI presets and the Service dialog.

**Outcome:** any key can get a second face, `[key.on]`, with its own label, icon or glyph,
and colours. Which face shows comes from the real system: a shell command's exit status
polled in the background (night light, stay awake, Do Not Disturb, Bluetooth, …), or the
mute state the daemon already reads for a volume mute key. A key with no source flips on
each press, as the Elgato app's two-state actions do. A command toggle can also report
**busy** (exit 75) with a progress line, for things that take a while to come up or go
down, like a local model server (e.g. vLLM as a systemd user service), a VM or a Docker
stack: the key shows the progress
text and the watcher polls faster until it settles. Separately, **any** key can opt in to
looking busy for as long as its own command runs (`busy_while_running`), with the same
busy face, progress text and optional bar fed from the command's output. The audio worker is folded into a new
generic watcher thread, so the volume dials and the mute keys share one read. The GUI gets
toggle presets for what Omarchy ships. Old configs load and save byte-identical.

## How the Elgato app does it (researched 2026-10-16)

- An action declares `States` in its manifest. "Stream Deck supports up to two states";
  more can be declared but "its functionality is not fully supported". With two states,
  "the action will act as a toggle".
  ([keys guide](https://docs.elgato.com/streamdeck/sdk/guides/keys),
  [manifest](https://docs.elgato.com/streamdeck/sdk/references/manifest))
- **The state flips on press automatically**, and the new index is in the event payload.
  A plugin can opt out with `DisableAutomaticStates` and drive it with `setState(0|1)`,
  which is how plugins show real state (e.g. a mute that follows the system).
- **Each state has its own image, title, title colour, alignment and font**, all
  user-overridable. Users "configure the icon for each state within Stream Deck". A plugin
  can only set a state's image when the user hasn't picked one. Whether users pick a
  per-state *title* in the app is **unconfirmed** (the docs show plugins doing it).
- **One action per key; the state advances.** The built-in exceptions are System →
  **Hotkey Switch** (two hotkeys, used alternately) and **Multi Action Switch** (Stream Deck
  4.4: two multi-actions, e.g. start/stop stream)
  ([hotkeys how-to](https://www.elgato.com/eu/el/explorer/products/stream-deck/how-to-set-up-stream-deck-hotkeys/),
  [4.4 release notes](https://help.elgato.com/hc/en-us/articles/5162934218637-Elgato-Stream-Deck-4-4-Release-Notes)).
  So "a second action for the second state" is Elgato behaviour too.
- Multi-actions can pick a desired state per step (`Name` per state, `userDesiredState`).
  Out of scope until PR 12.
- **Unconfirmed:** whether the app remembers an optimistic state across restarts, and what
  the app's property inspector looks like for choosing which state you are editing.
- Dials (encoders) have no `States`; their feedback is the strip layout. Nothing to copy.
- **No busy state.** Elgato has no third "in progress" state. Plugins show progress by
  calling `setTitle` repeatedly (e.g. "Loading 43%") and can flash `showAlert` (a warning
  triangle) or `showOk` (a check mark) once an operation ends
  ([keys guide](https://docs.elgato.com/streamdeck/sdk/guides/keys)). omadeck's busy face is
  the same idea (a progress title) driven by the state command instead of plugin code.

## What Omarchy ships (github.com/basecamp/omarchy `master`, 2026-10-16)

| Toggle | Action command | State command (exit 0 = on) | Notes |
|---|---|---|---|
| Night light | `omarchy-toggle-nightlight` | `omarchy-toggle-nightlight --status \| jq -e .enabled` | `--status` prints `{"enabled":…}` from `hyprctl hyprsunset temperature` |
| Stay awake (idle off) | `omarchy-toggle-idle` | `omarchy-toggle-idle --status \| jq -e .enabled` | a flag file under `~/.local/state/omarchy/indicators/` |
| Do Not Disturb | `omarchy-toggle-notification-silencing` | `jq -e .dnd ~/.local/state/omarchy/notifications.json` | DND is the `dnd` key in that file (`docs/notifications.md`); `omarchy-shell notifications dndState` exists but its output is unconfirmed |
| Bluetooth | `omarchy-bluetooth-power toggle` | `omarchy-bluetooth-power is-on` | `is-on` can take ~2 s when D-Bus is slow |
| Mic mute | `volume` action, input, mute | built in (audio) | `omarchy-audio-input-mute` behind it (PR 4) |
| Output mute | `volume` action, output, mute | built in (audio) | |
| Generic flags | `omarchy-toggle <flag>` | `omarchy-toggle-enabled <flag>` | a file per flag in `~/.local/state/omarchy/toggles/` |

Not offered as presets: Wi‑Fi (`omarchy-network-wifi-connected` answers "connected over
Wi‑Fi", not "radio on", and there's no Omarchy radio toggle), VPN (no Omarchy command; the
README shows an `nmcli` example instead). Whether Omarchy 4.0.4 (the installed version) has
`--status` on the nightlight and idle scripts is **unconfirmed**: check on the machine; if
not, the presets fall back to `test -f` on the flag file and `pgrep -x hyprsunset`.

## Config

A key becomes a toggle by having an `[key.on]` table. Everything toggle-related lives in it.

```rust
// config.rs
pub struct KeyConfig {
    …
    /// A second face shown while the key is on, and where on/off comes from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<OnFace>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OnFace {
    // The look while on; each unset field falls back to the key's own.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label_color: Option<String>,
    /// A shell command; exit 0 = on, 75 = busy (first stdout line = progress text), any
    /// other status = off. Unset: follow the mute of a volume mute action, else flip on
    /// each press.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub state: Option<String>,
    /// Seconds between runs of `state`. Clamped to at least 1.
    #[serde(default = "two", skip_serializing_if = "is_two")] pub interval: u16,
    /// Run this instead of the key's action while on (Elgato's Hotkey Switch).
    #[serde(default, skip_serializing_if = "Action::is_none")] pub action: Action,
}

/// The look while a command toggle reports busy (exit 75). Its presence also marks the key
/// as a *slow* toggle (see "Busy state" below); an empty `[key.busy]` is valid.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct BusyFace {
    /// Unset: the command's progress line, else "…".
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub icon: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub glyph: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub background: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub label_color: Option<String>,
    /// Draw a progress bar when the busy text contains a percentage (the last `NN%`).
    /// Off by default: text only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")] pub bar: bool,
}

// on KeyConfig, next to `on`:
    /// The busy face of a command toggle. Ignored without `on.state`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub busy: Option<BusyFace>,

/// Where a toggle key's on/off comes from. Core, so the GUI can name it.
pub enum StateSource {
    /// No source: flips on each press (Elgato's automatic states).
    Press,
    /// `sh -c command`, exit 0 = on, 75 = busy, else off, every `interval`.
    Command { command: String, interval: Duration },
    /// The mute of an audio device, read by the daemon's watcher. On = muted.
    Mute(AudioDevice),
}

/// What a toggle key currently shows. Core (`state.rs`), shared by daemon, render and GUI.
pub enum Toggle { Off, On, Busy(String) }
```

- `KeyConfig::state_source() -> Option<StateSource>`: `None` without `on`; `Command` when
  `on.state` is set and not blank (a command wins); `Mute(device)` when the key's action is
  `Volume { mute: true, .. }`; else `Press`. This derives the built-in source from the
  action, the same way `DialConfig::level_source()` does for the strip.
- `KeyConfig::face_on(on: bool) -> Face`: the base face, or the merged on face. If the on
  face sets `icon` **or** `glyph`, it replaces both of the base's (one "image" per state, as
  in Elgato; otherwise a base icon would always beat an on glyph). A `label` of `Some("")`
  means no label while on.
- `KeyConfig::action_for(on: bool) -> &Action`: `on.action` when on and set, else `action`.
- `KeyConfig::face_for(t: &Toggle) -> Face`: `Off`/`On` as `face_on`; `Busy(text)` is the
  **off** face with the busy face's fields merged over it (same image rule as `face_on`),
  and the label = `busy.label`, else `text`, else "…". Without `[key.busy]` the label colour
  defaults to the accent so a busy key reads as different from off.
- `state::progress(text) -> Option<u8>`: the last `NN%` in the busy text (0–100, clamped;
  `43%`, `43.5 %` → 43, none → `None`). Used only when `busy.bar` is set.
- `KeyConfig::is_slow_toggle()`: `busy.is_some()` and the source is `Command`.
- `is_blank` adds `&& self.on.is_none() && self.busy.is_none()`; `KeyConfig::new` sets both
  to `None`.
- "On" for a mute key means **muted**: the toggle being on is the thing the key does.

```toml
[[key]]
index = 11
label = "Night light"
glyph = "f0594"
[key.action]
type = "app"
command = "omarchy-toggle-nightlight"
[key.on]
label = "Night on"
background = "#3b2a12"
state = "omarchy-toggle-nightlight --status | jq -e .enabled"

[[key]]
index = 10
label = "Mic"
glyph = "f036c"
[key.action]
type = "volume"
device = "input"
mute = true
[key.on]                    # no state: follows the mic's mute
label = "Muted"
glyph = "f036d"

[[key]]                        # a slow toggle: any systemd user service (here a model server)
index = 4
label = "LLM off"
glyph = "f048b"
[key.action]
type = "app"
command = "systemctl --user is-active -q llm.service && systemctl --user stop --no-block llm.service || systemctl --user start --no-block llm.service"
[key.on]
label = "LLM on"
state = """
s=$(systemctl --user is-active llm.service)
case $s in
  active) curl -sf -o /dev/null -m 2 http://127.0.0.1:8000/health && exit 0
          echo Loading; exit 75 ;;
  activating|deactivating|reloading) echo "$s"; exit 75 ;;
  *) exit 1 ;;
esac"""
[key.busy]                     # empty: the label comes from the command's output
```

To also draw a progress bar, opt in with `bar` and print a percentage while busy:

```toml
[key.busy]
bar = true                     # "Loading 43%" → label "Loading 43%" + bar at 43%
```

```sh
echo "Loading 43%"; exit 75    # bar at 43%
echo "Starting"; exit 75       # no percentage: text only, no bar
```

The **Service** preset (GUI, below) writes exactly this from a unit name and an optional
health URL, so nobody has to type it. Without a health URL, `active` means on.

No new `Action` variant and no `KINDS` change: a toggle is a key property, not an action.

## The state watcher (`omadeck-core/src/state.rs` + `crates/omadeckd/src/watch.rs`, new)

Replaces `AudioCmd`/`AudioWorker`/`start_audio_worker` in `main.rs`. The `Probe` and
`Value` types live in **core** (`state.rs`), because the renderer and the configurator
(the `KeyState` event) need them too; the worker thread (`Watcher`, `WatchCmd`, `start`)
lives in the daemon's `watch.rs`. Later PRs add variants to the core types and match arms
to the worker. The deck loop keeps the latest values in `Deck.values: HashMap<Probe, Value>`.

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe {
    /// An audio device's level and mute (was the audio worker).
    Volume(AudioDevice),
    /// `sh -c cmd`; exit 0 = on, 75 = busy (with its first stdout line), else off.
    Command { cmd: String, interval: Duration },
    // PR 9: CommandOutput { cmd, interval }, Clock, Cpu, …  PR 10: App(..)  PR 14: Player(..)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Toggle(Toggle),
    Level(audio::Level),
    // PR 9: Text(String) …
}

pub enum WatchCmd {
    /// The complete set to poll from now on (replaces the previous set).
    Watch(Vec<Probe>),
    /// We just changed this; re-read soon and send the result even if unchanged.
    Refresh(Probe),
    /// A slow toggle was pressed while showing `from`: poll it fast until it reads
    /// something other than `from` that isn't busy, or `FAST_WINDOW` passes.
    Leave { probe: Probe, from: Toggle },
}

pub struct Watcher { pub cmd: Sender<WatchCmd>, pub values: Receiver<(Probe, Value)> }
pub fn start() -> Watcher
```

**How each probe is read.** `Probe` says how, so a later variant only adds match arms:
- `fn interval(&self)`: `Volume` 1 s (today's `AUDIO_POLL`), `Command` its own, never < 1 s.
- `fn refresh_after(&self) -> &'static [Duration]`: `Volume` `[150 ms]` (today's
  `AUDIO_REFRESH_DELAY`); `Command` `[300 ms, 1200 ms]`, a second read for slow toggles.
- `fn read_now(&self) -> Option<Option<Value>>`: in-thread reads, `Some` for `Volume`
  (`audio::volume(dev).map(Value::Level)`, a few ms, as today). `None` = spawned probe.
- `fn spawn(&self) -> Option<Command>`: `sh -c cmd`, `process_group(0)`, stdin and stderr
  null, stdout **piped**, `LC_ALL=C`, cwd `$HOME`. A small reader thread per child reads
  stdout to EOF and keeps the first 4 KiB, so a chatty command can never block on a full
  pipe; only the first non-empty line is used, trimmed, max 64 chars. PR 9's
  `CommandOutput` reuses the same spawn and reader.

**Polling speed.** A probe's next poll is `interval()`, except:
- **while its last value is `Busy`**: every `BUSY_INTERVAL = 500 ms`, so progress moves;
- **during a `Leave` window**: every 500 ms until a read is neither `Busy` nor equal to
  `from` (the toggle finished), or `FAST_WINDOW = 15 s` passes (it never started, e.g. the
  command failed). The window covers the gap before a slow toggle reports busy at all.
`BUSY_INTERVAL` is the only place a probe runs faster than once a second; `MAX_IN_FLIGHT`
and "never two at once per probe" still apply, so a state command slower than 500 ms just
runs back to back.

**The thread** (`name("watch")`), a loop with a 50 ms tick like the audio worker:
1. Drain commands. `Watch(set)`: replace `watched`; `last.retain(in set)`; kill in-flight
   children of dropped probes; new probes are due now. `Refresh(p)`: `last.remove(&p)` at
   each of `p.refresh_after()` (the result is sent even if equal: the deck may be showing
   an optimistic guess), and the next regular poll moves to after the last one.
2. For each due probe: `read_now` probes run inline. Spawned probes start a child, unless
   one is already in flight for that probe (never two at once) or `MAX_IN_FLIGHT = 8` are
   running (it waits a tick). In-flight: `HashMap<Probe, (Child, Instant)>`.
3. Reap: `child.try_wait()` each tick, then join its reader. Exit 0 → `Toggle::On`, 75 →
   `Toggle::Busy(first line)`, anything else → `Toggle::Off`. A change in the busy text
   counts as a change, so progress repaints.
4. Change detection: send `(probe, value)` only if `last.get(&probe) != Some(&value)`, then
   store it. A send error (main loop gone) ends the thread.
5. Sleep 50 ms.

**Errors.** Nothing slow or failing reaches the deck loop:
- A command that runs past `COMMAND_TIMEOUT = 5 s` is killed with its whole process group
  (`libc::killpg(pgid, SIGKILL)`, so a pipeline dies too; `libc` 0.2 is already in
  `Cargo.lock` as a transitive dependency; add it to `omadeckd`'s `[dependencies]`) and reaped; no value is sent (the key keeps its last face), and a
  warning is logged **once** per probe until it succeeds again.
- Exit 127/126 (not found / not executable) counts as off and logs a warning once
  ("state command not found: …", most likely a typo). Other non-zero exits are just "off".
- A spawn error (no `sh`) logs once and retries at the next interval.
- `Volume` reads returning `None` (no `wpctl`) send nothing, as today.

**Cost:** a page with no toggles and no volume dials watches nothing and the thread only
ticks. Commands are only watched for the current page, so a 10-page config runs at most
15 key probes + the dials.

**Fold the audio worker in now.** A mute key needs the same `audio::volume` read the
volume dial makes; keeping the audio worker means either teaching it about keys or polling
the same device twice from two threads of the same shape, which is the duplication the
shared-blocks section exists to avoid. The move is mechanical (`AudioCmd::Watch` →
`Probe::Volume`, `Refresh` → `Refresh(Probe::Volume(dev))`, same 1 s / 150 ms timings), and
the PR 4/6 hardware checks re-run cheaply. PR 10 then adds stream probes to one place.

## Daemon (`main.rs`)

`Deck` changes:
- `device_levels: HashMap<AudioDevice, Level>` → `values: HashMap<Probe, Value>`, the
  contract's map. `dial_status` reads `values.get(&Probe::Volume(dev))` and calls
  `audio::Level::to_panel()`.
- `key_probes: Vec<Option<KeySource>>`, per key index on the **current page**, where
  `KeySource` is `Probe(Probe)` or `Press`; recomputed on connect, reload and page change,
  like `dial_levels`.
- `flipped: HashSet<(u8, u8)>`: (page, index) of `Press` toggles that are on.
- `settle: HashMap<Probe, Settle>`: optimistic values waiting for confirmation, where
  `Settle::Expect(Value, Instant)` (fast toggles: ignore reads that contradict the guess
  until the deadline) or `Settle::Leaving(Toggle, Instant)` (slow toggles: ignore reads
  still equal to the state the press left, until the deadline).

`fn watched_probes(cfg, page, n_dials) -> Vec<Probe>` replaces `watched_devices`: the
dials' `Volume` probes plus the current page's keys' probes, deduplicated. Sent as
`WatchCmd::Watch` on connect, reload and `do_page`; `Watch(vec![])` on disconnect.

`fn key_toggle(d, cfg, i) -> Toggle`: `Press` → `flipped`; probe → `values`
(`Toggle(t)` → t, `Level(l)` → `On` if `l.muted`); unknown → `Off`. `write_key` renders
with it; a new `key_is_on` is `matches!(key_toggle(..), Toggle::On)`.

Press (`ButtonDown`) on a toggle key:
1. `t = key_toggle(..)`; run `k.action_for(t == On)` through the existing match (page,
   brightness and ordinary actions all work). **While busy**, it runs the base action
   (`action_for(false)`): the script behind a slow toggle knows best what a press during
   loading means (cancel, or nothing).
2. If the action failed, stop: no flip.
3. Show the press at once, as Elgato does:
   - `Press` → toggle `flipped`.
   - `Command`, **fast toggle** (no `[key.busy]`) → `values.insert(p, Toggle(!on))`,
     `settle.insert(p, Expect(Toggle(!on), now + SETTLE))` with `SETTLE = 1 s`, send
     `Refresh(p)`.
   - `Command`, **slow toggle** (`[key.busy]`) → `values.insert(p, Toggle(Busy("")))`
     (the busy face with "…"), `settle.insert(p, Leaving(t, now + FAST_WINDOW))`, send
     `Leave { probe: p, from: t }`. If the key was already busy, only the `Leave` is sent.
   - `Volume` → unchanged `after_volume` path (which now writes `values` and sends
     `Refresh(Probe::Volume(dev))`); no settle, PR 4's behaviour already corrects within
     150 ms.
4. `ButtonUp` repaints with the new face (it already repaints the key).

Values from the watcher (replacing the `audio.levels` drain): for `(p, v)`, check `settle`:
- `Expect(guess, until)`: before `until`, ignore a `v` that differs from `guess` (a toggle
  that hasn't finished yet);
- `Leaving(from, until)`: before `until`, ignore a `v` equal to `from` (the slow toggle
  hasn't started yet; showing it would flash the old face between the press and the first
  busy read). A `Busy` or the other state ends the settle.
Otherwise drop the settle entry, store `v`, and repaint every key on the current page whose
probe is `p` and every dial whose level source is `Probe::Volume`. The second refresh read
at 1.2 s is outside the `Expect` window, so a fast toggle that really failed shows the true
state within ~1.2 s; a slow toggle that never starts shows its true state after 15 s.

Reload: recompute `key_probes`/`dial_levels`; `values.retain` to probes the new config
still uses on **any** page; `flipped.retain` to keys that are still `Press` toggles; send
`Watch`. Page change: `values` is **not** pruned, so a page you return to shows the last
known faces at once and corrects on the first read (no flash to "off"). Reconnect: the
`Deck` is rebuilt, so values and `flipped` start empty; Press toggles reset to off.

`omadeckd press KEY` keeps running the key's base action (it can't know the state).

## Busy while a command runs (any key)

The same busy face, for the time a key's own command takes: a backup script, a
`git pull` of dotfiles, a build. Opt-in per key; it works on toggles and plain keys alike.

```rust
// on KeyConfig
/// Show the busy face until this key's command exits. Only for `app` (a plain command,
/// not a `.desktop` id) and `script` actions; ignored for others, which finish at once.
#[serde(default, skip_serializing_if = "std::ops::Not::not")]
pub busy_while_running: bool,
```

```toml
[[key]]
index = 6
label = "Backup"
glyph = "f0bc8"
busy_while_running = true
[key.action]
type = "script"
path = "~/bin/backup"          # prints "Copying 43%" lines as it goes
[key.busy]                     # optional; label from the output, bar from its NN%
bar = true
```

**Core (`action.rs`).** `pub fn run_tracked(action) -> Result<Option<Running>>`:
- `App` with a plain command and `Script` → spawned **directly** (`sh -c` / the script,
  `process_group(0)`, stdin and stderr null, stdout piped), **not through `uwsm-app`**:
  `uwsm-app` hands the command to a systemd scope and exits at once, so there would be
  nothing to wait for. A tracked command is a job, not an app that needs its own scope.
  A script with `terminal = true` runs inside the terminal as today; it reads as busy
  while the terminal is open, which only works if the terminal doesn't hand off to an
  already-running instance (noted in Hardware checks).
- Everything else → `run(action)` as today and `Ok(None)` (nothing to track).
- `Running { child: Child, lines: Receiver<String> }`: a reader thread sends each
  non-empty stdout line (trimmed, max 64 chars); the shared line reader from the watcher,
  in `state.rs`, so both use one implementation.

**Daemon.** `running: HashMap<(u8, u8), Running>` keyed by (page, index), held in the main
loop's state **outside `Deck`**, so an unplug/replug doesn't lose track of a job.
- Press on a key with `busy_while_running`: if it is already running, **ignore the press**
  (log it; a second backup on top of the first is never what you meant). Otherwise
  `run_tracked`; on `Some(r)`, insert it, set the key to `Busy("")` and publish
  `KeyState`. On `None` (the action kind can't be tracked), behave as today.
- Each loop pass (≤ 100 ms, the existing `READ_TIMEOUT`): drain each job's `lines` and
  keep the **latest** line as the busy text (repaint on change); `try_wait()` the child.
  On exit: remove it, repaint with the key's normal state, and on a non-zero status
  publish `ActionFailed { index, message: "exited with status N" }` (the GUI toasts it, as
  for a failed launch). A toggle key then gets `Refresh` (fast) or `Leave` (slow) so its
  state catches up at once.
- **Face precedence:** a running job shows busy over any toggle state; when it ends, the
  toggle's own state (or off) shows again.
- Page change: jobs keep running; a key on another page shows busy when you return.
  Reload: entries whose key no longer exists or no longer has the flag are still reaped
  but not shown. Daemon exit: jobs are left running (they are in their own process
  group, as spawned commands are today).
- No timeout: a job may legitimately run for an hour. The fast polling is only of the
  child's exit, which costs nothing.

**IPC:** `KeyState` (above) is also published for non-toggle keys while they run, with
`on: false`; the configurator shows the busy face and the progress live.

## IPC

`Event::KeyState { page: u8, index: u8, on: bool, busy: Option<String> }` (`busy` omitted
when `None`), published when a toggle key's face changes, including each change of busy
text, and for every toggle key on the page after connect, reload and page change. Old
configurators ignore it.

## Render

`Renderer::render(key, style, size, pressed)` gains `toggle: &Toggle` and draws
`key.face_for(toggle)`. When on and the on face sets no `background`, the background is
the key's mixed 30% towards the accent (`theme.accent` with `follow_theme`, else the label
colour), so a bare `[key.on]` is still visibly different. Busy uses the off face's
background (or `busy.background`) with an accent label, and the progress text wraps to two
lines and ellipsizes like any label. With `busy.bar` and a percentage in the text, a
progress bar is drawn along the bottom of the key: the same groove bar as the strip's
`$B1` indicator (fully rounded ends; it's inline in `draw_b1` today, so first extract
`fn draw_bar(img, rect, fill, track, fg)` from `draw_b1` as its own step, with
`$B1`'s pixels unchanged), inset 10 px from the
sides and 8 px from the bottom, 8 px high, accent fill on a `muted` track (white on
`#808080` without `follow_theme`, as on the strip); the label moves up to make room. No
percentage, or `bar` off: no bar, text only. No animation: a spinner would need a repaint timer on
the deck loop for no information the text doesn't already give. Callers: daemon
`write_key` and `snapshot` (front page, off faces), GUI `App::refresh_key`.

## GUI

- `App.key_states: RefCell<HashMap<(u8, u8), Toggle>>` from `Event::KeyState`;
  `refresh_key` renders with it, so the grid preview shows what the deck shows, progress
  text included.
- Key editor, new **Toggle** group under the action:
  - a **Two faces (toggle)** switch, adding or removing `[key.on]`;
  - **State** combo: *Flip on each press* / *Shell command* / *Follows mute* (shown only
    when the action is a volume mute, and then the default); a command entry and an
    interval spin (1–60 s) for *Shell command*, with the hint "exit 0 = on, 75 = busy";
  - a **Slow (shows progress)** switch for *Shell command*, adding or removing `[key.busy]`;
  - a **Progress bar** switch under it (slow toggles only), setting `busy.bar`, with the
    hint "draws a bar when the command prints a percentage";
  - an **On action** expander using the existing `action_editor` ("When pressed while on",
    default Nothing = same action).
- **Busy while running** switch in the key's action group, shown only for `app` (plain
  command) and `script` actions, with the hint "show the busy face until the command
  finishes". When on, the Appearance group gets the Busy editing button and the Progress
  bar switch even for a key that isn't a toggle.
- **Editing: Off | On | Busy** linked buttons in the Appearance group header (only for
  toggles; Busy only for slow ones). On and Busy make the label, icon, glyph and colour
  rows edit `on.*` / `busy.*`, and pin this key's preview to that face while it is selected
  (Busy previews with "Loading 42%", and a bar at 42% when `bar` is on). Unset fields show the base value as a placeholder;
  Busy's label placeholder is "from the command".
- **Toggle presets** menu button in the key group header, beside *Try it now*: Mic mute,
  Output mute, Night light, Stay awake, Do Not Disturb, Bluetooth, and **Service…** (a
  small dialog: a systemd user unit, picked from `systemctl --user list-unit-files
  --type=service` or typed, and an optional health URL; it fills the slow-toggle example
  above, with `[key.busy]`). Each fills action,
  label, glyphs, `on` face and state from the table above. A preset whose command isn't
  on `PATH` is insensitive with a tooltip ("needs Omarchy 4").
- *Try it now* runs `action_for(the face being edited)`.

## Starter layout (`starter.rs`)

Key 10 "Mute" becomes `Volume { output, mute }` with an on face (`f075f`, "Muted"); key 11
"Night light" gets the night light on face and state. New installs only.

## Docs

README Features ("Toggle keys: a second face that follows real state, with progress for
slow toggles"), tick the roadmap's **Toggle keys** item, a short README section on writing
a state command (exit 0 / 1 / 75 and the progress line), `examples/config.toml` gets the
night light and mic examples above plus a commented VPN example (`nmcli -t -f TYPE
connection show --active | grep -q vpn`) and the commented service example, CHANGELOG
line.

## Implementation steps

1. [ ] `state.rs` (core) with `Probe::Volume` and `Value::Level` only, and `watch.rs`; swap the audio worker for
       it; `device_levels` → `values`. No behaviour change. Re-run PR 4/6 checks.
2. [ ] `Probe::Command`, `Toggle`, `Value::Toggle`, spawn/reap/timeout/killpg, warn-once,
       the stdout reader, exit 75 → busy.
3. [ ] Config: `OnFace`, `BusyFace`, `StateSource`, `state_source`, `face_on`, `face_for`,
       `action_for`, `is_slow_toggle`, tests.
4. [ ] Render: the `toggle` parameter, the accent-tinted on default, the busy face.
5. [ ] Daemon: `key_probes`, `watched_probes`, `flipped`, press flip, settle, repaint,
       reload and page change; `Event::KeyState`.
6. [ ] Busy polling: `BUSY_INTERVAL`, `WatchCmd::Leave` and `FAST_WINDOW`, `Settle::Leaving`.
7. [ ] Busy while running: `busy_while_running`, `run_tracked` (bypassing `uwsm-app`),
       the shared line reader, the daemon's `running` map, exit status → `ActionFailed`.
       Try it with a test unit (see Hardware checks).
8. [ ] GUI: live preview, Toggle group, Off/On/Busy editing, Busy while running, presets.
9. [ ] Starter, README, example, CHANGELOG.

## Tests

- config: `toggle_roundtrip_and_defaults` (interval 2 and empty fields omitted);
  `old_config_saves_byte_identical` (load the example without `on`, save, compare);
  `key_with_only_on_is_not_blank`; `state_source_command_wins_over_mute`;
  `state_source_mute_from_volume_action`; `state_source_press_by_default`;
  `blank_state_is_press`; `interval_clamped_to_one_second`;
  `face_on_merges_and_image_replaces_both`; `face_on_empty_label_hides_label`;
  `action_for_uses_on_action_only_when_on`; `empty_busy_table_roundtrips` (`[key.busy]`
  with no fields survives a save); `face_for_busy_label_order` (busy.label, then the
  text, then "…"); `busy_without_state_is_not_slow`; `example_parses` covers the new
  examples.
- watch: `probe_intervals_and_refresh_delays`; `command_true_is_on`/`false_is_off`
  (spawn `true`/`false` through the real loop, receive within 500 ms);
  `exit_75_is_busy_with_first_line` (`printf 'x\nLoading 43%%\n'; exit 75` → `Busy("Loading
  43%")`, blank lines skipped); `busy_text_change_is_resent`;
  `busy_polls_at_busy_interval`; `leave_polls_fast_until_settled_or_window`;
  `large_output_does_not_block` (`yes | head -c 1000000; exit 0` → `On`);
  `unchanged_value_not_resent`; `refresh_resends_unchanged`;
  `hung_command_is_killed_and_sends_nothing` (`sleep 30` with the timeout lowered by a
  `cfg(test)` constant); `unwatch_kills_in_flight`; `missing_command_is_off`.
- daemon (pure helpers): `watched_probes_current_page_and_dials_deduped`;
  `settle_ignores_contradicting_value_until_expiry`;
  `leaving_ignores_old_state_but_takes_busy_and_new_state`.
- ipc: `KeyState` roundtrip and wire shape (`busy` omitted when `None`).
- render: `on_face_without_background_is_tinted`, `off_face_unchanged` (pixel compare
  against `render` with `Toggle::Off`), `busy_face_uses_accent_label_by_default`,
  `busy_bar_only_when_enabled_and_percent_present` (bar off + `43%` → no bar pixels;
  bar on + `Starting` → no bar; bar on + `43%` → bar fill ends at 43% of its width).
- state: `progress_parses_last_percentage` (`"shard 2/4 43%"` → 43, `"120%"` → 100,
  `"43.5 %"` → 43, `"no number"` → `None`).
- config: `busy_bar_default_off_and_omitted` (`bar = false` isn't written);
  `busy_while_running_default_off_and_omitted`.
- action: `run_tracked_none_for_instant_kinds` (url, hyprland, volume, `.desktop` id →
  `None`); `run_tracked_app_bypasses_uwsm` (the built command is `sh -c …`, never
  `uwsm-app`); `run_tracked_streams_latest_line` (`for i in 1 2 3; do echo "$i%"; done` →
  lines arrive in order, child exits 0).
- daemon (pure helpers): `press_while_running_is_ignored`;
  `running_job_overrides_toggle_face`; `nonzero_exit_reports_failure`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy 4)

- [ ] Volume dials and the mic dial behave exactly as before the watcher swap (level,
      mute, external changes within ~1 s, optimistic bar).
- [ ] Mic mute key: the face follows the mute when toggled from the key, the dial and the
      keyboard's mic key; a mic dial and the key agree.
- [ ] Night light key: press flips the face at once and it stays flipped (no flicker),
      including the first press after login when hyprsunset is still starting.
- [ ] Stay awake, DND and Bluetooth presets follow changes made from the Omarchy menu/bar.
- [ ] Note whether 4.0.4 has `--status` on the nightlight and idle scripts.
- [ ] A state command of `sleep 30`: the deck stays responsive, one warning in the log,
      no `sleep` processes pile up (`pgrep -c sleep`).
- [ ] A typo'd command: off face, one warning, no repeat spam.
- [ ] Progress bar: a test state command that counts `10%`…`100%` over 20 s with
      `bar = true` draws a bar that fills in steps about twice a second; the same with
      `bar` off shows text only; a busy line with no percentage shows no bar.
- [ ] Busy while running: a script printing `Step 10%` … `Step 100%` once a second, with
      `busy_while_running` and `bar`: the key shows the steps and the bar filling, then
      its normal face when it exits. A second press while it runs does nothing.
- [ ] A script that exits 1: busy, then the normal face and a toast in the configurator.
- [ ] Start a long job, change page and come back: still busy. Unplug and replug the deck
      mid-job: busy again after reconnect, normal when the job ends.
- [ ] A `terminal = true` script with `busy_while_running`: note whether Omarchy's
      terminal stays the child (busy until the window closes) or hands off (busy flashes).
- [ ] For a real model server, find a percentage it reports while loading (vLLM: its
      log's `Loading safetensors checkpoint shards: NN%` lines, e.g. `journalctl --user -u
      llm.service -n 20 -o cat | grep -o '[0-9]*%' | tail -n 1`; format unconfirmed).
- [ ] A Press toggle with an on action (e.g. `togglefloating` pair) alternates actions.
- [ ] Page away and back: faces show the last state immediately.
- [ ] Idle CPU of `omadeckd` with three command toggles on screen stays ~0%.
- [ ] Service preset with a test unit that takes a while: `ExecStart=sh -c 'sleep 20; exec
      python -m http.server 8765'` and health URL `http://127.0.0.1:8765/`. Press while off:
      "…" at once, then "Loading" (or "activating"), then the on face once the server
      answers. Press again: "deactivating" if the stop takes time, then off. No flash of the
      old face right after either press.
- [ ] The same with a real model server (vLLM) that takes minutes: fast polling continues
      for the whole load, not just the first 15 s, and CPU stays low (one state command at
      a time).
- [ ] A unit started at login (enabled): the key shows Loading at startup and turns on
      by itself when the server is ready, with no press.
- [ ] The service crashes or is stopped from a terminal while the key shows on: the key
      goes off within one interval.
- [ ] Config without `on` saves byte-identical from the GUI.

## Decisions to explain

- **`[key.on]` rather than Elgato's numbered states.** Elgato caps usable states at two;
  "off face = the key, on face = overrides" keeps existing keys unchanged and the TOML
  minimal, and "on" reads better than "state 1" in a hand-edited file.
- **Real state is the point, a press flip the fallback.** Elgato's automatic flip only
  tracks the last press, which goes wrong as soon as you toggle from the keyboard or bar.
  omadeck still flips on press first (Elgato-like, instant), then trusts the command.
- **A shell command's exit status as the source**, not built-in knowledge of each tool: it
  works with every Omarchy version and any user script, mirrors how Omarchy's own scripts
  answer (`omarchy-toggle-enabled`, `is-on`), and the presets supply good commands.
- **The mute source is derived from the action**, like the strip's `level_source()`, so a
  mute key needs only `[key.on]`.
- **An optional on action** copies Elgato's Hotkey Switch/Multi Action Switch rather than
  inventing `on_press`/`off_press`; by default one action toggles, as in Elgato.
- **Press toggles are not persisted.** Writing the config on each press would trigger a
  reload and fight the GUI (brightness does it, but rarely); they reset to off on restart
  or replug. See Later.
- **Polling, not events.** Exit-status polling every 2 s costs a fork; event sources (D-Bus
  for BlueZ/NetworkManager, inotify on Omarchy's flag files) are per-tool work. The
  watcher's `Probe` shape can take an event-driven variant later without changing callers.
- **Only the current page is watched**, per the shared contract; values are kept across
  page changes to avoid a flash.
- **The audio worker is folded in now** (see above), with identical timings.
- **One warning per failing probe**, not per poll, so a broken command can't flood the
  journal.
- **A busy state, which Elgato doesn't have.** Some toggles take seconds to minutes (a
  local model, a VM, a container stack), and on/off alone shows the wrong thing for that
  whole time. Elgato plugins solve it by retitling the key with progress; omadeck has no
  plugins, so the state command reports it.
- **Exit 75 means busy.** It is `EX_TEMPFAIL` from `sysexits.h` ("temporary failure, try
  again"), which says exactly that. Exit 2 would be the obvious "third value", but `grep`
  and `jq` already return 2 on errors, so a preset like `jq -e .dnd file` would read as
  busy when the file is missing.
- **`[key.busy]` marks a slow toggle.** Its presence changes what a press does: show busy
  at once and poll fast until the state moves, instead of flipping to the other face and
  confirming within a second. A fast toggle that briefly exits 75 still gets a busy face;
  only the press behaviour differs.
- **Fast polling only while it matters**: every 500 ms while busy or for up to 15 s after
  pressing a slow toggle (the gap before it reports busy), and the normal interval
  otherwise. The numbers come from my earlier Stream Deck plugin, which tracked a local
  model server this way.
- **The progress bar is opt-in** (`busy.bar`). Text is always there; a bar only helps when
  the command reports a percentage, and a key that shows `Loading` with an empty bar
  stuck at 0% would mislead. Reading the percentage out of the text keeps one output
  convention instead of a second channel.
- **Busy while running is opt-in**, and only for `app` (plain command) and `script`
  actions. Most actions finish at once or launch an app that keeps running on purpose;
  showing those as busy would just flicker or never end.
- **A tracked command bypasses `uwsm-app`.** `uwsm-app` returns as soon as systemd has
  the command, so there's nothing to wait for. A job you want to watch is not an app
  that needs its own scope.
- **A second press while running is ignored.** Starting a second copy of a backup or a
  build is never what you meant; cancelling is listed under Later.
- **A non-zero exit is reported** like a failed launch (a toast in the configurator), so a
  job that failed doesn't look the same as one that finished.
- **Progress as text, no spinner.** The command's own words ("Loading", "Loading 43%") carry
  more than an animation would, and nothing needs a repaint timer.
- **Generic, not tied to a tool.** omadeck knows nothing about model servers. A systemd
  user service is the common case on Omarchy (autostarted or not), and `systemctl
  is-active` already reports `activating`/`deactivating`; a health URL adds "running but
  not ready yet". The Service preset only writes that shell command; any other slow
  thing works through its own state command.

## Later

- Toggle faces on dial panels (a dial press that is a toggle showing on/off on the strip).
- Persisting Press toggles to `$XDG_STATE_HOME/omadeck/toggles`.
- Event-driven probes: inotify on Omarchy's flag files and `notifications.json`, D-Bus for
  Bluetooth and NetworkManager (Wi‑Fi radio, VPN presets).
- Multi-action steps that set a toggle's state (Elgato's `userDesiredState`, with PR 12).
- `snapshot` running each state command once to show real faces.
- Busy while running for multi-actions (PR 12): a multi counts as running until its last
  step ends. PR 12 decides how its sequencer reports that.
- Cancelling a running job from the deck (e.g. a long press kills its process group).
- Elgato's `showOk`/`showAlert` flash when a slow toggle finishes or never starts.
- A busy indicator on a dial's strip panel.

**Size:** about 1,050 lines (watcher ~300 with tests, config ~180, action ~60, render ~75,
daemon ~250, GUI ~240, docs). The busy state adds about 130 (the optional bar ~30 of
that) and busy while running about 120. Split the GUI into a follow-up PR, as PR 5
allowed, unless the core and daemon land well under 800.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- Press-flip toggles (no `state` command) are kept in memory only and reset to off on restart or replug. Persist them instead?
- Night light and idle presets rely on `--status` in the Omarchy toggle scripts, read from `master`; confirm 4.0.4 has it (hardware check), else use the plan's fallback.
- Do Not Disturb state reads `~/.local/state/omarchy/notifications.json` because `omarchy-shell notifications dndState`'s output is unconfirmed. Keep, or switch once confirmed?
- Wi‑Fi and VPN presets are left out (no Omarchy command). Add the `nmcli` VPN example as a preset, or keep it in docs only?
- About 900 lines: split the GUI into a follow-up PR if it passes ~950?
- `busy_while_running` as the field name? Alternatives: `track`, `show_running`.
- A press while a tracked command runs is ignored (planned). Queue it, or start another
  copy?
- Pressing a slow toggle while it's busy runs the base action and leaves the face to the
  next read (planned). Ignore the press instead, or run the on action?
- Busy = exit 75 (planned). Any reason to prefer a stdout convention (e.g. a `busy:`
  prefix) instead?
- Default busy look: the off face with the progress as an accent-coloured label (planned).
  Want a distinct background or a glyph by default?
- `FAST_WINDOW` 15 s and `BUSY_INTERVAL` 500 ms (from the old plugin). Keep?
- Service preset: health check with `curl` (planned; is it always installed on Omarchy?)
  or a built-in HTTP check in the watcher?
