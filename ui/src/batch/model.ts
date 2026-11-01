// 批量分析进度表 (AnalysisTable.java) window ↔ main window contract. The main window relays the
// core's queue (WorkbenchView.batch) and whether automatic analysis runs; the window draws them
// and reports its buttons back, which the main window turns into commands. The window keeps no
// queue state of its own.
import type { BatchItemView, BatchMove, BatchRunView, Language } from "../contract/generated";

/** src-tauri BATCH_WINDOW. */
export const BATCH_LABEL = "batch-table";
/** Main window → window. */
export const BATCH_DATA_EVENT = "batch-table-data";
/** Window → main window. */
export const BATCH_INPUT_EVENT = "batch-table-input";

export interface BatchPayload {
  /** The UI language of this session (i18n.setLanguage). */
  language: Language;
  /** The rows: the current record first. */
  items: BatchItemView[];
  running: BatchRunView | null;
  /** Lizzie.config.isAutoAna || isManualAutoAnalysisStarting. */
  autoAnalysis: boolean;
}

/** The top buttons, in Java's order. */
export type BatchButton = "stopGo" | "stopStart" | "stopStartAnalysisMode" | "addFile" | "clearAllFiles";

export interface BatchReport {
  rows: string[][];
  targets: { key: string; text: string; enabled: boolean; x: number; y: number }[];
}

export type BatchInput =
  | { type: "ready" }
  | { type: "button"; button: BatchButton }
  /** 优先 / 上移 / 下移 / 删除 of the row showing `item`. */
  | { type: "row"; item: number; action: BatchMove | "Remove" }
  | { type: "report"; report: BatchReport };
