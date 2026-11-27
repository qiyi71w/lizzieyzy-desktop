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
        EngineState, EngineEntryView, ForegroundEngineView, EngineView, AnalysisStatus, ZhiziConnection, CandidateView, NodeAnalysisView,
        HistoryPoint, EngineFailureView, EngineNotice, ConsoleLineKind, ConsoleLineView, RulesPromptView, RulesStatus,
        SessionRulesView,
        WorkbenchView, LogsStatus, JavaImportView, ImportStatus, ImportReason, ImportTarget, ImportItemView,
        EngineProblem, ImportEngineView, ImportPreviewView, ImportReportView, ImportFailure,
        ImportCommentaryCredential,
        lizzie_core::diagnostics::DiagnosticModule, lizzie_core::diagnostics::TraceScope,
        lizzie_core::diagnostics::LoggingSettings,
        lizzie_core::quick_links::QuickLinkProgram, lizzie_core::quick_links::QuickLinkSettings,
        lizzie_core::engine::runtime::KataParam, lizzie_core::engine::profile::KataParamSettings,
        lizzie_core::api::KataParamEdit, lizzie_core::api::KataEngineParamsEdit, lizzie_core::api::KataParamsView,
        lizzie_core::engine::profile::LeelaOptionSetting, lizzie_core::engine::profile::LeelaOptionSettings,
        lizzie_core::engine::profile::CustomPdaMode, lizzie_core::engine::profile::CustomPdaSettings,
        lizzie_core::api::CustomPdaView,
        lizzie_core::engine::runtime::ThreadOverride, SearchThreadsCommand, ThreadRestoreSource, SearchThreadsView,
        SearchThreadsOperationView, SearchThreadsStatus,
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
        EditCommand, EditTarget, EditingView, EditSettings, lizzie_core::settings::MoveRect, ClickReviewView,
        lizzie_core::settings::PaintingColor,
        MarkTool, StoneChange, Transform,
        ExportKind, CommentLabels, SlotRef, SlotThumbnail, RecoveryCommand, SlotView, RecoveryView, RecoverySettings,
        ImportSlotView, RecoveryImport, DisplaySettings, ShownValues, AsCandidate,
        MoveRankMode, SubBoardMode, BoardOverlayView, MoveNumberLabel, MoveRank, RankMark, NextMoveVerdict,
        NextMoveInfo,
        RawNetMode, RawNetView,
        EstimateStyle, CoordinateStyle, PvVisitsMode,
        KataEstimateMode, KataEstimateState, KataEstimateView, ScoreCommand, ScorePoint, ScoreView, ScoreVerdict,
        MoveEvaluation, MoveChange, StatisticsView, lizzie_core::api::WinrateReading, FramePlacement,
        lizzie_core::settings::BigWinrateSettings, lizzie_core::settings::BigWinrateStyle,
        ExtraMode, SecondaryEngineView, SecondaryView,
        AnalysisEngineSettings, QuickModelView, QuickModelStatus, QuickAnalysisView, ProblemListSettings,
        HumanSlSettingsView, HumanSlModelView, HumanSlModelStatus, CoachLevelView,
        lizzie_core::settings::HumanSlSettings, lizzie_core::settings::HumanSlTrainingSettings,
        lizzie_core::settings::TrainingMode, lizzie_core::humansl::CoachLevel,
        lizzie_core::humansl::CoachLevelKind, lizzie_core::humansl::CoachGrade, lizzie_core::humansl::Pace,
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
        lizzie_core::api::PkBoardView,
        lizzie_core::api::MatchRulesUnverified, lizzie_core::api::MatchEnd,
        lizzie_core::api::MatchOutcomeView, lizzie_core::api::MatchNotice,
        lizzie_core::api::MatchWinrateImageRequest, lizzie_core::api::MatchSettingsView,
        lizzie_core::api::MatchParametersView, lizzie_core::api::MatchParameterPosition,
        lizzie_core::api::MatchParameterValue, lizzie_core::api::MatchParameter,
        lizzie_core::api::MatchParameterFailure, lizzie_core::api::MatchParameterFailureReason,
        lizzie_core::settings::MatchTimeMode, lizzie_core::settings::MatchIdentity,
        lizzie_core::settings::MatchResignPolicy, lizzie_core::settings::MatchSettings,
        lizzie_core::settings::MatchOpeningParameters,
        HumanSlCommand, HumanSlPhase, HumanSlStartupStage, HumanSlSide, HumanSlAnalysisDisplay,
        HumanSlView, HumanSlNotice, HumanSlResult,
        HumanSlContinuation, HumanSlDecision, HumanSlFeedbackView, HumanSlLastMove, HumanSlHintView,
        HumanSlDecisionView, lizzie_core::humansl::feedback::MoveFeedback, lizzie_core::humansl::feedback::Verdict,
        lizzie_core::humansl::assist::HintStep, lizzie_core::humansl::assist::CoachMarks,
        StrengthCommand, StrengthView, StrengthStatusView, StrengthManualView, StrengthManualOutcome,
        StrengthModelView, StrengthReport, StrengthSideReport, StrengthSegment, StrengthSummary,
        StrengthSample, StrengthConfidence, StrengthMoveCategory, StrengthContext, StrengthStage,
        StrengthPhases, StrengthPhase, PhaseSide, PhaseBadge, PhaseVerdict, PhaseReview,
        CommentaryCommand, ConnectionCommand, CommentaryView, CommentaryMode, CommentaryProvider,
        WebBoardCommand, WebBoardView,
        UpdateCommand, UpdateOrigin, UpdateOutcome, UpdateDownloadPhase, UpdateError, UpdateOfferView,
        UpdateResultView, UpdateDownloadView, UpdateView,
        lizzie_core::update::UpdateSettings, lizzie_core::update::UpdateChannel,
        lizzie_core::update::UpdateSource, lizzie_core::update::InstallForm, lizzie_core::update::FailureKind,
        CommentaryPreferences, CommentarySettings, CommentaryConnectionFacts, CommentaryAccount,
        CommentaryModel, CommentarySecureStorage, CommentaryChatGptFacts, CommentaryText,
        CommentaryStatus, CommentaryTone, CommentaryOutputState,
        FoxKifuCommand, KifuPage, FoxKifuView, FoxKifuRow, FoxKifuBusy, KifuNotice,
        lizzie_core::settings::KifuAfterGet, lizzie_core::settings::FoxRecentSearch,
        TencentKifuCommand, TencentKifuView, TencentKifuRow, TencentKifuBusy,
        lizzie_core::settings::TencentRecentSearch,
        OgsKifuCommand, OgsKifuView, OgsKifuRow, OgsKifuLabel, OgsKifuStatus,
        lizzie_core::ogs_kifu::OgsFailure,
        FunctionMatch, FunctionTarget, MatchLevel, SettingsSection,
        lizzie_core::settings::PanelSettings, lizzie_core::settings::PanelSizes,
        lizzie_core::settings::PanelArrangement, lizzie_core::settings::ArrangedPanel,
        lizzie_core::settings::ArrangeablePanel,
        lizzie_core::settings::TopToolbarSettings,
        lizzie_core::settings::BottomButtonSettings,
        lizzie_core::settings::BottomToolbar, lizzie_core::settings::WorkbenchPanel,
        lizzie_core::appearance::AppearanceSettings, lizzie_core::appearance::BoardStyle,
        lizzie_core::appearance::AppearanceMode,
        lizzie_core::appearance::ThemeSettings,
        lizzie_core::appearance::ThemeNodeColor,
        lizzie_core::appearance::ThemeEntry, lizzie_core::appearance::AppearanceCatalog,
        lizzie_core::appearance::ThemeImages, lizzie_core::appearance::ShippedBoard, lizzie_core::appearance::ShippedStones,
        lizzie_core::settings::CustomLayout, lizzie_core::settings::IndependentBoard,
        RegionCommand, TrackingCommand, RegionView, FocusView, TrackingUnavailable, TrackedPointView,
        lizzie_core::engine::runtime::FocusCapability, lizzie_core::settings::RegionSettings,
        lizzie_core::settings::TrackingSettings,
        lizzie_core::settings::TsumegoSettings, lizzie_core::settings::TsumegoFrameSettings,
        lizzie_core::settings::CaptureSettings, lizzie_core::tsumego::TsumegoToPlay,
        lizzie_core::tsumego::TsumegoAttacker, lizzie_core::tsumego::TsumegoKoThreat,
        lizzie_core::tsumego::capture::CapturedProblem, lizzie_core::tsumego::capture::CapturePreview,
        lizzie_core::engine::ssh::SshSettings, lizzie_core::engine::ssh::RemoteFailure,
        lizzie_core::engine::cloud::CloudStartFailure, lizzie_core::engine::diagnosis::StartupDiagnosis,
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
    out.push_str(&rule_constants());
    out.truncate(out.trim_end().len());
    out.push('\n');
    out
}

