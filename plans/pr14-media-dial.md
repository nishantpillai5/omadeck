# PR 14: `feat/media-dial`: media dial and media keys

Part of [PLAN.md](PLAN.md). Base: `feat/folders` (PR 13). Uses the **state watcher**
(PR 8) and the **strip layers** (PR 7, panels draw over them unchanged). Uses the
**audio streams** (PR 10) only as an example config (turn = the player's app volume); no
code depends on it. Assumptions about PR 8 and PR 10 are listed under "Assumptions".

**Outcome:** a `media` action for keys and dials: play/pause, next, previous, stop, and
seek ± N seconds. A **media dial** (turn = scrub, press = play/pause) shows the playing
track on its strip panel: title, artist, album art when it's a local file, elapsed / total
time and a progress bar that moves while playing. Media keys show a play or a pause face
that follows the real player state. Play/pause, next and previous go through Omarchy's own
media commands when they're installed (so its pop-up shows, as for the keyboard's media
keys); seeking and reading the state go through `playerctl`. The configurator gets a
**Media dial** preset and media key presets. Existing configs load and save byte-identical.

## How the Elgato app does it (researched 2026-10-16)

- **System › Multimedia** (keys) has these commands: *Previous Track, Play / Pause, Next
  Track, Stop, Mute, Increase Volume, Decrease Volume*. Each command has its own icon.
  <https://help.elgato.com/hc/en-us/articles/360028234471-Elgato-Stream-Deck-System-Actions-Hotkey-Open-Website-Multimedia>
  (the page returns 403 to scripts; the command list comes from its search snippet).
  Unconfirmed: whether the Play / Pause key changes its icon to follow the player, or only
  flips between two states on each press.
