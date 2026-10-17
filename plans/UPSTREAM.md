# The upstream series (`up/*`, 2026-10-18)

The branches to open upstream, one PR at a time, in this order. They replace the `pr/*`
series as the PR candidates (`pr/*` stays on the fork for reference). Base:
`upstream/pages-and-folders` (`fb9052f`), upstream's own unmerged pages branch. PR 1's
description has to say it builds on that branch, or wait for it to land on `master`.

| # | Branch | Tip | What | From | Commits | Size |
|---|--------|-----|------|------|---------|------|
| 1 | `up/01-plus-support` | `3f883bc` | Stream Deck + support: dial and strip input, dial actions, the strip drawn with `$X1` | PR 1, 2, 3 | 3 | +1682 −246 |
| 2 | `up/02-dials-and-strip` | `6cbf1cd` | Volume and brightness dials with live levels, strip flick and `page_step`, background images | PR 4, 5, 6, 7 | 5 | +2906 −162 |
| 3 | `up/03-groundwork` | `c42a542` | The watcher, the daemon split, a hub that never blocks, the state file | PR 7b | 12 | +2459 −1229 |
| 4 | `up/04-toggle-keys` | `3662d88` | Toggle keys, busy faces, presets | PR 8 | 11 | +3754 −250 |
| 5 | `up/05-live-keys` | `03cd881` | Live keys and panels; the Hyprland event listener | PR 9 | 8 | +2138 −83 |
| 6 | `up/06-app-volume` | `0ffcfeb` | Per-app volume | PR 10 | 9 | +2114 −94 |
| 7 | `up/07-ctl-and-multi-actions` | `55846c8` | `omadeckd` control commands; multi and delay actions | PR 11, 12 | 12 | +2246 −208 |
| 8 | `up/08-media-dial` | `aaeb117` | The media dial and media keys | PR 14 | 7 | +2436 −66 |
| 9 | `up/09-page-navigation` | `26afc6a` | Folder gestures (hold Back for Home, nested page steps) and pages that follow apps | PR 13, 15 | 6 | +1164 −59 |
| 10 | `up/10-strip-long-press` | `2235154` | A second action per dial on a held strip panel | PR 16 | 2 | +187 −40 |

Not for upstream: `up/captures` (`f1faaeb`) = `up/10` + the capture script and the dated
machine captures (they hold private data). That's what `daily` should be next.

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

## Still to do before PR 1

1. CI on each `up/*` tip (the GUI links there).
2. On the machine: `daily` = `up/captures`, build, smoke-test (the code is the verified
   code, so this is a quick pass, not VERIFY.md again).
3. The GUI checks VERIFY.md still has open; they were skipped.
4. Contact the author first (PLAN.md, "Order of work with upstream").
