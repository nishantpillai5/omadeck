# What to verify on the Omarchy machine

Part of [PLAN.md](PLAN.md). From `fix/review-2` on, the branches were built on a Mac
with no deck and no gtk4. On the Mac:

- core and daemon were built and tested (`cargo test`, `cargo clippy --all-targets -- -D
  warnings`, `cargo fmt --check`). The hub socket tests can't bind a socket in the
  sandbox, so they still need a run on the machine (two up to PR 10; from PR 11 also
  `command_reply_over_socket`).
- Where a plan asked for a capture from the machine (real `hyprctl`, `pactl`,
  `playerctl` output, …), nothing was invented: tests use hand-written input in the
  documented format, and each section lists the capture and the default that was
  assumed. Do the captures first; swap them into the tests where the section says.
- **The GUI was type-checked, not built.** With a stub `pkg-config` (see
  [review-2026-10-17c.md](review-2026-10-17c.md)), `cargo clippy -p omadeck-gui
  --all-targets` checks it against the real gtk4/libadwaita crates without linking. Every
  branch tip has 0 errors. Clippy warnings make `-D warnings` fail from `feat/live-keys` on
  (BUGS.md 78). Linking, the GUI tests and every click still need the machine.
- Nothing was run on the hardware, and nothing in the GUI was clicked.

Final test counts on `feat/strip-long-press`: core 265, omadeckd 101 (98 here + 3
socket tests). Each section gives its own branch's counts.

