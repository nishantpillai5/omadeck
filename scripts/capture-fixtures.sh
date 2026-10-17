#!/usr/bin/env bash
# Capture real tool output on the Omarchy machine into test fixtures.
#
#   scripts/capture-fixtures.sh            (interactive: prompts one step at a time)
#   CAPTURE_SECONDS=60 scripts/capture-fixtures.sh
#
# PRs 9-16 were built without a deck or Hyprland, so their parser tests use hand-written
# input. This records the real thing (the plans' "Capture on the machine first" items and
# plans/VERIFY.md) into dated folders:
#
#   crates/omadeck-core/tests/fixtures/captured/YYYY-MM-DD/   (core parsers)
#   crates/omadeckd/tests/fixtures/captured/YYYY-MM-DD/       (Hyprland socket, strip log)
#
# The tests in crates/omadeck-core/tests/captured.rs, crates/omadeckd/tests/captured_strip.rs
# and omadeckd's hypr.rs read the newest folder; see the README in the core folder.
#
# Read-only: it never changes the volume, media playback, Hyprland state or any config.
# The only thing it changes is the omadeckd service, and only for the strip step, after
# asking: it is stopped while a throwaway omadeckd (empty config, so a tap runs nothing)
# logs the strip, and started again afterwards if it was running.
#
# Each step whose tool is missing is skipped with a message. Interactive steps say what to
# do; press Enter to capture, or type s and Enter to skip. Nothing is redacted: check the
# files (window titles, paths, track names) before committing them.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT=$PWD
DAY=$(date +%Y-%m-%d)
CORE="$ROOT/crates/omadeck-core/tests/fixtures/captured/$DAY"
DAEMON="$ROOT/crates/omadeckd/tests/fixtures/captured/$DAY"
SECONDS_SOCKET2="${CAPTURE_SECONDS:-45}"
mkdir -p "$CORE" "$DAEMON"

captured=()
skipped=()

say() { printf '\033[1;36m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*"; }
have() { command -v "$1" >/dev/null 2>&1; }
rel() { printf '%s' "${1#"$ROOT"/}"; }
got() {
  captured+=("$(rel "$1")")
  say "saved $(rel "$1")"
}
skip() {
  skipped+=("$1")
  warn "skipped: $1"
}

INTERACTIVE=1
[[ -t 0 ]] || INTERACTIVE=0

# ask "what to do" — prints the instructions and waits. Returns 1 when skipped (or when
# not run from a terminal).
ask() {
  if ((!INTERACTIVE)); then
    return 1
  fi
  printf '\n\033[1;35m-->\033[0m %s\n' "$1"
  local ans
  read -r -p "    Enter when ready, s to skip: " ans || return 1
  [[ $ans != s && $ans != S ]]
}

# yesno "question" — y/n/s; prints the answer (yes, no, skipped).
yesno() {
  if ((!INTERACTIVE)); then
    echo skipped
    return
  fi
  local ans
  read -r -p "    $1 [y/n/s] " ans || ans=s
  case $ans in
    y | Y) echo yes ;;
    n | N) echo no ;;
    *) echo skipped ;;
  esac
}

# The Hyprland instance dir, or empty.
hypr_dir() {
  local run=${XDG_RUNTIME_DIR:-}
  [[ -n $run ]] || return 0
  if [[ -n ${HYPRLAND_INSTANCE_SIGNATURE:-} && -S $run/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock ]]; then
    printf '%s' "$run/hypr/$HYPRLAND_INSTANCE_SIGNATURE"
  fi
}
HYPR=$(hypr_dir)

# One request on Hyprland's request socket (what omadeckd's hypr.rs sends), read-only
# requests only. socat, else python3.
hypr_request() {
  local req=$1
  case $req in
    j/activewindow | j/activeworkspace | j/clients) ;;
    *)
      warn "refusing non-read request $req"
      return 1
      ;;
  esac
  if have socat; then
    printf '%s' "$req" | socat - "UNIX-CONNECT:$HYPR/.socket.sock"
  elif have python3; then
    python3 - "$HYPR/.socket.sock" "$req" <<'PY'
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.settimeout(2)
s.connect(sys.argv[1])
s.sendall(sys.argv[2].encode())
out = b""
while True:
    b = s.recv(65536)
    if not b:
        break
    out += b
