// 野狐棋谱 window (FoxKifuDownload.java): the account row, 当前野狐账号, 近期搜索, the game table
// with its 打开 buttons (or, while it has no rows, what the search came to), the page buttons and
// the progress overlay. The core owns the search, the list and every busy state; this window keeps
// only the account field and 查询方式 drafts.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { dispatch, isApiError } from "../contract/client";
import { errorText } from "../errorText";
import type { Command, KifuAfterGet, FoxKifuCommand, KifuPage, FoxKifuRow, FoxKifuView } from "../contract/generated";
import { confirm, dialogOpen, message } from "../dialogs";
import { format, isChinese, product, registerProduct, setLanguage, t } from "../i18n";
import { FOX_KIFU_DATA_EVENT, FOX_KIFU_INPUT_EVENT, type FoxKifuInput, type FoxKifuPayload, type FoxKifuReport } from "./model";
import { icon } from "../design/icons";
import { applyTypeSizes } from "../design/theme";
import "../design/window";
import "../design/stage.css";
import { locateControl } from "../search/highlight";
import { pendingLocate } from "../search/locateRules";
import "./foxkifu.css";

const AFTER_GET: KifuAfterGet[] = ["Minimize", "Close", "Keep"];
/** The table's fixed columns in type sizes; 0 marks the two player columns, which share the rest. */
const COLUMN_WIDTHS = [4.5, 12, 0, 4.5, 0, 4.5, 10, 4.5, 5];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

registerProduct({
  ZhCn: { kifuIdle: "尚未查询", kifuIdleText: "输入昵称或 UID 后按“{0}”，棋谱会列在这里。", kifuNone: "没有结果", kifuNoneText: "没有找到“{0}”的棋谱。检查昵称或 UID 后再按“{1}”。", kifuFailed: "查询失败", kifuFailedText: "{0} 按“{1}”重试。" },
  ZhTw: { kifuIdle: "尚未查詢", kifuIdleText: "輸入暱稱或 UID 後按「{0}」，棋譜會列在這裡。", kifuNone: "沒有結果", kifuNoneText: "沒有找到「{0}」的棋譜。檢查暱稱或 UID 後再按「{1}」。", kifuFailed: "查詢失敗", kifuFailedText: "{0} 按「{1}」重試。" },
  EnUs: { kifuIdle: "Not searched yet", kifuIdleText: "Enter a nickname or UID and press {0}; the games are listed here.", kifuNone: "No results", kifuNoneText: "No games found for “{0}”. Check the nickname or UID and press {1} again.", kifuFailed: "Search failed", kifuFailedText: "{0} Press {1} to try again." },
  JaJp: { kifuIdle: "未検索", kifuIdleText: "ニックネームまたは UID を入力して「{0}」を押すと、棋譜がここに表示されます。", kifuNone: "結果なし", kifuNoneText: "「{0}」の棋譜は見つかりませんでした。ニックネームまたは UID を確認して、もう一度「{1}」を押してください。", kifuFailed: "検索に失敗しました", kifuFailedText: "{0} 「{1}」を押すと再試行します。" },
  KoKr: { kifuIdle: "아직 검색하지 않음", kifuIdleText: "닉네임 또는 UID를 입력하고 [{0}]을 누르면 기보가 여기에 표시됩니다.", kifuNone: "결과 없음", kifuNoneText: "“{0}”의 기보를 찾지 못했습니다. 닉네임 또는 UID를 확인하고 [{1}]을 다시 누르세요.", kifuFailed: "검색 실패", kifuFailedText: "{0} [{1}]을 눌러 다시 시도하세요." },
  ThTh: { kifuIdle: "ยังไม่ได้ค้นหา", kifuIdleText: "ป้อนชื่อเล่นหรือ UID แล้วกด “{0}” บันทึกเกมจะแสดงที่นี่", kifuNone: "ไม่มีผลลัพธ์", kifuNoneText: "ไม่พบบันทึกเกมของ “{0}” ตรวจสอบชื่อเล่นหรือ UID แล้วกด “{1}” อีกครั้ง", kifuFailed: "ค้นหาไม่สำเร็จ", kifuFailedText: "{0} กด “{1}” เพื่อลองอีกครั้ง" },
});

