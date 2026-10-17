<h1 align="center"><img src="assets/dev.omadeck.Omadeck.svg" width="56" alt=""><br>omadeck</h1>

<p align="center">
  <b>Elgato Stream Deck support for <a href="https://omarchy.org">Omarchy</a>.</b><br>
  Launch apps, run scripts, play sounds, open websites and drive Hyprland, with your theme on every key.
</p>

<p align="center">
  <a href="https://github.com/thewizster/omadeck/actions/workflows/ci.yml"><img src="https://github.com/thewizster/omadeck/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="https://github.com/thewizster/omadeck/releases/latest"><img src="https://img.shields.io/github/v/release/thewizster/omadeck" alt="Latest release"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue" alt="MIT license"></a>
</p>

<p align="center">
  <a href="https://omadeck.app/"><b>Website and live demo</b></a> ·
  <a href="#install">Install</a> ·
  <a href="#roadmap">Roadmap</a>
</p>

<p align="center">
  <img src="docs/demo.gif" width="720" alt="Pressing Stream Deck keys opens apps, switches workspaces and locks the screen; switching theme recolours every key">
</p>

Written in Rust: a tiny background daemon drives the hardware, and a GTK4/libadwaita app lays out your keys.

<p align="center">
  <img src="docs/configurator.png" width="480" alt="The omadeck configurator">
</p>

## Features

- **Twelve action types**
  - **Launch app:** pick from your installed apps; the app's icon and name come along. Apps launch through `uwsm-app`, the same way Omarchy launches them.
  - **Run script:** optionally in a terminal so you can watch the output.
  - **Play sound:** through PipeWire (`pw-play`), with per-key volume.
  - **Open website:** one click fetches the site's icon for the key. The icon is requested from the site itself; DuckDuckGo's icon service is used only if the site offers none, and the app tells you when that happens.
  - **Hyprland:** any dispatcher. Lua (`hl.dsp.focus({ workspace = "3" })`) is passed straight to `hyprctl dispatch`. Classic syntax (`workspace 3`, `movetoworkspace 2`, `togglefloating`, `fullscreen`, `killactive`, `exec …`) is translated automatically on Hyprland versions that use a Lua config.
  - **Open page:** pages and folders, for more keys than your deck has. A new page starts with a Back key, so a key that opens it works like a folder.
  - **Next/previous page:** the next or previous page — for a key, a dial turn, or a flick of the Stream Deck +'s touch strip (left is next, right is previous, wrapping). Folders are skipped.
  - **Volume:** change the output or input (microphone) volume, or toggle mute. It goes through Omarchy's own volume commands when they're installed, so the same on-screen pop-up shows as your volume keys — otherwise it falls back to `wpctl`.
  - **Brightness:** change the panel brightness by ±%, or set it to an exact value. The value is remembered across restarts (in the state file, not your config), as in the Elgato app.
  - **App volume:** a dial or key for one app's volume (Spotify, a browser…), or for whichever app is focused. The strip shows the app's icon, name and level. A focused-app dial keeps controlling the last focused app that played audio when you move to a silent window.
  - **Media:** play/pause, next, previous and seek from keys or a dial; the strip shows the track, artist, cover and a moving progress bar. Play/pause, next and previous go through Omarchy's own media commands when they're there, so its pop-up shows as for your media keys, unless a media dial or key shows a player: then they go to that player through `playerctl`. Seeking and the strip need `playerctl` (`sudo pacman -S playerctl`).
  - **Multi action:** several of the above from one key or dial, in order, with **delays** between them, like the Elgato app's Multi Action. The configurator edits the steps as a list.
