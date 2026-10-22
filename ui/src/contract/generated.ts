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
gtpConfiguration?: GtpConfiguration, };

export type EngineStartup = "Default" | "Last" | "Empty" | "Manual";

export type AnalysisSettings = { 
/**
 * `analyze-update-interval-centisec` [10].
 */
intervalCentisec: number, 
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
showPonderLimitedTips: boolean, };

export type EngineScope = "Current" | "Others" | "All";

export type Command = { "type": "ChooseAndOpenRecord" } | { "type": "OpenRecord", path: string, source: OpenSource, } | { "type": "PasteRecord", confirmed: boolean, } | { "type": "SaveRecord", confirmed: number | null, suggested_name: string, } | { "type": "ChooseAndSaveRecord", suggested_name: string, } | { "type": "SaveRecordTo", path: string, } | { "type": "SetReplaceFileHint", enabled: boolean, } | { "type": "ExportRecord", kind: ExportKind, suggested_name: string, labels: CommentLabels, } | { "type": "ExportRecordTo", kind: ExportKind, path: string, labels: CommentLabels, } | { "type": "CopyRecord" } | { "type": "ExportImage", png: string, } | { "type": "ExportImageTo", path: string, png: string, } | { "type": "CopyImage", png: string, } | { "type": "Recovery", op: RecoveryCommand, } | { "type": "Navigate", nav: Navigation, } | { "type": "GotoNode", node: number, } | { "type": "GotoMoveNumber", number: number, } | { "type": "SetReadKomi", enabled: boolean, } | { "type": "SetAppendWinrateToComment", enabled: boolean, } | { "type": "SetLanguage", language: Language, } | { "type": "SetPlaySound", enabled: boolean, } | { "type": "ClearPersonalData" } | { "type": "ResetUiPosition" } | { "type": "OpenLogFolder" } | { "type": "ExportDiagnostics" } | { "type": "ToggleAnalysis" } | { "type": "SwitchEngine", index: number, } | { "type": "RestartEngine" } | { "type": "ShutdownEngines", scope: EngineScope, } | { "type": "SetExtraMode", mode: ExtraMode, } | { "type": "SwitchSecondaryEngine", index: number, } | { "type": "RestartSecondaryEngine" } | { "type": "ShutdownSecondaryEngine" } | { "type": "SaveEngineSettings", engines: Array<EngineProfile>, startup: EngineStartup, } | { "type": "SetAnalysisSettings", settings: AnalysisSettings, } | { "type": "SetKomi", komi: number, } | { "type": "SetKataRules", rules: string, auto_load: boolean, } | { "type": "SendGtpCommand", command: string, } | { "type": "SetFullTrace", enabled: boolean, } | { "type": "SetPonderLimitTips", enabled: boolean, } | { "type": "AnswerRulesPrompt", prompt: number, accept: boolean, } | { "type": "ChooseJavaFolder", program: boolean, } | { "type": "PreviewJavaImport", config_dir: string, program_dir: string | null, } | { "type": "ApplyJavaImport", preview: number, copy_credentials: Array<number>, replace: boolean, recovery: RecoveryImport | null, } | { "type": "DismissJavaImportOffer" } | { "type": "Resource", op: ResourceCommand, } | { "type": "Tuning", op: TuningCommand, } | { "type": "Edit", op: EditCommand, } | { "type": "SetEditSettings", settings: EditSettings, } | { "type": "SetDisplaySettings", settings: DisplaySettings, } | { "type": "ToggleRawNet" } | { "type": "StopRawNet" } | { "type": "SetKataEstimate", mode: KataEstimateMode, } | { "type": "ToggleKataEstimate" } | { "type": "Score", op: ScoreCommand, };

export type CommandResult = { "type": "Done" } | { "type": "Unchanged" } | { "type": "OpenStarted", ticket: number, } | { "type": "NeedsConfirmation" } | { "type": "NeedsEditConfirmation", target: EditTarget, } | { "type": "Exported", path: string, } | { "type": "ExportFailed", path: string, detail: string, } | { "type": "Refused", message_key: string, } | { "type": "SaveStarted", ticket: number, } | { "type": "NeedsReplaceConfirmation", document: number, file_name: string, } | { "type": "NeedsOverwriteConfirmation", path: string, } | { "type": "NeedsExportOverwriteConfirmation", kind: ExportKind, path: string, } | { "type": "NeedsImageOverwriteConfirmation", path: string, } | { "type": "FolderChosen", path: string, } | { "type": "JavaImportPreview", preview: ImportPreviewView, } | { "type": "JavaImportFailed", failure: ImportFailure, } | { "type": "JavaImported", report: ImportReportView, } | { "type": "Resource", result: ResourceResult, } | { "type": "Tuning", result: TuningResult, };

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
startup_benchmark: boolean, };

export type SettingsView = { language: Language, 
/**
 * Language the UI is displaying in this session (resolved at start).
 */
active_language: Language, 
/**
 * Whether the saved language differs from the one in use until restart.
 */
restart_required: boolean, read_komi: boolean, append_winrate_to_comment: boolean, 
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
extra_mode: ExtraMode, engine: EngineSettingsView, edit: EditSettings, display: DisplaySettings, };

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
commands: Array<string>, };

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
 * Full trace of engine GTP traffic is being written.
 */
full_trace: boolean, };

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
ownership: Array<number> | null, };

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
repair?: ResourceRepairView, };

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
 * Each move's result against the analysis of the position it was played in, and the AI
 * match score (统计 panel, captured panel, 超级鹰眼).
 */
statistics: StatisticsView, 
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
 * An engine that failed to start or stopped unexpectedly, once.
 */