Review 3 (BUGS.md 73–104) found bugs on these branches. Each section lists its own; fix
73, 74 and 78 before the machine pass (see the review's "What next").

Work through each branch in order. Tick an item when it passes. When one fails, add the
bug to BUGS.md and leave a link to it here.

## Every branch, first

```bash
systemctl --user stop omadeckd
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
```
Then CI on the fork:
`gh workflow run ci.yml --ref <branch> -R nishantpillai5/omadeck`, then
`gh run list --workflow ci.yml -R nishantpillai5/omadeck --limit 1`.

## `fix/review-2` (review-fixes-2.md)

Review 3 bugs brought in here (BUGS.md): 90, 92 (doc comment), 102, 104 (older).

Commits: `b1cdf93` (tasks 1–6), `8b33817` (tasks 7–8, GUI, not compiled), `f309370`
(task 9).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included.
- [ ] `cargo test`: core 141, omadeckd 43 (the hub socket tests too), gui 3.
- [ ] `cargo clippy --version` matches CI (0.1.95 or newer), and the CI run is `success`.

Task 4 (brightness):
- [ ] Turn the brightness dial to 20, relabel a key in the GUI, then
      `systemctl --user restart omadeckd`. The deck comes back at 20, and `state.toml`
      has `brightness = 20` and `config_brightness = <the config's value>`.
- [ ] Stop the daemon, edit `brightness = 35` in config.toml, start it: 35.
- [ ] With the daemon running, edit `brightness = 45`: the deck dims within a second.

Task 5 (watcher):
- [ ] Spin the volume dial hard both ways, then stop. The bar ends on the real level
      (`wpctl get-volume @DEFAULT_AUDIO_SINK@`) and is still there 2 s later.
- [ ] Unplug the deck mid-spin and plug it back: the bar shows the real level.
- [ ] Mute the mic with its key, then unplug and replug: the GUI still shows the key on.
- [ ] A `slow = true` toggle with `state = "false"` and an action that does nothing.
      Press it: busy, then off again after about 15 s.
- [ ] Deck unplugged, GUI open: `RUST_LOG=debug omadeckd` shows no `Disconnected` every
      2 s, and the GUI doesn't flicker.

Task 6 (Hyprland worker):
- [ ] A dial on `workspace e+1`, spun hard: workspaces step in order, with no stall.
- [ ] A key with `dispatch = "nosuchdispatcher"`: the GUI toasts "Key N: hyprctl: …"
      within a moment, and the deck stays responsive.

Task 7 (toggle editor, GUI):
- [ ] Select a toggle key, click **On**, then click a plain key: no crash, and the plain
      key's editor shows no tabs. Back on the toggle key, the Off tab is shown.
- [ ] On the On tab, switch **Two faces** off: no crash, and the Off tab is shown.
- [ ] Mute the mic with its key (the deck shows on), select it, and switch **Two faces**
      off: no crash.
- [ ] Drag a key that is on onto a plain key: both previews are right, and no crash.
- [ ] Clicking the active face tab keeps it active, and only one tab is ever active.
- [ ] The Mic key's "State from" shows **Follows mute**.
- [ ] Type a state command: nothing runs while you type (`journalctl --user -u omadeckd
      -f` shows no 127 warnings). Enter saves it, and so does ✓.
- [ ] Type a state command and click another row without pressing Enter: it is saved.
      Change it again and click another key: it's saved on the key it was typed for.
- [ ] A plain script key: switch on **Busy while running**. The Busy tab and the
      **Progress bar** row appear at once. Same for **Slow** on a command toggle.
- [ ] **Service…** on key 5 with `http://127.0.0.1:8000/h?a=1&b=2`: key 5 becomes the
      toggle (no key moves to slot 1), its background stays, and config.toml's state
      command quotes the URL. Cancel closes the dialog with no change.

Task 8 (GUI):
- [ ] A key set to Volume keeps "Home · Row 1 · Column 2" under its title, and the
      volume explanation is a dim row.
- [ ] `systemctl --user stop omadeckd` with the GUI open: the banner shows, and the
      previews don't repaint every 2 s (`GTK_DEBUG=updates`).
- [ ] Break config.toml, then finish a strip-background pick: nothing changes. Fixing
      the file shows the old background.
- [ ] On a toggle, set "When pressed while on" to Open page → **New page**: a folder is
      made, and the key's Off face keeps its glyph and label.
- [ ] With `jq` renamed away, a fresh starter layout's key 11 is a plain Night light key.

Task 9 (docs):
- [ ] Uncomment the Backup block in a scratch copy of `examples/config.toml`; `cargo run
      -p omadeckd -- snapshot /tmp/s.png` loads it. (On the Mac, its TOML was checked to
      parse as one `[[key]]` with `busy_while_running` and `[key.busy] bar = true`.)

Not done on the Mac, deliberately:
- `dial_slot` has no caller until PR 10, so it carries `#[allow(dead_code)]`. PR 10
  removes the allow.

## `feat/live-keys` (pr9-live-keys.md)

Review 3 bugs brought in here (BUGS.md): 78 (clippy), 81, 87, 97.

Commits: `837c11e` (task A), `cdbaba1` (B), `056fd8a` (C), `faa2562` (D), `635a384` (E),
`91167cd` (F), `969ab3a` (G, GUI, not compiled), `ab0df79` (H).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included.
- [ ] `cargo test`: core 179, omadeckd 60 (the hub socket tests too), gui 3.
- [ ] `git diff --stat fix/review-2 -- Cargo.lock` shows only lines added under `jiff`
      (`windows-link`, already locked) and `omadeck-core` (`jiff`), and the
      `omadeckd` line for `serde_json`: no new `[[package]]`.

Captures (the plan's "Capture on the machine first"; none were run):
- [ ] 1. Record the socket2 transcript (plan's command), add it as
      `crates/omadeckd/tests/fixtures/socket2.txt` and write the
      `transcript_gives_events` test. Assumed for now: event addresses may or may not
      have `0x` (both are normalised), and `activewindowv2` with nothing focused sends
      an empty address or `,` (both mean no window). The test
      `documented_lines_give_events` uses the wiki's forms instead.
- [ ] 2. `j/activeworkspace` / `j/activewindow` JSON matches what `hypr.rs` reads
      (`id`, `name`; `address`, `class`, `title`, `pid`; `{}` with nothing focused).
- [ ] 3. `systemctl --user show-environment` has `XDG_RUNTIME_DIR` (without it the
      listener doesn't start) and `ls $XDG_RUNTIME_DIR/hypr/` shows the instance.
- [ ] 4. Battery sysfs layout matches the `battery_in` tests (`type`, `scope`,
      `status`, `capacity`, `energy_now`/`energy_full`).
- [ ] 5. `/proc/stat` and `/proc/meminfo` samples parse (CPU and memory keys show a %).
- [ ] 6. The seven glyphs (`f0150 f061a f035b f0079 f0084 f056e f018d`) render as a
      clock, chip, memory, battery, charging battery, workspace and console. Change a
      constant in `render.rs` only if it shows the wrong symbol.
- [x] 7. jiff `%n` gives a newline (`clock_formats_and_ticks` passes on the Mac).

Task F (daemon, on the hardware):
- [ ] A `%H:%M` clock key turns over within ~0.15 s of the minute (compare with
      Waybar); a `%H:%M:%S` key ticks every second without skipping.
- [ ] After suspend and resume the clock is right within 5 s.
- [ ] `timedatectl set-timezone …` moves the clock (how fast jiff notices: unconfirmed).
- [ ] CPU follows btop roughly; memory and battery match `free` and Waybar; plugging
      the charger shows the charging glyph within 5 s.
- [ ] Workspace key follows `SUPER+1..5`, a click on another monitor's window, and a
      rename; `–` when the daemon runs outside Hyprland (TTY).
- [ ] Restarting Hyprland reconnects the listener with the new signature.
- [ ] A command key with `sleep 30` doesn't delay the clock; its run is killed at 5 s
      (default interval).
- [ ] Spinning a volume dial hard while a seconds clock runs: the clock may stall for
      a moment but catches up.
- [ ] Pressing a live key runs its action; the pressed ring shows over the value. A
      live key with `busy_while_running` shows the busy face while its command runs,
      then the value again.
- [ ] A live command key (e.g. `playerctl status`) updates right after a press.
- [ ] Battery and clock dials draw `$B1` and `$A1` correctly on the strip.
- [ ] No visible CPU cost from the daemon with a seconds clock (`top`).

Task G (GUI):
- [ ] Pick each source on a key and on a dial: the preview shows `–` with the daemon
      stopped, and the live value with it running, within a second.
- [ ] The clock's example row follows typing in Format, and focus stays in the entry.
- [ ] A live command is saved on Enter, ✓ or focus-out, not while typing.
- [ ] The On tab of a live toggle key still shows the value.

Task H (docs):
- [ ] Uncomment the live keys and the battery dial in a scratch copy of
      `examples/config.toml`; `cargo run -p omadeckd -- snapshot /tmp/s.png` loads it.
      (On the Mac the live key block was checked to parse as TOML, and
      `example_dial_parses` uncomments the dial.)

Assumptions and changes from the plan:
- `Cargo.lock` gained two lines for jiff, not one: its `tz-system` feature adds
  `windows-link` (already in the lock, Windows only). No package was added, so jiff
  stays (no libc fallback).
- `bad_format_is_question_mark` uses `"%H:%"`: jiff reads `%Q` as the zone name.
- The example's live keys use indices 7–9 (the plan's 4 and 6 are taken by real keys).
- The clock Format entry saves per keystroke (it runs nothing, like a label); the
  Command entry saves on Enter or focus-out, like a state command.
- With an icon or glyph, a clock's `%n` is drawn as a space (the value is one line
  under the icon).

## `feat/app-volume` (pr10-app-volume.md)

Review 3 bugs brought in here (BUGS.md): 77, 80 (pactl re-read), 96, 100.

Commits: `39e2937` (task A), `34c6918` (B), `66dae21` (C), `528ab06` (D), `cb49c14` (E),
`b872a7f` (F), `554812b` (G), `7ba769e` (H, GUI, not compiled), `f044473` (I).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 206, omadeckd 68 (the hub socket tests too), gui 3.
- [ ] `git diff --stat feat/live-keys -- Cargo.lock` is empty (no new crates).

Captures (the plan's "Capture on the machine first"; none were run):
- [ ] 1. Save `pactl -f json list sink-inputs` per case under
      `crates/omadeck-core/tests/fixtures/sink-inputs-<case>.json` and add the
      `pactl_fixtures_parse` test (not written: there was nothing to read).
      Assumed for now: `object.id` and pids may be strings or numbers (both parse),
      `value_percent` is `"NN%"`, `mute`/`corked` are booleans. `pactl_streams_parse`
      and `pactl_stream_fallbacks` use hand-written JSON.
- [ ] 2. `object.id` is the node id `wpctl` takes (not `index` or `object.serial`).
- [ ] 3. `wpctl set-volume -l 1.0 <id> 5%+` and `wpctl set-mute <id> 1` work on a
      stream node.
- [ ] 4. Save `hyprctl -j activewindow` (Firefox, and an empty workspace) as
      `activewindow.json` / `activewindow-empty.json` and point
      `hyprctl_window_parses` at them (it uses hand-written JSON now; `{}` and a
      `pid` of -1 are assumed for "nothing"). Check Chromium's and `mpv`-in-a-terminal's
      stream pids descend from the window pid.
- [ ] 5. Icons: the hicolor/pixmaps/`Icon=` lookup finds Spotify, Firefox, Chromium, mpv.
- [ ] 6. Versions (`pactl`, `pipewire`, Omarchy).
- [ ] 7. `f08c6` renders as an application window in the Omarchy terminal (`APP_GLYPH`).

Hardware (tasks F, G):
- [ ] Spotify dial: turn ±5% matches Omarchy's audio panel slider; a fast spin sends
      one change, never above 100%.
- [ ] Press mutes Spotify only (the system volume stays); the panel shows `Muted`
      within a moment.
- [ ] Spotify not running: dimmed, Spotify's icon, `–`. Start it and play: lights up
      within ~1 s.
- [ ] Spotify paused (corked): still shows its level, turning still works.
- [ ] Firefox with two playing tabs: both change, the panel shows the louder. Chromium
      the same (audio from a child process).
- [ ] Focused dial: focus Firefox → Firefox. Focus a silent terminal → still Firefox
      (sticky). Focus a terminal playing `mpv` → mpv.
- [ ] Focused dial right after daemon start on a silent window: dimmed, the window's
      class, `–`.
- [ ] Alt-tab quickly: the panel follows without a blink; daemon CPU ~0% at idle.
- [ ] A Flatpak app (if any) matches by class.
- [ ] Holding the dial turns only the title to the accent colour.
- [ ] A strip image that's dark under the title and light under the value: each text
      picks its own contrast colour.
- [ ] `omadeckd press` / `dial` with no daemon on a chosen-app action changes it (the
      `app-volume` thread).

Task H (GUI):
- [ ] It compiles: check first `apps.rs` `pick_audio` and `audio_key`, `editor.rs`
      `dial_presets_button` and the `Action::AppVolume` arm of `action_rows`.
- [ ] Presets menu: all four presets fill all three actions; App volume dial… opens the
      picker first and changes nothing if it's closed.
- [ ] The picker lists what's playing (deduplicated, with icons); Installed apps… works.
- [ ] Choose… on a dial with no label sets the label; a key's label is left alone.
- [ ] Typing in App keeps the focus and updates the Application subtitle.
- [ ] The strip preview follows the deck while the daemon runs; without it, an app dial
      is dimmed with its label or the app's name.
- [ ] ▶ is off for a focused action, with the tooltip.

Task I (docs):
- [ ] Uncomment the two app volume dials in a scratch copy of `examples/config.toml`;
      `cargo run -p omadeckd -- snapshot /tmp/s.png` loads it.

Assumptions and changes from the plan:
- No fixtures were captured, so `pactl_fixtures_parse` doesn't exist yet and
  `hyprctl_window_parses` uses hand-written JSON (captures 1 and 4 above).
- The watcher's icon lookup is a `Poller` fn field too (`find_icon`, default
  `audio::app_icon`), so tests don't read the Mac's or Linux's data dirs.
- Extra tests: `pactl_stream_fallbacks` (audio) and
  `failed_read_sends_nothing_and_retries` (watcher).
- Task F was committed with a temporary `#[allow(dead_code)]` on `WatchCmd` (its two
  new variants are only sent from task G, which removes it), to keep each commit
  clippy-clean.
- `audio_key` (Installed apps…) stores the executable's name, except for launchers
  (`flatpak`, `env`, `sh`, `bash`, `gtk-launch`, `uwsm-app`), where it stores the
  desktop id without `.desktop` so Flatpak apps match by app id.
- The two new presets also clear the dial's `live` (a live value would hide the app
  panel); Volume dial and Brightness dial are moved unchanged.
- `resolve_focused` only makes a window's app sticky when its key isn't empty.
- `run_commands` now writes the `exec …` log line (moved from `apply_volume`).
- The example's app dials use indices 1 and 2 ("in place of" the volume and battery
  dials above).

