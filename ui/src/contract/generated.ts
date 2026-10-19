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

export type Command = { "type": "ChooseAndOpenRecord" } | { "type": "OpenRecord", path: string, source: OpenSource, } | { "type": "PasteRecord", confirmed: boolean, } | { "type": "Navigate", nav: Navigation, } | { "type": "GotoNode", node: number, } | { "type": "GotoMoveNumber", number: number, } | { "type": "SetReadKomi", enabled: boolean, } | { "type": "SetLanguage", language: Language, } | { "type": "SetPlaySound", enabled: boolean, } | { "type": "ClearPersonalData" } | { "type": "ResetUiPosition" } | { "type": "OpenLogFolder" } | { "type": "ExportDiagnostics" };

export type CommandResult = { "type": "Done" } | { "type": "Unchanged" } | { "type": "OpenStarted", ticket: number, } | { "type": "NeedsConfirmation" } | { "type": "Exported", path: string, };

export type ErrorKind = "InvalidArgument" | "Io" | "Unavailable";

export type ApiError = { kind: ErrorKind, message: string, };

export type DocumentView = { 
/**
 * Identity of the open document; changes on every adopted record.
 */
id: number, format: RecordFormat | null, path: string | null, file_name: string | null, size: BoardSize, info: GameInfo, };

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

export type PendingOpen = { ticket: number, path: string | null, };

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
play_sound: boolean, };

export type StoneSound = "Stone" | "DeadStone" | "DeadStoneMore";

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
komi: number, };

export type LogsStatus = { directory: string, app_log_bytes: number | null, crash_log_bytes: number | null, };
