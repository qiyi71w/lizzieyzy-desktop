//! Java Theme.java presentation values. Bundled themes are immutable; edits shadow them in
//! this product's config directory. No Java installation is consulted at runtime.
use std::path::{Path, PathBuf};
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use ts_rs::TS;
use crate::settings::{self, Storage};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum BoardStyle { #[default] Japanese, ChineseClassic }

/// 外观模式: the interface follows the system's light or dark setting, or is fixed to one. Light
/// by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum AppearanceMode { System, #[default] Light, Dark }

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ThemeNodeColor { pub node: crate::record::NodeId, pub color: Vec<u8> }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: String,
    pub board_style: BoardStyle,
    pub mode: AppearanceMode,
    pub custom_board_image: String,
    pub custom_background_image: String,
    pub frame_font_size: i32,
    /// Config.java:1809; no Java UI changes it. Legacy level 0 uses the theme's background blur.
    pub glass_effect_level: i32,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        // Config.java:1608,1871,2924; Menu.java:604-629.
        Self { theme: "default".into(), board_style: BoardStyle::Japanese, mode: AppearanceMode::Light,
            custom_board_image: String::new(), custom_background_image: String::new(), frame_font_size: 12,
            glass_effect_level: 1 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ThemeSettings {
    pub use_pure_board: bool, pub use_pure_background: bool, pub use_pure_stone: bool,
    pub pure_board_color: Vec<u8>, pub pure_background_color: Vec<u8>,
    pub show_stone_shadow: bool, pub shadow_size: i32, pub stone_indicator_type: i32,
    pub winrate_stroke_width: f64, pub score_lead_stroke_width: f64, pub minimum_blunder_bar_width: i32,
    pub comment_font_size: i32, pub background_filter: i32,
    pub winrate_line_color: Vec<u8>, pub winrate_miss_line_color: Vec<u8>, pub scoremean_line_color: Vec<u8>,
    pub blunder_bar_color: Vec<u8>, pub best_move_color: Vec<u8>,
    pub comment_background_color: Vec<u8>, pub comment_font_color: Vec<u8>, pub comment_node_color: Vec<u8>,
    pub show_comment_node_color: bool, pub use_scorediff_in_variation_tree: bool,
    pub scorediff_in_variation_tree_factor: f64,
    pub font_name: String, pub ui_font_name: String, pub winrate_font_name: String,
    pub board_image: String, pub background_image: String, pub black_stone_image: String, pub white_stone_image: String,
    pub blunder_winrate_thresholds: Vec<f64>, pub blunder_node_colors: Vec<Vec<u8>>,
    pub glass_panel_overlay_color: Vec<u8>, pub glass_panel_border_color: Vec<u8>,
    pub glass_panel_highlight_color: Vec<u8>, pub glass_panel_shadow_color: Vec<u8>, pub glass_accent_color: Vec<u8>,
    pub glass_blur_radius: i32, pub liquid_blur_radius: i32, pub glass_corner_radius: i32,
    /// 界面颜色, this product's own (no Java key): the bars', the controls', the accent and the
    /// interface text's colour, `[r, g, b]`; empty leaves the skin's own colour.
    pub ui_chrome_color: Vec<u8>, pub ui_control_color: Vec<u8>, pub ui_accent_color: Vec<u8>, pub ui_text_color: Vec<u8>,
}
impl Default for ThemeSettings {
    fn default() -> Self {
        // Theme.java:36-44,87-113,219-356. comment-font-size 0 means automatic sizing.
        Self { use_pure_board: false, use_pure_background: false, use_pure_stone: false,
            pure_board_color: vec![217,152,77], pure_background_color: vec![128,128,128],
            show_stone_shadow: true, shadow_size: 85, stone_indicator_type: 1,
            winrate_stroke_width: 1.7, score_lead_stroke_width: 1.0, minimum_blunder_bar_width: 1,
            comment_font_size: 0, background_filter: 20, winrate_line_color: vec![100,180,255],
            winrate_miss_line_color: vec![0,0,178], scoremean_line_color: vec![255,0,255],
            blunder_bar_color: vec![255,204,255,170], best_move_color: vec![0,255,255],
            comment_background_color: vec![0,0,0,200], comment_font_color: vec![255,255,255], comment_node_color: vec![0,0,255],
            show_comment_node_color: true, use_scorediff_in_variation_tree: true, scorediff_in_variation_tree_factor: 0.5,
            font_name: "Lizzie Default".into(), ui_font_name: "Lizzie Default".into(), winrate_font_name: "Lizzie Default".into(),
            board_image: String::new(), background_image: String::new(), black_stone_image: String::new(), white_stone_image: String::new(),
            blunder_winrate_thresholds: vec![-24.,-12.,-6.,-3.,-1.,3.,100.],
            // Theme.java:738-767: the glass theme, used for glass-effect-level > 0.
            glass_panel_overlay_color:vec![24,24,26,102], glass_panel_border_color:vec![255,255,255,26],
            glass_panel_highlight_color:vec![255,255,255,77], glass_panel_shadow_color:vec![0,0,0,64], glass_accent_color:vec![96,165,250],
            glass_blur_radius:10, liquid_blur_radius:15, glass_corner_radius:12,
            blunder_node_colors: vec![vec![155,25,150],vec![208,16,19],vec![200,140,50],vec![180,180,0],vec![140,202,34],vec![0,220,0],vec![0,210,210]],
            ui_chrome_color: Vec::new(), ui_control_color: Vec::new(), ui_accent_color: Vec::new(), ui_text_color: Vec::new() }
    }
}
impl ThemeSettings {
    pub fn valid(&self) -> bool {
        let colors = [&self.pure_board_color,&self.pure_background_color,&self.winrate_line_color,&self.winrate_miss_line_color,
            &self.scoremean_line_color,&self.blunder_bar_color,&self.best_move_color,&self.comment_background_color,
            &self.comment_font_color,&self.comment_node_color,&self.glass_panel_overlay_color,&self.glass_panel_border_color,
            &self.glass_panel_highlight_color,&self.glass_panel_shadow_color,&self.glass_accent_color];
        colors.iter().all(|c| matches!(c.len(),3|4)) && self.blunder_node_colors.iter().all(|c| matches!(c.len(),3|4))
            && [&self.ui_chrome_color,&self.ui_control_color,&self.ui_accent_color,&self.ui_text_color].iter().all(|c| matches!(c.len(),0|3))
            && !self.blunder_winrate_thresholds.is_empty() && self.blunder_winrate_thresholds.len() == self.blunder_node_colors.len()
            && self.blunder_winrate_thresholds.iter().all(|v| v.is_finite())
            && self.blunder_winrate_thresholds.windows(2).all(|v| v[0] < v[1])
            && self.winrate_stroke_width.is_finite() && self.winrate_stroke_width > 0.
            && self.score_lead_stroke_width.is_finite() && self.score_lead_stroke_width > 0.
            && self.scorediff_in_variation_tree_factor.is_finite() && self.minimum_blunder_bar_width > 0
            && self.comment_font_size >= 0 && self.background_filter >= 0 && self.shadow_size > 0
            && self.glass_blur_radius>=0 && self.liquid_blur_radius>=0 && self.glass_corner_radius>=4
            && (0..=3).contains(&self.stone_indicator_type)
    }
}

pub const BUNDLED: &[(&str, &str)] = &[
    ("sabaki",include_str!("../../../ui/public/themes/sabaki/theme.txt")),
    ("yasnaya",include_str!("../../../ui/public/themes/yasnaya/theme.txt")),
    ("Megapack",include_str!("../../../ui/public/themes/Megapack/theme.txt")),
    ("Custom",include_str!("../../../ui/public/themes/Custom/theme.txt")),
    ("Fast",include_str!("../../../ui/public/themes/Fast/theme.txt")),
    // Java's own 默认 values (Theme.java getters, Config.createDefaultConfig) with the jar's images.
    ("java-classic",include_str!("../../../ui/public/themes/java-classic/theme.txt")),
];
pub fn themes_dir(storage: &Storage) -> PathBuf { storage.config_file.parent().unwrap_or(Path::new(".")).join("themes") }
pub fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name == name.trim() && !matches!(name,"."|"..") && !name.eq_ignore_ascii_case(".images")
        && !name.chars().any(|c| c.is_control() || "/\\:*?\"<>|".contains(c))
}
pub fn theme_file(storage: &Storage, name: &str) -> std::io::Result<PathBuf> {
    if !valid_name(name) || name.eq_ignore_ascii_case("default") { return Err(std::io::Error::other("invalid theme name")); }
    Ok(themes_dir(storage).join(name).join("theme.txt"))
}
pub fn java_key(field: &str) -> String {
    let mut out = String::new();
    for c in field.chars() { if c.is_ascii_uppercase() { out.push('-'); out.push(c.to_ascii_lowercase()); } else { out.push(c); } }
    out
}
/// Keys of a Java `theme.txt` whose value no theme can use (a colour channel out of range, a
/// non-positive width, a wrong type). The 错误节点 pair is not judged here: Java itself falls back
/// to the ui rules for a bad pair (Theme.java:373-422), and so does [`merge_java`].
pub fn invalid_java_keys(raw: &Map<String,Value>) -> Vec<String> {
    let base = serde_json::to_value(ThemeSettings::default()).expect("theme serializes");
    let mut out = Vec::new();
    for field in base.as_object().expect("theme object").keys() {
        if field == "blunderWinrateThresholds" || field == "blunderNodeColors" { continue; }
        let key = java_key(field);
        let Some(value) = raw.get(&key) else { continue };
        // merge_java raises these to their minimum, as the glass painters do.
        if matches!(field.as_str(),"glassBlurRadius"|"liquidBlurRadius"|"glassCornerRadius") && value.as_i64().is_some() { continue; }
        let mut one = base.clone();
        one[field] = value.clone();
        if serde_json::from_value::<ThemeSettings>(one).ok().is_none_or(|theme| !theme.valid()) { out.push(key); }
    }
    out
}
/// Java theme fields override ui fields individually (Theme.java:473-478).
pub fn merge_java(base: &ThemeSettings, raw: &Map<String,Value>) -> ThemeSettings {
    let mut json = serde_json::to_value(base).expect("theme serializes");
    let fields: Vec<String> = json.as_object().expect("theme object").keys().cloned().collect();
    for field in fields {
        if field == "blunderWinrateThresholds" || field == "blunderNodeColors" { continue; }
        if let Some(value) = raw.get(&java_key(&field)) {
            let old = json[&field].clone();
            json[&field] = value.clone();
            if matches!(field.as_str(),"glassBlurRadius"|"liquidBlurRadius"|"glassCornerRadius") {
                if let Some(value)=value.as_i64() { json[&field]=Value::from(value.max(if field=="glassCornerRadius" {4} else {0})); }
            }
            if serde_json::from_value::<ThemeSettings>(json.clone()).ok().is_none_or(|theme| !theme.valid()) { json[&field] = old; }
        }
    }
    if let (Some(thresholds),Some(colors)) = (raw.get("blunder-winrate-thresholds"),raw.get("blunder-node-colors")) {
        let mut pair = json.clone();
        pair["blunderWinrateThresholds"] = thresholds.clone(); pair["blunderNodeColors"] = colors.clone();
        if serde_json::from_value::<ThemeSettings>(pair.clone()).ok().is_some_and(|theme|theme.valid()) { json = pair; }
    }
    // Legacy bad blunder tables fall back as a pair (Theme.java:385-422).
    let mut next: ThemeSettings = serde_json::from_value(json).unwrap_or_else(|_| base.clone());
    if next.blunder_winrate_thresholds.len() != next.blunder_node_colors.len()
        || next.blunder_winrate_thresholds.is_empty()
        || !next.blunder_winrate_thresholds.windows(2).all(|v| v[0].is_finite() && v[0] < v[1])
        || next.blunder_node_colors.iter().any(|c| !matches!(c.len(),3|4)) {
        next.blunder_winrate_thresholds = base.blunder_winrate_thresholds.clone(); next.blunder_node_colors = base.blunder_node_colors.clone();
    }
    next
}
/// The `theme.txt` keys the 主题外观 page edits: those ConfigDialog2 puts into a named theme
/// (ConfigDialog2.java:6204-6285), and this product's 界面颜色.
const PAGE_KEYS: &[&str] = &[
    "use-pure-stone", "use-pure-board", "pure-board-color", "use-pure-background", "pure-background-color",
    "show-stone-shadow", "winrate-stroke-width", "score-lead-stroke-width", "minimum-blunder-bar-width", "shadow-size",
    "font-name", "ui-font-name", "winrate-font-name", "use-scorediff-in-variation-tree", "scorediff-in-variation-tree-factor",
    "background-image", "board-image", "black-stone-image", "white-stone-image",
    "winrate-line-color", "winrate-miss-line-color", "blunder-bar-color", "scoremean-line-color",
    "stone-indicator-type", "show-comment-node-color", "comment-node-color", "comment-background-color", "comment-font-color",
    "best-move-color", "comment-font-size", "background-filter", "blunder-winrate-thresholds", "blunder-node-colors",
    "ui-chrome-color", "ui-control-color", "ui-accent-color", "ui-text-color",
];
/// The `theme.txt` a save of the named theme `name` writes. Like Theme.save (Theme.java:617-643)
/// it rewrites the object read from the file, `stored` (the user's file, or the bundled one when
/// there is none; an empty object for a theme with neither, as `new Theme(name)` reads), with the
/// page's values put in ([`PAGE_KEYS`]): every other key stays as it was, and a key whose value is
/// a blank string (String.trim) is left out, as is an unset 界面颜色.
pub fn to_java(name: &str, stored: Option<&[u8]>, theme: &ThemeSettings) -> Value {
    let mut raw = stored.and_then(|bytes| serde_json::from_slice::<Map<String,Value>>(bytes).ok())
        .or_else(|| BUNDLED.iter().find(|(n,_)| *n == name).and_then(|(_,raw)| serde_json::from_str(raw).ok()))
        .unwrap_or_default();
    let Value::Object(fields) = serde_json::to_value(theme).expect("theme serializes") else { unreachable!() };
    for (key,value) in fields.into_iter().map(|(k,v)|(java_key(&k),v)).filter(|(k,_)| PAGE_KEYS.contains(&k.as_str())) {
        if value.as_array().is_some_and(Vec::is_empty) { raw.remove(&key); } else { raw.insert(key,value); }
    }
    raw.retain(|_,v| !v.as_str().is_some_and(|s| s.trim_matches(|c: char| c <= ' ').is_empty()));
    Value::Object(raw)
}
pub fn load_theme(storage: &Storage, name: &str, defaults: &ThemeSettings) -> ThemeSettings {
    if name == "default" || !valid_name(name) { return defaults.clone(); }
    // A key the theme lacks takes the 默认 theme's value (Theme.java:473-478), but the 界面颜色
    // are each theme's own: a theme without them shows its skin's colours.
    let defaults = &ThemeSettings { ui_chrome_color: Vec::new(), ui_control_color: Vec::new(), ui_accent_color: Vec::new(), ui_text_color: Vec::new(), ..defaults.clone() };
    let hidden: Vec<String> = settings::load_json(&storage.config_file.with_file_name("hidden-themes.json"));
    if hidden.iter().any(|hidden| hidden == name) && theme_file(storage,name).is_ok_and(|path|!path.is_file()) { return defaults.clone(); }
    let bundled = BUNDLED.iter().find(|(n,_)| *n == name).and_then(|(_,raw)|serde_json::from_str::<Value>(raw).ok());
    let mut theme = bundled.as_ref().and_then(Value::as_object).map_or_else(|| defaults.clone(), |v| merge_java(defaults,v));
    if let Ok(path) = theme_file(storage,name) {
        if let Ok(bytes) = std::fs::read(path) { if let Ok(Value::Object(raw)) = serde_json::from_slice(&bytes) { theme = merge_java(&theme,&raw); } }
    }
    theme
}
/// One entry of the theme combo box (ConfigDialog2.loadThemeTab, ConfigDialog2.java:4372-4384);
/// "default" is Java's LizzieConfig.title.defaultTheme item.
#[derive(Serialize, TS)]
pub struct ThemeEntry { pub name: String, pub values: ThemeSettings }
/// `appearance_catalog`: the themes, the system font families (GraphicsEnvironment) and the
/// boards and stones the program ships ([`shipped_boards`], [`shipped_stones`]).
#[derive(Serialize, TS)]
pub struct AppearanceCatalog { pub themes: Vec<ThemeEntry>, pub fonts: Vec<String>, pub boards: Vec<ShippedBoard>, pub stones: Vec<ShippedStones> }
/// A board the program ships, under the name of the theme it comes with ("default" for the
/// program's own) and every address a theme can name it by ([`shipped_image`]); bundled themes
/// that ship the same picture share one entry.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ShippedBoard { pub theme: String, pub addresses: Vec<String> }
/// A pair of stones the program ships: black and white addresses, one pair per bundled theme
/// that ships the same two pictures.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ShippedStones { pub theme: String, pub pairs: Vec<(String, String)> }
/// The images each bundled theme's `keys` name, as `/themes/<theme>/<file>` addresses; themes
/// whose images are the same pictures form one group, under the first such theme, in BUNDLED
/// order.
fn shipped<const N: usize>(keys: [&str; N]) -> Vec<(&'static str, Vec<[String; N]>)> {
    let mut groups: Vec<(&'static str, [&'static str; N], Vec<[String; N]>)> = Vec::new();
    for (name,raw) in BUNDLED {
        let Ok(Value::Object(raw)) = serde_json::from_str::<Value>(raw) else { continue };
        let found = keys.map(|key| raw.get(key).and_then(Value::as_str)
            .and_then(|file| BUNDLED_IMAGES.iter().find(|(t,f,_)| t == name && *f == file)));
        if found.iter().any(Option::is_none) { continue; }
        let found = found.map(|image| image.expect("checked above"));
        let hashes = found.map(|(_,_,hash)| *hash);
        let addresses = found.map(|(_,file,_)| format!("/themes/{name}/{file}"));
        match groups.iter_mut().find(|(_,h,_)| *h == hashes) {
            Some((_,_,same)) => same.push(addresses),
            None => groups.push((name, hashes, vec![addresses])),
        }
    }
    groups.into_iter().map(|(name,_,addresses)| (name, addresses)).collect()
}
/// The boards the program ships: its own default board, then the bundled themes'.
pub fn shipped_boards() -> Vec<ShippedBoard> {
    std::iter::once(ShippedBoard { theme: "default".into(), addresses: vec!["/assets/board.png".into()] })
        .chain(shipped(["board-image"]).into_iter().map(|(theme,addresses)|
            ShippedBoard { theme: theme.into(), addresses: addresses.into_iter().map(|[board]| board).collect() }))
        .collect()
}
/// The stones the program ships: its own default pair, then the bundled themes'.
pub fn shipped_stones() -> Vec<ShippedStones> {
    std::iter::once(ShippedStones { theme: "default".into(), pairs: vec![("/assets/black0.png".into(),"/assets/white0.png".into())] })
        .chain(shipped(["black-stone-image","white-stone-image"]).into_iter().map(|(theme,pairs)|
            ShippedStones { theme: theme.into(), pairs: pairs.into_iter().map(|[black,white]| (black,white)).collect() }))
        .collect()
}
pub fn catalog(storage: &Storage, defaults: &ThemeSettings) -> Vec<ThemeEntry> {
    let dir = themes_dir(storage);
    let hidden: Vec<String> = settings::load_json(&storage.config_file.with_file_name("hidden-themes.json"));
    let mut names: Vec<String> = BUNDLED.iter().filter(|(n,_)|!hidden.iter().any(|h|h == n)).map(|(n,_)|n.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir(&dir) { for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if valid_name(&name) && entry.path().join("theme.txt").is_file() && !names.contains(&name) { names.push(name); }
    } }
    names.sort_by_key(|s|s.to_lowercase()); names.insert(0,"default".into());
    names.into_iter().map(|name|ThemeEntry{values:load_theme(storage,&name,defaults),name}).collect()
}
pub fn add_theme(storage: &Storage, name: &str) -> std::io::Result<()> {
    // "java-classic" stays the bundled theme's name even after it is deleted (hidden).
    if !valid_name(name) || name.eq_ignore_ascii_case("default") || name.eq_ignore_ascii_case("java-classic")
        || catalog(storage,&ThemeSettings::default()).iter().any(|e|e.name.eq_ignore_ascii_case(name)) {
        return Err(std::io::Error::other("invalid or duplicate theme name"));
    }
    let file = theme_file(storage,name)?;
    let dir = file.parent().expect("theme directory");
    // Utils.java:1268-1277: every new theme starts from the jar's fixed template.
    let files: &[(&str,&[u8])] = &[
        ("black.png",include_bytes!("../../../ui/public/assets/newtheme/black.png")),
        ("white.png",include_bytes!("../../../ui/public/assets/newtheme/white.png")),
        ("board.png",include_bytes!("../../../ui/public/assets/board.png")),
        ("background.jpg",include_bytes!("../../../ui/public/assets/background.jpg")),
        ("theme.txt",include_bytes!("../../../ui/public/assets/newtheme/theme.txt")),
    ];
    std::fs::create_dir_all(themes_dir(storage))?;
    std::fs::create_dir(dir)?;
    let result = files.iter().try_for_each(|(name,bytes)|settings::write_atomic(&dir.join(name),bytes));
    if result.is_err() { let _ = std::fs::remove_dir_all(dir); }
    result
}
pub fn delete_theme(storage: &Storage, name: &str) -> std::io::Result<()> {
    let file = theme_file(storage,name)?;
    // The built-in files stay read-only; hide a deleted bundle and remove its user override.
    let dir = file.parent().expect("theme directory");
    if dir.exists() { std::fs::remove_dir_all(dir)?; }
    if BUNDLED.iter().any(|(n,_)| *n == name) {
        let path = storage.config_file.with_file_name("hidden-themes.json");
        let mut hidden: Vec<String> = settings::load_json(&path);
        if !hidden.iter().any(|h|h==name) { hidden.push(name.into()); settings::save_json(&path,&hidden)?; }
    }
    Ok(())
}

