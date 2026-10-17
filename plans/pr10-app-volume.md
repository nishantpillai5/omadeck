# PR 10: `feat/app-volume`: per-app volume, for a chosen app or the focused one

Part of [PLAN.md](PLAN.md). Base: `feat/live-keys` (PR 9), which sits on `fix/review-2`
and `feat/toggle-keys` (PR 8, `55d0d62`). Uses groundwork's **state watcher** (core
`probe.rs`, daemon `watch.rs`) for stream levels and PR 9's **Hyprland event listener**
(`omadeckd/src/hypr.rs`) for focus. Owns the **audio streams** building block
(`audio.rs`; no later plan uses it), and adds `Backdrop::tone_in`, which PR 14 uses.

**Rewritten 2026-10-17 (second time)** against the code as merged on `feat/toggle-keys`
(`55d0d62`), the round-2 decisions R1–R9 that `fix/review-2` delivers, and PR 9's plan.
Every name in "What this PR builds on" is copied from the code. Before Task A, `grep -n`
each PR 9 and `fix/review-2` name against the branch; where the branch differs, fix this
plan first. Do not reconcile in code.

**Outcome:** a new `app_volume` action (kind 11) changes or mutes the volume of one app's
audio streams. **Chosen app** controls one app you pick (e.g. Spotify). **Focused app**
controls whichever app has the focused Hyprland window, and keeps controlling the last
focused app that had audio when you move to a silent window. On a dial, the strip panel
uses `$B1` with the app's icon, its name as the title, the level and a bar. When the app
has no stream, the panel is dimmed with `–`. Turn ±5%, capped at 100%. Press mutes the
app. Keys can run the action too. The configurator mirrors the panel from daemon events.

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

### The code at `55d0d62` (verbatim)

- **Config** (`config.rs`): `Action` is `#[serde(tag = "type", rename_all = "snake_case")]`
  with 11 variants, `None` … `Brightness { change: i8, set: Option<u8> }`.
  `Action::KINDS: [&'static str; 11]` (0 "Nothing" … 10 "Brightness"),
  `kind_index(&self) -> usize`, `empty_of_kind(i: usize) -> Self`,
  `default_for(kind: usize, dir: Direction) -> Self` (Left negates `change`, a Volume
  Press gets `change: 0, mute: true`), `summary(&self) -> String` (Volume through
  `volume_summary(device, change, mute)`), `is_deck_owned(&self) -> bool` (a `matches!`
  on Page, Back, PageStep, Brightness), `is_trackable` (`_ => false`). The helper
  `fn is_zero_i8(v: &i8) -> bool`. `pub enum Direction { Left, Right, Press }`.
- `pub enum LevelSource { Volume(AudioDevice), Brightness }`, `Copy + Eq`.
  `DialConfig::level_source(&self) -> Option<LevelSource>` checks turn_right, turn_left,
  press in that order: the first `Volume` wins, else `Brightness` if any.
  `DialConfig::volume_device` matches `LevelSource` exhaustively. `DialConfig::face(&self)
  -> render::Face<'_>` with `label: self.label.trim()`.
- **Audio** (`audio.rs`): `pub struct Level { pub percent: u8, pub muted: bool }`
  (`Debug, Clone, Copy, PartialEq, Eq`, no serde) with `to_panel(&self) -> render::Level`
  (value capped at 1.0, text `Muted` or `N%`). Private `fn capture(args: &[&str]) ->
  Option<String>` (sets `LC_ALL=C`, `proc::output_timeout(.., COMMAND_TIMEOUT)`, stdout on
  success only). `pub fn focused_monitor()` already parses `hyprctl monitors -j` with
  `serde_json::Value`.
- **Actions** (`action.rs`): `run(action: &Action) -> Result<()>` (an exhaustive `match`;
  deck-owned kinds refused first), `run_steps(action: &Action, steps: u8) -> Result<()>`
  (Volume folds into one run), `pub fn folded_volume(change: i8, steps: u8) -> i8`
  (clamped ±100). `run_volume` spawns a named thread around the blocking `apply_volume`.
- **Probes** (`probe.rs`): `Probe { Volume(AudioDevice), Command { command: String,
  interval_ms: u32 } }` (`Debug, Clone, PartialEq, Eq, Hash`), `Probe::interval()`
  (Volume 1 s), `Probe::refresh_after()`. `Value { Level(render::Level), Toggle(Toggle) }`
  (`Debug, Clone, PartialEq, Serialize, Deserialize`). `Values { probes: HashMap<Probe,
  Value>, brightness: u8 }` with `level(&self, device) -> Option<&Level>` (an exhaustive
  match on `Value`). `dial_status(dial: Option<&DialConfig>, values: &Values, pressed:
  bool) -> DialStatus`, `shows(dial: Option<&DialConfig>, probe: &Probe) -> bool`,
  `watched_probes(cfg, page, n_dials: usize) -> Vec<Probe>`. Note `probe.rs` imports
  `render::Level`; `audio::Level` is the other one (always write it `audio::Level`).
- **Render** (`render.rs`): `PanelStatus { level: Option<Level> }` (serde, each field
  `default` + `skip_serializing_if`), `DialStatus { pressed, panel: PanelStatus }`.
  `Renderer::render_strip_panel(&mut self, cfg, i: u8, panels: u32, st: DialStatus) ->
  RgbaImage`; inside it `let backdrop = self.panel_backdrop(cfg, i, panels);`, then
  `backdrop.img` is **moved** into `img`. `pub struct Backdrop { pub img: RgbaImage, pub
  tone: Rgb, pub has_image: bool }`, private `fn mean_rgb(img: &RgbaImage) -> Rgb`.
  Private `struct B1Colors { title: Rgb, value: Rgb, fill: Rgb, track: Rgb }`,
  `fn draw_bar(img, rect, value: f32, fill: Rgb, track: Rgb)` (already clamps 0–1, NaN
  empty; tested by `draw_bar_clamps_its_value`), `fn icon(&mut self, path: &Path, box_px:
  u32) -> Option<RgbaImage>` (cached in `self.icons`, warns once per miss),
  `fn glyph(&self, text, box_px, color)`, `fn brightness_icon(&mut self, face, color)`.
  Rects `X1_TITLE (16, 10, 136, 24)`, `B1_ICON (16, 40, 48, 48)`, `B1_VALUE (76, 40, 108,
  32)`, `B1_BAR (76, 74, 108, 12)`, `STRIP_ICON_PX = 48`, `PANEL_W = 200`, `PANEL_H = 100`.
  Test helpers in `render::tests`: `test_style()`, `renderer_with_font()`, `golden(img,
  name)`, `in_rect(x, y, rect)`, `volume_dial()`, `level(v, muted) -> DialStatus`
  (builds `PanelStatus { level: .. }` as a struct literal), `solid_img(w, h, c)`,
  `tmp_image(name, img)`. The fixture font covers U+0020–007E, `–` and `…` only; test
  text uses `A`.
- **Watcher** (`omadeckd/src/watch.rs`): `WatchCmd { Watch(Vec<Probe>), Refresh(Probe),
  Volume { device, change: i32, mute }, Leave { probe, from }, Done { probe, result },
  Stop }` (`Debug, Clone, PartialEq`). `Watcher { pub cmd: Sender<WatchCmd>, pub values:
  Receiver<(Probe, Value, i32)>, thread }`. `struct Poller` with `watched`, `last`,
  `next_due`, `forced: HashSet<Probe>`, `refresh_at`, `in_flight`, `fast_until`,
  `failing`, and the I/O as fn fields `read_volume: fn(AudioDevice) -> Option<Value>`,
  `change_volume: fn(AudioDevice, i32, bool)`; `Default` is written by hand.
  `Poller::handle(&mut self, cmd, now, out: &mut Vec<(Probe, Value, i32)>)`,
  `run_due(&mut self, now, out) -> Vec<(Probe, Arc<AtomicBool>)>` (Volume read inline,
  `Command` handed to runners), `record(&mut self, p, v, now, out, applied: i32)` (sends
  on change or when `forced`), `warn_once`. `fn coalesce(cmds: Vec<WatchCmd>) ->
  Vec<WatchCmd>` merges adjacent non-mute `Volume` of one device. Tests build a
  `Poller { read_volume: fake_read, change_volume: fake_change, ..Poller::default() }`.
