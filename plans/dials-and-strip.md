# Plan: dials and the touch strip (PRs 1–4)

Expands PRs 1–4 of [PLAN.md](PLAN.md). Each PR has its own plan file (table below); this
file holds what they share. Written against `master` at `3bb7eb4` and
`elgato-streamdeck 0.13.1`. Re-check both before starting each PR.

User-facing choices follow the Elgato SDK (see "Reference" in PLAN.md). Code follows
omadeck. Each PR plan says what Elgato does where it matters, and every difference is
listed under "Decisions to explain".

## What the crate already gives us (checked in the 0.13.1 source)

- **Input.** `DeviceStateReader::read` already yields the Plus events. Today the daemon
  drops them in its `_ => {}` arm:
  - `EncoderTwist(dial, delta: i8)`: `delta` is signed (+ is clockwise) and can be more
    than ±1 when the dial is spun fast.
  - `EncoderDown(dial)` / `EncoderUp(dial)`
  - `TouchScreenPress(x, y)`, `TouchScreenLongPress(x, y)`, `TouchScreenSwipe(from, to)`
- **Geometry.** `Kind::encoder_count()` is 4 on the Plus and 6 on the Plus XL.
  `Kind::lcd_strip_size()` is `(800, 100)` on the Plus, `(100, 1200)` on the Plus XL
  (rotated 270°), and `(248, 58)` on the Neo, which has a strip but no dials.
  Keys on the Plus are 120 px, laid out 2 × 4.
- **Drawing the strip.** `StreamDeck::write_lcd(x, y, &ImageRect)` writes any rectangle
  of the strip (Plus and Plus XL only), and `images::ImageRect::from_image(DynamicImage)`
  JPEG-encodes it. Unlike `set_button_image`, `write_lcd` **sends straight away** and does
  not wait for `flush()`. **No crate bump is needed.**
- `reset()` on shutdown should put the strip back to the Elgato logo too. Check this on
  the hardware in PR 3.

## Change to the PR order

Draw the strip **before** volume: PR 3 is now the strip and PR 4 is volume.

- The strip is generic: it shows each dial's label and icon and reacts to presses, for any
  action. So it does not depend on volume.
- When volume lands, it can show its level on the strip in the same PR, which is the
  "status" the deck is really for. Built the other way round, volume would ship with no
  feedback and the strip would ship with no live value.

| PR | Plan | Branch | Base | Depends on |
|----|------|--------|------|------------|
| 1 | [pr1-plus-input.md](pr1-plus-input.md) | `feat/plus-input` | `upstream/master` | none |
| 2 | [pr2-dial-actions.md](pr2-dial-actions.md) | `feat/dial-actions` | `feat/plus-input` | 1 |
| 3 | [pr3-touch-strip.md](pr3-touch-strip.md) | `feat/touch-strip` | `feat/dial-actions` | 2 |
| 4 | [pr4-dial-volume.md](pr4-dial-volume.md) | `feat/dial-volume` | `feat/touch-strip` | 3 (2 for the action itself) |

Naming: user-facing code and config say **dial** and **strip**, never "encoder" or "LCD".
The roadmap item says "dials and touch strip".

## Decisions to explain (copy into PLAN.md as they're confirmed)

- **"dial" / "strip"** in config and code, not "encoder" / "LCD": that's what the roadmap
  and users call them.
- **`[[dial]]` with `turn_left` / `turn_right` / `press`** that each take an ordinary
  `Action`: same shape as `[[key]]`, no new action language, and any script or Hyprland
  dispatcher works on a dial for free.
- **Actions run once per detent, capped at 8 per event**: it feels right for stepping
  through workspaces, and a fast spin can't fork-bomb. Volume folds the steps into one
  command.
- **The strip copies the Elgato app's layouts.** We use `$X1` (icon) and `$B1`
  (indicator), with Elgato's published positions and sizes: no dividers, title at the
  top left, nothing in an empty slot. People moving from the Elgato software recognise
  it, and we don't have to defend a layout we made up. The differences, all deliberate:
  theme colours and the configured font (with `follow_theme` off it's white on dark like
  Elgato's), an ellipsis on long titles instead of clipping, and a light pressed state
  (the title turns the accent colour).
- **Tap on a panel = press that dial, swipe = change page**, as in the Elgato app. Long
  press does nothing for now.
- **Separate `DialActionFailed` event** rather than overloading `ActionFailed.index`.
- **Strip order before volume**: generic first, so volume can show its status on the day
  it lands.
- **`Action::Volume` rather than a dial-only "volume mode"**: keys can use it too, and the
  strip finds the value to show from the actions.
- **Volume goes through Omarchy's own commands when they're installed**
  (`omarchy-audio-output-volume` / `omarchy-audio-input-mute` on Omarchy 4,
  `swayosd-client` on Omarchy 3), and through plain `wpctl` otherwise. So turning a dial
  shows the same pop-up as the volume keys, changes the same physical sink, and keeps
  Omarchy's 100% cap and unmute-on-change. It's the same pattern as using `uwsm-app` for
  launching apps.
- **Polling volume every 1 s in a worker thread** rather than subscribing to PipeWire:
  simple, nothing new needed, and it never touches the deck loop.
- **Plus only for strip drawing**: the Plus XL (vertical strip, 6 dials) and the Neo's
  info bar are left alone until someone can test them.
