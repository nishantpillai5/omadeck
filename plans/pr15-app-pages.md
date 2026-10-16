# PR 15: `feat/app-pages`: switch page when an app is focused

Part of [PLAN.md](PLAN.md). Base: `feat/media-dial` (PR 14). Needs the **Hyprland event
listener** from PR 9 (`omadeckd/src/hypr.rs`, `HyprEvent::ActiveWindow`), `Nav::nearest`
from PR 13, `Config::is_folder` from fix/review, and groundwork's `Daemon` struct and
`Daemon::next_deadline` (D3/D4). Works without PR 11 (ctl); a ctl page change is just
another manual move.

**Outcome:** a page can follow apps. While a linked app is focused, the deck shows that
page. When focus moves to an app that isn't linked, the deck goes back to where it was,
open folder included. This is Elgato's **Smart Profiles**, with omadeck's pages in place
of Elgato's profiles. You set it up with `apps = ["spotify"]` on upstream's `[[page]]`
table, or with **Follow apps** in the configurator's page settings. Configs without
`apps` load and save byte-identical.

## How the Elgato app does it (researched 2026-10-16, trimmed)

- **Linking:** Preferences → Profiles, pick a profile, choose an app from its
  **Application** drop-down. "Stream Deck will automatically switch to the Smart Profile
  once the application is detected as the focused window."
  ([Smart Profiles][sp], [how to set up][howto])
- **Paused while editing:** "When the software is open, Smart Profiles are deactivated to
  prevent profile switching while editing the Stream Deck configuration." ([sp])
- **How apps are identified:** the executable name on Windows, `CFBundleIdentifier` on
  macOS ([app-monitoring]). On Linux the equivalent is the Hyprland window class.
- **Unconfirmed** (not in the docs): leaving the app goes back to the profile that was
  active before; one app per profile; a manual switch while the app is focused stays until
  focus changes; which of two profiles linked to one app wins.

[sp]: https://www.elgato.com/us/en/explorer/products/stream-deck/smart-profiles-stream-deck/
[howto]: https://www.elgato.com/eu/et/explorer/products/stream-deck/how-to-set-up-smart-profiles-on-stream-deck/
[app-monitoring]: https://docs.elgato.com/streamdeck/sdk/guides/app-monitoring

## Decided

- **Decided: `apps` on upstream's `Page`,** not a new table. Upstream's `[[page]]` already
  has a name, so the old plan's `PageConfig { index, name, apps }` and optional names are
  gone.
- **Decided: Home can't follow apps.** Home is the top-level `[[key]]`s and has no table.
  Leaving linked apps often lands on Home anyway.
- **Decided: folders can't follow apps.** `Config::app_page` skips `folder = true` pages,
  and the GUI greys the setting out on them. A folder is reached from a key; switching
  into it sideways would leave its Back key pointing at Home.
