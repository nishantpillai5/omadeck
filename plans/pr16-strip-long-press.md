# PR 16: `feat/strip-long-press`: long press on a strip panel as its own action

Part of [PLAN.md](PLAN.md). Base: `feat/app-pages` (PR 15) for a clean merge order.
It needs groundwork (`Source`, D3; `ActionSlot` and `ActionTarget`, D10) and PR 11's
`Daemon::try_input` (where the failure event is published since PR 11) and
`Daemon::tap_dial`. Nothing from PRs 8–10 or 12–15 is used.

**Outcome:** each dial can have a `long_press` action that runs when you press and hold
its panel on the touch strip (Elgato's "long touch"). A dial with no `long_press` does on
a hold what it does on a tap: it runs the dial's `press`. Today a hold does nothing on the
deck; the daemon only publishes `StripLongPress`.

The old plan also fixed BUGS.md #6 (a dial press or strip tap running `page` or
`brightness` failed). The rebase's one `dispatch` fixed that, so it's gone from here.

## How the Elgato app does it (researched 2026-10-16, trimmed)

- The Stream Deck + firmware classifies touches itself: tap, long press and flick are
  separate HID reports, and the host never times the hold. `elgato-streamdeck` passes
  them on as `TouchScreenPress`, `TouchScreenLongPress` and `TouchScreenSwipe`.
