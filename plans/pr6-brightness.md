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

- [ ] Dial 3 (mic): turn changes the input volume (the mic OSD pop-up shows),
      press mutes the mic, and the panel shows the mic level.
- [ ] Dial 4 (brightness): turn dims/brightens the whole deck live; the panel
      shows the current percent; at 0 the panel is dark but still readable
      enough to turn it back.
- [ ] Unplug/replug (or restart the service): the brightness comes back at the
      value the dial left it at (persisted), not the old config default.
- [ ] The GUI's brightness slider and the dial agree after a reload.

**Size:** about 400–500 lines.
