# Plan: making omadeck better

Personal working notes; not meant for upstream. These files live on the `plans` branch
only (`git checkout plans`), never on `master` or `daily`.

**Do NOT open any PRs right now. This is private work.** Feature branches are pushed only
to my own fork (`nishantpillai5/omadeck`) and run from `daily`. Opening PRs, or contacting
the upstream author, is a later decision I make explicitly; don't open or prepare them
unless I say so.

## Done so far (PRs to open later)

Stream Deck + support and pages, all on `daily`, each on its own `feat/*` branch:

| PR | Branch | What | Plan |
|----|--------|------|------|
| 1 | `feat/plus-input` | Read and publish the Plus's dial and touch-strip input | [pr1](pr1-plus-input.md) |
| 2 | `feat/dial-actions` | `[[dial]]` config; turns and presses run ordinary actions | [pr2](pr2-dial-actions.md) |
| 3 | `feat/touch-strip` | Draw the strip with Elgato's `$X1`/`$B1` layouts | [pr3](pr3-touch-strip.md) |
| 4 | `feat/dial-volume` | `volume` action, live level on the strip, mic capped at 100% | [pr4](pr4-dial-volume.md) |
| 5 | `feat/plus-pages` | The Plus's page parts on upstream's pages: strip flick, `page_step`, folder skipping | [rebase](rebase-on-upstream-pages.md) (replaces `feat/pages`) |
| 6 | `feat/brightness` | `brightness` action, live level on the strip, persisted in the state file | [pr6](pr6-brightness.md) |
| 7 | `feat/strip-backgrounds` | Background images: one across the strip, one per dial | [pr7](pr7-strip-backgrounds.md) |

Known bugs are in [BUGS.md](BUGS.md); 50–72 are open (from the second review), plus
the parts of 25, 31, 38, 45 and 48 that review 2 found not fully fixed.

Shared facts and the decisions to explain for PRs 1–4 are in
[dials-and-strip.md](dials-and-strip.md). Hardware checks still open on PRs 1–4 and 6 are
listed in each file.

## What's next, in order

**Regrouped (2026-10-18): the PR candidates are now the ten `up/*` branches** — see
[UPSTREAM.md](UPSTREAM.md). The `pr/*` notes below are history.

**Folded (2026-10-17):** the fixes now also live inside the features they fix, on
`pr/01-plus-input` … `pr/16-strip-long-press` (see Branches below). From here, a fix found
on the machine goes on the `pr/*` branch it belongs to, and the branches above it are
rebased (or, until the PRs are opened, on `daily` first and folded later).
**Cleaned (2026-10-17):** the `pr/*` branches were then rebuilt once more, changing only
CHANGELOG.md and five doc comments: "Fixed" lines about our own unreleased features are
gone (two folded into their "Added" entries); upstream's lines and our fixes to released
or upstream behaviour (Hyprland actions, replugging a smaller deck) stay. The code is
otherwise identical to `fix/review-3`. The tips before that pass, in case anything is
needed from them: 01 `1a0506e`, 02 `73f6d3f`, 03 `5b8ddc3`, 04 `8a6211d`, 05 `b36a0ab`,
06 `6c07d1a`, 07 `89abf9a`, 07b `509bc26`, 08 `5ef423e`, 09 `5bee4d0`, 10 `1bffb22`,
11 `6a49497`, 12 `ac02546`, 13 `a48e4a9`, 14 `f51bfd6`, 15 `f1f7a5f`, 16 `412a906`.

**Built, not verified (2026-10-17): `fix/review-2` and PRs 9–16.** All nine branches were
built on the Mac in one go and pushed to the fork, each on the one before:
`fix/review-2` → `feat/live-keys` → `feat/app-volume` → `feat/ctl` →
`feat/multi-actions` → `feat/folders` → `feat/media-dial` → `feat/app-pages` →
`feat/strip-long-press`. Core and daemon tests, clippy and fmt pass on each (except the
hub socket tests, which the sandbox can't bind). The GUI type-checks on every branch (a
stub `pkg-config`, see [review-2026-10-17c.md](review-2026-10-17c.md)) but was never
linked or run, and nothing was run on the deck. **Review 3**
([review-2026-10-17c.md](review-2026-10-17c.md), BUGS.md 73–104): one high (73), four
medium (74–77) and CI red from clippy (78); all 32 are fixed on `fix/review-3` (on top of
`feat/strip-long-press`; `daily` was that branch, `746e326`, until the series was cleaned), most with regression
tests, including a socket-free test `Daemon`. `scripts/capture-fixtures.sh` collects every
machine capture into test fixtures. On the machine: run the capture script first, then
VERIFY.md from the top. Next: work through [VERIFY.md](VERIFY.md) on the Omarchy
machine, branch by branch from the bottom. Fix compile errors and bugs on the lowest
branch that has them, then rebase the ones above. After that, rebuild `daily`.

