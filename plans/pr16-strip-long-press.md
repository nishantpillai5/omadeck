# PR 16: `feat/strip-long-press`: long press on a strip panel as its own action

Part of [PLAN.md](PLAN.md). Base: `feat/app-pages` (PR 15) for a clean merge order. It
needs fix/review-2's action walker (R2), `Daemon::report_failure` (R6) and exhaustive
`ActionTarget` matches (R9), PR 11's `Daemon::try_input` and `Daemon::tap_dial`, and PR
12's `run_of`. It touches the
dial-slot lists PRs 9, 10 and 14 added to, but uses none of their features.

**Status (2026-10-17): built, not verified.** Built on the Mac and pushed to the fork as
`feat/strip-long-press`: 942a058 … 13597e8 (2 commits, tasks 1–2). Core and daemon tests, clippy and fmt pass; the GUI type-checks
(not linked or run; GUI clippy warns, BUGS.md 78), and nothing was run on the deck or with the captures below. Where the code
differs from this plan, the code wins: the deviations, the defaults taken in place of
captures, and every check still to do are in its `feat/strip-long-press` section of [VERIFY.md](VERIFY.md). The review
of what was built is [review-2026-10-17c.md](review-2026-10-17c.md).

**Outcome:** each dial can have a `long_press` action that runs when you press and hold
its panel on the touch strip (Elgato's "long touch"). A dial with no `long_press` does on
a hold what it does on a tap: it runs the dial's `press`. Today a hold does nothing on the
deck; the daemon only publishes `StripLongPress`.

The old plan also fixed BUGS.md #6 (a dial press or strip tap running `page` or
`brightness` failed). The rebase's one `dispatch` fixed that, so it's gone from here.

**Size:** about 270 lines: config ~90 with tests, daemon ~60, GUI ~80, docs ~30.

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
- **Decided: this PR stays last.** It builds on PR 11's `tap_dial`; earlier, the long press
  would go into the `TouchScreenPress` arm and be moved again.
- **Decided: `Source::LongTap(u8)` through `Daemon::try_input`** (PR 11). No new
  dispatch. Failures publish `DialActionFailed`, like `Source::Tap` (its arm in
  `Daemon::report_failure`).
- **Decided: tap and long press share one `strip_touch(x, long)` and PR 11's
  `tap_dial(i, long)`,** so the panel mapping, flash and logging exist once.
- **Decided: a dial's press and long press share one multi-action slot** (PR 12):
  `run_of(Source::LongTap(i))` is `RunOf::Press(i)`, as a tap's is.
- **Decided: the firmware decides what a long press is.** No timer of our own.
- **Decided: per dial, not per page** (dials are device-level, PR 5).
- **Decided: `long_press`, not `long_touch`** in the TOML: omadeck's word, matching the
  crate's event and the existing `StripLongPress` IPC event. The README names Elgato's.
- **Decided: `long_press` is a dial action like the other three.** It goes last in every
  list of a dial's actions: the action walker (rename and delete a page, `links_to`),
  `level_source` (so a dial whose only volume action is a long-press mute still shows the
  volume), PR 10's `app_query` and PR 14's `media_player`.
- **Decided: no ctl.** `omadeckd dial N long-press` is Later; PR 11's dial command isn't
  touched.
- **Decided: no new IPC.** `StripLongPress { x, y }` exists; failures reuse
  `DialActionFailed`.

## Ask before building

None; all settled 2026-10-17.

## What this PR builds on

### Code as merged (`daily` = `feat/toggle-keys`, `55d0d62`), verbatim

```rust
// crates/omadeck-core/src/config.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialConfig {
    // … index, label, show_label, icon, glyph, background, background_image, label_color …
    /// Runs once per detent when the dial is turned left.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub turn_left: Action,
    /// Runs once per detent when the dial is turned right.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub turn_right: Action,
    /// Runs when the dial is pressed down.
    #[serde(default, skip_serializing_if = "Action::is_none")]
    pub press: Action,
}
impl DialConfig {
    pub fn new(index: u8) -> Self;          // every action Action::None
    pub fn is_blank(&self) -> bool;         // … && self.turn_left.is_none() && self.turn_right.is_none() && self.press.is_none()
    pub fn level_source(&self) -> Option<LevelSource> {
        let actions = [&self.turn_right, &self.turn_left, &self.press];
        …
    }
}
impl Config {
    /// Every action in the config: each key's, on every page, and each dial's three.
    fn all_actions(&self) -> impl Iterator<Item = &Action>;          // … self.dials.iter().flat_map(|d| [&d.turn_left, &d.turn_right, &d.press])
    fn all_actions_mut(&mut self) -> impl Iterator<Item = &mut Action>;
    // fix/review-2 replaces both (R2, below): they are `pub` and built on
    // `KeyConfig::actions` and `DialConfig::actions`.
    pub fn links_to(&self, name: &str) -> usize;
    pub fn rename_page(&mut self, old: &str, new: &str);
    pub fn remove_page(&mut self, name: &str);
}
```

