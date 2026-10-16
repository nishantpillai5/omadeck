# PR 8: `feat/toggle-keys`: keys with on/off faces that follow real state

Part of [PLAN.md](PLAN.md). Base: `feat/strip-backgrounds` (PR 7), which itself sits on
`feat/brightness`. Nothing from PR 7 is used; the base is only for a clean merge order.
This PR introduces the **state watcher** building block that PRs 9, 10 and 14 extend.

**Outcome:** any key can get a second face, `[key.on]`, with its own label, icon or glyph,
and colours. Which face shows comes from the real system: a shell command's exit status
polled in the background (night light, stay awake, Do Not Disturb, Bluetooth, …), or the
mute state the daemon already reads for a volume mute key. A key with no source flips on
each press, as the Elgato app's two-state actions do. The audio worker is folded into a new
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
    /// A shell command; exit status 0 means on. Unset: follow the mute of a volume mute
    /// action, else flip on each press.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub state: Option<String>,
    /// Seconds between runs of `state`. Clamped to at least 1.
    #[serde(default = "two", skip_serializing_if = "is_two")] pub interval: u16,
    /// Run this instead of the key's action while on (Elgato's Hotkey Switch).
    #[serde(default, skip_serializing_if = "Action::is_none")] pub action: Action,
}

