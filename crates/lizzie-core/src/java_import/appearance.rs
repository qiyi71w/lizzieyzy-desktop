use super::{as_bool, as_i64, Handled, ImportReason, ImportStatus, ImportTarget, Mapped, Reader};
use serde_json::{Map, Value};
use crate::appearance::{self, ThemeFolder, ThemeSettings, JAVA_SOFT};
use crate::settings::Storage;

const APPEARANCE: &[(&str,&str)] = &[
    ("theme","theme"),("board-style","boardStyle"),("is-apple-style","mode"),
    ("custom-board-image","customBoardImage"),
    ("glass-effect-level","glassEffectLevel"),
    ("custom-window-background-image","customBackgroundImage"),("frame-font-size","frameFontSize")];
/// `LizzieConfig.title.defaultTheme` in every Java language: the theme tab saves the label of its
/// first entry, the default theme (ConfigDialog2.java:4384, 6864), which no folder holds.
const DEFAULT_THEME_LABELS: [&str; 7] = ["Default","デフォルト","기본값","ค่าเริ่มต้น","默认","默認","預設"];
const THEME: &[(&str,&str)] = &[
    ("glass-panel-overlay-color","glassPanelOverlayColor"),("glass-panel-border-color","glassPanelBorderColor"),
    ("glass-panel-highlight-color","glassPanelHighlightColor"),("glass-panel-shadow-color","glassPanelShadowColor"),
    ("glass-accent-color","glassAccentColor"),("glass-blur-radius","glassBlurRadius"),
    ("liquid-blur-radius","liquidBlurRadius"),("glass-corner-radius","glassCornerRadius"),
    ("use-pure-board","usePureBoard"),("use-pure-background","usePureBackground"),("use-pure-stone","usePureStone"),
    ("pure-board-color","pureBoardColor"),("pure-background-color","pureBackgroundColor"),
    ("show-stone-shadow","showStoneShadow"),("shadow-size","shadowSize"),("stone-indicator-type","stoneIndicatorType"),
    ("winrate-stroke-width","winrateStrokeWidth"),("score-lead-stroke-width","scoreLeadStrokeWidth"),
    ("minimum-blunder-bar-width","minimumBlunderBarWidth"),("comment-font-size","commentFontSize"),("background-filter","backgroundFilter"),
    ("winrate-line-color","winrateLineColor"),("winrate-miss-line-color","winrateMissLineColor"),
    ("scoremean-line-color","scoremeanLineColor"),("blunder-bar-color","blunderBarColor"),("best-move-color","bestMoveColor"),
    ("comment-background-color","commentBackgroundColor"),("comment-font-color","commentFontColor"),("comment-node-color","commentNodeColor"),
    ("show-comment-node-color","showCommentNodeColor"),("use-scorediff-in-variation-tree","useScorediffInVariationTree"),
    ("scorediff-in-variation-tree-factor","scorediffInVariationTreeFactor"),("font-name","fontName"),
    ("ui-font-name","uiFontName"),("winrate-font-name","winrateFontName"),("board-image","boardImage"),
    ("background-image","backgroundImage"),("black-stone-image","blackStoneImage"),("white-stone-image","whiteStoneImage"),
    ("blunder-winrate-thresholds","blunderWinrateThresholds"),("blunder-node-colors","blunderNodeColors")];

