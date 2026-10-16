# PR 13: `feat/folders`: folder gestures

Part of [PLAN.md](PLAN.md). Base: `feat/multi-actions` (PR 12). It needs `feat/groundwork`
(the `Daemon` struct, `Daemon::run_input`, `next_deadline`, D3/D4), upstream's pages
(D12), fix/review's `Config::is_folder`, and PR 11's `key_down` (the `ButtonDown` arm calls
it). Nothing else from PRs 8–12 is used.

**Rescoped (2026-10-17).** The old plan built folders from scratch: a `folder` string on
`KeyConfig`, `key_at`, `Location`, `Action::Folder`, a fixed Back tile. Upstream's
`pages-and-folders` (`d4e458f`, `fb9052f`) already does most of it, so all of that is
gone. This PR adds only the parts of Elgato's folder behaviour that upstream's model is
missing, plus one nested-folder fix.

**Outcome:** holding a Back key for half a second goes straight Home, as in the Elgato
app. A flick or page step from inside a folder that was opened from another folder steps
from the page the folders hang off, not from Home. In the configurator you double-click a
folder key to open it and double-click a Back key to go up, and the page switcher shows
folders with their path (`Music › Media › Spotify`).

## What's already there, and what's left

| Elgato | Upstream / ours as merged (`d7bd448`) | This PR |
|---|---|---|
| A folder opens from a key | `Action::Page` (kind 6) + `[[page]] folder = true` | — |
| Back goes up one level | `Action::Back` (kind 7), `Nav::go`/`Nav::back` breadcrumb | — |
| Back key placed for you | GUI `back_key()`: index 0, label "Back", glyph `f004d` | — |
| Folders nest | `Nav::go` pushes the trail; loops are cut | — |
| A default folder face | GUI `new_folder()`: glyph `f024b` if the key is plain | — |
| Configurator follows the deck | `Event::PageChanged`, hub replays `page` | — |
| **Hold Back → main layout** | Back acts on key-down; no hold | **Task 2** |
| Page step from a nested folder | `dispatch` uses `nav.parent()`: one level only (2026-10-17 review) | **Task 1** |
| **Open a folder by double-clicking it** in the editor | Only via the page dropdown or the Open-page row | **Task 2** |
| Breadcrumb in the editor | Flat dropdown of page names | **Task 2** (path in the dropdown labels) |
| Back fixed top-left, not editable | Back is an ordinary key: movable, deletable | Not done, on purpose (below) |
| Auto Exit (6.5) | — | Later |

## How the Elgato app does it (researched 2026-10-16, trimmed)