## `feat/ctl` (pr11-ctl.md)

Review 3 bugs brought in here (BUGS.md): 74, 86, 88, 91.

Commits: `dbe1ae3` (task 1), `32390eb` (2), `a617ec6` (3), `b35a507` (4), `b70ad8a` (5),
`68df866` (6, GUI and docs, GUI not compiled).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 211, omadeckd 75, gui 4 (`editor::tests::try_it_routes`; its
      `route` logic was run once on the Mac in a scratch copy inside core and passed).
- [ ] The socket tests, which the Mac sandbox can't bind: `hub::tests::new_client_gets_the_replay`,
      `hub::tests::a_client_that_never_reads_does_not_block_publish` and the new
      `hub::tests::command_reply_over_socket` (it also checks the 0600 mode). Each failed
      here only with `Operation not permitted` on bind.
      `hub::tests::a_full_queue_disconnects_instead_of_dropping_the_reply` uses a socket
      pair and passes here.
- [ ] `git diff --stat feat/app-volume -- Cargo.lock` is empty (no new crates).

Captures (none were run):
- [ ] `ls ~/.config/hypr/ && head -30 ~/.config/hypr/bindings.conf`: the file exists and
      uses `bindd`. The README's binds block assumes so; if this Hyprland uses a Lua
      config, swap in its syntax.
