// 弈客直播中心 window (YikeLiveDialog.java): 分类 / 筛选 / 刷新 / 更多 / 同步 / 打开网页 in a left
// FlowLayout row, the eight-column live table (sortable by its header, single selection) and the
// status line. The core owns the list and its loading; this window keeps the interaction state the
// Swing dialog keeps (filter, sort, selection, the room it asked to follow, the status line) and
// reports every request to the main window.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { LiveCategory, LiveGameView } from "../contract/generated";
import { icon } from "../design/icons";
import { setLanguage, t } from "../i18n";
import { LIVE_DATA_EVENT, LIVE_INPUT_EVENT, type LiveInput, type LivePayload, type LiveReport } from "./model";
import { onlineText } from "./text";
import "../design/window";
import "./live.css";

/** The combo's entries in Java order (推荐 official=1, 地方 0, 个人 4). */
const CATEGORIES: [LiveCategory, () => string][] = [
  ["Recommend", () => t("YikeLiveDialog.category.recommend")],
  ["Local", () => t("YikeLiveDialog.category.local")],
  ["Personal", () => t("YikeLiveDialog.category.personal")],
];

/** LiveTableModel's columns. */
const COLUMNS: (() => string)[] = [
  () => t("YikeLiveDialog.column.status"),
  () => t("YikeLiveDialog.column.time"),
  () => t("YikeLiveDialog.column.game"),
  () => t("YikeLiveDialog.column.black"),
  () => t("YikeLiveDialog.column.white"),
  () => t("YikeLiveDialog.column.hands"),
  () => t("YikeLiveDialog.column.rate"),
  () => t("YikeLiveDialog.column.attention"),
];

/** LiveTableModel.getValueAt as text (the core's status already carries the pinned `*`). */
function cells(g: LiveGameView): string[] {
  return [g.status, g.time, g.game, g.black, g.white, String(g.hands), g.rate, g.attention];
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, id = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (id) e.id = id;
  if (text) e.textContent = text;
  return e;
}

