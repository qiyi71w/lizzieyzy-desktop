// Generated from crates/lizzie-core (api, record, browse, settings). Do not edit.
// Regenerate: UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract

export type Stone = "Black" | "White";

export type Point = { x: number, y: number, };

export type BoardSize = { width: number, height: number, };

export type MarkKind = { "kind": "Triangle" } | { "kind": "Square" } | { "kind": "Circle" } | { "kind": "Cross" } | { "kind": "Label", "text": string };

export type Mark = { point: Point, mark: MarkKind, };

export type GameInfo = { black: string, white: string, black_rank: string, white_rank: string, komi: number, handicap: number, result: string, 
/**
 * Raw `RU` value, if any.
 */
rules: string | null, date: string, };

export type RecordFormat = "Sgf" | "Gib";

export type Language = "System" | "ZhCn" | "EnUs" | "KoKr" | "JaJp" | "ZhTw" | "ThTh";

export type WindowPlacement = { x: number, y: number, width: number, height: number, maximized: boolean, };

export type Navigation = { "op": "Forward", "count": number } | { "op": "Back", "count": number } | { "op": "First" } | { "op": "Last" } | { "op": "NextBranch" } | { "op": "PreviousBranch" } | { "op": "BranchPoint" } | { "op": "BranchStart" } | { "op": "NextComment" } | { "op": "PreviousComment" } | { "op": "MainTrunk" };

export type OpenSource = "Chooser" | "Recent" | "Drop" | "CommandLine";

export type EngineProfile = { 
/**
 * Stable identity across renames and reordering (Java assigns a UUID).
 */
id: string, name: string, 
/**
 * Command line, split like `Utils.splitCommand`.
 */
command: string, 
/**
 * GTP commands sent after start-up, separated by `;`.
 */
initialCommand: string, 
/**
 * Started in the background at launch so switching to it is immediate.
 */
preload: boolean, width: number, height: number, 
/**
 * Komi applied when the engine starts on an empty game.
 */
komi: number, isDefault: boolean, 
/**
 * Written by engine resource setup: this client's resource store owns the command, as long
 * as the command is still the one it wrote (Java `BundledKataGoProfile`).
 */
managed?: ManagedProfile, 
/**
 * KataGo search-thread policy and stored benchmark results (Java `threadPolicy`).
 */
threadPolicy?: ThreadPolicy, 
/**
 * Settings chosen through the engine's GTP configuration protocol (Java
 * `gtpConfigurationProtocol` / `gtpConfigurationProfile`).
 */
gtpConfiguration?: GtpConfiguration, 
/**
 * 远程引擎 (`useJavaSSH`): the command runs on this SSH host.
 */
ssh?: SshSettings, 
/**
 * Cloud launch parameters selected in the remote compute center; credentials stay in the
 * account adapter. The command is only the provider marker, never a bearer token.
 */
cloud?: CloudLaunch, };

export type EngineStartup = "Default" | "Last" | "Empty" | "Manual";

export type AnalysisSettings = { 
/**
 * `analyze-update-interval-centisec` [10].
 */
intervalCentisec: number, 
/**
 * `analyze-update-interval-centisecssh` [10]: the interval of engines that run over SSH
 * (`Leelaz.getInterval`).
 */
intervalCentisecSsh: number, 
/**
 * `limit-time` [true] with `max-analyze-time-seconds` [600].
 */
limitTime: boolean, maxAnalyzeTimeSeconds: number, 
/**
 * `limit-playout` [false] with `limit-playouts` [100000].
 */
limitPlayout: boolean, limitPlayouts: number, 
/**
 * `limit-max-suggestion` [10]; 0 shows every candidate.
 */
limitMaxSuggestion: number, 
/**
 * `limit-branch-length` [15]; 0 shows the whole variation.
 */
limitBranchLength: number, 
/**
 * `fast-engine-change` [true]: a switched-away engine keeps running in the background.
 */
fastEngineChange: boolean, 
/**
 * `show-ponder-limited-tips` [true]: explain once when a limit stops the analysis.
 */
showPonderLimitedTips: boolean, 
/**
 * `ui.auto-check-engine-alive` [true]: a foreground engine that ends unexpectedly is
 * started again and brought back to the current position (`EngineManager.checkEngineAlive`).
 */
autoCheckEngineAlive: boolean, 
/**
 * `leelaz.enable-lizzie-cache` [true]: a node's stored result gives way only to a stronger
 * one (more visits, or marked to give way); off, every new result replaces it.
 */
enableLizzieCache: boolean, 
/**
 * `leelaz.stop-at-empty-board` [false]: an analysis of the empty board stops at its first
 * result, without the limit notice (`Leelaz.java:7458-7468`).
 */
stopAtEmptyBoard: boolean, 
/**
 * `ui.always-gtp` [true]: the GTP console records while it is hidden; off, a hidden console
 * records only engines that are still starting.
 */
alwaysGtp: boolean, };

export type EngineScope = "Current" | "Others" | "All";

export type AnalysisScope = "All" | "Current";

export type Command = { "type": "CloudAccount", op: CloudCommand, owner: number, } | { "type": "CloudActivate", settings: CloudSettings, confirmed: boolean, } | { "type": "CloudLocal" } | { "type": "CloudCancel" } | { "type": "CloudCancelRequest", owner: number, } | { "type": "CloudImportQr" } | { "type": "OpenCloudLink", custom: boolean, } | { "type": "Fox", op: FoxCommand, } | { "type": "Online", op: OnlineCommand, } | { "type": "Strength", op: StrengthCommand, } | { "type": "Commentary", op: CommentaryCommand, } | { "type": "ReadBoard", op: ReadBoardCommand, } | { "type": "CommentaryConnection", op: ConnectionCommand, } | { "type": "FoxKifu", op: FoxKifuCommand, } | { "type": "TencentKifu", op: TencentKifuCommand, } | { "type": "WebBoard", op: WebBoardCommand, } | { "type": "Update", op: UpdateCommand, } | { "type": "SshCredential", op: SshCredentialCommand, } | { "type": "Contribution", op: ContributionCommand, } | { "type": "SetAppearanceSettings", settings: AppearanceSettings, theme: ThemeSettings, } | { "type": "SetFrameFontSize", size: number, } | { "type": "AddTheme", name: string, } | { "type": "DeleteTheme", name: string, } | { "type": "ChooseAndOpenRecord" } | { "type": "OpenRecord", path: string, source: OpenSource, } | { "type": "PasteRecord", confirmed: boolean, } | { "type": "SaveRecord", confirmed: number | null, suggested_name: string, } | { "type": "ChooseAndSaveRecord", suggested_name: string, } | { "type": "SaveRecordTo", path: string, } | { "type": "SetReplaceFileHint", enabled: boolean, } | { "type": "ExportRecord", kind: ExportKind, suggested_name: string, labels: CommentLabels, } | { "type": "ExportRecordTo", kind: ExportKind, path: string, labels: CommentLabels, } | { "type": "CopyRecord" } | { "type": "ExportImage", png: string, } | { "type": "ExportImageTo", path: string, png: string, } | { "type": "CopyImage", png: string, } | { "type": "Recovery", op: RecoveryCommand, } | { "type": "Navigate", nav: Navigation, } | { "type": "GotoNode", node: number, } | { "type": "GotoMoveNumber", number: number, } | { "type": "SetReadKomi", enabled: boolean, } | { "type": "SetAppendWinrateToComment", enabled: boolean, } | { "type": "SetLanguage", language: Language, } | { "type": "SetPlaySound", enabled: boolean, } | { "type": "ClearPersonalData" } | { "type": "ResetUiPosition" } | { "type": "OpenLogFolder" } | { "type": "ExportDiagnostics" } | { "type": "ToggleAnalysis" } | { "type": "HoldAnalysis", held: boolean, } | { "type": "ClearAnalysisCache", scope: AnalysisScope, } | { "type": "ClearAnalysisInfo", scope: AnalysisScope, } | { "type": "RefreshCurrentMove" } | { "type": "SwitchEngine", index: number, } | { "type": "RestartEngine" } | { "type": "ShutdownEngines", scope: EngineScope, } | { "type": "SetExtraMode", mode: ExtraMode, } | { "type": "SetClassicMode" } | { "type": "ToggleIndependentBoard", board: IndependentBoard, } | { "type": "CloseIndependentBoard", board: IndependentBoard, } | { "type": "CommitLayout", mode: ExtraMode, panels: PanelSettings, board_position_proportion: number, save_slot: number | null, window: WindowPlacement | null, } | { "type": "ApplyCustomLayout", slot: number, } | { "type": "SwitchSecondaryEngine", index: number, } | { "type": "RestartSecondaryEngine" } | { "type": "ShutdownSecondaryEngine" } | { "type": "SaveEngineSettings", engines: Array<EngineProfile>, startup: EngineStartup, } | { "type": "SetAnalysisSettings", settings: AnalysisSettings, } | { "type": "SaveInitSettings", display: DisplaySettings, analysis: AnalysisSettings, language: Language, } | { "type": "SetKomi", komi: number, } | { "type": "SetKataRules", rules: string, auto_load: boolean, } | { "type": "SetKataParam", param: KataParam, enabled: boolean, value: number, } | { "type": "SaveKataEngineParams", params: KataEngineParamsEdit, } | { "type": "SearchThreads", op: SearchThreadsCommand, } | { "type": "SaveLeelaEngineOptions", options: LeelaOptionSettings, } | { "type": "SaveCustomPda", settings: CustomPdaSettings, } | { "type": "SetPriorityPoint", point: Point, } | { "type": "ClearPriorityPoints" } | { "type": "SendGtpCommand", command: string, } | { "type": "SetConsoleVisible", visible: boolean, } | { "type": "SetFullTrace", enabled: boolean, scopes?: Array<TraceScope>, } | { "type": "SetLogging", settings: LoggingSettings, } | { "type": "SetQuickLinks", settings: QuickLinkSettings, } | { "type": "LaunchQuickLink", index: number, } | { "type": "ChooseQuickLinkProgram", title: string, filter_name: string, } | { "type": "SetPonderLimitTips", enabled: boolean, } | { "type": "SetAnalyzeSides", black: boolean, white: boolean, } | { "type": "AnswerRulesPrompt", prompt: number, accept: boolean, } | { "type": "ChooseJavaFolder", program: boolean, } | { "type": "PreviewJavaImport", config_dir: string, program_dir: string | null, } | { "type": "ApplyJavaImport", preview: number, copy_credentials: Array<number>, 
/**
 * Separate, unchecked-by-default consent for Java's commentary API key.
 */
copy_commentary_key: boolean, 
/**
 * Separate, unchecked-by-default consent for the password of 精析设置's 远程引擎.
 */
copy_analysis_ssh_password: boolean, 
/**
 * Independent consent; neither account secret follows engine selection.
 */
copy_cloud_token: boolean, copy_cloud_password: boolean, 
/**
 * Separate, unchecked-by-default consent for the 跑谱贡献 account, its password, a custom
 * command's passwords and its 远程SSH password.
 */
copy_contribution_credentials: boolean, 
/**
 * Separate, unchecked-by-default consent for 快速启动 commands that carry credentials.
 */
copy_quick_link_credentials: boolean, replace: boolean, recovery: RecoveryImport | null, } | { "type": "DismissJavaImportOffer" } | { "type": "Resource", op: ResourceCommand, } | { "type": "Tuning", op: TuningCommand, } | { "type": "Edit", op: EditCommand, } | { "type": "SetEditSettings", settings: EditSettings, } | { "type": "ClickReview", point: Point, } | { "type": "SetDisplaySettings", settings: DisplaySettings, } | { "type": "TogglePanel", panel: WorkbenchPanel, } | { "type": "SetPanelSettings", settings: PanelSettings, } | { "type": "SetPanelSizes", sizes: PanelSizes, } | { "type": "NudgeBoardPosition", delta: number, } | { "type": "RestoreDefaultPanelSizes" } | { "type": "SetPanelArrangement", arrangement: PanelArrangement | null, } | { "type": "SetBottomToolbar", mode: BottomToolbar, } | { "type": "ToggleRawNet" } | { "type": "StopRawNet" } | { "type": "SetKataEstimate", mode: KataEstimateMode, } | { "type": "ToggleKataEstimate" } | { "type": "Score", op: ScoreCommand, } | { "type": "SetAutoQuickAnalyze", enabled: boolean, } | { "type": "SetLoadSgfLast", enabled: boolean, } | { "type": "SaveAnalysisEngineSettings", settings: AnalysisEngineSettings, shown_command: string, edited: boolean, } | { "type": "GenerateAnalysisCommand", index: number | null, } | { "type": "SetQuickModelEnabled", enabled: boolean, } | { "type": "SetProblemList", settings: ProblemListSettings, } | { "type": "JumpToProblemMove", node: number, } | { "type": "StartFlashAnalysis", target: FlashTarget, } | { "type": "CancelFlashAnalysis" } | { "type": "RedoFlashAnalysis" } | { "type": "Region", op: RegionCommand, } | { "type": "SetRegionSettings", settings: RegionSettings, } | { "type": "Tracking", op: TrackingCommand, } | { "type": "SetTrackingSettings", settings: TrackingSettings, } | { "type": "StartAutoAnalysis", settings: AutoAnalysisSettings, } | { "type": "StopAutoAnalysis" } | { "type": "AnswerAutoAnalysisDiff", accept: boolean, } | { "type": "SaveAutoAnalysisSettings", settings: AutoAnalysisSettings, } | { "type": "DismissAutoAnalysisTip", never_again: boolean, } | { "type": "StartWholeGameAnalysis", visits: number, } | { "type": "PauseWholeGameAnalysis" } | { "type": "ResumeWholeGameAnalysis" } | { "type": "StopWholeGameAnalysis" } | { "type": "EstimateWholeGameAnalysis", visits: number, } | { "type": "Hawkeye", op: HawkeyeCommand, } | { "type": "SetHawkeyeSettings", settings: HawkeyeSettings, } | { "type": "ExportTable", rows: Array<Array<string>>, } | { "type": "Batch", op: BatchCommand, } | { "type": "Game", op: GameCommand, } | { "type": "Match", op: MatchCommand, } | { "type": "HumanSl", op: HumanSlCommand, } | { "type": "SetCaptureSettings", settings: CaptureSettings, } | { "type": "SetTsumegoButtonShown", shown: boolean, } | { "type": "SetTopToolbarSettings", settings: TopToolbarSettings, } | { "type": "SetBottomButtons", settings: BottomButtonSettings, };

export type CommandResult = { "type": "Done" } | { "type": "Cloud", reply: CloudReply, } | { "type": "CloudError", failure: CloudFailure, } | { "type": "CloudUrl", url: string, } | { "type": "CloudSwitch", profile: string, after: number, } | { "type": "Strength", report: StrengthReport, } | { "type": "CommentaryModels", models: Array<string>, } | { "type": "SshPassword", state: SshPasswordState, } | { "type": "ContributionPassword", state: SshPasswordState, } | { "type": "ContributionSavedEngines", candidates: Array<SavedEngineCandidate>, } | { "type": "ContributionQualification", qualification: ContributionQualification, } | { "type": "StrengthImportFailed", detail: string, } | { "type": "WholeGameEstimate", millis: number | null, } | { "type": "Unchanged" } | { "type": "OpenStarted", ticket: number, } | { "type": "NeedsConfirmation" } | { "type": "NeedsEditConfirmation", target: EditTarget, } | { "type": "NeedsBatchStop" } | { "type": "Exported", path: string, } | { "type": "ExportFailed", path: string, detail: string, } | { "type": "Refused", message_key: string, } | { "type": "SaveStarted", ticket: number, } | { "type": "SearchThreads", ticket: number, } | { "type": "NeedsReplaceConfirmation", document: number, file_name: string, } | { "type": "NeedsOverwriteConfirmation", path: string, } | { "type": "NeedsExportOverwriteConfirmation", kind: ExportKind, path: string, } | { "type": "NeedsImageOverwriteConfirmation", path: string, } | { "type": "FolderChosen", path: string, } | { "type": "FileChosen", path: string, } | { "type": "QuickLinkProgramChosen", program: QuickLinkProgram, } | { "type": "JavaImportPreview", preview: ImportPreviewView, } | { "type": "JavaImportFailed", failure: ImportFailure, } | { "type": "JavaImported", report: ImportReportView, } | { "type": "Resource", result: ResourceResult, } | { "type": "Tuning", result: TuningResult, } | { "type": "AnalysisCommand", command: string, generated_config: string | null, } | { "type": "TsumegoFramed", no_room: Array<Stone>, } | { "type": "ClickReview", review: ClickReviewView | null, };

export type ErrorKind = "InvalidArgument" | "Io" | "Unavailable";

export type ApiError = { kind: ErrorKind, message: string, };

export type DocumentView = { 
/**
 * Identity of the open document; changes on every adopted record.
 */
id: number, format: RecordFormat | null, path: string | null, file_name: string | null, 
/**
 * The file name shown when the record has no file of its own: a 存档与读档 slot it was
 * loaded from (Java shows `game2.sgf` in the title; saving goes through the chooser).
 */
origin_name: string | null, size: BoardSize, info: GameInfo, 
/**
 * Content version; every content change increases it.
 */
revision: number, 
/**
 * The revision known to be the content of the file at `path` (read from it or confirmed
 * written to it); `None` while the content was never on disk (new or pasted record).
 */
saved_revision: number | null, 
/**
 * The current content is not confirmed on disk: `saved_revision != Some(revision)`.
 */
unsaved: boolean, };

export type TreeNodeKind = "Root" | "Snapshot" | "Black" | "White";

export type TreeNodeView = { id: number, parent: number | null, 
/**
 * Children in order, without dummy placeholders.
 */
children: Array<number>, kind: TreeNodeKind, pass: boolean, move_number: number, 
/**
 * Display numbering, including branch restarts, SGF MN and passes; 0 for no number.
 */
number: number, has_comment: boolean, };

export type TreeView = { document: number, nodes: Array<TreeNodeView>, };

export type LastAction = { color: Stone, 
/**
 * `None` for a pass.
 */
point: Point | null, };

export type PositionView = { document: number, node: number, 
/**
 * Row-major, `size.width * size.height`.
 */
stones: Array<Stone | null>, last_action: LastAction | null, snapshot: boolean, move_number: number, to_play: Stone, black_captures: number, white_captures: number, marks: Array<Mark>, comment: string, 
/**
 * Display text only; the comment editor continues using `comment` verbatim.
 */
user_comment: string, ai_commentary: string | null, 
/**
 * Root to current node.
 */
path: Array<number>, 
/**
 * End of the current line through first children (Java `history.getEnd()`).
 */
line_end: number, main_trunk: boolean, 
/**
 * Points of the move children, first child first (Java `drawNextMoves`: passes and dummies
 * are not shown).
 */
next_moves: Array<Point>, 
/**
 * Colour of the first move child; the next-move rings use it.
 */
next_color: Stone | null, };

export type OpenOutcome = { "type": "Adopted", ticket: number, document: number, 
/**
 * The record holds secondary-engine analysis (`LZ2`/`LZOP2`) and 双引擎模式 is off: ask
 * whether to switch to it (`SGFParse.doubleEngineHint`), then send `SetExtraMode`.
 */
double_engine_hint: boolean, } | { "type": "Failed", ticket: number, message_key: string, detail: string, };

export type SaveOutcome = { "type": "Saved", ticket: number, document: number, revision: number, path: string, 
/**
 * The write was an export; it does not make the record saved.
 */
export: ExportKind | null, } | { "type": "Failed", ticket: number, document: number, path: string, message_key: string, detail: string, };

export type PendingOpen = { ticket: number, path: string | null, };

export type EngineSettingsView = { engines: Array<EngineProfile>, startup: EngineStartup, analysis: AnalysisSettings, 
/**
 * `kata-rules` as saved (JSON or preset name; empty = the engine's own).
 */
kata_rules: string, auto_load_kata_rules: boolean, 
/**
 * Settings > Advanced: run the official benchmark after start-up (`ui.enable-startup-benchmark`).
 */
startup_benchmark: boolean, 
/**
 * The toolbar's 激进度 (`ui.txt-kata-engine-pda`, `ui.chk-kata-engine-pda`).
 */
kata_pda: number | null, kata_pda_enabled: boolean, kata_params: KataParamSettings, 
/**
 * The Leela Zero / Sai 引擎参数 window's rows.
 */
leela_options: LeelaOptionSettings, 
/**
 * KataGoPda 激进参数PDA设置 (`SetKataPDA`): mode, value and 自动套用.
 */
custom_pda: CustomPdaSettings, };

