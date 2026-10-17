//! Executes key actions. Everything is spawned detached so a slow app never blocks the deck.

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;


use anyhow::{Context, Result, bail};

use crate::audio;
use crate::config::{Action, AudioDevice};
use crate::{paths, proc};

pub fn run(action: &Action) -> Result<()> {
    if action.is_deck_owned() {
        bail!("{} only works on the deck", Action::KINDS[action.kind_index()]);
    }
    match action {
        Action::None => Ok(()),
        Action::App { command } => launch_app(command),
        Action::Script { path, terminal } => run_script(path, *terminal),
        Action::Sound { file, volume } => play_sound(&paths::resolve(file), *volume),
        Action::Url { url } => open_url(url),
        Action::Hyprland { dispatch } => hypr_dispatch(dispatch),
        Action::Volume { device, change, mute } => run_volume(*device, *change, *mute),
        // Refused above; the daemon runs these itself.
        Action::Page { .. } | Action::Back | Action::PageStep { .. } | Action::Brightness { .. } => Ok(()),
    }
}

/// Run `action` `steps` times, once per detent for dial turns. The daemon caps `steps`
/// at 8 per event, so a hard spin can't fork dozens of processes at once.
pub fn run_steps(action: &Action, steps: u8) -> Result<()> {
    // A fast spin folds into one volume command instead of `steps` processes: Omarchy 4's
    // script ignores a second mute within 250 ms, and one command is snappier.
    if let Action::Volume { device, change, mute } = action {
        return run_volume(*device, if *mute { 0 } else { folded_volume(*change, steps) }, *mute);
    }
    // `hyprctl dispatch` waits for Hyprland's answer. The daemon never gets here with a
    // Hyprland action (it queues it on its ordered `HyprWorker`); for the other callers
    // (`omadeckd dial` with no daemon) more than one step runs in order on a helper
    // thread, and a failure there is logged. One step stays inline so its error is shown.
    if let Action::Hyprland { dispatch } = action
        && steps > 1
    {
        if dispatch.trim().is_empty() {
            bail!("no dispatcher set");
        }
        let dispatch = dispatch.clone();
        thread::spawn(move || {
            for _ in 0..steps {
                if let Err(e) = hypr_dispatch(&dispatch) {
                    log::warn!("hyprctl dispatch {dispatch}: {e:#}");
                    break;
                }
            }
        });
        return Ok(());
    }
    for _ in 0..steps {
        run(action)?;
    }
    Ok(())
}

/// The volume change `steps` detents of `change` make, as one command: clamped to ±100.
pub fn folded_volume(change: i8, steps: u8) -> i8 {
    (change as i32 * steps as i32).clamp(-100, 100) as i8
}

/// Change the system volume without waiting for it (`omadeckd press`, the configurator's
/// "Try it now"). The daemon calls [`apply_volume`] on its watcher thread instead.
fn run_volume(device: AudioDevice, change: i8, mute: bool) -> Result<()> {
    thread::Builder::new().name("volume".into()).spawn(move || apply_volume(device, change, mute))?;
    Ok(())
}

/// Change the system volume and wait until it's done. It goes through Omarchy's own
/// commands when they're installed, so its on-screen pop-up shows just like the volume
/// keys; otherwise it falls back to `wpctl`. Blocking: never call it on the deck loop.
pub fn apply_volume(device: AudioDevice, change: i8, mute: bool) {
    let backend = audio::backend();
    // Omarchy 3's pop-up needs the focused monitor, which costs a `hyprctl` run.
    let monitor = (backend == audio::Backend::Omarchy3).then(audio::focused_monitor).flatten();
    let cmds = volume_command(backend, device, change, mute, monitor.as_deref());
    log::info!("exec {}", cmds.iter().map(|a| format!("{a:?}")).collect::<Vec<_>>().join(" ; "));
    for argv in cmds {
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]).process_group(0);
        if proc::output_timeout(&mut c, proc::COMMAND_TIMEOUT).is_none() {
            log::warn!("{argv:?} failed to start or didn't finish");
        }
    }
    // Omarchy 4 has no command to change the mic's volume, so show its pop-up ourselves.
    // The change above has finished, so this reads the new level.
    if backend == audio::Backend::Omarchy4
        && device == AudioDevice::Input
        && !mute
        && let Some(lv) = audio::volume(AudioDevice::Input)
    {
        let mut c = Command::new("omarchy-osd");
        c.args(["-i", "microphone", "-p", &lv.percent.to_string()]);
        let _ = spawn(c);
    }
}

