// 独立棋局控制面板 (Controller.java) window ↔ main window contract. The window only shows its eight
// buttons and reports which one was pressed; the main window runs the browsing command.
import type { Language } from "../contract/generated";

/** Main window → window: the session's UI language (i18n.setLanguage). */
export const CONTROL_DATA_EVENT = "control-panel-data";
/** Window → main window. */
export const CONTROL_INPUT_EVENT = "control-panel-input";
/** src-tauri control_panel CLOSED_EVENT: the panel's close box hid it. */
export const CONTROL_CLOSED_EVENT = "control-panel-closed";

export interface ControlPayload {
  language: Language;
}

/** The buttons in Java's GridLayout(4, 2) order. */
export type ControlButton = "first" | "last" | "back5" | "forward5" | "back1" | "forward1" | "backToMain" | "painting";

export interface ControlReport {
  /** The window's content size. */
  width: number;
  height: number;
  /** Each button's text, font size and centre in the window's client coordinates. */
  buttons: { key: string; text: string; fontSize: number; x: number; y: number; width: number; height: number }[];
}

export type ControlInput =
  | { type: "ready" }
  | { type: "button"; button: ControlButton }
  | { type: "report"; report: ControlReport };
