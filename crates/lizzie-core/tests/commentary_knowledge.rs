//! Knowledge matching through the public catalogue interface. A problem whose diagram is not
//! marked `positionVerified: true` is a placeholder: it must not match a board with the same
//! stones, and it must not reach the knowledge text, whose coordinates the model would cite.

use lizzie_core::commentary::knowledge::{match_text, match_text_with_catalogues, KnowledgeQuery};
use serde_json::{json, Value};

const TRAINING: &str = include_str!("../resources/knowledge/training-catalog.json");
const PATTERNS: &str = include_str!("../resources/knowledge/pattern-cards.json");
const GOLDEN: &str = include_str!("fixtures/commentary/knowledge-golden.json");
/// Knowledge text the verified problem produces on the fixed position, recorded from the
/// starting commit (4144d7f), which ignores the flag: a verified diagram must keep that output.
const VERIFIED_EXPECTED: &str = include_str!("fixtures/commentary/verified-problem-knowledge.txt");
const PROBLEM: &str = "角部 L 形：对杀分支";

fn bundled_problem(title: &str) -> Value {
    let catalog: Value = serde_json::from_str(TRAINING).unwrap();
    catalog["lifeDeathProblems"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["title"] == title)
        .unwrap_or_else(|| panic!("bundled problem {title}"))
        .clone()
}

/// A catalogue holding only the fixed-position problem; `flag` is written as `positionVerified`
/// when given.
fn single_problem_catalog(flag: Option<Value>) -> String {
    let mut problem = bundled_problem(PROBLEM);
    if let Some(flag) = flag {
        problem["positionVerified"] = flag;
    }
    json!({"lifeDeathProblems": [problem], "tesujiProblems": [], "josekiLines": []}).to_string()
}

/// The problem's own stones, with its first answer played and offered as the candidate.
fn fixed_position() -> KnowledgeQuery {
    let problem = bundled_problem(PROBLEM);
    let answer = problem["correctMoves"][0]["move"].as_str().unwrap().to_owned();
    let snapshot: Vec<Value> = problem["initialStones"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| json!({"point": s["point"], "color": s["color"]}))
        .collect();
    serde_json::from_value(json!({
        "boardSize": 19,
        "moveNumber": 40,
        "playedMove": answer,
        "text": "",
        "candidateMoves": [answer],
        "principalVariation": [answer],
        "boardSnapshot": snapshot,
        "lossScore": 3.0
    }))
    .unwrap()
}

fn knowledge(flag: Option<Value>) -> String {
    match_text_with_catalogues(&single_problem_catalog(flag), "[]", &fixed_position()).unwrap()
}

#[test]
fn placeholder_problem_does_not_match_identical_stones() {
    let text = knowledge(None);
    assert_eq!(
        text, "",
        "no geometry, no answer overlap, no exact match, no knowledge text"
    );
    assert!(
        !text.contains("E2") && !text.contains("Q15"),
        "diagram coordinates stay out of the facts"
    );
}

#[test]
fn only_a_boolean_true_flag_marks_a_verified_diagram() {
    for flag in [json!(false), json!("true"), json!(1), Value::Null] {
        assert_eq!(
            knowledge(Some(flag.clone())),
            "",
            "flag {flag} is not a verified diagram"
        );
    }
}

#[test]
fn verified_problem_matches_as_before() {
    assert_eq!(knowledge(Some(json!(true))), VERIFIED_EXPECTED);
}

#[test]
fn bundled_placeholders_are_not_matched_on_the_board() {
    let text = match_text(&fixed_position());
    assert!(!text.contains(PROBLEM), "bundled placeholder {PROBLEM} matched: {text}");
}

#[test]
fn non_position_knowledge_is_unchanged() {
    let goldens: Vec<Value> = serde_json::from_str(GOLDEN).unwrap();
    let golden = goldens.iter().find(|g| g["name"] == "joseki-corner-sequence").unwrap();
    let query: KnowledgeQuery = serde_json::from_value(golden["query"].clone()).unwrap();
    let expected = golden["expected"].as_str().unwrap();
    assert_eq!(match_text(&query), expected);
    assert_eq!(
        match_text_with_catalogues(TRAINING, PATTERNS, &query).unwrap(),
        expected
    );
}