- [ ] `ls -l $XDG_RUNTIME_DIR/omadeck.sock` on the previous build, then on this one
      (`srw-------`).

Task 5 (CLI), by hand:
- [ ] `omadeckd state` with the service stopped prints `omadeckd is not running
      (systemctl --user start omadeckd)` and exits 1 (checked on the Mac with no
      `XDG_RUNTIME_DIR` and with a missing socket; usage errors exit 2).

Hardware (Stream Deck +, 0fd9:0084, Omarchy):
- [ ] `omadeckd page next` / `prev` / `home` / `page Media` switch pages; the
      configurator follows; `page Nope` prints an error and exits 1.
- [ ] `omadeckd brightness 20`, `+10`, `-10`: the deck dims live, a brightness dial's bar
      and the settings slider follow, and the value survives a service restart.
- [ ] With the deck unplugged, `omadeckd brightness 30`, then plug in: it comes up at 30.
- [ ] A held `binded` brightness key: smooth, no backlog after release.
- [ ] `omadeckd press 3` with the daemon up and the configurator open on key 0: key 3
      flashes, the action runs, and the configurator stays on key 0. A page key pressed
      this way switches page. A toggle key's face flips on the deck and in the preview.
- [ ] `omadeckd press 3 Other` for a page the deck isn't showing: the action runs, no
      flash, no flip.
- [ ] `omadeckd press 3` with the service stopped: runs locally as before.
- [ ] `omadeckd dial 0 right 5` moves the volume bar five steps; `dial 0 press` flashes the
      panel and mutes. The configurator's selection doesn't move; its strip preview follows.
- [ ] A key, a dial turn, a dial press and a strip tap on the deck behave as before, and a
      key pressed on the deck still selects it in the configurator.
- [ ] `omadeckd reload` after a deliberate typo prints the parse error; the deck keeps the
      last good config.
- [ ] `omadeckd state | jq .page` with the deck plugged and unplugged.
- [ ] A fast dial spin while running `for i in $(seq 50); do omadeckd state >/dev/null || echo fail; done`:
      no "fail" lines (and if one appears, it says "daemon busy", never a 3 s hang).
- [ ] An old configurator build still shows status and key presses against the new daemon.

Task 6 (GUI):
- [ ] It compiles: check first `editor.rs` `test_button` (the `gio::spawn_blocking`
      closures and `toast_reply`'s generic error type) and `route`, and `app.rs`
      `flush_save`.
- [ ] *Try it now* on a Go back key and a brightness dial acts on the deck; with the
      service stopped it toasts. A Hyprland key's ▶ doesn't freeze the window.
- [ ] A toggle key's header ▶ flashes the key and flips it, as the deck does; the Toggle
      group's new ▶ runs the on action without flipping. Change a key's command and click
      ▶ at once: the new command runs. With the deck unplugged, ▶ still runs the action.

Assumptions and changes from the plan:
- The capture items weren't run: the README's binds use `bindd`/`binded` as the plan
  says.
- A ctl press that moves the page (a page or Back key) doesn't flash: the flash would
  land on the new page's key at that place. Every other full ctl press flashes.
- `Dial` replies say `dial 0 right x5: …`, or `dial 0 right: …` for one step; a press
  never shows a count. `dial_locally` prints the same shape.
- `page next`/`prev`/`previous` are matched in lower case only (so a page named `Next`
  is still reachable); `home` in any case.
- `handle_command` answers a stray `ShowPage`/`SetBrightness` with an empty `Ok`, for
  exhaustiveness; `handle_commands` never passes them in.
- Task 3 was committed with a temporary `#[allow(dead_code)]` on `Source::Ctl` (task 4
  removes it), to keep each commit clippy-clean.
- `flush_save` returns true whenever a save was pending, even when `save_now` reloaded
  from disk instead of writing; the extra `Reload` is harmless.
- The README's Command line section lists the new verbs, the `state` JSON and the binds;
  "Event feed" became "Event feed and control".

## `feat/multi-actions` (pr12-multi-actions.md)

Review 3 bugs brought in here (BUGS.md): 93.

Commits: `4ff79ca` (task 1), `5fb32bf` (2), `14d5212` (3), `2ea3b81` (4), `8e0581f` (5, GUI
not compiled), `db97dfa` (6, GUI and docs, GUI not compiled).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 220 (+9: 8 in `config::`, `action::tests::multi_blocking_collects_errors`),
      omadeckd 85 (+10: 9 in `sequence::`, `step_and_ctl_sources_name_themselves`), gui 7
      (+3: `app::tests::step_targets_compare_by_parent_and_index`,
      `editor::tests::selection_follows_the_step`, `editor::tests::multi_routes`).
      `selection_after` was run once on the Mac in a scratch copy inside core and passed.
- [ ] The three socket tests still fail here only on bind (`Operation not permitted`), as
      for `feat/ctl`; no new socket test.
- [ ] `git diff --stat feat/ctl -- Cargo.lock` is empty (no new crates).

Task 4 (daemon), by hand:
- [ ] With the README's "Meeting" key on a free index and a `Meeting` page, `omadeckd press
      <i>` runs every step; the log shows `key <i> step 4: hyprctl dispatch movetoworkspace 5`
      about 2 s after step 2.
- [ ] `omadeckd press <i>` with the service stopped runs the steps here (the 2 s delay
      included), prints `step N: …` for a failed step and exits 1; a multi with a page step
      exits 1 with "only work on the deck".

Hardware (Stream Deck +, 0fd9:0084, Omarchy):
- [ ] The "Meeting" example: the mic mutes (OSD shows), Zoom launches, 2 s later the window
      moves to workspace 5, and the deck shows page Meeting.
- [ ] Pressing the key again during the 2 s delay does nothing (log: ignored); after it ends
      it runs again.
- [ ] Other keys and dials stay responsive during a 10 s delay (turn the volume dial).
- [ ] `brightness set = 20`, `delay 1000`, `brightness set = 80`: the deck dims and comes back;
      a brightness dial's panel follows both.
- [ ] Volume `-50`, `+50` back to back on a dial press: the level ends where it started.
- [ ] Two Hyprland steps back to back (`workspace 3`, `workspace 4`): they land in that
      order, and the deck stays responsive while Hyprland answers.
- [ ] A multi on a dial press and on a strip tap runs. A failing step toasts "Dial 2: step 1:
      script not found …" and the next step still runs.
- [ ] `omadeckd press 5` (through the daemon) runs the whole multi, page step included.
- [ ] Unplug during a delay: the run finishes (log), no panic; replug shows the page it moved to.
- [ ] Edit the config mid-delay: the old steps finish.
- [ ] Rename the page a multi step opens: the step follows the new name.
- [ ] A config with no multi loads and saves byte-identical.

Task 5 and 6 (GUI):
- [ ] It compiles: check first `app.rs` `action_at`/`set_action_at` (the recursive `Step`
      arms) and `editor.rs` `multi_rows` (the popover's weak ref, `connect_activated`,
      `PreferencesGroup` added inside a group), `change_steps`, `action_editor` (the combo
      over `slot.kinds`, `gtk::INVALID_LIST_POSITION`), the `Delay` arm's `SpinRow` and
      `test_button`'s `Route::Local` closure.
- [ ] A key's kind combo has no "Delay"; a hand-edited top-level `type = "delay"` key shows
      an empty combo instead of the wrong kind.
- [ ] Build the "Meeting" key from the example; the saved `config.toml` matches the
      example's shape. Reorder and remove steps; the selected step follows.
- [ ] The step group shows under "Add step" inside the key's group. If it renders badly,
      add it to the page after the group instead (and note it here).
- [ ] Each step's editor works (app picker, file browse, Hyprland, volume, delay spin;
      the spin keeps focus while typing). Pick an app for a step, then switch key before
      the picker closes: the change lands on that step (or toasts that it was dropped if
      the step went).
- [ ] The key's ▶ flashes the key and runs the steps; a second ▶ during a delay is
      ignored. A step's ▶ runs only that step. With the service stopped, the key's ▶ runs
      a plain multi here and toasts the first failure as "Step 2: …"; a multi with a page
      step toasts that it needs the service.

Assumptions and changes from the plan:
- Unconfirmed Elgato behaviour, taken as the plan's defaults: a second press while a multi
  runs is ignored; a failing step doesn't stop the rest; a multi may sit on a dial turn
  (once per turn event) and press.
- Task 1 put stubs in `action::run` and `action_rows` (replaced in tasks 2, 5 and 6), and
  task 3 committed `mod sequence` with a temporary `#[allow(dead_code)]` (task 4 removes
  it), to keep each commit clippy-clean.
