// Java BottomToolbar main bar: its buttons (BottomToolbar.java:723-884) in their groups
// (5209-5235), placed by layoutMainBar (5237-5320); 自定义内容 (chrome/bottomButtons.ts) decides which
// are shown. Buttons that do not fit go to 更多. Navigation, the move field, the record-editing and
// board display buttons are implemented here; the other buttons report that the capability is not
// available yet. 工具栏 > 底部工具栏 shows or hides the bar; the mode is the core's window
// presentation setting. This product leaves out Java's 详细工具栏.
import { navigate, run, state, subscribe, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { rankMarkLit, toggleCoordinates, toggleMoveNumbers, toggleMoveRank } from "../display/commands";
import { toggleRawNet } from "../display/state";
import "../editing/commands";
import { product, registerProduct, t } from "../i18n";
import { openAutoSetup } from "../setup/dialog";
import { openRemoteCompute } from "../cloud/center";
import { toggleKataEstimate } from "../analysis/infoCommands";
import { toggleScore } from "../analysis/scoreDialog";
import { toggleFrame } from "../analysis/tableCommands";
import { AUTO_ANALYSIS_MENU } from "../quick/autoAnalysis";
import { LIVE_MENU } from "../online/commands";
import type { MenuNode } from "./javaMenus";
import { locatePopupItem, menuOpen, openPopupMenu } from "./menubar";
import { bottomButtonShown, type BottomButton } from "./bottomButtons";
import { icon, type IconName } from "../design/icons";

registerProduct({
  ZhCn: { stepFirstMove: "跳到第一手", stepLastMove: "跳到最后一手" },
  ZhTw: { stepFirstMove: "跳到第一手", stepLastMove: "跳到最後一手" },
  EnUs: { stepFirstMove: "Go to first move", stepLastMove: "Go to last move" },
  JaJp: { stepFirstMove: "初手へ移動", stepLastMove: "最終手へ移動" },
  KoKr: { stepFirstMove: "첫 수로 이동", stepLastMove: "마지막 수로 이동" },
  ThTh: { stepFirstMove: "ไปตาแรก", stepLastMove: "ไปตาสุดท้าย" },
});

/** The controls a toolbar's arrows move between: its buttons and check boxes. Text and number
 *  fields keep their own keys and Tab stops. */
const TOOLBAR_ITEM = '[role="button"], input[type="checkbox"]';

/** The WAI-ARIA toolbar keys on `bar`, named `name`. The bar is one Tab stop, on the item last
 *  focused in it, else on its first enabled item. ←/→ move between the shown items and wrap,
 *  Home/End go to the first/last; Enter/Space press the focused button as a click does (nothing
 *  while it is aria-disabled), and a check box keeps Space. These keys stay in the bar; every other
 *  key goes on to the workbench shortcuts. A press on an item does not focus it: the focus leaves a
 *  field as it did on chrome that takes no focus, and an item that keeps the focus where it was
 *  (its own mousedown preventDefault) keeps doing so. A press outside the bar takes the focus off
 *  it, so a click on the board gives the arrows back to the moves. Returns the function that moves
 *  the Tab stop off an item that was hidden or disabled; run it after items show, hide or change
 *  state. */
export function toolbarKeys(bar: HTMLElement, name: string): () => void {
  bar.setAttribute("role", "toolbar");
  bar.setAttribute("aria-label", name);
  const all = () => [...bar.querySelectorAll<HTMLElement>(TOOLBAR_ITEM)];
  const usable = (el: HTMLElement) => !(el instanceof HTMLInputElement && el.disabled) && el.getClientRects().length > 0;
  let last: HTMLElement | null = null;
  let stop: HTMLElement | null = null;
  for (const el of all()) el.tabIndex = -1;
  const settle = () => {
    const next = last && usable(last) ? last : (all().find((el) => el.getAttribute("aria-disabled") !== "true" && usable(el)) ?? all().find(usable) ?? null);
    if (next === stop) return;
    if (stop) stop.tabIndex = -1;
    stop = next;
    if (stop) stop.tabIndex = 0;
  };
  bar.addEventListener("focusin", (e) => {
    if (!(e.target as Element).matches(TOOLBAR_ITEM)) return;
    last = e.target as HTMLElement;
    settle();
  });
  bar.addEventListener("keydown", (e) => {
    const el = e.target as HTMLElement;
    if (!el.matches(TOOLBAR_ITEM) || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || menuOpen()) return;
    if (e.key === "Enter" || e.key === " ") {
      e.stopPropagation();
      if (el instanceof HTMLInputElement) return;
      e.preventDefault();
      if (el.getAttribute("aria-disabled") !== "true") el.click();
      return;
    }
    const items = all().filter(usable);
    const at = items.indexOf(el);
    const to = e.key === "ArrowRight" ? at + 1 : e.key === "ArrowLeft" ? at - 1 : e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : null;
    if (to === null || items.length === 0) return;
    e.preventDefault();
    e.stopPropagation();
    items[(to + items.length) % items.length].focus();
  });
  bar.addEventListener("mousedown", (e) => {
    if (e.defaultPrevented || !(e.target as Element).closest(`${TOOLBAR_ITEM}, .tb-checklabel`)) return;
    e.preventDefault();
    const focus = document.activeElement;
    if (focus instanceof HTMLElement && focus !== document.body) focus.blur();
  });
  // The board keeps the focus where it is on a press (its mousedown preventDefault): a toolbar
  // item that has it lets it go first.
  document.addEventListener("mousedown", (e) => {
    const focus = document.activeElement;
    if (focus instanceof HTMLElement && bar.contains(focus) && !bar.contains(e.target as Node)) focus.blur();
  }, true);
  settle();
  return settle;
}

interface Item {
  name: string;
  /** The label's resource key; for an `icon` button, Java's own glyph label. */
  key: string;
  cls?: string;
  /** Drawn in place of the label. */
  icon?: IconName;
  /** An icon button's accessible name. */
  label?: () => string;
  /** Its capsule: neighbouring buttons of one capsule adjoin as its segments. */
  cap: string;
  /** A display switch: lit while this holds. */
  lit?: () => boolean;
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

// BottomToolbar.functionTarget's actions (BottomToolbar.java:565-579): the navigation buttons; function
// search clicks them, so each runs its button's action.
COMMANDS["review.previous-ten"] = { run: () => navigate({ op: "Back", count: 10 }) };
// backward1 (BottomToolbar.java:1423-1434): during a game it takes back a move pair (悔棋).
COMMANDS["review.previous"] = { run: () => (state.view?.game ? void run({ type: "Game", op: { type: "Regret" } }) : navigate({ op: "Back", count: 1 })) };
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
let analyseIcon: SVGSVGElement;
let analyseLabel: HTMLSpanElement;

const GROUPS: Record<"leftPrimary" | "leftOptional" | "center" | "rightPrimary" | "rightOptional", Item[]> = {
  leftPrimary: [
    { name: "analyse", key: "BottomToolbar.analyse", cap: "analyse", setting: "analyse", run: () => run({ type: "ToggleAnalysis" }) },
    { name: "batchOpen", key: "BottomToolbar.batchOpen", cap: "services", setting: "batchOpen", popup: AUTO_ANALYSIS_MENU },
  ],
  leftOptional: [
    // openFile / saveOriFile: 保存 overwrites the record's own file, else saves as (1473-1486).
    { name: "openfile", key: "BottomToolbar.openfile", cap: "file", setting: "openFile", run: () => COMMANDS["files.open"].run() },
    { name: "savefile", key: "BottomToolbar.savefile", cap: "file", setting: "saveFile", run: () => COMMANDS["files.save"].run() },
    { name: "remoteComputeButton", key: "Menu.remoteCompute", cap: "services", run: openRemoteCompute },
    { name: "downloadWeightButton", key: "BottomToolbar.downloadWeight", cap: "services", run: () => openAutoSetup("weights") },
    // toggleShowKataEstimate, shown only for a KataGo engine (BottomToolbar.java:1519-1524, 5192-5196)
    { name: "kataEstimate", key: "BottomToolbar.kataEstimate", cap: "views", setting: "kataEstimate", run: toggleKataEstimate, shown: () => !!state.view?.engine.foreground?.katago, lit: () => state.view?.kata_estimate.state.shown ?? false },
    // activeEngine.toggleHeatmap(false), as H (BottomToolbar.java:1365-1373)
    { name: "heatMap", key: "BottomToolbar.heatMap", cap: "views", setting: "heatMap", run: toggleRawNet, lit: () => !!state.view?.rawNet },
    // toggleBestMoves, as U (BottomToolbar.java:1324-1329)
    { name: "analyzeList", key: "BottomToolbar.analyzeList", cap: "views", setting: "analyzeList", run: toggleFrame },
    // toggleBadMoves (BottomToolbar.java:1311-1316)
    { name: "badMoves", key: "BottomToolbar.badMoves", cap: "views", setting: "badMoves", run: () => COMMANDS["analysis.bad-moves"].run() },
    { name: "refresh", key: "BottomToolbar.refresh", cap: "sync", setting: "refresh", run: () => COMMANDS["board.refresh"].run(), located: "board.refresh" },
    { name: "liveButton", key: "BottomToolbar.liveButton", cap: "sync", setting: "live", popup: LIVE_MENU },
  ],
  center: [
    { name: "firstButton", key: "|<", cls: "nav", cap: "nav", icon: "step-first", label: () => product("stepFirstMove"), run: () => navigate({ op: "First" }) },
    { name: "backward10", key: "<<", cls: "nav", cap: "nav", icon: "step-back-ten", label: () => t("Accessibility.previousTenMoves"), run: () => COMMANDS["review.previous-ten"].run() },
    { name: "backward1", key: "<", cls: "nav", cap: "nav", icon: "step-back", label: () => t("Accessibility.previousMove"), run: () => COMMANDS["review.previous"].run() },
    { name: "txtMoveNumber", key: "", cap: "nav", located: "review.goto" },
    { name: "gotomove", key: "BottomToolbar.gotomove", cap: "nav", run: () => gotoMove(moveInput) },
    { name: "forward1", key: ">", cls: "nav", cap: "nav", icon: "step-forward", label: () => t("Accessibility.nextMove"), run: () => COMMANDS["review.next"].run() },
    { name: "forward10", key: ">>", cls: "nav", cap: "nav", icon: "step-forward-ten", label: () => t("Accessibility.nextTenMoves"), run: () => COMMANDS["review.next-ten"].run() },
    { name: "lastButton", key: ">|", cls: "nav", cap: "nav", icon: "step-last", label: () => product("stepLastMove"), run: () => navigate({ op: "Last" }) },
    // new AutoPlay().setVisible(true) (BottomToolbar.java:1296-1302)
    { name: "autoPlay", key: "BottomToolbar.autoPlay", cap: "nav", setting: "autoPlay", run: () => COMMANDS["menu.makeAutoPlay"].run() },
  ],
  rightPrimary: [
    // 试下 / 恢复 (LizzieFrame.tryPlay, BottomToolbar.java:1321)
    { name: "tryPlay", key: "BottomToolbar.tryPlay", cap: "branch", setting: "tryPlay", run: () => COMMANDS["toolbar.try-play"].run(), located: "board.try-play" },
    // moveToMainTrunk (BottomToolbar.java:1353-1358)
    { name: "backMain", key: "BottomToolbar.backMain", cap: "branch", setting: "backMain", run: () => COMMANDS["menu.backToMainBranch"].run() },
    { name: "setMain", key: "BottomToolbar.setMain", cap: "branch", setting: "setMain", run: () => COMMANDS["menu.setAsMain"].run() },
    // toggleShowMoveRankMark, as Alt+M (BottomToolbar.java:1075-1081)
    { name: "moveRank", key: "BottomToolbar.moveRank", cap: "marks", setting: "moveRank", run: toggleMoveRank, lit: rankMarkLit },
    // toggleShowMoveNumber / toggleCoordinates (BottomToolbar.java:1330-1343)
    { name: "move", key: "BottomToolbar.move", cap: "marks", setting: "moveNumbers", run: toggleMoveNumbers, lit: () => (state.view?.settings.display.moveNumbers ?? 0) !== 0 },
    { name: "coords", key: "BottomToolbar.coords", cap: "marks", setting: "coords", run: toggleCoordinates, lit: () => state.view?.settings.display.coordinates ?? false },
  ],
  rightOptional: [
    // toggleScoreMode (BottomToolbar.java:1512-1518)
    { name: "finalScore", key: "BottomToolbar.finalScore", cap: "marks", setting: "finalScore", run: toggleScore, lit: () => !!state.view?.score },
    // newEmptyBoard (1490) and deleteMoveNoHint (1307)
    { name: "clearButton", key: "BottomToolbar.clearButton", cls: "danger", cap: "edit", setting: "clearBoard", run: () => COMMANDS["files.new"].run() },
    { name: "deleteMove", key: "BottomToolbar.deleteMove", cls: "danger", cap: "edit", setting: "deleteMove", run: () => COMMANDS["board.delete-without-prompt"].run(), located: "board.delete-without-prompt" },
  ],
};
const ITEMS = Object.values(GROUPS).flat();
const MORE: Item = { name: "moreActionsButton", key: "BottomToolbar.moreActions", cap: "more", run: () => openMore([]) };

/** The 同步 button when `id` is one of its popup's items (BottomToolbar.functionTarget's yike
 *  entries), the control function search reveals before locating the item. */
export function livePopupAnchor(id: string): HTMLElement | null {
  if (!LIVE_MENU.some((node) => node.t !== "sep" && node.id === id)) return null;
  return document.querySelector<HTMLElement>('#bottombar [data-name="liveButton"]');
}

/** BottomToolbar.locateFunction's popup branch (BottomToolbar.java:645-649): yike.show(this, 0,
 *  -height) opens the popup above the bar, so 同步 need not be on it. With 同步 on the bar the popup
 *  stands above the button; otherwise above the bar's left end, as in Java. */
export function locateLivePopupItem(id: string): boolean {
  const button = livePopupAnchor(id);
  if (!button) return false;
  const anchor = button.getClientRects().length > 0 ? button : elements.get(MORE)?.parentElement;
  return !!anchor && locatePopupItem(anchor, LIVE_MENU, id);
}

/** The buttons the last layout sent to 更多, in layout order. */
let overflowed: Item[] = [];
/** Set while a layout must show 更多 although every button fits. */
let showMore = false;

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
    showMore = true;
    layoutBottomBar(host);
    showMore = false;
  }
  return locatePopupItem(more, moreItems([item]), moreId(item));
}

