// 浮动主棋盘 / 浮动小棋盘 in the main window: opens and closes the windows to match the panel flags
// (Config.isShowingIndependentMain / isShowingIndependentSub, LizzieFrame.toggleIndependentMainBoard
// / toggleIndependentSubBoard), sends each the scene to paint and applies their input. The core
// owns the flags, the lock / always-on-top preferences and the closing policy; src-tauri owns the
// native windows and their persisted bounds.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { run, state } from "../app";
import type { IndependentBoard, PanelSettings, WorkbenchView } from "../contract/generated";
import { language, t } from "../i18n";
import { INDEPENDENT_DATA_EVENT, INDEPENDENT_INPUT_EVENT, INDEPENDENT_LABELS, type IndependentInput, type IndependentPayload, type WireBoardScene } from "./independentModel";
import type { SubBoardScene } from "./paint";
import { presentation } from "./presentation";

interface FloatingWindow {
  open: boolean;
  opening: boolean;
  lastSent: string;
  /** The always-on-top state last applied, null to apply it again. */
  onTop: boolean | null;
}

const windows: Record<IndependentBoard, FloatingWindow> = {
  Main: { open: false, opening: false, lastSent: "", onTop: null },
  Sub: { open: false, opening: false, lastSent: "", onTop: null },
};
let repaint: () => void = () => {};
/** Compound layout operations in progress: the windows follow only their final state. */
let held = 0;

function flags(board: IndependentBoard, p: PanelSettings) {
  return board === "Main"
    ? { shown: p.showingIndependentMain, locked: p.independentMainLocked, onTop: p.independentMainTop }
    : { shown: p.showingIndependentSub, locked: p.independentSubLocked, onTop: p.independentSubTop };
}

/** Runs several commands as one layout change: the floating windows are not opened or closed
 *  for an intermediate state. */
export async function holdIndependent<T>(work: () => Promise<T>): Promise<T> {
  held++;
  try {
    return await work();
  } finally {
    held--;
    repaint();
  }
}

async function openWindow(board: IndependentBoard): Promise<void> {
  const w = windows[board];
  w.opening = true;
  try {
    await invoke("open_independent_board", {
      board,
      title: board === "Main" ? t("IndependentMainBoard.title") : t("IndependentSubBoard.title"),
    });
    w.open = true;
    w.lastSent = "";
    w.onTop = null;
  } catch (e) {
    console.error(`opening the floating ${board} board failed`, e);
  } finally {
    w.opening = false;
    repaint();
  }
}

/** What the main window sends: the main board's scene and the sub board's, each only while its
 *  window is shown (built by the workbench renderer). */
export interface IndependentScenes {
  main: { scene: WireBoardScene; bigMargin: boolean } | null;
  sub: SubBoardScene | null;
}

/** Opens, closes and feeds the windows for the view (and an open SetCustomMode draft); called
 *  with every main-window render. */
export function relayIndependent(v: WorkbenchView, scenes: IndependentScenes): void {
  if (held > 0) return;
  const p = presentation(v).panels;
  for (const board of ["Main", "Sub"] as const) {
    const w = windows[board];
    const f = flags(board, p);
    if (!f.shown) {
      if (w.open) {
        w.open = false;
        void invoke("close_independent_board_window", { board });
      }
      continue;
    }
    if (!w.open) {
      if (!w.opening) void openWindow(board);
      continue;
    }
    if (w.onTop !== f.onTop) {
      w.onTop = f.onTop;
      void WebviewWindow.getByLabel(INDEPENDENT_LABELS[board]).then((win) => win?.setAlwaysOnTop(f.onTop));
    }
    const common = { language: language(), locked: f.locked, onTop: f.onTop };
    let payload: IndependentPayload;
    if (board === "Main") {
      if (!scenes.main) continue;
      payload = { ...common, board, bigMargin: scenes.main.bigMargin, allowDrag: v.settings.edit.allowDrag, scene: scenes.main.scene };
    } else {
      if (!scenes.sub) continue;
      payload = { ...common, board, scene: scenes.sub };
    }
    const text = JSON.stringify(payload);
    if (text === w.lastSent) continue;
    w.lastSent = text;
    void emitTo(INDEPENDENT_LABELS[board], INDEPENDENT_DATA_EVENT, payload);
  }
}

/** One panel flag of the floating windows, the rest as committed (SetPanelSettings). */
function setFlag(change: Partial<PanelSettings>): void {
  const p = state.view?.settings.panels;
  if (p) void run({ type: "SetPanelSettings", settings: { ...p, ...change } });
}

/** Listens to the windows. `board` handles the pointer input the workbench owns (hover, presses,
 *  wheel, entering and leaving); keys go to the workbench's own key handling. */
export async function startIndependent(board: (input: IndependentInput) => void, schedule: () => void): Promise<void> {
  repaint = schedule;
  await listen<IndependentInput>(INDEPENDENT_INPUT_EVENT, (e) => {
    const input = e.payload;
    const main = input.board === "Main";
    switch (input.type) {
      case "ready":
        windows[input.board].lastSent = "";
        break;
      case "close":
        // btnClose / windowClosing (IndependentMainBoard.java:192-206, IndependentSubBoard.java:150-168).
        void run({ type: "CloseIndependentBoard", board: input.board });
        break;
      case "lock": {
        const p = state.view?.settings.panels;
        if (p) setFlag(main ? { independentMainLocked: !p.independentMainLocked } : { independentSubLocked: !p.independentSubLocked });
        break;
      }
      case "top": {
        const p = state.view?.settings.panels;
        if (p) setFlag(main ? { independentMainTop: !p.independentMainTop } : { independentSubTop: !p.independentSubTop });
        break;
      }
      case "key":
        // InputIndependentMainBoard / InputIndependentSubboard: the frame's own key handling.
        document.dispatchEvent(new KeyboardEvent(input.kind, { ...input.init, bubbles: true, cancelable: true }));
        break;
      default:
        board(input);
    }
    schedule();
  });
}

/** What the acceptance probe reports: which windows are open. */
export function independentReport() {
  return { main: windows.Main.open, sub: windows.Sub.open };
}