- **Decided: `Nav::switch` to the linked page,** so Back doesn't walk an app-switch trail
  (the rebase's rule for sideways moves).
- **Decided: leaving restores the whole `Nav`** saved before the first linked app took
  over: the page, an open folder and its trail. `Nav::retain_existing` drops anything a
  reload removed meanwhile. This replaces the old plan's "pop to root".
- **Decided: a manual move sticks, detected by position.** No `manual()` hook in every
  page path. On leaving, the deck goes back only if it is still on the linked page or in
  a folder opened from it (`Nav::nearest(|p| !cfg.is_folder(p)) == link`). A flick, a
  page key, the configurator's switcher and PR 11's ctl all count as manual with no extra
  code.
- **Decided: A → B → leave returns to before A.** Moving from one linked app to another
  keeps the saved place, unless you moved by hand in between (then the place you moved to
  is saved).
- **Decided: the first matching page in file order wins** when two pages list the same app.
- **Decided: match on window class,** ASCII case-insensitive, `*` matches any run. The
  glob covers Chromium web apps (`chrome-<host>__<path>-Default`).
- **Decided: 200 ms settle** (`APP_FOCUS_SETTLE`). Only the newest focus event counts, so
  an alt-tab through four windows makes one decision.
- **Decided: the configurator's window is ignored** (`CONFIGURATOR_CLASS`). Same result as
  Elgato's pause ("editing never jumps"), without pausing while the window sits on another
  workspace.
- **Decided: following works with the deck unplugged.** `Nav` and the follow state live in
  `Daemon`, not `Deck`, so a deck plugged in while Spotify is focused shows the Spotify
  page.
- **Decided: no new IPC.** `PageChanged` already moves the configurator.

## Ask before building

1. A manual page change while a linked app is focused sticks when you leave (planned). Or
   should leaving always return?
2. 200 ms settle before switching: OK, or shorter?

## Before building: check against the base

PR 9 and groundwork weren't merged when this plan was written. Check these on
`feat/media-dial` and use the branch's names if they differ:

```bash
grep -n "pub enum HyprEvent\|pub struct ActiveWindow\|pub fn start" crates/omadeckd/src/hypr.rs
grep -n "HyprEvent::ActiveWindow\|rt.window\|fn next_deadline\|fn show_page\|retain_existing" crates/omadeckd/src/*.rs
grep -n "pub fn nearest\|pub fn is_folder" crates/omadeckd/src/nav.rs crates/omadeck-core/src/config.rs
grep -rn "Page { name" crates
```

Expected (PR 9's plan, PR 13, fix/review, `d7bd448`):

```rust
// crates/omadeckd/src/hypr.rs (PR 9)
/// The focused window, as `j/activewindow` reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveWindow {
    /// Without a `0x` prefix, lowercase, so event and JSON forms compare equal.
    pub address: String,
    pub class: String,
    pub title: String,
    pub pid: Option<u32>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HyprEvent {
    /// The focused workspace changed, moved monitor, or was renamed.
    Workspace { id: i64, name: String },
    /// The focused window changed or was retitled; `None` when nothing has focus.
    ActiveWindow(Option<ActiveWindow>),
}
/// Starts the `hypr` thread. `None` outside Hyprland (no thread).
pub fn start() -> Option<mpsc::Receiver<HyprEvent>>;
// daemon.rs: Daemon: `hypr: Option<mpsc::Receiver<HyprEvent>>`; Runtime: `window: Option<hypr::ActiveWindow>`.
// `Daemon::apply_hypr(&mut self)` drains `hypr` with try_recv; main.rs's run() calls it every
// iteration right after `apply_values`, before the no-deck branch;
// its `ActiveWindow(w)` arm sets `rt.window = w` and sends PR 10's `WatchCmd::Focus(w)` ("PR 10 and 15 hook in here").
// A title change arrives as ActiveWindow(Some) with the same class.

// crates/omadeckd/src/nav.rs (PR 13)
#[derive(Debug, Default)]
pub struct Nav { current: Option<String>, history: Vec<Option<String>> }
pub fn nearest(&self, keep: impl Fn(&str) -> bool) -> Option<&str>;
pub fn switch(&mut self, page: Option<&str>) -> bool;
pub fn retain_existing(&mut self, cfg: &Config) -> bool;

// crates/omadeck-core/src/config.rs (upstream + PR 5 + fix/review)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    #[serde(default, rename = "key", skip_serializing_if = "Keys::is_empty")]
    pub keys: Keys,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub folder: bool,
}
pub fn is_folder(&self, name: &str) -> bool;
```

## Config

```toml
[[page]]
name = "Music"
apps = ["spotify", "chrome-music.youtube.com*"]
```

## Task 1: core: `apps`, matching, open windows (~170 lines)

**Files:** `crates/omadeck-core/src/config.rs`, new `crates/omadeck-core/src/follow.rs`,
`crates/omadeck-core/src/lib.rs` (`pub mod follow;`), every `Page { … }` literal.

`Page` gains, after `folder`:

```rust
/// Show this page while a window of one of these apps is focused (Elgato's Smart
/// Profiles). Hyprland window classes, case-insensitive; `*` matches any run. Ignored on
/// folders.
#[serde(default, skip_serializing_if = "Vec::is_empty")]
pub apps: Vec<String>,
```

New in `impl Config`, next to `is_folder`:

```rust
/// The page to show while a window of `class` is focused: the first non-folder page, in
/// file order, with a matching `apps` pattern.
pub fn app_page(&self, class: &str) -> Option<&str> {
    self.pages
        .iter()
        .filter(|p| !p.folder)
        .find(|p| p.apps.iter().any(|a| crate::follow::class_matches(a, class)))
        .map(|p| p.name.as_str())
}
```

`follow.rs`:

```rust
//! Pages that follow the focused app: window-class matching and the open-window list.

use serde::Deserialize;

/// The configurator's window class (its GTK application id). The daemon ignores focus on
/// it, so editing never moves the deck.
pub const CONFIGURATOR_CLASS: &str = "dev.omadeck.Omadeck";

/// Whether a Hyprland window class matches `pattern`: ASCII case-insensitive, `*` matches
/// any run. An empty pattern or class never matches.
pub fn class_matches(pattern: &str, class: &str) -> bool {
    let (p, c) = (pattern.trim().to_ascii_lowercase(), class.to_ascii_lowercase());
    if p.is_empty() || c.is_empty() {
        return false;
    }
    let parts: Vec<&str> = p.split('*').collect();
    if parts.len() == 1 {
        return p == c;
    }
    let Some(mut rest) = c.strip_prefix(parts[0]) else { return false };
    for mid in &parts[1..parts.len() - 1] {
        match rest.find(mid) {
            Some(i) => rest = &rest[i + mid.len()..],
            None => return false,
        }
    }
    rest.ends_with(parts[parts.len() - 1])
}

/// `(class, title)` for each open window in `hyprctl -j clients` output, one per class,
/// sorted. Empty classes are dropped; anything unparsable gives an empty list.
pub fn parse_clients(json: &str) -> Vec<(String, String)> {
    #[derive(Deserialize)]
    struct Client {
        #[serde(default)]
        class: String,
        #[serde(default)]
        title: String,
    }
    let mut v: Vec<(String, String)> = serde_json::from_str::<Vec<Client>>(json)
        .unwrap_or_default()
        .into_iter()
        .filter(|c| !c.class.is_empty())
        .map(|c| (c.class, c.title))
        .collect();
    v.sort();
    v.dedup_by(|a, b| a.0 == b.0);
    v
}
```

- **Trap:** every `Page { name, keys, folder }` literal (config tests, `nav.rs` tests,
  PR 13's tests, fix/review's `Config::add_page`) needs `apps: Vec::new()`. The grep above lists them.
- `rename_page`/`remove_page` need nothing: `apps` lives on the page itself.

Tests in `follow.rs` (copy this one): `class_matches_globs`, plus
`parse_clients_dedups_and_sorts` (two Spotify windows and a kitty window → two pairs;
`"not json"` → empty). In `config.rs`: `follow_apps_omitted_when_empty` (a page without `apps`
saves without the word), `follow_app_page_skips_folders_and_takes_first` (a folder
listing `spotify` and two pages listing it: the first non-folder wins).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn class_matches_globs() {
        assert!(class_matches("spotify", "Spotify"), "case-insensitive");
        assert!(!class_matches("spot", "spotify"), "no star: the whole class");
        assert!(class_matches("chrome-*-Default", "chrome-music.youtube.com__-Default"));
        assert!(class_matches("*spotify*", "com.spotify.Client"));
        assert!(class_matches("*", "kitty"));
        assert!(!class_matches("a*b*a", "ab"), "parts can't overlap");
        assert!(!class_matches("", "kitty") && !class_matches("*", ""), "empty never matches");
    }
}
```

**Check:** `cargo test -p omadeck-core follow && cargo clippy --all-targets -- -D warnings`
→ `test result: ok. 4 passed` (two in `follow::tests`, two `config::tests::follow_*`),
no clippy warnings.

## Task 2: daemon: `AppFollow` and the focus wiring (~260 lines)

**Files:** new `crates/omadeckd/src/follow.rs` (`mod follow;` in `main.rs`),
`crates/omadeckd/src/nav.rs` (derive), `crates/omadeckd/src/daemon.rs` (`Daemon`).

`Nav`'s derive becomes `#[derive(Debug, Default, Clone, PartialEq)]`.

`follow.rs`:

```rust
//! Pages that follow apps (Elgato's Smart Profiles, on pages).

use omadeck_core::Config;

use crate::nav::Nav;

#[derive(Debug, Default)]
pub struct AppFollow {
    /// The page the focused app is linked to, if any.
    link: Option<String>,
    /// Where the deck was before a linked app took it; put back on leaving.
    back: Option<Nav>,
}

impl AppFollow {
    /// Focus moved to an app linked to `link` (`None`: not linked, or nothing focused).
    /// Moves `nav` if it should; returns whether the deck needs repainting.
    pub fn focus(&mut self, link: Option<&str>, nav: &mut Nav, cfg: &Config) -> bool {
        if link == self.link.as_deref() {
            return false;
        }
        // Still where we put it: on the linked page, or in a folder opened from it.
        let ours = self.link.is_some() && nav.nearest(|p| !cfg.is_folder(p)) == self.link.as_deref();
        match link {
            Some(page) => {
                if !ours {
                    self.back = Some(nav.clone());
                }
                self.link = Some(page.to_string());
                nav.switch(Some(page))
            }
            None => {
                self.link = None;
                match self.back.take() {
                    Some(saved) if ours => {
                        let before = nav.clone();
                        *nav = saved;
                        nav.retain_existing(cfg);
                        *nav != before
                    }
                    _ => false,
                }
            }
        }
    }
}
```

`Daemon` gains (all outlive a replug):

```rust
/// Hyprland sends a focus event for every window an alt-tab passes; act once focus has
/// stayed put this long.
const APP_FOCUS_SETTLE: Duration = Duration::from_millis(200);

follow: AppFollow,
/// The focused window's class once it settled; `""` when nothing has focus.
focus_class: Option<String>,
/// The newest focus change and when it settles.
focus_pending: Option<(String, Instant)>,
```

- Imports: `use omadeck_core::{HOME, follow::CONFIGURATOR_CLASS};` and `use follow::AppFollow;`.
- **In PR 9's `HyprEvent::ActiveWindow(w)` arm** of `apply_hypr` (where it sets `rt.window = w`), first
  `let class = w.as_ref().map(|w| w.class.clone()).unwrap_or_default();`, then PR 9's and
  PR 10's handling as they are, then, unless
  `class.eq_ignore_ascii_case(CONFIGURATOR_CLASS)`,
  `self.focus_pending = Some((class, Instant::now() + APP_FOCUS_SETTLE));`.
- **`Daemon::next_deadline()`** includes `self.focus_pending.as_ref().map(|(_, at)| *at)`.
- **Each pass**, before the no-deck branch: at the end of `apply_hypr` (after its drain
  loop; `run()` calls it every iteration), with `let now = Instant::now();`:
  `if let Some((class, _)) = self.focus_pending.take_if(|(_, at)| now >= *at) { self.focus_class = Some(class); self.apply_focus(); }`.
- New method:

```rust
/// Show the page the focused app is linked to, or go back when it isn't linked.
fn apply_focus(&mut self) {
    let Some(class) = self.focus_class.clone() else { return };
    let link = self.cfg.app_page(&class).map(str::to_string);
    if self.follow.focus(link.as_deref(), &mut self.nav, &self.cfg) {
        let page = self.nav.current().unwrap_or(HOME).to_string();
        match &link {
            Some(_) => log::info!("page {page} (follows {class})"),
            None => log::info!("page {page} (left {class})"),
        }
        self.show_page();
    }
}
```

- **Reload:** call `self.apply_focus()` right after the reload's `nav.retain_existing`
  and its `PageChanged`. A link added or removed in the GUI takes effect without a focus
  change; an unchanged link is a no-op.
- **Connect / disconnect:** nothing. `Nav` survives the replug, so the deck comes up on
  the followed page.
- **Trap:** `apply_focus` borrows `self.follow`, `self.nav` and `self.cfg` as separate
  fields in one call. That compiles only as written, inside the method body. Don't move
  it into a helper that takes `&mut self` and `&self.cfg`. And `class` and `link` are
  owned `String`s so `self.show_page()` can borrow `self` mutably.

Tests in `follow.rs`: copy this one, then add `a_then_b_then_leave_returns_to_before_a`,
`manual_move_sticks` (follow Music, `nav.switch(Some("Work"))`, leave → stays on Work,
returns `false`), `folder_opened_from_linked_page_still_returns` (follow Music,
`nav.go(Some("F"))`, leave → back), `manual_move_then_b_saves_new_place`,
`same_link_is_noop`.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use omadeck_core::{Keys, Page};

    fn cfg() -> Config {
        let page =
            |name: &str, folder: bool| Page { name: name.into(), keys: Keys::default(), folder, apps: Vec::new() };
        let pages = vec![page("Work", false), page("Music", false), page("Chat", false), page("F", true)];
        Config { pages, ..Config::default() }
    }

    #[test]
    fn enter_and_leave_restores_the_folder() {
        let cfg = cfg();
        let (mut f, mut nav) = (AppFollow::default(), Nav::default());
        nav.go(Some("Work"));
        nav.go(Some("F"));
        assert!(f.focus(Some("Music"), &mut nav, &cfg));
        assert_eq!(nav.current(), Some("Music"));
        assert!(!f.focus(Some("Music"), &mut nav, &cfg), "another window of the same app");
        assert!(f.focus(None, &mut nav, &cfg));
        assert_eq!(nav.current(), Some("F"), "back where it was, folder and all");
        assert!(nav.back());
        assert_eq!(nav.current(), Some("Work"), "and the folder's Back still works");
    }
}
```

**Check:** `cargo test -p omadeckd follow:: && cargo clippy --all-targets -- -D warnings`
→ `test result: ok. 6 passed`, no clippy warnings. Then the first three hardware checks.

## Task 3: GUI "Follow apps", docs (~260 lines)

**Files:** `crates/omadeck-gui/src/main.rs`, `crates/omadeck-gui/src/app.rs`, new
`crates/omadeck-gui/src/pages.rs`, `README.md`, `examples/config.toml`, `CHANGELOG.md`.

- `main.rs`: `pub const APP_ID: &str = omadeck_core::follow::CONFIGURATOR_CLASS;` so the
  two can't drift.
- **Page settings button.** In `App::new`, a third button in `page_actions` (shown only
  for a page, never Home): icon `emblem-system-symbolic`, tooltip "Page settings", opens
  `pages::show(&app, &name)` for `current_page()`. If fix/review already added a page
  settings dialog for the Folder switch (`grep -n '"Folder' crates/omadeck-gui/src/app.rs`),
  add the group below to that dialog instead of making a second one.
- New `App` method, the one way the dialog edits:

```rust
/// Edit the `apps` of page `name`; false (and a toast) if the page is gone.
pub fn edit_page_apps(self: &Rc<Self>, name: &str, f: impl FnOnce(&mut Vec<String>)) -> bool {
    let mut cfg = self.config.borrow_mut();
    let Some(p) = cfg.pages.iter_mut().find(|p| p.name == name) else {
        drop(cfg);
        self.toast("That page is gone");
        return false;
    };
    f(&mut p.apps);
    drop(cfg);
    self.sync_page_list();
    self.schedule_save();
    true
}
```

- `pages.rs`: `pub fn show(app: &Rc<App>, page: &str)` presents an
  `adw::PreferencesDialog` titled with the page name, with one group **Follow apps**,
  description "Show this page while one of these apps is focused, and go back to where
  you were when you leave it." On a folder page the group is insensitive and the
  description reads "Folders can't follow apps: they open from a key."
  - One `adw::ActionRow` per pattern (title = the pattern, subtitle "Window class"), with
    a flat `user-trash-symbolic` button that removes it.
  - `adw::ButtonRow` **Add app…** → `apps::pick(&dialog, …)`. `pick` hands back a
    `gio::AppInfo`; the class is `info.downcast_ref::<gio::DesktopAppInfo>().and_then(|d| d.startup_wm_class())`,
    else `info.id()` without `.desktop`. That covers GTK and Flatpak apps (class = app id)
    and most Electron apps (case-insensitive matching covers `Spotify`/`spotify`).
  - `adw::ButtonRow` **Add open window…** → runs `hyprctl -j clients` off the UI thread:
    `gio::spawn_blocking(|| omadeck_core::proc::output_timeout(Command::new("hyprctl").args(["-j", "clients"]), Duration::from_secs(2)))`,
    awaited in `glib::spawn_future_local`, parsed with `follow::parse_clients`, shown as an
    `adw::AlertDialog` with a `boxed-list` of `adw::ActionRow`s (title = class, subtitle =
    title). Activating a row adds the class and closes it. An empty list toasts "No open
    windows found". This is the reliable path for web apps and anything whose desktop
    entry doesn't name its class. (A one-off read when the user asks, like `apps::pick`
    reading desktop files; D2's "only the daemon polls" is about live state.)
  - `adw::EntryRow` **Window class** with `show_apply_button(true)`; `connect_apply` adds
    the trimmed text. Its hint: "find it with `hyprctl activewindow`".
  - After each add or remove, rebuild the rows. Ignore a pattern already in the list.
- `sync_page_list`: a page with `apps` gets ` · follows apps` after its label (PR 13 made
  the labels `page_path(..).join(" › ")`).
- **Traps:** the rows-rebuild function is called from the row buttons it creates, so it's
  an `Rc<dyn Fn()>` (built once, cloned into each handler), not an `impl Fn`. And
  `edit_page_apps` must not run while anything holds `app.config.borrow()`: read the
  page's `apps` into a `Vec<String>` first, drop the borrow, then build rows.
- Docs: README feature bullet ("Pages can follow apps, like Elgato's Smart Profiles") and
  roadmap line `- [x] **App pages**`; a commented `apps = […]` in a `[[page]]` of
  `examples/config.toml`, with the `hyprctl activewindow` hint and a glob; CHANGELOG under
  Unreleased → Added.

**Check:** `cargo test && cargo clippy --all-targets -- -D warnings` → all tests pass
(`example_parses` covers the example), no warnings. Then the GUI hardware checks.

## Capture on the machine first

Paste each into this plan before Task 1 (or a fixture file) if it differs from what's
assumed:

- The configurator's class: run `omadeck`, focus it, `hyprctl activewindow | grep class`.
  Assumed `dev.omadeck.Omadeck`.
- Real classes for the apps you'll link: with Spotify, an Omarchy web app and a TUI app
  open, `hyprctl -j clients | jq -r '.[] | "\(.class)\t\(.title)"'`. Save the raw
  `hyprctl -j clients` as `crates/omadeck-core/tests/fixtures/hyprctl-clients.json` and
  add a `parse_clients` test on it.
- Whether Walker (or any layer-shell launcher) sends focus events:
  `socat -U - UNIX-CONNECT:$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock | grep activewindow`,
  then open and close Walker.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Link Music to Spotify. Focusing Spotify switches the deck; focusing the browser
      returns to the page it was on.
- [ ] Alt-tab quickly through several windows: the deck changes at most once.
- [ ] Flick to another page while Spotify is focused, then leave Spotify: the deck stays.
      Focus Spotify again: it switches again.
- [ ] Inside a folder on Work, focus Spotify, then leave: back in the folder, and its Back
      key goes to Work.
- [ ] Open a folder from Music while Spotify is focused, then leave: back to before.
- [ ] Spotify's popups and menus don't bounce the deck.
- [ ] Closing the last window on a workspace returns the deck.
- [ ] Focusing the configurator never switches the deck; its page switcher follows the
      deck's automatic switches.
- [ ] Unplug, focus Spotify, plug in: the deck comes up on Music.
- [ ] Add a link in the GUI while the app is focused: the deck switches on reload.
- [ ] An Omarchy web app added with **Add open window…** matches.
- [ ] A config without `apps` saves byte-identical.

## Decisions to explain

- **Pages, not profiles.** omadeck has one layout with pages. A page that follows apps
  gives the same result as a Smart Profile without adding profiles.
- **Several apps per page** (`apps` is a list). Elgato's drop-down takes one app
  (unconfirmed); a list costs nothing.
- **Window class with a `*` glob,** Hyprland's equivalent of Elgato's executable or
  bundle id.
- **Return to where you were,** open folder included. Matches what Elgato is reported to
  do (unconfirmed).
- **A manual change sticks,** detected by where the deck is rather than by hooking every
  page path (unconfirmed for Elgato).
- **Folders and Home can't follow apps** (see Decided).
- **200 ms settle.** Hyprland sends an event per focus step; without it an alt-tab would
  flicker the keys.
- **The configurator's window is ignored** rather than pausing the feature while it's
  open.
- **No new IPC event.**

## Later

- Matching on window title too (TUI apps inside a terminal, browser tabs).
- The app's icon in the page switcher, as Elgato shows it next to the profile.
- `omadeckd follow on|off` (PR 11's ctl) to pause following.
- Per-deck links, once multiple decks exist.

**Size:** about 650 lines: core ~170 with tests, daemon ~230 with tests, GUI ~220, docs
~30.