- **Toggle keys.** A second face (`[key.on]`) that follows real state: a shell command's exit status polled in the background (night light, stay awake, a service, …), the mute a volume mute key already changes, or a flip on each press. A command can report **busy** (exit 75) with a progress line — and a percentage for a bar — for things that take a while. Any key running a command or script can show the busy face for as long as it runs (`busy_while_running`). The configurator mirrors what the deck shows, and has toggle presets (mic mute, output mute, night light, stay awake, do-not-disturb, bluetooth, play/pause, and a service… dialog). A play/pause key's on face follows the player, which needs `playerctl`; without it the key still plays and pauses through Omarchy's media commands, but always shows its Play face.
- **Live keys.** A key, or a dial's panel on the touch strip, can show a value the daemon keeps up to date (`[key.live]`, `[dial.live]`): the time and date (any [strftime format](https://docs.rs/jiff/latest/jiff/fmt/strtime/), e.g. `%H:%M%n%a %d`), CPU, memory or battery %, the active Hyprland workspace, or the first line of a command's output, re-run every few seconds (`source = "command"`, `command = "curl -s 'wttr.in/?format=%t'"`, `interval = 900`). The key still runs its action on press. On the strip, levels draw a bar (Elgato's `$B1` layout) and text doesn't (`$A1`).
- **Icons your way**
  - PNG, JPEG, SVG, WebP or GIF images.
  - App icons from your icon theme.
  - **Nerd Font glyphs**, drawn in your theme's accent colour.
  - Labels are rendered in your Omarchy font.
- **Pages follow apps,** like Elgato's Smart Profiles. List window classes in a page's `apps` (or use **Follow apps** in the configurator's page bar), and the deck shows that page while one of those apps is focused, then goes back to where it was, open folder included, when you leave it. A page you pick by hand while the app is focused stays.
- **Stream Deck + dials.** Each dial runs any of the action types on turn left, turn right and press — a workspace dial, a volume dial, a launch-on-press dial.
- **Stream Deck + touch strip.** The strip shows each dial's label and icon in its own panel, in the theme's colours, following the same panel layouts as Elgato's own app. A volume dial shows its level and a groove bar in Elgato's `Indicator` layout, kept in sync when the volume changes elsewhere, and a brightness dial shows the panel's brightness. Tapping a panel presses its dial; the pressed panel highlights briefly. Hold a panel on the touch strip for a second action (`long_press` on a `[[dial]]`, Elgato's "long touch"); unset, a hold does what a tap does. A flick pages through, like the Elgato app. The strip and each dial panel can have a background image, like the Elgato app.
- **Theme aware.** Key backgrounds, labels, glyphs and the press highlight come from the active Omarchy theme. Run `omarchy theme set …` and the deck re-renders instantly.
- **Live editing.** Changes land on the hardware as you type, and the deck shows whichever page you're editing. Drag keys to rearrange them. Press a physical key to jump to it in the editor.
- **Robust.** Reconnects after unplug/replug, and a typo in a hand-edited config keeps the last good layout on the deck. Apps opened from the deck keep running when the daemon restarts or upgrades. Uses about 9 MB of RAM and 0% CPU at idle.
- **Undo and backups.** A deleted page or cleared key can be undone, and the app keeps backups of your config.
- **Portable.** Everything lives in `~/.config/omadeck/`, and imported icons are copied into it, so you can copy that folder to another machine.

<p align="center">
  <img src="docs/deck.png" width="560" alt="Starter layout as rendered on the deck">
</p>

## Install

### Prebuilt release (recommended, no Rust needed)

```bash
curl -LO https://github.com/thewizster/omadeck/releases/download/v0.2.0/omadeck-0.2.0-x86_64-linux.tar.gz
curl -LO https://github.com/thewizster/omadeck/releases/download/v0.2.0/SHA256SUMS
sha256sum -c SHA256SUMS
tar xzf omadeck-0.2.0-x86_64-linux.tar.gz && cd omadeck-0.2.0-x86_64-linux
scripts/install.sh
```

The installer uses the prebuilt binaries, enables the `omadeckd` user service and offers to set
up the udev rule (details below). If the deck isn't detected straight away, replug it once.

**Coming soon:** `omarchy pkg add omadeck-bin` from the Omarchy Package Repository. The package
is in review.

### From source

```bash
# Rust toolchain, if you don't have one yet
mise use -g rust@latest

git clone https://github.com/thewizster/omadeck && cd omadeck
scripts/install.sh
```

The script:

- builds a release binary (pinned to `Cargo.lock`), or uses the prebuilt ones from a release tarball, and installs `omadeck` + `omadeckd` to `~/.local/bin`;
- adds a launcher entry;
- enables the `omadeckd` systemd **user** service, which starts with your graphical session;
- if your user can't already open the deck, shows the udev rule and the exact `sudo` commands, and runs them only if you answer `y`.

To remove omadeck, run `scripts/uninstall.sh`. It leaves your config in place.

### Supported hardware

omadeck speaks to decks through the [`elgato-streamdeck`](https://crates.io/crates/elgato-streamdeck) crate.

| Model | Status |
|-------|--------|
| Stream Deck (V2), 15 keys | ✅ Tested |
| Stream Deck Original / MK.2, 15 keys | Expected to work |
| Stream Deck Mini / Mini MK.2, 6 keys | Expected to work |
| Stream Deck XL / XL V2, 32 keys | Expected to work |
| Stream Deck Neo, 8 keys | Keys expected to work; info screen and touch keys unsupported |
| Stream Deck +, 8 keys | Keys, dials (turn left/right, press) and the touch strip (one panel per dial; tap = press; hold = the dial's long press, else press; a horizontal flick changes page) |
| Stream Deck Pedal | Not supported (no display) |

Got one of the untested models? Please [file a hardware report](https://github.com/thewizster/omadeck/issues/new?template=hardware_report.yml), whether it works or not.

## Use it

Open **omadeck** from the launcher (<kbd>Super</kbd>+<kbd>Space</kbd>). On first run you get a starter layout:

| Row | Keys |
|-----|------|
| 1 | Terminal · Browser · Files · Screenshot · omarchy.org |
| 2 | Workspaces 1–5 (Hyprland) |
| 3 | Mute · Night light · Next wallpaper · a sound · Lock |

Click any key to change it, or clear it with the trash button. The ▶ button tests the action without touching the deck.

### Pages and folders

The switcher above the deck picks the page you're editing; **+** adds a page, and the folder, pencil and trash buttons mark it as a folder, rename or delete it. To make a folder, set a key to **Open page** and click **New page**: you get a page with a Back key already in place, and the key gets a folder symbol. Open it with **›** to fill it. The folder button marks a page as a folder: flicks and Next/previous page skip it. A page made with **New page** from an Open page action is a folder already. Double-click a folder key to open it, or a Back key to go up to the page that opens this one. The switcher shows a folder with its path, such as `Music › Media › Spotify`.

The cog button opens **Follow apps**: the apps a page follows. Add an installed app, one of the windows open now, or a window class by hand (`hyprctl activewindow` shows the focused window's class; `*` matches anything). While one of them is focused the deck shows the page; when you move to an app that isn't linked, it goes back to where it was. Folders and Home can't follow apps, and focusing the configurator never moves the deck.

On the deck, **Go back** returns to the page you came from, or Home, when you let go of the key; hold a Back key for half a second to go straight Home, as in the Elgato app. Next/previous page and flicks inside a folder, however deep, step from the page the folders hang off. Renaming a page updates every key that opens it.

### Command line

```text
omadeckd                        run the daemon (normally via systemd)
omadeckd list                   show connected decks
omadeckd page next|prev|home|NAME   next / previous page (as a flick), Home, or the page NAME
omadeckd brightness N|+N|-N     set the brightness to N%, or change it by N
omadeckd press 3 [PAGE]         press key 3 (keys count from 0) on the page the deck shows, or on PAGE
omadeckd dial 0 left [STEPS]    turn dial 0 left / right, or press it (`dial 0 press`)
omadeckd reload                 re-read the config and theme now; prints the error if it's broken
omadeckd state                  print the daemon's state as one JSON line
omadeckd snapshot x.png [PAGE]  render the layout to an image
```

`page`, `brightness`, `reload` and `state` drive the running daemon, and say so if it isn't running. `press` and `dial` go through the daemon when it runs, so the key flashes and page and brightness keys work; with no daemon (or no deck) they run the action themselves. They print one line on success, errors go to stderr with exit status 1. `state` prints JSON for `jq`:

```text
{"reply":"state","deck":{"model":"Stream Deck +",…},"page":"Media","pages":["Media","Apps"],"brightness":40}
```

They're handy in Hyprland bindings and scripts, for example to share one action between a key and a keyboard shortcut. Omarchy keeps your own binds in `~/.config/hypr/bindings.conf`; `bindd` adds a description, and `binded` repeats while held:

```conf
bindd = SUPER, F1, Next deck page, exec, omadeckd page next
bindd = SUPER SHIFT, F1, Previous deck page, exec, omadeckd page prev
bindd = SUPER, F2, Deck home page, exec, omadeckd page home
binded = SUPER, F3, Dim deck, exec, omadeckd brightness -10
binded = SUPER, F4, Brighten deck, exec, omadeckd brightness +10
bindd = SUPER, F5, Deck key 3, exec, omadeckd press 3
```

## Configuration

The config file is `~/.config/omadeck/config.toml`. The app edits it for you, but it's plain TOML and safe to edit by hand, even while the app is open; the daemon and the app reload it as soon as you save. If the app can't read your edit, it shows the error and leaves the file alone until you fix it. See [`examples/config.toml`](examples/config.toml).

Each time the app opens, and before it deletes a page, it keeps a copy of `config.toml` in `~/.local/state/omadeck/backups` (the newest 20). Deleting a page or clearing a key can also be undone from the notice that pops up.

```toml
brightness = 70               # start value; the deck remembers its last brightness, and an edit here sets it

[style]
follow_theme = true            # colours from the active Omarchy theme

[[key]]
index = 0                      # left-to-right, top-to-bottom, from 0
label = "Terminal"
icon = "icons/utilities-terminal.png"   # relative to ~/.config/omadeck
[key.action]
type = "app"
command = "xdg-terminal-exec"  # or a desktop id like "firefox.desktop"

[[key]]
index = 10
label = "Mute"
glyph = "\U000F075F"           # Nerd Font symbol (or "f075f" / "U+F075F")
[key.action]
type = "app"
command = "omarchy audio output volume mute-toggle"

[[key]]
index = 4
label = "Media"
glyph = "\U000F024B"
[key.action]
type = "page"                  # open another page
page = "Media"

[[page]]                       # top-level [[key]]s are the Home page; more pages go here
name = "Media"
# folder = true                # skipped by flicks and page_step
# apps = ["spotify"]           # shown while one of these window classes is focused

[[page.key]]
index = 0
label = "Back"
[page.key.action]
type = "back"                  # previous page, or Home
```

Action types:

| `type` | Fields |
|--------|--------|
| `app` | `command` |
| `script` | `path`, `terminal` |
| `sound` | `file`, `volume` (0.0–1.5) |
| `url` | `url` |
| `hyprland` | `dispatch` |
| `page` | `page` (a page `name`) |
| `back` | |
| `volume` | `device` (`output`/`input`), `change` (±%), `mute` |
| `page_step` | `change` (± pages, default 1) |
| `brightness` | `change` (±%) and/or `set` (absolute 0–100) |
| `app_volume` | `app` (a process name, app name, Flatpak id or desktop id), `focused` (follow the focused window instead), `change` (±%), `mute` |
| `media` | `command` (`play_pause`, the default, `next`, `previous`, `stop`), `seek` (± seconds; wins over `command`), `player` (a `playerctl --player` name; default: the one playing) |
| `multi` | `steps`: a list of actions, run in order (see [Multi actions](#multi-actions)) |
| `delay` | `ms`: wait this long before the next step (only inside a `multi`) |

Per-key look overrides are `background`, `label_color` and `show_label`. Dials take the same fields (`icon`, `glyph`, `background`, `label_color`, `show_label`) for their panel on the touch strip, plus `background_image`: an image behind the panel (200 × 100), over the strip's image. The strip itself takes `[strip] background_image`, one image across all the panels (800 × 100 on a Plus). Images of any size fill their area and are centre-cropped; a dial's image shows through where it's transparent, and labels flip dark on bright images.

On the Stream Deck +, dials are numbered left to right from 0. Each dial takes up to
three actions; turns run once per detent:

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

A fourth, `long_press`, runs when you press and hold the dial's panel on the touch strip
(Elgato's "long touch"). Unset, a hold does what a tap does: it runs `press`.

```toml
[dial.long_press]
type = "app"
command = "omarchy-menu"
```

A volume dial: turn to change the output volume, press to mute. Its panel on the
strip shows the current level and a groove bar (Elgato's `Indicator` layout), kept in
sync when the volume changes from the keyboard or elsewhere:

```toml
[[dial]]
index = 1
label = "Volume"
glyph = "f057e"
[dial.turn_left]
type = "volume"
change = -5
[dial.turn_right]
type = "volume"
change = 5
[dial.press]
type = "volume"
mute = true
```

An app volume dial: the same for one app's audio (all its streams together, capped at
100%). Its panel shows the app's icon, name and level, dimmed with `–` while the app
plays nothing. With `focused = true` instead of `app`, it follows the focused window:

```toml
[[dial]]
index = 2
turn_left = { type = "app_volume", app = "spotify", change = -5 }
turn_right = { type = "app_volume", app = "spotify", change = 5 }
press = { type = "app_volume", app = "spotify", mute = true }
```

A media dial: turn to scrub (seconds per detent), press to play or pause. Its panel
shows the track, artist, the cover (local files only) and a bar that moves while it
plays. Add `player = "spotify"` to stick to one player; `command = "next"` or
`"previous"` on a turn skips tracks instead:

```toml
[[dial]]
index = 3
label = "Media"
turn_left = { type = "media", seek = -5 }
turn_right = { type = "media", seek = 5 }
press = { type = "media" }
```

### Toggle keys

A key with a `[key.on]` table has a second face. Which face shows comes from the
real system, in this order of precedence:

1. **A state command** (`state = "…"` on `[key.on]`) is run in the background, every
   `interval` seconds (at least 1, default 2): exit **0** = on, **75** = busy, anything else
   = off. While busy, the command's first output line is the key's label — put a
   percentage in it (`echo 43%`) and, with `[key.busy] bar = true`, it becomes a
   progress bar. A `slow = true` toggle shows busy at once on a press and polls fast
   until the state moves, for things that take a while to come up or go down.
2. **The mute** a volume mute key already changes: a mute key with `[key.on]` and no
   state follows its mute.
3. **A flip on each press**, as the Elgato app's two-state actions do.

`[key.on] action` runs instead of the key's action while it's on (Elgato's Hotkey
Switch); unset, a press runs the key's own action.

Separately, any key running a plain command or script can set `busy_while_running =
true` to look busy — with the command's last output line — for as long as it runs;
the command keeps running if the daemon restarts (it runs in its own systemd scope),
but the key stops showing it, and a program that writes progress may end when nothing
reads it any more.

```toml
[[key]]
index = 11
label = "Night light"
[key.action]
type = "app"
command = "omarchy-toggle-nightlight"
[key.on]
label = "Night on"
state = "omarchy-toggle-nightlight --status | jq -e .enabled"
```

### Multi actions

A `multi` action runs its steps in order from one key, dial turn or dial press, as the
Elgato app's Multi Action does. A `delay` step waits; the daemon times it, so the deck
stays responsive. Each step is an ordinary action table:

```toml
[[key]]
index = 5
label = "Meeting"
[key.action]
type = "multi"
[[key.action.steps]]
type = "volume"
device = "input"
mute = true
[[key.action.steps]]
type = "app"
command = "zoom.desktop"
[[key.action.steps]]
type = "delay"
ms = 2000
[[key.action.steps]]
type = "hyprland"
dispatch = "movetoworkspace 5"
[[key.action.steps]]
type = "page"
page = "Meeting"
```

- Steps **start** in order; none waits for the one before it to finish. Apps, scripts,
  sounds and websites have been launched before the next step runs.
- Page, Go back, Next/previous page and Brightness steps have taken effect before the next
  step.
- Hyprland steps are queued in order (with every other Hyprland action), and volume steps
  likewise, but the next step doesn't wait for them to land: put a `delay` (100 ms is
  plenty for volume) after one when the next step depends on it.
- A step that fails is reported with its number, and the rest still run.
- Pressing the key again while its multi runs (delays included) does nothing.
- A multi can't contain another multi; a hand-edited one fails that step.

## How it works

```
crates/
  omadeck-core/   config, Omarchy theme, key renderer, actions, IPC types
  omadeckd/       daemon: owns the USB device, paints keys, runs actions
  omadeck-gui/    GTK4 + libadwaita configurator (binary: omadeck)
packaging/        systemd unit, udev rule, desktop entry, AUR PKGBUILDs
scripts/          install / uninstall / package a release
site/             project website: static HTML/CSS/JS with an interactive demo
```

- **Single source of truth.** The GUI only writes `config.toml` (atomically). The daemon watches it, plus Omarchy's theme state in `~/.local/state/omarchy/current`, and repaints.
- **Same pixels everywhere.** The renderer in `omadeck-core` is shared, so the previews in the app are rendered by the same code that paints the hardware.
- **Event feed and control.** The daemon publishes connect/disconnect, page and key events as JSON lines on `$XDG_RUNTIME_DIR/omadeck.sock` (mode 0600). The app uses this for its status line, press-to-select and following the deck's page, and sends back the page you're editing so the deck can show it. The same socket takes commands (`{"command":"press","index":3}`, `{"command":"run","action":{"type":"page_step"}}`, `{"command":"state"}`, …), each answered with one `{"reply":…}` line on the connection that sent it; `omadeckd page`, `brightness`, `press`, `dial`, `reload` and `state` are thin clients of it, and *Try it now* in the app goes through it too. A client that stops reading loses events; if a reply can't be queued, the daemon closes that connection.

## Troubleshooting

- **"Service not running"**: `systemctl --user status omadeckd` (press <kbd>q</kbd> to leave its pager, or add `--no-pager`). Logs: `journalctl --user -u omadeckd -f`.
- **Deck found but can't be opened**: install the udev rule (`packaging/70-omadeck.rules` → `/etc/udev/rules.d/`), then replug the deck.
- **Only one program can own the deck.** Quit the Elgato software or other tools such as `streamdeck-ui` first.

## Roadmap

Ideas, roughly in order; [docs/ROADMAP.md](docs/ROADMAP.md) has the details. Want one sooner? Open an issue or a PR; see [CONTRIBUTING](CONTRIBUTING.md).

- [x] **Pages and folders** (0.2.0)
- [ ] **Release actions**: keys that act on release too, for push-to-talk dictation
- [ ] **Templates**: ready-made pages to add in one click, starting with Omarchy Basics (your AI agent, dictation and everyday Omarchy keys); save any page as a template and share it
- [x] **Toggle keys** with on/off faces (mute, night light, …) that reflect real state
- [x] **Multi-actions**: run several actions from one key, with optional delays
- [x] **Stream Deck +**: value layouts on the strip — volume, brightness and media — and page switching on swipe
- [x] **Stream Deck +**: background images on the touch strip — one across the strip, one per dial
- [ ] **OBS** scene and recording keys via obs-websocket
- [x] **Live keys** that show a clock, CPU usage or the active workspace
- [x] **Per-app volume** (chosen or focused app)
- [x] **App pages**: a page follows the focused app, like Elgato's Smart Profiles
- [ ] **Multiple decks** with a layout per serial number

## How this was built

omadeck was built on Omarchy by Raymond Brady together with Claude Code (Anthropic's AI coding agent), as a showcase of what that pairing can do. Every change goes through the same checks as any contribution: `rustfmt`, `clippy -D warnings`, tests, CI and testing on real hardware.

## License

MIT © Raymond Brady