```rust
// crates/omadeckd/src/daemon.rs
/// Where an input came from, so a failure is reported against the right key or dial.
/// PR 11 adds `Ctl`, PR 12 `Step`, PR 16 `LongTap(u8)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick }
impl fmt::Display for Source {
    // Source::Key(i) => "key {i}", Source::Turn(i) | Source::Press(i) | Source::Tap(i) => "dial {i}", Source::Flick => "strip flick"
}
// run_input's failure match (fix/review-2 moves it into `Daemon::report_failure`, below):
//   Source::Turn(index) | Source::Press(index) | Source::Tap(index) => self.hub.publish(Event::DialActionFailed { index, message })
// test: source_names_its_key_or_dial
```

The two strip arms of `Daemon::handle_update`:

```rust
DeviceStateUpdate::TouchScreenPress(x, y) => {
    log::info!("strip tap at ({x}, {y})");
    self.hub.publish(Event::StripTap { x, y });
    // A tap on a panel does the same as pressing its dial, as in the Elgato
    // app: run the dial's press and flash the title.
    let Some(d) = &mut self.deck else { return };
    let Some((w, _)) = d.info.strip else { return };
    let Some(i) = tap_to_dial(x as u32, w, d.info.dials as usize) else { return };
    log::info!("strip tap on dial {i}");
    d.flash_until[i] = Some(Instant::now());
    d.mark_dirty(i as u8);
    let act = self.cfg.dial(i as u8).map(|d| d.press.clone()).unwrap_or_default();
    if !act.is_none() {
        self.run_input(Source::Tap(i as u8), &act, 1);
    }
}
DeviceStateUpdate::TouchScreenLongPress(x, y) => {
    // Published for now; PR 16's long press uses it.
    log::info!("strip long press at ({x}, {y})");
    self.hub.publish(Event::StripLongPress { x, y });
}
```

`flash_until[i]` holds the **tap time**, not a deadline (bug 15's convention).

GUI (`app.rs`, `editor.rs`):

```rust
/// … PR 8 adds `KeyOn(KeyRef)`, PR 12 `Step(Box<ActionTarget>, usize)`, PR 16
/// `DialLongPress(u8)`.
#[derive(Clone)]
pub enum ActionTarget { Key(KeyRef), KeyOn(KeyRef), DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8) }
impl ActionTarget {
    pub fn dial(i: u8, dir: Direction) -> Self;
    pub fn dial_slot(&self) -> Option<(u8, Direction)>;   // Key | KeyOn => None
}
pub fn action_at(&self, at: &ActionTarget) -> Option<Action>;
pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool;
    // dial targets: `let ok = self.locked.borrow().is_none(); self.edit_dial(i, |d| …); ok`
pub fn refresh_dial(&self, i: u8);   // tooltip: format!("{} — press: {}", name, d.press.summary())
pub fn deck_info(&self) -> DeckInfo; // .strip: Option<(u32, u32)>

// editor.rs
pub(crate) struct ActionSlot { pub target: ActionTarget, pub dir: Option<Direction>, pub get, pub set, pub own_page, pub on_app_picked, pub on_new_page, pub on_favicon }
impl ActionSlot { pub fn dial(i: u8, dir: Direction) -> Self; pub fn default_action(&self, kind: usize) -> Action; }
pub fn build_dial(app: &Rc<App>, i: u8) -> gtk::Widget;
    // for dir in [Direction::Left, Direction::Right, Direction::Press] { page.add(&dial_action_group(app, i, dir)); }
fn dial_action_group(app: &Rc<App>, i: u8, dir: Direction) -> adw::PreferencesGroup;
    // title; let slot = ActionSlot::dial(i, dir); header suffix test_button(..); action_editor(app, &group, "Action", &slot)
