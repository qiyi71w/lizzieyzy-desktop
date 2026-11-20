// 超级鹰眼's dialogs, drawn inside its own window as non-modal panels with a title bar (Java
// JDialogs owned by MoveListFrame): 列表筛选 (SetThreshold.java, fromMoveList), 统计阈值
// (StatisticsThreshold.java), 关于吻合度 (SetMatchAiPara.java) and the HtmlMessage boxes they open.
// The window holds only their drafts; every committed value goes to the main window as a core
// command or a display change. They take the program's dialog frame (design/dialog.css): the title
// with its close box, grouped rows, the footer band; hawkeye.css places them in the window.
import type { Command, DisplaySettings, HawkeyeSettings } from "../contract/generated";
import { icon } from "../design/icons";
import { closeBox } from "../dialogs";
import { type FooterButtons, footer, row, switchRow } from "../engine/controls";
import { product, t } from "../i18n";
import { type HawkeyePayload, settingsCommand } from "./model";

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
 *  title bar drags it; its close box runs `dismiss`, by default hiding it like JDialog's
 *  HIDE_ON_CLOSE. `buttons` are the footer's buttons for the panel. */
function openPanel(
  titleKey: string,
  title: string,
  body: HTMLElement,
  keyPrefix: string,
  options: { modal?: boolean; buttons?: (panel: Panel) => FooterButtons; dismiss?: (panel: Panel) => void } = {},
): Panel {
  if (!layer || !layer.isConnected) {
    layer = document.createElement("div");
    layer.className = "hkd-layer";
    document.body.append(layer);
  }
  const el = document.createElement("div");
  el.className = "hkd modal-box";
  el.tabIndex = -1;
  el.setAttribute("role", "dialog");
  el.setAttribute("aria-label", title);
  el.dataset.dialog = titleKey;
  let modal: HTMLElement | null = null;
  if (options.modal) {
    modal = document.createElement("div");
    modal.className = "hkd-modal";
    modal.addEventListener("mousedown", (ev) => ev.preventDefault());
    layer.append(modal);
  }
  const panel: Panel = { titleKey, el, modal };
  const bar = document.createElement("div");
  bar.className = "hkd-title modal-title";
  const caption = document.createElement("span");
  caption.textContent = title;
  const closeButton = closeBox(() => (options.dismiss ?? close)(panel));
  closeButton.dataset.key = `${keyPrefix}.close`;
  bar.append(caption, closeButton);
  body.classList.add("hkd-body", "modal-body");
  el.append(bar, body);
  if (options.buttons) footer(el, options.buttons(panel));
  layer.append(el);
  panels.push(panel);
  raise(panel);

  el.addEventListener("mousedown", () => raise(panel), true);
  // The dialog has the focus: the window's own key bindings (Y, Q, arrows, Space) stay out.
  // Escape is its close box.
  el.addEventListener("keydown", (ev) => {
    ev.stopPropagation();
    if (ev.key === "Escape") {
      ev.preventDefault();
      (options.dismiss ?? close)(panel);
    }
  });
  el.addEventListener("keyup", (ev) => ev.stopPropagation());
  el.addEventListener("contextmenu", (ev) => ev.stopPropagation());

  const place = (x: number, y: number) => {
    el.style.left = `${Math.max(0, Math.min(x, window.innerWidth - el.offsetWidth))}px`;
    el.style.top = `${Math.max(0, Math.min(y, window.innerHeight - el.offsetHeight))}px`;
  };
  place(Math.round((window.innerWidth - el.offsetWidth) / 2), Math.round((window.innerHeight - el.offsetHeight) / 2));
  (el.querySelector<HTMLElement>(":is(.hkd-body, .ds-footer) :is(input:not(:disabled), select:not(:disabled), button:not(:disabled))") ?? el).focus();
  bar.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0 || closeButton.contains(ev.target as Node)) return;
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

/** The 关闭 of a panel that only shows something. */
function closeOnly(keyPrefix: string): (panel: Panel) => FooterButtons {
  return (panel) => ({ cancel: button(product("close"), `${keyPrefix}.closeButton`, () => close(panel)) });
}

function button(text: string, key: string, onClick: () => void, primary = false): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  if (primary) b.className = "primary";
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

/** HtmlMessage (HtmlMessage.java): the bundle's HTML as the dialog's text. Utils.showMsg opens it
 *  modal titled Message.title; Utils.showHtmlMessage non-modal. One per title. */
