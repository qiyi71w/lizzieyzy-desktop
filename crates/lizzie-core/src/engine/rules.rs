//! KataGo rules (`KataGoRules`): parse a `kata-get-rules` answer or a preset name, compare
//! semantically, summarise for the captured panel and format for `kata-set-rules`; and the rule
//! sets (`RulePreset`), rules together with the komi that belongs to them.

use std::sync::LazyLock;

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

/// Official presets in Java's order (`KataGoRules.officialPresets`). KataGo reports
/// `friendlyPassOk` false for its territory presets (`kata-get-rules`, 1.18.1).
pub const PRESETS: [Preset; 13] = [
    ("tromp-taylor", "POSITIONAL", "AREA", "NONE", true, false, "0", false),
    ("chinese", "SIMPLE", "AREA", "NONE", false, false, "N", true),
    ("chinese-ogs", "POSITIONAL", "AREA", "NONE", false, false, "N", true),
    ("chinese-kgs", "POSITIONAL", "AREA", "NONE", false, false, "N", true),
    ("japanese", "SIMPLE", "TERRITORY", "SEKI", false, false, "0", false),
    ("korean", "SIMPLE", "TERRITORY", "SEKI", false, false, "0", false),
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

/// An engine-game rules choice: preset name and label key.
pub type MatchRuleOption = (&'static str, &'static str);

/// The engine game's standard rules choices in Java's order, each with its label
/// (`MatchRuleOption`); anything else is 自定义.
pub const MATCH_RULE_OPTIONS: [MatchRuleOption; 7] = [
    ("chinese", "LizzieFrame.currentRules.chinese"),
    ("stone-scoring", "LizzieFrame.currentRules.chn-ancient"),
    ("japanese", "MatchRules.option.japaneseKorean"),
    ("aga", "MatchRules.option.agaBga"),
    ("new-zealand", "MatchRules.option.newZealand"),
    ("tromp-taylor", "MatchRules.option.trompTaylor"),
    ("goe", "LizzieFrame.currentRules.goe"),
];

/// A rule set (`RulePreset`): KataGo rules together with the komi that belongs to them.
///
/// The Chinese rule sets are KataGo's chinese rules and differ in komi and in how a draw is
/// broken. The 2027 Chinese rules have Black give 7 points, so a game can end in a draw; the 2002
/// rules gave 7.5. Where a game must have a winner, the 2027 rules break a draw either for White,
/// which counts exactly like komi 7.5, or for the player who passed first, which is KataGo's
/// button rule with komi 7.
#[derive(Debug)]
pub struct RuleSet {
    /// Java's name for it (the `rule-preset` setting holds it).
    pub id: &'static str,
    /// The rules as `KataRules::parse` takes them: a preset name or a JSON object.
    rules: &'static str,
    pub komi: f64,
    /// Display-string key of its entry in a list of rule sets (`RulesQuickMenu.*`).
    pub label_key: &'static str,
    /// Display-string key of the rules caption, when it is not the rules' summary.
    caption_key: Option<&'static str>,
    /// It counts exactly like another rule set and is recognised only when chosen.
    draw_rule: bool,
}

/// The rule sets in Java's order (`RulePreset.values()`).
pub const RULE_SETS: [RuleSet; 8] = [
    RuleSet {
        id: "CHINESE_2027",
        rules: "chinese",
        komi: 7.0,
        label_key: "RulesQuickMenu.chinese2027",
        caption_key: Some("LizzieFrame.currentRules.chinese2027"),
        draw_rule: false,
    },
    RuleSet {
        id: "CHINESE_2027_DRAW_TO_WHITE",
        rules: "chinese",
        komi: 7.5,
        label_key: "RulesQuickMenu.chinese2027DrawToWhite",
        caption_key: Some("LizzieFrame.currentRules.chinese2027DrawToWhite"),
        draw_rule: true,
    },
    RuleSet {
        id: "CHINESE_2027_DRAW_TO_FIRST_PASS",
        // KataGo's chinese rules with the button, which the player who passes first takes.
        rules: r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":true,"whiteHandicapBonus":"N","friendlyPassOk":true}"#,
        komi: 7.0,
        label_key: "RulesQuickMenu.chinese2027DrawToFirstPass",
        caption_key: Some("LizzieFrame.currentRules.chinese2027DrawToFirstPass"),
        draw_rule: true,
    },
    RuleSet {
        id: "CHINESE",
        rules: "chinese",
        komi: 7.5,
        label_key: "RulesQuickMenu.chinese",
        caption_key: None,
        draw_rule: false,
    },
    RuleSet {
        id: "JAPANESE",
        rules: "japanese",
        komi: 6.5,
        label_key: "RulesQuickMenu.japanese",
        caption_key: None,
        draw_rule: false,
    },
    RuleSet {
        id: "CHINESE_ANCIENT",
        rules: "stone-scoring",
        komi: 7.5,
        label_key: "RulesQuickMenu.chnAncient",
        caption_key: None,
        draw_rule: false,
    },
    RuleSet {
        id: "TROMP_TAYLOR",
        rules: "tromp-taylor",
        komi: 7.5,
        label_key: "RulesQuickMenu.trompTaylor",
        caption_key: None,
        draw_rule: false,
    },
    RuleSet {
        id: "GOE",
        rules: "goe",
        komi: 7.5,
        label_key: "RulesQuickMenu.goe",
        caption_key: None,
        draw_rule: false,
    },
];

/// Each rule set's parsed rules and their summary key, in `RULE_SETS` order.
static RULE_SET_RULES: LazyLock<Vec<(KataRules, &'static str)>> = LazyLock::new(|| {
    RULE_SETS
        .iter()
        .map(|s| {
            let rules = KataRules::parse(s.rules).expect("rule set rules parse");
            let summary = rules.summary_key();
            (rules, summary)
        })
        .collect()
});