- **Daemon** (`omadeckd/src/daemon.rs`): `Daemon { cfg, renderer, nav, wanted_page, hub,
  watcher, saver, rt: Runtime, deck, published, published_keys, in_flight, reload_at,
  repaint_at }`; `Runtime { values, state, flipped, settle, jobs }` (`Default`).
  `run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool` →
  `dispatch(&mut self, act, steps) -> Result<bool>`, whose `Action::Volume` arm sends
  `WatchCmd::Volume`, and whose last arm is `a => action::run_steps(a, steps).map(|_|
  false)`. `apply_value(&mut self, probe, value, applied: i32)` → `store_value` →
  `mark_probe` (dials through `probe::shows`). `flush` publishes each panel's
  `PanelStatus` that differs from `published`. `watch()` builds the set from
  `watched_probes` (and today matches `Probe` kinds for the volume total).
  `keep_toggle_state` (on reload) keeps only values whose probe is in
  `config_probes(cfg, n_dials)` (a private fn in `daemon.rs`, dials then every page's
  keys).
- **GUI**: `editor.rs` `ActionSlot { target: ActionTarget, dir: Option<Direction>, get,
  set, own_page, on_app_picked, on_new_page, on_favicon }`, `ActionSlot::dial(i, dir)`,
  `action_editor(app, group, combo_title, slot)`, `action_rows(app, group, slot)` (one
  arm per `Action`), `test_button(app: &Rc<App>, _slot: &ActionSlot, run: ActionRun) ->
  gtk::Button` (its tooltip and sensitivity come from a `match` on the action),
  `build_dial(app, i)` with two flat preset buttons ("Volume dial", "Brightness dial") in
  the header, and the key group's `presets_button(app, i) -> gtk::MenuButton`. `apps.rs`
  `find(id) -> Option<gio::AppInfo>`, `pick(parent, on_pick)`. `media::import_app_icon`.
  `App::action_at(&self, at) -> Option<Action>`, `set_action_at(self: &Rc<Self>, at, act)
  -> bool`, `edit_dial(i, f)`, `rebuild_editor()`, `toast(msg)`, `refresh_strip()`
  (mirrored `PanelStatus` per dial, else `probe::dial_status(cfg.dial(i), &Values {
  brightness, .. }, false)`). The async pattern is the favicon fetch in `action_rows`:
  `glib::spawn_future_local(async move { gio::spawn_blocking(move || ..).await })`.

### What `fix/review-2` changes (R3, R4, R5, R6, R8, R9)

Names as [review-fixes-2.md](review-fixes-2.md) "Names later plans use" gives them.

- **R3:** `Daemon::new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver:
  StateSaver) -> Daemon`. Nothing here calls it.
- **R4:** `Value` serialises `rename_all = "snake_case"`, so `Value::App` is `"app"`.
- **R5:** every `WatchCmd::Volume` is acknowledged once with the unclamped sum, read or
  no read. The watcher's output tuple is now `(Probe, Option<Value>, i32)` everywhere:
  `Watcher.values: mpsc::Receiver<(Probe, Option<Value>, i32)>`,
  `Poller::handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe,
  Option<Value>, i32)>)`, `Poller::run_due(&mut self, now: Instant, out: &mut
  Vec<(Probe, Option<Value>, i32)>) -> Vec<(Probe, u64, Arc<AtomicBool>)>`, and `record`
  takes the same `out` (its `v` is still a plain `Value`).
  `Daemon::apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32)`.
  `WatchCmd::Done { probe: Probe, run: u64, result: RunResult }`; nothing here touches
  it. `Daemon::watch()` no longer matches `Probe` kinds (its `kept`/`in_flight.retain`
  block is gone). PR 10 keeps no in-flight total for apps (no guess, see Decided), so
  `WatchCmd::AppVolume` needs no ack: its read-backs go out through `record(.., 0)`, like
  a poll.
- **R6:** `Daemon::dispatch(&mut self, src: Source, act: &Action, steps: u8) ->
  Result<bool>` takes the source; its Hyprland arm queues on `Daemon.hypr: HyprWorker`
  (`omadeckd/src/hypr_worker.rs`). `run_input`'s failure publish is
  `Daemon::report_failure(&self, src: Source, page: Option<String>, message: String)`.
- **R8:** a press recolours only the title of a `$B1` panel. After it, the `$B1` arm
  builds `let fg = self.label_color(Some(&face), style, tone); let title = if
  st.pressed { self.theme.accent } else { fg };` and `B1Colors { title, value: fg,
  fill: fg, track: tone.mix(fg, 0.22) }`. Task E splits `fg` per rect.
