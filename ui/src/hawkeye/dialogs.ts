// 超级鹰眼's dialogs, drawn inside its own window as non-modal panels with a title bar (Java
// JDialogs owned by MoveListFrame): 列表筛选 (SetThreshold.java, fromMoveList), 统计阈值
// (StatisticsThreshold.java), 关于吻合度 (SetMatchAiPara.java) and the HtmlMessage boxes they open.
// The window holds only their drafts; every committed value goes to the main window as a core
// command or a display change. Sizes and colours follow the Java screenshots (FlatLaf-like fields
// and buttons, Metal spinners) measured in t15-whole-game/java-ref3 j05 / j08 / j13.
import type { Command, DisplaySettings, HawkeyeSettings } from "../contract/generated";
import { t } from "../i18n";
import { type HawkeyePayload, LOSS_COLORS, settingsCommand, UI_FONT } from "./model";

export interface DialogActions {
  run(command: Command): void;
  display(change: Partial<DisplaySettings>): void;
}

/** An interactive control for acceptance automation: its `data-key`, text, state and centre in
 *  window client coordinates. */
export interface ControlTarget {
  key: string;
  text: string;
  checked?: boolean;
  value?: string;
  x: number;
  y: number;
}

/** The visible `[data-key]` controls under `root`. */
export function controlTargets(root: ParentNode): ControlTarget[] {
  const targets: ControlTarget[] = [];
  for (const el of root.querySelectorAll<HTMLElement>("[data-key]")) {
    if (el.hidden || el.getClientRects().length === 0) continue;
    const r = el.getBoundingClientRect();
    const target: ControlTarget = {
      key: el.dataset.key!,
      text: (el.title || el.textContent || "").trim(),
      x: Math.round(r.left + r.width / 2),
      y: Math.round(r.top + r.height / 2),
    };
    if (el instanceof HTMLInputElement) {
      if (el.type === "checkbox") target.checked = el.checked;
      else target.value = el.value;
    }
    targets.push(target);
  }
  return targets;
}

