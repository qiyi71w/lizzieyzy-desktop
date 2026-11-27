//! Engines (引擎运行): configured engines, the GTP protocol, the running instances and the
//! real process adapter.
//!
//! - [`profile`]: committed engine settings.
//! - [`gtp`]: command lines, response framing, analysis lines, vertices.
//! - [`kata_analysis`]: the KataGo analysis engine's JSON queries and answers.
//! - [`analysis_command`]: the analysis engine's command line.
//! - [`rules`]: KataGo rules values.
//! - [`runtime`]: instances, command confirmation, request ownership, foreground lease, analysis
//!   processes and exit.
//! - [`process`]: the OS process launcher.
//! - [`diagnosis`]: why an engine could not start (Windows path error, too old a macOS).
//! - [`ssh`]: engines that run on an SSH host.
//! - [`tuning`]: thread policy, benchmark results and GTP configuration of a profile.

pub mod analysis_command;
pub mod benchmark;
pub mod cloud;
pub mod diagnosis;
pub mod gtp;
pub mod gtp_config;
pub mod kata_analysis;
pub mod process;
pub mod profile;
pub mod raw_net;
pub mod rules;
pub mod runtime;
pub mod ssh;
pub mod tuning;
