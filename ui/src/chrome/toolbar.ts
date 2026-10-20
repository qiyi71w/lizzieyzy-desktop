// Java double-menu top toolbar (Menu.doubleMenu / TopHeaderPanel) with the default item set of
// the fixed Java build (Config.java defaults; Menu.java:8189-8230, 9633-9931). Items this client
// implements run their catalog command; the others keep their Java look and report that the
// capability is not available yet.
import { run, state, unavailable } from "../app";
import { COMMANDS } from "../commands";
import { product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";

function marked(el: HTMLElement, command: string | undefined, tip: string): HTMLElement {
  const entry = command ? COMMANDS[command] : undefined;
  if (entry) {
    el.title = tip;
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

function checkLabel(text: string, checked: boolean): HTMLElement {
  const l = document.createElement("label");
  l.className = "tb-checklabel";
  l.append(check(checked), document.createTextNode(text));
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

export function buildToolbar(host: HTMLElement): void {
  const search = pill(` ${t("FunctionSearch.title")}`, "plain search");
  const glass = document.createElement("img");
  glass.src = "/assets/search.png";
  glass.alt = "";
  search.prepend(glass);
  // disabled while no game is running (Menu.java:9101-9104)
  const pause = document.createElement("span");
  pause.className = "tb-pill disabled pause";
  pause.textContent = t("Menu.pauseGameBtn");
  const setup = pill(t("Menu.autoSetupButton"), "wide w80", "engine.setup");
  setup.dataset.key = "engine.setup"; setup.title = t("Menu.autoSetup"); setup.setAttribute("role", "button"); setup.tabIndex = 0;
  setup.onkeydown = (event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); COMMANDS["engine.setup"].run(); } };
  host.append(
    sep("lead"),
    buttons(
      icon("newFile.png", t("Menu.btnNewFile.toolTipText")),
      icon("open.png", t("Menu.btnOpen.toolTipText"), "files.open"),
      icon("save.png", t("Menu.btnSave.toolTipText"), "files.save"),
      pill(t("Menu.foxKifuButton"), "w80"),
      pill(t("Menu.tencentKifuButton"), "w80"),
      pill(t("Menu.playerStrengthEstimateButton"), "w80"),
    ),
    buttons(setup, icon("flash.png", t("Menu.btnAnalyze.btnFlashAnalyze")), search),
    buttons(
      icon("hawkeye2.png", t("Menu.btnHawkeye.toolTipText")),
      icon("RankMarkOn.png", t("Menu.btnRankMark.toolTipText")),
      icon("pass.png", t("Menu.btnChangeTurn.toolTipText")),
      icon("setmain.png", t("Menu.btnSetMain.toolTipText")),
      icon("backmain.png", t("Menu.btnBackMain.toolTipText"), "menu.backToMainBranch"),
      icon("control.png", t("Menu.btnContrlPanel.toolTipText")),
      icon("mark1.png", t("Menu.btnMarkup.toolTipText")),
    ),
    sep(),
    icon("smallblack1.png", t("Menu.playBlackToolTipText")),
    icon("smallwhite.png", t("Menu.playWhiteToolTipText")),
    // iconbh = hb.png at menuIconSize + 8 in a 22 px button (Menu.java:3674-3680, 7602-7626)
    icon("hb.png", t("Menu.playAlternatelyToolTipText"), undefined, "bw"),
    icon("playpass.png", t("Menu.playPassToolTipText"), undefined, "w22"),
    sep(),
    icon("blueallow.png", t("Menu.selectAllowToolTipText"), undefined, "force"),
    icon("horizonDown.png", t("Accessibility.allowMoveOptions"), undefined, "more"),
    icon("redavoid.png", t("Menu.selectAvoidToolTipText"), undefined, "force"),
    icon("horizonDown.png", t("Accessibility.avoidMoveOptions"), undefined, "more"),
    icon("clear.png", t("Menu.clearSelectToolTipText"), undefined, "force"),
    sep(),
    pill(t("Menu.newGameBtn"), "newgame"),
    pause,
    sep(),
    group("komi", label(t("Menu.komi")), komiField(), komiSpinner()),
    sep("dash"),
    // KataGo parameter fields stay empty and unchecked without an engine
    group("pda", check(false), label(withColon(t("Menu.separateLblPda"))), field()),
    group("wrn", check(false), label(withColon(t("Menu.separateLblWrn"))), field()),
    sep(),
    label(t("Menu.lblShowCandidate")),
    checkLabel(t("Menu.Black"), true),
    checkLabel(t("Menu.White"), true),
    sep(),
    pill(t("Menu.rulesBtn"), "small", "engine.rules"),
    pill(t("Menu.tsumeGoBtn"), "small"),
    pill(t("Menu.paramsBtn"), "small"),
    pill(t("Menu.gobanBtn"), "small"),
    pill(t("Menu.saveBtn"), "small"),
  );
}

/** The komi field shows the current game komi (txtKomi: String.valueOf(double), so 6 is "6.0"). */
export function renderToolbar(): void {
  const komi = document.getElementById("komiField") as HTMLInputElement | null;
  const k = state.view?.komi;
  const value = k === undefined ? "" : javaDouble(k);
  if (komi && document.activeElement !== komi && komi.value !== value) komi.value = value;
}