const STYLE_ID = "hawkeye-dialogs-style";
const STYLE = `
.hkd-layer { position: fixed; inset: 0; z-index: 1000; pointer-events: none; }
.hkd-modal { position: absolute; inset: 0; pointer-events: auto; }
.hkd { position: absolute; pointer-events: auto; display: flex; flex-direction: column; box-sizing: border-box;
  font: 12px ${UI_FONT}; color: rgb(31, 41, 38); background: rgb(246, 247, 249);
  border: 1px solid rgb(122, 138, 153); box-shadow: 0 4px 14px rgba(0, 0, 0, 0.3); }
.hkd-title { flex: none; display: flex; align-items: center; height: 24px; padding: 0 0 0 8px; gap: 8px;
  background: rgb(238, 238, 238); border-bottom: 1px solid rgb(215, 216, 216); cursor: default; user-select: none; }
.hkd-title span { flex: 1; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.hkd-close { width: 28px; height: 23px; padding: 0; border: 0; background: transparent; font: 16px ${UI_FONT}; color: rgb(60, 60, 60); }
.hkd-close:hover { background: rgb(232, 17, 35); color: #fff; }
.hkd-body { flex: 1; min-height: 0; }
.hkd .hkd-label { display: flex; align-items: center; white-space: nowrap; user-select: none; }
.hkd-btn { height: 15px; padding: 0 7px; box-sizing: content-box; font: 12px ${UI_FONT}; line-height: 13px; color: rgb(31, 41, 38);
  background: #fff; border: 1px solid rgb(215, 216, 216); border-radius: 3px; white-space: nowrap; }
.hkd-btn:active { background: rgb(232, 232, 232); }
.hkd-field { min-width: 0; height: 100%; box-sizing: border-box; margin: 0; padding: 0 3px; font: 12px ${UI_FONT}; color: rgb(31, 41, 38);
  background: #fff; border: 1px solid rgb(219, 219, 219); border-radius: 3px; outline: none; }
.hkd-field:focus { border-color: rgb(140, 170, 210); }
.hkd-field:disabled { color: rgb(184, 207, 229); background: #fff; }
/* SetThreshold: EmptyBorder(12), GridLayout(4, 2, 4, 4), buttonBar EmptyBorder(12, 0, 0, 0). */
.hkd-set { padding: 12px; }
.hkd-set-grid { display: inline-grid; grid-template-columns: repeat(2, 1fr); grid-auto-rows: 21px; gap: 4px; }
.hkd-set-buttons { display: flex; justify-content: center; gap: 5px; padding-top: 12px; }
.hkd-set input[type=checkbox] { width: 13px; height: 13px; margin: 0 0 0 4px; align-self: center; }
/* JSpinner (Metal): field and a 16 px column of two arrow buttons. */
.hkd-spin { display: flex; min-width: 0; border: 1px solid rgb(122, 138, 153); background: #fff; }
.hkd-spin input { flex: 1; min-width: 0; margin: 0; padding: 0 3px; border: 0; outline: none; text-align: right;
  font: 12px ${UI_FONT}; color: rgb(51, 51, 51); background: #fff; }
.hkd-arrows { flex: none; display: flex; flex-direction: column; width: 15px; border-left: 1px solid rgb(122, 138, 153); }
.hkd-arrow { flex: 1; display: flex; align-items: center; justify-content: center; background: rgb(238, 238, 238); user-select: none; }
.hkd-arrow + .hkd-arrow { border-top: 1px solid rgb(122, 138, 153); }
.hkd-arrow:active { background: rgb(200, 221, 242); }
.hkd-arrow::after { content: ""; border: 4px solid transparent; }
.hkd-arrow.up::after { border-bottom: 4px solid #000; border-top: 0; }
.hkd-arrow.down::after { border-top: 4px solid #000; border-bottom: 0; }
/* StatisticsThreshold: labelPanel EmptyBorder(5, 0, 5, 0) GridLayout(1, 2, 3, 3); thresholdPanel
   EmptyBorder(0, 5, 0, 5) GridLayout(1, 2, 3, 3) of GridLayout(6, 3, 3, 3); FlowLayout buttons. */
.hkd-stat { display: inline-flex; flex-direction: column; }
.hkd-stat-head { display: grid; grid-template-columns: repeat(2, 1fr); gap: 3px; padding: 5px 0; }
.hkd-stat-head .hkd-label { justify-content: center; height: 15px; }
.hkd-stat-panels { display: grid; grid-template-columns: repeat(2, 1fr); gap: 3px; padding: 0 5px; }
.hkd-stat-grid { display: grid; grid-template-columns: repeat(3, 1fr); grid-auto-rows: 21px; gap: 3px; }
.hkd-stat-grid .hkd-field { width: 45px; }
.hkd-stat-buttons { display: flex; justify-content: center; gap: 5px; padding: 5px; }
/* SetMatchAiPara: a null layout at its setBounds. */
.hkd-matchai { position: relative; }
.hkd-matchai > *, .hkd .hkd-matchai > .hkd-label { position: absolute; display: block; margin: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
/* HtmlMessage: a JTextPane with EmptyBorder(12). */
.hkd-message { padding: 12px; max-width: 560px; line-height: 1.3; user-select: text; background: rgb(248, 249, 251); color: rgb(32, 36, 40); }
`;

function injectStyle(): void {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = STYLE;
  document.head.append(style);
}

interface Panel {
  /** The Java title key. */
  titleKey: string;
  el: HTMLElement;
  modal: HTMLElement | null;
}

/** Open panels, bottom first. */
const panels: Panel[] = [];
let layer: HTMLDivElement | null = null;
let zTop = 0;
/** The latest payload: settings changes merge into its settings (refreshDialogs). */
let latest: HawkeyePayload | null = null;

function raise(panel: Panel): void {
  const k = panels.indexOf(panel);
  if (k < 0) return;
  panels.splice(k, 1);
  panels.push(panel);
  zTop += 2;
  if (panel.modal) panel.modal.style.zIndex = String(zTop - 1);
  panel.el.style.zIndex = String(zTop);
}

function close(panel: Panel): void {
  const k = panels.indexOf(panel);
  if (k < 0) return;
  panels.splice(k, 1);
  panel.el.remove();
  panel.modal?.remove();
}

function closeKind(titleKey: string): void {
  for (const panel of panels.filter((p) => p.titleKey === titleKey)) close(panel);
}

/** A dialog panel with a title bar, centred in the window (setLocationRelativeTo(owner)). The
 *  title bar drags it; its close button hides it like JDialog's HIDE_ON_CLOSE. */
