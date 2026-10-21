// Board display dialogs: JOptionPane integer inputs (下一手提示需要计算量, 落子评价 显示最后多手),
// 设置显示手数 (MovenumberDialog.java), 设置变化图回放间隔 (SetReplayTime.java), 延迟显示候选点
// (SetDelayShowCandidates.java) and 自定义信息顺序 (SuggestionInfoOrderSettings.java). Each applies
// through SetDisplaySettings and closes; validation follows the Java dialog.
import { state } from "../app";
import { setDisplay } from "../commands";
import { ask, formPanel, message } from "../dialogs";
import { isChinese, product, registerProduct, t } from "../i18n";
import { GRAPH_FONT, images, javaDouble, playouts } from "../workbench/paint";
import { delayCandidates, setCandidates } from "./state";

// The JDK's own JOptionPane.showInputDialog title (OptionPane.inputDialogTitle).
registerProduct({
  ZhCn: { inputTitle: "输入" },
  ZhTw: { inputTitle: "輸入" },
  EnUs: { inputTitle: "Input" },
  JaJp: { inputTitle: "入力" },
  KoKr: { inputTitle: "입력" },
});

/** Integer.parseInt: an optionally signed decimal int, nothing else. */
export function parseJavaInt(text: string): number | null {
  if (!/^[+-]?\d+$/.test(text)) return null;
  const n = Number.parseInt(text, 10);
  return n >= -(2 ** 31) && n <= 2 ** 31 - 1 ? n : null;
}

/** JTextField.setText(String.valueOf(int)) after JOptionPane.showInputDialog(message, initial):
 *  the typed text, or null for 取消 / Escape. */
export async function inputDialog(text: string, initial: string): Promise<string | null> {
  const body = document.createElement("div");
  body.className = "display-input";
  const label = document.createElement("div");
  label.textContent = text;
  const field = document.createElement("input");
  field.type = "text";
  field.value = initial;
  field.setAttribute("aria-label", text);
  body.append(label, field);
  queueMicrotask(() => { field.focus(); field.select(); });
  return (await ask(product("inputTitle"), body, product("ok"), product("cancel"))) ? field.value : null;
}

/** DigitOnlyFilter (MovenumberDialog / SetReplayTime): inserted text keeps its digits only. */
function digitsOnly(field: HTMLInputElement): void {
  field.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const digits = inserted.replace(/\D+/g, "");
    if (digits === inserted) return;
    e.preventDefault();
    if (digits) field.setRangeText(digits, field.selectionStart ?? field.value.length, field.selectionEnd ?? field.value.length, "end");
  });
}

/** txtFieldValue: blank or ten or more characters read as 0. */
function fieldValue(field: HTMLInputElement): number {
  const s = field.value.trim();
  if (s === "" || s.length >= String(2 ** 31 - 1).length) return 0;
  return Number.parseInt(s, 10);
}

/** A null-layout Swing panel: children placed at their setBounds. */
export function absolute(width: number, height: number): HTMLDivElement {
  const panel = document.createElement("div");
  panel.className = "modal-body display-abs";
  panel.style.width = `${width}px`;
  panel.style.height = `${height}px`;
  return panel;
}

export function place<T extends HTMLElement>(panel: HTMLElement, el: T, x: number, y: number, w: number, h: number): T {
  el.style.left = `${x}px`;
  el.style.top = `${y}px`;
  el.style.width = `${w}px`;
  el.style.height = `${h}px`;
  panel.append(el);
  return el;
}

function labelEl(text: string): HTMLSpanElement {
  const l = document.createElement("span");
  l.className = "display-label";
  l.textContent = text;
  l.title = text;
  return l;
}

export function button(text: string): HTMLButtonElement {
  const b = document.createElement("button");
  b.textContent = text;
  return b;
}

/** getRootPane().setDefaultButton: Enter anywhere in the dialog presses it. */
function defaultButton(body: HTMLElement, action: () => void): void {
  body.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      action();
    }
  });
}

