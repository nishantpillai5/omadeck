# Rebase onto upstream's pages

Part of [PLAN.md](PLAN.md). Do this **before PR 7**: every later plan builds on the
page model, so it has to settle first.

**Outcome:** `daily` runs upstream's pages and folders (`thewizster/omadeck`, branch
`pages-and-folders`) instead of our PR 5. Our Stream Deck + work (PRs 1–4, 6) sits on
top of it, with the Plus-only page parts (strip flick, next/previous, dials that stay
the same on every page) ported to upstream's model as a new, small PR 5. `ba0eced` is
retired.

## Why

Upstream wrote pages at the same time we did, and we didn't see it:

| When (UTC) | What |
|---|---|
| 2026-10-04 | Upstream `master` at `3bb7eb4`; the roadmap still lists "Pages and folders" as open. |
| 2026-10-15 04:11 | Upstream pushes `pages-and-folders` (`d4e458f`). No PR, no announcement. |
| 2026-10-15 22:26 | Our PR 5 pages, `ba0eced`. |
| 2026-10-16 03:28 | Upstream pushes a fix commit, `fb9052f`. |

We had no `upstream` remote (the setup step was never done), so the branch never
showed up. The review in [BUGS.md](BUGS.md) ("Pages: our `ba0eced` vs upstream")
compares the two: upstream's version is better on everything except the Plus. It has
named pages, folders with Back, a breadcrumb nav stack, a GUI→daemon command channel
and async-safe edits. The two can't coexist:

- Both use `type = "page"` with different fields.
- A trial merge leaves `ipc.rs` with two `PageChanged` variants.
- Nine files conflict.

Upstream will presumably merge its own version, so ours would be dead weight in every
PR.

## What upstream has (on `fb9052f`)

- **Config:** top-level `[[key]]` is the unnamed Home page. Other pages are
  `[[page]] name = "…"` with `[[page.key]]`. A key list is a `Keys` newtype
  (`get`, `get_mut`, `clear`, `swap`). `Config::page(Option<&str>)`, `page_mut`,
  `key(page, index)`, `page_names`, `page_name_problem`, `unique_page_name`,
  `links_to`, `rename_page`, `remove_page`. A folder is just a page with a Back key.
- **Actions:** `Page { page: String }` and `Back`. `KINDS` index 6 is "Open page" and 7
  is "Go back". `is_navigation()` marks both, and `action::run` refuses them.
- **Daemon:** `nav.rs` `Nav { current, history }` with `go`, `back` and
  `retain_existing`, bounded so cycles cut back. It lives in the run loop, so it
  survives a replug. `wanted_page` is for a page the GUI asked for that hasn't been
  saved yet. CLI `press KEY [PAGE]` and `snapshot FILE [PAGE]`.
- **IPC:** `Event::PageChanged { page: Option<String> }`, replayed to new clients.
  `KeyDown` carries `page`. `ipc::connect() -> (Sender, events)` with
  `Command::ShowPage`, so the deck shows the page being edited.
- **GUI:** a page dropdown with New, Rename and Delete. "New page" from an Open-page
  key builds a folder with a Back key. `KeyRef { page, index }` + `edit_ref` so that
  async picker results land on the right page.

It also has bugs; see BUGS.md U1–U9.

## Base

- **Default:** rebase onto `upstream/pages-and-folders` now, and onto `upstream/master`
  once it lands there. If upstream changes the branch first, rebase again; `rerere`
  helps.
- Before starting, `git fetch upstream` and check `fb9052f` is still the branch tip.
  If upstream has moved on (or merged it), redo the "What upstream has" list.

## Branches

Keep the old tips first, so nothing is lost:

```bash
for b in plus-input dial-actions touch-strip dial-volume pages brightness; do
  git tag "old/feat-$b" "origin/feat/$b"
done
git tag old/daily origin/daily
```

Then rebuild the `feat/*` branches in place (they're only on my fork, with no PRs open,
so force-pushing them is fine). The new stack, each branch based on the one before:

| PR | Branch | Comes from |
|----|--------|------------|
| 1 | `feat/plus-input` | `ffc6c54` |
| 2 | `feat/dial-actions` | `e56a25c` |
| 3 | `feat/touch-strip` | `1676c66` |
| 4 | `feat/dial-volume` | `4c1ec00` + `da2b426` |
| 5 | `feat/plus-pages` | **new**: flick, next/previous (replaces `feat/pages`) |
| 6 | `feat/brightness` | `4860e9f`, minus its PR 5 parts, plus bug fixes |

