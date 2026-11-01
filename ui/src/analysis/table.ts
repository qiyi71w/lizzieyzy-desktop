// 选点列表 tables: the main window's SuggestionTableScrollPane (LizzieFrame.java:861-1041,
// 14665-15109) and the rows the 独立选点列表 shows (AnalysisFrame.java), with the interaction
// state both share: LizzieFrame.suggestionclick (the highlighted point), the shown variation
// (mouseOverCoordinate, through the board's hover) and each table's clickOrder / selectedorder.
// Values come from the core projections; nothing is converted by side here.
import { invoke } from "@tauri-apps/api/core";
import { state } from "../app";
import type { CandidateView, HistoryPoint, Point, ShownValues, WorkbenchView } from "../contract/generated";
import { t } from "../i18n";
import { frameCoordinate, playouts, themeFonts } from "../workbench/paint";
import { appearance, frameMetrics } from "../appearance/state";
import { customFont } from "../appearance/model";
import { bindColumnResize, layoutColumns, MIN_COLUMN, type FrameBar, type FrameCell, type FramePayload } from "./suggestionsFrame";

/** One row of a table model (`data2`), as Java's MoveData. */
interface TableRow {
  point: Point | null;
  /** Candidate rank; -100 for an actual move outside the candidates. */
  order: number;
  /** The actual move (isNextMove). */
  actual: boolean;
  visits: number;
  /** allocationVisits: edgeVisits when reported, else playouts. */
  allocation: number;
  /** Side to move, as reported; the actual row's differences use these. */
  winrate: number;
  score: number | null;
  /** The main table's values (总是显示黑胜率, 目差显示). */
  shown: ShownValues;
  /** The 独立选点列表's values (side to move, 盘面). */
  sideToMove: ShownValues;
  /** Unknown (`null`) shows "--", like Java's placeholder for the actual row's missing values. */
  lcb: number | null;
  prior: number | null;
  stdev: number | null;
  bestWinrate: number;
  bestScore: number | null;
}

/** The 独立选点列表 windows: the primary engine's (AnalysisFrame index 1) and, in 双引擎模式, the
 *  secondary engine's (index 2). */
export type FrameId = "frame" | "sub";
type Table = "main" | FrameId;

/** A variation a board shows, with its displayed length (hasSelectedVariation). */
export interface ShownVariation {
  key: string;
  steps: number;
  length: number;
}

/** What the main window provides: the boards' variation (mouseOverCoordinate) and repainting. */
export interface TablesHost {
  /** Shows the boards' variation at `point` (mouseOverCoordinate): a candidate's, or the actual
   *  move's with the next position's variation; null shows none. */
  showVariation(point: Point | null): void;
  /** The variations the boards show: the main (left) board's and, in 双引擎模式, the right
   *  board's (boardRenderer2). */
  variations(): { left: ShownVariation | null; right: ShownVariation | null };
  repaint(): void;
}

let host: TablesHost | null = null;
/** LizzieFrame.suggestionclick. */
let suggestionClick: Point | null = null;
/** clickOrder / selectedorder of each table (row indices, as Java keeps them). */
const selection: Record<Table, { click: number; selected: number }> = {
  main: { click: -1, selected: -1 },
  frame: { click: -1, selected: -1 },
  sub: { click: -1, selected: -1 },
};
/** Each window's sortnum (the header last released, -1 for the engine's order) and the rows and
 *  cells it last showed. */
const frames: Record<FrameId, { sort: number; rows: TableRow[]; cells: string[][] }> = {
  frame: { sort: -1, rows: [], cells: [] },
  sub: { sort: -1, rows: [], cells: [] },
};
/** The variation lengths the wheel set over a table (doBranch), each for its variation only. */
const branches = new Map<string, number>();
/** Bumped by every interaction, so the tables repaint. */
let revision = 0;
let mainRows: TableRow[] = [];
let mainCells: string[][] = [];

const samePoint = (a: Point | null, b: Point | null) => !!a && !!b && a.x === b.x && a.y === b.y;

