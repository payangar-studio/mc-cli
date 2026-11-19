//! Running a server until its log says `Done`, then stopping it.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::Step;

/// How many log lines a failure shows.
const TAIL: usize = 20;
/// How long a server gets to stop after `stop` before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(60);

/// Starts `command`, waits for the `Done` line of the log, then sends `stop`.
/// Fails when the process exits first or `timeout` passes, with the last
/// lines of the log.
pub fn until_done(command: Command, timeout: Duration) -> Step<()> {
    let mut server = Running::start(command)?;
    match server.wait_for(timeout, is_done) {
        Ok(_) => {
            server.stop();
            Ok(())
        }
        Err(Wait::Timeout) => Err(format!(
            "no `Done` in the log within {}s; last lines of the log:\n{}",
            timeout.as_secs(),
            server.tail()
        )),
        Err(Wait::Exited(status)) => Err(format!(
            "the server stopped before `Done` ({status}); last lines of the log:\n{}",
            server.tail()
        )),
    }
}

/// Why [`Running::wait_for`] found no line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wait {
    /// The time ran out.
    Timeout,
    /// The process exited, with this status.
    Exited(String),
}

/// A server process: its log lines as they come, and its console. Killed when
/// dropped, unless stopped before.
pub struct Running {
    child: Child,
    lines: mpsc::Receiver<String>,
    log: Vec<String>,
    stopped: bool,
}