export type SettingsView = { cloud: CloudSettings, language: Language, 
/**
 * Language the UI is displaying in this session (resolved at start).
 */
active_language: Language, 
/**
 * Whether the saved language or frame font size differs from this session until restart.
 */
restart_required: boolean, read_komi: boolean, 
/**
 * 打开后跳到最后一手 (`ui.load-sgf-last`).
 */
load_sgf_last: boolean, 
/**
 * 打开棋谱后自动快速分析 (`ui.auto-quick-analyze-on-load`).
 */
auto_quick_analyze_on_load: boolean, append_winrate_to_comment: boolean, 
/**
 * Settings > "播放落子音效" (Java `ui.play-sound`, default on).
 */
play_sound: boolean, 
/**
 * Ask before "保存" overwrites the record's own file (Java `ui.show-replace-file-hint`).
 */
replace_file_hint: boolean, 
/**
 * 布局模式 (Java `ui.extra-mode`).
 */
extra_mode: ExtraMode, panels: PanelSettings, 
/**
 * The top toolbar's groups (自定义内容) and 自动换行.
 */
top_toolbar: TopToolbarSettings, panel_sizes: PanelSizes, 
/**
 * The committed 排列, or none for the Java layout.
 */
panel_arrangement: PanelArrangement | null, bottom_toolbar: BottomToolbar, 
/**
 * The bottom toolbar's buttons (自定义内容).
 */
bottom_buttons: BottomButtonSettings, comment_panel_auto_hidden: boolean, custom_layouts: [CustomLayout | null, CustomLayout | null], independent_main_board: WindowPlacement | null, independent_sub_board: WindowPlacement | null, engine: EngineSettingsView, 
/**
 * 诊断与日志's saved switches and 完整日志 scope preference.
 */
logging: LoggingSettings, 
/**
 * 显示快速启动 and the 快速启动 program list.
 */
quick_links: QuickLinkSettings, edit: EditSettings, display: DisplaySettings, 
/**
 * 精析设置 as saved.
 */
analysis_engine: AnalysisEngineSettings, 
/**
 * 快速曲线轻量模型 on the weights page.
 */
quick_model: QuickModelView, human_sl: HumanSlSettingsView, 
/**
 * 评论/问题手 panel state.
 */
problems: ProblemListSettings, 
/**
 * 局部精析's move range (`analysis-start-move`, `analysis-end-move`).
 */
flash_range: FlashRange, auto_analysis: AutoAnalysisSettings, whole_game: WholeGameSettings, 
/**
 * 超级鹰眼's remembered choices.
 */
hawkeye: HawkeyeSettings, appearance: AppearanceSettings, theme: ThemeSettings, 
/**
 * Increases whenever theme folders are rewritten in place (a Java import replacing them):
 * the same theme values may then name different image files' contents.
 */
theme_files_revision: number, 
/**
 * Frame size in use until the next start (LizzieFrame.java:20766-20768).
 */
active_frame_font_size: number, 
/**
 * 强制计算区域's depth choices.
 */
region: RegionSettings, 
/**
 * 选点重点研究.
 */
tracking: TrackingSettings, 
/**
 * 人机对弈 dialog settings.
 */
game: GameSettings, 
/**
 * 自动落子 (a 分析模式 game's or 棋盘同步工具's).
 */
autoplay: AutoPlayView, matches: MatchSettingsView, 
/**
 * 死活题 dialog choices, 抓取死活题 参数设置 and the 死活 toolbar button.
 */
tsumego: TsumegoSettings, 
/**
 * 跑谱贡献 settings and the contribution window's choices.
 */
contribution: ContributionSettings, };

export type StoneSound = "Stone" | "DeadStone" | "DeadStoneMore";

export type EngineState = "Loading" | "Ready";

export type EngineEntryView = { name: string, command: string, 
/**
 * `None` when not running.
 */
state: EngineState | null, };

export type ForegroundEngineView = { 
/**
 * Position in the engine list.
 */
index: number, 
/**
 * The configured name, else the GTP name.
 */
name: string, gtp_name: string, version: string, katago: boolean, 
/**
 * Leela Zero or Sai: its 引擎参数 window is the Leela one (`lz-setoption`).
 */
leela_sai: boolean, 
/**
 * Speaks `kata-analyze` / `lz-analyze`.
 */
can_analyse: boolean, 
/**
 * Identified and synced to the current position: the switch is complete.
 */
ready: boolean, 
/**
 * `list_commands` (GtpConsolePane 常用命令).
 */
commands: Array<string>, 
/**
 * This engine process loads an official B11 model (Java `Leelaz.usesB11ForSpeedNotice`):
 * the title adds `B11SpeedNotice.title` after the speed. False until its model is known.
 */
b11: boolean, };

export type EngineView = { engines: Array<EngineEntryView>, foreground: ForegroundEngineView | null, 
/**
 * The 双引擎模式 secondary engine, while one is chosen.
 */
secondary: SecondaryEngineView | null, 
/**
 * The user paused analysis (Space); not remembered across restarts.
 */
paused: boolean, 
/**
 * The foreground engine is analysing the current node (Java `isPondering`).
 */
analysing: boolean, 
/**
 * An analysis limit stopped the analysis of the current node.
 */
stopped_by_limit: boolean, 
/**
 * 分析 黑 / 白: ordinary analysis goes on for these sides (session only, both at start).
 */
analyze_black: boolean, analyze_white: boolean, 
/**
 * Captured-panel rules caption key (`KataGoRules.Summary`), when KataGo reported its rules.
 */
rules_key: string | null, 
/**
 * Rules the foreground KataGo reported (JSON), for the rules dialog.
 */
rules: string | null, 
/**
 * Last console line; the console fetches newer lines.
 */
console_seq: number, 
/**
 * 复用当前前台引擎 target status (`AnalysisSettings.reuseStatus`).
 */
lease: LeaseAvailabilityView, 
/**
 * The foreground engine was not given back after it was borrowed: it does not analyse until
 * it is restarted (`LizzieFrame.display.foregroundUnrestored`).
 */
unrestored: UnrestoredReason | null, 
/**
 * The foreground engine failed and nothing took its seat since (`LizzieFrame.display.down`,
 * Java `isDownWithError`).
 */
down: boolean, 
/**
 * The foreground KataGo's PDA and WRN (toolbar 激进度 / 广度).
 */
kata_params: KataParamsView, 
/**
 * The foreground KataGo process's 搜索线程数, while one is ready.
 */
search_threads: SearchThreadsView | null, 
/**
 * The latest 搜索线程数 operation (it outlives the process a restore replaces).
 */
search_threads_operation: SearchThreadsOperationView | null, 
/**
 * The foreground KataGoPda's current PDA and cap and the saved dialog mode.
 */
custom_pda: CustomPdaView, 
/**
 * Points recorded with 优先分析此点 for the current position; a recorded point stays
 * visible beyond 最大候选点数. Session only.
 */
priority_points: Array<Point>, };

export type CandidateView = { 
/**
 * `None` for a pass.
 */
point: Point | null, visits: number, 
/**
 * Root allocation (`edgeVisits` when reported, else `visits`).
 */
allocation_visits: number, winrate: number, score_lead: number | null, score_stdev: number | null, prior: number, lcb: number | null, order: number, 
/**
 * Principal variation; `None` entries are passes.
 */
pv: Array<Point | null>, 
/**
 * KataGo `pvVisits`, one count per reported variation move; empty when absent.
 */
pv_visits: Array<number>, 
/**
 * For each `pv` move, the stones it captures when played out from the node's position
 * (`Branch`; none under no-capture); the board removes them with `remove-dead-in-variation`.
 */
pv_captures: Array<Array<Point>>, 
/**
 * Win rate and score lead as the board and the candidate table show them (总是显示黑胜率,
 * 目差显示); set by the view projection.
 */
shown: ShownValues, 
/**
 * The side to move's numbers with 盘面 applied, ignoring 总是显示黑胜率 (独立选点列表).
 */
side_to_move: ShownValues, 
/**
 * KataGo `movesOwnership` after this move, Black-positive, row-major from the top-left
 * (按选点显示评估); absent unless requested and reported.
 */
ownership: Array<number> | null, 
/**
 * Kept among the candidates beyond 最大候选点数 (`MoveData.lastTimeUnlimited`,
 * BoardData.tryToLimitMoves): it was within the limit at the node's previous result and is
 * still searched (more visits, or within 3 seconds).
 */
kept: boolean, };

export type NodeAnalysisView = { node: number, engine: string, to_play: Stone, 
/**
 * Total visits of the candidates (`playouts`).
 */
visits: number, root_visits: number | null, 
/**
 * The best candidate's win rate, side to move.
 */
winrate: number, 
/**
 * The best candidate's score lead, side to move (KataGo).
 */
score_lead: number | null, score_stdev: number | null, komi: number, 
/**
 * In `order`.
 */
candidates: Array<CandidateView>, 
/**
 * Black's numbers as the statistics panel and the title show them (the lead with the komi
 * under 盘面); set by the view projection.
 */
black: ShownValues, };

export type HistoryPoint = { node: number, move_number: number, to_play: Stone, visits: number, 
/**
 * Side to move, 0–100.
 */
winrate: number, score_lead: number | null, 
/**
 * Black's numbers as the win-rate graph shows them (the lead with the komi under 盘面).
 */
black: ShownValues, 
/**
 * This position's result as a candidate of the previous position (the candidate table's
 * actual-move row).
 */
as_candidate: AsCandidate, 
/**
 * The best candidate's score uncertainty (KataGo), for the actual-move row's 目数标准差.
 */
score_stdev: number | null, };

export type EngineFailureView = { engine_name: string, 
/**
 * Launch command with credentials masked.
 */
command: string, during_startup: boolean, message: string, 
/**
 * Last lines the engine printed.
 */
output: Array<string>, 
/**
 * Missing store-owned NVIDIA runtime components, if this managed profile can be repaired.
 */
repair?: ResourceRepairView, 
/**
 * A remote engine's connection failure or end (`SSHController` messages).
 */
remote?: RemoteFailure, 
/**
 * Why a 智子 engine never became ready (Java `ZhiziGtpTransport` startup failures).
 */
cloud?: CloudStartFailure, 
/**
 * The configured engine that failed (empty for the analysis engine), so the dialog can
 * start it again (Java opens 更多引擎 after a remote login failure).
 */
profile_id: string, 
/**
 * It was started for or held the 双引擎模式 secondary seat: starting it again (信任新密钥并重试)
 * takes that seat, not the foreground one.
 */
secondary: boolean, };

export type EngineNotice = "StoppedByLimit" | "SetupModeActive" | "SameEngine";

export type ConsoleLineKind = "Sent" | "Output" | "Error";

export type ConsoleLineView = { seq: number, engine: string, kind: ConsoleLineKind, text: string, };

export type RulesPromptView = { 
/**
 * Answer with [`Command::AnswerRulesPrompt`].
 */
prompt: number, 
/**
 * The declared rules (the SGF `RU` text, or the chosen rules' fields).
 */
target: string, 
/**
 * The engine's reported rules (`fieldSummary`), if any.
 */
observed: string | null, 
/**
 * `SessionRulesSynchronizer.Failure` name.
 */
failure: string, };

export type WorkbenchView = { 
/**
 * Increases with every published view.
 */
seq: number, document: DocumentView, 
/**
 * Present when the document changed since the previous view, and in full snapshots.
 */
tree: TreeView | null, position: PositionView, pending_open: PendingOpen | null, 
/**
 * The latest finished open, once.
 */
open_outcome: OpenOutcome | null, 
/**
 * The latest finished save, once.
 */
save_outcome: SaveOutcome | null, 
/**
 * Sounds of the browsing step that produced this view, one per node entered forward, in
 * order (Java plays one per `moveHistoryForward`); handed over once, empty while sound is off.
 */
sounds: Array<StoneSound>, settings: SettingsView, 
/**
 * Newest first.
 */
recent_files: Array<string>, 
/**
 * Current session komi (toolbar komi field).
 */
komi: number, engine: EngineView, 
/**
 * The scopes of the running 完整日志 session; empty while none runs.
 */
full_trace: Array<TraceScope>, 
/**
 * Automatic quick analysis of the open record.
 */
quick: QuickAnalysisView, 
/**
 * Manual flash analysis in progress.
 */
flash: FlashAnalysisView | null, auto_analysis: AutoAnalysisView, 
/**
 * 批量分析 / 批量精析 and their 进度表.
 */
batch: BatchView, 
/**
 * How a flash analysis ended, once.
 */
flash_outcome: FlashOutcome | null, 
/**
 * 强制计算区域 of this session.
 */
region: RegionView, 
/**
 * 选点重点研究 of the current position.
 */
focus: FocusView, 
/**
 * Session projection, retained after completion/cancel/failure.
 */
whole_game: WholeGameAnalysisView, 
/**
 * 问题手 list and its progress badge.
 */
problems: ProblemListView, 
/**
 * Stored analysis of the current node (whether or not the engine is analysing it now).
 */
analysis: NodeAnalysisView | null, 
/**
 * Move numbers, 落子评价 and 下一手 详细 of the current position, as the display settings
 * choose them.
 */
board: BoardOverlayView, 
/**
 * Foreground engine's pure-network result, never from another position or instance.
 */
rawNet: RawNetView | null, 
/**
 * Stored results along the current line, its continuation and the main trunk, root first
 * (the win-rate graph's data).
 */
history: Array<HistoryPoint>, 
/**
 * Coloured analysed tree nodes, including branches outside the graph's current line.
 */
theme_nodes: Array<ThemeNodeColor>, 
/**
 * Each move's result against the analysis of the position it was played in, and the AI
 * match score (统计 panel, captured panel, 超级鹰眼).
 */
statistics: StatisticsView, 
/**
 * 超级鹰眼 while its window is open (computed only then).
 */
hawkeye: HawkeyeView | null, 
/**
 * Kata形势显示 and the ownership it draws.
 */
kata_estimate: KataEstimateView, 
/**
 * 终局点目, while scoring.
 */
score: ScoreView | null, 
/**
 * 双引擎模式's right board: the secondary engine's results for the current node. Present
 * only in 双引擎模式.
 */
secondary: SecondaryView | null, 
/**
 * 记录选点信息到评论中: the current comment as Java shows it while an engine analyses,
 * with the analysis summary appended (Java writes it into the node, `appendComment`);
 * absent when the comment pane shows `position.comment` unchanged.
 */
comment_with_analysis: string | null, 
/**
 * Engines that failed to start or stopped unexpectedly since the last view, oldest first,
 * each once: engine output between two views is coalesced into one, and every failure keeps
 * its own window (Java shows an EngineFailedMessage per failure).
 */
engine_failures: Array<EngineFailureView>, 
/**
 * A notice about analysis, once.
 */
engine_notice: EngineNotice | null, 
/**
 * The running 人机对弈 (also while it gives the engine back).
 */
game: GameView | null, fox: FoxView, 
/**
 * How the last game ended, once.
 */
game_outcome: GameOutcomeView | null, 
/**
 * 人机对弈 messages, once each, in order.
 */
game_notices: Array<GameNotice>, engine_match: MatchView | null, 
/**
 * The open document is an engine game (Java `Board.isPkBoard` with `isPkBoardKataB` /
 * `isPkBoardKataW`; `WinrateGraph.isEngineOrPkGraphMode`): always while a match exists, its
 * score side from the match's engines; else a document a match game was played on
 * (`EngineManager` 2918, kata side 15801-15805), a 跑谱贡献 rating game being watched
 * (`ContributeEngine` 965-971) or a record whose root `DZ` is `Y` / `KB` / `KW`
 * (`SGFParser` 775-791, also after 转换为初始局面, `Board` 1320), until 清除Lizzie缓存(全局),
 * the start of 自动分析 or a fully answered flash analysis, 自动棋谱快析 attempt or 整盘精析
 * pass ends it (`clearBoardStat` / `clearPkBoardStat` in `AnalysisEngine.setResult`). Saves
 * write its `DZ` (`SGFParser` 1443-1462). `None` for any other
 * document.
 */
pk_board: PkBoardView | null, match_outcome: MatchOutcomeView | null, match_notices: Array<MatchNotice>, match_winrate_image: MatchWinrateImageRequest | null, human_sl: HumanSlView | null, strength: StrengthView, 
/**
 * 在线棋谱 / 弈客直播.
 */
online: OnlineView, commentary: CommentaryView, 
/**
 * 野狐棋谱 window.
 */
fox_kifu: FoxKifuView, 
/**
 * 腾讯棋谱 window.
 */
tencent_kifu: TencentKifuView, 
/**
 * Web 旁观 and a viewer's 试下.
 */
web_board: WebBoardView, 
/**
 * 检查更新.
 */
update: UpdateView, human_sl_notices: Array<HumanSlNotice>, 
/**
 * 跑谱贡献 session, until its window is closed after it ended.
 */
contribution: ContributionView | null, 
/**
 * 跑谱贡献 messages, once each, in order.
 */
contribution_notices: Array<ContributionNotice>, human_sl_continuation: HumanSlContinuation | null, 
/**
 * 读秒 countdown voices to play (`sound/<n>.wav`), once each.
 */
byoyomi_voices: Array<number>, 
/**
 * The session rules could not be applied to the foreground engine; ask whether to analyse
 * with the engine's current rules (`KifuRules.failurePrompt`), once.
 */
rules_prompt: RulesPromptView | null, java_import: JavaImportView, 
/**
 * Engine resources (auto setup); present when changed since the previous view, and in full
 * snapshots.
 */
resources: ResourcesView | null, 
/**
 * Engine performance (thread policy, benchmark); present when changed since the previous
 * view, and in full snapshots.
 */
tuning: TuningView | null, editing: EditingView, 
/**
 * 存档与读档.
 */
recovery: RecoveryView, 
/**
 * 棋盘同步工具.
 */
readboard: ReadBoardView, };

export type LogsStatus = { directory: string, app_log_bytes: number | null, crash_log_bytes: number | null, };

export type JavaImportView = { 
/**
 * First use (this product has no settings file yet): offer the import.
 */
offer: boolean, 
/**
 * Java configuration folders found in Java's conventional per-user / shared locations.
 */
candidates: Array<string>, };

export type ImportStatus = "Imported" | "Incompatible" | "NotMigrated";

export type ImportReason = "NotYetAvailable" | "Credential" | "InvalidValue" | "WindowCoordinates" | "CredentialNotConfirmed" | "SessionOnly" | "Unreadable" | "NotChosen" | "Obsolete" | "JavaState";

export type ImportTarget = "Appearance" | "Themes" | "Commentary" | "Cloud" | "ReadKomi" | "LoadSgfLast" | "ReplaceFileHint" | "AppendWinrateToComment" | "Language" | "PlaySound" | "NoSoundInSync" | "ReadBoardStatSync" | "ReadBoardGetFocus" | "ReadBoardPonderingNotice" | "Diagnostics" | "RecentFiles" | "LastFolder" | "Engines" | "EngineStartup" | "LastEngine" | "AnalysisInterval" | "LimitTime" | "MaxAnalyzeTime" | "LimitPlayout" | "LimitPlayouts" | "LimitMaxSuggestion" | "LimitBranchLength" | "FastEngineChange" | "PonderLimitTips" | "AutoCheckEngineAlive" | "LizzieCache" | "StopAtEmptyBoard" | "AlwaysGtp" | "KataRules" | "AutoLoadKataRules" | "KataPda" | "KataWrn" | "KataParamAutoLoad" | "KataParamToolbar" | "LeelaOptions" | "CustomPda" | "StartupBenchmark" | "NetworkProxy" | "WebBoard" | "AllowDoubleClick" | "LiveAlwaysGotoLast" | "LiveOpenHtml" | "AllowDrag" | "RightClickMenu" | "NewBoardHint" | "MarkupTools" | "OtherBoardSize" | "NoCapture" | "MoveRect" | "ClickReview" | "PaintingColor" | "AutoSaveOnExit" | "ResumePreviousGame" | "ZoomThumbnails" | "LastImageFolder" | "WinrateAlwaysBlack" | "ScoreLeadWithKomi" | "Coordinates" | "NamesOnBoard" | "MoveNumbers" | "VariationTree" | "MoveRank" | "Suggestions" | "NextMoves" | "SubBoard" | "PureNetwork" | "WinrateGraph" | "KataEstimate" | "Scoring" | "MatchAi" | "SuggestionsFrame" | "ExtraMode" | "WorkbenchPanels" | "PanelSizes" | "BottomToolbar" | "QuickLinks" | "Update" | "AutoQuickAnalysis" | "AnalysisEngine" | "QuickModel" | "HumanSlTraining" | "HumanSlModel" | "StrengthBackground" | "StrengthModels" | "Tsumego" | "CaptureTsumego" | "TopToolbar" | "BottomButtons" | "ProblemList" | "FlashRange" | "AutoAnalysis" | "HumanEngineGame" | "GameAiTime" | "GameRandomStart" | "AutoPlay" | "EngineMatch" | "WholeGameVisits" | "AnalysisRegion" | "Tracking" | "Hawkeye" | "FoxKifu" | "TencentKifu" | "Contribution";

export type ImportItemView = { 
/**
 * Java location: `config.txt` section and key (`ui.read-komi`), `persist` key
 * (`persist:filesystem.last-folder`) or a file in the Java folder (`file:save`).
 */
key: string, status: ImportStatus, 
/**
 * This product's setting it maps to.
 */
target: ImportTarget | null, 
/**
 * The value taken over, for imported settings; the rejected value for incompatible ones.
 */
value: string | null, reason: ImportReason | null, detail: string | null, credential: boolean, };

export type EngineProblem = { "type": "ProgramNotFound", path: string, } | { "type": "FileNotFound", option: string, path: string, } | { "type": "EmptyCommand" } | { "type": "InvalidValue", field: string, };

export type ImportEngineView = { 
/**
 * Position in the Java engine list.
 */
index: number, name: string, 
/**
 * The command as it will be used (Java-relative paths resolved), credentials masked.
 */
command: string, status: ImportStatus, reason: ImportReason | null, 
/**
 * The command or initial commands hold credentials: imported only when confirmed.
 */
credential: boolean, is_default: boolean, problems: Array<EngineProblem>, 
/**
 * Java fields of this entry this product does not use yet.
 */
unmigrated_fields: Array<string>, 
/**
 * Java fields that do not apply here: Java's management of its own bundled KataGo package
 * (`managedProfileType`, `managedProfileCommand`); this client manages only resources it
 * downloaded or imported itself.
 */
incompatible_fields: Array<string>, };

export type ImportPreviewView = { 
/**
 * Answer with [`Command::ApplyJavaImport`].
 */
id: number, config_file: string, persist_file: string | null, program_dir: string | null, items: Array<ImportItemView>, engines: Array<ImportEngineView>, commentary_credential: ImportCommentaryCredential | null, cloud_credentials: CloudImportCredentials | null, 
/**
 * The SSH account (`user@host:port`) whose Java password 精析设置's 远程引擎 would copy
 * when confirmed.
 */
analysis_ssh_credential: string | null, 
/**
 * Java has a 跑谱贡献 account, password or SSH password to copy when confirmed.
 */
contribution_credential: boolean, 
/**
 * A Java 快速启动 command carries credentials: the program list comes over only when confirmed.
 */
quick_link_credential: boolean, 
/**
 * Settings already chosen in this product that the import would replace.
 */
replaces: Array<ImportTarget>, 
/**
 * This product's themes that the Java theme folders of the same name would replace
 * (listed under [`ImportTarget::Themes`] in `replaces`).
 */
replaced_themes: Array<string>, 
/**
 * Java 存档与读档 slots; imported only when the user chooses to.
 */
recovery: Array<ImportSlotView>, };