const elements = new Map<Item, HTMLElement>();
const widths = new Map<Item, number>();
/** Moves the bar's Tab stop off a button that hid or was disabled (toolbarKeys). */
let settleTabStop = () => {};

/** The capsules laid under the runs of buttons, reused from layout to layout. */
const capsules: HTMLElement[] = [];

/** Button widths as segments of a capsule: the label's width + 24 (44 at least), 30 around a step
 *  icon, 56 for the move field with its margins. The analyse button keeps one width for both
 *  states: the wider of its two labels + 42 for its icon (setAnalyseButtonSize,
 *  BottomToolbar.java:2608-2623). */
function measure(host: HTMLElement) {
  const g = document.createElement("canvas").getContext("2d")!;
  g.font = getComputedStyle(host).font;
  const text = (s: string) => Math.ceil(g.measureText(s).width);
  for (const item of [...ITEMS, MORE]) {
    if (item.name === "txtMoveNumber") widths.set(item, 56);
    else if (item.name === "analyse") {
      widths.set(item, Math.max(text(t("BottomToolbar.analyse")), text(t("BottomToolbar.pauseAnalyse"))) + 42);
    } else widths.set(item, item.icon ? 30 : Math.max(44, text(elements.get(item)!.textContent ?? "") + 24));
  }
}

