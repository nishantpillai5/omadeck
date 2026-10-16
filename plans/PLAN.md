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

Known bugs (and the bug pass still to do) are in [BUGS.md](BUGS.md).

Shared facts and the decisions to explain for PRs 1–4 are in
[dials-and-strip.md](dials-and-strip.md). Hardware checks still open on PRs 1–4 and 6 are
listed in each file.

## What's next, in order

**Done (2026-10-17): [rebase onto upstream's pages](rebase-on-upstream-pages.md).**
PRs 1–6 are rebuilt on `upstream/pages-and-folders` (`fb9052f`); `daily` is now
`feat/brightness` (`b53a2e6`), and the Plus's page parts are the new PR 5
(`feat/plus-pages`, `4c7bf70`). Upstream is still at `fb9052f` with no open PRs; the
bugs U1–U9 in [BUGS.md](BUGS.md) are to go in the first message to the author.

Next: **PR 8, toggle keys** (`feat/toggle-keys`, based on
`feat/strip-backgrounds`).

Each feature gets its own `feat/*` branch, plan file and (later) PR. Work them in this
order; the order is mine, not upstream's roadmap order.

| # | Branch | Feature | Plan |
|---|--------|---------|------|
| 7 | `feat/strip-backgrounds` | Background images for the whole strip and per dial | [pr7](pr7-strip-backgrounds.md) |
| 8 | `feat/toggle-keys` | Keys with on/off faces that follow real state, plus a busy state with progress for slow toggles (e.g. a model server starting up) and for any key while its command runs | [pr8](pr8-toggle-keys.md) |
| 9 | `feat/live-keys` | Keys that show a clock, CPU, workspace, … and update themselves | [pr9](pr9-live-keys.md) |
| 10 | `feat/app-volume` | Per-app volume: a dial for a chosen app, or for the focused app | [pr10](pr10-app-volume.md) |
| 11 | `feat/ctl` | Drive the running daemon from the shell (`omadeckd page next`, `brightness 30`, …) | [pr11](pr11-ctl.md) |
| 12 | `feat/multi-actions` | One key runs several actions, with optional delays | [pr12](pr12-multi-actions.md) |
| 13 | `feat/folders` | A key opens a sub-page with a Back key (finishes "Pages and folders") | [pr13](pr13-folders.md) |
| 14 | `feat/media-dial` | Media dial: scrub, play/pause, track and progress on the strip | [pr14](pr14-media-dial.md) |
| 15 | `feat/app-pages` | Switch page automatically when an app is focused | [pr15](pr15-app-pages.md) |
| 16 | `feat/strip-long-press` | Hold a strip panel for a second action per dial (Elgato's "long touch") | [pr16](pr16-strip-long-press.md) |

### Shared building blocks

Several features need the same machinery. Build each piece once, in the first PR that
needs it, and have the later PRs reuse it rather than grow a second copy:

- **State watcher** (PR 8, reused by 9, 10, 14). `Probe`/`Value` types in core
  (`state.rs`), a daemon worker thread (`watch.rs`) that replaces today's audio worker: the main loop sends it what to watch, it polls or listens off the
  deck loop and sends back only *changes*, and the loop repaints only the keys or panels
  that show them. Toggle keys introduce it (state from a command); live keys add built-in
  sources; per-app volume and the media dial feed it audio-stream and player state.
- **Hyprland event listener** (PR 9, reused by 10 and 15). One thread on Hyprland's
  `.socket2.sock` that turns `workspace>>`, `activewindow>>`, … into messages for the main
  loop. Live keys need it for the active workspace, per-app volume for the focused app,
  app pages for the focused app. Falls back to doing nothing outside Hyprland.
- **Daemon control socket** (PR 11). Today the hub only *publishes* events. PR 11 adds
  commands in the other direction on the same socket (`Command`/`Reply` in `ipc.rs`), and
  `omadeckd press`/`dial` go through the daemon when it runs. Features before it don't
  depend on it.
- **Daemon-owned actions** (built in the rebase, `4c7bf70`; reused by 11, 12 and 16).
  One dispatch that routes the actions the deck loop runs itself (Open page, Back, Page
  step, Brightness) for every input path, so keys, dials, strip taps, multi-actions and
  ctl all go through it.
- **Strip layers** (built in PR 7, `7aa52c3`; reused by 9, 10 and 14).
  `Renderer::panel_backdrop()` draws everything under a panel's layout (base colour,
  strip image slice, dial colour, dial image) and returns a `Backdrop` with the mean
  `tone` for contrast; later panels (live values, app volume, media) draw their items on
  top unchanged.
- **Audio streams** (PR 10, reused by 14). Per-app volume extends `audio.rs` from devices
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

Each plan has a **Before handing this to the local model** list near the top with what
the 2026-10-16 review found in it.

Each plan ends with **Open decisions**: the choices it made a default for. Settle them when
that PR's turn comes, before building it.

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

- [ ] Personal identity for this repo only: `git config user.email <personal or noreply>`
      (it's currently the work email).
- [ ] Push access to `nishantpillai5/omadeck` with my personal account.
- [x] `git remote add upstream https://github.com/thewizster/omadeck.git && git fetch upstream`
      (done 2026-10-16; not having it is how we missed upstream's pages).
- [ ] **Before starting any feature:** `git fetch upstream` and `git branch -r` to see
      upstream's branches, and check its open PRs, so we don't build what the author is
      already building.
- [ ] `git config rerere.enabled true`, which helps when rebuilding `daily`.
- [ ] On the Omarchy machine: `sudo pacman -S --needed gtk4 libadwaita systemd-libs`, then
      `cargo build`. Before running a dev build: `systemctl --user stop omadeckd`.

### Branches

| Branch | Base | Purpose |
|--------|------|---------|
| `master` | `upstream/master` | Pure mirror. Never commit here. |
| `feat/*` | `upstream/master`, or the feature it depends on | One branch per PR. Small and self-contained. |
| `daily` | `upstream/master` + every `feat/*` merged in | What I build and run. |
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
3. Open the PRs one at a time, in order. Each plan's "Decisions to explain" becomes the
   basis of its PR description.