/** 手数 > 自定义 (MovenumberDialog, frame 172x105): an empty field; 确定 with 0 or less (or blank)
 *  turns the field red with 无效的数字 and keeps the dialog; otherwise the last n numbers show. */
export function openMoveNumberDialog(): void {
  const panel = absolute(156, 66);
  place(panel, labelEl(t("MovenumberDialog.lblChangeTo")), 10, 10, 118, 20);
  const field = place(panel, document.createElement("input"), 100, 11, 43, 20);
  field.type = "text";
  field.setAttribute("aria-label", t("MovenumberDialog.lblChangeTo"));
  digitsOnly(field);
  const ok = place(panel, button(t("MovenumberDialog.okButton")), 35, 38, 83, 23);
  const dialog = formPanel(t("MovenumberDialog.title"), panel);
  const apply = () => {
    const n = fieldValue(field);
    if (n <= 0) {
      field.classList.add("invalid");
      field.title = t("LizzieChangeMove.txtMoveNumber.error");
      return;
    }
    void dialog.close();
    setDisplay({ moveNumbers: n });
  };
  ok.onclick = apply;
  defaultButton(panel, apply);
}

/** 推荐点 > 设置变化图回放间隔 (SetReplayTime, frame 357x130): milliseconds; blank reads as 0. */
export function openReplayIntervalDialog(): void {
  const s = state.view?.settings.display;
  if (!s) return;
  const panel = absolute(341, 91);
  place(panel, labelEl(t("SetReplayTime.lblInterval")), 92, 18, 104, 20);
  const field = place(panel, document.createElement("input"), 187, 19, 70, 19);
  field.type = "text";
  field.value = String(Math.trunc(s.replayIntervalSeconds * 1000));
  field.setAttribute("aria-label", t("SetReplayTime.lblInterval"));
  digitsOnly(field);
  const ok = place(panel, button(t("SetReplayTime.okButton")), 138, 50, 74, 29);
  const dialog = formPanel(t("SetReplayTime.title"), panel);
  const apply = () => {
    const ms = fieldValue(field);
    void dialog.close();
    setDisplay({ replayIntervalSeconds: ms / 1000 });
  };
  ok.onclick = apply;
  defaultButton(panel, apply);
}

/** DoubleDocument: the text must stay a (partial) decimal number. */
function decimalOnly(field: HTMLInputElement): void {
  field.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const start = field.selectionStart ?? field.value.length;
    const end = field.selectionEnd ?? start;
    const next = field.value.slice(0, start) + inserted + field.value.slice(end);
    if (!/^-?\d*\.?\d*$/.test(next)) e.preventDefault();
  });
}

/** 推荐点 > 延迟显示候选点 (SetDelayShowCandidates, packed): the check box enables the seconds field;
 *  应用 saves both (an unparsable field keeps the old seconds), then hides the candidates for the
 *  delay when on, or shows them when off. */
export function openCandidatesDelayDialog(): void {
  const s = state.view?.settings.display;
  if (!s) return;
  const body = document.createElement("div");
  body.className = "modal-body display-delay";
  const row = document.createElement("div");
  row.className = "display-delay-row";
  const check = document.createElement("input");
  check.type = "checkbox";
  check.checked = s.delayCandidates;
  const checkLabel = document.createElement("label");
  checkLabel.append(check, t("SetDelayShowCandidates.lblDelayCandidates"));
  const field = document.createElement("input");
  field.type = "text";
  field.size = 10;
  field.title = t("SetDelayShowCandidates.txtDelayCandidates.tooltips");
  field.value = javaDouble(s.delayCandidatesSeconds);
  field.disabled = !s.delayCandidates;
  decimalOnly(field);
  check.onchange = () => (field.disabled = !check.checked);
  row.append(checkLabel, field);
  const buttons = document.createElement("div");
  buttons.className = "display-buttons";
  const apply = button(t("SetDelayShowCandidates.apply"));
  const cancel = button(t("SetDelayShowCandidates.cancel"));
  buttons.append(apply, cancel);
  body.append(row, buttons);
  const dialog = formPanel(t("SetDelayShowCandidates.title"), body);
  cancel.onclick = () => void dialog.close();
  apply.onclick = () => {
    const parsed = Number(field.value.trim());
    const seconds = field.value.trim() !== "" && Number.isFinite(parsed) ? parsed : s.delayCandidatesSeconds;
    void dialog.close();
    setDisplay({ delayCandidates: check.checked, delayCandidatesSeconds: seconds });
    if (check.checked) delayCandidates(seconds);
    else setCandidates(true, true);
  };
}

