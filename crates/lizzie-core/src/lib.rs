//! LizzieYzy Tauri domain core. Independent of Tauri, the WebView and any UI toolkit.
//!
//! - [`record`]: game record tree and node semantics (document and rules).
//! - [`formats`]: SGF / GIB reading into candidate records.
//! - [`browse`]: where browsing commands land.
//! - [`workbench`]: application flow: open document, browsing position, open requests, settings.
//! - [`settings`]: committed settings and application data files.
//! - [`api`]: the typed cross-language contract.
//! - [`engine`]: engine settings, GTP protocol, running engine instances and processes.

pub mod api;
pub mod browse;
pub mod engine;
pub mod formats;
pub mod record;
pub mod settings;
pub mod workbench;
