// 腾讯棋谱 window (TencentKifuDownload.java): the nickname / UID row, 当前腾讯查询, 近期搜索, the
// game table with its 打开 buttons (or, while it has no rows, what the search came to), the page
// buttons and the progress overlay. The layout is 野狐棋谱's (same Java frame and measures) without
// its 查询方式, with the grey input hint. The core owns the search, the list and every busy state;
// this window keeps only the field draft.
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { dispatch, isApiError } from "../contract/client";
import { errorText } from "../errorText";
import type { Command, KifuAfterGet, KifuPage, TencentKifuCommand, TencentKifuRow, TencentKifuView } from "../contract/generated";
import { confirm, dialogOpen, message } from "../dialogs";
import { isChinese, setLanguage, t } from "../i18n";
import {
  TENCENT_KIFU_DATA_EVENT,
  TENCENT_KIFU_INPUT_EVENT,
  type TencentKifuInput,
  type TencentKifuPayload,
  type TencentKifuReport,
} from "./model";
import { icon } from "../design/icons";
import "../design/window";
import "../design/stage.css";
import { kifuEmpty, type KifuOutcome } from "../foxkifu/window";
import "../foxkifu/foxkifu.css";
import "./tencentkifu.css";

const AFTER_GET: KifuAfterGet[] = ["Minimize", "Close", "Keep"];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

/** formatTencentUser: the name (未知昵称 when empty) with a different UID in brackets. */
function tencentUser(name: string, uid: string): string {
  const shown = name.trim() || t("TencentKifuDownload.unknownNick");
  const id = uid.trim();
  if (!id || shown === id) return shown;
  return `${shown} (${id})`;
}

/** formatTencentRank: no rank for 0; 100.. are professional P1.. dan, apart from the online
 *  ranks around 17 (1 级 / 1 段). */
function tencentRank(raw: number): string {
  if (raw <= 0) return "";
  if (raw >= 100) return `P${raw - 99}${t("TencentKifuDownload.rank.dan")}`;
  const rank = raw - 17;
  return rank > 0 ? rank + t("TencentKifuDownload.rank.dan") : Math.abs(rank) + 1 + t("TencentKifuDownload.rank.kyu");
}

/** buildResultText; the score is `DecimalFormat("0.##")` of hundredths. */
function tencentResult(r: TencentKifuRow): string {
  if (r.winner !== 1 && r.winner !== 2) return t("TencentKifuDownload.other");
  const text = r.winner === 1 ? t("TencentKifuDownload.black") : t("TencentKifuDownload.white");
  if (r.point < 0) {
    if (r.point === -1) return text + t("TencentKifuDownload.winByRes");
    if (r.point === -2) return text + t("TencentKifuDownload.winByTime");
    return text + t("TencentKifuDownload.win");
  }
  const unit = r.rule === 1 ? t("TencentKifuDownload.stones") : t("TencentKifuDownload.points");
  const score = String(Number((r.point / 100).toFixed(2)));
  return text + (isChinese() ? t("TencentKifuDownload.win") + score + unit : ` +${score}${unit}`);
}