- **Back key:** placed top-left and the app gives no way to move it. The evidence is
  indirect: a community tool exists only to move it by editing profile files
  ([BarRaider folder editor](https://github.com/BarRaider/streamdeck-foldereditor)).
- **Leaving a folder:** Back goes up one level at a time, and **holding** the Back key goes
  straight to the main layout
  ([folders guide](https://www.elgato.com/explorer/products/stream-deck/how-to-use-folders-stream-deck/)).
  The hold time is not documented (unconfirmed). Whether Back acts on press or on release
  is unconfirmed too.
- **Nesting:** "Folders can live inside other folders" (folders guide). No depth limit is
  documented.
- **Editing:** in the editor you double-click a folder to open it, and a breadcrumb above
  the canvas shows where you are (unconfirmed in the docs; it is how the app behaves).
- **Auto Exit** (Stream Deck 6.5): leave a folder after an action runs, off by default
  ([6.5 release notes](https://help.elgato.com/hc/en-us/articles/22500155667469-Elgato-Stream-Deck-6-5-Release-Notes)).
- **Stream Deck + dials inside a folder:** nothing found. Dials stay device-level in
  omadeck (PR 5), so a folder doesn't change them.

## Decided

- **Decided: rescope, don't retire.** Hold-Back-for-Home is the one Elgato folder gesture
  upstream lacks, and the nested-folder page step is a real (small) bug in our PR 5 code.
  Together with the configurator's double-click that is about 350 lines in two tasks:
  small, but a feature someone coming from Elgato will reach for. Folding it into other
  plans would split one user-visible behaviour across three PRs.
- **Decided: Back stays an ordinary key.** Upstream made Back a normal action that the GUI
  places at index 0 of a new page. Forcing it top-left and read-only would fight
  upstream's model (and its `back_key()`), and nobody has asked to move it less. Hold
  works on **any** key whose action is `Back`, wherever it sits.
- **Decided: Back acts on release.** A hold has to wait for the release to know it wasn't
  a hold, so a `Back` key no longer acts on key-down. Every other key still acts on
  key-down. Held for `BACK_HOLD` (500 ms) it goes Home at once, while still held, and the
  release then does nothing.
- **Decided: hold only on keys.** Dial presses, strip taps and ctl (`Source::Ctl`) run
  `Back` at once, as today. PR 16's long press is a separate action, not a hold.
- **Decided: `Nav::nearest` replaces `Nav::parent`.** Page steps start from the nearest
  page on the trail that isn't a folder, however deep the folders go. `parent()` has no
  other caller and would be dead code (the daemon is a binary crate, so clippy fails).
- **Decided: the configurator's "parent" comes from the config,** not from the deck's
  trail: `Config::opener(name)` is the page holding the first key that opens it (Home
  first, then file order). The GUI has no trail of its own, and the config answer is the
  same one a user sees when reading the TOML.
- **Decided: breadcrumb as dropdown labels,** not a new widget. A folder page's entry in
  the page switcher reads `Music › Media › Spotify` (via `Config::page_path`). No layout
  work, and nested folders are told apart in a flat list.
- **Decided: no Auto Exit, no folder-only paging.** Later, if wanted.

## Ask before building

1. Back acting on **release** instead of press (needed for the hold). OK, or keep press
   and make the hold a separate "Home" key instead?
2. 500 ms hold. Elgato's value is undocumented; measure it on the Elgato app if you can.

## Before building: check against the base

The daemon parts are written against `feat/groundwork`'s shapes from D3/D4, which weren't
merged when this plan was written. Before Task 2, check these on `feat/multi-actions` and
use the branch's names if they differ:

```bash
grep -n "struct Daemon\|fn run_input\|fn next_deadline\|enum Source\|struct Deck\|fn show_page\|dial_pressed" crates/omadeckd/src/*.rs
grep -rn "Action::PageStep { change } =>" crates/omadeckd/src
```

Expected (groundwork, PRs 8, 11, 12): `deck.rs`: `struct Deck { dev, reader, kind, info,
dial_pressed, flash_until, dirty_panels, dirty_keys, key_flash }` built in `deck::open`;
`daemon.rs`: `enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick, Ctl, Step }`,
`fn Daemon::try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()>`
(PR 11) with `fn Daemon::run_input(..) -> bool` its wrapper,
`fn Daemon::next_deadline(&self) -> Option<Instant>`, the `PageStep` arm in
`fn Daemon::dispatch`, and PR 11's `fn key_down(&mut self, i: u8) -> Result<()>` (paint
pressed, publish `KeyDown`, then PR 8's `press_key(i)`), which the `ButtonDown` arm and
ctl's `Press` both call. fix/review already added `Config::is_folder(&self, name) -> bool`.

## Code as merged (verbatim, `d7bd448`)

```rust
// crates/omadeck-core/src/config.rs
/// A named page of keys. Pages are flat: a "folder" is just a page with a Back key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    #[serde(default, rename = "key", skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    /// A folder: opened from a key, with a Back key. Folders are skipped by page steps,
    /// so a flick can't land inside one.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
}
pub fn has_page(&self, name: &str) -> bool;
pub fn step_page(&self, current: Option<&str>, change: i32) -> Option<Option<String>>;
    // Home + non-folder pages, wraps; a current that isn't steppable steps from Home
pub fn links_to(&self, name: &str) -> usize;

// crates/omadeckd/src/nav.rs
#[derive(Debug, Default)]
pub struct Nav {
    /// `None` is the home page.
    current: Option<String>,
    history: Vec<Option<String>>,
}
pub fn current(&self) -> Option<&str>;
pub fn go(&mut self, page: Option<&str>) -> bool;      // Home clears, revisit truncates, else push
pub fn parent(&self) -> Option<Option<String>>;        // history.last()
pub fn switch(&mut self, page: Option<&str>) -> bool;  // replace current, clear history
pub fn back(&mut self) -> bool;                        // pop or Home; false on Home
pub fn retain_existing(&mut self, cfg: &Config) -> bool;
```

The `PageStep` arm of `dispatch` in `d7bd448` (groundwork moved it, unchanged apart
from `self.`, into `Daemon::dispatch` in `daemon.rs`):

```rust
Action::PageStep { change } => {
    // From inside a folder, step from the folder's parent, leaving the folder.
    let in_folder = cfg.pages.iter().any(|p| p.folder && nav.current().is_some_and(|c| p.name == c));
    let origin = if in_folder { nav.parent().flatten() } else { nav.current().map(str::to_string) };
    let by = i32::from(*change) * steps as i32;
    match cfg.step_page(origin.as_deref(), by) {
        Some(page) => Ok(nav.switch(page.as_deref())),
        None => Ok(false),
    }
}
```

GUI: `App::show_page(self: &Rc<Self>, page: Option<String>, tell_daemon: bool)`,
`App::key(&self, i: u8) -> Option<KeyConfig>` (a clone, for the page being edited),
`App::current_page() -> Option<String>`, `fn sync_page_list(&self)` (fills `page_list`
with `HOME` then `cfg.page_names()`), key buttons built in `fn rebuild_grid` with
`button.connect_clicked(… app.select(Some(Selection::Key(i))))`.

## Task 1: nearest origin and the config helpers (~150 lines)

**Files:** `crates/omadeck-core/src/config.rs`, `crates/omadeckd/src/nav.rs`, the
`PageStep` arm in `crates/omadeckd/src` (grep above).

New in `impl Config`, next to `has_page` (fix/review's `is_folder` is already there;
don't add a second):

```rust
/// The page holding the first key that opens `name`: Home first, then the pages in file
/// order (never `name` itself). `None` when no key opens it.
pub fn opener(&self, name: &str) -> Option<Option<&str>> {
    let opens = |keys: &Keys| keys.iter().any(|k| matches!(&k.action, Action::Page { page } if page == name));
    if opens(&self.keys) {
        return Some(None);
    }
    self.pages.iter().filter(|p| p.name != name).find(|p| opens(&p.keys)).map(|p| Some(p.name.as_str()))
}

/// Where `name` sits, for the configurator: the folders it's inside and the page they hang
/// off, outermost first, ending with `name`. A normal page is just itself.
pub fn page_path(&self, name: &str) -> Vec<String> {
    let mut path = vec![name.to_string()];
    let mut cur = name;
    while self.is_folder(cur) && path.len() < 8 {
        match self.opener(cur) {
            Some(Some(p)) if !path.iter().any(|q| q == p) => {
                path.insert(0, p.to_string());
                cur = p;
            }
            _ => break,
        }
    }
    path
}
```

New in `impl Nav`, replacing `parent`:

```rust
/// The nearest page on the trail, current first, that `keep` accepts; `None` is Home.
/// Page steps use it to step from the page a folder hangs off, however deep.
pub fn nearest(&self, keep: impl Fn(&str) -> bool) -> Option<&str> {
    std::iter::once(&self.current)
        .chain(self.history.iter().rev())
        .map(|p| p.as_deref())
        .find(|p| p.is_none_or(&keep))
        .flatten()
}
```

The `PageStep` arm becomes (field names as on the base branch):

```rust
Action::PageStep { change } => {
    // From inside a folder (however deep), step from the page the folders hang off.
    let origin = self.nav.nearest(|p| !self.cfg.is_folder(p)).map(str::to_string);
    let by = i32::from(*change) * steps as i32;
    match self.cfg.step_page(origin.as_deref(), by) {
        Some(page) => Ok(self.nav.switch(page.as_deref())),
        None => Ok(false),
    }
}
```

- Delete `Nav::parent` and its test `parent_is_where_back_would_go`; grep `\.parent()` in
  `crates/omadeckd` first to be sure nothing else calls it.
- **Trap:** `.map(str::to_string)` is what ends the borrow of `self.nav` before
  `self.nav.switch`. Don't keep `origin` as `Option<&str>`.

Tests: in `nav.rs` (copy this one), and in `config.rs`
`page_path_follows_folder_openers` (Music opens folder Media, Media opens folder Spotify,
Spotify opens Media back: `page_path("Spotify") == ["Music", "Media", "Spotify"]`,
`page_path("Music") == ["Music"]`, a folder nothing opens is just itself,
`opener("Media") == Some(Some("Music"))`).

```rust
#[test]
fn nearest_skips_nested_folders() {
    let mut n = Nav::default();
    n.go(Some("Music"));
    n.go(Some("F1"));
    n.go(Some("F2"));
    let not_folder = |p: &str| !p.starts_with('F');
    assert_eq!(n.nearest(not_folder), Some("Music"), "two folders deep steps from Music");
    n.go(None);
    n.go(Some("F1"));
    assert_eq!(n.nearest(not_folder), None, "a folder opened from Home steps from Home");
    n.switch(Some("Music"));
    assert_eq!(n.nearest(not_folder), Some("Music"), "a normal page is its own origin");
}
```

**Check:**
`cargo test -p omadeckd nav:: && cargo test -p omadeck-core page_path && cargo clippy --all-targets -- -D warnings`
→ `test result: ok. 5 passed` (nav: four kept, one new), `test result: ok. 1 passed`, and
clippy prints no warnings.

## Task 2: hold Back for Home, double-click in the configurator, docs (~200 lines)

**Files:** `crates/omadeckd/src/nav.rs`, `crates/omadeckd/src/deck.rs` (`Deck`),
`crates/omadeckd/src/daemon.rs` (the input arms, `next_deadline`, `pass`),
`crates/omadeck-gui/src/app.rs`, `README.md`, `CHANGELOG.md`.

In `nav.rs`:

```rust
use std::time::{Duration, Instant};

/// A key with a `Back` action that's down. Back waits for the release, so holding it can
/// go Home instead, as in the Elgato app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackHold {
    pub key: u8,
    pub since: Instant,
}

impl BackHold {
    pub const HOLD: Duration = Duration::from_millis(500);

    /// When holding turns into "go Home".
    pub fn deadline(&self) -> Instant {
        self.since + Self::HOLD
    }
}

impl Nav {
    /// Go straight Home and clear the trail (holding Back). Returns whether anything changed.
    pub fn home(&mut self) -> bool {
        self.go(None)
    }
}
```

Daemon:

- `Deck` gains `back_held: Option<BackHold>` (input state, like `dial_pressed`; a replug
  resets it). `None` in `deck::open`.
- **The `ButtonDown(i)` arm** (PR 11 made it `let _ = self.key_down(i);`): if the key's
  action is `Action::Back`, paint pressed (`self.paint_key(i, true)`), publish `KeyDown`,
  set `back_held = Some(BackHold { key: i, since: Instant::now() })` the way the
  `EncoderDown` arm sets `dial_pressed`, and **don't** call `key_down`. Any other action
  calls `key_down(i)` as now. Only the hardware arm changes: ctl's `Press` still goes
  through `key_down` → `press_key`, so it runs Back at once ("hold only on keys").
- **`ButtonUp(i)`:** after the paint and `KeyUp`:
  `if deck.back_held.take_if(|h| h.key == i).is_some() { self.run_input(Source::Key(i), &Action::Back, 1); }`.
- **Hold:** `Daemon::next_deadline()` also includes `back_held.map(|h| h.deadline())`
  (beside the flashes, `repaint_at`, `reload_at` and PR 12's `seq.next_due()`). In
  `pass`, next to the flash expiry:
  `if let Some(h) = deck.back_held.take_if(|h| now >= h.deadline())` → log
  `key {}: held Back, going Home` and, if `self.nav.home()`, repaint and publish the page
  the way `run_input` does after a page move (groundwork's `show_page`). The release that
  follows finds `back_held` empty and does nothing.
- **Trap (BUGS.md 4):** don't call `self.run_input(..)` while holding a borrow of
  `self.cfg` or `self.deck`. Clone the key's action first
  (`let act = self.cfg.key(self.nav.current(), i).map(|k| k.action.clone())`), set
  `back_held` in its own short `if let Some(d) = &mut self.deck` block, then dispatch.

GUI (`app.rs`):

- In `rebuild_grid`, after `button.connect_clicked(…)`, add a double-click gesture. It runs
  in the capture phase so the button's own click (select) still happens:

```rust
// Double-click a folder key to open its page, or a Back key to go up, as in the Elgato app.
let open = gtk::GestureClick::builder().button(1).propagation_phase(gtk::PropagationPhase::Capture).build();
let weak = Rc::downgrade(self);
open.connect_pressed(move |_, n_press, _, _| {
    if n_press == 2
        && let Some(app) = weak.upgrade()
    {
        app.open_from_key(i);
    }
});
button.add_controller(open);
```

- New method:

```rust
/// Follow key `i`: an Open page key shows its page, a Back key the page that opens this
/// one (Home if none). The deck follows, as with the page switcher.
pub fn open_from_key(self: &Rc<Self>, i: u8) {
    let target = match self.key(i).map(|k| k.action) {
        Some(Action::Page { page }) if self.config.borrow().has_page(&page) => Some(page),
        Some(Action::Back) => match self.current_page() {
            Some(cur) => self.config.borrow().opener(&cur).flatten().map(str::to_string),
            None => return,
        },
        _ => return,
    };
    self.show_page(target, true);
}
```

- `sync_page_list`: label each page with `cfg.page_path(name).join(" › ")` instead of the
  bare name. The order is unchanged, so the dropdown's index → page mapping in
  `connect_selected_notify` still works.
- The hint under the deck: add `· double-click a folder to open it`.
- **Trap:** keep the `self.config.borrow()` calls as temporaries inside the expressions
  above; `show_page` borrows `config` mutably further down.

Docs: README "Pages and folders": "hold a Back key to go straight Home; double-click a
folder in the configurator to open it". CHANGELOG under Unreleased → Added (hold Back)
and Fixed (page steps from nested folders).

Test (in `nav.rs`, copy):

```rust
#[test]
fn home_clears_a_nested_trail() {
    let mut n = Nav::default();
    n.go(Some("A"));
    n.go(Some("F1"));
    n.go(Some("F2"));
    assert!(n.home());
    assert_eq!(n.current(), None);
    assert!(n.history.is_empty());
    assert!(!n.home(), "home from Home does nothing");
    assert!(!n.back());
    let t = Instant::now();
    assert_eq!(BackHold { key: 0, since: t }.deadline(), t + Duration::from_millis(500));
}
```

**Check:** `cargo test -p omadeckd nav:: && cargo clippy --all-targets -- -D warnings`
→ `test result: ok. 6 passed`, no clippy warnings. Then the hardware checks.

## Capture on the machine first

Nothing: this PR reads no system state.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Tap Back in a folder: goes up one level on release.
- [ ] Hold Back ~0.5 s two folders deep: the deck goes Home while the key is still held;
      releasing does nothing more.
- [ ] A Back key on a normal page (not a folder) works the same.
- [ ] Two folders deep (Music → Media → Spotify), flick left: the deck shows the page
      after Music, not the page after Home.
- [ ] Double-click a folder key in the configurator: it opens, and the deck follows.
      Double-click its Back key: back to the page that opens it. A single click still
      selects; dragging keys still works.
- [ ] The page switcher shows `Music › Media › Spotify` for a nested folder.
- [ ] Unplug while holding Back, replug: no stray Home or Back.

## Decisions to explain

- **Hold Back for Home** copies Elgato's "hold Exit to return to the main layout".
- **Back acts on release.** A hold needs the press to wait; only Back keys change.
- **Back is not fixed top-left.** Upstream's Back is an ordinary key; we keep that.
  Elgato's is fixed (indirect evidence), so this is a deliberate difference.
- **The configurator's breadcrumb is in the page switcher labels,** worked out from the
  config, not from the deck's trail.
- **Nested folders step from their root page,** extending PR 5's "a flick inside a
  folder steps among the folder's parent's siblings" to any depth.

## Later

- Auto Exit (Elgato 6.5): `auto_exit` on `[[page]]` for folders.
- A real breadcrumb widget above the grid, if the dropdown labels aren't enough.
- Copy a folder as a page and back (Elgato 6.6).

**Size:** about 350 lines: core ~70 with tests, nav ~70 with tests, daemon ~50, GUI ~50,
docs ~20.
