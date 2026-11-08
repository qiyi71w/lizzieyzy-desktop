//! Fox record normalisation against goldens produced by the fixed Java build (cb53acc).
//!
//! `fixtures/fox_kifu/normalize_golden.json` holds the `chess` text Java's
//! `GetFoxRequest.normalizeFoxSgfPayload` returns for each input, with the live board set to the
//! record's size (the product always merges with the record's own size).

use lizzie_core::fox_kifu::normalize::normalize;
use serde_json::Value;

const GOLDEN: &str = include_str!("fixtures/fox_kifu/normalize_golden.json");

#[test]
fn matches_java_goldens() {
    let cases: Vec<Value> = serde_json::from_str(GOLDEN).expect("golden fixture is JSON");
    assert!(cases.len() >= 15, "fixture lost cases: {}", cases.len());
    let mut failures = Vec::new();
    for case in &cases {
        let name = case["name"].as_str().expect("name");
        let input = case["input"].as_str().expect("input");
        let no_capture = case["noCapture"].as_bool().expect("noCapture");
        let expected = case["output"].as_str().expect("output");
        let actual = normalize(input, no_capture);
        if actual != expected {
            failures.push(format!("{name}\n  expected: {expected}\n  actual:   {actual}"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} case(s) differ from Java:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn deeply_nested_record_keeps_its_main_line() {
    let depth = 50_000;
    let mut sgf = String::from("(;GM[1]FF[4]SZ[19]");
    for i in 0..depth {
        sgf.push_str(if i % 2 == 0 { "(;B[aa]" } else { "(;W[bb]" });
    }
    sgf.push_str(&")".repeat(depth + 1));

    let normalized = normalize(&sgf, false);

    assert!(normalized.starts_with("(;GM[1]FF[4]CA[UTF-8]SZ[19];B[aa];W[bb];B[aa]"));
    assert_eq!(normalized.matches(';').count(), depth + 1);
}
