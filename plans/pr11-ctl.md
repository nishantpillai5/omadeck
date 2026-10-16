# PR 11: `feat/ctl`: drive the running daemon from the shell

Part of [PLAN.md](PLAN.md). Base: `feat/app-volume` (PR 10). It owns the **daemon
control socket** building block. It needs nothing from PRs 7–10 beyond the code they
leave in `omadeckd/src/main.rs`. The one assumption is that PR 8 sends
`WatchCmd::Refresh` from the key-press path (see "Assumptions" below).

**Before handing this to the local model** (from the 2026-10-16 review; do these when
rewriting the plan after the previous feature lands, then delete this list):

- [ ] Rewrite against the code as merged (upstream's named pages: `Option<&str>` / `Config::key(page, index)` / `Nav`), not `ba0eced`'s `page: u8`.
- [ ] Replace every "assumption about PR N" with that PR's real, merged signatures, copied in full.
- [ ] Settle every Open decision and write it as "Decided: …".
- [ ] Split into tasks of ~150–300 lines, each ending with an exact `cargo test -p … <filter>` and what it should show.
- [ ] Refer to symbols (function names, grep anchors), not line numbers.
- [ ] After the rebase, upstream already has the client→daemon half: `ipc::Command` (tagged `"command"`), per-client reader threads, `Hub::commands()`, `ipc::connect()`. Rewrite §IPC and §Hub as new variants plus a reply path on that, not a second `Command` enum tagged `"cmd"`.
- [ ] Pages are names: redo the CLI (`omadeckd page next|prev|<name>|home`), `State { page: Option<String>, … }`. Upstream already has `press KEY [PAGE]`, so drop Later's `--page N` and BUGS.md 2.
- [ ] Parameter lists like `press_key(d, cfg, renderer, hub, audio, i)` use `AudioWorker`, which PR 8 removes. Introduce one `Ctx` struct, or list the real arguments.
- [ ] Step 1 (pulling three handlers out of the run loop) has no test harness. Name the exact blocks by their `DeviceStateUpdate` arm and the new functions' boundaries; the check is `cargo test` plus four manual presses. It routes through rebase step 2's dispatch.
- [ ] Use the rebase's dispatch (`is_navigation()` + Brightness), not PR 12's `is_deck_owned()`, which comes later.
- [ ] Import the enum as `ipc::Command`: `std::process::Command` is used in the same files.
- [ ] Key flash: today `flash_until` stores now+FLASH but expiry checks `duration_since(t) >= DIAL_FLASH`, so it lasts ~2× (BUGS.md 15). Say whether to fix it first.

**Outcome:** the daemon accepts commands on the socket it already publishes events on.
`omadeckd page next`, `omadeckd page 2`, `omadeckd brightness 30`, `omadeckd brightness
+10`, `omadeckd reload` and `omadeckd state` drive the running daemon. `omadeckd press 3`
and `omadeckd dial 1 left` go through the daemon when it runs, so they work exactly like a
physical press: the face flashes, page and brightness keys work, toggle and live state
refresh, and volume bars move. When no daemon is running they fall back to running the
action locally, as they do today. In the configurator, *Try it now* works for page and
brightness actions. Hyprland binds can page the deck or dim it from the keyboard.

## How the Elgato app does it (researched 2026-10-16)

- **No first-party shell control of the deck.** The `streamdeck` CLI (`@elgato/cli`,
  alias `sd`) is for plugin development only: `create`, `link`, `unlink`, `restart`,
  `stop`, `dev`, `validate`, `pack`, `config`, `list`. It cannot press a key, switch a
  page or set the brightness.
  <https://docs.elgato.com/streamdeck/cli>, <https://github.com/elgatosf/cli> (archived;
  moved into the SDK repo).
- **Deep links** (Stream Deck 6.5+): `streamdeck://plugins/message/<PLUGIN_UUID>/path?q#f`
  delivers a message to *one plugin* (`streamDeck.system.onDidReceiveDeepLink`). Since 7.0,
  `?streamdeck=hidden` keeps the app in the background. It does not switch profiles or
  pages and does not press keys; a plugin has to implement that itself.
  <https://docs.elgato.com/streamdeck/sdk/guides/deep-linking>
