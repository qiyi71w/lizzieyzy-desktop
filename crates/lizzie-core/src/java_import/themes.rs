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

impl Reader<'_> {
    /// Every folder of Java's `theme/` holding a `theme.txt` (ConfigDialog2.java:4372-4384). The
    /// folder is the working directory's `theme/`: the work directory, else the program folder.
    /// An untouched copy of a bundled theme needs no folder of its own.
    pub(super) fn themes(&mut self) {
        let Some(root) = self.source.roots().into_iter().map(|r| r.join("theme")).find(|d| d.is_dir()) else { return };
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root).into_iter().flatten().flatten()
            .map(|e| e.path()).filter(|p| p.join(THEME_FILE).is_file()).collect();
        dirs.sort();
        for dir in dirs {
            let name = dir.file_name().unwrap_or_default().to_string_lossy().into_owned();
            let key = format!("file:theme/{name}");
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
            let images: Vec<(&str, Option<Image>)> = IMAGES.iter().map(|&(k, default)| (k, image(&dir, &raw, k, default))).collect();
            let shown: Vec<(&str, &[u8])> = images.iter().filter_map(|(_, i)| i.as_ref().map(|(f, b)| (f.as_str(), b.as_slice()))).collect();
            if appearance::bundled_copy(&name, &raw, &shown) { continue; }
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

/// The image Java shows for `key` from the theme itself: the configured file (or the default
/// name) relative to the theme folder, or absolute. `None` when Java falls back to the ui value.
fn image(dir: &Path, raw: &Map<String, Value>, key: &str, default: &str) -> Option<Image> {
    let configured = match raw.get(key) {
        None => default,
        Some(Value::String(s)) => s.as_str(),
        Some(_) => return None,
    };
    if configured.trim().is_empty() { return None; }
    let path = dir.join(configured);
    let bytes = std::fs::read(&path).ok().filter(|b| appearance::is_image(b))?;
    Some((path.file_name()?.to_string_lossy().into_owned(), bytes))
}

/// `theme.txt` naming the copied images (one flat folder; a clashing name gets a number), and
/// leaving out the images Java takes from the ui settings, as this product then does too.
fn folder(name: String, mut raw: Map<String, Value>, images: Vec<(&str, Option<Image>)>) -> ThemeFolder {
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (key, image) in images {
        let Some((file, bytes)) = image else { raw.remove(key); continue; };
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
