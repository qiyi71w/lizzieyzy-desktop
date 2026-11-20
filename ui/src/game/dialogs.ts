// 人机对弈 dialogs: 新对局(Genmove模式) (NewGameDialog.java), 新对局(分析模式) (NewAnaGameDialog.java),
// 修改AI用时 (SetAiTimes.java) and 设置随机开局 (SetAnaGameRandomStart.java). Each holds a draft of the
// saved settings; 确定 sends one Game command and Rust owns the game and the saved values.
import { run, state } from "../app";
import type { AiTimeMode, AiTimeSettings, AutoPlaySettings, GameCommand, GameSettings, HumanSlContinuation, KataTimeType } from "../contract/generated";
import { icon } from "../design/icons";
import { formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { button, footer, groupTitle, primaryButton, row, segmented, switchRow } from "../engine/controls";
import { plainJava } from "../engine/tuningText";
import { product, registerProduct, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import { endHumanSlFirst, registerHumanSlContinuation } from "../humansl/state";
import "./game.css";

registerProduct({
  ZhCn: { gameGroupOptions: "选项" },
  ZhTw: { gameGroupOptions: "選項" },
  EnUs: { gameGroupOptions: "Options" },
  JaJp: { gameGroupOptions: "オプション" },
  KoKr: { gameGroupOptions: "옵션" },
  ThTh: { gameGroupOptions: "ตัวเลือก" },
});

const HANDICAPS = [0, 2, 3, 4, 5, 6, 7, 8, 9];
const KATA_TYPES: KataTimeType[] = ["Byoyomi", "Fischer", "Absolute"];

/** A JCheckBox as a switch row whose switch carries the automation key; `extra` are the controls
 *  it turns on, standing before the switch. */
function toggle(host: HTMLElement, title: string, sub: string, checked: boolean, key: string, ...extra: (Node | string)[]): HTMLInputElement {
  const box = switchRow(host, title, sub, checked, ...extra).input;
  box.dataset.key = key;
  return box;
}

/** A hint Java writes as "(…)" beside its check box, without the parentheses: a row's description. */
const bare = (text: string) => text.replace(/^\s*[(（]([\s\S]*)[)）]\s*$/, "$1");

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
    el.inputMode = filter === "int" ? "numeric" : "decimal";
    const allowed = filter === "int" ? /\D/g : /[^\d.-]/g;
    el.addEventListener("input", () => {
      const kept = el.value.replace(allowed, "");
      if (kept !== el.value) el.value = kept;
    });
  }
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

/** One choice of an AI time group (chkTimeGroup): its radio before its name, its fields at the
 *  end; a click on the name chooses it. A row with one field is a label already, whose first
 *  control is the radio; any other row gets a label around the name. */
function choice(host: HTMLElement, group: string, checked: boolean, key: string, text: string, ...controls: HTMLElement[]): HTMLInputElement {
  const box = document.createElement("input");
  box.type = "radio";
  box.name = group;
  box.checked = checked;
  box.dataset.key = key;
  const name = document.createElement("span");
  name.className = "game-choice";
  name.append(box, text);
  if (row(host, name, "", ...controls).tagName !== "LABEL") {
    const label = document.createElement("label");
    label.className = name.className;
    label.append(...name.childNodes);
    name.replaceWith(label);
  }
  return box;
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

/** 限制我的用时 with 保留时间 / 读秒 / 次数; both "无" is refused as Byoyomi.emptyTimeHint. Returns
 *  the values to save. */
function myTime(host: HTMLElement, g: GameSettings) {
  const save = select([t("Byoyomi.none"), "5", "10", "20", "30", "60", "120", "180"], g.mySaveTime, "game.mySaveTime");
  const byo = select([t("Byoyomi.none"), "10", "15", "20", "30", "40", "60", "120"], g.myByoyomiSeconds, "game.myByoyomiSeconds");
  const times = select(["1", "2", "3", "5", "10", "20"], g.myByoyomiTimes, "game.myByoyomiTimes");
  const limit = toggle(host, t("Byoyomi.newGame.limitMyTime"), "", g.limitMyTime, "game.limitMyTime", t("Byoyomi.newGame.saveTime"), save, t("Byoyomi.newGame.byoyomi"), byo, t("Byoyomi.newGame.byoyomiTimes"), times);
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
  limit.onchange = () => (save.disabled = byo.disabled = times.disabled = !limit.checked);
  save.disabled = byo.disabled = times.disabled = !limit.checked;
  if (limit.checked && byo.selectedIndex === 0) times.disabled = true;
  return () => ({ limitMyTime: limit.checked, mySaveTime: save.selectedIndex, myByoyomiSeconds: byo.selectedIndex, myByoyomiTimes: times.selectedIndex });
}

/** KataGo专用时间: `type` (读秒制 / 加秒制 / 包干制) stands in the KataGo choice's row; `place` adds
 *  the rows under it, shown while KataGo is chosen: 保留时间, then 读秒 and 次数 for 读秒制 or
 *  每手增加 for 加秒制 (包干制 only 保留时间). */
function kataTime(a: AiTimeSettings) {
  const type = select([t("NewGameDialog.kataTime.byoyomi"), t("NewGameDialog.kataTime.fisher"), t("NewGameDialog.kataTime.absolute")], KATA_TYPES.indexOf(a.kataType), "game.kataType");
  const mins = field(String(a.kataMainMinutes), "game.kataMainMinutes", "int");
  const secs = field(String(a.kataByoyomiSeconds), "game.kataByoyomiSeconds", "int");
  const times = field(String(a.kataByoyomiTimes), "game.kataByoyomiTimes", "int");
  const inc = field(String(a.kataFischerIncrement), "game.kataFischerIncrement", "int");
  const rows: HTMLElement[] = [];
  let chosen = false;
  const show = () => {
    if (!rows.length) return;
    const [minsRow, secsRow, timesRow, incRow] = rows;
    const k = type.selectedIndex;
    minsRow.hidden = !chosen;
    secsRow.hidden = timesRow.hidden = !chosen || k !== 0;
    incRow.hidden = !chosen || k !== 1;
  };
  type.onchange = show;
  const place = (host: HTMLElement) => {
    rows.push(
      row(host, t("Byoyomi.newGame.saveTime"), "", mins),
      row(host, t("Byoyomi.newGame.byoyomi"), "", secs),
      row(host, t("Byoyomi.newGame.byoyomiTimes"), "", times),
      row(host, t("NewGameDialog.kataTime.increment"), "", inc),
    );
    for (const r of rows) r.classList.add("game-sub");
    show();
  };
  const enable = (on: boolean) => {
    chosen = on;
    for (const el of [type, mins, secs, times, inc]) el.disabled = !on;
    show();
  };
  const read = (base: AiTimeSettings): AiTimeSettings => ({
    ...base,
    kataType: KATA_TYPES[type.selectedIndex] ?? base.kataType,
    kataMainMinutes: textInt(mins, base.kataMainMinutes),
    kataByoyomiSeconds: textInt(secs, base.kataByoyomiSeconds),
    kataByoyomiTimes: textInt(times, base.kataByoyomiTimes),
    kataFischerIncrement: textInt(inc, base.kataFischerIncrement),
  });
  return { type, place, enable, read };
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

/** 当前局面续弈, 让子 and 自由让子; 当前局面续弈 disables 让子. */
function handicapRows(host: HTMLElement, g: GameSettings) {
  const cont = toggle(host, t("NewGameDialog.ContinuePlay"), "", g.continuePlay, "game.continuePlay");
  const handicap = select(HANDICAPS.map(String), HANDICAPS.indexOf(g.handicap), "game.handicap");
  row(host, t("NewGameDialog.Handicap"), "", handicap);
  const free = toggle(host, t("NewGameDialog.useFreeHandicap"), "", g.freeHandicap, "game.freeHandicap");
  cont.addEventListener("change", () => (handicap.disabled = cont.checked));
  handicap.disabled = cont.checked;
  return { cont, handicap, free };
}

/** 显示分析选点(黑) and (白), each name after its stone. */
function shownSides(host: HTMLElement, g: GameSettings) {
  const side = (text: string, checked: boolean, key: string, stone: "stone-black" | "stone-white") => {
    const r = switchRow(host, text, "", checked);
    r.input.dataset.key = key;
    r.row.classList.add("game-side");
    r.row.querySelector(".ds-row-title")?.prepend(icon(stone));
    return r.input;
  };
  return {
    black: side(t("NewAnaGameDialog.showBlack"), g.showBlack, "game.showBlack", "stone-black"),
    white: side(t("NewAnaGameDialog.showWhite"), g.showWhite, "game.showWhite", "stone-white"),
  };
}

/** 确定 (initButtonBar's OK) and 取消; Escape also closes without a game. */
function okCancel(body: HTMLElement, label: string, apply: () => Promise<void>, close: () => void): void {
  const ok = primaryButton(label, () => void apply());
  ok.dataset.key = "game-dialog-ok";
  const cancel = button(product("cancel"), close);
  cancel.dataset.key = "game.cancel";
  footer(body, { primary: ok, cancel });
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
  const body = document.createElement("div");
  body.className = "game-dialog";
  const engine = engineSelect();
  row(body, t("NewAnaGameDialog.chooseEngine"), "", engine);
  const side = select([t("NewGameDialog.playBlack"), t("NewGameDialog.playWhite")], g.playBlack ? 0 : 1, "game.side");
  row(body, t("NewGameDialog.chooseBlackWhite"), "", side);
  const { cont, handicap, free } = handicapRows(body, g);
  const komi = field(javaDouble(g.komi), "game.komi", "komi");
  row(body, t("NewGameDialog.Komi"), "", komi);
  const myTimes = myTime(body, g);
  // The four AI time choices (chkTimeGroup), then the KataGo visit limits.
  groupTitle(body, t("SetAiTimes.title"));
  const time = field(String(a.fixedSeconds), "game.fixedSeconds", "int");
  const normal = choice(body, "game-new-time", a.mode === "Fixed", "game.timeFixed", t("NewGameDialog.time"), time);
  const advText = field(a.advanced, "game.advancedTime");
  const adv = choice(body, "game-new-time", a.mode === "Advanced", "game.timeAdvanced", t("NewGameDialog.lblAdvanceTime"), advText, advancedHelp());
  const kataPanel = kataTime(a);
  const kata = choice(body, "game-new-time", a.mode === "KataGo", "game.timeKataGo", t("NewGameDialog.kataTime"), kataPanel.type);
  kataPanel.place(body);
  const owned = choice(body, "game-new-time", a.mode === "EngineOwned", "game.timeEngineOwned", t("NewGameDialog.engineOwnedTime"));
  const visits = field(g.kataVisits > 0 ? String(g.kataVisits) : "", "game.kataVisits", "int");
  const playouts = field(g.kataPlayouts > 0 ? String(g.kataPlayouts) : "", "game.kataPlayouts", "int");
  const limits = toggle(body, t("NewGameDialog.chkUseKataAdvSettings"), bare(t("NewGameDialog.lblVersionRequire")), g.kataLimits, "game.kataLimits", t("NewGameDialog.kataVisits"), visits, t("NewGameDialog.kataPlayouts"), playouts);
  const limitsChanged = () => (visits.disabled = playouts.disabled = !limits.checked);
  limits.onchange = limitsChanged;
  limitsChanged();
  const timeChanged = () => {
    kataPanel.enable(kata.checked);
    advText.disabled = !adv.checked;
    time.disabled = kata.checked || adv.checked || owned.checked;
  };
  for (const box of [normal, kata, adv, owned]) box.onchange = timeChanged;
  timeChanged();
  groupTitle(body, product("gameGroupOptions"));
  const ponder = toggle(body, t("NewGameDialog.chkPonder"), bare(t("NewGameDialog.chkPonderDescribe")), g.playPonder, "game.playPonder");
  const playMode = toggle(body, t("NewGameDialog.chkUsePlayMode"), bare(t("NewGameDialog.chkUsePlayModeDescribe")), g.usePlayMode, "game.usePlayMode");
  const { black, white } = shownSides(body, g);
  const autoSave = toggle(body, t("NewAnaGameDialog.lblAutoSave"), autoSaveText(), g.autoSave, "game.autoSave");
  const dialog = formPanel(t("NewGameDialog.title"), body);
  okCancel(body, t("NewGameDialog.okButton"), async () => {
    if (engine.selectedIndex < 0) {
      await message(t("Message.title"), t("NewAnaGameDialog.noEngineHint"));
      return;
    }
    const mode: AiTimeMode = owned.checked ? "EngineOwned" : adv.checked ? "Advanced" : kata.checked ? "KataGo" : "Fixed";
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
      continuePlay: cont.checked,
      handicap: HANDICAPS[handicap.selectedIndex] ?? 0,
      freeHandicap: free.checked,
      komi: /^[+-]?(\d+\.?\d*|\.\d+)$/.test(komiText) ? Number(komiText) : 7.5,
      ...myTimes(),
      aiTime: { ...kataPanel.read(now.aiTime), mode, fixedSeconds, advanced: advText.value },
      kataLimits: limits.checked,
      kataVisits: textInt(visits, -1),
      kataPlayouts: textInt(playouts, -1),
      playPonder: ponder.checked,
      usePlayMode: playMode.checked,
      showBlack: black.checked,
      showWhite: white.checked,
      autoSave: autoSave.checked,
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
  const body = document.createElement("div");
  body.className = "game-dialog";
  const engine = engineSelect();
  row(body, t("NewAnaGameDialog.chooseEngine"), "", engine);
  const side = select([t("NewGameDialog.playBlack"), t("NewGameDialog.playWhite")], g.playBlack ? 0 : 1, "game.side");
  row(body, t("NewGameDialog.chooseBlackWhite"), "", side);
  const { cont, handicap, free } = handicapRows(body, g);
  const komi = field(javaDouble(g.komi), "game.komi", "komi");
  row(body, t("NewGameDialog.Komi"), "", komi);
  const myTimes = myTime(body, g);
  // 落子条件: each switch and its field (the field holds the toolbar text even while off).
  groupTitle(body, t("SetAiTimes.title"));
  const pureNet = toggle(body, t("NewAnaGameDialog.lblAiUsePureNet"), "", g.pureNet, "game.pureNet");
  const time = field(String(g.aiTime.fixedSeconds), "game.moveTime", "int");
  const useTime = toggle(body, t("NewAnaGameDialog.moveTime"), "", auto.timeEnabled, "game.moveTimeOn", time);
  const playouts = field(conditionText(auto.playouts), "game.movePlayouts", "int");
  const usePlayouts = toggle(body, t("NewAnaGameDialog.moveVisits"), "", auto.playoutsEnabled, "game.movePlayoutsOn", playouts);
  const first = field(conditionText(auto.firstPlayouts), "game.moveFirstPlayouts", "int");
  const useFirst = toggle(body, t("NewAnaGameDialog.firstVisits"), "", auto.firstPlayoutsEnabled, "game.moveFirstPlayoutsOn", first);
  useTime.onchange = () => (time.disabled = !useTime.checked);
  usePlayouts.onchange = () => (playouts.disabled = !usePlayouts.checked);
  useFirst.onchange = () => (first.disabled = !useFirst.checked);
  // 纯网络 disables the three fields; turning it off enables each by its switch.
  const pureChanged = () => {
    time.disabled = pureNet.checked || !useTime.checked;
    playouts.disabled = pureNet.checked || !usePlayouts.checked;
    first.disabled = pureNet.checked || !useFirst.checked;
  };
  pureNet.onchange = pureChanged;
  pureChanged();
  groupTitle(body, product("gameGroupOptions"));
  const ponder = toggle(body, t("NewAnaGameDialog.ponder"), "", g.playPonder, "game.playPonder");
  const random = button(t("NewAnaGameDialog.btnRandomStart"), () => void openRandomStart());
  random.dataset.key = "game.randomStart";
  row(body, t("NewAnaGameDialog.random"), "", random);
  const startMove = field(String(g.resignStartMove), "game.resignStartMove");
  const resignMove = field(String(g.resignMove), "game.resignMove");
  const resignPercent = field(String(g.resignPercent), "game.resignPercent");
  startMove.inputMode = resignMove.inputMode = "numeric";
  resignPercent.inputMode = "decimal";
  row(body, t("NewAnaGameDialog.resign"), "", t("NewAnaGameDialog.resign0"), startMove, t("NewAnaGameDialog.resign1"), resignMove, t("NewAnaGameDialog.resign2"), resignPercent, "%");
  const wrn = toggle(body, t("NewAnaGameDialog.lblDisableWRNInGame"), "", g.disableWrn, "game.disableWrn");
  const playMode = toggle(body, t("NewAnaGameDialog.chkUsePlayMode"), "", g.usePlayMode, "game.usePlayMode");
  const { black, white } = shownSides(body, g);
  const autoSave = toggle(body, t("NewAnaGameDialog.lblAutoSave"), autoSaveText(), g.autoSave, "game.autoSave");
  const dialog = formPanel(t("NewAnaGameDialog.title"), body);
  okCancel(body, t("NewAnaGameDialog.okButton"), async () => {
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
      timeEnabled: seconds > 0 && useTime.checked,
      timeSeconds: seconds > 0 ? seconds : -1,
      playoutsEnabled: visits > 0 && usePlayouts.checked,
      playouts: visits > 0 ? visits : -1,
      firstPlayoutsEnabled: firstVisits > 0 && useFirst.checked,
      firstPlayouts: firstVisits > 0 ? firstVisits : -1,
    };
    const settings: GameSettings = {
      ...now,
      playBlack,
      continuePlay: cont.checked,
      handicap: HANDICAPS[handicap.selectedIndex] ?? 0,
      freeHandicap: free.checked,
      komi: leadingDouble(komi.value) ?? 7.5,
      ...myTimes(),
      aiTime: { ...now.aiTime, fixedSeconds: seconds > 0 ? seconds : now.aiTime.fixedSeconds },
      playPonder: ponder.checked,
      pureNet: pureNet.checked,
      resignStartMove: textInt(startMove, now.resignStartMove),
      resignMove: textInt(resignMove, now.resignMove),
      resignPercent: textDouble(resignPercent, now.resignPercent),
      disableWrn: wrn.checked,
      usePlayMode: playMode.checked,
      showBlack: black.checked,
      showWhite: white.checked,
      autoSave: autoSave.checked,
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
  const body = document.createElement("div");
  body.className = "game-dialog game-narrow";
  const ponder = segmented("game-ai-ponder", [["yes", t("SetAiTimes.rdoPonder")], ["no", t("SetAiTimes.rdoNoPonder")]], v.settings.game.playPonder ? "yes" : "no");
  const [yes, no] = ponder.inputs;
  yes.dataset.key = "game.ponderYes";
  no.dataset.key = "game.ponderNo";
  row(body, t("SetAiTimes.lblPonder"), "", ponder.el);
  groupTitle(body, t("SetAiTimes.lblGenmoveMode"));
  const setTime = field(String(a.fixedSeconds), "game.fixedSeconds", "int");
  const normal = choice(body, "game-set-time", a.mode === "Fixed", "game.timeFixed", t("SetAiTimes.lblGenmoveTime"), setTime);
  const advText = field(a.advanced, "game.advancedTime");
  // `txtAdvanceTime.setEditable(false)` unless 高级时间 was saved: choosing it here enables the
  // field but it stays read-only (SetAiTimes.java:393-394).
  if (a.mode !== "Advanced") advText.readOnly = true;
  const adv = choice(body, "game-set-time", a.mode === "Advanced", "game.timeAdvanced", t("SetAiTimes.lblAdvTime"), advText, advancedHelp());
  const kataPanel = kataTime(a);
  const kata = choice(body, "game-set-time", a.mode === "KataGo", "game.timeKataGo", t("SetAiTimes.lblKataTime"), kataPanel.type);
  kataPanel.place(body);
  const owned = choice(body, "game-set-time", a.mode === "EngineOwned", "game.timeEngineOwned", t("NewGameDialog.engineOwnedTime"));
  const timeChanged = () => {
    kataPanel.enable(kata.checked);
    advText.disabled = !adv.checked;
    setTime.disabled = kata.checked || adv.checked || owned.checked;
  };
  for (const box of [normal, kata, adv, owned]) box.onchange = timeChanged;
  timeChanged();
  groupTitle(body, t("SetAiTimes.analyzeMode"));
  const anaTime = field(auto.timeEnabled ? conditionText(auto.timeSeconds) : "", "game.anaTime");
  const anaPlayouts = field(auto.playoutsEnabled ? conditionText(auto.playouts) : "", "game.anaPlayouts");
  const anaFirst = field(auto.firstPlayoutsEnabled ? conditionText(auto.firstPlayouts) : "", "game.anaFirstPlayouts");
  anaTime.inputMode = anaPlayouts.inputMode = anaFirst.inputMode = "numeric";
  row(body, t("SetAiTimes.lblTime"), "", anaTime);
  row(body, t("SetAiTimes.lblPo"), "", anaPlayouts);
  row(body, t("SetAiTimes.lblFirstPo"), "", anaFirst);
  const dialog = formPanel(t("SetAiTimes.title"), body);
  const ok = primaryButton(t("SetAiTimes.okButton"), () => void apply());
  ok.dataset.key = "game-ai-time-ok";
  const cancel = button(t("SetAiTimes.cancel"), () => void dialog.close());
  cancel.dataset.key = "game.aiTimeCancel";
  footer(body, { primary: ok, cancel });
  // getRootPane().setDefaultButton(okButton).
  body.addEventListener("keydown", (e) => {
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
    const mode: AiTimeMode = owned.checked ? "EngineOwned" : adv.checked ? "Advanced" : kata.checked ? "KataGo" : "Fixed";
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
    const result = await game({ type: "SetAiTime", time, ponder: yes.checked, autoplay });
    if (result && result.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

/** SetAnaGameRandomStart: 启用, 前 N 手, the win rate and playouts tolerances. */
export async function openRandomStart(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const r = v.settings.game.randomStart;
  const body = document.createElement("div");
  body.className = "game-dialog game-narrow";
  // 启用 reads with 前 N 手: "enable for the first N moves".
  const moves = field(String(r.moves), "game.randomMoves");
  moves.inputMode = "numeric";
  const enable = toggle(body, t("SetAnaGameRandomStart.chkEnable"), "", r.enabled, "game.randomEnabled", t("SetAnaGameRandomStart.lblFirst.text"), moves, t("SetAnaGameRandomStart.lblMove.text"));
  const winrate = field(String(r.winrateDiff), "game.randomWinrate");
  const playouts = field(String(r.playoutsDiff), "game.randomPlayouts");
  winrate.inputMode = playouts.inputMode = "decimal";
  row(body, t("SetAnaGameRandomStart.lblWinrate.text"), "", winrate, "%");
  row(body, t("SetAnaGameRandomStart.lblVisits.text"), "", playouts, "%");
  const enabled = () => (moves.disabled = winrate.disabled = playouts.disabled = !enable.checked);
  enable.onchange = enabled;
  enabled();
  const dialog = formPanel(t("SetAnaGameRandomStart.title"), body);
  const ok = primaryButton(t("SetAnaGameRandomStart.apply"), () => void apply());
  ok.dataset.key = "game-random-ok";
  const cancel = button(t("SetAnaGameRandomStart.cancel"), () => void dialog.close());
  cancel.dataset.key = "game.randomCancel";
  footer(body, { primary: ok, cancel });
  async function apply(): Promise<void> {
    // applyChange: Integer.parseInt / Double.parseDouble, else SetAnaGameRandomStart.wrongFormat.
    const n = parseJavaInt(moves.value);
    const w = textDouble(winrate, Number.NaN), p = textDouble(playouts, Number.NaN);
    if (n === null || !Number.isFinite(w) || !Number.isFinite(p)) {
      await message(t("Message.title"), t("SetAnaGameRandomStart.wrongFormat"));
      return;
    }
    const result = await game({ type: "SetRandomStart", settings: { enabled: enable.checked, moves: n, winrateDiff: w, playoutsDiff: p } });
    if (result && result.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

// The dialogs an AI 陪练 exit opens afterwards (deferUntilHumanSlExit(this::startNewGame) /
// (this::startAnalyzeGameDialog)).
registerHumanSlContinuation("NewGame", () => void openNewGame());
registerHumanSlContinuation("NewAnalysisGame", () => void openNewAnaGame());
