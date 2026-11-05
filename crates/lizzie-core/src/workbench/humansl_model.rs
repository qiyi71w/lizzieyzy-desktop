//! Java HumanSL model discovery: dedicated model paths only, never analysis weights.
use super::{resources::HUMAN_SL_MODEL, Workbench};
use crate::api::{HumanSlModelStatus, HumanSlModelView, HumanSlSettingsView};
use crate::resources::{self as r, Model};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub(super) struct VerifiedModel {
    size: u64,
    modified: Option<SystemTime>,
    sha256: String,
    valid: bool,
}

impl Workbench {
    pub(crate) fn human_sl_model_path(&self) -> Option<PathBuf> {
        let model = self.resources.catalog().models.get(HUMAN_SL_MODEL)?;
        self.config
            .human_sl
            .model_path
            .as_ref()
            .map(PathBuf::from)
            .into_iter()
            .chain(std::iter::once(
                self.resources.store().human_sl_models_dir().join(&model.file_name),
            ))
            .find(|path| self.human_sl_model_valid(path, model))
    }

    /// Read-only checksum evidence for consumers that require the official model even when
    /// training accepts a custom filename. Never hashes or changes T22's admission rules.
    pub(super) fn human_sl_official_verification(&self, path: &Path) -> Option<bool> {
        let meta = std::fs::metadata(path).ok()?;
        let modified = meta.modified().ok();
        self.resources.human_sl_verified.lock().unwrap().get(path)
            .filter(|entry| entry.size == meta.len() && entry.modified == modified
                && entry.sha256.eq_ignore_ascii_case(crate::strength::HUMAN_SHA))
            .map(|entry| entry.valid)
    }

    fn human_sl_model_valid(&self, path: &Path, model: &Model) -> bool {
        let Ok(meta) = std::fs::metadata(path) else {
            return false;
        };
        if !meta.is_file() {
            return false;
        }
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_ascii_lowercase();
        if !name.eq_ignore_ascii_case(&model.file_name) {
            return (name.ends_with(".bin.gz") || name.ends_with(".txt.gz")) && meta.len() > 1024 * 1024;
        }
        if meta.len() != model.size_bytes {
            return false;
        }
        let modified = meta.modified().ok();
        let mut cache = self.resources.human_sl_verified.lock().unwrap();
        if let Some(entry) = cache
            .get(path)
            .filter(|entry| entry.size == meta.len() && entry.modified == modified && entry.sha256 == model.sha256)
        {
            return entry.valid;
        }
        let valid = r::sha256_file(path).is_ok_and(|digest| digest.eq_ignore_ascii_case(&model.sha256));
        cache.insert(
            path.to_path_buf(),
            VerifiedModel {
                size: meta.len(),
                modified,
                sha256: model.sha256.clone(),
                valid,
            },
        );
        valid
    }

    pub(super) fn human_sl_settings_view(&self) -> HumanSlSettingsView {
        let path = self.human_sl_model_path();
        HumanSlSettingsView {
            model: HumanSlModelView {
                status: if path.is_some() {
                    HumanSlModelStatus::Installed
                } else {
                    HumanSlModelStatus::Missing
                },
                path: path.map(|p| p.to_string_lossy().into_owned()),
            },
            training: self.config.human_sl.training.clone().normalized(),
            advanced_visible: self.config.human_sl.advanced_visible,
        }
    }
}
