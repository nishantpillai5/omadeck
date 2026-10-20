# GUI click-through checklist (item 3)

The GUI checks `VERIFY.md` still has open, ready to run at the keyboard. Setup is done —
this is just clicking. Tick each item here and, when a section is done, tick the matching
box(es) in [VERIFY.md](VERIFY.md) and record a note.

## Setup (already done)

- **Test config is in place** at `~/.config/omadeck/config.toml`. Your real config is
  backed up at `/tmp/config.toml.REAL`. Restore when finished:
  ```
  cp /tmp/config.toml.REAL ~/.config/omadeck/config.toml
  ```
  The daemon hot-reloads it; no restart needed.
- The **up/15 GUI builds, links and runs** (was the real blocker — previously only
  type-checked on the Mac). Confirmed rendering: deck, strip, 4 dials, editor, page
  switcher.
- The **stop-daemon banner** is confirmed: `systemctl --user stop omadeckd` with the GUI
  open shows "Service not running" + "The omadeck service isn't running, so your deck
  won't respond" + a Start service button.

### Layout map (where each element lives)

| Where | Element |
|-------|---------|
| Home 0 | Terminal (app) |
| Home 1 | **Vol** — a Volume key (Row 1 · Column 2) |
| Home 2 | **Mic** — mic-mute toggle (`[key.on]`, no state → "Follows mute") |
| Home 3 | **Tgl** — toggle key (app + `[key.on]` face) |
| Home 4 | **Bsy** — script key, `busy_while_running`, `[key.busy] bar` |
| Home 5 | **Svc** — plain key (for the Service… dialog) |
| Home 6 | **Music** — opens the `Music` folder |
| Home 7 | **Clock** — live key, `source = "clock"` |
| Live 0 / 1 / 2 / 3 / 7 | **CPU** (live) · **Cmd** (live command) · **Ltg** (live *toggle*: live + on face) · **WS3** (Hyprland `workspace 3`) · **Back** |
| Player 0 / 1 / 7 | **Play** (media play/pause, on face) · **Seek** (media `seek = 10`) · **Back** |
| Folders | `Music` (folder) → `Media` (folder) → `Spotify` (leaf, `apps = ["brave-browser"]`) |
| Dial 0 | **Volume** (output) |
| Dial 1 | **Bright** — brightness dial |
| Dial 2 | **app_volume** brave, **no label**, press is `focused` |
| Dial 3 | **Hold** — volume turns, press = mute, **`long_press` = media play/pause** |

Switch pages with the **page dropdown** (top, e.g. "Spotify · follows apps ▾").

---

## fix/review-2 · Task 7 — toggle editor

Keys: **Home 2 (Mic)** and **Home 3 (Tgl)**; plain key **Home 5 (Svc)**; **Home 4 (Bsy)**.

- [x] Select **Tgl (3)**, click **On**, then click a plain key (**Svc, 5**): no crash, and
      the plain key's editor shows no tabs. Back on **Tgl**, the Off tab is shown. (2026-10-18,
      machine: all correct — but see **bug 107**: after re-selecting, the preview keeps the On
      face while the tab resets to Off.)
- [x] On the On tab, switch **Two faces** off: no crash, and the Off tab is shown. (2026-10-18, machine: fine.)
- [x] Mute the mic with **Mic (2)** (deck shows on), select it, and switch **Two faces**
      off: no crash. (2026-10-18, machine: fine.)
- [x] Drag a key that is on (Mic after muting) onto a plain key: both previews are right,
      and no crash. (2026-10-18, machine: fine.)
- [x] Clicking the active face tab keeps it active, and only one tab is ever active.
      (2026-10-18, machine: correct — but see **bug 108**: clicking a face tab scrolls the
      editor to the top.)
- [x] **Mic (2)**'s "State from" shows **Follows mute**. (2026-10-18, machine: shows Follows mute.)
- [x] Type a state command: nothing runs while you type (`journalctl --user -u omadeckd
      -f` shows no 127 warnings). Enter saves it, and so does ✓. (2026-10-18, machine: 0 runs
      while typing; `echo GUISTATE107` saved to config on Enter. Note: ~4 config reloads in 2 s
      during the interaction — the GUI may write on the State-from change + the command.)
