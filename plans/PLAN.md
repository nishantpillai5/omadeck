# Plan: Stream Deck + support for omadeck

Personal working notes; not meant for upstream. Keep this file out of every PR
(see Setup).

## Status (as of the brightness PR)

**Do NOT open any PRs right now. This is private work.** The feature branches
(`feat/plus-input`, `feat/dial-actions`, `feat/touch-strip`, `feat/dial-volume`,
`feat/pages`, `feat/brightness`) are pushed only to my own fork and are *not* opened
as pull requests against upstream. Everything runs from my `daily` branch. Opening
PRs is a later decision I make explicitly; don't open or prepare them unless I say so.

**These plan files live on the `plans` branch**, not on `master` or `daily`: check
`git checkout plans` to read or update them. `master` is a pure mirror of
`upstream/master`.

PRs 1–6 are done and on `daily`: the Plus's dials and strip are read, dials run
actions, the strip is drawn, dials 1–3 are volume/mic dials (turn ±5, press mutes)
with a live $B1 panel on the strip, the deck has pages (a `page` action, per-page
keys, and a strip flick to page through), and dial 4 sets the deck's brightness
(persisted). After it: only the "later, only if wanted" items — folders, scrubbing.

## Goal

Add the roadmap items "Stream Deck +: dials and touch strip (volume, brightness,
scrubbing)" and "Pages" to omadeck, structured as a series of small, independently
mergeable PRs that upstream *could* merge one at a time. Meanwhile run everything from
my own `daily` branch, so I never wait on review. (See Status above: no PRs are opened
yet.)

## Principle: build it the omadeck way

Design every feature from omadeck's existing code and conventions, not from other
Stream Deck projects (including my old Python plugin). New code should look as if the
author wrote it:

- **Config** (`omadeck-core/src/config.rs`): serde structs, `#[serde(default)]` and
  `skip_serializing_if`, so existing configs stay valid and saved files stay minimal.
  Hand-editable TOML, relative paths resolved through `paths::resolve`.
- **Actions** (`action.rs`): reuse the `Action` enum where possible. Anything new
  becomes an `Action` variant with a `KINDS` entry, a `summary()`, and detached
  execution through `spawn()`.
- **Rendering** (`render.rs`): one renderer for the GUI preview, `snapshot` and the
  hardware. Theme colours come from `Theme`, labels use the configured font, and
  `follow_theme` is respected.
- **Daemon** (`omadeckd`): single-threaded loop, nothing slow on it, config and theme
  hot reload, last-good-config fallback, events published on the hub.
- **GUI** (`omadeck-gui`): edit what the daemon understands. The preview matches the hardware.
- **Docs:** README feature list, roadmap checkbox, `examples/config.toml`, CHANGELOG.

## Reference: the Elgato SDK for what the user sees

The rule above covers **how** to build things. For **what** the user sees and does
(layouts, sizes, gestures, defaults, naming), the reference is Elgato's own Stream Deck
software as described in its SDK. Someone coming from the Elgato app should recognise how
omadeck behaves, and a choice that matches Elgato's is easy to defend to upstream.

- **Before designing any user-facing behaviour, check the Elgato docs** and write down
  what Elgato does: touch strip layouts, dial and touch gestures, pages and folders,
  multi-actions, toggle states, brightness, and so on.
- **Copy Elgato by default.** Where we differ, it is deliberate and has one of these
  reasons:
  - omadeck's principles: theme colours, `follow_theme`, the configured font, plain TOML
    config;
  - something Elgato does that doesn't fit Omarchy or Linux;
  - simpler for now, with Elgato's behaviour listed under "Later".
- **Write down every difference from Elgato** under "Decisions to explain". Note any
  Elgato detail we're guessing (not stated in the docs) as unconfirmed, until it's checked
  against the real app or a screenshot.
- **Put links to the Elgato pages used** in each feature's plan and PR description.
- **Elgato names come second to omadeck's.** Our words are the roadmap's (dial, strip,
  page), but mention Elgato's term where it helps (e.g. layout `$B1` "Indicator").

