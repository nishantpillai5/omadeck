# Bugs

Personal working notes, next to [PLAN.md](PLAN.md). Bugs found in the code on `daily`,
with where they are, what goes wrong, and which branch should fix them. Bugs 1–21 are
closed (see the bug pass under Rebase outcomes), except that 18's fix didn't work (see
29, now closed). The 2026-10-17 code review added 22–49: 22–43 are fixed on
`fix/review` (review-fixes.md, 2026-10-17); 44–49 go to `feat/groundwork`
(groundwork.md), because their proper fix is that branch's structure.

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

- **Status:** fixed in `6622480` (core) and `25fb9a1` (GUI). **Found:** 2026-10-17, review follow-up (checked in the code).
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

- **Status:** fixed in `6622480`. **Found:** 2026-10-17, code review.
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

- **Status:** fixed in `c71b21c`. **Found:** 2026-10-17, code review.
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

- **Status:** open, to confirm on the machine first (see review-fixes.md). **Found:**
  2026-10-17, code review.
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

- **Status:** fixed in `485a6ca`. **Found:** 2026-10-17, code review.
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

- **Status:** fixed in `87e08c7` + `0badd8e` (brightness in `Runtime`, which outlives the deck). **Found:** 2026-10-17, code review.
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

- **Status:** fixed in `edc4bdb` (the read happens after the change on the same thread) and the in-flight follow-up (the bar shows confirmed + in-flight, so a late read-back can't pull it back — see the note at the end of groundwork.md). **Found:** 2026-10-17, code review.
- **Where:** `main.rs` `after_volume` + `AUDIO_REFRESH_DELAY` + the worker's `Refresh`.
- **What goes wrong:** when the volume command takes longer than 150 ms (Omarchy 4's
  script, Omarchy 3's `hyprctl`), or a 1 s poll is already running, the read returns the
  old level and, since bug 9's fix dropped `last`, sends it: the bar shows 55, snaps to 50,
  and the next guess builds on 50.
- **Why groundwork:** the read happens after the change on the same thread (D5).

#### 48. A hung `pactl` or `wpctl` freezes the bars for good

- **Status:** fixed in `66373ff` + `edc4bdb` (`proc::output_timeout` on the watcher's reads and changes). **Found:** 2026-10-17, code review.
- **Where:** `audio.rs` `capture` (`Command::output()`, no timeout).
- **What goes wrong:** while PipeWire restarts, a read can hang; the worker stops, the
  bars freeze, and a `Refresh` per turn piles up in the unbounded channel.
- **Why groundwork:** `proc::output_timeout` on the watcher (D5).

#### 49. A slow configurator stalls the deck loop and is then dropped

- **Status:** fixed in `75738f8` (per-client bounded queue + writer thread; the loop never writes to a socket). Verified with a frozen GUI: the deck stayed responsive. **Found:** 2026-10-17, code review.
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
