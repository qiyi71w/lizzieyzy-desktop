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
            if !appearance::valid_name(&name) || name.eq_ignore_ascii_case("default") {
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
            let own: Vec<(&str, &str, Option<Image>)> = IMAGES.iter().map(|&(k, d)| (k, d, image(&dir, &raw, k, d))).collect();
            let complete = own.iter().all(|(k, d, i)| i.is_some() || configured(&raw, k, d).is_some_and(|c| c.trim().is_empty()));
            let shown: Vec<(&str, &[u8])> = own.iter().filter_map(|(_, _, i)| i.as_ref().map(|(f, b)| (f.as_str(), b.as_slice()))).collect();
            if complete && appearance::bundled_copy(&name, &raw, &shown) { continue; }
            let bundled = appearance::BUNDLED.iter().any(|(n, _)| *n == name);
            let images = own.into_iter().map(|(k, d, i)| (k, match i {
                Some(i) => Shown::Copy(i),
                // Theme.getImageByKey's next steps: ui's value (or the same default name) in theme/,
                // then the jar asset. A user theme keeps following ui's value; a user copy of a
                // bundled theme would show the bundled picture instead, so it records the result.
                None => match image(&root, ui, k, d) {
                    Some(i) if bundled || !ui.contains_key(k) => Shown::Copy(i),
                    _ if bundled => Shown::Asset,
                    _ => Shown::Inherit,
                },
            })).collect();
            self.mapped.themes.push(folder(name, raw, images));
            let it = self.item(key, ImportStatus::Imported);
            it.target = Some(ImportTarget::Themes);
        }
    }

    fn theme_unreadable(&mut self, key: String, detail: String) {
        let it = self.item(key, ImportStatus::Incompatible);
        it.target = Some(ImportTarget::Themes);
        it.reason = Some(ImportReason::Unreadable);
        it.detail = Some(super::sanitize(&detail));
    }
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
/// to `dir`, or absolute. `None` when it is unset or not a readable picture.
fn image(dir: &Path, raw: &Map<String, Value>, key: &str, default: &str) -> Option<Image> {
    let configured = configured(raw, key, default)?;
    if configured.trim().is_empty() { return None; }
    let path = dir.join(configured);
    let bytes = std::fs::read(&path).ok().filter(|b| appearance::is_image(b))?;
    Some((path.file_name()?.to_string_lossy().into_owned(), bytes))
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