/** What the latest search came to while the table has no rows: none yet, no games for the query
 *  shown, or the failure the core reported (Java's message). `told` carries the whole line (OGS 棋谱
 *  shows Java's own messages for each state). */
export type KifuOutcome =
  | { kind: "idle" }
  | { kind: "none"; query: string }
  | { kind: "failed"; reason: string }
  | { kind: "told"; state: "idle" | "none" | "failed"; text: string };

/** The game table's empty state (野狐棋谱, 腾讯棋谱, OGS 棋谱): a title and one line under the
 *  table head saying what to do next; `search` is the window's 搜索 label. `null` hides it (the
 *  table has rows or a search runs). */
export function kifuEmpty(): { el: HTMLElement; show: (outcome: KifuOutcome | null, search: string) => void } {
  const box = el("div", "fk-empty");
  box.dataset.key = "kifu.empty";
  box.setAttribute("role", "status");
  const title = el("div", "fk-empty-title");
  const text = el("div", "fk-empty-text");
  box.append(title, text);
  const titles = { idle: "kifuIdle", none: "kifuNone", failed: "kifuFailed" } as const;
  const show = (outcome: KifuOutcome | null, search: string) => {
    box.hidden = outcome === null;
    if (!outcome) return;
    const state = outcome.kind === "told" ? outcome.state : outcome.kind;
    box.dataset.state = state;
    const heading = product(titles[state]);
    const line = outcome.kind === "told" ? outcome.text
      : outcome.kind === "idle" ? product("kifuIdleText", search)
      : outcome.kind === "none" ? product("kifuNoneText", outcome.query, search)
      : product("kifuFailedText", outcome.reason, search);
    // Written only when changed (a rewrite under the pointer can cost WebKit a click).
    if (title.textContent !== heading) title.textContent = heading;
    if (text.textContent !== line) text.textContent = line;
  };
  return { el: box, show };
}

/** Function search's 打开棋谱后此窗口 in a 棋谱 window: a payload carrying a request (`locate`) puts
 *  the focus and the gold line on the choice once per request, once the choice can take the focus
 *  (a busy window may disable it; the request's next payload tries again), and `located` tells the
 *  main window which request it located. The window's report follows the gold line coming and
 *  going. */
export function kifuLocator(choice: HTMLSelectElement, around: HTMLElement, report: () => void, located: (request: number) => void): (locate: number | null) => void {
  const request = pendingLocate(
    () => !choice.disabled,
    (id) => {
      locateControl(choice, around, report);
      report();
      located(id);
    },
  );
  return (locate) => void request(locate);
}

/** A 棋谱 window's acceptance report goes out only once the acceptance channel has asked for one
 *  (`acceptance-probe`, which only acceptance builds send): until then the window measures and
 *  sends nothing; from then on it reports at once and on every change. */
export function probeGate(report: () => void): { readonly on: boolean } {
  const gate = { on: false };
  void listen("acceptance-probe", () => {
    if (gate.on) return;
    gate.on = true;
    report();
  });
  return gate;
}

/** The horizontal padding of a 棋谱 table cell (foxkifu.css `.fk-table th, .fk-table td`). */
export const KIFU_CELL_PADDING = 16;

/** A 棋谱 table's fixed columns (`widths` in type sizes; 0 for a column that shares the rest) are
 *  never narrower than their header: each takes the larger of its type sizes and the header text
 *  with the cell padding. Returns the fixed columns' total width, as CSS. */
export function fitHeaderColumns(columns: HTMLElement, headRow: HTMLTableRowElement, widths: readonly number[]): string {
  const range = document.createRange();
  return widths
    .flatMap((width, k) => {
      if (!width || !headRow.cells[k]) return [];
      range.selectNodeContents(headRow.cells[k]);
      const css = `max(calc(var(--ds-size) * ${width}), ${Math.ceil(range.getBoundingClientRect().width) + KIFU_CELL_PADDING}px)`;
      (columns.children[k] as HTMLElement).style.width = css;
      return [css];
    })
    .join(" + ");
}

