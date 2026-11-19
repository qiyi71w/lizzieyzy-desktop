// 超级鹰眼 window (MoveListFrame.java): the top pages (统计信息 / 简易列表 / 详细列表), the filter bar
// and the chart panel below. It draws what the main window relays (model.ts) and reports input
// back; the core computes rows and statistics, this window only formats, sorts (sortnum,
// issorted, isOriginOrder) and lays them out.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { appMenuKey } from "../appMenuKey";
import type { Command, DisplaySettings, HawkeyePeriod, HawkeyeSettings, MoveEvaluation } from "../contract/generated";
import { setLanguage, t } from "../i18n";
import { bindColumnResize, layoutColumns, namedFace } from "../analysis/suggestionsFrame";
import { icon } from "../design/icons";
import { frameCoordinate, playouts } from "../workbench/paint";
import { mountCharts, type ChartPanel } from "./charts";
import { dialogTargets, openDialog, openSetThreshold, openStatisticsThreshold, openMatchAiHelp, refreshDialogs } from "./dialogs";
import {
  HAWKEYE_DATA_EVENT,
  HAWKEYE_INPUT_EVENT,
  HAWKEYE_SUB_INPUT_EVENT,
  HAWKEYE_SUB_LABEL,
  fixed,
  settingsCommand,
  type HawkeyeInput,
  type HawkeyePayload,
  type HawkeyeReport,
} from "./model";
import { mountStatistics } from "./statistics";
import "../design/window";
import "../design/dialog.css";
import "../design/stage.css";
import "./hawkeye.css";

/** The 详细列表's preferred widths (MoveListFrame.java:1272-1280); later columns keep JTable's 75. */
const DETAIL_WIDTHS = [52, 50, 57, 72, 77, 70, 70, 70, 70, 75, 75, 75];

/** Sort state shared by the three lists, as Java keeps it on the frame. */
interface Sort {
  column: number;
  sorted: boolean;
  origin: boolean;
}

/** One rendered row: the move, its cells and the move's evaluation. */
interface Row {
  e: MoveEvaluation;
  cells: string[];
  pinned: boolean;
}

/** `(diff > 0 ? "+" : "-") + %.2f|diff| + arrow` (MoveListFrame getValueAt). */
function diffText(v: number): string {
  return `${v > 0 ? "+" : "-"}${fixed(Math.abs(v), 2)}${v > 0 ? "↑" : "↓"}`;
}

/** ColorTableCellRenderer: the class of a 胜率波动 / 目数波动 cell from its shown number, as the
 *  colour hawkeye.css gives it. */
function diffColor(text: string, gain: number, thresholds: number[]): string {
  const v = Number.parseFloat(text.substring(0, text.length - 1));
  if (v >= gain) return "var(--hk-diff-gain)";
  if (v <= thresholds[4]) return "var(--hk-diff-5)";
  if (v <= thresholds[3]) return "var(--hk-diff-4)";
  if (v <= thresholds[2]) return "var(--hk-diff-3)";
  if (v <= thresholds[1]) return "var(--hk-diff-2)";
  if (v <= thresholds[0]) return "var(--hk-diff-1)";
  return "var(--hk-diff-even)";
}

/** The raw change Java keeps in `NodeInfo.diffWinrate` / `scoreMeanDiff` (0 before it is known);
 *  the sort keys only. */
const winDiff = (e: MoveEvaluation) => e.winrate_change ?? 0;
const scoreDiff = (e: MoveEvaluation) => e.score_change ?? 0;

/** A value the analysis does not have is an empty cell, as MoveListFrame leaves 一选胜率 empty
 *  without the previous position's analysis (java:7589-7593); never drawn as a measured 0. */
function known(v: number | null, text: (v: number) => string): string {
  return v === null ? "" : text(v);
}
const two = (v: number) => fixed(v, 2);

/** The 详细列表 comparator (MoveListFrame.java:7267-7484): a key per column, larger first unless
 *  sorted; the change columns go through |Δ| ↓, Δ ↓, Δ ↑. Java's ascending 一选目数 compared
 *  `scoreLead - scoreLead` (always 0); here it sorts by the 一选目数 like the descending order. */
