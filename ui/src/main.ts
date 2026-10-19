// Workbench entry: Java LizzieFrame presentation state and incremental painters.
// Record and engine operations go through the Rust-owned command interface (app.ts).
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getVersion } from "@tauri-apps/api/app";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { apply, navigate, pasteRecord, run, state, subscribe, unavailable, windowTitle } from "./app";
import { buildBottomBar, layoutBottomBar } from "./chrome/bottombar";
import { buildMenubar, menuOpen } from "./chrome/menubar";
import { buildToolbar, renderToolbar } from "./chrome/toolbar";
import { onUnavailable, onView, snapshot } from "./contract/client";
import type { CandidateView, HistoryPoint, Navigation, WorkbenchView } from "./contract/generated";
import { dialogOpen, message } from "./dialogs";
import { isChinese, product, setLanguage, t } from "./i18n";
import { boardGeom, graphGeom, inFrameLayout, type Panels, type Rect } from "./workbench/layout";
import { handleEngineKey } from "./engine/keys";
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
  type BoardScene,
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
let presentationFor = "";
let candidateCount = 0;
let tableRowCount = 0;
let graphPointCount = 0;
let maxScoreLead = 15;
let graphDocument = -1;
let graphNodesSource = state.nodes;
let graphLineKey = "";
let graphLineByMove = new Map<number, number>();
let mainLine = new Set<number>();
let historyByNode = new Map<number, HistoryPoint>();
let analysisSignature = "";
let historySignature = "";
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

function hover(order: number | null) {
  const candidate = state.view?.analysis?.candidates.find((c) => c.order === order);
  const point = candidate?.point ? `${candidate.point.x},${candidate.point.y}` : null;
  if (hoveredCandidate === order && hoveredPoint === point) return;
  hoveredCandidate = order;
  hoveredPoint = point;
  schedule();
}

/** Cache line navigation only when the tree/position changes, not on engine frames. */
function projectAnalysis(v: WorkbenchView) {
  if (projectedView !== v) {
    projectedView = v;
    analysisSignature = JSON.stringify(v.analysis);
    historySignature = JSON.stringify(v.history);
    historyByNode = new Map(v.history.map((p) => [p.node, p]));
    if (graphDocument !== v.document.id) {
      graphDocument = v.document.id;
      maxScoreLead = 15;
    }
    for (const p of v.history) if (p.score_lead !== null) maxScoreLead = Math.max(maxScoreLead, Math.abs(p.score_lead));
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
    selectedCandidate = null;
    subCandidate = 0;
    subLength = null;
  }
  // Java keeps the hovered coordinate, even when streaming visits reorder the candidates.
  hoveredCandidate = hoveredPoint === null ? null : v.analysis?.candidates.find((c) => c.point && `${c.point.x},${c.point.y}` === hoveredPoint)?.order ?? null;
  subCandidate = Math.min(subCandidate, Math.max(0, (v.analysis?.candidates.length ?? 0) - 1));
}