function touched() {
  revision++;
  host?.repaint();
}

export function bindTables(h: TablesHost): void {
  host = h;
  void invoke<number[] | null>("main_list_columns").then((saved) => {
    if (saved?.length === 5) {
      mainPreferred = saved.map((w) => Math.max(MIN_COLUMN, w));
      mainStructure = "";
      touched();
    }
  });
}

/** Changes whenever a table's interaction state does. */
export function tablesRevision(): number {
  return revision;
}

/** The wheel-set displayed length of variation `key`, if any. */
export function tableBranchLength(key: string): number | null {
  return branches.get(key) ?? null;
}

/** A new position (Board.java:3656-3692): no clicked row, point or variation length remains. */
export function resetTables(): void {
  if (suggestionClick === null && branches.size === 0 && Object.values(selection).every((s) => s.click === -1 && s.selected === -1)) return;
  suggestionClick = null;
  branches.clear();
  for (const s of Object.values(selection)) {
    s.click = -1;
    s.selected = -1;
  }
  revision++;
}

/** clearSuggestionTablePreview (LizzieFrame.java:10207-10216). */
function clearMainPreview() {
  selection.main.click = -1;
  selection.main.selected = -1;
  suggestionClick = null;
  host?.showVariation(null);
}

/** The board's popup press with a clicked main-table row only clears it (openRightClickMenu,
 *  LizzieFrame.java:3079-3083). Returns whether it did. */
export function clearTableSelection(): boolean {
  if (selection.main.click === -1) return false;
  clearMainPreview();
  touched();
  return true;
}

/** The pointer over the board with a clicked main-table row (onMouseMoved, LizzieFrame.java:
 *  10074-10079): off the grid the row keeps its preview (false: leave the hover as it is); on the
 *  grid the preview ends. */
export function boardPointerMoved(inBoard: boolean): boolean {
  if (selection.main.click === -1) return true;
  if (!inBoard) return false;
  clearMainPreview();
  touched();
  return true;
}

/** A new AnalysisFrame starts unsorted and unselected; closing one (toggleBestMoves, also when
 *  a box rebuilds it) clears the highlighted point. */
export function resetFrame(id: FrameId, closing: boolean): void {
  frames[id].sort = -1;
  selection[id].click = -1;
  selection[id].selected = -1;
  if (closing) suggestionClick = null;
  touched();
}

// ------------------------------------------------------------------ rows

/** The first child when it is a placed move (hasRealMoveCoordinates), with its analysis from
 *  `history` (the secondary engine's for the 副引擎 list). */
function nextMove(v: WorkbenchView, history: HistoryPoint[]): { point: Point; analysis: HistoryPoint | null } | null {
  const id = state.nodes.get(v.position.node)?.children[0];
  const child = id === undefined ? undefined : state.nodes.get(id);
  const point = v.position.next_moves[0];
  if (!child || child.pass || (child.kind !== "Black" && child.kind !== "White") || !point) return null;
  return { point, analysis: history.find((p) => p.node === id) ?? null };
}

function candidateRow(c: CandidateView): TableRow {
  return {
    point: c.point,
    order: c.order,
    actual: false,
    visits: c.visits,
    allocation: c.allocation_visits,
    winrate: c.winrate,
    score: c.score_lead,
    shown: c.shown,
    sideToMove: c.side_to_move,
    lcb: c.lcb,
    prior: c.prior,
    stdev: c.score_stdev,
    bestWinrate: 0,
    bestScore: 0,
  };
}

/** The table model's rows: the candidates in the engine's order, the actual move inserted first
 *  (LizzieFrame.java:14743-14814, AnalysisFrame.java:783-930). The frame keeps lcb, policy and
 *  stdev and counts an outside move with no visits; the main table does not show them. The
 *  secondary window takes its candidates and the next position's result from the secondary
 *  engine (bestMoves2, winrate2). */
