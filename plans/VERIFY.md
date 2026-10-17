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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine; gtk4 4.22.4 / libadwaita 1.9.3 present, so the GUI linked.)
- [x] `cargo test`: core 141, omadeckd 43 (the hub socket tests too), gui 3. (2026-10-17, machine; counts exact.)
- [x] `cargo clippy --version` matches CI (0.1.95 or newer), and the CI run is `success`. (2026-10-17; run 39060927550 success. The first run 39060500703 was red on `run_tracked_streams_lines` — BUGS.md 105, fixed and the chain rebased.)

Task 4 (brightness):
- [ ] Turn the brightness dial to 20, relabel a key in the GUI, then
      `systemctl --user restart omadeckd`. The deck comes back at 20, and `state.toml`
      has `brightness = 20` and `config_brightness = <the config's value>`.
- [x] Stop the daemon, edit `brightness = 35` in config.toml, start it: 35. (2026-10-17, machine, dev build. A config edit after the last save wins at start: with `config_brightness = 40` recorded, editing the config to 45 and starting came up at 45. Note: a *legacy* state file written before `config_brightness` existed still lets its saved brightness win — that is the documented rule, so the very first start after upgrading keeps the old value until the new daemon records `config_brightness` once.)
- [x] With the daemon running, edit `brightness = 45`: the deck dims within a second. (2026-10-17, machine, dev build; edited to 60, `brightness 60%` logged <1 s later and saved to state.toml.)

Task 5 (watcher):
- [x] Spin the volume dial hard both ways, then stop. The bar ends on the real level
      (`wpctl get-volume @DEFAULT_AUDIO_SINK@`) and is still there 2 s later. (2026-10-17, machine; ended at 55%, stable at 2 s, GUI agreed.)
- [x] Unplug the deck mid-spin and plug it back: the bar shows the real level. (2026-10-17, machine; came back at the real 20%, no guess-from-0 flash.)
- [x] Mute the mic with its key, then unplug and replug: the GUI still shows the key on. (2026-10-17, machine; the Mic toggle key 7, made with the Mic-mute preset, stayed on after replug on deck and GUI.)
- [x] A `slow = true` toggle with `state = "false"` and an action that does nothing.
      Press it: busy, then off again after about 15 s. (2026-10-17, machine; key 5, Busy tab + progress bar, settled to off.)
- [x] Deck unplugged, GUI open: `RUST_LOG=debug omadeckd` shows no `Disconnected` every
      2 s, and the GUI doesn't flicker. (2026-10-17, machine; exactly one `lost Stream Deck +` event over ~10 s out.)

Task 6 (Hyprland worker):
- [x] A dial on `workspace e+1`, spun hard: workspaces step in order, with no stall. (2026-10-17, machine; dial 1, ordered `workspace e+1` in the log. Note: this Hyprland translates to Lua dispatchers, `hl.dsp.focus({ workspace = "e+1" })`.)
- [x] A key with `dispatch = "nosuchdispatcher"`: the GUI toasts "Key N: hyprctl: …"
      within a moment, and the deck stays responsive. (2026-10-17, machine; key 5, toast shown, deck responsive.)

Task 7 (toggle editor, GUI) — **skipped at the user's request (2026-10-17)**; the GUI was
linked and runs (Tasks 5–6 exercised it), but these editor click-throughs were not done:
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

Task 8 (GUI) — **skipped at the user's request (2026-10-17)**:
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
- [x] Uncomment the Backup block in a scratch copy of `examples/config.toml`; `cargo run
      -p omadeckd -- snapshot /tmp/s.png` loads it. (2026-10-17, machine; dev build, `snapshot /tmp/s.png` wrote the PNG with the block uncommented — `busy_while_running`, `[key.on] slow/state` and `[key.busy] bar` all parse.)

Not done on the Mac, deliberately:
- `dial_slot` has no caller until PR 10, so it carries `#[allow(dead_code)]`. PR 10
  removes the allow.

## `feat/live-keys` (pr9-live-keys.md)

Review 3 bugs brought in here (BUGS.md): 78 (clippy), 81, 87, 97.

Commits: `837c11e` (task A), `cdbaba1` (B), `056fd8a` (C), `faa2562` (D), `635a384` (E),
`91167cd` (F), `969ab3a` (G, GUI, not compiled), `ab0df79` (H).

Build:
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine; clippy 0.1.99. Needed BUGS.md 78's fix on this branch: cherry-picked `9df8df3` as `9abbf73`, chain rebased.)
- [x] `cargo test`: core 179, omadeckd 60 (the hub socket tests too), gui 3. (2026-10-17, machine; counts exact.)
- [x] `git diff --stat fix/review-2 -- Cargo.lock` shows only lines added under `jiff`
      (`windows-link`, already locked) and `omadeck-core` (`jiff`), and the
      `omadeckd` line for `serde_json`: no new `[[package]]`. (2026-10-17, machine; exactly those 3 lines.)

