# Changelog

All notable changes to omadeck are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Pages can follow apps, like Elgato's Smart Profiles: `apps = ["spotify"]` on a
  `[[page]]` (Hyprland window classes, `*` matches anything) shows that page while one of
  those apps is focused, and goes back to where the deck was, open folder included, when
  you move to an app that isn't linked. A page picked by hand meanwhile stays. The
  configurator's page bar has a **Follow apps** dialog: add an installed app, an open
  window or a class.
- Hold a Back key for half a second to go straight Home, as in the Elgato app. In the
  configurator, double-click a folder key to open it or a Back key to go up; the page
  switcher shows a folder with its path (`Music › Media › Spotify`).
- Media: a `media` action plays or pauses, skips to the next or previous track, stops,
  or seeks by a number of seconds. A media dial scrubs on a turn and plays or pauses on a
  press; its strip panel shows the track, artist, cover (local files) and a bar that
  moves while it plays. A play/pause key's on face follows the player. Play/pause, next
  and previous go through Omarchy's media commands when it has them, unless a media panel
  or key shows a player: then they go to that player through `playerctl`. Seeking and the
  panel need `playerctl`. The configurator has a Media dial preset and a Play/Pause key
  preset.
- A `multi` action runs several actions in order from one key or dial, with `delay`
  steps in between, as in the Elgato app's Multi Action. The configurator edits the steps
  as a list.
- `omadeckd page`, `brightness`, `reload` and `state` control the running daemon, e.g.
  from Hyprland binds. `press` and `dial` go through the daemon when it runs, so the key
  flashes and page and brightness keys work. *Try it now* runs through the daemon, so Go
  back, Next/previous page and Brightness work there too, and a key's own ▶ presses the
  key, toggle flip included. The socket is now 0600.
- App volume: an `app_volume` action changes or mutes one app's audio (all its
  streams), chosen by name or following the focused Hyprland window (staying on the
  last focused app that played audio). On a dial the strip panel shows the app's icon,
  name and level, dimmed while it plays nothing. The configurator has an app picker
  listing what's playing, and the dial presets move into a Presets menu with App volume
  and Focused app dials. Text on `$B1` panels now picks its contrast colour from what's
  under it, not the whole panel.
- Live keys: a key or a dial's strip panel can show the time, CPU, memory, battery, the
  active Hyprland workspace or a command's output, kept up to date by the daemon.
- Toggle keys: a second face (`[key.on]`) that follows real state — a shell command's
  exit status polled in the background (exit 0 = on, 75 = busy with a progress line,
  else off), the mute a volume mute key already changes, or a flip on each press. A
  `slow = true` toggle shows busy at once and polls fast until the state moves (at most 15
  seconds); `[key.busy] bar = true` draws a progress bar from the first `NN%`. Any key
  running a command or script can set `busy_while_running = true` to look busy for as long
  as it runs (the command keeps running if the daemon stops). The configurator mirrors
  what the deck shows, edits each face in its own tab, and has toggle presets: mic mute,
  output mute, night light, stay awake, do-not-disturb, bluetooth, and a service… dialog.
- Stream Deck +: flicking the touch strip changes page (left is next, right is
  previous, wrapping), and a `page_step` action moves to the next or previous page,
  on a key or per dial turn. Folders are skipped, so a flick can't land inside one;
  the configurator's folder button marks a page as one. Dials and the strip are the
  same on every page.
- A `brightness` action: change the panel brightness by ±%, or set it to an exact
  value. It is remembered across restarts in the state file (the config's value is
  the start value), as in the Elgato app. A brightness dial shows its value and a
  groove bar on the touch strip, and the configurator has a one-click **Brightness
  dial** preset.
