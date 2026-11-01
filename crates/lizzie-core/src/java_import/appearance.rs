use super::{as_bool, as_i64, Handled, ImportReason, ImportStatus, ImportTarget, Reader};
use serde_json::{Map, Value};
use crate::appearance::{self, ThemeSettings};

const APPEARANCE: &[(&str,&str)] = &[
    ("theme","theme"),("board-style","boardStyle"),("is-apple-style","appleStyle"),
    ("use-morandi-colors","morandiColors"),("custom-board-image","customBoardImage"),
    ("glass-effect-level","glassEffectLevel"),
    ("custom-window-background-image","customBackgroundImage"),("frame-font-size","frameFontSize")];
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
        // Config.java:2226-2285 and Theme.java:219-356: named values override ui defaults.
        for &(key,field) in APPEARANCE {
            let Some(raw)=ui.get(key) else {continue}; handled.push(("ui",key));
            let value=match key {
                "theme"=>raw.as_str().filter(|v|appearance::valid_name(v)).map(|s|Value::from(s.to_string())),
                "board-style"=>raw.as_str().and_then(|v|match v {"japanese"=>Some(Value::from("Japanese")),"chinese-classic"=>Some(Value::from("ChineseClassic")),_=>None}),
                "is-apple-style"|"use-morandi-colors"=>as_bool(raw).map(Value::from),
                // Config.java:1871 reads optInt without the interactive dialog's 12..20 limit.
                "frame-font-size"|"glass-effect-level"=>as_i64(raw).and_then(|v|i32::try_from(v).ok()).map(Value::from),
                _=>self.appearance_image(raw,key),
            };
            match value {
                Some(value)=> {
                    if key=="theme" && value.as_str().is_some_and(|n| n!="default" && !appearance::BUNDLED.iter().any(|(b,_)|*b==n)) {
                        self.not_migrated(format!("ui.{key}"),raw); continue;
                    }
                    self.mapped.appearance.push((field,value)); self.imported(format!("ui.{key}"),ImportTarget::Appearance,super::shown(raw));
                },
                None=> {
                    if !key.contains("image") {self.invalid(format!("ui.{key}"),ImportTarget::Appearance,raw);}
                }
            }
        }
        // Do not make importing Apple change an explicitly configured palette; the renderer uses
        // Apple OR Morandi. Menu enabling Apple uses its separate Java transition.
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