impl Reader<'_> {
    pub(super) fn appearance(&mut self, ui:&Map<String,Value>, handled:&mut Handled) {
        // Java's Morandi palette (经典配色 off, Config.java:1808) over its 默认 theme (also what it
        // shows without a theme name, Theme.java:63-68) is the bundled java-soft theme here.
        let morandi=ui.get("use-morandi-colors").filter(|raw| as_bool(raw)==Some(true));
        let mut soft=morandi.is_some() && !ui.contains_key("theme");
        // Config.java:2226-2285 and Theme.java:219-356: named values override ui defaults.
        for &(key,field) in APPEARANCE {
            let Some(raw)=ui.get(key) else {continue}; handled.push(("ui",key));
            let value=match key {
                // Java reads theme/<ui.theme>/ (Theme.java:63-68): an imported folder wins, under
                // its name here; otherwise the default entry's labels mean the default theme.
                "theme"=>raw.as_str().map(|v| {
                    let name = super::themes::product_name(v);
                    if self.mapped.themes.iter().any(|t| t.name == name) { name }
                    else if v.eq_ignore_ascii_case("default") || DEFAULT_THEME_LABELS.contains(&v) {
                        soft=morandi.is_some();
                        (if soft { JAVA_SOFT } else { "default" }).to_string()
                    }
                    else { v.to_string() }
                }).filter(|v|appearance::valid_name(v)).map(Value::from),
                "board-style"=>raw.as_str().and_then(|v|match v {"japanese"=>Some(Value::from("Japanese")),"chinese-classic"=>Some(Value::from("ChineseClassic")),_=>None}),
                // 外观模式 replaces Java's interface styles: Apple style was its dark interface.
                "is-apple-style"=>as_bool(raw).map(|apple| Value::from(if apple {"Dark"} else {"System"})),
                // Config.java:1871 reads optInt without the interactive dialog's 12..20 limit.
                "frame-font-size"|"glass-effect-level"=>as_i64(raw).and_then(|v|i32::try_from(v).ok()).map(Value::from),
                _=>self.appearance_image(raw,key),
            };
            match value {
                Some(value)=> {
                    if key=="theme" && value.as_str().is_some_and(|n| n!="default" && !appearance::BUNDLED.iter().any(|(b,_)|*b==n)
                        && !self.mapped.themes.iter().any(|t| t.name == n)) {
                        // Java shows its default values for a theme without a readable folder.
                        let it=self.item(format!("ui.{key}"),ImportStatus::Incompatible);
                        it.target=Some(ImportTarget::Appearance); it.reason=Some(ImportReason::Unreadable);
                        it.value=Some(super::sanitize(&super::shown(raw))); it.detail=value.as_str().map(|n| super::sanitize(&format!("theme/{n}/theme.txt")));
                        continue;
                    }
                    self.mapped.appearance.push((field,value)); self.imported(format!("ui.{key}"),ImportTarget::Appearance,super::shown(raw));
                },
                None=> {
                    if !key.contains("image") {self.invalid(format!("ui.{key}"),ImportTarget::Appearance,raw);}
                }
            }
        }
        if let Some(raw)=morandi {
            // Anything else is Java's 经典配色, reported with the unused keys.
            handled.push(("ui","use-morandi-colors"));
            if soft {
                self.mapped.soft_theme=true;
                if !ui.contains_key("theme") { self.mapped.appearance.push(("theme",Value::from(JAVA_SOFT))); }
                self.imported("ui.use-morandi-colors".into(),ImportTarget::Appearance,super::shown(raw));
            } else {
                // A named theme is kept: this product has no Morandi palette over it.
                self.not_migrated("ui.use-morandi-colors".into(),raw);
                self.items.last_mut().expect("item just reported").target=Some(ImportTarget::Appearance);
            }
        }
        let mut theme=serde_json::to_value(ThemeSettings::default()).expect("theme serializes");
        let mut image_failures=Vec::new();
        for &(key,field) in THEME {
            let Some(raw)=ui.get(key) else {continue}; handled.push(("ui",key));
            let value=if key.ends_with("-image") {self.appearance_image(raw,key)}
                else if matches!(key,"glass-blur-radius"|"liquid-blur-radius"|"glass-corner-radius") {
                    as_i64(raw).and_then(|v|i32::try_from(v).ok()).map(|v|Value::from(v.max(if key=="glass-corner-radius" {4} else {0})))
                } else {Some(raw.clone())};
            let Some(value)=value else {
                if !key.ends_with("-image") { self.invalid(format!("ui.{key}"),ImportTarget::Appearance,raw); }
                image_failures.push(key); continue
            };
            theme[field]=value.clone();
        }
        let accepted=serde_json::from_value::<ThemeSettings>(theme.clone()).ok().filter(ThemeSettings::valid);
        // Blunder rules are one validated pair, just as Theme.java:385-422 reads them.
        for &(key,field) in THEME {
            let Some(raw)=ui.get(key) else {continue};
            if image_failures.contains(&key) {continue;}
            if accepted.is_some() {
                self.mapped.theme.push((field,theme[field].clone()));
                self.imported(format!("ui.{key}"),ImportTarget::Appearance,super::shown(raw));
            } else {
                let mut one=serde_json::to_value(ThemeSettings::default()).expect("theme serializes");
                one[field]=theme[field].clone();
                if key.starts_with("blunder-") && (key.ends_with("thresholds") || key.ends_with("colors")) {
                    one["blunderWinrateThresholds"]=theme["blunderWinrateThresholds"].clone();
                    one["blunderNodeColors"]=theme["blunderNodeColors"].clone();
                }
                if serde_json::from_value::<ThemeSettings>(one).ok().is_some_and(|s|s.valid()) {
                    self.mapped.theme.push((field,theme[field].clone())); self.imported(format!("ui.{key}"),ImportTarget::Appearance,super::shown(raw));
                } else {self.invalid(format!("ui.{key}"),ImportTarget::Appearance,raw);}
            }
        }
    }
    fn appearance_image(&mut self, raw:&Value,key:&str) -> Option<Value> {
        let Some(value)=raw.as_str() else {self.invalid(format!("ui.{key}"),ImportTarget::Appearance,raw);return None};
        if value.trim().is_empty() {return Some(Value::from(""));}
        // Theme.java:519-556 resolves custom/ui paths relative to theme/, then absolute paths.
        let p=std::path::Path::new(value);
        let paths=if p.is_absolute() {vec![p.to_path_buf()]} else {
            self.source.roots().iter().flat_map(|root|[root.join("theme").join(p),root.join(p)]).collect()
        };
        for path in &paths { if let Ok(bytes)=appearance::image_data(path) {return Some(Value::from(bytes));} }
        let item=self.item(format!("ui.{key}"),ImportStatus::Incompatible);
        item.target=Some(ImportTarget::Appearance); item.reason=Some(ImportReason::Unreadable);
        item.detail=Some(super::sanitize(&paths.iter().map(|p|p.to_string_lossy()).collect::<Vec<_>>().join("\n")));
        None
    }
}

