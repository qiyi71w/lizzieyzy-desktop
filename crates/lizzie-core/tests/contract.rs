//! The TypeScript side of the contract is generated from the Rust types. This test fails when
//! `ui/src/contract/generated.ts` no longer matches them; regenerate with
//! `UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract`.

use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::profile::{AnalysisSettings, EngineProfile, EngineStartup};
use lizzie_core::record::{BoardSize, GameInfo, Mark, MarkKind, Point, RecordFormat, Stone};
use lizzie_core::settings::{Language, WindowPlacement};
use ts_rs::{Config, TS};

macro_rules! decls {
    ($cfg:expr; $($t:ty),* $(,)?) => {
        vec![$(<$t as TS>::decl($cfg)),*]
    };
}

fn generate() -> String {
    // u64 values are serialised as JSON numbers.
    let cfg = Config::default().with_large_int("number");
    let decls = decls![&cfg;
        Stone, Point, BoardSize, MarkKind, Mark, GameInfo, RecordFormat, Language, WindowPlacement,
        Navigation, OpenSource, EngineProfile, EngineStartup, AnalysisSettings, EngineScope,
        Command, CommandResult, ErrorKind, ApiError,
        DocumentView, TreeNodeKind, TreeNodeView, TreeView, LastAction, PositionView,
        OpenOutcome, SaveOutcome, PendingOpen, EngineSettingsView, SettingsView, StoneSound,
        EngineState, EngineEntryView, ForegroundEngineView, EngineView, CandidateView, NodeAnalysisView,
        HistoryPoint, EngineFailureView, EngineNotice, ConsoleLineKind, ConsoleLineView, RulesPromptView,
        WorkbenchView, LogsStatus,
    ];
    let mut out = String::from(
        "// Generated from crates/lizzie-core (api, record, browse, settings). Do not edit.\n\
         // Regenerate: UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract\n\n",
    );
    for d in decls {
        out.push_str("export ");
        out.push_str(d.trim());
        out.push_str("\n\n");
    }
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

#[test]
fn typescript_contract_matches_rust_types() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/contract/generated.ts");
    let generated = generate();
    if std::env::var_os("UPDATE_CONTRACT").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, &generated).unwrap();
        return;
    }
    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == generated,
        "ui/src/contract/generated.ts is out of date; run UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract"
    );
}
