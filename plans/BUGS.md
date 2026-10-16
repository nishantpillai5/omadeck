# Bugs

Personal working notes, next to [PLAN.md](PLAN.md). Bugs found in the code on `daily`,
with where they are, what goes wrong, and which branch should fix them. All 21 recorded
bugs are closed (see the bug pass under Rebase outcomes); add what a fresh review finds.

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
- **18** — fixed: volume/brightness changes mark panels dirty; one `paint_dial` per loop
  pass (the `paint_dial` comment is updated to match).
- **21** — `a` and `e` were already fixed by the rebase (Brightness test disabled;
  `reload_from_disk` calls `update_style`); `b`, `c`, `d` fixed (app pick no longer renames
  the dial; `default_for` gives a new volume the direction's sign; a smaller replugged deck
  clears a stale selection).

All 21 bugs are now closed. `daily` is `fix/bugs`.

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
