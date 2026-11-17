//! The Windows board synchronization tool (ReadBoard, Java `analysis/ReadBoard.java`): a separate
//! helper process recognises a Go board on screen, streams it to this client and clicks the moves
//! this client asks for on the target window. This module holds the platform-independent side:
//! the line protocol, the per-helper session, launching, hosted helper updates and the helper's
//! logging negotiation. The process itself is owned by the app shell through [`HelperHost`].

pub mod diagnostics;
pub mod launch;
pub mod logging;
pub mod protocol;
pub mod session;
pub mod update;

use std::path::PathBuf;

/// One helper launch. Ids grow monotonically and are never reused, so lines from a helper that
/// was replaced or stopped are recognised and dropped.
pub type HelperId = u64;

/// What a helper process produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelperInput {
    /// One protocol line, without the trailing `\n` (a trailing `\r` may remain).
    Line(String),
    /// The process ended, its output closed, or reading failed (`error` is the read error).
    Ended { error: Option<String> },
}

/// How to start the helper.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HelperLaunch {
    pub program: PathBuf,
    pub dir: PathBuf,
    pub args: Vec<String>,
}

/// The operating-system side of a helper: process creation, its input pipe and its end.
pub trait HelperHost: Send {
    /// Starts the process with piped input/output, error output merged into the output (Java
    /// `redirectErrorStream(true)`) and `spec.dir` as working directory. Its lines and its end
    /// arrive later through the app as `Workbench::readboard_input(helper, HelperInput)`.
    fn launch(&mut self, helper: HelperId, spec: &HelperLaunch) -> std::io::Result<()>;
    /// Writes `line` and a newline (UTF-8) and flushes.
    fn send(&mut self, helper: HelperId, line: &str) -> std::io::Result<()>;
    /// Stops the process without blocking the caller (Java `releaseHostedResources`): waits up to
    /// 1000 ms for it to exit, then destroys it, waits 200 ms and kills it. A graceful exit sends
    /// `quit` first. Idempotent; unknown ids are ignored.
    ///
    /// Returns whether the process is still exiting. Then, once it has exited or was killed (not
    /// merely closed its output), the app reports that exactly once as
    /// `Workbench::readboard_exited(helper)`, always later and never from inside `stop`; a
    /// successor waits for it. `false` (unknown id, process already reaped) reports nothing.
    fn stop(&mut self, helper: HelperId) -> bool;
}