- In the SDK a long press is a `touchTap` with `hold: true`
  ([plugin WebSocket reference](https://docs.elgato.com/streamdeck/sdk/references/websocket/plugin)).
  The action under the touch decides what a held tap does; one that ignores `hold` treats
  it as a tap.
- Elgato calls it **"long touch"** (`setTriggerDescription`'s `longTouch`, the manifest's
  `"LongTouch": "Skip Track"`), text the app shows to describe the gesture
  ([dials guide](https://docs.elgato.com/streamdeck/sdk/guides/dials)).
- There is no long push of the **dial** itself, only `dialDown`/`dialUp`. Out of scope.
- **Unconfirmed:** the firmware's hold threshold, and whether a hold also sends a tap.

## Decided

- **Decided: an unset `long_press` falls back to `press`,** as an Elgato action that
  ignores `hold` does. Holding is then never a dead gesture.
- **Decided: `Source::LongTap(u8)` through `Daemon::try_input`** (D3, PR 11). No new dispatch.
  Failures publish `DialActionFailed`, like `Source::Tap`.
- **Decided: tap and long press share one `strip_touch(x, long)` and PR 11's
  `tap_dial(i, long)`,** so the panel mapping, flash and logging exist once.
- **Decided: the firmware decides what a long press is.** No timer of our own.
- **Decided: per dial, not per page** (dials are device-level, PR 5).
- **Decided: `long_press`, not `long_touch`** in the TOML: omadeck's word, matching the
  crate's event and the existing `StripLongPress` IPC event. The README names Elgato's.
- **Decided: `long_press` counts for the panel's level.** `level_source()` looks at it
  last, after turn right, turn left and press, so a dial whose only volume action is a
  long-press mute still shows the volume.
- **Decided: no ctl.** `omadeckd dial N long-press` is Later; PR 11's dial command isn't
  touched.
- **Decided: no new IPC.** `StripLongPress { x, y }` exists; failures reuse
  `DialActionFailed`.

## Ask before building

1. Unset `long_press` falls back to `press` (planned), or should a hold do nothing?
2. Keep it last (planned)? Building it earlier means writing the long press into
   groundwork's tap arm instead of PR 11's `tap_dial`.

## Before building: check against the base

Groundwork wasn't merged when this plan was written. Check on `feat/app-pages` and use
the branch's names if they differ:

```bash
grep -n "enum Source\|fn try_input\|fn run_input\|fn tap_dial\|Source::Tap\|TouchScreenPress\|TouchScreenLongPress" crates/omadeckd/src/*.rs
grep -n "struct ActionSlot\|enum ActionTarget\|fn action_editor\|fn dial_action_group\|Direction::" crates/omadeck-gui/src/*.rs
grep -n "fn default_for\|enum Direction\|fn level_source" crates/omadeck-core/src/config.rs
grep -rn "DialConfig {$" crates
```

Expected (groundwork, PRs 8, 11, 12): `daemon.rs`: `enum Source { Key(u8), Turn(u8),
Press(u8), Tap(u8), Flick, Ctl, Step }`, `fn Daemon::try_input(&mut self, src: Source,
act: &Action, steps: u8) -> Result<()>` (logs, dispatches, routes volume to the watcher,
publishes the failure event for `src`) with `fn Daemon::run_input(..) -> bool` as its
wrapper, and PR 11's `fn tap_dial(&mut self, i: u8) -> Result<()>` (flash, mark dirty,
run `press` with `Source::Tap(i)`), which the `TouchScreenPress` arm (after
`tap_to_dial`) and ctl's `Dial { input: Press }` call. GUI: `pub(crate) struct ActionSlot
{ target: ActionTarget, dir: Option<Direction>, get: Rc<dyn Fn(&Rc<App>) -> Action>,
set: Rc<dyn Fn(&Rc<App>, Action)>, … }`, `ActionSlot::dial(i, dir)`, `enum ActionTarget
{ Key(KeyRef), DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8), KeyOn(KeyRef),
Step(Box<ActionTarget>, usize) }` with `dial_slot()`, `App::action_at`,
`App::set_action_at`; core `Action::default_for(kind: usize, dir: Direction) -> Action`.

## Code as merged (verbatim, `d7bd448`)

```rust
// crates/omadeck-core/src/config.rs, DialConfig
    /// Runs when the dial is pressed down.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub press: Action,
}
pub fn new(index: u8) -> Self;          // every action Action::None
pub fn is_blank(&self) -> bool;         // … && self.press.is_none()
pub fn level_source(&self) -> Option<LevelSource> {
    let actions = [&self.turn_right, &self.turn_left, &self.press];
    …
}
```

```rust
// crates/omadeckd/src/daemon.rs, Daemon::handle_update (groundwork): the long-press arm
DeviceStateUpdate::TouchScreenLongPress(x, y) => {
    // Published for now; PR 16's long press uses it.
    log::info!("strip long press at ({x}, {y})");
    self.hub.publish(Event::StripLongPress { x, y });
}
```

The tap arm (`TouchScreenPress(x, y)`) publishes `StripTap`, maps `x` with
`tap_to_dial(x as u32, w, d.info.dials as usize)` and calls PR 11's `tap_dial(i)`, which
sets `flash_until[i] = Some(Instant::now())` (the **tap time**, not a deadline: bug 15's
convention), marks the panel dirty and runs `press` through
`try_input(Source::Tap(i), …)`.

## Config

```toml
[[dial]]
index = 3
label = "Brightness"
[dial.press]                   # tap the panel or push the dial: dim to 20%
type = "brightness"
set = 20
[dial.long_press]              # hold the panel: full brightness
type = "brightness"
set = 100
```

## Task 1: config and daemon (~150 lines)

**Files:** `crates/omadeck-core/src/config.rs`, `crates/omadeckd/src/daemon.rs` (`Source`,
`try_input`, the input arms, `tap_dial`, the ctl `Dial` handling).

`DialConfig` gains, right after `press`:

```rust
/// Runs when the dial's strip panel is pressed and held (Elgato's "long touch"). Unset:
/// a long press does what a tap does (runs `press`).
#[serde(default, skip_serializing_if = "Action::is_none")]
pub long_press: Action,
```

```rust
impl DialConfig {
    /// What holding this dial's strip panel runs: `long_press`, else `press`.
    pub fn long_press_action(&self) -> &Action {
        if self.long_press.is_none() { &self.press } else { &self.long_press }
    }
}
```

- `DialConfig::new` sets `long_press: Action::None`; `is_blank` adds
  `&& self.long_press.is_none()`; `level_source` adds `&self.long_press` **last** in its
  list of actions.
- **Trap:** a full `DialConfig { … }` literal without `..DialConfig::new(i)` stops
  compiling. The grep above finds them; add `long_press: Action::None`.

Daemon:

- `Source` gains `LongTap(u8)` (`Display`: `dial {i}`, like `Tap`). In `try_input`'s
  match on the source for the failure event (PR 11 moved it there from `run_input`),
  `Source::LongTap(i)` goes with `Source::Tap(i)` (`DialActionFailed { index: i, … }`),
  and the log names it "long press".
- PR 11's `tap_dial` gains `long: bool` and runs `long_press_action()` when it's set.
  Ctl's `Dial { input: Press }` calls `tap_dial(index, false)`. The tap arm and the
  long-press arm both call one helper. Keep publishing `StripTap`/`StripLongPress` in the
  arms, before the call.

```rust
/// A tap or long press on the strip: the dial under `x`, then `tap_dial`, as in the
/// Elgato app.
fn strip_touch(&mut self, x: u16, long: bool) {
    let Some(deck) = &self.deck else { return };
    let Some((w, _)) = deck.info.strip else { return };
    let Some(i) = tap_to_dial(u32::from(x), w, deck.info.dials as usize) else { return };
    let _ = self.tap_dial(i as u8, long);
}

/// PR 11's, with `long`: flash panel `i` and run the dial's press (tap) or long press.
fn tap_dial(&mut self, i: u8, long: bool) -> Result<()> {
    if let Some(d) = &mut self.deck
        && let Some(t) = d.flash_until.get_mut(i as usize)
    {
        // The tap time, not a deadline (bug 15).
        *t = Some(Instant::now());
        d.mark_dirty(i);
    }
    let act = self.cfg.dial(i).map(|dc| if long { dc.long_press_action() } else { &dc.press }.clone());
    let Some(act) = act.filter(|a| !a.is_none()) else { return Ok(()) };
    log::info!("strip {} on dial {i}: {}", if long { "long press" } else { "tap" }, act.summary());
    self.try_input(if long { Source::LongTap(i) } else { Source::Tap(i) }, &act, 1)
}
```

- **Trap (BUGS.md 4):** the action is cloned out of `self.cfg` and the `deck` borrow ends
  before `self.try_input`, which takes `&mut self`. Keep that order.
- Remove the "Published for now" comment.

Tests in `config.rs` (copy this one), plus `long_press_counts_for_the_panel_level` (a dial
whose only action is `long_press = Volume { mute: true }` →
`level_source() == Some(LevelSource::Volume(AudioDevice::Output))`; `Output` is
`AudioDevice`'s default).

```rust
#[test]
fn long_press_falls_back_to_press_and_is_omitted() {
    let mut d = DialConfig { press: Action::Brightness { change: 0, set: Some(20) }, ..DialConfig::new(3) };
    assert_eq!(d.long_press_action(), &d.press, "unset: same as a tap");
    let s = toml::to_string_pretty(&d).unwrap();
    assert!(!s.contains("long_press"), "nothing written when unset");
    d.long_press = Action::Brightness { change: 0, set: Some(100) };
    assert_eq!(d.long_press_action(), &Action::Brightness { change: 0, set: Some(100) });
    let back: DialConfig = toml::from_str(&toml::to_string_pretty(&d).unwrap()).unwrap();
    assert_eq!(back, d);
    assert!(!DialConfig { long_press: Action::Back, ..DialConfig::new(0) }.is_blank());
}
```

**Check:** `cargo test -p omadeck-core long_press && cargo clippy --all-targets -- -D warnings`
→ `test result: ok. 2 passed`, no clippy warnings.

## Task 2: GUI and docs (~100 lines)

**Files:** `crates/omadeck-gui/src/app.rs` (`ActionTarget`), `crates/omadeck-gui/src/editor.rs`
(`build_dial`), `crates/omadeck-gui/src/app.rs` (`refresh_dial`), `README.md`,
`examples/config.toml`, `CHANGELOG.md`.

- **`ActionTarget`** (groundwork, D10) gains `DialLongPress(u8)`, the variant groundwork
  names for this PR. `dial_slot()` returns `None` for it (`Direction` has no long press);
  `App::action_at` and `App::set_action_at` get explicit arms that read and write
  `DialConfig::long_press` (through `cfg.dial(i)` and `edit_dial(i, ..)`). The slot is
  built like `ActionSlot::dial(i, Direction::Press)` (so kind defaults use the press
  direction) but with `target: ActionTarget::DialLongPress(i)` and its `get`/`set`
  through that target.
- **`build_dial`**: a fourth action group after "Press", built exactly like the other
  three (same helper, same `ActionSlot`), titled **"Long press on the strip"**, with the
  description "Press and hold this dial's panel. Nothing set: same as Press." Only when
  the deck has a strip (`app.deck_info().strip.is_some()`), like the strip preview. Kind
  defaults use the press direction: `Action::default_for(kind, Direction::Press)`.
- **`refresh_dial`** tooltip: append ` · hold: {}` with `d.long_press.summary()` when
  `long_press` is set.
- Presets ("Volume dial", "Brightness dial") are unchanged: a long press is something you
  add.
- *Try it now* follows the existing rule (`Action::is_deck_owned()`, D11).
- **Trap:** the group's closures are `Rc<dyn Fn…>` inside `ActionSlot` (D10). Don't add
  an `impl Fn + Clone` parameter anywhere.
- Docs: README, under the Stream Deck + features: "hold a panel on the touch strip for a
  second action (Elgato's "long touch"); unset, a hold does what a tap does". A commented
  `[dial.long_press]` in the dial example of `examples/config.toml` (keep it inside a
  `# [[dial]]` block so `example_dial_parses` checks it). CHANGELOG under Unreleased →
  Added.

**Check:** `cargo test && cargo clippy --all-targets -- -D warnings` → all tests pass
(`example_dial_parses` covers the new example), no warnings.

## Capture on the machine first

Paste the output here before Task 1:

```bash
systemctl --user stop omadeckd
cargo run -p omadeckd 2>&1 | grep -E "strip (tap|long press|swipe)"
```

Then tap a panel, hold a panel about a second, and hold one while drifting sideways.
Record: whether a hold logs **only** `strip long press at`, or a `strip tap at` first
(if it does, Task 1 must not run `press` for that tap: stop and rethink); roughly how long
the hold takes before the line appears; whether a drifting hold is a long press or a
swipe.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Hold each of the four panels: that dial's `long_press` runs, once, and the panel
      flashes. A short tap still runs `press`.
- [ ] A dial without `long_press`: holding its panel runs `press`, once.
- [ ] A hold that drifts sideways doesn't change page.
- [ ] A long press that fails (e.g. a missing script) shows the configurator's toast.
- [ ] The "Long press on the strip" group is missing on a deck without a strip.
- [ ] A config without `long_press` saves byte-identical from the GUI.

## Decisions to explain

- **A long press falls back to the tap action,** as an Elgato action that ignores `hold`
  does.
- **The firmware decides what a long press is,** so it feels the same as in the Elgato
  app.
- **Per dial, not per page:** dials and the strip are device-level (PR 5).
- **`long_press`, not `long_touch`:** omadeck's naming; Elgato's term is in the README.

## Later

- A long push of the dial itself, timed in the daemon from `EncoderDown`/`EncoderUp`.
- `omadeckd dial N long-press` through PR 11's ctl.
- Showing what a long press does, like Elgato's trigger descriptions (a tooltip on the
  strip preview).

**Size:** about 250 lines: config ~70 with tests, daemon ~60, GUI ~60, docs ~30.