export type ImportReportView = { config_file: string, items: Array<ImportItemView>, engines: Array<ImportEngineView>, 
/**
 * The imported language applies after a restart.
 */
restart_required: boolean, recovery: Array<ImportSlotView>, };

export type ImportFailure = { "type": "NoConfig", path: string, } | { "type": "Unreadable", path: string, detail: string, } | { "type": "Invalid", path: string, detail: string, };

export type ImportCommentaryCredential = { source: string, base_url: string, };

export type DiagnosticModule = "engine" | "gtp-summary" | "readboard-yike" | "network-remote";

export type TraceScope = "engine-gtp" | "readboard-yike" | "network-websocket";

export type LoggingSettings = { 
/**
 * 诊断记录 (`diagnostics-enabled`).
 */
diagnosticsEnabled: boolean, 
/**
 * The modules whose diagnostic records are written (`diagnostic-modules`).
 */
diagnosticModules: Array<DiagnosticModule>, 
/**
 * 完整日志's scope choice for the next session (`preferred-full-trace-scopes`).
 */
preferredFullTraceScopes: Array<TraceScope>, };

export type QuickLinkProgram = { name: string, command: string, };

export type QuickLinkSettings = { show: boolean, programs: Array<QuickLinkProgram>, };

export type KataParam = "Pda" | "Wrn";

export type KataParamSettings = { 
/**
 * `ui.txt-kata-engine-wrn`: the last 广度 value; Java's empty default is none.
 */
wrn: number | null, 
/**
 * `ui.chk-kata-engine-wrn` [false].
 */
wrnEnabled: boolean, 
/**
 * `ui.autoload-kata-engine-pda` [false] with `ui.auto-load-txt-kata-engine-pda`: set on
 * every KataGo that starts, except a `KataGoPda` one.
 */
autoLoadPda: boolean, autoLoadPdaValue: number | null, 
/**
 * `ui.autoload-kata-engine-wrn` [false] with `ui.auto-load-txt-kata-engine-wrn`.
 */
autoLoadWrn: boolean, autoLoadWrnValue: number | null, 
/**
 * `ui.show-pda-in-menu` / `ui.show-wrn-in-menu` [true]: the toolbar shows the group.
 */
showPda: boolean, showWrn: boolean, };

export type KataParamEdit = { enabled: boolean, value: number | null, auto_load: boolean, };

export type KataEngineParamsEdit = { pda: KataParamEdit | null, wrn: KataParamEdit, 
/**
 * 在工具栏中控制 (`show-pda-in-menu`, `show-wrn-in-menu`).
 */
show_pda: boolean, show_wrn: boolean, };

export type KataParamsView = { 
/**
 * What the engine last confirmed (read after start-up, or a set it accepted).
 */
pda: number | null, wrn: number | null, 
/**
 * Why the toolbar cannot change each now (a Java message key), `None` when it can.
 */
pda_unavailable: string | null, wrn_unavailable: string | null, 
/**
 * Change with every start-up read-back of the foreground engine (`readKataParameters`)
 * and with another foreground engine; the toolbar shows each value then
 * (`LizzieFrame.setPdaAndWrn`), not after each set it made itself. 广度's also changes when
 * a game gives back a 广度 the user changed during it (`restoreWRN`). `None` before any.
 */
pda_read_back: string | null, wrn_read_back: string | null, };

export type LeelaOptionSetting = { enabled: boolean, autoLoad: boolean, value: string, };

export type LeelaOptionSettings = { 
/**
 * `mem`: `Maximum Memory Use (MiB)`.
 */
memory: LeelaOptionSetting, visits: LeelaOptionSetting, lagbuffer: LeelaOptionSetting, 
/**
 * `resign`: `Resign Percentage`.
 */
resign: LeelaOptionSetting, };

export type CustomPdaMode = "Disabled" | "Dynamic" | "Fixed";

export type CustomPdaSettings = { mode: CustomPdaMode, dynamicCap: number | null, fixedValue: number | null, autoApply: boolean, };

export type CustomPdaView = { pda: number | null, cap: number | null, mode: CustomPdaMode, };

export type ThreadOverride = "Off" | "On" | "Unknown";

export type SearchThreadsCommand = { "type": "Read", process: number, } | { "type": "Confirm", process: number, enabled: boolean, text: string, edited: boolean, params: KataEngineParamsEdit, } | { "type": "Restore", ticket: number, };

export type ThreadRestoreSource = "CFG" | "BENCHMARK" | "COMMAND";

export type SearchThreadsView = { 
/**
 * The process; another value is another process.
 */
process: number, state: ThreadOverride, 
/**
 * What the process last confirmed to this row (a read or a set), `None` before that.
 */
confirmed: number | null, 
/**
 * It runs on another host (SSH, 远程算力): its source cannot be restored from here.
 */
remote: boolean, };

export type SearchThreadsOperationView = { ticket: number, status: SearchThreadsStatus, };

export type SearchThreadsStatus = { "type": "Running" } | { "type": "ConfirmRestore", source: ThreadRestoreSource, recommended: number | null, } | { "type": "Done", threads: number | null, } | { "type": "Failed", message_key: string, detail: string | null, };

export type CloudImportCredentials = { identifier: string, token: boolean, password: boolean, };

export type CloudLaunch = { provider: CloudProvider, identifier: string, args: string, url: string, };

export type CloudProvider = "Zhizi" | "Custom";

export type CloudSettings = { provider: CloudAccountProvider, identifier: string, rememberToken: boolean, rememberPassword: boolean, args: string, 
/**
 * Java's persistent custom websocket URL may itself contain authentication material.
 */
customUrl: string, catalog: CloudCatalog, catalogUpdatedAt: number, };

export type CloudAccountProvider = "local" | "zhizi" | "custom";

export type CloudCommand = { "type": "State" } | { "type": "PasswordLogin", identifier: string, password: string, remember_token: boolean, remember_password: boolean, } | { "type": "CodeLogin", identifier: string, code: string, remember_token: boolean, } | { "type": "SendCode", identifier: string, purpose: CodePurpose, } | { "type": "ResetPassword", identifier: string, code: string, password: string, remember_token: boolean, remember_password: boolean, } | { "type": "Remember", token: boolean, password: boolean, } | { "type": "Logout" } | { "type": "Overview", force: boolean, } | { "type": "Usages", page: number, page_size: number, finished: boolean | null, } | { "type": "Credits", page: number, page_size: number, credit_type: string | null, } | { "type": "RefreshCatalog", args: string, } | { "type": "Products" } | { "type": "CreateOrder", purchase: Purchase, confirmed: boolean, } | { "type": "ReadOrder", id: string, } | { "type": "VerifyPayment", id: string, };

export type CloudReply = { settings: CloudSettings, state: CloudState, result: CloudResult, };

export type CloudState = { authenticated: boolean, hasPassword: boolean, credentialBackend: string, credentialAvailable: boolean, tokenStoredSecurely: boolean, passwordStoredSecurely: boolean, storageWarning: boolean, generation: number, };

export type CloudResult = { "type": "State" } | { "type": "LoggedIn" } | { "type": "CodeSent", retry_after_seconds: number, } | { "type": "PasswordReset" } | { "type": "LoggedOut" } | { "type": "Overview", overview: Overview, } | { "type": "Usages", page: UsagePage, } | { "type": "Credits", page: CreditPage, } | { "type": "Catalog", catalog: CloudCatalog, } | { "type": "Products", products: Array<MembershipProduct>, top_up_presets_fen: Array<number>, } | { "type": "Order", order: PaymentOrder, } | { "type": "PaymentVerification", order: PaymentOrder, verification: PaymentVerification, };

export type CloudFailure = { key: string, status: number, retryAfterSeconds: number, retryable: boolean, uncertain: boolean, cancelled: boolean, authenticationInvalidated: boolean, };

export type CodePurpose = "fast_login" | "reset_password";

export type Purchase = { "type": "TopUp", amount_fen: number, } | { "type": "Membership", product_name: string, auto_renew: boolean, };

export type CloudCatalog = { serverVersion: string, defaultWeight: string, weights: Array<Weight>, };

export type Weight = { name: string, description: string, source: DiscoverySource, };

export type DiscoverySource = "OFFICIAL_DOCUMENTED" | "BUILT_IN_CURRENT" | "SERVER_CAPABILITIES" | "CACHED_LEGACY" | "USER_PRESERVED";

export type AccountProfile = { phone: string, email: string, membership: boolean, membershipExpiresAt: string | null, membershipAutoRenew: boolean | null, };

export type BalanceInfo = { remainingBalanceYuan: string, totalCashAmountYuan: string, yesterdayConsumptionYuan: string, totalConsumptionYuan: string, totalDurationSeconds: number, last24HoursShareDurationSeconds: number, last24HoursVipDurationSeconds: number, currentConnections: number, currentNodes: number, };

export type UsageRecord = { startedAt: string | null, endedAt: string | null, finished: boolean, ready: boolean, durationSeconds: number, totalCostYuan: string, gpuType: string, engineType: string, vip: boolean, shared: boolean, };

export type UsagePage = { total: number, page: number, pageSize: number, items: Array<UsageRecord>, };

export type CreditRecord = { creditType: string, amountYuan: string, source: string, productName: string, createdAt: string | null, };

export type CreditPage = { total: number, page: number, pageSize: number, items: Array<CreditRecord>, };

export type Overview = { account: AccountProfile, balance: BalanceInfo, recentUsage: UsagePage, loadedAt: number, };

export type MembershipProduct = { name: string, priceFen: number, durationMonths: number, };

export type PaymentStatus = "Pending" | "Success" | "Fail";

export type PaymentPurpose = "BalanceTopUp" | "VipMembership";

export type PaymentOrder = { id: string, amountFen: number, productName: string, purpose: PaymentPurpose, status: PaymentStatus, codeUrl: string, paidAt: string | null, updatedAt: string | null, };

export type PaymentVerification = { settled: boolean, remainingBalanceChanged: boolean, cashCreditFound: boolean, membershipAdvanced: boolean, };

export type EditCommand = { "op": "Play", point: Point, color: Stone | null, } | { "op": "Pass" } | { "op": "Insert", point: Point, color: Stone, } | { "op": "EditStone", point: Point, change: StoneChange, } | { "op": "DeleteMove", confirmed: EditTarget | null, } | { "op": "DeleteMoveNoHint" } | { "op": "DeleteBranch", confirmed: EditTarget | null, } | { "op": "MoveBranch", up: boolean, } | { "op": "SetAsMain" } | { "op": "UndoEdit" } | { "op": "RedoEdit" } | { "op": "ClearEditCache" } | { "op": "ChangeTurn" } | { "op": "PlayBestMove" } | { "op": "PlayVariation", moves: Array<Point | null>, } | { "op": "AddVariationAsBranch", moves: Array<Point | null>, } | { "op": "ContinueLadder" } | { "op": "FindMove", point: Point, any_branch: boolean, } | { "op": "SetComment", node: number, comment: string, } | { "op": "ToggleMark", point: Point, tool: MarkTool, } | { "op": "ClearMarks" } | { "op": "SetGameInfo", black: string, white: string, komi: number, } | { "op": "NewBoard", confirmed: EditTarget | null, } | { "op": "ClearBoard" } | { "op": "SetBoardSize", width: number, height: number, } | { "op": "Transform", transform: Transform, } | { "op": "SetSetupMode", enabled: boolean, confirmed: EditTarget | null, } | { "op": "SetupStone", point: Point, stone: Stone | null, } | { "op": "SetupClearAll" } | { "op": "SetupSideToPlay", color: Stone, } | { "op": "ConvertToStartingPosition", confirmed: EditTarget | null, } | { "op": "TryPlay" } | { "op": "FrameTsumego", settings: TsumegoFrameSettings, } | { "op": "LoadCapturedProblem", problem: CapturedProblem, };

export type EditTarget = { document: number, revision: number, node: number, };

export type EditingView = { 
/**
 * 起始局面设置模式 is on.
 */
setup_mode: boolean, 
/**
 * 撤销删除 has a record to go back to.
 */
can_undo: boolean, 
/**
 * 恢复删除 has a record to go forward to.
 */
can_redo: boolean, 
/**
 * 试下 is on.
 */
trying: boolean, };

export type EditSettings = { 
/**
 * 启用双击找子 (`allow-double-click`, default on).
 */
allowDoubleClick: boolean, 
/**
 * 启用拖动棋子 (`allow-drag`, default off).
 */
allowDrag: boolean, 
/**
 * Right click opens the board context menu; off, it goes back one move
 * (`show-right-menu`, default on).
 */
showRightMenu: boolean, 
/**
 * 显示新建棋盘确认对话框 (`show-new-board-hint`, default on).
 */
showNewBoardHint: boolean, 
/**
 * 标记工具 shown in the top toolbar (`show-markup-tools`, default off).
 */
showMarkupTools: boolean, 
/**
 * 设置棋盘大小 "其他" width and height (`other-size-width` / `-height`, default 21).
 */
otherSizeWidth: number, otherSizeHeight: number, 
/**
 * 五子棋无提子规则 (`no-capture`, default off): stones placed or read while it is on remove
 * nothing (`Board.place`, `BoardHistoryList.place`).
 */
noCapture: boolean, 
/**
 * 显示落子矩形 (`show-move-rect`, default 仅对局时).
 */
moveRect: MoveRect, 
/**
 * 启用点击棋子回看 (`enable-click-review`, default off): holding a stone shows the position
 * before it was played (`Board.setPressStoneInfo`). 右键 回看 works either way.
 */
clickReview: boolean, 
/**
 * 自由涂鸦's pen when it opens: the one it was closed with (`last-painting-color`, blue).
 */
paintingColor: PaintingColor, };

export type MoveRect = "Always" | "OnPlay" | "Never";

export type ClickReviewView = { document: number, revision: number, 
/**
 * The current node the move was looked up from.
 */
from: number, 
/**
 * The node shown: the position before that move.
 */
node: number, 
/**
 * Row-major, `size.width * size.height`.
 */
stones: Array<Stone | null>, last_action: LastAction | null, snapshot: boolean, to_play: Stone, 
/**
 * Its stored analysis, as the displays show it.
 */
analysis: NodeAnalysisView | null, 
/**
 * 落子评价 marks of that position (`drawMoveRankMark`, move numbers ignored), or `None`
 * while 落子评价 is off and only its last move is marked (`drawLastMoveMark`).
 */
ranks: Array<RankMark> | null, };

export type PaintingColor = "Blue" | "Green" | "Red";

export type MarkTool = "Letter" | "Number" | "Circle" | "Cross" | "Square" | "Triangle" | "Eraser";

export type StoneChange = { "kind": "Switch" } | { "kind": "Delete" } | { "kind": "MoveTo", point: Point, };

export type Transform = "ExchangeColors" | "RotateRight" | "RotateLeft" | "MirrorVertical" | "MirrorHorizontal";

export type ExportKind = "Raw" | "RawWithComments" | "Branch";

export type CommentLabels = { black: string, white: string, winrate: string, 
/**
 * 目差 (the lead after komi).
 */
lead: string, 
/**
 * 盘面 (`show-katago-score-lead-with-komi`).
 */
lead_with_komi: string, stdev: string, playouts: string, komi: string, pda: string, };

export type SlotRef = { "type": "Auto" } | { "type": "Manual", index: number, };

export type SlotThumbnail = { png: string, document: number, revision: number, node: number, };

export type RecoveryCommand = { "type": "NewSlot", name: string, thumbnail: SlotThumbnail | null, } | { "type": "OverwriteSlot", index: number, name: string, thumbnail: SlotThumbnail | null, } | { "type": "RenameSlot", index: number, name: string, } | { "type": "LoadSlot", slot: SlotRef, } | { "type": "DeleteSlot", slot: SlotRef, } | { "type": "DeleteAllSlots" } | { "type": "SetSettings", settings: RecoverySettings, } | { "type": "AutoSave", thumbnail: SlotThumbnail | null, } | { "type": "ExitAutoSave", thumbnail: SlotThumbnail | null, };

export type SlotView = { slot: SlotRef, 
/**
 * Empty for the automatic slot (the UI shows `LizzieFrame.saveAndLoad.exitRecord`).
 */
name: string, 
/**
 * Local time it was written, `yyyy-MM-dd HH:mm:ss`.
 */
time: string, move_number: number, has_thumbnail: boolean, };

export type RecoveryView = { settings: RecoverySettings, 
/**
 * The automatic slot, first on the panel when present.
 */
auto: SlotView | null, slots: Array<SlotView>, 
/**
 * Changes whenever a slot or picture changes (thumbnail cache key).
 */
revision: number, };

export type RecoverySettings = { 
/**
 * 自动存档: the automatic slot is written on exit and every 60 seconds.
 */
autoSaveOnExit: boolean, 
/**
 * 启动时自动恢复: start with the automatic slot's record.
 */
resumePreviousGame: boolean, 
/**
 * 鼠标悬停放大缩略棋盘.
 */
zoomThumbnails: boolean, };

export type ImportSlotView = { 
/**
 * `save:game2`, `save:autoGame1`.
 */
key: string, 
/**
 * Java's automatic slot; it becomes a manual slot here, so this product's own automatic save
 * is kept.
 */
auto: boolean, name: string, time: string, move_number: number, status: ImportStatus, 
/**
 * `Unreadable` (record missing or not readable), `NotChosen`.
 */
reason: ImportReason | null, detail: string | null, };

export type RecoveryImport = { 
/**
 * Name for Java's automatic slot (the UI's `LizzieFrame.saveAndLoad.exitRecord`).
 */
auto_slot_name: string, };

