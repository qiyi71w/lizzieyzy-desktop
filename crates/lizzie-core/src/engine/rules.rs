//! KataGo rules (`KataGoRules`): parse a `kata-get-rules` answer or a preset name, compare
//! semantically, summarise for the captured panel and format for `kata-set-rules`.

use serde_json::{Map, Value};

const REQUIRED: [&str; 7] = [
    "ko",
    "scoring",
    "tax",
    "suicide",
    "hasButton",
    "whiteHandicapBonus",
    "friendlyPassOk",
];

/// A rules value: normalized JSON fields; key order is not identity.
#[derive(Clone, Debug, PartialEq)]
pub struct KataRules(Map<String, Value>);

/// One official preset: name, ko, scoring, tax, suicide, hasButton, whiteHandicapBonus,
/// friendlyPassOk.
type Preset = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    bool,
    bool,
    &'static str,
    bool,
);

/// Official presets in Java's order (`KataGoRules.officialPresets`).
pub const PRESETS: [Preset; 13] = [
    ("tromp-taylor", "POSITIONAL", "AREA", "NONE", true, false, "0", false),
    ("chinese", "SIMPLE", "AREA", "NONE", false, false, "N", true),
    ("chinese-ogs", "POSITIONAL", "AREA", "NONE", false, false, "N", true),
    ("chinese-kgs", "POSITIONAL", "AREA", "NONE", false, false, "N", true),
    ("japanese", "SIMPLE", "TERRITORY", "SEKI", false, false, "0", true),
    ("korean", "SIMPLE", "TERRITORY", "SEKI", false, false, "0", true),
    ("stone-scoring", "SIMPLE", "AREA", "ALL", false, false, "0", true),
    ("aga", "SITUATIONAL", "AREA", "NONE", false, false, "N-1", true),
    ("bga", "SITUATIONAL", "AREA", "NONE", false, false, "N-1", true),
    ("new-zealand", "SITUATIONAL", "AREA", "NONE", true, false, "0", true),
    ("aga-button", "SITUATIONAL", "AREA", "NONE", false, true, "N-1", true),
    ("goe", "POSITIONAL", "AREA", "NONE", true, false, "0", true),
    (
        "ancient-territory",
        "SIMPLE",
        "TERRITORY",
        "ALL",
        false,
        false,
        "0",
        false,
    ),
];

const ALIASES: [(&str, &str); 15] = [
    ("cn", "chinese"),
    ("jp", "japanese"),
    ("kr", "korean"),
    ("chineseogs", "chinese-ogs"),
    ("chinesekgs", "chinese-kgs"),
    ("stonescoring", "stone-scoring"),
    ("ancientarea", "stone-scoring"),
    ("ancient-area", "stone-scoring"),
    ("ancientterritory", "ancient-territory"),
    ("agabutton", "aga-button"),
    ("french", "aga"),
    ("nz", "new-zealand"),
    ("newzealand", "new-zealand"),
    ("tromptaylor", "tromp-taylor"),
    ("ing", "goe"),
];

impl KataRules {
    pub fn preset(name: &str) -> Option<KataRules> {
        let &(_, ko, scoring, tax, suicide, button, bonus, friendly) = PRESETS.iter().find(|p| p.0 == name)?;
        let mut m = Map::new();
        m.insert("ko".into(), ko.into());
        m.insert("scoring".into(), scoring.into());
        m.insert("tax".into(), tax.into());
        m.insert("suicide".into(), suicide.into());
        m.insert("hasButton".into(), button.into());
        m.insert("whiteHandicapBonus".into(), bonus.into());
        m.insert("friendlyPassOk".into(), friendly.into());
        Some(KataRules(m))
    }

