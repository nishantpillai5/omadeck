# PR 13: `feat/folders`: folder gestures

Part of [PLAN.md](PLAN.md). Base: `feat/multi-actions` (PR 12).

**Status (2026-10-17): built, not verified.** Built on the Mac and pushed to the fork as
`feat/folders`: 9978f6a … b05fb0e (3 commits, tasks 1–3). Core and daemon tests, clippy and fmt pass; the GUI type-checks
(not linked or run; GUI clippy warns, BUGS.md 78), and nothing was run on the deck or with the captures below. Where the code
differs from this plan, the code wins: the deviations, the defaults taken in place of
captures, and every check still to do are in its `feat/folders` section of [VERIFY.md](VERIFY.md). The review
of what was built is [review-2026-10-17c.md](review-2026-10-17c.md).

**Rescoped (2026-10-17).** The old plan built folders from scratch: a `folder` string on
`KeyConfig`, `key_at`, `Location`, `Action::Folder`, a fixed Back tile. Upstream's
`pages-and-folders` (`d4e458f`, `fb9052f`) already does most of it, so all of that is
gone. This PR adds only the parts of Elgato's folder behaviour that upstream's model is
missing, plus one nested-folder fix and the held-key fix the Back hold needs (BUGS.md
65, a held key repainted as released).

**Outcome:** holding a Back key for half a second goes straight Home, as in the Elgato
app. A flick or page step from inside a folder that was opened from another folder steps
from the page the folders hang off, not from Home. A key you hold stays drawn pressed
while its face changes underneath (today the end-of-pass paint draws it released: BUGS.md
65). In
the configurator you double-click a folder key to open it and double-click a Back key to
go up, and the page switcher shows folders with their path (`Music › Media › Spotify`).

**Size:** about 450 lines over three tasks: core and nav ~150, daemon ~180, GUI and docs
~120.

## What's already there, and what's left

| Elgato | omadeck as merged (`55d0d62`) | This PR |
|---|---|---|
| A folder opens from a key | `Action::Page` (kind 6) + `[[page]] folder = true` | — |
| Back goes up one level | `Action::Back` (kind 7), `Nav::go`/`Nav::back` breadcrumb | — |
| Back key placed for you | GUI `back_key()`: index 0, label "Back", glyph `f004d` | — |
| Folders nest | `Nav::go` pushes the trail; loops are cut | — |
| A default folder face | GUI `new_folder()`: glyph `f024b` if the key is plain | — |
| Configurator follows the deck | `Event::PageChanged`, hub replays `page` | — |
| **Hold Back → main layout** | Back acts on key-down; no hold | **Task 2** |
| A held key looks held | `flush` paints dirty keys with `pressed: false` (BUGS.md 65) | **Task 2** |
| Page step from a nested folder | `dispatch` uses `nav.parent()`: one level only | **Task 1** |
| **Open a folder by double-clicking it** in the editor | Only via the page dropdown or the Open-page row | **Task 3** |
| Breadcrumb in the editor | Flat dropdown of page names | **Task 3** (path in the dropdown labels) |
| Back fixed top-left, not editable | Back is an ordinary key: movable, deletable | Not done, on purpose (below) |
| Auto Exit (6.5) | — | Later |

## How the Elgato app does it (researched 2026-10-16, trimmed)

