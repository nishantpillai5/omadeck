# Bugs

Personal working notes, next to [PLAN.md](PLAN.md). Bugs found in the code on `daily`,
with where they are, what goes wrong, and which branch should fix them. Bugs 1–21 are
closed (see the bug pass under Rebase outcomes), except that 18's fix didn't work (see
29, now closed). The 2026-10-17 code review added 22–49: 22–43 are fixed on
`fix/review` (review-fixes.md, 2026-10-17); 44–49 were fixed on `feat/groundwork`
(groundwork.md), because their proper fix is that branch's structure. The second review
(`d7bd448..55d0d62`, groundwork and PR 8) added 50–72 and found 25 regressed and 31, 38,
45 and 48 only partly fixed; all of that goes to `fix/review-2`
([review-fixes-2.md](review-fixes-2.md)), except 65, which PR 13
([pr13-folders.md](pr13-folders.md)) fixes.

Each entry: **status** (open / fixed in `<commit>`), **found** (date, how), the file and
line on `daily`, what goes wrong, and the fix.

## Rebase outcomes (2026-10-17)

PRs 1–6 were rebuilt on `upstream/pages-and-folders` (`fb9052f`); `daily` is now
`fix/bugs` (`d7bd448`), based on `feat/strip-backgrounds` (`0312a0a`). Line numbers
below are stale on the new `daily`; the status lines (here and in each entry) are
current.

- **1** — retired: `App::key(i)` is page-aware on the new model (`key(page, i)`), so
  the editor's `get` closure reads the page being edited.
- **2** — retired: upstream's `omadeckd press KEY [PAGE]` names the page.
- **3** — retired as written: `page_count()` is gone; PR 13's folder counting is the
  live question now.
- **4** — fixed in `0397645` (PR 2): the rows copy the action out of the borrow first.
- **5** — fixed in `b949033` (PR 6): brightness lives in
  `$XDG_STATE_HOME/omadeck/state.toml`, written by a debounced saver thread; the daemon
  never writes `config.toml`.
- **6** — fixed by the rebase: every input path (key, turn, press, tap, flick) goes
  through `dispatch()`/`run_deck_action()`.
- **7** — retired: with bug 5's state file the slider and the daemon no longer fight
  over `config.toml`; the slider sends `Command::SetBrightness` and saves the start
  value.
- **8** — retired: `set_view_page` no longer exists; the GUI follows `PageChanged`.
- **12** — retired: no page spin on the named-page model; new pages are made by name.
- **19** — fixed in `4c7bf70` (PR 5): `FLICK_MIN_PX = 50`, mostly-vertical swipes don't
  count.
- **20** — retired: `KeyDown`/`ActionFailed`/`Connected` carry the page on upstream's
  IPC.

**Bug pass (2026-10-17, `fix/bugs` = `d7bd448`):** every open bug was re-verified on the
new `daily` and fixed there (based on `feat/strip-backgrounds`):

- **9** — fixed: `AudioCmd::Refresh(device)` drops the worker's `last` for that device, so
  the true level is resent even when it equals the guess.
- **10** — fixed: `run_volume` builds its command on the helper thread, so the Omarchy 3
  `focused_monitor` `hyprctl` runs off the deck loop.
- **11** — fixed: the strip preview reads a `strip_levels` cache filled by a background
  poller (1 s), no `pactl`/`wpctl` on the UI thread per edit.
- **13** — retired: with bug 5's state file the daemon never writes `config.toml`, and the
  rewritten `save_now`/`watch_config`/`reload_from_disk` cancel a pending save and reload
  the hand edit (with a toast). The remaining edge (a deleted file stops saving) is U1.
- **14** — fixed: the Device row shows for mute actions too, so a mic-mute key is settable.
- **15** — fixed: `flash_until` stores the tap time, so the flash lasts 150 ms.
- **16** — fixed: `wpctl_rel` uses `unsigned_abs()`.
- **17** — fixed: `parse_pactl` clamps like `parse_wpctl`.
- **18** — marked fixed, but it isn't: the flush sits inside the per-update loop, and a
  pass reads one HID report. Reopened as 29.
- **21** — `a` and `e` were already fixed by the rebase (Brightness test disabled;
  `reload_from_disk` calls `update_style`); `b`, `c`, `d` fixed (app pick no longer renames
  the dial; `default_for` gives a new volume the direction's sign; a smaller replugged deck
  clears a stale selection).

All 21 bugs were closed then (18 regressed, see 29). `daily` is `fix/bugs`.

## Open

### 1. The key editor shows the wrong action on page 2 and later

- **Status:** retired by the rebase (2026-10-17). **Found:** 2026-10-16, while planning folders (PR 13).
- **Where:** `crates/omadeck-gui/src/editor.rs:68`, in `build_key`, the `get` closure
  passed to `action_editor`:
  `move |c| c.key(i).map(|k| k.action.clone()).unwrap_or_default()`.
- **What goes wrong:** `Config::key(i)` returns the first key with that index on *any*
  page. Saved configs are sorted by `(page, index)`, so on page 2 or later the action
  editor fills its fields (and works out the current kind) from **page 1's** key with the
  same index, not the key being edited. Writes go to the right page (`App::edit` uses
  `key_mut_on(view_page, i)`), so editing one field can save a mix of the two keys'
  actions. The rest of `build_key` (label, icon, *Try it now*) already uses
  `key_on(app.view_page(), i)`; only this getter was missed in PR 5.
- **Repro:** two pages, key 1 set to different actions on each. Select key 1 on page 2:
  the editor shows page 1's action.
- **Fix:** `move |c| c.key_on(a.view_page(), i)…` (capture an `App` clone), on
  `feat/pages`, and add a GUI-free test if the getter can be pulled out. PR 13 also lists
  it; fix it on `feat/pages` first so it isn't tied to folders.

### 2. `omadeckd press N` ignores pages

- **Status:** retired by the rebase (2026-10-17). **Found:** 2026-10-16, while planning folders (PR 13).
- **Where:** `crates/omadeckd/src/main.rs:172`: `cfg.key(i)`.
- **What goes wrong:** it runs the first key with index `N` on any page: page 1's key if
  there is one, otherwise a key from a later page. There's no way to name the page.
- **Fix:** make it page 1's key only (`key_on(0, i)`), as PR 13 plans, and let PR 11
  route `press` through the daemon so it acts on the page actually shown.

## Latent (not a bug yet)

### 3. `Config::page_count()` counts every key