impl Running {
    /// Starts `command`, its console on stdin and its log read from stdout
    /// and stderr.
    pub fn start(mut command: Command) -> Step<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command
            .spawn()
            .map_err(|e| format!("cannot start the server: {e}"))?;
        let (tx, lines) = mpsc::channel();
        let stdout = child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        let stderr = child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn Read + Send>);
        for stream in [stdout, stderr].into_iter().flatten() {
            let tx = tx.clone();
            thread::spawn(move || {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
        }
        Ok(Self {
            child,
            lines,
            log: Vec::new(),
            stopped: false,
        })
    }

    /// Waits up to `timeout` for a line of the log for which `found` is true,
    /// and returns it.
    pub fn wait_for(
        &mut self,
        timeout: Duration,
        found: impl Fn(&str) -> bool,
    ) -> Result<String, Wait> {
        let deadline = Instant::now() + timeout;
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => {
                    tracing::debug!(target: "server", "{line}");
                    let hit = found(&line);
                    self.log.push(line);
                    if hit {
                        return Ok(self.log.last().cloned().unwrap_or_default());
                    }
                }
                Err(RecvTimeoutError::Timeout) => return Err(Wait::Timeout),
                Err(RecvTimeoutError::Disconnected) => {
                    let status = self
                        .child
                        .wait()
                        .map_or_else(|e| e.to_string(), |s| s.to_string());
                    self.stopped = true;
                    return Err(Wait::Exited(status));
                }
            }
        }
    }

    /// Sends `command` to the console.
    pub fn send(&mut self, command: &str) -> Step<()> {
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or("the console of the server is closed")?;
        stdin
            .write_all(format!("{command}\n").as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("cannot send `{command}` to the server: {e}"))
    }

    /// Every line of the log read so far.
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// The last lines of the log read so far.
    pub fn tail(&self) -> String {
        self.log[self.log.len().saturating_sub(TAIL)..].join("\n")
    }

    /// Sends `stop` and waits for the process to exit, killing it after a
    /// grace period.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        stop(&mut self.child);
        // What the server logged while stopping.
        while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100)) {
            self.log.push(line);
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

/// `[Server thread/INFO]: Done (5.123s)! For help, type "help"`
pub fn is_done(line: &str) -> bool {
    line.contains("]: Done (")
}

/// Sends `stop` and waits for the process to exit, killing it after a grace
/// period. The server already reached `Done`: a slow stop is only logged.
fn stop(child: &mut Child) {
    if let Some(stdin) = child.stdin.as_mut() {
        let _ = stdin.write_all(b"stop\n").and_then(|()| stdin.flush());
    }
    let deadline = Instant::now() + STOP_GRACE;
    while Instant::now() < deadline {
        if let Ok(Some(_)) = child.try_wait() {
            return;
        }
        thread::sleep(Duration::from_millis(100));
    }
    tracing::warn!(
        "the server did not stop within {}s of `stop`, killed",
        STOP_GRACE.as_secs()
    );
    let _ = child.kill();
    let _ = child.wait();
}

/// The last lines of a text, for an installer that failed.
pub fn last_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(TAIL)..].join("\n")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn fake(script: &str) -> Command {
        let mut command = Command::new("sh");
        command.arg("-c").arg(script);
        command
    }

    const DONE: &str = r#"echo '[Server thread/INFO]: Done (0.42s)! For help, type "help"'"#;

    #[test]
    fn done_line_passes_and_the_server_is_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let got = dir.path().join("got");
        let script = format!(
            "echo starting; {DONE}; read cmd; echo \"$cmd\" > '{}'",
            got.display()
        );
        let status = until_done(fake(&script), Duration::from_secs(20));
        assert_eq!(status, Ok(()));
        assert_eq!(std::fs::read_to_string(got).unwrap(), "stop\n");
    }

    #[test]
    fn crash_fails_with_the_last_lines_of_the_log() {
        // The pause keeps stderr after stdout in the merged log.
        let script = "for i in $(seq 1 30); do echo line $i; done; sleep 1; \
                      echo 'Exception in thread main' >&2; exit 3";
        let reason = match until_done(fake(script), Duration::from_secs(20)) {
            Err(reason) => reason,
            other => panic!("{other:?}"),
        };
        assert!(
            reason.starts_with("the server stopped before `Done`"),
            "{reason}"
        );
        assert!(reason.contains("3"), "exit status shown: {reason}");
        assert!(reason.contains("Exception in thread main"), "{reason}");
        assert!(reason.contains("line 30"), "{reason}");
        assert!(!reason.contains("line 5\n"), "only the tail: {reason}");
    }

    #[test]
    fn timeout_fails_and_kills_the_server() {
        let started = Instant::now();
        let status = until_done(fake("echo loading; exec sleep 30"), Duration::from_secs(1));
        assert!(started.elapsed() < Duration::from_secs(10));
        let reason = status.unwrap_err();
        assert!(
            reason.starts_with("no `Done` in the log within 1s"),
            "{reason}"
        );
        assert!(reason.ends_with("loading"), "{reason}");
    }

    #[test]
    fn a_running_server_takes_commands_and_shows_its_log() {
        let script = format!(
            "{DONE}; while read cmd; do echo \"got $cmd\"; [ \"$cmd\" = stop ] && exit 0; done"
        );
        let mut server = Running::start(fake(&script)).unwrap();
        let done = server.wait_for(Duration::from_secs(20), is_done).unwrap();
        assert!(done.contains("Done ("), "{done}");
        server.send("list").unwrap();
        let got = server
            .wait_for(Duration::from_secs(20), |l| l.starts_with("got "))
            .unwrap();
        assert_eq!(got, "got list");
        server.stop();
        assert!(
            server.log().iter().any(|l| l == "got stop"),
            "{:?}",
            server.log()
        );
    }

    #[test]
    fn waiting_ends_when_the_server_exits_or_the_time_runs_out() {
        let mut server = Running::start(fake("echo one; exec sleep 30")).unwrap();
        let started = Instant::now();
        assert_eq!(
            server.wait_for(Duration::from_secs(1), |l| l == "two"),
            Err(Wait::Timeout)
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(server.tail(), "one");
        drop(server);

        let mut server = Running::start(fake("echo one; exit 4")).unwrap();
        match server.wait_for(Duration::from_secs(20), |l| l == "two") {
            Err(Wait::Exited(status)) => assert!(status.contains('4'), "{status}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn last_lines_keeps_the_tail() {
        let text: String = (1..=25).map(|i| format!("l{i}\n")).collect();
        let tail = last_lines(&text);
        assert!(tail.starts_with("l6\n") && tail.ends_with("l25"), "{tail}");
    }
}
