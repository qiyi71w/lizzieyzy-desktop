//! Offline function search over the fixed Java catalog and its eight maintained bundles.
//! Resource keys are returned unchanged; presentation and activation belong to the caller.
use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use unicode_general_category::{get_general_category, GeneralCategory};
use unicode_normalization::UnicodeNormalization;

use crate::settings::Language;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum MatchLevel { Exact, Phrase, Words, WeakAlias, Typo }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FunctionTarget { Window, Setting, Action, Navigation, Context }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum SettingsSection { Display, Kifu, Engine, Play, Advanced, Theme }

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct FunctionMatch {
    pub id: String,
    pub level: MatchLevel,
    pub title: String,
    pub description: String,
    pub category: String,
    pub target: FunctionTarget,
    pub path: Vec<String>,
    pub shortcut: String,
    pub section: Option<SettingsSection>,
}

/// Catalog metadata accepted by the same index used by production search.
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionEntry {
    pub id: String,
    pub title: String,
    pub description: String,
    pub category: String,
    pub target: FunctionTarget,
    pub path: Vec<String>,
    pub shortcut: String,
    pub aliases: String,
    pub weak_aliases: String,
    pub section: Option<SettingsSection>,
}

/// Text resolved with Java ResourceBundle parent lookup, tagged with its actual bundle locale.
#[derive(Clone, Debug, Deserialize)]
pub struct FunctionBundle {
    pub locale: String,
    pub strings: HashMap<String, String>,
}

#[derive(Deserialize)]
struct Catalog {
    categories: Vec<String>,
    entries: Vec<FunctionEntry>,
    bundles: Vec<FunctionBundle>,
}

struct Production { categories: Vec<String>, index: FunctionIndex }
static PRODUCTION: LazyLock<Production> = LazyLock::new(|| {
    let catalog: Catalog = serde_json::from_str(include_str!("catalog.json"))
        .expect("committed Java function catalog");
    Production { categories: catalog.categories, index: FunctionIndex::new(catalog.entries, catalog.bundles) }
});
static CATEGORIES: LazyLock<Vec<&'static str>> = LazyLock::new(|| {
    PRODUCTION.categories.iter().map(String::as_str).collect()
});

pub fn categories() -> &'static [&'static str] { &CATEGORIES }

