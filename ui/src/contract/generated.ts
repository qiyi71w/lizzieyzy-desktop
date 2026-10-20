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

export type Command = { "type": "ChooseAndOpenRecord" } | { "type": "OpenRecord", path: string, source: OpenSource, } | { "type": "PasteRecord", confirmed: boolean, } | { "type": "SaveRecord", confirmed: number | null, suggested_name: string, } | { "type": "ChooseAndSaveRecord", suggested_name: string, } | { "type": "SaveRecordTo", path: string, } | { "type": "SetReplaceFileHint", enabled: boolean, } | { "type": "Navigate", nav: Navigation, } | { "type": "GotoNode", node: number, } | { "type": "GotoMoveNumber", number: number, } | { "type": "SetReadKomi", enabled: boolean, } | { "type": "SetLanguage", language: Language, } | { "type": "SetPlaySound", enabled: boolean, } | { "type": "ClearPersonalData" } | { "type": "ResetUiPosition" } | { "type": "OpenLogFolder" } | { "type": "ExportDiagnostics" } | { "type": "ToggleAnalysis" } | { "type": "SwitchEngine", index: number, } | { "type": "RestartEngine" } | { "type": "ShutdownEngines", scope: EngineScope, } | { "type": "SaveEngineSettings", engines: Array<EngineProfile>, startup: EngineStartup, } | { "type": "SetAnalysisSettings", settings: AnalysisSettings, } | { "type": "SetKomi", komi: number, } | { "type": "SetKataRules", rules: string, auto_load: boolean, } | { "type": "SendGtpCommand", command: string, } | { "type": "SetFullTrace", enabled: boolean, } | { "type": "SetPonderLimitTips", enabled: boolean, } | { "type": "AnswerRulesPrompt", prompt: number, accept: boolean, } | { "type": "ChooseJavaFolder", program: boolean, } | { "type": "PreviewJavaImport", config_dir: string, program_dir: string | null, } | { "type": "ApplyJavaImport", preview: number, copy_credentials: Array<number>, replace: boolean, } | { "type": "DismissJavaImportOffer" } | { "type": "Resource", op: ResourceCommand, } | { "type": "Tuning", op: TuningCommand, };

export type CommandResult = { "type": "Done" } | { "type": "Unchanged" } | { "type": "OpenStarted", ticket: number, } | { "type": "NeedsConfirmation" } | { "type": "Exported", path: string, } | { "type": "Refused", message_key: string, } | { "type": "SaveStarted", ticket: number, } | { "type": "NeedsReplaceConfirmation", document: number, file_name: string, } | { "type": "NeedsOverwriteConfirmation", path: string, } | { "type": "FolderChosen", path: string, } | { "type": "JavaImportPreview", preview: ImportPreviewView, } | { "type": "JavaImportFailed", failure: ImportFailure, } | { "type": "JavaImported", report: ImportReportView, } | { "type": "Resource", result: ResourceResult, } | { "type": "Tuning", result: TuningResult, };

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
output: Array<string>, 
/**
 * Missing store-owned NVIDIA runtime components, if this managed profile can be repaired.
 */
repair?: ResourceRepairView, };

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
tuning: TuningView | null, };

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

export type ImportTarget = "ReadKomi" | "LoadSgfLast" | "ReplaceFileHint" | "Language" | "PlaySound" | "RecentFiles" | "LastFolder" | "Engines" | "EngineStartup" | "LastEngine" | "AnalysisInterval" | "LimitTime" | "MaxAnalyzeTime" | "LimitPlayout" | "LimitPlayouts" | "LimitMaxSuggestion" | "LimitBranchLength" | "FastEngineChange" | "PonderLimitTips" | "KataRules" | "AutoLoadKataRules" | "StartupBenchmark" | "NetworkProxy";

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
replaces: Array<ImportTarget>, };

export type ImportReportView = { config_file: string, items: Array<ImportItemView>, engines: Array<ImportEngineView>, 
/**
 * The imported language applies after a restart.
 */
restart_required: boolean, };

export type ImportFailure = { "type": "NoConfig", path: string, } | { "type": "Unreadable", path: string, detail: string, } | { "type": "Invalid", path: string, detail: string, };

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