sys.stdout.write(out.decode("utf-8", "replace"))
PY
  else
    return 1
  fi
}

# Read socket2 for N seconds into a file. socat, else python3.
socket2_record() {
  local secs=$1 out=$2
  if have socat; then
    timeout "$secs" socat -U - "UNIX-CONNECT:$HYPR/.socket2.sock" >"$out" || true
  elif have python3; then
    timeout "$secs" python3 - "$HYPR/.socket2.sock" >"$out" <<'PY' || true
import socket, sys
s = socket.socket(socket.AF_UNIX)
s.connect(sys.argv[1])
while True:
    b = s.recv(65536)
    if not b:
        break
    sys.stdout.write(b.decode("utf-8", "replace"))
    sys.stdout.flush()
PY
  else
    return 1
  fi
}

# Both forms of the focused window: hyprctl (core) and the request socket (daemon).
# $1: core file name, $2: daemon file name or empty.
window_json() {
  if have hyprctl; then
    hyprctl -j activewindow >"$CORE/$1"
    got "$CORE/$1"
  fi
  if [[ -n $2 && -n $HYPR ]]; then
    if hypr_request j/activewindow >"$DAEMON/$2"; then got "$DAEMON/$2"; else rm -f "$DAEMON/$2"; fi
  fi
}

say "Capturing into $(rel "$CORE") and $(rel "$DAEMON")"
((INTERACTIVE)) || warn "not a terminal: interactive steps are skipped"

# ---------------------------------------------------------------------------------------
# Versions and environment (PR 9 capture 3, PR 10 capture 6, PR 14 captures 1-2, PR 11)
# ---------------------------------------------------------------------------------------
{
  echo "# date: $(date -Is)"
  for t in hyprctl pactl pipewire wpctl playerctl playerctld omarchy-shell omarchy-version \
    omarchy-audio-output-volume swayosd-client socat walker omadeckd; do
    printf '%-30s %s\n' "$t" "$(command -v "$t" || echo missing)"
  done
  echo
  have hyprctl && hyprctl version | head -3
  have pactl && pactl --version
  have pipewire && pipewire --version
  have playerctl && echo "playerctl $(playerctl --version)"
  have playerctl && have pacman && pacman -Qo "$(command -v playerctl)"
  have omarchy-version && echo "omarchy $(omarchy-version)"
  head -3 /etc/os-release 2>/dev/null
} >"$CORE/versions.txt" 2>&1 || true
got "$CORE/versions.txt"

{
  echo "# systemctl --user show-environment (HYPRLAND, XDG_RUNTIME, OMARCHY_PATH, PATH)"
  if have systemctl; then
    systemctl --user show-environment 2>&1 | grep -E '^(HYPRLAND|XDG_RUNTIME|OMARCHY_PATH|PATH=)' || echo "(none of them)"
    echo
    echo "# systemctl --user show omadeckd -p Environment -p ActiveState"
    systemctl --user show omadeckd -p Environment -p ActiveState 2>&1 || true
  else
    echo "systemctl missing"
  fi
  echo
  echo "# this shell: HYPRLAND_INSTANCE_SIGNATURE=${HYPRLAND_INSTANCE_SIGNATURE:-<unset>} XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR:-<unset>}"
  echo "# ls -la \$XDG_RUNTIME_DIR/hypr/"
  ls -la "${XDG_RUNTIME_DIR:-/nonexistent}/hypr/" 2>&1 || true
  echo
  echo "# ls -l \$XDG_RUNTIME_DIR/omadeck.sock"
  ls -l "${XDG_RUNTIME_DIR:-/nonexistent}/omadeck.sock" 2>&1 || true
} >"$CORE/service-env.txt"
got "$CORE/service-env.txt"

