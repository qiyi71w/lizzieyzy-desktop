// 人机对弈 dialogs: 新对局(Genmove模式) (NewGameDialog.java), 新对局(分析模式) (NewAnaGameDialog.java),
// 修改AI用时 (SetAiTimes.java) and 设置随机开局 (SetAnaGameRandomStart.java). Each holds a draft of the
// saved settings; 确定 sends one Game command and Rust owns the game and the saved values.
import { run, state } from "../app";
import type { AiTimeMode, AiTimeSettings, AutoPlaySettings, GameCommand, GameSettings, HumanSlContinuation, KataTimeType } from "../contract/generated";
import { icon } from "../design/icons";
import { formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { actions, button, primaryButton } from "../engine/controls";
import { plainJava } from "../engine/tuningText";
import { product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import { endHumanSlFirst, registerHumanSlContinuation } from "../humansl/state";
import "./game.css";

const HANDICAPS = [0, 2, 3, 4, 5, 6, 7, 8, 9];
const KATA_TYPES: KataTimeType[] = ["Byoyomi", "Fischer", "Absolute"];

function span(text: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.textContent = text;
  return el;
}

/** A JCheckBox (or a radio button), with its own text when `text` is given. */
function check(checked: boolean, key: string, text = ""): { box: HTMLInputElement; wrap: HTMLLabelElement } {
  const box = document.createElement("input");
  box.type = "checkbox";
  box.checked = checked;
  box.dataset.key = key;
  const wrap = document.createElement("label");
  wrap.className = "game-check";
  wrap.append(box);
  if (text) wrap.append(text);
  return { box, wrap };
}

function select(options: string[], index: number, key: string): HTMLSelectElement {
  const el = document.createElement("select");
  el.dataset.key = key;
  for (const text of options) {
    const o = document.createElement("option");
    o.textContent = text;
    el.append(o);
  }
  el.selectedIndex = options.length ? Math.min(Math.max(0, index), options.length - 1) : -1;
  return el;
}

/** A JTextField; `filter` is IntDocument (digits) or KomiDocument (a signed decimal). */
function field(value: string, key: string, filter: "int" | "komi" | null = null): HTMLInputElement {
  const el = document.createElement("input");
  el.type = "text";
  el.value = value;
  el.dataset.key = key;
  if (filter) {
    const allowed = filter === "int" ? /\D/g : /[^\d.-]/g;
    el.addEventListener("input", () => {
      const kept = el.value.replace(allowed, "");
      if (kept !== el.value) el.value = kept;
    });
  }
  return el;
}

function inline(...items: HTMLElement[]): HTMLDivElement {
  const el = document.createElement("div");
  el.className = "game-inline";
  el.append(...items);
  return el;
}

/** Utils.parseTextToInt: Integer.parseInt of the trimmed text, else the default. */
function textInt(el: HTMLInputElement, fallback: number): number {
  return parseJavaInt(el.value.trim()) ?? fallback;
}

/** Utils.parseTextToDouble / Double.parseDouble of the trimmed text, else the default. */
function textDouble(el: HTMLInputElement, fallback: number): number {
  const s = el.value.trim();
  return /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(s) ? Number(s) : fallback;
}

/** DecimalFormat("0").parse: the leading integer of the text, null when it has none. */
function leadingInt(text: string): number | null {
  const m = /^\s*([+-]?\d+)/.exec(text);
  return m ? Number.parseInt(m[1], 10) : null;
}

/** DecimalFormat("#0.0").parse (NewAnaGameDialog FORMAT_KOMI): the leading number. */
function leadingDouble(text: string): number | null {
  const m = /^\s*([+-]?(\d+\.?\d*|\.\d+))/.exec(text);
  return m ? Number(m[1]) : null;
}

function game(op: GameCommand) {
  return run({ type: "Game", op });
}

/** The dialogs' form: a label column and a control column. A check box that turns its control on
 *  takes the label column; a check box row of its own starts there and spans both. */
function form() {
  const body = document.createElement("div");
  body.className = "game-dialog";
  const grid = document.createElement("div");
  grid.className = "game-form";
  body.append(grid);
  return {
    body,
    row: (left: HTMLElement, right: HTMLElement) => grid.append(left, right),
    wide: (item: HTMLElement) => {
      item.classList.add("game-wide");
      grid.append(item);
    },
    /** A group's title, or a hairline before an untitled group. */
    group: (title = "") => {
      const el = document.createElement(title ? "h3" : "hr");
      el.textContent = title;
      grid.append(el);
    },
    /** A hint under the control above, or (`checkRow`) under the text of the check box row above. */
    hint: (text: string, checkRow = false) => {
      const p = document.createElement("p");
      p.className = checkRow ? "engine-hint game-hint-check" : "engine-hint game-hint";
      p.textContent = text;
      grid.append(p);
    },
  };
}

/** 选择引擎: "[n]name" of every configured engine, the current one selected. */
function engineSelect(): HTMLSelectElement {
  const v = state.view!;
  return select(v.engine.engines.map((e, k) => `[${k + 1}]${e.name}`), v.engine.foreground?.index ?? 0, "game.engine");
}

/** The engine to switch to first (switchEngine when the choice is not the current engine). */
function chosenEngine(el: HTMLSelectElement): number | null {
  return el.selectedIndex === state.view?.engine.foreground?.index ? null : el.selectedIndex;
}

/** 限制我的用时 with 保留时间 / 读秒 / 次数; both "无" is refused as Byoyomi.emptyTimeHint. */
function myTime(g: GameSettings) {
  const limit = check(g.limitMyTime, "game.limitMyTime", t("Byoyomi.newGame.limitMyTime"));
  const save = select([t("Byoyomi.none"), "5", "10", "20", "30", "60", "120", "180"], g.mySaveTime, "game.mySaveTime");
  const byo = select([t("Byoyomi.none"), "10", "15", "20", "30", "40", "60", "120"], g.myByoyomiSeconds, "game.myByoyomiSeconds");
  const times = select(["1", "2", "3", "5", "10", "20"], g.myByoyomiTimes, "game.myByoyomiTimes");
  save.onchange = () => {
    if (save.selectedIndex === 0 && byo.selectedIndex === 0) {
      void message(t("Message.title"), t("Byoyomi.emptyTimeHint"));
      save.selectedIndex = 1;
    }
  };
  byo.onchange = () => {
    if (byo.selectedIndex === 0) {
      if (save.selectedIndex === 0) {
        void message(t("Message.title"), t("Byoyomi.emptyTimeHint"));
        byo.selectedIndex = 1;
        return;
      }
      times.disabled = true;
    } else times.disabled = false;
  };
  limit.box.onchange = () => (save.disabled = byo.disabled = times.disabled = !limit.box.checked);
  save.disabled = byo.disabled = times.disabled = !limit.box.checked;
  if (limit.box.checked && byo.selectedIndex === 0) times.disabled = true;
  const panel = inline(span(t("Byoyomi.newGame.saveTime")), save, span(t("Byoyomi.newGame.byoyomi")), byo, span(t("Byoyomi.newGame.byoyomiTimes")), times);
  const read = () => ({ limitMyTime: limit.box.checked, mySaveTime: save.selectedIndex, myByoyomiSeconds: byo.selectedIndex, myByoyomiTimes: times.selectedIndex });
  return { limit, panel, read };
}

/** KataGo专用时间: 读秒制 shows 读秒 and 次数, 加秒制 每手增加, 包干制 only 保留时间. */
function kataTime(a: AiTimeSettings) {
  const type = select([t("NewGameDialog.kataTime.byoyomi"), t("NewGameDialog.kataTime.fisher"), t("NewGameDialog.kataTime.absolute")], KATA_TYPES.indexOf(a.kataType), "game.kataType");
  const mins = field(String(a.kataMainMinutes), "game.kataMainMinutes", "int");
  const secs = field(String(a.kataByoyomiSeconds), "game.kataByoyomiSeconds", "int");
  const times = field(String(a.kataByoyomiTimes), "game.kataByoyomiTimes", "int");
  const inc = field(String(a.kataFischerIncrement), "game.kataFischerIncrement", "int");
  const lSecs = span(t("Byoyomi.newGame.byoyomi")), lTimes = span(t("Byoyomi.newGame.byoyomiTimes")), lInc = span(t("NewGameDialog.kataTime.increment"));
  const show = () => {
    const k = type.selectedIndex;
    for (const el of [lSecs, secs, lTimes, times]) el.hidden = k !== 0;
    for (const el of [lInc, inc]) el.hidden = k !== 1;
  };
  type.onchange = show;
  show();
  const panel = inline(type, span(t("Byoyomi.newGame.saveTime")), mins, lSecs, secs, lTimes, times, lInc, inc);
  const enable = (on: boolean) => { for (const el of [type, mins, secs, times, inc]) el.disabled = !on; };
  const read = (base: AiTimeSettings): AiTimeSettings => ({
    ...base,
    kataType: KATA_TYPES[type.selectedIndex] ?? base.kataType,
    kataMainMinutes: textInt(mins, base.kataMainMinutes),
    kataByoyomiSeconds: textInt(secs, base.kataByoyomiSeconds),
    kataByoyomiTimes: textInt(times, base.kataByoyomiTimes),
    kataFischerIncrement: textInt(inc, base.kataFischerIncrement),
  });
  return { panel, enable, read };
}

/** A ButtonGroup of check boxes: choosing one clears the others; the chosen one stays. */
function exclusive(boxes: HTMLInputElement[], changed: () => void): void {
  for (const box of boxes) {
    box.onchange = () => {
      box.checked = true;
      for (const other of boxes) if (other !== box) other.checked = false;
      changed();
    };
  }
}

/** btnAboutAdvTime: Utils.showHtmlMessageModal(AdvanceTimeSettings.title, .describe). */
function advancedHelp(): HTMLButtonElement {
  const help = button("", () => void message(t("AdvanceTimeSettings.title"), plainJava(t("AdvanceTimeSettings.describe"))));
  help.append(icon("help"));
  help.className = "game-help";
  help.title = t("AdvanceTimeSettings.title");
  help.setAttribute("aria-label", t("AdvanceTimeSettings.title"));
  help.dataset.key = "game.advancedTimeHelp";
  return help;
}

/** 自动保存棋谱's place: "Lizzie目录" + File.separator + "MyGames文件夹". */
function autoSaveText(): string {
  const separator = navigator.userAgent.includes("Windows") ? "\\" : "/";
  return `${t("NewAnaGameDialog.chkAutoSaveDirectory")}${separator}${t("NewAnaGameDialog.chkAutoSaveFolder")}`;
}

function handicapRow(g: GameSettings) {
  const cont = check(g.continuePlay, "game.continuePlay", t("NewGameDialog.ContinuePlay"));
  const handicap = select(HANDICAPS.map(String), HANDICAPS.indexOf(g.handicap), "game.handicap");
  const free = check(g.freeHandicap, "game.freeHandicap", t("NewGameDialog.useFreeHandicap"));
  // 接着当前局面下 disables 让子.
  cont.box.addEventListener("change", () => (handicap.disabled = cont.box.checked));
  handicap.disabled = cont.box.checked;
  return { cont, handicap, free };
}

/** 显示分析选点(黑) and (白) side by side, each with its stone. */
function shownSides(g: GameSettings) {
  const black = check(g.showBlack, "game.showBlack", t("NewAnaGameDialog.showBlack"));
  const white = check(g.showWhite, "game.showWhite", t("NewAnaGameDialog.showWhite"));
  black.box.after(icon("stone-black"));
  white.box.after(icon("stone-white"));
  const row = inline(black.wrap, white.wrap);
  return { black, white, row };
}

/** 确定 (initButtonBar's OK) and 取消; Escape also closes without a game. */
function footer(body: HTMLElement, label: string, apply: () => Promise<void>, close: () => void): void {
  const ok = primaryButton(label, () => void apply());
  ok.dataset.key = "game-dialog-ok";
  const cancel = button(product("cancel"), close);
  cancel.dataset.key = "game.cancel";
  actions(body, ok, cancel);
}

/** startNewGameReserved / startAnalyzeGameDialogReserved: during an AI 陪练 session the dialog is
 *  the session exit's continuation and opens once the core publishes it (deferUntilHumanSlExit,
 *  LizzieFrame.java:3710-3712, 13459-13468); a running game ends (stopAiPlayingAndPolicy) and
 *  running analysis pauses while the dialog is open. The returned function resumes that analysis
 *  when the dialog is cancelled; null means no dialog opens now. */
async function enterNewGame(continuation: HumanSlContinuation): Promise<(() => Promise<void>) | null> {
  if (!(await endHumanSlFirst(continuation))) return null;
  if (state.view?.game) await game({ type: "End" });
  const analysing = !!state.view && !state.view.engine.paused && !state.view.game;
  if (analysing) await run({ type: "ToggleAnalysis" });
  return async () => {
    if (analysing && state.view?.engine.paused && !state.view.game) await run({ type: "ToggleAnalysis" });
  };
}

/** NewGameDialog (Genmove模式): the game, then the four AI time choices, then the options. */
export async function openNewGame(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const leave = await enterNewGame("NewGame");
  if (!leave) return;
  let started = false;
  const g = v.settings.game;
  const a = g.aiTime;
  const f = form();
  const engine = engineSelect();
  f.row(span(t("NewAnaGameDialog.chooseEngine")), engine);
  const side = select([t("NewGameDialog.playBlack"), t("NewGameDialog.playWhite")], g.playBlack ? 0 : 1, "game.side");
  f.row(span(t("NewGameDialog.chooseBlackWhite")), side);
  const { cont, handicap, free } = handicapRow(g);
  f.wide(cont.wrap);
  f.row(span(t("NewGameDialog.Handicap")), inline(handicap, free.wrap));
  const komi = field(javaDouble(g.komi), "game.komi", "komi");
  f.row(span(t("NewGameDialog.Komi")), komi);
  const mine = myTime(g);
  f.row(mine.limit.wrap, mine.panel);
  // The four AI time choices (chkTimeGroup) and the KataGo visit limits.
  f.group(t("SetAiTimes.title"));
  const normal = check(a.mode === "Fixed", "game.timeFixed", t("NewGameDialog.time"));
  const time = field(String(a.fixedSeconds), "game.fixedSeconds", "int");
  f.row(normal.wrap, time);
  const adv = check(a.mode === "Advanced", "game.timeAdvanced", t("NewGameDialog.lblAdvanceTime"));
  const advText = field(a.advanced, "game.advancedTime");
  f.row(inline(adv.wrap, advancedHelp()), advText);
  const kata = check(a.mode === "KataGo", "game.timeKataGo", t("NewGameDialog.kataTime"));
  const kataPanel = kataTime(a);
  f.row(kata.wrap, kataPanel.panel);
  const limits = check(g.kataLimits, "game.kataLimits", t("NewGameDialog.chkUseKataAdvSettings"));
  const visits = field(g.kataVisits > 0 ? String(g.kataVisits) : "", "game.kataVisits", "int");
  const playouts = field(g.kataPlayouts > 0 ? String(g.kataPlayouts) : "", "game.kataPlayouts", "int");
  const limitsChanged = () => (visits.disabled = playouts.disabled = !limits.box.checked);
  limits.box.onchange = limitsChanged;
  limitsChanged();
  f.row(limits.wrap, inline(span(t("NewGameDialog.kataVisits")), visits, span(t("NewGameDialog.kataPlayouts")), playouts));
  f.hint(t("NewGameDialog.lblVersionRequire"));
  const owned = check(a.mode === "EngineOwned", "game.timeEngineOwned", t("NewGameDialog.engineOwnedTime"));
  f.wide(owned.wrap);
  const timeChanged = () => {
    kataPanel.enable(kata.box.checked);
    advText.disabled = !adv.box.checked;
    time.disabled = kata.box.checked || adv.box.checked || owned.box.checked;
  };
  exclusive([normal.box, kata.box, adv.box, owned.box], timeChanged);
  timeChanged();
  f.group();
  const ponder = check(g.playPonder, "game.playPonder", t("NewGameDialog.chkPonder"));
  f.wide(ponder.wrap);
  f.hint(t("NewGameDialog.chkPonderDescribe"), true);
  const playMode = check(g.usePlayMode, "game.usePlayMode", t("NewGameDialog.chkUsePlayMode"));
  f.wide(playMode.wrap);
  f.hint(t("NewGameDialog.chkUsePlayModeDescribe"), true);
  const { black, white, row: sides } = shownSides(g);
  f.wide(sides);
  const autoSave = check(g.autoSave, "game.autoSave", t("NewAnaGameDialog.lblAutoSave"));
  f.wide(autoSave.wrap);
  f.hint(autoSaveText(), true);
  const dialog = formPanel(t("NewGameDialog.title"), f.body);
  footer(f.body, t("NewGameDialog.okButton"), async () => {
    if (engine.selectedIndex < 0) {
      await message(t("Message.title"), t("NewAnaGameDialog.noEngineHint"));
      return;
    }
    const mode: AiTimeMode = owned.box.checked ? "EngineOwned" : adv.box.checked ? "Advanced" : kata.box.checked ? "KataGo" : "Fixed";
    const now = state.view?.settings.game ?? g;
    let fixedSeconds = now.aiTime.fixedSeconds;
    if (mode === "Fixed") {
      // apply(): a ParseException of the fixed seconds hides the mistake and keeps the dialog.
      const n = leadingInt(time.value.trim());
      if (n === null) return;
      fixedSeconds = n;
    }
    const komiText = komi.value.trim();
    const settings: GameSettings = {
      ...now,
      playBlack: side.selectedIndex === 0,
      continuePlay: cont.box.checked,
      handicap: HANDICAPS[handicap.selectedIndex] ?? 0,
      freeHandicap: free.box.checked,
      komi: /^[+-]?(\d+\.?\d*|\.\d+)$/.test(komiText) ? Number(komiText) : 7.5,
      ...mine.read(),
      aiTime: { ...kataPanel.read(now.aiTime), mode, fixedSeconds, advanced: advText.value },
      kataLimits: limits.box.checked,
      kataVisits: textInt(visits, -1),
      kataPlayouts: textInt(playouts, -1),
      playPonder: ponder.box.checked,
      usePlayMode: playMode.box.checked,
      showBlack: black.box.checked,
      showWhite: white.box.checked,
      autoSave: autoSave.box.checked,
    };
    const result = await game({ type: "Start", kind: "Genmove", settings, autoplay: null, engine: chosenEngine(engine) });
    started = !!result && result.type !== "Refused";
    if (started) await dialog.close();
  }, () => void dialog.close());
  await dialog.closed;
  if (!started) await leave();
}

/** 落子条件 text: the toolbar's 按时间 / 总计算量 / 首位计算量 field ("" without a value). */
const conditionText = (n: number) => (n > 0 ? String(n) : "");

/** NewAnaGameDialog (分析模式): the game, then the move conditions, then the options. */
export async function openNewAnaGame(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const leave = await enterNewGame("NewAnalysisGame");
  if (!leave) return;
  let started = false;
  const g = v.settings.game;
  const auto = v.settings.autoplay.settings;
  const f = form();
  const engine = engineSelect();
  f.row(span(t("NewAnaGameDialog.chooseEngine")), engine);
  const side = select([t("NewGameDialog.playBlack"), t("NewGameDialog.playWhite")], g.playBlack ? 0 : 1, "game.side");
  f.row(span(t("NewGameDialog.chooseBlackWhite")), side);
  const { cont, handicap, free } = handicapRow(g);
  f.wide(cont.wrap);
  f.row(span(t("NewGameDialog.Handicap")), inline(handicap, free.wrap));
  const komi = field(javaDouble(g.komi), "game.komi", "komi");
  f.row(span(t("NewGameDialog.Komi")), komi);
  const mine = myTime(g);
  f.row(mine.limit.wrap, mine.panel);
  // 落子条件: each box and its field (the field holds the toolbar text even while unchecked).
  f.group(t("SetAiTimes.title"));
  const pureNet = check(g.pureNet, "game.pureNet", t("NewAnaGameDialog.lblAiUsePureNet"));
  f.wide(pureNet.wrap);
  const useTime = check(auto.timeEnabled, "game.moveTimeOn", t("NewAnaGameDialog.moveTime"));
  const time = field(String(g.aiTime.fixedSeconds), "game.moveTime", "int");
  f.row(useTime.wrap, time);
  const usePlayouts = check(auto.playoutsEnabled, "game.movePlayoutsOn", t("NewAnaGameDialog.moveVisits"));
  const playouts = field(conditionText(auto.playouts), "game.movePlayouts", "int");
  f.row(usePlayouts.wrap, playouts);
  const useFirst = check(auto.firstPlayoutsEnabled, "game.moveFirstPlayoutsOn", t("NewAnaGameDialog.firstVisits"));
  const first = field(conditionText(auto.firstPlayouts), "game.moveFirstPlayouts", "int");
  f.row(useFirst.wrap, first);
  useTime.box.onchange = () => (time.disabled = !useTime.box.checked);
  usePlayouts.box.onchange = () => (playouts.disabled = !usePlayouts.box.checked);
  useFirst.box.onchange = () => (first.disabled = !useFirst.box.checked);
  // 纯网络 disables the three fields; turning it off enables each by its box.
  const pureChanged = () => {
    time.disabled = pureNet.box.checked || !useTime.box.checked;
    playouts.disabled = pureNet.box.checked || !usePlayouts.box.checked;
    first.disabled = pureNet.box.checked || !useFirst.box.checked;
  };
  pureNet.box.onchange = pureChanged;
  pureChanged();
  f.group();
  const ponder = check(g.playPonder, "game.playPonder", t("NewAnaGameDialog.ponder"));
  f.wide(ponder.wrap);
  const random = button(t("NewAnaGameDialog.btnRandomStart"), () => void openRandomStart());
  random.dataset.key = "game.randomStart";
  f.row(span(t("NewAnaGameDialog.random")), random);
  const startMove = field(String(g.resignStartMove), "game.resignStartMove");
  const resignMove = field(String(g.resignMove), "game.resignMove");
  const resignPercent = field(String(g.resignPercent), "game.resignPercent");
  f.row(span(t("NewAnaGameDialog.resign")), inline(span(t("NewAnaGameDialog.resign0")), startMove, span(t("NewAnaGameDialog.resign1")), resignMove, span(t("NewAnaGameDialog.resign2")), resignPercent, span("%")));
  const wrn = check(g.disableWrn, "game.disableWrn", t("NewAnaGameDialog.lblDisableWRNInGame"));
  f.wide(wrn.wrap);
  const playMode = check(g.usePlayMode, "game.usePlayMode", t("NewAnaGameDialog.chkUsePlayMode"));
  f.wide(playMode.wrap);
  const { black, white, row: sides } = shownSides(g);
  f.wide(sides);
  const autoSave = check(g.autoSave, "game.autoSave", t("NewAnaGameDialog.lblAutoSave"));
  f.wide(autoSave.wrap);
  f.hint(autoSaveText(), true);
  const dialog = formPanel(t("NewAnaGameDialog.title"), f.body);
  footer(f.body, t("NewAnaGameDialog.okButton"), async () => {
    if (engine.selectedIndex < 0) {
      await message(t("Message.title"), t("NewAnaGameDialog.noEngineHint"));
      return;
    }
    const now = state.view?.settings.game ?? g;
    const playBlack = side.selectedIndex === 0;
    // apply(): a value above 0 keeps its box's choice; none turns the condition off.
    const seconds = leadingInt(time.value.trim()) ?? 0;
    const visits = leadingInt(playouts.value.trim()) ?? 0;
    const firstVisits = leadingInt(first.value.trim()) ?? 0;
    const autoplay: AutoPlaySettings = {
      black: !playBlack,
      white: playBlack,
      timeEnabled: seconds > 0 && useTime.box.checked,
      timeSeconds: seconds > 0 ? seconds : -1,
      playoutsEnabled: visits > 0 && usePlayouts.box.checked,
      playouts: visits > 0 ? visits : -1,
      firstPlayoutsEnabled: firstVisits > 0 && useFirst.box.checked,
      firstPlayouts: firstVisits > 0 ? firstVisits : -1,
    };
    const settings: GameSettings = {
      ...now,
      playBlack,
      continuePlay: cont.box.checked,
      handicap: HANDICAPS[handicap.selectedIndex] ?? 0,
      freeHandicap: free.box.checked,
      komi: leadingDouble(komi.value) ?? 7.5,
      ...mine.read(),
      aiTime: { ...now.aiTime, fixedSeconds: seconds > 0 ? seconds : now.aiTime.fixedSeconds },
      playPonder: ponder.box.checked,
      pureNet: pureNet.box.checked,
      resignStartMove: textInt(startMove, now.resignStartMove),
      resignMove: textInt(resignMove, now.resignMove),
      resignPercent: textDouble(resignPercent, now.resignPercent),
      disableWrn: wrn.box.checked,
      usePlayMode: playMode.box.checked,
      showBlack: black.box.checked,
      showWhite: white.box.checked,
      autoSave: autoSave.box.checked,
    };
    const result = await game({ type: "Start", kind: "Analysis", settings, autoplay, engine: chosenEngine(engine) });
    started = !!result && result.type !== "Refused";
    if (started) await dialog.close();
  }, () => void dialog.close());
  await dialog.closed;
  if (!started) await leave();
}

/** txtFieldValue: blank or ten or more characters read as 0. */
function fieldValue(el: HTMLInputElement): number {
  const s = el.value.trim();
  if (s === "" || s.length >= String(2 ** 31 - 1).length) return 0;
  return Number.parseInt(s, 10) || 0;
}

/** SetAiTimes: 允许AI后台思考, then the Genmove模式 time choices and the 分析模式 conditions. */
export async function openSetAiTime(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const a = v.settings.game.aiTime;
  const auto = v.settings.autoplay.settings;
  const f = form();
  const radio = (text: string, checked: boolean, key: string) => {
    const r = check(checked, key, text);
    r.box.type = "radio";
    r.box.name = "game-ai-ponder";
    return r;
  };
  const yes = radio(t("SetAiTimes.rdoPonder"), v.settings.game.playPonder, "game.ponderYes");
  const no = radio(t("SetAiTimes.rdoNoPonder"), !v.settings.game.playPonder, "game.ponderNo");
  f.row(span(t("SetAiTimes.lblPonder")), inline(yes.wrap, no.wrap));
  f.group(t("SetAiTimes.lblGenmoveMode"));
  const normal = check(a.mode === "Fixed", "game.timeFixed", t("SetAiTimes.lblGenmoveTime"));
  const setTime = field(String(a.fixedSeconds), "game.fixedSeconds", "int");
  f.row(normal.wrap, setTime);
  const adv = check(a.mode === "Advanced", "game.timeAdvanced", t("SetAiTimes.lblAdvTime"));
  const advText = field(a.advanced, "game.advancedTime");
  // `txtAdvanceTime.setEditable(false)` unless 高级时间 was saved: choosing it here enables the
  // field but it stays read-only (SetAiTimes.java:393-394).
  if (a.mode !== "Advanced") advText.readOnly = true;
  f.row(inline(adv.wrap, advancedHelp()), advText);
  const kata = check(a.mode === "KataGo", "game.timeKataGo", t("SetAiTimes.lblKataTime"));
  const kataPanel = kataTime(a);
  f.row(kata.wrap, kataPanel.panel);
  const owned = check(a.mode === "EngineOwned", "game.timeEngineOwned", t("NewGameDialog.engineOwnedTime"));
  f.wide(owned.wrap);
  const timeChanged = () => {
    kataPanel.enable(kata.box.checked);
    advText.disabled = !adv.box.checked;
    setTime.disabled = kata.box.checked || adv.box.checked || owned.box.checked;
  };
  exclusive([normal.box, kata.box, adv.box, owned.box], timeChanged);
  timeChanged();
  f.group(t("SetAiTimes.analyzeMode"));
  const anaTime = field(auto.timeEnabled ? conditionText(auto.timeSeconds) : "", "game.anaTime");
  f.row(span(t("SetAiTimes.lblTime")), anaTime);
  const anaPlayouts = field(auto.playoutsEnabled ? conditionText(auto.playouts) : "", "game.anaPlayouts");
  f.row(span(t("SetAiTimes.lblPo")), anaPlayouts);
  const anaFirst = field(auto.firstPlayoutsEnabled ? conditionText(auto.firstPlayouts) : "", "game.anaFirstPlayouts");
  f.row(span(t("SetAiTimes.lblFirstPo")), anaFirst);
  const dialog = formPanel(t("SetAiTimes.title"), f.body);
  const ok = primaryButton(t("SetAiTimes.okButton"), () => void apply());
  ok.dataset.key = "game-ai-time-ok";
  const cancel = button(t("SetAiTimes.cancel"), () => void dialog.close());
  cancel.dataset.key = "game.aiTimeCancel";
  actions(f.body, ok, cancel);
  // getRootPane().setDefaultButton(okButton).
  f.body.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault();
      void apply();
    }
  });
  setTime.addEventListener("input", () => setTime.classList.remove("invalid"));
  async function apply(): Promise<void> {
    // checkMove: 每手用时 must be above 0 whichever time is chosen.
    const seconds = fieldValue(setTime);
    if (seconds <= 0) {
      setTime.title = t("LizzieChangeMove.txtMoveNumber.error");
      setTime.classList.add("invalid");
      return;
    }
    const now = state.view?.settings ?? v!.settings;
    const base = now.game.aiTime;
    const mode: AiTimeMode = owned.box.checked ? "EngineOwned" : adv.box.checked ? "Advanced" : kata.box.checked ? "KataGo" : "Fixed";
    const time: AiTimeSettings = {
      ...kataPanel.read(base),
      mode,
      fixedSeconds: seconds,
      advanced: mode === "Advanced" ? advText.value : base.advanced,
    };
    // A value above 0 turns its condition on with that value, anything else turns it off.
    const condition = (el: HTMLInputElement) => leadingInt(el.value.trim()) ?? 0;
    const t1 = condition(anaTime), p1 = condition(anaPlayouts), f1 = condition(anaFirst);
    const cur = now.autoplay.settings;
    const autoplay: AutoPlaySettings = {
      ...cur,
      timeEnabled: t1 > 0,
      timeSeconds: t1 > 0 ? t1 : cur.timeSeconds,
      playoutsEnabled: p1 > 0,
      playouts: p1 > 0 ? p1 : cur.playouts,
      firstPlayoutsEnabled: f1 > 0,
      firstPlayouts: f1 > 0 ? f1 : cur.firstPlayouts,
    };
    const result = await game({ type: "SetAiTime", time, ponder: yes.box.checked, autoplay });
    if (result && result.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

/** SetAnaGameRandomStart: 启用, 前 N 手, the win rate and playouts tolerances. */
export async function openRandomStart(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const r = v.settings.game.randomStart;
  const f = form();
  const enable = check(r.enabled, "game.randomEnabled", t("SetAnaGameRandomStart.chkEnable"));
  f.wide(enable.wrap);
  const moves = field(String(r.moves), "game.randomMoves");
  f.row(span(t("SetAnaGameRandomStart.lblFirst.text")), inline(moves, span(t("SetAnaGameRandomStart.lblMove.text"))));
  const winrate = field(String(r.winrateDiff), "game.randomWinrate");
  f.row(span(t("SetAnaGameRandomStart.lblWinrate.text")), inline(winrate, span("%")));
  const playouts = field(String(r.playoutsDiff), "game.randomPlayouts");
  f.row(span(t("SetAnaGameRandomStart.lblVisits.text")), inline(playouts, span("%")));
  const enabled = () => (moves.disabled = winrate.disabled = playouts.disabled = !enable.box.checked);
  enable.box.onchange = enabled;
  enabled();
  const dialog = formPanel(t("SetAnaGameRandomStart.title"), f.body);
  const ok = primaryButton(t("SetAnaGameRandomStart.apply"), () => void apply());
  ok.dataset.key = "game-random-ok";
  const cancel = button(t("SetAnaGameRandomStart.cancel"), () => void dialog.close());
  cancel.dataset.key = "game.randomCancel";
  actions(f.body, ok, cancel);
  async function apply(): Promise<void> {
    // applyChange: Integer.parseInt / Double.parseDouble, else SetAnaGameRandomStart.wrongFormat.
    const n = parseJavaInt(moves.value);
    const w = textDouble(winrate, Number.NaN), p = textDouble(playouts, Number.NaN);
    if (n === null || !Number.isFinite(w) || !Number.isFinite(p)) {
      await message(t("Message.title"), t("SetAnaGameRandomStart.wrongFormat"));
      return;
    }
    const result = await game({ type: "SetRandomStart", settings: { enabled: enable.box.checked, moves: n, winrateDiff: w, playoutsDiff: p } });
    if (result && result.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

// The dialogs an AI 陪练 exit opens afterwards (deferUntilHumanSlExit(this::startNewGame) /
// (this::startAnalyzeGameDialog)).
registerHumanSlContinuation("NewGame", () => void openNewGame());
registerHumanSlContinuation("NewAnalysisGame", () => void openNewAnaGame());