/// The komi new games start with when no rule set is chosen, as far as the even-game komi goes
/// (`RulePreset.evenGameKomi` asks `newGameKomi(config, 7.5f)`).
const NEW_GAME_KOMI: f64 = 7.5;

impl RuleSet {
    pub fn rules(&self) -> &'static KataRules {
        &self.parsed().0
    }

    /// Display-string key of the rules caption: the rule set's own short name, else the rules'
    /// summary.
    pub fn caption_key(&self) -> &'static str {
        self.caption_key.unwrap_or(self.parsed().1)
    }

    /// The rule set these rules and komi are (`RulePreset.of`). Rule sets with the same rules are
    /// told apart by komi; one whose rules no other rule set has is recognised whatever the komi.
    /// A draw rule that counts like another rule set is recognised only when it is `chosen`, the
    /// one picked last.
    pub fn of(rules: &KataRules, komi: f64, chosen: Option<&RuleSet>) -> Option<&'static RuleSet> {
        RuleSet::recognise(rules, rules.summary_key(), komi, chosen)
    }

    fn recognise(rules: &KataRules, summary: &str, komi: f64, chosen: Option<&RuleSet>) -> Option<&'static RuleSet> {
        let matches = || {
            RULE_SETS
                .iter()
                .filter(move |s| s.has_rules(rules, summary) && (!s.shares_rules() || s.komi == komi))
        };
        if let Some(chosen) = chosen.and_then(|c| matches().find(|s| s.id == c.id)) {
            return Some(chosen);
        }
        let single = matches().count() == 1;
        matches().find(|s| !s.draw_rule || single)
    }

    /// The komi of an even game under the rules KataGo plays now (`RulePreset.evenGameKomi`):
    /// that of the rule set they are, such as 7 for the 2027 rules or 6.5 for the Japanese; else,
    /// after a handicap game, the komi new games start with (the chosen rule set's, or 7.5); else
    /// the current game's `komi`.
    pub fn even_game_komi(rules: Option<&KataRules>, komi: f64, handicap_game: bool, chosen: Option<&RuleSet>) -> f64 {
        match rules.and_then(|r| RuleSet::of(r, komi, chosen)) {
            Some(set) => set.komi,
            None if handicap_game => chosen.map_or(NEW_GAME_KOMI, |c| c.komi),
            None => komi,
        }
    }

    fn parsed(&self) -> &'static (KataRules, &'static str) {
        let at = RULE_SETS
            .iter()
            .position(|s| s.id == self.id)
            .expect("one of RULE_SETS");
        &RULE_SET_RULES[at]
    }

    /// A named KataGo rule set is recognised by its summary, which allows its close variants;
    /// other rules field for field.
    fn has_rules(&self, observed: &KataRules, observed_summary: &str) -> bool {
        let (rules, summary) = self.parsed();
        if *summary == "LizzieFrame.currentRules.others" {
            rules == observed
        } else {
            *summary == observed_summary
        }
    }

    fn shares_rules(&self) -> bool {
        let (rules, summary) = self.parsed();
        RULE_SETS
            .iter()
            .any(|other| other.id != self.id && other.has_rules(rules, summary))
    }
}

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

    /// Whether these rules are preset `name`. Territory scoring does not use `friendlyPassOk`
    /// (the rules editor disables it there), so a territory preset matches whatever that field
    /// says (`matchesOfficialApartFromFriendlyPass`); other presets match field for field.
    pub fn matches_preset(&self, name: &str) -> bool {
        let Some(preset) = KataRules::preset(name) else {
            return false;
        };
        if preset.0.get("scoring").and_then(Value::as_str) != Some("TERRITORY") {
            return preset == *self;
        }
        let others = |m: &Map<String, Value>| m.iter().filter(|(k, _)| *k != "friendlyPassOk").count();
        others(&self.0) == others(&preset.0)
            && preset
                .0
                .iter()
                .filter(|(k, _)| *k != "friendlyPassOk")
                .all(|(k, v)| self.0.get(k) == Some(v))
    }

    /// The engine-game choice (`MatchRuleOption.matching`) these rules are, if any.
    pub fn match_option(&self) -> Option<&'static MatchRuleOption> {
        MATCH_RULE_OPTIONS.iter().find(|o| self.matches_preset(o.0))
    }

    /// KataGo's button: a drawn count goes to the first player to pass (`FirstPassDraw.applies`).
    pub fn has_button(&self) -> bool {
        self.0.get("hasButton").and_then(Value::as_bool).unwrap_or(false)
    }

    /// The official preset this value equals, if any.
    pub fn official_name(&self) -> Option<&'static str> {
        PRESETS.iter().map(|p| p.0).find(|n| self.matches_preset(n))
    }

    /// `KataGoRules.summary().labelKey()`: the rules' summary caption key.
    pub fn summary_key(&self) -> &'static str {
        if self.matches_preset("chinese") || self.matches_preset("chinese-ogs") {
            "LizzieFrame.currentRules.chinese"
        } else if self.matches_preset("japanese") {
            "LizzieFrame.currentRules.japanese"
        } else if self.matches_preset("stone-scoring") {
            "LizzieFrame.currentRules.chn-ancient"
        } else if self.matches_preset("tromp-taylor") {
            "LizzieFrame.currentRules.tromp-taylor"
        } else if self.matches_preset("goe") {
            "LizzieFrame.currentRules.goe"
        } else {
            "LizzieFrame.currentRules.others"
        }
    }

    /// The captured-panel rules caption key with the game's `komi` (`RulePreset.captionKey`): the
    /// rule set's short name when these rules and komi are one, else the rules' summary.
    pub fn caption_key(&self, komi: f64, chosen: Option<&RuleSet>) -> &'static str {
        let summary = self.summary_key();
        RuleSet::recognise(self, summary, komi, chosen).map_or(summary, RuleSet::caption_key)
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