- **Back key:** placed top-left and the app gives no way to move it. The evidence is
  indirect: a community tool exists only to move it by editing profile files
  ([BarRaider folder editor](https://github.com/BarRaider/streamdeck-foldereditor)).
- **Leaving a folder:** Back goes up one level at a time, and **holding** the Back key goes
  straight to the main layout
  ([folders guide](https://www.elgato.com/explorer/products/stream-deck/how-to-use-folders-stream-deck/)).
  The hold time is not documented (unconfirmed). Whether Back acts on press or
  release is unconfirmed: check in the Elgato app.
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
  With the configurator's double-click it is small, but it is a feature someone coming
  from Elgato will reach for. Folding it into other plans would split one user-visible
  behaviour across three PRs.
- **Decided: Back stays an ordinary key.** Upstream made Back a normal action that the GUI
  places at index 0 of a new page. Forcing it top-left and read-only would fight
  upstream's model (and its `back_key()`), and nobody has asked to move it less. Hold
  works on **any** key whose press would run `Back`, wherever it sits.
- **Decided: Back acts on release** (needed to tell a tap from a hold), not a separate "Home"
  key. A hold has to wait for the release to know it wasn't a hold, so a key whose press would run `Back` no longer acts on key-down. Every other
  key still acts on key-down. Held for `BACK_HOLD` (500 ms) it goes Home at once, while
  still held, and the release then does nothing. "Would run Back" is the action
  `press_key` would pick: `KeyConfig::action_for(on)`, so a toggle whose on action is
  Back holds only while on. A multi action (PR 12) with a Back step is not a Back key.
- **Decided: the hold is 500 ms,** one constant, `BACK_HOLD`. Elgato's value is
  undocumented; 500 ms is long enough not to catch a tap.
- **Decided: the release runs the press through `press_key`,** the same path a key-down
  takes, so toggles, jobs and PR 11's error result behave as for any key.
- **Decided: hold only on hardware keys.** Dial presses, strip taps and ctl presses
  (`Source::Ctl`, PR 11's `key_down`) run `Back` at once, as today. PR 16's long press is
  a separate action, not a hold.
- **Decided: held-key state lives in `Deck`** (BUGS.md 65, review 2's daemon finding 9;
  decided in the round-2 review): `key_pressed: Vec<bool>`, set on `ButtonDown`, cleared on `ButtonUp`, and
  `back_held: Option<BackHold>` beside it. A replug resets both, like `dial_pressed`.
  `Daemon::key_states` draws a key pressed while it is held, or while PR 11's
  `key_flash` is on, so a held key no longer flips to released when its face repaints.
- **Decided: a page change forgets held keys.** `show_page` clears `key_pressed` and
  `back_held`: the key under your finger on the new page is drawn released, and its
  release only repaints it. That is what the deck shows today after a page key.
- **Decided: `Nav::nearest` replaces `Nav::parent`.** Page steps start from the nearest
  page on the trail that isn't a folder, however deep the folders go. `parent()` has no
  other caller and would be dead code (the daemon is a binary crate, so clippy fails).
- **Decided: the configurator's "parent" comes from the config,** not from the deck's
  trail: `Config::opener(name)` is the page holding the first key that opens it (Home
  first, then file order), looking at every action a key owns (fix/review-2's
  `KeyConfig::actions()`: its action, its on action, and through PR 12's `Multi` arm in
  `Action::walk`, a multi's steps). The GUI has no
  trail of its own, and the config answer is the one a user sees when reading the TOML.
- **Decided: breadcrumb as dropdown labels,** not a new widget. A folder page's entry in
  the page switcher reads `Music › Media › Spotify` (via `Config::page_path`). No layout
  work, and nested folders are told apart in a flat list.
- **Decided: no Auto Exit, no folder-only paging.** Later, if wanted.

## Ask before building

None; all settled 2026-10-17.

## What this PR builds on

### Code as merged (`daily` = `feat/toggle-keys`, `55d0d62`), verbatim

```rust
// crates/omadeckd/src/nav.rs
#[derive(Debug, Default)]
pub struct Nav {
    /// `None` is the home page.
    current: Option<String>,
    history: Vec<Option<String>>,
}
impl Nav {
    pub fn current(&self) -> Option<&str>;
    pub fn go(&mut self, page: Option<&str>) -> bool;       // home clears, a page on the trail cuts back, else push
    pub fn parent(&self) -> Option<Option<String>>;         // history.last().cloned(); only dispatch calls it
    pub fn switch(&mut self, page: Option<&str>) -> bool;   // sideways: replace current, clear the trail
    pub fn back(&mut self) -> bool;                         // pop, or Home; false on Home
    pub fn retain_existing(&mut self, cfg: &Config) -> bool;
}
// tests: folders, loops_stay_bounded, switch_clears_the_trail, parent_is_where_back_would_go, deleted_pages

// crates/omadeckd/src/deck.rs
pub const DIAL_FLASH: Duration = Duration::from_millis(150);
pub struct Deck {
    pub dev: Arc<StreamDeck>,
    pub reader: Arc<DeviceStateReader>,
    pub kind: Kind,
    pub info: DeckInfo,
    /// Whether each dial is physically pressed down.
    pub dial_pressed: Vec<bool>,
    /// When each dial's panel was tapped (the tap time, not a deadline), while it flashes.
    pub flash_until: Vec<Option<Instant>>,
    pub dirty_panels: Vec<bool>,
    pub dirty_keys: Vec<bool>,
}
impl Deck {
    pub fn pressed(&self, i: u8, now: Instant) -> bool;     // dial held, or tapped < DIAL_FLASH ago
    pub fn flash_deadline(&self) -> Option<Instant>;
    pub fn expire_flashes(&mut self, now: Instant);
    // mark_key_dirty, mark_dirty, dirty_where
}
pub fn open(hid: &mut HidApi) -> Option<Deck>;              // builds the struct literal

// crates/omadeckd/src/daemon.rs (Daemon)
pub fn key_state(cfg: &Config, rt: &Runtime, page: Option<&str>, i: u8, pressed: bool) -> KeyStatus;
fn key_states(&self, n: u8) -> Vec<KeyStatus>;   // (0..n).map(|i| key_state(.., i, false))
pub fn next_deadline(&self) -> Option<Instant>;  // [flash, self.repaint_at, self.reload_at].min()
pub fn pass(&mut self) -> Result<(), StreamDeckError>;
fn show_page(&mut self);                         // log, paint_all, watch, publish PageChanged
fn paint_key(&mut self, i: u8, pressed: bool);
fn dispatch(&mut self, act: &Action, steps: u8) -> Result<bool>;   // fix/review-2 adds `src: Source` first
```

`pass` begins:

```rust
let now = Instant::now();
if self.repaint_at.is_some_and(|t| now >= t) {
    self.repaint_at = None;
    self.paint_all();
}
let Some(d) = &mut self.deck else { return Ok(()) };
d.expire_flashes(now);
```

The `PageStep` arm of `Daemon::dispatch`:

```rust
Action::PageStep { change } => {
    // From inside a folder, step from the folder's parent, leaving the folder.
    let in_folder =
        self.cfg.pages.iter().any(|p| p.folder && self.nav.current().is_some_and(|c| p.name == c));
    let origin =
        if in_folder { self.nav.parent().flatten() } else { self.nav.current().map(str::to_string) };
    let by = i32::from(*change) * steps as i32;
    Ok(match self.cfg.step_page(origin.as_deref(), by) {
        Some(page) => self.nav.switch(page.as_deref()),
        None => false,
    })
}
```

Core (`config.rs`): `pub struct Page { pub name: String, pub keys: Keys, pub folder: bool }`,
`Config::has_page(&self, name: &str) -> bool`, `Config::is_folder(&self, name: &str) ->
bool` (already there; don't add a second), `Config::step_page`, `Config::page(&self,
page: Option<&str>) -> Option<&Keys>`, `Keys::iter()`, `KeyConfig::action_for(&self, on:
bool) -> &Action`.

GUI (`app.rs`): `App::show_page(self: &Rc<Self>, page: Option<String>, tell_daemon:
bool)`, `App::key(&self, i: u8) -> Option<KeyConfig>` (a clone, for the page being
edited), `App::current_page(&self) -> Option<String>`, `fn sync_page_list(&self)` (fills
`page_list` with `HOME` then `cfg.page_names()`, then selects the current one), key
buttons built in `fn rebuild_grid(self: &Rc<Self>)` with
`button.connect_clicked(… app.select(Some(Selection::Key(i))))` and `self.add_dnd(..)`.
The hint under the deck is a `gtk::Label` in `App::build` reading "Click a key to edit it
· drag keys to rearrange · press a key on your deck to jump to it".

### From the plans before this one

- **fix/review-2 (R2):** `KeyConfig::actions(&self) -> Vec<&Action>`, every action a key
  owns, nested ones included: its `action`, then its `on.action`, each through
  `Action::walk` (PR 12 adds the `Multi` arm there, so a multi's steps are in the list
  too). `Config::opener` uses it.
- **fix/review-2 (R6):** `fn dispatch(&mut self, src: Source, act: &Action, steps: u8) ->
  Result<bool>`. Task 1 changes only its `PageStep` arm, whose body doesn't use `src`.
- **PR 11:** the hardware `ButtonDown(i)` and `ButtonUp(i)` arms clear `key_flash[i]`
  first; `ButtonDown` then calls `let _ = self.key_down(i, true);`. `fn key_down(&mut
  self, i: u8, publish: bool) -> Result<()>` paints pressed, publishes `KeyDown` only when
  `publish`, then calls `fn press_key(&mut self, i: u8) -> Result<()>`. Ctl's `Press` (and
  the configurator's ▶ on a key) calls `key_down(i, false)`, quiet, then sets
  `key_flash[i]`. `deck.rs` gains `pub const KEY_FLASH: Duration` (150 ms), `pub
  key_flash: Vec<Option<Instant>>` on `Deck` (the press time; `vec![None; n_keys]` in
  `open`), `Deck::expire_key_flashes(&mut self, now)` (clears the ended ones and marks
  them dirty; no `KeyUp`),
  and a `#[cfg(test)] mod tests` with `key_flashes_end_after_their_time`.
  `Deck::flash_deadline` covers `key_flash`. `Daemon::key_states` draws a flashing key
  pressed: `pressed = deck.key_flash[i].is_some()`.
- **PR 12:** `next_deadline` also lists `self.seq.next_due()`.

Nothing from PRs 9 and 10 is used.

Check these on `feat/multi-actions` before Task 2; if a name differs, the branch wins:

```bash
grep -n "fn key_down\|fn press_key\|key_flash\|KEY_FLASH\|fn next_deadline" crates/omadeckd/src/*.rs
grep -n "pub fn actions\|pub fn walk" crates/omadeck-core/src/config.rs
grep -n "fn dispatch" crates/omadeckd/src/daemon.rs
```

## Task 1: nearest origin and the config helpers (~150 lines)

**Files:** `crates/omadeck-core/src/config.rs`, `crates/omadeckd/src/nav.rs`,
`crates/omadeckd/src/daemon.rs` (the `PageStep` arm of `dispatch`).

New in `impl Config`, after `is_folder`:

```rust
/// The page holding the first key that opens `name` (any action the key owns: its
/// action, its on action, a multi step): Home first, then the pages in file order, never
/// `name` itself. `Some(None)` is Home; `None` when no key opens it.
pub fn opener(&self, name: &str) -> Option<Option<&str>> {
    let opens = |keys: &Keys| {
        keys.iter().any(|k| k.actions().into_iter().any(|a| matches!(a, Action::Page { page } if page == name)))
    };
    if opens(&self.keys) {
        return Some(None);
    }
    self.pages.iter().filter(|p| p.name != name).find(|p| opens(&p.keys)).map(|p| Some(p.name.as_str()))
}

/// Where `name` sits, for the configurator: the folders it's inside and the page they
/// hang off, outermost first, ending with `name`. A normal page is just itself.
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

(`k.actions()` is fix/review-2's walker; it returns a `Vec<&Action>`, hence `into_iter()`,
as in `Config::links_to`.)

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

The `PageStep` arm becomes:

```rust
Action::PageStep { change } => {
    // From inside a folder (however deep), step from the page the folders hang off.
    let origin = self.nav.nearest(|p| !self.cfg.is_folder(p)).map(str::to_string);
    let by = i32::from(*change) * steps as i32;
    Ok(match self.cfg.step_page(origin.as_deref(), by) {
        Some(page) => self.nav.switch(page.as_deref()),
        None => false,
    })
}
```

- Delete `Nav::parent` and its test `parent_is_where_back_would_go`. First check nothing
  else calls it: `grep -rn "\.parent()" crates/omadeckd` must show only the `dispatch`
  line you are replacing.
- **Trap:** `.map(str::to_string)` is what ends the borrow of `self.nav` before
  `self.nav.switch`. Don't keep `origin` as an `Option<&str>`.
- **Trap:** the closure `|p| !self.cfg.is_folder(p)` borrows only `self.cfg` (closures
  capture fields), so it compiles beside `self.nav.nearest`. Don't move it into a helper
  that takes `&self`.

Tests. In `nav.rs` (copy this one):

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

In `config.rs`, `page_path_follows_folder_openers`, built with `Config::parse`: Home has
nothing; page Music (normal) has a key opening Media; Media (`folder = true`) has a key
opening Spotify and a Back key; Spotify (`folder = true`) has a key opening Media. Assert
`page_path("Spotify") == ["Music", "Media", "Spotify"]`, `page_path("Music") ==
["Music"]`, `opener("Media") == Some(Some("Music"))`, and a folder page nothing opens is
just itself. Add one key on Home whose `[key.on]` action opens a page `Lone`
(`folder = true`): `opener("Lone") == Some(None)` (the on action counts).

**Check:**

```bash
cargo test -p omadeckd nav:: && cargo test -p omadeck-core page_path && cargo clippy --all-targets -- -D warnings
```

→ the `nav::` run ends `test result: ok.` with `0 failed` and lists
`nav::tests::nearest_skips_nested_folders ... ok` (and no `parent_is_where_back_would_go`);
the core run shows `test result: ok. 1 passed`; clippy prints no warnings.

## Task 2: held keys and hold Back for Home (~180 lines)

**Files:** `crates/omadeckd/src/nav.rs`, `crates/omadeckd/src/deck.rs`,
`crates/omadeckd/src/daemon.rs` (`handle_update`'s `ButtonDown`/`ButtonUp` arms,
`key_states`, `next_deadline`, `pass`, `show_page`).

In `nav.rs`:

```rust
/// Go straight Home and clear the trail (holding Back). Returns whether anything changed.
pub fn home(&mut self) -> bool {
    self.go(None)
}
```

In `deck.rs`:

```rust
/// How long a Back key is held before it goes Home instead (Elgato's "hold to exit";
/// its time isn't documented).
pub const BACK_HOLD: Duration = Duration::from_millis(500);

/// A key whose press would run `Back` that's down. Back waits for the release, so
/// holding it can go Home instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BackHold {
    pub key: u8,
    pub since: Instant,
}

impl BackHold {
    /// When holding turns into "go Home".
    pub fn deadline(&self) -> Instant {
        self.since + BACK_HOLD
    }
}
```

`Deck` gains, after `dial_pressed`:

```rust
/// Whether each key is physically held down (ButtonDown to ButtonUp).
pub key_pressed: Vec<bool>,
/// The Back key being held, until it's released or goes Home.
pub back_held: Option<BackHold>,
```

`deck::open` sets `key_pressed: vec![false; n_keys]` and `back_held: None`. New method,
next to `pressed`:

```rust
/// Whether key `i` is drawn pressed: held down, or still flashing after a ctl press
/// (PR 11's `key_flash`; `expire_key_flashes` clears it when it ends).
pub fn key_held(&self, i: u8) -> bool {
    let n = i as usize;
    let flashed = self.key_flash.get(n).is_some_and(Option::is_some);
    self.key_pressed.get(n).copied().unwrap_or(false) || flashed
}
```

In `daemon.rs`:

- A pure helper, beside `key_state`:

  ```rust
  /// Whether a press of `key` waits for its release: the action `press_key` would run
  /// (the on action while on) is `Back`.
  fn waits_for_release(key: Option<&KeyConfig>, on: bool) -> bool {
      key.is_some_and(|k| matches!(k.action_for(on), Action::Back))
  }
  ```

- `key_states` reads the held state (replacing PR 11's `pressed =
  deck.key_flash[i].is_some()`, which `key_held` now covers):

  ```rust
  fn key_states(&self, n: u8) -> Vec<KeyStatus> {
      (0..n)
          .map(|i| {
              let held = self.deck.as_ref().is_some_and(|d| d.key_held(i));
              key_state(&self.cfg, &self.rt, self.nav.current(), i, held)
          })
          .collect()
  }
  ```

- New `fn set_key_pressed(&mut self, i: u8, pressed: bool)`, written like
  `set_dial_pressed` but without `mark_dirty` (the arms paint the key themselves).
- **`ButtonDown(i)`:** keep PR 11's `key_flash` clear first. Then:

  ```rust
  self.set_key_pressed(i, true);
  let page = self.nav.current();
  let on = key_state(&self.cfg, &self.rt, page, i, false).state == Toggle::On;
  if waits_for_release(self.cfg.key(page, i), on) {
      // Back waits for the release (a hold goes Home instead); show the press now.
      self.paint_key(i, true);
      self.hub.publish(Event::KeyDown { index: i, page: self.nav.current().map(str::to_string) });
      if let Some(d) = &mut self.deck {
          d.back_held = Some(BackHold { key: i, since: Instant::now() });
      }
  } else {
      let _ = self.key_down(i, true);
  }
  ```

  `on` is a `bool` and `waits_for_release` returns a `bool`, so no borrow of `self.cfg`
  or `self.nav` is alive when `paint_key` takes `&mut self`. Only the hardware arm
  changes: ctl's `Press` still calls `key_down(i, false)`, so it runs Back at once.
- **`ButtonUp(i)`:** after PR 11's `key_flash` clear, `self.set_key_pressed(i, false)`,
  then the existing `paint_key(i, false)` and `KeyUp`, then:

  ```rust
  let held = self.deck.as_mut().and_then(|d| d.back_held.take_if(|h| h.key == i));
  if held.is_some() {
      let _ = self.press_key(i); // a tap: Back, through the normal press path
  }
  ```

- **Hold:** in `pass`, right after the `repaint_at` block and before `let Some(d) = &mut
  self.deck`:

  ```rust
  if let Some(h) = self.deck.as_mut().and_then(|d| d.back_held.take_if(|h| now >= h.deadline())) {
      log::info!("key {}: held Back, going Home", h.key);
      if self.nav.home() {
          self.show_page();
      }
  }
  ```

  The release that follows finds `back_held` empty and only repaints the key.
- **`next_deadline`:** add `self.deck.as_ref().and_then(|d| d.back_held).map(|h|
  h.deadline())` to its list (beside the flashes, `repaint_at`, `reload_at` and PR 12's
  `seq.next_due()`). Without it the loop can sleep 100 ms past the hold.
- **`show_page`:** first thing, forget held keys:

  ```rust
  if let Some(d) = &mut self.deck {
      d.key_pressed.fill(false);
      d.back_held = None;
  }
  ```

Tests (BUGS.md 65's fix is the `key_held`/`key_states` change, checked on the deck) in
`daemon.rs` (copy this one), plus `back_hold_deadline` in `deck.rs`'s tests (PR
11 made the module; `BackHold { key: 0, since: t }.deadline() == t + BACK_HOLD`), and
`home_clears_a_nested_trail` in `nav.rs` (A → F1 → F2, `home()` is true, `current()` is
`None`, `history` empty, a second `home()` is false).

```rust
#[test]
fn only_a_back_press_waits_for_release() {
    let cfg = Config::parse(
        "[[key]]\nindex = 0\n[key.action]\ntype = \"back\"\n\n\
         [[key]]\nindex = 1\n[key.action]\ntype = \"page\"\npage = \"A\"\n\n\
         [[key]]\nindex = 2\n[key.action]\ntype = \"app\"\ncommand = \"true\"\n[key.on]\n[key.on.action]\ntype = \"back\"\n",
    )
    .unwrap();
    assert!(waits_for_release(cfg.key(None, 0), false), "a Back key");
    assert!(!waits_for_release(cfg.key(None, 1), false), "Open page acts on key-down");
    assert!(!waits_for_release(cfg.key(None, 2), false), "a toggle runs its off action while off");
    assert!(waits_for_release(cfg.key(None, 2), true), "and its Back on action while on");
    assert!(!waits_for_release(None, false), "no key");
}
```

**Check:**

```bash
cargo test -p omadeckd -- waits_for_release back_hold home_clears && cargo clippy --all-targets -- -D warnings
```

→ `test result: ok. 3 passed; 0 failed`, no clippy warnings. Then `cargo test -p
omadeckd` passes in full, and the first four hardware checks.

**Traps:**
- BUGS.md 4: don't call `self.key_down`, `self.press_key` or `self.show_page` while a
  `&self.cfg` or `&mut self.deck` borrow is alive. The code above takes `bool`s and the
  `BackHold` (it's `Copy`) out first.
- Don't call `key_down` for a Back key and then try to undo it: `key_down` runs the
  action.
- `key_pressed` is input state. It must not go in `Runtime` (a replug must reset it).

## Task 3: double-click in the configurator, docs (~120 lines)

**Files:** `crates/omadeck-gui/src/app.rs`, `README.md`, `CHANGELOG.md`.

- In `rebuild_grid`, after `button.connect_clicked(…)`, add a double-click gesture. It
  runs in the capture phase, so the button's own click (select) still happens:

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

- New method, in the `// --- pages` section:

  ```rust
  /// Follow key `i`: an Open page key shows its page, a Back key the page that opens
  /// this one (Home if none). The deck follows, as with the page switcher.
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
  bare name (collect the labels into a `Vec<String>`, then take `&str`s for `splice`).
  The order is unchanged, so the dropdown's index → page mapping in
  `connect_selected_notify` (`cfg.pages.get(n - 1)`) still works.
- The hint under the deck: append ` · double-click a folder to open it`.
- **Trap:** keep each `self.config.borrow()` a temporary inside its expression, as above.
  `show_page` borrows `config` again and `sync_page_list` borrows `page` mutably further
  down; a `let cfg = self.config.borrow();` held across `show_page` panics.

Docs: README "Pages and folders": "hold a Back key to go straight Home; double-click a
folder in the configurator to open it". CHANGELOG under Unreleased → Added (hold Back,
double-click a folder) and Fixed (page steps from nested folders; a held key stays drawn
pressed, BUGS.md 65). After the branch, mark BUGS.md 65 fixed with Task 2's commit.

Test: GUI code has no unit tests; `page_path` and `opener` are covered in Task 1. Copy
Task 1's `nearest_skips_nested_folders` pattern if you add a core helper.

**Check:** `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → all
green, no warnings. Then the configurator hardware checks.

## Capture on the machine first

Nothing: this PR reads no system state.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Tap Back in a folder: goes up one level on release.
- [ ] Hold Back ~0.5 s two folders deep: the deck goes Home while the key is still held;
      releasing does nothing more.
- [ ] A Back key on a normal page (not a folder) works the same.
- [ ] Hold a toggle key (e.g. the mic mute): its face flips to the new state and stays
      drawn pressed until you let go (BUGS.md 65).
- [ ] Two folders deep (Music → Media → Spotify), flick left: the deck shows the page
      after Music, not the page after Home.
- [ ] `omadeckd press 0` (PR 11) on a Back key goes back at once.
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
