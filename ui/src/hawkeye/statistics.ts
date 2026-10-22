// 超级鹰眼's 统计信息 page (MoveListFrame.statisticsPanel → statisticsGraph, GridLayout(1, 2),
// MoveListFrame.java:371-437): the left keyPanel (drawKeyPanel, 6391-6728) with the 吻合度 / 吻合率
// tooltips, the right lossPanel JTabbedPane (目数损失 / 胜率损失, drawLossPanel 6003-6379). Both
// halves paint the core's aggregates of the 统计 range (pane.sides, pane.*_buckets); Java counts
// them while painting. The selected loss tab is the only input (loss-panel-select-winrate).
import type { Command } from "../contract/generated";
import { t } from "../i18n";
import { GRAPH_FONT } from "../workbench/paint";
import { controlTargets, type ControlTarget } from "./dialogs";
import { type HawkeyePayload, fixed, LOSS_COLORS, settingsCommand, UI_FONT } from "./model";

const STYLE_ID = "hawkeye-statistics-style";

/** The Metal JTabbedPane of the loss panel (lossPanel.setFont(frameFontSize 12)), measured on the
 *  Java screenshots: 2 px above the tabs, 18 px tabs with a 6 px slanted top-left corner, the
 *  selected tab opening into the 4/2/3/3 px content border. */
const STYLE = `
.hks { display: grid; width: 100%; height: 100%; overflow: hidden; font-family: ${UI_FONT}; }
.hks-key { position: relative; overflow: hidden; background: rgb(190, 190, 190); }
.hks-key canvas, .hks-loss-frame canvas { position: absolute; left: 0; top: 0; }
.hks-tip { position: absolute; left: 0; }
.hks-loss { display: flex; flex-direction: column; min-width: 0; background: rgb(246, 247, 249); }
.hks-tabs { flex: none; display: flex; align-items: flex-end; height: 20px; padding: 2px 0 0 2px; box-sizing: border-box; }
.hks-tab { position: relative; height: 18px; padding: 1px 1px 0; box-sizing: border-box; cursor: default; user-select: none;
  background: rgb(122, 138, 153); clip-path: polygon(6px 0, 100% 0, 100% 100%, 0 100%, 0 6px); }
.hks-tab span { display: flex; align-items: center; height: 100%; padding: 0 11px; font-size: 12px; white-space: nowrap;
  color: rgb(51, 51, 51); background: rgb(238, 238, 238); clip-path: polygon(5px 0, 100% 0, 100% 100%, 0 100%, 0 5px); }
.hks-tab.on { z-index: 1; height: 20px; margin-bottom: -2px; background: rgb(99, 130, 191); }
.hks-tab.on span { color: #000; background: rgb(200, 221, 242); box-shadow: inset 1px 1px #fff; }
.hks-loss-outer { flex: 1; min-height: 0; display: flex; border-style: solid; border-color: rgb(122, 138, 153); border-width: 0 1px 1px 0; }
.hks-loss-body { position: relative; flex: 1; min-width: 0; overflow: hidden; background: #fff;
  border-style: solid; border-width: 1px 0 0 1px; border-color: rgb(99, 130, 191); }
.hks-loss-frame { position: absolute; inset: 1px 0 0 0; overflow: hidden;
  border-style: solid; border-color: rgb(200, 221, 242); border-width: 2px 2px 2px 1px; }
`;

function injectStyle(): void {
  if (document.getElementById(STYLE_ID)) return;
  const style = document.createElement("style");
  style.id = STYLE_ID;
  style.textContent = STYLE;
  document.head.append(style);
}

const i = Math.trunc;
/** Font.getSize() of a derived float font: (int) (size + 0.5). */
const size = (v: number) => i(v + 0.5);
/** Config.frameFontSize (12) * javaScaleFactor + 6: drawStringMid's size cap. */
const MAX_FONT = 18;
const PINK = "rgb(255,175,175)";
const BLUE = "rgb(0,0,255)";

/** Java FontMetrics ints (FontDesignMetrics rounds each value as (int)(0.95 + v)). */
function ascentMinusDescent(g: CanvasRenderingContext2D): number {
  const m = g.measureText("0");
  return i(0.95 + m.fontBoundingBoxAscent) - i(0.95 + m.fontBoundingBoxDescent);
}

