// Win-rate graph (胜率图): the single-engine branch of WinrateGraph.draw (WinrateGraph.java:573-1680
// with helpers 186-570, 2163-3250) and its hit testing for LizzieFrame's click / drag / hover
// (LizzieFrame.java:9600-10030, 10371-10500). Values are Black's from the core (`HistoryPoint.black`);
// nothing here flips sides. Java paints three layers (background, blunder bars, lines and text) and
// composites them in that order; the painter records each layer's operations and replays them so.
import type { DisplaySettings, HistoryPoint, TreeNodeView } from "../contract/generated";
import { t } from "../i18n";
import { graphGeom, type Rect } from "../workbench/layout";
import { GRAPH_FONT } from "../workbench/paint";

const i = Math.trunc;

/** What the graph is drawn from. */
export interface GraphScene {
  nodes: Map<number, TreeNodeView>;
  current: number;
  /** Stored results by node (the view's `history`). */
  history: Map<number, HistoryPoint>;
  display: DisplaySettings;
  /** WinrateGraph.mouseOverNode, only while 显示鼠标悬停信息 is on. */
  hover: number | null;
}

/** A rendered anchor (GraphPoint) with its node's move number. */
interface Anchor {
  node: number;
  x: number;
  y: number;
  moveNumber: number;
}

/** The rendered point sources Java keeps for hit testing (renderedGraphPoints, params). */
export interface GraphHits {
  origW: number;
  origH: number;
  posx: number;
  width: number;
  numMoves: number;
  anchors: Anchor[];
  /** buildGraphNavigationTargets: the graph path's action / snapshot nodes, in path order. */
  targets: { node: number; moveNumber: number }[];
}

/** What the graph drew, for the acceptance probe. */
export interface GraphReport {
  /** Nodes plotted with a win rate (WinrateGraph's analysed points; the root never). */
  pointCount: number;
  winrateRenderable: boolean;
  scoreRenderable: boolean;
  /** Baseline chip text: "50%", "0" or null. */
  chip: string | null;
  baseline: boolean;
  fill: boolean;
  fillShapes: number;
  blunderBars: number;
  /** Separately stroked win-rate line pieces: connected runs of strokes, broken by a 25+ move gap or
   *  the jump to a fork's trunk end. Miss-coloured bridges continue a piece. 0 without the line. */
  segments: number;
  /** Win-rate strokes drawn in the miss colour (each bridges unanalysed moves under the 25 limit). */
  missSegments: number;
  numMoves: number;
  /** The score scale used, or null while no score line is drawn. */
  maxScoreLead: number | null;
  scorePointCount: number;
  anchors: number;
  /** Win-rate anchors (analysed graph-path nodes, without the current-move fallback) in graph-canvas
   *  coordinates, the space of currentMarker and of the click hit test; `move` is the move number. */
  points: { node: number; move: number; x: number; y: number }[];
  currentMarker: { node: number; x: number; y: number } | null;
  scoreMarker: { x: number; y: number } | null;
  markersOverlap: boolean;
  currentText: string | null;
  currentScoreText: string | null;
  hoverNode: number | null;
  hoverText: string | null;
  hoverScoreText: string | null;
  legend: string[];
  xLabels: string[];
}

// ------------------------------------------------------------------ colours (Java defaults)

/** resolveGraphBackgroundColor with the theme's comment background (0,0,0,200): (22,25,29,165). */
const BACKGROUND = "rgba(22,25,29,0.647)";
const GRID = "rgba(255,255,255,0.282)";
const BASELINE = "rgba(236,232,224,0.659)";
const LINE_RGB = [100, 180, 255];
const SCORE_RGB = [255, 0, 255];
const LINE = "rgb(100,180,255)";
const MISS = "rgb(240,240,240)";
const SCORE = "rgb(255,0,255)";
const GUIDE = "rgba(232,225,210,0.706)";
/** resolveBelowBaselineFillColor on the dark background. */
const BELOW_FILL = "rgba(232,225,210,0.165)";
const rgba = (c: number[], a: number) => `rgba(${c[0]},${c[1]},${c[2]},${+(a / 255).toFixed(3)})`;

const DOT_RADIUS = 3;
const ANCHOR_HALF = 2;
const MARKER_RADIUS = 4;
const MOVE_MARKER = { fill: "rgb(244,67,72)", border: rgba([112, 24, 28], 230), halo: rgba([255, 250, 240], 225) };
const SCORE_MARKER = { fill: "rgb(46,204,113)", border: rgba([20, 96, 55], 230), halo: rgba([248, 255, 245], 225) };

// ------------------------------------------------------------------ Java number helpers

/** Java's (int) cast of a double. */
function jInt(v: number): number {
  if (Number.isNaN(v)) return 0;
  return i(Math.max(-(2 ** 31), Math.min(2 ** 31 - 1, v)));
}

/** String.format(Locale.ENGLISH, "%.1f", v) for v >= 0: HALF_UP on the shortest decimal form. */
export function javaFixed1(v: number): string {
  const s = String(v);
  const m = /^(\d+)(?:\.(\d+))?$/.exec(s);
  if (!m) return v.toFixed(1);
  const frac = m[2] ?? "";
  const tenths = Number(m[1]) * 10 + Number(frac[0] ?? "0") + ((frac[1] ?? "0") >= "5" ? 1 : 0);
  return `${i(tenths / 10)}.${tenths % 10}`;
}

