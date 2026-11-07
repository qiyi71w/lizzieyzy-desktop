// 引擎对战 windows: 引擎对局 (NewEngineGameDialog.java), 引擎对局设置 (EnginePkConfig.java, from the
// toolbar or from 更多设置), 人工干预 (Manual.java), 修改多盘引擎对局数 (Menu.java:3121-3147) and the
// non-modal end messages (Utils.showMsgNoModal). Each holds a draft; committed values come from
// the settings view and every change is one Match command. Rust owns the match.
import { run, state, subscribe } from "../app";
import { isApiError, openMatchOpenings } from "../contract/client";
import type { MatchCommand, MatchIntervention, MatchSessionSettings, MatchSettings, MatchTimeMode, Stone } from "../contract/generated";
import { ask, formPanel, message } from "../dialogs";
import { absolute, parseJavaInt, place } from "../display/dialogs";
import { button } from "../engine/controls";
import { composeMatchRules } from "../engine/rules";
import { plainJava } from "../engine/tuningText";
import { product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import { engineIdentity as identity, matchActive, matchValues, setEngineMatchChecked } from "./state";
import { endHumanSlFirst, registerHumanSlContinuation } from "../humansl/state";
import "./match.css";

function match(op: MatchCommand) {
  return run({ type: "Match", op });
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function check(checked: boolean, key: string, text = ""): { box: HTMLInputElement; wrap: HTMLLabelElement } {
  const box = el("input");
  box.type = "checkbox";
  box.checked = checked;
  box.dataset.key = key;
  const wrap = el("label", "match-check");
  wrap.append(box);
  if (text) wrap.append(text);
  return { box, wrap };
}

function radio(name: string, checked: boolean, key: string, text: string): { box: HTMLInputElement; wrap: HTMLLabelElement } {
  const r = check(checked, key, text);
  r.box.type = "radio";
  r.box.name = name;
  return r;
}

/** IntDocument (digits), DoubleDocument (digits and a point) or KomiDocument (signed decimal). */
function field(value: string, key: string, filter: "int" | "double" | "komi" | null = null, width = 0): HTMLInputElement {
  const f = el("input");
  f.type = "text";
  f.value = value;
  f.dataset.key = key;
  if (width) f.style.width = `${width}px`;
  if (filter) {
    const allowed = filter === "int" ? /\D/g : filter === "double" ? /[^\d.]/g : /[^\d.-]/g;
    f.addEventListener("input", () => {
      const kept = f.value.replace(allowed, "");
      if (kept !== f.value) f.value = kept;
    });
  }
  return f;
}

function select(options: string[], index: number, key: string): HTMLSelectElement {
  const s = el("select");
  s.dataset.key = key;
  for (const text of options) s.append(el("option", "", text));
  s.selectedIndex = options.length ? Math.min(Math.max(0, index), options.length - 1) : -1;
  return s;
}

function inline(...items: HTMLElement[]): HTMLDivElement {
  const d = el("div", "match-inline");
  d.append(...items);
  return d;
}

/** createSectionPanel: a 1 px rgb(220,224,230) line around 12 px padding. */
function section(...rows: HTMLElement[]): HTMLDivElement {
  const d = el("div", "match-section");
  d.append(...rows);
  return d;
}

/** Integer.parseInt of the text, else the current value (catch NumberFormatException {}). */
const intOr = (f: HTMLInputElement, keep: number) => parseJavaInt(f.value) ?? keep;
/** Double.parseDouble of the text, else the current value. */
function doubleOr(f: HTMLInputElement, keep: number): number {
  const s = f.value.trim();
  return /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(s) ? Number(s) : keep;
}
/** A toolbar number field: "" for 0 (BottomToolbar restores only values above 0). */
const shown = (n: number) => (n > 0 ? String(Math.trunc(n)) : "");
/** A toolbar number field read back: blank or unreadable is 0. */
const read = (f: HTMLInputElement) => parseJavaInt(f.value.trim()) ?? 0;

/** DecimalFormat("#0.0").parse: the leading number, null when the text has none. */
function leadingDouble(text: string): number | null {
  const m = /^\s*([+-]?(\d+\.?\d*|\.\d+))/.exec(text);
  return m ? Number(m[1]) : null;
}

/** DecimalFormat("0").parse: the leading integer, null when the text has none. */
function leadingInt(text: string): number | null {
  const m = /^\s*([+-]?\d+)/.exec(text);
  return m ? Number.parseInt(m[1], 10) : null;
}

/** The engine boxes' items: "[n]name" of each configured engine (EngineManager.java:4432). */
function engineItems(): string[] {
  return (state.view?.engine.engines ?? []).map((e, k) => `[${k + 1}]${e.name}`);
}

/** The toolbar values the dialog edits in place (Java hosts the toolbar's own components). */
export interface SharedFields {
  black: HTMLSelectElement;
  white: HTMLSelectElement;
  timeOn: HTMLInputElement;
  timeBlack: HTMLInputElement;
  timeWhite: HTMLInputElement;
  visitsOn: HTMLInputElement;
  visitsBlack: HTMLInputElement;
  visitsWhite: HTMLInputElement;
  firstOn: HTMLInputElement;
  firstBlack: HTMLInputElement;
  firstWhite: HTMLInputElement;
  batchOn: HTMLInputElement;
  batch: HTMLInputElement;
  continuePlay: HTMLInputElement;
}

/** The session values of the toolbar fields over the saved ones. */
export function sharedSession(f: SharedFields, base: MatchSessionSettings): MatchSessionSettings {
  return {
    ...base,
    timeEnabled: f.timeOn.checked,
    firstSeconds: read(f.timeBlack),
    secondSeconds: read(f.timeWhite),
    visitsEnabled: f.visitsOn.checked,
    firstVisits: read(f.visitsBlack),
    secondVisits: read(f.visitsWhite),
    firstMoveVisitsEnabled: f.firstOn.checked,
    firstMoveVisits: read(f.firstBlack),
    secondMoveVisits: read(f.firstWhite),
    batchEnabled: f.batchOn.checked,
    batchLimit: read(f.batch),
    continuePosition: f.continuePlay.checked,
  };
}

/** btnAboutAdvTime: Utils.showHtmlMessageModal(AdvanceTimeSettings.title, .describe). */
function advancedHelp(): HTMLButtonElement {
  const help = button("", () => void message(t("AdvanceTimeSettings.title"), plainJava(t("AdvanceTimeSettings.describe"))));
  help.className = "match-help";
  help.title = t("AdvanceTimeSettings.title");
  help.dataset.key = "match.advancedTimeHelp";
  const img = el("img");
  img.src = "/assets/settings.png";
  img.alt = "";
  help.append(img);
  return help;
}

/** MatchRuleOption.options() with their labels, in Java's order. */
export function matchRuleOptions(): [string, string][] {
  return [
    ["chinese", t("LizzieFrame.currentRules.chinese")],
    ["stone-scoring", t("LizzieFrame.currentRules.chn-ancient")],
    ["japanese", t("MatchRules.option.japaneseKorean")],
    ["aga", t("MatchRules.option.agaBga")],
    ["new-zealand", t("MatchRules.option.newZealand")],
    ["tromp-taylor", t("MatchRules.option.trompTaylor")],
    ["goe", t("LizzieFrame.currentRules.goe")],
  ];
}

/** MatchRulesPicker: the standard choices, then 自定义. A standard choice is sent as its preset
 *  name, 自定义 as the composed rules (SetKataRules compose mode); cancelling the editor returns to
 *  the previous choice. */
function rulesPicker(prefill: string, choice: string | null) {
  const options = matchRuleOptions();
  const s = select([...options.map(([, label]) => label), t("NewEngineGameDialog.matchRules.custom")], 0, "match.rules");
  let selected = choice ?? prefill;
  let custom = prefill;
  const index = () => {
    const k = options.findIndex(([name]) => name === selected);
    return k >= 0 ? k : options.length;
  };
  s.selectedIndex = index();
  s.onchange = async () => {
    const k = s.selectedIndex;
    if (k < options.length) {
      selected = options[k][0];
      return;
    }
    const edited = await composeMatchRules(custom);
    if (edited !== null) {
      custom = edited;
      selected = edited;
    }
    s.selectedIndex = index();
  };
  return { el: s, value: () => selected };
}

const TIME_MODES: MatchTimeMode[] = ["EngineOwned", "Fixed", "RawAdvanced"];

/** The open 引擎对局 dialog, so a second Alt+E does not stack another. */
let dialogOpen = false;

/** LizzieFrame.startEngineGameDialog (13412-13457) and NewEngineGameDialog. */
export async function openEngineGameDialog(): Promise<void> {
  if (!state.view || dialogOpen) return;
  // deferUntilHumanSlExit (LizzieFrame.java:13412-13415): during an AI 陪练 session the dialog is
  // the exit's continuation; the core publishes it once the session is gone.
  if (!(await endHumanSlFirst("EngineGame"))) return;
  const v = state.view;
  if (!v || dialogOpen) return;
  // startEngineGameDialogReserved: refused while a game of the batch is playing.
  if (v.engine_match?.phase === "Playing" || v.engine_match?.phase === "Paused") {
    await message(t("Message.title"), t("LizzieFrame.engineGameStopFirstHint"));
    return;
  }
  // showEngineGameDialogAfterModeTransition: a 人机对弈 game ends first.
  if (v.game) await run({ type: "Game", op: { type: "End" } });
  const m = (state.view ?? v).settings.matches;
  dialogOpen = true;
  setEngineMatchChecked(true);
  const items = engineItems();
  const black = select(items, m.first_engine, "match.engineBlack");
  const white = select(items, m.second_engine, "match.engineWhite");
  // A dialog with both on one engine moves 白 to the next entry (or the previous at the end).
  if (black.selectedIndex === white.selectedIndex && items.length > 1) {
    const b = black.selectedIndex;
    white.selectedIndex = b <= items.length - 2 ? b + 1 : b - 1 >= 0 ? b - 1 : b;
    void run(matchValues({ settings: { second: identity(white.selectedIndex) } }));
  }
  const s = m.session;
  const f: SharedFields = {
    black,
    white,
    timeOn: check(s.timeEnabled, "match.timeOn").box,
    timeBlack: field(shown(s.firstSeconds), "match.timeBlack", "int"),
    timeWhite: field(shown(s.secondSeconds), "match.timeWhite", "int"),
    visitsOn: check(s.visitsEnabled, "match.visitsOn").box,
    visitsBlack: field(shown(s.firstVisits), "match.visitsBlack", "int"),
    visitsWhite: field(shown(s.secondVisits), "match.visitsWhite", "int"),
    firstOn: check(s.firstMoveVisitsEnabled, "match.firstVisitsOn").box,
    firstBlack: field(shown(s.firstMoveVisits), "match.firstVisitsBlack", "int"),
    firstWhite: field(shown(s.secondMoveVisits), "match.firstVisitsWhite", "int"),
    batchOn: check(s.batchEnabled, "match.batchOn").box,
    batch: field(shown(s.batchLimit), "match.batch", "int", 72),
    continuePlay: check(s.continuePosition, "match.continue", t("NewEngineGameDialog.lblContinue")).box,
  };
  const cfg = m.settings;
  const modeB = select([t("NewGameDialog.engineOwnedTime"), t("NewGameDialog.time"), t("NewEngineGameDialog.lblAdvanceTime")], TIME_MODES.indexOf(cfg.firstTimeMode), "match.timeModeBlack");
  const modeW = select([t("NewGameDialog.engineOwnedTime"), t("NewGameDialog.time"), t("NewEngineGameDialog.lblAdvanceTime")], TIME_MODES.indexOf(cfg.secondTimeMode), "match.timeModeWhite");
  const advB = field(cfg.firstTimeCommand, "match.advancedBlack");
  const advW = field(cfg.secondTimeCommand, "match.advancedWhite");

  // 黑方设置 / 白方设置 with the toolbar's own boxes beside the row labels.
  const engines = el("div", "match-engines");
  const cell = (item: HTMLElement | null) => item ?? el("span");
  const row = (label: HTMLElement, enable: HTMLElement | null, b: HTMLElement, w: HTMLElement) => engines.append(label, cell(enable), b, w);
  const header = (key: string) => el("span", "match-side", key);
  engines.append(el("span"), el("span"), header(t("NewEngineGameDialog.lblB")), header(t("NewEngineGameDialog.lblW")));
  row(el("span", "", t("NewEngineGameDialog.lblengine")), null, black, white);
  row(el("span", "", t("NewEngineGameDialog.lblTime")), f.timeOn, f.timeBlack, f.timeWhite);
  row(el("span"), null, modeB, modeW);
  row(inline(el("span", "", t("NewEngineGameDialog.lblAdvanceTime")), advancedHelp()), null, advB, advW);
  row(el("span", "", t("NewEngineGameDialog.lblPlayout")), f.visitsOn, f.visitsBlack, f.visitsWhite);
  row(el("span", "", t("NewEngineGameDialog.lblFirstPlayout")), f.firstOn, f.firstBlack, f.firstWhite);

  // 认输预置: hidden in Genmove mode, where the hint takes its place.
  const resignHint = el("div", "match-hint", t("EnginePkConfig.lblresignGenmove"));
  const resign = resignRows(cfg);
  const resignSection = section(resignHint, resign.rows);

  // 贴目 / 让子, 本局规则, 多盘, 当前局面续弈 / 加载开局SGF, 顺序随机 / 多选SGF.
  const komi = field(javaDouble(cfg.komi), "match.komi", "komi", 72);
  const handicap = field(String(cfg.handicap), "match.handicap", "int", 72);
  const handicapLabel = el("span", "", t("NewEngineGameDialog.handicap"));
  const rules = rulesPicker(m.rules_prefill, m.rules_choice);
  const sgf = check(cfg.sgfStart, "match.sgfStart", t("NewEngineGameDialog.lblsgf").trim());
  const random = select([t("NewEngineGameDialog.cbxRandomSgf1"), t("NewEngineGameDialog.cbxRandomSgf2")], cfg.sgfRandom ? 1 : 0, "match.sgfRandom");
  const chooseSgf = button(t("NewEngineGameDialog.btnSGFstart"), () => void openMatchOpenings([]).then(
    (r) => { if (r.type === "Refused") void message(t("Message.title"), t(r.message_key), "warning"); },
    (e: unknown) => message("Error", isApiError(e) ? e.message : String(e), "error"),
  ));
  chooseSgf.dataset.key = "match.chooseSgf";
  const game = el("div", "match-setup");
  const span = (item: HTMLElement, columns: number) => {
    item.style.gridColumn = `span ${columns}`;
    return item;
  };
  game.append(
    el("span", "", t("NewEngineGameDialog.komi")), komi, handicapLabel, handicap,
    el("span", "", t("NewEngineGameDialog.matchRules")), span(rules.el, 3),
    el("span", "", t("NewEngineGameDialog.lblBatchGame").trim()), f.batchOn, span(f.batch, 2),
    span(f.continuePlay.parentElement!, 2), span(sgf.wrap, 2),
    random, span(chooseSgf, 3),
  );
  const ponder = check(cfg.ponder, "match.ponder", t("NewEngineGameDialog.checkBoxAllowPonder"));
  const wrn = check(m.disable_wrn, "match.disableWrn", t("NewAnaGameDialog.lblDisableWRNInGame"));
  const body = el("div", "match-dialog");
  const main = el("div", "match-scroll");
  main.append(section(engines), resignSection, section(game), section(ponder.wrap, wrn.wrap));
  const more = button(t("NewEngineGameDialog.btnConfig"), () => void openEnginePkConfig(false).then(modeChanged));
  more.dataset.key = "match.moreSettings";
  const ok = button(t("NewEngineGameDialog.okButton"), () => void apply());
  ok.dataset.key = "match-dialog-ok";
  const bar = el("div", "match-buttons");
  bar.append(more, el("span", "match-fill"), ok);
  body.append(main, bar);

  // Enable rules (initContentPanel, updateModeDependentControls, setGenmove).
  const genmove = () => state.view?.settings.matches.session.kind === "Genmove";
  const enableRows = () => {
    f.timeBlack.disabled = f.timeWhite.disabled = !f.timeOn.checked;
    f.visitsOn.disabled = f.firstOn.disabled = genmove();
    f.visitsBlack.disabled = f.visitsWhite.disabled = genmove() || !f.visitsOn.checked;
    f.firstBlack.disabled = f.firstWhite.disabled = genmove() || !f.firstOn.checked;
    f.batch.disabled = !f.batchOn.checked;
    modeB.disabled = modeW.disabled = !genmove();
    advB.disabled = !genmove() || modeB.selectedIndex !== 2;
    advW.disabled = !genmove() || modeW.selectedIndex !== 2;
  };
  const openingRows = () => {
    const fixed = f.continuePlay.checked;
    chooseSgf.disabled = random.disabled = fixed || !sgf.box.checked;
    handicap.disabled = fixed || sgf.box.checked;
    handicapLabel.classList.toggle("disabled", handicap.disabled);
  };
  function modeChanged(): void {
    wrn.wrap.hidden = genmove();
    resign.rows.hidden = genmove();
    resignHint.hidden = !genmove();
    enableRows();
  }
  // The toolbar's values change as they are edited (Java shows the toolbar's components).
  const commitShared = () => {
    const now = state.view?.settings.matches;
    if (now) void run(matchValues({ session: sharedSession(f, now.session) }));
  };
  for (const control of [f.timeOn, f.timeBlack, f.timeWhite, f.visitsOn, f.visitsBlack, f.visitsWhite, f.firstOn, f.firstBlack, f.firstWhite, f.batchOn, f.batch]) {
    control.addEventListener("change", () => {
      enableRows();
      commitShared();
    });
  }
  modeB.onchange = modeW.onchange = enableRows;
  black.onchange = () => void run(matchValues({ settings: { first: identity(black.selectedIndex) } }));
  white.onchange = () => void run(matchValues({ settings: { second: identity(white.selectedIndex) } }));
  // 当前局面续弈 and 加载开局SGF exclude each other; 加载开局SGF, 顺序/随机, 允许后台计算 and 禁用WRN
  // are saved as they change.
  f.continuePlay.onchange = () => {
    const clearSgf = f.continuePlay.checked && sgf.box.checked;
    if (clearSgf) sgf.box.checked = false;
    openingRows();
    const now = state.view?.settings.matches;
    if (now) void run(matchValues({ settings: clearSgf ? { sgfStart: false } : {}, session: sharedSession(f, now.session) }));
  };
  sgf.box.onchange = () => {
    if (sgf.box.checked) f.continuePlay.checked = false;
    openingRows();
    const now = state.view?.settings.matches;
    if (now) void run(matchValues({ settings: { sgfStart: sgf.box.checked }, session: sharedSession(f, now.session) }));
  };
  random.onchange = () => void run(matchValues({ settings: { sgfRandom: random.selectedIndex === 1 } }));
  ponder.box.onchange = () => void run(matchValues({ settings: { ponder: ponder.box.checked } }));
  wrn.box.onchange = () => void run(matchValues({ disable_wrn: wrn.box.checked }));
  if (f.continuePlay.checked) sgf.box.checked = false;
  openingRows();
  modeChanged();

  // pendingEngineGameStart: accepted, waiting for the first move; closing it stops the start.
  let pending = false;
  let started = false;
  /** The accepted start has shown its match, so its disappearing means the start failed. */
  let seen = false;
  const dialog = formPanel(t("NewEngineGameDialog.title"), body, async () => {
    if (pending) {
      pending = false;
      await match({ type: "Stop" });
    }
    return true;
  });
  body.closest<HTMLElement>(".modal-box")?.classList.add("match-dialog-box");
  subscribe(() => {
    if (!pending) return;
    const phase = state.view?.engine_match?.phase;
    if (phase === "Playing" || phase === "Paused") {
      pending = false;
      started = true;
      void dialog.close();
    } else if (phase) seen = true;
    else if (seen) {
      // StartObserver.startFailed: the failure message comes as the match's notice.
      pending = false;
      ok.disabled = false;
    }
  });

  async function apply(): Promise<void> {
    const now = state.view?.settings.matches;
    if (!now) return;
    if (black.selectedIndex < 0 || white.selectedIndex < 0) {
      await message(t("Message.title"), t("NewAnaGameDialog.noEngineHint"));
      return;
    }
    if (sgf.box.checked) {
      if (now.openings.length === 0) {
        await message(t("Message.title"), t("NewEngineGameDialog.message"));
        return;
      }
      if (now.openings.length > 1 && !f.batchOn.checked) await message(t("Message.title"), t("NewEngineGameDialog.multiSgfNotBatch"));
    }
    const komiValue = leadingDouble(komi.value);
    if (komiValue === null) {
      await message(t("Message.title"), t("Menu.inputDoubleTip"));
      return;
    }
    // parseEngineGameHandicap: a disabled or blank field is an even game.
    const handicapValue = handicap.disabled || handicap.value.trim() === "" ? 0 : leadingInt(handicap.value);
    if (handicapValue === null) {
      await message(t("Message.title"), t("Menu.inputIntegerHint"));
      return;
    }
    const base = now.settings;
    const firstMode = TIME_MODES[modeB.selectedIndex] ?? "Fixed";
    const secondMode = TIME_MODES[modeW.selectedIndex] ?? "Fixed";
    const settings: MatchSettings = {
      ...base,
      ...resign.read(base),
      first: identity(black.selectedIndex),
      second: identity(white.selectedIndex),
      komi: komiValue,
      handicap: Math.max(0, handicapValue),
      rules: rules.value(),
      sgfStart: sgf.box.checked,
      ponder: ponder.box.checked,
      firstTimeMode: firstMode,
      secondTimeMode: secondMode,
      advancedTime: firstMode === "RawAdvanced" || secondMode === "RawAdvanced",
      firstTimeCommand: firstMode === "RawAdvanced" ? advB.value.trim() : base.firstTimeCommand,
      secondTimeCommand: secondMode === "RawAdvanced" ? advW.value.trim() : base.secondTimeCommand,
    };
    ok.disabled = true;
    setEngineMatchChecked(true);
    const r = await match({ type: "Start", settings, session: sharedSession(f, now.session), first_engine: black.selectedIndex, second_engine: white.selectedIndex, disable_wrn: wrn.box.checked });
    if (r && r.type !== "Refused") {
      pending = true;
      seen = !!state.view?.engine_match;
      const phase = state.view?.engine_match?.phase;
      if (phase === "Playing" || phase === "Paused") {
        pending = false;
        started = true;
        await dialog.close();
      }
    } else ok.disabled = false;
  }
  await dialog.closed;
  dialogOpen = false;
  // A cancelled dialog unchecks 引擎对战 (and with it the toolbar's engine boxes).
  if (!started && !matchActive()) setEngineMatchChecked(false);
}

// The dialog an AI 陪练 exit opens afterwards (deferUntilHumanSlExit(this::startEngineGameDialog)).
registerHumanSlContinuation("EngineGame", () => void openEngineGameDialog());

/** The 黑方/白方 认输预置 rows: 最小手数, 连续 … 手,胜率低于 … %. */
function resignRows(cfg: MatchSettings) {
  const rows = el("div", "match-resign");
  const side = (label: string, p: MatchSettings["firstResign"], key: string) => {
    const min = field(String(p.minMove), `match.${key}MinMove`, "int", 58);
    const moves = field(String(p.consecutiveMoves), `match.${key}ResignMoves`, "int", 58);
    const rate = field(javaDouble(p.winrate), `match.${key}ResignWinrate`, "double", 68);
    rows.append(
      el("span", "", label), el("span", "", t("EnginePkConfig.lblresignSettingConsistent")), min,
      el("span", "", t("EnginePkConfig.lblresignSetting2")), moves,
      el("span", "", t("EnginePkConfig.lblresignSetting3")), rate, el("span", "", "%"),
    );
    return { fields: [min, moves, rate], read: (keep: MatchSettings["firstResign"]) => ({ minMove: intOr(min, keep.minMove), consecutiveMoves: intOr(moves, keep.consecutiveMoves), winrate: doubleOr(rate, keep.winrate) }) };
  };
  const b = side(t("EnginePkConfig.lblresignSettingBlack"), cfg.firstResign, "black");
  const w = side(t("EnginePkConfig.lblresignSettingWhite"), cfg.secondResign, "white");
  return {
    rows,
    fields: [...b.fields, ...w.fields],
    read: (base: MatchSettings) => ({ firstResign: b.read(base.firstResign), secondResign: w.read(base.secondResign) }),
  };
}

/** EnginePkConfig: from the toolbar (允许后台计算 and 认输预置 shown) or from 更多设置 (neither).
 *  Resolves when it closes. */
export async function openEnginePkConfig(fromToolbar: boolean): Promise<void> {
  const v = state.view;
  if (!v) return;
  const m = v.settings.matches;
  const cfg = m.settings;
  const s = m.session;
  const genmove = radio("match-mode", s.kind === "Genmove", "match.genmove", t("EnginePkConfig.rdoGenmove"));
  const analysis = radio("match-mode", s.kind !== "Genmove", "match.analysisMode", t("EnginePkConfig.rdoAna"));
  const about = button("?", () => void message(t("LizzieFrame.aboutAnalyzeGenmoveInfoTitle"), plainJava(t("LizzieFrame.aboutAnalyzeGenmoveInfo"))));
  about.className = "match-help";
  about.dataset.key = "match.modeHelp";
  const ponder = check(cfg.ponder, "match.configPonder", t("NewEngineGameDialog.checkBoxAllowPonder"));
  ponder.box.onchange = () => void run(matchValues({ settings: { ponder: ponder.box.checked } }));
  const exchange = check(s.exchangeColors, "match.exchange", t("EnginePkConfig.chkExchange"));
  const maxOn = check(s.maxMovesEnabled, "match.maxMovesOn", t("EnginePkConfig.lblGameMAX"));
  const maxMoves = field(String(s.maxMoves), "match.maxMoves", "int", 72);
  const autoSave = check(s.autoSave, "match.autoSave", t("EnginePkConfig.chkAutosave"));
  const saveWinrate = check(s.saveWinrate, "match.saveWinrate", t("EnginePkConfig.chkSaveWinrate"));
  const batchName = field(s.batchName, "match.batchName", null, 240);
  const startOn = check(s.startNumberEnabled, "match.startNumberOn", t("EnginePkConfig.chkSatartNum"));
  const startNumber = field(String(s.startNumber), "match.startNumber", "int", 72);
  const randomOn = check(s.randomMovesEnabled, "match.randomMovesOn", t("EnginePkConfig.chkRandomMove"));
  const randomMoves = field(s.randomMoves > 0 ? String(s.randomMoves) : "", "match.randomMoves", "int", 64);
  const randomDiff = field(javaDouble(s.randomWinrateDiff), "match.randomWinrate", "double", 64);
  const visitsOn = check(cfg.checkRandomVisits, "match.randomVisitsOn", t("EnginePkConfig.chkRandomMoveVists"));
  const visitsPercent = field(javaDouble(cfg.randomVisitsPercent), "match.randomVisits", "double", 72);
  const resign = resignRows(cfg);
  const current = radio("match-bestmoves", !cfg.showPreviousBestmoves, "match.currentMove", t("EnginePkConfig.rdoCurrentMove"));
  const previous = radio("match-bestmoves", cfg.showPreviousBestmoves, "match.lastMove", t("EnginePkConfig.rdoLastMove"));
  const onlyFirst = check(cfg.previousBestmoveOnly, "match.onlyFirstMove", t("EnginePkConfig.chkPreviousBestmovesOnlyFirstMove"));
  const hint = el("div", "match-hint", t("EnginePkConfig.textAreaHint"));

  const percent = () => el("span", "", "%");
  const modeRows: HTMLElement[] = [inline(genmove.wrap, analysis.wrap, about)];
  if (fromToolbar) modeRows.push(ponder.wrap);
  const sections = [
    section(...modeRows),
    section(
      inline(exchange.wrap, maxOn.wrap, maxMoves),
      inline(autoSave.wrap, saveWinrate.wrap),
      inline(el("span", "", t("EnginePkConfig.lblnameSetting")), batchName),
      inline(startOn.wrap, startNumber),
    ),
    section(
      inline(randomOn.wrap, randomMoves, el("span", "", t("EnginePkConfig.lblRandomWinrate")), randomDiff, percent()),
      inline(visitsOn.wrap, visitsPercent, percent()),
    ),
  ];
  if (fromToolbar) sections.push(section(resign.rows));
  sections.push(section(inline(el("span", "", t("EnginePkConfig.lblChooseBestMoves")), current.wrap, previous.wrap), onlyFirst.wrap), section(hint));
  // setTextEnable: Genmove模式 disables 认输预置 and 随机落子.
  const textEnable = () => {
    const on = analysis.box.checked;
    for (const control of [...resign.fields, randomOn.box, randomMoves, randomDiff, visitsOn.box, visitsPercent]) control.disabled = !on;
  };
  genmove.box.onchange = analysis.box.onchange = textEnable;
  textEnable();
  const candidates = () => (onlyFirst.box.disabled = !previous.box.checked);
  current.box.onchange = previous.box.onchange = candidates;
  candidates();

  const body = el("div", "match-dialog");
  const main = el("div", "match-scroll");
  main.append(...sections);
  const cancel = button(t("EnginePkConfig.cancelButton"), () => void dialog.close());
  cancel.dataset.key = "match.configCancel";
  const ok = button(t("EnginePkConfig.okButton"), () => void apply());
  ok.dataset.key = "match-config-ok";
  const bar = el("div", "match-buttons");
  bar.append(el("span", "match-fill"), cancel, ok);
  body.append(main, bar);
  const dialog = formPanel(t("EnginePkConfig.title"), body);
  body.closest<HTMLElement>(".modal-box")?.classList.add("match-dialog-box", fromToolbar ? "match-config-toolbar" : "match-config");

  /** applyChange: each number field keeps its value when it does not parse. */
  async function apply(): Promise<void> {
    const now = state.view?.settings.matches;
    if (!now) return;
    const base = now.settings;
    const session = now.session;
    const r = await run(matchValues({
      settings: {
        ...resign.read(base),
        showPreviousBestmoves: previous.box.checked,
        previousBestmoveOnly: onlyFirst.box.checked,
        checkRandomVisits: visitsOn.box.checked,
        randomVisitsPercent: doubleOr(visitsPercent, base.randomVisitsPercent),
      },
      session: {
        startNumber: intOr(startNumber, session.startNumber),
        startNumberEnabled: startOn.box.checked,
        autoSave: autoSave.box.checked,
        kind: genmove.box.checked ? "Genmove" : "Analysis",
        batchName: batchName.value,
        exchangeColors: exchange.box.checked,
        randomMovesEnabled: randomOn.box.checked,
        saveWinrate: saveWinrate.box.checked,
        randomMoves: parseJavaInt(randomMoves.value.trim()) ?? session.randomMoves,
        randomWinrateDiff: doubleOr(randomDiff, session.randomWinrateDiff),
        maxMovesEnabled: maxOn.box.checked,
        maxMoves: parseJavaInt(maxMoves.value.trim()) ?? session.maxMoves,
      },
    }));
    if (r && r.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

/** The one 人工干预 window (Manual: non-modal, always on top, 189x165). */
let manual: HTMLElement | null = null;

/** Manual.java: 立即落子 and 允许/关闭人工落子 (分析模式 only), 黑认输 and 白认输. */
export function openManual(): void {
  if (manual) {
    manual.hidden = false;
    return;
  }
  const box = el("div", "modal-box match-manual");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", t("Mannul.title"));
  box.dataset.freeWindow = "";
  const head = el("div", "modal-title", t("Mannul.title"));
  const close = el("button", "match-close", "×");
  close.dataset.key = "match.manualClose";
  close.setAttribute("aria-label", product("close"));
  close.onclick = () => (box.hidden = true);
  head.append(close);
  const panel = absolute(173, 126);
  const push = (key: string, y: number, op: MatchIntervention) => {
    const b = place(panel, el("button"), 10, y, 158, 29);
    b.dataset.key = key;
    b.onclick = () => void match({ type: "Intervention", op });
    return b;
  };
  const playNow = push("match.playNow", 5, { type: "PlayNow" });
  playNow.textContent = t("Mannul.playNow");
  const manualOne = push("match.manualOne", 35, { type: "ToggleManual" });
  const resign = (side: Stone, y: number, key: string, text: string) => {
    const b = push(key, y, { type: "Resign", side });
    b.textContent = text;
    return b;
  };
  const blackResign = resign("Black", 65, "match.blackResign", t("Mannul.blackResign"));
  const whiteResign = resign("White", 95, "match.whiteResign", t("Mannul.whiteResign"));
  box.append(head, panel);
  document.body.append(box);
  manual = box;
  const sync = () => {
    const m = state.view?.engine_match;
    playNow.disabled = !m?.can_play_now;
    manualOne.disabled = !m?.can_toggle_manual;
    blackResign.disabled = whiteResign.disabled = !m?.can_resign;
    const text = m?.manual_moves ? t("Mannul.manualOne.disable") : t("Mannul.manualOne.enable");
    if (manualOne.textContent !== text) manualOne.textContent = text;
  };
  subscribe(sync);
  sync();
}

/** 修改多盘引擎对局数: JOptionPane.showInputDialog, then reviseLiveBatchLimit; text that is not an
 *  integer shows Menu.inputIntegerHint. */
export async function reviseBatchLimit(): Promise<void> {
  const body = el("div", "match-input");
  const input = field("", "match.batchLimitInput");
  body.append(el("div", "", t("Menu.changeEngineGameNumbersMessage")), input);
  const asked = ask(t("Menu.changeEngineGameNumbersTitle"), body, product("ok"), product("cancel"));
  input.focus();
  if (!(await asked)) return;
  const limit = parseJavaInt(input.value.trim());
  if (limit === null || limit < 0) {
    await message(t("Message.title"), t("Menu.inputIntegerHint"));
    return;
  }
  await match({ type: "ReviseBatchLimit", limit });
}

let messages = 0;

/** Utils.showMsgNoModal: the Message window, which leaves the main window usable. */
export function messageNoModal(text: string): void {
  const box = el("div", "modal-box match-notice");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", t("Message.title"));
  box.dataset.freeWindow = "";
  box.style.translate = `${(messages % 5) * 16}px ${(messages % 5) * 16}px`;
  messages++;
  const ok = button(product("ok"), () => box.remove());
  ok.dataset.key = "match.noticeOk";
  const row = el("div", "modal-buttons");
  row.append(ok);
  box.append(el("div", "modal-title", t("Message.title")), el("div", "modal-body", text), row);
  document.body.append(box);
}