The bottom of the stack is `upstream/pages-and-folders`, not `upstream/master`. Every
PR description then says "depends on upstream's pages-and-folders".

`daily` = the base + all six, as before. Delete `origin/feat/pages` once
`feat/plus-pages` works.

## Steps

### 1. PRs 1–4: cherry-pick and resolve

`git merge-tree` against `upstream/pages-and-folders` shows the same nine files
conflicting even without our pages commit:

- `config.rs`
- `editor.rs`
- `omadeckd/main.rs`
- `app.rs`
- `examples/config.toml`
- `README.md`
- `CHANGELOG.md`
- `action.rs`
- `lib.rs`

Most of them are union conflicts: both sides added fields, variants, re-exports or
match arms. Keep both.

- **`KINDS`:** Nothing 0 … Hyprland 5, Open page 6, Go back 7, **Volume 8**.
  Update `kind_index`, `empty_of_kind`, and the tests that assert `KINDS.len() - 1`.
- **`Config`:** `keys: Keys`, `pages: Vec<Page>`, `dials: Vec<DialConfig>`. Dials stay
  top-level, so they're the same on every page with no extra code (as in the Elgato
  app; see [pr5-pages.md](pr5-pages.md)).
- **`is_navigation()`** stays upstream's. Volume is not navigation.
- **Daemon (`main.rs`):** keep upstream's `nav`, `hub.commands()`, `FileStamp` and
  `paint(…, page)`. Add our input handling, dial and strip painting, and the audio
  worker. Dial and strip painting take **no** page. Upstream's
  `paint`/`write_key`/`paint_all` already take `page: Option<&str>`, so ours call them
  with `nav.current()`.
- **GUI (`editor.rs`):** the real work. We split `build` into `build_key`/`build_dial`
  and pulled out `action_editor`, and upstream threaded `KeyRef`/`edit_ref` through
  the same functions.
  - Take upstream's file as the base and re-apply our split on top, not the reverse.
  - Give the dial editor the same async safety, either a `DialRef` or one
    `Target { Key(KeyRef), Dial(u8) }`. A dial has no page, so a `Dial(u8)` target is
    always safe.
  - Fix BUGS.md 4 (the five `if let … config.borrow()` panics) while touching these
    rows.
- **GUI (`app.rs`):** use upstream's `ipc::connect()` (Sender + events) and handle our
  `Dial*`/`Strip*` events next to its own.
- Each cherry-pick: `cargo fmt`, `cargo clippy --all-targets -- -D warnings`,
  `cargo test`. Then commit with the original message (amend the body if the
  resolution changed something worth saying).

### 2. PR 5 `feat/plus-pages`: the Plus parts of pages, on upstream's model

What upstream lacks: a relative next/previous (a flick and a dial turn both need
one), and a page order to step through.

- **Action.** A new kind, `PageStep { change: i8 }` with `type = "page_step"`, at
  **`KINDS` index 9** ("Next/previous page"). Its summary is "Next page", "Previous
  page", or "Page +N".
  - Its own tag, so upstream's `type = "page"` is untouched.
  - Upstream's `Page { page }` already covers jumping to a page, so our `index` field
    goes away.
  - Add it to `is_navigation()`.
- **Page order.** `[Home] + pages`, in file order, **skipping folders**. Otherwise a
  flick on Home lands inside a folder.
  - Upstream has no way to tell a folder from a page. Add
    `#[serde(default, skip_serializing_if = "Not::not")] folder: bool` to `Page`, set
    by upstream's "New page from an Open-page key" flow.
  - A hand-written page without it counts as a normal page.
  - See Open decisions.
- **Core.** `Config::step_page(current: Option<&str>, change: i32) -> Option<Option<String>>`
  is a pure function adapted from our `switch_page`:
  - `None` with fewer than two steppable pages;
  - wraps both ways;
  - multi-step works;
  - from inside a folder it steps from the folder's parent (see Open decisions).
  - Keep `switch_page`'s tests, rewritten for names.
- **Nav.** `Nav::switch(page)` **replaces** the current page and clears history,
  instead of pushing like `go`. A flick or step is a sideways move, so Back mustn't
  undo it. Test it next to upstream's `folders`/`loops_stay_bounded`.
