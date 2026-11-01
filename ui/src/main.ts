// Workbench entry: Java LizzieFrame presentation state and incremental painters.
// Record and engine operations go through the Rust-owned command interface (app.ts).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { apply, copyRecord, exportRecord, navigate, pasteRecord, run, saveRecord, saveRecordAs, state, subscribe, unavailable } from "./app";
import { buildBottomBar, layoutBottomBar } from "./chrome/bottombar";
import { buildMenubar, menuOpen, secondaryMenuTitle } from "./chrome/menubar";
import { buildToolbar, renderToolbar } from "./chrome/toolbar";
import { onUnavailable, onView, snapshot } from "./contract/client";
import type { BoardOverlayView, HistoryPoint, Navigation, NodeAnalysisView, Point, WorkbenchView } from "./contract/generated";
import { dialogOpen, message } from "./dialogs";
import { format, isChinese, product, setLanguage, t } from "./i18n";
import { openJavaImport } from "./javaImport";
import { boardGeom, doubleEngineLayout, mainMarginRatio, SUB_MARGIN_RATIO, workbenchLayout, type BoardGeom, type DualPanels, type PanelId, type Panels, type Rect } from "./workbench/layout";
import { handleLayoutKey } from "./workbench/extraMode";
import { bindPanels, controlsShowing, handlePanelKey, layoutRequest, placeHandles } from "./workbench/panels";
import "./workbench/displayPage";
import { boardUi, candidatesShownFor, onBoardUiChange } from "./display/state";
import { bindReplayHost, finishReplayBranch, replayLength, replaying, type ReplayTarget } from "./display/replay";
import { autoplayReport, autoReplayLength, bindAutoplayHost, subAutoplayLength, subWheeled } from "./display/autoplay";
import { sameAnalysis, sameHistory } from "./workbench/viewEquality";
import { handleEngineKey } from "./engine/keys";
import { handleDisplayKey } from "./display/keys";
import { handleInfoKey } from "./analysis/infoCommands";
import { openAutoAnalysis } from "./quick/autoAnalysis";
import { bindScoreDialog, scoreDialogReport, scorePoint } from "./analysis/scoreDialog";
import { layerSignature, mainOwnership, secondaryOwnership, startTitleUpdates, statScene, subOwnership, workbenchTitle } from "./analysis/info";
import { mountSetupBanner } from "./setup/banner";
import "./editing/commands";
import "./analysis/graphCommands";
import { graphDestination, graphTarget, paintGraph, type GraphHits, type GraphReport } from "./analysis/graph";
import { bindTables, boardPointerMoved, boardWheel, clearTableSelection, mainTableSignature, renderMainTable, resetTables, tableBranchLength, tablesReport } from "./analysis/table";
import { frameReport, framesReport, handleTableKey, relayFrame, startFrame } from "./analysis/tableCommands";
import { handleHawkeyeKey, hawkeyeReport, relayHawkeye, startHawkeye } from "./hawkeye/commands";
import { batchReport, handleBatchKey, startBatchTable } from "./batch/commands";
import { bindCommentEditor, commentEditing } from "./editing/comment";
import { contextMenuItems, contextMenuOpen } from "./editing/contextMenu";
import { bindBoardInput, handleEditKey } from "./editing/input";
import { editing } from "./editing/state";
import { bindCaptureSurface, copyMainBoardPicture, copySubBoardPicture, saveMainBoardPicture, saveSubBoardPicture, saveWinratePicture } from "./recovery/capture";
import { startAutoSave } from "./recovery/commands";
import { recoveryReport } from "./recovery/panel";
import { bindProblemPanel, problemsReport, renderProblemPanel } from "./quick/problems";
import "./quick/analysisSettings";
import { flashWindowOpen, startFlash } from "./quick/flash";
import { openWholeGameAnalysis } from "./quick/wholeGame";
import {
  aiScoreText,
  loadImages,
  paintBackground,
  paintBoard,
  paintCaptured,
  paintStat,
  paintStatus,
  paintDualStrip,
  playouts,
  paintSubBoard,
  paintTree,
  prepare,
  ARROW,
  SCROLLBAR,
  type BoardReport,
  type BoardScene,
  type PreviewVariation,
  type StatReport,
  type SubBoardReport,
  type TreeViewport,
} from "./workbench/paint";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;
const PANEL_IDS: PanelId[] = ["captured", "stat", "graph", "comment", "status", "tree", "table", "sub"];

// Java's chrome fonts are platform-resolved (Dialog / Microsoft YaHei UI on Windows, DejaVu / Noto
// on Linux); styles.css selects the matching set from this attribute.
document.documentElement.dataset.os = navigator.userAgent.includes("Windows")
  ? "windows"
  : navigator.userAgent.includes("Mac OS")
    ? "mac"
    : "linux";

let panels: Panels | null = null;
let treeHits = new Map<string, number>();
let treeView: TreeViewport | null = null;
/** The tree the panel last drew (state.nodes is replaced with every tree update). */
let treeSource = state.nodes;
let treeRevision = 0;
let treeNumbersDrawn = 0;
/** User scroll of the variation tree, for the position it was made at (`treeScrollFor`). */
let treeScroll: { top: number; left: number } | null = null;
let treeScrollFor = "";
/** A status-line notice replacing the shortcut hint for a few seconds. */
let notice: { text: string; until: number } | null = null;
let version = "";
/** The hovered candidate rank of the main (left) board, and of 双引擎模式's right board. */
let hoveredCandidate: number | null = null;
let hoveredCandidate2: number | null = null;
/** LizzieFrame.mouseOverCoordinate, shared by both boards: a point "x,y" or NEXT_MOVE. */
let hoveredPoint: string | null = null;
let subCandidate = 0;
let subLength: number | null = null;
/** The pointer rests on the sub board (SubBoardRenderer.isMouseOver with no-refresh-on-sub). */
let subOver = false;
/** The sub board's frozen analysis while the pointer rests there, for its position only. */
let subFrozen: { node: number; analysis: NodeAnalysisView | null; revision: number } | null = null;
let subFrozenRevision = 0;
type FrozenVariation = { key: string; pv: (Point | null)[]; pv_captures: Point[][]; pv_visits: number[] };
/** The hovered variation as first shown, kept while 鼠标悬停显示变化图时,变化图不刷新 is on. */
let hoverFrozen: FrozenVariation | null = null;
let hoverFrozen2: FrozenVariation | null = null;
let presentationFor = "";
let boardReport: BoardReport | null = null;
let subReport: SubBoardReport | null = null;
let statReport: StatReport | null = null;
/** 双引擎模式: the boards' rectangles, what the right board drew and the status strips as drawn. */
let dual: DualPanels | null = null;
let rightReport: BoardReport | null = null;
let dualStatus: string[] = [];
/** Everything the boards draw from the view besides the position and analysis, per view. */
let displaySig = "";
let displaySig2 = "";
let displaySigFor: WorkbenchView | null = null;
/** What the win-rate graph last drew, and its hit-test sources (WinrateGraph's rendered points). */
let graphReport: GraphReport | null = null;
let graphHits: GraphHits | null = null;
/** WinrateGraph.mouseOverNode: the node under the pointer while 显示鼠标悬停信息 is on. */
let graphHover: number | null = null;
let historyByNode = new Map<number, HistoryPoint>();
/** Bumped when the shown analysis / history differ field-wise from the previous view's. */
let analysisRevision = 0;
let secondaryRevision = 0;
let historyRevision = 0;
let projectedAnalysis: NodeAnalysisView | null = null;
let projectedSecondary: NodeAnalysisView | null = null;
let projectedHistory: HistoryPoint[] = [];
let projectedView: WorkbenchView | null = null;

/** Java runtime summary keys must also be literal t() calls for bundle extraction. */
function rulesCaption(key: string | null): string | null {
  if (!key) return null;
  const captions: Record<string, string> = {
    "LizzieFrame.currentRules.chinese": t("LizzieFrame.currentRules.chinese"),
    "LizzieFrame.currentRules.chn-ancient": t("LizzieFrame.currentRules.chn-ancient"),
    "LizzieFrame.currentRules.japanese": t("LizzieFrame.currentRules.japanese"),
    "LizzieFrame.currentRules.tromp-taylor": t("LizzieFrame.currentRules.tromp-taylor"),
    "LizzieFrame.currentRules.goe": t("LizzieFrame.currentRules.goe"),
    "LizzieFrame.currentRules.others": t("LizzieFrame.currentRules.others"),
  };
  return captions[key] ?? t(key);
}

