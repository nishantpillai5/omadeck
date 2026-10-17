# `fix/review-2`: the bugs the second review found

Part of [PLAN.md](PLAN.md). Branch `fix/review-2`, based on `feat/toggle-keys` (`55d0d62`,
which is `daily`). It fixes BUGS.md 50–72 (except 65, which PR 13 fixes), the parts of 25,
31, 38, 45 and 48 the review found still open, the clippy failure that would turn CI red,
and the small API changes R1–R9 that PRs 9–16 build on. No new features. The review is
summarised in [review-2026-10-17b.md](review-2026-10-17b.md). PR 9 is based on this
branch.

**Written 2026-10-17 against `55d0d62`.** Every type and signature below is copied from
that tree. The core and daemon code in tasks 1–6 was built and tested in a scratch copy:
`cargo test -p omadeck-core` → 141 passed, `cargo test -p omadeckd` → 41 passed plus the
two hub socket tests (they can't bind a socket in the sandbox; they pass on the machine),
`cargo clippy -p omadeck-core -p omadeckd --all-targets -- -D warnings` clean with clippy
0.1.95, `cargo fmt --check` clean. The GUI code (tasks 7–8) was written against the tree
but not compiled: gtk4 doesn't build here.

**Outcome:** CI is green with the current clippy; the configurator can't crash on a
toggle key's faces; renaming a page fixes on actions too; a slow toggle that never moves
comes back from busy; the volume bar can't drift; replugs keep the configurator's toggle
states; Hyprland never runs on the deck loop; a GUI edit no longer makes the next start
forget the brightness; the toggle editor's rows behave; the docs match the code.

## Where things run

- `omadeck-core` and `omadeckd` tests run anywhere (`cargo test -p omadeck-core`,
  `cargo test -p omadeckd`). None needs a deck.
- `omadeck-gui` builds and runs **on the Omarchy machine only** (gtk4, libadwaita). GTK
  checks are manual there.
- Hardware checks need the Stream Deck + (`0fd9:0084`). Before running a dev build:
  `systemctl --user stop omadeckd`.

## Capture on the machine first

1. **CI after task 1** (and again before PR 9 starts), on the fork:
   ```bash
   gh workflow run ci.yml --ref fix/review-2 -R nishantpillai5/omadeck
   gh run list --workflow ci.yml -R nishantpillai5/omadeck --limit 1
   ```
   The conclusion must be `success`. `feat/groundwork` and `feat/toggle-keys` never ran
   CI; with clippy 0.1.95 they fail on `render.rs` (task 1 fixes it).
2. **The local clippy version:** `cargo clippy --version`. The PR 8 notes said "clippy
   clean", but 0.1.95 isn't. If the machine's is older, `rustup update` (or `pacman -Syu
   rust`) first, so local and CI agree.

## Ask before building

None; all settled 2026-10-17.

## Decided

These are the round-2 decisions; later plans use them as existing code.

- **Decided: the State command row saves on Enter, on ✓, and when it loses focus** (task 7,
  bug 58). Clicking away never loses an edit; nothing is saved per keystroke, so a
  half-typed command still never runs while you type.
- **Decided: an old state file without `config_brightness` keeps its saved brightness
  once** (task 4), as before `d9644be`. A config edit made while the daemon was stopped
  right at the upgrade is missed that one time; accepted.
- **Decided: Hyprland failures arrive a moment later** (task 6). A key with a Hyprland
  action and a press toggle flips when the dispatch is queued, even if Hyprland then
  refuses it; the failure still toasts. Accepted: Elgato flips on press too.

- **R1 On face fallback.** `KeyConfig::face_for(Toggle::On)` with no `[key.on]` returns the
  off face. `KeyConfig::face_tabs()` says which tabs a key has; `App::select` resets
  `face_tab` to Off when the selection changes, and `build_key` clamps it.
- **R2 One action walker.** `Action::walk`/`walk_mut` is the one recursive walk (PR 12 adds
  the `Multi` arm there). `KeyConfig::actions`/`actions_mut` (action, then `on.action`),
  `DialConfig::actions`/`actions_mut` (slots in `Direction::ALL` order; PR 16 adds the long
  press there) and `Config::all_actions`/`all_actions_mut` (now `pub`) are built on it;
  `links_to`, `rename_page` and `remove_page` use them. `Direction::ALL` exists.
- **R3 Start brightness by value.** `State.config_brightness: Option<u8>` records the
  config value the daemon last saw. At start and on reload the config wins only if its
  value differs. `d9644be`'s mtime rule goes.
- **R4 Wire names.** `Value` serialises `rename_all = "snake_case"` (`{"toggle":"on"}`,
  `{"level":{…}}`). No compat shim: GUI and daemon ship together.
- **R5 Watcher.** Every `WatchCmd::Volume` is acknowledged once with the unclamped sum it
  applied, read or not (`(probe, None, applied)` when there's no read); the daemon's
  in-flight total isn't clamped and survives `detach`/`watch`. A `Leave` window that runs
  out forces its read. `Done` carries a run id. `published_keys` is cleared on attach and
  detach.
- **R6 Hyprland off the loop, in order.** The daemon sends every Hyprland dispatch (one
  step too) to one ordered worker thread, `HyprWorker`. Failures come back and are
  published by the loop. PR 12's guarantee for Hyprland steps is "queued in order on that
  worker". `action::run_steps` keeps its helper thread for Hyprland with `steps > 1` for
  the callers without a daemon (`omadeckd dial`); the daemon no longer calls it for
  Hyprland.
- **R7 Progress.** `probe::progress` reads the first number a `%` follows. `1/4`, `3 of 7`
  give no bar.
- **R8 `$B1` press.** A press recolours the title only.
- **R9 Test support and safety.** Core feature `test-support` makes
  `Renderer::with_font_bytes` and `render::TEST_FONT` public; the daemon and the GUI take
  it as a dev-dependency. `watched_probes` takes the key count. `App::action_at` and
  `App::set_action_at` match every `ActionTarget` variant by name (no wildcard), and
  `set_action_at(KeyOn)` is false without an on face. `action_rows` adds a hint row
  instead of setting the group's description.
- **Bug 65 (a held key repainted as released)** is PR 13's: its Back hold adds the
  held-key state to `Deck`.
- **Presets apply in place.** `apply_service` validates before it changes anything; a
  preset that sets a glyph clears `icon`, and keeps index, background and colours.
- **`Disconnected` goes out once**, from `Daemon::detach`; the hub's replay already starts
  with it. The no-deck scan no longer publishes it every 2 s.

## Known traps

- **BUGS.md 4:** never `if let Action::X { .. } = get(&a.config.borrow()) { edit(..) }`.
  Clone out, drop the borrow, then edit. In task 8's `set_action_at`, the `has_on` check
  ends its borrow before `edit_ref` runs.
- **No `RefCell` borrow across a call that borrows again:** `swap_key_states` and the
  `key_states` removal in `clear_key` run *before* `refresh_key`, which borrows
  `key_states` itself.
- **Nothing slow on the deck loop.** Task 6 takes `hyprctl` off it; don't call
  `action::run_steps` or `action::hypr_dispatch` from `Daemon` for a Hyprland action.
- **`face_tab` is reset only when the selection changes.** `rebuild_editor` calls
  `select(selected)` with the same selection; resetting there would throw the user off
  the On tab after every switch.
- **Signal loops:** set a widget's value (and a `ToggleButton`'s group) before connecting
  its handler.
- **Goldens:** no task here regenerates them. `pressed_b1_recolours_only_the_title` and the
  goldens are unpressed or compared against each other; a golden diff is a regression.
- **The in-flight total:** after task 5 nothing clamps or clears `Daemon.in_flight`. Don't
  add a clamp back "to be safe": the acknowledgements are unclamped, and a clamp is what
  made it drift.
- **`Done` in tests:** after task 5 every test `Done` needs `run: p.run_of(&probe)`
  (two-phase borrow: `p.handle(WatchCmd::Done { run: p.run_of(&probe), .. }, ..)`
  compiles).
- **Clippy 0.1.95** (`nonminimal_bool`, `is_none_or`): `rust-version` is 1.85 and
  `Option::is_none_or` has been stable since 1.82, so use it.

## Tasks

Do them in order, one commit each. Each ends with its check. Before each commit:
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` on the Omarchy
machine (the GUI only builds there). Test counts below are on the machine; `omadeckd`
counts include the two hub socket tests.

### Task 1: CI green; the on-face fallback; one action walker (clippy, R1 core, R2; bugs 25, 50)

Files: `crates/omadeck-core/src/render.rs`, `config.rs`.

1. `render.rs`, `Renderer::render`, the busy-accent condition. Clippy 0.1.95 rejects
   `!x.is_some_and(|s| !…)`:
   ```rust
   && style.label_color.as_deref().is_none_or(|s| s.trim().is_empty())
   ```
2. `config.rs` `KeyConfig::face_for`, the `Toggle::On` arm starts:
   ```rust
   Toggle::On => {
       // Only a key with `[key.on]` is ever on, but the configurator can draw a
       // stale state for a moment (a mirrored state, a face tab) after the on
       // face is removed: draw the off face then, never panic.
       let Some(on) = self.on.as_ref() else { return f };
   ```
3. `Direction` gets its list, under the enum:
   ```rust
   impl Direction {
       /// Every slot, in the order the editor shows them. PR 16's long press is not a
       /// `Direction`; it joins `DialConfig::actions` on its own.
       pub const ALL: [Direction; 3] = [Direction::Left, Direction::Right, Direction::Press];
   }
   ```
4. `impl Action`, after `is_none`:
   ```rust
   /// Visit this action and every action nested in it. The one recursive walk: PR 12
   /// adds an arm for a multi action's steps here, and every walker built on it
   /// (`KeyConfig::actions`, `Config::all_actions`, …) follows.
   pub fn walk<'a>(&'a self, f: &mut dyn FnMut(&'a Action)) {
       f(self);
   }

   /// `walk`, mutably. `f` sees the outer action first, then what's nested in it.
   pub fn walk_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
       f(self);
   }
   ```
5. `KeyConfig`, after `action_for`:
   ```rust
   /// Every action this key owns, nested ones included: `action`, then `on.action`.
   pub fn actions(&self) -> Vec<&Action> {
       let mut out = Vec::new();
       self.action.walk(&mut |a| out.push(a));
       if let Some(on) = &self.on {
           on.action.walk(&mut |a| out.push(a));
       }
       out
   }

   /// `actions`, mutably: `f` sees each one in the same order.
   pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
       self.action.walk_mut(f);
       if let Some(on) = &mut self.on {
           on.action.walk_mut(f);
       }
   }
   ```
   `DialConfig`, after `action_mut`:
   ```rust
   /// Every action this dial owns, nested ones included, slot by slot in
   /// `Direction::ALL` order. PR 16 adds the long press here.
   pub fn actions(&self) -> Vec<&Action> {
       let mut out = Vec::new();
       for dir in Direction::ALL {
           self.action(dir).walk(&mut |a| out.push(a));
       }
       out
   }

   /// `actions`, mutably.
   pub fn actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
       for dir in Direction::ALL {
           self.action_mut(dir).walk_mut(f);
       }
   }
   ```
   (`level_source` keeps its own order, right turn first: it picks which Volume wins.)
6. `Config`: replace the private `all_actions`/`all_actions_mut` and `links_to`:
   ```rust
   /// Every action in the config, nested ones included: each key's (its action and its
   /// on action) on every page, then each dial's. Built on `KeyConfig::actions` and
   /// `DialConfig::actions`, so a new place an action can live is added there once.
   pub fn all_actions(&self) -> Vec<&Action> {
       let keys = self.keys.iter().chain(self.pages.iter().flat_map(|p| p.keys.iter()));
       keys.flat_map(KeyConfig::actions).chain(self.dials.iter().flat_map(DialConfig::actions)).collect()
   }

   /// `all_actions`, mutably: `f` sees each one in the same order.
   pub fn all_actions_mut(&mut self, f: &mut dyn FnMut(&mut Action)) {
       for k in self.keys.0.iter_mut().chain(self.pages.iter_mut().flat_map(|p| p.keys.0.iter_mut())) {
           k.actions_mut(f);
       }
       for d in &mut self.dials {
           d.actions_mut(f);
       }
   }

   /// How many actions (keys, their on actions, and dial turns and presses) open the
   /// page `name`.
   pub fn links_to(&self, name: &str) -> usize {
       self.all_actions().into_iter().filter(|a| matches!(a, Action::Page { page } if page == name)).count()
   }
   ```
   In `rename_page` and `remove_page` the `for a in self.all_actions_mut() { … }` loops
   become `self.all_actions_mut(&mut |a| { … });` with the same bodies.
7. The face tabs, in the `impl KeyConfig` that holds `look`, before it:
   ```rust
   /// The face tabs the editor offers for this key, in order: Off always, On for a
   /// toggle (a key with `[key.on]`), Busy for a slow toggle or a busy-while-running
   /// key. A tab not in here must never be shown or written to.
   pub fn face_tabs(&self) -> Vec<FaceTab> {
       let mut tabs = vec![FaceTab::Off];
       if self.on.is_some() {
           tabs.push(FaceTab::On);
       }
       if self.is_slow_toggle() || self.busy_while_running {
           tabs.push(FaceTab::Busy);
       }
       tabs
   }

   /// `tab` if this key offers it, else Off (the editor's tab outlived the face it
   /// showed: another key was selected, or "Two faces" was switched off).
   pub fn clamp_face_tab(&self, tab: FaceTab) -> FaceTab {
       if self.face_tabs().contains(&tab) { tab } else { FaceTab::Off }
   }
   ```

The test to copy (`config.rs` tests):
```rust
#[test]
fn page_links_include_on_actions() {
    let mut c = Config::default();
    c.add_page("Media", false, KeyConfig::new(0));
    let on = OnFace { action: Action::Page { page: "Media".into() }, ..OnFace::default() };
    *c.keys.get_mut(3) = KeyConfig { on: Some(on), ..KeyConfig::new(3) };
    assert_eq!(c.links_to("Media"), 1, "the on action counts");
    c.rename_page("Media", "Music");
    let on_action = |c: &Config| c.key(None, 3).and_then(|k| k.on.as_ref()).map(|o| o.action.clone());
    assert_eq!(on_action(&c), Some(Action::Page { page: "Music".into() }));
    c.remove_page("Music");
    assert_eq!(on_action(&c), Some(Action::None));
    assert_eq!(c.links_to("Music"), 0);
}
```
Three more in the same style:
- `walkers_visit_every_slot_in_order`: a dial with `turn_left: Back`, `press: PageStep {
  change: 1 }` gives `[&Back, &None, &PageStep { change: 1 }]`; a key with `action: Back`
  and an on action `PageStep { change: -1 }` gives both; `KeyConfig::new(0).actions()` is
  `[&None]`.
- `on_state_without_on_face_draws_the_off_face`: a plain key with a label and glyph
  `"A"`: `face_for(&Toggle::On)` and `face_for(&Toggle::Off)` have the same `label`,
  `glyph` and `background`.
- `face_tabs_follow_the_key`: a plain key has `[Off]` and clamps On to Off; a key with
  `on: Some(OnFace::default())` has `[Off, On]`, keeps On, clamps Busy to Off; a
  `busy_while_running` key has `[Off, Busy]`.

**Check:** `cargo clippy --all-targets -- -D warnings` clean (it failed at `render.rs`
before); `cargo test -p omadeck-core` → `test result: ok. 132 passed` (128 + 4). Then
capture step 1: the CI run's conclusion is `success`.

### Task 2: core: progress, wire names, looks, watched keys, presets (R4, R7, R9 part; bugs 51 core, 52, 60, 63, 64)

Files: `crates/omadeck-core/src/probe.rs`, `config.rs`, `presets.rs`,
`crates/omadeckd/src/daemon.rs` (two call sites).

1. `probe.rs` `progress` reads only numbers a `%` follows, the first one:
   ```rust
   /// The first explicit `NN%` in a busy text, clamped to 100 (`"shard 2/4 43%"` → 43,
   /// `"43.5 %"` → 43, `"120%"` → 100). Only a number a `%` follows counts, so `"1/4"`,
   /// `"3 of 7"` or `"v2"` give no bar.
   pub fn progress(text: &str) -> Option<u8> {
       let parts: Vec<&str> = text.split('%').collect();
       // Every part but the last is followed by a `%`.
       parts[..parts.len() - 1].iter().find_map(|part| number_at_end(part))
   }

   /// The number (digits, at most one `.`) a text ends with, ignoring trailing spaces,
   /// clamped to 0–100.
   fn number_at_end(part: &str) -> Option<u8> {
       let t = part.trim_end();
       let b = t.as_bytes();
       let end = b.len();
       let mut start = end;
       let mut dot = false;
       while start > 0 {
           match b[start - 1] {
               b'0'..=b'9' => start -= 1,
               b'.' if !dot => {
                   dot = true;
                   start -= 1;
               }
               _ => break,
           }
       }
       if start < end && b[start..].iter().any(u8::is_ascii_digit) {
           t[start..].parse::<f32>().ok().map(|n| n.clamp(0.0, 100.0) as u8)
       } else {
           None
       }
   }
   ```
   Rename the test `progress_parses_last_percentage` to
   `progress_takes_the_first_explicit_percentage`; `"10% then 20%"` is now `Some(10)`;
   add `"Loading 43% step 5"` → 43, and `None` for `"1/4"`, `"Downloading 3 of 7"`,
   `"v2"` and `"%"`. Fix the `BusyFace.bar` doc: "(the first `NN%`)".
2. `Value` gets `#[serde(rename_all = "snake_case")]` under its derive.
3. `watched_probes` takes the key count:
   ```rust
   /// Everything the watcher should read while `page` is shown on a deck with `n_dials`
   /// dials and `n_keys` keys, each once: the dials' volume levels in dial order, then the
   /// probes of the page's keys the deck has (a toggle's state command, or the mute it
   /// follows). A key past `n_keys` (a config made for a bigger deck) is not polled. No
   /// deck: both are 0, so nothing is. PR 9 adds live keys, PR 10 app volumes.
   pub fn watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize, n_keys: u8) -> Vec<Probe> {
   ```
   and its key loop is `for k in ks.iter().filter(|k| k.index < n_keys) {`. The existing
   tests pass `32` as the new last argument. In `daemon.rs`, `Daemon::watch` and
   `keep_toggle_state` pass `self.key_count()` (0 with no deck, which also stops polling
   while unplugged, bug 64):
   ```rust
   // No deck: no dials and no keys, so nothing is polled until one is attached.
   let probes = probe::watched_probes(&self.cfg, self.nav.current(), dials, self.key_count());
   ```
4. `config.rs` `KeyConfig::set_look`, the blank branch's On and Busy arms never create a
   face:
   ```rust
   // Clearing never creates a face: an `[key.on]` would make the key a
   // toggle, and an empty `[key.busy]` is only noise in the file.
   (FaceTab::On, f) => {
       if let Some(on) = &mut self.on {
           on.clear(f);
       }
   }
   (FaceTab::Busy, f) => {
       if let Some(busy) = &mut self.busy {
           busy.clear(f);
       }
       if self.busy.as_ref().is_some_and(|b| *b == BusyFace::default()) {
           self.busy = None;
       }
   }
   ```
   Doc: "Setting a field on On or Busy creates that table if missing; clearing never
   does, and a busy face left with nothing set is dropped."
5. `presets.rs`: a glyph replaces the icon, and the Service URL is quoted.
   ```rust
   /// A preset's glyph replaces the key's icon too: an icon would beat the glyph on the off
   /// face (and the on face's glyph would then be the only one shown).
   fn set_glyph(k: &mut KeyConfig, glyph: &str) {
       k.icon = None;
       k.glyph = Some(glyph.into());
   }
   ```
   `mic_mute`, `output_mute`, `night_light` and `apply_service` call `set_glyph(k, "…")`
   instead of `k.glyph = Some("…".into())`. In `apply_service`, after the unit check:
   ```rust
   if let Some(url) = health_url
       && url.chars().any(char::is_control)
   {
       bail!("the health URL has a control character in it");
   }
   ```
   and `let state = match health_url.map(shell_words::quote) {`. Its doc: "`Err` when
   `unit` has characters outside `[A-Za-z0-9@._:-]`, or the URL has a control character;
   then `k` is untouched, so it can be applied to the key in place. The key's index,
   background and colours are kept."

The test to copy (`presets.rs` tests):
```rust
#[test]
fn service_quotes_the_health_url() {
    let mut k = KeyConfig::new(5);
    apply_service(&mut k, "llm.service", Some("http://127.0.0.1:8000/h?a=1&b=2")).unwrap();
    assert_eq!(k.index, 5, "applied in place, the key keeps its slot");
    let state = k.on.unwrap().state.unwrap();
    assert!(state.contains("-m 2 'http://127.0.0.1:8000/h?a=1&b=2' && exit 0"), "{state}");
    // The script still parses as shell.
    let ok = std::process::Command::new("sh").args(["-n", "-c", &state]).status().unwrap();
    assert!(ok.success());
    assert!(apply_service(&mut KeyConfig::new(0), "llm.service", Some("http://x/\nrm -rf ~")).is_err());
}
```
Four more in the same style:
- `presets_keep_the_key_and_replace_its_icon`: for every preset, a key 7 with
  `icon: Some("old.png")` and `background: Some("#112233")` keeps index 7 and the
  background; if the preset set a glyph, `icon` is `None`.
- `value_wire_names_are_snake_case` (`probe.rs`): `Value::Toggle(Toggle::On)` →
  `{"toggle":"on"}`; a `Value::Level` → `{"level":{"value":0.5,"text":"50%","muted":false}}`.
- `watched_probes_skip_keys_the_deck_lacks` (`probe.rs`): keys 1 (`state = "a"`) and 10
  (`state = "b"`): `(cfg, None, 4, 8)` → `[a]`, `(cfg, None, 4, 32)` → `[a, b]`,
  `(cfg, None, 0, 0)` → `[]`.
- `clearing_a_look_never_creates_a_face` (`config.rs`): clearing On Glyph and a blank
  Busy Label on a plain key leave `on` and `busy` `None`; setting then clearing the Busy
  label drops `busy`; a busy face with `bar: true` survives a cleared glyph.

**Check:** `cargo test -p omadeck-core` → `test result: ok. 137 passed`; `cargo test -p
omadeckd` → `test result: ok. 39 passed` (it compiles with the new `watched_probes`).

### Task 3: render: title-only press, test support, hermetic tests (R8, R9; bugs 61, 71)

Files: `crates/omadeck-core/Cargo.toml`, `src/render.rs`.

1. `Cargo.toml`, at the end:
   ```toml
   [features]
   # `Renderer::with_font_bytes` and the bundled test font, for other crates' tests:
   # `omadeck-core = { workspace = true, features = ["test-support"] }` under
   # `[dev-dependencies]`.
   test-support = []
   ```
2. `render.rs`, above `type Stamp`:
   ```rust
   /// The bundled test font: a JetBrains Mono Bold subset (OFL, printable ASCII plus `–`
   /// and `…`), for [`Renderer::with_font_bytes`]. Draw `"A"` where a test needs a glyph.
   #[cfg(any(test, feature = "test-support"))]
   pub const TEST_FONT: &[u8] = include_bytes!("../tests/fixtures/test-font.ttf");
   ```
   `with_font_bytes` becomes public under the feature. The stray "Re-resolve the font…"
   line above it moves down to `update_style`, where it belongs:
   ```rust
   /// A renderer that draws with this font file instead of fontconfig's choice, so pixel
   /// tests give the same result on every machine. The theme is the built-in default.
   /// For tests only: other crates get it with the `test-support` feature (pass
   /// [`TEST_FONT`]).
   #[cfg(any(test, feature = "test-support"))]
   pub fn with_font_bytes(style: &Style, data: &[u8]) -> Self {
       Self {
           font: Some(FontVec::try_from_vec(data.to_vec()).expect("a valid test font")),
           font_pattern: style.font.clone(),
           theme: Theme::default(),
           icons: HashMap::new(),
           backdrops: HashMap::new(),
           #[cfg(test)]
           decode_count: std::cell::Cell::new(0),
           #[cfg(test)]
           load_attempts: std::cell::Cell::new(0),
       }
   }

   /// Re-resolve the font if the style asks for a different one.
   pub fn update_style(&mut self, style: &Style) {
   ```
   (The two counters stay `#[cfg(test)]`: with only the feature, the struct has no such
   fields.)
3. `render_strip_panel`: the press colours the title only.
   ```rust
   // A pressed dial (or a tap, for a moment) turns the title into the accent colour,
   // as the only feedback: the layout stays the same as the Elgato app's. The value
   // and the bar keep their colours (PR 3's decision).
   let fg = self.label_color(Some(&face), style, tone);
   let title = if st.pressed { self.theme.accent } else { fg };
   ```
   `glyph_color` keeps using `fg`. The `$B1` colours become
   `B1Colors { title, value: fg, fill: fg, track: tone.mix(fg, 0.22) }`, and the `$X1`
   title is drawn with `title` instead of `fg`.
4. Tests: import `TEST_FONT` in `mod tests`' `use super::{…}`; `renderer_with_font`
   becomes `Renderer::with_font_bytes(&test_style(), TEST_FONT)`; and every
   `Renderer::new(&<style>)` in the tests (19, plus `label_color_contrast_kept`'s inline
   `Style`) becomes `Renderer::with_font_bytes(&<style>, TEST_FONT)`. In
   `busy_face_uses_accent_label_by_default`, delete the stale "(The busy label is the
   progress text…)" comment.