/// A whole theme folder to write: `theme.txt` and the image files it names, by file name.
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeFolder { pub name: String, pub files: Vec<(String, Vec<u8>)> }

/// Whether a theme folder holds the untouched bundled theme `name`: the same `theme.txt` values,
/// and every image it shows (`(file, bytes)`) the bundled file of that name.
pub fn bundled_copy(name: &str, raw: &Map<String,Value>, images: &[(&str,&[u8])]) -> bool {
    use sha2::{Digest, Sha256};
    let Some((_,bundled)) = BUNDLED.iter().find(|(n,_)| *n == name) else { return false };
    serde_json::from_str::<Value>(bundled).ok().as_ref().and_then(Value::as_object) == Some(raw)
        && images.iter().all(|(file,bytes)| { let hash = format!("{:x}",Sha256::digest(bytes));
            BUNDLED_IMAGES.iter().any(|(t,f,h)| *t == name && f == file && *h == hash) })
}
/// Entries of the themes folder named `name` in any letter case (one folder on Windows and macOS).
fn same_name_entries(dir: &Path, name: &str) -> Vec<PathBuf> {
    std::fs::read_dir(dir).into_iter().flatten().flatten()
        .filter(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name)).map(|e| e.path()).collect()
}
/// The folder already exists under exactly this name and holds these files.
fn unchanged(dir: &Path, folder: &ThemeFolder) -> bool {
    same_name_entries(dir,&folder.name).iter().any(|p| p.file_name().is_some_and(|n| *n == *folder.name))
        && folder.files.iter().all(|(file,bytes)| std::fs::read(dir.join(&folder.name).join(file)).is_ok_and(|b| b == *bytes))
}
/// The existing theme folders that writing `folders` would replace.
pub fn replaced_themes(storage: &Storage, folders: &[ThemeFolder]) -> Vec<String> {
    let dir = themes_dir(storage);
    folders.iter().filter(|f| !unchanged(&dir,f) && !same_name_entries(&dir,&f.name).is_empty()).map(|f| f.name.clone()).collect()
}
/// Theme folders written by a settings import; dropping it before [`Self::keep`] removes them
/// and puts every folder they replaced back.
pub(crate) struct ThemeInstall { work: Option<PathBuf>, created: Option<PathBuf>, moves: Vec<(PathBuf, PathBuf)> }
impl ThemeInstall {
    /// Keeps the written folders; whether any folder was written.
    pub(crate) fn keep(mut self) -> bool {
        let written = !self.moves.is_empty();
        self.moves.clear(); self.created = None;
        if let Some(work) = self.work.take() { let _ = std::fs::remove_dir_all(work); }
        written
    }
}
impl Drop for ThemeInstall {
    fn drop(&mut self) {
        for (from,to) in self.moves.drain(..).rev() { let _ = std::fs::rename(to,from); }
        if let Some(work) = self.work.take() { let _ = std::fs::remove_dir_all(work); }
        if let Some(dir) = self.created.take() { let _ = std::fs::remove_dir(dir); }
    }
}
/// Writes `folders` into the themes folder, each replacing any folder of the same name; folders
/// already holding the same files are left alone. All are written aside first and then moved in.
pub(crate) fn install_themes(storage: &Storage, folders: &[ThemeFolder]) -> std::io::Result<ThemeInstall> {
    let dir = themes_dir(storage);
    let mut install = ThemeInstall { work: None, created: None, moves: Vec::new() };
    let pending: Vec<&ThemeFolder> = folders.iter().filter(|f| !unchanged(&dir,f)).collect();
    if pending.is_empty() { return Ok(install); }
    if !dir.is_dir() { std::fs::create_dir_all(&dir)?; install.created = Some(dir.clone()); }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0,|d| d.as_nanos());
    let work = dir.join(format!(".import-{}-{stamp}",std::process::id()));
    std::fs::create_dir(&work)?;
    install.work = Some(work.clone());
    for (i,folder) in pending.iter().enumerate() {
        let staged = work.join(i.to_string());
        std::fs::create_dir(&staged)?;
        for (file,bytes) in &folder.files { std::fs::write(staged.join(file),bytes)?; }
    }
    for (i,folder) in pending.iter().enumerate() {
        for (j,old) in same_name_entries(&dir,&folder.name).into_iter().enumerate() {
            let aside = work.join(format!("replaced-{i}-{j}"));
            std::fs::rename(&old,&aside)?;
            install.moves.push((old,aside));
        }
        let (staged,target) = (work.join(i.to_string()),dir.join(&folder.name));
        std::fs::rename(&staged,&target)?;
        install.moves.push((staged,target));
    }
    Ok(install)
}

