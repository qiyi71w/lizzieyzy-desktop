//! OS process host for the board synchronization helper (Java `ReadBoard.startEngine`, `read`,
//! `sendCommandTo` and `releaseHostedResources`). The helper's output and error output share one
//! pipe (Java `redirectErrorStream(true)`); a reader thread per helper splits it into lines and
//! hands them, then the end, to the sink. Stopping never blocks the caller: a background thread
//! waits for the exit, kills the helper when it does not leave in time and then reports the exit.

use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lizzie_core::readboard::{HelperHost, HelperId, HelperInput, HelperLaunch};
use lizzie_core::workbench::ReadBoardInstall;

/// Receives each helper's lines in order, then exactly one [`HelperInput::Ended`], on the
/// helper's reader thread.
pub type HelperSink = Arc<dyn Fn(HelperId, HelperInput) + Send + Sync>;

/// Receives a stopped helper's exit (the process exited or was killed and reaped), once, on its
/// stop thread ([`HelperHost::stop`]).
pub type ExitSink = Arc<dyn Fn(HelperId) + Send + Sync>;

/// Java `PROCESS_EXIT_WAIT_TIMEOUT_MS`: how long a stopped helper may take to exit by itself.
const EXIT_WAIT: Duration = Duration::from_millis(1000);
/// Java `PROCESS_DESTROY_WAIT_TIMEOUT_MS`: how long it may take after being terminated.
const DESTROY_WAIT: Duration = Duration::from_millis(200);
const POLL: Duration = Duration::from_millis(10);

type Endings = Arc<Mutex<Vec<JoinHandle<()>>>>;

struct Running {
    child: Child,
    stdin: ChildStdin,
}

/// The helper processes of the app, by helper id.
pub struct ProcessHelperHost {
    sink: HelperSink,
    exited: ExitSink,
    running: HashMap<HelperId, Running>,
    endings: Endings,
}

/// Waits for helpers that are being stopped (application exit), without access to the host.
#[derive(Clone)]
pub struct HelperWaiter {
    endings: Endings,
}

impl HelperWaiter {
    /// Waits until every stop sequence has finished (its exit reported), at most `timeout`.
    /// Sequences still running stay listed, so another waiter (the installer, application exit)
    /// still waits for them.
    pub fn wait(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        loop {
            let running = {
                let mut endings = self.endings.lock().unwrap();
                endings.retain(|h| !h.is_finished());
                !endings.is_empty()
            };
            if !running || Instant::now() >= deadline {
                return;
            }
            thread::sleep(POLL);
        }
    }
}

impl ProcessHelperHost {
    pub fn new(sink: HelperSink, exited: ExitSink) -> ProcessHelperHost {
        ProcessHelperHost { sink, exited, running: HashMap::new(), endings: Arc::new(Mutex::new(Vec::new())) }
    }

    pub fn waiter(&self) -> HelperWaiter {
        HelperWaiter { endings: self.endings.clone() }
    }
}

/// Java `read`: lines end at `\n` (a `\r` before it stays; parsers trim it), decoded as UTF-8.
/// The end of the output and a read failure both end the helper; an unterminated last line is
/// dropped like Java's.
fn read_lines(helper: HelperId, output: io::PipeReader, sink: &HelperSink) {
    let mut reader = BufReader::new(output);
    let mut line = Vec::new();
    let error = loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) => break None,
            Ok(_) if line.last() != Some(&b'\n') => break None,
            Ok(_) => {
                line.pop();
                sink(helper, HelperInput::Line(String::from_utf8_lossy(&line).into_owned()));
            }
            Err(e) => break Some(e.to_string()),
        }
    };
    sink(helper, HelperInput::Ended { error });
}

/// Waits up to `timeout` for the process to exit.
fn exited_within(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL),
            _ => return false,
        }
    }
}