- **R9:** core feature `test-support` is declared in `crates/omadeck-core/Cargo.toml`;
  `omadeckd` and the GUI have `omadeck-core = { workspace = true, features =
  ["test-support"] }` under `[dev-dependencies]`. `watched_probes(cfg: &Config, page:
  Option<&str>, n_dials: usize, n_keys: u8) -> Vec<Probe>`. `App::action_at`/
  `set_action_at` have one arm per `ActionTarget` variant (PR 10 adds no `ActionTarget`,
  so they don't change). `action_rows` never calls `set_description`: each arm's
  explanation is a row, `group.add(&hint_row("…"))` (`fn hint_row(text: &str) ->
  adw::ActionRow` in `editor.rs`), and App volume's does the same.
- **The test `action::tests::run_refuses_exactly_the_deck_owned_kinds`** (at `55d0d62`)
  loops over `0..Action::KINDS.len()`, skips kind 8 (Volume) and calls
  `run(&Action::empty_of_kind(kind))`. Kind 11 would start an `app-volume` thread that
  runs `pactl`; Task C adds it to the skip.

### What PR 9 adds (from its plan)

- `hypr.rs`: `pub struct ActiveWindow { pub address: String, pub class: String, pub
  title: String, pub pid: Option<u32> }`, `pub enum HyprEvent { Workspace { id: i64,
  name: String }, ActiveWindow(Option<ActiveWindow>) }`, `pub fn start() ->
  Option<mpsc::Receiver<HyprEvent>>`. `omadeckd` gets `serde_json`.
- `Daemon.hypr_events: Option<mpsc::Receiver<hypr::HyprEvent>>` (the listener; `Daemon.hypr`
  is fix/review-2's `HyprWorker`), `Runtime.window: Option<hypr::ActiveWindow>`, and
  `Daemon::apply_hypr(&mut self)`, whose `ActiveWindow(w)` arm sets `rt.window = w`.
  `main.rs` `run()` each pass: drain notify → 1 s stamp check → `reload()` when due →
  `handle_commands()` → `apply_values()` → `apply_hypr()` → `poll_jobs()` →
  `poll_hypr()` → no deck: `flush()`, every 2 s `deck::open` → `attach`, else sleep
  `wait_for(now, next_deadline())`; deck: `pass()`, `Err` → `detach`. PR 10 adds no step.
  PR 9 puts `#[allow(dead_code)] // read by PR 10 and PR 15` on `Runtime.window`,
  `HyprEvent::Workspace`'s `id` and `ActiveWindow`'s fields.
- `PanelStatus.live: Option<LiveShow>` ("PR 10's `app` goes beside it");
  `DialConfig.live`, checked first in `level_source()` (a live dial has no level
  source). `probe::shows` starts with the live check. `draw_b1` gains a last parameter
  `bar: bool`; its live arm builds "exactly the level arm's `B1Colors`". More `Probe`
  and `Value` variants (`Probe::{Clock, Cpu, Memory, Battery, Workspace, Output {
  command, interval_ms }}`, `Value::{Text, Percent, Battery}`); `Probe::interval` and
  `refresh_after` stay exhaustive, so adding a probe is a compile error until handled.
- `watched_probes(cfg, page, n_dials, n_keys: u8)` (R9's key count) and
  `config_probes(cfg: &Config, n_dials: usize) -> Vec<Probe>`, both now adding `live`
  probes.

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

Add it after `Brightness`. In TOML:

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

- `KINDS` grows to `[&'static str; 12]`, `"App volume"` at **11**.
- `kind_index`: `Action::AppVolume { .. } => 11`.
- `empty_of_kind(11)` → `AppVolume { app: String::new(), focused: true, change: 5, mute:
  false }` (focused works without choosing anything).
- `default_for`: two arms next to Volume's. Press → `change: 0, mute: true`; Left →
  `change: -change`. All keep `app` and `focused`.
- `summary()`: an arm calling `fn app_volume_summary(app: &str, focused: bool, change: i8,
  mute: bool) -> String`. Strings: `Spotify volume +5%`, `Toggle mute (Spotify)`,
  `Focused app volume -5%`, `Toggle mute (focused app)`, and `App volume (no app set)`
  when `app` is blank and `focused` is off. The app part is `audio::display_app(app)`.
- `is_deck_owned()`: unchanged. `AppVolume` runs without the daemon too.
- `pub fn app_query(&self) -> Option<AppQuery>` on `Action`: `Focused` when `focused`,
  else `Named(app.trim().to_string())` when that isn't empty, else `None`. Every other
  action → `None`.
- `LevelSource` gains a unit variant `App` (still `Copy`). In `level_source()`, after PR
  9's `live` check, the `find_map` over turn_right, turn_left, press returns
  `Volume(device)` for a Volume action and `App` for an AppVolume whose `app_query()` is
  `Some`; the first one wins. Then Brightness as today. An AppVolume with no app set is
  skipped (the panel stays `$X1`).
- `pub fn app_query(&self) -> Option<AppQuery>` on `DialConfig`: the first
  `Action::app_query()` among turn_right, turn_left, press.
- `volume_device`: `LevelSource::Brightness | LevelSource::App => None`.
- Every `match` this breaks: run `cargo build --workspace` after adding the variant and
  fix each error. Expected: `kind_index`, `summary`, `action::run`, `volume_device`,
  `render_strip_panel`, `probe::dial_status`, the GUI's `action_rows`. Then
  `grep -rn 'LevelSource::' crates` for the `if let`/`matches!` uses the compiler doesn't
  flag (`watched_probes`, `config_probes`, `set_brightness`).

Old configs: no new fields on existing structs, so they load and save as before.

**Why a new action, not `Volume { target = "app" }`:** `Volume` is defined by its device
backends (Omarchy scripts, swayosd, the physical-sink lookup); app volume uses none of
them and needs `app` and `focused`, which would make meaningless combinations such as
`device = "input"` with `app = …`. Elgato splits them the same way (App vs Input/Output).

## Audio streams (`omadeck-core/src/audio.rs`)

**Read** with `pactl -f json list sink-inputs` (compact, what Omarchy's scripts use).
**Write** with `wpctl` on the PipeWire node id, so `-l 1.0` caps at 100% as the mic
volume does today (`pactl set-sink-input-volume` has no cap). No `pw-dump` fallback.

`Level` gains `serde::Serialize, serde::Deserialize` in its derives (it goes inside
`Value::App`).

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
    /// `application.name`, else `media.name`, else `node.name` ("" if none).
    pub name: String,
    /// `pipewire.access.portal.app_id`, else `application.id`, lowercased.
    pub app_id: Option<String>,
    /// `application.icon_name`.
    pub icon_name: Option<String>,
    /// The loudest channel's percent, plus mute.
    pub level: Level,
    /// Paused but present: still controllable.
    pub corked: bool,
}

/// What an app volume action or panel targets. The probe key.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppQuery {
    Named(String),
    Focused,
}

/// One app's streams added up, as the watcher reports them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct AppLevel {
    /// Display name. For a focused dial with no audio: the window's class ("" if none).
    pub name: String,
    /// What `select_named` finds the app by again: the first stream's binary, else its
    /// app id, else its lowercased name. "" when nothing matched. The sticky app.
    pub key: String,
    /// Filled by the watcher from `icon_names`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    /// Names to look the icon up by, in order. Never sent.
    #[serde(skip)]
    pub icon_names: Vec<String>,
    /// `percent` = max over streams, `muted` = all muted. `None`: no stream now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
    /// Every stream's node id.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ids: Vec<u32>,
}

/// The focused window, as matching needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub pid: Option<u32>,
    pub class: String,
}

pub fn streams() -> Option<Vec<Stream>>;                         // None: pactl missing or failed
pub fn parse_pactl_streams(json: &str) -> Option<Vec<Stream>>;
pub fn normalise_app(q: &str) -> String;                         // trim, lowercase, last `/` segment, strip ".desktop"
pub fn display_app(q: &str) -> String;                           // known names, else first letter upper-cased
pub fn select_named<'a>(s: &'a [Stream], name: &str) -> Vec<&'a Stream>;
pub fn select_window<'a>(s: &'a [Stream], w: &Window, parent: impl Fn(u32) -> Option<u32>) -> Vec<&'a Stream>;
pub fn app_level(matched: &[&Stream]) -> Option<AppLevel>;      // None for no match; icon left None
pub fn resolve_named(s: &[Stream], name: &str) -> AppLevel;      // no match: level None, name display_app(name)
/// What a focused dial controls: the window's app if it has a stream, else the sticky
/// app if it has one, else nothing (a `level: None` AppLevel named after the class).
/// Updates `sticky`.
pub fn resolve_focused(s: &[Stream], w: Option<&Window>, sticky: &mut Option<String>,
                       parent: impl Fn(u32) -> Option<u32>) -> AppLevel;
/// `resolve_named` or `resolve_focused`, by the query.
pub fn resolve(s: &[Stream], q: &AppQuery, w: Option<&Window>, sticky: &mut Option<String>,
               parent: impl Fn(u32) -> Option<u32>) -> AppLevel;