The test to copy:
```rust
#[test]
fn pressed_b1_recolours_only_the_title() {
    let mut r = renderer_with_font();
    let mut cfg = Config { style: test_style(), ..Default::default() };
    cfg.dials.push(DialConfig { glyph: Some("A".into()), ..volume_dial() });
    let up = r.render_strip_panel(&cfg, 0, 4, level(0.5, false));
    let down = r.render_strip_panel(&cfg, 0, 4, DialStatus { pressed: true, ..level(0.5, false) });
    let mut title_changed = false;
    for (x, y, p) in down.enumerate_pixels() {
        if in_rect(x, y, X1_TITLE) {
            title_changed |= p != up.get_pixel(x, y);
        } else {
            assert_eq!(p, up.get_pixel(x, y), "({x}, {y}) is outside the title");
        }
    }
    assert!(title_changed, "the title turns accent");
}
```

**Check:** `cargo test -p omadeck-core` → `test result: ok. 138 passed`;
`no_images_pixels_unchanged` still passes (no golden changes). `grep -n "Renderer::new"
crates/omadeck-core/src/render.rs` shows only `pub fn new`. `cargo clippy -p
omadeck-core --features test-support -- -D warnings` clean.

### Task 4: brightness by value; command timeouts kill the group (R3; bugs 45, 48)

