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

const updaters: (() => void)[] = [];

/** A button whose icon shows a selected state (JButton.setIcon with the "2" image). */
function liveIcon(off: string, on: string, tip: string, selected: () => boolean, action: () => void, cls = ""): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-icon ${cls}`;
  b.title = tip;
  const image = document.createElement("img");
  image.alt = "";
  b.append(image);
  b.onclick = action;
  updaters.push(() => {
    const lit = selected();
    const src = `/assets/${lit ? on : off}`;
    if (image.getAttribute("src") !== src) image.src = src;
    b.classList.toggle("on", lit);
  });
  return b;
}

/** A catalog toggle shown as a two-image button. */
function commandIcon(off: string, on: string, tip: string, command: string, cls = ""): HTMLElement {
  const entry = COMMANDS[command];
  const b = liveIcon(off, on, tip, () => entry.checked?.() ?? false, entry.run, cls);
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
    at("toolbar.markup-letter", commandIcon("label.png", "label2.png", t("Accessibility.markupLetter"), "toolbar.markup-letter")),
    at("toolbar.markup-number", commandIcon("labelnum1.png", "labelnum2.png", t("Accessibility.markupNumber"), "toolbar.markup-number")),
    at("toolbar.markup-cross", commandIcon("x.png", "x2.png", t("Accessibility.markupCross"), "toolbar.markup-cross")),
    at("toolbar.markup-square", commandIcon("square.png", "square2.png", t("Accessibility.markupSquare"), "toolbar.markup-square")),
    at("toolbar.markup-triangle", commandIcon("sanjiao.png", "sanjiao2.png", t("Accessibility.markupTriangle"), "toolbar.markup-triangle")),
    at("toolbar.markup-eraser", commandIcon("eraser.png", "eraser2.png", t("Accessibility.markupEraser"), "toolbar.markup-eraser")),
    at("toolbar.markup-clear", icon("clear.png", t("Menu.btnMarkupClear.toolTipText"), "toolbar.markup-clear")),
    at("toolbar.painting", icon("paint.png", t("Menu.drawPainting.toolTipText"), "toolbar.painting")),
  );
  updaters.push(() => {
    const display = markupToolsShown() ? "" : "none";
    if (tools.style.display !== display) tools.style.display = display;
  });
  return [at("toolbar.markup-tools", commandIcon("mark1.png", "mark2.png", t("Menu.btnMarkup.toolTipText"), "toolbar.markup-tools")), tools];
}

/** Edit bar (Menu.java:7552-7643): 添加黑子 / 添加白子 toggle back to alternate play when chosen
 *  again; 交替落子 toggles only its own icon. Each clears insert mode. */
function editBar(): HTMLElement[] {
  return [
    liveIcon("smallblack1.png", "smallblack2.png", t("Menu.playBlackToolTipText"), () => editing.placement === "Black",
      () => choosePlacement(editing.placement === "Black" ? "alternate" : "Black", false), "stone"),
    liveIcon("smallwhite.png", "smallwhite4.png", t("Menu.playWhiteToolTipText"), () => editing.placement === "White",
      () => choosePlacement(editing.placement === "White" ? "alternate" : "White", false), "stone"),
    // iconbh = hb.png at menuIconSize + 8 in a 22 px button (Menu.java:3674-3680, 7602-7626)
    liveIcon("hb.png", "hb2.png", t("Menu.playAlternatelyToolTipText"), alternateLit,
      () => choosePlacement("alternate", !alternateLit()), "bw"),
    icon("playpass.png", t("Menu.playPassToolTipText"), "menu.playPassMove", "w22"),
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

/** btnRankMark: RankMarkOn / RankMarkOff by Menu.setBtnRankMark; opens rankMarkPopup below it. */
function rankMarkButton(): HTMLElement {
  const b = liveIcon("RankMarkOff.png", "RankMarkOn.png", t("Menu.btnRankMark.toolTipText"), rankMarkLit, () => openPopupMenu(b, RANK_POPUP));
  b.dataset.key = "toolbar.rank-mark";
  return b;
}

/** btnFlashAnalyze: opens FlashAnalyzeMenu below it (Menu.java:8406-8415). */
function flashButton(): HTMLElement {
  const b = icon("flash.png", t("Menu.btnAnalyze.btnFlashAnalyze"));
  b.classList.remove("unavailable");
  b.title = t("Menu.btnAnalyze.btnFlashAnalyze");
  b.dataset.key = "toolbar.flash";
  b.onclick = () => openPopupMenu(b, FLASH_MENU);
  return b;
}

/** selectAllowMore / selectAvoidMore: the region's limit popup below the button. */
function limitButton(allow: boolean, tip: string): HTMLElement {
  const b = icon("horizonDown.png", tip, undefined, "more");
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

function icon(img: string, tip: string, command?: string, cls = ""): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-icon ${cls}`;
  const image = document.createElement("img");
  image.src = `/assets/${img}`;
  image.alt = "";
  b.append(image);
  return marked(b, command, tip);
}