export type DisplaySettings = { 
/**
 * 总是显示黑胜率: win rates and score leads are Black's instead of the side to move's.
 */
winrateAlwaysBlack: boolean, 
/**
 * 目差显示 > 盘面: score leads are the board difference (KataGo's lead plus the komi for
 * Black, minus it for White) instead of the lead after komi (目差).
 */
scoreLeadWithKomi: boolean, 
/**
 * 坐标 (`show-coordinates`, on).
 */
coordinates: boolean, 
/**
 * Exclusive special-coordinate mode (`ConfigDialog2.SpecialCoordsCbx`).
 */
coordinateStyle: CoordinateStyle, 
/**
 * 棋盘下方显示黑白名字 (`show-name-in-board`, on).
 */
namesOnBoard: boolean, 
/**
 * 手数 (`allow-move-number`): 0 none, -1 every move, n the last n moves. A new Java profile
 * has `show-move-number` off, so none.
 */
moveNumbers: number, 
/**
 * 总是从1开始显示 (`movenumber-from-one`): the last n moves count 1..n.
 */
moveNumbersFromOne: boolean, 
/**
 * 分支内从1开始显示 (`new-move-number-in-branch`, on) versus 分支内继续总手数. Only branches
 * created afterwards follow a change (`Menu.moveNumberInBranchTips`).
 */
branchNumbersFromOne: boolean, 
/**
 * 分支内总是显示所有手数 (`show-moveall-inbranch`, off).
 */
allNumbersInBranch: boolean, 
/**
 * 显示分支面板上的手数 (`show-var-move`, on).
 */
treeMoveNumbers: boolean, 
/**
 * 分支面板设置 > 使用滚动面板拓展宽度 (`show-scroll-variation`, on): the variation tree scrolls;
 * off, the whole tree is laid out in the panel around the current node.
 */
scrollVariation: boolean, 
/**
 * 设置最大宽度 (`max-tree-width`, 10000): a scrolled tree this wide is cut off there and
 * counts as too wide.
 */
maxTreeWidth: number, 
/**
 * 超过宽度依然使用滚动条(超过部分将不显示) (`ignore-out-of-width`, off): a too wide tree keeps
 * scrolling instead of being laid out in the panel.
 */
ignoreOutOfWidth: boolean, 
/**
 * 落子评价 (`move-rank-mark-last-move`): -1 none, 0 every move, n the last n moves (1).
 */
moveRank: number, 
/**
 * 显示最后多手 (`txt-move-rank-mark-last-move`, 3).
 */
moveRankCustom: number, 
/**
 * 判定依据 (`move-rank-evaluation-mode`).
 */
moveRankMode: MoveRankMode, 
/**
 * 落子评价 > 原棋盘不显示评价标记(棋盘同步时) (`disable-move-rank-in-origin`, off): the
 * 棋盘同步工具 float board draws no rank marks (`FloatBoardRenderer.java:152-157`); the main
 * board is unaffected.
 */
disableMoveRankInOrigin: boolean, 
/**
 * Win rate loss of rank levels 1–5 (`win-loss-threshold-1..5`), percentage points.
 */
winLossThresholds: [number, number, number, number, number], 
/**
 * Score loss of rank levels 1–5 (`score-loss-threshold-1..5`), points.
 */
scoreLossThresholds: [number, number, number, number, number], 
/**
 * 选点信息 > 胜率 / 计算量 / 目差 (`show-winrate-in-suggestion`, `-playouts-`,
 * `-scoremean-`, all on).
 */
suggestionWinrate: boolean, suggestionPlayouts: boolean, suggestionScore: boolean, 
/**
 * 目差显示为与最佳选点之差 (`show-score-as-diff`, off).
 */
scoreAsDiff: boolean, 
/**
 * 自定义信息顺序: rows of win rate, playouts and score (`suggestion-info-winrate`,
 * `-playouts`, `-scorelead`; 1, 2, 3).
 */
suggestionInfoOrder: [number, number, number], 
/**
 * 延迟显示候选点 (`delay-show-candidates`, off; `delay-candidates-seconds`, 10).
 */
delayCandidates: boolean, delayCandidatesSeconds: number, 
/**
 * 显示选点右上方角标 (`show-suggestion-order`, on).
 */
suggestionOrder: boolean, 
/**
 * 候选点最高值红色高亮 (`show-suggestion-maxred`, off).
 */
suggestionMaxRed: boolean, 
/**
 * 轮白下字体显示为白色 (`white-suggestion-white`, off).
 */
whiteSuggestionWhite: boolean, 
/**
 * Blue best-candidate ring (`show-blue-ring`, on).
 */
blueRing: boolean, 
/**
 * Candidate colour concentration, 1..=3 (`suggestion-color-ratio`, 2).
 */
suggestionColorRatio: number, whiteSuggestionOrderWhite: boolean, 
/**
 * Draw forbidden-analysis points (`leelaz.show-nosugg-circle`, off).
 */
noSuggCircle: boolean, pvVisits: PvVisitsMode, pvVisitsLimit: number, removeDeadInVariation: boolean, 
/**
 * 鼠标悬停显示变化图 (`show-suggestion-variations`, on).
 */
suggestionVariations: boolean, 
/**
 * 鼠标悬停显示变化图时,变化图不刷新 (`norefresh-onmouse-move`, on).
 */
noRefreshOnMouse: boolean, 
/**
 * 设置变化图回放间隔 (`replay-branch-interval-seconds`, 0.9 in a new profile).
 */
replayIntervalSeconds: number, autoReplayBranch: boolean, entireVariationFirst: boolean, entireVariationFirstSeconds: number, continueWithBestMove: boolean, directlyWithBestMove: boolean, 
/**
 * Java `persist.ui-persist.toolbar-parameter[40]`; -1 means no interval entered.
 */
autoplayMainSeconds: number, 
/**
 * Java `persist.ui-persist.toolbar-parameter[41]`; -1 means no interval entered.
 */
autoplaySubMillis: number, 
/**
 * Candidates below this share of the most visits show no numbers
 * (`min-playout-ratio-for-stats`, 0).
 */
minPlayoutRatio: number, 
/**
 * 下一手 (`show-next-moves`, on; `show-next-move-blunder`, on = 详细).
 */
nextMoves: boolean, nextMoveInfo: boolean, 
/**
 * 下一手提示需要计算量 (`min-playouts-for-next-move`, 30).
 */
nextMoveMinPlayouts: number, 
/**
 * 小棋盘设置 (`show-heat`, `show-heat-aftercalc`, `subboard-raw`).
 */
subBoard: SubBoardMode, 
/**
 * The sub board keeps its variation while the pointer is over it (`no-refresh-on-sub`, on).
 */
noRefreshOnSub: boolean, 
/**
 * 纯网络 ownership painter (Java's five `show-pure-estimate-*` exclusive menu choices).
 */
pureEstimate: EstimateStyle, 
/**
 * 胜率图设置 > 显示胜率/目数曲线: the win-rate line (`show-win-rate-line`, on) and the score
 * lead line (`show-score-lead-line`, on); 胜率 / 目数 / 全部.
 */
winrateLine: boolean, scoreLeadLine: boolean, 
/**
 * 单曲线模式使用面积填充 (`show-winrate-graph-fill`, on): only while one line shows.
 */
winrateGraphFill: boolean, 
/**
 * 显示恶手损失条 (`show-blunder-bar`, off).
 */
blunderBar: boolean, 
/**
 * 显示鼠标悬停信息(仅限黑视角) (`show-mouse-over-winrate-graph`, on).
 */
mouseOverWinrateGraph: boolean, 
/**
 * 初始最大目数差 (`initial-max-score-lead`, 15): the score scale before larger leads widen it.
 */
initialMaxScoreLead: number, 
/**
 * Kata形势显示 square style (`show-katago-estimate-small` / `-normal` / `-not-on-live` /
 * `-big-below` / `-by-size`; small).
 */
kataEstimateStyle: EstimateStyle, 
/**
 * 按选点显示评估 (`use-moves-ownership`, on): a hovered candidate shows its own ownership.
 */
useMovesOwnership: boolean, 
/**
 * 使用快捷键(大键盘点) (`shortcut-kata-estimate`, off): `.` toggles Kata形势.
 */
kataEstimateShortcut: boolean, 
/**
 * 记忆评估显示状态 (`save-kata-estimate-status`, off).
 */
kataEstimateSaveStatus: boolean, 
/**
 * The Kata形势 state the next start begins with (`show-katago-estimate`,
 * `-onmainboard`, `-onsubbord`), written as Java writes those keys.
 */
kataEstimate: KataEstimateState, 
/**
 * 终局点目 counts territory (数目, `use-territory-in-score`, off) instead of area (数子).
 */
territoryScoring: boolean, 
/**
 * A move matches the AI when it is among the first `match-ai-moves` candidates (3) with at
 * least `match-ai-percents-playouts` percent (20) of the most visited one's visits.
 */
matchAiMoves: number, matchAiPercentsPlayouts: number, 
/**
 * 独立选点列表 (U) is open (`show-suggestions-frame`, off); reopened at the next start.
 */
suggestionsFrame: boolean, 
/**
 * Its 总在最前 (`suggestions-always-ontop`, off; Q).
 */
suggestionsOnTop: boolean, 
/**
 * Its 列表 / 柱状图 / 当前手 / 鼠标悬停 boxes (`show-bestmoves-list` on,
 * `show-bestmoves-graph` off, `anaframe-show-next` on, `anaframe-use-mousemove` off).
 */
bestMovesList: boolean, bestMovesGraph: boolean, frameShowNext: boolean, frameMouseMove: boolean, };

export type ShownValues = { 
/**
 * Percent, for the shown side.
 */
winrate: number, 
/**
 * For the shown side, with the komi when 盘面 is chosen.
 */
score_lead: number | null, };

export type AsCandidate = { 
/**
 * Percent, for the side that moved (the table compares it with the best candidate's).
 */
winrate: number, 
/**
 * For the side that moved.
 */
score_lead: number | null, shown: ShownValues, 
/**
 * For the side that moved, 盘面 applied, 总是显示黑胜率 ignored (独立选点列表).
 */
side_to_move: ShownValues, };

export type MoveRankMode = "Auto" | "Score" | "Winrate" | "Combined";

export type SubBoardMode = "Variation" | "Raw" | "Policy" | "Visits";

export type BoardOverlayView = { 
/**
 * Move numbers outside a hovered variation. Empty while 手数 is off or 落子评价 shows; while
 * 试下 runs, only its own stones, counted from 1.
 */
move_numbers: Array<MoveNumberLabel>, 
/**
 * 落子评价 marks, at most one per point.
 */
ranks: Array<RankMark>, 
/**
 * The last-move ring gets the red corner triangle (落子评价 of several moves).
 */
rank_corner: boolean, 
/**
 * 整盘精析 result view draws move numbers before evaluation marks.
 */
ranks_after_numbers: boolean, 
/**
 * 下一手 详细, while the next-move hints are on.
 */
next_move: NextMoveInfo | null, 
/**
 * The 自定义信息顺序 dialog's KataGo sample as a candidate of this position would show it.
 */
info_order_sample: ShownValues, };

export type MoveNumberLabel = { point: Point, number: number, 
/**
 * The last move: drawn red, with the red corner triangle.
 */
last: boolean, };

export type MoveRank = "Best" | "Good" | "Normal" | "Inaccuracy" | "Mistake" | "Blunder";

export type RankMark = { point: Point, rank: MoveRank, 
/**
 * The current position's move: the mark fills the last-move ring.
 */
last: boolean, };

export type NextMoveVerdict = "Bad" | "Questionable" | "Good";

export type NextMoveInfo = { point: Point, verdict: NextMoveVerdict, 
/**
 * The result after the move, for the shown side (总是显示黑胜率 / 目差显示 applied).
 */
winrate: number, 
/**
 * `None` without KataGo scores on both positions.
 */
score_lead: number | null, 
/**
 * The mover's score after the move minus the best candidate's (目差显示为与最佳选点之差).
 */
score_diff: number | null, 
/**
 * The candidate on that point is not shown: the move was played and analysed further
 * than the candidate (`shouldIgnoreBestMove`).
 */
hides_candidate: boolean, 
/**
 * The actual next move followed by its best candidate's principal variation.
 */
pv: Array<Point | null>, pv_captures: Array<Array<Point>>, 
/**
 * `nextPvVisits`: the reply's visits for the actual move, then the reply's `pvVisits`;
 * empty when the engine reported no `pvVisits`.
 */
pv_visits: Array<number>, };

export type RawNetMode = "Off" | "Policy" | "Ownership";

export type RawNetView = { mode: RawNetMode, node: number, 
/**
 * Row-major probabilities (Java renders `(int)(policy * 1000)`).
 */
policy: Array<number> | null, 
/**
 * White's win probability, independent of the player to move.
 */
whiteWinrate: number | null, whiteLead: number | null, symmetry: number | null, 
/**
 * Black-positive ownership; exposed only in Ownership mode.
 */
ownership: Array<number> | null, };

export type EstimateStyle = "Small" | "Transparent" | "TransparentNotOnLive" | "BigBelow" | "BySize";

export type CoordinateStyle = "Normal" | "WithI" | "Fox" | "NumbersFromTop" | "NumbersFromBottom";

export type PvVisitsMode = "Off" | "LastMove" | "AllMoves";

export type KataEstimateMode = "Close" | "Hide" | "Main" | "Sub" | "Both";

export type KataEstimateState = { shown: boolean, onMain: boolean, onSub: boolean, };

export type KataEstimateView = { 
/**
 * Session state; [`crate::settings::DisplaySettings::kata_estimate`] is what the next
 * start begins with.
 */
state: KataEstimateState, 
/**
 * 隐藏: requested but drawn on neither board; `.` toggles the boards.
 */
hidden: boolean, 
/**
 * Black-positive ownership of the current node, else (none yet) of its parent, row-major
 * from the top-left; absent while Kata形势 is off or nothing was reported.
 */
ownership: Array<number> | null, };

export type ScoreCommand = { "type": "Toggle" } | { "type": "Point", point: Point, } | { "type": "Recalculate" } | { "type": "Confirm" };

export type ScorePoint = { 
/**
 * Whose area the point counts for; `None` for neutral (dame).
 */
owner: Stone | null, 
/**
 * A stone marked dead.
 */
dead: boolean, 
/**
 * An empty point marked 不计入.
 */
excluded: boolean, };

export type ScoreView = { node: number, 
/**
 * Row-major from the top-left.
 */
points: Array<ScorePoint>, 
/**
 * 数目 (territory) instead of 数子 (area).
 */
territory: boolean, 
/**
 * Living stones (less the handicap White's handicap bonus takes back under area scoring).
 */
black_alive: number, white_alive: number, 
/**
 * Counted points that are not the side's own living stones.
 */
black_points: number, white_points: number, 
/**
 * Prisoners each side took, dead stones included.
 */
black_captures: number, white_captures: number, komi: number, black_total: number, white_total: number, 
/**
 * SGF result written by 确认结果 (`B+3.5`, `W+0.5`; Black wins ties like Java).
 */
result: string, };

export type MoveEvaluation = { 
/**
 * The node the move leads to.
 */
node: number, move_number: number, 
/**
 * The side that played the move.
 */
mover: Stone, point: Point, 
/**
 * 1-based rank of the move among the previous position's candidates, if it is one.
 */
matched: number | null, 
/**
 * 1 for the first candidate, else its visits over the most visits; 0 when not a candidate.
 */
share: number, 
/**
 * Among the first `match_ai_moves` candidates with at least `match_ai_percents_playouts`.
 */
match_ai: boolean, best: boolean, 
/**
 * The position after the move is analysed too (`NodeInfo.analyzed`); the fields below that
 * need it are `None` otherwise.
 */
analyzed: boolean, 
/**
 * Win-rate change for the mover, percentage points (negative is a loss); both positions
 * analysed.
 */
winrate_change: number | null, 
/**
 * Score-lead change for the mover (KataGo), points.
 */
score_change: number | null, 
/**
 * The mover's win rate after the move (实战胜率).
 */
winrate: number | null, 
/**
 * The mover's score lead after the move (实战目数).
 */
score: number | null, 
/**
 * The previous position's best candidate for the mover (一选胜率 / 一选目数).
 */
best_winrate: number, best_score: number | null, 
/**
 * Visits of the position before the move (此手计算量) and after it (下一手计算量, 0 when not
 * analysed).
 */
visits_before: number, visits_after: number, };

export type MoveChange = { 
/**
 * Percentage points.
 */
winrate: number, 
/**
 * Points (KataGo).
 */
score: number | null, };

export type StatisticsView = { 
/**
 * Moves of the current line (to its end) and of the main trunk, in path order.
 */
moves: Array<MoveEvaluation>, 
/**
 * AI总评分 of Black and White, percent: the mean match share of their evaluated moves up to
 * the current position and the move played from it.
 */
ai_score_black: number | null, ai_score_white: number | null, 
/**
 * 统计 上一手 change for the move leading to the current node, the current node analysed:
 * against its parent's analysis, else (Java `drawMoveStatistics`) its grandparent's, two
 * moves back.
 */
last_move: MoveChange | null, };

export type FramePlacement = { x: number, y: number, width: number, height: number, columns: Array<number>, };

export type ExtraMode = "Normal" | "DoubleEngine" | "FourSub" | "Thinking" | "Min" | "FloatBoard";

export type SecondaryEngineView = { 
/**
 * Position in the engine list.
 */
index: number, 
/**
 * The configured name, else the GTP name.
 */
name: string, katago: boolean, 
/**
 * Speaks `kata-analyze` / `lz-analyze`.
 */
can_analyse: boolean, 
/**
 * Identified and synced to the current position.
 */
ready: boolean, 
/**
 * It is analysing the current node (its own 思考 开/关).
 */
analysing: boolean, 
/**
 * An analysis limit stopped its analysis of the current node.
 */
stopped_by_limit: boolean, };

export type SecondaryView = { 
/**
 * Stored secondary analysis of the current node (`BoardData.bestMoves2` and summary).
 */
analysis: NodeAnalysisView | null, 
/**
 * Move numbers, 落子评价 and 下一手 详细 drawn from the secondary analysis.
 */
board: BoardOverlayView, 
/**
 * The secondary engine's pure-network result for the current position.
 */
rawNet: RawNetView | null, 
/**
 * Kata形势 of the secondary engine (`estimateArray2`): Black-positive ownership of the current
 * node, else of its parent; absent while Kata形势 is off or nothing was reported.
 */
ownership: Array<number> | null, 
/**
 * The secondary engine's stored results on the current line and the main trunk, like
 * `history` (`winrate2`, `scoreMean2` of each node): the 副引擎 独立选点列表's actual-move row
 * and its score columns (`isContainsKataData2`).
 */
history: Array<HistoryPoint>, };

export type AnalysisEngineSettings = { 
/**
 * 分析引擎命令 (`analysis-engine-command`).
 */
command: string, 
/**
 * The command was written by the user rather than generated (`analysis-engine-command-customized`):
 * a command that is not customized follows a managed engine.
 */
commandCustomized: boolean, 
/**
 * 单步计算量 (`analysis-max-visits`, at least 1): a request searches one more visit.
 */
maxVisits: number, 
/**
 * 使用当前引擎规则 (`analysis-use-current-rules`) versus 使用指定规则 (`analysis-specific-rules`).
 */
useCurrentRules: boolean, specificRules: string, 
/**
 * 总是覆盖已有分析结果 (`analysis-always-override`).
 */
alwaysOverride: boolean, 
/**
 * 启动Lizzie时预加载引擎 (`analysis-engine-preload`).
 */
preload: boolean, 
/**
 * 结束分析后退出引擎 (`analysis-auto-quit`).
 */
autoQuit: boolean, 
/**
 * 复用当前前台引擎 (`analysis-reuse-current-engine`): borrow the foreground engine instead of
 * starting an analysis process.
 */
reuseCurrentEngine: boolean, 
/**
 * 批量精析's 每手总计算量 (`batch-analysis-playouts`, default 100): a batch request searches
 * at least 2 visits (`AnalysisEngine.targetAnalysisVisits`).
 */
batchVisits: number, 
/**
 * 远程引擎 (`analysis-engine-ssh-info.useJavaSSH`): the analysis command runs on the SSH
 * host `ssh`. Turning it off keeps the connection settings, like Java.
 */
useSsh: boolean, ssh: SshSettings, };

export type QuickModelView = { status: QuickModelStatus, 
/**
 * The installed model file.
 */
path: string | null, 
/**
 * 自动使用 is on.
 */
enabled: boolean, };

export type QuickModelStatus = "Missing" | "RequiresKataGo117" | "Installed" | "Enabled";

export type QuickAnalysisView = { 
/**
 * A task is active (waiting, starting, running or giving its engine back).
 */
running: boolean, 
/**
 * An attempt failed and the task retries in the background
 * (`LizzieFrame.quickAnalysis.retrying`).
 */
retrying: boolean, 
/**
 * The task borrows the foreground engine instead of running an analysis process.
 */
borrowed: boolean, };

export type ProblemListSettings = { 
/**
 * The 问题手 page is shown instead of 评论 (`is-showing-blunder-table`).
 */
showing: boolean, side: ProblemSide, sort: ProblemSort, 
/**
 * Moves losing less win rate are not listed (`problem-list-winrate-threshold`, 10 points).
 */
winrateThreshold: number, 
/**
 * Both positions need this many visits (`blunder-playouts-threshold`, 0).
 */
playoutsThreshold: number, 
/**
 * 显示评论/问题手面板顶部控制条 is off (`hide-blunder-table-control-pane`).
 */
hideControlPane: boolean, };

export type HumanSlSettingsView = { model: HumanSlModelView, training: HumanSlTrainingSettings, advanced_visible: boolean, };

export type HumanSlModelView = { status: HumanSlModelStatus, path: string | null, };

export type HumanSlModelStatus = "Missing" | "Installed";

export type HumanSlSettings = { modelPath: string | null, training: HumanSlTrainingSettings, advancedVisible: boolean, };

export type HumanSlTrainingSettings = { mode: TrainingMode, opponent: OpponentPreset, danRank: boolean, rank: number, playerColor: PlayerColor, moveTimeSeconds: number, handicap: number, komi: number, };

export type TrainingMode = "PostGameReview" | "LiveAnalysis";

export type OpponentPreset = "Rank" | "ModernPro" | "Online9d";

export type PlayerColor = "Random" | "Black" | "White";

export type ProblemSide = "Black" | "White";

export type ProblemSort = "LossDesc" | "MoveAsc" | "MoveDesc";

export type ProblemListView = { 
/**
 * Moves of each side over the win-rate threshold, in the chosen order.
 */
black: Array<ProblemMoveView>, white: Array<ProblemMoveView>, 
/**
 * Main-trunk moves from move 2 whose position before and after are analysed.
 */
analyzed: number, 
/**
 * Main-trunk moves from move 2.
 */
total: number, 
/**
 * Background analysis is running (`评估中`).
 */
running: boolean, 
/**
 * The move a jump marked on the board (`clickbadmove`), while browsing the position before it.
 */
mark: Point | null, };

export type ProblemMoveView = { 
/**
 * The move's node.
 */
node: number, move_number: number, mover: Stone, point: Point, 
/**
 * Win rate lost by the mover, percentage points.
 */
winrate_loss: number, 
/**
 * Points lost, when both positions have KataGo scores.
 */
score_loss: number | null, 
/**
 * Visits of the position after the move.
 */
playouts: number, 
/**
 * 1–5 by the 落子评价 win-rate thresholds; 5 is shown in the loss colour.
 */
tier: number, 
/**
 * The browsing position is this move, or the position before it with the move marked.
 */
current: boolean, };

export type LeaseAvailabilityView = "Available" | "NoForegroundEngine" | "EngineNotReady" | "NotKatago" | "MissingCapability" | "ExistingLease" | "EngineStateUnrestored" | "PlayMode" | "ApplicationExclusive" | "HumanSlGame" | "ReadBoardGma";

export type UnrestoredReason = "Stop" | "Restore";

export type FlashTarget = { "kind": "WholeGame" } | { "kind": "MoveRange", start_move: number, end_move: number, } | { "kind": "AllBranches" };

export type FlashRange = { startMove: number, endMove: number, };

export type FlashAnalysisView = { 
/**
 * Identity of this run; the UI's 隐藏界面 applies to one run.
 */
run: number, 
/**
 * The engine is still starting or being borrowed (进度: 引擎加载中...).
 */
loading: boolean, 
/**
 * Positions answered so far, of `total`.
 */
done: number, total: number, 
/**
 * The foreground engine is borrowed instead of an analysis process.
 */
borrowed: boolean, };

export type FlashOutcome = { "type": "Completed", run: number, } | { "type": "Failed", run: number, message_key: string, } | { "type": "EngineFailed", run: number, };

export type AutoAnalysisSettings = { startMove: number, endMove: number, timeEnabled: boolean, timeSeconds: number, visitsEnabled: boolean, visits: number, firstVisitsEnabled: boolean, firstVisits: number, black: boolean, white: boolean, allBranches: boolean, autoSave: boolean, exitByPause: boolean, exitTip: boolean, diffEnable: boolean, diffBlack: boolean, diffWhite: boolean, diffUseWin: boolean, diffUseScore: boolean, diffWinThreshold: number, diffScoreThreshold: number, diffTime: number, diffVisits: number, diffFirstVisits: number, };

export type AutoAnalysisView = { active: boolean, starting: boolean, diff: boolean, confirmation: boolean, diff_moves: number, diff_nodes: number, pause_tip: boolean, notice: AutoAnalysisNotice | null, };

export type AutoAnalysisNotice = { id: number, message_key: string, directory: string | null, timed: boolean, };

