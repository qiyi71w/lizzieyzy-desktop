//! Application log (`logs/app.log`) and crash log (`logs/crash.log`) in the product's data
//! directory, the files Java's "诊断与日志" dialog points at. Record contents and full paths are
//! not logged; file names only.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Rotate `app.log` to `app.log.1` at start once it exceeds this size.
const ROTATE_BYTES: u64 = 1024 * 1024;

pub struct Log {
    file: Mutex<Option<File>>,
}

fn timestamp() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

/// File name of a path, for log lines.
pub fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default()
}

impl Log {
    pub fn start(dir: &Path) -> Log {
        let _ = std::fs::create_dir_all(dir);
        let path = dir.join("app.log");
        if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROTATE_BYTES) {
            let _ = std::fs::rename(&path, dir.join("app.log.1"));
        }
        let file = OpenOptions::new().create(true).append(true).open(&path).ok();
        install_panic_hook(dir.join("crash.log"));
        Log { file: Mutex::new(file) }
    }

    fn write(&self, level: &str, message: &str) {
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = writeln!(f, "{} {level} {message}", timestamp());
        }
    }

    pub fn info(&self, message: &str) {
        self.write("INFO", message);
    }

    pub fn warn(&self, message: &str) {
        self.write("WARN", message);
    }
}

fn install_panic_hook(crash: PathBuf) {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&crash) {
            let _ = writeln!(f, "{} PANIC {info}", timestamp());
        }
        default(info);
    }));
}
