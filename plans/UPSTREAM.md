# The upstream series (`up/*`, 2026-10-18)

## Status (2026-10-20): upstream declined — the fork is now home

**Issue #1 was posted 2026-10-19, declined by thewizster on 2026-10-20 02:29 UTC, and
the user closed it at 08:43 UTC.** The author thanked the user and explained the project is
early and every release goes through hands-on hardware QA, so a series this size couldn't be
merged while the foundation settles. The user replied that they'd been testing on a Stream
Deck + only, and will **develop on their own fork with no upstream PRs**, inviting thewizster
to take things from the fork or ping for help. (Full thread in the issue.)

**Consequence:** the `up/*` series (15 branches, restacked on v0.2.0 `5f4638b`) is **parked on
the fork for reference** — no PRs will be opened from it. Development continues on the fork's
`master` (= `daily` = `up/15-strip-long-press`, `26efd09`). The series sits 3 commits behind
the latest `upstream/master` (`f4f27bd`) — the 0.3.0/0.4.0 roadmap, the keep-responsive fix
(`0684db0`), and the contributions note — none of which block the fork's work. Remaining work
is verification (the 6 open GUI checks in [GUI-CHECKLIST.md](GUI-CHECKLIST.md)), the parked
bugs (107–109, fixed on `up/07-toggle-keys` but unverified on the machine), and new features
on the fork. The `v0.2.0` tag points at `c770aab` (a later "Release v0.2.0" commit); the series
is based on `5f4638b`, the earlier one.

**Reset (2026-10-20):** `master` had still pointed at the old rebase (`812f088`) while `daily`
and `up/15` had moved to the v0.2.0 re-stack; it was force-pushed to `daily`'s tip (`26efd09`).
It is now kept in step with `daily`, and fork-only features merge into it (first: `omadeckd
events`, [automations.md](automations.md)).

**Fork-only fixes stay fork-only (2026-10-26).** Changes made for the user's own machine
and deck setup are committed to the fork's `master` and **never proposed upstream — and
never recommended for it**. First under this rule: the daemon re-reading a dial's live
value right after its own turn or strip tap (`0bb62ee`), which made the Prompts page's
audio-source dial track its turns. If a fork-only fix later looks upstream-worthy, the
decision is the user's to make, on their say-so only.

---

