// 超级鹰眼's 统计信息 page (MoveListFrame.statisticsPanel → statisticsGraph, GridLayout(1, 2),
// MoveListFrame.java:371-437): the left keyPanel (drawKeyPanel, 6391-6728) with the 吻合度 / 吻合率
// tooltips, the right lossPanel with its 目数损失 / 胜率损失 tabs (drawLossPanel 6003-6379). Both
// halves paint the core's aggregates of the 统计 range (pane.sides, pane.*_buckets); Java counts
// them while painting. The selected loss tab is the only input (loss-panel-select-winrate). The
// painters take the window's tokens (hawkeye.css, stage.css) and repaint when the colour mode
// changes.
import type { Command } from "../contract/generated";
import { namedFace } from "../analysis/suggestionsFrame";
import { onThemeChange, token } from "../design/theme";
import { t } from "../i18n";
import { tabList, typeSizes } from "./charts";
import { controlTargets, type ControlTarget } from "./dialogs";
import { type HawkeyePayload, fixed, settingsCommand } from "./model";

const i = Math.trunc;

/** The colours and face the page paints with. */
interface Look {
  ground: string;
  track: string;
  trackEdge: string;
  text: string;
  label: string;
  barEdge: string;
  stoneBlack: string;
  stoneBlackEdge: string;
  stoneWhite: string;
  stoneWhiteEdge: string;
  onBlack: string;
  onWhite: string;
  loss: string[];
  face: string;
  sizes: number[];
}

function look(p: HawkeyePayload): Look {
  const s = typeSizes();
  return {
    ground: token("--ds-panel"),
    track: token("--ds-sunken"),
    trackEdge: token("--ds-line"),
    text: token("--ds-text"),
    label: token("--ds-text-2"),
    barEdge: token("--ds-line-strong"),
    stoneBlack: token("--ws-stone-black"),
    stoneBlackEdge: token("--ws-stone-black-edge"),
    stoneWhite: token("--ws-stone-white"),
    stoneWhiteEdge: token("--ws-stone-white-edge"),
    onBlack: token("--ws-on-black"),
    onWhite: token("--ws-on-white"),
    loss: [0, 1, 2, 3, 4, 5].map((k) => token(`--hk-loss-${k}`)),
    // LizzieFrame.uiFont: the theme's when it names one.
    face: namedFace(p.fonts.ui) ?? token("--ds-font"),
    sizes: [s.xl, s.l, s.m, s.s],
  };
}

const font = (lk: Look, px: number, bold = false) => `${bold ? "600 " : ""}${px}px ${lk.face}`;
const stringWidth = (g: CanvasRenderingContext2D, s: string) => Math.round(g.measureText(s).width);

/** The visible height of the digits in the current font, for vertical centring. */
function digitHeight(g: CanvasRenderingContext2D): number {
  const m = g.measureText("0");
  return Math.round(m.actualBoundingBoxAscent);
}

/** MoveListFrame.drawStringMid (6785-6815): `s` centred in the box whose top-left is (x, y), at the
 *  largest size of the type scale that fits the box (the smallest when none does). Returns the
 *  size (the caller reuses it for the bar values). */
function stringMid(g: CanvasRenderingContext2D, lk: Look, x: number, y: number, s: string, maxW: number, maxH: number, bold = false): number {
  let px = lk.sizes[lk.sizes.length - 1];
  for (const size of lk.sizes) {
    g.font = font(lk, size, bold);
    if (stringWidth(g, s) <= maxW && size <= maxH) {
      px = size;
      break;
    }
  }
  g.font = font(lk, px, bold);
  g.fillText(s, x + i((maxW - stringWidth(g, s)) / 2), i(y + maxH / 2 + digitHeight(g) / 2));
  return px;
}

/** A player's name after their flat stone, centred in its box. */
function nameMid(g: CanvasRenderingContext2D, lk: Look, x: number, y: number, name: string, maxW: number, maxH: number, black: boolean): void {
  const dot = 10;
  let px = lk.sizes[lk.sizes.length - 1];
  for (const size of lk.sizes) {
    g.font = font(lk, size, true);
    if (stringWidth(g, name) + dot + 6 <= maxW && size <= maxH) {
      px = size;
      break;
    }
  }
  g.font = font(lk, px, true);
  const width = stringWidth(g, name) + dot + 6;
  const left = x + i((maxW - width) / 2);
  const mid = y + maxH / 2;
  g.beginPath();
  g.arc(left + dot / 2, mid, dot / 2 - 0.5, 0, Math.PI * 2);
  g.fillStyle = black ? lk.stoneBlack : lk.stoneWhite;
  g.fill();
  g.strokeStyle = black ? lk.stoneBlackEdge : lk.stoneWhiteEdge;
  g.lineWidth = 1;
  g.stroke();
  g.fillStyle = lk.text;
  g.fillText(name, left + dot + 6, i(mid + digitHeight(g) / 2));
}