// ------------------------------------------------------------------ 自定义信息顺序

const WINRATE_FONT = '"Lizzie Winrate", "DejaVu Sans", sans-serif';

interface Preview {
  winrate: number;
  playouts: number;
  score: number;
  /** [win rate, playouts, score] rows. */
  order: [number, number, number];
  show: [boolean, boolean, boolean];
}

/** The font at the largest size within maxH whose text fits maxW (Java's derived 100 pt font). */
function fit(g: CanvasRenderingContext2D, s: string, family: string, maxH: number, maxW: number): number {
  g.font = `100px ${family}`;
  const size = Math.min(maxH, (100 * maxW) / Math.max(1, g.measureText(s).width));
  g.font = `${size}px ${family}`;
  return size;
}

/** Java ascent - descent of the current font. */
function textHeight(g: CanvasRenderingContext2D): number {
  const m = g.measureText("0");
  return Math.trunc(m.fontBoundingBoxAscent - m.fontBoundingBoxDescent);
}

/** drawStringFor3row: centred on x, baseline y. */
function row3(g: CanvasRenderingContext2D, s: string, x: number, y: number, family: string, maxH: number, maxW: number): void {
  fit(g, s, family, maxH, maxW);
  g.fillText(s, x - Math.trunc(g.measureText(s).width / 2), y);
}

/** drawString(..., aboveOrBelow): 1 baseline at y, 0 centred on y. */
function centred(g: CanvasRenderingContext2D, s: string, x: number, y: number, family: string, maxH: number, maxW: number, aboveOrBelow = 0): void {
  fit(g, s, family, maxH, maxW);
  const h = textHeight(g);
  const offset = aboveOrBelow === 1 ? -Math.trunc(h / 2) : aboveOrBelow === -1 ? Math.trunc(h / 2) : 0;
  g.fillText(s, x - Math.trunc(g.measureText(s).width / 2), y + Math.trunc(h / 2) + offset);
}

function disc(g: CanvasRenderingContext2D, x: number, y: number, r: number, extra = 0): void {
  g.beginPath();
  g.arc(x + 0.5, y + 0.5, r + 0.5 + extra, 0, Math.PI * 2);
  g.fill();
}

function ring(g: CanvasRenderingContext2D, x: number, y: number, r: number, f: number): void {
  g.lineWidth = r / f;
  g.beginPath();
  g.arc(x, y, r, 0, Math.PI * 2);
  g.stroke();
}

/** Utils.convertScoreToString(score, 5.0) for the preview position: 目差显示为差值 uses the
 *  sample's own lead, otherwise the core's projection of the sample (shown side, 盘面). */
function scoreText(score: number): string {
  const v = state.view;
  const tenth = (x: number) => javaDouble(Math.round(x * 10) / 10);
  if (v?.settings.display.scoreAsDiff) {
    const diff = score - 5;
    return diff > 0 ? `+${tenth(diff)}` : diff < 0 ? `-${tenth(-diff)}` : tenth(diff);
  }
  return tenth(v?.board.info_order_sample.score_lead ?? score);
}