function detailKey(column: number, katago: boolean, sort: Sort): ((e: MoveEvaluation) => number) | null {
  const map: Record<number, (e: MoveEvaluation) => number> = katago
    ? {
        1: (e) => -e.move_number,
        3: winDiff,
        4: scoreDiff,
        5: (e) => e.winrate ?? 0,
        6: (e) => e.best_winrate,
        // `scoreLead` is the opponent's score after the move.
        7: (e) => -(e.score ?? 0),
        8: (e) => -(e.score ?? 0) - scoreDiff(e),
        9: (e) => e.visits_before,
        10: (e) => e.visits_after,
        11: (e) => e.share,
      }
    : {
        1: (e) => -e.move_number,
        3: winDiff,
        4: (e) => e.winrate ?? 0,
        5: (e) => e.best_winrate,
        6: (e) => e.visits_before,
        7: (e) => e.visits_after,
        8: (e) => e.share,
      };
  const key = map[column];
  if (!key) return null;
  if ((column === 3 || (katago && column === 4)) && sort.origin) return (e) => Math.abs(key(e));
  return key;
}

/** The 简易列表 comparator (7791-7895): 手数 ascending first; changes three-state; 吻合度. */
function minKey(column: number, katago: boolean, sort: Sort): ((e: MoveEvaluation) => number) | null {
  const change = column === 2 ? winDiff : katago && column === 3 ? scoreDiff : null;
  if (change) return sort.origin ? (e) => Math.abs(change(e)) : change;
  if (column === 0) return (e) => -e.move_number;
  if ((katago && column === 4) || (!katago && column === 3)) return (e) => e.share;
  return null;
}

function sorted(moves: MoveEvaluation[], key: ((e: MoveEvaluation) => number) | null, sort: Sort, colour: boolean): MoveEvaluation[] {
  // Java collects from the line end back to the root; equal keys keep that order.
  const list = moves.slice().reverse();
  if (colour) {
    const black = (e: MoveEvaluation) => (e.mover === "Black" ? 0 : 1);
    return list.sort((a, b) => (sort.sorted ? black(b) - black(a) : black(a) - black(b)));
  }
  if (!key) return list;
  return list.sort((a, b) => (sort.sorted ? key(a) - key(b) : key(b) - key(a)));
}

function detailCells(e: MoveEvaluation, p: HawkeyePayload): string[] {
  const coord = frameCoordinate(e.point, p.coordinateStyle, p.width, p.height);
  const colour = e.mover === "Black" ? t("Movelistframe.black") : t("Movelistframe.white");
  const winrate = known(e.winrate, two);
  const best = fixed(e.best_winrate, 2);
  if (p.pane.katago) {
    return [colour, String(e.move_number), coord, known(e.winrate_change, diffText), known(e.score_change, diffText), winrate, best,
      known(e.score, two), known(e.best_score, two), playouts(e.visits_before), playouts(e.visits_after), fixed(e.share * 100, 1)];
  }
  return [colour, String(e.move_number), coord, known(e.winrate_change, diffText), winrate, best,
    playouts(e.visits_before), playouts(e.visits_after), fixed(e.share * 100, 1)];
}

function minCells(e: MoveEvaluation, p: HawkeyePayload): string[] {
  const coord = frameCoordinate(e.point, p.coordinateStyle, p.width, p.height);
  const head = [String(e.move_number), coord, known(e.winrate_change, diffText)];
  return p.pane.katago ? [...head, known(e.score_change, diffText), fixed(e.share * 100, 1)] : [...head, fixed(e.share * 100, 1)];
}