/** The rank of the candidate at the hovered point in `analysis`, -1 for the 下一手 详细 point
 *  (BoardRenderer.mouseOveredMove). */
function hoveredOrder(analysis: NodeAnalysisView | null | undefined): number | null {
  if (hoveredPoint === null) return null;
  if (hoveredPoint === NEXT_MOVE) return -1;
  return analysis?.candidates.find((c) => c.point && `${c.point.x},${c.point.y}` === hoveredPoint)?.order ?? null;
}

/** Sets the hovered point; each board previews its own engine's candidate there (LizzieFrame.java:
 *  10120-10160, selectNormalVariation on both renderers). */
function hover(point: string | null) {
  if (hoveredPoint === point) return;
  hoveredPoint = point;
  hoveredCandidate = hoveredOrder(state.view?.analysis);
  hoveredCandidate2 = hoveredOrder(state.view?.secondary?.analysis);
  // notChangedMouseOverMove: a variation kept by 变化图不刷新 lasts only while the pointer stays.
  if (point === null) {
    hoverFrozen = null;
    hoverFrozen2 = null;
  }
  schedule();
}

/** `hoveredPoint` of the 下一手 详细 point. */
const NEXT_MOVE = "next";

/** BoardData.tryToLimitMoves (BoardData.java:586-636) for the shown position: the previous kept
 *  candidates with their lastTimeUnlimited flag and time, and the points kept beyond 最大候选点数
 *  now. Only with a limit and without show-nosugg-circle; a new position starts afresh. */
const limitMemory = {
  key: "",
  previous: [] as { point: string; order: number; visits: number; unlimited: boolean; time: number }[],
  unlimited: new Set<string>(),
};
/** The limit settings `limitMemory` was last worked out with. */
let limitedWith = "";

function limitMoves(v: WorkbenchView) {
  const limit = v.settings.engine.analysis.limitMaxSuggestion;
  const key = `${v.document.id}:${v.position.node}`;
  if (limitMemory.key !== key) {
    limitMemory.key = key;
    limitMemory.previous = [];
  }
  const pointKey = (p: Point | null) => (p ? `${p.x},${p.y}` : "pass");
  const candidates = v.analysis?.candidates ?? [];
  const kept = candidates.map((c) => ({ point: pointKey(c.point), order: c.order, visits: c.visits, unlimited: false, time: 0 }));
  limitMemory.unlimited = new Set();
  if (limit > 0 && !v.settings.display.noSuggCircle && candidates.length > limit) {
    const now = Date.now();
    const last = limitMemory.previous;
    for (let n = limit; n < kept.length; n++) {
      const move = kept[n];
      let keep = false;
      if (move.order < limit + 1) {
        for (let s = 0; s < limit + 1 && s < last.length; s++) {
          const before = last[s];
          if (s >= limit && !before.unlimited) continue;
          if (move.point !== before.point) continue;
          move.unlimited = true;
          if (move.visits > before.visits || !before.unlimited) {
            move.time = now;
            keep = true;
          } else if (now - before.time < 3000) {
            move.time = before.time;
            keep = true;
          }
        }
      }
      if (keep) limitMemory.unlimited.add(move.point);
      else kept.splice(n--, 1);
    }
  }
  limitMemory.previous = kept;
}

/** Cache line navigation only when the tree/position changes, not on engine frames. */
function projectAnalysis(v: WorkbenchView) {
  if (projectedView !== v) {
    projectedView = v;
    const limitSettings = `${v.settings.engine.analysis.limitMaxSuggestion}:${v.settings.display.noSuggCircle}`;
    if (!sameAnalysis(projectedAnalysis, v.analysis)) {
      analysisRevision++;
      limitMoves(v);
    } else if (limitSettings !== limitedWith) limitMoves(v);
    limitedWith = limitSettings;
    projectedAnalysis = v.analysis;
    const secondary = v.secondary?.analysis ?? null;
    if (!sameAnalysis(projectedSecondary, secondary)) secondaryRevision++;
    projectedSecondary = secondary;
    const historyChanged = !sameHistory(projectedHistory, v.history);
    projectedHistory = v.history;
    if (historyChanged) {
      historyRevision++;
      historyByNode = new Map(v.history.map((p) => [p.node, p]));
    }
  }
  const lineKey = `${v.document.id}:${v.position.node}:${v.position.line_end}`;
  if (presentationFor !== lineKey) {
    presentationFor = lineKey;
    hoveredCandidate = null;
    hoveredCandidate2 = null;
    hoveredPoint = null;
    hoverFrozen = null;
    hoverFrozen2 = null;
    resetTables();
    // SubBoardRenderer.clearAfterMove; the frozen sub board belongs to the position it was taken at.
    subCandidate = 0;
    subLength = null;
    subFrozen = null;
  }
  // Java keeps the hovered coordinate, even when streaming visits reorder the candidates.
  hoveredCandidate = hoveredOrder(v.analysis);
  hoveredCandidate2 = hoveredOrder(v.secondary?.analysis);
  subCandidate = Math.min(subCandidate, Math.max(0, ((subFrozen?.analysis ?? v.analysis)?.candidates.length ?? 0) - 1));
  if (displaySigFor !== v) {
    displaySigFor = v;
    const fg = v.engine.foreground;
    const limits = [v.settings.engine.analysis.limitMaxSuggestion, v.settings.engine.analysis.limitBranchLength];
    displaySig = JSON.stringify([v.settings.display, v.board, v.rawNet, fg?.katago ?? null, !!fg, ...limits]);
    const se = v.engine.secondary;
    displaySig2 = JSON.stringify([v.settings.display, v.secondary?.board ?? null, v.secondary?.rawNet ?? null, v.rawNet !== null, se?.katago ?? null, !!se, ...limits]);
  }
}

/** 棋盘下方显示黑白名字 with a loaded record's player names (BoardRenderer.isEmptyName). */
function namesUnderBoard(v: WorkbenchView): boolean {
  return v.settings.display.namesOnBoard && v.document.format !== null && (v.document.info.black !== "" || v.document.info.white !== "");
}

/** Main-board geometry; painting and every hit test use this one margin (isBigMargin). */
function mainGeom(v: WorkbenchView, r: Rect): BoardGeom {
  const fg = v.engine.foreground;
  const big = v.settings.display.coordinates || namesUnderBoard(v) || v.rawNet !== null || (boardUi.policy && !!fg && !fg.katago);
  return boardGeom(r, v.document.size.width, v.document.size.height, mainMarginRatio(big));
}

/** LizzieFrame.isMouseOver: hovering previews candidates only while the side to move's candidates
 *  and 鼠标悬停显示变化图 are on; 纯网络 and 显示策略网络 (T) show no variation
 *  (BoardRenderer.java:1639-1644), so R has nothing to replay either. */
function hoverAllowed(v: WorkbenchView): boolean {
  return (v.whole_game.show_results || candidatesShownFor(v.position.to_play)) && (v.whole_game.show_results || v.settings.display.suggestionVariations) && v.rawNet === null && !boardUi.policy && !v.score;
}

/** The 下一手 详细 point while it shows (isShowingNextMoveBlunder): hovering it previews the actual
 *  move followed by the next position's best variation, before any candidate (1780-1799). Each
 *  board takes it from its own engine's overlay. */
function nextMoveTarget(v: WorkbenchView, overlay: BoardOverlayView | null = v.board): Point | null {
  return hoverAllowed(v) && v.settings.display.nextMoves && v.position.next_color ? overlay?.next_move?.point ?? null : null;
}

/** The hovered candidate's variation (BoardRenderer.prepareSelection, BoardRenderer.java:1640-1711)
 *  on the main board, or with `right` on 双引擎模式's right board from the secondary engine's
 *  results: frozen while 不刷新 is on, shortened by a running replay or auto-replay, and not shown
 *  for a candidate without enough playouts or beyond 最大候选点数 unless it is the next move or the
 *  variation auto-replays. */