impl Mapped {
    /// Adds to [`Mapped::themes`] java-soft's folder when Java's Morandi palette chose java-soft:
    /// its `theme.txt` as saving it on the 主题外观 page with the 默认 theme's imported values
    /// writes it (each picture named as the import keeps it), with the imported values the page
    /// does not edit (the glass values) as well. Values that equal java-soft's own need no folder,
    /// unless the user's own java-soft folder would keep other values. Run once, by the preview.
    pub fn settle_theme_folders(&mut self, storage:&Storage) {
        if !std::mem::take(&mut self.soft_theme) {return;}
        let mut theme=serde_json::to_value(ThemeSettings::default()).expect("theme serializes");
        for &(field,ref value) in &self.theme {
            theme[field]=match value.as_str() {
                Some(image) if field.ends_with("Image") => Value::from(appearance::owned_image_name(storage,image)),
                _ => value.clone(),
            };
        }
        let settings:ThemeSettings=serde_json::from_value(theme.clone()).expect("validated theme values");
        let Value::Object(mut raw)=appearance::to_java(JAVA_SOFT,None,&settings) else {unreachable!()};
        for &(field,_) in &self.theme {
            let key=appearance::java_key(field);
            if !appearance::PAGE_KEYS.contains(&key.as_str()) { raw.insert(key,theme[field].clone()); }
        }
        let bundled=appearance::BUNDLED.iter().find(|(n,_)| *n==JAVA_SOFT)
            .and_then(|(_,raw)| serde_json::from_str::<Map<String,Value>>(raw).ok()).unwrap_or_default();
        let base=ThemeSettings::default();
        if appearance::merge_java(&base,&raw)==appearance::merge_java(&base,&bundled) && !appearance::has_folder(storage,JAVA_SOFT) {return;}
        let bytes=serde_json::to_vec_pretty(&raw).expect("theme serializes");
        self.themes.push(ThemeFolder { name:JAVA_SOFT.into(), files:vec![("theme.txt".into(),bytes)] });
    }
}
