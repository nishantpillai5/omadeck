//! Running a short command and waiting for it, with a time limit, so a hung tool (a
//! `pactl` while PipeWire restarts, a stalled Hyprland) can never block whoever waits.

use std::io::Read;
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
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
}