/** WinrateGraph.formatScoreLead (WinrateGraph.java:405-416). */
function formatScoreLead(black: number): string {
  const magnitude = javaFixed1(Math.round(Math.abs(black) * 10) / 10);
  if (magnitude === "0.0") return "0.0";
  return (black > 0 ? t("WinrateGraph.scoreLeadBlackPrefix") : t("WinrateGraph.scoreLeadWhitePrefix")) + magnitude;
}

/** resolveBlunderBarHeight (WinrateGraph.java:297-310). */
function blunderBarHeight(graphHeight: number, winrateDrop: number, scoreDrop: number): number {
  const usable = Math.max(1, graphHeight);
  const maxHeight = Math.max(4, i(usable / 3));
  const minHeight = Math.max(2, Math.min(6, maxHeight));
  const severity = Math.max(Math.abs(winrateDrop) / 30, Math.abs(scoreDrop) / 12);
  if (severity <= 0.01) return Math.max(1, Math.min(3, i(usable / 24)));
  const eased = Math.sqrt(Math.min(1, severity));
  return Math.max(1, Math.min(maxHeight, Math.floor(minHeight + (maxHeight - minHeight) * eased + 0.5)));
}

/** getBlunderColor (WinrateGraph.java:261-270). */
function blunderColor(winrateDrop: number, scoreDrop: number): string {
  if (scoreDrop > 5 || winrateDrop > 20) return rgba([235, 60, 60], 220);
  if (scoreDrop > 2 || winrateDrop > 10) return rgba([230, 140, 40], 220);
  if (scoreDrop > 1 || winrateDrop > 5) return rgba([230, 200, 50], 200);
  if (scoreDrop > 0.3 || winrateDrop > 2) return rgba([150, 180, 80], 150);
  return rgba([120, 120, 120], 50);
}

// ------------------------------------------------------------------ tree helpers (BoardHistoryNode)

const parentOf = (nodes: Map<number, TreeNodeView>, n: number): number | null => nodes.get(n)?.parent ?? null;
const nextOf = (nodes: Map<number, TreeNodeView>, n: number): number | undefined => nodes.get(n)?.children[0];
const moveNumber = (nodes: Map<number, TreeNodeView>, n: number): number => nodes.get(n)?.move_number ?? 0;

/** graphTraversalEnd: follow first children to the end. */
function lineEnd(nodes: Map<number, TreeNodeView>, n: number): number {
  for (let next = nextOf(nodes, n); next !== undefined; next = nextOf(nodes, n)) n = next;
  return n;
}

/** isFirstChild. */
function isFirstChild(nodes: Map<number, TreeNodeView>, n: number): boolean {
  const p = parentOf(nodes, n);
  return p !== null && nextOf(nodes, p) === n;
}

/** isMainTrunk. */
function isMainTrunk(nodes: Map<number, TreeNodeView>, n: number): boolean {
  for (let p = parentOf(nodes, n); p !== null; n = p, p = parentOf(nodes, n)) if (nextOf(nodes, p) !== n) return false;
  return true;
}

/** findTop: the highest node where the path leaves the first children (the fork on the trunk). */
function findTop(nodes: Map<number, TreeNodeView>, n: number): number {
  let top = n;
  for (let p = parentOf(nodes, n); p !== null; n = p, p = parentOf(nodes, n)) if (nextOf(nodes, p) !== n) top = p;
  return top;
}

/** getDepth: first-child moves below `n`. */
function depth(nodes: Map<number, TreeNodeView>, n: number): number {
  let c = 0;
  for (let next = nextOf(nodes, n); next !== undefined; next = nextOf(nodes, next)) c++;
  return c;
}

/** isGraphAnchorNode: a move, a real pass or a snapshot (dummy passes are not in the tree view). */
function isAnchorNode(nodes: Map<number, TreeNodeView>, n: number): boolean {
  const kind = nodes.get(n)?.kind;
  return kind === "Black" || kind === "White" || kind === "Snapshot";
}

/** displayedDefaultAnchorWinrate (WinrateGraph.java:2340-2353, 2464-2479): Black's win rate of an
 *  analysed node, the fallback for an unanalysed snapshot, else null. */
function displayedWinrate(s: GraphScene, n: number, fallback: number): number | null {
  const p = s.history.get(n);
  if (p) return Math.max(0, Math.min(100, p.black.winrate));
  if (s.nodes.get(n)?.kind !== "Snapshot") return null;
  return Math.max(0, Math.min(100, fallback));
}

// ------------------------------------------------------------------ drawing primitives

type Op = () => void;

function metrics(g: CanvasRenderingContext2D) {
  const m = g.measureText("0");
  return { ascent: i(0.95 + m.fontBoundingBoxAscent), descent: i(0.95 + m.fontBoundingBoxDescent) };
}
const stringWidth = (g: CanvasRenderingContext2D, s: string) => Math.round(g.measureText(s).width);
const plain = (size: number) => `${size}px ${GRAPH_FONT}`;
const bold = (size: number) => `bold ${size}px ${GRAPH_FONT}`;

/** drawLine with a 1 px aligned stroke. */
function line(g: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, color: string, width: number, dashed: boolean) {
  g.strokeStyle = color;
  g.lineWidth = width;
  g.setLineDash(dashed ? [4, 4] : []);
  g.beginPath();
  g.moveTo(x1 + 0.5, y1 + 0.5);
  g.lineTo(x2 + 0.5, y2 + 0.5);
  g.stroke();
  g.setLineDash([]);
}