/// The command(s) that apply a volume change, run in order. Pure, so every backend's
/// table row can be tested without spawning anything. `monitor` is the focused monitor,
/// used only by Omarchy 3's `swayosd-client --monitor`.
pub fn volume_command(
    backend: audio::Backend,
    device: AudioDevice,
    change: i8,
    mute: bool,
    monitor: Option<&str>,
) -> Vec<Vec<String>> {
    match backend {
        audio::Backend::Omarchy4 => omarchy4(device, change, mute),
        audio::Backend::Omarchy3 => swayosd(monitor, device, change, mute),
        audio::Backend::Plain => wpctl(device, change, mute),
    }
}

fn omarchy4(device: AudioDevice, change: i8, mute: bool) -> Vec<Vec<String>> {
    if device.is_output() {
        let arg = if mute { "mute-toggle".into() } else { signed(change) };
        vec![vec!["omarchy-audio-output-volume".into(), arg]]
    } else if mute {
        vec![vec!["omarchy-audio-input-mute".into()]]
    } else {
        // No Omarchy command for mic volume; `wpctl`, capped at 100% (the output command
        // clamps itself, but a bare `wpctl set-volume` would let a source go over 100% on
        // a fast spin). The pop-up is added by run_volume.
        vec![vec![
            "wpctl".into(),
            "set-volume".into(),
            "-l".into(),
            "1.0".into(),
            "@DEFAULT_AUDIO_SOURCE@".into(),
            wpctl_rel(change),
        ]]
    }
}

fn swayosd(monitor: Option<&str>, device: AudioDevice, change: i8, mute: bool) -> Vec<Vec<String>> {
    let which = if device.is_output() { "--output-volume" } else { "--input-volume" };
    let arg = if mute { "mute-toggle".to_string() } else { signed(change) };
    let mut argv = vec!["swayosd-client".to_string()];
    if let Some(m) = monitor {
        argv.extend(["--monitor".to_string(), m.to_string()]);
    }
    argv.extend([which.to_string(), arg]);
    vec![argv]
}

fn wpctl(device: AudioDevice, change: i8, mute: bool) -> Vec<Vec<String>> {
    let target = if device.is_output() { "@DEFAULT_AUDIO_SINK@" } else { "@DEFAULT_AUDIO_SOURCE@" };
    if mute {
        vec![vec!["wpctl".into(), "set-mute".into(), target.into(), "toggle".into()]]
    } else {
        vec![vec!["wpctl".into(), "set-volume".into(), "-l".into(), "1.0".into(), target.into(), wpctl_rel(change)]]
    }
}

/// A signed percent step, `+5` / `-5`, as Omarchy's and swayosd's scripts take it.
fn signed(n: i8) -> String {
    if n >= 0 { format!("+{n}") } else { n.to_string() }
}

/// A relative percent step, `5%+` / `5%-`, as `wpctl set-volume` takes it.
/// `unsigned_abs` so a hand-edited `-128` can't panic on `i8::abs`.
fn wpctl_rel(n: i8) -> String {
    format!("{}%{}", n.unsigned_abs(), if n >= 0 { '+' } else { '-' })
}

pub(crate) fn which(bin: &str) -> bool {
    std::env::var_os("PATH").map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file())).unwrap_or(false)
}

/// Omarchy launches apps through uwsm so they get their own systemd scope.
fn has_uwsm() -> bool {
    static HAS: OnceLock<bool> = OnceLock::new();
    *HAS.get_or_init(|| which("uwsm-app"))
}

/// A command that outlives the daemon. Anything started directly would stay in
/// omadeckd's cgroup, and systemd kills that whole cgroup when the service stops or
/// restarts — taking browsers, terminals and scripts with it. So run it through
/// uwsm-app (as Omarchy does) or, failing that, in a transient systemd scope.
fn detached<S: AsRef<std::ffi::OsStr>>(argv: &[S]) -> Command {
    static SCOPE: OnceLock<bool> = OnceLock::new();
    let mut c = if has_uwsm() {
        let mut c = Command::new("uwsm-app");
        c.arg("--");
        c
    } else if *SCOPE.get_or_init(|| which("systemd-run")) {
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--scope", "--collect", "--quiet", "--"]);
        c
    } else {
        let mut c = Command::new(&argv[0]);
        c.args(&argv[1..]);
        return c;
    };
    c.args(argv);
    c
}

