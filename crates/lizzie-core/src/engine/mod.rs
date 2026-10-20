//! Engines (引擎运行): configured engines, the GTP protocol, the running instances and the
//! real process adapter.
//!
//! - [`profile`]: committed engine settings.
//! - [`gtp`]: command lines, response framing, analysis lines, vertices.
//! - [`rules`]: KataGo rules values.
//! - [`runtime`]: instances, command confirmation, request ownership, foreground and exit.
//! - [`process`]: the OS process launcher.
//! - [`tuning`]: thread policy, benchmark results and GTP configuration of a profile.

pub mod benchmark;
pub mod gtp;
pub mod gtp_config;
pub mod process;
pub mod profile;
pub mod rules;
pub mod runtime;
pub mod tuning;
