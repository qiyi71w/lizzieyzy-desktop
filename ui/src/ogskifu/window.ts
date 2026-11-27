// OGS 棋谱 window (Java OgsKifuDownload / OgsKifuBrowser): the username / player ID row with 搜索 and
// 打开棋谱后此窗口, the player line, 近期搜索, the game table (日期, 黑, 白, 结果, 棋盘大小, 打开) or,
// while it has no rows, what the search came to, the page buttons with the status line, and the
// progress card. The layout is the 野狐棋谱 and 腾讯棋谱 windows' (foxkifu.css). The core owns the
// search, the list and every busy state; this window keeps only the field draft and the selected
// row. The table body is one Tab stop: ↑/↓ and Home/End move the selection, Enter opens the
// selected game, as a double click and 打开 do.
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { dispatch, isApiError } from "../contract/client";
import { errorText } from "../errorText";
import type { Command, KifuAfterGet, KifuPage, Language, OgsKifuCommand, OgsKifuRow, OgsKifuStatus, OgsKifuView } from "../contract/generated";
import { language, product, setLanguage, t } from "../i18n";
import { OGS_KIFU_DATA_EVENT, OGS_KIFU_INPUT_EVENT, type OgsKifuInput, type OgsKifuPayload, type OgsKifuReport } from "./model";
import { icon } from "../design/icons";
import { applyTypeSizes } from "../design/theme";
import "../design/window";
import "../design/stage.css";
import { fitHeaderColumns, KIFU_CELL_PADDING, kifuEmpty, kifuLocator, kifuProbe, probeGate, type KifuOutcome } from "../foxkifu/window";
import "../foxkifu/foxkifu.css";
import "./ogskifu.css";
import "./text";

const AFTER_GET: KifuAfterGet[] = ["Minimize", "Close", "Keep"];
const LOCALES: Record<Exclude<Language, "System">, string> = {
  ZhCn: "zh-CN",
  ZhTw: "zh-TW",
  EnUs: "en-US",
  JaJp: "ja-JP",
  KoKr: "ko-KR",
  ThTh: "th-TH",
};

/** 时间 and 结果: the columns sized to their content. */
const DATA_COLUMNS = [0, 3];
/** 棋盘大小 and 打开 in type sizes, each at least as wide as its header; 0 marks the columns sized
 *  to their content and the two player columns. */
const COLUMN_WIDTHS = [0, 0, 0, 0, 7.5, 5];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

/** Writes a text only when it changed (a rewrite under the pointer can cost WebKit a click). */
function setText(e: HTMLElement, text: string): void {
  if (e.textContent !== text) e.textContent = text;
}

/** `displayDate`: the end time in the interface language's short date and time, local time; the
 *  text as OGS wrote it when it is not a time. */
function displayDate(value: string): string {
  const time = Date.parse(value);
  if (Number.isNaN(time)) return value;
  const lang = language();
  const locale = lang === "System" ? undefined : LOCALES[lang];
  return new Intl.DateTimeFormat(locale, { dateStyle: "short", timeStyle: "short" }).format(time);
}

function cells(r: OgsKifuRow): string[] {
  return [displayDate(r.ended), r.black, r.white, r.result, r.size];
}

/** The status line's text. */
function statusText(s: OgsKifuStatus): string {
  switch (s.type) {
    case "NeedPlayer":
      return t("OgsKifuDownload.player");
    case "Downloading":
      return t("KifuLoad.sharedDownloading");
    case "Opening":
      return t("KifuLoad.parsing");
    case "NoPlayer":
      return t("OgsKifuDownload.noPlayer");
    case "Empty":
      return t("OgsKifuDownload.empty");
    case "Failed":
      if (s.failure === "RateLimited") return product(s.record ? "ogsRecordRateLimited" : "ogsRateLimited");
      return s.record ? t("OgsKifuImport.failed") : t("OgsKifuDownload.failed");
  }
}