export function bootYikeLive(): void {
  const send = (input: LiveInput) => void emitTo("main", LIVE_INPUT_EVENT, input);
  const self = getCurrentWebviewWindow();

  // ------------------------------------------------------------- top row
  const top = el("div", "yl-top");
  const categoryLabel = el("label");
  const category = el("select");
  category.dataset.key = "yikeLive.category";
  categoryLabel.htmlFor = category.id = "yl-category";
  const filterLabel = el("label");
  const filter = el("input");
  filter.type = "text";
  filter.dataset.key = "yikeLive.filter";
  filterLabel.htmlFor = filter.id = "yl-filter";
  const button = (key: string) => {
    const b = el("button");
    b.type = "button";
    b.dataset.key = `yikeLive.${key}`;
    return b;
  };
  const refresh = button("refresh");
  const more = button("more");
  const sync = button("sync");
  const web = button("openWeb");
  top.append(categoryLabel, category, filterLabel, filter, refresh, more, sync, web);

  // ------------------------------------------------------------- table
  const scroll = el("div", "yl-scroll");
  const table = el("table");
  table.tabIndex = 0;
  table.dataset.key = "yikeLive.table";
  const head = el("thead");
  const body = el("tbody");
  table.append(head, body);
  scroll.append(table);

  const bottom = el("div", "yl-bottom");
  const status = el("div", "yl-status");
  status.dataset.key = "yikeLive.status";
  status.textContent = " ";
  bottom.append(status);
  document.body.append(top, scroll, bottom);

  // ------------------------------------------------------------- state
  let payload: LivePayload | null = null;
  let generation = -1;
  let lastListStatus = "";
  let lastStatusSeq: number | null = null;
  let activeUrl = "";
  /** Model index of the selected game. */
  let selected: number | null = null;
  let sort: { column: number; ascending: boolean } | null = null;
  /** View order: model indexes after the row filter and the sort. */
  let order: number[] = [];
  const collator = new Intl.Collator(undefined);

  const games = () => payload?.live.games ?? [];

  /** TableRowSorter with RowFilter.regexFilter("(?i)" + quote(filter)): a row stays when any cell
   *  contains the trimmed text, ignoring case; the comparator is the Collator over toString
   *  (the model's column class is Object), ties in model order. */
  const computeOrder = () => {
    const needle = filter.value.trim().toLowerCase();
    const rows = games().map(cells);
    order = rows.map((_, i) => i).filter((i) => !needle || rows[i].some((c) => c.toLowerCase().includes(needle)));
    if (sort) {
      const { column, ascending } = sort;
      // DefaultRowSorter.compare: the sort key's order, then the model order in either direction.
      order.sort((a, b) => collator.compare(rows[a][column], rows[b][column]) * (ascending ? 1 : -1) || a - b);
    }
    if (selected !== null && !order.includes(selected)) selected = null;
  };

  let lastReport = "";
  const report = () => {
    const targets = [...document.querySelectorAll<HTMLElement>("[data-key]")].map((e) => {
      const r = e.getBoundingClientRect();
      return {
        key: e.dataset.key!,
        text: e instanceof HTMLInputElement || e instanceof HTMLSelectElement ? e.value : (e.textContent ?? ""),
        enabled: !((e instanceof HTMLButtonElement || e instanceof HTMLSelectElement) && e.disabled),
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + r.height / 2),
      };
    });
    const rows = order.map((i) => cells(games()[i]));
    const r: LiveReport = {
      category: category.value as LiveCategory,
      filter: filter.value,
      loading: payload?.live.loading ?? false,
      status: status.textContent ?? "",
      rows,
      selected: selected === null ? null : order.indexOf(selected),
      sort,
      activeUrl,
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    send({ type: "report", report: r });
  };

  const drawHeader = () => {
    const tr = el("tr");
    COLUMNS.forEach((text, column) => {
      const th = el("th");
      const label = el("span", "", text());
      th.append(label);
      th.dataset.key = `yikeLive.column.${column}`;
      if (sort?.column === column) {
        th.classList.add(sort.ascending ? "asc" : "desc");
        label.append(icon(sort.ascending ? "chevron-up" : "chevron-down"));
      }
      // Header clicks toggle the column ascending / descending (DefaultRowSorter.toggleSortOrder).
      th.onclick = () => {
        sort = sort?.column === column ? { column, ascending: !sort.ascending } : { column, ascending: true };
        drawRows();
      };
      tr.append(th);
    });
    head.replaceChildren(tr);
  };

  const drawRows = () => {
    computeOrder();
    drawHeader();
    const list = games();
    body.replaceChildren(
      ...order.map((index) => {
        const tr = el("tr");
        tr.dataset.index = String(index);
        for (const text of cells(list[index])) {
          const td = el("td", "", text);
          td.title = text;
          tr.append(td);
        }
        // Selecting repaints the rows in place, so a double click lands on the same row.
        tr.onmousedown = () => {
          selected = index;
          table.focus();
          showSelection();
        };
        tr.ondblclick = () => syncSelected();
        return tr;
      }),
    );
    showSelection();
  };

  const showSelection = () => {
    for (const tr of body.rows) tr.classList.toggle("selected", tr.dataset.index === String(selected));
    body.querySelector("tr.selected")?.scrollIntoView({ block: "nearest" });
    report();
  };

  const setStatus = (text: string) => {
    status.textContent = text || " ";
    report();
  };

  /** syncSelectedGame (YikeLiveDialog.java:284-301). */
  const syncSelected = () => {
    const game = selected === null ? undefined : games()[selected];
    if (!game) {
      setStatus(t("YikeLiveDialog.noSelection"));
      return;
    }
    if (!game.room_url) {
      activeUrl = "";
      setStatus(`${t("YikeLiveDialog.syncNoSgf")} ${game.game}`);
      return;
    }
    activeUrl = game.room_url;
    setStatus(`${t("YikeLiveDialog.syncing")}: ${game.game}`);
    send({ type: "follow", url: activeUrl });
  };

  const load = (more: boolean) => send({ type: "load", category: category.value as LiveCategory, more });

  refresh.onclick = () => load(false);
  more.onclick = () => load(true);
  sync.onclick = syncSelected;
  web.onclick = () => send({ type: "web" });
  category.onchange = () => load(false);
  filter.oninput = () => drawRows();

  table.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      syncSelected();
    } else if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (order.length === 0) return;
      const at = selected === null ? -1 : order.indexOf(selected);
      const next = e.key === "ArrowDown" ? Math.min(order.length - 1, at + 1) : Math.max(0, at - 1);
      selected = order[next];
      drawRows();
    }
  });

  // Escape closes (hides) the window, like installEscapeToClose with HIDE_ON_CLOSE.
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      void self.close();
    }
  });
  document.addEventListener("contextmenu", (e) => e.preventDefault());

  const render = (p: LivePayload) => {
    const first = payload === null;
    payload = p;
    setLanguage(p.language);
    document.title = t("YikeLiveDialog.title");
    categoryLabel.textContent = t("YikeLiveDialog.category");
    filterLabel.textContent = t("YikeLiveDialog.filter");
    refresh.textContent = t("YikeLiveDialog.refresh");
    more.textContent = t("YikeLiveDialog.more");
    sync.textContent = t("YikeLiveDialog.sync");
    web.textContent = t("YikeLiveDialog.openWeb");
    if (category.options.length === 0) {
      for (const [value, text] of CATEGORIES) category.append(new Option(text(), value));
    }
    if (category.value !== p.live.category) category.value = p.live.category;
    // setLoading: everything but the filter and the table waits for the list.
    for (const control of [refresh, more, sync, web, category]) control.disabled = p.live.loading;

    // fireTableDataChanged: a new list clears the selection; the sort and the filter stay.
    if (p.live.generation !== generation) {
      generation = p.live.generation;
      selected = null;
    }
    // The list's own status (setLoading), then the follow's (updateSyncStatus), whichever moved.
    const listStatus = p.live.status ? JSON.stringify(p.live.status) : "";
    let text: string | null = null;
    if (listStatus !== lastListStatus) {
      lastListStatus = listStatus;
      if (p.live.status) text = onlineText(p.live.status);
    }
    const s = p.status;
    if (first) {
      // A status from before the window existed is not shown (the dialog starts with " ").
      lastStatusSeq = s?.seq ?? null;
    } else if (s && s.seq !== lastStatusSeq) {
      lastStatusSeq = s.seq;
      if (activeUrl === "" || s.url === activeUrl) text = onlineText(s.text);
    }
    if (text !== null) status.textContent = text || " ";
    drawRows();
  };

  void self.listen<LivePayload>(LIVE_DATA_EVENT, (e) => render(e.payload)).then(() => send({ type: "ready" }));
}