/// Starts `action` as a job the daemon watches (a `busy_while_running` key). Only for
/// `is_trackable()` kinds. Not through `uwsm-app`: it hands the command to systemd and
/// returns at once, so there would be nothing to wait for. Instead the job gets its own
/// scope (`systemd-run --scope` execs its command, so the child *is* the job), so
/// stopping the daemon doesn't kill it; with no user bus it runs directly. SIGPIPE is
/// ignored so the job survives the
/// daemon (its stdout's reader) going away.
pub fn run_tracked(action: &Action) -> Result<proc::Job> {
    let (argv, dir): (Vec<String>, Option<PathBuf>) = match action {
        Action::App { command } => {
            let command = command.trim();
            if command.is_empty() {
                bail!("no application set");
            }
            let argv = if needs_shell(command) {
                vec!["sh".into(), "-c".into(), command.into()]
            } else {
                shell_words::split(command).context("could not parse command")?
            };
            (argv, std::env::var_os("HOME").map(PathBuf::from))
        }
        Action::Script { path, terminal } => {
            let p = paths::resolve(path.trim());
            if !p.exists() {
                bail!("script not found: {}", p.display());
            }
            let executable = p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false);
            let mut argv: Vec<String> = Vec::new();
            if !executable {
                argv.push("sh".into());
            }
            argv.push(p.to_string_lossy().into_owned());
            if *terminal {
                let term: &[&str] =
                    if which("xdg-terminal-exec") { &["xdg-terminal-exec"] } else { &["alacritty", "-e"] };
                argv.splice(0..0, term.iter().map(|s| s.to_string()));
            }
            (argv, p.parent().map(PathBuf::from))
        }
        _ => bail!("this action can't be watched while it runs"),
    };
    static HAS_SCOPE: OnceLock<bool> = OnceLock::new();
    let argv = tracked_argv(&argv, *HAS_SCOPE.get_or_init(|| which("systemd-run") && user_bus()));
    let mut c = Command::new(&argv[0]);
    c.args(&argv[1..]);
    if let Some(dir) = dir {
        c.current_dir(dir);
    }
    let job = proc::Job::spawn(c).with_context(|| format!("failed to start {argv:?}"))?;
    log::info!("tracked: {argv:?}");
    Ok(job)
}

/// Whether the user bus is up: `systemd-run --user` fails without it (a CI runner or a
/// hand-started shell has the binary but no bus), so a scope needs both.
fn user_bus() -> bool {
    std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|dir| Path::new(&dir).join("bus").exists())
}

/// The argv `run_tracked` runs. Pure for tests. `scope`: systemd-run is on PATH and the
/// user bus is up.
pub fn tracked_argv(argv: &[String], scope: bool) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if scope {
        out.extend(["systemd-run", "--user", "--scope", "--collect", "--quiet", "--"].map(String::from));
    }
    out.extend(["sh", "-c", "trap '' PIPE; exec \"$@\"", "sh"].map(String::from));
    out.extend(argv.iter().cloned());
    out
}

/// Spawn in its own process group with no stdio, reaping it on a helper thread.
fn spawn(mut cmd: Command) -> Result<()> {
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).process_group(0);
    let desc = format!("{cmd:?}");
    log::info!("exec {desc}");
    let mut child = cmd.spawn().with_context(|| format!("failed to start {desc}"))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn needs_shell(s: &str) -> bool {
    s.chars().any(|c| "|&;<>()$`*?[]{}~!#".contains(c))
}