function buildRows(v: WorkbenchView, table: Table): TableRow[] {
  const frame = table !== "main";
  // The secondary window lists the secondary engine's candidates (bestMoves2).
  const analysis = table === "sub" ? v.secondary?.analysis ?? null : v.analysis;
  const rows = (analysis?.candidates ?? []).map(candidateRow);
  const next = frame && !v.settings.display.frameShowNext ? null : nextMove(v, table === "sub" ? v.secondary?.history ?? [] : v.history);
  if (!next || !rows.length) return rows;
  const best = rows[0];
  const actual = { actual: true, bestWinrate: best.winrate, bestScore: best.score };
  const fromNext = (h: HistoryPoint, order: number, visits: number, prior: number | null): TableRow => ({
    point: next.point,
    order,
    visits,
    // A new MoveData has no edgeVisits: its playouts are its allocation.
    allocation: visits,
    winrate: h.as_candidate.winrate,
    score: h.as_candidate.score_lead,
    shown: h.as_candidate.shown,
    sideToMove: h.as_candidate.side_to_move,
    // Java's -10000 (lcb, the outside move's policy): no value.
    lcb: null,
    prior,
    stdev: frame ? h.score_stdev : null,
    ...actual,
  });
  const move = rows.find((r) => samePoint(r.point, next.point));
  if (move) {
    if (move.order === 0) Object.assign(move, actual);
    else if (next.analysis && next.analysis.visits > move.visits)
      rows.unshift(fromNext(next.analysis, move.order, next.analysis.visits, frame ? move.prior : null));
    else
      rows.unshift({ ...move, ...actual, allocation: move.visits, lcb: frame ? move.lcb : null, stdev: frame ? move.stdev : null });
  } else if (next.analysis) {
    rows.unshift(fromNext(next.analysis, -100, frame ? 0 : next.analysis.visits, null));
  }
  return rows;
}

/** MoveData.getAllocationVisits over the model, inserted rows included. */
const totalAllocation = (rows: TableRow[]) => rows.reduce((s, r) => s + r.allocation, 0);

/** The actual row's change against the best candidate, then the value as the table shows it;
 *  without both scores only the value. */
function difference(value: number | null, best: number | null, shown: number): string {
  if (value === null || best === null) return shown.toFixed(1);
  const d = value - best;
  return `${d > 0 ? "↑" : "↓"}${d.toFixed(1)}(${shown.toFixed(1)})`;
}

function orderLabel(r: TableRow, frame: boolean): string {
  if (r.order === -100) return t("AnalysisFrame.actual");
  if (r.actual) return `${r.order + 1}(${t("AnalysisFrame.actual")})`;
  return !frame && !r.point ? "Pass" : String(r.order + 1);
}

/** The difference written before "(" in a cell, as the renderers parse it back. */
function parsedDifference(text: string | undefined): number {
  return text?.includes("(") ? Number.parseFloat(text.substring(1, text.indexOf("("))) : 0;
}

/** ColorTableCellRenderer: the actual row's level by the configured win-rate / score loss
 *  thresholds, stronger while it is the highlighted point (LizzieFrame.java:15025-15098). */
const LEVELS: [number, number, number, number, number][] = [
  [85, 25, 80, 120, 70],
  [208, 16, 19, 100, 50],
  [200, 140, 50, 100, 50],
  [180, 180, 0, 100, 50],
  [140, 202, 34, 100, 50],
  [0, 180, 0, 100, 60],
];

function rankBackground(v: WorkbenchView, diff: number, scoreDiff: number, selected: boolean): string {
  const w = v.settings.display.winLossThresholds, s = v.settings.display.scoreLossThresholds;
  let level = 5;
  for (let k = 4; k >= 0; k--)
    if (diff <= w[k] || scoreDiff <= s[k]) {
      level = 4 - k;
      break;
    }
  const [r, g, b, strong, light] = LEVELS[level];
  return `rgba(${r}, ${g}, ${b}, ${(selected ? strong : light) / 255})`;
}