pub fn proc_parent(pid: u32) -> Option<u32>;                     // /proc/<pid>/stat, the 2nd field after ")"
pub fn app_icon_in(roots: &[PathBuf], names: &[String]) -> Option<PathBuf>;
pub fn app_icon(names: &[String]) -> Option<PathBuf>;            // the real roots, cached
pub fn app_volume_commands(ids: &[u32], change: i32, mute_to: Option<bool>) -> Vec<Vec<String>>;
/// Run each argv through `proc::output_timeout(.., COMMAND_TIMEOUT)`, in order, in its
/// own process group. Blocking: the watcher and `action::run`'s thread only.
pub fn run_commands(cmds: &[Vec<String>]);
/// `hyprctl -j activewindow`, for `action::run` outside the daemon.
pub fn focused_window() -> Option<Window>;
pub fn parse_hyprctl_window(json: &str) -> Option<Window>;       // `{}` or no "class" → None
```

- **`streams()`** is `capture(&["pactl", "-f", "json", "list", "sink-inputs"])` then
  `parse_pactl_streams`.
- **`parse_pactl_streams`**: `serde_json::Value`; not an array → `None`; `[]` →
  `Some(vec![])`. The `properties` values are strings (`"object.id": "87"`); read every
  number with `as_u64().or_else(|| as_str()?.parse().ok())` so either form works. A
  stream without a usable `object.id` is **skipped**, with `log::warn!` (it can't be
  written with wpctl). Percent: the max over `volume.*.value_percent` (`"86%"`), clamped
  to 255. `mute` and `corked` are JSON booleans. A missing optional key never fails the
  stream.
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
  generic (`audio-src`, starts with `ALSA plug-in`) or empty, else
  `display_app(binary)`. `icon_names` = the first stream's `icon_name`, `key`, `name`
  and `app_id`, skipping empty ones.
- **`resolve_named`** with no match: `AppLevel { name: display_app(name), key: "",
  icon: None, icon_names: vec![normalise_app(name)], level: None, ids: vec![] }`, so a
  stopped Spotify still shows Spotify's icon.
- **Sticky** (`resolve_focused`): a window match sets `*sticky = Some(lv.key)`; with no
  window match and `sticky` set, `resolve_named(s, sticky)` if that has a stream; else
  `AppLevel { name: class, key: "", icon: None, icon_names: vec![], level: None, ids:
  vec![] }` (`class` is `""` with no window). The sticky app is never cleared by a silent
  window, only replaced by the next window that has audio.
- **`display_app`**: a small table first (`spotify` → `Spotify`, `firefox` → `Firefox`,
  `chromium` → `Chromium`, `google-chrome` → `Chrome`, `brave` → `Brave`, `mpv` → `mpv`,
  `vlc` → `VLC`, `discord` → `Discord`, `com.spotify.client` → `Spotify`), else
  `normalise_app(q)` with its first letter upper-cased.
- **`app_icon_in`** (no GTK in the daemon): for each name, in order, look for
  `<name>.svg`, then `<name>.png`, under each root's
  `icons/hicolor/{scalable,256x256,128x128,64x64,48x48}/apps` and `pixmaps`. If none hits,
  read `Icon=` from `applications/<name>.desktop` under the same roots and look that up
  (an absolute `Icon=` path is used as is). **`app_icon`** builds the roots:
  `$XDG_DATA_HOME` (default `~/.local/share`), each `$XDG_DATA_DIRS` entry (default
  `/usr/local/share:/usr/share`), `~/.local/share/flatpak/exports/share` and
  `/var/lib/flatpak/exports/share`. It caches results, misses included, in a
  `OnceLock<Mutex<HashMap<Vec<String>, Option<PathBuf>>>>`. Watcher thread only.
- **`app_volume_commands`**: change → one `wpctl set-volume -l 1.0 <id> 5%+` (or `5%-`,
  `change.unsigned_abs()`, clamped to 100) per id, run in order, relative so tabs keep
  their balance. Mute → `wpctl set-mute <id> 1` (or `0`) for every id: one target state.
  The caller picks it: if any stream is unmuted, mute all, else unmute all (a `toggle`
  would flip a mixed set into another mixed set). `change == 0` with no mute → no
  commands.
- **`run_commands`** is the loop in `action::apply_volume`, moved into a function:
  `Command::new(&argv[0]).args(&argv[1..]).process_group(0)`, then `output_timeout`; a
  `None` logs a warning.
- **Test helper.** `#[cfg(any(test, feature = "test-support"))] pub fn test_stream(id:
  u32, pid: u32, binary: &str) -> Stream` (40%, unmuted, `name = binary`), so the
  daemon's tests can build streams (R9's feature).

## Probes and the panel (`probe.rs`, `render.rs`)

```rust
pub enum Probe { /* … */ AppVolume(AppQuery) }
pub enum Value { /* … */ App(AppLevel) }
```

- `Probe::interval()`: `AppVolume(_)` → 1 s. `refresh_after()`: `AppVolume(_)` → `&[]`
  (nothing sends `Refresh` for it; its own changes are read back at once).
- `Values::app(&self, q: &AppQuery) -> Option<&AppLevel>`, like `Values::level`.
- `Values::level` (and any other exhaustive `match` on `Value`): `Value::App(_) => None`.

`render.rs` gains what the panel draws, next to `Level` and PR 9's `LiveShow`:

```rust
/// The app on an app volume panel (PR 10).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AppShow {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dimmed: bool,
}
```

`PanelStatus` gains `#[serde(default, skip_serializing_if = "Option::is_none")] pub app:
Option<AppShow>`, so `Event::DialStatus { index, status }` carries it with no event
change. One pure helper in `probe.rs` decides what the panel shows, so the daemon, the GUI
(through the event) and `snapshot` agree:

```rust
/// The face and level of an app volume panel. `lv` is the watcher's value, `None`
/// before the first read.
pub fn app_show(dial: &DialConfig, q: &AppQuery, lv: Option<&AppLevel>) -> (AppShow, Option<Level>);
```

`label` below is `dial.label.trim()`; "label, else X" means X when the label is empty.

| Case | Title | Icon | Level | Dimmed |
|---|---|---|---|---|
| Named, playing | label, else `lv.name` | `lv.icon` | `lv.level.to_panel()` | no |
| Named, no stream (`lv.level` None) | label, else `lv.name` | `lv.icon` | None (`–`) | yes |
| Named, no value yet | label, else `display_app(app)` | None | None | yes |
| Focused, playing (focused or sticky) | `lv.name` (label ignored) | `lv.icon` | `lv.level.to_panel()` | no |
| Focused, no audio | `lv.name` if not empty, else label, else `Focused app` | None | None | yes |
| Focused, no value yet | label, else `Focused app` | None | None | yes |

`dial_status` gets a `LevelSource::App` arm:

```rust
Some(LevelSource::App) => {
    if let Some((d, q)) = dial.zip(dial.and_then(DialConfig::app_query)) {
        let (show, level) = app_show(d, &q, values.app(&q));
        panel.app = Some(show);
        panel.level = level;
    }
}
```

(Shape it to fit `dial_status` as PR 9 left it: build the `PanelStatus`, then fill
`level` and `app` per source.)

- `shows`: `(Some(LevelSource::App), Probe::AppVolume(q)) => dial.and_then(DialConfig::app_query).as_ref() == Some(q)`,
  so `apply_value` marks the panel dirty.
- `watched_probes`: the dial loop's `if let Some(LevelSource::Volume(dev))` becomes a
  `match`: `Volume(dev)` → `Probe::Volume(dev)`, `App` → `Probe::AppVolume(q)` with `q =
  dial.app_query()`, deduplicated as today. `config_probes` in `daemon.rs` gets the same
  change (Task G).

## Render (`render.rs`)

- **`Backdrop::tone_in`**:

```rust
impl Backdrop {
    /// The mean colour under `rect` (panel units), for text drawn there. `tone` stays the
    /// whole-panel mean.
    pub fn tone_in(&self, rect: (u32, u32, u32, u32)) -> Rgb {
        let (x, y, w, h) = rect;
        mean_rgb(&imageops::crop_imm(&self.img, x, y, w, h).to_image())
    }
}
```

- **Tones per rect.** In `render_strip_panel`, compute `title_tone =
  backdrop.tone_in(X1_TITLE)`, `value_tone = backdrop.tone_in(B1_VALUE)` and `bar_tone =
  backdrop.tone_in(B1_BAR)` **before** `backdrop.img` is moved into `img`. Every `$B1`
  panel (volume, brightness, PR 9's live, app) then builds its colours with one helper:

```rust
/// The `$B1` colours: the title against what's under the title, the value and bar
/// against what's under them. A press turns the title to the accent (R8); a dimmed
/// panel mixes both halfway into what's under them.
fn b1_colors(&self, face: &Face, style: &Style, tones: (Rgb, Rgb, Rgb), pressed: bool, dimmed: bool) -> B1Colors {
    let (title_tone, value_tone, bar_tone) = tones;
    let mut title = self.label_color(Some(face), style, title_tone);
    let mut value = self.label_color(Some(face), style, value_tone);
    if dimmed {
        title = title.mix(title_tone, 0.5);
        value = value.mix(value_tone, 0.5);
    }
    let title = if pressed { self.theme.accent } else { title };
    B1Colors { title, value, fill: value, track: bar_tone.mix(value, 0.22) }
}
```

  On a plain colour the three tones equal `tone`, so the goldens don't change.
- **The `LevelSource::App` arm** draws `$B1` through `draw_b1` (PR 9's `bar: true`) with
  a copy of the face whose `label` is the title: `let title = app.title.clone(); let face
  = Face { label: &title, ..face };` and `want_label = face.show_label &&
  !title.is_empty()`. `app` is `st.panel.app.clone()`, or, when it's `None` (a GUI with
  no daemon before `dial_status` ran), `AppShow { title: face.label.to_string(), icon:
  None, dimmed: true }`. Icon order: the user's `icon`, then `app.icon`
  (`self.icon(path, STRIP_ICON_PX)`), then the user's `glyph`, then `APP_GLYPH =
  "f08c6"` (nf-md application), in the glyph colour, as `brightness_icon` does. Write it
  as `fn app_icon(&mut self, face: &Face, app: &AppShow, color: Rgb) ->
  Option<RgbaImage>`. The app keeps its own icon when muted: which app it is matters
  more, and the text says `Muted`.
- **Dimmed:** colours from `b1_colors(.., dimmed: true)`, the icon's alpha halved with
  `fn fade(img: &mut RgbaImage, f: f32)` (multiplies each pixel's alpha by `f`), and the
  level `None`, so the value is `–` and the bar empty.
- **Above 100%:** sink inputs can be. `audio::Level::to_panel` caps the bar at 1.0 and the
  text still reads `150%`; `draw_bar` clamps as well. Nothing to add.

## Watcher (`omadeckd/src/watch.rs`)

- **One read serves every app probe.** All `Probe::AppVolume` probes share one schedule.
  When any is due, the watcher reads streams once and answers **every** watched
  `AppVolume` probe from that list, then sends only the changed ones. A read that
  returns `None` sends nothing and tries again in 1 s.
- New `Poller` fields (and their lines in the hand-written `Default`):

```rust
/// The focused window, from the daemon (PR 9's listener). Kept across `Watch`.
focus: Option<audio::Window>,
/// The last focused app that had audio (`AppLevel.key`).
sticky: Option<String>,
/// The I/O, swapped out in tests.
read_streams: fn() -> Option<Vec<audio::Stream>>,   // default audio::streams
run_commands: fn(&[Vec<String>]),                   // default audio::run_commands
```

- New commands:

```rust
/// The focused window changed (from PR 9's listener). Re-read now if a focused probe is
/// watched; otherwise just remember it.
Focus(Option<audio::Window>),
/// Change or mute an app's streams, in order with every other change, then re-read.
/// `change` already has the steps folded in (±100 at most); 0 with `mute`.
AppVolume { query: AppQuery, change: i32, mute: bool },
```

- A pure function does the per-read answer, so it's tested without I/O:

```rust
/// Answer every watched app probe from one stream list, icons included.
fn app_values(
    queries: &[AppQuery],
    streams: &[audio::Stream],
    focus: Option<&audio::Window>,
    sticky: &mut Option<String>,
    parent: &dyn Fn(u32) -> Option<u32>,
    icon: &dyn Fn(&[String]) -> Option<PathBuf>,
) -> Vec<(Probe, Value)>;
```

  Each query: `audio::resolve(streams, q, focus, sticky, parent)`, then `lv.icon =
  icon(&lv.icon_names)`, then `(Probe::AppVolume(q.clone()), Value::App(lv))`.
- `fn read_apps(&mut self, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>)` collects the
  watched app queries, calls `(self.read_streams)()`, and on `Some` records each answer
  of `app_values(.., &audio::proc_parent, &audio::app_icon)` with `record(p, v, now, out,
  0)`; on `None` it sets each app probe's `next_due` to `now + interval`.
- `run_due`: an `AppVolume(_)` arm in the loop. The first due one calls `read_apps` and
  sets a local `apps_read = true`; later ones in the same pass skip (their `next_due` was
  just reset by `record`).
- `handle(Focus(w))`: `self.focus = w;` and, if `Probe::AppVolume(AppQuery::Focused)` is
  watched, set its `next_due` to `now`. A burst of focus changes in one batch gives one
  read (the loop calls `run_due` once per batch).
- `handle(AppVolume { query, change, mute })`: read streams fresh (the ids may have
  changed since the last read, and a key's query may not be watched at all). `None` →
  warn once and stop. Resolve with `audio::resolve(&s, &query, self.focus.as_ref(), &mut
  self.sticky, audio::proc_parent)`. No ids → `log::info!("no app with audio for
  {query:?}")` and stop. Else `mute_to = mute.then(|| !lv.level.is_some_and(|l|
  l.muted))`, run `(self.run_commands)(&audio::app_volume_commands(&lv.ids, change,
  mute_to))`, put every watched app probe in `forced`, and `read_apps(now, out)` (sent
  even if unchanged, like a Volume read-back).
- `coalesce`: adjacent non-mute `AppVolume` of the same `query` merge, `change` summed and
  clamped ±100 (no ack depends on the sum: R5's rule is for `Volume` only).
- **No optimistic bar.** The change and the read run back to back on this thread, so the
  new level arrives in tens of ms. Each read-back reflects only finished changes and a
  spin only moves one way, so the bar never snaps back. The daemon keeps no in-flight
  total for apps.

## Daemon (`omadeckd/src/daemon.rs`)

- `dispatch(&mut self, src: Source, act: &Action, steps: u8)`, next to the
  `Action::Volume` arm, so every input path (keys, turns, presses, taps, later ctl and
  multi) gets it through `run_input`:

```rust
Action::AppVolume { change, mute, .. } => {
    let Some(query) = act.app_query() else { anyhow::bail!("no app set") };
    let change = if *mute { 0 } else { i32::from(action::folded_volume(*change, steps)) };
    // Runs on the watcher, after any earlier change, and is read back there.
    let _ = self.watcher.cmd.send(WatchCmd::AppVolume { query, change, mute: *mute });
    Ok(false)
}
```

- PR 9's `apply_hypr`, `HyprEvent::ActiveWindow(w)` arm: before `rt.window = w`, send
  `WatchCmd::Focus(w.as_ref().map(|w| audio::Window { pid: w.pid, class: w.class.clone()
  }))`. Always: the watcher only remembers it when nothing follows focus.
- `config_probes`: the dial loop gets the same `match` as `watched_probes`, so a reload
  keeps app values. Without it the value is dropped while the watcher (whose `last`
  survives the `Watch`) never resends it: the panel would stay dimmed until the level
  changes.
- `watch()`: unchanged (after R5 it no longer matches `Probe` kinds).
- Repaint and events: nothing else. `apply_value` → `mark_probe` → `probe::shows` marks
  the panel; `flush` publishes the `PanelStatus` (now with `app`) when it differs.
- The sticky app lives in the watcher, so it survives reloads and replugs.
- `snapshot`: `Values::default()`, so named dials show the dimmed "no value yet" face and
  focused dials `Focused app`.

## Outside the daemon (`omadeck-core/src/action.rs`)

`run` gets `Action::AppVolume { change, mute, .. } => run_app_volume(action, *change as
i32, *mute)`. `run_steps` folds first, like Volume: `if let Action::AppVolume { change,
mute, .. } = action { return run_app_volume(action, if *mute { 0 } else {
folded_volume(*change, steps) as i32 }, *mute); }`.

```rust
/// Change an app's volume on a helper thread (`omadeckd press`, "Try it now"). The
/// daemon runs `WatchCmd::AppVolume` on its watcher instead.
fn run_app_volume(action: &Action, change: i32, mute: bool) -> Result<()>;
```

It bails `"no app set"` when `action.app_query()` is `None`. Else one named thread
(`"app-volume"`) reads `audio::streams()`, resolves the query with a fresh `let mut sticky
= None` (no sticky app here) and, for `Focused`, `audio::focused_window()`, then runs
`audio::app_volume_commands` through `audio::run_commands`. No match → `log::info!` and
return (the panel already shows `–`). `apply_volume` calls `audio::run_commands` too, in
place of its own loop.

## GUI (`omadeck-gui`)

- `action_rows` gets an `Action::AppVolume` arm. Its explanation is a hint row, as the
  Volume arm's is (R9): `group.add(&hint_row("Changes one app's volume. On a dial, the
  strip shows the app's icon, name and level."));`. Never `group.set_description`.
  - **Follow the focused window** `SwitchRow`; on hides the app rows. Switch handler:
    read the action with `(slot.get)(&a)`, edit, `(slot.set)(&a, act)`,
    `a.rebuild_editor()`.
  - **Application** `ActionRow`: subtitle `audio::display_app(app)`, or "None chosen".
    **Choose…** opens `apps::pick_audio` (below). The pick writes `app` with
    `a.set_action_at(&at, ..)` (the slot's `ActionTarget`, captured before the dialog
    opens), and on a dial slot (`at.dial_slot()`) with an empty label also sets the
    label through `a.edit_dial(i, ..)`. Key labels are left alone.
  - **App** `EntryRow` for free text, writing through `slot.set` (no `rebuild_editor` in
    its `connect_changed`).
  - **Toggle mute** `SwitchRow`, and "Change per turn (%)" `SpinRow` −25…25 when mute is
    off, as in the Volume arm.
- `apps.rs`: `pub fn pick_audio(parent: &impl IsA<gtk::Widget>, on_pick: impl Fn(String,
  String) + 'static)` (the `key` and the display name). A dialog shaped like `pick`. A
  **Playing now** list is filled when one `audio::streams()` read, run with
  `gio::spawn_blocking` from `glib::spawn_future_local` as the dialog opens, comes back.
  It's deduped by `key`, each row with the `gio::AppInfo` icon when
  `find(&format!("{key}.desktop"))` hits. An **Installed apps…** row opens `pick` and
  stores the basename of `info.executable()`. The read is never repeated and never on
  the GTK thread.
- `build_dial`: the header's preset buttons move into a `gtk::MenuButton` "Presets" built
  by `fn dial_presets_button(app: &Rc<App>, i: u8) -> gtk::MenuButton`, the key group's
  `presets_button` pattern (a `gio::SimpleActionGroup`, one action per preset). Four
  presets: **Volume dial** and **Brightness dial** (moved, unchanged), **App volume
  dial** (opens `pick_audio`, then fills turn left/right ±5 and press mute with that
  `app`, label = the display name, `glyph = None`), and **Focused app dial** (all three
  with `focused = true`, label and glyph cleared).
- `test_button`: signature unchanged. One more arm in its tooltip `match`:
  `Action::AppVolume { focused: true, .. }` → insensitive, "Follows the focused window —
  here that would be the configurator". Chosen-app actions run through `action::run`
  (which spawns its thread, so the GTK thread doesn't wait).
- Strip preview: from the mirrored `PanelStatus`, now with `app`. No daemon → no status,
  so `dial_status` gives the "no value yet" face. The GUI never polls streams.

## Docs

README Features: "**App volume:** a dial or key for one app's volume (Spotify, a
browser…), or for whichever app is focused. The strip shows the app's icon, name and
level." The actions table row `app_volume` | `app`, `focused`, `change` (±%), `mute`; a
short example. Roadmap: add and tick "Per-app volume (chosen or focused app)".
`examples/config.toml`: a commented Spotify dial and a commented focused-app dial.
CHANGELOG under Unreleased/Added.

## Capture on the machine first

Save outputs under `crates/omadeck-core/tests/fixtures/` with the names given; Task A's
fixture test reads them with `include_str!("../tests/fixtures/…")`. Nothing for PR 10 has
been captured yet.

1. **Streams**, one file per case:
   `pactl -f json list sink-inputs > crates/omadeck-core/tests/fixtures/sink-inputs-<case>.json`
   for `spotify` (playing), `spotify-paused`, `firefox-two-tabs`, `chromium`,
   `mpv-in-terminal`, `flatpak` (if one is installed), `none` (nothing playing). Check in
   each: `properties."object.id"` present, string or number; pids string or number;
   the `volume.*.value_percent` form.
2. **Index vs node id:**
   `pactl -f json list sink-inputs | jq '.[] | {index, id: .properties."object.id", serial: .properties."object.serial"}'`.
3. **wpctl on a stream node:** `wpctl set-volume -l 1.0 <object.id> 5%+ && wpctl get-volume <object.id>`
   and `wpctl set-mute <object.id> 1 && wpctl get-volume <object.id>`; then back.
4. **Focused window:** `hyprctl -j activewindow > crates/omadeck-core/tests/fixtures/activewindow.json`
   with Firefox focused, and the same on an empty workspace (`activewindow-empty.json`).
   PR 9's capture 2 asks for the same JSON from the socket; if it's done, copy it. Then,
   with Chromium and with a terminal running `mpv`: `pstree -p <window pid>` and
   `ps -o pid,ppid,comm -p <stream pid>`.
5. **Icons:** `ls /usr/share/icons/hicolor/*/apps/ | grep -i -E 'spotify|firefox|chromium|mpv'`,
   `grep -h '^Icon=' /usr/share/applications/{spotify,firefox,chromium,mpv}*.desktop ~/.local/share/applications/*.desktop 2>/dev/null`.
6. **Versions:** `pactl --version; pipewire --version; head -3 /etc/os-release`, and the
   Omarchy version (`omarchy-version` if present).
7. **Glyph:** `printf '\U000F08C6\n'` in the Omarchy terminal shows an application window
   symbol.

## Tasks

~150–300 lines each including tests, in order; each ends with its check and a commit
(`cargo fmt` first). Known traps for every task:

- No `if let … = get(&a.config.borrow()) { edit(..) }` (BUGS.md 4): clone, drop the
  borrow, then edit.
- Nothing slow on the deck loop: every `pactl`/`wpctl`/`hyprctl` run is on the watcher, a
  helper thread or `gio::spawn_blocking`.
- Two `Level` types: `render::Level` (the panel) and `audio::Level` (a read). Name
  `audio::Level` in full outside `audio.rs`.
- Struct literals of `PanelStatus` (render tests' `level()`, `dial_status`) need `app:
  None` or `..Default::default()`.

### Task A — streams: parse and read (~200 lines)

Files: `audio.rs` (`Stream`, `AppQuery`, `AppLevel`, `Window`, the serde derives on
`Level`, `parse_pactl_streams`, `streams`, `test_stream`), the fixtures from Capture 1.
(`test-support` is already declared in core's `Cargo.toml` by fix/review-2.) Tests:
`pactl_streams_parse` (below), `pactl_garbage_is_none` (`"x"` → None, `"[]"` →
`Some(vec![])`), `pactl_skips_stream_without_object_id`, `pactl_fixtures_parse` (every
captured file parses; `firefox-two-tabs` gives two streams with the same `binary`;
`none` an empty list).

```rust
#[test]
fn pactl_streams_parse() {
    // Hand-written in the shape `pactl -f json list sink-inputs` gives; the captured
    // fixtures are checked in `pactl_fixtures_parse`. The pid is a number here, the id a
    // string: both forms must parse.
    let json = r#"[{"index":91,"corked":false,"mute":false,
        "volume":{"front-left":{"value":45875,"value_percent":"70%","db":"-9.29 dB"},
                  "front-right":{"value":52428,"value_percent":"80%","db":"-5.81 dB"}},
        "properties":{"object.id":"87","application.name":"Spotify",
                      "application.process.id":4242,"application.process.binary":"spotify",
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

Traps: `audio.rs`'s existing `mod tests` uses `super::*`; add the new tests there.
`include_str!` paths are relative to `src/audio.rs`.

**Check:** `cargo test -p omadeck-core audio::` → `test result: ok.`, 0 failed.

### Task B — matching, sticky focus, icons, commands (~300 lines)

Files: `audio.rs` (`normalise_app`, `display_app`, `select_named`, `select_window`,
`proc_parent`, `app_level`, `resolve_named`, `resolve_focused`, `resolve`,
`app_icon_in`/`app_icon`, `app_volume_commands`, `run_commands`, `focused_window`,
`parse_hyprctl_window`), `action.rs` (`apply_volume` calls `audio::run_commands`).
Tests: `select_named_forms` (`spotify`, `Spotify`, `spotify.desktop`,
`/usr/bin/spotify`, `com.spotify.Client` with `app_id` set hit their stream; `spot`
doesn't), `select_window_pid_and_children` (a `parent` closure over a `HashMap`; stops at
1 and at a cycle), `select_window_class_fallback`, `app_level_aggregates` (two streams:
max percent, muted only when both are, both ids), `display_name_rules` (`audio-src` →
the title-cased binary), `sticky_focus` (below), `app_icon_lookup` (a temp dir with
`icons/hicolor/scalable/apps/foo.svg` and `applications/bar.desktop` holding
`Icon=foo`; `bar` finds the svg), `app_volume_commands_change_and_mute`,
`hyprctl_window_parses` (the Capture 4 fixtures: Firefox gives a class and a pid, the
empty workspace `None`).

```rust
#[test]
fn sticky_focus() {
    let s = vec![test_stream(87, 100, "firefox")];
    let no_parent = |_| None;
    let mut sticky = None;
    // Firefox focused: it, and it becomes sticky.
    let w = Window { pid: Some(100), class: "firefox".into() };
    let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
    assert_eq!(lv.ids, vec![87]);
    assert_eq!(sticky.as_deref(), Some("firefox"));
    // A silent terminal focused: still Firefox.
    let w = Window { pid: Some(200), class: "Alacritty".into() };
    let lv = resolve_focused(&s, Some(&w), &mut sticky, no_parent);
    assert_eq!(lv.ids, vec![87]);
    // Firefox stops: the terminal's class, no level.
    let lv = resolve_focused(&[], Some(&w), &mut sticky, no_parent);
    assert_eq!((lv.name.as_str(), lv.level), ("Alacritty", None));
    assert_eq!(sticky.as_deref(), Some("firefox"), "kept until another app plays");
}
```

Traps: `app_icon_in` takes the roots so the test never reads `/usr/share`. Temp dirs
under `std::env::temp_dir()` with the pid in the name, as `render::tests::tmp_image`
does. The ancestor walk must stop on a cycle (a `HashSet` of seen pids).

**Check:** `cargo test -p omadeck-core audio::` → `test result: ok.`, 0 failed.

### Task C — config and the action outside the daemon (~220 lines)

Files: `config.rs` (the variant, `KINDS`, `kind_index`, `empty_of_kind`, `default_for`,
`summary` and `app_volume_summary`, both `app_query`s, `LevelSource::App`,
`level_source`, `volume_device`), `action.rs` (the `run` and `run_steps` arms,
`run_app_volume`, and the skip in `run_refuses_exactly_the_deck_owned_kinds`),
`lib.rs` (`pub use audio::{AppLevel, AppQuery};`), and compile fixes:
`dial_status` gets a `LevelSource::App => None` arm and `render_strip_panel`'s icon
`match` a `LevelSource::App => None` arm (`$B1`, no icon, a dash) until Tasks D and E; the GUI's
`action_rows` gets an empty `Action::AppVolume { .. } => {}` arm until Task H. Tests:
`app_volume_roundtrip` (below), the existing old-config tests (still pass),
`level_source_app` (a `Volume` turn_left plus an `AppVolume` turn_right: App wins, being
first in turn_right, turn_left, press order; an AppVolume with no app set is skipped),
`summary_app_volume` (the five strings), `kinds_has_app_volume_at_11`,
`default_for_app_volume` (Left negates, Press mutes with change 0).

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

In `action.rs`'s `run_refuses_exactly_the_deck_owned_kinds`, the skip becomes:

```rust
// Volume would really change the volume, and App volume would run pactl and
// wpctl; neither is deck-owned anyway.
if kind == 8 || kind == 11 {
    continue;
}
```

Traps: `deck_owned_kinds` loops over every kind and must still give `[6, 7, 9, 10]`.
`config.rs` → `audio::AppQuery` and `audio.rs` → `config::AudioDevice` is a cycle between
modules, which Rust allows.

**Check:** `cargo test -p omadeck-core config::` → `test result: ok.`, 0 failed;
`cargo build --workspace` clean.

### Task D — probes and the panel model (~200 lines)

Files: `probe.rs` (`Probe::AppVolume`, `Value::App`, `interval`/`refresh_after` arms,
`Values::app`, `app_show`, the `dial_status` arm, the `shows` case, `watched_probes`),
`render.rs` (`AppShow` and `PanelStatus.app` only; fix the struct literals `cargo build`
flags), `lib.rs` (`pub use render::AppShow;`), `ipc.rs` tests. Tests (in `probe.rs`):
`app_show_cases` (one assert per table row), `dial_status_app_named` (below),
`shows_app_matches_its_query`, `watched_probes_adds_app_dials` (a named dial and a
focused dial give `AppVolume(Named("spotify"))` and `AppVolume(Focused)`, each once);
in `ipc.rs`, `dial_status_event_carries_app` (`{"app":{"title":"Spotify"}}` inside
`status` round-trips, and a status without `app` writes no `app` key).

```rust
#[test]
fn dial_status_app_named() {
    let dial = DialConfig {
        turn_right: Action::AppVolume { app: "spotify".into(), focused: false, change: 5, mute: false },
        ..DialConfig::new(0)
    };
    let q = AppQuery::Named("spotify".into());
    let mut values = Values::default();
    // No value yet: dimmed, the app's display name, a dash.
    let st = dial_status(Some(&dial), &values, false).panel;
    assert_eq!(st.app, Some(AppShow { title: "Spotify".into(), icon: None, dimmed: true }));
    assert_eq!(st.level, None);
    // Playing: lit, its level.
    let lv = AppLevel {
        name: "Spotify".into(),
        key: "spotify".into(),
        icon: None,
        icon_names: vec![],
        level: Some(crate::audio::Level { percent: 40, muted: false }),
        ids: vec![87],
    };
    values.probes.insert(Probe::AppVolume(q), Value::App(lv));
    let st = dial_status(Some(&dial), &values, false).panel;
    assert!(!st.app.unwrap().dimmed);
    assert_eq!(st.level.unwrap().text, "40%");
}
```

Traps: `Value` derives `Serialize`, so `AppLevel` must too (Task A did). `AppShow` lives
in `render.rs` but `app_show` in `probe.rs`, next to `dial_status`.
`watched_probes(cfg, page, n_dials, n_keys)` keeps its signature; only the dial loop
changes, and a dial's app probe goes where its level probe went (before PR 9's `live`
probe for that dial). `shows`' new arm goes after PR 9's live check.

**Check:** `cargo test -p omadeck-core -- probe:: ipc::` → `test result: ok.`, 0 failed;
`cargo build --workspace` clean.

### Task E — render: `tone_in` and the app panel (~220 lines)

Files: `render.rs` (`tone_in`, the three tones, `b1_colors` used by every `$B1` arm,
`APP_GLYPH`, `app_icon`, `fade`, the `App` arm). Tests: `tone_in_follows_the_text_area`
(below), `app_panel_dimmed_fades_title` (title `A`: the title pixel furthest from the
background is closer to it than on the same panel undimmed), `app_panel_uses_its_title`
(a dial with no label and `AppShow { title: "AA", .. }` draws ink in `X1_TITLE`; with
`show_label = false` it doesn't), `pressed_app_panel_changes_only_the_title` (pressed and
unpressed are equal outside `X1_TITLE`), and the goldens unchanged.

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
    let b = Backdrop { tone: super::mean_rgb(&img), img, has_image: true };
    assert_eq!(b.tone_in(X1_TITLE), Rgb(255, 255, 255));
    assert_eq!(b.tone_in(B1_BAR), Rgb(0, 0, 0));
    // The whole-panel mean is mid-grey; that's what used to pick the title colour.
    assert_eq!(b.tone, Rgb(127, 127, 127));
}
```

Traps: the tones must be taken before `backdrop.img` moves into `img` (the compiler says
"borrow of moved value" otherwise). `render::tests` imports names one by one: add
`Backdrop`, `PANEL_W`, `PANEL_H`, `AppShow` to its `use super::{..}`. Don't touch the
golden PNGs; if a golden test fails, the tone or colour code is wrong.

**Check:** `cargo test -p omadeck-core render::` → `test result: ok.`, 0 failed (the
goldens included).

### Task F — watcher (~250 lines)

Files: `crates/omadeckd/src/watch.rs` (`WatchCmd::Focus`, `WatchCmd::AppVolume`, the
`Poller` fields and `Default`, `app_values`, `read_apps`, the `run_due` arm, the two
`handle` arms, `coalesce`). (fix/review-2 already gives `omadeckd` core with
`test-support` under `[dev-dependencies]`.) Tests: `one_read_answers_all_app_probes`
(below), `focused_without_window_is_dimmed_and_classless`, `app_change_runs_then_reads_back`
(a `Poller` with `read_streams` returning two Firefox tabs and `run_commands` recording
into a `static Mutex<Vec<Vec<String>>>`: two `wpctl set-volume` lines, then a forced
send), `focus_rereads_only_when_followed`,
`coalesce_merges_app_changes_of_one_query`.

```rust
#[test]
fn one_read_answers_all_app_probes() {
    use omadeck_core::audio::{AppQuery, Window, test_stream};
    let streams = vec![test_stream(87, 100, "spotify"), test_stream(90, 300, "firefox")];
    let focus = Window { pid: Some(300), class: "firefox".into() };
    let mut sticky = None;
    let qs = [AppQuery::Named("spotify".into()), AppQuery::Focused, AppQuery::Named("mpv".into())];
    let out = app_values(&qs, &streams, Some(&focus), &mut sticky, &|_| None, &|_| None);
    let ids = |q: &AppQuery| match out.iter().find(|(p, _)| *p == Probe::AppVolume(q.clone())) {
        Some((_, Value::App(lv))) => lv.ids.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(ids(&qs[0]), vec![87]);
    assert_eq!(ids(&qs[1]), vec![90]);
    assert!(ids(&qs[2]).is_empty(), "mpv isn't playing");
    assert_eq!(sticky.as_deref(), Some("firefox"));
}
```

Traps: the fn-pointer fields can't capture; the recording fake uses a `static`, and only
one test uses it (tests run in parallel, as the `FAKE` volume comment in this file says).
`test_stream` is under `cfg(any(test, feature = "test-support"))`, which `omadeckd`'s
dev-dependency turns on. Every expected `out` tuple wraps its value: `(probe,
Some(Value::App(..)), 0)`. Don't forget the new fields in `impl Default for
Poller`.

**Check:** `cargo test -p omadeckd watch::` → `test result: ok.`, 0 failed.

### Task G — daemon wiring (~150 lines)

Files: `crates/omadeckd/src/daemon.rs` (the `dispatch` arm, the `Focus` send in
`apply_hypr`, `config_probes`). Tests (in `daemon.rs`):
`config_probes_keeps_app_dials` (below), `app_volume_without_app_is_an_error` (a
`Daemon` isn't needed: test `Action::app_query()` returns `None` for `app = ""`,
`focused = false`, which is what `dispatch` bails on).

```rust
#[test]
fn config_probes_keeps_app_dials() {
    let cfg = Config::parse(
        r#"
[[dial]]
index = 0
[dial.turn_right]
type = "app_volume"
app = "spotify"
change = 5

[[dial]]
index = 1
[dial.turn_right]
type = "app_volume"
focused = true
change = 5
"#,
    )
    .unwrap();
    use omadeck_core::audio::AppQuery;
    let all = config_probes(&cfg, 4);
    assert!(all.contains(&Probe::AppVolume(AppQuery::Named("spotify".into()))));
    assert!(all.contains(&Probe::AppVolume(AppQuery::Focused)));
    // Only the dials the deck has.
    assert_eq!(config_probes(&cfg, 1), vec![Probe::AppVolume(AppQuery::Named("spotify".into()))]);
}
```

Traps: `apply_hypr` must send `Focus` for `None` too (nothing focused), so a focused dial
can fall back to its sticky app. `apply_hypr` now reads `ActiveWindow.pid` and `.class`:
drop PR 9's `#[allow(dead_code)]` from those two fields only (PR 15 reads the rest,
`Runtime.window` included). `dispatch` returns `Err` for no app, which `run_input`
hands to `report_failure` (it logs and publishes `ActionFailed`/`DialActionFailed`);
don't log it twice.

**Check:** `cargo test -p omadeckd` → `test result: ok.`, 0 failed;
`cargo clippy --all-targets -- -D warnings` clean. Then the named-dial and focused-dial
hardware checks.

### Task H — GUI (~250 lines)

Files: `editor.rs` (the `AppVolume` arm, `dial_presets_button`, `test_button`'s arm),
`apps.rs` (`pick_audio`). No test (GTK). Traps: BUGS.md 4 (read with `(slot.get)(&a)`,
then `(slot.set)`); the picker's callback writes through `a.set_action_at(&at, ..)` with
the `ActionTarget` captured before the dialog opens, not a captured index, because the
selection may change while it's open; recursive or shared closures are `Rc<dyn Fn…>`, not
`impl Fn + Clone`; no `rebuild_editor()` from an `EntryRow`'s `connect_changed` (focus is
lost); `gio::spawn_blocking` returns a `Result` (the favicon code shows the
`Ok`/`Err` handling).

**Check:** `cargo clippy --all-targets -- -D warnings` clean; by hand: both new presets
fill all three actions; the picker lists what's playing; the preview follows the deck
while the daemon runs; ▶ is off for a focused action.

### Task I — docs (~60 lines)

README, `examples/config.toml`, CHANGELOG. Trap: `config::tests::example_dial_parses`
counts the commented `# [[dial]]` blocks (3 at `55d0d62`, 4 after PR 9's battery dial).
Two new blocks → 6, and assert both new dials' (`c.dials[4]`, `c.dials[5]`)
`turn_right` is `Action::AppVolume`.

**Check:** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → all
green (`example_parses` and `example_dial_parses` included).

## Hardware checks

- [ ] Spotify dial: turn ±5% matches Omarchy's audio panel slider; a fast spin sends one
      change, never above 100%.
- [ ] Press mutes Spotify only (the system volume stays); the panel shows `Muted` within
      a moment.
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
- [ ] Holding the dial turns only the title to the accent colour.
- [ ] A strip image that's dark under the title and light under the value: each text
      picks its own contrast colour.
- [ ] The configurator's strip preview matches the deck.

## Decided (2026-10-17)

- **Decided: a new `AppVolume` action at kind 11,** not a `Volume` target.
- **Decided: `AppQuery::{Named, Focused}` is the probe key;** the watcher resolves focus
  and the sticky app itself, so what the panel shows and what a turn changes come from
  the same place and the same read.
- **Decided: changes run on the watcher thread** (`WatchCmd::AppVolume`) with a fresh
  stream read, in order with every other change, then a re-read. No optimistic bar and
  no in-flight total, so R5's ack doesn't apply.
- **Decided: all streams of an app move together;** the panel shows the loudest and reads
  muted only when all are. Change is relative per stream; mute sets one state for all.
- **Decided: `pactl` to read, `wpctl` to write;** streams without `object.id` are
  skipped. No `pw-dump` fallback.
- **Decided: pid first (with ancestors), class second.**
- **Decided: the focused window reaches the watcher as `audio::Window`,** converted from
  PR 9's `hypr::ActiveWindow` in `apply_hypr`, so `watch.rs` and core don't depend on
  `hypr.rs`.
- **Decided: an AppVolume with no app set gives no level source;** the panel stays `$X1`.
- **Decided: no pop-up** for app volume (Omarchy's OSD can't show a name with a bar;
  its speaker icon would read as the system volume).
- **Decided: the app's icon over the mute glyph** when muted.
- **Decided: icon lookup without GTK** in the daemon, cached, on the watcher thread.
- **Decided: focused is the default** for a new App volume action.
- **Decided: `tone_in` for every `$B1` panel's title, value and bar,** through one
  `b1_colors`; pixels on plain colours are unchanged. A press recolours only the title
  (R8).
- **Decided: the dial presets move into a Presets menu,** like the key group's, since
  four buttons don't fit the header.
- **Decided: a picked app fills an empty dial label;** key labels are left alone.
- **Decided: a focused-app dial on a silent window stays on the last app that had
  audio** ("sticky"). Clicking away from a video shouldn't stop the dial turning it down,
  and the panel names the app, so nothing hidden changes.
- **Decided: the GUI picker reads streams once, in the background, when it opens.** It
  doesn't poll, so the "only the daemon reads system state" rule is bent once per open;
  PR 11 may move it to a daemon query (Later).

## Ask before building

None; all settled 2026-10-17.

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
- **pactl to read, wpctl to write.** `wpctl -l 1.0` keeps the 100% cap the mic volume
  already has.
- **No pop-up.** The strip panel is the feedback; on a key there's none (Later).
- **Per-rect text contrast** (`tone_in`): a strip image can be dark under the title and
  light under the value.

## Later

- An app-named pop-up if Omarchy's OSD ever takes a message with a bar.
- Per-tab control (by `media.name`); input streams (a per-app mic level for Discord).
- Setting the volume of an app that isn't running (WirePlumber's stream-restore table).
- Elgato's Auto Detection: the running apps on the dials by themselves.
- Live key faces showing an app's level; a toggle key's on face following an app's mute.
- `pactl subscribe` instead of 1 s polling, if polling feels slow; a `pw-dump` fallback.
- The picker asks the daemon for the stream list (a PR 11 query) instead of its own
  one-off read.

**Size:** about 1,850 lines including tests over 9 tasks (A 200, B 300, C 220, D 200,
E 220, F 250, G 150, H 250, I 60).
