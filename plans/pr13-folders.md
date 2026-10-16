# PR 13: `feat/folders`: folders

Part of [PLAN.md](PLAN.md). Finishes the roadmap item "Pages and folders" (PR 5 did
pages). Based on `feat/multi-actions` (PR 12); needs PR 5's pages. It touches the
state watcher (PR 8/9) and ctl (PR 11) only through the small hooks listed under
[Interactions](#interactions-with-other-prs).

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] **Mostly obsolete.** Upstream's pages-and-folders already has folders: a page with a Back key at index 0, `Nav::go`/`back`/`retain_existing`, `new_folder()` in the GUI. `KeyConfig.folder`, `key_at`, `Location`, `Action::Folder` and the breadcrumb all go.
- [ ] Rewrite as a small plan (~150 lines) on upstream's `Nav` and the rebase's `Page.folder` flag, covering what's left of Elgato's behaviour: Back fixed and not editable at top-left; hold Back for Home (500 ms from key-down, e.g. `Deck.back_down: Option<Instant>` checked in the loop, `Nav::home()`); a default folder face; optionally Auto Exit. Or drop it.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.

**Outcome:** a `folder` action opens a sub-page of keys. Inside a folder the top-left
key is a fixed **Back** key: a press goes up one level, and holding it returns to the
front page, as in the Elgato app. Folders can nest and can have pages of their own. The
dials and the strip stay as they are. The configurator opens a folder with a
double-click and shows a breadcrumb next to the page chips. Existing configs load and
save byte-identical, because no `folder` field is written for keys outside folders.

## How the Elgato app does it (researched 2026-10-16)

