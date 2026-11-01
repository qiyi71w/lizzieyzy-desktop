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

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct ThemeNodeColor { pub node: crate::record::NodeId, pub color: Vec<u8> }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme: String,
    pub board_style: BoardStyle,
    pub apple_style: bool,
    pub morandi_colors: bool,
    pub custom_board_image: String,
    pub custom_background_image: String,
    pub frame_font_size: i32,
    /// Config.java:1809; no Java UI changes it. Legacy level 0 uses the theme's background blur.
    pub glass_effect_level: i32,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        // Config.java:1608,1871,2924; Menu.java:604-629,733-775.
        Self { theme: "default".into(), board_style: BoardStyle::Japanese, apple_style: false,
            morandi_colors: false, custom_board_image: String::new(), custom_background_image: String::new(), frame_font_size: 12,
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
            // Theme.java:738-767: the glass theme, used for glass-effect-level > 0 / Apple.
            glass_panel_overlay_color:vec![24,24,26,102], glass_panel_border_color:vec![255,255,255,26],
            glass_panel_highlight_color:vec![255,255,255,77], glass_panel_shadow_color:vec![0,0,0,64], glass_accent_color:vec![96,165,250],
            glass_blur_radius:10, liquid_blur_radius:15, glass_corner_radius:12,
            blunder_node_colors: vec![vec![155,25,150],vec![208,16,19],vec![200,140,50],vec![180,180,0],vec![140,202,34],vec![0,220,0],vec![0,210,210]] }
    }
}
impl ThemeSettings {
    pub fn valid(&self) -> bool {
        let colors = [&self.pure_board_color,&self.pure_background_color,&self.winrate_line_color,&self.winrate_miss_line_color,
            &self.scoremean_line_color,&self.blunder_bar_color,&self.best_move_color,&self.comment_background_color,
            &self.comment_font_color,&self.comment_node_color,&self.glass_panel_overlay_color,&self.glass_panel_border_color,
            &self.glass_panel_highlight_color,&self.glass_panel_shadow_color,&self.glass_accent_color];
        colors.iter().all(|c| matches!(c.len(),3|4)) && self.blunder_node_colors.iter().all(|c| matches!(c.len(),3|4))
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
pub fn to_java(theme: &ThemeSettings) -> Value {
    let Value::Object(fields) = serde_json::to_value(theme).expect("theme serializes") else { unreachable!() };
    Value::Object(fields.into_iter().map(|(k,v)|(java_key(&k),v)).collect())
}
pub fn load_theme(storage: &Storage, name: &str, defaults: &ThemeSettings) -> ThemeSettings {
    if name == "default" || !valid_name(name) { return defaults.clone(); }
    let hidden: Vec<String> = settings::load_json(&storage.config_file.with_file_name("hidden-themes.json"));
    if hidden.iter().any(|hidden| hidden == name) && theme_file(storage,name).is_ok_and(|path|!path.is_file()) { return defaults.clone(); }
    let bundled = BUNDLED.iter().find(|(n,_)| *n == name).and_then(|(_,raw)|serde_json::from_str::<Value>(raw).ok());
    let mut theme = bundled.as_ref().and_then(Value::as_object).map_or_else(|| defaults.clone(), |v| merge_java(defaults,v));
    if let Ok(path) = theme_file(storage,name) {
        if let Ok(bytes) = std::fs::read(path) { if let Ok(Value::Object(raw)) = serde_json::from_slice(&bytes) { theme = merge_java(&theme,&raw); } }
    }
    theme
}
#[derive(Serialize)]
pub struct ThemeEntry { pub name: String, pub values: ThemeSettings }
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
    if !valid_name(name) || name.eq_ignore_ascii_case("default")
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

/// Only explicit image settings are read. Bytes are decoded before crossing IPC; no directory
/// is exposed through a WebView file/asset protocol (Theme.java:519-530 override precedence).
pub fn image_data(path: &Path) -> std::io::Result<String> {
    image_bytes(&std::fs::read(path)?)
}
fn image_bytes(bytes: &[u8]) -> std::io::Result<String> {
    let format = image::guess_format(&bytes).map_err(std::io::Error::other)?;
    image::load_from_memory_with_format(&bytes,format).map_err(std::io::Error::other)?;
    let mime = match format { image::ImageFormat::Png=>"image/png",image::ImageFormat::Jpeg=>"image/jpeg",
        image::ImageFormat::Gif=>"image/gif",image::ImageFormat::Bmp=>"image/bmp",_=>return Err(std::io::Error::other("unsupported image format")) };
    Ok(format!("data:{mime};base64,{}",base64::engine::general_purpose::STANDARD.encode(bytes)))
}
#[derive(Serialize)]
#[serde(rename_all="camelCase")]
pub struct ThemeImages {
    pub board:String, pub background:String, pub black:String, pub white:String,
    pub custom_board:bool, pub custom_background:bool,
}
fn bundled_image_exists(theme:&str, image:&str) -> bool {
    match theme {
        "sabaki"|"Megapack" => matches!(image,"board.png"|"background.png"|"black.png"|"white.png"),
        "yasnaya" => matches!(image,"board7.jpg"|"background2.png"|"black1.png"|"white1.png"),
        "Custom" => matches!(image,"board.png"|"background.jpg"|"black1.png"|"white1.png"),
        "Fast" => matches!(image,"board.jpg"|"background.jpg"|"black0.png"|"white0.png"),
        _ => false,
    }
}
pub fn images(storage:&Storage, appearance:&AppearanceSettings, theme:&ThemeSettings) -> ThemeImages {
    let resolve = |custom:&str, value:&str, fallback:&str| {
        for (index,candidate) in [custom,value].into_iter().enumerate() {
            if candidate.is_empty() { continue; }
            let is_custom=index==0;
            if candidate.starts_with("data:image/") { return (candidate.to_string(),is_custom); }
            let path = Path::new(candidate);
            if path.is_absolute() { if let Ok(data) = image_data(path) { return (data,is_custom); } }
            else if valid_name(&appearance.theme) {
                let root=if is_custom { themes_dir(storage) } else { themes_dir(storage).join(&appearance.theme) };
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


/// Newly copied immutable images belong to the surrounding settings transaction until kept.
pub(crate) struct ImageCopies(Vec<PathBuf>);
impl ImageCopies { pub(crate) fn keep(mut self) { self.0.clear(); } }
impl Drop for ImageCopies {
    fn drop(&mut self) { for path in &self.0 { let _ = std::fs::remove_file(path); } }
}
pub(crate) fn capture_external_images(storage: &Storage, settings: &mut AppearanceSettings, theme: &mut ThemeSettings) -> std::io::Result<ImageCopies> {
    use sha2::{Digest, Sha256};
    let root = themes_dir(storage).join(".images");
    let mut copies = ImageCopies(Vec::new());
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
        if !file.is_file() { settings::write_atomic(&file, &bytes)?; copies.0.push(file.clone()); }
        *value = file.to_string_lossy().into_owned();
    }
    Ok(copies)
}
