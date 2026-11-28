//! The rule sets (Java `RulePreset`): KataGo rules with the komi that belongs to them, which rule
//! set given rules and a komi are, the rules caption they give and the komi of an even game.

use lizzie_core::engine::rules::{KataRules, MatchRuleOption, Recognition, RuleSet, MATCH_RULE_OPTIONS, RULE_SETS};

/// KataGo's `kata-get-rules` answers.
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
/// Chinese rules with the button (数子规则下收后方贴还 0.5 目: 是).
const CHINESE_BUTTON: &str = r#"{"friendlyPassOk":true,"hasButton":true,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
/// Japanese and Korean rules as KataGo 1.18.1 reports them.
const JAPANESE: &str = r#"{"friendlyPassOk":false,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#;

fn rules(text: &str) -> KataRules {
    KataRules::parse(text).unwrap()
}

fn set(id: &str) -> &'static RuleSet {
    RULE_SETS.iter().find(|s| s.id == id).unwrap()
}

fn of(text: &str, komi: f64, chosen: Option<&str>) -> Option<&'static str> {
    RuleSet::of(&rules(text), komi, chosen.map(set)).map(|s| s.id)
}

/// `RulePreset.values()`: order, rules and komi.
#[test]
fn the_eight_rule_sets_have_their_rules_and_komi() {
    let table: Vec<(&str, f64)> = RULE_SETS.iter().map(|s| (s.id, s.komi)).collect();
    assert_eq!(
        table,
        [
            ("CHINESE_2027", 7.0),
            ("CHINESE_2027_DRAW_TO_WHITE", 7.5),
            ("CHINESE_2027_DRAW_TO_FIRST_PASS", 7.0),
            ("CHINESE", 7.5),
            ("JAPANESE", 6.5),
            ("CHINESE_ANCIENT", 7.5),
            ("TROMP_TAYLOR", 7.5),
            ("GOE", 7.5),
        ]
    );
    let chinese = rules(CHINESE);
    assert_eq!(set("CHINESE_2027").rules(), &chinese);
    assert_eq!(set("CHINESE_2027_DRAW_TO_WHITE").rules(), &chinese);
    assert_eq!(set("CHINESE").rules(), &chinese);
    assert_eq!(set("CHINESE_2027_DRAW_TO_FIRST_PASS").rules(), &rules(CHINESE_BUTTON));
    assert_eq!(set("JAPANESE").rules(), &rules("japanese"));
    assert_eq!(set("CHINESE_ANCIENT").rules(), &rules("stone-scoring"));
    assert_eq!(set("TROMP_TAYLOR").rules(), &rules("tromp-taylor"));
    assert_eq!(set("GOE").rules(), &rules("goe"));
    let labels: Vec<&str> = RULE_SETS.iter().map(|s| s.label_key).collect();
    assert_eq!(
        labels,
        [
            "RulesQuickMenu.chinese2027",
            "RulesQuickMenu.chinese2027DrawToWhite",
            "RulesQuickMenu.chinese2027DrawToFirstPass",
            "RulesQuickMenu.chinese",
            "RulesQuickMenu.japanese",
            "RulesQuickMenu.chnAncient",
            "RulesQuickMenu.trompTaylor",
            "RulesQuickMenu.goe",
        ]
    );
}

/// Chinese rules are told apart by komi; 平局时白方胜 counts exactly like 中国规则 2002 and is
/// that only when it was picked last; a komi of no Chinese rule set is no rule set.
#[test]
fn chinese_rule_sets_are_told_apart_by_komi() {
    assert_eq!(of(CHINESE, 7.0, None), Some("CHINESE_2027"));
    assert_eq!(of(CHINESE, 7.5, None), Some("CHINESE"));
    assert_eq!(
        of(CHINESE, 7.5, Some("CHINESE_2027_DRAW_TO_WHITE")),
        Some("CHINESE_2027_DRAW_TO_WHITE")
    );
    // A chosen rule set the rules and komi are not does not count.
    assert_eq!(
        of(CHINESE, 7.0, Some("CHINESE_2027_DRAW_TO_WHITE")),
        Some("CHINESE_2027")
    );
    assert_eq!(of(CHINESE, 7.5, Some("JAPANESE")), Some("CHINESE"));
    assert_eq!(of(CHINESE, 6.5, None), None);
    // Chinese rules as OGS states them have the Chinese summary.
    assert_eq!(of("chinese-ogs", 7.0, None), Some("CHINESE_2027"));
}

