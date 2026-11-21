// Win-rate graph (胜率图): WinrateGraph.draw (WinrateGraph.java:573-1680 with helpers 186-570,
// 2163-3250) and its hit testing for LizzieFrame's click / drag / hover (LizzieFrame.java:9600-10030,
// 10371-10500). The single-engine graph plots Black's values from the core (`HistoryPoint.black`);
// during an engine game (引擎对局) each engine's curve plots its own results instead (the side to
// move's, `HistoryPoint.winrate`). The plot sits above an axis strip carrying the move numbers. The
// painter records each layer's operations and replays them bottom up: grid and scales, area fill,
// the legend, blunder bars, guides, curves, points and markers, then the readout chips and the caption.
import type { DisplaySettings, HistoryPoint, Stone, TreeNodeView } from "../contract/generated";
import { product, registerProduct, t } from "../i18n";
import { graphGeom, type Rect } from "../workbench/layout";
import { fillTabular, panelFont, tabularWidth } from "../workbench/paint";
import { appearance, frameMetrics } from "../appearance/state";
import { belowBaselineFill, cssColor, graphBackground, sameRgba, THEME_DEFAULT, themeColor } from "../appearance/model";
import { stagePalette } from "../design/stage";

registerProduct({
  ZhCn: { graphNoAnalysis: "尚无分析数据，开始分析后显示曲线", graphAnalysing: "正在分析，结果会显示在这里", graphMove: "第{0}手", graphNamed: "{0}（{1}）" },
  ZhTw: { graphNoAnalysis: "尚無分析資料，開始分析後顯示曲線", graphAnalysing: "正在分析，結果會顯示在這裡", graphMove: "第{0}手", graphNamed: "{0}（{1}）" },
  EnUs: { graphNoAnalysis: "No analysis yet. Start analysis to draw the curve.", graphAnalysing: "Analysing; results appear here.", graphMove: "Move {0}", graphNamed: "{0} ({1})" },
  JaJp: { graphNoAnalysis: "まだ分析データがありません。分析を開始すると曲線を表示します", graphAnalysing: "分析中です。結果はここに表示されます", graphMove: "{0}手目", graphNamed: "{0}（{1}）" },
  KoKr: { graphNoAnalysis: "아직 분석 데이터가 없습니다. 분석을 시작하면 곡선이 표시됩니다", graphAnalysing: "분석 중입니다. 결과가 여기에 표시됩니다", graphMove: "{0}수", graphNamed: "{0} ({1})" },
  ThTh: { graphNoAnalysis: "ยังไม่มีข้อมูลการวิเคราะห์ เริ่มวิเคราะห์เพื่อแสดงเส้นกราฟ", graphAnalysing: "กำลังวิเคราะห์ ผลจะแสดงที่นี่", graphMove: "ตาที่ {0}", graphNamed: "{0} ({1})" },
});

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
  /** The view's engine is analysing: the empty graph says results are on their way. */
  analysing: boolean;
  /** WinrateGraph suppressGraphContent (play mode): no content and no empty-state caption. */
  suppressed: boolean;
  /** An engine game on the board (Board.isPkBoard, EngineGamePresentation.current()): the engine
   *  playing each side, whether a game is being played (EngineGameSnapshot.playing) and the side
   *  whose engine gives the score line (`PkBoardView.score_side`: whiteKataScoreMode, else
   *  blackKataScoreMode); null for the single-engine graph. */
  engineGame: { playing: boolean; black: string; white: string; scoreSide: Stone | null } | null;
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

/** The engine-game graph's own results, for the acceptance probe. */
export interface EngineGraphReport {
  /** The engine names of the black and the white engine's curve. */
  black: string;
  white: string;
  /** Separately stroked pieces of each engine's win-rate curve (0 without the line). */
  blackPieces: number;
  whitePieces: number;
  /** The side whose engine the score line follows (whiteKataScoreMode before
   *  blackKataScoreMode), null without a score line. */
  scoreSide: "Black" | "White" | null;
  /** Each engine's win rate (one decimal, no unit) in the current readout: the current move's and
   *  the move's before it, by the side that was to move. */
  blackText: string | null;
  whiteText: string | null;
  /** The same for the hovered move and the move before it. */
  hoverBlackText: string | null;
  hoverWhiteText: string | null;
}

/** What the graph drew, for the acceptance probe. */
export interface GraphReport {
  /** Nodes plotted with a win rate (WinrateGraph's analysed points; the root never). */
  pointCount: number;
  winrateRenderable: boolean;
  scoreRenderable: boolean;
  /** The midline's scale label while one curve shows: "50%" for the win rate, "0" for the score
   *  lead; null with both curves or none (each curve then labels its own scale). */
  chip: string | null;
  /** The dashed midline marks a curve's baseline (any curve shows). */
  baseline: boolean;
  /** 面积填充 applies: the setting is on, exactly one curve shows and no engine game is on. */
  fill: boolean;
  /** Area shapes drawn, one per analysed run of the one curve. */
  fillShapes: number;
  blunderBars: number;
  /** Separately stroked win-rate line pieces: connected runs of strokes, broken by a 25+ move gap or
   *  the jump to a fork's trunk end (in an engine game, both engines' pieces). Dashed bridges
   *  continue a piece. 0 without the line. */
  segments: number;
  /** Win-rate strokes drawn as dashed miss-colour bridges over unanalysed moves (under the 25 limit). */
  missSegments: number;
  numMoves: number;
  /** The score scale used, or null while no score line is drawn. */
  maxScoreLead: number | null;
  scorePointCount: number;
  /** Win-rate anchors, the current move's fallback included. */
  anchors: number;
  /** Win-rate anchors (analysed graph-path nodes, without the current-move fallback) in graph-canvas
   *  coordinates, the space of currentMarker and of the click hit test; the curve's vertices lie on
   *  them. `move` is the move number. */
  points: { node: number; move: number; x: number; y: number }[];
  currentMarker: { node: number; x: number; y: number } | null;
  scoreMarker: { x: number; y: number } | null;
  markersOverlap: boolean;
  /** Black's current win rate (one decimal, no unit) shown in the current readout chip; null in
   *  an engine game, whose readout is in `engineGame`. */
  currentText: string | null;
  /** The current score lead (formatScoreLead) shown in the current readout chip. */
  currentScoreText: string | null;
  hoverNode: number | null;
  /** The hovered move's win rate and score lead, as in the hover readout chip. */
  hoverText: string | null;
  hoverScoreText: string | null;
  /** The curve names the legend draws (empty when it is left out). */
  legend: string[];
  /** The move numbers on the axis strip. */
  xLabels: string[];
  /** The empty graph's caption, null while anything is plotted or the graph is suppressed. */
  emptyText: string | null;
  /** The engine-game graph, null for the single-engine graph. */
  engineGame: EngineGraphReport | null;
}

// ------------------------------------------------------------------ colours

