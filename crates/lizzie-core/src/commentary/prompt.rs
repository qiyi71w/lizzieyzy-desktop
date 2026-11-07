//! Exact literals from TeacherPromptBuilder, analysis/TeacherPersona and runtime bundles.
#[path = "prompt_literals.rs"]
mod literals;
use super::{
    evidence::{Position, Range},
    settings::Preferences,
    Message,
};
use crate::settings::Language;
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, ts_rs::TS)]
pub enum Mode {
    Next,
    Range,
    Whole,
}
use literals::{option, text};
pub fn locale(l: Language) -> &'static str {
    match l {
        Language::EnUs => "en_US",
        Language::ZhTw => "zh_TW",
        Language::JaJp => "ja_JP",
        Language::KoKr => "ko",
        Language::ThTh => "th_TH",
        _ => "zh_CN",
    }
}
pub fn localized(l: Language, key: &str) -> &'static str {
    literals::localized(locale(l), key)
}
fn label(l: Language, key: &str, v: &str) -> String {
    localized(l, key).replace("''", "'").replace("{0}", v)
}
fn integer(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
fn persona(p: &Preferences, chinese: bool) -> String {
    let p = p.clone().normalized();
    let dan = p.rank_mode == "d";
    if chinese {
        let level = if dan {
            3
        } else if p.rank_num >= 10 {
            0
        } else if p.rank_num >= 5 {
            1
        } else {
            2
        };
        let mut parts = vec![
            "【风格设置】",
            option("zhLevel", level),
            option("zhRank", if dan { p.rank_num as usize } else { 0 }),
            text("zhAge"),
            option("zhStyle", p.style_index as usize),
            option("zhDensity", p.density_index as usize),
            option("zhPace", p.pace_index as usize),
            option("zhVariation", p.variation_index as usize),
        ];
        parts.extend([option("zhPriority", 0), option("zhPriority", 1)]);
        parts.join("\n")
    } else {
        let rank = if dan {
            if p.rank_num >= 4 {
                "strong dan player"
            } else {
                "advanced amateur (dan level)"
            }
        } else if p.rank_num >= 10 {
            "beginner"
        } else if p.rank_num >= 5 {
            "intermediate amateur"
        } else {
            "advanced amateur (single-digit kyu)"
        };
        format!("The person is a {rank} (rank {}{}). Adjust the depth of the explanation to that level; do not state the rank in the answer. {}{}{}{}",p.rank_mode,p.rank_num,option("enStyle",p.style_index as usize),option("enDensity",p.density_index as usize),option("enPace",p.pace_index as usize),option("enVariation",p.variation_index as usize))
    }
}
pub fn system_prompt(language: Language, preferences: &Preferences) -> String {
    if matches!(language, Language::System | Language::ZhCn | Language::ZhTw) {
        format!(
            "{}{}\n{}{}{}，不要混用其他语言。\n{}",
            text("zhStart"),
            persona(preferences, true),
            text("zhRules"),
            text("zhLanguageStart"),
            if language == Language::ZhTw {
                "繁體中文"
            } else {
                "简体中文"
            },
            text("zhEnd")
        )
    } else {
        let lang = match language {
            Language::JaJp => "Japanese",
            Language::KoKr => "Korean",
            Language::ThTh => "Thai",
            _ => "English",
        };
        format!(
            "{}{lang}{}{}{}\n{}",
            text("enStart"),
            text("enAfterLanguage"),
            text("enAnalysis"),
            persona(preferences, false),
            text("enEnd")
        )
    }
}
pub fn format_position(p: &Position, l: Language) -> String {
    let mut out = String::new();
    for (key, v) in [
        ("positionAfter", integer(u64::from(p.move_number))),
        ("sideToPlay", p.to_play.clone()),
        ("rootVisits", integer(p.playouts)),
        (
            "actualMove",
            if p.actual_move.is_empty() {
                localized(l, "notAvailable").into()
            } else {
                p.actual_move.clone()
            },
        ),
    ] {
        out.push_str(&label(l, key, &v));
        out.push('\n');
    }
    if let Some(loss) = p.actual_winrate_loss.filter(|n| n.is_finite()) {
        out.push_str(&label(l, "winrateLoss", &format!("{loss:.1}")));
        out.push('\n');
    }
    if p.board.is_none() && !p.played_continuation.is_empty() {
        out.push_str(&label(l, "playedContinuation", &p.played_continuation.join(" ")));
        out.push('\n');
    }
    for c in &p.candidates {
        out.push_str(&format!(
            "{}: move={}, visits={}",
            label(l, "candidateRank", &integer(c.rank as u64)),
            c.coordinate,
            c.visits
        ));
        if c.winrate.is_finite() {
            out.push_str(&format!(", winrate={:.1}%", c.winrate));
        }
        if c.score_lead.is_finite() {
            out.push_str(&format!(", scoreLead={:.1}", c.score_lead));
        }
        if p.board.is_none() && !c.variation.is_empty() {
            out.push_str(", pv=");
            let mut black = p.to_play == "B";
            for (i, m) in c.variation.iter().enumerate() {
                if i > 0 {
                    out.push(' ');
                }
                out.push_str(m);
                out.push_str(localized(l, if black { "black" } else { "white" }));
                black = !black;
            }
        }
        out.push('\n');
    }
    if !p.actual_move.is_empty()
        && !p
            .candidates
            .iter()
            .any(|c| c.coordinate.eq_ignore_ascii_case(&p.actual_move))
    {
        out.push_str(&label(l, "notInCandidates", &p.actual_move));
        out.push('\n');
    }
    if let Some(loss) = p.actual_score_loss() {
        out.push_str(&format!("Actual move score loss (points, NOT percent): {loss:.1}\n"));
    }
    if let Some(board) = &p.board {
        out.push('\n');
        out.push_str(&board.facts());
    }
    out
}
fn system(l: Language, p: &Preferences) -> Message {
    Message::new("system", system_prompt(l, p) + text("contract"))
}
pub fn for_position(position: &Position, l: Language, p: &Preferences) -> Vec<Message> {
    vec![
        system(l, p),
        Message::new(
            "user",
            format!(
                "{}\n\n【KataGo evidence】\n{}",
                localized(l, "modeNextMove"),
                format_position(position, l)
            ),
        ),
    ]
}
pub fn for_range(range: &Range, mode: Mode, l: Language, p: &Preferences) -> Vec<Message> {
    let mut evidence = format!(
        "Analyzed positions: {}; selected key positions: {}",
        range.analyzed_positions,
        range.positions.len()
    );
    if range.omitted_positions > 0 {
        evidence.push_str(&format!(
            "; omitted lower-priority positions: {}",
            range.omitted_positions
        ));
    }
    evidence.push('\n');
    for p in &range.positions {
        evidence.push('\n');
        evidence.push_str(&format_position(p, l));
    }
    vec![
        system(l, p),
        Message::new(
            "user",
            format!(
                "{}\n\n【KataGo evidence】\n{evidence}",
                localized(l, if mode == Mode::Whole { "modeWhole" } else { "modeRange" })
            ),
        ),
    ]
}
pub fn for_follow_up(
    context: &[Message],
    previous_answer: &str,
    question: &str,
    l: Language,
    p: &Preferences,
) -> Vec<Message> {
    let mut messages = context.to_vec();
    if messages.is_empty() {
        messages.push(Message::new("system", system_prompt(l, p)));
    }
    let answer = super::java_trim(previous_answer);
    let utf16_len = answer.encode_utf16().count();
    let answer = if utf16_len > 12_000 {
        String::from_utf16_lossy(&answer.encode_utf16().skip(utf16_len - 12_000).collect::<Vec<_>>())
    } else {
        answer.into()
    };
    if !answer.is_empty() {
        messages.push(Message::new("assistant", answer));
    }
    messages.push(Message::new(
        "user",
        format!("{}\n\n{}", localized(l, "modeFollowUp"), super::java_trim(question)),
    ));
    messages
}
