# PR 3: `feat/touch-strip`: draw the touch strip

Part of [dials and the touch strip](dials-and-strip.md), which has the crate
facts, the PR order and the decisions to explain. User-facing choices follow the Elgato
SDK (see "Reference" in [PLAN.md](PLAN.md)).

**Outcome:** the strip shows one panel per dial (icon or glyph and label) in the theme's
colours, highlights the panel while its dial is pressed, and treats a tap on a panel as a
press. The GUI preview and `snapshot` show the same picture.

## Layout: match the Elgato Stream Deck app
The strip should look like it does in Elgato's own software, so someone coming from it
recognises each panel. The Elgato app splits the 800 × 100 strip into four 200 × 100
panels, one above each dial, and draws each panel with a fixed **layout**. The positions
and sizes are published in the SDK
([Layouts (SD+)](https://docs.elgato.com/sdk/plugins/layouts-sd+),
[touch strip layout reference](https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout)).
omadeck uses two of the six built-in layouts. Every rectangle below is
`[x, y, w, h]` inside one 200 × 100 panel:

**`$X1` "Icon"** (Elgato's default layout): for a dial with plain actions (PR 3)
| Item | Rect | Style |
|------|------|-------|
| title | `[16, 10, 136, 24]` | text, 16 px, weight 600, **left-aligned**, clipped |
| icon | `[76, 40, 48, 48]` | 48 × 48 image, centred in the panel |

**`$B1` "Indicator"**: for a dial with a value (PR 4, volume)
| Item | Rect | Style |
|------|------|-------|
| title | `[16, 10, 136, 24]` | as above |
| icon | `[16, 40, 48, 48]` | 48 × 48, now on the left |
| value | `[76, 40, 108, 32]` | text, 24 px, weight 600, **right-aligned** (e.g. `40%`) |
| indicator | `[76, 74, 108, 12]` | bar, "groove" style, no border, range 0–100 |

```
 one 200×100 panel, $X1                one 200×100 panel, $B1
┌──────────────────────────┐          ┌──────────────────────────┐
│ Workspace                │ y=10     │ Volume                   │
│                          │          │ ┌────┐             40%   │ y=40
│         ┌────┐           │ y=40     │ │ 󰕾  │                   │
│         │ ⌨  │           │          │ └────┘ ▬▬▬▬▬▬──────────  │ y=74
│         └────┘           │          │                          │
└──────────────────────────┘          └──────────────────────────┘
  x=16 title, x=76 icon                 x=16 icon, x=76 value/bar
```

Rules the Elgato app follows, which we copy:
- **No dividers** between panels. They sit side by side on one background.
- **Empty dial slot:** only the background, with no title and no icon. Use the same dimmed
  background as an empty key, which also matches what the Elgato app shows.
- **Text colour:** the Elgato default is white. Ours is `label_color()`, which is white
  when `follow_theme` is off and the theme foreground when it's on.
- **Bar colours:** Elgato's defaults are a white fill on a `darkGray` track. With
  `follow_theme` on, use `theme.accent` for the fill and `theme.muted` for the track.
  With it off, use white on `#808080`. That `#808080` is a guess: the Stream Deck app is
  Qt-based, and Qt's `darkGray` is `#808080`. Compare against a screenshot when one is
  available.
- **Groove bar:** a track with fully rounded ends (radius = h/2), and a fill with the same
  rounding, clipped to `value%` of the width.
- **Titles clip at 136 px and don't shrink.** Elgato clips; we put an ellipsis at the end
  (`…`), as key labels already do. Fonts come from the configured font, not Elgato's.
- **`show_label = false`** leaves out the title; the icon stays where it is.
- **Layers:** in the Elgato app the user stacks an 800×100 strip background, then a
  200×100 dial background, then the dial's icon. For PR 3 we only do the dial
  background as a *colour* (`background`, as on keys). Background images
  (`[strip] background = "…"` and a per-dial `background_image`) can come later; note it
  under "Later".
- **Pressed feedback:** the Elgato app doesn't document any. We keep a light one so a tap
  is visible: the title turns `theme.accent` while the dial is pressed, or for ~150 ms
  after a tap. No ring, so the layout stays the same as Elgato's. Leave the hook in
  place for PR 4: `DialStatus { pressed, level: Option<Level> }`.

The rectangles go in one place in `render.rs`, as named constants
(`X1_TITLE`, `X1_ICON`, `B1_ICON`, `B1_VALUE`, `B1_BAR`) in 200 × 100 panel units, with
a comment linking to the Elgato page. The renderer scales them when it draws at a
different size (GUI preview, `snapshot`).

## `omadeck-core/src/config.rs`
- Add look fields to `DialConfig`, with the same names and serde attributes as on
  `KeyConfig`: `icon`, `glyph`, `background`, `label_color`, and `show_label`. Count them
  in `is_blank()`.

## `omadeck-core/src/render.rs`
- So the look code isn't written twice, put the parts of `render_face` that read look
  fields behind a small borrowed view:
  `struct Face<'a> { label, show_label, icon, glyph, background, label_color }`, with
  `KeyConfig::face()` and `DialConfig::face()`. `background()`/`label_color()` take
  `Option<&Face>`.
  - This is the riskiest refactor in the series. Add a test that renders a few keys before
    and after the change and compares the pixels.
  - Nothing outside `render.rs` should need to change. If the GUI calls `background()`
    directly, keep a wrapper.
- `pub fn render_strip(&mut self, cfg: &Config, status: &[DialStatus], size: (u32, u32)) -> RgbaImage`
  draws the whole strip. `render_strip_panel(..)` draws one 200 × 100 panel, so the
  daemon can update a single panel.
- Text: `draw_line` only centres a line across the whole image. Add
  `draw_text_in(img, rect, px, align, color)` for left- and right-aligned text that
  ellipsizes at a fixed size. Have `draw_line` use it, so there's still only one text
  path.
- Icons and glyphs go into a 48 × 48 box (`self.icon(path, 48)`, and `self.glyph(.., 48 ×
  0.86, ..)` as for keys), centred in the icon rectangle.
- The strip is horizontal only. For a tall size (the Plus XL) return a plain background
  and log once that it isn't supported yet. Don't guess layouts for hardware that hasn't
  been tested.

## `omadeck-core/src/device.rs`
- Re-export `elgato_streamdeck::images::ImageRect`.

## `omadeckd/src/main.rs`
- `Deck` gains `dials: Vec<DialStatus>`.
- `paint_all` also draws the whole strip with `write_lcd(0, 0, ..)` when `info.strip` is
  set. It runs on connect, config reload and theme change, so `follow_theme` and hot
  reload come for free.
- `paint_dial(d, i)` redraws one panel with `write_lcd(i * panel_w, 0, ..)`. Call it on
  `EncoderDown/Up`. Batch all updates from one `read()` and redraw each panel once at the
  end, so a fast spin doesn't queue a JPEG per detent.
- **Touch, matching the Elgato app.** In the Elgato app, a tap on a panel does the same
  as pressing its dial, a long press is an optional second action that some plugins add,
  and a quick swipe left or right changes page.
  - Tap on panel `x / panel_w`: run that dial's `press`, and show the pressed title colour
    for about 150 ms. The loop already wakes every 100 ms, so keep a
    `flash_until: Instant` in `DialStatus`.
  - Long press: publish only. A `long_press` action can be added later if wanted.
  - Swipe: publish only. PR 5 uses it to change page, as the Elgato app does. Say so in
    a comment and in the PR.
- `snapshot`: on decks with a strip, draw the strip under the key grid, scaled to the
  grid's width, then a row of dial circles. That way the README image and the bug reports
  show it.

## GUI
- Under the key grid, add a `gtk::Picture` of `render_strip(..)` scaled to the grid's
  width. A `GestureClick` turns the x position into a panel, and clicking a panel selects
  its dial, like clicking a dial knob.
- Redraw it in `refresh_all`, and in the dial editor's edit path (like `refresh_key`).
- The dial editor gains the look rows. Reuse `icon_row` and `color_row`, which need the
  same getter/setter refactor that PR 2 did for actions.

## Tests
- `render_strip` returns exactly 800×100 for a Plus. Panel `i` changes only when dial `i`
  changes (compare pixel regions).
- Layout: with a solid-colour glyph, the drawn pixels fall inside `X1_ICON`, and the
  title's pixels fall inside `X1_TITLE`, starting near x = 16 (left-aligned).
- An empty dial slot is exactly the empty-key background colour.
- Before/after render test for keys (see the refactor above).
- `tap_to_dial(x, width, dials)`, including the edges (x = 0, x = 799, x ≥ width).

## Docs
- README: under Features, the strip shows each dial's label and icon and follows the
  theme. Tap = press. Update `docs/deck.png` if it's worth it.
- CHANGELOG line.

## Hardware checks
- [ ] Strip colours and orientation are right. Check for a mirrored or rotated image: the
      crate says `Rot0`, no mirroring.
- [ ] Turning the dials doesn't make the USB traffic lag key presses.
- [ ] `omarchy-theme-next` recolours the keys and the strip together.
- [ ] Brightness affects the strip. If it doesn't, write down what does.
- [ ] Stopping `omadeckd` returns the strip to the Elgato logo. If it doesn't, clear the
      strip in the shutdown path.
- [ ] Tap, long press and swipe on each panel edge pick the right dial.
- [ ] Panels with no font (`fc-match` missing) still draw, without a label.

**Size:** about 350 lines including the GUI. The render refactor is the part to review
most carefully.