/** fillOval(cx - r, cy - r, 2r, 2r). */
function oval(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, color: string) {
  g.fillStyle = color;
  g.beginPath();
  g.arc(cx, cy, r, 0, Math.PI * 2);
  g.fill();
}

/** drawMetricMarkerCore. */
function markerCore(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, c: typeof MOVE_MARKER) {
  oval(g, cx, cy, r, c.fill);
  g.strokeStyle = c.border;
  g.lineWidth = 1;
  g.beginPath();
  g.arc(cx, cy, r, 0, Math.PI * 2);
  g.stroke();
}

/** drawMetricMarker: the halo, then the core. */
function marker(g: CanvasRenderingContext2D, cx: number, cy: number, radius: number, c: typeof MOVE_MARKER) {
  const r = Math.max(3, radius);
  oval(g, cx, cy, r + 2, c.halo);
  markerCore(g, cx, cy, r, c);
}

interface Box { x: number; y: number; w: number; h: number }

const intersects = (a: Box, b: Box) => a.w > 0 && a.h > 0 && b.w > 0 && b.h > 0 && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

function clampBox(box: Box, bounds: Box): Box {
  const x = box.w >= bounds.w ? bounds.x : Math.max(bounds.x, Math.min(box.x, bounds.x + bounds.w - box.w));
  const y = box.h >= bounds.h ? bounds.y : Math.max(bounds.y, Math.min(box.y, bounds.y + bounds.h - box.h));
  return { x, y, w: box.w, h: box.h };
}

/** placeGraphLabelBox (WinrateGraph.java:460-482). */
function placeLabel(preferred: Box, bounds: Box, occupied: Box[]): Box {
  const box = clampBox(preferred, bounds);
  if (!occupied.some((o) => intersects(box, o))) return box;
  const step = Math.max(1, preferred.h + 1);
  for (const dy of [-step, step, -2 * step, 2 * step, -3 * step, 3 * step])
    for (const dx of [0, preferred.w + 4, -(preferred.w + 4)]) {
      const candidate = clampBox({ x: preferred.x + dx, y: preferred.y + dy, w: preferred.w, h: preferred.h }, bounds);
      if (!occupied.some((o) => intersects(candidate, o))) return candidate;
    }
  return box;
}

// ------------------------------------------------------------------ painter

/** WinrateGraph.scoreAjustMove / scoreAjustBelow: the current score text keeps its side while the
 *  two labels nearly coincide (WinrateGraph.java:1590-1615). */
let scoreAdjustMove = -10;
let scoreAdjustBelow = false;

/** WinrateGraph.draw (single engine). Returns what was drawn and the hit-test sources (null when
 *  nothing is plotted). */
