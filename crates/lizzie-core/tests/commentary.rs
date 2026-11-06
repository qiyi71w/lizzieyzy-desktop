use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::commentary::{
    self, codec,
    evidence::{Candidate, Position, Range},
    grounded, prompt,
    settings::Preferences,
    verify, ChatTransport, CommentaryFailure, CommentaryJob, JobConnection, Message,
};
use lizzie_core::engine::{
    gtp::LaunchSpec,
    runtime::{EngineIo, InstanceId, Launcher},
};
use lizzie_core::record::{DocumentEdit, RecordFormat, Stone, ROOT};
use lizzie_core::settings::{Config, Language, Storage};
use lizzie_core::workbench::{OpenOrigin, Workbench};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
const GAME: &str = include_str!("fixtures/commentary/analyzed.sgf");
struct NoEngine;
impl Launcher for NoEngine {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        panic!("commentary must not launch an engine")
    }
}
fn fixture(text: &str) -> (tempfile::TempDir, Workbench) {
    let dir = tempfile::tempdir().unwrap();
    let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let mut config = Config::default();
    config.ui.language = Language::EnUs;
    config.record.auto_quick_analyze_on_load = false;
    lizzie_core::settings::save_json(&storage.config_file, &config).unwrap();
    let mut wb = Workbench::new(storage, None, Box::new(NoEngine), vec![]);
    open(&mut wb, text);
    wb.set_commentary_connection(
        CommentaryProvider::ApiKey,
        "https://example.com".into(),
        "test-model".into(),
        false,
    )
    .unwrap();
    wb.set_commentary_facts(CommentaryConnectionFacts {
        loaded: true,
        api_key_present: true,
        ..Default::default()
    });
    wb.commentary(CommentaryCommand::Open).unwrap();
    (dir, wb)
}
fn open(wb: &mut Workbench, text: &str) {
    let read = lizzie_core::formats::parse(RecordFormat::Sgf, text, false);
    let t = wb.begin_open(OpenOrigin::Clipboard);
    assert!(wb.complete_open(t, read.map(|r| (RecordFormat::Sgf, r))));
}
fn start(wb: &mut Workbench, mode: CommentaryMode) -> CommentaryJob {
    assert_eq!(
        wb.commentary(CommentaryCommand::Start {
            mode,
            first: 1,
            last: 1000
        })
        .unwrap(),
        CommandResult::Done
    );
    let mut jobs = wb.take_commentary_jobs();
    assert_eq!(jobs.len(), 1);
    jobs.remove(0)
}
fn example() -> Position {
    Position {
        move_number: 42,
        to_play: "B".into(),
        playouts: 1600,
        actual_move: "D4".into(),
        actual_winrate_loss: Some(2.5),
        candidates: vec![
            Candidate {
                rank: 1,
                coordinate: "Q16".into(),
                winrate: 62.4,
                score_lead: 3.1,
                visits: 1000,
                variation: vec!["Q16".into(), "D4".into()],
            },
            Candidate {
                rank: 2,
                coordinate: "D4".into(),
                winrate: 59.9,
                score_lead: 1.4,
                visits: 600,
                variation: vec!["D4".into(), "Q16".into()],
            },
        ],
        played_continuation: vec![],
        board: None,
    }
}
#[test]
fn frozen_evidence_uses_top_three_plus_actual_and_stored_playouts() {
    let (_dir, mut wb) = fixture(GAME);
    let job = start(&mut wb, CommentaryMode::Next);
    let p = &job.positions[0];
    // Java SGFParser.java:4730 stores the header's rootVisits= as the position's playouts.
    assert_eq!(p.playouts, 9000);
    assert_eq!(p.actual_move, "D4");
    assert_eq!(
        p.candidates.iter().map(|c| c.rank).collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!((p.actual_winrate_loss.unwrap() - 6.5).abs() < 1e-9);
    assert!((p.actual_score_loss().unwrap() - 0.5).abs() < 1e-9);
    assert_eq!(p.candidates[0].variation.len(), 12);
    assert_eq!(p.played_continuation.len(), 5);
    assert!(p.board.is_some());
    assert!(!job.messages.iter().any(|m| m.content.contains("User comment")));
    wb.navigate(Navigation::Last);
    assert_eq!(p.move_number, 0);
    assert_eq!(p.board.as_ref().unwrap().width, 19);
}
#[test]
fn range_selects_spaced_chronological_key_moments_and_counts_omissions() {
    let (_dir, mut wb) = fixture(include_str!("fixtures/strength/seed12-kata-61.sgf"));
    let job = start(&mut wb, CommentaryMode::Whole);
    assert_eq!(job.positions.len(), 5);
    assert!(job
        .positions
        .windows(2)
        .all(|p| p[1].move_number - p[0].move_number >= 3));
    assert!(job.messages[1]
        .content
        .contains("Analyzed positions: 61; selected key positions: 5; omitted lower-priority positions: 56"));
    assert_eq!(wb.inspect().commentary.scope.key, "Teacher.position.whole");
}
#[test]
fn replay_records_captures_and_excludes_illegal_suffixes() {
    let text = "(;SZ[9]AB[ab][ba][cb]AW[bb]LZ[KataGo 60 100 1 10 0\nmove B7 visits 100 winrate 6000 prior 1000 scoreMean 1 order 0 pv B7 A8 C3];B[bc])";
    let (_dir, mut wb) = fixture(text);
    let job = start(&mut wb, CommentaryMode::Next);
    let b = job.positions[0].board.as_ref().unwrap();
    let line = b.lines.iter().find(|l| l.label == "B7").unwrap();
    assert_eq!(line.moves, vec!["B7"]);
    assert!(line.truncated);
    assert_eq!(line.frames[1].captures, 1);
    assert!(b.facts().contains("captures=1"));
    assert!(!verify::verify("C3", &job.positions).ok());
}
#[test]
fn prompts_match_fixed_java_golden_for_languages_preferences_and_modes() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/commentary/prompt-golden.json");
    let golden: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("generate with GoldenPrompt.java against cb53acc jar"))
            .unwrap();
    for case in golden.as_array().unwrap() {
        let language = Language::from_system_locale(case["language"].as_str());
        let preferences: Preferences = serde_json::from_value(case["preferences"].clone()).unwrap();
        let p = example();
        let original = prompt::for_position(&p, language, &preferences);
        let range = Range {
            positions: vec![p],
            analyzed_positions: 7,
            omitted_positions: 6,
        };
        let actual = match case["kind"].as_str().unwrap() {
            "range" => prompt::for_range(&range, CommentaryMode::Range, language, &preferences),
            "whole" => prompt::for_range(&range, CommentaryMode::Whole, language, &preferences),
            "followUp" => prompt::for_follow_up(
                &original,
                "previous answer",
                "Why was move 42 important?",
                language,
                &preferences,
            ),
            _ => original,
        };
        assert_eq!(
            serde_json::to_value(actual).unwrap(),
            case["messages"],
            "{} {} {}",
            case["language"],
            case["kind"],
            case["preferences"]
        );
    }
}
#[test]
fn follow_up_retains_context_and_last_twelve_thousand_characters() {
    let context = vec![
        Message::new("system", "evidence-only"),
        Message::new("user", "move 12 evidence\nmove 38 evidence"),
    ];
    let answer = "x".repeat(1000) + &"y".repeat(12000);
    let messages = prompt::for_follow_up(&context, &answer, " Why? ", Language::EnUs, &Preferences::default());
    assert_eq!(&messages[..2], &context);
    assert_eq!(messages[2].content, "y".repeat(12000));
    assert!(messages[3].content.ends_with("\n\nWhy?"));
    let mut p = example();
    p.candidates[0].winrate = f64::NAN;
    p.candidates[0].score_lead = f64::INFINITY;
    let evidence = prompt::format_position(&p, Language::EnUs);
    assert!(!evidence.contains("NaN"));
    assert!(!evidence.contains("inf"));
}
#[test]
fn verifier_mirrors_java_coordinate_union_percentages_and_warning_only_wording() {
    let p = example();
    assert!(verify::verify("Q16 then D4", std::slice::from_ref(&p)).ok());
    assert!(!verify::verify("C3", std::slice::from_ref(&p)).ok());
    let mut other = p.clone();
    other.actual_move = "C3".into();
    assert!(verify::verify("C3", &[p.clone(), other]).ok());
    let mut continuation = p;
    continuation.played_continuation.push("C3".into());
    assert!(verify::verify("C3", &[continuation]).ok());
    let check = verify::verify("certainly the only move has 125.5%", &[]);
    assert_eq!(check.violations, vec!["Impossible winrate percentage 125.5%."]);
    assert_eq!(check.warnings.len(), 1);
    assert!(verify::verify("Q25", &[]).ok());
    assert!(verify::verify("forced\nreply", &[]).warnings.is_empty());
}
struct FakeTransport {
    replies: Vec<String>,
    calls: Vec<Vec<Message>>,
    cancel_on_return: bool,
}
impl ChatTransport for FakeTransport {
    fn complete(&mut self, m: &[Message], cancel: &AtomicBool) -> Result<String, CommentaryFailure> {
        self.calls.push(m.to_vec());
        if self.cancel_on_return {
            cancel.store(true, Ordering::Release);
        }
        Ok(self.replies.remove(0))
    }
}
fn job() -> CommentaryJob {
    CommentaryJob {
        id: 1,
        messages: vec![Message::new("system", "original frozen evidence")],
        positions: vec![example()],
        connection: JobConnection::ApiKey {
            base_url: "https://example.com".into(),
            model: "test".into(),
        },
        cancel: Arc::new(AtomicBool::new(false)),
    }
}
#[test]
fn grounded_loop_repairs_once_without_exposing_invalid_drafts() {
    let mut transport = FakeTransport {
        replies: vec!["C3".into(), "Q16".into()],
        calls: vec![],
        cancel_on_return: false,
    };
    assert_eq!(grounded::run(&mut transport, &job()).unwrap(), "Q16");
    assert_eq!(transport.calls.len(), 2);
    assert_eq!(transport.calls[1][1], Message::new("assistant", "C3"));
    assert!(transport.calls[1][2].content.starts_with(grounded::CORRECTION));
    let mut transport = FakeTransport {
        replies: vec!["C3".into(), "C3".into()],
        calls: vec![],
        cancel_on_return: false,
    };
    assert_eq!(
        grounded::run(&mut transport, &job()).unwrap_err().message_key,
        "Teacher.learning.unverified"
    );
    let mut transport = FakeTransport {
        replies: vec!["C3".into()],
        calls: vec![],
        cancel_on_return: true,
    };
    assert!(grounded::run(&mut transport, &job()).unwrap_err().cancelled);
    assert_eq!(transport.calls.len(), 1);
}
#[test]
fn codec_replaces_complete_blocks_preserves_incomplete_and_escapes_markers() {
    let text = codec::upsert(
        "user  \ntext",
        " answer\r\nline ",
        "model\r\nnext",
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    assert_eq!(codec::extract(&text).unwrap(), "answer\nline");
    assert_eq!(codec::remove_blocks(&text), "user  \ntext");
    assert!(text.contains("model=model next\n"));
    let replaced = codec::upsert(&text, "new", "", "now").unwrap();
    assert_eq!(replaced.matches(codec::BEGIN).count(), 1);
    assert_eq!(codec::extract(&replaced).unwrap(), "new");
    let incomplete = format!("user\n{}\nbroken", codec::BEGIN);
    assert_eq!(codec::remove_blocks(&incomplete), incomplete);
    assert!(codec::extract(&incomplete).is_none());
    let escaped = codec::upsert("", codec::BEGIN, "", "now").unwrap();
    assert!(codec::extract(&escaped).unwrap().contains("BEGIN escaped"));
    assert!(codec::upsert("", "  ", "", "now").is_err());
    assert!(codec::upsert("", &"😀".repeat(50_001), "", "now").is_err());
}
#[test]
fn lifecycle_saves_to_captured_target_across_navigation_and_splits_display_comment() {
    let (_dir, mut wb) = fixture(GAME);
    let job = start(&mut wb, CommentaryMode::Next);
    wb.navigate(Navigation::Last);
    assert!(wb.commentary_finished(job.id, Ok("D4 is a useful reference.".into())));
    let saved = &wb.document().record.node(ROOT).comment;
    assert_eq!(codec::extract(saved).unwrap(), "D4 is a useful reference.");
    assert!(saved.starts_with("User comment"));
    wb.navigate(Navigation::First);
    let view = wb.inspect();
    assert_eq!(
        view.position.ai_commentary.as_deref(),
        Some("D4 is a useful reference.")
    );
    assert!(!view.position.user_comment.contains(codec::BEGIN));
    assert!(view.position.comment.contains(codec::BEGIN));
    assert_eq!(view.commentary.status.key, "Teacher.status.completedSaved");
    wb.commentary(CommentaryCommand::Close).unwrap();
    wb.commentary(CommentaryCommand::Open).unwrap();
    assert_eq!(wb.inspect().commentary.status.key, "Teacher.status.savedLoaded");
}
#[test]
fn stop_close_and_document_switch_cancel_and_drop_late_completion() {
    let (_dir, mut wb) = fixture(GAME);
    let j = start(&mut wb, CommentaryMode::Next);
    wb.commentary(CommentaryCommand::Stop).unwrap();
    assert!(j.cancel.load(Ordering::Acquire));
    assert!(!wb.commentary_finished(j.id, Ok("late".into())));
    let j = start(&mut wb, CommentaryMode::Next);
    wb.commentary(CommentaryCommand::Close).unwrap();
    assert!(!wb.commentary_finished(j.id, Ok("late".into())));
    wb.commentary(CommentaryCommand::Open).unwrap();
    let j = start(&mut wb, CommentaryMode::Next);
    open(&mut wb, "(;SZ[9])");
    assert!(j.cancel.load(Ordering::Acquire));
    assert!(!wb.commentary_finished(j.id, Ok("late".into())));
    assert!(wb.inspect().commentary.output.is_empty());
}
#[test]
fn changed_or_deleted_target_is_never_written_and_empty_reply_fails() {
    let (_dir, mut wb) = fixture(GAME);
    let j = start(&mut wb, CommentaryMode::Next);
    wb.edit_document(DocumentEdit::SetToPlay {
        node: ROOT,
        color: Stone::White,
    })
    .unwrap();
    assert!(wb.commentary_finished(j.id, Ok("lesson".into())));
    assert!(codec::extract(&wb.document().record.node(ROOT).comment).is_none());
    assert!(wb.inspect().commentary.target_changed);
    let (_dir, mut wb) = fixture(include_str!("fixtures/strength/seed12-kata-61.sgf"));
    wb.navigate(Navigation::Forward(1));
    let node = wb.inspect().position.node;
    let j = start(&mut wb, CommentaryMode::Next);
    wb.navigate(Navigation::First);
    wb.edit_document(DocumentEdit::Remove { node }).unwrap();
    assert!(wb.commentary_finished(j.id, Ok("lesson".into())));
    assert!(wb.inspect().commentary.target_changed);
    let (_dir, mut wb) = fixture(GAME);
    let j = start(&mut wb, CommentaryMode::Next);
    assert!(wb.commentary_finished(j.id, Ok("  ".into())));
    let v = wb.inspect();
    assert_eq!(v.commentary.status.key, "Teacher.status.failed");
    assert_eq!(v.commentary.status.args, vec!["AI service returned an empty response."]);
    assert!(!v.commentary.running);
}
#[test]
fn completion_checkbox_is_live_follow_up_question_restores_once_and_notes_are_display_only() {
    let (_dir, mut wb) = fixture(GAME);
    let j = start(&mut wb, CommentaryMode::Next);
    wb.commentary(CommentaryCommand::SetWriteToSgf { enabled: false })
        .unwrap();
    assert!(wb.commentary_finished(j.id, Ok("Q16 is forced".into())));
    assert!(codec::extract(&wb.document().record.node(ROOT).comment).is_none());
    assert!(wb.inspect().commentary.output.contains("Verifier notes"));
    wb.commentary(CommentaryCommand::Ask {
        question: "Why this reply?".into(),
    })
    .unwrap();
    let j = wb.take_commentary_jobs().remove(0);
    assert!(!j.messages.iter().any(|m| m.content.contains("Verifier notes")));
    assert!(j
        .messages
        .iter()
        .any(|m| m.role == "assistant" && m.content == "Q16 is forced"));
    assert!(wb.commentary_finished(j.id, Err(CommentaryFailure::new("network"))));
    assert_eq!(
        wb.view(false).commentary.restore_question.as_deref(),
        Some("Why this reply?")
    );
    assert!(wb.view(false).commentary.restore_question.is_none());
}
#[test]
fn settings_validation_clamping_and_persistence_are_nonsecret() {
    use commentary::settings::*;
    assert_eq!(
        validate_base_url(" https://example.com/// ").unwrap(),
        "https://example.com"
    );
    assert!(validate_base_url("http://example.com").is_err());
    assert!(validate_base_url("http://localhost:8080/v1").is_ok());
    assert!(validate_base_url("http://[::1]:8080").is_ok());
    for s in [
        "https://user@example.com",
        "https://example.com?key=x",
        "https://example.com/#secret",
        "https://example.com/%zz",
    ] {
        assert!(validate_base_url(s).is_err(), "{s}");
    }
    assert!(validate_model("a\nb").is_err());
    assert_eq!(validate_model(" model ").unwrap(), "model");
    assert_eq!(
        credential_account("https://example.com/").unwrap(),
        credential_account("https://example.com").unwrap()
    );
    let (_dir, mut wb) = fixture(GAME);
    wb.commentary(CommentaryCommand::SavePreferences {
        preferences: Preferences {
            rank_mode: "d".into(),
            rank_num: 99,
            style_index: -1,
            density_index: 20,
            pace_index: 20,
            variation_index: -1,
        },
    })
    .unwrap();
    assert_eq!(wb.config().commentary.preferences.rank_num, 9);
    assert_eq!(wb.inspect().commentary.preferences.style_index, 0);
    assert_eq!(wb.inspect().commentary.preferences.density_index, 2);
}
#[test]
fn follow_up_after_stop_keeps_the_frozen_request_target() {
    let (_dir, mut wb) = fixture(include_str!("fixtures/strength/seed12-kata-61.sgf"));
    let whole = start(&mut wb, CommentaryMode::Whole);
    wb.navigate(Navigation::Forward(7));
    assert_ne!(wb.inspect().position.node, ROOT);
    wb.commentary(CommentaryCommand::Stop).unwrap();
    wb.commentary(CommentaryCommand::Ask {
        question: "Why?".into(),
    })
    .unwrap();
    let ask = wb.take_commentary_jobs().remove(0);
    assert_eq!(ask.positions, whole.positions);
    assert!(wb.commentary_finished(ask.id, Ok("lesson".into())));
    assert_eq!(
        codec::extract(&wb.document().record.node(ROOT).comment).as_deref(),
        Some("lesson")
    );
    let cursor = wb.inspect().position.node;
    assert!(codec::extract(&wb.document().record.node(cursor).comment).is_none());
}
#[test]
fn loading_saved_commentary_drops_frozen_evidence_of_an_abandoned_request() {
    let (_dir, mut wb) = fixture(GAME);
    start(&mut wb, CommentaryMode::Next);
    wb.commentary(CommentaryCommand::Stop).unwrap();
    wb.navigate(Navigation::Forward(1));
    let b = wb.inspect().position.node;
    let saved = codec::upsert("", "saved lesson", "m", "2026-11-06T00:00:00Z").unwrap();
    wb.edit_document(DocumentEdit::SetComment {
        node: b,
        comment: saved,
    })
    .unwrap();
    wb.commentary(CommentaryCommand::Open).unwrap();
    assert_eq!(wb.inspect().commentary.output, "saved lesson");
    // B has no analysis: Java rebuilds evidence for the saved answer's node and refuses.
    wb.commentary(CommentaryCommand::Ask {
        question: "More?".into(),
    })
    .unwrap();
    assert!(wb.take_commentary_jobs().is_empty());
    assert_eq!(wb.inspect().commentary.status.key, "Teacher.status.needsAnalysis");
}
#[test]
fn comment_too_large_for_sgf_fails_and_restores_the_question() {
    let (_dir, mut wb) = fixture(GAME);
    let j = start(&mut wb, CommentaryMode::Next);
    assert!(wb.commentary_finished(j.id, Ok("Q16 first.".into())));
    wb.commentary(CommentaryCommand::Ask {
        question: "Why this reply?".into(),
    })
    .unwrap();
    let j = wb.take_commentary_jobs().remove(0);
    assert!(wb.commentary_finished(j.id, Ok("x".repeat(100_001))));
    let v = wb.view(false).commentary;
    assert_eq!(v.status.key, "Teacher.status.failed");
    assert_eq!(v.restore_question.as_deref(), Some("Why this reply?"));
}
