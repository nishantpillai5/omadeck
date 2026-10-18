"""Bug fixes from the final review, applied to every tree of the series where the buggy
code is (build.py). Each fix: the groups it applies to (0-based, inclusive), the file,
literal (old, new) rewrites applied wherever `old` is present, and `gone`: text that must
not be left in that file in any of those trees (the build stops if it is)."""

# PR 1: a dial's Open page / Go back ran through action::run, which refuses them.
DIAL_TURN = (0, 0, 'crates/omadeckd/src/main.rs', [('                            if let Some(d) = cfg.dial(i) {\n                                let (act, dir) =\n                                    if delta >= 0 { (&d.turn_right, "right") } else { (&d.turn_left, "left") };', '                            if let Some(dc) = cfg.dial(i) {\n                                let (act, dir) =\n                                    if delta >= 0 { (&dc.turn_right, "right") } else { (&dc.turn_left, "left") };'), ('                                    if let Err(e) = action::run_steps(act, steps) {\n                                        log::warn!("dial {i}: {e:#}");\n                                        hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });\n                                    }', '                                    match run_dial_action(&cfg, &mut nav, act, steps) {\n                                        Ok(true) => show_page(Some(d), &cfg, &nav, &mut renderer, &hub),\n                                        Ok(false) => {}\n                                        Err(e) => {\n                                            log::warn!("dial {i}: {e:#}");\n                                            hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });\n                                        }\n                                    }'), ('/// Repaint the deck after `nav` moved to another page, and tell the configurators.\nfn show_page(', '/// Run a dial\'s action. Page moves go to `nav` (`true`: the page changed), as for keys.\nfn run_dial_action(cfg: &Config, nav: &mut Nav, act: &Action, steps: u8) -> Result<bool> {\n    match act {\n        Action::Page { page } if cfg.has_page(page) => Ok(nav.go(Some(page))),\n        Action::Page { page } => Err(anyhow::anyhow!("there\'s no page named “{page}”")),\n        Action::Back => Ok(nav.back()),\n        a => action::run_steps(a, steps).map(|_| false),\n    }\n}\n\n/// Repaint the deck after `nav` moved to another page, and tell the configurators.\nfn show_page(')], ["action::run_steps(act, steps)"])
DIAL_PRESS = (0, 0, 'crates/omadeckd/src/main.rs', [('                                if let Err(e) = action::run(&dc.press) {\n                                    log::warn!("dial {i}: {e:#}");\n                                    hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });\n                                }', '                                match run_dial_action(&cfg, &mut nav, &dc.press, 1) {\n                                    Ok(true) => show_page(Some(d), &cfg, &nav, &mut renderer, &hub),\n                                    Ok(false) => {}\n                                    Err(e) => {\n                                        log::warn!("dial {i}: {e:#}");\n                                        hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });\n                                    }\n                                }'), ('                                    if let Err(e) = action::run(&dc.press) {\n                                        log::warn!("dial {i}: {e:#}");\n                                        hub.publish(Event::DialActionFailed {\n                                            index: i as u8,\n                                            message: format!("{e:#}"),\n                                        });\n                                    }', '                                    match run_dial_action(&cfg, &mut nav, &dc.press, 1) {\n                                        Ok(true) => show_page(Some(d), &cfg, &nav, &mut renderer, &hub),\n                                        Ok(false) => {}\n                                        Err(e) => {\n                                            log::warn!("dial {i}: {e:#}");\n                                            hub.publish(Event::DialActionFailed {\n                                                index: i as u8,\n                                                message: format!("{e:#}"),\n                                            });\n                                        }\n                                    }')], ["action::run(&dc.press)"])
DIAL_PRESS_OLD = (0, 0, 'crates/omadeckd/src/main.rs', [
    ("""                            if let Some(d) = cfg.dial(i)
                                && !d.press.is_none()
                            {
                                log::info!("dial {i}: {}", d.press.summary());
                                if let Err(e) = action::run(&d.press) {
                                    log::warn!("dial {i}: {e:#}");
                                    hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                }
                            }""",
     """                            if let Some(dc) = cfg.dial(i)
                                && !dc.press.is_none()
                            {
                                log::info!("dial {i}: {}", dc.press.summary());
                                match run_dial_action(&cfg, &mut nav, &dc.press, 1) {
                                    Ok(true) => show_page(Some(d), &cfg, &nav, &mut renderer, &hub),
                                    Ok(false) => {}
                                    Err(e) => {
                                        log::warn!("dial {i}: {e:#}");
                                        hub.publish(Event::DialActionFailed { index: i, message: format!("{e:#}") });
                                    }
                                }
                            }"""),
], ["action::run(&d.press)"])

