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

- [ ] Select **Tgl (3)**, click **On**, then click a plain key (**Svc, 5**): no crash, and
      the plain key's editor shows no tabs. Back on **Tgl**, the Off tab is shown.
- [ ] On the On tab, switch **Two faces** off: no crash, and the Off tab is shown.
- [ ] Mute the mic with **Mic (2)** (deck shows on), select it, and switch **Two faces**
      off: no crash.
- [ ] Drag a key that is on (Mic after muting) onto a plain key: both previews are right,
      and no crash.
- [ ] Clicking the active face tab keeps it active, and only one tab is ever active.
- [ ] **Mic (2)**'s "State from" shows **Follows mute**.
- [ ] Type a state command: nothing runs while you type (`journalctl --user -u omadeckd
      -f` shows no 127 warnings). Enter saves it, and so does ✓.
- [ ] Type a state command and click another row without pressing Enter: it is saved.
      Change it again and click another key: it's saved on the key it was typed for.
- [ ] **Bsy (4)** (a script key): switch on **Busy while running**. The Busy tab and the
      **Progress bar** row appear at once. Same for **Slow** on a command toggle.
- [ ] **Service…** on **Svc (5)** with `http://127.0.0.1:8000/h?a=1&b=2`: key 5 becomes the
      toggle (no key moves to slot 1), its background stays, and config.toml's state
      command quotes the URL. Cancel closes the dialog with no change.

## fix/review-2 · Task 8

- [ ] **Vol (1)** (a Volume key) keeps "Home · Row 1 · Column 2" under its title, and the
      volume explanation is a dim row.
- [x] `systemctl --user stop omadeckd` with the GUI open: the banner shows. *(done 2026-10-18,
      machine — banner + Start service button confirmed. The "previews don't repaint every
      2 s" half needs `GTK_DEBUG=updates`, relaunch the GUI with it to check.)*
- [ ] Break config.toml (append a bad line), then finish a strip-background pick (the
      **brush** button): nothing changes. Fixing the file shows the old background.
- [ ] On **Tgl (3)**, set "When pressed while on" to Open page → **New page**: a folder is
      made, and the key's Off face keeps its glyph and label.
- [ ] With `jq` renamed away (`sudo mv $(command -v jq) /tmp/jq.bak`), a fresh starter
      layout's key 11 is a plain Night light key. *(needs sudo; restore jq after.)*

## live-keys · Task G

Keys: **Home 7 (Clock)**; **Live 1 (Cmd)**; **Live 2 (Ltg)**.

- [ ] **Clock (Home 7)**: the clock's example row follows typing in Format, and focus
      stays in the entry.
- [ ] **Cmd (Live 1)**: a live command is saved on Enter, ✓ or focus-out, not while typing.
- [ ] **Ltg (Live 2)**: the On tab of a live toggle key still shows the value.

## app-volume · Task H

Dial: **Dial 2** (app_volume brave, no label, press focused).

- [ ] **Dial 2** (no label): **Choose…** sets the label; a key's label is left alone.
- [ ] **Dial 2**: typing in App keeps the focus and updates the Application subtitle.
- [ ] The strip preview follows the deck while the daemon runs; without it, the app dial
      is dimmed with its label or the app's name.
- [ ] **Dial 2**'s press is `focused` → ▶ is off, with the tooltip.

## ctl · Task 6

- [x] It compiles. *(done 2026-10-18, machine — GUI built and running.)*
- [ ] *Try it now* on a **Go back key** (any **Back** key, e.g. Player 7) acts on the
      deck; with the service stopped it toasts.
- [ ] *Try it now* on **Bright (Dial 1)** (a brightness dial) acts on the deck.
- [ ] A Hyprland key's ▶ doesn't freeze the window — **WS3 (Live 3)**.
- [ ] A toggle key's header ▶ flashes the key and flips it, as the deck does (**Tgl, 3**);
      the Toggle group's new ▶ runs the on action without flipping. Change a key's command
      and click ▶ at once: the new command runs. With the deck unplugged, ▶ still runs.

## folders · Task 3

Tree: **Home 6 → Music (folder) → Media (folder) → Spotify (leaf)**.

- [ ] Double-click a folder key (**Music, Home 6**): it opens and the deck follows;
      double-click its Back key: back to the opener; single click still selects; dragging
      works.
- [ ] The page switcher shows `Music › Media › Spotify` for the nested folder; the folder
      button updates the label at once.

## media-dial · Task 7 (remaining)

Keys: **Player 0 (Play)** and **Player 1 (Seek)**.

- [ ] *Try it now* on **Play (Player 0)** (a media action) plays/pauses.
- [ ] *(optional, needs a short track near its end)* Seeking past the end goes to the next
      track, and the panel follows.

## app-pages · Task 3

Page: **Spotify** (leaf, `apps = ["brave-browser"]`); folders **Music**/**Media**.

- [ ] The **cog** button shows for a page (Spotify), not Home; greyed out on a folder
      (Music/Media).
- [ ] **Add app… / Add open window… / Window class row**; duplicates; trash; the
      switcher's `· follows apps`.
- [ ] Focusing the configurator never switches the deck; the switcher follows automatic
      switches.
- [ ] Add a link in the GUI while the app (Brave) is focused: the deck switches on reload.

## strip-long-press · Task 2

Dial: **Hold (Dial 3)** (press = mute, `long_press` = media play/pause).

- [ ] **Hold (3)**: the group's ▶ runs the long press only (no flash); missing on a deck
      without a strip; press defaults; the `press: … · hold: …` tooltip.
- [ ] A long press that fails shows the configurator's toast.
- [ ] A long press that opens a page: rename keeps it working; deleting clears it.

## fix/review-3 · GUI batch (BUGS.md 77, 98, 99, 100, 101, 102, 103, 104)

- [ ] **77** — a dial showing a clock gets Presets › Volume/Brightness: the panel shows the
      level, not the clock. *(set any dial's live to a clock first, e.g. copy Home 7's
      clock onto Dial 0, then apply the preset.)*
- [ ] **98** — the page switcher's folder paths don't go stale: make a new folder from a
      key on `Music`; the switcher shows `Music › <new>`, not `Key N`.
- [ ] **99** — the Follow apps dialog won't edit while config.toml is broken, and its toast
      is on the dialog. *(open Spotify's cog, break config.toml, try to edit.)*
- [ ] **100** — picking a `uwsm app --`-launched app stores the app, not "uwsm". *(needs a
      uwsm-launched app with audio; environmental — skip if none here.)*
- [ ] **101** — a dial with `long_press` = Media gets the Volume preset: the panel shows the
      volume, not the player. **Hold (Dial 3)** is exactly this — apply Presets › Volume
      dial to it.
- [ ] **102** — switching "Two faces" off on **Bsy (4)** (a busy-while-running key) keeps its
      Busy look.
- [ ] **103** — the media Seconds spin on **Seek (Player 1)** can cross zero (typing 0 steps
      to ±1 in the direction of travel).
- [ ] **104** — a key's own action rows don't edit through the current page: change a
      Volume/Brightness/Media spin, then switch page on the deck and press Enter — the
      change lands on the key you edited, not the new page's.

---

### When done

1. `cp /tmp/config.toml.REAL ~/.config/omadeck/config.toml` (restore your real config).
2. Tell me which items passed / failed and I'll tick them in [VERIFY.md](VERIFY.md) with
   notes, and log any new bugs in [BUGS.md](BUGS.md).