/// Searches without a result cap. `language` is the resolved active UI language.
pub fn search(query: &str, language: Language) -> Vec<FunctionMatch> {
    PRODUCTION.index.search(query, language)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind { Title, Description, Category, Path, Shortcut, StrongAlias, WeakAlias }
impl Kind {
    fn quality(self) -> usize {
        match self {
            Self::Title | Self::StrongAlias => 4,
            Self::Description => 3,
            Self::Path => 2,
            _ => 1,
        }
    }
}
struct Field { kind: Kind, text: String, latin_words: Vec<String> }
impl Field {
    fn new(kind: Kind, text: &str) -> Self {
        let text = normalize(text);
        // Java Character.isLetter(char) splits supplementary characters at their surrogates.
        let latin_words = text.split(|c: char| c as u32 > 0xffff || !matches!(get_general_category(c),
            GeneralCategory::UppercaseLetter | GeneralCategory::LowercaseLetter |
            GeneralCategory::TitlecaseLetter | GeneralCategory::ModifierLetter | GeneralCategory::OtherLetter))
            .filter(|word| ascii_word(word)).map(str::to_owned).collect();
        Self { kind, text, latin_words }
    }
    fn matches(&self, word: &str, typo: bool) -> bool {
        self.text.contains(word) || (typo && self.latin_words.iter().any(|w| one_edit_away(word, w)))
    }
}
struct Localization { locale: String, fields: Range<usize> }
struct IndexedEntry {
    entry: FunctionEntry,
    fields: Vec<Field>,
    localizations: Vec<Localization>,
}

/// Immutable reusable index. Construction resolves and normalizes resources once, never per query.
pub struct FunctionIndex { entries: Vec<IndexedEntry> }
impl FunctionIndex {
    pub fn new(entries: Vec<FunctionEntry>, bundles: Vec<FunctionBundle>) -> Self {
        let entries = entries.into_iter().map(|entry| {
            let mut fields = Vec::new();
            let mut localizations = Vec::with_capacity(bundles.len());
            for bundle in &bundles {
                let start = fields.len();
                let resource = |key: &str| bundle.strings.get(key).map(String::as_str);
                let title = resource(&entry.title).unwrap_or(&entry.id);
                fields.push(Field::new(Kind::Title, title));
                fields.push(Field::new(Kind::Description, resource(&entry.description).unwrap_or("")));
                fields.push(Field::new(Kind::Category, resource(&entry.category).unwrap_or("")));
                for path in &entry.path { fields.push(Field::new(Kind::Path, resource(path).unwrap_or(""))); }
                if !entry.shortcut.trim().is_empty() { fields.push(Field::new(Kind::Shortcut, &entry.shortcut)); }
                let mut strong = aliases(resource(&entry.aliases).unwrap_or(""));
                if strong.is_empty() && !title.trim().is_empty() { strong.push(title.to_owned()); }
                for alias in strong { fields.push(Field::new(Kind::StrongAlias, &alias)); }
                for alias in aliases(resource(&entry.weak_aliases).unwrap_or("")) {
                    fields.push(Field::new(Kind::WeakAlias, &alias));
                }
                localizations.push(Localization { locale: bundle.locale.clone(), fields: start..fields.len() });
            }
            IndexedEntry { entry, fields, localizations }
        }).collect();
        Self { entries }
    }

    pub fn search(&self, query: &str, language: Language) -> Vec<FunctionMatch> {
        self.search_locale(query, language_locale(language))
    }

    /// Locale seam also covers ROOT and zh_HK, which are indexed but not UI language choices.
    pub fn search_locale(&self, query: &str, locale: &str) -> Vec<FunctionMatch> {
        let normalized = normalize(query);
        let words: Vec<&str> = normalized.split(' ').filter(|s| !s.is_empty()).collect();
        let mut best: HashMap<&str, (usize, Score)> = HashMap::with_capacity(self.entries.len());
        for (index, entry) in self.entries.iter().enumerate() {
            if words.is_empty() {
                // Every localization has the same browse score; only the stable ID orders results.
                best.entry(&entry.entry.id).or_insert((index,
                    Score { level: MatchLevel::Words, coverage: 0, quality: 0, distance: 0 }));
                continue;
            }
            let mut retain = |score: Score| {
                let previous = best.entry(&entry.entry.id).or_insert((index, score));
                if score.cmp(&previous.1).is_lt() { *previous = (index, score); }
            };
            for localized in &entry.localizations {
                if let Some(score) = score_fields(&entry.fields[localized.fields.clone()], &normalized,
                    &words, locale_distance(&localized.locale, locale), false) { retain(score); }
            }
            if entry.localizations.len() >= 2 {
                if let Some(mut score) = score_fields(&entry.fields, &normalized, &words, 0, true) {
                    score.distance = words.iter().map(|word| {
                        entry.localizations.iter().filter(|localized| {
                            selected(&entry.fields[localized.fields.clone()], score.level == MatchLevel::WeakAlias)
                                .any(|field| field.matches(word, score.level == MatchLevel::Typo))
                        }).map(|localized| locale_distance(&localized.locale, locale)).min().unwrap_or(0)
                    }).max().unwrap_or(0);
                    retain(score);
                }
            }
        }
        let mut matches: Vec<_> = best.into_values().collect();
        matches.sort_by(|(left, a), (right, b)| a.cmp(b).then_with(|| {
            // Java String.compareTo orders UTF-16 code units, not UTF-8 bytes.
            self.entries[*left].entry.id.encode_utf16().cmp(self.entries[*right].entry.id.encode_utf16())
        }));
        matches.into_iter().map(|(index, score)| {
            let entry = &self.entries[index].entry;
            FunctionMatch { id: entry.id.clone(), level: score.level, title: entry.title.clone(),
                description: entry.description.clone(), category: entry.category.clone(), target: entry.target,
                path: entry.path.clone(), shortcut: entry.shortcut.clone(), section: entry.section }
        }).collect()
    }
}

#[derive(Clone, Copy)]
struct Score { level: MatchLevel, coverage: usize, quality: usize, distance: usize }
impl Score {
    fn cmp(&self, other: &Self) -> Ordering {
        self.level.cmp(&other.level).then_with(|| other.coverage.cmp(&self.coverage))
            .then_with(|| other.quality.cmp(&self.quality)).then_with(|| self.distance.cmp(&other.distance))
    }
}
fn selected(fields: &[Field], weak: bool) -> impl Iterator<Item = &Field> {
    fields.iter().filter(move |field| weak || field.kind != Kind::WeakAlias)
}
fn score_fields(fields: &[Field], query: &str, words: &[&str], distance: usize, combined: bool) -> Option<Score> {
    let exact = !combined && selected(fields, false).any(|f|
        matches!(f.kind, Kind::Title | Kind::StrongAlias) && f.text == query);
    let phrase_quality = if combined { 0 } else {
        selected(fields, false).filter(|f| !f.text.is_empty() && f.text.contains(query))
            .map(|f| f.kind.quality()).max().unwrap_or(0)
    };
    let all = |weak, typo| words.iter().all(|word| selected(fields, weak).any(|f| f.matches(word, typo)));
    let level = if exact { MatchLevel::Exact }
        else if phrase_quality > 0 { MatchLevel::Phrase }
        else if all(false, false) { MatchLevel::Words }
        else if all(true, false) { MatchLevel::WeakAlias }
        else if all_with_java_typo_order(fields, words) { MatchLevel::Typo }
        else { return None };
    let weak = level == MatchLevel::WeakAlias;
    let typo = level == MatchLevel::Typo;
    let quality = if exact { 4 } else if level == MatchLevel::Phrase { phrase_quality } else {
        fields.iter().filter(|f| if weak { f.kind == Kind::WeakAlias } else { f.kind != Kind::WeakAlias })
            .filter(|f| words.iter().any(|word| f.matches(word, typo)))
            .map(|f| f.kind.quality()).max().unwrap_or(0)
    };
    let coverage = words.iter().map(|word| {
        selected(fields, weak).map(|f| {
            if f.text.contains(word) { word.chars().count() }
            else if typo && f.latin_words.iter().any(|w| one_edit_away(word, w)) { word.len().saturating_sub(1) }
            else { 0 }
        }).max().unwrap_or(0)
    }).sum();
    Some(Score { level, coverage, quality, distance })
}

// Preserve Java's field-order short circuit: a typo in an earlier field can precede a later exact hit.
fn all_with_java_typo_order(fields: &[Field], words: &[&str]) -> bool {
    let mut used = false;
    for word in words {
        let mut found = false;
        for field in selected(fields, false) {
            if field.text.contains(word) { found = true; break; }
            if field.latin_words.iter().any(|w| one_edit_away(word, w)) {
                found = true; used = true; break;
            }
        }
        if !found { return false; }
    }
    used
}

fn aliases(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    for alias in text.split('|').map(normalize).filter(|s| !s.is_empty()) {
        if !result.contains(&alias) { result.push(alias); }
    }
    result
}
fn normalize(text: &str) -> String {
    // str::to_lowercase includes Unicode's context-sensitive final sigma, like Locale.ROOT.
    let lower = text.nfkc().collect::<String>().to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut space = false;
    for c in lower.chars() {
        if c.is_whitespace() || matches!(c, '\u{001c}'..='\u{001f}') { space = !result.is_empty(); }
        else {
            if space { result.push(' '); space = false; }
            result.push(c);
        }
    }
    result
}
fn ascii_word(word: &str) -> bool { !word.is_empty() && word.bytes().all(|b| b.is_ascii_lowercase()) }
fn one_edit_away(query: &str, target: &str) -> bool {
    if !ascii_word(query) || !ascii_word(target) || query.len() < 4
        || query.len().abs_diff(target.len()) > 1 || query == target { return false; }
    let a = query.as_bytes();
    let b = target.as_bytes();
    if a.len() == b.len() {
        let first = a.iter().zip(b).position(|(x, y)| x != y).unwrap();
        let differences = a.iter().zip(b).filter(|(x, y)| x != y).count();
        return differences == 1 || (differences == 2 && first + 1 < a.len()
            && a[first] == b[first + 1] && a[first + 1] == b[first] && a[first + 2..] == b[first + 2..]);
    }
    let (long, short) = if a.len() > b.len() { (a, b) } else { (b, a) };
    let (mut i, mut j, mut skipped) = (0, 0, false);
    while i < long.len() && j < short.len() {
        if long[i] == short[j] { i += 1; j += 1; }
        else if skipped { return false; }
        else { skipped = true; i += 1; }
    }
    true
}
fn locale_distance(candidate: &str, effective: &str) -> usize {
    if candidate == effective { 0 }
    else if !candidate.is_empty() && candidate.split('_').next() == effective.split('_').next() { 1 }
    else if candidate.is_empty() { 2 }
    else { 3 }
}
fn language_locale(language: Language) -> &'static str {
    match language {
        Language::ZhCn => "zh_CN", Language::ZhTw => "zh_TW", Language::EnUs => "en_US",
        Language::JaJp => "ja_JP", Language::KoKr => "ko", Language::ThTh => "th_TH",
        Language::System => "", // Callers pass resolved languages; ROOT is Java's null-locale behavior.
    }
}
