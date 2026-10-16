# PR 7: `feat/strip-backgrounds`: background images on the touch strip

Part of [PLAN.md](PLAN.md). Based on `feat/brightness` (PR 6), which it touches in
`render.rs`, `config.rs`, the daemon's strip paint path and the GUI's dial editor. No
other PR is needed first.

**Rewritten 2026-10-17 against `daily` = `feat/brightness` (`b53a2e6`).** All Open
decisions are settled below; tasks A–F are split with exact checks; the code the tasks
build is written out in the "Code" sections. Verified against the tree: `Config` fields
are `brightness, style, keys, pages, dials`; `render_strip_panel(&mut self, dial,
style, st)`; `icon_row(app, icon, target, set_icon)` (the async-safe `target` is part of
the signature); `media::import_icon(src)`; `reload_from_disk` already calls
`renderer.clear_cache()`; the snapshot knob samples `panel.get_pixel(2, 2)`.

**Outcome:** the Stream Deck + touch strip can show one 800 × 100 background image across
the whole strip, and each dial panel can have its own 200 × 100 image, which wins over
the strip image for that panel. Both sit under the existing `$X1`/`$B1` layouts, so
titles, icons, values and bars draw on top unchanged. Images of the wrong size fill the
area and are centre-cropped. The GUI picks images the same way it picks key icons, and
the preview and `snapshot` show the same picture as the hardware. Without images, every
pixel is the same as today.

This PR adds the **strip layers** building block: `Renderer::panel_backdrop()` builds
everything under a panel's layout (colour, strip image slice, dial colour, dial image)
and returns a `Backdrop`. App volume (PR 10) and the media dial (PR 14) start from a
`Backdrop` and draw their layouts over it unchanged.

## How the Elgato app does it (researched 2026-10-16)