function hoveredVariation(v: WorkbenchView, right = false): { target: ReplayTarget; preview: PreviewVariation | null } | null {
  const analysis = right ? v.secondary?.analysis ?? null : v.analysis;
  const overlay = right ? v.secondary?.board ?? null : v.board;
  const hovered = right ? hoveredCandidate2 : hoveredCandidate;
  const candidates = analysis?.candidates ?? [];
  const nextPoint = hovered === -1 ? nextMoveTarget(v, overlay) : null;
  const c = hoverAllowed(v) && !nextPoint ? candidates.find((c) => c.order === hovered) : undefined;
  const point = nextPoint ?? c?.point;
  const info = overlay?.next_move;
  if (!point || (nextPoint && !info)) {
    if (right) hoverFrozen2 = null;
    else hoverFrozen = null;
    return null;
  }
  const source = nextPoint ? { pv: info!.pv, pv_captures: info!.pv_captures, pv_visits: info!.pv_visits } : { pv: c!.pv, pv_captures: c!.pv_captures, pv_visits: c!.pv_visits };
  const key = `${v.document.id}:${v.position.node}:${right ? "right:" : ""}${nextPoint ? "next" : ""}${point.x},${point.y}`;
  const before = right ? hoverFrozen2 : hoverFrozen;
  const frozen = !v.settings.display.noRefreshOnMouse || before?.key !== key ? { key, ...source } : before;
  if (right) hoverFrozen2 = frozen;
  else hoverFrozen = frozen;
  const { pv, pv_captures, pv_visits } = frozen;
  const limit = v.settings.engine.analysis.limitBranchLength;
  const steps = limit > 0 ? Math.min(limit, pv.length) : pv.length;
  const target = { key, steps };
  const replayed = replayLength(key) ?? tableBranchLength(key) ?? autoReplayLength(key, steps);
  const order = nextPoint ? -1 : c!.order;
  if (c && replayed === null && !v.settings.display.autoReplayBranch) {
    const next = v.position.next_moves[0];
    const needShow = !!next && next.x === point.x && next.y === point.y;
    const ratio = c.allocation_visits / Math.max(1, ...candidates.map((x) => x.allocation_visits));
    const maxSuggestion = v.settings.engine.analysis.limitMaxSuggestion;
    // lastTimeUnlimited is kept for the main board's candidates only.
    const kept = !right && limitMemory.unlimited.has(`${point.x},${point.y}`);
    if (!needShow && (ratio <= v.settings.display.minPlayoutRatio || (maxSuggestion > 0 && c.order + 1 > maxSuggestion && !kept)))
      return { target, preview: null };
  }
  return { target, preview: { order, pv, pv_captures, pv_visits, length: replayed ?? steps } };
}

function subAnalysis(v: WorkbenchView): NodeAnalysisView | null {
  return subFrozen && subFrozen.node === v.position.node ? subFrozen.analysis : v.analysis;
}

function flash(text: string) {
  notice = { text, until: performance.now() + 4000 };
  schedule();
  setTimeout(schedule, 4050);
}

// ------------------------------------------------------------------ keyboard (Input.java:325-840)

function textTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true;
  return target instanceof HTMLInputElement && target.type !== "checkbox";
}

/** Browsing keys of Input.keyPressed. Returns null for keys Java does not bind. */
function navigationFor(e: KeyboardEvent): Navigation | null {
  const ctrl = e.ctrlKey || e.metaKey;
  switch (e.key) {
    case "ArrowUp":
      if (ctrl && e.shiftKey) return { op: "PreviousComment" };
      if (e.shiftKey) return { op: "BranchStart" };
      return ctrl ? { op: "Back", count: 10 } : { op: "Back", count: 1 };
    case "ArrowDown":
      if (ctrl && e.shiftKey) return { op: "NextComment" };
      return ctrl ? { op: "Forward", count: 10 } : { op: "Forward", count: 1 };
    case "ArrowLeft":
      return ctrl && !e.shiftKey ? { op: "BranchPoint" } : { op: "PreviousBranch" };
    case "ArrowRight":
      return { op: "NextBranch" };
    case "PageUp":
      return { op: "Back", count: 10 };
    case "PageDown":
      return { op: "Forward", count: 10 };
    case "Home":
      return { op: "First" };
    case "End":
      return { op: "Last" };
    default:
      return null;
  }
}

/** Java key codes bound in Input.keyPressed besides navigation (letters, digits, punctuation). */
const JAVA_BOUND = new Set([..."ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789", " ", ",", ".", "[", "]", "Enter", "Delete", "Backspace"]);

document.addEventListener("keydown", (e) => {
  if (dialogOpen() || flashWindowOpen() || menuOpen() || contextMenuOpen() || textTarget(e.target)) return;
  // The 存档与读档 controls keep their keys, like the Swing components holding the focus there.
  if (e.target instanceof Element && e.target.closest("#recovery")) return;
  if (handleLayoutKey(e) || handlePanelKey(e) || handleEngineKey(e) || handleEditKey(e) || handleDisplayKey(e) || handleInfoKey(e) || handleTableKey(e) || handleHawkeyeKey(e) || handleBatchKey(e)) {
    e.preventDefault();
    return;
  }
  const ctrl = e.ctrlKey || e.metaKey;
  const nav = navigationFor(e);
  if (nav) {
    e.preventDefault();
    navigate(nav);
    return;
  }
  // macOS turns Option+letter into another character; Java's VK code is the letter key itself.
  const key = e.altKey && /^Key[A-Z]$/.test(e.code) ? e.code.slice(3) : e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (key === "O" && !ctrl && !e.altKey && !e.shiftKey) {
    e.preventDefault();
    run({ type: "ChooseAndOpenRecord" });
  } else if (key === "A" && !ctrl && !e.altKey && !e.shiftKey) {
    e.preventDefault();
    void openAutoAnalysis();
  } else if (key === "V" && ctrl && !e.altKey) {
    e.preventDefault();
    pasteRecord();
  } else if (key === "S") {
    // Input.java VK_S, in Java's order of precedence.
    e.preventDefault();
    if (ctrl && e.shiftKey) void exportRecord("Raw");
    else if (ctrl && e.altKey) void exportRecord("Branch");
    else if (e.shiftKey && e.altKey) void saveWinratePicture();
    else if (e.shiftKey) void saveSubBoardPicture();
    else if (e.altKey) void saveMainBoardPicture();
    else if (ctrl) saveRecord();
    else saveRecordAs();
  } else if (key === "C" && (e.altKey || e.shiftKey || ctrl)) {
    // Input.java VK_C; plain C (coordinates) is a display key (display/keys.ts).
    e.preventDefault();
    if (e.altKey) void copySubBoardPicture();
    else if (e.shiftKey) void copyMainBoardPicture();
    else copyRecord();
  } else if (key === "B" && !ctrl && !e.altKey) {
    e.preventDefault();
    navigate({ op: "MainTrunk" });
  } else if (key === "B" && ctrl && e.shiftKey && !e.altKey) {
    e.preventDefault();
    void openWholeGameAnalysis();
  } else if (key === "B" && ctrl && !e.shiftKey && !e.altKey) {
    // Input.java VK_B: Ctrl+B 整盘速览（闪电）; Ctrl+Shift+B is 整盘深入分析.
    e.preventDefault();
    void startFlash({ kind: "WholeGame" });
  } else if (!e.metaKey && key !== " " && JAVA_BOUND.has(key)) {
    e.preventDefault();
    flash(product("unavailable"));
  }
});

// ------------------------------------------------------------------ pointer