export function paintGraph(g: CanvasRenderingContext2D, r: Rect, s: GraphScene): { report: GraphReport; hits: GraphHits | null } {
  const W0 = r.w;
  const H0 = r.h;
  const d = s.display;
  const nodes = s.nodes;
  const background: Op[] = [];
  const blunder: Op[] = [];
  const front: Op[] = [];
  const flush = () => {
    for (const op of background) op();
    for (const op of blunder) op();
    for (const op of front) op();
    g.setLineDash([]);
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
  };
  g.textAlign = "left";
  g.textBaseline = "alphabetic";

  // currentRenderableMetrics: the score line needs a score source (Java: KataGo / Sai engine or a
  // KataGo record), here any stored lead on the graph's nodes.
  let scoreAvailable = false;
  for (const p of s.history.values()) if (p.black.score_lead !== null) { scoreAvailable = true; break; }
  const winrateRenderable = d.winrateLine;
  const scoreRenderable = d.scoreLeadLine && scoreAvailable;
  const renderable = Number(winrateRenderable) + Number(scoreRenderable);
  const chip = renderable !== 1 ? null : winrateRenderable ? "50%" : "0";
  const baselineActive = renderable > 0;
  const fillEnabled = d.winrateGraphFill && renderable === 1;
  const report: GraphReport = {
    pointCount: 0,
    winrateRenderable,
    scoreRenderable,
    chip,
    baseline: baselineActive,
    fill: fillEnabled,
    fillShapes: 0,
    blunderBars: 0,
    segments: 0,
    missSegments: 0,
    numMoves: 0,
    maxScoreLead: null,
    scorePointCount: 0,
    anchors: 0,
    points: [],
    currentMarker: null,
    scoreMarker: null,
    markersOverlap: false,
    currentText: null,
    currentScoreText: null,
    hoverNode: s.hover,
    hoverText: null,
    hoverScoreText: null,
    legend: [],
    xLabels: [],
  };

  background.push(() => {
    g.fillStyle = BACKGROUND;
    g.fillRect(0, 0, W0, H0);
  });
  const posx = 0;
  const posy = 2;
  let width = W0 - 6;
  const height = H0 - 4;
  // Dashed grid at 25 / 50 / 75 %; the highlighted baseline replaces the midline.
  for (let k = 1; k <= 3; k++) {
    if (baselineActive && k * 2 === 4) continue;
    const y = posy + height - jInt((height * ((k * 100) / 4)) / 100);
    const w = width;
    background.push(() => line(g, posx, y, posx + w, y, GRID, 1, true));
  }
  const baselineY = posy + i(height / 2);
  const occupied: Box[] = [];
  let chipBox: Box | null = null;
  if (chip) {
    g.font = plain(11);
    const fm = metrics(g);
    const h = fm.ascent + fm.descent + 2;
    chipBox = { x: posx + 4, y: baselineY - i(h / 2), w: stringWidth(g, chip) + 8, h };
    occupied.push(chipBox);
  }
  /** paintHighlightedBaseline (WinrateGraph.java:1730-1759). */
  const paintBaseline = (w: number) => {
    if (!baselineActive) return;
    background.push(() => {
      line(g, posx, baselineY, posx + w, baselineY, BASELINE, 1.5, false);
      if (!chip || !chipBox) return;
      g.font = plain(11);
      const fm = metrics(g);
      g.fillStyle = "rgba(0,0,0,0.667)";
      g.beginPath();
      g.roundRect(chipBox.x, chipBox.y, chipBox.w, chipBox.h, 3);
      g.fill();
      g.fillStyle = rgba([236, 232, 224], 230);
      g.fillText(chip, chipBox.x + 4, chipBox.y + fm.ascent + 1);
    });
  };

  const cur = s.current;
  if (!nodes.has(cur)) {
    paintBaseline(width);
    flush();
    return { report, hits: null };
  }
  const variationTop = isMainTrunk(nodes, cur) ? null : findTop(nodes, cur);
  let numMoves = variationTop === null ? 0 : depth(nodes, variationTop) + moveNumber(nodes, variationTop) - 1;
  const end = lineEnd(nodes, cur);
  if (numMoves < moveNumber(nodes, end) - 1) numMoves = moveNumber(nodes, end) - 1;
  if (numMoves < 1) {
    paintBaseline(width);
    flush();
    return { report, hits: null };
  }
  const geo = graphGeom(W0, H0, numMoves);
  numMoves = geo.slots;
  width = geo.width;
  report.numMoves = numMoves;
  const X = (moveIndex: number) => posx + i((moveIndex * width) / numMoves);
  const WY = (wr: number) => posy + height - jInt((wr * height) / 100);
  const clampDotY = (y: number) => Math.max(0, Math.min(H0 - DOT_RADIUS * 2, y));
  const activeCurve = scoreRenderable && !winrateRenderable ? SCORE_RGB : LINE_RGB;
  const aboveFill = rgba(activeCurve, 52);

  /** drawFilledGraphLine: the fill under the segment (only for analysed neighbours), then the line. */
  const filledLine = (missing: boolean, x1: number, y1: number, x2: number, y2: number, color: string, stroke: number, dashed: boolean) => {
    if (fillEnabled && !missing && !(y1 === baselineY && y2 === baselineY)) {
      const polygon = (above: boolean, points: number[][]) => {
        report.fillShapes++;
        background.push(() => {
          g.fillStyle = above ? aboveFill : BELOW_FILL;
          g.beginPath();
          g.moveTo(points[0][0], points[0][1]);
          for (const [px, py] of points.slice(1)) g.lineTo(px, py);
          g.closePath();
          g.fill();
        });
      };
      if ((y1 - baselineY) * (y2 - baselineY) >= 0) {
        polygon(y1 < baselineY || y2 < baselineY, [[x1, y1], [x2, y2], [x2, baselineY], [x1, baselineY]]);
      } else {
        const xi = x1 + ((baselineY - y1) / (y2 - y1)) * (x2 - x1);
        polygon(y1 < baselineY, [[x1, y1], [xi, baselineY], [x1, baselineY]]);
        polygon(y2 < baselineY, [[x2, y2], [xi, baselineY], [x2, baselineY]]);
      }
    }
    front.push(() => line(g, x1, y1, x2, y2, color, stroke, dashed));
  };

  /** drawBlunderBar (WinrateGraph.java:272-295) along the panel's bottom edge. */
  const blunderBar = (from: number, to: number, winrateDrop: number, scoreDrop: number) => {
    const barHeight = blunderBarHeight(height, winrateDrop, scoreDrop);
    const start = posx + i((Math.min(from, to) * width) / numMoves);
    const stop = posx + i((Math.max(from, to) * width) / numMoves);
    const barWidth = Math.max(1, stop - start);
    const color = blunderColor(winrateDrop, scoreDrop);
    report.blunderBars++;
    blunder.push(() => {
      g.fillStyle = color;
      g.fillRect(start, H0 - barHeight, barWidth + 1, barHeight);
    });
  };

  // ---------------------------------------------------------------- win-rate line (WinrateGraph.java:976-1135)
  const curPoint = s.history.get(cur);
  const curParent = parentOf(nodes, cur);
  const currentMoveNumber = moveNumber(nodes, cur);
  const plotted = new Set<number>();
  let lastWr = 50;
  let lastScore = 0;
  let lastNodeOk = false;
  let movenum = moveNumber(nodes, end) - 1;
  let lastOkMove = -1;
  let cwr = -1;
  let cmovenum = -1;
  let mwr = -1;
  let mmovenum = -1;
  let canDrawBlunderBar = true;
  let forkNode: number | null = null;
  let dashed = false;
  /** Whether the next stroke continues the previous piece (it starts where that one ended). */
  let pieceOpen = false;
  let top = variationTop;
  let node = end;
  while (parentOf(nodes, node) !== null) {
    const p = s.history.get(node);
    // Java's own win rate of an unanalysed node only places the hover number; 50 puts it below.
    let wr = 50;
    const shown = displayedWinrate(s, node, lastWr);
    if (shown !== null) {
      wr = shown;
      plotted.add(node);
      const score = p ? p.black.score_lead ?? 0 : lastScore;
      const color = lastNodeOk ? LINE : MISS;
      if (lastOkMove > 0 && lastOkMove - movenum < 25) {
        if (canDrawBlunderBar && d.blunderBar && p) blunderBar(lastOkMove, movenum, Math.abs(lastWr - wr), Math.abs(lastScore - score));
        if (d.winrateLine) {
          filledLine(!lastNodeOk, X(lastOkMove), WY(lastWr), X(movenum), WY(wr), color, dashed ? 1 : 1.7, dashed);
          if (!pieceOpen) report.segments++;
          if (!lastNodeOk) report.missSegments++;
          pieceOpen = true;
        }
      } else pieceOpen = false;
      if (forkNode !== null && forkNode === node) {
        canDrawBlunderBar = true;
        dashed = false;
      }
      lastWr = wr;
      lastScore = score;
      lastNodeOk = true;
      // Reached the fork: the main trunk continues dashed from its end back to the fork.
      if (top !== null && top === node && nextOf(nodes, node) !== undefined) {
        canDrawBlunderBar = false;
        forkNode = top;
        dashed = true;
        pieceOpen = false;
        node = lineEnd(nodes, node);
        movenum = moveNumber(nodes, node) - 1;
        const continuation = displayedWinrate(s, node, lastWr);
        if (continuation !== null) {
          lastWr = continuation;
          wr = continuation;
          plotted.add(node);
        }
        lastScore = s.history.get(node)?.black.score_lead ?? 0;
        top = null;
        if (continuation === null) lastNodeOk = false;
      }
      if (d.winrateLine && (node === cur || (node === curParent && !curPoint))) {
        const x = X(movenum), y = clampDotY(WY(wr) - DOT_RADIUS);
        front.push(() => oval(g, x, y + DOT_RADIUS, DOT_RADIUS, LINE));
        cwr = wr;
        cmovenum = movenum;
      }
      lastOkMove = lastNodeOk ? movenum : -1;
    } else lastNodeOk = false;

    if (s.hover !== null && node === s.hover) {
      // The guide, the move number at the bottom (at the top under 3%), and the hover values.
      const x = X(movenum);
      const moveNum = moveNumber(nodes, node);
      const low = wr < 3;
      front.push(() => {
        line(g, x, posy, x, posy + height, GUIDE, 1, true);
        g.font = bold(12);
        const fm = metrics(g);
        const dx = moveNum < i(numMoves / 2) ? 3 : moveNum < 10 ? -10 : currentMoveNumber > 99 ? -22 : -16;
        g.fillStyle = "#fff";
        g.fillText(String(moveNum), x + dx, low ? posy + fm.ascent - fm.descent - 1 : posy + height - 1);
      });
      if (d.winrateLine && s.history.has(node)) {
        mwr = wr;
        mmovenum = movenum;
      }
    }
    node = parentOf(nodes, node)!;
    movenum--;
  }
  report.pointCount = plotted.size;

  // ---------------------------------------------------------------- score lead line (WinrateGraph.java:1151-1470)
  let maxScore = d.initialMaxScoreLead;
  let curScoreMoveNum = -1;
  let drawCurScore = 0;
  let mScoreMoveNum = -1;
  let drawHoverScore = 0;
  let markerIndex = -1;
  let markerMean = 0;
  const convert = (v: number) => (v > maxScore ? maxScore : v < 0 && Math.abs(v) > maxScore ? -maxScore : v);
  const SY = (v: number) => baselineY - jInt((convert(v) * height) / 2 / maxScore);
  /** scoreLeadAnchorY with the clamped lead. */
  const anchorY = (v: number) => posy + i(height / 2) - jInt((convert(v) * height) / 2 / Math.max(1, maxScore));
  if (d.scoreLeadLine && scoreAvailable) {
    // setMaxScoreLead: the initial scale, widened by the current line's largest lead (root included).
    for (let n: number | null = end; n !== null; n = parentOf(nodes, n)) {
      const lead = Math.abs(s.history.get(n)?.score_lead ?? 0);
      if (lead > maxScore) maxScore = lead;
    }
    report.maxScoreLead = maxScore;
    node = end;
    movenum = moveNumber(nodes, end) - 1;
    lastOkMove = -1;
    lastNodeOk = false;
    let lastMean = -500;
    while (parentOf(nodes, node) !== null) {
      const lead = s.history.get(node)?.black.score_lead ?? null;
      if (lead !== null) {
        report.scorePointCount++;
        if (node === cur || (node === curParent && !(curPoint && curPoint.visits > 0))) {
          curScoreMoveNum = movenum;
          drawCurScore = lead;
          markerIndex = movenum;
          markerMean = lead;
        }
        if (s.hover !== null && node === s.hover) {
          mScoreMoveNum = movenum;
          drawHoverScore = lead;
        }
        if (lastOkMove > 0 && Math.abs(movenum - lastOkMove) < 25 && lastMean > -500)
          filledLine(!lastNodeOk, X(lastOkMove), SY(lastMean), X(movenum), SY(lead), SCORE, 1, false);
        lastMean = lead;
        lastOkMove = movenum;
        lastNodeOk = true;
      } else lastNodeOk = false;
      node = parentOf(nodes, node)!;
      movenum--;
    }
  }
  paintBaseline(width);

  // ---------------------------------------------------------------- texts (WinrateGraph.java:1474-1647)
  const bounds: Box = { x: 0, y: 0, w: W0, h: H0 };
  /** drawPlacedString in the current font and colour. */
  const placed = (text: string, font: string, color: string, x: number, y: number) => {
    g.font = font;
    const fm = metrics(g);
    const box = placeLabel({ x, y: y - fm.ascent, w: stringWidth(g, text), h: fm.ascent + fm.descent }, bounds, occupied);
    occupied.push(box);
    front.push(() => {
      g.font = font;
      g.fillStyle = color;
      g.fillText(text, box.x, box.y + fm.ascent);
    });
  };
  const centredX = (text: string, moveIndex: number) => Math.min(Math.max(X(moveIndex) - i(stringWidth(g, text) / 2), 0), W0 - stringWidth(g, text));
  let mwrHeight = -1;
  let mWinFontHeight = -1;
  let oriMWrHeight = -1;
  if (mwr >= 0) {
    const x = X(mmovenum), y = clampDotY(WY(mwr) - DOT_RADIUS);
    front.push(() => oval(g, x, y + DOT_RADIUS, DOT_RADIUS, "rgb(255,0,0)"));
    g.font = bold(16);
    const fm = metrics(g);
    oriMWrHeight = posy + (height - jInt((mwr * height) / 100));
    mwrHeight = oriMWrHeight + (mwr < 10 ? -5 : (mwr > 90 ? 6 : -2) * DOT_RADIUS);
    mWinFontHeight = fm.ascent - fm.descent;
    if (mwrHeight > H0) mwrHeight = H0 - 2;
    const text = javaFixed1(mwr);
    report.hoverText = text;
    placed(text, bold(16), "#fff", centredX(text, mmovenum), mwrHeight);
  }
  if (mScoreMoveNum >= 0) {
    g.font = bold(14);
    const fm = metrics(g);
    let h = anchorY(drawHoverScore) - 3;
    const ori = h;
    const fontHeight = fm.ascent - fm.descent;
    const up = fontHeight;
    const down = H0;
    h = Math.min(down, Math.max(up, h));
    if (Math.abs(mwrHeight - h) < fontHeight) {
      if (ori < oriMWrHeight) h = mwrHeight - mWinFontHeight - 1 >= up ? mwrHeight - mWinFontHeight - 1 : mwrHeight + fontHeight + 1;
      else h = mwrHeight + fontHeight + 1 <= down ? mwrHeight + fontHeight + 1 : mwrHeight - mWinFontHeight - 1;
    }
    if (h > H0) h = Math.max(H0, mwrHeight - mWinFontHeight);
    const text = formatScoreLead(drawHoverScore);
    report.hoverScoreText = text;
    placed(text, bold(14), "rgb(255,255,0)", centredX(text, mScoreMoveNum), h);
  }
  let cwrHeight = -1;
  let winFontHeight = -1;
  let oriWrHeight = -1;
  if (cwr >= 0) {
    g.font = bold(16);
    const fm = metrics(g);
    oriWrHeight = posy + (height - jInt((cwr * height) / 100));
    cwrHeight = oriWrHeight + (cwr < 10 ? -5 : (cwr > 90 ? 6 : -2) * DOT_RADIUS);
    winFontHeight = fm.ascent - fm.descent;
    if (cwrHeight > H0) cwrHeight = H0 - 2;
    const text = javaFixed1(cwr);
    report.currentText = text;
    placed(text, bold(16), "#fff", centredX(text, cmovenum), cwrHeight);
  }
  if (curScoreMoveNum >= 0) {
    g.font = bold(14);
    const fm = metrics(g);
    let h = anchorY(drawCurScore) - 3;
    const ori = h;
    const fontHeight = fm.ascent - fm.descent;
    const up = fontHeight;
    const down = H0;
    h = Math.min(down, Math.max(up, h));
    const diff = Math.abs(cwrHeight - h);
    if (diff < fontHeight) {
      if (diff <= i(fontHeight / 3) && scoreAdjustMove === curScoreMoveNum) {
        h = scoreAdjustBelow ? cwrHeight + fontHeight + 1 : cwrHeight - winFontHeight - 1;
      } else {
        if (ori < oriWrHeight) {
          scoreAdjustBelow = !(cwrHeight - winFontHeight - 1 >= up);
        } else scoreAdjustBelow = cwrHeight + fontHeight + 1 <= down;
        h = scoreAdjustBelow ? cwrHeight + fontHeight + 1 : cwrHeight - winFontHeight - 1;
        scoreAdjustMove = diff <= i(fontHeight / 3) ? curScoreMoveNum : -1;
      }
    }
    const text = formatScoreLead(drawCurScore);
    report.currentScoreText = text;
    placed(text, bold(14), "rgb(255,255,0)", centredX(text, curScoreMoveNum), h);
  }

  // ---------------------------------------------------------------- move numbers and legend
  if (width >= 150) {
    const step = numMoves <= 63 ? 10 : numMoves <= 125 ? 20 : numMoves < 205 ? 30 : 40;
    for (let k = 1; k <= i(numMoves / step); k++) {
      if (numMoves - k * step <= 3) continue;
      const label = String(k * step);
      const x = posx + i(((k * step - 1) * width) / numMoves) - 3;
      report.xLabels.push(label);
      background.push(() => {
        g.font = plain(11);
        g.fillStyle = "rgb(200,200,200)";
        g.fillText(label, x, posy + height - 1);
      });
    }
  }
  // drawLineLegend (WinrateGraph.java:1774-1830); the outline keeps the background's dashed stroke.
  const legend: [string, string][] = [];
  if (winrateRenderable) legend.push([t("WinrateGraph.legendWinrate"), LINE]);
  if (scoreRenderable) legend.push([t("WinrateGraph.legendScoreLead"), SCORE]);
  if (W0 >= 240 && legend.length) {
    report.legend = legend.map(([label]) => label);
    background.push(() => {
      g.font = plain(11);
      const fm = metrics(g);
      const widths = legend.map(([label]) => stringWidth(g, label));
      const boxW = widths.reduce((sum, w) => sum + 16 + 5 + w + 12, 0) - 12 + 16;
      const boxH = fm.ascent + fm.descent + 6;
      const boxX = W0 - boxW - 6;
      const boxY = 4;
      g.fillStyle = "rgba(0,0,0,0.588)";
      g.beginPath();
      g.roundRect(boxX, boxY, boxW, boxH, 4);
      g.fill();
      g.strokeStyle = "rgba(255,255,255,0.098)";
      g.lineWidth = 1;
      g.setLineDash([4, 4]);
      g.beginPath();
      g.roundRect(boxX + 0.5, boxY + 0.5, boxW, boxH, 4);
      g.stroke();
      g.setLineDash([]);
      const textY = boxY + 3 + fm.ascent;
      const lineY = textY - i(fm.ascent / 2);
      let cx = boxX + 8;
      legend.forEach(([label, color], k) => {
        g.strokeStyle = color;
        g.lineWidth = 2;
        g.beginPath();
        g.moveTo(cx, lineY);
        g.lineTo(cx + 16, lineY);
        g.stroke();
        cx += 21;
        g.fillStyle = "rgb(210,210,210)";
        g.fillText(label, cx, textY);
        cx += widths[k] + 12;
      });
    });
  }

  // ---------------------------------------------------------------- anchors and current markers
  const pointX = (mn: number) => Math.max(posx, Math.min(posx + Math.max(0, width - 1), posx + i(((mn - 1) * width) / numMoves)));
  const pointY = (wr: number) => Math.max(posy, Math.min(posy + Math.max(0, height - 1), posy + height - jInt((wr * height) / 100)));
  const anchors: Anchor[] = [];
  if (d.winrateLine) {
    // buildDefaultGraphAnchorPoints (WinrateGraph.java:2314-2338).
    const append = (n: number, wr: number) => {
      if (parentOf(nodes, n) === null || !isAnchorNode(nodes, n)) return;
      const mn = moveNumber(nodes, n);
      anchors.push({ node: n, x: pointX(mn), y: pointY(wr), moveNumber: mn });
    };
    let last = 50;
    let fork = variationTop;
    for (let n = end; parentOf(nodes, n) !== null; n = parentOf(nodes, n)!) {
      const wr = displayedWinrate(s, n, last);
      if (wr !== null) {
        last = wr;
        append(n, wr);
      }
      if (fork !== null && fork === n && nextOf(nodes, n) !== undefined) {
        n = lineEnd(nodes, n);
        const cont = displayedWinrate(s, n, last);
        if (cont !== null) {
          last = cont;
          append(n, cont);
        }
        fork = null;
      }
    }
    report.points = anchors.map((a) => ({ node: a.node, move: a.moveNumber, x: a.x, y: a.y }));
    // includeCurrentMoveMarkerPoint: an unanalysed current move takes its last analysed ancestor's height.
    if (!anchors.some((a) => a.node === cur)) {
      for (let n = parentOf(nodes, cur); n !== null; n = parentOf(nodes, n)) {
        const fallback = anchors.find((a) => a.node === n);
        if (fallback) {
          anchors.push({ node: cur, x: pointX(currentMoveNumber), y: fallback.y, moveNumber: currentMoveNumber });
          break;
        }
      }
    }
  }
  report.anchors = anchors.length;
  if (anchors.length) {
    const half = anchorHalfWidth(anchors);
    front.push(() => {
      for (const a of anchors) {
        g.fillStyle = nodes.get(a.node)?.kind === "Snapshot" ? rgba([255, 208, 84], 110) : rgba(LINE_RGB, 80);
        g.fillRect(a.x - half, a.y - ANCHOR_HALF, half * 2 + 1, ANCHOR_HALF * 2 + 1);
      }
    });
  }
  // drawMainCurrentMarkers (WinrateGraph.java:3131-3171).
  const movePoint = anchors.find((a) => a.node === cur) ?? null;
  const scorePoint = d.scoreLeadLine && markerIndex >= 0
    ? { x: posx + i((markerIndex * width) / numMoves), y: Math.max(posy, Math.min(posy + height, anchorY(markerMean))) }
    : null;
  if (movePoint) report.currentMarker = { node: cur, x: movePoint.x, y: movePoint.y };
  report.scoreMarker = scorePoint;
  if (scorePoint && movePoint) {
    const dx = movePoint.x - scorePoint.x;
    const dy = movePoint.y - scorePoint.y;
    const overlap = MARKER_RADIUS * 2 + 2;
    report.markersOverlap = dx * dx + dy * dy <= overlap * overlap;
  }
  front.push(() => {
    if (!scorePoint) {
      if (movePoint) marker(g, movePoint.x, movePoint.y, MARKER_RADIUS, MOVE_MARKER);
      return;
    }
    if (!movePoint) return marker(g, scorePoint.x, scorePoint.y, MARKER_RADIUS, SCORE_MARKER);
    if (!report.markersOverlap) {
      marker(g, scorePoint.x, scorePoint.y, MARKER_RADIUS, SCORE_MARKER);
      marker(g, movePoint.x, movePoint.y, MARKER_RADIUS, MOVE_MARKER);
      return;
    }
    // Overlapping: a larger score marker with the move's core drawn on it.
    marker(g, scorePoint.x, scorePoint.y, MARKER_RADIUS + 3, SCORE_MARKER);
    markerCore(g, movePoint.x, movePoint.y, Math.max(3, MARKER_RADIUS - 1), MOVE_MARKER);
  });
  flush();

  // buildGraphNavigationTargets over buildGraphPath: the current line, then the main trunk after
  // the fork.
  const targets: GraphHits["targets"] = [];
  const path: number[] = [];
  for (let n: number | null = end; n !== null; n = parentOf(nodes, n)) path.push(n);
  path.reverse();
  if (variationTop !== null) for (let n = nextOf(nodes, variationTop); n !== undefined; n = nextOf(nodes, n)) path.push(n);
  for (const n of path) if (parentOf(nodes, n) !== null && isAnchorNode(nodes, n)) targets.push({ node: n, moveNumber: moveNumber(nodes, n) });
  return { report, hits: { origW: W0, origH: H0, posx, width, numMoves, anchors, targets } };
}

