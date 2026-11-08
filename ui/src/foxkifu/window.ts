// 野狐棋谱 window (FoxKifuDownload.java): the account row, 当前野狐账号, 近期搜索, the game table
// with its 打开 buttons, the page buttons and the progress overlay. The core owns the search, the
// list and every busy state; this window keeps only the account field and 查询方式 drafts.
import { invoke } from "@tauri-apps/api/core";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { dispatch, isApiError } from "../contract/client";
import type { Command, FoxAfterGet, FoxKifuCommand, FoxKifuPage, FoxKifuRow, FoxKifuView } from "../contract/generated";
import { confirm, dialogOpen, message } from "../dialogs";
import { format, isChinese, setLanguage, t } from "../i18n";
import { FOX_KIFU_DATA_EVENT, FOX_KIFU_INPUT_EVENT, type FoxKifuInput, type FoxKifuPayload, type FoxKifuReport } from "./model";
import "../styles.css";
import "./foxkifu.css";

const AFTER_GET: FoxAfterGet[] = ["Minimize", "Close", "Keep"];

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
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
  const searchButton = el("button", "fk-btn");
  searchButton.dataset.key = "fox.search";
  const afterLabel = el("label", "", "");
  const after = el("select", "fk-after");
  after.id = "fk-after";
  afterLabel.htmlFor = after.id;
  const afterOptions = AFTER_GET.map(() => el("option"));
  after.append(...afterOptions);
  searchRow.append(accountLabel, mode, account, searchButton, afterLabel, after);
  const currentUser = el("div", "fk-current");
  const recentRow = el("div", "fk-recent");
  const recentLabel = el("span", "fk-recent-label");
  const recentButtons = el("div", "fk-recent-list");
  const clearRecent = el("button", "fk-btn fk-clear");
  clearRecent.dataset.key = "fox.clearRecent";
  recentRow.append(recentLabel, recentButtons, clearRecent);
  north.append(searchRow, currentUser, recentRow);

  const scroll = el("div", "fk-scroll");
  const table = el("table", "fk-table");
  const columns = el("colgroup");
  for (const width of [40, 180, 160, 50, 160, 50, 95, 55, 45]) {
    const c = el("col");
    c.style.width = `${width}px`;
    columns.append(c);
  }
  const head = el("thead");
  const headRow = el("tr");
  head.append(headRow);
  const body = el("tbody");
  table.append(columns, head, body);
  scroll.append(table);

  const south = el("div", "fk-south");
  const pageButtons: [string, FoxKifuPage][] = [
    ["|<", "First"],
    ["<", "Previous"],
    [">", "Next"],
    [">|", "Last"],
  ];
  for (const [text, to] of pageButtons) {
    const b = el("button", "fk-btn fk-page", text);
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
  const barText = el("span", "fk-bar-text");
  bar.append(el("span", "fk-bar-run"), barText);
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
      if (isApiError(e)) await show(e.message);
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
      const tr = el("tr", r.won ? "won" : "");
      if (r.chessid === selected) tr.classList.add("selected");
      for (const text of cells(r)) tr.append(el("td", "", text));
      const openCell = el("td", "fk-open-cell");
      const open = el("button", "fk-btn fk-open", t("FoxKifuDownload.column.open"));
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
    return t("FoxKifuDownload.currentUser.label") + display;
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
      const b = el("button", "fk-btn fk-recent-btn", foxUser(r.nickname, r.uid));
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
    barText.textContent = text ?? "";
    // showProgressNotice → presentWindow: progress brings the window back, even when it was closed.
    if (text !== null && (!previous || overlayMessage(previous) !== text)) {
      void self.unminimize().then(() => self.show()).then(() => self.setFocus());
    }
    if (view.notice && view.notice.id !== notice) {
      const first = notice === -1 && !previous;
      notice = view.notice.id;
      if (!first) {
        const n = view.notice;
        void show(coreText(n.message_key) + (n.message_key === "FoxKifuDownload.getKifuFailed" ? n.detail : ""));
      }
    }
    if (opened !== -1 && view.opened > opened) {
      // completeFoxKifuLoad: 打开棋谱后此窗口, then the main window comes forward.
      void invoke("fox_kifu_opened", { minimize: view.after_get === "Minimize", close: view.after_get === "Close" });
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
    const r: FoxKifuReport = {
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
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    emit({ type: "report", report: r });
  }
  account.addEventListener("input", () => report());
  mode.addEventListener("change", () => report());
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
    .listen<FoxKifuPayload>(FOX_KIFU_DATA_EVENT, (e) => {
      setLanguage(e.payload.language);
      render(e.payload.view);
    })
    .then(() => {
      emit({ type: "ready" });
      account.focus();
    });
}
