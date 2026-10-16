# PR 10: `feat/app-volume`: per-app volume, for a chosen app or the focused one

Part of [PLAN.md](PLAN.md). Base: `feat/live-keys` (PR 9). Uses groundwork's **state
watcher** (core `probe.rs`, daemon `watch.rs`) for stream levels and PR 9's **Hyprland
event listener** (`omadeckd/src/hypr.rs`) for focus. Owns the **audio streams** building
block (`audio.rs`; no later plan uses it), and adds `Backdrop::tone_in` (D9), which PR 14
uses.

**Rewritten 2026-10-17** against the cross-plan decisions (D1–D15), the code as merged on
`fix/bugs` (`d7bd448`), and the names groundwork, PR 8 and PR 9 deliver. Before Task A,
check the names in "What this PR builds on" against the tree (`grep -n` each); fix this
plan first if any differ.

**Outcome:** a new `app_volume` action (kind 11, D14) changes or mutes the volume of one
app's audio streams. **Chosen app** controls one app you pick (e.g. Spotify). **Focused
app** controls whichever app has the focused Hyprland window, and keeps controlling the
last focused app that had audio when you move to a silent window. On a dial, the strip
panel uses `$B1` with the app's icon, its name as the title, the level and a bar. When
the app has no stream, the panel is dimmed with `–`. Turn ±5%, capped at 100%. Press
mutes the app. Keys can run the action too. The configurator mirrors the panel from
daemon events (D2).

## How the Elgato app does it (researched 2026-10-16)