- **Elgato's Spotify plugin** (v2.x, by Elgato) has a *Multimedia* **dial** action for
  the Stream Deck +: "Press to play or pause. Rotate to skip tracks or adjust volume."
  Volume uses a user-set step. Its keys "update in real time to show playback status …
  and what's currently playing". On the Stream Deck Neo's info bar it shows "cover art,
  track name, and artist, with progress shown optionally".
  <https://www.elgato.com/us/en/explorer/products/marketplace/control-spotify-from-stream-deck/>,
  <https://marketplace.elgato.com/product/spotify-17f49e8b-09a0-4209-95ed-6dee2469f97b>
  (v2.5.0.86, 2026-09-22). **No seek/scrub is documented.** Unconfirmed: what the dial's
  strip panel shows (the page doesn't say), and what touch does on it.
- **Strip layouts:** the SDK has six built-in layouts (`$X1`, `$A0`, `$A1`, `$B1`, `$B2`,
  `$C1`). None is a media layout. Plugins can ship **custom layouts** made of `text`,
  `bar`, `gbar` and `pixmap` items. Text items have `text-overflow: clip | ellipsis |
  fade`. **There is no marquee (scrolling text).**
  <https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout>,
  <https://docs.elgato.com/streamdeck/sdk/references/manifest>
- Community plugins (e.g. [xmow49/stream-deck-spotify-plus](https://github.com/xmow49/stream-deck-spotify-plus))
  split it over two dials: one turns through tracks, the other sets the volume. Not a
  reference, only for comparison.

## How Omarchy does it today (checked 2026-10-16)

- **Omarchy 4.0.4** (`default/hypr/bindings/media.lua`): the media keys call the shell's
  IPC, `{ ipc = "media.playPause" }`, `media.next`, `media.previous`. The same calls work
  from a shell as `omarchy-shell media playPause|next|previous|status` (see
  `bin/omarchy-audio-source-switch`). The Quickshell media service
  (`shell/plugins/services/media/Service.qml`) picks the player (the oldest one playing,
  real players before proxies, else a preferred one, else one with a track) and shows an
  OSD. `media status` returns JSON: `playing`, `title`, `artist`, `album`, `artUrl`,
  `identity`, `canGoNext`, … **but no position or length, and there is no seek.**
- **`playerctl` is not in Omarchy 4's base packages** (`install/omarchy-base.packages`).
  `mpv` and `mpv-mpris` are. **Omarchy 3.x shipped `playerctl` and `spotify`**, and its
  media keys ran `omarchy-swayosd-client --playerctl play-pause|next|previous`. A machine
  upgraded from 3.x probably still has `playerctl`. Unconfirmed on this machine.
- `omarchy-shell` needs `OMARCHY_PATH` set. Unconfirmed: whether the systemd user service
  that runs `omadeckd` has it.

## Config

One new `Action` variant. `DialConfig` and `KeyConfig` don't change, so no existing
config changes on load or save.

```rust
/// What a media action does to the player.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaCommand { #[default] PlayPause, Next, Previous, Stop }

impl MediaCommand { pub fn is_play_pause(&self) -> bool { matches!(self, Self::PlayPause) } }

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
```

The same shape as `Volume` (`change` vs `mute`) and `Brightness` (`set` wins over
`change`): a relative value on turns and a command on press. A bare `type = "media"`
toggles play/pause.

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
label = "Next"
[key.action]
type = "media"
command = "next"
player = "spotify"   # optional
```

- `KINDS` gets "Media" at the next free index (**13**: App volume 9, Multi action 10, Delay 11,
  Folder 12; use `KINDS.len()` at the time). Add the matching `kind_index`, and
  `empty_of_kind` → `Media { command: PlayPause, seek: 0, player: None }`.
- `summary()`: "Play/Pause", "Next track", "Previous track", "Stop", "Seek +5 s" /
  "Seek -5 s", with " (spotify)" when `player` is set.
- `LevelSource` gains `Player(Option<String>)`. This drops `Copy` (keep `Clone, PartialEq,
  Eq`); the few `*src == Some(..)` comparisons in the daemon still compile, and the
  render match binds by value. `level_source()` checks for a `Media` action **first**: a
  dial that controls the player shows the player, even when its turn is a volume action
  (see Decisions).
- `is_zero_i16` helper next to `is_zero_i8`.

## `omadeck-core/src/player.rs` (new)

Named `player.rs`, not `media.rs`, because the GUI already has a `media.rs` (icon import).

- `pub enum PlayStatus { Playing, Paused, Stopped }`
- `pub struct PlayerState { name: String /* instance, for --player */, status, position_ms:
  u64, length_ms: Option<u64>, title: String, artist: String, art: Option<PathBuf> }`
  (`Clone, PartialEq, Eq`).
- **Backend**, chosen once and cached in a `OnceLock` like `audio::backend()`:

| Backend | Detected by | Play/pause, next, previous | Stop, seek | State |
|---|---|---|---|---|
| **Omarchy 4** | `omarchy-shell` on PATH | `omarchy-shell media playPause` / `next` / `previous` | `playerctl` | `playerctl` if installed, else `omarchy-shell media status` (no position) |
| **Omarchy 3** | `omarchy-swayosd-client` on PATH | `omarchy-swayosd-client --playerctl play-pause` / `next` / `previous` | `playerctl` | `playerctl` |
| **Plain** | neither | `playerctl play-pause` / `next` / `previous` | `playerctl` | `playerctl` |

  When `player` is set, or the daemon already knows which player a panel shows, every
  command goes to `playerctl --player=<name> …` instead, so the controlled player is the
  one on screen. Omarchy's commands can't target a player.
- `pub fn media_command(backend, cmd, seek, target: Option<&str>) -> Vec<Vec<String>>`:
  pure, so every table row is tested. Seek: `playerctl [--player=X] position 5+` / `5-`.
- **Reading, `pub fn state(player: Option<&str>) -> Option<PlayerState>`**, through
  `playerctl -a metadata --format '<fields joined by \x1f>'`, with the fields
  `{{playerInstance}} {{playerName}} {{status}} {{position}} {{mpris:length}}
  {{xesam:title}} {{xesam:artist}} {{mpris:artUrl}}`. That's one process for all
  players, one line each. The `\x1f` separator means a title with a tab can't break it.
  - `fn parse_playerctl(out) -> Vec<(String /*name*/, PlayerState)>`. Missing length
    means a live stream: `None`, and the bar is hidden.
  - `fn pick(players, wanted: Option<&str>) -> Option<usize>`: with `wanted`, players
    whose name or instance starts with it. Then the first `Playing`, else the first
    `Paused` with a title, else `None` ("nothing playing"). `playerctld` (a proxy) is
    skipped, as Omarchy's shell skips proxies.
  - `fn art_path(url) -> Option<PathBuf>`: **only `file://` URLs** (percent-decoded),
    and only if the file exists. `https://` (Spotify) and `data:` give `None`. The daemon
    never fetches anything from the network.
  - Fallback without `playerctl` on Omarchy 4: `fn parse_omarchy_status(json)` from
    `omarchy-shell media status`. It fills title, artist, art and status; position and
    length are `None`.
- `pub fn to_panel(state: Option<&PlayerState>, since: Duration) -> render::MediaPanel`:
  the display model, the media version of `audio::Level::to_panel`. While `Playing`,
  elapsed = `position + since`, clamped to the length. Pure and tested.
- `pub fn fmt_time(secs) -> String`: `0:07`, `1:23`, `12:05`, `1:02:03`.

## `omadeck-core/src/action.rs`

- `run` → `Action::Media { command, seek, player } => run_media(*command, *seek,
  player.as_deref(), 1)`. `run_steps` folds a fast spin into **one** seek
  (`seek × steps`, clamped to ±600 s), as volume does. `next`/`previous` on a turn run
  once per detent (capped at 8 as today), so a turn can skip tracks.
- `run_media` spawns the commands through the existing `spawn_seq`. If `playerctl` is
  missing it fails with "playerctl not found (install playerctl)", which shows as a toast
  or a `DialActionFailed` event.
- `pub fn run_media_on(action, steps, target: Option<&str>)` is what the daemon calls
  with the name of the player its panel shows (see below). `run` (the GUI's *Try it
  now*, `omadeckd press`/`dial`) passes `None`.

## `omadeck-core/src/render.rs`: the Media panel

`DialStatus` gains `media: Option<MediaPanel>`:

```rust
pub struct MediaPanel { title: String, artist: String, playing: bool,
    elapsed: Option<u32>, length: Option<u32>, art: Option<PathBuf> }
```

Elgato has no media layout, so this is a **custom layout** built on the `$B1` grid, which
is what an Elgato plugin would do. The icon, the bar and the title row are exactly `$B1`.
Only the value row is split into two smaller lines:

| Item | Rect | Style |
|------|------|-------|
| title | `[16, 10, 168, 24]` | track title, 16 px, weight 600, left, ellipsis; the dial's label when nothing plays |
| icon | `[16, 40, 48, 48]` | album art (`file://` only) scaled to 48 × 48, else the state glyph |
| artist | `[76, 40, 108, 18]` | 14 px, left, ellipsis, label colour mixed 35% toward the background |
| time | `[76, 58, 108, 16]` | 13 px, right, `1:23 / 3:45` (or `1:23` for a stream) |
| progress | `[76, 74, 108, 12]` | groove bar as `$B1`, `elapsed / length` |

- Constants `M1_TITLE`, `M1_ARTIST`, `M1_TIME` next to `B1_*`, with a comment that this is
  omadeck's own layout on the `$B1` grid.
- The title is 168 wide, not 136: track titles need the room, and nothing sits to the
  right of it on this layout.
- **Paused:** the bar fill switches to the track colour mixed halfway toward the fill
  colour (dimmed, not empty: that's for mute), and the glyph is the play glyph.
- **Nothing playing:** title = dial label, artist = "Nothing playing", time `–`, empty
  bar. The layout stays put (no switch back to `$X1`), as volume does with `–`.
- **Glyphs** (Nerd Font MD; check each code point in the font before committing): play
  `f040a` 󰐊, pause `f03e4` 󰏤, next `f04ad`, previous `f04ae`, stop `f04db`, music
  `f075a`. The panel and the keys show **what a press will do** (pause while playing),
  as every player UI does. A user `icon`/`glyph` on the dial wins over the state glyph,
  but album art wins over both while a track has art.
- **Album art** gets its own one-entry cache in `Renderer` (`art: Option<(PathBuf,
  SystemTime, RgbaImage)>`) instead of `icons`, which is only cleared on reload. Every
  track has a new temp file, so `icons` would grow all day. Decoding happens once per
  track on the deck loop, the same as icons load today (a few ms).
- `draw_media(..)` sits next to `draw_b1` and shares `fill_pill` and `draw_text_in`.
- **Key faces:** `render()` takes the key's media state from PR 8's key-state path (see
  Assumptions). A `Media` key with no `icon`/`glyph` of its own draws the command's
  glyph: the play/pause key shows play or pause from the state, and the others are static.
  With no player it draws the play glyph.

## Daemon (`omadeckd`)

- **Probe:** `Probe::Player { player: Option<String> }`, interval **1 s**. **Value:**
  `Value::Player(Option<PlayerState>)`. In `watch.rs`, polling the probe calls
  `player::state(player)`.
- **Send only on change**, with a twist: while playing, the position changes on every
  poll, so the worker would send every second. `fn player_changed(prev, prev_at, new,
  now) -> bool` (pure, in `player.rs`) is true when the status, track, length, art or
  player changes, or when the position is more than **1.5 s** off from where
  `prev + elapsed` predicts it (a seek from elsewhere).
- **Which probes:** the PR 8 probe collector adds `Probe::Player { player }` for each dial
  whose `level_source()` is `Player`, and for each key on the current page with a
  play/pause `Media` action (the other media keys have static faces). On connect, reload,
  page change and disconnect this happens through PR 8's existing `WatchCmd::Watch`, so
  nothing new is needed there.
- **Moving bar without polling faster:** the deck keeps `received: Instant` with each
  `Value::Player`. `dial_status` calls `player::to_panel(state, now - received)`. Each
  loop pass (it already wakes every 100 ms), for media panels that are playing, compare
  the shown second (`Deck.media_shown: Vec<Option<u32>>`) with the current one and
  `paint_dial` only when it changed. That's one 200 × 100 JPEG per second per playing
  panel, and nothing while paused.
- **Actions from the deck:** a `Media` action on a key or a dial goes through
  `after_media(d, …)`, like `after_volume`:
  1. It calls `action::run_media_on(act, steps, target)`. `target` is the `name` of the
     player the probe currently shows, so a seek always scrubs the track on screen.
  2. It updates optimistically: seek → `position += seek × steps` (clamped to 0..length);
     play/pause → flip `status`. Then it repaints the affected dial and keys at once.
  3. It sends `WatchCmd::Refresh(probe)` so the true value follows within ~150 ms.
- `snapshot`: media dials draw the "Nothing playing" panel. Nothing live in a snapshot,
  as with volume.
- No new IPC events.

## GUI

- **Action editor**, "Media": a command combo (Play/Pause, Next track, Previous track,
  Stop, Seek). With Seek, a seconds spin (−60…60, default 5). A player entry
  (placeholder "Any player", e.g. `spotify`).
- **Dial header:** a **Media dial** preset next to Volume dial and Brightness dial. It sets
  label "Media", turn ±5 s seek and press play/pause, and leaves the glyph empty so the
  state glyph and the art show.
- **Key header:** a **Media** menu button with three presets (Play/Pause, Next,
  Previous). Each sets the action and label and leaves the glyph empty.
- **Preview:** on refresh, read `player::state` once for each media dial (one
  `playerctl`, a few ms, as the volume preview does). If nothing is playing, show a
  sample, "Track title" / "Artist", `1:23 / 3:45` at 37%, so the layout can be seen
  while editing. Media keys preview their real state, or the play glyph.
- *Try it now* works (media actions aren't owned by the deck).

## Starter layout

Unchanged: the 15-key starter is full, and starter dials are a separate question. The
README and `examples/config.toml` show a media dial.

## Docs

- README Features: "Media: play/pause, next, previous and seek from keys or a dial; the
  strip shows the track, artist, cover and a moving progress bar". Roadmap line:
  "value layouts on the strip: volume, brightness and media". Note that seeking needs
  `playerctl` on Omarchy 4 (`sudo pacman -S playerctl`).
- `examples/config.toml`: a media dial, a next-track key, and a commented variant with
  turn = the player's app volume (PR 10's `type = "app_volume"`, `app = "spotify"`).
- CHANGELOG line under Unreleased.

## Implementation steps

1. [ ] `config.rs`: `MediaCommand`, `Action::Media`, KINDS/kind_index/empty_of_kind/
       summary, `is_zero_i16`, `LevelSource::Player` (drop `Copy`, fix call sites).
       Round-trip tests. Commit.
2. [ ] `player.rs`: types, `parse_playerctl`, `pick`, `art_path`, `fmt_time`,
       `to_panel`, `player_changed`, `parse_omarchy_status`, backend, `media_command`,
       all with tests from captured real output. Commit.
3. [ ] `action.rs`: `run_media`, `run_media_on`, `run_steps` folding. Commit.
4. [ ] `render.rs`: `MediaPanel`, `M1_*` constants, `draw_media`, the art cache, media
       key glyphs. Layout tests. Commit.
5. [ ] `watch.rs`: `Probe::Player`, `Value::Player`, poll via `player::state`, change
       test with `player_changed`. Probe collection for dials and play/pause keys. Commit.
6. [ ] `main.rs`: `received` stamps, `media_shown` once-per-second repaint, `after_media`
       with optimistic update and Refresh, snapshot. Commit.
7. [ ] GUI: editor rows, Media dial preset, key presets, preview with sample. Commit.
8. [ ] Docs, example, CHANGELOG. `cargo fmt && cargo clippy --all-targets -- -D warnings
       && cargo test`. Commit.

## Tests

- `config`: `media_roundtrip` (all commands, seek, player), `media_defaults_left_out`
  (bare `type = "media"` saves as just that), `old_config_byte_identical` (load + save of
  the PR 13 example config is unchanged), `level_source_media_wins` (media press +
  volume turn → `Player`), `media_summary`.
- `player`: `parse_playerctl_two_players` (captured Spotify + Firefox lines),
  `parse_playerctl_stream_without_length`, `parse_playerctl_garbage`;
  `pick_prefers_playing`, `pick_wanted_by_name_or_instance`, `pick_skips_playerctld`,
  `pick_none_when_stopped`; `art_only_local_files` (`file:///tmp/a%20b.png` → path,
  `https://…` → None, `data:` → None, missing file → None); `fmt_time_forms`;
  `to_panel_interpolates_while_playing`, `to_panel_frozen_while_paused`,
  `to_panel_clamps_to_length`; `changed_on_seek_not_on_tick`;
  `parse_omarchy_status_json`; `media_command_rows` (each backend × command, `--player`
  when targeted, seek `5+`/`5-`).
- `action`: `seek_folds_steps` (3 detents × 5 s → one `position 15+`), clamp ±600.
- `render`: `media_panel_layout` (title pixels in `M1_TITLE`, bar in `B1_BAR`),
  `media_bar_fills_to_progress`, `media_nothing_playing_keeps_layout`,
  `media_key_glyph_follows_state` (play vs pause faces differ).

## Hardware checks

On the Stream Deck + (0fd9:0084), Omarchy 4.0.4, Spotify or mpv playing.

- [ ] `which playerctl` on this machine. Note whether it came with the 3.x install.
- [ ] `omarchy-shell media playPause` works from the systemd service (`OMARCHY_PATH` is
      set). If not, fall back to `playerctl` and write it down.
- [ ] Press = play/pause, and Omarchy's media pop-up shows, the same as the keyboard key.
- [ ] Turn = ±5 s per detent. A fast spin is one `playerctl position N+` (daemon log).
      No `playerctl` processes pile up (`pgrep -c playerctl`).
- [ ] Seeking right past the end goes to the next track (MPRIS `Seek` does that), and
      the panel follows.
- [ ] The bar moves smoothly once a second while playing, stops when paused, and jumps
      right after a seek from the deck (optimistic) and within ~1 s after a seek in the
      player's own UI.
- [ ] A track change in the player updates title, artist and art within ~1 s.
- [ ] Album art: mpv (`mpv-mpris`) and Chromium/Firefox give `file://` art that shows;
      Spotify gives `https://` and shows the glyph. Note what each player gives.
- [ ] Two players (Spotify paused, YouTube playing): the panel shows the playing one,
      and a seek goes to that one.
- [ ] `player = "spotify"` pins the dial to Spotify while something else plays.
- [ ] Play/pause key face follows play/pause from the keyboard keys within ~1 s.
- [ ] Without `playerctl` (take it off `PATH`): keys and press still work through
      `omarchy-shell`, the panel shows title/artist with `–` time, and a turn shows the
      "install playerctl" toast once per action.
- [ ] Idle CPU with a media dial while paused, and while playing (one repaint/s), is close
      to before.

## Decisions to explain

- **`playerctl` for seeking and state, Omarchy's commands for play/pause/next/previous.**
  Same pattern as volume: the deck's media buttons behave exactly like the keyboard's
  (same player choice, same pop-up). Omarchy 4 has no seek and no position, so those
  need `playerctl`, which Omarchy 3 shipped and which is one `pacman -S` away. With no
  `playerctl` everything except scrubbing and the bar still works through
  `omarchy-shell media status`.
- **Polling once a second, plus moving the bar locally**, rather than a long-lived
  `playerctl --follow`. That's the state watcher's model (PR 8). Nothing has to be
  restarted when a player quits, and no child lives across reloads. `--follow` doesn't
  emit the position on every tick anyway. Between polls the deck moves the bar itself,
  so it moves every second without a process every 100 ms. A track change shows up to
  1 s late, the same as volume.
- **The dial shows and controls the same player.** Seeks and targeted commands carry the
  `--player` instance the panel shows. When two players play at once, Omarchy's
  play/pause may pick a different one (it prefers the oldest playing); the panel follows
  within a second. Setting `player` avoids it.
- **Turn = seek by default, not skip or volume as in Elgato's Spotify dial.** The bar
  makes scrubbing visible, the deck already has a volume dial, and skipping is a key
  press away. Other modes need **no `turn = …` option**: a dial's turn is just an action,
  so `turn_* = { type = "media", command = "next"/"previous" }` gives Elgato's skip and
  PR 10's app volume gives Elgato's volume mode.
- **A media action on a dial wins the panel** over a volume action, because a dial that
  controls the player should show the player. Omarchy's own pop-up shows the volume.
- **A custom layout on the `$B1` grid** instead of plain `$B1` (no room for an artist)
  or `$C1` (two bars, meant for two values). Elgato's SDK lets plugins define layouts,
  and this one keeps `$B1`'s icon, title and bar positions.
- **Album art only from local `file://` URLs.** The daemon never fetches from the
  network: it's a privacy and surprise question, and Spotify's art URLs would mean an
  HTTP client in a USB daemon.
- **Faces show the next action** (pause while playing). Elgato's behaviour is
  unconfirmed. This matches every player UI.
- **Ellipsis, no marquee:** the Elgato SDK has no marquee either (`clip`, `ellipsis`,
  `fade`).
- **`player.rs` in core**, not `media.rs`, so it isn't confused with the GUI's icon
  module.

## Assumptions about other PRs

- **PR 8 (state watcher):** `watch.rs` has `Probe` (`Hash + Eq + Clone`) and `Value`
  (`Clone + PartialEq`) enums that this PR extends. The worker polls each probe at its
  own interval and sends `(Probe, Value)` on change; this PR's custom change test is
  either a hook in the worker (`fn changed(&Probe, &Value, &Value, Duration) -> bool`,
  default `!=`) or done in `player::state`'s caller. The deck keeps `HashMap<Probe,
  Value>`; this PR adds a `received: Instant` beside it for player values (or PR 8 already
  stamps every value). `WatchCmd::Refresh(Probe)` re-polls within ~150 ms. There is a
  per-key state path that `render()` can read for faces (PR 8's on/off faces); media keys
  plug into it with a play/pause state instead of a bool.
- **PR 10 (audio streams):** used only in the example of turn = app volume. PR 10's shape
  is `Action::AppVolume { app, focused, change, mute }` (`type = "app_volume"`).
- **PR 7 (strip layers):** `render_strip_panel` draws the panel over the background layer.
  The media panel draws only its items, never a full background, so it inherits that.
- **PR 13:** the next free KINDS index: 13, after Folder at 12.

## Later

- Event-driven updates (`playerctl --follow` or a D-Bus `PropertiesChanged` listener),
  if a 1 s track-change lag bothers.
- A `busctl --user` (systemd) backend for seek and position, so Omarchy 4 needs no extra
  package.
- Album art from `https://`, fetched by the GUI or behind an explicit opt-in, cached on
  disk.
- Marquee (scrolling) titles on the panel and on keys.
- A "now playing" key (title on the face), shuffle/repeat keys, like Elgato's Spotify
  keys.
- Touch on the panel to seek to that spot of the bar; long press = next track.
- Follow Omarchy's own player choice (`omarchy-shell media status` identity) when several
  play at once.

**Size:** about 800 lines: core `player.rs` ~300 with tests, render ~150, config/action
~100, daemon ~120, GUI ~100, docs ~30.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- `playerctl` isn't in Omarchy 4. Tell users to install it for seeking and progress (planned), or seek over `busctl --user` (systemd, nothing to install)?
- Does the `omadeckd` service have `OMARCHY_PATH` set? `omarchy-shell` needs it for play/pause/next/previous.
- When a dial has both a media action and a volume action, the media panel takes the strip (planned). OK?
- Cover art from local `file://` only; Spotify's `https` art shows a glyph. Keep the daemon network-free (planned), or allow fetching?
