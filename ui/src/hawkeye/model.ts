// 超级鹰眼 (MoveListFrame.java) window ↔ main window contract. The main window relays the core
// projection (WorkbenchView.hawkeye, one pane per window) with the settings it needs; the window
// draws it and reports input back. Business state stays in the core: the window holds only page
// layout, sort and hover state and the drafts of its open dialogs.
import type {
  Command,
  CoordinateStyle,
  DisplaySettings,
  HawkeyePane,
  HawkeyeSettings,
  Language,
  Point,
} from "../contract/generated";

/** The primary window and 双引擎模式's secondary engine window (src-tauri). */
export const HAWKEYE_LABEL = "hawkeye";
export const HAWKEYE_SUB_LABEL = "hawkeye-sub";
/** Main window → window (sent to each window by its label). */
export const HAWKEYE_DATA_EVENT = "hawkeye-data";
/** Window → main window, one event name per window. */
export const HAWKEYE_INPUT_EVENT = "hawkeye-input";
export const HAWKEYE_SUB_INPUT_EVENT = "hawkeye-sub-input";

/** What one window draws. */
export interface HawkeyePayload {
  /** The UI language of this session (i18n.setLanguage). */
  language: Language;
  /** Java `Lizzie.config.isChinese`: picks the Chinese / English control bounds. */
  chinese: boolean;
  /** 双引擎模式's secondary engine window (MoveListFrame(2)). */
  sub: boolean;
  settings: HawkeyeSettings;
  /** 落子评价 loss thresholds (`win-loss-threshold-1..5`, `score-loss-threshold-1..5`; negative). */
  winLoss: number[];
  scoreLoss: number[];
  /** 吻合率条件 (`match-ai-moves`, `match-ai-percents-playouts`). */
  matchAiMoves: number;
  matchAiPercents: number;
  /** 黑 / 白 boxes (session state in the core). */
  black: boolean;
  white: boolean;
  /** 统计 range in effect (moves after `firstMove` up to `lastMove`). */
  firstMove: number;
  lastMove: number;
  /** Move number of the browsing position. */
  currentMove: number;
  /** The record shown (`DocumentView.id`); navigation commands name it. */
  document: number;
  /** The browsing position's node; whether it is on the main trunk. */
  currentNode: number;
  onMainTrunk: boolean;
  /** clickbadmove while the position before it is browsed. */
  mark: Point | null;
  pane: HawkeyePane;
  /** LizzieFrame.getPlayerName(isBlack, 12). */
  blackName: string;
  whiteName: string;
  /** Board size and 坐标格式 for coordinate names. */
  width: number;
  height: number;
  coordinateStyle: CoordinateStyle;
  /** 盘面 (`show-katago-score-lead-with-komi`), for the chart's score line label. */
  scoreWithKomi: boolean;
  /** Identifies one opening of the window: the sort column and widths below apply to it. */
  layout: number;
  /** badmoves-list-position[0] and the saved 详细列表 widths (empty: Java's preferred widths). */
  sort: number;
  columns: number[];
  /** Lizzie.config.uiFontName / fontName as the main window resolved them (CSS stacks). */
  fonts: { ui: string; info: string };
  /** Config.menuHeight of this session: the lists' row height (MoveListFrame.java table rows). */
  menuHeight: number;
  /** Config.frameFontSize of this session. */
  frameFontSize: number;
}

/** A window's input. */
export type HawkeyeInput =
  | { type: "ready" }
  /** A core command (navigation, side boxes, settings, table export). */
  | { type: "run"; command: Command }
  /** Y, Q (MoveListFrame key listeners), arrows / Space the main window handles like its own keys. */
  | { type: "key"; code: string }
  /** The sort column (sortnum) and the 详细列表's laid-out column widths. */
  | { type: "layout"; sort: number; columns: number[] }
  /** A change of the display settings the window edits: 统计阈值 (`winLossThresholds`,
   *  `scoreLossThresholds`) and 吻合率条件 (`matchAiMoves`, `matchAiPercentsPlayouts`). */
  | { type: "display"; change: Partial<DisplaySettings> }
  | { type: "report"; report: HawkeyeReport };

export interface HawkeyeReport {
  topPage: number;
  chart: number;
  rows: string[][];
  dialog: string | null;
  targets: { key: string; text: string; checked?: boolean; value?: string; x: number; y: number }[];
}

/** One `HawkeyeSettings` change, the rest as it is. */
export function settingsCommand(settings: HawkeyeSettings, change: Partial<HawkeyeSettings>): Command {
  return { type: "SetHawkeyeSettings", settings: { ...settings, ...change } };
}

/** Java `String.format(Locale.ENGLISH, "%.Nf", v)`: NaN and infinities print as Java does. */
export function fixed(v: number, digits: number): string {
  if (Number.isNaN(v)) return "NaN";
  if (v === Infinity) return "Infinity";
  if (v === -Infinity) return "-Infinity";
  return v.toFixed(digits);
}

/** Bar / text colours shared by the statistics panel, the dialogs and the cell renderers. */
export const LOSS_COLORS = ["rgb(22,222,0)", "rgb(158,232,34)", "rgb(222,222,0)", "rgb(200,140,50)", "rgb(172,16,19)", "rgb(85,25,80)"];

/** Config.sysDefaultFontName (Dialog) as the main window resolves it. */
export const UI_FONT = navigator.userAgent.includes("Windows")
  ? 'Arial, "Microsoft YaHei UI", "Microsoft YaHei", sans-serif'
  : navigator.userAgent.includes("Mac OS")
    ? '"Lucida Grande", "PingFang SC", sans-serif'
    : '"DejaVu Sans", "Noto Sans CJK SC", sans-serif';
