// 超级鹰眼 bottom panel (MoveListFrame.java `bottomPanel`): the nine chart tabs, each holding
// `matchPanel` (the chart, `draw`) and the 60 px `matchPanelmin` (`drawMin` and its controls;
// absent on the 翻车现场 tabs). Everything painted comes from the payload; the panel keeps only
// the mouse-over state and the text of its fields while they are edited. The painters take the
// window's tokens (stage.css graph and stone colours, hawkeye.css match areas and loss classes)
// and repaint when the colour mode changes.
import type { Command, DisplaySettings, HawkeyeStage, HistoryPoint, MoveEvaluation, Stone } from "../contract/generated";
import { namedFace } from "../analysis/suggestionsFrame";
import { icon } from "../design/icons";
import { onThemeChange, token } from "../design/theme";
import { t } from "../i18n";
import { fixed, settingsCommand, type HawkeyePayload } from "./model";

export interface ChartActions {
  /** Core commands: { type: "Hawkeye", op: { type: "GotoMoveNumber", document, number } } for curve press/drag, { type: "Hawkeye", op: { type: "GotoMove", document, node } } for a 翻车现场 slot (node = pane.moves[mistake.index].node), { type: "Navigate", nav: { op: "Back"|"Forward", count: 1 } } for the wheel, SetHawkeyeSettings via settingsCommand(...) for chart tab / detail toggle / diff ranges. */
  run(command: Command): void;
  /** 吻合率条件 fields: { matchAiMoves } / { matchAiPercentsPlayouts }. */
  display(change: Partial<DisplaySettings>): void;
  /** The settings (gear) button: opens SetMatchAiPara (implemented elsewhere). */
  openMatchHelp(): void;
}

export interface ChartPanel {
  /** Repaints for a new payload (the tab strip, min bar and chart). */
  render(p: HawkeyePayload): void;
  /** The host was resized. */
  resize(): void;
  /** Visible controls with client-area centres: { key, text, checked?, value?, x, y }. */
  targets(): { key: string; text: string; checked?: boolean; value?: string; x: number; y: number }[];
}

/** One entry of `ChartPanel.targets()`. */
interface Target {
  key: string;
  text: string;
  checked?: boolean;
  value?: string;
  x: number;
  y: number;
}

// ------------------------------------------------------------------ constants

/** Visual tab order → Java `selectedIndex` (MoveListFrame.java:666-684, 1141-1195). */
const TAB_CHARTS = [0, 4, 5, 6, 2, 3, 1, 7, 8];

function tabLabel(chart: number): string {
  switch (chart) {
    case 0:
      return t("Movelistframe.winrateMatch");
    case 1:
      return t("Movelistframe.accuracyLine");
    case 2:
      return t("Movelistframe.winrateDifference");
    case 3:
      return t("Movelistframe.scoreDifference");
    case 4:
      return t("Movelistframe.stageScore");
    case 5:
      return t("Movelistframe.winrateStatistics");
    case 6:
      return t("Movelistframe.scoreStatistics");
    case 7:
      return t("Movelistframe.winrateBigMistake");
    default:
      return t("Movelistframe.scoreBigMistake");
  }
}

/** matchPanelmin height (MoveListFrame.java:627, 2071). */
const MIN_HEIGHT = 60;
const DOT_RADIUS = 3;
const DASH = [4];
/** The width of the plotted lines. */
const SERIES = 1.5;

/** The type scale's sizes in px (--ds-size-s, --ds-size, --ds-size-l, --ds-size-xl) for canvas
 *  text; --ds-size follows 界面字体大小. */
export function typeSizes(): { s: number; m: number; l: number; xl: number } {
  const m = Number.parseFloat(token("--ds-size")) || 12;
  return { s: m - 1, m, l: m + 1, xl: m + 3 };
}

/** What the charts paint with, read from the tokens of the current colour mode. */
interface Palette {
  /** Plot ground, hairline grid, dashed midline and axis lines, axis numbers. */
  ground: string;
  grid: string;
  baseline: string;
  axis: string;
  text: string;
  label: string;
  /** The current move's guide, the hovered one's guide and dot. */
  guide: string;
  hover: string;
  /** 胜率 line inside the 统计 range, and outside it or across unanalysed moves. */
  line: string;
  lineOff: string;
  /** 目数 line and its value, the current move's loss. */
  score: string;
  scoreText: string;
  loss: string;
  stoneBlack: string;
  stoneBlackEdge: string;
  stoneWhite: string;
  stoneWhiteEdge: string;
  matchBlack: string;
  matchWhite: string;
  face: string;
  sizes: { s: number; m: number; l: number; xl: number };
}

let palette: Palette | null = null;
onThemeChange(() => (palette = null));

function currentPalette(): Palette {
  palette ??= {
    ground: token("--ds-panel"),
    grid: token("--ws-graph-grid"),
    baseline: token("--ws-graph-baseline"),
    axis: token("--ds-text-3"),
    text: token("--ds-text"),
    label: token("--ds-text-2"),
    guide: token("--ws-graph-guide"),
    hover: token("--ws-graph-hover"),
    line: token("--ws-winrate-line"),
    lineOff: token("--ds-text-3"),
    score: token("--ws-score-line"),
    scoreText: token("--ws-graph-score-text"),
    loss: token("--ws-loss"),
    stoneBlack: token("--ws-stone-black"),
    stoneBlackEdge: token("--ws-stone-black-edge"),
    stoneWhite: token("--ws-stone-white"),
    stoneWhiteEdge: token("--ws-stone-white-edge"),
    matchBlack: token("--hk-match-black"),
    matchWhite: token("--hk-match-white"),
    face: token("--ds-font"),
    sizes: typeSizes(),
  };
  return palette;
}

/** Java `(int)` of a double: NaN is 0, out of range saturates. */
function jint(v: number): number {
  if (Number.isNaN(v)) return 0;
  if (v >= 2147483647) return 2147483647;
  if (v <= -2147483648) return -2147483648;
  return Math.trunc(v);
}

/** Java int division (a zero divisor, which Java throws on, gives 0). */
function idiv(a: number, b: number): number {
  return b === 0 ? 0 : Math.trunc(a / b);
}

/** DecimalFormat("#0").parse / ("#0.00").parse: the leading number, the rest ignored; null when
 *  the text does not start with one (ParseException). */
function parseJava(text: string): number | null {
  const m = /^[-+]?(\d+(\.\d*)?|\.\d+)/.exec(text);
  return m ? Number(m[0]) : null;
}

// ------------------------------------------------------------------ Graphics2D on a canvas

/** The few Graphics2D operations MoveListFrame paints with, on Java's pixel grid, and the stone
 *  marks the charts draw each side with. */
class G {
  /** fonts: Lizzie.config.uiFontName / fontName when the theme names them, else the interface
   *  face. */
  readonly fonts: { ui: string; info: string };

  constructor(readonly c: CanvasRenderingContext2D, readonly p: Palette, fonts: { ui: string; info: string }) {
    c.textBaseline = "alphabetic";
    c.textAlign = "left";
    this.fonts = { ui: namedFace(fonts.ui) ?? p.face, info: namedFace(fonts.info) ?? p.face };
  }

  color(s: string): void {
    this.c.fillStyle = s;
    this.c.strokeStyle = s;
  }

  /** BasicStroke(width) (CAP_SQUARE), or the dashed CAP_BUTT strokes. */
  stroke(width: number, dash?: number[]): void {
    this.c.lineWidth = width;
    this.c.setLineDash(dash ?? []);
    this.c.lineCap = dash ? "butt" : "square";
  }

  /** A size of the type scale, at weight 600 or 400. */
  font(size: "s" | "m" | "l", bold = false, family: string = this.p.face): void {
    this.c.font = `${bold ? "600 " : ""}${this.p.sizes[size]}px ${family}`;
  }

  line(x1: number, y1: number, x2: number, y2: number): void {
    const o = Math.round(this.c.lineWidth) % 2 === 1 ? 0.5 : 0;
    this.c.beginPath();
    this.c.moveTo(x1 + o, y1 + o);
    this.c.lineTo(x2 + o, y2 + o);
    this.c.stroke();
  }

