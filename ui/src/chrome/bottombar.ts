// Java BottomToolbar main bar: the default button set (Config.java:1900-1936) in its groups
// (BottomToolbar.java:5209-5235), placed by layoutMainBar (5237-5320). Navigation, the move field,
// the record-editing and board display buttons are implemented here; the other buttons report
// that the capability is not available yet. 工具栏 > 底部工具栏 hides the bar or adds the 详细工具栏
// panels below it (bottomDetail.ts); the mode is the core's window presentation setting.
import { navigate, run, state, subscribe, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { toggleCoordinates, toggleMoveNumbers } from "../display/commands";
import { toggleRawNet } from "../display/state";
import "../editing/commands";
import { product, t } from "../i18n";
import { openAutoSetup } from "../setup/dialog";
import { toggleKataEstimate } from "../analysis/infoCommands";
import { toggleScore } from "../analysis/scoreDialog";
import { AUTO_ANALYSIS_MENU } from "../quick/autoAnalysis";
import { openPopupMenu } from "./menubar";
import { buildBottomDetail, layoutBottomDetail } from "./bottomDetail";

interface Item {
  name: string;
  key: string;
  cls?: string;
  run?: () => void;
  /** Off by default in Java (Config.java:232, 236: backMain, tryPlay); visibleButtons skips it. */
  hidden?: boolean;
  /** Shown only while this holds (reSetButtonLocation); re-laid out when it changes. */
  shown?: () => boolean;
}

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
    { name: "analyse", key: "BottomToolbar.analyse", cls: "primary", run: () => run({ type: "ToggleAnalysis" }) },
    { name: "batchOpen", key: "BottomToolbar.batchOpen", cls: "primary" },
  ],
  leftOptional: [
    { name: "remoteComputeButton", key: "Menu.remoteCompute", cls: "primary" },
    { name: "downloadWeightButton", key: "BottomToolbar.downloadWeight", run: () => openAutoSetup("weights") },
    // toggleShowKataEstimate, shown only for a KataGo engine (BottomToolbar.java:1519-1524, 5192-5196)
    { name: "kataEstimate", key: "BottomToolbar.kataEstimate", run: toggleKataEstimate, shown: () => !!state.view?.engine.foreground?.katago },
    // activeEngine.toggleHeatmap(false), as H (BottomToolbar.java:1365-1373)
    { name: "heatMap", key: "BottomToolbar.heatMap", run: toggleRawNet },
    // toggleBadMoves (BottomToolbar.java:1311-1316); badMoves defaults to off while the top
    // toolbar and the double menu were shown at start-up (Config.java:1936)
    { name: "badMoves", key: "BottomToolbar.badMoves", shown: () => badMovesShown, run: () => COMMANDS["analysis.bad-moves"].run() },
    { name: "liveButton", key: "BottomToolbar.liveButton" },
  ],
  center: [
    { name: "firstButton", key: "|<", cls: "nav", run: () => navigate({ op: "First" }) },
    { name: "backward10", key: "<<", cls: "nav", run: () => navigate({ op: "Back", count: 10 }) },
    { name: "backward1", key: "<", cls: "nav", run: () => navigate({ op: "Back", count: 1 }) },
    { name: "txtMoveNumber", key: "" },
    { name: "gotomove", key: "BottomToolbar.gotomove", run: () => gotoMove(moveInput) },
    { name: "forward1", key: ">", cls: "nav", run: () => navigate({ op: "Forward", count: 1 }) },
    { name: "forward10", key: ">>", cls: "nav", run: () => navigate({ op: "Forward", count: 10 }) },
    { name: "lastButton", key: ">|", cls: "nav", run: () => navigate({ op: "Last" }) },
    // new AutoPlay().setVisible(true) (BottomToolbar.java:1296-1302)
    { name: "autoPlay", key: "BottomToolbar.autoPlay", run: () => COMMANDS["menu.makeAutoPlay"].run() },
  ],
  rightPrimary: [
    // 试下 / 恢复 (LizzieFrame.tryPlay, BottomToolbar.java:1321)
    { name: "tryPlay", key: "BottomToolbar.tryPlay", hidden: true, run: () => COMMANDS["toolbar.try-play"].run() },
    { name: "backMain", key: "BottomToolbar.backMain", hidden: true, run: () => COMMANDS["menu.backToMainBranch"].run() },
    { name: "setMain", key: "BottomToolbar.setMain", run: () => COMMANDS["menu.setAsMain"].run() },
    // toggleShowMoveNumber / toggleCoordinates (BottomToolbar.java:1330-1343)
    { name: "move", key: "BottomToolbar.move", run: toggleMoveNumbers },
    { name: "coords", key: "BottomToolbar.coords", run: toggleCoordinates },
  ],
  rightOptional: [
    // toggleScoreMode (BottomToolbar.java:1512-1518); off in the default bar (finalScore false)
    { name: "finalScore", key: "BottomToolbar.finalScore", hidden: true, run: toggleScore },
    // newEmptyBoard (1490) and deleteMoveNoHint (1307)
    { name: "clearButton", key: "BottomToolbar.clearButton", cls: "danger", run: () => COMMANDS["files.new"].run() },
    { name: "deleteMove", key: "BottomToolbar.deleteMove", cls: "danger", run: () => COMMANDS["board.delete-without-prompt"].run() },
  ],
};
const MORE: Item = { name: "moreActionsButton", key: "BottomToolbar.moreActions" };

