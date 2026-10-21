// Workbench entry: Java LizzieFrame presentation state and incremental painters.
// Record and engine operations go through the Rust-owned command interface (app.ts).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { apply, copyRecord, exportRecord, navigate, pasteRecord, run, saveRecord, saveRecordAs, state, subscribe, unavailable, windowTitle } from "./app";
import { buildBottomBar, layoutBottomBar } from "./chrome/bottombar";
import { buildMenubar, menuOpen } from "./chrome/menubar";
import { buildToolbar, renderToolbar } from "./chrome/toolbar";
import { onUnavailable, onView, snapshot } from "./contract/client";
import type { CandidateView, HistoryPoint, Navigation, NodeAnalysisView, Point, WorkbenchView } from "./contract/generated";
import { dialogOpen, message } from "./dialogs";
import { isChinese, product, setLanguage, t } from "./i18n";
import { openJavaImport } from "./javaImport";
import { boardGeom, graphGeom, inFrameLayout, mainMarginRatio, SUB_MARGIN_RATIO, type BoardGeom, type Panels, type Rect } from "./workbench/layout";
import { boardUi, candidatesShownFor, onBoardUiChange } from "./display/state";
import { bindReplayHost, finishReplayBranch, replayLength, replaying, type ReplayTarget } from "./display/replay";
import { autoplayReport, autoReplayLength, bindAutoplayHost, subAutoplayLength, subWheeled } from "./display/autoplay";
import { sameAnalysis, sameHistory } from "./workbench/viewEquality";
import { handleEngineKey } from "./engine/keys";
import { handleDisplayKey } from "./display/keys";
import { mountSetupBanner } from "./setup/banner";
import "./editing/commands";
import { bindCommentEditor, commentEditing } from "./editing/comment";
import { contextMenuItems, contextMenuOpen } from "./editing/contextMenu";
import { bindBoardInput, handleEditKey } from "./editing/input";
import { editing } from "./editing/state";
import { bindCaptureSurface, copyMainBoardPicture, copySubBoardPicture, saveMainBoardPicture, saveSubBoardPicture, saveWinratePicture } from "./recovery/capture";
import { startAutoSave } from "./recovery/commands";
import { recoveryReport } from "./recovery/panel";
import {
  loadImages,
  paintBackground,
  paintBoard,
  paintCaptured,
  paintGraph,
  paintStat,
  paintStatus,
  paintSubBoard,
  paintTree,
  prepare,
  playouts,
  ARROW,
  SCROLLBAR,
  type BoardReport,
  type BoardScene,
  type PreviewVariation,
  type SubBoardReport,
  type TreeViewport,
} from "./workbench/paint";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

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
let hoveredCandidate: number | null = null;
let hoveredPoint: string | null = null;
let selectedCandidate: number | null = null;
let subCandidate = 0;
let subLength: number | null = null;
/** The pointer rests on the sub board (SubBoardRenderer.isMouseOver with no-refresh-on-sub). */
let subOver = false;
/** The sub board's frozen analysis while the pointer rests there, for its position only. */
let subFrozen: { node: number; analysis: NodeAnalysisView | null; revision: number } | null = null;
let subFrozenRevision = 0;
/** The hovered variation as first shown, kept while 鼠标悬停显示变化图时,变化图不刷新 is on. */
let hoverFrozen: { key: string; pv: (Point | null)[]; pv_captures: Point[][]; pv_visits: number[] } | null = null;
let presentationFor = "";
let boardReport: BoardReport | null = null;
let subReport: SubBoardReport | null = null;
/** Everything the boards draw from the view besides the position and analysis, per view. */
let displaySig = "";
let displaySigFor: WorkbenchView | null = null;
let tableRowCount = 0;
let graphPointCount = 0;
let maxScoreLead = 15;
let graphDocument = -1;
let graphNodesSource = state.nodes;
let graphLineKey = "";
let graphLineByMove = new Map<number, number>();
let mainLine = new Set<number>();
let historyByNode = new Map<number, HistoryPoint>();
/** Bumped when the shown analysis / history differ field-wise from the previous view's. */
let analysisRevision = 0;
let historyRevision = 0;
let projectedAnalysis: NodeAnalysisView | null = null;
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

