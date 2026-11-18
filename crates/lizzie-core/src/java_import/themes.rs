//! Java's own theme folders: `theme/<name>/theme.txt` and the images it shows (Theme.java:53-77,
//! 146-204, 493-588). Each becomes this product's theme of the same name, its images copied in.
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::{read_json, ImportReason, ImportStatus, ImportTarget, JsonError, Reader};
use crate::appearance::{self, ThemeFolder};

/// The theme's image keys and the file Java reads when the key is absent.
const IMAGES: [(&str, &str); 4] = [
    ("board-image", "board.png"),
    ("background-image", "background.png"),
    ("black-stone-image", "black.png"),
    ("white-stone-image", "white.png"),
];
const THEME_FILE: &str = "theme.txt";
/// The file name and bytes of an image a theme shows.
type Image = (String, Vec<u8>);

/// What the imported theme names for one image key.
enum Shown {
    /// This file, copied into the theme.
    Copy(Image),
    /// Nothing: the bundled default picture (Java's jar asset).
    Asset,
    /// The key is left out, so the theme follows the default theme's (ui) value, as Java does.
    Inherit,
}

impl Reader<'_> {
    /// Every folder of Java's `theme/` holding a `theme.txt` (ConfigDialog2.java:4372-4384). The
    /// folder is the working directory's `theme/`: the work directory, else the program folder.
    /// An untouched copy of a bundled theme needs no folder of its own. `ui` is Java's ui section,
    /// whose images a theme falls back to.
    pub(super) fn themes(&mut self, ui: &Map<String, Value>) {
        let roots = [Some(self.source.config_dir.clone()), self.source.program_dir.clone()];
        let Some(root) = roots.into_iter().flatten().map(|r| r.join("theme")).find(|d| d.is_dir()) else { return };
        let entries = match std::fs::read_dir(&root) {
            Ok(entries) => entries,
            Err(e) => { self.theme_unreadable("file:theme".into(), e.to_string()); return; }
        };
        let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let key = format!("file:theme/{name}");
            match std::fs::metadata(dir.join(THEME_FILE)) {
                Ok(m) if m.is_file() => {}
                Ok(_) => continue,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                Err(e) => { self.theme_unreadable(key, e.to_string()); continue; }
            }
            if !appearance::valid_name(&product_name(&name)) {
                let it = self.item(key, ImportStatus::Incompatible);
                it.target = Some(ImportTarget::Themes);
                it.reason = Some(ImportReason::InvalidValue);
                continue;
            }
            let raw = match read_json(&dir.join(THEME_FILE)) {
                Ok(Value::Object(raw)) => raw,
                Ok(_) => { self.theme_unreadable(key, "not a JSON object".into()); continue; }
                Err(JsonError::Io(d) | JsonError::Syntax(d)) => { self.theme_unreadable(key, d); continue; }
            };
            let invalid = appearance::invalid_java_keys(&raw);
            if !invalid.is_empty() {
                let it = self.item(key, ImportStatus::Incompatible);
                it.target = Some(ImportTarget::Themes);
                it.reason = Some(ImportReason::InvalidValue);
                it.detail = Some(super::sanitize(&invalid.join(", ")));
                continue;
            }
            // A picture Java reads but this product cannot use leaves the folder out (reported),
            // rather than coming over without it.
            let own: Result<Vec<(&str, &str, Option<Image>)>, String> =
                IMAGES.iter().map(|&(k, d)| Ok((k, d, image(&dir, &raw, k, d).map_err(|f| format!("{k}: {f}"))?))).collect();
            let own = match own {
                Ok(own) => own,
                Err(detail) => { self.theme_unreadable(key, detail); continue; }
            };
            let complete = own.iter().all(|(k, d, i)| i.is_some() || configured(&raw, k, d).is_some_and(|c| c.trim().is_empty()));
            let shown: Vec<(&str, &[u8])> = own.iter().filter_map(|(_, _, i)| i.as_ref().map(|(f, b)| (f.as_str(), b.as_slice()))).collect();
            if complete && appearance::bundled_copy(&name, &raw, &shown) { continue; }
            let bundled = appearance::BUNDLED.iter().any(|(n, _)| *n == name);
            let images: Result<Vec<(&str, Shown)>, String> = own.into_iter().map(|(k, d, i)| Ok((k, match i {
                Some(i) => Shown::Copy(i),
                // Theme.getImageByKey's next steps: ui's value (or the same default name) in theme/,
                // then the jar asset. A user theme keeps following ui's value; a user copy of a
                // bundled theme would show the bundled picture instead, so it records the result.
                None if bundled || !ui.contains_key(k) => match image(&root, ui, k, d).map_err(|f| format!("{k}: {f}"))? {
                    Some(i) => Shown::Copy(i),
                    None if bundled => Shown::Asset,
                    None => Shown::Inherit,
                },
                None => Shown::Inherit,
            }))).collect();
            let images = match images {
                Ok(images) => images,
                Err(detail) => { self.theme_unreadable(key, detail); continue; }
            };
            let product = product_name(&name);
            let renamed = product != name;
            self.mapped.themes.push(folder(product.clone(), raw, images));
            let it = self.item(key, ImportStatus::Imported);
            it.target = Some(ImportTarget::Themes);
            // This product's default theme is `default`: the Java theme comes over renamed.
            if renamed { it.value = Some(super::sanitize(&product)); }
        }
    }

    fn theme_unreadable(&mut self, key: String, detail: String) {
        let it = self.item(key, ImportStatus::Incompatible);
        it.target = Some(ImportTarget::Themes);
        it.reason = Some(ImportReason::Unreadable);
        it.detail = Some(super::sanitize(&detail));
    }
}

