# PR 12: `feat/multi-actions`: one key runs several actions, with delays

Part of [PLAN.md](PLAN.md). Base: `feat/ctl` (PR 11), so `omadeckd press` through the daemon and the factored
key-press path already exist. Doesn't need the state watcher, the Hyprland listener or
strip layers. It does lean on PR 11's control socket for one thing: a `omadeckd press` through the daemon of a
multi key has to run the multi on the deck loop.

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Rewrite against the code as merged (upstream's named pages: `Option<&str>` / `Config::key(page, index)` / `Nav`), not `ba0eced`'s `page: u8`.
- [ ] Replace every "assumption about PR N" with that PR's real, merged signatures, copied in full.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.
- [ ] `action_editor` is generic over `impl Fn` closures. Calling it from its own `Multi` arm nests closure types without end and rustc hits the recursion limit. Change it to take `Rc<dyn Fn(&Config) -> Action>` / `Rc<dyn Fn(&Rc<App>, Action)>` first, as its own step.
- [ ] The step `edit` closure must not hold `a.config.borrow()` while calling the outer `edit` (BUGS.md 4). Clone the action, drop the borrow, then edit.
- [ ] Stale after the rebase: `KINDS` 12/13; the example's `type = "page" index = 1` becomes `page = "name"`; `Source::Key { page: u8 }` holds the page name. Deck-owned steps are Open page, Back, Page step and Brightness (no Folder kind; PR 8 toggles aren't actions).
- [ ] `dispatch()` is rebase step 2's dispatch, extended for `Multi`, not a new function. Drop `audio` from its parameters (PR 8 removes it).
- [ ] Combo mapping: write both directions between `KINDS` indices and positions in `KEY_KINDS`/`STEP_KINDS`.
- [ ] Sketch the steps editor's widget tree, and say how selecting a step triggers `rebuild_editor`.

**Outcome:** a new `multi` action holds an ordered list of steps. Each step is an ordinary
action (app, script, sound, website, Hyprland, volume, page, brightness, and whatever
PRs 8–11 added) or a `delay` step that waits some milliseconds. It works on keys, dial
presses and strip taps (and dial turns, once per event). Steps that act on the deck
(page, brightness) run on the deck loop like a normal press. External steps are spawned
detached as they are today. Delays are timed by a small **sequencer** on the deck loop, so
the loop never blocks. The configurator edits the steps as a list (add, remove, move
up/down), and each step reuses the existing single-action editor. Old configs load and
save byte-identical.

The roadmap item is "Multi-actions: run several actions from one key, with optional delays".

## How the Elgato app does it (researched 2026-10-16)

