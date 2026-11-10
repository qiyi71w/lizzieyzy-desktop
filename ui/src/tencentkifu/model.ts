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
  targets: { key: string; text: string; x: number; y: number; enabled: boolean }[];
}

export type TencentKifuInput = { type: "ready" } | { type: "report"; report: TencentKifuReport };