- **Where:** `crates/omadeck-core/src/config.rs:452`.
- **What:** one plus the highest `page` over *all* keys. Correct today, but once folders
  exist (PR 13) keys inside a folder would add pages to the root. PR 13 changes it to
  count root keys only (and PR 15's `[[page]]` tables).

## Review: `daily` and upstream `pages-and-folders` (2026-10-16)

A code review (reading only; `cargo test` couldn't run in the sandbox) of `daily`
(`4860e9f`, i.e. 3bb7eb4 + PRs 1–6) and of upstream's unmerged `pages-and-folders`
(`d4e458f` + `fb9052f`). Lines are on those commits. Bug 1 above was found again
(`editor.rs:68`); bug 2 too.

### `daily`: high

#### 4. Five editor rows panic on a RefCell double borrow

- **Status:** fixed in `0397645` (PR 2, 2026-10-17). **Found:** 2026-10-16, review (repro'd with a `rustc --edition 2024` test).
- **Where:** `crates/omadeck-gui/src/editor.rs:409` (Script "Run in a terminal"),
  `:446` (Sound volume), `:556` (Volume "Toggle mute"), `:580` (Volume device),
  `:598` (Volume "Change per turn").
- **What goes wrong:** `if let Action::X { .. } = get(&a.config.borrow()) { edit(&a, ..) }`.
  The `Ref` from the scrutinee lives through the then-block, and `edit` →
  `App::edit` does `config.borrow_mut()` → `BorrowMutError`, which aborts the app
  from a GTK callback. A regression from the `action_editor` refactor: toggling
  "Run in a terminal" worked on master. Reachable from keys and dials.
- **Fix:** `let cur = get(&a.config.borrow()); if let Action::X { .. } = cur { .. }`.

#### 5. The daemon rewrites the whole config on every brightness change

- **Status:** fixed in `b949033` (PR 6, 2026-10-17): the state file + debounced saver
  thread; the daemon never writes `config.toml`.
- **Where:** `crates/omadeckd/src/main.rs:640` `persist_brightness` → `Config::load()` +
  `save()` (`config.rs:426`).
- **What goes wrong:** before PR 6 the daemon never wrote `config.toml`. Now one turn
  of a brightness dial (or a brightness key) re-serializes the file: all comments,
  the commented `# [[dial]]` examples, unknown fields and the hand-chosen order are
  gone. It races with the user's editor. Every detent is a separate
  read/parse/serialize/`fsync`/rename **on the daemon loop** (no debounce), and each
  write is then picked up by the daemon's own watcher (`relevant()`, `:527`, only
  skips the tmp file) → full reload: theme, `clear_cache`, `paint_all`, audio
  re-watch. In the GUI each write also triggers `reload_config` → `rebuild_editor`,
  which steals focus from a text entry being typed in (bug 13).
  A brightness change made while the config is invalid is kept in memory, then
  lost on the next good reload.
- **Fix:** keep brightness out of the hand-edited file: a state file under
  `$XDG_STATE_HOME/omadeck/`, written debounced (idle/exit) from a thread. If it
  must stay in the config, at least debounce, write off-loop, and ignore the
  reload the daemon caused itself.

#### 6. Dial press and strip tap can't run Page or Brightness actions

- **Status:** fixed by the rebase (2026-10-17). (Also found 2026-10-16 while planning PR 16; that plan and commit
  `48f138e` called it "bug 4" before the numbers were merged. #4 is the RefCell panic.)

- **Where:** `crates/omadeckd/src/main.rs:447` (`EncoderDown`) and `:479`
  (`TouchScreenPress`) call `action::run(&dc.press)` directly; only the key path
  (`:382`) and the turn path (`:415`) route to `do_page`/`do_brightness`.
- **What goes wrong:** `action::run` bails on both (`action.rs:27`, `:29`), but the
  dial editor offers every kind for Press. Press = "Next page" or "Brightness 40%"
  → `DialActionFailed` on every press.
- **Fix:** one `dispatch(d, act, steps, source)` used by all four input paths.

#### 7. The settings Brightness slider no longer saves

- **Status:** retired by the rebase (2026-10-17): with bug 5's state file the slider
  and the daemon no longer fight over `config.toml`.
- **Where:** `crates/omadeck-gui/src/app.rs:599-601` `save_now`.
- **What goes wrong:** to avoid stomping a daemon-persisted brightness, `save_now`
  always copies `disk.brightness` into the config before saving, so the slider's
  own change (`:709`) is overwritten with the old value every time. The slider
  also never updates when a reload brings a new brightness (popover built once,
  `:703`).
- **Fix:** goes away with bug 5's state file; otherwise only adopt the disk value
  when the GUI didn't change brightness itself, and refresh the slider on reload.

#### 8. The editor goes stale on a page switch; edits land on the other page

- **Status:** retired by the rebase (2026-10-17): `set_view_page` no longer exists; the
  GUI follows `PageChanged`.
- **Where:** `crates/omadeck-gui/src/app.rs:422` `set_view_page` refreshes the grid,
  chips and title but not the editor.
- **What goes wrong:** press a page key on the deck: `KeyDown` opens that key's
  editor for page N, then `PageChanged` moves `view_page` to M. The editor still
  shows page N's key but writes through `key_mut_on(M, i)`: typing in Label copies
  the page-N label onto page M's key. Same after clicking a chip.
- **Fix:** `rebuild_editor()` (or re-`select`) in `set_view_page`.

### `daily`: medium

#### 9. Optimistic volume can stick at a wrong value

- **Status:** fixed in `d7bd448` (2026-10-17).
- **Where:** `crates/omadeckd/src/main.rs:612` `after_volume` guesses the level and
  sends `AudioCmd::Refresh`; the worker only sends a level that differs from its
  `last`, and `Refresh` doesn't clear `last`.
- **What goes wrong:** when the real value didn't change, the guess is never
  corrected: turning while muted shows unmuted (the guess sets `muted: false`,
  `wpctl set-volume` doesn't unmute); at the 0/100% clamp; when `wpctl` fails; and
  just after connect, when the guess starts from 0.
- **Fix:** `Refresh` drops `last` for that device (or always sends).

#### 10. `hyprctl monitors -j` runs on the daemon loop for every volume action

- **Status:** fixed in `d7bd448` (2026-10-17).
- **Where:** `crates/omadeck-core/src/action.rs:56` `run_volume` calls
  `audio::focused_monitor()` before spawning (Omarchy 3 backend).
- **What goes wrong:** every detent blocks on a `hyprctl` subprocess plus a JSON parse.
- **Fix:** move it into the `spawn_seq` thread.

#### 11. The strip preview spawns audio processes on the UI thread per edit

- **Status:** fixed in `d7bd448` (2026-10-17).
- **Where:** `crates/omadeck-gui/src/app.rs:457-468` `refresh_strip` → `audio::volume()`
  per volume dial (3 spawns each on Omarchy 4), from `edit_dial` (every keystroke
  in a dial label) and `refresh_all`.
- **Fix:** cache the level, or read it off the UI thread.

#### 12. Page spin and emptied pages

- **Status:** retired by the rebase (2026-10-17): no page spin on the named-page model.
- **Where:** `crates/omadeck-gui/src/editor.rs:116-118`; `app.rs:543-557`.
- **What goes wrong:** (a) moving a key to a page where that index is already taken
  makes two `[[key]]` with the same (page, index); the second is a ghost the GUI
  can't reach, and Clear removes both. (b) `edit`/`clear_key` don't clamp
  `view_page`: clear the only key on page 3 → chips show only "1", none current,
  and the grid keeps editing the invisible page 3. (c) The spin on an empty slot
  makes a blank key that `save_to` drops, so there's no way to start a new page
  from the GUI.
- **Fix:** swap or refuse on a clash; clamp in `edit`/`clear_key`.

#### 13. The GUI's config watcher drops or interrupts changes

- **Status:** retired by the rebase + bug 5's state file (2026-10-17); the remaining edge
  is U1 (a deleted file stops saving).
- **Where:** `crates/omadeck-gui/src/app.rs:681-687`.
- **What goes wrong:** a reload is skipped while a GUI save is pending and never
  retried; the pending save then overwrites the hand edit (only brightness is
  merged), with no toast. `suppress_reload` is one bool, so a daemon write inside
  the 150 ms window of a GUI save is swallowed with it. Every reload rebuilds the
  editor even when nothing visible changed (see bug 5).
- **Fix:** retry the skipped reload after the save; compare before rebuilding.

#### 14. Mic mute can't be set in the GUI

- **Status:** fixed in `d7bd448` (2026-10-17).
- **Where:** `crates/omadeck-gui/src/editor.rs:564` hides the Device row when
  "Toggle mute" is on, so a mute action is always Output.

### `daily`: low

- **15.** (Fixed in `d7bd448`.) The tap flash lasts 300 ms, not 150: `main.rs:472` stores
  `now + DIAL_FLASH`, then `:130`/`:351` compare `now.duration_since(t)`, which is 0
  while `t` is in the future. Store the tap time instead.
- **16.** (Fixed in `d7bd448`.) `wpctl_rel` panics on `change = -128` (`action.rs:133`,
  `i8::abs`); key and
  press paths don't clamp. Use `unsigned_abs()`.
- **17.** (Fixed in `d7bd448`.) `parse_pactl` uses `parse::<u8>` (`audio.rs:108`): above
  255% the Omarchy 4
  level read fails and the bar freezes. Clamp like `parse_wpctl`.
- **18.** (Fixed in `d7bd448`.) Volume and brightness repaint the strip panel per detent
  on the daemon
  loop (`write_lcd` + flush each time), against the `paint_dial` comment. Coalesce
  to one paint per loop pass.
- **19.** (Fixed in `4c7bf70`, PR 5: `FLICK_MIN_PX = 50`.) Any swipe flips the page
  (`main.rs:500`), however short or vertical; equal x counts as previous. Consider a
  minimum horizontal distance.
- **20.** (Retired by the rebase: upstream's IPC carries the page.) IPC has no page
  context: `KeyDown`/`ActionFailed` carry no page and
  `Connected` doesn't say the current page, so a configurator that attaches after a
  page change (or after a replug resets it to 0 without `PageChanged`) is out of
  sync. (Upstream solves this; see below.)
- **21.** GUI odds and ends. **Status:** all closed — `a` (dial "Try it now" enabled
  for Page/Brightness, `editor.rs:264`) and `e` (a hand-edited font doesn't reach the
  preview) were fixed by the rebase (`reload_from_disk` calls `update_style`); `b` (picking
  an app for "Turn left" renames the whole dial, `:283`), `c` (choosing Volume from the
  kind combo on Turn left *raises* the volume — `empty_of_kind` is `change: 5` for every
  slot) and `d` (a `Selection::Dial` survives replugging a deck without dials, `app.rs:326`)
  fixed in `d7bd448`.

### Upstream `pages-and-folders`

Report these upstream (or fix them while rebasing; see below). Lines are on `fb9052f`.

- **U1 (medium).** The GUI stops saving once `config.toml` is deleted with the window
  open. `save_now` (`app.rs:525-529`) sees `on_disk = None` ≠ `disk`, calls
  `reload_from_disk`, which returns early on the unreadable file (`:572`) without
  updating `disk`. Every later edit is dropped, with no toast. The comment "the next
  edit here writes it back" is wrong. Also: an unparsable external save cancels a
  pending edit with no toast, and a file deleted while the window is locked never
  unlocks it.
- **U2 (medium).** The colour picker still uses `a.edit(i, ..)` (`editor.rs:492`),
  not `KeyRef`/`edit_ref` like the other async pickers in `fb9052f`. A page change
  from the deck while the dialog is open puts the colour on the other page's key.
- **U3.** Rename doesn't recheck after its dialog: if the page went away in a reload
  meanwhile, `app.page` points at a page that doesn't exist and `with_page`'s
  fallback writes every edit into Home. It can also make duplicate names.
- **U4.** Page names aren't checked at load. Duplicate names, `"Home"` or `""` give
  unreachable or blank dropdown entries, and rename/delete hit both copies.
- **U5.** GUI ↔ daemon page sync has no echo suppression. Picking A then B in the
  dropdown can bounce the GUI back to A and close the editor. Browsing in the GUI
  also feeds the deck's Back history.
- **U6.** `wanted_page` (`omadeckd/src/main.rs`) is never cleared, so a much later
  reload that adds that page name moves the deck there.
- **U7.** Each config change reloads the daemon twice (inotify, then the 1 s stat
  poll, because `cfg_stamp` isn't updated on the inotify path): a double repaint.
- **U8.** A key linking to its own page shows "A (missing)" (`editor.rs` Page rows).
- **U9.** Launching through `systemd-run --user --scope` with stdio to /dev/null
  fails silently when there's no user bus (e.g. a hand-started `omadeckd run`).
  Before, the command ran directly.

What's right there: a flat page model (Home = top-level `[[key]]`, `[[page]] name`
with `[[page.key]]`, a folder is a page with a Back key), a bounded breadcrumb
`Nav`, `retain_existing` on reload, rename/delete fixing links, a replayed
`PageChanged` and a GUI→daemon command channel (`Command::ShowPage`). Old configs
load and save unchanged.

### Pages: our `ba0eced` vs upstream

Upstream's model is better on every axis except the Plus. It has named pages,
folders and Back, a nav stack, two-way IPC and async-safe edits (`KeyRef`). Ours
has relative next/previous (needed by a flick or a dial), the flick, and
page-independent dials. The two can't both live:

- Both use `type = "page"` with incompatible fields, and the trial merge
  (`git merge-tree`) leaves `ipc.rs` with **two `PageChanged` variants**.
- 9 files conflict (`config.rs` alone has 14 hunks). Even `feat/dial-volume`
  without pages conflicts in the same 9 files, mostly union conflicts plus
  `KINDS` index 6, which both sides claim.
- `ba0eced`'s `key_on`/`*_on` API collides with upstream's `Keys` newtype.

**Plan:** drop `ba0eced` and rebuild PRs 1–4 and 6 on `upstream/pages-and-folders`
(or master once it lands). This also means PR 13 (folders) is mostly done upstream.

1. Cherry-pick ffc6c54, e56a25c, 1676c66, 4c1ec00, da2b426. Renumber `KINDS`
   (…, Open page 6, Go back 7, Volume 8, Page step 9, Brightness 10; see the
   table in [rebase-on-upstream-pages.md](rebase-on-upstream-pages.md)). Dials stay top-level
   (page-independent for free). Dial painting takes no page. The dial editor uses
   the `KeyRef` pattern too.
2. Cherry-pick 4860e9f against upstream's page-aware `paint_all`, fixing bug 5 while
   at it.
3. Port the Plus parts as a new small commit: `Action::PageStep { change }` under its
   own tag (`page_step`), stepping over `[Home] + pages` in file order but **not
   folders** (needs a `folder`/`hidden` flag on `[[page]]`, or a flick lands in
   folders). Add a `Nav::switch` that replaces the current page rather than pushing
   it, so Back doesn't undo a flick. Keep `switch_page`'s tests. Wire up the flick
   and the dial turn. Extend `is_navigation()`.
4. Drop: `KeyConfig.page`, `MAX_PAGES`, `key_on`/`*_on`, page chips, the Page spin,
   `PageChanged { page: u8 }`. No migration: those configs only exist on `daily`.
   This also retires bugs 1, 2, 3, 8, 12 and 20.
5. `PageStep` + flick + the folder flag make a natural small PR to offer upstream.

## Review 2026-10-17

A code review of `daily` = `fix/bugs` (`d7bd448`) in four parts (core, render, daemon,
GUI), each finding re-checked against the code; the summary is in
[review-2026-10-17.md](review-2026-10-17.md). File and symbol are on `d7bd448` (no line
numbers). 22–43 are fixed on `fix/review` ([review-fixes.md](review-fixes.md)); 44–49 on
`feat/groundwork`, whose structure their fix needs.

### `fix/review`: high

#### 22. CI never runs: `ci.yml` is invalid YAML

- **Status:** fixed in `45ba8b7`. **Found:** 2026-10-17, code review (Psych and `actionlint` both reject it).
- **Where:** `.github/workflows/ci.yml`, step "Install dependencies". Since `2b389cc` (PR 3).
- **What goes wrong:** the new `# font-dejavu: …` comment sits at 8 spaces and `run:` at 6,
  so the step list doesn't parse ("did not find expected '-' indicator", line 16). GitHub
  shows "Invalid workflow file" and no job runs: no fmt, clippy, tests or shell check.
  Nobody saw it because `on:` only fires on `master`/`main` and pull requests.
- **Fix:** indent `run:` under the step; add `workflow_dispatch:` so our branches can be
  run by hand. Lands with 23, or CI goes red at once.

#### 23. The strip golden test depends on the machine's font

- **Status:** fixed in `45ba8b7`. **Found:** 2026-10-17, code review (fails on macOS; reproduced).
- **Where:** `render::tests::no_images_pixels_unchanged`, `renderer_with_font`, fixtures
  `tests/fixtures/strip-{x1,b1}.png`.
- **What goes wrong:** the renderer's font is whatever `fc-match monospace:weight=bold`
  returns. The fixtures were drawn with a Nerd Font, so the test fails on a Mac (Andale
  Mono) and in CI (DejaVu Sans Mono, no PUA glyphs for the `$B1` speaker), and breaks on
  the Omarchy box after `omarchy-font-set` or a font update. With no font at all,
  `renderer_with_font` returns `None` and the test passes without checking anything.
- **Fix (D9):** a bundled OFL font subset, `Renderer::with_font_bytes`, glyph `"A"` in the
  goldens, `OMADECK_UPDATE_GOLDEN=1` to regenerate. No test skips any more.

#### 24. Every page made in the GUI is a folder, so flicks and `page_step` skip them all

- **Status:** fixed in `6622480` (core) and `25fb9a1` (GUI). **Found:** 2026-10-17, code review (core and GUI parts both).
- **Where:** `omadeck-gui/src/app.rs` `App::add_page` (`folder: true` always), used by
  `ask_new_page` (**+**), `editor.rs` `new_folder` and the dial's "New page".
- **What goes wrong:** make Home + "Work" + "Media" with **+** and flick the strip: nothing
  happens (`step_page(None, 1) == None`); a "Next/previous page" key or dial does nothing.
  Nothing in the GUI shows or edits the flag, and the README doesn't mention `folder`.
- **Fix (D12):** `add_page(name, folder)`; **+** makes a normal page, "New page" on an
  Open-page action makes a folder; a "Folder" toggle in the page actions. Core gets
  `Config::add_page`, `is_folder`, `set_folder`.

#### 25. Renaming or deleting a page misses dial actions that open it

- **Status:** the rest fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). Before that: fixed in `6622480` (core) and `25fb9a1` (GUI) for keys and dials; **regressed
  by PR 8**: `all_actions(_mut)` don't walk a toggle's `on.action`, which can be Open page
  (review 2). Fixed again on `fix/review-2` (task 1, one recursive action walker).
  **Found:** 2026-10-17, review follow-up (checked in the code).
- **Where:** `config.rs` `Config::rename_page`, `remove_page` (through `all_keys_mut`) and
  `links_to`: keys only.
- **What goes wrong:** a dial press "Open page Media" keeps pointing at "Media" after a
  rename (the press then fails with "there's no page named …"), and keeps it after a
  delete. The delete dialog's "the N keys that open it" undercounts.
- **Fix:** `all_actions`/`all_actions_mut` over key actions and each dial's three slots;
  the dialog says "keys or dials".

#### 26. A hand edit of `brightness` in config.toml no longer reaches the deck

- **Status:** fixed in `23177ae`. **Found:** 2026-10-17, code review. A regression against upstream.
- **Where:** `omadeckd/src/main.rs` `run()`, the reload block: `Deck.brightness` is set
  once in `connect()` and `paint_all` uses it; upstream's `paint_all` used
  `cfg.brightness` on every reload. `snapshot` draws brightness dials at `cfg.brightness`.
- **What goes wrong:** edit `brightness = 30`, save: the deck stays where it was, now and
  after a restart (the state file wins). `omadeckd snapshot` shows the config's value
  while the deck shows the state file's.
- **Fix (D1, bug part):** on reload, a changed `brightness` is applied and saved like a
  `set`. `snapshot` uses `State::load().brightness.unwrap_or(cfg.brightness)`.

#### 27. A GUI brightness change with no deck plugged in is dropped

- **Status:** fixed in `23177ae`. **Found:** 2026-10-17, code review.
- **Where:** `run()`, `Command::SetBrightness`: `if let Some(d) = &mut deck`.
- **What goes wrong:** move the slider while the deck is unplugged: the daemon throws it
  away and the next connect starts from the old state-file value.
- **Fix:** with no deck, send the value to the saver; `connect` reads it back.

#### 28. The last brightness change is lost on shutdown

- **Status:** fixed in `23177ae`. **Found:** 2026-10-17, code review.
- **Where:** `run()` spawns `brightness_saver` without keeping its `JoinHandle`.
- **What goes wrong:** turn the brightness dial and stop the service within 200 ms
  (`systemctl --user restart omadeckd`, logout): the process exits before the debounced
  save, and the next start restores the previous value.
- **Fix:** keep the handle; at shutdown drop the sender and join. The saver already
  saves a pending value when the channel closes.

#### 29. Panels are still painted per HID report (bug 18 isn't fixed)

- **Status:** fixed in `6fb82c0`. **Found:** 2026-10-17, code review (checked against elgato-streamdeck
  0.13.1's `DeviceStateReader::read`).
- **Where:** `run()`, the `dirty_panels` flush inside `for u in updates`; the
  `paint_dial` calls for flash expiry, audio levels, `EncoderDown`/`EncoderUp` and taps.
- **What goes wrong:** the flush runs after every update, and `read()` returns one report
  per call anyway, so a hard spin still renders, encodes and writes the panel once per
  report; input queues behind the LCD writes and the bar lags. A brightness change from
  the GUI only marks the panel dirty, and the flush is reached only through device input,
  so the panel shows the old value until you touch the deck. A press paints its panel
  twice in one update; two levels flush twice.
- **Fix:** drain queued reports with a zero timeout (bounded), mark panels dirty
  everywhere, and paint dirty panels with one flush at the end of every pass, whatever
  the read returned. The `next_deadline` timing is D4, in groundwork.

### `fix/review`: medium

#### 30. A volume turn just after connect shows a guess from 0

- **Status:** fixed in `6fb82c0`. **Found:** 2026-10-17, code review.
- **Where:** `main.rs` `after_volume`: an unknown device's level defaults to `0` / `–`.
- **What goes wrong:** turn up right after the deck connects: the bar jumps to 5%, then
  snaps to the real level when the refresh lands.
- **Fix:** no guess without a known level; just send `Refresh`.

#### 31. A Hyprland dial turn runs up to eight `hyprctl` calls on the deck loop

- **Status:** the rest fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). Before that: partly fixed in `6622480`: more than one step runs on a thread, but a single
  step (every key press) still blocks the loop up to 2 s (4 s on Lua config), and the
  multi-step threads are unbounded and unordered across reports (review 2). The rest is
  `fix/review-2` task 6: every dispatch on one ordered worker. **Found:** 2026-10-17, code
  review.
- **Where:** `action.rs` `run_steps` → `run` → `hypr_dispatch` → `hyprctl_dispatch`
  (waits for the answer; two runs per step on Lua-config Hyprland).
- **What goes wrong:** a dial on `workspace e+1`, spun hard, blocks the loop for up to 8
  (or 16) round trips per report; with Hyprland stalled the deck freezes.
- **Fix (D6):** more than one step runs in order on a helper thread; one step stays inline
  so its error still shows.

#### 32. 21c's fix covers Volume only: Turn left Brightness or Page step goes the wrong way

- **Status:** fixed in `6622480` (core) and `c71b21c` (GUI). **Found:** 2026-10-17, code review.
- **Where:** `editor.rs` `dial_action_group` (`default_volume` only rewrites `Volume`);
  the Brightness "Mode" combo hard-codes `change: 5`.
- **What goes wrong:** Brightness on Turn left brightens; Next/previous page on Turn left
  goes to the next page; Volume on Press is `change: 0, mute: false`, a press that does
  nothing; Mode "Set" then back to "Change" gives `+5` on Turn left.
- **Fix (D10):** `Action::default_for(kind, Direction)` in core, used by the dial slots
  and the Mode combo. A Volume press mutes, as on Elgato's volume dial.

#### 33. The "Brightness dial" preset leaves a Volume press behind

- **Status:** fixed in `c71b21c`. **Found:** 2026-10-17, code review.
- **Where:** `editor.rs` `build_dial`, the Brightness preset (sets both turns, not `press`).
- **What goes wrong:** click **Volume dial**, then **Brightness dial**: turning changes
  brightness, but the panel shows the speaker and the output volume (`level_source`
  prefers Volume) and pressing mutes.
- **Fix:** the preset sets `press = Action::None`.

#### 34. A dial's editor closes whenever the deck changes page

- **Status:** fixed in `25fb9a1`. **Found:** 2026-10-17, code review.
- **Where:** `app.rs` `App::show_page`: `select(None)` on any page change.
- **What goes wrong:** a dial press "Next/previous page": `DialDown` selects the dial, the
  `PageChanged` right after blanks the editor. "Try it now" on a dial's Open page does too.
  Dials aren't per page, so only a key selection should close.
- **Fix:** clear only a `Selection::Key`.

### `fix/review`: low

#### 35. A dial's "Open page" list hides the page the window shows

- **Status:** fixed in `25fb9a1`. **Found:** 2026-10-17, code review.
- **Where:** `editor.rs` `action_rows`, `Action::Page`: filters out `app.current_page()`.
- **What goes wrong:** viewing "Media", you can't set a dial press to open "Media"; set by
  hand, it reads "Media (missing)". Right for a key (it can't open its own page), wrong for
  a dial.
- **Fix:** exclude the key's own page through its `Target`; dials exclude nothing.

#### 36. The strip background row is missing when the deck is plugged in after the GUI starts

- **Status:** fixed in `c71b21c`. **Found:** 2026-10-17, code review.
- **Where:** `app.rs` `settings_popover` (`if self.deck.borrow().strip.is_some()`), built
  in `build` and `reload_from_disk` only, not on a `Connected` relayout.
- **Fix:** rebuild the popover in `rebuild_grid`.

#### 37. Each right-click on the strip leaks a popover

- **Status:** fixed in `c71b21c`. **Found:** 2026-10-17, code review.
- **Where:** `app.rs` `strip_menu`: `popover.set_parent(&self.strip)`, never unparented.
- **Fix:** unparent it when it closes.

#### 38. A dial picker result lands in memory while config.toml is broken

- **Status:** the rest fixed in `8b33817` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). Before that: fixed in `c71b21c` for dials; `App::set_strip_background` still has no
  `locked` check, so a strip background picked while config.toml is broken is lost the same
  way (review 2). The rest is `fix/review-2` task 8. **Found:** 2026-10-17, code review.
- **Where:** `app.rs` `App::edit_dial` (no `locked` check, unlike `edit_ref`).
- **What goes wrong:** the file breaks while a dial's file dialog is open: the pick shows
  in the preview but is never saved, and vanishes on the next reload.
- **Fix:** return early when `locked`, as `edit_ref` does.

#### 39. A background image that fails to decode is decoded again on every paint

- **Status:** fixed in `31aa194`. **Found:** 2026-10-17, code review (a probe counted the loads).
- **Where:** `render.rs` `Renderer::backdrop_image`: a failure is stored as `None`, but
  only `Some(Some(..))` counts as a hit. The doc comment says it "warns once".
- **What goes wrong:** a truncated JPEG, a HEIC or a missing file as `background_image`:
  every panel paint (each detent, each tap flash) re-reads the file, tries to decode it on
  the deck loop and logs a warning.
- **Fix:** cache the mtime with the failure; an entry whose mtime matches is a hit, image
  or not.

#### 40. SVG backgrounds come out blurry, and a huge SVG allocates a huge pixmap

- **Status:** fixed in `31aa194`. **Found:** 2026-10-17, code review (probe: 134 blended columns).
- **Where:** `render.rs` `load_cover` → `load_svg_raw` (`rasterize_svg(tree, 1.0)`) →
  `fit_cover`.
- **What goes wrong:** a 24 × 24 icon-style SVG is rasterised at 24 × 24 and upscaled
  with Lanczos; `width="20000"` allocates 20000 × N before cropping. PR 7's plan said "at
  the cover scale".
- **Fix:** render straight into the `w × h` pixmap at the cover scale, centred.

#### 41. `fit_cover` resizes the whole image before cropping

- **Status:** fixed in `31aa194`. **Found:** 2026-10-17, code review (measured).
- **Where:** `render.rs` `fit_cover`.
- **What goes wrong:** a 4K wallpaper costs ~38 ms in the resize (on the deck loop, first
  paint); a 16 × 1000 image to 800 × 100 builds an 800 × 50000 intermediate (160 MB), and a
  2 × 2000 one about 2.5 GB. A 0 × 0 input returns 1 × 1, not `w × h`.
- **Fix:** crop in source space first, then resize to exactly `w × h`.

#### 42. The default mic glyph is "volume low", and the example's "right arrow" isn't one

- **Status:** fixed in `485a6ca` (the machine's cmap check is in the commit message;
  `volume_glyph_defaults` pins `INPUT_GLYPH = "f036c"`). **Found:** 2026-10-17, code review.
- **Where:** `render.rs` `INPUT_GLYPH = "f057f"` ("microphone"); `examples/config.toml`
  (`a mic ("f057f")`, `glyph = "f004e" # right arrow`).
- **What goes wrong:** in Material Design Icons U+F057F is volume-low (the mic is U+F036C)
  and U+F004E is a bold left arrow (right is U+F0054), so an input dial shows a speaker.
- **Fix:** `f036c` and `f0054`, after the check.

#### 43. Docs that disagree with the code

- **Status:** fixed in `485a6ca` (code points confirmed on the machine). **Found:** 2026-10-17, code review.
- **Where:** README hardware table ("swipe is reported but not used yet"; a flick has
  changed page since `4c7bf70`); README and example: `folder` undocumented, `brightness`
  described as the setting although the state file wins; `config.rs` docs on
  `Action::Brightness` ("persists it to the config"), `Page` ("a folder is just a page
  with a Back key", above the `folder` field), `LevelSource::Volume` ("the daemon's audio
  worker"); `render.rs` `DialStatus::level` ("volume, PR 4").
- **Fix:** reword each; see review-fixes.md task 8.

### `feat/groundwork`

#### 44. The GUI's brightness slider and preview show the config's value, not the deck's

- **Status:** fixed in `75738f8` + `ad261b5` (groundwork: `Event::Brightness`, the GUI mirrors it; an earlier note named `485a6ca`, which is a docs commit). **Found:** 2026-10-17, code review.
- **Where:** `app.rs` `refresh_strip` (`LevelSource::Brightness => cfg.brightness`),
  `settings_popover` (the slider), the slider handler (writes `config.brightness`).
- **What goes wrong:** turn a brightness dial from 50 to 80: the slider and the panel
  preview stay at 50, even after a GUI restart. Nudging the slider then jumps the deck. The
  slider doesn't refresh the preview either, and each pause while dragging rewrites
  config.toml for a value the daemon mostly ignores.
- **Why groundwork:** the fix is the daemon publishing `Event::Brightness` and the GUI
  mirroring it (D1); fix/review's 26 and 27 make the config edit and the no-deck case work
  meanwhile.

#### 45. Brightness can revert on a quick replug

- **Status:** the rest fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). Before that: fixed in `87e08c7` + `0badd8e` for the replug (brightness in `Runtime`, which
  outlives the deck). **Partly regressed at start** by `d9644be`'s rule "the config wins
  when its mtime is newer than the state file's": the configurator rewrites config.toml on
  every edit with the old `brightness`, so after any GUI edit a restart goes back to it
  (review 2). The rest is `fix/review-2` task 4 (compare values: `State.config_brightness`).
  **Found:** 2026-10-17, code review.
- **Where:** `main.rs` `Deck.brightness`, re-read in `connect()` from `state.toml`.
- **What goes wrong:** turn the dial, and the USB link drops within the saver's 200 ms:
  the new `Deck` reads the old value from the file.
- **Why groundwork:** brightness moves to `Runtime`, which outlives the deck (D1, D3).

#### 46. Volume changes run unordered, and the mic pop-up can show the old level

- **Status:** fixed in `edc4bdb` (one watcher thread, changes in order, each followed by its read). **Found:** 2026-10-17, code review.
- **Where:** `action.rs` `run_volume`: a thread per call; the Omarchy 4 mic pop-up sleeps
  80 ms on another thread and reads.
- **What goes wrong:** detents ~20 ms apart run `wpctl`/`omarchy-audio-*` concurrently;
  `wpctl set-volume 5%+` is read-modify-write, so a step can be lost. On a loaded machine
  the mic pop-up shows the value before the change.
- **Why groundwork:** volume changes run in order on the watcher thread, each followed by
  its read (D5).

#### 47. A stale volume read can overwrite the optimistic bar

- **Status:** fixed in `edc4bdb` (the read happens after the change on the same thread) and the in-flight follow-up (the bar shows confirmed + in-flight, so a late read-back can't pull it back — see the note at the end of groundwork.md). The in-flight total can drift for good on four paths; that's 54. **Found:** 2026-10-17, code review.
- **Where:** `main.rs` `after_volume` + `AUDIO_REFRESH_DELAY` + the worker's `Refresh`.
- **What goes wrong:** when the volume command takes longer than 150 ms (Omarchy 4's
  script, Omarchy 3's `hyprctl`), or a 1 s poll is already running, the read returns the
  old level and, since bug 9's fix dropped `last`, sends it: the bar shows 55, snaps to 50,
  and the next guess builds on 50.
- **Why groundwork:** the read happens after the change on the same thread (D5).

#### 48. A hung `pactl` or `wpctl` freezes the bars for good

- **Status:** the rest fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). Before that: partly fixed in `66373ff` + `edc4bdb` (`proc::output_timeout` on the watcher's
  reads and changes). Review 2: on timeout `output_timeout` kills only the direct child, so
  a script's children (and the pipes they hold) survive; and after a normal exit it waits
  up to the rest of the deadline for a pipe a background grandchild holds, on the watcher
  thread. The rest is `fix/review-2` task 4 (own process group, `killpg`, `GRACE`).
  **Found:** 2026-10-17, code review.
- **Where:** `audio.rs` `capture` (`Command::output()`, no timeout).
- **What goes wrong:** while PipeWire restarts, a read can hang; the worker stops, the
  bars freeze, and a `Refresh` per turn piles up in the unbounded channel.
- **Why groundwork:** `proc::output_timeout` on the watcher (D5).

#### 49. A slow configurator stalls the deck loop and is then dropped

- **Status:** fixed in `75738f8` (per-client bounded queue + writer thread; the loop never writes to a socket). Verified with a frozen GUI: the deck stayed responsive. The `Disconnected` published every 2 s with no deck is 55. **Found:** 2026-10-17, code review.
- **Where:** `hub.rs` `Hub::publish` (upstream): `write_all` to every client on the loop,
  200 ms write timeout. We added per-report events (`DialTurn`, `StripSwipe`, …).
- **What goes wrong:** the GUI busy for a moment (rebuilding the editor) while a dial
  spins: the socket fills, the loop blocks up to 200 ms, the client is dropped, and the
  GUI stops getting events until it reconnects.
- **Why groundwork:** per-client writer threads with a bounded channel (D8).

### Design items moved to groundwork.md

Not bugs; they're restructuring the later PRs need. See the review summary for detail.

- One `Daemon` struct and `run_input(Source, …)` instead of four copies of the input
  block; `dispatch` owns the volume follow-up (D3).
- Loop timing: `next_deadline()` instead of a fixed 100 ms read (D4).
- One watcher thread for all probes, in `probe.rs`/`watch.rs`, not `state.rs`; the GUI's
  `strip_levels` poller goes, the daemon publishes `DialStatus` (D2, D5).
- A generic `StateSaver` that writes the whole `State` (today it would wipe any new field),
  with `load_from`/`save_to` and fsync (D7).
- Renderer: stat only on `clear_cache` with `(mtime, len)` stamps, shared images, a cache
  sweep, `Level` clamped in `draw_b1`, `$B1` colours as PR 3 decided, one `dial_status`
  builder (D5, D9).
- GUI: `ActionSlot` instead of nine `impl Fn` parameters (blocks PR 12's recursion) (D10).
- One `is_deck_owned()` list used by `action::run` and the GUI (D11).
- Smaller, for later: two `Level` types; `run_steps`' folding untestable; images decoded
  on the GTK thread; a 2:1 thumbnail of the 8:1 strip; the strip preview blurry on HiDPI;
  `PageStep` from a nested folder steps from Home (PR 13).

## Review 2 (2026-10-17, d7bd448..55d0d62)

A second review, of what `fix/review`, `feat/groundwork` and PR 8 (`feat/toggle-keys`)
added on top of `d7bd448`, in the same four parts. Each finding below was re-checked
against the code at `55d0d62`; the ones marked *(proto)* were also fixed and tested in a
scratch build. The summary is in [review-2026-10-17b.md](review-2026-10-17b.md). File and
symbol are on `55d0d62`. All go to `fix/review-2` ([review-fixes-2.md](review-fixes-2.md),
base `feat/toggle-keys`), task in brackets, except 65 (PR 13, as decided).

### `fix/review-2`: high

#### 50. The configurator aborts when a key is drawn "on" without an on face

- **Status:** fixed in `b1cdf93` (core) and `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (core, render and GUI parts; panic
  reproduced). *(proto)*
- **Where:** core `config.rs` `KeyConfig::face_for` (`.expect("a key is on only with an on
  face")`), reached from `Renderer::render` and `editor.rs` `appearance_rows`; GUI `app.rs`
  `App::select` (never resets `face_tab`), `App::preview_status`, `App::refresh_key` (draws
  the mirrored `key_states` against the GUI's own config).
- **What goes wrong:** the daemon never asks for On without `[key.on]`, but the GUI does.
  Select a toggle key, click the **On** tab, then click a plain key (or press one on the
  deck): `build_key` draws the stale On tab on a key with no on face and the app aborts.
  Same with "Two faces" switched off on the On tab, the trash button, switching it off
  while a mic key is muted (the mirror says On), dragging an "on" key onto a plain key,
  or removing `[key.on]` by hand while the key is on.
- **Fix:** (R1) `face_for(On)` with no on face returns the off face. Core gets
  `KeyConfig::face_tabs()`/`clamp_face_tab()`; `App::select` resets `face_tab` to Off when
  the selection changes, `build_key` clamps it, and a drag swaps the two keys' mirrored
  states. [tasks 1, 7]

#### 51. The Service… preset moves the key to index 0

- **Status:** fixed in `b1cdf93` (core) and `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (core and GUI parts).
- **Where:** `editor.rs` `service_dialog`: `let mut k = KeyConfig::new(0); apply_service(&mut k, …);
  a2.edit(i, |k2| *k2 = k)`.
- **What goes wrong:** the whole key is replaced by one with `index = 0`. Apply it to key
  5: key 5 goes empty and the page has two `index = 0` keys (the old key 0 wins `get(0)`;
  both are saved). The key's background and colours are wiped too. The dialog is async and
  writes through `edit(i)`, so a page change on the deck meanwhile puts it on another
  page's key. The dialog also has no Cancel, and `d.unparent()` in the response handler
  pulls an `AdwAlertDialog` out of its host.
- **Fix:** apply in place through a `KeyRef` captured before presenting
  (`edit_ref(&at, |k| apply_service(k, …))`; `apply_service` validates before it changes
  anything). Presets that set a glyph clear `icon` (an icon beats the glyph). Cancel and
  Create responses, no `unparent`. [tasks 2, 7]

#### 52. The Service preset puts the health URL into the shell unquoted

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (reproduced with a mocked `curl`). *(proto)*
- **Where:** `presets.rs` `apply_service`: `curl -sf -o /dev/null -m 2 {url}`. PR 8's plan
  said it's quoted with `shell_words::quote`.
- **What goes wrong:** `http://127.0.0.1:8000/h?a=1&b=2`: `&` puts `curl` in the
  background, so the state command exits 0 and the key shows **on** while the service is
  still loading. `?` and `*` glob; a pasted URL can run any shell.
- **Fix:** `shell_words::quote(url)`; refuse a URL with a control character. Test with
  `sh -n`. [task 2]

#### 53. A slow toggle whose state never moves stays busy for good

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (confirmed with a `Poller` test). *(proto)*
- **Where:** `watch.rs` `Poller::record` (the `Leave` window); `daemon.rs`
  `after_toggle_press` (slow arm) and `Settle::Leaving`.
- **What goes wrong:** a slow press shows `Busy("")` and sends `Leave`. Every read that
  still says the old state equals the watcher's `last`, so nothing is sent, including the
  read after the 15 s window ends. The daemon's settle only expires when a value arrives,
  and none does. `slow = true` on a service that fails to start shows busy until the page
  is left. PR 8's plan says it "shows its true state after 15 s".
- **Fix:** a window that runs out forces its read to be sent, changed or not. [task 5]

#### 54. The volume in-flight total can drift for good

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2. *(proto)*
- **Where:** `daemon.rs` `in_flight`, `guess_volume`, `apply_value`/`settle_volume`,
  `detach`, `watch`; `watch.rs` `Poller::handle` (`WatchCmd::Volume`), `coalesce`.
- **What goes wrong:** only a read-back takes a change off the total, and four paths
  have none or a different one: the read after a change fails (a hung `wpctl`); the
  device isn't watched (a "Vol +5" key with no volume dial), and is watched later; an
  unplug mid-spin clears the total and the queued read-backs then drive it negative; and
  the daemon clamps the running total to ±100 while the watcher clamps each batch
  (`change = 10`, two fast 8-detent events: 100 sent, 160 confirmed). The bar then sits a
  fixed amount off the real level until a restart, and `settle_volume` shows a muted
  device as unmuted while anything is "in flight".
- **Fix:** (R5) every `WatchCmd::Volume` is acknowledged exactly once with the unclamped
  sum it applied, read or not; no clamp on the total; `detach` and `watch` leave it alone;
  the shown level keeps the read's mute. [task 5]

### `fix/review-2`: medium

#### 55. After a replug the configurators lose every toggle's state

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (daemon and GUI parts). *(proto)*
- **Where:** `daemon.rs` `Daemon::attach`/`detach` (clear `published`, not
  `published_keys`); `hub.rs` `Replay::remember` and the GUI's `Event::Disconnected`
  handler (both clear key states); `main.rs` `run()`, the no-deck branch.
- **What goes wrong:** after a replug `publish_keys` compares with the old
  `published_keys` and sends nothing for keys that didn't change, while the GUI and the
  hub's replay forgot them: a muted mic key shows off in the configurator until it
  changes. With no deck, `run()` publishes `Disconnected` on every 2 s scan, so the GUI
  clears and repaints everything every 2 s and drops a job's state published meanwhile.
- **Fix:** clear `published_keys` in `attach` and `detach`; publish `Disconnected` only
  from `detach` (the hub's replay starts with it). [task 5]

#### 56. A probe dropped and watched again mid-run gets two runs

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (confirmed with a `Poller` test). *(proto)*
- **Where:** `watch.rs` `Poller::handle` (`Watch` cancels and forgets; `Done` matches by
  probe only).
- **What goes wrong:** page A → B → A while A's state command runs: run 1 is cancelled,
  run 2 starts, and run 1's late `Done` (timed out) ends run 2's slot. A spurious "state
  command timed out" is logged, a third run can start while run 2 is going, and a stale
  result can land over a fresh one.
- **Fix:** (R5) a run id: `in_flight` holds `(u64, cancel)`, `Done { probe, run, result }`
  is ignored unless `run` matches. [task 5]

#### 57. The "State from" combo shows nothing for a mute key

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part; PR 8's unverified "Follows
  mute" check).
- **Where:** `editor.rs` `build_key`: `state_row.set_selected(key.state_choice() as usize as u32)`.
- **What goes wrong:** the enum's discriminant is used as the row: `Mute` is 2, but a mute
  key's list is `[Mute, Command]`, so nothing is selected. The handler indexes
  `choices[sel]`, which panics on `INVALID_LIST_POSITION`. Each rebuild also leaks the
  label `Vec` (`.leak()`).
- **Fix:** select by position in `state_choices()`; ignore an invalid selection; build the
  `StringList` from a local `Vec`. [task 7]

#### 58. The state command runs while you type it

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `editor.rs` `build_key`, the "State command" `EntryRow::connect_changed`.
- **What goes wrong:** each pause while typing saves (250 ms), the daemon reloads, and the
  watcher runs the half-typed command every `interval`: `systemctl --user stop` before the
  unit name is typed, and a 126/127 warning per prefix.
- **Fix:** commit on apply (`show_apply_button(true)`, `connect_apply`: Enter or ✓) and on
  focus-out (`EventControllerFocus` `leave`), never per keystroke. [task 7]

#### 59. Toggle rows don't follow their own switches; jobs can't get a progress bar

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `editor.rs` `build_key`: the "Busy while running" and "Slow" switches; the
  "Progress bar" row inside `if let Some(on) = &key.on`.
- **What goes wrong:** the two switches don't rebuild the editor, so the Busy tab and the
  Progress bar row don't appear or go until something else rebuilds. A plain
  `busy_while_running` key (no `[key.on]`) never gets the bar row, though PR 8's plan
  shows it for those keys.
- **Fix:** `rebuild_editor()` in both handlers; the bar row moves out of the `on` block,
  shown when `key.face_tabs()` has Busy. [task 7]

#### 60. Busy bars appear for numbers that aren't percentages

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (render part; measured). *(proto)*
- **Where:** `probe.rs` `progress` (also reads the part after the last `%`).
- **What goes wrong:** `"1/4"` → 4, `"Downloading 3 of 7"` → 7, `"Loading 43% step 5"` → 5.
  A job printing `1/4`…`4/4` with `bar = true` shows a bar stuck at 4%.
- **Fix:** (R7) only the first number a `%` follows. [task 2]

#### 61. Pressing a `$B1` dial recolours the value and the whole bar

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (render part). *(proto)*
- **Where:** `render.rs` `Renderer::render_strip_panel`: `fg` is the accent when pressed,
  then `B1Colors { title: fg, value: fg, fill: fg, … }`.
- **What goes wrong:** the comment, PR 3's decision and PR 10/14's plans say a press turns
  the title accent only; the value, fill and track turn accent too.
- **Fix:** (R8) a separate `title` colour; value and bar keep theirs. The goldens are
  unpressed and don't change. [task 3]

#### 62. The example's Backup key is never busy, and the toggle docs contradict the code

- **Status:** fixed in `f309370` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (core part; the TOML placement tested).
- **Where:** `examples/config.toml` (the index-12 Backup block), README "Toggle keys",
  CHANGELOG.
- **What goes wrong:** `busy_while_running = true` sits after `[key.on]`, so TOML puts it in
  the on table, where serde ignores it. README and example say a state command's **last**
  output line is the progress; it's the **first** (`run_state`), and the last only for a
  running job. "Every 1–60 s" is the GUI's limit, not the config's. `[key.on] action`
  isn't documented anywhere. "The command keeps running if the daemon stops" holds only
  for programs that ignore a closed stdout, and the key isn't busy any more after a
  restart. The bar's "last `NN%`" changes to "first" with 60.
- **Fix:** move the line above `[key.action]`; reword the rest. [task 9]

### `fix/review-2`: low

#### 63. Clearing a look field on the On or Busy tab creates that face

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (core part; tested). *(proto)*
- **Where:** `config.rs` `KeyConfig::set_look`: the blank branch calls `get_or_insert_with`.
- **What goes wrong:** clearing an On field on a plain key gives it `[key.on]`, which makes
  it a press toggle; clearing a Busy field leaves an empty `[key.busy]`. Reachable through
  50's stale tab once 50 no longer panics.
- **Fix:** clear only an existing face; drop a busy face equal to its default. [task 2]

#### 64. State commands of keys the deck doesn't have are polled

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (core and daemon parts). *(proto)*
- **Where:** `probe.rs` `watched_probes` (every key on the page); `daemon.rs` `watch`
  (called from `reload` and `show_page` with no deck too).
- **What goes wrong:** the example's key 10 on a Plus (8 keys), or a config from an XL,
  runs its command every 2 s. With no deck, the next save or GUI page change starts
  polling the page's commands again, though `detach` stopped them.
- **Fix:** (R9) `watched_probes` takes `n_keys: u8`; with no deck both counts are 0. [task 2]

#### 65. A held key is repainted as released

- **Status:** fixed in `a09db77` on `feat/folders`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (daemon part).
- **Where:** `daemon.rs` `Daemon::key_states` (`pressed: false`), used by
  `deck::flush_dirty`.
- **What goes wrong:** pressing a toggle key paints it pressed, `press_key` marks it dirty,
  and the end-of-pass flush paints the new state unpressed while the key is still down.
- **Fix:** `Deck.key_pressed: Vec<bool>` set on `ButtonDown`/`ButtonUp`, read by
  `key_states` through `Deck::key_held` (PR 13, task 2).

#### 66. A runner thread that fails to start silences its probe

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (daemon part). *(proto)*
- **Where:** `watch.rs` `run`, `if let Err(e) = spawned`.
- **What goes wrong:** the probe stays in `in_flight`, `next_due` skips it, and it is
  never polled again until the next `Watch`.
- **Fix:** on `Err`, take it out of `in_flight` and schedule it an interval later. [task 5]

#### 67. The face tab buttons aren't a group

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `editor.rs` `appearance_tabs`: plain `ToggleButton`s, no `set_group`.
- **What goes wrong:** clicking the active tab un-toggles it (no tab shown active);
  clicking another leaves two active until the idle rebuild.
- **Fix:** `set_group(Some(&first))`. [task 7]

#### 68. The configurator repaints everything every 2 s while the daemon is down

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `app.rs` `App::handle`, `Msg::DaemonDown` (sent after every failed connect).
- **What goes wrong:** every 2 s: `sync_slider`, `refresh_strip`, then `refresh_all`
  (which draws the strip again): every key at 192 px, forever, while the daemon is
  stopped.
- **Fix:** do the clearing only when `daemon_up` was true; drop the extra
  `refresh_strip`. [task 8]

#### 69. The starter layout's night light ignores that it needs `jq`

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `starter.rs` `layout`: `PRESETS[1]` and `PRESETS[2]` by index.
- **What goes wrong:** without `jq` the night light key's state command fails (127) on
  every poll after one warning, and the key never shows on. Indexing breaks silently if
  the preset list is reordered.
- **Fix:** look presets up by name; without the binary, a plain night light key. [task 8]

#### 70. An action editor overwrites its group's description

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `editor.rs` `action_rows`: `group.set_description(..)` for Volume, Brightness
  and Hyprland.
- **What goes wrong:** a key whose action is Volume loses "Home · Row 1 · Column 2"; the
  on-action editor rewrites the Toggle group's; PR 12's nested editors would all fight
  over one description.
- **Fix:** (R9) `action_rows` adds its own hint row instead. [task 8]

#### 71. Some render tests depend on the machine's font and theme

- **Status:** fixed in `b1cdf93` on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (render part). *(proto)*
- **Where:** `render.rs` tests built with `Renderer::new` (`strip_panel_isolation`,
  `panel_isolation_with_strip_image`, `busy_face_uses_accent_label_by_default` and 16
  others): `fc-match` and `Theme::load_omarchy()`.
- **What goes wrong:** with no system font the isolation tests fail; `busy_face_…` fails on
  a theme whose accent is `#ff0000`. This is why CI still needs `ttf-dejavu`.
- **Fix:** every render test uses the bundled font and `Theme::default()` (R9's
  `with_font_bytes` + `TEST_FONT`). [task 3]

#### 72. "New page" on a toggle's on action restyles the key's off face

- **Status:** fixed in `8b33817` (GUI, not compiled yet) on `fix/review-2`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 2 (GUI part).
- **Where:** `editor.rs` `build_key`, `on_slot.on_new_page` → `new_folder(a, i)`.
- **What goes wrong:** `new_folder` gives the key a folder glyph (and the page name as its
  label) when it has none: right for a key that opens a folder, wrong when only its on
  action does.
- **Fix:** the on slot makes the folder page without touching the key
  (`App::add_page(&name, true)` with the key's label as the base). [task 8]

## Review 3 (2026-10-17, 55d0d62..13597e8)

A third review, of what was built on the Mac: `fix/review-2` and PRs 9–16, up to
`feat/strip-long-press` (`13597e8`). Six parts: core (config, actions, IPC, docs), core
(probes, audio, player, render), daemon (loop and input), daemon (watcher, hub,
Hyprland), GUI (app, pages, apps), GUI (editor). The summary is in
[review-2026-10-17c.md](review-2026-10-17c.md). Each entry names the branch that brought
the bug in; fix it there (or in a `fix/review-3` on top, as decided when fixing).
*(test)* means a test in a scratch copy showed it; *(read)* means it was traced through
the code; *(plausible)* needs the machine to confirm. All 32 are fixed on `fix/review-3`
(on top of `feat/strip-long-press`, now also `daily`), most with a regression test.

### Review 3: high

#### 73. Unplugging the deck while music plays spins the daemon at 100% CPU

- **Status:** fixed in `816caac` (test in abad8d7) on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon loop) *(read, re-checked)*.
- **Branch:** `feat/media-dial` (`75694de`).
- **Where:** `Daemon::detach`, `Daemon::flush`, `Daemon::next_deadline`; `run()` in
  `main.rs`, the no-deck branch.
- **What goes wrong:** while a media panel plays, every `flush` sets `media_due` up to 1 s
  ahead. Only `pass()` clears it, and `pass()` runs only with a deck. `detach` clears
  `repaint_at` but not `media_due`, and with no deck `flush` returns before updating it.
  So `next_deadline()` keeps returning a past instant, `wait_for` gives zero, and the
  no-deck loop never sleeps until a deck is back. Undocking a laptop with music playing
  is enough.
- **Fix:** `self.media_due = None;` in `detach` (or ignore `media_due` in `next_deadline`
  when there is no deck).

### Review 3: medium

#### 74. Each `omadeckd` ctl call leaks a thread and a socket until the next event

- **Status:** fixed in `887d9ac` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(test)*.
- **Branch:** `feat/ctl` (`32390eb`). The writer and reader structure is older, but before
  ctl every client was a long-lived configurator.
- **Where:** `hub.rs` `Hub::start` (accept loop), `spawn_writer`, `read_requests`,
  `Hub::publish`.
- **What goes wrong:** when a ctl client exits, its reader returns, but the writer thread
  waits on its queue (the hub still holds the sender) and keeps the fd. Only the next
  `publish` makes it write, fail and exit. An idle daemon publishes almost nothing, so a
  status-bar script polling `omadeckd state` every second leaks one thread and one fd a
  second. At the 1024 fd limit, `accept` fails with EMFILE and
  `listener.incoming().flatten()` spins at 100% CPU; hidapi and every child process fail
  too.
- **Fix:** the reader queues an empty sentinel on every return path, and the writer stops
  on it. Replace `incoming().flatten()` with a loop that sleeps briefly on `Err`.

#### 75. A seek key does nothing unless something else watches the player

- **Status:** fixed in `324c99d` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core probes) *(test)*.
- **Branch:** `feat/media-dial` (`839d72d`, `75694de`).
- **Where:** `probe::watched_probes`, `probe::media_target`; the daemon's `Action::Media`
  arm (`seek != 0 && target.is_none()` → "nothing known to seek").
- **What goes wrong:** a key `media` with `seek = 10` on a page with no media dial and no
  play/pause toggle: nothing watches `Player(None)`, so there's no target and the daemon
  drops the seek with only an info log. Without the daemon the same key seeks (untargeted
  `playerctl position 10+`), which is what the plan's `media_argv` table says.
- **Fix:** drop the seek only when the probe is watched and read "nothing playing";
  otherwise send it untargeted.

#### 76. Media keys can target a stale player read on another page

- **Status:** fixed in `324c99d` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core probes) *(read)*.
- **Branch:** `feat/media-dial` (`839d72d`, `75694de`).
- **Where:** `probe::media_target`; `Daemon::dispatch` (`Action::Media`). Values of
  unwatched probes are kept on purpose (faces).
- **What goes wrong:** a Media page's play/pause key read `chromium.instance4821`. You go
  Home, close the tab and start Spotify. A Home key's Next runs `playerctl
  --player=chromium.instance4821 next`: it fails or skips the wrong player, and skips
  Omarchy's pop-up.
- **Fix:** target from a player value only while its probe is watched; otherwise use the
  action's own `player` (untargeted when blank).

#### 77. The Volume and Brightness dial presets keep the dial's live value

- **Status:** fixed in `5f9cf64` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI editor) *(read)*.
- **Branch:** `feat/app-volume` (`7ba769e`, the Presets menu); `live` on dials is PR 9's.
- **Where:** `editor.rs` `dial_presets_button`, the `volume` and `brightness` actions.
- **What goes wrong:** a dial showing a clock gets Presets › Volume dial: the actions
  change, but `level_source` lets the explicit `live` win, so the panel still shows the
  clock. The other three presets clear `live`.
- **Fix:** `d.live = None;` in both closures.

#### 78. `cargo clippy --all-targets -- -D warnings` fails for the GUI

- **Status:** fixed in `9df8df3` on `fix/review-3`; cherry-picked to `feat/live-keys`
  (`9abbf73`, the branch the code lives on) and the chain rebased on it (2026-10-17,
  machine: clippy 0.1.99 clean from `feat/live-keys` through `fix/review-3` and
  `pr/09` … `pr/16`, GUI linked). **Found:** 2026-10-17, review 3 (GUI) *(test: GUI
  type-checked with a stub pkg-config)*.
- **Branch:** `feat/live-keys` (`969ab3a`: two `type_complexity` warnings on the `LiveEdit`
  closures in `editor.rs`); `feat/media-dial` (`large_enum_variant` on `app.rs` `Msg`:
  PR 14's events made `Event` 280 bytes).
- **What goes wrong:** every plan's check step, and CI, run clippy with `-D warnings`: CI is
  red from `feat/live-keys` on.
- **Fix:** a `type LiveEdit = Rc<dyn Fn(&Rc<App>, Box<dyn FnOnce(&mut Option<Live>)>)>;`
  alias (the fix the review checked); `Msg::Event(Box<Event>)` or an `#[allow]` with a
  reason on `Msg`.

### Review 3: low

#### 79. A reload that moves the page while Back is held runs the wrong key on release

- **Status:** fixed in `afad70e` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon loop) *(read)*.
- **Branch:** `feat/folders` (`a09db77`, `back_held`); `feat/app-pages` (`54ea4fb`) added a
  trigger (`follow_focus` in `reload`).
- **Where:** `Daemon::reload` (moves `nav` without `show_page`); the `ButtonUp` arm.
- **What goes wrong:** hold Back in folder F while a configurator save deletes F (or adds
  an app link): the reload moves the deck, and the release runs whatever key `i` is on the
  new page. The new page's key `i` also stays drawn pressed.
- **Fix:** in `reload`, clear `back_held` always and `key_pressed` when the page moved.

#### 80. A window title change restarts the app-page settle and re-reads `pactl`

- **Status:** fixed in `371f3d0` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (both daemon parts) *(read)*.
- **Branch:** `feat/app-pages` (`54ea4fb`, the settle); `feat/app-volume` (`554812b`, the
  `Focus` send).
- **Where:** `Daemon::apply_hypr`, the `ActiveWindow` arm; `hypr.rs` turns every
  `windowtitlev2` of the focused window into `ActiveWindow`.
- **What goes wrong:** a terminal or build tool that retitles faster than every 200 ms
  never settles, so its linked page never shows; each retitle also runs `pactl` when a
  focused-app dial is watched.
- **Fix:** send `Focus` and restart the settle only when the window's address, class or
  pid changed; always store `rt.window`.

#### 81. The Hyprland listener can stay stuck on a crashed instance's directory

- **Status:** fixed in `e904fce` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(plausible)*.
- **Branch:** `feat/live-keys` (`635a384`).
- **Where:** `hypr.rs` `find_instance`, `run`.
- **What goes wrong:** if Hyprland crashes and leaves its old `.socket2.sock`, the env
  signature's dir wins every time, `connect` is refused, and the newest-dir fallback is
  never tried: workspace keys freeze and app pages stop following.
- **Fix:** try `connect` on each candidate (env dir first, then newest first).

#### 82. A slow `playerctl` read blanks the media dial and play/pause keys

- **Status:** fixed in `05c5897` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(read)*.
- **Branch:** `feat/media-dial` (`57c828e`).
- **Where:** `player::read` (timeout → `Ok(None)`), `watch.rs` `Poller::run_due`.
- **What goes wrong:** a timeout reads as "nothing playing", so the panel goes idle and
  the key off, then back: flicker.
- **Fix:** tell a timeout from "no players" and keep the last value, as volume reads do.

#### 83. A cover that won't decode is decoded again every second

- **Status:** fixed in `05c5897` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(test)*.
- **Branch:** `feat/media-dial` (`57c828e`).
- **Where:** `watch.rs` `with_art` (its comment says it doesn't retry).
- **Fix:** remember the failed path on the poller and skip it.

#### 84. On Omarchy 3, every media command runs `hyprctl monitors` even when unused

- **Status:** fixed in `05c5897` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(read)*.
- **Branch:** `feat/media-dial` (`57c828e`).
- **Where:** `watch.rs` `Poller::handle` (`WatchCmd::Media`); `player::media_argv`.
- **Fix:** look the monitor up only for an untargeted non-seek, non-stop command.

#### 85. Two player probes run two `playerctl -a` reads a second

- **Status:** fixed in `05c5897` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (daemon watcher/hub) *(read)*.
- **Branch:** `feat/media-dial` (`57c828e`).
- **Where:** `watch.rs` `Poller::run_due` (`Probe::Player`).
- **Fix:** one read per pass, then `player::pick` per watched probe (as app volume does).

#### 86. A long ▶ (`Command::Run`) is refused by the 4096-byte line cap

- **Status:** fixed in `29b90a4` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `feat/ctl` (`dbe1ae3`).
- **Where:** `ipc::MAX_LINE`, `hub::read_requests`.
- **What goes wrong:** a multi with a few dozen steps or a long command goes past 4 KiB and
  gets "command too long". A cut inside a UTF-8 character gets no reply at all ("daemon
  busy").
- **Fix:** raise the cap (64 KiB) and decode after `read_until(b'\n')`.

#### 87. A clock with `%-S` or fractional seconds updates only every few seconds

- **Status:** fixed in `e62ccbf` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(test)*.
- **Branch:** `feat/live-keys` (`837c11e`).
- **Where:** `live::needs_seconds`.
- **Fix:** skip jiff's flags and width (`-_0^#`, digits, `.`, `:`) after `%`, and add `f`
  and `N`.

#### 88. `parse_command` calls a known command with a bad field "unknown"

- **Status:** fixed in `77d7168` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(test)*.
- **Branch:** `feat/ctl` (`dbe1ae3`).
- **Fix:** keep serde's message for a known name (`bad “press” command: …`).

#### 89. The Play/Pause preset needs `playerctl` for its on face but says it needs nothing

- **Status:** fixed in `bf67a74` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `feat/media-dial` (`072d31c`).
- **Where:** `presets::PRESETS` (`needs: None`); README features list.
- **Fix:** `needs: Some("playerctl")`, or say so in the README.

#### 90. A preset applied in place keeps the key's live value and busy settings

- **Status:** fixed in `9387a41` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `fix/review-2` (`b1cdf93`, presets apply in place); `live` is PR 9's; PR 14's Play/Pause preset needs the same clearing (folded there too).
- **What goes wrong:** Mic mute on a CPU key keeps showing CPU; a leftover
  `busy_while_running` or `[key.busy]` stays.
- **Fix:** the shared preset helper also sets `live = None`, `busy = None`,
  `busy_while_running = false`; extend `presets_keep_the_key_and_replace_its_icon`.

#### 91. `ipc::commands` is dead code that drops bad lines silently

- **Status:** fixed in `7e701e7` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `feat/ctl` (`dbe1ae3`). **Fix:** delete it.

#### 92. Docs: "Eleven action types", no `media` row, `BusyFace::bar` says "last"

- **Status:** fixed in `7dba580` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `feat/media-dial` (`072d31c`, README); `fix/review-2` (`b1cdf93`, the
  `BusyFace::bar` doc comment).
- **Fix:** "Twelve action types", a `media` row in the action table, "the first `NN%`".

#### 93. Outside the daemon, a multi's app volume step can land after the next step

- **Status:** fixed in `6bc2be3` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core) *(read)*.
- **Branch:** `feat/multi-actions` (`5fb32bf`).
- **Where:** `action::run_multi_blocking` (an `AppVolume` step goes through `run`, which
  spawns a thread). Its doc says volume steps wait.
- **Fix:** split out `apply_app_volume` and call it inline from the multi runner.

#### 94. A media dial's panel and its actions can follow different players

- **Status:** fixed in `324c99d` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core probes) *(read)*.
- **Branch:** `feat/media-dial` (`839d72d`); the strip long press (PR 16) adds `Source::LongTap` to the dial sources that fall back.
- **Where:** `DialConfig::media_player` (the panel) vs `Daemon::dispatch` (each action's
  own `player`).
- **What goes wrong:** turns on `player = "spotify"`, press with no player: the panel shows
  Spotify, the press pauses whatever Omarchy picks.
- **Fix:** a dial's blank `player` uses the dial's `media_player()`.

#### 95. `player::read` drops good lines when `playerctl -a` exits non-zero

- **Status:** fixed in `05c5897` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core probes) *(plausible)*.
- **Branch:** `feat/media-dial` (`839d72d`).
- **What goes wrong:** one player without metadata can make the whole call exit 1:
  "Nothing playing" while Spotify plays.
- **Fix:** always parse stdout; use the status only when nothing parsed.

#### 96. Flatpak app icons are never found (the app id is lowercased first)

- **Status:** fixed in `c3da136` on `fix/review-3`, not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core probes) *(plausible: the Mac's
  filesystem ignores case)*.
- **Branch:** `feat/app-volume` (`34c6918`).
- **Where:** `audio::parse_pactl_streams`, `audio::app_level`.
- **Fix:** keep the original-case id for icon lookups; match on the lowercased one.

#### 97. A clock with `%n` on a dial draws the newline as a glyph

- **Status:** fixed in `9c57e9a` on `fix/review-3`, verified on the machine 2026-10-17 ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (core render) *(plausible)*.
- **Branch:** `feat/live-keys` (`056fd8a`).
- **Where:** `Renderer::render_strip_panel`, the live arm (only `render_body` handles `\n`).
- **Fix:** replace `\n` with a space there (or draw the first line).

#### 98. The page switcher's folder paths go stale

- **Status:** fixed in `e13123f` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI app) *(read)*.
- **Branch:** `feat/folders` (`b05fb0e`).
- **Where:** `app.rs` `sync_page_list`; `editor.rs` `new_folder`, the `on_new_page`
  closures (`add_page` syncs before the key points at the page).
- **What goes wrong:** a new folder from key 3 on "Music" shows as "Key 3", not
  "Music › Key 3", until something else syncs.
- **Fix:** sync after a key edit that adds or removes an `Action::Page`.

#### 99. The Follow apps dialog edits while config.toml is broken, and toasts behind itself

- **Status:** fixed in `5d99fd8` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI app) *(read)*.
- **Branch:** `feat/app-pages` (`88f5a7e`).
- **Where:** `app.rs` `App::edit_page_apps`; `pages.rs` `AppRows`.
- **Fix:** return early when `locked`; toast on the dialog.

#### 100. Picking a `uwsm app --` launched app stores "uwsm"

- **Status:** fixed in `29bb822` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI app) *(plausible)*.
- **Branch:** `feat/app-volume` (`7ba769e`).
- **Where:** `apps.rs` `audio_key` (only `uwsm-app` is in the launcher list).
- **Fix:** add `"uwsm"`.

#### 101. Dial presets keep a long press that then decides the panel

- **Status:** fixed in `57e0f62` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI editor) *(read)*.
- **Branch:** `feat/strip-long-press` (`13597e8`).
- **Where:** `editor.rs` `dial_presets_button`; `DialConfig::level_source` walks the long
  press too.
