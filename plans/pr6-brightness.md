# PR 6: `feat/brightness`: the deck's brightness as an action

Part of [PLAN.md](PLAN.md). This is the roadmap's remaining "brightness" dial
behaviour, asked for directly: dial 4 should set the Stream Deck panel's
brightness.

**Outcome:** a `brightness` action (change by ±%, or set an absolute 0–100) that
keys and dials can run. A brightness dial shows its value on the touch strip in
the same `$B1` Indicator layout as the volume dials. The brightness is persisted
to the config (like the Elgato app, which remembers it per profile), so it
survives a restart.

## Design

- `Action::Brightness { change: i8, set: Option<u8> }` — `set` wins over
  `change` (a key sets an absolute value; a dial turns relatively). New `KINDS`
  entry "Brightness" (index 8).
- The daemon owns the live value (`Deck.brightness`, like the page): applied with
  `set_brightness()`, clamped 0–100, persisted by loading the config, changing
  only `brightness`, and saving atomically — a hand-edited file keeps its other
  edits, and a broken file is never overwritten (warn instead). The reload that
  the write triggers just reads back the same value; no loop.
- Pure `apply_brightness(current, set, change) -> u8` in the core, unit-tested.
- The strip panel: `DialConfig::level_source()` generalises `volume_device()`:
  `Volume(AudioDevice)` or `Brightness`, volume winning when a dial has both.
  The daemon keeps one `Option<LevelSource>` per dial; a brightness dial's panel
  reads `Deck.brightness` directly (no worker — it's in-process state).
- The GUI's `save_now` adopts the config's `brightness` from disk before saving,
  so a GUI save can't stomp a brightness the dial just persisted.
- GUI: a "Brightness" arm in the action editor (change per turn / set to a
  value), a **Brightness dial** preset next to **Volume dial**, and the strip
  preview shows the config's brightness for brightness dials. *Try it now* is
  disabled for brightness actions, as for page actions (the daemon owns the
  value).
- Default panel glyph for a brightness dial without its own icon: `f00de`
  (Material Design `brightness_5`).

## Hardware checks

- [x] Dial 3 (mic): turn changes the input volume (the mic OSD pop-up shows),
      press mutes the mic, and the panel shows the mic level.
- [x] Dial 4 (brightness): turn dims/brightens the whole deck live; the panel
      shows the current percent.
- [x] Restart the service: the brightness comes back at the value the dial left it
      at (persisted), not the old config default.
- [x] The GUI's brightness slider and the dial agree after a reload (the
      configurator now watches the config file, so it picks up a brightness the
      dial persisted).
- [ ] At 0 the panel is dark but still readable enough to turn it back. (Not tested
      yet; the hardware has a brightness floor, so the panel never fully blacks out.)

All confirmed on the Stream Deck + (serial A00WA5081LZEYC, firmware 2.0.3.2).

## Fixes that came out of running it

All in the same commit (`b8fae3b`):

- **Optimistic volume/mic bars.** A volume action moved the bar only after the audio
  worker read the new value back (~200 ms later), so the bar lagged the turn while the
  brightness bar (in-process state) was instant. Now a volume action moves the bar
  immediately from the last known level plus the change, and the read-back a moment
  later corrects it — the same number for a relative change, so the correction is
  invisible.
- **No blanking on config reload.** Persisting a brightness writes the config, which
  trips the daemon's watcher and reloads it. The reload used to clear the cached device
  levels and repaint, blanking the volume/mic bars for a moment until the worker
  re-polled. It now keeps the levels of any device still shown and drops only what is
  no longer watched.
- **Configurator watches the config file.** The app loaded the config once at startup
  and never re-read it, so a hand edit (or a brightness the dial persisted) didn't show
  until a restart. It now watches the config directory like the daemon does and reloads
  on a settled change, skipping the reload while a user edit is pending and right after
  its own save.

**Size:** about 450 lines (estimated 400–500).
