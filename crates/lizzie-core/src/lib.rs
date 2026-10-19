//! LizzieYzy Tauri domain core. Independent of Tauri, the WebView and any UI toolkit.
//!
//! - [`record`]: game record tree and node semantics (document and rules).
//! - [`formats`]: SGF / GIB reading into candidate records, SGF writing.
//! - [`browse`]: where browsing commands land.
//! - [`workbench`]: application flow: open document, browsing position, open and save requests,
//!   engines, settings, Java import.
//! - [`settings`]: committed settings and application data files.
//! - [`java_import`]: reading the fixed Java program's configuration for an explicit import.
//! - [`api`]: the typed cross-language contract.
//! - [`engine`]: engine settings, GTP protocol, running engine instances and processes.

pub mod api;
pub mod browse;
pub mod engine;
pub mod formats;
pub mod java_import;
pub mod record;
pub mod sanitize;
pub mod settings;
pub mod workbench;