/** The hovered candidate rank, or -1 for the 下一手 详细 point (BoardRenderer.mouseOveredMove). */
function hover(order: number | null) {
  const candidate = state.view?.analysis?.candidates.find((c) => c.order === order);
  const point = order === -1 ? NEXT_MOVE : candidate?.point ? `${candidate.point.x},${candidate.point.y}` : null;
  if (hoveredCandidate === order && hoveredPoint === point) return;
  hoveredCandidate = order;
  hoveredPoint = point;
  // notChangedMouseOverMove: a variation kept by 变化图不刷新 lasts only while the pointer stays.
  if (point === null) hoverFrozen = null;
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
    const historyChanged = !sameHistory(projectedHistory, v.history);
    projectedHistory = v.history;
    if (historyChanged) {
      historyRevision++;
      historyByNode = new Map(v.history.map((p) => [p.node, p]));
    }
    const documentChanged = graphDocument !== v.document.id;
    if (documentChanged) {
      graphDocument = v.document.id;
      maxScoreLead = 15;
    }
    if (historyChanged || documentChanged)
      for (const p of v.history) if (p.black.score_lead !== null) maxScoreLead = Math.max(maxScoreLead, Math.abs(p.black.score_lead));
  }
  const lineKey = `${v.document.id}:${v.position.node}:${v.position.line_end}`;
  if (graphNodesSource !== state.nodes || graphLineKey !== lineKey) {
    graphNodesSource = state.nodes;
    graphLineKey = lineKey;
    graphLineByMove = new Map();
    mainLine = new Set();
    for (let id: number | undefined = state.root; id !== undefined; id = state.nodes.get(id)?.children[0]) {
      mainLine.add(id);
      const n = state.nodes.get(id);
      if (n) graphLineByMove.set(n.move_number, id);
    }
    // The displayed branch owns overlapping columns; the dashed trunk owns its continuation.
    for (const id of state.line) {
      const n = state.nodes.get(id);
      if (n) graphLineByMove.set(n.move_number, id);
    }
  }
  if (presentationFor !== lineKey) {
    presentationFor = lineKey;
    hoveredCandidate = null;
    hoveredPoint = null;
    hoverFrozen = null;
    selectedCandidate = null;
    // SubBoardRenderer.clearAfterMove; the frozen sub board belongs to the position it was taken at.
    subCandidate = 0;
    subLength = null;
    subFrozen = null;
  }
  // Java keeps the hovered coordinate, even when streaming visits reorder the candidates.
  hoveredCandidate = hoveredPoint === null ? null : hoveredPoint === NEXT_MOVE ? -1 : v.analysis?.candidates.find((c) => c.point && `${c.point.x},${c.point.y}` === hoveredPoint)?.order ?? null;
  subCandidate = Math.min(subCandidate, Math.max(0, ((subFrozen?.analysis ?? v.analysis)?.candidates.length ?? 0) - 1));
  if (displaySigFor !== v) {
    displaySigFor = v;
    const fg = v.engine.foreground;
    displaySig = JSON.stringify([v.settings.display, v.board, v.rawNet, fg?.katago ?? null, !!fg, v.settings.engine.analysis.limitMaxSuggestion, v.settings.engine.analysis.limitBranchLength]);
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
  return candidatesShownFor(v.position.to_play) && v.settings.display.suggestionVariations && v.rawNet === null && !boardUi.policy;
}

/** The 下一手 详细 point while it shows (isShowingNextMoveBlunder): hovering it previews the actual
 *  move followed by the next position's best variation, before any candidate (1780-1799). */
function nextMoveTarget(v: WorkbenchView): Point | null {
  return hoverAllowed(v) && v.settings.display.nextMoves && v.position.next_color ? v.board.next_move?.point ?? null : null;
}

/** The hovered candidate's variation (BoardRenderer.prepareSelection, BoardRenderer.java:1640-1711):
 *  frozen while 不刷新 is on, shortened by a running replay or auto-replay, and not shown for a
 *  candidate without enough playouts or beyond 最大候选点数 unless it is the next move or the
 *  variation auto-replays. */
