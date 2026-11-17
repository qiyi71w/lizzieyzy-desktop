// Java BottomToolbar main bar: its buttons (BottomToolbar.java:723-884) in their groups
// (5209-5235), placed by layoutMainBar (5237-5320); 自定义内容 (chrome/bottomButtons.ts) decides which
// are shown. Buttons that do not fit go to 更多. Navigation, the move field, the record-editing and
// board display buttons are implemented here; the other buttons report that the capability is not
// available yet. 工具栏 > 底部工具栏 hides the bar or adds the 详细工具栏 panels below it
// (bottomDetail.ts); the mode is the core's window presentation setting.
import { navigate, run, state, subscribe, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { toggleCoordinates, toggleMoveNumbers, toggleMoveRank } from "../display/commands";
import { toggleRawNet } from "../display/state";
import "../editing/commands";
import { product, t } from "../i18n";
import { openAutoSetup } from "../setup/dialog";
import { openRemoteCompute } from "../cloud/center";
import { toggleKataEstimate } from "../analysis/infoCommands";
import { toggleScore } from "../analysis/scoreDialog";
import { toggleFrame } from "../analysis/tableCommands";
import { AUTO_ANALYSIS_MENU } from "../quick/autoAnalysis";
import { LIVE_MENU } from "../online/commands";
import type { MenuNode } from "./javaMenus";
import { locatePopupItem, openPopupMenu } from "./menubar";
import { buildBottomDetail, layoutBottomDetail } from "./bottomDetail";
import { bottomButtonShown, detailButtonShown, type BottomButton } from "./bottomButtons";

interface Item {
  name: string;
  key: string;
  cls?: string;
  run?: () => void;
  /** Its 自定义内容 setting; visibleButtons skips the button while it is off. */
  setting?: BottomButton;
  /** Shown only while this holds as well (reSetButtonLocation); re-laid out when it changes. */
  shown?: () => boolean;
  /** Java action id function search locates here (BottomToolbar.functionTarget). */
  located?: string;
  /** The JPopupMenu the button opens above itself (批量分析, 同步: BottomToolbar.java:1346-1351, 1452-1457). */
  popup?: MenuNode[];
}

// 刷新 (LizzieFrame.refreshCurrentMove, BottomToolbar.java:1054-1059); function search clicks it.
COMMANDS["board.refresh"] = { run: () => void run({ type: "RefreshCurrentMove" }) };

// BottomToolbar.functionTarget's actions (BottomToolbar.java:565-579): the navigation buttons.
COMMANDS["review.previous-ten"] = { run: () => navigate({ op: "Back", count: 10 }) };
COMMANDS["review.previous"] = { run: () => navigate({ op: "Back", count: 1 }) };
COMMANDS["review.next"] = { run: () => navigate({ op: "Forward", count: 1 }) };
COMMANDS["review.next-ten"] = { run: () => navigate({ op: "Forward", count: 10 }) };

/** Text of the move field; goto clears it and leaves the field (BottomToolbar.java:1013-1027,
 *  1435-1449). Integer.parseInt semantics: anything but an optionally signed integer is 0. */
function gotoMove(input: HTMLInputElement) {
  const text = input.value;
  input.value = "";
  input.blur();
  if (!/^[+-]?\d+$/.test(text)) return;
  const n = Number.parseInt(text, 10);
  // A negative number walks back past the root (goToMoveNumberWithinBranch).
  if (n < 0) navigate({ op: "First" });
  else if (n > 0) run({ type: "GotoMoveNumber", number: n });
}

let moveInput: HTMLInputElement;
let analyseIcon: HTMLImageElement;
let analyseLabel: HTMLSpanElement;

const GROUPS: Record<"leftPrimary" | "leftOptional" | "center" | "rightPrimary" | "rightOptional", Item[]> = {
  leftPrimary: [
    { name: "analyse", key: "BottomToolbar.analyse", cls: "primary", setting: "analyse", run: () => run({ type: "ToggleAnalysis" }) },
    { name: "batchOpen", key: "BottomToolbar.batchOpen", cls: "primary", setting: "batchOpen", popup: AUTO_ANALYSIS_MENU },
  ],
  leftOptional: [
    // openFile / saveOriFile: 保存 overwrites the record's own file, else saves as (1473-1486).
    { name: "openfile", key: "BottomToolbar.openfile", setting: "openFile", run: () => COMMANDS["files.open"].run() },
    { name: "savefile", key: "BottomToolbar.savefile", setting: "saveFile", run: () => COMMANDS["files.save"].run() },
    { name: "remoteComputeButton", key: "Menu.remoteCompute", cls: "primary", run: openRemoteCompute },
    { name: "downloadWeightButton", key: "BottomToolbar.downloadWeight", run: () => openAutoSetup("weights") },
    // toggleShowKataEstimate, shown only for a KataGo engine (BottomToolbar.java:1519-1524, 5192-5196)
    { name: "kataEstimate", key: "BottomToolbar.kataEstimate", setting: "kataEstimate", run: toggleKataEstimate, shown: () => !!state.view?.engine.foreground?.katago },
    // activeEngine.toggleHeatmap(false), as H (BottomToolbar.java:1365-1373)
    { name: "heatMap", key: "BottomToolbar.heatMap", setting: "heatMap", run: toggleRawNet },
    // toggleBestMoves, as U (BottomToolbar.java:1324-1329)
    { name: "analyzeList", key: "BottomToolbar.analyzeList", setting: "analyzeList", run: toggleFrame },
    // toggleBadMoves (BottomToolbar.java:1311-1316)
    { name: "badMoves", key: "BottomToolbar.badMoves", setting: "badMoves", run: () => COMMANDS["analysis.bad-moves"].run() },
    { name: "refresh", key: "BottomToolbar.refresh", setting: "refresh", run: () => COMMANDS["board.refresh"].run(), located: "board.refresh" },
    { name: "liveButton", key: "BottomToolbar.liveButton", setting: "live", popup: LIVE_MENU },
  ],
  center: [
    { name: "firstButton", key: "|<", cls: "nav", run: () => navigate({ op: "First" }) },
    { name: "backward10", key: "<<", cls: "nav", run: () => COMMANDS["review.previous-ten"].run() },
    // backward1 (BottomToolbar.java:1423-1434): during a game it takes back a move pair (悔棋).
    { name: "backward1", key: "<", cls: "nav", run: () => (state.view?.game ? void run({ type: "Game", op: { type: "Regret" } }) : COMMANDS["review.previous"].run()) },
    { name: "txtMoveNumber", key: "", located: "review.goto" },
    { name: "gotomove", key: "BottomToolbar.gotomove", run: () => gotoMove(moveInput) },
    { name: "forward1", key: ">", cls: "nav", run: () => COMMANDS["review.next"].run() },
    { name: "forward10", key: ">>", cls: "nav", run: () => COMMANDS["review.next-ten"].run() },
    { name: "lastButton", key: ">|", cls: "nav", run: () => navigate({ op: "Last" }) },
    // new AutoPlay().setVisible(true) (BottomToolbar.java:1296-1302)
    { name: "autoPlay", key: "BottomToolbar.autoPlay", setting: "autoPlay", run: () => COMMANDS["menu.makeAutoPlay"].run() },
  ],
  rightPrimary: [
    // 试下 / 恢复 (LizzieFrame.tryPlay, BottomToolbar.java:1321)
    { name: "tryPlay", key: "BottomToolbar.tryPlay", setting: "tryPlay", run: () => COMMANDS["toolbar.try-play"].run(), located: "board.try-play" },
    // moveToMainTrunk (BottomToolbar.java:1353-1358)
    { name: "backMain", key: "BottomToolbar.backMain", setting: "backMain", run: () => COMMANDS["menu.backToMainBranch"].run() },
    { name: "setMain", key: "BottomToolbar.setMain", setting: "setMain", run: () => COMMANDS["menu.setAsMain"].run() },
    // toggleShowMoveRankMark, as Alt+M (BottomToolbar.java:1075-1081)
    { name: "moveRank", key: "BottomToolbar.moveRank", setting: "moveRank", run: toggleMoveRank },
    // toggleShowMoveNumber / toggleCoordinates (BottomToolbar.java:1330-1343)
    { name: "move", key: "BottomToolbar.move", setting: "moveNumbers", run: toggleMoveNumbers },
    { name: "coords", key: "BottomToolbar.coords", setting: "coords", run: toggleCoordinates },
  ],
  rightOptional: [
    // toggleScoreMode (BottomToolbar.java:1512-1518)
    { name: "finalScore", key: "BottomToolbar.finalScore", setting: "finalScore", run: toggleScore },
    // newEmptyBoard (1490) and deleteMoveNoHint (1307)
    { name: "clearButton", key: "BottomToolbar.clearButton", cls: "danger", setting: "clearBoard", run: () => COMMANDS["files.new"].run() },
    { name: "deleteMove", key: "BottomToolbar.deleteMove", cls: "danger", setting: "deleteMove", run: () => COMMANDS["board.delete-without-prompt"].run(), located: "board.delete-without-prompt" },
  ],
};
const ITEMS = Object.values(GROUPS).flat();
const MORE: Item = { name: "moreActionsButton", key: "BottomToolbar.moreActions", run: () => openMore([]) };

/** The 同步 button when `id` is one of its popup's items (BottomToolbar.functionTarget's yike
 *  entries), the control function search reveals before locating the item. */
export function livePopupAnchor(id: string): HTMLElement | null {
  if (!LIVE_MENU.some((node) => node.t !== "sep" && node.id === id)) return null;
  return document.querySelector<HTMLElement>('#bottombar [data-name="liveButton"]');
}

export function locateLivePopupItem(id: string): boolean {
  const anchor = livePopupAnchor(id);
  return !!anchor && locatePopupItem(anchor, LIVE_MENU, id);
}

/** The buttons the last layout sent to 更多, in layout order. */
let overflowed: Item[] = [];

const moreId = (item: Item) => `bottombar.more.${item.name}`;

// moreActionsPopup's items (rebuildMoreActionsPopup, BottomToolbar.java:533-559): each one shows its
// button's current text and clicks the button. A button with its own popup opens it over 更多.
for (const item of ITEMS) {
  COMMANDS[moreId(item)] = {
    label: () => elements.get(item)?.textContent ?? t(item.key),
    run: () => {
      if (item.popup) openPopupMenu(elements.get(MORE)!, item.popup, true);
      else if (item.run) item.run();
      else unavailable();
    },
  };
}

function moreItems(extra: Item[]): MenuNode[] {
  return [...new Set([...overflowed, ...extra])].map((item) => ({ t: "item", key: item.key, id: moreId(item) }));
}

/** 更多: the buttons that did not fit, above the button (BottomToolbar.java:1085-1095). */
function openMore(extra: Item[]): void {
  const items = moreItems(extra);
  if (items.length > 0) openPopupMenu(elements.get(MORE)!, items, true);
}

/** BottomToolbar.locateFunction for a button that is not on the bar (switched off, or overflowed):
 *  it joins 更多's items, which open with it selected (BottomToolbar.java:656-675). Its saved
 *  自定义内容 choice is unchanged. */
export function locateHiddenButton(id: string): boolean {
  const item = ITEMS.find((i) => i.located === id);
  const more = elements.get(MORE);
  const host = more?.parentElement;
  if (!item || !more || !host) return false;
  if (more.style.display === "none") {
    more.style.display = "";
    more.style.left = `${host.clientWidth - 10 - (widths.get(MORE) ?? 52)}px`;
    more.style.width = `${widths.get(MORE) ?? 52}px`;
  }
  return locatePopupItem(more, moreItems([item]), moreId(item));
}

let detailButton: HTMLElement;
let detailIcon: HTMLImageElement;

const elements = new Map<Item, HTMLElement>();
const widths = new Map<Item, number>();

/** Java button widths: string width + 26 with a 52 minimum for text buttons (navigation arrows have
 *  no minimum); the move field is 56. The analyse button carries a 16 px icon and keeps one width
 *  for both states: the wider of its two labels + 38 (setAnalyseButtonSize, BottomToolbar.java:2608-2623). */
function measure(host: HTMLElement) {
  const g = document.createElement("canvas").getContext("2d")!;
  g.font = getComputedStyle(host).font;
  const text = (s: string) => Math.ceil(g.measureText(s).width);
  for (const item of [...ITEMS, MORE]) {
    if (item.name === "txtMoveNumber") widths.set(item, 56);
    else if (item.name === "analyse") {
      widths.set(item, Math.max(text(t("BottomToolbar.analyse")), text(t("BottomToolbar.pauseAnalyse"))) + 38);
    } else {
      const w = text(elements.get(item)!.textContent ?? "") + 26;
      widths.set(item, item.cls === "nav" ? w : Math.max(52, w));
    }
  }
}

/** Menu.updateAnalysisToolbarButton (Menu.java:11086-11091): ready2 + "pause" while analysing,
 *  playing2 + "analyse" otherwise. */
function showAnalysis(analysing: boolean) {
  analyseIcon.src = analysing ? "/assets/ready2.png" : "/assets/playing2.png";
  analyseLabel.textContent = t(analysing ? "BottomToolbar.pauseAnalyse" : "BottomToolbar.analyse");
}

/** detail (BottomToolbar.java:815-822, 1374-1385): up.png while the 详细工具栏 is collapsed, down.png
 *  while it is open; a click opens or collapses it, without the menu item's question. */
function buildDetailButton(host: HTMLElement): void {
  detailButton = document.createElement("span");
  detailButton.className = "bb detail";
  detailButton.dataset.name = "detail";
  detailButton.setAttribute("role", "button");
  detailIcon = document.createElement("img");
  detailIcon.alt = "";
  detailButton.append(detailIcon);
  detailButton.onclick = () => {
    const mode = state.view?.settings.bottom_toolbar === "Detailed" ? "Simple" : "Detailed";
    void run({ type: "SetBottomToolbar", mode });
  };
  host.append(detailButton);
}

export function buildBottomBar(host: HTMLElement): void {
  buildDetailButton(host);
  for (const item of [...ITEMS, MORE]) {
    if (item.name === "txtMoveNumber") {
      moveInput = document.createElement("input");
      moveInput.id = "moveInput";
      if (item.located) moveInput.dataset.function = item.located;
      moveInput.addEventListener("keydown", (e) => {
        if (e.key === "Enter") gotoMove(moveInput);
      });
      elements.set(item, moveInput);
      host.append(moveInput);
      continue;
    }
    const b = document.createElement("span");
    b.className = `bb ${item.cls ?? ""}`;
    b.dataset.name = item.name;
    if (item.located) b.dataset.function = item.located;
    if (item.name === "analyse") {
      b.classList.add("icon");
      analyseIcon = document.createElement("img");
      analyseIcon.alt = "";
      analyseLabel = document.createElement("span");
      b.append(analyseIcon, analyseLabel);
      showAnalysis(false);
    } else b.textContent = t(item.key);
    // batchOpen's autoAnalyzePopup (above the button) and liveButton's `yike` popup
    // (BottomToolbar.java:1452-1457): JPopupMenus shown over the bar.
    const popup = item.popup;
    if (popup) {
      b.dataset.key = item.name === "batchOpen" ? "autoAnalysis.popup" : "liveButton.popup";
      b.setAttribute("role", "button"); b.tabIndex = 0;
      b.onclick = () => openPopupMenu(b, popup, true);
      b.onkeydown = (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); b.click(); } };
      elements.set(item, b); host.append(b); continue;
    }
    if (item.name === "downloadWeightButton") {
      b.dataset.key = "weights.download"; b.setAttribute("role", "button"); b.tabIndex = 0;
      b.onkeydown = (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); item.run?.(); } };
    }
    if (item.run) b.onclick = item.run;
    else {
      b.classList.add("unavailable");
      b.title = product("unavailable");
      b.onclick = unavailable;
    }
    elements.set(item, b);
    host.append(b);
  }
  measure(host);
  buildBottomDetail(host);
  let lastAnalysis = false;
  let lastTrying = false;
  const tryPlay = elements.get(GROUPS.rightPrimary.find((item) => item.name === "tryPlay")!)!;
  const analyse = elements.get(ITEMS.find((item) => item.name === "analyse")!)!;
  let lastShown = "";
  let lastMode = "";
  subscribe(() => {
    const v = state.view;
    // 显示 / 不显示 / 使用详细工具栏 (Menu.java:4532-4591): the bar's height 26, 0 or the panels'.
    const mode = v?.settings.bottom_toolbar ?? "Simple";
    if (mode !== lastMode) {
      lastMode = mode;
      host.style.display = mode === "Hidden" ? "none" : "";
      layoutBottomBar(host);
    }
    const analysing = state.view?.engine.analysing ?? false;
    if (analysing !== lastAnalysis) {
      lastAnalysis = analysing;
      showAnalysis(analysing);
    }
    // enableDisabelForEngineGame: 分析 is off while an 引擎对战 batch runs.
    analyse.classList.toggle("match-off", !!state.view?.engine_match);
    // LizzieFrame.tryPlay / clearTryPlay: "恢复" while trying, "试下" otherwise.
    const trying = state.view?.editing.trying ?? false;
    if (trying !== lastTrying) {
      lastTrying = trying;
      tryPlay.textContent = trying ? t("BottomToolbar.tryplayBack") : t("BottomToolbar.tryPlay");
      measure(host);
      layoutBottomBar(host);
    }
    // reSetButtonLocation after a 自定义内容 check, an engine change, the 详细工具栏 button or
    // 设置模块顺序.
    const order = v?.settings.detail_panel_order;
    const shown = [...ITEMS.map((item) => visible(item)), detailButtonShown(), order?.autoAnalysis, order?.engineMatch, order?.autoPlay].join();
    if (shown !== lastShown) {
      lastShown = shown;
      layoutBottomBar(host);
    }
  });
}

