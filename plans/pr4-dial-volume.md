# PR 4: `feat/dial-volume`: volume, with its level on the strip

Part of [dials and the touch strip](dials-and-strip.md), which has the crate
facts, the PR order and the decisions to explain. User-facing choices follow the Elgato
SDK (see "Reference" in [PLAN.md](PLAN.md)).

**Outcome:** a volume action for output and input (turn = change volume, press = mute),
which keys can use too. The dial's strip panel shows the current level and mute state,
and stays in sync when the volume changes elsewhere.

## Choice: an `Action` variant, not a dial-only control
`Action::Volume` works on keys too (a mute key, volume up/down keys), and it follows the
plan's rule to "reuse the Action enum where possible". The strip finds the value to show
from the dial's actions, so no separate "dial mode" concept is needed. The cost is that a
volume dial takes three small tables in the config; the GUI gets a one-click
**Volume dial** preset to make that easy. Note it under Decisions.

```toml
[[dial]]
index = 0
label = "Volume"
glyph = "f057e"
[dial.turn_left]
type = "volume"
change = -5          # percent; device defaults to "output"
[dial.turn_right]
type = "volume"
change = 5
[dial.press]
type = "volume"
mute = true          # toggle mute
```

## `omadeck-core/src/config.rs`
```rust
/// Change the system volume, through Omarchy's volume commands (so its OSD shows) or `wpctl`.
Volume {
    #[serde(default, skip_serializing_if = "AudioDevice::is_output")]
    device: AudioDevice,           // output (default sink) | input (default source)
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    change: i8,                    // percent, ±
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    mute: bool,                    // toggle mute instead
},
```
- Add a `KINDS` entry "Volume", `kind_index`, `empty_of_kind` (output, +5) and a
  `summary()` ("Volume +5%", "Toggle mute (mic)").
- `DialConfig::volume_device()`: the device of the first `Volume` action among
  turn_right, turn_left and press. That device's level is what the panel shows.

## `omadeck-core/src/action.rs`
**Go through Omarchy's own volume commands, so its on-screen pop-up shows**, just as it
does for the keyboard's volume keys. This follows the same pattern as `launch_app`, which
already uses `uwsm-app` when it's installed. Pick the backend once and cache it in a
`OnceLock` (like `has_uwsm()`):

| Backend | Detected by | Output ±N | Output mute | Input mute | Input ±N |
|---|---|---|---|---|---|
| **Omarchy 4** | `omarchy-audio-output-volume` on PATH | `omarchy-audio-output-volume +N` / `-N` | `omarchy-audio-output-volume mute-toggle` | `omarchy-audio-input-mute` | `wpctl set-volume -l 1.0 …`, then `omarchy-osd -i microphone -p <percent>` |
| **Omarchy 3** | `swayosd-client` on PATH | `swayosd-client --monitor <focused> --output-volume +N` | `… --output-volume mute-toggle` | `… --input-volume mute-toggle` | `… --input-volume +N` (unconfirmed: check it works) |
| **Plain** | neither | `wpctl set-volume -l 1.0 @DEFAULT_AUDIO_SINK@ N%+` | `wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle` | same with `@DEFAULT_AUDIO_SOURCE@` | same |

What Omarchy does today, so we match it exactly:
- **Omarchy 4.0.4** (key bindings in `default/hypr/bindings/media.lua`):
  - The volume keys run `omarchy-audio-output-volume raise|lower|mute-toggle` (raise and
    lower are ±5), and Alt + the volume keys run `+1`/`-1`.
  - The script caps the volume at 100%, unmutes when the volume changes, and ignores a
    second mute within 250 ms. It then calls `omarchy-osd`, which shows Omarchy's
    Quickshell pop-up.
  - It changes the **physical** sink found by `omarchy-audio-output-sink`, looking
    through DSP sinks (speaker tuning, EasyEffects), not `@DEFAULT_AUDIO_SINK@`.
  - The mic mute key runs `omarchy-audio-input-mute` (which also drives the mic-mute LED).
    There is no Omarchy command for changing mic volume, hence `wpctl` + `omarchy-osd`
    in the table.
- **Omarchy 3.x** (`media.conf`): the keys run
  `swayosd-client --monitor "$(hyprctl monitors -j | jq -r '.[] | select(.focused == true).name')" --output-volume raise|lower|mute-toggle|+1|-1`,
  and `--input-volume mute-toggle` for the mic. We ask `hyprctl monitors -j` for the
  focused monitor ourselves (serde_json is already a dependency), so we don't need `jq`.