export type WholeGameState = "Idle" | "Preparing" | "Baseline" | "Deep" | "Pausing" | "Paused" | "Complete" | "Cancelled" | "Failed";

export type WholeGameAnalysisView = { run: number, state: WholeGameState, completed: number, total: number, baseline_completed: number, deep_completed: number, target_visits: number, visits: number, percent: number, remaining_ms: number | null, mode_key: string | null, detail_key: string, 
/**
 * Force result display for this document/node without changing saved display settings.
 */
show_results: boolean, save_failed: boolean, };

export type WholeGameSettings = { deepVisits: number, };

export type HawkeyeCommand = { "type": "Toggle" } | { "type": "SetSides", black: boolean, white: boolean, } | { "type": "GotoRow", document: number, node: number, } | { "type": "GotoMove", document: number, node: number, } | { "type": "GotoMoveNumber", document: number, number: number, };

export type HawkeyeSide = { 
/**
 * Moves whose position before is analysed (`matchAnalyzedCount`): the match denominators.
 */
match_analyzed: number, 
/**
 * Of those, AI matches (`isMatchAi`).
 */
matched: number, 
/**
 * Of those, the first candidate (`isBest`).
 */
best: number, 
/**
 * Sum of match shares (`percentsMatch`).
 */
share_sum: number, 
/**
 * Moves with both positions analysed (`analyzedCount`): the loss denominators, the
 * score one too (Java counts a move without a score lead as changing it by 0).
 */
analyzed: number, 
/**
 * Sum of win-rate losses, percentage points (best moves lose nothing).
 */
winrate_loss_sum: number, 
/**
 * Sum of score losses, points.
 */
score_loss_sum: number, };

export type HawkeyeBand = { count: number, sum: number, 
/**
 * Sum of squared losses (Java's "方差" is this over the count).
 */
squares: number, };

export type HawkeyeSpread = { analyzed: number, sum: number, squares: number, 
/**
 * At least the first (`winrate-diff-range1` / `score-diff-range1`) and the second threshold.
 */
above1: HawkeyeBand, above2: HawkeyeBand, };

export type HawkeyeStage = { match_analyzed: number, share_sum: number, analyzed: number, winrate_loss_sum: number, score_loss_sum: number, };

export type HawkeyeMistake = { 
/**
 * Index into [`HawkeyePane::moves`].
 */
index: number, 
/**
 * The loss (negative; 0 for a best move).
 */
change: number, };

export type HawkeyeRunning = { share_sum: number, count: number, };

export type HawkeyePane = { 
/**
 * KataGo / SAI columns and score pages (`isKatago`: the engine is KataGo or the record holds
 * its score data).
 */
katago: boolean, 
/**
 * Move number at the end of the selected line (总手数).
 */
total_moves: number, 
/**
 * The analysed positions of the selected line, root first (the curves).
 */
points: Array<HistoryPoint>, 
/**
 * Move number of the selected line's first move off the main trunk; the score line is
 * dashed from there (`!node.isMainTrunk()`).
 */
off_trunk_from: number | null, 
/**
 * Every move of the selected line whose position before is analysed, root first.
 */
moves: Array<MoveEvaluation>, 
/**
 * Running AI score of Black and White before each of `moves` and after the last one:
 * share sum and count of the moves so far (吻合度走势, `getMatchValue`). `moves.len() + 1`
 * entries, index i holding the moves before `moves[i]`.
 */
running: Array<[HawkeyeRunning, HawkeyeRunning]>, 
/**
 * The 简易列表 rows (indexes into `moves`, root first): both positions analysed, over the
 * 列表筛选 thresholds; each table takes its side's. The window sorts them.
 */
rows: Array<number>, 
/**
 * The 详细列表 rows: `rows` of the sides whose 黑 / 白 box is checked.
 */
detail_rows: Array<number>, 
/**
 * 置顶当前手: the current move's row, shown first whatever the order.
 */
pinned: number | null, 
/**
 * Black and White over the 统计 range.
 */
sides: [HawkeyeSide, HawkeyeSide], 
/**
 * 损失分档 counts of Black and White over the 统计 range, best first: win rate and score.
 */
winrate_buckets: [[number, number, number, number, number, number], [number, number, number, number, number, number]], score_buckets: [[number, number, number, number, number, number], [number, number, number, number, number, number]], 
/**
 * 胜率损失分布 / 目数损失分布 summaries over the 统计 range.
 */
winrate_spread: [HawkeyeSpread, HawkeyeSpread], score_spread: [HawkeyeSpread, HawkeyeSpread], 
/**
 * 分段吻合度: Black and White in 布局, 中盘, 收官 of the whole line.
 */
stages: [[HawkeyeStage, HawkeyeStage, HawkeyeStage], [HawkeyeStage, HawkeyeStage, HawkeyeStage]], 
/**
 * 胜率损失统计 / 目数损失统计: Black and White moves per stage under the first threshold,
 * between the thresholds and over the second.
 */
winrate_misses: [[[number, number, number], [number, number, number], [number, number, number]], [[number, number, number], [number, number, number], [number, number, number]]], score_misses: [[[number, number, number], [number, number, number], [number, number, number]], [[number, number, number], [number, number, number], [number, number, number]]], 
/**
 * 翻车现场(胜率) / (目数): up to five per side, in move order.
 */
winrate_mistakes: Array<HawkeyeMistake>, score_mistakes: Array<HawkeyeMistake>, };

export type HawkeyeView = { 
/**
 * The 黑 / 白 boxes.
 */
black: boolean, white: boolean, 
/**
 * The 统计 range in effect (`matchAiFirstMove`, `matchAiLastMove`): moves after `first_move`
 * up to `last_move`; 当前 follows the browsing position.
 */
first_move: number, last_move: number, 
/**
 * Move number of the browsing position.
 */
current_move: number, 
/**
 * The marked move (`clickbadmove`) while the position before it is browsed.
 */
mark: Point | null, primary: HawkeyePane, 
/**
 * 双引擎模式's secondary engine window.
 */
secondary: HawkeyePane | null, };

export type HawkeyeSettings = { 
/**
 * The window is open; reopened at the next start (`show-badmoves-frame`, off).
 */
showFrame: boolean, 
/**
 * 总在最前, Q (`badmoves-always-ontop`, off; Java wrote `badmoves-always-on-top` but read this
 * key, so the choice was lost on restart).
 */
alwaysOnTop: boolean, 
/**
 * Top page: 0 统计信息, 1 简易列表, 2 详细列表 (`movelist-selected-indextop`, 0).
 */
topPage: number, 
/**
 * Chart page as Java numbers it (`movelist-selected-index`, 0): 0 胜率吻合, 1 吻合度走势,
 * 2 胜率损失分布, 3 目数损失分布, 4 分段吻合度, 5 胜率损失统计, 6 目数损失统计,
 * 7 翻车现场(胜率), 8 翻车现场(目数).
 */
chart: number, 
/**
 * 分支 (`moveList-selected-branch`, 主干).
 */
branch: HawkeyeBranch, 
/**
 * The lists and statistics are shown (`show-movelist-matchai`, on).
 */
showList: boolean, 
/**
 * The chart is shown (`show-winrate-matchai`, on).
 */
showGraph: boolean, 
/**
 * 列表筛选: rows need this win-rate change, score change (KataGo) and visits on both
 * positions (`move-list-winrate-threshold`, `-score-`, `-visits-`, all 0).
 */
winrateThreshold: number, scoreThreshold: number, visitsThreshold: number, 
/**
 * 置顶当前手(仅详细列表) (`movelist-top-curnode`, off).
 */
topCurrentMove: boolean, 
/**
 * 统计 range (全局).
 */
period: HawkeyePeriod, 
/**
 * 布局结束 / 中盘结束 (`opening-end-move` 60, `middle-end-move` 160).
 */
openingEndMove: number, middleEndMove: number, 
/**
 * 其他 (`game-statistics-custom-start` 10, `-end` 200).
 */
customStart: number, customEnd: number, 
/**
 * 胜率损失 thresholds of the charts (`winrate-diff-range1` 3, `-range2` 10) and score
 * (`score-diff-range1` 2, `-range2` 5).
 */
winrateRange1: number, winrateRange2: number, scoreRange1: number, scoreRange2: number, 
/**
 * 损失分档 shows 胜率损失 rather than 目数损失 (`loss-panel-select-winrate`, off).
 */
lossWinrate: boolean, };

export type HawkeyeBranch = "Main" | "Current";

export type HawkeyePeriod = "Current" | "All" | "Opening" | "Middle" | "End" | "Custom";

export type HawkeyePlacement = { x: number, y: number, width: number, height: number, 
/**
 * The sort column (`sortnum`).
 */
sort: number, 
/**
 * The 详细列表's 9 or 12 column widths; empty for Java's preferred widths.
 */
columns: Array<number>, };

export type BatchMode = "Ordinary" | "Flash";

export type BatchMove = "Prior" | "Up" | "Down";

export type BatchCommand = { "type": "Choose", mode: BatchMode, } | { "type": "Open", mode: BatchMode, paths: Array<string>, source: OpenSource, } | { "type": "Setup", mode: BatchMode, } | { "type": "StartOrdinary", settings: AutoAnalysisSettings, } | { "type": "StartFlash", start_move: number, end_move: number, visits: number, all_branches: boolean, } | { "type": "CancelSetup" } | { "type": "CancelFlash" } | { "type": "AddFiles" } | { "type": "Clear" } | { "type": "Move", item: number, to: BatchMove, } | { "type": "Remove", item: number, };

export type BatchItemView = { id: number, 
/**
 * The file name shown in the 文件名 column.
 */
name: string, path: string, };

export type BatchRunView = { mode: BatchMode, 
/**
 * The queued record, the first row.
 */
item: number, 
/**
 * The record is being read, or its engine is starting or being borrowed
 * (进度: 引擎加载中...).
 */
loading: boolean, 
/**
 * Positions analysed so far, of `total`.
 */
done: number, total: number, 
/**
 * 批量精析 borrows the foreground engine instead of an analysis process.
 */
borrowed: boolean, };

export type BatchFailure = { path: string, 
/**
 * `LizzieFrame.prompt.failedToOpenFile` or `LizzieFrame.saveFileFailed`.
 */
message_key: string, detail: string, };

export type BatchNotice = { id: number, 
/**
 * `Leelaz.batchAutoAnalyzeComplete`, `Leelaz.batchAutoAnalyzeStopped`,
 * `LizzieFrame.prompt.failedToOpenFile`, `AnalysisEngine.requestDispatchFailed`,
 * `AnalysisEngine.foregroundRestoreFailed` or `AutoAnalyze.engineUnavailable`.
 */
message_key: string, 
/**
 * Every record of this batch that could not be read or saved, in order.
 */
failures: Array<BatchFailure>, };

export type BatchView = { 
/**
 * 批量分析进度表's rows: the current record first, then the records still to analyse
 * (finished records leave the table, as `Batchfiles` from `BatchAnaNum`).
 */
items: Array<BatchItemView>, 
/**
 * The settings dialog to show for the queue.
 */
setup: BatchMode | null, 
/**
 * The first record is being read before that mode's settings dialog (a pending start:
 * 停止自动(批量)分析 cancels 批量分析 here).
 */
loading: BatchMode | null, running: BatchRunView | null, 
/**
 * Increases when 进度表 should open (`openAnalysisTable` as 批量分析 of several records
 * starts).
 */
open_table: number, notice: BatchNotice | null, };

export type FoxCommand = { "type": "Prepare" } | { "type": "Start", port: number, max_time: number, max_visits: number, warning_confirmed: boolean, replace: FoxReplaceChoice, document: number, revision: number, } | { "type": "Stop" } | { "type": "ReturnLive" } | { "type": "SetPda", value: number, };

export type FoxReplaceChoice = "Ask" | "Save" | "Replace" | "Cancel";

export type FoxView = { status_key: string, listening: boolean, port: number, owns_board: boolean, preparing: boolean, restoring: boolean, ready: boolean, can_start: boolean, can_stop: boolean, rules: string | null, max_time: number | null, max_visits: number | null, connection: number, game: number, revision: number, live_node: number | null, browsing: boolean, browsing_status_key: string, clock: FoxClockView | null, pending: FoxSubmissionView | null, submissions: Array<FoxSubmissionView>, desired_pda: number | null, confirmed_pda: number | null, pda_status_key: string, transcript: Array<string>, outcome: FoxStartOutcome | null, };

export type FoxClockView = { main_seconds: number, byo_seconds: number, periods: number, remaining_seconds: number, };

export type FoxSubmissionView = { connection: number, game: number, revision: number, search: number, vertex: string, outcome: string, written: boolean, reason: string, };

export type FoxStartOutcome = { "type": "NeedsWarning" } | { "type": "NeedsReplacement", document: number, revision: number, } | { "type": "SaveRequested" } | { "type": "Cancelled" } | { "type": "Listening" } | { "type": "Failed", message_key: string, };

export type GameCommand = { "type": "Start", kind: GameKind, settings: GameSettings, autoplay: AutoPlaySettings | null, engine: number | null, } | { "type": "Continue", kind: GameKind, human: Stone | null, } | { "type": "Pause" } | { "type": "Resign" } | { "type": "End" } | { "type": "Regret" } | { "type": "SetAiTime", time: AiTimeSettings, ponder: boolean, autoplay: AutoPlaySettings, } | { "type": "SetRandomStart", settings: RandomStartSettings, } | { "type": "EngineMove" };

export type GameKind = "Genmove" | "Analysis";

export type GamePhase = "Starting" | "Playing" | "Paused" | "Ending";

export type GameClockView = { minutes: number, seconds: number, byo_seconds: number, byo_times: number, };

export type GameView = { id: number, kind: GameKind, phase: GamePhase, human: Stone, 
/**
 * The engine is choosing its move.
 */
thinking: boolean, 
/**
 * 对弈模式 (`isInPlayMode`): analysis hidden while the game runs.
 */
play_mode: boolean, 
/**
 * Candidates shown when the game started (显示黑/白棋候选点; both off for 续弈).
 */
show_black: boolean, show_white: boolean, 
/**
 * AI只使用纯网络思考.
 */
pure_net: boolean, clock: GameClockView | null, };

export type GameEnd = "EngineResigned" | "HumanResigned" | "HumanTimeout" | "Completed" | "Terminated" | "Retired";

export type GameOutcomeView = { game: number, end: GameEnd, 
/**
 * `RE` written by the game (B+R, W+R, B+T, W+T).
 */
result: string | null, 
/**
 * Where the finished game is saved (自动保存棋谱), once the save is under way.
 */
saved: string | null, };

export type GameNotice = { message_key: string, prefix: string | null, };

export type AutoPlayView = { enabled: boolean, settings: AutoPlaySettings, };

export type GameSettings = { 
/**
 * `ui.check-play-black`: 我执黑 (else 我执白).
 */
playBlack: boolean, 
/**
 * `ui.check-continue-play`: 接着当前局面下.
 */
continuePlay: boolean, 
/**
 * `ui.new-game-handicap`: 0 or 2–9.
 */
handicap: number, 
/**
 * `ui.use-free-handicap`: KataGo chooses the handicap points (`place_free_handicap`).
 */
freeHandicap: boolean, 
/**
 * `ui.new-game-komi`.
 */
komi: number, 
/**
 * `ui.limit-my-time`: 限制我的用时.
 */
limitMyTime: boolean, 
/**
 * `ui.my-save-time` index: 无, 5, 10, 20, 30, 60, 120, 180 minutes.
 */
mySaveTime: number, 
/**
 * `ui.my-byoyomo-seconds` index: 无, 10, 15, 20, 30, 40, 60, 120 seconds.
 */
myByoyomiSeconds: number, 
/**
 * `ui.my-byoyomo-times` index: 1, 2, 3, 5, 10, 20.
 */
myByoyomiTimes: number, aiTime: AiTimeSettings, 
/**
 * `ui.kata-visits-playouts-settings`: KataGo `maxVisits` / `maxPlayouts` for the game.
 */
kataLimits: boolean, 
/**
 * `ui.kata-visits-txt` (≤ 0: unlimited).
 */
kataVisits: number, 
/**
 * `ui.kata-playouts-txt` (≤ 0: unlimited).
 */
kataPlayouts: number, 
/**
 * `leelaz.play-ponder`: the engine keeps thinking on the human's turn.
 */
playPonder: boolean, 
/**
 * `ui.use-play-mode`: 对弈模式 hides the analysis while the game runs.
 */
usePlayMode: boolean, 
/**
 * `ui.new-game-show-black` / `ui.new-game-show-white`: candidates shown during the game.
 */
showBlack: boolean, showWhite: boolean, 
/**
 * `ui.auto-save-played-game`: the finished game is saved to `MyGames`.
 */
autoSave: boolean, 
/**
 * `ui.use-pure-net-in-game`: AI只使用纯网络思考 (分析模式).
 */
pureNet: boolean, 
/**
 * `ui.anagame-resign-start-move`, `ui.anagame-resign-move`, `ui.anagame-resign-percent`.
 */
resignStartMove: number, resignMove: number, resignPercent: number, 
/**
 * `ui.disable-wrn-in-game`: 对局时不使用分析广度拓展 (KataGo `analysisWideRootNoise` 0).
 */
disableWrn: boolean, randomStart: RandomStartSettings, };

export type AiTimeSettings = { mode: AiTimeMode, 
/**
 * `leelaz.max-game-thinking-time-seconds`.
 */
fixedSeconds: number, 
/**
 * `ui.advance-time-txt`.
 */
advanced: string, kataType: KataTimeType, 
/**
 * `ui.kata-time-main-time-mins`.
 */
kataMainMinutes: number, 
/**
 * `ui.kata-time-byoyomi-secs`.
 */
kataByoyomiSeconds: number, 
/**
 * `ui.kata-time-byoyomi-times`.
 */
kataByoyomiTimes: number, 
/**
 * `ui.kata-time-fisher-increment-secs`.
 */
kataFischerIncrement: number, };

export type AiTimeMode = "Fixed" | "Advanced" | "KataGo" | "EngineOwned";

export type KataTimeType = "Byoyomi" | "Fischer" | "Absolute";

export type RandomStartSettings = { 
/**
 * `ui.enable-anagame-randomstart`.
 */
enabled: boolean, 
/**
 * `ui.anagame-random-move`.
 */
moves: number, 
/**
 * `ui.anagame-random-winratediff` (percentage points).
 */
winrateDiff: number, 
/**
 * `ui.anagame-random-playoutdiff` (percent of the best candidate's playouts).
 */
playoutsDiff: number, };

export type AutoPlaySettings = { black: boolean, white: boolean, 
/**
 * 按时间(秒).
 */
timeEnabled: boolean, timeSeconds: number, 
/**
 * 总计算量.
 */
playoutsEnabled: boolean, playouts: number, 
/**
 * 首位计算量.
 */
firstPlayoutsEnabled: boolean, firstPlayouts: number, };

export type MatchSessionSettings = { kind: GameKind, timeEnabled: boolean, firstSeconds: number, secondSeconds: number, visitsEnabled: boolean, firstVisits: number, secondVisits: number, firstMoveVisitsEnabled: boolean, firstMoveVisits: number, secondMoveVisits: number, batchEnabled: boolean, batchLimit: number, continuePosition: boolean, exchangeColors: boolean, maxMovesEnabled: boolean, maxMoves: number, autoSave: boolean, saveWinrate: boolean, batchName: string, startNumberEnabled: boolean, startNumber: number, randomMovesEnabled: boolean, randomMoves: number, randomWinrateDiff: number, };

export type MatchCommand = { "type": "Start", settings: MatchSettings, session: MatchSessionSettings, first_engine: number, second_engine: number, disable_wrn: boolean, } | { "type": "StartToolbar" } | { "type": "Stop" } | { "type": "Pause" } | { "type": "SetValues", settings: MatchSettings, session: MatchSessionSettings, disable_wrn: boolean, } | { "type": "ReviseBatchLimit", limit: number, } | { "type": "Intervention", op: MatchIntervention, } | { "type": "ReplaceOpenings", files: Array<MatchOpeningFile>, } | { "type": "WinrateImageDone", game: number, error: string | null, } | { "type": "ConfirmRules", game: number, allow: boolean, };

export type MatchIntervention = { "type": "PlayNow" } | { "type": "ToggleManual" } | { "type": "Resign", side: Stone, };

export type MatchOpeningFile = { path: string, bytes: Array<number>, };

export type MatchPhase = "Starting" | "Playing" | "Paused" | "Ending" | "BetweenGames";

export type MatchScoreView = { first_black_wins: number, first_white_wins: number, second_black_wins: number, second_white_wins: number, double_passes: number, max_moves: number, total_seconds: number, first_visits: number, second_visits: number, };

export type MatchView = { id: number, game: number, phase: MatchPhase, kind: GameKind, batch: boolean, ordinal: number, batch_limit: number, completed_games: number, first_engine: number, second_engine: number, first_name: string, second_name: string, first_is_black: boolean, 
/**
 * A participant is a `KataGoPda` engine (`EngineGamePresentation.participantHasKataGoPda`):
 * its own PDA "..." stays on the toolbar during the game, and the dialog's commands go to it.
 */
kata_go_pda_participant: boolean, score: MatchScoreView, thinking_side: Stone | null, thinking_engine: number | null, manual_moves: boolean, can_play_now: boolean, can_toggle_manual: boolean, can_resign: boolean, rules: string, rules_consent_required: boolean, rules_unverified: Array<MatchRulesUnverified>, rules_target_choice: string | null, opening_index: number | null, elapsed_seconds: number, };

export type PkBoardView = { 
/**
 * The side whose engine gives the score curve (`isPkBoardKataW`, else `isPkBoardKataB`):
 * White when the white engine is KataGo or Sai, else Black when the black one is; `None`
 * when neither is, and no score curve is drawn (`WinrateGraph.resolveScoreLeadAvailable`).
 */
score_side: Stone | null, };

export type MatchRulesUnverified = { name: string, commands: string, side: Stone | null, reason_key: string, };