    /// A JSON object (as `kata-get-rules` answers) or a preset name / alias; a GTP `=` prefix is
    /// ignored.
    pub fn parse(raw: &str) -> Option<KataRules> {
        let mut t = raw.trim();
        if let Some(rest) = t.strip_prefix(['=', '?']) {
            t = rest.trim_start_matches(|c: char| c.is_ascii_digit()).trim();
        }
        if t.is_empty() {
            return None;
        }
        if t.starts_with('{') {
            return match serde_json::from_str::<Value>(t) {
                Ok(Value::Object(m)) => Some(KataRules(m)),
                _ => None,
            };
        }
        let name = t.to_lowercase().replace(['_', ' '], "-");
        let name = ALIASES
            .iter()
            .find(|a| a.0 == name)
            .map_or(name.as_str(), |a| a.1)
            .to_string();
        KataRules::preset(&name)
    }

    fn matches(&self, preset: &str) -> bool {
        KataRules::preset(preset).is_some_and(|p| p == *self)
    }

    /// The official preset this value equals, if any.
    pub fn official_name(&self) -> Option<&'static str> {
        PRESETS.iter().map(|p| p.0).find(|n| self.matches(n))
    }

    /// `KataGoRules.summary().labelKey()`: the captured-panel caption key.
    pub fn summary_key(&self) -> &'static str {
        if self.matches("chinese") || self.matches("chinese-ogs") {
            "LizzieFrame.currentRules.chinese"
        } else if self.matches("japanese") {
            "LizzieFrame.currentRules.japanese"
        } else if self.matches("stone-scoring") {
            "LizzieFrame.currentRules.chn-ancient"
        } else if self.matches("tromp-taylor") {
            "LizzieFrame.currentRules.tromp-taylor"
        } else if self.matches("goe") {
            "LizzieFrame.currentRules.goe"
        } else {
            "LizzieFrame.currentRules.others"
        }
    }

    pub fn field(&self, name: &str) -> Option<&Value> {
        self.0.get(name)
    }

    /// All seven fields KataGo needs are present (`hasRequiredFields`).
    pub fn has_required_fields(&self) -> bool {
        REQUIRED.iter().all(|k| self.0.get(*k).is_some_and(|v| !v.is_null()))
    }

    /// `fieldSummary`: required fields in order, then the others, as `key=value, …`.
    pub fn field_summary(&self) -> String {
        let text = |v: &Value| match v {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let mut parts: Vec<String> = REQUIRED
            .iter()
            .filter_map(|k| {
                self.0
                    .get(*k)
                    .filter(|v| !v.is_null())
                    .map(|v| format!("{k}={}", text(v)))
            })
            .collect();
        let mut rest: Vec<(&String, &Value)> = self
            .0
            .iter()
            .filter(|(k, v)| !REQUIRED.contains(&k.as_str()) && !v.is_null())
            .collect();
        rest.sort_by(|a, b| a.0.cmp(b.0));
        parts.extend(rest.into_iter().map(|(k, v)| format!("{k}={}", text(v))));
        parts.join(", ")
    }

    /// The `kata-set-rules` argument: the JSON object.
    pub fn to_gtp_argument(&self) -> String {
        Value::Object(self.0.clone()).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_answer_is_summarised_by_exact_preset_match() {
        let chinese = KataRules::parse(
            r#"= {"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#,
        )
        .unwrap();
        assert_eq!(chinese.summary_key(), "LizzieFrame.currentRules.chinese");
        assert_eq!(KataRules::parse("cn"), Some(chinese));
        let tt = KataRules::parse("Tromp_Taylor").unwrap();
        assert_eq!(tt.summary_key(), "LizzieFrame.currentRules.tromp-taylor");
        // Korean has exactly the Japanese fields, so Java captions it as Japanese.
        assert_eq!(
            KataRules::parse("korean").unwrap().summary_key(),
            "LizzieFrame.currentRules.japanese"
        );
        assert_eq!(
            KataRules::parse("aga").unwrap().summary_key(),
            "LizzieFrame.currentRules.others"
        );
    }
}
