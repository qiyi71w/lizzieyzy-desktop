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
komi: number, isDefault: boolean, };

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

export type Command = { "type": "ChooseAndOpenRecord" } | { "type": "OpenRecord", path: string, source: OpenSource, } | { "type": "PasteRecord", confirmed: boolean, } | { "type": "SaveRecord", confirmed: number | null, suggested_name: string, } | { "type": "ChooseAndSaveRecord", suggested_name: string, } | { "type": "SaveRecordTo", path: string, } | { "type": "SetReplaceFileHint", enabled: boolean, } | { "type": "Navigate", nav: Navigation, } | { "type": "GotoNode", node: number, } | { "type": "GotoMoveNumber", number: number, } | { "type": "SetReadKomi", enabled: boolean, } | { "type": "SetLanguage", language: Language, } | { "type": "SetPlaySound", enabled: boolean, } | { "type": "ClearPersonalData" } | { "type": "ResetUiPosition" } | { "type": "OpenLogFolder" } | { "type": "ExportDiagnostics" } | { "type": "ToggleAnalysis" } | { "type": "SwitchEngine", index: number, } | { "type": "RestartEngine" } | { "type": "ShutdownEngines", scope: EngineScope, } | { "type": "SaveEngineSettings", engines: Array<EngineProfile>, startup: EngineStartup, } | { "type": "SetAnalysisSettings", settings: AnalysisSettings, } | { "type": "SetKomi", komi: number, } | { "type": "SetKataRules", rules: string, auto_load: boolean, } | { "type": "SendGtpCommand", command: string, } | { "type": "SetFullTrace", enabled: boolean, } | { "type": "SetPonderLimitTips", enabled: boolean, } | { "type": "AnswerRulesPrompt", prompt: number, accept: boolean, } | { "type": "ChooseJavaFolder", program: boolean, } | { "type": "PreviewJavaImport", config_dir: string, program_dir: string | null, } | { "type": "ApplyJavaImport", preview: number, copy_credentials: Array<number>, replace: boolean, } | { "type": "DismissJavaImportOffer" };

export type CommandResult = { "type": "Done" } | { "type": "Unchanged" } | { "type": "OpenStarted", ticket: number, } | { "type": "NeedsConfirmation" } | { "type": "Exported", path: string, } | { "type": "Refused", message_key: string, } | { "type": "SaveStarted", ticket: number, } | { "type": "NeedsReplaceConfirmation", document: number, file_name: string, } | { "type": "NeedsOverwriteConfirmation", path: string, } | { "type": "FolderChosen", path: string, } | { "type": "JavaImportPreview", preview: ImportPreviewView, } | { "type": "JavaImportFailed", failure: ImportFailure, } | { "type": "JavaImported", report: ImportReportView, };

export type ErrorKind = "InvalidArgument" | "Io" | "Unavailable";

export type ApiError = { kind: ErrorKind, message: string, };

export type DocumentView = { 
/**
 * Identity of the open document; changes on every adopted record.
 */
id: number, format: RecordFormat | null, path: string | null, file_name: string | null, size: BoardSize, info: GameInfo, 
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
children: Array<number>, kind: TreeNodeKind, pass: boolean, move_number: number, has_comment: boolean, };

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

export type OpenOutcome = { "type": "Adopted", ticket: number, document: number, } | { "type": "Failed", ticket: number, message_key: string, detail: string, };

export type SaveOutcome = { "type": "Saved", ticket: number, document: number, revision: number, path: string, } | { "type": "Failed", ticket: number, document: number, path: string, message_key: string, detail: string, };

export type PendingOpen = { ticket: number, path: string | null, };

export type EngineSettingsView = { engines: Array<EngineProfile>, startup: EngineStartup, analysis: AnalysisSettings, 
/**
 * `kata-rules` as saved (JSON or preset name; empty = the engine's own).
 */
kata_rules: string, auto_load_kata_rules: boolean, };

export type SettingsView = { language: Language, 
/**
 * Language the UI is displaying in this session (resolved at start).
 */
active_language: Language, 
/**
 * Whether the saved language differs from the one in use until restart.
 */
restart_required: boolean, read_komi: boolean, 
/**
 * Settings > "播放落子音效" (Java `ui.play-sound`, default on).
 */
play_sound: boolean, 
/**
 * Ask before "保存" overwrites the record's own file (Java `ui.show-replace-file-hint`).
 */
replace_file_hint: boolean, engine: EngineSettingsView, };

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
 * For each `pv` move, the stones it captures when played out from the node's position
 * (`Branch` with `remove-dead-in-variation`); empty for passes.
 */
pv_captures: Array<Array<Point>>, };

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
candidates: Array<CandidateView>, };

export type HistoryPoint = { node: number, move_number: number, to_play: Stone, visits: number, 
/**
 * Side to move, 0–100.
 */
winrate: number, score_lead: number | null, };

export type EngineFailureView = { engine_name: string, 
/**
 * Launch command with credentials masked.
 */
command: string, during_startup: boolean, message: string, 
/**
 * Last lines the engine printed.
 */
output: Array<string>, };

export type EngineNotice = "StoppedByLimit";

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
 * Stored results along the current line, its continuation and the main trunk, root first
 * (the win-rate graph's data).
 */
history: Array<HistoryPoint>, 
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
rules_prompt: RulesPromptView | null, java_import: JavaImportView, };

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

export type ImportReason = "NotYetAvailable" | "Credential" | "InvalidValue" | "WindowCoordinates" | "RemoteEngine" | "CredentialNotConfirmed" | "Unreadable";

export type ImportTarget = "ReadKomi" | "LoadSgfLast" | "ReplaceFileHint" | "Language" | "PlaySound" | "RecentFiles" | "LastFolder" | "Engines" | "EngineStartup" | "LastEngine" | "AnalysisInterval" | "LimitTime" | "MaxAnalyzeTime" | "LimitPlayout" | "LimitPlayouts" | "LimitMaxSuggestion" | "LimitBranchLength" | "FastEngineChange" | "PonderLimitTips" | "KataRules" | "AutoLoadKataRules";

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
 * Java fields of this entry this product does not use yet (thread policy, GTP profile, …).
 */
unmigrated_fields: Array<string>, };

export type ImportPreviewView = { 
/**
 * Answer with [`Command::ApplyJavaImport`].
 */
id: number, config_file: string, persist_file: string | null, program_dir: string | null, items: Array<ImportItemView>, engines: Array<ImportEngineView>, 
/**
 * Settings already chosen in this product that the import would replace.
 */
replaces: Array<ImportTarget>, };

export type ImportReportView = { config_file: string, items: Array<ImportItemView>, engines: Array<ImportEngineView>, 
/**
 * The imported language applies after a restart.
 */
restart_required: boolean, };

export type ImportFailure = { "type": "NoConfig", path: string, } | { "type": "Unreadable", path: string, detail: string, } | { "type": "Invalid", path: string, detail: string, };
