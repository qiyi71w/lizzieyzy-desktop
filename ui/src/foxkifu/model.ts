// The relay between the main window and the 野狐棋谱 window (label `fox-kifu`, foxkifu.html).
// The main window owns the only workbench subscription and sends the window its part of every
// view; the window sends its commands itself (dispatch) and reports what it shows.
import type { FoxKifuView, Language } from "../contract/generated";

export const FOX_KIFU_LABEL = "fox-kifu";
/** main → window: FoxKifuPayload. */
export const FOX_KIFU_DATA_EVENT = "fox-kifu-data";
/** window → main: FoxKifuInput. */
export const FOX_KIFU_INPUT_EVENT = "fox-kifu-input";

export interface FoxKifuPayload {
  language: Language;
  view: FoxKifuView;
}

/** What the acceptance probe reports for the window: its visible texts and controls. */
export interface FoxKifuReport {
  account: string;
  byUid: boolean;
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

export type FoxKifuInput = { type: "ready" } | { type: "report"; report: FoxKifuReport };