{
  echo "# ls ~/.config/hypr/"
  ls "$HOME/.config/hypr/" 2>&1 || true
  echo
  echo "# head -30 ~/.config/hypr/bindings.conf"
  head -30 "$HOME/.config/hypr/bindings.conf" 2>&1 || true
  echo
  echo "# Omarchy's media binds (command names): grep -rn 'media' ~/.local/share/omarchy/default/hypr/bindings/"
  grep -rn 'media' "$HOME/.local/share/omarchy/default/hypr/bindings/" 2>&1 || true
} >"$CORE/hypr-bindings.txt"
got "$CORE/hypr-bindings.txt"

# ---------------------------------------------------------------------------------------
# /proc and battery (PR 9 captures 4-5)
# ---------------------------------------------------------------------------------------
if [[ -r /proc/stat ]]; then
  head -1 /proc/stat >"$CORE/proc-stat-1.txt"
  sleep 1
  head -1 /proc/stat >"$CORE/proc-stat-2.txt"
  grep -E '^(MemTotal|MemAvailable):' /proc/meminfo >"$CORE/meminfo.txt"
  got "$CORE/proc-stat-1.txt"
  got "$CORE/proc-stat-2.txt"
  got "$CORE/meminfo.txt"
else
  skip "/proc samples (no /proc/stat)"
fi

