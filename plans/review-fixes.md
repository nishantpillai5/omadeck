# `fix/review`: the bugs the 2026-10-17 review found

Part of [PLAN.md](PLAN.md). Branch `fix/review`, based on `fix/bugs` (`d7bd448`, which is
`daily`). It fixes BUGS.md 22–43 and nothing else: no restructuring, no new features.
`feat/groundwork` (groundwork.md) is based on this branch and does the restructuring
(D3–D11 in the cross-plan decisions); bugs 44–49 wait for it. The review itself is
summarised in [review-2026-10-17.md](review-2026-10-17.md).

**Written 2026-10-17 against `d7bd448`.** Every type and signature below is copied from
that tree. The core and daemon code in tasks 1–5 was built and tested in a scratch clone
(`cargo test -p omadeck-core -p omadeckd` and `cargo clippy -p omadeck-core -p omadeckd
--all-targets -- -D warnings` clean on macOS). The GUI code (tasks 6–7) was written
against the tree and run through `rustfmt`, but not compiled: gtk4 doesn't build here.

**Outcome:** CI runs and passes on every machine; pages made with **+** are steppable and a
page can be marked as a folder; a hand-edited `brightness` reaches the deck; a fast dial
spin paints its panel once per pass; the dial editor's defaults go the right way; broken,
SVG and odd-sized background images behave; the docs match the code.

## Where things run

- `omadeck-core` and `omadeckd` tests run anywhere (`cargo test -p omadeck-core`,
  `cargo test -p omadeckd`). The daemon tests here don't need a deck.
- `omadeck-gui` builds and runs **on the Omarchy machine only** (gtk4, libadwaita). GTK
  checks are manual there.
- Hardware checks need the Stream Deck + (`0fd9:0084`). Before running a dev build:
  `systemctl --user stop omadeckd`.

## Capture on the machine first

Do these on the Omarchy machine before the task that needs them, and paste the results
into the commit message or this file. Don't guess the output.

1. **Test font (task 1).** Find the bold JetBrains Mono file and its licence:
   ```bash
   fc-match -f '%{file}\n' 'JetBrainsMono Nerd Font Mono:style=Bold'
   pacman -Qo "$(fc-match -f '%{file}' 'JetBrainsMono Nerd Font Mono:style=Bold')"
   pacman -Ql <package from the line above> | grep -i -E 'licen|ofl'
   ```
   The licence must be the SIL Open Font License. If it isn't, download JetBrains Mono
   from its GitHub release instead and use `fonts/ttf/JetBrainsMono-Bold.ttf` and
   `OFL.txt` from the zip. Then subset it (`sudo pacman -S --needed python-fonttools`):
   ```bash
   pyftsubset "<the bold .ttf>" --unicodes=U+0020-007E,U+2013,U+2026 \
     --output-file=crates/omadeck-core/tests/fixtures/test-font.ttf
   cp "<the licence file>" crates/omadeck-core/tests/fixtures/test-font-OFL.txt
   ls -l crates/omadeck-core/tests/fixtures/test-font.ttf
   ```
   Expect a file of tens of KB. If it's over 100 KB, the subset didn't apply.
2. **Glyph code points (task 8).** In the Omarchy terminal (it uses a Nerd Font), run
   `printf '\U000F057F  \U000F036C  \U000F004E  \U000F0054\n'`. The review expects:
   a speaker with low volume, a microphone, a bold left arrow, a right arrow. If that's
   what shows, task 8's glyph changes stand. If not, write down what each one shows and
   pick the microphone and right-arrow code points from the
   [Nerd Fonts cheat sheet](https://www.nerdfonts.com/cheat-sheet) (`nf-md-microphone`,
   `nf-md-arrow_right`).
3. **CI on the fork (task 1, after pushing).** Actions must be enabled on
   `nishantpillai5/omadeck` (Settings → Actions). Then:
   ```bash
   gh workflow run ci.yml --ref fix/review -R nishantpillai5/omadeck
   gh run list --workflow ci.yml -R nishantpillai5/omadeck --limit 1
   ```

## Ask before building

1. Task 1 commits a ~20–60 KB font file (OFL, with its licence) to the repo, and the
   upstream PR will carry it. **Default if you don't say otherwise:** yes, it's the only
   way the goldens can match on every machine.

## Decided

- **CI triggers:** add `workflow_dispatch:` only. No `push: branches: ['**']`: that
  would be an upstream diff for our convenience. Our branches run CI by hand
  (`gh workflow run`, above).
- **Test font:** JetBrains Mono Bold (OFL), subset to printable ASCII plus `–` and `…`,
  at `crates/omadeck-core/tests/fixtures/test-font.ttf`. The goldens draw the glyph `"A"`,
  not a Nerd Font symbol, so the subset needs no PUA glyphs. Which default glyph a volume
  dial picks is tested on its own (`volume_glyph`). With a bundled font no render test
  skips any more, which is stricter than D9's "panic when `CI` is set".
- **Goldens are regenerated once in task 1**, on the current code, and guard against
  regressions from then on. `OMADECK_UPDATE_GOLDEN=1` rewrites them; the PNG diff is
  reviewed by eye in the commit.
- **Folders:** pages made from an Open-page action ("New page" on a key or on a dial
  press) are folders, because they start with a Back key and are reached by pressing. The
  page switcher's **+** makes a normal page. A **Folder** toggle button (folder icon) in
  the page actions, next to rename and delete, flips it. That button is D12's "Folder"
  switch. It matches Elgato, where folders aren't part of page cycling.
- **Dial defaults** (`Action::default_for`): Turn left negates Volume, Brightness and
  Page step; Turn right is `empty_of_kind`; Press of Volume is `mute: true` (Elgato's
  volume dial press mutes); every other kind is `empty_of_kind`. Keys keep
  `Action::empty_of_kind`. `Direction` is `Left | Right | Press` in `config.rs`;
  groundwork's `ActionTarget` reuses it.
- **Hyprland on a dial:** more than one step per report runs in order on one helper
  thread, with failures logged only. One step stays inline, so its error still becomes
  `DialActionFailed`. Other kinds are unchanged.