Files: `crates/omadeck-core/src/state.rs`, `proc.rs`; `crates/omadeckd/src/daemon.rs`,
`main.rs`, `saver.rs` (test literals).

1. `state.rs`, the new field after `brightness`:
   ```rust
   /// The config's `brightness` when the daemon last read it. A config value that
   /// differs from it was edited since (by hand, or by the configurator with no daemon
   /// running), and wins. Compared by value, not by file times: the configurator
   /// rewrites config.toml on every edit without touching `brightness`.
   #[serde(default, skip_serializing_if = "Option::is_none")]
   pub config_brightness: Option<u8>,
   ```
   and first in `impl State`:
   ```rust
   /// The config's `brightness` when it was edited since the daemon last saw it, else
   /// `None`. A state file from before `config_brightness` existed has seen nothing, so
   /// its saved brightness still wins (as it did before).
   pub fn config_edit(&self, cfg: u8) -> Option<u8> {
       match self.config_brightness {
           Some(seen) if seen == cfg => None,
           None if self.brightness.is_some() => None,
           _ => Some(cfg),
       }
   }

   /// The brightness the deck starts at: a config edit, else the saved value, else the
   /// config's.
   pub fn start_brightness(&self, cfg: u8) -> u8 {
       self.config_edit(cfg).or(self.brightness).unwrap_or(cfg)
   }
   ```
   `State { brightness: … }` literals in the tests of `state.rs` and `saver.rs` gain
   `..State::default()` (or `config_brightness: None`).