- Stream Deck +: background images on the touch strip. One image across the whole
  strip (`[strip] background_image`) and one per dial (`background_image` on
  `[[dial]]`, which covers the strip image for that panel); both fill the area and
  are centre-cropped. The configurator picks them (right-clicking the strip preview
  sets the strip's), labels flip dark on bright images, and the images are cached per
  file, so a reload re-decodes only files that changed.
- A `volume` action: change the output or input (microphone) volume, or toggle mute.
  It runs through Omarchy's own volume commands when they're installed (so its on-screen
  pop-up shows, just like the volume keys) and falls back to `wpctl` otherwise. A fast
  dial spin folds into a single command.
- Stream Deck +: a volume dial shows its level and a groove bar on the touch strip in
  Elgato's `Indicator` layout, kept in sync when the volume changes elsewhere; the
  configurator has a one-click **Volume dial** preset.
- Stream Deck +: the daemon reads dial turns and presses and touch-strip taps and
  swipes, and reports them on its event socket.
- Dials on the Stream Deck + can run any action on turn left, turn right and press,
  configured with `[[dial]]` tables or in the configurator.
- The Stream Deck + touch strip is drawn: one panel per dial (label and icon, in the
  theme's colours, using Elgato's panel layouts). Tapping a panel runs that dial's press
  action and highlights the panel; the configurator previews the strip and edits each
  panel's look.

### Changed
- A Back key goes back when you let go of it, not when you press it, so holding it can
  go Home.
- With the daemon running, the configurator's brightness slider goes through the daemon
  instead of writing `config.toml`, and its strip preview shows what the deck shows
  instead of reading the system itself. A configurator that stops reading the daemon's
  socket can no longer hold up the deck.

### Fixed
- Hyprland actions never hold up the deck.
- `hyprctl` and `pactl` calls time out, so a stalled Hyprland or audio server can't hang
  the deck.
- A tapped strip panel's highlight ends on time.

## [0.2.0] - 2026-10-18

### Added
- Pages and folders. Extra pages live in `[[page]]` tables (top-level keys stay the Home page),
  with two new actions: `page` opens a page by name and `back` returns to the previous one.
  The configurator gets a page switcher (new, rename, delete), and "New page" on a key builds a
  folder with a Back key in place. The deck follows the page you're editing, and the app
  follows page changes made on the deck.
- `omadeckd press KEY [PAGE]` and `omadeckd snapshot FILE [PAGE]` take an optional page.
- Undo for deleting a page or clearing a key, and backups of `config.toml` in
  `~/.local/state/omadeck/backups` each time the app opens and before a page is deleted
  (the newest 20 are kept).
- The app warns when a page has no Back key, so the deck would get stuck on it, and offers to
  add one.

### Fixed
- A colour value with a non-ASCII character (in the config or the Omarchy theme) crashed the
  daemon, and systemd restarted it into the same crash.
- Websites, scripts and terminals opened from the deck ran inside the omadeckd service, so
  restarting or upgrading it closed them. They now get their own scope (via `uwsm-app`, or
  `systemd-run --scope` without it), like launched apps.
- The configurator now picks up edits made to `config.toml` while it's open instead of
  overwriting them on its next save.
- A config the configurator can't parse is no longer renamed to `config.toml.broken` and
  replaced with an empty one. The app shows the error and won't save until the file is fixed.
- A site icon or file chosen for a key no longer lands on another page's key if you switch
  pages before it finishes.
- Changing the system font (`omarchy font set`) now updates key labels without a restart, and
  icon glyphs no longer vanish when the new font has no Nerd Font symbols.
- Live reload keeps working after the config folder is deleted and recreated, and with a
  symlinked `config.toml`. Saving from the configurator keeps the symlink.

## [0.1.0] - 2026-10-02

The first public release.

### Added
- `omadeckd`, a small Rust daemon (systemd user service) that drives the Stream Deck:
  hotplug, live config reload, and a last-good-config fallback when a hand edit has a typo.
- `omadeck`, a GTK4/libadwaita configurator: click a key to edit it, drag to rearrange,
  press a physical key to jump to it, test any action with ▶.
- Five action types: launch app (via `uwsm-app`), run script (optionally in a terminal),
  play sound (PipeWire), open website, and Hyprland dispatchers (Lua or classic syntax).
- Key faces with images (PNG/JPEG/SVG/WebP/…), app icons from your icon theme,
  site favicons, or Nerd Font glyphs; labels in your Omarchy font.
- Omarchy theme integration: key colours follow the active theme and repaint on
  `omarchy theme set`.
- First-run starter layout exercising every action type.
- CLI: `omadeckd list`, `omadeckd press N`, `omadeckd snapshot FILE.png`.
- Installer, udev rule, desktop entry, AUR packages (`omadeck`, `omadeck-bin`) and a project website.

[Unreleased]: https://github.com/thewizster/omadeck/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/thewizster/omadeck/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/thewizster/omadeck/releases/tag/v0.1.0
