// The relay between the main window and the 腾讯棋谱 window (label `tencent-kifu`,
// tencentkifu.html). The main window owns the only workbench subscription and sends the window
// its part of every view; the window sends its commands itself (dispatch) and reports what it
// shows.
import type { Language, TencentKifuView } from "../contract/generated";

export const TENCENT_KIFU_LABEL = "tencent-kifu";
/** main → window: TencentKifuPayload. */
export const TENCENT_KIFU_DATA_EVENT = "tencent-kifu-data";
/** window → main: TencentKifuInput. */
export const TENCENT_KIFU_INPUT_EVENT = "tencent-kifu-input";

export interface TencentKifuPayload {
  language: Language;
  /** Config.frameFontSize of this session. */
  frameFontSize: number;
  /** Function search's request for 打开棋谱后此窗口, by number, until the window says it located it. */
  locate: number | null;
  view: TencentKifuView;
}

/** What the acceptance probe reports for the window: its visible texts and controls. */
export interface TencentKifuReport {
  query: string;
  afterGet: number;
  currentUser: string;
  recent: string[];
  header: string[];
  /** Shown rows, cell texts in column order (the 打开 column omitted), with their colour. */
  rows: { cells: string[]; won: boolean; chessid: string }[];
  page: string;
  overlay: string | null;
  tableEnabled: boolean;
  message: string | null;
  /** The window's KifuProbe: focus, located control, type size and what is cut off. */
  focus: string;
  located: string | null;
  fontSize: string;
  clipped: string[];
  targets: { key: string; text: string; x: number; y: number; enabled: boolean }[];
  /** How many reports the window has sent, this one included (it sends none before a probe). */
  sent: number;
}

export type TencentKifuInput = { type: "ready" } | { type: "located"; request: number } | { type: "report"; report: TencentKifuReport };
