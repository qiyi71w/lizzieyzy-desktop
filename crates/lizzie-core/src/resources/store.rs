//! Java `KataGoExperimentalBackendInstaller.installArchive`, secure extraction and weight import.
//! Every replacement is prepared beside its destination before an installed directory is moved.
use super::{
    check_cancel, invalid, is_runtime_license, read_weight_model_name, sha256_file, should_extract_runtime_dll,
    verify_file, Asset, Catalog, ExperimentalBackend, ResourceError, ResourceResult, RuntimePackage,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

static NEXT_STAGE: AtomicU64 = AtomicU64::new(1);
pub(crate) fn sibling_path(target: &Path, purpose: &str) -> PathBuf {
    let name = target.file_name().unwrap_or_default().to_string_lossy();
    target.with_file_name(format!(
        "{name}.{purpose}-{}-{}",
        std::process::id(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    ))
}
fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
}
struct Staging(PathBuf);
impl Staging {
    fn new(target: &Path) -> ResourceResult<Self> {
        let parent = target.parent().ok_or_else(|| invalid("Invalid resource destination"))?;
        fs::create_dir_all(parent)?;
        loop {
            let path = sibling_path(target, "installing");
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e.into()),
            }
        }
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum ResourceSource {
    Download,
    Import,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceReceipt {
    pub asset_id: String,
    pub version: String,
    pub archive_sha256: String,
    pub executable_sha256: String,
    pub executable: String,
    pub executable_size: u64,
    pub source: ResourceSource,
    pub installed_at: u64,
    pub experimental_backend: Option<ExperimentalBackend>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct InstalledPackage {
    pub directory: PathBuf,
    pub executable: PathBuf,
    pub receipt: ResourceReceipt,
    pub usable: bool,
}
impl InstalledPackage {
    pub fn is_current(&self, asset: &Asset) -> bool {
        self.usable
            && self.receipt.asset_id == asset.id
            && self.receipt.archive_sha256.eq_ignore_ascii_case(&asset.sha256)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WeightReceipt {
    pub sha256: String,
    pub size: u64,
    pub modified_nanos: u128,
    pub catalog_model_id: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct InstalledWeight {
    pub path: PathBuf,
    pub size: u64,
    pub model_name: Option<String>,
    pub catalog_model_id: Option<String>,
    pub sha256: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ResourceInventory {
    pub packages: Vec<InstalledPackage>,
    pub weights: Vec<InstalledWeight>,
    pub gtp_config: Option<PathBuf>,
    pub analysis_config: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct ResourceStore {
    root: PathBuf,
}
impl ResourceStore {
    /// Pass `Storage::resources_dir`; nothing outside this root is scanned.
    pub fn new(resources_dir: impl Into<PathBuf>) -> Self {
        Self {
            root: resources_dir.into(),
        }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn downloads_dir(&self) -> PathBuf {
        self.root.join("downloads")
    }
    pub fn weights_dir(&self) -> PathBuf {
        self.root.join("weights")
    }
    pub fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime/nvidia")
    }
    pub fn runtime_downloads_dir(&self) -> PathBuf {
        self.runtime_dir().join("downloads")
    }
    pub fn gtp_config(&self) -> PathBuf {
        self.root.join("configs/gtp.cfg")
    }
    pub fn analysis_config(&self) -> PathBuf {
        self.root.join("configs/analysis.cfg")
    }
    pub fn package_dir(&self, asset: &Asset, backend: Option<ExperimentalBackend>) -> ResourceResult<PathBuf> {
        safe_id(&asset.id)?;
        super::validate_sha256(&asset.sha256)?;
        let digest = asset.sha256[..12].to_ascii_lowercase();
        if let Some(backend) = backend {
            if backend.asset_id() != asset.id {
                return Err(invalid("Experimental backend does not match package"));
            }
            Ok(self
                .root
                .join("engines")
                .join(format!("{}-{digest}", backend.install_dir())))
        } else {
            Ok(self.root.join("engines").join(format!("{}-{digest}", asset.id)))
        }
    }
    /// The catalog identity and both archive checks are validated before extraction or replacement.
    pub fn install_package(
        &self,
        catalog: &Catalog,
        asset_id: &str,
        archive: &Path,
        source: ResourceSource,
        backend: Option<ExperimentalBackend>,
        cancel: &AtomicBool,
    ) -> ResourceResult<InstalledPackage> {
        let asset = catalog.asset(asset_id)?;
        check_cancel(cancel)?;
        verify_file(archive, Some(asset.size_bytes), Some(&asset.sha256))?;
        self.install_verified_package(asset, archive, source, backend, cancel)
    }
    fn install_verified_package(
        &self,
        asset: &Asset,
        archive: &Path,
        source: ResourceSource,
        backend: Option<ExperimentalBackend>,
        cancel: &AtomicBool,
    ) -> ResourceResult<InstalledPackage> {
        check_cancel(cancel)?;
        let target = self.package_dir(asset, backend)?;
        let stage = Staging::new(&target)?;
        extract_package(archive, &stage.0, cancel)?;
        for required in &asset.required_files {
            let path = stage.0.join(required);
            let present = if required.ends_with('/') {
                path.is_dir()
            } else {
                path.is_file()
            };
            if !present {
                return Err(invalid(format!("Package lacks {required}")));
            }
        }
        let executable = stage.0.join(&asset.executable);
        let executable_digest = verify_file(
            &executable,
            Some(asset.executable_size_bytes),
            Some(&asset.executable_sha256),
        )?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let permissions = fs::metadata(&executable)?.permissions().mode();
            fs::set_permissions(&executable, fs::Permissions::from_mode(permissions | 0o111))?;
        }
        let relative = executable
            .strip_prefix(&stage.0)
            .map_err(|_| invalid("Executable outside staging"))?
            .to_string_lossy()
            .into_owned();
        let receipt = ResourceReceipt {
            asset_id: asset.id.clone(),
            version: asset.version.clone(),
            archive_sha256: asset.sha256.clone(),
            executable_sha256: executable_digest,
            executable: relative.clone(),
            executable_size: fs::metadata(&executable)?.len(),
            source,
            installed_at: epoch_millis(),
            experimental_backend: backend,
        };
        write_json(&stage.0.join("resource.json"), &receipt)?;
        // Prepare missing configuration copies before touching the installed package.
        let config_stage = Staging::new(&self.root.join("configs"))?;
        let mut pending_configs = Vec::new();
        for (file, destination) in [
            ("default_gtp.cfg", self.gtp_config()),
            ("analysis_example.cfg", self.analysis_config()),
        ] {
            if !destination.exists() {
                let source = stage.0.join(file);
                if !source.is_file() {
                    return Err(invalid(format!("Package lacks {file}")));
                }
                let staged = config_stage.0.join(destination.file_name().expect("config name"));
                copy_file(&source, &staged, cancel)?;
                pending_configs.push((staged, destination));
            }
        }
        check_cancel(cancel)?;
        let backup = move_aside(&target)?;
        if let Err(error) = fs::rename(&stage.0, &target) {
            restore(&target, backup.as_deref())?;
            return Err(error.into());
        }
        let mut written_configs = Vec::new();
        let finish = (|| -> ResourceResult<()> {
            fs::create_dir_all(self.root.join("configs"))?;
            for (staged, destination) in &pending_configs {
                check_cancel(cancel)?;
                // Hard-link is an atomic create-if-absent, preserving even a concurrent user edit.
                match fs::hard_link(staged, destination) {
                    Ok(()) => written_configs.push(destination.clone()),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e.into()),
                }
            }
            check_cancel(cancel)?;
            Ok(())
        })();
        if let Err(error) = finish {
            for config in written_configs {
                let _ = fs::remove_file(config);
            }
            restore(&target, backup.as_deref())?;
            return Err(error);
        }
        if let Some(backup) = backup {
            let _ = fs::remove_dir_all(backup);
        }
        Ok(InstalledPackage {
            directory: target.clone(),
            executable: target.join(relative),
            receipt,
            usable: true,
        })
    }
    /// A mismatched archive cannot alter engines, configs or receipts.
    pub fn import_package(
        &self,
        catalog: &Catalog,
        platform: &str,
        archive: &Path,
        backend: Option<ExperimentalBackend>,
        cancel: &AtomicBool,
    ) -> ResourceResult<InstalledPackage> {
        check_cancel(cancel)?;
        let metadata = fs::metadata(archive)?;
        if !metadata.is_file() {
            return Err(ResourceError::ImportMismatch);
        }
        let size = metadata.len();
        let candidates: Vec<&Asset> = catalog
            .assets_for_platform(platform)
            .filter(|a| a.size_bytes == size)
            .collect();
        if candidates.is_empty() {
            return Err(ResourceError::ImportMismatch);
        }
        let digest = sha256_file(archive)?;
        let asset = candidates
            .into_iter()
            .find(|a| a.sha256.eq_ignore_ascii_case(&digest))
            .ok_or(ResourceError::ImportMismatch)?;
        self.install_verified_package(asset, archive, ResourceSource::Import, backend, cancel)
    }
    /// Config files are user-owned after creation. Repair is create-if-missing only.
    pub fn repair_analysis_config(&self) -> ResourceResult<PathBuf> {
        let target = self.analysis_config();
        if target.is_file() {
            return Ok(target);
        }
        // Java AnalysisEngineCommandHelper.ensureAnalysisConfig requires a GTP config
        // and copies the bundled template; package examples take precedence here.
        if !self.gtp_config().is_file() {
            return Err(invalid("GTP config is missing"));
        }
        let packages = self.inventory()?.packages;
        let source = packages
            .iter()
            .filter(|p| p.usable)
            .find_map(|p| find_named(&p.directory, "analysis_example.cfg").ok().flatten());
        let bytes = if let Some(source) = source {
            fs::read(source)?
        } else {
            include_bytes!("analysis_example.cfg").to_vec()
        };
        create_file_once(&target, &bytes)?;
        Ok(target)
    }
    pub fn import_weight(
        &self,
        catalog: &Catalog,
        source: &Path,
        cancel: &AtomicBool,
    ) -> ResourceResult<InstalledWeight> {
        check_cancel(cancel)?;
        let meta = fs::metadata(source)?;
        let name = source
            .file_name()
            .and_then(|s| s.to_str())
            .ok_or_else(|| invalid("Unsupported weight file"))?;
        if !meta.is_file() || !supported_weight(name) {
            return Err(invalid("Unsupported weight file; expected .bin.gz or .txt.gz"));
        }
        let directory = self.weights_dir();
        fs::create_dir_all(&directory)?;
        let target = unique_weight_target(&directory, name);
        let staged = sibling_path(&target, "importing");
        let result = (|| -> ResourceResult<InstalledWeight> {
            copy_file(source, &staged, cancel)?;
            let digest = sha256_file(&staged)?;
            check_cancel(cancel)?;
            // The workbench serializes imports; hard_link also makes collisions non-destructive.
            fs::hard_link(&staged, &target)?;
            let receipt = match self.record_weight_digest(catalog, &target, digest) {
                Ok(receipt) => receipt,
                Err(error) => {
                    let _ = fs::remove_file(&target);
                    return Err(error);
                }
            };
            weight_entry(&target, Some(receipt))
        })();
        let _ = fs::remove_file(staged);
        result
    }
    /// Call once after a verified download, passing its known digest to avoid hashing twice.
    pub fn register_weight(
        &self,
        catalog: &Catalog,
        downloaded: &super::Downloaded,
    ) -> ResourceResult<InstalledWeight> {
        let canonical = fs::canonicalize(&downloaded.path)?;
        if canonical.parent() != Some(fs::canonicalize(self.weights_dir())?.as_path())
            || !supported_weight(&canonical.file_name().unwrap_or_default().to_string_lossy())
        {
            return Err(invalid("Weight is outside resource weights directory"));
        }
        if fs::metadata(&canonical)?.len() != downloaded.size {
            return Err(invalid("Downloaded weight changed before registration"));
        }
        let receipt = self.record_weight_digest(catalog, &canonical, downloaded.sha256.clone())?;
        weight_entry(&canonical, Some(receipt))
    }
    fn record_weight_digest(&self, catalog: &Catalog, path: &Path, digest: String) -> ResourceResult<WeightReceipt> {
        let meta = fs::metadata(path)?;
        let catalog_model_id = catalog
            .models
            .values()
            .find(|m| m.size_bytes == meta.len() && m.sha256.eq_ignore_ascii_case(&digest))
            .map(|m| m.id.clone());
        let receipt = WeightReceipt {
            sha256: digest,
            size: meta.len(),
            modified_nanos: modified_nanos(&meta),
            catalog_model_id,
        };
        write_json(&weight_receipt_path(path), &receipt)?;
        Ok(receipt)
    }
    /// Inventory never hashes large weights. A receipt is trusted only for unchanged size/mtime.
    pub fn inventory(&self) -> ResourceResult<ResourceInventory> {
        let mut inventory = ResourceInventory::default();
        let engines = self.root.join("engines");
        for directory in read_dirs(&engines)? {
            if !fs::symlink_metadata(&directory)?.is_dir()
                || directory.file_name().is_some_and(|n| n.to_string_lossy().contains('.'))
            {
                continue;
            }
            let Ok(bytes) = fs::read(directory.join("resource.json")) else {
                continue;
            };
            let Ok(receipt) = serde_json::from_slice::<ResourceReceipt>(&bytes) else {
                continue;
            };
            if safe_relative(&receipt.executable).is_err() {
                continue;
            }
            let executable = directory.join(&receipt.executable);
            let usable =
                fs::symlink_metadata(&executable).is_ok_and(|m| m.is_file() && m.len() == receipt.executable_size);
            inventory.packages.push(InstalledPackage {
                directory,
                executable,
                receipt,
                usable,
            });
        }
        for path in read_dirs(&self.weights_dir())? {
            if !supported_weight(&path.file_name().unwrap_or_default().to_string_lossy())
                || !fs::symlink_metadata(&path)?.is_file()
            {
                continue;
            }
            let receipt = fs::read(weight_receipt_path(&path))
                .ok()
                .and_then(|b| serde_json::from_slice::<WeightReceipt>(&b).ok());
            inventory.weights.push(weight_entry(&path, receipt)?);
        }
        inventory.packages.sort_by(|a, b| a.directory.cmp(&b.directory));
        inventory.weights.sort_by(|a, b| a.path.cmp(&b.path));
        inventory.gtp_config = self.gtp_config().is_file().then(|| self.gtp_config());
        inventory.analysis_config = self.analysis_config().is_file().then(|| self.analysis_config());
        Ok(inventory)
    }
    pub fn runtime_cache_size(&self) -> ResourceResult<u64> {
        directory_size(&self.runtime_downloads_dir())
    }
    pub fn clean_runtime_cache(&self, include_partial: bool) -> ResourceResult<u64> {
        clean_cache(&self.runtime_downloads_dir(), include_partial)
    }
    /// All component archives are verified before any runtime is changed. DLLs and licenses
    /// are staged together; downloads stay in the old directory until the atomic swap succeeds.
    pub fn install_nvidia_runtime(
        &self,
        packages: &[(RuntimePackage, PathBuf)],
        cancel: &AtomicBool,
    ) -> ResourceResult<()> {
        if packages.is_empty() {
            return Err(invalid("No NVIDIA runtime packages selected"));
        }
        for (package, path) in packages {
            check_cancel(cancel)?;
            safe_id(&package.key)?;
            verify_file(path, Some(package.size), Some(&package.sha256))?;
        }
        let target = self.runtime_dir();
        let stage = Staging::new(&target)?;
        if target.exists() {
            copy_tree(&target, &stage.0, Some("downloads"), cancel)?;
        }
        for (package, path) in packages {
            check_cancel(cancel)?;
            let mut zip = zip::ZipArchive::new(File::open(path)?)?;
            for index in 0..zip.len() {
                check_cancel(cancel)?;
                let mut entry = zip.by_index(index)?;
                let relative = safe_relative(entry.name())?;
                if entry.is_dir() {
                    continue;
                }
                reject_link(entry.unix_mode())?;
                let name = relative
                    .file_name()
                    .ok_or_else(|| invalid("Invalid runtime archive path"))?
                    .to_string_lossy();
                let destination = if should_extract_runtime_dll(&package.key, &name) {
                    Some(stage.0.join(name.as_ref()))
                } else if is_runtime_license(entry.name()) {
                    Some(stage.0.join("licenses").join(format!("{}-{name}", package.key)))
                } else {
                    None
                };
                if let Some(destination) = destination {
                    fs::create_dir_all(destination.parent().expect("runtime parent"))?;
                    let mut file = File::create(&destination)?;
                    copy_stream(&mut entry, &mut file, cancel)?;
                    file.sync_all()?;
                }
            }
        }
        let manifest: Vec<&RuntimePackage> = packages.iter().map(|(p, _)| p).collect();
        write_json(&stage.0.join("runtime.json"), &manifest)?;
        check_cancel(cancel)?;
        let backup = move_aside(&target)?;
        if let Err(error) = fs::rename(&stage.0, &target) {
            restore(&target, backup.as_deref())?;
            return Err(error.into());
        }
        if let Some(backup) = &backup {
            let cache = backup.join("downloads");
            if cache.exists() {
                if let Err(error) = fs::rename(&cache, target.join("downloads")) {
                    restore(&target, Some(backup))?;
                    return Err(error.into());
                }
            }
        }
        if let Some(backup) = backup {
            let _ = fs::remove_dir_all(backup);
        }
        Ok(())
    }
}
fn safe_id(id: &str) -> ResourceResult<()> {
    if super::matches_pattern("^[A-Za-z0-9][A-Za-z0-9_-]*$", id) {
        Ok(())
    } else {
        Err(invalid("Unsafe resource id"))
    }
}
/// Cross-platform zip-slip guard: backslashes and Windows drive prefixes matter on Unix too.
fn safe_relative(name: &str) -> ResourceResult<PathBuf> {
    let normalized = name.replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with('/') || normalized.contains([':', '\0']) {
        return Err(invalid("Unsafe archive path"));
    }
    let path = PathBuf::from(normalized);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return Err(invalid("Archive attempts to escape installation directory"));
    }
    Ok(path)
}
fn reject_link(mode: Option<u32>) -> ResourceResult<()> {
    if mode.is_some_and(|m| m & 0o170000 == 0o120000) {
        Err(invalid("Archive symlinks are not allowed"))
    } else {
        Ok(())
    }
}
fn extract_package(archive: &Path, directory: &Path, cancel: &AtomicBool) -> ResourceResult<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)?;
    let mut seen = std::collections::BTreeSet::new();
    for index in 0..zip.len() {
        check_cancel(cancel)?;
        let mut entry = zip.by_index(index)?;
        let relative = safe_relative(entry.name())?;
        reject_link(entry.unix_mode())?;
        let target = directory.join(&relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        // Reject ambiguous case-colliding paths as well as duplicate executable entries.
        if !seen.insert(relative.to_string_lossy().to_lowercase()) {
            return Err(invalid("Archive has duplicate paths"));
        }
        fs::create_dir_all(target.parent().expect("extraction parent"))?;
        let mut file = OpenOptions::new().write(true).create_new(true).open(&target)?;
        copy_stream(&mut entry, &mut file, cancel)?;
        file.sync_all()?;
    }
    Ok(())
}
fn find_named(directory: &Path, name: &str) -> ResourceResult<Option<PathBuf>> {
    let mut found = None;
    for path in read_dirs(directory)? {
        let meta = fs::symlink_metadata(&path)?;
        let candidate = if meta.is_dir() {
            find_named(&path, name)?
        } else if meta.is_file() && path.file_name().is_some_and(|n| n == name) {
            Some(path)
        } else {
            None
        };
        if let Some(candidate) = candidate {
            if found.is_some() {
                return Err(invalid(format!("Package has multiple {name} files")));
            }
            found = Some(candidate);
        }
    }
    Ok(found)
}
fn read_dirs(directory: &Path) -> ResourceResult<Vec<PathBuf>> {
    match fs::read_dir(directory) {
        Ok(entries) => entries.map(|e| e.map(|e| e.path()).map_err(Into::into)).collect(),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}
fn move_aside(target: &Path) -> ResourceResult<Option<PathBuf>> {
    if target.exists() {
        let backup = sibling_path(target, "backup");
        fs::rename(target, &backup)?;
        Ok(Some(backup))
    } else {
        Ok(None)
    }
}
fn restore(target: &Path, backup: Option<&Path>) -> ResourceResult<()> {
    if target.exists() {
        fs::remove_dir_all(target)?;
    }
    if let Some(backup) = backup {
        fs::rename(backup, target)?;
    }
    Ok(())
}
fn copy_stream(source: &mut impl Read, destination: &mut impl Write, cancel: &AtomicBool) -> ResourceResult<()> {
    let mut buffer = [0; 64 * 1024];
    loop {
        check_cancel(cancel)?;
        let n = source.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        destination.write_all(&buffer[..n])?;
    }
    check_cancel(cancel)
}
fn copy_file(source: &Path, destination: &Path, cancel: &AtomicBool) -> ResourceResult<()> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new().create_new(true).write(true).open(destination)?;
    copy_stream(&mut input, &mut output, cancel)?;
    output.sync_all()?;
    Ok(())
}
fn copy_tree(source: &Path, destination: &Path, skip: Option<&str>, cancel: &AtomicBool) -> ResourceResult<()> {
    fs::create_dir_all(destination)?;
    for path in read_dirs(source)? {
        check_cancel(cancel)?;
        let name = path.file_name().ok_or_else(|| invalid("Invalid tree path"))?;
        if skip.is_some_and(|skip| name == skip) {
            continue;
        }
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_symlink() {
            return Err(invalid("Resource tree contains a symlink"));
        }
        let target = destination.join(name);
        if meta.is_dir() {
            copy_tree(&path, &target, None, cancel)?;
        } else if meta.is_file() {
            copy_file(&path, &target, cancel)?;
        }
    }
    Ok(())
}
fn write_json(path: &Path, value: &impl Serialize) -> ResourceResult<()> {
    atomic_write(path, &serde_json::to_vec_pretty(value)?)
}
fn atomic_write(path: &Path, bytes: &[u8]) -> ResourceResult<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| invalid("No resource parent directory"))?)?;
    let staged = sibling_path(path, "writing");
    let result = (|| -> ResourceResult<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&staged)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        super::download::replace_file(&staged, path)
    })();
    let _ = fs::remove_file(staged);
    result
}
fn create_file_once(path: &Path, bytes: &[u8]) -> ResourceResult<()> {
    fs::create_dir_all(path.parent().ok_or_else(|| invalid("No resource parent directory"))?)?;
    let staged = sibling_path(path, "writing");
    let result = (|| -> ResourceResult<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(&staged)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        match fs::hard_link(&staged, path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(e) => Err(e.into()),
        }
    })();
    let _ = fs::remove_file(staged);
    result
}
fn modified_nanos(meta: &fs::Metadata) -> u128 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}
fn supported_weight(name: &str) -> bool {
    let name = name.to_lowercase();
    name.ends_with(".bin.gz") || name.ends_with(".txt.gz")
}
fn unique_weight_target(directory: &Path, name: &str) -> PathBuf {
    let target = directory.join(name);
    if !target.exists() {
        return target;
    }
    let (stem, suffix) = name.split_at(name.len() - 7);
    for index in 1..1000 {
        let path = directory.join(format!("{stem}-{index}{suffix}"));
        if !path.exists() {
            return path;
        }
    }
    directory.join(format!(
        "{stem}-{}-{}{suffix}",
        epoch_millis(),
        NEXT_STAGE.fetch_add(1, Ordering::Relaxed)
    ))
}
fn weight_receipt_path(path: &Path) -> PathBuf {
    path.with_file_name(format!(
        "{}.resource.json",
        path.file_name().unwrap_or_default().to_string_lossy()
    ))
}
fn weight_entry(path: &Path, receipt: Option<WeightReceipt>) -> ResourceResult<InstalledWeight> {
    let meta = fs::metadata(path)?;
    let receipt = receipt.filter(|r| r.size == meta.len() && r.modified_nanos == modified_nanos(&meta));
    Ok(InstalledWeight {
        path: path.into(),
        size: meta.len(),
        model_name: read_weight_model_name(path),
        catalog_model_id: receipt.as_ref().and_then(|r| r.catalog_model_id.clone()),
        sha256: receipt.map(|r| r.sha256),
    })
}
fn directory_size(directory: &Path) -> ResourceResult<u64> {
    let mut total: u64 = 0;
    for path in read_dirs(directory)? {
        let meta = fs::symlink_metadata(&path)?;
        total = total.saturating_add(if meta.is_dir() {
            directory_size(&path)?
        } else if meta.is_file() {
            meta.len()
        } else {
            0
        });
    }
    Ok(total)
}
fn clean_cache(directory: &Path, include_partial: bool) -> ResourceResult<u64> {
    let mut freed: u64 = 0;
    for path in read_dirs(directory)? {
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            freed = freed.saturating_add(clean_cache(&path, include_partial)?);
            let _ = fs::remove_dir(&path);
        } else if include_partial || !path.file_name().is_some_and(|n| n.to_string_lossy().ends_with(".part")) {
            fs::remove_file(&path)?;
            if meta.is_file() {
                freed = freed.saturating_add(meta.len());
            }
        }
    }
    Ok(freed)
}