function showMessage(titleKey: string, title: string, html: string, modal: boolean): void {
  closeKind(titleKey);
  const body = document.createElement("div");
  body.className = "hkd-message";
  body.dataset.key = "hawkeye.message.text";
  // Resource-bundle HTML (<html>…<br />…</html>), not user input.
  body.innerHTML = html;
  openPanel(titleKey, title, body, "hawkeye.message", { modal, buttons: closeOnly("hawkeye.message") });
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
    arrow.className = "hkd-arrow";
    arrow.dataset.key = `${key}.${up ? "up" : "down"}`;
    arrow.append(icon(up ? "chevron-up" : "chevron-down"));
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
  const body = document.createElement("div");
  body.className = "hkd-rows";
  row(body, t("Movelistframe.lblDropWinrate"), "", winrate.el);
  row(body, t("Movelistframe.lblDropScore"), "", score.el);
  row(body, t("Movelistframe.lblPlayouts"), "", visits.el);
  const top = switchRow(body, t("Movelistframe.chkTopCurrentMove"), "", s.topCurrentMove).input;
  top.dataset.key = "hawkeye.setThreshold.topCurrentMove";
  top.addEventListener("change", save);
  // 取消 and the close box restore the values the dialog opened with.
  const cancel = (panel: Panel) => {
    actions.run(settingsCommand((latest ?? p).settings, origin));
    close(panel);
  };
  openPanel("SetThreshold.title", t("SetThreshold.title"), body, "hawkeye.setThreshold", {
    dismiss: cancel,
    buttons: (panel) => ({
      // Values are already saved: 确定 only hides (166-177).
      primary: button(t("SetKataEngines.btnApply"), "hawkeye.setThreshold.confirm", () => close(panel), true),
      cancel: button(t("SetKataEngines.btnCancel"), "hawkeye.setThreshold.cancel", () => cancel(panel)),
    }),
  });
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
  // One row per loss class, 5 (worst) to 0, its 目数 and 胜率 fields under the column names; row 0
  // is disabled and mirrors row 1 (StatisticsThreshold.java:120-250).
  const body = document.createElement("div");
  body.className = "hkd-rows";
  const columns = document.createElement("div");
  columns.className = "hkd-stat-columns";
  const scoreName = t("StatisticsThreshold.lblScoreLoss");
  const winName = t("StatisticsThreshold.lblWinLoss");
  columns.append(label(scoreName), label(winName));
  body.append(columns);
  const score: HTMLInputElement[] = [];
  const win: HTMLInputElement[] = [];
  for (let k = 5; k >= 0; k--) {
    const text = k === 0 ? `${t("StatisticsThreshold.loss")} <` : `${t("StatisticsThreshold.loss")}>=`;
    const field = (kind: "score" | "win", column: string) => {
      const input = document.createElement("input");
      input.type = "text";
      input.inputMode = "decimal";
      input.dataset.key = `hawkeye.statThreshold.${kind}.${k}`;
      input.setAttribute("aria-label", `${column} ${text}`);
      if (k === 0) input.disabled = true;
      else komiField(input);
      return input;
    };
    score[k] = field("score", scoreName);
    win[k] = field("win", winName);
    const name = document.createElement("span");
    name.className = "hkd-stat-name";
    const colour = document.createElement("span");
    colour.className = "hkd-swatch";
    colour.style.backgroundColor = `var(--hk-loss-${k})`;
    name.append(colour, text);
    row(body, name, "", score[k], win[k]);
  }
  for (const inputs of [score, win]) inputs[1].addEventListener("input", () => (inputs[0].value = inputs[1].value));
  // loadValue (278-292): the stored thresholds are negative.
  const load = (inputs: HTMLInputElement[], values: string[]) => inputs.forEach((input, k) => (input.value = values[k]));
  load(win, [p.winLoss[0], ...p.winLoss].map((v) => javaDouble(-v)));
  load(score, [p.scoreLoss[0], ...p.scoreLoss].map((v) => javaDouble(-v)));

  // saveSettings (294-345): each row parsed (an unparsable one keeps the stored value), negated,
  // and the upper rows must not be smaller than the lower ones.
  const confirm = (panel: Panel) => {
    const now = latest ?? p;
    const read = (inputs: HTMLInputElement[], stored: number[]) =>
      stored.map((v, k) => -(parseJavaDouble(inputs[k + 1].value) ?? -v));
    const winLoss = read(win, now.winLoss);
    const scoreLoss = read(score, now.scoreLoss);
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
  openPanel("StatisticsThreshold.title", t("StatisticsThreshold.title"), body, "hawkeye.statThreshold", {
    buttons: (panel) => ({
      start: [
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
          load(win, DEFAULT_WIN);
          load(score, DEFAULT_SCORE);
        }),
      ],
      primary: button(t("StatisticsThreshold.btnConfirm"), "hawkeye.statThreshold.confirm", () => confirm(panel), true),
      cancel: button(t("StatisticsThreshold.btnCancel"), "hawkeye.statThreshold.cancel", () => close(panel)),
    }),
  });
}

/** SetMatchAiPara (SetMatchAiPara.java): the description, then the "计算方式:" label beside the
 *  formula image at 708 × 96. Java keeps one instance per MoveListFrame (MoveListFrame.java:828). */
export function openMatchAiHelp(p: HawkeyePayload): void {
  latest = p;
  const existing = panels.find((panel) => panel.titleKey === "SetMatchAiPara.title");
  if (existing) {
    raise(existing);
    return;
  }
  const body = document.createElement("div");
  body.className = "hkd-matchai";
  const ai = label(t("SetMatchAiPara.lblAi"));
  ai.dataset.key = "hawkeye.matchAi.text";
  const form = document.createElement("img");
  form.src = p.chinese ? "/assets/matchform.png" : "/assets/matchformeng.png";
  form.alt = "";
  form.draggable = false;
  body.append(ai, label(t("SetMatchAiPara.lblCalculation")), form);
  openPanel("SetMatchAiPara.title", t("SetMatchAiPara.title"), body, "hawkeye.matchAi", { buttons: closeOnly("hawkeye.matchAi") });
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
