// 引擎对战 windows: 引擎对局 (NewEngineGameDialog.java), 引擎对局设置 (EnginePkConfig.java, from
// 更多设置), 人工干预 (Manual.java), 修改多盘引擎对局数 (Menu.java:3121-3147) and the
// non-modal end messages (Utils.showMsgNoModal). Each holds a draft; committed values come from
// the settings view and every change is one Match command. Rust owns the match.
import { run, state, subscribe } from "../app";
import { openMatchOpenings } from "../contract/client";
import { errorText } from "../errorText";
import { MATCH_RULE_OPTIONS } from "../contract/generated";
import type { MatchCommand, MatchIntervention, MatchSessionSettings, MatchSettings, MatchTimeMode } from "../contract/generated";
import { icon } from "../design/icons";
import { ask, closeBox, formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { button, footer, groupNote, groupTitle, primaryButton, row, segmented, switchRow } from "../engine/controls";
import { composeMatchRules } from "../engine/rules";
import { plainJava } from "../engine/tuningText";
import { product, registerProduct, t } from "../i18n";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { engineIdentity as identity, matchValues } from "./state";
import { endHumanSlFirst, registerHumanSlContinuation } from "../humansl/state";
import { locateControl } from "../search/highlight";
import "./match.css";

registerProduct({
  ZhCn: { matchGroupGame: "对局", matchGroupOptions: "选项", matchGroupKifu: "棋谱", matchGroupRandomMove: "随机落子", matchMoveMethod: "落子方式", matchCandidatesOf: "显示哪一手的选点", matchOpeningFailed: "开局参数未能确认，引擎对局未能开始。{0}" },
  ZhTw: { matchGroupGame: "對局", matchGroupOptions: "選項", matchGroupKifu: "棋譜", matchGroupRandomMove: "隨機落子", matchMoveMethod: "落子方式", matchCandidatesOf: "顯示哪一手的選點", matchOpeningFailed: "開局參數未能確認，引擎對局未能開始。{0}" },
  EnUs: { matchGroupGame: "Game", matchGroupOptions: "Options", matchGroupKifu: "Game records", matchGroupRandomMove: "Random moves", matchMoveMethod: "Move selection", matchCandidatesOf: "Show candidates of", matchOpeningFailed: "The opening parameters could not be confirmed, so the engine game did not start. {0}" },
  JaJp: { matchGroupGame: "対局", matchGroupOptions: "オプション", matchGroupKifu: "棋譜", matchGroupRandomMove: "ランダム着手", matchMoveMethod: "着手方式", matchCandidatesOf: "候補手を表示する手", matchOpeningFailed: "開始時のパラメータを確認できなかったため、エンジン対局を開始できませんでした。{0}" },
  KoKr: { matchGroupGame: "대국", matchGroupOptions: "옵션", matchGroupKifu: "기보", matchGroupRandomMove: "무작위 착수", matchMoveMethod: "착수 방식", matchCandidatesOf: "후보수를 표시할 수", matchOpeningFailed: "시작 매개변수를 확인하지 못해 엔진 대국을 시작하지 못했습니다. {0}" },
  ThTh: { matchGroupGame: "เกม", matchGroupOptions: "ตัวเลือก", matchGroupKifu: "บันทึกหมาก", matchGroupRandomMove: "เดินสุ่ม", matchMoveMethod: "วิธีเดินหมาก", matchCandidatesOf: "แสดงตัวเลือกของตา", matchOpeningFailed: "ยืนยันพารามิเตอร์เริ่มต้นไม่ได้ จึงไม่สามารถเริ่มเกมเอนจินได้ {0}" },
});

function match(op: MatchCommand) {
  return run({ type: "Match", op });
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** A JCheckBox as a switch row whose switch carries the automation key; `extra` are the controls
 *  it turns on, standing before the switch. */
function toggle(host: HTMLElement, title: string, sub: string, checked: boolean, key: string, ...extra: (Node | string)[]): HTMLInputElement {
  const box = switchRow(host, title, sub, checked, ...extra).input;
  box.dataset.key = key;
  return box;
}

/** A Java label as a row or group name: without its trailing colon. */
const named = (text: string) => text.replace(/\s*[:：]\s*$/, "");

/** IntDocument (digits), DoubleDocument (digits and a point) or KomiDocument (signed decimal). */
function field(value: string, key: string, filter: "int" | "double" | "komi" | null = null): HTMLInputElement {
  const f = el("input");
  f.type = "text";
  f.value = value;
  f.dataset.key = key;
  if (filter) {
    f.inputMode = filter === "int" ? "numeric" : "decimal";
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
interface SharedFields {
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
function sharedSession(f: SharedFields, base: MatchSessionSettings): MatchSessionSettings {
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

/** A 帮助 button: the message it shows is its name. */
function helpButton(key: string, title: string, text: string): HTMLButtonElement {
  const help = button("", () => void message(title, plainJava(text)));
  help.append(icon("help"));
  help.className = "match-help";
  help.title = title;
  help.setAttribute("aria-label", title);
  help.dataset.key = key;
  return help;
}

type MatchRuleOption = (typeof MATCH_RULE_OPTIONS)[number];

/** MatchRulesPicker: the standard choices as the core defines them (the Chinese rule sets first,
 *  then Java's order), then 自定义. A standard choice is sent as its rules and remembered by its id,
 *  which tells the Chinese choices with the same rules apart; 自定义 as the composed rules
 *  (SetKataRules compose mode); cancelling the editor returns to the previous choice. `picked` hears
 *  of every standard choice the user makes. */
function rulesPicker(prefill: string, choice: string | null, picked: (option: MatchRuleOption) => void) {
  const s = select([...MATCH_RULE_OPTIONS.map((o) => t(o.labelKey)), t("NewEngineGameDialog.matchRules.custom")], 0, "match.rules");
  let chosen = MATCH_RULE_OPTIONS.find((o) => o.id === choice) ?? null;
  let custom = prefill;
  const index = () => (chosen ? MATCH_RULE_OPTIONS.indexOf(chosen) : MATCH_RULE_OPTIONS.length);
  s.selectedIndex = index();
  s.onchange = async () => {
    const option = MATCH_RULE_OPTIONS[s.selectedIndex];
    if (option) {
      chosen = option;
      picked(option);
      return;
    }
    const edited = await composeMatchRules(custom);
    if (edited !== null) {
      custom = edited;
      chosen = null;
    }
    s.selectedIndex = index();
  };
  return { el: s, rules: () => chosen?.rules ?? custom, choice: () => chosen?.id ?? null };
}

const TIME_MODES: MatchTimeMode[] = ["EngineOwned", "Fixed", "RawAdvanced"];

/** The open 引擎对局 dialog, so a second Alt+E does not stack another. */
let dialogOpen = false;

/** LizzieFrame.startEngineGameDialog (13412-13457) and NewEngineGameDialog. Function search's
 *  开局广度 / 开局激进度 open it with the focus on 引擎 1's field of that row and the gold line
 *  round the row. */
export async function openEngineGameDialog(locate: "wrn" | "pda" | null = null): Promise<void> {
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
  const cfg = m.settings;
  const timeBlack = field(shown(s.firstSeconds), "match.timeBlack", "int");
  const timeWhite = field(shown(s.secondSeconds), "match.timeWhite", "int");
  const visitsBlack = field(shown(s.firstVisits), "match.visitsBlack", "int");
  const visitsWhite = field(shown(s.secondVisits), "match.visitsWhite", "int");
  const firstBlack = field(shown(s.firstMoveVisits), "match.firstVisitsBlack", "int");
  const firstWhite = field(shown(s.secondMoveVisits), "match.firstVisitsWhite", "int");
  const timeModes = [t("NewGameDialog.engineOwnedTime"), t("NewGameDialog.time"), t("NewEngineGameDialog.lblAdvanceTime")];
  const modeB = select(timeModes, TIME_MODES.indexOf(cfg.firstTimeMode), "match.timeModeBlack");
  const modeW = select(timeModes, TIME_MODES.indexOf(cfg.secondTimeMode), "match.timeModeWhite");
  const advB = field(cfg.firstTimeCommand, "match.advancedBlack");
  const advW = field(cfg.secondTimeCommand, "match.advancedWhite");
  // 开局广度 / 开局激进度 (Java NewEngineGameDialog): saved for 引擎 1 and 引擎 2, the two columns, and
  // applied at the start of every analysis-mode game; exchanging colours does not exchange them.
  // Plain text fields like Java's: 确定 parses what was typed, a sign or an exponent included.
  const openingTip = t("NewEngineGameDialog.openingParametersTip");
  const openingField = (value: number, key: string) => {
    const input = field(String(value), key);
    input.inputMode = "decimal";
    input.title = openingTip;
    return input;
  };
  const wrnFirst = openingField(cfg.opening.firstWrn, "match.openingWrnFirst");
  const wrnSecond = openingField(cfg.opening.secondWrn, "match.openingWrnSecond");
  const pdaFirst = openingField(cfg.opening.firstPda, "match.openingPdaFirst");
  const pdaSecond = openingField(cfg.opening.secondPda, "match.openingPdaSecond");
  // The two columns share each row's name; every control also says whose side it sets.
  const sideNames = [t("NewEngineGameDialog.lblB"), t("NewEngineGameDialog.lblW")];
  for (const [name, pair] of [
    [t("NewEngineGameDialog.lblengine"), [black, white]],
    [t("NewEngineGameDialog.lblTime"), [timeBlack, timeWhite]],
    [t("SetAiTimes.lblGenmoveMode"), [modeB, modeW]],
    [t("NewEngineGameDialog.lblAdvanceTime"), [advB, advW]],
    [t("NewEngineGameDialog.lblPlayout"), [visitsBlack, visitsWhite]],
    [t("NewEngineGameDialog.lblFirstPlayout"), [firstBlack, firstWhite]],
  ] as const) {
    pair.forEach((control, k) => control.setAttribute("aria-label", `${sideNames[k]} · ${name}`));
  }
  const positionNames = [t("EngineGameParameters.position1"), t("EngineGameParameters.position2")];
  for (const [name, pair] of [
    [t("NewEngineGameDialog.lblOpeningWrn"), [wrnFirst, wrnSecond]],
    [t("NewEngineGameDialog.lblOpeningPda"), [pdaFirst, pdaSecond]],
  ] as const) {
    pair.forEach((control, k) => control.setAttribute("aria-label", `${positionNames[k]} · ${name}`));
  }

  // 黑方设置 / 白方设置: a column each at the end of the rows, under its stone. A row that one of the
  // toolbar's own boxes turns on ends in its switch; the other rows keep that place empty.
  const body = el("div", "match-dialog");
  const sides = el("div", "match-sides");
  body.append(sides);
  const head = el("div", "match-side-head");
  for (const [stone, text] of [["stone-black", t("NewEngineGameDialog.lblB")], ["stone-white", t("NewEngineGameDialog.lblW")]] as const) {
    const h = el("span");
    h.append(icon(stone), text);
    head.append(h);
  }
  head.append(el("span", "match-slot"));
  sides.append(head);
  row(sides, t("NewEngineGameDialog.lblengine"), "", black, white, el("span", "match-slot"));
  const timeOn = toggle(sides, t("NewEngineGameDialog.lblTime"), "", s.timeEnabled, "match.timeOn", timeBlack, timeWhite);
  // The time choice only counts in Genmove模式.
  row(sides, t("SetAiTimes.lblGenmoveMode"), "", modeB, modeW, el("span", "match-slot"));
  const advName = el("span", "match-name", t("NewEngineGameDialog.lblAdvanceTime"));
  advName.append(helpButton("match.advancedTimeHelp", t("AdvanceTimeSettings.title"), t("AdvanceTimeSettings.describe")));
  row(sides, advName, "", advB, advW, el("span", "match-slot"));
  const visitsOn = toggle(sides, t("NewEngineGameDialog.lblPlayout"), "", s.visitsEnabled, "match.visitsOn", visitsBlack, visitsWhite);
  const firstOn = toggle(sides, t("NewEngineGameDialog.lblFirstPlayout"), "", s.firstMoveVisitsEnabled, "match.firstVisitsOn", firstBlack, firstWhite);
  const wrnRow = row(sides, t("NewEngineGameDialog.lblOpeningWrn"), "", wrnFirst, wrnSecond, el("span", "match-slot"));
  const pdaRow = row(sides, t("NewEngineGameDialog.lblOpeningPda"), "", pdaFirst, pdaSecond, el("span", "match-slot"));
  wrnRow.title = pdaRow.title = openingTip;

  // 认输预置: hidden in Genmove mode, where the note takes its place.
  const resignHint = groupNote(body, t("EnginePkConfig.lblresignGenmove"));
  const resign = resignRows(body, cfg);

  // 贴目 / 让子, 本局规则, 多盘, 当前局面续弈 / 加载开局SGF with 顺序/随机 and 多选SGF.
  groupTitle(body, product("matchGroupGame"));
  const komi = field(javaDouble(cfg.komi), "match.komi", "komi");
  row(body, t("NewEngineGameDialog.komi"), "", komi);
  const handicap = field(String(cfg.handicap), "match.handicap", "int");
  const handicapRow = row(body, t("NewEngineGameDialog.handicap"), "", handicap);
  // Picking rules fills in their komi, which stays editable; a handicap game (as the start counts
  // its stones) keeps its komi, and AGA/BGA or New Zealand have none to give.
  const rules = rulesPicker(m.rules_prefill, m.rules_choice, (option) => {
    const stones = handicap.disabled || handicap.value.trim() === "" ? 0 : leadingInt(handicap.value) ?? 0;
    if (option.komi === null || stones > 0) return;
    const text = javaDouble(option.komi);
    if (komi.value !== text) komi.value = text;
  });
  row(body, t("NewEngineGameDialog.matchRules"), "", rules.el);
  const batch = field(shown(s.batchLimit), "match.batch", "int");
  const batchOn = toggle(body, t("NewEngineGameDialog.lblBatchGame").trim(), "", s.batchEnabled, "match.batchOn", batch);
  const continuePlay = toggle(body, t("NewEngineGameDialog.lblContinue"), "", s.continuePosition, "match.continue");
  const random = select([t("NewEngineGameDialog.cbxRandomSgf1"), t("NewEngineGameDialog.cbxRandomSgf2")], cfg.sgfRandom ? 1 : 0, "match.sgfRandom");
  const chooseSgf = button(t("NewEngineGameDialog.btnSGFstart"), () => void openMatchOpenings([]).then(
    (r) => { if (r.type === "Refused") void message(t("Message.title"), t(r.message_key), "warning"); },
    (e: unknown) => message(t("Message.title"), errorText(e), "error"),
  ));
  chooseSgf.dataset.key = "match.chooseSgf";
  const sgf = toggle(body, t("NewEngineGameDialog.lblsgf").trim(), "", cfg.sgfStart, "match.sgfStart", random, chooseSgf);
  groupTitle(body, product("matchGroupOptions"));
  const ponder = toggle(body, t("NewEngineGameDialog.checkBoxAllowPonder"), "", cfg.ponder, "match.ponder");
  const f: SharedFields = { black, white, timeOn, timeBlack, timeWhite, visitsOn, visitsBlack, visitsWhite, firstOn, firstBlack, firstWhite, batchOn, batch, continuePlay };

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
    chooseSgf.disabled = random.disabled = fixed || !sgf.checked;
    handicap.disabled = fixed || sgf.checked;
    handicapRow.classList.toggle("disabled", handicap.disabled);
  };
  function modeChanged(): void {
    // The opening parameters only count in an analysis-mode game.
    wrnRow.hidden = pdaRow.hidden = genmove();
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
  // 当前局面续弈 and 加载开局SGF exclude each other; 加载开局SGF, 顺序/随机 and 允许后台计算 are
  // saved as they change.
  f.continuePlay.onchange = () => {
    const clearSgf = f.continuePlay.checked && sgf.checked;
    if (clearSgf) sgf.checked = false;
    openingRows();
    const now = state.view?.settings.matches;
    if (now) void run(matchValues({ settings: clearSgf ? { sgfStart: false } : {}, session: sharedSession(f, now.session) }));
  };
  sgf.onchange = () => {
    if (sgf.checked) f.continuePlay.checked = false;
    openingRows();
    const now = state.view?.settings.matches;
    if (now) void run(matchValues({ settings: { sgfStart: sgf.checked }, session: sharedSession(f, now.session) }));
  };
  random.onchange = () => void run(matchValues({ settings: { sgfRandom: random.selectedIndex === 1 } }));
  ponder.onchange = () => void run(matchValues({ settings: { ponder: ponder.checked } }));
  if (f.continuePlay.checked) sgf.checked = false;
  openingRows();
  modeChanged();

  // pendingEngineGameStart: accepted, waiting for the first move; closing it stops the start.
  let pending = false;
  /** The accepted start has shown its match, so its disappearing means the start failed. */
  let seen = false;
  const dialog = formPanel(t("NewEngineGameDialog.title"), body, async () => {
    if (pending) {
      pending = false;
      await match({ type: "Stop" });
    }
    return true;
  });
  // 更多设置 at the footer's start: it opens 引擎对局设置 and leaves this dialog open.
  const more = button(t("NewEngineGameDialog.btnConfig"), () => void openEnginePkConfig().then(modeChanged));
  more.dataset.key = "match.moreSettings";
  const ok = primaryButton(t("NewEngineGameDialog.okButton"), () => void apply());
  ok.dataset.key = "match-dialog-ok";
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "match.cancel";
  footer(body, { start: [more], primary: ok, cancel });
  if (locate === "wrn") locateControl(wrnFirst, wrnRow);
  else if (locate === "pda") locateControl(pdaFirst, pdaRow);
  subscribe(() => {
    if (!pending) return;
    const phase = state.view?.engine_match?.phase;
    if (phase === "Playing" || phase === "Paused") {
      pending = false;
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
    if (sgf.checked) {
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
    // EngineGameOpeningParameters.parseWrn / parsePda: a number within range, else the message;
    // a Genmove game keeps the saved values.
    let opening = now.settings.opening;
    if (!genmove()) {
      const parse = (input: HTMLInputElement, max: number, min: number): number | null => {
        const value = parseJavaDouble(input.value);
        return value !== null && value >= min && value <= max ? value : null;
      };
      const [firstWrn, secondWrn] = [parse(wrnFirst, 2, 0), parse(wrnSecond, 2, 0)];
      if (firstWrn === null || secondWrn === null) {
        await message(t("Message.title"), t("EngineGameParameters.invalidWrn"));
        return;
      }
      const [firstPda, secondPda] = [parse(pdaFirst, 3, -3), parse(pdaSecond, 3, -3)];
      if (firstPda === null || secondPda === null) {
        await message(t("Message.title"), t("EngineGameParameters.invalidPda"));
        return;
      }
      opening = { firstWrn, secondWrn, firstPda, secondPda };
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
      rules: rules.rules(),
      rulesChoice: rules.choice(),
      sgfStart: sgf.checked,
      ponder: ponder.checked,
      firstTimeMode: firstMode,
      secondTimeMode: secondMode,
      advancedTime: firstMode === "RawAdvanced" || secondMode === "RawAdvanced",
      firstTimeCommand: firstMode === "RawAdvanced" ? advB.value.trim() : base.firstTimeCommand,
      secondTimeCommand: secondMode === "RawAdvanced" ? advW.value.trim() : base.secondTimeCommand,
      opening,
    };
    ok.disabled = true;
    const r = await match({ type: "Start", settings, session: sharedSession(f, now.session), first_engine: black.selectedIndex, second_engine: white.selectedIndex });
    if (r && r.type !== "Refused") {
      pending = true;
      seen = !!state.view?.engine_match;
      const phase = state.view?.engine_match?.phase;
      if (phase === "Playing" || phase === "Paused") {
        pending = false;
        await dialog.close();
      }
    } else ok.disabled = false;
  }
  await dialog.closed;
  dialogOpen = false;
}

// The dialog an AI 陪练 exit opens afterwards (deferUntilHumanSlExit(this::startEngineGameDialog)).
registerHumanSlContinuation("EngineGame", () => void openEngineGameDialog());

/** The 黑方/白方 认输预置 rows, each 最小手数, 连续 … 手,胜率低于 … %: a group of their own, `rows`. */
function resignRows(host: HTMLElement, cfg: MatchSettings) {
  const rows = el("div", "match-resign");
  host.append(rows);
  const side = (label: string, p: MatchSettings["firstResign"], key: string) => {
    const min = field(String(p.minMove), `match.${key}MinMove`, "int");
    const moves = field(String(p.consecutiveMoves), `match.${key}ResignMoves`, "int");
    const rate = field(javaDouble(p.winrate), `match.${key}ResignWinrate`, "double");
    row(rows, named(label), "", t("EnginePkConfig.lblresignSettingConsistent"), min, t("EnginePkConfig.lblresignSetting2"), moves, t("EnginePkConfig.lblresignSetting3"), rate, "%");
    return (keep: MatchSettings["firstResign"]) => ({ minMove: intOr(min, keep.minMove), consecutiveMoves: intOr(moves, keep.consecutiveMoves), winrate: doubleOr(rate, keep.winrate) });
  };
  const black = side(t("EnginePkConfig.lblresignSettingBlack"), cfg.firstResign, "black");
  const white = side(t("EnginePkConfig.lblresignSettingWhite"), cfg.secondResign, "white");
  return { rows, read: (base: MatchSettings) => ({ firstResign: black(base.firstResign), secondResign: white(base.secondResign) }) };
}

/** EnginePkConfig as 更多设置 opens it (without 允许后台计算 and 认输预置, which the 引擎对局 dialog
 *  shows). Resolves when it closes. */
async function openEnginePkConfig(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const m = v.settings.matches;
  const cfg = m.settings;
  const s = m.session;
  const body = el("div", "match-dialog match-config");
  const mode = segmented("match-mode", [["Genmove", t("EnginePkConfig.rdoGenmove")], ["Analysis", t("EnginePkConfig.rdoAna")]], s.kind === "Genmove" ? "Genmove" : "Analysis");
  const [genmove, analysis] = mode.inputs;
  genmove.dataset.key = "match.genmove";
  analysis.dataset.key = "match.analysisMode";
  row(body, product("matchMoveMethod"), "", mode.el, helpButton("match.modeHelp", t("LizzieFrame.aboutAnalyzeGenmoveInfoTitle"), t("LizzieFrame.aboutAnalyzeGenmoveInfo")));
  const exchange = toggle(body, t("EnginePkConfig.chkExchange"), "", s.exchangeColors, "match.exchange");
  const maxMoves = field(String(s.maxMoves), "match.maxMoves", "int");
  const maxOn = toggle(body, t("EnginePkConfig.lblGameMAX"), "", s.maxMovesEnabled, "match.maxMovesOn", maxMoves);
  groupTitle(body, product("matchGroupKifu"));
  const autoSave = toggle(body, t("EnginePkConfig.chkAutosave"), "", s.autoSave, "match.autoSave");
  const saveWinrate = toggle(body, t("EnginePkConfig.chkSaveWinrate"), "", s.saveWinrate, "match.saveWinrate");
  const batchName = field(s.batchName, "match.batchName");
  row(body, named(t("EnginePkConfig.lblnameSetting")), "", batchName);
  const startNumber = field(String(s.startNumber), "match.startNumber", "int");
  const startOn = toggle(body, t("EnginePkConfig.chkSatartNum"), "", s.startNumberEnabled, "match.startNumberOn", startNumber);
  groupTitle(body, product("matchGroupRandomMove"));
  const randomMoves = field(s.randomMoves > 0 ? String(s.randomMoves) : "", "match.randomMoves", "int");
  const randomDiff = field(javaDouble(s.randomWinrateDiff), "match.randomWinrate", "double");
  const randomOn = toggle(body, t("EnginePkConfig.chkRandomMove"), "", s.randomMovesEnabled, "match.randomMovesOn", randomMoves, t("EnginePkConfig.lblRandomWinrate"), randomDiff, "%");
  const visitsPercent = field(javaDouble(cfg.randomVisitsPercent), "match.randomVisits", "double");
  const visitsOn = toggle(body, t("EnginePkConfig.chkRandomMoveVists"), "", cfg.checkRandomVisits, "match.randomVisitsOn", visitsPercent, "%");
  groupTitle(body, named(t("EnginePkConfig.lblChooseBestMoves")));
  const bestmoves = segmented("match-bestmoves", [["current", t("EnginePkConfig.rdoCurrentMove")], ["previous", t("EnginePkConfig.rdoLastMove")]], cfg.showPreviousBestmoves ? "previous" : "current");
  const [current, previous] = bestmoves.inputs;
  current.dataset.key = "match.currentMove";
  previous.dataset.key = "match.lastMove";
  row(body, product("matchCandidatesOf"), "", bestmoves.el);
  const onlyFirst = toggle(body, t("EnginePkConfig.chkPreviousBestmovesOnlyFirstMove"), "", cfg.previousBestmoveOnly, "match.onlyFirstMove");
  groupNote(body, t("EnginePkConfig.textAreaHint")).classList.add("match-note");
  // setTextEnable: Genmove模式 disables 随机落子.
  const textEnable = () => {
    const on = analysis.checked;
    for (const control of [randomOn, randomMoves, randomDiff, visitsOn, visitsPercent]) control.disabled = !on;
  };
  genmove.onchange = analysis.onchange = textEnable;
  textEnable();
  const candidates = () => (onlyFirst.disabled = !previous.checked);
  current.onchange = previous.onchange = candidates;
  candidates();

  const dialog = formPanel(t("EnginePkConfig.title"), body);
  const cancel = button(t("EnginePkConfig.cancelButton"), () => void dialog.close());
  cancel.dataset.key = "match.configCancel";
  const ok = primaryButton(t("EnginePkConfig.okButton"), () => void apply());
  ok.dataset.key = "match-config-ok";
  footer(body, { primary: ok, cancel });

  /** applyChange: each number field keeps its value when it does not parse. */
  async function apply(): Promise<void> {
    const now = state.view?.settings.matches;
    if (!now) return;
    const base = now.settings;
    const session = now.session;
    const r = await run(matchValues({
      settings: {
        showPreviousBestmoves: previous.checked,
        previousBestmoveOnly: onlyFirst.checked,
        checkRandomVisits: visitsOn.checked,
        randomVisitsPercent: doubleOr(visitsPercent, base.randomVisitsPercent),
      },
      session: {
        startNumber: intOr(startNumber, session.startNumber),
        startNumberEnabled: startOn.checked,
        autoSave: autoSave.checked,
        kind: genmove.checked ? "Genmove" : "Analysis",
        batchName: batchName.value,
        exchangeColors: exchange.checked,
        randomMovesEnabled: randomOn.checked,
        saveWinrate: saveWinrate.checked,
        randomMoves: parseJavaInt(randomMoves.value.trim()) ?? session.randomMoves,
        randomWinrateDiff: doubleOr(randomDiff, session.randomWinrateDiff),
        maxMovesEnabled: maxOn.checked,
        maxMoves: parseJavaInt(maxMoves.value.trim()) ?? session.maxMoves,
      },
    }));
    if (r && r.type !== "Refused") await dialog.close();
  }
  await dialog.closed;
}

/** A non-modal window over the workbench: the dialog frame with a close button in its title;
 *  Escape in the window is that button. */
function toolWindow(cls: string, title: string, closeKey: string, close: () => void): HTMLDivElement {
  const box = el("div", `modal-box match-window ${cls}`);
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", title);
  box.dataset.freeWindow = "";
  const head = el("div", "modal-title", title);
  const x = closeBox(close);
  x.dataset.key = closeKey;
  head.append(x);
  box.append(head);
  box.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || e.isComposing) return;
    e.preventDefault();
    e.stopPropagation();
    close();
  });
  return box;
}

/** The one 人工干预 window (Manual: non-modal, always on top). */
let manual: HTMLElement | null = null;

/** Manual.java: 立即落子 and 允许/关闭人工落子 (分析模式 only), 黑认输 and 白认输. */
export function openManual(): void {
  if (manual) {
    manual.hidden = false;
    return;
  }
  const box = toolWindow("match-manual", t("Mannul.title"), "match.manualClose", () => (box.hidden = true));
  const panel = el("div", "match-manual-actions");
  const push = (key: string, op: MatchIntervention, text = "") => {
    const b = button(text, () => void match({ type: "Intervention", op }));
    b.dataset.key = key;
    panel.append(b);
    return b;
  };
  const playNow = push("match.playNow", { type: "PlayNow" }, t("Mannul.playNow"));
  const manualOne = push("match.manualOne", { type: "ToggleManual" });
  const blackResign = push("match.blackResign", { type: "Resign", side: "Black" }, t("Mannul.blackResign"));
  const whiteResign = push("match.whiteResign", { type: "Resign", side: "White" }, t("Mannul.whiteResign"));
  box.append(panel);
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

/** Utils.showMsgNoModal: the Message window, which leaves the main window usable. It only
 *  informs: 关闭, the title's close box, Enter and Escape close it. */
export function messageNoModal(text: string): void {
  const previous = document.activeElement;
  const close = () => {
    const focused = box.contains(document.activeElement);
    box.remove();
    if (focused && previous instanceof HTMLElement && previous.isConnected) previous.focus();
  };
  const box = toolWindow("match-notice", t("Message.title"), "match.noticeClose", close);
  box.style.translate = `${(messages % 5) * 16}px ${(messages % 5) * 16}px`;
  messages++;
  const closeButton = button(product("close"), close);
  closeButton.dataset.key = "match.noticeOk";
  const row = el("div", "modal-buttons");
  row.append(closeButton);
  box.append(el("div", "modal-body", text), row);
  box.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" || e.isComposing || e.target instanceof HTMLButtonElement) return;
    e.preventDefault();
    e.stopPropagation();
    close();
  });
  document.body.append(box);
  closeButton.focus();
}