# PR 2 on: no hard-coded monitor when the focused one is unknown.
MONITOR = (1, 14, "crates/omadeck-core/src/action.rs", [
    ('swayosd(monitor.unwrap_or("HDMI-A-1"), device, change, mute)', "swayosd(monitor, device, change, mute)"),
    ("""fn swayosd(monitor: &str, device: AudioDevice, change: i8, mute: bool) -> Vec<Vec<String>> {
    let which = if device.is_output() { "--output-volume" } else { "--input-volume" };
    let arg = if mute { "mute-toggle".to_string() } else { signed(change) };
    vec![vec!["swayosd-client".into(), "--monitor".into(), monitor.into(), which.into(), arg]]
}""",
     """fn swayosd(monitor: Option<&str>, device: AudioDevice, change: i8, mute: bool) -> Vec<Vec<String>> {
    let which = if device.is_output() { "--output-volume" } else { "--input-volume" };
    let arg = if mute { "mute-toggle".to_string() } else { signed(change) };
    let mut argv = vec!["swayosd-client".to_string()];
    if let Some(m) = monitor {
        argv.extend(["--monitor".to_string(), m.to_string()]);
    }
    argv.extend([which.to_string(), arg]);
    vec![argv]
}"""),
], ["HDMI-A-1"])

# PR 2: a brightness edited in config.toml while the daemon was stopped lost to the state
# file. The rule groundwork has, brought forward.
STATE_RULE = (1, 5, "crates/omadeck-core/src/state.rs", [
    ("""/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's
/// `brightness` is the start value; once the deck has run, this file wins.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
}

impl State {
""",
     """/// The daemon's state, in `$XDG_STATE_HOME/omadeck/state.toml`. The config's `brightness`
/// is the start value; once the deck has run, this file wins (a hand edit of the config's
/// value wins again).
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct State {
    /// Panel brightness, 0–100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<u8>,
    /// The config's `brightness` when the daemon last read it. A config value that
    /// differs from it was edited since (by hand, or by the configurator with no daemon
    /// running), and wins. Compared by value, not by file times: the configurator
    /// rewrites config.toml on every edit without touching `brightness`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config_brightness: Option<u8>,
}

impl State {
    /// The config's `brightness` when it was edited since the daemon last saw it, else
    /// `None`. A state file from before `config_brightness` existed has seen nothing, so
    /// its saved brightness still wins (as it did before).
    pub fn config_edit(&self, cfg: u8) -> Option<u8> {
        match self.config_brightness {
            Some(seen) if seen == cfg => None,
            None if self.brightness.is_some() => None,
            _ => Some(cfg),
        }
    }

    /// The brightness the deck starts at: a config edit, else the saved value, else the
    /// config's.
    pub fn start_brightness(&self, cfg: u8) -> u8 {
        self.config_edit(cfg).or(self.brightness).unwrap_or(cfg)
    }

"""),
    ("""        let s = State { brightness: Some(42) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }""",
     """        let s = State { brightness: Some(42), config_brightness: Some(70) };
        let toml = toml::to_string_pretty(&s).unwrap();
        assert_eq!(toml::from_str::<State>(&toml).unwrap(), s);
        assert_eq!(State::default(), State { brightness: None, config_brightness: None });
        assert!(toml::from_str::<State>("").unwrap().brightness.is_none());
    }

    #[test]
    fn a_config_brightness_edited_since_wins_at_start() {
        let s = State { brightness: Some(20), config_brightness: Some(70) };
        assert_eq!(s.start_brightness(70), 20);
        assert_eq!(s.start_brightness(35), 35);
        let old = State { brightness: Some(20), config_brightness: None };
        assert_eq!(old.start_brightness(70), 20);
        assert_eq!(State::default().start_brightness(70), 70);
    }"""),
], [])