function bindPointer() {
  const main = $("main");
  // Input.mousePressed (Input.java:68-72): while scoring, a press on the drawn workbench (board,
  // sub board, graph, variation tree, panels) does nothing but a left click marks the board point.
  for (const type of ["mousedown", "mouseup", "click", "dblclick", "contextmenu"]) {
    main.addEventListener(type, (ev) => {
      const e = ev as MouseEvent;
      if (!state.view?.score || !(e.target as HTMLElement).closest("#board, #board2, #sub, #graph, #tree, #captured, #stat")) return;
      e.preventDefault();
      e.stopPropagation();
      if (type !== "mousedown" || e.button !== 0 || !(e.target as HTMLElement).closest("#board, #board2")) return;
      const point = boardPointAt(e.clientX, e.clientY);
      if (point) scorePoint(point);
    }, true);
  }
  // Input.mouseWheelMoved: one move per notch over the workbench. Scroll panes take their own
  // wheel: the comment, the candidate list and the variation tree (a JScrollPane, scrolled below).
  main.addEventListener(
    "wheel",
    (ev) => {
      if ((ev.target as HTMLElement).closest("#comment, #table, #tree, #sub, #recovery")) return;
      ev.preventDefault();
      if (ev.deltaY !== 0 && !boardWheel(ev.deltaY > 0)) navigate(ev.deltaY > 0 ? { op: "Forward", count: 1 } : { op: "Back", count: 1 });
    },
    { passive: false },
  );
  // 双引擎模式: a press on the right board is a press on the same point of the main board
  // (BoardRenderer.convertScreenToCoordinates, BoardRenderer.java:4451).
  bindBoardInput([$("board"), $("board2")], {
    pointAt: boardPointAt,
    stoneAt: (p) => {
      const v = state.view;
      if (!panels || !v) return null;
      const geo = mainGeom(v, panels.board);
      const m = $("main").getBoundingClientRect();
      return { x: m.left + geo.x + geo.margin + p.x * geo.square, y: m.top + geo.y + geo.margin + p.y * geo.square, r: geo.stoneRadius };
    },
    previewMoves: () => {
      const v = state.view;
      const preview = v ? hoveredVariation(v)?.preview : null;
      if (!preview) return null;
      // The moves the board shows; passes are not placed.
      const moves = preview.pv.slice(0, preview.length).filter((p): p is Point => p !== null);
      return moves.length ? moves : null;
    },
    clearTableSelection,
  });
  // LizzieFrame.onMouseMoved (LizzieFrame.java:10120-10160): the point under the pointer on either
  // board is the hovered point of both.
  const pointerMoved = (ev: MouseEvent, right: boolean) => {
    const v = state.view;
    if (!v || !panels) return;
    if (!boardPointerMoved(boardPointAt(ev.clientX, ev.clientY) !== null)) return;
    if (!hoverAllowed(v)) return hover(null);
    const geo = mainGeom(v, right && dual ? dual.right : panels.board);
    const rect = $("main").getBoundingClientRect();
    const x = ev.clientX - rect.left - geo.x - geo.margin;
    const y = ev.clientY - rect.top - geo.y - geo.margin;
    const near = (p: Point) => Math.hypot(x - p.x * geo.square, y - p.y * geo.square) <= geo.stoneRadius + 1;
    const doubleEngine = dual !== null;
    // The 下一手 详细 point comes first (BoardRenderer.mouseOveredMove).
    const next = nextMoveTarget(v) ?? (doubleEngine ? nextMoveTarget(v, v.secondary?.board ?? null) : null);
    if (next && near(next)) return hover(NEXT_MOVE);
    // Beyond 最大候选点数 the candidates exist only with show-nosugg-circle (BoardData.tryToLimitMoves).
    const limit = v.settings.engine.analysis.limitMaxSuggestion;
    const candidate = v.analysis?.candidates.find((c) => c.point && c.visits > 0 && near(c.point) &&
      (limit === 0 || c.order < limit || v.settings.display.noSuggCircle || limitMemory.unlimited.has(`${c.point.x},${c.point.y}`)))
      ?? (doubleEngine ? v.secondary?.analysis?.candidates.find((c) => c.point && c.visits > 0 && near(c.point) &&
        (limit === 0 || c.order < limit || v.settings.display.noSuggCircle)) : undefined);
    hover(candidate?.point ? `${candidate.point.x},${candidate.point.y}` : null);
  };
  $("board").addEventListener("mousemove", (ev) => pointerMoved(ev, false));
  $("board2").addEventListener("mousemove", (ev) => pointerMoved(ev, true));
  // The open board popup keeps the preview it was opened over.
  for (const id of ["board", "board2"]) {
    $(id).addEventListener("mouseleave", () => {
      finishReplayBranch();
      if (!contextMenuOpen()) hover(null);
    });
  }
  // The replay ends when the window is no longer the active one (LizzieFrame.java:11650).
  window.addEventListener("blur", finishReplayBranch);
  const sub = $("sub");
  sub.addEventListener("contextmenu", (ev) => ev.preventDefault());
  // LizzieFrame.java:10522-10532: left click the next candidate, right click the previous one; the
  // index stays at the last candidate (getBestMove) and the variation starts whole again.
  sub.addEventListener("mousedown", (ev) => {
    const v = state.view;
    const count = v ? subAnalysis(v)?.candidates.length ?? 0 : 0;
    if (!count || (ev.button !== 0 && ev.button !== 2)) return;
    ev.preventDefault();
    subCandidate = ev.button === 0 ? Math.min(count - 1, subCandidate + 1) : Math.max(0, subCandidate - 1);
    subLength = null;
    schedule();
  });
  // LizzieFrame.java:10688-10703: forward from the whole variation starts at its first move.
  sub.addEventListener("wheel", (ev) => {
    ev.preventDefault();
    const v = state.view, c = v ? subAnalysis(v)?.candidates[subCandidate] : undefined;
    if (!v || !c || ev.deltaY === 0 || v.settings.display.subBoard !== "Variation") return;
    const limit = v.settings.engine.analysis.limitBranchLength;
    const max = limit > 0 ? Math.min(limit, c.pv.length) : c.pv.length;
    // A running sub-board autoplay hands its length to the wheel (SubBoardRenderer.wheeled).
    const current = subLength ?? subAutoplayLength();
    if (current === null) {
      if (ev.deltaY > 0 && max > 0) subLength = 1;
    } else if (ev.deltaY > 0) subLength = Math.min(max, current + 1);
    else subLength = Math.max(1, current - 1);
    subWheeled();
    schedule();
  }, { passive: false });
  // processSubOnMouseMoved / onMouseExited (LizzieFrame.java:9872-9930, 10042-10068): with
  // no-refresh-on-sub the resting pointer freezes the sub board; leaving resets it.
  sub.addEventListener("mousemove", () => {
    if (subOver) return;
    subOver = true;
    schedule();
  });
  sub.addEventListener("mouseleave", () => {
    const frozen = subOver && !!state.view?.settings.display.noRefreshOnSub;
    subOver = false;
    subFrozen = null;
    if (frozen) {
      subCandidate = 0;
      subLength = null;
    }
    schedule();
  });
  const graph = $("graph");
  /** The graph target a click (or the latest drag event) asks for; Swing's invokeLater coalescing. */
  let queuedGraph: { node: number; document: number; withinBranch: boolean } | null = null;
  let navigatingGraph = false;
  const graphNodeAt = (e: MouseEvent): number | null => {
    const r = graph.getBoundingClientRect();
    return graphHits ? graphTarget(graphHits, Math.floor(e.clientX - r.left), Math.floor(e.clientY - r.top)) : null;
  };
  /** LizzieFrame.onClicked / onMouseDragged: the latest target wins and a drag miss clears it;
   *  each one is reached from the position current when it runs (goToWinrateGraphTarget). */
  const goToGraphNode = async (target: number | null, withinBranch: boolean) => {
    const v = state.view;
    queuedGraph = target === null || !v ? null : { node: target, document: v.document.id, withinBranch };
    if (!queuedGraph || navigatingGraph) return;
    navigatingGraph = true;
    try {
      while (queuedGraph) {
        const q = queuedGraph;
        queuedGraph = null;
        const now = state.view;
        if (!now || now.document.id !== q.document) continue;
        const node = graphDestination(state.nodes, now.position.node, q.node, q.withinBranch);
        if (node !== null) await run({ type: "GotoNode", node });
      }
    } finally { navigatingGraph = false; }
  };
  const setGraphHover = (node: number | null) => {
    if (graphHover === node) return;
    graphHover = node;
    schedule();
  };
  graph.addEventListener("contextmenu", (ev) => ev.preventDefault());
  graph.addEventListener("mousedown", (ev) => {
    if (!state.view) return;
    if (ev.button === 2) {
      // Off the board: openRightClickMenu clears the table selection, else undoForRightClick.
      ev.preventDefault();
      if (!clearTableSelection()) navigate({ op: "Back", count: 1 });
      return;
    }
    if (ev.button !== 0) return;
    ev.preventDefault();
    const target = graphNodeAt(ev);
    if (target !== null) void goToGraphNode(target, false);
    const drag = (e: MouseEvent) => void goToGraphNode(graphNodeAt(e), true);
    const end = () => { document.removeEventListener("mousemove", drag); document.removeEventListener("mouseup", end); };
    document.addEventListener("mousemove", drag);
    document.addEventListener("mouseup", end);
  });
  // processMouseMoveOnWinrateGraph: hovering redraws the graph only; a drag keeps the hovered node.
  graph.addEventListener("mousemove", (ev) => {
    if (ev.buttons === 0 && state.view?.settings.display.mouseOverWinrateGraph) setGraphHover(graphNodeAt(ev));
  });
  graph.addEventListener("mouseleave", () => setGraphHover(null));
  const tree = $("tree");
  // BasicScrollPaneUI: the vertical bar if shown, else the horizontal one; unit increment 16 x 3
  // rows per notch (LizzieFrame.java:806-807).
  tree.addEventListener(
    "wheel",
    (ev) => {
      ev.preventDefault();
      const step = Math.sign(ev.deltaY || ev.deltaX) * TREE_UNIT * 3;
      if (treeView?.vertical && !ev.shiftKey) scrollTree(step, 0);
      else if (treeView?.horizontal) scrollTree(0, step);
    },
    { passive: false },
  );
  tree.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0 || !treeView || !panels) return;
    const r = tree.getBoundingClientRect();
    const x = ev.clientX - r.left;
    const y = ev.clientY - r.top;
    const v = treeView;
    if (v.vertical && x >= panels.tree.w - SCROLLBAR && y < v.viewH) {
      pressScrollbar(ev, "vertical", y);
      return;
    }
    if (v.horizontal && y >= panels.tree.h - SCROLLBAR && x < v.viewW) {
      pressScrollbar(ev, "horizontal", x);
      return;
    }
    for (const [k, id] of treeHits) {
      const [hx, hy] = k.split(",").map(Number);
      if (Math.abs(hx - x) <= 8 && Math.abs(hy - y) <= 8) {
        run({ type: "GotoNode", node: id });
        return;
      }
    }
  });
}