/** Cell colours of one row: blue on the highlighted point's rows (red where the clicked row moved
 *  away from it), the rank colour on the actual row; the 占比 cell draws its bar only. */
function colourCells(v: WorkbenchView, table: Table, rows: TableRow[], cells: string[][], shareColumn: number, winrateColumn: number, scoreColumn: number | null): FrameCell[][] {
  const selectedAlpha = table === "main" ? 70 : 35;
  return rows.map((r, k) => {
    const isSelect = samePoint(r.point, suggestionClick);
    const isChanged = isSelect && selection[table].selected >= 0 && selection[table].selected !== k;
    let fg = "", bg = "";
    if (isSelect) {
      fg = "#0000ff";
      bg = `rgba(0, 0, 0, ${selectedAlpha / 255})`;
    }
    if (isChanged) fg = "#ff0000";
    if (r.actual) {
      const scoreDiff = scoreColumn === null ? 0 : parsedDifference(cells[k][scoreColumn]);
      bg = rankBackground(v, parsedDifference(cells[k][winrateColumn]), scoreDiff, isSelect);
    }
    return cells[k].map((text, column) =>
      column === shareColumn ? { text, fg: "", bg: "", bar: Number.parseFloat(text) } : { text, fg, bg, bar: null });
  });
}

// ------------------------------------------------------------------ interaction

/** handleTableClick (LizzieFrame.java:14665-14692, AnalysisFrame.java:719-745): a row shows its
 *  variation and highlights its point; clicking the clicked point again clears both. */
function leftClick(table: Table, row: number) {
  const r = (table === "main" ? mainRows : frames[table].rows)[row];
  if (!r) return;
  // boardRenderer.startNormalBoard: the whole variation again.
  branches.clear();
  if (!r.point) return touched();
  const s = selection[table];
  if (s.click !== -1 && s.selected >= 0 && samePoint(r.point, suggestionClick)) {
    if (table === "main") clearMainPreview();
    else {
      suggestionClick = null;
      host?.showVariation(null);
      s.selected = -1;
      s.click = -1;
    }
  } else {
    s.click = row;
    s.selected = row;
    suggestionClick = r.point;
    host?.showVariation(r.point);
  }
  touched();
}

/** handleTableRightClick (LizzieFrame.java:14694-14707, AnalysisFrame.java:747-762): only the
 *  point is highlighted, no variation; again on the same row clears it. Java draws nothing on the
 *  board for it (LizzieFrame.isMouseOversub has no caller). */
function rightClick(table: Table, row: number) {
  const r = (table === "main" ? mainRows : frames[table].rows)[row];
  if (!r?.point) return;
  const s = selection[table];
  if (s.selected !== row) {
    suggestionClick = r.point;
    host?.showVariation(null);
    s.selected = row;
  } else {
    suggestionClick = null;
    s.selected = -1;
  }
  touched();
}

/** doBranch (LizzieFrame.java:12036-12073) for a table's clicked row, on both boards at once:
 *  forward from the left board's whole variation shows 2 moves on each board, then one more on
 *  each with room; back, the left board shows all but the last, then one less, and the right board
 *  goes from its whole variation to its replay length (Java's getReplayBranch), then one less. */
function wheel(table: Table, down: boolean): boolean {
  if (selection[table].click === -1) return false;
  const shown = host?.variations();
  const left = shown?.left ?? null;
  const right = shown?.right ?? null;
  if (!left && !right) return true;
  const leftLength = left ? tableBranchLength(left.key) : null;
  const rightLength = right ? tableBranchLength(right.key) : null;
  if (down) {
    if (leftLength === null) {
      if (left) branches.set(left.key, 2);
      if (right) branches.set(right.key, 2);
    } else {
      if (left && left.steps > leftLength) branches.set(left.key, leftLength + 1);
      if (right && rightLength !== null && right.steps > rightLength) branches.set(right.key, rightLength + 1);
    }
  } else {
    if (left && leftLength === null) branches.set(left.key, left.steps - 1);
    else if (left && leftLength !== null && leftLength > 1) branches.set(left.key, leftLength - 1);
    if (right && rightLength === null) branches.set(right.key, right.steps);
    else if (right && rightLength !== null && rightLength > 1) branches.set(right.key, rightLength - 1);
  }
  touched();
  return true;
}