const font = (px: number) => `${px}px ${GRAPH_FONT}`;
const stringWidth = (g: CanvasRenderingContext2D, s: string) => Math.round(g.measureText(s).width);

/** MoveListFrame.drawStringMid (6785-6815): LizzieFrame.uiFont derived at 100 (makeFont) is
 *  scaled to span maxW, capped at maxH and at frameFontSize + 6, and centred in the box whose
 *  top-left is (x, y). Returns the font size (the caller reuses it for the bar values). */
function stringMid(g: CanvasRenderingContext2D, x: number, y: number, s: string, maxW: number, maxH: number): number {
  g.font = font(100);
  let px = Math.min(maxH, size((100 * maxW) / stringWidth(g, s)));
  if (size(px) > MAX_FONT) px = MAX_FONT;
  g.font = font(px);
  const length = stringWidth(g, s);
  const height = ascentMinusDescent(g);
  g.fillText(s, x + i((maxW - length) / 2), i(y + maxH / 2 + i(height / 2)));
  return px;
}

/** MoveListFrame.drawRectWithValue (6730-6783): nothing for NaN, <= 0 or > 1; left bars grow
 *  leftwards from the right edge. The value is written outside the bar while the free part is
 *  wider than 1.2 × the text, else inside it in the inner colour. */
function rectWithValue(
  g: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  value: number,
  text: string,
  rect: string,
  outside: string,
  inside: string,
  px: number,
  left: boolean,
): void {
  if (Number.isNaN(value) || value <= 0 || value > 1) return;
  g.fillStyle = rect;
  if (left) g.fillRect(i(x + width * (1 - value)), y + i(height / 6), i(width * value) + 1, i((height * 2) / 3));
  else g.fillRect(x, y + i(height / 6), i(width * value) + 1, i((height * 2) / 3));
  g.font = font(px);
  const length = stringWidth(g, text);
  const baseline = y + i(height / 2) + i(ascentMinusDescent(g) / 2);
  const out = width * (1 - value) > length * 1.2;
  g.fillStyle = out ? outside : inside;
  if (left) g.fillText(text, out ? i(x + width * (1 - value) - length * 1.1) : i(x + width * (1 - value) + length * 0.1), baseline);
  else g.fillText(text, out ? i(x + width * value + length * 0.1) : i(x + width * value - length * 1.1), baseline);
}

/** g.drawRect(x - 1, y - 1, w + 1, h + 1): the 1 px frame around a value column. */
function frame(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number): void {
  g.strokeStyle = "#000";
  g.lineWidth = 1;
  g.strokeRect(x - 0.5, y - 0.5, w + 1, h + 1);
}

interface TipBand {
  y: number;
  h: number;
}

/** drawKeyPanel (MoveListFrame.java:6391-6728). Returns the two tooltip bands
 *  (keyPanel_tooltip1/2.setBounds(0, y1 [+ row], width, stringH)). */
