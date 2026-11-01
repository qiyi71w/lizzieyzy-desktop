// 独立选点列表 window (AnalysisFrame.java): it shows what the main window relays (rows and texts
// formatted there from the core view, see analysis/table.ts) and reports its input back: hovers,
// clicks, the wheel, keys, header sorts, the four boxes and its column widths. It holds no
// analysis state of its own. The same page serves 双引擎模式's secondary window (AnalysisFrame
// index 2). Also the JTable column layout shared with the main window's 选点列表.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";

/** The primary engine's window and 双引擎模式's secondary engine window (src-tauri). */
export const FRAME_LABEL = "suggestions";
export const SUB_FRAME_LABEL = "suggestions-sub";
/** Main window → frame (sent to each window by its label). */
export const FRAME_DATA_EVENT = "suggestions-frame-data";
/** Frame → main window, one event per window. */
export const FRAME_INPUT_EVENT = "suggestions-frame-input";
export const SUB_FRAME_INPUT_EVENT = "suggestions-sub-frame-input";

export interface FrameCell {
  text: string;
  /** CSS colours; empty for the table's own. */
  fg: string;
  bg: string;
  /** 占比: the LIGHT_GRAY bar's share of the cell width, percent. */
  bar: number | null;
}

/** One bar of the 柱状图 (the candidates in engine order). */
export interface FrameBar {
  /** Of the most allocated candidate. */
  ratio: number;
  text: string;
}

export interface FramePayload {
  headers: string[];
  /** Preferred widths to lay the columns out with; sent when the layout is (re)created. */
  widths: number[] | null;
  /** Identifies one layout of the window (opening, 7/9 columns, the boxes recreate it). */
  layout: number;
  rows: FrameCell[][];
  list: boolean;
  graph: boolean;
  boxes: { key: "list" | "graph" | "showNext" | "mouseMove"; label: string; checked: boolean }[];
  /** 总计算量 / 最高计算量 / 集中度 line; null without candidates (nothing is painted then). */
  stats: string | null;
  bars: FrameBar[];
  /** Lizzie.config.uiFontName as the main window resolved it, Config.menuHeight and
   *  Config.frameFontSize of this session: the table fonts and rows (AnalysisFrame.java:95-101). */
  uiFont: string;
  menuHeight: number;
  frameFontSize: number;
}

export type FrameInput =
  | { type: "ready" }
  | { type: "hover"; row: number }
  | { type: "leave" }
  | { type: "click"; row: number; right: boolean }
  | { type: "wheel"; down: boolean }
  | { type: "sort"; column: number }
  | { type: "key"; code: string }
  | { type: "box"; key: FramePayload["boxes"][number]["key"] }
  | { type: "columns"; widths: number[] };

/** TableColumn's minimum width. */
export const MIN_COLUMN = 15;

/** JTable's fit of preferred widths into the viewport (AUTO_RESIZE_SUBSEQUENT_COLUMNS): each
 *  column gets an equal share of the difference. */
export function layoutColumns(preferred: number[], width: number): number[] {
  const n = preferred.length;
  const extra = width - preferred.reduce((s, w) => s + w, 0);
  return preferred.map((w, k) => Math.max(0, w + Math.floor((extra * (k + 1)) / n) - Math.floor((extra * k) / n)));
}

/** Dragging the right border of column `k` by `d`: the subsequent columns give or take the
 *  difference equally, none below the minimum width. */
export function resizeColumns(start: number[], k: number, d: number): number[] {
  const rest = start.length - k - 1;
  if (rest <= 0) return start;
  const restWidth = start.slice(k + 1).reduce((s, w) => s + w, 0);
  const delta = Math.round(Math.max(MIN_COLUMN - start[k], Math.min(d, restWidth - MIN_COLUMN * rest)));
  const widths = start.slice();
  widths[k] += delta;
  for (let j = 0; j < rest; j++) widths[k + 1 + j] -= Math.floor((delta * (j + 1)) / rest) - Math.floor((delta * j) / rest);
  // An uneven share may leave one column short; the next one with room makes up for it.
  for (let j = k + 1; j < widths.length; j++) {
    const short = MIN_COLUMN - widths[j];
    if (short <= 0) continue;
    widths[j] = MIN_COLUMN;
    const donor = widths.findIndex((w, i) => i > k && i !== j && w - short >= MIN_COLUMN);
    if (donor >= 0) widths[donor] -= short;
  }
  return widths;
}

