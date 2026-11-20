// Java double-menu top toolbar (Menu.doubleMenu / TopHeaderPanel) with the groups 自定义内容
// chooses (chrome/topGroups.ts; Menu.java:8251-9931). Items this client implements run their
// catalog command; the others keep their Java look and report that the capability is not
// available yet.
import { run, state, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { rankMarkLit, setCandidateSide } from "../display/commands";
import { boardUi, onBoardUiChange } from "../display/state";
import { choosePlacement, placementLit } from "../editing/commands";
import { editing, markupToolsShown, onEditingChange } from "../editing/state";
import { product, t } from "../i18n";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import type { MenuNode } from "./javaMenus";
import { openPopupMenu } from "./menubar";
import { FLASH_MENU } from "../quick/flash";
import { functionSearchRefused, openFunctionSearch } from "../search/controller";
import { limitPopup } from "../analysis/region";
import { clockText } from "../game/commands";
import { matchPlaying } from "../match/state";
import { foxOwnsEngine, submitFoxPda } from "../fox/dialog";
import type { AnalysisSettings, KataParam } from "../contract/generated";
import { kataGoPda, kataRevealed, kataToolbarState, kataToolbarUse, kataValueInvalid } from "../engine/kataParams";
import { openCustomPda } from "../engine/customPda";
import "../editing/chrome.css";
import "../strength/commands";
import { topGroupShown, topToolbarWraps, type TopGroup } from "./topGroups";
import { toggleDoubleMenu } from "../workbench/panels";
import "../display/painting";
import { icon, type IconName } from "../design/icons";
import { stepper } from "../design/stepper";
import { buildAiCoachButton } from "../humansl/commands";
import { buildCommentaryButton } from "../commentary/commands";

const updaters: (() => void)[] = [];

/** A toggle button: while its choice is selected the host carries `on` and aria-pressed. */
function liveIcon(name: IconName, tip: string, selected: () => boolean, action: () => void, cls = ""): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-icon ${cls}`;
  b.title = tip;
  b.setAttribute("role", "button");
  b.append(icon(name));
  b.onclick = action;
  updaters.push(() => {
    const lit = selected();
    b.classList.toggle("on", lit);
    b.setAttribute("aria-pressed", String(lit));
  });
  return b;
}

/** A catalog toggle shown as a toggle button. */
function commandIcon(name: IconName, tip: string, command: string, cls = ""): HTMLElement {
  const entry = COMMANDS[command];
  const b = liveIcon(name, tip, () => entry.checked?.() ?? false, entry.run, cls);
  b.dataset.key = command;
  return b;
}

/** Marks the control function search locates for a Java top-toolbar action id
 *  (Menu.topFunctionTarget / AutoAnalyzeMenu.ACTION_PROPERTY). */
function at(id: string, control: HTMLElement): HTMLElement {
  control.dataset.function = id;
  return control;
}

/** btnMarkup and, while show-markup-tools is on, the markup buttons and 自由涂鸦 after it in Java's
 *  rightArea order (Menu.java:8694-8703; the circle button is created but never added). */
function markupTools(): HTMLElement[] {
  const tools = document.createElement("span");
  tools.className = "tb-markup";
  tools.append(
    at("toolbar.markup-letter", commandIcon("markup-letter", t("Accessibility.markupLetter"), "toolbar.markup-letter")),
    at("toolbar.markup-number", commandIcon("markup-number", t("Accessibility.markupNumber"), "toolbar.markup-number")),
    at("toolbar.markup-cross", commandIcon("markup-cross", t("Accessibility.markupCross"), "toolbar.markup-cross")),
    at("toolbar.markup-square", commandIcon("markup-square", t("Accessibility.markupSquare"), "toolbar.markup-square")),
    at("toolbar.markup-triangle", commandIcon("markup-triangle", t("Accessibility.markupTriangle"), "toolbar.markup-triangle")),
    at("toolbar.markup-eraser", commandIcon("eraser", t("Accessibility.markupEraser"), "toolbar.markup-eraser")),
    at("toolbar.markup-clear", iconButton("clear", t("Menu.btnMarkupClear.toolTipText"), "toolbar.markup-clear")),
    at("toolbar.painting", iconButton("paint-brush", t("Menu.drawPainting.toolTipText"), "toolbar.painting")),
  );
  updaters.push(() => {
    const display = markupToolsShown() ? "" : "none";
    if (tools.style.display !== display) tools.style.display = display;
  });
  return [at("toolbar.markup-tools", commandIcon("markup", t("Menu.btnMarkup.toolTipText"), "toolbar.markup-tools")), tools];
}

/** Edit bar (Menu.java:7552-7643): 添加黑子 / 添加白子 toggle back to alternate play when chosen
 *  again; 交替落子 toggles only its own icon. Each clears insert mode. */
function editBar(): HTMLElement[] {
  return [
    liveIcon("stone-black", t("Menu.playBlackToolTipText"), () => editing.placement === "Black",
      () => choosePlacement(editing.placement === "Black" ? "alternate" : "Black", false)),
    liveIcon("stone-white", t("Menu.playWhiteToolTipText"), () => editing.placement === "White",
      () => choosePlacement(editing.placement === "White" ? "alternate" : "White", false)),
    liveIcon("stone-alternate", t("Menu.playAlternatelyToolTipText"), alternateLit,
      () => choosePlacement("alternate", !alternateLit())),
    iconButton("pass", t("Menu.playPassToolTipText"), "menu.playPassMove", "tb-cut"),
  ];
}

function alternateLit(): boolean {
  return editing.placement === "alternate" && placementLit.alternate;
}

function refreshButtons(): void {
  updaters.forEach((u) => u());
}
onEditingChange(refreshButtons);
onBoardUiChange(refreshButtons);

/** rankMarkPopup (addRankMarkMenu with a popup, Menu.java:9354-9360): the count is typed into its
 *  item on every platform. */
const RANK_POPUP: MenuNode[] = [
  { t: "check", key: "Menu.rankMenu.rankNoneMove" },
  { t: "check", key: "Menu.rankMenu.rankLastMove" },
  { t: "check", key: "Menu.rankMenu.lblCustomMove" },
  { t: "check", key: "Menu.rankMenu.rankAllMove" },
  { t: "sep" },
  {
    t: "submenu",
    key: "Menu.rankMenu.evaluationMode",
    items: [
      { t: "radio", key: "Menu.rankMenu.evaluationMode.auto" },
      { t: "radio", key: "Menu.rankMenu.evaluationMode.score" },
      { t: "radio", key: "Menu.rankMenu.evaluationMode.winrate" },
      { t: "sep" },
      { t: "radio", key: "Menu.rankMenu.evaluationMode.combined" },
    ],
  },
];

/** btnRankMark: lit while move rank marks are shown (Menu.setBtnRankMark); opens rankMarkPopup below it. */
function rankMarkButton(): HTMLElement {
  const b = liveIcon("move-rank", t("Menu.btnRankMark.toolTipText"), rankMarkLit, () => openPopupMenu(b, RANK_POPUP));
  b.dataset.key = "toolbar.rank-mark";
  return b;
}

/** btnFlashAnalyze: opens FlashAnalyzeMenu below it (Menu.java:8406-8415). */
function flashButton(): HTMLElement {
  const b = iconButton("flash", t("Menu.btnAnalyze.btnFlashAnalyze"));
  b.classList.remove("unavailable");
  b.title = t("Menu.btnAnalyze.btnFlashAnalyze");
  b.dataset.key = "toolbar.flash";
  b.onclick = () => openPopupMenu(b, FLASH_MENU);
  return b;
}

/** selectAllowMore / selectAvoidMore: the region's limit popup below the button. */
function limitButton(allow: boolean, tip: string): HTMLElement {
  const b = iconButton("chevron-down", tip, undefined, "more");
  b.classList.remove("unavailable");
  b.title = tip;
  b.dataset.key = allow ? "toolbar.force-allow-options" : "toolbar.force-avoid-options";
  b.onclick = () => openPopupMenu(b, limitPopup(allow));
  return b;
}

function marked(el: HTMLElement, command: string | undefined, tip: string): HTMLElement {
  const entry = command ? COMMANDS[command] : undefined;
  if (entry) {
    el.title = tip;
    el.dataset.key = command;
    el.onclick = entry.run;
  } else {
    el.title = tip ? `${tip} — ${product("unavailable")}` : product("unavailable");
    el.classList.add("unavailable");
    el.onclick = unavailable;
  }
  return el;
}

function iconButton(name: IconName, tip: string, command?: string, cls = ""): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-icon ${cls}`;
  b.append(icon(name));
  return marked(b, command, tip);
}