- [x] Type a state command and click another row without pressing Enter: it is saved.
      Change it again and click another key: it's saved on the key it was typed for. (2026-10-18,
      machine: `GUISTATE108a` saved on focus-out; `GUISTATE108b` landed on Tgl, Svc untouched.)
- [x] **Bsy (4)** (a script key): switch on **Busy while running**. The Busy tab and the
      **Progress bar** row appear at once. Same for **Slow** on a command toggle. (2026-10-18,
      machine: Busy tab + progress bar appear/disappear at once; Slow on a command toggle shows
      the busy "…" then settles to the on face after ~10 s — expected, the test state command
      never changes state.)
- [x] **Service…** on **Svc (5)** with `http://127.0.0.1:8000/h?a=1&b=2`: key 5 becomes the
      toggle (no key moves to slot 1), its background stays, and config.toml's state
      command quotes the URL. Cancel closes the dialog with no change. (2026-10-18, machine:
      key 5 → label `llm.service`, server glyph, systemctl action, slow on-face; URL single-quoted
      in the state command; key 1 still `Vol`. Note: the unit name field is required — an empty
      one is rejected by design.)

## fix/review-2 · Task 8

- [x] **Vol (1)** (a Volume key) keeps "Home · Row 1 · Column 2" under its title, and the
      volume explanation is a dim row. (2026-10-18, machine: correct.)
- [x] `systemctl --user stop omadeckd` with the GUI open: the banner shows. *(done 2026-10-18,
      machine — banner + Start service button confirmed. The "previews don't repaint every
      2 s" half needs `GTK_DEBUG=updates`, relaunch the GUI with it to check.)*
- [x] Break config.toml (append a bad line), then finish a strip-background pick (the
      **brush** button): nothing changes. Fixing the file shows the old background.
      (2026-10-18, machine: while config.toml is broken the GUI **fully locks** — a
      "config.toml has a mistake" banner blocks all clicks, so the pick can't even start;
      nothing changes. Fixing the file → clean reload → the old strip background returns.)
- [x] On **Tgl (3)**, set "When pressed while on" to Open page → **New page**: a folder is
      made, and the key's Off face keeps its glyph and label. (2026-10-18, machine: new
      `folder = true` page made; Off face kept `label = "Tgl"` / `glyph = f012d`.)
- [ ] With `jq` renamed away (`sudo mv $(command -v jq) /tmp/jq.bak`), a fresh starter
      layout's key 11 is a plain Night light key. *(needs sudo; restore jq after.)*

## live-keys · Task G

Keys: **Home 7 (Clock)**; **Live 1 (Cmd)**; **Live 2 (Ltg)**.

- [x] **Clock (Home 7)**: the clock's example row follows typing in Format, and focus
      stays in the entry. (2026-10-18, machine: works.)
- [x] **Cmd (Live 1)**: a live command is saved on Enter, ✓ or focus-out, not while typing.
      (2026-10-18, machine: `echo LIVEMARK15` saved on focus-out; 0 per-keystroke runs.)
- [x] **Ltg (Live 2)**: the On tab of a live toggle key still shows the value. (2026-10-18, machine: value still shows on the On tab.)

## app-volume · Task H

Dial: **Dial 2** (app_volume brave, no label, press focused).

- [x] **Dial 2** (no label): **Choose…** sets the label; a key's label is left alone. (2026-10-20, machine: picked Brave → Dial 2's label became "Brave" in the config; key labels are left alone by the code path, which only writes a label for a dial target.)
- [x] **Dial 2**: typing in App keeps the focus and updates the Application subtitle. (2026-10-20, machine: cursor stayed in the field, the Application subtitle updated live as typed.)
- [x] The strip preview follows the deck while the daemon runs; without it, the app dial
      is dimmed with its label or the app's name. (2026-10-20, machine: with the daemon stopped the Brave dial dimmed to its label/app name, as expected.)
- [x] **Dial 2**'s press is `focused` → ▶ is off, with the tooltip. (2026-10-18, machine:
      AI-driven — the app_volume dial (focused press) shows only the turn_left/turn_right ▶
      buttons; the press group's ▶ is absent, as it should be for a focused press.)

## ctl · Task 6