/// Only explicit image settings are read. Bytes are decoded before crossing IPC; no directory
/// is exposed through a WebView file/asset protocol (Theme.java:519-530 override precedence).
pub fn image_data(path: &Path) -> std::io::Result<String> {
    image_bytes(&std::fs::read(path)?)
}
fn image_bytes(bytes: &[u8]) -> std::io::Result<String> {
    Ok(format!("data:{};base64,{}",image_mime(bytes)?,base64::engine::general_purpose::STANDARD.encode(bytes)))
}
/// The MIME type of a decodable PNG, JPEG, GIF or BMP image.
fn image_mime(bytes: &[u8]) -> std::io::Result<&'static str> {
    let format = image::guess_format(bytes).map_err(std::io::Error::other)?;
    let mime = match format { image::ImageFormat::Png=>"image/png",image::ImageFormat::Jpeg=>"image/jpeg",
        image::ImageFormat::Gif=>"image/gif",image::ImageFormat::Bmp=>"image/bmp",_=>return Err(std::io::Error::other("unsupported image format")) };
    image::load_from_memory_with_format(bytes,format).map_err(std::io::Error::other)?;
    Ok(mime)
}
/// Whether `bytes` are an image this product shows.
pub fn is_image(bytes: &[u8]) -> bool { image_mime(bytes).is_ok() }
/// A TIFF picture, which Java's `ImageIO` reads but a webview does not show, as PNG: `None` when
/// `bytes` are not a TIFF, an error when the TIFF does not decode.
pub fn tiff_as_png(bytes: &[u8]) -> Option<std::io::Result<Vec<u8>>> {
    if image::guess_format(bytes).ok()? != image::ImageFormat::Tiff { return None; }
    Some((|| {
        let picture = image::load_from_memory_with_format(bytes, image::ImageFormat::Tiff).map_err(std::io::Error::other)?;
        // PNG has no floating-point samples.
        let picture = match picture {
            image::DynamicImage::ImageRgb32F(_) | image::DynamicImage::ImageRgba32F(_) => image::DynamicImage::ImageRgba8(picture.to_rgba8()),
            other => other,
        };
        let mut png = std::io::Cursor::new(Vec::new());
        picture.write_to(&mut png, image::ImageFormat::Png).map_err(std::io::Error::other)?;
        Ok(png.into_inner())
    })())
}
/// `appearance_images`: loadable URLs of the four theme images; `custom_board` /
/// `custom_background` are Theme.isUsingCustomBoardImage / isUsingCustomBackgroundImage
/// (Theme.java:210-217), the configured override decoded.
#[derive(Serialize, TS)]
#[serde(rename_all="camelCase")]
pub struct ThemeImages {
    pub board:String, pub background:String, pub black:String, pub white:String,
    pub custom_board:bool, pub custom_background:bool,
}
/// The image files shipped with each bundled theme (byte-identical to Java's `theme/` folders)
/// and their SHA-256, so an untouched Java copy of a bundled theme is recognised.
const BUNDLED_IMAGES: &[(&str, &str, &str)] = &[
    ("sabaki","background.png","273bce866dbb5cddfc88420783bcf1ce53126c4ecca064946aedf19c5170c3e2"),
    ("sabaki","black.png","8501003b04b61792cb88419720a3a3185e0f354fe96078c2ba619b4cae503f2c"),
    ("sabaki","board.png","6494a8d724cabf8211a3c3e640ac65ccfb8a47a2b0a0e2eb308e6ec71866e5e7"),
    ("sabaki","white.png","c71eee3ecca625de9f5561ed59b956f4b11972728c7a57729c51f2c9870a8cbb"),
    ("yasnaya","background2.png","2c81f36723892f21611cfacc1f9501b4f604f3c256b4f8fe67c3c34b5f7170aa"),
    ("yasnaya","black1.png","8c972e6321fd0a8ed8881b736ea2c698a1dea857e5f55ecaee4650a04a91f45e"),
    ("yasnaya","board7.jpg","b293b4faf13727f2717aa5594ebc4be9cf2512fde9eb374714de24cef6e1addc"),
    ("yasnaya","white1.png","d40aa399125062fca66e1eb2ce2d0996f0d3cd2fd5795ba84cdb0709b1c0851d"),
    ("Megapack","background.png","273bce866dbb5cddfc88420783bcf1ce53126c4ecca064946aedf19c5170c3e2"),
    ("Megapack","black.png","ae2dfeaddb159ed20687c8550fb6047a327b5801fe24814c0273569b947faff0"),
    ("Megapack","board.png","a6b3acebb3c627df8fa3ad73092c2f377c08290ba45939e7d104a96480fda563"),
    ("Megapack","white.png","6e882efec042173efcb230cfd93e5dbba84a42d78ada2cd00acff24c1337497c"),
    ("Custom","background.jpg","71cde6ab1d4d606f81fe2ed5627c6b83251dd6813c97b766942d1ae45d5b8636"),
    ("Custom","black1.png","8c972e6321fd0a8ed8881b736ea2c698a1dea857e5f55ecaee4650a04a91f45e"),
    ("Custom","board.png","be5a03ce70484a13f41e7fda5360f1ec4a6632f7579e821d583c59e0e42e8426"),
    ("Custom","white1.png","d40aa399125062fca66e1eb2ce2d0996f0d3cd2fd5795ba84cdb0709b1c0851d"),
    ("Fast","background.jpg","f619859a8781cec52b6b0ac9cf9e302e0ba4394e3345840e128e1068a5ca39a8"),
    ("Fast","black0.png","d716a6ecc27529ed388b72199e85a66f257f6431cc2fb28b1f35aa4b7658ad7b"),
    ("Fast","board.jpg","b293b4faf13727f2717aa5594ebc4be9cf2512fde9eb374714de24cef6e1addc"),
    ("Fast","white0.png","e2a25f80f301a8b0b9588ca8cda0248b33bc40e6b3b5e9d960ab90648cd884e1"),
];
fn bundled_image_exists(theme:&str, image:&str) -> bool {
    BUNDLED_IMAGES.iter().any(|(t,f,_)| *t == theme && *f == image)
}
/// The program's own default images, which a theme can name as `/assets/<file>`.
const DEFAULT_IMAGES: &[&str] = &["board.png","background.jpg","black0.png","white0.png"];
/// Whether `value` is the address of an image the program ships: `/assets/<file>` for its own
/// default images, `/themes/<theme>/<file>` for a bundled theme's. Any theme may name one (the
/// 主题外观 page's 棋盘 and 棋子 choices); the webview loads it as it is.
pub fn shipped_image(value:&str) -> bool {
    if let Some(file) = value.strip_prefix("/assets/") { return DEFAULT_IMAGES.contains(&file); }
    value.strip_prefix("/themes/").and_then(|rest| rest.split_once('/')).is_some_and(|(theme,file)| bundled_image_exists(theme,file))
}
pub fn images(storage:&Storage, appearance:&AppearanceSettings, theme:&ThemeSettings) -> ThemeImages {
    let resolve = |custom:&str, value:&str, fallback:&str| {
        for (index,candidate) in [custom,value].into_iter().enumerate() {
            if candidate.is_empty() { continue; }
            let is_custom=index==0;
            if candidate.starts_with("data:image/") || shipped_image(candidate) { return (candidate.to_string(),is_custom); }
            let path = Path::new(candidate);
            if path.is_absolute() { if let Ok(data) = image_data(path) { return (data,is_custom); } }
            else if valid_name(&appearance.theme) {
                // The custom overrides and the 默认 theme's own paths are relative to the themes
                // folder (Theme.java:502,523: Theme.pathPrefix), a named theme's to its folder.
                let root=if is_custom || appearance.theme == "default" { themes_dir(storage) } else { themes_dir(storage).join(&appearance.theme) };
                if let Ok(data) = image_data(&root.join(path)) { return (data,is_custom); }
                if !is_custom && bundled_image_exists(&appearance.theme,candidate) {
                    return (format!("/themes/{}/{}",appearance.theme,candidate.replace('\\',"/")),false);
                }
            }
        }
        (fallback.to_string(),false)
    };
    let (board,custom_board)=resolve(&appearance.custom_board_image,&theme.board_image,"/assets/board.png");
    let (background,custom_background)=resolve(&appearance.custom_background_image,&theme.background_image,"/assets/background.jpg");
    ThemeImages {board,background,custom_board,custom_background,
        black:resolve("",&theme.black_stone_image,"/assets/black0.png").0,
        white:resolve("",&theme.white_stone_image,"/assets/white0.png").0}
}


