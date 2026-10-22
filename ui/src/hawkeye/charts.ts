// 超级鹰眼 bottom panel (MoveListFrame.java `bottomPanel`): the JTabbedPane with the nine chart
// tabs, each holding `matchPanel` (the chart, `draw`) and the 60 px `matchPanelmin` (`drawMin` and
// its controls; absent on the 翻车现场 tabs). Everything painted comes from the payload; the
// panel keeps only the mouse-over state and the text of its fields while they are edited.
import type { Command, DisplaySettings, HawkeyeStage, HistoryPoint, MoveEvaluation, Stone } from "../contract/generated";
import { t } from "../i18n";
import { fixed, settingsCommand, UI_FONT, type HawkeyePayload } from "./model";

export interface ChartActions {
  /** Core commands: { type: "Hawkeye", op: { type: "GotoMoveNumber", number } } for curve press/drag, { type: "Hawkeye", op: { type: "GotoMove", node } } for a 翻车现场 slot (node = pane.moves[mistake.index].node), { type: "Navigate", nav: { op: "Back"|"Forward", count: 1 } } for the wheel, SetHawkeyeSettings via settingsCommand(...) for chart tab / detail toggle / diff ranges. */
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

/** Config.frameFontSize (default 12): the tab labels and the 翻车现场 texts. */
const FRAME_FONT_SIZE = 12;
/** matchPanelmin height (MoveListFrame.java:627, 2071). */
const MIN_HEIGHT = 60;
/** Metal (Ocean) JTabbedPane as the Java screenshots show it: an 18 px tab run, a content border
 *  of one dark line, one white line and two lines of the selected-tab colour on top, two lines of
 *  that colour and the outer border on the other sides. */
const TAB_ROW = 18;
const CONTENT_TOP = 4;
const INSET = 3;
/** Tab width = label width + this (measured on j05-hawkeye.png). */
const TAB_PAD = 21;
const PANEL_BG = "rgb(246,247,249)";
const TAB_BG = "rgb(238,238,238)";
const OCEAN_SELECTED = "rgb(200,221,242)";
const OCEAN_DARK = "rgb(99,130,191)";
const OCEAN_BORDER = "rgb(122,138,153)";

// java.awt.Color constants used by draw / drawMin
const BLACK = "rgb(0,0,0)";
const WHITE = "rgb(255,255,255)";
const GRAY = "rgb(128,128,128)";
const LIGHT_GRAY = "rgb(192,192,192)";
const YELLOW = "rgb(255,255,0)";
const ORANGE = "rgb(255,200,0)";
const CYAN = "rgb(0,255,255)";
const MAGENTA = "rgb(255,0,255)";
const RED = "rgb(255,0,0)";
/** new Color(150, 150, 150): the chart background (MoveListFrame.java:3305). */
const CHART_BG = "rgb(150,150,150)";
/** defaultScoreColor (MoveListFrame.java:248). */
const SCORE_COLOR = "rgb(255,0,0)";
const MATCH_BLACK = "rgba(0,0,255,0.392)";
const MATCH_WHITE = "rgba(0,255,0,0.392)";
const DOT_RADIUS = 3;
const DASH = [4];

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

/** The few Graphics2D operations MoveListFrame paints with, on Java's pixel grid. */
class G {
  constructor(readonly c: CanvasRenderingContext2D) {
    c.textBaseline = "alphabetic";
    c.textAlign = "left";
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

  font(size: number, bold: boolean): void {
    this.c.font = `${bold ? "bold " : ""}${size}px ${UI_FONT}`;
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
function fit(canvas: HTMLCanvasElement, w: number, h: number): G {
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
  return new G(c);
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
  g.color(CHART_BG);
  g.rect(posx, posy, chart !== 0 ? width + 5 : width + 2, height);
  // the border line, still in the background colour (MoveListFrame.java:3322-3334)
  g.stroke(1);
  g.line(posx + 1, posy + 1, posx - 1 + width, posy + 1);
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
      g.color(WHITE);
      g.line(0, posy, origW + 5, posy);
      g.line(0, posy + height, origW + 5, posy + height);
    }
  }

  // grid lines and their labels (MoveListFrame.java:3363-3451)
  g.color(WHITE);
  g.stroke(1, DASH);
  let gridLines = 3;
  if (chart === 2 || chart === 3) gridLines = 7;
  if (chart === 4) gridLines = 4;
  if (chart > 4) gridLines = 5;
  if (chart >= 7) gridLines = 3;
  g.font(10, true);
  for (let i = 1; i <= gridLines; i++) {
    const percent = (i * 100.0) / (gridLines + 1);
    const y = posy + height - jint((height * percent) / 100);
    if (chart === 2 || chart === 3) {
      const label = ["+20", "+5", "0", "-5", "-10", "-20", "-50"][i - 1];
      if (i === 3) g.stroke(1);
      g.text(label, 3, y + 3);
      g.line(posx, y, posx + width - 3, y);
      g.stroke(1, DASH);
    } else if (chart === 4) {
      if (i === 1) {
        g.stroke(1);
        g.line(0, y, posx + width, y);
        g.stroke(1, DASH);
      } else {
        g.color(LIGHT_GRAY);
        g.line(posx + 17, y, posx + width - 25, y);
        g.color(WHITE);
        g.text(String(idiv(100 * (i - 1), 4)), 3, y + 3);
      }
    } else if (chart > 4 && chart < 7) {
      if (i === 2) {
        g.color(WHITE);
        g.stroke(1);
        g.line(0, y, posx + width, y);
        g.stroke(1, DASH);
        g.color(LIGHT_GRAY);
      } else if (i > 2) {
        g.color(LIGHT_GRAY);
        g.line(posx, y, posx + width - 25, y);
      }
    } else {
      if (chart >= 7) {
        g.color(GRAY);
        g.line(posx, y, posx + width, y);
      } else g.line(posx, y, posx + width - 25, y);
      if (chart < 5) g.text(String(idiv(100 * i, 4)), 3, y + 3);
    }
  }

  if (chart === 5 || chart === 6) drawMisses(g, p, posx, posy, width, height, gridLines, chart === 6);
  if (chart === 4) {
    drawStages(g, p, posx, posy, width, height);
    return null;
  }
  if (chart === 5 || chart === 6) return null; // the rest of draw paints nothing for these

  g.stroke(2);
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
    g.font(FRAME_FONT_SIZE - 2, false);
    const heightFont = g.ascentMinusDescent();
    g.color(WHITE);
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
            g.color(black ? MATCH_BLACK : MATCH_WHITE);
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
            g.color(inRange ? CYAN : ORANGE);
            g.line(xOf(lastOkMove), yOf(lastWr), xOf(movenum), yOf(wr));
          } else {
            g.color(ORANGE);
            const lostMoves = lastOkMove - movenum;
            if (lostMoves < 35)
              g.line(xOf(lastOkMove), yOf(lastWr), xOf(movenum + 1), posy + height - jint(((wr - (wr - lastWr) / lostMoves) * height) / 100));
            g.color(inRange ? CYAN : ORANGE);
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
        g.color(black ? "rgba(0,0,0,0.784)" : "rgba(255,255,255,0.784)");
        const lastMatchValue = black ? lastMatchValueB : lastMatchValueW;
        if (k !== total)
          g.line(
            xOf(movenum + 2),
            posy + height - jint(((lastMatchValue > 0 ? lastMatchValue : matchValue) * height) / 100),
            xOf(movenum),
            jint(posy + height - (matchValue * height) / 100),
          );
      }
      if (!Number.isNaN(blackValue)) lastMatchValueB = blackValue;
      if (!Number.isNaN(whiteValue)) lastMatchValueW = whiteValue;
      if (k === cur) {
        const x = posx + idiv((cur - 1) * width, numMoves);
        g.stroke(1, DASH);
        g.color(YELLOW);
        g.line(x, 0, x, posy + origH);
        g.color(BLACK);
        g.text(String(cur), x + 3, posy + origH - 10);
        g.stroke(2);
        g.font(15, true);
        const hi = posy + height - jint((Math.max(lastMatchValueB, lastMatchValueW) * height) / 100) - 4;
        const lo = posy + height - jint((Math.min(lastMatchValueB, lastMatchValueW) * height) / 100) + 16;
        const bx = xOf(movenum) - 5;
        const blackHigh = lastMatchValueB >= lastMatchValueW;
        g.color(BLACK);
        if (rb.count >= 1) g.text(fixed(lastMatchValueB, 1), bx, blackHigh ? hi : lo);
        g.color(WHITE);
        if (rw.count >= 1) g.text(fixed(lastMatchValueW, 1), bx, blackHigh ? lo : hi);
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
        if (mv.mover === "Black" && p.black) {
          g.color(BLACK);
          drawn = true;
        } else if (mv.mover === "White" && p.white) {
          g.color(WHITE);
          drawn = true;
        }
        if (drawn) {
          convert = convertHeight(diff);
          if (score) convert = Math.min(100, Math.max(-100, convert));
          const x = posx + jint(((movenum + (single ? 0.5 : 1)) * width) / numMoves);
          const w = idiv((single ? 2 : 1) * width, numMoves) - (single ? 1 : 0);
          const base = posy + height - idiv(height * 3, 8);
          if (diff > 0) g.rect(x, base, w, jint((convert * height) / 100));
          else g.rect(x, jint(base + (convert * height) / 100), w, jint((Math.abs(convert) * height) / 100));
        }
      }
      if (cur > 0 && k === cur - 1) {
        const x = posx + jint(((movenum + 1.5) * width) / numMoves);
        g.stroke(1, DASH);
        g.color(YELLOW);
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
  g.color(WHITE);
  g.font(10, true);
  const step = numMoves <= 100 ? 10 : 20;
  if (chart === 2 || chart === 3) {
    // String concatenation as Java does: "0" + 10 → "010" (MoveListFrame.java:5685-5687)
    const prefix = String(first > 0 ? first : 0);
    for (let i = 1; i <= idiv(numMoves, step); i++)
      g.text(prefix + String(i * step), posx + idiv((i * step - 1) * width, numMoves) + 3, posy + origH - 5);
    const x = posx + idiv((cur - 1 - (first - 1 > 0 ? first : 0)) * width, numMoves);
    g.color(BLACK);
    g.font(10, true);
    g.text(String(cur), x + 3, posy + origH - 5);
    if (cmovenumCase2 > 0) {
      g.font(13, true);
      const text = fixed(cdiffwinCase2, 1);
      const mw = g.width(text);
      g.color(RED);
      const x2 = posx + jint(((cmovenumCase2 + 1.5) * width) / numMoves - idiv(mw, 2));
      if (cdiffwinCase2 <= 0) g.text(text, x2, jint(posy + height + 13 - idiv(height * 3, 8)));
      else g.text("+" + text, x2, jint(posy + height + 13 + (cConvertHeight / 100) * height - idiv(height * 3, 8)));
    }
  } else {
    for (let i = 1; i <= idiv(numMoves, step); i++)
      g.text(String(i * step), posx + idiv((i * step - 1) * width, numMoves) + 3, posy + origH - 10);
  }

  if (chart === 0) {
    // the current and mouse-over positions, the score line (MoveListFrame.java:5757-5941)
    let x = posx + idiv((cur - 1) * width, numMoves);
    g.stroke(1, DASH);
    g.color(YELLOW);
    g.line(x, 0, x, posy + origH);
    g.color(BLACK);
    g.text(String(cur), x + 3, posy + origH - 10);
    if (mmovenum > 0 && hoverMove !== null) {
      x = posx + idiv((hoverMove - 1) * width, numMoves);
      g.color(WHITE);
      g.line(x, 0, x, posy + origH);
      g.color(BLACK);
      g.text(String(hoverMove), x + 3, posy + origH - 10);
    }
    g.stroke(2);
    if (lastOkMoveSave > 0 && !firstNodeAnalyzed) {
      g.color(ORANGE);
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
          g.color(SCORE_COLOR);
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
      g.color(WHITE);
      g.font(13, true);
      g.text(fixed(s, 1), numMoves - n < 2 ? xOf(n) - 13 : xOf(n), scoreY(s));
    };
    if (cmovenum > 0) {
      g.font(16, true);
      const wrString = fixed(cwr, 1);
      const strWidth = g.width(wrString);
      const cPosX = xOf(cmovenum) - shift;
      if (Math.abs(cPosX - mPosX) > strWidth) {
        g.color(BLACK);
        g.text(wrString, cPosX, textY(cwr));
        g.color(MAGENTA);
        g.oval(xOf(cmovenum) - DOT_RADIUS, posy + height - jint((cwr * height) / 100) - DOT_RADIUS, DOT_RADIUS * 2, DOT_RADIUS * 2);
        if (cScore > -500) scoreText(cScore, cmovenum);
      }
    }
    if (mmovenum > 0) {
      g.font(16, true);
      g.color(BLACK);
      g.text(fixed(mwr, 1), mPosX, textY(mwr));
      g.color(YELLOW);
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
    g.color(YELLOW);
    g.stroke(1.5, [5]);
    g.line(cx, 0, cx, height);
  }
  g.color(move.mover === "Black" ? BLACK : WHITE);
  const winrate = move.winrate ?? 50;
  const yc = jint(height * 0.15 + (height * 0.7 * (100 - winrate)) / 100.0);
  g.oval(cx - idiv(height, 26), yc - idiv(height, 26), idiv(height, 13), idiv(height, 13));
  g.poly(
    [cx - idiv(height, 40), cx - idiv(height, 55), cx - idiv(height, 10)],
    [yc, yc - idiv(height, 30), yc - idiv(height, 10)],
  );
  g.font(FRAME_FONT_SIZE - 2, false);
  const label = String(move.move_number);
  g.text(label, cx - idiv(g.width(label), 2), height - 5);
  g.font(FRAME_FONT_SIZE, false);
  g.text(fixed(change, 1) + (isScore ? "" : "%"), cx + idiv(height, 13), yc);
}

/** 胜率损失统计 / 目数损失统计 histograms (MoveListFrame.java:3452-4696). */
function drawMisses(g: G, p: HawkeyePayload, posx: number, posy: number, width: number, height: number, gridLines: number, score: boolean): void {
  const misses = score ? p.pane.score_misses : p.pane.winrate_misses;
  const s = p.settings;
  const r1 = score ? s.scoreRange1 : s.winrateRange1;
  const r2 = score ? s.scoreRange2 : s.winrateRange2;
  const unit = score ? "" : "%";
  g.font(12, true);
  g.color(BLACK);
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
  for (const side of [0, 1]) {
    if (side === 1) g.color(WHITE);
    for (let group = 0; group < 4; group++)
      for (let b = 0; b < 3; b++) {
        const count = group === 0 ? all(side, b) : misses[side][group - 1][b];
        const max = group === 0 ? allMax : stageMax(group - 1);
        const k = [2, 20, 38, 56][group] + 5 * b + 2 * side;
        const h = jint(unitH * (count / max));
        g.rect(posx + idiv(k * width, 72), y - h, idiv(width, 36), h);
        const text = String(count);
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
  g.font(12, true);
  g.color(BLACK);
  const labelY = posy + idiv(height * 4, 5) + 15;
  const centred = (text: string, k: number) => g.text(text, posx + idiv(k * width, 16) - idiv(g.width(text), 2), labelY);
  centred(t("Movelistframe.all"), 2);
  centred(t("Movelistframe.open") + "(1-" + o + ")", 6);
  centred(t("Movelistframe.middle") + "(" + (o + 1) + "-" + m + ")", 10);
  centred(t("Movelistframe.end") + "(" + (m + 1) + t("Movelistframe.toEnd") + ")", 14);
  const bar = (v: number, k: number, at: number) => {
    const h = jint(v > 0.75 ? ((v - 0.75) * height * 4) / 7 + idiv(height * 3, 5) : (v * height * 4) / 5);
    const top = posy + idiv(height * 4, 5) - h;
    g.rect(posx + idiv(k * width, 16), top, idiv(width, 16), h);
    const text = fixed(v * 100, 1);
    g.text(text, jint(posx + (at * width) / 16 - idiv(g.width(text), 2)), top - 3);
  };
  for (const side of [0, 1]) {
    if (side === 1) g.color(WHITE);
    const st = stages[side];
    const allAnalyzed = st[0].match_analyzed + st[1].match_analyzed + st[2].match_analyzed;
    if (allAnalyzed > 0) bar((st[0].share_sum + st[1].share_sum + st[2].share_sum) / allAnalyzed, 1 + side, 1.5 + side);
    for (let n = 0; n < 3; n++) if (st[n].analyzed > 0) bar(share(st[n]), 5 + 4 * n + side, 5.5 + 4 * n + side);
  }
}

// ------------------------------------------------------------------ drawMin (matchPanelmin)

/** MoveListFrame.drawMin (MoveListFrame.java:2100-3161). */
function drawMin(g: G, p: HawkeyePayload, w: number, h: number): void {
  g.color(PANEL_BG);
  g.rect(0, 0, w, h);
  const chart = p.settings.chart;
  const pane = p.pane;
  const s = p.settings;
  g.color(BLACK);
  g.font(13, false);
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
    [0, 1].forEach((side) => {
      const sd = pane.sides[side];
      const percent = fixed(Math.fround(Math.fround(sd.matched / (sd.match_analyzed > 0 ? sd.match_analyzed : 1)) * 100), 1) + "%";
      g.text(
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
    g.text(t("Movelistframe.allmoves") + pane.total_moves + t("Movelistframe.analyzed") + analyzedMatch, 491, 15);
    // the legend (MoveListFrame.java:2759-2807)
    const at = p.chinese
      ? { b: [500, 520], w: [560, 580], a: [622, 645], u: [688, 711], s: [754, 777] }
      : { b: [500, 521], w: [592, 614], a: [688, 711], u: [768, 791], s: [861, 884] };
    g.color(MATCH_BLACK);
    g.rect(at.b[0], 23, 20, 14);
    g.color(BLACK);
    g.text(t("Movelistframe.blackMatchLabel"), at.b[1], 35);
    g.color(MATCH_WHITE);
    g.rect(at.w[0], 23, 20, 14);
    g.color(BLACK);
    g.text(t("Movelistframe.whiteMatchLabel"), at.w[1], 35);
    g.color(CYAN);
    g.stroke(3);
    g.line(at.a[0], 30, at.a[0] + 18, 30);
    g.color(BLACK);
    g.text(t("Movelistframe.analyzedLabel"), at.a[1], 35);
    g.color(ORANGE);
    g.stroke(3);
    g.line(at.u[0], 30, at.u[0] + 18, 30);
    g.color(BLACK);
    g.text(t("Movelistframe.unanalyzedLabel"), at.u[1], 35);
    g.color(SCORE_COLOR);
    g.stroke(1);
    g.line(at.s[0], 30, at.s[0] + 18, 30);
    g.color(BLACK);
    g.text(t("Movelistframe.scoreLeadLabel"), at.s[1], 35);
  } else if (chart === 1) {
    const [b, wt] = pane.sides;
    g.text(t("Movelistframe.blackAIScore") + fixed((b.share_sum * 100) / b.match_analyzed, 2), 25, 15);
    g.text(t("Movelistframe.whiteAIScore") + fixed((wt.share_sum * 100) / wt.match_analyzed, 2), 25, 35);
    // Chinese counts the match-analysed moves, English the loss-analysed ones (MoveListFrame.java:2830-2872)
    const x0 = p.chinese ? 160 : 175;
    const counted = p.chinese ? b.match_analyzed + wt.match_analyzed : b.analyzed + wt.analyzed;
    g.text(t("Movelistframe.allmoves") + pane.total_moves + t("Movelistframe.analyzed") + counted, x0, 15);
    const l = p.chinese ? { line: 172, a: 195, u: [238, 256], ut: 261 } : { line: 182, a: 205, u: [268, 280], ut: 285 };
    g.color(BLACK);
    g.stroke(3);
    g.line(l.line, 33, l.line + 18, 33);
    g.color(LIGHT_GRAY);
    g.line(l.line, 27, l.line + 18, 27);
    g.color(BLACK);
    g.text(t("Movelistframe.analyzedLabel"), l.a, 35);
    g.color(ORANGE);
    g.line(l.u[0], 30, l.u[1], 30);
    g.color(BLACK);
    g.text(t("Movelistframe.unanalyzedLabel"), l.ut, 35);
  } else if (chart === 4) {
    g.font(12, false);
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
    g.font(12, false);
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

// ------------------------------------------------------------------ the tab strip

interface TabBox {
  chart: number;
  label: string;
  x: number;
  y: number;
  w: number;
}

/** JTabbedPane WRAP_TAB_LAYOUT: runs of tabs, the selected tab's run next to the content. */
function layoutTabs(g: G, width: number, selected: number): { boxes: TabBox[]; strip: number } {
  g.font(FRAME_FONT_SIZE, false);
  const runs: TabBox[][] = [[]];
  let x = 2;
  for (const chart of TAB_CHARTS) {
    const label = tabLabel(chart);
    const w = g.width(label) + TAB_PAD;
    if (x + w > width - 2 && runs[runs.length - 1].length) {
      runs.push([]);
      x = 2;
    }
    runs[runs.length - 1].push({ chart, label, x, y: 0, w });
    x += w;
  }
  const at = runs.findIndex((run) => run.some((b) => b.chart === selected));
  if (at >= 0 && at !== runs.length - 1) runs.push(...runs.splice(at, 1));
  runs.forEach((run, r) => run.forEach((b) => (b.y = r * TAB_ROW)));
  return { boxes: runs.flat(), strip: runs.length * TAB_ROW };
}

/** The tab strip and the content border (Metal Ocean as on j05-hawkeye.png). */
function paintFrame(g: G, w: number, h: number, boxes: TabBox[], strip: number, selected: number): void {
  const c = g.c;
  g.color(PANEL_BG);
  g.rect(0, 0, w, h);
  g.stroke(1);
  // content border: dark top line, white line, two selected-colour lines; the other sides
  g.color(OCEAN_SELECTED);
  g.rect(0, strip + 2, w, h - strip - 2);
  g.color(PANEL_BG);
  g.rect(INSET, strip + CONTENT_TOP, w - 2 * INSET, h - strip - CONTENT_TOP - INSET);
  g.color(OCEAN_DARK);
  g.rect(0, strip, w, 1);
  g.color(WHITE);
  g.rect(0, strip + 1, w, 1);
  g.color(OCEAN_BORDER);
  g.rect(0, strip + 1, 1, h - strip - 1);
  g.rect(w - 1, strip + 1, 1, h - strip - 1);
  g.rect(0, h - 1, w, 1);
  g.font(FRAME_FONT_SIZE, false);
  const outline = (b: TabBox, top: number, bottom: number) => {
    c.beginPath();
    c.moveTo(b.x + 0.5, bottom);
    c.lineTo(b.x + 0.5, top + 6.5);
    c.lineTo(b.x + 6.5, top + 0.5);
    c.lineTo(b.x + b.w + 0.5, top + 0.5);
    c.lineTo(b.x + b.w + 0.5, bottom);
  };
  for (const b of boxes) {
    if (b.chart === selected) continue;
    const top = b.y + 2;
    const bottom = b.y + TAB_ROW;
    outline(b, top, bottom);
    c.fillStyle = TAB_BG;
    c.fill();
    c.strokeStyle = OCEAN_BORDER;
    c.stroke();
    g.color(BLACK);
    g.text(b.label, b.x + Math.round((b.w - g.width(b.label)) / 2), b.y + 14);
  }
  const b = boxes.find((x) => x.chart === selected);
  if (b) {
    const bottom = b.y + TAB_ROW + 1;
    outline(b, b.y, bottom);
    c.fillStyle = OCEAN_SELECTED;
    c.fill();
    c.strokeStyle = OCEAN_DARK;
    c.stroke();
    c.beginPath();
    c.moveTo(b.x + 1.5, bottom);
    c.lineTo(b.x + 1.5, b.y + 6.5);
    c.lineTo(b.x + 6.5, b.y + 1.5);
    c.lineTo(b.x + b.w - 0.5, b.y + 1.5);
    c.strokeStyle = WHITE;
    c.stroke();
    g.color(BLACK);
    g.text(b.label, b.x + Math.round((b.w - g.width(b.label)) / 2), b.y + 14);
  }
}

// ------------------------------------------------------------------ the panel

const STYLE_ID = "hawkeye-charts-style";
const STYLE = `
.hk-charts { overflow: hidden; background: ${PANEL_BG}; user-select: none; }
.hk-charts canvas { position: absolute; display: block; }
.hk-charts .hk-tab { position: absolute; cursor: default; }
.hk-charts .hk-min { position: absolute; overflow: hidden; }
.hk-charts .hk-min > * { position: absolute; box-sizing: border-box; margin: 0; }
.hk-charts .hk-label { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; font: 12px ${UI_FONT}; line-height: 20px; color: #000; }
.hk-charts .hk-field { border: 1px solid ${OCEAN_BORDER}; background: #fff; font: 12px ${UI_FONT}; padding: 0 2px; outline: none; color: #000; }
.hk-charts .hk-field.invalid { background: rgb(255,0,0); }
.hk-charts .hk-button { padding: 0; border: 1px solid ${OCEAN_BORDER}; background: rgb(240,242,245); display: flex; align-items: center; justify-content: center; }
.hk-charts .hk-button:active { background: ${OCEAN_SELECTED}; }
.hk-charts .hk-button img { display: block; max-width: 16px; max-height: 16px; pointer-events: none; }
`;

type Bounds = [number, number, number, number];

export function mountCharts(host: HTMLElement, actions: ChartActions): ChartPanel {
  if (!document.getElementById(STYLE_ID)) {
    const style = document.createElement("style");
    style.id = STYLE_ID;
    style.textContent = STYLE;
    document.head.append(style);
  }
  host.classList.add("hk-charts");
  host.replaceChildren();

  const frame = document.createElement("canvas");
  frame.style.left = "0px";
  frame.style.top = "0px";
  const tabLayer = document.createElement("div");
  const chartCanvas = document.createElement("canvas");
  chartCanvas.dataset.key = "hawkeye.chart.canvas";
  const min = document.createElement("div");
  min.className = "hk-min";
  const minCanvas = document.createElement("canvas");
  minCanvas.style.left = "0px";
  minCanvas.style.top = "0px";

  const button = (key: string, icon: string, tip?: string) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "hk-button";
    b.dataset.key = key;
    b.tabIndex = -1; // setFocusable(false)
    if (tip) b.title = tip;
    const img = document.createElement("img");
    img.alt = "";
    img.src = icon;
    b.append(img);
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
    f.className = "hk-field";
    f.dataset.key = key;
    f.spellcheck = false;
    f.addEventListener("focus", () => f.select()); // focusGained → selectAll
    return f;
  };

  // matchPanelmin controls (MoveListFrame.java:763-1132, 6996-7063)
  const detail = button("hawkeye.chart.detail", "/assets/down.png");
  const settings = button("hawkeye.chart.settings", "/assets/settings.png", t("Movelistframe.settingsToolTip"));
  const lblMatch1 = label(t("Movelistframe.lblMatchConfig1"));
  const lblMatch2 = label(t("Movelistframe.lblMatchConfig2"));
  const lblMatch3 = label("%");
  const lblMatch4 = label(t("Movelistframe.accuracyAndMatch.tip"));
  const suggestionMoves = field("hawkeye.chart.match-moves");
  const percentPlayouts = field("hawkeye.chart.match-percent");
  const lblDiff1 = label(t("Movelistframe.lblDiffConfig1"));
  const lblDiff2 = label(t("Movelistframe.lblDiffConfig2"));
  const range1 = field("hawkeye.chart.range1");
  const range2 = field("hawkeye.chart.range2");
  min.append(minCanvas, detail, settings, lblMatch1, suggestionMoves, lblMatch2, percentPlayouts, lblMatch3, lblMatch4, lblDiff1, range1, lblDiff2, range2);
  host.append(frame, chartCanvas, min, tabLayer);

  let payload: HawkeyePayload | null = null;
  let line: Line | null = null;
  let params: Params | null = null;
  let boxes: TabBox[] = [];
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
    const cn = p.chinese;
    const tabChanged = panelChart !== chart;
    panelChart = chart;
    const all = [detail, settings, lblMatch1, suggestionMoves, lblMatch2, percentPlayouts, lblMatch3, lblMatch4, lblDiff1, range1, lblDiff2, range2];
    for (const el of all) place(el, null);
    if (chart === 2 || chart === 3 || chart === 5 || chart === 6) {
      const score = chart === 3 || chart === 6;
      lblDiff1.textContent = score ? t("Movelistframe.lblDiffConfigScore") : t("Movelistframe.lblDiffConfig1");
      place(lblDiff1, [5, 40, 235, 20]);
      place(range1, [cn ? 140 : 165, 43, 25, 16]);
      place(lblDiff2, [cn ? 170 : 195, 40, 155, 20]);
      place(range2, [cn ? 224 : 244, 43, 25, 16]);
      setField(range1, String(score ? p.settings.scoreRange1 : p.settings.winrateRange1), tabChanged);
      setField(range2, String(score ? p.settings.scoreRange2 : p.settings.winrateRange2), tabChanged);
    } else if (chart < 7) {
      place(detail, [0, 1, 20, 20]);
      place(settings, [0, 20, 20, 20]);
      const img = detail.firstElementChild as HTMLImageElement;
      const icon = p.settings.showGraph ? "/assets/down.png" : "/assets/up.png";
      if (img.getAttribute("src") !== icon) img.src = icon;
      if (chart === 0) {
        place(lblMatch1, [5, 40, 166, 20]);
        place(suggestionMoves, [cn ? 84 : 123, 43, 25, 16]);
        place(lblMatch2, [cn ? 110 : 150, 40, 250, 20]);
        place(percentPlayouts, [cn ? 258 : 363, 43, 40, 16]);
        place(lblMatch3, [cn ? 299 : 405, 40, 15, 20]);
        place(lblMatch4, [cn ? 320 : 435, 40, 500, 20]);
        setField(suggestionMoves, String(p.matchAiMoves), tabChanged);
        // Java String.valueOf(double): 20.0, 12.5
        const percent = p.matchAiPercents;
        setField(percentPlayouts, Number.isInteger(percent) ? `${percent}.0` : String(percent), tabChanged);
      } else place(lblMatch4, [5, 40, 500, 20]);
    }
  };

  const paintChart = () => {
    const p = payload;
    if (!p || !line || chartCanvas.style.display === "none") return;
    const w = chartCanvas.clientWidth || parseFloat(chartCanvas.style.width) || 0;
    const h = chartCanvas.clientHeight || parseFloat(chartCanvas.style.height) || 0;
    if (w <= 0 || h <= 0) return;
    const g = fit(chartCanvas, w, h);
    g.color(PANEL_BG);
    g.rect(0, 0, w, h);
    const done = draw(g, p, line, w - 5, h, hover);
    if (done) params = done;
  };

  const paintMin = () => {
    const p = payload;
    if (!p || min.style.display === "none") return;
    const w = min.clientWidth || parseFloat(min.style.width) || 0;
    if (w <= 0) return;
    drawMin(fit(minCanvas, w, MIN_HEIGHT), p, w, MIN_HEIGHT);
  };

  const paint = () => {
    const p = payload;
    if (!p) return;
    const W = host.clientWidth;
    const H = host.clientHeight;
    const g = fit(frame, Math.max(1, W), Math.max(1, H));
    const chart = p.settings.chart;
    const tabs = layoutTabs(g, W, chart);
    boxes = tabs.boxes;
    paintFrame(g, W, H, boxes, tabs.strip, chart);

    // tab hit areas
    tabLayer.replaceChildren(
      ...boxes.map((b) => {
        const d = document.createElement("div");
        d.className = "hk-tab";
        d.dataset.key = `hawkeye.chart.tab.${b.chart}`;
        d.dataset.text = b.label;
        if (b.chart === chart) d.dataset.selected = "true";
        d.style.left = `${b.x}px`;
        d.style.top = `${b.y}px`;
        d.style.width = `${b.w}px`;
        d.style.height = `${TAB_ROW}px`;
        d.addEventListener("mousedown", (e) => {
          if (e.button !== 0 || !payload || payload.settings.chart === b.chart) return;
          actions.run(settingsCommand(payload.settings, { chart: b.chart }));
        });
        return d;
      }),
    );

    const top = tabs.strip + CONTENT_TOP;
    const inner = Math.max(0, W - 2 * INSET);
    const contentH = Math.max(0, H - top - INSET);
    const hasMin = chart < 7;
    if (hasMin) {
      place(min, [INSET, Math.max(top, H - INSET - MIN_HEIGHT), inner, MIN_HEIGHT]);
      layoutControls(p);
    } else place(min, null);
    const chartH = Math.max(0, contentH - (hasMin ? MIN_HEIGHT : 0));
    if (p.settings.showGraph && chartH > 0 && inner > 0) {
      chartCanvas.style.display = "";
      chartCanvas.style.left = `${INSET}px`;
      chartCanvas.style.top = `${top}px`;
      chartCanvas.style.width = `${inner}px`;
      chartCanvas.style.height = `${chartH}px`;
    } else chartCanvas.style.display = "none";
    paintChart();
    paintMin();
  };

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
    if (number === lastGoto) return;
    lastGoto = number;
    actions.run({ type: "Hawkeye", op: { type: "GotoMoveNumber", number } });
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
      if (move) actions.run({ type: "Hawkeye", op: { type: "GotoMove", node: move.node } });
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
  const visible = (el: HTMLElement) => el.style.display !== "none" && (el.parentElement?.style.display ?? "") !== "none";

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
      for (const d of Array.from(tabLayer.children) as HTMLElement[])
        out.push({ key: d.dataset.key!, text: d.dataset.text ?? "", checked: d.dataset.selected === "true", ...centre(d) });
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