export type MatchEnd = "Resigned" | "DoublePass" | "MaxMoves" | "UserStopped" | "Retired";

export type MatchOutcomeView = { game: number, ordinal: number, games_dir: string, batch: boolean, end: MatchEnd, winner: Stone | null, result: string, batch_finished: boolean, score: MatchScoreView, saved_paths: Array<string>, unrestored: Array<UnrestoredReason>, };

export type MatchNotice = { message_key: string, arguments: Array<string>, };

export type MatchWinrateImageRequest = { game: number, path: string, };

export type MatchSettingsView = { settings: MatchSettings, session: MatchSessionSettings, disable_wrn: boolean, first_engine: number, second_engine: number, openings: Array<string>, rules_prefill: string, rules_choice: string | null, rules_corrupt: boolean, };

export type MatchTimeMode = "EngineOwned" | "Fixed" | "RawAdvanced";

export type MatchIdentity = { commands: string, name: string, };

export type MatchResignPolicy = { minMove: number, consecutiveMoves: number, winrate: number, };

export type MatchSettings = { first: MatchIdentity, second: MatchIdentity, komi: number, handicap: number, rules: string, ponder: boolean, sgfStart: boolean, sgfRandom: boolean, firstTimeMode: MatchTimeMode, secondTimeMode: MatchTimeMode, advancedTime: boolean, firstTimeCommand: string, secondTimeCommand: string, firstResign: MatchResignPolicy, secondResign: MatchResignPolicy, checkRandomVisits: boolean, randomVisitsPercent: number, showPreviousBestmoves: boolean, previousBestmoveOnly: boolean, };

export type HumanSlCommand = { "type": "Start", settings: HumanSlTrainingSettings, from_current: boolean, advanced_visible: boolean, 
/**
 * The user confirmed stopping a running batch analysis ([`crate::api::CommandResult::NeedsBatchStop`]).
 */
stop_batch: boolean, } | { "type": "CancelPreparation" } | { "type": "Pass" } | { "type": "RetryAi" } | { "type": "Finish" } | { "type": "Abort" } | { "type": "AbortThen", continuation: HumanSlContinuation, };

export type HumanSlPhase = "Preparing" | "Playing" | "Stopping" | "CleanupPending";

export type HumanSlStartupStage = "Starting" | "LoadingModels" | "OptimizingGpu" | "CacheReady" | "Ready";

export type HumanSlSide = "Human" | "Ai";

export type HumanSlAnalysisDisplay = "Suppress" | "Show";

export type HumanSlView = { id: number, phase: HumanSlPhase, stage: HumanSlStartupStage | null, preparing_secs: number, mode: TrainingMode, opponent: OpponentPreset, rank: number, dan_rank: boolean, human_is_black: boolean, human_ms: number, ai_ms: number, clock: HumanSlSide | null, human_turn: boolean, ai_thinking: boolean, ai_failed: boolean, analysis_display: HumanSlAnalysisDisplay, };

export type HumanSlNotice = { "type": "AiPassed" } | { "type": "AiMoveFailed" } | { "type": "StartFailed", reason: string, detail: string | null, } | { "type": "ExitRecoveryFailed" } | { "type": "Ended" };

export type HumanSlContinuation = "NewGame" | "NewAnalysisGame" | "EngineGame";

export type StrengthCommand = { "type": "Report" } | { "type": "SelectModel", id: string, } | { "type": "ImportModel", path: string, } | { "type": "SetBackground", enabled: boolean, } | { "type": "RunHumanSl", resume: boolean, } | { "type": "CancelHumanSl" };

export type StrengthView = { models: Array<StrengthModelView>, active: string, background: boolean, status: StrengthStatusView, manual: StrengthManualView | null, stamp: number, };

export type StrengthStatusView = { key: string, completed: number, total: number, revision: number, };

export type StrengthManualView = { 
/**
 * Numbers each admitted 补算 HumanSL; a dialog only takes the outcome of the run it started
 * (an earlier run's outcome stays visible until the next run is admitted).
 */
run: number, running: boolean, outcome: StrengthManualOutcome | null, };

export type StrengthManualOutcome = { "type": "Ready" } | { "type": "Cancelled" } | { "type": "Failed", message_key: string, detail: string | null, };

export type StrengthModelView = { id: string, name: string, human_sl: boolean, };

export type StrengthReport = { model: StrengthModelView, black: StrengthSideReport, white: StrengthSideReport, overall: StrengthSideReport, enough_data: boolean, };

export type StrengthSideReport = { summary: StrengthSummary, samples: Array<StrengthSample>, segments: Array<StrengthSegment>, minimum_segment_samples: number, };

export type StrengthSegment = { first_move: number, last_move: number, summary: StrengthSummary, };

export type StrengthSummary = { sample_count: number, score_sample_count: number, rank_value: number | null, prediction_sigma: number | null, quality_score: number | null, average_winrate_loss: number, average_score_loss: number, median_score_loss: number, weighted_score_loss: number, average_score_equivalent_loss: number, match_rate: number, first_choice_rate: number, good_move_rate: number, move_rank_good_move_rate: number, move_rank_counts: Array<number>, bad_move_rate: number, mistake_rate: number, blunder_rate: number, average_difficulty: number | null, strength_band: string, confidence: StrengthConfidence, human_sl_coverage: number | null, };

export type StrengthSample = { color: Stone, move_number: number, coordinate: string, winrate_loss: number, score_loss: number | null, first_choice: boolean, ai_rank: number, category: StrengthMoveCategory, move_rank: MoveRank, score_equivalent_loss: number, complexity: number, adjusted_weight: number, };

export type StrengthConfidence = "Low" | "Medium" | "High";

export type StrengthMoveCategory = "Excellent" | "Great" | "Good" | "Inaccuracy" | "Mistake" | "Blunder";

export type CommentaryCommand = { "type": "Open" } | { "type": "Close" } | { "type": "Start", mode: Mode, first: number, last: number, } | { "type": "Ask", question: string, } | { "type": "Stop" } | { "type": "SetWriteToSgf", enabled: boolean, } | { "type": "SavePreferences", preferences: Preferences, } | { "type": "ReloadKnowledge" };

export type ConnectionCommand = { "type": "ListApiModels", base_url: string, api_key: string, } | { "type": "SaveApiKeyConnection", base_url: string, model: string, api_key: string, remember: boolean, } | { "type": "ForgetApiKey" } | { "type": "OpenApiKeyTab" } | { "type": "ChatGptSignIn", account: string | null, } | { "type": "ChatGptCancelSignIn" } | { "type": "ChatGptSelectAccount", account: string, } | { "type": "ChatGptSignOut", account: string, } | { "type": "ChatGptRefreshModels" } | { "type": "SaveChatGptConnection", account: string, model: string, effort: string | null, } | { "type": "SelectProvider", provider: Provider, } | { "type": "OpenUsage" };

export type CommentaryView = { open: boolean, running: boolean, mode: Mode, scope: CommentaryText, current_move: number, main_line_last_move: number, output: string, output_state: CommentaryOutputState, evidence: string, status: CommentaryStatus, model_status: CommentaryText, connection_ready: boolean, settings_loaded: boolean, settings_usable: boolean, chatgpt_usage_visible: boolean, controls_enabled: boolean, start_enabled: boolean, settings_enabled: boolean, stop_enabled: boolean, write_to_sgf: boolean, preferences: Preferences, connection: CommentarySettings, facts: CommentaryConnectionFacts, restore_question: string | null, target_changed: boolean, };

export type Mode = "Next" | "Range" | "Whole";

export type Provider = "unselected" | "chatGpt" | "apiKey";

export type WebBoardCommand = { "type": "Start" } | { "type": "Stop" } | { "type": "ForceExitTrial" };

export type WebBoardView = { 
/**
 * The access address while Web 旁观 runs (`WebBoardManager.getAccessUrl`).
 */
url: string | null, 
/**
 * A viewer's 试下 drives the board (`LizzieFrame.isTrialActive`).
 */
trial_active: boolean, };

export type UpdateCommand = { "type": "Settings", settings: UpdateSettings, } | { "type": "Check" } | { "type": "Update", offer: number, } | { "type": "TogglePause" } | { "type": "Cancel" } | { "type": "Retry" };

export type UpdateOrigin = "Automatic" | "Manual";

export type UpdateOutcome = "UnavailableBuild" | "SigningNotConfigured" | "UnsupportedForm" | "NoUpdate" | "NoPackage" | "Offer" | "Failed";

export type UpdateDownloadPhase = "None" | "Downloading" | "Paused" | "Cancelled" | "Failed" | "Ready";

export type UpdateError = "Integrity" | "Sources" | "Install" | "Permission" | "InUse" | "Interrupted" | "Unrecovered" | "AuthorizationDismissed" | "Unauthorized" | "PackageManager" | "PackageUnfinished" | "Generic";

export type UpdateOfferView = { 
/**
 * Identifies this offer for [`UpdateCommand::Update`].
 */
id: number, version: string, current_version: string, form: InstallForm, asset_name: string, size: number, 
/**
 * 查看 Release.
 */
notes_url: string | null, };

export type UpdateResultView = { outcome: UpdateOutcome, failure: FailureKind | null, 
/**
 * The channel the check used.
 */
channel: UpdateChannel, 
/**
 * 测试 channel: one candidate could not be verified (`UpdateCheckFeedback.warning`).
 */
stable_candidate_failed: boolean, beta_candidate_failed: boolean, offer: UpdateOfferView | null, };

export type UpdateDownloadView = { phase: UpdateDownloadPhase, 
/**
 * No progress reported yet (`准备下载...`).
 */
started: boolean, bytes: number, total: number, 
/**
 * Downloaded; checking the signature.
 */
verifying: boolean, error: UpdateError | null, 
/**
 * With `error`, what the system itself said (the package manager's reasons), or the command
 * that installs the prepared package by hand. Not translated.
 */
detail: string | null, };

export type UpdateView = { 
/**
 * 当前版本.
 */
current_version: string, settings: UpdateSettings, 
/**
 * This run's automatic check: a checked option without it shows (下次启动生效).
 */
auto_check_active: boolean, auto_download_active: boolean, checking: boolean, 
/**
 * A check, download or installation is under way: 检查更新 and 立即更新 wait.
 */
busy: boolean, origin: UpdateOrigin | null, result: UpdateResultView | null, download: UpdateDownloadView, 
/**
 * Increases when the offer should be shown: after a manual check, or once for an automatic
 * check or download (`origin` tells which; an automatic one does not take focus).
 */
remind: number, 
/**
 * The installation was handed over: the program closes (and restarts) now.
 */
installing: boolean, 
/**
 * The previous run closed to install an update, which did not happen: why (`Install` when
 * the installer did not start). Shown once at start.
 */
handover_failed: UpdateError | null, };

export type UpdateSettings = { 
/**
 * 启动时自动检查更新.
 */
autoCheck: boolean, 
/**
 * 自动下载更新: only with 启动时自动检查更新 (`UpdatePreferences.autoDownload`).
 */
autoDownload: boolean, channel: UpdateChannel, source: UpdateSource, };

export type UpdateChannel = "Stable" | "Beta";

export type UpdateSource = "Official" | "Github";

export type InstallForm = "Nsis" | "App" | "AppImage" | "PortableZip" | "Deb" | "Other" | "Unpackaged";

export type FailureKind = "Fetch" | "InvalidCandidate" | "Unexpected";

export type Preferences = { rankMode: string, rankNum: number, styleIndex: number, densityIndex: number, paceIndex: number, variationIndex: number, };

export type CommentarySettings = { provider: Provider, baseUrl: string, model: string, rememberApiKey: boolean, rankMode: string, rankNum: number, styleIndex: number, densityIndex: number, paceIndex: number, variationIndex: number, };

export type CommentaryConnectionFacts = { loaded: boolean, settings_usable: boolean, secure_storage: CommentarySecureStorage, api_key_present: boolean, chatgpt: CommentaryChatGptFacts, };

export type CommentaryAccount = { id: string, email: string, signed_in: boolean, authorized: boolean, model: string, effort: string | null, credentials_unavailable: boolean, session_only: boolean, };

export type CommentaryModel = { slug: string, display_name: string, efforts: Array<string>, default_effort: string | null, };

export type CommentarySecureStorage = { available: boolean, backend: string, };

export type CommentaryChatGptFacts = { accounts: Array<CommentaryAccount>, active: string | null, signing_in: boolean, models: Array<CommentaryModel>, status_key: string, };

export type CommentaryText = { key: string, args: Array<string>, };

export type CommentaryStatus = { key: string, args: Array<string>, tone: CommentaryTone, detail_key: string, };

export type CommentaryTone = "Neutral" | "Running" | "Success" | "Warning" | "Error";

export type CommentaryOutputState = "Empty" | "Loading" | "Output";

export type FoxKifuCommand = { "type": "Open" } | { "type": "Search", text: string, by_uid: boolean, nickname_hint: string, } | { "type": "Page", to: KifuPage, } | { "type": "OpenGame", chessid: string, } | { "type": "SetAfterGet", action: KifuAfterGet, } | { "type": "ClearRecent" };

export type KifuPage = "First" | "Previous" | "Next" | "Last";

export type FoxKifuView = { 
/**
 * Starts each search (and the window's first state): the account field and 查询方式 show
 * `last_name` / `last_query_by_uid` again then.
 */
search: number, last_name: string, last_query_by_uid: boolean, after_get: KifuAfterGet, 
/**
 * 近期搜索, newest first.
 */
recent: Array<FoxRecentSearch>, 
/**
 * 当前野狐账号; both empty: 等待搜索.
 */
nickname: string, uid: string, 
/**
 * The table's rows (the shown page). A new search keeps the previous rows until its games
 * arrive.
 */
rows: Array<FoxKifuRow>, 
/**
 * A list exists: the page buttons work.
 */
listed: boolean, 
/**
 * The page label `page/pages`, with `...` while more games may exist.
 */
page: number, pages: number, complete: boolean, 
/**
 * The progress overlay over the window (blocks the window's mouse input).
 */
busy: FoxKifuBusy | null, 
/**
 * The table is disabled while a record downloads and opens.
 */
loading: boolean, 
/**
 * The latest message box, by increasing id.
 */
notice: KifuNotice | null, 
/**
 * Counts records opened from this window: after each, 打开棋谱后此窗口 applies and the main
 * window comes forward.
 */
opened: number, };

export type FoxKifuRow = { index: number, time: string, black_name: string | null, black_uid: string, black_rank: number, white_name: string | null, white_uid: string, white_rank: number, winner: number, point: number, rule: number, moves: number, 
/**
 * The searched account won (red text).
 */
won: boolean, chessid: string, };

export type FoxKifuBusy = { "type": "Searching", text: string, } | { "type": "Downloading" } | { "type": "Opening" };

export type KifuNotice = { id: number, 
/**
 * The Java message key; a `getKifuFailed` key is followed by `detail`.
 */
message_key: string, detail: string, };

export type KifuAfterGet = "Minimize" | "Close" | "Keep";

export type FoxRecentSearch = { uid: string, nickname: string, };

export type TencentKifuCommand = { "type": "Open" } | { "type": "Search", text: string, } | { "type": "Page", to: KifuPage, } | { "type": "OpenGame", chessid: string, } | { "type": "SetAfterGet", action: KifuAfterGet, } | { "type": "ClearRecent" };

export type TencentKifuView = { 
/**
 * Starts each search (and the window's first state): the nickname / UID field shows
 * `last_name` again then.
 */
search: number, last_name: string, 
/**
 * 打开棋谱后此窗口: the window's own choice, else 野狐棋谱's.
 */
after_get: KifuAfterGet, 
/**
 * 近期搜索, newest first.
 */
recent: Array<TencentRecentSearch>, 
/**
 * 当前腾讯查询; empty: 等待搜索.
 */
query: string, 
/**
 * The table's rows (the shown page). A new search keeps the previous rows until its games
 * arrive.
 */
rows: Array<TencentKifuRow>, 
/**
 * A list exists: the page buttons work.
 */
listed: boolean, 
/**
 * The page label `page/pages`, with `...` while more games may exist.
 */
page: number, pages: number, complete: boolean, 
/**
 * The progress overlay over the window (blocks the window's mouse input).
 */
busy: TencentKifuBusy | null, 
/**
 * The table is disabled while a record downloads and opens.
 */
loading: boolean, 
/**
 * The latest message box, by increasing id.
 */
notice: KifuNotice | null, 
/**
 * Counts records opened from this window: after each, 打开棋谱后此窗口 applies and the main
 * window comes forward.
 */
opened: number, };

export type TencentKifuRow = { index: number, 
/**
 * `starttime`; when empty, `start_seconds` (Unix seconds, 0 none) in local time.
 */
time: string, start_seconds: number, black_name: string | null, black_uid: string, black_rank: number, white_name: string | null, white_uid: string, white_rank: number, winner: number, point: number, rule: number, moves: number, 
/**
 * The searched player won (red text).
 */
won: boolean, chessid: string, };

export type TencentKifuBusy = { "type": "Searching", text: string, } | { "type": "LoadingMore" } | { "type": "Downloading" } | { "type": "Opening" };

export type TencentRecentSearch = { uid: string, query: string, };

export type FunctionMatch = { id: string, level: MatchLevel, title: string, description: string, category: string, target: FunctionTarget, path: Array<string>, shortcut: string, section: SettingsSection | null, };

export type FunctionTarget = "Window" | "Setting" | "Action" | "Navigation" | "Context";

export type MatchLevel = "Exact" | "Phrase" | "Words" | "WeakAlias" | "Typo";

export type SettingsSection = "display" | "kifu" | "engine" | "play" | "advanced" | "theme";

export type PanelSettings = { classicMode: boolean, showingIndependentMain: boolean, showingIndependentSub: boolean, independentMainLocked: boolean, independentSubLocked: boolean, independentMainTop: boolean, independentSubTop: boolean, showSubBoard: boolean, showWinrateGraph: boolean, largeWinrateGraph: boolean, showComment: boolean, showVariationGraph: boolean, showListPane: boolean, showCaptured: boolean, showStatus: boolean, largeSubBoard: boolean, showTopToolbar: boolean, showDoubleMenu: boolean, alwaysOnTop: boolean, showWinrateInTitle: boolean, 
/**
 * Java remembers a sub board hidden by 大胜率图 and restores it when leaving that mode.
 */
hideSubBoardFromLargeWinrate: boolean, };

export type PanelSizes = { leftoverLeftShare: number | null, boardPositionProportion: number, commentHeightShare: number | null, variationGraphShare: number | null, };

export type PanelArrangement = { left: Array<ArrangedPanel>, right: Array<ArrangedPanel>, };

export type ArrangedPanel = { panel: ArrangeablePanel, weight: number, };

export type ArrangeablePanel = "Captured" | "Winrate" | "Comment" | "SubBoard" | "VariationGraph" | "CandidateList";

export type TopToolbarSettings = { 
/**
 * The ▲ button first in the row, which turns the row off (`ui.show-double-menu-btn` [false]).
 */
showDoubleMenuButton: boolean, 
/**
 * 新建 / 打开 / 保存 … 标记 (`ui.show-basic-btn` [true]).
 */
showBasicButtons: boolean, 
/**
 * 落子工具 黑 / 白 / 交替 / 停一手 (`ui.show-edit-bar` [true]).
 */
showEditBar: boolean, 
/**
 * 强制计算工具 允许 / 排除 / 清除 (`ui.show-force-menu` [true]).
 */
showForce: boolean, 
/**
 * 对局控制 新对局 / 暂停 / 认输 (`ui.show-double-menu-game-control` [true]).
 */
showGameControls: boolean, 
/**
 * 限制: 时间 (`ui.show-time-control-in-menu` [false]).
 */
showTimeLimit: boolean, 
/**
 * 限制: 计算 (`ui.show-playout-control-in-menu` [false]).
 */
showVisitLimit: boolean, 
/**
 * 分析: 黑 / 白 (`ui.show-analyze-controller` [false]).
 */
showAnalyzeSides: boolean, 
/**
 * 选点 黑 / 白 (`ui.show-double-menu-var` [true]).
 */
showCandidateSides: boolean, 
/**
 * 选点 胜率 / 计算量 / 目数 (`ui.show-double-menu-moveinfo` [false]).
 */
showCandidateInfo: boolean, 
/**
 * 规则 (`ui.show-rule-menu` [true]).
 */
showRules: boolean, 
/**
 * 参数 (`ui.show-param-menu` [true]).
 */
showParameters: boolean, 
/**
 * 棋盘 (`ui.show-goban-menu` [true]).
 */
showBoardSize: boolean, 
/**
 * 存档 (`ui.show-saveload-menu` [true]).
 */
showSaveLoad: boolean, 
/**
 * 自动换行: the row wraps; off, it stays one row and what does not fit is cut off
 * (`ui.auto-wrap-tool-bar` [true], LizzieFrame.java:14488-14507).
 */
autoWrap: boolean, };