STATE_USE = (1, 4, "crates/omadeckd/src/main.rs", [
    ("State::load().brightness.unwrap_or(cfg.brightness)", "State::load().start_brightness(cfg.brightness)"),
    ("use nav::Nav;\n",
     "use nav::Nav;\n\n/// The config's `brightness` as the daemon last read it, saved with the brightness.\nstatic CONFIG_BRIGHTNESS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);\n"),
    ("""fn save_brightness(v: u8) {
    if let Err(e) = (State { brightness: Some(v) }).save() {""",
     """fn save_brightness(v: u8) {
    let seen = CONFIG_BRIGHTNESS.load(std::sync::atomic::Ordering::Relaxed);
    if let Err(e) = (State { brightness: Some(v), config_brightness: Some(seen) }).save() {"""),
    ("""                Ok(new_cfg) => {
                    hand_edit = hand_edited_brightness(&cfg, &new_cfg);
                    cfg = new_cfg;""",
     """                Ok(new_cfg) => {
                    hand_edit = hand_edited_brightness(&cfg, &new_cfg);
                    CONFIG_BRIGHTNESS.store(new_cfg.brightness, std::sync::atomic::Ordering::Relaxed);
                    cfg = new_cfg;"""),
    ("""fn connect(hid: &mut HidApi, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Option<Deck> {
""",
     """fn connect(hid: &mut HidApi, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Option<Deck> {
    CONFIG_BRIGHTNESS.store(cfg.brightness, std::sync::atomic::Ordering::Relaxed);
"""),
], ["State::load().brightness.unwrap_or(cfg.brightness)"])

# PR 2: the audio worker woke every 50 ms forever; now it sleeps until a poll is due, or
# until a command while nothing is watched.
AUDIO_IDLE = (1, 4, "crates/omadeckd/src/main.rs", [
    ("""            loop {
                loop {
                    match cmd_rx.try_recv() {
                        Ok(AudioCmd::Watch(devs)) => {
                            watched = devs;
                            last.clear();
                            next_poll = Instant::now();
                        }
                        Ok(AudioCmd::Refresh(dev)) => {
                            // Drop the last known value so the next read is sent even
                            // when it equals what we already knew.
                            last.remove(&dev);
                            next_poll = Instant::now() + AUDIO_REFRESH_DELAY;
                        }
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let now = Instant::now();""",
     """            loop {
                let cmd = if watched.is_empty() {
                    cmd_rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    cmd_rx.recv_timeout(next_poll.saturating_duration_since(Instant::now()))
                };
                match cmd {
                    Ok(AudioCmd::Watch(devs)) => {
                        watched = devs;
                        last.clear();
                        next_poll = Instant::now();
                        continue;
                    }
                    Ok(AudioCmd::Refresh(dev)) => {
                        // Drop the last known value so the next read is sent even
                        // when it equals what we already knew.
                        last.remove(&dev);
                        next_poll = Instant::now() + AUDIO_REFRESH_DELAY;
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                let now = Instant::now();"""),
    ("""                    next_poll = now + AUDIO_POLL;
                }
                std::thread::sleep(Duration::from_millis(50));
            }""",
     """                    next_poll = now + AUDIO_POLL;
                }
            }"""),
], ["std::thread::sleep(Duration::from_millis(50));\n            }\n        })\n        .expect(\"spawn audio thread\")"])

# PR 2: the configurator read both volumes every second on any deck; now only while its
# strip preview shows a volume dial.
GUI_LEVELS = (1, 4, "crates/omadeck-gui/src/app.rs", [
    ("use crate::{editor, media};\n",
     "use crate::{editor, media};\n\n/// Whether the strip preview shows a volume level, so the level poller has work.\nstatic LEVELS_WANTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);\n"),
    ("""                loop {
                    let mut cur = Vec::new();""",
     """                loop {
                    if !LEVELS_WANTED.load(std::sync::atomic::Ordering::Relaxed) {
                        std::thread::sleep(Duration::from_secs(1));
                        continue;
                    }
                    let mut cur = Vec::new();"""),
    ("""        let cfg = self.config.borrow();
        let n = self.deck.borrow().dials as usize;
        let levels = self.strip_levels.borrow();""",
     """        let cfg = self.config.borrow();
        let n = self.deck.borrow().dials as usize;
        let shows_volume = (0..n).any(|i| cfg.dial(i as u8).and_then(|d| d.volume_device()).is_some());
        LEVELS_WANTED.store(shows_volume, std::sync::atomic::Ordering::Relaxed);
        let levels = self.strip_levels.borrow();"""),
], ["                loop {\n                    let mut cur = Vec::new();"])

