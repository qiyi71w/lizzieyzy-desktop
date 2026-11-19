//! Behavioral cases ported from the fixed Java FunctionSearchTest, through the public index seam.
use std::collections::HashSet;
use lizzie_core::function_search::{self, FunctionBundle, FunctionEntry, FunctionIndex, FunctionMatch,
    FunctionTarget, MatchLevel, SettingsSection};
use lizzie_core::settings::Language;

fn entry(id: &str) -> FunctionEntry {
    FunctionEntry { id: id.into(), title: format!("{id}.title"), description: format!("{id}.description"),
        category: "category".into(), target: FunctionTarget::Window, path: vec!["path".into()],
        shortcut: String::new(), aliases: format!("{id}.strong"), weak_aliases: format!("{id}.weak"), section: None }
}
fn bundle(locale: &str, text: &[(&str, &str)]) -> FunctionBundle {
    FunctionBundle { locale: locale.into(), strings: text.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
}
fn ranked(matches: Vec<FunctionMatch>) -> Vec<(String, MatchLevel)> {
    matches.into_iter().map(|m| (m.id, m.level)).collect()
}
fn one(level: MatchLevel) -> Vec<(String, MatchLevel)> { vec![("weights.download".into(), level)] }
fn index(text: &[(&str, &str)]) -> FunctionIndex {
    FunctionIndex::new(vec![entry("weights.download")], vec![bundle("en_US", text)])
}

/// The fixed Java program's DisplayStrings bundles with this product's own resource keys, as the
/// UI carries them.
fn java_strings() -> serde_json::Value {
    let mut strings = serde_json::from_str::<serde_json::Value>(include_str!("../../../ui/src/i18n/javaStrings.json")).unwrap()["strings"].take();
    let product = serde_json::from_str::<serde_json::Value>(include_str!("../../../ui/src/i18n/productStrings.json")).unwrap();
    for (language, table) in product["strings"].as_object().unwrap() {
        for (key, text) in table.as_object().unwrap() { strings[language][key] = text.clone(); }
    }
    strings
}

#[test]
fn browse_preserves_java_id_order_and_unique_words_results() {
    assert_eq!(function_search::categories().len(), 6);
    let browse = function_search::search("", Language::EnUs);
    // Java's 360 functions less the three approved exceptions the product does not offer and the
    // 详细工具栏's eleven, with Java's two interface-style switches replaced by the three 外观模式
    // choices.
    assert_eq!(browse.len(), 347);
    let ids: Vec<&str> = browse.iter().map(|m| m.id.as_str()).collect();
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "in id order, each once");
    assert!(browse.iter().all(|m| function_search::categories().contains(&m.category.as_str())));
    for id in ["share.private-history", "share.public-history", "menu.frameLooks"] {
        assert!(!ids.contains(&id), "{id}");
        assert!(function_search::search(id, Language::EnUs).iter().all(|m| m.id != id), "{id}");
    }
    assert!(browse.iter().all(|m| m.level == MatchLevel::Words));
    for whitespace in ["\u{2003}", "\u{00a0}\u{0085}\u{202f}\u{3000}\t\r\n", "\u{001c}\u{001f}"] {
        assert_eq!(function_search::search(whitespace, Language::EnUs), browse);
    }
}

/// Destinations as the fixed Java `FunctionCatalog` / `ConfigSettingTargets` declare them.
#[test]
fn representative_functions_lead_to_their_java_destinations() {
    use FunctionTarget::*;
    fn comprehensive(page: &'static str, title: &'static str) -> Vec<&'static str> {
        vec!["Menu.settings", "Menu.comprehensiveSettings", page, title]
    }
    let cases: [(&str, FunctionTarget, Option<SettingsSection>, Vec<&str>); 8] = [
        ("weights.download", Window, None,
            vec!["Menu.settings", "Menu.autoSetup", "AutoSetup.navWeights", "AutoSetup.officialWeightTab"]),
        ("game.komi", Setting, None, vec!["Menu.edit", "Menu.setInfo", "GameInfoDialog.komi"]),
        ("sync.board", Window, None, vec!["Menu.sync", "Menu.readBoard"]),
        ("menu.webBoardToggle", Action, None, vec!["Menu.sync", "Menu.webBoard", "Menu.webBoardStart"]),
        ("files.recent", Navigation, None, vec!["Menu.fileMenu", "Menu.openRecent"]),
        ("config.kifu.read-komi", Setting, Some(SettingsSection::Kifu),
            comprehensive("ConfigDialog2.modern.nav.kifu", "ConfigDialog2.modern.kifu.readKomi")),
        ("config.engine.always-gtp", Setting, Some(SettingsSection::Advanced),
            comprehensive("ConfigDialog2.modern.nav.advanced", "ConfigDialog2.modern.engineHealth.alwaysGtp")),
        ("config.theme.shadow", Setting, Some(SettingsSection::Theme),
            comprehensive("ConfigDialog2.modern.nav.theme", "ConfigDialog2.modern.theme.shadow")),
    ];
    let browse = function_search::search("", Language::EnUs);
    for (id, target, section, path) in cases {
        let found = browse.iter().find(|m| m.id == id).unwrap_or_else(|| panic!("{id}"));
        assert_eq!((found.target, found.section, found.path.iter().map(String::as_str).collect::<Vec<_>>()), (target, section, path), "{id}");
    }
}