- **Creating:** drag the *Create Folder* action (called *Folder* in the general guide)
  onto a key, open it, then drag actions in "just as you would in your main layout".
  ([folders guide](https://www.elgato.com/explorer/products/stream-deck/how-to-use-folders-stream-deck/),
  [Neo quick start](https://www.elgato.com/explorer/products/neo/elgato-stream-deck-neo-quick-start-guide/))
- **Back key:** it is placed **top-left** and the app gives no way to move it. The
  evidence is indirect: a community tool exists only to "move the *Back* action from
  the top-left in a Stream Deck Folder" by editing profile files
  ([BarRaider folder editor](https://github.com/BarRaider/streamdeck-foldereditor)).
  So: fixed, top-left, not editable. That the Elgato app has no setting for it at all
  is unconfirmed.
- **Leaving a folder:** Back goes up one level at a time, and **holding** the Exit/Back
  key goes straight to the main layout (folders guide).
- **Nesting:** "Folders can live inside other folders"; drag a Folder action into an
  existing folder (folders guide). No depth limit is documented (unconfirmed).
- **Auto Exit** (Stream Deck 6.5, Dec 2023): an option to leave a folder after an
  action runs, after a delay from immediate up to 60 s, set per folder or per action.
  It is **off by default**, so normally running an action inside a folder keeps the
  folder open.
  ([6.5 release notes](https://help.elgato.com/hc/en-us/articles/22500155667469-Elgato-Stream-Deck-6-5-Release-Notes),
  folders guide)
- **Folders and pages:** in 6.6 a folder can be copied and pasted as a page, and a page
  pasted onto the canvas becomes a folder; folders can also be pinned across pages
  ([6.6 release notes](https://help.elgato.com/hc/en-us/articles/26005306818701-Elgato-Stream-Deck-6-6-Release-Notes)).
  That a page converts into a folder suggests a folder holds about one page of keys.
  Whether a folder can have **multiple pages** of its own is **unconfirmed**; I could
  not open the release-note pages (HTTP 403) or find it in the guides.
- **Folder key face:** a default folder image that you can replace with your own
  image or design (folders guide). The exact default artwork is unconfirmed.
- **Stream Deck + dials and strip inside a folder:** nothing found. PR 5's finding is
  that dial settings belong to the device and not the page. Whether an Elgato folder on
  the + carries its own dials is **unconfirmed**. What a strip flick does inside a
  folder is **unconfirmed** too.

## Config

A key belongs to a folder through a new field on `KeyConfig`. The folder's name is
just a string, so there are no `[[folder]]` tables: the key that opens the folder
carries its label and icon, as in Elgato, where the folder key is the folder's face.

```rust
pub struct KeyConfig {
    pub index: u8,
    /// The folder this key is in; empty = the deck's own pages. Keys in a folder start
    /// at index 1: index 0 (top-left) is the folder's fixed Back key.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    pub page: u8,          // unchanged; inside a folder, the folder's own page
    …
}
```

- Folder names form one flat namespace. Nesting comes from where the opening key sits:
  a `folder = "media"` key whose action opens `"spotify"` makes Spotify a sub-folder of
  Media. Several keys may open the same folder.
- New page-aware methods take a folder. The existing ones become root-only wrappers, so
  PR 5's callers keep compiling:
  - `key_at(folder, page, index)`, `key_mut_at(…)` (creates the key there),
    `clear_key_at`, `swap_keys_at`, `keys_at(folder, page) -> impl Iterator`.
  - `key_on(p, i)` = `key_at("", p, i)`, and so on.
  - `page_count_in(folder)`. **Fix:** `page_count()` becomes `page_count_in("")`.
    Today it counts every key, so a folder key with `page = 3` would add root pages.
  - `key(i)` (used by the CLI) only looks at root keys.
  - `folder_exists(name)`: some key has that `folder`, or some action opens it (an
    empty folder still exists).
  - `rename_folder(old, new) -> Result<()>`: renames the keys' `folder` fields and
    every `Folder` action that targets it. Refuses a name that is already in use.
  - `delete_folder(name)`: removes its keys and, recursively, the folders only it
    contains.
  - `new_folder_name()`: `"folder-1"`, `"folder-2"`, … picking the first unused one.
  - `move_key(from: (&str, u8, u8), to_folder, to_page, key_count) -> Option<u8>`:
    moves the key to the first free index there (skipping 0 in a folder). `None` when
    the destination is full.
  - `folder_warnings() -> Vec<String>` for the daemon log: a key at index 0 in a
    folder is hidden under Back, and a folder that no key opens is unreachable.
- `save_to` sorts by `(folder, page, index)`. Root keys have the empty folder, which
  sorts first, so old files keep their order. `is_blank` ignores `folder` (as it does
  `page`).

```toml
[[key]]
index = 4
label = "Media"
[key.action]
type = "folder"
name = "media"

[[key]]
index = 1
folder = "media"            # inside the Media folder; index 0 is its Back key
label = "Play/Pause"
[key.action]
type = "app"
command = "playerctl play-pause"
```

## Action

```rust
/// Open a folder of keys, as the Elgato app's Create Folder action does. The daemon
/// owns where the deck is, so this is never "run" like the other actions.
Folder { name: String },
```

- `KINDS` gains "Folder" at the next free index. That is **12**: App volume (PR 10) is 9,
  Multi action and Delay (PR 12) are 10 and 11.
  `empty_of_kind` gives `Folder { name: "" }`, and the GUI fills in a unique name.
- `summary()`: `Open folder media`, or `Open a folder` when the name is empty.
- `action::run` bails with "folder actions are handled by the daemon", like `Page` and
  `Brightness`. Add `Action::is_deck_owned()` (Page | Brightness | Folder) if PR 12 has
  not already added an equivalent. The daemon, the GUI's *Try it now* and PR 12's step
  runner all use that one test.
- **Multi-actions (assumption about PR 12):** a `Folder` step is handled the same way
  PR 12 handles a `Page` step, whichever way that is (sent back to the deck loop, or
  refused in the editor). This PR adds no special case.

## Core: where the deck is

Pure, in `config.rs` next to `switch_page`, and unit-tested:

```rust
pub const MAX_FOLDER_DEPTH: usize = 8;

/// One open folder, with the page its parent was on (restored by Back).
pub struct Frame { pub folder: String, pub parent_page: u8 }

/// Where the deck is: the open folders (outermost first) and the page shown.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Location { pub stack: Vec<Frame>, pub page: u8 }

impl Location {
    pub fn folder(&self) -> &str;          // "" at the root
    pub fn path(&self) -> Vec<String>;     // for the IPC event
    pub fn open(&mut self, name: &str) -> bool;  // page 0 of the folder
    pub fn back(&mut self) -> bool;        // pop one, restore parent_page
    pub fn home(&mut self) -> bool;        // root, the page the root was on
    pub fn revalidate(&mut self, cfg: &Config);  // after a reload
}
```

- `open` on a folder that is already on the stack pops back to it instead of pushing
  again, so a folder that opens its own parent cannot make the stack grow forever.
  Beyond `MAX_FOLDER_DEPTH` it does nothing and logs a warning.
- `revalidate` cuts the stack at the first folder that no longer exists, so the deck
  lands in the deepest folder that is still there, or at the root. The page is
  restored from that frame's `parent_page` and clamped with `page_count_in`.
- Paging inside a folder uses the existing `switch_page(cfg.page_count_in(folder), …)`.

## Render

- `Renderer::render_back(style, size, pressed)`: the theme's key background with a
  `BACK_GLYPH` arrow in the accent colour and no label. It reuses `render_face`
  through a synthetic `Face`, so `follow_theme`, `style.background` and the pressed
  ring all behave as on any key.
- A `Folder` key with no icon or glyph gets a default `FOLDER_GLYPH`. This works like
  the default volume and brightness glyphs on the strip.
- Glyphs: Material Design `arrow_left` (`f004d`) and `folder` (`f024b`). **Check both
  codepoints against the Nerd Fonts cheat sheet** before committing.

## Daemon (`omadeckd/src/main.rs`)

- `Deck.page: u8` becomes `Deck.loc: Location` (root page 0 on connect, as today).
  Add `Deck.back_down: Option<Instant>` for the hold.
- `write_key(d, …, i, pressed)`: in a folder, index 0 draws `render_back`; every other
  key draws `cfg.key_at(d.loc.folder(), d.loc.page, i)`. Keys beyond the deck's
  `key_count()` are not shown, the same as at the root.
- **Key press in a folder, index 0 (Back):** paint pressed and set `back_down`. The
  loop checks `back_down`, like the flash expiry does: once it has been held for
  `BACK_HOLD = 500 ms` it runs `go(d, …, Location::home)` and clears `back_down`. On
  release, if `back_down` is still set, it runs `back`. Any configured key at index 0
  is ignored, and `folder_warnings` has already logged it. Publish `KeyDown`/`KeyUp`
  as usual.
- **Other keys:** the dispatch match gains `Action::Folder { name } => open_folder(…)`.
  `Page` pages within the current folder's pages (`page_count_in`). Ordinary actions
  run and the folder stays open; there is no auto-exit (Elgato's default).
- **Dials:** they are device-level, so they are unchanged inside a folder. A dial press
  or strip tap whose action is `Folder` opens it. A `Folder` action on a turn opens it
  once. A dial `Page` action pages within the current folder.
- **Flick:** pages within the current folder when it has more than one page, otherwise
  it does nothing. It never pops the folder.
- `do_page` generalises to `go(d, cfg, renderer, hub, watch, f: impl FnOnce(&mut
  Location) -> bool)`. If the location changed, it repaints all keys, publishes the
  location event, re-sends the watcher probes (see below) and logs
  `folder media › page 1 of 2`.
- **Reload:** `d.loc.revalidate(&cfg)` replaces the page clamp, and the
  `folder_warnings` are logged. If the location changed, it is published.
- **Disconnect/reconnect:** a new `Deck` starts at the root, like the page does today.
- **CLI:** `omadeckd press KEY` runs root keys only (`Config::key`). A folder action
  there prints the same "handled by the daemon" error as a page action.

## IPC

PR 5's `PageChanged` is extended. A new event is not needed:

```rust
PageChanged {
    page: u8,
    /// Open folders, outermost first; empty at the root (and then not written).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    folders: Vec<String>,
},
```

- At the root, the wire shape is byte-identical to PR 5's. An old configurator ignores
  `folders` (serde ignores unknown fields) and shows the page number on the root, which
  is wrong only while a folder is open. A new configurator talking to an old daemon gets
  `folders = []`.
- **Hub replay:** `Inner` keeps the last `PageChanged` and replays it after the status
  line to a client that connects later. It is reset on `Disconnected`. Otherwise a
  configurator opened while the deck is inside a folder would show the root. This also
  fixes PR 5's version of the same gap.

## GUI

- `App.view_page: Cell<u8>` becomes `App.view: RefCell<Location>`. `view_page()`
  stays, and `view_folder() -> String` is added. `edit`, `clear_key`, `refresh_key`,
  DnD and the chips all go through `key_at`/`page_count_in` with the viewed folder.
- **Fix (PR 5 bug, needed here):** `editor::build_key` passes
  `move |c| c.key(i)…` as the action getter, which reads the *first key with that index
  on any page*. On page 2, or in a folder, the action editor reads the wrong key. It
  should use `c.key_at(&folder, page, i)`, captured when the editor is built.
- **Breadcrumb**, in the chips row, to the left of the chips and only when a folder is
  being viewed: `Deck › Media › Spotify` as flat buttons. Clicking one goes to that
  level (the deck at its root page, a folder at page 0). The chips show the viewed
  folder's pages.
- **Grid in a folder:** tile 0 shows the Back face. Clicking it goes up a level. It
  cannot be selected or dragged and is not a drop target. Its tooltip reads "Back —
  fixed in folders, as in the Elgato app".
- **Entering a folder:** double-click a folder key (`GestureClick`, `n_press == 2`). A
  single click still selects it. The folder action's editor also has an **Open folder**
  button.
- **Creating a folder:** choose "Folder" in the action combo. The GUI sets
  `name = cfg.new_folder_name()` and, if the label is empty, `label = "Folder"`. The
  Folder arm has a **Name** entry: renaming calls `rename_folder`, a name already in
  use shows the `error` style and keeps the old name, and the entry's subtitle says how
  many keys the folder holds.
- **Moving a key into or out of a folder:** a **Folder** combo row in Appearance, next
  to the Page spin, listing "Deck" plus every folder (with its path). Picking one calls
  `move_key` and follows the key there (the view switches and the key stays selected).
  A full destination shows the toast "Media is full". Dragging a key onto a folder tile
  is Later.
- **Clearing a folder key:** if no other key opens that folder, an `adw::AlertDialog`
  asks "Delete the folder Media and its N keys?". Delete calls `delete_folder`, and
  Keep clears only the key (the folder stays, and `folder_warnings` reports it as
  unreachable).
- *Try it now* is insensitive for `is_deck_owned()` actions.
- `PageChanged { page, folders }` sets `view` to that location, so the configurator
  follows the deck. `KeyDown { index: 0 }` while viewing a folder does not select
  anything.
- Title subtitle: `· Media · page 1 of 2` (the folder part only inside a folder).
- `reload_config`: `view.revalidate(&cfg)`, just as the daemon does.

## Starter layout

Unchanged. The starter fills one page for a 15-key deck, and a folder would be the
first thing a new user meets that the Elgato-style canvas explains better than a
starter does. Folders are shown in the README and the example config instead.

## Interactions with other PRs

- **State watcher (PR 8/9):** the probes come from the visible keys. I assume PR 8 has
  a function like `watched_probes(cfg, page, n_keys, n_dials)` that walks
  `cfg.keys_on(page)`. This PR changes it to take `&Location` and walk
  `cfg.keys_at(loc.folder(), loc.page)` plus the dials, and `go()` re-sends
  `WatchCmd::Watch` after every location change, as PR 8 does on page change. The
  Back tile has no probe.
- **ctl (PR 11):** PR 11 sends page and brightness as `Command::Run { action }` and
  adds `omadeckd page …` next to `press`/`dial`. Add `omadeckd folder NAME` (sends
  `Run { action: Action::Folder { name } }`), plus `Command::Back` and `Command::Home` for
  `omadeckd back` and `omadeckd home`. They go through the same `go()`. `omadeckd page N`
  pages within the current folder, as a page key does. PR 11's `State` reply gains the
  folder path.
- **App pages (PR 15):** assumption: an app's page is a root page. Switching to it
  calls `Location::home()` and then sets the page, so focusing an app leaves any open
  folder. Switching back on unfocus restores the root page PR 15 remembers, not the
  folder. A folder as an app's target is PR 15's call (it would be `Location` instead
  of `u8`).

## Docs

- README: in Features, "Pages and folders" (a folder key opens a sub-page with a Back
  key top-left; hold Back for the front page; folders nest and can have pages). Add a
  `folder` row to the action table, `folder` to the key-fields paragraph, a TOML
  example, and tick the roadmap item `[x] Pages and folders`.
- `examples/config.toml`: a commented Media folder (the example above).
- CHANGELOG under Unreleased → Added.

## Implementation steps

1. [ ] core: `KeyConfig.folder`, the `*_at` methods, the `page_count()` fix, sorting,
       `folder_exists`/`rename_folder`/`delete_folder`/`new_folder_name`/`move_key`/
       `folder_warnings`, with their tests.
2. [ ] core: `Action::Folder`, KINDS, summary, `run` bail, `is_deck_owned`.
3. [ ] core: `Location` with `open`/`back`/`home`/`revalidate`, with tests.
4. [ ] render: `render_back`, the default folder glyph (codepoints checked).
5. [ ] ipc: `PageChanged.folders`, plus the hub replay of the last location.
6. [ ] daemon: `Deck.loc`, `write_key`, Back press and hold, `go()`, folder and page
       dispatch for keys, dials and flicks, reload `revalidate`, warnings, CLI.
7. [ ] daemon: watcher probes per location (on top of PR 8/9), and the ctl commands
       (on top of PR 11).
8. [ ] GUI: `view: Location`, the action-getter fix, Back tile, breadcrumb,
       double-click to open, follows `PageChanged`.
9. [ ] GUI: Folder action arm (name, Open), Folder combo in Appearance (`move_key`),
       delete-folder dialog.
10. [ ] Docs, then `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`.

## Tests

- config: `folder_roundtrip`; `folder_omitted_when_empty` (an old config saves
  byte-identical); `old_config_loads_at_root`; `page_count_ignores_folders`;
  `page_count_in_folder`; `key_at_separates_folders` (same index and page in two
  folders); `save_sorts_root_first`; `rename_folder_moves_keys_and_actions`;
  `rename_folder_refuses_existing`; `delete_folder_is_recursive`;
  `delete_folder_keeps_shared_subfolder` (a sub-folder also opened from elsewhere
  survives); `new_folder_name_skips_used`; `move_key_skips_index_0_in_folder`;
  `move_key_full_is_none`; `folder_warnings_index_0_and_orphan`;
  `empty_folder_exists_via_action`.
- action: `folder_roundtrip` and wire shape (`type = "folder"`, `name`);
  `folder_summary`; `run_refuses_folder`; `is_deck_owned`.
- location: `open_back_restores_parent_page`; `home_from_nested`;
  `open_existing_on_stack_pops_to_it`; `depth_capped`;
  `revalidate_deleted_inner_keeps_outer`; `revalidate_deleted_outer_goes_root`;
  `revalidate_clamps_page`; `back_at_root_is_noop`.
- ipc: `page_changed_root_wire_unchanged` (pins PR 5's line);
  `page_changed_with_folders_roundtrip`; `old_line_parses_with_no_folders`.
- render: `back_key_renders_glyph` (a non-background pixel in the middle, using
  `glyph: "A"`-style font independence as `key_glyph_stays_in_its_box` does);
  `folder_key_gets_default_glyph`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] A folder key opens its folder: top-left shows the Back arrow in the theme accent,
      and the folder's keys are at their indices.
- [ ] Back goes up one level and restores the page the parent was on; holding Back
      ~0.5 s goes straight to the front page (two levels deep).
- [ ] A nested folder opens, and Back walks out of it level by level.
- [ ] Running an ordinary action inside a folder keeps the folder open.
- [ ] A two-page folder: a page key and a flick page within the folder; a flick in a
      one-page folder does nothing.
- [ ] Dials and strip panels are unchanged while a folder is open; a volume dial keeps
      working and its bar stays live.
- [ ] Deleting the open folder from the config (hand edit) drops the deck to its
      parent or the root without a blank or stale key.
- [ ] Theme change while inside a folder repaints the Back key in the new colours.
- [ ] The configurator follows the deck into and out of folders (breadcrumb and chips),
      and when opened while the deck is in a folder it shows that folder (hub replay).
- [ ] Double-click opens a folder in the configurator; the Back tile can't be
      selected or dragged; moving a key into a folder through the Folder row works.
- [ ] Unplug and replug inside a folder: the deck comes back at the root page 0.
- [ ] A config with no folders loads, works and saves byte-identical.
- [ ] Toggle/live keys (PR 8/9) inside a folder update while it's open, and their
      probes stop when it's closed (daemon log).

## Decisions to explain

- **Folder as a `folder` string on the key, not nested tables.** It keeps the TOML flat
  and hand-editable, like PR 5's `page`, and old files are untouched. Nested
  `[[key.keys]]` tables would make deep folders unreadable and move every key's
  position in the file. A flat name namespace means a folder can be opened from two
  places, which Elgato does not offer but which costs nothing.
- **No `[[folder]]` tables.** In Elgato the folder key *is* the folder's face, so its
  label and icon live on that key. Folder-level settings (auto-exit) can go on the
  `Folder` action later.
- **Back fixed at index 0, with absolute indices.** This matches Elgato's top-left,
  non-movable Back key. Folder keys keep physical indices (index 1 is the second key),
  so `key_at`, DnD and different key counts all work as on the root. The cost is that
  index 0 is unusable in folders; the GUI never creates one there, and the daemon warns.
- **Folders have pages; page keys and flicks act within the current folder.** Elgato
  is unconfirmed here. With `(folder, page)` everywhere this costs almost nothing, and
  paging the root behind a closed folder would be invisible. If Elgato turns out to
  allow one page per folder, the GUI can hide the chips in folders without a config
  change.
- **Back acts on release, hold acts at 500 ms.** Other keys act on press. A hold
  gesture needs the press to wait, and this follows Elgato's "hold Exit to return to
  the main layout". The exact hold time is unconfirmed.
- **Dials untouched by folders.** This follows PR 5's device-level dials. Whether
  Elgato's + folders carry their own dials is unconfirmed (see Later).
- **Reload falls back to the deepest surviving folder,** not always the root page 0.
  It is the same rule applied level by level, and it keeps you near where you were
  after an unrelated edit.
- **`PageChanged` extended instead of a new event.** The root wire shape stays pinned,
  old clients degrade gracefully, and there is still one "where is the deck" event.
- **`page_count()` now counts root keys only,** and `Config::key(i)` (the CLI) only
  root keys. Both are needed fixes, because folder keys would otherwise leak into root
  paging and `omadeckd press`.
- **Starter layout gets no folder** (see above).

## Later

- **Auto Exit** (Elgato 6.5): `auto_exit_ms: Option<u32>` on the `Folder` action, plus
  a per-action variant.
- Dragging a key onto a folder tile to move it in; pasting a folder as a page and back
  (Elgato 6.6); pinned keys across pages.
- Per-folder dials on the Stream Deck +, if Elgato's folders turn out to carry them.
- Moving the Back key (Elgato doesn't allow it either).
- A deck-aware snapshot of a folder (`omadeckd snapshot --folder media`).

**Size:** about 650–750 lines: core ~250 with tests, daemon ~120, ipc/hub ~40,
GUI ~250, docs ~40. If it passes ~800, split the configurator part (steps 8–9) into a
follow-up PR. The daemon alone is usable by hand-editing the config.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- Pages inside folders (planned), or one page per folder like Elgato? With one page per folder the configurator just hides the page chips inside folders; no config change.
- Elgato 6.5's Auto Exit (leave the folder after running an action): later (planned) or in this PR?
- Back-key hold time (500 ms planned) and the arrow and folder glyph codepoints (`f004d`, `f024b`) still to confirm.
- About 650–750 lines: split the configurator part into a follow-up PR if it passes ~800?