/** Config.badMoves's default, from the toolbar flags the program started with. */
let badMovesShown = false;
let badMovesDecided = false;

const elements = new Map<Item, HTMLElement>();
const widths = new Map<Item, number>();

/** Java button widths: string width + 26 with a 52 minimum for text buttons (navigation arrows have
 *  no minimum); the move field is 56. The analyse button carries a 16 px icon and keeps one width
 *  for both states: the wider of its two labels + 38 (setAnalyseButtonSize, BottomToolbar.java:2608-2623). */
function measure(host: HTMLElement) {
  const g = document.createElement("canvas").getContext("2d")!;
  g.font = getComputedStyle(host).font;
  const text = (s: string) => Math.ceil(g.measureText(s).width);
  for (const item of [...Object.values(GROUPS).flat(), MORE]) {
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

export function buildBottomBar(host: HTMLElement): void {
  for (const item of [...Object.values(GROUPS).flat(), MORE]) {
    if (item.name === "txtMoveNumber") {
      moveInput = document.createElement("input");
      moveInput.id = "moveInput";
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
    if (item.name === "analyse") {
      b.classList.add("icon");
      analyseIcon = document.createElement("img");
      analyseIcon.alt = "";
      analyseLabel = document.createElement("span");
      b.append(analyseIcon, analyseLabel);
      showAnalysis(false);
    } else b.textContent = t(item.key);
    if (item.name === "batchOpen") {
      b.dataset.key = "autoAnalysis.popup";
      b.setAttribute("role", "button"); b.tabIndex = 0;
      b.onclick = () => openPopupMenu(b, AUTO_ANALYSIS_MENU, true);
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
  let lastShown = "";
  let lastMode = "";
  subscribe(() => {
    const v = state.view;
    if (v && !badMovesDecided) {
      badMovesDecided = true;
      badMovesShown = !(v.settings.panels.showTopToolbar && v.settings.panels.showDoubleMenu);
    }
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
    // LizzieFrame.tryPlay / clearTryPlay: "恢复" while trying, "试下" otherwise.
    const trying = state.view?.editing.trying ?? false;
    if (trying !== lastTrying) {
      lastTrying = trying;
      tryPlay.textContent = trying ? t("BottomToolbar.tryplayBack") : t("BottomToolbar.tryPlay");
      measure(host);
      layoutBottomBar(host);
    }
    const conditional = Object.values(GROUPS).flat().filter((item) => item.shown);
    const shown = conditional.map((item) => item.shown!()).join();
    if (shown !== lastShown) {
      lastShown = shown;
      layoutBottomBar(host);
    }
  });
}

/** layoutMainBar: left groups greedy from the left, navigation centred between them, right groups
 *  from the right (optional ones overflow first); overflowed buttons go to "更多", which reserves
 *  its width in a second pass. */
export function layoutBottomBar(host: HTMLElement): void {
  const height = layoutBottomDetail(host, state.view?.settings.bottom_toolbar === "Detailed");
  if (host.style.height !== `${height}px`) host.style.height = `${height}px`;
  const PAD = 10;
  const GAP = 6;
  const width = (item: Item) => widths.get(item) ?? 52;
  const span = (items: Item[]) => items.reduce((s, e) => s + width(e), 0) + Math.max(0, items.length - 1) * GAP;
  const place = new Map<Item, number>();
  // visibleButtons: buttons switched off in the toolbar settings or for this engine take no place.
  const shown = (items: Item[]) => items.filter((item) => !item.hidden && (item.shown?.() ?? true));
  const g = {
    leftPrimary: shown(GROUPS.leftPrimary),
    leftOptional: shown(GROUPS.leftOptional),
    center: shown(GROUPS.center),
    rightPrimary: shown(GROUPS.rightPrimary),
    rightOptional: shown(GROUPS.rightOptional),
  };
  const centerWidth = span(g.center);
  for (const reserve of [false, true]) {
    place.clear();
    let overflow = false;
    const rightEdge = W - PAD - (reserve ? width(MORE) + GAP : 0);
    const minStart = PAD + span(g.leftPrimary) + GAP * 2;
    const maxStart = rightEdge - span(g.rightPrimary) - centerWidth - GAP * 2;
    const centerStart = Math.max(PAD, Math.max(minStart, Math.min(Math.floor((W - centerWidth) / 2), maxStart)));
    let x = PAD;
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
  for (const [item, el] of elements) {
    const x = place.get(item);
    el.style.display = x === undefined ? "none" : "";
    if (x !== undefined) {
      el.style.left = `${x}px`;
      el.style.width = `${width(item)}px`;
    }
  }
}
