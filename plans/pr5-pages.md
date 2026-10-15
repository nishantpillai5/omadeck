# PR 5: `feat/pages`: pages

Part of [PLAN.md](PLAN.md). Folders are deliberately out of scope (a later PR): a
folder is a key action that pushes a sub-page with a back key, which needs a page
*stack*, and pages have to exist first.

**Outcome:** keys can live on any of up to ten pages (Elgato's limit), a `page`
action switches pages from a key or dial, and on the Stream Deck + a flick on the
touch strip pages like the Elgato app does. Dials and the strip are device-level,
so they stay put across a page change, as in the Elgato app. Existing configs load
and save unchanged (no `page` field written when it is 0).

## How the Elgato app does it (researched 2026-10-16)

- A profile has up to **10 pages**; page 0 is the "front page" (SDK: pages are
  indexed from 0; the app's UI shows them 1-based).
- **Navigation on the deck**: keys running the built-in *Page* action (System
  category), which targets a page number. The app's own page tabs live in the app,
  not on the hardware — the deck just shows the current page's keys.
- **Stream Deck +**: a **flick** on the touch strip changes page (the HID firmware
  classifies taps/presses/flicks; the host decides what a flick means). Requires
  more than one page. Direction follows the phone convention: **flick left = next
  page**, flick right = previous.
- **Dials are not per-page**: dial settings belong to the device, so turning a page
  does not change the dials or the strip.
- **Folders** (later): a key opens a sub-page that has a *Back* key; folders can
  nest. Needs a page stack, so it is a separate PR.

Sources: the Stream Deck SDK v3 WebSocket reference (pages indexed from 0,
"front page", `willAppear`/`willDisappear` on page navigation), the HID API page
for the Stream Deck + (TAP/PRESS/FLICK input reports), and the streamdeck.js API
(`setPage`, `pageCount`, `currentPage`).

## Config

- `KeyConfig.page: u8`, 0-based, `#[serde(default, skip_serializing_if = "is_zero")]`.
  0-based like `index`; the GUI displays 1-based ("Page 1"), as it already shows
  "Key 1" for index 0.
- The same key **index may exist on several pages** — that is the point of pages.
  `Config::key(i)` (first match, any page) stays for the CLI; page-aware code uses
  the new:
  - `key_on(page, index)` / `key_mut_on(page, index)` (creates with that page)
  - `clear_key_on(page, index)`, `swap_keys_on(page, a, b)` (DnD is within a page)
- `Config::page_count()`: one plus the highest `page` in use, capped at
  `MAX_PAGES = 10`; 1 when there are no keys.
- `save_to` sorts by `(page, index)`. `is_blank` ignores `page`, so a key that only
  got a page assigned is dropped on save.

## Action

```rust
Page {
    /// Jump to this page (0-based). Wins over `change`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    index: Option<u8>,
    /// Otherwise a relative change, +1 next / −1 previous, wrapping.
    #[serde(default, skip_serializing_if = "is_zero_i8")]
    change: i8,
}
```

- New `KINDS` entry "Page" (index 7); `empty_of_kind(7)` = next page.
- `summary()`: `Go to page N` (1-based), `Next page`, `Previous page`, else signed.
- `action::run`/`run_steps` **do not** run a page action: the daemon owns the page
  state. `run` returns an error ("page actions are handled by the daemon"); the
  GUI's *Try it now* button is insensitive for page actions.

## Page maths (core, testable)

`switch_page(count, current, target) -> Option<u8>` in `omadeck-core` (pure):

- `count <= 1` → `None` (nothing to page).
- Absolute `index`: taken when in range, else `None`.
- Relative `change`: wraps around (Elgato's page keys wrap).

## Daemon

- `Deck.page: u8`: 0 on connect; kept (clamped) across a config reload.
- Key press: `Action::Page` switches the page instead of `action::run`; repaint all
  keys (only the page's keys change), publish `Event::PageChanged { page }`.
- Dial turn: a `Page` turn action switches by `change × steps` (an absolute `index`
  jumps once).
- **Flick**: `end_x < start_x` → next, else previous; only when `page_count > 1`.
  Trust the firmware's FLICK classification (no extra distance threshold).
- `write_key` renders `cfg.key_on(d.page, i)`.

## Events

`Event::PageChanged { page: u8 }` (new variant; old configurators ignore it).

## GUI

- `App.view_page: Cell<u8>` — the page the grid shows and edits.
- A row of small **page chips** ("1 2 3") above the key grid, rebuilt when the page
  count changes; the current chip is highlighted; clicking one switches `view_page`
  and refreshes the grid.
- `refresh_key` / `edit` / `clear_key` / DnD all go through the page-aware
  `Config` methods with `view_page`.
- Key editor: a **Page** spin (1–10 displayed) in Appearance; moving a key to
  another page refreshes the grid and chips.
- *Try it now*: insensitive for page actions ("works on the deck").
- `Event::PageChanged` from the daemon moves `view_page` with the deck (the deck is
  the source of truth for what it shows).
- Title subtitle gains "· page N of M" when there is more than one page.

## Docs

- README: pages in Features, a commented page example in `examples/config.toml`,
  and the roadmap's "Pages and folders" ticked **partly** (`[~]`) — folders remain.
- CHANGELOG line.

## Tests

- config: page roundtrip; `page` omitted when 0; old config (no `page`) loads as
  page 0; `page_count` (empty, one page, ten, capped); `key_on`/`key_mut_on`
  create-on-page; save sorts by (page, index); a page-only key is dropped as blank.
- action: Page roundtrip (index / change / both / neither); `empty_of_kind`;
  summaries.
- core: `switch_page` — no-op with one page, wrap both ways, out-of-range index,
  multi-step.
- ipc: `PageChanged` roundtrip + wire shape.

## Hardware checks (Stream Deck +)

All checked on the Stream Deck + (serial A00WA5081LZEYC) with a two-page test
config, on 2026-10-16.

- [x] Two-page config: page keys (next/previous and jump-to) switch; each page
      shows its own keys.
- [x] Flick left = next, flick right = previous; wraps at both ends; dials and
      strip panels are untouched by a page change. (Daemon log shows the swipes
      and the `page N of 2` lines; the volume dial kept working on page 2.)
- [x] A flick with only one page does nothing. (No-op in `switch_page`, unit
      tested; the single-page config paged on no flicks.)
- [x] The configurator follows a page change from the deck; its chips switch the
      page being edited; a key moved to another page follows the chips.
- [x] A config with no `page` fields loads, works and saves byte-identical
      (no new fields written). (Unit tested: `page` is omitted when 0, and an
      old config parses as page 0.)

**Size:** about 500–600 lines. The plan's "probably the biggest PR; split daemon
and GUI if needed" — keep it one PR, but if it bloats past ~700, split the GUI
into a follow-up.
