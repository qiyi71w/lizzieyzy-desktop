// 独立选点列表 window (AnalysisFrame.java): it shows what the main window relays (rows and texts
// formatted there from the core view, see analysis/table.ts) and reports its input back: hovers,
// clicks, the wheel, keys, header sorts, the four boxes and its column widths. It holds no
// analysis state of its own. The same page serves 双引擎模式's secondary window (AnalysisFrame
// index 2), started by suggestions.html with the design base and its stylesheet. Also the JTable
// column layout shared with the main window's 选点列表, and the face rule both windows' lists
// follow; the main window loads this module for those, so it imports no style sheets.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { appMenuKey } from "../appMenuKey";
import { applyTypeSizes, onThemeChange, token } from "../design/theme";
import { GRAPH_FONT } from "../workbench/paint";

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
  /** CSS colours (design and stage tokens); empty for the table's own. */
  fg: string;
  bg: string;
  /** 占比: the share bar's share of the cell width, percent. */
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
  /** The column the rows are sorted by (descending), null in the engine's order. */
  sorted: number | null;
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

/** The theme's own face from a font stack the main window resolved (paint.ts themeFonts), or
 *  null when the theme names none and the interface face (--ds-font) applies: an unnamed font
 *  resolves to the Dialog stack, which starts as GRAPH_FONT does. */
export function namedFace(stack: string): string | null {
  return stack.startsWith(GRAPH_FONT.replace(/, sans-serif$/, "")) ? null : stack;
}

/** AnalysisFrame bottom panel: one 22 px line with the list, half the window with the 柱状图, all
 *  of it without the list (componentResized, AnalysisFrame.java:199-215). The line holds the
 *  totals and the four boxes; where both do not fit, the boxes move to a line of their own. */
const BOTTOM = 22;

/** The four boxes in Java's order (AnalysisFrame.java:120-189). */
const BOX_KEYS: FramePayload["boxes"][number]["key"][] = ["list", "graph", "showNext", "mouseMove"];

/** A header text with line-break opportunities before its bracket ("Percents(%)") and between
 *  camel-cased words ("ScoreStdev"), where a narrow column wraps it instead of cutting it. */
