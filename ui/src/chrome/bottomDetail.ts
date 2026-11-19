// 详细工具栏 (BottomToolbar.java:1556-2374, layoutDetailPanels 686-721): three panels under the main
// bar. Each panel is a group headed by its main switch, with its controls in rows of related sets;
// styles.css lays them out and the panels wrap as whole groups. 自动分析 edits the saved 自动分析
// settings and starts or stops the run, as StartAnaDialog does; 显示黑 / 显示白, 自动落子, 自动播放(大)
// and 自动变化图 drive the candidate sides, the engine playing the checked sides and the players.
// 引擎对战 edits the match's toolbar values and starts, pauses, stops and intervenes in the core's match.
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

/** The main bar's row, as styles.css draws it; the panels' area starts under it. */
const MAIN_BAR_HEIGHT = 32;

function text(content: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.textContent = content;
  return el;
}

/** A JCheckBox with its text: both in one <label>, so the text toggles the box too. */
function check(key: string, content: string): HTMLInputElement {
  const wrap = document.createElement("label");
  const box = document.createElement("input");
  box.type = "checkbox";
  box.dataset.key = key;
  wrap.append(box, content);
  return box;
}

/** A JTextField with IntDocument: only digits (with `decimal`, also a point) can be typed. The field
 *  is sized for `chars` characters. */
function digits(key: string, chars: number, decimal = false): HTMLInputElement {
  const field = document.createElement("input");
  field.type = "text";
  field.dataset.key = key;
  field.style.setProperty("--chars", String(chars));
  field.addEventListener("input", () => {
    const kept = field.value.replace(decimal ? /[^\d.]/g : /\D/g, "");
    if (kept !== field.value) field.value = kept;
  });
  return field;
}

function push(key: string, content: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.dataset.key = key;
  button.textContent = content;
  return button;
}

/** Related controls kept together on one line; a check box stands for its label, a string is a
 *  plain label. */
function group(...parts: (HTMLElement | string)[]): HTMLElement {
  const el = document.createElement("span");
  el.className = "bb-detail-group";
  for (const part of parts) {
    if (typeof part === "string") el.append(text(part));
    else el.append(part instanceof HTMLInputElement && part.type === "checkbox" ? part.parentElement! : part);
  }
  return el;
}

/** A panel: its main switch as the heading, then rows of groups. */
function detailPanel(area: HTMLElement, name: string, func: string, head: HTMLInputElement, rows: HTMLElement[][]): void {
  const panel = document.createElement("div");
  panel.className = "bb-detail";
  panel.dataset.name = name;
  panel.dataset.function = func;
  const heading = head.parentElement!;
  heading.classList.add("bb-detail-head");
  const body = document.createElement("div");
  body.className = "bb-detail-body";
  for (const groups of rows) {
    const row = document.createElement("div");
    row.className = "bb-detail-row";
    row.append(...groups);
    body.append(row);
  }
  panel.append(heading, body);
  area.append(panel);
}

const fieldText = (n: number) => (n > 0 ? String(n) : "");
const fieldInt = (field: HTMLInputElement) => (/^\d+$/.test(field.value) ? Number(field.value) : -1);