engine_failure: EngineFailureView | null, 
/**
 * A notice about analysis, once.
 */
engine_notice: EngineNotice | null, 
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
recovery: RecoveryView, };

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

export type ImportReason = "NotYetAvailable" | "Credential" | "InvalidValue" | "WindowCoordinates" | "RemoteEngine" | "CredentialNotConfirmed" | "Unreadable" | "NotChosen" | "Obsolete";

export type ImportTarget = "ReadKomi" | "LoadSgfLast" | "ReplaceFileHint" | "AppendWinrateToComment" | "Language" | "PlaySound" | "RecentFiles" | "LastFolder" | "Engines" | "EngineStartup" | "LastEngine" | "AnalysisInterval" | "LimitTime" | "MaxAnalyzeTime" | "LimitPlayout" | "LimitPlayouts" | "LimitMaxSuggestion" | "LimitBranchLength" | "FastEngineChange" | "PonderLimitTips" | "KataRules" | "AutoLoadKataRules" | "StartupBenchmark" | "NetworkProxy" | "AllowDoubleClick" | "AllowDrag" | "RightClickMenu" | "NewBoardHint" | "MarkupTools" | "OtherBoardSize" | "NoCapture" | "AutoSaveOnExit" | "ResumePreviousGame" | "ZoomThumbnails" | "LastImageFolder" | "WinrateAlwaysBlack" | "ScoreLeadWithKomi" | "Coordinates" | "NamesOnBoard" | "MoveNumbers" | "MoveRank" | "Suggestions" | "NextMoves" | "SubBoard" | "PureNetwork" | "WinrateGraph" | "KataEstimate" | "Scoring" | "MatchAi" | "SuggestionsFrame" | "ExtraMode";

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
id: number, config_file: string, persist_file: string | null, program_dir: string | null, items: Array<ImportItemView>, engines: Array<ImportEngineView>, 
/**
 * Settings already chosen in this product that the import would replace.
 */
replaces: Array<ImportTarget>, 
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

export type EditCommand = { "op": "Play", point: Point, color: Stone | null, } | { "op": "Pass" } | { "op": "Insert", point: Point, color: Stone, } | { "op": "EditStone", point: Point, change: StoneChange, } | { "op": "DeleteMove", confirmed: EditTarget | null, } | { "op": "DeleteMoveNoHint" } | { "op": "DeleteBranch", confirmed: EditTarget | null, } | { "op": "MoveBranch", up: boolean, } | { "op": "SetAsMain" } | { "op": "UndoEdit" } | { "op": "RedoEdit" } | { "op": "ClearEditCache" } | { "op": "ChangeTurn" } | { "op": "PlayBestMove" } | { "op": "PlayVariation", moves: Array<Point>, } | { "op": "AddVariationAsBranch", moves: Array<Point>, } | { "op": "ContinueLadder" } | { "op": "FindMove", point: Point, any_branch: boolean, } | { "op": "SetComment", node: number, comment: string, } | { "op": "ToggleMark", point: Point, tool: MarkTool, } | { "op": "ClearMarks" } | { "op": "SetGameInfo", black: string, white: string, komi: number, } | { "op": "NewBoard", confirmed: EditTarget | null, } | { "op": "ClearBoard" } | { "op": "SetBoardSize", width: number, height: number, } | { "op": "Transform", transform: Transform, } | { "op": "SetSetupMode", enabled: boolean, confirmed: EditTarget | null, } | { "op": "SetupStone", point: Point, stone: Stone | null, } | { "op": "SetupClearAll" } | { "op": "SetupSideToPlay", color: Stone, } | { "op": "ConvertToStartingPosition", confirmed: EditTarget | null, } | { "op": "TryPlay" };

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
noCapture: boolean, };

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

export type RecoveryCommand = { "type": "NewSlot", name: string, thumbnail: string, } | { "type": "OverwriteSlot", index: number, name: string, thumbnail: string, } | { "type": "RenameSlot", index: number, name: string, } | { "type": "LoadSlot", slot: SlotRef, } | { "type": "DeleteSlot", slot: SlotRef, } | { "type": "DeleteAllSlots" } | { "type": "SetSettings", settings: RecoverySettings, } | { "type": "AutoSave", thumbnail: string, } | { "type": "ExitAutoSave", thumbnail: string | null, };

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
 * Move numbers outside a hovered variation. Empty while 手数 is off or 落子评价 shows.
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
mover: Stone, 
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
 * Win-rate change for the mover, percentage points (negative is a loss); both positions
 * analysed.
 */
winrate_change: number | null, 
/**
 * Score-lead change for the mover (KataGo), points.
 */
score_change: number | null, };

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

export type ExtraMode = "Normal" | "DoubleEngine";

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
confirmed: boolean, };

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
path: string | null, } | { "type": "ChooseLocalEngine", 
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

export type TuningCommand = { "type": "Refresh" } | { "type": "SetStartupBenchmark", enabled: boolean, } | { "type": "StartBenchmark", engine_id: string, experimental: boolean, } | { "type": "StopBenchmark" } | { "type": "DismissStartupNotice", task_id: number, } | { "type": "SetThreadSource", engine_id: string, source: ThreadSource, } | { "type": "ImportMeasured", engine_id: string, path: string | null, } | { "type": "ConfirmMeasured", engine_id: string, review_id: number, } | { "type": "RestoreMeasured", engine_id: string, } | { "type": "InspectGtpConfiguration", engine_id: string, } | { "type": "SaveGtpConfiguration", engine_id: string, configuration: GtpConfiguration, } | { "type": "Scan", engine_type: string, } | { "type": "ScanIkatago", platform: string, user_name: string, password: string, };

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