- **Multi Action** runs a sequence of actions in order from one key tap. You drag it onto
  a key and the editor opens. Actions dragged into the container run top to bottom.
  ([Elgato: how to use Multi Actions](https://www.elgato.com/us/en/explorer/products/stream-deck/how-to-use-multi-actions/),
  [help article 360027960912](https://help.elgato.com/hc/en-us/articles/360027960912-Elgato-Stream-Deck-Multi-Actions),
  which returned 403 when fetched, so its content comes from search snippets.)
- **No nesting:** "You cannot nest a Multi Action within another Multi Action" (same page).
- **Delay** is its own action, found in the Multi Action category of the actions list and
  dropped between steps. It is measured in **milliseconds** (1000 ms = 1 s) and is meant
  for longer pauses.
- **Built-in gap between steps:** Stream Deck 5.2 had a fixed 200 ms between steps. 5.3
  made it adjustable per step with a timer icon that has two sliders: "Virtual Key Press"
  (the gap between a virtual key-down and key-up, default 100 ms) and "Next Action" (the
  pause before the next step, default 100 ms). Elgato's pages disagree on the range (2–100
  or 2–200 ms), so the range is **unconfirmed**.
  ([help article 7092527123725](https://help.elgato.com/hc/en-us/articles/7092527123725),
  also 403, read from search snippets and a translated copy.)
- **Multi Action Switch** (added in Stream Deck 4.4) alternates between two Multi Actions
  on each press, for example "start stream" and then "stop stream". It works like Hotkey
  Switch, with two states.
  ([4.4 release notes](https://help.elgato.com/hc/en-us/articles/5162934218637-Elgato-Stream-Deck-4-4-Release-Notes), from snippets.)
- **Random Action** runs one action picked at random from a group on each tap (the
  how-to page above).
- **Plugin states in a multi:** a two-state action placed in a multi lets the user pick
  which state that step sets. The plugin sees `isInMultiAction` and `userDesiredState`.
  ([SDK keys guide](https://docs.elgato.com/streamdeck/sdk/guides/keys))
- **Unconfirmed**, not found in any Elgato page:
  - what a second press does while a multi is still running (restart, ignore, queue, or a
    second run in parallel);
  - whether a step that fails stops the rest (Elgato shows its alert triangle on the key);
  - whether a Multi Action can go on a Stream Deck + dial (Elgato's dial features are
    Dial Stacks and Action Trigger);
  - the maximum number of steps.

## Config

Two new `Action` variants. Delay is an action, as it is in Elgato's actions list. That
keeps a step a plain `Action` and the TOML uniform, with no wrapper enum:

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

`Action` is already `#[serde(tag = "type", rename_all = "snake_case")]`, so the steps are
arrays of tables nested in the key's action table:

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
index = 1
```

A dial uses `[dial.press]` with `type = "multi"` and `[[dial.press.steps]]`. There are no
new fields on existing structs, so old files round-trip byte-identical. A unit test checks
that the `toml` serializer writes `[[key.action.steps]]` and not inline arrays.
`KeyConfig::is_blank` needs no change: an empty multi is not `None`, so it is kept, which
matches the user having picked "Multi action".

**Nesting:** serde will parse a `multi` inside a `multi`, but it is not run (see Daemon).
The GUI never offers it. A loader error would throw away the whole config for one bad
step, so the check happens at run time instead.

## Action changes (`omadeck-core`)

- `KINDS` gains `"Multi action"` and `"Delay"` at the end. (After the rebase: App volume is 11, so these are **12** and **13**; the numbers that
  follow are from before the rebase.) After PR 10's "App volume" (9) these are
  indices **10 and 11** (PRs 7, 8, 9 and 11 add no kinds); `kind_index`,
  `empty_of_kind` and the tests use the real numbers. `empty_of_kind`: `Multi { steps: vec![] }`
  and `Delay { ms: 500 }`.
- Two kind lists for the GUI combos, as indices into `KINDS`:
  `Action::KEY_KINDS` (everything except Delay) and `Action::STEP_KINDS` (everything
  except Multi). The combo position maps through these lists instead of being the kind
  index itself.
- `Action::is_deck_owned(&self) -> bool`: `Page | Brightness`, plus any daemon-owned
  kinds PR 8 / PR 13 add (toggle, folder). This replaces the inline `matches!` in
  `editor.rs` and is used by the multi runner below.
- `summary()`:
  - `Delay`: `Wait 500 ms`, or `Wait 2 s` / `Wait 1.5 s` from 1000 ms up (`delay_summary`).
  - `Multi`: `Multi action (no steps)` when empty. Otherwise
    `{n} actions: {first non-delay summary}, …`, where `n` counts non-delay steps
    (`1 action: …` when there is one).
- `action::run`:
  - `Delay`: `Ok(())`, a no-op.
  - `Multi`: spawns a `multi` thread that calls `run_multi_blocking` and logs the
    failures. The daemon never calls `run` on a multi. This arm exists for other callers.
- New `pub fn run_multi_blocking(steps: &[Action]) -> Vec<(usize, anyhow::Error)>` runs
  the steps in order on the calling thread:
  - a delay sleeps;
  - a deck-owned step, or a nested multi, is skipped with a `log::warn`;
  - any other step goes through `run`, and its error is collected and the run goes on.
  It serves the offline CLI and the GUI's *Try it now*, where no deck loop exists.
- No `run_steps` change: the daemon handles `Multi` before it calls `run_steps`.

### The sequencer (new `omadeck-core/src/sequence.rs`, pure and unit-tested)

```rust
pub struct Sequencer<K> { runs: Vec<Run<K>> }
struct Run<K> { key: K, steps: Vec<Action>, next: usize, due: Instant }

impl<K: Clone + PartialEq> Sequencer<K> {
    /// Start `steps` for `key`. False (ignored) while that key's previous run is going.
    pub fn start(&mut self, key: K, steps: Vec<Action>, now: Instant) -> bool;
    /// Every step whose time has come, in order, as (key, step number, action). A delay
    /// pushes its run's `due` forward and is not returned. Finished runs are dropped.
    pub fn due(&mut self, now: Instant) -> Vec<(K, usize, Action)>;
    /// When the earliest waiting run is due, so the loop can shorten its read timeout.
    pub fn next_due(&self) -> Option<Instant>;
    pub fn is_running(&self, key: &K) -> bool;
}
```

Steps with no delay between them come out of one `due()` call, so they run back to back
in the same loop iteration. A run's `due` advances from the delay's own deadline, not from
when the loop got around to it, so lateness doesn't add up across delays.

## Daemon (`omadeckd/src/main.rs`)

- **One dispatch path.** Today's three `match act { Page / Brightness / _ => run }`
  blocks (key down, dial turn, dial press and strip tap) become one function:
  `fn dispatch(d, cfg, renderer, hub, audio, src: Source, act: &Action, steps: u8)`,
  with `enum Source { Key { page: u8, index: u8 }, Dial(u8) }`. It takes over from the
  press helper PR 11 factored out (assumed: PR 11 has one press function shared by
  `ButtonDown` and `omadeckd press` through the daemon).
  - `Page` / `Brightness` go to `do_page` / `do_brightness`, as now.
  - `Multi { steps }` calls `d.multi.start(src, steps.clone(), now)`. If that returns
    false, it logs `key 5: multi action still running; ignored`. Otherwise the loop runs
    the first due steps at once.
  - `Delay` does nothing.
  - Anything else goes to `action::run` / `run_steps` and then `after_volume`, as now.
  - A failure publishes `ActionFailed` (for a key) or `DialActionFailed` (for a dial).
- **Running due steps.** Each loop iteration, after the audio levels, runs
  `for (src, n, step) in d.multi.due(now)` through the same `dispatch`, and logs
  `key 5 step 2: Launch zoom.desktop`. A nested `Multi` step is not started. It is
  reported as a failed step: "step 3: multi-actions can't be nested". A failed step
  publishes the same event with the message prefixed `step N: `. The run then continues
  with the next step.
- **Timing.** `Deck.multi: Sequencer<Source>`. The HID read uses
  `min(READ_TIMEOUT, next_due - now)` with a floor of 1 ms. That makes a delay accurate to
  a few ms instead of up to the 100 ms read timeout, and the loop still only wakes as
  often as now when no multi is running.
- **Dial turns.** A multi on `turn_left` / `turn_right` starts once per turn event,
  whatever the detent count. It is ignored while the previous run is going, so a fast spin
  can't stack runs.
- **Page change:** runs continue. A multi that switches page and then does more must
  finish. Steps were cloned at start, so it doesn't matter which page is showing.
- **Config reload:** running sequences keep their cloned steps and finish. New presses use
  the new config.
- **Disconnect:** the sequencer lives in `Deck`, so pending steps are dropped with it and
  the log says how many runs were dropped. Deck-owned steps couldn't apply without a deck
  anyway.
- **Shutdown:** dropped. Detached children are not affected, as today.
- **CLI:** `omadeckd press` / `dial` (offline) use `run_multi_blocking` for a multi, print
  each skipped or failed step, and exit 1 if any step failed. `omadeckd press` through the daemon (PR 11) goes
  through the daemon, so page and brightness steps work there too.
- **Hyprland steps** each run `hyprctl dispatch` synchronously on the loop, as today
  (a few ms each). Batching consecutive dispatchers into `hyprctl --batch` is left for
  Later.

No IPC changes. `ActionFailed` / `DialActionFailed` carry the step number in `message`, so
older configurators show it unchanged.

## GUI (`omadeck-gui/src/editor.rs`)

- `action_editor` gains two parameters:
  - `kinds: &'static [usize]`: `KEY_KINDS` for keys and dials, `STEP_KINDS` for steps;
  - `page: &adw::PreferencesPage`, so a multi can add its step group below.
  `dial_action_group` passes its page through.
- **`Action::Multi` arm**, inside the existing group:
  - **One row per step** (`adw::ActionRow`, title `2 · Launch Zoom`, from the step's
    `summary()`). Its suffixes are ↑ / ↓ / ✕ flat buttons for move up, move down and
    remove. Clicking the row selects that step for editing.
  - **An "Add step" row:** a `gtk::MenuButton` listing `STEP_KINDS` (Delay included). A new
    step is appended as `empty_of_kind` and selected.
  - **The step editor:** the selected step gets its own `PreferencesGroup` titled
    "Step N", added to `page` right after this group. It is built by calling
    `action_editor` again with `STEP_KINDS`. Its `get` closure reads `steps[n]` of this
    action; its `edit` closure replaces `steps[n]` through the outer `edit`. App picks set
    the key's label only if it is empty. The website favicon handler is passed down
    unchanged.
  - The selected step is `App.selected_step: Cell<Option<usize>>`. It is reset by
    `App::select`, and kept across `rebuild_editor` while the index is still valid.
- **`Action::Delay` arm:** an `adw::SpinRow` "Wait", 0–600 000 ms in steps of 100. The
  subtitle shows the value in seconds.
- **Try it now:**
  - for a multi, it runs `run_multi_blocking` in `gio::spawn_blocking` and toasts the
    first failure ("Step 2: script not found …");
  - if the multi has deck-owned steps, the tooltip says "Page and brightness steps only
    work on the deck";
  - it is insensitive when every step is deck-owned or a delay.
- **Preview:** no change. A multi key shows its own label, icon and glyph like any key.
- **Starter layout:** unchanged. A multi is personal, and the starter has no deck-owned
  actions either.

## Docs

- README: multi-actions in Features, and the roadmap's "Multi-actions" ticked `[x]`.
- `examples/config.toml`: the commented "Meeting" example above.
- CHANGELOG, under Unreleased / Added: "A `multi` action runs several actions in order from
  one key or dial press, with `delay` steps in between, as in the Elgato app's Multi
  Action. The configurator edits the steps as a list."

## Implementation steps

1. [ ] Core: `Multi` and `Delay` variants, `KINDS`, `kind_index`, `empty_of_kind`,
   `KEY_KINDS` / `STEP_KINDS`, `is_deck_owned`, the summaries, and the TOML round-trip
   tests. Commit.
2. [ ] Core: `action::run` arms and `run_multi_blocking`, with tests (deck-owned and nested
   steps skipped, errors collected). Commit.
3. [ ] Core: `sequence.rs` `Sequencer` and its unit tests. Commit.
4. [ ] Daemon: refactor the three action blocks into `dispatch(src, …)`. No behaviour
   change; check with a key, a dial turn, a dial press and a strip tap. Commit.
5. [ ] Daemon: `Deck.multi`, the `Multi` arm, running due steps, the read-timeout
   shortening, and the disconnect log. Offline CLI `press` / `dial` use
   `run_multi_blocking`. Commit.
6. [ ] GUI: `kinds` / `page` parameters on `action_editor`, and the `Delay` arm. Commit.
7. [ ] GUI: the step list, the step editor group, `selected_step`, and Try-it-now. Commit.
8. [ ] Docs: README, example, CHANGELOG. Then `cargo fmt && cargo clippy --all-targets
   -- -D warnings && cargo test`.

## Tests

- config:
  - `multi_roundtrip`: a key with four steps serializes to `[[key.action.steps]]` tables
    and parses back equal;
  - `multi_on_dial_press_roundtrip`;
  - `empty_multi_writes_type_only`;
  - `delay_roundtrip` (`ms` kept, and `ms` missing → 0);
  - `old_config_unchanged` (existing fixture, byte-identical save).
- action kinds:
  - `empty_of_kind` for both new kinds;
  - `kind_index` matches `KINDS`;
  - `KEY_KINDS` excludes Delay, and `STEP_KINDS` excludes Multi;
  - `is_deck_owned` for Page and Brightness, false for App and Multi.
- summaries: `Wait 500 ms`, `Wait 1.5 s`, `Wait 2 s`, `Multi action (no steps)`,
  `1 action: …`, and `3 actions: …` (which skips leading delays and doesn't count them).
- `run_multi_blocking`:
  - an empty `App` step returns `(0, "no application set")`, and the next step still runs
    (a `Delay { ms: 0 }` and a second bad step give two errors);
  - a `Page` step and a nested `Multi` are skipped and not counted as errors.
- sequence:
  - `start_runs_leading_steps_now`: steps before the first delay come out of `due(now)`;
  - `delay_holds_rest` (nothing at `now + 499 ms`, the rest at `now + 500 ms`);
  - `delays_do_not_drift` (a late `due` call still schedules the next delay from the
    deadline);
  - `restart_ignored_while_running`, and the key can start again once finished;
  - `two_keys_run_independently`;
  - `next_due_is_earliest`;
  - `empty_multi_finishes_at_once`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] The "Meeting" example: the mic mutes (OSD shows), Zoom launches, 2 s later the window
      moves to workspace 5, and the deck switches to page 2. Order and timing look right.
- [ ] Pressing the key again during the 2 s delay does nothing. The log says it was
      ignored, and pressing after it finishes runs it again.
- [ ] Other keys and dials stay responsive during a long delay (for example a 10 s delay
      while turning the volume dial).
- [ ] A multi with `brightness set = 20` then `delay 1000` then `brightness set = 80`:
      the deck dims and comes back. The brightness dial's panel follows both changes.
- [ ] A multi on a dial press and on a strip tap runs. A failing step (a missing script)
      toasts "Dial 2: step 1: script not found …" and the next step still runs.
- [ ] `omadeckd press 5` (through the daemon, PR 11) runs the whole multi, including the page step.
- [ ] Unplugging during a delay drops the run (log line) without a panic. Replugging works.
- [ ] Editing the config while a multi is mid-delay: it finishes the old steps.
- [ ] GUI: add, reorder and remove steps; each step's editor works (app picker, file
      browse, Hyprland, volume, delay spin); the saved TOML matches the example's shape;
      Try-it-now runs the external steps.
- [ ] A config with no multi loads and saves byte-identical.

## Decisions to explain

- **Delay is an `Action` variant, not a separate step type.** Elgato lists Delay as an
  action in the Multi Action category. It keeps the TOML uniform
  (`[[key.action.steps]] type = "delay"`) and lets every step reuse `action_editor`.
  The cost: a top-level Delay is possible in a hand-edited file. It does nothing, and the
  GUI never offers it (`KEY_KINDS`).
- **Nesting is refused at run time, not at load.** Elgato forbids nesting. Rejecting it in
  `Config::load` would make the daemon fall back to the last good config because of one
  step. Reporting it as a failed step is narrower and still visible, and the GUI can't
  create one.
- **Second press while running: ignored.** Elgato's behaviour is unconfirmed. Ignoring is
  the only choice that never doubles side effects (two Zooms, a page flipping twice) and
  never cuts a sequence off halfway. If the real app restarts or queues, switch to that
  and record it here.
- **A failing step doesn't stop the rest.** Elgato's behaviour is unconfirmed. Most
  failures are things like a missing script, and the user usually still wants the other
  steps. Each failure is reported on its own, with its step number.
- **No implicit gap between steps.** Elgato's default 100 ms "Next Action" gap exists for
  virtual hotkey presses, and omadeck has no hotkey action. Spawns start in order, and
  `hyprctl dispatch` waits for "ok", so order holds. Users add a Delay where an app needs
  time to start. Per-step gaps go to Later.
- **The sequencer is on the deck loop, not a worker thread.** Every step has to end up on
  the loop anyway: deck-owned steps need `Deck`, and external steps are already
  non-blocking spawns. A thread would only sleep and send each step back over a channel.
  A timer queue polled by the loop, with the read timeout shortened to the next deadline,
  is less code, keeps one thread owning the deck, and is a pure, testable core type.
- **A multi on a dial turn runs once per turn event,** not once per detent. A sequence per
  detent on a fast spin makes no sense, and "ignore while running" covers it.
  Elgato possibly doesn't allow multis on dials at all (unconfirmed); omadeck's dial
  actions are already "any action", so it stays consistent.
- **The offline CLI and Try-it-now skip page and brightness steps.** They have no deck.
  `omadeckd press` through the daemon goes through the daemon and runs everything.
- **Steps are reordered with ↑/↓ buttons, not drag and drop** as in Elgato's editor.
  Simpler for now.

## Later

- **Multi Action Switch** (two step lists that alternate). It overlaps with PR 8 toggle
  keys: once PR 8's toggle key has an action per state, each of those can simply be a
  `multi`, which gives Elgato's switch with real state on top. Revisit after both are in.
- **Random Action** (run one random step).
- Per-step "Next action" gaps like Elgato 5.3's timer icon.
- Choosing a state for a two-state step (Elgato's `userDesiredState`), for example
  "mute on" rather than a mute toggle.
- Press and hold to cancel a running multi.
- PR 8's `busy_while_running` for multis: the key stays busy until the sequencer's last
  step ends (and a tracked step inside it has exited).
- Drag-and-drop step reorder, and duplicating a step.
- Batching consecutive Hyprland steps into one `hyprctl --batch` call, with per-command
  Lua translation.
- `MultiStarted` / `MultiFinished` hub events.

**Size:** about 600 lines:

| Part | Lines |
|------|-------|
| Core: variants, summaries, runner, tests | ~250 |
| Daemon: dispatch refactor and sequencer wiring | ~120 |
| GUI: step list and delay editor | ~200 |
| Docs | ~30 |

If the GUI grows past ~250 lines, it can be split into a follow-up, as PR 5 allowed.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- A second press while a multi is running is ignored (planned). Elgato's behaviour is unconfirmed: restart or queue instead?
- A failing step doesn't stop the rest (planned). Stop instead? Unconfirmed in Elgato.
- Multi on a dial turn runs once per turn event, not per detent (planned). OK? Whether Elgato allows multis on dials is unconfirmed.
- Multi Action Switch: left for later, or folded into toggle keys (PR 8) by giving each state its own multi?