- **Elgato's "Volume Controller"** (`com.elgato.volume-controller`, v2.0.4.196, Windows and
  Mac, dials supported) has **Profile**, **App** ("Control volume for a specific app":
  set, adjust or mute), **Input** and **Output** actions.
  [Marketplace](https://marketplace.elgato.com/product/volume-controller-34d9aa59-a41a-4a4c-a853-202ca91409e1),
  [Help](https://help.elgato.com/hc/en-us/articles/10568396413965-Elgato-Stream-Deck-Volume-Controller-Plugin)
  (403 to the fetcher; from its search snippet).
- **On dials:** "Select an application and rotate the dials to adjust the volume. Press
  the dial or LCD screen to toggle mute." An "Auto Detection" mode seems to put running
  apps on the dials by itself (**unconfirmed**).
- **Strip contents:** app icon, name and level in an indicator layout (**unconfirmed**;
  assumed from `$B1` and the plugin below).
- **Focused app:** not in Elgato's plugin. The third-party **Focused Application Volume**
  (dlprows, Windows, [GitHub](https://github.com/dlprows/FocusVolumeControl)) does it:
  "The screen updates to show the name/icon of the app so that you can always know what
  you're about to change". What it does when the focused app is silent: **unconfirmed**.

**What Omarchy offers** (`omacom/omarchy`, branch `quattro`, 2026-10-16): no per-app
volume command (the `omarchy-audio-*` scripts are per device). The Quickshell audio panel
labels a stream with `application.name`, else `media.name`, else `node.name`, and treats
`audio-src` as generic. Omarchy 4 drops `pavucontrol`, `wiremix` and `playerctl`; `pactl`
(pipewire-pulse) and `wpctl` stay. `omarchy-osd` shows a message **or** an icon with a
bar, never both, so there is no "Spotify 40%" pop-up (Decided below).

## What this PR builds on

Check each before Task A:

- Groundwork (D5): core `probe.rs` with `Probe`, `Value` (derives `Serialize,
  Deserialize`), `Values { probes, brightness }`, `dial_status(dial: Option<&DialConfig>,
  values: &Values, pressed: bool) -> DialStatus`, `shows(dial, probe)` and
  `watched_probes(cfg, page, n_dials: usize)`; `proc::output_timeout(cmd: &mut Command,
  timeout: Duration) -> Option<Output>` and `COMMAND_TIMEOUT = 2 s`; daemon `watch.rs`
  with `WatchCmd::{Watch, Refresh, Volume { device, change: i32, mute }, Stop}` (plus PR
  8's `Leave`, `Done`), run in order on the watcher thread then re-read, and the
  `Poller` (`handle`, `run_due`, `record`, the `read_volume`/`change_volume` fn fields).
- Groundwork (D3): `daemon.rs` with `Daemon`, `Runtime { values, state }` (plus PR 8's
  `flipped`, `settle`, `jobs` and PR 9's `window`), `Source`,
  `Daemon::run_input(src, act, steps) -> bool` and `dispatch`, which routes
  `Action::Volume` to `WatchCmd::Volume`; `Daemon::apply_value` and `Daemon::flush`,
  which publishes each panel's `PanelStatus` that changed. `Action::default_for(kind,
  dir)` (D10), `ActionSlot` (with `target: ActionTarget`) and `action_editor(app, group,
  combo_title, &slot)` in the GUI. `Action::is_deck_owned()` (D11).
- Groundwork (D2): `render::DialStatus { pressed, panel: PanelStatus }`,
  `PanelStatus { level }`, `Event::DialStatus { index, status: PanelStatus }`, replayed
  to new clients; the GUI keeps the latest `PanelStatus` per panel and draws the strip
  from it.
- Groundwork (D9): `draw_b1` takes the title colour and the value/bar colours separately
  (a press recolours the title only, `B1Colors`); groundwork extracts the bar helper
  `draw_bar(img, rect, value, fill, track)` from `draw_b1`, and it already clamps
  `value`. `Renderer::with_font_bytes(..)` for tests, called through fix/review's
  `renderer_with_font()` helper in `render.rs`'s tests.
- PR 9: `PanelStatus.live: Option<LiveShow>`, `Runtime.window:
  Option<hypr::ActiveWindow>`, `Daemon::apply_hypr` with its `HyprEvent::ActiveWindow(w)`
  arm, and:

```rust
// crates/omadeckd/src/hypr.rs (PR 9)
pub struct ActiveWindow { pub address: String, pub class: String, pub title: String, pub pid: Option<u32> }
pub enum HyprEvent { Workspace { id: i64, name: String }, ActiveWindow(Option<ActiveWindow>) }
pub fn start() -> Option<mpsc::Receiver<HyprEvent>>
```

## Config (`omadeck-core/src/config.rs`)

```rust
/// Change or mute one app's audio streams (PipeWire sink inputs).
AppVolume {
    /// The app: a process name (`spotify`), an app name (`Firefox`), a Flatpak id
    /// (`com.spotify.Client`) or a desktop id (`spotify.desktop`). Ignored when `focused`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    app: String,
    /// Follow the focused window instead of a chosen app.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    focused: bool,
    /// Percent to change by, signed (per detent on a dial). Ignored when `mute` is set.
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    change: i8,
    /// Toggle the app's mute instead.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    mute: bool,
},
```

```toml
[[dial]]
index = 1
label = "Spotify"            # optional; defaults to the app's name
turn_left = { type = "app_volume", app = "spotify", change = -5 }
turn_right = { type = "app_volume", app = "spotify", change = 5 }
press = { type = "app_volume", app = "spotify", mute = true }

[[dial]]
index = 2                    # follows the focused window
turn_left = { type = "app_volume", focused = true, change = -5 }
turn_right = { type = "app_volume", focused = true, change = 5 }
press = { type = "app_volume", focused = true, mute = true }
```

- `KINDS` grows to `[&'static str; 12]` with `"App volume"` at **11** (D14):

```rust
pub const KINDS: [&'static str; 12] = [
    "Nothing", "Launch app", "Run script", "Play sound", "Open website", "Hyprland",
    "Open page", "Go back", "Volume", "Next/previous page", "Brightness",
    "App volume", // 11 AppVolume (PR 10)
];
```

- `kind_index`: `Action::AppVolume { .. } => 11`. `empty_of_kind(11)` →
  `AppVolume { app: "", focused: true, change: 5, mute: false }` (focused works without
  choosing anything). `default_for(11, dir)`: turn left `change: -5`, turn right `5`,
  press `mute: true`, all `focused: true`.
- `summary()`: `Spotify volume +5%`, `Toggle mute (Spotify)`, `Focused app volume −5%`,
  `Toggle mute (focused app)`, and `App volume (no app set)` when `app` is empty and
  `focused` is off. The app part is `audio::display_app(app)` (title-cased, below).
- `is_deck_owned()`: unchanged (`AppVolume` can run without the daemon).
- `Action::app_query(&self) -> Option<AppQuery>`: `Focused` when `focused`,
  `Named(app.trim())` when `app` isn't blank, else `None`. Other actions → `None`.
- `LevelSource` gains a unit `App`, so it stays `Copy`. `level_source()` (after PR 9's
  `live` check): the first `Volume` **or** `AppVolume` among turn_right, turn_left, press
  decides (`Volume(device)` or `App`), then `Brightness`.
- `DialConfig::app_query(&self) -> Option<AppQuery>`: the first `app_query()` among
  turn_right, turn_left, press.
- Every `match` this breaks: run `cargo build --workspace` after adding the variant and fix
  each error (expected: `kind_index`, `summary`, `action::run`, `DialConfig::volume_device`,
  `render_strip_panel`, `probe::dial_status`, the GUI's `action_rows`). Also
  `grep -rn 'LevelSource::' crates` for `matches!`/`==` uses the compiler doesn't flag
  (the daemon's repaint checks).

Old configs: no new fields on existing structs, so they load and save byte-identical
(Task C's test).

**Why a new action, not `Volume { target = "app" }`:** `Volume` is defined by its device
backends (Omarchy scripts, swayosd, the physical-sink lookup); app volume uses none of
them and needs `app` and `focused`, which would make meaningless combinations such as
`device = "input"` with `app = …`. Elgato splits them the same way (App vs Input/Output).

## Audio streams (`omadeck-core/src/audio.rs`): the shared block

**Read** with `pactl -f json list sink-inputs` (compact, what Omarchy's scripts use).
**Write** with `wpctl` on the PipeWire node id, so `-l 1.0` caps at 100% as in PR 4
(`pactl set-sink-input-volume` has no cap). No `pw-dump` fallback (pipewire-pulse is part
of Omarchy; Later).

```rust
/// One playback stream (a sink input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    /// PipeWire node id (`properties."object.id"`), what wpctl takes.
    pub id: u32,
    /// `application.process.id`.
    pub pid: Option<u32>,
    /// `application.process.binary`, lowercased ("" if absent).
    pub binary: String,
    /// `application.name`, else `media.name`, else `node.name`.
    pub name: String,
    /// `pipewire.access.portal.app_id`, else `application.id` (Flatpak), lowercased.
    pub app_id: Option<String>,
    /// `application.icon_name`.
    pub icon_name: Option<String>,
    /// The loudest channel's percent, plus mute.
    pub level: Level,
    /// Paused but present: still controllable.
    pub corked: bool,
}

/// What an app volume action or panel targets.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppQuery { Named(String), Focused }

/// One app's streams added up, as the watcher reports them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLevel {
    /// Display name. For a focused dial with no audio: the window's class ("" if none).
    pub name: String,
    /// The key for sticky focus ("" when nothing matched).
    pub binary: String,
    pub icon: Option<PathBuf>,
    /// `percent` = max over streams, `muted` = all muted. `None`: no stream now.
    pub level: Option<Level>,
    pub ids: Vec<u32>,
}

/// The focused window, as matching needs it.
pub struct Window<'a> { pub pid: Option<u32>, pub class: &'a str }

pub fn streams() -> Option<Vec<Stream>>;                         // None: pactl missing or failed
pub fn parse_pactl_streams(json: &str) -> Option<Vec<Stream>>;
pub fn select_named<'a>(s: &'a [Stream], name: &str) -> Vec<&'a Stream>;
pub fn select_window<'a>(s: &'a [Stream], w: &Window, parent: impl Fn(u32) -> Option<u32>) -> Vec<&'a Stream>;
pub fn app_level(matched: &[&Stream]) -> Option<AppLevel>;      // None for no match; icon left None
/// What a focused dial controls: the window's app if it has a stream, else the sticky
/// app, else nothing (a `level: None` AppLevel named after the class). Updates `sticky`.
pub fn resolve_focused(s: &[Stream], w: Option<&Window>, sticky: &mut Option<String>,
                       parent: impl Fn(u32) -> Option<u32>) -> AppLevel;
pub fn resolve_named(s: &[Stream], name: &str) -> AppLevel;      // no match: level None, name display_app(name)
pub fn normalise_app(q: &str) -> String;                         // trim, lowercase, last `/` segment, strip ".desktop"
pub fn display_app(q: &str) -> String;                           // known names map, else title-case normalise_app
pub fn proc_parent(pid: u32) -> Option<u32>;                     // /proc/<pid>/stat, the 2nd field after ")"
pub fn app_icon(names: &[&str]) -> Option<PathBuf>;              // cached lookup, see below
pub fn app_volume_commands(ids: &[u32], change: i32, mute_to: Option<bool>) -> Vec<Vec<String>>;
```

- **`streams()`** runs `pactl -f json list sink-inputs` through `proc::output_timeout(..,
  COMMAND_TIMEOUT)` with `LC_ALL=C`, then `parse_pactl_streams`.
- **`parse_pactl_streams`**: `serde_json::Value`; not an array → `None`; `[]` →
  `Some(vec![])`. Per entry: the `properties` values are strings (`"object.id": "87"`,
  pids too: capture to confirm), parsed with `.as_str()?.parse()`. A stream without a
  parseable `object.id` is **skipped** with `log::warn!` once (it can't be written with
  wpctl). Percent: the max over `volume.*.value_percent` (`"86%"`), clamped to 255.
  `mute` and `corked` are JSON booleans. A missing optional key never fails the stream.
- **`select_named`**: `n = normalise_app(name)`; a stream matches when `n` equals its
  `binary`, its lowercased `name` or its `app_id`.
- **`select_window`**: (1) **pid first**: a stream matches when its `pid == w.pid`, or
  `w.pid` is among its ancestors (walk `parent` up from the stream's pid, stop at 1, at a
  repeat, or after 32 hops). That covers Chromium and Electron (audio in a child) and a
  terminal running `mpv`. (2) **class second**, only if no pid matched: lowercased
  `w.class` equals the stream's `app_id`, `binary` or lowercased `name` (Flatpak apps,
  whose pids live in a sandbox namespace).
- **`app_level`**: several streams per app (a tab each) count as one: percent = max,
  muted = all muted, every id included. `name` = the first stream's name unless it's
  generic (`audio-src`, starts with `ALSA plug-in`) or empty, else `display_app(binary)`.
- **Sticky** (`resolve_focused`): a window match sets `*sticky = Some(binary)`; with no
  window match and `sticky` set, `resolve_named(s, sticky)` if it has a stream; else
  `AppLevel { name: class, binary: "", icon: None, level: None, ids: [] }`. The sticky
  app is never cleared by a silent window, only replaced by the next window that has
  audio.
- **`app_icon`** (no GTK in the daemon): for each name, in order, look for
  `<name>.svg`/`.png` in `$XDG_DATA_HOME` and each `$XDG_DATA_DIRS` entry (default
  `/usr/local/share:/usr/share`), under `icons/hicolor/{scalable,256x256,128x128,64x64,48x48}/apps`
  and `pixmaps`, plus `~/.local/share/flatpak/exports/share` and
  `/var/lib/flatpak/exports/share`. If none hits, read `Icon=` from
  `applications/<name>.desktop` in the same roots and look that up. Results, misses
  included, go in a `OnceLock<Mutex<HashMap<String, Option<PathBuf>>>>`. It runs on the
  watcher thread only. Write it as `app_icon_in(roots: &[PathBuf], names)` plus the
  wrapper, so the test can pass a temp dir.
- **`app_volume_commands`**: change → one `wpctl set-volume -l 1.0 <id> 5%+` (or `5%-`,
  `change.unsigned_abs()`, clamped to 100) per id, run in order, relative so tabs keep
  their balance. Mute → `wpctl set-mute <id> 1` (or `0`) for every id: one target state.
  The caller picks it: if any stream is unmuted, mute all, else unmute all (a `toggle`
  would flip a mixed set into another mixed set).

## Probes (`omadeck-core/src/probe.rs`) and the panel

```rust
pub enum Probe { /* … */ AppVolume(AppQuery) }
pub enum Value { /* … */ App(AppLevel) }
```

`render.rs` gains what the panel draws, next to `LiveShow`:

```rust
/// The app on an app volume panel (PR 10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppShow {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dimmed: bool,
}
```

`PanelStatus` (not `DialStatus`; groundwork's rule) gains
`#[serde(default, skip_serializing_if = "Option::is_none")] pub app: Option<AppShow>`, so
`Event::DialStatus { index, status }` carries it with no event change. `Value` derives
`Serialize, Deserialize` (groundwork), so `AppLevel` and `audio::Level` (today `Debug,
Clone, Copy, PartialEq, Eq`) add `serde::Serialize, serde::Deserialize` to their derives.
One pure helper decides what
the panel shows, so the daemon, the GUI (through the event) and `snapshot` agree:

```rust
/// The face and level of an app volume panel. `lv` is the watcher's value, `None`
/// before the first read.
pub fn app_show(dial: &DialConfig, q: &AppQuery, lv: Option<&AppLevel>) -> (AppShow, Option<render::Level>);
```

| Case | Title | Icon | Level | Dimmed |
|---|---|---|---|---|
| Named, playing | label, else `lv.name` | `lv.icon` | `lv.level.to_panel()` | no |
| Named, no stream (`lv.level` None) | label, else `lv.name` | `lv.icon` | None (`–`) | yes |
| Named, no value yet | label, else `display_app(app)` | None | None | yes |
| Focused, playing (focused or sticky) | `lv.name` (label ignored) | `lv.icon` | `lv.level.to_panel()` | no |
| Focused, no audio | `lv.name` if not empty, else label, else `Focused app` | None | None | yes |
| Focused, no value yet | label, else `Focused app` | None | None | yes |

`dial_status`'s `LevelSource::App` arm: `q = dial.app_query()`, `lv = match
values.probes.get(&Probe::AppVolume(q))` with `Value::App(lv)`, then `app_show` fills
`app` and `level`.

## Render (`omadeck-core/src/render.rs`)

- **`Backdrop::tone_in`** (D9):

```rust
impl Backdrop {
    /// The mean colour under `rect` (panel units), for text drawn there. `tone` stays the
    /// whole-panel mean (the snapshot knob uses it).
    pub fn tone_in(&self, rect: (u32, u32, u32, u32)) -> Rgb {
        let (x, y, w, h) = rect;
        mean_rgb(&imageops::crop_imm(&self.img, x, y, w, h).to_image())
    }
}
```

  `render_strip_panel` computes `title_tone = backdrop.tone_in(X1_TITLE)` and
  `value_tone = backdrop.tone_in(B1_VALUE)` **before** `backdrop.img` is moved into
  `img`, and every `$B1` panel (volume, brightness, live, app) takes its title colour
  from `label_color(.., title_tone)` and its value colour from `label_color(..,
  value_tone)`. On a plain colour both equal `tone`, so the goldens don't change.
- **Clamp** (D9): groundwork's `draw_bar` already clamps `value`. Sink inputs go above
  100%; the text still reads `150%`, the bar stops at full (Task D's test pins it).
- **The `LevelSource::App` arm** draws `$B1` through `draw_b1` with a copy of the face
  whose `label` is `st.app.title` (still subject to `show_label`). Icon order: the user's
  `icon`, then `app.icon` (`self.icon(path, STRIP_ICON_PX)`), then the user's `glyph`,
  then `APP_GLYPH = "f08c6"` (nf-md application; Capture list). The app keeps its own icon
  when muted: which app it is matters more, and the text says `Muted`.
- **Dimmed:** title and value colours become `c.mix(value_tone, 0.5)`, the icon's alpha
  is halved (`fn fade(img: &mut RgbaImage, f: f32)`), the value is `–` and the bar empty.

## Watcher (`omadeckd/src/watch.rs`)

- **One read serves every app probe.** All `Probe::AppVolume` probes share one 1 s
  schedule. When any is due, the watcher calls `audio::streams()` once, then answers
  **every** watched `AppVolume` probe from that list (`resolve_named` / `resolve_focused`
  with `audio::proc_parent`), fills `icon` with `audio::app_icon(&[icon_name, binary,
  name, app_id])` for a match, and sends only the changed ones. `streams()` returning
  `None` sends nothing.
- New `Poller` fields: `focus: Option<hypr::ActiveWindow>`, `sticky: Option<String>`.
- New commands:

```rust
/// The focused window changed (from PR 9's listener). Re-read now if a focused probe is
/// watched; otherwise just remember it.
WatchCmd::Focus(Option<hypr::ActiveWindow>),
/// Change or mute an app's streams, in order with every other change, then re-read.
/// `change` already has the steps folded in (± 100 at most).
WatchCmd::AppVolume { query: AppQuery, change: i32, mute: bool },
```

- `AppVolume` handling: read `streams()` fresh (the ids may have changed since the last
  read; a key's query may not be watched at all), resolve the query (`Focused` uses
  `focus` and `sticky`), and with no match log at info "no app with audio for …" and stop.
  Else build `app_volume_commands(&lv.ids, change, mute_to)` with `mute_to = mute.then(||
  !all_muted)`, run each through `proc::output_timeout(.., COMMAND_TIMEOUT)` in order, then
  re-read and send the watched app probes (sent even if unchanged, as `Refresh` does).
- **Optimistic bar: none.** The change and the read run back to back on this thread (D5),
  so the new level arrives in tens of ms; there's no stale read to fight.

## Daemon (`omadeckd/src/daemon.rs`)

- `dispatch`: `Action::AppVolume { change, mute, .. }` → `act.app_query()`; `None` →
  `Err("no app set")` (the failure event, as for other actions). Else
  `watcher.cmd.send(WatchCmd::AppVolume { query, change: (change as i32 * steps as
  i32).clamp(-100, 100), mute })`. Next to the `Volume` arm, so every input path (keys,
  turns, presses, taps, later ctl and multi) gets it through `run_input`.
- PR 9's `HyprEvent::ActiveWindow(w)` arm in `apply_hypr`: after `rt.window = w.clone()`, send
  `WatchCmd::Focus(w)`. Always: the watcher ignores it cheaply when nothing follows focus.
- `probe::watched_probes` (core): each dial whose `level_source()` is `App` adds
  `Probe::AppVolume(dial.app_query())`.
- Repaint and events: `probe::shows` gains the `App` case (a dial whose `app_query()`
  matches the probe's query), so `apply_value` marks its panel dirty. `flush` already
  publishes each `PanelStatus` that differs from the last one sent; with `app` inside
  `PanelStatus`, nothing else changes.
- Reload: values of still-watched probes kept. The sticky app lives in the watcher, so it
  survives reloads and replugs.
- `snapshot`: `Values::default()`, so named dials show the dimmed "no value yet" face and
  focused dials `Focused app`.
- `omadeckd press` with no daemon runs `action::run`, below.

## Outside the daemon (`omadeck-core/src/action.rs`)

`action::run(AppVolume)` (and `run_steps`, folding `change × steps` clamped to ±100): one
`thread::spawn` that reads `audio::streams()`, resolves the query, and runs
`app_volume_commands` in order, each through `proc::output_timeout`. Focused mode has no
sticky app here: `audio::focused_window()` reads `hyprctl -j activewindow` (through
`output_timeout`) for `pid` and `class`, then `select_window`. No query → `bail!("no app
set")`; no match → log and `Ok(())` (the panel already shows `–`). `spawn_seq` isn't used:
it takes commands already built, and these need a read first.

## GUI (`omadeck-gui`)

- `action_rows` gets an `Action::AppVolume` arm. Group description: "Changes one app's
  volume. On a dial, the strip shows the app's icon, name and level."
  - **Follow the focused window** `SwitchRow`: on hides the app rows (`rebuild_editor`).
  - **Application** `ActionRow`: subtitle the chosen name or "None chosen". **Choose…**
    opens `apps::pick_audio`: a **Playing now** list from one `audio::streams()` read run
    with `gio::spawn_blocking` when the dialog opens (one read, never repeated, never on
    the GTK thread), deduped by binary, with the `gio::AppInfo` icon when
    `apps::find("<binary>.desktop")` hits; and **Installed apps…**, the existing
    `apps::pick`, stored as `info.executable()`'s basename. Either way `app` holds a
    binary, and an empty dial label is filled through the slot's `on_app_picked`.
  - **App** `EntryRow` for free text (no `rebuild_editor` in its `connect_changed`).
  - **Toggle mute** switch, and "Change per turn (%)" `SpinRow` −25…25 when mute is off,
    as in the Volume arm.
- Dial presets, next to the existing ones: **App volume dial** (opens the picker, then
  fills turn left/right ±5 and press mute) and **Focused app dial** (all three with
  `focused = true`, label cleared). If the header gets too wide, move the presets into a
  `gtk::MenuButton` "Presets".
- *Try it now*: works for chosen-app actions (`action::run`); insensitive for focused
  mode, tooltip "Follows the focused window — the configurator would be the focused app".
- Strip preview: from the mirrored `PanelStatus` (groundwork's `App.panel_status`, now
  with `app`); no daemon → no status, so
  the panel draws its "no value yet" face. The GUI never polls streams.
- Traps: BUGS.md 4 (clone the action, drop the `config` borrow, then `edit`); the picker's
  callback edits through `App::set_action_at(&slot.target, ..)` (the slot's
  `ActionTarget`), not a captured index, because the selection may change while the
  dialog is open.

## Docs

README Features: "**App volume:** a dial or key for one app's volume (Spotify, a
browser…), or for whichever app is focused. The strip shows the app's icon, name and
level." The actions table row `app_volume` | `app`, `focused`, `change`, `mute`; a short
example. Roadmap: add and tick "Per-app volume (chosen or focused app)".
`examples/config.toml`: a commented Spotify dial and a focused-app dial. CHANGELOG under
Unreleased/Added.

## Capture on the machine first

Save outputs under `crates/omadeck-core/tests/fixtures/` with the names given; Task A's
fixture test reads them with `include_str!("../tests/fixtures/…")`.

1. **Streams**, one file per case:
   `pactl -f json list sink-inputs > crates/omadeck-core/tests/fixtures/sink-inputs-<case>.json`
   for `spotify` (playing), `spotify-paused`, `firefox-two-tabs`, `chromium`,
   `mpv-in-terminal`, `flatpak` (if one is installed), `none` (nothing playing). Check in
   each: `properties."object.id"` present, string or number; pids string or number;
   `volume.*.value_percent` form.
2. **Index vs node id:**
   `pactl -f json list sink-inputs | jq '.[] | {index, id: .properties."object.id", serial: .properties."object.serial"}'`.
3. **wpctl on a stream node:** `wpctl set-volume -l 1.0 <object.id> 5%+ && wpctl get-volume <object.id>`
   and `wpctl set-mute <object.id> 1 && wpctl get-volume <object.id>`; then back.
4. **Focused window:** `hyprctl -j activewindow` with Firefox, Chromium, a terminal running
   `mpv`, and on an empty workspace. With the pid: `pstree -p <pid>` and
   `ps -o pid,ppid,comm -p <stream pid>` for Chromium's audio process.
5. **Icons:** `ls /usr/share/icons/hicolor/*/apps/ | grep -i -E 'spotify|firefox|chromium|mpv'`,
   `grep -h '^Icon=' /usr/share/applications/{spotify,firefox,chromium,mpv}*.desktop ~/.local/share/applications/*.desktop 2>/dev/null`.
6. **Versions:** `pactl --version; pipewire --version; cat /etc/os-release | head -3`, and
   the Omarchy version (`omarchy-version` if present).
7. **Glyph:** `printf '\U000F08C6\n'` in the Omarchy terminal shows an application window
   symbol.

## Tasks

~150–300 lines each including tests, in order; each ends with its check and a commit
(`cargo fmt` first).

### Task A — streams: parse and read (~200 lines)

Files: `audio.rs` (`Stream`, `AppQuery`, `AppLevel`, `parse_pactl_streams`, `streams`),
fixtures from Capture 1. Tests: `pactl_streams_parse` (below), `pactl_garbage_is_none`
(`"x"` → None, `"[]"` → `Some(vec![])`), `pactl_skips_stream_without_object_id`,
`pactl_fixtures_parse` (every captured file parses; for `firefox-two-tabs` two streams
with the same `binary`; for `none` an empty list).

```rust
#[test]
fn pactl_streams_parse() {
    // Hand-written in the shape `pactl -f json list sink-inputs` gives; the captured
    // fixtures are checked in `pactl_fixtures_parse`.
    let json = r#"[{"index":91,"corked":false,"mute":false,
        "volume":{"front-left":{"value":45875,"value_percent":"70%","db":"-9.29 dB"},
                  "front-right":{"value":52428,"value_percent":"80%","db":"-5.81 dB"}},
        "properties":{"object.id":"87","application.name":"Spotify",
                      "application.process.id":"4242","application.process.binary":"spotify",
                      "application.icon_name":"spotify-client"}}]"#;
    let s = parse_pactl_streams(json).unwrap();
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].id, 87);
    assert_eq!(s[0].pid, Some(4242));
    assert_eq!(s[0].binary, "spotify");
    assert_eq!(s[0].name, "Spotify");
    assert_eq!(s[0].icon_name.as_deref(), Some("spotify-client"));
    assert_eq!(s[0].level, Level { percent: 80, muted: false });
    assert!(!s[0].corked);
}
```

If Capture 1 shows numbers instead of strings for ids or pids, accept both
(`as_u64().or_else(|| as_str()?.parse().ok())`) and add that form to this test.

**Check:** `cargo test -p omadeck-core audio::` → `test result: ok.`, 0 failed.

### Task B — matching, sticky focus and icons (~300 lines)

Files: `audio.rs` (`normalise_app`, `display_app`, `select_named`, `select_window`,
`proc_parent`, `app_level`, `resolve_named`, `resolve_focused`, `app_icon_in`/`app_icon`,
`app_volume_commands`). Tests: `select_named_forms` (`spotify`, `Spotify`,
`spotify.desktop`, `/usr/bin/spotify`, `com.spotify.Client` hit their stream; `spot`
doesn't), `select_window_pid_and_children` (fake `parent` map; stops at 1 and at a
cycle), `select_window_class_fallback`, `app_level_aggregates`, `display_name_rules`
(`audio-src` → title-cased binary), `sticky_focus` (below), `app_icon_lookup` (temp dir
with `icons/hicolor/scalable/apps/foo.svg` and `applications/bar.desktop` with
`Icon=foo`), `app_volume_commands_change_and_mute`.

```rust
fn stream(id: u32, pid: u32, binary: &str) -> Stream {
    Stream {
        id,
        pid: Some(pid),
        binary: binary.into(),
        name: binary.into(),
        app_id: None,
        icon_name: None,
        level: Level { percent: 40, muted: false },
        corked: false,
    }
}

#[test]
fn sticky_focus() {
    let s = vec![stream(87, 100, "firefox")];
    let no_parent = |_| None;
    let mut sticky = None;
    // Firefox focused: it, and it becomes sticky.
    let w = Window { pid: Some(100), class: "firefox" };
    let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
    assert_eq!(lv.ids, vec![87]);
    assert_eq!(sticky.as_deref(), Some("firefox"));
    // A silent terminal focused: still Firefox.
    let w = Window { pid: Some(200), class: "Alacritty" };
    let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
    assert_eq!(lv.ids, vec![87]);
    // Firefox stops: the terminal's class, no level.
    let lv = resolve_focused(&[], Some(&w), &mut sticky, no_parent);
    assert_eq!((lv.name.as_str(), lv.level), ("Alacritty", None));
    assert_eq!(sticky.as_deref(), Some("firefox"), "kept until another app plays");
}
```

**Check:** `cargo test -p omadeck-core audio::` → `test result: ok.`, 0 failed.

### Task C — config and the action outside the daemon (~200 lines)

Files: `config.rs` (the variant, `KINDS`, `kind_index`, `empty_of_kind`, `default_for`,
`summary`, `app_query`s, `LevelSource::App`, `level_source`), `action.rs` (`run` /
`run_steps` arms, `run_app_volume`), `audio.rs` (`focused_window`), and the compile fixes
in `render.rs`/`probe.rs` (`LevelSource::App => …` arms that draw `$X1` until Task D; the
GUI's `action_rows` gets an empty arm until Task F). Tests: `app_volume_roundtrip` (below),
`old_config_saves_byte_identical` (still passes), `level_source_app` (a `Volume` turn
plus an `AppVolume` press: the first in turn_right, turn_left, press order wins),
`summary_app_volume` (the five strings), `kinds_has_app_volume_at_11`.

```rust
#[test]
fn app_volume_roundtrip() {
    let mut c = Config::default();
    let d = c.dial_mut(1);
    d.turn_right = Action::AppVolume { app: "spotify".into(), focused: false, change: 5, mute: false };
    d.press = Action::AppVolume { app: String::new(), focused: true, change: 0, mute: true };
    let s = toml::to_string_pretty(&c).unwrap();
    assert!(s.contains(r#"type = "app_volume""#), "{s}");
    assert!(s.contains(r#"app = "spotify""#), "{s}");
    assert!(!s.contains("focused = false"), "defaults are left out: {s}");
    assert!(!s.contains("change = 0"), "{s}");
    assert_eq!(toml::from_str::<Config>(&s).unwrap(), c);
    assert_eq!(c.dial(1).unwrap().level_source(), Some(LevelSource::App));
    assert_eq!(c.dial(1).unwrap().app_query(), Some(AppQuery::Named("spotify".into())));
}
```

**Check:** `cargo test -p omadeck-core config::` → `test result: ok.`, 0 failed;
`cargo build --workspace` clean.

### Task D — render: `tone_in`, clamp, the app panel (~250 lines)

Files: `render.rs` (`AppShow`, `PanelStatus.app`, `tone_in`, per-rect tones,
`APP_GLYPH`, `fade`, the `App` arm), `probe.rs` (`Probe::AppVolume`, `Value::App`,
`app_show`, the `dial_status` arm, the `shows` case), `audio.rs` (serde derives on
`AppLevel` and `Level`). Fix struct literals `cargo build` flags with `app: None`. Tests: `tone_in_follows_the_text_area`
(below), `b1_fill_above_one_stays_in_bar` (`value: 1.5`: pixels right of the bar rect stay
background), `app_panel_dimmed_fades_title` (title `A`, as the goldens use:
the title pixel furthest from the background is closer to it than on the undimmed
panel), `app_show_cases` (one assert per table row),
the goldens unchanged.

```rust
#[test]
fn tone_in_follows_the_text_area() {
    // Top half white (under the title), bottom half black.
    let mut img = RgbaImage::from_pixel(PANEL_W, PANEL_H, Rgba([0, 0, 0, 255]));
    for y in 0..PANEL_H / 2 {
        for x in 0..PANEL_W {
            img.put_pixel(x, y, Rgba([255, 255, 255, 255]));
        }
    }
    let b = Backdrop { tone: mean_rgb(&img), img, has_image: true };
    assert_eq!(b.tone_in(X1_TITLE), Rgb(255, 255, 255));
    assert_eq!(b.tone_in(B1_BAR), Rgb(0, 0, 0));
    // The whole-panel mean is mid-grey; that's what used to pick the title colour.
    assert_eq!(b.tone, Rgb(127, 127, 127));
}
```

**Check:** `cargo test -p omadeck-core -- render:: probe::` → `test result: ok.`, 0 failed.

### Task E — watcher and daemon (~250 lines)

Files: `crates/omadeckd/src/watch.rs` (grouped read in `run_due`, `Poller.focus`,
`Poller.sticky`, `WatchCmd::Focus`, `WatchCmd::AppVolume` in `handle`), `daemon.rs`
(`dispatch` arm, the `ActiveWindow` forward), core `probe.rs` (`watched_probes`). Pull the per-pass answer into a pure function and test it:

```rust
/// Answer every watched app probe from one stream list.
fn app_values(probes: &[AppQuery], streams: &[Stream], focus: Option<&hypr::ActiveWindow>,
              sticky: &mut Option<String>, parent: impl Fn(u32) -> Option<u32> + Copy) -> Vec<(Probe, Value)>;
```

Tests (in `watch.rs`): `one_read_answers_all_app_probes` (below),
`focused_without_window_is_dimmed_class_less`, and in core `probe.rs`
`watched_probes_adds_app_dials`.

```rust
#[test]
fn one_read_answers_all_app_probes() {
    let streams = vec![stream(87, 100, "spotify"), stream(90, 300, "firefox")];
    let focus = hypr::ActiveWindow { address: "a".into(), class: "firefox".into(), title: "t".into(), pid: Some(300) };
    let mut sticky = None;
    let qs = [AppQuery::Named("spotify".into()), AppQuery::Focused, AppQuery::Named("mpv".into())];
    let out = app_values(&qs, &streams, Some(&focus), &mut sticky, |_| None);
    let ids = |q: &AppQuery| match out.iter().find(|(p, _)| *p == Probe::AppVolume(q.clone())) {
        Some((_, Value::App(lv))) => lv.ids.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(ids(&qs[0]), vec![87]);
    assert_eq!(ids(&qs[1]), vec![90]);
    assert!(ids(&qs[2]).is_empty(), "mpv isn't playing");
}
```

(`stream` as in Task B; copy it into this test module or move it to a shared
`#[cfg(test)] pub fn` in `audio.rs`.)

**Check:** `cargo test -p omadeckd` → `test result: ok.`, 0 failed;
`cargo clippy --all-targets -- -D warnings` clean. Then the named-dial and focused-dial
hardware checks.

### Task F — GUI (~250 lines)

Files: `editor.rs` (the `AppVolume` arm, presets, Try-it rule), `apps.rs`
(`pick_audio`). No test (GTK). **Check:** `cargo clippy --all-targets -- -D warnings`
clean; by hand: both presets fill all three actions; the picker lists what's playing; the
preview follows the deck while the daemon runs.

### Task G — docs (~60 lines)

README, `examples/config.toml`, CHANGELOG. **Check:**
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → all green.

## Hardware checks

- [ ] Spotify dial: turn ±5% matches Omarchy's audio panel slider; a fast spin sends one
      change, never above 100%.
- [ ] Press mutes Spotify only (the system volume stays); the panel shows `Muted` at once.
- [ ] Spotify not running: dimmed, Spotify's icon, `–`. Start it and play: lights up
      within ~1 s.
- [ ] Spotify paused (corked): still shows its level, turning still works.
- [ ] Firefox with two playing tabs: both change, the panel shows the louder. Chromium the
      same (audio from a child process).
- [ ] Focused dial: focus Firefox → Firefox. Focus a silent terminal → still Firefox
      (sticky). Focus a terminal playing `mpv` → mpv.
- [ ] Focused dial right after daemon start on a silent window: dimmed, the window's
      class, `–`.
- [ ] Alt-tab quickly: the panel follows without a blink; daemon CPU ~0% at idle.
- [ ] A Flatpak app (if any) matches by class.
- [ ] A strip image that's dark under the title and light under the value: each text
      picks its own contrast colour.
- [ ] The configurator's strip preview matches the deck.

## Decided (2026-10-17)

- **Decided: a new `AppVolume` action at kind 11** (D14), not a `Volume` target.
- **Decided: `AppQuery::{Named, Focused}` is the probe key;** the watcher resolves focus
  and the sticky app itself, so what the panel shows and what a turn changes come from
  the same place and the same read.
- **Decided: changes run on the watcher thread** (`WatchCmd::AppVolume`) with a fresh
  stream read, in order with every other change, then a re-read (D5). No optimistic bar.
- **Decided: all streams of an app move together;** the panel shows the loudest and reads
  muted only when all are. Change is relative per stream; mute sets one state for all.
- **Decided: `pactl` to read, `wpctl` to write;** streams without `object.id` are
  skipped. No `pw-dump` fallback.
- **Decided: pid first (with ancestors), class second.**
- **Decided: no pop-up** for app volume (Omarchy's OSD can't show a name with a bar;
  its speaker icon would read as the system volume).
- **Decided: the app's icon over the mute glyph** when muted.
- **Decided: icon lookup without GTK** in the daemon, cached, on the watcher thread.
- **Decided: focused is the default** for a new App volume action.
- **Decided: `tone_in` for every `$B1` panel's title and value,** not only the app panel;
  pixels on plain colours are unchanged.
- **Decided: the GUI picker reads streams once** when it opens. It's a user-started list,
  like the installed apps; the preview itself only mirrors daemon events (D2).

## Ask before building

1. A focused-app dial on a silent window: keep controlling the last app that had audio
   (planned, "sticky"), do nothing, or fall back to the system volume? Only
   `resolve_focused` and `app_show`'s table change.
2. The picker's one-off stream read in the GUI (above) bends D2. Keep it, or make the
   picker free text plus installed apps only until PR 11 can ask the daemon?

## Decisions to explain

- **A new `AppVolume` action, not a `Volume` target.** Elgato also keeps App separate from
  Input and Output.
- **Focused mode at all.** Not in Elgato's plugin; copied from the third-party Focused
  Application Volume, because I wanted it. Hyprland only; elsewhere it stays dimmed.
- **Sticky focus.** Click away from a video and still turn it down. The panel always names
  what the dial will change (the third-party plugin's rule), so it never acts on
  something hidden.
- **All streams of an app, showing the highest.** One dial for "Firefox", like Windows'
  per-process sessions.
- **pactl to read, wpctl to write.** `wpctl -l 1.0` keeps PR 4's 100% cap.
- **No pop-up.** The strip panel is the feedback; on a key there's none (Later).
- **Per-rect text contrast** (`tone_in`): a strip image can be dark under the title and
  light under the value.

## Later

- An app-named pop-up if Omarchy's OSD ever takes a message with a bar.
- Per-tab control (by `media.name`); input streams (a per-app mic level for Discord).
- Setting the volume of an app that isn't running (WirePlumber's stream-restore table).
- Elgato's Auto Detection: the running apps on the dials by themselves.
- Live key faces showing an app's level.
- `pactl subscribe` instead of 1 s polling, if polling feels slow; a `pw-dump` fallback.

**Size:** about 1,500 lines including tests over 7 tasks (A 200, B 300, C 200, D 250,
E 250, F 250, G 60).
