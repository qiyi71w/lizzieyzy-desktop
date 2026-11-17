//! "诊断与日志" support: log status and the diagnostics package (Java `DiagnosticsDialog`,
//! `DiagnosticBundleExporter`), limited to the data this product produces so far: the
//! application and crash logs, the 完整日志 files, versions/runtime, a redacted settings snapshot, and
//! the 棋盘同步 helper's logs, logging state and sync snapshots (`SyncDiagnosticsExporter`).

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
        // Exported again through the sanitizer (`ExportSanitizer`): crash text and logs written by
        // earlier versions or by the helper are not trusted to be clean.
        let sanitized = |path: std::path::PathBuf| {
            std::fs::read(path).ok().map(|bytes| lizzie_core::sanitize::sanitize(&String::from_utf8_lossy(&bytes)))
        };
        let traces = lizzie_core::diagnostics::TraceScope::ALL.map(|scope| scope.file_name());
        for name in ["app.log", "app.log.1", "crash.log"].into_iter().chain(traces) {
            if let Some(text) = sanitized(logs.join(name)) {
                entries.push((format!("logs/lizzie/{name}"), text.into_bytes()));
            }
        }
        // The helper writes its own logs under `<logs>/readboard` (`--log-dir`).
        for name in ["app.log", "crash.log"] {
            if let Some(text) = sanitized(logs.join("readboard").join(name)) {
                entries.push((format!("logs/readboard/{name}"), text.into_bytes()));
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
            // Java `ConfigExportProjection`: the diagnostics switches and the running session.
            "logging": config.logging,
            "activeTraceScopes": wb.diagnostics().full_trace(),
        });
        entries.push((
            "snapshots/config.json".into(),
            serde_json::to_vec_pretty(&redacted).unwrap_or_default(),
        ));
        let readboard = wb.readboard_diagnostics(version);
        let versions = serde_json::json!({ "app": version, "readboard": readboard.helper_version });
        entries.push((
            "snapshots/versions.json".into(),
            serde_json::to_vec_pretty(&versions).unwrap_or_default(),
        ));
        entries.push((
            "snapshots/readboard-observed.json".into(),
            lizzie_core::sanitize::sanitize(&readboard.observed).into_bytes(),
        ));
        for (name, text) in readboard.snapshots {
            entries.push((format!("snapshots/{name}"), lizzie_core::sanitize::sanitize(&text).into_bytes()));
        }
        let runtime = format!("os={}\narch={}\n", std::env::consts::OS, std::env::consts::ARCH);
        entries.push(("snapshots/runtime.txt".into(), runtime.into_bytes()));
        let resources = wb.resource_snapshot();
        let summary = serde_json::json!({
            "machine": resources.machine,
            "packages": resources.inventory.packages.iter().map(|p| serde_json::json!({
                "asset": p.receipt.asset_id, "version": p.receipt.version,
                "source": p.receipt.source, "usable": p.usable,
            })).collect::<Vec<_>>(),
            "weights": resources.inventory.weights.iter().map(|w| serde_json::json!({
                "model": w.model_name, "size": w.size, "catalogModel": w.catalog_model_id,
            })).collect::<Vec<_>>(),
            "acceleration": resources.acceleration,
            "proxyMode": resources.network.proxy_mode,
        });
        let summary = lizzie_core::sanitize::sanitize(&serde_json::to_string_pretty(&summary).unwrap_or_default());
        entries.push(("snapshots/resources.json".into(), summary.into_bytes()));
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