function openPanel(titleKey: string, title: string, body: HTMLElement, keyPrefix: string, options: { modal?: boolean; width?: number; height?: number } = {}): Panel {
  injectStyle();
  if (!layer || !layer.isConnected) {
    layer = document.createElement("div");
    layer.className = "hkd-layer";
    document.body.append(layer);
  }
  const el = document.createElement("div");
  el.className = "hkd";
  el.setAttribute("role", "dialog");
  el.dataset.dialog = titleKey;
  const bar = document.createElement("div");
  bar.className = "hkd-title";
  const caption = document.createElement("span");
  caption.textContent = title;
  const closeButton = document.createElement("button");
  closeButton.className = "hkd-close";
  closeButton.type = "button";
  closeButton.textContent = "×";
  closeButton.dataset.key = `${keyPrefix}.close`;
  closeButton.title = "Close";
  bar.append(caption, closeButton);
  body.classList.add("hkd-body");
  el.append(bar, body);
  if (options.width !== undefined) el.style.width = `${options.width}px`;
  if (options.height !== undefined) el.style.height = `${options.height}px`;

  let modal: HTMLElement | null = null;
  if (options.modal) {
    modal = document.createElement("div");
    modal.className = "hkd-modal";
    modal.addEventListener("mousedown", (ev) => ev.preventDefault());
    layer.append(modal);
  }
  layer.append(el);
  const panel: Panel = { titleKey, el, modal };
  panels.push(panel);
  raise(panel);

  closeButton.addEventListener("click", () => close(panel));
  el.addEventListener("mousedown", () => raise(panel), true);
  // The dialog has the focus: the window's own key bindings (Y, Q, arrows, Space) stay out.
  el.addEventListener("keydown", (ev) => ev.stopPropagation());
  el.addEventListener("keyup", (ev) => ev.stopPropagation());
  el.addEventListener("contextmenu", (ev) => ev.stopPropagation());

  const place = (x: number, y: number) => {
    el.style.left = `${Math.max(0, Math.min(x, window.innerWidth - el.offsetWidth))}px`;
    el.style.top = `${Math.max(0, Math.min(y, window.innerHeight - el.offsetHeight))}px`;
  };
  place(Math.round((window.innerWidth - el.offsetWidth) / 2), Math.round((window.innerHeight - el.offsetHeight) / 2));
  bar.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0 || ev.target === closeButton) return;
    ev.preventDefault();
    const dx = ev.clientX - el.offsetLeft;
    const dy = ev.clientY - el.offsetTop;
    const move = (e: MouseEvent) => place(e.clientX - dx, e.clientY - dy);
    const up = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
    };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  });
  return panel;
}

function button(text: string, key: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.className = "hkd-btn";
  b.textContent = text;
  b.dataset.key = key;
  b.addEventListener("click", onClick);
  return b;
}

function label(text: string): HTMLElement {
  const l = document.createElement("div");
  l.className = "hkd-label";
  l.textContent = text;
  return l;
}

/** HtmlMessage (HtmlMessage.java): the bundle's HTML in a text pane with a 12 px border. Utils.showMsg
 *  opens it modal titled Message.title; Utils.showHtmlMessage non-modal. One per title. */
function showMessage(titleKey: string, title: string, html: string, modal: boolean): void {
  closeKind(titleKey);
  const body = document.createElement("div");
  body.className = "hkd-message";
  body.dataset.key = "hawkeye.message.text";
  // Resource-bundle HTML (<html>…<br />…</html>), not user input.
  body.innerHTML = html;
  openPanel(titleKey, title, body, "hawkeye.message", { modal });
}

/** Java's grouping NumberFormat of the JSpinner.NumberEditor ("#,##0"). */
const grouped = (n: number) => n.toLocaleString("en-US");

interface Spinner {
  el: HTMLElement;
  value(): number;
}

/** JSpinner over SpinnerNumberModel(value, min, max, step): the arrows (and Up / Down in the
 *  field) step unless the result leaves the range; typed text commits on Enter or focus loss and
 *  reverts when it is not a whole number in range (COMMIT_OR_REVERT). `changed` runs on every new
 *  value (ChangeListener). */