/** JScrollBar unit increment of the variation tree. */
const TREE_UNIT = 16;

/** Scrolls the variation tree viewport; the scroll holds until the browsing position changes,
 *  when renderVarTree centres the current node again. */
function scrollTree(dTop: number, dLeft: number) {
  if (!treeView) return;
  const from = treeScroll ?? { top: treeView.top, left: treeView.left };
  treeScroll = {
    top: Math.max(0, Math.min(from.top + dTop, treeView.maxY - treeView.viewH)),
    left: Math.max(0, Math.min(from.left + dLeft, treeView.maxX - treeView.viewW)),
  };
  schedule();
}

/** Arrow buttons scroll one unit, the track one visible extent toward the press, and the thumb
 *  drags (BasicScrollBarUI). */
function pressScrollbar(ev: MouseEvent, axis: "vertical" | "horizontal", at: number) {
  const v = treeView!;
  const vertical = axis === "vertical";
  const bar = (vertical ? v.vertical : v.horizontal)!;
  const length = vertical ? v.viewH : v.viewW;
  const content = vertical ? v.maxY : v.maxX;
  const move = (d: number) => (vertical ? scrollTree(d, 0) : scrollTree(0, d));
  ev.preventDefault();
  if (at < ARROW) return move(-TREE_UNIT);
  if (at >= length - ARROW) return move(TREE_UNIT);
  const along = at - ARROW;
  if (along < bar.thumbStart) return move(-length);
  if (along >= bar.thumbStart + bar.thumb) return move(length);
  const startPointer = vertical ? ev.clientY : ev.clientX;
  const startOffset = vertical ? v.top : v.left;
  const ratio = (content - length) / Math.max(1, bar.track - bar.thumb);
  const drag = (e: MouseEvent) => {
    const target = startOffset + ((vertical ? e.clientY : e.clientX) - startPointer) * ratio;
    const current = treeScroll ?? { top: v.top, left: v.left };
    if (vertical) scrollTree(target - current.top, 0);
    else scrollTree(0, target - current.left);
  };
  window.addEventListener("mousemove", drag);
  window.addEventListener("mouseup", () => window.removeEventListener("mousemove", drag), { once: true });
}

/** The intersection under a client point: on the main board, or in 双引擎模式 on whichever board
 *  is there (both show the same position). */
function boardPointAt(clientX: number, clientY: number): Point | null {
  const v = state.view;
  if (!panels || !v) return null;
  const { width, height } = v.document.size;
  const rect = $("main").getBoundingClientRect();
  const geo = mainGeom(v, dual && clientX - rect.left >= dual.right.x ? dual.right : panels.board);
  const x = clientX - rect.left - geo.x - geo.margin;
  const y = clientY - rect.top - geo.y - geo.margin;
  const cx = Math.floor((x + geo.square / 2) / geo.square);
  const cy = Math.floor((y + geo.square / 2) / geo.square);
  return cx >= 0 && cy >= 0 && cx < width && cy < height ? { x: cx, y: cy } : null;
}

// ------------------------------------------------------------------ rendering

let pending = false;
function schedule() {
  if (pending) return;
  pending = true;
  requestAnimationFrame(() => {
    pending = false;
    render();
  });
}

function place(el: HTMLElement, r: Rect) {
  el.style.left = `${r.x}px`;
  el.style.top = `${r.y}px`;
  el.style.width = `${r.w}px`;
  el.style.height = `${r.h}px`;
}

/** Per-panel input signatures: a panel repaints only when what it shows changed. */
const lastSig: Record<string, string> = {};
function changed(panel: string, sig: string): boolean {
  if (lastSig[panel] === sig) return false;
  lastSig[panel] = sig;
  return true;
}

/** The wallpaper with its frosted containers is a static background of #main, one per size. */
let backgroundUrl: string | null = null;
async function installBackground(size: string, w: number, h: number, glass: Rect[]) {
  const blob = await paintBackground(w, h, glass);
  if (!blob || lastSig.bg !== size) return;
  if (backgroundUrl) URL.revokeObjectURL(backgroundUrl);
  backgroundUrl = URL.createObjectURL(blob);
  const main = $("main");
  main.style.backgroundImage = `url(${backgroundUrl})`;
  main.style.backgroundSize = `${w}px ${h}px`;
}

/** LizzieFrame.drawPonderingState: only ambient status depends on active pondering. An engine left
 *  unrestored after a borrow shows its cause and the way back instead (loadingStatusLines,
 *  java:7871-7890); a retrying quick analysis replaces the shortcut hint (drawCommandString). */
function statusLines(): string[] {
  const v = state.view;
  const hint = notice && notice.until > performance.now()
    ? notice.text
    : v?.quick.retrying ? t("LizzieFrame.quickAnalysis.retrying") : t("LizzieFrame.prompt.showControlsHint");
  const unrestored = v?.engine.unrestored;
  if (unrestored) {
    const reason = unrestored === "Stop" ? t("LizzieFrame.foregroundUnrestored.reason.stop") : t("LizzieFrame.foregroundUnrestored.reason.restore");
    return [
      format(t("LizzieFrame.display.foregroundUnrestored"), reason),
      format(t("LizzieFrame.display.foregroundUnrestoredAction"), t("Menu.restartCurrentEngine")),
      hint,
    ];
  }
  return [
    v?.engine.foreground?.name ?? t("LizzieFrame.noEngineText"),
    `${t("LizzieFrame.display.pondering")} ${v?.engine.analysing ? t("LizzieFrame.display.on") : t("LizzieFrame.display.off")}${t("LizzieFrame.display.space")}`,
    hint,
  ];
}