- **Daemon:**
  - A flick (`to.0 < from.0` means next, as now) and a `PageStep` key or dial turn
    (`change × steps`) call `step_page`, then `Nav::switch`, then upstream's
    `show_page`. `show_page` already repaints and publishes `PageChanged`.
  - Add a minimum horizontal distance to the flick (BUGS.md 19).
  - Route every input path (key, turn, press, tap) through **one** dispatch for
    deck-owned actions. That fixes BUGS.md 6 (dial press can't page) and is the
    `is_deck_owned()` that PR 12 planned.
- **GUI:** a "Next/previous page" row in the action editor (change ±1, or a spin) for
  keys and dial turns. "Try it now" is disabled for every `is_navigation()` action, in
  the dial editor too (BUGS.md 21).
- **Dropped from `ba0eced`:** `KeyConfig.page`, `MAX_PAGES`, `key_on`/`key_mut_on`/
  `clear_key_on`/`swap_keys_on`, `page_count`, page chips, the Page spin in the key
  editor, and `Event::PageChanged { page: u8 }`. Upstream's dropdown and
  `PageChanged { page: Option<String> }` replace them. No migration: `page = N` configs
  only ever existed on my `daily`. Convert my own config by hand.
- **Docs:** CHANGELOG, a README line (flick, next/previous, dials stay put), and a
  `page_step` example in `examples/config.toml`.
- **Size:** about 250–300 lines, half of them tests.

### 3. PR 6 `feat/brightness`: rebase and fix

- Cherry-pick `4860e9f`. Brightness goes to **`KINDS` index 10**.
- Drop its PR 5 parts: the chip refresh and the `page_count` uses.
- Resolve its `paint_all` changes against upstream's page-aware signature.
- Fix these while here, since they're in this commit's code:
  - **BUGS.md 5:** the daemon rewrites the whole config on every brightness change.
    Move the live brightness into a state file (`$XDG_STATE_HOME/omadeck/state.toml`),
    written debounced from a thread. `Config.brightness` stays the hand-set start
    value. That also removes the daemon's self-triggered reload.
  - **BUGS.md 7:** the settings slider doesn't save. It goes away once the daemon
    stops writing `config.toml`; delete the "adopt the disk brightness" merge in
    `save_now`.
  - **BUGS.md 13:** the editor is rebuilt on every reload. It's mostly gone with 5;
    also skip `rebuild_editor` when nothing visible changed.
- Upstream's GUI already compares the file on disk before saving ("theirs wins"), so
  the GUI side of the race is upstream's code, not ours.

### 4. Bugs: which branch fixes what

| BUGS.md | Fixed by |
|---|---|
| 1, 2, 3, 8, 12, 20 | Gone with `ba0eced`; upstream's code replaces it (check upstream doesn't have the same bug). |
| 4 | Step 1, `feat/dial-actions` (where `action_editor` came in) |
| 5, 7, 13 | Step 3, `feat/brightness` |
| 6, 19, 21 (dial Try it now) | Step 2, `feat/plus-pages` |
| 9, 10, 15, 16, 17, 18 | `feat/dial-volume` / `feat/touch-strip`, during step 1 or right after |
| 11, 14, 21 (rest) | GUI clean-up in the branch that introduced each |
| U1–U9 | Upstream's. Don't fix in our PRs; collect for the message to the author (see Open decisions). |

### 5. Rebuild `daily` and check on the hardware

```bash
git checkout -B daily upstream/pages-and-folders
git merge --no-edit feat/brightness   # the top of the stack carries the rest
```

Hardware checks (Stream Deck +, `0fd9:0084`):

- [ ] Keys, dials, strip panels and volume/mic bars work as on `old/daily`.
- [ ] Upstream pages: an Open-page key, Back, a folder made from the GUI, and a
      page rename and delete with links fixed up.
- [ ] Flick left = next, right = previous; it wraps; it skips folders; it does nothing
      with one page. A short or vertical swipe doesn't page.
- [ ] Inside a folder, a flick does what Open decisions settled.
- [ ] A dial `page_step` turn pages; a dial press with Next page pages (BUGS.md 6).
- [ ] Dials and strip panels are unchanged by any page change; a volume dial keeps
      working on every page.
