# PR 16: `feat/strip-long-press`: long press on a strip panel as its own action

Part of [PLAN.md](PLAN.md). Base: `feat/app-pages` (PR 15) for a clean merge order, but
nothing from PRs 7–15 is used; it only needs PRs 2–3 (dials, strip), so it can move earlier
in the order without changes. If it lands after PR 12, use PR 12's `dispatch()` instead of
the small helper below.

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Don't build `run_dial_action`: rebase step 2 owns the one dispatch (it also fixes bug #6). Call it for `long_press_action()`. If it is built before the rebase, keep the helper below (it routes `Page` and `Brightness`); after the rebase it must also route `Back` and `PageStep`, which the shared dispatch does.
- [ ] Test a pure `fn route(act: &Action) -> Route { Deck, Spawn }` instead of the helper, which takes `Deck`, `Renderer` and `Hub`.
- [ ] Name the worker parameter for its base: `&AudioWorker` before PR 8, the watcher after.
- [ ] Decided by default unless changed: a dial without `long_press` falls back to `press`.
- [ ] Add the check per step: `cargo test -p omadeck-core <filter>` and `cargo clippy --all-targets -- -D warnings`.
- [ ] Small and accurate otherwise; a good early task for the local model.

**Outcome:** each dial can have a `long_press` action that runs when you press and hold its
panel on the touch strip. A dial with no `long_press` keeps today's behaviour: a long
press does what a tap does (runs the dial's press). On the way, it fixes [BUGS.md](BUGS.md)
#6 (called #4 before BUGS.md was renumbered): a dial press or strip tap whose action is `page` or `brightness` fails today.

## How the Elgato app does it (researched 2026-10-16)

- The Stream Deck + firmware classifies touches itself: tap, long press ("hold") and flick
  are separate HID reports; the host never measures the hold time. The `elgato-streamdeck`
  crate passes them on as `TouchScreenPress`, `TouchScreenLongPress` and
  `TouchScreenSwipe`.
- In the SDK a long press is **not a separate event**: it is a `touchTap` whose payload has
  `hold: true` ("Determines whether the tap was considered "held""), plus `tapPos` relative
  to the action's panel
  ([plugin WebSocket reference](https://docs.elgato.com/streamdeck/sdk/references/websocket/plugin)).
  The action under the touch decides what a held tap does. An action that ignores `hold`
  treats it as a tap.
- Elgato's name for it is **"long touch"**: `setTriggerDescription` has a `longTouch` field
  next to `push`, `rotate` and `touch`, and the manifest's `TriggerDescription` example
  uses `"LongTouch": "Skip Track"`. Those are text the Elgato app shows to describe the
  gesture, not on the device
  ([dials guide](https://docs.elgato.com/streamdeck/sdk/guides/dials)).
- There is no long press of the **dial** itself (pushing and holding the knob): only
  `dialDown`/`dialUp`, and plugins time it themselves if they want one. Out of scope here.
- **Unconfirmed:** the firmware's hold threshold, and what Elgato's own built-in actions do
  on a long touch (e.g. whether the Volume action does anything different from a tap).

## Config

```rust
// DialConfig, after `press`
/// Runs when the dial's strip panel is pressed and held. Unset: a long press does what a
/// tap does (runs `press`).
#[serde(default, skip_serializing_if = "Action::is_none")]
pub long_press: Action,
```

```toml
[[dial]]
index = 3
label = "Brightness"
[dial.turn_left]
type = "brightness"
change = -5
[dial.turn_right]
type = "brightness"
change = 5
[dial.press]                   # tap the panel or push the dial: dim to 20%
type = "brightness"
set = 20
[dial.long_press]              # hold the panel: full brightness
type = "brightness"
set = 100
```

- `DialConfig::long_press_action() -> &Action`: `long_press` if set, else `press`.
- `DialConfig::is_blank` adds `&& self.long_press.is_none()`; `DialConfig::new` sets it to
  `Action::None`.
- Old configs: no `long_press`, nothing written; they load and save byte-identical.
- `long_press` and not `long_touch`: omadeck's word for the gesture matches the crate's
  event and the existing `StripLongPress` IPC event; the README mentions Elgato's
  "long touch".

No new `Action` variant and no `KINDS` change.

## Daemon (`main.rs`)

- New helper `run_dial_action(d, cfg, renderer, hub, audio, i, act, steps)`, used by
  `EncoderTwist` (its existing `match` moves in), `EncoderDown`, the strip tap and the new
  long press. It handles `Page` and `Brightness` on the deck loop (`do_page`,
  `do_brightness`) and everything else with `action::run_steps` + `after_volume`, and
  publishes `DialActionFailed` on error. That fixes bug #6 for dial presses and taps.
  When PR 12's `dispatch()` exists, this helper becomes a call to it.
- `TouchScreenLongPress(x, y)`: keep publishing `StripLongPress { x, y }`; then, as the tap
  does, map `x` to a dial with `render::tap_to_dial`, flash that panel (`DIAL_FLASH`), and
  run `dc.long_press_action()` through the helper.
- The tap and long-press branches share one `strip_touch(d, …, x, long: bool)` so the
  mapping, flash and logging aren't duplicated.
- PR 8's busy state is keys only; nothing here.

## IPC

No new events. `StripLongPress` already exists. Failures reuse `DialActionFailed`.

## GUI (`editor.rs`, `build_dial`)

- A fourth `dial_action_group`, **"Long press on the strip"**, after "Press", with the
  description "Press and hold this dial's panel. Nothing set: same as Press." Only shown
  when the deck has a strip (`deck_info().strip.is_some()`), like the strip preview.
- Presets are unchanged: a long press is something you add to a dial, not part of the
  Volume or Brightness dial presets.
- *Try it now* for the group follows the existing rules (disabled for page/brightness).

## Docs

README: under the Stream Deck + features, "hold a panel on the touch strip for a second
action (Elgato's "long touch")". `examples/config.toml`: a commented `[dial.long_press]`
in the dial example. CHANGELOG: one line for the feature and one for the bug fix.

## Implementation steps

1. [ ] `run_dial_action` helper; move the twist `match` into it; use it for `EncoderDown`
       and the tap. Fixes bug #6. Tests. (Worth doing first as its own commit, or even on
       `feat/brightness` right away; see BUGS.md.)
2. [ ] Config: `long_press`, `long_press_action`, `is_blank`, tests.
3. [ ] Daemon: `strip_touch` for tap and long press.
4. [ ] GUI: the "Long press on the strip" group.
5. [ ] README, example, CHANGELOG.

## Tests

- config: `long_press_roundtrip_and_omitted_when_none`; `long_press_falls_back_to_press`;
  `dial_with_only_long_press_is_not_blank`; `old_dial_config_saves_byte_identical`;
  `example_parses` covers the new example.
- daemon (pure helper): `deck_owned_dial_actions_do_not_go_to_action_run` (page and
  brightness are routed to the deck, not `action::run`, for press, tap and long press).

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Hold each of the four panels: the right dial's `long_press` runs, and the panel
      flashes. A short tap still runs `press`.
- [ ] A dial without `long_press`: holding its panel runs `press`, once.
- [ ] Note roughly how long the firmware waits before it reports a long press, and that a
      hold never also produces a tap (no double action).
- [ ] A hold that drifts sideways is still a long press, not a flick (no page change).
- [ ] Bug #6: a dial whose press is `brightness set = 100`, and one whose press is
      `page index = 0`: pushing the dial and tapping its panel both work, no toast.
- [ ] Config without `long_press` saves byte-identical from the GUI.

## Decisions to explain

- **A long press falls back to the tap action**, as an Elgato action that ignores `hold`
  does. Today a long press does nothing, which feels broken; falling back makes holding
  harmless, and `long_press` is only for when you want something different.
- **The firmware decides what a long press is.** No timer of our own, so it feels the same
  as in the Elgato app.
- **Per dial, not per page**: dials and the strip are device-level (PR 5), so their long
  press is too.
- **`long_press`, not `long_touch`**: omadeck's own naming, matching the crate and the
  existing IPC event; Elgato's term is mentioned in the README.
- **The bug fix rides along** because the long press goes through the same path; it can
  also be cherry-picked on its own.

## Later

- A long push of the dial itself (hold the knob), timed in the daemon from
  `EncoderDown`/`EncoderUp`; Elgato has no built-in equivalent.
- Showing what a long press does, like Elgato's trigger descriptions (a tooltip on the
  strip preview).
- Long press on keys (Elgato has none built in; plugins time it).

**Size:** about 150 lines (helper and bug fix ~50, config ~30 with tests, daemon ~30, GUI
~25, docs).

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- An unset `long_press` falls back to `press` (planned). Or should it do nothing, as today?
- Where in the order: it only needs PRs 2–3, so it could go right after PR 7 (strip
  backgrounds) instead of last.
- Fix bug #6 now on `feat/brightness` (planned as an option), or wait for this PR?