/** anaPanel (BottomToolbar.java:1574-1800). */
function analysisPanel(area: HTMLElement): void {
  const autoAnalyse = check("toolbar.detail.autoAnalyze", t("BottomToolbar.detail.autoAnalyze"));
  const black = check("toolbar.detail.anaBlack", t("BottomToolbar.detail.black"));
  const white = check("toolbar.detail.anaWhite", t("BottomToolbar.detail.white"));
  const first = digits("toolbar.detail.firstMove", 4);
  const last = digits("toolbar.detail.lastMove", 4);
  const start = push("toolbar.detail.start", t("BottomToolbar.detail.start"));
  start.dataset.function = "legacy.auto-analysis-start";
  const pause = push("toolbar.detail.pause", t("BottomToolbar.detail.pause"));
  const visitsOn = check("toolbar.detail.totalPlayoutsOn", t("BottomToolbar.detail.totalPlayouts"));
  const visits = digits("toolbar.detail.totalPlayouts", 7);
  const firstVisitsOn = check("toolbar.detail.firstPlayoutsOn", t("BottomToolbar.detail.firstPlayouts"));
  const firstVisits = digits("toolbar.detail.firstPlayouts", 7);
  const timeOn = check("toolbar.detail.byTimeOn", t("BottomToolbar.detail.byTime"));
  const time = digits("toolbar.detail.byTime", 4);
  const autoSave = check("toolbar.detail.saveKifu", t("BottomToolbar.detail.saveKifu"));
  const batchProgress = push("toolbar.detail.batchProgress", t("BottomToolbar.detail.batchProgress"));
  batchProgress.onclick = () => COMMANDS["batch-progress"].run();
  detailPanel(area, "anaPanel", "legacy.auto-analysis", autoAnalyse, [
    [group(black, white), group(t("BottomToolbar.detail.moveRange"), first, t("BottomToolbar.detail.to"), last), group(start, pause)],
    [group(visitsOn, visits), group(firstVisitsOn, firstVisits)],
    [group(timeOn, time), group(autoSave), group(batchProgress)],
  ]);

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
function engineMatchPanel(area: HTMLElement): void {
  const pkOn = check("toolbar.detail.engineMatch", t("BottomToolbar.detail.engineMatch"));
  const black = document.createElement("select");
  black.dataset.key = "toolbar.detail.engineBlack";
  const score = text("0:0");
  score.className = "bb-detail-score";
  score.dataset.key = "toolbar.detail.pkResult";
  const white = document.createElement("select");
  white.dataset.key = "toolbar.detail.engineWhite";
  const start = push("toolbar.detail.pkStart", t("BottomToolbar.detail.start"));
  start.dataset.function = "legacy.engine-game-start";
  const pause = push("toolbar.detail.pkPause", t("BottomToolbar.detail.pause"));
  pause.dataset.function = "legacy.engine-game-pause";
  const intervene = push("toolbar.detail.intervene", t("BottomToolbar.detail.intervene"));
  intervene.dataset.function = "legacy.engine-game-intervene";
  const settings = push("toolbar.detail.pkSettings", t("BottomToolbar.detail.settings"));
  settings.dataset.function = "legacy.engine-game-settings";
  const visitsOn = check("toolbar.detail.pkPlayoutsOn", t("BottomToolbar.detail.totalPlayoutsBlack"));
  const visitsBlack = digits("toolbar.detail.pkPlayoutsBlack", 7);
  const visitsWhite = digits("toolbar.detail.pkPlayoutsWhite", 7);
  const firstOn = check("toolbar.detail.pkFirstPlayoutsOn", t("BottomToolbar.detail.firstPlayoutsBlack"));
  const firstBlack = digits("toolbar.detail.pkFirstPlayoutsBlack", 7);
  const firstWhite = digits("toolbar.detail.pkFirstPlayoutsWhite", 7);
  const timeOn = check("toolbar.detail.pkTimeOn", t("BottomToolbar.detail.timeBlack"));
  const timeBlack = digits("toolbar.detail.pkTimeBlack", 4);
  const timeWhite = digits("toolbar.detail.pkTimeWhite", 4);
  const batchOn = check("toolbar.detail.pkBatchOn", t("BottomToolbar.detail.multiGames"));
  const batch = digits("toolbar.detail.pkBatch", 4);
  const continuePlay = check("toolbar.detail.pkContinue", t("BottomToolbar.detail.continueGame"));
  continuePlay.title = t("BottomToolbar.detail.fixedStartHint");
  const whiteColon = t("BottomToolbar.detail.whiteColon");
  detailPanel(area, "enginePkPanel", "legacy.engine-game", pkOn, [
    [group(t("BottomToolbar.detail.blackColon"), black, score, whiteColon, white), group(start, pause, intervene, settings)],
    [group(visitsOn, visitsBlack, whiteColon, visitsWhite), group(firstOn, firstBlack, whiteColon, firstWhite)],
    [group(timeOn, timeBlack, t("BottomToolbar.detail.white"), timeWhite), group(batchOn, batch), group(continuePlay)],
  ]);
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

/** autoPlayPanel (BottomToolbar.java:1802-2006), headed by 自动落子 as 设置模块顺序 names it. */
function autoPlayPanel(area: HTMLElement): void {
  // 自动落子 with its sides and conditions (BottomToolbar.java:1802-2006): the engine plays the
  // checked sides once a condition is met; every change is SetAutoPlay.
  const autoOn = check("toolbar.detail.autoMove", t("BottomToolbar.detail.autoMove"));
  const autoBlack = check("toolbar.detail.autoMoveBlack", t("BottomToolbar.detail.black"));
  const autoWhite = check("toolbar.detail.autoMoveWhite", t("BottomToolbar.detail.white"));
  const timeOn = check("toolbar.detail.autoMoveTimeOn", t("BottomToolbar.detail.byTime"));
  const time = digits("toolbar.detail.autoMoveTime", 4);
  const playoutsOn = check("toolbar.detail.autoMovePlayoutsOn", t("BottomToolbar.detail.totalPlayouts"));
  const playouts = digits("toolbar.detail.autoMovePlayouts", 7);
  const firstOn = check("toolbar.detail.autoMoveFirstPlayoutsOn", t("BottomToolbar.detail.firstPlayouts"));
  const first = digits("toolbar.detail.autoMoveFirstPlayouts", 7);
  const showBlack = check("toolbar.detail.showBlack", t("BottomToolbar.detail.showBlack"));
  const showWhite = check("toolbar.detail.showWhite", t("BottomToolbar.detail.showWhite"));
  // chkAutoMain: autoPlayMain, reading txtAutoMain (seconds) at every step.
  const autoMain = check("toolbar.detail.autoMain", t("BottomToolbar.detail.autoMain"));
  autoMain.dataset.function = "legacy.auto-main";
  const mainField = digits("toolbar.detail.autoMainSeconds", 5, true);
  // chkAutoSub: auto-replay-branch (BottomToolbar.java:1857-1884); txtAutoSub is the sub board
  // player's interval in milliseconds.
  const autoSub = check("toolbar.detail.autoVariation", t("BottomToolbar.detail.autoVariation"));
  autoSub.dataset.function = "legacy.auto-sub";
  const subField = digits("toolbar.detail.autoVariationMillis", 5, true);
  detailPanel(area, "autoPlayPanel", "legacy.auto-play", autoOn, [
    [group(autoBlack, autoWhite), group(timeOn, time), group(playoutsOn, playouts), group(firstOn, first)],
    [group(showBlack, showWhite), group(autoMain, mainField), group(autoSub, subField)],
  ]);

  showBlack.onchange = () => setCandidateSide(true, showBlack.checked);
  showWhite.onchange = () => setCandidateSide(false, showWhite.checked);
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

/** Builds the panels in their area, hidden; layoutBottomDetail shows them while 详细工具栏 is on. */
export function buildBottomDetail(host: HTMLElement): void {
  const area = document.createElement("div");
  area.className = "bb-details";
  area.style.display = "none";
  host.append(area);
  analysisPanel(area);
  engineMatchPanel(area);
  autoPlayPanel(area);
}

/** layoutDetailPanels: the panels in their 设置模块顺序 places (equal places keep 自动分析, 引擎对战,
 *  自动播放), wrapping as whole groups when the next one does not fit; returns the toolbar height,
 *  the main bar's row plus the area as laid out at the bar's current width. */
export function layoutBottomDetail(host: HTMLElement, expanded: boolean): number {
  const area = host.querySelector<HTMLElement>(":scope > .bb-details")!;
  area.style.display = expanded ? "" : "none";
  if (!expanded) return MAIN_BAR_HEIGHT;
  const order = state.view?.settings.detail_panel_order ?? { autoAnalysis: 0, engineMatch: 1, autoPlay: 2 };
  const place: Record<string, number> = { anaPanel: order.autoAnalysis, enginePkPanel: order.engineMatch, autoPlayPanel: order.autoPlay };
  for (const panel of area.querySelectorAll<HTMLElement>(":scope > .bb-detail")) panel.style.order = String(place[panel.dataset.name!]);
  return MAIN_BAR_HEIGHT + area.offsetHeight;
}