AUDIO_IDLE_OLD = (1, 4, "crates/omadeckd/src/main.rs", [
    ("""            loop {
                loop {
                    match cmd_rx.try_recv() {
                        Ok(AudioCmd::Watch(devs)) => {
                            watched = devs;
                            last.clear();
                            next_poll = Instant::now();
                        }
                        Ok(AudioCmd::Refresh) => next_poll = Instant::now() + AUDIO_REFRESH_DELAY,
                        Err(mpsc::TryRecvError::Empty) => break,
                        Err(mpsc::TryRecvError::Disconnected) => return,
                    }
                }
                let now = Instant::now();""",
     """            loop {
                let cmd = if watched.is_empty() {
                    cmd_rx.recv().map_err(|_| mpsc::RecvTimeoutError::Disconnected)
                } else {
                    cmd_rx.recv_timeout(next_poll.saturating_duration_since(Instant::now()))
                };
                match cmd {
                    Ok(AudioCmd::Watch(devs)) => {
                        watched = devs;
                        last.clear();
                        next_poll = Instant::now();
                        continue;
                    }
                    Ok(AudioCmd::Refresh) => {
                        next_poll = Instant::now() + AUDIO_REFRESH_DELAY;
                        continue;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => return,
                }
                let now = Instant::now();"""),
    ("""                    next_poll = now + AUDIO_POLL;
                }
                std::thread::sleep(Duration::from_millis(50));
            }""",
     """                    next_poll = now + AUDIO_POLL;
                }
            }"""),
], [])

STATE_USE_OLD = (1, 4, "crates/omadeckd/src/main.rs", [
    ("use nav::Nav;\n",
     "use nav::Nav;\n\n/// The config's `brightness` as the daemon last read it, saved with the brightness.\nstatic CONFIG_BRIGHTNESS: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);\n"),
    ("State::load().brightness.unwrap_or(cfg.brightness)", "State::load().start_brightness(cfg.brightness)"),
    ("""        if let Err(e) = (State { brightness: Some(v) }).save() {""",
     """        let seen = CONFIG_BRIGHTNESS.load(std::sync::atomic::Ordering::Relaxed);
        if let Err(e) = (State { brightness: Some(v), config_brightness: Some(seen) }).save() {"""),
    ("""                Ok(new_cfg) => cfg = new_cfg,""",
     """                Ok(new_cfg) => {
                    CONFIG_BRIGHTNESS.store(new_cfg.brightness, std::sync::atomic::Ordering::Relaxed);
                    cfg = new_cfg;
                }"""),
    ("""fn connect(hid: &mut HidApi, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Option<Deck> {
""",
     """fn connect(hid: &mut HidApi, cfg: &Config, page: Option<&str>, renderer: &mut Renderer) -> Option<Deck> {
    CONFIG_BRIGHTNESS.store(cfg.brightness, std::sync::atomic::Ordering::Relaxed);
"""),
], [])

# PR 5 on: a live command's run is bounded by 10 s, not by its interval.
LIVE_TIMEOUT = (7, 14, "crates/omadeckd/src/watch.rs", [
    ("""                // At least 1 s (the interval's floor), at most 10 s.
                Probe::Output { command, .. } => (command.clone(), probe.interval().min(LIVE_COMMAND_TIMEOUT)),""",
     """                Probe::Output { command, .. } => (command.clone(), LIVE_COMMAND_TIMEOUT),"""),
], ["probe.interval().min(LIVE_COMMAND_TIMEOUT)"])