- **Three layers.** Elgato's touch strip help article lists, top to bottom: "1 - Dial
  action icon, 2 - Dial background, 3 - Touch Strip background". To set the strip one, you
  right-click the touch strip in the app.
  ([Elgato Stream Deck Touch Strip](https://help.elgato.com/hc/en-us/articles/10567698991629).
  The English page returned 403 to the fetcher, so this comes from the search-indexed text
  of the same article in pt-BR, ko and it.)
- **Strip background and per-dial background.** "For the touch strip, you can right-click
  to either upload your own image or choose from the background library", and "You can
  also customize the background for a specific dial action"
  ([How to add a background and screensaver](https://www.elgato.com/explorer/products/stream-deck/how-to-add-a-background-and-screensaver/)).
- **Sizes.** The strip is 800 × 100 (help article). Each dial's slot is 200 × 100
  ([Layouts (SD+)](https://docs.elgato.com/sdk/plugins/layouts-sd+)). In the
  [manifest](https://docs.elgato.com/streamdeck/sdk/references/manifest),
  `Encoder.background` is "the image … displayed on the touchscreen behind the action's
  layout": PNG or SVG, 200 × 100 (400 × 200 @2x), and the user can override it. So a dial
  background is a 200 × 100 layer under the layout, which is exactly the layer this PR adds.
- **Animated GIFs.** The SDK allows GIF only for key images, and encoder backgrounds are
  PNG/SVG only. Elgato's background page mentions GIFs only for screensavers.
  **Unconfirmed** whether the app accepts an animated GIF as a strip background a user
  uploads. We treat it as not supported and list it under Later.
- **Scaling and cropping.** For key backgrounds the page only says to "adjust the image as
  needed" (there is an image editor). **Unconfirmed** what the app does with a strip image
  of the wrong size, and whether it offers the same editor. We use "cover"
  (fill, then centre-crop), which is what an "adjust" editor starts from.
- **Readability.** The layout reference gives text items a `background` property with no
  default and a default `color` of white. Nothing is documented about a scrim, shadow or
  outline added over background images
  ([touch strip layout](https://docs.elgato.com/streamdeck/sdk/references/touch-strip-layout)).
  So Elgato adds nothing, as far as the docs say (**unconfirmed** against the real app).
- **Scope.** Key backgrounds are set "for that page" (per profile or page). Dials and the
  strip belong to the device, not the page (see [PR 5](pr5-pages.md)). **Unconfirmed**
  whether the strip background can differ per page. We make it one per config, like the dials.
- **Related: the full-deck key background.** One image spans the whole key area of a page
  ("Set from file…" on any key). It is a separate feature, but it would reuse this PR's
  cover-fit and slicing code. It goes under Later.

## Config

A dial's `background` is already a `#rrggbb` colour (PR 3), so the image field needs its
own name: `background_image`, on both the strip and the dial. That keeps `background`
meaning "colour" everywhere, and leaves room for a strip-wide colour later.

```rust
/// The Stream Deck + touch strip as a whole (`[strip]`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct StripConfig {
    /// An image across the whole strip (800×100 on a Plus). Any size: it fills the
    /// strip and is centre-cropped. Relative paths resolve against the config dir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background_image: Option<String>,
}
impl StripConfig { pub fn is_default(&self) -> bool { *self == Self::default() } }

// Config
#[serde(default, skip_serializing_if = "StripConfig::is_default")]
pub strip: StripConfig,

// DialConfig, after `background`
/// An image behind this dial's panel (200×100), over the strip image. Fills and crops.
#[serde(default, skip_serializing_if = "Option::is_none")]
pub background_image: Option<String>,
```

- `DialConfig::new` sets it to `None`. `is_blank()` counts it, so a dial with only an
  image is kept on save and drawn.
- `Face` gains `background_image: Option<&'a str>`. `KeyConfig::face()` sets it to `None`
  (keys get no image in this PR); `DialConfig::face()` passes its own through.
- `Config::default()` gets `strip: StripConfig::default()`. The field goes after
  `style` and before `keys` (current order `brightness, style, keys, pages, dials`), so
  `[strip]` saves after `[style]`, before `[[key]]`.
- Old configs: no `[strip]` and no `background_image` load as `None`, and saving writes
  neither, so a re-saved file is byte-identical.

```toml
[strip]
background_image = "icons/aurora-strip.png"   # 800×100 ideal; other sizes fill and crop

[[dial]]
index = 1
label = "Volume"
background_image = "icons/waves.png"          # 200×100 ideal; covers the strip image here
```

## Action changes

None. No new `Action` variant and no `KINDS` change.

## Core: `render.rs`, the strip layers

Layers of one panel, bottom to top. The first four are the backdrop; the layout draws on it:

| # | Layer | From | Notes |
|---|-------|------|-------|
| 1 | base colour | `style.background` / theme background / black | as today; dimmed ×0.55 for an empty slot **with no image** (layer 2 or 4) |
| 2 | strip slice | `[strip] background_image`, fitted to `PANEL_W·panels × PANEL_H`, cropped to `[i·200, 0, 200, 100]` | Elgato layer 3 |
| 3 | dial colour | the dial's own `background` (only when set on the dial) | the existing per-dial colour, now a layer over the strip image |
| 4 | dial image | the dial's `background_image`, fitted to 200 × 100 | Elgato layer 2. Alpha-composited, so a transparent PNG shows the strip through |
| 5 | layout | `$X1` / `$B1` (and later PR 10/14 layouts) | unchanged code |

New API, the building block later PRs use:

```rust
/// Everything under a strip panel's layout.
pub struct Backdrop {
    pub img: RgbaImage,      // PANEL_W × PANEL_H
    /// The colour layouts treat as "the background" for contrast: the plain colour
    /// without images, else the mean colour of `img`.
    pub tone: Rgb,
}

/// Layers 1–4 for dial `i` of `panels`.
pub fn panel_backdrop(&mut self, cfg: &Config, i: u8, panels: u32) -> Backdrop;
```

- `render_strip_panel` becomes `render_strip_panel(&mut self, cfg: &Config, i: u8, panels: u32,
  st: DialStatus)`. It needs the panel index to slice the strip image. It calls
  `panel_backdrop`, then draws `$X1`/`$B1` as now, with `bg` replaced by `backdrop.tone`
  in `label_color(..)` and in `draw_b1`'s track (`tone.mix(fg, 0.22)`). Without images,
  `tone` is the old `bg`, so the pixels are identical. Callers: `render_strip`,
  `omadeckd::write_panel`, `snapshot`, and the tests.
- `fn fit_cover(img: &RgbaImage, w: u32, h: u32) -> RgbaImage` (pure): scale by
  `max(w/iw, h/ih)` with `Lanczos3`, then centre-crop to `w × h`. It returns the input
  unchanged when the size already matches. An SVG is rasterised by resvg straight at the
  cover scale. `load_cover(path, w, h)` reuses `load_icon`'s SVG/raster split.
- An animated GIF loads its **first frame**. `image::open` already does this, and the
  `gif` feature is enabled in omadeck-core.
- An empty slot (no dial, or a blank dial) still gets layer 2. With a strip image, the
  empty panels show their slice **undimmed**, so the picture is continuous, as Elgato's
  strip background is.
- A missing or unreadable file logs one warning (the cache stores `None`, like `icon()`
  does) and the layer is skipped. The panel then looks as it does today.
- A tall strip (Plus XL) still returns the plain background. Nothing changes there.

**Readability.** We keep the Elgato look and add no scrim or outline. Two things already
in omadeck do the work. Titles and values are drawn by `draw_text_in(.., shadow: true)`,
which puts a 1 px 60 % black shadow under the text. And the automatic label colour now
reads `tone`, so a bright image flips the default white label to near-black. An explicit
`label_color` always wins, as now.

**follow_theme.** Images are never tinted. The theme still drives layer 1 (visible through
transparent images and under empty slots), the label/glyph/accent colours and the bar
colours, so `omarchy-theme-next` repaints as before.

**Pressed state.** Unchanged: the title turns `theme.accent`. Keeping the whole backdrop
means a press repaints only the text colour, with no layout jump.

**Caching, so repaints stay cheap.**
- `Renderer.backdrops: HashMap<(PathBuf, u32, u32), Option<(SystemTime, RgbaImage)>>` holds
  each fitted image at native size (800 × 100 strip, 200 × 100 dial), with the file's mtime.
- A panel repaint then costs one 200 × 100 crop (`imageops::crop_imm(..).to_image()`), at
  most one `overlay`, and one mean over 20 000 pixels for `tone`. That is microseconds,
  next to the JPEG encode we already pay for.
- `clear_cache()` keeps backdrop entries whose file mtime hasn't changed (one `stat` each)
  and drops the rest. This matters: a brightness dial persists on every turn, which reloads
  the config, which calls `clear_cache()`. Without the mtime check, every turn would decode
  a large wallpaper again. Icons keep today's clear-everything behaviour.

## Daemon (`omadeckd/src/main.rs`)

No new threads, channels or events. The strip is static, so nothing needs watching.
- `write_panel` calls `renderer.render_strip_panel(cfg, i, d.info.dials as u32, dial_status(..))`.
- Connect, reload, theme change and page change already run `paint_all`, which repaints
  every panel, so the backdrop comes along. A page change doesn't touch the strip anyway.
  Disconnect/reconnect needs nothing: the cache lives in `Renderer`, which outlives `Deck`.
- Decoding happens on the deck loop, once per image per change, as icons do today. Measure
  it with a 4K JPEG (hardware check below). If it takes more than ~100 ms, the GUI import
  can store a pre-fitted copy instead (Later).
- Image files in `icons/` are already watched (`relevant()`), so replacing one there reloads
  and the mtime check refreshes it. An image elsewhere (`~/Pictures/…`) is picked up on the
  next config change. Note this in the README.
- `snapshot`: `render_strip` covers the strip. The dial knobs currently sample
  `panel.get_pixel(2, 2)`, which would pick an image pixel. Use the backdrop's `tone`
  instead (`renderer.panel_backdrop(&cfg, i, n).tone`), so a knob takes the panel's
  average colour.

## IPC

None.

## GUI (`omadeck-gui`)

- **Image rows.** Generalise `icon_row(app, icon, target, set_icon)` to
  `image_row(app, title, hint, current, target, set)`, keeping the async-safe `target`
  (`Fn(&Rc<App>) -> Target`) so a pick lands where it started, as the pickers do today.
  `icon_row` stays a one-line wrapper over it, so the key editor doesn't change. The
  preview uses `render::load_cover(path, 80, 40)` for backgrounds, so the thumbnail shows
  the crop. Images import through `media::import_icon` into `icons/` (it validates and
  de-duplicates); no rename.
- **Dial editor, Appearance group:** a "Background image" row ("200 × 100; fills the panel
  and covers the strip image") under the Background colour row. The colour row's subtitle
  reads "Under the image" when both are set.
- **Strip background, two ways in:**
  - Right-click on the strip preview (`self.strip`, a `gtk::Image`) opens a small popover
    with "Set strip background…" and "Remove strip background". This copies Elgato's
    right-click on the touch strip:
    ```rust
    let click = gtk::GestureClick::builder().set_button(gdk::BUTTON_SECONDARY).build();
    click.connect_pressed({ let a = app.clone(); move |_, _, x, y| a.strip_menu(x, y) });
    strip.add_controller(&click);
    ```
    where `strip_menu` pops a `gtk::Popover` at the pointer.
  - The same row ("Touch strip background", "800 × 100; fills the strip") in the settings
    popover, shown only when `deck.strip.is_some()`. The popover is built once at startup,
    so the row keeps the `gtk::Image` thumbnail handle and updates its own subtitle and
    thumbnail after a change (a small `refresh` closure captured by both buttons).
  - Both edit `config.strip.background_image`, then `refresh_strip()` and `schedule_save()`.
- **Preview:** `refresh_strip` already renders through `render_strip`, so it matches the
  hardware. `reload_from_disk` already calls `renderer.clear_cache()`; with the mtime-aware
  cache a replaced file shows up and an unchanged one doesn't re-decode.
- **Presets and *Try it now*:** nothing changes.

## Starter layout

No change. omadeck ships no background library, and a default image would hide the theme
colours, which are the point of `follow_theme`.

## Docs

- README Features: the strip line gains "with an optional background image for the whole
  strip or per dial, like the Elgato app". The config reference gains `[strip]
  background_image` and the dial's `background_image`, with the fill-and-crop rule and the
  note about files outside the config dir.
- Roadmap: add and tick `- [x] **Stream Deck +**: background images on the touch strip`.
- `examples/config.toml`: a commented `# [strip]` block after the pages section, and a
  `# # background_image = "icons/…"` line in the first dial block (the
  `example_dial_parses` block count stays 3).
- CHANGELOG, under Unreleased → Added: "Stream Deck +: background images on the touch
  strip: one image across the strip (`[strip] background_image`) and one per dial
  (`background_image` on `[[dial]]`), which fill and crop to size; the configurator picks
  them, and right-clicking the strip preview sets the strip's."

## Code (write this, don't redesign it)

**`tone`, exactly.** The mean RGB of the final composited backdrop image, alpha ignored
(layer 1 is opaque, so the composite is opaque): `img.pixels().fold((0u32,0,0,0), …)`
averaging `px[0..3]`, divided by the pixel count. No image anywhere → the mean of a solid
rectangle, i.e. the old `bg`.

**`fit_cover` and `load_cover`** (free functions next to `load_icon`, `grep "pub fn load_icon"`):

```rust
/// Scale to cover `w × h` (longer side to the longer side, `Lanczos3`), then
/// centre-crop. Exact-size input is returned unchanged.
pub fn fit_cover(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
    if img.dimensions() == (w, h) { return img.clone(); }
    let (iw, ih) = img.dimensions();
    let scale = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let (nw, nh) = (((iw as f32 * scale).round() as u32).max(1), ((ih as f32 * scale).round() as u32).max(1));
    let big = imageops::resize(img, nw, nh, FilterType::Lanczos3);
    imageops::crop_imm(&big, (nw - w) / 2, (nh - h) / 2, w, h).to_image()
}

/// `load_icon`'s SVG/raster split, but fitted to cover `w × h` instead of inside a box.
pub fn load_cover(path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
    let is_svg = path.extension().and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    if is_svg {
        // Rasterise the SVG at the cover scale of its own size, then crop.
        let img = load_svg_raw(path)?;          // full-size raster, new helper split out of `load_svg`
        return Some(fit_cover(&img, w, h));
    }
    Some(image::open(path).ok()?.to_rgba8()).map(|img| fit_cover(&img, w, h))
}
```

`load_svg(path, box_px)` keeps its fit-to-box behaviour for icons; `load_svg_raw` is the
same body without the scale/crop (rasterise at the SVG's intrinsic size). An animated GIF
loads its first frame: `image::open` already does this (`gif` feature is enabled).

**`panel_backdrop`** (method on `Renderer`, `grep "fn render_strip_panel"` for the
neighbourhood). `dial` is `None` or blank → no layers 3/4:

```rust
pub fn panel_backdrop(&mut self, cfg: &Config, i: u8, panels: u32) -> Backdrop {
    let base = self.background(None, &cfg.style);
    let dial = cfg.dial(i).filter(|d| !d.is_blank());
    let mut img = solid_rect(PANEL_W, PANEL_H, base);
    // Layer 2: the strip image, fitted once per file and sliced per panel.
    if let Some(src) = cfg.strip.background_image.as_deref()
        && let Some(full) = self.backdrop_image(&paths::resolve(src), PANEL_W * panels, PANEL_H)
    {
        let slice = imageops::crop_imm(&full, i as u32 * PANEL_W, 0, PANEL_W, PANEL_H).to_image();
        imageops::overlay(&mut img, &slice, 0, 0);
    }
    if let Some(d) = &dial {
        // Layer 3: the dial's own colour, only when set on the dial.
        if let Some(c) = d.background.as_deref().and_then(Rgb::parse) {
            imageops::overlay(&mut img, &solid_rect(PANEL_W, PANEL_H, c), 0, 0);
        }
        // Layer 4: the dial image, alpha-composited over everything under it.
        if let Some(src) = d.background_image.as_deref()
            && let Some(image) = self.backdrop_image(&paths::resolve(src), PANEL_W, PANEL_H)
        {
            imageops::overlay(&mut img, &image, 0, 0);
        }
    }
    Backdrop { tone: mean_rgb(&img), img }
}

/// A fitted background image, decoded once per file (mtime in the key's value).
fn backdrop_image(&mut self, path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
    let mtime = fs::metadata(path).ok()?.modified().ok();
    let k = (path.to_path_buf(), w, h);
    if let Some((mt, img)) = self.backdrops.get(&k).cloned()
        && mt == mtime
    {
        return Some(img);
    }
    let loaded = load_cover(path, w, h);
    if loaded.is_none() {
        log::warn!("could not load background image {}", path.display());
    }
    if let Some(img) = &loaded { self.decode_count += 1; }
    self.backdrops.insert(k, mtime.zip(loaded.clone()));
    loaded
}
```

The empty-slot dimming (×0.55) moves out of `render_strip_panel` into the caller's
knowledge of layers: an empty slot **with no image** (no strip slice, no dial image) is
`solid(…)` of the dimmed base, exactly as today; with an image it is the undimmed
backdrop. Implement it in `render_strip_panel`:

```rust
pub fn render_strip_panel(&mut self, cfg: &Config, i: u8, panels: u32, st: DialStatus) -> RgbaImage {
    let dial = cfg.dial(i).filter(|d| !d.is_blank());
    let face = dial.map(|d| d.face());
    let has_image = cfg.strip.background_image.is_some()
        || dial.is_some_and(|d| d.background_image.is_some());
    let mut img = if face.is_none() && !has_image {
        solid_rect(PANEL_W, PANEL_H, self.background(None, &cfg.style).mix(Rgb(0, 0, 0), 0.55))
    } else {
        self.panel_backdrop(cfg, i, panels).img
    };
    let Some(face) = face else { return img };
    let tone = /* the backdrop's tone: recompute or keep from the branch above */;
    // … the rest of today's body unchanged, with `bg` replaced by `tone` in
    // `label_color(..)` and `draw_b1(..)` (its track is `tone.mix(fg, 0.22)`).
}
```

Compute the `tone` once (in `panel_backdrop`, and as the dimmed/plain base in the
no-image branch) so it isn't measured twice.

**Cache fields** on `Renderer` (next to `icons`, `grep "icons: HashMap"`):

```rust
backdrops: HashMap<(PathBuf, u32, u32), Option<(std::time::SystemTime, RgbaImage)>>,
#[cfg(test)]
pub decode_count: std::cell::Cell<u32>,
```

`clear_cache` (grep `"pub fn clear_cache"`): keep `self.icons.clear()`; for `backdrops`,
retain entries whose file mtime still matches (one `stat` each) and drop the rest.

## Implementation steps

Task sizes are ~150–300 lines including tests; each ends with its check.

0. [ ] **Golden pixels (old code, before any change).** On `daily`, add a throwaway test
       that renders: (a) a `$X1` panel (label + glyph dial, no image), (b) a `$B1` volume
       panel (`Level { value: 0.4, text: "40%" }`), (c) an empty slot — each at 200 × 100
       — and writes them to `crates/omadeck-core/tests/fixtures/strip-{x1,b1,empty}.png`
       (`include_bytes!`-able). Run it, commit the three PNGs and delete the throwaway
       test. **Check:** `ls crates/omadeck-core/tests/fixtures/` shows the three PNGs;
       `cargo test -p omadeck-core` still green.
1. [ ] **Task A — config.** `StripConfig`, `Config.strip` (after `style`),
       `DialConfig.background_image` + `new` + `is_blank`, `Face.background_image` in both
       `face()` impls. Tests: `strip_background_roundtrip`,
       `old_config_saves_without_strip`, `dial_background_image_roundtrip`,
       `dial_with_only_background_image_is_not_blank`. **Check:**
       `cargo test -p omadeck-core config` → all pass; `example_parses` green. Commit.
2. [ ] **Task B — render, pure.** `fit_cover`, `load_svg_raw`, `load_cover`, the
       `backdrops` cache + `backdrop_image` + mtime-aware `clear_cache` + `decode_count`.
       Tests: `fit_cover_exact_size_is_identity`, `fit_cover_wide_crops_centre`,
       `fit_cover_tall_crops_centre`, `clear_cache_keeps_unchanged_backdrop`,
       `touched_file_is_reloaded` (temp dir, `filetime`-free: write, `clear_cache`, write
       again with a newer mtime via `fs::set_file_times`). **Check:**
       `cargo test -p omadeck-core render` → all pass. Commit.
3. [ ] **Task C — render, layers.** `Backdrop`, `mean_rgb`, `panel_backdrop`,
       `render_strip_panel(cfg, i, panels, st)` with `tone`, `render_strip` updated to pass
       `(cfg, i, panels)`; fix the in-crate test callers. Tests from the Tests section
       (layer tests + `no_images_pixels_unchanged` comparing against the Task 0 fixtures).
       **Check:** `cargo test -p omadeck-core render` → all pass, including
       `no_images_pixels_unchanged`. The daemon no longer compiles (old signature) —
       expected; Task D fixes it. Commit with the daemon broken is **not** allowed:
       instead do Task D in the same commit as the call-site fixes, or fix
       `write_panel`/`snapshot` mechanically here and keep the knob-tone fix in Task D.
4. [ ] **Task D — daemon.** `write_panel` calls
       `render_strip_panel(cfg, i, d.info.dials as u32, dial_status(..))`; the snapshot
       knob uses `renderer.panel_backdrop(&cfg, i, n).tone` instead of
       `panel.get_pixel(2, 2)`. **Check:** `cargo build --workspace` and
       `cargo clippy --all-targets -- -D warnings` clean. Hardware checks below. Commit.
5. [ ] **Task E — GUI.** `image_row` (generalised `icon_row`, wrapper kept), the dial
       editor row, the strip right-click popover, the settings-popover row. **Check:**
       `cargo clippy --all-targets -- -D warnings` clean; manual: pick a dial image and a
       strip image, preview matches, right-click works. Commit.
6. [ ] **Task F — docs.** README, `examples/config.toml` (`example_dial_parses` block
       count stays 3), CHANGELOG. **Check:** `cargo fmt && cargo clippy --all-targets
       -- -D warnings && cargo test` all green. Commit.

## Tests

- config: `strip_background_roundtrip`, `old_config_saves_without_strip` (no `[strip]` and
  no `background_image` in the output; load then save of an old file is byte-identical),
  `dial_background_image_roundtrip`, `dial_with_only_background_image_is_not_blank`,
  `example_parses` / `example_dial_parses` still pass.
- render, pure:
  - `fit_cover_exact_size_is_identity`
  - `fit_cover_wide_crops_centre`: 1600 × 100 with red/green/blue thirds gives 800 × 100
    whose centre is green and whose edges are not red.
  - `fit_cover_tall_crops_centre`: the output is always exactly `w × h`.
- render, layers. These use a 4-colour 800 × 100 PNG written to a temp dir:
  - `strip_image_slices_per_panel`: panel `i`'s corner pixel is quarter `i`'s colour.
  - `dial_image_wins_over_strip`
  - `transparent_dial_image_shows_strip`
  - `dial_colour_covers_strip_image`
  - `empty_slot_shows_strip_undimmed`
  - `missing_image_falls_back_to_colour`: same pixels as with no image set.
  - `no_images_pixels_unchanged`: renders a `$X1` and a `$B1` panel through the new path
    and compares them with the old `bg`-based colours at fixed points.
  - `bright_image_flips_label_dark`: with a white image, `tone` is light and the default
    label colour is `Rgb(16, 16, 16)`.
  - `panel_isolation_with_strip_image`: changing dial 2's label changes only panel 2.
- render, cache: `clear_cache_keeps_unchanged_backdrop` (a second lookup after
  `clear_cache` doesn't re-read; check with a counter behind `cfg(test)`) and
  `touched_file_is_reloaded` (set a newer mtime, get new pixels).

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] An 800 × 100 strip image lines up across the four panels, with no seams or offsets
      at x = 200/400/600, and isn't mirrored or flipped.
- [ ] A 1920 × 1080 wallpaper as the strip image fills and centre-crops; reload time with a
      4K JPEG logged (`omadeckd` debug log). Keep it under ~100 ms, otherwise do the
      pre-fitted import from Later.
- [ ] Spinning a brightness dial with a strip image set doesn't lag. Each persist reloads
      the config, and the mtime cache must avoid re-decoding.
- [ ] A per-dial image covers only its panel. A transparent PNG shows the strip through it.
- [ ] Titles, `40%` values and groove bars stay readable on a busy photo and on a white
      image (the label flips dark). Compare with a photo of the Elgato app if one is to hand.
- [ ] Tap flash (accent title) is visible over an image.
- [ ] `omarchy-theme-next` with images set: the text and bar colours change, the images don't.
- [ ] An animated GIF shows its first frame and doesn't animate or error.
- [ ] Replacing an image in `icons/` updates the strip without restarting.
- [ ] Unplug and replug: the backdrop comes back.
- [ ] GUI: right-click on the strip preview sets and removes the strip image; the preview
      matches the hardware; `omadeckd snapshot` shows it.

## Decisions to explain

- **`background_image`, not `background`.** On `[[dial]]` and `[[key]]`, `background`
  is already a colour. Reusing the name for a path would make one field mean two things.
  The `[strip]` table uses the same name, so it can take a colour `background` later.
- **A `[strip]` table rather than a `Style` field.** The strip is its own surface in the
  Elgato app (you right-click it). A table also gives later strip options a home.
- **One strip image per config, not per page.** In omadeck (as in Elgato) the dials and
  strip are device-level, not per page. Whether Elgato allows a different strip
  background per page is unconfirmed.
- **Fill and centre-crop ("cover").** Elgato's fitting rule for the strip is unconfirmed.
  Cover never leaves bars or stretches, and an exactly-sized image is untouched. There is
  no crop editor; see Later.
- **The dial colour is a layer over the strip image.** This follows Elgato's order (dial
  background over strip background). The global/theme colour stays at the bottom, so it
  never hides a strip image.
- **Empty slots show the strip image undimmed.** Without an image they still use the
  dimmed empty-key colour from PR 3. With one, dark boxes would break up the picture.
- **No scrim or outline for readability.** Elgato documents none. omadeck's existing 1 px
  text shadow and the automatic label contrast (now measured against the image's mean
  colour) are enough; the user can still set `label_color`.
- **Images aren't themed.** `follow_theme` keeps driving the colours around the images, and
  an image the user picked is shown as it is.
- **First frame of a GIF only.** Elgato's SDK limits encoder backgrounds to PNG/SVG, and an
  animated strip would mean a JPEG write per frame per panel over USB. See Later.
- **Images go in `icons/`.** We reuse the icon importer instead of adding a second folder.
  The directory is already watched, and the config dir stays copyable.
- **Decoding on the deck loop.** It happens once per changed file, the same as icons
  today; the mtime cache means a brightness persist doesn't trigger it.

## Later

- **Full-deck key background** (Elgato's "Set from file…" across the key area, per page):
  `[[page]]`-level or `[style] key_background_image`, using `fit_cover` and slicing per key
  (decide whether the bezel gaps count). Then a per-key `background_image`.
- **Animated GIF strip/dial backgrounds** (unconfirmed in Elgato): a frame timer on the
  deck loop, capped at the SDK's 10 updates/s, writing only the panels that change.
- **A crop/position editor** in the GUI (or `background_fit = "cover" | "contain" |
  "stretch"`), if Elgato's editor turns out to offer that.
- **A background library**, like Elgato's, or "use the current Omarchy wallpaper" for the
  strip (`~/.config/omarchy/current/background`), which would fit `follow_theme` nicely.
- **A pre-fitted import:** the GUI stores the 800 × 100 / 200 × 100 crop instead of the
  original, if decode time on reload turns out to matter.
- Watching image files outside the config dir.
- Plus XL (vertical 100 × 1200 strip) backgrounds, together with its layouts.

**Size:** about 450 lines. Core plus tests ~280, daemon ~20, GUI ~120, docs ~30. The
`render_strip_panel` signature change and the `tone` substitution are the parts to
review most carefully.

## Decided (2026-10-17)

- **Decided: `background_image` everywhere.** `background` already means a colour on
  keys and dials; one field can't mean two things. `[strip]` keeps the same name, so a
  strip colour can join it later.
- **Decided: one strip image per config.** Dials and the strip are device-level in
  omadeck (as in Elgato, so far as the docs say); a per-page strip background is a Later
  item if Elgato turns out to allow it.
- **Decided: decode on the deck loop, mtime-cached.** Same as icons today. The 4K-JPEG
  hardware check is a gate: if a reload takes more than ~100 ms, move to the pre-fitted
  import from Later — nothing in Tasks A–E changes shape either way.

## Open decisions

None.
