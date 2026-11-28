//! KataGo rules (`KataGoRules`): parse a `kata-get-rules` answer or a preset name, compare
//! semantically, summarise for the captured panel and format for `kata-set-rules`; and the rule
//! sets (`RulePreset`), rules together with the komi that belongs to them.

use std::sync::LazyLock;

use serde_json::{Map, Value};

use super::profile::EngineSettings;

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

/// Each preset's rules, in `PRESETS` order: recognising rules compares with these, so it builds
/// nothing.
static PRESET_RULES: LazyLock<Vec<KataRules>> = LazyLock::new(|| PRESETS.iter().map(KataRules::of_preset).collect());

/// An engine-game rules choice (`MatchRuleOption`): rules, the dialog's entry and the komi that goes
/// with them.
#[derive(Debug)]
pub struct MatchRuleOption {
    /// How the engine-game settings remember the choice; the Chinese ones are their rule set's id.
    pub id: &'static str,
    /// The rules sent to the engines, as `KataRules::parse` takes them: a preset name or a JSON
    /// object.
    pub rules: &'static str,
    /// Display-string key of the dialog's entry.
    pub label_key: &'static str,
    /// Display-string key of the game's rules name (本局规则), which the rules alone decide: the
    /// options sharing the Chinese rules are all 中国规则.
    pub name_key: &'static str,
    /// The rule set whose komi the choice brings, if it has one.
    rule_set: Option<&'static str>,
}

impl MatchRuleOption {
    /// A rule set as a choice: its rules, its entry and its komi.
    const fn of(set: &RuleSet, name_key: &'static str) -> MatchRuleOption {
        MatchRuleOption {
            id: set.id,
            rules: set.rules,
            label_key: set.label_key,
            name_key,
            rule_set: Some(set.id),
        }
    }
}

/// The engine game's standard rules choices; anything else is 自定义. Java offers one Chinese choice
/// (the 2002 rules); here the Chinese rule sets come first, the rest follow in Java's order.
pub const MATCH_RULE_OPTIONS: [MatchRuleOption; 10] = [
    MatchRuleOption::of(&RULE_SETS[0], "LizzieFrame.currentRules.chinese"),
    MatchRuleOption::of(&RULE_SETS[1], "LizzieFrame.currentRules.chinese"),
    MatchRuleOption::of(&RULE_SETS[2], "LizzieFrame.currentRules.chinese2027DrawToFirstPass"),
    MatchRuleOption::of(&RULE_SETS[3], "LizzieFrame.currentRules.chinese"),
    MatchRuleOption {
        id: "CHINESE_ANCIENT",
        rules: "stone-scoring",
        label_key: "LizzieFrame.currentRules.chn-ancient",
        name_key: "LizzieFrame.currentRules.chn-ancient",
        rule_set: Some("CHINESE_ANCIENT"),
    },
    MatchRuleOption {
        id: "JAPANESE_KOREAN",
        rules: "japanese",
        label_key: "MatchRules.option.japaneseKorean",
        name_key: "MatchRules.option.japaneseKorean",
        rule_set: Some("JAPANESE"),
    },
    MatchRuleOption {
        id: "AGA_BGA",
        rules: "aga",
        label_key: "MatchRules.option.agaBga",
        name_key: "MatchRules.option.agaBga",
        rule_set: None,
    },
    MatchRuleOption {
        id: "NEW_ZEALAND",
        rules: "new-zealand",
        label_key: "MatchRules.option.newZealand",
        name_key: "MatchRules.option.newZealand",
        rule_set: None,
    },
    MatchRuleOption {
        id: "TROMP_TAYLOR",
        rules: "tromp-taylor",
        label_key: "MatchRules.option.trompTaylor",
        name_key: "MatchRules.option.trompTaylor",
        rule_set: Some("TROMP_TAYLOR"),
    },
    MatchRuleOption {
        id: "GOE",
        rules: "goe",
        label_key: "LizzieFrame.currentRules.goe",
        name_key: "LizzieFrame.currentRules.goe",
        rule_set: Some("GOE"),
    },
];

/// Each choice's parsed rules, in `MATCH_RULE_OPTIONS` order.
static MATCH_RULE_OPTION_RULES: LazyLock<Vec<KataRules>> = LazyLock::new(|| {
    MATCH_RULE_OPTIONS
        .iter()
        .map(|o| KataRules::parse(o.rules).expect("match rule option rules parse"))
        .collect()
});