- Page or profile changes from outside the app come from plugins (`switchToProfile`)
  or third-party plugins and tools. Unconfirmed: whether the app has any global hotkey or
  URL that changes page or brightness. I found none in the docs.
- So there is nothing for the user to recognise here. This is a Linux and Omarchy
  feature. omadeck's existing CLI (`press`, `dial`) is the reference for naming and
  numbering, not Elgato.

## Command line

No `ctl` prefix and no new binary: `press` and `dial` are already "control" subcommands
of `omadeckd`, so the new verbs sit next to them. `omadeck` is the GTK app; a CLI there
would start GTK.

```text
omadeckd page next|prev|N        go to the next / previous page, or page N (0-based)
omadeckd brightness N|+N|-N      set the brightness to N%, or change it by ±N
omadeckd press KEY               press a key on the current page (keys count from 0)
omadeckd dial N left|right|press [STEPS]
omadeckd reload                  re-read the config and theme now; print the error if it is broken
omadeckd state                   print the daemon's state as one JSON line
```

- Numbers are **0-based**, like `press`, `dial` and the config's `page`/`index` fields
  (open decision below).
- Output: quiet on success except for a one-line message the daemon may send (e.g.
  `key 3: Launch firefox`, `page 2 of 3`, `brightness 40%`). Errors go to stderr with
  exit 1. Usage errors exit 2, as today.
- **Daemon not running:**
  - `press` and `dial` run the action locally, as today. That is also what they do when
    the daemon is up but no deck is connected: the daemon replies `NoDeck` and the CLI
    falls back.
  - `page`, `brightness`, `reload` and `state` fail with
    `omadeckd is not running (systemctl --user start omadeckd)` and exit 1.
- Parsing: a pure `parse_ctl(args: &[String]) -> Result<Command, String>` in `main.rs`
  so it can be unit tested. `brightness`: a leading `+` or `-` means change, otherwise
  set. `page`: `next`/`prev`/`previous` or a number.
- `state` prints a single JSON line for `jq`, e.g.
  `{"connected":{"model":"Stream Deck +",...},"page":1,"pages":3,"brightness":40}`.

### Hyprland / Omarchy binds (for the README)

Omarchy keeps user binds in `~/.config/hypr/bindings.conf` and uses `bindd` (bind with
a description). Plain `bind` also works:

```conf
bindd = SUPER, F1, Next deck page, exec, omadeckd page next
bindd = SUPER SHIFT, F1, Previous deck page, exec, omadeckd page prev
bindd = SUPER, F2, Deck page 1, exec, omadeckd page 0
binded = SUPER, F3, Dim deck, exec, omadeckd brightness -10     # repeats while held
binded = SUPER, F4, Brighten deck, exec, omadeckd brightness +10
bindd = SUPER, F5, Deck key 3, exec, omadeckd press 3
```

## IPC (`omadeck-core/src/ipc.rs`)