/** SuggestionInfoOrderSettings.drawMove: stoneRadius 44, squareWidth 91. */
function drawSample(g: CanvasRenderingContext2D, p: Preview, x: number, y: number, color: string, order: number, kata: boolean): void {
  const d = state.view?.settings.display;
  const r = 44, sq = 91;
  const best = order === 0;
  const badge = (dx: number) => {
    const label = String(order + 1);
    fit(g, label, WINRATE_FONT, sq * 0.36, sq * 0.39);
    const h = textHeight(g);
    const w = Math.round(g.measureText(label).width);
    const left = Math.round(x + sq * 0.43) + dx - Math.trunc(w / 2);
    const base = Math.round(y - sq * 0.358) - dx;
    g.fillStyle = "#ffc800";
    g.fillRect(left, base - h, w, h + Math.max(1, Math.trunc(h / 12)));
    g.fillStyle = "#000";
    g.fillText(label, left, base);
  };
  const showOrder = d?.suggestionOrder ?? true;
  if (showOrder && best) badge(1);
  g.fillStyle = "#000";
  disc(g, x, y, r);
  g.strokeStyle = "#808080";
  ring(g, x, y, r + 1, 26.5);
  g.fillStyle = color;
  disc(g, x, y, r, best ? 1 : 0);
  if (best) {
    // show-blue-ring, else the grey ring again (SuggestionInfoOrderSettings.java:612-620).
    g.strokeStyle = d?.blueRing ?? true ? "#00f" : "#808080";
    if (d?.blueRing ?? true) ring(g, x, y, r + 2, 15);
    else ring(g, x, y, r + 1, 26.5);
  }
  if (showOrder && order > 0 && order < 9) badge(0);

  const maxRed = d?.suggestionMaxRed ?? false;
  const maxWinrate = maxRed && best, maxPlayouts = maxRed && p.playouts === 1300, maxScore = maxRed && kata && p.score === 5.7;
  const ink = (max: boolean) => (g.fillStyle = max ? "#f00" : "#000");
  const rounded = Math.round(p.winrate * 10) / 10;
  const winrate = rounded.toFixed(1), visits = playouts(p.playouts), score = scoreText(p.score);
  const [showW, showP] = p.show;
  const showS = kata && p.show[2];
  const [ow, op, os] = p.order;
  const defaultOrder = ow === 1 && op === 2 && os === 3;
  if (showW && showP && showS) {
    if (defaultOrder) {
      ink(maxWinrate);
      row3(g, winrate, x, y - Math.round(sq * (rounded < 10 ? 0.127 : 0.125)), WINRATE_FONT, sq * (rounded < 10 ? 0.36 : 0.35), sq * 0.67);
      ink(maxPlayouts);
      row3(g, visits, x, y + Math.round(sq * 0.18), GRAPH_FONT, sq * 0.34, r * (p.playouts >= 1000 ? 1.8 : 1.3));
      ink(maxScore);
      row3(g, score, x, y + Math.round(sq * 0.435), WINRATE_FONT, (1639 * 0.273) / 18, r * 1.6);
    } else {
      // getSuggestionInfoRow1..3
      const items: [string, boolean, number][] = [[winrate, maxWinrate, 0], [visits, maxPlayouts, 1], [score, maxScore, 2]];
      const rowItem = (n: number, prefer: number[]) => items[prefer.find((k) => p.order[k] === n) ?? prefer[0]];
      const rows = [rowItem(1, [0, 1, 2]), rowItem(2, [1, 0, 2]), rowItem(3, [2, 0, 1])];
      const ys = [-Math.round(sq * 0.125), Math.round(sq * 0.18), Math.round(sq * 0.435)];
      const limits: [number, number][] = [[sq * 0.35, sq * 0.67], [sq * 0.32, r * 1.8], [(1639 * 0.273) / 18, r * 1.6]];
      rows.forEach(([text, max], k) => {
        ink(max);
        row3(g, text, x, y + ys[k], op === k + 1 ? GRAPH_FONT : WINRATE_FONT, limits[k][0], limits[k][1]);
      });
    }
  } else if (showW && (showP || showS)) {
    const [second, secondMax, secondFont, secondOrder]: [string, boolean, string, number] = showP ? [visits, maxPlayouts, GRAPH_FONT, op] : [score, maxScore, WINRATE_FONT, os];
    if (defaultOrder || ow < secondOrder) {
      ink(maxWinrate);
      if (rounded < 10) centred(g, winrate, x, y - Math.trunc(sq / 15), WINRATE_FONT, r, sq * 0.57, 1);
      else centred(g, winrate, x, y - Math.trunc(sq / 16), WINRATE_FONT, r, sq * 0.735, 1);
      ink(secondMax);
      if (showP) centred(g, second, x, y + Math.trunc((r * 15) / 35), secondFont, r * 0.8, r * 1.4);
      else centred(g, second, x, y + Math.trunc((r * 4) / 9), secondFont, r * 0.75, r * 1.6);
    } else {
      ink(maxWinrate);
      centred(g, winrate, x, y + Math.trunc((r * 15) / 35), WINRATE_FONT, showP ? r * 0.77 : r * 0.75, showP ? r * 1.8 : r * 1.77);
      ink(secondMax);
      if (showP) centred(g, second, x, y - Math.trunc(sq / 15), secondFont, sq * 0.4, sq * 0.97, 1);
      else centred(g, second, x, y - Math.trunc(sq / 16), secondFont, r * 0.88, sq * 0.735, 1);
    }
  } else if (showP && showS) {
    if (defaultOrder || op < os) {
      ink(maxPlayouts);
      centred(g, visits, x, y - Math.trunc(r / 15), GRAPH_FONT, r * 0.82, r * 1.73, 1);
      ink(maxScore);
      centred(g, score, x, y + Math.trunc((r * 4) / 9), WINRATE_FONT, r * 0.75, r * 1.6);
    } else {
      ink(maxPlayouts);
      centred(g, visits, x, y + Math.trunc((r * 15) / 35), GRAPH_FONT, r * 0.8, r * 1.4);
      ink(maxScore);
      centred(g, score, x, y - Math.trunc(sq / 16), WINRATE_FONT, r * 0.88, sq * 0.735, 1);
    }
  } else if (showW) {
    ink(maxWinrate);
    centred(g, winrate, x, y, WINRATE_FONT, sq * 0.46, r * 1.9);
  } else if (showP) {
    ink(maxPlayouts);
    centred(g, visits, x, y, GRAPH_FONT, r, r * 1.9);
  } else if (showS) {
    ink(maxScore);
    centred(g, score, x, y, WINRATE_FONT, r, r * 1.7);
  }
}