function render() {
  const v = state.view;
  if (!v) return;
  const main = $("main");
  const W = main.clientWidth;
  const H = main.clientHeight;
  // A hidden window has no allocation yet; the window minimum is 520x400 (LizzieFrame.java:686).
  if (W < 100 || H < 100) return;
  const pos = v.position;
  const { width: cols, height: rows } = v.document.size;
  // 双引擎模式 shows the two boards and their status strips only (LizzieFrame.java:6640-6720).
  dual = v.settings.extra_mode === "DoubleEngine" ? doubleEngineLayout(W, H, cols, rows) : null;
  const normal = workbenchLayout(W, H, layoutRequest(v));
  panels = dual ? { ...normal, board: dual.left, glass: [], handles: null } : normal;
  const P = panels;
  const size = `${W}x${H}:${window.devicePixelRatio}`;
  // Edits change a node's position, marks or side to play in place: the revision moves on.
  const docSig = `${v.document.id}:${v.document.revision}:${pos.node}:${pos.to_play}`;
  projectAnalysis(v);
  const background = `${size}:${dual !== null}:${JSON.stringify(P.glass)}`;
  if (changed("bg", background)) installBackground(background, W, H, P.glass);
  // 界面 > 面板 flags; holding X hides the tree, the list and the comment (Input.java:616-623).
  const controls = controlsShowing();
  for (const id of PANEL_IDS) {
    const shown = !dual && P.shown[id] && !(controls && (id === "tree" || id === "table" || id === "comment"));
    const display = shown ? "" : "none";
    if ($(id).style.display !== display) $(id).style.display = display;
  }
  if (changed("mode", String(dual !== null))) {
    for (const id of ["board2", "dualStatus"]) $(id).style.display = dual ? "" : "none";
  }
  placeHandles(P.handles);

  const firstChildId = state.nodes.get(pos.node)?.children[0];
  const firstChild = firstChildId === undefined ? undefined : state.nodes.get(firstChildId);
  const scene: BoardScene = {
    cols,
    rows,
    stones: pos.stones,
    lastAction: pos.last_action,
    toPlay: pos.to_play,
    marks: pos.marks,
    nextMoves: pos.next_moves,
    nextMainMove: firstChild && !firstChild.pass && (firstChild.kind === "Black" || firstChild.kind === "White")
      ? pos.next_moves[0] ?? null : null,
    nextColor: pos.next_color,
    black: v.document.info.black,
    white: v.document.info.white,
    names: namesUnderBoard(v),
    snapshot: pos.snapshot,
    mainTrunk: pos.main_trunk,
    analysis: v.analysis,
    overlay: v.board,
    display: v.whole_game.show_results ? { ...v.settings.display, suggestionWinrate: true, suggestionPlayouts: true, suggestionVariations: true } : v.settings.display,
    suggestionLimit: v.settings.engine.analysis.limitMaxSuggestion,
    unlimited: limitMemory.unlimited,
    candidatesShown: v.whole_game.show_results || candidatesShownFor(pos.to_play),
    policy: boardUi.policy,
    rawNetOn: v.rawNet !== null,
    // A result for another position is never shown.
    rawNet: v.rawNet?.node === pos.node ? v.rawNet : null,
    katago: !!v.engine.foreground?.katago,
    engine: !!v.engine.foreground,
    hovered: hoverAllowed(v) ? hoveredCandidate : null,
    preview: hoveredVariation(v)?.preview ?? null,
    ownership: mainOwnership(v, hoverAllowed(v) ? hoveredCandidate : null),
    score: v.score,
    mark: v.problems.mark,
  };
  const ui = `${boardUi.showBlack}:${boardUi.showWhite}:${boardUi.policy}`;
  const previewSig = scene.preview ? `${scene.preview.order}:${scene.preview.length}:${scene.preview.pv.length}` : "";
  const scoreSig = v.score ? JSON.stringify(v.score) : "";
  if (changed("board", `${background}:${P.board.x},${P.board.y},${P.board.w}:${docSig}:${analysisRevision}:${displaySig}:${ui}:${scene.hovered}:${previewSig}:${scene.names}:${layerSignature("main", scene.ownership)}:${scoreSig}:${scene.mark?.x},${scene.mark?.y}`))
    boardReport = paintBoard(prepare($("board"), P.board), mainGeom(v, P.board), P.board, scene);
  if (dual) renderDual(v, dual, W, H, size, docSig, ui, scoreSig, scene);
  else renderPanels(v, P, H, size, docSig, ui, scene);
  relayFrame(v);
  relayHawkeye(v);
  if (changed("bottombar", `${$("bottombar").clientWidth}`)) {
    layoutBottomBar($("bottombar"));
    // 详细工具栏 wraps its panels by width: a new height resizes the workbench.
    if (main.clientHeight !== H) schedule();
  }
  renderToolbar();
  const title = workbenchTitle(v, version, historyByNode, state.nodes.get(pos.node)?.parent ?? undefined);
  if (changed("title", title)) {
    document.title = title;
    getCurrentWindow().setTitle(title);
  }
}

/** The Normal-mode panels beside the main board: sub board, captured and statistics panels, the
 *  win-rate graph, the status lines, the variation tree, the comment and the 选点列表. */
function renderPanels(v: WorkbenchView, P: Panels, H: number, size: string, docSig: string, ui: string, scene: BoardScene) {
  const pos = v.position;
  const { width: cols, height: rows } = v.document.size;
  // The resting pointer freezes the sub board's variation for this position (no-refresh-on-sub).
  const over = subOver && v.settings.display.noRefreshOnSub;
  if (!over) subFrozen = null;
  else if (!subFrozen || subFrozen.node !== pos.node) subFrozen = { node: pos.node, analysis: v.analysis, revision: ++subFrozenRevision };
  // The sub board's ownership follows its shown variation's candidate (SubBoardRenderer.getEstimateArray).
  const subVariation = v.settings.display.subBoard === "Variation" && scene.candidatesShown && scene.engine ? subAnalysis(v)?.candidates : undefined;
  const subLayer = subOwnership(v, subVariation?.[Math.min(subCandidate, subVariation.length - 1)]);
  const subShown = subLength ?? subAutoplayLength();
  // Panel rectangles follow the panel flags and separators, not just the size; hidden panels are
  // not drawn (their signatures keep what they last showed).
  const at = (r: Rect) => `${size}:${r.x},${r.y},${r.w},${r.h}`;
  if (P.shown.sub && changed("sub", `${at(P.sub)}:${docSig}:${analysisRevision}:${displaySig}:${ui}:${subCandidate}:${subShown}:${over}:${subFrozen?.revision}:${layerSignature("sub", subLayer)}`))
    subReport = paintSubBoard(prepare($("sub"), P.sub), boardGeom(P.sub, cols, rows, SUB_MARGIN_RATIO), P.sub, {
      cols,
      rows,
      stones: pos.stones,
      toPlay: pos.to_play,
      mode: v.settings.display.subBoard,
      analysis: subAnalysis(v),
      heat: v.analysis,
      candidate: subCandidate,
      length: subShown,
      over,
      candidatesShown: scene.candidatesShown,
      engine: scene.engine,
      branchLimit: v.settings.engine.analysis.limitBranchLength,
      removeDead: v.settings.display.removeDeadInVariation,
      ownership: subLayer,
    });
  const ai = v.statistics;
  if (P.shown.captured && changed("captured", `${at(P.captured)}:${docSig}:${v.komi}:${v.engine.analysing}:${v.engine.rules_key}:${ai.ai_score_black}:${ai.ai_score_white}`))
    paintCaptured(prepare($("captured"), P.captured), P.captured, {
      toPlay: pos.to_play,
      blackCaptures: pos.black_captures,
      whiteCaptures: pos.white_captures,
      moveNumber: pos.move_number,
      komi: v.komi,
      analysing: v.engine.analysing,
      rules: rulesCaption(v.engine.rules_key),
      aiScoreBlack: ai.ai_score_black,
      aiScoreWhite: ai.ai_score_white,
    });
  const stat = statScene(v, historyByNode, (node) => state.nodes.get(node)?.parent ?? undefined);
  if (P.shown.stat && changed("stat", `${at(P.stat)}:${JSON.stringify(stat)}`)) statReport = paintStat(prepare($("stat"), P.stat), P.stat, stat);
  const graphDisplay = v.settings.display;
  // Turning 显示鼠标悬停信息 off clears the hovered node (clearMouseOverWinrateGraph).
  if (!graphDisplay.mouseOverWinrateGraph) graphHover = null;
  const graphSettings = `${graphDisplay.winrateLine}:${graphDisplay.scoreLeadLine}:${graphDisplay.winrateGraphFill}:${graphDisplay.blunderBar}:${graphDisplay.initialMaxScoreLead}`;
  if (P.shown.graph && changed("graph", `${at(P.graph)}:${docSig}:${pos.line_end}:${historyRevision}:${graphSettings}:${graphHover}`)) {
    const painted = paintGraph(prepare($("graph"), P.graph), P.graph, { nodes: state.nodes, current: pos.node, history: historyByNode, display: graphDisplay, hover: graphHover });
    graphReport = painted.report;
    graphHits = painted.hits;
  }
  const statusRect = { x: 0, y: P.ponderingY, w: P.board.x, h: H - P.ponderingY };
  const lines = statusLines();
  if (P.shown.status && changed("status", `${at(statusRect)}:${lines.join("|")}`)) paintStatus(prepare($("status"), statusRect), statusRect, lines);
  if (treeScrollFor !== docSig) {
    treeScroll = null;
    treeScrollFor = docSig;
  }
  if (treeSource !== state.nodes) {
    treeSource = state.nodes;
    treeRevision++;
  }
  const treeNumbers = v.settings.display.treeMoveNumbers;
  if (P.shown.tree && changed("tree", `${at(P.tree)}:${docSig}:${treeRevision}:${treeNumbers}:${treeScroll?.top}:${treeScroll?.left}`)) {
    const painted = paintTree(prepare($("tree"), P.tree), P.tree, state.nodes, state.root, pos.node, state.line, treeScroll, treeNumbers);
    treeHits = painted.hits;
    treeView = painted.viewport;
    treeNumbersDrawn = painted.numbers;
  }
  place($("comment"), P.comment);
  // 记录选点信息到评论中: Java's pane shows the summary its engine writes into the comment.
  const comment = v.comment_with_analysis ?? pos.comment;
  if (changed("comment", `${docSig}:${comment}`)) $("commentText").textContent = comment;
  renderProblemPanel(v);
  place($("table"), P.table);
  if (P.shown.table && changed("table", `${at(P.table)}:${docSig}:${analysisRevision}:${historyRevision}:${mainTableSignature(v)}`)) renderMainTable($("table"), v);
}