# PR 6–8: a retitle of the focused window re-read pactl (PR 9 has this already).
RETITLE = (8, 11, "crates/omadeckd/src/daemon.rs", [
    ("""                HyprEvent::ActiveWindow(w) => {
                    // Always, nothing focused included: the watcher only remembers it
                    // when no focused dial is watched, and a focused dial with no window
                    // falls back to its sticky app.
                    let focus = w.as_ref().map(|w| audio::Window { pid: w.pid, class: w.class.clone() });
                    let _ = self.watcher.cmd.send(WatchCmd::Focus(focus));
                    self.rt.window = w;
                }""",
     """                HyprEvent::ActiveWindow(w) => {
                    // A retitle of the focused window comes as an `ActiveWindow` too: only
                    // another window is news (each would re-read `pactl`).
                    if self.rt.window.as_ref().map(window_identity) != w.as_ref().map(window_identity) {
                        // Nothing focused included: the watcher only remembers it when no
                        // focused dial is watched, and a focused dial with no window falls
                        // back to its sticky app.
                        let focus = w.as_ref().map(|w| audio::Window { pid: w.pid, class: w.class.clone() });
                        let _ = self.watcher.cmd.send(WatchCmd::Focus(focus));
                    }
                    self.rt.window = w;
                }"""),
    ("""        self.watcher.stop();
        self.saver.finish();
    }
}
""",
     """        self.watcher.stop();
        self.saver.finish();
    }
}

/// What makes a focused window another one: a retitle keeps all three.
fn window_identity(w: &hypr::ActiveWindow) -> (&str, &str, Option<u32>) {
    (&w.address, &w.class, w.pid)
}
"""),
], ["// Always, nothing focused included: the watcher only remembers it"])

# PR 7 on: `omadeckd press N` with no page is the shown page, as documented.
PRESS_PAGE = (9, 14, "crates/omadeckd/src/daemon.rs", [
    ("""                let page = match page {
                    Some(p) if p.eq_ignore_ascii_case(omadeck_core::HOME) => None,
                    Some(p) if !self.cfg.has_page(&p) => {
                        return Reply::error(format!("there's no page named “{p}”"));
                    }
                    page => page,
                };
                let shown = self.nav.current().map(str::to_string);""",
     """                let shown = self.nav.current().map(str::to_string);
                let page = match page {
                    None => shown.clone(),
                    Some(p) if p.eq_ignore_ascii_case(omadeck_core::HOME) => None,
                    Some(p) if !self.cfg.has_page(&p) => {
                        return Reply::error(format!("there's no page named “{p}”"));
                    }
                    page => page,
                };"""),
], ["                    page => page,\n                };\n                let shown = self.nav.current()"])