/** paintMianPanel: board texture, 预览 / KataGo / Leela titles and the two sample candidates. */
function paintPreview(canvas: HTMLCanvasElement, order: [number, number, number], show: [boolean, boolean, boolean]): void {
  const dpr = window.devicePixelRatio || 1;
  const w = 480, h = 170;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  const board = images["board.png"];
  const pattern = board?.naturalWidth ? g.createPattern(board, "repeat") : null;
  g.fillStyle = pattern ?? "rgb(217,152,77)";
  g.fillRect(0, 0, w, h);
  g.fillStyle = "#000";
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.font = `bold 27px ${GRAPH_FONT}`;
  g.fillText(t("SuggestionInfoOrderSettings.preview"), isChinese() ? 20 : 5, 110);
  g.fillText("KataGo", 134, 30);
  g.fillText("Leela", 315, 30);
  drawSample(g, { winrate: 68.5, playouts: 1300, score: 5.7, order, show }, 180, 100, "rgb(0,217,240)", 0, true);
  drawSample(g, { winrate: 46.5, playouts: 730, score: 0, order, show }, 350, 100, "rgba(11,227,0,0.929)", 1, false);
}

/** 推荐点 > 自定义信息顺序 (SuggestionInfoOrderSettings, frame 480x366, not resizable): a live
 *  preview, the three shown-item check boxes and one radio per row and item. 确定 saves the check
 *  boxes, then the order only when every row has a different item (otherwise wrongSettingsHint and
 *  the dialog stays). The preview keeps the last valid order. */