2. `daemon.rs`: `Runtime::load`, `start_brightness`, `config_newer_than_state`,
   `hand_edited_brightness` and their two tests go (and the now unused `paths` import).
   In their place:
   ```rust
   impl Runtime {
       /// The runtime at start, from the saved state and the config's `brightness`
       /// (`None` when config.toml didn't load: a fallback default is not an edit). The
       /// config wins when its value differs from the one the state last saw; either way the
       /// state now records the config's value.
       pub fn start(mut state: State, cfg_brightness: Option<u8>) -> Self {
           let brightness = match cfg_brightness {
               Some(c) => {
                   let b = state.start_brightness(c);
                   state.config_brightness = Some(c);
                   b
               }
               None => state.brightness.unwrap_or_else(|| Config::default().brightness),
           };
           state.brightness = Some(brightness);
           Runtime { values: Values { brightness, ..Values::default() }, state, ..Self::default() }
       }
   }
   ```
   `Daemon::new` takes whether the config loaded:
   ```rust
   /// `cfg_loaded` is false when config.toml didn't parse and `cfg` is the empty
   /// fallback: its `brightness` is then no edit.
   pub fn new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self {
       let renderer = Renderer::new(&cfg.style);
       let saved = State::load();
       let rt = Runtime::start(saved.clone(), cfg_loaded.then_some(cfg.brightness));
       if rt.state != saved {
           saver.send(rt.state.clone());
       }
   ```
   `reload`: `hand_edit = self.rt.state.config_edit(new_cfg.brightness);`, and
   ```rust
   // A hand edit of `brightness` wins over the saved value, like any other field.
   // The state records the value it has now seen, so the next start doesn't apply
   // it again over a later dial turn.
   if let Some(v) = hand_edit {
       self.rt.state.config_brightness = Some(v);
       self.saver.send(self.rt.state.clone());
       self.set_brightness(Some(v), 0);
   }
   ```
   (Two sends: `set_brightness` returns early when the value is already the deck's, and
   `config_brightness` must still be saved. The saver debounces them into one write.)
3. `main.rs` `run()` loads the config itself (`load_config` stays for `press`/`dial`):
   ```rust
   let (cfg, cfg_loaded) = match Config::load() {
       Ok(c) => (c, true),
       Err(e) => {
           log::error!("{e:#}; using an empty config");
           (Config::default(), false)
       }
   };
   let mut daemon = Daemon::new(cfg, cfg_loaded, hub, watch::start(), saver::start_saver());
   ```
   `snapshot` uses the same rule: `brightness: State::load().start_brightness(cfg.brightness)`,
   with the comment "The brightness the daemon would start at (the state file's, unless
   the config's value was edited since)."
4. `proc.rs` `output_timeout` (bug 48): the child leads its own group, a timeout kills the
   group, and after exit the pipes get `GRACE`:
   ```rust
   /// Run `cmd` with stdin closed and stdout/stderr captured, and wait at most `timeout`.
   /// `None` if it couldn't start or ran too long (then its whole process group is killed,
   /// so a script's children go too). A non-zero exit is still `Some`: look at `status`.
   /// The child leads its own process group.
   pub fn output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
       cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
   ```
   the timeout arm:
   ```rust
   _ => {
       // Not reaped yet, so the group id is still ours: take the grandchildren
       // (and the pipes they hold) down with the child.
       unsafe { libc::killpg(child.id() as libc::pid_t, libc::SIGKILL) };
       let _ = child.wait();
   ```
   and the end:
   ```rust
   // A background grandchild can keep a pipe open after the child exits: wait at most
   // `GRACE` for the rest of the output, as `Job` does, not the rest of the deadline
   // (the watcher would sit out every queued volume change behind it).
   Some(Output {
       status,
       stdout: stdout.recv_timeout(GRACE).unwrap_or_default(),
       stderr: stderr.recv_timeout(GRACE).unwrap_or_default(),
   })
   ```
   `deadline` is then used only in the wait loop.

The test to copy (`daemon.rs` tests):
```rust
#[test]
fn start_brightness_follows_a_changed_config_value_only() {
    // Dimmed to 20 with the dial; the configurator later rewrote config.toml with
    // the same 70: the deck comes back at 20, and the state now records 70.
    let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, Some(70));
    assert_eq!(rt.values.brightness, 20);
    // The value was edited to 35 while the daemon was stopped: 35, saved as seen.
    let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, Some(35));
    assert_eq!((rt.values.brightness, rt.state.config_brightness), (35, Some(35)));
    assert_eq!(rt.state.brightness, Some(35));
    // config.toml didn't parse: its fallback default is not an edit.
    let rt = Runtime::start(State { brightness: Some(20), config_brightness: Some(70) }, None);
    assert_eq!((rt.values.brightness, rt.state.config_brightness), (20, Some(70)));
}
```
Three more in core:
- `a_config_edit_is_a_changed_value_not_a_newer_file` (`state.rs`): `{20, seen 70}` →
  `config_edit(70) == None`, `start_brightness(70) == 20`, `config_edit(35) == Some(35)`;
  `{20, seen None}` starts at 20; `State::default().start_brightness(70) == 70`.
- `output_timeout_does_not_wait_for_a_background_child` (`proc.rs`): `sh -c "sleep 5 &
  echo hi"` with a 5 s timeout returns `Some`, success, in under 1 s (its output is lost:
  the pipe never closes).
- `output_timeout_kills_the_whole_group`: `sh -c "sleep 30 | cat"` with 200 ms → `None`
  in under 1 s.

**Check:** `cargo test -p omadeck-core` → `test result: ok. 141 passed`; `cargo test -p
omadeckd` → `test result: ok. 38 passed` (39 − 2 + 1). On the deck:
- [ ] Turn the brightness dial to 20. Relabel a key in the GUI. `systemctl --user restart
      omadeckd`: the deck comes back at 20. `cat ~/.local/state/omadeck/state.toml` shows
      `brightness = 20` and `config_brightness = <the config's value>`.
- [ ] Stop the daemon, edit `brightness = 35` in config.toml, start it: 35.
- [ ] With the daemon running, edit `brightness = 45`: the deck dims within a second.

### Task 5: watcher and daemon: acknowledged volume, settled slow toggles, run ids, replugs (R5; bugs 53–56, 66)

Files: `crates/omadeckd/src/watch.rs`, `daemon.rs`, `main.rs`.

1. `watch.rs` types. `WatchCmd::Volume`'s doc adds "Always acknowledged exactly once, read
   or not, with the unclamped `change` it confirms, so the deck's in-flight total always
   comes back to zero." `Done` carries the run:
   ```rust
   /// A runner thread finished. Sent by the runner, not by the daemon. `run` is the id
   /// `run_due` gave the run: a `Done` from a run that was cancelled and replaced (the
   /// probe dropped and watched again meanwhile) doesn't match and is ignored.
   Done { probe: Probe, run: u64, result: RunResult },
   ```
   `Watcher.values` becomes `mpsc::Receiver<(Probe, Option<Value>, i32)>` (doc: "`value`
   is `None` only for a volume change that wasn't read back (the device isn't watched,
   or the read failed): it still confirms its change."). The same tuple replaces
   `(Probe, Value, i32)` in `Poller::handle`, `run_due`, `record`, `start` and `run`.
   `Poller.in_flight` becomes `HashMap<Probe, (u64, Arc<AtomicBool>)>`, with a new field
   `next_run: u64` (0 in `Default`). `Watch`'s retain is `|p, (_, cancel)|`, `cancel_all`
   iterates `for (_, c) in self.in_flight.values()`.
2. `Poller::handle`, the `Volume` arm:
   ```rust
   (self.change_volume)(device, change, mute);
   let p = Probe::Volume(device);
   let applied = if mute { 0 } else { change };
   let read = if self.watched.contains(&p) { (self.read_volume)(device) } else { None };
   match read {
       Some(v) => {
           self.forced.insert(p.clone());
           self.record(p, v, now, out, applied);
       }
       // Not watched, or the read failed (a hung `wpctl`): acknowledge the
       // change anyway, or the deck would count it as in flight for good.
       None => out.push((p, None, applied)),
   }
   ```
   The `Done` arm starts:
   ```rust
   WatchCmd::Done { probe, run, result } => {
       // Unwatched meanwhile, or a cancelled run whose probe has a new one.
       if self.in_flight.get(&probe).is_none_or(|(id, _)| *id != run) {
           return;
       }
       self.in_flight.remove(&probe);
   ```
3. `run_due` returns `Vec<(Probe, u64, Arc<AtomicBool>)>`; where it starts a command:
   ```rust
   let cancel = Arc::new(AtomicBool::new(false));
   self.next_run += 1;
   self.in_flight.insert(p.clone(), (self.next_run, cancel.clone()));
   start.push((p, self.next_run, cancel));
   ```
4. `record` (bug 53):
   ```rust
   let busy = matches!(v, Value::Toggle(Toggle::Busy(_)));
   // A window that ran out without the state moving sends its read anyway: the deck
   // is still showing busy, and would otherwise never hear that nothing changed.
   let expired = self.fast_until.get(&p).is_some_and(|(_, until)| now >= *until);
   // Decide first, then remove: `from` borrows `fast_until`.
   let window_over =
       expired || self.fast_until.get(&p).is_some_and(|(from, _)| !busy && v != Value::Toggle(from.clone()));
   ```
   then `let forced = self.forced.remove(&p) | expired;` (`|`, not `||`: the removal must
   always happen) and `out.push((p, Some(v), applied));`.
5. `run` (the thread loop), the runner start (bug 66):
   ```rust
   for (probe, run, cancel) in poller.run_due(Instant::now(), &mut out) {
       let Probe::Command { command, .. } = &probe else { continue };
       let (command, tx, p) = (command.clone(), runner_tx.clone(), probe.clone());
       let spawned = thread::Builder::new().name("watch-run".into()).spawn(move || {
           let result = run_state(&command, STATE_TIMEOUT, &cancel);
           let _ = tx.send(WatchCmd::Done { probe: p, run, result }); // watcher gone: nothing to do
       });
       if let Err(e) = spawned {
           // No run, so no `Done` will come: don't leave it in flight (it would never
           // be polled again); try again at the next interval.
           log::warn!("could not start a runner thread: {e}");
           poller.in_flight.remove(&probe);
           poller.next_due.insert(probe.clone(), Instant::now() + probe.interval());
       }
   }
   ```
6. `coalesce` sums without a clamp: `*c1 += *c2;` with the comment "Unclamped: the sum is
   what the read-back confirms to the deck. `change_volume` clamps what it applies." Its
   test's `60 + 60` case is now `120`.