/** The workbench wheel (Input.mouseWheelMoved, Input.java:873-893): with a main-table row
 *  clicked and a variation shown (hasSelectedVariation) it steps the variation instead of
 *  browsing. Returns whether it did. */
export function boardWheel(down: boolean): boolean {
  const shown = host?.variations();
  return selection.main.click !== -1 && !!(shown?.left || shown?.right) && wheel("main", down);
}

/** AnalysisFrame mouseMoved with 鼠标悬停 and no clicked row: the row under the pointer shows its
 *  variation; the empty area clears it (AnalysisFrame.java:319-363). */
export function frameHover(v: WorkbenchView, id: FrameId, row: number): void {
  const s = selection[id];
  if (!v.settings.display.frameMouseMove || s.click !== -1) return;
  const r = frames[id].rows[row];
  if (!r) {
    if (suggestionClick !== null) {
      suggestionClick = null;
      host?.showVariation(null);
      s.selected = -1;
      s.click = -1;
      touched();
    }
    return;
  }
  if (!r.point || (s.selected >= 0 && samePoint(r.point, suggestionClick))) return;
  s.selected = row;
  suggestionClick = r.point;
  host?.showVariation(r.point);
  touched();
}

/** mouseExited (AnalysisFrame.java:384-395). */
export function frameLeave(v: WorkbenchView, id: FrameId): void {
  const s = selection[id];
  if (!v.settings.display.frameMouseMove || s.click !== -1 || suggestionClick === null) return;
  suggestionClick = null;
  host?.showVariation(null);
  s.selected = -1;
  touched();
}

export function frameClick(id: FrameId, row: number, right: boolean): void {
  if (right) rightClick(id, row);
  else leftClick(id, row);
}

export function frameWheel(id: FrameId, down: boolean): void {
  wheel(id, down);
}

/** header.mouseReleased. */
export function frameSortBy(id: FrameId, column: number): void {
  frames[id].sort = column;
  touched();
}

// ------------------------------------------------------------------ main window table

const TABLE_HEADER = navigator.userAgent.includes("Windows") ? 19 : 17;

/** refreshSuggestionTableStyle (LizzieFrame.java:2697-2721): the list takes
 *  workspaceFont(PLAIN, frameFontSize) — the theme's UI font when one is named — and rows of
 *  max(Config.menuHeight - 4, font height + 8) (SuggestionTableScrollPane.rowHeightFor); the header
 *  is the font height plus its insets. The 12 px heights are the measured ones above; other sizes
 *  add the font's own height difference, and the row never drops below menuHeight - 4. */
function mainTableStyle(table: HTMLTableElement): { row: number; header: number } {
  const { frameFontSize, menuHeight } = frameMetrics();
  const a = appearance();
  table.style.fontFamily = a && customFont(a.theme.uiFontName) ? themeFonts().ui : "";
  table.style.fontSize = `${frameFontSize}px`;
  const family = getComputedStyle(table).fontFamily;
  const c = document.createElement("canvas").getContext("2d")!;
  const height = (size: number) => {
    c.font = `${size}px ${family}`;
    const m = c.measureText("Hg");
    return Math.ceil(m.fontBoundingBoxAscent + m.fontBoundingBoxDescent);
  };
  const grow = frameFontSize === 12 ? 0 : height(frameFontSize) - height(12);
  return { row: Math.max(menuHeight - 4, TABLE_HEADER + 3 + grow), header: TABLE_HEADER + grow };
}
let mainRowHeight = TABLE_HEADER + 3;
/** setPreferredWidth of 序号, 胜率, 计算量, 占比, 目数 (LizzieFrame.java:907-912); 序号's 10 is
 *  raised to TableColumn's minimum. */