/// A rule set whose rules no other has is recognised whatever the komi.
#[test]
fn rule_sets_of_their_own_rules_ignore_komi() {
    for komi in [7.0, 7.5, 6.5, 0.5, 0.0] {
        assert_eq!(
            of(CHINESE_BUTTON, komi, None),
            Some("CHINESE_2027_DRAW_TO_FIRST_PASS"),
            "{komi}"
        );
        assert_eq!(of(JAPANESE, komi, None), Some("JAPANESE"), "{komi}");
        assert_eq!(of("korean", komi, None), Some("JAPANESE"), "{komi}");
        assert_eq!(of("stone-scoring", komi, None), Some("CHINESE_ANCIENT"), "{komi}");
        assert_eq!(of("tromp-taylor", komi, None), Some("TROMP_TAYLOR"), "{komi}");
        assert_eq!(of("goe", komi, None), Some("GOE"), "{komi}");
        assert_eq!(of("aga", komi, None), None, "{komi}");
        assert_eq!(of("new-zealand", komi, None), None, "{komi}");
    }
}

/// `RulePreset.captionKey`: the rule set's short name for the 2027 rules, else the rules' summary.
#[test]
fn the_caption_names_the_rule_set_else_the_rules() {
    let caption = |text: &str, komi: f64, chosen: Option<&str>| Recognition::of(&rules(text), komi, chosen.map(set)).caption_key();
    assert_eq!(caption(CHINESE, 7.0, None), "LizzieFrame.currentRules.chinese2027");
    assert_eq!(caption(CHINESE, 7.5, None), "LizzieFrame.currentRules.chinese");
    assert_eq!(
        caption(CHINESE, 7.5, Some("CHINESE_2027_DRAW_TO_WHITE")),
        "LizzieFrame.currentRules.chinese2027DrawToWhite"
    );
    assert_eq!(
        caption(CHINESE_BUTTON, 7.5, None),
        "LizzieFrame.currentRules.chinese2027DrawToFirstPass"
    );
    assert_eq!(caption(CHINESE, 6.5, None), "LizzieFrame.currentRules.chinese");
    assert_eq!(caption(JAPANESE, 7.5, None), "LizzieFrame.currentRules.japanese");
    assert_eq!(
        caption("stone-scoring", 7.0, None),
        "LizzieFrame.currentRules.chn-ancient"
    );
    assert_eq!(
        caption("tromp-taylor", 7.0, None),
        "LizzieFrame.currentRules.tromp-taylor"
    );
    assert_eq!(caption("goe", 7.0, None), "LizzieFrame.currentRules.goe");
    assert_eq!(caption("aga", 7.5, None), "LizzieFrame.currentRules.others");
}

/// `RulePreset.evenGameKomi`: the komi of the rule set the rules and komi are; else, after a
/// handicap game, the komi new games start with (the chosen rule set's, or 7.5); else the
/// current game's.
#[test]
fn the_even_game_komi_follows_the_rule_set() {
    let even = |text: Option<&str>, komi: f64, handicap: bool, chosen: Option<&str>| {
        let chosen = chosen.map(set);
        RuleSet::even_game_komi(text.map(|t| Recognition::of(&rules(t), komi, chosen)), komi, handicap, chosen)
    };
    assert_eq!(even(Some(CHINESE), 7.0, false, None), 7.0);
    assert_eq!(even(Some(JAPANESE), 7.5, false, None), 6.5);
    assert_eq!(even(Some(CHINESE_BUTTON), 7.5, false, None), 7.0);
    assert_eq!(even(Some(CHINESE), 7.5, false, None), 7.5);
    // The rule set the rules are wins over the komi new games start with.
    assert_eq!(even(Some(JAPANESE), 0.5, true, None), 6.5);
    assert_eq!(even(Some(CHINESE_BUTTON), 0.5, true, Some("CHINESE")), 7.0);
    // No rule set after a handicap game: the komi new games start with.
    assert_eq!(even(Some("aga"), 0.5, true, None), 7.5);
    assert_eq!(even(Some(CHINESE), 0.5, true, None), 7.5);
    assert_eq!(even(Some("aga"), 0.5, true, Some("CHINESE_2027")), 7.0);
    assert_eq!(even(None, 0.5, true, None), 7.5);
    // No rule set in an even game: the current komi.
    assert_eq!(even(Some("aga"), 6.0, false, None), 6.0);
    assert_eq!(even(Some(CHINESE), 6.5, false, None), 6.5);
    assert_eq!(even(None, 6.5, false, Some("CHINESE_2027")), 6.5);
}