/** The theme's graph values (Config.readThemeVaule, Config.java:2248-2264): winrate-line-color,
 *  winrate-miss-line-color, scoremean-line-color, blunder-bar-color, the two stroke widths and
 *  minimum-blunder-bar-width (WinrateGraph.java:174-184, 261-295, 654-656, 1208-1209); a colour
 *  still at the theme default takes the stage palette's shade of it, and stroke widths still at
 *  the default (1.7 and 1) are the card's 1.4 and 1.2. The graph draws on its card section
 *  without a ground of its own while comment-background-color is the default, else on
 *  resolveGraphBackgroundColor of it, and the area below the midline then takes
 *  resolveBelowBaselineFillColor of that ground (312-323, 396-399). */
function graphTheme() {
  const p = stagePalette();
  const th = appearance()?.theme;
  const comment = th?.commentBackgroundColor ?? THEME_DEFAULT.commentBackground;
  const own = sameRgba(comment, THEME_DEFAULT.commentBackground) ? null : graphBackground(comment);
  const winrateWidth = th?.winrateStrokeWidth ?? 1.7;
  const scoreWidth = th?.scoreLeadStrokeWidth ?? 1;
  return {
    palette: p,
    line: themeColor(th?.winrateLineColor, THEME_DEFAULT.winrateLine, p.winrateLine),
    miss: themeColor(th?.winrateMissLineColor, THEME_DEFAULT.winrateMiss, p.winrateMiss),
    /** The engine-game graph's white-engine curve: the theme's miss colour when it was changed. */
    engineWhite: themeColor(th?.winrateMissLineColor, THEME_DEFAULT.winrateMiss, p.engineWhiteLine),
    score: themeColor(th?.scoremeanLineColor, THEME_DEFAULT.scoreLine, p.scoreLine),
    bar: themeColor(th?.blunderBarColor, THEME_DEFAULT.blunderBar, p.blunderBar),
    winrateWidth: winrateWidth === 1.7 ? 1.4 : winrateWidth,
    scoreWidth: scoreWidth === 1 ? 1.2 : scoreWidth,
    barMinimum: Math.max(1, th?.minimumBlunderBarWidth ?? 1),
    /** The graph's own ground, null on the card's. */
    background: own ? cssColor(own) : null,
    /** What markers and hover dots are ringed with to separate them from the curves. */
    ground: own ? cssColor(own) : p.panelSolid,
    belowFill: own ? cssColor(belowBaselineFill(own)) : p.graphFillBelow,
  };
}

const DOT_RADIUS = 3;
const ANCHOR_HALF = 2;
const MARKER_RADIUS = 4;
/** Clearance between the plot's edges and the chips, scale labels and legend inside it. */
const INSET = 4;
/** Readout chips, legend and caption: inner padding, gap between parts, corner radius. */
const CHIP_PAD = 6;
const CHIP_GAP = 6;
const CHIP_RADIUS = 4;
/** Dash patterns: an analysed run is solid, a bridge over unanalysed moves and the main trunk's
 *  continuation after a fork are dashed (the bridge finer). */
const SOLID: number[] = [];
const BRIDGE_DASH = [3, 3];
const TRUNK_DASH = [4, 4];
/** A readout part's curve swatch: a short line and the gap after it, as in the legend. */
const SWATCH = 16;

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