export function bootOgsKifu(): void {
  const self = getCurrentWebviewWindow();
  const emit = (input: OgsKifuInput) => void emitTo("main", OGS_KIFU_INPUT_EVENT, input);

  let v: OgsKifuView | null = null;
  let shown = -1;
  let opened = -1;
  let selected: number | null = null;

  // ---------------------------------------------------------------- layout
  const north = el("div", "fk-north");
  const searchRow = el("div", "fk-row");
  const queryLabel = el("label");
  const query = el("input", "fk-account");
  query.type = "text";
  query.id = "ogs-query";
  queryLabel.htmlFor = query.id;
  const searchButton = el("button", "primary");
  searchButton.dataset.key = "ogs.search";
  const afterLabel = el("label");
  const after = el("select", "fk-after");
  after.id = "ogs-after";
  afterLabel.htmlFor = after.id;
  const afterOptions = AFTER_GET.map(() => el("option"));
  after.append(...afterOptions);
  const searchPair = el("span", "fk-pair");
  searchPair.append(queryLabel, query, searchButton);
  const afterPair = el("span", "fk-pair");
  afterPair.append(afterLabel, after);
  searchRow.append(searchPair, afterPair);
  const player = el("div", "fk-current");
  player.dataset.key = "ogs.player";
  const recentRow = el("div", "fk-recent");
  const recentLabel = el("span", "fk-recent-label");
  const recentButtons = el("div", "fk-recent-list");
  const clearRecent = el("button", "fk-clear");
  clearRecent.dataset.key = "ogs.clearRecent";
  recentRow.append(recentLabel, recentButtons, clearRecent);
  north.append(searchRow, player, recentRow);

  const scroll = el("div", "fk-scroll");
  const table = el("table", "fk-table ogs-table");
  const columns = el("colgroup");
  // 时间 and 结果 fit their widest cell, 棋盘大小 and 打开 their labels (fitDataColumns); the
  // players share the rest.
  for (let k = 0; k < COLUMN_WIDTHS.length; k++) columns.append(el("col"));
  const head = el("thead");
  const headRow = el("tr");
  head.append(headRow);
  const body = el("tbody");
  body.tabIndex = 0;
  body.dataset.key = "ogs.table";
  table.append(columns, head, body);
  const empty = kifuEmpty();
  scroll.append(table, empty.el);

  const south = el("div", "fk-south");
  const pageButtons: [string, KifuPage, "step-first" | "step-back" | "step-forward" | "step-last"][] = [
    ["|<", "First", "step-first"],
    ["<", "Previous", "step-back"],
    [">", "Next", "step-forward"],
    [">|", "Last", "step-last"],
  ];
  const pagers = pageButtons.map(([text, to, glyph]) => {
    // The icon shows; the Java text stays as the button's (unseen) text.
    const b = el("button", "fk-page");
    b.append(icon(glyph), el("span", "fk-unseen", text));
    b.dataset.key = `ogs.page.${to}`;
    b.onclick = () => void command({ type: "Page", to });
    south.append(b);
    return [to, b] as const;
  });
  const pageLabel = el("span", "fk-page-label", "1/1");
  south.append(pageLabel);
  const status = el("div", "ogs-status");
  status.dataset.key = "ogs.status";
  status.setAttribute("role", "status");
  const southBox = el("div", "ogs-south");
  southBox.append(south, status);

  const overlay = el("div", "fk-overlay");
  overlay.hidden = true;
  const card = el("div", "fk-card");
  const overlayText = el("div", "fk-card-text");
  const bar = el("div", "fk-bar");
  bar.append(el("span", "fk-bar-run"));
  card.append(overlayText, bar);
  overlay.append(card);
  overlay.setAttribute("role", "progressbar");

  const root = el("div", "fk-root");
  root.append(north, scroll, southBox, overlay);
  document.body.append(root);

  // ---------------------------------------------------------------- commands
  async function command(op: OgsKifuCommand): Promise<void> {
    const cmd: Command = { type: "OgsKifu", op };
    try {
      await dispatch(cmd);
    } catch (e) {
      if (isApiError(e)) setText(status, errorText(e));
    }
  }

  const startSearch = (text: string) => void command({ type: "Search", text });
  searchButton.onclick = () => startSearch(query.value);
  query.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      startSearch(query.value);
    }
  });
  after.onchange = () => void command({ type: "SetAfterGet", action: AFTER_GET[after.selectedIndex] });
  clearRecent.onclick = () => void command({ type: "ClearRecent" });
  const openGame = (id: number | null) => {
    if (id === null || !v || v.busy) return;
    void command({ type: "OpenGame", id });
  };

  function select(id: number | null): void {
    selected = id;
    for (const row of body.children) {
      const on = Number((row as HTMLElement).dataset.id) === id;
      row.classList.toggle("selected", on);
      row.setAttribute("aria-selected", String(on));
      if (on) (row as HTMLElement).scrollIntoView({ block: "nearest" });
    }
    report();
  }

  body.addEventListener("keydown", (e) => {
    if (!v || v.rows.length === 0) return;
    const ids = v.rows.map((r) => r.id);
    const at = selected === null ? -1 : ids.indexOf(selected);
    let to: number | null = null;
    if (e.key === "ArrowDown") to = Math.min(at + 1, ids.length - 1);
    else if (e.key === "ArrowUp") to = Math.max(at - 1, 0);
    else if (e.key === "Home") to = 0;
    else if (e.key === "End") to = ids.length - 1;
    else if (e.key === "Enter") {
      e.preventDefault();
      openGame(selected);
      return;
    } else if (e.key === " ") {
      e.preventDefault();
      if (at < 0) select(ids[0]);
      return;
    }
    if (to === null) return;
    e.preventDefault();
    select(ids[to]);
  });

  // ---------------------------------------------------------------- render
  function renderTexts(): void {
    setText(queryLabel, t("OgsKifuDownload.player"));
    setText(searchButton, t("TencentKifuDownload.btnSearch"));
    setText(afterLabel, t("TencentKifuDownload.lblAfterGet"));
    setText(afterOptions[0], t("TencentKifuDownload.cbxAfterGet.min"));
    setText(afterOptions[1], t("TencentKifuDownload.cbxAfterGet.close"));
    setText(afterOptions[2], t("TencentKifuDownload.cbxAfterGet.none"));
    setText(recentLabel, t("TencentKifuDownload.recentSearches"));
    setText(clearRecent, t("TencentKifuDownload.clearRecent"));
    body.setAttribute("aria-label", t("Menu.ogsKifu"));
    document.title = t("Menu.ogsKifu");
    const headers = [
      t("TencentKifuDownload.column.time"),
      t("TencentKifuDownload.column.black"),
      t("TencentKifuDownload.column.white"),
      t("TencentKifuDownload.column.result"),
      t("OgsKifuDownload.boardSize"),
      t("TencentKifuDownload.column.open"),
    ];
    if ([...headRow.children].map((c) => c.textContent).join("\n") !== headers.join("\n")) {
      headRow.replaceChildren(...headers.map((text) => el("th", "", text)));
    }
  }

  function renderRows(view: OgsKifuView): void {
    body.replaceChildren();
    for (const r of view.rows) {
      const tr = el("tr");
      tr.dataset.id = String(r.id);
      tr.setAttribute("aria-selected", String(r.id === selected));
      if (r.id === selected) tr.classList.add("selected");
      for (const text of cells(r)) {
        const td = el("td", "", text);
        td.title = text;
        tr.append(td);
      }
      const openCell = el("td", "fk-open-cell");
      const open = el("button", "fk-open", t("TencentKifuDownload.column.open"));
      open.dataset.key = `ogs.open.${r.id}`;
      // The table body's Enter opens the selected game; the buttons stay out of the Tab order.
      open.tabIndex = -1;
      open.disabled = view.busy;
      open.onclick = (e) => {
        e.stopPropagation();
        select(r.id);
        openGame(r.id);
      };
      openCell.append(open);
      tr.append(openCell);
      tr.onclick = () => {
        if (!view.busy) select(r.id);
      };
      tr.ondblclick = () => openGame(r.id);
      body.append(tr);
    }
  }

  /** 时间 and 结果 are data and show in full: each column takes its widest text (header and
   *  cells, the cell padding added); 棋盘大小 and 打开 take their type sizes or their header,
   *  whichever is wider. The table's least width is those four and each player column's five type
   *  sizes (four characters and the ellipsis); a narrower window scrolls sideways. */
  function fitDataColumns(): void {
    const range = document.createRange();
    const textWidth = (cell: Element) => {
      range.selectNodeContents(cell);
      return range.getBoundingClientRect().width;
    };
    let fitted = 0;
    for (const k of DATA_COLUMNS) {
      const texts = [headRow.children[k], ...[...body.rows].map((row) => row.cells[k])].filter(Boolean);
      const width = Math.ceil(Math.max(...texts.map(textWidth))) + KIFU_CELL_PADDING;
      (columns.children[k] as HTMLElement).style.width = `${width}px`;
      fitted += width;
    }
    const fixed = fitHeaderColumns(columns, headRow, COLUMN_WIDTHS);
    table.style.minWidth = `calc(var(--ds-size) * ${2 * 5} + ${fixed} + ${fitted + 2 * KIFU_CELL_PADDING}px)`;
  }

  function render(view: OgsKifuView): void {
    v = view;
    renderTexts();
    setText(
      player,
      view.label.type === "Hint" ? t("OgsKifuDownload.hint") : view.label.type === "Searched" ? view.label.text : `${view.label.name} (${view.label.id})`,
    );
    after.selectedIndex = AFTER_GET.indexOf(view.after_get);
    // OgsKifuBrowser.updateControls: a search or a download holds the search controls and
    // 打开棋谱后此窗口 (the 野狐 and 腾讯 windows never hold it, as in Java).
    for (const c of [query, searchButton, after, clearRecent]) c.disabled = view.busy;
    clearRecent.disabled ||= view.recent.length === 0;
    const recentKey = view.recent.join("\n");
    if (recentButtons.dataset.items !== recentKey || recentButtons.childElementCount === 0) {
      recentButtons.dataset.items = recentKey;
      recentButtons.replaceChildren();
      if (view.recent.length === 0) recentButtons.append(el("span", "fk-recent-empty", t("TencentKifuDownload.recentEmpty")));
      for (const text of view.recent) {
        const b = el("button", "fk-recent-btn", text);
        b.dataset.key = `ogs.recent.${text}`;
        b.onclick = () => {
          query.value = text;
          startSearch(text);
        };
        recentButtons.append(b);
      }
    }
    for (const b of recentButtons.querySelectorAll("button")) b.disabled = view.busy;
    // A newly shown page selects its first game (`setRowSelectionInterval(0, 0)`).
    if (view.shown !== shown) {
      shown = view.shown;
      selected = view.rows[0]?.id ?? null;
    } else if (selected !== null && !view.rows.some((r) => r.id === selected)) {
      selected = null;
    }
    renderRows(view);
    fitDataColumns();
    table.classList.toggle("disabled", view.busy);
    for (const [to, b] of pagers) {
      const back = to === "First" || to === "Previous";
      b.disabled = view.busy || !view.listed || (back ? view.page <= 1 : view.page >= view.pages);
    }
    setText(pageLabel, `${view.page}/${Math.max(1, view.pages)}`);

    const busyText = view.busy && view.status && (view.status.type === "Downloading" || view.status.type === "Opening") ? statusText(view.status) : view.busy ? t("KifuLoad.sharedDownloading") : null;
    overlay.hidden = busyText === null;
    setText(overlayText, busyText ?? "");

    // The empty table says what the search came to; the status line carries what the rows do not.
    let outcome: KifuOutcome | null = null;
    const s = view.status;
    if (view.rows.length === 0 && !view.busy) {
      if (s?.type === "NoPlayer" || s?.type === "Empty") outcome = { kind: "told", state: "none", text: statusText(s) };
      else if (s?.type === "Failed") outcome = { kind: "told", state: "failed", text: statusText(s) };
      else outcome = { kind: "told", state: "idle", text: product("ogsIdleText", t("TencentKifuDownload.btnSearch")) };
    }
    empty.show(outcome, searchButton.textContent ?? "");
    const line = s && !view.busy && (s.type === "NeedPlayer" || (s.type === "Failed" && view.rows.length > 0)) ? statusText(s) : "";
    setText(status, line);

    if (opened !== -1 && view.opened > opened) {
      // 打开棋谱后此窗口, then the main window comes forward.
      void invoke("ogs_kifu_opened", { minimize: view.after_get === "Minimize", close: view.after_get === "Close" });
    }
    opened = view.opened;
    report();
  }

  // ---------------------------------------------------------------- report
  // Acceptance builds only (probeGate): ordinary builds never measure the window or send this.
  const probe = probeGate(() => report());
  let lastReport = "";
  let sent = 0;
  function report(): void {
    if (!v || !probe.on) return;
    const targets = [...document.querySelectorAll<HTMLElement>("button[data-key], select, input, tbody")].map((e) => {
      const r = e.getBoundingClientRect();
      return {
        key: e.dataset.key || e.id || e.className,
        text: e instanceof HTMLSelectElement ? e.value : e instanceof HTMLInputElement ? e.value : e.tagName === "TBODY" ? "" : (e.textContent ?? ""),
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + Math.min(r.height, 26) / 2),
        enabled: !(e as HTMLButtonElement).disabled && !e.closest(".disabled"),
      };
    });
    const box = empty.el.hidden ? null : { title: empty.el.children[0].textContent ?? "", text: empty.el.children[1].textContent ?? "" };
    const r: Omit<OgsKifuReport, "sent"> = {
      query: query.value,
      afterGet: after.selectedIndex,
      player: player.textContent ?? "",
      recent: v.recent.slice(),
      header: [...headRow.children].map((c) => c.textContent ?? ""),
      rows: v.rows.map((row) => ({ cells: cells(row), id: row.id })),
      selected,
      // 时间 and 结果 show in full.
      ...kifuProbe(root, [1, 4]),
      page: pageLabel.textContent ?? "",
      overlay: overlay.hidden ? null : overlayText.textContent,
      status: status.textContent ?? "",
      empty: box,
      busy: v.busy,
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    emit({ type: "report", report: { ...r, sent: ++sent } });
  }
  query.addEventListener("input", () => report());
  document.addEventListener("focusin", () => report());
  window.addEventListener("resize", () => report());
  const locate = kifuLocator(after, afterPair, report, (request) => emit({ type: "located", request }));
  // A face that arrives after the first list changes the texts' widths.
  void document.fonts.ready.then(() => {
    fitDataColumns();
    report();
  });

  // ---------------------------------------------------------------- keys and events
  document.addEventListener("keydown", (e) => {
    // installEscapeToClose: the window closes, cancelling its request.
    if (e.key === "Escape" && !e.defaultPrevented) {
      e.preventDefault();
      void self.close();
    }
  });
  document.addEventListener("contextmenu", (e) => e.preventDefault());

  void self
    .listen<OgsKifuPayload>(OGS_KIFU_DATA_EVENT, (e) => {
      applyTypeSizes(e.payload.frameFontSize);
      setLanguage(e.payload.language);
      render(e.payload.view);
      locate(e.payload.locate);
    })
    .then(() => {
      emit({ type: "ready" });
      query.focus();
    });
}