/** Menu.updateAnalysisToolbarButton (Menu.java:11086-11091): the pause icon and "pause" while
 *  analysing, the play icon and "analyse" otherwise. */
function showAnalysis(analysing: boolean) {
  analyseIcon = swapIcon(analyseIcon, analysing ? "pause" : "play");
  analyseLabel.textContent = t(analysing ? "BottomToolbar.pauseAnalyse" : "BottomToolbar.analyse");
}

/** The named icon in place of `current`: `current` itself when it already shows that icon. */
function swapIcon(current: SVGSVGElement, name: IconName): SVGSVGElement {
  if (current.dataset.icon === name) return current;
  const next = icon(name);
  current.replaceWith(next);
  return next;
}

export function buildBottomBar(host: HTMLElement): void {
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
    b.setAttribute("role", "button");
    if (item.located) b.dataset.function = item.located;
    if (item.name === "analyse") {
      analyseIcon = icon("play");
      analyseLabel = document.createElement("span");
      b.append(analyseIcon, analyseLabel);
      showAnalysis(false);
    } else if (item.icon) {
      b.setAttribute("aria-label", item.label?.() ?? item.key);
      b.append(icon(item.icon));
    } else b.textContent = t(item.key);
    // batchOpen's autoAnalyzePopup (above the button) and liveButton's `yike` popup
    // (BottomToolbar.java:1452-1457): JPopupMenus shown over the bar.
    const popup = item.popup;
    if (popup) {
      b.dataset.key = item.name === "batchOpen" ? "autoAnalysis.popup" : "liveButton.popup";
      b.onclick = () => openPopupMenu(b, popup, true);
      elements.set(item, b); host.append(b); continue;
    }
    if (item.name === "downloadWeightButton") b.dataset.key = "weights.download";
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
  settleTabStop = toolbarKeys(host, t("Menu.bottomToolBar"));
  let lastAnalysis = false;
  let lastTrying = false;
  const tryPlay = elements.get(GROUPS.rightPrimary.find((item) => item.name === "tryPlay")!)!;
  const analyse = elements.get(ITEMS.find((item) => item.name === "analyse")!)!;
  let lastShown = "";
  let lastMode = "";
  subscribe(() => {
    const v = state.view;
    // 显示 / 不显示 (Menu.java:4532-4551).
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
    const matchOff = !!state.view?.engine_match;
    analyse.classList.toggle("match-off", matchOff);
    analyse.setAttribute("aria-disabled", String(matchOff));
    // LizzieFrame.tryPlay / clearTryPlay: "恢复" while trying, "试下" otherwise.
    const trying = state.view?.editing.trying ?? false;
    if (trying !== lastTrying) {
      lastTrying = trying;
      tryPlay.textContent = trying ? t("BottomToolbar.tryplayBack") : t("BottomToolbar.tryPlay");
      measure(host);
      layoutBottomBar(host);
    }
    for (const [item, el] of elements) {
      if (!item.lit) continue;
      const lit = item.lit();
      el.classList.toggle("on", lit);
      el.setAttribute("aria-pressed", String(lit));
    }
    // reSetButtonLocation after a 自定义内容 check or an engine change.
    const shown = ITEMS.map((item) => visible(item)).join();
    if (shown !== lastShown) {
      lastShown = shown;
      layoutBottomBar(host);
    }
    settleTabStop();
  });
}