# PR 7 on: a client that closes its write side still gets the replies it is owed.
REPLY_HUB = (9, 14, "crates/omadeckd/src/hub.rs", [
    ("use std::sync::mpsc::{self, SyncSender, TrySendError};\n",
     "use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};\nuse std::sync::mpsc::{self, SyncSender, TrySendError};\n"),
    ("""pub struct ReplyTo {
    queue: SyncSender<Arc<str>>,
    stream: Arc<UnixStream>,
}""",
     """pub struct ReplyTo {
    queue: SyncSender<Arc<str>>,
    stream: Arc<UnixStream>,
    /// Commands handed to the loop and not answered yet, and whether the client hung up:
    /// its writer ends after the last answer.
    pending: Arc<(AtomicUsize, AtomicBool)>,
}"""),
    ("""                let _ = self.stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }
}""",
     """                let _ = self.stream.shutdown(std::net::Shutdown::Both);
            }
        }
    }

    /// The loop's answer to one of this client's commands.
    pub fn answer(&self, r: &Reply) {
        self.send(r);
        self.done();
    }

    /// One of this client's commands was handled, answered or not. After the last one, a
    /// client that hung up has its writer ended.
    pub fn done(&self) {
        if self.pending.0.fetch_sub(1, Ordering::SeqCst) == 1 && self.pending.1.load(Ordering::SeqCst) {
            self.end();
        }
    }

    /// End the writer once what is queued is written.
    fn end(&self) {
        let _ = self.queue.try_send(Arc::from(""));
    }
}"""),
    ("""    let _ = reply.queue.try_send(Arc::from(""));
}""",
     """    reply.pending.1.store(true, Ordering::SeqCst);
    if reply.pending.0.load(Ordering::SeqCst) == 0 {
        reply.end();
    }
}"""),
    ("""            Ok(cmd) => {
                if tx.send(Request { cmd, reply: reply.clone() }).is_err() {""",
     """            Ok(cmd) => {
                reply.pending.0.fetch_add(1, Ordering::SeqCst);
                if tx.send(Request { cmd, reply: reply.clone() }).is_err() {"""),
    ("ReplyTo { queue: queue.clone(), stream: Arc::new(closer) }",
     "ReplyTo { queue: queue.clone(), stream: Arc::new(closer), pending: Arc::default() }"),
    ("ReplyTo { queue, stream: Arc::new(ours) }", "ReplyTo { queue, stream: Arc::new(ours), pending: Arc::default() }"),
    ("ReplyTo { queue: queue.clone(), stream: Arc::new(ours.try_clone().unwrap()) }",
     "ReplyTo { queue: queue.clone(), stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() }"),
    ("ReplyTo { queue, stream: Arc::new(ours.try_clone().unwrap()) }",
     "ReplyTo { queue, stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() }"),
    ("""            assert!(start.elapsed() < Duration::from_secs(2), "the writer is still alive");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
""",
     """            assert!(start.elapsed() < Duration::from_secs(2), "the writer is still alive");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn a_client_that_closes_its_write_side_still_gets_its_reply() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let (queue, lines) = mpsc::sync_channel::<Arc<str>>(CLIENT_QUEUE);
        let reply = ReplyTo { queue, stream: Arc::new(ours.try_clone().unwrap()), pending: Arc::default() };
        let (tx, rx) = mpsc::channel();
        let reader = ours.try_clone().unwrap();
        let read = std::thread::spawn(move || read_requests(reader, reply, tx));
        spawn_writer(ours, lines);
        std::io::Write::write_all(&mut &theirs, b"{\\"command\\":\\"state\\"}\\n").unwrap();
        theirs.shutdown(std::net::Shutdown::Write).unwrap();
        let req: Request = rx.recv_timeout(Duration::from_secs(2)).unwrap();
        read.join().unwrap();
        req.reply.answer(&Reply::ok("answered"));
        let mut line = String::new();
        BufReader::new(&theirs).read_line(&mut line).unwrap();
        assert!(line.contains("answered"), "{line:?}");
    }
"""),
], ["    let _ = reply.queue.try_send(Arc::from(\"\"));\n}"])

REPLY_DAEMON = (9, 14, "crates/omadeckd/src/daemon.rs", [
    ("req.reply.send(&r);", "req.reply.answer(&r);"),
    ("""                Command::ShowPage { page } => show = Some(page),
                Command::SetBrightness { value } => bright = Some(value),""",
     """                Command::ShowPage { page } => {
                    show = Some(page);
                    req.reply.done();
                }
                Command::SetBrightness { value } => {
                    bright = Some(value);
                    req.reply.done();
                }"""),
], ["req.reply.send(&r);"])

# PR 7 on: a delay is capped at the configurator's limit, so a typo can't hold a key.
DELAY_CORE = (9, 14, "crates/omadeck-core/src/config.rs", [
    ("""/// `Wait 500 ms` below a second;""",
     """/// The longest a `delay` step waits (the configurator's limit too).
pub const MAX_DELAY_MS: u32 = 600_000;

/// `Wait 500 ms` below a second;"""),
], [])
DELAY_BLOCKING = (9, 14, "crates/omadeck-core/src/action.rs", [
    ("thread::sleep(Duration::from_millis(u64::from(*ms)));",
     "thread::sleep(Duration::from_millis(u64::from((*ms).min(crate::config::MAX_DELAY_MS))));"),
], ["thread::sleep(Duration::from_millis(u64::from(*ms)));"])
DELAY_SEQ = (9, 14, "crates/omadeckd/src/sequence.rs", [
    ("Action::Delay { ms } => run.due += Duration::from_millis(u64::from(*ms)),",
     "Action::Delay { ms } => {\n                        run.due += Duration::from_millis(u64::from((*ms).min(omadeck_core::config::MAX_DELAY_MS)))\n                    }"),
], ["run.due += Duration::from_millis(u64::from(*ms)),"])
DELAY_GUI = (9, 14, "crates/omadeck-gui/src/editor.rs", [
    ("adw::SpinRow::with_range(0.0, 600_000.0, 100.0)",
     "adw::SpinRow::with_range(0.0, f64::from(omadeck_core::config::MAX_DELAY_MS), 100.0)"),
], ["adw::SpinRow::with_range(0.0, 600_000.0, 100.0)"])

