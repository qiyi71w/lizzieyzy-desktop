//! The Java settings import as an application operation: preview, confirmation, one write of
//! this product's settings, and putting them into effect.
//!
//! Reading the Java files ([`crate::java_import::read`]) happens off the workbench (the adapter
//! reads on a worker thread); the workbench keeps the read candidate as the preview the user is
//! looking at. Applying it writes `config.json` and `persist.json` as one change (a failure leaves
//! both as they were and the settings in memory unchanged), then restarts the engines with the
//! imported engine list and start-up choice. The Java files are never written.

use super::Workbench;
use crate::api::{ApiError, EngineScope, ErrorKind, ImportPreviewView, ImportReportView, ImportStatus, JavaImportView};
use crate::java_import::{Candidate, CONFIG_FILE};
use crate::settings;

/// Refusal: the preview was replaced or already applied.
pub const IMPORT_STALE: &str = "product.importStale";
/// Refusal: the import would replace settings chosen in this product, and that was not confirmed.
pub const IMPORT_NEEDS_REPLACE: &str = "product.importNeedsReplace";

impl Workbench {
    /// Java's conventional work directories (see [`crate::java_import::conventional_dirs`]); those
    /// holding a `config.txt` are offered in the import dialog.
    pub fn set_java_import_locations(&mut self, dirs: Vec<std::path::PathBuf>) {
        let mut found: Vec<String> = Vec::new();
        for d in dirs {
            let shown = d.to_string_lossy().into_owned();
            if d.join(CONFIG_FILE).is_file() && !found.contains(&shown) {
                found.push(shown);
            }
        }
        self.java_candidates = found;
    }

    pub(super) fn java_import_view(&self) -> JavaImportView {
        JavaImportView {
            offer: self.new_profile,
            candidates: self.java_candidates.clone(),
        }
    }

    /// Keeps a read Java configuration as the preview the user is looking at (replacing any
    /// earlier one) and describes what importing it would do. Nothing is written.
    pub fn preview_java_import(&mut self, candidate: Candidate) -> ImportPreviewView {
        self.next_preview += 1;
        let id = self.next_preview;
        let view = ImportPreviewView {
            id,
            config_file: candidate.config_file.to_string_lossy().into_owned(),
            persist_file: candidate
                .persist_file
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            program_dir: candidate
                .source
                .program_dir
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned()),
            // As confirming every credential would import them; the report shows the choice made.
            items: candidate.items_after(&candidate.credential_engines()),
            engines: candidate.engines.clone(),
            replaces: self.java_import_replaces(&candidate),
        };
        self.java_preview = Some((id, candidate));
        view
    }

    /// Imports preview `preview`. `copy_credentials` are the Java engine indexes whose credentials
    /// the user agreed to copy; `replace` confirms replacing settings chosen in this product.
    /// `Ok(Err(key))` is a refusal with nothing changed; `Err` a failed write with nothing changed.
    pub fn apply_java_import(
        &mut self,
        preview: u64,
        copy_credentials: &[u32],
        replace: bool,
    ) -> Result<Result<ImportReportView, &'static str>, ApiError> {
        let Some((id, candidate)) = self.java_preview.take() else {
            return Ok(Err(IMPORT_STALE));
        };
        if id != preview {
            self.java_preview = Some((id, candidate));
            return Ok(Err(IMPORT_STALE));
        }
        // Settings may have changed since the preview was made.
        if !replace && !self.java_import_replaces(&candidate).is_empty() {
            self.java_preview = Some((id, candidate));
            return Ok(Err(IMPORT_NEEDS_REPLACE));
        }
        let (config, persisted) = candidate.mapped.apply(&self.config, &self.persisted, copy_credentials);
        // Also after "重置界面位置": the imported folder is an explicit choice; the window
        // placement it reset stays unsaved until the next start.
        let write_persist = persisted != self.persisted;
        if let Err(e) = settings::save_both(&self.storage, &config, write_persist.then_some(&persisted)) {
            // Nothing changed; the same preview can be tried again.
            self.java_preview = Some((id, candidate));
            return Err(ApiError::new(
                ErrorKind::Io,
                format!("saving imported settings failed: {e}"),
            ));
        }

        let previous_analysis = self.config.engine.analysis.clone();
        self.config = config;
        self.persisted = persisted;
        self.new_profile = false;
        self.analysis_settings_changed(&previous_analysis);
        // The imported engine list and start-up choice take over, as after a start.
        self.shutdown_engines(EngineScope::All);
        self.start_engines();

        let engines = candidate.engines_after(copy_credentials);
        let items = candidate.items_after(copy_credentials);
        let count = |s: ImportStatus| items.iter().filter(|i| i.status == s).count();
        self.log(
            false,
            format!(
                "java import from {}: {} imported, {} incompatible, {} not migrated; engines {} of {}",
                candidate.source.config_dir.display(),
                count(ImportStatus::Imported),
                count(ImportStatus::Incompatible),
                count(ImportStatus::NotMigrated),
                engines.iter().filter(|e| e.status == ImportStatus::Imported).count(),
                engines.len(),
            ),
        );
        Ok(Ok(ImportReportView {
            config_file: candidate.config_file.to_string_lossy().into_owned(),
            items,
            engines,
            restart_required: self.config.ui.language.resolve(self.system_locale.as_deref()) != self.active_language,
        }))
    }

    /// Settings of this product the candidate would change (see [`crate::java_import::Mapped::replaces`]).
    fn java_import_replaces(&self, candidate: &Candidate) -> Vec<crate::api::ImportTarget> {
        let has_settings = self.storage.config_file.exists();
        candidate.mapped.replaces(&self.config, &self.persisted, has_settings)
    }

    /// The first-use offer was declined: this product's settings file is written (with the
    /// current settings), so the next start is no longer a first use.
    pub fn dismiss_java_import_offer(&mut self) -> Result<bool, ApiError> {
        if !self.new_profile {
            return Ok(false);
        }
        if !self.storage.config_file.exists() {
            self.save_config()?;
        }
        self.new_profile = false;
        Ok(true)
    }
}
