// 超级鹰眼 entries (Y, 视图 > 面板 > 超级鹰眼, 分析 > 超级鹰眼, the toolbar and bottom bar buttons;
// LizzieFrame.toggleBadMoves) and the relay between the core and its windows. The core owns
// whether it is open (`show-badmoves-frame`, reopened at start); this module opens and closes
// the windows to match WorkbenchView.hawkeye, sends each window its pane and applies the
// windows' input. In 双引擎模式 a second window shows the secondary engine's results
// (MoveListFrame(2), LizzieFrame.java:3537-3563).
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { run, state } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import type { HawkeyePane, WorkbenchView } from "../contract/generated";
import { isChinese, language, t } from "../i18n";
import {
  HAWKEYE_DATA_EVENT,
  HAWKEYE_INPUT_EVENT,
  HAWKEYE_LABEL,
  HAWKEYE_SUB_INPUT_EVENT,
  HAWKEYE_SUB_LABEL,
  settingsCommand,
  type HawkeyeInput,
  type HawkeyePayload,
  type HawkeyeReport,
} from "./model";

/** src-tauri HAWKEYE_CLOSED_EVENT: a window's own close box closed both. */
const CLOSED_EVENT = "hawkeye-closed";

type Id = "primary" | "sub";
const LABELS: Record<Id, string> = { primary: HAWKEYE_LABEL, sub: HAWKEYE_SUB_LABEL };

/** An open window: the layout it shows (a new one per opening or record), its saved sort and
 *  widths, its title and the payload last sent. */
interface Open {
  layout: number;
  document: number;
  sort: number;
  columns: number[];
  title: string;
  lastSent: string;
  report: HawkeyeReport | null;
}

const windows: Record<Id, Open | null> = { primary: null, sub: null };
/** Windows being created (the core view may arrive again meanwhile). */
const opening: Record<Id, boolean> = { primary: false, sub: false };
let layouts = 0;

/** setTopTitle (MoveListFrame.java:254-263, 7079-7083). */
function title(id: Id, v: WorkbenchView): string {
  const engine = v.settings.extra_mode !== "DoubleEngine" ? "" : id === "primary" ? t("Movelistframe.titleMain") : t("Movelistframe.titleSub");
  const base = t("Movelistframe.title") + engine + t("Movelistframe.titleHint");
  return (v.settings.hawkeye.alwaysOnTop ? t("Lizzie.alwaysOnTopTitle") : "") + base;
}

async function openWindow(id: Id, v: WorkbenchView): Promise<void> {
  if (windows[id] || opening[id]) return;
  opening[id] = true;
  try {
    const [sort, columns] = await invoke<[number, number[]]>("open_hawkeye", {
      sub: id === "sub",
      chinese: isChinese(),
      onTop: v.settings.hawkeye.alwaysOnTop,
      title: title(id, v),
    });
    windows[id] = { layout: ++layouts, document: v.document.id, sort, columns, title: title(id, v), lastSent: "", report: null };
  } finally {
    opening[id] = false;
  }
}

/** toggleBadMoves: the core opens or closes it; the windows follow the next view. */
export function toggleHawkeye(): void {
  void run({ type: "Hawkeye", op: { type: "Toggle" } });
}

/** togglealwaysontop (MoveListFrame.java:7065-7077), Q in either window: both windows. */
function toggleOnTop(): void {
  const v = state.view;
  if (!v?.hawkeye) return;
  void run(settingsCommand(v.settings.hawkeye, { alwaysOnTop: !v.settings.hawkeye.alwaysOnTop }));
}

/** LizzieFrame.getPlayerName(isBlack, 12): longer names keep 11 characters; empty ones are 黑 / 白. */
function playerName(name: string, black: boolean): string {
  const shown = name.length > 12 ? name.substring(0, 11) : name;
  return shown || (black ? t("SGFParse.black") : t("SGFParse.white"));
}