let mainPreferred = [15, 30, 30, 45, 30];
let mainWidths: number[] = [];
let mainStructure = "";

/** The main 选点列表 (getRowCount / getValueAt, LizzieFrame.java:14725-14916): at most 20 rows;
 *  the coordinate column stays hidden and the 目数 column shows with score data only. */
export function renderMainTable(host: HTMLElement, v: WorkbenchView): void {
  const hasScore = !!v.analysis?.candidates.some((c) => c.score_lead !== null);
  const columns = hasScore ? 5 : 4;
  const structure = `${host.clientWidth}:${columns}:${t("AnalysisFrame.column1")}:${appearance()?.theme.uiFontName}`;
  if (mainStructure !== structure) {
    mainStructure = structure;
    const heads = [t("AnalysisFrame.column1"), t("LizzieFrame.listColumn2"), t("AnalysisFrame.column5"), t("LizzieFrame.listColumn4"), t("AnalysisFrame.column8")].slice(0, columns);
    const table = document.createElement("table");
    const colgroup = document.createElement("colgroup");
    const thead = document.createElement("thead"), body = document.createElement("tbody");
    const headRow = thead.insertRow();
    for (const h of heads) {
      const th = document.createElement("th");
      th.textContent = h;
      headRow.append(th);
      colgroup.append(document.createElement("col"));
    }
    const apply = (widths: number[]) => {
      mainWidths = widths;
      table.style.width = `${widths.reduce((s, w) => s + w, 0)}px`;
      [...colgroup.children].forEach((col, k) => ((col as HTMLElement).style.width = `${widths[k]}px`));
    };
    // Java writes the widths only while the score column shows (Config.persist).
    const report = () => void invoke("set_main_list_columns", { columns: hasScore ? mainWidths : null });
    apply(layoutColumns(mainPreferred.slice(0, columns), host.clientWidth));
    report();
    bindColumnResize(headRow, () => mainWidths, apply, (widths) => {
      // A hidden 目数 column keeps the width it had (listTableColum5Width).
      mainPreferred = hasScore ? widths : [...widths, mainPreferred[4]];
      apply(widths);
      report();
    });
    const rowAt = (ev: MouseEvent) => (ev.target as HTMLElement).closest<HTMLTableRowElement>("tbody tr")?.sectionRowIndex ?? -1;
    body.addEventListener("click", (ev) => {
      const row = rowAt(ev);
      if (row >= 0) leftClick("main", row);
    });
    body.addEventListener("auxclick", (ev) => {
      const row = rowAt(ev);
      if (row >= 0 && ev.button === 1) leftClick("main", row);
    });
    body.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      const row = rowAt(ev);
      if (row >= 0) rightClick("main", row);
    });
    // With a clicked row the wheel steps its variation; otherwise the list scrolls.
    table.addEventListener("wheel", (ev) => {
      if (ev.deltaY !== 0 && wheel("main", ev.deltaY > 0)) ev.preventDefault();
    }, { passive: false });
    table.append(colgroup, thead, body);
    host.replaceChildren(table);
    // Measured once attached, so the stylesheet's font family applies.
    const style = mainTableStyle(table);
    mainRowHeight = style.row;
    headRow.style.height = `${style.header}px`;
  }
  const all = buildRows(v, "main");
  const total = totalAllocation(all);
  mainRows = all.slice(0, 20);
  mainCells = mainRows.map((r) => {
    const cells = [
      orderLabel(r, false),
      r.actual && r.order !== 0 ? difference(r.winrate, r.bestWinrate, r.shown.winrate) : r.shown.winrate.toFixed(1),
      playouts(r.visits),
      (total > 0 ? (r.allocation / total) * 100 : 0).toFixed(1),
    ];
    if (hasScore) {
      const shown = r.shown.score_lead;
      cells.push(shown === null ? "--" : r.actual && r.order !== 0 ? difference(r.score, r.bestScore, shown) : shown.toFixed(1));
    }
    return cells;
  });
  const coloured = colourCells(v, "main", mainRows, mainCells, 3, 1, hasScore ? 4 : null);
  const body = host.querySelector("tbody")!;
  while (body.rows.length > coloured.length) body.deleteRow(body.rows.length - 1);
  coloured.forEach((cells, k) => {
    const row = body.rows[k] ?? body.insertRow();
    const r = mainRows[k];
    // The hidden 坐标 column.
    row.dataset.point = r.point ? `${r.point.x},${r.point.y}` : "pass";
    row.style.height = `${mainRowHeight}px`;
    while (row.cells.length > cells.length) row.deleteCell(row.cells.length - 1);
    cells.forEach((c, column) => {
      const cell = row.cells[column] ?? row.insertCell();
      if (cell.textContent !== c.text) cell.textContent = c.text;
      // The actual row's cells carry their text as a tooltip.
      cell.title = r.actual ? c.text : "";
      cell.style.color = c.fg;
      cell.style.background = c.bar === null
        ? c.bg
        : `linear-gradient(to right, rgb(192, 192, 192) ${Math.min(100, c.bar)}%, transparent ${Math.min(100, c.bar)}%)`;
    });
  });
}