7. `daemon.rs`: `apply_value` takes the option and settles the total first:
   ```rust
   pub fn apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32) {
       // Every volume change is acknowledged exactly once, read or not: take its part
       // off the in-flight total first, so the total can't drift.
       if let Probe::Volume(dev) = &probe
           && applied != 0
       {
           let f = self.in_flight.entry(*dev).or_insert(0);
           *f -= applied;
           if *f == 0 {
               self.in_flight.remove(dev);
           }
       }
       // An acknowledgement without a read: nothing to show.
       let Some(mut value) = value else { return };
   ```
   and further down
   `value = Value::Level(settle_volume(self.in_flight.get(dev).copied().unwrap_or(0), read));`.
   `settle_volume` becomes:
   ```rust
   /// What a volume panel shows for a read: the read plus what is still in flight (`f`,
   /// already net of what this read confirmed), so the bar never drops to a value the
   /// queued changes haven't reached yet. The mute is the read's: a level change doesn't
   /// unmute everywhere (`wpctl set-volume` doesn't).
   fn settle_volume(f: i32, read: &Level) -> Level {
       if f == 0 {
           return read.clone();
       }
       let guess = probe::guess_volume(read, f, false);
       if read.muted { Level { value: guess.value, ..read.clone() } } else { guess }
   }
   ```
   `guess_volume` adds without a clamp:
   ```rust
   // The change goes to the watcher right after: it's in flight until the watcher
   // acknowledges it (always, read or not). Not clamped: the acknowledgement isn't.
   if !mute && change != 0 {
       *self.in_flight.entry(device).or_insert(0) += change;
   }
   ```
   `watch()` loses its `kept`/`in_flight.retain` block. `detach` loses
   `self.in_flight.clear();` (comment: "`in_flight` stays: the watcher still acknowledges
   every queued change.") and gains `self.published_keys.clear();`; `attach` gains it
   after `self.published.clear();`, with "The configurators forgot every panel and key
   state on `Disconnected` (so did the hub's replay): send them all again."
8. `main.rs` `run()`, the no-deck scan publishes nothing on a miss:
   ```rust
   // `Disconnected` goes out once, from `detach` (and the hub's replay
   // starts with it), not on every scan.
   if let Some(d) = deck::open(&mut hid) {
       daemon.attach(d);
   }
   ```
9. Tests: every test `Done` gains `run: p.run_of(&probe)`, every expected tuple wraps its
   value in `Some(..)`. In `watch.rs`'s `mod tests`:
   ```rust
   impl Poller {
       /// The id of the run in flight for `p` (0 if none), for a test's `Done`.
       fn run_of(&self, p: &Probe) -> u64 {
           self.in_flight.get(p).map_or(0, |(id, _)| *id)
       }
   }
   ```
   `fast_spin_does_not_snap_back` becomes: `settle_volume(5, &level(55)).text == "60%"`,
   `settle_volume(0, &level(60)).text == "60%"`, and a muted 40% read with 10 in flight
   stays `muted` and reads "Muted".

The test to copy (`watch.rs` tests):
```rust
#[test]
fn a_leave_window_that_runs_out_sends_the_unchanged_state() {
    // A slow toggle pressed while on, whose state never moves (the service fails to
    // stop): at the end of the window the deck must hear "still on", or it shows
    // busy for good.
    let probe = cmd("state");
    let mut p = Poller::default();
    let mut out = Vec::new();
    let t0 = Instant::now();
    watched(&mut p, probe.clone(), t0, &mut out);
    p.handle(WatchCmd::Done { probe: probe.clone(), run: p.run_of(&probe), result: exit(0, "") }, t0, &mut out);
    out.clear();
    p.handle(WatchCmd::Leave { probe: probe.clone(), from: Toggle::On }, t0, &mut out);
    // Inside the window an unchanged read is not sent…
    let t1 = t0 + FAST_INTERVAL;
    p.run_due(t1, &mut out);
    p.handle(WatchCmd::Done { probe: probe.clone(), run: p.run_of(&probe), result: exit(0, "") }, t1, &mut out);
    assert!(out.is_empty());
    // …but the first read after it ends is, unchanged or not.
    let end = t0 + omadeck_core::probe::FAST_WINDOW;
    p.run_due(end, &mut out);
    p.handle(WatchCmd::Done { probe: probe.clone(), run: p.run_of(&probe), result: exit(0, "") }, end, &mut out);
    assert_eq!(out, vec![(probe, Some(Value::Toggle(Toggle::On)), 0)]);
}
```
Two more in the same style:
- `a_cancelled_runs_done_does_not_end_the_new_run`: watch, note `first = p.run_of(..)`,
  `Watch(vec![])`, watch again; `Done { run: first, result: TimedOut }` changes nothing:
  `run_due` starts nothing and `failing` is empty; the new run's `Done` with exit 0 sends
  `(probe, Some(Toggle::On), 0)`.
- `a_volume_change_is_always_acknowledged`: a `Poller` whose `read_volume` returns `None`:
  an unwatched Input `+5` and a watched Output `+160` give
  `[(Volume(Input), None, 5), (Volume(Output), None, 160)]`.

**Check:** `cargo test -p omadeckd` → `test result: ok. 41 passed`. On the deck:
- [ ] Spin the volume dial hard both ways and stop: the bar ends on the real level
      (compare `wpctl get-volume @DEFAULT_AUDIO_SINK@`), and stays there after 2 s.
- [ ] Unplug the deck mid-spin and plug it back: the bar shows the real level.
- [ ] Mute the mic with its key, unplug and replug: the GUI still shows the key on.
- [ ] `slow = true` toggle with `state = "false"` and an action that does nothing: press;
      busy, then off again after about 15 s.
- [ ] Deck unplugged, GUI open: `RUST_LOG=debug omadeckd` shows no `Disconnected` every
      2 s, and the GUI doesn't flicker.

### Task 6: Hyprland on an ordered worker (R6, R9 use; bug 31)

Files: `crates/omadeckd/src/hypr_worker.rs` (new), `daemon.rs`, `main.rs`, `Cargo.toml`;
`crates/omadeck-core/src/action.rs`.

1. `action.rs`: `hypr_dispatch` becomes `pub`, with the doc "Run one Hyprland dispatcher
   and wait for Hyprland's answer (up to `COMMAND_TIMEOUT`, twice on a Lua-config
   Hyprland). Blocking: the daemon runs it on its Hyprland worker (`omadeckd`'s
   `HyprWorker`), never on the deck loop." `run_steps` is unchanged (its helper thread for
   `steps > 1` stays for `omadeckd dial` with no daemon); its comment says the daemon
   never gets there with a Hyprland action. Delete the stray first doc line of
   `run_tracked` ("Spawn in its own process group with no stdio, reaping it on a helper
   thread.").
2. `hypr_worker.rs`, the whole file:
   ```rust
   //! The Hyprland worker: one background thread that runs every Hyprland dispatch the deck
   //! asks for, in the order asked. `hyprctl dispatch` waits for Hyprland's answer (up to
   //! 2 s, twice on a Lua config, when Hyprland is stalled), so it never runs on the deck
   //! loop. One ordered worker, not a thread per call: a spin's dispatches stay in order
   //! and a stalled Hyprland can't pile up threads. Failures come back to the loop, which
   //! reports them against the key or dial that asked.

   use std::sync::mpsc;
   use std::thread;

   use anyhow::Result;
   use omadeck_core::action;

   use crate::daemon::Source;

   /// One input's dispatch: `dispatch` run `steps` times (a dial's detents), stopping at
   /// the first failure.
   #[derive(Debug, Clone, PartialEq)]
   pub struct HyprJob {
       pub src: Source,
       /// The page the deck showed when the input came in (for `ActionFailed`).
       pub page: Option<String>,
       pub dispatch: String,
       pub steps: u8,
   }

   /// A job that failed, for the loop to publish.
   #[derive(Debug, Clone, PartialEq)]
   pub struct HyprFailed {
       pub src: Source,
       pub page: Option<String>,
       pub message: String,
   }

   pub struct HyprWorker {
       tx: mpsc::Sender<HyprJob>,
       /// Failures, in order; the loop drains them each pass (`Daemon::poll_hypr`).
       pub failed: mpsc::Receiver<HyprFailed>,
   }

   impl HyprWorker {
       pub fn start() -> Self {
           Self::start_with(action::hypr_dispatch)
       }

       /// With the dispatch function swapped out, for tests.
       fn start_with(run: fn(&str) -> Result<()>) -> Self {
           let (tx, jobs) = mpsc::channel::<HyprJob>();
           let (failed_tx, failed) = mpsc::channel();
           // Not joined at shutdown: a queued dispatch may wait 2 s on a stalled Hyprland,
           // and nothing is lost by leaving it. The thread ends when `tx` is dropped.
           thread::Builder::new()
               .name("hypr".into())
               .spawn(move || {
                   for job in jobs {
                       for _ in 0..job.steps.max(1) {
                           if let Err(e) = run(&job.dispatch) {
                               let message = format!("{e:#}");
                               let _ = failed_tx.send(HyprFailed { src: job.src, page: job.page.clone(), message });
                               break;
                           }
                       }
                   }
               })
               .expect("spawn hypr thread");
           HyprWorker { tx, failed }
       }

       /// Queue a job behind the ones already sent. Never blocks.
       pub fn send(&self, job: HyprJob) {
           let _ = self.tx.send(job);
       }
   }
   ```
   plus its test (below). `main.rs`: `mod hypr_worker;`, and `daemon.poll_hypr();` right
   after `daemon.poll_jobs();` in `run()`.
3. `daemon.rs`: `use crate::hypr_worker::{HyprJob, HyprWorker};`; a field after `saver`,
   `/// Runs every Hyprland dispatch off the loop, in order.` `pub hypr: HyprWorker,`,
   set to `HyprWorker::start()` in `new`. `dispatch` takes the source,
   `fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>` (and
   `run_input` calls `self.dispatch(src, act, steps)`), with a new arm before the catch-all:
   ```rust
   Action::Hyprland { dispatch } => {
       // Queued on the worker, in order behind earlier ones: `hyprctl` waits for
       // Hyprland, which must never stall the deck. Only an empty dispatcher fails
       // here; anything else comes back through `poll_hypr`.
       if dispatch.trim().is_empty() {
           anyhow::bail!("no dispatcher set");
       }
       let page = self.nav.current().map(str::to_string);
       self.hypr.send(HyprJob { src, page, dispatch: dispatch.clone(), steps });
       Ok(false)
   }
   ```
   `run_input`'s `Err` arm becomes
   `let page = self.nav.current().map(str::to_string); self.report_failure(src, page, format!("{e:#}")); false`,
   and the `match src` moves into:
   ```rust
   /// Tell the configurators an input's action failed, against the key (on `page`) or
   /// dial it came from.
   fn report_failure(&self, src: Source, page: Option<String>, message: String) {
       log::warn!("{src}: {message}");
       match src {
           Source::Key(index) => self.hub.publish(Event::ActionFailed { index, page, message }),
           Source::Turn(index) | Source::Press(index) | Source::Tap(index) => {
               self.hub.publish(Event::DialActionFailed { index, message })
           }
           Source::Flick => {}
       }
   }

   /// Every pass: report the Hyprland dispatches that failed on the worker.
   pub fn poll_hypr(&mut self) {
       while let Ok(f) = self.hypr.failed.try_recv() {
           self.report_failure(f.src, f.page, f.message);
       }
   }
   ```
4. `crates/omadeckd/Cargo.toml` (R9):
   ```toml
   [dev-dependencies]
   # The bundled test font and `Renderer::with_font_bytes`, so pixel tests don't depend on
   # the machine's fonts.
   omadeck-core = { workspace = true, features = ["test-support"] }
   ```
   and a daemon test that uses it, `a_flipped_press_toggle_paints_its_on_face`: a key
   `label = "A"` with `[key.on] label = "B"`, rendered at 72 px through
   `key_state(&cfg, &rt, None, 0, false)` with
   `Renderer::with_font_bytes(&cfg.style, omadeck_core::render::TEST_FONT)`, differs after
   `rt.flipped.insert(KeyId { page: None, index: 0 })`.

The test to copy (`hypr_worker.rs`):
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    static RAN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn fake(dispatch: &str) -> Result<()> {
        RAN.lock().unwrap().push(dispatch.to_string());
        if dispatch == "bad" { anyhow::bail!("hyprctl: invalid dispatcher") } else { Ok(()) }
    }

    #[test]
    fn jobs_run_in_order_and_failures_come_back() {
        let w = HyprWorker::start_with(fake);
        let job = |src, dispatch: &str, steps| HyprJob { src, page: None, dispatch: dispatch.into(), steps };
        w.send(job(Source::Turn(0), "workspace e+1", 2));
        w.send(job(Source::Key(3), "bad", 3)); // stops at the first failure
        w.send(job(Source::Key(4), "togglefloating", 1));
        let failed = w.failed.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(
            failed,
            HyprFailed { src: Source::Key(3), page: None, message: "hyprctl: invalid dispatcher".into() }
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while RAN.lock().unwrap().len() < 4 {
            assert!(Instant::now() < deadline, "timed out");
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*RAN.lock().unwrap(), ["workspace e+1", "workspace e+1", "bad", "togglefloating"]);
        assert!(w.failed.try_recv().is_err(), "one failure");
    }
}
```

**Check:** `cargo test -p omadeckd` → `test result: ok. 43 passed`; `cargo test -p
omadeck-core` → `141 passed`. `grep -n "run_steps\|hypr_dispatch" crates/omadeckd/src/*.rs`
shows only the catch-all `action::run_steps(a, steps)` in `dispatch`. On the deck:
- [ ] A dial on `workspace e+1` spun hard: workspaces step in order, no stall.
- [ ] `kill -STOP $(pidof Hyprland)` is not safe to try; instead set a key to
      `hyprland` `dispatch = "nosuchdispatcher"`: the GUI toasts "Key N: hyprctl: …"
      within a moment, and the deck stays responsive.

### Task 7: GUI: the toggle editor (R1 GUI; bugs 50, 51, 57, 58, 59, 67)

Files: `crates/omadeck-gui/src/app.rs`, `editor.rs`. Omarchy machine only.

1. `App::select`, first lines:
   ```rust
   pub fn select(self: &Rc<Self>, sel: Option<Selection>) {
       // Another key (or none): its editor starts on the Off tab. The tab outlived its
       // key before, and drew a face the new key doesn't have (BUGS.md 50). PR 12 clears
       // `selected_step` here too.
       if sel != self.selected.get() {
           self.face_tab.set(FaceTab::Off);
       }
       self.selected.set(sel);
   ```
2. `build_key`, after `let key = …;`:
   ```rust
   // A tab this key doesn't offer (its on face was just removed) falls back to Off.
   let tab = key.clamp_face_tab(app.face_tab.get());
   app.face_tab.set(tab);
   ```
   and `appearance_rows(app, &look, i, tab, &key);` at the end.
3. `appearance_tabs` takes the tabs from core and groups the buttons:
   ```rust
   fn appearance_tabs(app: &Rc<App>, group: &adw::PreferencesGroup, key: &KeyConfig) {
       let tabs = key.face_tabs();
       if tabs.len() < 2 {
           return;
       }
       let current = app.face_tab.get();
       let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
       buttons.add_css_class("linked");
       let mut first: Option<gtk::ToggleButton> = None;
       for tab in tabs {
           let label = match tab {
               FaceTab::Off => "Off",
               FaceTab::On => "On",
               FaceTab::Busy => "Busy",
           };
           let b = gtk::ToggleButton::builder().label(label).active(tab == current).build();
           // One group: exactly one tab is active, and clicking the active one keeps it.
           match &first {
               Some(f) => b.set_group(Some(f)),
               None => first = Some(b.clone()),
           }
           let a = app.clone();
           let self_btn = b.clone();
           b.connect_toggled(move |_| {
               if self_btn.is_active() {
                   a.set_face_tab(tab);
               }
           });
           buttons.append(&b);
       }
       group.set_header_suffix(Some(&buttons));
   }
   ```
4. Mirrored states follow the keys. In `App`:
   ```rust
   /// Two keys of the page being edited swapped places: so do their mirrored states,
   /// until the daemon reloads and says what each shows now.
   fn swap_key_states(&self, a: u8, b: u8) {
       let page = self.page.borrow().clone();
       let mut states = self.key_states.borrow_mut();
       let sa = states.remove(&(page.clone(), a));
       let sb = states.remove(&(page.clone(), b));
       if let Some(s) = sa {
           states.insert((page.clone(), b), s);
       }
       if let Some(s) = sb {
           states.insert((page, a), s);
       }
   }
   ```
   `add_dnd`'s drop handler calls `app.swap_key_states(from as u8, i);` right after
   `keys.swap`, before the `refresh_key` calls. `clear_key` first does
   `self.key_states.borrow_mut().remove(&(self.page.borrow().clone(), i));` (before
   `refresh_key`).
5. `build_key`, the State-from combo (bug 57):
   ```rust
   let choices = key.state_choices();
   let names: Vec<&str> = choices
       .iter()
       .map(|c| match c {
           StateChoice::Press => "Flips on each press",
           StateChoice::Command => "Shell command",
           StateChoice::Mute => "Follows mute",
       })
       .collect();
   let list = gtk::StringList::new(&names);
   let state_row = adw::ComboRow::builder().title("State from").model(&list).build();
   // The row is the choice's place in this key's list, not the enum's number.
   state_row.set_selected(choices.iter().position(|c| *c == key.state_choice()).unwrap_or(0) as u32);
   let choices = choices.to_vec();
   let a = app.clone();
   state_row.connect_selected_notify(move |row| {
       let Some(&c) = choices.get(row.selected() as usize) else { return };
       a.edit(i, |k| k.set_state_choice(c));
       a.rebuild_editor();
   });
   ```
6. The State command row saves on Enter, on ✓, and on focus-out (bug 58):
   ```rust
   let cmd = adw::EntryRow::builder()
       .title("State command")
       .text(on.state.as_deref().unwrap_or(""))
       .show_apply_button(true)
       .build();
   ```
   (keep its suffix label). `connect_changed` goes; one `save` closure commits, called
   from `connect_apply` (Enter or ✓) and from a focus controller's `leave`:
   ```rust
   // Saved on Enter, ✓ or focus-out, not per keystroke: the daemon runs whatever is
   // saved, and a half-typed `systemctl --user stop` would run too. Focus-out keeps an
   // edit the user clicked away from.
   let at = app.key_ref(i); // the key it was built for, even if the page changes first
   let saved = Rc::new(RefCell::new(cmd.text().to_string()));
   let save: Rc<dyn Fn(&adw::EntryRow)> = {
       let a = app.clone();
       Rc::new(move |e| {
           let text = e.text().to_string();
           // Unchanged (Enter, then focus moves on): no second edit, no second save.
           if *saved.borrow() == text {
               return;
           }
           saved.replace(text.clone());
           a.edit_ref(&at, |k| {
               if let Some(on) = &mut k.on {
                   on.state = Some(text);
               }
           });
       })
   };
   cmd.connect_apply({
       let save = save.clone();
       move |e| save(e)
   });
   let focus = gtk::EventControllerFocus::new();
   let weak = cmd.downgrade();
   focus.connect_leave(move |_| {
       if let Some(e) = weak.upgrade() {
           save(&e);
       }
   });
   cmd.add_controller(focus);
   ```
   `leave` fires when focus leaves the row or anything inside it (its text field), so a
   click on another row, key or window commits first. The weak reference keeps the
   controller from holding the row alive.
7. Bug 59: the "Busy while running" and "Slow" handlers call `a.rebuild_editor();` after
   their `a.edit`. The "Progress bar" row moves out of `if let Some(on) = &key.on { … }`
   to just after that block (before `page.add(&toggle)`), shown when
   `key.face_tabs().contains(&FaceTab::Busy)`, subtitle "Draw the first percentage of the
   busy text".
8. `service_dialog` (bug 51), whole:
   ```rust
   fn service_dialog(app: &Rc<App>, i: u8) {
       let unit = gtk::Entry::builder().placeholder_text("llm.service").build();
       let url_entry = gtk::Entry::builder().placeholder_text("http://127.0.0.1:8000/health (optional)").build();
       let entries = gtk::Box::new(gtk::Orientation::Vertical, 6);
       entries.append(&unit);
       entries.append(&url_entry);
       // The key it was opened for: a page change on the deck meanwhile must not move it.
       let at = app.key_ref(i);
       let dialog = adw::AlertDialog::builder()
           .heading("Service toggle")
           .body("A slow toggle for a systemd user unit. Optionally check a health URL while it comes up.")
           .build();
       dialog.set_extra_child(Some(&entries));
       dialog.add_responses(&[("cancel", "Cancel"), ("ok", "Create")]);
       dialog.set_close_response("cancel");
       dialog.set_default_response(Some("ok"));
       dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
       let a = app.clone();
       dialog.connect_response(None, move |_, response| {
           if response != "ok" {
               return;
           }
           let u = unit.text().to_string();
           let url = url_entry.text().trim().to_string();
           let url = (!url.is_empty()).then_some(url.as_str());
           // Checked on a scratch key first: a typo must not cost an edit (and a save)
           // of the real key.
           if let Err(e) = omadeck_core::apply_service(&mut KeyConfig::new(0), &u, url) {
               a.toast(&format!("{e:#}"));
               return;
           }
           // In place: the key keeps its index, background and colours.
           a.edit_ref(&at, |k| {
               let _ = omadeck_core::apply_service(k, &u, url);
           });
           a.rebuild_editor();
       });
       // AdwAlertDialog closes itself after a response: no `unparent`.
       dialog.present(Some(&app.window));
   }
   ```

No GUI-free test beyond tasks 1–2's (`face_tabs_follow_the_key`,
`presets_keep_the_key_and_replace_its_icon`); the check is manual.

**Check:** `cargo clippy --all-targets -- -D warnings` clean on the Omarchy machine. In
the app:
- [ ] Select a toggle key, click **On**, click a plain key: no crash, the plain key's
      editor shows no tabs. Back on the toggle key: the Off tab.
- [ ] On the On tab, switch **Two faces** off: no crash, Off tab.
- [ ] Mute the mic with its key (the deck shows on), select it, switch **Two faces** off:
      no crash.
- [ ] Drag a key that is on onto a plain key: both previews right, no crash.
- [ ] Clicking the active face tab keeps it active; only one tab is ever active.
- [ ] The Mic key's "State from" shows **Follows mute**.
- [ ] Type a state command: nothing runs while typing (`journalctl --user -u omadeckd -f`
      shows no 127 warnings). Enter saves it; so does ✓.
- [ ] Type a state command and click another row without Enter: it is saved
      (`config.toml` has it). Change it again and click another key: saved too, on the
      key it was typed for.
- [ ] A plain script key: switch on **Busy while running**: the Busy tab and the
      **Progress bar** row appear at once. Same for **Slow** on a command toggle.
- [ ] **Service…** on key 5 with `http://127.0.0.1:8000/h?a=1&b=2`: key 5 becomes the
      toggle (no key moves to slot 1), its background stays, and the state command in
      config.toml quotes the URL. Cancel closes the dialog with no change.

### Task 8: GUI: safe action targets, hint rows and smaller fixes (R9 GUI; bugs 38, 68, 69, 70, 72)

Files: `crates/omadeck-gui/src/app.rs`, `editor.rs`, `starter.rs`, `Cargo.toml`.
Omarchy machine only.

1. `App::action_at` and `set_action_at` match every target by name:
   ```rust
   /// The action `at` names; `None` if its page has gone.
   pub fn action_at(&self, at: &ActionTarget) -> Option<Action> {
       let cfg = self.config.borrow();
       // One arm per target, no wildcard: a new `ActionTarget` variant (PR 12's `Step`,
       // PR 16's `DialLongPress`) doesn't compile until it is handled here.
       match at {
           ActionTarget::Key(k) => {
               cfg.page(k.page()).map(|keys| keys.get(k.index).map(|kc| kc.action.clone()).unwrap_or_default())
           }
           ActionTarget::KeyOn(k) => cfg.page(k.page()).map(|keys| {
               keys.get(k.index).and_then(|kc| kc.on.as_ref().map(|f| f.action.clone())).unwrap_or_default()
           }),
           ActionTarget::DialTurnLeft(i) => Some(dial_action(&cfg, *i, Direction::Left)),
           ActionTarget::DialTurnRight(i) => Some(dial_action(&cfg, *i, Direction::Right)),
           ActionTarget::DialPress(i) => Some(dial_action(&cfg, *i, Direction::Press)),
       }
   }

   /// Replace the action `at` names. False when it can't: config.toml is broken (both
   /// edit paths drop late results then), the key's page has gone, or an on action's
   /// key is no longer a toggle.
   pub fn set_action_at(self: &Rc<Self>, at: &ActionTarget, act: Action) -> bool {
       match at {
           ActionTarget::Key(k) => self.edit_ref(k, |kc| kc.action = act),
           ActionTarget::KeyOn(k) => {
               // "Two faces" switched off while a picker was open: nothing to set.
               let has_on = self.config.borrow().key(k.page(), k.index).is_some_and(|kc| kc.on.is_some());
               has_on
                   && self.edit_ref(k, |kc| {
                       if let Some(on) = &mut kc.on {
                           on.action = act;
                       }
                   })
           }
           ActionTarget::DialTurnLeft(i) => self.set_dial_action(*i, Direction::Left, act),
           ActionTarget::DialTurnRight(i) => self.set_dial_action(*i, Direction::Right, act),
           ActionTarget::DialPress(i) => self.set_dial_action(*i, Direction::Press, act),
       }
   }

   fn set_dial_action(self: &Rc<Self>, i: u8, dir: Direction, act: Action) -> bool {
       let ok = self.locked.borrow().is_none();
       self.edit_dial(i, |d| *d.action_mut(dir) = act);
       ok
   }
   ```
   and a free function next to `back_key`:
   ```rust
   /// The action in dial `i`'s slot `dir` (`None` for a dial that isn't configured).
   fn dial_action(cfg: &Config, i: u8, dir: Direction) -> Action {
       cfg.dial(i).map(|d| d.action(dir).clone()).unwrap_or_default()
   }
   ```
   (`Config` joins the `omadeck_core::{…}` import if it isn't there.) `ActionTarget::dial_slot`
   stays for its other callers.
2. `editor.rs`, a hint row instead of the group's description (bug 70):
   ```rust
   /// A short explanation under an action's rows. Its own row, not the group's
   /// description: the group belongs to whoever built it (a key's "Home · Row 1 ·
   /// Column 2", the Toggle group, PR 12's step editors).
   fn hint_row(text: &str) -> adw::ActionRow {
       adw::ActionRow::builder()
           .subtitle(text)
           .subtitle_lines(0)
           .use_markup(false)
           .activatable(false)
           .css_classes(["dim-label"])
           .build()
   }
   ```
   In `action_rows`, the Hyprland, Volume and Brightness arms' `group.set_description(Some(X));`
   become `group.add(&hint_row(X));` with the same text.
3. `App::set_strip_background` (bug 38) starts:
   ```rust
   // Like `edit_dial`: a late pick is dropped while config.toml can't be parsed.
   if self.locked.borrow().is_some() {
       return;
   }
   ```
4. `App::handle`, `Msg::DaemonDown` (bug 68):
   ```rust
   Msg::DaemonDown => {
       // Sent after every failed connect, every 2 s while the daemon is stopped:
       // clear and repaint only when it was up.
       let was_up = self.daemon_up.replace(false);
       self.daemon.borrow_mut().take();
       self.banner.set_revealed(true);
       if !was_up {
           return;
       }
       self.connected.set(false);
       self.live_brightness.set(None);
       self.panel_status.borrow_mut().clear();
       self.key_states.borrow_mut().clear();
       let b = self.config.borrow().brightness;
       self.sync_slider(b);
       self.refresh_all(); // the strip too
   }
   ```
5. `starter.rs` `layout` (bug 69):
   ```rust
   // The two presets that work everywhere, by name: the output mute toggle (the on
   // face follows the mute) and the night light (a real state command, when jq is
   // there; a plain key otherwise, or its state command fails every poll).
   let preset = |name: &str| omadeck_core::PRESETS.iter().find(|p| p.name == name).expect("a built-in preset");
   let mut mute = KeyConfig::new(10);
   (preset("Output mute").apply)(&mut mute);
   let night_light = preset("Night light");
   let night = if night_light.needs.is_none_or(omadeck_core::on_path) {
       let mut k = KeyConfig::new(11);
       (night_light.apply)(&mut k);
       k
   } else {
       key(11, "Night light", None, app("omarchy-toggle-nightlight"))
   };
   ```
6. `build_key`, the on slot's `on_new_page` (bug 72):
   ```rust
   // A folder for the on action only: the key keeps its own look (`new_folder`
   // would make the key itself look like a folder).
   on_new_page: Rc::new(move |a| {
       let label = a.key(i).map(|k| k.label).unwrap_or_default();
       a.add_page(if label.trim().is_empty() { "Page" } else { &label }, true)
   }),
   ```
7. `crates/omadeck-gui/Cargo.toml` (R9), the same `[dev-dependencies]` block as task 6's,
   so a GUI pixel test can use `Renderer::with_font_bytes(…, omadeck_core::render::TEST_FONT)`.

No GUI-free test; the check is manual.

**Check:** `cargo clippy --all-targets -- -D warnings` and `cargo test` green on the
Omarchy machine (`cargo test -p omadeck-gui` → its 3 `media` tests). In the app:
- [ ] A key set to Volume keeps "Home · Row 1 · Column 2" under its title; the volume
      explanation is a dim row.
- [ ] `systemctl --user stop omadeckd` with the GUI open: the banner shows, and the
      previews don't repaint every 2 s (`GTK_DEBUG=updates` shows no flashes).
- [ ] Break config.toml, then finish a strip-background pick: nothing changes, and
      fixing the file shows the old background.
- [ ] A toggle's "When pressed while on" → Open page → **New page**: a folder is made, and
      the key's Off face keeps its glyph and label.
- [ ] With `jq` renamed away, a fresh starter layout's key 11 is a plain Night light key.

### Task 9: docs and comments (bug 62; doc nits)

Files: `README.md`, `examples/config.toml`, `CHANGELOG.md`,
`crates/omadeck-core/src/render.rs`, `config.rs`, `crates/omadeckd/src/daemon.rs`.

- `examples/config.toml`, the Backup example: move `# busy_while_running = true` above
  `# [key.action]` (after `# label = "Backup"`), so TOML doesn't read it as part of
  `[key.on]`. The toggle comment: "its **first** output line is the progress"; the
  `[key.busy]` line: `# bar = true   # draw a progress bar from the first "NN%" in the
  output`; the interval line: `# seconds between runs (at least 1; the configurator
  offers 1–60)`. Add to the Night light example: `# action = { type = "app", command =
  "…" }   # optional: run this instead while on (Elgato's Hotkey Switch)`.
- `README.md` "Toggle keys": "(1–60, default 2)" → "(at least 1, default 2)"; "the
  command's last output line" → "the command's first output line" for state commands;
  `busy_while_running`'s "the command's last output line" stays (it is the latest line);
  a new paragraph: "`[key.on] action` runs instead of the key's action while it's on
  (Elgato's Hotkey Switch); unset, a press runs the key's own action." The restart
  sentence becomes: "the command keeps running if the daemon restarts (it runs in its own
  systemd scope), but the key stops showing it, and a program that writes progress may
  end when nothing reads it any more."
- `CHANGELOG.md`, `## [Unreleased]`: in the Toggle keys entry, "the last `NN%`" → "the
  first `NN%`"; under `### Fixed` (add it if missing):
  ```markdown
  - The configurator no longer crashes when a toggle key loses its on face while on.
  - Renaming or deleting a page also updates a toggle's on action that opens it.
  - The Service… preset keeps the key where it is, and quotes its health URL.
  - A slow toggle whose state never changes leaves busy after 15 seconds.
  - The volume bar can no longer stay off the real level after a failed read or a replug.
  - After a replug the configurator still shows which keys are on.
  - Hyprland actions never hold up the deck.
  - A brightness set on the deck survives a restart after any configurator edit.
  - A busy bar needs a real percentage (`43%`); `1/4` no longer draws one.
  - Pressing a volume or brightness dial colours only its title.
  ```
- `render.rs`: the `BRIGHTNESS_GLYPH` comment shows U+F00DE (it shows U+C07CE today);
  `no_images_pixels_unchanged` keeps only its second doc paragraph; `KeyStatus.pressed`
  ("Drawn with the press ring.") and `.state` ("Off, on or busy; drawn by
  `KeyConfig::face_for`.") get docs.
- `daemon.rs`: drop `#[allow(dead_code)]` on `key_state` and "Task F's paint path calls
  it." from its doc.

**Check:** `cargo test -p omadeck-core` → `141 passed` (`example_parses` still passes);
`cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` green on the
Omarchy machine; `grep -n "last output line" README.md examples/config.toml` shows only
the `busy_while_running` sentence; uncomment the Backup block in a scratch copy of the
example and `cargo run -p omadeckd -- snapshot /tmp/s.png` loads it (no parse error).

## After the branch

- Mark 50–64 and 66–72 fixed in BUGS.md with each task's commit, and 25, 31, 38, 45 and
  48 fully fixed. 65 stays open for PR 13.
- Run CI by hand on the branch (capture step 1) before PR 9 starts.
- PR 9 is based on this branch; it uses the names below.

## Names later plans use

As built in the scratch copy (task in brackets). `Daemon::reload(&mut self)` and
`Daemon::handle_commands(&mut self)` are unchanged.

- **R1** [1, 7]: `KeyConfig::face_tabs(&self) -> Vec<FaceTab>`,
  `KeyConfig::clamp_face_tab(&self, tab: FaceTab) -> FaceTab`. `App::select` starts with
  `if sel != self.selected.get() { self.face_tab.set(FaceTab::Off); }` before
  `self.selected.set(sel)`: that branch is where PR 12 clears `selected_step`.
- **R2** [1]: the recursive helper is `Action::walk<'a>(&'a self, f: &mut dyn FnMut(&'a
  Action))` and `Action::walk_mut(&mut self, f: &mut dyn FnMut(&mut Action))` (PR 12's
  `Multi` arm goes in both). `KeyConfig::actions(&self) -> Vec<&Action>`,
  `KeyConfig::actions_mut(&mut self, f: &mut dyn FnMut(&mut Action))`,
  `DialConfig::actions`/`actions_mut` (same shapes; PR 16's `long_press` goes there),
  `Config::all_actions(&self) -> Vec<&Action>` and `Config::all_actions_mut(&mut self, f:
  &mut dyn FnMut(&mut Action))`, both now `pub`. `links_to`, `rename_page` and
  `remove_page` still use `all_actions(_mut)`. `Direction::ALL: [Direction; 3]`.
- **R3** [4]: `State.config_brightness: Option<u8>`, `State::config_edit(&self, cfg: u8)
  -> Option<u8>`, `State::start_brightness(&self, cfg: u8) -> u8`. `Runtime::load` is gone:
  `Runtime::start(state: State, cfg_brightness: Option<u8>) -> Runtime`.
  `Daemon::new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver:
  StateSaver) -> Daemon`.
- **R4** [2]: `Value` is `#[serde(rename_all = "snake_case")]`.
- **R5** [5]: `WatchCmd::Done { probe: Probe, run: u64, result: RunResult }`.
  `Poller.in_flight: HashMap<Probe, (u64, Arc<AtomicBool>)>`, `Poller.next_run: u64`.
  `Poller::run_due(&mut self, now: Instant, out: &mut Vec<(Probe, Option<Value>, i32)>) ->
  Vec<(Probe, u64, Arc<AtomicBool>)>`; `handle` and `record` take the same `out`.
  `Watcher.values: mpsc::Receiver<(Probe, Option<Value>, i32)>`.
  `Daemon::apply_value(&mut self, probe: Probe, value: Option<Value>, applied: i32)`.
  `settle_volume(f: i32, read: &Level) -> Level`. In tests: `Poller::run_of(&self, p:
  &Probe) -> u64`.
- **R6** [6]: module `omadeckd/src/hypr_worker.rs`. `pub struct HyprWorker { tx, pub failed:
  mpsc::Receiver<HyprFailed> }`, `HyprWorker::start() -> HyprWorker`,
  `HyprWorker::send(&self, job: HyprJob)`. `pub struct HyprJob { pub src: Source, pub
  page: Option<String>, pub dispatch: String, pub steps: u8 }`; `pub struct HyprFailed {
  pub src: Source, pub page: Option<String>, pub message: String }`. `Daemon.hypr:
  HyprWorker` (its thread is named `hypr`; PR 9's event listener is a separate module,
  `hypr.rs`, on `Daemon.hypr_events`). Results are drained by `Daemon::poll_hypr(&mut
  self)`, called in `main.rs` `run()` right after `daemon.poll_jobs()`. `run_input`'s
  `match src` lives in `Daemon::report_failure(&self, src: Source, page: Option<String>, message: String)`:
  PR 11's `Source::Ctl` and PR 12's `Source::Step` add their arms there.
  `Daemon::dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>`
  gains `src`. `action::hypr_dispatch(dispatch: &str) -> Result<()>` is `pub`.
  `action::run_steps` still runs Hyprland `steps > 1` on a helper thread (offline
  `omadeckd dial` and PR 11's `dial_locally` rely on it); the daemon never calls it for
  Hyprland.
- **R7** [2]: `probe::progress(text: &str) -> Option<u8>`, first explicit `NN%`.
- **R8** [3]: `render_strip_panel` computes `fg` (base) and `title` (accent when pressed);
  PR 10's per-rect tones and PR 14's `M1Colors` copy that split.
- **R9** [2, 3, 6, 8]: `watched_probes(cfg: &Config, page: Option<&str>, n_dials: usize,
  n_keys: u8) -> Vec<Probe>`. Core feature `test-support`;
  `Renderer::with_font_bytes(style: &Style, data: &[u8]) -> Renderer` and
  `omadeck_core::render::TEST_FONT: &[u8]`, both `#[cfg(any(test, feature =
  "test-support"))]`; dev-dependency `omadeck-core = { workspace = true, features =
  ["test-support"] }`. `App::action_at`/`set_action_at`: one arm per `ActionTarget`
  variant (as in task 8), dials through `dial_action(cfg, i, dir)` and
  `App::set_dial_action(self: &Rc<Self>, i, dir, act) -> bool`; PR 12 and PR 16 add their
  arms there. `action_rows` puts its explanation in `hint_row(text) -> adw::ActionRow`,
  added to the group as a row; it never calls `set_description`.
- **Existing test PR 10 must extend:** `action::tests::run_refuses_exactly_the_deck_owned_kinds`
  iterates `0..Action::KINDS.len()` and skips kind 8 (Volume, which would really change
  the volume). PR 10's kind 11 (App volume) runs `pactl`, so it must be skipped too.
