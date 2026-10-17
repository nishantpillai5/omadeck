# PR 11: `feat/ctl`: drive the running daemon from the shell

Part of [PLAN.md](PLAN.md). Base: `feat/app-volume` (PR 10), which sits on PR 9, PR 8,
`fix/review-2` and `feat/groundwork`. It owns the **daemon control socket** building block:
the GUI → daemon half is already there (`ipc::Command`, tagged `"command"`, read by an
`ipc-client` thread per client into `Hub::commands()`). This PR adds variants, a reply to
each new one, and the CLI verbs. There is no second socket and no second `Command` enum.

**Outcome:** `omadeckd page next`, `omadeckd page Media`, `omadeckd brightness 30`,
`omadeckd brightness +10`, `omadeckd reload` and `omadeckd state` drive the running
daemon. `omadeckd press 3` and `omadeckd dial 1 left` go through the daemon when it runs,
so they work like a physical press: the face flashes, page and brightness keys work, toggle
and live state refresh, and volume bars move. With no daemon (or no deck) they run the
action locally, as today. In the configurator, *Try it now* goes through the daemon when it
runs, so Go back, Next/previous page and Brightness work there too, and a Hyprland action
no longer waits on the GTK thread. A key's own ▶ presses the key, as the deck would, toggle
flip included. Hyprland binds can page or dim the deck.

## How the Elgato app does it (researched 2026-10-16)

- **No first-party shell control of the deck.** The `streamdeck` CLI (`@elgato/cli`) is for
  plugin development only (`create`, `link`, `restart`, `validate`, `pack`, …). It can't
  press a key, switch a page or set the brightness. <https://docs.elgato.com/streamdeck/cli>
- **Deep links** (6.5+), `streamdeck://plugins/message/<PLUGIN_UUID>/…`, deliver a message
  to one plugin. They don't switch pages or press keys.
  <https://docs.elgato.com/streamdeck/sdk/guides/deep-linking>
- Unconfirmed: whether the app has any global hotkey or URL for page or brightness. None
  found in the docs.
- So there is nothing for the user to recognise. This is a Linux feature, and omadeck's own
  CLI (`press`, `dial`) is the reference for naming.

## What this plan uses from the base

Copied from the code at `55d0d62` (`feat/toggle-keys`), plus what `fix/review-2`, PR 9 and
PR 10 change (each marked). **Before task 1, grep each name below on `feat/app-volume`.**
If one differs, use the merged name and fix this list.

```bash
grep -n "pub enum Command\|pub fn commands\|fn line\|pub struct Sender\|pub fn connect" crates/omadeck-core/src/ipc.rs
grep -n "CLIENT_QUEUE\|pub struct Hub\|struct Inner\|ipc-client\|fn spawn_writer\|pub fn commands" crates/omadeckd/src/hub.rs
grep -n "pub enum Source\|pub fn run_input\|fn dispatch\|fn report_failure\|pub fn poll_hypr\|pub hypr\|hypr_events\|fn press_key\|fn handle_update\|pub fn handle_commands\|pub fn reload\|pub fn next_deadline\|fn key_states" crates/omadeckd/src/daemon.rs
grep -n "pub struct HyprJob\|pub struct HyprFailed\|pub fn send" crates/omadeckd/src/hypr_worker.rs
grep -n "daemon\.[a-z_]*()" crates/omadeckd/src/main.rs
grep -n "pub struct Deck\|flash_until\|fn flash_deadline\|fn expire_flashes\|DIAL_FLASH" crates/omadeckd/src/deck.rs
grep -n "fn test_button\|type ActionRun" crates/omadeck-gui/src/editor.rs
```

```rust
// omadeck-core/src/ipc.rs (unchanged since upstream)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Show a page on the deck (`None` is home), e.g. the one being edited.
    ShowPage { page: Option<String> },
    /// Set the panel brightness, e.g. from the configurator's settings.
    SetBrightness { value: u8 },
}
fn line<T: Serialize>(v: &T) -> String;                 // JSON + '\n'; Event::to_line uses it
pub struct Sender(UnixStream);                          // send(&mut self, cmd: &Command) -> Result<()>
pub fn connect() -> Result<(Sender, impl Iterator<Item = Event>)>;   // skips lines that aren't events
pub fn commands(stream: UnixStream) -> impl Iterator<Item = Command>; // skips garbage silently
// Event::ActionFailed { index: u8, page: Option<String>, message: String },
// Event::DialActionFailed { index: u8, message: String }, Event::KeyDown { index, page }, Event::KeyUp { index }

// omadeck-core/src/device.rs
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckInfo { pub model: String, pub serial: String, pub rows: u8, pub cols: u8,
                      pub key_size: u32, pub dials: u8, pub strip: Option<(u32, u32)> }
impl DeckInfo { pub fn key_count(&self) -> u8; }

// omadeck-core/src/config.rs
pub const HOME: &str = "Home";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction { Left, Right, Press }               // fix/review-2 adds Direction::ALL
impl Config { pub fn has_page(&self, name: &str) -> bool; pub fn page_names(&self) -> Vec<String>;
              pub fn page(&self, page: Option<&str>) -> Option<&Keys>;
              pub fn key(&self, page: Option<&str>, index: u8) -> Option<&KeyConfig>;
              pub fn dial(&self, index: u8) -> Option<&DialConfig>; }
impl DialConfig { pub fn action(&self, dir: Direction) -> &Action; }
impl KeyConfig { pub fn action_for(&self, on: bool) -> &Action; }
impl Action { pub fn is_deck_owned(&self) -> bool;     // Page | Back | PageStep | Brightness
              pub fn summary(&self) -> String; pub fn is_none(&self) -> bool; }
// PR 10: Action::AppVolume { app: String, focused: bool, change: i8, mute: bool }, KINDS[11] = "App volume".

// omadeck-core/src/action.rs
pub fn run(action: &Action) -> Result<()>;              // bails "<kind> only works on the deck" for deck-owned
pub fn run_steps(action: &Action, steps: u8) -> Result<()>;

// omadeck-core/src/paths.rs
pub fn socket_path() -> anyhow::Result<PathBuf>;        // $XDG_RUNTIME_DIR/omadeck.sock, error if unset

// omadeckd/src/daemon.rs
pub const READ_TIMEOUT: Duration = Duration::from_millis(100);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyId { pub page: Option<String>, pub index: u8 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick }
// Display: "key {i}", "dial {i}" (Turn/Press/Tap), "strip flick". PR 9 and PR 10 add none.
pub fn key_state(cfg: &Config, rt: &Runtime, page: Option<&str>, i: u8, pressed: bool) -> KeyStatus;
pub struct Daemon { pub cfg: Config, pub renderer: Renderer, pub nav: Nav, pub wanted_page: Option<String>,
                    pub hub: Hub, pub watcher: Watcher, pub saver: StateSaver,
                    pub hypr: HyprWorker,          // fix/review-2 (R6): the ordered Hyprland dispatch worker
                    pub rt: Runtime, pub deck: Option<Deck>, /* private: */ published, published_keys, in_flight,
                    pub reload_at: Option<Instant>, pub repaint_at: Option<Instant>,
                    hypr_events: Option<mpsc::Receiver<hypr::HyprEvent>> }  // PR 9: the Hyprland event listener
// Fields PR 10 adds aren't used by this PR.
impl Daemon {
    pub fn new(cfg: Config, cfg_loaded: bool, hub: Hub, watcher: Watcher, saver: StateSaver) -> Self; // fix/review-2 (R3)
    pub fn reload(&mut self);                  // keeps the last good config: log::error!("{e:#}; keeping the previous config")
    pub fn handle_commands(&mut self);         // `for cmd in self.hub.commands()`: last ShowPage, last SetBrightness
    pub fn next_deadline(&self) -> Option<Instant>; // [flash, repaint_at, reload_at].min()
    pub fn pass(&mut self) -> Result<(), StreamDeckError>; // repaint_at, expire_flashes, read + MAX_DRAIN, flush
    fn handle_update(&mut self, u: DeviceStateUpdate);     // ButtonDown/ButtonUp/EncoderTwist/EncoderDown/… inline
    fn press_key(&mut self, i: u8);            // job running → ignored; busy_while_running → run_tracked; else run_input(Source::Key(i)) then after_toggle_press
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool; // log, dispatch, show_page if moved, report_failure
    fn dispatch(&mut self, src: Source, act: &Action, steps: u8) -> Result<bool>; // Ok(page moved); fix/review-2 (R6) added `src`
    fn report_failure(&self, src: Source, page: Option<String>, message: String); // fix/review-2 (R6): logs, publishes for `src`
    pub fn poll_hypr(&mut self);               // fix/review-2 (R6): report_failure for each HyprFailed from the worker
    pub fn set_brightness(&mut self, set: Option<u8>, change: i32);           // works with no deck
    fn show_page(&mut self);                   // paint_all, watch(), publish PageChanged
    fn key_states(&self, n: u8) -> Vec<KeyStatus>; // key_state(.., false) for each key
    fn paint_key(&mut self, i: u8, pressed: bool);
    pub fn flush(&mut self);
}
pub fn wait_for(now: Instant, deadline: Option<Instant>) -> Duration;
```