/// The name a Java theme folder has here: `default` (any case) is this product's default theme,
/// so a Java theme of that name becomes "<name> (LizzieYzy Next)".
pub(super) fn product_name(java: &str) -> String {
    if java.eq_ignore_ascii_case("default") { format!("{java} (LizzieYzy Next)") } else { java.to_string() }
}

/// `config.optString(key, default)` for an image key; `None` for a value that is not text.
fn configured<'a>(raw: &'a Map<String, Value>, key: &str, default: &'a str) -> Option<&'a str> {
    match raw.get(key) {
        None => Some(default),
        Some(Value::String(s)) => Some(s.as_str()),
        Some(_) => None,
    }
}

/// The image Java reads for `key` from `raw`: the configured file (or the default name) relative
/// to `dir`, or absolute. `None` when it is unset or not a readable picture. A TIFF (Java's
/// `ImageIO` reads it) comes as a PNG of the same name; one that does not decode is `Err(file)`.
fn image(dir: &Path, raw: &Map<String, Value>, key: &str, default: &str) -> Result<Option<Image>, String> {
    let Some(configured) = configured(raw, key, default) else { return Ok(None) };
    if configured.trim().is_empty() { return Ok(None); }
    let path = dir.join(configured);
    let (Ok(bytes), Some(file)) = (std::fs::read(&path), path.file_name()) else { return Ok(None) };
    let file = file.to_string_lossy().into_owned();
    if appearance::is_image(&bytes) { return Ok(Some((file, bytes))); }
    match appearance::tiff_as_png(&bytes) {
        None => Ok(None),
        Some(Ok(png)) => {
            let stem = file.rsplit_once('.').map_or(file.as_str(), |(stem, _)| stem);
            Ok(Some((format!("{stem}.png"), png)))
        }
        Some(Err(_)) => Err(file),
    }
}

/// `theme.txt` naming the copied images (one flat folder; a clashing name gets a number).
fn folder(name: String, mut raw: Map<String, Value>, images: Vec<(&str, Shown)>) -> ThemeFolder {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (key, shown) in images {
        let (file, bytes) = match shown {
            Shown::Copy(image) => image,
            Shown::Asset => { raw.insert(key.into(), Value::from("")); continue; }
            Shown::Inherit => { raw.remove(key); continue; }
        };
        let file = match files.iter().find(|(f, b)| *f == file && *b == bytes) {
            Some((same, _)) => same.clone(),
            None => {
                let (stem, ext) = file.rsplit_once('.').unwrap_or((file.as_str(), ""));
                let mut candidate = file.clone();
                let mut n = 2;
                while candidate.eq_ignore_ascii_case(THEME_FILE) || files.iter().any(|(f, _)| f.eq_ignore_ascii_case(&candidate)) {
                    candidate = if ext.is_empty() { format!("{stem}-{n}") } else { format!("{stem}-{n}.{ext}") };
                    n += 1;
                }
                files.push((candidate.clone(), bytes));
                candidate
            }
        };
        raw.insert(key.into(), Value::from(file));
    }
    let text = serde_json::to_vec_pretty(&Value::Object(raw)).expect("theme serializes");
    files.insert(0, (THEME_FILE.into(), text));
    ThemeFolder { name, files }
}
