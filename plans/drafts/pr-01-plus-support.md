# Stream Deck + support: dials and the touch strip

Draft description for `up/01-plus-support` (the technical part kept out of the issue).

## What it adds

- **Input.** The daemon reads dial turns (with the signed delta, which exceeds ±1 on a fast
  spin), dial presses, and touch-strip taps, long presses and swipes, and reports them on the
  event socket. `DeckInfo` gains the dials and the strip; the new fields have serde defaults,
  so older and newer configurators and daemons still read each other's events.
- **Dial actions.** `[[dial]]` tables, one per dial, each with up to three ordinary actions:

  ```toml
  [[dial]]
  index = 0
  label = "Workspace"
  [dial.turn_left]
  type = "hyprland"
  dispatch = "workspace e-1"
  [dial.turn_right]
  type = "hyprland"
  dispatch = "workspace e+1"
  [dial.press]
  type = "app"
  command = "walker"
  ```

  Any action type works, so there's no new action language. A turn runs once per detent (at
  most 8 per event, so a hard spin can't fork a storm of processes). Old configs load
  unchanged and save without dial tables. `omadeckd dial N left|right|press` sits next to
  `omadeckd press`. Undo for a deleted page re-links the dials that opened it.
- **The strip.** One 200×100 panel per dial, drawn with Elgato's `$X1` "Icon" layout in the
  theme's colours. A tap runs that dial's press and highlights its panel, as in Elgato's app.
  The configurator shows the dials under the key grid and previews the strip, and the key and
  dial editors share their look rows.

Other decks are untouched: with no dials or strip nothing new runs.

## Tested

On a Stream Deck + (`0fd9:0084`): every input shows up on the socket, dial and tap actions
run, the strip draws on the right panels and repaints on reload and theme change. `cargo fmt`,
`cargo clippy --all-targets -- -D warnings` and `cargo test` pass.
