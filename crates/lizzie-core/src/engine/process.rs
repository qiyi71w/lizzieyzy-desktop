//! OS process adapter for engines: spawns the command, writes from its own thread (a slow engine
//! never blocks the caller) and reports stdout, stderr and the exit through a sink with the
//! instance id. Ending a process sends `quit`, waits up to three seconds and then kills it
//! (Java `Leelaz.normalQuit`: quit, destroy, destroyForcibly).

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use super::gtp::LaunchSpec;
use super::runtime::{EngineInput, EngineIo, InstanceId, Launcher};

pub type Sink = Arc<dyn Fn(InstanceId, EngineInput) + Send + Sync>;

const QUIT_GRACE: Duration = Duration::from_secs(3);

pub struct ProcessLauncher {
    sink: Sink,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

/// Waits for the processes a [`ProcessLauncher`] is ending (application exit), without access to
/// the launcher itself.
#[derive(Clone)]
pub struct ProcessWaiter {
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl ProcessLauncher {
    pub fn new(sink: Sink) -> ProcessLauncher {
        ProcessLauncher {
            sink,
            endings: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn waiter(&self) -> ProcessWaiter {
        ProcessWaiter {
            endings: self.endings.clone(),
        }
    }
}

impl ProcessWaiter {
    /// Waits until every ending process is gone, at most `timeout` (each is killed after the
    /// quit grace period anyway).
    pub fn wait(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        let handles: Vec<JoinHandle<()>> = std::mem::take(&mut *self.endings.lock().unwrap());
        for h in handles {
            while !h.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(10));
            }
            if h.is_finished() {
                let _ = h.join();
            }
        }
    }
}
enum Write_ {
    Line(String),
    Quit,
}

struct ProcessIo {
    tx: Sender<Write_>,
    child: Arc<Mutex<Child>>,
    endings: Arc<Mutex<Vec<JoinHandle<()>>>>,
    ended: bool,
}

fn read_lines(stream: impl Read, mut each: impl FnMut(String)) {
    let mut reader = BufReader::new(stream);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => return,
            Ok(_) => {
                while buf.last().is_some_and(|b| *b == b'\n' || *b == b'\r') {
                    buf.pop();
                }
                each(String::from_utf8_lossy(&buf).into_owned());
            }
        }
    }
}

impl Launcher for ProcessLauncher {
    fn launch(&self, instance: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        let mut cmd = Command::new(&spec.program);
        cmd.args(&spec.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(dir) = &spec.working_dir {
            cmd.current_dir(dir);
        }
        #[cfg(windows)]
        if !spec.library_dirs.is_empty() {
            let mut paths = spec.library_dirs.clone();
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
            if let Ok(path) = std::env::join_paths(paths) {
                cmd.env("PATH", path);
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = cmd.spawn()?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let child = Arc::new(Mutex::new(child));
        let (tx, rx) = mpsc::channel::<Write_>();
        std::thread::Builder::new()
            .name(format!("engine-{instance}-in"))
            .spawn(move || {
                for msg in rx {
                    let line = match msg {
                        Write_::Line(l) => l,
                        Write_::Quit => "quit".to_string(),
                    };
                    let quit = line == "quit";
                    if writeln!(stdin, "{line}").and_then(|_| stdin.flush()).is_err() || quit {
                        return; // dropping stdin closes the pipe
                    }
                }
            })?;
        let sink = self.sink.clone();
        std::thread::Builder::new()
            .name(format!("engine-{instance}-err"))
            .spawn(move || read_lines(stderr, |l| sink(instance, EngineInput::Stderr(l))))?;
        let sink = self.sink.clone();
        let waiter = child.clone();
        std::thread::Builder::new()
            .name(format!("engine-{instance}-out"))
            .spawn(move || {
                read_lines(stdout, |l| sink(instance, EngineInput::Stdout(l)));
                // Output closed: report the exit once the process is gone.
                let deadline = Instant::now() + QUIT_GRACE;
                loop {
                    let status = waiter.lock().unwrap().try_wait();
                    match status {
                        Ok(Some(s)) => return sink(instance, EngineInput::Exited(s.code())),
                        Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
                        _ => return sink(instance, EngineInput::Exited(None)),
                    }
                }
            })?;
        Ok(Box::new(ProcessIo {
            tx,
            child,
            endings: self.endings.clone(),
            ended: false,
        }))
    }
}

impl ProcessIo {
    fn end(&mut self) {
        if self.ended {
            return;
        }
        self.ended = true;
        let _ = self.tx.send(Write_::Quit);
        let child = self.child.clone();
        let handle = std::thread::spawn(move || {
            let deadline = Instant::now() + QUIT_GRACE;
            loop {
                let mut c = child.lock().unwrap();
                match c.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() < deadline => {
                        drop(c);
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    _ => {
                        let _ = c.kill();
                        let _ = c.wait();
                        return;
                    }
                }
            }
        });
        let mut endings = self.endings.lock().unwrap();
        endings.retain(|h| !h.is_finished());
        endings.push(handle);
    }
}

impl EngineIo for ProcessIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.tx
            .send(Write_::Line(line.to_string()))
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::BrokenPipe, "engine input closed"))
    }

    fn terminate(mut self: Box<Self>) {
        self.end();
    }
}

impl Drop for ProcessIo {
    fn drop(&mut self) {
        self.end();
    }
}
