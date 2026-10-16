# Draft: issue for thewizster/omadeck

Not posted. Post it as an issue (or a discussion) on `thewizster/omadeck` only after the
pre-flight list below is done. Everything under the line is the issue text.

Pre-flight:
- [ ] PR 1's hardware checks ticked (`pr1-plus-input.md`).
- [ ] `upstream` remote added; `feat/*` branches rebased if upstream `master` moved, and the
      line counts below re-measured.
- [ ] Optional: a photo of the Plus running the strip, and a configurator screenshot, to
      attach.

---

**Title:** Stream Deck + support (dials, touch strip) and pages: proposal and PR plan

Hi! I've been running omadeck on a Stream Deck + (`0fd9:0084`) under Omarchy and have
built two roadmap items on my fork:

- **Stream Deck +: dials and touch strip (volume, brightness)**
- **Pages** (the first half of "Pages and folders")

Before opening anything, I'd like to check you want these, and how you'd like them
split, since together they're a fair amount of code. Everything is written to follow
omadeck's existing patterns (serde config with defaults, `Action` variants, one renderer
for the GUI preview, `snapshot` and the hardware, the single-threaded daemon loop), and
for what the user sees it copies the Elgato app as described in its SDK, so people coming
from it recognise it.

### What it does

- The daemon reads the Plus's 4 dials (turn, press) and the touch strip (tap, long press,
  flick) and publishes them on the event socket.
- **Dials run ordinary actions.** A `[[dial]]` table, shaped like `[[key]]`, with
  `turn_left`, `turn_right` and `press` actions, so any app, command, script or Hyprland
  dispatcher works on a dial with no new action language. One action per detent, capped
  at 8 per event so a fast spin can't run away.
- **The strip is drawn** as four 200×100 panels using Elgato's own `$X1` (icon) and `$B1`
  (indicator: icon, value and bar) layouts, in the Omarchy theme colours and the
  configured font, following `follow_theme`. A tap on a panel presses that dial.
- **Volume:** a `volume` action (output or mic; change ±N or toggle mute) for keys and
  dials. It goes through Omarchy's own `omarchy-audio-*` commands when they're installed,
  so the dial shows the same pop-up and keeps the same 100% cap as the volume keys, and
  falls back to `wpctl`. A volume dial's panel shows the live level.
- **Brightness:** a `brightness` action (change ±N or set) for keys and dials; a
  brightness dial shows its level on the strip, and the value is saved to the config so
  it survives a restart.
- **Pages:** keys get an optional `page` (up to 10 pages, Elgato's limit), a `page` action
  goes next/previous/to a page, and on the Plus a flick on the strip pages through, as in
  the Elgato app. Dials and the strip stay the same across pages, as in Elgato. The
  configurator gets page chips to edit each page.

Existing configs load and save unchanged: every new field is optional and isn't written
when unused. Decks without dials or a strip behave exactly as before.

### Proposed config

```toml
[[dial]]
index = 0
label = "Volume"
glyph = "f057e"
[dial.turn_left]
type = "volume"
change = -5
[dial.turn_right]
type = "volume"
change = 5
[dial.press]
type = "volume"
mute = true

[[dial]]
index = 1
label = "Workspace"
[dial.turn_left]
type = "hyprland"
dispatch = "workspace e-1"
[dial.turn_right]
type = "hyprland"
dispatch = "workspace e+1"

[[key]]
index = 7
page = 1            # 0-based like `index`; omitted on the front page
label = "Next page"
[key.action]
type = "page"
change = 1
```

### Proposed PRs, in order

Each one builds on the previous and is usable on its own:

| # | PR | Size (approx.) |
|---|----|----------------|
| 1 | Read and publish the Plus's dial and touch-strip input (`DeckInfo` reports dials and strip) | +170 |
| 2 | `[[dial]]` config: turns and presses run existing actions | +620 / −140 |
| 3 | Draw the touch strip (`$X1`/`$B1` panels, GUI preview, `snapshot`) | +730 / −110 |
| 4 | `volume` action with a live level on the strip | +930 / −40 |
| 5 | Pages: per-page keys, `page` action, strip flick | +540 / −50 |
| 6 | `brightness` action, live level on the strip, persisted | +450 / −50 |

All tested on a Stream Deck + on Omarchy 4. `cargo fmt`, `clippy -D warnings` and
`cargo test` pass on each, with a CHANGELOG line per PR.

### Choices you might want to weigh in on

- **"dial" / "strip"** in the config and code rather than "encoder" / "LCD", matching the
  roadmap wording.
- **`[[dial]]` reuses `Action`** rather than adding dial-specific modes; `volume` and
  `brightness` are ordinary actions that keys can use too, and the strip works out what
  value to show from a dial's actions.
- **The strip copies Elgato's published layouts** (positions and sizes from the SDK), with
  theme colours and an ellipsis on long titles instead of clipping.
- **Volume goes through Omarchy's commands when present**, the same idea as launching apps
  through `uwsm-app`.
- **Live levels are polled once a second on a worker thread**, so nothing slow ever runs
  on the deck loop.
- **Brightness is written back to the config** when a dial changes it (atomically, only
  that field), so the deck comes back at the same brightness.
- **Only the Plus's strip is drawn.** The Plus XL and the Neo's info bar are left alone
  until someone can test on them.

### Questions

1. Do you want these upstream, and is one PR at a time in this order OK? I can also
   merge PRs 1–2 into one, or hold PR 1 until PR 2 is ready if input that does nothing
   visible on its own isn't worth merging.
2. Any changes you'd like to the config format before I open PR 1? That's the part
   that's hardest to change later.
3. Anything in the list above you'd rather see done differently?

Happy to adjust anything. Thanks for omadeck!