/** What a 棋谱 window's acceptance probe adds to its texts: the focused control, the located one,
 *  the interface size, and what is cut off (across or up and down) or reaches past the window's
 *  edges: the strip's labels, buttons, choices and fields, the recent searches, the page row and
 *  status, the table head, the 打开 buttons, and of the data only the `whole` columns (1-based),
 *  which show in full; the players may end in an ellipsis and the table scrolls sideways. */
export function kifuProbe(root: HTMLElement, whole: readonly number[]): { focus: string; located: string | null; fontSize: string; clipped: string[] } {
  const name = (e: Element) => (e instanceof HTMLElement && (e.dataset.key || e.id)) || e.tagName.toLowerCase();
  const cells = whole.map((n) => `td:nth-child(${n})`).join(", ");
  const clipped = [...root.querySelectorAll<HTMLElement>(`.fk-north :is(label, button, select, input), .tk-hint, .fk-current, .fk-recent-label, .fk-recent-btn, .fk-clear, .fk-south *:not(.fk-unseen), .ogs-status, th, .fk-open, ${cells}`)]
    .filter((e) => e.getClientRects().length > 0)
    .filter((e) => {
      const r = e.getBoundingClientRect();
      const cut = !(e instanceof HTMLInputElement) && (e.scrollWidth > e.clientWidth + 1 || e.scrollHeight > e.clientHeight + 1);
      return cut || (!e.closest(".fk-scroll") && (r.right > window.innerWidth + 0.5 || r.bottom > window.innerHeight + 0.5));
    })
    .map((e) => `${name(e)}:${e.textContent ?? ""}`);
  const located = document.querySelector(".located");
  return {
    focus: document.activeElement ? name(document.activeElement) : "",
    located: located ? name(located.querySelector("select, input") ?? located) : null,
    fontSize: getComputedStyle(document.body).fontSize,
    clipped,
  };
}

/** FoxKifuDownload.formatFoxUser: the nickname with the UID in brackets. */
function foxUser(nickname: string, uid: string): string {
  const name = nickname.trim();
  const id = uid.trim();
  if (!name) return id || t("FoxKifuDownload.unknownNick");
  if (!id || name === id) return name;
  return `${name} (${id})`;
}

/** A listed player: a game without a nickname shows 未知昵称 (resolveFoxName). */
function player(nickname: string | null, uid: string): string {
  return foxUser(nickname ?? t("FoxKifuDownload.unknownNick"), uid);
}

/** formatFoxRank: 100..108 are professional 1..9 dan, apart from the online ranks. */
function foxRank(raw: number): string {
  if (raw >= 100) return t("FoxKifuDownload.rank.professionalPrefix") + (raw - 99) + t("FoxKifuDownload.rank.dan");
  const rank = raw - 17;
  return rank > 0 ? rank + t("FoxKifuDownload.rank.dan") : Math.abs(rank) + 1 + t("FoxKifuDownload.rank.kyu");
}

/** buildResultText; the score is `DecimalFormat("0.##")` of hundredths. */
function foxResult(r: FoxKifuRow): string {
  if (r.winner !== 1 && r.winner !== 2) return t("FoxKifuDownload.other");
  let text = r.winner === 1 ? t("FoxKifuDownload.black") : t("FoxKifuDownload.white");
  if (r.point < 0) {
    if (r.point === -1) return text + t("FoxKifuDownload.winByRes");
    if (r.point === -2) return text + t("FoxKifuDownload.winByTime");
    return text + t("FoxKifuDownload.win");
  }
  const unit = r.rule === 1 ? t("FoxKifuDownload.stones") : r.rule === 0 ? t("FoxKifuDownload.points") : "";
  const score = String(Number((r.point / 100).toFixed(2)));
  text += isChinese() ? t("FoxKifuDownload.win") + score + unit : ` +${score}${unit}`;
  return text;
}