  /** fillRect: nothing for a non-positive size. */
  rect(x: number, y: number, w: number, h: number): void {
    if (w > 0 && h > 0) this.c.fillRect(x, y, w, h);
  }

  poly(xs: number[], ys: number[]): void {
    this.c.beginPath();
    this.c.moveTo(xs[0], ys[0]);
    for (let i = 1; i < xs.length; i++) this.c.lineTo(xs[i], ys[i]);
    this.c.closePath();
    this.c.fill("evenodd");
  }

  oval(x: number, y: number, w: number, h: number): void {
    if (w <= 0 || h <= 0) return;
    this.c.beginPath();
    this.c.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
    this.c.fill();
  }

  text(s: string, x: number, y: number): void {
    this.c.fillText(s, x, y);
  }

  /** Text on a rim of the plot ground, so it reads over the lines and areas under it. */
  haloText(s: string, x: number, y: number, color: string): void {
    this.c.save();
    this.c.setLineDash([]);
    this.c.lineJoin = "round";
    this.c.lineWidth = 3;
    this.c.strokeStyle = this.p.ground;
    this.c.strokeText(s, x, y);
    this.c.restore();
    this.c.fillStyle = color;
    this.c.fillText(s, x, y);
  }

  /** A side's bar: the stone colour inside its 1 px rim (the rim alone when too thin for both). */
  stoneRect(x: number, y: number, w: number, h: number, black: boolean): void {
    this.color(black ? this.p.stoneBlackEdge : this.p.stoneWhiteEdge);
    this.rect(x, y, w, h);
    this.color(black ? this.p.stoneBlack : this.p.stoneWhite);
    if (w > 2 && h > 2) this.rect(x + 1, y + 1, w - 2, h - 2);
  }

  /** A side's dot or marker: the stone colour with its rim. */
  stoneOval(x: number, y: number, w: number, h: number, black: boolean): void {
    this.color(black ? this.p.stoneBlack : this.p.stoneWhite);
    this.oval(x, y, w, h);
    if (w <= 0 || h <= 0) return;
    this.c.lineWidth = 1;
    this.c.setLineDash([]);
    this.c.strokeStyle = black ? this.p.stoneBlackEdge : this.p.stoneWhiteEdge;
    this.c.stroke();
  }

  /** A side's line in the current stroke width, cased in the stone's rim colour. */
  stoneLine(x1: number, y1: number, x2: number, y2: number, black: boolean): void {
    const width = this.c.lineWidth;
    this.c.beginPath();
    this.c.moveTo(x1, y1);
    this.c.lineTo(x2, y2);
    this.c.lineWidth = width + 1.5;
    this.c.strokeStyle = black ? this.p.stoneBlackEdge : this.p.stoneWhiteEdge;
    this.c.stroke();
    this.c.lineWidth = width;
    this.c.strokeStyle = black ? this.p.stoneBlack : this.p.stoneWhite;
    this.c.stroke();
  }

  /** A side's value after a small flat stone of its colour. */
  stoneLabel(s: string, x: number, y: number, black: boolean): void {
    const d = 8;
    this.stoneOval(x, y - this.ascentMinusDescent() / 2 - d / 2, d, d, black);
    this.haloText(s, x + d + 3, y, this.p.text);
  }

  /** FontMetrics.stringWidth. */
  width(s: string): number {
    return Math.round(this.c.measureText(s).width);
  }

  /** FontMetrics.getHeight. */
  height(): number {
    const m = this.c.measureText("Ag");
    return Math.round(m.fontBoundingBoxAscent + m.fontBoundingBoxDescent);
  }

  /** FontMetrics.getAscent() - getDescent(). */
  ascentMinusDescent(): number {
    const m = this.c.measureText("Ag");
    return Math.round(m.fontBoundingBoxAscent) - Math.round(m.fontBoundingBoxDescent);
  }
}

/** Sizes a canvas for the device pixel ratio and returns its context in CSS pixels. */
function fit(canvas: HTMLCanvasElement, w: number, h: number, fonts: { ui: string; info: string }): G {
  const dpr = window.devicePixelRatio || 1;
  canvas.style.width = `${w}px`;
  canvas.style.height = `${h}px`;
  const pw = Math.max(1, Math.round(w * dpr));
  const ph = Math.max(1, Math.round(h * dpr));
  if (canvas.width !== pw) canvas.width = pw;
  if (canvas.height !== ph) canvas.height = ph;
  const c = canvas.getContext("2d")!;
  c.setTransform(dpr, 0, 0, dpr, 0, 0);
  c.clearRect(0, 0, w, h);
  return new G(c, currentPalette(), fonts);
}

// ------------------------------------------------------------------ the selected line

/** The line's per-move data by move number. */
interface Line {
  total: number;
  points: Map<number, HistoryPoint>;
  moves: Map<number, MoveEvaluation>;
  /** Move numbers of `pane.moves`, ascending (for the running AI score). */
  moveNumbers: number[];
}

function lineOf(p: HawkeyePayload): Line {
  const points = new Map<number, HistoryPoint>();
  for (const pt of p.pane.points) points.set(pt.move_number, pt);
  const moves = new Map<number, MoveEvaluation>();
  for (const m of p.pane.moves) moves.set(m.move_number, m);
  return { total: p.pane.total_moves, points, moves, moveNumbers: p.pane.moves.map((m) => m.move_number) };
}

function other(s: Stone): Stone {
  return s === "Black" ? "White" : "Black";
}

/** The side to move at the line's position `k` (Java `node.getData().blackToPlay`). */
function toPlayAt(line: Line, k: number): Stone {
  const pt = line.points.get(k);
  if (pt) return pt.to_play;
  const next = line.moves.get(k + 1);
  if (next) return next.mover;
  const prev = line.moves.get(k);
  if (prev) return other(prev.mover);
  for (const m of line.moves.values()) return (k - m.move_number + 1) % 2 === 0 ? m.mover : other(m.mover);
  for (const q of line.points.values()) return (k - q.move_number) % 2 === 0 ? q.to_play : other(q.to_play);
  return k % 2 === 0 ? "Black" : "White";
}