function pill(text: string, cls = "", command?: string): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-pill ${cls}`;
  b.textContent = text;
  return marked(b, command, "");
}

/** Shows `items` while `shown` holds: a 自定义内容 choice, or function search revealed it. */
function showWhile(shown: () => boolean, ...items: HTMLElement[]): HTMLElement[] {
  updaters.push(() => {
    const display = shown() ? "" : "none";
    for (const item of items) if (item.style.display !== display) item.style.display = display;
  });
  return items;
}

/** A capsule: one raised surface around a group of related controls, which wraps as a whole.
 *  With `shown` it shows only while that holds. */
function capsule(shown: (() => boolean) | null, ...items: HTMLElement[]): HTMLElement {
  const cap = document.createElement("span");
  cap.className = "tb-cap";
  cap.append(...items);
  if (shown) showWhile(shown, cap);
  return cap;
}

/** One row of the toolbar; it takes no room while nothing in it shows. */
function row(...items: HTMLElement[]): HTMLElement {
  const r = document.createElement("div");
  r.className = "tb-row";
  r.append(...items);
  updaters.push(() => {
    const display = items.some((item) => !item.classList.contains("tb-grow") && item.style.display !== "none") ? "" : "none";
    if (r.style.display !== display) r.style.display = display;
  });
  return r;
}

/** The free space of a row: what follows it sits at the row's end. */
function spacer(): HTMLElement {
  const s = document.createElement("span");
  s.className = "tb-grow";
  return s;
}

/** A parameter (komiPanel / pdaFieldPanel / wrnFieldPanel) or a captioned set of choices inside a
 *  capsule. */
function group(cls: string, ...items: HTMLElement[]): HTMLElement {
  const g = document.createElement("span");
  g.className = `tb-group ${cls}`;
  g.append(...items);
  return g;
}

/** A caption inside a capsule. Its colour sets it apart from the values, so the colon some
 *  languages' strings end with is left out. */
function label(text: string): HTMLElement {
  const s = document.createElement("span");
  s.className = "tb-label";
  s.textContent = text.replace(/[:：]\s*$/, "");
  return s;
}

/** 选点 黑 / 白 (chkShowBlack / chkShowWhite, Menu.java:9714-9753): one colour's candidates. */
function candidateCheck(text: string, black: boolean): HTMLElement {
  const c = document.createElement("input");
  c.type = "checkbox";
  c.className = "tb-check";
  c.tabIndex = -1;
  c.dataset.key = black ? "toolbar.candidates-black" : "toolbar.candidates-white";
  c.onchange = () => setCandidateSide(black, c.checked);
  updaters.push(() => (c.checked = black ? boardUi.showBlack : boardUi.showWhite));
  const l = document.createElement("label");
  l.className = "tb-checklabel";
  l.append(c, document.createTextNode(text));
  return l;
}

/** A check box with its text after it (JFontCheckBox). */
function checkLabel(text: string, key: string): { box: HTMLInputElement; label: HTMLElement } {
  const box = document.createElement("input");
  box.type = "checkbox";
  box.className = "tb-check";
  box.tabIndex = -1;
  box.dataset.key = key;
  const label = document.createElement("label");
  label.className = "tb-checklabel";
  label.append(box, document.createTextNode(text));
  return { box, label };
}

function shownGroup(group: TopGroup): () => boolean {
  return () => topGroupShown(group);
}

function saveAnalysis(change: Partial<AnalysisSettings>): void {
  const settings = state.view?.settings.engine.analysis;
  if (settings) void run({ type: "SetAnalysisSettings", settings: { ...settings, ...change } });
}

/** IntDocument (IntDocument.java:25-50): an edit is taken only when the whole text is still an
 *  int; deleting is always taken. Negative numbers have no meaning as a limit here. */
function intText(text: string): boolean {
  return text === "" || (/^\d+$/.test(text) && Number(text) <= 2147483647);
}

/** 时间 chkTime + txtTimeLimit and 计算 chkPlayOut + txtPlayOutsLimit (Menu.java:6502-6587,
 *  7124-7137): the check writes limit-time / limit-playout and enables the field; every edit of
 *  the field writes its number (an empty field keeps the value, Utils.parseTextToLong). Each
 *  change lets an analysis a limit stopped go on when it is within the new limits. Both are
 *  separate toolbar items, so the field may wrap away from its check box as in Java. The arrows
 *  step a second or a hundred visits. */
function limitControl(time: boolean): HTMLElement[] {
  const { box, label } = checkLabel(t(time ? "Menu.chkTime" : "Menu.chkPlayOut"), time ? "toolbar.limit-time" : "toolbar.limit-visits");
  label.classList.add("fit");
  const field = document.createElement("input");
  field.className = `tb-input ${time ? "time-limit" : "visit-limit"}`;
  field.inputMode = "numeric";
  field.min = "0";
  field.step = time ? "1" : "100";
  field.dataset.key = time ? "toolbar.limit-time-value" : "toolbar.limit-visits-value";
  field.setAttribute("aria-label", t(time ? "Menu.chkTime" : "Menu.chkPlayOut"));
  let taken = "";
  box.onchange = () => {
    field.disabled = !box.checked;
    saveAnalysis(time ? { limitTime: box.checked } : { limitPlayout: box.checked });
  };
  field.addEventListener("input", () => {
    if (!intText(field.value)) {
      field.value = taken;
      return;
    }
    taken = field.value;
    if (taken === "") return;
    saveAnalysis(time ? { maxAnalyzeTimeSeconds: Number(taken) } : { limitPlayouts: Number(taken) });
  });
  updaters.push(() => {
    const s = state.view?.settings.engine.analysis;
    if (!s) return;
    box.checked = time ? s.limitTime : s.limitPlayout;
    field.disabled = !box.checked;
    if (document.activeElement === field) return;
    const value = String(time ? s.maxAnalyzeTimeSeconds : s.limitPlayouts);
    if (field.value !== value) field.value = taken = value;
  });
  at(time ? "toolbar.time-limit" : "toolbar.visit-limit", box);
  return [label, stepper(field)];
}

/** 限制 (Menu.lblLimit, Menu.java:9698-9712): shown while either limit is. */
function limitCapsule(): HTMLElement {
  const time = shownGroup("showTimeLimit");
  const visits = shownGroup("showVisitLimit");
  return capsule(
    () => time() || visits(),
    group("choices", label(t("Menu.lblLimit")), ...showWhile(time, ...limitControl(true)), ...showWhile(visits, ...limitControl(false))),
  );
}

/** 分析 黑 / 白 (chkAnalyzeBlack / chkAnalyzeWhite, Menu.java:9754-9793, 9864-9872): the session's
 *  choice of the sides analysis goes on for after moves and browsing. */
function analyzeCheck(black: boolean): HTMLElement {
  const { box, label } = checkLabel(t(black ? "Menu.Black" : "Menu.White"), black ? "toolbar.analyze-black" : "toolbar.analyze-white");
  box.onchange = () => {
    const e = state.view?.engine;
    if (!e) return;
    void run({ type: "SetAnalyzeSides", black: black ? box.checked : e.analyze_black, white: black ? e.analyze_white : box.checked });
  };
  updaters.push(() => {
    const e = state.view?.engine;
    if (e) box.checked = black ? e.analyze_black : e.analyze_white;
  });
  if (black) at("toolbar.analyze-sides", box);
  return label;
}

/** 选点 胜率 / 计算量 / 目数 (chkShowWinrate / chkShowPlayouts / chkShowScore, Menu.java:9804-9899):
 *  the same choices as 显示 > 推荐点's items, redrawn at once. */
function candidateInfoCheck(text: string, command: string, key: string): HTMLElement {
  const entry = COMMANDS[command];
  const { box, label } = checkLabel(text, key);
  label.classList.add("fit");
  box.onchange = () => entry.run();
  updaters.push(() => (box.checked = entry.checked?.() ?? false));
  return label;
}

/** The PDA statuses on which FoxLiveSession republishes the confirmed value (publishConfirmedPda):
 *  a dynamic / unverified refusal and a failed update. */
const PDA_REPUBLISH: Record<string, true> = { "Fox.pdaDynamic": true, "Fox.pdaUnknown": true, "Fox.pdaFailed": true };

/** Menu.setPdaAndWrnByEngineForDouble (Menu.java:9970-10030): a group shows while the foreground
 *  engine is KataGo, no engine game is starting or playing, and its show-*-in-menu is on (or
 *  function search revealed it for this run). A KataGoPda engine has its own PDA panel instead.
 *  While 野狐 AI 直连 holds the engine its 激进度 stays shown, as it was before these settings. */
function kataGroupShown(param: KataParam): boolean {
  const v = state.view;
  if (param === "Pda" && foxOwnsEngine()) return true;
  if (!v?.engine.foreground?.katago || v.engine_match?.phase === "Starting" || matchPlaying(v)) return false;
  if (param === "Pda" && kataGoPda()) return false;
  const saved = v.settings.engine.kata_params;
  return kataRevealed[param] || (param === "Pda" ? saved.showPda : saved.showWrn);
}

/** 激进度 chkPDA / txtGfPDA and 广度 chkWRN / txtWRN (Menu.java:6174-6447). The check box or each
 *  edit of the value sends the value (0 when unchecked or not a number) to the foreground KataGo at
 *  once; a value outside -3..3 / 0..2 only turns the field red. The arrows, ↑ / ↓ and the wheel
 *  step 激进度 by 0.1 and 广度 by 0.01 within those ranges. The group shows the values the engine reports when
 *  it starts (Menu.setPdaAndWrn), not the answers to its own sets, and is disabled while the
 *  engine is not free for it (refreshBreadthAdmission).
 *
 *  While 野狐 AI 直连 holds the engine every PDA check and text edit is the session's PDA intent
 *  (FoxLiveSession.submitPda) and the PDA controls show its confirmed value whenever that changes,
 *  and again after an intent is refused or an update fails (setUseGfPda, Menu.java:11124-11131). */
function kataGroup(param: KataParam): HTMLElement {
  const pda = param === "Pda";
  const tip = pda ? t("Menu.chkPDA.toolTopText") : t("Menu.chkWRN.toolTopText");
  const box = document.createElement("input");
  box.type = "checkbox";
  box.className = "tb-check";
  box.tabIndex = -1;
  box.title = tip;
  const value = document.createElement("input");
  value.className = "tb-input";
  value.inputMode = "decimal";
  value.step = pda ? "0.1" : "0.01";
  value.min = pda ? "-3" : "0";
  value.max = pda ? "3" : "2";
  value.title = tip;
  box.dataset.key = pda ? "toolbar.pda" : "toolbar.wrn";
  value.dataset.key = pda ? "toolbar.pda-value" : "toolbar.wrn-value";
  const foxLive = () => pda && foxOwnsEngine();
  // The foreground engine's read-back last shown; `undefined` shows the next one in any case.
  let shownReadBack: string | null | undefined = undefined;
  let wasFox = false;
  const use = (enabled: boolean, text: string) => {
    box.checked = enabled;
    value.value = text;
    value.disabled = box.disabled || !enabled;
    value.classList.toggle("invalid", kataValueInvalid(param, text));
  };
  kataToolbarUse[param] = use;
  kataToolbarState[param] = () => ({ enabled: box.checked, text: value.value });
  const send = (enabled: boolean) => {
    const v = enabled ? (parseJavaDouble(value.value) ?? 0) : 0;
    void run({ type: "SetKataParam", param, enabled, value: v });
  };

  let foxShown: number | null = null;
  let foxStatus = "";
  let foxEdits = 0;
  // Assigning the controls fires neither click nor input, so showing sends no new intent.
  const foxShow = (confirmed: number | null) => {
    foxShown = confirmed;
    box.checked = confirmed !== null && confirmed !== 0;
    value.value = confirmed === null ? "" : javaDouble(confirmed);
    value.disabled = confirmed !== null && confirmed === 0;
  };
  // A refusal leaves the status as it was when the same draft is refused again, so the view
  // after each intent decides; only the latest edit may reset the controls it was typed into.
  const foxSubmit = (draft: string) => {
    const edit = ++foxEdits;
    void submitFoxPda(draft).then((fox) => {
      if (fox && edit === foxEdits && foxOwnsEngine() && PDA_REPUBLISH[fox.pda_status_key] === true) foxShow(fox.confirmed_pda);
    });
  };

  box.onclick = () => {
    value.disabled = !box.checked;
    if (foxLive()) {
      foxSubmit(box.checked ? value.value : "0");
      return;
    }
    if (!box.checked && value.classList.contains("invalid")) {
      value.value = "";
      value.classList.remove("invalid");
    }
    send(box.checked);
  };
  value.addEventListener("input", () => {
    if (foxLive()) {
      foxSubmit(value.value);
      return;
    }
    value.classList.toggle("invalid", kataValueInvalid(param, value.value));
    send(box.checked);
  });
  const name = document.createElement("label");
  name.className = "tb-checklabel";
  name.title = tip;
  name.append(at(pda ? "engine.pda" : "toolbar.wrn", box), document.createTextNode(pda ? t("Menu.separateLblPda") : t("Menu.separateLblWrn")));
  const g = group(pda ? "pda" : "wrn", name, stepper(value, { format: javaDouble }));
  updaters.push(() => {
    const display = kataGroupShown(param) ? "" : "none";
    if (g.style.display !== display) g.style.display = display;
    if (foxLive()) {
      box.disabled = false;
      const confirmed = state.view?.fox.confirmed_pda ?? null;
      const status = state.view?.fox.pda_status_key ?? "";
      const republish = status !== foxStatus && PDA_REPUBLISH[status] === true;
      foxStatus = status;
      if (!wasFox || confirmed !== foxShown || republish) foxShow(confirmed);
      wasFox = true;
      return;
    }
    if (wasFox) {
      // The session gave the engine back: show the engine's own value again.
      wasFox = false;
      foxStatus = "";
      shownReadBack = undefined;
    }
    const params = state.view?.engine.kata_params;
    box.disabled = ((pda ? params?.pda_unavailable : params?.wrn_unavailable) ?? null) !== null;
    value.disabled = box.disabled || !box.checked;
    const readBack = (pda ? params?.pda_read_back : params?.wrn_read_back) ?? null;
    if (readBack === shownReadBack || document.activeElement === value) return;
    shownReadBack = readBack;
    const reported = (pda ? params?.pda : params?.wrn) ?? null;
    if (reported !== null) use(reported !== 0, reported === 0 ? "0" : javaDouble(reported));
  });
  return g;
}

/** more2 "..." (Menu.java:6101-6116) opens KataGo 激进参数PDA设置. Java shows it only in a
 *  KataGoPda engine's own PDA panel; here it follows every foreground KataGo outside engine games
 *  (approved: the window can be opened and saved with any KataGo). */
function pdaIdleShown(): boolean {
  const v = state.view;
  return !!v?.engine.foreground?.katago && v.engine_match?.phase !== "Starting" && !matchPlaying(v);
}

/** The "..." also stays during an engine game, playing or paused, that has a KataGoPda
 *  participant, whichever engine is in front (Menu.java:7240-7276, 9970-9986
 *  `participantHasKataGoPda`); the dialog then sets that participant. */
function pdaMoreShown(): boolean {
  const v = state.view;
  return pdaIdleShown() || (matchPlaying(v) && !!v?.engine_match?.kata_go_pda_participant);
}

/** A KataGoPda engine's own PDA panel, lblCustomPda and txtPDA (Menu.java:5943-5965, 9658-9661,
 *  9970-9986): the label without a colon and the engine's PDA as it reports it ("%.3f"), shown
 *  until the field is being edited. Each edit that leaves a number sets it on the engine
 *  (`kata-set-param playoutDoublingAdvantage`); empty, "-" or "." send nothing. The arrows step it
 *  by 0.1. While 野狐 AI 直连 holds the engine its own 激进度 group shows instead. */
function customPdaGroup(): HTMLElement {
  const value = document.createElement("input");
  value.className = "tb-input custom-pda";
  value.inputMode = "decimal";
  value.step = "0.1";
  value.dataset.key = "toolbar.customPda";
  value.setAttribute("aria-label", t("Menu.chkPDA.toolTopText"));
  value.addEventListener("input", () => {
    const pda = parseJavaDouble(value.value);
    if (pda !== null) void run({ type: "SetKataParam", param: "Pda", enabled: true, value: pda });
  });
  const g = group("custom-pda", label(t("Menu.separateLblPda")), stepper(value, { format: (pda) => pda.toFixed(3) }));
  let shown: number | null | undefined = undefined;
  updaters.push(() => {
    const display = kataGoPda() && !foxOwnsEngine() && pdaIdleShown() ? "" : "none";
    if (g.style.display !== display) g.style.display = display;
    const pda = state.view?.engine.custom_pda.pda ?? null;
    if (pda === shown || document.activeElement === value) return;
    shown = pda;
    value.value = pda === null ? "" : pda.toFixed(3);
  });
  return g;
}

function pdaMoreButton(): HTMLElement {
  const b = document.createElement("span");
  b.className = "tb-icon pda-more";
  b.append(icon("more"));
  b.title = t("Accessibility.pdaOptions");
  b.setAttribute("role", "button");
  b.setAttribute("aria-label", t("Accessibility.pdaOptions"));
  b.dataset.key = "toolbar.pdaMore";
  // Function search's engine.pda with a KataGoPda engine (search/targets.ts toolbarControl).
  at("engine.pda", b);
  b.onclick = () => void openCustomPda();
  updaters.push(() => {
    const display = pdaMoreShown() ? "" : "none";
    if (b.style.display !== display) b.style.display = display;
  });
  return b;
}

/** 贴目: typed in, or stepped by 0.5 with the arrows, ↑ / ↓ and the wheel; Enter or leaving the
 *  field sets it. */
function komiField(): HTMLElement {
  const control = document.createElement("input"); control.id = "komiField"; control.className = "tb-input";
  control.type = "number"; control.step = "0.5"; control.setAttribute("aria-label", t("Menu.komi"));
  control.onchange = () => { if (control.value.trim() && Number.isFinite(Number(control.value))) void run({ type: "SetKomi", komi: Number(control.value) }); };
  control.onkeydown = (event) => { if (event.key === "Enter") control.blur(); };
  return stepper(control, { titles: [t("Accessibility.increaseKomi"), t("Accessibility.decreaseKomi")] });
}

/** The search features button (Menu.java:8660-8685): the search icon and the title, which
 *  collapses to the icon alone below a 900 px wide window. Not focusable, so the control
 *  that had the focus gets it back when the search is cancelled. */
function searchButton(): HTMLElement {
  const title = t("FunctionSearch.title");
  const b = document.createElement("span");
  b.className = "tb-search";
  b.dataset.key = "function.search";
  b.title = title;
  b.setAttribute("role", "button");
  b.setAttribute("aria-label", title);
  const text = document.createElement("span");
  text.className = "label";
  b.append(icon("search"), text);
  const fit = () => {
    const narrow = window.innerWidth < 900;
    text.textContent = narrow ? "" : title;
    b.classList.toggle("collapsed", narrow);
  };
  fit();
  window.addEventListener("resize", fit);
  // Decided on the press: a text control keeps the focus (preventDefault) and the press has not
  // yet closed an open menu, so both still refuse the search as the shortcut does.
  let admitted = false;
  b.addEventListener("mousedown", (e) => {
    e.preventDefault();
    admitted = !functionSearchRefused();
  });
  b.onclick = () => {
    if (admitted) void openFunctionSearch();
    admitted = false;
  };
  return b;
}

/** newGamePopup (Menu.java:9109-9135): Genmove, 分析模式 and 引擎对局. */
const NEW_GAME_POPUP: MenuNode[] = [
  { t: "item", key: "Menu.newGenmoveGame", id: "menu.newGenmoveGame" },
  { t: "item", key: "Menu.newAnalyzeModeGame", id: "menu.newAnalyzeModeGame" },
  { t: "item", key: "Menu.newEngineGame", id: "menu.newEngineGame" },
];

function gamePill(text: string, cls: string, key: string, run: () => void): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-pill ${cls}`;
  b.textContent = text;
  b.dataset.key = key;
  b.setAttribute("role", "button");
  b.onclick = run;
  return b;
}