- **What goes wrong:** a dial with long press = Media gets the Volume preset: the panel
  shows the player, not the volume.
- **Fix:** each preset resets the long press (or at least one that carries a level).

#### 102. Switching "Two faces" off erases a busy-while-running key's Busy look

- **Status:** fixed in `26364d1` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI editor) *(read)*.
- **Branch:** `fix/review-2` (`8b33817` gave such keys a Busy tab; the `k.busy = None` is
  older).
- **Where:** `editor.rs` `build_key`, the `two` handler.
- **Fix:** `k.on = None; if !k.busy_while_running { k.busy = None; }`.

#### 103. The media Seconds spin can't cross zero

- **Status:** fixed in `22e8d25` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI editor) *(read)*.
- **Branch:** `feat/media-dial` (`072d31c`).
- **Where:** `editor.rs` `media_rows`, the seek spin (`0` resets to the slot's default).
- **Fix:** on 0, skip to ±1 in the direction of travel (remember the last value).

#### 104. A key's own action rows edit through the current page

- **Status:** fixed in `746e326` on `fix/review-3` (GUI: type-checked, not run), not yet verified on the machine ([VERIFY.md](VERIFY.md)). **Found:** 2026-10-17, review 3 (GUI editor) *(plausible; older than
  `55d0d62`, but the new App volume, Media and Volume spins use it)*.
- **Branch:** older than `55d0d62` (the key slot's `get`/`set` since groundwork); the fix
  needs PR 12's `ActionTarget` handling, so it is folded into PR 12.
- **Where:** `editor.rs` `build_key`, `slot.get`/`slot.set` and `on_slot`'s
  (`a.key(i)`/`a.edit(i, …)`).
- **What goes wrong:** a spin's focus-out commit after the deck changed page writes key `i`
  of the new page.
- **Fix:** build the key slots on `ActionTarget::Key(app.key_ref(i))` through
  `action_at`/`set_action_at`, as the dial and step slots are.

#### 105. Tracked jobs fail everywhere the user bus is down (CI red from `fix/review-2` on)

- **Status:** fixed in `b7a5bc3` on `feat/toggle-keys`, cherry-picked to `pr/08-toggle-keys`
  (`9352712`); the `feat/*`/`fix/*` chain rebased on it (`fix/review-2` … `fix/review-3`,
  `daily`). `pr/09` … `pr/16` were not (found 2026-10-18); the `up/*` series has it. CI on `fix/review-2` is green again (run 39060927550).
- **Found:** 2026-10-17, machine pass — the first CI run that includes `run_tracked`
  (run 39060500703): `action::tests::run_tracked_streams_lines` fails in CI, passes on the
  machine. A no-bus run with `XDG_RUNTIME_DIR` pointed at an empty dir reproduces it.
- **Branch:** `feat/toggle-keys` (PR 8 introduced `run_tracked`); affects every branch
  from `fix/review-2` up, and `pr/08` … `pr/16`.
- **Where:** `action.rs` `run_tracked`: `HAS_SCOPE.get_or_init(|| which("systemd-run"))`.
- **What goes wrong:** a CI runner (and any session without a user bus) has `systemd-run`
  on PATH but no bus, so `systemd-run --user --scope` exits non-zero and every tracked
  job — and the test — reports failure. U9's "fails silently when there's no user bus",
  now with a visible failure.
- **Fix:** `user_bus()` — the scope is taken only when `$XDG_RUNTIME_DIR/bus` exists;
  otherwise the job runs directly, as the code already did when `systemd-run` was
  missing. `run_tracked_streams_lines` now exercises the direct path in CI and the
  scope path on a machine with a bus.

#### 106. The deck preview frame is wider than the deck: blank after the 4th key and 4th dial

- **Status:** fixed in `935fbf7` on `feat/folders`, cherry-picked to `pr/13-folders`
  (`8aa181e`); the chain rebased (`feat/media-dial` … `fix/review-3`, `pr/14` … `pr/16`,
  `daily`).
- **Found:** 2026-10-17, machine pass — first real look at the configurator. The deck
  frame (the dark body around the keys) stretched well past the 4th column: blank space
  after the 4th key in each row and after the 4th dial, and the strip stretched wider
  than the keys. (The "5th dial" some might swear they see is the wallpaper through the
  translucent window; the device reports 4 dials.)
- **Branch:** `feat/folders` (PR 13's GUI commit `fa1a6b7` lengthened the hint line under
  the deck, which is what made it visible).
- **Where:** `app.rs` `App::new`, the `deck_area` box and its `frame` child.
- **What goes wrong:** `deck_area` has `halign(Center)`, so it takes the natural width of
  its widest child — the hint label, one unwrapped line. The `frame` below it has the
  default `halign(Fill)`, so it stretches to the hint's width; the 4-column key grid and
  dial grid inside keep their natural 426 px and sit left, leaving the blank. Before
  `fa1a6b7` the hint was only a little wider than the deck, so the same code looked fine
  — which is why the installed (older) GUI looked snug in the same window.
- **Fix:** `frame.set_halign(Center)` — the frame shrink-wraps to the deck (478 px) and
  centers in the area; the grid, strip and dials all line up at the grid's width.

#### 107. Toggle key preview keeps the On face after re-selecting the key (tab resets to Off)

- **Status:** fixed 2026-10-19 on `up/07-toggle-keys`, in "Configurator: a Toggle group makes any key a toggle": a changed selection redraws the old and the new key's tiles; not yet verified on the machine. Found 2026-10-18, GUI check pass (item 3, [GUI-CHECKLIST.md](GUI-CHECKLIST.md) fix/review-2 Task 7).
- **Branch:** `up/07-toggle-keys` (PR 8's toggle key editor).
- **Where:** GUI, the key preview vs the `FaceTab` reset when the selected key changes (`editor.rs` / `app.rs` selection path).
- **Repro:** select a toggle key (e.g. a key with an `[key.on]` face); the preview shows the off face and the Off tab is active. Click the **On** tab — the preview shows the on face. Select a different key, then select the toggle key again.
- **What goes wrong:** the active tab correctly resets to **Off**, but the key **preview** still shows the **on** face (stale) — it isn't re-rendered to match the reset tab / the key's real (off) state. The physical deck shows off (correct); only the GUI preview is wrong.
- **Fix (TBD):** when the selected key changes, re-render the preview for the (reset) active face, not just the tab.

#### 108. Clicking a toggle key's On/Off face tab scrolls the editor to the top

- **Status:** fixed 2026-10-19 on `up/07-toggle-keys`, in "Configurator: a Toggle group makes any key a toggle": rebuilding the editor for the same selection keeps its scroll; not yet verified on the machine. Found 2026-10-18, GUI check pass (item 3, fix/review-2 Task 7).
- **Branch:** `up/07-toggle-keys` (PR 8's toggle key editor).
- **Where:** GUI, the `FaceTab` switch handler rebuilding the key editor (`editor.rs`); the
  scrollable editor area resets its position when the face's rows are swapped.
- **What goes wrong:** with a toggle key selected, clicking the On or Off tab makes the
  editor scroll to the top. Jarring while working down the key's rows; the tab switch
  shouldn't reset scroll.
- **Fix (TBD):** preserve the scroll offset across a face-tab swap (or swap only the face
  rows without rebuilding the whole scrollable).

#### 109. State-command entry: the edit and check icons sit left of the hint text

- **Status:** fixed 2026-10-19 on `up/07-toggle-keys` ("Configurator: a Toggle group makes any key a toggle" and "Use toggle keys in the starter layout; document them"): the hint is a hint row under the entry, so the edit and ✓ icons sit at the row's end; not yet verified on the machine. Found 2026-10-18, GUI check pass (item 3, fix/review-2 Task 7).
- **Branch:** `up/07-toggle-keys` (PR 8's toggle key editor, the state-command entry).
- **Where:** GUI, the state-command entry row (`editor.rs`); the edit and ✓ (apply) buttons and
  the placeholder/hint text.
- **What goes wrong:** the entry has a hint (placeholder) text, but the edit and ✓ icons are
  drawn to the **left** of the hint. They should be at the **end** (right) of the row, with the
  hint text to their left.
- **Fix (TBD):** put the buttons at the trailing edge of the entry row.