- **Panel painting:** panels are marked dirty everywhere and painted once at the end of
  every pass (`flush_dirty`). After a read that returned updates, up to `MAX_DRAIN = 32`
  more reports are taken with a zero timeout. Checked in elgato-streamdeck 0.13.1:
  `read(Some(Duration::ZERO))` calls hidapi's `read_timeout(.., 0)`, which returns at
  once with no data (then `NoData`, an empty `Vec`). This is the bug part of D4. The
  `next_deadline()` timing stays in groundwork.
- **Brightness (the bug part of D1):** a reload whose `brightness` differs from the
  previous config's applies it like a `set` (and saves it to the state file). A
  `SetBrightness` with no deck goes to the saver; `connect` reads it back. If the deck
  connects within the saver's 200 ms, it can start from the older value. Groundwork's
  `Runtime` removes that window. `snapshot` uses
  `State::load().brightness.unwrap_or(cfg.brightness)`.
- **Background cache:** the per-lookup `stat` stays for now. D9's "stat only on
  `clear_cache` with `(mtime, len)` stamps" is groundwork. Here a failed load is cached
  with its mtime, so it is retried only when the file changes.
- **Page links:** `links_to`, `rename_page` and `remove_page` cover dial slots as well as
  keys. The delete dialog says "keys or dials".

## Known traps

- **BUGS.md 4:** never `if let Action::X { .. } = get(&a.config.borrow()) { edit(..) }`.
  Clone the action out first (`let mut act = get(&a);`), drop the borrow, then edit.
  Every GUI change below follows this; keep it that way.
- **No `Ref` across `select`/`edit`:** `show_page`, `rebuild_grid` and `settings_popover`
  borrow `config` and `deck` briefly. Don't hold a `self.deck.borrow()` while calling
  `settings_popover()`.
- **Nothing slow on the deck loop.** Task 4 moves painting to one place per pass; don't
  add a `write_lcd` or `flush` anywhere else in `run()`. Task 3 moves repeated `hyprctl`
  off the loop.
- **`impl Fn + Clone` editors:** `action_rows` gets one more generic parameter
  (`default_for`). That's still fine without recursion; groundwork replaces the lot with
  `ActionSlot` (`Rc<dyn Fn>`). Don't make anything recursive here.