/** The double menu's game controls (toggleDoubleMenuGameStatus, Menu.java:10321-10362): 新对局
 *  becomes the red 结束对局 while a 人机对弈 game or an 引擎对战 batch runs, 暂停 / 继续 is enabled
 *  only then and 认输 is shown only in 人机对弈. */
function gameControls(): HTMLElement[] {
  const newGame = gamePill(t("Menu.newGameBtn"), "newgame", "game-new", () => openPopupMenu(newGame, NEW_GAME_POPUP));
  const stop = at("toolbar.game-stop", gamePill(t("Menu.endGameBtn"), "newgame danger", "game-end", () => COMMANDS["toolbar.game-stop"].run()));
  const pause = at("toolbar.game-pause", gamePill(t("Menu.pauseGameBtn"), "pause", "game-pause", () => { if (state.view?.game || state.view?.engine_match) COMMANDS["toolbar.game-pause"].run(); }));
  const resign = at("toolbar.game-resign", gamePill(t("Menu.resignBtn"), "pause danger", "game-resign", () => COMMANDS["toolbar.game-resign"].run()));
  updaters.push(() => {
    const game = state.view?.game;
    // EngineGameSnapshot.playing(): between games of a batch the 新对局 button is back.
    const match = matchPlaying() ? state.view?.engine_match : null;
    newGame.style.display = game || match ? "none" : "";
    stop.style.display = game || match ? "" : "none";
    resign.style.display = game ? "" : "none";
    pause.classList.toggle("disabled", !game && !match);
    const paused = match ? match.phase === "Paused" : game?.phase === "Paused";
    pause.textContent = paused ? t("Menu.continueGameBtn") : t("Menu.pauseGameBtn");
  });
  return [newGame, stop, pause, resign];
}

