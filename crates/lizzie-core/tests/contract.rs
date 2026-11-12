//! The TypeScript side of the contract is generated from the Rust types. This test fails when
//! `ui/src/contract/generated.ts` no longer matches them; regenerate with
//! `UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract`.

#[path = "contract/readboard.rs"]
mod readboard;
#[path = "contract/resources.rs"]
mod resources;
#[path = "contract/online.rs"]
mod online;
#[path = "contract/tuning.rs"]
mod tuning;

use lizzie_core::api::*;
use lizzie_core::browse::Navigation;
use lizzie_core::engine::profile::{AnalysisSettings, EngineProfile, EngineStartup};
use lizzie_core::engine::raw_net::{RawNetMode, RawNetView};
use lizzie_core::function_search::{FunctionMatch, FunctionTarget, MatchLevel, SettingsSection};
use lizzie_core::record::{
    BoardSize, GameInfo, Mark, MarkKind, MarkTool, Point, RecordFormat, Stone, StoneChange, Transform,
};
use lizzie_core::display::{BoardOverlayView, MoveNumberLabel, MoveRank, NextMoveInfo, NextMoveVerdict, RankMark};
use lizzie_core::settings::{
    AnalysisEngineSettings, CoordinateStyle, DisplaySettings, EditSettings, EstimateStyle, ExtraMode, FlashRange,
    FramePlacement, KataEstimateState, Language, MoveRankMode, ProblemListSettings, ProblemSide, ProblemSort,
    PvVisitsMode, RecoverySettings, SubBoardMode, WindowPlacement,
    AutoAnalysisSettings, HawkeyeBranch, HawkeyePeriod, HawkeyePlacement, HawkeyeSettings,
};
use lizzie_core::viewpoint::{AsCandidate, ShownValues};
use ts_rs::Config;

