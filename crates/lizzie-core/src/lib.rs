//! LizzieYzy Tauri domain core. Independent of Tauri, the WebView and any UI toolkit.
//!
//! - [`record`]: game record tree and node semantics (document and rules).
//! - [`formats`]: SGF / GIB reading into candidate records, SGF writing and exports, pictures.
//! - [`browse`]: where browsing commands land.
//! - [`workbench`]: application flow: open document, browsing position, open and save requests,
//!   engines, settings, Java import.
//! - [`settings`]: committed settings and application data files.
//! - [`java_import`]: reading the fixed Java program's configuration for an explicit import.
//! - [`recovery`]: 存档与读档 slots and the automatic saves.
//! - [`viewpoint`]: whose win rate and score lead the displays and exports show.
//! - [`api`]: the typed cross-language contract.
//! - [`engine`]: engine settings, GTP protocol, running engine instances and processes.
//! - [`resources`]: engine resources: trusted catalog, machine detection, recommendation,
//!   acquisition and verification.

pub mod api;
pub mod appearance;
pub mod browse;
pub mod display;
pub mod engine;
pub mod formats;
pub mod function_search;
pub mod humansl;
pub mod java_import;
pub mod play;
pub mod record;
pub mod recovery;
pub mod resources;
pub mod sanitize;
pub mod scoring;
pub mod settings;
pub mod tsumego;
pub mod viewpoint;
pub mod workbench;