/** Column borders in the header row drag like a JTableHeader's. `apply` lays out live widths;
 *  `done` receives the final ones. */
export function bindColumnResize(
  head: HTMLTableRowElement,
  current: () => number[],
  apply: (widths: number[]) => void,
  done: (widths: number[]) => void,
): void {
  const edgeAt = (x: number): number => {
    const cells = head.cells;
    for (let k = 0; k < cells.length - 1; k++) {
      const r = cells[k].getBoundingClientRect();
      if (r.width > 0 && Math.abs(x - r.right) <= 3) return k;
    }
    return -1;
  };
  head.addEventListener("mousemove", (ev) => {
    head.style.cursor = edgeAt(ev.clientX) >= 0 ? "col-resize" : "";
  });
  head.addEventListener("mousedown", (ev) => {
    const k = ev.button === 0 ? edgeAt(ev.clientX) : -1;
    if (k < 0) return;
    ev.preventDefault();
    const start = current(), x0 = ev.clientX;
    const move = (e: MouseEvent) => apply(resizeColumns(start, k, e.clientX - x0));
    const up = (e: MouseEvent) => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      done(resizeColumns(start, k, e.clientX - x0));
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  });
}

// ------------------------------------------------------------------ the window

/** AnalysisFrame bottom panel: 22 px with the list, half the window with the 柱状图, all of it
 *  without the list (componentResized, AnalysisFrame.java:199-215). */
const BOTTOM = 22;
/** Lizzie.config.uiFontName (Dialog) as the main window resolves it. */
const UI_FONT = navigator.userAgent.includes("Windows")
  ? 'Arial, "Microsoft YaHei UI", "Microsoft YaHei", sans-serif'
  : navigator.userAgent.includes("Mac OS")
    ? '"Lucida Grande", "PingFang SC", sans-serif'
    : '"DejaVu Sans", "Noto Sans CJK SC", sans-serif';

const STYLE = `
html, body { margin: 0; height: 100%; overflow: hidden; background: #eee; font-family: ${UI_FONT}; }
#top { position: absolute; left: 0; top: 0; right: 0; overflow-x: hidden; overflow-y: auto; background: #fff; }
#top table { border-collapse: separate; border-spacing: 0; table-layout: fixed; }
/* rows Config.menuHeight, header Font(uiFontName, PLAIN, max(frameFontSize, 13)), cells
   Font(uiFontName, BOLD, max(frameFontSize, 14)) (AnalysisFrame.java:95-101), set from each payload */
#top th { position: sticky; top: 0; z-index: 1; height: var(--af-row, 20px); padding: 0; font-weight: normal; font-family: var(--af-ui-font, inherit); font-size: var(--af-head-size, 13px);
  background: rgb(238, 238, 238); border: solid rgb(128, 128, 128); border-width: 0 1px 1px 0; white-space: nowrap; overflow: hidden; user-select: none; }
#top td { height: var(--af-row, 20px); padding: 0; text-align: center; font-weight: bold; font-family: var(--af-ui-font, inherit); font-size: var(--af-cell-size, 14px); color: #000;
  border: solid rgb(128, 128, 128); border-width: 0 1px 1px 0; white-space: nowrap; overflow: hidden; cursor: default; }
#bottom { position: absolute; left: 0; right: 0; bottom: 0; background: #eee; }
#bottom canvas { position: absolute; left: 0; top: 0; }
#bottom label { position: absolute; top: 0; height: 18px; display: flex; align-items: center; font-size: 12px; white-space: nowrap; user-select: none; }
#bottom input { margin: 0 2px 0 0; }
`;

