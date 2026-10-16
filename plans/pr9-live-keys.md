# PR 9: `feat/live-keys`: keys and panels that update themselves

Part of [PLAN.md](PLAN.md). Roadmap item: "**Live keys** that show a clock, CPU usage or
the active workspace". Base branch: `feat/toggle-keys` (PR 8), because it reuses PR 8's
**state watcher** (`omadeckd/src/watch.rs`, `Probe`, `Value`, `WatchCmd`). This PR
introduces the **Hyprland event listener** (`omadeckd/src/hypr.rs`), which PR 10 (focused
app volume) and PR 15 (pages follow the focused app) reuse.

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Rewrite against the code as merged (upstream's named pages: `Option<&str>` / `Config::key(page, index)` / `Nav`), not `ba0eced`'s `page: u8`.
- [ ] Replace every "assumption about PR N" with that PR's real, merged signatures, copied in full.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.
- [ ] CPU/memory/battery as `Value::Level` carry a meaningless `muted`. Add `Value::Percent(u8)`, or say `muted: false`.
- [ ] Define the text-only `$A1` panel exactly. `render::Level` has `value: f32`; cleaner is `DialStatus { text, bar: Option<f32> }`.
- [ ] jiff: make step 1 a go/no-go. Run `cargo tree -i jiff -e features` after enabling `tz-system`; if it pulls in a new package, use the fallback.
- [ ] `hypr.rs`: give the `Pending` struct, the `Parsed` enum and the reconnect loop as code, plus a sample `.socket2.sock` transcript with expected events as a test fixture.
- [ ] Give the glyph codepoints as a fixed list to use as-is; check them in the font by hand.
- [ ] Mark which Open decisions block coding (only caption-vs-hide does).
- [ ] Split: (a) `live.rs` + `sysinfo.rs`, pure, with tests (a good first task), (b) `Live` config, (c) render, (d) watcher probes + daemon, (e) `hypr.rs` as its own PR (10 and 15 need it), (f) GUI.

**Outcome:** any key, and any dial's strip panel, can show a live value: the time or
date (strftime format), CPU %, memory %, battery % (with charging), the active Hyprland
workspace, or the first line of a command's output, polled at an interval. The value is
drawn the way Elgato's plugins set a key's title; on the strip it uses Elgato's `$A1`
(text) or `$B1` (text + bar) layouts. A live key still runs its action on press. Nothing
polls faster than it needs to: the clock wakes on the minute (or second) boundary, the
workspace is event-driven, and only the keys and panels whose value changed are repainted.
Old configs load and save byte-identical (no `live` field written when unset).

## How the Elgato app does it (researched 2026-10-16)

- **No built-in clock or system-monitor action in the app itself.** The app's own
  actions (System, Multimedia, Navigation, …) don't include them; both come as free
  plugins from the Elgato Marketplace. (Absence confirmed only from the docs and search,
  not from a current app install: unconfirmed.)
- **CPU** (`com.elgato.cpu`, author "Elgato", open source at
  <https://github.com/elgatosf/streamdeck-cpu>): one key action. Its manifest gives a
  static image plus `"TitleAlignment": "middle"`, `"FontSize": "17"`; the plugin's timer
  runs every **1000 ms** and calls `SetTitle(std::to_string(value) + "%")`
  (`Sources/MyStreamDeckPlugin.cpp`). So the live value is just the key's **title**, over
  the icon. Pressing opens Task Manager / Activity Monitor (from the Marketplace listing,
  v1.3.0 changelog: <https://marketplace.elgato.com/product/cpu-58f3a1f4-dd4d-43bb-9b79-ec95700568a4>).
- **Clocks** plugin (<https://www.elgato.com/us/en/explorer/products/stream-deck/clocks-plugin-for-stream-deck-track-multiple-time-zones-at-a-glance/>):
  30+ analog/digital styles, 12/24 h, show/hide date and weekday; a short press toggles
  the date; on the Stream Deck + a dial cycles time zones, press toggles the date, and it
  has a countdown-timer mode. Whether Elgato itself wrote it: unconfirmed.
- **Titles vs the user's title** (SDK keys guide,
  <https://docs.elgato.com/streamdeck/sdk/guides/keys>): `setTitle`/`setImage` only take
  effect "when the user has not specified a custom title"; user titles and images take
  precedence. The title is drawn at the top, middle or bottom of the key, over the image.
- **Strip** (SDK dials guide, <https://docs.elgato.com/streamdeck/sdk/guides/dials>):
  plugins show values with `setFeedback` on a built-in layout. `$A1` = title
  `[16,10,136,24]`, icon `[16,40,48,48]`, value text `[76,40,108,32]` 24 pt/600 right
  aligned, **no bar**; `$B1` = the same plus the bar `[76,74,108,12]` (which PR 4 draws).
- Workspace / window state: no Elgato equivalent (no Hyprland on Windows/macOS).

## Config

A new optional `live` table on `[[key]]` and `[[dial]]`, in `omadeck-core/src/config.rs`.
It says what the face *shows*, separate from the `action` it *does*.

```rust
/// A value a key or dial panel shows and keeps up to date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Live {
    /// Local time/date, strftime format (jiff's strtime: %H %M %S %a %d %b %n …).
    Clock {
        #[serde(default = "default_clock_format", skip_serializing_if = "is_default_clock_format")]
        format: String,               // "%H:%M"
    },
    Cpu,                              // average over all cores, %
    Memory,                           // (MemTotal − MemAvailable) / MemTotal, %
    Battery,                          // system battery %, plus charging
    Workspace,                        // Hyprland's active workspace name
    /// The first line of a command's stdout (`sh -c`), re-run every `interval` seconds.
    Command {
        command: String,
        #[serde(default = "default_live_interval", skip_serializing_if = "is_default_live_interval")]
        interval: u32,                // seconds, default 5, clamped to ≥ 1
    },
}
```

- `KeyConfig.live` and `DialConfig.live`: `Option<Live>`,
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. `KeyConfig::new`/
  `DialConfig::new` set `None`; both `is_blank()`s also require `live.is_none()`.
- `Live::KINDS: [&str; 7] = ["Nothing", "Clock", "CPU", "Memory", "Battery", "Active
  workspace", "Command output"]` with `kind_index()` / `of_kind(i)` for the GUI, in the
  style of `Action::KINDS` (index 0 = no live value).
- `DialConfig::level_source()` returns `None` when `live` is set: an explicit live value
  wins over the volume/brightness value a dial's actions would imply.

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

No new `Action` variant and no `KINDS` change: a live key's press runs its ordinary
`action` (or nothing).

## Assumptions about PR 8 (state watcher)

**Stale:** written before PR 8's plan. Replace this section with PR 8's merged API.
Known differences: PR 8 spawns asynchronously with a 5 s `COMMAND_TIMEOUT` (not
`min(interval, 10 s)` below; pick one policy per probe kind); `Value::Level` is
`audio::Level { percent, muted }`; `apply_value` and `KeyStatus` exist only if PR 8
adds them as its handoff list asks. Originally assumed:
- `Probe` and `Value` live in **core** (`omadeck-core/src/state.rs`), `Hash + Eq + Clone`,
  because `config.rs` derives probes and `render.rs` draws values; the worker loop is in
  `omadeckd/src/watch.rs`. If PR 8 kept them in the daemon, PR 9 moves them to core first.
- `Probe::Command { cmd, interval }` → `Value::Toggle(Toggle::{Off, On, Busy(text)})`
  (exit 0 / other / 75), and `Value::{Toggle, Level}`; PR 9 adds `Value::Text(String)`.
  PR 8 already spawns commands with piped stdout and a reader thread (first non-empty
  line, 4 KiB cap), so `CommandOutput` reuses that spawn path unchanged.
- The main loop has `watched_probes(cfg, page, n_dials) -> Vec<Probe>`, sent as
  `WatchCmd::Watch` on connect, reload and page change; `Deck.values: HashMap<Probe,
  Value>`; and one `apply_value(d, cfg, renderer, probe, value)` that inserts on change and
  repaints the current page's keys and the dials that use `probe`.
- Rendering takes a `KeyStatus { pressed: bool, state: Option<&Value>, live: Option<&Value> }`
  (PR 9 adds the `live` field; if PR 8 passed a bare value, PR 9 introduces the struct,
  mirroring `DialStatus`).
- If PR 8's worker runs commands synchronously, PR 9 changes it to **spawn and poll**
  (`Child::try_wait` each tick, killed after `min(interval, 10 s)`, no second run while one
  is going), so a slow command never holds up the clock or the other probes.

## State watcher additions (`state.rs`, `watch.rs`)

New variants:

| Probe | Value | How | When |
|---|---|---|---|
| `Clock { format }` | `Text` | `jiff::Zoned::now().strftime(format)` | next minute boundary, or second boundary if the format shows seconds; sleep capped at 5 s |
| `Cpu` | `Level` | `/proc/stat` first line, delta of two samples | every 1 s (Elgato's CPU plugin rate) |
| `Memory` | `Level` | `/proc/meminfo` | every 2 s |
| `Battery` | `Battery { percent: u8, charging: bool }` (new `Value` variant) | `/sys/class/power_supply/*` | every 5 s |
| `CommandOutput { cmd, interval }` | `Text` | `sh -c`, first non-empty stdout line, trimmed, max 64 chars | every `interval` s |
| `Workspace` | `Text` | **not polled**: fed by the Hyprland listener | on events |

- `Live::probe(&self) -> Probe` maps config to probe. `Probe::is_polled()` is false for
  `Workspace` (and PR 10/15's event-driven probes); `watched_probes` filters those out
  before sending `Watch`.
- `Value::display_text()`: `Text(s)` → `s`; `Level(p)`/`Battery { percent, .. }` → `"{p}%"`.
  `Value::to_level() -> Option<Level>` for the strip bar (`Level`/`Battery` only).
- **Clock timing** (`omadeck-core/src/live.rs`, pure): `needs_seconds(format)` (true for
  `%S %T %s %r %X %c %+`, not for `%%S`); `next_tick(now_unix_ms, seconds) -> Duration` =
  time to the next boundary + 10 ms (so the format runs just after the rollover), capped
  at 5 s. The cap matters: `thread::sleep` uses the monotonic clock, which stops during
  suspend, so an uncapped minute sleep would leave a stale time after resume. Waking at
  most every 5 s costs one strftime; nothing is sent unless the text changed.
- An invalid format gives `"?"` and one `log::warn!` per format.
- CPU: `busy = total − idle − iowait` (guest is already in user), `pct = Δbusy/Δtotal`;
  the first sample only primes, so the key shows `–` for one second.
- Battery: entries with `type == Battery` and no `scope == Device` (that's a mouse or
  headset); several batteries are combined as Σ`energy_now`/Σ`energy_full` (or
  `charge_*`), else the mean `capacity`. `charging` = `status == "Charging"`. No battery →
  no value (the key shows `–`).
- **Refresh after a press:** when a key with a `Command` live source runs its action,
  the loop sends `WatchCmd::Refresh(probe)` (as PR 8 does for toggles), so a key showing
  e.g. `playerctl status` updates right after its press.

New `omadeck-core/src/sysinfo.rs` holds the readers, each with a pure parser:
`parse_proc_stat(&str) -> Option<CpuTimes>`, `cpu_percent(prev, now) -> u8`,
`parse_meminfo(&str) -> Option<u8>`, `battery_in(root: &Path) -> Option<(u8, bool)>`
(`battery()` = `battery_in("/sys/class/power_supply")`).

**Dependencies:** no new crate. `jiff 0.2.37` is already in `Cargo.lock` (via
`env_logger`); core adds `jiff = { version = "0.2", default-features = false, features =
["std", "tz-system", "tzdb-zoneinfo"] }` for the local zone from `/etc/localtime` and
`/usr/share/zoneinfo`. Step 1 checks `Cargo.lock` gains no new package (if it did, fall
back to `libc::localtime_r` + `strftime`, `libc` is also already there). CPU, memory and
battery read `/proc` and `/sys` directly; no `sysinfo` crate.

## Hyprland event listener (`omadeckd/src/hypr.rs`): this PR owns it

```rust
pub enum HyprEvent {
    /// The focused workspace changed (or was renamed).
    Workspace { id: i64, name: String },
    /// The focused window changed or retitled; `None` when nothing has focus.
    ActiveWindow(Option<ActiveWindow>),
}
pub struct ActiveWindow { pub class: String, pub title: String, pub address: String, pub pid: Option<u32> }

pub fn start() -> Option<mpsc::Receiver<HyprEvent>>   // None outside Hyprland
```

- **Outside Hyprland:** if neither `HYPRLAND_INSTANCE_SIGNATURE` is set nor
  `$XDG_RUNTIME_DIR/hypr/` exists, `start()` logs once at debug and returns `None`; no
  thread. Workspace keys then show `–`.
- **Socket discovery** (`fn find_instance() -> Option<PathBuf>`, the instance dir):
  `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/` if its `.socket2.sock` exists, else
  the most recently modified `$XDG_RUNTIME_DIR/hypr/*/` that has one. The fallback covers a
  service started without the variable, and a Hyprland restart (new signature, stale env).
  The pre-0.40 `/tmp/hypr/` location is not supported (Omarchy ships current Hyprland).
- **Thread** `hypr`: connect to `.socket2.sock`, then **seed**, then read lines with a
  blocking `BufReader::lines()`. On EOF or error: back off 1 s, 2 s, 4 s … capped at 30 s
  (reset after a connection that lived > 10 s), re-run discovery, reconnect, seed again.
  The thread ends when the channel's receiver is gone.
- **Seed and queries** use Hyprland's request socket `.socket.sock` directly (what
  `hyprctl -j` sends): connect, write `j/activeworkspace` or `j/activewindow`, read to EOF,
  close at once (the wiki warns that an open request connection freezes Hyprland). No
  `hyprctl` process; `serde_json` (add to `omadeckd`'s deps, already a workspace dep)
  parses `{"id","name"}` and `{"address","class","title","pid"}`; an empty object or
  missing `address` means no focused window.
- **Lines parsed** (`fn parse(line, &mut Pending) -> Option<Parsed>`, pure; split at the
  first `>>`, data split with `splitn` so names and titles may contain commas):

| Line | Result |
|---|---|
| `workspacev2>>ID,NAME` | `Workspace { id, name }` |
| `focusedmon>>MON,NAME` | re-query `j/activeworkspace` → `Workspace` (the wiki: `workspace` is *not* emitted when focus moves to another monitor by mouse) |
| `renameworkspace>>ID,NAME` | re-query `j/activeworkspace` → `Workspace` (only sent if it is the active one; dedup below) |
| `activewindow>>CLASS,TITLE` | store class/title in `Pending` (Hyprland sends it just before v2) |
| `activewindowv2>>ADDRESS` | query `j/activewindow` for the pid, then `ActiveWindow(Some{…})`; empty address (or `,` above) → `ActiveWindow(None)` |
| `windowtitlev2>>ADDRESS,TITLE` | if `ADDRESS` is the focused one: `ActiveWindow(Some{… new title})` |
| `configreloaded>>` | re-seed both |
| `workspace>>`, `focusedmonv2>>`, `activespecial*`, everything else | ignored |

- The thread keeps the last sent workspace and window and **sends only changes**.
- **In the main loop** (`run()`): `hypr_state: HyprState { workspace: Option<(i64, String)>,
  window: Option<ActiveWindow> }` lives outside `Deck`, so it survives an unplug. Events
  are drained with `try_recv` at the top of every iteration, **before** the
  "no deck" branch (the loop wakes at least every `READ_TIMEOUT`, 100 ms). A `Workspace`
  event updates `hypr_state` and, with a deck, calls `apply_value(d, …, Probe::Workspace,
  Value::Text(name))`. `ActiveWindow` only updates `hypr_state` in this PR (PR 10 and 15
  hook in there). On connect, `Deck.values` is seeded with `Probe::Workspace` from
  `hypr_state`; reload and page change keep it (it is never polled, so never cleared).

## Render (`omadeck-core/src/render.rs`)

**Keys** (`render` with `KeyStatus.live`): the live text is drawn like Elgato's title.
- No icon or glyph: the value is the face, large and centred (`draw_block`, wrapping up to
  three lines). `draw_block` learns to honour `\n` (split on lines first, then wrap), so
  `%n` in a clock format puts the date under the time. If the key has a `label` and
  `show_label`, it is drawn as a small caption at the top (`s × 0.14` px, centred at
  `s × 0.16`) and the value block moves to the area below it (new `draw_block_in(img,
  area, …)`).
- Icon or glyph: the icon as today, and the value **in the label's place**
  (`render_face` with `label := value`); the label itself isn't drawn.
- Battery without its own icon/glyph: charging adds the default `f0084` (battery-charging)
  glyph as the icon, so "charging" is visible; not charging, the value stands alone.
- No value yet: `–`, as `$B1` does before its first level.
- Pressed: unchanged (the whole face, value included, shrinks in the accent ring).

**Strip panels** (`render_strip_panel`, `DialStatus.level` from `Value::to_level()`, or a
text-only `Level` for `Text` values): a dial with `live` draws `$B1` (icon, value, bar) for
`Cpu`/`Memory`/`Battery` and `$A1` (icon, value, no bar) for `Clock`/`Workspace`/`Command`.
`draw_b1` gets a `bar: bool` parameter; `$A1` is `draw_b1(.., bar = false)` with the same
rects. Default panel glyphs when the dial has no icon/glyph (codepoints to verify in the
font): clock `f0150`, CPU `f061a`, memory `f035b`, battery `f0079` / charging `f0084`,
workspace `f056e`, command `f018d`.

## Daemon (`omadeckd/src/main.rs`)

- `run()`: `let hypr = hypr::start();` next to `start_audio_worker()` / PR 8's watcher.
- `watched_probes`: adds `k.live.probe()` for keys on the current page and `dc.live` for
  dials, keeping only `is_polled()` probes.
- `dial_status`: a dial with `live` takes its level from `d.values`.
- `write_key` builds `KeyStatus { live: k.live.as_ref().and_then(|l| d.values.get(&l.probe())), .. }`.
- Key press path: unchanged except the `WatchCmd::Refresh` for `Command` live keys.
- Reload: values of probes still watched are kept (PR 6's no-blanking rule), the rest
  dropped; `Workspace` always kept. Disconnect: `Watch(vec![])` as today; `hypr` keeps
  running. Snapshot: clock keys show the current time; other live values show `–`.
- No new IPC events (the configurator doesn't need live values); a `WorkspaceChanged`
  event can come later if a client wants it.

## GUI (`omadeck-gui`)

- `editor.rs`: a **Live value** group in `build_key` (and "Panel value" in `build_dial`):
  an `adw::ComboRow` from `Live::KINDS`, then per source: Clock → `EntryRow` "Format"
  (subtitle "strftime, e.g. %H:%M or %a %d %b", a live example under it); Command →
  `EntryRow` "Command" + `SpinRow` "Every (seconds)" 1–3600. Changing the source calls
  `rebuild_editor()`. A hint row: "The label shows as a caption; with an icon, the value
  takes the label's place."
- Preview (`refresh_key`, `refresh_strip`): clock keys render the current time (core's
  `format_clock`), memory and battery read once from `sysinfo`, CPU/workspace/command show
  `–` (the GUI doesn't poll or run commands). *Try it now* still runs the key's action.
- **Starter layout: unchanged.** It already fills all 15 keys, and keys 5–9 switch
  workspaces 1–5; a separate "active workspace" key would duplicate them. The better
  follow-up is highlighting the active workspace's key (Later).

## Docs

README: a "Live keys" feature bullet (sources, the strftime link, `command` example);
roadmap `- [x] **Live keys** …`. `examples/config.toml`: the clock, CPU, workspace and
command keys above and a battery dial. CHANGELOG under Unreleased/Added: "Live keys: a key
or a dial's strip panel can show the time, CPU, memory, battery, the active Hyprland
workspace or a command's output, kept up to date by the daemon."

## Implementation steps

1. [ ] Core: `jiff` dep (check `Cargo.lock` gains no package); `live.rs` (`format_clock`,
   `needs_seconds`, `next_tick`) with tests.
2. [ ] Core: `sysinfo.rs` readers and parsers with tests.
3. [ ] Core: `Live` in `config.rs`, `live` on `KeyConfig`/`DialConfig`, `is_blank`,
   `level_source` rule, `Live::KINDS`; round-trip and byte-identical tests.
4. [ ] Core: new `Probe`/`Value` variants, `Live::probe`, `is_polled`, `display_text`,
   `to_level`.
5. [ ] Render: `KeyStatus.live`, caption + value layouts, `\n` in `draw_block`,
   `draw_b1(bar)` / `$A1`, default glyphs; render tests.
6. [ ] Watcher: the new polled probes, clock boundary timing, spawn-and-poll commands.
7. [ ] Daemon: `watched_probes`, `dial_status`, `write_key`, `Refresh` after a press,
   reload retention, snapshot. Commit; try clock/CPU/command on hardware.
8. [ ] `hypr.rs`: discovery, parser, request-socket queries, reconnect/backoff, tests;
   wire into `run()`, `HyprState`, `Probe::Workspace`. Commit.
9. [ ] GUI: Live value / Panel value groups, preview values.
10. [ ] Docs, CHANGELOG; `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.

## Tests

- config: `live_key_round_trips` (each source), `clock_default_format_not_written`
  (`source = "clock"` only), `command_interval_default_not_written`,
  `old_config_saves_byte_identical`, `key_with_only_live_is_not_blank`,
  `dial_live_wins_over_level_source`.
- live: `needs_seconds_formats` (`%H:%M` no; `%T`, `%S`, `%r` yes; `%%S` no),
  `next_tick_minute_boundary` (12:00:59.500 → 510 ms), `next_tick_second_boundary`,
  `next_tick_capped_at_5s`, `format_clock_fixed_time` (a fixed UTC `Zoned`),
  `format_clock_bad_format_is_question_mark`.
- sysinfo: `cpu_percent_from_two_samples`, `cpu_percent_no_delta_is_zero`,
  `meminfo_used_percent`, `meminfo_missing_field_is_none`, `battery_charging_status`
  (Charging / Full / Discharging / Not charging), `battery_skips_device_scope` and
  `battery_combines_two` (fake sysfs in a temp dir).
- hypr: `parse_workspacev2`, `parse_workspace_name_with_comma` (`5,my,ws` → name `my,ws`),
  `parse_activewindow_title_with_commas`, `activewindow_pairs_with_v2` (one event with
  class, title and address), `empty_activewindow_is_none`, `windowtitlev2_only_for_focused`,
  `unknown_and_special_events_ignored`, `seed_activeworkspace_json`,
  `seed_activewindow_empty_object`, `find_instance_prefers_env`,
  `find_instance_falls_back_to_newest` (temp `XDG_RUNTIME_DIR`).
- render: `live_key_value_centred_without_icon`, `live_key_label_is_caption`,
  `live_key_with_glyph_puts_value_in_label_slot`, `draw_block_honours_newline`,
  `dial_live_text_is_a1_without_bar` (bar rect stays background),
  `dial_live_level_is_b1_with_bar`, `live_key_without_value_shows_dash`.
- daemon: `watched_probes_skips_workspace`, `watched_probes_includes_dial_live`,
  `watched_probes_only_current_page`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] A `%H:%M` clock key turns over within ~0.1 s of the minute (compare with Waybar);
      a `%H:%M:%S` key ticks every second without skipping.
- [ ] After suspend and resume the clock is right within 5 s.
- [ ] `timedatectl set-timezone …` moves the clock (jiff's zone cache: unconfirmed how
      quickly).
- [ ] CPU key follows btop roughly (load with `stress` or a build); memory and battery
      match `free` and Waybar; plugging the charger shows the bolt within 5 s.
- [ ] Workspace key follows `SUPER+1..5`, a click on another monitor's window, and a
      rename; `–` when the daemon runs outside Hyprland (TTY).
- [ ] The systemd user service sees `HYPRLAND_INSTANCE_SIGNATURE` under uwsm; if not, the
      directory fallback finds the socket.
- [ ] Restarting Hyprland (`hyprctl dispatch exit` and log in again) reconnects the
      listener, with the new signature.
- [ ] A command key with `sleep 30` doesn't delay the clock; the run is killed at 10 s.
- [ ] Pressing a live key runs its action; the pressed ring shows over the value.
- [ ] Battery and clock dials draw `$B1` and `$A1` correctly on the strip.
- [ ] Changing page and back: the clock and workspace keys show their values at once.
- [ ] No visible CPU cost from the daemon with a seconds clock (`top`).

## Decisions to explain

- **`live` is a key field, not an `Action`.** Elgato ties the display to the plugin's
  action (the CPU key also opens Activity Monitor). In omadeck what a key shows and what it
  does are separate, so a clock can launch a calendar and a workspace key can run any
  dispatcher, and dials get the same field.
- **The value wins over the label.** Elgato draws a user's custom title *instead of* the
  plugin's title; that would hide the value, so omadeck keeps the label as a caption
  instead.
- **Text on keys, no bar.** Elgato's CPU key is just `N%` as the title over an image; bars
  are a strip thing (`$B1`). A key bar is under Later.
- **`$A1` for text, `$B1` for levels** on the strip, Elgato's built-in layouts. This also
  delivers the `$A1` layout listed under PLAN.md's "Later".
- **CPU every 1 s** like Elgato's plugin; memory 2 s, battery 5 s are our own choices.
- **Clock wakes on the boundary, capped at 5 s**, because monotonic sleeps stop during
  suspend; cheaper than a fixed poll and correct after resume.
- **jiff** for time: already in the build through `env_logger`, so no new crate; it reads
  the system zone itself. No `sysinfo` crate: three small `/proc`/`/sys` parsers.
- **The command source** has no Elgato built-in counterpart (Marketplace plugins do it);
  first line of stdout, any exit status, `–` when empty, killed after `min(interval, 10 s)`.
- **Hyprland via sockets, not `hyprctl`.** Events from `.socket2.sock`; seeding and the
  focused window's pid from `.socket.sock` with the same `j/…` requests `hyprctl -j` sends,
  so no process per focus change. The active workspace also follows `focusedmon`, since
  `workspace>>` isn't sent when focus moves to another monitor by mouse.
- **The listener always runs inside Hyprland**, even with no workspace key, because PRs 10
  and 15 need the focused window; it's one idle blocking read.
- **Starter layout unchanged** (all keys used, workspace keys already there).

## Later

- Highlight the active workspace's key in the starter (PR 8 states fed by
  `Probe::Workspace`), and special workspaces (`activespecial`).
- A `window` source (focused window's title/class) — the event is already there.
- Analog clock faces and time zones; dial turn to cycle zones (Elgato's Clocks plugin).
- A level bar on keys, and per-source format templates (`"CPU {}"`).
- Temperature, network rate, GPU, per-core CPU.
- A `WorkspaceChanged` hub event if a client wants it.

**Size:** about 1,200 lines including tests (core ~450, `hypr.rs` ~300, watcher and
daemon ~150, GUI ~180, docs ~60).

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- When a live key has a label: label becomes a small caption above the value (planned), or the label hides the value as in Elgato?
- Poll intervals: CPU 1 s (matches Elgato), memory 2 s, battery 5 s. OK?
- Does the systemd user service see `HYPRLAND_INSTANCE_SIGNATURE` under uwsm? If not, the newest-folder fallback is used (hardware check).
- Default Nerd Font codepoints for the strip icons still to confirm.
- About 1,200 lines including tests: split the GUI or the Hyprland listener into its own PR?
