use super::evidence::{self, Position};
use fancy_regex::Regex;
use std::collections::HashSet;
use std::sync::LazyLock;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Verification {
    pub violations: Vec<String>,
    pub warnings: Vec<String>,
}
impl Verification {
    pub fn ok(&self) -> bool {
        self.violations.is_empty()
    }
    pub fn has_notes(&self) -> bool {
        !self.violations.is_empty() || !self.warnings.is_empty()
    }
}
static COORDINATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?<![A-Za-z0-9])([A-HJ-Za-hj-z](?:[1-9]|1[0-9]|2[0-5]))(?![A-Za-z0-9])").expect("coordinate regex")
});
static PERCENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)\s*%").expect("percent regex"));
static ABSOLUTE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\A.*(明显恶手|必败|唯一|绝对|certainly|only\s+move|forced).*\z").expect("wording regex")
});
pub fn verify(markdown: &str, positions: &[Position]) -> Verification {
    let mut result = Verification::default();
    if markdown.trim().is_empty() {
        return result;
    }
    let mut allowed = HashSet::<String>::new();
    for p in positions {
        if !p.actual_move.is_empty() {
            allowed.insert(p.actual_move.to_uppercase());
        }
        for c in &p.candidates {
            if !c.coordinate.is_empty() {
                allowed.insert(c.coordinate.to_uppercase());
            }
        }
        if let Some(board) = &p.board {
            for g in &board.groups {
                allowed.extend(g.stones.iter().cloned());
                allowed.extend(g.liberties.iter().cloned());
            }
            for line in &board.lines {
                allowed.extend(line.moves.iter().cloned());
                if let Some(frame) = line.frames.last() {
                    for g in evidence::groups(&frame.stones) {
                        if g.liberties.len() <= 2 {
                            allowed.extend(g.liberties);
                        }
                    }
                }
            }
        } else {
            allowed.extend(p.played_continuation.iter().map(|s| s.to_uppercase()));
            for c in &p.candidates {
                allowed.extend(c.variation.iter().map(|s| s.to_uppercase()));
            }
        }
    }
    if !allowed.is_empty() {
        for m in COORDINATE.captures_iter(markdown).flatten() {
            let coordinate = m[1].to_uppercase();
            if !allowed.contains(&coordinate) {
                result.violations.push(format!(
                    "Unsupported coordinate {coordinate}: not in the supplied KataGo or game-record evidence."
                ));
            }
        }
    }
    if ABSOLUTE.is_match(markdown).unwrap_or(false) {
        result
            .warnings
            .push("The wording sounds too absolute; soften it unless the evidence clearly supports it.".into());
    }
    for m in PERCENT.captures_iter(markdown).flatten() {
        if let Ok(v) = m[1].parse::<f64>() {
            if v > 100.0 {
                let number = if v.fract() == 0.0 {
                    format!("{v:.1}")
                } else {
                    v.to_string()
                };
                result
                    .violations
                    .push(format!("Impossible winrate percentage {number}%."));
            }
        }
    }
    result
}
