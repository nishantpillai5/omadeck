# PR 10: `feat/app-volume`: per-app volume, for a chosen app or the focused one

Part of [PLAN.md](PLAN.md). Base: `feat/live-keys` (PR 9). Uses the **state watcher**
(PR 8, `watch.rs`) for stream levels and the **Hyprland event listener** (PR 9, `hypr.rs`)
for focus. Owns the **audio streams** building block, which PR 14 (media dial) reuses.

**Outcome:** a new `app_volume` action that changes or mutes the volume of one app's
audio streams. It has two modes. **Chosen app** controls one app you pick (e.g. Spotify).
**Focused app** controls whichever app has the focused Hyprland window, and keeps
controlling the last focused app that had audio when you move to a silent window. On a
dial, the strip panel uses `$B1` and shows the app's icon, its name as the title, the
level and a bar. When the app has no stream, the panel is dimmed with `–`. Turn ±5%,
capped at 100%. Press mutes the app. Keys can run the action too.

## How the Elgato app does it (researched 2026-10-16)

- **Elgato's own plugin is "Volume Controller"** (`com.elgato.volume-controller`, v2.0.4.196,
  11 Aug 2026, Windows and Mac, dials supported). Its actions are **Profile** (builds a
  volume profile from the apps that are running), **App** ("Control volume for a specific
  app": set, adjust or mute), **Input** and **Output** (a chosen device). There is also a
  separate "Volume Controller for macOS" plugin.
  [Marketplace](https://marketplace.elgato.com/product/volume-controller-34d9aa59-a41a-4a4c-a853-202ca91409e1),
  [Top 5 plugins](https://www.elgato.com/us/en/explorer/products/stream-deck/top-5-stream-deck-plugins-for-getting-started/),
  [Help: Volume Controller](https://help.elgato.com/hc/en-us/articles/10568396413965-Elgato-Stream-Deck-Volume-Controller-Plugin)
  (the help page returned 403; the next point comes from the search snippet of it).
- **On dials:** "Manual Detection: Select an application and rotate the dials to adjust
  the volume. Press the dial or LCD screen to toggle mute." There is also an "Auto
  Detection" mode that seems to put the running apps on the dials by itself
  (**unconfirmed**: I couldn't read the page).
- **Strip contents:** app icon, app name and level in an indicator layout. This is
  **unconfirmed** for Elgato's plugin. I'm assuming it from the `$B1` layout and the
  third-party plugin below.
- **Focused app:** Elgato's plugin has no mode that follows focus, as far as I could find.
  The third-party **Focused Application Volume** (dlprows, Windows only, v1.4, dials
  supported) does. Its README says: "The screen updates to show the name/icon of the app
  so that you can always know what you're about to change". It matches a focused process
  to its audio session, with a per-app override file for apps whose audio runs in another
  process.
  [Marketplace](https://marketplace.elgato.com/product/focused-application-volume-916c9a3f-453e-47f5-a194-5719448cdd41),
  [GitHub](https://github.com/dlprows/FocusVolumeControl). What it does when the focused
  app has no audio is **unconfirmed**.
- **App not running:** **unconfirmed** for both plugins.

**What Omarchy offers** (checked in `omacom/omarchy`, branch `quattro`, 2026-10-16):
- **No per-app volume command.** The `omarchy-audio-*` scripts work on devices only.
- The Quickshell audio panel (`shell/plugins/panels/audio/Model.js`) lists playback
  streams. It labels each one with `application.name`, falling back to `media.name` and
  `node.name`, and it treats `audio-src` as a generic label.
- Omarchy 4 removes `wiremix`, `pavucontrol` and `playerctl` (`retired_packages` in
  `bin/omarchy-upgrade-to-quattro`). `pactl` (pipewire-pulse) and `wpctl` stay, because
  Omarchy's own scripts use them.
- `omarchy-osd` shows **either** a message **or** an icon plus a progress bar, never both
  (`stateForShow` in `shell/plugins/osd/OsdModel.js` sets `hasProgress` only when the
  message is empty). So it can't show "Spotify 40%" with a bar. **Decision: no pop-up for
  app volume** (see Decisions).

## Config

New `Action` variant in `omadeck-core/src/config.rs`:

```rust
/// Change or mute one app's audio streams (PipeWire sink inputs).
AppVolume {
    /// The app to control: a process name (`spotify`), an app name (`Firefox`), a
    /// Flatpak id (`com.spotify.Client`) or a desktop id (`spotify.desktop`).
    /// Ignored when `focused` is set.
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
[dial.turn_left]
type = "app_volume"
app = "spotify"
change = -5
[dial.turn_right]
type = "app_volume"
app = "spotify"
change = 5
[dial.press]
type = "app_volume"
app = "spotify"
mute = true

[[dial]]
index = 2                    # follows the focused window
[dial.turn_right]
type = "app_volume"
focused = true
change = 5
# turn_left: change = -5; press: mute = true
```

- `KINDS` gains "App volume" at the next free index (**9**: PRs 7–9 add no kinds; `KINDS` only grows at the end).
  `empty_of_kind` → `AppVolume { app: "", focused: true, change: 5, mute: false }`. Focused
  is the default because it works without choosing anything.
- `summary()`: `Spotify volume +5%`, `Toggle mute (Spotify)`, `Focused app volume −5%`,
  `Toggle mute (focused app)`, and `App volume (no app set)` when there is no target.
- New `pub enum AppTarget { Named(String), Focused }` and
  `DialConfig::app_target() -> Option<AppTarget>`: the first `AppVolume` among
  turn_right, turn_left and press.
- `LevelSource` gains `App`. It is a unit variant, so the enum stays `Copy`; the target is
  read through `app_target()`. `level_source()` order: the first `Volume` **or**
  `AppVolume` (turn_right, turn_left, press), then `Brightness`.
- Old configs: no new fields on existing structs, so they load and save byte-identical.
  The round-trip test covers it.

## Choice: a new `Action::AppVolume`, not `Action::Volume { target = "app" }`

- `Volume` is defined by its device backends: Omarchy scripts, swayosd, the OSD and the
  physical-sink lookup. App volume uses none of them and needs fields of its own (`app`,
  `focused`). Adding those to `Volume` would leave combinations that mean nothing, such
  as `device = "input"` with `app = …`.
- Leaving `Volume` untouched keeps PR 4's serde, tests and GUI arm as they are.
- The panel's level comes from a different source (a stream probe instead of a device
  poll), and the daemon has to resolve focused mode before it runs the action. A separate
  variant makes both of those one `match` arm.
- Elgato splits them the same way: App, Input and Output are separate actions in Volume
  Controller.

## Audio streams building block (`omadeck-core/src/audio.rs`)

This is the shared contract, which PR 14 reuses. **Read** with `pactl -f json list
sink-inputs`: it is compact, it is what Omarchy's scripts use, and pipewire-pulse is
always present on Omarchy. Fall back to `pw-dump` when `pactl` is missing. **Write**
with `wpctl` on the PipeWire node id, so `-l 1.0` caps at 100% as in PR 4. `pactl
set-sink-input-volume` has no cap, which is why it isn't used.

```rust
/// One playback stream (a sink input).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub id: u32,                  // PipeWire node id (`object.id`), what wpctl takes
    pub pid: Option<u32>,         // application.process.id
    pub binary: String,           // application.process.binary, lowercased ("" if absent)
    pub name: String,             // application.name, else media.name, else node.name
    pub app_id: Option<String>,   // pipewire.access.portal.app_id / application.id (Flatpak)
    pub icon_name: Option<String>,// application.icon_name
    pub level: Level,             // the loudest channel's percent, plus mute
    pub corked: bool,             // paused but present: still controllable
}
/// What to control, as the watcher and the action resolve it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppQuery { Named(String), Window { pid: u32, class: String } }
/// One app's streams added up, for the panel and the action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppLevel {
    pub name: String,             // display name (see `display_name`)
    pub binary: String,           // the key used for "sticky" focus
    pub icon: Option<PathBuf>,    // resolved icon file, if any
    pub level: Level,             // percent = max over streams; muted = all muted
    pub ids: Vec<u32>,
}
pub fn streams() -> Option<Vec<Stream>>;               // None: no tool / failed
pub fn parse_pactl_streams(json: &str) -> Option<Vec<Stream>>;
pub fn parse_pw_dump_streams(json: &str) -> Option<Vec<Stream>>; // media.class == Stream/Output/Audio
pub fn select<'a>(s: &'a [Stream], q: &AppQuery, parent: impl Fn(u32) -> Option<u32>) -> Vec<&'a Stream>;
pub fn app_level(s: &[Stream], q: &AppQuery) -> Option<AppLevel>;   // None: no matching stream
pub fn proc_parent(pid: u32) -> Option<u32>;           // /proc/<pid>/stat, the field after ")"
pub fn app_icon(names: &[&str]) -> Option<PathBuf>;    // cached lookup, see below
pub fn focused_window() -> Option<(u32, String)>;      // `hyprctl -j activewindow` → pid, class
```

**Matching** (`select`, pure, with `parent` injected so tests can fake `/proc`):
- `Named(q)`: normalise `q` (trim, lowercase, take the last `/` segment, strip
  `.desktop`). A stream matches when that equals its `binary`, its lowercased `name` or
  its lowercased `app_id`. Binary is the best key: it is stable, and the GUI picker
  writes it.
- `Window { pid, class }`:
  1. **Pid first.** A stream matches when its `pid == pid`, or when `pid` appears among
     its ancestors (walk `parent` up to pid 1, at most 32 hops). This covers Chromium and
     Electron, whose audio runs in a child process, and also a terminal running `mpv`
     (the terminal's window controls mpv).
  2. **Class second**, only if no pid matched: `class` (lowercased) equals the stream's
     `app_id`, `binary` or lowercased `name`. This covers Flatpak apps, whose pids live in
     a sandbox namespace, and apps whose audio comes from a helper outside their process
     tree.
- **Several streams per app** (Firefox and Chromium create one per tab or player): the
  action applies to **all** of them. The panel shows the **highest** level, and muted
  only when **all** are muted. Per-tab control is under Later.
- `display_name`: Omarchy's rule. Use `application.name` unless it's empty or generic
  (`audio-src`, `ALSA plug-in [...]`), else title-case the binary. A known-names map
  (`spotify` → `Spotify`) handles the rest.
- `app_icon`: no GTK in the daemon. Try, in order, `icon_name`, `binary`, the lowercased
  name and `app_id`. For each, look for `<name>.{svg,png}` in `$XDG_DATA_HOME` and
  `$XDG_DATA_DIRS` (`icons/hicolor/{scalable,256x256,128x128,64x64,48x48}/apps`,
  `pixmaps`, and the Flatpak `exports/share/icons`). If that finds nothing, read `Icon=`
  from `applications/<name>.desktop` and look that name up the same way. Results,
  misses included, are cached in a `OnceLock<Mutex<HashMap>>`. The renderer already loads
  SVG and PNG from a path.

**Commands** (`action.rs`, pure `app_volume_commands(ids, change, mute_to) -> Vec<Vec<String>>`):
- Change: one command per stream, `wpctl set-volume -l 1.0 <id> 5%+` (or `5%-`), run in
  order by `spawn_seq`. The change is relative, so tabs keep their balance and two quick
  turns can't overwrite each other's read.
- Mute: one target state for all streams. If any is unmuted, mute all
  (`wpctl set-mute <id> 1`), else unmute all (`… 0`). A plain `toggle` would flip a mixed
  set into a different mixed set.
- `run_steps` folds a fast spin into one change of `change × steps`, clamped to ±100,
  the same as `Volume`.
- `pub fn run_app_volume(q: AppQuery, change: i8, mute: bool, steps: u8) -> Result<()>`:
  on the helper thread, read `streams()`, `select`, then run the commands. With no
  matching stream it logs and does nothing (it's not an error toast, because the panel
  already shows `–`).
- `action::run(AppVolume)`: named mode → `run_app_volume(Named(app))`. Focused mode,
  outside the daemon (`omadeckd press`, a keybind): `focused_window()` →
  `Window { pid, class }`. With an empty `app` and `focused` unset, it fails with "no app
  set".

## Core / render

- `render::DialStatus` gains `app: Option<AppFace>`, where
  `pub struct AppFace { pub title: String, pub icon: Option<PathBuf>, pub dimmed: bool }`.
- `render_strip_panel`, arm `LevelSource::App`, draws `$B1` through `draw_b1`. The title
  is `app.title` (still subject to `show_label`). The icon is chosen in this order: the
  user's `icon`, then `app.icon`, then the user's `glyph`, then `APP_GLYPH` (nf-md
  `application`, `f08c6`; check the codepoint renders). The app keeps its own icon when
  muted (not the mute glyph): which app it is matters more, and the text already says
  `Muted`.
- `dimmed`: draw the title, value and bar with `fg.mix(bg, 0.5)`, and the icon at half
  alpha. Value `–`, empty bar.
- One pure helper decides what the panel shows, so the daemon, the GUI preview and
  `snapshot` agree. `pub fn app_panel(dial, target, lv: Option<&AppLevel>,
  window_class: Option<&str>) -> (AppFace, Option<render::Level>)`:

| Case | Title | Icon | Value | Dimmed |
|---|---|---|---|---|
| Named, playing | label, else app name | app icon | `45%` / `Muted` | no |
| Named, no stream | label, else `app` title-cased | `app_icon([app])` | `–` | yes |
| Focused, app with audio (focused or sticky) | app name (label ignored) | app icon | `45%` / `Muted` | no |
| Focused, no audio anywhere yet | focused window class, else label, else `Focused app` | glyph | `–` | yes |

## Daemon (`omadeckd`)

Assumptions about other PRs' blocks (adapt if they land differently):
- **PR 8:** `watch.rs` has `Probe` (Hash + Eq), `Value`, `WatchCmd::{Watch(Vec<Probe>),
  Refresh(Probe)}`, and `Deck.values: HashMap<Probe, Value>`. The probes come from one
  function (here called `watched_probes(cfg, deck)`), recomputed on connect, reload and
  page change. If PR 8 left the PR 4 `AudioWorker` separate, this PR still adds its
  probes to `watch.rs` and doesn't touch `AudioWorker`.
- **PR 9:** `HyprEvent::ActiveWindow { class, title, pid: Option<u32> }` arrives on the
  main loop. `activewindow>>` carries no pid, so I assume the listener thread gets it
  (with `hyprctl -j activewindow` on `activewindowv2>>`) **on its own thread**. If PR 9
  didn't do that, this PR adds it in `hypr.rs`, never on the deck loop.

Changes:
- `Probe::App(AppQuery)` and `Value::App(Option<AppLevel>)`. The worker groups every
  `Probe::App` into **one** `audio::streams()` per tick (1 s, like `AUDIO_POLL`), then
  runs `app_level` per probe and sends only the ones that changed. `proc_parent` and
  `app_icon` run here, never on the deck loop. A new `Watch` polls straight away, so a
  focus change shows within tens of ms.
- `Deck` gains `active_window: Option<(u32, String)>` and `focus_sticky: Option<String>`
  (the binary of the last focused app that had audio).
- `watched_probes` adds `Probe::App(Named(app))` for each chosen-app dial. Each focused
  dial adds `Probe::App(Window { pid, class })` for the active window, plus
  `Probe::App(Named(sticky))` when there is a sticky app.
- Focus change: when `HyprEvent::ActiveWindow` arrives and some dial is in focused mode,
  update `active_window` and send `Watch(watched_probes(..))`. No repaint until the value
  arrives, so the panel doesn't blink.
- Value received: store it in `state`, then repaint the dials whose probe it is. When a
  `Window` probe answers `Some(lv)` and `lv.binary != focus_sticky`, set
  `focus_sticky = lv.binary` and send `Watch` again.
- `focused_query(d) -> Option<AppQuery>`: the window probe's query if its value is
  `Some`, else `Named(sticky)` if that value is `Some`, else `None`. It is the single
  answer to "what does the focused dial control", used for both the panel and the action,
  so what you see is what you turn.
- Key, turn, press and strip-tap handlers: `AppVolume { focused: true, .. }` is
  intercepted (as `Page`/`Brightness` are) → `focused_query(d)` →
  `action::run_app_volume`. With `None`, log "no focused app with audio". Named mode goes
  through `action::run_steps` as usual.
- `after_app_volume` (the counterpart of `after_volume`): optimistic bar from the last
  value ± change, clamped 0–100, and mute flipped on press. Repaint, then
  `WatchCmd::Refresh(probe)`.
- Reload: keep the `state` entries whose probes are still watched (the PR 6 no-blink
  fix). Keep `focus_sticky`. Page change: dials don't depend on the page, so no change.
  Disconnect: `Watch(vec![])`. The sticky app is lost with the `Deck`, which is fine.
- `snapshot`: named dials show the dimmed "no stream" face, and focused dials show
  `Focused app` (a snapshot has no focus).
- IPC: no new events. Focus already reaches the GUI through PR 9's events if needed.

## GUI (`omadeck-gui`)

- `action_editor` gets a new `Action::AppVolume` arm. Description: "Changes one app's
  volume. On a dial, the strip shows the app's icon, name and level."
  - **Follow the focused window** (SwitchRow): hides the app rows when on.
  - **Application** row: subtitle shows the chosen name, or "None chosen", and the icon
    goes in the prefix. **Choose…** opens a new `apps::pick_audio` dialog. Its top
    section, **Playing now**, lists `audio::streams()` deduped by binary, with the
    `gio::AppInfo` icon when `apps::find("<binary>.desktop")` hits. Below it, **Installed
    apps…** opens the existing `apps::pick`, and the pick is stored as
    `info.executable()`'s basename. Either way `app` holds a binary, and the dial label is
    filled through `on_app_picked` when it's empty.
  - **App** EntryRow for free text (process name, app name or Flatpak id).
  - **Toggle mute** switch, plus a "Change per turn (%)" spin (−25…25) when mute is off,
    as in the Volume arm.
- Presets in `build_dial`: **App volume dial** (opens the picker, then fills
  turn_left/right ±5 and press mute) and **Focused app dial** (fills all three with
  `focused = true`, label empty). There are four buttons now, so if the header gets too
  wide, move the presets into a `gtk::MenuButton` "Presets".
- *Try it now*: works for chosen-app actions. It is insensitive for focused mode, with
  the tooltip "Follows the focused window — the configurator would be the focused app".
- Strip preview (`App::refresh_strip`): one `audio::streams()` per refresh, then
  `app_panel` for named dials. Focused dials show the dimmed `Focused app` face.

## Starter layout

No change. The starter has no dials.

## Docs

- README Features: "**App volume:** a dial or key for one app's volume (Spotify, a
  browser…), or for whichever app is focused. The strip shows the app's icon, name and
  level." Add the actions table row: `app_volume` | `app`, `focused`, `change`, `mute`.
  Add a short "App volume dial" example. Roadmap: add and tick "Per-app volume
  (chosen or focused app)".
- `examples/config.toml`: a commented Spotify dial and a focused-app dial.
- CHANGELOG under Unreleased/Added.

## Implementation steps

1. [ ] `audio.rs`: `Stream`, `parse_pactl_streams`, `parse_pw_dump_streams`, `streams()`,
   with tests built from real JSON captured on the Omarchy machine (Spotify, Firefox
   with two tabs, Chromium, mpv in a terminal, one Flatpak if any).
2. [ ] `AppQuery`, `select`, `proc_parent`, `app_level`, `display_name`, with tests.
3. [ ] `app_icon` lookup and cache, with a tempdir test.
4. [ ] `config.rs`: `Action::AppVolume`, KINDS, `empty_of_kind`, `summary`, `AppTarget`,
   `app_target()`, `LevelSource::App`, round-trip tests.
5. [ ] `action.rs`: `app_volume_commands`, `run_app_volume`, `run`/`run_steps` arms,
   `focused_window`.
6. [ ] `render.rs`: `AppFace`, `DialStatus.app`, the `App` arm, dimmed drawing,
   `app_panel`.
7. [ ] `state.rs` / `watch.rs`: `Probe::App`, `Value::App`, a grouped read per tick.
8. [ ] `main.rs`: named dials end to end (probes, repaint, `after_app_volume`, reload).
   Check on hardware.
9. [ ] `main.rs`: focused mode (active window, sticky, `focused_query`, action
   intercept). Check on hardware.
10. [ ] GUI arm, `pick_audio`, presets, preview, Try-it rule.
11. [ ] `snapshot`, docs, CHANGELOG. Then `cargo fmt && cargo clippy --all-targets -- -D
    warnings && cargo test`.

## Tests

- `pactl_streams_parse`: real JSON → ids from `object.id`, pid, binary, name, max
  channel percent, mute, corked. Missing `properties` keys don't fail the stream. A
  stream without `object.id` is skipped. Garbage → `None`. `[]` → `Some(vec![])`.
- `pw_dump_streams_parse`: keeps only `Stream/Output/Audio` nodes. Volume comes from
  `info.params.Props[0].channelVolumes`, cube-rooted. wpctl shows cubic volumes, so check
  the number matches `wpctl get-volume <id>`.
- `select_named`: `spotify`, `Spotify`, `spotify.desktop`, `/usr/bin/spotify`,
  `com.spotify.Client` all hit their stream, and `spot` doesn't.
- `select_window_pid_and_children`: the window pid matches its own stream and a
  grandchild's stream (fake `parent` map). An unrelated pid doesn't match. The walk stops
  at 1 and at a cycle.
- `select_window_class_fallback`: a Flatpak stream with a foreign pid matches by
  `app_id == class`. Class is used only when no pid matched.
- `app_level_aggregates`: max percent, muted only if all are, every id included, and
  `None` for no match.
- `display_name_rules`: `audio-src` → title-cased binary, `spotify` → `Spotify`.
- `app_volume_commands_change_and_mute`: one capped `wpctl set-volume -l 1.0 <id> 5%+`
  per id. Mute with a mixed set → `set-mute <id> 1` for all. Mute with all muted →
  `… 0`.
- `run_steps_folds_app_volume`: `change × steps`, clamped.
- `app_volume_roundtrip`: TOML both ways, with defaults left out. An old config without
  `app_volume` saves byte-identical.
- `level_source_app`: `App` for an app dial. A `Volume` turn plus an `AppVolume` press
  gives whichever is first in turn_right, turn_left, press order.
- `summary_app_volume`: the four strings plus "no app set".
- `app_panel_cases`: one assert per table row.
- `app_icon_lookup`: a tempdir with `icons/hicolor/scalable/apps/foo.svg` and a desktop
  file with `Icon=bar`. Misses are cached.
- render: `app_panel_dimmed_draws_dash`. The value region is not empty and the bar track
  has no fill (the same kind of pixel check as the existing B1 tests).

## Hardware checks

- [ ] Spotify dial: turn ±5%, matching pavucontrol's/Omarchy panel's stream slider; spin
      fast → one command per stream, never above 100%.
- [ ] Press mutes Spotify only (the system volume stays). The panel shows `Muted` within
      one poll.
- [ ] Spotify not running: the panel is dimmed with the Spotify icon and `–`. Start it
      and play: the panel lights up within ~1 s.
- [ ] Spotify paused (corked): the panel still shows its level and turning still works.
- [ ] Firefox with two playing tabs: both change, and the panel shows the louder one.
      Chromium the same (its audio comes from a child process).
- [ ] Focused dial: focus Firefox → shows Firefox. Focus a terminal with no audio → still
      shows and controls Firefox (sticky). Focus a terminal playing `mpv` → shows mpv.
- [ ] Focused dial right after daemon start, with a silent window focused: dimmed with
      the window's class and `–`.
- [ ] Alt-tab quickly through windows: the panel follows without a blink, and the
      daemon's CPU stays ~0% at idle.
- [ ] A Flatpak app (if one is installed) matches by class.
- [ ] `object.id` is present in `pactl -f json list sink-inputs` props on Omarchy 4, and
      `wpctl set-volume` works on a stream node id.
- [ ] Note the Omarchy version, and the `pactl` and `wpctl` versions.

## Decisions to explain

- **A new `AppVolume` action, not a `Volume` target.** See the Choice section. Elgato also
  keeps App separate from Input and Output.
- **Focused mode at all.** Elgato's plugin doesn't have it. It is copied from the popular
  third-party Focused Application Volume, and the user asked for it. It is
  Hyprland-only, and outside Hyprland focused mode stays dimmed.
- **Sticky focus.** When the focused window has no audio, the dial keeps controlling the
  last focused app that had audio, and shows its name. This lets you click away from a
  video and still turn it down. The panel always names what the dial will change (the
  third-party plugin's own rule), so it never acts on something hidden.
- **All streams of an app, showing the highest.** It's one dial for "Firefox", like
  Windows' per-process sessions. The change is relative per stream, which keeps tab
  balance and avoids a read-then-write race. Mute sets one state for all streams.
- **pactl to read, wpctl to write.** `pactl` gives compact JSON and is what Omarchy uses.
  `wpctl -l 1.0` keeps PR 4's 100% cap, which `pactl set-sink-input-volume` lacks.
  `pw-dump` is only a fallback because it dumps the whole graph every second.
- **No pop-up.** Omarchy has no per-app OSD, and `omarchy-osd` can't show a name and a bar
  together. Its speaker icon with a bar would read as the *system* volume. The strip
  panel is the feedback. On a key this means no visual feedback (Later).
- **App icon over the mute glyph** when muted: the text already says `Muted`, and which
  app it is matters more.
- **Icon lookup without GTK in the daemon:** a small hicolor/pixmaps/desktop-file search,
  cached, so the daemon gains no dependencies.
- **Focused is the default** for a new App volume action, so it works before you choose
  anything.

## Later

- An app-named pop-up, if Omarchy's OSD ever takes a message with a bar (or use `-m
  "Spotify 45%"` for keys only).
- Per-tab or per-stream control (choosing by `media.name`).
- Setting the volume of an app that isn't running (WirePlumber's stream-restore table).
- Elgato's **Profile/Auto Detection**: put the running apps on the dials by themselves.
- Live key faces showing an app's level (with PR 9's live keys).
- Input streams (source outputs), e.g. a per-app mic level for Discord.
- A `pactl subscribe` event stream instead of 1 s polling, if polling feels laggy.
- A setting for focused mode's fallback (sticky / nothing / system volume).

**Size:** about 950 lines (core ~450 with tests, daemon ~200, GUI ~250, docs ~50).

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- Focused-app dial when the focused window has no audio: keep controlling the last app that had audio (planned, "sticky"), do nothing, or fall back to system volume?
- Browsers: all streams (tabs) of an app move together and the panel shows the loudest (planned). OK?
- Confirm on hardware that `pactl -f json` gives `object.id`, that `wpctl set-volume` works on stream node ids, and that `f08c6` is the right default app glyph.
- About 950 lines: split the GUI app picker into a follow-up PR?