function spinner(key: string, initial: number, min: number, max: number, step: number, changed: () => void): Spinner {
  let value = initial;
  const el = document.createElement("div");
  el.className = "hkd-spin";
  const input = document.createElement("input");
  input.type = "text";
  input.dataset.key = key;
  const show = () => {
    input.value = grouped(value);
  };
  const set = (v: number) => {
    if (v === value) {
      show();
      return;
    }
    value = v;
    show();
    changed();
  };
  const commit = () => {
    const text = input.value.trim();
    if (!/^[+-]?(\d{1,3}(,\d{3})+|\d+)$/.test(text)) {
      show();
      return;
    }
    const n = Number(text.replace(/,/g, ""));
    if (n < min || n > max) show();
    else set(n);
  };
  const stepBy = (up: boolean) => {
    commit();
    const next = up ? value + step : value - step;
    if (next >= min && next <= max) set(next);
  };
  input.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") commit();
    else if (ev.key === "ArrowUp" || ev.key === "ArrowDown") {
      ev.preventDefault();
      stepBy(ev.key === "ArrowUp");
    }
  });
  input.addEventListener("blur", commit);

  const arrows = document.createElement("div");
  arrows.className = "hkd-arrows";
  for (const up of [true, false]) {
    const arrow = document.createElement("div");
    arrow.className = `hkd-arrow ${up ? "up" : "down"}`;
    arrow.dataset.key = `${key}.${up ? "up" : "down"}`;
    // BasicSpinnerUI.ArrowButtonHandler: one step on press, then auto-repeat while held.
    let timer = 0;
    const stop = () => {
      window.clearTimeout(timer);
      window.clearInterval(timer);
      window.removeEventListener("mouseup", stop);
    };
    arrow.addEventListener("mousedown", (ev) => {
      if (ev.button !== 0) return;
      ev.preventDefault();
      stepBy(up);
      timer = window.setTimeout(() => {
        timer = window.setInterval(() => stepBy(up), 60);
      }, 300);
      window.addEventListener("mouseup", stop);
    });
    arrow.addEventListener("mouseleave", stop);
    arrows.append(arrow);
  }
  el.append(input, arrows);
  show();
  return { el, value: () => value };
}

export function openSetThreshold(p: HawkeyePayload, actions: DialogActions): void {
  latest = p;
  // MoveListFrame.java:1381: a new SetThreshold(thisDialog, true) per click.
  closeKind("SetThreshold.title");
  const s = p.settings;
  // originMoveList* (SetThreshold.java:41-44): what 取消 restores (187-198).
  const origin: Partial<HawkeyeSettings> = {
    winrateThreshold: s.winrateThreshold,
    scoreThreshold: s.scoreThreshold,
    visitsThreshold: s.visitsThreshold,
    topCurrentMove: s.topCurrentMove,
  };
  // fromMoveList: every change saves all four at once (Config.saveThreshold, SetThreshold.java:111-151).
  const save = () =>
    actions.run(
      settingsCommand((latest ?? p).settings, {
        winrateThreshold: winrate.value(),
        scoreThreshold: score.value(),
        visitsThreshold: visits.value(),
        topCurrentMove: top.checked,
      }),
    );
  const winrate = spinner("hawkeye.setThreshold.winrate", s.winrateThreshold, 0, 99, 1, save);
  const score = spinner("hawkeye.setThreshold.score", s.scoreThreshold, 0, 99, 1, save);
  const visits = spinner("hawkeye.setThreshold.visits", s.visitsThreshold, 0, 999999, 100, save);
  const top = document.createElement("input");
  top.type = "checkbox";
  top.checked = s.topCurrentMove;
  top.dataset.key = "hawkeye.setThreshold.topCurrentMove";
  top.addEventListener("change", save);

  const body = document.createElement("div");
  body.className = "hkd-set";
  const grid = document.createElement("div");
  grid.className = "hkd-set-grid";
  grid.append(
    label(t("Movelistframe.lblDropWinrate")), winrate.el,
    label(t("Movelistframe.lblDropScore")), score.el,
    label(t("Movelistframe.lblPlayouts")), visits.el,
    label(t("Movelistframe.chkTopCurrentMove")), top,
  );
  const buttons = document.createElement("div");
  buttons.className = "hkd-set-buttons";
  body.append(grid, buttons);
  const panel = openPanel("SetThreshold.title", t("SetThreshold.title"), body, "hawkeye.setThreshold");
  buttons.append(
    // Values are already saved: 确定 only hides (166-177).
    button(t("SetKataEngines.btnApply"), "hawkeye.setThreshold.confirm", () => close(panel)),
    button(t("SetKataEngines.btnCancel"), "hawkeye.setThreshold.cancel", () => {
      actions.run(settingsCommand((latest ?? p).settings, origin));
      close(panel);
    }),
  );
}