- Offline `press`/`dial` (`main.rs` `run_multi_here`): when some steps failed it still
  waits 300 ms before exiting 1, so the steps that did launch get to start.
- `set_action_at` on a step whose parent's page has gone toasts "That key's page was
  deleted, so the change was dropped" (the `edit_ref` wording); a step that's gone, or a
  parent that's no longer a multi, toasts "That step was removed, so the change was
  dropped".
- A step's ▶ on a Delay is insensitive with "Nothing to try: a delay only waits" (the
  plan's "Delay only works inside a multi action" is kept for a top-level delay).
- Structural edits leave another multi's selection alone (a dial's other slot), except
  Add, which selects the new step.
- `multi_rows` adds a hint row under "Add step" (steps don't wait for each other) once a
  multi has a delay or more than one step.
- The commented example in `examples/config.toml` uses index 5, as the plan does, but key
  5 is taken there; its comment says to pick a free index. The site (`site/index.html`)
  wasn't touched: it doesn't list the newer action types either.

## `feat/folders` (pr13-folders.md)

Review 3 bugs brought in here (BUGS.md): 79, 98.

Commits: `9978f6a` (task 1), `a09db77` (task 2), `b05fb0e` (task 3, GUI and docs, GUI not
compiled).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 221 (+1: `config::tests::page_path_follows_folder_openers`),
      omadeckd 88 (+3, -1: `nav::tests::nearest_skips_nested_folders` replaces
      `parent_is_where_back_would_go`, plus `nav::tests::home_clears_a_nested_trail`,
      `deck::tests::back_hold_deadline`, `daemon::tests::only_a_back_press_waits_for_release`),
      gui 7 (unchanged).
- [ ] The three socket tests still fail here only on bind (`Operation not permitted`); no
      new socket test.
- [ ] `git diff --stat feat/multi-actions -- Cargo.lock` is empty (no new crates).

Task 1 and 2 (daemon), on the deck:
- [ ] Tap Back in a folder: goes up one level on release.
- [ ] Hold Back ~0.5 s two folders deep: the deck goes Home while the key is still held
      (log: `key <i>: held Back, going Home`); releasing does nothing more.
- [ ] A Back key on a normal page (not a folder) works the same.
- [ ] Hold a toggle key (e.g. the mic mute): its face flips to the new state and stays
      drawn pressed until you let go (BUGS.md 65).
- [ ] Two folders deep (Music → Media → Spotify), flick left: the deck shows the page
      after Music, not the page after Home.
- [ ] `omadeckd press 0` (PR 11) on a Back key goes back at once.
- [ ] Unplug while holding Back, replug: no stray Home or Back.

Task 3 (GUI):
- [ ] It compiles: check first `app.rs` `rebuild_grid` (the double-click `GestureClick`
      builder with `.button(gdk::BUTTON_PRIMARY)` and `.propagation_phase(..)`, the typed
      `connect_pressed` closure, `g.set_state(gtk::EventSequenceState::Claimed)`) and
      `key_target` (`?` on `current_page()`, the guard borrowing `config`).
- [ ] Double-click a folder key in the configurator: it opens, and the deck follows.
      Double-click its Back key: back to the page that opens it. A single click still
      selects; dragging keys still works; after the double-click no key is selected on
      the new page.
- [ ] The page switcher shows `Music › Media › Spotify` for a nested folder; ticking or
      unticking the folder button updates the label at once. Check the dropdown doesn't
      get uncomfortably wide with long paths.

Assumptions and changes from the plan:
- Unconfirmed Elgato behaviour, taken as the plan's defaults: Back acts on release; the
  hold is 500 ms; a hold works on any key whose press would run Back, not only top-left.
- A Back key on Home, held, does nothing (`Nav::home` is false there); tapped, it does
  nothing as before. Pressing a second Back key while one is held replaces the hold, so
  the first key's release only repaints it.
- GUI: no `open_from_key` method. The double-click handler calls `key_target(i)` (where
  the key goes, `None` for any other key) and only on a target claims the second press,
  so its release doesn't select the key under the pointer on the new page, then calls
  `show_page(target, true)`. Double-clicking an ordinary key is left alone. Back on Home
  goes nowhere.
- The folder button's toggle handler now also calls `sync_page_list`, so the path label
  follows. Labels otherwise refresh on a page switch, add, rename, delete or reload, not
  when a key's Open-page target is edited (stale until the next switch).
- The longer hint under the deck is one string over 120 columns (rustfmt leaves it).

## `feat/media-dial` (pr14-media-dial.md)

Review 3 bugs brought in here (BUGS.md): 73, 75, 76, 82, 83, 84, 85, 89, 92 (README), 94, 95, 103.

Commits: `25ef012` (task 1), `b60e523` (2), `839d72d` (3), `dc80b7f` (4), `57c828e` (5),
`75694de` (6), `072d31c` (7, GUI and docs, GUI not compiled). Tasks 1 and 3 also touch the
GUI (a stub `Media` arm, the "Follows the player" label).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 257 (+36: 5 + 1 in `config::`, 17 in `player::`, 5 in `probe::`,
      7 `render::tests::media_*`, `ipc::tests::dial_status_media_on_the_wire`), omadeckd 95
      (+7: `watch::tests::{with_art_keeps_decoded_image_for_same_path,
      record_ignores_a_playing_tick, media_cmd_runs_then_refreshes}`,
      `daemon::tests::{media_tick_waits_for_the_next_second, guess_track_seeks_and_flips,
      settle_expect_accepts_a_playing_tick, config_probes_keep_media_dials}`), gui 7
      (unchanged). `CI=1 cargo test -p omadeck-core render` passes; no golden changed.
- [ ] The three socket tests still fail here only on bind (`Operation not permitted`); no
      new socket test.
- [ ] `git diff --stat feat/folders -- Cargo.lock` is empty (no new crates).

Capture first (pr14-media-dial.md, "Capture on the machine first"; nothing was captured):
- [ ] Captures 1–3: `playerctl` installed, `omarchy-shell` works from the service
      (`OMARCHY_PATH` in its environment), and the media command names are `playPause`,
      `next`, `previous`. If not, fix `player::media_argv` and its test.
- [ ] Captures 4–5: save `playerctl-two.txt` and `playerctl-stream.txt` under
      `crates/omadeck-core/tests/fixtures/` and switch `player::tests::parse_playerctl_two_players`
      and `parse_playerctl_stream_without_length` from their synthetic lines to the files
      (`include_str!`). Check positions are microseconds and Spotify's art is `https://`.
- [ ] Capture 6–7: no players exits non-zero (read as "Nothing playing"); `playerctld`
      shows up as a player name and is skipped.
- [ ] Capture 8: set `probe::MEDIA_REFRESH_DELAY` (600 ms now).
- [ ] Capture 10: the six glyphs (`f040a`, `f03e4`, `f04ad`, `f04ae`, `f04db`, `f075a`)
      draw in the configured font.

Tasks 5 and 6 (daemon), by hand:
- [ ] `RUST_LOG=debug omadeckd` with a media dial: one `playerctl` per second (`pgrep -c
      playerctl` stays at 0–1), and no new value logged while a track just plays.

Hardware (Stream Deck +, 0fd9:0084, Omarchy 4.0.4, Spotify and a browser playing):
- [ ] Press = play/pause, and Omarchy's media pop-up shows, the same as the keyboard key.
- [ ] Turn = ±5 s per detent. A fast spin is one `playerctl position N+` (daemon log). No
      `playerctl` processes pile up.
- [ ] Seeking past the end goes to the next track (Capture 9), and the panel follows.
- [ ] The bar moves once a second while playing, stops when paused, jumps at once after a
      seek from the deck, and within ~1 s after a seek in the player's own UI.
- [ ] A track change updates title, artist and art within ~1 s.
- [ ] Art: the browser's `file://` art shows; Spotify's `https://` shows the glyph.
- [ ] Two players (Spotify paused, YouTube playing): the panel shows YouTube, and a seek
      goes to YouTube.
- [ ] `player = "spotify"` pins the dial to Spotify while YouTube plays.
- [ ] The play/pause key's face follows the keyboard's play/pause within ~1 s, and doesn't
      flicker after a press on the deck.
- [ ] Holding the media dial down turns only its title to the accent colour.
- [ ] Without `playerctl` (rename it off `PATH`): keys and press still work through
      `omarchy-shell`, the panel says "Needs playerctl", and a turn shows the "install
      playerctl" toast.
- [ ] Idle CPU with a media dial: paused, and playing (one repaint a second), close to
      before (`top -p $(pgrep omadeckd)`).
- [ ] The example's Media page: key 1 plays/pauses and its face follows the player; key 2
      skips to the next track.

Task 7 (GUI):
- [ ] It compiles: check first `editor.rs` `media_rows` (the `position_of` closure used
      before and inside the combo handler, the `SpinRow` resetting itself to the default
      on 0, `EntryRow::set_text`), the `media` action in `dial_presets_button`, and
      `app.rs` `handle`'s `Event::DialStatus { index, mut status }` arm (let chain, the
      `panel_status` borrow ending before `load_icon`), `refresh_strip` and `tick_media`.