/** visibleButtons: a button switched off in 自定义内容, or for this engine, takes no place. */
function visible(item: Item): boolean {
  return (!item.setting || bottomButtonShown(item.setting)) && (item.shown?.() ?? true);
}

/** layoutMainBar: left groups greedy from the left (after the 详细工具栏 button), navigation centred
 *  between them, right groups from the right (optional ones overflow first); overflowed buttons go
 *  to "更多", which reserves its width in a second pass. */
export function layoutBottomBar(host: HTMLElement): void {
  const expanded = state.view?.settings.bottom_toolbar === "Detailed";
  const height = layoutBottomDetail(host, expanded);
  if (host.style.height !== `${height}px`) host.style.height = `${height}px`;
  const W = host.clientWidth;
  const PAD = 10;
  const GAP = 6;
  const detail = detailButtonShown();
  detailButton.style.display = detail ? "" : "none";
  detailIcon.src = expanded ? "/assets/down.png" : "/assets/up.png";
  const leftStart = PAD + (detail ? 32 : 0);
  const width = (item: Item) => widths.get(item) ?? 52;
  const span = (items: Item[]) => items.reduce((s, e) => s + width(e), 0) + Math.max(0, items.length - 1) * GAP;
  const place = new Map<Item, number>();
  const g = {
    leftPrimary: GROUPS.leftPrimary.filter(visible),
    leftOptional: GROUPS.leftOptional.filter(visible),
    center: GROUPS.center.filter(visible),
    rightPrimary: GROUPS.rightPrimary.filter(visible),
    rightOptional: GROUPS.rightOptional.filter(visible),
  };
  const centerWidth = span(g.center);
  for (const reserve of [false, true]) {
    place.clear();
    let overflow = false;
    const rightEdge = W - PAD - (reserve ? width(MORE) + GAP : 0);
    const minStart = leftStart + span(g.leftPrimary) + GAP * 2;
    const maxStart = rightEdge - span(g.rightPrimary) - centerWidth - GAP * 2;
    const centerStart = Math.max(leftStart, Math.max(minStart, Math.min(Math.floor((W - centerWidth) / 2), maxStart)));
    let x = leftStart;
    for (const b of [...g.leftPrimary, ...g.leftOptional]) {
      if (x + width(b) > centerStart - GAP) overflow = true;
      else {
        place.set(b, x);
        x += width(b) + GAP;
      }
    }
    const rightLimit = centerStart + centerWidth + GAP;
    x = rightEdge;
    const primaryWidth = span(g.rightPrimary);
    for (const b of [...g.rightOptional].reverse()) {
      if (x - width(b) < rightLimit + primaryWidth + GAP) overflow = true;
      else {
        place.set(b, x - width(b));
        x -= width(b) + GAP;
      }
    }
    for (const b of [...g.rightPrimary].reverse()) {
      if (x - width(b) < rightLimit) overflow = true;
      else {
        place.set(b, x - width(b));
        x -= width(b) + GAP;
      }
    }
    x = centerStart;
    for (const b of g.center) {
      place.set(b, x);
      x += width(b) + GAP;
    }
    if (!overflow) break;
    if (reserve) place.set(MORE, W - PAD - width(MORE));
  }
  overflowed = [...g.leftPrimary, ...g.leftOptional, ...g.rightPrimary, ...g.rightOptional].filter((item) => !place.has(item));
  for (const [item, el] of elements) {
    const x = place.get(item);
    el.style.display = x === undefined ? "none" : "";
    if (x !== undefined) {
      el.style.left = `${x}px`;
      el.style.width = `${width(item)}px`;
    }
  }
}