function pill(text: string, cls = "", command?: string): HTMLElement {
  const b = document.createElement("span");
  b.className = `tb-pill ${cls}`;
  b.textContent = text;
  return marked(b, command, "");
}

/** TopHeaderPanel.ToolbarSeparator (10x24 leading, 8x22 between groups) or "dash" for the
 *  JToolBar.Separator after the komi panel (TopHeaderPanel.java:47-55, Menu.java:9654). */
function sep(kind = ""): HTMLElement {
  const s = document.createElement("span");
  s.className = `tb-sep ${kind}`;
  return s;
}

/** A composite toolbar item (komiPanel / pdaFieldPanel / wrnFieldPanel): wraps as a whole. */
function group(cls: string, ...items: HTMLElement[]): HTMLElement {
  const g = document.createElement("span");
  g.className = `tb-group ${cls}`;
  g.append(...items);
  return g;
}

/** TopHeaderPanel.GroupPanel: FlowLayout(LEFT, 2, 0) holding a run of buttons. */
function buttons(...items: HTMLElement[]): HTMLElement {
  const g = document.createElement("span");
  g.className = "tb-buttons";
  g.append(...items);
  return g;
}

function label(text: string): HTMLElement {
  const s = document.createElement("span");
  s.className = "tb-label";
  s.textContent = text;
  return s;
}