impl MatchRuleOption {
    /// The choice these rules are (`MatchRuleOption.matching`). Choices with the same rules, the
    /// Chinese ones, are told apart by the one `chosen` last; without it they are the 2002 rules,
    /// Java's one Chinese choice, listed last of them.
    pub fn matching(rules: &KataRules, chosen: Option<&str>) -> Option<&'static MatchRuleOption> {
        let mut found = None;
        for (option, own) in MATCH_RULE_OPTIONS.iter().zip(MATCH_RULE_OPTION_RULES.iter()) {
            if rules.same_as(own) {
                if chosen == Some(option.id) {
                    return Some(option);
                }
                found = Some(option);
            }
        }
        found
    }

    /// The komi of its rule set, which choosing it fills in; none for AGA/BGA and New Zealand.
    pub fn komi(&self) -> Option<f64> {
        RuleSet::by_id(self.rule_set?).map(|s| s.komi)
    }
}

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
    /// The rule set Java names `id` (`RulePreset.valueOf`).
    pub fn by_id(id: &str) -> Option<&'static RuleSet> {
        RULE_SETS.iter().find(|s| s.id == id)
    }

    /// The rule set picked last (`RulePreset.chosen`): it counts only while KataGo starts with
    /// exactly its rules (启动时自动应用 on); once the startup rules are others, it is not chosen.
    pub fn chosen(settings: &EngineSettings) -> Option<&'static RuleSet> {
        let set = RuleSet::by_id(settings.rule_set.as_deref()?)?;
        (settings.auto_load_kata_rules && KataRules::parse(&settings.kata_rules).as_ref() == Some(set.rules())).then_some(set)
    }

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
        RuleSet::even_game_komi_of(rules.map(|r| Recognition::of(r, komi, chosen)), komi, handicap_game, chosen)
    }

    /// [`Self::even_game_komi`] from the rules' [`Recognition`] with the same `komi` and `chosen`.
    pub fn even_game_komi_of(rules: Option<Recognition>, komi: f64, handicap_game: bool, chosen: Option<&RuleSet>) -> f64 {
        match rules.and_then(|r| r.set) {
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

/// What some rules are with a komi: their summary and the rule set they are ([`RuleSet::of`]).
/// Worked out once, it gives both the rules caption and the even-game komi.
#[derive(Clone, Copy, Debug)]
pub struct Recognition {
    summary: &'static str,
    set: Option<&'static RuleSet>,
}

impl Recognition {
    pub fn of(rules: &KataRules, komi: f64, chosen: Option<&RuleSet>) -> Recognition {
        let summary = rules.summary_key();
        Recognition { summary, set: RuleSet::recognise(rules, summary, komi, chosen) }
    }

    /// [`KataRules::caption_key`].
    pub fn caption_key(&self) -> &'static str {
        self.set.map_or(self.summary, RuleSet::caption_key)
    }
}

/// [`RuleSet::chosen`] kept with the settings it was worked out from, so that settings that did
/// not change are not parsed again; it compares the three settings the answer depends on.
#[derive(Debug, Default)]
pub struct ChosenRuleSet {
    rule_set: Option<String>,
    auto_load: bool,
    kata_rules: String,
    chosen: Option<&'static RuleSet>,
}

impl ChosenRuleSet {
    fn holds(&self, settings: &EngineSettings) -> bool {
        self.auto_load == settings.auto_load_kata_rules
            && self.rule_set == settings.rule_set
            && self.kata_rules == settings.kata_rules
    }

    /// Works the answer out again when `settings` are no longer the ones it was worked out from.
    pub fn refresh(&mut self, settings: &EngineSettings) {
        if !self.holds(settings) {
            *self = ChosenRuleSet {
                rule_set: settings.rule_set.clone(),
                auto_load: settings.auto_load_kata_rules,
                kata_rules: settings.kata_rules.clone(),
                chosen: RuleSet::chosen(settings),
            };
        }
    }

    /// [`RuleSet::chosen`] for `settings`: the kept answer while they are the ones it was worked
    /// out from, else worked out now.
    pub fn get(&self, settings: &EngineSettings) -> Option<&'static RuleSet> {
        if self.holds(settings) {
            self.chosen
        } else {
            RuleSet::chosen(settings)
        }
    }
}

impl KataRules {
    pub fn preset(name: &str) -> Option<KataRules> {
        KataRules::preset_ref(name).cloned()
    }

    /// The rules of official preset `name`, from the preset table.
    fn preset_ref(name: &str) -> Option<&'static KataRules> {
        PRESETS.iter().position(|p| p.0 == name).map(|at| &PRESET_RULES[at])
    }

    fn of_preset(&(_, ko, scoring, tax, suicide, button, bonus, friendly): &Preset) -> KataRules {
        let mut m = Map::new();
        m.insert("ko".into(), ko.into());
        m.insert("scoring".into(), scoring.into());
        m.insert("tax".into(), tax.into());
        m.insert("suicide".into(), suicide.into());
        m.insert("hasButton".into(), button.into());
        m.insert("whiteHandicapBonus".into(), bonus.into());
        m.insert("friendlyPassOk".into(), friendly.into());
        KataRules(m)
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

    /// Whether these rules are preset `name` (see `same_as`).
    pub fn matches_preset(&self, name: &str) -> bool {
        KataRules::preset_ref(name).is_some_and(|preset| self.same_as(preset))
    }

    /// Whether these rules are `other`. Territory scoring does not use `friendlyPassOk` (the rules
    /// editor disables it there), so territory rules match whatever that field says
    /// (`matchesOfficialApartFromFriendlyPass`); other rules match field for field.
    fn same_as(&self, other: &KataRules) -> bool {
        if other.0.get("scoring").and_then(Value::as_str) != Some("TERRITORY") {
            return other == self;
        }
        let others = |m: &Map<String, Value>| m.iter().filter(|(k, _)| *k != "friendlyPassOk").count();
        others(&self.0) == others(&other.0)
            && other
                .0
                .iter()
                .filter(|(k, _)| *k != "friendlyPassOk")
                .all(|(k, v)| self.0.get(k) == Some(v))
    }

    /// Display-string key of these rules as the game's rules (`MatchRuleOption.displayName`), else
    /// 自定义.
    pub fn match_name_key(&self) -> &'static str {
        MatchRuleOption::matching(self, None).map_or("NewEngineGameDialog.matchRules.custom", |o| o.name_key)
    }

    /// KataGo's button: a drawn count goes to the first player to pass (`FirstPassDraw.applies`).
    pub fn has_button(&self) -> bool {
        self.0.get("hasButton").and_then(Value::as_bool).unwrap_or(false)
    }

    /// The official preset this value equals, if any.
    pub fn official_name(&self) -> Option<&'static str> {
        PRESETS.iter().zip(PRESET_RULES.iter()).find(|(_, rules)| self.same_as(rules)).map(|(p, _)| p.0)
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
        Recognition::of(self, komi, chosen).caption_key()
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