Useful pages (check they're still current when you use them):
- SDK overview: <https://docs.elgato.com/streamdeck/sdk/introduction/getting-started>
- Touch strip layouts `$X1`/`$A0`/`$A1`/`$B1`/`$B2`/`$C1`: <https://docs.elgato.com/sdk/plugins/layouts-sd+>
- Touch strip layout items and their defaults: <https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout>
- Dials: <https://help.elgato.com/hc/en-us/articles/10715833118989-Elgato-Stream-Deck-Dials-Introduction>
- Touch strip (help article; WebFetch gets a 403, so open it in a browser): <https://help.elgato.com/hc/en-us/articles/10567698991629>
- Backgrounds: <https://www.elgato.com/explorer/products/stream-deck/how-to-add-a-background-and-screensaver/>

## Setup (once)

- [ ] Personal identity for this repo only: `git config user.email <personal or noreply email>`
      (the global config is my work email).
- [ ] Push access to `nishantpillai5/omadeck` with my personal account (SSH host alias or a
      fine-grained token). HTTPS uses the work account's GitHub login.
- [ ] `git remote add upstream https://github.com/thewizster/omadeck.git && git fetch upstream`
- [x] Keep these notes on the `plans` branch (see Branches), so they never land in a
      PR or on `daily`. (Originally tried `.git/info/exclude`; a dedicated branch is
      cleaner and keeps the history of the notes.)
- [ ] `git config rerere.enabled true`, which helps when rebuilding `daily`.
- [ ] Build on the Omarchy machine (the deck is plugged in there):
      `sudo pacman -S --needed gtk4 libadwaita systemd-libs`, then `cargo build`.
      Before running a dev build: `systemctl --user stop omadeckd`.

## Branches

| Branch | Base | Purpose |
|--------|------|---------|
| `master` | `upstream/master` | Pure mirror. Never commit here. |
| `feat/*` | `upstream/master`, or the feature it depends on | One branch per PR. Small and self-contained. |
| `daily` | `upstream/master` + every `feat/*` merged in | What I build and run. Rebuild it after any upstream or feature change. |
| `plans` | `upstream/master` | These working notes only. Never merged into `daily` or `master`. |

Rebuilding `daily`:

```bash
git fetch upstream
git checkout -B daily upstream/master
git merge --no-edit feat/plus-input feat/dial-actions ...   # every live feat/* branch
```

When a PR depends on an unmerged one, base its branch on that one and say in the
PR "depends on #N, review the last commit(s)". Or open it only once #N is merged.

## Before every PR (from CONTRIBUTING.md)

- `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
- A line under **Unreleased** in `CHANGELOG.md`
- Name the hardware I tested on (Stream Deck +, `0fd9:0084`)
- One feature per PR. At most 1–2 PRs open at a time.

## Order of work

1. Build everything on `feat/*` branches and run it from `daily` until it all works well
   on my Stream Deck +.
2. Then contact the author (an issue or discussion on `thewizster/omadeck`) with a summary
   of the features, the proposed config format, and the PR order below. Ask whether they
   want them and how.
3. Open the PRs one at a time, in order.

Because the author sees the work late, expect requests to change things, especially the
config format. Keep each `feat/*` branch tidy and separate so it can be reshaped or
rebased on its own. Write down design choices the author might question under
"Decisions to explain" below.

## Features, in PR order

Work out each design when its turn comes, from omadeck's code at that point, and from the
Elgato SDK for anything user-facing (see "Reference" above).

PRs 1–4 are planned in detail, one file per PR:
[pr1-plus-input.md](pr1-plus-input.md), [pr2-dial-actions.md](pr2-dial-actions.md),
[pr3-touch-strip.md](pr3-touch-strip.md), [pr4-dial-volume.md](pr4-dial-volume.md).
PR 5 is in [pr5-pages.md](pr5-pages.md); PR 6 in [pr6-brightness.md](pr6-brightness.md).
What PRs 1–4 share (crate facts, PR order, decisions to explain) is in
[dials-and-strip.md](dials-and-strip.md). That plan swaps PR 3 and PR 4: the strip comes
before volume, so volume can show its level on the strip.

### PR 5: `feat/pages`: pages
- Done: `b809d0f`, on `daily`.
- Per-page keys, a `page` action (next/previous/go-to), and a strip flick that pages
  through on the Plus. Dials and the strip are page-independent, as in the Elgato app.
- Folders are deliberately a later PR (they need a page *stack*).

### PR 6: `feat/brightness`: the deck's brightness
- Done: `b8fae3b`, on `daily`.
- A `brightness` action (change ±, or set absolute) for keys and dials, a $B1 panel
  on the strip for a brightness dial, and the value persisted to the config so it
  survives a restart. Running it surfaced three fixes, all in the same commit: the
  volume/mic bars now update optimistically (no read-back lag), a config reload no
  longer blanks the strip's level bars, and the configurator watches the config file
  so it stays in sync with a hand edit or a brightness the dial persisted.

### PR 1: `feat/plus-input`: read dials and the touch strip
- Done: `ffc6c54`, on `daily`.
- The daemon reads the Plus's dial and touch-strip events, logs them, and publishes them
  on the hub. Nothing else changes yet.
- `DeckInfo` reports the dials and the strip, so the GUI can draw them later.
- Confirm on hardware that `connect()` picks up the Plus and that the 8 keys work today.

### PR 2: `feat/dial-actions`: dials in the config, running existing actions
- Done: `e56a25c`, on `daily`.
- Dial settings in the config, in the same style as `[[key]]`.
- Turning and pushing run existing `Action`s, so any command, script or Hyprland
  dispatcher works with no new action types.
- Old configs must still load (add a test).

### PR 3: `feat/dial-volume`: volume (the roadmap's own example)
- Done: `4c1ec00`, on `daily` (landed after the strip, per the swap).
- System output and input volume, plus mute on push, done the way omadeck already
  talks to PipeWire and the system.
- Choose between a new `Action` variant and a dial-specific control, and note why.

### PR 4: `feat/touch-strip`: draw the touch strip
- Done: `1676c66`, on `daily`.
- First check what `elgato-streamdeck` offers for writing the strip; it may need a crate bump.
- Drawing lives in `omadeck-core::render`, follows the theme, and shows in `snapshot`
  and the GUI preview.
- Touch-strip input does something sensible by default.

### PR 5: `feat/pages`: pages
- The roadmap item "Pages and folders"; pages first, folders maybe later.
- Existing configs keep working unchanged.
- A way to switch pages on every deck (a key action), and on the Plus a touch-strip swipe.
- Before designing, check how the Elgato app does pages and folders: page navigation
  keys, swiping on the strip (it needs at least two pages), what happens to the dials on a
  page change, and folder back keys.
- GUI support. Probably the biggest PR; split daemon and GUI if needed.

### Later, only if wanted
- More dial behaviours from the roadmap: brightness, scrubbing.
- Per-app volume.
- Strip background images like the Elgato app: an 800×100 strip background and a 200×100
  background per dial.
- More Elgato strip layouts: `$A1` (value only) and `$C1` (two bars, e.g. output and mic).
- Commands to the daemon over the socket (brightness, page), if Hyprland binds or a
  shell plugin need them.

## Decisions to explain

Choices the author might question, with the reason for each. Fill this in while building;
it becomes the basis of the message to the author and the PR descriptions.

- (none yet)