/** 双引擎模式's right board (boardRenderer2): the main board's scene from the secondary engine's
 *  results; with no secondary engine or result only the position. */
function renderDual(v: WorkbenchView, d: DualPanels, W: number, H: number, size: string, docSig: string, ui: string, scoreSig: string, left: BoardScene) {
  const s = v.secondary;
  const allowed = hoverAllowed(v);
  const scene: BoardScene = {
    ...left,
    display: v.settings.display,
    candidatesShown: candidatesShownFor(v.position.to_play),
    analysis: s?.analysis ?? null,
    // Move numbers belong to the position; without a secondary view nothing else is drawn.
    overlay: s?.board ?? { move_numbers: v.board.move_numbers, ranks: [], rank_corner: false, ranks_after_numbers: false, next_move: null, info_order_sample: v.board.info_order_sample },
    unlimited: NO_POINTS,
    // A result for another position is never shown.
    rawNet: s?.rawNet?.node === v.position.node ? s.rawNet : null,
    katago: !!v.engine.secondary?.katago,
    engine: !!v.engine.secondary,
    hovered: allowed ? hoveredCandidate2 : null,
    preview: hoveredVariation(v, true)?.preview ?? null,
    ownership: secondaryOwnership(v, allowed ? hoveredCandidate2 : null),
  };
  const previewSig = scene.preview ? `${scene.preview.order}:${scene.preview.length}:${scene.preview.pv.length}` : "";
  if (changed("board2", `${size}:${docSig}:${secondaryRevision}:${displaySig2}:${ui}:${scene.hovered}:${previewSig}:${scene.names}:${layerSignature("right", scene.ownership)}:${scoreSig}:${scene.mark?.x},${scene.mark?.y}`))
    rightReport = paintBoard(prepare($("board2"), d.right), mainGeom(v, d.right), d.right, scene);
  // drawPonderingStateForExtraMode2 (LizzieFrame.java:6663-6719): each engine's name and 思考
  // state, then its visits and side-to-move win rate for this node when it has a result. The
  // secondary strip shows the secondary engine's own 思考 state (Java read the primary's).
  const fg = v.engine.foreground;
  const se = v.engine.secondary;
  const name = (text: string) => (text.length > 15 ? text.substring(0, 10) : text);
  const pondering = (on: boolean) => `${t("LizzieFrame.display.pondering")} ${on ? t("LizzieFrame.display.on") : t("LizzieFrame.display.off")}`;
  const result = (a: NodeAnalysisView | null | undefined) =>
    a && a.candidates.length ? `${t("LizzieFrame.visits")}${playouts(a.visits)} ${t("LizzieFrame.winrate")}${a.winrate.toFixed(1)}%` : null;
  const text1 = `${t("LizzieFrame.mainEngine")}${name(fg?.name ?? "")} ${pondering(v.engine.analysing)}${t("LizzieFrame.display.space")}`;
  const text2 = se ? `${t("LizzieFrame.subEngine")}${name(se.name)} ${pondering(se.analysing)}` : t("LizzieFrame.subEngine");
  const result1 = result(v.analysis);
  const result2 = result(s?.analysis);
  if (!changed("dualStatus", `${size}:${d.size}:${text1}:${text2}:${result1}:${result2}`)) return;
  const g = prepare($<HTMLCanvasElement>("dualStatus"), { x: 0, y: 0, w: W, h: H });
  const first = paintDualStrip(g, text1, 0, d.size, 18, d.size, W, H);
  const second = paintDualStrip(g, text2, d.size, d.size, 18, d.size, W, H);
  const third = result1 === null ? null : paintDualStrip(g, result1, first.width + 5, d.size, 18, d.size, W, H);
  const fourth = result2 === null ? null : paintDualStrip(g, result2, d.size + second.width + 5, d.size, 18, d.size, W, H);
  dualStatus = [first, second, third, fourth].filter((strip) => !!strip?.text).map((strip) => strip!.text);
}

/** No candidate is kept beyond 最大候选点数 on the right board (lastTimeUnlimited is the main
 *  board's). */
const NO_POINTS = new Set<string>();

// ------------------------------------------------------------------ acceptance probe

/** Answers the acceptance channel's `{"query":"ui"}` with what the window shows. The event is only
 *  emitted by builds with the `acceptance` feature. */
