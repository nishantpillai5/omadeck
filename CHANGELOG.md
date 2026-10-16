# Changelog

All notable changes to omadeck are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added
- Pages and folders. Extra pages live in `[[page]]` tables (top-level keys stay the Home page),
  with two new actions: `page` opens a page by name and `back` returns to the previous one.
  The configurator gets a page switcher (new, rename, delete), and "New page" on a key builds a
  folder with a Back key in place. The deck follows the page you're editing, and the app
  follows page changes made on the deck.
- `omadeckd press KEY [PAGE]` and `omadeckd snapshot FILE [PAGE]` take an optional page.
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
- Volume and mic bars on the strip now update optimistically: a volume action moves
  the bar immediately, and the daemon's read-back a moment later corrects it. A
  config reload no longer blanks the bars of devices that are still shown.
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

### Fixed
- Replugging a smaller deck no longer leaves the configurator editing a key or dial the
  hardware doesn't have.
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

[Unreleased]: https://github.com/thewizster/omadeck/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/thewizster/omadeck/releases/tag/v0.1.0
