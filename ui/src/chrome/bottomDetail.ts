// 详细工具栏 (BottomToolbar.java:1556-2374, layoutDetailPanels 686-721): the three 44 px panels under
// the main bar, at Java's component bounds. 自动分析 edits the saved 自动分析 settings and starts or
// stops the run, as StartAnaDialog does; 显示黑 / 显示白, 自动落子, 自动播放(大) and 自动变化图 drive the
// candidate sides, the engine playing the checked sides and the players. 引擎对战 edits the match's
// toolbar values and starts, pauses, stops and intervenes in the core's match.
import { run, state, subscribe } from "../app";
import { COMMANDS, setDisplay } from "../commands";
import type { AutoAnalysisSettings } from "../contract/generated";
import { toolbarAutoplay } from "../display/autoplay";
import { setCandidateSide } from "../display/commands";
import { boardUi, onBoardUiChange } from "../display/state";
import { t } from "../i18n";
import { startFromToolbar, stopMatch, toggleMatchPause } from "../match/commands";
import { openEnginePkConfig, openManual, sharedSession, type SharedFields } from "../match/dialogs";
import { engineIdentity, matchActive, matchUi, matchValues, onMatchUiChange, setEngineMatchChecked } from "../match/state";

const MAIN_BAR_HEIGHT = 26;
const DETAIL_PANEL_HEIGHT = 44;
const DETAIL_PANEL_GAP = 8;
const SIDE_PADDING = 10;

type Bounds = [number, number, number, number];

function at<T extends HTMLElement>(panel: HTMLElement, el: T, [x, y, w, h]: Bounds): T {
  Object.assign(el.style, { left: `${x}px`, top: `${y}px`, width: `${w}px`, height: `${h}px` });
  panel.append(el);
  return el;
}

function label(panel: HTMLElement, text: string, bounds: Bounds): HTMLElement {
  const el = at(panel, document.createElement("span"), bounds);
  el.textContent = text;
  return el;
}

/** A JCheckBox; with `text` the box carries its own label. */
function check(panel: HTMLElement, key: string, bounds: Bounds, text = ""): HTMLInputElement {
  const wrap = at(panel, document.createElement("label"), bounds);
  const box = document.createElement("input");
  box.type = "checkbox";
  box.dataset.key = key;
  wrap.append(box, text);
  return box;
}

/** A JTextField with IntDocument: only digits can be typed. */
function digits(panel: HTMLElement, key: string, bounds: Bounds, decimal = false): HTMLInputElement {
  const field = at(panel, document.createElement("input"), bounds);
  field.type = "text";
  field.dataset.key = key;
  field.addEventListener("input", () => {
    const kept = field.value.replace(decimal ? /[^\d.]/g : /\D/g, "");
    if (kept !== field.value) field.value = kept;
  });
  return field;
}

function push(panel: HTMLElement, key: string, text: string, bounds: Bounds): HTMLButtonElement {
  const button = at(panel, document.createElement("button"), bounds);
  button.dataset.key = key;
  button.textContent = text;
  return button;
}

function detailPanel(host: HTMLElement, name: string, width: number): HTMLElement {
  const panel = document.createElement("div");
  panel.className = "bb-detail";
  panel.dataset.name = name;
  panel.dataset.width = String(width);
  host.append(panel);
  return panel;
}

const fieldText = (n: number) => (n > 0 ? String(n) : "");
const fieldInt = (field: HTMLInputElement) => (/^\d+$/.test(field.value) ? Number(field.value) : -1);

let panels: HTMLElement[] = [];