function detailHeaders(katago: boolean): string[] {
  return katago
    ? [t("Movelistframe.tableColumnColor"), t("Movelistframe.tableColumnMoveNum"), t("Movelistframe.minTableColumnCoords"),
        t("Movelistframe.minTableColumnWinDiff"), t("Movelistframe.minTableColumnScoreDiff"), t("Movelistframe.tableColumnThisWin"),
        t("Movelistframe.tableColumnBestWin"), t("Movelistframe.tableColumnThisScore"), t("Movelistframe.tableColumnBestScore"),
        t("Movelistframe.tableColumnPlayouts"), t("Movelistframe.tableColumnNextPlayouts"), t("Movelistframe.minTableColumnAiScore")]
    : [t("Movelistframe.tableColumnColor"), t("Movelistframe.tableColumnMoveNum"), t("Movelistframe.minTableColumnCoords"),
        t("Movelistframe.minTableColumnWinDiff"), t("Movelistframe.tableColumnThisWin"), t("Movelistframe.tableColumnBestWin"),
        t("Movelistframe.tableColumnPlayouts"), t("Movelistframe.tableColumnNextPlayouts"), t("Movelistframe.minTableColumnAiScore")];
}

function minHeaders(black: boolean, katago: boolean): string[] {
  const head = [black ? t("Movelistframe.minTableColumnBlack") : t("Movelistframe.minTableColumnWhite"),
    t("Movelistframe.minTableColumnCoords"), t("Movelistframe.minTableColumnWinDiff")];
  return katago ? [...head, t("Movelistframe.minTableColumnScoreDiff"), t("Movelistframe.minTableColumnAiScore")]
    : [...head, t("Movelistframe.minTableColumnAiScore")];
}

/** How one cell is drawn: its text and ground colours (empty for the table's own) and whether it
 *  holds the figure the row is listed for. */
interface CellLook {
  fg: string;
  bg: string;
  key: boolean;
}

/** A JTable in a JScrollPane: header, rows, click and right-click. `categorical` names the columns
 *  that hold words (colour, coordinate): they align left, the numbers right. */
class ListTable {
  readonly scroll = document.createElement("div");
  private readonly table = document.createElement("table");
  private readonly colgroup = document.createElement("colgroup");
  private readonly head: HTMLTableRowElement;
  private readonly body = document.createElement("tbody");
  private headers: string[] = [];
  rows: Row[] = [];
  widths: number[] = [];
  preferred: number[] | null = null;

  constructor(key: string, private readonly categorical: number[], onSort: (column: number) => void, onRow: (row: Row) => void, onMenu: (x: number, y: number) => void, onColumns?: (w: number[]) => void) {
    this.scroll.className = "hk-scroll";
    this.scroll.dataset.key = key;
    const thead = document.createElement("thead");
    this.head = thead.insertRow();
    this.table.append(this.colgroup, thead, this.body);
    this.scroll.append(this.table);
    if (onColumns) {
      bindColumnResize(this.head, () => this.widths, (w) => this.apply(w), (w) => {
        this.preferred = w;
        this.apply(w);
        onColumns(w);
      });
    }
    // header.mouseReleased: sortnum = the column under the pointer.
    this.head.addEventListener("mouseup", (ev) => {
      if (ev.button !== 0) return;
      const cell = (ev.target as HTMLElement).closest("th");
      if (cell) onSort((cell as HTMLTableCellElement).cellIndex);
    });
    this.body.addEventListener("click", (ev) => {
      const tr = (ev.target as HTMLElement).closest("tr");
      if (tr) onRow(this.rows[tr.sectionRowIndex]);
    });
    this.scroll.addEventListener("contextmenu", (ev) => {
      ev.preventDefault();
      if ((ev.target as HTMLElement).closest("tbody, thead") || ev.target === this.scroll) onMenu(ev.clientX, ev.clientY);
    });
  }

  private apply(w: number[]): void {
    this.widths = w;
    this.table.style.width = `${w.reduce((s, x) => s + x, 0)}px`;
    [...this.colgroup.children].forEach((col, k) => ((col as HTMLElement).style.width = `${w[k] ?? 0}px`));
  }

  /** JTable's fit of the preferred widths into the viewport, or equal shares without them. */
  layout(): void {
    const width = this.scroll.clientWidth;
    const n = this.headers.length;
    if (!n || width <= 0) return;
    this.apply(layoutColumns(this.preferred ?? Array.from({ length: n }, () => 75), width));
  }