/** Everything the main table draws from besides the view's analysis and history. */
export function mainTableSignature(v: WorkbenchView): string {
  const d = v.settings.display;
  return `${revision}:${d.winLossThresholds.join(",")}:${d.scoreLossThresholds.join(",")}`;
}

// ------------------------------------------------------------------ 独立选点列表 rows

/** Default column widths (AnalysisFrame.java:249-259). */
export const FRAME_WIDTHS = [35, 20, 40, 70, 35, 50, 35, 70, 35];

/** getColumnCount (AnalysisFrame.java:769-781): 9 with a KataGo engine or when a main-line node
 *  holds score data (Board.isContainsKataData / isContainsKataData2, Board.java:5854-5870: from
 *  the start, the line's last node not checked), each window from its own engine. Java's Sai
 *  engine also gives 9; the core does not report Sai. */
export function frameNine(v: WorkbenchView, id: FrameId): boolean {
  if (id === "sub" ? v.engine.secondary?.katago : v.engine.foreground?.katago) return true;
  const history = id === "sub" ? v.secondary?.history ?? [] : v.history;
  const scored = new Set(history.filter((p) => p.score_lead !== null).map((p) => p.node));
  for (let id: number | undefined = state.root; id !== undefined; ) {
    const next: number | undefined = state.nodes.get(id)?.children[0];
    if (next === undefined) break;
    if (scored.has(id)) return true;
    id = next;
  }
  return false;
}

/** Collections.sort by sortnum, descending and stable (AnalysisFrame.java:948-989); a missing
 *  value sorts last, as Java's -10000 placeholder does. */
const SORT_KEYS: Record<number, (r: TableRow) => number | null> = {
  2: (r) => r.lcb,
  3: (r) => r.winrate,
  4: (r) => r.visits,
  5: (r) => r.visits,
  6: (r) => r.prior,
  7: (r) => r.score,
  8: (r) => r.stdev,
};