/** graphColumnHitHalfWidth (WinrateGraph.java:2792-2815): at most 2, less where columns crowd. */
function anchorHalfWidth(points: Anchor[]): number {
  const xs = [...new Set(points.map((p) => p.x))].sort((a, b) => a - b);
  let spacing = Infinity;
  for (let k = 1; k < xs.length; k++) if (xs[k] - xs[k - 1] > 0) spacing = Math.min(spacing, xs[k] - xs[k - 1]);
  if (spacing === Infinity) return 0;
  return Math.min(ANCHOR_HALF, Math.max(0, i((spacing - 1) / 2)));
}

// ------------------------------------------------------------------ hit testing

/** WinrateGraph.resolveMoveTargetNode (WinrateGraph.java:2194-2218) in graph-canvas pixels: a
 *  direct anchor hit, then a column with points of different nodes (branch over trunk), then the
 *  path's node at that move number or the nearest one. */
export function graphTarget(h: GraphHits, x: number, y: number): number | null {
  if (!(x >= 0 && x < h.origW && y >= 0 && y < h.origH)) return null;
  const distance = (p: Anchor) => (p.x - x) ** 2 + (p.y - y) ** 2;
  const half = anchorHalfWidth(h.anchors);
  let best: Anchor | null = null;
  for (const p of h.anchors)
    if (Math.abs(p.x - x) <= half && Math.abs(p.y - y) <= ANCHOR_HALF && (!best || distance(p) <= distance(best))) best = p;
  if (best) return best.node;
  if (h.width <= 0 || h.numMoves <= 0) return null;
  // targetGraphMoveNumber: Math.round of the scaled index, within the slots.
  const target = Math.max(0, Math.min(h.numMoves - 1, Math.floor(((x - h.posx) * h.numMoves) / h.width + 0.5))) + 1;
  const column = h.anchors.filter((p) => p.moveNumber === target);
  if (column.some((p) => p.node !== column[0].node)) {
    for (const p of column)
      if (column.some((o) => o.node === p.node && Math.abs(o.y - y) <= ANCHOR_HALF) && (!best || distance(p) <= distance(best))) best = p;
    if (best) return best.node;
  }
  if (!h.targets.length) return null;
  const exact = h.targets.find((p) => p.moveNumber === target);
  if (exact) return exact.node;
  let nearest: number | null = null;
  let nearestDistance = Infinity;
  for (const p of h.targets)
    if (Math.abs(p.moveNumber - target) < nearestDistance) {
      nearest = p.node;
      nearestDistance = Math.abs(p.moveNumber - target);
    }
  return nearest;
}