`run_input`'s failure path after fix/review-2 (R6), which this PR moves into `try_input`.
The `Err` arm of `run_input` is
`let page = self.nav.current().map(str::to_string); self.report_failure(src, page, format!("{e:#}")); false`,
and the `match src` lives in `report_failure`:

```rust
/// Tell the configurators an input's action failed, against the key (on `page`) or
/// dial it came from.
fn report_failure(&self, src: Source, page: Option<String>, message: String) {
    log::warn!("{src}: {message}");
    match src {
        Source::Key(index) => self.hub.publish(Event::ActionFailed { index, page, message }),
        Source::Turn(index) | Source::Press(index) | Source::Tap(index) => {
            self.hub.publish(Event::DialActionFailed { index, message })
        }
        Source::Flick => {}
    }
}

/// Every pass: report the Hyprland dispatches that failed on the worker.
pub fn poll_hypr(&mut self) {
    while let Ok(f) = self.hypr.failed.try_recv() {
        self.report_failure(f.src, f.page, f.message);
    }
}
```

**fix/review-2 (R6):** every Hyprland dispatch goes to one ordered background worker,
`HyprWorker` (`omadeckd/src/hypr_worker.rs`, `Daemon.hypr`). `dispatch` sends it a
`HyprJob { src, page, dispatch, steps }` and returns `Ok(false)` once the action is
*queued*; a dispatch that fails later comes back as a `HyprFailed { src, page, message }`,
and `poll_hypr` reports it through `report_failure`. So `report_failure` is where this
PR's `Source::Ctl => {}` arm goes, and it covers both paths.

`main.rs` `run()` order on each iteration (after PR 10; check it):

1. Drain the notify channel; the 1 s stamp check; `reload_at` due → `daemon.reload()`.
2. `daemon.handle_commands()`.
3. `daemon.apply_values()`.
4. `daemon.apply_hypr()` (PR 9).
5. `daemon.poll_jobs()`.
6. `daemon.poll_hypr()` (fix/review-2).
7. No deck: `daemon.flush()`, then every 2 s `deck::open` → `attach` (nothing is published
   on a miss), else sleep `wait_for(now, next_deadline())`; `continue`.
8. Deck: `daemon.pass()`; `Err` → `detach`.

```rust
// omadeckd/src/deck.rs
pub const DIAL_FLASH: Duration = Duration::from_millis(150);
pub struct Deck {
    pub dev: Arc<StreamDeck>, pub reader: Arc<DeviceStateReader>, pub kind: Kind, pub info: DeckInfo,
    pub dial_pressed: Vec<bool>,
    pub flash_until: Vec<Option<Instant>>,   // the tap time, not a deadline (bug 15)
    pub dirty_panels: Vec<bool>,
    pub dirty_keys: Vec<bool>,
}
impl Deck {
    pub fn mark_key_dirty(&mut self, i: u8);
    pub fn mark_dirty(&mut self, i: u8);
    pub fn flash_deadline(&self) -> Option<Instant>;   // flash_until.iter().flatten().map(|t| *t + DIAL_FLASH).min()
    pub fn expire_flashes(&mut self, now: Instant);
}
pub fn open(hid: &mut HidApi) -> Option<Deck>;          // builds the Vecs: vec![None; n_dials], vec![false; n_keys], …

// omadeckd/src/hub.rs
const CLIENT_QUEUE: usize = 256;
pub struct Hub { path: PathBuf, inner: Arc<Mutex<Inner>>, commands: mpsc::Receiver<Command> }
struct Inner { clients: Vec<SyncSender<Arc<str>>>, replay: Replay }
impl Hub {
    pub fn start(path: PathBuf) -> Result<Self>;  // "ipc" accept thread; per client: (queue, lines) = sync_channel(CLIENT_QUEUE),
                                                  // "ipc-client" reader (ipc::commands → tx), replay try_sent under the lock,
                                                  // clients.push(queue), spawn_writer(stream, lines) ("ipc-write")
    pub fn publish(&self, ev: Event);             // try_send to each; Full → event dropped, Disconnected → client dropped
    pub fn commands(&self) -> mpsc::TryIter<'_, Command>;
    pub fn shutdown(self);                        // removes the socket file
}
// tests: fn socket(name: &str) -> PathBuf; imports BufRead, BufReader, Duration, Instant.

// omadeckd/src/nav.rs
impl Nav { pub fn current(&self) -> Option<&str>; pub fn go(&mut self, page: Option<&str>) -> bool;
           pub fn back(&mut self) -> bool; pub fn switch(&mut self, page: Option<&str>) -> bool; }
```

GUI (`omadeck-gui/src/editor.rs`, `app.rs`):

```rust
type ActionRun = Rc<dyn Fn(&Rc<App>) -> Action>;
fn test_button(app: &Rc<App>, _slot: &ActionSlot, run: ActionRun) -> gtk::Button;
// Today: Open page → show_page(Some(page), true); is_deck_owned() → insensitive,
// "<kind> only works on the deck"; else action::run(&act) on the GTK thread, toast on error.
// PR 10: one more arm in the tooltip match: `Action::AppVolume { focused: true, .. }` →
// insensitive, "Follows the focused window — here that would be the configurator".
// Chosen-app actions run through action::run (it spawns its own thread).
// Key: test_button(app, &slot, run) with run = the face being edited (action_for(face_tab == On)).
// Dial: test_button(app, &slot, slot.get.clone()).
// App: `daemon_up: Cell<bool>` (private), `pub fn toast(&self, msg: &str)`,
//      `pub fn show_page(self: &Rc<Self>, page: Option<String>, tell_daemon: bool)`.
// app.rs imports `ipc::{self, Command as DaemonCommand, Event}` (std::process::Command is there too).
```

**PR 11 keeps `test_button`'s parameter types** but uses the slot: `_slot` becomes
`slot`, and `slot.target` says whether ▶ is the key's own (`ActionTarget::Key`). `KeyRef`
gains `pub fn index(&self) -> u8 { self.index }` beside `page()`.

## Command line

No `ctl` prefix and no new binary. `press` and `dial` are already control verbs of
`omadeckd`, so the new ones sit next to them. `omadeck` is the GTK app.

```text
omadeckd page next|prev|home|NAME     next / previous page (as a flick), Home, or the page NAME
omadeckd brightness N|+N|-N           set the brightness to N%, or change it by N
omadeckd press KEY [PAGE]             press a key (from 0) on the page the deck shows, or on PAGE
omadeckd dial N left|right|press [STEPS]
omadeckd reload                       re-read the config and theme now; print the error if it's broken
omadeckd state                        print the daemon's state as one JSON line
```