function uiReport() {
  const active = document.activeElement as HTMLElement | null;
  return {
    title: document.title,
    focus: active ? { tag: active.tagName.toLowerCase(), id: active.id, className: active.className } : null,
    menuOpen: menuOpen(),
    dialogs: [...document.querySelectorAll(".modal-box")].map((box) => ({
      title: box.querySelector(".modal-title")?.textContent ?? "",
      body: box.querySelector(".modal-body")?.textContent ?? "",
    })),
    status: statusLines(),
    candidateCount: boardReport?.candidates.length ?? 0,
    hoveredCandidate,
    subBoardCandidateIndex: subCandidate,
    subBoardBranchLength: subLength,
    tableRowCount: tablesReport().main.rows.length,
    tableRows: tablesReport().main.rows,
    /** Both 选点列表 tables (cells as drawn, clicked / selected rows, the 独立选点列表's sort) and
     *  the frame window. */
    tables: { ...tablesReport(), window: frameReport() },
    graphPointCount: graphReport?.pointCount ?? 0,
    graph: graphReport,
    statusLines: statusLines(),
    candidates: state.view?.analysis?.candidates.map((c) => ({ order: c.order, point: c.point, visits: c.visits, pvLength: c.pv.length })) ?? [],
    /** Client-area geometry so automation points at real board / sub-board / graph / table spots. */
    geometry: panels && state.view && (() => {
      const v = state.view!;
      const m = $("main").getBoundingClientRect();
      const g = mainGeom(v, panels.board);
      const s = boardGeom(panels.sub, v.document.size.width, v.document.size.height, SUB_MARGIN_RATIO);
      const tb = $("table").getBoundingClientRect();
      const ct = $("commentText").getBoundingClientRect();
      return {
        board: { x: m.left + g.x + g.margin, y: m.top + g.y + g.margin, square: g.square, margin: g.margin, size: g.size },
        sub: { x: m.left + s.x + s.margin, y: m.top + s.y + s.margin, square: s.square },
        graph: { x: m.left + panels.graph.x, y: m.top + panels.graph.y, w: panels.graph.w, h: panels.graph.h },
        table: { x: tb.left, y: tb.top, w: tb.width, h: tb.height },
        comment: { x: ct.left, y: ct.top, w: ct.width, h: ct.height },
      };
    })(),
    /** 界面 > 面板 / 工具栏 (T18): the panels shown, their rectangles, the separator zones (panel
     *  coordinates), the X overlay and the toolbars' heights. */
    layout: panels && {
      shown: panels.shown,
      rects: { board: panels.board, captured: panels.captured, stat: panels.stat, graph: panels.graph, comment: panels.comment, tree: panels.tree, table: panels.table, sub: panels.sub },
      handles: panels.handles,
      controls: controlsShowing(),
      topToolbar: $("toolbar").offsetHeight,
      bottomToolbar: $("bottombar").offsetHeight,
    },
    comment: $("commentText").textContent,
    moveField: $<HTMLInputElement>("moveInput").value,
    bottomBar: [...document.querySelectorAll<HTMLElement>("#bottombar .bb")].filter((b) => b.style.display !== "none").map((b) => b.textContent),
    editing: {
      placement: editing.placement,
      insert: editing.insert,
      markup: editing.markup,
      setupTool: editing.setupTool,
      commentEditing: commentEditing(),
      contextMenu: contextMenuItems(),
    },
    view: state.view ? { seq: state.view.seq, node: state.view.position.node, document: state.view.document.id } : null,
    tree: treeView && {
      top: treeView.top,
      left: treeView.left,
      vertical: !!treeView.vertical,
      horizontal: !!treeView.horizontal,
      /** Client point of the vertical bar's down arrow. */
      downArrow: (() => {
        const r = $("tree").getBoundingClientRect();
        return { x: Math.round(r.right - SCROLLBAR / 2), y: Math.round(r.top + treeView.viewH - ARROW / 2) };
      })(),
    },
    lastSound: state.lastSound,
    recovery: recoveryReport(),
    /** 评论/问题手 panel (T13): page, side, sort, badge, rows and the board's marked move. */
    problems: problemsReport(),
    /** 超级鹰眼 (T15): the open windows, their titles and what each shows. */
    hawkeye: hawkeyeReport(),
    /** 批量分析 (T17): whether its settings dialog is shown and what the 进度表 window last drew. */
    batch: batchReport(),
    /** Visible menu, toolbar, bottom-bar and dialog controls with client-area centres, so
     *  automation clicks the real controls. */
    targets: [...document.querySelectorAll<HTMLElement>("[data-key], [data-name], #moveInput, .modal-box button")]
      .map((el) => ({ el, r: el.getBoundingClientRect() }))
      .filter(({ r }) => r.width > 0 && r.height > 0)
      .map(({ el, r }) => ({
        key: el.dataset.key ?? el.dataset.name ?? el.id ?? "",
        text: el.textContent ?? "",
        checked: el instanceof HTMLInputElement && el.type === "checkbox" ? el.checked : undefined,
        value: el instanceof HTMLInputElement && el.type === "text" ? el.value : undefined,
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + r.height / 2),
      })),
    /** What the boards draw (T10 display modes): `board` carries the coordinate labels, the best
     *  ring, unimportant circles, white badges, PV visits and the 下一手 详细 drawing. */
    display: {
      boardUi: { ...boardUi },
      rawNet: state.view?.rawNet ? { mode: state.view.rawNet.mode, node: state.view.rawNet.node, hasPolicy: !!state.view.rawNet.policy } : null,
      board: boardReport,
      sub: subReport && { ...subReport, candidateIndex: subCandidate },
      /** Kata形势 / 统计 / AI总评分 / 终局点目 (T11): what the panels and overlays drew. */
      information: {
        stat: statReport,
        aiScore: state.view ? { black: state.view.statistics.ai_score_black === null ? null : aiScoreText(state.view.statistics.ai_score_black), white: state.view.statistics.ai_score_white === null ? null : aiScoreText(state.view.statistics.ai_score_white) } : null,
        kataEstimate: state.view ? { ...state.view.kata_estimate.state, hidden: state.view.kata_estimate.hidden, style: state.view.settings.display.kataEstimateStyle, mainBoard: boardReport?.estimate ?? null, subBoard: subReport?.ownership ?? null } : null,
        score: { overlay: boardReport?.score ?? null, result: state.view?.score?.result ?? null, dialog: scoreDialogReport() },
      },
      replay: {
        running: replaying(),
        length: boardReport?.preview && replaying() ? boardReport.preview.length : null,
      },
      /** The hovered variation is the 下一手 详细 one (actual move + next position's best PV). */
      nextMovePreview: boardReport?.preview?.order === -1,
      /** Candidates kept beyond 最大候选点数 by lastTimeUnlimited ("x,y"). */
      keptBeyondLimit: [...limitMemory.unlimited],
      tree: { numbers: state.view?.settings.display.treeMoveNumbers ?? false, drawn: treeNumbersDrawn },
      autoplay: autoplayReport(),
    },
    /** 双引擎模式 (T12): the mode, the right board's drawing, both boards' client geometry, the
     *  status strips as drawn, the secondary engine menu's title and the candidate windows. */
    dual: {
      mode: state.view?.settings.extra_mode ?? null,
      rightBoard: dual ? rightReport : null,
      geometry: (() => {
        const d = dual;
        const v = state.view;
        if (!d || !v) return null;
        const m = $("main").getBoundingClientRect();
        const at = (r: Rect) => {
          const g = mainGeom(v, r);
          return { x: m.left + g.x + g.margin, y: m.top + g.y + g.margin, square: g.square, margin: g.margin, size: g.size };
        };
        return { left: at(d.left), right: at(d.right) };
      })(),
      status: dual ? dualStatus : [],
      menu2Title: secondaryMenuTitle(),
      frames: framesReport(),
    },
  };
}

// ------------------------------------------------------------------ boot

async function boot() {
  // Views published before the snapshot is applied are held back and replayed after it; stale
  // ones (lower seq) are then dropped by apply, so no tree update can be missed.
  let early: WorkbenchView[] | null = [];
  await onView((v) => (early ? early.push(v) : apply(v)));
  const [first, appVersion] = await Promise.all([snapshot(), getVersion()]);
  version = appVersion;
  setLanguage(first.settings.active_language);
  document.documentElement.lang = isChinese() ? "zh" : "en";
  buildMenubar($("menubar"));
  buildToolbar($("toolbar"));
  buildBottomBar($("bottombar"));
  mountSetupBanner();
  bindProblemPanel();
  bindPointer();
  bindPanels(schedule);
  bindCommentEditor($("commentText"));
  bindCaptureSurface(() => panels && { panels, width: $("main").clientWidth, height: $("main").clientHeight, background: backgroundUrl }, render);
  await Promise.all([
    loadImages(["board.png", "black0.png", "white0.png", "background.jpg", "heatmap.png", "smallUp.png", "smallDown.png", "smallLeft.png", "smallRight.png"]),
    document.fonts.load('12px "Lizzie Winrate"'),
  ]);
  subscribe(schedule);
  bindScoreDialog(() => (panels ? $("board").getBoundingClientRect() : null));
  startTitleUpdates(schedule);
  // F, holding Z, 延迟显示候选点 and T repaint both boards at once.
  onBoardUiChange(schedule);
  bindAutoplayHost({
    target: () => (state.view ? hoveredVariation(state.view)?.target ?? null : null),
    repaint: schedule,
  });
  bindReplayHost({
    target: () => (state.view ? hoveredVariation(state.view)?.target ?? null : null),
    repaint: schedule,
  });
  bindTables({
    // handleTableClick sets mouseOverCoordinate (LizzieFrame.java:14665-14691): a candidate's point
    // shows that candidate; the actual move outside the candidates is the 下一手 详细 point, which
    // previews it followed by the next position's variation (BoardRenderer.mouseOveredMove).
    showVariation: (point) => {
      const v = state.view;
      if (!point || !v) return hover(null);
      // In 双引擎模式 the point is shared: the right board shows the secondary engine's candidate.
      const doubleEngine = v.settings.extra_mode === "DoubleEngine";
      const at = (c: { point: Point | null }) => c.point?.x === point.x && c.point.y === point.y;
      if (v.analysis?.candidates.some(at) || (doubleEngine && v.secondary?.analysis?.candidates.some(at))) return hover(`${point.x},${point.y}`);
      const next = nextMoveTarget(v) ?? (doubleEngine ? nextMoveTarget(v, v.secondary?.board ?? null) : null);
      hover(next && next.x === point.x && next.y === point.y ? NEXT_MOVE : null);
    },
    variations: () => {
      const v = state.view;
      const shown = (right: boolean) => {
        const variation = v ? hoveredVariation(v, right) : null;
        return variation?.preview ? { ...variation.target, length: variation.preview.length } : null;
      };
      return { left: shown(false), right: v?.settings.extra_mode === "DoubleEngine" ? shown(true) : null };
    },
    repaint: schedule,
  });
  await onUnavailable(() => unavailable());
  await listen("acceptance-probe", () => invoke("acceptance_ui_report", { report: uiReport() }));
  await startAutoSave();
  window.addEventListener("resize", schedule);
  apply(first);
  for (const v of early) apply(v);
  early = null;
  render();
  await startFrame(schedule);
  await startHawkeye(schedule);
  await startBatchTable();
  await getCurrentWindow().show();
  await getCurrentWindow().setFocus();
  schedule();
  // First use (no settings of this product yet): offer the LizzieYzy Next import.
  if (first.java_import.offer) void openJavaImport(true);
}

// A start-up failure must not leave the hidden window invisible forever.
boot().catch(async (e) => {
  console.error("start-up failed", e);
  await getCurrentWindow().show();
  message("LizzieYzy Tauri", e instanceof Error ? `${e}\n${e.stack ?? ""}` : String(e), "error");
});