# Copies the attributes battery_in reads into $CORE/$1/<supply>/.
copy_power_supply() {
  local dest="$CORE/$1" d f
  rm -rf "$dest"
  for d in /sys/class/power_supply/*/; do
    [[ -d $d ]] || continue
    mkdir -p "$dest/$(basename "$d")"
    for f in type scope status capacity energy_now energy_full charge_now charge_full; do
      if [[ -r $d$f ]]; then cat "$d$f" >"$dest/$(basename "$d")/$f" 2>/dev/null || true; fi
    done
  done
  got "$dest"
}

if grep -qx Battery /sys/class/power_supply/*/type 2>/dev/null; then
  if ask "Battery: unplug the charger (running on battery)."; then
    copy_power_supply power_supply-battery
  else skip "power_supply-battery"; fi
  if ask "Battery: plug the charger in."; then
    copy_power_supply power_supply-ac
  else skip "power_supply-ac"; fi
else
  skip "battery sysfs (no type=Battery under /sys/class/power_supply)"
  if [[ -d /sys/class/power_supply ]]; then copy_power_supply power_supply-ac; fi
fi

# ---------------------------------------------------------------------------------------
# Glyphs (PR 9 capture 6, PR 10 capture 7, PR 14 capture 10): you look, it records
# ---------------------------------------------------------------------------------------
{
  echo "# fc-list ':charset=f056e' family"
  if have fc-list; then fc-list ":charset=f056e" family | head -3; else echo "fc-list missing"; fi
} >"$CORE/glyphs.txt"
if ((INTERACTIVE)); then
  printf '\n'
  # shellcheck disable=SC2059 # the glyph's code point has to be in the format
  for g in F0150 F061A F035B F0079 F0084 F056E F018D; do printf "  %s \U000$g\n" "$g"; done
  live=$(yesno "PR 9: clock, chip, memory stick, battery, charging battery, workspace, console?")
  printf '  F08C6 \U000F08C6\n'
  app=$(yesno "PR 10: an application window?")
  printf '  \U000f040a \U000f03e4 \U000f04ad \U000f04ae \U000f04db \U000f075a\n'
  media=$(yesno "PR 14: play, pause, next, previous, stop, music (none a box)?")
  printf 'live keys (f0150 f061a f035b f0079 f0084 f056e f018d): %s\napp volume (f08c6): %s\nmedia (f040a f03e4 f04ad f04ae f04db f075a): %s\n' \
    "$live" "$app" "$media" >>"$CORE/glyphs.txt"
fi
got "$CORE/glyphs.txt"

# ---------------------------------------------------------------------------------------
# Icons (PR 10 capture 5)
# ---------------------------------------------------------------------------------------
{
  echo "# hicolor apps matching spotify|firefox|chromium|mpv"
  icons=$(find /usr/share/icons/hicolor/*/apps \( -iname '*spotify*' -o -iname '*firefox*' \
    -o -iname '*chromium*' -o -iname '*mpv*' \) 2>/dev/null || true)
  echo "${icons:-(none)}"
  echo
  echo "# Icon= lines"
  grep -H '^Icon=' /usr/share/applications/{spotify,firefox,chromium,mpv}*.desktop \
    "$HOME"/.local/share/applications/*.desktop 2>/dev/null || echo "(none)"
} >"$CORE/icons.txt"
got "$CORE/icons.txt"

# ---------------------------------------------------------------------------------------
# Hyprland windows (PR 9 capture 2, PR 10 capture 4, PR 15)
# ---------------------------------------------------------------------------------------
if have hyprctl || [[ -n $HYPR ]]; then
  if ask "Focused window: focus a Firefox window (any normal window will do)."; then
    window_json activewindow.json j-activewindow.json
    if [[ -n $HYPR ]] && hypr_request j/activeworkspace >"$DAEMON/j-activeworkspace.json"; then
      got "$DAEMON/j-activeworkspace.json"
    else rm -f "$DAEMON/j-activeworkspace.json"; fi
  else skip "activewindow.json"; fi
  if ask "Nothing focused: switch to an EMPTY workspace (e.g. SUPER+9), then come back here and press Enter within 3 s (it waits 3 s)."; then
    sleep 3
    window_json activewindow-empty.json j-activewindow-empty.json
  else skip "activewindow-empty.json"; fi
  if have hyprctl; then
    if ask "Open windows: have Spotify, an Omarchy web app and a TUI app (e.g. btop) open."; then
      hyprctl -j clients >"$CORE/hyprctl-clients.json"
      got "$CORE/hyprctl-clients.json"
    else skip "hyprctl-clients.json"; fi
    if have omadeck; then
      if ask "Configurator: run 'omadeck', focus its window, then come back and press Enter within 3 s (it waits 3 s)."; then
        sleep 3
        hyprctl -j activewindow >"$CORE/activewindow-configurator.json"
        got "$CORE/activewindow-configurator.json"
      else skip "activewindow-configurator.json"; fi
    else
      skip "configurator class (omadeck not on PATH)"
    fi
  fi
else
  skip "Hyprland windows (no hyprctl and no Hyprland socket)"
fi

# ---------------------------------------------------------------------------------------
# App streams (PR 10 capture 1, 4): pactl JSON, plus the focused window and the process
# tree, so a test can check the focused window finds its streams.
# ---------------------------------------------------------------------------------------
# $1 case, $2 instructions, $3 1 when a window should be focused
stream_case() {
  local case=$1 what=$2 focus=$3
  if [[ $focus == 1 ]]; then
    what+=" Focus that app's window, then come back and press Enter within 3 s (it waits 3 s)."
  fi
  if ! ask "Streams ($case): $what"; then
    skip "sink-inputs-$case.json"
    return
  fi
  [[ $focus == 1 ]] && sleep 3
  if [[ $focus == 1 ]] && have hyprctl; then
    hyprctl -j activewindow >"$CORE/activewindow-$case.json"
    ps -e -o pid=,ppid= >"$CORE/proc-tree-$case.txt"
    got "$CORE/activewindow-$case.json"
    got "$CORE/proc-tree-$case.txt"
  fi
  pactl -f json list sink-inputs >"$CORE/sink-inputs-$case.json"
  got "$CORE/sink-inputs-$case.json"
  if [[ $focus == 1 ]] && have pstree && have hyprctl; then
    local wpid
    wpid=$(grep -o '"pid": *[0-9-]*' "$CORE/activewindow-$case.json" | grep -o -- '-\?[0-9]*$' || true)
    if [[ -n $wpid && $wpid -gt 0 ]]; then
      pstree -p "$wpid" >"$CORE/pstree-$case.txt" 2>&1 || true
      got "$CORE/pstree-$case.txt"
    fi
  fi
}

if have pactl; then
  stream_case spotify "Play a song in Spotify." 1
  stream_case spotify-paused "Pause Spotify (keep it open)." 0
  stream_case firefox-two-tabs "Play audio in two Firefox tabs." 1
  stream_case chromium "Play audio in Chromium (or an Omarchy web app)." 1
  stream_case mpv-in-terminal "Run mpv on an audio file in a terminal." 1
  if have flatpak && [[ -n $(flatpak list --app --columns=application 2>/dev/null) ]]; then
    stream_case flatpak "Play audio in a Flatpak app." 1
  else
    skip "sink-inputs-flatpak.json (no Flatpak apps)"
  fi
  stream_case none "Stop everything that plays audio." 0
else
  skip "app streams (pactl missing)"
fi

# ---------------------------------------------------------------------------------------
# Players (PR 14 captures 4-7)
# ---------------------------------------------------------------------------------------
playerctl_read() {
  LC_ALL=C playerctl -a metadata --format $'{{playerInstance}}\x1f{{playerName}}\x1f{{status}}\x1f{{position}}\x1f{{mpris:length}}\x1f{{xesam:title}}\x1f{{xesam:artist}}\x1f{{mpris:artUrl}}'
}
if have playerctl; then
  if ask "Players: Spotify PAUSED and a YouTube tab PLAYING in the browser."; then
    playerctl_read >"$CORE/playerctl-two.txt" 2>/dev/null || true
    got "$CORE/playerctl-two.txt"
    {
      echo "# pgrep -a playerctld"
      pgrep -a playerctld || echo "(not running)"
      echo "# playerctl -l"
      playerctl -l 2>&1 || true
    } >"$CORE/playerctl-list.txt"
    got "$CORE/playerctl-list.txt"
  else skip "playerctl-two.txt"; fi
  if ask "Players: play a live stream with no length, e.g. mpv https://ice1.somafm.com/groovesalad-128-mp3 (others may stay open)."; then
    playerctl_read >"$CORE/playerctl-stream.txt" 2>/dev/null || true
    got "$CORE/playerctl-stream.txt"
  else skip "playerctl-stream.txt"; fi
  if ask "No players: close every media player and browser tab with media."; then
    set +e
    playerctl_read >"$CORE/playerctl-none.txt" 2>"$CORE/playerctl-none.stderr.txt"
    echo "exit $?" >>"$CORE/playerctl-none.stderr.txt"
    set -e
    got "$CORE/playerctl-none.txt"
    got "$CORE/playerctl-none.stderr.txt"
  else skip "playerctl-none.txt"; fi
else
  skip "players (playerctl missing)"
fi

# ---------------------------------------------------------------------------------------
# socket2 transcript (PR 9 capture 1, PR 15 Walker)
# ---------------------------------------------------------------------------------------
if [[ -n $HYPR ]] && { have socat || have python3; }; then
  if ask "socket2: for ${SECONDS_SOCKET2} s after Enter, do (any order, skip what doesn't apply):
      SUPER+2, SUPER+1; click a window on another monitor; focus a terminal; focus Firefox;
      switch a Firefox tab (title change); go to an empty workspace; open Walker (SUPER+Space)
      and close it with Esc; optionally rename a workspace yourself and rename it back."; then
    say "recording for ${SECONDS_SOCKET2} s..."
    socket2_record "$SECONDS_SOCKET2" "$DAEMON/socket2.txt"
    got "$DAEMON/socket2.txt"
  else skip "socket2.txt"; fi
else
  skip "socket2 transcript (no Hyprland socket, or neither socat nor python3)"
fi

# ---------------------------------------------------------------------------------------
# Strip long press (PR 16): what the hardware sends for a tap, a hold and a drifting hold.
# A throwaway omadeckd with an empty config (so nothing runs) logs the strip events.
# ---------------------------------------------------------------------------------------
strip_capture() {
  local bin=$1 tmp log pid was_active=0 lines
  tmp=$(mktemp -d)
  log="$tmp/omadeckd.log"
  mkdir -p "$tmp/config" "$tmp/state"
  if have systemctl && systemctl --user is-active --quiet omadeckd; then
    was_active=1
    say "stopping the omadeckd service for this step"
    systemctl --user stop omadeckd
  fi
  # Every log line gets a wall-clock stamp, so a hold's delay can be read off.
  (XDG_CONFIG_HOME="$tmp/config" XDG_STATE_HOME="$tmp/state" RUST_LOG=debug exec "$bin") \
    > >(while IFS= read -r l; do printf '%s %s\n' "$EPOCHREALTIME" "$l"; done >"$log") 2>&1 &
  pid=$!
  sleep 3
  # $1 file, $2 instructions
  strip_step() {
    local file=$1 what=$2 start
    if ! ask "Strip: $what"; then
      skip "$(basename "$file")"
      return
    fi
    start=$EPOCHREALTIME
    lines=$(wc -l <"$log")
    sleep 4
    {
      echo "# enter at $start; $what"
      tail -n +"$((lines + 1))" "$log" | grep -E 'strip|TouchScreen|dial [0-9]' || echo "# (no strip lines)"
    } >"$file"
    got "$file"
  }
  strip_step "$DAEMON/strip-tap.log" "after Enter, TAP one strip panel once (within 4 s)."
  strip_step "$DAEMON/strip-hold.log" "after Enter, put a finger on one panel and HOLD it still for about 2 s, then lift (within 4 s)."
  strip_step "$DAEMON/strip-drift.log" "after Enter, hold one panel about 2 s while slowly drifting sideways, then lift (within 4 s)."
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
  sleep 0.2
  cp "$log" "$DAEMON/omadeckd-debug.log"
  got "$DAEMON/omadeckd-debug.log"
  rm -rf "$tmp"
  if ((was_active)); then
    say "starting the omadeckd service again"
    systemctl --user start omadeckd
  fi
}

omadeckd_bin=""
for c in "$ROOT/target/release/omadeckd" "$ROOT/target/debug/omadeckd" "$(command -v omadeckd || true)"; do
  if [[ -n $c && -x $c ]]; then
    omadeckd_bin=$c
    break
  fi
done
if [[ -z $omadeckd_bin ]]; then
  skip "strip long press (no omadeckd: cargo build -p omadeckd)"
elif ask "Strip long press: this stops the omadeckd service (if running), runs $omadeckd_bin with an empty config for three short steps, then starts the service again. The deck must be plugged in."; then
  strip_capture "$omadeckd_bin"
else
  skip "strip long press"
fi

# ---------------------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------------------
# Empty files from a skipped tool are noise.
find "$CORE" "$DAEMON" -type f -empty ! -name playerctl-none.txt -delete 2>/dev/null || true

echo
say "Captured ${#captured[@]}:"
for f in "${captured[@]}"; do printf '    %s\n' "$f"; done
if ((${#skipped[@]})); then
  warn "Skipped ${#skipped[@]}:"
  for f in "${skipped[@]}"; do printf '    %s\n' "$f"; done
fi
cat <<'EOF'

Not captured here (they change state, so do them by hand; see plans/VERIFY.md):
    wpctl set-volume / set-mute on a stream node (PR 10 capture 3)
    omarchy-shell media status / playPause (PR 14 capture 3)
    how fast a player reports a new status after play-pause (PR 14 capture 8)
    seeking past the end and before the start (PR 14 capture 9)
    hyprctl reload / renameworkspace during the socket2 recording (PR 9 capture 1)

Before committing: nothing was redacted. Check window titles, track names, paths and
the process list in the files above for anything private.

Next: commit the fixtures (git add crates/*/tests/fixtures/captured) and run
    cargo test -p omadeck-core -p omadeckd
The fixture tests print "fixture X not captured yet" for anything missing.
EOF