function hoveredVariation(v: WorkbenchView): { target: ReplayTarget; preview: PreviewVariation | null } | null {
  const candidates = v.analysis?.candidates ?? [];
  const nextPoint = hoveredCandidate === -1 ? nextMoveTarget(v) : null;
  const c = hoverAllowed(v) && !nextPoint ? candidates.find((c) => c.order === hoveredCandidate) : undefined;
  const point = nextPoint ?? c?.point;
  const info = v.board.next_move;
  if (!point || (nextPoint && !info)) {
    hoverFrozen = null;
    return null;
  }
  const source = nextPoint ? { pv: info!.pv, pv_captures: info!.pv_captures, pv_visits: info!.pv_visits } : { pv: c!.pv, pv_captures: c!.pv_captures, pv_visits: c!.pv_visits };
  const key = `${v.document.id}:${v.position.node}:${nextPoint ? "next" : ""}${point.x},${point.y}`;
  if (!v.settings.display.noRefreshOnMouse || hoverFrozen?.key !== key) hoverFrozen = { key, ...source };
  const { pv, pv_captures, pv_visits } = hoverFrozen;
  const limit = v.settings.engine.analysis.limitBranchLength;
  const steps = limit > 0 ? Math.min(limit, pv.length) : pv.length;
  const target = { key, steps };
  const replayed = replayLength(key) ?? autoReplayLength(key, steps);
  const order = nextPoint ? -1 : c!.order;
  if (c && replayed === null && !v.settings.display.autoReplayBranch) {
    const next = v.position.next_moves[0];
    const needShow = !!next && next.x === point.x && next.y === point.y;
    const ratio = c.allocation_visits / Math.max(1, ...candidates.map((x) => x.allocation_visits));
    const maxSuggestion = v.settings.engine.analysis.limitMaxSuggestion;
    const kept = limitMemory.unlimited.has(`${point.x},${point.y}`);
    if (!needShow && (ratio <= v.settings.display.minPlayoutRatio || (maxSuggestion > 0 && c.order + 1 > maxSuggestion && !kept)))
      return { target, preview: null };
  }
  return { target, preview: { order, pv, pv_captures, pv_visits, length: replayed ?? steps } };
}

function subAnalysis(v: WorkbenchView): NodeAnalysisView | null {
  return subFrozen && subFrozen.node === v.position.node ? subFrozen.analysis : v.analysis;
}

function engineTitle(v: WorkbenchView): string {
  // LizzieFrame.updateTitle returns while trying: the title stays LizzieFrame.tryTitle.
  if (v.editing.trying) return windowTitle(version);
  const base = windowTitle(version), foreground = v.engine.foreground, a = v.analysis;
  let prefix = "";
  if (a && a.visits > 0) {
    // updateTitle: Black's win rate and Black's lead (core `black`; the board difference under 盘面).
    const { winrate, score_lead: lead } = a.black;
    prefix = `[${winrate.toFixed(1)}${lead === null ? "" : ` ${lead.toFixed(1)}`} ${playouts(a.visits)}] `;
  }
  if (!foreground) return prefix + base;
  const engine = `${foreground.name}${v.engine.analysing && a ? ` ${playouts(a.visits)} visits` : ` - ${t("LizzieFrame.speedUnit")}`}`;
  const stem = `LizzieYzy Tauri ${version}`;
  return `${prefix}${stem} - ${engine}${base.slice(stem.length)}`;
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
  if (dialogOpen() || menuOpen() || contextMenuOpen() || textTarget(e.target)) return;
  // The 存档与读档 controls keep their keys, like the Swing components holding the focus there.
  if (e.target instanceof Element && e.target.closest("#recovery")) return;
  if (handleEngineKey(e) || handleEditKey(e) || handleDisplayKey(e)) {
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
  } else if (!e.metaKey && key !== " " && JAVA_BOUND.has(key)) {
    e.preventDefault();
    flash(product("unavailable"));
  }
});

// ------------------------------------------------------------------ pointer