export type BottomButtonSettings = { 
/**
 * 同步 (`ui.liveButton` [true]).
 */
live: boolean, 
/**
 * Kata形势 (`ui.kataEstimate` [true]); shown only for a KataGo engine.
 */
kataEstimate: boolean, 
/**
 * 批量分析 (`ui.batchOpen` [true]).
 */
batchOpen: boolean, 
/**
 * 打开 (`ui.openfile` [false]).
 */
openFile: boolean, 
/**
 * 保存 (`ui.savefile` [false]).
 */
saveFile: boolean, 
/**
 * 终局数子 (`ui.finalScore` [false]).
 */
finalScore: boolean, 
/**
 * 纯网络 (`ui.heatMap` [true]).
 */
heatMap: boolean, 
/**
 * 超级鹰眼 (`ui.badMoves`). `None` until chosen: Java's default is off while the top toolbar
 * and its double menu were both shown at start-up, on otherwise.
 */
badMoves: boolean | null, 
/**
 * 选点列表 (`ui.analyze-list` [false]).
 */
analyzeList: boolean, 
/**
 * 刷新 (`ui.refresh` [false]).
 */
refresh: boolean, 
/**
 * 分析 (`ui.analyse` [true]).
 */
analyse: boolean, 
/**
 * 试下 (`ui.tryPlay` [false]).
 */
tryPlay: boolean, 
/**
 * 设为主分支 (`ui.setMain` [true]).
 */
setMain: boolean, 
/**
 * 返回主分支 (`ui.backMain` [false]).
 */
backMain: boolean, 
/**
 * 清空棋盘 (`ui.clearButton` [true]).
 */
clearBoard: boolean, 
/**
 * 删除 (`ui.deleteMove` [true]).
 */
deleteMove: boolean, 
/**
 * 手数评价 (`ui.move-rank` [false]).
 */
moveRank: boolean, 
/**
 * 手数 (`ui.move` [true]).
 */
moveNumbers: boolean, 
/**
 * 坐标 (`ui.coords` [true]).
 */
coords: boolean, 
/**
 * 自动播放 (`ui.autoPlay` [true]).
 */
autoPlay: boolean, };

export type BottomToolbar = "Hidden" | "Simple";

export type WorkbenchPanel = "SubBoard" | "WinrateGraph" | "LargeWinrateGraph" | "Comment" | "VariationGraph" | "ListPane" | "Captured" | "Status" | "LargeSubBoard" | "AlwaysOnTop";

export type AppearanceSettings = { theme: string, boardStyle: BoardStyle, mode: AppearanceMode, customBoardImage: string, customBackgroundImage: string, frameFontSize: number, 
/**
 * Config.java:1809; no Java UI changes it. Legacy level 0 uses the theme's background blur.
 */
glassEffectLevel: number, };

export type BoardStyle = "Japanese" | "ChineseClassic";

export type AppearanceMode = "System" | "Light" | "Dark";

export type ThemeSettings = { usePureBoard: boolean, usePureBackground: boolean, usePureStone: boolean, pureBoardColor: Array<number>, pureBackgroundColor: Array<number>, showStoneShadow: boolean, shadowSize: number, stoneIndicatorType: number, winrateStrokeWidth: number, scoreLeadStrokeWidth: number, minimumBlunderBarWidth: number, commentFontSize: number, backgroundFilter: number, winrateLineColor: Array<number>, winrateMissLineColor: Array<number>, scoremeanLineColor: Array<number>, blunderBarColor: Array<number>, bestMoveColor: Array<number>, commentBackgroundColor: Array<number>, commentFontColor: Array<number>, commentNodeColor: Array<number>, showCommentNodeColor: boolean, useScorediffInVariationTree: boolean, scorediffInVariationTreeFactor: number, fontName: string, uiFontName: string, winrateFontName: string, boardImage: string, backgroundImage: string, blackStoneImage: string, whiteStoneImage: string, blunderWinrateThresholds: Array<number>, blunderNodeColors: Array<Array<number>>, glassPanelOverlayColor: Array<number>, glassPanelBorderColor: Array<number>, glassPanelHighlightColor: Array<number>, glassPanelShadowColor: Array<number>, glassAccentColor: Array<number>, glassBlurRadius: number, liquidBlurRadius: number, glassCornerRadius: number, 
/**
 * 界面颜色, this product's own (no Java key): the bars', the controls', the accent and the
 * interface text's colour, `[r, g, b]`; empty leaves the skin's own colour.
 */
uiChromeColor: Array<number>, uiControlColor: Array<number>, uiAccentColor: Array<number>, uiTextColor: Array<number>, };

export type ThemeNodeColor = { node: number, color: Array<number>, };

export type ThemeEntry = { name: string, values: ThemeSettings, replaced_images: Array<string>, };

export type AppearanceCatalog = { themes: Array<ThemeEntry>, fonts: Array<string>, boards: Array<ShippedBoard>, stones: Array<ShippedStones>, };

export type ThemeImages = { board: string, background: string, black: string, white: string, customBoard: boolean, customBackground: boolean, };

export type ShippedBoard = { theme: string, addresses: Array<string>, };

export type ShippedStones = { theme: string, pairs: Array<[string, string]>, };

export type CustomLayout = { panels: PanelSettings, board_position_proportion: number, window: WindowPlacement | null, independent_main: WindowPlacement | null, independent_sub: WindowPlacement | null, };

export type IndependentBoard = "Main" | "Sub";

export type RegionCommand = { "type": "Select", allow: boolean, from: Point, to: Point, } | { "type": "Keep" } | { "type": "AllowPoint", point: Point, } | { "type": "AddAllowPoint", point: Point, } | { "type": "RemoveAllowPoint", point: Point, } | { "type": "AvoidPoint", point: Point, } | { "type": "ToggleKeepForcing" } | { "type": "ClearForcing" } | { "type": "Clear" };

export type TrackingCommand = { "type": "Track", point: Point, } | { "type": "Untrack", point: Point, } | { "type": "Clear" };

export type RegionView = { 
/**
 * The region analyses only these points (`allow`), or never these (`avoid`).
 */
allow: boolean, 
/**
 * Empty when there is no region.
 */
points: Array<Point>, 
/**
 * 持续分析/不分析 is on.
 */
keep_forcing: boolean, };

export type FocusView = { 
/**
 * What the foreground engine's own start-up probe found (`Unknown` without one).
 */
capability: FocusCapability, 
/**
 * A point can be tracked now (`canStartTrackingAnalysis`).
 */
available: boolean, target_visits: number, 
/**
 * Tracked points, in the order they were chosen.
 */
points: Array<TrackedPointView>, 
/**
 * The engine still searches focus points the user no longer tracks (the change is sent,
 * not yet accepted).
 */
cancellation_pending: boolean, };

export type TrackedPointView = { point: Point, 
/**
 * Still searched extra (below the target, progressing).
 */
active: boolean, 
/**
 * The highest visits seen for this candidate.
 */
visits: number, 
/**
 * 落子评价 of the candidate against the best one, when both are in the analysis.
 */
rank: MoveRank | null, };

export type FocusCapability = "Unknown" | "Supported" | "Unsupported" | "Failed";

export type RegionSettings = { 
/**
 * `ui.select-allow-moves` [999].
 */
allowMoves: number, 
/**
 * `ui.select-avoid-moves` [999].
 */
avoidMoves: number, 
/**
 * `ui.select-allow-custom-moves` [5].
 */
allowCustomMoves: number, 
/**
 * `ui.select-avoid-custom-moves` [5].
 */
avoidCustomMoves: number, };

export type TrackingSettings = { 
/**
 * Cumulative visits each tracked point is searched to [500]; 0 is stored as 500.
 */
maxVisits: number, 
/**
 * The dashed quality outline around tracked points [true].
 */
showOutline: boolean, 
/**
 * Its opacity in percent, 0–100 [92].
 */
outlineOpacity: number, };

export type TsumegoSettings = { 
/**
 * The 死活题 dialog's last confirmed choices.
 */
frame: TsumegoFrameSettings, 
/**
 * 抓取死活题's 参数设置.
 */
capture: CaptureSettings, 
/**
 * 自定义工具栏 > 死活: the 死活 button on the top toolbar (`show-tsume-go-menu`, on).
 */
showButton: boolean, };

export type TsumegoFrameSettings = { toPlay: TsumegoToPlay, attacker: TsumegoAttacker, koThreat: TsumegoKoThreat, 
/**
 * 墙面间距; a confirmed value that is not above 0 keeps the previous one.
 */
wallDistance: number, };

export type CaptureSettings = { 
/**
 * 黑最大偏色: each channel of a black pixel is at most this.
 */
blackOffset: number, 
/**
 * 黑最低占比: percent of black pixels that makes a black stone.
 */
blackPercent: number, 
/**
 * 白最大偏色: each channel of a white pixel is at least 255 minus this.
 */
whiteOffset: number, 
/**
 * 白最低占比.
 */
whitePercent: number, 
/**
 * 最大灰度偏色: channels of a counted pixel differ by less than this.
 */
grayOffset: number, };

export type TsumegoToPlay = "Keep" | "Black" | "White";

export type TsumegoAttacker = "Auto" | "Black" | "White";

export type TsumegoKoThreat = "Both" | "Attacker" | "Defender" | "None";

export type CapturedProblem = { 
/**
 * The board corner the problem is placed in: 1 upper left, 2 upper right, 3 lower left,
 * 4 lower right.
 */
direction: number, width: number, height: number, stones: Array<number>, };

export type CapturePreview = { x_min: number, y_min: number, width: number, height: number, 
/**
 * The fitted spacing of the squares.
 */
cell_width: number, cell_height: number, 
/**
 * Squares across and down (`cols`, `rows`); the board has one more line each way.
 */
columns: number, rows: number, 
/**
 * Row-major over `(columns + 1) * (rows + 1)` intersections: 0 empty, 1 black, 2 white.
 */
stones: Array<number>, };

export type SshSettings = { host: string, 
/**
 * As entered; a value that is not a port number means 22 (`RemoteConnect.setPort`).
 */
port: string, user: string, 
/**
 * 密钥 (key file) rather than 密码.
 */
useKey: boolean, keyPath: string, };

export type RemoteFailure = { "type": "Connect", detail: string, } | { "type": "Authentication" } | { "type": "PasswordMissing", account: string, } | { "type": "HostKeyChanged", host: string, port: number, trusted: string, offered: string, } | { "type": "Ended", status: number | null, signal: string | null, } | { "type": "Lost", detail: string, };

export type CloudStartFailure = { "type": "Token", key: string, } | { "type": "Unreachable" } | { "type": "Refused", detail: string, } | { "type": "Disconnected" } | { "type": "NotReady" };

export type SshCredentialCommand = { "type": "PasswordState", ssh: SshSettings, } | { "type": "StagePassword", ssh: SshSettings, password: string, } | { "type": "SettlePassword", ssh: SshSettings, keep: boolean, } | { "type": "TrustHostKey", host: string, port: number, offered: string, } | { "type": "ChooseKeyFile", title: string, };

export type SshPasswordState = "None" | "Stored" | "Session";

export type ContributionCommand = { "type": "SaveSettings", settings: ContributionSettings, } | { "type": "SetPassword", user: string, password: string, } | { "type": "PasswordState", user: string, } | { "type": "Start" } | { "type": "PauseResume" } | { "type": "SlowQuit" } | { "type": "ForceQuit" } | { "type": "CloseView" } | { "type": "WatchGame", index: number, } | { "type": "SaveAllGames" } | { "type": "SetWatchSettings", settings: ContributionWatchSettings, } | { "type": "ToggleResultHidden" } | { "type": "SetShowMenu", enabled: boolean, } | { "type": "SavedEngines" } | { "type": "Qualify", engine_path: string, } | { "type": "ChooseFile", kind: ContributionFile, title: string, } | { "type": "OpenLink", link: ContributionLink, };

export type ContributionFile = "Engine" | "Config";

export type ContributionLink = "SignUp" | "OfficialWebsite";

export type ContributionPhase = "Starting" | "Running" | "Exited";

export type ContributionTip = { key: string, 
/**
 * `Signal to stop` was seen: only pause/resume tips replace it (`setEternalTip`).
 */
eternal: boolean, };

export type ContributionGameView = { game_id: string, 
/**
 * 评分对局 (the players differ) versus 自对弈训练.
 */
rating: boolean, black: string, white: string, width: number, height: number, komi: number, 
/**
 * `Utils.getRuleString` in the session language, lines separated by `\r\n`.
 */
rules_text: string, complete: boolean, 
/**
 * Empty until the game is complete and its result was read.
 */
result: string, };

export type ContributionView = { 
/**
 * Session identity; a new start gets a new one.
 */
id: number, phase: ContributionPhase, paused: boolean, 
/**
 * Java `isContributing`: starting or running and not paused. Gates the board, new games,
 * other engine modes and engine switching.
 */
active: boolean, 
/**
 * 顺序结束 was sent.
 */
slow_quitting: boolean, tip: ContributionTip, 
/**
 * The watched game, 1-based; 0 before the first game.
 */
watching: number, total: number, finished: number, game: ContributionGameView | null, result_hidden: boolean, 
/**
 * The `engine` name of this session's console lines (`engine-console` events).
 */
console_engine: string, };

export type ContributionNotice = { "type": "Message", key: string, } | { "type": "OutOfDate", version: string, } | { "type": "OpenSettings", key: string, } | { "type": "Saved", folder: string, } | { "type": "Failed", failure: EngineFailureView, };

export type SavedEngineKind = "Local" | "Remote" | "Indirect" | "Unknown";

export type SavedEngineReason = "EmptyCommand" | "UnparseableCommand" | "MissingFile" | "RemoteTransport" | "IndirectLauncher";

export type SavedEngineCandidate = { 
/**
 * Position in the engine list, from 1.
 */
index: number, name: string, kind: SavedEngineKind, 
/**
 * The normalized absolute executable when it can be filled in.
 */
executable: string | null, 
/**
 * The engine path field text for `executable` (quoted when needed).
 */
field_text: string | null, reason: SavedEngineReason | null, };

export type ContributionQualification = { "type": "Managed", asset_id: string, version: string, backend: string, status: CapabilityStatus, } | { "type": "Unknown" };

export type ContributionSettings = { 
/**
 * KataGo引擎路径 (`contribute-engine-path`).
 */
enginePath: string, 
/**
 * 配置文件路径(可选) (`contribute-config-path`).
 */
configPath: string, 
/**
 * 用户名 (`contribute-user-name`).
 */
userName: string, 
/**
 * 同时对局数 (`contribute-batch-games`, text "6").
 */
batchGames: string, 
/**
 * 启用领地 (`contribute-show-estimate`): `includeOwnership`, and Kata形势 shown while it runs.
 */
showEstimate: boolean, 
/**
 * 不进行评分对局 (`contribute-disable-rating-matches`): `maxRatingMatches=0`.
 */
disableRatingMatches: boolean, 
/**
 * 自定义命令行 (`contribute-use-command`) and its text (`contribute-command`).
 */
useCommand: boolean, command: string, 
/**
 * 完全遵循自定义命令行,无视后续设置 (`contribute-use-pure-command`).
 */
usePureCommand: boolean, 
/**
 * 自动保存完成的棋谱 (`contribute-auto-save`).
 */
autoSave: boolean, 
/**
 * 远程SSH (`contribute-engine-ssh-info.useJavaSSH`), used only with the custom command.
 */
remote: boolean, ssh: SshSettings, 
/**
 * Settings > 显示跑谱贡献菜单 (`show-Contribute`).
 */
showMenu: boolean, 
/**
 * The contribution window's own choices.
 */
watch: ContributionWatchSettings, };

export type ContributionWatchSettings = { 
/**
 * 总是跳转最新一手 (`contribute-watch-always-last-move`).
 */
alwaysLastMove: boolean, 
/**
 * 自动播放 (`contribute-watch-auto-play`).
 */
autoPlay: boolean, 
/**
 * 每手时间(秒) (`contribute-watch-auto-play-interval`, 2.0; only positive values apply).
 */
autoPlayInterval: number, 
/**
 * 自动播放下一局 (`contribute-watch-auto-play-next-game`).
 */
autoPlayNextGame: boolean, 
/**
 * 跳过非19x19 (`contribute-watch-skip-none-19`).
 */
skipNon19: boolean, 
/**
 * 隐藏控制台 (`contribute-hide-console`).
 */
hideConsole: boolean, 
/**
 * 隐藏规则 (`contribute-hide-rules`).
 */
hideRules: boolean, 
/**
 * 总在最前 (`contribute-always-top`).
 */
alwaysTop: boolean, };

export type ReadBoardCommand = { "type": "Open" } | { "type": "SetAlwaysSyncBoardStat", enabled: boolean, } | { "type": "SetGetFocus", enabled: boolean, } | { "type": "SetNoSoundInSync", enabled: boolean, } | { "type": "AnswerUpdate", accept: boolean, } | { "type": "AnswerPonderingNotice", generation: number, decision: PonderingNoticeDecision, } | { "type": "ReturnFocus" } | { "type": "SetFloatEditMode", enabled: boolean, } | { "type": "SetFloatSuggestionsHidden", hidden: boolean, } | { "type": "FloatBoardHidden", request: number, } | { "type": "SetHelperLogging", diagnostics: boolean, capture: boolean, trace: boolean, };

export type PonderingNoticeDecision = "Confirm" | "Suppress" | "Dismiss";

export type ReadBoardView = { 
/**
 * `readboard.exe` was found next to the program (the menu entry works; Windows only).
 */
available: boolean, 
/**
 * A helper process runs.
 */
running: boolean, 
/**
 * The helper syncs a board (`sync` … `endsync`/`stopsync`).
 */
syncing: boolean, 
/**
 * Moves played here are clicked on the target window (`bothSync`).
 */
both_sync: boolean, 
/**
 * GMA auto-play is armed for this colour (the helper's `play>…gma`).
 */
gma: Stone | null, 
/**
 * The float board over the target window, while shown.
 */
float_board: FloatBoardView | null, 
/**
 * The float board was hidden because a `place` is about to be sent (`foreFoxWithInBoard`)
 * and that `place` waits until the window is off the screen: once it is, answer
 * `FloatBoardHidden` with this number. Without the answer the `place` is never written; the
 * local move fails at its confirmation timeout.
 */
float_hide_request: number | null, 
/**
 * A helper update waits for the user's answer (`ReadBoard.updateInstallConfirmMessage`
 * with this version tag).
 */
update_offer: string | null, 
/**
 * GMA waits for the answer to `ReadBoard.websocketPonderingNotice` (the helper asked for
 * background pondering, which the custom WebSocket engine cannot do); answer
 * `AnswerPonderingNotice` with this number.
 */
pondering_notice: number | null, 
/**
 * The helper's log writers (诊断与日志 > ReadBoard), while a helper runs.
 */
logging: HelperLoggingView | null, 
/**
 * 棋盘同步工具选项 > `Menu.alwaysKeepBoardStatSync` (Java `ui.always-sync-boardstat`).
 */
always_sync_board_stat: boolean, 
/**
 * 棋盘同步工具选项 > `Menu.readBoardGetFocus` (Java `ui.read-board-get-focus`).
 */
get_focus: boolean, 
/**
 * 设置 > `Menu.notPlaySoundInSync` (Java `ui.not-play-sound-insync`).
 */
no_sound_in_sync: boolean, 
/**
 * Messages to show once each, in order.
 */
notices: Array<ReadBoardNotice>, };

export type FloatBoardView = { x: number, y: number, width: number, height: number, 
/**
 * `0` Fox, `1` Tygem, `2` Sina, other values a generic client.
 */
board_type: number, 
/**
 * The DPI factor of the helper's scaled form, if it sent one.
 */
scale: number | null, edit_mode: boolean, suggestions_hidden: boolean, 
/**
 * The edit button is shown (not while both boards are synced).
 */
edit_button: boolean, 
/**
 * A click places a move (both boards synced, or edit mode; `FloatBoard.onClicked`).
 */
clicks_play: boolean, 
/**
 * What the overlay draws outside the edit mode: the main line's end
 * (`getCurOrMainEnd(false)`), whatever node the main window shows. Absent in the edit mode,
 * which draws the current position like the main board.
 */
scene: FloatSceneView | null, };

export type FloatSceneView = { node: number, 
/**
 * Row-major, `size.width * size.height`.
 */
stones: Array<Stone | null>, last_action: LastAction | null, to_play: Stone, snapshot: boolean, 
/**
 * Points of the move children (variations behind an end placeholder), first child first.
 */
next_moves: Array<Point>, 
/**
 * Stored analysis of that node, as the main board's `analysis`.
 */
analysis: NodeAnalysisView | null, 
/**
 * Move numbers and 落子评价 of that node, as the main board's `board`.
 */
board: BoardOverlayView, 
/**
 * Kata形势 of that node, else of its parent (`preEstimateArray`); absent while Kata形势 is
 * off or nothing was reported.
 */
ownership: Array<number> | null, };

export type ReadBoardNotice = { "type": "Message", key: string, args: Array<string>, } | { "type": "GmaUnsupported" };

export type HelperLoggingView = { status: LoggingStatus, 
/**
 * The helper process's log session (`DiagnosticsDialog.helperCapability`), empty while unknown.
 */
process_session_id: string, persistence: Persistence | null, drop_count: number | null, desired_diagnostics: boolean, desired_capture: boolean, desired_trace: boolean, diagnostics: Presentation, capture: Presentation, trace: Presentation, 
/**
 * `DiagnosticsDialog.captureSummary`.
 */
capture_summary: string, };

export type LoggingStatus = "Starting" | "CapabilityReady" | "Applying" | "Observed" | "Unknown" | "LegacyUnconfirmed";

export type Persistence = "Healthy" | "Degraded" | "Unavailable";

export type Presentation = "Off" | "On" | "OnStorageDegraded" | "OnStorageUnavailable" | "NotApplied" | "Unknown" | "LegacyUnconfirmed";

export type ManagedProfile = { command: string, assetId: string, weightPath: string, configPath: string, };

export type SupportTier = "stable" | "optional" | "experimental";

export type RuntimeRequirement = "none" | "linux-cuda-cudnn" | "nvidia-runtime-download" | "tensorrt-runtime" | "opencl-icd" | "metal" | "directx12" | "openvino" | "rocm-family";

export type AssetOrigin = "official-release" | "project-build";

export type CapabilityStatus = "unsupported" | "unverified";

export type Asset = { id: string, platform: string, backend: string, assetName: string, sizeBytes: number, sha256: string, executable: string, executableSha256: string, executableSizeBytes: number, origin: AssetOrigin, releaseRepository: string, releaseTag: string, version: string, sourceTag: string, sourceCommit: string, releaseTier: SupportTier, requiredFiles: Array<string>, externalLibraries: Array<string>, cpuFeatures: Array<string>, gpuFamily: string, cuda: string | null, cudnn: string | null, tensorrt: string | null, minimumDriver: string | null, appImage: boolean, licenseSummary: string, licenseTexts: Array<string>, runtimeRequirement: RuntimeRequirement, analysis: boolean, focus: CapabilityStatus, contribution: CapabilityStatus, };