/** String.valueOf(double): whole numbers keep ".0" (and -0.0 its sign). */
function javaDouble(v: number): string {
  if (Object.is(v, -0)) return "-0.0";
  return Number.isInteger(v) ? v.toFixed(1) : String(v);
}

/** Double.parseDouble of the trimmed text (Utils.parseTextToDouble), null when Java throws. */
function parseJavaDouble(text: string): number | null {
  const s = text.trim();
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[fFdD]?$/.test(s)) return null;
  return Number(s.replace(/[fFdD]$/, ""));
}

/** KomiDocument(false).insertString (KomiDocument.java:16-33) for an insertion of `text` at
 *  `offset` into `old`: the result must parse as a BigDecimal (a lone "-" passes) and at most one
 *  character may follow an earlier decimal point. */
function komiAccepts(old: string, offset: number, text: string): boolean {
  const next = old.slice(0, offset) + text + old.slice(offset);
  if (next.trim() !== "-" && !/^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/.test(next)) return false;
  const point = old.lastIndexOf(".");
  return !(point > -1 && point < offset && next.length - (point + 1) > 1);
}

/** A JTextField with a KomiDocument: a replacement removes the selection first, then inserts. */
function komiField(input: HTMLInputElement): void {
  input.addEventListener("beforeinput", (ev) => {
    if (!ev.inputType.startsWith("insert")) return;
    const text = ev.data ?? ev.dataTransfer?.getData("text/plain") ?? "";
    const start = input.selectionStart ?? input.value.length;
    const end = input.selectionEnd ?? start;
    const old = input.value.slice(0, start) + input.value.slice(end);
    if (komiAccepts(old, start, text)) return;
    ev.preventDefault();
    if (end > start) {
      input.value = old;
      input.setSelectionRange(start, start);
      input.dispatchEvent(new Event("input"));
    }
  });
}

const DEFAULT_WIN = ["1.0", "1.0", "3.0", "6.0", "12.0", "24.0"];
const DEFAULT_SCORE = ["0.5", "0.5", "1.5", "3.0", "6.0", "12.0"];