/** Entries of `pane.moves` with a move number up to `k` (the index into `pane.running`). */
function movesUpTo(line: Line, k: number): number {
  let lo = 0;
  let hi = line.moveNumbers.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (line.moveNumbers[mid] <= k) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** The current position the charts mark: the main trunk's end while 主干 is chosen and the
 *  browsing position is in a variation (MoveListFrame.java:5061-5070). */
function currentOf(p: HawkeyePayload): number {
  return p.settings.branch === "Main" && !p.onMainTrunk ? p.pane.total_moves : p.currentMove;
}

/** The loss bars' height in percent of the chart (MoveListFrame.java:5537-5556). */
function convertHeight(d: number): number {
  if (d < -50) return -(4 / 8.0 + (Math.abs(d) - 50) / (8 * 50.0)) * 100.0;
  if (d < -20) return -(3 / 8.0 + (Math.abs(d) - 20) / (8 * 30.0)) * 100.0;
  if (d < -10) return -(2 / 8.0 + (Math.abs(d) - 10) / (8 * 10.0)) * 100.0;
  if (d < -5) return -(1 / 8.0 + (Math.abs(d) - 5) / (8 * 5.0)) * 100.0;
  if (d < 0) return -(Math.abs(d) / (8 * 5.0)) * 100.0;
  if (d < 5) return (Math.abs(d) / (8 * 5.0)) * 100.0;
  if (d < 20) return (1 / 8.0) * 100 + ((Math.abs(d) - 5.0) / (8 * 15.0)) * 100.0;
  return (2 / 8.0) * 100 + ((Math.abs(d) - 20.0) / (8.0 * 80)) * 100.0;
}

/** Java `params` after a complete `draw`: what `moveNumber` and the 翻车现场 slots use. */
interface Params {
  posx: number;
  width: number;
  numMoves: number;
}

interface Hover {
  /** curMouseOverNode's move number (胜率吻合). */
  move: number | null;
  /** mouseOverBigMistakeIndex. */
  slot: number;
}

// ------------------------------------------------------------------ draw (matchPanel)

/** MoveListFrame.draw (MoveListFrame.java:3288-5949) for `draw(g, 0, 0, panelWidth - 5, panelHeight)`. */
function draw(g: G, p: HawkeyePayload, line: Line, W: number, H: number, hover: Hover): Params | null {
  const chart = p.settings.chart;
  let posx = 0;
  let posy = 0;
  let width = W;
  let height = H;
  const P = g.p;
  g.color(P.ground);
  g.rect(posx, posy, chart !== 0 ? width + 5 : width + 2, height);
  const origH = height;
  const origW = width;

  if (chart < 7) {
    if (chart === 2 || chart === 3) posx += 23;
    else if (chart < 4) posx += 15;
    posy += chart === 2 || chart === 3 ? 2 : 8;
    width -= 4;
    if (chart === 2 || chart === 3) height -= 15;
    else if (chart >= 4) height -= 10;
    else height -= 16;
    if (chart === 2 || chart === 3) {
      g.stroke(1);
      g.color(P.baseline);
      g.line(0, posy, origW + 5, posy);
      g.line(0, posy + height, origW + 5, posy + height);
    }
  }

  // grid lines and their labels (MoveListFrame.java:3363-3451): hairlines, the middle or zero
  // line dashed, the base the bars stand on solid.
  let gridLines = 3;
  if (chart === 2 || chart === 3) gridLines = 7;
  if (chart === 4) gridLines = 4;
  if (chart > 4) gridLines = 5;
  if (chart >= 7) gridLines = 3;
  g.font("s");
  const gridLine = (x1: number, y: number, x2: number, kind: "grid" | "mid" | "base") => {
    g.stroke(1, kind === "mid" ? DASH : undefined);
    g.color(kind === "grid" ? P.grid : P.baseline);
    g.line(x1, y, x2, y);
  };
  const axisText = (text: string, y: number) => {
    g.color(P.axis);
    g.text(text, 3, y + 3);
  };
  for (let i = 1; i <= gridLines; i++) {
    const percent = (i * 100.0) / (gridLines + 1);
    const y = posy + height - jint((height * percent) / 100);
    if (chart === 2 || chart === 3) {
      gridLine(posx, y, posx + width - 3, i === 3 ? "mid" : "grid");
      axisText(["+20", "+5", "0", "-5", "-10", "-20", "-50"][i - 1], y);
    } else if (chart === 4) {
      if (i === 1) gridLine(0, y, posx + width, "base");
      else {
        gridLine(posx + 17, y, posx + width - 25, "grid");
        axisText(String(idiv(100 * (i - 1), 4)), y);
      }
    } else if (chart > 4 && chart < 7) {
      if (i === 2) gridLine(0, y, posx + width, "base");
      else if (i > 2) gridLine(posx, y, posx + width - 25, "grid");
    } else {
      if (chart >= 7) gridLine(posx, y, posx + width, "grid");
      else gridLine(posx, y, posx + width - 25, i === 2 ? "mid" : "grid");
      if (chart < 5) axisText(String(idiv(100 * i, 4)), y);
    }
  }

  if (chart === 5 || chart === 6) drawMisses(g, p, posx, posy, width, height, gridLines, chart === 6);
  if (chart === 4) {
    drawStages(g, p, posx, posy, width, height);
    return null;
  }
  if (chart === 5 || chart === 6) return null; // the rest of draw paints nothing for these

  g.stroke(SERIES);
  const total = line.total;
  const cur = currentOf(p);
  const first = p.firstMove;
  const last = p.lastMove;
  const offset = first - 1 > 0 ? first - 1 : 0;
  let numMoves = chart !== 2 && chart !== 3 ? 50 : 0;
  // the line's last position drawn (MoveListFrame.java:5072-5098)
  let end: number;
  let trueNumMoves: number;
  if (chart === 2 || chart === 3) {
    let k = cur;
    if (k >= total && k > 0) k--;
    while (k < total && k + 1 < total && k <= last + 1) k++;
    while (k > 0 && k > last + 1) k--;
    end = k;
    trueNumMoves = end - offset;
  } else {
    end = total;
    trueNumMoves = end - 1;
  }
  if (numMoves < trueNumMoves) numMoves = trueNumMoves;
  if (trueNumMoves < 1) return null;
  const firstNodeAnalyzed = line.points.has(1);
  width = chart === 0 ? width - 30 : width - 35;
  const xOf = (n: number) => posx + idiv(n * width, numMoves);
  const yOf = (wr: number) => posy + height - jint((wr * height) / 100);

  if (chart === 7 || chart === 8) {
    const list = chart === 7 ? p.pane.winrate_mistakes : p.pane.score_mistakes;
    g.font("s");
    const heightFont = g.ascentMinusDescent();
    g.color(P.baseline);
    g.stroke(1);
    g.line(0, height - heightFont - 10, width, height - heightFont - 10);
    list.forEach((m, i) => {
      const move = p.pane.moves[m.index];
      if (move) drawOneBigMistakePoint(g, width, height, move, m.change, i, chart === 8, hover.slot);
    });
    return { posx, width, numMoves };
  }

  let movenum = end - 1 - (chart === 2 || chart === 3 ? offset : 0);
  let lastWr = 50;
  let lastNodeOk = false;
  let lastOkMove = -1;
  let lastOkMoveSave = -1;
  let lastWrSave = 50;
  let lastMatchValueB = 0;
  let lastMatchValueW = 0;
  let cwr = -1;
  let cmovenum = -1;
  let cScore = -500;
  let mmovenum = -1;
  let mwr = -1;
  let mScore = -500;
  let cmovenumCase2 = -1;
  let cdiffwinCase2 = 0;
  let cConvertHeight = 0;
  const hoverMove = hover.move;
  const single = p.black !== p.white;

  for (let k = end; k > 0 && (chart === 0 || chart === 1 || k >= first); k--) {
    if (chart === 0) {
      // 胜率吻合: shading and the win-rate line (MoveListFrame.java:5290-5437)
      const pt = line.points.get(k);
      if (pt) {
        const wr = pt.black.winrate;
        const mv = line.moves.get(k);
        if (lastOkMove < 0) lastWr = wr;
        if (k <= last && k - 1 >= first && mv?.match_ai) {
          const black = mv.mover === "Black";
          if ((black && p.black) || (!black && p.white)) {
            g.color(black ? P.matchBlack : P.matchWhite);
            const edge = black ? origH : 0;
            const convert = lastOkMove - movenum > 1 ? wr + (lastWr - wr) / (lastOkMove - movenum) : lastWr;
            // the previous position's win rate as Black's (MoveListFrame.java:5283-5286, 5340, 5376)
            const previousWr = line.points.get(k - 1)?.black.winrate ?? 50;
            g.poly([xOf(movenum + 1), xOf(movenum + 1), xOf(movenum), xOf(movenum)], [yOf(convert), edge, edge, yOf(wr)]);
            g.poly([xOf(movenum), xOf(movenum), xOf(movenum - 1), xOf(movenum - 1)], [yOf(wr), edge, edge, yOf(previousWr)]);
          }
        }
        if (lastOkMove > 0) {
          const inRange = k + 1 <= last && k >= first;
          if (lastNodeOk) {
            g.color(inRange ? P.line : P.lineOff);
            g.line(xOf(lastOkMove), yOf(lastWr), xOf(movenum), yOf(wr));
          } else {
            g.color(P.lineOff);
            const lostMoves = lastOkMove - movenum;
            if (lostMoves < 35)
              g.line(xOf(lastOkMove), yOf(lastWr), xOf(movenum + 1), posy + height - jint(((wr - (wr - lastWr) / lostMoves) * height) / 100));
            g.color(inRange ? P.line : P.lineOff);
            const a = jint((lastWr * height) / 100);
            const b = jint((wr * height) / 100);
            g.line(xOf(movenum + 1), posy + height - a + idiv((a - b) * (lostMoves - 1), lostMoves), xOf(movenum), yOf(wr));
          }
        }
        if (k === cur) {
          cwr = wr;
          cmovenum = movenum;
        } else if (k === hoverMove) {
          mwr = wr;
          mmovenum = movenum;
        }
        lastWr = wr;
        lastNodeOk = true;
        lastOkMove = movenum;
      } else lastNodeOk = false;
      if (lastOkMove > 0 && !firstNodeAnalyzed) {
        lastOkMoveSave = lastOkMove;
        lastWrSave = lastWr;
      }
    } else if (chart === 1) {
      // 吻合度走势: getMatchValue of each position (MoveListFrame.java:3252-3286, 5438-5514)
      const [rb, rw] = p.pane.running[movesUpTo(line, k)] ?? [
        { share_sum: 0, count: 0 },
        { share_sum: 0, count: 0 },
      ];
      const blackValue = (rb.share_sum * 100) / rb.count;
      const whiteValue = (rw.share_sum * 100) / rw.count;
      const black = toPlayAt(line, k) === "Black";
      const analyzed = black ? rb.count : rw.count;
      const matchValue = black ? blackValue : whiteValue;
      if (analyzed >= 1) {
        const lastMatchValue = black ? lastMatchValueB : lastMatchValueW;
        if (k !== total)
          g.stoneLine(
            xOf(movenum + 2),
            posy + height - jint(((lastMatchValue > 0 ? lastMatchValue : matchValue) * height) / 100),
            xOf(movenum),
            jint(posy + height - (matchValue * height) / 100),
            black,
          );
      }
      if (!Number.isNaN(blackValue)) lastMatchValueB = blackValue;
      if (!Number.isNaN(whiteValue)) lastMatchValueW = whiteValue;
      if (k === cur) {
        const x = posx + idiv((cur - 1) * width, numMoves);
        g.stroke(1, DASH);
        g.color(P.guide);
        g.line(x, 0, x, posy + origH);
        g.font("s", true);
        g.haloText(String(cur), x + 3, posy + origH - 10, P.text);
        g.stroke(SERIES);
        g.font("l", true, g.fonts.info); // Font(Lizzie.config.fontName, BOLD, 15), MoveListFrame.java:5473
        const hi = posy + height - jint((Math.max(lastMatchValueB, lastMatchValueW) * height) / 100) - 4;
        const lo = posy + height - jint((Math.min(lastMatchValueB, lastMatchValueW) * height) / 100) + 16;
        const bx = xOf(movenum) - 5;
        const blackHigh = lastMatchValueB >= lastMatchValueW;
        if (rb.count >= 1) g.stoneLabel(fixed(lastMatchValueB, 1), bx, blackHigh ? hi : lo, true);
        if (rw.count >= 1) g.stoneLabel(fixed(lastMatchValueW, 1), bx, blackHigh ? lo : hi, false);
      }
    } else {
      // 胜率损失分布 / 目数损失分布: the loss of the move played here (MoveListFrame.java:5516-5659)
      const score = chart === 3;
      const mv = line.moves.get(k + 1);
      const raw = mv ? (score ? mv.score_change : mv.winrate_change) : null;
      const diff = mv?.best ? 0 : (raw ?? 0);
      let drawn = false;
      let convert = 0;
      if (mv?.analyzed && Math.abs(diff) > 0) {
        const black = mv.mover === "Black";
        drawn = black ? p.black : p.white;
        if (drawn) {
          convert = convertHeight(diff);
          if (score) convert = Math.min(100, Math.max(-100, convert));
          const x = posx + jint(((movenum + (single ? 0.5 : 1)) * width) / numMoves);
          const w = idiv((single ? 2 : 1) * width, numMoves) - (single ? 1 : 0);
          const base = posy + height - idiv(height * 3, 8);
          if (diff > 0) g.stoneRect(x, base, w, jint((convert * height) / 100), black);
          else g.stoneRect(x, jint(base + (convert * height) / 100), w, jint((Math.abs(convert) * height) / 100), black);
        }
      }
      if (cur > 0 && k === cur - 1) {
        const x = posx + jint(((movenum + 1.5) * width) / numMoves);
        g.stroke(1, DASH);
        g.color(P.guide);
        g.line(x, 0, x, posy + origH);
      }
      if (k + 1 === cur && drawn) {
        cmovenumCase2 = movenum;
        cdiffwinCase2 = diff;
        // Java keeps the raw score change here (MoveListFrame.java:5656)
        cConvertHeight = score ? diff : convert;
      }
    }
    movenum--;
  }

  // move number ticks (MoveListFrame.java:5679-5756)
  g.color(P.axis);
  g.font("s", false, g.fonts.ui); // Font(Lizzie.config.uiFontName, BOLD, 10), MoveListFrame.java:5680
  const step = numMoves <= 100 ? 10 : 20;
  if (chart === 2 || chart === 3) {
    // String concatenation as Java does: "0" + 10 → "010" (MoveListFrame.java:5685-5687)
    const prefix = String(first > 0 ? first : 0);
    for (let i = 1; i <= idiv(numMoves, step); i++)
      g.text(prefix + String(i * step), posx + idiv((i * step - 1) * width, numMoves) + 3, posy + origH - 5);
    const x = posx + idiv((cur - 1 - (first - 1 > 0 ? first : 0)) * width, numMoves);
    g.font("s", true);
    g.haloText(String(cur), x + 3, posy + origH - 5, P.text);
    if (cmovenumCase2 > 0) {
      g.font("m", true, g.fonts.info); // MoveListFrame.java:5715
      const text = fixed(cdiffwinCase2, 1);
      const mw = g.width(text);
      const x2 = posx + jint(((cmovenumCase2 + 1.5) * width) / numMoves - idiv(mw, 2));
      if (cdiffwinCase2 <= 0) g.haloText(text, x2, jint(posy + height + 13 - idiv(height * 3, 8)), P.loss);
      else g.haloText("+" + text, x2, jint(posy + height + 13 + (cConvertHeight / 100) * height - idiv(height * 3, 8)), P.loss);
    }
  } else {
    for (let i = 1; i <= idiv(numMoves, step); i++)
      g.text(String(i * step), posx + idiv((i * step - 1) * width, numMoves) + 3, posy + origH - 10);
  }

  if (chart === 0) {
    // the current and mouse-over positions, the score line (MoveListFrame.java:5757-5941)
    let x = posx + idiv((cur - 1) * width, numMoves);
    g.stroke(1, DASH);
    g.color(P.guide);
    g.line(x, 0, x, posy + origH);
    g.font("s", true);
    g.haloText(String(cur), x + 3, posy + origH - 10, P.text);
    if (mmovenum > 0 && hoverMove !== null) {
      x = posx + idiv((hoverMove - 1) * width, numMoves);
      g.stroke(1, DASH);
      g.color(P.hover);
      g.line(x, 0, x, posy + origH);
      g.haloText(String(hoverMove), x + 3, posy + origH - 10, P.text);
    }
    g.stroke(SERIES);
    if (lastOkMoveSave > 0 && !firstNodeAnalyzed) {
      g.color(P.lineOff);
      g.line(posx, posy + height - idiv(50 * height, 100), xOf(lastOkMoveSave), yOf(lastWrSave));
    }

    let maxScore = 30;
    const katago = p.pane.katago;
    if (katago) {
      for (const pt of line.points.values()) if (pt.score_lead !== null && Math.abs(pt.score_lead) > maxScore) maxScore = Math.abs(pt.score_lead);
      let lastScore = -500;
      let lastOk = -1;
      let n = total - 1;
      for (let k = total; k > 0; k--, n--) {
        const s = line.points.get(k)?.black.score_lead;
        if (s === undefined || s === null) continue;
        if (k === cur) cScore = s;
        else if (k === hoverMove) mScore = s;
        if (lastOk > 0 && Math.abs(n - lastOk) < 25 && lastScore > -500) {
          g.color(P.score);
          // dashed from a position off the main trunk (MoveListFrame.java:5829-5831)
          const offTrunk = p.pane.off_trunk_from !== null && k >= p.pane.off_trunk_from;
          if (offTrunk) g.stroke(1, DASH);
          else g.stroke(1);
          g.line(xOf(lastOk), scoreY(lastScore), xOf(n), scoreY(s));
        }
        lastScore = s;
        lastOk = n;
      }
    }
    function scoreY(s: number): number {
      const v = s > maxScore ? maxScore : s < 0 && Math.abs(s) > maxScore ? -maxScore : s;
      return posy + idiv(height, 2) - jint((v * height) / 2 / maxScore);
    }
    const shift = (katago ? 9 : 3) * DOT_RADIUS;
    const mPosX = mmovenum > 0 ? xOf(mmovenum) - shift : -1;
    const textY = (wr: number) => posy + (height - jint((wr * height) / 100)) + (wr > 50 ? 6 * DOT_RADIUS : -2 * DOT_RADIUS);
    const scoreText = (s: number, n: number) => {
      g.font("m", true, g.fonts.ui); // MoveListFrame.java:5891, 5929
      g.haloText(fixed(s, 1), numMoves - n < 2 ? xOf(n) - 13 : xOf(n), scoreY(s), P.scoreText);
    };
    if (cmovenum > 0) {
      g.font("l", true, g.fonts.ui); // MoveListFrame.java:5860
      const wrString = fixed(cwr, 1);
      const strWidth = g.width(wrString);
      const cPosX = xOf(cmovenum) - shift;
      if (Math.abs(cPosX - mPosX) > strWidth) {
        g.haloText(wrString, cPosX, textY(cwr), P.text);
        g.color(P.line);
        g.oval(xOf(cmovenum) - DOT_RADIUS, posy + height - jint((cwr * height) / 100) - DOT_RADIUS, DOT_RADIUS * 2, DOT_RADIUS * 2);
        if (cScore > -500) scoreText(cScore, cmovenum);
      }
    }
    if (mmovenum > 0) {
      g.font("l", true, g.fonts.ui); // MoveListFrame.java:5906
      g.haloText(fixed(mwr, 1), mPosX, textY(mwr), P.text);
      g.color(P.hover);
      g.oval(xOf(mmovenum) - DOT_RADIUS, posy + height - jint((mwr * height) / 100) - DOT_RADIUS, DOT_RADIUS * 2, DOT_RADIUS * 2);
      if (mScore > -500) scoreText(mScore, mmovenum);
    }
  }
  return { posx, width, numMoves };
}

/** drawOneBigMistakePoint (MoveListFrame.java:5951-6001). */
function drawOneBigMistakePoint(
  g: G,
  width: number,
  height: number,
  move: MoveEvaluation,
  change: number,
  index: number,
  isScore: boolean,
  hoverSlot: number,
): void {
  const cx = idiv(width, 20) + idiv(index * width, 10);
  if (index === hoverSlot) {
    g.color(g.p.hover);
    g.stroke(1.5, [5]);
    g.line(cx, 0, cx, height);
  }
  const black = move.mover === "Black";
  const winrate = move.winrate ?? 50;
  const yc = jint(height * 0.15 + (height * 0.7 * (100 - winrate)) / 100.0);
  g.color(black ? g.p.stoneBlackEdge : g.p.stoneWhiteEdge);
  g.poly(
    [cx - idiv(height, 40), cx - idiv(height, 55), cx - idiv(height, 10)],
    [yc, yc - idiv(height, 30), yc - idiv(height, 10)],
  );
  g.stoneOval(cx - idiv(height, 26), yc - idiv(height, 26), idiv(height, 13), idiv(height, 13), black);
  g.font("s");
  g.color(g.p.axis);
  const label = String(move.move_number);
  g.text(label, cx - idiv(g.width(label), 2), height - 5);
  g.font("m", true);
  g.haloText(fixed(change, 1) + (isScore ? "" : "%"), cx + idiv(height, 13), yc, g.p.text);
}

/** 胜率损失统计 / 目数损失统计 histograms (MoveListFrame.java:3452-4696). */
function drawMisses(g: G, p: HawkeyePayload, posx: number, posy: number, width: number, height: number, gridLines: number, score: boolean): void {
  const misses = score ? p.pane.score_misses : p.pane.winrate_misses;
  const s = p.settings;
  const r1 = score ? s.scoreRange1 : s.winrateRange1;
  const r2 = score ? s.scoreRange2 : s.winrateRange2;
  const unit = score ? "" : "%";
  g.font("m", false, g.fonts.info); // MoveListFrame.java:3453, 4082
  g.color(g.p.label);
  const y = posy + height - jint((height * ((2 * 100.0) / (gridLines + 1))) / 100);
  const y2 = posy + height - jint((height * (100.0 / (gridLines + 1))) / 100);
  const fh = g.height();
  const centred = (text: string, k: number, at: number) => g.text(text, posx + idiv(k * width, 72) - idiv(g.width(text), 2), at);
  const all = (side: number, b: number) => misses[side][0][b] + misses[side][1][b] + misses[side][2][b];
  const allMax = Math.max(all(0, 0), all(0, 1), all(0, 2), all(1, 0), all(1, 1), all(1, 2));
  const stageMax = (st: number) => Math.max(...misses[0][st], ...misses[1][st]);
  const o = s.openingEndMove;
  const m = s.middleEndMove;
  centred(t("Movelistframe.all"), 9, y2 + fh);
  centred(t("Movelistframe.open") + "(1-" + o + ")", 27, y2 + fh);
  centred(t("Movelistframe.middle") + "(" + (o + 1) + "-" + m + ")", 45, y2 + fh);
  centred(t("Movelistframe.end") + "(" + (m + 1) + t("Movelistframe.toEnd") + ")", 63, y2 + fh);
  for (const base of [0, 18, 54, 36]) {
    centred("<" + r1 + unit, 4 + base, y + fh);
    centred(r1 + "-" + r2 + unit, 9 + base, y + fh);
    centred(">" + r2 + unit, 14 + base, y + fh);
  }
  const unitH = idiv(height * 4, 7);
  g.font("s", true, g.fonts.info);
  for (const side of [0, 1]) {
    for (let group = 0; group < 4; group++)
      for (let b = 0; b < 3; b++) {
        const count = group === 0 ? all(side, b) : misses[side][group - 1][b];
        const max = group === 0 ? allMax : stageMax(group - 1);
        const k = [2, 20, 38, 56][group] + 5 * b + 2 * side;
        const h = jint(unitH * (count / max));
        g.stoneRect(posx + idiv(k * width, 72), y - h, idiv(width, 36), h, side === 0);
        const text = String(count);
        g.color(g.p.text);
        g.text(text, posx + idiv((k + 1) * width, 72) - idiv(g.width(text), 2), y - h - idiv(fh, 4));
      }
  }
}

/** 分段吻合度 histogram (MoveListFrame.java:4698-5053). */
function drawStages(g: G, p: HawkeyePayload, posx: number, posy: number, width: number, height: number): void {
  const stages = p.pane.stages;
  const o = p.settings.openingEndMove;
  const m = p.settings.middleEndMove;
  const share = (st: HawkeyeStage) => (st.match_analyzed > 0 ? st.share_sum / st.match_analyzed : 0);
  g.font("m", false, g.fonts.info); // MoveListFrame.java:4726
  g.color(g.p.label);
  const labelY = posy + idiv(height * 4, 5) + 15;
  const centred = (text: string, k: number) => g.text(text, posx + idiv(k * width, 16) - idiv(g.width(text), 2), labelY);
  centred(t("Movelistframe.all"), 2);
  centred(t("Movelistframe.open") + "(1-" + o + ")", 6);
  centred(t("Movelistframe.middle") + "(" + (o + 1) + "-" + m + ")", 10);
  centred(t("Movelistframe.end") + "(" + (m + 1) + t("Movelistframe.toEnd") + ")", 14);
  const bar = (v: number, k: number, at: number, black: boolean) => {
    const h = jint(v > 0.75 ? ((v - 0.75) * height * 4) / 7 + idiv(height * 3, 5) : (v * height * 4) / 5);
    const top = posy + idiv(height * 4, 5) - h;
    g.stoneRect(posx + idiv(k * width, 16), top, idiv(width, 16), h, black);
    const text = fixed(v * 100, 1);
    g.color(g.p.text);
    g.text(text, jint(posx + (at * width) / 16 - idiv(g.width(text), 2)), top - 3);
  };
  g.font("m", true, g.fonts.info);
  for (const side of [0, 1]) {
    const st = stages[side];
    const allAnalyzed = st[0].match_analyzed + st[1].match_analyzed + st[2].match_analyzed;
    if (allAnalyzed > 0) bar((st[0].share_sum + st[1].share_sum + st[2].share_sum) / allAnalyzed, 1 + side, 1.5 + side, side === 0);
    for (let n = 0; n < 3; n++) if (st[n].analyzed > 0) bar(share(st[n]), 5 + 4 * n + side, 5.5 + 4 * n + side, side === 0);
  }
}

// ------------------------------------------------------------------ drawMin (matchPanelmin)

/** MoveListFrame.drawMin (MoveListFrame.java:2100-3161): two lines of figures, and on 胜率吻合 /
 *  吻合度走势 the legend after them (Java's fixed places, moved right where the text runs longer). */
function drawMin(g: G, p: HawkeyePayload, w: number, h: number): void {
  const P = g.p;
  g.color(P.ground);
  g.rect(0, 0, w, h);
  const chart = p.settings.chart;
  const pane = p.pane;
  const s = p.settings;
  g.color(P.text);
  g.font("m");
  /** Writes `text` and returns where it ends. */
  const write = (text: string, x: number, y: number) => {
    g.color(P.text);
    g.font("m");
    g.text(text, x, y);
    return x + g.width(text);
  };
  /** The legend from `x` on the second line: each mark in an 18 px slot, then its label. */
  const legend = (x: number, items: [(x: number) => void, string][]) => {
    for (const [mark, text] of items) {
      mark(x);
      x += 23;
      g.font("s");
      g.color(P.axis);
      g.text(text, x, 34);
      x += g.width(text) + 12;
    }
  };
  const swatch = (color: string) => (x: number) => {
    g.color(color);
    g.rect(x, 25, 18, 10);
  };
  const stroke = (color: string, width: number) => (x: number) => {
    g.color(color);
    g.stroke(width);
    g.line(x, 30, x + 18, 30);
  };
  if (chart === 2 || chart === 3) {
    const score = chart === 3;
    const spreads = score ? pane.score_spread : pane.winrate_spread;
    const r1 = score ? s.scoreRange1 : s.winrateRange1;
    const r2 = score ? s.scoreRange2 : s.winrateRange2;
    [0, 1].forEach((side) => {
      const sp = spreads[side];
      const head = side === 0 ? t("Movelistframe.blackAnalyzed") : t("Movelistframe.whiteAnalyzed");
      const band = (b: { count: number; sum: number; squares: number }, range: number) =>
        t("Movelistframe.above") +
        range +
        (score ? t("Movelistframe.scorePoints") : "%：") +
        b.count +
        t("Movelistframe.movedAndAverage") +
        fixed(b.sum / b.count, 2) +
        (score ? t("Movelistframe.stdev") : t("Movelistframe.percentAndStdev")) +
        fixed(b.squares / b.count, 1) +
        t("Movelistframe.rightBrackets");
      g.text(
        head +
          sp.analyzed +
          t("Movelistframe.movedAndAverage") +
          fixed(sp.sum / sp.analyzed, 2) +
          (score ? t("Movelistframe.scoreAndStdev") : t("Movelistframe.percentAndStdev")) +
          fixed(sp.squares / sp.analyzed, 1) +
          band(sp.above1, r1) +
          band(sp.above2, r2),
        5,
        side === 0 ? 15 : 35,
      );
    });
  } else if (chart === 0) {
    const ends = [0, 1].map((side) => {
      const sd = pane.sides[side];
      const percent = fixed(Math.fround(Math.fround(sd.matched / (sd.match_analyzed > 0 ? sd.match_analyzed : 1)) * 100), 1) + "%";
      return write(
        (side === 0 ? t("Movelistframe.blackMatch") : t("Movelistframe.whiteMatch")) +
          sd.matched +
          "/" +
          sd.match_analyzed +
          " " +
          percent +
          t("Movelistframe.AIscore") +
          fixed((sd.share_sum * 100) / sd.match_analyzed, 2) +
          t("Movelistframe.avgDifference") +
          fixed(sd.winrate_loss_sum / sd.analyzed, 2) +
          "%" +
          "/ " +
          fixed(sd.score_loss_sum / sd.analyzed, 2),
        25,
        side === 0 ? 15 : 35,
      );
    });
    const analyzedMatch = pane.sides[0].match_analyzed + pane.sides[1].match_analyzed;
    write(t("Movelistframe.allmoves") + pane.total_moves + t("Movelistframe.analyzed") + analyzedMatch, Math.max(491, ends[0] + 16), 15);
    // the legend (MoveListFrame.java:2759-2807)
    legend(Math.max(500, ends[1] + 16), [
      [swatch(P.matchBlack), t("Movelistframe.blackMatchLabel")],
      [swatch(P.matchWhite), t("Movelistframe.whiteMatchLabel")],
      [stroke(P.line, 3), t("Movelistframe.analyzedLabel")],
      [stroke(P.lineOff, 3), t("Movelistframe.unanalyzedLabel")],
      [stroke(P.score, 1), t("Movelistframe.scoreLeadLabel")],
    ]);
  } else if (chart === 1) {
    const [b, wt] = pane.sides;
    const end = write(t("Movelistframe.blackAIScore") + fixed((b.share_sum * 100) / b.match_analyzed, 2), 25, 15);
    const end2 = write(t("Movelistframe.whiteAIScore") + fixed((wt.share_sum * 100) / wt.match_analyzed, 2), 25, 35);
    // Chinese counts the match-analysed moves, English the loss-analysed ones (MoveListFrame.java:2830-2872)
    const counted = p.chinese ? b.match_analyzed + wt.match_analyzed : b.analyzed + wt.analyzed;
    write(t("Movelistframe.allmoves") + pane.total_moves + t("Movelistframe.analyzed") + counted, Math.max(p.chinese ? 160 : 175, end + 16), 15);
    g.stroke(SERIES);
    legend(Math.max(p.chinese ? 172 : 182, end2 + 16), [
      [
        (x) => {
          g.stoneLine(x, 33, x + 18, 33, true);
          g.stoneLine(x, 27, x + 18, 27, false);
        },
        t("Movelistframe.analyzedLabel"),
      ],
      [stroke(P.lineOff, 3), t("Movelistframe.unanalyzedLabel")],
    ]);
  } else if (chart === 4) {
    g.font("m");
    [0, 1].forEach((side) => {
      const st = pane.stages[side];
      const win = (a: number, sum: number) => (a > 0 ? fixed(sum / a, 1) : "0");
      const sum = (f: (x: HawkeyeStage) => number) => f(st[0]) + f(st[1]) + f(st[2]);
      const stage = (x: HawkeyeStage) => win(x.analyzed, x.winrate_loss_sum) + t("Movelistframe.percentAndScore") + win(x.analyzed, x.score_loss_sum);
      g.text(
        (side === 0 ? t("Movelistframe.blackDifference") : t("Movelistframe.whiteDifference")) +
          t("Movelistframe.allWinrate") +
          win(sum((x) => x.analyzed), sum((x) => x.winrate_loss_sum)) +
          t("Movelistframe.percentAndScore") +
          win(sum((x) => x.analyzed), sum((x) => x.score_loss_sum)) +
          t("Movelistframe.openAndWinrate") +
          stage(st[0]) +
          t("Movelistframe.middleAndWinrate") +
          stage(st[1]) +
          t("Movelistframe.endAndWinrate") +
          stage(st[2]) +
          t("Movelistframe.rightBrackets"),
        25,
        side === 0 ? 15 : 35,
      );
    });
  } else if (chart === 5 || chart === 6) {
    g.font("m");
    const score = chart === 6;
    const misses = score ? pane.score_misses : pane.winrate_misses;
    const r1 = score ? s.scoreRange1 : s.winrateRange1;
    const r2 = score ? s.scoreRange2 : s.winrateRange2;
    const unit = score ? t("Movelistframe.pts") : "% ";
    [0, 1].forEach((side) => {
      const m = misses[side];
      const stage = (n: number) =>
        r1 + unit + m[n][0] + t("Movelistframe.moves") + r1 + "-" + r2 + unit + m[n][1] + t("Movelistframe.movesAnd>") + r2 + unit + m[n][2];
      g.text(
        (side === 0 ? t("Movelistframe.blackOpen") : t("Movelistframe.whiteOpen")) +
          stage(0) +
          t("Movelistframe.movesAndMiddle") +
          stage(1) +
          t("Movelistframe.movesAndEnd") +
          stage(2) +
          t("Movelistframe.movesAndRightBrackets"),
        5,
        side === 0 ? 15 : 35,
      );
    });
  }
}

type Bounds = [number, number, number, number];

export function mountCharts(host: HTMLElement, actions: ChartActions): ChartPanel {
  host.classList.add("hk-charts");
  host.replaceChildren();

  // The chart tabs (MoveListFrame.java:666-684), wrapping onto a second row when they do not fit.
  const tabLayer = document.createElement("div");
  tabLayer.className = "hk-chart-tabs";
  const tabs = TAB_CHARTS.map((chart) => {
    const d = document.createElement("div");
    d.className = "hk-chart-tab";
    d.dataset.key = `hawkeye.chart.tab.${chart}`;
    d.addEventListener("mousedown", (e) => {
      if (e.button !== 0 || !payload || payload.settings.chart === chart) return;
      actions.run(settingsCommand(payload.settings, { chart }));
    });
    tabLayer.append(d);
    return { chart, d };
  });
  const chartCanvas = document.createElement("canvas");
  chartCanvas.dataset.key = "hawkeye.chart.canvas";
  const min = document.createElement("div");
  min.className = "hk-min";
  const minCanvas = document.createElement("canvas");
  minCanvas.style.left = "0px";
  minCanvas.style.top = "0px";

  const button = (key: string, tip?: string) => {
    const b = document.createElement("button");
    b.type = "button";
    b.dataset.key = key;
    b.tabIndex = -1; // setFocusable(false)
    if (tip) b.title = tip;
    return b;
  };
  const label = (text: string) => {
    const l = document.createElement("span");
    l.className = "hk-label";
    l.textContent = text;
    return l;
  };
  const field = (key: string) => {
    const f = document.createElement("input");
    f.type = "text";
    f.dataset.key = key;
    f.spellcheck = false;
    f.addEventListener("focus", () => f.select()); // focusGained → selectAll
    return f;
  };

  // matchPanelmin controls (MoveListFrame.java:763-1132, 6996-7063): the two buttons left of the
  // figures, the match condition or the loss ranges as one line of labels and fields under them.
  const detail = button("hawkeye.chart.detail");
  const detailIcons = { up: icon("chevron-up"), down: icon("chevron-down") };
  const settings = button("hawkeye.chart.settings", t("Movelistframe.settingsToolTip"));
  settings.append(icon("control-panel"));
  const lblMatch1 = label(t("Movelistframe.lblMatchConfig1"));
  const lblMatch2 = label(t("Movelistframe.lblMatchConfig2"));
  const lblMatch3 = label("%");
  const lblMatch4 = label(t("Movelistframe.accuracyAndMatch.tip"));
  lblMatch4.className = "hk-note";
  lblMatch4.title = lblMatch4.textContent ?? "";
  const suggestionMoves = field("hawkeye.chart.match-moves");
  const percentPlayouts = field("hawkeye.chart.match-percent");
  percentPlayouts.className = "wide";
  const lblDiff1 = label(t("Movelistframe.lblDiffConfig1"));
  const lblDiff2 = label(t("Movelistframe.lblDiffConfig2"));
  const range1 = field("hawkeye.chart.range1");
  const range2 = field("hawkeye.chart.range2");
  const row = document.createElement("div");
  row.className = "hk-min-row";
  row.append(lblMatch1, suggestionMoves, lblMatch2, percentPlayouts, lblMatch3, lblDiff1, range1, lblDiff2, range2, lblMatch4);
  min.append(minCanvas, detail, settings, row);
  host.append(tabLayer, chartCanvas, min);

  let payload: HawkeyePayload | null = null;
  let line: Line | null = null;
  let params: Params | null = null;
  const hover: Hover = { move: null, slot: -1 };
  /** The chart whose texts the fields were last given (Java setPanel on a tab change). */
  let panelChart = -1;
  /** The setting each field last showed: a field takes a new value only when it changes. */
  const shown = new Map<HTMLInputElement, string>();

  const place = (el: HTMLElement, b: Bounds | null) => {
    if (!b || b[2] <= 0 || b[3] <= 0) {
      el.style.display = "none";
      return;
    }
    el.style.display = "";
    el.style.left = `${b[0]}px`;
    el.style.top = `${b[1]}px`;
    el.style.width = `${b[2]}px`;
    el.style.height = `${b[3]}px`;
  };

  /** JTextField.setText from a setting: skipped while the field is being edited. */
  const setField = (f: HTMLInputElement, text: string, force: boolean) => {
    const changed = shown.get(f) !== text;
    shown.set(f, text);
    if (document.activeElement === f) return;
    if (changed || force) {
      f.value = text;
      f.classList.remove("invalid");
    }
  };

  const layoutControls = (p: HawkeyePayload) => {
    const chart = p.settings.chart;
    const tabChanged = panelChart !== chart;
    panelChart = chart;
    const all = [detail, settings, lblMatch1, suggestionMoves, lblMatch2, percentPlayouts, lblMatch3, lblMatch4, lblDiff1, range1, lblDiff2, range2];
    const show = (...shown: HTMLElement[]) => {
      for (const el of all) el.style.display = shown.includes(el) ? "" : "none";
    };
    if (chart === 2 || chart === 3 || chart === 5 || chart === 6) {
      const score = chart === 3 || chart === 6;
      lblDiff1.textContent = score ? t("Movelistframe.lblDiffConfigScore") : t("Movelistframe.lblDiffConfig1");
      show(lblDiff1, range1, lblDiff2, range2);
      setField(range1, String(score ? p.settings.scoreRange1 : p.settings.winrateRange1), tabChanged);
      setField(range2, String(score ? p.settings.scoreRange2 : p.settings.winrateRange2), tabChanged);
    } else if (chart < 7) {
      place(detail, [2, 0, 20, 18]);
      place(settings, [2, 18, 20, 18]);
      const next = p.settings.showGraph ? detailIcons.down : detailIcons.up;
      if (detail.firstChild !== next) detail.replaceChildren(next);
      if (chart === 0) {
        show(detail, settings, lblMatch1, suggestionMoves, lblMatch2, percentPlayouts, lblMatch3, lblMatch4);
        setField(suggestionMoves, String(p.matchAiMoves), tabChanged);
        // Java String.valueOf(double): 20.0, 12.5
        const percent = p.matchAiPercents;
        setField(percentPlayouts, Number.isInteger(percent) ? `${percent}.0` : String(percent), tabChanged);
      } else show(detail, settings, lblMatch4);
    } else show();
  };

  const paintChart = () => {
    const p = payload;
    if (!p || !line || chartCanvas.style.display === "none") return;
    const w = chartCanvas.clientWidth || parseFloat(chartCanvas.style.width) || 0;
    const h = chartCanvas.clientHeight || parseFloat(chartCanvas.style.height) || 0;
    if (w <= 0 || h <= 0) return;
    const done = draw(fit(chartCanvas, w, h, p.fonts), p, line, w - 5, h, hover);
    if (done) params = done;
  };

  const paintMin = () => {
    const p = payload;
    if (!p || min.style.display === "none") return;
    const w = min.clientWidth || parseFloat(min.style.width) || 0;
    if (w <= 0) return;
    drawMin(fit(minCanvas, w, MIN_HEIGHT, p.fonts), p, w, MIN_HEIGHT);
  };

  const paint = () => {
    const p = payload;
    if (!p) return;
    const W = host.clientWidth;
    const H = host.clientHeight;
    const chart = p.settings.chart;
    for (const tab of tabs) {
      const text = tabLabel(tab.chart);
      if (tab.d.textContent !== text) tab.d.textContent = text;
      tab.d.classList.toggle("on", tab.chart === chart);
    }

    const top = tabLayer.offsetHeight;
    const contentH = Math.max(0, H - top);
    const hasMin = chart < 7;
    if (hasMin) {
      place(min, [0, Math.max(top, H - MIN_HEIGHT), W, MIN_HEIGHT]);
      layoutControls(p);
    } else place(min, null);
    const chartH = Math.max(0, contentH - (hasMin ? MIN_HEIGHT : 0));
    if (p.settings.showGraph && chartH > 0 && W > 0) {
      chartCanvas.style.display = "";
      chartCanvas.style.left = "0px";
      chartCanvas.style.top = `${top}px`;
      chartCanvas.style.width = `${W}px`;
      chartCanvas.style.height = `${chartH}px`;
    } else chartCanvas.style.display = "none";
    paintChart();
    paintMin();
  };
  onThemeChange(paint);

  // ---------------------------------------------------------------- field edits

  // updateSuggestionMoves / updatePercentPlayouts (MoveListFrame.java:1930-1970)
  suggestionMoves.addEventListener("input", () => {
    const v = parseJava(suggestionMoves.value);
    const moves = v === null ? null : Math.trunc(v);
    const ok = moves !== null && moves >= 1;
    suggestionMoves.classList.toggle("invalid", !ok);
    if (ok) actions.display({ matchAiMoves: moves });
  });
  percentPlayouts.addEventListener("input", () => {
    const v = parseJava(percentPlayouts.value);
    const ok = v !== null && v <= 100 && v >= 0;
    percentPlayouts.classList.toggle("invalid", !ok);
    if (ok) actions.display({ matchAiPercentsPlayouts: v });
  });
  // winrateDiffRange1 / 2 document listeners (MoveListFrame.java:970-1132)
  const rangeInput = (f: HTMLInputElement, second: boolean) => () => {
    const v = parseJava(f.value);
    f.classList.toggle("invalid", v === null);
    if (v === null || !payload) return;
    const chart = payload.settings.chart;
    const n = Math.trunc(v);
    if (chart === 3 || chart === 6) actions.run(settingsCommand(payload.settings, second ? { scoreRange2: n } : { scoreRange1: n }));
    if (chart === 2 || chart === 5) actions.run(settingsCommand(payload.settings, second ? { winrateRange2: n } : { winrateRange1: n }));
  };
  range1.addEventListener("input", rangeInput(range1, false));
  range2.addEventListener("input", rangeInput(range2, true));

  detail.addEventListener("click", () => {
    if (payload) actions.run(settingsCommand(payload.settings, { showGraph: !payload.settings.showGraph }));
  });
  settings.addEventListener("click", () => actions.openMatchHelp());

  // ---------------------------------------------------------------- mouse on the chart

  /** moveNumber(x, y) (MoveListFrame.java:3169-3194): -1 outside the chart. */
  const moveNumber = (x: number, y: number): number => {
    const p = payload;
    if (!p || !params) return -1;
    const w = chartCanvas.clientWidth - 5;
    const h = chartCanvas.clientHeight;
    if (!(0 <= x && x < w && 0 <= y && y < h)) return -1;
    const chart = p.settings.chart;
    if (chart === 2 || chart === 3) {
      const n = Math.round(((x - params.posx) * (params.numMoves - 1)) / params.width);
      return n + (p.firstMove > 0 ? p.firstMove : 1);
    }
    return Math.round(((x - params.posx) * params.numMoves) / params.width) + 1;
  };
  /** getMouseOverBigMistakeIndex (MoveListFrame.java:3163-3167). */
  const slotAt = (x: number): number => {
    if (!params || params.width <= 10) return -1;
    return idiv(x, idiv(params.width, 10));
  };
  const at = (e: MouseEvent) => {
    const r = chartCanvas.getBoundingClientRect();
    return { x: Math.floor(e.clientX - r.left), y: Math.floor(e.clientY - r.top) };
  };
  let lastGoto: number | null = null;
  /** goToMoveNumberBeyondBranch: -1 goes back to the start. */
  const goto = (n: number) => {
    const number = Math.max(0, n);
    if (number === lastGoto || !payload) return;
    lastGoto = number;
    actions.run({ type: "Hawkeye", op: { type: "GotoMoveNumber", document: payload.document, number } });
  };

  chartCanvas.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || !payload) return;
    const { x, y } = at(e);
    const chart = payload.settings.chart;
    lastGoto = null;
    if (chart >= 7) {
      const index = slotAt(x);
      if (index < 0) return;
      const list = chart === 7 ? payload.pane.winrate_mistakes : payload.pane.score_mistakes;
      const move = list[index] ? payload.pane.moves[list[index].index] : undefined;
      if (move) actions.run({ type: "Hawkeye", op: { type: "GotoMove", document: payload.document, node: move.node } });
      return;
    }
    if (chart >= 4) return;
    chartCanvas.setPointerCapture(e.pointerId);
    goto(moveNumber(x, y));
  });
  chartCanvas.addEventListener("pointermove", (e) => {
    const p = payload;
    if (!p) return;
    const { x, y } = at(e);
    const chart = p.settings.chart;
    if (e.buttons & 1) {
      if (chart < 4 && chartCanvas.hasPointerCapture(e.pointerId)) goto(moveNumber(x, y));
      return;
    }
    if (chart >= 7) {
      const slot = slotAt(x);
      if (slot !== hover.slot) {
        hover.slot = slot;
        paintChart();
      }
    }
    if (chart === 0) {
      // curMouseOverNode: the line's position at that move number (MoveListFrame.java:1731-1759)
      const n = moveNumber(x, y);
      if (n >= 0) {
        const move = Math.min(Math.max(n, 0), line?.total ?? 0);
        if (move !== hover.move) {
          hover.move = move;
          paintChart();
        }
      }
    }
  });
  chartCanvas.addEventListener("pointerup", (e) => {
    if (chartCanvas.hasPointerCapture(e.pointerId)) chartCanvas.releasePointerCapture(e.pointerId);
    lastGoto = null;
  });
  chartCanvas.addEventListener("mouseleave", () => {
    if (hover.move === null && hover.slot < 0) return;
    hover.move = null;
    hover.slot = -1;
    paintChart();
  });
  chartCanvas.addEventListener(
    "wheel",
    (e) => {
      e.preventDefault();
      if (e.deltaY > 0) actions.run({ type: "Navigate", nav: { op: "Forward", count: 1 } });
      else if (e.deltaY < 0) actions.run({ type: "Navigate", nav: { op: "Back", count: 1 } });
    },
    { passive: false },
  );

  // ---------------------------------------------------------------- the panel

  const centre = (el: Element): { x: number; y: number } => {
    const r = el.getBoundingClientRect();
    return { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) };
  };
  const visible = (el: HTMLElement) => el.getClientRects().length > 0;

  return {
    render(p: HawkeyePayload) {
      if (payload && payload.settings.chart !== p.settings.chart) {
        hover.move = null;
        hover.slot = -1;
      }
      payload = p;
      line = lineOf(p);
      paint();
    },
    resize() {
      paint();
    },
    targets(): Target[] {
      const p = payload;
      if (!p) return [];
      const out: Target[] = [];
      for (const { chart, d } of tabs)
        out.push({ key: d.dataset.key!, text: d.textContent ?? "", checked: chart === p.settings.chart, ...centre(d) });
      if (visible(detail)) out.push({ key: detail.dataset.key!, text: p.settings.showGraph ? "down" : "up", checked: p.settings.showGraph, ...centre(detail) });
      if (visible(settings)) out.push({ key: settings.dataset.key!, text: settings.title, ...centre(settings) });
      for (const f of [suggestionMoves, percentPlayouts, range1, range2])
        if (visible(f)) out.push({ key: f.dataset.key!, text: f.classList.contains("invalid") ? "invalid" : "", value: f.value, ...centre(f) });
      if (chartCanvas.style.display !== "none") {
        out.push({ key: chartCanvas.dataset.key!, text: tabLabel(p.settings.chart), ...centre(chartCanvas) });
        const chart = p.settings.chart;
        if (chart >= 7 && params) {
          const r = chartCanvas.getBoundingClientRect();
          const list = chart === 7 ? p.pane.winrate_mistakes : p.pane.score_mistakes;
          list.forEach((m, i) => {
            const move = p.pane.moves[m.index];
            if (!move || !params) return;
            const x = idiv(params.width, 20) + idiv(i * params.width, 10);
            out.push({ key: `hawkeye.chart.mistake.${i}`, text: String(move.move_number), x: Math.round(r.left + x), y: Math.round(r.top + r.height / 2) });
          });
        }
      }
      return out;
    },
  };
}