fn launch_app(command: &str) -> Result<()> {
    let command = command.trim();
    if command.is_empty() {
        bail!("no application set");
    }
    // Desktop entry id, e.g. "org.gnome.Nautilus.desktop".
    if command.ends_with(".desktop") && !command.contains(char::is_whitespace) {
        if has_uwsm() {
            return spawn(detached(&[command]));
        }
        return spawn(detached(&["gtk-launch", command.trim_end_matches(".desktop")]));
    }
    let argv: Vec<String> = if needs_shell(command) {
        vec!["sh".into(), "-c".into(), command.into()]
    } else {
        shell_words::split(command).context("could not parse command")?
    };
    let mut c = detached(&argv);
    c.current_dir(std::env::var_os("HOME").unwrap_or_else(|| "/".into()));
    spawn(c)
}

fn run_script(path: &str, terminal: bool) -> Result<()> {
    let p = paths::resolve(path.trim());
    if !p.exists() {
        bail!("script not found: {}", p.display());
    }
    let executable = p.metadata().map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false);
    let mut argv: Vec<String> = Vec::new();
    if !executable {
        argv.push("sh".into());
    }
    argv.push(p.to_string_lossy().into_owned());

    if terminal {
        let term: &[&str] = if which("xdg-terminal-exec") { &["xdg-terminal-exec"] } else { &["alacritty", "-e"] };
        argv.splice(0..0, term.iter().map(|s| s.to_string()));
    }
    let mut c = detached(&argv);
    if let Some(dir) = p.parent() {
        c.current_dir(dir);
    }
    spawn(c)
}

fn play_sound(file: &Path, volume: Option<f32>) -> Result<()> {
    if !file.is_file() {
        bail!("sound not found: {}", file.display());
    }
    let vol = volume.map(|v| v.clamp(0.0, 1.5));
    let c = if which("pw-play") {
        let mut c = Command::new("pw-play");
        if let Some(v) = vol {
            c.arg(format!("--volume={v}"));
        }
        c.arg(file);
        c
    } else if which("paplay") {
        let mut c = Command::new("paplay");
        if let Some(v) = vol {
            c.arg(format!("--volume={}", (v * 65536.0) as u32));
        }
        c.arg(file);
        c
    } else {
        let mut c = Command::new("aplay");
        c.arg(file);
        c
    };
    spawn(c)
}

pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    if url.contains("://") || url.starts_with("mailto:") { url.to_string() } else { format!("https://{url}") }
}

fn open_url(url: &str) -> Result<()> {
    if url.trim().is_empty() {
        bail!("no URL set");
    }
    spawn(detached(&["xdg-open".to_string(), normalize_url(url)]))
}

/// Run one Hyprland dispatcher and wait for Hyprland's answer (up to `COMMAND_TIMEOUT`,
/// twice on a Lua-config Hyprland). Blocking: the daemon runs it on its Hyprland worker
/// (`omadeckd`'s `HyprWorker`), never on the deck loop.
pub fn hypr_dispatch(dispatch: &str) -> Result<()> {
    let d = dispatch.trim();
    if d.is_empty() {
        bail!("no dispatcher set");
    }
    // Lua dispatcher (Hyprland 0.55+ with a Lua config), e.g. hl.dsp.focus({ workspace = "3" }).
    if d.contains('(') {
        return hyprctl_dispatch(&[d.to_string()]);
    }
    // Classic syntax, e.g. `workspace 3`. Lua-config Hyprland rejects it, so translate.
    let args = shell_words::split(d).context("could not parse dispatcher")?;
    match hyprctl_dispatch(&args) {
        Ok(()) => Ok(()),
        Err(e) if e.to_string().to_lowercase().contains("lua") || e.to_string().contains("hl.") => {
            match legacy_to_lua(d) {
                Some(lua) => hyprctl_dispatch(&[lua]),
                None => bail!(
                    "this Hyprland expects Lua dispatchers, e.g. hl.dsp.focus({{ workspace = \"3\" }}) — couldn't translate `{d}`"
                ),
            }
        }
        Err(e) => Err(e),
    }
}