```

`omadeckd dial N left|right|press` (offline, `main.rs`) is unchanged.

### From the plans before this one

- **fix/review-2:** R2's walker. `Action::walk<'a>(&'a self, f: &mut dyn FnMut(&'a
  Action))` and `Action::walk_mut(&mut self, f: &mut dyn FnMut(&mut Action))` are the one
  recursive walk (PR 12 adds `Multi`'s steps there). On top of it:
  ```rust
  impl DialConfig {
      /// Every action this dial owns, nested ones included, slot by slot in
      /// `Direction::ALL` order. PR 16 adds the long press here.
      pub fn actions(&self) -> Vec<&Action> {
          let mut out = Vec::new();
          for dir in Direction::ALL {
              self.action(dir).walk(&mut |a| out.push(a));
          }
          out
      }

      /// `actions`, mutably.
      pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
          for dir in Direction::ALL {
              self.action_mut(dir).walk_mut(f);
          }
      }
  }
  ```
  `KeyConfig::actions(&self) -> Vec<&Action>` and `KeyConfig::actions_mut(&mut self, f:
  &mut dyn FnMut(&mut Action))` are the key side. `Config::all_actions(&self) ->
  Vec<&Action>` and `Config::all_actions_mut(&mut self, f: &mut dyn FnMut(&mut Action))`
  (now `pub`) chain every key's `actions` and then every dial's, and `links_to`,
  `rename_page` and `remove_page` use them. `Direction::ALL: [Direction; 3]`.
  R6: the failure `match src` lives in `fn report_failure(&self, src: Source, page:
  Option<String>, message: String)` (`Key` → `ActionFailed`; `Turn | Press | Tap` →
  `DialActionFailed`; `Flick` → nothing), called by `run_input`'s `Err` arm and by
  `poll_hypr` for the Hyprland worker's failures.
  R9: `App::action_at` and `App::set_action_at` are one exhaustive `match at { … }` each
  (no `_ => unreachable!`), so a new `ActionTarget` variant doesn't compile until handled;
  their dial arms go through `dial_action(&cfg, *i, dir)` and `self.set_dial_action(*i,
  dir, act)`. `action_rows` puts its explanations in `hint_row(text)` rows and never
  calls the group's `set_description`.
- **PR 9:** `level_source()` returns `None` first when `live` is set.
- **PR 10:** `DialConfig::app_query()` (the first `app_query()` among turn right, turn
  left, press); `level_source()`'s Volume/AppVolume step.
- **PR 11:** `Source::Ctl`; `pub fn try_input(&mut self, src: Source, act: &Action, steps:
  u8) -> Result<()>` (logs, dispatches, `show_page` if moved, publishes the failure for
  `src` through `report_failure`), with `run_input` its `bool` wrapper; `fn tap_dial(&mut self, i: u8) ->
  Result<()>` (flash_until[i] = tap time, mark dirty, run `press` with `Source::Tap(i)`),
  called by the `TouchScreenPress` arm (after `tap_to_dial`, as `tap_dial(i as u8)`) and
  by ctl's `Command::Dial { index, dir: Direction::Press, .. }`. `report_failure` has
  `Source::Ctl => {}`. `test_button(app: &Rc<App>, slot: &ActionSlot, run: ActionRun)`
  reads `slot.target`: only a key's own ▶ (`ActionTarget::Key`) sends `Command::Press`;
  every other ▶ sends `Command::Run` for that action only. A dial group calls
  `test_button(app, &slot, slot.get.clone())`.
- **PR 12:** `Source::Step` (`report_failure` has `Source::Step => {}`); `fn
  run_of(&self, src: Source) -> Option<RunOf>`, an exhaustive match over `Source`, with
  `RunOf::Press(u8)` for a dial press or tap; `ActionTarget::Step(Box<ActionTarget>,
  usize)` and `#[derive(Clone, PartialEq)]` on `ActionTarget`; `ActionSlot.kinds: &'static
  [usize]` (`Action::KEY_KINDS` in `ActionSlot::dial`).
- **PR 14:** `DialConfig::media_player()` (the first `Media` action's `player` among turn
  right, turn left, press) and the `Media` check in `level_source()`.

Check these on `feat/app-pages` before Task 1; if a name differs, the branch wins:

```bash
grep -n "enum Source\|fn try_input\|fn tap_dial\|fn run_of\|fn report_failure\|Source::Tap" crates/omadeckd/src/*.rs
grep -n "enum ActionTarget\|fn action_at\|fn set_action_at\|fn set_dial_action\|fn dial_slot" crates/omadeck-gui/src/app.rs
grep -n "pub kinds\|pub fn dial(i: u8\|fn test_button" crates/omadeck-gui/src/editor.rs
grep -n "press\]\|impl DialConfig\|pub fn actions" crates/omadeck-core/src/*.rs
```

The last grep lists every place that spells out a dial's slots (`[…, &self.press]`) and
the walker's `DialConfig::actions`/`actions_mut`: each one gets `long_press` last.

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

## Task 1: config and daemon (~170 lines)

**Files:** `crates/omadeck-core/src/config.rs`, `crates/omadeckd/src/daemon.rs` (`Source`,
`report_failure`, `run_of`, the strip arms, `tap_dial`, ctl's `Dial` handling).

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
  `&& self.long_press.is_none()`.
- Every list of a dial's slots (the `press\]` grep) gets `long_press` last:
  `level_source`'s `actions` (`[&self.turn_right, &self.turn_left, &self.press,
  &self.long_press]`), PR 10's `app_query`, PR 14's `media_player`, and fix/review-2's
  `DialConfig::actions`/`actions_mut`, after their `Direction::ALL` loop:
  ```rust
  // in `actions`, after the loop:
  // The long press isn't a `Direction`: it joins the walk on its own, last.
  self.long_press.walk(&mut |a| out.push(a));
  // in `actions_mut`, after the loop:
  self.long_press.walk_mut(f);
  ```
  Update both doc comments ("slot by slot in `Direction::ALL` order, then the long
  press"). `Config::all_actions(_mut)`, `links_to`, `rename_page` and `remove_page` then
  see it with no change of their own. Leave `Direction` and `Direction::ALL` alone: a
  long press is not a dial direction.
- **Trap:** a full `DialConfig { … }` literal without `..DialConfig::new(i)` stops
  compiling. `grep -rn "DialConfig {$" crates` finds them; add `long_press: Action::None`.

Daemon:

- `Source` gains `LongTap(u8)`; `Display` writes `dial {i}`, like `Tap` (add it to that
  arm). In `report_failure`'s `match src`, `Source::LongTap(index)` goes with
  `Source::Tap(index)` (`DialActionFailed { index, message }`); the compiler points at it. In PR 12's `run_of`,
  `Source::LongTap(i)` maps to `Some(RunOf::Press(i))`, like `Tap`.
- PR 11's `tap_dial` gains `long: bool` and runs `long_press_action()` when it's set.
  Ctl's `Command::Dial` with `dir: Direction::Press` calls `tap_dial(index, false)`. The tap arm and the
  long-press arm both call one helper. Keep publishing `StripTap`/`StripLongPress` (and
  their `log::info!` lines) in the arms, before the call:

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
    log::info!("strip {} on dial {i}", if long { "long press" } else { "tap" });
    self.try_input(if long { Source::LongTap(i) } else { Source::Tap(i) }, &act, 1)
}
```

- **Trap (BUGS.md 4):** the action is cloned out of `self.cfg` and the `deck` borrow ends
  before `self.try_input`, which takes `&mut self`. Keep that order. `strip_touch`'s
  `deck` borrow ends at its last use, before `self.tap_dial`.
- Remove the "Published for now; PR 16's long press uses it." comment.
- Extend the daemon test `source_names_its_key_or_dial` with
  `assert_eq!(Source::LongTap(2).to_string(), "dial 2");`.

Tests in `config.rs` (copy this one), plus `long_press_counts_for_the_panel_level` (a dial
whose only action is `long_press = Volume { mute: true }` →
`level_source() == Some(LevelSource::Volume(AudioDevice::Output))`; `Output` is
`AudioDevice`'s default) and `rename_page_follows_long_press` (a dial whose `long_press`
opens page "A": `rename_page("A", "B")` rewrites it, `links_to("B") == 1`, and
`remove_page("B")` leaves `Action::None`).

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

**Check:**

```bash
cargo test -p omadeck-core long_press && cargo test -p omadeckd source_names && cargo clippy --all-targets -- -D warnings
```

→ `test result: ok. 3 passed` for core, `test result: ok. 1 passed` for the daemon, no
clippy warnings.

## Task 2: GUI and docs (~100 lines)

**Files:** `crates/omadeck-gui/src/app.rs` (`ActionTarget`, `action_at`,
`set_action_at`, `refresh_dial`), `crates/omadeck-gui/src/editor.rs` (`ActionSlot`,
`build_dial`), `README.md`, `examples/config.toml`, `CHANGELOG.md`.

- **`ActionTarget`** gains `DialLongPress(u8)`, the variant its doc comment names for
  this PR. Update that comment's list. `dial_slot()` returns `None` for it (`Direction`
  has no long press). The compiler (R9) then points at the two arms to write:
  ```rust
  // action_at
  ActionTarget::DialLongPress(i) => Some(cfg.dial(*i).map(|d| d.long_press.clone()).unwrap_or_default()),
  // set_action_at
  ActionTarget::DialLongPress(i) => {
      let ok = self.locked.borrow().is_none();
      self.edit_dial(*i, |d| d.long_press = act);
      ok
  }
  ```
- **`ActionSlot`:** split `ActionSlot::dial`'s body into `fn for_dial(i: u8, target:
  ActionTarget, dir: Direction) -> Self` (everything as now, with `target` and `dir`
  passed in). `dial(i, dir)` becomes `Self::for_dial(i, ActionTarget::dial(i, dir),
  dir)`, and the new `pub fn dial_long_press(i: u8) -> Self` is `Self::for_dial(i,
  ActionTarget::DialLongPress(i), Direction::Press)`, so kind picks start from the press
  defaults (`Action::default_for(kind, Direction::Press)`). `for_dial` keeps PR 12's
  `kinds: Action::KEY_KINDS`, so a long press offers the same kinds as a press.
- **`build_dial`:** right after the three `dial_action_group`s (before PR 9's live group
  and Appearance), only on a deck with a strip:

  ```rust
  if app.deck_info().strip.is_some() {
      page.add(&long_press_group(app, i));
  }
  ```

  ```rust
  /// The "Long press on the strip" group: what holding the dial's panel runs.
  fn long_press_group(app: &Rc<App>, i: u8) -> adw::PreferencesGroup {
      let group = adw::PreferencesGroup::builder()
          .title("Long press on the strip")
          .description("Press and hold this dial's panel. Nothing set: same as Press.")
          .build();
      let slot = ActionSlot::dial_long_press(i);
      group.set_header_suffix(Some(&test_button(app, &slot, slot.get.clone())));
      action_editor(app, &group, "Action", &slot);
      group
  }
  ```

  `test_button` is called as `dial_action_group` calls it: `test_button(app, &slot,
  slot.get.clone())` (PR 11 kept its signature).
- **`refresh_dial`** tooltip: append ` · hold: {}` with `d.long_press.summary()` when
  `long_press` is set.
- Presets ("Volume dial", "Brightness dial", PR 14's "Media dial") are unchanged: a long
  press is something you add.
- *Try it now* follows PR 11's rule: the long-press slot's target is
  `ActionTarget::DialLongPress`, not `Key`, so its ▶ sends `Command::Run` for the long
  press action only (sensitivity still from `Action::is_deck_owned()`).
- **Trap:** the group's closures are `Rc<dyn Fn…>` inside `ActionSlot`. Don't add an
  `impl Fn + Clone` parameter anywhere.
- Docs: README, under the Stream Deck + features: "hold a panel on the touch strip for a
  second action (Elgato's "long touch"); unset, a hold does what a tap does". A commented
  `# [dial.long_press]` inside an existing `# [[dial]]` block of `examples/config.toml`
  (so `example_dial_parses` checks it without changing its block count). CHANGELOG under
  Unreleased → Added.

Test: GUI code has no unit tests; copy Task 1's `long_press_falls_back_to_press_and_is_omitted`
pattern for any core helper you add.

**Check:** `cargo fmt && cargo test && cargo clippy --all-targets -- -D warnings` → all
tests pass (`example_dial_parses` covers the new example), no warnings.

## Capture on the machine first

Not captured yet. Paste the output here before Task 1:

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
- [ ] A long press that opens a page: renaming that page in the configurator keeps it
      working.
- [ ] The "Long press on the strip" group's ▶ runs the long press action only: the panel
      doesn't flash.
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