/** byoyomiTime (Menu.java:7507-7512, bold 15): the human's clock while the win-rate graph is
 *  hidden; with the graph shown the clock is drawn in the statistics strip (drawContDownForHuman). */
function clockLabel(): HTMLElement {
  const label = document.createElement("span");
  label.className = "tb-label tb-clock";
  label.dataset.key = "game.clock";
  updaters.push(() => {
    const v = state.view;
    const text = v?.game?.clock && !v.settings.panels.showWinrateGraph ? `  ${clockText(v.game.clock)}` : "";
    if (label.textContent !== text) label.textContent = text;
    label.style.display = text ? "" : "none";
  });
  return label;
}

/** btnFoxKifu (Menu.java:8353-8367): primary style, 野狐棋谱 as its tip. */
function foxKifuButton(): HTMLElement {
  const b = pill(t("Menu.foxKifuButton"), "", "menu.foxKifu");
  b.title = t("Menu.foxKifu");
  return b;
}

/** btnTencentKifu (Menu.java:8369-8384): primary style, 腾讯棋谱 as its tip. */
function tencentKifuButton(): HTMLElement {
  const b = pill(t("Menu.tencentKifuButton"), "", "menu.tencentKifu");
  b.title = t("Menu.tencentKifu");
  return b;
}