/** resolvePlayTime: `starttime`, else `gamestarttime` as yyyy-MM-dd HH:mm:ss local time. */
function playTime(r: TencentKifuRow): string {
  if (r.time) return r.time;
  if (r.start_seconds <= 0) return "";
  const d = new Date(r.start_seconds * 1000);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

/** The refusals and notices the core sends (keys written out so the string table carries them). */
function coreText(key: string): string {
  const texts: Record<string, string> = {
    "TencentKifuDownload.noUser": t("TencentKifuDownload.noUser"),
    "TencentKifuDownload.waitLastSearch": t("TencentKifuDownload.waitLastSearch"),
    "TencentKifuDownload.noMoreKifu": t("TencentKifuDownload.noMoreKifu"),
    "TencentKifuDownload.noKifu": t("TencentKifuDownload.noKifu"),
    "TencentKifuDownload.getKifuFailed": t("TencentKifuDownload.getKifuFailed"),
    "KifuLoad.wait": t("KifuLoad.wait"),
  };
  return texts[key] ?? t(key);
}

function cells(r: TencentKifuRow): string[] {
  return [
    String(r.index),
    playTime(r),
    tencentUser(r.black_name ?? "", r.black_uid),
    tencentRank(r.black_rank),
    tencentUser(r.white_name ?? "", r.white_uid),
    tencentRank(r.white_rank),
    tencentResult(r),
    String(r.moves),
  ];
}

export function bootTencentKifu(): void {
  const self = getCurrentWebviewWindow();
  const emit = (input: TencentKifuInput) => void emitTo("main", TENCENT_KIFU_INPUT_EVENT, input);

  let v: TencentKifuView | null = null;
  let search = -1;
  let notice = -1;
  let opened = -1;
  let shownMessage: string | null = null;
  let outcome: KifuOutcome = { kind: "idle" };

  // ---------------------------------------------------------------- layout
  const north = el("div", "fk-north");
  const searchRow = el("div", "fk-row");
  const queryLabel = el("label");
  const query = el("input", "fk-account");
  query.type = "text";
  query.id = "tk-query";
  queryLabel.htmlFor = query.id;
  const searchButton = el("button", "primary");
  searchButton.dataset.key = "tencent.search";
  const hint = el("span", "tk-hint");
  const afterLabel = el("label");
  const after = el("select", "fk-after");
  after.id = "tk-after";
  afterLabel.htmlFor = after.id;
  const afterOptions = AFTER_GET.map(() => el("option"));
  after.append(...afterOptions);
  const searchPair = el("span", "fk-pair");
  searchPair.append(queryLabel, query, searchButton);
  const afterPair = el("span", "fk-pair");
  afterPair.append(afterLabel, after);
  searchRow.append(searchPair, hint, afterPair);
  const currentUser = el("div", "fk-current");
  const recentRow = el("div", "fk-recent");
  const recentLabel = el("span", "fk-recent-label");
  const recentButtons = el("div", "fk-recent-list");
  const clearRecent = el("button", "fk-clear");
  clearRecent.dataset.key = "tencent.clearRecent";
  recentRow.append(recentLabel, recentButtons, clearRecent);
  north.append(searchRow, currentUser, recentRow);

  const scroll = el("div", "fk-scroll");
  const table = el("table", "fk-table");
  const columns = el("colgroup");
  // Fixed columns in multiples of the type size; the two player columns share the rest.
  for (const width of [4.5, 12, 0, 4.5, 0, 4.5, 10, 4.5, 5]) {
    const c = el("col");
    if (width) c.style.width = `calc(var(--ds-size) * ${width})`;
    columns.append(c);
  }
  const head = el("thead");
  const headRow = el("tr");
  head.append(headRow);
  const body = el("tbody");
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
  for (const [text, to, glyph] of pageButtons) {
    // The icon shows; the Java text stays as the button's (unseen) text.
    const b = el("button", "fk-page");
    b.append(icon(glyph), el("span", "fk-unseen", text));
    b.dataset.key = `tencent.page.${to}`;
    b.onclick = () => void command({ type: "Page", to });
    south.append(b);
  }
  const pageLabel = el("span", "fk-page-label", "1/1");
  south.append(pageLabel);

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
  root.append(north, scroll, south, overlay);
  const dialogHost = el("div");
  dialogHost.id = "dialog";
  dialogHost.hidden = true;
  document.body.append(root, dialogHost);

  // ---------------------------------------------------------------- commands
  const show = async (text: string) => {
    shownMessage = text;
    report();
    await message(t("Message.title"), text);
    shownMessage = null;
    report();
  };

  async function command(op: TencentKifuCommand): Promise<void> {
    const cmd: Command = { type: "TencentKifu", op };
    try {
      const result = await dispatch(cmd);
      if (result.type === "Refused") await show(coreText(result.message_key));
    } catch (e) {
      if (isApiError(e)) await show(errorText(e));
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
  clearRecent.onclick = async () => {
    if (!v || v.recent.length === 0) return;
    if (await confirm(t("TencentKifuDownload.clearRecentTitle"), t("TencentKifuDownload.clearRecentConfirm"))) {
      void command({ type: "ClearRecent" });
    }
  };
  const openGame = (chessid: string) => {
    if (!v || v.loading) return;
    void command({ type: "OpenGame", chessid });
  };

  // ---------------------------------------------------------------- render
  function renderTexts(): void {
    queryLabel.textContent = t("TencentKifuDownload.lblUserName");
    searchButton.textContent = t("TencentKifuDownload.btnSearch");
    hint.textContent = t("TencentKifuDownload.uidOnlyHint");
    afterLabel.textContent = t("TencentKifuDownload.lblAfterGet");
    afterOptions[0].textContent = t("TencentKifuDownload.cbxAfterGet.min");
    afterOptions[1].textContent = t("TencentKifuDownload.cbxAfterGet.close");
    afterOptions[2].textContent = t("TencentKifuDownload.cbxAfterGet.none");
    recentLabel.textContent = t("TencentKifuDownload.recentSearches");
    clearRecent.textContent = t("TencentKifuDownload.clearRecent");
    document.title = t("TencentKifuDownload.title");
  }

  function renderHeader(): void {
    headRow.replaceChildren(
      ...[
        t("TencentKifuDownload.column.index"),
        t("TencentKifuDownload.column.time"),
        t("TencentKifuDownload.column.black"),
        t("TencentKifuDownload.column.rank"),
        t("TencentKifuDownload.column.white"),
        t("TencentKifuDownload.column.rank"),
        t("TencentKifuDownload.column.result"),
        t("TencentKifuDownload.column.moves"),
        t("TencentKifuDownload.column.open"),
      ].map((text) => el("th", "", text)),
    );
  }

  let selected: string | null = null;
  function renderRows(view: TencentKifuView): void {
    body.replaceChildren();
    for (const r of view.rows) {
      const tr = el("tr", r.won ? "won" : r.winner === 1 || r.winner === 2 ? "lost" : "");
      if (r.chessid === selected) tr.classList.add("selected");
      cells(r).forEach((text, column) => tr.append(el("td", column === 6 ? "fk-result" : "", text)));
      const openCell = el("td", "fk-open-cell");
      const open = el("button", "fk-open", t("TencentKifuDownload.column.open"));
      open.dataset.key = `tencent.open.${r.chessid}`;
      open.onclick = (e) => {
        e.stopPropagation();
        openGame(r.chessid);
      };
      openCell.append(open);
      tr.append(openCell);
      tr.onclick = () => {
        if (view.loading) return;
        selected = r.chessid;
        for (const row of body.children) row.classList.toggle("selected", row === tr);
      };
      tr.ondblclick = () => openGame(r.chessid);
      body.append(tr);
    }
  }

  function overlayMessage(view: TencentKifuView): string | null {
    const busy = view.busy;
    if (!busy) return null;
    if (busy.type === "Searching") return t("TencentKifuDownload.searching").replace("%s", busy.text);
    if (busy.type === "LoadingMore") return t("TencentKifuDownload.loadingMore");
    if (busy.type === "Downloading") return t("KifuLoad.tencentDownloading");
    return t("KifuLoad.parsing");
  }

  function render(view: TencentKifuView): void {
    const previous = v;
    v = view;
    renderTexts();
    renderHeader();
    // A new search (and the first state) shows the remembered text.
    if (view.search !== search) {
      search = view.search;
      query.value = view.last_name;
    }
    after.selectedIndex = AFTER_GET.indexOf(view.after_get);
    currentUser.textContent =
      t("TencentKifuDownload.currentUser.label") + (view.query.trim() || t("TencentKifuDownload.currentUser.waiting"));
    recentButtons.replaceChildren();
    if (view.recent.length === 0) recentButtons.append(el("span", "fk-recent-empty", t("TencentKifuDownload.recentEmpty")));
    for (const r of view.recent) {
      const b = el("button", "fk-recent-btn", tencentUser(r.query, r.uid));
      const keyword = r.query.trim() || r.uid.trim();
      b.dataset.key = `tencent.recent.${keyword}`;
      b.onclick = () => {
        query.value = keyword;
        startSearch(keyword);
      };
      recentButtons.append(b);
    }
    renderRows(view);
    table.classList.toggle("disabled", view.loading);
    pageLabel.textContent = `${view.page}/${Math.max(1, view.pages)}${view.complete ? "" : "..."}`;
    const text = overlayMessage(view);
    overlay.hidden = text === null;
    overlayText.textContent = text ?? "";
    // showProgressNotice → presentWindow: progress brings the window back, even when it was closed.
    if (text !== null && (!previous || overlayMessage(previous) !== text)) {
      void self.unminimize().then(() => self.show()).then(() => self.setFocus());
    }
    if (view.notice && view.notice.id !== notice) {
      const first = notice === -1 && !previous;
      notice = view.notice.id;
      const n = view.notice;
      const text = coreText(n.message_key) + (n.message_key === "TencentKifuDownload.getKifuFailed" ? n.detail : "");
      if (n.message_key === "TencentKifuDownload.noKifu") outcome = { kind: "none", query: view.last_name };
      else if (n.message_key === "TencentKifuDownload.getKifuFailed") outcome = { kind: "failed", reason: text };
      if (!first) void show(text);
    }
    empty.show(view.rows.length === 0 && !view.busy ? outcome : null, searchButton.textContent ?? "");
    if (opened !== -1 && view.opened > opened) {
      // completeTencentKifuLoad: 打开棋谱后此窗口, then the main window comes forward.
      void invoke("tencent_kifu_opened", { minimize: view.after_get === "Minimize", close: view.after_get === "Close" });
    }
    opened = view.opened;
    report();
  }

  // ---------------------------------------------------------------- report
  let lastReport = "";
  function report(): void {
    if (!v) return;
    const targets = [...document.querySelectorAll<HTMLElement>("button[data-key], select, input")].map((e) => {
      const r = e.getBoundingClientRect();
      return {
        key: e.dataset.key ?? e.id ?? e.className,
        text: e instanceof HTMLSelectElement ? e.value : e instanceof HTMLInputElement ? e.value : (e.textContent ?? ""),
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + r.height / 2),
        enabled: !(e as HTMLButtonElement).disabled && !e.closest(".disabled"),
      };
    });
    const r: TencentKifuReport = {
      query: query.value,
      afterGet: after.selectedIndex,
      currentUser: currentUser.textContent ?? "",
      recent: [...recentButtons.children].map((c) => c.textContent ?? ""),
      header: [...headRow.children].map((c) => c.textContent ?? ""),
      rows: v.rows.map((row) => ({ cells: cells(row), won: row.won, chessid: row.chessid })),
      page: pageLabel.textContent ?? "",
      overlay: overlay.hidden ? null : overlayText.textContent,
      tableEnabled: !v.loading,
      message: shownMessage,
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    emit({ type: "report", report: r });
  }
  query.addEventListener("input", () => report());
  window.addEventListener("resize", () => report());

  // ---------------------------------------------------------------- keys and events
  document.addEventListener("keydown", (e) => {
    // installEscapeToClose: the frame closes (hidden, its search kept).
    if (e.key === "Escape" && !dialogOpen() && !e.defaultPrevented) {
      e.preventDefault();
      void self.hide();
    }
  });
  document.addEventListener("contextmenu", (e) => e.preventDefault());

  void self
    .listen<TencentKifuPayload>(TENCENT_KIFU_DATA_EVENT, (e) => {
      setLanguage(e.payload.language);
      render(e.payload.view);
    })
    .then(() => {
      emit({ type: "ready" });
      query.focus();
    });
}