/// The core's rules presets, rule sets and engine-game rules choices, for the rules editor's 典型规则
/// buttons, the 规则面板's list and the 本局规则 picker: the UI keeps no copy of its own.
fn rule_constants() -> String {
    use lizzie_core::engine::rules::{KataRules, MATCH_RULE_OPTIONS, PRESETS, RULE_SETS};
    let json = |rules: &KataRules| serde_json::from_str::<serde_json::Value>(&rules.to_gtp_argument()).unwrap();
    let presets: serde_json::Map<String, serde_json::Value> = PRESETS
        .iter()
        .map(|p| (p.0.to_string(), json(&KataRules::preset(p.0).unwrap())))
        .collect();
    let sets: Vec<serde_json::Value> = RULE_SETS
        .iter()
        .map(|s| serde_json::json!({ "id": s.id, "labelKey": s.label_key, "komi": s.komi, "rules": json(s.rules()) }))
        .collect();
    let options: Vec<serde_json::Value> = MATCH_RULE_OPTIONS
        .iter()
        .map(|o| serde_json::json!({ "id": o.id, "rules": o.rules, "labelKey": o.label_key, "komi": o.komi() }))
        .collect();
    format!(
        "/** KataGo's official rules presets (`KataGoRules.officialPresets`), by name. */\n\
         export const KATA_RULE_PRESETS: Readonly<Record<string, Readonly<Record<string, string | boolean>>>> = {};\n\n\
         /** The rule sets in their list order (`RulePreset`): id, label key, komi and rules. */\n\
         export const RULE_SETS: ReadonlyArray<{{ readonly id: string; readonly labelKey: string; readonly komi: number; readonly rules: Readonly<Record<string, string | boolean>> }}> = {};\n\n\
         /** The engine game's standard rules choices (`MatchRuleOption`): id, the rules sent, label key and komi. */\n\
         export const MATCH_RULE_OPTIONS: ReadonlyArray<{{ readonly id: string; readonly rules: string; readonly labelKey: string; readonly komi: number | null }}> = {};\n",
        serde_json::to_string(&presets).unwrap(),
        serde_json::to_string(&sets).unwrap(),
        serde_json::to_string(&options).unwrap(),
    )
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