/** btnDoubleMenu (Menu.java:7534-7550, 8259): first in the row while its 自定义内容 choice is on.
 *  The row shows only 分开, so the button shows the up chevron, which turns the row to 合并. */
function doubleMenuButton(): HTMLElement {
  const b = document.createElement("span");
  b.className = "tb-icon";
  b.dataset.key = "toolbar.double-menu";
  b.title = t("Accessibility.toolbarDetailsDescription");
  b.append(icon("chevron-up"));
  b.onclick = toggleDoubleMenu;
  return b;
}

/** 选点 (lblShowCandidate, Menu.java:9874-9899): shown with either 黑 / 白 or 胜率 / 计算量 / 目数,
 *  the two parts divided while both show. */
function candidateCapsule(): HTMLElement {
  const sides = shownGroup("showCandidateSides");
  const info = shownGroup("showCandidateInfo");
  return capsule(
    () => sides() || info(),
    group(
      "choices",
      label(t("Menu.lblShowCandidate")),
      ...showWhile(sides, at("toolbar.candidate-sides", candidateCheck(t("Menu.Black"), true)), candidateCheck(t("Menu.White"), false)),
    ),
    ...showWhile(
      info,
      group(
        "choices",
        at("toolbar.candidate-info", candidateInfoCheck(t("Menu.winrate"), "menu.suggestion1", "toolbar.candidate-winrate")),
        candidateInfoCheck(t("Menu.visits"), "menu.suggestion2", "toolbar.candidate-visits"),
        candidateInfoCheck(t("Menu.score"), "menu.suggestion3", "toolbar.candidate-score"),
      ),
    ),
  );
}

