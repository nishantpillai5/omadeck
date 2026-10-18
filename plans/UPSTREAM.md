# The upstream series (`up/*`, 2026-10-18)

The branches to open upstream, one PR at a time, in this order. They replace the `pr/*`
series (deleted from the fork on 2026-10-18). Base:
`upstream/pages-and-folders` (`fb9052f`), upstream's own unmerged pages branch. PR 1's
description has to say it builds on that branch, or wait for it to land on `master`.

| # | Branch | Tip | What | From | Commits | Size |
|---|--------|-----|------|------|---------|------|
| 1 | `up/01-plus-support` | `9f69519` | Stream Deck + support: dial and strip input, dial actions, the strip drawn with `$X1` | PR 1, 2, 3 | 3 | +1703 −246 |
| 2 | `up/02-volume-dial` | `46b9b61` | The volume action; a volume dial's level on the strip | PR 4 | 1 | +1101 −80 |
| 3 | `up/03-strip-flick` | `8c211ff` | Flick the strip to change page; `page_step` | PR 5 | 1 | +328 −49 |
| 4 | `up/04-brightness` | `c820286` | The brightness action, the state file; the daemon drains input and paints once per pass | PR 6 | 2 | +676 −81 |
| 5 | `up/05-strip-backgrounds` | `1a7d50e` | Background images on the strip and dials | PR 7 | 2 | +966 −86 |
| 6 | `up/06-groundwork` | `fff8403` | The watcher, the daemon split, a hub that never blocks, the state file whole | PR 7b | 12 | +2430 −1243 |
| 7 | `up/07-toggle-keys` | `62a648d` | Toggle keys, busy faces, presets; Hyprland actions on a worker | PR 8 | 12 | +3753 −250 |
| 8 | `up/08-live-keys` | `f6e6760` | Live keys and panels; the Hyprland event listener | PR 9 | 8 | +2136 −82 |
| 9 | `up/09-app-volume` | `e7fe09d` | Per-app volume | PR 10 | 9 | +2122 −94 |
| 10 | `up/10-ctl` | `fc1c659` | `omadeckd` control commands | PR 11 | 6 | +1196 −150 |
| 11 | `up/11-multi-actions` | `29aec45` | Multi and delay actions | PR 12 | 6 | +1125 −66 |
| 12 | `up/12-media-dial` | `0fb61e7` | The media dial and media keys; a test daemon | PR 14 | 8 | +2428 −66 |
| 13 | `up/13-folder-gestures` | `128baa7` | Hold Back for Home, Back on release, page steps from nested folders | PR 13 | 3 | +338 −49 |
| 14 | `up/14-app-pages` | `ac18125` | Pages that follow the focused app | PR 15 | 3 | +835 −27 |
| 15 | `up/15-strip-long-press` | `812f088` | A second action per dial on a held strip panel | PR 16 | 2 | +181 −40 |

**`daily` = `up/15-strip-long-press`** (2026-10-18): the 15 PRs and nothing else. The
capture script and the machine captures are only on `up/captures` (PR 15 + them,
`4474b21`), never upstream.

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