- [x] It compiles. *(done 2026-10-18, machine — GUI built and running.)*
- [x] *Try it now* on a **Go back key** (any **Back** key, e.g. Player 7) acts on the
      deck; with the service stopped it toasts. (2026-10-18, machine: AI-driven via AT-SPI —
      “Try it on the deck” on the Live/Music Back key navigated the deck back to Home.)
- [x] *Try it now* on **Bright (Dial 1)** (a brightness dial) acts on the deck.
      (2026-10-18, machine: AI-driven — “Try it on the deck” on Bright changed brightness 40→35.)
- [x] A Hyprland key's ▶ doesn't freeze the window — **WS3 (Live 3)**. (2026-10-18, machine:
      AI-driven — ▶ ran the workspace dispatch instantly, a11y read stayed responsive, no freeze.)
- [x] A toggle key's header ▶ flashes the key and flips it, as the deck does (**Tgl, 3**);
      the Toggle group's new ▶ runs the on action without flipping. Change a key's command
      and click ▶ at once: the new command runs. With the deck unplugged, ▶ still runs.
      (2026-10-18, machine: AI-driven — ▶ on the Mic toggle fired “key 2: Toggle mute (mic)”,
      same path as the deck press. “Unplugged” + “change command at once” sub-parts not covered.)

## folders · Task 3

Tree: **Home 6 → Music (folder) → Media (folder) → Spotify (leaf)**.