/// Newly copied immutable images belong to the surrounding settings transaction until kept;
/// dropped, they are removed with the folders created for them.
pub(crate) struct ImageCopies { files: Vec<PathBuf>, created: Vec<PathBuf> }
impl ImageCopies { pub(crate) fn keep(mut self) { self.files.clear(); self.created.clear(); } }
impl Drop for ImageCopies {
    fn drop(&mut self) {
        for path in &self.files { let _ = std::fs::remove_file(path); }
        for dir in &self.created { let _ = std::fs::remove_dir(dir); }
    }
}
pub(crate) fn capture_external_images(storage: &Storage, settings: &mut AppearanceSettings, theme: &mut ThemeSettings) -> std::io::Result<ImageCopies> {
    use sha2::{Digest, Sha256};
    let root = themes_dir(storage).join(".images");
    // Innermost first, so removing them in order empties the parent.
    let created = [root.clone(), themes_dir(storage)].into_iter().filter(|d| !d.exists()).collect();
    let mut copies = ImageCopies { files: Vec::new(), created };
    for value in [&mut settings.custom_board_image, &mut settings.custom_background_image,
        &mut theme.board_image, &mut theme.background_image, &mut theme.black_stone_image, &mut theme.white_stone_image] {
        if value.is_empty() { continue; }
        let path = Path::new(value.as_str());
        if path.starts_with(&root) && path.is_file() { continue; }
        let bytes = if value.starts_with("data:image/") {
            let (_, data) = value.split_once(',').ok_or_else(||std::io::Error::other("invalid image data"))?;
            base64::engine::general_purpose::STANDARD.decode(data).map_err(std::io::Error::other)?
        } else if path.is_absolute() && !value.starts_with("/assets/") && !value.starts_with("/themes/") {
            std::fs::read(path)?
        } else { continue; };
        let format = image::guess_format(&bytes).map_err(std::io::Error::other)?;
        image::load_from_memory_with_format(&bytes, format).map_err(std::io::Error::other)?;
        let file = root.join(format!("{:x}.{}", Sha256::digest(&bytes), format.extensions_str()[0]));
        if !file.is_file() { settings::write_atomic(&file, &bytes)?; copies.files.push(file.clone()); }
        *value = file.to_string_lossy().into_owned();
    }
    Ok(copies)
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use sha2::{Digest, Sha256};
    /// The table names exactly the shipped image files, with their current bytes.
    #[test]
    fn bundled_image_table_matches_the_shipped_files() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/public/themes");
        let mut shipped = Vec::new();
        for (theme,_) in super::BUNDLED {
            for entry in std::fs::read_dir(root.join(theme)).unwrap() {
                let path = entry.unwrap().path();
                let file = path.file_name().unwrap().to_string_lossy().into_owned();
                if file != "theme.txt" { shipped.push((theme.to_string(),file,format!("{:x}",Sha256::digest(std::fs::read(&path).unwrap())))); }
            }
        }
        let mut table: Vec<_> = super::BUNDLED_IMAGES.iter().map(|(t,f,h)|(t.to_string(),f.to_string(),h.to_string())).collect();
        shipped.sort(); table.sort();
        assert_eq!(table,shipped);
    }
    /// The themes Java ships and its 新建主题 template are all usable as they are.
    #[test]
    fn java_s_own_themes_have_no_invalid_values() {
        let template = include_str!("../../../ui/public/assets/newtheme/theme.txt");
        for raw in super::BUNDLED.iter().map(|(_,raw)|*raw).chain([template]) {
            let raw: serde_json::Map<String,serde_json::Value> = serde_json::from_str(raw.trim_start_matches('\u{feff}')).unwrap();
            assert!(super::invalid_java_keys(&raw).is_empty(), "{:?}", super::invalid_java_keys(&raw));
        }
    }
}