function paintKey(g: CanvasRenderingContext2D, width: number, height: number, p: HawkeyePayload): [TipBand, TipBand] {
  g.fillStyle = "rgb(190,190,190)";
  g.fillRect(0, 0, width, height);
  const border = 0.05;
  const aw = width * (1 - border);
  const ah = height * (1 - border);
  const startX = i((width * border) / 2);
  const startY = i((height * border) / 2);
  const titleHeight = ah / 10;
  g.textBaseline = "alphabetic";
  // Lizzie.frame.getPlayerName(isBlack, 12).
  g.fillStyle = "#000";
  stringMid(g, startX, startY, p.blackName, i(aw * 0.4), i(titleHeight));
  g.fillStyle = "#fff";
  stringMid(g, startX + i(aw * 0.6), startY, p.whiteName, i(aw * 0.4), i(titleHeight));

  const heightGap = ah * 0.03;
  const valueStartY = i(startY + titleHeight + heightGap);
  const valueWidth = i(aw * 0.4);
  const valueHeight = i(ah - titleHeight - heightGap);
  const xBlack = startX;
  const xWhite = startX + i(aw * 0.6);
  g.fillStyle = "rgb(170,170,170)";
  g.fillRect(xBlack, valueStartY, valueWidth, valueHeight);
  g.fillRect(xWhite, valueStartY, valueWidth, valueHeight);

  const row = i((ah - titleHeight - heightGap) / 5);
  const x1 = i(startX + valueWidth + aw * 0.2 * 0.05);
  const y1 = valueStartY + i(row / 4);
  const stringW = aw * 0.2 * 0.9;
  const stringH = i(row / 2);
  g.fillStyle = "#000";
  const px = stringMid(g, x1, y1, t("Movelistframe.keyPanel.accuracy"), stringW, stringH);
  stringMid(g, x1, y1 + row, t("Movelistframe.keyPanel.match"), stringW, stringH);
  stringMid(g, x1, y1 + row * 2, t("Movelistframe.keyPanel.matchBestMove"), stringW, stringH);
  stringMid(g, x1, y1 + row * 3, t("Movelistframe.keyPanel.avgScoreLoss"), stringW, stringH);
  stringMid(g, x1, y1 + row * 4, t("Movelistframe.keyPanel.avgWinLoss"), stringW, stringH);

  // Per side over the 统计 range (6492-6575): 吻合度 / 吻合率 / 第一选点率 over the moves whose
  // position before is analysed, both average losses over the moves with both positions analysed.
  // A side without moves divides 0 by 0: NaN, no bar and no text (drawRectWithValue).
  const values = p.pane.sides.map((s) => ({
    accuracy: s.share_sum / s.match_analyzed,
    match: s.matched / s.match_analyzed,
    best: s.best / s.match_analyzed,
    winLoss: s.winrate_loss_sum / s.analyzed,
    scoreLoss: s.score_loss_sum / s.analyzed,
  }));
  // Comparisons with NaN are false, as in Java's `if (avg > max) max = avg`. Java computes a
  // maxAvgScoreLoss too (6581-6584) but scales both loss rows by maxAvgWinLoss (6633, 6703).
  let maxAvgWinLoss = 10;
  for (const v of values) {
    if (v.winLoss > maxAvgWinLoss) maxAvgWinLoss = v.winLoss;
  }
  values.forEach((v, side) => {
    const black = side === 0;
    const x = black ? xBlack : xWhite;
    const colour = black ? "#000" : "#fff";
    const inner = black ? PINK : BLUE;
    const bar = (k: number, value: number, text: string) =>
      rectWithValue(g, x, valueStartY + row * k, valueWidth, row, value, text, colour, colour, inner, px, black);
    bar(0, v.accuracy, fixed(v.accuracy * 100, 1));
    bar(1, v.match, `${fixed(v.match * 100, 1)}%`);
    bar(2, v.best, `${fixed(v.best * 100, 1)}%`);
    bar(3, v.scoreLoss / maxAvgWinLoss, fixed(v.scoreLoss, 1));
    bar(4, v.winLoss / maxAvgWinLoss, `${fixed(v.winLoss, 1)}%`);
  });
  frame(g, xBlack, valueStartY, valueWidth, valueHeight);
  frame(g, xWhite, valueStartY, valueWidth, valueHeight);
  return [
    { y: y1, h: stringH },
    { y: y1 + row, h: stringH },
  ];
}

/** Utils.getIfRound (Utils.java:494-497): whole numbers without a decimal part, others as
 *  Double.toString. */
function ifRound(v: number): string {
  if (v % 1 === 0) return String(i(v));
  return String(v);
}

/** The middle column of drawLossPanel (6129-6216) from the stored (negative) thresholds; Java
 *  prints `-threshold`. */
function lossLabels(thresholds: number[], winrate: boolean): string[] {
  const r = thresholds.map((v) => ifRound(-v));
  const unit = winrate ? "%" : "";
  return [
    `<${r[0]}${unit}`,
    `${r[0]}-${r[1]}${unit}`,
    `${r[1]}-${r[2]}${unit}`,
    `${r[2]}-${r[3]}${unit}`,
    `${r[3]}-${r[4]}${unit}`,
    `≥${r[4]}${unit}`,
  ];
}

/** drawLossPanel (MoveListFrame.java:6003-6379): analysed totals strip, then per row the black
 *  bar growing left, the threshold label and the white bar. */
