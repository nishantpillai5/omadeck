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
- Stream Deck +: the daemon reads dial turns and presses and touch-strip taps and
  swipes, and reports them on its event socket.
- Dials on the Stream Deck + can run any action on turn left, turn right and press,
  configured with `[[dial]]` tables or in the configurator.

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