/// The engine game's 本局规则 entries: the four Chinese rule sets first, worded as the rules panel
/// lists them, then Java's other `MatchRuleOption`s; each fills in its rule set's komi, and the
/// AGA/BGA and New Zealand rules, which no rule set has, fill in none.
#[test]
fn the_match_rules_entries_bring_their_rule_sets_komi() {
    let table: Vec<(&str, KataRules, &str, Option<f64>)> = MATCH_RULE_OPTIONS
        .iter()
        .map(|o| (o.id, rules(o.rules), o.label_key, o.komi()))
        .collect();
    assert_eq!(
        table,
        [
            ("CHINESE_2027", rules(CHINESE), "RulesQuickMenu.chinese2027", Some(7.0)),
            (
                "CHINESE_2027_DRAW_TO_WHITE",
                rules(CHINESE),
                "RulesQuickMenu.chinese2027DrawToWhite",
                Some(7.5)
            ),
            (
                "CHINESE_2027_DRAW_TO_FIRST_PASS",
                rules(CHINESE_BUTTON),
                "RulesQuickMenu.chinese2027DrawToFirstPass",
                Some(7.0)
            ),
            ("CHINESE", rules(CHINESE), "RulesQuickMenu.chinese", Some(7.5)),
            (
                "CHINESE_ANCIENT",
                rules("stone-scoring"),
                "LizzieFrame.currentRules.chn-ancient",
                Some(7.5)
            ),
            (
                "JAPANESE_KOREAN",
                rules("japanese"),
                "MatchRules.option.japaneseKorean",
                Some(6.5)
            ),
            ("AGA_BGA", rules("aga"), "MatchRules.option.agaBga", None),
            (
                "NEW_ZEALAND",
                rules("new-zealand"),
                "MatchRules.option.newZealand",
                None
            ),
            (
                "TROMP_TAYLOR",
                rules("tromp-taylor"),
                "MatchRules.option.trompTaylor",
                Some(7.5)
            ),
            ("GOE", rules("goe"), "LizzieFrame.currentRules.goe", Some(7.5)),
        ]
    );
    // Every entry but the button rules goes to the engines as KataGo's preset name, as Java's do.
    assert!(MATCH_RULE_OPTIONS
        .iter()
        .filter(|o| o.id != "CHINESE_2027_DRAW_TO_FIRST_PASS")
        .all(|o| !o.rules.starts_with('{')));
}

/// The entry given rules are: the Chinese entries share their rules and are told apart by the one
/// picked last, else they are the 2002 rules; Japanese rules match whatever `friendlyPassOk` says.
#[test]
fn match_rules_entries_are_recognised_by_rules_and_the_entry_picked_last() {
    let entry = |text: &str, chosen: Option<&str>| MatchRuleOption::matching(&rules(text), chosen).map(|o| o.id);
    assert_eq!(entry(CHINESE, None), Some("CHINESE"));
    assert_eq!(entry(CHINESE, Some("CHINESE_2027")), Some("CHINESE_2027"));
    assert_eq!(
        entry(CHINESE, Some("CHINESE_2027_DRAW_TO_WHITE")),
        Some("CHINESE_2027_DRAW_TO_WHITE")
    );
    assert_eq!(entry(CHINESE, Some("CHINESE_2027_DRAW_TO_FIRST_PASS")), Some("CHINESE"));
    assert_eq!(entry(CHINESE, Some("unknown")), Some("CHINESE"));
    assert_eq!(entry(CHINESE_BUTTON, None), Some("CHINESE_2027_DRAW_TO_FIRST_PASS"));
    assert_eq!(
        entry(CHINESE_BUTTON, Some("CHINESE_2027")),
        Some("CHINESE_2027_DRAW_TO_FIRST_PASS")
    );
    assert_eq!(entry(JAPANESE, Some("CHINESE_2027")), Some("JAPANESE_KOREAN"));
    assert_eq!(entry("korean", None), Some("JAPANESE_KOREAN"));
    assert_eq!(entry("aga-button", None), None);
    assert_eq!(rules(CHINESE).match_name_key(), "LizzieFrame.currentRules.chinese");
    assert_eq!(
        rules(CHINESE_BUTTON).match_name_key(),
        "LizzieFrame.currentRules.chinese2027DrawToFirstPass"
    );
    assert_eq!(
        rules(CHINESE_BUTTON).match_name_key(),
        set("CHINESE_2027_DRAW_TO_FIRST_PASS").caption_key()
    );
    assert_eq!(
        rules("aga-button").match_name_key(),
        "NewEngineGameDialog.matchRules.custom"
    );
}