/// Java `waitForHostedProcessExit`: wait, terminate, wait, kill. The standard library only has
/// the forcible kill (Java's `destroy()` is already `TerminateProcess` on Windows), so both steps
/// kill; the process is then reaped.
fn finish(mut child: Child) {
    if exited_within(&mut child, EXIT_WAIT) {
        return;
    }
    let _ = child.kill();
    if exited_within(&mut child, DESTROY_WAIT) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn not_running(helper: HelperId) -> io::Error {
    io::Error::new(io::ErrorKind::NotConnected, format!("helper {helper} is not running"))
}

impl HelperHost for ProcessHelperHost {
    /// No `CREATE_NO_WINDOW`: the helper is a window application.
    fn launch(&mut self, helper: HelperId, spec: &HelperLaunch) -> io::Result<()> {
        if self.running.contains_key(&helper) {
            return Err(io::Error::new(io::ErrorKind::AlreadyExists, format!("helper {helper} is already running")));
        }
        let (output, output_writer) = io::pipe()?;
        let mut command = Command::new(&spec.program);
        command
            .args(&spec.args)
            .current_dir(&spec.dir)
            .stdin(Stdio::piped())
            .stdout(output_writer.try_clone()?)
            .stderr(output_writer);
        let spawned = command.spawn();
        // The command holds the pipe's write ends; only the helper may keep them, so the reader
        // sees the end of the output when the helper exits.
        drop(command);
        let mut child = spawned?;
        let stdin = child.stdin.take().expect("piped stdin");
        let sink = self.sink.clone();
        let reader = thread::Builder::new()
            .name(format!("readboard-{helper}-out"))
            .spawn(move || read_lines(helper, output, &sink));
        if let Err(e) = reader {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
        self.running.insert(helper, Running { child, stdin });
        Ok(())
    }

    fn send(&mut self, helper: HelperId, line: &str) -> io::Result<()> {
        let running = self.running.get_mut(&helper).ok_or_else(|| not_running(helper))?;
        if running.child.try_wait()?.is_some() {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, format!("helper {helper} has exited")));
        }
        let mut bytes = Vec::with_capacity(line.len() + 1);
        bytes.extend_from_slice(line.as_bytes());
        bytes.push(b'\n');
        running.stdin.write_all(&bytes)?;
        running.stdin.flush()
    }

    /// Closes the helper's input like Java's `closeQuietly(outputStream)`, then runs the exit
    /// sequence on a background thread, which reports the exit when it is over.
    fn stop(&mut self, helper: HelperId) -> bool {
        let Some(Running { child, stdin }) = self.running.remove(&helper) else {
            return false;
        };
        drop(stdin);
        let slot = Arc::new(Mutex::new(Some(child)));
        let worker = slot.clone();
        let exited = self.exited.clone();
        let spawned = thread::Builder::new().name(format!("readboard-{helper}-stop")).spawn(move || {
            let child = worker.lock().unwrap().take();
            if let Some(child) = child {
                finish(child);
                exited(helper);
            }
        });
        match spawned {
            Ok(handle) => {
                let mut endings = self.endings.lock().unwrap();
                endings.retain(|h| !h.is_finished());
                endings.push(handle);
                true
            }
            // Without a thread the process is ended here, before returning: nothing to report.
            Err(_) => {
                if let Some(mut child) = slot.lock().unwrap().take() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                false
            }
        }
    }
}

impl Drop for ProcessHelperHost {
    fn drop(&mut self) {
        let helpers: Vec<HelperId> = self.running.keys().copied().collect();
        for helper in helpers {
            self.stop(helper);
        }
    }
}

/// Where `readboard/` may be (Java's working directory, the program's directory and its
/// bundled runtime): the working directory, the executable's directory and the resource
/// directory where the Windows packages install `readboard/readboard.exe`.
pub fn base_dirs(app: &tauri::AppHandle) -> Vec<PathBuf> {
    use tauri::Manager;
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        dirs.push(dir);
    }
    if let Ok(dir) = app.path().resource_dir() {
        dirs.push(dir);
    }
    dirs
}