function bindPointer() {
  const main = $("main");
  // Input.mouseWheelMoved: one move per notch over the workbench. Scroll panes take their own
  // wheel: the comment, the candidate list and the variation tree (a JScrollPane, scrolled below).
  main.addEventListener(
    "wheel",
    (ev) => {
      if ((ev.target as HTMLElement).closest("#comment, #table, #tree, #sub, #graph, #recovery")) return;
      ev.preventDefault();
      if (ev.deltaY !== 0) navigate(ev.deltaY > 0 ? { op: "Forward", count: 1 } : { op: "Back", count: 1 });
    },
    { passive: false },
  );
  bindBoardInput($("board"), {
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
    clearTableSelection: () => {
      if (selectedCandidate === null) return false;
      selectedCandidate = null;
      hover(null);
      schedule();
      return true;
    },
  });
  $("board").addEventListener("mousemove", (ev) => {
    const v = state.view;
    if (!v || !panels) return;
    if (!hoverAllowed(v)) return hover(null);
    const geo = mainGeom(v, panels.board);
    const rect = $("main").getBoundingClientRect();
    const x = ev.clientX - rect.left - geo.x - geo.margin;
    const y = ev.clientY - rect.top - geo.y - geo.margin;
    const near = (p: Point) => Math.hypot(x - p.x * geo.square, y - p.y * geo.square) <= geo.stoneRadius + 1;
    // The 下一手 详细 point comes first (BoardRenderer.mouseOveredMove).
    const next = nextMoveTarget(v);
    if (next && near(next)) return hover(-1);
    // Beyond 最大候选点数 the candidates exist only with show-nosugg-circle (BoardData.tryToLimitMoves).
    const limit = v.settings.engine.analysis.limitMaxSuggestion;
    const candidate = v.analysis?.candidates.find((c) => c.point && c.visits > 0 && near(c.point) &&
      (limit === 0 || c.order < limit || v.settings.display.noSuggCircle || limitMemory.unlimited.has(`${c.point.x},${c.point.y}`)));
    hover(candidate?.order ?? null);
  });
  // The open board popup keeps the preview it was opened over.
  $("board").addEventListener("mouseleave", () => {
    finishReplayBranch();
    if (!contextMenuOpen()) hover(null);
  });
  // The replay ends when the window is no longer the active one (LizzieFrame.java:11650).
  window.addEventListener("blur", finishReplayBranch);
  $("table").addEventListener("mouseleave", () => hover(null));
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
  let queuedGraphNode: { node: number; document: number } | null = null;
  let navigatingGraph = false;
  graph.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0 || !panels || !state.view) return;
    ev.preventDefault();
    const nodes = new Map(graphLineByMove), documentId = state.view.document.id;
    const r = graph.getBoundingClientRect();
    const geo = graphGeom(panels.graph.w, panels.graph.h, Math.max(state.mainLineNodes, state.line.size));
    const scrub = async (e: MouseEvent) => {
      const move = Math.min(geo.slots - 1, Math.max(0, Math.round((e.clientX - r.left) * geo.slots / geo.width))) + 1;
      const node = nodes.get(move);
      if (node === undefined || state.view?.document.id !== documentId) return;
      queuedGraphNode = { node, document: documentId };
      if (navigatingGraph) return;
      navigatingGraph = true;
      try {
        while (queuedGraphNode) {
          const target = queuedGraphNode;
          queuedGraphNode = null;
          if (state.view?.document.id === target.document && state.view.position.node !== target.node) await run({ type: "GotoNode", node: target.node });
        }
      } finally { navigatingGraph = false; }
    };
    scrub(ev);
    const end = () => { document.removeEventListener("mousemove", scrub); document.removeEventListener("mouseup", end); };
    document.addEventListener("mousemove", scrub);
    document.addEventListener("mouseup", end);
  });
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
  for (const id of ["problemsTab"]) $(id).addEventListener("click", unavailable);
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

