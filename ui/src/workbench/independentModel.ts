// 浮动主棋盘 / 浮动小棋盘 (IndependentMainBoard / IndependentSubBoard) window ↔ main window contract.
// The main window owns the projection and every interaction state (hovered candidate, the sub
// board's candidate and branch length): it sends each window the scene to paint and applies the
// pointer and key input the window reports. A window keeps only its own frame state (lock,
// always-on-top, the press it is dragging the window with).
import type { IndependentBoard, Language, Point } from "../contract/generated";
import type { BoardScene, SubBoardScene } from "./paint";

/** Window labels (src-tauri INDEPENDENT_MAIN_WINDOW / INDEPENDENT_SUB_WINDOW). */
export const INDEPENDENT_LABELS: Record<IndependentBoard, string> = { Main: "independent-main", Sub: "independent-sub" };
/** Main window → window. */
export const INDEPENDENT_DATA_EVENT = "independent-board-data";
/** Window → main window. */
export const INDEPENDENT_INPUT_EVENT = "independent-board-input";

/** BoardScene over the wire: the kept-beyond-limit points as an array. */
export type WireBoardScene = Omit<BoardScene, "unlimited"> & { unlimited: string[] };
export type IndependentPayload = {
  language: Language;
  /** The lock button (independent-mainboard-locked / independent-subboard-locked): no moving
   *  or resizing by dragging. */
  locked: boolean;
  /** The always-on-top button's state (independent-main-board-top / -sub-board-top). */
  onTop: boolean;
} & (
  | {
      board: "Main";
      /** BoardRenderer.isBigMargin for this scene (0.055 margin instead of 0.03). */
      bigMargin: boolean;
      /** 启用拖动棋子: a press on a stone drags the stone instead of the window. */
      allowDrag: boolean;
      scene: WireBoardScene;
    }
  | { board: "Sub"; scene: SubBoardScene }
);

/** The key fields a forwarded KeyboardEvent is rebuilt from. */
export interface KeyInit {
  key: string;
  code: string;
  altKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
  repeat: boolean;
}

export type IndependentInput = { board: IndependentBoard } & (
  | { type: "ready" }
  /** The pointer over the main board, in intersection units from the top-left intersection, and
   *  the stone radius + 1 px in the same units (BoardRenderer.mouseOveredMove's reach). */
  | { type: "hover"; fx: number; fy: number; reach: number }
  | { type: "enter" }
  | { type: "leave" }
  /** `sx`, `sy`: the pressed screen point (CSS px), where a popup opens. */
  | { type: "press"; point: Point | null; button: number; detail: number; alt: boolean; sx: number; sy: number }
  | { type: "release"; point: Point | null; button: number }
  | { type: "wheel"; down: boolean }
  | { type: "key"; kind: "keydown" | "keyup"; init: KeyInit }
  /** The window's close button (btnClose): the same as closing the window. */
  | { type: "close" }
  /** The lock / always-on-top buttons. */
  | { type: "lock" }
  | { type: "top" }
);