- Output: quiet except one line the daemon sends back (`key 3: Launch firefox`,
  `page Media`, `brightness 40%`). Errors go to stderr with exit 1. Usage errors exit 2,
  as today.
- **No daemon:** `press` and `dial` run the action locally, as today. They also do that when
  the daemon answers `no_deck`. `page`, `brightness`, `reload` and `state` print
  `omadeckd is not running (systemctl --user start omadeckd)` and exit 1.
- **Daemon busy:** if the daemon drops the connection without answering (below), the CLI
  prints `daemon busy: omadeckd closed the connection without answering; try again` and
  exits 1. `press` and `dial` don't fall back then: the daemon may have run it.
- `state` prints the reply line as is, for `jq`:
  `{"reply":"state","deck":{"model":"Stream Deck +",…},"page":"Media","pages":["Media","Apps"],"brightness":40}`.

Decided:
- **Pages by name.** `next`/`prev`/`previous` step like a flick (`PageStep`, folders
  skipped). `home` (any case) is Home. Anything else is a page name. A page literally named
  `next` can't be reached this way; that's fine.
- **`press KEY` with no PAGE means the page the deck shows** when it goes through the daemon
  (it's a physical press). Offline it stays Home, as `press` does today, because nothing is
  shown. PAGE `home` (any case) means Home in both.
- **Brightness `+N`/`-N` changes, `N` sets.** `N` above 100 is clamped to 100; `±N` is
  clamped to −100..=100 before it becomes an `i8`.

### Hyprland binds (for the README)

Omarchy keeps user binds in `~/.config/hypr/bindings.conf` and uses `bindd` (bind with a
description). `binded` repeats while held:

```conf
bindd = SUPER, F1, Next deck page, exec, omadeckd page next
bindd = SUPER SHIFT, F1, Previous deck page, exec, omadeckd page prev
bindd = SUPER, F2, Deck home page, exec, omadeckd page home
binded = SUPER, F3, Dim deck, exec, omadeckd brightness -10
binded = SUPER, F4, Brighten deck, exec, omadeckd brightness +10
bindd = SUPER, F5, Deck key 3, exec, omadeckd press 3
```

## Capture on the machine first

- `ls ~/.config/hypr/ && head -30 ~/.config/hypr/bindings.conf`: confirm the file exists and
  uses `bindd`. If this Hyprland uses a Lua config, paste its bind syntax here and use that
  in the README instead.
- `ls -l $XDG_RUNTIME_DIR/omadeck.sock` on today's build, to compare with the 0600 one.

## IPC (`omadeck-core/src/ipc.rs`)

The same socket carries both directions: events tagged `"event"`, commands tagged
`"command"`, replies tagged `"reply"`. The new commands each get **exactly one reply**, on
the connection that sent them. `ShowPage` and `SetBrightness` stay fire-and-forget, so the
configurator's connection is unchanged.

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Show a page on the deck (`None` is home), e.g. the one being edited.
    ShowPage { page: Option<String> },
    /// Set the panel brightness, e.g. from the configurator's settings.
    SetBrightness { value: u8 },
    /// Press key `index` as if by hand, but quietly: no `KeyDown`/`KeyUp`. `page: None`
    /// is the page the deck shows; a name (or "Home") that isn't the shown page runs that
    /// page's key without flashing anything.
    Press {
        index: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<String>,
    },
    /// Turn dial `index` by `steps` detents, or press it (like a tap on its panel).
    Dial {
        index: u8,
        dir: Direction,
        #[serde(default = "one", skip_serializing_if = "is_one")]
        steps: u8,
    },
    /// Run an action on the daemon, deck-owned ones included (Try it now, `page next`).
    Run { action: Action },
    /// Show page `page` (`None` is home). Unlike `ShowPage`, a missing page is an error.
    Page { page: Option<String> },
    /// Re-read config and theme now.
    Reload,
    /// Report the daemon's state.
    State,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "snake_case")]
pub enum Reply {
    Ok {
        #[serde(default, skip_serializing_if = "String::is_empty")]
        message: String,
    },
    Error { message: String },
    /// No deck is attached; `press`/`dial` callers run the action themselves.
    NoDeck,
    State {
        #[serde(default)]
        deck: Option<DeckInfo>,
        #[serde(default)]
        page: Option<String>,
        #[serde(default)]
        pages: Vec<String>,
        brightness: u8,
    },
}

fn one() -> u8 { 1 }
fn is_one(n: &u8) -> bool { *n == 1 }

impl Command { pub fn to_line(&self) -> String; }   // line(self)
impl Reply {
    pub fn to_line(&self) -> String;
    pub fn ok(message: impl Into<String>) -> Self;
    pub fn error(message: impl Into<String>) -> Self;
}

/// How long `request` waits for the daemon's answer.
pub const REPLY_TIMEOUT: Duration = Duration::from_secs(3);
/// The longest command line the daemon reads; longer ones are refused.
pub const MAX_LINE: usize = 4096;

/// Parse one command line from a client. Unknown or malformed commands are an error
/// message (the hub sends it back as `Reply::Error`), so a newer client never hangs on an
/// older daemon.
pub fn parse_command(line: &str) -> Result<Command, String>;

/// Send one command and wait for its reply. `Ok(None)` when no daemon is running (no
/// runtime dir, no socket, or a stale one).
pub fn request(cmd: &Command) -> Result<Option<Reply>>;

/// The first reply line from the daemon. Skips the status replay and events that arrive
/// first. Split out of `request` so it can be tested on a `&[u8]`.
pub fn read_reply(r: impl BufRead) -> Result<Reply>;
```

- `Direction` (core `config.rs`) gains `Serialize, Deserialize` and
  `#[serde(rename_all = "snake_case")]`, so the wire says `"left"`, `"right"`, `"press"`.
  Decided: no separate `DialInput` enum; `DialConfig::action(dir)` already picks the slot.
