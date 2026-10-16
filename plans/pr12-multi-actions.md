# PR 12: `feat/multi-actions`: one key runs several actions, with delays

Part of [PLAN.md](PLAN.md). Base: `feat/ctl` (PR 11). It needs groundwork's `run_input`,
`next_deadline`, watcher and `ActionSlot`, and PR 11's `Source::Ctl`, `Command::Run` and
`test_button`. No IPC change.

**Outcome:** a new `multi` action holds an ordered list of steps. A step is any ordinary
action (app, script, sound, website, Hyprland, volume, app volume, page, brightness) or a
`delay` that waits some milliseconds. It works on keys, dial presses, strip taps, dial
turns (once per turn event), `omadeckd press` and *Try it now*. Deck-owned steps run on the
deck loop like a press. Delays are timed by a small **sequencer** on the loop, so the loop
never blocks. The configurator edits the steps as a list (add, remove, move up/down), and
each step reuses the single-action editor. Old configs load and save byte-identical.

The roadmap item is "Multi-actions: run several actions from one key, with optional delays".

## How the Elgato app does it (researched 2026-10-16)

- **Multi Action** runs a sequence of actions top to bottom from one key tap.
  ([how to use Multi Actions](https://www.elgato.com/us/en/explorer/products/stream-deck/how-to-use-multi-actions/),
  [help 360027960912](https://help.elgato.com/hc/en-us/articles/360027960912-Elgato-Stream-Deck-Multi-Actions), read from search snippets: 403)
- **No nesting:** "You cannot nest a Multi Action within another Multi Action".
- **Delay** is its own action in the Multi Action category, in **milliseconds**.
- **Gap between steps:** 5.2 had a fixed 200 ms; 5.3 made it per step ("Next Action",
  default 100 ms; range unconfirmed, 2–100 or 2–200 ms).
  ([help 7092527123725](https://help.elgato.com/hc/en-us/articles/7092527123725), 403, from snippets)
- **Multi Action Switch** (4.4) alternates between two multis; **Random Action** runs one
  step at random.
- **Unconfirmed:** what a second press does while a multi runs; whether a failed step stops
  the rest; whether a multi can go on a Stream Deck + dial; the maximum number of steps.

## What this plan uses from the base

From `daily` (`d7bd448`) and from [groundwork.md](groundwork.md), PRs 8–11. **Before task 1,
grep each name on `feat/ctl`.** If one differs, use the merged name and fix this list.

```rust
// omadeck-core/src/config.rs: Action is #[serde(tag = "type", rename_all = "snake_case")]
// KINDS after PR 10: [&str; 12], 11 = "App volume" (Action::AppVolume { .. })
impl Action {
    pub fn kind_index(&self) -> usize;
    pub fn is_deck_owned(&self) -> bool;        // groundwork: Page | Back | PageStep | Brightness
    pub fn empty_of_kind(i: usize) -> Self;
    pub fn default_for(kind: usize, dir: Direction) -> Action; // fix/review; Direction { Left, Right, Press }
    pub fn summary(&self) -> String;
}
// omadeck-core/src/action.rs (groundwork Task 1)
pub fn run(action: &Action) -> Result<()>;      // bails when action.is_deck_owned()
pub fn run_steps(action: &Action, steps: u8) -> Result<()>;
pub fn apply_volume(device: AudioDevice, change: i8, mute: bool);   // blocking; the watcher's call

// omadeckd/src/daemon.rs (groundwork Task 7, PR 11)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick, Ctl }   // + Display
impl Daemon {
    /// Logs, dispatches (Volume → WatchCmd::Volume, AppVolume → WatchCmd::AppVolume,
    /// deck-owned on nav/rt, the rest action::run_steps), show_page if moved, publishes
    /// the failure event for src (none for Ctl). PR 11.
    pub fn try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()>;
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool; // try_input(..).is_ok()
    pub fn next_deadline(&self) -> Option<Instant>;  // [flash, repaint_at, reload_at].min()
    pub fn handle_commands(&mut self);
    pub fn pass(&mut self) -> Result<(), StreamDeckError>;
    pub fn shutdown(self);
}
// main.rs run(): per iteration daemon.handle_commands(), apply_values(), poll_jobs() (PR 8),
// apply_hypr() (PR 9), then pass() with a deck, else flush() (PR 8), sleep
// wait_for(now, next_deadline()) and scan.
// Event::ActionFailed { index: u8, page: Option<String>, message: String }   (PR 8 added `page`)
// Event::DialActionFailed { index: u8, message: String }

// omadeck-gui (groundwork Task 10, PR 8, PR 11)
#[derive(Clone)]
pub enum ActionTarget { Key(KeyRef), DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8), KeyOn(KeyRef) } // KeyOn: PR 8
impl App {
    pub fn action_at(&self, at: &ActionTarget) -> Option<Action>;            // None if its page has gone
    pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool;
}
#[derive(Clone)]
pub(crate) struct ActionSlot {
    pub target: ActionTarget, pub dir: Option<Direction>,
    pub get: Rc<dyn Fn(&Rc<App>) -> Action>, pub set: Rc<dyn Fn(&Rc<App>, Action)>,
    pub own_page: Option<String>,
    pub on_app_picked: Rc<dyn Fn(&Rc<App>, &ActionTarget, String, String, Option<String>)>,
    pub on_new_page: Rc<dyn Fn(&Rc<App>) -> String>,
    pub on_favicon: Option<FaviconHandler>,
}
impl ActionSlot { pub fn dial(i: u8, dir: Direction) -> Self; pub fn default_action(&self, kind: usize) -> Action; }
fn action_editor(app: &Rc<App>, group: &adw::PreferencesGroup, combo_title: &str, slot: &ActionSlot);
fn action_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot);
fn test_button(app: &Rc<App>, slot: &ActionSlot) -> gtk::Button;   // PR 11: deck-owned → Command::Run
```

## Config

Two new `Action` variants. Delay is an action, as in Elgato's list, so a step is a plain
`Action` and the TOML stays uniform:

```rust
/// Run several actions in order, as Elgato's Multi Action does. `delay` steps wait.
/// The daemon times the steps, so a multi never blocks the deck.
Multi {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    steps: Vec<Action>,
},
/// Wait this many milliseconds before the next step. Only meaningful inside a `multi`;
/// on its own it does nothing.
Delay {
    #[serde(default)]
    ms: u32,
},
```

The steps are arrays of tables in the key's action table. Checked with `toml` 1.1
(`to_string_pretty`): it writes `[[key.action.steps]]` and `[[page.key.action.steps]]`, not
inline arrays, and reads them back equal.

```toml
[[key]]
index = 5
label = "Meeting"
glyph = "f0f4a"

[key.action]
type = "multi"

[[key.action.steps]]
type = "volume"
device = "input"
mute = true

[[key.action.steps]]
type = "app"
command = "zoom.desktop"

[[key.action.steps]]
type = "delay"
ms = 2000

[[key.action.steps]]
type = "hyprland"
dispatch = "movetoworkspace 5"

[[key.action.steps]]
type = "page"
page = "Meeting"
```

A dial uses `[dial.press]` with `type = "multi"` and `[[dial.press.steps]]`. No new fields
on existing structs, so old files round-trip byte-identical. `KeyConfig::is_blank` needs no
change: an empty multi isn't `None`.

Decided: **nesting is refused at run time, not at load.** serde parses a `multi` inside a
`multi`; running it fails that step ("multi-actions can't be nested"). A load error would
throw the whole config away for one bad step. The GUI never offers it.

## Core (`omadeck-core`)

### Kinds (`config.rs`)

```rust
pub const KINDS: [&'static str; 14] = [
    "Nothing", "Launch app", "Run script", "Play sound", "Open website", "Hyprland",
    "Open page", "Go back", "Volume", "Next/previous page", "Brightness", "App volume",
    "Multi action", // 12 (PR 12)
    "Delay",        // 13 (PR 12)
];
/// The kinds a key or dial action can be: everything but Delay.
pub const KEY_KINDS: &'static [usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];
/// The kinds a multi's step can be: everything but Multi.
pub const STEP_KINDS: &'static [usize] = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 13];

/// The combo row position of `kind` in `kinds`, or `None` when it isn't offered there
/// (a hand-edited nested multi). The other way is `kinds[pos]`.
pub fn kind_position(kinds: &[usize], kind: usize) -> Option<u32>;
```

- `kind_index`: `Multi { .. } => 12`, `Delay { .. } => 13`.
- `empty_of_kind` (and so `default_for`): `12 => Multi { steps: vec![] }`,
  `13 => Delay { ms: 500 }`.
- `is_deck_owned` gains `Action::Multi { steps } => steps.iter().any(Action::is_deck_owned)`.
- PR 14 appends Media (14) to `KINDS`, `KEY_KINDS` and `STEP_KINDS`.

### Summaries

- `Delay`: `Wait 500 ms` below 1000 ms; from 1000 ms, seconds with up to two decimals and no
  trailing zeros: `Wait 1.5 s`, `Wait 2 s`, `Wait 1.25 s`
  (`format!("Wait {} s", (ms as f64 / 10.0).round() / 100.0)`).
- `Multi`: `Multi action (no steps)` when empty; `Multi action (only waits)` when every
  step is a delay; else `1 action: {first}` or `{n} actions: {first}, …`, where `n` counts
  the non-delay steps and `first` is the first non-delay step's summary.

### Running without the deck loop (`action.rs`)

```rust
/// Run a multi's steps in order on this thread, for callers with no deck loop (the
/// offline CLI and the configurator's Try it now). Delays sleep; a volume step waits
/// for its command. Deck-owned steps fail (`run` bails), and so do nested multis.
/// Returns each failed step's number (from 0) and error; the other steps still run.
pub fn run_multi_blocking(steps: &[Action]) -> Vec<(usize, anyhow::Error)>;
```

- `Delay { ms }` → `thread::sleep(Duration::from_millis(ms.into()))`.
- `Multi { .. }` → error `multi-actions can't be nested`.
- `Volume { device, change, mute }` → `apply_volume(*device, *change, *mute)` (blocking, so
  it's done before the next step; it returns nothing, so a volume step never fails here).
- anything else → `run(step)`, PR 10's `AppVolume` included. This runner is only the
  no-daemon path (offline CLI, Try it now with no daemon), so an `AppVolume` step is spawned
  here and not waited for. In the daemon, `Volume` and `AppVolume` steps never reach
  `action::run`: `try_input` → `dispatch` sends them to the watcher (`WatchCmd::Volume` /
  `WatchCmd::AppVolume`), see "What order the steps keep".

`run` gains:
- `Delay { .. } => Ok(())`.
- `Multi { steps }`: when deck-owned, `bail!("this multi action has page or brightness steps, which only work on the deck")`
  (before the generic deck-owned bail). Otherwise spawn a thread named `multi` that calls
  `run_multi_blocking(&steps)` and logs each failure. The daemon never calls `run` on a multi.

### The sequencer (new `omadeck-core/src/sequence.rs`, pure)

```rust
use std::time::{Duration, Instant};
use crate::config::Action;

/// Running multi actions, timed by whoever polls it (the daemon's loop). Pure: no
/// threads, no clock of its own, so it's unit-tested with made-up instants.
#[derive(Debug, Default)]
pub struct Sequencer<K> { runs: Vec<Run<K>> }

#[derive(Debug)]
struct Run<K> { key: K, steps: Vec<Action>, next: usize, due: Instant }

impl<K: Clone + PartialEq> Sequencer<K> {
    /// Start `steps` for `key`. False (and nothing starts) while that key's last run is
    /// still going. An empty list starts nothing and returns true.
    pub fn start(&mut self, key: K, steps: Vec<Action>, now: Instant) -> bool;
    /// Every step whose time has come, in order, as (key, step number from 0, action).
    /// A delay moves its run's deadline on from the previous deadline (so lateness doesn't
    /// add up) and isn't returned. A run is dropped once its last step, or its trailing
    /// delay, is done.
    pub fn due(&mut self, now: Instant) -> Vec<(K, usize, Action)>;
    /// The earliest deadline of a waiting run, for the loop's read timeout.
    pub fn next_due(&self) -> Option<Instant>;
    pub fn is_running(&self, key: &K) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`due` is a `retain_mut` over the runs:

```rust
pub fn due(&mut self, now: Instant) -> Vec<(K, usize, Action)> {
    let mut out = Vec::new();
    self.runs.retain_mut(|run| {
        while run.due <= now {
            let Some(step) = run.steps.get(run.next) else { return false };
            let n = run.next;
            run.next += 1;
            match step {
                Action::Delay { ms } => run.due += Duration::from_millis(u64::from(*ms)),
                _ => out.push((run.key.clone(), n, step.clone())),
            }
        }
        true
    });
    out
}
```

Decided:
- **A trailing delay keeps the run going** until it ends, so it works as a cool-down for
  "ignored while running".
- Steps with no delay between them come out of one `due` call and run in the same pass.

## Daemon (`omadeckd`)

```rust
/// Which input started a multi: what "ignored while running" compares, and where a
/// failed step is reported.
#[derive(Debug, Clone, PartialEq)]
enum RunOf {
    Key { page: Option<String>, index: u8 },
    /// Either direction: a left and a right multi on one dial don't overlap.
    Turn(u8),
    /// A dial press or a tap on its panel (the same action).
    Press(u8),
    Ctl,
}
impl std::fmt::Display for RunOf { /* "key 5", "key 5 on Media", "dial 2", "omadeckd" */ }

pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick, Ctl, Step }  // + Step ("step")

pub struct Daemon { /* … */ seq: Sequencer<RunOf> }   // Sequencer::default() in Daemon::new

impl Daemon {
    /// `Key`/`Turn`/`Press`/`Tap`/`Ctl` → its RunOf (`Key` takes the shown page);
    /// `Flick` and `Step` → None (a step can't start a multi).
    fn run_of(&self, src: Source) -> Option<RunOf>;
    /// Run the steps whose time has come. `main.rs`'s `run()` calls it on every
    /// iteration, right after `handle_commands`, deck or not.
    pub fn run_due_steps(&mut self, now: Instant);
}
```

- **`try_input`**, at the top, before it logs and dispatches:
  - `Action::Delay { .. } => return Ok(())`.
  - `Action::Multi { steps }`: `let Some(of) = self.run_of(src) else { bail!("multi-actions can't be nested") }`.
    If `!self.seq.start(of.clone(), steps.clone(), Instant::now())`, log
    `{of}: multi action still running; ignored` and return `Ok(())`. `steps` is ignored
    (a turn starts one run, whatever the detent count).
  - `Source::Step` publishes no failure event (the caller does, below), like `Ctl`.
- **`run_due_steps`:**

```rust
fn run_due_steps(&mut self, now: Instant) {
    for (of, n, step) in self.seq.due(now) {
        log::info!("{of} step {}: {}", n + 1, step.summary());
        if let Err(e) = self.try_input(Source::Step, &step, 1) {
            let message = format!("step {}: {e:#}", n + 1);
            log::warn!("{of}: {message}");
            match of {
                RunOf::Key { index, page } => self.hub.publish(Event::ActionFailed { index, page, message }),
                RunOf::Turn(index) | RunOf::Press(index) => {
                    self.hub.publish(Event::DialActionFailed { index, message })
                }
                RunOf::Ctl => {}
            }
        }
    }
}
```

- **Timing:** `next_deadline` includes `self.seq.next_due()`, so `pass`'s read timeout (and
  the no-deck sleep) shrinks to the next step (D4). A step started by a press in `pass` is
  due at once, so the next iteration's read waits 0 ms and `run_due_steps` runs it; its
  dirty panels are painted by that iteration's `flush`. A delay is accurate to a few ms, and
  with no multi running the loop wakes as often as before.
- **`omadeckd press` through the daemon** (PR 11) runs a multi key like a press:
  `Source::Key` → `RunOf::Key`. `Command::Run { Multi }` (Try it now) is `RunOf::Ctl`; its
  reply is the multi's summary, and failed steps are only logged.

### What order the steps keep

Decided, and written in the README:
- Steps **start** in list order. The sequencer never waits for a step's process to exit:
  apps and scripts can run for hours, and Elgato doesn't wait for a launched app either.
- A **deck-owned** step (page, back, page step, brightness) has taken effect before the
  next step runs: it's done on the loop.
- A **Hyprland** step has had Hyprland's answer before the next step runs (`hyprctl
  dispatch` waits for `ok`, a few ms on the loop, as for a key).
- A **volume** or **app volume** step goes through `try_input` → `dispatch`, which queues it
  on the watcher (`WatchCmd::Volume` / `WatchCmd::AppVolume`, never `action::run`); the
  watcher applies volume changes
  one at a time in the order sent (D5). So volume steps keep their order among themselves
  and against the dials. The next step does **not** wait for it: a launch right after a mute
  can start a few ms before the mute lands. Put a `delay` of 100 ms between them when it
  matters.
- App, script, sound and website steps: the process has been spawned before the next step.

### Lifecycle

Decided:
- **Second press while running: ignored**, logged. It never doubles side effects (two
  Zooms, a page flipping twice) and never cuts a run off halfway.
- **A failing step doesn't stop the rest.** Each failure is reported with its number, e.g.
  the toast "Key 6: step 2: script not found …".
- **Page change:** runs go on. Steps were cloned at start, so the page shown doesn't matter.
- **Config reload:** running sequences keep their cloned steps. New presses use the new config.
- **Unplug:** runs go on (the sequencer is in `Daemon`, not `Deck`). Deck-owned steps act on
  `nav`/`Runtime` and show on reconnect, as D1 does for brightness.
- **Shutdown:** in `Daemon::shutdown`, if any are running, log `dropping {n} running multi actions`. Spawned
  children are not affected.

### Offline CLI

PR 11's `press_locally` / `dial_locally`: a `Multi` that isn't deck-owned goes to
`run_multi_blocking` (on the main thread, so delays aren't cut short by the exit); each
failure is printed as `step N: …` on stderr, and the exit code is 1 if any failed. A
deck-owned multi fails with `run`'s message, exit 1.

## GUI (`omadeck-gui`)

### A step is an `ActionTarget`

Groundwork names `Step(Box<ActionTarget>, usize)` as PR 12's variant. It gets `PartialEq`
(and `KeyRef` too), so a selection can be compared:

```rust
#[derive(Clone, PartialEq)]
pub enum ActionTarget {
    Key(KeyRef),
    DialTurnLeft(u8),
    DialTurnRight(u8),
    DialPress(u8),
    KeyOn(KeyRef),   // PR 8
    /// Step `n` (from 0) of the multi action at the inner target.
    Step(Box<ActionTarget>, usize),
}
```

- `dial_slot()`: `Step(..) => None`.
- `App::action_at`: a `Step(of, n)` arm before the existing code:
  `Some(match self.action_at(of)? { Action::Multi { steps } => steps.get(*n).cloned().unwrap_or_default(), _ => Action::None })`.
- `App::set_action_at`: a `Step(of, n)` arm: take `self.action_at(of)` (an owned clone, the
  borrow is already dropped), and if it's a `Multi` with `n < steps.len()`, replace
  `steps[n]` and return `self.set_action_at(of, parent)`. Otherwise toast "That step was
  removed, so the change was dropped" and return false.

So late results (the app picker, a file dialog) land on the step they were started for,
through the same path a key's or dial's do, and a step inside a key on a deleted page is
dropped by `edit_ref` as today.

### The slot

`ActionSlot` gains `pub kinds: &'static [usize]`: `Action::KEY_KINDS` in `build_key` and
`ActionSlot::dial`. And a constructor:

```rust
impl ActionSlot {
    /// The slot for step `n` of the multi that `self` edits. Reads and writes go through
    /// its target, like a dial slot's.
    pub fn step(&self, n: usize) -> Self {
        let target = ActionTarget::Step(Box::new(self.target.clone()), n);
        let (t1, t2) = (target.clone(), target.clone());
        ActionSlot {
            target,
            dir: None,
            get: Rc::new(move |a| a.action_at(&t1).unwrap_or_default()),
            set: Rc::new(move |a, act| {
                a.set_action_at(&t2, act);
            }),
            own_page: self.own_page.clone(),
            // A step's app pick sets only the step: the key's label and icon aren't its.
            on_app_picked: Rc::new(|a, at, command, _, _| {
                a.set_action_at(at, Action::App { command });
            }),
            on_new_page: self.on_new_page.clone(),
            on_favicon: None,
            kinds: Action::STEP_KINDS,
        }
    }
}
```

`dir: None`, so a kind picked for a step starts from `empty_of_kind` (`default_action`).

### Combo rows over a kind list

`action_editor`'s kind combo shows `slot.kinds` instead of all of `KINDS`:

```rust
let names: Vec<&str> = slot.kinds.iter().map(|&k| Action::KINDS[k]).collect();
let kind_row = adw::ComboRow::builder().title(combo_title).model(&gtk::StringList::new(&names)).build();
kind_row.set_selected(kind_position(slot.kinds, action.kind_index()).unwrap_or(gtk::INVALID_LIST_POSITION));
// in connect_selected_notify:
let Some(&kind) = slot.kinds.get(row.selected() as usize) else { return };
if kind != (slot.get)(&a).kind_index() {
    (slot.set)(&a, slot.default_action(kind));
    a.rebuild_editor();
}
```

### Delay arm (`action_rows`)

An `adw::SpinRow` "Wait (ms)", 0–600 000 in steps of 100, subtitle `act.summary()`
("Wait 1.5 s"). `connect_value_notify` calls `(slot.set)(&a, Delay { ms })` and updates the
subtitle directly; no `rebuild_editor` (it would steal focus from the spin).

### Multi arm (`action_rows`)

The widget tree, inside the action's existing group:

```text
PreferencesGroup "Key 6"                       (or "Turn left" / "Press" for a dial)
 ├ ComboRow "When pressed" = Multi action
 ├ ActionRow "1 · Toggle mute (mic)"        [↑] [↓] [🗑]   activatable: selects the step
 ├ ActionRow "2 · Launch zoom.desktop"   ✎  [↑] [↓] [🗑]   ✎ = the selected step
 ├ ActionRow "3 · Wait 2 s"                 [↑] [↓] [🗑]
 ├ ActionRow "Add step"                               [+]   MenuButton → popover of kinds
 └ PreferencesGroup "Step 2"                               only while a step is selected
    ├ ComboRow "Action" (STEP_KINDS)
    └ that kind's rows (action_rows, recursively)
```

- The step group is added with `group.add(&step_group)`: libadwaita puts children that
  aren't rows below the group's list, so it shows under "Add step". (If it renders badly
  on the machine, add it to the page after the group instead and say so here.)
- Its contents: `action_editor(app, &step_group, "Action", &slot.step(n))`.
- Step row title: `format!("{} · {}", n + 1, step.summary())`; the selected one gets a
  `document-edit-symbolic` prefix icon. A nested multi's row has the subtitle
  "Multi-actions can't be nested" and the `error` CSS class.
- ↑ is insensitive on the first row, ↓ on the last. Each button edits through `edit_steps`
  and then `a.rebuild_editor()`; ↑/↓ keep the moved step selected, 🗑 clears the selection
  if it was that step and moves it up by one if it was after.
- "Add step": `gtk::MenuButton` (icon `list-add-symbolic`, flat) with a `gtk::Popover`
  holding a vertical `gtk::Box` of flat buttons, one per `STEP_KINDS` entry except
  "Nothing". A click pops the popover down, pushes `Action::empty_of_kind(kind)`, selects the
  new step and rebuilds.
- **Selecting a step:** `App` gets `pub selected_step: RefCell<Option<ActionTarget>>`, holding
  the selected step's own target (`ActionTarget::Step(of, n)`). A row's `connect_activated`
  sets it to `slot.step(n).target` (or clears it if that was already selected) and calls
  `a.rebuild_editor()`. `rebuild_editor` → `select(same)` → `build_key`/`build_dial` →
  `action_rows` builds the step group for the `n` where
  `Some(&ActionTarget::Step(Box::new(slot.target.clone()), n)) == selected_step.borrow().as_ref()`
  and `n < steps.len()`. Because the target includes the parent, a dial's three groups
  don't share a selection.
- `App::select` clears `selected_step` when `sel != self.selected.get()`. A rebuild
  (`select` with the same selection) keeps it.

```rust
/// Change the multi's step list: an owned copy is edited and written back, so no borrow
/// of the config is held across `set` (BUGS.md 4).
fn edit_steps(app: &Rc<App>, slot: &ActionSlot, f: impl FnOnce(&mut Vec<Action>)) {
    let mut act = (slot.get)(app);
    if let Action::Multi { steps } = &mut act {
        f(steps);
        (slot.set)(app, act);
    }
}
```

### Try it now

PR 11's `test_button(app, slot)`, for a `Multi`:
- with deck-owned steps: the deck-owned path (needs the daemon), tooltip "Try it on the deck";
- otherwise, with the daemon up: also `Command::Run` (the steps run in the daemon's order,
  volume through the watcher);
- otherwise: `gio::spawn_blocking(move || action::run_multi_blocking(&steps))`, then toast
  the first failure as "Step 2: script not found …";
- insensitive when it has no steps or only delays.

The preview needs nothing: a multi key shows its label, icon and glyph like any key.

## Tasks

Each task ends with `cargo fmt && cargo clippy --all-targets -- -D warnings` clean, plus the
check named.

### Task 1: core variants, kinds and summaries (~200 lines)

Files: `crates/omadeck-core/src/config.rs`.

`Multi`/`Delay`, `KINDS`, `KEY_KINDS`, `STEP_KINDS`, `kind_position`, `kind_index`,
`empty_of_kind`, `is_deck_owned`, `summary` (+ a `delay_summary(ms: u32) -> String` and
`multi_summary(steps: &[Action]) -> String`). New `match` arms elsewhere that stop compiling
(`action::run`, the GUI's `action_rows`) get a stub for now: `run` → `Ok(())`, `action_rows`
→ no rows.

Tests: `multi_roundtrip` (below), `multi_on_dial_press_roundtrip`,
`empty_multi_writes_type_only`, `delay_roundtrip` (`ms` kept; `type = "delay"` alone → 0),
`kinds_for_combos` (`kind_index` matches `KINDS` for every `empty_of_kind`; `KEY_KINDS` has
no 13, `STEP_KINDS` no 12; `kind_position(STEP_KINDS, 13) == Some(12)`,
`kind_position(STEP_KINDS, 12) == None`, and `STEP_KINDS[12] == 13`),
`multi_is_deck_owned_by_its_steps`, `delay_and_multi_summaries` (`Wait 500 ms`,
`Wait 1.5 s`, `Wait 2 s`, `Multi action (no steps)`, `Multi action (only waits)`,
`1 action: Launch a`, `3 actions: Launch a, …` with a leading delay not counted). The
existing `roundtrip` and `old_config_without_dials_loads` still pass.

```rust
#[test]
fn multi_roundtrip() {
    let mut c = Config::default();
    c.keys.get_mut(5).action = Action::Multi {
        steps: vec![
            Action::Volume { device: AudioDevice::Input, change: 0, mute: true },
            Action::App { command: "zoom.desktop".into() },
            Action::Delay { ms: 2000 },
            Action::Page { page: "Meeting".into() },
        ],
    };
    let s = toml::to_string_pretty(&c).unwrap();
    assert!(s.contains("[[key.action.steps]]"), "steps are tables, not inline arrays:\n{s}");
    assert!(s.contains("type = \"delay\"\nms = 2000"));
    let back: Config = toml::from_str(&s).unwrap();
    assert_eq!(c, back);
}
```

Check: `cargo test -p omadeck-core config::` → `test result: ok`, 0 failed, the 7 new tests listed.

### Task 2: sequencer and the blocking runner (~250 lines)

Files: `crates/omadeck-core/src/sequence.rs` (new), `crates/omadeck-core/src/lib.rs`
(`pub mod sequence;`), `crates/omadeck-core/src/action.rs`.

`Sequencer` as above; `run_multi_blocking`; `run`'s `Delay` and `Multi` arms (replacing
task 1's stub).

Tests in `sequence::tests`: `start_runs_leading_steps_now`, `delay_holds_the_rest`
(below), `delays_do_not_drift` (call `due` 300 ms late after a 500 ms delay that is followed
by another 500 ms delay: the last step is due at `t0 + 1000 ms`, not `t0 + 1300 ms`),
`restart_ignored_while_running` (and it starts again once finished),
`trailing_delay_keeps_it_running`, `two_keys_run_independently`, `next_due_is_earliest`,
`empty_multi_finishes_at_once`.

```rust
fn app(c: &str) -> Action {
    Action::App { command: c.into() }
}

#[test]
fn delay_holds_the_rest() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut s = Sequencer::default();
    assert!(s.start("k", vec![app("a"), Action::Delay { ms: 500 }, app("b"), app("c")], t0));
    assert_eq!(s.due(t0), vec![("k", 0, app("a"))]);
    assert_eq!(s.next_due(), Some(t0 + ms(500)));
    assert!(s.due(t0 + ms(499)).is_empty());
    assert_eq!(s.due(t0 + ms(500)), vec![("k", 2, app("b")), ("k", 3, app("c"))]);
    assert!(!s.is_running(&"k"));
    assert_eq!(s.next_due(), None);
}
```

Tests in `action::tests`: `multi_blocking_collects_errors`:
`run_multi_blocking(&[App { command: "" }, Delay { ms: 0 }, Url { url: "" }, Multi { steps: vec![] }, Back])`
returns step numbers `[0, 2, 3, 4]` with messages `no application set`, `no URL set`,
`multi-actions can't be nested` and groundwork's deck-owned bail. None of these spawn anything.

Check: `cargo test -p omadeck-core sequence:: action::` → `test result: ok`, 0 failed.

### Task 3: the daemon runs multis (~180 lines)

Files: `crates/omadeckd/src/daemon.rs`, `crates/omadeckd/src/main.rs`.

`RunOf` and its `Display`, `Source::Step` (and its `Display` arm, `step`), `Daemon.seq`,
`run_of`, the `Delay`/`Multi` arms at the top of `try_input`, `run_due_steps` called from
`run()` after `handle_commands`, `next_deadline`, the shutdown log, and the offline CLI
branch in `press_locally`/`dial_locally`.

Test (in `daemon.rs`'s `tests` module), for the one pure piece:

```rust
#[test]
fn run_of_display() {
    assert_eq!(RunOf::Key { page: None, index: 5 }.to_string(), "key 5");
    assert_eq!(RunOf::Key { page: Some("Media".into()), index: 5 }.to_string(), "key 5 on Media");
    assert_eq!(RunOf::Turn(2).to_string(), "dial 2");
    assert_eq!(RunOf::Press(2).to_string(), "dial 2");
    assert_eq!(RunOf::Ctl.to_string(), "omadeckd");
}
```

Check: `cargo test -p omadeckd daemon::tests::run_of_display` → `1 passed`, then
`cargo test -p omadeckd` → `test result: ok`, 0 failed. By hand with the example
config: `omadeckd press 5` runs all steps with the 2 s gap (the log shows
`key 5 step 3: hyprctl dispatch movetoworkspace 5` about 2 s after step 2).

### Task 4: kind lists, Delay editor and step slots (~150 lines)

Files: `crates/omadeck-gui/src/editor.rs`, `crates/omadeck-gui/src/app.rs`.

`ActionSlot.kinds` (`KEY_KINDS` in `build_key` and `ActionSlot::dial`), the combo mapping,
the `Delay` arm, `ActionTarget::Step` with `PartialEq` (and on `KeyRef`), its `action_at`,
`set_action_at` and `dial_slot` arms, `ActionSlot::step`, `edit_steps`, `App.selected_step`
and its reset in `select`.

No GTK test harness. The mapping is covered by task 1's `kinds_for_combos`.

Check: `cargo build -p omadeck-gui` clean. By hand: a key's kind combo has no "Delay"; a hand-
edited `type = "delay"` key shows an empty combo instead of the wrong kind.

### Task 5: the step list and Try it now (~250 lines)

Files: `crates/omadeck-gui/src/editor.rs`.

The `Multi` arm (rows, buttons, "Add step" popover, the step group) and the `Multi` case in
`test_button`.

Check: `cargo build -p omadeck-gui` clean. By hand: build the "Meeting" key from the
example; the saved `config.toml` matches the example's shape; reorder and remove steps;
pick an app for a step, then switch key before the picker closes: the toast says the change
was dropped.

### Task 6: docs (~40 lines)

Files: `README.md`, `examples/config.toml`, `CHANGELOG.md`.

- README Features: multi actions, with the "What order the steps keep" list in short.
  Roadmap: tick "Multi-actions" `[x]`.
- `examples/config.toml`: the "Meeting" key above, commented out.
- CHANGELOG, Unreleased / Added: "A `multi` action runs several actions in order from one
  key or dial, with `delay` steps in between, as in the Elgato app's Multi Action. The
  configurator edits the steps as a list."

Check: `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → every crate
`test result: ok`.

## Known traps

- **No `impl Fn` builders.** `action_editor` → `action_rows` → `action_editor` for a step only
  compiles because `ActionSlot` holds `Rc<dyn Fn…>`. Don't add a generic parameter to either.
- **BUGS.md 4:** never `if let Some(k) = a.config.borrow().key(..) { edit(..) }`. Every step
  edit goes through `(slot.get)(a)` or `action_at`, which return owned clones, and only then
  `(slot.set)` / `set_action_at`.
- **No `rebuild_editor` from a spin or entry handler** (focus is lost). The Delay spin and
  text rows edit in place; only structural changes (kind, add, move, remove, select) rebuild.
- **Popover cycles:** a button inside the popover that captures the popover keeps both alive.
  Capture `pop.downgrade()` and `upgrade()` in the handler.
- **Nothing slow on the loop:** a delay is a deadline in the sequencer, never a sleep.
  `run_multi_blocking` sleeps, so it runs only off the loop (CLI main thread, `spawn_blocking`).
- **`run_due_steps` borrows:** `self.seq.due(now)` returns an owned `Vec`, so the loop over it
  can call `self.try_input`.
- A step's number is from 0 in code and from 1 in every message and title.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] The "Meeting" example: the mic mutes (OSD shows), Zoom launches, 2 s later the window
      moves to workspace 5, and the deck shows page Meeting.
- [ ] Pressing the key again during the 2 s delay does nothing (log: ignored); after it ends
      it runs again.
- [ ] Other keys and dials stay responsive during a 10 s delay (turn the volume dial).
- [ ] `brightness set = 20`, `delay 1000`, `brightness set = 80`: the deck dims and comes back;
      a brightness dial's panel follows both.
- [ ] Volume `-50`, `+50` back to back on a dial press: the level ends where it started.
- [ ] A multi on a dial press and on a strip tap runs. A failing step toasts "Dial 2: step 1:
      script not found …" and the next step still runs.
- [ ] `omadeckd press 5` (through the daemon) runs the whole multi, page step included.
- [ ] Unplug during a delay: the run finishes (log), no panic; replug shows the page it moved to.
- [ ] Edit the config mid-delay: the old steps finish.
- [ ] GUI: add, reorder, remove steps; each step's editor works (app picker, file browse,
      Hyprland, volume, delay spin); Try it now runs the steps.
- [ ] A config with no multi loads and saves byte-identical.

## Decisions to explain

- **Delay is an `Action` variant**, as Elgato lists it among actions. The TOML stays uniform
  and every step reuses `action_editor`. A top-level Delay in a hand-edited file does
  nothing, and the GUI never offers it.
- **Nesting is refused at run time**, so one bad step doesn't throw away the config.
- **Second press while running: ignored.** Elgato's behaviour is unconfirmed; ignoring never
  doubles side effects or cuts a run in half.
- **A failing step doesn't stop the rest** (Elgato unconfirmed); each failure is reported.
- **No implicit gap between steps.** Elgato's 100 ms default is for virtual hotkey presses,
  which omadeck doesn't have. Steps start in order; add a Delay where an app needs time.
- **Steps don't wait for their process** (see "What order the steps keep").
- **The sequencer is on the deck loop, not a thread.** Every step ends up on the loop anyway
  (deck-owned steps need it), so a timer queue polled by the loop is less code and a pure,
  testable type.
- **A multi on a dial turn runs once per turn event.** Elgato may not allow multis on dials
  at all (unconfirmed); omadeck's dial actions are already "any action".
- **Steps move with ↑/↓ buttons**, not drag and drop as in Elgato's editor. Simpler for now.

## Ask before building

1. A failing step doesn't stop the rest (decided). Would you rather a failure stop the
   multi, as a script with `set -e` would?
2. A second press while a multi runs is ignored (decided). Restart or queue instead?

## Later

- **Multi Action Switch**: once PR 8's toggle key has an action per state, each can be a
  `multi`, which gives Elgato's switch with real state on top.
- **Random Action**; per-step "Next action" gaps; choosing a state for a two-state step.
- Hold to cancel a running multi; PR 8's busy state until the last step ends.
- Drag-and-drop reorder and duplicating a step.
- Batching consecutive Hyprland steps into one `hyprctl --batch`.
- Waiting for a volume step to land before the next step (an ack from the watcher).
- `MultiStarted` / `MultiFinished` hub events.

**Size:** about 1 070 lines with tests: core ~450, daemon ~180, GUI ~400, docs ~40.
