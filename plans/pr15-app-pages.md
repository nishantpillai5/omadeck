# PR 15: `feat/app-pages`: switch page when an app is focused

Part of [PLAN.md](PLAN.md). Base: `feat/media-dial` (PR 14). Needs the **Hyprland
event listener** from PR 9 (`hypr.rs`, `HyprEvent::ActiveWindow`) and the folder stack
from PR 13. It works without PR 11 (ctl), but treats a ctl `page` the same as a page key.

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Rewrite against the code as merged (upstream's named pages: `Option<&str>` / `Config::key(page, index)` / `Nav`), not `ba0eced`'s `page: u8`.
- [ ] Replace every "assumption about PR N" with that PR's real, merged signatures, copied in full.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.
- [ ] Upstream already has `[[page]]` (`Page { name, keys }`). Drop `PageConfig`, `index`, `name` and `page_config_mut`; add `#[serde(default, skip_serializing_if = "Vec::is_empty")] apps: Vec<String>` to upstream's `Page`. Decide whether folder pages can be linked (probably not).
- [ ] Daemon: `Deck.page`, `do_page` and `set_page` are gone. Define the target type explicitly (e.g. `enum Target { Home, Page(String) }`), switch with `Nav::switch` (rebase step 2), and name where `manual()` hooks into the flick, `PageStep` and Open-page paths.
- [ ] GUI: no chips after the rebase; put the setting by upstream's page dropdown.
- [ ] `apps::pick` returns a `gio::AppInfo`; `startup_wm_class()` needs `downcast_ref::<gio::DesktopAppInfo>()`. Say so.
- [ ] Copy PR 9's `HyprListener` / `HyprEvent::ActiveWindow` definitions in.
- [ ] Redo the Folders decision and its hardware check: folders don't sit under a numbered root with `Nav`.

**Outcome:** a page can follow apps. While a linked app is focused, the deck shows that
page. When focus moves to an app that isn't linked, the deck goes back to the page it was
on before. This is Elgato's **Smart Profiles**, with omadeck's pages in place of Elgato's
profiles. You set it up in a small `[[page]]` table (`apps = ["spotify", "obs"]`), or with
a **Follow apps** setting next to the page chips in the configurator. The same table also
gives a page an optional name. Configs without `[[page]]` load and save byte-identical.

## How the Elgato app does it (researched 2026-10-16)

- **Linking:** in Preferences → Profiles you pick a profile and choose an app from its
  **Application** drop-down. The app's icon then shows next to the profile. "Stream Deck
  will automatically switch to the Smart Profile once the application is detected as the
  focused window." ([Elgato Explorer: Smart Profiles][sp],
  [How to set up Smart Profiles][howto])
- **Paused while editing:** "When the software is open, Smart Profiles are deactivated to
  prevent profile switching while editing the Stream Deck configuration." The Stream Deck
  window has to be minimised for Smart Profiles to work. ([sp])
- **All models,** the Stream Deck + included ([howto]). Profiles belong to one device
  (SDK: "shareable layouts, specific to a Stream Deck device" ([profiles])), so a Smart
  Profile works per device.
- **How apps are identified:** the executable name on Windows and `CFBundleIdentifier` on
  macOS ([app-monitoring]). The Linux equivalent is the Hyprland window class.