**Done (2026-10-17): [rebase onto upstream's pages](rebase-on-upstream-pages.md).**
PRs 1–6 are rebuilt on `upstream/pages-and-folders` (`fb9052f`); `daily` is now
`feat/brightness` (`b53a2e6`), and the Plus's page parts are the new PR 5
(`feat/plus-pages`, `4c7bf70`). Upstream is still at `fb9052f` with no open PRs; the
bugs U1–U9 in [BUGS.md](BUGS.md) are to go in the first message to the author.

**Done (2026-10-17): bug pass.** All 21 recorded bugs closed; the fixes live on
`fix/bugs` (`d7bd448`, based on `feat/strip-backgrounds`), which is now `daily`.

**Done (2026-10-17): code review of everything on `daily`.** Summary and ranking in
[review-2026-10-17.md](review-2026-10-17.md); the bugs are BUGS.md 22–49 (bug 18 reopened
as 29). It also showed most later plans had drifted from the code, so all of them (8–16)
were rewritten against `d7bd448` and the two new steps below.

**Done (2026-10-17): `fix/review`.** All of BUGS.md 22–43 fixed on `fix/review`
(`87be396`, eight task commits), which is now `daily`. CI runs on the fork
(`workflow_dispatch`); the goldens use a bundled OFL font subset. One plan typo fixed
along the way: the Arch font package is `ttf-dejavu`.

**Done (2026-10-17): PR 8, `feat/toggle-keys`.** Eleven task commits (A–K) on
`feat/groundwork`: probe/proc + `Job`, `[key.on]`/`[key.busy]`/`busy_while_running`,
on-face tint and the busy progress bar, the watcher's `Refresh`/`Leave`/fast polling,
the daemon's press toggles and optimistic guesses (`Settle`), tracked jobs in their own
systemd scope, the GUI mirror with Off/On/Busy face tabs, the Toggle group, and presets
(mic/output mute, night light, stay awake, DND, bluetooth, Service…). 170 tests. Deck-side
hardware checks passed (mute-follow, command state, optimistic press, busy jobs); the GUI
checks were partly verified and the rest skipped at the user's request — see the
verification notes at the end of [pr8-toggle-keys.md](pr8-toggle-keys.md).

**Done (2026-10-17): second code review** of `d7bd448..55d0d62` (fix/review, groundwork,
PR 8). Summary in [review-2026-10-17b.md](review-2026-10-17b.md); bugs are BUGS.md 50–72.
Config compatibility holds. The worst: the configurator aborts when a key is drawn "on"
without an on face (50), the Service… preset moves the key to index 0 (51), a slow toggle
can stay busy for good (53), the volume in-flight total can drift (54), brightness reverts
at start after any GUI save (45), and CI fails on clippy 1.95 at `daily`. Plans 9–16 were
rewritten against the merged code and `fix/review-2`.

Next, in this order:

1. **`fix/review-2`** ([review-fixes-2.md](review-fixes-2.md), base `feat/toggle-keys`):
   BUGS.md 50–72 except 65, plus small API fixes PRs 9–16 rely on: one action walker
   that includes `on.action`, start brightness by value (`State.config_brightness`), one
   ordered Hyprland worker, run ids on watcher results, the `test-support` feature,
   exhaustive `ActionTarget` matches.
2. **PR 9, `feat/live-keys`** (base `fix/review-2`), then the remaining features, each
   based on the previous branch.

`feat/groundwork` is **done** (ten task commits on `fix/review`, plus the in-flight
volume fix and the start-time brightness precedence found on the hardware pass — see
the verification notes at the end of [groundwork.md](groundwork.md)). BUGS.md 44–49
are closed.

Each feature gets its own `feat/*` branch, plan file and (later) PR. Work them in this
order; the order is mine, not upstream's roadmap order.

| # | Branch | Feature | Plan |
|---|--------|---------|------|
| – | `fix/review` | Bugs from the 2026-10-17 review — **done** (`87be396`) | [review-fixes](review-fixes.md) |
| – | `feat/groundwork` | Daemon split, watcher, ordered volume, non-blocking hub, GUI mirrors the daemon, `ActionSlot` — **done** (2026-10-17) | [groundwork](groundwork.md) |
| 8 | `feat/toggle-keys` | Keys with on/off faces that follow real state, plus a busy state with progress for slow toggles (e.g. a model server starting up) and for any key while its command runs — **done** (2026-10-17) | [pr8](pr8-toggle-keys.md) |
| – | `fix/review-2` | Bugs from the second review (50–72) and the API fixes PRs 9–16 rely on — **built, not verified** ([VERIFY.md](VERIFY.md)) | [review-fixes-2](review-fixes-2.md) |
| 9 | `feat/live-keys` | Keys that show a clock, CPU, workspace, … and update themselves; the Hyprland event listener — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr9](pr9-live-keys.md) |
| 10 | `feat/app-volume` | Per-app volume: a dial for a chosen app, or for the focused app — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr10](pr10-app-volume.md) |
| 11 | `feat/ctl` | Drive the running daemon from the shell (`omadeckd page next`, `brightness 30`, …) — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr11](pr11-ctl.md) |
| 12 | `feat/multi-actions` | One key runs several actions, with optional delays — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr12](pr12-multi-actions.md) |
| 13 | `feat/folders` | Folder gestures: hold Back for Home, page steps from nested folders, double-click a folder in the configurator (upstream already has folders) — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr13](pr13-folders.md) |
| 14 | `feat/media-dial` | Media dial: scrub, play/pause, track and progress on the strip — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr14](pr14-media-dial.md) |
| 15 | `feat/app-pages` | Pages follow the focused app (`apps` on `[[page]]`, Elgato's Smart Profiles) — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr15](pr15-app-pages.md) |
| 16 | `feat/strip-long-press` | Hold a strip panel for a second action per dial (Elgato's "long touch"). Needs PR 11's `tap_dial`/`try_input`, so it can move up to right after PR 11 — **built, not verified** ([VERIFY.md](VERIFY.md)) | [pr16](pr16-strip-long-press.md) |
| – | `fix/review-3` | Bugs from the third review (73–104), the capture script, a test `Daemon`; `daily` = this branch — **built, not verified** ([VERIFY.md](VERIFY.md)) | [review-2026-10-17c](review-2026-10-17c.md) |

### Shared building blocks

Several features need the same machinery. Build each piece once, in the first PR that
needs it, and have the later PRs reuse it rather than grow a second copy. Groundwork's and
PR 8's pieces are now built; take their names from the code, not from this list.

- **Daemon structure** (groundwork). `Daemon` owns config, renderer, nav, hub, watcher,
  saver and a `Runtime { values, state }` that survives a replug; `Deck` holds only
  hardware state. Every input path goes through `Daemon::run_input(Source, &Action, steps)`
  (PR 11 adds `try_input` for replies, and `Source::Ctl`; PR 12 `Source::Step`; PR 16
  `Source::LongTap`). The loop reads HID with a timeout up to `next_deadline()`, drains
  further reports at once, and flushes dirty panels once per pass. No HID reader thread:
  the device handle can't be split across threads.
- **State watcher** (groundwork, extended by 8, 9, 10, 14). Core `probe.rs` has
  `Probe`/`Value`/`Values` and the one `dial_status` builder (`state.rs` stays the saved
  `State`). Daemon `watch.rs` replaces the audio worker: the loop sends it what to watch,
  it reads off the deck loop and sends back only *changes*. It also runs volume changes,
  in order, each followed by a read. Every child process goes through
  `proc::output_timeout`. PR 8 adds command probes (on runner threads), PR 9 built-in
  sources, PR 10 app streams, PR 14 the player.
- **Only the daemon reads system state** (groundwork). The GUI mirrors it from
  `Event::Brightness`, `Event::DialStatus { index, status: PanelStatus }` and, from PR 8,
  `Event::KeyState`. The hub replays the latest to new clients and never blocks the loop
  (per-client writer threads).
- **State file** (groundwork). The daemon owns the whole `State` and sends it to one
  `StateSaver`, which is joined at shutdown. Later PRs that persist something add a field.
- **Action editors** (groundwork). `action_editor(ActionSlot)` with `Rc<dyn Fn>` closures
  and an `ActionTarget` saying which action it edits. PR 8 (on-action), PR 12 (steps) and
  PR 16 (long press) add targets; recursion works. From `fix/review-2`, `action_at` and
  `set_action_at` match exhaustively, so a new target won't compile until it's handled.
- **One action walker** (`fix/review-2`). Every action in the config, `on.action`
  included, is reached by one walker; page rename/delete and link counting use it.
  PR 12 adds multi steps to it and PR 16 the long press. Any new action field must too.
- **Hyprland worker** (`fix/review-2`). Every Hyprland dispatch goes to one ordered
  background worker, never the deck loop. PR 12's Hyprland steps are queued on it in order.
- **Test support** (`fix/review-2`). Core's `test-support` feature gives daemon and GUI
  tests the bundled font and `Renderer::with_font_bytes`, so no test depends on the
  machine's fonts.
- **Hyprland event listener** (PR 9, reused by 10 and 15). One thread on Hyprland's
  `.socket2.sock` that turns `workspace>>`, `activewindow>>`, … into messages for the main
  loop. Live keys need it for the active workspace, per-app volume for the focused app,
  app pages for the focused app. Falls back to doing nothing outside Hyprland.
- **Daemon control socket** (PR 11). Upstream's socket already carries GUI commands
  (`Command::ShowPage`, `SetBrightness`, tagged `"command"`). PR 11 adds more commands
  and a `Reply`, and `omadeckd press`/`dial` go through the daemon when it runs. Features
  before it don't depend on it.
- **Daemon-owned actions** (built in the rebase, `4c7bf70`; reused by 11, 12 and 16).
  One dispatch that routes the actions the deck loop runs itself (Open page, Back, Page
  step, Brightness) for every input path. `Action::is_deck_owned()` is the single list of
  them (groundwork).
- **Strip layers** (built in PR 7, `7aa52c3`; reused by 9, 10 and 14).
  `Renderer::panel_backdrop()` draws everything under a panel's layout (base colour,
  strip image slice, dial colour, dial image) and returns a `Backdrop` with the mean
  `tone` for contrast; later panels (live values, app volume, media) draw their items on
  top unchanged. Groundwork adds `draw_bar` (clamps) and separate title/value colours;
  PR 10 adds `Backdrop::tone_in(rect)` for contrast under each item.
- **Audio streams** (PR 10). Per-app volume extends `audio.rs` from devices
  (sink/source) to streams (sink inputs), done the same Omarchy-first way.

New action kinds land in this order, so `KINDS` indices are fixed. After the rebase onto
upstream's pages: Open page 6 and Go back 7 (upstream), Volume 8, Page step 9, Brightness
10, App volume 11 (PR 10), Multi action 12 and Delay 13 (PR 12), Media 14 (PR 14). Folder
is no longer a kind (upstream's Open page covers it). PRs 7, 8, 9, 11, 13 and 15 add no
action kinds (toggle and live are key properties, not actions).

## Handing a plan to the local model

Plans are handed to a local model (Qwen 3.8 27B). It copies what it's given and doesn't
reconcile contradictions, so a plan is ready only when:

- [ ] It matches the code as merged now. Rewrite it after the previous feature lands
      (and after each rebase); no "assumptions about PR N", copy PR N's real types in.
- [ ] Every Open decision is settled and written as "Decided: …".
- [ ] It's split into tasks of ~150–300 lines, one per session, in order. Each task names
      the files, the new types and signatures in full, and ends with an exact check
      (`cargo test -p omadeck-core <filter>`, `cargo clippy --all-targets -- -D warnings`)
      and what it should show.
- [ ] Each task has one fully written test to copy the pattern from.
- [ ] Code is referred to by symbol (function name, a string to grep for), not by line.
- [ ] Known traps are spelled out: no `if let … = get(&a.config.borrow()) { edit(..) }`
      (BUGS.md 4: clone, drop the borrow, then edit); recursive GUI builders take
      `Rc<dyn Fn…>`, not `impl Fn`; nothing slow on the deck loop.
- [ ] Hardware facts (command output, JSON samples, which tools Omarchy ships) are
      captured on the machine first and pasted into the plan or a fixture file.

Work the model can't do well: the rebase itself (conflict resolution), and GUI layout
judgement. Do those yourself, or with a stronger model.

The 2026-10-17 rewrite settled every open decision as "Decided: …". What was genuinely my
call went in each plan's **Ask before building** list; for review-fixes-2 and PRs 9–16
those were settled on 2026-10-17 too and moved to "Decided". Hardware facts still to
collect are in **Capture on the machine first**, with exact commands. Each plan opens
with what it takes from the plan before it. Plans after groundwork were written before
their base exists, so check those names against the branch first (each plan says how);
the branch wins.

### Later, only if wanted

- More Elgato strip layouts: `$C1` (two bars, e.g. output and mic). (`$A1`, value only,
  comes with live keys in PR 9.)
- Screensaver and idle dimming: dim or blank the deck when hypridle says idle or locked.
- From upstream's roadmap: **OBS** scene and recording keys, **multiple decks** with a
  layout per serial, **import/export** of shareable layouts.
- Plus XL (vertical strip, 6 dials) and the Neo's info bar, once someone can test them.

## Build it the omadeck way

Design every feature from omadeck's existing code and conventions, not from other Stream
Deck projects. New code should look as if the author wrote it:

- **Config** (`omadeck-core/src/config.rs`): serde structs, `#[serde(default)]` and
  `skip_serializing_if`, so existing configs stay valid and saved files stay minimal.
  Hand-editable TOML, relative paths resolved through `paths::resolve`.
- **Actions** (`action.rs`): reuse the `Action` enum where possible. Anything new becomes
  an `Action` variant with a `KINDS` entry, a `summary()`, and detached execution through
  `spawn()`.
- **Rendering** (`render.rs`): one renderer for the GUI preview, `snapshot` and the
  hardware. Theme colours come from `Theme`, labels use the configured font, and
  `follow_theme` is respected.
- **Daemon** (`omadeckd`): single-threaded loop, nothing slow on it (workers for anything
  that waits), config and theme hot reload, last-good-config fallback, events published
  on the hub.
- **GUI** (`omadeck-gui`): edit what the daemon understands. The preview matches the
  hardware.
- **Docs:** README feature list, roadmap checkbox, `examples/config.toml`, CHANGELOG.

## The Elgato app is the reference for what the user sees

The rule above covers **how** to build. For **what** the user sees and does (layouts,
sizes, gestures, defaults, naming), copy Elgato's Stream Deck app as described in its SDK
and help pages. Someone coming from it should recognise how omadeck behaves, and a choice
that matches Elgato's is easy to defend to upstream.

- **Before designing anything user-facing, check the Elgato docs** and write down what
  Elgato does, with links, in the feature's plan.
- **Copy Elgato by default.** Differ only on purpose: omadeck's principles (theme colours,
  `follow_theme`, the configured font, plain TOML), something that doesn't fit Omarchy or
  Linux, or "simpler for now" with Elgato's behaviour listed under that plan's "Later".
- **Write down every difference** under the plan's "Decisions to explain", and mark any
  Elgato detail we're guessing as unconfirmed until it's checked against the real app.
- **omadeck's words first** (dial, strip, page), Elgato's term alongside where it helps.

Useful pages (check they're still current when you use them):
- SDK overview: <https://docs.elgato.com/streamdeck/sdk/introduction/getting-started>
- Touch strip layouts: <https://docs.elgato.com/sdk/plugins/layouts-sd+>
- Touch strip layout items: <https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout>
- Manifest (states, controllers, encoder): <https://docs.elgato.com/streamdeck/sdk/references/manifest>
- Dials: <https://help.elgato.com/hc/en-us/articles/10715833118989-Elgato-Stream-Deck-Dials-Introduction>
- Backgrounds: <https://www.elgato.com/explorer/products/stream-deck/how-to-add-a-background-and-screensaver/>

## Working setup

### Still to do (once)

- [x] Personal identity for this repo only (done 2026-10-17 on the Mac:
      `nishantpillai5@users.noreply.github.com`). Repeat on the Omarchy machine if needed.
- [x] Push access to `nishantpillai5/omadeck` with my personal account (done 2026-10-17,
      fine-grained token).
- [x] `git remote add upstream https://github.com/thewizster/omadeck.git && git fetch upstream`
      (done 2026-10-16; not having it is how we missed upstream's pages).
- [ ] **Before starting any feature:** `git fetch upstream` and `git branch -r` to see
      upstream's branches, and check its open PRs, so we don't build what the author is
      already building.
- [ ] `git config rerere.enabled true`, which helps when rebuilding `daily`.
- [x] GUI type-check on the Mac (2026-10-17): a stub `pkg-config` that reports version
      99.0 lets `cargo clippy -p omadeck-gui --all-targets` check the GUI without gtk4
      installed (it can't link or run). The script is in
      [review-2026-10-17c.md](review-2026-10-17c.md).
- [ ] On the Omarchy machine: `sudo pacman -S --needed gtk4 libadwaita systemd-libs`, then
      `cargo build`. Before running a dev build: `systemctl --user stop omadeckd`.

### Branches

| Branch | Base | Purpose |
|--------|------|---------|
| `master` | `upstream/master` | Pure mirror. Never commit here. |
| `feat/*` | `upstream/master`, or the feature it depends on | One branch per PR. Small and self-contained. |
| `daily` | `upstream/master` + every `feat/*` merged in | What I build and run. Today it is `up/15-strip-long-press` (`26efd09`, on upstream v0.2.0): the 15 upstream PRs and nothing else. Before that `up/captures`, `fix/review-3`. |
| `fix/bugs` | `feat/strip-backgrounds` | The 2026-10-17 bug pass (all 21 bugs closed). |
| `fix/review` | `fix/bugs` | The 2026-10-17 review fixes, BUGS.md 22–43 (`87be396`). |
| `pr/01-*` … `pr/16-*` | each on the previous | **Deleted from the fork 2026-10-18, superseded by `up/*`.** Not consistently stacked after the 2026-10-18 re-stack: `pr/09` and `pr/15` sit on older tips of the branch below, so `pr/09` … `pr/16` lack BUGS.md 105. **The PR series (2026-10-17).** The `feat/*` branches with every fix folded in: each fix is its own commit at the end of the feature it fixes (`Folded from <sha>` in the message). `pr/07b-groundwork` is groundwork as its own PR, before PR 8. `pr/16-strip-long-press` has exactly `fix/review-3`'s tree; `pr/07-…` equals `fix/review`'s and `pr/08-…` equals `fix/review-2`'s. Every tip passes fmt, clippy (GUI type-checked) and the core and daemon tests. The old `feat/*` and `fix/*` branches stay as they were, so the SHAs in these notes still resolve. |
| `up/01-*` … `up/15-*` | each on the previous, from v0.2.0 (`5f4638b`) | **The upstream series (2026-10-18), what gets opened.** Fifteen PRs, fixes folded into their task commits, no internal references. [UPSTREAM.md](UPSTREAM.md). |
| `up/captures` | `up/15-strip-long-press` | The capture script and the machine captures; private, never upstream. |
| `up/work` | `fb9052f` | History: the linear series the old base's `up/*` were folded from. Fixes now go on `up/*` directly. |
| `plans` | `upstream/master` | These notes only. Never merged anywhere. |

New features from PR 7 on mostly touch the same files as PRs 1–6, so base each new branch
on the previous `feat/*` it builds on (PR 7 on `feat/brightness`, and so on) and note the
dependency in its plan. Rebuilding `daily`:

```bash
git fetch upstream
git checkout -B daily upstream/master
git merge --no-edit feat/plus-input feat/dial-actions ...   # every live feat/* branch
```

### Before every PR (from CONTRIBUTING.md)

- `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
- A line under **Unreleased** in `CHANGELOG.md`
- Name the hardware tested on (Stream Deck +, `0fd9:0084`)
- One feature per PR. At most 1–2 PRs open at a time.

### Order of work with upstream (later)

1. Build everything on `feat/*` branches and run it from `daily` until it works well.
2. Then contact the author (an issue or discussion on `thewizster/omadeck`) with a summary,
   the proposed config format and the PR order. Ask whether they want them and how.
3. Open the `up/*` PRs one at a time, in order ([UPSTREAM.md](UPSTREAM.md)). Each plan's
   "Decisions to explain" becomes the basis of its PR description.