/** LizzieFrame.goToWinrateGraphTarget (LizzieFrame.java:10413-10500): the node browsing ends on for
 *  a graph target, or null when Java would not move. Backward or forward along the line; a click
 *  (not a drag, `withinBranch`) on the shown main trunk goes there through the fork. A drag going
 *  back stops at the first node of the branch it is in. */
export function graphDestination(nodes: Map<number, TreeNodeView>, current: number, target: number, withinBranch: boolean): number | null {
  if (current === target || !nodes.has(current) || !nodes.has(target)) return null;
  const reachable = (from: number, backwards: boolean) => {
    for (let n: number | null | undefined = from; n !== null && n !== undefined; n = backwards ? parentOf(nodes, n) : nextOf(nodes, n))
      if (n === target) return true;
    return false;
  };
  if (reachable(current, true)) {
    let n = current;
    while (n !== target) {
      if (withinBranch && !isFirstChild(nodes, n)) break;
      n = parentOf(nodes, n)!;
    }
    return n === current ? null : n;
  }
  if (reachable(current, false)) return target;
  if (!withinBranch && !isMainTrunk(nodes, current) && isMainTrunk(nodes, target)) {
    const fork = findTop(nodes, current);
    if (fork !== target && reachable(fork, false)) return target;
  }
  return null;
}
