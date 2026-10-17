# PR 12: `feat/multi-actions`: one key runs several actions, with delays

Part of [PLAN.md](PLAN.md). Base: `feat/ctl` (PR 11), which sits on PRs 10, 9, 8,
`fix/review-2` and `feat/groundwork`. It needs groundwork's `run_input`, `next_deadline`,
watcher and `ActionSlot`, fix/review-2's action walker (R2), Hyprland worker (R6) and
exhaustive `action_at`/`set_action_at` (R9), and PR 11's `try_input`, `Source::Ctl`,
`Command::Run`, `route` and the offline `press_locally`/`dial_locally`. No IPC change.

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
- A failed step doesn't stop the rest (unconfirmed: check in the Elgato app).
- **Unconfirmed:** what a second press does while a multi runs; whether a multi can go on
  a Stream Deck + dial; the maximum number of steps.

## What this plan uses from the base

Copied from the code at `55d0d62`, plus what `fix/review-2`, PR 10 and PR 11 change (each
marked). **Before task 1, grep each name below on `feat/ctl`.** If one differs, use the
merged name and fix this list.

```bash
grep -n "pub const KINDS\|pub fn kind_index\|pub fn is_deck_owned\|pub fn empty_of_kind\|pub fn default_for\|pub fn summary\|fn all_actions\|fn actions" crates/omadeck-core/src/config.rs
grep -n "pub fn run\b\|pub fn run(\|pub fn run_steps\|pub fn apply_volume\|fn run_refuses_exactly" crates/omadeck-core/src/action.rs
grep -n "pub enum Source\|pub fn try_input\|pub fn run_input\|fn press_key\|pub fn next_deadline\|pub fn shutdown\|pub struct KeyId" crates/omadeckd/src/daemon.rs
grep -n "fn press_locally\|fn dial_locally\|handle_commands()" crates/omadeckd/src/main.rs
grep -n "pub enum ActionTarget\|pub struct KeyRef\|pub fn action_at\|pub fn set_action_at\|pub fn select\|pub fn add_page" crates/omadeck-gui/src/app.rs
grep -n "pub(crate) struct ActionSlot\|fn action_editor\|fn action_rows\|fn test_button\|enum Route\|fn route" crates/omadeck-gui/src/editor.rs
```