- [ ] Brightness: turning the dial doesn't touch `config.toml` (comments survive),
      survives a restart (state file), and the settings slider saves.
- [ ] The configurator follows the deck between pages and folders, and the dial
      editor never panics (BUGS.md 4 rows).
- [ ] My own config, converted by hand from `page = N`, loads and works.

## Building and testing

- **Mac (this laptop):** core tests and a daemon `cargo check` work. The sandbox has
  no crate cache, so fetch into a scratch `CARGO_HOME` first (`cargo fetch` needs
  `index.crates.io` and `static.crates.io`), then run `--offline`. The GUI doesn't
  build here (no GTK4/libadwaita/pkg-config). Either
  `brew install gtk4 libadwaita pkgconf`, or do the `editor.rs`/`app.rs` resolution
  on the Omarchy machine.
- **Omarchy machine:** the full `cargo clippy` + `cargo test` and the hardware checks.

## Effect on the other plans

Update each of these when its turn comes; they were written against `ba0eced`'s
`(page: u8)` model.

- **PR 13 folders:** mostly done upstream (a folder = a page with a Back key, a nav
  stack, Back). What's left of Elgato's behaviour:
  - Back fixed top-left;
  - hold Back to go Home;
  - a folder face;
  - Auto Exit.

  That's a small follow-up PR on upstream's model, or nothing. Rewrite pr13 then, and
  drop its `folder` string field and `key_at` API.
- **PR 11 ctl:** upstream already has the client→daemon half (`Command`, `Sender`,
  `hub.commands()`). Build `omadeckd page next`, `brightness 30`, etc. as new
  `Command` variants on it instead of a second channel.
- **PR 12 multi-actions:** `is_deck_owned()` is the one dispatch from step 2. Keep
  `is_navigation()` as upstream's name, or rename it in our PR only if Brightness
  needs it too.
- **PR 15 app pages:** the target is a page *name* (`Option<String>`), not an index.
  It uses `Nav::switch`, not `go`, so Back doesn't walk an app-switch trail.
- **`KINDS` order** (replaces PLAN.md's list):

  | Index | Kind | From |
  |---|---|---|
  | 6 | Open page | upstream |
  | 7 | Go back | upstream |
  | 8 | Volume | PR 4 |
  | 9 | Page step | PR 5 |
  | 10 | Brightness | PR 6 |
  | 11 | App volume | PR 10 |
  | 12 | Multi action | PR 12 |
  | 13 | Delay | PR 12 |
  | 14 | Media | PR 14 |

  Folder (PR 13) is no longer a kind: upstream's Open page covers it.

## Decisions to explain

- **Upstream's pages over ours.** Theirs came first by a day, does folders, and is the
  author's own design. We contribute only what's Plus-specific.
- **`page_step` as its own action** rather than a relative field on upstream's
  `Page`. It leaves upstream's TOML and `Page { page }` exactly as they are, and a
  step has no target page.
- **A `folder` flag on `[[page]]`.** It's needed so a flick doesn't land inside a
  folder. It's off by default, so upstream's existing configs are unchanged.
- **`Nav::switch` for sideways moves.** In the Elgato app, swiping pages isn't
  something Back undoes; only leaving a folder is.
- **Brightness moves to a state file.** The daemon never writes the user's
  hand-edited `config.toml`, as on upstream master.

## Open decisions (settle when starting)

The plan picks a default for each; confirm or change it first.

- Base on `upstream/pages-and-folders` now (planned), or wait until upstream merges it
  to `master`?
- Rebuild `feat/*` in place after tagging `old/*` (planned), or new branch names?
- How to tell folders from pages for stepping: a `folder` flag on `[[page]]` (planned),
  or infer "a page that has a Back key"? Inferring needs no config change, but it's
  implicit and breaks if someone puts Back on a normal page.
- A flick inside a folder: step among the folder's parent's siblings, leaving the
  folder (planned), or do nothing? Elgato's behaviour is unconfirmed (see pr13).
- Brightness in a state file (planned), or keep it in `config.toml`, debounced and
  written off-loop?
- Upstream's bugs U1–U9: report them to the author with the first message (planned),
  or offer fixes as separate small PRs?
- Contact the author now, since we now know they're actively working on overlapping
  features, instead of waiting until everything works (PLAN.md's "Order of work")?
