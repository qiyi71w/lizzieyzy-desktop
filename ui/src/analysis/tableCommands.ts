// 面板 > 独立选点列表(U) (Menu.java:1420, Input.java:371-373 toggleBestMoves): the AnalysisFrame
// windows' life (open, close, 总在最前 with Q, reopened at start) and the relay between them and
// the main window: the main window formats their rows from the core view (analysis/table.ts) and
// applies their input to the boards. In 双引擎模式 U opens a second window for the secondary
// engine next to the first (LizzieFrame.java:3245-3272, AnalysisFrame index 2); src-tauri places
// both and closes both from either close box.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { navigate, run, state, unavailable } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import type { DisplaySettings, WorkbenchView } from "../contract/generated";
import { t } from "../i18n";
import { FRAME_DATA_EVENT, FRAME_INPUT_EVENT, FRAME_LABEL, SUB_FRAME_INPUT_EVENT, SUB_FRAME_LABEL, type FrameInput, type FramePayload } from "./suggestionsFrame";
import { FRAME_WIDTHS, frameClick, frameHover, frameLeave, frameNine, framePayload, frameSortBy, frameWheel, resetFrame, type FrameId } from "./table";

/** The frame's own close box (src-tauri SUGGESTIONS_CLOSED_EVENT): either window closes both. */
const FRAME_CLOSED_EVENT = "suggestions-frame-closed";

/** An open window: its column layout, the widths it lays out with, the document it was built
 *  for, the title it was built with (AnalysisFrame.oriTitle) and the payload last sent. */
interface OpenFrame {
  nine: boolean;
  layout: number;
  widths: number[] | null;
  columns: number[];
  document: number;
  title: string;
  lastSent: string;
}

const windows: Record<FrameId, OpenFrame | null> = { frame: null, sub: null };
const LABELS: Record<FrameId, string> = { frame: FRAME_LABEL, sub: SUB_FRAME_LABEL };
let opening = false;
let layouts = 0;
/** The layout mode the windows were last arranged for. */
let arrangedFor: WorkbenchView["settings"]["extra_mode"] | null = null;

/** AnalysisFrame title (oriTitle, AnalysisFrame.java:64-67), fixed when the window is built:
 *  in 双引擎模式 主引擎 / 副引擎, otherwise the plain one. */
function baseTitle(id: FrameId, v: WorkbenchView): string {
  if (v.settings.extra_mode !== "DoubleEngine") return t("AnalysisFrame.title");
  return id === "frame" ? t("AnalysisFrame.titleMain") : t("AnalysisFrame.titleSub");
}

/** setTopTitle: Lizzie.alwaysOnTopTitle before the title while on top. */
function shownTitle(base: string, onTop: boolean): string {
  return (onTop ? t("Lizzie.alwaysOnTopTitle") : "") + base;
}

/** Creates a window, or rebuilds the open one, for the column count its engine has now
 *  (AnalysisFrame constructor): the core places it at that layout's saved bounds; the secondary
 *  window then moves to the primary one's right edge. */
async function createWindow(id: FrameId, v: WorkbenchView): Promise<void> {
  const nine = frameNine(v, id);
  const onTop = v.settings.display.suggestionsOnTop;
  const title = baseTitle(id, v);
  const saved = await invoke<number[]>(id === "frame" ? "open_suggestions_frame" : "open_suggestions_sub", { nine, onTop, title: shownTitle(title, onTop) });
  const count = nine ? 9 : 7;
  // An open window is only moved by the core: it takes the title it is rebuilt with here.
  await (await WebviewWindow.getByLabel(LABELS[id]))?.setTitle(shownTitle(title, onTop));
  resetFrame(id, false);
  windows[id] = { nine, layout: ++layouts, widths: saved.length === count ? saved : FRAME_WIDTHS.slice(0, count), columns: [], document: v.document.id, title, lastSent: "" };
}

/** toggleBestMoves opening: the primary window, then in 双引擎模式 the secondary one beside it. */
async function openFrames(): Promise<void> {
  const v = state.view;
  if (!v || windows.frame || opening) return;
  opening = true;
  try {
    await createWindow("frame", v);
    if (v.settings.extra_mode === "DoubleEngine" && !windows.sub) await createWindow("sub", v);
    if (!v.settings.display.suggestionsFrame) setDisplay({ suggestionsFrame: true });
  } finally {
    opening = false;
  }
}

/** Closing (toggleBestMoves, either close box): both windows go, the point is cleared and the
 *  state remembered. */
async function closeFrames(fromWindow: boolean): Promise<void> {
  if (!windows.frame && !windows.sub) return;
  const hadSub = windows.sub !== null;
  windows.frame = null;
  windows.sub = null;
  if (!fromWindow) await invoke("close_suggestions_frame");
  if (hadSub) resetFrame("sub", true);
  resetFrame("frame", true);
  if (state.view?.settings.display.suggestionsFrame) setDisplay({ suggestionsFrame: false });
}

export function toggleFrame(): void {
  void (windows.frame ? closeFrames(false) : openFrames());
}

/** toggleAnalysisFrameAlwaysontop (LizzieFrame.java:3221-3243), Q in a frame: both windows. */
async function toggleOnTop(): Promise<void> {
  if (!windows.frame) return;
  const onTop = !state.view?.settings.display.suggestionsOnTop;
  setDisplay({ suggestionsOnTop: onTop });
  for (const id of ["frame", "sub"] as const) {
    const open = windows[id];
    if (!open) continue;
    const window = await WebviewWindow.getByLabel(LABELS[id]);
    await window?.setAlwaysOnTop(onTop);
    await window?.setTitle(shownTitle(open.title, onTop));
  }
}

