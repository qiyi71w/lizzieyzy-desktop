// 棋盘同步工具's float board (FloatBoard.java) window ↔ main window contract. The core owns the
// window's existence, geometry, edit mode and hidden suggestions (WorkbenchView.readboard
// .float_board); the main window owns the hovered candidate and its preview depth, paints nothing
// itself and sends the window the scene to paint. The window keeps only its 位置 button state.
import type { AppearanceSettings, Language, Point, ThemeSettings } from "../contract/generated";
import type { WireBoardScene } from "../workbench/independentModel";

/** Window label (src-tauri floatboard::LABEL). */
export const FLOAT_LABEL = "readboard-float";
/** Main window → window. */
export const FLOAT_DATA_EVENT = "readboard-float-data";
/** Window → main window. */
export const FLOAT_INPUT_EVENT = "readboard-float-input";

export interface FloatPayload {
  language: Language;
  appearance: AppearanceSettings;
  theme: ThemeSettings;
  /** SettingsView.theme_files_revision. */
  themeFilesRevision: number;
  /** The window's size in physical pixels (posWidth × posHeight) and the edge between it and the
   *  helper's rectangle (zoomIn(20)), both as src-tauri placed it. */
  width: number;
  height: number;
  pad: number;
  scene: WireBoardScene;
  edit: boolean;
  /** hideSuggestion: the +/- button shows plus. */
  hidden: boolean;
  /** btnEdit is shown (not while both boards are synced). */
  editButton: boolean;
  /** btnStopGo shows tostop while the engine analyses. */
  pondering: boolean;
  /** FloatBoard.boardType, the client the helper reads (0 野狐: the captured-point cross sits 1px
   *  right and down). */
  boardType: number;
  /** 起始局面设置模式: the goban and the stones only (FloatBoardRenderer.java:133-139). */
  setup: boolean;
}

export type FloatButton = "stopGo" | "hideShow" | "edit" | "left" | "right" | "up" | "down";

export type FloatInput =
  | { type: "ready" }
  /** The pointer's intersection (convertScreenToCoordinates), null off the grid. */
  | { type: "move"; point: Point | null }
  | { type: "leave" }
  | { type: "press"; point: Point | null; button: number }
  | { type: "wheel"; down: boolean }
  /** KeyEvent codes FloatBoard's key listener binds (KeyboardEvent.code). */
  | { type: "key"; code: string }
  | { type: "button"; button: FloatButton };