function paintLoss(g: CanvasRenderingContext2D, width: number, height: number, p: HawkeyePayload, winrate: boolean): void {
  g.fillStyle = "rgb(200,200,200)";
  g.fillRect(0, 0, width, height);
  const buckets = winrate ? p.pane.winrate_buckets : p.pane.score_buckets;
  const totals = buckets.map((b) => b.reduce((s, n) => s + n, 0));

  const border = 0.03;
  const aw = width * (1 - border);
  const ah = height * (1 - border);
  const startX = i((width * border) / 2);
  const startY = i((height * border) / 2);
  const titleHeight = ah / 10;
  g.textBaseline = "alphabetic";
  g.fillStyle = "#000";
  g.fillRect(startX, startY, i(aw / 2), i(titleHeight));
  g.fillStyle = "#fff";
  stringMid(g, startX, startY + i(titleHeight * 0.1), String(totals[0]), i(aw / 2), i(titleHeight * 0.8));
  g.fillRect(startX + i(aw / 2), startY, i(aw / 2), i(titleHeight));
  g.fillStyle = "#000";
  stringMid(g, startX + i(aw / 2), startY + i(titleHeight * 0.1), String(totals[1]), i(aw / 2), i(titleHeight * 0.8));

  const heightGap = ah * 0.02;
  const valueStartY = i(startY + titleHeight + heightGap);
  const valueWidth = i(aw * 0.43);
  const valueHeight = i(ah - titleHeight - heightGap);
  const xBlack = startX;
  const xWhite = startX + i(aw * 0.57);
  g.fillStyle = "rgb(170,170,170)";
  g.fillRect(xBlack, valueStartY, valueWidth, valueHeight);
  g.fillRect(xWhite, valueStartY, valueWidth, valueHeight);

  const row = i((ah - titleHeight - heightGap) / 6);
  const labelX = i(startX + valueWidth + aw * 0.14 * 0.03);
  g.fillStyle = "#000";
  let px = 0;
  lossLabels(winrate ? p.winLoss : p.scoreLoss, winrate).forEach((label, k) => {
    const s = stringMid(g, labelX, valueStartY + row * k + i(row / 4), label, aw * 0.14 * 0.94, i(row / 2));
    if (k === 0) px = s;
  });
  for (let k = 0; k < 6; k++) {
    const y = valueStartY + row * k;
    // The black best-move row writes its inner text in Color(150, 100, 0), the others in PINK.
    rectWithValue(g, xBlack, y, valueWidth, row, buckets[0][k] / totals[0], String(buckets[0][k]),
      LOSS_COLORS[k], "#000", k === 0 ? "rgb(150,100,0)" : PINK, px, true);
    rectWithValue(g, xWhite, y, valueWidth, row, buckets[1][k] / totals[1], String(buckets[1][k]),
      LOSS_COLORS[k], "#fff", BLUE, px, false);
  }
  frame(g, xBlack, valueStartY, valueWidth, valueHeight);
  frame(g, xWhite, valueStartY, valueWidth, valueHeight);
}

/** Sizes the canvas to its parent in device pixels and returns a CSS-pixel context. */
function prepareCanvas(canvas: HTMLCanvasElement): { g: CanvasRenderingContext2D; w: number; h: number } | null {
  const parent = canvas.parentElement!;
  const w = parent.clientWidth;
  const h = parent.clientHeight;
  if (w <= 0 || h <= 0) return null;
  const dpr = window.devicePixelRatio || 1;
  canvas.width = Math.round(w * dpr);
  canvas.height = Math.round(h * dpr);
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { g, w, h };
}

