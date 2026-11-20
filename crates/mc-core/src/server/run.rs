//! Running a server until its log says `Done`, then stopping it.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufRead, BufReader, LineWriter, Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use super::{Step, io_err};

/// How many log lines a failure shows.
const TAIL: usize = 20;
/// How long a server gets to stop after `stop` before it is killed.
const STOP_GRACE: Duration = Duration::from_secs(60);
/// How long a stopped server's log is still read.
const DRAIN: Duration = Duration::from_secs(5);

/// Starts `command`, waits for the `Done` line of the log, then sends `stop`.
/// Fails when the process exits first or `timeout` passes, with the last
/// lines of the log.
pub fn until_done(command: Command, timeout: Duration) -> Step<()> {
    let mut server = Running::start(command, None)?;
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
    /// The last lines read.
    tail: VecDeque<String>,
    /// Where every line read goes, if anywhere.
    file: Option<(LineWriter<File>, String)>,
    stopped: bool,
}

impl Running {
    /// Starts `command`, its console on stdin and its log read from stdout
    /// and stderr. Every line read is also written to the file `log`, when
    /// given; only the last ones are kept in memory.
    pub fn start(mut command: Command, log: Option<&Path>) -> Step<Self> {
        let file = match log {
            Some(path) => Some((
                LineWriter::new(File::create(path).map_err(io_err("creating", path))?),
                path.display().to_string(),
            )),
            None => None,
        };
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
            tail: VecDeque::with_capacity(TAIL),
            file,
            stopped: false,
        })
    }

    /// Keeps `line` in the tail and writes it to the log file. A log file
    /// that cannot be written is given up, the run goes on.
    fn keep(&mut self, line: String) {
        if let Some((file, path)) = &mut self.file
            && let Err(e) = writeln!(file, "{line}")
        {
            tracing::warn!("cannot write the server log {path}: {e}");
            self.file = None;
        }
        if self.tail.len() == TAIL {
            self.tail.pop_front();
        }
        self.tail.push_back(line);
    }

    /// Waits up to `timeout` for a line of the log for which `found` is true,
    /// and returns it.
    pub fn wait_for(
        &mut self,
        timeout: Duration,
        mut found: impl FnMut(&str) -> bool,
    ) -> Result<String, Wait> {
        let deadline = Instant::now() + timeout;
        loop {
            match self
                .lines
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            {
                Ok(line) => {
                    tracing::debug!(target: "server", "{line}");
                    if found(&line) {
                        self.keep(line.clone());
                        return Ok(line);
                    }
                    self.keep(line);
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

    /// The last lines of the log read so far.
    pub fn tail(&self) -> String {
        self.tail.iter().cloned().collect::<Vec<_>>().join("\n")
    }

    /// Sends `stop` and waits for the process to exit, killing it after a
    /// grace period.
    pub fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        stop(&mut self.child);
        // What the server logged while stopping, unless something it left
        // behind keeps logging.
        let deadline = Instant::now() + DRAIN;
        while Instant::now() < deadline
            && let Ok(line) = self.lines.recv_timeout(Duration::from_millis(100))
        {
            self.keep(line);
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
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("server.log");
        let mut server = Running::start(fake(&script), Some(&log)).unwrap();
        let done = server.wait_for(Duration::from_secs(20), is_done).unwrap();
        assert!(done.contains("Done ("), "{done}");
        server.send("list").unwrap();
        let got = server
            .wait_for(Duration::from_secs(20), |l| l.starts_with("got "))
            .unwrap();
        assert_eq!(got, "got list");
        server.stop();
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.ends_with("got list\ngot stop\n"), "{text}");
        assert!(text.contains("Done ("), "{text}");
    }

    #[test]
    fn waiting_ends_when_the_server_exits_or_the_time_runs_out() {
        let mut server = Running::start(fake("echo one; exec sleep 30"), None).unwrap();
        let started = Instant::now();
        assert_eq!(
            server.wait_for(Duration::from_secs(1), |l| l == "two"),
            Err(Wait::Timeout)
        );
        assert!(started.elapsed() < Duration::from_secs(10));
        assert_eq!(server.tail(), "one");
        drop(server);

        let mut server = Running::start(fake("echo one; exit 4"), None).unwrap();
        match server.wait_for(Duration::from_secs(20), |l| l == "two") {
            Err(Wait::Exited(status)) => assert!(status.contains('4'), "{status}"),
            other => panic!("{other:?}"),
        }
    }

    /// Memory keeps the last lines only; the log file gets them all.
    #[test]
    fn the_log_file_has_every_line_and_memory_the_last_ones() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("server.log");
        let script = "for i in $(seq 1 30); do echo line $i; done; exec sleep 30";
        let mut server = Running::start(fake(script), Some(&log)).unwrap();
        server
            .wait_for(Duration::from_secs(20), |l| l == "line 30")
            .unwrap();
        let tail = server.tail();
        assert!(
            tail.starts_with("line 11\n") && tail.ends_with("line 30"),
            "{tail}"
        );
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.starts_with("line 1\nline 2\n"), "{text}");
        assert!(text.ends_with("line 30\n"), "{text}");
    }

    /// A process left behind by the server that keeps the log open and
    /// writing does not hold `stop` forever.
    #[test]
    fn stopping_ends_while_something_keeps_logging() {
        let script =
            format!("{DONE}; (while :; do echo still; sleep 0.05; done) & read cmd; exit 0");
        let mut server = Running::start(fake(&script), None).unwrap();
        server.wait_for(Duration::from_secs(20), is_done).unwrap();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            server.stop();
            let _ = tx.send(());
            // Dropping the server closes the log: the writer gets SIGPIPE.
        });
        assert!(
            rx.recv_timeout(Duration::from_secs(30)).is_ok(),
            "stop still drains the log"
        );
    }

    #[test]
    fn last_lines_keeps_the_tail() {
        let text: String = (1..=25).map(|i| format!("l{i}\n")).collect();
        let tail = last_lines(&text);
        assert!(tail.starts_with("l6\n") && tail.ends_with("l25"), "{tail}");
    }
}
