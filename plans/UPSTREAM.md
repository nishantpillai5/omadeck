# The upstream series (`up/*`, 2026-10-18)

The branches to open upstream, one PR at a time, in this order. They replace the `pr/*`
series (deleted from the fork on 2026-10-18). Base:
`upstream/pages-and-folders` (`fb9052f`), upstream's own unmerged pages branch. PR 1's
description has to say it builds on that branch, or wait for it to land on `master`.

| # | Branch | Tip | What | From | Commits | Size |
|---|--------|-----|------|------|---------|------|
| 1 | `up/01-plus-support` | `7fd7d3b` | Stream Deck + support: dial and strip input, dial actions, the strip drawn with `$X1` | PR 1, 2, 3 | 3 | +1682 −246 |
| 2 | `up/02-dials-and-strip` | `e26bfdf` | Volume and brightness dials with live levels, strip flick and `page_step`, background images | PR 4, 5, 6, 7 | 6 | +2906 −162 |
| 3 | `up/03-groundwork` | `e5c2916` | The watcher, the daemon split, a hub that never blocks, the state file | PR 7b | 12 | +2459 −1229 |
| 4 | `up/04-toggle-keys` | `e9708c9` | Toggle keys, busy faces, presets | PR 8 | 12 | +3754 −250 |
| 5 | `up/05-live-keys` | `6319f49` | Live keys and panels; the Hyprland event listener | PR 9 | 8 | +2138 −83 |
| 6 | `up/06-app-volume` | `be45ea1` | Per-app volume | PR 10 | 9 | +2114 −94 |
| 7 | `up/07-ctl-and-multi-actions` | `8bbab9d` | `omadeckd` control commands; multi and delay actions | PR 11, 12 | 12 | +2246 −208 |
| 8 | `up/08-media-dial` | `6b65d9e` | The media dial and media keys | PR 14 | 8 | +2436 −66 |
| 9 | `up/09-page-navigation` | `0000ec6` | Folder gestures (hold Back for Home, nested page steps) and pages that follow apps | PR 13, 15 | 6 | +1164 −59 |
| 10 | `up/10-strip-long-press` | `b799a1f` | A second action per dial on a held strip panel | PR 16 | 2 | +187 −40 |

Not for upstream: `up/captures` (`b62e45f`) = `up/10` + the capture script and the dated
machine captures (they hold private data). `daily` = `up/captures` since 2026-10-18.

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

**Not done (needs a decision):**

- **Scope.** Every reviewer said the combined PRs break "one feature per PR": PR 2 is
  four features, PR 7 and PR 9 two each, and PR 3 carries setup only later PRs use.
  Splitting gives about 14 PRs; each split is on a commit boundary.
- ~~Code in the wrong commits~~ **moved (2026-10-18)**, by patch (`tools/regroup/moves.py`,
  `patches/`; the tips are unchanged):
  - PR 2: the daemon's own fixes (draining queued input, painting once per pass, the
    volume refresh, brightness handling) are a commit of their own before background
    images, which keep only their two daemon hunks.
  - PR 4: the Hyprland worker is its own last commit, "Run Hyprland actions on a worker
    thread", with its CHANGELOG line; the progress-bar doc fix ("first NN%") is in PR 4.
  - PR 5: the event-socket fallback, the transcript test and its fixture are in "Listen to
    Hyprland's event socket"; the presets change is with the live field.
  - PR 6: the unused `activewindow-empty.json` left the series (only the capture tests
    read it; it is in `up/captures`).
  - PR 8: the test daemon is its own last commit.
  - PR 9: the deck-frame fix is in the folders' configurator commit, which is retitled
    for the folder-gesture docs it carries.

  Checked: all 78 commits type-check, and the core and daemon tests pass at every commit
  (not only the tips).
- **CHANGELOG:** no groundwork entry; Back-on-release and the strip hold are "Changed",
  not "Added"; PR 1's "Fixed" entry describes a bug upstream doesn't have; PR 2 lists
  fixes to its own new feature.
- **Smaller:** PR 8's docs say media keys go through Omarchy's commands, but with a
  watched player they go to `playerctl` directly; ctl multis share one run slot and an
  ignored press still answers OK; spun next/previous queue up to 8 commands per event.

## Still to do before PR 1

1. CI on each `up/*` tip (the GUI links there).
2. On the machine: reset to the new `daily`, build, smoke-test (the code is the verified
   code, so this is a quick pass, not VERIFY.md again).
3. The GUI checks VERIFY.md still has open; they were skipped.
4. Contact the author first (PLAN.md, "Order of work with upstream").