Captures (the plan's "Capture on the machine first"; run on the Omarchy machine, Hyprland 0.56.2):
- [x] 1. Record the socket2 transcript, add it as `crates/omadeckd/tests/fixtures/socket2.txt`
      and write the test. (2026-10-17. 95 lines captured while switching workspaces 1–3,
      focusing the terminal/browser, changing tabs, and empty workspaces. The test
      `committed_transcript_gives_expected_events` walks all lines through parse/handle
      against a canned query and asserts the exact 13-event sequence. Confirmed: event
      addresses have **no** `0x` prefix (the request-socket JSON does — both normalise),
      an empty `activewindowv2>>` (no comma) means no focus, and legacy + v2 lines are
      both sent. Note: the request socket rejects a trailing `\n` ("unknown request");
      `hypr.rs` writes without one, so it is unaffected. The capture-fixtures commit's
      generic `transcript_gives_events` (reads the dated `captured/` folder) is a
      separate test.)
- [x] 2. `j/activeworkspace` / `j/activewindow` JSON matches what `hypr.rs` reads. (2026-10-17; `{"id":1,"name":"1",…}` and `{"address":"0x…","class":…,"title":…,"pid":…}` — the `0x` is present in the JSON, absent in events.)
- [x] 3. `systemctl --user show-environment` has `XDG_RUNTIME_DIR` and the instance dir. (2026-10-17; `XDG_RUNTIME_DIR=/run/user/1000`, `HYPRLAND_INSTANCE_SIGNATURE` set, instance dir has `.socket.sock` + `.socket2.sock`.)
- [~] 4. Battery sysfs. (2026-10-17: **no battery** — this is a desktop, `/sys/class/power_supply` is empty. The battery key shows `–` here; `battery_in`'s None path is the expected behaviour. Not testable on this machine.)
- [x] 5. `/proc/stat` and `/proc/meminfo` samples parse. (2026-10-17; `cpu ` line has the 10 standard fields, `MemTotal`/`MemAvailable` present.)
- [x] 6. The seven glyphs (`f0150 f061a f035b f0079 f0084 f056e f018d`) render as a
      clock, chip, memory, battery, charging battery, workspace and console. (2026-10-17, user confirmed all seven in the Omarchy terminal; no constant changed.)
- [x] 7. jiff `%n` gives a newline (`clock_formats_and_ticks` passes on the Mac).

Task F (daemon, on the hardware) — keys 4/5/6 set to clock/workspace/cpu in the live config:
- [x] A `%H:%M` clock key turns over within ~0.15 s of the minute (compare with
      Waybar); a `%H:%M:%S` key ticks every second without skipping. (2026-10-17, machine; key 4 rolled over promptly at the boundary. The `%S` variant not tried.)
- [ ] After suspend and resume the clock is right within 5 s. *(not done — needs a suspend)*
- [ ] `timedatectl set-timezone …` moves the clock (how fast jiff notices: unconfirmed). *(not done)*
- [x] CPU follows btop roughly; memory and battery match `free` and Waybar; plugging
      the charger shows the charging glyph within 5 s. (2026-10-17, machine; key 6's % was plausible against the bar. Memory key not set; no battery on this machine.)
- [x] Workspace key follows `SUPER+1..5`, a click on another monitor's window, and a
      rename; `–` when the daemon runs outside Hyprland (TTY). (2026-10-17, machine; key 5 followed `SUPER+1/2`. No second monitor to click; rename and the TTY case not tried — and note `renameworkspace` has no Lua dispatcher on Hyprland 0.56.2, so a rename may not be reachable here at all.)
- [ ] Restarting Hyprland reconnects the listener with the new signature.
- [ ] A command key with `sleep 30` doesn't delay the clock; its run is killed at 5 s
      (default interval).
- [ ] Spinning a volume dial hard while a seconds clock runs: the clock may stall for
      a moment but catches up.
- [ ] Pressing a live key runs its action; the pressed ring shows over the value. A
      live key with `busy_while_running` shows the busy face while its command runs,
      then the value again.
- [ ] A live command key (e.g. `playerctl status`) updates right after a press.
- [ ] Battery and clock dials draw `$B1` and `$A1` correctly on the strip. *(no live dial set)*
- [x] No visible CPU cost from the daemon with a seconds clock (`top`). (2026-10-17, machine; 0.4% with a `%H:%M` clock, a workspace key and a CPU key live. A seconds clock would poll more.)

Task G (GUI):
- [x] Pick each source on a key and on a dial: the preview shows `–` with the daemon
      stopped, and the live value with it running, within a second. (2026-10-17, machine; the configurator's key 4/5/6 previews matched the deck's clock/workspace/CPU values.)
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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine; GUI linked for the first time on this branch.)
- [x] `cargo test`: core 206, omadeckd 68 (the hub socket tests too), gui 3. (2026-10-17, machine; core 206 and gui 3 exact; omadeckd 69 = 68 + the live-keys `committed_transcript_gives_expected_events` test inherited from the branch below.)
- [x] `git diff --stat feat/live-keys -- Cargo.lock` is empty (no new crates). (2026-10-17, machine; empty.)

Captures (run on the Omarchy machine; pactl 17.0, libpipewire 1.6.8, Hyprland 0.56.2):
- [x] 1. Save `pactl -f json list sink-inputs` per case. (2026-10-17. Saved `crates/omadeck-core/tests/fixtures/sink-inputs-brave.json` — one Brave stream. Confirmed the real shape: `object.id` and `application.process.id` live **under `properties`** (not top level), `value_percent` is `"NN%"`, `mute`/`corked` booleans. The code reads `props.get("object.id")` — correct. The `pactl_fixtures_parse` test is still not written; the hand-written `pactl_streams_parse` matches the captured shape.)
- [x] 2. `object.id` is the node id `wpctl` takes. (2026-10-17; `wpctl get-volume 123` returned the Brave stream's 100%.)
- [x] 3. `wpctl set-volume -l 1.0 <id> 5%+` works on a stream node. (2026-10-17; `5%-`→0.95, `5%+`→1.00. Note: the code emits the step number-first (`5%-`), which is the form wpctl accepts — a leading `-` like `-5%+` is rejected as an unknown option.)
- [x] 4. Save `hyprctl -j activewindow` (window + empty workspace). (2026-10-17. Window case: `{address:"0x…", class, title, pid}` — the `0x` present as the code expects. Empty-workspace case: focused an unused workspace via the dispatch socket and captured **exactly `{}`** — saved as `crates/omadeck-core/tests/fixtures/activewindow-empty.json`; the code's `parse_window_json("{}") → None` path is what handles it. Chromium's/`mpv`-in-terminal pid-descent not checked.)
- [~] 5. Icons. (2026-10-17: Chromium and mpv found under `/usr/share/icons/hicolor/…/apps/`. Spotify and Firefox are **not installed** here — Brave is, and `brave-browser` has no hicolor icon, so it falls back to `APP_GLYPH`. Partial.)
- [x] 6. Versions. (2026-10-17: pactl 17.0, libpipewire 1.6.8, Hyprland 0.56.2. `omarchy --version` isn't a command.)
- [ ] 7. `f08c6` renders as an application window in the Omarchy terminal (`APP_GLYPH`).

Hardware (tasks F, G) — Spotify/Firefox aren't installed here; **Brave** (playing
YouTube) stood in, and dial 1 was set to the **focused** app:
- [x] Spotify dial: turn ±5% matches Omarchy's audio panel slider; a fast spin sends
      one change, never above 100%. (2026-10-17, machine, with Brave as the chosen app: the picker listed Brave (playing, with icon) plus "Installed apps…"; turning moved only Brave's stream.)
- [x] Press mutes Spotify only (the system volume stays); the panel shows `Muted`
      within a moment. (2026-10-17, machine; focused-press ran `wpctl set-mute 123 1/0` on the Brave stream only.)
- [ ] Spotify not running: dimmed, Spotify's icon, `–`. Start it and play: lights up
      within ~1 s. *(not done)*
- [x] Spotify paused (corked): still shows its level, turning still works. (2026-10-17, machine, Brave paused.)
- [x] Firefox with two playing tabs: both change, the panel shows the louder. Chromium
      the same (audio from a child process). (2026-10-17, machine; two Brave/YouTube tabs playing — both moved, panel showed the louder.)
- [x] Focused dial: focus Firefox → Firefox. Focus a silent terminal → still Firefox
      (sticky). Focus a terminal playing `mpv` → mpv. (2026-10-17, machine, with Brave. The focused dial turned **only** Brave's stream node (123) — `wpctl set-volume -l 1.0 123 5%+` in the log — and a press ran `wpctl set-mute 123 1` then `123 0`, i.e. it muted/unmuted **Brave only**, system volume untouched. The silent-terminal-stickiness and `mpv`-in-terminal cases not tried.)
- [ ] Focused dial right after daemon start on a silent window: dimmed, the window's
      class, `–`. *(not done)*
- [x] Alt-tab quickly: the panel follows without a blink; daemon CPU ~0% at idle. (2026-10-17, machine; Brave ↔ terminal. The "switch to mpv" half is **not reproducible here** — see the note below the section.)
- [ ] A Flatpak app (if any) matches by class. *(no flatpak on this machine)*
- [x] Holding the dial turns only the title to the accent colour. (2026-10-17, machine.)
- [ ] A strip image that's dark under the title and light under the value: each text
      picks its own contrast colour. *(not done — needs a strip background image)*
- [x] `omadeckd press` / `dial` with no daemon on a chosen-app action changes it (the
      `app-volume` thread). (2026-10-17, machine; daemon stopped, `omadeckd dial 1 left` ran `wpctl set-volume -l 1.0 118 5%-` — 0.65 → 0.60.)

**Why the focused dial never switched to `mpv` (2026-10-17):** two environmental
factors, not a code bug. (1) mpv's sink input publishes **no `application.process.id`**
(only `application.name`/`application.id` = "mpv"), so there is no pid to match a
window against. (2) The terminal runs under **tmux**: the shell's parent is the tmux
*server* (child of systemd), not the kitty window, so even a published mpv pid would
not descend from the window pid. The plan's "mpv-in-a-terminal pid descends from the
window pid" assumption does not hold on this machine; the dial correctly kept the last
matchable app (Brave).

Task H (GUI):
- [x] It compiles: check first `apps.rs` `pick_audio` and `audio_key`, `editor.rs`
      `dial_presets_button` and the `Action::AppVolume` arm of `action_rows`. (2026-10-17, machine; linked and run.)
- [x] Presets menu: all four presets fill all three actions; App volume dial… opens the
      picker first and changes nothing if it's closed. (2026-10-17, machine; all four present, closing the picker changed nothing.)
- [x] The picker lists what's playing (deduplicated, with icons); Installed apps… works. (2026-10-17, machine; Brave listed while playing, with icon.)
- [ ] Choose… on a dial with no label sets the label; a key's label is left alone. *(not done)*
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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine.)
- [x] `cargo test`: core 211, omadeckd 75, gui 4. (2026-10-17, machine; core 211 and gui 4 exact; omadeckd 76 = 75 + the live-keys `committed_transcript_gives_expected_events` test inherited from below. **The socket tests that the Mac sandbox couldn't bind all pass here** — `new_client_gets_the_replay`, `a_client_that_never_reads_does_not_block_publish`, `command_reply_over_socket`.)
- [x] `git diff --stat feat/app-volume -- Cargo.lock` is empty (no new crates). (2026-10-17, machine; empty.)

Captures:
- [x] `ls ~/.config/hypr/`: **this Hyprland uses a Lua config** (`bindings.lua`, `hyprland.lua`, …) — there is no `bindings.conf`. The README's binds block assumes `bindings.conf`/`bindd`; on this machine that syntax doesn't apply. (2026-10-17.)
- [x] `ls -l $XDG_RUNTIME_DIR/omadeck.sock`: **`srw-------` (0600)** on the feat/ctl build. (2026-10-17. Note: an earlier 0755 reading was a **stale feat/app-volume binary** in `target/debug` — I'd run clippy/test but not `cargo build`; after a real build the socket is 0600 and the `command_reply_over_socket` mode assert passes. Not a bug.)

Task 5 (CLI), by hand (all 2026-10-17, machine, dev daemon on feat/ctl):
- [x] `omadeckd state` with the service stopped prints `omadeckd is not running
      (systemctl --user start omadeckd)` and exits 1. (Exact string, exit 1. With the daemon up it returns the deck/page/brightness JSON, exit 0.)

Hardware (Stream Deck +, 0fd9:0084, Omarchy) — 2026-10-17, machine, dev daemon:
- [x] `omadeckd page next` / `prev` / `home` / `page Media` switch pages; `page Nope`
      prints an error and exits 1. (`page next`/`home` → `page Home`, exit 0; `page Nope` →
      `there's no page named “Nope”`, exit 1. The configurator-follows half and a real
      `page Media` need a second page — see the config note below.)
- [x] `omadeckd brightness 20`, `+10`, `-10`: the value survives a restart. (20 → 30 → 25, and
      **33 survived a daemon restart**. The deck-dims-live / dial-bar / slider-follow halves
      are visual — user to confirm.)
- [ ] With the deck unplugged, `omadeckd brightness 30`, then plug in: it comes up at 30. *(user)*
- [ ] A held `binded` brightness key: smooth, no backlog after release. *(user — no binded key in the config; set one in the GUI first)*
- [x] `omadeckd press 3` (daemon up): the action runs. (Ran key 3's Element launch, exit 0. The flash + configurator-stays-on-key-0 halves are visual — user. **A page key pressed this way switches page** is blocked: the config's key 0 points at page `Terminal 3`, which isn't defined — `press 0` → `there's no page named “Terminal 3”`. The toggle-key flip needs a toggle key; none in the config.)
- [x] `omadeckd press 3 Other` for a page the deck isn't showing. (`Other` isn't a defined page → clean error, exit 1. The "defined-but-not-showing" case needs a second page.)
- [x] `omadeckd press 3` with the service stopped: runs locally. (Exec'd the command, exit 0.)
- [x] `omadeckd dial 0 right 5` moves the volume five steps. (`dial 0 right x5: Volume +5%`, exit 0.)

**Config note (2026-10-17):** the user's `config.toml` key 0 (Terminal) has
`type = "page"`, `page = "Terminal 3"`, but no `[[page]]` named `Terminal 3` is defined —
so `omadeckd press 0` errors. The page-key-switches-page and defined-but-hidden-page checks
need a real `[[page]]` in the config (create one in the GUI).
- [x] `omadeckd dial 0 press` mutes. (Toggled the Scarlett sink's mute on and back off, exit 0. The flash/preview halves are visual — user.)
- [ ] A key, a dial turn, a dial press and a strip tap on the deck behave as before, and a
      key pressed on the deck still selects it in the configurator. *(user)*
- [x] `omadeckd reload` after a deliberate typo prints the parse error; the deck keeps the
      last good config. (Appended a bad line → `TOML parse error at line 204 … key with no value` + `; keeping the previous config`, exit 1; restored the file, daemon stayed healthy.)
- [x] `omadeckd state | jq .page` with the deck plugged. (`page: null`. The unplugged half is user.)
- [x] 50× `omadeckd state` in a tight loop: no fails, no 3 s hang. (50 calls in ~5 s, zero failures. The fast-dial-spin half is user.)
- [~] An old configurator build still shows status and key presses against the new daemon. *(skipped by the user, 2026-10-17: on this machine both binaries are installed from the same build at once, so a mixed GUI/daemon version can't occur)*

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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine.)
- [x] `cargo test`: core 220, omadeckd 85, gui 7. (2026-10-17, machine; core 220 and gui 7 exact; omadeckd 86 = 85 + the live-keys `committed_transcript_gives_expected_events` test inherited from below.)
- [x] The three socket tests pass here (they only failed on the Mac's bind). (2026-10-17, machine.)
- [x] `git diff --stat feat/ctl -- Cargo.lock` is empty (no new crates). (2026-10-17, machine; empty.)

Task 4 (daemon), by hand (2026-10-17, machine, dev daemon, README's Meeting key on index 7 + a `Meeting` page):
- [x] `omadeckd press <i>` runs every step; step 4 lands ~2 s after step 2. (Measured **2.01 s** gap step2→step4. Steps ran in order: mic mute, zoom launch, [delay], `hyprctl dispatch movetoworkspace 5`, open page Meeting.)
- [x] With the service stopped: runs here (delay included), prints `step N: …` for a failed step, exits 1; a multi with a page step exits 1 with "only work on the deck". (Page-step multi → `this multi action has page or brightness steps, which only work on the deck`, exit 1. Without the page step: ran locally incl. the 2 s delay, a failing **script** step printed `step 2: script not found: /nonexistent/zoom.sh`, exit 1, the other steps still ran. Note: an *app* step whose binary later exits non-zero is **not** reported — `launch_app` is a detached spawn by design, so only spawn/launch failures count; a script/hyprland/deck-owned step is the real failure path.)

Hardware (Stream Deck +, 0fd9:0084, Omarchy) — 2026-10-17, machine, dev daemon:
- [x] The "Meeting" example: mic mutes, the (missing) Zoom step toasts, 2 s later the window
      moves to workspace 5, the deck shows page Meeting. (User pressed the key on the deck; all steps in the log. A `Go back` key was added to the Meeting page so it's navigable.)
- [x] Pressing the key again during the 2 s delay does nothing (log: ignored). (`key 7: multi action still running, press ignored`; the first run finished.)
- [x] Other keys and dials stay responsive during a 10 s delay. (With a 10 s delay running, `omadeckd dial 0 right 2` answered in **6 ms**.)
- [x] `brightness set = 20`, `delay 1000`, `brightness set = 80`. (State read **20** mid-delay, **80** after.)
- [x] Volume `-50`, `+50` back to back on a dial press. (0.60 → 0.60.)
- [x] Two Hyprland steps back to back (`workspace 3`, `workspace 4`). (Landed on workspace 4, in order.)
- [x] A multi on a dial press runs; a failing step toasts and the next step still runs. (`dial 0: step 1: script not found: /nonexistent/boom.sh` (WARN/toast), then `dial 0 step 2: Volume +5%`. The **strip-tap** half not exercised — no strip-tap multi in the config.)
- [x] `omadeckd press` (through the daemon) runs the whole multi, page step included. (Deck ended on the Meeting page.)
- [x] Unplug during a delay: the run finishes (log), no panic; replug shows the page it moved to. (User unplugged mid-delay: `lost Stream Deck + … waiting for it to come back`, all steps logged, no panic; replug came up on the Meeting page.)
- [x] Edit the config mid-delay: the old steps finish. (Edited the label 1.5 s into a 3 s delay; `reloaded config` fired and the old `step 2: Volume +3%` still ran.)
- [x] Rename the page a multi step opens: the step follows the new name. (User, in the GUI.)
- [x] A config with no multi loads and saves byte-identical. (User deleted all of key 7's steps in the GUI; the saved file has no leftover multi fields and parses clean.)

Task 5 and 6 (GUI) — 2026-10-17, machine, GUI linked and run:
- [x] It compiles (the `Step` arms, `multi_rows`, `change_steps`, `action_editor`, the
      `Delay` `SpinRow`, `test_button`'s `Route::Local` closure). (Linked and run.)
- [x] A key's kind combo has no "Delay"; a hand-edited top-level `type = "delay"` key shows
      an empty combo. (User: all good.)
- [x] Build the "Meeting" key from the example; reorder and remove steps; the selected step
      follows. (User: all good.)
- [x] The step group shows under "Add step" inside the key's group. (User: renders fine, no re-parenting needed.)
- [x] Each step's editor works (app picker, file browse, Hyprland, volume, delay spin; the
      spin keeps focus while typing). (User: all good.)
- [x] The key's ▶ flashes and runs the steps; a second ▶ during a delay is ignored; a step's
      ▶ runs only that step. (User: all good.)

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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine.)
- [x] `cargo test`: core 221, omadeckd 88, gui 7. (2026-10-17, machine; core 221 and gui 7 exact; omadeckd 89 = 88 + the live-keys `committed_transcript_gives_expected_events` test inherited from below.)
- [x] The three socket tests pass here (Mac failed only on bind). (2026-10-17, machine.)
- [x] `git diff --stat feat/multi-actions -- Cargo.lock` is empty (no new crates). (2026-10-17, machine; empty.)

Task 1 and 2 (daemon), on the deck (2026-10-17, machine; test tree Music(folder) → Media(folder) → Spotify(leaf)):
- [x] Tap Back in a folder: goes up one level on release. (User.)
- [x] Hold Back ~0.5 s two folders deep: the deck goes Home while still held; release does nothing more. (First attempt was just under the 500 ms threshold — not a bug. A temporary debug build traced it: `back_held SET on key 0` → `deadline fired, held 500.8ms` → `held Back, going Home` → release took nothing. Debug logs removed.)
- [ ] A Back key on a normal page (not a folder) works the same. *(not done — Spotify's Back is a leaf, not a normal page; skipped with the rest at the user's request)*
- [ ] Hold a toggle key: its face flips and stays drawn pressed until release. *(skipped — no toggle key in the config; user's request)*
- [x] Two folders deep, flick: skips the nested folder pages. (Flicks cycled `Spotify → Home → Meeting → Spotify → Home` — Music and Media never appeared. **Note:** the plan's "the page after Music" assumed Music was a *normal* page; here Music is itself a folder, so the cycle anchors on the leaf/`Home` instead. The folder-skipping behaviour is correct; the "empty page" the user saw was the Meeting page, since deleted.)
- [x] `omadeckd press 0 [PAGE]` on a Back key goes back at once. (`press 0 Spotify` → `key 0: Go back`, deck went Spotify → Media. Note: `press 0` with no page targets **Home's** key 0, not the showing page — it hit the dangling `Terminal 3` page ref.)
- [ ] Unplug while holding Back, replug: no stray Home or Back. *(skipped at the user's request)*

Task 3 (GUI) — 2026-10-17, machine, GUI linked and run:
- [x] It compiles (the double-click `GestureClick`, `key_target`). (Linked and run.)
- [ ] Double-click a folder key: it opens and the deck follows; double-click its Back key:
      back to the opener; single click still selects; dragging works. *(skipped at the user's request)*
- [ ] The page switcher shows `Music › Media › Spotify` for a nested folder; the folder
      button updates the label at once. *(skipped at the user's request)*

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
- [x] `cargo clippy --all-targets -- -D warnings` clean, the GUI included. (2026-10-17, machine.)
- [x] `cargo test`: core 257, omadeckd 95, gui 7. (2026-10-17, machine; core 257 and gui 7 exact; omadeckd 96 = 95 + the live-keys `committed_transcript_gives_expected_events` test inherited from below. `CI=1 cargo test -p omadeck-core render` passes; no golden changed.)
- [x] The three socket tests pass here (Mac failed only on bind). (2026-10-17, machine.)
- [x] `git diff --stat feat/folders -- Cargo.lock` is empty (no new crates). (2026-10-17, machine; empty.)

Capture first (2026-10-17, machine):
- [x] Captures 1–3. `playerctl` was **not** installed (installed 2.4.1 during the session). `omarchy-shell` is on the service's `PATH` (`/usr/share/omarchy/bin`); `OMARCHY_PATH=/usr/share/omarchy` is set. Command names are correct: `omarchy-shell media playPause` → `ok` (`next`/`previous` → `unhandled` for a browser that doesn't implement them, but the names are right). No fix to `media_argv` needed.
- [x] Captures 4–5. Saved `crates/omadeck-core/tests/fixtures/playerctl-two.txt` (a browser player with `file://` art + a local-file player with none) and switched `parse_playerctl_two_players` to `include_str!` (commit `382dd0e`). Positions confirmed **microseconds**. `parse_playerctl_stream_without_length` stays hand-written: no real no-length stream was available (a live radio URL was blocked, and mpv estimates a pipe's length), and its empty/`0`-length edges are exactly what a hand line tests.
- [x] Capture 6–7. No players exits non-zero (`playerctl --player=doesnotexist` → exit 1); `playerctld` skip is covered by `pick_skips_playerctld`.
- [x] Capture 8. Confirmed the real cadence: a logging `playerctl` shim saw **exactly one read per second** (6 calls in 6 s, ~1 s apart), no pile-up. (A `pgrep` poll misses the ~10 ms processes — use the shim.)
- [x] Capture 10. The six glyphs draw correctly (user).

Tasks 5 and 6 (daemon), by hand (2026-10-17, machine, media dial on dial 2, Brave playing):
- [x] One `playerctl` per second, no pile-up, no new value while a track just plays. (Shim: one per second. The "no new value while playing" suppression is `record_ignores_a_playing_tick`; a temporary poller trace showed the read happening each second with the value suppressed while the position tracks the prediction.)

Hardware (Stream Deck +, 0fd9:0084, Omarchy 4.0.4) — 2026-10-17, machine, media dial on
      dial 2, a browser (Brave) playing; Spotify isn't installed so Brave + mpv stood in:
- [x] Press = play/pause, and Omarchy's media pop-up shows. (User.)
- [x] Turn = ±5 s per detent; a fast spin is one folded `playerctl position N+`. (User; the shim captured the folded command.)
- [ ] Seeking past the end goes to the next track, and the panel follows. *(not done — needs a short track near its end)*
- [x] The bar moves once a second while playing, stops when paused, jumps after a seek. (User.)
- [x] A track change updates title, artist and art within ~1 s. (User.)
- [x] Art: the browser's `file://` art shows. (User. The `https://`-shows-the-glyph half needs Spotify.)
- [x] Two players (one paused, one playing): a seek goes to the playing one. (Brave paused, mpv playing: a dial seek moved mpv and left paused Brave's position **byte-identical**.)
- [x] `player = "…"` pins the dial. (With `player = "brave"` on a turn, the shim showed `playerctl --player=brave.instance3602 position 5+` and Brave moved +5 s **while paused**, with mpv the active player. Note: the pin is per-action — each media slot uses its own `player`; a slot left blank follows the active player, which is the documented `media_player`/`media_target` behaviour.)
- [ ] The play/pause key's face follows the keyboard's play/pause within ~1 s. *(not done — no media key in the config; needs the example's Media page)*
- [x] Holding the media dial down turns only its title to the accent colour. (User.)
- [x] Without `playerctl`: press still works through `omarchy-shell`, a turn shows the "install playerctl" toast. (Verified before installing playerctl: press ran Play/Pause; turn logged `playerctl not found (install playerctl)`. The panel's "Needs playerctl" note is the same `Value::Player(Err)` path.)
- [x] Idle CPU with a media dial, paused and playing. (~0.3 s of CPU over the whole sample in both states — negligible.)
- [ ] The example's Media page: key 1 plays/pauses and its face follows; key 2 skips. *(not done — needs the example page set up)*

Task 7 (GUI) — 2026-10-17, machine, GUI linked and run:
- [x] It compiles (the `media_rows` `position_of` closure, `SpinRow` reset, `media` preset, the `Event::DialStatus` arm, `refresh_strip`, `tick_media`). (Linked and run.)
- [ ] Presets › Media dial fills the dial; the preview shows dashes with the daemon stopped,
      and the playing track with it running. *(user — not yet done)*
- [ ] A key's Presets › Play/Pause makes a toggle whose *State from* reads "Follows the
      player"; the Command combo switches between Play/Pause, Next, Previous, Stop and Seek;
      the Player entry saves blank as no `player`. *(user — not yet done)*
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

## `fix/review-3` (BUGS.md 73–104)

On top of `feat/strip-long-press` (`746e326`). `daily` is now `pr/16-strip-long-press`
(`e9ad538`): the same code, with these fixes folded into the PRs. It fixes every
review 3 bug, adds `scripts/capture-fixtures.sh` (`33a7678`) and a socket-free test
`Daemon` (`abad8d7`). The fix commits are listed in BUGS.md.

Build:
- [ ] `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
      clean. Counts here: core 273 + 10 capture tests, omadeckd 111 + 3 capture tests,
      plus the 3 hub socket tests (EPERM on the Mac). GUI: its new unit tests
      (`seek_spin_steps_over_zero`, `uwsm_lines_give_the_program_after_the_dashes`) have
      only been type-checked.
- [ ] CI run on `fix/review-3` is `success` (it couldn't be started from the Mac: `gh`
      there is logged in to the work account).

**First on the machine: the captures.**
- [ ] `scripts/capture-fixtures.sh`. It is read-only and asks before each step (start two
      players, focus Walker, hold the strip, …); skip any step. It saves into
      `crates/*/tests/fixtures/captured/<date>/`. It stops the service for the strip step and
      starts it again. Check the files for anything private (window titles, paths), commit
      them, and run `cargo test`: the capture tests then check the parsers against real
      output instead of skipping. The README in each `captured/` folder says which test
      reads which file.
- [ ] The strip capture answers PR 16's open question: `captured_strip` fails if a hold
      logs a tap before the long press.
- [ ] Not captured (each changes state), so do them by hand as the script lists at the
      end: `wpctl set-volume`/`set-mute`, `omarchy-shell media …`, how fast a player
      reports a new status, seeking past the end, `hyprctl reload`/`renameworkspace`.

Fixes to check on the deck:
- [ ] 73: play music, unplug the deck: `top` shows omadeckd idle, not at 100%.
- [ ] 74: `for i in $(seq 200); do omadeckd state >/dev/null; done`, then
      `ls /proc/$(pidof omadeckd)/fd | wc -l` is back near where it started.
- [ ] 75: a seek key on a page with no media dial seeks.
- [ ] 76: on a Media page play something in Chromium, go Home, close it, start Spotify:
      a Home Next key skips in Spotify, and Omarchy's pop-up shows.
- [ ] 94: a dial whose turns have `player = "spotify"` and whose press has none: the
      press pauses Spotify.
- [ ] 82, 95: a stalled MPRIS player doesn't make the dial or keys flicker; with
      several players, one without metadata doesn't make it show "Nothing playing".
- [ ] 79: hold Back in a folder while the configurator deletes that folder; releasing
      runs nothing, and no key stays drawn pressed.
- [ ] 80: a terminal retitling quickly (`while :; do printf '\e]2;%s\a' $RANDOM; sleep
      0.05; done`) with an app link: its page appears after 200 ms.
- [ ] 81: after a Hyprland crash (not a clean restart), workspace keys update again.
- [ ] 86: `omadeckd ctl` (or ▶) on a multi of 40 steps works.
- [ ] 96: a Flatpak app (Spotify) shows its icon on an app volume dial.
- [ ] 97: a dial clock with `%n` reads as one line.
- [ ] 93: a multi with an app volume step that finds no app reports a failed step (new
      behaviour).

Fixes to check in the configurator:
- [ ] 77, 101: a dial with a clock and a Media long press, Presets › Volume dial: the panel
      shows the volume, and the long press is gone.
- [ ] 98: New folder from a key on "Music": the switcher shows "Music › …" at once.
- [ ] 99: break config.toml with the Follow apps dialog open: adding an app shows a toast
      on the dialog, and nothing changes.
- [ ] 100: an app whose `Exec` is `uwsm app -- foo` is matched in "Playing now".
- [ ] 102: a busy-while-running key with a Busy label: switching "Two faces" off keeps it.
- [ ] 103: the media Seconds spin goes from 1 to -1 with the minus button, and back.
- [ ] 104: type into a key's spin, then change page from the deck: the other page's key
      is untouched. A key's action rows now do nothing while config.toml is broken, and
      toast if the key's page was deleted (new behaviour).

Decisions taken while fixing (check they're right):
- One rule for media targets, in `probe::media_target`'s doc: the action's player, on a
  dial falling back to the dial's; the instance a *watched* probe shows; else
  untargeted. A media step inside a multi doesn't fall back to a dial's player.
- Dial presets now clear the long press too (a preset defines the whole dial). This
  changes PR 16's "a long press is something you add".
- The Play/Pause preset keeps `needs: None` (the press works without playerctl); the
  README says its on face needs playerctl.
- A Hyprland instance that refuses connections is logged at debug level only.
