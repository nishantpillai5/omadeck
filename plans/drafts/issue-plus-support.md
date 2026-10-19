# Stream Deck + support: dials and the touch strip (a PR series)

Hi! I've been running omadeck on a Stream Deck + (`0fd9:0084`) and built support for its dials
and touch strip, plus a few features from the README's roadmap along the way. Before opening
anything I wanted to check whether you'd like it, and how you'd prefer to receive it.

## What the first PR adds

Stream Deck + support, as one PR (it's only useful as a whole):

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
  `omadeckd press`.
- **The strip.** One 200×100 panel per dial, drawn with Elgato's own `$X1` "Icon" layout in
  the theme's colours. A tap runs that dial's press and highlights its panel, as in Elgato's
  app. The configurator shows the dials under the key grid and previews the strip, and the
  key and dial editors share their look rows.

Other decks are untouched: with no dials or strip nothing new runs.

About 1,750 lines in three commits, tested on the Stream Deck +.

## The rest, if you want it

I've kept everything else as separate PRs, each on top of the one before, each building and
passing `fmt`, `clippy -D warnings` and the tests on its own:

1. Stream Deck + support (above)
2. A volume action, with a live level on a dial's strip panel (Elgato's `$B1` layout)
3. Page flicks on the strip, and a `page_step` action
4. A brightness action, kept across restarts in a state file
5. Background images on the strip and per dial
6. Daemon groundwork: a watcher thread for system reads, a daemon split, and an event hub
   that never blocks on a slow client
7. Toggle keys: a second face that follows real state (a command's exit status, a mute),
   with presets
8. Live keys: the time, CPU, memory, battery, the Hyprland workspace or a command's output
9. Per-app volume (one app, or the focused one)
10. `omadeckd page|brightness|reload|state` and presses through the running daemon
11. Multi actions (several actions from one key, with delays)
12. A media dial: scrub, play/pause, track and progress on the strip
13. Folder gestures: hold Back for Home, and Back acts on release
14. Pages that follow the focused app
15. A long press on a strip panel as a dial's second action

They follow the Elgato app wherever it has an answer: its panel layouts, Multi Action,
Smart Profiles, the long touch.

## Questions

- Would you like these, and is one PR at a time, in this order, OK? Some are small (3, 13,
  15); I'm happy to merge or split any of them.
- **Release actions.** Your roadmap's next item is `on_release`. PR 13 makes a Back key act on
  release (so holding it can go Home, as in Elgato's app) and keeps a held key drawn pressed
  until it's let go. Happy to fit that around your design, or to wait for it.
- Anything about the config format above you'd want different before the rest builds on it?

The branches are on my fork (`up/01-plus-support` … `up/15-strip-long-press`), rebased on
v0.2.0.