/** anaPanel (BottomToolbar.java:1574-1800). */
function analysisPanel(host: HTMLElement): void {
  const p = detailPanel(host, "anaPanel", 400);
  const autoAnalyse = check(p, "toolbar.detail.autoAnalyze", [1, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.autoAnalyze"), [21, 0, 60, 20]);
  const black = check(p, "toolbar.detail.anaBlack", [68, 1, 40, 20], t("BottomToolbar.detail.black"));
  label(p, t("BottomToolbar.detail.moveRange"), [108, 0, 40, 20]);
  const first = digits(p, "toolbar.detail.firstMove", [138, 2, 30, 18]);
  label(p, t("BottomToolbar.detail.to"), [170, 0, 15, 20]);
  const last = digits(p, "toolbar.detail.lastMove", [185, 2, 30, 18]);
  const firstVisitsOn = check(p, "toolbar.detail.firstPlayoutsOn", [215, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.firstPlayouts"), [234, 0, 80, 20]);
  const firstVisits = digits(p, "toolbar.detail.firstPlayouts", [298, 2, 42, 18]);
  const start = push(p, "toolbar.detail.start", t("BottomToolbar.detail.start"), [340, 1, 30, 20]);
  const pause = push(p, "toolbar.detail.pause", t("BottomToolbar.detail.pause"), [369, 1, 30, 20]);
  const autoSave = check(p, "toolbar.detail.saveKifu", [1, 22, 20, 20]);
  label(p, t("BottomToolbar.detail.saveKifu"), [21, 22, 50, 20]);
  const white = check(p, "toolbar.detail.anaWhite", [68, 22, 40, 20], t("BottomToolbar.detail.white"));
  const visitsOn = check(p, "toolbar.detail.totalPlayoutsOn", [104, 22, 20, 20]);
  label(p, t("BottomToolbar.detail.totalPlayouts"), [124, 22, 80, 20]);
  const visits = digits(p, "toolbar.detail.totalPlayouts", [175, 23, 50, 18]);
  const timeOn = check(p, "toolbar.detail.byTimeOn", [225, 22, 20, 20]);
  label(p, t("BottomToolbar.detail.byTime"), [245, 22, 80, 20]);
  const time = digits(p, "toolbar.detail.byTime", [305, 23, 25, 18]);
  push(p, "toolbar.detail.batchProgress", t("BottomToolbar.detail.batchProgress"), [330, 22, 69, 20]).onclick = () => COMMANDS["batch-progress"].run();

  /** The panel's fields over the saved settings: the store StartAnaDialog edits as well. */
  const edited = (saved: AutoAnalysisSettings): AutoAnalysisSettings => ({
    ...saved,
    startMove: fieldInt(first),
    endMove: fieldInt(last),
    black: black.checked,
    white: white.checked,
    autoSave: autoSave.checked,
    firstVisitsEnabled: firstVisitsOn.checked,
    firstVisits: fieldInt(firstVisits),
    visitsEnabled: visitsOn.checked,
    visits: fieldInt(visits),
    timeEnabled: timeOn.checked,
    timeSeconds: fieldInt(time),
  });
  const save = () => {
    const saved = state.view?.settings.auto_analysis;
    if (saved) void run({ type: "SaveAutoAnalysisSettings", settings: edited(saved) });
  };
  for (const control of [black, white, autoSave, firstVisitsOn, visitsOn, timeOn, first, last, firstVisits, visits, time]) control.addEventListener("change", save);
  // chkAutoAnalyse and 开始 / 终止: startAutoAna with the panel's fields, or stopAutoAna.
  const startOrStop = () => {
    const v = state.view;
    if (!v) return;
    if (v.auto_analysis.active) void run({ type: "StopAutoAnalysis" });
    else void run({ type: "StartAutoAnalysis", settings: edited(v.settings.auto_analysis) });
  };
  autoAnalyse.onchange = startOrStop;
  start.onclick = startOrStop;
  // 暂停 / 继续: Lizzie.leelaz.togglePonder.
  pause.onclick = () => void run({ type: "ToggleAnalysis" });
  subscribe(() => {
    const v = state.view;
    if (!v) return;
    const s = v.settings.auto_analysis;
    const a = v.auto_analysis;
    for (const [field, value] of [[first, s.startMove >= 0 ? String(s.startMove) : ""], [last, fieldText(s.endMove)], [firstVisits, fieldText(s.firstVisits)], [visits, fieldText(s.visits)], [time, fieldText(s.timeSeconds)]] as const)
      if (document.activeElement !== field && field.value !== value) field.value = value;
    black.checked = s.black;
    white.checked = s.white;
    autoSave.checked = s.autoSave;
    firstVisitsOn.checked = s.firstVisitsEnabled;
    visitsOn.checked = s.visitsEnabled;
    timeOn.checked = s.timeEnabled;
    autoAnalyse.checked = a.active || a.starting;
    autoAnalyse.disabled = start.disabled = a.starting;
    start.textContent = a.starting ? t("AutoAnalyze.preparing") : a.active ? t("BottomToolbar.detail.stop") : t("BottomToolbar.detail.start");
    pause.textContent = v.engine.analysing ? t("BottomToolbar.detail.pause") : t("BottomToolbar.detail.continue");
  });
}

/** enginePkPanel (BottomToolbar.java:2008-2336). The fields are the toolbar values the core keeps
 *  (toolbar-parameter); 多盘 revises the running batch as it is typed (applyLiveBatchLimit). */
function engineMatchPanel(host: HTMLElement): void {
  const p = detailPanel(host, "enginePkPanel", 600);
  p.dataset.function = "legacy.engine-game";
  const pkOn = check(p, "toolbar.detail.engineMatch", [2, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.engineMatch"), [22, 0, 60, 18]);
  label(p, t("BottomToolbar.detail.blackColon"), [75, 0, 15, 20]);
  const black = at(p, document.createElement("select"), [90, 2, 88, 18]);
  black.dataset.key = "toolbar.detail.engineBlack";
  const score = label(p, "0:0", [186, 0, 45, 20]);
  score.dataset.key = "toolbar.detail.pkResult";
  label(p, t("BottomToolbar.detail.whiteColon"), [237, 0, 15, 20]);
  const white = at(p, document.createElement("select"), [255, 2, 88, 18]);
  white.dataset.key = "toolbar.detail.engineWhite";
  const visitsOn = check(p, "toolbar.detail.pkPlayoutsOn", [355, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.totalPlayoutsBlack"), [375, 0, 70, 18]);
  const visitsBlack = digits(p, "toolbar.detail.pkPlayoutsBlack", [445, 2, 50, 18]);
  label(p, t("BottomToolbar.detail.whiteColon"), [498, 0, 15, 18]);
  const visitsWhite = digits(p, "toolbar.detail.pkPlayoutsWhite", [513, 2, 50, 18]);
  const intervene = push(p, "toolbar.detail.intervene", t("BottomToolbar.detail.intervene"), [563, 1, 35, 20]);
  intervene.dataset.function = "legacy.engine-game-intervene";
  const start = push(p, "toolbar.detail.pkStart", t("BottomToolbar.detail.start"), [8, 22, 35, 20]);
  start.dataset.function = "legacy.engine-game-start";
  const settings = push(p, "toolbar.detail.pkSettings", t("BottomToolbar.detail.settings"), [42, 22, 35, 20]);
  settings.dataset.function = "legacy.engine-game-settings";
  const pause = push(p, "toolbar.detail.pkPause", t("BottomToolbar.detail.pause"), [76, 22, 35, 20]);
  pause.dataset.function = "legacy.engine-game-pause";
  const timeOn = check(p, "toolbar.detail.pkTimeOn", [110, 23, 20, 18]);
  label(p, t("BottomToolbar.detail.timeBlack"), [130, 22, 70, 18]);
  const timeBlack = digits(p, "toolbar.detail.pkTimeBlack", [190, 24, 23, 18]);
  label(p, t("BottomToolbar.detail.white"), [215, 22, 15, 18]);
  const timeWhite = digits(p, "toolbar.detail.pkTimeWhite", [227, 24, 23, 18]);
  const firstOn = check(p, "toolbar.detail.pkFirstPlayoutsOn", [250, 23, 20, 18]);
  label(p, t("BottomToolbar.detail.firstPlayoutsBlack"), [269, 22, 90, 18]);
  const firstBlack = digits(p, "toolbar.detail.pkFirstPlayoutsBlack", [350, 24, 50, 18]);
  label(p, t("BottomToolbar.detail.whiteColon"), [400, 22, 20, 18]);
  const firstWhite = digits(p, "toolbar.detail.pkFirstPlayoutsWhite", [415, 24, 50, 18]);
  const batchOn = check(p, "toolbar.detail.pkBatchOn", [465, 23, 20, 18]);
  label(p, t("BottomToolbar.detail.multiGames"), [485, 22, 30, 20]);
  const batch = digits(p, "toolbar.detail.pkBatch", [515, 24, 30, 18]);
  const continuePlay = check(p, "toolbar.detail.pkContinue", [545, 23, 20, 18]);
  continuePlay.title = t("BottomToolbar.detail.fixedStartHint");
  label(p, t("BottomToolbar.detail.continueGame"), [565, 22, 50, 20]);
  const fields: SharedFields = { black, white, timeOn, timeBlack, timeWhite, visitsOn, visitsBlack, visitsWhite, firstOn, firstBlack, firstWhite, batchOn, batch, continuePlay };
  pkOn.onchange = () => setEngineMatchChecked(pkOn.checked);
  // addEngineLis: the chosen side is saved at once (EnginePkIdentity.persistSide).
  black.onchange = () => void run(matchValues({ settings: { first: engineIdentity(black.selectedIndex) } }));
  white.onchange = () => void run(matchValues({ settings: { second: engineIdentity(white.selectedIndex) } }));
  const save = () => {
    const m = state.view?.settings.matches;
    if (m && !matchActive()) void run(matchValues({ session: sharedSession(fields, m.session) }));
  };
  for (const control of [timeOn, timeBlack, timeWhite, visitsOn, visitsBlack, visitsWhite, firstOn, firstBlack, firstWhite, batchOn, batch, continuePlay]) control.addEventListener("change", save);
  batch.addEventListener("input", () => {
    if (matchActive() && /^\d+$/.test(batch.value.trim())) void run({ type: "Match", op: { type: "ReviseBatchLimit", limit: Number(batch.value.trim()) } });
  });
  // btnStartPk: 开始 starts with the toolbar values, 终止 stops the batch.
  start.onclick = () => (matchActive() ? stopMatch() : void startFromToolbar());
  pause.onclick = toggleMatchPause;
  settings.onclick = () => void openEnginePkConfig(true);
  intervene.onclick = openManual;

  let engineNames = "";
  const sync = () => {
    const v = state.view;
    if (!v) return;
    const m = v.settings.matches;
    const s = m.session;
    const names = v.engine.engines.map((e, k) => `[${k + 1}]${e.name}`);
    if (names.join("\n") !== engineNames) {
      engineNames = names.join("\n");
      for (const box of [black, white]) box.replaceChildren(...names.map((n) => Object.assign(document.createElement("option"), { textContent: n })));
    }
    if (document.activeElement !== black) black.selectedIndex = names.length ? m.first_engine : -1;
    if (document.activeElement !== white) white.selectedIndex = names.length ? m.second_engine : -1;
    for (const [field, n] of [[timeBlack, s.firstSeconds], [timeWhite, s.secondSeconds], [visitsBlack, s.firstVisits], [visitsWhite, s.secondVisits], [firstBlack, s.firstMoveVisits], [firstWhite, s.secondMoveVisits], [batch, s.batchLimit]] as const) {
      const value = fieldText(Math.trunc(n));
      if (document.activeElement !== field && field.value !== value) field.value = value;
    }
    timeOn.checked = s.timeEnabled;
    visitsOn.checked = s.visitsEnabled;
    firstOn.checked = s.firstMoveVisitsEnabled;
    batchOn.checked = s.batchEnabled;
    continuePlay.checked = s.continuePosition;
    pkOn.checked = matchUi.engineMatchChecked;
    const match = v.engine_match;
    const shown = match?.score ?? matchUi.last?.score;
    score.textContent = shown ? `${shown.first_black_wins + shown.first_white_wins}:${shown.second_black_wins + shown.second_white_wins}` : "0:0";
    // enableDisabelForEngineGame and setGenmove: a running batch locks the values except 多盘;
    // Genmove模式 has no playout conditions.
    const active = !!match;
    const genmove = s.kind === "Genmove";
    for (const control of [timeOn, continuePlay, batchOn, pkOn, settings]) control.disabled = active;
    black.disabled = white.disabled = active || !pkOn.checked;
    timeBlack.disabled = timeWhite.disabled = active || !timeOn.checked;
    visitsOn.disabled = firstOn.disabled = active || genmove;
    visitsBlack.disabled = visitsWhite.disabled = active || genmove || !visitsOn.checked;
    firstBlack.disabled = firstWhite.disabled = active || genmove || !firstOn.checked;
    batch.disabled = !batchOn.checked;
    start.textContent = active ? t("BottomToolbar.detail.stop") : t("BottomToolbar.detail.start");
    pause.textContent = match?.phase === "Paused" ? t("BottomToolbar.detail.continue") : t("BottomToolbar.detail.pause");
  };
  subscribe(sync);
  onMatchUiChange(sync);
}

/** autoPlayPanel (BottomToolbar.java:1802-2006). */
function autoPlayPanel(host: HTMLElement): void {
  const p = detailPanel(host, "autoPlayPanel", 495);
  const showBlack = check(p, "toolbar.detail.showBlack", [5, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.showBlack"), [25, 0, 40, 18]);
  const showWhite = check(p, "toolbar.detail.showWhite", [5, 22, 20, 18]);
  label(p, t("BottomToolbar.detail.showWhite"), [25, 22, 40, 18]);
  showBlack.onchange = () => setCandidateSide(true, showBlack.checked);
  showWhite.onchange = () => setCandidateSide(false, showWhite.checked);
  // 自动落子 with its sides and conditions (BottomToolbar.java:1802-2006): the engine plays the
  // checked sides once a condition is met; every change is SetAutoPlay.
  const autoMove = document.createElement("div");
  autoMove.className = "bb-detail-group";
  at(p, autoMove, [60, 0, 265, 44]);
  const autoOn = check(autoMove, "toolbar.detail.autoMove", [0, 1, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.autoMove"), [20, 0, 60, 20]);
  const autoBlack = check(autoMove, "toolbar.detail.autoMoveBlack", [70, 1, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.black"), [90, 0, 20, 20]);
  const autoWhite = check(autoMove, "toolbar.detail.autoMoveWhite", [105, 1, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.white"), [125, 0, 20, 20]);
  const timeOn = check(autoMove, "toolbar.detail.autoMoveTimeOn", [145, 1, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.byTime"), [165, 0, 70, 20]);
  const time = digits(autoMove, "toolbar.detail.autoMoveTime", [230, 2, 33, 18]);
  const playoutsOn = check(autoMove, "toolbar.detail.autoMovePlayoutsOn", [0, 23, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.totalPlayouts"), [20, 22, 60, 20]);
  const playouts = digits(autoMove, "toolbar.detail.autoMovePlayouts", [75, 23, 50, 18]);
  const firstOn = check(autoMove, "toolbar.detail.autoMoveFirstPlayoutsOn", [125, 23, 20, 18]);
  label(autoMove, t("BottomToolbar.detail.firstPlayouts"), [145, 22, 70, 20]);
  const first = digits(autoMove, "toolbar.detail.autoMoveFirstPlayouts", [212, 23, 50, 18]);
  const sendAutoPlay = () => void run({ type: "Game", op: { type: "SetAutoPlay", enabled: autoOn.checked, settings: {
    black: autoBlack.checked,
    white: autoWhite.checked,
    timeEnabled: timeOn.checked,
    timeSeconds: fieldInt(time),
    playoutsEnabled: playoutsOn.checked,
    playouts: fieldInt(playouts),
    firstPlayoutsEnabled: firstOn.checked,
    firstPlayouts: fieldInt(first),
  } } });
  for (const control of [autoOn, autoBlack, autoWhite, timeOn, time, playoutsOn, playouts, firstOn, first]) control.addEventListener("change", sendAutoPlay);
  // chkAutoMain: autoPlayMain, reading txtAutoMain (seconds) at every step.
  const autoMain = check(p, "toolbar.detail.autoMain", [325, 1, 20, 18]);
  label(p, t("BottomToolbar.detail.autoMain"), [345, 0, 100, 18]);
  const mainField = digits(p, "toolbar.detail.autoMainSeconds", [445, 2, 40, 18], true);
  // chkAutoSub: auto-replay-branch (BottomToolbar.java:1857-1884); txtAutoSub is the sub board
  // player's interval in milliseconds.
  const autoSub = check(p, "toolbar.detail.autoVariation", [325, 22, 20, 18]);
  label(p, t("BottomToolbar.detail.autoVariation"), [345, 22, 100, 18]);
  const subField = digits(p, "toolbar.detail.autoVariationMillis", [445, 23, 40, 18], true);
  const interval = (field: HTMLInputElement) => {
    const n = Number(field.value);
    return field.value !== "" && n > 0 ? n : -1;
  };
  mainField.oninput = () => toolbarAutoplay.setInterval("main", interval(mainField));
  subField.oninput = () => toolbarAutoplay.setInterval("sub", Math.trunc(interval(subField)) || -1);
  autoMain.onchange = () => toolbarAutoplay.setMain(autoMain.checked);
  autoSub.onchange = () => setDisplay({ autoReplayBranch: autoSub.checked });
  const sync = () => {
    showBlack.checked = boardUi.showBlack;
    showWhite.checked = boardUi.showWhite;
    const auto = state.view?.settings.autoplay;
    if (auto) {
      autoOn.checked = auto.enabled;
      autoBlack.checked = auto.settings.black;
      autoWhite.checked = auto.settings.white;
      timeOn.checked = auto.settings.timeEnabled;
      playoutsOn.checked = auto.settings.playoutsEnabled;
      firstOn.checked = auto.settings.firstPlayoutsEnabled;
      for (const [field, n] of [[time, auto.settings.timeSeconds], [playouts, auto.settings.playouts], [first, auto.settings.firstPlayouts]] as const) {
        const value = fieldText(n);
        if (document.activeElement !== field && field.value !== value) field.value = value;
      }
    }
    autoMain.checked = toolbarAutoplay.mainRunning();
    autoSub.checked = state.view?.settings.display.autoReplayBranch ?? false;
    for (const [field, kind] of [[mainField, "main"], [subField, "sub"]] as const) {
      const value = fieldText(toolbarAutoplay.interval(kind));
      if (document.activeElement !== field && field.value !== value) field.value = value;
    }
  };
  subscribe(sync);
  onBoardUiChange(sync);
}

/** Builds the panels hidden; layoutBottomDetail shows them while 详细工具栏 is on. */
export function buildBottomDetail(host: HTMLElement): void {
  analysisPanel(host);
  engineMatchPanel(host);
  autoPlayPanel(host);
  panels = [...host.querySelectorAll<HTMLElement>(":scope > .bb-detail")];
  for (const panel of panels) panel.style.display = "none";
}

/** layoutDetailPanels: in their default order (自动分析, 引擎对战, 自动播放) from the left, wrapping
 *  to a new row when the next one does not fit; returns the toolbar height. */
export function layoutBottomDetail(host: HTMLElement, expanded: boolean): number {
  if (!expanded) {
    for (const panel of panels) panel.style.display = "none";
    return MAIN_BAR_HEIGHT;
  }
  const width = host.clientWidth;
  const available = Math.max(320, width - SIDE_PADDING * 2);
  let x = SIDE_PADDING;
  let y = MAIN_BAR_HEIGHT + 4;
  let rows = 1;
  for (const panel of panels) {
    const w = Math.min(Number(panel.dataset.width), available);
    if (x > SIDE_PADDING && x + w > width - SIDE_PADDING) {
      rows++;
      x = SIDE_PADDING;
      y += DETAIL_PANEL_HEIGHT + DETAIL_PANEL_GAP;
    }
    Object.assign(panel.style, { display: "", left: `${x}px`, top: `${y}px`, width: `${w}px`, height: `${DETAIL_PANEL_HEIGHT}px` });
    x += w + DETAIL_PANEL_GAP;
  }
  return MAIN_BAR_HEIGHT + 4 + rows * DETAIL_PANEL_HEIGHT + Math.max(0, rows - 1) * DETAIL_PANEL_GAP;
}
