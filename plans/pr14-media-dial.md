# PR 14: `feat/media-dial`: media dial and media keys

Part of [PLAN.md](PLAN.md). Base: `feat/folders` (PR 13), the last branch in the chain
`fix/review-2` → PR 9 → … → PR 13. Nothing here uses PR 13's code.

**Outcome:** a `media` action for keys and dials: play/pause, next, previous, stop, and
seek ± N seconds. A **media dial** (turn = scrub, press = play/pause) shows the playing
track on its strip panel: title, artist, album art when it's a local file, elapsed / total
time, and a bar that moves while playing. A play/pause key can show a play or a pause face
that follows the real player (PR 8's on/off faces). Play/pause, next and previous go
through Omarchy's own media commands when they're there, so its pop-up shows as for the
keyboard's media keys. Seeking and reading the state go through `playerctl`. The
configurator gets a **Media dial** preset and a **Play/Pause** key preset, and its preview
mirrors the deck. Existing configs load and save byte-identical.

**Size:** about 1,500 lines over 7 tasks, tests included.

## How the Elgato app does it (researched 2026-10-16)

- **System › Multimedia** (keys): *Previous Track, Play / Pause, Next Track, Stop, Mute,
  Increase Volume, Decrease Volume*, each with its own icon.
  <https://help.elgato.com/hc/en-us/articles/360028234471-Elgato-Stream-Deck-System-Actions-Hotkey-Open-Website-Multimedia>
  Unconfirmed: whether the Play / Pause key follows the player or only flips on each press.
- **Elgato's Spotify plugin** (v2.5, by Elgato) has a *Multimedia* **dial** for the
  Stream Deck +: "Press to play or pause. Rotate to skip tracks or adjust volume." Its keys
  "update in real time to show playback status". On the Neo's info bar it shows "cover
  art, track name, and artist, with progress shown optionally". **No seek is documented.**
  Unconfirmed: what the dial's strip panel shows.
  <https://marketplace.elgato.com/product/spotify-17f49e8b-09a0-4209-95ed-6dee2469f97b>
- **Strip layouts:** six built-in (`$X1`, `$A0`, `$A1`, `$B1`, `$B2`, `$C1`), none for
  media. Plugins ship **custom layouts** of `text`, `bar`, `gbar` and `pixmap` items. Text
  has `text-overflow: clip | ellipsis | fade`. **No marquee.**
  <https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout>

## How Omarchy does it (checked in its repo 2026-10-16; confirm on the machine)

- **Omarchy 4** media keys call the shell's IPC (`media.playPause`, `media.next`,
  `media.previous`); from a shell that is `omarchy-shell media playPause|next|previous`.
  The shell picks the player and shows an OSD. `omarchy-shell media status` has title,
  artist and art **but no position, length or seek**. `playerctl` is **not** in Omarchy 4's
  base packages; Omarchy 3 shipped it, so an upgraded machine probably still has it.
- **Omarchy 3** media keys ran `swayosd-client --monitor <focused> --playerctl
  play-pause|next|previous`.
- `omarchy-shell` needs `OMARCHY_PATH` in its environment.

## Capture on the machine first

Nothing for PR 14 is captured yet. Do these on the Omarchy box (Stream Deck +
`0fd9:0084`, Omarchy 4.0.4) before Task 2, with Spotify **and** a browser tab (YouTube)
open. Paste each output into the named file or into this plan. Don't invent output: the
parser tests are only as good as these samples.

1. What's installed, and what `audio::backend()` will pick:
   ```bash
   which playerctl playerctld omarchy-shell omarchy-audio-output-volume swayosd-client
   playerctl --version
   pacman -Qo "$(which playerctl)"
   omarchy-version
   ```
2. The service's environment (does `omarchy-shell` work from `omadeckd`?):
   ```bash
   systemctl --user show-environment | grep -E '^(OMARCHY_PATH|PATH)='
   systemctl --user show omadeckd -p Environment
   ```
3. Omarchy's media command names and what they print:
   ```bash
   grep -rn 'media\.' ~/.local/share/omarchy/default/hypr/bindings/
   omarchy-shell media status; echo "exit $?"
   omarchy-shell media playPause; echo "exit $?"
   ```
4. **Fixture `crates/omadeck-core/tests/fixtures/playerctl-two.txt`**: Spotify paused,
   YouTube playing. The separator is the ASCII unit separator, `\x1f`:
   ```bash
   playerctl -a metadata --format $'{{playerInstance}}\x1f{{playerName}}\x1f{{status}}\x1f{{position}}\x1f{{mpris:length}}\x1f{{xesam:title}}\x1f{{xesam:artist}}\x1f{{mpris:artUrl}}' \
     > crates/omadeck-core/tests/fixtures/playerctl-two.txt; echo "exit $?"
   cat -v crates/omadeck-core/tests/fixtures/playerctl-two.txt
   ```
   Check: one line per player, eight fields, `position` and `length` in **microseconds**,
   Spotify's art an `https://` URL, the browser's a `file://` one (or empty).
5. **Fixture `playerctl-stream.txt`**: a live stream with no length (e.g.
   `mpv https://ice1.somafm.com/groovesalad-128-mp3`), same command. Expect an empty
   fifth field.
6. No player at all (close everything): the same command, and note the exit status and
   the stderr text (`No players found` expected):
   ```bash
   playerctl -a metadata --format '{{status}}'; echo "exit $?"
   ```