/** The refusals and notices the core sends (keys written out so the string table carries them). */
function coreText(key: string): string {
  const texts: Record<string, string> = {
    "FoxKifuDownload.noUser": t("FoxKifuDownload.noUser"),
    "FoxKifuDownload.noUid": t("FoxKifuDownload.noUid"),
    "FoxKifuDownload.invalidUidInput": t("FoxKifuDownload.invalidUidInput"),
    "FoxKifuDownload.invalidUid": t("FoxKifuDownload.invalidUid"),
    "FoxKifuDownload.waitLastSearch": t("FoxKifuDownload.waitLastSearch"),
    "FoxKifuDownload.noMoreKifu": t("FoxKifuDownload.noMoreKifu"),
    "FoxKifuDownload.noKifu": t("FoxKifuDownload.noKifu"),
    "FoxKifuDownload.getKifuFailed": t("FoxKifuDownload.getKifuFailed"),
    "KifuLoad.wait": t("KifuLoad.wait"),
  };
  return texts[key] ?? t(key);
}

function cells(r: FoxKifuRow): string[] {
  return [
    String(r.index),
    r.time,
    player(r.black_name, r.black_uid),
    foxRank(r.black_rank),
    player(r.white_name, r.white_uid),
    foxRank(r.white_rank),
    foxResult(r),
    String(r.moves),
  ];
}