/** The double menu's toolbar (Menu.doubleMenu / setKomiPanelInDoubleMenu, Menu.java:8251-9931) as
 *  two rows of capsules, one capsule per 自定义内容 group: the icon groups and the search field in
 *  the upper row; the game controls, AI 陪练 / AI 解说, the engine parameters and the text
 *  buttons in the lower one. 自动换行 off keeps one row (LizzieFrame.java:14503-14507). */
export function buildToolbar(host: HTMLElement): void {
  const setup = pill(t("Menu.autoSetupButton"), "", "engine.setup");
  setup.dataset.key = "engine.setup"; setup.title = t("Menu.autoSetup"); setup.setAttribute("role", "button"); setup.tabIndex = 0;
  setup.onkeydown = (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); COMMANDS["engine.setup"].run(); } };
  const basic = shownGroup("showBasicButtons");
  const rules = shownGroup("showRules");
  const parameters = shownGroup("showParameters");
  const boardSize = shownGroup("showBoardSize");
  const saveLoad = shownGroup("showSaveLoad");
  const tsumego = () => state.view?.settings.tsumego.showButton !== false;
  host.append(
    row(
      capsule(shownGroup("showDoubleMenuButton"), doubleMenuButton()),
      capsule(
        basic,
        iconButton("new-file", t("Menu.btnNewFile.toolTipText"), "files.new"),
        iconButton("open", t("Menu.btnOpen.toolTipText"), "files.open"),
        iconButton("save", t("Menu.btnSave.toolTipText"), "files.save"),
      ),
      capsule(
        basic,
        flashButton(),
        iconButton("hawkeye", t("Menu.btnHawkeye.toolTipText"), "analysis.bad-moves"),
        rankMarkButton(),
        at("toolbar.change-turn", iconButton("swap-turn", t("Menu.btnChangeTurn.toolTipText"), "toolbar.change-turn")),
        iconButton("set-main", t("Menu.btnSetMain.toolTipText"), "menu.setAsMain"),
        iconButton("back-to-main", t("Menu.btnBackMain.toolTipText"), "menu.backToMainBranch"),
        iconButton("control-panel", t("Menu.btnContrlPanel.toolTipText"), "menu.ctrlPanel"),
        ...markupTools(),
      ),
      capsule(shownGroup("showEditBar"), ...editBar()),
      capsule(
        shownGroup("showForce"),
        at("toolbar.force-allow", commandIcon("region-allow", t("Menu.selectAllowToolTipText"), "toolbar.force-allow")),
        at("toolbar.force-allow-options", limitButton(true, t("Accessibility.allowMoveOptions"))),
        at("toolbar.force-avoid", commandIcon("region-avoid", t("Menu.selectAvoidToolTipText"), "toolbar.force-avoid")),
        at("toolbar.force-avoid-options", limitButton(false, t("Accessibility.avoidMoveOptions"))),
        at("toolbar.force-clear", iconButton("clear", t("Menu.clearSelectToolTipText"), "toolbar.force-clear")),
      ),
      spacer(),
      ...showWhile(basic, searchButton()),
    ),
    row(
      capsule(shownGroup("showGameControls"), ...gameControls()),
      capsule(null, buildAiCoachButton(), buildCommentaryButton()),
      capsule(null, group("komi", label(t("Menu.komi")), komiField()), kataGroup("Pda"), customPdaGroup(), pdaMoreButton(), kataGroup("Wrn")),
      limitCapsule(),
      capsule(shownGroup("showAnalyzeSides"), group("choices", label(t("Menu.lblShowAnalyze")), analyzeCheck(true), analyzeCheck(false))),
      candidateCapsule(),
      capsule(basic, foxKifuButton(), tencentKifuButton(), strengthButton(), setup),
      spacer(),
      capsule(
        () => rules() || parameters() || boardSize() || saveLoad() || tsumego(),
        ...showWhile(rules, pill(t("Menu.rulesBtn"), "", "engine.rules")),
        tsumegoButton(),
        ...showWhile(parameters, pill(t("Menu.paramsBtn"), "", "engine.parameters")),
        // setBoardSize opens 设置棋盘大小 (Menu.java:6145-6153).
        ...showWhile(boardSize, pill(t("Menu.gobanBtn"), "", "menu.setBoard")),
        ...showWhile(saveLoad, pill(t("Menu.saveBtn"), "", "menu.saveAndLoad")),
      ),
      clockLabel(),
    ),
  );
  updaters.push(() => host.classList.toggle("nowrap", !topToolbarWraps()));
}