STATE_USE_GW = (5, 5, STATE_USE[2], STATE_USE[3], [])

AUDIO_IDLE_GW = (5, 5, AUDIO_IDLE[2], AUDIO_IDLE[3], [])
GUI_LEVELS_GW = (5, 5, GUI_LEVELS[2], GUI_LEVELS[3], [])

# PR 4 (and PR 5's commits before the fix lands): the progress bar takes the first NN%.
BUSY_BAR_DOC = (6, 7, "crates/omadeck-core/src/config.rs", [
    ("(the last `NN%`).", "(the first `NN%`)."),
], ["(the last `NN%`)."])

# Control commands on: a press the key would ignore (still running) is an error to ctl,
# not "ok". With multi actions, a running multi counts too, and an ignored ctl multi
# says so (the deck's own inputs stay quiet).
PRESS_BUSY_CTL = (9, 14, "crates/omadeckd/src/daemon.rs", [
    ("""                let res = self.key_down(index, false);""",
     """                if self.rt.jobs.contains_key(&KeyId { page: shown.clone(), index }) {
                    return Reply::error(format!("key {index} is still running"));
                }
                let res = self.key_down(index, false);"""),
], [])
PRESS_BUSY_MULTI = (10, 14, "crates/omadeckd/src/daemon.rs", [
    ("""                if self.rt.jobs.contains_key(&KeyId { page: shown.clone(), index }) {
                    return Reply::error(format!("key {index} is still running"));
                }""",
     """                let id = KeyId { page: shown.clone(), index };
                if self.rt.jobs.contains_key(&id) || self.seq.is_running(&RunOf::Key(id)) {
                    return Reply::error(format!("key {index} is still running"));
                }"""),
    ("""        if !self.seq.start(of.clone(), steps.to_vec(), now) {
            log::info!("{of}: multi action still running; ignored");
            return Ok(());
        }""",
     """        if !self.seq.start(of.clone(), steps.to_vec(), now) {
            log::info!("{of}: multi action still running; ignored");
            if of == RunOf::Ctl {
                anyhow::bail!("a multi action from omadeckd is still running");
            }
            return Ok(());
        }"""),
    ("""    /// once). Ignored, and Ok, while that input's last run is still going.""",
     """    /// once). Ignored while that input's last run is still going (an error for omadeckd,
    /// which is answered)."""),
], ["            log::info!(\"{of}: multi action still running; ignored\");\n            return Ok(());"])

# Media dial on: one next/previous per turn event, not one per detent queued on the
# watcher behind the volume reads.
MEDIA_ONE = (11, 14, "crates/omadeckd/src/daemon.rs", [
    ("""                // Next/previous on a turn: once per detent (steps is already capped at 8).
                let times =
                    if seek == 0 && matches!(command, MediaCommand::Next | MediaCommand::Previous) { steps } else { 1 };
                for _ in 0..times {
                    let _ = self.watcher.cmd.send(WatchCmd::Media {
                        command: *command,
                        seek,
                        target: target.clone(),
                        probe: probe.clone(),
                    });
                }""",
     """                // One command per input: a spun dial skips one track, not a queue of them.
                let _ = self.watcher.cmd.send(WatchCmd::Media { command: *command, seek, target, probe });"""),
], ["let times =\n                    if seek == 0 && matches!(command, MediaCommand::Next"])

FIXES = [PRESS_BUSY_CTL, PRESS_BUSY_MULTI, MEDIA_ONE, BUSY_BAR_DOC, STATE_USE_GW, AUDIO_IDLE_GW, GUI_LEVELS_GW, DIAL_TURN, DIAL_PRESS, DIAL_PRESS_OLD, MONITOR, STATE_RULE, STATE_USE_OLD, STATE_USE, AUDIO_IDLE_OLD, AUDIO_IDLE, GUI_LEVELS, LIVE_TIMEOUT, RETITLE, PRESS_PAGE,
         REPLY_HUB, REPLY_DAEMON, DELAY_CORE, DELAY_BLOCKING, DELAY_SEQ, DELAY_GUI]
