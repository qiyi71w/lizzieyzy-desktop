// 黑白胜率大字 (Java BigWinrateWindow) window ↔ main window contract. The main window turns the core's
// win-rate reading into the two figures and sends them, with the style, 窗口置顶 and the fonts, only
// when something the window shows changed; the window draws them and reports its right-click menu
// choices and what it drew.
import type { BigWinrateStyle, Language, WinrateReading } from "../contract/generated";

/** Main window → window: what to draw. */
export const BIG_WINRATE_DATA_EVENT = "big-winrate-data";
/** Window → main window. */
export const BIG_WINRATE_INPUT_EVENT = "big-winrate-input";
/** src-tauri big_winrate CLOSED_EVENT: the window's close box closed it. */
export const BIG_WINRATE_CLOSED_EVENT = "big-winrate-closed";

/** The two figures as Java shows them (`BigWinratePanel.figure`): each side's whole percent, the two
 *  making 100, or "--" for both without analysis; `dim` while they are not the current
 *  position's own (an earlier position's, or none at all: Java `WinrateReading.current`). */
export interface BigWinrateFigures {
  black: string;
  white: string;
  dim: boolean;
}

export function bigWinrateFigures(reading: WinrateReading | null): BigWinrateFigures {
  if (!reading) return { black: "--", white: "--", dim: true };
  // Java Math.round: half up.
  const black = Math.floor(reading.black + 0.5);
  return { black: String(black), white: String(100 - black), dim: !reading.current };
}

export interface BigWinratePayload {
  figures: BigWinrateFigures;
  style: BigWinrateStyle;
  onTop: boolean;
  language: Language;
  /** LizzieFrame.winrateFont's and uiFont's faces as the main window resolves them. */
  figureFont: string;
  labelFont: string;
  /** 界面字体大小 of this session, for the right-click menu. */
  frameFontSize: number;
}

export interface BigWinrateReport {
  width: number;
  height: number;
  style: BigWinrateStyle;
  /** Draws so far: one per payload, size or colour change that altered the picture. */
  paints: number;
  cards: BigWinrateCardReport[];
  menu: BigWinrateMenuReport | null;
}

export interface BigWinrateCardReport {
  side: "black" | "white";
  card: Box;
  label: { text: string; box: Box };
  figure: { text: string; dim: boolean; box: Box };
}

export interface BigWinrateMenuReport {
  box: Box;
  items: { key: string; text: string; checked: boolean; box: Box; textWidth: number; textBox: Box }[];
}

export interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type BigWinrateInput =
  | { type: "ready" }
  | { type: "style"; style: BigWinrateStyle }
  | { type: "onTop"; onTop: boolean }
  | { type: "report"; report: BigWinrateReport };