function boardPointAt(clientX: number, clientY: number): Point | null {
  const v = state.view;
  if (!panels || !v) return null;
  const { width, height } = v.document.size;
  const geo = mainGeom(v, panels.board);
  const rect = $("main").getBoundingClientRect();
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

/** LizzieFrame.drawPonderingState: only ambient status depends on active pondering. */
function statusLines(): string[] {
  const hint = notice && notice.until > performance.now() ? notice.text : t("LizzieFrame.prompt.showControlsHint");
  return [
    state.view?.engine.foreground?.name ?? t("LizzieFrame.noEngineText"),
    `${t("LizzieFrame.display.pondering")} ${state.view?.engine.analysing ? t("LizzieFrame.display.on") : t("LizzieFrame.display.off")}${t("LizzieFrame.display.space")}`,
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
  panels = inFrameLayout(W, H);
  const P = panels;
  const size = `${W}x${H}:${window.devicePixelRatio}`;
  const pos = v.position;
  const { width: cols, height: rows } = v.document.size;
  // Edits change a node's position, marks or side to play in place: the revision moves on.
  const docSig = `${v.document.id}:${v.document.revision}:${pos.node}:${pos.to_play}`;
  projectAnalysis(v);
  if (changed("bg", size)) installBackground(size, W, H, P.glass);

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
    display: v.settings.display,
    suggestionLimit: v.settings.engine.analysis.limitMaxSuggestion,
    unlimited: limitMemory.unlimited,
    candidatesShown: candidatesShownFor(pos.to_play),
    policy: boardUi.policy,
    rawNetOn: v.rawNet !== null,
    // A result for another position is never shown.
    rawNet: v.rawNet?.node === pos.node ? v.rawNet : null,
    katago: !!v.engine.foreground?.katago,
    engine: !!v.engine.foreground,
    hovered: hoverAllowed(v) ? hoveredCandidate : null,
    preview: hoveredVariation(v)?.preview ?? null,
  };
  const ui = `${boardUi.showBlack}:${boardUi.showWhite}:${boardUi.policy}`;
  const previewSig = scene.preview ? `${scene.preview.order}:${scene.preview.length}:${scene.preview.pv.length}` : "";
  if (changed("board", `${size}:${docSig}:${analysisRevision}:${displaySig}:${ui}:${scene.hovered}:${previewSig}:${scene.names}`))
    boardReport = paintBoard(prepare($("board"), P.board), mainGeom(v, P.board), P.board, scene);
  // The resting pointer freezes the sub board's variation for this position (no-refresh-on-sub).
  const over = subOver && v.settings.display.noRefreshOnSub;
  if (!over) subFrozen = null;
  else if (!subFrozen || subFrozen.node !== pos.node) subFrozen = { node: pos.node, analysis: v.analysis, revision: ++subFrozenRevision };
  const ownership = scene.rawNet?.mode === "Ownership" && scene.katago ? scene.rawNet.ownership : null;
  const subShown = subLength ?? subAutoplayLength();
  if (changed("sub", `${size}:${docSig}:${analysisRevision}:${displaySig}:${ui}:${subCandidate}:${subShown}:${over}:${subFrozen?.revision}`))
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
      ownership,
      pureEstimate: v.settings.display.pureEstimate,
    });
  if (changed("captured", `${size}:${docSig}:${v.komi}:${v.engine.analysing}:${v.engine.rules_key}`))
    paintCaptured(prepare($("captured"), P.captured), P.captured, {
      toPlay: pos.to_play,
      blackCaptures: pos.black_captures,
      whiteCaptures: pos.white_captures,
      moveNumber: pos.move_number,
      komi: v.komi,
      analysing: v.engine.analysing,
      rules: rulesCaption(v.engine.rules_key),
    });
  const parent = state.nodes.get(pos.node)?.parent ?? -1;
  const current = historyByNode.get(pos.node) ?? null;
  const previous = historyByNode.get(parent) ?? null;
  const leadWithKomi = v.settings.display.scoreLeadWithKomi;
  if (changed("stat", `${size}:${docSig}:${parent}:${historyRevision}:${v.analysis?.score_stdev}:${leadWithKomi}`))
    paintStat(prepare($("stat"), P.stat), P.stat, current, previous, pos.move_number, v.analysis?.score_stdev ?? null, leadWithKomi);
  const lineNodes = Math.max(state.mainLineNodes, state.line.size);
  if (changed("graph", `${size}:${docSig}:${lineNodes}:${historyRevision}`))
    graphPointCount = paintGraph(prepare($("graph"), P.graph), P.graph, lineNodes, { history: v.history, current: pos.node, line: state.line, mainLine, maxScore: maxScoreLead });
  const statusRect = { x: 0, y: P.ponderingY, w: P.board.x, h: H - P.ponderingY };
  const lines = statusLines();
  if (changed("status", `${size}:${lines.join("|")}`)) paintStatus(prepare($("status"), statusRect), statusRect, lines);
  if (treeScrollFor !== docSig) {
    treeScroll = null;
    treeScrollFor = docSig;
  }
  if (treeSource !== state.nodes) {
    treeSource = state.nodes;
    treeRevision++;
  }
  const treeNumbers = v.settings.display.treeMoveNumbers;
  if (changed("tree", `${size}:${docSig}:${treeRevision}:${treeNumbers}:${treeScroll?.top}:${treeScroll?.left}`)) {
    const painted = paintTree(prepare($("tree"), P.tree), P.tree, state.nodes, state.root, pos.node, state.line, treeScroll, treeNumbers);
    treeHits = painted.hits;
    treeView = painted.viewport;
    treeNumbersDrawn = painted.numbers;
  }
  place($("comment"), P.comment);
  if (changed("comment", `${docSig}:${pos.comment}`)) $("commentText").textContent = pos.comment;
  place($("table"), P.table);
  if (changed("table", `${size}:${docSig}:${analysisRevision}:${historyRevision}:${selectedCandidate}`)) renderTable();
  if (changed("bottombar", `${$("bottombar").clientWidth}`)) layoutBottomBar($("bottombar"));
  renderToolbar();
  const title = engineTitle(v);
  if (changed("title", title)) {
    document.title = title;
    getCurrentWindow().setTitle(title);
  }
}

