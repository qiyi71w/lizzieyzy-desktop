use super::{
    calibrator::{self, Calibrator},
    xgboost::{self, Booster, DEFAULT_FEATURE_INDICES, FULL_FEATURE_NAMES},
    StrengthModelView,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{self, Read},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub struct StrengthModel {
    pub id: String,
    pub name: String,
    pub human_sl: bool,
    pub prediction_sigma: f64,
    indices: Vec<usize>,
    booster: Option<Arc<Booster>>,
    calibrator: Option<Arc<Calibrator>>,
    available: bool,
}
impl StrengthModel {
    pub fn xgboost20tun() -> Self {
        Self {
            id: "bundled:xgboost20tun".into(),
            name: "xgboost20tun".into(),
            human_sl: false,
            prediction_sigma: 0.92,
            indices: DEFAULT_FEATURE_INDICES.to_vec(),
            booster: None,
            calibrator: None,
            available: true,
        }
    }
    pub fn humansl40() -> Self {
        Self {
            id: "bundled:humansl40".into(),
            name: "HumanSL 40".into(),
            human_sl: true,
            prediction_sigma: 0.92,
            indices: vec![0; 40],
            booster: None,
            calibrator: None,
            available: true,
        }
    }
    pub fn view(&self) -> StrengthModelView {
        StrengthModelView {
            id: self.id.clone(),
            name: self.name.clone(),
            human_sl: self.human_sl,
        }
    }
    pub fn is_available(&self) -> bool {
        self.available
    }
    pub(super) fn predict(&self, full: &[f64; 29]) -> Option<f64> {
        if !self.available || self.human_sl {
            return None;
        }
        let mut stack = [0.0; 29];
        let selected;
        let features = if self.indices.len() <= stack.len() {
            for (value, &index) in stack.iter_mut().zip(&self.indices) {
                *value = full[index];
            }
            &stack[..self.indices.len()]
        } else {
            selected = self.indices.iter().map(|&i| full[i]).collect::<Vec<_>>();
            &selected
        };
        let booster = self.booster.as_deref().unwrap_or_else(|| xgboost::bundled());
        let raw = super::clamp(booster.predict(features).ok()?, -1.0, 12.0);
        // Java calibratorFor(null, null) resolves the default even for imported boosters.
        Some(
            self.calibrator
                .as_deref()
                .unwrap_or_else(|| calibrator::bundled())
                .apply(raw, full),
        )
    }
}
fn absolute(path: &Path) -> io::Result<PathBuf> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => normalized.push(other.as_os_str()),
        }
    }
    Ok(normalized)
}
fn error(message: impl std::fmt::Display) -> io::Error {
    io::Error::other(message.to_string())
}
#[derive(Clone, Debug)]
struct ModelSpec {
    name: String,
    indices: Vec<usize>,
    sigma: f64,
}
impl Default for ModelSpec {
    fn default() -> Self {
        Self {
            name: String::new(),
            indices: DEFAULT_FEATURE_INDICES.to_vec(),
            sigma: 0.92,
        }
    }
}
impl ModelSpec {
    fn load(path: &Path) -> io::Result<Self> {
        Self::parse(&fs::read(path)?)
    }
    fn parse(bytes: &[u8]) -> io::Result<Self> {
        let root: Value = serde_json::from_slice(bytes).map_err(error)?;
        let first = |names: &[&str]| {
            names
                .iter()
                .find_map(|n| root[*n].as_str().filter(|s| !s.trim().is_empty()).map(str::trim))
                .unwrap_or("")
                .to_owned()
        };
        let kind = first(&["model_type", "type"]);
        if !kind.is_empty() && kind != "xgboost_rank_value" && kind != "xgboost20tun" {
            return Err(error(format!("Unsupported strength model_type: {kind}")));
        }
        let order = root["feature_order"]
            .as_array()
            .or_else(|| root["final_features"].as_array())
            .filter(|a| !a.is_empty())
            .ok_or_else(|| error("metadata.json must contain non-empty feature_order."))?;
        let mut indices = Vec::with_capacity(order.len());
        for name in order {
            let name = name.as_str().ok_or_else(|| error("Invalid feature name"))?.trim();
            indices.push(
                super::full_feature_index(name)
                    .ok_or_else(|| error(format!("Unknown feature in metadata feature_order: {name}")))?,
            );
        }
        let sigma = if root.get("prediction_sigma").is_some() {
            root["prediction_sigma"].as_f64()
        } else {
            root["sigma"].as_f64()
        }
        .unwrap_or(0.92);
        Ok(Self {
            name: first(&["display_name", "model_name", "name"]),
            indices,
            sigma: if sigma > 0.0 { sigma } else { 0.92 },
        })
    }
    fn json(&self) -> Value {
        json!({"model_type":"xgboost_rank_value","feature_order":self.indices.iter().map(|&i| FULL_FEATURE_NAMES[i]).collect::<Vec<_>>(),"prediction_sigma":self.sigma})
    }
}
/// The directory's metadata (an unreadable one keeps no features) and `displayName`: the
/// display-name.txt, else the metadata's name, else the directory's.
fn spec_and_name(dir: &Path) -> (ModelSpec, String) {
    let spec = if dir.join("metadata.json").is_file() {
        ModelSpec::load(&dir.join("metadata.json")).unwrap_or_else(|_| ModelSpec {
            name: String::new(),
            indices: vec![],
            sigma: 0.92,
        })
    } else {
        ModelSpec::default()
    };
    let name = fs::read_to_string(dir.join("display-name.txt"))
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.trim().to_owned())
        .unwrap_or_else(|| {
            if !spec.name.is_empty() {
                spec.name.clone()
            } else {
                dir.file_name().unwrap_or_default().to_string_lossy().into_owned()
            }
        });
    (spec, name)
}
/// `cached`: models listed before, whose parsed booster and calibrator are reused by directory
/// (XGBoostStrengthModel's EXTERNAL_MODELS keeps a parsed booster per path).
fn from_directory(dir: &Path, cached: &[StrengthModel]) -> Option<StrengthModel> {
    let booster_path = dir.join("booster.json");
    if !booster_path.is_file() {
        return None;
    }
    let (spec, name) = spec_and_name(dir);
    let id = format!("file:{}", absolute(dir).ok()?.display());
    let previous = cached.iter().find(|m| m.id == id);
    let booster = previous.and_then(|m| m.booster.clone()).or_else(|| {
        fs::read(&booster_path)
            .ok()
            .and_then(|b| Booster::parse(&b).ok())
            .map(Arc::new)
    });
    let calibrator_path = dir.join("calibrator.json");
    let calibrator = previous.and_then(|m| m.calibrator.clone()).or_else(|| {
        fs::read(&calibrator_path)
            .ok()
            .and_then(|b| Calibrator::parse(&b).ok())
            .map(Arc::new)
    });
    let available = booster.as_ref().is_some_and(|b| b.feature_count == spec.indices.len())
        && (!calibrator_path.is_file() || calibrator.is_some());
    Some(StrengthModel {
        id,
        name,
        human_sl: false,
        prediction_sigma: spec.sigma,
        indices: spec.indices,
        booster,
        calibrator,
        available,
    })
}
pub fn selectable_models(models_dir: &Path, cached: &[StrengthModel]) -> Vec<StrengthModel> {
    let mut imported: Vec<_> = fs::read_dir(models_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| from_directory(&e.path(), cached))
        .filter(StrengthModel::is_available)
        .collect();
    imported.sort_by_key(|m| m.name.to_lowercase());
    let mut result = vec![StrengthModel::xgboost20tun(), StrengthModel::humansl40()];
    result.extend(imported);
    result
}
pub fn effective_model(id: &str, models_dir: &Path) -> StrengthModel {
    selectable_models(models_dir, &[])
        .into_iter()
        .find(|m| m.id == id)
        .unwrap_or_else(StrengthModel::xgboost20tun)
}
fn sibling(source: &Path, calibrator: bool) -> Option<PathBuf> {
    let parent = source.parent()?;
    let name = source.file_name()?.to_string_lossy();
    let keyword = if calibrator { "calibrator" } else { "metadata" };
    let mut candidates = Vec::new();
    if name.contains("booster") {
        candidates.push(parent.join(name.replace("booster", keyword)));
    }
    candidates.push(parent.join(format!("{keyword}.json")));
    candidates.push(parent.join(if calibrator {
        "xgboost20tun_residual_calibrator.json"
    } else {
        "model_metadata.json"
    }));
    let valid = |p: &PathBuf| {
        p.is_file() && (!calibrator || fs::read(p).ok().and_then(|b| Calibrator::parse(&b).ok()).is_some())
    };
    if let Some(p) = candidates.into_iter().find(&valid) {
        return Some(p);
    }
    let mut all: Vec<_> = fs::read_dir(parent)
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            let s = p.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            s.ends_with(".json") && s.contains(keyword)
        })
        .collect();
    all.sort();
    all.into_iter().find(valid)
}
fn slug(value: &str) -> String {
    let mut result = String::new();
    let mut invalid = false;
    for c in value.chars() {
        if c.is_ascii_alphanumeric() || "._-".contains(c) {
            result.push(c);
            invalid = false;
        } else if !invalid {
            result.push('-');
            invalid = true;
        }
    }
    let result = result.trim_matches('-');
    if result.is_empty() {
        "xgboost20tun".into()
    } else {
        result.into()
    }
}
fn unique(root: &Path, name: &str) -> PathBuf {
    let mut p = root.join(name);
    let mut n = 2;
    while p.exists() {
        p = root.join(format!("{name}-{n}"));
        n += 1;
    }
    p
}
fn stem(path: &Path) -> String {
    let s = path.file_name().unwrap_or_default().to_string_lossy();
    match s.rfind('.') {
        Some(i) if i > 0 => s[..i].into(),
        _ => s.into_owned(),
    }
}
fn validate_booster(path: &Path, spec: &ModelSpec, metadata: bool) -> io::Result<()> {
    let b = fs::read(path)
        .ok()
        .and_then(|b| Booster::parse(&b).ok())
        .ok_or_else(|| error("The selected file is not a readable XGBoost JSON booster."))?;
    if b.feature_count == spec.indices.len() {
        return Ok(());
    }
    Err(error(if !metadata {
        format!("This booster has {} features. Add metadata.json with feature_order, or package booster.json and metadata.json in a zip.",b.feature_count)
    } else {
        format!(
            "Booster feature count {} does not match metadata feature_order length {}.",
            b.feature_count,
            spec.indices.len()
        )
    }))
}
/// Imports JSON plus sibling metadata/calibrator, or a ZIP bundle, into a caller-owned directory.
/// Only the three recognized leaves are extracted; archive paths are never used as destinations.
/// Any newly created model directory is removed when importing fails.
pub fn import_model(source: &Path, models_dir: &Path) -> io::Result<StrengthModel> {
    if source.as_os_str().is_empty() {
        return Err(error("No model file selected."));
    }
    let source = absolute(source)?;
    if !source.is_file() {
        return Err(error(format!("Model file does not exist: {}", source.display())));
    }
    let is_zip = source
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase()
        .ends_with(".zip");
    let metadata = if is_zip { None } else { sibling(&source, false) };
    let spec = if let Some(p) = &metadata {
        ModelSpec::load(p)?
    } else {
        ModelSpec::default()
    };
    if !is_zip {
        validate_booster(&source, &spec, metadata.is_some())?;
    }
    fs::create_dir_all(models_dir)?;
    let name = if spec.name.is_empty() {
        stem(&source)
    } else {
        spec.name.clone()
    };
    let target = unique(models_dir, &slug(&name));
    let imported = (|| {
        fs::create_dir_all(&target)?;
        if is_zip {
            let mut zip = zip::ZipArchive::new(fs::File::open(&source)?).map_err(error)?;
            let mut found: [Option<usize>; 3] = [None, None, None];
            for (slot, (exact, keyword)) in [
                ("booster.json", "booster"),
                ("metadata.json", "metadata"),
                ("calibrator.json", "calibrator"),
            ]
            .iter()
            .enumerate()
            {
                for i in 0..zip.len() {
                    let entry = zip.by_index(i).map_err(error)?;
                    if entry.is_dir() {
                        continue;
                    }
                    let leaf = entry
                        .name()
                        .rsplit(['/', '\\'])
                        .next()
                        .unwrap_or_default()
                        .to_lowercase();
                    if leaf == *exact {
                        found[slot] = Some(i);
                        break;
                    }
                    if found[slot].is_none() && leaf.ends_with(".json") && leaf.contains(*keyword) {
                        found[slot] = Some(i);
                    }
                }
            }
            if found[0].is_none() {
                return Err(error("Model bundle is missing booster.json."));
            }
            if found[1].is_none() {
                return Err(error("Model bundle is missing metadata.json with feature_order."));
            }
            for (i, leaf) in ["booster.json", "metadata.json", "calibrator.json"].iter().enumerate() {
                if let Some(index) = found[i] {
                    let mut b = Vec::new();
                    zip.by_index(index).map_err(error)?.read_to_end(&mut b)?;
                    fs::write(target.join(leaf), b)?;
                }
            }
        } else {
            fs::copy(&source, target.join("booster.json"))?;
            fs::write(target.join("display-name.txt"), &name)?;
            if let Some(p) = &metadata {
                fs::copy(p, target.join("metadata.json"))?;
            } else {
                fs::write(
                    target.join("metadata.json"),
                    serde_json::to_vec_pretty(&spec.json()).map_err(error)?,
                )?;
            }
            if let Some(p) = sibling(&source, true) {
                fs::copy(p, target.join("calibrator.json"))?;
            }
        }
        let imported = from_directory(&target, &[])
            .filter(StrengthModel::is_available)
            .ok_or_else(|| {
                error(if is_zip {
                    "Imported model bundle did not pass runtime validation."
                } else {
                    "Imported model did not pass runtime validation."
                })
            })?;
        if is_zip {
            fs::write(target.join("display-name.txt"), &imported.name)?;
        }
        Ok(imported)
    })();
    if imported.is_err() {
        let _ = fs::remove_dir_all(&target);
    }
    imported
}
/// The files a model directory holds (`importJsonModel` / `importZipModel` write no others).
const MODEL_FILES: [&str; 4] = ["booster.json", "metadata.json", "calibrator.json", "display-name.txt"];
/// Java's model directories (`strength-models/<name>/booster.json`), sorted by name.
pub fn java_models(java_dir: &Path) -> Vec<PathBuf> {
    let mut dirs: Vec<_> = fs::read_dir(java_dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("booster.json").is_file())
        .collect();
    dirs.sort();
    dirs
}
/// The same model: booster, metadata, calibrator and the name it is shown by (a renamed copy
/// carries that name in its display-name.txt).
fn same_model(a: &Path, b: &Path) -> bool {
    MODEL_FILES[..3]
        .iter()
        .all(|f| fs::read(a.join(f)).ok() == fs::read(b.join(f)).ok())
        && spec_and_name(a).1 == spec_and_name(b).1
}
/// Model directories copied by a settings import; removed when dropped before [`Self::keep`].
pub struct ModelCopies(Vec<PathBuf>);
impl ModelCopies {
    pub fn keep(mut self) {
        self.0.clear();
    }
}
impl Drop for ModelCopies {
    fn drop(&mut self) {
        for dir in &self.0 {
            let _ = fs::remove_dir_all(dir);
        }
    }
}
/// Copies Java's model directories into `models_dir` under their own names (numbered beside a
/// different model of that name; a model copied before is not copied again). A renamed copy
/// keeps the name Java showed. The Java folder is only read.
pub fn copy_java_models(java_dir: &Path, models_dir: &Path) -> io::Result<ModelCopies> {
    let mut copies = ModelCopies(Vec::new());
    for source in java_models(java_dir) {
        let name = source.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let existing = models_dir.join(&name);
        let copied = fs::read_dir(models_dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|e| e.path().join("booster.json").is_file() && same_model(&source, &e.path()));
        if copied {
            continue;
        }
        let target = unique(models_dir, &name);
        fs::create_dir_all(&target)?;
        copies.0.push(target.clone());
        for file in MODEL_FILES {
            if source.join(file).is_file() {
                fs::copy(source.join(file), target.join(file))?;
            }
        }
        if target != existing && !source.join("display-name.txt").is_file() {
            fs::write(target.join("display-name.txt"), spec_and_name(&source).1)?;
        }
    }
    Ok(copies)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_and_unique_names() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.json");
        fs::write(
            &source,
            include_bytes!("../../resources/strength/xgboost20tun_booster.json"),
        )
        .unwrap();
        let dir = temp.path().join("models");
        let a = import_model(&source, &dir).unwrap();
        let b = import_model(&source, &dir).unwrap();
        assert_ne!(a.id, b.id);
        assert_eq!(selectable_models(&dir, &[]).len(), 4);
        assert_eq!(effective_model("missing", &dir).id, "bundled:xgboost20tun");
    }
    #[test]
    fn failed_bundle_cleans_directory() {
        use std::io::Write;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("bad.zip");
        let mut z = zip::ZipWriter::new(fs::File::create(&source).unwrap());
        z.start_file("metadata.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        z.write_all(b"{}").unwrap();
        z.finish().unwrap();
        let dir = temp.path().join("models");
        assert_eq!(
            import_model(&source, &dir).unwrap_err().to_string(),
            "Model bundle is missing booster.json."
        );
        assert_eq!(fs::read_dir(dir).unwrap().count(), 0);
    }
}