export type Model = { id: string, fileName: string, displayFamily: string, tier: string, architecture: string, minimumKataGoVersion: string, sizeBytes: number, sha256: string, bundled: boolean, downloadUrl: string, publishedAt: string, licenseSummary: string, licenseTexts: Array<string>, runtimeRequirement: RuntimeRequirement, supportTier: SupportTier, analysis: boolean, contribution: boolean, };

export type Catalog = { schemaVersion: number, modelReleaseTag: string, defaultModelId: string, models: { [key in string]: Model }, assets: { [key in string]: Asset }, };

export type CudaCompatibility = "SUPPORTED" | "PROBE_REQUIRED" | "UNSUPPORTED" | "UNKNOWN";

export type NvidiaQueryStatus = "found" | "none" | "failed";

export type ComputeCapability = { major: number, minor: number, inferred: boolean, };

export type NvidiaGpu = { name: string, compute: ComputeCapability | null, driver: string | null, memoryMib: number | null, };

export type AppleFacts = { model: string | null, chip: string | null, build: string | null, };

export type MachineFacts = { os: string, arch: string, rosetta: boolean | null, logicalCpus: number | null, totalMemoryBytes: number | null, cpuName: string | null, cpuAvx2Fma: boolean | null, fuse: boolean | null, libraryVersions: { [key in string]: string }, nvidiaGpus: Array<NvidiaGpu>, nvidiaStatus: NvidiaQueryStatus, cudaCompatibility: CudaCompatibility, apple: AppleFacts | null, 
/**
 * None means the cache/ICD probe failed, not that libraries are absent.
 */
loaderLibraries: Array<string> | null, openclIcd: boolean | null, };

export type RecommendationReason = "nvidiaDetected" | "nvidiaNotDetected" | "driverSupported" | "driverProbeRequired" | "driverUnknown" | "driverTooOld" | "openclAvailable" | "openclUnavailable" | "cpuFallback" | "cpuOptimized" | "cpuFeaturesUnknown" | "fuseUnknown" | "libraryVersionUnknown" | "nativeMetal" | "rosettaUseArm64" | "tensorRtAllowed" | "tensorRtNotRecommended" | "computeUnknown" | "experimentalManualOnly" | "dependenciesMissing";

export type MissingDependency = { "kind": "nvidiaGpu" } | { "kind": "minimumDriver", "detail": string } | { "kind": "linuxLibrary", "detail": string } | { "kind": "loaderCacheUnknown" } | { "kind": "windowsNvidiaRuntime" } | { "kind": "tensorRtRuntime" } | { "kind": "openclIcd" } | { "kind": "directX12" } | { "kind": "openVinoDevice" } | { "kind": "rocmFamily", "detail": string } | { "kind": "cpuFeature", "detail": string } | { "kind": "appImageFuse" };

export type PackageOption = { assetId: string, tier: SupportTier, 
/**
 * Advisory eligibility only: all listed options remain manually selectable.
 */
eligible: boolean, recommended: boolean, reasons: Array<RecommendationReason>, missingDependencies: Array<MissingDependency>, downloadSize: number, };

export type ModelCard = { id: string, tier: string, isDefault: boolean, hintKey: string, };

export type Recommendation = { platform: string | null, packages: Array<PackageOption>, models: Array<ModelCard>, };

export type RuntimePackage = { key: string, version: string, url: string, sha256: string, size: number, };

export type ExperimentalBackend = "direct-ml" | "open-vino-gpu" | "open-vino-npu" | "rocm-gfx103x" | "rocm-gfx110x" | "rocm-gfx1151" | "rocm-gfx120x";

export type ResourceSource = "download" | "import";

export type ResourceReceipt = { assetId: string, version: string, archiveSha256: string, executableSha256: string, executable: string, executableSize: number, source: ResourceSource, installedAt: number, experimentalBackend: ExperimentalBackend | null, };

export type InstalledPackage = { directory: string, executable: string, receipt: ResourceReceipt, usable: boolean, };

export type InstalledWeight = { path: string, size: number, modelName: string | null, catalogModelId: string | null, sha256: string | null, };

export type ResourceInventory = { packages: Array<InstalledPackage>, weights: Array<InstalledWeight>, gtpConfig: string | null, analysisConfig: string | null, };

export type WeightCompatibility = { compatible: boolean, minimumVersion: string | null, };

export type RemoteWeight = { name: string, downloadUrl: string, publishedAt: string, elo: string, sha256: string | null, sizeBytes: number | null, transformer: boolean, recommended: boolean, latest: boolean, };

export type ValidationStatus = "ACTIVE" | "VALID" | "TIMED_OUT" | "WRONG_ARCHITECTURE" | "MISSING_DEPENDENCY" | "START_FAILED";

export type EngineValidation = { status: ValidationStatus, version: string | null, detail: string, };

export type CudaProbeResult = { passed: boolean, timedOut: boolean, detail: string, };

export type NetworkSettings = { proxyMode: ProxyMode, 
/**
 * `network-proxy-host` (manual mode; non-empty when manual).
 */
proxyHost: string, 
/**
 * `network-proxy-port` (manual mode; 1–65535 when manual).
 */
proxyPort: number, };

export type ProxyMode = "Direct" | "System" | "Manual";

export type ResourceItem = { "type": "Package", 
/**
 * Catalog asset identifier.
 */
asset: string, 
/**
 * Explicit experimental-install confirmation.
 */
confirmed: boolean, 
/**
 * Optional experimental device variant.
 */
backend: ExperimentalBackend | null, } | { "type": "Model", 
/**
 * Catalog model identifier.
 */
model: string, } | { "type": "OnlineWeight", 
/**
 * Network name, not a URL.
 */
name: string, } | { "type": "NvidiaRuntime", 
/**
 * `cuda` or `tensorrt`.
 */
backend: string, 
/**
 * Explicit runtime-install confirmation.
 */
confirmed: boolean, } | { "type": "QuickModel" } | { "type": "HumanSlModel" };

export type LocalEngineChoice = { 
/**
 * Executable path; None opens the executable chooser.
 */
executable: string | null, 
/**
 * GTP config; discovered beside the executable before asking.
 */
config: string | null, 
/**
 * Model; discovered beside the executable before asking.
 */
weight: string | null, };

export type ResourceEngineChoice = { 
/**
 * Installed asset identity, or `local` for an explicitly chosen executable.
 */
assetId: string, 
/**
 * Absolute executable path.
 */
executable: string, 
/**
 * GTP config path.
 */
config: string, 
/**
 * Experimental device command overrides.
 */
backend: ExperimentalBackend | null, 
/**
 * Managed profile to update; None adds a new profile.
 */
updateProfileId: string | null, };

export type ResourcePlanView = { 
/**
 * Opaque workbench plan identity.
 */
id: number, 
/**
 * Existing managed profile, or None for addition.
 */
replacesProfileId: string | null, 
/**
 * Name that will be saved.
 */
name: string, 
/**
 * Exact launch command that will be saved.
 */
command: string, 
/**
 * Whether the target is currently the default.
 */
isDefault: boolean, };

export type ResourceTaskView = { 
/**
 * Workbench-assigned identity.
 */
id: number, 
/**
 * Operation category.
 */
kind: string, 
/**
 * Safe catalog identity, never a URL or personal path.
 */
item: string, 
/**
 * Bytes received.
 */
bytes: number, 
/**
 * Entire resource size, if known.
 */
total: number | null, 
/**
 * Current phase (detecting/downloading/installing/verifying).
 */
phase: string, 
/**
 * Whether cancellation may be requested.
 */
cancellable: boolean, };

export type ResourceOutcome = { 
/**
 * Task identity.
 */
task: number, 
/**
 * done / cancelled / failed.
 */
status: string, 
/**
 * Java localization key.
 */
messageKey: string, 
/**
 * Sanitized bounded diagnostic, without personal paths or credentials.
 */
detail: string, };

export type ResourceSetupView = { 
/**
 * Selected profile identity.
 */
profileId: string | null, 
/**
 * Executable path.
 */
executable: string | null, 
/**
 * Explicitly verified version or the ready foreground KataGo's GTP identity; no probe is started.
 */
engineVersion: string | null, 
/**
 * Weight path.
 */
weight: string | null, 
/**
 * Identity read from the weight header.
 */
modelName: string | null, 
/**
 * B11 transformer notice applies.
 */
b11: boolean, 
/**
 * GTP config path.
 */
config: string | null, 
/**
 * Analysis config path.
 */
analysisConfig: string | null, 
/**
 * The command still matches its store-owned marker.
 */
managed: boolean, 
/**
 * foreground / default / local / downloaded / imported.
 */
discoverySource: string, };

export type ResourceAccelerationView = { 
/**
 * Backend identifier.
 */
backend: string, 
/**
 * Platform supports the installer.
 */
applicable: boolean, 
/**
 * Required missing runtime file patterns.
 */
missing: Array<string>, 
/**
 * Bytes safe to delete after confirmation.
 */
cacheBytes: number, 
/**
 * Experimental packages are never automatic recommendations.
 */
experimental: boolean, };

export type ResourceRepairView = { 
/**
 * Failed managed profile.
 */
profileId: string, 
/**
 * CUDA or TensorRT backend.
 */
backend: string, 
/**
 * Missing runtime items.
 */
missing: Array<string>, 
/**
 * Java action label.
 */
messageKey: string, };

export type ResourceCommand = { "type": "Inspect", 
/**
 * Re-probe even when cached.
 */
refresh: boolean, } | { "type": "Download", 
/**
 * Catalog or cached item.
 */
item: ResourceItem, } | { "type": "CancelTask", 
/**
 * Workbench-assigned identity.
 */
task: number, } | { "type": "ImportPackage", 
/**
 * None opens the zip chooser.
 */
path: string | null, 
/**
 * Experimental device variant.
 */
backend: ExperimentalBackend | null, 
/**
 * Confirmation for experimental packages.
 */
confirmed: boolean, } | { "type": "ImportWeight", 
/**
 * None opens the gzip chooser.
 */
path: string | null, } | { "type": "ImportHumanSlModel", path: string | null, } | { "type": "ChooseLocalEngine", 
/**
 * Paths or chooser requests.
 */
choice: LocalEngineChoice, } | { "type": "Verify", 
/**
 * Engine selection.
 */
target: ResourceEngineChoice, 
/**
 * Weight for a required CUDA probe.
 */
weight: string | null, } | { "type": "PlanSetup", 
/**
 * Engine/config selection.
 */
engine: ResourceEngineChoice, 
/**
 * Installed or explicitly chosen weight path.
 */
weight: string, } | { "type": "ApplySetup", 
/**
 * Plan identity.
 */
plan: number, 
/**
 * Set the committed profile as default.
 */
make_default: boolean, 
/**
 * Start through the normal switch-engine path.
 */
switch_now: boolean, } | { "type": "RefreshRemoteWeights" } | { "type": "RepairAnalysisConfig" } | { "type": "OpenResourceFolder" } | { "type": "CleanNvidiaCache", 
/**
 * False inspects; true confirms deletion.
 */
confirmed: boolean, } | { "type": "RepairTensorRt", 
/**
 * Explicit download/install confirmation.
 */
confirmed: boolean, } | { "type": "EnableTensorRt" } | { "type": "SwitchBackToCuda" } | { "type": "SetNetworkProxy", 
/**
 * Direct/System/Manual configuration.
 */
settings: NetworkSettings, };

export type ResourceResult = { "type": "Done" } | { "type": "Started", 
/**
 * Task identity.
 */
task: number, } | { "type": "Plan", 
/**
 * Exact changes awaiting confirmation.
 */
plan: ResourcePlanView, } | { "type": "Verified", 
/**
 * Version classification.
 */
validation: EngineValidation, 
/**
 * Required inference probe, when applicable.
 */
cuda: CudaProbeResult | null, } | { "type": "Chosen", 
/**
 * Complete engine selection.
 */
engine: ResourceEngineChoice, 
/**
 * Selected model.
 */
weight: string, } | { "type": "Cancelled" } | { "type": "CacheSize", 
/**
 * Bytes in the NVIDIA download cache only.
 */
bytes: number, } | { "type": "Refused", 
/**
 * Java localization key.
 */
message_key: string, };

export type ResourcesView = { 
/**
 * Last detected facts; None means not yet detected.
 */
machine: MachineFacts | null, 
/**
 * Detection currently running.
 */
detecting: boolean, 
/**
 * Structured package/model recommendations.
 */
recommendation: Recommendation | null, 
/**
 * Trusted package descriptions, licenses, source and capabilities.
 */
packages: Array<Asset>, 
/**
 * Pinned model descriptions, licenses, source and capabilities.
 */
models: Array<Model>, 
/**
 * Catalog asset identity to trusted download URL.
 */
packageUrls: { [key in string]: string }, 
/**
 * Catalog model identity to trusted download URL.
 */
modelUrls: { [key in string]: string }, 
/**
 * Bounded store inventory including receipts and header identities.
 */
inventory: ResourceInventory, 
/**
 * Cached official network list.
 */
onlineWeights: Array<RemoteWeight>, 
/**
 * idle / refreshing / done / Java cache-preserving failure key.
 */
remoteStatus: string, 
/**
 * Current foreground/default setup.
 */
currentSetup: ResourceSetupView, 
/**
 * CUDA, TensorRT and experimental status rows.
 */
acceleration: Array<ResourceAccelerationView>, 
/**
 * Current operation.
 */
task: ResourceTaskView | null, 
/**
 * Last terminal outcome, consumed once in incremental delivery.
 */
lastOutcome: ResourceOutcome | null, 
/**
 * Last explicitly verified executable and result.
 */
verification: EngineValidation | null, 
/**
 * Executable associated with `verification`; both are updated atomically on task completion.
 */
verificationExecutable: string | null, 
/**
 * Required CUDA inference result.
 */
cudaProbe: CudaProbeResult | null, 
/**
 * Complete local selection after native choosers.
 */
chosenEngine: ResourceEngineChoice | null, 
/**
 * Complete local weight selection.
 */
chosenWeight: string | null, 
/**
 * No configured command and startup is not Empty (Java banner has no dismiss).
 */
startupNeedsRepair: boolean, 
/**
 * Persisted network configuration.
 */
network: NetworkSettings, };

export type ThreadSource = "CFG" | "BENCHMARK";

export type ThreadPolicy = { source: ThreadSource, sourceRevision: number, initialSetup: boolean, "katago-benchmark-threads": number, "katago-benchmark-current-threads": number, "katago-benchmark-backend": string, "katago-benchmark-summary": string, "katago-benchmark-updated-at": number, "katago-benchmark-topology": string, "katago-benchmark-nn-server-threads": number, "katago-benchmark-batch-size": number, "katago-benchmark-visits-per-second": number, "katago-benchmark-nn-evals-per-second": number, "katago-benchmark-average-batch-size": number, environment: unknown, "katago-tuning-profiles-v1": unknown, "katago-measured-scenes-v1": unknown, };

export type GtpConfiguration = { protocol: string, profile: Record<string, unknown>, };

export type TuningCommand = { "type": "Refresh" } | { "type": "SetStartupBenchmark", enabled: boolean, } | { "type": "StartBenchmark", engine_id: string, experimental: boolean, } | { "type": "StopBenchmark" } | { "type": "DismissStartupNotice", task_id: number, } | { "type": "SetThreadSource", engine_id: string, source: ThreadSource, } | { "type": "ImportMeasured", engine_id: string, path: string | null, } | { "type": "ConfirmMeasured", engine_id: string, review_id: number, } | { "type": "RestoreMeasured", engine_id: string, } | { "type": "InspectGtpConfiguration", engine_id: string, } | { "type": "SaveGtpConfiguration", engine_id: string, configuration: GtpConfiguration, } | { "type": "Scan", engine_type: string, analysis: boolean, } | { "type": "ScanIkatago", platform: string, user_name: string, password: string, };

export type TuningResult = { "type": "Done" } | { "type": "Started", task_id: number, } | { "type": "Cancelled" } | { "type": "GtpSchema", engine_id: string, schema: Record<string, unknown> | null, } | { "type": "Scanned", command: string, } | { "type": "MeasuredReview", engine_id: string, review_id: number, report: Record<string, unknown>, };

export type TuningProfileView = { 
/**
 * Stable saved engine identity.
 */
engineId: string, 
/**
 * Saved display name.
 */
name: string, 
/**
 * Only local KataGo profiles can use thread tuning.
 */
eligible: boolean, 
/**
 * Full saved policy, including benchmark metadata and measured scene presence.
 */
policy: ThreadPolicy, 
/**
 * Java message key when the environment changed or cannot be fingerprinted.
 */
environmentStatus: string | null, 
/**
 * Resolved executable / configuration / network metadata for the performance card.
 */
engine: string, 
/**
 * Resolved GTP configuration path.
 */
config: string, 
/**
 * Resolved neural network path.
 */
model: string, 
/**
 * Java's B11 speed notice applies: the network's model header names the official B11.
 */
b11: boolean, };

export type BenchmarkTaskView = { 
/**
 * Workbench-assigned identity used to reject stale progress and completion.
 */
id: number, 
/**
 * Profile captured when the run started.
 */
engineId: string, 
/**
 * Whether the experimental Apple topology search was explicitly requested.
 */
experimental: boolean, 
/**
 * Automatic opt-in startup run; display the non-modal Java benchmark notice.
 */
automatic: boolean, 
/**
 * Progress in thousandths, from real KataGo positions output.
 */
permille: number, 
/**
 * Recent sanitized process status (not analysis output).
 */
statusLines: Array<string>, 
/**
 * Stop requested; remains active until the process has been reaped.
 */
cancelling: boolean, };

export type TuningOutcome = { 
/**
 * Completed task identity.
 */
taskId: number, 
/**
 * Profile targeted by the task.
 */
engineId: string, 
/**
 * Java message key for successful, cancelled or failed outcome.
 */
key: string, 
/**
 * Sanitized explanatory detail.
 */
detail: string, 
/**
 * True only when a valid result was committed.
 */
success: boolean, };

export type TuningView = { 
/**
 * Saved engine policies, including profiles without a result.
 */
profiles: Array<TuningProfileView>, 
/**
 * At most one active benchmark.
 */
task: BenchmarkTaskView | null, 
/**
 * Automatic run notice (same task identity/progress); closing it cancels, it never hides work.
 */
startupNotice: BenchmarkTaskView | null, 
/**
 * Terminal outcome sent once, not replayed by unrelated updates.
 */
outcome: TuningOutcome | null, 
/**
 * Enables the experimental button only on native macOS arm64.
 */
appleSilicon: boolean, };

export type OnlineCommand = { "type": "Open", url: string, refresh_seconds: number, } | { "type": "Stop" } | { "type": "FollowLive", url: string, } | { "type": "LoadLiveList", category: LiveCategory, more: boolean, } | { "type": "SetAlwaysGotoLast", enabled: boolean, } | { "type": "SetOpenHtmlOnLive", enabled: boolean, } | { "type": "BrowserLoad", url: string, } | { "type": "BrowserStopSync" };

export type LiveCategory = "Recommend" | "Local" | "Personal";

export type OnlineText = { key: string, args: Array<string>, };

export type LiveWinrate = { 
/**
 * `BlackWinRate`, in percent.
 */
black: number, 
/**
 * `100 - BlackWinRate`, kept within 0..=100.
 */
white: number, 
/**
 * `Delta` in points; `None` within 0.01 of 0, which the column leaves out.
 */
score: number | null, };

export type LiveGameView = { 
/**
 * 状态: `*` for a pinned game, then 直播预告 / 正在直播 / the result or 已结束 / 未知.
 */
status: string, 
/**
 * 时间.
 */
time: string, 
/**
 * 棋局.
 */
game: string, 
/**
 * 黑方 / 白方: the name and ` [county]`.
 */
black: string, white: string, 
/**
 * 手数.
 */
hands: number, 
/**
 * 胜率: `None` without realtime analysis or a black win rate.
 */
rate: LiveWinrate | null, 
/**
 * 关注.
 */
attention: string, 
/**
 * The room the 同步 button follows (`toRoomUrl`); `None` for a preview without moves
 * (`isPreviewWithoutMoves`), which 同步 refuses with 这局暂时还没有可用棋谱。.
 */
room_url: string | null, };

export type LiveListView = { category: LiveCategory, loading: boolean, games: Array<LiveGameView>, 
/**
 * 正在加载弈客直播... / 已加载 N 局 / 加载弈客直播失败: …
 */
status: OnlineText | null, 
/**
 * Moves on with every loaded list.
 */
generation: number, };

export type OnlineFollowView = { url: string, 
/**
 * Java type: 1, 2, 5, 6, 7 弈客; 3, 4 huanle.
 */
kind: number, yike: boolean, };

export type OnlineStatusView = { seq: number, url: string, text: OnlineText, };

export type ProxyWarningView = { seq: number, message: string, };

export type OnlineView = { following: OnlineFollowView | null, status: OnlineStatusView | null, 
/**
 * `revealYikeLiveSyncStatus`: brings 弈客直播中心 to the front when it moves on.
 */
reveal: number, 
/**
 * `error(true)` for a link started from 打开在线链接: show 无效的链接 and the dialog again.
 */
dialog_error: number, 
/**
 * The 网络代理设置 warning: `message` is Java's text; a new `seq` shows it again.
 */
proxy_warning: ProxyWarningView | null, 
/**
 * The web window follows page addresses (`yikeBrowserSyncEnabled`).
 */
browser_sync: boolean, 
/**
 * The web window title's status (`reportSyncStatus`).
 */
browser_status: OnlineText | null, live: LiveListView, always_goto_last: boolean, open_html_on_live: boolean, };