/** Menu.withTrailingColon (Menu.java:8179-8187). */
function withColon(text: string): string {
  return text.endsWith(":") || text.endsWith("：") ? text : `${text}:`;
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

/** A run of top-toolbar items shown while its 自定义内容 choice is on (or function search revealed
 *  it); the items wrap one by one like Java's. */
function optional(shown: () => boolean, ...items: HTMLElement[]): HTMLElement {
  const run = document.createElement("span");
  run.className = "tb-run";
  run.append(...items);
  updaters.push(() => {
    const display = shown() ? "" : "none";
    if (run.style.display !== display) run.style.display = display;
  });
  return run;
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
 *  separate toolbar items, so the field may wrap away from its check box as in Java. */
function limitControl(time: boolean): HTMLElement[] {
  const { box, label } = checkLabel(t(time ? "Menu.chkTime" : "Menu.chkPlayOut"), time ? "toolbar.limit-time" : "toolbar.limit-visits");
  label.classList.add("fit");
  const field = document.createElement("input");
  field.className = `tb-input ${time ? "time-limit" : "visit-limit"}`;
  field.inputMode = "numeric";
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
  return [label, field];
}

/** 限制: (Menu.lblLimit, Menu.java:9698-9712): shown while either limit is. */
function limitGroup(): HTMLElement {
  const time = shownGroup("showTimeLimit");
  const visits = shownGroup("showVisitLimit");
  return optional(
    () => time() || visits(),
    sep(),
    label(t("Menu.lblLimit")),
    optional(time, ...limitControl(true)),
    optional(visits, ...limitControl(false)),
    sep("gap"),
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
 *  once; a value outside -3..3 / 0..2 only turns the field red. The group shows the values the
 *  engine reports when it starts (Menu.setPdaAndWrn), not the answers to its own sets, and is
 *  disabled while the engine is not free for it (refreshBreadthAdmission).
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
  const name = pda ? t("Menu.separateLblPda") : t("Menu.separateLblWrn");
  const g = group(pda ? "pda" : "wrn", at(pda ? "engine.pda" : "toolbar.wrn", box), label(withColon(name)), value);
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
function pdaMoreShown(): boolean {
  const v = state.view;
  return !!v?.engine.foreground?.katago && v.engine_match?.phase !== "Starting" && !matchPlaying(v);
}

/** A KataGoPda engine's own PDA panel, lblCustomPda and txtPDA (Menu.java:5943-5965, 9658-9661,
 *  9970-9986): the label without a colon and the engine's PDA as it reports it ("%.3f"), shown
 *  until the field is being edited. Each key release with a number sets it on the engine
 *  (`kata-set-param playoutDoublingAdvantage`); empty, "-" or "." send nothing. While 野狐 AI 直连
 *  holds the engine its own 激进度 group shows instead. */
function customPdaGroup(): HTMLElement {
  const value = document.createElement("input");
  value.className = "tb-input custom-pda";
  value.dataset.key = "toolbar.customPda";
  value.setAttribute("aria-label", t("Menu.chkPDA.toolTopText"));
  value.addEventListener("keyup", () => {
    const pda = parseJavaDouble(value.value);
    if (pda !== null) void run({ type: "SetKataParam", param: "Pda", enabled: true, value: pda });
  });
  const g = group("custom-pda", label(t("Menu.separateLblPda")), value);
  let shown: number | null | undefined = undefined;
  updaters.push(() => {
    const display = kataGoPda() && !foxOwnsEngine() && pdaMoreShown() ? "" : "none";
    if (g.style.display !== display) g.style.display = display;
    const pda = state.view?.engine.custom_pda.pda ?? null;
    if (pda === shown || document.activeElement === value) return;
    shown = pda;
    value.value = pda === null ? "" : pda.toFixed(3);
  });
  return g;
}

function pdaMoreButton(): HTMLElement {
  const b = pill("...", "small pda-more");
  b.classList.remove("unavailable");
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

/** sepForPdaWrn: the separator after the komi panel goes with the last PDA / WRN group. */
function kataSeparator(): HTMLElement {
  const s = sep("dash");
  updaters.push(() => {
    const display = kataGroupShown("Pda") || kataGroupShown("Wrn") || pdaMoreShown() ? "" : "none";
    if (s.style.display !== display) s.style.display = display;
  });
  return s;
}

function komiSpinner(): HTMLElement {
  const spinner = document.createElement("span"); spinner.className = "tb-spin";
  for (const step of [0.5, -0.5]) {
    const arrow = document.createElement("i");
    arrow.title = step > 0 ? t("Accessibility.increaseKomi") : t("Accessibility.decreaseKomi");
    arrow.onclick = () => {
      const field = document.getElementById("komiField") as HTMLInputElement;
      if (!field.value.trim() || !Number.isFinite(Number(field.value))) return;
      const komi = Number(field.value) + step;
      field.value = String(komi);
      void run({ type: "SetKomi", komi });
    };
    spinner.append(arrow);
  }
  return spinner;
}

function komiField(): HTMLInputElement {
  const control = document.createElement("input"); control.id = "komiField"; control.className = "tb-input";
  control.type = "number"; control.step = "0.5"; control.setAttribute("aria-label", t("Menu.komi"));
  const commit = () => { if (control.value.trim() && Number.isFinite(Number(control.value))) void run({ type: "SetKomi", komi: Number(control.value) }); };
  control.onchange = commit;
  control.onkeydown = (event) => {
    if (event.key === "Enter") { commit(); control.blur(); }
    if (event.key === "ArrowUp" || event.key === "ArrowDown") {
      event.preventDefault();
      const komi = Number(control.value) + (event.key === "ArrowUp" ? 0.5 : -0.5);
      control.value = String(komi); void run({ type: "SetKomi", komi });
    }
  };
  return control;
}

/** The search features button (Menu.java:8660-8685): FunctionSearchDialog.SearchIcon and the title,
 *  which collapses to the icon alone below a 900 px wide window. Not focusable, so the control
 *  that had the focus gets it back when the search is cancelled. */
function searchButton(): HTMLElement {
  const title = t("FunctionSearch.title");
  const b = document.createElement("span");
  b.className = "tb-pill plain search";
  b.dataset.key = "function.search";
  b.title = title;
  b.setAttribute("role", "button");
  b.setAttribute("aria-label", title);
  const glass = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  glass.setAttribute("viewBox", "0 0 16 16");
  glass.setAttribute("aria-hidden", "true");
  glass.innerHTML = '<circle cx="5.5" cy="5.5" r="4.5"/><line x1="9" y1="9" x2="14" y2="14"/>';
  const text = document.createElement("span");
  text.className = "label";
  b.append(glass, text);
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
  const b = pill(t("Menu.foxKifuButton"), "w80", "menu.foxKifu");
  b.title = t("Menu.foxKifu");
  return b;
}

/** btnTencentKifu (Menu.java:8369-8384): primary style, 腾讯棋谱 as its tip. */
function tencentKifuButton(): HTMLElement {
  const b = pill(t("Menu.tencentKifuButton"), "w80", "menu.tencentKifu");
  b.title = t("Menu.tencentKifu");
  return b;
}

/** btnDoubleMenu (Menu.java:7534-7550, 8259): first in the row while its 自定义内容 choice is on.
 *  The row shows only 分开, so the button shows ▲, which turns the row to 合并. */
function doubleMenuButton(): HTMLElement {
  const b = document.createElement("span");
  b.className = "tb-icon";
  b.dataset.key = "toolbar.double-menu";
  b.title = t("Accessibility.toolbarDetailsDescription");
  const image = document.createElement("img");
  image.src = "/assets/up.png";
  image.alt = "";
  b.append(image);
  b.onclick = toggleDoubleMenu;
  return b;
}

/** 选点: (lblShowCandidate, Menu.java:9874-9899): shown with either 黑 / 白 or 胜率 / 计算量 / 目数,
 *  with a separator between the two while both are. */
function candidateGroup(): HTMLElement {
  const sides = shownGroup("showCandidateSides");
  const info = shownGroup("showCandidateInfo");
  const caption = label(t("Menu.lblShowCandidate"));
  caption.classList.add("candidate");
  return optional(
    () => sides() || info(),
    sep(),
    caption,
    optional(sides, at("toolbar.candidate-sides", candidateCheck(t("Menu.Black"), true)), candidateCheck(t("Menu.White"), false)),
    optional(
      info,
      optional(sides, sep()),
      at("toolbar.candidate-info", candidateInfoCheck(t("Menu.winrate"), "menu.suggestion1", "toolbar.candidate-winrate")),
      candidateInfoCheck(t("Menu.visits"), "menu.suggestion2", "toolbar.candidate-visits"),
      candidateInfoCheck(t("Menu.score"), "menu.suggestion3", "toolbar.candidate-score"),
    ),
  );
}

/** Menu.doubleMenu / setKomiPanelInDoubleMenu (Menu.java:8251-9931): each 自定义内容 group with
 *  the separator after it; 自动换行 off keeps one row (LizzieFrame.java:14503-14507). */
export function buildToolbar(host: HTMLElement): void {
  const search = searchButton();
  const setup = pill(t("Menu.autoSetupButton"), "wide w80", "engine.setup");
  setup.dataset.key = "engine.setup"; setup.title = t("Menu.autoSetup"); setup.setAttribute("role", "button"); setup.tabIndex = 0;
  setup.onkeydown = (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); COMMANDS["engine.setup"].run(); } };
  const rules = shownGroup("showRules");
  const parameters = shownGroup("showParameters");
  const boardSize = shownGroup("showBoardSize");
  const saveLoad = shownGroup("showSaveLoad");
  host.append(
    optional(shownGroup("showDoubleMenuButton"), doubleMenuButton()),
    optional(
      shownGroup("showBasicButtons"),
      sep("lead"),
      buttons(
        icon("newFile.png", t("Menu.btnNewFile.toolTipText"), "files.new"),
        icon("open.png", t("Menu.btnOpen.toolTipText"), "files.open"),
        icon("save.png", t("Menu.btnSave.toolTipText"), "files.save"),
        foxKifuButton(),
        tencentKifuButton(),
        strengthButton(),
      ),
      buttons(setup, flashButton(), search),
      buttons(
        icon("hawkeye2.png", t("Menu.btnHawkeye.toolTipText"), "analysis.bad-moves"),
        rankMarkButton(),
        at("toolbar.change-turn", icon("pass.png", t("Menu.btnChangeTurn.toolTipText"), "toolbar.change-turn")),
        icon("setmain.png", t("Menu.btnSetMain.toolTipText"), "menu.setAsMain"),
        icon("backmain.png", t("Menu.btnBackMain.toolTipText"), "menu.backToMainBranch"),
        icon("control.png", t("Menu.btnContrlPanel.toolTipText")),
        ...markupTools(),
      ),
      sep(),
    ),
    optional(shownGroup("showEditBar"), ...editBar(), sep()),
    optional(
      shownGroup("showForce"),
      at("toolbar.force-allow", commandIcon("blueallow.png", "blueallow2.png", t("Menu.selectAllowToolTipText"), "toolbar.force-allow", "force")),
      at("toolbar.force-allow-options", limitButton(true, t("Accessibility.allowMoveOptions"))),
      at("toolbar.force-avoid", commandIcon("redavoid.png", "redavoid2.png", t("Menu.selectAvoidToolTipText"), "toolbar.force-avoid", "force")),
      at("toolbar.force-avoid-options", limitButton(false, t("Accessibility.avoidMoveOptions"))),
      at("toolbar.force-clear", icon("clear.png", t("Menu.clearSelectToolTipText"), "toolbar.force-clear", "force")),
      sep(),
    ),
    optional(shownGroup("showGameControls"), ...gameControls(), sep()),
    group("komi", label(t("Menu.komi")), komiField(), komiSpinner()),
    kataSeparator(),
    kataGroup("Pda"),
    customPdaGroup(),
    pdaMoreButton(),
    kataGroup("Wrn"),
    limitGroup(),
    optional(shownGroup("showAnalyzeSides"), sep(), label(t("Menu.lblShowAnalyze")), analyzeCheck(true), analyzeCheck(false)),
    candidateGroup(),
    // 死活题 alone brings no separator (Menu.java:9905-9909).
    optional(() => rules() || parameters() || boardSize() || saveLoad(), sep()),
    optional(rules, pill(t("Menu.rulesBtn"), "small", "engine.rules")),
    tsumegoButton(),
    optional(parameters, pill(t("Menu.paramsBtn"), "small", "engine.parameters")),
    // setBoardSize opens 设置棋盘大小 (Menu.java:6145-6153).
    optional(boardSize, pill(t("Menu.gobanBtn"), "small", "menu.setBoard")),
    optional(saveLoad, pill(t("Menu.saveBtn"), "small", "menu.saveAndLoad")),
    clockLabel(),
  );
  updaters.push(() => host.classList.toggle("nowrap", !topToolbarWraps()));
}

/** btnPlayerStrengthEstimate (Menu.java:8386-8404): 棋力评估 with the menu item's text as its tip. */
function strengthButton(): HTMLElement {
  const b = pill(t("Menu.playerStrengthEstimateButton"), "w80", "menu.playerStrengthEstimate");
  b.title = t("Menu.playerStrengthEstimate");
  return b;
}

/** Menu.tsumeGoBtn: 死活题, shown while 自定义工具栏 > 死活 is on (show-tsume-go-menu). */
function tsumegoButton(): HTMLElement {
  const b = pill(t("Menu.tsumeGoBtn"), "small", "game.tsumego");
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