/// Run `hyprctl dispatch …` and wait for its answer (it replies "ok" in a few ms).
fn hyprctl_dispatch(args: &[String]) -> Result<()> {
    log::info!("exec hyprctl dispatch {}", args.join(" "));
    let mut c = Command::new("hyprctl");
    c.arg("dispatch").args(args);
    let out = proc::output_timeout(&mut c, proc::COMMAND_TIMEOUT)
        .ok_or_else(|| anyhow::anyhow!("hyprctl didn't start or didn't answer"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    let text = text.trim();
    if out.status.success() && (text.is_empty() || text == "ok") {
        Ok(())
    } else {
        let msg: Vec<&str> = text.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        bail!("hyprctl: {}", if msg.is_empty() { "failed".to_string() } else { msg.join(" ") })
    }
}

fn lua_str(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Translate common classic dispatchers to Hyprland's Lua API.
pub fn legacy_to_lua(dispatch: &str) -> Option<String> {
    let d = dispatch.trim();
    let (name, arg) = d.split_once(char::is_whitespace).map(|(n, a)| (n, a.trim())).unwrap_or((d, ""));
    let lua = match (name, arg) {
        ("workspace", ws) if !ws.is_empty() => format!("hl.dsp.focus({{ workspace = {} }})", lua_str(ws)),
        ("movetoworkspace", ws) if !ws.is_empty() => format!("hl.dsp.window.move({{ workspace = {} }})", lua_str(ws)),
        ("movetoworkspacesilent", ws) if !ws.is_empty() => {
            format!("hl.dsp.window.move({{ workspace = {}, follow = false }})", lua_str(ws))
        }
        ("togglespecialworkspace", n) => format!("hl.dsp.workspace.toggle_special({})", lua_str(n)),
        ("togglefloating", _) => "hl.dsp.window.float({ action = \"toggle\" })".into(),
        ("fullscreen", "1") => "hl.dsp.window.fullscreen({ mode = \"maximized\" })".into(),
        ("fullscreen", _) => "hl.dsp.window.fullscreen({ mode = \"fullscreen\" })".into(),
        ("killactive", _) => "hl.dsp.window.close()".into(),
        ("pseudo", _) => "hl.dsp.window.pseudo()".into(),
        ("movefocus", dir @ ("l" | "r" | "u" | "d")) => format!("hl.dsp.focus({{ direction = {} }})", lua_str(dir)),
        ("exec", cmd) if !cmd.is_empty() => format!("hl.dsp.exec_cmd({})", lua_str(cmd)),
        _ => return None,
    };
    Some(lua)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn folded_volume_clamps() {
        assert_eq!(folded_volume(5, 1), 5);
        assert_eq!(folded_volume(-5, 3), -15);
        assert_eq!(folded_volume(25, 8), 100);
        assert_eq!(folded_volume(-25, 8), -100);
        assert_eq!(folded_volume(i8::MIN, 8), -100);
    }

    #[test]
    fn run_refuses_exactly_the_deck_owned_kinds() {
        for kind in 0..Action::KINDS.len() {
            // Volume would really change the volume; it isn't deck-owned anyway.
            if kind == 8 {
                continue;
            }
            let a = Action::empty_of_kind(kind);
            let refused = run(&a).is_err_and(|e| e.to_string().ends_with("only works on the deck"));
            assert_eq!(refused, a.is_deck_owned(), "{a:?}");
        }
    }

    #[test]
    fn hypr_steps_check_before_the_thread() {
        // More than one step would spawn the helper thread; an empty dispatcher must be
        // rejected first, on the caller's thread, like the inline one-step case.
        let err = run_steps(&Action::Hyprland { dispatch: "  ".into() }, 3).unwrap_err();
        assert!(err.to_string().contains("no dispatcher"));
    }

    #[test]
    fn hypr_translation() {
        assert_eq!(legacy_to_lua("workspace 3").unwrap(), r#"hl.dsp.focus({ workspace = "3" })"#);
        assert_eq!(
            legacy_to_lua("movetoworkspacesilent 2").unwrap(),
            r#"hl.dsp.window.move({ workspace = "2", follow = false })"#
        );
        assert_eq!(legacy_to_lua("killactive").unwrap(), "hl.dsp.window.close()");
        assert_eq!(legacy_to_lua(r#"exec notify-send "hi""#).unwrap(), r#"hl.dsp.exec_cmd("notify-send \"hi\"")"#);
        assert_eq!(legacy_to_lua("bogus 1"), None);
    }

    #[test]
    fn urls() {
        assert_eq!(normalize_url("omarchy.org"), "https://omarchy.org");
        assert_eq!(normalize_url("http://x"), "http://x");
    }

    use audio::Backend;

    fn one(cmds: Vec<Vec<String>>) -> Vec<String> {
        assert_eq!(cmds.len(), 1);
        cmds.into_iter().next().unwrap()
    }

    #[test]
    fn volume_command_omarchy4() {
        assert_eq!(
            one(volume_command(Backend::Omarchy4, AudioDevice::Output, 5, false, None)),
            vec!["omarchy-audio-output-volume", "+5"]
        );
        assert_eq!(
            one(volume_command(Backend::Omarchy4, AudioDevice::Output, -5, false, None)),
            vec!["omarchy-audio-output-volume", "-5"]
        );
        assert_eq!(
            one(volume_command(Backend::Omarchy4, AudioDevice::Output, 0, true, None)),
            vec!["omarchy-audio-output-volume", "mute-toggle"]
        );
        assert_eq!(
            one(volume_command(Backend::Omarchy4, AudioDevice::Input, 0, true, None)),
            vec!["omarchy-audio-input-mute"]
        );
        // Mic volume has no Omarchy command: wpctl, capped at 100% (the pop-up is added
        // by run_volume).
        assert_eq!(
            one(volume_command(Backend::Omarchy4, AudioDevice::Input, 5, false, None)),
            vec!["wpctl", "set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SOURCE@", "5%+"]
        );
    }

    #[test]
    fn volume_command_plain() {
        assert_eq!(
            one(volume_command(Backend::Plain, AudioDevice::Output, 5, false, None)),
            vec!["wpctl", "set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%+"]
        );
        assert_eq!(
            one(volume_command(Backend::Plain, AudioDevice::Output, -5, false, None)),
            vec!["wpctl", "set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", "5%-"]
        );
        assert_eq!(
            one(volume_command(Backend::Plain, AudioDevice::Output, 0, true, None)),
            vec!["wpctl", "set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]
        );
        assert_eq!(
            one(volume_command(Backend::Plain, AudioDevice::Input, 5, false, None)),
            vec!["wpctl", "set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SOURCE@", "5%+"]
        );
    }

    #[test]
    fn volume_command_omarchy3() {
        assert_eq!(
            one(volume_command(Backend::Omarchy3, AudioDevice::Output, 5, false, Some("DP-1"))),
            vec!["swayosd-client", "--monitor", "DP-1", "--output-volume", "+5"]
        );
        assert_eq!(
            one(volume_command(Backend::Omarchy3, AudioDevice::Input, 0, true, Some("DP-1"))),
            vec!["swayosd-client", "--monitor", "DP-1", "--input-volume", "mute-toggle"]
        );
    }

    #[test]
    fn volume_step_signs() {
        assert_eq!(signed(5), "+5");
        assert_eq!(signed(-5), "-5");
        assert_eq!(wpctl_rel(5), "5%+");
        assert_eq!(wpctl_rel(-5), "5%-");
        // A hand-edited `change = -128` must not panic.
        assert_eq!(wpctl_rel(i8::MIN), "128%-");
        assert_eq!(wpctl_rel(i8::MAX), "127%+");
    }

    #[test]
    fn tracked_argv_uses_a_scope_never_uwsm() {
        let argv = vec!["sleep".into(), "5".into()];
        let a = tracked_argv(&argv, true);
        assert_eq!(
            a,
            vec![
                "systemd-run",
                "--user",
                "--scope",
                "--collect",
                "--quiet",
                "--",
                "sh",
                "-c",
                "trap '' PIPE; exec \"$@\"",
                "sh",
                "sleep",
                "5",
            ]
        );
        let b = tracked_argv(&argv, false);
        assert_eq!(&b[..4], ["sh", "-c", "trap '' PIPE; exec \"$@\"", "sh"]);
        assert!(!b.iter().any(|s| s == "uwsm-app"), "never through uwsm-app: it returns at once");
    }

    #[test]
    fn run_tracked_streams_lines() {
        let mut job = run_tracked(&Action::App { command: "for i in 1 2 3; do echo \"$i%\"; done".into() }).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(s) = job.try_finish() {
                break s;
            }
            assert!(Instant::now() < deadline, "job did not finish in time");
            thread::sleep(Duration::from_millis(20));
        };
        assert!(status.success());
        assert_eq!(job.lines().collect::<Vec<_>>(), vec!["1%", "2%", "3%"]);
    }
}