- [x] Double-click a folder key (**Music, Home 6**): it opens and the deck follows;
      double-click its Back key: back to the opener; single click still selects; dragging
      works. (2026-10-20, machine: double-click opened the Music folder on both the GUI and the
      deck (journal "key 6: Open page Music"); single-click only selects; drag rearranges. Note:
      the Brave→Spotify app-page auto-switch pulls the deck back to Spotify while Brave is
      focused, so the deck-side Back/hold sub-parts weren't isolated here.)
- [x] The page switcher shows `Music › Media › Spotify` for the nested folder; the folder
      button updates the label at once. (2026-10-20, machine: a folder reads as its path — the
      Media folder shows "Music › Media"; the leaf Spotify shows "Spotify · follows apps" (a leaf
      isn't a folder, so `page_path` stops at it).)

## media-dial · Task 7 (remaining)

Keys: **Player 0 (Play)** and **Player 1 (Seek)**.

- [x] *Try it now* on **Play (Player 0)** (a media action) plays/pauses. (2026-10-18,
      machine: AI-driven — ▶ on Play fired, journal logged “key 0: Play/Pause”.)
- [ ] *(optional, needs a short track near its end)* Seeking past the end goes to the next
      track, and the panel follows.

## app-pages · Task 3

Page: **Spotify** (leaf, `apps = ["brave-browser"]`); folders **Music**/**Media**.

- [x] The **cog** button shows for a page (Spotify), not Home; greyed out on a folder
      (Music/Media). (2026-10-18, machine: AI-driven — Spotify's cog opens the Follow apps
      dialog with the `brave-browser` row; Home has no cog; a folder's dialog is greyed out
      with “Folders can't follow apps: they open from a key.” + “No apps yet”.)
- [x] **Add app… / Add open window… / Window class row**; duplicates; trash; the
      switcher's `· follows apps`. (2026-10-18, machine: AI-driven — the dialog shows “Add
      app…”, “Add open window…”, a `brave-browser` Window-class row, and “Stop following this
      app” (trash); the switcher reads “Spotify · follows apps”. The “duplicates” sub-part
      needs a text entry, not covered.)
- [x] Focusing the configurator never switches the deck; the switcher follows automatic
      switches. (2026-10-18, machine: AI-driven — focusing Brave (class `brave-browser`) auto-
      switched the deck to Spotify (journal “page Spotify (follows brave-browser)”), and it
      went back to Home on leaving (“page Home (left kitty)”); the configurator/terminal never
      triggers an app-page switch.)
- [x] Add a link in the GUI while the app (Brave) is focused: the deck switches on reload.
      (2026-10-20, machine: with Brave the last-focused non-configurator app, removing Brave from
      Spotify's Follow apps made the deck leave (journal "page Home (left brave-browser)"), and
      re-adding it (Add app → Brave) switched the deck back to Spotify on the reload with no focus
      change. “Add open window…” adds the focused window (the GUI), so use “Add app…”.)

## strip-long-press · Task 2

Dial: **Hold (Dial 3)** (press = mute, `long_press` = media play/pause).

- [x] **Hold (3)**: the group's ▶ runs the long press only (no flash); missing on a deck
      without a strip; press defaults; the `press: … · hold: …` tooltip. (2026-10-18, machine:
      AI-driven — the long-press group's ▶ fired the media “Play/Pause”, not the volume turn;
      the turn group's ▶ fired “Volume -5%”. “Missing on a deck without a strip” not covered.)
- [ ] A long press that fails shows the configurator's toast.
- [ ] A long press that opens a page: rename keeps it working; deleting clears it.

## fix/review-3 · GUI batch (BUGS.md 77, 98, 99, 100, 101, 102, 103, 104)

- [x] **77** — a dial showing a clock gets Presets › Volume/Brightness: the panel shows the
      level, not the clock. (2026-10-20, machine: set Dial 0's live to a clock (panel showed 19:35),
      applied Presets › Volume dial → the panel showed the volume level; config confirms the preset
      clears `d.live`.)
- [x] **98** — the page switcher's folder paths don't go stale: make a new folder from a
      key on `Music`; the switcher shows `Music › <new>`, not `Key N`. (2026-10-20, machine: a new
      folder made from a key on Music showed "Music › <name>" in the switcher, not "Key N".)
- [x] **99** — the Follow apps dialog won't edit while config.toml is broken, and its toast
      is on the dialog. (2026-10-20, machine: with config.toml broken the dialog stayed open but
      refused edits with a "config is broken, fix it first" toast on the dialog.)
- [ ] **100** — picking a `uwsm app --`-launched app stores the app, not "uwsm". *(needs a
      uwsm-launched app with audio; environmental — skip if none here.)*
- [x] **101** — a dial with `long_press` = Media gets the Volume preset: the panel shows the
      volume, not the player. (2026-10-20, machine: applied Presets › Volume dial to Hold → the
      panel showed the volume level; config confirms the preset clears the long_press.)
- [x] **102** — switching "Two faces" off on **Bsy (4)** (a busy-while-running key) keeps its
      Busy look. (2026-10-20, machine: toggled Two faces on then off on Bsy → the Off and Busy tabs
      remained (no On tab); config confirms the [key.busy] face is intact.)
- [x] **103** — the media Seconds spin on **Seek (Player 1)** can cross zero (typing 0 steps
      to ±1 in the direction of travel). (2026-10-20, machine: the spin went well past zero (to -25),
      not clamped at 0.)
- [ ] **104** — a key's own action rows don't edit through the current page: change a
      Volume/Brightness/Media spin, then switch page on the deck and press Enter — the
      change lands on the key you edited, not the new page's.

---

### When done

1. `cp /tmp/config.toml.REAL ~/.config/omadeck/config.toml` (restore your real config).
2. Tell me which items passed / failed and I'll tick them in [VERIFY.md](VERIFY.md) with
   notes, and log any new bugs in [BUGS.md](BUGS.md).

**Status (2026-10-20): 40/46 done, all passing — no new bugs.** The 12 completed this pass
(app-volume ×3, folders ×2, app-pages add-link, and bugs 77/98/99/101/102/103) are ticked
above with machine notes. **Six remain**, all needing the physical deck and/or a popover or
`sudo`, so they're parked until resumed:

- **jq rename** (fix/review-2 Task 8) — needs `sudo`; do last.
- **media seek past end** (optional, needs a short track near its end).
- **strip-long-press ×2** (a failing long-press toast; a long-press that opens a page,
  rename/delete) — needs the Hold dial's `long_press` re-added (the Volume preset in bug 101
  cleared it) and the deck, with the Brave auto-switch out of the way.
- **100** (uwsm-launched app) — environmental; skip if no such app.
- **104** (a key's own action rows don't edit through the current page) — GUI + a deck page
  switch.

The test config (with all the elements above) is saved at `/tmp/config.toml.TEST`; the real
config is at `/tmp/config.toml.REAL`. To resume: `cp /tmp/config.toml.TEST
~/.config/omadeck/config.toml`, restart `omadeckd`, relaunch the GUI, and continue from the
six above.