- **Not in the docs, so unconfirmed:**
  - *Leaving the app.* The app is widely reported to go back to the profile that was
    active before (the last profile with no app linked), not to a fixed default.
  - *One app per profile.* The drop-down takes a single app.
  - *Manual switches.* A manual profile switch (the *Switch Profile* key) while the app is
    focused stays until focus changes again.
  - *Two profiles linked to one app.* Which one wins.
  - *Open folders.* Whether a profile switch closes an open folder. (A profile switch
    shows the other profile's root.)
- The SDK's `switchToProfile` only reaches profiles that ship with a plugin ([profiles]).
  Users have asked Elgato for a "back to the previous profile" feature (Keyboard Maestro
  and MSFS forums). So the return-to-previous behaviour exists only inside the Smart
  Profile feature itself.

[sp]: https://www.elgato.com/us/en/explorer/products/stream-deck/smart-profiles-stream-deck/
[howto]: https://www.elgato.com/eu/et/explorer/products/stream-deck/how-to-set-up-smart-profiles-on-stream-deck/
[profiles]: https://docs.elgato.com/streamdeck/sdk/guides/profiles
[app-monitoring]: https://docs.elgato.com/streamdeck/sdk/guides/app-monitoring

## Config

Pages have no table of their own today. A page exists because some `[[key]]` has
`page = N` (PR 5), and `Config::page_count()` counts keys only. A field on `KeyConfig`
would be the wrong place, because the link belongs to the page, not to a key. So this PR
adds a small top-level `[[page]]` table, written only for pages that actually have
settings:

```rust
/// Settings for one page; only pages with a name or linked apps get a table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PageConfig {
    /// Zero-based page number, like `KeyConfig::page`.
    pub index: u8,
    /// Shown in the configurator's chip tooltip and title. Optional.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// Show this page while a window of one of these apps is focused (Elgato's Smart
    /// Profiles). Hyprland window classes, case-insensitive; `*` matches any run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub apps: Vec<String>,
}
```

- `Config.pages: Vec<PageConfig>`, `#[serde(default, rename = "page", skip_serializing_if
  = "Vec::is_empty")]`. It goes after `dials`, so it serialises after `[[dial]]`. Add it
  to `Config::default()`. An old file has no `pages`, so it saves exactly as before.
- `PageConfig::is_blank()`: no name and no apps (and an empty pattern counts as none).
  `save_to` drops blank pages (`retain`), removes empty `apps` strings, and sorts by
  `index`, just as it already does for keys and dials.
- `Config::page_config(p) -> Option<&PageConfig>` returns the first table with that
  index. `page_config_mut(p)` creates the table if it is missing.
- `page_count()` now also counts the indices of non-blank `[[page]]` tables, still capped
  at `MAX_PAGES`. A page you named or linked before giving it keys still gets a chip.
  Without `[[page]]` the count is unchanged.
- `Config::app_page(class: &str) -> Option<u8>` returns the lowest-index page below
  `page_count()` whose `apps` match. It skips tables with an index of `MAX_PAGES` or
  more.

```toml
[[page]]
index = 2
name = "Music"
apps = ["spotify", "chrome-music.youtube.com*"]
```

## Core: `crates/omadeck-core/src/follow.rs` (new, pure, tested)

- `pub fn class_matches(pattern: &str, class: &str) -> bool`: compares ASCII
  case-insensitively and supports `*` globs (any run, as many as needed). An empty
  pattern or an empty class never matches. About 20 lines, no new dependency.
- `pub const CONFIGURATOR_CLASS: &str = "dev.omadeck.Omadeck"`. The GUI's
  `main.rs::APP_ID` re-exports it, so the daemon can recognise the configurator's window.
- The state machine. The daemon keeps one `AppFollow` value, and every rule below is a
  unit test:

```rust
#[derive(Debug, Default, Clone, PartialEq)]
pub struct AppFollow {
    /// The page the focused app is linked to, if any.
    link: Option<u8>,
    /// Where to go when focus leaves linked apps; `None` once the user paged by hand.
    back: Option<u8>,
}
impl AppFollow {
    /// Focus moved to an app linked to `link`. Returns the page to show, if it changes.
    pub fn focus(&mut self, link: Option<u8>, current: u8) -> Option<u8>;
    /// The user changed page (key, dial, flick, ctl): they took over, so do not return.
    pub fn manual(&mut self);
    /// A reload shrank the page count.
    pub fn clamp(&mut self, page_count: u8);
}
```

`focus` does the following, in order:

1. `link == self.link` → `None`. This covers another window of the same app, a different
   app linked to the same page, and a popup.
2. `Some(p)` → if `back` is `None`, set `back = Some(current)`. Then set `link = Some(p)`
   and return `Some(p)` unless `p == current`. Moving from linked app A to linked app B
   keeps the original `back`, so leaving B returns to the page you were on before A.
3. `None` → `back.take()`, filtered to `!= current`.

`pub fn parse_clients(json: &str) -> Vec<(String, String)>` reads `(class, title)` pairs
from `hyprctl -j clients` (serde_json is already a dependency of core), drops duplicates
and empty classes, and sorts them. `pub fn open_windows()` runs `hyprctl` and calls it.
Only the GUI's window picker uses these.

## Daemon (`omadeckd/src/main.rs`)

**PR 9 assumptions.** `hypr.rs` exposes `HyprListener { events: mpsc::Receiver<HyprEvent> }`
with `HyprEvent::ActiveWindow(Option<ActiveWindow>)`, where `ActiveWindow { class, title,
address, pid }` (PR 9). It sends one seeded event at start and again after each
reconnect, and sends `ActiveWindow(None)` for `activewindow>>,` (treat it as an empty
class). Title-only changes (`windowtitlev2`) also arrive as `ActiveWindow(Some)` with the
same class, so the "same app, do nothing" rule matters. If PR 9 starts the listener only
when some key needs it, add "any `[[page]]` has `apps`" to that condition. Outside
Hyprland no events arrive, and this feature does nothing.

**PR 13 assumptions.** `Deck.page: u8` stays the root page. Folders are a stack on
`Deck` (for example `Deck.folders: Vec<…>`), and a page change through `do_page` empties
it (pops to root). If PR 13 names these differently, only `set_page` below changes.

New state in `run()`, kept across disconnects:

- `focus_class: Option<String>`: the last settled focused class.
- `focus_pending: Option<(String, Instant)>`: the newest unsettled class and when it
  settles.
- `Deck.follow: AppFollow`: reset on connect.

Event handling, in the loop next to the audio levels:

- **Drain `hypr.events`.** For `ActiveWindow(w)` (class = `w.class`, or empty for `None`), ignore it when it is
  `CONFIGURATOR_CLASS`; this is our version of Elgato's "paused while the app is open".
  Otherwise set `focus_pending = Some((class, now + APP_FOCUS_SETTLE))`, with
  `APP_FOCUS_SETTLE = 200 ms`. Only the newest event counts, so an alt-tab cycle through
  four windows makes one decision.
- **When `focus_pending` is due:** store the class in `focus_class`. Then, if a deck is
  connected, call
  `apply_focus(d, &cfg, …) = d.follow.focus(cfg.app_page(class), d.page)` and
  `set_page` to the result. The loop already wakes every `READ_TIMEOUT` (100 ms), so a
  switch lands 200–300 ms after focus settles.
- **Empty class** (no window focused, an empty workspace): treated as an app with no link,
  so the deck returns. Special workspaces and scratchpads need nothing special. Hyprland
  reports focus inside them like any other window, and focus is global across monitors.
- **Refactor `do_page`.** Split it into `set_page(d, cfg, renderer, hub, next)`, which
  pops folders, repaints, logs and publishes `PageChanged`, and `do_page`. All of
  `do_page`'s existing callers are manual: page keys, page dials and flicks, plus PR 11's
  ctl `page`. `do_page` calls `d.follow.manual()` when the page actually changes.
  Automatic switches call `set_page` directly. Opening or closing a folder is not a page
  change and leaves `back` alone.
- **Connect:** the new `Deck` gets `AppFollow::default()`. If `focus_class` is set, apply
  it straight away, so a deck plugged in while Spotify is focused opens on the Spotify
  page. Coming back means going back to page 0, where a fresh connect starts anyway.
- **Reload:** run `d.follow.clamp(cfg.page_count())` after the existing page clamp, then
  apply `focus_class` again. A link that was just added or removed takes effect without
  a focus change, and an unchanged link is a no-op (rule 1).
- Log lines: `page 3 of 4 (follows spotify)` and `page 1 of 4 (left spotify)`.
- `snapshot` is unchanged, since it shows the front page.

**IPC:** no new events. `Event::PageChanged` already moves the configurator, and an
automatic switch looks like any other page change to clients.

## GUI

- **Page settings button.** `refresh_chips` adds a small `gtk::Button` with a
  `emblem-system-symbolic` icon after the chips, tooltip "Page N settings". It opens a new
  `crates/omadeck-gui/src/pages.rs`, whose `pub fn show(app: &Rc<App>, page: u8)`
  presents an `adw::Dialog` titled "Page N" for the viewed page. This matches Elgato,
  where the link lives in the profile's settings and not on a key.
  - `adw::EntryRow` **Name**: edits `page_config_mut(page).name`.
  - Group **Follow apps**, description "Show this page while one of these apps is
    focused, and go back when you leave it." Each class gets one `adw::ActionRow`: the
    title is the app's display name when a desktop entry matches, otherwise the class;
    the subtitle is the class; a remove button sits at the end.
  - **Add app…** reuses `apps::pick`. The class comes from
    `DesktopAppInfo::startup_wm_class()`, otherwise the desktop id without `.desktop`.
    That works for GTK and Flatpak apps (class = app id) and for most Electron apps
    (case-insensitive matching covers `Spotify`/`spotify`).
  - **Add open window…** lists `follow::open_windows()` as class and title rows in a
    dialog like `apps::pick`. This is the reliable path where the desktop entry and the
    class don't match. Chromium web apps get classes like
    `chrome-<host>__<path>-Default`. TUI apps take the terminal's class unless they are
    launched with their own (unconfirmed for Omarchy's `omarchy-launch-tui`).
  - An `adw::EntryRow` **Window class** for typing a pattern with `*`. Its hint reads
    "find it with `hyprctl activewindow`".
- Chips: a linked page's chip gets a `linked` CSS class (a small accent underline). Its
  tooltip becomes "Page 3 · Music · follows Spotify, OBS".
- Title subtitle: "· page 3 of 4 (Music)" when the page has a name.
- All edits go through `schedule_save`, like key edits. The chips are rebuilt when a
  table makes a new page exist.
- **Preview:** unchanged. The grid follows the deck through `PageChanged`. Focusing the
  configurator never triggers a switch (the daemon ignores `CONFIGURATOR_CLASS`), so
  you can edit any page while the deck stays where it is.
- No *Try it now* (this is not an action). The starter layout is unchanged: no app is
  linked by default.

## Docs

- README: a feature bullet ("Pages can follow apps, like Elgato's Smart Profiles") and a
  roadmap line `- [x] **App pages**: switch page when an app is focused`.
- `examples/config.toml`: a commented `[[page]]` block under the Pages section, with the
  `hyprctl activewindow` hint and a glob example.
- CHANGELOG: one line under Unreleased → Added.

## Implementation steps

1. [ ] Core: `follow.rs` (`class_matches`, `AppFollow`, `parse_clients`) with its tests.
2. [ ] Core: `PageConfig`, `Config.pages`, `page_config(_mut)`, `app_page`,
       `page_count`, and `save_to` cleanup, with tests (including byte-identical saving).
3. [ ] Daemon: split `do_page` into `set_page` and `do_page` (`manual()`). No behaviour
       change yet.
4. [ ] Daemon: wire `HyprEvent::ActiveWindow` to `focus_pending`, then the settle and
       apply steps, plus reapplying on connect and reload.
5. [ ] GUI: `CONFIGURATOR_CLASS` re-exported as `APP_ID`; chip tooltips and title
       with the page name.
6. [ ] GUI: `pages.rs` dialog (name, app list, Add app…, Add open window…, class entry).
7. [ ] Docs, example, CHANGELOG. Then fmt, clippy and test.

## Tests

- follow: `class_matches_case_insensitive`, `class_matches_globs` (`chrome-*-Default`,
  `*spotify*`, `*`), `empty_pattern_or_class_never_matches`.
- follow: `enter_and_leave_returns` (0 → link 2 → None gives 2, then 0),
  `enter_when_already_there_is_noop`, `a_then_b_then_leave_returns_to_original`,
  `manual_switch_in_app_sticks` (manual, then leave → None),
  `refocus_same_link_does_not_switch_back` (manual, popup of the same app → None),
  `leave_then_reenter_switches_again`, `clamp_drops_out_of_range_back`.
- follow: `parse_clients` on captured `hyprctl -j clients` output, plus garbage input
  returning an empty list.
- config: `page_table_roundtrip`, `old_config_saves_byte_identical` (a fixture without
  `[[page]]`, loaded and saved, compared byte for byte), `blank_page_table_dropped`,
  `page_tables_sorted`, `page_count_counts_page_tables`,
  `app_page_lowest_index_wins`, `app_page_ignores_out_of_range`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] Link page 2 to Spotify. Focusing Spotify switches the deck, and focusing the
      browser returns to the page it was on.
- [ ] Alt-tab quickly through several windows: the deck changes at most once, after
      focus settles.
- [ ] Flick to another page while Spotify is focused, then leave Spotify: the deck stays
      where you put it. Focus Spotify again: the deck switches again.
- [ ] Spotify's own popups and menus don't bounce the deck.
- [ ] Closing the last window on a workspace (`activewindow>>,`) returns the deck.
- [ ] A scratchpad app (special workspace) linked to a page switches it when shown, and
      the deck returns when the scratchpad is hidden.
- [ ] Walker or other layer-shell launchers don't emit `activewindow` (unconfirmed) and
      don't move the deck.
- [ ] Focusing the configurator never switches the deck. Its grid follows the deck's
      automatic switches.
- [ ] With a folder open on page 1, focusing a linked app shows that page's root.
      Leaving the app returns to page 1's root.
- [ ] Unplug and replug while a linked app is focused: the deck reconnects on the linked
      page.
- [ ] Add a link in the GUI while the app is focused: the deck switches on reload,
      without a focus change.
- [ ] An Omarchy web app (Chromium `--app`), added with **Add open window…**, matches.
- [ ] A config without `[[page]]` saves byte-identical.

## Decisions to explain

- **Pages, not profiles.** omadeck has one layout with up to ten pages. A page that
  follows apps gives the same result as a Smart Profile without adding profiles.
- **A `[[page]]` table** instead of a field on keys. The link belongs to the page, and
  the table is written only when used, so old configs are unchanged. The table also
  makes room for names, which is the cheap reason to add `name` now.
- **Several apps per page** (`apps` is a list). Elgato's drop-down takes one app
  (unconfirmed). A list costs nothing and covers "OBS or the browser that hosts the
  stream".
- **Matching on window class** with a case-insensitive `*` glob. The class is Hyprland's
  equivalent of Elgato's executable or bundle id. The glob covers Chromium web apps,
  whose classes embed a URL.
- **Return to the page you were on before,** not to page 0. This matches what Elgato is
  reported to do (unconfirmed). A→B→leave returns to the page from before A.
- **A manual page change while an app is focused sticks:** leaving doesn't jump back
  (unconfirmed for Elgato). We never pull the deck away from a page the user just picked.
  Re-focusing the same app doesn't switch either; only a change of the linked page does.
- **Two pages linked to one app:** the lowest index wins. Elgato's behaviour here is
  unconfirmed.
- **Folders:** an automatic switch pops to root, and so does the return. Folder state is
  transient, and restoring it means guessing whether it is still valid.
- **A 200 ms settle** before acting. Elgato has no documented debounce. Hyprland sends
  an event for every focus step, so without the settle an alt-tab would flicker the
  keys.
- **The configurator's window is ignored.** Elgato instead turns Smart Profiles off
  whenever its own window is open. Ignoring our window gives the same "editing never
  jumps" result without pausing the feature while the window sits on another workspace.
- **No new IPC event.** `PageChanged` already describes what the deck shows.

## Later

- Matching on window title as well (for TUI apps inside a terminal and browser tabs).
- Linking an app to a folder (open folder X when the app is focused).
- Per-deck links, once multiple decks exist.
- The app's icon on its page chip, as Elgato shows it next to the profile.
- A ctl `omadeck follow on|off` to pause following, once PR 11 exists.
- Restoring an open folder when focus returns.

**Size:** about 550 lines. Core with tests about 250, daemon about 80, GUI about 190,
docs about 30.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- A manual page change while in a linked app sticks when you leave (planned), or should leaving always return to the earlier page?
- Page names in the `[[page]]` table: in scope (planned, cheap) or later?
- 200 ms settle delay before switching on a focus change: OK?