```rust
// omadeck-core/src/config.rs. Action is #[serde(tag = "type", rename_all = "snake_case")],
// derives Debug, Clone, Default (None), PartialEq, Serialize, Deserialize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]   // PR 11 adds Serialize, Deserialize, rename_all
pub enum Direction { Left, Right, Press }      // fix/review-2 adds Direction::ALL
impl Action {
    pub const KINDS: [&'static str; 12] = [    // 11 entries at 55d0d62; PR 10 adds "App volume" at 11
        "Nothing", "Launch app", "Run script", "Play sound", "Open website", "Hyprland",
        "Open page", "Go back", "Volume", "Next/previous page", "Brightness", "App volume",
    ];
    pub fn is_deck_owned(&self) -> bool {      // a plain matches!, not recursive
        matches!(self, Action::Page { .. } | Action::Back | Action::PageStep { .. } | Action::Brightness { .. })
    }
    pub fn kind_index(&self) -> usize;         // exhaustive match, 0..=11 after PR 10
    pub fn is_none(&self) -> bool;
    pub fn is_trackable(&self) -> bool;        // App (not .desktop) and Script only
    pub fn empty_of_kind(i: usize) -> Self;    // `_ => Action::None`
    pub fn default_for(kind: usize, dir: Direction) -> Self;
    pub fn summary(&self) -> String;           // "Launch {command}", "hyprctl dispatch {d}", "Open page {p}", "Go back", …
}
// PR 10: Action::AppVolume { app: String, focused: bool, change: i8, mute: bool }.
impl KeyConfig { pub fn is_blank(&self) -> bool; }   // `self.action == Action::None` among others
impl Config {
    pub fn links_to(&self, name: &str) -> usize;     // counts Action::Page { page == name } over all_actions()
    pub fn rename_page(&mut self, old: &str, new: &str); // over all_actions_mut()
    pub fn remove_page(&mut self, name: &str);           // Page → Action::None over all_actions_mut()
}
// fix/review-2 (R2): ONE recursive walk. Today no action holds other actions, so both
// bodies are just `f(self);`. PR 12 adds the `Multi` arm to both, nothing else.
impl Action {
    pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Action));
    pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Action));
}
impl KeyConfig {                                  // action, then on.action, each through walk
    pub fn actions(&self) -> Vec<&Action>;
    pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action));
}
impl DialConfig {                                 // slots in Direction::ALL order, through walk
    pub fn actions(&self) -> Vec<&Action>;
    pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action));
}
impl Config {                                     // pub since fix/review-2; built on the two above
    pub fn all_actions(&self) -> Vec<&Action>;
    pub fn all_actions_mut(&mut self, f: &mut dyn FnMut(&mut Action));
}

// omadeck-core/src/action.rs
pub fn run(action: &Action) -> Result<()>;      // starts with: if action.is_deck_owned() { bail!("{} only works on the deck", KINDS[kind_index]) }
                                                // its match lists the deck-owned arm `=> Ok(())`
pub fn run_steps(action: &Action, steps: u8) -> Result<()>;
pub fn apply_volume(device: AudioDevice, change: i8, mute: bool);   // blocking; the watcher's call
// test `run_refuses_exactly_the_deck_owned_kinds` loops over every kind and calls
// `run(&Action::empty_of_kind(kind))`. It skips 8 (Volume, which would really change the
// volume) and, since PR 10, 11 (App volume, which runs pactl). This PR adds no skip: an
// empty Multi spawns a `multi` thread that does nothing, and Delay is `Ok(())` (task 2).
// The module imports `std::thread` but not `std::time::Duration` (its tests do).

// omadeckd/src/daemon.rs
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId { pub page: Option<String>, pub index: u8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick, Ctl }   // Ctl: PR 11
// Display: "key {i}", "dial {i}" (Turn/Press/Tap), "strip flick", "omadeckd" (Ctl).
pub struct Daemon { /* … */ pub hypr: HyprWorker, /* … */ }   // fix/review-2 (R6); PR 9's listener is `hypr_events`
impl Daemon {
    /// PR 11. Logs, dispatches, show_page if moved, and on failure calls `report_failure`
    /// for `src`. Returns the error too.
    pub fn try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()>;
    /// fix/review-2 (R6): the one failure `match src`, used by `try_input` and `poll_hypr`.
    /// Logs `{src}: {message}`; `Key` → ActionFailed (with `page`); Turn/Press/Tap →
    /// DialActionFailed; `Flick` → nothing; PR 11 added `Source::Ctl => {}`.
    fn report_failure(&self, src: Source, page: Option<String>, message: String);
    /// fix/review-2 (R6): every pass, `report_failure` for each `HyprFailed` on `self.hypr.failed`.
    pub fn poll_hypr(&mut self);
    /// fix/review-2 (R6) added `src`, for the Hyprland worker's job.
    fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>;
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool;  // try_input(..).is_ok()
    fn press_key(&mut self, i: u8) -> Result<()>;        // PR 11: job running → ignored; tracked; else try_input(Source::Key(i)) + after_toggle_press
    pub fn next_deadline(&self) -> Option<Instant>;       // [flash, self.repaint_at, self.reload_at].into_iter().flatten().min()
    pub fn shutdown(self);                                // deck reset, hub.shutdown, watcher.stop, saver.finish
}
// dispatch: Volume → guess_volume + WatchCmd::Volume (fix/review-2 R5: always acknowledged);
// PR 10: AppVolume → WatchCmd::AppVolume { query, change, mute }; deck-owned on nav/rt;
// fix/review-2 R6: Hyprland → `self.hypr.send(HyprJob { src, page, dispatch, steps })` on
// the one ordered worker, Ok once queued; a later failure comes back as `HyprFailed { src,
// page, message }` and `poll_hypr` passes it to `report_failure`.
// The rest → action::run_steps.
// Event::ActionFailed { index: u8, page: Option<String>, message: String }
// Event::DialActionFailed { index: u8, message: String }

// omadeckd/src/main.rs (PR 11)
fn press_locally(index: u8, page: Option<&str>) -> i32;   // offline `press`: action::run, 300 ms sleep
fn dial_locally(index: u8, dir: Direction, steps: u8) -> i32;
// run() order each iteration (after this PR): drain notify → 1 s stamp check → reload()
// when due → handle_commands() → run_due_steps(Instant::now()) (PR 12) → apply_values() →
// apply_hypr() (PR 9) → poll_jobs() → poll_hypr() (fix/review-2) → no deck: flush(),
// every 2 s deck::open → attach, else sleep wait_for(now, next_deadline()); deck: pass(),
// Err → detach.

// omadeck-gui/src/app.rs
#[derive(Clone)] pub struct KeyRef { page: Option<String>, index: u8 }   // no PartialEq
#[derive(Clone)]
pub enum ActionTarget { Key(KeyRef), KeyOn(KeyRef), DialTurnLeft(u8), DialTurnRight(u8), DialPress(u8) }
impl ActionTarget {
    pub fn dial(i: u8, dir: Direction) -> Self;
    pub fn dial_slot(&self) -> Option<(u8, Direction)>;   // exhaustive; Key | KeyOn => None
}
impl App {
    pub fn action_at(&self, at: &ActionTarget) -> Option<Action>;            // None if its page has gone
    pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool;
    // fix/review-2 (R9): both are ONE exhaustive `match at { … }`, one arm per variant, no
    // wildcard: dials go through `dial_action(&cfg, *i, dir)` and
    // `self.set_dial_action(*i, dir, act)`. PR 12 adds the `ActionTarget::Step` arm to both.
    pub fn select(self: &Rc<Self>, sel: Option<Selection>);
    // fix/review-2 (R1): select starts with
    //     if sel != self.selected.get() { self.face_tab.set(FaceTab::Off); }
    // before `self.selected.set(sel)`. PR 12 clears `selected_step` inside that branch.
    pub fn rebuild_editor(self: &Rc<Self>);                    // idle select(selected)
    pub fn add_page(self: &Rc<Self>, name: &str, folder: bool) -> String;
    pub fn toast(&self, msg: &str);
    pub fn daemon_up(&self) -> bool;                           // PR 11
}

// omadeck-gui/src/editor.rs
type FaviconHandler = Rc<dyn Fn(&Rc<App>, ActionTarget, String, String) + 'static>;
type ActionRun = Rc<dyn Fn(&Rc<App>) -> Action>;
#[derive(Clone)]
#[allow(clippy::type_complexity)]
pub(crate) struct ActionSlot {
    pub target: ActionTarget,
    pub dir: Option<Direction>,
    pub get: Rc<dyn Fn(&Rc<App>) -> Action>,
    pub set: Rc<dyn Fn(&Rc<App>, Action)>,
    pub own_page: Option<String>,
    pub on_app_picked: Rc<dyn Fn(&Rc<App>, &ActionTarget, String, String, Option<String>)>,
    pub on_new_page: Rc<dyn Fn(&Rc<App>) -> String>,
    pub on_favicon: Option<FaviconHandler>,
}
impl ActionSlot { pub fn dial(i: u8, dir: Direction) -> Self; pub fn default_action(&self, kind: usize) -> Action; }
// Slots are built in three places: build_key's `slot` (Key), build_key's `on_slot` (KeyOn),
// and ActionSlot::dial.
fn action_editor(app: &Rc<App>, group: &adw::PreferencesGroup, combo_title: &str, slot: &ActionSlot);
// today: gtk::StringList::new(&Action::KINDS), kind_row.set_selected(action.kind_index() as u32),
// connect_selected_notify: `sel != (slot.get)(&a).kind_index()` → (slot.set)(&a, slot.default_action(sel)); a.rebuild_editor()
fn action_rows(app: &Rc<App>, group: &adw::PreferencesGroup, slot: &ActionSlot);
// fix/review-2 (R9): action_rows never calls group.set_description; an explanation is
// `group.add(&hint_row(text))`, with `fn hint_row(text: &str) -> adw::ActionRow`.
fn test_button(app: &Rc<App>, slot: &ActionSlot, run: ActionRun) -> gtk::Button;   // PR 11: uses slot.target
// PR 11: enum Route { Nothing, ShowPage(String), Press { index: u8, page: String }, Daemon, NeedsDaemon, Local };
//        fn route(act: &Action, key: Option<(u8, Option<&str>)>, daemon_up: bool) -> Route;
//        `key` is Some only for a key's own ▶ (slot.target is ActionTarget::Key): with the daemon up
//        that is Press → Command::Press for the key (flash, toggle flip, as on the deck). Every
//        other ▶ (a dial's, the Toggle group's on action) is Daemon → Command::Run for that action only.
//        Both via ipc::request in gio::spawn_blocking; Local → action::run in gio::spawn_blocking.
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

The steps are arrays of tables in the key's action table. Checked with `toml` 1.1.8
(`to_string_pretty`, 2026-10-17): it writes `[[key.action.steps]]` and
`[[dial.press.steps]]`, not inline arrays, an empty multi as `type = "multi"` alone, and
reads them all back equal.

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

Decided: **Multi action is kind 12, Delay kind 13.**

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

All four are associated items on `Action` (like `KINDS`), so nothing new needs
re-exporting from `lib.rs`.

- `kind_index`: `Multi { .. } => 12`, `Delay { .. } => 13`.
- `empty_of_kind` (and so `default_for`): `12 => Multi { steps: vec![] }`,
  `13 => Delay { ms: 500 }`.
- `is_deck_owned` becomes a `match`:
  `Action::Multi { steps } => steps.iter().any(Action::is_deck_owned)`, the four deck-owned
  kinds `=> true`, `_ => false`.
- **The walker (R2):** `Action::walk` and `Action::walk_mut` each get the arm that visits a
  `Multi`'s steps, recursively, after `f(self)`:
  ```rust
  pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Action)) {
      f(self);
      if let Action::Multi { steps } = self {
          for s in steps {
              s.walk(f);
          }
      }
  }
  ```
  and the same in `walk_mut` with `steps.iter_mut()` and `s.walk_mut(f)`. Then
  `KeyConfig::actions`, `DialConfig::actions`, `Config::all_actions(_mut)`, and so
  `links_to`, `rename_page` and `remove_page`, reach a page step with no other change.
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
/// offline CLI and the configurator's Try it now with no daemon). Delays sleep; a volume
/// step waits for its command; a Hyprland step waits for hyprctl's answer. Deck-owned
/// steps fail (`run` bails), and so do nested multis. Returns each failed step's number
/// (from 0) and error; the other steps still run.
pub fn run_multi_blocking(steps: &[Action]) -> Vec<(usize, anyhow::Error)>;
```

- `Delay { ms }` → `thread::sleep(Duration::from_millis(u64::from(*ms)))`.
- `Multi { .. }` → error `multi-actions can't be nested`.
- `Volume { device, change, mute }` → `apply_volume(*device, *change, *mute)` (blocking, so
  it's done before the next step; it returns nothing, so a volume step never fails here).
- anything else → `run(step)`, PR 10's `AppVolume` included (spawned and not waited for).

`run` gains:
- Before the generic deck-owned bail: a deck-owned `Multi` →
  `bail!("this multi action has page or brightness steps, which only work on the deck")`.
- `Delay { .. } => Ok(())`.
- `Multi { steps }` → spawn a thread named `multi` that calls `run_multi_blocking(&steps)`
  and logs each failure as `step {n + 1}: {e:#}`; `Ok(())`. The daemon never calls `run` on
  a multi.

## The sequencer (new `omadeckd/src/sequence.rs`, pure)

It lives in the daemon crate: nothing else uses it. `main.rs` adds `mod sequence;`.

```rust
use std::time::{Duration, Instant};

use omadeck_core::Action;

use crate::daemon::KeyId;

/// Which input started a multi: what "ignored while running" compares, and where a
/// failed step is reported.
#[derive(Debug, Clone, PartialEq)]
pub enum RunOf {
    Key(KeyId),
    /// Either direction: a left and a right multi on one dial don't overlap.
    Turn(u8),
    /// A dial press or a tap on its panel (the same action).
    Press(u8),
    /// `omadeckd` (a `Command::Run`: a nested or dial ▶, `page next`; or a press on a page
    /// the deck isn't showing). A key's own ▶ is a `Press`, so it is `Key`.
    Ctl,
}
impl std::fmt::Display for RunOf { /* "key 5", "key 5 on Media", "dial 2", "omadeckd" */ }

/// Running multi actions, timed by whoever polls it (the daemon's loop). Pure: no
/// threads, no clock of its own, so it's unit-tested with made-up instants.
#[derive(Debug, Default)]
pub struct Sequencer { runs: Vec<Run> }

#[derive(Debug)]
struct Run { of: RunOf, steps: Vec<Action>, next: usize, due: Instant }

impl Sequencer {
    /// Start `steps` for `of`. False (and nothing starts) while that input's last run is
    /// still going. An empty list starts nothing and returns true.
    pub fn start(&mut self, of: RunOf, steps: Vec<Action>, now: Instant) -> bool;
    /// Every step whose time has come, in order, as (run, step number from 0, action).
    /// A delay moves its run's deadline on from the previous deadline (so lateness doesn't
    /// add up) and isn't returned. A run is dropped once its last step, or its trailing
    /// delay, is done.
    pub fn due(&mut self, now: Instant) -> Vec<(RunOf, usize, Action)>;
    /// The earliest deadline of a waiting run, for the loop's read timeout.
    pub fn next_due(&self) -> Option<Instant>;
    pub fn is_running(&self, of: &RunOf) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
}
```

`due` is a `retain_mut` over the runs:

```rust
pub fn due(&mut self, now: Instant) -> Vec<(RunOf, usize, Action)> {
    let mut out = Vec::new();
    self.runs.retain_mut(|run| {
        while run.due <= now {
            let Some(step) = run.steps.get(run.next) else { return false };
            let n = run.next;
            run.next += 1;
            match step {
                Action::Delay { ms } => run.due += Duration::from_millis(u64::from(*ms)),
                _ => out.push((run.of.clone(), n, step.clone())),
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
- **All ctl multis share `RunOf::Ctl`**: a second `Run` of a multi (a dial's ▶, the Toggle
  group's ▶) while one runs is ignored. A key's own ▶ is a `Press`, so it is `RunOf::Key`
  and is ignored while that key's multi runs, as on the deck.

## Daemon (`omadeckd/src/daemon.rs`, `main.rs`)

```rust
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick, Ctl, Step }  // + Step (Display: "step")

pub struct Daemon { /* … */ seq: Sequencer }   // Sequencer::default() in Daemon::new

impl Daemon {
    /// `Key` (with the shown page) / `Turn` / `Press` and `Tap` / `Ctl` → its RunOf;
    /// `Flick` and `Step` → None (a step can't start a multi). Exhaustive: PR 16's
    /// `LongTap` must pick one.
    fn run_of(&self, src: Source) -> Option<RunOf>;
    /// Start a multi for `src` and run its leading steps now.
    fn start_multi(&mut self, src: Source, steps: &[Action]) -> Result<()>;
    /// Run the steps whose time has come. `main.rs`'s `run()` calls it on every
    /// iteration, right after `handle_commands`, deck or not.
    pub fn run_due_steps(&mut self, now: Instant);
}
```

- **`try_input`**, at the top, before it logs and dispatches:
  - `Action::Delay { .. } => return Ok(())`.
  - `Action::Multi { steps } => return self.start_multi(src, steps)`.
  - `report_failure`'s `match src` gains `Source::Step => {}` next to PR 11's
    `Source::Ctl => {}` (`run_due_steps` reports a step's failure against its run, below).
    It's the one match, so a Hyprland step that fails later on the worker and comes back
    through `poll_hypr` with `Source::Step` is logged and publishes nothing.
- **`start_multi`:** `let Some(of) = self.run_of(src) else { bail!("multi-actions can't be nested") }`.
  Log `{src}: {summary}`. If `!self.seq.start(of.clone(), steps.to_vec(), Instant::now())`,
  log `{of}: multi action still running; ignored` and return `Ok(())`. Else
  `self.run_due_steps(Instant::now())` (the leading steps run in this pass, so a page step
  shows at once) and `Ok(())`. A turn's `steps` count is ignored: one run per turn event.
- **`press_key`:** right after it picks `act`, if `act` is a `Multi` and
  `self.seq.is_running(&RunOf::Key(id.clone()))`, log `key {i}: multi action still running,
  press ignored` and return `Ok(())`, like the running-job check above it. So an ignored
  press never flips a toggle.
- **`run_due_steps`:**

```rust
pub fn run_due_steps(&mut self, now: Instant) {
    for (of, n, step) in self.seq.due(now) {
        log::info!("{of} step {}: {}", n + 1, step.summary());
        if let Err(e) = self.try_input(Source::Step, &step, 1) {
            let message = format!("step {}: {e:#}", n + 1);
            log::warn!("{of}: {message}");
            match of {
                RunOf::Key(KeyId { page, index }) => self.hub.publish(Event::ActionFailed { index, page, message }),
                RunOf::Turn(index) | RunOf::Press(index) => {
                    self.hub.publish(Event::DialActionFailed { index, message })
                }
                RunOf::Ctl => {}
            }
        }
    }
}
```

- **Timing:** `next_deadline` adds `self.seq.next_due()` to its list, so `pass`'s read
  timeout (and the no-deck sleep) shrinks to the next step. A delay is accurate to a few
  ms, and with no multi running the loop wakes as often as before.
- **Shutdown:** in `Daemon::shutdown`, if `!self.seq.is_empty()`, log
  `dropping {n} running multi actions`. Spawned children are not affected.
- **`omadeckd press` through the daemon** (PR 11), and a key's own ▶ (a `Command::Press`),
  run a multi key like a press: `Source::Key` → `RunOf::Key`. `Command::Run { Multi }` (a
  dial's ▶ or the Toggle group's ▶) is `RunOf::Ctl`; its reply is the multi's summary
  (PR 11's `run_message`), and failed steps are only logged.

### What order the steps keep

Decided, and written in the README:
- Steps **start** in list order. The sequencer never waits for a step's process to exit:
  apps and scripts can run for hours, and Elgato doesn't wait for a launched app either.
- A **deck-owned** step (page, back, page step, brightness) has taken effect before the
  next step runs: it's done on the loop.
- A **Hyprland** step is **queued in order** on fix/review-2's one Hyprland worker before the
  next step runs (R6). Hyprland steps keep their order among themselves and with every
  other Hyprland action, but the next step does **not** wait for Hyprland's answer. Put a
  `delay` after it when the next step needs the window moved first.
- A **volume** or **app volume** step goes through `try_input` → `dispatch`, which queues it
  on the watcher (`WatchCmd::Volume` / PR 10's `WatchCmd::AppVolume`, never `action::run`).
  The watcher applies volume changes one at a time in the order sent, so volume steps keep
  their order among themselves and against the dials. The next step doesn't wait for it: a
  launch right after a mute can start a few ms before the mute lands. Put a `delay` of
  100 ms between them when it matters.
- App, script, sound and website steps: the process has been spawned before the next step.

### Lifecycle

Decided:
- **Decided: a second press while running is ignored**, logged. It never doubles side
  effects (two Zooms, a page flipping twice) and never cuts a run off halfway.
- **Decided: a failing step doesn't stop the rest**, believed to match Elgato's Multi Action (unconfirmed). A
  per-multi "stop on failure" is under Later. Each failure is reported with its number, e.g.
  the toast "Key 6: step 2: script not found …". A Hyprland step that fails later, on the
  worker, is only logged: the worker reports it with `Source::Step`, which publishes nothing.
- **Page change:** runs go on. Steps were cloned at start, so the page shown doesn't matter.
- **Config reload:** running sequences keep their cloned steps. New presses use the new config.
- **Unplug:** runs go on (the sequencer is in `Daemon`, not `Deck`). Deck-owned steps act on
  `nav`/`Runtime` and show on reconnect, as for brightness.

### Offline CLI

PR 11's `press_locally` / `dial_locally`: a `Multi` that isn't deck-owned goes to
`action::run_multi_blocking` (on the main thread, so delays aren't cut short by the exit);
each failure is printed as `step N: …` on stderr, and the exit code is 1 if any failed. A
deck-owned multi fails with `run`'s message, exit 1. For a dial turn, `steps` is ignored
(one run per turn event).

## GUI (`omadeck-gui`)

### A step is an `ActionTarget`

`ActionTarget` gets the variant groundwork named for PR 12, and `PartialEq` (so does
`KeyRef`), so a selection can be compared:

```rust
#[derive(Clone, PartialEq)]
pub enum ActionTarget {
    Key(KeyRef),
    KeyOn(KeyRef),
    DialTurnLeft(u8),
    DialTurnRight(u8),
    DialPress(u8),
    /// Step `n` (from 0) of the multi action at the inner target.
    Step(Box<ActionTarget>, usize),
}
```

- `dial_slot()`: `ActionTarget::Step(..) => None`.
- `App::action_at`, a `Step(of, n)` arm (next to the `DialPress` arm of fix/review-2's
  exhaustive match):
  `ActionTarget::Step(of, n) => Some(match self.action_at(of)? { Action::Multi { steps } => steps.get(*n).cloned().unwrap_or_default(), _ => Action::None })`.
- `App::set_action_at`, a `Step(of, n)` arm (next to `DialPress`): take `self.action_at(of)` (an owned clone, the
  borrow is already dropped). If it's a `Multi` with `n < steps.len()`, replace `steps[n]`
  and return `self.set_action_at(of, Action::Multi { steps })`. Otherwise toast "That step
  was removed, so the change was dropped" and return false.

Since R9 made both matches exhaustive, the compiler points at both places. Late results
(the app picker, a file dialog) land on the step they were started for, through the same
path a key's or dial's do, and a step inside a key on a deleted page is dropped by
`edit_ref` as today.

### The slot

`ActionSlot` gains `pub kinds: &'static [usize]`: `Action::KEY_KINDS` in all three places
slots are built today (`build_key`'s `slot` and `on_slot`, `ActionSlot::dial`). And a
constructor:

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
            // A new page for a step doesn't restyle the key as a folder (new_folder would).
            on_new_page: Rc::new(|a| a.add_page("Page", true)),
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
kind_row.set_selected(Action::kind_position(slot.kinds, action.kind_index()).unwrap_or(gtk::INVALID_LIST_POSITION));
// in connect_selected_notify:
let Some(&kind) = slot.kinds.get(row.selected() as usize) else { return };
if kind != (slot.get)(&a).kind_index() {
    (slot.set)(&a, slot.default_action(kind));
    a.rebuild_editor();
}
```

Subtitles next to the existing ones: Multi "Runs its steps in order, top to bottom",
Delay "Waits before the next step".

### Delay arm (`action_rows`)

An `adw::SpinRow` "Wait (ms)", `with_range(0.0, 600_000.0, 100.0)`, subtitle
`act.summary()` ("Wait 1.5 s"). `connect_value_notify` calls `(slot.set)(&a, Delay { ms })`
and updates the subtitle directly; no `rebuild_editor` (it would steal focus from the spin).

### Multi arm (`action_rows`)

If `slot.target` is itself a `Step` (a hand-edited nested multi), add one `ActionRow`
"Multi-actions can't be nested" with the `error` CSS class and stop. Otherwise the widget
tree, inside the action's existing group:

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
- Its contents: `let step = slot.step(n);`, a ▶ in its header,
  `step_group.set_header_suffix(Some(&test_button(app, &step, step.get.clone())));`, then
  `action_editor(app, &step_group, "Action", &step)`. The step's target is a `Step`, so
  its ▶ sends `Command::Run` for that step only (PR 11's rule for a nested ▶).
- Step row title: `format!("{} · {}", n + 1, step.summary())`; the selected one gets a
  `document-edit-symbolic` prefix icon. A nested multi's row has the subtitle
  "Multi-actions can't be nested" and the `error` CSS class.
- ↑ is insensitive on the first row, ↓ on the last. Each button edits through `edit_steps`,
  moves the selection with `selection_after`, then `a.rebuild_editor()`.
- "Add step": `gtk::MenuButton` (icon `list-add-symbolic`, flat) with a `gtk::Popover`
  holding a vertical `gtk::Box` of flat buttons, one per `STEP_KINDS` entry except
  "Nothing". A click pops the popover down, pushes `Action::empty_of_kind(kind)`, selects the
  new step and rebuilds.
- **Selecting a step:** `App` gets `pub selected_step: RefCell<Option<ActionTarget>>`
  (`RefCell::default()` in `App::new`), holding the selected step's own target
  (`ActionTarget::Step(of, n)`). A row's `connect_activated` sets it to `slot.step(n).target`
  (or clears it if that was already selected) and calls `a.rebuild_editor()`. The rebuild
  → `select(same)` → `build_key`/`build_dial` → `action_rows` builds the step group for the
  `n` where `Some(&slot.step(n).target) == selected_step.borrow().as_ref()` and
  `n < steps.len()`. Because the target includes the parent, a dial's three groups don't
  share a selection.
- `App::select` clears `selected_step` when the selection changes, in the branch
  fix/review-2 added for `face_tab` (R1):
  ```rust
  if sel != self.selected.get() {
      self.face_tab.set(FaceTab::Off);
      self.selected_step.borrow_mut().take();
  }
  ```
  A rebuild (`select` with the same selection) keeps it.

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

/// One structural edit of a step list.
#[derive(Debug, Clone, Copy)]
enum StepEdit { Up(usize), Down(usize), Remove(usize), Add }

/// Where the selected step is after `edit` on a list of `len` steps (before the edit).
/// A moved step stays selected; removing the selected one clears it; removing one above
/// moves it up; Add selects the new last step. Pure, for the test.
fn selection_after(sel: Option<usize>, edit: StepEdit, len: usize) -> Option<usize>;
```

### Try it now

PR 11's `test_button` and `route` already do most of it, since `is_deck_owned` is now
recursive. PR 11's rule holds: the key's own ▶ presses the key; a step's ▶ (above), a
dial's ▶ and the Toggle group's ▶ run only their action.
- Build: a `Multi` with no non-delay steps is insensitive, tooltip "Add a step first"; a
  top-level `Delay` is insensitive, "Delay only works inside a multi action". A deck-owned
  multi gets PR 11's "Try it on the deck".
- `Route::Press` (a multi key's own ▶, daemon up): `Command::Press` for the key; the
  daemon runs the multi as for a deck press (`RunOf::Key`), so a second ▶ while it runs
  is ignored.
- `Route::Daemon` (daemon up; a dial's ▶, the Toggle group's ▶, a step's ▶):
  `Command::Run` as for any action; a multi's steps run in the daemon's order, volume
  through the watcher.
- `Route::NeedsDaemon` (deck-owned steps, no daemon): PR 11's toast.
- `Route::Local` with a `Multi`: `gio::spawn_blocking(move || action::run_multi_blocking(&steps))`,
  then toast the first failure as "Step 2: script not found …". Other actions as in PR 11.

The preview needs nothing: a multi key shows its label, icon and glyph like any key.

## Tasks

Each task ends with `cargo fmt && cargo clippy --all-targets -- -D warnings` clean, plus the
check named.

### Task 1: core variants, kinds, summaries and the walker arm (~220 lines)

Files: `crates/omadeck-core/src/config.rs`.

`Multi`/`Delay`, `KINDS`, `KEY_KINDS`, `STEP_KINDS`, `kind_position`, `kind_index`,
`empty_of_kind`, `is_deck_owned`, `summary` (+ a `fn delay_summary(ms: u32) -> String` and
`fn multi_summary(steps: &[Action]) -> String`, next to `volume_summary`), and the walker's
`Multi` arm. New `match` arms elsewhere that stop compiling (`action::run`, the GUI's
`action_rows`) get a stub for now: `run` → `Ok(())`, `action_rows` → no rows.

Tests: `multi_roundtrip` (below), `multi_on_dial_press_roundtrip`,
`empty_multi_writes_type_only`, `delay_roundtrip` (`ms` kept; `type = "delay"` alone → 0),
`kinds_for_combos` (`kind_index` matches `KINDS` for every `empty_of_kind`; `KEY_KINDS` has
no 13, `STEP_KINDS` no 12; `kind_position(STEP_KINDS, 13) == Some(12)`,
`kind_position(STEP_KINDS, 12) == None`, and `STEP_KINDS[12] == 13`),
`multi_is_deck_owned_by_its_steps`, `delay_and_multi_summaries` (`Wait 500 ms`,
`Wait 1.5 s`, `Wait 2 s`, `Multi action (no steps)`, `Multi action (only waits)`,
`1 action: Launch a`, `3 actions: Launch a, …` with a leading delay not counted),
`rename_page_reaches_multi_steps` (`links_to` counts a page step; `rename_page` renames it;
`remove_page` turns it into `Action::None`). The existing `roundtrip`,
`deck_owned_kinds` (still `[6, 7, 9, 10]`: an empty multi isn't deck-owned) and
`old_config_without_dials_loads` still pass.

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

Check: `cargo test -p omadeck-core config::` → `test result: ok`, 0 failed, the 8 new tests listed.

### Task 2: the blocking runner (~150 lines)

Files: `crates/omadeck-core/src/action.rs`.

`run_multi_blocking`; `run`'s deck-owned multi bail and its `Delay` and `Multi` arms
(replacing task 1's stub). Add `use std::time::Duration;` at the top of the module.

Test (in `action::tests`):

```rust
#[test]
fn multi_blocking_collects_errors() {
    // None of these spawn anything: each fails before it would, or does nothing.
    let steps = [
        Action::App { command: "".into() },
        Action::Delay { ms: 0 },
        Action::Url { url: "".into() },
        Action::Multi { steps: vec![] },
        Action::Back,
    ];
    let failed = run_multi_blocking(&steps);
    let got: Vec<(usize, String)> = failed.iter().map(|(n, e)| (*n, e.to_string())).collect();
    assert_eq!(
        got,
        vec![
            (0, "no application set".to_string()),
            (2, "no URL set".to_string()),
            (3, "multi-actions can't be nested".to_string()),
            (4, "Go back only works on the deck".to_string()),
        ]
    );
    let owned = Action::Multi { steps: vec![Action::Delay { ms: 10 }, Action::PageStep { change: 1 }] };
    assert!(run(&owned).unwrap_err().to_string().contains("only work on the deck"));
}
```

Check: `cargo test -p omadeck-core action::` → `test result: ok`, 0 failed.
`run_refuses_exactly_the_deck_owned_kinds` (skips 8 and 11 since PR 10) needs no change and
still passes: for kind 12 an empty multi isn't deck-owned and spawns a `multi` thread that
does nothing, and kind 13 (Delay) is `Ok(())`.

### Task 3: the sequencer (~220 lines)

Files: `crates/omadeckd/src/sequence.rs` (new), `crates/omadeckd/src/main.rs`
(`mod sequence;`).

`RunOf` and its `Display`, `Sequencer`, `Run`, as above.

Tests in `sequence::tests`: `start_runs_leading_steps_now`, `delay_holds_the_rest`
(below), `delays_do_not_drift` (`[a, Delay 500, b, Delay 500, c]`: `due(t0 + 800 ms)`
returns `b`, and `next_due()` is `t0 + 1000 ms`, not `t0 + 1300 ms`),
`restart_ignored_while_running` (and it starts again once finished),
`trailing_delay_keeps_it_running`, `two_inputs_run_independently`, `next_due_is_earliest`,
`empty_multi_finishes_at_once`, `run_of_display` (`"key 5"`, `"key 5 on Media"`,
`"dial 2"` for `Turn(2)` and `Press(2)`, `"omadeckd"`).

```rust
fn app(c: &str) -> Action {
    Action::App { command: c.into() }
}

#[test]
fn delay_holds_the_rest() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut s = Sequencer::default();
    assert!(s.start(RunOf::Turn(1), vec![app("a"), Action::Delay { ms: 500 }, app("b"), app("c")], t0));
    assert_eq!(s.due(t0), vec![(RunOf::Turn(1), 0, app("a"))]);
    assert_eq!(s.next_due(), Some(t0 + ms(500)));
    assert!(s.due(t0 + ms(499)).is_empty());
    assert_eq!(s.due(t0 + ms(500)), vec![(RunOf::Turn(1), 2, app("b")), (RunOf::Turn(1), 3, app("c"))]);
    assert!(!s.is_running(&RunOf::Turn(1)));
    assert_eq!(s.next_due(), None);
}
```

Check: `cargo test -p omadeckd sequence::` → `test result: ok. 9 passed`.

### Task 4: the daemon runs multis (~200 lines)

Files: `crates/omadeckd/src/daemon.rs`, `crates/omadeckd/src/main.rs`.

`Source::Step` (and its `Display` arm, `step`), `Daemon.seq`, `run_of`, `start_multi`, the
`Delay`/`Multi` arms at the top of `try_input`, `report_failure`'s `Source::Step => {}`
arm, `press_key`'s running-multi check, `run_due_steps` called from `run()` right after
`daemon.handle_commands()` and before `daemon.apply_values()`
(`daemon.run_due_steps(Instant::now());`), `next_deadline`, the shutdown log, and the
offline branch in `press_locally`/`dial_locally`.

Test (in `daemon.rs`'s `tests` module; `Source` already has `source_names_its_key_or_dial`):

```rust
#[test]
fn step_and_ctl_sources_name_themselves() {
    assert_eq!(Source::Step.to_string(), "step");
    assert_eq!(Source::Ctl.to_string(), "omadeckd");
    assert_eq!(Source::Tap(2).to_string(), "dial 2");
}
```

Check: `cargo test -p omadeckd` → `test result: ok`, 0 failed. By hand with the example
config: `omadeckd press 5` runs all steps with the 2 s gap (the log shows
`key 5 step 4: hyprctl dispatch movetoworkspace 5` about 2 s after step 2).

### Task 5: kind lists, Delay editor and step slots (~180 lines)

Files: `crates/omadeck-gui/src/editor.rs`, `crates/omadeck-gui/src/app.rs`.

`ActionSlot.kinds` (`KEY_KINDS` in the three existing slots), the combo mapping and
subtitles, the `Delay` arm, `ActionTarget::Step` with `PartialEq` (and on `KeyRef`), its
`dial_slot`, `action_at` and `set_action_at` arms, `ActionSlot::step`, `App.selected_step`
and its reset in `select`.

Test (a new `#[cfg(test)] mod tests` in `app.rs`, `use super::*;`; `KeyRef`'s fields are
private, so the test lives in `app.rs`):

```rust
#[test]
fn step_targets_compare_by_parent_and_index() {
    let key = ActionTarget::Key(KeyRef { page: None, index: 5 });
    let step = ActionTarget::Step(Box::new(key.clone()), 1);
    assert!(step == ActionTarget::Step(Box::new(key.clone()), 1));
    assert!(step != ActionTarget::Step(Box::new(key), 2));
    assert!(step != ActionTarget::Step(Box::new(ActionTarget::DialPress(0)), 1));
    assert_eq!(step.dial_slot(), None);
}
```

Check (on the machine, it needs GTK): `cargo test -p omadeck-gui step_targets` → `1 passed`.
By hand: a key's kind combo has no "Delay"; a hand-edited `type = "delay"` key shows an
empty combo instead of the wrong kind.

### Task 6: the step list, Try it now, and docs (~280 lines)

Files: `crates/omadeck-gui/src/editor.rs`, `README.md`, `examples/config.toml`,
`CHANGELOG.md`.

- The `Multi` arm (rows, buttons, "Add step" popover, the step group), `edit_steps`,
  `StepEdit`, `selection_after`, the step group's ▶, and the `Multi`/`Delay` cases in
  `test_button`.
- README Features: multi actions, with the "What order the steps keep" list in short.
  Roadmap: tick "Multi-actions" `[x]`.
- `examples/config.toml`: the "Meeting" key above, commented out.
- CHANGELOG, Unreleased / Added: "A `multi` action runs several actions in order from one
  key or dial, with `delay` steps in between, as in the Elgato app's Multi Action. The
  configurator edits the steps as a list."

Test (in `editor.rs`'s `tests` module from PR 11):

```rust
#[test]
fn selection_follows_the_step() {
    // Moving the selected step up keeps it selected; moving its neighbour shifts it.
    assert_eq!(selection_after(Some(2), StepEdit::Up(2), 4), Some(1));
    assert_eq!(selection_after(Some(1), StepEdit::Up(2), 4), Some(2));
    assert_eq!(selection_after(Some(1), StepEdit::Down(1), 4), Some(2));
    // Removing it clears the selection; removing one above moves it up.
    assert_eq!(selection_after(Some(2), StepEdit::Remove(2), 4), None);
    assert_eq!(selection_after(Some(2), StepEdit::Remove(0), 4), Some(1));
    assert_eq!(selection_after(Some(0), StepEdit::Remove(3), 4), Some(0));
    // A new step is selected.
    assert_eq!(selection_after(None, StepEdit::Add, 4), Some(4));
}
```

Check (on the machine): `cargo test -p omadeck-gui selection_follows` → `1 passed`; then
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → every crate
`test result: ok`. By hand: build the "Meeting" key from the example; the saved
`config.toml` matches the example's shape; reorder and remove steps; pick an app for a
step, then switch key before the picker closes: the toast says the change was dropped.

## Known traps

- **No `impl Fn` builders.** `action_editor` → `action_rows` → `action_editor` for a step only
  compiles because `ActionSlot` holds `Rc<dyn Fn…>`. Don't add a generic parameter to either.
- **BUGS.md 4:** never `if let Some(k) = a.config.borrow().key(..) { edit(..) }`. Every step
  edit goes through `(slot.get)(a)` or `action_at`, which return owned clones, and only then
  `(slot.set)` / `set_action_at`. The `Step` arm of `set_action_at` must not hold a
  `self.config.borrow()` while it calls `set_action_at` on the parent.
- **No `rebuild_editor` from a spin or entry handler** (focus is lost). The Delay spin and
  text rows edit in place; only structural changes (kind, add, move, remove, select) rebuild.
- **Popover cycles:** a button inside the popover that captures the popover keeps both alive.
  Capture `pop.downgrade()` and `upgrade()` in the handler.
- **Nothing slow on the loop:** a delay is a deadline in the sequencer, never a sleep.
  `run_multi_blocking` sleeps, so it runs only off the loop (CLI main thread, `spawn_blocking`).
- **Borrows in the daemon:** `self.seq.due(now)` returns an owned `Vec`, so the loop over it
  can call `self.try_input`. `start_multi` gets `steps` as a slice of the caller's cloned
  action, and `try_input`'s callers already pass clones (`press_key` clones the key).
- **Recursion:** `try_input` → `start_multi` → `run_due_steps` → `try_input(Source::Step)`.
  A nested `Multi` step stops at `run_of(Step) == None`, so it can't loop.
- **`kind_index` and friends:** `run_refuses_exactly_the_deck_owned_kinds` and
  `deck_owned_kinds` loop over all of `KINDS`, so they cover 12 and 13 at once; read their
  failures before "fixing" them. Don't add 12 or 13 to the refuse test's skip (8 and 11):
  neither starts a process.
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
- [ ] Two Hyprland steps back to back (`workspace 3`, `workspace 4`): they land in that
      order, and the deck stays responsive while Hyprland answers.
- [ ] A multi on a dial press and on a strip tap runs. A failing step toasts "Dial 2: step 1:
      script not found …" and the next step still runs.
- [ ] `omadeckd press 5` (through the daemon) runs the whole multi, page step included.
- [ ] Unplug during a delay: the run finishes (log), no panic; replug shows the page it moved to.
- [ ] Edit the config mid-delay: the old steps finish.
- [ ] Rename the page a multi step opens: the step follows the new name.
- [ ] GUI: add, reorder, remove steps; each step's editor works (app picker, file browse,
      Hyprland, volume, delay spin). The key's ▶ flashes the key and runs the steps; a
      second ▶ during a delay is ignored. A step's ▶ runs only that step.
- [ ] A config with no multi loads and saves byte-identical.

## Decisions to explain

- **Delay is an `Action` variant**, as Elgato lists it among actions. The TOML stays uniform
  and every step reuses `action_editor`. A top-level Delay in a hand-edited file does
  nothing, and the GUI never offers it.
- **Nesting is refused at run time**, so one bad step doesn't throw away the config.
- **Second press while running: ignored.** Ignoring never doubles side effects or cuts a
  run in half.
- **A failing step doesn't stop the rest**, believed to match Elgato (unconfirmed); each failure is reported.
- **No implicit gap between steps.** Elgato's 100 ms default is for virtual hotkey presses,
  which omadeck doesn't have. Steps start in order; add a Delay where an app needs time.
- **Steps don't wait for their process, nor for Hyprland's answer** (see "What order the
  steps keep"). Hyprland steps are queued in order on one worker, so they keep their order
  without holding up the deck.
- **The sequencer is on the deck loop, not a thread.** Every step ends up on the loop anyway
  (deck-owned steps need it), so a timer queue polled by the loop is less code and a pure,
  testable type.
- **A multi on a dial turn runs once per turn event.** Elgato may not allow multis on dials
  at all (unconfirmed); omadeck's dial actions are already "any action".
- **Steps move with ↑/↓ buttons**, not drag and drop as in Elgato's editor. Simpler for now.

## Ask before building

None; all settled 2026-10-17.

## Later

- **Multi Action Switch**: PR 8's toggle key has an action per state, and each can be a
  `multi`, which gives Elgato's switch with real state on top.
- **Random Action**; per-step "Next action" gaps; choosing a state for a two-state step.
- Hold to cancel a running multi; PR 8's busy state until the last step ends.
- A per-multi "stop on failure" switch, for a multi that should halt like `set -e`.
- Drag-and-drop reorder and duplicating a step.
- Batching consecutive Hyprland steps into one `hyprctl --batch`.
- Waiting for a volume or Hyprland step to land before the next step (a done signal from
  the watcher and the worker), and toasting a Hyprland step that fails on the worker.
- `MultiStarted` / `MultiFinished` hub events.

**Size:** about 1,250 lines with tests: core ~370, sequencer ~220, daemon ~200, GUI ~460.