- [ ] Presets › Media dial fills the dial; the preview shows dashes with the daemon
      stopped, and the playing track (moving once a second) with it running.
- [ ] A key's Presets › Play/Pause makes a toggle whose *State from* reads "Follows the
      player"; the Command combo switches between Play/Pause, Next, Previous, Stop and Seek
      (Seek shows the Seconds spin; typing 0 snaps back to 5, or −5 on a left turn); the
      Player entry saves blank as no `player`.
- [ ] *Try it now* on a media action plays/pauses (through the daemon, or here without it).

Assumptions and changes from the plan:
- Nothing from "Capture on the machine first" was run. Defaults taken: Omarchy 4's
  commands are `omarchy-shell media playPause|next|previous`; `MEDIA_REFRESH_DELAY` is
  600 ms; no players is a non-zero exit. The two parser tests use synthetic lines in
  playerctl's format, not fixture files (see Capture 4–5 above).
- Omarchy 4 is still detected by `omarchy-audio-output-volume` (`audio::backend()`); a
  machine with that but no working `omarchy-shell` (no `OMARCHY_PATH` in the service)
  would send play/pause nowhere. Capture 2 decides whether to check for it.
- The player name is trimmed everywhere it becomes a probe or a target (`state_probe`,
  `dispatch`, `run_media`), so a key and a dial with `" spotify "` share one probe.
