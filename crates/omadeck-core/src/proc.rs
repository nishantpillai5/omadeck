//! Running a short command and waiting for it, with a time limit, so a hung tool (a
//! `pactl` while PipeWire restarts, a stalled Hyprland) can never block whoever waits.
//!
//! [`Job`] is the other half: a child whose output is read on its own thread, for callers
//! that must not block (the watcher's state commands, `busy_while_running` keys).

use std::io::{Read, Result as IoResult};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

/// How long any command we wait on may take before it's killed.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

/// How often to check whether the child has exited.
const POLL: Duration = Duration::from_millis(5);

/// Run `cmd` with stdin closed and stdout/stderr captured, and wait at most `timeout`.
/// `None` if it couldn't start or ran too long (it is killed then). A non-zero exit is
/// still `Some`: look at `status`.
pub fn output_timeout(cmd: &mut Command, timeout: Duration) -> Option<Output> {
    cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            log::debug!("failed to start {cmd:?}: {e}");
            return None;
        }
    };
    let deadline = Instant::now() + timeout;
    // Read both pipes on their own threads, so a chatty command can't fill a pipe and
    // block before it exits.
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                log::warn!("{cmd:?} took longer than {timeout:?}; killed it");
                return None;
            }
        }
    };
    // A background grandchild can keep a pipe open after the child exits; don't wait
    // for it past the deadline.
    let left = deadline.saturating_duration_since(Instant::now()).max(Duration::from_millis(50));
    Some(Output {
        status,
        stdout: stdout.recv_timeout(left).unwrap_or_default(),
        stderr: stderr.recv_timeout(left).unwrap_or_default(),
    })
}

/// A child whose output is read line by line on its own thread, so polling it never
/// blocks and a chatty command can't fill a pipe. The child runs in its own process
/// group, so [`Job::kill_group`] takes a pipeline down with it.
pub struct Job {
    child: Child,
    lines: mpsc::Receiver<String>,
    reader_done: Arc<AtomicBool>,
    exited: Option<(ExitStatus, Instant)>,
    started: Instant,
}

/// Chars kept per output line.
const LINE_MAX: usize = 64;
/// Bytes buffered per line before cutting (so nothing grows unbounded).
const PIECE_MAX: usize = 4096;
/// Wait for output this long after the child exits (a background grandchild can hold
/// the pipe open forever).
pub const GRACE: Duration = Duration::from_millis(100);

impl Job {
    /// Spawn in its own process group; stdin and stderr null; stdout split into lines on
    /// a helper thread ("job-out").
    pub fn spawn(mut cmd: Command) -> IoResult<Job> {
        cmd.stdin(Stdio::null()).stderr(Stdio::null()).stdout(Stdio::piped()).process_group(0);
        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let (tx, lines) = mpsc::channel();
        let reader_done = Arc::new(AtomicBool::new(false));
        let done = reader_done.clone();
        thread::Builder::new().name("job-out".into()).spawn(move || {
            read_lines(stdout, |l| {
                let _ = tx.send(l);
            });
            done.store(true, Ordering::Release);
        })?;
        Ok(Job { child, lines, reader_done, exited: None, started: Instant::now() })
    }

    pub fn started(&self) -> Instant {
        self.started
    }

    /// Lines that arrived since the last call.
    pub fn lines(&self) -> mpsc::TryIter<'_, String> {
        self.lines.try_iter()
    }

    /// The exit status once the child has exited and its output is read, or `GRACE` after
    /// exit.
    pub fn try_finish(&mut self) -> Option<ExitStatus> {
        if self.exited.is_none() {
            match self.child.try_wait() {
                Ok(Some(s)) => self.exited = Some((s, Instant::now())),
                Ok(None) => return None,
                Err(_) => self.exited = Some((ExitStatus::from_raw(1 << 8), Instant::now())),
            }
        }
        let (s, at) = self.exited?;
        (self.reader_done.load(Ordering::Acquire) || at.elapsed() >= GRACE).then_some(s)
    }

    /// SIGKILL the whole process group, so a pipeline dies too, then reap.
    pub fn kill_group(&mut self) {
        if self.exited.is_none() {
            unsafe { libc::killpg(self.child.id() as libc::pid_t, libc::SIGKILL) };
            let _ = self.child.wait();
            self.exited = Some((ExitStatus::from_raw(libc::SIGKILL), Instant::now()));
        }
    }
}

/// Split on `\n` **and** `\r` (progress bars redraw with `\r`), trim, skip empty pieces,
/// keep `LINE_MAX` chars. A piece is cut at `PIECE_MAX` bytes, so nothing grows
/// unbounded.
fn read_lines(mut r: impl Read, mut each: impl FnMut(String)) {
    let mut buf = [0u8; 4096];
    let mut cur: Vec<u8> = Vec::new();
    let mut emit = |cur: &mut Vec<u8>| {
        let s = String::from_utf8_lossy(cur);
        let t = s.trim();
        if !t.is_empty() {
            each(t.chars().take(LINE_MAX).collect());
        }
        cur.clear();
    };
    loop {
        match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        emit(&mut cur);
                    } else if cur.len() < PIECE_MAX {
                        cur.push(b);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    emit(&mut cur);
}

/// Read a pipe to the end on a helper thread; the bytes arrive on the returned channel.
fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    match pipe {
        Some(mut p) => {
            thread::spawn(move || {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                let _ = tx.send(buf);
            });
        }
        None => {
            let _ = tx.send(Vec::new());
        }
    }
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sh(script: &str) -> Command {
        let mut c = Command::new("sh");
        c.args(["-c", script]);
        c
    }

    #[test]
    fn output_timeout_returns_output_and_status() {
        let out = output_timeout(&mut sh("echo hi; echo oops >&2; exit 3"), Duration::from_secs(2)).unwrap();
        assert_eq!(out.status.code(), Some(3));
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"oops\n");
    }

    #[test]
    fn output_timeout_kills_a_hung_command() {
        let start = Instant::now();
        assert!(output_timeout(&mut sh("sleep 5"), Duration::from_millis(200)).is_none());
        assert!(start.elapsed() < Duration::from_secs(2), "took {:?}", start.elapsed());
    }

    #[test]
    fn output_timeout_missing_command_is_none() {
        assert!(output_timeout(&mut Command::new("omadeck-no-such-command"), Duration::from_secs(1)).is_none());
    }

    #[test]
    fn output_timeout_big_output_does_not_block() {
        // More than a pipe buffer (64 KiB): would deadlock if we read only after exit.
        let out = output_timeout(&mut sh("head -c 200000 /dev/zero"), Duration::from_secs(2)).unwrap();
        assert_eq!(out.stdout.len(), 200_000);
    }

    #[test]
    fn read_lines_splits_on_cr_and_lf() {
        let mut got = Vec::new();
        read_lines(&b"one\r two \n\n\rthree"[..], |l| got.push(l));
        assert_eq!(got, ["one", "two", "three"]);
        let mut long = Vec::new();
        read_lines(&[b'x'; 10_000][..], |l| long.push(l));
        assert_eq!(long.len(), 1);
        assert_eq!(long[0].chars().count(), 64, "cut to LINE_MAX");
    }
}
