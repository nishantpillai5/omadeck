# PR 2: `feat/dial-actions`: dials in the config, running existing actions

Part of [dials and the touch strip](dials-and-strip.md), which has the crate
facts, the PR order and the decisions to explain. User-facing choices follow the Elgato
SDK (see "Reference" in [PLAN.md](PLAN.md)).

**Outcome:** `[[dial]]` tables in the config run any existing `Action` when a dial is
turned left, turned right or pressed. The configurator can edit them.

## Config format
```toml
[[dial]]
index = 0                 # dials are numbered left to right from 0
label = "Workspace"
[dial.turn_left]          # runs once per detent
type = "hyprland"
dispatch = "workspace e-1"
[dial.turn_right]
type = "hyprland"
dispatch = "workspace e+1"
[dial.press]
type = "app"
command = "walker"
```

## What Elgato does
In the Elgato SDK a dial action receives `dialRotate` (with signed `ticks`, and a
`pressed` flag for turning while held down), `dialDown`/`dialUp` and `touchTap` (with a
`hold` flag for a long press). Our `turn_left`/`turn_right` (one run per tick), `press`,
and tap = press (PR 3) match that. Turning while pressed, as a separate action, goes in
"Later". Check
<https://docs.elgato.com/streamdeck/sdk/guides/dials> (or the current page for dials)
before starting, and link it in the PR.

## `omadeck-core/src/config.rs`
- `DialConfig { index, label, turn_left: Action, turn_right: Action, press: Action }`.
  - Each action field is `#[serde(default, skip_serializing_if = "Action::is_none")]`, so a
    dial with only a press action saves as just that.
  - Add `Action::is_none()`.
  - Look fields (icon, glyph, colours) wait until PR 3, where something draws them.
- `DialConfig::new(index)` and `is_blank()`, mirroring `KeyConfig`.
- `Config.dials: Vec<DialConfig>` with
  `#[serde(default, rename = "dial", skip_serializing_if = "Vec::is_empty")]`.
  Also add it to `Default`.
- `dial(i)`, `dial_mut(i)` and `clear_dial(i)`, mirroring the key helpers.
  `save_to` drops blank dials and sorts them by index, as it does for keys.
- Export `DialConfig` from `lib.rs`.

## `omadeck-core/src/action.rs`
- `pub fn run_steps(action: &Action, steps: u8) -> Result<()>` runs the action `steps`
  times. The daemon caps `steps` at **8** per event, so a hard spin can't fork 30
  processes at once. PR 4 overrides this for volume, folding the steps into one command.

## `omadeckd/src/main.rs`
- `EncoderTwist(i, delta)`: pick `turn_right` or `turn_left` by the sign of `delta`, then
  call `run_steps(act, delta.unsigned_abs().min(8))`.
- `EncoderDown(i)`: run `press`. `EncoderUp` only publishes.
- Failures publish a new `Event::DialActionFailed { index, message }`. It is a separate
  variant so key indices and dial indices never get mixed up, and older GUIs ignore it.
- Strip touches are still only published. PR 3 gives them a meaning.
- Add `omadeckd dial N left|right|press` next to `press KEY`. It allows testing without
  hardware, and the README already documents `press`.

## GUI (`omadeck-gui`)
- **Selection:** `selected: Cell<Option<u8>>` becomes
  `Cell<Option<Selection>>` with `enum Selection { Key(u8), Dial(u8) }`. Go through every
  `select(Some(i))` call site.
- **Deck preview:** when `deck.dials > 0`, add a row of round `deck-dial` buttons under
  the key grid in `deck-frame`, spaced to sit under the key columns the way they do on the
  hardware. A button's tooltip is the dial's label plus the press summary. Add CSS for
  `deck-dial` (circle, knurled border), `.selected` and `.pressed`, reusing `@deck_accent`.
- `rebuild_grid` relayout check: also compare `dials`.
- **Editor:** factor `editor::action_rows(app, i, key, group)` into one that edits an
  `Action` through a getter and setter
  (`impl Fn(&Config) -> Action`, `impl Fn(&mut Config, Action)`). The key editor and
  the new `editor::build_dial(app, i)` then share it. The dial page has three groups
  (**Turn left**, **Turn right**, **Press**), each with ▶ test, and a Label row and Clear
  at the top.
- **Daemon events:** `DialDown` selects the dial and adds `.pressed`, like `KeyDown`.
  `DialUp` removes `.pressed`. `DialActionFailed` shows a toast "Dial N: …".
- `starter.rs`: no dials by default. Check on hardware that a first run on a Plus still
  produces a good layout.

## Tests
- An old config (an inline TOML string with keys only) loads, and `dials` is empty.
- Saving a keys-only config writes no `dial`. Compare the TOML string so the format stays
  minimal.
- A config with dials round-trips. A dial with only `press` saves without
  `turn_left`/`turn_right`.
- `examples/config.toml` has a commented-out `[[dial]]` block. `example_parses` still
  passes, and a second test parses the block uncommented.

## Docs
- README: under Features, a "Stream Deck +" bullet about dials (turn/press → any action).
  Under Configuration, the `[[dial]]` format.
- `examples/config.toml`: the commented `[[dial]]` example above.
- CHANGELOG: "Dials on the Stream Deck + can run any action on turn left, turn right and
  press."

## Hardware checks
- [ ] A workspace dial: slow turns step one workspace at a time. A fast spin steps
      several, but no more than 8.
- [ ] Press runs the press action. With the GUI open, pressing selects the dial.
- [ ] A bad script path shows a toast from the GUI.
- [ ] Hot reload: editing `[[dial]]` by hand takes effect without a restart, and a typo
      keeps the last good config.
- [ ] Old `~/.config/omadeck/config.toml` from 0.1.0 loads unchanged on a Plus and on a
      15-key deck (or the fallback layout).

**Size:** core and daemon about 200 lines, GUI about 300. If the GUI part gets big, split
it: 2a daemon and config, 2b GUI. Ask the author which they prefer.