export function openStatisticsThreshold(p: HawkeyePayload, actions: DialogActions): void {
  latest = p;
  // MoveListFrame.java:1394: a new StatisticsThreshold(thisDialog) per click.
  closeKind("StatisticsThreshold.title");
  const fields = (kind: "score" | "win") => {
    const inputs: HTMLInputElement[] = [];
    const grid = document.createElement("div");
    grid.className = "hkd-stat-grid";
    // Rows 5 (worst) to 0; row 0 is disabled and mirrors row 1 (StatisticsThreshold.java:120-250).
    for (let k = 5; k >= 0; k--) {
      const input = document.createElement("input");
      input.type = "text";
      input.className = "hkd-field";
      input.dataset.key = `hawkeye.statThreshold.${kind}.${k}`;
      if (k === 0) input.disabled = true;
      else komiField(input);
      inputs[k] = input;
      const colour = document.createElement("div");
      colour.style.background = LOSS_COLORS[k];
      grid.append(label(k === 0 ? `${t("StatisticsThreshold.loss")} <` : `${t("StatisticsThreshold.loss")}>=`), input, colour);
    }
    inputs[1].addEventListener("input", () => {
      inputs[0].value = inputs[1].value;
    });
    return { inputs, grid };
  };
  const score = fields("score");
  const win = fields("win");
  // loadValue (278-292): the stored thresholds are negative.
  const load = (inputs: HTMLInputElement[], values: string[]) => inputs.forEach((input, k) => (input.value = values[k]));
  load(win.inputs, [p.winLoss[0], ...p.winLoss].map((v) => javaDouble(-v)));
  load(score.inputs, [p.scoreLoss[0], ...p.scoreLoss].map((v) => javaDouble(-v)));

  const body = document.createElement("div");
  body.className = "hkd-stat";
  const head = document.createElement("div");
  head.className = "hkd-stat-head";
  head.append(label(t("StatisticsThreshold.lblScoreLoss")), label(t("StatisticsThreshold.lblWinLoss")));
  const sides = document.createElement("div");
  sides.className = "hkd-stat-panels";
  sides.append(score.grid, win.grid);
  const buttons = document.createElement("div");
  buttons.className = "hkd-stat-buttons";
  body.append(head, sides, buttons);
  const panel = openPanel("StatisticsThreshold.title", t("StatisticsThreshold.title"), body, "hawkeye.statThreshold");

  // saveSettings (294-345): each row parsed (an unparsable one keeps the stored value), negated,
  // and the upper rows must not be smaller than the lower ones.
  const confirm = () => {
    const now = latest ?? p;
    const read = (inputs: HTMLInputElement[], stored: number[]) =>
      stored.map((v, k) => -(parseJavaDouble(inputs[k + 1].value) ?? -v));
    const winLoss = read(win.inputs, now.winLoss);
    const scoreLoss = read(score.inputs, now.scoreLoss);
    const ordered = (v: number[]) => v[0] >= v[1] && v[1] >= v[2] && v[2] >= v[3] && v[3] >= v[4];
    if (!ordered(winLoss) || !ordered(scoreLoss)) {
      // Utils.showMsg: a modal HtmlMessage titled Message.title.
      showMessage("Message.title", t("Message.title"), t("StatisticsThreshold.wrongTHR"), true);
      return;
    }
    actions.display({
      winLossThresholds: winLoss as DisplaySettings["winLossThresholds"],
      scoreLossThresholds: scoreLoss as DisplaySettings["scoreLossThresholds"],
    });
    close(panel);
  };
  buttons.append(
    button(t("StatisticsThreshold.btnCalcMethod"), "hawkeye.statThreshold.calcMethod", () =>
      showMessage(
        "StatisticsThreshold.calcMethod.title",
        t("StatisticsThreshold.calcMethod.title"),
        t("StatisticsThreshold.calcMethod.content"),
        false,
      ),
    ),
    // resetDefauleValue (253-267): only the fields; 确定 saves them.
    button(t("StatisticsThreshold.btnReset"), "hawkeye.statThreshold.reset", () => {
      load(win.inputs, DEFAULT_WIN);
      load(score.inputs, DEFAULT_SCORE);
    }),
    button(t("StatisticsThreshold.btnConfirm"), "hawkeye.statThreshold.confirm", confirm),
    button(t("StatisticsThreshold.btnCancel"), "hawkeye.statThreshold.cancel", () => close(panel)),
  );
}

/** SetMatchAiPara (SetMatchAiPara.java): Lizzie.setFrameSize(810, 215) gives an 820 × 225 window
 *  at scale 1; a null layout with the description, the "计算方式:" label and the formula image
 *  scaled to 708 × 96. Java keeps one instance per MoveListFrame (MoveListFrame.java:828). */
export function openMatchAiHelp(p: HawkeyePayload): void {
  latest = p;
  const existing = panels.find((panel) => panel.titleKey === "SetMatchAiPara.title");
  if (existing) {
    raise(existing);
    return;
  }
  const body = document.createElement("div");
  body.className = "hkd-matchai";
  const place = (el: HTMLElement, x: number, y: number, w: number, h: number) => {
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
    el.style.width = `${w}px`;
    el.style.height = `${h}px`;
    el.style.lineHeight = `${h}px`;
    body.append(el);
  };
  const ai = label(t("SetMatchAiPara.lblAi"));
  ai.dataset.key = "hawkeye.matchAi.text";
  place(ai, 10, 10, 762, 29);
  place(label(t("SetMatchAiPara.lblCalculation")), 10, 50, 93, 15);
  const form = document.createElement("img");
  form.src = p.chinese ? "/assets/matchform.png" : "/assets/matchformeng.png";
  form.alt = "";
  form.draggable = false;
  place(form, 76, 45, 708, 96);
  openPanel("SetMatchAiPara.title", t("SetMatchAiPara.title"), body, "hawkeye.matchAi", { width: 820, height: 225 });
}

/** The open dialog's Java title key, or null. */
export function openDialog(): string | null {
  return panels.at(-1)?.titleKey ?? null;
}

/** Updates an open dialog after a new payload when it shows live values (SetThreshold shows the
 *  committed values). Every dialog here shows its own committed values or draft, as Java's do;
 *  the payload is kept so later writes merge into the current settings and thresholds. */
export function refreshDialogs(p: HawkeyePayload): void {
  latest = p;
}

export function dialogTargets(): ControlTarget[] {
  return layer ? controlTargets(layer) : [];
}