/** A window's contents for its layout (`nine` columns), `widths` when the layout is new. */
export function framePayload(v: WorkbenchView, id: FrameId, nine: boolean, layout: number, widths: number[] | null): FramePayload {
  const d = v.settings.display;
  const rows = buildRows(v, id);
  const key = SORT_KEYS[frames[id].sort];
  if (key) rows.sort((a, b) => (key(b) ?? -Infinity) - (key(a) ?? -Infinity) || 0);
  frames[id].rows = rows;
  const total = totalAllocation(rows);
  const { width, height } = v.document.size;
  const cells = (frames[id].cells = rows.map((r) => {
    const cells = [
      orderLabel(r, true),
      frameCoordinate(r.point, d.coordinateStyle, width, height),
      r.lcb === null ? "--" : r.lcb.toFixed(1),
      r.actual && r.order !== 0 ? difference(r.winrate, r.bestWinrate, r.sideToMove.winrate) : r.sideToMove.winrate.toFixed(1),
      r.order === -100 ? t("AnalysisFrame.exclude") : playouts(r.visits),
      (total > 0 ? (r.allocation / total) * 100 : 0).toFixed(1),
      r.prior === null ? "--" : r.prior.toFixed(2),
    ];
    if (nine) {
      const shown = r.sideToMove.score_lead;
      cells.push(shown === null ? "--" : r.actual && r.order !== 0 ? difference(r.score, r.bestScore, shown) : shown.toFixed(1), r.stdev === null ? "--" : r.stdev.toFixed(1));
    }
    return cells;
  }));
  const headers = [
    t("AnalysisFrame.column1"), t("AnalysisFrame.column2"), t("AnalysisFrame.column3"), t("AnalysisFrame.column4"),
    t("AnalysisFrame.column5"), t("AnalysisFrame.column6"), t("AnalysisFrame.column7"),
  ];
  if (nine) headers.push(t("AnalysisFrame.column8"), t("AnalysisFrame.column9"));
  // paintBottomPanel (AnalysisFrame.java:485-555): the candidates themselves, no actual row.
  const analysis = id === "sub" ? v.secondary?.analysis ?? null : v.analysis;
  const best = analysis?.candidates ?? [];
  let stats: string | null = null;
  const bars: FrameBar[] = [];
  if (best.length) {
    const allocation = best.reduce((s, c) => s + c.allocation_visits, 0);
    const maxAllocation = Math.max(...best.map((c) => c.allocation_visits));
    const maxVisits = Math.max(...best.map((c) => c.visits));
    let spread = 0;
    for (let i = 0; i < 11; i++) {
      const gap = maxAllocation - (i < best.length ? best[i].allocation_visits : 0);
      spread += gap * gap;
    }
    const concentration = Math.sqrt(allocation > 0 ? spread / 10 / allocation / allocation : 0) * 100;
    stats = `${t("AnalysisFrame.totalVisits")}${playouts(analysis!.visits)} ${t("AnalysisFrame.maxVisits")}${playouts(maxVisits)} ${t("AnalysisFrame.concentration")}${concentration.toFixed(2)}%`;
    for (const c of best)
      bars.push({
        ratio: maxAllocation > 0 ? c.allocation_visits / maxAllocation : 0,
        text: `${(allocation > 0 ? (c.allocation_visits / allocation) * 100 : 0).toFixed(2)}%`,
      });
  }
  return {
    headers,
    widths,
    layout,
    rows: colourCells(v, id, rows, cells, 5, 3, nine ? 7 : null),
    list: d.bestMovesList,
    graph: d.bestMovesGraph,
    boxes: [
      { key: "list", label: t("AnalysisFrame.checkList"), checked: d.bestMovesList },
      { key: "graph", label: t("AnalysisFrame.checkGraph"), checked: d.bestMovesGraph },
      { key: "showNext", label: t("AnalysisFrame.checkShowNext"), checked: d.frameShowNext },
      { key: "mouseMove", label: t("AnalysisFrame.checkUseMouseMove"), checked: d.frameMouseMove },
    ],
    stats,
    bars,
    uiFont: themeFonts().ui,
    menuHeight: frameMetrics().menuHeight,
    frameFontSize: frameMetrics().frameFontSize,
  };
}

/** What the acceptance probe reports of the tables. */
export function tablesReport() {
  return {
    suggestionClick,
    branchLength: branches.values().next().value ?? null,
    main: { rows: mainCells, selectedRow: selection.main.selected, clickedRow: selection.main.click, columns: mainWidths },
    frame: { rows: frames.frame.cells, selectedRow: selection.frame.selected, clickedRow: selection.frame.click, sort: frames.frame.sort },
    sub: { rows: frames.sub.cells, selectedRow: selection.sub.selected, clickedRow: selection.sub.click, sort: frames.sub.sort },
  };
}
