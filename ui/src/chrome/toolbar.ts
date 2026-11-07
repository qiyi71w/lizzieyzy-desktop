// Java double-menu top toolbar (Menu.doubleMenu / TopHeaderPanel) with the default item set of
// the fixed Java build (Config.java defaults; Menu.java:8189-8230, 9633-9931). Items this client
// implements run their catalog command; the others keep their Java look and report that the
// capability is not available yet.
import { run, state, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { rankMarkLit, setCandidateSide } from "../display/commands";
import { boardUi, onBoardUiChange } from "../display/state";
import { choosePlacement, placementLit } from "../editing/commands";
import { editing, markupToolsShown, onEditingChange } from "../editing/state";
import { product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import type { MenuNode } from "./javaMenus";
import { openPopupMenu } from "./menubar";
import { FLASH_MENU } from "../quick/flash";
import { functionSearchRefused, openFunctionSearch } from "../search/controller";
import { limitPopup } from "../analysis/region";
import { clockText } from "../game/commands";
import { matchPlaying } from "../match/state";
import "../editing/chrome.css";
import "../strength/commands";

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

/** btnMarkup and, while show-markup-tools is on, the markup buttons after it in Java's rightArea
 *  order (Menu.java:8694-8703; the circle button is created but never added). */
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

/** A Java check box whose setting belongs to a later slice: shown, not changeable. */
function check(checked: boolean): HTMLElement {
  const c = document.createElement("input");
  c.type = "checkbox";
  c.className = "tb-check unavailable";
  c.checked = checked;
  c.tabIndex = -1;
  c.title = product("unavailable");
  c.onclick = (e) => {
    e.preventDefault();
    unavailable();
  };
  return c;
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

function field(id?: string): HTMLInputElement {
  const c = document.createElement("input");
  c.className = "tb-input unavailable";
  c.readOnly = true;
  c.tabIndex = -1;
  c.title = product("unavailable");
  if (id) c.id = id;
  return c;
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

export function buildToolbar(host: HTMLElement): void {
  const search = searchButton();
  const setup = pill(t("Menu.autoSetupButton"), "wide w80", "engine.setup");
  setup.dataset.key = "engine.setup"; setup.title = t("Menu.autoSetup"); setup.setAttribute("role", "button"); setup.tabIndex = 0;
  setup.onkeydown = (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); COMMANDS["engine.setup"].run(); } };
  host.append(
    sep("lead"),
    buttons(
      icon("newFile.png", t("Menu.btnNewFile.toolTipText"), "files.new"),
      icon("open.png", t("Menu.btnOpen.toolTipText"), "files.open"),
      icon("save.png", t("Menu.btnSave.toolTipText"), "files.save"),
      pill(t("Menu.foxKifuButton"), "w80"),
      pill(t("Menu.tencentKifuButton"), "w80"),
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
    ...editBar(),
    sep(),
    at("toolbar.force-allow", commandIcon("blueallow.png", "blueallow2.png", t("Menu.selectAllowToolTipText"), "toolbar.force-allow", "force")),
    at("toolbar.force-allow-options", limitButton(true, t("Accessibility.allowMoveOptions"))),
    at("toolbar.force-avoid", commandIcon("redavoid.png", "redavoid2.png", t("Menu.selectAvoidToolTipText"), "toolbar.force-avoid", "force")),
    at("toolbar.force-avoid-options", limitButton(false, t("Accessibility.avoidMoveOptions"))),
    at("toolbar.force-clear", icon("clear.png", t("Menu.clearSelectToolTipText"), "toolbar.force-clear", "force")),
    sep(),
    ...gameControls(),
    sep(),
    group("komi", label(t("Menu.komi")), komiField(), komiSpinner()),
    sep("dash"),
    // KataGo parameter fields stay empty and unchecked without an engine
    group("pda", check(false), label(withColon(t("Menu.separateLblPda"))), field()),
    group("wrn", at("toolbar.wrn", check(false)), label(withColon(t("Menu.separateLblWrn"))), field()),
    sep(),
    label(t("Menu.lblShowCandidate")),
    at("toolbar.candidate-sides", candidateCheck(t("Menu.Black"), true)),
    candidateCheck(t("Menu.White"), false),
    sep(),
    pill(t("Menu.rulesBtn"), "small", "engine.rules"),
    tsumegoButton(),
    pill(t("Menu.paramsBtn"), "small"),
    pill(t("Menu.gobanBtn"), "small"),
    pill(t("Menu.saveBtn"), "small", "menu.saveAndLoad"),
    clockLabel(),
  );
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