/** How a bar is drawn: its fill inside a 1 px edge, the value's colour beside and on it. */
interface BarLook {
  fill: string;
  edge: string;
  outside: string;
  inside: string;
}

/** MoveListFrame.drawRectWithValue (6730-6783): nothing for NaN, <= 0 or > 1; left bars grow
 *  leftwards from the right edge. The value is written outside the bar while the free part is
 *  wider than 1.2 × the text, else inside it. */
function rectWithValue(
  g: CanvasRenderingContext2D,
  lk: Look,
  x: number,
  y: number,
  width: number,
  height: number,
  value: number,
  text: string,
  bar: BarLook,
  px: number,
  left: boolean,
): void {
  if (Number.isNaN(value) || value <= 0 || value > 1) return;
  const bx = left ? i(x + width * (1 - value)) : x;
  const by = y + i(height / 6);
  const bw = i(width * value) + 1;
  const bh = i((height * 2) / 3);
  g.fillStyle = bar.fill;
  g.fillRect(bx, by, bw, bh);
  g.strokeStyle = bar.edge;
  g.lineWidth = 1;
  g.strokeRect(bx + 0.5, by + 0.5, bw - 1, bh - 1);
  g.font = font(lk, px, true);
  const length = stringWidth(g, text);
  const baseline = y + i(height / 2) + i(digitHeight(g) / 2);
  const out = width * (1 - value) > length * 1.2;
  g.fillStyle = out ? bar.outside : bar.inside;
  if (left) g.fillText(text, out ? i(x + width * (1 - value) - length * 1.1) : i(x + width * (1 - value) + length * 0.1), baseline);
  else g.fillText(text, out ? i(x + width * value + length * 0.1) : i(x + width * value - length * 1.1), baseline);
}

/** A value column's track: the sunken ground inside a hairline. */
function track(g: CanvasRenderingContext2D, lk: Look, x: number, y: number, w: number, h: number): void {
  g.fillStyle = lk.track;
  g.fillRect(x, y, w, h);
  g.strokeStyle = lk.trackEdge;
  g.lineWidth = 1;
  g.strokeRect(x - 0.5, y - 0.5, w + 1, h + 1);
}

interface TipBand {
  y: number;
  h: number;
}

/** drawKeyPanel (MoveListFrame.java:6391-6728). Returns the two tooltip bands
 *  (keyPanel_tooltip1/2.setBounds(0, y1 [+ row], width, stringH)). */
function paintKey(g: CanvasRenderingContext2D, lk: Look, width: number, height: number, p: HawkeyePayload): [TipBand, TipBand] {
  g.fillStyle = lk.ground;
  g.fillRect(0, 0, width, height);
  const border = 0.05;
  const aw = width * (1 - border);
  const ah = height * (1 - border);
  const startX = i((width * border) / 2);
  const startY = i((height * border) / 2);
  const titleHeight = ah / 10;
  g.textBaseline = "alphabetic";
  // Lizzie.frame.getPlayerName(isBlack, 12), each after its stone.
  nameMid(g, lk, startX, startY, p.blackName, i(aw * 0.4), i(titleHeight), true);
  nameMid(g, lk, startX + i(aw * 0.6), startY, p.whiteName, i(aw * 0.4), i(titleHeight), false);

  const heightGap = ah * 0.03;
  const valueStartY = i(startY + titleHeight + heightGap);
  const valueWidth = i(aw * 0.4);
  const valueHeight = i(ah - titleHeight - heightGap);
  const xBlack = startX;
  const xWhite = startX + i(aw * 0.6);
  track(g, lk, xBlack, valueStartY, valueWidth, valueHeight);
  track(g, lk, xWhite, valueStartY, valueWidth, valueHeight);

  const row = i((ah - titleHeight - heightGap) / 5);
  const x1 = i(startX + valueWidth + aw * 0.2 * 0.05);
  const y1 = valueStartY + i(row / 4);
  const stringW = aw * 0.2 * 0.9;
  const stringH = i(row / 2);
  g.fillStyle = lk.label;
  const px = stringMid(g, lk, x1, y1, t("Movelistframe.keyPanel.accuracy"), stringW, stringH);
  stringMid(g, lk, x1, y1 + row, t("Movelistframe.keyPanel.match"), stringW, stringH);
  stringMid(g, lk, x1, y1 + row * 2, t("Movelistframe.keyPanel.matchBestMove"), stringW, stringH);
  stringMid(g, lk, x1, y1 + row * 3, t("Movelistframe.keyPanel.avgScoreLoss"), stringW, stringH);
  stringMid(g, lk, x1, y1 + row * 4, t("Movelistframe.keyPanel.avgWinLoss"), stringW, stringH);

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
    // Each side's bars in its stone colour, the value on the bar in the stone's text colour.
    const sideBar: BarLook = black
      ? { fill: lk.stoneBlack, edge: lk.stoneBlackEdge, outside: lk.text, inside: lk.onBlack }
      : { fill: lk.stoneWhite, edge: lk.stoneWhiteEdge, outside: lk.text, inside: lk.onWhite };
    const bar = (k: number, value: number, text: string) =>
      rectWithValue(g, lk, x, valueStartY + row * k, valueWidth, row, value, text, sideBar, px, black);
    bar(0, v.accuracy, fixed(v.accuracy * 100, 1));
    bar(1, v.match, `${fixed(v.match * 100, 1)}%`);
    bar(2, v.best, `${fixed(v.best * 100, 1)}%`);
    bar(3, v.scoreLoss / maxAvgWinLoss, fixed(v.scoreLoss, 1));
    bar(4, v.winLoss / maxAvgWinLoss, `${fixed(v.winLoss, 1)}%`);
  });
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
 *  bar growing left, the threshold label and the white bar, each in its loss class's colour. */