function engineTitle(v: WorkbenchView): string {
  const base = windowTitle(version), foreground = v.engine.foreground, a = v.analysis;
  let prefix = "";
  if (a && a.visits > 0) {
    const black = a.to_play === "Black";
    prefix = `[${(black ? a.winrate : 100 - a.winrate).toFixed(1)}${a.score_lead === null ? "" : ` ${(black ? a.score_lead : -a.score_lead).toFixed(1)}`} ${playouts(a.visits)}] `;
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
  if (dialogOpen() || menuOpen() || textTarget(e.target)) return;
  if (handleEngineKey(e)) {
    e.preventDefault();
    return;
  }
  const ctrl = e.ctrlKey || e.metaKey;
  // Edits (Shift+Left/Right move a branch, Alt rotations, Ctrl+Home clear) are not browsing.
  const editModifiers =
    ((e.key === "ArrowLeft" || e.key === "ArrowRight") && e.shiftKey) ||
    (e.altKey && e.key.startsWith("Arrow") && ctrl) ||
    (e.key === "Home" && ctrl);
  const nav = editModifiers ? null : navigationFor(e);
  if (nav) {
    e.preventDefault();
    navigate(nav);
    return;
  }
  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (key === "O" && !ctrl && !e.altKey && !e.shiftKey) {
    e.preventDefault();
    run({ type: "ChooseAndOpenRecord" });
  } else if (key === "V" && ctrl && !e.altKey) {
    e.preventDefault();
    pasteRecord();
  } else if (key === "B" && !ctrl && !e.altKey) {
    e.preventDefault();
    navigate({ op: "MainTrunk" });
  } else if (key !== " " && (editModifiers || JAVA_BOUND.has(key))) {
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
      if ((ev.target as HTMLElement).closest("#comment, #table, #tree, #sub, #graph")) return;
      ev.preventDefault();
      if (ev.deltaY !== 0) navigate(ev.deltaY > 0 ? { op: "Forward", count: 1 } : { op: "Back", count: 1 });
    },
    { passive: false },
  );
  // Board clicks place stones in Java (document editing).
  $("board").addEventListener("mousedown", (ev) => {
    if (ev.button === 0 && boardPoint(ev)) flash(product("unavailable"));
  });
  $("board").addEventListener("mousemove", (ev) => {
    const v = state.view;
    if (!v || !panels) return;
    const geo = boardGeom(panels.board, v.document.size.width, v.document.size.height);
    const rect = $("main").getBoundingClientRect();
    const x = ev.clientX - rect.left - geo.x - geo.margin;
    const y = ev.clientY - rect.top - geo.y - geo.margin;
    const candidate = v.analysis?.candidates.find((c) => c.point && c.visits > 0 && Math.hypot(x - c.point.x * geo.square, y - c.point.y * geo.square) <= geo.stoneRadius + 1 &&
      (v.settings.engine.analysis.limitMaxSuggestion === 0 || c.order < v.settings.engine.analysis.limitMaxSuggestion || c.order === hoveredCandidate));
    hover(candidate?.order ?? null);
  });
  $("board").addEventListener("mouseleave", () => hover(null));
  $("table").addEventListener("mouseleave", () => hover(null));
  const sub = $("sub");
  sub.addEventListener("contextmenu", (ev) => ev.preventDefault());
  sub.addEventListener("mousedown", (ev) => {
    const count = state.view?.analysis?.candidates.length ?? 0;
    if (!count || (ev.button !== 0 && ev.button !== 2)) return;
    ev.preventDefault();
    subCandidate = ev.button === 0 ? (subCandidate + 1) % count : Math.max(0, subCandidate - 1);
    subLength = null;
    schedule();
  });
  sub.addEventListener("wheel", (ev) => {
    ev.preventDefault();
    const v = state.view, c = v?.analysis?.candidates[subCandidate];
    if (!v || !c || ev.deltaY === 0) return;
    const limit = v.settings.engine.analysis.limitBranchLength;
    const max = limit > 0 ? Math.min(limit, c.pv.length) : c.pv.length;
    subLength = subLength === null ? Math.min(1, max) : Math.max(1, Math.min(max, subLength + Math.sign(ev.deltaY)));
    schedule();
  }, { passive: false });
  sub.addEventListener("mouseleave", () => {
    subCandidate = 0;
    subLength = null;
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

function boardPoint(ev: MouseEvent): boolean {
  const v = state.view;
  if (!panels || !v) return false;
  const { width, height } = v.document.size;
  const geo = boardGeom(panels.board, width, height);
  const rect = $("main").getBoundingClientRect();
  const x = ev.clientX - rect.left - geo.x - geo.margin;
  const y = ev.clientY - rect.top - geo.y - geo.margin;
  const cx = Math.floor((x + geo.square / 2) / geo.square);
  const cy = Math.floor((y + geo.square / 2) / geo.square);
  return cx >= 0 && cy >= 0 && cx < width && cy < height;
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
  const docSig = `${v.document.id}:${pos.node}`;
  projectAnalysis(v);
  if (changed("bg", size)) installBackground(size, W, H, P.glass);

  const scene: BoardScene = {
    cols,
    rows,
    stones: pos.stones,
    lastAction: pos.last_action,
    toPlay: pos.to_play,
    marks: pos.marks,
    nextMoves: pos.next_moves,
    nextColor: pos.next_color,
    black: v.document.info.black,
    white: v.document.info.white,
    showNames: v.document.format !== null,
    analysis: v.analysis,
    hovered: hoveredCandidate,
    suggestionLimit: v.settings.engine.analysis.limitMaxSuggestion,
    branchLimit: v.settings.engine.analysis.limitBranchLength,
    subCandidate,
    subLength,
  };
  if (changed("board", `${size}:${docSig}:${analysisSignature}:${hoveredCandidate}:${scene.suggestionLimit}:${scene.branchLimit}`))
    candidateCount = paintBoard(prepare($("board"), P.board), boardGeom(P.board, cols, rows), P.board, scene);
  if (changed("sub", `${size}:${docSig}:${analysisSignature}:${subCandidate}:${subLength}:${scene.branchLimit}`))
    paintSubBoard(prepare($("sub"), P.sub), boardGeom(P.sub, cols, rows, 0.035), P.sub, scene);
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
  const current = historyByNode.get(pos.node) ?? null;
  const previous = historyByNode.get(state.nodes.get(pos.node)?.parent ?? -1) ?? null;
  if (changed("stat", `${size}:${docSig}:${JSON.stringify(current)}:${JSON.stringify(previous)}:${v.analysis?.score_stdev}`))
    paintStat(prepare($("stat"), P.stat), P.stat, current, previous, pos.move_number, v.analysis?.score_stdev ?? null);
  const lineNodes = Math.max(state.mainLineNodes, state.line.size);
  if (changed("graph", `${size}:${docSig}:${lineNodes}:${historySignature}`))
    graphPointCount = paintGraph(prepare($("graph"), P.graph), P.graph, lineNodes, { history: v.history, current: pos.node, line: state.line, mainLine, maxScore: maxScoreLead });
  const statusRect = { x: 0, y: P.ponderingY, w: P.board.x, h: H - P.ponderingY };
  const lines = statusLines();
  if (changed("status", `${size}:${lines.join("|")}`)) paintStatus(prepare($("status"), statusRect), statusRect, lines);
  if (treeScrollFor !== docSig) {
    treeScroll = null;
    treeScrollFor = docSig;
  }
  if (changed("tree", `${size}:${docSig}:${state.nodes.size}:${treeScroll?.top}:${treeScroll?.left}`)) {
    const painted = paintTree(prepare($("tree"), P.tree), P.tree, state.nodes, state.root, pos.node, state.line, treeScroll);
    treeHits = painted.hits;
    treeView = painted.viewport;
  }
  place($("comment"), P.comment);
  if (changed("comment", docSig)) $("commentText").textContent = pos.comment;
  place($("table"), P.table);
  if (changed("table", `${size}:${docSig}:${analysisSignature}:${historySignature}:${selectedCandidate}`)) renderTable();
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
        ? { ...actual.candidate, visits: nextData.visits, allocation_visits: nextData.visits, winrate: 100 - nextData.winrate, score_lead: nextData.score_lead === null ? null : -nextData.score_lead }
        : actual.candidate;
      data.unshift({ candidate, actual: true });
    }
  }
  if (!actual && next && candidates.length && v) {
    const nextId = state.nodes.get(v.position.node)?.children[0];
    const nextData = nextId === undefined ? undefined : historyByNode.get(nextId);
    if (nextData && nextData.visits > 0) data.unshift({ actual: true, candidate: {
      point: next, order: -100, visits: nextData.visits, allocation_visits: nextData.visits,
      winrate: 100 - nextData.winrate, score_lead: nextData.score_lead === null ? null : -nextData.score_lead,
      score_stdev: null, prior: 0, lcb: null, pv: [], pv_captures: [],
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
    const difference = (value: number, baseline: number) => `${value - baseline > 0 ? "↑" : "↓"}${(value - baseline).toFixed(1)}(${value.toFixed(1)})`;
    const share = c.allocation_visits / Math.max(1, v?.analysis?.visits ?? 0) * 100;
    const values = [
      isActual ? c.order === -100 ? t("AnalysisFrame.actual") : `${c.order + 1}(${t("AnalysisFrame.actual")})` : c.point ? String(c.order + 1) : "Pass",
      isActual && c.order !== 0 && best ? difference(c.winrate, best.winrate) : c.winrate.toFixed(1),
      playouts(c.visits),
      share.toFixed(1),
    ];
    if (hasScore) values.push(c.score_lead === null ? "" : isActual && c.order !== 0 && best?.score_lead !== null && best?.score_lead !== undefined ? difference(c.score_lead, best.score_lead) : c.score_lead.toFixed(1));
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
    candidateCount,
    hoveredCandidate,
    subBoardCandidateIndex: subCandidate,
    subBoardBranchLength: subLength,
    tableRowCount,
    graphPointCount,
    statusLines: statusLines(),
    candidates: state.view?.analysis?.candidates.map((c) => ({ order: c.order, point: c.point, visits: c.visits, pvLength: c.pv.length })) ?? [],
    /** Client-area geometry so automation points at real board / sub-board / graph / table spots. */
    geometry: panels && state.view && (() => {
      const { width, height } = state.view!.document.size;
      const m = $("main").getBoundingClientRect();
      const g = boardGeom(panels.board, width, height);
      const s = boardGeom(panels.sub, width, height, 0.035);
      const tb = $("table").getBoundingClientRect();
      return {
        board: { x: m.left + g.x + g.margin, y: m.top + g.y + g.margin, square: g.square },
        sub: { x: m.left + s.x + s.margin, y: m.top + s.y + s.margin, square: s.square },
        graph: { x: m.left + panels.graph.x, y: m.top + panels.graph.y, w: panels.graph.w, h: panels.graph.h },
        table: { x: tb.left, y: tb.top, w: tb.width, h: tb.height },
      };
    })(),
    comment: $("commentText").textContent,
    moveField: $<HTMLInputElement>("moveInput").value,
    bottomBar: [...document.querySelectorAll<HTMLElement>("#bottombar .bb")].filter((b) => b.style.display !== "none").map((b) => b.textContent),
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
    /** Visible menu, toolbar, bottom-bar and dialog controls with client-area centres, so
     *  automation clicks the real controls. */
    targets: [...document.querySelectorAll<HTMLElement>("[data-key], [data-name], #moveInput, .modal-box button")]
      .map((el) => ({ el, r: el.getBoundingClientRect() }))
      .filter(({ r }) => r.width > 0 && r.height > 0)
      .map(({ el, r }) => ({
        key: el.dataset.key ?? el.dataset.name ?? el.id ?? "",
        text: el.textContent ?? "",
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + r.height / 2),
      })),
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
  $("commentsTab").textContent = t("SidebarHeader.comments");
  $("problemsTab").textContent = t("SidebarHeader.problems");
  $("problemsTab").title = product("unavailable");
  bindPointer();
  await Promise.all([
    loadImages(["board.png", "black0.png", "white0.png", "background.jpg", "smallUp.png", "smallDown.png", "smallLeft.png", "smallRight.png"]),
    document.fonts.load('12px "Lizzie Winrate"'),
  ]);
  subscribe(schedule);
  await onUnavailable(() => unavailable());
  await listen("acceptance-probe", () => invoke("acceptance_ui_report", { report: uiReport() }));
  window.addEventListener("resize", schedule);
  apply(first);
  for (const v of early) apply(v);
  early = null;
  render();
  await getCurrentWindow().show();
  await getCurrentWindow().setFocus();
  schedule();
}

// A start-up failure must not leave the hidden window invisible forever.
boot().catch(async (e) => {
  console.error("start-up failed", e);
  await getCurrentWindow().show();
  message("LizzieYzy Tauri", e instanceof Error ? `${e}\n${e.stack ?? ""}` : String(e), "error");
});