- `action::run_multi_blocking` runs a media step inline and waits for it (as a volume
  step does) instead of on the `media` thread, so a multi's later steps see it done.
- `config::tests::default_for_follows_direction` now skips kind 14: a media right turn
  seeks +5 s, unlike `empty_of_kind(14)`.
- The Media dial preset is an item in PR 10's dial Presets menu (not a header button), and
  also clears `live`, as the app presets do. The Play/Pause preset uses `set_glyph`
  (clears `icon`), as the other presets do.
- The Player entry is an `adw::EntryRow` titled "Player (e.g. spotify; empty = the one
  playing)": an entry row has no subtitle or placeholder.
- The example's Media page now uses the media action (key 1 play/pause with an on face,
  key 2 next) instead of `playerctl play-pause` as an app; the commented media dial and
  an app-volume-turn variant raise `example_dial_parses` to eight blocks.
- `finish_job` gets an explicit `Some(Probe::Player(_)) => None` arm next to the existing
  catch-all.

## `feat/app-pages` (pr15-app-pages.md)

Review 3 bugs brought in here (BUGS.md): 79 (new trigger), 80, 99.

Commits: `0460fbe` (task 1), `54ea4fb` (task 2), `88f5a7e` (task 3, GUI and docs, GUI not
compiled; also adds `follow::desktop_class` to core).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 262 (+5: `follow::tests::{class_matches_globs,
      parse_clients_dedups_and_sorts, desktop_class_prefers_startup_wm_class}`,
      `config::tests::{follow_apps_omitted_when_empty,
      follow_app_page_skips_folders_and_takes_first}`), omadeckd 101 (+6, all in
      `follow::tests`), gui 7 (unchanged).
- [ ] The three socket tests still fail here only on bind (`Operation not permitted`); no
      new socket test.
- [ ] `git diff --stat feat/media-dial -- Cargo.lock` is empty (no new crates).

Capture first (pr15-app-pages.md, "Capture on the machine first"; nothing was captured):
- [ ] The configurator's class is `dev.omadeck.Omadeck` (`hyprctl activewindow | grep
      class` with it focused). If not, change `follow::CONFIGURATOR_CLASS`.
- [ ] Save `hyprctl -j clients` (Spotify, an Omarchy web app, a TUI app open) as
      `crates/omadeck-core/tests/fixtures/hyprctl-clients.json` and add a `parse_clients`
      test on it; the current test uses a synthetic list.
- [ ] Whether Walker (or another layer-shell launcher) sends `activewindow` events
      (socket2, open and close Walker). If it sends an empty one, opening Walker over a
      linked app for longer than 200 ms puts the deck back.

Task 2 (daemon), on the deck:
- [ ] Link Music to Spotify. Focusing Spotify switches the deck (log: `page Music (follows
      spotify)`); focusing the browser returns to the page it was on.
- [ ] Alt-tab quickly through several windows: the deck changes at most once.
- [ ] Flick to another page while Spotify is focused, then leave Spotify: the deck stays.
      Focus Spotify again: it switches again.
- [ ] Inside a folder on Work, focus Spotify, then leave: back in the folder, and its Back
      key goes to Work.
- [ ] Open a folder from Music while Spotify is focused, then leave: back to before.
- [ ] Spotify's popups and menus don't bounce the deck.
- [ ] Closing the last window on a workspace returns the deck.
- [ ] Unplug, focus Spotify, plug in: the deck comes up on Music.
- [ ] A config without `apps` saves byte-identical.

Task 3 (GUI):
- [ ] It compiles: check first `pages.rs` (`AppRows::fill` and its row buttons calling
      back into it, `show`'s `ButtonRow::connect_activated` handlers, the
      `gio::spawn_blocking` around `proc::output_timeout`, `PreferencesDialog::add_toast`,
      `pick_window`'s `AlertDialog::set_extra_child`, `desktop_entry` with
      `glib::user_data_dir`/`system_data_dirs`), and in `app.rs` `edit_page_apps` (the
      `borrow_mut` in the `match` scrutinee) and the `follow_apps` handler's let chain.
- [ ] The cog button shows for a page, not Home. On a folder the group is greyed out and
      says folders can't follow apps.
- [ ] Add app… with Spotify adds `spotify` (or its `StartupWMClass`); a GTK app adds its
      app id. Adding the same class twice (any case) does nothing; the trash button
      removes it. The switcher reads `Music · follows apps`.
- [ ] An Omarchy web app added with **Add open window…** matches. With Hyprland not
      running the dialog toasts "No open windows found".
- [ ] Typing a class in the Window class row and pressing apply adds it and clears the row.
- [ ] Focusing the configurator never switches the deck; its page switcher follows the
      deck's automatic switches.
- [ ] Add a link in the GUI while the app is focused: the deck switches on reload.

Assumptions and changes from the plan:
- Nothing from "Capture on the machine first" was run; the plan's defaults are taken.
- The daemon reads the class from `rt.window` when focus settles, instead of carrying it
  in `focus_pending` (now `Option<Instant>`), so `Runtime.window` is used and its
  `#[allow(dead_code)]` is gone. Every focus event, the configurator's too, restarts the
  200 ms; if focus settles on the configurator nothing is decided. (The plan kept the
  last non-configurator class pending, so an alt-tab ending on the configurator still
  switched to the app before it.)
