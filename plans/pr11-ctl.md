# PR 11: `feat/ctl`: drive the running daemon from the shell

Part of [PLAN.md](PLAN.md). Base: `feat/app-volume` (PR 10), which sits on PR 9, PR 8 and
`feat/groundwork`. It owns the **daemon control socket** building block: upstream already
has the GUI → daemon half (`ipc::Command`, tagged `"command"`, read by a thread per client
into `Hub::commands()`). This PR adds variants, a reply to each new one, and the CLI verbs.
There is no second socket and no second `Command` enum.

**Outcome:** `omadeckd page next`, `omadeckd page Media`, `omadeckd brightness 30`,
`omadeckd brightness +10`, `omadeckd reload` and `omadeckd state` drive the running
daemon. `omadeckd press 3` and `omadeckd dial 1 left` go through the daemon when it runs,
so they work like a physical press: the face flashes, page and brightness keys work, toggle
and live state refresh, and volume bars move. With no daemon (or no deck) they run the
action locally, as today. In the configurator, *Try it now* works for Go back, Next/previous
page and Brightness. Hyprland binds can page or dim the deck.

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

Copied from `daily` (`d7bd448`) where groundwork leaves it alone, and from
[groundwork.md](groundwork.md) and PRs 8–10 where they change it. **Before task 1, grep each
name below on `feat/app-volume`.** If one differs, use the merged name and fix this list.

```rust
// omadeck-core/src/ipc.rs (upstream, unchanged by groundwork)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Command {
    /// Show a page on the deck (`None` is home), e.g. the one being edited.
    ShowPage { page: Option<String> },
    /// Set the panel brightness, e.g. from the configurator's settings.
    SetBrightness { value: u8 },
}
fn line<T: Serialize>(v: &T) -> String;          // JSON + '\n'
pub struct Sender(UnixStream);                   // send(&mut self, cmd: &Command) -> Result<()>
pub fn connect() -> Result<(Sender, impl Iterator<Item = Event>)>;   // skips lines that aren't events
pub fn commands(stream: UnixStream) -> impl Iterator<Item = Command>; // skips garbage silently

// omadeck-core/src/device.rs
pub struct DeckInfo { pub model: String, pub serial: String, pub rows: u8, pub cols: u8,
                      pub key_size: u32, pub dials: u8, pub strip: Option<(u32, u32)> } // Serialize + Deserialize

// omadeck-core/src/config.rs
pub const HOME: &str = "Home";                    // no page may be called this (page_name_problem)
impl Config { pub fn has_page(&self, name: &str) -> bool; pub fn page_names(&self) -> Vec<String>;
              pub fn page(&self, page: Option<&str>) -> Option<&Keys>; pub fn key(&self, page: Option<&str>, index: u8) -> Option<&KeyConfig>;
              pub fn dial(&self, index: u8) -> Option<&DialConfig>; }
impl Action { pub fn is_deck_owned(&self) -> bool; pub fn summary(&self) -> String; } // groundwork: Page | Back | PageStep | Brightness

// omadeckd/src/daemon.rs (groundwork Task 7)
#[derive(Debug, Default)] pub struct Runtime { pub values: Values, pub state: State, /* PR 8: */ pub flipped, settle, pub jobs, /* PR 9: */ pub window }
// brightness: rt.values.brightness
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source { Key(u8), Turn(u8), Press(u8), Tap(u8), Flick }   // + Display: "key 3", "dial 1", "strip flick"
pub struct Daemon { pub cfg, pub renderer, pub nav, pub wanted_page, pub hub, pub watcher, pub saver,
                    pub rt: Runtime, pub deck: Option<Deck>, published: HashMap<u8, PanelStatus>,
                    pub reload_at: Option<Instant>, pub repaint_at: Option<Instant> }
impl Daemon {
    pub fn reload(&mut self);                         // keeps the last good config on error, logs it
    pub fn handle_commands(&mut self);                // drains hub.commands(): last ShowPage, last SetBrightness
    pub fn next_deadline(&self) -> Option<Instant>;   // flash, repaint_at, reload_at
    pub fn pass(&mut self) -> Result<(), StreamDeckError>; // with a deck: expire_flashes, read+drain, flush
    fn handle_update(&mut self, u: DeviceStateUpdate);     // the ButtonDown/EncoderTwist/… arms, inline
    /// Logs, dispatches, show_page if moved, publishes ActionFailed/DialActionFailed. True if it ran.
    pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool;
    fn dispatch(&mut self, act: &Action, steps: u8) -> Result<bool>;   // Volume → WatchCmd::Volume
    pub fn set_brightness(&mut self, set: Option<u8>, change: i32);  // works with no deck
    fn show_page(&mut self);                          // paint_all + PageChanged
    fn paint_key(&mut self, i: u8, pressed: bool);
}
// PR 8: fn press_key(&mut self, i: u8)  (toggle/busy, then run_input(Source::Key(i), ..));
//       Event::ActionFailed { index: u8, page: Option<String>, message: String }
// omadeckd/src/main.rs run(): each iteration calls daemon.handle_commands(), apply_values(),
// poll_jobs() (PR 8) and apply_hypr() (PR 9), then pass() with a deck, else flush() (PR 8),
// sleeps wait_for(now, next_deadline()) and scans.

// omadeckd/src/deck.rs
pub struct Deck { pub dev, pub reader, pub kind, pub info, pub dial_pressed: Vec<bool>,
                  pub flash_until: Vec<Option<Instant>>, pub dirty_panels: Vec<bool> }
impl Deck { pub fn flash_deadline(&self) -> Option<Instant>; pub fn expire_flashes(&mut self, now: Instant); }
// flash_until stores the TAP TIME, not a deadline (bug 15); expiry is now - t >= DIAL_FLASH (150 ms).

// omadeckd/src/hub.rs (groundwork Task 8)
const CLIENT_QUEUE: usize = 256;
struct Inner { clients: Vec<SyncSender<Arc<str>>>, replay: Replay }
// accept loop: makes `(queue, lines) = sync_channel::<Arc<str>>(CLIENT_QUEUE)`, then spawns
// the `ipc-client` reader (ipc::commands → mpsc::Sender<Command>), queues the replay lines
// under the lock, pushes `queue`, and `spawn_writer(stream, lines)` (thread `ipc-write`).
pub fn commands(&self) -> mpsc::TryIter<'_, Command>;
// tests: fn socket(name: &str) -> PathBuf  (temp_dir()/omadeck-{name}-{pid}.sock)
```