7. `playerctld` in the list (it's a proxy and must be skipped):
   ```bash
   pgrep -a playerctld; playerctl -l
   ```
8. How long a player takes to report a new status after a command (sets
   `MEDIA_REFRESH_DELAY`, default 600 ms). Do it for Spotify and for the browser:
   ```bash
   playerctl --player=spotify play-pause; for i in $(seq 10); do sleep 0.1; printf '%s ' "$(playerctl --player=spotify status)"; done; echo
   ```
9. Seek past the end, and a negative seek at the start:
   ```bash
   playerctl --player=spotify position 9999+; sleep 1; playerctl --player=spotify metadata xesam:title
   playerctl --player=spotify position 0; playerctl --player=spotify position 5-; playerctl --player=spotify position
   ```
10. The glyphs exist in the configured font (one line, six icons, none a box):
    ```bash
    printf '\U000f040a \U000f03e4 \U000f04ad \U000f04ae \U000f04db \U000f075a\n'
    ```
    (play, pause, next, previous, stop, music; Nerd Font MD.)

## Ask before building

None; all settled 2026-10-17.

## Decided

- **Decided: require `playerctl` for seeking and the panel.** Users without it get keys
  and play/pause through Omarchy, and a panel that says "Needs playerctl". A `busctl
  --user` backend that needs nothing installed is about 150 more lines; it's under Later.
- **Decided: album art from local `file://` only.** Spotify's `https://` art shows the
  play/pause glyph instead. Fetching would put an HTTP client in the daemon; `https://`
  art is under Later.
- **Decided: a media action wins the panel over volume.** A dial with a media press and a
  volume turn (PR 10's app volume on the player) shows the track: a dial that controls
  the player shows the player. Omarchy's own pop-up still shows the volume.
- **Decided: poll `playerctl` once a second, read inline on the watcher thread through
  `proc::output_timeout`.** One `playerctl -a metadata --format …` per second reads every
  player (one process, one line each), next to the inline volume read in
  `Poller::run_due`. No `playerctl --follow` child: the watcher would need a second thread
  to read its pipe, the child would have to be restarted when it dies and kept across
  reloads, and `--follow` doesn't print the position on every tick anyway, so the bar would
  still be moved locally. Not PR 8's runner threads either: those are for user shell
  commands that may take seconds; `playerctl` answers in a few ms, like `wpctl`, and
  `COMMAND_TIMEOUT` (2 s) caps a hung one. It runs only while a media dial or a play/pause
  toggle is on screen. Nothing runs on the deck loop. `--follow` is under Later.
- **Decided: a read that only confirms the prediction is not a change.** A `Track` carries
  `read_ms`, the wall-clock time its `position_ms` was true (Unix ms, so it can cross
  IPC). `Value::same_as(&self, newer)` compares players with `player::same`: same player,
  status, title, artist, length and art, and the position within **1.5 s** of
  `old.show().position_at(new.read_ms)`; every other value with `==`. Three places use it
  instead of `==`: the watcher's `Poller::record` (send only on change), PR 8's
  `Settle::accepts` (its `Expect` arm) and PR 8's equality skip in `Daemon::apply_value`.
  On "same" the old value stays (the anchor), so nothing is sent or repainted while a
  track simply plays. A seek from elsewhere, a pause or a track change is sent.
- **Decided: `dial_status` stays pure; painting advances the position.** `dial_status`
  puts the anchored `MediaShow` (`as_of_ms = read_ms`) in `PanelStatus.media`, so the
  `published` comparison in `Daemon::flush` only sees a change when the anchor changes,
  and `Event::DialStatus` goes out on a new value or a guess, never once a second.
  `deck::panel_status` (it has `now`) advances it with `MediaShow::at(now_ms)` before
  drawing; the GUI does the same in `refresh_strip`. A replayed event isn't stale, because
  `as_of_ms` is a wall-clock time.
- **Decided: the deck repaints a playing panel once per second, on a deadline.**
  `Daemon.media_due: Option<Instant>` is the next moment a playing panel's shown second
  changes; `next_deadline()` includes it. When it passes, the media panels are marked
  dirty; it is recomputed after each flush. One 200 × 100 write per second per playing
  panel, nothing while paused.
- **Decided: `PanelStatus` gains `media: Option<MediaShow>`** (next to PR 9's `live` and
  PR 10's `app`; left out on the wire when `None`). `Event::DialStatus { index, status }`
  is unchanged. `level` stays `None` for media panels.
- **Decided: media commands from the deck run on the watcher, in order, then re-read**
  (`WatchCmd::Media`), as volume does. `dispatch` sends it; nothing waits on the loop.
  The GUI's *Try it now* and `omadeckd press`/`dial` with no daemon use `action::run`,
  which runs the commands on a `media` thread, as `run_volume` does for volume.
- **Decided: one function builds the commands**, `player::media_argv(backend, cmd, seek,
  target, monitor) -> Vec<Vec<String>>`, and one runs each, `player::run_argv`. Callers:
  the watcher (`WatchCmd::Media`) and `action::run_media`. `seek` is the event's total in
  seconds, folded once by `player::fold_seek(seek, steps)` (clamped ±600).
- **Decided: reuse `audio::backend()`.** `Omarchy4` → `omarchy-shell media
  playPause|next|previous`; `Omarchy3` → `swayosd-client [--monitor M] --playerctl
  play-pause|next|previous` (the monitor exactly as `action::apply_volume` gets it:
  `audio::focused_monitor()` only for `Omarchy3`); `Plain` → `playerctl
  play-pause|next|previous`. Stop and seek are always `playerctl`. A **target** sends
  everything, play/pause included, to `playerctl --player=<instance> …`, because
  Omarchy's commands can't target a player.
- **Decided: the target is the shown player.** `probe::media_target(values, wanted)` is the
  instance of the track in `values.probes[&Probe::Player(wanted)]`, else `wanted`. With no
  `player` set and no track known, play/pause/next/previous go untargeted through Omarchy
  (its pop-up shows); a seek with nothing known to scrub does nothing.
- **Decided: guess, then let the real state confirm it**, as PR 8's fast toggles do. A seek
  moves the stored track's position, play/pause flips its status, both re-anchored at now,
  stored the way `after_toggle_press`'s fast arm stores a guess (insert into
  `rt.values.probes`, a `Settle::Expect { value, until: now + SETTLE }` in `rt.settle`,
  then `mark_probe`). `Probe::Player.refresh_after()` is `[MEDIA_REFRESH_DELAY, 1500 ms]`
  (600 ms; Capture 8 sets it): the first read usually confirms the guess, the second is
  outside the settle window, so a failed command shows the truth within 1.5 s.
  Next/previous/stop don't guess.
- **Decided: art is decoded on the watcher, never on the deck loop.** The watcher loads a
  new `art` path with `render::load_icon(path, 48)` into `Track::art_img:
  Option<Arc<RgbaImage>>`, reusing the previous image when the path is unchanged. A file
  that won't decode sets `art = None`. The GUI decodes the path once per track when an
  event brings a new one. The renderer only draws `art_img`; no new cache in `Renderer`.
- **Decided: `LevelSource::Player` is a unit variant**, so `LevelSource` stays `Copy`. The
  wanted player comes from the dial: `DialConfig::media_player() -> Option<&str>`.
- **Decided: `level_source()` checks for a `Media` action right after PR 9's `live`
  check** (a media action wins the panel), before PR 10's Volume/AppVolume step.
- **Decided: play/pause keys are PR 8 toggles.** `KeyConfig::state_probe()` maps a
  play/pause `Media` action to `Probe::Player(player)`, the way it maps a volume mute to
  `Probe::Volume`; `probe::key_status` reads a playing track as `Toggle::On`. A key
  without `[key.on]` keeps one static face. The configurator's *State from* combo gets a
  `StateChoice::Player` ("Follows the player") for such keys, as a mute key has
  `StateChoice::Mute`. The *Play/Pause* preset (in `PRESETS`) writes both faces: off =
  play glyph "Play", on = pause glyph "Pause". Faces show **what a press will do**, as
  every player UI does. No key rendering changes. Next and previous get no preset:
  they're one combo pick away, and `PRESETS` is for toggles.
- **Decided: no Omarchy `media status` fallback.** Without `playerctl` the panel reads
  "Needs playerctl" (`Value::Player(Err(..))`), and a seek or stop fails with
  "playerctl not found (install playerctl)", shown as a toast or `DialActionFailed`.
- **Decided: a custom layout on the `$B1` grid** (Elgato lets plugins define layouts):
  `$B1`'s title row, icon and bar, with the value row split into artist and time. The
  title is 168 wide, as nothing sits to its right.
- **Decided: a press recolours the title only** (R8, as every `$B1` panel after
  fix/review-2): artist, time, bar and the state glyph keep their colours.
- **Decided: idle and unknown panels keep the layout.** No daemon or no read yet
  (`media: None`): the dial's label, artist `–`, time `–`, empty bar. Nothing playing:
  the label, "Nothing playing", `–`, empty bar. No switch back to `$X1`.
- **Decided: kind 14 "Media"**, appended to `KINDS`, `KEY_KINDS` and `STEP_KINDS`
  (PR 12), so a multi-action step can be a media command. `is_deck_owned()` is false for
  it, so *Try it now* works.
- **Decided: PR 9's `old_config_saves_byte_identical` is the old-config test.** This PR
  adds a media dial to the example and that test still passes.

## Config

One new `Action` variant and one `LevelSource` variant. `DialConfig` and `KeyConfig` don't
change, so no existing config changes on load or save.

```rust
// config.rs

/// What a media action does to the player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommand {
    #[default]
    PlayPause,
    Next,
    Previous,
    Stop,
}

impl MediaCommand {
    pub fn is_play_pause(&self) -> bool {
        matches!(self, Self::PlayPause)
    }
}

// in `enum Action`, after PR 12's `Multi` and `Delay`:
    /// Control the media player (MPRIS), like the Elgato app's Multimedia actions.
    Media {
        /// Play/pause (default), next, previous or stop. Ignored when `seek` is set.
        #[serde(default, skip_serializing_if = "MediaCommand::is_play_pause")]
        command: MediaCommand,
        /// Seek by this many seconds, signed (per detent on a dial). Wins over `command`.
        #[serde(default, skip_serializing_if = "is_zero_i16")]
        seek: i16,
        /// Only this player (`playerctl --player` name, e.g. "spotify"). Default: the one
        /// playing.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        player: Option<String>,
    },

// next to `is_zero_i8`:
fn is_zero_i16(v: &i16) -> bool {
    *v == 0
}

// in `enum LevelSource`:
    /// The media player a `Media` action controls; which one is `DialConfig::media_player`.
    Player,
```

```toml
[[dial]]
index = 2
label = "Media"
[dial.turn_left]
type = "media"
seek = -5            # seconds per detent
[dial.turn_right]
type = "media"
seek = 5
[dial.press]
type = "media"       # play/pause

[[key]]
index = 7
label = "Play"
glyph = "f040a"
[key.action]
type = "media"       # play/pause; [key.on] makes it follow the player
[key.on]
label = "Pause"
glyph = "f03e4"

[[key]]
index = 8
label = "Next"
glyph = "f04ad"
[key.action]
type = "media"
command = "next"
player = "spotify"   # optional
```

## What this PR builds on

### Code as merged (`daily` = `feat/toggle-keys`, `55d0d62`), verbatim

Lines marked "fix/review-2" are as that branch leaves them (see the names at the end of
[review-fixes-2.md](review-fixes-2.md)); everything else is as merged.

Core `config.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Left, Right, Press }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelSource { Volume(AudioDevice), Brightness }
impl Action {
    pub fn is_deck_owned(&self) -> bool;     // Page | Back | PageStep | Brightness
    pub fn kind_index(&self) -> usize;
    pub fn empty_of_kind(i: usize) -> Self;
    pub fn default_for(kind: usize, dir: Direction) -> Self; // match (Self::empty_of_kind(kind), dir) { …, (act, _) => act }
    pub fn summary(&self) -> String;
}
fn is_zero_i8(v: &i8) -> bool;
impl DialConfig {
    pub fn new(index: u8) -> Self;
    pub fn volume_device(&self) -> Option<AudioDevice>;  // match on level_source()
    pub fn action(&self, dir: Direction) -> &Action;
    pub fn level_source(&self) -> Option<LevelSource>;   // let actions = [&self.turn_right, &self.turn_left, &self.press]; …
}
impl KeyConfig {
    pub fn state_probe(&self) -> Option<Probe>;  // on.state → Command; else match &self.action { Action::Volume { device, mute: true, .. } => Some(Probe::Volume(*device)), _ => None }
    pub fn state_choices(&self) -> &'static [StateChoice];
    pub fn state_choice(&self) -> StateChoice;
    pub fn set_state_choice(&mut self, c: StateChoice);
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateChoice { Press, Command, Mute }
```

Core `probe.rs`:

```rust
const VOLUME_REFRESH: [Duration; 1] = [Duration::from_millis(150)];
const COMMAND_REFRESH: [Duration; 2] = [Duration::from_millis(300), Duration::from_millis(1200)];
pub const SETTLE: Duration = Duration::from_secs(1);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Probe { Volume(AudioDevice), Command { command: String, interval_ms: u32 } }
impl Probe {
    pub fn interval(&self) -> Duration;
    pub fn refresh_after(&self) -> &'static [Duration];
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Value { Level(Level), Toggle(Toggle) }      // snake_case on the wire after R4
#[derive(Debug, Clone, Default)]
pub struct Values { pub probes: HashMap<Probe, Value>, pub brightness: u8 }
impl Values { pub fn level(&self, device: AudioDevice) -> Option<&Level>; }
pub fn dial_status(dial: Option<&DialConfig>, values: &Values, pressed: bool) -> DialStatus;
pub fn shows(dial: Option<&DialConfig>, probe: &Probe) -> bool;
pub fn key_status(key: Option<&KeyConfig>, values: &Values, pressed: bool) -> KeyStatus;
    // match values.probes.get(&p) { Some(Value::Toggle(t)) => t.clone(), Some(Value::Level(l)) if l.muted => Toggle::On, _ => Toggle::Off }
pub fn watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize, n_keys: u8) -> Vec<Probe>; // fix/review-2
```

Core `render.rs`:

```rust
#[derive(Debug, Clone, Default)]
pub struct DialStatus { pub pressed: bool, pub panel: PanelStatus }
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PanelStatus {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<Level>,
}
const X1_TITLE: (u32, u32, u32, u32) = (16, 10, 136, 24);
const X1_ICON: (u32, u32, u32, u32) = (76, 40, 48, 48);
const B1_ICON: (u32, u32, u32, u32) = (16, 40, 48, 48);
const B1_VALUE: (u32, u32, u32, u32) = (76, 40, 108, 32);
const B1_BAR: (u32, u32, u32, u32) = (76, 74, 108, 12);
const STRIP_ICON_PX: u32 = 48;
const STRIP_TITLE_PX: f32 = 16.0;
pub fn render_strip_panel(&mut self, cfg: &Config, i: u8, panels: u32, st: DialStatus) -> RgbaImage;
fn icon(&mut self, path: &Path, box_px: u32) -> Option<RgbaImage>;
fn glyph(&self, text: &str, box_px: u32, color: Rgb) -> Option<RgbaImage>;
fn draw_text_in(&self, img: &mut RgbaImage, rect: (u32, u32, u32, u32), px: f32, align: Align, text: &str, color: Rgb, shadow: bool);
fn draw_bar(img: &mut RgbaImage, rect: (u32, u32, u32, u32), value: f32, fill: Rgb, track: Rgb); // clamps
pub fn label_color(&self, face: Option<&Face>, style: &Style, bg: Rgb) -> Rgb;
pub struct Backdrop { pub img: RgbaImage, pub tone: Rgb, pub has_image: bool }
pub fn load_icon(path: &Path, box_px: u32) -> Option<RgbaImage>;
// tests: fn test_style() -> Style; fn renderer_with_font() -> Renderer (the bundled test font)
```

In `render_strip_panel`, `backdrop.img` is moved into `img` near the top, and the icon
colour is `let glyph_color = face.label_color.and_then(Rgb::parse).unwrap_or(if
style.follow_theme { self.theme.accent } else { … });`. The level panels sit in `if let
Some(src) = dial.and_then(|d| d.level_source()) { let icon = match src { … }; …;
self.draw_b1(..); return img; }`.

Core `action.rs` and `audio.rs`:

```rust
pub fn run(action: &Action) -> Result<()>;               // bails on is_deck_owned(); one arm per kind
pub fn run_steps(action: &Action, steps: u8) -> Result<()>; // folds Volume; Hyprland steps > 1 on a thread; else `run` × steps
fn run_volume(device: AudioDevice, change: i8, mute: bool) -> Result<()>; // thread "volume" → apply_volume
pub(crate) fn which(bin: &str) -> bool;
// audio.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend { Omarchy4, Omarchy3, Plain }
pub fn backend() -> Backend;
pub fn focused_monitor() -> Option<String>;
// proc.rs
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);
pub fn output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output>;
// presets.rs
pub struct Preset { pub name: &'static str, pub needs: Option<&'static str>, pub apply: fn(&mut KeyConfig) }
pub const PRESETS: &[Preset];   // test: every_preset_makes_a_toggle_that_roundtrips
```

Daemon `watch.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum WatchCmd { Watch(Vec<Probe>), Refresh(Probe), Volume { device: AudioDevice, change: i32, mute: bool },
                    Leave { probe: Probe, from: Toggle }, Done { probe: Probe, run: u64, result: RunResult }, Stop }
                    // fix/review-2: `Done` carries the run id; every `Volume` is acknowledged
struct Poller {
    watched: Vec<Probe>, last: HashMap<Probe, Value>, next_due: HashMap<Probe, Instant>,
    forced: HashSet<Probe>, refresh_at: Vec<(Instant, Probe)>,
    in_flight: HashMap<Probe, (u64, Arc<AtomicBool>)>, next_run: u64,   // fix/review-2
    fast_until: HashMap<Probe, (Toggle, Instant)>, failing: HashSet<Probe>,
    read_volume: fn(AudioDevice) -> Option<Value>, change_volume: fn(AudioDevice, i32, bool),
}
impl Poller {   // fix/review-2: `out` carries `Option<Value>` (`None`: a volume change acknowledged without a read)
    fn handle(&mut self, cmd: WatchCmd, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>);
    fn run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>) -> Vec<(Probe, u64, Arc<AtomicBool>)>;
    fn record(&mut self, p: Probe, v: Value, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>, applied: i32);
        // … if forced || self.last.get(&p) != Some(&v) { self.last.insert(..); out.push((p, Some(v), applied)); }
}
// Watcher.values: mpsc::Receiver<(Probe, Option<Value>, i32)> (fix/review-2)
```

`run_due`'s `match &p` has a `Probe::Volume(dev)` arm that reads inline and calls
`self.record(p, v, now, out, 0)`, and a `Probe::Command { .. }` arm that hands the probe to
a runner. `Poller` implements `Default` by hand (it sets `read_volume` and
`change_volume`).

Daemon `daemon.rs` and `deck.rs`:

```rust
enum Settle { Expect { value: Value, until: Instant }, Leaving { from: Toggle, until: Instant } }
impl Settle { fn accepts(&self, v: &Value, now: Instant) -> bool; }
    // Settle::Expect { value, until } => now >= *until || v == value,
pub struct Runtime { pub values: Values, pub state: State, pub flipped: HashSet<KeyId>, settle: HashMap<Probe, Settle>, pub jobs: HashMap<KeyId, Running> }
fn finish_job(rt: &mut Runtime, cfg: &Config, id: &KeyId, status: ExitStatus) -> JobEnd;
    // match k.state_probe() { Some(p @ Probe::Command { .. }) => Some(p), Some(Probe::Volume(_)) => None, None => { … } }
impl Daemon {
    fn watch(&mut self);   // fix/review-2 removed its `kept`/`in_flight.retain` block: no `match` on `Probe` left in it
    pub fn apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32);   // fix/review-2
        // settles `in_flight`, then `let Some(mut value) = value else { return };` …
        // … if self.rt.values.probes.get(&probe) == Some(&value) { return; } self.store_value(probe, value);
    fn mark_probe(&mut self, probe: &Probe);
    pub fn next_deadline(&self) -> Option<Instant>;
    pub fn pass(&mut self) -> Result<(), StreamDeckError>;
    fn after_toggle_press(&mut self, id: &KeyId, k: &KeyConfig, before: Toggle);
        // match k.state_probe() { None => flip, Some(Probe::Volume(_)) => {}, Some(p) if busy => Refresh,
        //                         Some(p) if k.is_slow_toggle() => …, Some(p) => fast guess }
    fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>;  // fix/review-2 adds `src`; … Action::Volume { .. } => { guess_volume; WatchCmd::Volume }, a => action::run_steps(a, steps).map(|_| false)
    fn guess_volume(&mut self, device: AudioDevice, change: i32, mute: bool);
    pub fn flush(&mut self);  // publish_keys; let Some(d) = &mut self.deck else { return }; flush_dirty; DialStatus by comparison with `published`
}
fn config_probes(cfg: &Config, n_dials: usize) -> Vec<Probe>;  // dials' Volume probes + every key's state_probe, any page
// deck.rs
pub fn panel_status(d: &Deck, cfg: &Config, values: &Values, i: u8, now: Instant) -> DialStatus;
    // probe::dial_status(cfg.dial(i), values, d.pressed(i, now))
impl Deck { pub fn dirty_where(&mut self, cfg: &Config, pred: impl Fn(Option<&DialConfig>) -> bool); }
```

GUI (`app.rs`, `editor.rs`):

```rust
// App fields
pub config: RefCell<Config>,
panel_status: RefCell<std::collections::HashMap<u8, PanelStatus>>,
// App::handle
Msg::Event(Event::DialStatus { index, status }) => {
    self.panel_status.borrow_mut().insert(index, status);
    self.refresh_strip();
}
pub fn refresh_strip(&self);   // mirrored.get(&i) → DialStatus { pressed: false, panel: panel.clone() }, else probe::dial_status(..)
pub fn edit_dial(self: &Rc<Self>, i: u8, f: impl FnOnce(&mut DialConfig));
pub fn rebuild_editor(self: &Rc<Self>);
// editor.rs
pub fn build_dial(app: &Rc<App>, i: u8) -> gtk::Widget;  // header presets: "Volume dial", "Brightness dial" buttons in `presets` (a gtk::Box)
fn action_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot);
pub(crate) struct ActionSlot { pub target, pub dir: Option<Direction>, pub get: Rc<dyn Fn(&Rc<App>) -> Action>, pub set: Rc<dyn Fn(&Rc<App>, Action)>, … }
// The "State from" combo in build_key maps each StateChoice to a label:
//   StateChoice::Press => "Flips on each press", StateChoice::Command => "Shell command", StateChoice::Mute => "Follows mute"
```

The `Action::Volume` arm of `action_rows` is the pattern for each row: read with `(get)(&a)`,
change one field, write with `(set)(&a, act)`.

### From the plans before this one

- **fix/review-2:** R4 (`Value` and every wire enum `rename_all = "snake_case"`); R5
  (the `watch.rs` and `apply_value` forms above); R6 (`dispatch` takes `src: Source`;
  Hyprland goes to `Daemon.hypr: HyprWorker`); R8 (`render_strip_panel` computes `fg`
  and a separate `title`, the accent while pressed: a `$B1` press recolours the title
  only); R9 (`watched_probes(cfg, page, n_dials, n_keys: u8)`; `action_rows` never calls
  `set_description`, it adds `hint_row(text) -> adw::ActionRow` to the group; the core
  feature `test-support` and `render::TEST_FONT`, which `renderer_with_font()` uses).
- **PR 9:** `PanelStatus.live: Option<LiveShow>`; `DialConfig::level_source()` returns
  `None` first thing when `live` is set; new `Probe`/`Value` variants (so some `match`es
  on them are already `_ =>`); `old_config_saves_byte_identical` in `config.rs`.
- **PR 10:** `PanelStatus.app: Option<AppShow>`; `LevelSource::App`;
  `DialConfig::app_query()`; `Backdrop::tone_in(&self, rect: (u32, u32, u32, u32)) -> Rgb`;
  `render_strip_panel` takes `title_tone = backdrop.tone_in(X1_TITLE)`, `value_tone =
  backdrop.tone_in(B1_VALUE)` and `bar_tone = backdrop.tone_in(B1_BAR)` before
  `backdrop.img` is moved, and every `$B1` panel gets its colours from `fn
  b1_colors(&self, face: &Face, style: &Style, tones: (Rgb, Rgb, Rgb), pressed: bool,
  dimmed: bool) -> B1Colors` (title to the accent when pressed, `fill: value`, `track:
  bar_tone.mix(value, 0.22)`). `config_probes`' dial loop matches `watched_probes`'.
- **PR 12:** `Action::KINDS: [&'static str; 14]` (12 Multi action, 13 Delay),
  `Action::KEY_KINDS`, `Action::STEP_KINDS`, `kind_position`, `ActionSlot.kinds`;
  `Action::Multi { steps }` and `Action::Delay { ms }`.

Check these on `feat/folders` before Task 1; if a name differs, the branch wins:

```bash
grep -n "KINDS\|KEY_KINDS\|STEP_KINDS\|fn tone_in\|pub live\|pub app\b\|LevelSource::App\|fn old_config_saves_byte_identical" crates/omadeck-core/src/*.rs
grep -n "pub fn watched_probes" crates/omadeck-core/src/probe.rs
grep -n "fn hint_row\|fn run_refuses_exactly_the_deck_owned_kinds" -A6 crates/omadeck-gui/src/editor.rs crates/omadeck-core/src/action.rs
```

## Tasks

Work them in order, one per session. Run `cargo fmt` before each check. Every task ends
green on `cargo clippy --all-targets -- -D warnings` too.

### Task 1: config (`config.rs`), ~170 lines

**Files:** `crates/omadeck-core/src/config.rs`, `crates/omadeck-core/src/lib.rs`, and the
compile fixes `cargo build --workspace` lists.

- `MediaCommand`, `Action::Media`, `is_zero_i16` exactly as under Config.
- `KINDS` grows to `[&'static str; 15]`: `"Media", // 14 (PR 14)`. Append `14` to
  `KEY_KINDS` and `STEP_KINDS`. `kind_index`: `Action::Media { .. } => 14`.
  `empty_of_kind`: `14 => Action::Media { command: MediaCommand::PlayPause, seek: 0,
  player: None }`.
- `Action::default_for`, two arms before the catch-all `(act, _) => act`:
  ```rust
  (Action::Media { player, .. }, Direction::Left) => Action::Media { command: MediaCommand::PlayPause, seek: -5, player },
  (Action::Media { player, .. }, Direction::Right) => Action::Media { command: MediaCommand::PlayPause, seek: 5, player },
  ```
  A press (and a key, through `empty_of_kind`) stays play/pause.
- `summary()`:
  ```rust
  Action::Media { command, seek, player } => {
      let what = if *seek != 0 {
          format!("Seek {}{} s", if *seek > 0 { "+" } else { "" }, seek)
      } else {
          match command {
              MediaCommand::PlayPause => "Play/Pause".into(),
              MediaCommand::Next => "Next track".into(),
              MediaCommand::Previous => "Previous track".into(),
              MediaCommand::Stop => "Stop".into(),
          }
      };
      match player {
          Some(p) if !p.trim().is_empty() => format!("{what} ({})", p.trim()),
          _ => what,
      }
  }
  ```
- `LevelSource::Player`. `level_source()` gets a step right after PR 9's `live` check (a
  dial with `live` still shows its live value), before PR 10's Volume/AppVolume step:
  ```rust
  if actions.iter().any(|a| matches!(a, Action::Media { .. })) {
      return Some(LevelSource::Player);
  }
  ```
  (`actions` is the `[&self.turn_right, &self.turn_left, &self.press]` list the method
  already builds; move its `let` above this check if PR 9's check sits before it.)
- New on `DialConfig`:
  ```rust
  /// The player a media dial follows: the first `Media` action's `player` among turn
  /// right, turn left and press, trimmed; `None` when blank or there's no media action.
  pub fn media_player(&self) -> Option<&str> {
      [&self.turn_right, &self.turn_left, &self.press]
          .into_iter()
          .find_map(|a| match a {
              Action::Media { player, .. } => Some(player.as_deref().map(str::trim).filter(|p| !p.is_empty())),
              _ => None,
          })
          .flatten()
  }
  ```
- Re-export `MediaCommand` from `lib.rs` (`pub use config::{…, MediaCommand, …}`).
- `action::run` needs an arm to compile: `Action::Media { .. } => bail!("media commands
  come in Task 3")` for now. Every exhaustive `match` on `LevelSource` the compiler lists
  (`DialConfig::volume_device`, `render_strip_panel`'s `match src`, `probe::dial_status`,
  the GUI's) gets a `LevelSource::Player => None` (or no-op) arm for now; Tasks 3 and 4
  fill them in. The GUI's `action_rows` gets an empty `Action::Media { .. } => {}` arm
  until Task 7.

**Test to copy** (in `config.rs`'s tests):

```rust
#[test]
fn media_roundtrip_and_defaults() {
    let src = r#"
[[dial]]
index = 2
label = "Media"
[dial.turn_left]
type = "media"
seek = -5
[dial.turn_right]
type = "media"
seek = 5
[dial.press]
type = "media"

[[key]]
index = 7
[key.action]
type = "media"
command = "next"
player = "spotify"
"#;
    let cfg = Config::parse(src).unwrap();
    let d = cfg.dial(2).unwrap();
    assert_eq!(d.turn_left, Action::Media { command: MediaCommand::PlayPause, seek: -5, player: None });
    assert_eq!(d.press, Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None });
    assert_eq!(d.level_source(), Some(LevelSource::Player));
    let k = cfg.key(None, 7).unwrap();
    assert_eq!(k.action, Action::Media { command: MediaCommand::Next, seek: 0, player: Some("spotify".into()) });
    // Defaults are left out: a bare play/pause saves as just its type.
    let out = toml::to_string_pretty(&cfg).unwrap();
    assert!(out.contains("[dial.press]\ntype = \"media\"\n"), "{out}");
    assert!(!out.contains("command = \"play_pause\""), "{out}");
    assert!(!out.contains("seek = 0"), "{out}");
    assert_eq!(Config::parse(&out).unwrap(), cfg);
}
```

Also write: `media_summary` (each command, `Seek +5 s`, `Seek -5 s`, ` (spotify)`),
`level_source_media_wins` (media press + volume turn → `Player`), `media_player_from_dial`
(`player = " spotify "` → `Some("spotify")`, blank → `None`), `media_kind_index_and_empty`
(`kind_index` of `empty_of_kind(14)` is 14, `KINDS[14] == "Media"`, `KEY_KINDS` and
`STEP_KINDS` end with 14).

**Check:** `cargo test -p omadeck-core config::tests` ends with `test result: ok.` and
`0 failed`, and lists the five new tests as `ok`: `media_roundtrip_and_defaults`,
`media_summary`, `level_source_media_wins`, `media_player_from_dial`,
`media_kind_index_and_empty` (and PR 9's `old_config_saves_byte_identical` still `ok`).
`cargo build --workspace` builds.

**Traps:** `seek` is `i16` (a hand-edited `seek = 200` must not fail the whole config the way
an `i8` would). Don't add `Copy`-breaking data to `LevelSource`.

### Task 2: the player model (`player.rs`, new), ~300 lines

**Files:** `crates/omadeck-core/src/player.rs` (new), `crates/omadeck-core/src/lib.rs`
(`pub mod player;`), `crates/omadeck-core/src/render.rs` (only the `MediaShow` struct and
its `impl`, no drawing yet), fixtures `playerctl-two.txt` and `playerctl-stream.txt` from
Capture 4–5.

Named `player.rs`, not `media.rs`: the GUI already has a `media.rs` (icon import).

```rust
// render.rs, next to `Level`. Serde, because it rides in `PanelStatus` on `Event::DialStatus`.
/// The media panel's display model. `position_ms` was true at `as_of_ms` (Unix ms).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MediaShow {
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub position_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub length_ms: Option<u64>,
    pub as_of_ms: u64,
    /// A local cover file, for the GUI to load. The deck uses `art_img`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art: Option<PathBuf>,
    /// The cover, decoded off the deck loop (48 × 48 box). Not sent over IPC.
    #[serde(skip)]
    pub art_img: Option<Arc<RgbaImage>>,
    /// Why there's no track, e.g. "Needs playerctl". Empty: nothing playing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

impl MediaShow {
    /// The position at `now_ms`: moves while playing, clamped to the length.
    pub fn position_at(&self, now_ms: u64) -> u64 {
        let p = if self.playing { self.position_ms + now_ms.saturating_sub(self.as_of_ms) } else { self.position_ms };
        self.length_ms.map_or(p, |l| p.min(l))
    }
    /// This panel advanced to `now_ms`.
    pub fn at(&self, now_ms: u64) -> MediaShow {
        MediaShow { position_ms: self.position_at(now_ms), as_of_ms: now_ms, ..self.clone() }
    }
    /// No track: nothing playing, or no way to read one.
    pub fn is_idle(&self) -> bool {
        self.title.is_empty() && self.artist.is_empty()
    }
}
```

Re-export `MediaShow` from `lib.rs` beside `PanelStatus`.

```rust
// player.rs
//! The media player (MPRIS through `playerctl`): reading it, and the commands that drive it.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;

use image::RgbaImage;
use serde::{Deserialize, Serialize};

use crate::audio::Backend;
use crate::config::MediaCommand;
use crate::render::MediaShow;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlayStatus { Playing, Paused, Stopped }

/// One player's state, as read by `playerctl`. Serde because `probe::Value` has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// The instance, for `--player` (e.g. `chromium.instance4821`).
    pub player: String,
    /// The player's name (e.g. `chromium`, `spotify`), for `wanted` and the `playerctld` skip.
    pub name: String,
    pub status: PlayStatus,
    pub position_ms: u64,
    /// `None` for a live stream: the bar is left empty and the time shows elapsed only.
    pub length_ms: Option<u64>,
    pub title: String,
    pub artist: String,
    pub art: Option<PathBuf>,
    /// Decoded by the watcher (Task 5); never serialised.
    #[serde(skip)]
    pub art_img: Option<Arc<RgbaImage>>,
    /// When `position_ms` was true (Unix ms).
    pub read_ms: u64,
}

/// What a read gives: a track, nothing playing, or why it couldn't read.
pub type PlayerRead = Result<Option<Track>, String>;

pub const SEP: char = '\x1f';
pub const FORMAT: &str = "{{playerInstance}}\x1f{{playerName}}\x1f{{status}}\x1f{{position}}\x1f{{mpris:length}}\x1f{{xesam:title}}\x1f{{xesam:artist}}\x1f{{mpris:artUrl}}";
/// A read within this of the predicted position is "the same" (no seek happened).
pub const DRIFT_MS: u64 = 1500;
/// The largest seek one event sends, either way.
pub const MAX_SEEK_S: i32 = 600;

pub fn now_ms() -> u64;                                          // SystemTime since UNIX_EPOCH, 0 on error
pub fn parse_playerctl(out: &str, read_ms: u64) -> Vec<Track>;   // skips malformed lines
pub fn pick(tracks: Vec<Track>, wanted: Option<&str>) -> Option<Track>;
pub fn art_path(url: &str) -> Option<PathBuf>;                   // file:// only, percent-decoded, must exist
pub fn same(old: Option<&Track>, new: Option<&Track>) -> bool;
pub fn fmt_time(ms: u64) -> String;                              // 0:07, 1:23, 12:05, 1:02:03
pub fn fold_seek(seek: i16, steps: u8) -> i32;                   // (seek * steps).clamp(±MAX_SEEK_S)
/// Whether this command needs `playerctl` (seek, stop, a target, or no Omarchy backend).
pub fn needs_playerctl(backend: Backend, cmd: MediaCommand, seek: i32, targeted: bool) -> bool;
pub fn media_argv(backend: Backend, cmd: MediaCommand, seek: i32, target: Option<&str>, monitor: Option<&str>) -> Vec<Vec<String>>;
/// Run one command and wait for it (bounded by COMMAND_TIMEOUT), output discarded.
/// Returns whether it exited 0. Blocking: the watcher and the `media` thread only.
pub fn run_argv(argv: &[String]) -> bool;
pub fn read(wanted: Option<&str>) -> PlayerRead;                 // runs playerctl via proc::output_timeout
impl Track { pub fn show(&self) -> MediaShow }                   // playing = status == Playing; as_of_ms = read_ms; art_img cloned (Arc)
```

- `parse_playerctl`: split each line on `SEP`; exactly 8 fields or skip the line. Status
  `Playing`/`Paused`/`Stopped`, else skip. Position and length are microseconds: divide by
  1000. A length that is empty or `0` is `None`. Title and artist trimmed. `art_path(f[7])`.
- `pick`: drop `name == "playerctld"`. With `wanted`, keep tracks whose `name == wanted`
  or whose `player` starts with `wanted`. Then the first `Playing`, else the first
  `Paused` with a non-empty title, else `None`.
- `art_path`: `url.strip_prefix("file://")`, percent-decode (`%20` → space; a bad escape
  gives `None`), then `Some(path)` only if `path.is_file()`. `https://`, `data:` and empty
  give `None`. The daemon never fetches from the network.
- `same`: `(None, None)` true; `(Some(a), Some(b))` true when `player`, `status`, `title`,
  `artist`, `length_ms` and `art` are equal **and** `b.position_ms.abs_diff(a.show()
  .position_at(b.read_ms)) <= DRIFT_MS`; anything else false.
- `needs_playerctl`: `seek != 0 || cmd == MediaCommand::Stop || targeted || backend ==
  Backend::Plain`.
- `media_argv`, with `pc(target)` = `["playerctl"]` plus `--player=<t>` when targeted:

| `seek` / `cmd` | Targeted | `Omarchy4` | `Omarchy3` | `Plain` |
|---|---|---|---|---|
| `seek > 0` | any | `pc position <s>+` | same | same |
| `seek < 0` | any | `pc position <s>-` (absolute value) | same | same |
| `Stop` | any | `pc stop` | same | same |
| `PlayPause` | no | `omarchy-shell media playPause` | `swayosd-client [--monitor M] --playerctl play-pause` | `playerctl play-pause` |
| `Next` | no | `omarchy-shell media next` | `… --playerctl next` | `playerctl next` |
| `Previous` | no | `omarchy-shell media previous` | `… --playerctl previous` | `playerctl previous` |
| any of the three | yes | `pc play-pause` / `pc next` / `pc previous` | same | same |

  A seek with `target == None` is `pc position …` untargeted (playerctl's own choice).
  `Omarchy3` with `monitor == None` leaves out `--monitor`. Replace the Omarchy 4 names
  with what Capture 3 shows.
- `run_argv`: `Command::new(&argv[0])`, `.args(&argv[1..])`, stdin null, through
  `crate::proc::output_timeout(&mut c, crate::proc::COMMAND_TIMEOUT)`. Exit 0 → `true`; a
  non-zero exit → `log::warn!("{argv:?} exited with {}", out.status)`, `false`; `None` →
  `log::warn!("{argv:?} could not run or timed out")`, `false`.
- `read(wanted)`: `Command::new("playerctl")` with `["-a", "metadata", "--format", FORMAT]`,
  `LC_ALL=C`, stdin null, through `proc::output_timeout(&mut cmd, COMMAND_TIMEOUT)`.
  `None` from it (not installed, or timed out): `Err("Needs playerctl".into())` when
  `!crate::action::which("playerctl")`, else `Ok(None)`. A non-zero exit (no players,
  Capture 6) is `Ok(None)`. Else `Ok(pick(parse_playerctl(&stdout, now_ms()), wanted))`.
  Art is **not** decoded here (the watcher does it, Task 5).

**Test to copy:**

```rust
#[test]
fn same_on_tick_not_on_seek() {
    let t = |pos: u64, read: u64, status| Track {
        player: "spotify".into(),
        name: "spotify".into(),
        status,
        position_ms: pos,
        length_ms: Some(200_000),
        title: "Song".into(),
        artist: "Band".into(),
        art: None,
        art_img: None,
        read_ms: read,
    };
    let old = t(10_000, 1_000_000, PlayStatus::Playing);
    // One second later, one second further: just playing.
    assert!(same(Some(&old), Some(&t(11_000, 1_001_000, PlayStatus::Playing))));
    // Within the drift allowance (players report coarse positions).
    assert!(same(Some(&old), Some(&t(12_400, 1_001_000, PlayStatus::Playing))));
    // A seek from the player's own UI.
    assert!(!same(Some(&old), Some(&t(60_000, 1_001_000, PlayStatus::Playing))));
    // Paused: the position must not move.
    let paused = t(10_000, 1_000_000, PlayStatus::Paused);
    assert!(same(Some(&paused), Some(&t(10_000, 1_005_000, PlayStatus::Paused))));
    assert!(!same(Some(&old), Some(&paused)));
    assert!(same(None, None));
    assert!(!same(Some(&old), None));
}
```

Also write: `parse_playerctl_two_players` (the Capture 4 fixture through `include_str!`:
two tracks, the right instances, positions in ms, Spotify's art `None`),
`parse_playerctl_stream_without_length` (Capture 5), `parse_playerctl_garbage` (`""`,
`"a\x1fb"`, a line with an unknown status → empty), `pick_prefers_playing`,
`pick_wanted_by_name_or_instance`, `pick_skips_playerctld`, `pick_none_when_stopped`,
`art_only_local_files` (write a temp file named `a b.png`; `file://…/a%20b.png` → that path,
`https://x/y.jpg`, `data:image/png;base64,AA`, a missing file and `file://%zz` → `None`),
`fmt_time_forms` (`7_000` → `0:07`, `83_000` → `1:23`, `725_000` → `12:05`, `3_723_000` →
`1:02:03`), `panel_moves_while_playing` and `panel_frozen_while_paused` and
`panel_clamps_to_length` (on `MediaShow::position_at`), `fold_seek_clamps` (`5 × 3 = 15`,
`-120 × 8 = -600`), `media_argv_rows` (every row of the table, `--player` when targeted,
`5+`/`5-`), `needs_playerctl_rows`.

Put the three `panel_*` tests in `player.rs`'s tests too, so one filter finds them all.

**Check:** `cargo test -p omadeck-core player::tests` shows `test result: ok. 16 passed; 0 failed`.

**Traps:** the fixtures contain `\x1f`; don't let an editor strip it (check with `cat -v`).
`{{position}}` is microseconds, not ms. `read` and `run_argv` must never call
`Command::output()` directly: a hung `playerctl` would stall every probe.

### Task 3: probe, panel status and the detached run (`probe.rs`, `render.rs`, `config.rs`, `action.rs`), ~220 lines

**Files:** `crates/omadeck-core/src/probe.rs`, `crates/omadeck-core/src/render.rs`
(`PanelStatus.media` only), `crates/omadeck-core/src/config.rs` (`state_probe`,
`StateChoice`), `crates/omadeck-core/src/action.rs`, and every `PanelStatus { .. }` literal
(`grep -rn "PanelStatus {" crates`).

```rust
// probe.rs
pub enum Probe { …, Player(Option<String>) }        // the wanted player; None = whichever plays
pub enum Value  { …, Player(PlayerRead) }

/// How long a player takes to report a new status after a command (Capture 8).
pub const MEDIA_REFRESH_DELAY: Duration = Duration::from_millis(600);
/// After a media command: the first read usually confirms the guess; the second lands
/// after `SETTLE`, so a failed command shows the truth.
const PLAYER_REFRESH: [Duration; 2] = [MEDIA_REFRESH_DELAY, Duration::from_millis(1500)];

impl Value {
    /// Whether `newer` says the same as `self`, for "send / store only on change".
    /// Players compare with `player::same` (a moving position is not a change); all
    /// other values compare with `==`.
    pub fn same_as(&self, newer: &Value) -> bool {
        match (self, newer) {
            (Value::Player(Ok(a)), Value::Player(Ok(b))) => player::same(a.as_ref(), b.as_ref()),
            (a, b) => a == b,
        }
    }
}

/// What a media dial's panel shows: the track the dial's player probe last read, a
/// "nothing playing" or a note, or `None` before the first read.
pub fn media_show(dial: Option<&DialConfig>, values: &Values) -> Option<MediaShow> {
    let wanted = Probe::Player(dial.and_then(DialConfig::media_player).map(str::to_string));
    match values.probes.get(&wanted) {
        Some(Value::Player(Ok(Some(t)))) => Some(t.show()),
        Some(Value::Player(Ok(None))) => Some(MediaShow::default()),
        Some(Value::Player(Err(note))) => Some(MediaShow { note: note.clone(), ..MediaShow::default() }),
        _ => None,
    }
}

/// The player a media action should go to: the one its panel or key shows, else `wanted`.
pub fn media_target(values: &Values, wanted: Option<&str>) -> Option<String>;
```

- `Probe::interval`: `Player(_)` → 1 s. `Probe::refresh_after`: `Player(_)` → `&PLAYER_REFRESH`.
- Any other exhaustive `match` on `Value` (`Values::level`, after PR 9 and 10 maybe
  already `_ => None`): `Value::Player(_) => None`.
- `render::PanelStatus` gains, after PR 10's `app`:
  ```rust
  /// The track on a media panel (PR 14); `None` before the first read or with no daemon.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub media: Option<MediaShow>,
  ```
- `dial_status`: for a dial whose `level_source()` is `Some(LevelSource::Player)`, `level`
  is `None` and `media` is `media_show(dial, values)`; for every other dial `media` is
  `None`. Fill in the Task 1 placeholder arm. `t.show()` is anchored at `read_ms`. **No
  clock here**: `dial_status` stays pure, so the daemon's `published` comparison only
  changes when the value does.
- `shows`: `(Some(LevelSource::Player), Probe::Player(w)) =>
  dial.and_then(DialConfig::media_player) == w.as_deref()`.
- `watched_probes`: each dial whose `level_source()` is `Player` adds
  `Probe::Player(dial.media_player().map(str::to_string))` (once each, in dial order, as
  for volume).
- `KeyConfig::state_probe`, in its `match &self.action`:
  `Action::Media { command: MediaCommand::PlayPause, seek: 0, player } =>
  Some(Probe::Player(player.clone()))`.
- `key_status`, in its value match: `Some(Value::Player(Ok(Some(t)))) if t.status ==
  PlayStatus::Playing => Toggle::On` (anything else falls through to `Off`).
- `StateChoice` gains `Player` ("Follows the player"). `state_choices`: a play/pause
  `Media` action (`command: PlayPause, seek: 0`) → `&[StateChoice::Player,
  StateChoice::Command]`. `state_choice`: `StateChoice::Player` for that action when
  `on.state` is `None`. `set_state_choice`: `Player` behaves like `Press` and `Mute`
  (clears `state` and `slow`).
- `action.rs`: `run` gets `Action::Media { command, seek, player } => run_media(*command,
  player::fold_seek(*seek, 1), player.as_deref())` (replacing Task 1's placeholder), and
  `run_steps` folds a spin into **one** seek, before the Hyprland branch:
  `if let Action::Media { command, seek, player } = action && *seek != 0 { return
  run_media(*command, player::fold_seek(*seek, steps), player.as_deref()); }`. Next and
  previous on a turn run once per detent through the existing loop.
  ```rust
  /// Run a media command without waiting (GUI *Try it now*, `omadeckd press`/`dial` with
  /// no daemon). The daemon sends `WatchCmd::Media` to its watcher instead.
  fn run_media(cmd: MediaCommand, seek: i32, target: Option<&str>) -> Result<()> {
      let backend = audio::backend();
      if player::needs_playerctl(backend, cmd, seek, target.is_some()) && !which("playerctl") {
          bail!("playerctl not found (install playerctl)");
      }
      let target = target.map(str::to_string);
      thread::Builder::new().name("media".into()).spawn(move || {
          let monitor = (backend == audio::Backend::Omarchy3).then(audio::focused_monitor).flatten();
          for argv in player::media_argv(backend, cmd, seek, target.as_deref(), monitor.as_deref()) {
              player::run_argv(&argv);
          }
      })?;
      Ok(())
  }
  ```
- `action.rs` tests, `run_refuses_exactly_the_deck_owned_kinds`: add `14` to the kinds it
  skips (8, Volume, and PR 10's 11, App volume). `run_media` really sends play/pause to
  the player, so the test must never run an empty `Media`.

**Test to copy** (in `probe.rs`):

```rust
#[test]
fn dial_status_shows_paused_track() {
    let dial = DialConfig {
        label: "Media".into(),
        press: Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None },
        ..DialConfig::new(0)
    };
    let track = player::Track {
        player: "spotify".into(),
        name: "spotify".into(),
        status: player::PlayStatus::Paused,
        position_ms: 83_000,
        length_ms: Some(225_000),
        title: "Song".into(),
        artist: "Band".into(),
        art: None,
        art_img: None,
        read_ms: 1_000,
    };
    let mut values = Values::default();
    values.probes.insert(Probe::Player(None), Value::Player(Ok(Some(track))));
    let st = dial_status(Some(&dial), &values, false);
    let m = st.panel.media.expect("a media panel");
    assert_eq!((m.title.as_str(), m.artist.as_str(), m.playing), ("Song", "Band", false));
    assert_eq!((m.position_ms, m.as_of_ms), (83_000, 1_000), "anchored at the read, not advanced");
    assert!(st.panel.level.is_none());
    assert!(shows(Some(&dial), &Probe::Player(None)));
    assert!(!shows(Some(&dial), &Probe::Player(Some("mpv".into()))));
    // No value yet: no media model, the renderer draws the dashes.
    assert!(dial_status(Some(&dial), &Values::default(), false).panel.media.is_none());
}
```

Also write, in `probe.rs`: `value_same_as_player_and_others` (a `Level` change is a change,
a playing tick is not), `media_target_prefers_shown_player`,
`watched_probes_include_media_dials`, `play_pause_key_follows_player` (a key with `[key.on]`
and a play/pause action: `state_probe() == Some(Probe::Player(None))`, `key_status` is On
for a playing track and Off for a paused one); in `config.rs`:
`play_pause_key_state_choice_is_player`; in `ipc.rs`: `dial_status_media_on_the_wire` (an
`Event::DialStatus` whose `status.media` is set round-trips without `art_img`, and
`PanelStatus::default()` still serialises without a `media` key).

**Check:** `cargo test -p omadeck-core -- probe:: ipc:: config:: action::` ends with
`0 failed` and lists the new tests as `ok` (and `run_refuses_exactly_the_deck_owned_kinds`,
with no player starting or pausing); `cargo build --workspace` builds (the daemon and the GUI
compile with the new field and variants; the GUI's `StateChoice` label `match` gets
`StateChoice::Player => "Follows the player"`).

**Traps:** `Value` derives `PartialEq`, and `Track` has `read_ms`, so `==` is almost never
true for players. Anything that asks "did it change?" must use `same_as`. No spawning in
tests: test `needs_playerctl` and `fold_seek`, never `run_media`.

### Task 4: render (`render.rs`), ~230 lines

**Files:** `crates/omadeck-core/src/render.rs`.

| Item | Rect (x, y, w, h) | Style |
|------|-------------------|-------|
| title | `M1_TITLE = (16, 10, 168, 24)` | `STRIP_TITLE_PX` (16), left, ellipsis, shadow; the track title, else the dial's label (when shown) |
| icon | `B1_ICON = (16, 40, 48, 48)` | `art_img`, else the user's icon or glyph, else the state glyph |
| artist | `M1_ARTIST = (76, 40, 108, 18)` | 14 px, left, ellipsis; its colour mixed 35% toward its tone |
| time | `M1_TIME = (76, 58, 108, 16)` | 13 px, right; `1:23 / 3:45`, `1:23` for a stream, `–` idle |
| progress | `B1_BAR = (76, 74, 108, 12)` | the `$B1` bar, `position / length`; empty for a stream or idle |

```rust
/// omadeck's own media layout on the `$B1` grid (Elgato has none; plugins define theirs).
const M1_TITLE: (u32, u32, u32, u32) = (16, 10, 168, 24);
const M1_ARTIST: (u32, u32, u32, u32) = (76, 40, 108, 18);
const M1_TIME: (u32, u32, u32, u32) = (76, 58, 108, 16);
const M1_ARTIST_PX: f32 = 14.0;
const M1_TIME_PX: f32 = 13.0;
const PLAY_GLYPH: &str = "f040a";
const PAUSE_GLYPH: &str = "f03e4";

/// The glyph for a media panel: what a press will do (pause while playing).
fn media_glyph(m: Option<&MediaShow>) -> &'static str;

/// The artist row and the time row for a media panel.
fn media_texts(m: Option<&MediaShow>) -> (String, String);
// None → ("–", "–"); idle with a note → (note, "–"); idle → ("Nothing playing", "–");
// a track → (artist, "1:23 / 3:45" or "1:23").

/// The colours of a media panel's rows, chosen by `render_strip_panel`.
#[derive(Debug, Clone, Copy)]
struct M1Colors { title: Rgb, artist: Rgb, time: Rgb, fill: Rgb, track: Rgb }

/// Draw the media layout. `art` wins over `icon` in the icon box.
fn draw_media(&self, img: &mut RgbaImage, face: &Face, colors: M1Colors, want_label: bool,
              art: Option<&RgbaImage>, icon: Option<RgbaImage>, media: Option<&MediaShow>);
```

- In `render_strip_panel`, before `backdrop.img` is moved into `img` (where PR 10 takes
  `title_tone`, `value_tone` and `bar_tone`), take the media panel's own tones, only for
  a media dial:
  ```rust
  let is_media = dial.and_then(DialConfig::level_source) == Some(LevelSource::Player);
  let media_tones = is_media.then(|| (backdrop.tone_in(M1_TITLE), backdrop.tone_in(M1_ARTIST), backdrop.tone_in(M1_TIME)));
  ```
- Then, before the `if let Some(src) = dial.and_then(|d| d.level_source())` block, a media
  block that ends in `return img;`. Colours, built like PR 10's `b1_colors` (R8: a press
  recolours the title only):
  - `title` = `self.theme.accent` while `st.pressed`, else `self.label_color(Some(&face),
    style, m_title_tone)`;
  - `artist` = `self.label_color(Some(&face), style, artist_tone).mix(artist_tone, 0.35)`;
  - `time` = `self.label_color(Some(&face), style, time_tone)`;
  - `fill` = `time` (as `b1_colors` uses its value colour), `track` =
    `bar_tone.mix(fill, 0.22)` with PR 10's `bar_tone`. **Paused:** `fill =
    track.mix(fill, 0.5)` (dimmed, not empty: empty means idle).
  - The state glyph uses the same unpressed glyph colour the `$B1` arm uses.
- The existing `match src` gets `LevelSource::Player => None` (drawn by the block above),
  replacing Task 1's placeholder.
- The bar: `draw_bar(img, B1_BAR, value, colors.fill, colors.track)` with `value =
  position_ms as f32 / length_ms as f32` for a track with a length, else 0 (`draw_bar`
  clamps).
- Icon order: `media.art_img` (drawn as given, centred in `B1_ICON`), else `face.icon`
  (`self.icon(&paths::resolve(p), STRIP_ICON_PX)`), else a non-blank `face.glyph`, else
  `media_glyph`.
- No clock here: the renderer draws `position_ms` as given (`deck::panel_status` and the
  GUI advance it, Tasks 6 and 7).

**Test to copy** (the bundled-font `renderer_with_font()` and `test_style()` in
`render.rs`'s tests, and glyph "A" so the test font draws it):

```rust
fn media_dial() -> DialConfig {
    DialConfig {
        label: "Media".into(),
        glyph: Some("A".into()),
        press: Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None },
        ..DialConfig::new(0)
    }
}

fn media(position_ms: u64, playing: bool) -> DialStatus {
    DialStatus {
        pressed: false,
        panel: PanelStatus {
            media: Some(MediaShow {
                title: "Song".into(),
                artist: "Band".into(),
                playing,
                position_ms,
                length_ms: Some(200_000),
                ..Default::default()
            }),
            ..Default::default()
        },
    }
}

#[test]
fn media_bar_fills_to_progress() {
    let mut r = renderer_with_font();
    let mut cfg = Config { style: test_style(), ..Default::default() };
    cfg.dials.push(media_dial());
    let (bx, by, bw, bh) = B1_BAR;
    let (x, y) = (bx + bw * 3 / 4, by + bh / 2);
    let start = r.render_strip_panel(&cfg, 0, 4, media(0, true));
    let end = r.render_strip_panel(&cfg, 0, 4, media(200_000, true));
    assert_ne!(start.get_pixel(x, y), end.get_pixel(x, y), "the fill should follow the position");
    // The title, artist and time rows have ink.
    let ink = |img: &RgbaImage, rect: (u32, u32, u32, u32)| {
        let bg = *img.get_pixel(5, 95);
        let (x0, y0, w, h) = rect;
        (x0..x0 + w).any(|x| (y0..y0 + h).any(|y| *img.get_pixel(x, y) != bg))
    };
    assert!(ink(&start, M1_ARTIST) && ink(&start, M1_TIME) && ink(&start, M1_TITLE));
}
```

(Add `MediaShow`, `PanelStatus`, `M1_ARTIST`, `M1_TIME`, `M1_TITLE` to the tests'
`use super::{..}`, and `MediaCommand` to the config import.)

Also write: `media_paused_dims_fill` (paused and playing differ in the fill colour at the
same position), `media_idle_keeps_layout` (`media: None` and `Some(default)` both draw the
bar's track and nothing in `X1_ICON`'s centre), `media_texts_forms` (pure: each row of the
`media_texts` comment), `media_glyph_follows_state`, `media_art_wins_over_glyph` (an
`art_img` of solid red shows red at the centre of `B1_ICON`), `media_press_recolours_title_only`
(pressed and unpressed panels are equal outside `M1_TITLE`).

**Check:** `cargo test -p omadeck-core render::tests::media` shows `test result: ok. 7
passed; 0 failed`; `CI=1 cargo test -p omadeck-core render` passes in full (the goldens
don't change).

**Traps:** the art image is drawn as given; don't resize it on every paint (it's already
48 × 48 at most). Don't add another image cache to `Renderer`. Take every tone before
`backdrop.img` is moved, or the borrow checker stops you.

### Task 5: the watcher (`watch.rs`), ~180 lines

**Files:** `crates/omadeckd/src/watch.rs`.

- `Poller` gains two I/O fields beside `read_volume`/`change_volume` (tests replace them),
  set in its `Default`:
  ```rust
  read_player: fn(Option<&str>) -> PlayerRead,   // player::read
  run_media: fn(&[String]) -> bool,              // player::run_argv
  ```
- **Reading**, in `Poller::run_due`'s `match &p`, next to the inline `Probe::Volume` arm:
  ```rust
  Probe::Player(wanted) => {
      let v = with_art(self.last.get(&p), Value::Player((self.read_player)(wanted.as_deref())));
      self.record(p, v, now, out, 0);
  }
  ```
  `player::read` goes through `proc::output_timeout`, so this is bounded by
  `COMMAND_TIMEOUT`. A `Probe::Player` never goes to the runner threads.
- **Art**, a free function in `watch.rs`:
  ```rust
  /// Decode a new cover off the deck loop; keep the old image when the path is unchanged.
  fn with_art(prev: Option<&Value>, mut v: Value) -> Value {
      if let Value::Player(Ok(Some(t))) = &mut v
          && let Some(path) = t.art.clone()
      {
          let old = match prev {
              Some(Value::Player(Ok(Some(p)))) if p.art.as_ref() == Some(&path) => p.art_img.clone(),
              _ => None,
          };
          t.art_img = old.or_else(|| render::load_icon(&path, 48).map(Arc::new));
          if t.art_img.is_none() {
              t.art = None; // won't decode: draw the glyph, and don't try again every second
          }
      }
      v
  }
  ```
- **Change test** in `Poller::record`: replace `self.last.get(&p) != Some(&v)` with
  `!self.last.get(&p).is_some_and(|old| old.same_as(&v))`. `last` is only written when a
  value is sent, so on "same" the old anchor stays. Don't move the `insert` above the test.
- **A new command:**
  ```rust
  /// Run a media command here, in order with the other changes, then re-read `probe`
  /// after `probe.refresh_after()` and send those reads even if unchanged.
  Media { command: MediaCommand, seek: i32, target: Option<String>, probe: Probe },
  ```
  In `Poller::handle`:
  ```rust
  WatchCmd::Media { command, seek, target, probe } => {
      let backend = audio::backend();
      let monitor = (backend == audio::Backend::Omarchy3).then(audio::focused_monitor).flatten();
      for argv in player::media_argv(backend, command, seek, target.as_deref(), monitor.as_deref()) {
          (self.run_media)(&argv);
      }
      if self.watched.contains(&probe) {
          for d in probe.refresh_after() {
              self.refresh_at.push((now + *d, probe.clone()));
          }
      }
  }
  ```

**Test to copy** (the pure part; the loop is covered by PR 8's `Poller` tests):

```rust
#[test]
fn with_art_keeps_decoded_image_for_same_path() {
    let dir = std::env::temp_dir().join(format!("omadeck-art-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("cover.png");
    image::RgbaImage::from_pixel(64, 64, image::Rgba([200, 30, 30, 255])).save(&path).unwrap();
    let track = |art: Option<std::path::PathBuf>| player::Track {
        player: "mpv".into(),
        name: "mpv".into(),
        status: player::PlayStatus::Playing,
        position_ms: 0,
        length_ms: None,
        title: "T".into(),
        artist: String::new(),
        art,
        art_img: None,
        read_ms: 0,
    };
    let first = with_art(None, Value::Player(Ok(Some(track(Some(path.clone()))))));
    let Value::Player(Ok(Some(t1))) = &first else { panic!() };
    let img = t1.art_img.clone().expect("decoded");
    assert!(img.width() <= 48 && img.height() <= 48);
    let second = with_art(Some(&first), Value::Player(Ok(Some(track(Some(path.clone()))))));
    let Value::Player(Ok(Some(t2))) = &second else { panic!() };
    assert!(Arc::ptr_eq(&img, t2.art_img.as_ref().unwrap()), "same path: no second decode");
    // A file that won't decode drops the art.
    std::fs::write(&path, b"not a png").unwrap();
    let bad = with_art(None, Value::Player(Ok(Some(track(Some(path.clone()))))));
    let Value::Player(Ok(Some(t3))) = &bad else { panic!() };
    assert!(t3.art.is_none() && t3.art_img.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
```

(`image` is a dependency of `omadeckd` already: `deck.rs` uses `image::DynamicImage`.)

Also write `record_ignores_a_playing_tick` with the existing `Poller` test pattern
(`Poller::default()`, `handle(WatchCmd::Watch(..))`, then `record(.., 0)`): record a
playing track, then the same track one second later and one second further (nothing
sent), then one 60 s further (sent). And `media_cmd_runs_then_refreshes`: a `Poller` with
`run_media` set to a function that pushes into a `thread_local!`
`RefCell<Vec<Vec<String>>>` and returns `true`; `handle(WatchCmd::Media { .. })` on a
watched `Probe::Player(None)` records one argv and two `refresh_at` entries.

**Check:** `cargo test -p omadeckd -- with_art record_ignores media_cmd` shows `test
result: ok. 3 passed; 0 failed`, and `cargo test -p omadeckd` passes in full. Then by
hand (any Linux box with `playerctl` and a player): `RUST_LOG=debug cargo run -p
omadeckd` with a media dial shows **one** `playerctl` run per second (`pgrep -c
playerctl` stays at 0 or 1), and no new value logged while a track just plays.

**Traps:** never decode art on the deck loop. `Poller::run_due` must not read a probe that's
no longer watched (the `Watch` set replaces it). Don't send `Value::Player` through PR 8's
runner path; `RunResult` has no place for it.

### Task 6: the daemon (`daemon.rs`, `deck.rs`), ~200 lines

**Files:** `crates/omadeckd/src/daemon.rs`, `crates/omadeckd/src/deck.rs`.

- **Dispatch**, next to the `Action::Volume` arm:
  ```rust
  Action::Media { command, seek, player } => {
      let probe = Probe::Player(player.clone());
      let target = probe::media_target(&self.rt.values, player.as_deref());
      let seek = player::fold_seek(*seek, steps);
      if player::needs_playerctl(audio::backend(), *command, seek, target.is_some()) && !omadeck_core::on_path("playerctl") {
          anyhow::bail!("playerctl not found (install playerctl)");
      }
      if seek != 0 && target.is_none() {
          return Ok(false); // nothing known to scrub
      }
      if seek != 0 || *command == MediaCommand::PlayPause {
          self.guess_media(&probe, *command, seek);
      }
      // Next/previous on a turn: once per detent (steps is already capped at 8).
      let times = if seek == 0 && matches!(command, MediaCommand::Next | MediaCommand::Previous) { steps } else { 1 };
      for _ in 0..times {
          let _ = self.watcher.cmd.send(WatchCmd::Media { command: *command, seek, target: target.clone(), probe: probe.clone() });
      }
      Ok(false)
  }
  ```
- **The guess**, next to `guess_volume`:
  ```rust
  /// Move the shown track at once (seek, or play/pause flipped), and hold the guess
  /// against stale reads for `SETTLE`, as PR 8's fast toggles do.
  fn guess_media(&mut self, probe: &Probe, cmd: MediaCommand, seek: i32) {
      // Clone out: everything below takes &mut self (BUGS.md 4).
      let Some(Value::Player(Ok(Some(mut t)))) = self.rt.values.probes.get(probe).cloned() else { return };
      guess_track(&mut t, cmd, seek, player::now_ms());
      let guess = Value::Player(Ok(Some(t)));
      // As after_toggle_press's fast arm: store directly, then settle, then mark.
      self.rt.values.probes.insert(probe.clone(), guess.clone());
      self.rt.settle.insert(probe.clone(), Settle::Expect { value: guess, until: Instant::now() + SETTLE });
      self.mark_probe(probe);
  }

  /// Pure, so it's tested without a deck: re-anchor at `now_ms`, then seek (clamped to
  /// the track) or flip play/pause.
  fn guess_track(t: &mut player::Track, cmd: MediaCommand, seek: i32, now_ms: u64);
  ```
  Don't route the guess through `apply_value`: its settle check would accept the guess
  (it is the same as itself) and remove the settle at once.
- **PR 8's code, the changes `same_as` needs:**
  - `Settle::accepts`' `Expect` arm becomes `now >= *until || value.same_as(v)`.
  - `apply_value`'s skip becomes `if self.rt.values.probes.get(&probe).is_some_and(|old|
    old.same_as(&value)) { return; }`.
  - `after_toggle_press` gets `Some(Probe::Player(_)) => {}` next to
    `Some(Probe::Volume(_)) => {}` (dispatch already guessed; without this arm the
    generic fast arm would store a `Value::Toggle` under a player probe).
  - Every other exhaustive `match` on `Probe` the compiler lists: `finish_job`'s
    `match k.state_probe()` → `Some(Probe::Player(_)) => None`. (`Daemon::watch` has
    none left after fix/review-2.)
- **`config_probes`:** a dial whose `level_source()` is `Some(LevelSource::Player)` adds
  `Probe::Player(dial.media_player().map(str::to_string))`, as it adds volume probes, so a
  reload keeps the track instead of blanking the panel.
- **Painting advances the position.** `deck::panel_status`:
  ```rust
  pub fn panel_status(d: &Deck, cfg: &Config, values: &Values, i: u8, now: Instant) -> DialStatus {
      let mut st = probe::dial_status(cfg.dial(i), values, d.pressed(i, now));
      if let Some(m) = &mut st.panel.media {
          *m = m.at(player::now_ms());
      }
      st
  }
  ```
  `Daemon::flush` keeps publishing from `probe::dial_status` (not `panel_status`), so the
  once-a-second repaints publish nothing.
- **Once-a-second repaint:** `Daemon` gains `media_due: Option<Instant>` (`None` in
  `Daemon::new`).
  ```rust
  /// When the shown second of a playing media panel next changes, if any is playing.
  fn next_media_tick(&self, now: Instant) -> Option<Instant>;
  ```
  It takes, over the dials `0..d.info.dials` (none without a deck), the minimum of `now +
  Duration::from_millis(ms_to_next_second(&m, player::now_ms())?)` for each
  `probe::dial_status(self.cfg.dial(i), &self.rt.values, false).panel.media` that is
  `Some`. In `pass`, after the `repaint_at` block: `if self.media_due.is_some_and(|t| now
  >= t)`, mark the media panels dirty (`d.dirty_where(&self.cfg, |dial|
  dial.and_then(DialConfig::level_source) == Some(LevelSource::Player))`). At the end of
  `flush`, after the `DialStatus` loop: `self.media_due =
  self.next_media_tick(Instant::now());`. `next_deadline()` adds `self.media_due` to its
  list.
- **Snapshot:** nothing to do: no player values, so `media: None` and the dashes.

**Test to copy:**

```rust
/// The wait until the shown second changes, for a playing panel at `now_ms`.
fn ms_to_next_second(show: &MediaShow, now_ms: u64) -> Option<u64> {
    show.playing.then(|| 1000 - show.position_at(now_ms) % 1000)
}

#[test]
fn media_tick_waits_for_the_next_second() {
    let p = MediaShow { playing: true, position_ms: 83_250, as_of_ms: 10_000, length_ms: Some(200_000), ..Default::default() };
    assert_eq!(ms_to_next_second(&p, 10_000), Some(750));
    assert_eq!(ms_to_next_second(&p, 10_750), Some(1000));
    let paused = MediaShow { playing: false, ..p.clone() };
    assert_eq!(ms_to_next_second(&paused, 10_000), None);
}
```

Also write `guess_track_seeks_and_flips` (playing at 10 s read 2 s ago: a +5 s seek gives
17 s anchored at now; −60 s gives 0; past the length gives the length; play/pause gives
`Paused` at the re-anchored position; `Next` changes nothing) and
`settle_expect_accepts_a_playing_tick` (a `Settle::Expect` holding a playing track
accepts the same track read 1 s later and 1 s further, inside the window).

**Check:** `cargo test -p omadeckd -- media_tick guess_track settle_expect` shows `test
result: ok. 4 passed; 0 failed` (with PR 8's `settle_expect_ignores_contradiction_until_expiry`);
`cargo clippy --all-targets -- -D warnings` is clean. On the deck: the bar moves once a
second while playing and stops when paused (Hardware checks).

**Traps:**
- Nothing here may wait on a process. `dispatch` only sends `WatchCmd::Media`;
  `on_path` is a few `stat` calls.
- The clock is read in `panel_status`, `guess_media` and `next_media_tick` only. Never
  in `dial_status`, or every pass publishes an event.
- BUGS.md 4: don't hold a borrow of `self.rt.values` across a `&mut self` call. Clone the
  value out, as `guess_media` does.

### Task 7: GUI, preset, docs and example, ~240 lines

**Files:** `crates/omadeck-gui/src/editor.rs`, `crates/omadeck-gui/src/app.rs`,
`crates/omadeck-core/src/presets.rs`, `crates/omadeck-core/src/config.rs` (the
`example_dial_parses` counts), `README.md`, `examples/config.toml`, `CHANGELOG.md`.

- **Action rows** (`action_rows`, the `Action::Media` arm from Task 1): a *Command* combo
  (Play/Pause, Next track, Previous track, Stop, Seek). With Seek, a *Seconds* spin
  (−60…60, step 1; 0 snaps back to the slot's default, 5 or −5 by `slot.dir`) and the
  action's `command` reset to play/pause. A *Player* entry (placeholder "Any player",
  subtitle "e.g. spotify; empty = the one playing"); blank saves as `None`. Every row
  reads with `(slot.get)(&a)` and writes with `(slot.set)(&a, act)`, as the Volume arm
  does; build the new action from a fresh `get`, never from one captured when the row
  was made. Its explanation is a hint row, as the Volume arm's is:
  `group.add(&hint_row("Seeking and the strip need playerctl."));` (never
  `group.set_description`).
- **Dial preset**, next to Volume dial and Brightness dial in `build_dial`'s header:
  ```rust
  let preset = gtk::Button::builder()
      .label("Media dial")
      .tooltip_text("Set this dial to the media player: turn to scrub, press to play or pause")
      .valign(gtk::Align::Center)
      .css_classes(["flat"])
      .build();
  let a = app.clone();
  preset.connect_clicked(move |_| {
      a.edit_dial(i, |d| {
          d.label = "Media".into();
          d.glyph = None; // the state glyph and the cover show
          d.turn_left = Action::Media { command: MediaCommand::PlayPause, seek: -5, player: None };
          d.turn_right = Action::Media { command: MediaCommand::PlayPause, seek: 5, player: None };
          d.press = Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None };
      });
      a.rebuild_editor();
  });
  presets.append(&preset);
  ```
- **Key preset** in `PRESETS`, so its menu and its round-trip test pick it up:
  ```rust
  Preset { name: "Play/Pause", needs: None, apply: play_pause }, // works through Omarchy's commands without playerctl

  fn play_pause(k: &mut KeyConfig) {
      k.label = "Play".into();
      k.glyph = Some("f040a".into());
      k.action = Action::Media { command: MediaCommand::PlayPause, seek: 0, player: None };
      k.on = Some(on_face("Pause", Some("f03e4"), None));
  }
  ```
- **Mirroring:** in `App::handle`'s `Event::DialStatus` arm, before the insert, give the
  new status its cover: if `status.media` has an `art` path, reuse the `art_img` of the
  stored status when its `art` is the same path, else load it once with
  `omadeck_core::render::load_icon(&path, 48).map(Arc::new)`. Clone the old image out and
  drop the `panel_status` borrow before loading (BUGS.md 4).
- **`refresh_strip`:** for a mirrored panel, advance its media before drawing:
  `let mut panel = panel.clone(); if let Some(m) = &mut panel.media { *m = m.at(player::now_ms()); }`.
- **Tick:** one `glib::timeout_add_local(Duration::from_secs(1), …)` started in
  `App::build`, holding a `Weak<App>`: it returns `glib::ControlFlow::Break` once the app
  is gone, and calls `refresh_strip()` only when some mirrored `PanelStatus` has
  `media.as_ref().is_some_and(|m| m.playing)`.
- **No daemon:** `dial_status` with empty `Values` gives `media: None` (dashes).
- **Docs:** README Features: "Media: play/pause, next, previous and seek from keys or a
  dial; the strip shows the track, artist, cover and a moving progress bar. Seeking and
  the strip need `playerctl` (`sudo pacman -S playerctl`)." Roadmap line: "value layouts
  on the strip: volume, brightness and media". `examples/config.toml`: a commented
  `# [[dial]]` media dial block, the play/pause key with `[key.on]` and the next-track
  key from Config, plus a commented variant with turn = the player's app volume (PR 10's
  `type = "app_volume"`, `app = "spotify"`). CHANGELOG line under Unreleased.
- **`example_dial_parses`** counts the commented `# [[dial]]` blocks (`assert_eq!(blocks.len(),
  N)` and `assert_eq!(c.dials.len(), N)`). The media block adds one: raise both counts by
  one and assert the new dial's `press` is `Action::Media { .. }`.

**Test to copy** (core; GUI code has no unit tests. Put it in `player.rs`'s tests):

```rust
#[test]
fn media_show_advances_from_as_of() {
    let m = MediaShow { playing: true, position_ms: 10_000, as_of_ms: 1_000, length_ms: Some(12_000), ..Default::default() };
    assert_eq!(m.at(3_000).position_ms, 12_000, "two seconds later, clamped to the length");
    assert_eq!(m.at(3_000).as_of_ms, 3_000);
    let paused = MediaShow { playing: false, ..m.clone() };
    assert_eq!(paused.at(9_000).position_ms, 10_000);
}
```

**Check:** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` is green,
including `presets::tests::every_preset_makes_a_toggle_that_roundtrips` (now with
Play/Pause), `config::tests::example_dial_parses` and
`config::tests::old_config_saves_byte_identical` with the new example. Then by hand: the
Media dial preset fills the dial; the preview shows dashes with the daemon stopped, and
the playing track (moving once a second) with it running; a play/pause key's *State from*
reads "Follows the player".

**Traps:** recursive editors take `Rc<dyn Fn…>`, not `impl Fn`. In the presets and the
event arm, never hold `app.config.borrow()` or `panel_status.borrow()` across
`edit_dial`, `edit` or `refresh_strip` (BUGS.md 4: clone, drop the borrow, then edit).
The tick must not run a process: the GUI only mirrors.

## Hardware checks

On the Stream Deck + (0fd9:0084), Omarchy 4.0.4, Spotify and a browser playing.

- [ ] Press = play/pause, and Omarchy's media pop-up shows, the same as the keyboard key.
- [ ] Turn = ±5 s per detent. A fast spin is one `playerctl position N+` (daemon log). No
      `playerctl` processes pile up (`pgrep -c playerctl` stays at 0–1).
- [ ] Seeking past the end goes to the next track (Capture 9), and the panel follows.
- [ ] The bar moves once a second while playing, stops when paused, jumps at once after a
      seek from the deck, and within ~1 s after a seek in the player's own UI.
- [ ] A track change updates title, artist and art within ~1 s.
- [ ] Art: the browser's `file://` art shows; Spotify's `https://` shows the glyph.
- [ ] Two players (Spotify paused, YouTube playing): the panel shows YouTube, and a seek
      goes to YouTube.
- [ ] `player = "spotify"` pins the dial to Spotify while YouTube plays.
- [ ] The play/pause key's face follows the keyboard's play/pause within ~1 s, and doesn't
      flicker after a press on the deck.
- [ ] Holding the media dial down turns only its title to the accent colour.
- [ ] Without `playerctl` (rename it off `PATH`): keys and press still work through
      `omarchy-shell`, the panel says "Needs playerctl", and a turn shows the "install
      playerctl" toast.
- [ ] The GUI preview mirrors the panel and moves once a second; with the daemon stopped it
      shows dashes.
- [ ] Idle CPU with a media dial: paused, and playing (one repaint a second), close to
      before (`top -p $(pgrep omadeckd)`).

## Decisions to explain

- **`playerctl` for seeking and state, Omarchy's commands for play/pause/next/previous.**
  Same pattern as volume: the deck's media buttons behave like the keyboard's (same player
  choice, same pop-up). Omarchy 4 has no seek and no position, so those need `playerctl`.
- **Polling once a second and moving the bar locally**, rather than `playerctl --follow`.
  The watcher's model: nothing to restart when a player quits, no child across reloads.
  Between reads the deck moves the bar itself, and a read that matches the prediction isn't
  even sent. A track change shows up to 1 s late, the same as volume.
- **The dial shows and controls the same player:** seeks and targeted commands carry the
  `--player` instance the panel shows. With two players, Omarchy's play/pause may pick
  another (it prefers the oldest playing); the panel follows within a second. Setting
  `player` avoids it.
- **Turn = seek by default**, not skip or volume as in Elgato's Spotify dial. The bar makes
  scrubbing visible, the deck already has a volume dial, and a dial's turn is just an
  action: `command = "next"`/`"previous"` gives Elgato's skip, PR 10's app volume gives
  its volume mode.
- **A media action wins the panel** over a volume action: a dial that controls the player
  shows the player.
- **A custom layout on the `$B1` grid** instead of plain `$B1` (no room for an artist) or
  `$C1` (two bars). Elgato lets plugins define layouts; this one keeps `$B1`'s positions.
- **Album art only from local files.** The daemon never fetches from the network.
- **Faces show the next action** (pause while playing). Elgato's behaviour is unconfirmed;
  this matches every player UI.
- **Ellipsis, no marquee:** the Elgato SDK has none either.
- **The play/pause key is a PR 8 toggle**, so its two faces are editable like any other
  toggle, and no second key-state mechanism exists.

## Later

- Event-driven updates (`playerctl --follow`, or a D-Bus `PropertiesChanged` listener) if
  the 1 s track-change lag bothers.
- A `busctl --user` backend for seek and position, so Omarchy 4 needs nothing extra.
- Album art from `https://`, fetched by the GUI or behind an opt-in, cached on disk.
- Marquee titles; a "now playing" key; shuffle/repeat keys, like Elgato's Spotify keys.
- Touch on the panel to seek to that spot of the bar; long press = next track (PR 16).
- Follow Omarchy's own player choice (`omarchy-shell media status`) when several play.