/** btnPlayerStrengthEstimate (Menu.java:8386-8404): 棋力评估 with the menu item's text as its tip. */
function strengthButton(): HTMLElement {
  const b = pill(t("Menu.playerStrengthEstimateButton"), "", "menu.playerStrengthEstimate");
  b.title = t("Menu.playerStrengthEstimate");
  return b;
}

/** Menu.tsumeGoBtn: 死活题, shown while 自定义工具栏 > 死活 is on (show-tsume-go-menu). */
function tsumegoButton(): HTMLElement {
  const b = pill(t("Menu.tsumeGoBtn"), "", "game.tsumego");
  b.dataset.key = "toolbar.tsumego";
  updaters.push(() => {
    const display = state.view?.settings.tsumego.showButton === false ? "none" : "";
    if (b.style.display !== display) b.style.display = display;
  });
  return b;
}

/** The komi field shows the current game komi (txtKomi: String.valueOf(double), so 6 is "6.0");
 *  the edit and markup buttons show the current selection. */
export function renderToolbar(): void {
  const komi = document.getElementById("komiField") as HTMLInputElement | null;
  const k = state.view?.komi;
  const value = k === undefined ? "" : javaDouble(k);
  if (komi && document.activeElement !== komi && komi.value !== value) komi.value = value;
  refreshButtons();
}