  setHeaders(headers: string[], preferred: number[] | null): void {
    if (headers.join("\u0000") === this.headers.join("\u0000") && preferred === this.preferred) return;
    this.headers = headers;
    this.preferred = preferred;
    this.colgroup.replaceChildren(...headers.map(() => document.createElement("col")));
    this.head.replaceChildren(...headers.map((text, column) => {
      const th = document.createElement("th");
      th.textContent = text;
      th.title = text;
      th.classList.toggle("cat", this.categorical.includes(column));
      return th;
    }));
    this.layout();
  }

  setRows(rows: Row[], look: (row: Row, column: number) => CellLook): void {
    this.rows = rows;
    while (this.body.rows.length > rows.length) this.body.deleteRow(this.body.rows.length - 1);
    rows.forEach((r, k) => {
      const tr = this.body.rows[k] ?? this.body.insertRow();
      while (tr.cells.length > r.cells.length) tr.deleteCell(tr.cells.length - 1);
      r.cells.forEach((text, column) => {
        const td = tr.cells[column] ?? tr.insertCell();
        if (td.textContent !== text) td.textContent = text;
        const c = look(r, column);
        td.style.color = c.fg;
        td.style.backgroundColor = c.bg;
        td.classList.toggle("cat", this.categorical.includes(column));
        td.classList.toggle("key", c.key);
      });
    });
  }

  /** Utils.exportTable: the header row, then the cells as shown. */
  exportRows(): string[][] {
    return [this.headers, ...this.rows.map((r) => r.cells)];
  }

  /** Cell texts (acceptance report). */
  texts(): string[][] {
    return this.rows.map((r) => r.cells);
  }
}