/** A scale value: one decimal at most, without a trailing ".0". */
function compactNumber(v: number): string {
  const s = javaFixed1(Math.round(v * 10) / 10);
  return s.endsWith(".0") ? s.slice(0, -2) : s;
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

/** BoardData.blackToPlay: the analysed side to move, else the side after the node's move; a
 *  snapshot keeps its parent's side, the root has Black to move. */
function blackToPlay(s: GraphScene, n: number): boolean {
  const point = s.history.get(n);
  if (point) return point.to_play === "Black";
  const node = s.nodes.get(n);
  if (node?.kind === "Black") return false;
  if (node?.kind === "White") return true;
  return node?.parent == null ? true : blackToPlay(s, node.parent);
}

// ------------------------------------------------------------------ drawing primitives

type Op = () => void;

function metrics(g: CanvasRenderingContext2D) {
  const m = g.measureText("0");
  return { ascent: i(0.95 + m.fontBoundingBoxAscent), descent: i(0.95 + m.fontBoundingBoxDescent) };
}
const stringWidth = (g: CanvasRenderingContext2D, s: string) => Math.round(g.measureText(s).width);
const plain = (size: number) => `${size}px ${panelFont()}`;
const bold = (size: number) => `600 ${size}px ${panelFont()}`;

/** `text` cut to `max` pixels in the current font, ending in "…" where it is cut. */
function elide(g: CanvasRenderingContext2D, text: string, max: number): string {
  if (stringWidth(g, text) <= max) return text;
  let k = text.length;
  while (k > 0 && stringWidth(g, `${text.slice(0, k)}…`) > max) k--;
  return `${text.slice(0, k)}…`;
}

/** drawLine with a 1 px aligned stroke; `dashed` is Java's 4 / 4 dash or a pattern of its own. */
function line(g: CanvasRenderingContext2D, x1: number, y1: number, x2: number, y2: number, color: string, width: number, dashed: boolean | number[]) {
  g.strokeStyle = color;
  g.lineWidth = width;
  g.setLineDash(dashed === true ? [4, 4] : dashed || []);
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

/** A current marker: a filled dot ringed in the ground under the graph. */
function marker(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, fill: string, ring: string) {
  oval(g, cx, cy, r + 2, ring);
  oval(g, cx, cy, r, fill);
}

/** A hovered value: a hollow dot in the curve's colour on the ground under the graph. */
function hoverDot(g: CanvasRenderingContext2D, cx: number, cy: number, color: string, ground: string) {
  oval(g, cx, cy, DOT_RADIUS + 1, ground);
  g.strokeStyle = color;
  g.lineWidth = 1.5;
  g.beginPath();
  g.arc(cx, cy, DOT_RADIUS, 0, Math.PI * 2);
  g.stroke();
}

interface Box { x: number; y: number; w: number; h: number }

const intersects = (a: Box, b: Box) => a.w > 0 && a.h > 0 && b.w > 0 && b.h > 0 && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;

/** How much of a polyline runs inside `box`: each segment clipped to it (Liang–Barsky). */
function lengthInside(points: number[][], box: Box): number {
  let total = 0;
  for (let k = 1; k < points.length; k++) {
    const [x1, y1] = points[k - 1];
    const [x2, y2] = points[k];
    const dx = x2 - x1;
    const dy = y2 - y1;
    let t0 = 0;
    let t1 = 1;
    for (const [p, q] of [[-dx, x1 - box.x], [dx, box.x + box.w - x1], [-dy, y1 - box.y], [dy, box.y + box.h - y1]]) {
      if (p === 0) {
        if (q < 0) t1 = -1;
      } else if (p < 0) t0 = Math.max(t0, q / p);
      else t1 = Math.min(t1, q / p);
    }
    if (t1 > t0) total += (t1 - t0) * Math.hypot(dx, dy);
  }
  return total;
}

/** How a curve piece is stroked; `filled` pieces enclose the area fill with the midline. */
interface StrokeStyle { color: string; width: number; dash: number[]; filled: boolean }

/** A polyline of one style: consecutive strokes of that style that join extend it. */
interface Stroke extends StrokeStyle { points: number[][] }

function addStroke(strokes: Stroke[], x1: number, y1: number, x2: number, y2: number, style: StrokeStyle) {
  const last = strokes.at(-1);
  const end = last?.points.at(-1);
  if (last && end && end[0] === x1 && end[1] === y1 && last.color === style.color && last.width === style.width && last.dash === style.dash && last.filled === style.filled)
    last.points.push([x2, y2]);
  else strokes.push({ ...style, points: [[x1, y1], [x2, y2]] });
}

/** Strokes each polyline as one path with round joins (round caps on solid runs). */
function strokePaths(g: CanvasRenderingContext2D, strokes: Stroke[]) {
  g.lineJoin = "round";
  for (const s of strokes) {
    g.strokeStyle = s.color;
    g.lineWidth = s.width;
    g.setLineDash(s.dash);
    g.lineCap = s.dash.length ? "butt" : "round";
    g.beginPath();
    g.moveTo(s.points[0][0], s.points[0][1]);
    for (let k = 1; k < s.points.length; k++) g.lineTo(s.points[k][0], s.points[k][1]);
    g.stroke();
  }
  g.setLineDash([]);
  g.lineCap = "butt";
  g.lineJoin = "miter";
}

/** A piece of a readout chip's text; `swatch` leads it with that curve's colour. */
interface ChipPart { text: string; font: string; color: string; swatch?: string }

// ------------------------------------------------------------------ painter

/** WinrateGraph.draw: the single-engine graph, or the engine-game one (engineOrPkBoard) while
 *  `s.engineGame` is set. Returns what was drawn and the hit-test sources (null when nothing is
 *  plotted). */
export function paintGraph(g: CanvasRenderingContext2D, r: Rect, s: GraphScene): { report: GraphReport; hits: GraphHits | null } {
  const W0 = r.w;
  const H0 = r.h;
  const d = s.display;
  const nodes = s.nodes;
  const theme = graphTheme();
  const p = theme.palette;
  const LINE = theme.line;
  const SCORE = theme.score;
  // largeEnough (WinrateGraph.java:581): a large graph sets its text one type step up.
  const type = frameMetrics().type;
  const large = W0 > 475 && H0 > 335;
  const labelFont = plain(large ? type.size : type.s);
  const readSize = large ? type.l : type.size;
  const background: Op[] = [];
  const fills: Op[] = [];
  const legendLayer: Op[] = [];
  const blunder: Op[] = [];
  const guides: Op[] = [];
  const curves: Op[] = [];
  const front: Op[] = [];
  /** Readout chips and caption, over everything. */
  const overlay: Op[] = [];
  const flush = () => {
    for (const op of [...background, ...fills, ...legendLayer, ...blunder]) op();
    // The current and hovered move's guides break around the scale labels they cross (the line
    // end's guide stands over the right-hand ones whenever the last move is current).
    g.save();
    g.beginPath();
    g.rect(0, 0, W0, plotBottom + 1);
    for (const box of scaleBoxes) g.rect(box.x - 1, box.y - 1, box.w + 2, box.h + 2);
    g.clip("evenodd");
    for (const op of guides) op();
    g.restore();
    for (const op of [...curves, ...front, ...overlay]) op();
    g.setLineDash([]);
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
  };
  g.textAlign = "left";
  g.textBaseline = "alphabetic";

  const eg = s.engineGame;
  // currentRenderableMetrics: the score line needs a score source (Java: KataGo / Sai engine or a
  // KataGo record), here any stored lead on the graph's nodes. In an engine game
  // (resolveScoreLeadAvailable) it is the engine of the score side alone; without one there is no
  // score line.
  const engineScoreSide = eg ? eg.scoreSide : null;
  let scoreAvailable = engineScoreSide !== null;
  if (!eg) for (const p of s.history.values()) if (p.black.score_lead !== null) { scoreAvailable = true; break; }
  const winrateRenderable = d.winrateLine;
  const scoreRenderable = d.scoreLeadLine && scoreAvailable;
  const renderable = Number(winrateRenderable) + Number(scoreRenderable);
  const chip = renderable !== 1 ? null : winrateRenderable ? "50%" : "0";
  const baselineActive = renderable > 0;
  // No area fill while an engine game is on the board (WinrateGraph.java:629-633).
  const fillEnabled = d.winrateGraphFill && renderable === 1 && !eg;
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
    emptyText: null,
    engineGame: eg
      ? { black: eg.black, white: eg.white, blackPieces: 0, whitePieces: 0, scoreSide: scoreRenderable ? engineScoreSide : null, blackText: null, whiteText: null, hoverBlackText: null, hoverWhiteText: null }
      : null,
  };

  if (theme.background) {
    const ground = theme.background;
    background.push(() => {
      g.fillStyle = ground;
      g.fillRect(0, 0, W0, H0);
    });
  }
  // The plot above the axis strip, which is a line of Label text with 2 px either side.
  g.font = labelFont;
  const lm = metrics(g);
  const labelH = lm.ascent + lm.descent;
  const posx = 0;
  const posy = 2;
  const plotBottom = Math.max(posy + 4, H0 - labelH - 4);
  const height = plotBottom - posy;
  const baselineY = posy + i(height / 2);
  /** The grid line at k quarters of the win rate. */
  const quarterY = (k: number) => posy + height - jInt((height * k * 25) / 100);
  // Hairlines at 25 / 75 %, the dashed midline at 50 % (the baseline of the curves).
  background.push(() => {
    for (const k of [1, 3]) line(g, posx, quarterY(k), W0, quarterY(k), p.graphGrid, 1, false);
    line(g, posx, baselineY, W0, baselineY, p.graphBaseline, 1, [2, 3]);
  });
  /** Where the scale labels stand, which the legend leaves free. */
  const scaleBoxes: Box[] = [];
  /** The readout chips drawn; a scale label one of them covers is left out (a short graph's 75 %
   *  labels share the band at the top of the plot with the chips). */
  const readoutBoxes: Box[] = [];
  /** The current move's markers with their rings; a scale label one of them covers is left out
   *  too (the current move at the line end stands over the right-hand labels). */
  const markerBoxes: Box[] = [];
  /** The scale labels just above the lines at 75 / 50 / 25 % (the midline's alone where the
   *  quarters are closer than a line of Label text): the win rate at the left edge; the score
   *  lead at ±maxScore / 2 and 0 at the right edge, at the left without the win-rate curve. */
  const scaleLabels = (maxScore: number | null) => {
    const quarters = baselineY - quarterY(3) >= labelH + 2;
    const rows: { y: number; text: string; color: string; right: boolean }[] = [];
    const add = (texts: [string, string, string], color: string, right: boolean) =>
      [quarterY(3), baselineY, quarterY(1)].forEach((y, k) => {
        if (k === 1 || quarters) rows.push({ y, text: texts[k], color, right });
      });
    if (winrateRenderable) add(["75%", "50%", "25%"], p.graphLabel, false);
    if (maxScore !== null) {
      const half = compactNumber(maxScore / 2);
      add([t("WinrateGraph.scoreLeadBlackPrefix") + half, "0", t("WinrateGraph.scoreLeadWhitePrefix") + half], p.graphScoreText, winrateRenderable);
    }
    // The labels' ink (digits, signs and "%" reach neither the line's ascent nor its descent), so
    // a legend in a top corner may stand in the space above a short graph's 75 % labels.
    g.font = labelFont;
    for (const row of rows) {
      const w = tabularWidth(g, row.text);
      const ink = g.measureText(row.text);
      const baseline = row.y - 2 - lm.descent;
      scaleBoxes.push({ x: row.right ? W0 - INSET - w : posx + INSET, y: baseline - ink.actualBoundingBoxAscent - 1, w, h: ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent + 2 });
    }
    background.push(() => {
      g.font = labelFont;
      rows.forEach((row, k) => {
        if ([...readoutBoxes, ...markerBoxes].some((box) => intersects(box, scaleBoxes[k]))) return;
        g.fillStyle = row.color;
        fillTabular(g, row.text, row.right ? W0 - INSET - tabularWidth(g, row.text) : posx + INSET, row.y - 2 - lm.descent);
      });
    });
  };
  /** The empty graph's caption on the chip ground, centred on the midline: nothing is plotted and
   *  the graph is not suppressed. Left out where one line of it does not fit. */
  const caption = () => {
    if (s.suppressed || report.pointCount || report.scorePointCount) return;
    const text = product(s.analysing ? "graphAnalysing" : "graphNoAnalysis");
    g.font = labelFont;
    const w = stringWidth(g, text) + 2 * CHIP_PAD;
    const h = labelH + 6;
    if (w > W0 - 2 * INSET || h > height) return;
    report.emptyText = text;
    const x = i((W0 - w) / 2);
    const y = baselineY - i(h / 2);
    overlay.push(() => {
      g.fillStyle = p.graphChip;
      g.beginPath();
      g.roundRect(x, y, w, h, CHIP_RADIUS);
      g.fill();
      g.font = labelFont;
      g.fillStyle = p.graphLabel;
      g.fillText(text, x + CHIP_PAD, y + 3 + lm.ascent);
    });
  };
  /** Nothing to plot along: the grid and scales, and the caption. */
  const bare = () => {
    scaleLabels(scoreRenderable ? d.initialMaxScoreLead : null);
    caption();
    flush();
    return { report, hits: null };
  };

  const cur = s.current;
  if (!nodes.has(cur)) return bare();
  const variationTop = isMainTrunk(nodes, cur) ? null : findTop(nodes, cur);
  let numMoves = variationTop === null ? 0 : depth(nodes, variationTop) + moveNumber(nodes, variationTop) - 1;
  const end = lineEnd(nodes, cur);
  if (numMoves < moveNumber(nodes, end) - 1) numMoves = moveNumber(nodes, end) - 1;
  if (numMoves < 1) return bare();
  const geo = graphGeom(W0, H0, numMoves);
  numMoves = geo.slots;
  const width = geo.width;
  report.numMoves = numMoves;
  /** A move index's column and a win rate's height, kept inside the plot; anchors, curve
   *  vertices, fill and markers all take these. */
  const X = (moveIndex: number) => posx + Math.max(0, Math.min(Math.max(0, width - 1), i((moveIndex * width) / numMoves)));
  const WY = (wr: number) => Math.max(posy, Math.min(plotBottom - 1, posy + height - jInt((wr * height) / 100)));
  const winRun: StrokeStyle = { color: LINE, width: theme.winrateWidth, dash: SOLID, filled: fillEnabled };
  const winTrunk: StrokeStyle = { color: LINE, width: 1, dash: TRUNK_DASH, filled: false };
  const winBridge: StrokeStyle = { color: theme.miss, width: 1, dash: BRIDGE_DASH, filled: false };
  const scoreRun: StrokeStyle = { color: SCORE, width: theme.scoreWidth, dash: SOLID, filled: fillEnabled };
  const scoreBridge: StrokeStyle = { color: SCORE, width: 1, dash: BRIDGE_DASH, filled: false };
  const winStrokes: Stroke[] = [];
  const scoreStrokes: Stroke[] = [];
  /** drawFilledGraphLine's area (WinrateGraph.java:452-548, 3082-3105): each analysed run of the
   *  one curve encloses one shape with the midline, the ink tint above it (Black ahead) and the
   *  paper tint below it; bridges and the dashed trunk are not filled. */
  const paintFill = (strokes: Stroke[]) => {
    for (const run of strokes) {
      if (!run.filled || run.points.every(([, y]) => y === baselineY)) continue;
      report.fillShapes++;
      fills.push(() => {
        for (const [above, color] of [[true, p.graphFillAbove], [false, theme.belowFill]] as const) {
          g.save();
          g.beginPath();
          if (above) g.rect(0, 0, W0, baselineY);
          else g.rect(0, baselineY, W0, H0 - baselineY);
          g.clip();
          g.fillStyle = color;
          g.beginPath();
          g.moveTo(run.points[0][0], baselineY);
          for (const [x, y] of run.points) g.lineTo(x, y);
          g.lineTo(run.points[run.points.length - 1][0], baselineY);
          g.closePath();
          g.fill();
          g.restore();
        }
      });
    }
  };

  /** Where the blunder bars stand, which the legend avoids as it avoids the curves. */
  const barBoxes: Box[] = [];
  /** drawBlunderBar (WinrateGraph.java:272-295), growing up from the plot's bottom edge. */
  const blunderBar = (from: number, to: number, winrateDrop: number, scoreDrop: number) => {
    const barHeight = blunderBarHeight(height, winrateDrop, scoreDrop);
    const start = posx + i((Math.min(from, to) * width) / numMoves);
    const stop = posx + i((Math.max(from, to) * width) / numMoves);
    const barWidth = Math.max(theme.barMinimum, stop - start);
    const color = theme.bar;
    report.blunderBars++;
    barBoxes.push({ x: start, y: plotBottom - barHeight, w: barWidth + 1, h: barHeight });
    blunder.push(() => {
      g.fillStyle = color;
      g.fillRect(start, plotBottom - barHeight, barWidth + 1, barHeight);
    });
  };

  // ---------------------------------------------------------------- win-rate line (WinrateGraph.java:976-1135), single engine
  const curPoint = s.history.get(cur);
  const curParent = parentOf(nodes, cur);
  const currentMoveNumber = moveNumber(nodes, cur);
  const plotted = new Set<number>();
  /** Plotted nodes a stroke reaches; the others are drawn as single points. */
  const joined = new Set<number>();
  let lastWr = 50;
  let lastScore = 0;
  let lastNodeOk = false;
  let movenum = moveNumber(nodes, end) - 1;
  let lastOkMove = -1;
  let lastOkNode = -1;
  let cwr = -1;
  let cmovenum = -1;
  let mwr = -1;
  let mmovenum = -1;
  let hoverX: number | null = null;
  let hoverMove = 0;
  let canDrawBlunderBar = true;
  let forkNode: number | null = null;
  let dashed = false;
  /** Whether the next stroke continues the previous piece (it starts where that one ended). */
  let pieceOpen = false;
  let top = variationTop;
  let node = end;
  while (!eg && parentOf(nodes, node) !== null) {
    const point = s.history.get(node);
    // Java's own win rate of an unanalysed node only places the hover value; 50 puts it below.
    let wr = 50;
    const shown = displayedWinrate(s, node, lastWr);
    if (shown !== null) {
      wr = shown;
      plotted.add(node);
      const score = point ? point.black.score_lead ?? 0 : lastScore;
      if (lastOkMove > 0 && lastOkMove - movenum < 25) {
        if (canDrawBlunderBar && d.blunderBar && point) blunderBar(lastOkMove, movenum, Math.abs(lastWr - wr), Math.abs(lastScore - score));
        if (d.winrateLine) {
          addStroke(winStrokes, X(lastOkMove), WY(lastWr), X(movenum), WY(wr), !lastNodeOk ? winBridge : dashed ? winTrunk : winRun);
          joined.add(lastOkNode);
          joined.add(node);
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
        cwr = wr;
        cmovenum = movenum;
      }
      lastOkMove = lastNodeOk ? movenum : -1;
      lastOkNode = node;
    } else lastNodeOk = false;

    if (s.hover !== null && node === s.hover) {
      hoverX = X(movenum);
      hoverMove = moveNumber(nodes, node);
      if (d.winrateLine && s.history.has(node)) {
        mwr = wr;
        mmovenum = movenum;
      }
    }
    node = parentOf(nodes, node)!;
    movenum--;
  }
  report.pointCount = plotted.size;

  // ---------------------------------------------------------------- engine curves (WinrateGraph.java:706-975)
  // Each engine's own win rate (its side to move's, not turned to Black's), every node joined to
  // the node two moves back: the black engine's positions (Black to move) in the win-rate colour,
  // the white engine's in the miss colour, without area fill. An unanalysed node takes the result
  // two moves back, else 50; a blunder bar spans every such pair.
  /** The engine searched the position (getPlayouts() > 0). */
  const searched = (n: number) => (s.history.get(n)?.visits ?? 0) > 0;
  /** The current readout's values: the current move's and its parent's (Java's saveCurWr and the
   *  previous move's dot), and the hover readout's: the hovered move's and its parent's. */
  const engineCurrent: { node: number; x: number; y: number; wr: number; black: boolean }[] = [];
  const engineHover: { x: number; y: number; wr: number; black: boolean }[] = [];
  if (eg) {
    const blackRun: StrokeStyle = { color: LINE, width: theme.winrateWidth, dash: SOLID, filled: false };
    const whiteRun: StrokeStyle = { color: theme.engineWhite, width: theme.winrateWidth, dash: SOLID, filled: false };
    const blackPieces: Stroke[] = [];
    const whitePieces: Stroke[] = [];
    const counted = new Set<number>();
    for (let n = end; parentOf(nodes, n) !== null && parentOf(nodes, parentOf(nodes, n)!) !== null; n = parentOf(nodes, n)!) {
      const back = parentOf(nodes, parentOf(nodes, n)!)!;
      const index = moveNumber(nodes, n) - 1;
      const backIndex = Math.max(0, moveNumber(nodes, back) - 1);
      const own = s.history.get(n);
      const prior = s.history.get(back);
      let wr = 50;
      let score = 0;
      if (own && searched(n)) {
        wr = own.winrate;
        score = own.score_lead ?? 0;
        counted.add(n);
      } else if (prior && searched(back)) {
        wr = prior.winrate;
        score = prior.score_lead ?? 0;
      }
      const backWr = prior && searched(back) ? prior.winrate : wr;
      const backScore = prior && searched(back) ? prior.score_lead ?? 0 : score;
      if (prior && searched(back) && parentOf(nodes, back) !== null) counted.add(back);
      if (d.blunderBar) blunderBar(backIndex, index, Math.abs(backWr - wr), Math.abs(backScore - score));
      if (!d.winrateLine) continue;
      const black = blackToPlay(s, n);
      const pieces = black ? blackPieces : whitePieces;
      const before = pieces.length;
      addStroke(pieces, X(index), WY(wr), X(backIndex), WY(backWr), black ? blackRun : whiteRun);
      if (pieces.length > before) report.segments++;
      if (curParent !== null && index > 1 && (n === cur || n === curParent)) engineCurrent.push({ node: n, x: X(index), y: WY(wr), wr, black });
    }
    // The black engine's curve over the white one's.
    winStrokes.push(...whitePieces, ...blackPieces);
    report.engineGame!.blackPieces = blackPieces.length;
    report.engineGame!.whitePieces = whitePieces.length;
    report.pointCount = counted.size;
    for (let n = end; s.hover !== null && parentOf(nodes, n) !== null; n = parentOf(nodes, n)!) {
      if (n !== s.hover) continue;
      hoverX = X(moveNumber(nodes, n) - 1);
      hoverMove = moveNumber(nodes, n);
      for (const m of [n, parentOf(nodes, n)!]) {
        const point = s.history.get(m);
        if (d.winrateLine && point && searched(m)) engineHover.push({ x: X(moveNumber(nodes, m) - 1), y: WY(point.winrate), wr: point.winrate, black: blackToPlay(s, m) });
      }
      break;
    }
  }

  // ---------------------------------------------------------------- score lead line (WinrateGraph.java:1151-1470)
  let maxScore = d.initialMaxScoreLead;
  let curScoreMoveNum = -1;
  let drawCurScore = 0;
  let mScoreMoveNum = -1;
  let drawHoverScore = 0;
  let markerIndex = -1;
  let markerMean = 0;
  const convert = (v: number) => (v > maxScore ? maxScore : v < 0 && Math.abs(v) > maxScore ? -maxScore : v);
  /** scoreLeadAnchorY with the clamped lead, kept inside the plot. */
  const SY = (v: number) => Math.max(posy, Math.min(plotBottom - 1, baselineY - jInt((convert(v) * height) / 2 / Math.max(1, maxScore))));
  /** Score points no stroke reaches, drawn as single points. */
  const scoreDots: number[][] = [];
  if (d.scoreLeadLine && scoreAvailable) {
    // setMaxScoreLead: the initial scale, widened by the current line's largest lead (root included).
    for (let n: number | null = end; n !== null; n = parentOf(nodes, n)) {
      const lead = Math.abs(s.history.get(n)?.score_lead ?? 0);
      if (lead > maxScore) maxScore = lead;
    }
    report.maxScoreLead = maxScore;
  }
  if (d.scoreLeadLine && scoreAvailable && !eg) {
    node = end;
    movenum = moveNumber(nodes, end) - 1;
    lastOkMove = -1;
    lastNodeOk = false;
    let lastMean = -500;
    let lastJoined = false;
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
        const join = lastOkMove > 0 && Math.abs(movenum - lastOkMove) < 25 && lastMean > -500;
        if (join) addStroke(scoreStrokes, X(lastOkMove), SY(lastMean), X(movenum), SY(lead), lastNodeOk ? scoreRun : scoreBridge);
        // The previous point stood alone when neither of its neighbours joined it.
        else if (lastMean > -500 && !lastJoined) scoreDots.push([X(lastOkMove), SY(lastMean)]);
        lastJoined = join;
        lastMean = lead;
        lastOkMove = movenum;
        lastNodeOk = true;
      } else lastNodeOk = false;
      node = parentOf(nodes, node)!;
      movenum--;
    }
    if (lastMean > -500 && !lastJoined) scoreDots.push([X(lastOkMove), SY(lastMean)]);
  }
  if (eg && d.scoreLeadLine && engineScoreSide !== null) {
    // WinrateGraph.java:1170-1415: Black's lead from the scoring engine's positions only, every
    // second node back from the line end, joined under the 25-move limit; the earliest one runs
    // flat back to the start. While a game is played, an unanalysed position at the end reads the
    // lead two moves back.
    const white = engineScoreSide === "White";
    /** blackPerspectiveScoreMean: 0 without a result. */
    const blackLead = (n: number) => {
      const point = s.history.get(n);
      const lead = point?.score_lead ?? 0;
      return point?.to_play === "White" ? -lead : lead;
    };
    let n = end;
    let mv = moveNumber(nodes, end) - 1;
    let lastMean: number | null = null;
    let lastOk = -1;
    let lastJoined = false;
    let at = -1;
    let atMean = 0;
    // The line end and the current move step back to the scoring engine's positions.
    if (blackToPlay(s, n) === white) {
      mv--;
      const prev = parentOf(nodes, n);
      if (prev !== null) {
        if (eg.playing) {
          at = mv;
          atMean = blackLead(prev);
          lastMean = atMean;
          lastOk = mv;
        }
        n = prev;
      }
    }
    const scoreNode = blackToPlay(s, cur) === white && curParent !== null ? curParent : cur;
    while (parentOf(nodes, n) !== null && parentOf(nodes, parentOf(nodes, n)!) !== null) {
      const back = parentOf(nodes, parentOf(nodes, n)!)!;
      if (searched(n)) {
        const mean = blackLead(n);
        report.scorePointCount++;
        if (n === scoreNode) {
          at = mv;
          atMean = mean;
        }
        const join = lastMean !== null && lastOk > 0 && Math.abs(mv - lastOk) < 25;
        if (join) addStroke(scoreStrokes, X(lastOk), SY(lastMean!), X(mv), SY(mean), scoreRun);
        else if (lastMean !== null && !lastJoined) scoreDots.push([X(lastOk), SY(lastMean)]);
        lastJoined = join;
        lastMean = mean;
        lastOk = mv;
      } else if (eg.playing) {
        const next = nextOf(nodes, n);
        if (next === undefined || nextOf(nodes, next) === undefined) {
          at = mv;
          atMean = blackLead(back);
        }
      }
      n = back;
      mv -= 2;
    }
    if (lastMean !== null) {
      if (mv !== lastOk) addStroke(scoreStrokes, X(lastOk), SY(lastMean), X(mv), SY(lastMean), scoreRun);
      else if (!lastJoined) scoreDots.push([X(lastOk), SY(lastMean)]);
    }
    if (at >= 0) {
      curScoreMoveNum = markerIndex = at;
      drawCurScore = markerMean = atMean;
    }
    // The hovered move's lead: its own or its parent's, whichever is the scoring engine's.
    for (const m of hoverX === null ? [] : [s.hover!, parentOf(nodes, s.hover!)]) {
      if (m === null || blackToPlay(s, m) === white || !searched(m)) continue;
      mScoreMoveNum = moveNumber(nodes, m) - 1;
      drawHoverScore = blackLead(m);
      break;
    }
  }
  paintFill(winStrokes);
  paintFill(scoreStrokes);
  // The score line under the win-rate line.
  curves.push(() => {
    strokePaths(g, scoreStrokes);
    strokePaths(g, winStrokes);
  });
  scaleLabels(report.maxScoreLead);

  // ---------------------------------------------------------------- move numbers on the axis strip
  if (width >= 150) {
    const step = numMoves <= 63 ? 10 : numMoves <= 125 ? 20 : numMoves < 205 ? 30 : 40;
    for (let k = 1; k <= i(numMoves / step); k++) {
      if (numMoves - k * step <= 3) continue;
      const label = String(k * step);
      const x = X(k * step - 1);
      report.xLabels.push(label);
      background.push(() => {
        line(g, x, posy, x, plotBottom - 1, p.graphGrid, 1, false);
        g.font = labelFont;
        const w = tabularWidth(g, label);
        g.fillStyle = p.graphLabel;
        fillTabular(g, label, Math.max(0, Math.min(W0 - w, Math.round(x + 0.5 - w / 2))), plotBottom + 2 + lm.ascent);
      });
    }
  }

  // ---------------------------------------------------------------- anchors, points and markers
  const anchors: Anchor[] = [];
  if (d.winrateLine) {
    // appendGraphPoint: an action or snapshot node off the root at its column and win rate.
    const append = (n: number, wr: number) => {
      if (parentOf(nodes, n) === null || !isAnchorNode(nodes, n)) return;
      const mn = moveNumber(nodes, n);
      anchors.push({ node: n, x: X(mn - 1), y: WY(wr), moveNumber: mn });
    };
    // buildEngineGraphAnchorPoints (WinrateGraph.java:2355-2385): each node at its own result (else
    // the node's two moves back, else 50) and the node two moves back at its result (else the same).
    for (let n = end; eg && parentOf(nodes, n) !== null && parentOf(nodes, parentOf(nodes, n)!) !== null; n = parentOf(nodes, n)!) {
      const back = parentOf(nodes, parentOf(nodes, n)!)!;
      const wr = s.history.get(n)?.winrate ?? s.history.get(back)?.winrate ?? 50;
      append(n, wr);
      append(back, s.history.get(back)?.winrate ?? wr);
    }
    // buildDefaultGraphAnchorPoints (WinrateGraph.java:2314-2338).
    let last = 50;
    let fork = variationTop;
    for (let n = end; !eg && parentOf(nodes, n) !== null; n = parentOf(nodes, n)!) {
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
          anchors.push({ node: cur, x: X(currentMoveNumber - 1), y: fallback.y, moveNumber: currentMoveNumber });
          break;
        }
      }
    }
  }
  report.anchors = anchors.length;
  // drawGraphAnchors: the curve carries ordinary moves without marks (they still take clicks); a
  // point no stroke reaches is a dot of the curve, and snapshots keep their marks.
  const half = anchorHalfWidth(anchors);
  front.push(() => {
    for (const [x, y] of scoreDots) oval(g, x, y, Math.max(2, theme.scoreWidth + 0.6), SCORE);
    for (const a of anchors) {
      if (plotted.has(a.node) && !joined.has(a.node)) oval(g, a.x, a.y, Math.max(2, theme.winrateWidth + 0.6), LINE);
      if (nodes.get(a.node)?.kind !== "Snapshot") continue;
      g.fillStyle = p.graphSnapshot;
      g.fillRect(a.x - half, a.y - ANCHOR_HALF, half * 2 + 1, ANCHOR_HALF * 2 + 1);
    }
  });
  // drawMainCurrentMarkers (WinrateGraph.java:3131-3171).
  const movePoint = anchors.find((a) => a.node === cur) ?? null;
  const scorePoint = d.scoreLeadLine && markerIndex >= 0 ? { x: X(markerIndex), y: SY(markerMean) } : null;
  if (movePoint) report.currentMarker = { node: cur, x: movePoint.x, y: movePoint.y };
  report.scoreMarker = scorePoint;
  // The current move's guide: a solid hairline through the plot at its column.
  const guideX = movePoint ? movePoint.x : cmovenum >= 0 ? X(cmovenum) : scorePoint ? scorePoint.x : null;
  if (guideX !== null) guides.push(() => line(g, guideX, posy, guideX, plotBottom - 1, p.graphGuide, 1, false));
  if (scorePoint && movePoint) {
    const dx = movePoint.x - scorePoint.x;
    const dy = movePoint.y - scorePoint.y;
    const overlap = MARKER_RADIUS * 2 + 2;
    report.markersOverlap = dx * dx + dy * dy <= overlap * overlap;
  }
  // The engine readout's other value: a dot in its engine's colour at the move before.
  front.push(() => {
    for (const e of engineCurrent) if (e.node !== cur) marker(g, e.x, e.y, DOT_RADIUS, e.black ? LINE : theme.engineWhite, theme.ground);
  });
  front.push(() => {
    if (scorePoint) marker(g, scorePoint.x, scorePoint.y, report.markersOverlap ? MARKER_RADIUS + 3 : MARKER_RADIUS, p.graphScoreMarker, theme.ground);
    // Overlapping: the move's smaller core, ringed, on the larger score marker.
    if (movePoint) marker(g, movePoint.x, movePoint.y, report.markersOverlap ? MARKER_RADIUS - 1 : MARKER_RADIUS, p.graphMoveMarker, theme.ground);
  });
  // The hovered move: a dashed guide and hollow dots on its values.
  const hoverYs: number[] = [];
  if (hoverX !== null) {
    const x = hoverX;
    guides.push(() => line(g, x, posy, x, plotBottom - 1, p.graphGuide, 1, true));
  }
  if (mwr >= 0) hoverYs.push(WY(mwr));
  if (mScoreMoveNum >= 0) hoverYs.push(SY(drawHoverScore));
  for (const e of engineHover) hoverYs.push(e.y);
  front.push(() => {
    if (mwr >= 0) hoverDot(g, X(mmovenum), WY(mwr), LINE, theme.ground);
    if (mScoreMoveNum >= 0) hoverDot(g, X(mScoreMoveNum), SY(drawHoverScore), SCORE, theme.ground);
    for (const e of engineHover) hoverDot(g, e.x, e.y, e.black ? LINE : theme.engineWhite, theme.ground);
  });

  // ---------------------------------------------------------------- readout chips (WinrateGraph.java:1474-1647)
  const valueFont = bold(readSize);
  g.font = valueFont;
  const vm = metrics(g);
  const chipH = vm.ascent + vm.descent + 4;
  /** A readout chip centred over the guide at `cx` at the top of the plot, or at its bottom when
   *  one of the values it reads (`ys`) lies in that band; kept inside the plot. */
  const placeChip = (parts: ChipPart[], cx: number, ys: number[]): Box => {
    let w = 2 * CHIP_PAD + CHIP_GAP * (parts.length - 1);
    for (const part of parts) {
      g.font = part.font;
      w += tabularWidth(g, part.text) + (part.swatch ? SWATCH : 0);
    }
    const x = Math.max(posx, Math.min(W0 - w, Math.round(cx + 0.5 - w / 2)));
    const topY = posy + 2;
    const clear = MARKER_RADIUS + 3;
    const y = ys.some((v) => v + clear >= topY && v - clear <= topY + chipH) ? plotBottom - 2 - chipH : topY;
    return { x, y, w, h: chipH };
  };
  const drawChip = (box: Box, parts: ChipPart[]) => {
    readoutBoxes.push(box);
    overlay.push(() => {
      g.fillStyle = p.graphChip;
      g.beginPath();
      g.roundRect(box.x, box.y, box.w, box.h, CHIP_RADIUS);
      g.fill();
      let x = box.x + CHIP_PAD;
      for (const part of parts) {
        g.font = part.font;
        if (part.swatch) {
          const lineY = box.y + i(box.h / 2) + 0.5;
          g.strokeStyle = part.swatch;
          g.lineWidth = 2;
          g.lineCap = "round";
          g.beginPath();
          g.moveTo(x + 1, lineY);
          g.lineTo(x + 11, lineY);
          g.stroke();
          g.lineCap = "butt";
          x += SWATCH;
        }
        g.fillStyle = part.color;
        fillTabular(g, part.text, x, box.y + 2 + vm.ascent);
        x += tabularWidth(g, part.text) + CHIP_GAP;
      }
    });
  };
  /** An engine-game readout: each side's win rate after its curve's swatch and side, Black first;
   *  returns the parts and the two values as the report gives them. */
  const engineParts = (values: { wr: number; black: boolean }[]) => {
    const parts: ChipPart[] = [];
    const texts: [string | null, string | null] = [null, null];
    for (const black of [true, false]) {
      const value = values.find((e) => e.black === black);
      if (!value) continue;
      const text = javaFixed1(Math.max(0, Math.min(100, value.wr)));
      texts[black ? 0 : 1] = text;
      parts.push({ text: black ? t("GameInfoDialog.black") : t("GameInfoDialog.white"), font: plain(readSize), color: p.graphLabel, swatch: black ? LINE : theme.engineWhite });
      parts.push({ text: `${text}%`, font: valueFont, color: p.graphText });
    }
    return { parts, texts };
  };
  const currentParts: ChipPart[] = [];
  if (eg) {
    const readout = engineParts(engineCurrent);
    currentParts.push(...readout.parts);
    [report.engineGame!.blackText, report.engineGame!.whiteText] = readout.texts;
  }
  if (cwr >= 0) {
    report.currentText = javaFixed1(cwr);
    currentParts.push({ text: `${report.currentText}%`, font: valueFont, color: p.graphText });
  }
  if (curScoreMoveNum >= 0) {
    report.currentScoreText = formatScoreLead(drawCurScore);
    currentParts.push({ text: report.currentScoreText, font: valueFont, color: p.graphScoreText });
  }
  const markerYs = [movePoint?.y, scorePoint?.y, ...engineCurrent.map((e) => e.y)].filter((y): y is number => y !== undefined);
  let currentBox = guideX !== null && currentParts.length ? placeChip(currentParts, guideX, markerYs) : null;
  let hoverBox: Box | null = null;
  if (hoverX !== null) {
    const parts: ChipPart[] = [{ text: product("graphMove", hoverMove), font: plain(readSize), color: p.graphLabel }];
    if (eg) {
      const readout = engineParts(engineHover);
      parts.push(...readout.parts);
      [report.engineGame!.hoverBlackText, report.engineGame!.hoverWhiteText] = readout.texts;
    }
    if (mwr >= 0) {
      report.hoverText = javaFixed1(mwr);
      parts.push({ text: `${report.hoverText}%`, font: valueFont, color: p.graphText });
    }
    if (mScoreMoveNum >= 0) {
      report.hoverScoreText = formatScoreLead(drawHoverScore);
      parts.push({ text: report.hoverScoreText, font: valueFont, color: p.graphScoreText });
    }
    hoverBox = placeChip(parts, hoverX, hoverYs);
    drawChip(hoverBox, parts);
  }
  // While hovering, the current readout yields where the two would meet.
  if (currentBox && hoverBox && intersects({ x: currentBox.x - INSET, y: currentBox.y, w: currentBox.w + 2 * INSET, h: currentBox.h }, hoverBox)) currentBox = null;
  if (currentBox) drawChip(currentBox, currentParts);

  // ---------------------------------------------------------------- legend (WinrateGraph.java:1774-1830)
  // Under the data on the chip ground: the curves, blunder bars, guides and markers draw over it,
  // so it never hides one. It takes the corner of the plot (top right, top left, bottom left,
  // bottom right) no readout, current marker or scale label takes that has the least curve,
  // current / hover guide and blunder bar under it, on one row, or with the names stacked when
  // that leaves less under it (a narrow box fits beside a readout or a guide); it is left out
  // where something takes every corner or it is wider than the plot.
  const legend: [string, string][] = [];
  g.font = labelFont;
  if (eg) {
    // An engine game names each win-rate curve by its side and engine (cut so the stacked box
    // takes at most half the plot), and the lead by the side whose engine gives it.
    const nameWidth = (W0 - 2 * INSET) / 2 - 2 * CHIP_PAD - 16;
    if (winrateRenderable) {
      legend.push([elide(g, product("graphNamed", t("GameInfoDialog.black"), eg.black), nameWidth), LINE]);
      legend.push([elide(g, product("graphNamed", t("GameInfoDialog.white"), eg.white), nameWidth), theme.engineWhite]);
    }
    if (scoreRenderable) legend.push([product("graphNamed", t("WinrateGraph.legendScoreLead"), engineScoreSide === "White" ? t("GameInfoDialog.white") : t("GameInfoDialog.black")), SCORE]);
  } else {
    if (winrateRenderable) legend.push([t("WinrateGraph.legendWinrate"), LINE]);
    if (scoreRenderable) legend.push([t("WinrateGraph.legendScoreLead"), SCORE]);
  }
  const legendWidths = legend.map(([label]) => stringWidth(g, label));
  const layouts = [{ w: 2 * CHIP_PAD + legendWidths.reduce((sum, w) => sum + 16 + w, 0) + 10 * Math.max(0, legend.length - 1), h: labelH + 6, stacked: false }];
  // Stacked names sit closer than a full line (their ink is shorter than the line's ascent plus
  // descent), so the box fits above the 75 % scale labels of a panel of the default size.
  const stackStep = labelH - 2;
  if (legend.length > 1) layouts.push({ w: 2 * CHIP_PAD + 16 + Math.max(...legendWidths), h: 4 + labelH + (legend.length - 1) * stackStep, stacked: true });
  const taken: Box[] = [...scaleBoxes];
  if (currentBox) taken.push(currentBox);
  if (hoverBox) taken.push(hoverBox);
  for (const pt of [movePoint, scorePoint, ...engineCurrent]) if (pt) markerBoxes.push({ x: pt.x - MARKER_RADIUS - 3, y: pt.y - MARKER_RADIUS - 3, w: 2 * MARKER_RADIUS + 6, h: 2 * MARKER_RADIUS + 6 });
  taken.push(...markerBoxes);
  const strokes = [...winStrokes, ...scoreStrokes].map((stroke) => stroke.points);
  for (const x of [guideX, hoverX]) if (x !== null) strokes.push([[x, posy], [x, plotBottom]]);
  let chosen: { box: Box; stacked: boolean } | undefined;
  let covered = Infinity;
  for (const { w, h, stacked } of layouts) {
    for (const [x, y] of [[W0 - w - INSET, posy + 2], [INSET, posy + 2], [INSET, plotBottom - 2 - h], [W0 - w - INSET, plotBottom - 2 - h]]) {
      const box = { x, y, w, h };
      if (x < 0 || x + w > W0 || y < posy || y + h > plotBottom || taken.some((other) => intersects(other, box))) continue;
      // A curve passing just outside the box still touches its edge at the curve's width.
      const near = { x: x - 2, y: y - 2, w: w + 4, h: h + 4 };
      let under = strokes.reduce((sum, points) => sum + lengthInside(points, near), 0);
      for (const bar of barBoxes) if (intersects(bar, near)) under += Math.min(bar.x + bar.w, near.x + near.w) - Math.max(bar.x, near.x);
      if (under < covered) [chosen, covered] = [{ box, stacked }, under];
    }
  }
  if (W0 >= 240 && legend.length && chosen) {
    const { box, stacked } = chosen;
    report.legend = legend.map(([label]) => label);
    legendLayer.push(() => {
      g.fillStyle = p.graphChip;
      g.beginPath();
      g.roundRect(box.x, box.y, box.w, box.h, CHIP_RADIUS);
      g.fill();
      g.font = labelFont;
      let cx = box.x + CHIP_PAD;
      let top = box.y + (stacked ? 2 : 3);
      legend.forEach(([label, color], k) => {
        const lineY = top + i(labelH / 2) + 0.5;
        g.strokeStyle = color;
        g.lineWidth = 2;
        g.lineCap = "round";
        g.beginPath();
        g.moveTo(cx + 1, lineY);
        g.lineTo(cx + 11, lineY);
        g.stroke();
        g.lineCap = "butt";
        g.fillStyle = p.graphLabel;
        g.fillText(label, cx + 16, top + lm.ascent);
        if (stacked) top += stackStep;
        else cx += 16 + legendWidths[k] + 10;
      });
    });
  }
  caption();
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