- `parse_command`: `serde_json::from_str::<Command>(line.trim())`. On error, if the line
  parses as a JSON object with a `"command"` string, `Err(format!("unknown command “{name}”"))`,
  else `Err("not a command".into())`. (A known command with bad fields is also "unknown
  command"; that's fine.)
- `request`: `let Ok(path) = paths::socket_path() else { return Ok(None) }`. Connect;
  `NotFound` or `ConnectionRefused` → `Ok(None)`; any other error → `Err`. Set the read
  timeout to `REPLY_TIMEOUT` and the write timeout to 200 ms, write `cmd.to_line()`, then
  `read_reply(BufReader::new(stream)).map(Some)`.
- `read_reply`: for each line, parse as `serde_json::Value`; skip it unless it has a
  `"reply"` key; then `serde_json::from_value::<Reply>` (error: `unexpected reply from
  omadeckd: {line}`). A read error of kind `WouldBlock` or `TimedOut` (Linux gives
  `WouldBlock` for a socket timeout) →
  `omadeckd did not answer (is it older than this omadeckd? systemctl --user restart omadeckd)`.
  EOF before a reply → `daemon busy: omadeckd closed the connection without answering; try again`.

Decided:
- **`Run { action }` instead of a command per verb.** It reuses the TOML action shapes, lets
  *Try it now* send unsaved edits, and later deck-owned actions (PR 12's multi) work for free.
  `page next` sends `Run { PageStep { change: 1 } }`; `brightness +10` sends
  `Run { Brightness { change: 10, set: None } }`; `brightness 30` sends
  `Run { Brightness { change: 0, set: Some(30) } }`.
- **`Page { page }` is the one page-specific command**, because Home has no action
  (`Action::Page` needs a name). `ShowPage` can't serve: it has no reply and parks a missing
  page in `wanted_page` for the configurator's save race.
- **`Dial { dir: Press }` acts like a strip tap** (flash, run `press`), not `EncoderDown`:
  no `EncoderUp` would ever follow.

## Hub (`omadeckd/src/hub.rs`)

```rust
/// One command from a client, with the way back to that client.
pub struct Request { pub cmd: Command, pub reply: ReplyTo }

/// A client's queue to its writer thread (the same one events go into), and a handle to
/// close its socket.
#[derive(Clone)]
pub struct ReplyTo { queue: SyncSender<Arc<str>>, stream: Arc<UnixStream> }

impl ReplyTo {
    /// Queue a reply line. Never blocks the loop, and never drops a reply silently: if the
    /// client's queue is full, the client is disconnected (its CLI then says "daemon busy").
    pub fn send(&self, r: &Reply);
}

pub struct Hub { path: PathBuf, inner: Arc<Mutex<Inner>>, commands: mpsc::Receiver<Request> }
impl Hub {
    pub fn commands(&self) -> mpsc::TryIter<'_, Request>;   // was TryIter<'_, Command>
}

/// The `ipc-client` thread's body: read command lines, hand them to the loop, answer
/// parse errors itself.
fn read_requests(stream: UnixStream, reply: ReplyTo, tx: mpsc::Sender<Request>);
```

`ReplyTo::send`:

```rust
pub fn send(&self, r: &Reply) {
    match self.queue.try_send(r.to_line().into()) {
        Ok(()) | Err(TrySendError::Disconnected(_)) => {}
        Err(TrySendError::Full(_)) => {
            log::warn!("a client isn't reading; closing its connection so the reply isn't lost");
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
        }
    }
}
```

- `shutdown(Both)` acts on the socket, not just this fd: the writer's next write fails and
  it ends, the reader sees EOF and ends, the client reads EOF. The hub drops the client on
  its next `publish` (`Disconnected`), as now.
- In the accept loop (which already makes `(queue, lines)` before spawning the reader),
  take two `try_clone`s of the stream: one for the reader, one for
  `ReplyTo { queue: queue.clone(), stream: Arc::new(closer) }`. The rest of the loop
  (replay under the lock, push, `spawn_writer`) is unchanged.
- `read_requests` replaces `ipc::commands` in the reader. It loops on `read_line` through
  `(&mut reader).take(ipc::MAX_LINE as u64 + 1)`. `Ok(0)` or `Err` ends the thread. A line
  longer than `MAX_LINE` gets `Reply::error("command too long")` and the thread ends. Blank
  lines are skipped.
- Each line goes through `ipc::parse_command`. `Ok(cmd)` → `tx.send(Request { cmd, reply:
  reply.clone() })` (a send error ends the thread). `Err(msg)` → `reply.send(&Reply::error(msg))`
  straight from the reader. Only the writer thread touches the stream's write side, so a
  reply can't interleave with an event line.
- After `bind`, `std::fs::set_permissions(&path, Permissions::from_mode(0o600))`. The runtime
  dir is already 0700; this makes it explicit now that the socket accepts commands.
- A ctl client is a normal client: it gets the replay and events until it hangs up. It lives
  a few milliseconds, and `read_reply` skips what isn't a reply.
- `ipc::commands` stays in core (it's public) but nothing in the daemon uses it any more.

Decided (2026-10-17): **replies are never dropped.** Events may be dropped for a slow
client, as now; a reply that can't be queued closes that client instead, so a CLI never
waits 3 s for an answer that was thrown away.

## Daemon (`omadeckd/src/daemon.rs`, `deck.rs`, `main.rs`)

### Input paths shared with commands

`Source` gains `Ctl` (`Display`: `omadeckd`). The body of `run_input` moves into a method
that returns the error, and `run_input` keeps its `bool` for its callers (PR 8 flips a
toggle only when it ran):

```rust
/// What `run_input` did: log, `dispatch(src, act, steps)`, show_page if moved, and on
/// `Err` `report_failure(src, nav.current(), …)` (nothing published for `Ctl`: the reply
/// carries it). The error is returned too.
pub fn try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()>;

pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool {
    self.try_input(src, act, steps).is_ok()
}
```

`report_failure`'s match gains `Source::Ctl => {}` (it still logs). That one arm also
covers a Hyprland action queued from ctl that fails later on the worker: `poll_hypr`
reports it through `report_failure`, so it is only logged (its reply has gone).

`press_key` gets a result: `try_input(..)?` where it called `run_input`; "still running,
press ignored" and a missing key stay `Ok(())`; a `run_tracked` failure returns the error
after publishing it, as now. The rest of the input arms move into methods, bodies
unchanged, so commands share them:

```rust
/// ButtonDown, and ctl's `Press`: paint pressed, publish `KeyDown` only when `publish`
/// (the hardware arm passes true, ctl false), then `press_key(i)`.
fn key_down(&mut self, i: u8, publish: bool) -> Result<()>;
/// PR 8's, now with a result.
fn press_key(&mut self, i: u8) -> Result<()>;
/// EncoderTwist, and ctl's `Dial` left/right: publish `DialTurn` only when `publish`
/// (hardware true, ctl false), run turn_right (delta >= 0) or turn_left with
/// steps = |delta| capped at 8. An empty action does nothing and is Ok.
fn turn_dial(&mut self, i: u8, delta: i8, publish: bool) -> Result<()>;
/// A tap on panel i (TouchScreenPress after tap_to_dial, and `Dial { dir: Press }`):
/// flash_until[i] = tap time, mark dirty, run the dial's press with Source::Tap(i).
/// Publishes nothing: the `TouchScreenPress` arm publishes `StripTap` itself.
fn tap_dial(&mut self, i: u8) -> Result<()>;
```

The `handle_update` arms call them with `publish: true` and ignore the result (`let _ =`),
as they ignore `run_input`'s `bool` today. The `TouchScreenPress` arm keeps its
`StripTap` publish and the `tap_to_dial` mapping, and calls `tap_dial(i as u8)`.

**Ctl presses are quiet.** Commands call `key_down(i, false)` and `turn_dial(i, d,
false)`, so an open configurator doesn't move its selection (or drop an unsaved State
command) while the user edits another key. Everything else still goes out: the deck
flashes the key, and `KeyState`, `DialStatus`, `PageChanged`, `Brightness` and failures
are published as for a hardware press, so faces stay in sync.

### Key flash

A ctl press has no `ButtonUp`, so the key flashes like a strip tap, with the same
convention as `flash_until` (bug 15):

```rust
// deck.rs
/// How long a key stays drawn pressed after a ctl press.
pub const KEY_FLASH: Duration = Duration::from_millis(150);
pub struct Deck { /* … */ pub key_flash: Vec<Option<Instant>> }  // the press time; vec![None; n_keys] in open
impl Deck {
    /// End the key flashes that have run their time and mark those keys dirty.
    pub fn expire_key_flashes(&mut self, now: Instant);
}
/// Clear the flashes older than `len` and return their indices. Pure, for the test.
fn take_ended(flashes: &mut [Option<Instant>], now: Instant, len: Duration) -> Vec<u8>;
```

- `flash_deadline` also takes `key_flash`'s `t + KEY_FLASH`, so `next_deadline` covers it.
- `pass` calls `expire_key_flashes(now)` right after `expire_flashes`, before it reads
  (`flush` repaints them). No `KeyUp` is published: only a ctl press flashes a key, and
  it published no `KeyDown`.
- `Daemon::key_states` draws a flashing key pressed:
  `pressed = deck.key_flash[i].is_some()`, so the end-of-pass paint keeps the flash.
- The hardware `ButtonDown(i)` and `ButtonUp(i)` arms clear `key_flash[i]` first.

### Commands on the loop

`handle_commands` keeps only the last `ShowPage` and the last `SetBrightness`, as now, and
answers every other request at once, in arrival order:

```rust
pub fn handle_commands(&mut self) {
    let mut show = None;
    let mut bright = None;
    // Collected first: `handle_command` needs `&mut self`, the iterator borrows `self.hub`.
    let requests: Vec<Request> = self.hub.commands().collect();
    for req in requests {
        match req.cmd {
            Command::ShowPage { page } => show = Some(page),
            Command::SetBrightness { value } => bright = Some(value),
            cmd => {
                let r = self.handle_command(cmd);
                req.reply.send(&r);
            }
        }
    }
    // … the `bright` and `show` handling below is unchanged …
}

/// Answer one ctl command. Never blocks: it does what a key press does.
fn handle_command(&mut self, cmd: Command) -> Reply;
/// The action key `i` on `page` runs now: the on action while it's on.
fn key_action_now(&self, page: Option<&str>, i: u8) -> Option<Action>;
```

`main.rs`'s `run()` calls `handle_commands` on every iteration, deck or not, so commands are
answered with no deck.

| Command | No deck | With a deck |
|---|---|---|
| `Press { index, page: None }` | `NoDeck` | `index >= info.key_count()` → `Error("no key {index}; this deck has {n} keys")`. Else `act = key_action_now(shown, index)`, `key_down(index, false)`, then `key_flash[index] = Some(now)`. `Ok("key 3: {act.summary()}")` or `Error` |
| `Press { index, page: Some(p) }` | `NoDeck` | `p` is Home (any case) or the shown page → as above. Missing page → `Error("there's no page named “p”")`. Else `try_input(Ctl, &key_action_now(Some(p), index), 1)`: no flash, no `KeyDown`, no toggle flip |
| `Dial { index, dir, steps }` | `NoDeck` | `index >= info.dials` → `Error("no dial {index}; this deck has {n} dials")`. `Left`/`Right` → `turn_dial(index, ∓/±steps.clamp(1, 8) as i8, false)`. `Press` → `tap_dial(index)`. `Ok("dial 0 right x5: {summary}")` |
| `Run { action }` | `try_input(Ctl, &action, 1)` | same |
| `Page { page }` | missing → `Error`; else `nav.go(page)`, `show_page()` if moved | same |
| `Reload` | `self.reload()` | same |
| `State` | `State { deck: None, page, pages: cfg.page_names(), brightness: rt.values.brightness }` | `deck: Some(info.clone())` |

- `Run`'s message is `run_message(&action, nav.current(), rt.values.brightness)` (task 4),
  read after it ran.
- `Page`/`Back`/`PageStep`/`Brightness` work with no deck: `dispatch` moves `nav` and
  `show_page` publishes `PageChanged`; `set_brightness` sets, saves and publishes. The deck
  shows both when it connects (`attach` paints `nav.current()` and the brightness).

Decided:
- **Decided: ctl presses are quiet** (no `KeyDown`/`KeyUp`, no `DialTurn`). A real press
  selects its key in an open configurator; a ctl press (a bind, a script, the
  configurator's own ▶) must not move the selection while the user edits another key, or
  drop an unsaved State command. The deck still flashes the key, and `KeyState` and
  `DialStatus` still go out, so faces stay in sync.
- **A press on a page the deck isn't showing** runs the key's current action (the on action
  while it's on) through `try_input(Ctl, ..)`. It doesn't flip a toggle or start a tracked
  job: those follow presses of the key on the deck.
- **`reload` returns the error:** `pub fn reload(&mut self) -> Result<()>`. On a bad file it
  still keeps the last good config, restyles and repaints, then returns `Config::load`'s
  error; it stops logging it itself. The debounced caller in `main.rs` logs
  `"{e:#}; keeping the previous config"` as before. The reply is
  `Ok("reloaded config (theme: X)")` or `Error("{e:#}; keeping the previous config")`.
  `Reload` also clears `reload_at`.
- **A Hyprland action's reply means "queued"** (fix/review-2's worker). A dispatch that
  fails later is published against its key or dial (`press`, `dial`) or only logged (`Run`).
- **Latency:** up to one pass (≤100 ms with nothing else due). Fine for a keybind and for
  `binded` repeat; commands queue and all run in order.

## GUI (`omadeck-gui`)

*Try it now* goes through the daemon when it runs (finding 15 of the GUI review: running
locally diverged from the deck, and Hyprland waited on the GTK thread). The decision is made
on click, from `App::daemon_up()`, so it never goes stale while the editor is open.

Two kinds of ▶:
- **A key's own ▶** (the key group's header; `slot.target` is `ActionTarget::Key`) sends
  `Command::Press { index, page }` for that key when the daemon is up, so it does exactly
  what the deck does: the key flashes, a toggle flips, a `busy_while_running` key is
  tracked. `page` is always the key's page by name (`KeyRef::page()`, `None` → `HOME`).
  The configurator shows the deck the page it edits, so that is normally the shown page
  and the press is a full one.
- **▶ in a nested editor** (the toggle's "When pressed while on", and later PR 12's steps
  and PR 16's long press) and on a dial sends `Command::Run` for that action only: no
  flash, no flip.

```rust
/// What ▶ does with an action, given whether the daemon is up. Pure, for the test.
#[derive(Debug, PartialEq)]
enum Route {
    /// Nothing to run.
    Nothing,
    /// Open page: show it here (and on the deck), as today.
    ShowPage(String),
    /// `Command::Press` for the key itself, through the daemon.
    Press { index: u8, page: String },
    /// `Command::Run` through the daemon.
    Daemon,
    /// A deck-owned action with no daemon: say so.
    NeedsDaemon,
    /// Run it here, off the GTK thread.
    Local,
}
/// `key` is the key's index and page (`None` is Home) when ▶ is the key's own, `None`
/// for a nested editor or a dial.
fn route(act: &Action, key: Option<(u8, Option<&str>)>, daemon_up: bool) -> Route;
```

`route`, first match wins: `None` → `Nothing`; `key` is `Some((index, page))` and the
daemon is up → `Press { index, page: page.unwrap_or(HOME).to_string() }`; `Page { page }`
→ `ShowPage(page.clone())`; deck-owned → `Daemon` if up, else `NeedsDaemon`; anything
else → `Daemon` if up, else `Local`.

`test_button` at build (sensitivity and tooltip only):

| Action | Sensitive | Tooltip |
|---|---|---|
| `None` | no | "Try it now" |
| `Page { page }` | if `has_page(page)` | "Try it now" |
| PR 10's focused App volume | no | PR 10's tooltip, unchanged |
| other `is_deck_owned()` | yes | "Try it on the deck" |
| anything else | yes | "Try it now" |

On click:

```rust
let key = match &slot.target {
    ActionTarget::Key(k) => Some((k.index(), k.page().map(str::to_string))),
    _ => None,
};
// … in the handler:
let act = (run)(&a);
match route(&act, key.as_ref().map(|(i, p)| (*i, p.as_deref())), a.daemon_up()) { … }
```

- `ShowPage(p)` → `a.show_page(Some(p), true)`, as today.
- `NeedsDaemon` → toast "This works on the deck: start omadeck's service first".
- `Press`, `Daemon` and `Local` → off the GTK thread:

```rust
let a = a.clone();
glib::spawn_future_local(async move {
    let res = gio::spawn_blocking(move || ipc::request(&ipc::Command::Run { action: act })).await;
    match res {
        Ok(Ok(Some(ipc::Reply::Error { message }))) => a.toast(&message),
        Ok(Ok(Some(_))) => {}
        Ok(Ok(None)) => a.toast("omadeckd isn't running"),
        Ok(Err(e)) => a.toast(&format!("{e:#}")),
        Err(_) => a.toast("Couldn't reach omadeckd"),
    }
});
```

  `Press` is the same shape. The daemon presses what it has loaded, and the GUI saves
  250 ms after an edit, so first, on the GTK thread, `let saved = a.flush_save();`
  (`flush_save` becomes `pub fn flush_save(self: &Rc<Self>) -> bool`, true when it wrote a
  pending save). The blocking closure then reloads if needed, sends the press and, if the
  daemon has no deck, runs the action there instead:

```rust
move || {
    if saved {
        // The daemon's own reload waits 150 ms; this press must see the edit.
        ipc::request(&ipc::Command::Reload)?;
    }
    match ipc::request(&ipc::Command::Press { index, page: Some(page) })? {
        Some(ipc::Reply::NoDeck) => ipc::request(&ipc::Command::Run { action: act }),
        r => Ok(r),
    }
}
```

  `Local` is the same shape with `gio::spawn_blocking(move || action::run(&act))` and a
  toast on `Ok(Err(e))`.

- The key's `run` (in `build_key`) becomes the key's own action,
  `a.key(i).map(|k| k.action).unwrap_or_default()`, no longer `action_for(face_tab ==
  On)`: with the daemon up the press picks the face itself, and offline (`Local`) the
  key's action is what a press runs while off. Delete the comment above it that says
  "▶ runs the face being edited".
- The Toggle group gets its own ▶ for the on action, built where `on_slot` is (only for a
  key with `[key.on]`):
  `toggle.set_header_suffix(Some(&test_button(app, &on_slot, on_slot.get.clone())));`.
  Its target is `ActionTarget::KeyOn`, so it sends `Run`.
- `App` gets `pub fn daemon_up(&self) -> bool { self.daemon_up.get() }`.
- The page change or brightness comes back as `PageChanged` / `Event::Brightness`, which the
  window already follows.
- The settings brightness slider stays on `SetBrightness`.

Decided: **a key's own ▶ presses the key; nested ▶ runs its action only.** The key's ▶
is "what happens when I press this", so it goes through the same path as the deck,
toggle flip included. A nested editor's ▶ tries just the action being edited (the on
action, a step, a long press), whatever state the key is in. A press
runs what the daemon has loaded, hence the `flush_save` and `Reload` before it.

## Tasks

Each task ends with `cargo fmt && cargo clippy --all-targets -- -D warnings` clean, plus the
check named.

### Task 1: IPC types, parsing and the client (~230 lines)

Files: `crates/omadeck-core/src/ipc.rs`, `crates/omadeck-core/src/config.rs` (`Direction`'s
derives).

- The `Command` variants, `Reply`, `one`/`is_one`, `Command::to_line`,
  `Reply::to_line`/`ok`/`error`, `REPLY_TIMEOUT`, `MAX_LINE`, `parse_command`, `request`,
  `read_reply`, all as above. `ipc.rs` adds `use crate::config::{Action, Direction};`.
- `Direction`: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]` and
  `#[serde(rename_all = "snake_case")]`.
- `Daemon::handle_commands`' `match cmd` on `ShowPage`/`SetBrightness` stops being
  exhaustive. Add `_ => {}` there for now; task 2 replaces it.

Tests (in `ipc::tests`):
- `command_roundtrip`: every variant, `Dial` with and without `steps`.
- `command_wire_shape` (below).
- `reply_roundtrip`: `Ok` with and without message, `Error`, `NoDeck`, `State` with `deck: None`.
- `reply_skips_events_and_names_eof`: below.
- `unknown_command_is_an_error`: `parse_command(r#"{"command":"explode"}"#)` is
  `Err("unknown command “explode”")`; `parse_command("nonsense")` is `Err("not a command")`.

```rust
#[test]
fn command_wire_shape() {
    // The socket protocol is a contract with scripts; pin it.
    assert_eq!(
        Command::Run { action: Action::PageStep { change: 1 } }.to_line(),
        "{\"command\":\"run\",\"action\":{\"type\":\"page_step\"}}\n"
    );
    assert_eq!(Command::Press { index: 3, page: None }.to_line(), "{\"command\":\"press\",\"index\":3}\n");
    assert_eq!(
        Command::Dial { index: 0, dir: Direction::Right, steps: 1 }.to_line(),
        "{\"command\":\"dial\",\"index\":0,\"dir\":\"right\"}\n"
    );
    assert_eq!(Command::State.to_line(), "{\"command\":\"state\"}\n");
    assert_eq!(Reply::ok("").to_line(), "{\"reply\":\"ok\"}\n");
    assert_eq!(Reply::NoDeck.to_line(), "{\"reply\":\"no_deck\"}\n");
}

#[test]
fn reply_skips_events_and_names_eof() {
    let lines = "{\"event\":\"disconnected\"}\n{\"event\":\"page_changed\",\"page\":null}\n\
                 {\"reply\":\"error\",\"message\":\"no key 40; this deck has 8 keys\"}\n";
    let r = read_reply(lines.as_bytes()).unwrap();
    assert_eq!(r, Reply::error("no key 40; this deck has 8 keys"));
    // The daemon closed the connection before answering: its queue for us was full.
    let e = read_reply("{\"event\":\"disconnected\"}\n".as_bytes()).unwrap_err();
    assert!(e.to_string().starts_with("daemon busy"), "{e}");
}
```

(`Action::PageStep { change: 1 }` writes no `change`: `skip_serializing_if = "is_default_step"`.)

Check: `cargo test -p omadeck-core ipc::` → `test result: ok`, 0 failed, with the 5 new
tests listed next to the existing ones (`dial_turn_wire_shape`, `key_state_wire_shape`, …).

### Task 2: the hub answers, never drops a reply (~180 lines)

Files: `crates/omadeckd/src/hub.rs`, `crates/omadeckd/src/daemon.rs`.

- `Request`, `ReplyTo` and `ReplyTo::send`, `read_requests`, `commands() -> TryIter<'_,
  Request>`, the second `try_clone` in the accept loop, `chmod 0600`.
- `daemon.rs`: `handle_commands` now collects `Request`s. For this task, keep `ShowPage` and
  `SetBrightness` as they are and answer anything else with
  `req.reply.send(&Reply::error("not yet"))`.

Tests, added to `hub::tests` (which has `socket(name)` and imports `BufRead`, `BufReader`,
`Duration`, `Instant`). Add `use omadeck_core::ipc::Reply; use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;`.

```rust
#[test]
fn a_full_queue_disconnects_instead_of_dropping_the_reply() {
    // No socket file needed: a connected pair stands in for the client.
    let (ours, mut theirs) = UnixStream::pair().unwrap();
    let (queue, _lines) = mpsc::sync_channel::<Arc<str>>(1);
    queue.try_send("{\"event\":\"disconnected\"}\n".into()).unwrap(); // full now
    let reply = ReplyTo { queue, stream: Arc::new(ours) };
    reply.send(&Reply::ok("page Media"));
    theirs.set_read_timeout(Some(Duration::from_secs(1))).unwrap();
    let mut buf = String::new();
    assert_eq!(theirs.read_to_string(&mut buf).unwrap(), 0, "the client sees EOF at once, not a timeout");
}
```

And `command_reply_over_socket`: start a hub on `socket("reply")`, check the socket's mode is
`0o600`, write `Command::State.to_line()` from a client, poll `hub.commands().next()` for up
to 3 s, answer `Reply::State { deck: None, page: None, pages: vec![], brightness: 40 }`, and
check `ipc::read_reply(BufReader::new(client))` returns it (the replay comes first and is
skipped). A second client writing `{"command":"explode"}` gets a `Reply::Error`.

Check: `cargo test -p omadeckd hub::` → `test hub::tests::a_full_queue_disconnects_instead_of_dropping_the_reply ... ok`
and `test hub::tests::command_reply_over_socket ... ok`. (The socket tests fail with EPERM
in a sandbox that forbids `bind`; run them on the machine.)

### Task 3: input methods and the key flash (~200 lines)

Files: `crates/omadeckd/src/daemon.rs`, `crates/omadeckd/src/deck.rs`.

- `Source::Ctl` and its `Display` arm; `try_input` with `run_input` as its wrapper;
  `report_failure`'s `Source::Ctl => {}`.
- `key_down(i, publish)`, `turn_dial(i, delta, publish)`, `tap_dial` out of
  `handle_update`, and `press_key -> Result<()>`; the arms call them with `publish: true`
  and `let _ =`.
- `deck.rs`: `Deck.key_flash` (built in `open`), `KEY_FLASH`, `flash_deadline`,
  `expire_key_flashes`, `take_ended`. `pass` expires them (no `KeyUp`); `key_states` draws
  flashing keys pressed; the hardware `ButtonDown`/`ButtonUp` arms clear the flash.

Test (a new `#[cfg(test)] mod tests` in `deck.rs`, `use super::*;`):

```rust
#[test]
fn key_flashes_end_after_their_time() {
    let t0 = Instant::now();
    let ms = Duration::from_millis;
    let mut flashes = vec![Some(t0), None, Some(t0 + ms(100))];
    assert!(take_ended(&mut flashes, t0 + ms(149), KEY_FLASH).is_empty());
    assert_eq!(take_ended(&mut flashes, t0 + ms(150), KEY_FLASH), vec![0]);
    assert_eq!(flashes, vec![None, None, Some(t0 + ms(100))]);
    assert_eq!(take_ended(&mut flashes, t0 + ms(250), KEY_FLASH), vec![2]);
}
```

Check: `cargo test -p omadeckd deck::` → `test deck::tests::key_flashes_end_after_their_time ... ok`,
then `cargo test -p omadeckd` → `test result: ok`, 0 failed (the existing
`source_names_its_key_or_dial` still passes). By hand: a key, a dial turn, a dial press and
a strip tap behave as before, and a key pressed on the deck still selects it in the
configurator (`publish: true`). There's no harness for a `Daemon` with a deck, so the
quiet ctl path is checked on the machine (Hardware checks).

### Task 4: the daemon answers commands (~200 lines)

Files: `crates/omadeckd/src/daemon.rs`, `crates/omadeckd/src/main.rs`.

- `handle_command`, `key_action_now` and the new `handle_commands`. Remove task 2's `"not yet"`.
- `reload() -> Result<()>`; `main.rs`'s debounced call logs the error.

There's no harness for a `Daemon` with a deck, so the test covers the pure piece, the
reply message for `Run` (in `daemon.rs`, test added to its `tests` module):

```rust
/// The reply line for a `Run` that went through: where the deck is now, or what ran.
fn run_message(act: &Action, page: Option<&str>, brightness: u8) -> String {
    match act {
        Action::Page { .. } | Action::Back | Action::PageStep { .. } => {
            format!("page {}", page.unwrap_or(omadeck_core::HOME))
        }
        Action::Brightness { .. } => format!("brightness {brightness}%"),
        a => a.summary(),
    }
}

#[test]
fn run_messages() {
    assert_eq!(run_message(&Action::PageStep { change: 1 }, Some("Media"), 70), "page Media");
    assert_eq!(run_message(&Action::Back, None, 70), "page Home");
    assert_eq!(run_message(&Action::Brightness { change: 10, set: None }, None, 40), "brightness 40%");
    assert_eq!(run_message(&Action::Url { url: "omarchy.org".into() }, None, 40), "Open omarchy.org");
}
```

Check: `cargo test -p omadeckd run_messages` → `1 passed`; `cargo test -p omadeckd` →
`test result: ok`, 0 failed.

### Task 5: the CLI (~200 lines)

Files: `crates/omadeckd/src/main.rs`.

```rust
/// A control verb's arguments as the command to send. `Err` is the usage line.
fn parse_ctl(verb: &str, args: &[String]) -> Result<ipc::Command, &'static str>;
/// Send `cmd`, print the answer, and fall back to running press/dial here. The exit code.
fn ctl(cmd: ipc::Command) -> i32;
/// Today's offline `press` and `dial` bodies, moved out of `main` unchanged except for
/// PAGE "home" meaning Home and `steps` going to `action::run_steps`.
fn press_locally(index: u8, page: Option<&str>) -> i32;
fn dial_locally(index: u8, dir: Direction, steps: u8) -> i32;
```

- `main`: `Some(verb @ ("page" | "brightness" | "reload" | "state" | "press" | "dial"))`
  collects the remaining args, calls `parse_ctl` (usage → `eprintln!("usage: omadeckd {u}")`,
  exit 2), then `std::process::exit(ctl(cmd))`.
- `ctl`: `Ok(Some(Reply::Ok { message }))` prints a non-empty message, 0.
  `Ok(Some(r @ Reply::State { .. }))` prints `r.to_line()` with `print!`, 0.
  `Ok(Some(Reply::Error { message }))` eprints, 1. `Ok(Some(NoDeck))` or `Ok(None)`: `Press`
  and `Dial` run locally; the rest eprint the "not running" line, 1. `Err(e)` eprints `{e:#}`
  (this is where "daemon busy" shows), 1.
- The top-level usage line lists the new verbs.

Usage strings (returned by `parse_ctl`): `page next|prev|home|NAME`,
`brightness N|+N|-N`, `press KEY [PAGE]   (keys are numbered from 0)`,
`dial N left|right|press [STEPS]`, `reload`, `state`.

Tests (add a `#[cfg(test)] mod tests` with `use super::*;` and `use omadeck_core::{Action,
Direction};` to `main.rs`; it has none):
- `parse_ctl_page`: `next`, `prev`, `previous`, `home`, `HOME`, `Media`; no argument and two
  arguments are errors.
- `parse_ctl_brightness` (below).
- `parse_ctl_press_and_dial`: `press 3`, `press 3 Media`, `press x` (error), `dial 1 left`,
  `dial 1 right 4`, `dial 1 press`, `dial 1 up` (error), `dial 1 left 0` (error).

```rust
fn args(s: &str) -> Vec<String> {
    s.split_whitespace().map(String::from).collect()
}

#[test]
fn parse_ctl_brightness() {
    let run = |action| Ok(ipc::Command::Run { action });
    assert_eq!(parse_ctl("brightness", &args("30")), run(Action::Brightness { change: 0, set: Some(30) }));
    assert_eq!(parse_ctl("brightness", &args("+10")), run(Action::Brightness { change: 10, set: None }));
    assert_eq!(parse_ctl("brightness", &args("-10")), run(Action::Brightness { change: -10, set: None }));
    assert_eq!(parse_ctl("brightness", &args("-500")), run(Action::Brightness { change: -100, set: None }));
    assert_eq!(parse_ctl("brightness", &args("250")), run(Action::Brightness { change: 0, set: Some(100) }));
    assert!(parse_ctl("brightness", &args("abc")).is_err());
    assert!(parse_ctl("brightness", &[]).is_err());
}
```

Check: `cargo test -p omadeckd parse_ctl` → `3 passed`. By hand: `omadeckd state` with the
service stopped prints the "not running" line and exits 1 (`echo $?`).

### Task 6: Try it now, and docs (~170 lines)

Files: `crates/omadeck-gui/src/editor.rs`, `crates/omadeck-gui/src/app.rs`, `README.md`,
`site/index.html`, `CHANGELOG.md`.

- `Route`, `route`, `test_button`'s build table and click handler (`slot` now used),
  `KeyRef::index`, `App::daemon_up`, `flush_save -> bool`, the key's `run` as its own
  action, and the Toggle group's ▶ for the on action, as above.
- README "Command line": the new verbs, the Hyprland binds block, and "`press` and `dial` go
  through the daemon when it runs, so the key flashes and page and brightness keys work".
- README "How it works", **Event feed** becomes **Event feed and control**: commands and
  replies on the same socket, the `state` JSON, and "a client that stops reading loses
  events; if a reply can't be queued, the daemon closes that connection".
- `site/index.html`: next to `omadeckd press 3`, add `omadeckd page next`.
- CHANGELOG, Unreleased / Added: "`omadeckd page`, `brightness`, `reload` and `state` control
  the running daemon, e.g. from Hyprland binds. `press` and `dial` go through the daemon when
  it runs, so the key flashes and page and brightness keys work. *Try it now* runs through
  the daemon, so Go back, Next/previous page and Brightness work there too, and a key's
  own ▶ presses the key, toggle flip included."

Test (a new `#[cfg(test)] mod tests` in `editor.rs`, `use super::*;`):

```rust
#[test]
fn try_it_routes() {
    let url = Action::Url { url: "omarchy.org".into() };
    let media = Action::Page { page: "Media".into() };
    // Nested editors and dials (no key): Run for that action only.
    assert_eq!(route(&Action::None, None, true), Route::Nothing);
    assert_eq!(route(&media, None, false), Route::ShowPage("Media".into()));
    assert_eq!(route(&Action::Back, None, true), Route::Daemon);
    assert_eq!(route(&Action::Back, None, false), Route::NeedsDaemon);
    assert_eq!(route(&url, None, true), Route::Daemon);
    assert_eq!(route(&url, None, false), Route::Local);
    // A key's own ▶: a press of that key while the daemon is up, by page name.
    assert_eq!(route(&url, Some((3, None)), true), Route::Press { index: 3, page: "Home".into() });
    assert_eq!(route(&media, Some((0, Some("Apps"))), true), Route::Press { index: 0, page: "Apps".into() });
    assert_eq!(route(&Action::None, Some((3, None)), true), Route::Nothing);
    // No daemon: as a nested ▶.
    assert_eq!(route(&url, Some((3, None)), false), Route::Local);
    assert_eq!(route(&Action::Back, Some((3, None)), false), Route::NeedsDaemon);
    assert_eq!(route(&media, Some((3, None)), false), Route::ShowPage("Media".into()));
}
```

Check (on the machine, it needs GTK): `cargo test -p omadeck-gui try_it_routes` → `1 passed`;
then `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test` → every crate
`test result: ok`.

## Known traps

- `ipc::Command` vs `std::process::Command`: both are used in `main.rs` and `app.rs`.
  Keep the enum qualified (`ipc::Command`) or aliased (`DaemonCommand`, as `app.rs` does).
- Never write to a client stream from the loop. Replies go through `ReplyTo::send` (a
  `try_send` on the client's bounded channel), like events. The only thing the loop does to
  a stream is `shutdown` it when that queue is full.
- `for req in self.hub.commands()` while calling `self.handle_command(..)` won't borrow-check.
  Collect first.
- Every ctl path calls `key_down(.., false)` and `turn_dial(.., false)`; only the
  `handle_update` arms pass `true`. A `true` from ctl moves an open configurator's
  selection.
- `ShowPage`/`SetBrightness` never get a reply. If they did, every configurator would get
  a line per slider move that it skips anyway.
- A socket read timeout is `WouldBlock` on Linux, not `TimedOut`. Match both.
- `gio::spawn_blocking` for `ipc::request` and `action::run` in the GUI. The GTK thread must
  never wait on the socket (up to 3 s) or on `hyprctl`.
- `dial_locally` with `steps > 1`: `action::run_steps` runs Hyprland `steps > 1` on its
  helper thread (fix/review-2 kept it for the callers without a daemon), so the 300 ms
  sleep before exit is kept. Only the daemon uses `HyprWorker`.
- Nothing new is slow on the loop: `handle_command` does what a key press does.

## Security

- The socket is in `$XDG_RUNTIME_DIR` (0700, the user's). `paths::socket_path` has no
  `/tmp` fallback. The socket itself becomes 0600.
- `Run` can run any action, scripts included. Anyone who can connect is the same user and
  can already run those from a shell, so this crosses no privilege boundary. No
  `SO_PEERCRED` check: `UnixStream::peer_cred` is still unstable in std.
- Lines are capped at 4 KiB and answered one at a time on the loop, so a bad client only
  slows itself. A client that never reads is closed when a reply for it can't be queued.

## Backwards compatibility

- Event lines are unchanged. Replies go only to the connection that asked, and old
  configurators drop lines that aren't events.
- An old daemon drops the new commands silently; a new CLI then times out after 3 s with
  the "older than this omadeckd" hint. That only happens mid-upgrade.
- `press` and `dial` keep their arguments and their offline behaviour. No config change.

## Hardware checks (Stream Deck +, 0fd9:0084, Omarchy)

- [ ] `omadeckd page next` / `prev` / `home` / `page Media` switch pages; the
      configurator follows; `page Nope` prints an error and exits 1.
- [ ] `omadeckd brightness 20`, `+10`, `-10`: the deck dims live, a brightness dial's bar
      and the settings slider follow, and the value survives a service restart.
- [ ] With the deck unplugged, `omadeckd brightness 30`, then plug in: it comes up at 30.
- [ ] A held `binded` brightness key: smooth, no backlog after release.
- [ ] `omadeckd press 3` with the daemon up and the configurator open on key 0: key 3
      flashes, the action runs, and the configurator stays on key 0. A page key pressed
      this way switches page. A toggle key's face flips on the deck and in the preview.
- [ ] `omadeckd press 3` with the service stopped: runs locally as before.
- [ ] `omadeckd dial 0 right 5` moves the volume bar five steps; `dial 0 press` flashes the
      panel and mutes. The configurator's selection doesn't move; its strip preview follows.
- [ ] `omadeckd reload` after a deliberate typo prints the parse error; the deck keeps the
      last good config.
- [ ] `omadeckd state | jq .page` with the deck plugged and unplugged.
- [ ] A fast dial spin while running `for i in $(seq 50); do omadeckd state >/dev/null || echo fail; done`:
      no "fail" lines (and if one appears, it says "daemon busy", never a 3 s hang).
- [ ] Configurator: *Try it now* on a Go back key and a brightness dial acts on the deck; with
      the service stopped it toasts. A Hyprland key's ▶ doesn't freeze the window.
- [ ] Configurator: a toggle key's header ▶ flashes the key and flips it, as the deck does;
      the Toggle group's ▶ runs the on action without flipping. Change a key's command and
      click ▶ at once: the new command runs. With the deck unplugged, ▶ still runs the
      action.
- [ ] `ls -l $XDG_RUNTIME_DIR/omadeck.sock` shows `srw-------`.
- [ ] An old configurator build still shows status and key presses against the new daemon.

## Decisions to explain

- **No Elgato equivalent.** Elgato's CLI is for plugin development and its deep links go to
  plugins, so this follows omadeck's own CLI.
- **Flat verbs, not `omadeckd ctl …` or a new binary.** `press` and `dial` already live at
  the top level; a prefix would leave two ways to press a key.
- **One socket.** The hub already reads commands; replies are told apart from events by
  their tag. A ctl client briefly gets events, which it skips.
- **Replies are never dropped.** A client whose queue is full when its reply is due is
  disconnected, so the CLI says "daemon busy" at once instead of hanging.
- **`Run { action }`** reuses the action shapes from the TOML and lets *Try it now* send
  unsaved edits.
- **`press`/`dial` go through the daemon when it's up**, so page and brightness keys work from
  a bind and a shared key/bind action looks the same on the deck. The offline fallback keeps
  today's behaviour for scripts.
- **Try it now goes through the daemon when it's up**, so it behaves as on the deck (volume
  through the watcher, Hyprland on its worker) and never blocks the window. A key's own ▶
  is a press of the key (flash, toggle flip); a nested ▶ runs only its action.
- **Ctl presses are quiet.** The deck flashes and faces update, but an open configurator
  doesn't jump to the key, so ▶ and binds never pull it away from the key being edited.
- **Commands wait for the deck loop (≤100 ms)** instead of touching the deck from the client
  thread. The deck stays single-threaded.
- **No uid check.** The 0700 runtime dir and the 0600 socket are the guard.

## Ask before building

None; all settled 2026-10-17.

## Later

- `omadeckd watch`: print events as JSON lines, for scripts and waybar.
- A waybar module from `state` (page and brightness).
- Editing commands (`set-key …`). Out of scope: the config file is the interface for edits.
- A reply for a Hyprland action that waits for Hyprland's answer (the worker would carry the
  `ReplyTo` back).

**Size:** about 1,050 lines with tests: ipc ~230, hub ~180, inputs and flash ~200, commands
~200, CLI ~200, GUI and docs ~170.