/** visibleButtons: a button switched off in 自定义内容, or for this engine, takes no place. */
function visible(item: Item): boolean {
  return (!item.setting || bottomButtonShown(item.setting)) && (item.shown?.() ?? true);
}

/** layoutMainBar: left groups greedy from the left, navigation centred between them, right groups
 *  from the right (optional ones overflow first); overflowed buttons go to "更多", which reserves
 *  its width in a second pass. Buttons of one capsule adjoin, capsules stand 8 px apart, and a
 *  capsule is laid under each run of adjoining buttons. */
export function layoutBottomBar(host: HTMLElement): void {
  const W = host.clientWidth;
  const PAD = 10;
  const GAP = 8;
  const width = (item: Item) => widths.get(item) ?? 44;
  const gap = (a: Item, b: Item) => (a.cap === b.cap ? 0 : GAP);
  const span = (items: Item[]) => items.reduce((s, e, k) => s + width(e) + (k > 0 ? gap(items[k - 1], e) : 0), 0);
  const place = new Map<Item, number>();
  const g = {
    leftPrimary: GROUPS.leftPrimary.filter(visible),
    leftOptional: GROUPS.leftOptional.filter(visible),
    center: GROUPS.center.filter(visible),
    rightPrimary: GROUPS.rightPrimary.filter(visible),
    rightOptional: GROUPS.rightOptional.filter(visible),
  };
  const centerWidth = span(g.center);
  for (const reserve of showMore ? [true] : [false, true]) {
    place.clear();
    let overflow = false;
    const rightEdge = W - PAD - (reserve ? width(MORE) + GAP : 0);
    const minStart = PAD + span(g.leftPrimary) + GAP * 2;
    const maxStart = rightEdge - span(g.rightPrimary) - centerWidth - GAP * 2;
    const centerStart = Math.max(PAD, Math.max(minStart, Math.min(Math.floor((W - centerWidth) / 2), maxStart)));
    let x = PAD;
    let placed: Item | null = null;
    for (const b of [...g.leftPrimary, ...g.leftOptional]) {
      const start = placed ? x + gap(placed, b) : x;
      if (start + width(b) > centerStart - GAP) overflow = true;
      else {
        place.set(b, start);
        x = start + width(b);
        placed = b;
      }
    }
    const rightLimit = centerStart + centerWidth + GAP;
    const primaryWidth = span(g.rightPrimary);
    x = rightEdge;
    placed = null;
    // From the right edge inwards: the optional buttons first, keeping room for the primary ones.
    for (const [b, reserved] of [...g.rightPrimary.map((b) => [b, 0] as const), ...g.rightOptional.map((b) => [b, primaryWidth + GAP] as const)].reverse()) {
      const start = (placed ? x - gap(b, placed) : x) - width(b);
      if (start < rightLimit + reserved) overflow = true;
      else {
        place.set(b, start);
        x = start;
        placed = b;
      }
    }
    x = centerStart;
    g.center.forEach((b, k) => {
      if (k > 0) x += gap(g.center[k - 1], b);
      place.set(b, x);
      x += width(b);
    });
    if (reserve && (overflow || showMore)) place.set(MORE, W - PAD - width(MORE));
    if (!overflow) break;
  }
  overflowed = [...g.leftPrimary, ...g.leftOptional, ...g.rightPrimary, ...g.rightOptional].filter((item) => !place.has(item));
  for (const [item, el] of elements) if (!place.has(item)) el.style.display = "none";

  // From left to right: each button at its place, and the runs of adjoining buttons of one capsule.
  const order = [...place].sort((a, b) => a[1] - b[1]);
  const text = (item: Item) => !item.icon && item.name !== "txtMoveNumber";
  const runs: { cap: string; left: number; right: number }[] = [];
  order.forEach(([item, x], k) => {
    const el = elements.get(item)!;
    const before = k > 0 ? order[k - 1] : null;
    const after = k + 1 < order.length ? order[k + 1] : null;
    const joined = !!before && before[0].cap === item.cap && before[1] + width(before[0]) === x;
    if (joined) runs[runs.length - 1].right = x + width(item);
    else runs.push({ cap: item.cap, left: x, right: x + width(item) });
    el.style.display = "";
    // The move field stands inside its capsule with a margin; the others fill their segment.
    const inset = item.name === "txtMoveNumber" ? 4 : 0;
    el.style.left = `${x + inset}px`;
    el.style.width = `${width(item) - inset * 2}px`;
    el.classList.toggle("first", !joined);
    el.classList.toggle("last", !after || after[0].cap !== item.cap || x + width(item) !== after[1]);
    // A line where a text button meets its neighbour; step icons stand together without one.
    el.classList.toggle("joined", joined && (text(item) || text(before[0])));
  });
  runs.forEach((run, k) => {
    const cap = (capsules[k] ??= host.insertBefore(document.createElement("div"), host.firstChild));
    cap.className = "bb-cap";
    cap.style.display = "";
    cap.style.left = `${run.left}px`;
    cap.style.width = `${run.right - run.left}px`;
  });
  for (const cap of capsules.slice(runs.length)) cap.style.display = "none";
  settleTabStop();
}