/** Rebuilds the open windows (toggleBestMoves twice): the point cleared, each window written,
 *  placed and laid out again for the column count now, unsorted, unselected. */
async function rebuild(v: WorkbenchView): Promise<void> {
  resetFrame("frame", true);
  await createWindow("frame", v);
  if (windows.sub) {
    resetFrame("sub", true);
    await createWindow("sub", v);
  }
}

/** A box toggles its setting and rebuilds the windows. */
function toggleBox(key: FramePayload["boxes"][number]["key"]): void {
  const v = state.view;
  if (!v || !windows.frame) return;
  const field = ({ list: "bestMovesList", graph: "bestMovesGraph", showNext: "frameShowNext", mouseMove: "frameMouseMove" } as const)[key];
  setDisplay({ [field]: !v.settings.display[field] } as Partial<DisplaySettings>);
  void rebuild(v);
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

function frameInput(id: FrameId, input: FrameInput): void {
  const v = state.view;
  const open = windows[id];
  if (!v || !open) return;
  switch (input.type) {
    case "ready":
      open.lastSent = "";
      if (open.columns.length) open.widths = open.columns;
      open.layout = ++layouts;
      break;
    case "hover":
      frameHover(v, id, input.row);
      break;
    case "leave":
      frameLeave(v, id);
      break;
    case "click":
      frameClick(id, input.row, input.right);
      break;
    case "wheel":
      frameWheel(id, input.down);
      break;
    case "sort":
      frameSortBy(id, input.column);
      break;
    case "key":
      frameKey(input.code);
      break;
    case "box":
      toggleBox(input.key);
      break;
    case "columns":
      open.columns = input.widths;
      // Only the primary window's layout is written (Config.persist).
      if (id === "frame") void invoke("set_suggestions_columns", { columns: input.widths });
      break;
  }
}

/** A layout mode change (LizzieFrame.extraMode): entering 双引擎模式 with the window open rebuilds
 *  it as the 主引擎 window and opens the 副引擎 one (toggleBestMoves twice, 3340-3347); leaving
 *  closes the secondary window (3292-3294). */
function arrange(v: WorkbenchView): void {
  const mode = v.settings.extra_mode;
  const before = arrangedFor;
  arrangedFor = mode;
  if (before === null || before === mode || opening) return;
  if (mode === "DoubleEngine" && windows.frame) {
    void rebuild(v).then(() => (windows.sub ? undefined : createWindow("sub", v)));
  } else if (mode !== "DoubleEngine" && windows.sub) {
    windows.sub = null;
    resetFrame("sub", true);
    void invoke("close_suggestions_sub");
  }
}

/** Sends the open windows what they show now; called with every main-window render. Another
 *  record rebuilds them for the column count that record has (resetMovelistFrameandAnalysisFrame,
 *  LizzieFrame.java:5192-5193, 11162-11194: toggleBestMoves twice). */
export function relayFrame(v: WorkbenchView): void {
  arrange(v);
  if (windows.frame && windows.frame.document !== v.document.id) {
    windows.frame.document = v.document.id;
    void rebuild(v);
    return;
  }
  for (const id of ["frame", "sub"] as const) {
    const open = windows[id];
    if (!open) continue;
    const payload = framePayload(v, id, open.nine, open.layout, open.widths);
    const text = JSON.stringify(payload);
    if (text === open.lastSent) continue;
    open.lastSent = text;
    void emitTo(LABELS[id], FRAME_DATA_EVENT, payload);
  }
}

/** U (Input.java:371-373), with any modifier. */
export function handleTableKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyU" || e.metaKey) return false;
  toggleFrame();
  return true;
}

/** Listens to the windows and reopens them when they were open at the last exit
 *  (EngineManager.java:758). */
export async function startFrame(repaint: () => void): Promise<void> {
  arrangedFor = state.view?.settings.extra_mode ?? null;
  await listen<FrameInput>(FRAME_INPUT_EVENT, (e) => {
    frameInput("frame", e.payload);
    repaint();
  });
  await listen<FrameInput>(SUB_FRAME_INPUT_EVENT, (e) => {
    frameInput("sub", e.payload);
    repaint();
  });
  await listen(FRAME_CLOSED_EVENT, () => {
    void closeFrames(true).then(repaint);
  });
  if (state.view?.settings.display.suggestionsFrame) await openFrames();
}

/** What the acceptance probe reports of the primary window. */
export function frameReport() {
  const open = windows.frame;
  return {
    open: open !== null,
    onTop: !!open && !!state.view?.settings.display.suggestionsOnTop,
    columns: open ? (open.nine ? 9 : 7) : null,
    widths: open?.columns ?? null,
  };
}

/** Which windows are open, and the secondary one's title (acceptance probe). */
export function framesReport() {
  const sub = windows.sub;
  return {
    main: windows.frame !== null,
    sub: sub !== null,
    subTitle: sub ? shownTitle(sub.title, !!state.view?.settings.display.suggestionsOnTop) : null,
  };
}

Object.assign(COMMANDS, {
  "menu.SuggestionList": { run: toggleFrame, checked: () => windows.frame !== null },
} satisfies Record<string, CommandEntry>);