export function openInfoOrderDialog(): void {
  const s = state.view?.settings.display;
  if (!s) return;
  const panel = absolute(480, 327);
  const canvas = place(panel, document.createElement("canvas"), 0, 0, 480, 170);
  canvas.className = "display-preview";
  place(panel, labelEl(t("SuggestionInfoOrderSettings.lblSuggestionInfo")), 10, 177, 124, 25);
  const columns: [string, number, number][] = [
    [t("SuggestionInfoOrderSettings.rdoWinrate"), 114, 103],
    [t("SuggestionInfoOrderSettings.rdoPlayouts"), 230, 113],
    [t("SuggestionInfoOrderSettings.rdoScoreLead"), 360, 121],
  ];
  const control = (type: "checkbox" | "radio", text: string, x: number, y: number, w: number, name = "") => {
    const label = place(panel, document.createElement("label"), x, y, w, 23);
    label.className = "display-choice";
    const input = document.createElement("input");
    input.type = type;
    if (name) input.name = name;
    label.append(input, text);
    return input;
  };
  const checks = columns.map(([text, x, w]) => control("checkbox", text, x, 178, w));
  [s.suggestionWinrate, s.suggestionPlayouts, s.suggestionScore].forEach((v, k) => (checks[k].checked = v));
  const rowLabels = ["SuggestionInfoOrderSettings.lblFirstRow", "SuggestionInfoOrderSettings.lblSecondRow", "SuggestionInfoOrderSettings.lblThirdRow"];
  rowLabels.forEach((key, row) => place(panel, labelEl(t(key)), 10, 205 + row * 30, 124, 25));
  // radios[item][row]; each row is a ButtonGroup.
  const radios = columns.map(([text, x, w], item) =>
    [0, 1, 2].map((row) => {
      const radio = control("radio", text, x, 208 + row * 30, w, `infoRow${row}`);
      radio.checked = s.suggestionInfoOrder[item] === row + 1;
      return radio;
    }),
  );
  const ok = place(panel, button(t("SuggestionInfoOrderSettings.okButton")), 150, 301, 93, 25);
  const cancel = place(panel, button(t("SuggestionInfoOrderSettings.cancelButton")), 250, 301, 93, 25);
  const chosen = (): [number, number, number] => radios.map((rows) => rows.findIndex((r) => r.checked) + 1) as [number, number, number];
  const valid = ([w, p, sc]: number[]) => w > 0 && p > 0 && sc > 0 && w !== p && p !== sc && sc !== w;
  let previewOrder: [number, number, number] = s.suggestionInfoOrder;
  const repaint = () => {
    const order = chosen();
    if (valid(order)) previewOrder = order;
    paintPreview(canvas, previewOrder, checks.map((c) => c.checked) as [boolean, boolean, boolean]);
  };
  panel.addEventListener("change", repaint);
  repaint();
  const dialog = formPanel(t("SuggestionInfoOrderSettings.title"), panel);
  cancel.onclick = () => void dialog.close();
  ok.onclick = async () => {
    const shows = { suggestionWinrate: checks[0].checked, suggestionPlayouts: checks[1].checked, suggestionScore: checks[2].checked };
    const order = chosen();
    if (!valid(order)) {
      setDisplay(shows);
      await message(t("Message.title"), t("SuggestionInfoOrderSettings.wrongSettingsHint"));
      return;
    }
    await dialog.close();
    setDisplay({ ...shows, suggestionInfoOrder: order });
  };
}