function payload(id: Id, open: Open, v: WorkbenchView, pane: HawkeyePane): HawkeyePayload {
  const h = v.hawkeye!;
  const d = v.settings.display;
  return {
    language: language(),
    chinese: isChinese(),
    sub: id === "sub",
    settings: v.settings.hawkeye,
    winLoss: d.winLossThresholds,
    scoreLoss: d.scoreLossThresholds,
    matchAiMoves: d.matchAiMoves,
    matchAiPercents: d.matchAiPercentsPlayouts,
    black: h.black,
    white: h.white,
    firstMove: h.first_move,
    lastMove: h.last_move,
    currentMove: h.current_move,
    document: v.document.id,
    currentNode: v.position.node,
    onMainTrunk: v.position.main_trunk,
    mark: h.mark,
    pane,
    blackName: playerName(v.document.info.black, true),
    whiteName: playerName(v.document.info.white, false),
    width: v.document.size.width,
    height: v.document.size.height,
    coordinateStyle: d.coordinateStyle,
    scoreWithKomi: d.scoreLeadWithKomi,
    layout: open.layout,
    sort: open.sort,
    columns: open.columns,
  };
}

/** Opens, retitles, closes and feeds the windows for the view; called with every main-window
 *  render. Another record lays the lists out again (resetMovelistFrameandAnalysisFrame). */
export function relayHawkeye(v: WorkbenchView): void {
  const h = v.hawkeye;
  if (!h) {
    if (windows.primary || windows.sub) {
      windows.primary = null;
      windows.sub = null;
      void invoke("close_hawkeye");
    }
    return;
  }
  if (!windows.primary) void openWindow("primary", v);
  if (h.secondary && !windows.sub) void openWindow("sub", v);
  if (!h.secondary && windows.sub) {
    // Leaving 双引擎模式 hides the secondary window (LizzieFrame.java:3359-3361).
    windows.sub = null;
    void invoke("close_hawkeye_sub");
  }
  for (const id of ["primary", "sub"] as const) {
    const open = windows[id];
    const pane = id === "primary" ? h.primary : h.secondary;
    if (!open || !pane) continue;
    if (open.document !== v.document.id) {
      open.document = v.document.id;
      open.layout = ++layouts;
    }
    const shown = title(id, v);
    if (shown !== open.title) {
      open.title = shown;
      void WebviewWindow.getByLabel(LABELS[id]).then(async (w) => {
        await w?.setAlwaysOnTop(v.settings.hawkeye.alwaysOnTop);
        await w?.setTitle(shown);
      });
    }
    const p = payload(id, open, v, pane);
    const text = JSON.stringify(p);
    if (text === open.lastSent) continue;
    open.lastSent = text;
    void emitTo(LABELS[id], HAWKEYE_DATA_EVENT, p);
  }
}

function input(id: Id, i: HawkeyeInput): void {
  const open = windows[id];
  if (!open) return;
  switch (i.type) {
    case "ready":
      open.lastSent = "";
      break;
    case "run":
      void run(i.command);
      break;
    case "display":
      setDisplay(i.change);
      break;
    case "key":
      if (i.code === "KeyY") toggleHawkeye();
      if (i.code === "KeyQ") toggleOnTop();
      break;
    case "layout":
      // Only the primary window's layout is written (Config.persist).
      if (id === "primary") void invoke("set_hawkeye_layout", { sort: i.sort, columns: i.columns });
      break;
    case "report":
      open.report = i.report;
      break;
  }
}

/** Y (Input.java:656), without Ctrl / Alt / Meta. */
export function handleHawkeyeKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyY" || e.ctrlKey || e.altKey || e.metaKey) return false;
  toggleHawkeye();
  return true;
}

/** Listens to the windows. Opening at start follows the core view (`show-badmoves-frame`). */
export async function startHawkeye(repaint: () => void): Promise<void> {
  await listen<HawkeyeInput>(HAWKEYE_INPUT_EVENT, (e) => {
    input("primary", e.payload);
    repaint();
  });
  await listen<HawkeyeInput>(HAWKEYE_SUB_INPUT_EVENT, (e) => {
    input("sub", e.payload);
    repaint();
  });
  await listen(CLOSED_EVENT, () => {
    windows.primary = null;
    windows.sub = null;
    if (state.view?.hawkeye) toggleHawkeye();
  });
}

/** What the acceptance probe reports: open windows, titles and each window's own report. */
export function hawkeyeReport() {
  return {
    primary: windows.primary && { title: windows.primary.title, report: windows.primary.report },
    sub: windows.sub && { title: windows.sub.title, report: windows.sub.report },
  };
}

Object.assign(COMMANDS, {
  // 视图 > 面板 > 超级鹰眼 (check) and 分析 > 超级鹰眼 share the Java action id (Menu.java:6681, 6872).
  "analysis.bad-moves": { run: toggleHawkeye, checked: () => !!state.view?.hawkeye },
} satisfies Record<string, CommandEntry>);