/** SuggestionTableScrollPane / LizzieFrame.java:14725-15015. Coordinates stay hidden by default. */
const TABLE_HEADER = navigator.userAgent.includes("Windows") ? 19 : 17;
const TABLE_BASE_WIDTHS = [15, 30, 30, 45, 30];
let tableStructure = "";
function renderTable() {
  const host = $("table"), v = state.view;
  const candidates = v?.analysis?.candidates ?? [];
  const hasScore = candidates.some((c) => c.score_lead !== null);
  const bases = hasScore ? TABLE_BASE_WIDTHS : TABLE_BASE_WIDTHS.slice(0, 4);
  const structure = `${host.clientWidth}:${hasScore}:${t("AnalysisFrame.column1")}`;
  if (tableStructure !== structure) {
    tableStructure = structure;
    const extra = host.clientWidth - bases.reduce((s, w) => s + w, 0);
    const widths = bases.map((w, k) => w + Math.floor(extra * (k + 1) / bases.length) - Math.floor(extra * k / bases.length));
    const heads = [t("AnalysisFrame.column1"), t("LizzieFrame.listColumn2"), t("AnalysisFrame.column5"), t("LizzieFrame.listColumn4")];
    if (hasScore) heads.push(t("AnalysisFrame.column8"));
    const table = document.createElement("table");
    table.style.width = `${host.clientWidth}px`;
    const colgroup = document.createElement("colgroup");
    for (const w of widths) {
      const col = document.createElement("col");
      col.style.width = `${w}px`;
      colgroup.append(col);
    }
    const headRow = document.createElement("tr");
    headRow.style.height = `${TABLE_HEADER}px`;
    for (const h of heads) {
      const th = document.createElement("th");
      th.textContent = h;
      headRow.append(th);
    }
    const thead = document.createElement("thead"), body = document.createElement("tbody");
    thead.append(headRow);
    body.addEventListener("mousemove", (ev) => {
      const row = (ev.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-order]");
      if (row) hover(Number(row.dataset.order));
    });
    body.addEventListener("click", (ev) => {
      const row = (ev.target as HTMLElement).closest<HTMLTableRowElement>("tr[data-order]");
      if (!row) return;
      const order = Number(row.dataset.order);
      selectedCandidate = selectedCandidate === order ? null : order;
      hover(selectedCandidate);
      schedule();
    });
    table.append(colgroup, thead, body);
    host.replaceChildren(table);
  }
  const data: { candidate: CandidateView; actual: boolean }[] = candidates.slice().sort((a, b) => b.visits - a.visits).map((candidate) => ({ candidate, actual: false }));
  const next = v?.position.next_moves[0];
  const actual = next ? data.find((row) => row.candidate.point?.x === next.x && row.candidate.point.y === next.y) : undefined;
  if (actual) {
    if (actual.candidate.order === 0) {
      actual.actual = true;
      data.splice(data.indexOf(actual), 1);
      data.unshift(actual);
    } else {
      const nextId = v ? state.nodes.get(v.position.node)?.children[0] : undefined;
      const nextData = nextId === undefined ? undefined : historyByNode.get(nextId);
      const candidate = nextData && nextData.visits > actual.candidate.visits
        ? { ...actual.candidate, visits: nextData.visits, allocation_visits: nextData.visits, ...nextData.as_candidate }
        : actual.candidate;
      data.unshift({ candidate, actual: true });
    }
  }
  if (!actual && next && candidates.length && v) {
    const nextId = state.nodes.get(v.position.node)?.children[0];
    const nextData = nextId === undefined ? undefined : historyByNode.get(nextId);
    if (nextData && nextData.visits > 0) data.unshift({ actual: true, candidate: {
      point: next, order: -100, visits: nextData.visits, allocation_visits: nextData.visits,
      ...nextData.as_candidate,
      score_stdev: null, prior: 0, lcb: null, pv: [], pv_visits: [], pv_captures: [],
    } });
  }
  const shown = data.slice(0, 20), best = candidates[0];
  tableRowCount = shown.length;
  const body = host.querySelector("tbody")!;
  while (body.rows.length > shown.length) body.deleteRow(body.rows.length - 1);
  shown.forEach(({ candidate: c, actual: isActual }, k) => {
    const row = body.rows[k] ?? body.insertRow();
    row.dataset.order = String(c.order);
    row.dataset.point = c.point ? `${c.point.x},${c.point.y}` : "pass";
    row.className = `${isActual ? "actual " : ""}${selectedCandidate === c.order ? "selected" : ""}`;
    row.style.height = `${TABLE_HEADER + 3}px`;
    // Java's table: the change against the best move from the side to move, the value as shown
    // (总是显示黑胜率 / 目差显示, core `shown`).
    const difference = (value: number, baseline: number, shown: number) => `${value - baseline > 0 ? "↑" : "↓"}${(value - baseline).toFixed(1)}(${shown.toFixed(1)})`;
    const share = c.allocation_visits / Math.max(1, v?.analysis?.visits ?? 0) * 100;
    const values = [
      isActual ? c.order === -100 ? t("AnalysisFrame.actual") : `${c.order + 1}(${t("AnalysisFrame.actual")})` : c.point ? String(c.order + 1) : "Pass",
      isActual && c.order !== 0 && best ? difference(c.winrate, best.winrate, c.shown.winrate) : c.shown.winrate.toFixed(1),
      playouts(c.visits),
      share.toFixed(1),
    ];
    if (hasScore) values.push(c.score_lead === null || c.shown.score_lead === null ? "" : isActual && c.order !== 0 && best?.score_lead !== null && best?.score_lead !== undefined ? difference(c.score_lead, best.score_lead, c.shown.score_lead) : c.shown.score_lead.toFixed(1));
    while (row.cells.length > values.length) row.deleteCell(row.cells.length - 1);
    values.forEach((text, column) => {
      const cell = row.cells[column] ?? row.insertCell();
      if (cell.textContent !== text) cell.textContent = text;
      cell.title = text;
      cell.style.backgroundImage = column === 3 ? `linear-gradient(to right, #c0c0c0 ${Math.min(100, share)}%, transparent ${Math.min(100, share)}%)` : "";
    });
  });
}

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
    tableRowCount,
    tableRows: [...$("table").querySelectorAll("tbody tr")].map((row) => [...(row as HTMLTableRowElement).cells].map((cell) => cell.textContent ?? "")),
    graphPointCount,
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
    /** Visible menu, toolbar, bottom-bar and dialog controls with client-area centres, so
     *  automation clicks the real controls. */
    targets: [...document.querySelectorAll<HTMLElement>("[data-key], [data-name], #moveInput, .modal-box button")]
      .map((el) => ({ el, r: el.getBoundingClientRect() }))
      .filter(({ r }) => r.width > 0 && r.height > 0)
      .map(({ el, r }) => ({
        key: el.dataset.key ?? el.dataset.name ?? el.id ?? "",
        text: el.textContent ?? "",
        checked: el instanceof HTMLInputElement && el.type === "checkbox" ? el.checked : undefined,
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
  $("commentsTab").textContent = t("SidebarHeader.comments");
  $("problemsTab").textContent = t("SidebarHeader.problems");
  $("problemsTab").title = product("unavailable");
  bindPointer();
  bindCommentEditor($("commentText"));
  bindCaptureSurface(() => panels && { panels, width: $("main").clientWidth, height: $("main").clientHeight, background: backgroundUrl }, render);
  await Promise.all([
    loadImages(["board.png", "black0.png", "white0.png", "background.jpg", "heatmap.png", "smallUp.png", "smallDown.png", "smallLeft.png", "smallRight.png"]),
    document.fonts.load('12px "Lizzie Winrate"'),
  ]);
  subscribe(schedule);
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
  await onUnavailable(() => unavailable());
  await listen("acceptance-probe", () => invoke("acceptance_ui_report", { report: uiReport() }));
  await startAutoSave();
  window.addEventListener("resize", schedule);
  apply(first);
  for (const v of early) apply(v);
  early = null;
  render();
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
