//! Acceptance control channel, compiled only with the `acceptance` feature and started only when
//! `LIZZIEYZY_TAURI_ACCEPTANCE_PORT` is set. Loopback, newline-delimited JSON:
//!
//! - `{"command": <Command>}` → `{"result": <CommandResult>}` or `{"error": <ApiError>}`; the
//!   same [`crate::dispatch`] the UI uses.
//! - `{"query": "view"}` → `{"view": <WorkbenchView>}` (full view).
//! - `{"query": "ui"}` → `{"ui": <object>}`: the page's own report of focus, title, dialogs and
//!   visible text, collected through the `acceptance-probe` event.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::sync::Mutex;
use std::time::Duration;

use lizzie_core::api::Command;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::State;

static UI_REPLY: Mutex<Option<mpsc::Sender<Value>>> = Mutex::new(None);

#[derive(Deserialize)]
struct Request {
    command: Option<Command>,
    query: Option<String>,
}

#[tauri::command]
pub fn acceptance_ui_report(report: Value) {
    if let Some(tx) = UI_REPLY.lock().unwrap().take() {
        let _ = tx.send(report);
    }
}

fn handle(app: &AppHandle, line: &str) -> Value {
    let req: Request = match serde_json::from_str(line) {
        Ok(r) => r,
        Err(e) => return json!({ "error": { "kind": "InvalidArgument", "message": e.to_string() } }),
    };
    let shared = app.state::<State>().inner().clone();
    if let Some(command) = req.command {
        let result = tauri::async_runtime::block_on(crate::dispatch(app.clone(), shared, command));
        return match result {
            Ok(r) => json!({ "result": r }),
            Err(e) => json!({ "error": e }),
        };
    }
    match req.query.as_deref() {
        Some("view") => json!({ "view": shared.workbench.lock().unwrap().inspect() }),
        Some("ui") => {
            let (tx, rx) = mpsc::channel();
            *UI_REPLY.lock().unwrap() = Some(tx);
            let _ = app.emit("acceptance-probe", ());
            match rx.recv_timeout(Duration::from_secs(5)) {
                Ok(report) => json!({ "ui": report }),
                Err(_) => json!({ "error": { "kind": "Unavailable", "message": "no UI report" } }),
            }
        }
        _ => json!({ "error": { "kind": "InvalidArgument", "message": "expected command or query" } }),
    }
}

pub fn start(app: AppHandle, shared: State) {
    let Ok(port) = std::env::var("LIZZIEYZY_TAURI_ACCEPTANCE_PORT") else {
        return;
    };
    let listener = match TcpListener::bind(("127.0.0.1", port.parse::<u16>().unwrap_or(0))) {
        Ok(l) => l,
        Err(e) => {
            shared.log.warn(&format!("acceptance channel not started: {e}"));
            return;
        }
    };
    shared
        .log
        .info(&format!("acceptance channel on {:?}", listener.local_addr().ok()));
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let app = app.clone();
            std::thread::spawn(move || {
                let Ok(reader_stream) = stream.try_clone() else { return };
                let mut writer = stream;
                for line in BufReader::new(reader_stream).lines() {
                    let Ok(line) = line else { break };
                    if line.trim().is_empty() {
                        continue;
                    }
                    let reply = handle(&app, &line);
                    if writeln!(writer, "{reply}").is_err() {
                        break;
                    }
                }
            });
        }
    });
}