/// A confirmed helper update: once the replaced helper has exited, the package is installed off
/// the UI thread and the helper starts again.
pub fn install_update(app: tauri::AppHandle, shared: crate::State, install: ReadBoardInstall) {
    thread::spawn(move || {
        // Java waits for the hosted process before touching its files (they are locked while it
        // runs on Windows).
        shared.helpers.wait(Duration::from_secs(5));
        let result = install.run();
        shared.workbench.lock().unwrap().finish_readboard_install(install, result);
        crate::publish(&app, &shared);
    });
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;

    const WAIT: Duration = Duration::from_secs(5);

    type Inputs = mpsc::Receiver<(HelperId, HelperInput)>;
    type Exits = mpsc::Receiver<HelperId>;

    fn host() -> (ProcessHelperHost, Inputs, Exits) {
        let (tx, rx) = mpsc::channel();
        let sink: HelperSink = Arc::new(move |helper: HelperId, input: HelperInput| {
            let _ = tx.send((helper, input));
        });
        let (exit_tx, exits) = mpsc::channel();
        let exited: ExitSink = Arc::new(move |helper: HelperId| {
            let _ = exit_tx.send(helper);
        });
        (ProcessHelperHost::new(sink, exited), rx, exits)
    }

    fn sh(script: &str) -> HelperLaunch {
        HelperLaunch {
            program: PathBuf::from("/bin/sh"),
            dir: PathBuf::from("/"),
            args: vec!["-c".to_string(), script.to_string()],
        }
    }

    fn line(text: &str) -> HelperInput {
        HelperInput::Line(text.to_string())
    }

    fn next(rx: &Inputs) -> (HelperId, HelperInput) {
        rx.recv_timeout(WAIT).expect("helper input")
    }

    #[test]
    fn output_and_error_lines_arrive_in_order_then_the_end() {
        let (mut host, rx, exits) = host();
        host.launch(7, &sh("pwd; printf 'one\\ntwo\\r\\n'; printf 'err\\n' >&2; printf 'three\\npartial'"))
            .unwrap();
        for expected in [line("/"), line("one"), line("two\r"), line("err"), line("three"), HelperInput::Ended { error: None }] {
            assert_eq!(next(&rx), (7, expected));
        }
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        // The process is gone: sending fails instead of panicking.
        assert!(host.send(7, "place 3 3").is_err());
        // Stopping reaps it and reports its exit once; a second stop knows nothing of it.
        assert!(host.stop(7));
        assert!(!host.stop(7));
        assert_eq!(exits.recv_timeout(WAIT), Ok(7));
        host.waiter().wait(WAIT);
        assert!(exits.recv_timeout(Duration::from_millis(200)).is_err());
    }

    #[test]
    fn sent_lines_reach_the_helper_and_stop_closes_its_input() {
        let (mut host, rx, _exits) = host();
        host.launch(1, &sh("while read l; do echo \"got $l\"; done; echo bye")).unwrap();
        host.send(1, "place 3 4").unwrap();
        assert_eq!(next(&rx), (1, line("got place 3 4")));
        host.send(1, "黑棋").unwrap();
        assert_eq!(next(&rx), (1, line("got 黑棋")));

        let started = Instant::now();
        host.stop(1);
        assert!(started.elapsed() < Duration::from_millis(500));
        assert_eq!(next(&rx), (1, line("bye")));
        assert_eq!(next(&rx), (1, HelperInput::Ended { error: None }));
        assert!(host.send(1, "quit").is_err());
        host.waiter().wait(WAIT);
    }

    #[test]
    fn stop_kills_a_helper_that_does_not_exit() {
        let (mut host, rx, exits) = host();
        host.launch(2, &sh("exec sleep 30")).unwrap();
        let started = Instant::now();
        assert!(host.stop(2));
        assert!(started.elapsed() < Duration::from_millis(500));
        // Its exit is reported only once it was killed.
        assert_eq!(exits.recv_timeout(WAIT), Ok(2));
        assert!(started.elapsed() >= EXIT_WAIT);
        assert_eq!(next(&rx), (2, HelperInput::Ended { error: None }));
        host.waiter().wait(WAIT);
    }

    #[test]
    fn launch_failure_and_unknown_helpers_are_errors_not_panics() {
        let (mut host, rx, exits) = host();
        let missing = HelperLaunch {
            program: PathBuf::from("/nonexistent/readboard/readboard.exe"),
            dir: PathBuf::from("/"),
            args: vec!["yzy".to_string()],
        };
        assert!(host.launch(3, &missing).is_err());
        assert!(host.send(3, "version").is_err());
        assert!(!host.stop(3));
        assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
        assert!(exits.try_recv().is_err());

        host.launch(4, &sh("read l")).unwrap();
        assert_eq!(host.launch(4, &sh("true")).unwrap_err().kind(), io::ErrorKind::AlreadyExists);
        drop(host);
        assert_eq!(next(&rx), (4, HelperInput::Ended { error: None }));
    }
}