#[macro_export]
macro_rules! decls {
    ($cfg:expr; $($t:ty),* $(,)?) => {
        vec![$(<$t as ts_rs::TS>::decl($cfg)),*]
    };
}
fn generate() -> String {
    // u64 values are serialised as JSON numbers.
    let cfg = Config::default().with_large_int("number");
    let mut decls = decls![&cfg;
        Stone, Point, BoardSize, MarkKind, Mark, GameInfo, RecordFormat, Language, WindowPlacement,
        Navigation, OpenSource, EngineProfile, EngineStartup, AnalysisSettings, EngineScope, AnalysisScope,
        Command, CommandResult, ErrorKind, ApiError,
        DocumentView, TreeNodeKind, TreeNodeView, TreeView, LastAction, PositionView,
        OpenOutcome, SaveOutcome, PendingOpen, EngineSettingsView, SettingsView, StoneSound,
        EngineState, EngineEntryView, ForegroundEngineView, EngineView, CandidateView, NodeAnalysisView,
        HistoryPoint, EngineFailureView, EngineNotice, ConsoleLineKind, ConsoleLineView, RulesPromptView,
        WorkbenchView, LogsStatus, JavaImportView, ImportStatus, ImportReason, ImportTarget, ImportItemView,
        EngineProblem, ImportEngineView, ImportPreviewView, ImportReportView, ImportFailure,
        ImportCommentaryCredential,
        lizzie_core::engine::runtime::KataParam, lizzie_core::engine::profile::KataParamSettings,
        lizzie_core::api::KataParamEdit, lizzie_core::api::KataEngineParamsEdit, lizzie_core::api::KataParamsView,
        lizzie_core::engine::profile::LeelaOptionSetting, lizzie_core::engine::profile::LeelaOptionSettings,
        lizzie_core::engine::profile::CustomPdaMode, lizzie_core::engine::profile::CustomPdaSettings,
        lizzie_core::api::CustomPdaView,
        lizzie_core::java_import::CloudImportCredentials,
        lizzie_core::engine::cloud::CloudLaunch, lizzie_core::engine::cloud::CloudProvider,
        lizzie_core::cloud::CloudSettings, lizzie_core::cloud::Provider,
        lizzie_core::cloud::CloudCommand, lizzie_core::cloud::CloudReply,
        lizzie_core::cloud::CloudState, lizzie_core::cloud::CloudResult, lizzie_core::cloud::CloudFailure,
        lizzie_core::cloud::CodePurpose, lizzie_core::cloud::Purchase,
        lizzie_core::cloud::Catalog, lizzie_core::cloud::Weight, lizzie_core::cloud::DiscoverySource,
        lizzie_core::cloud::AccountProfile, lizzie_core::cloud::BalanceInfo,
        lizzie_core::cloud::UsageRecord, lizzie_core::cloud::UsagePage,
        lizzie_core::cloud::CreditRecord, lizzie_core::cloud::CreditPage, lizzie_core::cloud::Overview,
        lizzie_core::cloud::MembershipProduct, lizzie_core::cloud::PaymentStatus,
        lizzie_core::cloud::PaymentPurpose, lizzie_core::cloud::PaymentOrder, lizzie_core::cloud::PaymentVerification,
        EditCommand, EditTarget, EditingView, EditSettings, MarkTool, StoneChange, Transform,
        ExportKind, CommentLabels, SlotRef, RecoveryCommand, SlotView, RecoveryView, RecoverySettings,
        ImportSlotView, RecoveryImport, DisplaySettings, ShownValues, AsCandidate,
        MoveRankMode, SubBoardMode, BoardOverlayView, MoveNumberLabel, MoveRank, RankMark, NextMoveVerdict,
        NextMoveInfo,
        RawNetMode, RawNetView,
        EstimateStyle, CoordinateStyle, PvVisitsMode,
        KataEstimateMode, KataEstimateState, KataEstimateView, ScoreCommand, ScorePoint, ScoreView,
        MoveEvaluation, MoveChange, StatisticsView, FramePlacement,
        ExtraMode, SecondaryEngineView, SecondaryView,
        AnalysisEngineSettings, QuickModelView, QuickModelStatus, QuickAnalysisView, ProblemListSettings,
        HumanSlSettingsView, HumanSlModelView, HumanSlModelStatus,
        lizzie_core::settings::HumanSlSettings, lizzie_core::settings::HumanSlTrainingSettings,
        lizzie_core::settings::TrainingMode, lizzie_core::settings::OpponentPreset,
        lizzie_core::settings::PlayerColor,
        ProblemSide, ProblemSort, ProblemListView, ProblemMoveView, LeaseAvailabilityView, UnrestoredReason,
        FlashTarget, FlashRange, FlashAnalysisView, FlashOutcome,
        AutoAnalysisSettings, AutoAnalysisView, AutoAnalysisNotice,
        WholeGameState, WholeGameAnalysisView, lizzie_core::settings::WholeGameSettings,
        HawkeyeCommand, HawkeyeSide, HawkeyeBand, HawkeyeSpread, HawkeyeStage, HawkeyeMistake, HawkeyeRunning,
        HawkeyePane, HawkeyeView, HawkeyeSettings, HawkeyeBranch, HawkeyePeriod, HawkeyePlacement,
        BatchMode, BatchMove, BatchCommand, BatchItemView, BatchRunView, BatchFailure, BatchNotice, BatchView,
        FoxCommand, FoxReplaceChoice, FoxView, FoxClockView, FoxSubmissionView, FoxStartOutcome,
        lizzie_core::api::GameCommand, lizzie_core::api::GameKind, lizzie_core::api::GamePhase,
        lizzie_core::api::GameClockView, lizzie_core::api::GameView, lizzie_core::api::GameEnd,
        lizzie_core::api::GameOutcomeView, lizzie_core::api::GameNotice, lizzie_core::api::AutoPlayView,
        lizzie_core::settings::GameSettings, lizzie_core::settings::AiTimeSettings,
        lizzie_core::settings::AiTimeMode, lizzie_core::settings::KataTimeType,
        lizzie_core::settings::RandomStartSettings, lizzie_core::settings::AutoPlaySettings,
        lizzie_core::api::MatchSessionSettings, lizzie_core::api::MatchCommand,
        lizzie_core::api::MatchIntervention, lizzie_core::api::MatchOpeningFile,
        lizzie_core::api::MatchPhase, lizzie_core::api::MatchScoreView, lizzie_core::api::MatchView,
        lizzie_core::api::MatchRulesUnverified, lizzie_core::api::MatchEnd,
        lizzie_core::api::MatchOutcomeView, lizzie_core::api::MatchNotice,
        lizzie_core::api::MatchWinrateImageRequest, lizzie_core::api::MatchSettingsView,
        lizzie_core::settings::MatchTimeMode, lizzie_core::settings::MatchIdentity,
        lizzie_core::settings::MatchResignPolicy, lizzie_core::settings::MatchSettings,
        HumanSlCommand, HumanSlPhase, HumanSlStartupStage, HumanSlSide, HumanSlAnalysisDisplay,
        HumanSlView, HumanSlNotice,
        HumanSlContinuation,
        StrengthCommand, StrengthView, StrengthStatusView, StrengthManualView, StrengthManualOutcome,
        StrengthModelView, StrengthReport, StrengthSideReport, StrengthSegment, StrengthSummary,
        StrengthSample, StrengthConfidence, StrengthMoveCategory,
        CommentaryCommand, ConnectionCommand, CommentaryView, CommentaryMode, CommentaryProvider,
        WebBoardCommand, WebBoardView,
        CommentaryPreferences, CommentarySettings, CommentaryConnectionFacts, CommentaryAccount,
        CommentaryModel, CommentarySecureStorage, CommentaryChatGptFacts, CommentaryText,
        CommentaryStatus, CommentaryTone, CommentaryOutputState,
        FoxKifuCommand, KifuPage, FoxKifuView, FoxKifuRow, FoxKifuBusy, KifuNotice,
        lizzie_core::settings::KifuAfterGet, lizzie_core::settings::FoxRecentSearch,
        TencentKifuCommand, TencentKifuView, TencentKifuRow, TencentKifuBusy,
        lizzie_core::settings::TencentRecentSearch,
        FunctionMatch, FunctionTarget, MatchLevel, SettingsSection,
        lizzie_core::settings::PanelSettings, lizzie_core::settings::PanelSizes,
        lizzie_core::settings::TopToolbarSettings,
        lizzie_core::settings::BottomToolbar, lizzie_core::settings::WorkbenchPanel,
        lizzie_core::appearance::AppearanceSettings, lizzie_core::appearance::BoardStyle,
        lizzie_core::appearance::ThemeSettings,
        lizzie_core::appearance::ThemeNodeColor,
        lizzie_core::appearance::ThemeEntry, lizzie_core::appearance::AppearanceCatalog,
        lizzie_core::appearance::ThemeImages,
        lizzie_core::settings::CustomLayout, lizzie_core::settings::IndependentBoard,
        RegionCommand, TrackingCommand, RegionView, FocusView, TrackedPointView,
        lizzie_core::engine::runtime::FocusCapability, lizzie_core::settings::RegionSettings,
        lizzie_core::settings::TrackingSettings,
        lizzie_core::settings::TsumegoSettings, lizzie_core::settings::TsumegoFrameSettings,
        lizzie_core::settings::CaptureSettings, lizzie_core::tsumego::TsumegoToPlay,
        lizzie_core::tsumego::TsumegoAttacker, lizzie_core::tsumego::TsumegoKoThreat,
        lizzie_core::tsumego::capture::CapturedProblem, lizzie_core::tsumego::capture::CapturePreview,
        lizzie_core::engine::ssh::SshSettings, lizzie_core::engine::ssh::RemoteFailure,
        SshCredentialCommand, SshPasswordState,
        ContributionCommand, ContributionFile, ContributionLink, ContributionPhase, ContributionTip,
        ContributionGameView, ContributionView, ContributionNotice, SavedEngineKind, SavedEngineReason,
        SavedEngineCandidate, ContributionQualification,
        lizzie_core::settings::ContributionSettings, lizzie_core::settings::ContributionWatchSettings,
    ];
    // 棋盘同步工具, engine resources and engine performance keep their own lists.
    decls.extend(readboard::decls(&cfg));
    decls.extend(resources::decls(&cfg));
    decls.extend(tuning::decls(&cfg));
    decls.extend(online::decls(&cfg));
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