The **same socket** (`$XDG_RUNTIME_DIR/omadeck.sock`) carries both directions:
line-delimited JSON, events tagged `"event"`, commands tagged `"cmd"`, replies tagged
`"reply"`.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum Command {
    /// Press key `index` on the deck's current page, as if pressed by hand.
    Press { index: u8 },
    /// Turn (`steps` detents) or press dial `index`.
    Dial { index: u8, input: DialInput, #[serde(default = "one")] steps: u8 },
    /// Run an action on the daemon: page and brightness actions included.
    Run { action: Action },
    /// Re-read config and theme now.
    Reload,
    /// Report the current state.
    State,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialInput { Left, Right, Press }

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Ok { #[serde(default, skip_serializing_if = "String::is_empty")] message: String },
    Error { message: String },
    /// No deck is attached; `press`/`dial` callers fall back to running locally.
    NoDeck,
    State { connected: Option<DeckInfo>, page: u8, pages: u8, brightness: u8 },
}
```

- `Command::to_line()` and `Reply::to_line()` mirror `Event::to_line()`.
- `omadeckd page next` sends `Run { action: Action::Page { index: None, change: 1 } }`,
  and `brightness +10` sends `Run { action: Action::Brightness { change: 10, set: None } }`.
  There are no page-specific or brightness-specific commands: the daemon already knows
  how to run these actions (`do_page`, `do_brightness`), and the GUI's *Try it now* can
  send the action it is editing, including unsaved edits.
- Client side: `pub fn request(cmd: &Command) -> Result<Option<Reply>>`.
  - It connects. `NotFound` or `ConnectionRefused` returns `Ok(None)`, meaning no daemon.
  - It sets a 3 s read timeout, writes one line, then reads lines. It **skips any line
    that is not a reply**: the hub sends every new client the connected/disconnected
    status line and then events. It returns the first `Reply`.
  - On a timeout it returns `Err("omadeckd did not answer (is it older than this
    omadeckd?)")`.
- Wire shapes are pinned in tests, like `dial_turn_wire_shape`:
  `{"cmd":"run","action":{"type":"page","change":1}}` → `{"reply":"ok","message":"page 2 of 3"}`.

## Hub (`omadeckd/src/hub.rs`)

Today the accept loop writes the status line and pushes the stream into `clients`, and
nothing ever reads from it. New:

- `Hub::start(path) -> Result<(Hub, mpsc::Receiver<Request>)>` with
  `pub struct Request { pub cmd: Command, pub reply: mpsc::Sender<Reply> }`.
- After `bind`, `set_permissions(&path, 0o600)`. The runtime dir is already 0700, so
  this is defence in depth (see Security).
- For each accepted stream, after pushing it into `clients`, spawn a thread named
  `ipc-client`. It owns `stream.try_clone()` and reads lines with a 4 KiB cap per line.
  For each line:
  - It parses a `Command`. Garbage or an unknown `cmd` gets `Reply::Error` ("unknown
    command"), which keeps newer clients working against older daemons.
  - It sends a `Request` to the main loop and waits with `recv_timeout(2 s)`. A timeout
    becomes `Error("daemon busy")`.
  - It writes the reply **while holding `inner`'s lock**, so a reply line can never be
    interleaved with an event line that `publish` is writing to the same stream.
  - The thread ends at EOF. A GUI that never writes leaves one parked thread per window,
    which is harmless.
- A ctl client is also in `clients` and gets events until it disconnects. `publish`
  drops it on the next failed write, as today. A ctl client lives about a millisecond,
  so this costs nothing and keeps the hub a single list.

## Daemon (`omadeckd/src/main.rs`)

- **Draining.** `run()` keeps `commands: Receiver<Request>`. Each loop iteration, right
  after the reload block and **before** the `let Some(d) = &mut deck else` branch (so
  commands are also answered with no deck), it runs
  `while let Ok(req) = commands.try_recv() { let r = handle_command(...); let _ = req.reply.send(r); }`.
- **Latency.** At worst one `READ_TIMEOUT` (100 ms) plus the work; about 50 ms on
  average. The no-deck branch sleeps `READ_TIMEOUT` too. That is fine for a keybind and
  for `binde` repeat (Hyprland's default repeat is 25/s; commands queue and all run).
  Keep 100 ms. Lowering it costs idle wakeups on every user's machine for a few
  milliseconds on a shortcut.
- **Refactor first, no behaviour change.** Pull the bodies of the `ButtonDown`,
  `EncoderTwist` and `EncoderDown` arms into `press_key(d, cfg, renderer, hub, audio, i)`,
  `turn_dial(…, i, delta)` and `press_dial(…, i)`. Hardware events and commands share one
  path, including PR 8–10's watcher refreshes. Pull the inline reload block into
  `reload(&mut cfg, &mut renderer, deck.as_mut(), &audio) -> Result<()>`. It returns the
  `Config::load` error (still keeping the last good config), so `omadeckd reload` can
  print `config.toml: expected '=' at line 12`.
- `handle_command(cmd, deck: Option<&mut Deck>, cfg, renderer, hub, audio) -> Reply`:

| Command | No deck | With a deck |
|---|---|---|
| `Press { index }` | `NoDeck` | `index >= key_count` → `Error`; else paint the face pressed, publish `KeyDown`, `press_key`, set `key_flash_until[index] = now + KEY_FLASH`; `Ok("key 3: <summary>")` |
| `Dial { index, input, steps }` | `NoDeck` | `index >= dials` → `Error`; Left/Right → `turn_dial(i, ∓/±steps)` (capped at 8 as today); Press → flash the panel like a strip tap (`flash_until`) + `press_dial` |
| `Run { action }` | `NoDeck` for `Page`/`Brightness`; otherwise `action::run` | `Page` → `do_page`; `Brightness` → `do_brightness`; else `action::run` + `after_volume` |
| `Reload` | `reload()` → `Ok` / `Error(msg)` | same |
| `State` | `State { connected: None, page: 0, pages, brightness: cfg.brightness }` | from `Deck` |

- **Key flash.** New `Deck.key_flash_until: Vec<Option<Instant>>` and
  `KEY_FLASH = 150 ms`, matching `DIAL_FLASH`. They expire in the same spot as dial
  flashes: repaint unpressed and publish `KeyUp`. A hardware `ButtonUp` clears the flash.
  The GUI sees `KeyDown`/`KeyUp` exactly as for a real press, so it highlights and
  selects the key.
- **Errors for scripts.** `do_page` returns the new page. A page index out of range is
  an `Error` ("no page 5; the config has 3"). A no-op (one page, or already there) is
  `Ok` with a message. `action::run` failures become `Error`, and `ActionFailed` is still
  published.
- **Brightness.** `do_brightness` already persists to the config. The GUI picks it up
  through its config watcher (PR 6), so no new event is needed.
- **Disconnect and reconnect** need nothing special: commands read `deck` fresh on every
  iteration.

### Assumptions on other PRs

- PR 8 adds the toggle-state refresh (`WatchCmd::Refresh(probe)`) inside the
  `ButtonDown` handling, and PR 10 adds the app-volume dial in the `EncoderTwist`
  handling. The refactor moves whatever is there into `press_key` and `turn_dial`
  unchanged. If those PRs already extracted helpers, reuse them.
- PR 13 (folders) will turn `Deck.page` into a stack. `State.page` then stays "the page
  shown" and `page N` stays a jump, which matches Elgato's Page action leaving a folder.

## Security

- The socket lives in `$XDG_RUNTIME_DIR` (logind makes it 0700 and owned by the user).
  `paths::socket_path` deliberately has no `/tmp` fallback. Today the socket file itself
  gets the umask's mode (usually `srwxr-xr-x`). Connecting needs write permission on the
  socket, so other users are already locked out twice over. The new `chmod 0600` makes
  that explicit, since the socket now *accepts* commands.
- `Run` can run any action, including `script` and `app`. Anyone who can connect is the
  same user and can already run those from a shell, so this crosses no privilege
  boundary. No `SO_PEERCRED` check: `UnixStream::peer_cred` is still unstable in std, and
  the directory permission is the real guard.
- Requests are size-capped (4 KiB per line) and answered one at a time on the deck loop,
  so a misbehaving client can only slow its own connection.

## Backwards compatibility

- Event lines are unchanged. Replies only go to the connection that asked, so old
  configurators never see them, and `subscribe()` drops unparseable lines anyway.
- The configurator never writes, so its reader thread just parks until EOF.
- A new CLI against an old daemon times out after 3 s with a clear message. Only happens
  mid-upgrade, before `systemctl --user restart omadeckd`.
- `press` and `dial` keep their arguments and their old behaviour when the daemon is not
  up. No config changes, so old configs load and save byte-identically.

## Config, actions, render

None. No new `Action` variant, no `KINDS` change, no render change.

## GUI (`omadeck-gui`)

- `editor.rs` key editor: today `deck_owned` actions get an insensitive *Try it now*
  ("This action works on the deck"). They now **try through the daemon**. The button is
  sensitive when `app.daemon_up` is set and the action is not `None`, with the tooltip
  "Try it on the deck". Clicking sends `Command::Run { action }` with the action being
  edited, from `gio::spawn_blocking` so the GTK thread never waits on the socket. An
  `Error` or `NoDeck` reply shows as a toast ("No deck connected").
- `dial_action_group`: same treatment. Today a page or brightness dial action's *Try*
  just toasts the `action::run` bail.
- Other actions keep running locally from *Try it now*, as today. That works without a
  daemon and doesn't flash a key the user isn't editing.
- The settings brightness slider stays as it is: it writes the config and the daemon
  applies it on reload. Moving it to `Run` would skip the debounce for no gain.
- A shared helper `App::try_on_deck(action)` keeps the two editors from duplicating code.

## Docs

- README "Command line": the new verbs, a short "Hyprland binds" block (the
  `bindings.conf` example above), and a line saying `press`/`dial` go through the daemon
  when it runs.
- README "Event feed" paragraph becomes "Event feed and control": commands and replies
  on the same socket, and the `state` JSON.
- `site/index.html` mentions `omadeckd press 3`; add `omadeckd page next`.
- No roadmap checkbox (not on upstream's roadmap). CHANGELOG under Unreleased/Added:
  "`omadeckd page`, `brightness`, `reload` and `state` control the running daemon, e.g.
  from Hyprland binds; `press` and `dial` go through the daemon when it runs, so the key
  flashes and page/brightness keys work. *Try it now* works for page and brightness
  actions."

## Implementation steps

1. [ ] Refactor `main.rs`: extract `press_key`, `turn_dial`, `press_dial` and `reload`.
       No behaviour change; build, run, commit.
2. [ ] `ipc.rs`: `Command`, `DialInput`, `Reply`, `to_line`, `request()`; tests for
       roundtrips and wire shapes.
3. [ ] `hub.rs`: `chmod 0600`, per-client reader thread, `Request` channel, locked reply
       write.
4. [ ] `main.rs`: drain `commands` each iteration; `handle_command`; `key_flash_until`
       and `KEY_FLASH`; make `do_page` return a result for the reply.
5. [ ] CLI: `parse_ctl` and the new verbs; `press`/`dial` try `request()` first and fall
       back on `None`/`NoDeck`; usage text.
6. [ ] GUI: `App::try_on_deck`; enable *Try it now* for deck-owned key and dial actions.
7. [ ] Docs: README, site, CHANGELOG. Then `cargo fmt && cargo clippy --all-targets -- -D
       warnings && cargo test`.

## Tests

- `ipc::tests::command_roundtrip`: every `Command` variant, including `Dial` with and
  without `steps`.
- `ipc::tests::command_wire_shape`: `Run { Page next }` and `Press { index: 3 }` lines
  are pinned.
- `ipc::tests::reply_roundtrip`: `Ok` with and without message (no `message` key when
  empty), `Error`, `NoDeck`, `State` with `connected: None`.
- `ipc::tests::reply_skips_events`: the reply reader skips a leading
  `{"event":"connected",…}` line and returns the reply (test the line-picking helper on
  an in-memory `BufRead`).
- `ipc::tests::unknown_command_is_error`: `{"cmd":"explode"}` fails to parse; the hub's
  mapping to `Reply::Error` is covered by the helper `parse_request(line) -> Result<Command, Reply>`.
- `main::tests::parse_ctl_page`: `next`, `prev`, `previous`, `0`, `9`, `x` (error), and
  no argument (error).
- `main::tests::parse_ctl_brightness`: `30` → set, `+10` → change 10, `-10` → change −10,
  `101` → set 100 (clamped by `apply_brightness`; parser accepts 0–255 as u8),
  `abc` → error.
- `main::tests::parse_ctl_dial`: `1 left`, `1 right 4`, `1 press`, `1 up` (error).
- The hub itself: `hub::tests::command_reply_over_socket` binds a socket in a temp dir,
  connects, sends `State`, answers from a test thread playing the main loop, and checks
  the client gets the status line followed by the reply. It also checks the socket mode
  is `0o600`.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] `omadeckd page next` / `prev` / `page 1` switch pages on the deck; the configurator's
      chips follow; `page 7` on a 3-page config prints an error and exits 1.
- [ ] `omadeckd brightness 20`, `+10`, `-10`: the deck dims live, a brightness dial's bar
      follows, and the value survives a service restart.
- [ ] Held `binded` brightness key: smooth, no backlog after release.
- [ ] `omadeckd press 3` with the daemon up: key 3's face flashes, the action runs, and the
      configurator highlights the key. A page key pressed this way switches page.
- [ ] `omadeckd press 3` with the service stopped: runs locally as before.
- [ ] `omadeckd dial 0 right 5` moves the volume bar five steps; `dial 0 press` flashes the
      panel and mutes.
- [ ] `omadeckd reload` after a deliberate typo prints the parse error; the deck keeps
      the last good config.
- [ ] `omadeckd state | jq .page` with the deck unplugged and plugged.
- [ ] Configurator: *Try it now* on a page key and a brightness dial acts on the deck;
      with the deck unplugged it toasts "No deck connected".
- [ ] `ls -l $XDG_RUNTIME_DIR/omadeck.sock` shows `srw-------`.
- [ ] An old configurator build still shows status and key presses against the new daemon.

## Decisions to explain

- **No Elgato equivalent.** Elgato's CLI is for plugin development and its deep links go
  to plugins, so this follows omadeck's own CLI instead.
- **Flat subcommands, not `omadeckd ctl …` or a new binary.** `press` and `dial` already
  live at the top level, and a `ctl` prefix would leave two ways to press a key.
  `omadeck` is the GTK app.
- **One socket, not a second `omadeck-ctl.sock`.** One path and one listener. The hub
  learns to read, events and replies are told apart by their tag, and old clients are
  unaffected. The cost is that a ctl client briefly receives events, which it skips.
- **`Run { action }` instead of `Page`/`Brightness` commands.** It reuses the `Action`
  shapes users already write in TOML, lets *Try it now* send unsaved edits, and new
  daemon-owned actions (folders, PR 13) work through it for free.
- **`press`/`dial` go through the daemon when it's up.** That is the only way page and
  brightness keys can work from a bind, and it makes a shared key/bind action look the
  same on the deck. The local fallback keeps the old behaviour without a daemon, which is
  how people script it today.
- **0-based numbers**, consistent with `press`, `dial` and the config. The GUI is 1-based
  because it shows names ("Page 1"), not indices.
- **Commands wait for the deck loop (≤100 ms)** rather than having their own thread touch
  the deck. The deck stays single-threaded, as the daemon's design requires.
- **No uid check on the socket.** The 0700 runtime dir plus a 0600 socket is the guard,
  and `peer_cred` is unstable in std.

## Later

- `press KEY --page N` (press a key on a page other than the current one).
- `omadeckd watch`: print events as JSON lines (the GUI's feed) for scripts and waybar.
- A `BrightnessChanged` event, if anything besides the GUI's config watcher needs it.
- A waybar module from `state` (page and brightness).
- `omadeckd set-key …`-style editing commands. Out of scope; the config file is the
  interface for edits.

**Size:** about 450 lines. Refactor ~80, ipc ~120 with tests, hub ~70, daemon commands
~100, CLI ~80, GUI ~40, docs ~40.

## Open decisions (settle when building)

The plan picks a default for each; confirm or change it before starting this PR.

- 0-based `page 2` (third page) or 1-based to match the GUI and the `Go to page N`
  summary? Planned 0-based for consistency with `press`/`dial`.
- Should a ctl `press` publish `KeyDown`, which makes an open configurator jump its
  selection to that key, or a quieter variant?