/// Where a toggle key's on/off comes from. Core, so the GUI can name it.
pub enum StateSource {
    /// No source: flips on each press (Elgato's automatic states).
    Press,
    /// `sh -c command`, exit 0 = on, every `interval`.
    Command { command: String, interval: Duration },
    /// The mute of an audio device, read by the daemon's watcher. On = muted.
    Mute(AudioDevice),
}
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
- `is_blank` adds `&& self.on.is_none()`; `KeyConfig::new` sets `on: None`.
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
```

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
    /// `sh -c cmd`; exit status 0 = on.
    Command { cmd: String, interval: Duration },
    // PR 9: Text { cmd, interval }, Clock, Cpu, …  PR 10: Stream(..)  PR 14: Player(..)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Bool(bool),
    Level(audio::Level),
    // PR 9: Text(String) …
}

pub enum WatchCmd {
    /// The complete set to poll from now on (replaces the previous set).
    Watch(Vec<Probe>),
    /// We just changed this; re-read soon and send the result even if unchanged.
    Refresh(Probe),
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
- `fn spawn(&self) -> Option<Command>`: `sh -c cmd`, `process_group(0)`, stdin/stdout/
  stderr null, `LC_ALL=C`, cwd `$HOME`. Bool commands need no output, so no pipe and no
  reader thread. PR 9's `Text` adds a piped stdout read on a helper thread, capped at 4 KiB.

**The thread** (`name("watch")`), a loop with a 50 ms tick like the audio worker:
1. Drain commands. `Watch(set)`: replace `watched`; `last.retain(in set)`; kill in-flight
   children of dropped probes; new probes are due now. `Refresh(p)`: `last.remove(&p)` at
   each of `p.refresh_after()` (the result is sent even if equal: the deck may be showing
   an optimistic guess), and the next regular poll moves to after the last one.
2. For each due probe: `read_now` probes run inline. Spawned probes start a child, unless
   one is already in flight for that probe (never two at once) or `MAX_IN_FLIGHT = 8` are
   running (it waits a tick). In-flight: `HashMap<Probe, (Child, Instant)>`.
3. Reap: `child.try_wait()` each tick. Exit → `Value::Bool(status.success())`.
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
- `settle: HashMap<Probe, (Value, Instant)>`: optimistic values waiting for confirmation.

`fn watched_probes(cfg, page, n_dials) -> Vec<Probe>` replaces `watched_devices`: the
dials' `Volume` probes plus the current page's keys' probes, deduplicated. Sent as
`WatchCmd::Watch` on connect, reload and `do_page`; `Watch(vec![])` on disconnect.

`fn key_is_on(d, cfg, i) -> bool`: `Press` → `flipped`; probe → `values` (`Bool(b)` → b,
`Level(l)` → `l.muted`); unknown → off. `write_key` renders with it.

Press (`ButtonDown`) on a toggle key:
1. `on = key_is_on(..)`; run `k.action_for(on)` through the existing match (page,
   brightness and ordinary actions all work).
2. If the action failed, stop: no flip.
3. Flip optimistically, as Elgato does on press: `Press` → toggle `flipped`. `Command` →
   `values.insert(p, Bool(!on))`, `settle.insert(p, (Bool(!on), now + SETTLE))` with
   `SETTLE = 1 s`, send `Refresh(p)`. `Volume` → unchanged `after_volume` path (which
   now writes `values` and sends `Refresh(Probe::Volume(dev))`); no settle, PR 4's
   behaviour already corrects within 150 ms.
4. `ButtonUp` repaints with the new face (it already repaints the key).

Values from the watcher (replacing the `audio.levels` drain): for `(p, v)`, if `settle`
has `p`, it is still running and `v` differs from the guess, ignore it (a toggle that
hasn't finished yet); otherwise drop the settle entry, store `v`, and repaint every key on
the current page whose probe is `p` and every dial whose level source is `Probe::Volume`.
The second refresh read at 1.2 s is outside the window, so a toggle that really failed
shows the true state within ~1.2 s.

Reload: recompute `key_probes`/`dial_levels`; `values.retain` to probes the new config
still uses on **any** page; `flipped.retain` to keys that are still `Press` toggles; send
`Watch`. Page change: `values` is **not** pruned, so a page you return to shows the last
known faces at once and corrects on the first read (no flash to "off"). Reconnect: the
`Deck` is rebuilt, so values and `flipped` start empty; Press toggles reset to off.

`omadeckd press KEY` keeps running the key's base action (it can't know the state).

## IPC

`Event::KeyState { page: u8, index: u8, on: bool }`, published when a toggle key's face
changes, and for every toggle key on the page after connect, reload and page change. Old
configurators ignore it.

## Render

`Renderer::render(key, style, size, pressed)` gains `on: bool` and draws
`key.face_on(on)`. When on and the on face sets no `background`, the background is the
key's mixed 30% towards the accent (`theme.accent` with `follow_theme`, else the label
colour), so a bare `[key.on]` is still visibly different. Callers: daemon `write_key` and
`snapshot` (front page, off faces), GUI `App::refresh_key`.

## GUI

- `App.key_states: RefCell<HashMap<(u8, u8), bool>>` from `Event::KeyState`;
  `refresh_key` renders with it, so the grid preview shows what the deck shows.
- Key editor, new **Toggle** group under the action:
  - a **Two faces (toggle)** switch, adding or removing `[key.on]`;
  - **State** combo: *Flip on each press* / *Shell command* / *Follows mute* (shown only
    when the action is a volume mute, and then the default); a command entry and an
    interval spin (1–60 s) for *Shell command*;
  - an **On action** expander using the existing `action_editor` ("When pressed while on",
    default Nothing = same action).
- **Editing: Off | On** linked buttons in the Appearance group header (only for toggles).
  On makes the label, icon, glyph and colour rows edit `on.*`, and pins this key's preview
  to the on face while it is selected. Unset on fields show the base value as a
  placeholder.
- **Toggle presets** menu button in the key group header, beside *Try it now*: Mic mute,
  Output mute, Night light, Stay awake, Do Not Disturb, Bluetooth. Each fills action,
  label, glyphs, `on` face and state from the table above. A preset whose command isn't
  on `PATH` is insensitive with a tooltip ("needs Omarchy 4").
- *Try it now* runs `action_for(the face being edited)`.

## Starter layout (`starter.rs`)

Key 10 "Mute" becomes `Volume { output, mute }` with an on face (`f075f`, "Muted"); key 11
"Night light" gets the night light on face and state. New installs only.

## Docs

README Features ("Toggle keys: a second face that follows real state"), tick the roadmap's
**Toggle keys** item, `examples/config.toml` gets the two examples above plus a commented
VPN example (`nmcli -t -f TYPE connection show --active | grep -q vpn`), CHANGELOG line.

## Implementation steps

1. [ ] `state.rs` (core) with `Probe::Volume` and `Value::Level` only, and `watch.rs`; swap the audio worker for
       it; `device_levels` → `values`. No behaviour change. Re-run PR 4/6 checks.
2. [ ] `Probe::Command`, `Value::Bool`, spawn/reap/timeout/killpg, warn-once.
3. [ ] Config: `OnFace`, `StateSource`, `state_source`, `face_on`, `action_for`, tests.
4. [ ] Render: the `on` parameter and the accent-tinted default.
5. [ ] Daemon: `key_probes`, `watched_probes`, `flipped`, press flip, settle, repaint,
       reload and page change; `Event::KeyState`.
6. [ ] GUI: live preview, Toggle group, Off/On editing, presets.
7. [ ] Starter, README, example, CHANGELOG.

## Tests

- config: `toggle_roundtrip_and_defaults` (interval 2 and empty fields omitted);
  `old_config_saves_byte_identical` (load the example without `on`, save, compare);
  `key_with_only_on_is_not_blank`; `state_source_command_wins_over_mute`;
  `state_source_mute_from_volume_action`; `state_source_press_by_default`;
  `blank_state_is_press`; `interval_clamped_to_one_second`;
  `face_on_merges_and_image_replaces_both`; `face_on_empty_label_hides_label`;
  `action_for_uses_on_action_only_when_on`; `example_parses` covers the new examples.
- watch: `probe_intervals_and_refresh_delays`; `command_true_is_on`/`false_is_off`
  (spawn `true`/`false` through the real loop, receive within 500 ms);
  `unchanged_value_not_resent`; `refresh_resends_unchanged`;
  `hung_command_is_killed_and_sends_nothing` (`sleep 30` with the timeout lowered by a
  `cfg(test)` constant); `unwatch_kills_in_flight`; `missing_command_is_off`.
- daemon (pure helpers): `watched_probes_current_page_and_dials_deduped`;
  `settle_ignores_contradicting_value_until_expiry`.
- ipc: `KeyState` roundtrip and wire shape.
- render: `on_face_without_background_is_tinted`, `off_face_unchanged` (pixel compare
  against `render` with `on = false`).

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
- [ ] A Press toggle with an on action (e.g. `togglefloating` pair) alternates actions.
- [ ] Page away and back: faces show the last state immediately.
- [ ] Idle CPU of `omadeckd` with three command toggles on screen stays ~0%.
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

## Later

- Toggle faces on dial panels (a dial press that is a toggle showing on/off on the strip).
- Persisting Press toggles to `$XDG_STATE_HOME/omadeck/toggles`.
- Event-driven probes: inotify on Omarchy's flag files and `notifications.json`, D-Bus for
  Bluetooth and NetworkManager (Wi‑Fi radio, VPN presets).
- Multi-action steps that set a toggle's state (Elgato's `userDesiredState`, with PR 12).
- `snapshot` running each state command once to show real faces.

**Size:** about 800 lines (watcher ~250 with tests, config ~150, render ~40, daemon ~150,
GUI ~200, docs). If it passes ~900, split the GUI into a follow-up as PR 5 allowed.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- Press-flip toggles (no `state` command) are kept in memory only and reset to off on restart or replug. Persist them instead?
- Night light and idle presets rely on `--status` in the Omarchy toggle scripts, read from `master`; confirm 4.0.4 has it (hardware check), else use the plan's fallback.
- Do Not Disturb state reads `~/.local/state/omarchy/notifications.json` because `omarchy-shell notifications dndState`'s output is unconfirmed. Keep, or switch once confirmed?
- Wi‑Fi and VPN presets are left out (no Omarchy command). Add the `nmcli` VPN example as a preset, or keep it in docs only?
- About 800 lines: split the GUI into a follow-up PR if it passes ~900?