- gio 0.22 has no `DesktopAppInfo` (it moved to the `gio-unix` crate), so **Add app…**
  reads the desktop file itself: `pages::desktop_entry` looks for
  `applications/<id>` in the user's then the system's data dirs, and core's
  `follow::desktop_class(id, entry)` takes `StartupWMClass` from `[Desktop Entry]`, else
  the id without `.desktop`. Desktop ids that stand for subdirectories (`kde-foo.desktop`)
  aren't found and fall back to the id.
- The dialog has two groups: **Follow apps** (one row per class, or "No apps yet") and an
  untitled one with Add app…, Add open window… and the Window class row; the
  `hyprctl activewindow` hint is that group's description (an entry row has no
  subtitle). The open-window list leaves out the configurator, and its toast goes on the
  dialog, not the main window.
- `edit_page_apps` ends its `borrow_mut` inside a `match` rather than the plan's
  `let … else` with `drop`.
- Duplicates are ignored case-insensitively, since matching is.
- The switcher's "· follows apps" shows only on non-folder pages (a folder's `apps` are
  ignored).

## `feat/strip-long-press` (pr16-strip-long-press.md)

Review 3 bugs brought in here (BUGS.md): 78 (`Msg`), 101.

Commits: `942a058` (task 1, core and daemon), `13597e8` (task 2, GUI and docs, GUI not
compiled).

Build:
- [ ] `cargo clippy --all-targets -- -D warnings` clean, the GUI included (core and
      daemon were clean on the Mac; the GUI was never compiled).
- [ ] `cargo test`: core 265 (+3: `config::tests::{long_press_falls_back_to_press_and_is_omitted,
      long_press_counts_for_the_panel_level, rename_page_follows_long_press}`; also
      `walkers_visit_every_slot_in_order` and `example_dial_parses` now check the long
      press), omadeckd 101 (unchanged: `source_names_its_key_or_dial` gains a line), gui 7
      (unchanged).
- [ ] The three socket tests still fail here only on bind; no new socket test.
- [ ] `git diff --stat feat/app-pages -- Cargo.lock` is empty (no new crates).

Capture first (pr16-strip-long-press.md, "Capture on the machine first"; nothing was
captured):
- [ ] `systemctl --user stop omadeckd`, `cargo run -p omadeckd 2>&1 | grep -E "strip
      (tap|long press|swipe)"`, then tap a panel, hold one about a second, and hold one
      while drifting sideways. A hold must log **only** `strip long press at`; if a
      `strip tap at` comes first, a hold runs `press` and then `long_press`: stop and
      rethink. Note how long the hold takes and whether a drifting hold is a long press
      or a swipe.

Task 1 (core and daemon), on the deck:
- [ ] Hold each of the four panels: that dial's `long_press` runs, once, and the panel
      flashes (log: `strip long press on dial N`). A short tap still runs `press`.
- [ ] A dial without `long_press`: holding its panel runs `press`, once.
- [ ] A hold that drifts sideways doesn't change page.
- [ ] A long press that fails (e.g. a missing script) shows the configurator's toast.
- [ ] A long press that opens a page: renaming that page in the configurator keeps it
      working; deleting the page clears it.
- [ ] A dial whose only volume action is a long-press mute shows the volume on its panel.

Task 2 (GUI):
- [ ] It compiles: check first `editor.rs` (`ActionSlot::for_dial`/`dial_long_press`,
      `long_press_group`) and `app.rs` (`action_at`/`set_action_at`'s `DialLongPress`
      arms, `dial_slot`, `refresh_dial`'s guarded match arm).
- [ ] The "Long press on the strip" group's ▶ runs the long press action only: the panel
      doesn't flash.
- [ ] The group is missing on a deck without a strip.
- [ ] A kind picked in the group starts from the press defaults (Volume mutes).
- [ ] The dial button's tooltip reads `… — press: … · hold: …` once a long press is set.
- [ ] A config without `long_press` saves byte-identical from the GUI.

Assumptions and changes from the plan:
- Nothing from "Capture on the machine first" was run; the plan's assumptions are taken:
  the firmware sends a hold as one `TouchScreenLongPress` with no tap before it, and its
  threshold is whatever it is (no timer of ours).
- The `strip tap on dial N` / `strip long press on dial N` log line is in `strip_touch`
  (as the old arm had it) rather than in `tap_dial`, so ctl's `dial N press` doesn't log
  it; `tap_dial` keeps PR 11's `unwrap_or_default` + `is_none` shape.
- `walkers_visit_every_slot_in_order` had to change: a dial's walk now has four slots.
- An app pick in a long press's App volume doesn't name an unlabelled dial after the app
  (the turn and press slots do): that goes through `dial_slot()`, which is `None` for a
  long press, as the plan says.
- Docs: besides the plan's README line, the README's dial config section gets a short
  `[dial.long_press]` example, and the example's brightness dial (index 3) gets
  `[dial.long_press]` set = 100, uncommented by `example_dial_parses` (still eight
  blocks).
