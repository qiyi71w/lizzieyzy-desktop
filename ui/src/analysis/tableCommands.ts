// 面板 > 独立选点列表(U) (Menu.java:1420, Input.java:371-373 toggleBestMoves): the AnalysisFrame
// window's life (open, close, 总在最前 with Q, reopened at start) and the relay between it and
// the main window: the main window formats its rows from the core view (analysis/table.ts) and
// applies the frame's input to the board.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { navigate, run, state, unavailable } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import type { DisplaySettings, WorkbenchView } from "../contract/generated";
import { t } from "../i18n";
import { FRAME_DATA_EVENT, FRAME_INPUT_EVENT, type FrameInput, type FramePayload } from "./suggestionsFrame";
import { FRAME_WIDTHS, frameClick, frameHover, frameLeave, frameNine, framePayload, frameSortBy, frameWheel, resetFrame } from "./table";

const FRAME_LABEL = "suggestions";
/** The frame's own close box (src-tauri SUGGESTIONS_CLOSED_EVENT). */
const FRAME_CLOSED_EVENT = "suggestions-frame-closed";

/** The open frame: its column layout, the widths it lays out with and the document it was built for. */
let frame: { nine: boolean; layout: number; widths: number[] | null; columns: number[]; document: number } | null = null;
let opening = false;
let layouts = 0;
let lastSent = "";

/** AnalysisFrame title, after Lizzie.alwaysOnTopTitle while on top (setTopTitle). */
function frameTitle(onTop: boolean): string {
  return (onTop ? t("Lizzie.alwaysOnTopTitle") : "") + t("AnalysisFrame.title");
}

/** Creates the window, or rebuilds the open one, for the column count the analysis has now
 *  (AnalysisFrame constructor): the core places it at that layout's saved bounds. */
async function createWindow(v: WorkbenchView): Promise<void> {
  const nine = frameNine(v);
  const onTop = v.settings.display.suggestionsOnTop;
  const saved = await invoke<number[]>("open_suggestions_frame", { nine, onTop, title: frameTitle(onTop) });
  const count = nine ? 9 : 7;
  resetFrame(false);
  frame = { nine, layout: ++layouts, widths: saved.length === count ? saved : FRAME_WIDTHS.slice(0, count), columns: [], document: v.document.id };
  lastSent = "";
}

async function openFrame(): Promise<void> {
  const v = state.view;
  if (!v || frame || opening) return;
  opening = true;
  try {
    await createWindow(v);
    if (!v.settings.display.suggestionsFrame) setDisplay({ suggestionsFrame: true });
  } finally {
    opening = false;
  }
}

/** Closing (toggleBestMoves, the close box): the point is cleared and the state remembered. */
async function closeFrame(fromWindow: boolean): Promise<void> {
  if (!frame) return;
  frame = null;
  if (!fromWindow) await invoke("close_suggestions_frame");
  resetFrame(true);
  if (state.view?.settings.display.suggestionsFrame) setDisplay({ suggestionsFrame: false });
}

export function toggleFrame(): void {
  void (frame ? closeFrame(false) : openFrame());
}

/** toggleAnalysisFrameAlwaysontop (LizzieFrame.java:3221-3243), Q in the frame. */
async function toggleOnTop(): Promise<void> {
  if (!frame) return;
  const onTop = !state.view?.settings.display.suggestionsOnTop;
  setDisplay({ suggestionsOnTop: onTop });
  const window = await WebviewWindow.getByLabel(FRAME_LABEL);
  await window?.setAlwaysOnTop(onTop);
  await window?.setTitle(frameTitle(onTop));
}

/** A box toggles its setting and rebuilds the frame (toggleBestMoves twice): the point cleared,
 *  the frame written, placed and laid out again for the column count now, unsorted, unselected. */
function toggleBox(key: FramePayload["boxes"][number]["key"]): void {
  const v = state.view;
  if (!v || !frame) return;
  const field = ({ list: "bestMovesList", graph: "bestMovesGraph", showNext: "frameShowNext", mouseMove: "frameMouseMove" } as const)[key];
  setDisplay({ [field]: !v.settings.display[field] } as Partial<DisplaySettings>);
  resetFrame(true);
  void createWindow(v);
}

function frameKey(code: string): void {
  switch (code) {
    case "KeyU":
      toggleFrame();
      break;
    case "KeyQ":
      void toggleOnTop();
      break;
    case "ArrowUp":
      navigate({ op: "Back", count: 1 });
      break;
    case "ArrowDown":
      navigate({ op: "Forward", count: 1 });
      break;
    case "Space":
      void run({ type: "ToggleAnalysis" });
      break;
    // 恶手列表 (toggleBadMoves) is not available yet.
    case "KeyY":
      unavailable();
      break;
  }
}

function frameInput(input: FrameInput): void {
  const v = state.view;
  if (!v || !frame) return;
  switch (input.type) {
    case "ready":
      lastSent = "";
      if (frame.columns.length) frame.widths = frame.columns;
      frame.layout = ++layouts;
      break;
    case "hover":
      frameHover(v, input.row);
      break;
    case "leave":
      frameLeave(v);
      break;
    case "click":
      frameClick(input.row, input.right);
      break;
    case "wheel":
      frameWheel(input.down);
      break;
    case "sort":
      frameSortBy(input.column);
      break;
    case "key":
      frameKey(input.code);
      break;
    case "box":
      toggleBox(input.key);
      break;
    case "columns":
      frame.columns = input.widths;
      void invoke("set_suggestions_columns", { columns: input.widths });
      break;
  }
}

/** Sends the open frame what it shows now; called with every main-window render. Another record
 *  rebuilds it for the column count that record has (resetMovelistFrameandAnalysisFrame,
 *  LizzieFrame.java:5192-5193, 11162-11194: toggleBestMoves twice). */
export function relayFrame(v: WorkbenchView): void {
  if (!frame) return;
  if (frame.document !== v.document.id) {
    frame.document = v.document.id;
    resetFrame(true);
    void createWindow(v);
    return;
  }
  const payload = framePayload(v, frame.nine, frame.layout, frame.widths);
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(FRAME_LABEL, FRAME_DATA_EVENT, payload);
}

/** U (Input.java:371-373), with any modifier. */
export function handleTableKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyU" || e.metaKey) return false;
  toggleFrame();
  return true;
}

/** Listens to the frame and reopens it when it was open at the last exit (EngineManager.java:758). */
export async function startFrame(repaint: () => void): Promise<void> {
  await listen<FrameInput>(FRAME_INPUT_EVENT, (e) => {
    frameInput(e.payload);
    repaint();
  });
  await listen(FRAME_CLOSED_EVENT, () => {
    void closeFrame(true).then(repaint);
  });
  if (state.view?.settings.display.suggestionsFrame) await openFrame();
}

/** What the acceptance probe reports of the frame window. */
export function frameReport() {
  return {
    open: frame !== null,
    onTop: !!frame && !!state.view?.settings.display.suggestionsOnTop,
    columns: frame ? (frame.nine ? 9 : 7) : null,
    widths: frame?.columns ?? null,
  };
}

Object.assign(COMMANDS, {
  "menu.SuggestionList": { run: toggleFrame, checked: () => frame !== null },
} satisfies Record<string, CommandEntry>);