Details:
- The Omarchy 4 input ±N row is two commands. Run them in order on the helper thread
  that `spawn()` already starts, so the main loop never waits.
- `run_steps` for `Volume { change, .. }`: one command with `change × steps` (e.g.
  `+15` for three ticks), not `steps` separate processes. For mute, toggle once. This
  matters more now: Omarchy 4's script ignores a second mute within 250 ms.
- Caps: the Omarchy 4 *output* script caps at 100% itself, but the *mic* path is a bare
  `wpctl set-volume` (there's no Omarchy mic command) and must carry `-l 1.0` — without
  it a fast spin, folded into one big change, pushes the source past 100%. Plain uses
  `-l 1.0` for both; swayosd uses its own maximum (check whether it allows more than
  100%; if so, add `--max-volume 100`).
- Missing tools fail with a clear message ("wpctl not found (install wireplumber)"), which
  shows up as a toast.
- `pub fn volume_command(backend, device, change, mute, steps) -> Vec<Vec<String>>` is
  pure, so every table row can be tested without running anything.

## `omadeck-core/src/audio.rs` (new)
- `pub struct Level { pub percent: u8, pub muted: bool }`
- `pub fn volume(device: AudioDevice) -> Option<Level>` reads **the same sink that the
  backend changes**. Otherwise the strip and Omarchy's pop-up show different numbers on
  a machine with speaker tuning or EasyEffects:
  - Omarchy 4, output: `sink=$(omarchy-audio-output-sink)`, then
    `LC_ALL=C pactl get-sink-volume <sink>` and `LC_ALL=C pactl get-sink-mute <sink>`,
    just as the Omarchy script does.
  - Everything else: `wpctl get-volume @DEFAULT_AUDIO_SINK@` / `@DEFAULT_AUDIO_SOURCE@`.
- `fn parse_wpctl(out) -> Option<Level>`: pure and unit-tested with `"Volume: 0.40"`,
  `"Volume: 0.40 [MUTED]"`, `"Volume: 1.00"`, `"Volume: 1.53"` (parsed as 153%; the panel
  shows the real number with a full bar) and garbage.
- `fn parse_pactl(volume_out, mute_out) -> Option<Level>`: the first `NN%` on the first
  line, and `Mute: yes`/`no`. Test it with real `pactl` output captured on the Omarchy
  machine and pasted into the test.

## `omadeckd`: keeping the level current without blocking the loop
- New `audio` worker thread, started in `run()`, with two channels:
  - main → worker: `Refresh` (after a volume action runs) and `Watch(Vec<AudioDevice>)`
    (after connect or reload: which devices any dial shows).
  - worker → main: `(AudioDevice, Level)`, sent only when the value changes.
- The worker polls watched devices every **1 s**, plus straight after `Refresh`. It does
  nothing while the watch list is empty, so decks without a volume dial pay nothing. The
  main loop `try_recv`s once per pass, updates `DialStatus.level` and repaints only the
  affected panels.
- After our own turn, the `Refresh` read waits ~50 ms so the volume command can finish
  (Omarchy 4's script runs several `pactl` calls, so measure it: it may need 100 ms or
  more). Otherwise the panel shows the old value until the next poll.
- Alternative to look at only if polling feels laggy: a long-lived `pactl subscribe`
  (needs pipewire-pulse) or `pw-mon` stream. Don't add it in this PR.

## Strip panel with a level: Elgato's `$B1` "Indicator" layout
- A dial whose `volume_device()` is set uses `$B1` (see [PR 3](pr3-touch-strip.md#layout-match-the-elgato-stream-deck-app)) instead of `$X1`: icon on
  the left at `[16, 40, 48, 48]`, `40%` right-aligned in `[76, 40, 108, 32]` at 24 px,
  and a groove bar in `[76, 74, 108, 12]` filled to the level.
- Muted: the value text reads `Muted`, and the bar fill switches to the track colour, so
  only the outline of the level remains. The icon becomes the mute glyph (󰝟 `f075f`)
  unless the user set their own icon.
- Above 100% (only possible if someone sets it outside omadeck): show the real number and
  a full bar.
- When the dial has no `glyph` or `icon`, default to a speaker glyph (output) or a mic
  glyph (input), so a bare `type = "volume"` config still looks right.
- While the level isn't known yet (before the first poll, or `wpctl` missing), draw `$B1`
  with `–` as the value and an empty bar rather than falling back to `$X1`, so the panel
  doesn't jump.

## GUI
- Action editor rows for Volume: device combo (Output / Input), change spin (−25…25 %),
  mute switch (which hides the change row).
- Dial editor header: a **Volume dial** preset button that fills all three actions,
  label and glyph.
- Strip preview: call `audio::volume` once for each shown device on refresh, so the
  preview shows the real level, then `refresh_all` (GUI thread, a few ms, only on
  refresh). If that turns out to be slow, show a fixed 50% instead.

## Tests
- `audio::parse` cases above.
- `Volume` TOML round-trip, including the defaults being left out (`device`, `change = 0`,
  `mute = false`).
- `volume_device()` picks the right device. `None` for non-volume dials.
- `run_steps` folding: a pure helper `volume_args(device, change, steps) -> Vec<String>`
  so it can be tested without running `wpctl`.

## Docs
- README: Features ("Volume: output/input, mute, and its level on the strip"). Tick the
  roadmap item **only partly**: brightness and scrubbing are still open, so reword it
  rather than checking it off.
- `examples/config.toml`: a volume dial.
- CHANGELOG line.

## Hardware checks

Checked on the Stream Deck + (serial A00WA5081LZEYC) with Omarchy 4.0.4-1, on 2026-10-16.

- [x] Turn = ±5% per detent, fast spin = one command, capped at 100%. (Log: a two-detent
      turn ran a single `omarchy-audio-output-volume +10`; the backend clamps.)
- [x] Omarchy's volume pop-up appears on turn and on press (mute), on the focused
      monitor, and looks the same as when you use the keyboard's volume keys. (Commands
      confirmed in the daemon log; the pop-up confirmed visually.)
- [ ] With a speaker tuning or EasyEffects sink active, the strip shows the same number
      as Omarchy's pop-up. (Not tested: no EasyEffects or tuning on this machine.)
- [ ] Mic dial: mute shows "Microphone muted" and lights the mic-mute LED, if the machine
      has one. (Not tested: no mic dial configured; the machine has Mic1/Mic2 sources.)
- [x] Note the Omarchy version (`omarchy-version`) in the PR. If possible, also check
      the plain `wpctl` path by temporarily taking the Omarchy scripts off `PATH`.
      (Version noted above; no PR exists yet. The `wpctl` parse path is unit-tested, not
      hardware-checked.)
- [x] Press toggles mute, and the panel shows the mute state within one poll. (Panel
      showed the muted state and an empty bar.)
- [ ] Changing volume with keyboard keys or `pavucontrol` updates the panel within ~1 s.
      (Pipeline is the same poll-and-repaint path as the verified dial turns; needs a
      visual check on the LCD, which the snapshot can't show.)
- [x] Input (mic) dial works the same way. (Tested on dial 2, a mic: turn ±5%, press
      mutes. Found and fixed a bug — a fast spin pushed the source past 100% because the
      bare `wpctl` had no cap; it now carries `-l 1.0`, clamped like the output.)
- [ ] Switching the default sink (e.g. to headphones) follows the new default. (Not
      testable: the machine has a single sink.)
- [x] No `wpctl` processes pile up (`pgrep -c wpctl` while spinning). (None; volume goes
      through the Omarchy scripts anyway, and a spin is one command.)
- [x] CPU of `omadeckd` at idle with a volume dial is the same as before. (~0%; the
      worker sleeps between polls and only sends on change.)

## Fixes that came out of running it

- **Mic volume capped at 100% (`da2b426`).** The mic has no Omarchy command, so it falls
  back to a bare `wpctl set-volume` with no limit — unlike the output, which goes through
  `omarchy-audio-output-volume` and clamps itself. A fast dial spin folds into one large
  change (e.g. `+15`), which pushed the source past 100% (verified: `95% + 10%+` → 105%).
  Added `-l 1.0`, as the Plain backend already used, so the mic clamps at 100% like the
  output. The read-back (`wpctl get-volume`) reports the real number above 100%, so the
  strip would have shown it.

**Size:** about 400 lines.