#[test]
fn curated_pinyin_and_distinct_cross_language_intents() {
    for (query, id) in [
        ("quanzhong", "weights.download"), ("qz", "weights.download"), ("lianpan", "sync.board"),
        ("全盘深度分析", "whole-game-deep-analysis"), ("批量分析", "batch-analysis"),
        ("同步棋盘", "sync.board"), ("配置同步", "sync.settings"), ("远程算力", "engine.remote"),
        ("引擎选择", "engine.select"), ("最近棋谱", "files.recent"), ("快速启动", "tools.quick-start"),
        ("终止人机对局", "game.stop-human"), ("暂停分析", "analysis.toggle"),
    ] {
        assert_eq!(function_search::search(query, Language::EnUs)[0].id, id, "{query}");
    }
    assert!(function_search::search("GTP", Language::JaJp).iter().any(|m| m.id == "config.engine.always-gtp"));
}

#[test]
fn every_java_title_finds_its_function() {
    // Java FunctionSearchTest.everyLocalizedDestinationCanBeRenderedAndFoundByItsRealTitle.
    let strings = java_strings();
    let browse = function_search::search("", Language::EnUs);
    for (language, name) in [
        (Language::EnUs, "EnUs"), (Language::ZhCn, "ZhCn"), (Language::ZhTw, "ZhTw"),
        (Language::JaJp, "JaJp"), (Language::KoKr, "KoKr"), (Language::ThTh, "ThTh"),
    ] {
        for function in &browse {
            let title = strings[name][&function.title].as_str().unwrap_or_else(|| panic!("{name}: {}", function.title));
            assert!(
                function_search::search(title, language).iter().any(|m| m.id == function.id),
                "{name}: {title} must find {}", function.id
            );
        }
    }
}

#[test]
fn nfkc_whitespace_case_and_accents_retain_meaningful_marks() {
    let search = index(&[("weights.download.title", "Ｕｐｄａｔｅ\u{00a0}Ｗｅｉｇｈｔ"),
        ("weights.download.description", "Official café weights")]);
    assert_eq!(ranked(search.search(" update\u{2003}weight ", Language::EnUs)), one(MatchLevel::Exact));
    assert!(search.search("cafe", Language::EnUs).is_empty());
    assert_eq!(ranked(search.search("café", Language::EnUs)), one(MatchLevel::Phrase));
    assert_eq!(ranked(search.search("cafe\u{301}", Language::EnUs)), one(MatchLevel::Phrase));
}

#[test]
fn multilingual_strong_alias_outranks_active_language_weak_alias() {
    let search = FunctionIndex::new(vec![entry("engine.acceleration"), entry("settings.black-winrate")], vec![
        bundle("en_US", &[("engine.acceleration.title", "Turbo setup"),
            ("settings.black-winrate.title", "Black display"), ("settings.black-winrate.weak", "gpu")]),
        bundle("zh_CN", &[("engine.acceleration.title", "显卡配置"),
            ("engine.acceleration.strong", "显卡加速|gpu"), ("settings.black-winrate.title", "黑方显示")]),
        bundle("", &[("engine.acceleration.title", "Turbo setup"), ("settings.black-winrate.title", "Black display")]),
    ]);
    assert_eq!(ranked(search.search("gpu", Language::EnUs)), vec![
        ("engine.acceleration".into(), MatchLevel::Exact), ("settings.black-winrate".into(), MatchLevel::WeakAlias)]);
    assert_eq!(ranked(search.search("显卡加速", Language::EnUs)), vec![("engine.acceleration".into(), MatchLevel::Exact)]);
}

