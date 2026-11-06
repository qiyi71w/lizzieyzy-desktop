use super::{library, match_text, reload, KnowledgeQuery};
use serde::Deserialize;
use std::{collections::BTreeSet, sync::Arc};

#[derive(Deserialize)]
struct Golden {
    name: String,
    query: KnowledgeQuery,
    expected: String,
}

#[test]
fn commentary_knowledge_matches_java_goldens_and_reloads() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/commentary/knowledge-golden.json");
    let fixture = std::fs::read_to_string(&path)
        .expect("Generate knowledge-golden.json using the cb53acc GoldenKnowledge Java harness first");
    let goldens: Vec<Golden> = serde_json::from_str(&fixture).expect("Java golden JSON");
    let names: BTreeSet<_> = goldens.iter().map(|g| g.name.as_str()).collect();
    for required in [
        "empty-no-match",
        "joseki-corner-sequence",
        "known-shape-explicit",
        "known-shape-geometry",
        "non-19-board",
        "late-move-still-opening",
        "tesuji-contact-loss",
        "pattern-candidate-pv-loss",
        "center-contact-intent",
        "side-jump",
        "geometry-transform-7-swapped",
    ] {
        assert!(names.contains(required), "missing golden scenario {required}");
    }
    for golden in &goldens {
        assert_eq!(
            match_text(&golden.query),
            golden.expected,
            "Java oracle: {}",
            golden.name
        );
    }
    // Mirrors the relevant Java read-only publication / parsed-once / reload
    // assertions. An Arc snapshot cannot mutate the published catalogue, and
    // remains valid after another request replaces the process-wide cache.
    let initial = library().expect("embedded catalogue loads");
    assert!(Arc::ptr_eq(&initial, &library().unwrap()));
    assert!(!initial.catalog.life_death_problems.is_empty());
    assert!(!initial.catalog.tesuji_problems.is_empty());
    assert!(!initial.catalog.joseki_lines.is_empty());
    assert!(!initial.patterns.is_empty());
    reload();
    let reloaded = library().expect("catalogue reloads");
    assert!(!Arc::ptr_eq(&initial, &reloaded));
    for golden in &goldens {
        assert_eq!(
            match_text(&golden.query),
            golden.expected,
            "after reload: {}",
            golden.name
        );
    }
    // Concurrent off-lock requests and reloads must see complete immutable data.
    let golden = goldens.iter().find(|g| g.name == "known-shape-geometry").unwrap();
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                reload();
                assert_eq!(match_text(&golden.query), golden.expected);
            });
        }
    });
}
