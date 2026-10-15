# PR 1: `feat/plus-input`: read dials and the touch strip

Part of [dials and the touch strip](dials-and-strip.md), which has the crate
facts, the PR order and the decisions to explain. User-facing choices follow the Elgato
SDK (see "Reference" in [PLAN.md](PLAN.md)).

**Outcome:** the daemon logs and publishes dial and strip input, and `DeckInfo` describes
the dials and the strip. Nothing reacts to the input yet.

## `omadeck-core/src/device.rs`
- Add two fields to `DeckInfo`. Both use `#[serde(default, skip_serializing_if = …)]`, so
  an older GUI and a newer daemon (or the other way round) still parse each other's
  `Connected` events:
  ```rust
  /// Rotary dials under the keys (Stream Deck +).
  #[serde(default, skip_serializing_if = "is_zero")]
  pub dials: u8,
  /// Touch strip above the dials, in pixels (width, height).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub strip: Option<(u32, u32)>,
  ```
- In `DeckInfo::new`, set `dials: kind.encoder_count()` and
  `strip: kind.lcd_strip_size().filter(|_| kind.encoder_count() > 0)`. The filter leaves
  out the Neo's info bar, because "strip" here means the strip above the dials. Leave a
  short comment saying so.
- Set `fallback()` to `dials: 0, strip: None`.

## `omadeck-core/src/ipc.rs`: new `Event` variants
```rust
DialTurn { index: u8, delta: i8 },   // + is clockwise
DialDown { index: u8 },
DialUp { index: u8 },
StripTap { x: u16, y: u16 },
StripLongPress { x: u16, y: u16 },
StripSwipe { from: (u16, u16), to: (u16, u16) },
```
Older GUIs skip events they don't know, because `subscribe()` filters with
`serde_json::from_str(..).ok()`. Mention this in the PR.

## `omadeckd/src/main.rs`
- Replace the catch-all arm with explicit arms for the six inputs. Each one logs and
  calls `hub.publish(..)`.
  - Dial turns arrive often, so log them at `debug`. Log presses and strip touches at
    `info`, the way key presses are logged today.
- Keep the `_ => {}` arm for `TouchPointDown/Up` (Neo only), with a comment.
- `omadeckd list` also prints `· 4 dials · 800x100 strip` when the deck has them.

## `omadeck-gui/src/app.rs`
- `handle()` must stay exhaustive, so add one arm that ignores the new events, with a
  comment that PR 2 will use them.

## Tests
- `device.rs`: a `DeckInfo` JSON without `dials`/`strip` parses with zero/`None`. A Plus
  `DeckInfo` round-trips.
- `ipc.rs`: each new event round-trips through `to_line()` and back. Pin the exact JSON
  shape of one of them (`{"event":"dial_turn","index":0,"delta":-2}`), because it is
  part of the socket protocol.

## Docs
- CHANGELOG under Unreleased → Added: "Stream Deck +: the daemon reads dial turns and
  presses and touch-strip taps and swipes, and reports them on its event socket."
- README: no change yet. Nothing is user-visible.

## Hardware checks (Stream Deck +, `0fd9:0084`)
- [ ] `omadeckd list` shows the Plus with 2×4 keys at 120 px, 4 dials and an 800x100 strip.
- [ ] `connect()` picks the Plus, and all 8 keys paint and run actions.
- [ ] `RUST_LOG=debug omadeckd` shows turns (including `|delta| > 1` on a fast spin),
      presses, taps, long presses and swipes with sensible coordinates.
- [ ] `socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/omadeck.sock` shows the new events.
- [ ] Unplug and replug the deck while turning a dial: the daemon recovers.

**Size:** about 120 lines. If the author would rather not merge input that does nothing
yet, PR 1 can be folded into PR 2. Ask in the first message.