**Restacked on upstream's v0.2.0 (`5f4638b`, 2026-10-19).** Upstream merged
`pages-and-folders` and released 0.2.0 (undo and backups, live font changes, a warning
for pages with no way back). The 78 commits were rebased onto it with
`tools/restack/` (each commit's CHANGELOG regenerated: upstream's file plus the commit's
own entries under Unreleased; known conflict shapes resolved by rule; every commit
type-checked as it went). Merge fixes, each in the commit that needed it: `select()`
takes a `Selection` in upstream's new Back-key button; `remove_page` keeps upstream's
undo record for keys and clears every other action that opened the page (dials, on
actions, steps; undo doesn't re-link those); upstream's two new tests get the `folder`
and `apps` fields; the test-font renderer gets upstream's font fields; the font-change
hook moves into the split daemon; one long-press test now creates the page it removes.
All 78 commits type-check and pass the tests; every tip passes fmt and clippy.

**Undo covers every link (2026-10-19).** Upstream's undo for a deleted page re-linked
keys only; each PR now extends it for the links it adds, in the commit that makes them
clearable: dial turns and presses (PR 1, "Draw the Stream Deck + touch strip"), a toggle's
on action (PR 7, its last commit), steps inside a multi action (PR 11, "Add multi and
delay actions"), a dial's long press (PR 15). The record is a list of places (a key, its
on action, a dial slot, and a step); undo re-links a place only if it's still empty.
Tests for each kind; all 78 commits type-check and pass the tests.

**From now on, fix on the `up/*` chain directly** (commit on the branch of the PR, then
rebase the ones above). `up/work` and `tools/regroup/` built the series on the old base
and are history.
The branches to open upstream, one PR at a time, in this order. They replace the `pr/*`
series (deleted from the fork on 2026-10-18). Base: upstream's
`v0.2.0` release (`5f4638b`).

| # | Branch | Tip | What | From | Commits | Size |
|---|--------|-----|------|------|---------|------|
| 1 | `up/01-plus-support` | `ff9719c` | Stream Deck + support: dial and strip input, dial actions, the strip drawn with `$X1` | PR 1, 2, 3 | 3 | +1703 −246 |
| 2 | `up/02-volume-dial` | `2470092` | The volume action; a volume dial's level on the strip | PR 4 | 1 | +1101 −80 |
| 3 | `up/03-strip-flick` | `b9e833c` | Flick the strip to change page; `page_step` | PR 5 | 1 | +328 −49 |
| 4 | `up/04-brightness` | `fbf1873` | The brightness action, the state file; the daemon drains input and paints once per pass | PR 6 | 2 | +676 −81 |
| 5 | `up/05-strip-backgrounds` | `d50e6cc` | Background images on the strip and dials | PR 7 | 2 | +966 −86 |
| 6 | `up/06-groundwork` | `9955b48` | The watcher, the daemon split, a hub that never blocks, the state file whole | PR 7b | 12 | +2430 −1243 |
| 7 | `up/07-toggle-keys` | `01f8e05` | Toggle keys, busy faces, presets; Hyprland actions on a worker | PR 8 | 12 | +3753 −250 |
| 8 | `up/08-live-keys` | `0e5af31` | Live keys and panels; the Hyprland event listener | PR 9 | 8 | +2136 −82 |
| 9 | `up/09-app-volume` | `7f0645e` | Per-app volume | PR 10 | 9 | +2122 −94 |
| 10 | `up/10-ctl` | `da9352f` | `omadeckd` control commands | PR 11 | 6 | +1196 −150 |
| 11 | `up/11-multi-actions` | `d52860d` | Multi and delay actions | PR 12 | 6 | +1125 −66 |
| 12 | `up/12-media-dial` | `111cd2e` | The media dial and media keys; a test daemon | PR 14 | 8 | +2428 −66 |
| 13 | `up/13-folder-gestures` | `5ec9143` | Hold Back for Home, Back on release, page steps from nested folders | PR 13 | 3 | +338 −49 |
| 14 | `up/14-app-pages` | `ad6b9b3` | Pages that follow the focused app | PR 15 | 3 | +835 −27 |
| 15 | `up/15-strip-long-press` | `26efd09` | A second action per dial on a held strip panel | PR 16 | 2 | +181 −40 |

**`daily` = `up/15-strip-long-press`** (2026-10-18): the 15 PRs and nothing else. The
capture script and the machine captures are only on `up/captures` (PR 15 + them,
`d3358e8`), never upstream.

**Split (2026-10-18):** the three combined PRs went back to one feature each (each split on
the original per-feature groups, whose fix commits were already separate). Groundwork
stays one PR: its four parts are interleaved and don't split on commit boundaries. In the
split, the brightness PR carries the volume bars' "update at once, keep on reload" work
(its original commits did); the deck-frame fix folds into folder gestures.


## How it was built

- **Order.** Media dial (old 14) now comes before folders (old 13), so folders and app
  pages are one PR. Everything else keeps its old order; merging neighbours needs no
  conflict work. Strip backgrounds stay in PR 2: pairing them with the long press would
  have meant removing backgrounds from about 55 commits in between.
- **Fixes folded in.** No "fix bug N" commits: each fix's changes are in the task commit
  that last touched the same file (or, for a file no task touched, the one before the
  fix). Done on trees, so every PR's last commit has exactly the tree of the verified
  code, apart from the cleanup below.
- **Cleanup.** No PR numbers, BUGS.md numbers, task letters, `plans/` paths, capture
  dates or "on the machine" in code comments or commit messages; such sentences were
  dropped, not reworded. The committed fixtures' window titles, track names and host
  names were replaced with neutral ones. No "Tested on" lines in commits: the hardware
  goes in the PR description.
- **Checked (Mac, 2026-10-18).** Every one of the 75 commits type-checks on its own
  (GUI against the stub pkg-config); every tip passes `cargo fmt --check`, `cargo clippy
  --workspace --all-targets -- -D warnings` and the core and daemon tests (the three
  hub socket tests can't bind in the sandbox). `up/10` differs from the verified
  `fix/review-3` only in comments, scrubbed fixture strings, CHANGELOG and the captures.
  CI has not run on `up/*` yet.
- **Rebuilding.** The scripts are in [tools/regroup](tools/regroup): `build.py` folds
  `up/work` (the linear series, also on the fork) into `up/*`, with `clean.py` for the
  comments; `check-all.sh` and `check-tips.sh` are the checks. A fix found on the
  machine: commit it on `up/work` in its group, then run `build.py` again.

## Final review (2026-10-18)

Five reviewers read the ten PRs. **Fixed** (`tools/regroup/fixes.py`: text rewrites applied
to every commit where the buggy code is, all or nothing per commit, with a check that no
buggy version is left; changed files run through rustfmt):

- PR 1: a dial set to Open page or Go back failed ("only works on the deck"); dials now
  route page moves like keys.
- PR 2 (and PR 3's early commits): a hard-coded `HDMI-A-1` monitor for Omarchy 3 volume
  (now no `--monitor` when unknown); a config brightness edited while the daemon was
  stopped lost to the state file (groundwork's rule brought forward, with its test); the
  audio worker woke every 50 ms forever, and the configurator read both volumes every
  second on any deck.
- PR 5: a live command was killed after its interval (1–2 s) instead of 10 s.
- PR 6: every window retitle re-read `pactl`.
- PR 7: `omadeckd press N` with no page ran Home's key instead of the shown page's; a
  client that closes its write side (`socat`, `nc -N`) lost its reply (new test); `delay`
  capped at 10 minutes, the configurator's limit.
- Leftover internal references in comments and messages, the socket2 fixture's
  browsing-history-shaped title and keyboard layout, and the groundwork brightness commit
  message (it claimed file times; the code compares values).

Checked again: all 75 commits type-check on their own; every tip passes fmt, clippy
`-D warnings` and the tests (385 at PR 10).

**Done since (2026-10-18):** the scope split above; the code moves (`moves.py`); the
CHANGELOG (a Changed section for Back on release and the strip hold, the groundwork's
user-visible changes, two entries dropped that described fixes to the series' own new
features or a bug upstream doesn't have); the media docs (a command goes to `playerctl`
directly when a panel or key shows a player); and the small leftovers (`omadeckd press`
on a busy key and an ignored ctl multi are errors, not "ok"; a spun media dial sends one
command per turn; a comment said once). Checked: all 78 commits type-check and pass the
core and daemon tests; every tip passes fmt and clippy `-D warnings`.

**Issue posted (2026-10-19):** [thewizster/omadeck#1](https://github.com/thewizster/omadeck/issues/1),
"Stream Deck + support: dials and the touch strip" (from `drafts/issue-plus-support.md`).
Waiting for the author's answer before opening PR 1.

## Still to do before PR 1

1. CI on each `up/*` tip (the GUI links there).
2. On the machine: reset to the new `daily`, build, smoke-test (the code is the verified
   code, so this is a quick pass, not VERIFY.md again). **Done (2026-10-18, machine):**
   `daily` reset to `origin/daily` = `up/15-strip-long-press` (`812f088`); `cargo
   fmt --check`, `clippy --all-targets -- -D warnings` and `cargo test` clean (core 274,
   omadeck 9, omadeckd 114); release built and installed to `~/.local/bin`; daemon started
   and connected the Stream Deck + (`A00WA5081LZEYC`), `state` served, brightness set/read
   round-tripped (55 → 40), a `dial 0 right` volume step and a 50× `state` loop all fine.
3. The GUI checks VERIFY.md still has open; they were skipped.
4. Contact the author first (PLAN.md, "Order of work with upstream").
