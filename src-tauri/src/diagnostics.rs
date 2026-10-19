//! "诊断与日志" support: log status and the diagnostics package (Java `DiagnosticsDialog`,
//! `DiagnosticBundleExporter`), limited to the data this product produces so far: the
//! application, crash and engine trace logs, versions/runtime, and a redacted settings snapshot.

use std::io::Write;
use std::path::Path;

use lizzie_core::workbench::Workbench;

pub fn default_file_name() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("lizzieyzy-tauri-diagnostics-{secs}.zip")
}

pub struct Package {
    entries: Vec<(String, Vec<u8>)>,
}

impl Package {
    pub fn collect(wb: &Workbench, version: &str) -> Package {
        let mut entries = Vec::new();
        let logs = &wb.storage().logs_dir;
        for name in ["app.log", "app.log.1", "crash.log", "engine-trace.log"] {
            // Exported again through the sanitizer (`ExportSanitizer`): crash text and logs
            // written by earlier versions are not trusted to be clean.
            if let Ok(bytes) = std::fs::read(logs.join(name)) {
                let text = lizzie_core::sanitize::sanitize(&String::from_utf8_lossy(&bytes));
                entries.push((format!("logs/lizzie/{name}"), text.into_bytes()));
            }
        }
        let config = wb.config();
        let redacted = serde_json::json!({
            "record": config.record,
            "ui": config.ui,
            // Paths are personal data; only the count is exported.
            "recentFiles": config.recent_files.len(),
            // Engine command lines name local paths: only their count and the analysis settings.
            "engines": config.engine.engines.len(),
            "engineStartup": config.engine.startup,
            "analysis": config.engine.analysis,
        });
        entries.push((
            "snapshots/config.json".into(),
            serde_json::to_vec_pretty(&redacted).unwrap_or_default(),
        ));
        let versions = serde_json::json!({ "app": version });
        entries.push((
            "snapshots/versions.json".into(),
            serde_json::to_vec_pretty(&versions).unwrap_or_default(),
        ));
        let runtime = format!("os={}\narch={}\n", std::env::consts::OS, std::env::consts::ARCH);
        entries.push(("snapshots/runtime.txt".into(), runtime.into_bytes()));
        let manifest: Vec<&str> = entries.iter().map(|(n, _)| n.as_str()).collect();
        let manifest = serde_json::to_vec_pretty(&serde_json::json!({ "files": manifest })).unwrap_or_default();
        entries.push(("manifest.json".into(), manifest));
        Package { entries }
    }

    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("zip.part");
        {
            let file = std::fs::File::create(&tmp)?;
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            for (name, bytes) in &self.entries {
                zip.start_file(name.as_str(), options).map_err(std::io::Error::other)?;
                zip.write_all(bytes)?;
            }
            zip.finish().map_err(std::io::Error::other)?;
        }
        std::fs::rename(&tmp, path)
    }
}
