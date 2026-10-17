# Captured fixtures

Real output from the Omarchy machine, recorded by `scripts/capture-fixtures.sh` into a
dated folder here (`YYYY-MM-DD/`) and in `crates/omadeckd/tests/fixtures/captured/`.
Each test reads the newest folder that has its file. A file that isn't there yet makes the
test print `fixture X not captured yet` and pass, so CI stays green until the captures
are committed. Nothing is redacted: check titles, track names and paths before committing.

## Core (`crates/omadeck-core/tests/fixtures/captured/<date>/`)

Tests are in `crates/omadeck-core/tests/captured.rs`.

| File | What | Test |
|---|---|---|
| `sink-inputs-<case>.json` | `pactl -f json list sink-inputs`; cases `spotify`, `spotify-paused`, `firefox-two-tabs`, `chromium`, `mpv-in-terminal`, `flatpak`, `none` | `pactl_fixtures_parse` |
| `activewindow-<case>.json`, `proc-tree-<case>.txt` | `hyprctl -j activewindow` and `ps -e -o pid=,ppid=` at the same moment, the app's window focused | `focused_window_finds_its_streams` |
| `pstree-<case>.txt` | `pstree -p <window pid>`, to read by hand | none |
| `activewindow.json`, `activewindow-empty.json` | `hyprctl -j activewindow`, a window focused / an empty workspace | `hyprctl_window_fixtures_parse` |
| `activewindow-configurator.json` | the same with `omadeck` focused | `configurator_class_is_the_constant` |
| `hyprctl-clients.json` | `hyprctl -j clients` (Spotify, a web app, a TUI app) | `parse_clients_on_capture` |
| `playerctl-two.txt` | `playerctl -a metadata --format …` (`player::FORMAT`), Spotify paused and a browser playing | `playerctl_two_players` |
| `playerctl-stream.txt` | the same with a live stream (no length) | `playerctl_stream_without_length` |
| `playerctl-none.txt`, `playerctl-none.stderr.txt` | the same with no players; stderr and exit status | `playerctl_no_players` |
| `playerctl-list.txt` | `pgrep -a playerctld`, `playerctl -l` | none (is `playerctld` listed?) |
| `proc-stat-1.txt`, `proc-stat-2.txt`, `meminfo.txt` | `/proc/stat` cpu line 1 s apart; MemTotal/MemAvailable | `proc_samples_parse` |
| `power_supply-battery/`, `power_supply-ac/` | the attributes `sysinfo::battery_in` reads, per supply, on battery / on the charger | `battery_layout_reads` |
| `versions.txt` | tool paths and versions (Hyprland, pactl, pipewire, playerctl, Omarchy) | none |
| `service-env.txt` | the user manager's environment (`HYPRLAND_*`, `XDG_RUNTIME_DIR`, `OMARCHY_PATH`, `PATH`), `omadeckd`'s, `$XDG_RUNTIME_DIR/hypr/`, the socket's mode | none |
| `hypr-bindings.txt` | `~/.config/hypr/` listing, `bindings.conf` head, Omarchy's media binds | none (README bind syntax, media command names) |
| `icons.txt` | hicolor icons and `Icon=` lines for Spotify, Firefox, Chromium, mpv | none |
| `glyphs.txt` | `fc-list` for the Nerd Font, and your yes/no on each glyph row | none |

## Daemon (`crates/omadeckd/tests/fixtures/captured/<date>/`)

| File | What | Test |
|---|---|---|
| `socket2.txt` | Hyprland's `.socket2.sock` for 45 s (`CAPTURE_SECONDS`) while you switch workspaces, focus windows, change a tab, open and close Walker | `hypr::tests::transcript_gives_events` |
| `j-activeworkspace.json`, `j-activewindow.json`, `j-activewindow-empty.json` | `j/activeworkspace` and `j/activewindow` on `.socket.sock`, as `hypr.rs` sends them | `hypr::tests::captured_request_json_parses` (also the answers in `transcript_gives_events`) |
| `strip-tap.log`, `strip-hold.log`, `strip-drift.log` | omadeckd's strip log lines (stamped with Unix time) for one tap, one still hold, one drifting hold | `tests/captured_strip.rs` |
| `omadeckd-debug.log` | the whole `RUST_LOG=debug` run behind the three strip logs | none |

## Not captured by the script

They change state, so do them by hand (plans/VERIFY.md): `wpctl set-volume`/`set-mute` on
a stream node, `omarchy-shell media …`, how fast a player reports a new status, seeking
past the end, and `hyprctl reload`/`renameworkspace` during the socket2 recording. The
glyphs are only checked by eye.