export function bootHawkeye(): void {
  const self = getCurrentWebviewWindow();
  const inputEvent = self.label === HAWKEYE_SUB_LABEL ? HAWKEYE_SUB_INPUT_EVENT : HAWKEYE_INPUT_EVENT;
  const send = (input: HawkeyeInput) => void emitTo("main", inputEvent, input);
  const run = (command: Command) => send({ type: "run", command });
  const display = (change: Partial<DisplaySettings>) => send({ type: "display", change });

  let data: HawkeyePayload | null = null;
  let layout = -1;
  const sort: Sort = { column: 2, sorted: false, origin: true };
  const settings = (change: Partial<HawkeyeSettings>) => data && run(settingsCommand(data.settings, change));

  // ---------------------------------------------------------------- top: pages and filter bar
  const top = document.createElement("div");
  top.id = "hk-top";
  const tabs = document.createElement("div");
  tabs.className = "hk-tabs";
  const pages = document.createElement("div");
  pages.id = "hk-pages";
  const statsPage = document.createElement("div");
  const simplePage = document.createElement("div");
  const detailPage = document.createElement("div");
  for (const page of [statsPage, simplePage, detailPage]) page.className = "hk-page";
  pages.append(statsPage, simplePage, detailPage);
  const pageTabs = [t("Movelistframe.statisticsPanel"), t("Movelistframe.topPanle.simpleList"), t("Movelistframe.topPanle.detailList")].map((text, k) => {
    const tab = document.createElement("span");
    tab.className = "hk-tab";
    tab.textContent = text;
    tab.dataset.key = `hawkeye.page.${k}`;
    tab.onclick = () => {
      if (!data || data.settings.topPage === k) return;
      // changeListener2: 简易列表 sorts by 胜率波动 (2), 详细列表 by 胜率波动 (3).
      if (k === 1) sort.column = 2;
      if (k === 2) sort.column = 3;
      reportLayout();
      settings({ topPage: k });
    };
    tabs.append(tab);
    return tab;
  });
  top.append(tabs, pages);

  const statistics = mountStatistics(statsPage, { run });

  const onRow = (row: Row | undefined) => {
    // handleTableDoubleClick: any click; the core ignores it while auto analysis runs.
    if (row && data) run({ type: "Hawkeye", op: { type: "GotoRow", document: data.document, node: row.e.node } });
  };
  const sortBy = (column: number, threeState: boolean) => {
    sort.column = column;
    if (threeState) {
      if (sort.origin) {
        sort.origin = false;
        sort.sorted = false;
      } else if (!sort.sorted) sort.sorted = true;
      else {
        sort.origin = true;
        sort.sorted = false;
      }
    } else sort.sorted = !sort.sorted;
    reportLayout();
    render();
  };
  let menuTable: ListTable | null = null;
  const menu = document.createElement("div");
  menu.className = "hk-menu";
  menu.hidden = true;
  const exportItem = document.createElement("div");
  exportItem.textContent = t("JTabel.export");
  exportItem.dataset.key = "hawkeye.export";
  exportItem.onclick = () => {
    menu.hidden = true;
    if (menuTable) run({ type: "ExportTable", rows: menuTable.exportRows() });
  };
  menu.append(exportItem);
  const showMenu = (table: ListTable) => (x: number, y: number) => {
    menuTable = table;
    menu.hidden = false;
    // Inside the window, beside the pointer.
    menu.style.left = `${Math.max(0, Math.min(x, window.innerWidth - menu.offsetWidth))}px`;
    menu.style.top = `${Math.max(0, Math.min(y, window.innerHeight - menu.offsetHeight))}px`;
  };
  document.addEventListener("mousedown", (ev) => {
    if (!menu.contains(ev.target as Node)) menu.hidden = true;
  });

  const katago = () => !!data?.pane.katago;
  const minSort = (column: number) => sortBy(column, column === 2 || (katago() && column === 3));
  const black: ListTable = new ListTable("hawkeye.simple.black", [1], minSort, onRow, (x, y) => showMenu(black)(x, y));
  const white: ListTable = new ListTable("hawkeye.simple.white", [1], minSort, onRow, (x, y) => showMenu(white)(x, y));
  simplePage.append(black.scroll, white.scroll);
  const detail: ListTable = new ListTable(
    "hawkeye.detail",
    [0, 2],
    (column) => sortBy(column, column === 3 || (katago() && column === 4)),
    onRow,
    (x, y) => showMenu(detail)(x, y),
    () => reportLayout(),
  );
  detailPage.append(detail.scroll);

  // The filter bar (MoveListFrame.java:1406-1428).
  const filter = document.createElement("div");
  filter.id = "hk-filter";
  const button = (key: string, text: string, action: () => void) => {
    const b = document.createElement("button");
    b.dataset.key = key;
    b.textContent = text;
    b.tabIndex = -1;
    b.onclick = action;
    return b;
  };
  const hideMove = button("hawkeye.hideMove", "", () => data && settings({ showList: !data.settings.showList }));
  hideMove.className = "icon";
  const hideIcons = { up: icon("chevron-up"), down: icon("chevron-down") };
  const tableThreshold = button("hawkeye.tableThreshold", t("Movelistframe.btnSetTabelThresold"), () => data && openSetThreshold(data, { run, display }));
  const statThreshold = button("hawkeye.statisticsThreshold", t("Movelistframe.btnSetStatisticsThresold"), () => data && openStatisticsThreshold(data, { run, display }));
  const sideBox = (key: string, label: string) => {
    const wrap = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.dataset.key = key;
    box.tabIndex = -1;
    const name = document.createElement("span");
    name.textContent = label;
    wrap.append(name, box);
    return { wrap, box };
  };
  const blackBox = sideBox("hawkeye.black", t("Movelistframe.checkBlackFilter"));
  const whiteBox = sideBox("hawkeye.white", t("Movelistframe.checkWhiteFilter"));
  const sides = () => run({ type: "Hawkeye", op: { type: "SetSides", black: blackBox.box.checked, white: whiteBox.box.checked } });
  blackBox.box.onchange = sides;
  whiteBox.box.onchange = sides;
  const branchLabel = document.createElement("span");
  branchLabel.textContent = t("Movelistframe.lblShowBranch");
  const branch = document.createElement("select");
  branch.dataset.key = "hawkeye.branch";
  branch.tabIndex = -1;
  for (const [value, text] of [["Main", t("Movelistframe.lblShowBranchItemMain")], ["Current", t("Movelistframe.lblShowBranchItemCurrent")]]) {
    const o = document.createElement("option");
    o.value = value;
    o.textContent = text;
    branch.append(o);
  }
  branch.onchange = () => settings({ branch: branch.value as HawkeyeSettings["branch"] });
  const periods: [HawkeyePeriod, string][] = [
    ["Current", t("Movelistframe.current")],
    ["All", t("Movelistframe.all")],
    ["Opening", t("Movelistframe.open")],
    ["Middle", t("Movelistframe.middle")],
    ["End", t("Movelistframe.end")],
    ["Custom", t("Movelistframe.chkCustom")],
  ];
  const periodBoxes = periods.map(([period, text]) => {
    const wrap = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.dataset.key = `hawkeye.period.${period}`;
    box.tabIndex = -1;
    // A ButtonGroup of check boxes: choosing one clears the others; the chosen one stays chosen.
    box.onclick = (ev) => {
      ev.preventDefault();
      if (data && data.settings.period !== period) settings({ period });
    };
    wrap.append(box, text);
    return { period, wrap, box };
  });
  /** IntDocument fields: only an integer is taken; the middle and opening ends turn red otherwise. */
  const intField = (key: string, apply: (v: number) => Partial<HawkeyeSettings>, red: boolean) => {
    const f = document.createElement("input");
    f.type = "text";
    f.dataset.key = key;
    f.addEventListener("input", () => {
      const ok = /^-?\d+$/.test(f.value.trim());
      if (red) f.classList.toggle("invalid", !ok);
      if (ok) settings(apply(Number.parseInt(f.value.trim(), 10)));
    });
    return f;
  };
  const customStart = intField("hawkeye.customStart", (v) => ({ customStart: v }), false);
  const customEnd = intField("hawkeye.customEnd", (v) => ({ customEnd: v }), false);
  const dash = document.createElement("span");
  dash.textContent = "-";
  const openingLabel = document.createElement("span");
  openingLabel.textContent = t("Movelistframe.openingEndMove");
  const opening = intField("hawkeye.openingEndMove", (v) => ({ openingEndMove: v }), true);
  const middleLabel = document.createElement("span");
  middleLabel.textContent = t("Movelistframe.middleEndMove");
  const middle = intField("hawkeye.middleEndMove", (v) => ({ middleEndMove: v }), true);
  // Related controls stand together; a wider gap starts the next group.
  for (const first of [blackBox.wrap, branchLabel, periodBoxes[0].wrap, customStart, openingLabel, middleLabel]) first.classList.add("group");
  filter.append(hideMove, tableThreshold, statThreshold, blackBox.wrap, whiteBox.wrap, branchLabel, branch,
    ...periodBoxes.map((p) => p.wrap), customStart, dash, customEnd, openingLabel, opening, middleLabel, middle);

  // ---------------------------------------------------------------- bottom: charts
  const bottom = document.createElement("div");
  bottom.id = "hk-bottom";
  document.body.append(top, filter, bottom, menu);
  const charts: ChartPanel = mountCharts(bottom, { run, display, openMatchHelp: () => data && openMatchAiHelp(data) });

  // ---------------------------------------------------------------- layout and rendering
  /** reSetLoc (MoveListFrame.java:2058-2075): the chart panel takes a quarter of the frame plus
   *  its strips while the list shows, all but the filter bar while it is hidden. Java's heights are
   *  of the frame, its title bar included (about 30 px). */
  const place = () => {
    if (!data) return;
    const h = window.innerHeight;
    const frameHeight = h + 30;
    const s = data.settings;
    const bottomHeight = s.showList ? (s.showGraph ? Math.floor(frameHeight / 4) + 95 : 100) : h - filter.offsetHeight;
    bottom.style.height = `${Math.max(0, bottomHeight)}px`;
    filter.style.bottom = `${bottomHeight}px`;
    top.hidden = !s.showList;
    top.style.height = `${Math.max(0, h - bottomHeight - filter.offsetHeight)}px`;
    const tabsHeight = tabs.offsetHeight;
    pages.style.top = `${tabsHeight}px`;
    pages.style.height = `${Math.max(0, h - bottomHeight - filter.offsetHeight - tabsHeight)}px`;
    const pw = pages.clientWidth, ph = pages.clientHeight;
    const half = Math.floor(pw / 2);
    Object.assign(black.scroll.style, { left: "0px", top: "0px", width: `${half}px`, height: `${ph}px` });
    Object.assign(white.scroll.style, { left: `${half}px`, top: "0px", width: `${pw - half}px`, height: `${ph}px` });
    Object.assign(detail.scroll.style, { left: "0px", top: "0px", width: `${pw}px`, height: `${ph}px` });
    black.layout();
    white.layout();
    detail.layout();
    statistics.resize();
    charts.resize();
  };

  const reportLayout = () => {
    send({ type: "layout", sort: sort.column, columns: detail.widths });
  };

  const render = () => {
    const p = data;
    if (!p) return;
    setLanguage(p.language);
    const s = p.settings;
    const pane = p.pane;
    if (p.layout !== layout) {
      layout = p.layout;
      // MoveListFrame constructor: sortnum 3 on 详细列表, else 2; |Δ| descending.
      Object.assign(sort, { column: s.topPage === 2 ? 3 : 2, sorted: false, origin: true });
      const count = pane.katago ? 12 : 9;
      // Java restores saved widths from a 17-entry list only for an 11-column table, which never
      // exists, so a 12-column layout lost its last three widths; all saved widths apply here.
      const saved = p.columns.length === count ? p.columns : p.columns.length >= 9 ? [...p.columns.slice(0, 9), ...DETAIL_WIDTHS.slice(9, count)] : null;
      detail.setHeaders(detailHeaders(pane.katago), saved ?? DETAIL_WIDTHS.slice(0, count));
    } else detail.setHeaders(detailHeaders(pane.katago), detail.preferred);
    pageTabs.forEach((tab, k) => tab.classList.toggle("sel", s.topPage === k));
    statsPage.hidden = s.topPage !== 0;
    simplePage.hidden = s.topPage !== 1;
    detailPage.hidden = s.topPage !== 2;
    black.setHeaders(minHeaders(true, pane.katago), null);
    white.setHeaders(minHeaders(false, pane.katago), null);

    const mark = p.mark;
    const marked = (e: MoveEvaluation) => !!mark && e.point.x === mark.x && e.point.y === mark.y;
    // The pinned current move on the sunken ground; the move marked on the board as the selection,
    // its other figures in the selection's text colour. The change columns keep their class colour.
    const lookOf = (diffColumns: [number, number | null]) => (row: Row, column: number): CellLook => {
      const isMarked = !row.pinned && marked(row.e);
      const bg = row.pinned ? "var(--ds-sunken)" : isMarked ? "var(--ds-accent-soft)" : "";
      const key = column === diffColumns[0] || column === diffColumns[1];
      let fg = isMarked ? "var(--ds-accent-text)" : "";
      if (column === diffColumns[0]) fg = diffColor(row.cells[column], 3, p.winLoss);
      else if (column === diffColumns[1]) fg = diffColor(row.cells[column], 1.5, p.scoreLoss);
      return { fg, bg, key };
    };
    const moves = pane.moves;
    const simple = pane.rows.map((i) => moves[i]);
    const minKeyNow = minKey(sort.column, pane.katago, sort);
    const toRows = (list: MoveEvaluation[], cells: (e: MoveEvaluation) => string[]) => list.map((e) => ({ e, cells: cells(e), pinned: false }));
    black.setRows(toRows(sorted(simple.filter((e) => e.mover === "Black"), minKeyNow, sort, false), (e) => minCells(e, p)),
      lookOf([2, pane.katago ? 3 : null]));
    white.setRows(toRows(sorted(simple.filter((e) => e.mover === "White"), minKeyNow, sort, false), (e) => minCells(e, p)),
      lookOf([2, pane.katago ? 3 : null]));
    const detailRows = toRows(
      sorted(pane.detail_rows.map((i) => moves[i]), detailKey(sort.column, pane.katago, sort), sort, sort.column === 0),
      (e) => detailCells(e, p),
    );
    if (pane.pinned !== null) detailRows.unshift({ e: moves[pane.pinned], cells: detailCells(moves[pane.pinned], p), pinned: true });
    detail.setRows(detailRows, lookOf([3, pane.katago ? 4 : null]));

    hideMove.replaceChildren(s.showList ? hideIcons.up : hideIcons.down);
    tableThreshold.hidden = s.topPage === 0;
    statThreshold.hidden = s.topPage !== 0;
    blackBox.box.checked = p.black;
    whiteBox.box.checked = p.white;
    branch.value = s.branch;
    for (const b of periodBoxes) b.box.checked = s.period === b.period;
    const custom = s.period === "Custom";
    customStart.disabled = !custom;
    customEnd.disabled = !custom;
    const setField = (f: HTMLInputElement, v: number) => {
      if (document.activeElement !== f) f.value = String(v);
    };
    setField(customStart, s.customStart);
    setField(customEnd, s.customEnd);
    setField(opening, s.openingEndMove);
    setField(middle, s.middleEndMove);

    place();
    statistics.render(p);
    charts.render(p);
    refreshDialogs(p);
    report();
  };

  /** Acceptance automation: what is shown and where its controls are. */
  let lastReport = "";
  const report = () => {
    if (!data) return;
    // The statistics page, the charts and the dialogs list their own controls.
    const own = [tabs, simplePage, detailPage, filter, menu]
      .flatMap((host) => [host, ...host.querySelectorAll<HTMLElement>("[data-key]")])
      .filter((el) => !!el.dataset.key && el.getClientRects().length > 0)
      .map((el) => {
        const r = el.getBoundingClientRect();
        return {
          key: el.dataset.key ?? "",
          text: el.textContent ?? "",
          checked: el instanceof HTMLInputElement && el.type === "checkbox" ? el.checked : undefined,
          value: el instanceof HTMLInputElement && el.type === "text" ? el.value : el instanceof HTMLSelectElement ? el.value : undefined,
          x: Math.round(r.left + r.width / 2),
          y: Math.round(r.top + r.height / 2),
        };
      });
    const page = data.settings.topPage;
    const rows = page === 2 ? detail.texts() : page === 1 ? [...black.texts(), ...white.texts()] : [];
    const r: HawkeyeReport = {
      topPage: page,
      chart: data.settings.chart,
      rows,
      dialog: openDialog(),
      targets: [...own, ...charts.targets(), ...statistics.targets(), ...dialogTargets()],
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    send({ type: "report", report: r });
  };

  // Keys of the frame and its tables (MoveListFrame.java:1562-1690): Y closes, Q 总在最前.
  document.addEventListener("keydown", (ev) => {
    const target = ev.target as HTMLElement;
    if (appMenuKey(ev) || target instanceof HTMLInputElement || target instanceof HTMLSelectElement) return;
    if (ev.code === "KeyY" || ev.code === "KeyQ") {
      ev.preventDefault();
      send({ type: "key", code: ev.code });
    }
  });
  document.addEventListener("contextmenu", (ev) => ev.preventDefault());

  void self.listen<HawkeyePayload>(HAWKEYE_DATA_EVENT, (e) => {
    data = e.payload;
    const root = document.documentElement.style;
    // The session's 界面字体大小 sizes the type scale, Config.menuHeight the list rows; a font the
    // theme names replaces the interface face in the lists.
    root.setProperty("--frame-font-size", `${data.frameFontSize}px`);
    root.setProperty("--hk-row", `${data.menuHeight}px`);
    const face = namedFace(data.fonts.ui);
    if (face) root.setProperty("--hk-ui-font", face);
    else root.removeProperty("--hk-ui-font");
    render();
  }).then(() => send({ type: "ready" }));
  window.addEventListener("resize", () => {
    place();
    report();
  });
}