#[test]
fn tokens_use_and_across_fields_and_languages() {
    let search = FunctionIndex::new(vec![entry("weights.download")], vec![
        bundle("en_US", &[("weights.download.title", "Official weights"),
            ("weights.download.description", "Download models"), ("category", "engine"), ("path", "Settings")]),
        bundle("zh_CN", &[("weights.download.title", "官方权重")]),
    ]);
    for query in ["download weights", "weights download", "download 权重"] {
        assert_eq!(ranked(search.search(query, Language::EnUs)), one(MatchLevel::Words));
    }
    assert!(search.search("download missing", Language::EnUs).is_empty());
}

#[test]
fn typo_transposition_insertion_deletion_substitution_and_length_boundaries() {
    let search = index(&[("weights.download.title", "weights")]);
    for query in ["weihgts", "weigths", "weightss", "weighxs"] {
        assert_eq!(ranked(search.search(query, Language::EnUs)), one(MatchLevel::Typo), "{query}");
    }
    for query in ["wet", "wets", "wigt"] { assert!(search.search(query, Language::EnUs).is_empty()); }
    let deletion = index(&[("weights.download.title", "weight")]);
    assert_eq!(ranked(deletion.search("wight", Language::EnUs)), one(MatchLevel::Typo));
    let short = index(&[("weights.download.title", "cat")]);
    assert!(short.search("cot", Language::EnUs).is_empty());
}

#[test]
fn duplicate_ids_collapse_to_the_best_single_match_including_browse() {
    let search = FunctionIndex::new(vec![entry("weights.download"), entry("weights.download")], vec![
        bundle("en_US", &[("weights.download.title", "Download weights")]),
    ]);
    assert_eq!(ranked(search.search("download weights", Language::EnUs)), one(MatchLevel::Exact));
    assert_eq!(ranked(search.search("\u{2003}", Language::EnUs)), one(MatchLevel::Words));
    let mut stronger = entry("weights.download");
    stronger.title = "other.title".into();
    let search = FunctionIndex::new(vec![entry("weights.download"), stronger], vec![
        bundle("en_US", &[("weights.download.title", "Download weights now"), ("other.title", "weights")]),
    ]);
    assert_eq!(ranked(search.search("weights", Language::EnUs)), one(MatchLevel::Exact));
}

#[test]
fn title_and_strong_alias_phrase_outrank_description_regardless_of_field_order() {
    let search = FunctionIndex::new(vec![entry("a-description"), entry("z-alias"), entry("y-title")], vec![
        bundle("en_US", &[("a-description.title", "Device configuration"), ("a-description.description", "Configure gpu"),
            ("z-alias.title", "Acceleration"), ("z-alias.description", "Configure gpu"), ("z-alias.strong", "gpu acceleration"),
            ("y-title.title", "gpu setup")]),
    ]);
    assert_eq!(ranked(search.search("gpu", Language::EnUs)), vec![
        ("y-title".into(), MatchLevel::Phrase), ("z-alias".into(), MatchLevel::Phrase),
        ("a-description".into(), MatchLevel::Phrase)]);
}

#[test]
fn active_language_breaks_equal_score_ties_before_id() {
    let search = FunctionIndex::new(vec![entry("a-chinese"), entry("z-english")], vec![
        bundle("en_US", &[("a-chinese.title", "Other"), ("z-english.title", "Device setup")]),
        bundle("zh_CN", &[("a-chinese.title", "Device setup"), ("z-english.title", "Other")]),
    ]);
    assert_eq!(search.search("Device", Language::EnUs)[0].id, "z-english");
    assert_eq!(search.search("Device", Language::ZhCn)[0].id, "a-chinese");
    assert_eq!(search.search("Device", Language::ZhTw)[0].id, "a-chinese");
    assert_eq!(search.search("Device", Language::EnUs).iter().map(|m| &m.id).collect::<HashSet<_>>().len(), 2);
}

#[test]
fn cross_locale_aggregate_distance_uses_the_worst_token_distance() {
    let search = FunctionIndex::new(vec![entry("a-root"), entry("z-active")], vec![
        bundle("en_US", &[("a-root.title", "first")]),
        bundle("zh_TW", &[("z-active.title", "first")]),
        bundle("", &[("a-root.description", "second")]),
        bundle("zh_CN", &[("z-active.description", "second")]),
    ]);
    assert_eq!(search.search("first second", Language::EnUs)[0].id, "a-root");
    assert_eq!(search.search("first second", Language::ZhCn)[0].id, "z-active");
}