export function bootFoxKifu(): void {
  const self = getCurrentWebviewWindow();
  const emit = (input: FoxKifuInput) => void emitTo("main", FOX_KIFU_INPUT_EVENT, input);

  let v: FoxKifuView | null = null;
  let search = -1;
  let notice = -1;
  let opened = -1;
  let shownMessage: string | null = null;
  let outcome: KifuOutcome = { kind: "idle" };

  // ---------------------------------------------------------------- layout
  const north = el("div", "fk-north");
  const searchRow = el("div", "fk-row");
  const accountLabel = el("label", "", "");
  const mode = el("select", "fk-mode");
  const nicknameOption = el("option", "", "");
  const uidOption = el("option", "", "");
  mode.append(nicknameOption, uidOption);
  const account = el("input", "fk-account");
  account.type = "text";
  account.id = "fk-account";
  accountLabel.htmlFor = account.id;
  const searchButton = el("button", "primary");
  searchButton.dataset.key = "fox.search";
  const afterLabel = el("label", "", "");
  const after = el("select", "fk-after");
  after.id = "fk-after";
  afterLabel.htmlFor = after.id;
  const afterOptions = AFTER_GET.map(() => el("option"));
  after.append(...afterOptions);
  const searchPair = el("span", "fk-pair");
  searchPair.append(accountLabel, mode, account, searchButton);
  const afterPair = el("span", "fk-pair");
  afterPair.append(afterLabel, after);
  searchRow.append(searchPair, afterPair);
  const currentUser = el("div", "fk-current");
  const recentRow = el("div", "fk-recent");
  const recentLabel = el("span", "fk-recent-label");
  const recentButtons = el("div", "fk-recent-list");
  const clearRecent = el("button", "fk-clear");
  clearRecent.dataset.key = "fox.clearRecent";
  recentRow.append(recentLabel, recentButtons, clearRecent);
  north.append(searchRow, currentUser, recentRow);

  const scroll = el("div", "fk-scroll");
  const table = el("table", "fk-table");
  const columns = el("colgroup");
  for (let k = 0; k < COLUMN_WIDTHS.length; k++) columns.append(el("col"));
  const head = el("thead");
  const headRow = el("tr");
  /** Fixed columns in multiples of the type size, each at least as wide as its header; the two
   *  player columns share the rest, at least seven type sizes each (a narrower window scrolls the
   *  table sideways). */
  function fitColumns(): void {
    table.style.minWidth = `calc(var(--ds-size) * 14 + ${fitHeaderColumns(columns, headRow, COLUMN_WIDTHS)})`;
  }
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
    b.dataset.key = `fox.page.${to}`;
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

  async function command(op: FoxKifuCommand): Promise<void> {
    const cmd: Command = { type: "FoxKifu", op };
    try {
      const result = await dispatch(cmd);
      if (result.type === "Refused") await show(coreText(result.message_key));
    } catch (e) {
      if (isApiError(e)) await show(errorText(e));
    }
  }

  const startSearch = (text: string, byUid: boolean, hint: string) =>
    void command({ type: "Search", text, by_uid: byUid, nickname_hint: hint });
  searchButton.onclick = () => startSearch(account.value, mode.selectedIndex === 1, "");
  account.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      startSearch(account.value, mode.selectedIndex === 1, "");
    }
  });
  after.onchange = () => void command({ type: "SetAfterGet", action: AFTER_GET[after.selectedIndex] });
  clearRecent.onclick = async () => {
    if (!v || v.recent.length === 0) return;
    if (await confirm(t("FoxKifuDownload.clearRecentTitle"), t("FoxKifuDownload.clearRecentConfirm"))) {
      void command({ type: "ClearRecent" });
    }
  };
  const openGame = (chessid: string) => {
    if (!v || v.loading) return;
    void command({ type: "OpenGame", chessid });
  };

  // ---------------------------------------------------------------- render
  function renderTexts(): void {
    accountLabel.textContent = t("FoxKifuDownload.lblUserName");
    mode.title = t("FoxKifuDownload.queryMode");
    mode.setAttribute("aria-label", t("FoxKifuDownload.queryMode"));
    nicknameOption.textContent = t("FoxKifuDownload.queryMode.nickname");
    uidOption.textContent = t("FoxKifuDownload.queryMode.uid");
    searchButton.textContent = t("FoxKifuDownload.btnSearch");
    afterLabel.textContent = t("FoxKifuDownload.lblAfterGet");
    afterOptions[0].textContent = t("FoxKifuDownload.cbxAfterGet.min");
    afterOptions[1].textContent = t("FoxKifuDownload.cbxAfterGet.close");
    afterOptions[2].textContent = t("FoxKifuDownload.cbxAfterGet.none");
    recentLabel.textContent = t("FoxKifuDownload.recentSearches");
    clearRecent.textContent = t("FoxKifuDownload.clearRecent");
    document.title = t("FoxKifuDownload.title");
  }

  function renderHeader(): void {
    headRow.replaceChildren(
      ...[
        t("FoxKifuDownload.column.index"),
        t("FoxKifuDownload.column.time"),
        t("FoxKifuDownload.column.black"),
        t("FoxKifuDownload.column.rank"),
        t("FoxKifuDownload.column.white"),
        t("FoxKifuDownload.column.rank"),
        t("FoxKifuDownload.column.result"),
        t("FoxKifuDownload.column.moves"),
        t("FoxKifuDownload.column.open"),
      ].map((text) => el("th", "", text)),
    );
  }

  let selected: string | null = null;
  function renderRows(view: FoxKifuView): void {
    body.replaceChildren();
    for (const r of view.rows) {
      const tr = el("tr", r.won ? "won" : r.winner === 1 || r.winner === 2 ? "lost" : "");
      if (r.chessid === selected) tr.classList.add("selected");
      cells(r).forEach((text, column) => tr.append(el("td", column === 6 ? "fk-result" : "", text)));
      const openCell = el("td", "fk-open-cell");
      const open = el("button", "fk-open", t("FoxKifuDownload.column.open"));
      open.dataset.key = `fox.open.${r.chessid}`;
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

  function userText(view: FoxKifuView): string {
    const name = view.nickname.trim();
    const uid = view.uid.trim();
    const display = !name && !uid ? t("FoxKifuDownload.currentUser.waiting") : !uid ? name : !name ? uid : foxUser(name, uid);
    // Java joins label and name directly; a half-width colon needs the space a full-width one includes.
    const label = t("FoxKifuDownload.currentUser.label");
    return label + (label.endsWith(":") ? " " : "") + display;
  }

  function overlayMessage(view: FoxKifuView): string | null {
    const busy = view.busy;
    if (!busy) return null;
    if (busy.type === "Searching") return format(t("KifuLoad.foxSearching"), busy.text);
    if (busy.type === "Downloading") return t("KifuLoad.foxDownloading");
    return t("KifuLoad.parsing");
  }

  function render(view: FoxKifuView): void {
    const previous = v;
    v = view;
    renderTexts();
    renderHeader();
    fitColumns();
    // A new search (and the first state) shows the remembered account and 查询方式.
    if (view.search !== search) {
      search = view.search;
      account.value = view.last_name;
      mode.selectedIndex = view.last_query_by_uid ? 1 : 0;
    }
    after.selectedIndex = AFTER_GET.indexOf(view.after_get);
    currentUser.textContent = userText(view);
    recentButtons.replaceChildren();
    if (view.recent.length === 0) recentButtons.append(el("span", "fk-recent-empty", t("FoxKifuDownload.recentEmpty")));
    for (const r of view.recent) {
      const b = el("button", "fk-recent-btn", foxUser(r.nickname, r.uid));
      b.dataset.key = `fox.recent.${r.uid || r.nickname}`;
      // A saved UID pins the account; only UID-less entries resolve the nickname again.
      b.onclick = () => (r.uid ? startSearch(r.uid, true, r.nickname) : startSearch(r.nickname, false, ""));
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
      const text = coreText(n.message_key) + (n.message_key === "FoxKifuDownload.getKifuFailed" ? n.detail : "");
      if (n.message_key === "FoxKifuDownload.noKifu") outcome = { kind: "none", query: view.last_name };
      else if (n.message_key === "FoxKifuDownload.getKifuFailed" || n.message_key === "FoxKifuDownload.invalidUid") outcome = { kind: "failed", reason: text };
      if (!first) void show(text);
    }
    empty.show(view.rows.length === 0 && !view.busy ? outcome : null, searchButton.textContent ?? "");
    if (opened !== -1 && view.opened > opened) {
      // completeFoxKifuLoad: 打开棋谱后此窗口, then the main window comes forward.
      void invoke("fox_kifu_opened", { minimize: view.after_get === "Minimize", close: view.after_get === "Close" });
    }
    opened = view.opened;
    report();
  }

  // ---------------------------------------------------------------- report
  const probe = probeGate(() => report());
  let lastReport = "";
  let sent = 0;
  function report(): void {
    if (!v || !probe.on) return;
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
    const r: Omit<FoxKifuReport, "sent"> = {
      account: account.value,
      byUid: mode.selectedIndex === 1,
      afterGet: after.selectedIndex,
      currentUser: currentUser.textContent ?? "",
      recent: [...recentButtons.children].map((c) => c.textContent ?? ""),
      header: [...headRow.children].map((c) => c.textContent ?? ""),
      rows: v.rows.map((row) => ({ cells: cells(row), won: row.won, chessid: row.chessid })),
      page: pageLabel.textContent ?? "",
      overlay: overlay.hidden ? null : overlayText.textContent,
      tableEnabled: !v.loading,
      message: shownMessage,
      // 时间 and 结果 show in full.
      ...kifuProbe(root, [2, 7]),
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    emit({ type: "report", report: { ...r, sent: ++sent } });
  }
  account.addEventListener("input", () => report());
  mode.addEventListener("change", () => report());
  window.addEventListener("resize", () => report());
  document.addEventListener("focusin", () => report());
  const locate = kifuLocator(after, afterPair, report, (request) => emit({ type: "located", request }));
  // A face that arrives after the first list changes the headers' widths.
  void document.fonts.ready.then(() => {
    if (!v) return;
    fitColumns();
    report();
  });

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
    .listen<FoxKifuPayload>(FOX_KIFU_DATA_EVENT, (e) => {
      applyTypeSizes(e.payload.frameFontSize);
      setLanguage(e.payload.language);
      render(e.payload.view);
      locate(e.payload.locate);
    })
    .then(() => {
      emit({ type: "ready" });
      account.focus();
    });
}