/** Checkbox bounds (x, width) in the bottom panel (AnalysisFrame.java:120-189). */
const BOX_PLACES: Record<FramePayload["boxes"][number]["key"], [number, number]> = {
  list: [370, 50],
  graph: [420, 65],
  showNext: [485, 65],
  mouseMove: [550, 95],
};

export function bootFrame(): void {
  const style = document.createElement("style");
  style.textContent = STYLE;
  document.head.append(style);
  const top = document.createElement("div");
  top.id = "top";
  const table = document.createElement("table");
  const colgroup = document.createElement("colgroup");
  const thead = document.createElement("thead");
  const headRow = thead.insertRow();
  const body = document.createElement("tbody");
  table.append(colgroup, thead, body);
  top.append(table);
  const bottom = document.createElement("div");
  bottom.id = "bottom";
  const canvas = document.createElement("canvas");
  bottom.append(canvas);
  const boxes = new Map<string, HTMLInputElement>();
  for (const key of Object.keys(BOX_PLACES) as (keyof typeof BOX_PLACES)[]) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    // setFocusable(false): the table keeps the keys.
    input.tabIndex = -1;
    input.addEventListener("mousedown", (ev) => ev.preventDefault());
    input.addEventListener("click", (ev) => {
      ev.preventDefault();
      send({ type: "box", key });
    });
    label.append(input, document.createElement("span"));
    label.style.left = `${BOX_PLACES[key][0]}px`;
    label.style.width = `${BOX_PLACES[key][1]}px`;
    label.dataset.key = key;
    bottom.append(label);
    boxes.set(key, input);
  }
  document.body.append(top, bottom);

  let data: FramePayload | null = null;
  let layout = -1;
  let preferred: number[] = [];
  let widths: number[] = [];

  const self = getCurrentWebviewWindow();
  const inputEvent = self.label === SUB_FRAME_LABEL ? SUB_FRAME_INPUT_EVENT : FRAME_INPUT_EVENT;
  const send = (input: FrameInput) => void emitTo("main", inputEvent, input);

  const applyWidths = (w: number[]) => {
    widths = w;
    table.style.width = `${w.reduce((s, x) => s + x, 0)}px`;
    [...colgroup.children].forEach((col, k) => ((col as HTMLElement).style.width = `${w[k] ?? 0}px`));
  };
  const relayout = () => {
    if (!preferred.length) return;
    applyWidths(layoutColumns(preferred, top.clientWidth));
    send({ type: "columns", widths });
  };
  bindColumnResize(headRow, () => widths, applyWidths, (w) => {
    preferred = w;
    applyWidths(w);
    send({ type: "columns", widths: w });
  });
  // header.mouseReleased: sortnum = the column under the pointer, a border drag included.
  headRow.addEventListener("mouseup", (ev) => {
    if (ev.button !== 0) return;
    const cell = (ev.target as HTMLElement).closest("th");
    if (cell) send({ type: "sort", column: (cell as HTMLTableCellElement).cellIndex });
  });

  const rowAt = (ev: MouseEvent): number => {
    const row = (ev.target as HTMLElement).closest<HTMLTableRowElement>("tbody tr");
    return row ? row.sectionRowIndex : -1;
  };
  table.addEventListener("mousemove", (ev) => {
    if (!(ev.target as HTMLElement).closest("thead")) send({ type: "hover", row: rowAt(ev) });
  });
  // fillsViewportHeight: the empty area below the rows is the table too (rowAtPoint -1).
  top.addEventListener("mousemove", (ev) => {
    if (ev.target === top) send({ type: "hover", row: -1 });
  });
  top.addEventListener("mouseleave", () => send({ type: "leave" }));
  top.addEventListener("click", (ev) => {
    const row = rowAt(ev);
    if (row >= 0) send({ type: "click", row, right: false });
  });
  top.addEventListener("auxclick", (ev) => {
    const row = rowAt(ev);
    if (row >= 0 && ev.button === 1) send({ type: "click", row, right: false });
  });
  top.addEventListener("contextmenu", (ev) => {
    ev.preventDefault();
    const row = rowAt(ev);
    if (row >= 0) send({ type: "click", row, right: true });
  });
  document.addEventListener("contextmenu", (ev) => ev.preventDefault());
  // With a clicked row the wheel steps its variation; otherwise it scrolls (handled by the
  // main window, which knows whether a row is clicked; it answers nothing for scrolling).
  top.addEventListener("wheel", (ev) => {
    if (ev.deltaY !== 0) send({ type: "wheel", down: ev.deltaY > 0 });
  }, { passive: true });
  document.addEventListener("keydown", (ev) => {
    if (["KeyU", "KeyQ", "KeyY", "Space", "ArrowUp", "ArrowDown"].includes(ev.code)) {
      ev.preventDefault();
      send({ type: "key", code: ev.code });
    }
  });

  const paintBottom = () => {
    if (!data) return;
    const w = bottom.clientWidth, h = bottom.clientHeight, dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    // paintBottomPanel (AnalysisFrame.java:485-555).
    if (data.stats === null) return;
    g.fillStyle = "#000";
    g.strokeStyle = "#000";
    g.font = `13px ${UI_FONT}`;
    g.textBaseline = "alphabetic";
    g.fillText(data.stats, 5, 15);
    if (!data.graph) return;
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(0, 20.5);
    g.lineTo(w, 20.5);
    g.stroke();
    const count = Math.floor((h - BOTTOM) / 20);
    for (let i = 0; i < count; i++) {
      g.fillText(String(i + 1), 3, BOTTOM + i * 20 + 15);
      g.fillStyle = "rgb(64, 64, 64)";
      const bar = data.bars[i];
      if (!bar) continue;
      const length = Math.trunc((w - 80) * bar.ratio);
      g.fillRect(20, BOTTOM + i * 20 + 2, length, 16);
      g.fillText(bar.text, 26 + length, BOTTOM + i * 20 + 15);
    }
  };

  const place = () => {
    if (!data) return;
    const h = window.innerHeight;
    const bottomHeight = !data.list ? h : data.graph ? Math.floor(h / 2) : BOTTOM;
    top.hidden = !data.list;
    top.style.height = `${h - bottomHeight}px`;
    bottom.style.height = `${bottomHeight}px`;
  };

  const render = () => {
    if (!data) return;
    place();
    if (data.layout !== layout) {
      layout = data.layout;
      colgroup.replaceChildren(...data.headers.map(() => document.createElement("col")));
      headRow.replaceChildren(...data.headers.map((text) => {
        const th = document.createElement("th");
        th.textContent = text;
        return th;
      }));
      preferred = data.widths ?? preferred;
      relayout();
    }
    while (body.rows.length > data.rows.length) body.deleteRow(body.rows.length - 1);
    data.rows.forEach((cells, k) => {
      const row = body.rows[k] ?? body.insertRow();
      while (row.cells.length > cells.length) row.deleteCell(row.cells.length - 1);
      cells.forEach((c, column) => {
        const cell = row.cells[column] ?? row.insertCell();
        if (cell.textContent !== c.text) cell.textContent = c.text;
        cell.style.color = c.fg;
        cell.style.background = c.bar === null
          ? c.bg
          : `linear-gradient(to right, rgb(192, 192, 192) ${Math.min(100, c.bar)}%, transparent ${Math.min(100, c.bar)}%)`;
      });
    });
    for (const box of data.boxes) {
      const input = boxes.get(box.key)!;
      input.checked = box.checked;
      (input.nextElementSibling as HTMLElement).textContent = box.label;
    }
    paintBottom();
  };

  // Only the rows sent to this window: the global listener would take the other window's too.
  void self.listen<FramePayload>(FRAME_DATA_EVENT, (e) => {
    data = e.payload;
    const root = document.documentElement.style;
    root.setProperty("--af-row", `${data.menuHeight}px`);
    root.setProperty("--af-ui-font", data.uiFont);
    root.setProperty("--af-head-size", `${Math.max(data.frameFontSize, 13)}px`);
    root.setProperty("--af-cell-size", `${Math.max(data.frameFontSize, 14)}px`);
    render();
  }).then(() => send({ type: "ready" }));
  window.addEventListener("resize", () => {
    place();
    relayout();
    paintBottom();
  });
}
