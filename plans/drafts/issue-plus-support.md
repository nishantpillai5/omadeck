# Stream Deck + support: dials and the touch strip (a PR series)

Hi! I've been running omadeck on a Stream Deck + and built support for its dials
and touch strip, plus a few features from the README's roadmap along the way. Before opening
anything I wanted to check whether you'd like it, and how you'd prefer to receive it.

## The first PR

Stream Deck + support: the dials run any action on turn and press, and the touch strip shows
a panel per dial that you can tap, following the Elgato app's layouts. Other decks work
exactly as before.

## The rest, if you want it

Each as its own PR, in this order:

1. Stream Deck + support
2. Volume dials, with the level shown on the strip
3. Flick the strip to change page
4. Brightness dials
5. Background images on the strip
6. Daemon groundwork the later features build on
7. Toggle keys that show real state (mute, night light, …)
8. Live keys: the time, CPU, battery, workspace, …
9. Per-app volume
10. Control the running daemon from the shell
11. Multi actions
12. A media dial
13. Folder gestures: hold Back to go Home
14. Pages that follow the focused app
15. A second action when you hold a strip panel

They follow the Elgato app wherever it has an answer.

If you'd like to see how far it's got, it's all on my fork,
[nishantpillai5/omadeck](https://github.com/nishantpillai5/omadeck): `daily` has everything
together, and the `up/01` … `up/15` branches are the PRs, one on top of the other.

Would you like these, and is one PR at a time, in this order, OK? I'm happy to merge or split
any of them.
