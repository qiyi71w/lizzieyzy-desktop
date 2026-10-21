//! Engine resources, independent of Tauri and the application-state writer.
//!
//! Wave 2 uses [`Catalog::embedded`], [`detect_machine`] with a [`Probe`], and [`recommend`]
//! for read-only discovery. [`download`] accepts a caller-owned HTTP [`Fetch`] adapter,
//! cancellation token and progress callback. [`ResourceStore`] stages package/runtime installs,
//! imports weights, maintains receipts and inventories only its resource directory.
//! [`verify_engine`] and [`verify_cuda`] are explicit process operations; [`plan_setup`]
//! returns a profile change without mutating profiles or starting an engine. The caller owns
//! confirmation, task identity, stale-result rejection and committing the returned plan.

mod catalog;
mod download;
mod machine;
mod recommendation;
mod runtime;
mod setup;
mod store;
mod verification;
mod weights;

pub use catalog::*;
pub use download::*;
pub use machine::*;
pub use recommendation::*;
pub use runtime::*;
pub use setup::*;
pub use store::*;
pub use verification::*;
pub use weights::*;

use serde::{Deserialize, Serialize};
use std::fmt;
use ts_rs::TS;

/// Java `BundledKataGoProfile`: ownership survives only while the command is unchanged.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ManagedProfile {
    pub command: String,
    pub asset_id: String,
    pub weight_path: String,
    pub config_path: String,
}

#[derive(Debug)]
pub enum ResourceError {
    Io(std::io::Error),
    Invalid(String),
    Integrity {
        expected: String,
        actual: String,
    },
    Cancelled,
    Http(u16),
    /// Import did not match a trusted asset; no installed resource was changed.
    ImportMismatch,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => e.fmt(f),
            Self::Invalid(s) => f.write_str(s),
            Self::Integrity { expected, actual } => {
                write!(f, "Integrity failure: expected {expected}; actual {actual}")
            }
            Self::Cancelled => f.write_str("Cancelled"),
            Self::Http(status) => write!(f, "HTTP {status}"),
            Self::ImportMismatch => {
                f.write_str("Archive does not match a catalog package for this platform; nothing changed")
            }
        }
    }
}
impl std::error::Error for ResourceError {}
impl From<std::io::Error> for ResourceError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<serde_json::Error> for ResourceError {
    fn from(e: serde_json::Error) -> Self {
        Self::Invalid(e.to_string())
    }
}
impl From<zip::result::ZipError> for ResourceError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Invalid(e.to_string())
    }
}
pub type ResourceResult<T> = Result<T, ResourceError>;

pub(crate) fn check_cancel(cancel: &std::sync::atomic::AtomicBool) -> ResourceResult<()> {
    if cancel.load(std::sync::atomic::Ordering::Relaxed) {
        Err(ResourceError::Cancelled)
    } else {
        Ok(())
    }
}
pub(crate) fn invalid(message: impl Into<String>) -> ResourceError {
    ResourceError::Invalid(message.into())
}
pub(crate) fn matches_pattern(pattern: &str, text: &str) -> bool {
    fancy_regex::Regex::new(pattern)
        .expect("static resource pattern")
        .is_match(text)
        .unwrap_or(false)
}