- **Signal loops:** set a widget's value before connecting its handler, and guard
  programmatic changes with `syncing_pages` (task 6's toggle does).
- **Goldens:** run `OMADECK_UPDATE_GOLDEN=1` only in task 1. Any later golden diff is a
  regression to explain, not to regenerate.

## Tasks

Do them in order, one commit each. Each ends with its check. Before each commit:
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` on the Omarchy
machine (the GUI only builds there).

### Task 1: CI parses, and the goldens use a bundled font (bugs 22, 23)

Files: `.github/workflows/ci.yml`, `crates/omadeck-core/src/render.rs`,
`crates/omadeck-core/tests/fixtures/{test-font.ttf, test-font-OFL.txt, strip-x1.png,
strip-b1.png}`. Do capture step 1 first.

1. `ci.yml`: put `workflow_dispatch:` under `on:`, and indent `run:` under its step:
   ```yaml
   on:
     push:
       branches: [master, main]
     pull_request:
     workflow_dispatch:
   ```
   ```yaml
         - name: Install dependencies
           # fonts-dejavu: the render tests draw text and need at least one installed font.
           run: pacman -Syu --noconfirm --needed base-devel rust gtk4 libadwaita systemd-libs fontconfig fonts-dejavu git
   ```
   Keep `fonts-dejavu` (the Arch package name; `font-dejavu` doesn't exist — the first CI
   run failed on it): `Renderer::new` in the other tests still asks fontconfig.
2. `render.rs`, in `impl Renderer`, before `update_style`:
   ```rust
   /// A renderer that draws with this font file instead of fontconfig's choice, so pixel
   /// tests give the same result on every machine. The theme is the built-in default.
   #[cfg(test)]
   pub(crate) fn with_font_bytes(style: &Style, data: &[u8]) -> Self {
       Self {
           font: Some(FontVec::try_from_vec(data.to_vec()).expect("a valid test font")),
           font_pattern: style.font.clone(),
           theme: Theme::default(),
           icons: HashMap::new(),
           backdrops: HashMap::new(),
           decode_count: std::cell::Cell::new(0),
       }
   }
   ```
   (Task 2 adds `load_attempts` to this literal and to `new`.)
3. Pull the default-glyph choice out of `volume_icon` into a free function next to the
   glyph constants, and call it from `volume_icon`:
   ```rust
   /// The glyph a volume dial's `$B1` panel shows when it has no icon: the mute glyph when
   /// muted, else the dial's own glyph, else a speaker (output) or a microphone (input).
   fn volume_glyph<'a>(face: &Face<'a>, device: AudioDevice, muted: bool) -> &'a str {
       if muted {
           MUTE_GLYPH
       } else if let Some(g) = face.glyph.filter(|g| !g.trim().is_empty()) {
           g
       } else if device.is_output() {
           OUTPUT_GLYPH
       } else {
           INPUT_GLYPH
       }
   }
   ```
   `volume_icon`'s body becomes the `face.icon` early return, then
   `self.glyph(volume_glyph(face, device, muted), (STRIP_ICON_PX as f32 * 0.86) as u32, color)`.
4. In `mod tests`, replace `renderer_with_font` and add `golden`:
   ```rust
   /// A renderer with the bundled test font (an OFL subset, `tests/fixtures/test-font.ttf`),
   /// so text pixels don't depend on the machine's fonts. Never skips.
   fn renderer_with_font() -> Renderer {
       Renderer::with_font_bytes(&test_style(), include_bytes!("../tests/fixtures/test-font.ttf"))
   }

   /// Compare `img` with the golden PNG `name` in `tests/fixtures`, or rewrite the golden
   /// when `OMADECK_UPDATE_GOLDEN=1` is set (then review the PNG diff before committing).
   fn golden(img: &RgbaImage, name: &str) {
       let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name);
       if std::env::var_os("OMADECK_UPDATE_GOLDEN").is_some() {
           img.save(&path).unwrap();
           return;
       }
       let want = image::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display())).to_rgba8();
       assert!(*img == want, "{name} differs; rerun with OMADECK_UPDATE_GOLDEN=1 and review the PNG diff");
   }
   ```
   Replace every `let Some(mut r) = renderer_with_font() else { return };` (seven of them)
   with `let mut r = renderer_with_font();`.
5. Rewrite `no_images_pixels_unchanged` (the `$B1` dial gets glyph `"A"`):
   ```rust
   /// The no-image panels must be pixel-identical to the goldens. They use the bundled
   /// font and the glyph "A", so they don't depend on the machine's fonts.
   #[test]
   fn no_images_pixels_unchanged() {
       let mut r = renderer_with_font();
       let cfg = Config { style: test_style(), ..Default::default() };
       golden(&r.render_strip_panel(&cfg, 0, 4, DialStatus::default()), "strip-empty.png");
       let mut cfg = Config { style: test_style(), ..Default::default() };
       cfg.dials.push(DialConfig { label: "Volume".into(), glyph: Some("A".into()), ..DialConfig::new(0) });
       golden(&r.render_strip_panel(&cfg, 0, 4, DialStatus::default()), "strip-x1.png");
       let mut cfg = Config { style: test_style(), ..Default::default() };
       cfg.dials.push(DialConfig { glyph: Some("A".into()), ..volume_dial() });
       let st = DialStatus { pressed: false, level: Some(Level { value: 0.4, text: "40%".into(), muted: false }) };
       golden(&r.render_strip_panel(&cfg, 0, 4, st), "strip-b1.png");
   }
   ```
6. Regenerate once: `OMADECK_UPDATE_GOLDEN=1 cargo test -p omadeck-core no_images`. Look
   at the two changed PNGs: label "Volume" and an "A" in the `$X1` panel; "A", "40%" and a
   40% bar in `$B1`. `strip-empty.png` must not change (it has no text).

The test to copy (add it next to the other render tests):
```rust
#[test]
fn volume_glyph_defaults() {
    use super::{INPUT_GLYPH, MUTE_GLYPH, OUTPUT_GLYPH, volume_glyph};
    let plain = volume_dial();
    let face = plain.face();
    assert_eq!(volume_glyph(&face, AudioDevice::Output, false), OUTPUT_GLYPH);
    assert_eq!(volume_glyph(&face, AudioDevice::Input, false), INPUT_GLYPH);
    assert_eq!(volume_glyph(&face, AudioDevice::Input, true), MUTE_GLYPH);
    let own = DialConfig { glyph: Some("A".into()), ..volume_dial() };
    assert_eq!(volume_glyph(&own.face(), AudioDevice::Output, false), "A");
}
```

**Check:**
- `actionlint .github/workflows/ci.yml` prints nothing. Without actionlint (`pacman -S
  actionlint`): `ruby -ryaml -e 'YAML.load(STDIN.read)' < .github/workflows/ci.yml`
  prints nothing and exits 0. On `d7bd448` both fail at line 16.
- `cargo test -p omadeck-core render` → `test result: ok. 27 passed` (26 before, plus
  `volume_glyph_defaults`; `no_images_pixels_unchanged` now passes). The same result
  on any machine, fonts or not.
- After pushing: capture step 3; the run's conclusion is `success`.

### Task 2: background images: cache failures, crop first, sharp SVG (bugs 39, 40, 41)

File: `crates/omadeck-core/src/render.rs`.

1. The cache keeps the mtime with a failure too. Above `pub struct Renderer`:
   ```rust
   /// A cached background image: the file's mtime when it was loaded, and the fitted image
   /// (`None` when the load failed).
   type Backdropped = (Option<std::time::SystemTime>, Option<RgbaImage>);
   ```
   The field becomes `backdrops: HashMap<(PathBuf, u32, u32), Backdropped>,` and a test
   counter is added after `decode_count` (in the struct, `new` and `with_font_bytes`):
   ```rust
   /// Every `load_cover` call, failed or not.
   #[cfg(test)]
   pub load_attempts: std::cell::Cell<u32>,
   ```
2. `clear_cache` keeps an entry whose stamp still matches, failed or not:
   ```rust
   self.backdrops
       .retain(|(path, _, _), (mtime, _)| fs::metadata(path).ok().and_then(|m| m.modified().ok()) == *mtime);
   ```
3. `backdrop_image`:
   ```rust
   /// A fitted background image (strip or dial), decoded once per file. A missing or
   /// unreadable file is cached as `None` with its mtime, so it warns once and the layer
   /// is skipped until the file changes.
   fn backdrop_image(&mut self, path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
       let k = (path.to_path_buf(), w, h);
       let mtime = fs::metadata(path).ok().and_then(|m| m.modified().ok());
       if let Some((mt, img)) = self.backdrops.get(&k)
           && *mt == mtime
       {
           return img.clone();
       }
       #[cfg(test)]
       self.load_attempts.set(self.load_attempts.get() + 1);
       let loaded = mtime.is_some().then(|| load_cover(path, w, h)).flatten();
       if loaded.is_none() {
           log::warn!("could not load background image {}", path.display());
       }
       #[cfg(test)]
       if loaded.is_some() {
           self.decode_count.set(self.decode_count.get() + 1);
       }
       self.backdrops.insert(k, (mtime, loaded.clone()));
       loaded
   }
   ```
   A missing file has `mtime == None`; it's cached as `(None, None)` and is a hit until
   the file appears.
4. `fit_cover` crops in source space, then resizes:
   ```rust
   /// Cover `w × h`: centre-crop the source to the target's aspect ratio, then resize
   /// (`Lanczos3`) to exactly `w × h`. Exact-size input is returned unchanged. Cropping
   /// first keeps the work bounded by the source and the target, whatever the aspect.
   pub fn fit_cover(img: &RgbaImage, w: u32, h: u32) -> RgbaImage {
       if img.dimensions() == (w, h) {
           return img.clone();
       }
       let (iw, ih) = img.dimensions();
       if iw == 0 || ih == 0 || w == 0 || h == 0 {
           return RgbaImage::new(w, h);
       }
       let s = (w as f32 / iw as f32).max(h as f32 / ih as f32);
       let cw = ((w as f32 / s).round() as u32).clamp(1, iw);
       let ch = ((h as f32 / s).round() as u32).clamp(1, ih);
       let src = imageops::crop_imm(img, (iw - cw) / 2, (ih - ch) / 2, cw, ch).to_image();
       if src.dimensions() == (w, h) { src } else { imageops::resize(&src, w, h, FilterType::Lanczos3) }
   }
   ```
5. SVGs render straight at the cover scale. In `load_cover`, the SVG branch becomes
   `return load_svg_cover(path, w, h);`. Replace `load_svg_raw` with:
   ```rust
   /// Rasterise an SVG straight at the cover scale into a `w × h` pixmap, centred, so it
   /// stays sharp (no raster resize) and a huge intrinsic size allocates nothing extra.
   fn load_svg_cover(path: &Path, w: u32, h: u32) -> Option<RgbaImage> {
       use resvg::tiny_skia;
       let tree = svg_tree(path)?;
       let size = tree.size();
       let (sw, sh) = (size.width(), size.height());
       if w == 0 || h == 0 || sw <= 0.0 || sh <= 0.0 {
           return None;
       }
       let s = (w as f32 / sw).max(h as f32 / sh);
       let tx = -(sw * s - w as f32) / 2.0;
       let ty = -(sh * s - h as f32) / 2.0;
       let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
       resvg::render(&tree, tiny_skia::Transform::from_scale(s, s).post_translate(tx, ty), &mut pixmap.as_mut());
       Some(pixmap_to_rgba(&pixmap))
   }
   ```
   Move the pixel copy out of `rasterize_svg` into a helper both use (`rasterize_svg`
   ends with `Some(pixmap_to_rgba(&pixmap))`):
   ```rust
   /// Copy a premultiplied tiny-skia pixmap into a straight-alpha `RgbaImage`.
   fn pixmap_to_rgba(pixmap: &resvg::tiny_skia::Pixmap) -> RgbaImage {
       let mut out = RgbaImage::new(pixmap.width(), pixmap.height());
       for (dst, src) in out.pixels_mut().zip(pixmap.pixels()) {
           let c = src.demultiply();
           *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
       }
       out
   }
   ```

The test to copy:
```rust
#[test]
fn failed_decode_is_cached() {
    let mut r = Renderer::new(&test_style());
    let dir = std::env::temp_dir().join(format!("omadeck-cover-garbage-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("broken.png");
    std::fs::write(&path, b"not a png").unwrap();
    assert!(r.backdrop_image(&path, 800, 100).is_none());
    assert!(r.backdrop_image(&path, 800, 100).is_none());
    assert_eq!(r.load_attempts.get(), 1, "a broken file is tried once, not on every paint");
    r.clear_cache();
    assert!(r.backdrop_image(&path, 800, 100).is_none());
    assert_eq!(r.load_attempts.get(), 1, "an unchanged broken file stays cached over a reload");
    let _ = std::fs::remove_dir_all(&dir);
}
```
Two more in the same style:
- `fit_cover_scales_and_crops`: a 100 × 400 red image with a green band at rows 194–206
  gives an 800 × 100 image that's green at (400, 50). Then
  `(3200, 200, 800, 100), (799, 99, 800, 100), (801, 101, 200, 100), (16, 1000, 800, 100)`
  each come out exactly `w × h`, and `fit_cover(&RgbaImage::new(0, 0), 8, 4)` is 8 × 4.
- `svg_cover_stays_sharp`: write a 16 × 2 SVG (`<rect>` red for x 0–8, blue for 8–16),
  `load_cover(&path, 800, 100)`, count the columns on row 50 where both red and blue are
  above 10: at most 2. (The old code blends ~134.)

**Check:** `cargo test -p omadeck-core render` → `test result: ok. 30 passed`; the
existing `fit_cover_*`, `clear_cache_keeps_unchanged_backdrop`, `touched_file_is_reloaded`
and `no_images_pixels_unchanged` still pass unchanged.

### Task 3: core: dial defaults, page helpers, dial links, Hyprland steps (bugs 24, 25, 31, 32 core parts)

Files: `crates/omadeck-core/src/config.rs`, `lib.rs`, `action.rs`.

1. `config.rs`, before `pub enum LevelSource`:
   ```rust
   /// One of a dial's three action slots.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum Direction {
       Left,
       Right,
       Press,
   }
   ```
   and re-export it: `lib.rs` `pub use config::{Action, AudioDevice, Config, DialConfig,
   Direction, HOME, KeyConfig, Keys, LevelSource, Page, Style};`.
2. `impl Action`, after `empty_of_kind`:
   ```rust
   /// The action a kind pick starts from in one of a dial's slots. Turning left gets
   /// the opposite sign of turning right (lower the volume or brightness, the previous
   /// page), and a Volume press mutes, as on Elgato's volume dial. Everything else is
   /// `empty_of_kind`.
   pub fn default_for(kind: usize, dir: Direction) -> Self {
       match (Self::empty_of_kind(kind), dir) {
           (Action::Volume { device, .. }, Direction::Press) => Action::Volume { device, change: 0, mute: true },
           (Action::Volume { device, change, mute }, Direction::Left) => {
               Action::Volume { device, change: -change, mute }
           }
           (Action::Brightness { change, set }, Direction::Left) => Action::Brightness { change: -change, set },
           (Action::PageStep { change }, Direction::Left) => Action::PageStep { change: -change },
           (act, _) => act,
       }
   }
   ```
3. `impl Config`: replace `all_keys_mut`, `links_to`, `rename_page`, `remove_page` and add
   the page helpers:
   ```rust
   /// Add a page named after `base` (made unique) that starts with `first`, and return
   /// its name. A `folder` page is skipped by page steps and flicks; the configurator's
   /// **+** makes normal pages, "New page" on an Open-page action makes folders.
   pub fn add_page(&mut self, base: &str, folder: bool, first: KeyConfig) -> String {
       let name = self.unique_page_name(base);
       self.pages.push(Page { name: name.clone(), keys: Keys(vec![first]), folder });
       name
   }

   /// Whether the page `name` is a folder (skipped by page steps).
   pub fn is_folder(&self, name: &str) -> bool {
       self.pages.iter().any(|p| p.folder && p.name == name)
   }

   /// Mark the page `name` as a folder or a normal page.
   pub fn set_folder(&mut self, name: &str, folder: bool) {
       if let Some(p) = self.pages.iter_mut().find(|p| p.name == name) {
           p.folder = folder;
       }
   }

   /// Every action in the config: each key's, on every page, and each dial's three.
   fn all_actions(&self) -> impl Iterator<Item = &Action> {
       let keys = self.keys.iter().chain(self.pages.iter().flat_map(|p| p.keys.iter())).map(|k| &k.action);
       keys.chain(self.dials.iter().flat_map(|d| [&d.turn_left, &d.turn_right, &d.press]))
   }

   fn all_actions_mut(&mut self) -> impl Iterator<Item = &mut Action> {
       let keys = self.keys.0.iter_mut().chain(self.pages.iter_mut().flat_map(|p| p.keys.0.iter_mut()));
       keys.map(|k| &mut k.action)
           .chain(self.dials.iter_mut().flat_map(|d| [&mut d.turn_left, &mut d.turn_right, &mut d.press]))
   }

   /// How many actions (keys, and dial turns and presses) open the page `name`.
   pub fn links_to(&self, name: &str) -> usize {
       self.all_actions().filter(|a| matches!(a, Action::Page { page } if page == name)).count()
   }

   /// Rename a page and every action (key or dial) that opens it.
   pub fn rename_page(&mut self, old: &str, new: &str) {
       let new = new.trim();
       for p in &mut self.pages {
           if p.name == old {
               p.name = new.to_string();
           }
       }
       for a in self.all_actions_mut() {
           if let Action::Page { page } = a
               && page == old
           {
               *page = new.to_string();
           }
       }
   }

   /// Delete a page. Keys and dials that opened it keep their look but no longer do
   /// anything.
   pub fn remove_page(&mut self, name: &str) {
       self.pages.retain(|p| p.name != name);
       for a in self.all_actions_mut() {
           if matches!(a, Action::Page { page } if page == name) {
               *a = Action::None;
           }
       }
   }
   ```
4. `action.rs` `run_steps`, after the `Volume` block and before the `for _ in 0..steps`:
   ```rust
   // `hyprctl dispatch` waits for Hyprland's answer, so a fast spin must not run several
   // on the caller's thread (the deck loop): more than one step runs in order on a helper
   // thread, and a failure there is logged. One step stays inline so its error is shown.
   if let Action::Hyprland { dispatch } = action
       && steps > 1
   {
       if dispatch.trim().is_empty() {
           bail!("no dispatcher set");
       }
       let dispatch = dispatch.clone();
       thread::spawn(move || {
           for _ in 0..steps {
               if let Err(e) = hypr_dispatch(&dispatch) {
                   log::warn!("hyprctl dispatch {dispatch}: {e:#}");
                   break;
               }
           }
       });
       return Ok(());
   }
   ```
   Don't write a test that runs a real dispatch: on the Omarchy machine it would switch
   workspaces. `hypr_steps_check_before_the_thread` checks that `run_steps(&Action::Hyprland
   { dispatch: "  ".into() }, 3)` errs with "no dispatcher".

The test to copy (`config.rs` tests):
```rust
#[test]
fn page_links_include_dials() {
    let mut c = Config::default();
    c.add_page("Media", false, KeyConfig::new(0));
    c.dials.push(DialConfig { press: Action::Page { page: "Media".into() }, ..DialConfig::new(0) });
    c.dials.push(DialConfig { turn_left: Action::Page { page: "Media".into() }, ..DialConfig::new(1) });
    assert_eq!(c.links_to("Media"), 2);
    c.rename_page("Media", "Music");
    assert_eq!(c.dial(0).unwrap().press, Action::Page { page: "Music".into() });
    assert_eq!(c.dial(1).unwrap().turn_left, Action::Page { page: "Music".into() });
    c.remove_page("Music");
    assert_eq!(c.dial(0).unwrap().press, Action::None);
    assert_eq!(c.links_to("Music"), 0);
}
```
Two more in the same style:
- `default_for_follows_direction`: Volume Left `-5`, Right `5`, Press
  `{ change: 0, mute: true }`; PageStep Left `-1`; Brightness Left `-5`; and
  `default_for(kind, Right) == empty_of_kind(kind)` for every `kind in 0..KINDS.len()`.
- `added_pages_step_unless_folder`: `add_page("Work", false, ..)` then
  `add_page("Work", true, ..)` gives `"Work 2"`; `step_page(None, 1)` is `Some(Some("Work"))`
  and `step_page(Some("Work"), 1)` is `Some(None)` (the folder is skipped); after
  `set_folder("Work 2", false)` it is `Some(Some("Work 2"))`.

**Check:** `cargo test -p omadeck-core` → `test result: ok. 77 passed` (69 at `d7bd448`,
plus 1 from task 1, 3 from task 2 and 4 here). `cargo clippy
-p omadeck-core --all-targets -- -D warnings` clean. The GUI still compiles: nothing it calls changed
signature (task 6 switches `App::add_page` to the new helper).

### Task 4: daemon: paint once per pass (bugs 29, 30)

File: `crates/omadeckd/src/main.rs`.

1. A constant next to `AUDIO_REFRESH_DELAY`:
   ```rust
   /// At most this many more HID reports are taken in one pass after the first, so a fast
   /// spin is handled together and its panels are painted once.
   const MAX_DRAIN: usize = 32;
   ```
2. Replace `paint_dial` with these two (delete `paint_dial`; nothing else may call it):
   ```rust
   /// Mark dial `i`'s panel for the end-of-pass paint.
   fn mark_dirty(d: &mut Deck, i: u8) {
       if let Some(f) = d.dirty_panels.get_mut(i as usize) {
           *f = true;
       }
   }

   /// Paint every dirty dial panel and flush once. Runs at the end of every loop pass, the
   /// only place panels are painted outside `paint_all`, so a fast spin, a tap flash and a
   /// level change in the same pass cost one write per panel.
   fn flush_dirty(d: &mut Deck, cfg: &Config, renderer: &mut Renderer) {
       if !d.dirty_panels.contains(&true) {
           return;
       }
       if d.info.strip.is_some() {
           for i in 0..d.info.dials {
               if d.dirty_panels.get(i as usize).copied().unwrap_or(false)
                   && let Err(e) = write_panel(d, cfg, renderer, i)
               {
                   log::warn!("paint dial {i}: {e}");
               }
           }
           if let Err(e) = d.dev.flush() {
               log::warn!("flush: {e}");
           }
       }
       d.dirty_panels.fill(false);
   }
   ```
3. In `run()`, every `paint_dial(d, &cfg, &mut renderer, i…)` becomes `mark_dirty(d, i…)`:
   the flash expiry (`for i in expired`), `EncoderDown`, `EncoderUp`, `TouchScreenPress`.
   In the audio-levels loop write `d.dirty_panels[i] = true;` (it iterates
   `d.dial_levels`, so `mark_dirty(d, ..)` would be a second borrow of `d`).
4. Drain the queued reports. `Ok(updates) => {` becomes:
   ```rust
   Ok(mut updates) => {
       // A read returns one HID report. Take the reports already queued behind
       // it too (a zero timeout doesn't wait), so a fast spin is one pass.
       if !updates.is_empty() {
           for _ in 0..MAX_DRAIN {
               match d.reader.read(Some(Duration::ZERO)) {
                   Ok(more) if !more.is_empty() => updates.extend(more),
                   _ => break,
               }
           }
       }
       for u in updates {
   ```
   A drain error is ignored here; the next pass's read reports it.
5. Delete the "One write per pass for what this batch of events dirtied" block inside
   `for u in updates`. After the whole `match d.reader.read(..) { … }`, still inside the
   `while`, add:
   ```rust
   // Everything this pass dirtied (input, levels, flashes, a GUI brightness change),
   // painted once.
   if let Some(d) = &mut deck {
       flush_dirty(d, &cfg, &mut renderer);
   }
   ```
   (The `Err` arm may have set `deck = None`; then nothing is painted.)
6. `after_volume`: no guess without a known level. Replace the `let cur = …unwrap_or(…)`
   line with:
   ```rust
   let Some(cur) = d.device_levels.get(device).cloned() else {
       // Nothing read yet (just connected): a guess from 0 would show a wrong bar; the
       // refresh brings the real value.
       let _ = audio.cmd.send(AudioCmd::Refresh(*device));
       return;
   };
   ```
7. Update the `Deck.dirty_panels` doc: "Panels to repaint at the end of this pass
   (`flush_dirty`)".

No unit test can drive a `Deck` (it holds the device). The check is clippy plus the
hardware checks. Task 5's tests go in the `mod tests` this task doesn't need.

**Check:** `cargo clippy -p omadeckd --all-targets -- -D warnings` clean (no unused
`paint_dial`); `grep -n "paint_dial\|\.flush()" crates/omadeckd/src/main.rs` shows only
`paint`, `paint_all` and `flush_dirty`. On the deck:
- [ ] Spin a volume dial hard for two seconds and stop: the bar stops with the knob, no
      catch-up after (`RUST_LOG=debug omadeckd` shows several `dial 0 turned` lines per
      `paint` burst, not one paint each).
- [ ] Drag the GUI's Brightness slider without touching the deck: the brightness dial's
      panel follows.
- [ ] A tap still flashes the title for about 150 ms; a dial press highlights while held.

### Task 5: daemon: brightness reaches the deck and is saved (bugs 26, 27, 28)

File: `crates/omadeckd/src/main.rs`.

1. The saver takes its save function, so it can be tested:
   ```rust
   fn brightness_saver(rx: mpsc::Receiver<u8>, save: impl Fn(u8)) {
   ```
   and its `State { brightness: Some(v) }.save()` call becomes `save(v);`, with:
   ```rust
   fn save_brightness(v: u8) {
       if let Err(e) = (State { brightness: Some(v) }).save() {
           log::warn!("saving brightness: {e:#}");
       }
   }
   ```
2. Keep the handle: `let saver = std::thread::Builder::new() … .spawn(move ||
   brightness_saver(brightness_rx, save_brightness)) …;` and at the end of `run()`, after
   `hub.shutdown();`:
   ```rust
   // Let the saver write a brightness that is still settling.
   drop(brightness_tx);
   let _ = saver.join();
   ```
3. Two helpers next to `do_brightness`:
   ```rust
   /// The new start brightness when `new` changes the config's `brightness` (a hand edit,
   /// or the configurator with no daemon), else `None`.
   fn hand_edited_brightness(old: &Config, new: &Config) -> Option<u8> {
       (new.brightness != old.brightness).then_some(new.brightness)
   }

   /// Set the brightness from outside the deck's own input (the configurator's slider, a
   /// config edit). With no deck it is only saved, and the next connect starts from it.
   fn set_brightness(deck: Option<&mut Deck>, saver: &mpsc::Sender<u8>, value: u8) {
       match deck {
           Some(d) => do_brightness(d, saver, Some(value), 0),
           None => {
               let _ = saver.send(value.min(100));
           }
       }
   }
   ```
4. The reload block: remember the edit before replacing `cfg`:
   ```rust
   let mut hand_edit = None;
   match Config::load() {
       Ok(new_cfg) => {
           hand_edit = hand_edited_brightness(&cfg, &new_cfg);
           cfg = new_cfg;
       }
       Err(e) => log::error!("{e:#}; keeping the previous config"),
   }
   ```
   and just before its `paint_all` (so the repaint uses the new value):
   ```rust
   // A hand edit of `brightness` in config.toml wins over the state file, like
   // any other hot-reloaded field (it is also saved there).
   if let Some(v) = hand_edit {
       set_brightness(deck.as_mut(), &brightness_tx, v);
   }
   ```
5. The hub commands: `if let Some(v) = bright && let Some(d) = &mut deck { do_brightness(..) }`
   becomes `if let Some(v) = bright { set_brightness(deck.as_mut(), &brightness_tx, v); }`.
6. `snapshot`: before `if let Some((sw, sh)) = info.strip`:
   ```rust
   // The deck runs on the state file's brightness once it has run, as `connect` does.
   let brightness = State::load().brightness.unwrap_or(cfg.brightness);
   ```
   and the brightness `Level` uses `brightness` instead of `cfg.brightness`. Its comment:
   "Brightness dials show the live start value; volume dials show a dash, as their level
   is read by the running daemon."

The test to copy (a new `#[cfg(test)] mod tests { use super::*; … }` at the end of
`main.rs`):
```rust
#[test]
fn saver_writes_the_last_value_when_dropped() {
    let saved = Arc::new(std::sync::Mutex::new(Vec::new()));
    let (tx, rx) = mpsc::channel::<u8>();
    let out = saved.clone();
    let saver = std::thread::spawn(move || brightness_saver(rx, move |v| out.lock().unwrap().push(v)));
    for v in [30, 40, 50] {
        tx.send(v).unwrap();
    }
    // Shutdown: drop the sender and join, as `run` does; the burst is saved once.
    drop(tx);
    saver.join().unwrap();
    assert_eq!(*saved.lock().unwrap(), vec![50]);
}
```
And `hand_edit_is_a_changed_brightness`: 70 → 30 is `Some(30)`, 70 → 70 is `None`.

**Check:** `cargo test -p omadeckd` → `test result: ok. 7 passed` (5 `nav` tests + 2).
`cargo clippy -p omadeckd --all-targets -- -D warnings` clean. On the deck:
- [ ] Edit `brightness = 30` in config.toml and save: the deck dims within a second, and
      `cat ~/.local/state/omadeck/state.toml` shows `brightness = 30`.
- [ ] Unplug the deck, move the GUI slider to 40, plug it back: it starts at 40.
- [ ] Turn the brightness dial and run `systemctl --user restart omadeckd` straight away:
      the deck comes back at the new value.
- [ ] `omadeckd snapshot /tmp/s.png` shows the brightness dial at the deck's value.

### Task 6: GUI: folders, page links, the dial editor survives a page change (bugs 24, 25, 34, 35)

Files: `crates/omadeck-gui/src/app.rs`, `editor.rs`. Omarchy machine only.

1. `KeyRef` gets an accessor (after the struct):
   ```rust
   impl KeyRef {
       /// The page the key is on (`None` is Home).
       pub fn page(&self) -> Option<&str> {
           self.page.as_deref()
       }
   }
   ```
2. `App::add_page` takes the flag and uses core's helper. Remove `Page` from the
   `omadeck_core::{…}` import (it's then unused):
   ```rust
   /// Add a page that starts with a Back key. Returns its name. A `folder` is skipped by
   /// flicks and Next/previous page: pages made for an Open-page action are folders,
   /// pages made with **+** are not.
   pub fn add_page(self: &Rc<Self>, name: &str, folder: bool) -> String {
       let name = self.config.borrow_mut().add_page(name, folder, back_key());
       self.sync_page_list();
       self.schedule_save();
       name
   }
   ```
   Callers: `ask_new_page` passes `false`; `editor.rs` `new_folder` and the dial's
   `on_new_page` closure in `dial_action_group` pass `true`.
3. The Folder toggle. Field after `page_actions`:
   ```rust
   /// The page actions' "Folder" toggle: on, flicks and Next/previous page skip the page.
   folder_toggle: gtk::ToggleButton,
   ```
   In `build`, before `page_actions` is made, and appended first:
   ```rust
   let folder_toggle = gtk::ToggleButton::builder()
       .icon_name("folder-symbolic")
       .tooltip_text("Folder: flicks and Next/previous page skip it")
       .build();
   let page_actions = gtk::Box::builder().css_classes(["linked"]).visible(false).build();
   page_actions.append(&folder_toggle);
   page_actions.append(&rename_page);
   page_actions.append(&delete_page);
   ```
   Add `folder_toggle,` to the `Self { … }` literal. Its handler, next to the others:
   ```rust
   let weak = Rc::downgrade(&app);
   app.folder_toggle.connect_toggled(move |b| {
       let Some(app) = weak.upgrade() else { return };
       if app.syncing_pages.get() {
           return;
       }
       let Some(page) = app.current_page() else { return };
       app.config.borrow_mut().set_folder(&page, b.is_active());
       app.schedule_save();
   });
   ```
   In `sync_page_list`, inside the `syncing_pages` guard, after `page_actions.set_visible`:
   ```rust
   self.folder_toggle.set_active(page.as_deref().is_some_and(|p| cfg.is_folder(p)));
   ```
4. `show_page` keeps a dial selected:
   ```rust
   if changed {
       self.sync_page_list();
       // Keys are per page, so a key's editor closes; dials aren't, so theirs stays.
       if matches!(self.selected.get(), Some(Selection::Key(_))) {
           self.select(None);
       }
       self.refresh_all();
   }
   ```
5. `editor.rs` `action_rows`, the `Action::Page` arm: exclude the key's own page only:
   ```rust
   // A key can't open the page it's on; a dial isn't on a page, so it can open any.
   let own = match target(app) {
       Target::Key(k) => k.page().map(str::to_string),
       _ => None,
   };
   let names: Vec<String> =
       app.config.borrow().page_names().into_iter().filter(|n| Some(n) != own.as_ref()).collect();
   ```
6. `ask_delete_page`'s body text: "the key that opens it" → "the key or dial that opens
   it", and "the {n} keys that open it" → "the {n} keys or dials that open it" (bug 25:
   `links_to` now counts dials).

No GUI-free test beyond task 3's; the check is manual.

**Check:** `cargo clippy --all-targets -- -D warnings` clean on the Omarchy machine. In the
app:
- [ ] **+** makes "Work": the folder button is off, and a flick on the deck reaches Work.
- [ ] A key set to Open page → **New page** makes a page whose folder button is on; a
      flick skips it. Toggling the button off saves `folder` away
      (`grep -n folder ~/.config/omadeck/config.toml`).
- [ ] With a dial press set to "Next/previous page", pressing it on the deck selects the
      dial and the dial editor stays open after the page changes.
- [ ] Viewing page "Media", a dial press's Open page list includes "Media"; a key's on
      "Media" doesn't.
- [ ] Rename a page a dial opens: the dial's Open page row shows the new name.

### Task 7: GUI: dial defaults, the Brightness preset, settings and popovers (bugs 32, 33, 36, 37, 38)

Files: `crates/omadeck-gui/src/app.rs`, `editor.rs`. Omarchy machine only.

1. `editor.rs`: import `Direction` (`use omadeck_core::{Action, AudioDevice, DialConfig,
   Direction, HOME, KeyConfig, Rgb, action, paths};`). `dial_action_group`'s last
   parameter `default_volume: i8` becomes `dir: Direction`, and its `default_for`
   argument becomes `move |kind| Action::default_for(kind, dir),`. In `build_dial`:
   ```rust
   // A kind pick starts from what the slot implies (`Action::default_for`): turning left
   // lowers or goes back, a Volume press mutes.
   page.add(&dial_action_group(app, i, "Turn left", |d| d.turn_left.clone(), |d, a| d.turn_left = a, Direction::Left));
   page.add(&dial_action_group(app, i, "Turn right", |d| d.turn_right.clone(), |d, a| d.turn_right = a, Direction::Right));
   page.add(&dial_action_group(app, i, "Press", |d| d.press.clone(), |d, a| d.press = a, Direction::Press));
   ```
   (`cargo fmt` splits the second one over several lines.)
2. `action_rows` takes `default_for` too, after `edit`:
   `default_for: impl Fn(usize) -> Action + Clone + 'static,`; `action_editor` passes its
   own: `action_rows(app, group, target, get, edit, default_for, edit_at, on_app_picked,
   on_new_page, on_favicon);`. In the `Action::Brightness` arm's Mode handler, clone it in
   (`let default_for = default_for.clone();` with the other clones) and:
   ```rust
   // Back to "change": the slot's default, so Turn left lowers again.
   let act = match sel {
       1 => Action::Brightness { change: 0, set: Some(50) },
       _ => default_for(Action::Brightness { change: 0, set: None }.kind_index()),
   };
   ```
   A key's `default_for` is `Action::empty_of_kind`, so a key still gets `+5`.
3. The Brightness preset sets every slot: after `d.turn_right = …` add
   ```rust
   // A preset sets every slot: a Volume press left over from the Volume preset
   // would make the panel show the volume.
   d.press = Action::None;
   ```
4. `app.rs` `rebuild_grid`, after `self.strip.set_visible(..)`:
   ```rust
   // The settings popover has a strip-background row only on a deck with a strip,
   // so rebuild it when a different deck (say, a Plus plugged in late) lays out.
   self.settings_btn.set_popover(Some(&self.settings_popover()));
   ```
   and delete the `app.settings_btn.set_popover(Some(&app.settings_popover()));` line in
   `build` (`build` calls `rebuild_grid` before showing the window). `reload_from_disk`
   keeps its own call.
5. `strip_menu`, after `popover.set_parent(&self.strip);`:
   ```rust
   // A new popover per right-click; unparent it once closed, or each one stays.
   popover.connect_closed(|p| {
       let p = p.clone();
       glib::idle_add_local_once(move || p.unparent());
   });
   ```
   (Unparent on idle, not inside the `closed` signal itself.)
6. `edit_dial` drops late results while the config is broken, as `edit_ref` does:
   ```rust
   /// Mutate a dial, refresh its preview and queue a save. Like [`App::edit_ref`], a
   /// late result (a picker) is dropped while config.toml can't be parsed.
   pub fn edit_dial(self: &Rc<Self>, i: u8, f: impl FnOnce(&mut DialConfig)) {
       if self.locked.borrow().is_some() {
           return;
       }
       f(self.config.borrow_mut().dial_mut(i));
       self.refresh_dial(i);
       self.refresh_strip();
       self.schedule_save();
   }
   ```

**Check:** `cargo clippy --all-targets -- -D warnings` clean. In the app:
- [ ] On a dial, pick Brightness for Turn left: change reads −5. Page step on Turn left:
      −1. Volume on Press: "Toggle mute" is on.
- [ ] Mode "Set to a value" then back to "Change per turn" on Turn left: −5.
- [ ] **Volume dial**, then **Brightness dial**: the panel shows the sun and the brightness;
      Press reads "Nothing".
- [ ] Start the app with the deck unplugged, then plug in the Plus: the settings menu has
      "Touch strip background".
- [ ] Right-click the strip ten times, closing each popover: GTK's inspector
      (`GTK_DEBUG=interactive`) shows no popover children left under the strip picture.

### Task 8: docs and glyphs (bugs 42, 43)

Files: `README.md`, `examples/config.toml`, `CHANGELOG.md`,
`crates/omadeck-core/src/config.rs`, `crates/omadeck-core/src/render.rs`. Do capture
step 2 first.

- `render.rs`: `const INPUT_GLYPH: &str = "f036c"; // microphone` (if step 2 confirmed it).
  Add `assert_eq!(INPUT_GLYPH, "f036c");` to `volume_glyph_defaults`. Fix the
  `DialStatus::level` doc: "A value to show in the `$B1` panel layout (volume or
  brightness)."
- `config.rs` docs:
  - `Action::Brightness`: "The daemon owns the current value and keeps it in the state
    file, so this is never "run" like the other actions."
  - `Page`: "A named page of keys. A `folder` page is one you open from a key (it starts
    with a Back key); flicks and `page_step` skip it."
  - `LevelSource::Volume`: "The level of an audio device, read by the daemon."
- `README.md`:
  - Hardware table, Stream Deck + row: "Keys, dials (turn left/right, press) and the touch
    strip (one panel per dial; tap = press; a horizontal flick changes page)".
  - "Pages and folders": after the **+** sentence, add "The folder button marks a page as
    a folder: flicks and Next/previous page skip it. A page made with **New page** from
    an Open page action is a folder already."
  - The config example: `brightness = 70` gets the comment `# start value; the deck
    remembers its last brightness, and an edit here sets it`, and the `[[page]]` example
    gets `# folder = true               # skipped by flicks and page_step`.
- `examples/config.toml`: the same `brightness` comment; `# folder = true  # skipped by
  flicks and page_step` under `[[page]] name = "Media"`; `# glyph = "f0054"   # right
  arrow`; `# glyph = "f057e"  # a speaker; a mic ("f036c") for an input dial`.
- `CHANGELOG.md`, under `## [Unreleased]`, a `### Fixed` section (add it if missing):
  ```markdown
  ### Fixed
  - Pages made with **+** are no longer folders, so flicks and `page_step` reach them; a
    page's folder button marks it as one.
  - Editing `brightness` in config.toml changes the deck again, and the last brightness
    is saved when the service stops.
  - A fast dial spin repaints its strip panel once per batch of input, not per report.
  - Renaming or deleting a page updates dials that open it.
  - The dial editor's defaults follow the slot: turning left lowers, a volume press mutes.
  - Background images: a broken file is tried once, SVGs are sharp, and wide or tall
    images no longer need a huge buffer.
  - An input volume dial shows a microphone.
  ```

**Check:** `cargo test -p omadeck-core` → `test result: ok.` (`example_parses` and
`example_dial_parses` still pass); `cargo fmt && cargo clippy --all-targets -- -D
warnings && cargo test` green on the Omarchy machine; `grep -n "not used yet\|persists it
to the config\|just a page with a Back key\|f057f\|f004e" README.md examples/config.toml
crates/omadeck-core/src/*.rs` prints nothing.

## After the branch

- Mark 22–43 fixed in BUGS.md with the commit of each task.
- Rebuild `daily` on `fix/review` and run it for a day before starting groundwork.
- groundwork.md is based on this branch: it can call `Action::default_for`, `Direction`,
  `Config::is_folder`, `flush_dirty`/`mark_dirty` and `set_brightness`, which exist
  after this.