GUI (groundwork Task 10): `pub(crate) struct ActionSlot { pub target: ActionTarget, pub dir:
Option<Direction>, pub get: Rc<dyn Fn(&Rc<App>) -> Action>, pub set: Rc<dyn Fn(&Rc<App>, Action)>, … }`
and `fn test_button(app: &Rc<App>, slot: &ActionSlot) -> gtk::Button`, which today makes
deck-owned actions (except Open page) insensitive with "… only works on the deck".

`Nav` (unchanged): `current() -> Option<&str>`, `go(Option<&str>) -> bool`,
`back() -> bool`, `switch(Option<&str>) -> bool`.

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
- `state` prints the reply line as is, for `jq`:
  `{"reply":"state","deck":{"model":"Stream Deck +",…},"page":"Media","pages":["Media","Apps"],"brightness":40}`.

Decided:
- **Pages by name.** `next`/`prev`/`previous` step like a flick (`PageStep`, folders
  skipped). `home` (any case) is Home. Anything else is a page name. A page literally named
  `next` can't be reached this way; that's fine.
- **`press KEY` with no PAGE means the page the deck shows** when it goes through the daemon
  (it's a physical press). Offline it stays Home, as upstream's `press` does today, because
  nothing is shown. PAGE `home` (any case) means Home in both.
- **Brightness `+N`/`-N` changes, `N` sets.** `N` above 100 is clamped by
  `apply_brightness`; `±N` is clamped to −100..=100 before it becomes an `i8`.

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
    /// Press key `index` as if by hand. `page: None` is the page the deck shows; a name
    /// (or "Home") runs that page's key without flashing anything.
    Press {
        index: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        page: Option<String>,
    },
    /// Turn dial `index` by `steps` detents, or press it (like a tap on its panel).
    Dial {
        index: u8,
        input: DialInput,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialInput { Left, Right, Press }

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

impl Command {
    pub fn to_line(&self) -> String;               // line(self)
    /// Whether the daemon answers this command. The configurator's two never are.
    pub fn wants_reply(&self) -> bool;             // !matches!(self, ShowPage { .. } | SetBrightness { .. })
}
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

- `parse_command`: `serde_json::from_str::<Command>(line.trim())`. On error, if the line
  parses as JSON with a `"command"` string, `Err(format!("unknown command “{name}”"))`, else
  `Err("not a command".into())`.
- `request`: `let Ok(path) = paths::socket_path() else { return Ok(None) }`. Connect;
  `NotFound` or `ConnectionRefused` → `Ok(None)`; any other error → `Err`. Set the read
  timeout to `REPLY_TIMEOUT` and the write timeout to 200 ms, write `cmd.to_line()`, then
  `read_reply(BufReader::new(stream)).map(Some)`.
- `read_reply`: for each line, parse as `serde_json::Value`; skip it unless it has a
  `"reply"` key; then `serde_json::from_value::<Reply>` (error: `unexpected reply from
  omadeckd: {line}`). A read error of kind `WouldBlock` or `TimedOut` (Linux gives
  `WouldBlock` for a socket timeout) →
  `omadeckd did not answer (is it older than this omadeckd? systemctl --user restart omadeckd)`.
  EOF → `omadeckd closed the connection without answering`.

Decided:
- **`Run { action }` instead of a command per verb.** It reuses the TOML action shapes, lets
  *Try it now* send unsaved edits, and later deck-owned actions (PR 12's multi) work for free.
  `page next` sends `Run { PageStep { change: 1 } }`; `brightness +10` sends
  `Run { Brightness { change: 10, set: None } }`; `brightness 30` sends
  `Run { Brightness { change: 0, set: Some(30) } }`.
- **`Page { page }` is the one page-specific command**, because Home has no action
  (`Action::Page` needs a name). `ShowPage` can't serve: it has no reply and parks a missing
  page in `wanted_page` for the configurator's save race.
- **`DialInput::Press` acts like a strip tap** (flash, run `press`), not `DialDown`: no
  `DialUp` would ever follow.

## Hub (`omadeckd/src/hub.rs`)

```rust
/// One command from a client, with the way back to that client.
pub struct Request { pub cmd: Command, pub reply: ReplyTo }

/// The client's queue to its writer thread (the same `SyncSender<Arc<str>>` events go into).
#[derive(Clone)]
pub struct ReplyTo(SyncSender<Arc<str>>);
impl ReplyTo {
    /// Queue a reply line. Never blocks the loop: if the client's queue is full the
    /// reply is dropped and logged, and the client times out.
    pub fn send(&self, r: &Reply);   // self.0.try_send(r.to_line().into())
}

pub struct Hub { path: PathBuf, inner: Arc<Mutex<Inner>>, commands: mpsc::Receiver<Request> }
impl Hub {
    pub fn commands(&self) -> mpsc::TryIter<'_, Request>;   // was TryIter<'_, Command>
}
```

- In the accept loop (which already makes `(queue, lines)` before spawning the reader),
  move a clone of `queue` into the `ipc-client` thread. The rest of the loop (replay under
  the lock, push, `spawn_writer`) is unchanged.
- The reader stops using `ipc::commands`. It loops on `BufRead::read_line` through
  `(&mut reader).take(MAX_LINE as u64 + 1)`. A line longer than `MAX_LINE` gets
  `Reply::error("command too long")` and the thread ends; EOF ends it too.
- Each line goes through `ipc::parse_command`. `Ok(cmd)` → send
  `Request { cmd, reply: ReplyTo(queue.clone()) }` to the loop. `Err(msg)` →
  `ReplyTo(queue.clone()).send(&Reply::error(msg))` straight from the reader. Only the
  writer thread touches the stream, so a reply can't interleave with an event line.
- After `bind`, `std::fs::set_permissions(&path, Permissions::from_mode(0o600))`. The runtime
  dir is already 0700; this makes it explicit now that the socket accepts commands.
- A ctl client is a normal client: it gets the replay and events until it hangs up. It lives
  a few milliseconds, and `read_reply` skips what isn't a reply.
- `ipc::commands` stays in core (it's public) but nothing in the daemon uses it any more.

## Daemon (`omadeckd/src/daemon.rs`, `deck.rs`, `main.rs`)

### Input paths shared with commands

`Source` gains `Ctl` (`Display`: `omadeckd`). The body of `run_input` moves into a method
that returns the error, and `run_input` keeps its `bool` for its callers (PR 8 flips a
toggle only when it ran):

```rust
/// What `run_input` did: log, dispatch, show_page if moved, publish the failure event
/// for `src` (none for `Ctl`: the reply carries it). The error is returned too.
pub fn try_input(&mut self, src: Source, act: &Action, steps: u8) -> Result<()>;

pub fn run_input(&mut self, src: Source, act: &Action, steps: u8) -> bool {
    self.try_input(src, act, steps).is_ok()
}
```

PR 8 already moved the key's part into `fn press_key(&mut self, i: u8)` (toggle, busy job,
then `run_input(Source::Key(i), ..)`); the `ButtonDown` arm paints pressed and publishes
`KeyDown` before calling it. PR 11 gives `press_key` a result (`try_input(..)?` where it
called `run_input`; "still running, press ignored" and a missing key stay `Ok(())`; a
`run_tracked` failure returns the error after publishing it, as now), and moves the rest of
the arms into methods, bodies unchanged, so commands share them:

```rust
/// ButtonDown: paint pressed, publish KeyDown, then `press_key(i)`.
fn key_down(&mut self, i: u8) -> Result<()>;
/// PR 8's, now with a result.
fn press_key(&mut self, i: u8) -> Result<()>;
/// EncoderTwist: publish DialTurn, run turn_right (delta >= 0) or turn_left with
/// steps = |delta| capped at 8. An empty action does nothing and is Ok.
fn turn_dial(&mut self, i: u8, delta: i8) -> Result<()>;
/// A tap on panel i (TouchScreenPress after tap_to_dial, and `Dial { input: Press }`):
/// flash_until[i] = tap time, mark dirty, run the dial's press with Source::Tap(i).
fn tap_dial(&mut self, i: u8) -> Result<()>;
```

They call `try_input` and return its result; the arms ignore it (`let _ =`), as they ignore
`run_input`'s `bool` today.

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
```

`main.rs`'s `run()` calls `handle_commands` on every iteration, deck or not, so commands are
answered with no deck.

| Command | No deck | With a deck |
|---|---|---|
| `Press { index, page: None }` | `NoDeck` | `index >= kind.key_count()` → `Error("no key {index}; this deck has {n} keys")`. Else `key_down(index)`, then `key_flash[index] = Some(now)`. `Ok("key 3: <summary>")` or `Error` |
| `Press { index, page: Some(p) }` | `NoDeck` | `p` is Home (any case) or the shown page → as above. Missing page → `Error("there's no page named “p”")`. Else `try_input(Ctl, key's action, 1)`: no flash, no `KeyDown` |
| `Dial { index, input, steps }` | `NoDeck` | `index >= info.dials` → `Error`. `Left`/`Right` → `turn_dial(index, ∓/±steps.clamp(1, 8) as i8)`. `Press` → `tap_dial(index)`. `Ok("dial 0 right x5: <summary>")` |
| `Run { action }` | `try_input(Ctl, &action, 1)` | same |
| `Page { page }` | missing → `Error`; else `nav.go(page)`, `show_page()` if moved | same |
| `Reload` | `self.reload()` | same |
| `State` | `State { deck: None, page, pages: cfg.page_names(), brightness: rt.values.brightness }` | `deck: Some(info.clone())` |

- `Run`'s message is `run_message(&action, nav.current(), rt.values.brightness)` (task 3),
  read after it ran.
- `Page`/`Back`/`PageStep`/`Brightness` work with no deck (D1): `dispatch` moves `nav` and
  `show_page` publishes `PageChanged`; `set_brightness` sets, saves and publishes. The deck
  shows both when it connects (`attach` paints `nav.current()`; `paint_all` sets the brightness).

Decided:
- **A ctl press publishes `KeyDown` and later `KeyUp`**, exactly like a real press, so an
  open configurator highlights and selects the key.
- **Key flash, same convention as `flash_until` (bug 15):** `Deck` gains
  `pub key_flash: Vec<Option<Instant>>` (len = key count, `vec![None; n]` in `deck::open`)
  holding the **press time**, and `const KEY_FLASH: Duration = Duration::from_millis(150);`.
  `Deck::flash_deadline` also takes `key_flash`'s `t + KEY_FLASH`, so `next_deadline` covers
  it. New `pub fn expire_key_flashes(&mut self, now: Instant) -> Vec<u8>` clears the ended
  ones and returns them; `pass` calls it right after `expire_flashes` and, for each index,
  `paint_key(i, false)` and publishes `KeyUp`. A hardware `ButtonDown` or `ButtonUp` on that
  key clears its flash first.
- **`reload` returns the error:** `pub fn reload(&mut self) -> Result<()>`. On a bad file it
  still keeps the last good config, restyles and repaints, then returns `Config::load`'s
  error. The debounced caller in `main.rs` logs it as before. The reply is
  `Ok("reloaded config (theme: X)")` or `Error("{e:#}; keeping the previous config")`.
  `Reload` also clears `reload_at`.
- **Latency:** up to one pass (≤100 ms with nothing else due). Fine for a keybind and for
  `binded` repeat; commands queue and all run in order.

## GUI (`omadeck-gui`)

Groundwork's `test_button(app, slot)` makes deck-owned actions insensitive ("… only works on
the deck"). They now try **through the daemon**:

| Action | Sensitive | Tooltip | Click |
|---|---|---|---|
| `None` | no | "Try it now" | — |
| `Page { page }` | if `has_page(page)` | "Try it now" | `a.show_page(Some(page), true)`, as today |
| other `is_deck_owned()` | yes | "Try it on the deck" | below |
| anything else | yes | "Try it now" | `action::run`, toast the error, as today |

Deck-owned click: if `!a.daemon_up()`, toast "This works on the deck: start omadeck's
service first". Else:

```rust
let a = a.clone();
glib::spawn_future_local(async move {
    let res = gio::spawn_blocking(move || ipc::request(&ipc::Command::Run { action: act })).await;
    match res {
        Ok(Ok(Some(Reply::Ok { .. }))) => {}
        Ok(Ok(Some(Reply::Error { message }))) => a.toast(&message),
        Ok(Ok(None)) => a.toast("omadeckd isn't running"),
        Ok(Ok(Some(_))) => {}
        Ok(Err(e)) => a.toast(&format!("{e:#}")),
        Err(_) => a.toast("Couldn't reach omadeckd"),
    }
});
```

- `App` gets `pub fn daemon_up(&self) -> bool { self.daemon_up.get() }`.
- The button is always sensitive for deck-owned actions and checks `daemon_up` on click, so
  it never goes stale when the daemon comes or goes while the editor is open.
- The page change or brightness comes back as `PageChanged` / `Event::Brightness`, which the
  window already follows.
- The settings brightness slider stays on `SetBrightness` (groundwork, D1).

## Tasks

Each task ends with `cargo fmt && cargo clippy --all-targets -- -D warnings` clean, plus the
check named.

### Task 1: IPC types, parsing and the client (~230 lines)

Files: `crates/omadeck-core/src/ipc.rs`.

- The `Command` variants, `DialInput`, `Reply`, `one`/`is_one`, `Command::to_line`,
  `wants_reply`, `Reply::to_line`/`ok`/`error`, `REPLY_TIMEOUT`, `MAX_LINE`,
  `parse_command`, `request`, `read_reply`, all as above.
- The `use` line in `omadeckd` and the GUI stays `ipc::Command` (or `Command as
  DaemonCommand` in `app.rs`): `std::process::Command` is in the same files.
- `Daemon::handle_commands`' `match cmd` on `ShowPage`/`SetBrightness` stops being
  exhaustive. Add `_ => {}` there for now; task 3 fills it in.

Tests (in `ipc::tests`):
- `command_roundtrip`: every variant, `Dial` with and without `steps`.
- `command_wire_shape` (below).
- `reply_roundtrip`: `Ok` with and without message, `Error`, `NoDeck`, `State` with `deck: None`.
- `reply_skips_events` (below).
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
        Command::Dial { index: 0, input: DialInput::Right, steps: 1 }.to_line(),
        "{\"command\":\"dial\",\"index\":0,\"input\":\"right\"}\n"
    );
    assert_eq!(Reply::ok("").to_line(), "{\"reply\":\"ok\"}\n");
    assert!(!Command::ShowPage { page: None }.wants_reply());
    assert!(Command::State.wants_reply());
}

#[test]
fn reply_skips_events() {
    let lines = "{\"event\":\"disconnected\"}\n{\"event\":\"page_changed\",\"page\":null}\n\
                 {\"reply\":\"error\",\"message\":\"no key 40; this deck has 8 keys\"}\n";
    let r = read_reply(lines.as_bytes()).unwrap();
    assert_eq!(r, Reply::error("no key 40; this deck has 8 keys"));
    assert!(read_reply("{\"event\":\"disconnected\"}\n".as_bytes()).is_err());
}
```

(`Action::PageStep { change: 1 }` writes no `change`: `skip_serializing_if = "is_default_step"`.)

Check: `cargo test -p omadeck-core ipc::` → `test result: ok`, with the 5 new tests listed
next to the existing event tests (`dial_turn_wire_shape` and the roundtrips) and 0 failed.

### Task 2: the hub answers (~150 lines)

Files: `crates/omadeckd/src/hub.rs`, `crates/omadeckd/src/daemon.rs`.

- `Request`, `ReplyTo`, `commands() -> TryIter<'_, Request>`, the capped reader, the
  `parse_command` path, `chmod 0600`.
- `daemon.rs`: `handle_commands` now gets `Request`s. For this task, keep `ShowPage` and
  `SetBrightness` as they are and answer anything else with
  `req.reply.send(&Reply::error("not yet"))`.

Test to copy, added to groundwork's `hub::tests` (which already has `socket(name)` and
imports `BufRead`, `BufReader`, `Duration`, `Instant`):

```rust
    use omadeck_core::ipc::Reply;
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn command_reply_over_socket() {
        let path = socket("reply");
        let hub = Hub::start(path.clone()).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);

        let mut client = UnixStream::connect(&path).unwrap();
        client.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        client.write_all(Command::State.to_line().as_bytes()).unwrap();

        // Play the deck loop: wait for the request and answer it.
        let deadline = Instant::now() + Duration::from_secs(3);
        let req = loop {
            if let Some(r) = hub.commands().next() {
                break r;
            }
            assert!(Instant::now() < deadline, "no request arrived");
            std::thread::sleep(Duration::from_millis(10));
        };
        assert_eq!(req.cmd, Command::State);
        let state = Reply::State { deck: None, page: None, pages: vec![], brightness: 40 };
        req.reply.send(&state);
        // The status and page replay come first; read_reply skips them.
        assert_eq!(ipc::read_reply(BufReader::new(client)).unwrap(), state);

        // An unknown command gets an error, not silence.
        let mut other = UnixStream::connect(&path).unwrap();
        other.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
        other.write_all(b"{\"command\":\"explode\"}\n").unwrap();
        assert!(matches!(ipc::read_reply(BufReader::new(other)).unwrap(), Reply::Error { .. }));

        hub.shutdown();
    }
```

Check: `cargo test -p omadeckd hub::` → `test hub::tests::command_reply_over_socket ... ok`.

### Task 3: the daemon handles commands (~250 lines)

Files: `crates/omadeckd/src/daemon.rs`, `deck.rs`, `main.rs`.

- `Source::Ctl` and its `Display`; `try_input` with `run_input` as its wrapper; nothing is
  published for `Ctl`.
- `key_down`, `turn_dial`, `tap_dial` out of `handle_update`, and `press_key -> Result<()>`;
  the arms call them.
- `deck.rs`: `Deck.key_flash`, `KEY_FLASH`, `flash_deadline`, `expire_key_flashes`; `pass`
  repaints and publishes `KeyUp` for the ended ones; hardware key events clear the flash.
- `reload() -> Result<()>`; `main.rs`'s debounced call logs the error.
- `handle_command` and the new `handle_commands`. Remove task 2's `"not yet"`.

There's no harness for a `Daemon` with a deck, so the test covers the one pure piece, the
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

Check: `cargo test -p omadeckd` → `test result: ok`, including `daemon::tests::run_messages`. Then by hand
(task 4 gives the CLI): a key, a dial turn, a dial press and a strip tap behave as before.

### Task 4: the CLI (~200 lines)

Files: `crates/omadeckd/src/main.rs`.

```rust
/// A control verb's arguments as the command to send. `Err` is the usage line.
fn parse_ctl(verb: &str, args: &[String]) -> Result<ipc::Command, &'static str>;
/// Send `cmd`, print the answer, and fall back to running press/dial here. The exit code.
fn ctl(cmd: ipc::Command) -> i32;
/// Today's offline `press` and `dial` bodies, moved out of `main` unchanged except for
/// `page` "home" meaning Home and `steps` going to `action::run_steps`.
fn press_locally(index: u8, page: Option<&str>) -> i32;
fn dial_locally(index: u8, input: ipc::DialInput, steps: u8) -> i32;
```

- `main`: `Some(verb @ ("page" | "brightness" | "reload" | "state" | "press" | "dial"))`
  collects the remaining args, calls `parse_ctl` (usage → `eprintln!("usage: omadeckd {u}")`,
  exit 2), then `std::process::exit(ctl(cmd))`.
- `ctl`: `Ok(Some(Reply::Ok { message }))` prints a non-empty message, 0.
  `Ok(Some(r @ Reply::State { .. }))` prints `r.to_line()` with `print!`, 0.
  `Ok(Some(Reply::Error { message }))` eprints, 1. `Ok(Some(NoDeck))` or `Ok(None)`: `Press`
  and `Dial` run locally; the rest eprint the "not running" line, 1. `Err(e)` eprints `{e:#}`, 1.
- The top-level usage line lists the new verbs.

Usage strings (returned by `parse_ctl`): `page next|prev|home|NAME`,
`brightness N|+N|-N`, `press KEY [PAGE]   (keys are numbered from 0)`,
`dial N left|right|press [STEPS]`, `reload`, `state`.

Tests (in `main.rs`; add a `#[cfg(test)] mod tests` with `use super::*;` if it has none):
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

### Task 5: Try it now, and docs (~150 lines)

Files: `crates/omadeck-gui/src/editor.rs`, `crates/omadeck-gui/src/app.rs`, `README.md`,
`site/index.html`, `CHANGELOG.md`.

- `test_button`'s deck-owned branch and `App::daemon_up`, as above.
- README "Command line": the new verbs, the Hyprland binds block, and "`press` and `dial` go
  through the daemon when it runs, so the key flashes and page and brightness keys work".
- README "How it works", **Event feed** becomes **Event feed and control**: commands and
  replies on the same socket, and the `state` JSON.
- `site/index.html`: next to `omadeckd press 3`, add `omadeckd page next`.
- CHANGELOG, Unreleased / Added: "`omadeckd page`, `brightness`, `reload` and `state` control
  the running daemon, e.g. from Hyprland binds. `press` and `dial` go through the daemon when
  it runs, so the key flashes and page and brightness keys work. *Try it now* works for Go
  back, Next/previous page and Brightness."

Check: `cargo build -p omadeck-gui` clean, and `cargo fmt && cargo clippy --all-targets -- -D warnings && cargo test`
→ every crate `test result: ok`.

## Known traps

- `ipc::Command` vs `std::process::Command`: both are used in `main.rs` and `app.rs`.
  Keep the enum qualified (`ipc::Command`) or aliased (`DaemonCommand`).
- Never write to a client stream from the loop. Replies go through `ReplyTo` (a
  `try_send` on the client's bounded channel), like events.
- `for req in self.hub.commands()` while calling `self.handle_command(..)` won't borrow-check.
  Collect first.
- `ShowPage`/`SetBrightness` never get a reply. If they did, every configurator would get
  a line per slider move that it skips anyway.
- A socket read timeout is `WouldBlock` on Linux, not `TimedOut`. Match both.
- `gio::spawn_blocking` for `ipc::request` in the GUI. The GTK thread must never wait on the
  socket (up to 3 s).
- Nothing new is slow on the loop: `handle_command` does what a key press does.

## Security

- The socket is in `$XDG_RUNTIME_DIR` (0700, the user's). `paths::socket_path` has no
  `/tmp` fallback. The socket itself becomes 0600.
- `Run` can run any action, scripts included. Anyone who can connect is the same user and
  can already run those from a shell, so this crosses no privilege boundary. No
  `SO_PEERCRED` check: `UnixStream::peer_cred` is still unstable in std.
- Lines are capped at 4 KiB and answered one at a time on the loop, so a bad client only
  slows itself.

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
- [ ] `omadeckd press 3` with the daemon up: key 3 flashes, the action runs, and the
      configurator selects the key. A page key pressed this way switches page.
- [ ] `omadeckd press 3` with the service stopped: runs locally as before.
- [ ] `omadeckd dial 0 right 5` moves the volume bar five steps; `dial 0 press` flashes the
      panel and mutes.
- [ ] `omadeckd reload` after a deliberate typo prints the parse error; the deck keeps the
      last good config.
- [ ] `omadeckd state | jq .page` with the deck plugged and unplugged.
- [ ] Configurator: *Try it now* on a Go back key and a brightness dial acts on the deck; with
      the service stopped it toasts.
- [ ] `ls -l $XDG_RUNTIME_DIR/omadeck.sock` shows `srw-------`.
- [ ] An old configurator build still shows status and key presses against the new daemon.

## Decisions to explain

- **No Elgato equivalent.** Elgato's CLI is for plugin development and its deep links go to
  plugins, so this follows omadeck's own CLI.
- **Flat verbs, not `omadeckd ctl …` or a new binary.** `press` and `dial` already live at
  the top level; a prefix would leave two ways to press a key.
- **One socket.** The hub already reads commands; replies are told apart from events by
  their tag. A ctl client briefly gets events, which it skips.
- **`Run { action }`** reuses the action shapes from the TOML and lets *Try it now* send
  unsaved edits.
- **`press`/`dial` go through the daemon when it's up**, so page and brightness keys work from
  a bind and a shared key/bind action looks the same on the deck. The offline fallback keeps
  today's behaviour for scripts.
- **Commands wait for the deck loop (≤100 ms)** instead of touching the deck from the client
  thread. The deck stays single-threaded.
- **No uid check.** The 0700 runtime dir and the 0600 socket are the guard.

## Ask before building

1. A ctl `press` selects the key in an open configurator, like a real press (decided). Keep
   that, or make ctl presses quiet (no `KeyDown`/`KeyUp`)?

## Later

- `omadeckd watch`: print events as JSON lines, for scripts and waybar.
- A waybar module from `state` (page and brightness).
- Editing commands (`set-key …`). Out of scope: the config file is the interface for edits.

**Size:** about 980 lines with tests: ipc ~230, hub ~150, daemon ~250, CLI ~200, GUI and
docs ~150.