function breakable(text: string): (string | HTMLElement)[] {
  // Lookahead only: older macOS WebKit rejects lookbehind when it parses the module.
  const parts = text.replace(/(.)(?=\()|([a-z])(?=[A-Z])/g, "$1$2\u0000").split("\u0000");
  return parts.flatMap((part, k) => (k === 0 ? [part] : [document.createElement("wbr"), part]));
}

/** The actual move's cell text in parts: in 序号 its rank with a smaller 实战 note under it
 *  ("15(实战)"; an actual move outside the candidates has the note alone), in a figure column a
 *  smaller change before the value ("↓-4.6(50.4)"), so the value lines up with the column's
 *  other rows; a column too narrow for both breaks between them. */
export function actualParts(text: string, rank: boolean): (string | HTMLElement)[] {
  const open = text.indexOf("(");
  if (open <= 0 && !rank) return [text];
  const note = document.createElement("span");
  note.className = "actual-note";
  if (open <= 0) {
    note.textContent = text;
    return [note];
  }
  const head = text.slice(0, open), inner = text.slice(open + 1, text.lastIndexOf(")"));
  note.textContent = rank ? inner : head;
  return rank ? [head, note] : [note, document.createElement("wbr"), inner];
}

export function bootFrame(): void {
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
  const boxRow = document.createElement("div");
  boxRow.className = "boxes";
  bottom.append(boxRow);
  const boxes = new Map<string, HTMLInputElement>();
  for (const key of BOX_KEYS) {
    const label = document.createElement("label");
    const input = document.createElement("input");
    input.type = "checkbox";
    // Tab reaches the boxes; a mouse press on a box or its name leaves the focus where it was, so
    // the window's keys (Space, the arrows) keep acting on the list after a click.
    label.addEventListener("mousedown", (ev) => ev.preventDefault());
    // Space on a focused box turns that box only (its click below), not 分析 as well.
    input.addEventListener("keydown", (ev) => {
      if (ev.key === " ") ev.stopPropagation();
    });
    // The main window turns the setting and sends the box's state back.
    input.addEventListener("click", (ev) => {
      ev.preventDefault();
      send({ type: "box", key });
    });
    label.append(input, document.createElement("span"));
    label.dataset.key = key;
    boxRow.append(label);
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
  // The header row is one Tab stop: ←/→ and Home/End move between the columns, Enter or Space
  // sorts the focused one as a click does. A press on a header leaves the focus where it was, so
  // the window's keys (Space, the arrows) keep acting on the list after a click.
  let headCursor = 0;
  const markHeader = (k: number) => {
    headCursor = k;
    [...headRow.cells].forEach((th, i) => (th.tabIndex = i === k ? 0 : -1));
  };
  headRow.addEventListener("mousedown", (ev) => ev.preventDefault());
  headRow.addEventListener("focusin", (ev) => {
    const cell = (ev.target as HTMLElement).closest("th");
    if (cell) markHeader((cell as HTMLTableCellElement).cellIndex);
  });
  headRow.addEventListener("keydown", (ev) => {
    const n = headRow.cells.length;
    const k = headCursor;
    let to: number;
    if (ev.key === "ArrowRight") to = Math.min(k + 1, n - 1);
    else if (ev.key === "ArrowLeft") to = Math.max(k - 1, 0);
    else if (ev.key === "Home") to = 0;
    else if (ev.key === "End") to = n - 1;
    else if (ev.key === "Enter" || ev.key === " ") to = -1;
    else return;
    ev.preventDefault();
    ev.stopPropagation();
    if (!n) return;
    if (to < 0) send({ type: "sort", column: k });
    else headRow.cells[to].focus();
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
    if (!appMenuKey(ev) && ["KeyU", "KeyQ", "KeyY", "Space", "ArrowUp", "ArrowDown"].includes(ev.code)) {
      ev.preventDefault();
      send({ type: "key", code: ev.code });
    }
  });

  /** The list's lower edge: the strip's height (BOTTOM, or more where the boxes take lines of
   *  their own); the 柱状图 starts below it. */
  let strip = BOTTOM;

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
    // paintBottomPanel (AnalysisFrame.java:485-555): the totals on the strip, the 柱状图 below it
    // on the panel ground.
    if (data.stats === null) return;
    const size = Number.parseFloat(token("--ds-size")) || 12;
    const face = token("--ds-font");
    g.textBaseline = "alphabetic";
    g.font = `${size}px ${face}`;
    g.fillStyle = token("--ds-text");
    g.fillText(data.stats, 8, 15);
    if (!data.graph) return;
    g.fillStyle = token("--ds-panel");
    g.fillRect(0, strip - 1, w, h - strip + 1);
    g.fillStyle = token("--ds-line");
    g.fillRect(0, strip - 2, w, 1);
    const index = token("--ds-text-3");
    const fill = token("--ds-line-strong");
    const text = token("--ds-text-2");
    const count = Math.floor((h - strip) / 20);
    for (let i = 0; i < count; i++) {
      g.font = `${size - 1}px ${face}`;
      g.fillStyle = index;
      g.fillText(String(i + 1), 3, strip + i * 20 + 15);
      const bar = data.bars[i];
      if (!bar) continue;
      const length = Math.trunc((w - 80) * bar.ratio);
      g.fillStyle = fill;
      g.fillRect(20, strip + i * 20 + 2, length, 16);
      g.font = `${size}px ${face}`;
      g.fillStyle = text;
      g.fillText(bar.text, 26 + length, strip + i * 20 + 15);
    }
  };

  const place = () => {
    if (!data) return;
    // The totals keep the line's left end; the boxes share it at the right end when both fit with
    // a gap, otherwise they start a line under the totals and wrap there as a group.
    let shared = data.stats === null;
    if (!shared) {
      const g = canvas.getContext("2d")!;
      g.font = `${Number.parseFloat(token("--ds-size")) || 12}px ${token("--ds-font")}`;
      shared = 8 + Math.ceil(g.measureText(data.stats!).width) + 16 + boxRow.offsetWidth + 8 <= bottom.clientWidth;
    }
    boxRow.style.top = `${shared ? 0 : BOTTOM - 3}px`;
    strip = boxRow.offsetTop + boxRow.offsetHeight + 1;
    const h = window.innerHeight;
    const bottomHeight = !data.list ? h : data.graph ? Math.max(strip, Math.floor(h / 2)) : strip;
    top.hidden = !data.list;
    top.style.height = `${h - bottomHeight}px`;
    bottom.style.height = `${bottomHeight}px`;
  };
  onThemeChange(() => {
    place();
    paintBottom();
  });

  const render = () => {
    if (!data) return;
    for (const box of data.boxes) {
      const input = boxes.get(box.key)!;
      input.checked = box.checked;
      (input.nextElementSibling as HTMLElement).textContent = box.label;
    }
    place();
    if (data.layout !== layout) {
      layout = data.layout;
      colgroup.replaceChildren(...data.headers.map(() => document.createElement("col")));
      const refocus = headRow.contains(document.activeElement);
      headRow.replaceChildren(...data.headers.map((text) => {
        const th = document.createElement("th");
        th.append(...breakable(text));
        return th;
      }));
      markHeader(Math.max(0, Math.min(headCursor, data.headers.length - 1)));
      if (refocus) headRow.cells[headCursor]?.focus();
      preferred = data.widths ?? preferred;
      relayout();
    }
    // The main window sorts one column at a time, always descending (AnalysisFrame.java:948-989).
    const sorted = data.sorted;
    [...headRow.cells].forEach((th, column) => {
      if (column === sorted) th.setAttribute("aria-sort", "descending");
      else th.removeAttribute("aria-sort");
    });
    while (body.rows.length > data.rows.length) body.deleteRow(body.rows.length - 1);
    data.rows.forEach((cells, k) => {
      const row = body.rows[k] ?? body.insertRow();
      while (row.cells.length > cells.length) row.deleteCell(row.cells.length - 1);
      cells.forEach((c, column) => {
        const cell = row.cells[column] ?? row.insertCell();
        // The actual move's cells as the main window's list shows them (actualParts).
        if (cell.dataset.text !== c.text) {
          cell.dataset.text = c.text;
          cell.replaceChildren(...actualParts(c.text, column === 0));
        }
        cell.classList.toggle("split", c.text.indexOf("(") > 0);
        cell.style.color = c.fg;
        // 占比: a hard-edged bar of the share token behind the right-aligned number, growing
        // leftwards from the cell's right padding, over the row's own tint (as the main list).
        const tint = cells[0].bg;
        const bar = c.bar === null ? "" : `linear-gradient(to left, var(--ws-share-bar) ${Math.min(100, c.bar)}%, transparent ${Math.min(100, c.bar)}%) right 4px center / calc(100% - 8px) calc(100% - 8px) no-repeat`;
        cell.style.background = c.bar === null ? c.bg : tint ? `${bar}, ${tint}` : bar;
      });
    });
    paintBottom();
  };

  // Only the rows sent to this window: the global listener would take the other window's too.
  void self.listen<FramePayload>(FRAME_DATA_EVENT, (e) => {
    data = e.payload;
    const root = document.documentElement.style;
    // The session's 界面字体大小 sizes the type scale, Config.menuHeight the rows; a font the theme
    // names replaces the interface face in the list.
    applyTypeSizes(data.frameFontSize);
    root.setProperty("--af-row", `${data.menuHeight}px`);
    const face = namedFace(data.uiFont);
    if (face) root.setProperty("--af-ui-font", face);
    else root.removeProperty("--af-ui-font");
    render();
  }).then(() => send({ type: "ready" }));
  window.addEventListener("resize", () => {
    place();
    relayout();
    paintBottom();
  });
}