function paintLoss(g: CanvasRenderingContext2D, lk: Look, width: number, height: number, p: HawkeyePayload, winrate: boolean): void {
  g.fillStyle = lk.ground;
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
  // The analysed moves of each side on its stone colour.
  const half = i(aw / 2);
  g.fillStyle = lk.stoneBlack;
  g.fillRect(startX, startY, half, i(titleHeight));
  g.strokeStyle = lk.stoneBlackEdge;
  g.lineWidth = 1;
  g.strokeRect(startX + 0.5, startY + 0.5, half - 1, i(titleHeight) - 1);
  g.fillStyle = lk.onBlack;
  stringMid(g, lk, startX, startY + i(titleHeight * 0.1), String(totals[0]), half, i(titleHeight * 0.8), true);
  g.fillStyle = lk.stoneWhite;
  g.fillRect(startX + half, startY, half, i(titleHeight));
  g.strokeStyle = lk.stoneWhiteEdge;
  g.lineWidth = 1;
  g.strokeRect(startX + half + 0.5, startY + 0.5, half - 1, i(titleHeight) - 1);
  g.fillStyle = lk.onWhite;
  stringMid(g, lk, startX + half, startY + i(titleHeight * 0.1), String(totals[1]), half, i(titleHeight * 0.8), true);

  const heightGap = ah * 0.02;
  const valueStartY = i(startY + titleHeight + heightGap);
  const valueWidth = i(aw * 0.43);
  const valueHeight = i(ah - titleHeight - heightGap);
  const xBlack = startX;
  const xWhite = startX + i(aw * 0.57);
  track(g, lk, xBlack, valueStartY, valueWidth, valueHeight);
  track(g, lk, xWhite, valueStartY, valueWidth, valueHeight);

  const row = i((ah - titleHeight - heightGap) / 6);
  const labelX = i(startX + valueWidth + aw * 0.14 * 0.03);
  g.fillStyle = lk.label;
  let px = 0;
  lossLabels(winrate ? p.winLoss : p.scoreLoss, winrate).forEach((label, k) => {
    const s = stringMid(g, lk, labelX, valueStartY + row * k + i(row / 4), label, aw * 0.14 * 0.94, i(row / 2));
    if (k === 0) px = s;
  });
  for (let k = 0; k < 6; k++) {
    const y = valueStartY + row * k;
    const bar: BarLook = { fill: lk.loss[k], edge: lk.barEdge, outside: lk.text, inside: lk.text };
    rectWithValue(g, lk, xBlack, y, valueWidth, row, buckets[0][k] / totals[0], String(buckets[0][k]), bar, px, true);
    rectWithValue(g, lk, xWhite, y, valueWidth, row, buckets[1][k] / totals[1], String(buckets[1][k]), bar, px, false);
  }
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
    const tab = document.createElement("button");
    tab.className = "hks-tab";
    tab.dataset.key = `hawkeye.stats.loss.${k}`;
    tab.textContent = text;
    return tab;
  });
  strip.append(...tabs);
  const markTab = tabList(strip, tabs, (k) => select(k === 1));
  const body = document.createElement("div");
  body.className = "hks-loss-body";
  const lossCanvas = document.createElement("canvas");
  body.append(lossCanvas);
  loss.append(strip, body);
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
    markTab(winrate ? 1 : 0);
    const lk = look(data);
    const k = prepareCanvas(keyCanvas);
    if (k) {
      const bands = paintKey(k.g, lk, k.w, k.h, data);
      bands.forEach((band, n) => {
        const el = tips[n].el;
        el.hidden = false;
        el.style.top = `${band.y}px`;
        el.style.width = `${k.w}px`;
        el.style.height = `${band.h}px`;
      });
    }
    const l = prepareCanvas(lossCanvas);
    if (l) paintLoss(l.g, lk, l.w, l.h, data, winrate);
  };
  onThemeChange(paint);

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