export function mountStatistics(
  host: HTMLElement,
  actions: { run(command: Command): void },
): { render(p: HawkeyePayload): void; resize(): void; targets(): ControlTarget[] } {
  injectStyle();
  const root = document.createElement("div");
  root.className = "hks";

  const key = document.createElement("div");
  key.className = "hks-key";
  const keyCanvas = document.createElement("canvas");
  // keyPanel_tooltip1 / 2 (MoveListFrame.java:371-376): transparent hover bands over the 吻合度
  // and 吻合率 rows.
  const tips = [
    { el: document.createElement("div"), dataKey: "hawkeye.stats.tip.accuracy" },
    { el: document.createElement("div"), dataKey: "hawkeye.stats.tip.match" },
  ];
  tips[0].el.title = t("Movelistframe.keyPanel.accuracy.tip");
  tips[1].el.title = t("Movelistframe.keyPanel.match.tip");
  for (const tip of tips) {
    tip.el.className = "hks-tip";
    tip.el.dataset.key = tip.dataKey;
    tip.el.hidden = true;
  }
  key.append(keyCanvas, ...tips.map((tip) => tip.el));

  // lossPanel: tab 0 Movelistframe.scoreLoss, tab 1 Movelistframe.winLoss (417-418).
  const loss = document.createElement("div");
  loss.className = "hks-loss";
  const strip = document.createElement("div");
  strip.className = "hks-tabs";
  const tabs = [t("Movelistframe.scoreLoss"), t("Movelistframe.winLoss")].map((text, k) => {
    const tab = document.createElement("div");
    tab.className = "hks-tab";
    tab.dataset.key = `hawkeye.stats.loss.${k}`;
    const label = document.createElement("span");
    label.textContent = text;
    tab.append(label);
    tab.addEventListener("mousedown", (ev) => {
      if (ev.button !== 0) return;
      ev.preventDefault();
      select(k === 1);
    });
    return tab;
  });
  strip.append(...tabs);
  const outer = document.createElement("div");
  outer.className = "hks-loss-outer";
  const body = document.createElement("div");
  body.className = "hks-loss-body";
  const inner = document.createElement("div");
  inner.className = "hks-loss-frame";
  const lossCanvas = document.createElement("canvas");
  inner.append(lossCanvas);
  body.append(inner);
  outer.append(body);
  loss.append(strip, outer);
  root.append(key, loss);
  host.append(root);

  let data: HawkeyePayload | null = null;
  // Java selects 胜率损失 at construction without KataGo (421-422) and leaves the tabs to the
  // user afterwards. `pick` is the user's tab until the saved setting echoes it (with KataGo) or
  // KataGo changes; without KataGo it stays, since the default would force 胜率损失 again.
  let katago: boolean | null = null;
  let pick: boolean | null = null;

  const winrateSelected = (p: HawkeyePayload) => pick ?? (p.settings.lossWinrate || !p.pane.katago);

  function select(winrate: boolean) {
    if (!data) return;
    if (winrateSelected(data) === winrate) return;
    // stateChanged (423-433): loss-panel-select-winrate follows the selected tab.
    pick = winrate;
    actions.run(settingsCommand(data.settings, { lossWinrate: winrate }));
    paint();
  }

  // GridLayout(1, 2): both cells floor(width / 2), the odd pixel left over at the right.
  const layout = () => {
    const half = Math.floor(host.clientWidth / 2);
    root.style.gridTemplateColumns = `${half}px ${half}px`;
  };

  const paint = () => {
    if (!data) return;
    const winrate = winrateSelected(data);
    tabs.forEach((tab, k) => tab.classList.toggle("on", (k === 1) === winrate));
    const k = prepareCanvas(keyCanvas);
    if (k) {
      const bands = paintKey(k.g, k.w, k.h, data);
      bands.forEach((band, n) => {
        const el = tips[n].el;
        el.hidden = false;
        el.style.top = `${band.y}px`;
        el.style.width = `${k.w}px`;
        el.style.height = `${band.h}px`;
      });
    }
    const l = prepareCanvas(lossCanvas);
    if (l) paintLoss(l.g, l.w, l.h, data, winrate);
  };

  return {
    render(p: HawkeyePayload) {
      if (katago !== p.pane.katago) pick = null;
      katago = p.pane.katago;
      if (p.pane.katago && p.settings.lossWinrate === pick) pick = null;
      data = p;
      layout();
      paint();
    },
    resize() {
      layout();
      paint();
    },
    targets() {
      const winrate = data ? winrateSelected(data) : false;
      return controlTargets(root).map((target) =>
        target.key.startsWith("hawkeye.stats.loss.") ? { ...target, checked: (target.key.endsWith(".1")) === winrate } : target,
      );
    },
  };
}
