// Canvas painters reproducing the fixed Java drawing (wimi321/lizzieyzy-next cb53acc).
// Analysis presentation consumes node-persisted projections, independent of engine pondering.
import type {
  BoardOverlayView,
  CandidateView,
  CoordinateStyle,
  DisplaySettings,
  HistoryPoint,
  LastAction,
  Mark,
  MoveRank,
  NodeAnalysisView,
  Point,
  RawNetView,
  Stone,
  SubBoardMode,
  TreeNodeView,
} from "../contract/generated";
import { t } from "../i18n";
import type { BoardGeom, Rect } from "./layout";
import { graphGeom } from "./layout";

const COLS = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
const COLS_WITH_I = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/** The 坐标格式 labels (BoardRenderer.java:1038-1099 with Board.asName, Board.java:444-467):
 *  columns are letters without I, letters with I (含I / 野狐), or numbers (数字); wider boards than
 *  the alphabet number them. Rows count up from the bottom unless numbered from the top or 野狐. */
export function coordinateLabels(style: CoordinateStyle, cols: number, rows: number): { cols: string[]; rows: string[] } {
  const numeric = style === "NumbersFromTop" || style === "NumbersFromBottom";
  const alphabet = style === "WithI" || style === "Fox" ? COLS_WITH_I : COLS;
  const fromTop = style === "NumbersFromTop" || style === "Fox";
  return {
    cols: Array.from({ length: cols }, (_, k) => (numeric || cols > alphabet.length ? `${k + 1}` : alphabet[k])),
    rows: Array.from({ length: rows }, (_, k) => `${rows <= 25 && !fromTop ? rows - k : k + 1}`),
  };
}

const FONT = '"DejaVu Sans", "Noto Sans CJK SC", sans-serif';
/** LizzieFrame.playoutsFont = Font("Dialog.plain", 12) (Utils.java:95): Arial on Windows, Lucida
 * Grande on macOS, DejaVu Sans on common Linux. */
const DIALOG_FONT = navigator.userAgent.includes("Windows")
  ? "Arial"
  : navigator.userAgent.includes("Mac OS")
    ? '"Lucida Grande"'
    : '"DejaVu Sans"';
/** Config.sysDefaultFontName = Dialog.plain; CJK falls back like the JDK composite font. */
export const GRAPH_FONT = `${DIALOG_FONT}, "Noto Sans CJK SC", "Microsoft YaHei", sans-serif`;
const i = Math.trunc;

/** LizzieFrame.uiFont and playoutsFont: Font(Config.sysDefaultFontName, PLAIN, 12) (Utils.java:94-95). */
const UI_FONT = GRAPH_FONT;
/** LizzieFrame.winrateFont: the bundled Lizzie font (Utils.java:96-101). */
const WINRATE_FONT = `"Lizzie Winrate", ${FONT}`;

/** The hovered candidate's variation as the main board shows it (BoardRenderer.drawBranch); `order`
 *  is -1 for the 下一手 详细 point's variation (BoardRenderer.mouseOveredMove). */
export interface PreviewVariation {
  order: number;
  pv: (Point | null)[];
  pv_captures: Point[][];
  /** kata-analyze `pvVisits`, one per `pv` move; empty when the engine sends none. */
  pv_visits: number[];
  /** Moves shown: limit-branch-length, or the branch replay's displayed length. */
  length: number;
}

/** What the main board shows for the current node. */
export interface BoardScene {
  cols: number;
  rows: number;
  stones: (Stone | null)[];
  lastAction: LastAction | null;
  toPlay: Stone;
  marks: Mark[];
  nextMoves: Point[];
  /** Actual first child only; a pass/setup first child has no point even if later branches do. */
  nextMainMove: Point | null;
  nextColor: Stone | null;
  black: string;
  white: string;
  /** 棋盘下方显示黑白名字 with a player name to show (BoardRenderer.isEmptyName). */
  names: boolean;
  snapshot: boolean;
  mainTrunk: boolean;
  analysis: NodeAnalysisView | null;
  overlay: BoardOverlayView;
  display: DisplaySettings;
  suggestionLimit: number;
  /** Points ("x,y") of the candidate kept beyond 最大候选点数 (MoveData.lastTimeUnlimited). */
  unlimited: Set<string>;
  /** 显示候选点 for the side to move (F, holding Z, 延迟显示候选点). */
  candidatesShown: boolean;
  /** 显示策略网络 (T). */
  policy: boolean;
  /** 纯网络 (H) is on. */
  rawNetOn: boolean;
  /** Its result for this position; null until it arrives or for another position. */
  rawNet: RawNetView | null;
  katago: boolean;
  engine: boolean;
  /** The hovered candidate (`order`), while hovering candidates applies. */
  hovered: number | null;
  /** The hovered candidate's variation, null when no preview shows. */
  preview: PreviewVariation | null;
}

/** What the main board drew, for the acceptance probe. */
export interface BoardReport {
  coordinates: boolean;
  /** Below the board: player names, the pure network line, or nothing. */
  bottomLine: "names" | "rawNet" | null;
  rawNetInfo: string | null;
  moveNumbers: { x: number; y: number; text: string; red: boolean }[];
  lastMoveMarker: Point | null;
  ranks: { x: number; y: number; rank: MoveRank; last: boolean }[];
  rankCorner: boolean;
  candidateMode: "stats" | "policy" | "rawNet" | "hidden";
  candidates: { x: number; y: number; texts: string[]; badge: string | null }[];
  rawNetPoints: number;
  ownership: string | null;
  nextMove: { x: number; y: number; texts: string[]; fill: string } | null;
  preview: { order: number; length: number; numbers: number } | null;
  /** Coordinate labels drawn along the top and the left edge (坐标格式). */
  coordinateLabels: { cols: string[]; rows: string[] } | null;
  /** The best candidate's ring: blue (show-blue-ring) or the faint black ring. */
  bestRing: "blue" | "plain" | null;
  /** Grey unimportant-candidate circles drawn (show-nosugg-circle, rank 20 and beyond). */
  unimportant: number;
  /** Rank badges drawn in the white style (white-suggestion-order-white while White plays). */
  whiteOrderBadges: boolean;
  /** PV visits labels drawn on the hovered variation. */
  pvVisits: { x: number; y: number; text: string }[];
}

export const images: Record<string, HTMLImageElement> = {};
export function loadImages(names: string[]): Promise<void> {
  return Promise.all(
    names.map(
      (n) =>
        new Promise<void>((resolve) => {
          const img = new Image();
          img.onload = () => resolve();
          img.onerror = () => resolve();
          img.src = `/assets/${n}`;
          images[n] = img;
        }),
    ),
  ).then(() => undefined);
}

export function prepare(canvas: HTMLCanvasElement, r: Rect): CanvasRenderingContext2D {
  canvas.style.left = `${r.x}px`;
  canvas.style.top = `${r.y}px`;
  canvas.style.width = `${r.w}px`;
  canvas.style.height = `${r.h}px`;
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(r.w * dpr));
  const h = Math.max(1, Math.round(r.h * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, r.w, r.h);
  return g;
}

function circle(g: CanvasRenderingContext2D, x: number, y: number, r: number) {
  g.beginPath();
  g.arc(x, y, r, 0, Math.PI * 2);
}

const patterns = new WeakMap<CanvasRenderingContext2D, Map<HTMLImageElement, CanvasPattern>>();

/** Tile `img` over the rect with the texture anchored at (ax, ay) (TexturePaint semantics). */
function drawTexture(g: CanvasRenderingContext2D, img: HTMLImageElement | undefined, x: number, y: number, w: number, h: number, ax = x, ay = y) {
  if (!img?.naturalWidth) {
    g.fillStyle = "rgb(217,152,77)";
    g.fillRect(x, y, w, h);
    return;
  }
  let byImage = patterns.get(g);
  if (!byImage) patterns.set(g, (byImage = new Map()));
  let pattern = byImage.get(img);
  if (!pattern) byImage.set(img, (pattern = g.createPattern(img, "repeat")!));
  pattern.setTransform(new DOMMatrix([1, 0, 0, 1, ax, ay]));
  g.fillStyle = pattern;
  g.fillRect(x, y, w, h);
}

/** jhlabs GaussianFilter(radius) on an opaque region: makeKernel (sigma = radius / 3, taps beyond
 * the radius zeroed, normalised), then two convolveAndTranspose passes with CLAMP_EDGES, each
 * rounding to (int)(v + 0.5). Canvas `filter` is not used: WebKitGTK ignores it. */
function blurRegion(g: CanvasRenderingContext2D, r: Rect, radius: number) {
  const dpr = window.devicePixelRatio || 1;
  const rad = radius * dpr;
  const [x, y, w, h] = [Math.round(r.x * dpr), Math.round(r.y * dpr), Math.round(r.w * dpr), Math.round(r.h * dpr)];
  const reach = Math.ceil(rad);
  const sigma = rad / 3;
  const kernel = Array.from({ length: reach * 2 + 1 }, (_, k) => {
    const d = (k - reach) ** 2;
    return d > rad * rad ? 0 : Math.exp(-d / (2 * sigma * sigma)) / Math.sqrt(2 * Math.PI * sigma);
  });
  const total = kernel.reduce((a, b) => a + b, 0);
  const weights = kernel.map((v) => v / total);
  const img = g.getImageData(x, y, w, h);
  const px = img.data;
  const tmp = new Uint8ClampedArray(px.length);
  const pass = (src: Uint8ClampedArray, dst: Uint8ClampedArray, horizontal: boolean) => {
    const [n, lines] = horizontal ? [w, h] : [h, w];
    for (let l = 0; l < lines; l++)
      for (let a = 0; a < n; a++) {
        let r0 = 0;
        let g0 = 0;
        let b0 = 0;
        for (let s = -reach; s <= reach; s++) {
          const wt = weights[s + reach];
          if (!wt) continue;
          const p = Math.min(n - 1, Math.max(0, a + s));
          const o = (horizontal ? l * w + p : p * w + l) * 4;
          r0 += src[o] * wt;
          g0 += src[o + 1] * wt;
          b0 += src[o + 2] * wt;
        }
        const o = (horizontal ? l * w + a : a * w + l) * 4;
        dst[o] = i(r0 + 0.5);
        dst[o + 1] = i(g0 + 0.5);
        dst[o + 2] = i(b0 + 0.5);
        dst[o + 3] = 255;
      }
  };
  pass(px, tmp, true);
  pass(tmp, px, false);
  g.putImageData(img, x, y);
}

/** LizzieFrame.createBackground (wallpaper tiled from the panel origin) plus drawContainer for each
 * panel: GlassEffectRenderer.drawGlassPanel(FROSTED, corner 16) with the baseline theme values
 * glass-blur-radius 10, overlay (24,24,26,102), border (255,255,255,26), highlight (255,255,255,77).
 * Painted once per size into a detached canvas and returned as a PNG blob: a live full-window canvas
 * layer would be recomposited every frame by WebKitGTK. */
export function paintBackground(w: number, h: number, glass: Rect[]): Promise<Blob | null> {
  const canvas = document.createElement("canvas");
  const g = prepare(canvas, { x: 0, y: 0, w, h });
  drawTexture(g, images["background.jpg"], 0, 0, w, h, 0, 0);
  for (const r of glass) {
    if (r.w <= 0 || r.h <= 0 || r.x < 0 || r.y < 0 || r.x + r.w > w || r.y + r.h > h) continue;
    blurRegion(g, r, 10);
    g.fillStyle = "rgba(24,24,26,0.4)";
    g.beginPath();
    g.roundRect(r.x, r.y, r.w, r.h, 8);
    g.fill();
    g.strokeStyle = "rgba(255,255,255,0.102)";
    g.lineWidth = 1;
    g.beginPath();
    g.roundRect(r.x + 0.5, r.y + 0.5, r.w, r.h, 8);
    g.stroke();
    const light = g.createLinearGradient(0, r.y, 0, r.y + r.h * 0.3);
    light.addColorStop(0, "rgba(255,255,255,0.302)");
    light.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = light;
    g.beginPath();
    g.roundRect(r.x + 1, r.y + 1, r.w - 2, i(r.h / 3), 8);
    g.fill();
  }
  const { promise, resolve } = Promise.withResolvers<Blob | null>();
  canvas.toBlob(resolve);
  return promise;
}

/** BoardRenderer.drawShadow (theme shadow-size 85). */
function stoneShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  g.save();
  g.globalAlpha = 0.85;
  const sh = g.createRadialGradient(cx + r * 0.12, cy + r * 0.12, r * 0.6, cx + r * 0.12, cy + r * 0.12, r * 1.18);
  sh.addColorStop(0, "rgba(0,0,0,0.55)");
  sh.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = sh;
  circle(g, cx + r * 0.12, cy + r * 0.12, r * 1.18);
  g.fill();
  g.restore();
}

/** BoardRenderer.drawShadow2 (BoardRenderer.java:3757-3789) under policy and raw-net circles: a
 *  faint ring just outside the circle. Java never sets `cachedStoneCenter2`, so the cached image's
 *  gradient and fill sit at its corner and only the lower-right quarter of the ring is drawn. */
function ghostShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  const size = Math.max(1, i(i((r * 70) / 100) * 0.2));
  const outer = r + size;
  const width = 2 * outer + size;
  const ring = g.createRadialGradient(cx, cy, 0, cx, cy, outer);
  ring.addColorStop(Math.max(0, r / outer - 0.0001), "rgba(0,0,0,0)");
  ring.addColorStop(r / outer, "rgba(50,50,50,0.157)");
  ring.addColorStop(1, "rgba(0,0,0,0)");
  g.save();
  g.beginPath();
  g.rect(cx, cy, width, width);
  g.clip();
  g.fillStyle = ring;
  fillCircle(g, cx, cy, outer);
  g.restore();
}

/** BoardRenderer.drawShadow + theme stone image (shadow-size 85). */
function drawStone(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, black: boolean) {
  stoneShadow(g, cx, cy, r);
  const img = images[black ? "black0.png" : "white0.png"];
  if (img?.naturalWidth) g.drawImage(img, cx - r, cy - r, r * 2 + 1, r * 2 + 1);
  else {
    g.fillStyle = black ? "#111" : "#f4f4f4";
    circle(g, cx, cy, r);
    g.fill();
  }
}

function textMid(g: CanvasRenderingContext2D, s: string, x: number, y: number, size: number, maxW: number, color: string, family = FONT) {
  let px = size;
  g.font = `${px}px ${family}`;
  while (px > 4 && g.measureText(s).width > maxW) {
    px -= 0.5;
    g.font = `${px}px ${family}`;
  }
  g.fillStyle = color;
  g.textAlign = "center";
  g.textBaseline = "middle";
  g.fillText(s, x, y);
}

/** Java FontMetrics ints (FontDesignMetrics rounds each value as (int)(0.95 + v)). */
function javaMetrics(g: CanvasRenderingContext2D) {
  const m = g.measureText("0");
  return { ascent: i(0.95 + m.fontBoundingBoxAscent), descent: i(0.95 + m.fontBoundingBoxDescent) };
}

function grid(g: CanvasRenderingContext2D, geo: BoardGeom, ox: number, oy: number, border: number, normal: number) {
  const sq = geo.square;
  g.strokeStyle = "#000";
  for (let k = 0; k < geo.cols; k++) {
    g.lineWidth = k === 0 || k === geo.cols - 1 ? border : normal;
    const a = geo.margin + sq * k;
    g.beginPath();
    g.moveTo(ox + a + 0.5, oy + geo.margin);
    g.lineTo(ox + a + 0.5, oy + geo.margin + sq * (geo.rows - 1));
    g.stroke();
  }
  for (let k = 0; k < geo.rows; k++) {
    g.lineWidth = k === 0 || k === geo.rows - 1 ? border : normal;
    const a = geo.margin + sq * k;
    g.beginPath();
    g.moveTo(ox + geo.margin, oy + a + 0.5);
    g.lineTo(ox + geo.margin + sq * (geo.cols - 1), oy + a + 0.5);
    g.stroke();
  }
}

/** Star points of the standard sizes (BoardStylePainter). */
function hoshi(cols: number, rows: number): [number, number][] {
  if (cols !== rows) return [];
  const n = cols;
  const pts = n === 19 ? [3, 9, 15] : n === 13 ? [3, 6, 9] : n === 9 ? [2, 4, 6] : [];
  const out: [number, number][] = [];
  for (const a of pts) for (const b of pts) if (!(n === 9 && (a === 4) !== (b === 4))) out.push([a, b]);
  return out;
}

/** Utils.getPlayoutsString (Utils.java:992-1002), including Java's trailing decimal. */
export function playouts(n: number): string {
  if (n >= 10_000_000) return `${(Math.round(n / 100_000) / 10).toFixed(1)}m`;
  if (n >= 9950) return `${Math.round(n / 1000)}k`;
  if (n >= 1000) return `${(Math.round(n / 100) / 10).toFixed(1)}k`;
  return String(n);
}

// ------------------------------------------------------------------ Java drawing primitives

/** fillCircle: fillOval(x - r, y - r, 2r + 1, 2r + 1). */
function fillCircle(g: CanvasRenderingContext2D, x: number, y: number, radius: number) {
  g.beginPath();
  g.arc(x + 0.5, y + 0.5, radius + 0.5, 0, Math.PI * 2);
  g.fill();
}

/** drawCircle(g, x, y, radius, f): a BasicStroke(radius / f) oval. */
function strokeCircle(g: CanvasRenderingContext2D, x: number, y: number, radius: number, width: number) {
  g.lineWidth = width;
  circle(g, x, y, radius);
  g.stroke();
}

/** drawPolygonSmall: the red corner triangle at the upper left of a stone. */
function smallTriangle(g: CanvasRenderingContext2D, x: number, y: number, r: number) {
  const a = i((r * 16) / 15), b = i((r * 4) / 11);
  g.beginPath();
  g.moveTo(x - a, y - a);
  g.lineTo(x - a, y - b);
  g.lineTo(x - b, y - a);
  g.closePath();
  g.fill();
}

/** java.awt.Color.HSBtoRGB with an alpha (0-255). */
function hsb(hue: number, saturation: number, brightness: number, alpha = 255): string {
  const h = (hue - Math.floor(hue)) * 6;
  const f = h - Math.floor(h);
  const p = brightness * (1 - saturation);
  const q = brightness * (1 - saturation * f);
  const t = brightness * (1 - saturation * (1 - f));
  const [r, g, b] = [[brightness, t, p], [q, brightness, p], [p, brightness, t], [p, q, brightness], [t, p, brightness], [brightness, p, q]][i(h)] ?? [brightness, t, p];
  return `rgba(${i(r * 255 + 0.5)},${i(g * 255 + 0.5)},${i(b * 255 + 0.5)},${alpha / 255})`;
}

/** drawLeelazSuggestions hues: RGBtoHSB(2, 0, 0), RGBtoHSB(0, 255, 0) and best-move-color (cyan). */
const RED_HUE = 0;
const GREEN_HUE = 1 / 3;
const CYAN_HUE = 0.5;
/** BoardRenderer.maxAlpha and drawLeelazSuggestions' minAlpha / alphaFactor. */
const MAX_ALPHA = 240;
const MIN_ALPHA = 32;
function suggestionAlpha(percent: number): number {
  return i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * Math.max(0, Math.log(percent) / 5 + 1));
}

/** BoardRenderer.drawString (BoardRenderer.java:4253-4289): the 100-point font scaled to `maxW`,
 *  capped at `maxH`, centred on x; aboveOrBelow 0 centres on y, 1 puts the baseline on y. */
function jString(g: CanvasRenderingContext2D, s: string, x: number, y: number, family: string, maxH: number, maxW: number, aboveOrBelow = 0, bold = false) {
  const weight = bold ? "bold " : "";
  g.font = `${weight}100px ${family}`;
  const size = Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(s).width))));
  g.font = `${weight}${Math.max(1, size)}px ${family}`;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const fm = javaMetrics(g);
  const height = fm.ascent - fm.descent;
  const offset = aboveOrBelow === -1 ? i(height / 2) : aboveOrBelow === 1 ? -i(height / 2) : 0;
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y + i(height / 2) + offset);
}

/** The wood texture under a disc (drawLeelazSuggestions / drawCircleBackground: setPaint(paint)). */
function woodDisc(g: CanvasRenderingContext2D, x: number, y: number, radius: number, alpha: number, ox: number, oy: number) {
  g.save();
  g.beginPath();
  g.arc(x + 0.5, y + 0.5, radius + 0.5, 0, Math.PI * 2);
  g.clip();
  g.globalAlpha = alpha;
  drawTexture(g, images["board.png"], x - radius - 1, y - radius - 1, 2 * radius + 3, 2 * radius + 3, ox, oy);
  g.restore();
}

/** Java String.valueOf(round(v * 10) / 10.0). */
function round1(v: number): string {
  return javaDouble(Math.round(v * 10) / 10 + 0);
}

/** Utils.convertScoreToString with 目差显示为与最佳选点之差: the difference to the best score. */
function scoreDiff(diff: number): string {
  const rounded = Math.round(Math.abs(diff) * 10) / 10;
  return diff > 0 ? `+${javaDouble(rounded)}` : diff < 0 ? `-${javaDouble(rounded)}` : javaDouble(Math.round(diff * 10) / 10 + 0);
}

/** MoveRankDefinition.Rank colours (classic palette) and boardMarkRadiusFactor. */
const RANK_COLOR: Record<MoveRank, string> = {
  Best: "rgb(0,180,0)",
  Good: "rgb(140,202,34)",
  Normal: "rgb(180,180,0)",
  Inaccuracy: "rgb(200,140,50)",
  Mistake: "rgb(208,16,19)",
  Blunder: "rgb(155,25,150)",
};
const RANK_RADIUS: Record<MoveRank, number> = { Best: 0.1, Good: 0.1, Normal: 0.1225, Inaccuracy: 0.145, Mistake: 0.1675, Blunder: 0.19 };

// ------------------------------------------------------------------ variations

interface Preview {
  stones: (Stone | null)[];
  numbers: Map<number, number>;
  /** Branch.pvVisitsList: the `pvVisits` of the move last placed on each point. */
  visits: Map<number, number>;
  captured: Map<number, Stone>;
  last: number | null;
  /** The last shown move is a pass. */
  pass: boolean;
  /** Side to play after the shown moves. */
  toPlay: Stone;
}

/** VariationPreviewGenerator / SubBoardRenderer Branch over the core's `Branch` result: each
 *  move's stone and the stones it captures come from the view; with 变化图中去掉死子 off
 *  (`removeDead` false) the captured stones stay on the board (`withSimulation`). */
function playVariation(stones: (Stone | null)[], toPlay: Stone, cols: number, v: { pv: (Point | null)[]; pv_captures: Point[][]; pv_visits?: number[] }, length: number, removeDead: boolean): Preview {
  const out = stones.slice();
  const numbers = new Map<number, number>();
  const visits = new Map<number, number>();
  const captured = new Map<number, Stone>();
  let color = toPlay;
  let last: number | null = null;
  let pass = false;
  for (let k = 0; k < Math.min(length, v.pv.length); k++) {
    const pt = v.pv[k];
    last = null;
    pass = !pt;
    if (pt) {
      const p = pt.y * cols + pt.x;
      out[p] = color;
      captured.delete(p);
      numbers.set(p, k + 1);
      const n = v.pv_visits?.[k];
      if (n !== undefined) visits.set(p, n);
      for (const q of removeDead ? v.pv_captures[k] ?? [] : []) {
        const index = q.y * cols + q.x;
        const stone = out[index];
        if (stone) captured.set(index, stone);
        out[index] = null;
        numbers.delete(index);
      }
      last = p;
    }
    color = color === "Black" ? "White" : "Black";
  }
  return { stones: out, numbers, visits, captured, last, pass, toPlay: color };
}

/** BoardRenderer.drawPass (BoardRenderer.java:504-523): translucent disc at the centre. */
function drawPassDisc(g: CanvasRenderingContext2D, ox: number, oy: number, geo: BoardGeom, blackToPlay: boolean) {
  const r = geo.stoneRadius;
  g.fillStyle = blackToPlay ? "rgba(255,255,255,0.314)" : "rgba(0,0,0,0.314)";
  g.beginPath();
  g.ellipse(ox + i(geo.size / 2), oy + i(geo.size / 2), i((r * 5) / 2), i((r * 5) / 2), 0, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = blackToPlay ? "rgba(0,0,0,0.784)" : "rgba(255,255,255,0.784)";
  jString(g, t("BoardRenderer.pass"), ox + i(geo.size / 2), oy + i(geo.size / 2), UI_FONT, r * 3, i((r * 9) / 2));
}

/** VariationPreviewGenerator.drawPvVisits (VariationPreviewGenerator.java:384-411): the orange
 *  strip above the number with the move's visits. */
function drawPvVisits(g: CanvasRenderingContext2D, x: number, y: number, visits: number, last: boolean, sq: number) {
  const left = last ? 0.43 : visits >= 1000 ? 0.3 : 0.25;
  const width = last ? (visits >= 1000 ? 0.93 : 0.9) : visits >= 1000 ? 0.8 : 0.72;
  g.fillStyle = "#ffc800"; // java.awt.Color.ORANGE
  g.fillRect(i(x - sq * left), i(y - sq * 0.5), i(sq * width), Math.round(sq * 0.33));
  g.fillStyle = "#000";
  jString(g, playouts(visits), i(x + sq * 0.1), i(y - sq * 0.2), UI_FONT, sq * 0.33, sq * 0.8, 1);
}

/** drawPolygonSmallPv (VariationPreviewGenerator.java:466-480), called with the square width. */
function smallPvTriangle(g: CanvasRenderingContext2D, x: number, y: number, sq: number) {
  g.beginPath();
  g.moveTo(x - i((sq * 9) / 20), i(y - sq * 0.5));
  g.lineTo(x - i((sq * 9) / 20), i(y - sq * 0.18));
  g.lineTo(x - i((sq * 17) / 40), i(y - sq * 0.18));
  g.lineTo(x - i((sq * 11) / 40), i(y - sq * 0.5));
  g.closePath();
  g.fill();
}

/** VariationPreviewGenerator.drawAnnotations (VariationPreviewGenerator.java:316-369): branch
 *  numbers 1..n, the last one red with the corner triangle; the hovered point keeps its candidate
 *  information unless a later move was played there. 变化图显示计算量 adds each move's visits above
 *  its number (the last move only, or every move) when above pv-visits-limit. Returns
 *  hideMouseOverInfo. */
function drawPreviewNumbers(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, geo: BoardGeom, hover: number, cols: number, d: DisplaySettings, report: BoardReport): boolean {
  const r = geo.stoneRadius, sq = geo.square;
  const visitsLast = d.pvVisits !== "Off", visitsAll = d.pvVisits === "AllMoves";
  let hide = false;
  for (const [p, n] of preview.numbers) {
    if (p === hover) {
      if (n > 1) hide = true;
      else continue;
    }
    const x = px(p), y = py(p);
    const last = p === preview.last;
    const visits = preview.visits.get(p) ?? 0;
    const showVisits = (last ? visitsLast : visitsAll) && visits > d.pvVisitsLimit;
    if (showVisits) {
      drawPvVisits(g, x, y, visits, last, sq);
      report.pvVisits.push({ x: p % cols, y: i(p / cols), text: playouts(visits) });
    }
    if (last) {
      g.fillStyle = "#f00";
      if (showVisits) smallPvTriangle(g, x, y, sq);
      else smallTriangle(g, x, y, r);
    } else g.fillStyle = preview.stones[p] === "Black" ? "#fff" : "#000";
    const wide = !last && n >= 100;
    if (showVisits) jString(g, String(n), x, i(y + sq * 0.12), UI_FONT, r * 1.3, i(r * (wide ? 1.85 : 1.4)));
    else jString(g, String(n), x, y, UI_FONT, r * 1.4, i(r * (n >= 100 ? 1.85 : 1.4)));
    report.moveNumbers.push({ x: p % cols, y: i(p / cols), text: String(n), red: last });
  }
  return hide;
}

function drawPreviewStones(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, r: number) {
  for (const [p, stone] of preview.captured) {
    if (preview.stones[p]) continue;
    g.fillStyle = stone === "Black" ? "rgba(0,0,0,0.235)" : "rgba(255,255,255,0.333)";
    circle(g, px(p), py(p), r);
    g.fill();
  }
}

// ------------------------------------------------------------------ ownership (纯网络 H)

type PureEstimate = DisplaySettings["pureEstimate"];

/** BoardRenderer.convertLength. */
function convertLength(v: number): number {
  const a = Math.abs(v);
  return a > 0.2 ? (a * 6) / 10 : 0;
}

/** BoardRenderer / SubBoardRenderer drawKataEstimateBySize and drawKataEstimateByTransparent
 *  (BoardRenderer.java:1262-1380, SubBoardRenderer.java:594-706) for a Black-positive ownership.
 *  Java hands them side-to-move-positive values with the side to move; both say Black owns a point
 *  exactly when the Black-positive value is positive. "Small" uses the small squares Java drew
 *  when the KataGo estimate's own small option was set (an objective fix: the pure-network choice
 *  decides); the sub board has no small squares in Java. */
function drawOwnership(g: CanvasRenderingContext2D, own: number[], stones: (Stone | null)[], cols: number, style: PureEstimate, X: (x: number) => number, Y: (y: number) => number, sq: number, main: boolean) {
  const big = style === "BigBelow";
  for (let k = 0; k < own.length; k++) {
    const o = own[k];
    if (!o) continue;
    const black = o > 0;
    const x = X(k % cols), y = Y(i(k / cols));
    if (style === "BySize") {
      const length = i(convertLength(o) * sq);
      g.fillStyle = black ? "rgba(0,0,0,0.706)" : "rgba(255,255,255,0.706)";
      if (length > 0) g.fillRect(x - i(length / 2), y - i(length / 2), length, length);
      continue;
    }
    if (!big && style === "TransparentNotOnLive" && stones[k] === (black ? "Black" : "White")) continue;
    const alpha = Math.abs(i(o * (big ? (black ? 105 : 165) : 255)));
    if (black && !big && stones[k] === "Black") {
      const v = 127 - i((alpha - 1) / 2);
      g.fillStyle = `rgb(${v},${v},${v})`;
    } else g.fillStyle = black ? `rgba(0,0,0,${alpha / 255})` : `rgba(255,255,255,${alpha / 255})`;
    if (big) g.fillRect(x - i((sq * 5) / 10), y - i((sq * 5) / 10), sq, sq);
    else if (main && style === "Small") {
      const length = Math.round(sq / (black ? 8 : 6));
      g.fillRect(x - length, y - length, length * 2, length * 2);
    } else g.fillRect(x - i(sq / 4), y - i(sq / 4), i(sq / 2), i(sq / 2));
  }
}

// ------------------------------------------------------------------ main board

/** drawStringForOrder (BoardRenderer.java:4291-4333): the rank badge at the upper right, black on
 *  orange; with white-suggestion-order-white while White plays, white on dark gold. */
function orderBadge(g: CanvasRenderingContext2D, x: number, y: number, order: number, sq: number, white: boolean) {
  const bx = Math.round(x + sq * 0.43) + (order === 0 ? 1 : 0);
  const by = Math.round(y - sq * 0.358) - (order === 0 ? 1 : 0);
  const label = String(order + 1);
  g.font = `100px ${WINRATE_FONT}`;
  const size = Math.min(sq * 0.36, Math.round((100 * sq * 0.39) / Math.max(1, Math.round(g.measureText(label).width))));
  g.font = `${size}px ${WINRATE_FONT}`;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const fm = javaMetrics(g);
  const height = fm.ascent - fm.descent;
  const width = Math.round(g.measureText(label).width);
  const left = bx - i(width / 2);
  g.fillStyle = white ? "rgb(155,118,36)" : "#ffc800"; // java.awt.Color.ORANGE
  g.fillRect(left, by - height, width, height + Math.max(1, i(height / 12)));
  g.fillStyle = white ? "#fff" : "#000";
  g.fillText(label, left, by);
}

/** drawOrder's colour choice: the white badge while White plays (blackToPlay false). */
function whiteBadges(s: BoardScene): boolean {
  return s.display.whiteSuggestionOrderWhite && s.toPlay === "White";
}

/** BoardRenderer.drawStringFor3row: Java's width-limited font, baseline on y. */
function suggestionText(g: CanvasRenderingContext2D, s: string, x: number, y: number, maxH: number, maxW: number, family: string) {
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.font = `100px ${family}`;
  const size = Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(s).width))));
  g.font = `${size}px ${family}`;
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y);
}

/** drawSuggestionText / drawTwoRowSuggestionField (BoardRenderer.java:2747-2913): the shown fields
 *  (win rate 0, playouts 1, score 2) in the rows of 自定义信息顺序. */
function drawSuggestionText(g: CanvasRenderingContext2D, geo: BoardGeom, x: number, y: number, texts: string[], colors: string[], fields: number[], defaultOrder: boolean, roundedWinrate: number, visits: number) {
  const r = geo.stoneRadius, sq = geo.square;
  const font = (field: number) => (field === 1 ? UI_FONT : WINRATE_FONT);
  if (fields.length === 3) {
    fields.forEach((field, row) => {
      g.fillStyle = colors[field];
      if (row === 0) {
        const narrow = defaultOrder && field === 0 && roundedWinrate < 10;
        suggestionText(g, texts[field], x, y - Math.round(sq * (narrow ? 0.127 : 0.125)), sq * (narrow ? 0.36 : 0.35), sq * 0.67, font(field));
      } else if (row === 1) {
        const defaultPlayouts = defaultOrder && field === 1;
        suggestionText(g, texts[field], x, y + Math.round(sq * 0.18), sq * (defaultPlayouts ? 0.34 : 0.32), defaultPlayouts && visits < 1000 ? r * 1.3 : r * 1.8, font(field));
      } else {
        suggestionText(g, texts[field], x, y + Math.round(sq * 0.435), ((sq * (geo.n - 1) + 1) * 0.273) / (geo.n - 1), r * 1.6, font(field));
      }
    });
  } else if (fields.length === 2) {
    for (const [field, other, first] of [[fields[0], fields[1], true], [fields[1], fields[0], false]] as const) {
      g.fillStyle = colors[field];
      if (!first)
        jString(g, texts[field], x, y + (field === 2 ? i((r * 4) / 9) : i((r * 15) / 35)), font(field), field === 2 ? r * 0.75 : r * 0.77, field === 2 ? r * 1.6 : r * 1.8);
      else if (field === 0)
        jString(g, texts[field], x, y - i(sq / (roundedWinrate < 10 ? 15 : 16)), font(field), r, sq * (roundedWinrate < 10 ? 0.57 : 0.735), 1);
      else if (field === 1) {
        const above = other === 0;
        jString(g, texts[field], x, y - (above ? i(sq / 15) : i(r / 15)), font(field), r * (above ? 0.77 : 0.82), r * (above ? 1.8 : 1.73), 1);
      } else jString(g, texts[field], x, y - i(sq / 16), font(field), r * 0.88, sq * 0.735, 1);
    }
  } else if (fields.length === 1) {
    const field = fields[0];
    g.fillStyle = colors[field];
    jString(g, texts[field], x, y, font(field), field === 0 ? sq * 0.46 : r, field === 2 ? r * 1.7 : r * 1.9);
  }
}

interface ShownCandidate {
  c: CandidateView;
  k: number;
  x: number;
  y: number;
  ratio: number;
  fraction: number;
  hovered: boolean;
  needShow: boolean;
  skipNumbers: boolean;
  /** The unimportant layer already drew its circle (hasDrawBackground). */
  background: boolean;
}

/** drawLeelazSuggestions' hue fraction: the allocation share raised to 1 / suggestion-color-ratio
 *  (BoardRenderer.java:2532-2536, 3127-3131). */
function hueFraction(ratio: number, s: BoardScene): number {
  return Math.pow(ratio, 1 / Math.max(1, s.display.suggestionColorRatio));
}

/** Rank beyond 最大候选点数 (`outOfOrder`), unless kept there (`lastTimeUnlimited`). */
function outOfOrder(c: CandidateView, s: BoardScene): boolean {
  return s.suggestionLimit > 0 && c.order + 1 > s.suggestionLimit && !(c.point && s.unlimited.has(`${c.point.x},${c.point.y}`));
}

const samePoint = (a: Point | null | undefined, b: Point | null | undefined): boolean => !!a && !!b && a.x === b.x && a.y === b.y;

/** The candidate drawn as hovered: the hovered rank, or the candidate on the 下一手 详细 point
 *  while its variation shows (the selection's coordinate, BoardRenderer.java:2507-2513). */
function isHovered(c: CandidateView, s: BoardScene): boolean {
  return s.hovered === c.order || (s.preview?.order === -1 && samePoint(c.point, s.overlay.next_move?.point));
}

/** drawLeelazSuggestionsUnimportant (BoardRenderer.java:3057-3172): candidates of rank 20 and
 *  beyond, and with show-nosugg-circle those beyond 最大候选点数, get a plain disc and ring without
 *  text, except the actual next move. Returns the points covered (hasDrawBackground); draws them
 *  only when `draw` (no branch shows). */
function drawUnimportant(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, draw: boolean, report: BoardReport): Set<number> {
  const covered = new Set<number>();
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const r = geo.stoneRadius;
  for (let k = candidates.length - 1; k >= 0; k--) {
    const c = candidates[k], pt = c.point;
    if (!pt || c.visits <= 0 || pt.x >= s.cols || pt.y >= s.rows || pt.x < 0 || pt.y < 0) continue;
    const beyond = outOfOrder(c, s);
    if (!beyond && c.order < 20) continue;
    if (samePoint(pt, s.nextMainMove)) continue;
    if (!s.display.noSuggCircle && beyond) continue;
    covered.add(pt.y * s.cols + pt.x);
    if (!draw) continue;
    const ratio = max > 0 ? Math.min(1, c.allocation_visits / max) : 0;
    const alphaRatio = Math.max(0, Math.log(ratio) / 5 + 1);
    g.fillStyle = hsb(k === 0 ? CYAN_HUE : RED_HUE + (GREEN_HUE - RED_HUE) * hueFraction(ratio, s), 1, 0.85, i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * alphaRatio));
    fillCircle(g, X(pt.x), Y(pt.y), r + 1);
    g.strokeStyle = `rgba(0,0,0,${i(48 + 48 * alphaRatio) / 255})`;
    strokeCircle(g, X(pt.x), Y(pt.y), r + 1, (r + 1) / 26.5);
    report.unimportant++;
  }
  return covered;
}

/** Candidates drawLeelazSuggestions (BoardRenderer.java:2437-2632) shows, worst first; the move
 *  played and analysed further than its candidate (`ignored`) is returned apart. Without
 *  show-nosugg-circle the candidates beyond 最大候选点数 are not kept at all, except the one
 *  lastTimeUnlimited keeps (BoardData.tryToLimitMoves, BoardData.java:586-635); with it, those under an unimportant
 *  circle show nothing more unless hovered (2523-2524). The actual next move (needShow) and the
 *  hovered one keep their numbers (2547-2553, 2571-2572). */
function shownCandidates(s: BoardScene, X: (x: number) => number, Y: (y: number) => number, ignored: Point | null, covered: Set<number>): { shown: ShownCandidate[]; ignoredCandidate: CandidateView | null } {
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(1, ...candidates.map((c) => c.allocation_visits));
  const shown: ShownCandidate[] = [];
  let ignoredCandidate: CandidateView | null = null;
  for (let k = candidates.length - 1; k >= 0; k--) {
    const c = candidates[k], pt = c.point;
    if (!pt || c.visits <= 0 || pt.x >= s.cols || pt.y >= s.rows || pt.x < 0 || pt.y < 0) continue;
    const beyond = outOfOrder(c, s);
    if (beyond && !s.display.noSuggCircle) continue;
    if (samePoint(ignored, pt)) {
      ignoredCandidate = c;
      continue;
    }
    const hovered = isHovered(c, s);
    const background = covered.has(pt.y * s.cols + pt.x);
    if (beyond && !hovered && background) continue;
    const ratio = Math.min(1, c.allocation_visits / max);
    const needShow = samePoint(s.nextMainMove, pt);
    const lack = ratio <= s.display.minPlayoutRatio;
    shown.push({
      c,
      k,
      x: X(pt.x),
      y: Y(pt.y),
      ratio,
      fraction: k === 0 ? 0 : hueFraction(ratio, s),
      hovered,
      needShow,
      skipNumbers: (beyond || lack) && !hovered && k !== 0 && !needShow,
      background,
    });
  }
  return { shown, ignoredCandidate };
}

/** BoardRenderer.java:2573-2629: the wood under the colour, the HSB disc with allocation alpha and
 *  its ring: the best candidate's blue ring (show-blue-ring; dimmed on a next move) or else the
 *  faint black ring the others get, which a next move's dashed ring replaces (isOnNext). A point
 *  under an unimportant circle is not painted again unless hovered. Nothing while a branch shows. */
function drawCandidateDiscs(g: CanvasRenderingContext2D, geo: BoardGeom, shown: ShownCandidate[], next: Point[], ox: number, oy: number, s: BoardScene, report: BoardReport) {
  const r = geo.stoneRadius;
  for (const { c, k, x, y, ratio, fraction, hovered, needShow, skipNumbers, background } of shown) {
    if (background && !hovered) continue;
    if (!skipNumbers) woodDisc(g, x, y, r + 1, ratio < 0.05 && !hovered && !needShow ? 0.8 : 1, ox, oy);
    const onNext = next.some((n) => n.x === c.point!.x && n.y === c.point!.y);
    const alphaRatio = Math.max(0, Math.log(ratio) / 5 + 1);
    g.fillStyle = hsb(k === 0 ? CYAN_HUE : RED_HUE + (GREEN_HUE - RED_HUE) * fraction, 1, 0.85, i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * alphaRatio));
    fillCircle(g, x, y, r + 1);
    if (k === 0 && s.display.blueRing) {
      g.strokeStyle = onNext ? "rgba(0,0,255,0.667)" : "#00f";
      strokeCircle(g, x, y, r + 2, (r + 2) / 15);
      report.bestRing = "blue";
    } else if (!onNext) {
      g.strokeStyle = `rgba(0,0,0,${i(48 + 48 * alphaRatio) / 255})`;
      strokeCircle(g, x, y, r + 1, (r + 1) / 26.5);
      if (k === 0) report.bestRing = "plain";
    } else if (k === 0) report.bestRing = "plain";
  }
}

/** BoardRenderer.java:2555-2569, 2630-2712: badges and information rows. While the 下一手 详细
 *  variation shows, the candidate on that point keeps only its badge (isMouseOverNextBlunder). */
function drawCandidateLabels(g: CanvasRenderingContext2D, geo: BoardGeom, shown: ShownCandidate[], s: BoardScene, branch: boolean, hideHoverInfo: boolean, hoverStoneBlack: boolean, report: BoardReport) {
  const d = s.display, sq = geo.square, r = geo.stoneRadius;
  const candidates = s.analysis?.candidates ?? [];
  const maxAlloc = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const maxWinrate = Math.max(0, ...candidates.map((c) => c.winrate));
  let maxScore = -300;
  if (d.suggestionScore) for (const c of candidates) if (c.score_lead !== null && c.score_lead > maxScore) maxScore = c.score_lead;
  const defaultOrder = d.suggestionInfoOrder[0] === 1 && d.suggestionInfoOrder[1] === 2 && d.suggestionInfoOrder[2] === 3;
  const white = whiteBadges(s);
  const nextHover = s.preview?.order === -1;
  for (const { c, k, x, y, fraction, hovered, skipNumbers } of shown) {
    if ((!branch || hovered) && d.suggestionOrder && c.order === 0) orderBadge(g, x, y, 0, sq, white);
    if (skipNumbers || (branch && !hovered)) continue;
    if (d.suggestionOrder && c.order < 9 && c.order > 0) orderBadge(g, x, y, c.order, sq, white);
    if (nextHover) continue;
    const badge = d.suggestionOrder && c.order < 9 ? String(c.order + 1) : null;
    if (hovered && branch) {
      g.strokeStyle = "#f00";
      strokeCircle(g, x, y, r + 1, (r + 1) / 11);
      if (hideHoverInfo) continue;
    }
    let normal = d.whiteSuggestionWhite && s.toPlay === "White" ? "#fff" : "#000";
    if (branch) normal = hoverStoneBlack ? "#fff" : "#000";
    const maxColor = k === 0 || fraction > 0.375 ? "#f00" : "rgb(100,255,235)";
    const canMax = d.suggestionMaxRed && !(hovered && branch);
    // 总是显示黑胜率 / 目差显示 are applied by the core (`shown`).
    const roundedWinrate = Math.round(c.shown.winrate * 10) / 10;
    const texts = [
      roundedWinrate.toFixed(1),
      playouts(c.visits),
      c.score_lead === null ? "" : d.scoreAsDiff ? scoreDiff(c.score_lead - maxScore) : round1(c.shown.score_lead ?? c.score_lead),
    ];
    const colors = [
      canMax && c.winrate === maxWinrate ? maxColor : normal,
      canMax && c.allocation_visits === maxAlloc ? maxColor : normal,
      canMax && c.score_lead === maxScore ? maxColor : normal,
    ];
    const fields: number[] = [];
    if (d.suggestionWinrate) fields.push(0);
    if (d.suggestionPlayouts) fields.push(1);
    if (d.suggestionScore && c.score_lead !== null) fields.push(2);
    if (!defaultOrder) fields.sort((a, b) => d.suggestionInfoOrder[a] - d.suggestionInfoOrder[b]);
    drawSuggestionText(g, geo, x, y, texts, colors, fields, defaultOrder, roundedWinrate, c.visits);
    report.candidates.push({ x: c.point!.x, y: c.point!.y, texts: fields.map((f) => texts[f]), badge });
  }
}

/** 显示策略网络 (BoardRenderer.java:2362-2421): every candidate's prior, best cyan, the others red to
 *  green by their share of the largest prior. */
function drawPolicy(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const r = geo.stoneRadius;
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(0, ...candidates.map((c) => c.prior));
  if (max <= 0) return;
  for (const c of candidates) {
    if (!c.point) continue;
    const x = X(c.point.x), y = Y(c.point.y);
    const percent = c.prior / max;
    ghostShadow(g, x, y, r);
    g.fillStyle = hsb(c.prior === max ? CYAN_HUE : RED_HUE + (GREEN_HUE - RED_HUE) * percent, 1, 0.85, suggestionAlpha(percent));
    fillCircle(g, x, y, r);
    const text = c.prior.toFixed(1);
    g.fillStyle = "#fff";
    jString(g, text, x, y, WINRATE_FONT, r, r * 1.9);
    report.candidates.push({ x: c.point.x, y: c.point.y, texts: [text], badge: null });
  }
}

/** 纯网络 (BoardRenderer.java:2232-2360): heatcount = (int)(policy * 1000) on every empty point with
 *  a positive count, best cyan, others red to green; in the ownership state only the numbers,
 *  coloured by their share. */
function drawRawNetPolicy(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const policy = s.rawNet?.policy;
  if (!policy) return;
  const r = geo.stoneRadius;
  const owner = s.katago && s.rawNet?.mode === "Ownership";
  const heat = policy.map((p) => i(p * 1000));
  const max = Math.max(0, ...heat);
  for (let k = 0; k < heat.length; k++) {
    if (heat[k] <= 0 || s.stones[k]) continue;
    const x = X(k % s.cols), y = Y(i(k / s.cols));
    const percent = heat[k] / max;
    const best = heat[k] === max;
    if (!owner) {
      ghostShadow(g, x, y, r);
      g.fillStyle = hsb(best ? CYAN_HUE : RED_HUE + (GREEN_HUE - RED_HUE) * percent, 1, 0.85, suggestionAlpha(percent));
      fillCircle(g, x, y, r);
    }
    const text = (heat[k] / 10).toFixed(1);
    g.fillStyle = !owner ? "#fff" : best ? "rgb(255,0,0)" : percent >= 0.3 ? "rgb(0,120,255)" : "rgb(0,235,0)";
    if (owner && percent < 0.3) jString(g, text, x, y, WINRATE_FONT, i((r * 4) / 5), r * 1.55);
    else jString(g, text, x, y, WINRATE_FONT, r, r * 1.9);
    report.rawNetPoints++;
    report.candidates.push({ x: k % s.cols, y: i(k / s.cols), texts: [text], badge: null });
  }
}

/** The 下一手 详细 numbers centred on (x, y) in the current fill (drawNextMoveBlunder,
 *  BoardRenderer.java:3487-3618; drawNextBlunderFirstMove 3404-3485). Returns the drawn texts. */
function nextMoveTexts(g: CanvasRenderingContext2D, s: BoardScene, sq: number, x: number, y: number): string[] {
  const info = s.overlay.next_move!;
  const d = s.display;
  const wr = info.winrate.toFixed(1);
  if (info.score_lead !== null) {
    // The mover's own score: the best candidate's plus the difference to it.
    const mover = (info.score_diff ?? 0) + (s.analysis?.candidates[0]?.score_lead ?? 0);
    if (d.suggestionWinrate && d.suggestionScore) {
      const sc = d.scoreAsDiff && info.score_diff !== null ? scoreDiff(info.score_diff) : round1(info.score_lead);
      const winrateFirst = d.suggestionInfoOrder[0] <= d.suggestionInfoOrder[2];
      const [top, bottom] = winrateFirst ? [wr, sc] : [sc, wr];
      const below = winrateFirst && mover > -10 ? i((sq * 11) / 30) : i(sq * 0.34);
      jString(g, top, x, y, WINRATE_FONT, sq * 0.39, sq * 0.815, 1);
      jString(g, bottom, x, y + below, WINRATE_FONT, sq * 0.3, sq * 0.65, 1);
      return [top, bottom];
    }
    if (d.suggestionScore) {
      const sc = mover.toFixed(1);
      jString(g, sc, x, y, WINRATE_FONT, sq * 0.4, sq * 0.835);
      return [sc];
    }
  }
  if (!d.suggestionWinrate) return [];
  jString(g, wr, x, y, WINRATE_FONT, sq * 0.4, sq * 0.835);
  return [wr];
}

/** 下一手 详细 (BoardRenderer.java:3264-3321, 3487-3618): the move's result inside its ring. */
function drawNextMoveInfo(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, ox: number, oy: number, report: BoardReport) {
  const info = s.overlay.next_move!;
  const r = geo.stoneRadius;
  const x = X(info.point.x), y = Y(info.point.y);
  const fill = info.verdict === "Bad" ? "rgba(220,80,30,0.333)" : info.verdict === "Questionable" ? "rgb(220,150,30)" : "rgb(0,220,0)";
  if (info.verdict === "Bad") woodDisc(g, x, y, r + 1, 0.8, ox, oy);
  g.fillStyle = fill;
  fillCircle(g, x, y, r + 1);
  g.strokeStyle = s.nextColor === "White" ? "#fff" : "#000";
  strokeCircle(g, x, y, r + 2, Math.max(r / 7, 2));
  g.fillStyle = info.verdict === "Good" ? "#000" : "#fff";
  const texts = nextMoveTexts(g, s, geo.square, x, y);
  report.nextMove = { x: info.point.x, y: info.point.y, texts, fill };
}

/** drawNextBlunderFirstMove (BoardRenderer.java:3404-3485): while the 下一手 详细 variation shows,
 *  the move itself as a stone of the side to play with its numbers in the opposite colour. */
function drawNextFirstMove(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const info = s.overlay.next_move!;
  const x = X(info.point.x), y = Y(info.point.y);
  const black = s.toPlay === "Black";
  drawStone(g, x, y, geo.stoneRadius, black);
  g.fillStyle = black ? "#fff" : "#000";
  const texts = nextMoveTexts(g, s, geo.square, x, y);
  report.nextMove = { x: info.point.x, y: info.point.y, texts, fill: black ? "#000" : "#fff" };
}

/** BoardRenderer.drawNextMoveOutlinesOnTop + drawDashedCircle: radius r + 2, first ring
 * max(r/5.5, 3) wide, others max(r/11, 2), dash {2.2w, 1.7w}, round caps, colour of the first next
 * move. */
function drawNextMoveOutlines(g: CanvasRenderingContext2D, next: Point[], color: Stone, X: (x: number) => number, Y: (y: number) => number, r: number) {
  g.strokeStyle = color === "Black" ? "#000" : "#fff";
  g.lineCap = "round";
  next.forEach((pt, k) => {
    const w = Math.max(k === 0 ? Math.max(r / 5.5, 3) : Math.max(r / 11, 2), 1);
    g.lineWidth = w;
    g.setLineDash([w * 2.2, w * 1.7]);
    circle(g, X(pt.x), Y(pt.y), r + 2);
    g.stroke();
  });
  g.setLineDash([]);
  g.lineCap = "butt";
}

/** BoardRenderer.drawRawWinrate (BoardRenderer.java:747-790): the pure network's line under the
 *  board. Before the first answer for this position nothing stale is shown. */
function rawNetLine(s: BoardScene): string {
  const net = s.rawNet;
  if (net && net.whiteWinrate !== null) {
    if (s.katago)
      return `${t("BoardRenderer.pureNetWhiteWinrate")}${(net.whiteWinrate * 100).toFixed(1)} ${t("BoardRenderer.whiteScore")}${(net.whiteLead ?? 0).toFixed(1)} ${t("BoardRenderer.symmetry")}${net.symmetry ?? 0}`;
    // Leela Zero's `heatmap` win rate is the side to move's.
    const own = s.toPlay === "White" ? net.whiteWinrate : 1 - net.whiteWinrate;
    return `${t("BoardRenderer.pureNetWinrate")}${(own * 100).toFixed(1)}`;
  }
  return t("BoardRenderer.noPureNetWinrate");
}

/** Main board: BoardRenderer.draw (BoardRenderer.java:277-430): goban and coordinates, the line
 *  under the board, stones, ownership, 落子评价 / move numbers / last-move mark, markup, 下一手,
 *  candidates (statistics, 策略网络 or 纯网络) and the next-move rings. */
export function paintBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, s: BoardScene): BoardReport {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  const cols = s.cols;
  const d = s.display;
  const X = (x: number) => ox + geo.margin + sq * x;
  const Y = (y: number) => oy + geo.margin + sq * y;
  const px = (p: number) => X(p % cols);
  const py = (p: number) => Y(i(p / cols));
  const report: BoardReport = {
    coordinates: d.coordinates,
    bottomLine: null,
    rawNetInfo: null,
    moveNumbers: [],
    lastMoveMarker: null,
    ranks: [],
    rankCorner: false,
    candidateMode: "hidden",
    candidates: [],
    rawNetPoints: 0,
    ownership: null,
    nextMove: null,
    preview: null,
    coordinateLabels: null,
    bestRing: null,
    unimportant: 0,
    whiteOrderBadges: false,
    pvVisits: [],
  };
  drawTexture(g, images["board.png"], ox, oy, geo.size, geo.size);
  const avail = geo.size - 2 * geo.margin;
  grid(g, geo, ox, oy, Math.max(geo.size > 560 ? 2 : 1, avail / 481), Math.max(1, avail / 750));
  // star points radius (int)(0.015*size)/2
  const star = i(i(0.015 * geo.size) / 2);
  g.fillStyle = "#000";
  for (const [a, b] of hoshi(geo.cols, geo.rows)) {
    circle(g, X(a) + 0.5, Y(b) + 0.5, star + 1);
    g.fill();
  }
  // The pure network's line replaces the names (BoardRenderer.java:303-311).
  const rawLine = s.rawNetOn || (s.policy && s.engine && !s.katago);
  // Coordinates (BoardRenderer.java:1038-1100) in the 坐标格式; the bottom row gives way to the
  // names and the line.
  if (d.coordinates) {
    g.fillStyle = "#000";
    const at = i((geo.margin * 4) / 10);
    const bottom = !s.names && !rawLine;
    const labels = coordinateLabels(d.coordinateStyle, geo.cols, geo.rows);
    labels.cols.forEach((label, k) => {
      jString(g, label, X(k), oy + at, UI_FONT, i((r * 4) / 5), r);
      if (bottom) jString(g, label, X(k), oy + geo.size - at, UI_FONT, i((r * 4) / 5), r);
    });
    labels.rows.forEach((label, k) => {
      jString(g, label, ox + at, Y(k), UI_FONT, i((r * 4) / 5), r);
      jString(g, label, ox + geo.size - at, Y(k), UI_FONT, i((r * 4) / 5), r);
    });
    report.coordinateLabels = labels;
  }
  if (rawLine) {
    const text = rawNetLine(s);
    g.fillStyle = "#000";
    jString(g, text, ox + i(geo.size / 2), oy - i((geo.margin * 13) / 40) + geo.size, GRAPH_FONT, i(geo.margin / 2), geo.size, 0, true);
    report.bottomLine = "rawNet";
    report.rawNetInfo = text;
  } else if (s.names) {
    drawName(g, s, ox, oy, geo);
    report.bottomLine = "names";
  }
  const ownership = s.rawNetOn && s.katago && s.rawNet?.mode === "Ownership" ? s.rawNet.ownership : null;
  // shouldShowCountBlockBelow: the big squares go under the stones.
  const below = d.pureEstimate === "BigBelow";
  if (ownership) report.ownership = d.pureEstimate;
  if (ownership && below) drawOwnership(g, ownership, s.stones, cols, d.pureEstimate, X, Y, sq, true);
  const preview = s.preview ? playVariation(s.stones, s.toPlay, cols, s.preview, s.preview.length, d.removeDeadInVariation) : null;
  const stones = preview?.stones ?? s.stones;
  for (let p = 0; p < stones.length; p++) {
    const stone = stones[p];
    if (stone) drawStone(g, px(p), py(p), r, stone === "Black");
  }
  if (ownership && !below) drawOwnership(g, ownership, s.stones, cols, d.pureEstimate, X, Y, sq, true);
  const nextHover = s.preview?.order === -1 && s.overlay.next_move ? s.overlay.next_move.point : null;
  const hoverPoint = nextHover ?? (s.preview ? s.analysis?.candidates.find((c) => c.order === s.preview!.order)?.point ?? null : null);
  const hoverIndex = hoverPoint ? hoverPoint.y * cols + hoverPoint.x : -1;
  if (preview) drawPreviewStones(g, preview, px, py, r);
  const hideHoverInfo = drawMoveOverlay(g, s, geo, ox, oy, X, Y, preview, hoverIndex, report);
  if (preview) report.preview = { order: s.preview!.order, length: s.preview!.length, numbers: preview.numbers.size };
  drawMarks(g, s, px, py, r, ox, oy);
  const branch = !!preview;
  const showNext = d.nextMoves && !branch && !!s.nextColor;
  // 下一手 详细 only beside the statistics (BoardRenderer.java:3214-3221).
  const nextInfo = showNext && s.candidatesShown && !s.policy && !s.rawNetOn ? s.overlay.next_move : null;
  if (nextInfo) drawNextMoveInfo(g, s, geo, X, Y, ox, oy, report);
  const next = showNext ? s.nextMoves : [];
  if (s.candidatesShown && s.rawNetOn) {
    report.candidateMode = "rawNet";
    if (!branch) drawRawNetPolicy(g, s, geo, X, Y, report);
  } else if (s.candidatesShown && s.policy) {
    report.candidateMode = "policy";
    if (!branch) drawPolicy(g, s, geo, X, Y, report);
  } else if (s.candidatesShown) {
    report.candidateMode = "stats";
    report.whiteOrderBadges = whiteBadges(s);
    const covered = drawUnimportant(g, s, geo, X, Y, !branch, report);
    const { shown, ignoredCandidate } = shownCandidates(s, X, Y, nextInfo?.hides_candidate ? nextInfo.point : null, covered);
    if (!branch) drawCandidateDiscs(g, geo, shown, next, ox, oy, s, report);
    // Java strokes the hints over finished candidates; with the rows on their Java baselines the
    // dashes would cross the score row, so the hints go between the discs and the labels.
    if (next.length) drawNextMoveOutlines(g, next, s.nextColor!, X, Y, r);
    // The move played and analysed further than its candidate keeps only its badge (2464-2494).
    if (ignoredCandidate?.point && d.suggestionOrder && ignoredCandidate.order < 9) orderBadge(g, X(ignoredCandidate.point.x), Y(ignoredCandidate.point.y), ignoredCandidate.order, sq, whiteBadges(s));
    drawCandidateLabels(g, geo, shown, s, branch, hideHoverInfo, hoverIndex >= 0 && preview?.stones[hoverIndex] === "Black", report);
  } else if (next.length) drawNextMoveOutlines(g, next, s.nextColor!, X, Y, r);
  if (nextHover) drawNextFirstMove(g, s, geo, X, Y, report);
  return report;
}

/** moveOverlayOrder (BoardRenderer.java:210-231, 337-361): 落子评价 while move numbers are off,
 *  else drawMoveNumbers; a shown variation brings its own numbers. Returns hideMouseOverInfo. */
function drawMoveOverlay(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, ox: number, oy: number, X: (x: number) => number, Y: (y: number) => number, preview: Preview | null, hoverIndex: number, report: BoardReport): boolean {
  const d = s.display, r = geo.stoneRadius, sq = geo.square, cols = s.cols;
  const last = s.lastAction;
  if (preview) {
    if (preview.pass) drawPassDisc(g, ox, oy, geo, preview.toPlay === "Black");
  } else if (last && !last.point && !s.snapshot) drawPassDisc(g, ox, oy, geo, s.toPlay === "Black");
  const ring = Math.round(sq * 0.22);
  const hiddenByBranch = (p: number) => !!preview && (!preview.stones[p] || preview.numbers.has(p));
  // drawMoveRankMark (BoardRenderer.java:529-658).
  if (d.moveNumbers === 0 && d.moveRank >= 0) {
    for (const m of s.overlay.ranks) {
      if (hiddenByBranch(m.point.y * cols + m.point.x)) continue;
      g.fillStyle = RANK_COLOR[m.rank];
      fillCircle(g, X(m.point.x), Y(m.point.y), m.last ? ring : Math.round(sq * RANK_RADIUS[m.rank]));
      report.ranks.push({ x: m.point.x, y: m.point.y, rank: m.rank, last: m.last });
    }
    if (last?.point && !s.snapshot && !hiddenByBranch(last.point.y * cols + last.point.x)) {
      g.strokeStyle = last.color === "White" ? "#000" : "#fff";
      strokeCircle(g, X(last.point.x), Y(last.point.y), ring, ring / 5);
      if (s.overlay.rank_corner) {
        g.fillStyle = "#f00";
        smallTriangle(g, X(last.point.x), Y(last.point.y), r);
        report.rankCorner = true;
      }
      report.lastMoveMarker = last.point;
    }
    if (!preview) return false;
  }
  if (preview) return drawPreviewNumbers(g, preview, (p) => X(p % cols), (p) => Y(i(p / cols)), geo, hoverIndex, cols, d, report);
  if (s.snapshot) return false;
  // drawMoveNumbers (BoardRenderer.java:1898-2101); the core chose the numbers.
  if (d.moveNumbers === 0 && !(d.allNumbersInBranch && !s.mainTrunk)) {
    // stone-indicator-type 1: a ring in the opposite colour.
    if (last?.point) {
      g.strokeStyle = last.color === "White" ? "#000" : "#fff";
      strokeCircle(g, X(last.point.x), Y(last.point.y), ring, ring / 5);
      report.lastMoveMarker = last.point;
    }
    return false;
  }
  for (const label of s.overlay.move_numbers) {
    const x = X(label.point.x), y = Y(label.point.y);
    if (label.last) {
      g.fillStyle = "#f00";
      smallTriangle(g, x, y, r);
    } else g.fillStyle = s.stones[label.point.y * cols + label.point.x] === "Black" ? "#fff" : "#000";
    jString(g, String(label.number), x, y, UI_FONT, r * 1.4, i(r * (label.number >= 100 ? 1.85 : 1.4)));
    report.moveNumbers.push({ x: label.point.x, y: label.point.y, text: String(label.number), red: label.last });
  }
  return false;
}

/** BoardRenderer markup: TR / SQ / CR / MA / LB, drawn in the contrasting colour of the point. */
function drawMarks(g: CanvasRenderingContext2D, scene: BoardScene, px: (p: number) => number, py: (p: number) => number, r: number, ox: number, oy: number) {
  const cols = scene.cols;
  g.lineWidth = Math.max(1.5, r / 10);
  for (const m of scene.marks) {
    const p = m.point.y * cols + m.point.x;
    const color = scene.stones[p] === "Black" ? "#fff" : "#000";
    const [x, y] = [px(p), py(p)];
    g.strokeStyle = color;
    switch (m.mark.kind) {
      case "Triangle":
        g.beginPath();
        g.moveTo(x, y - r * 0.55);
        g.lineTo(x - r * 0.5, y + r * 0.35);
        g.lineTo(x + r * 0.5, y + r * 0.35);
        g.closePath();
        g.stroke();
        break;
      case "Square":
        g.strokeRect(x - r * 0.4, y - r * 0.4, r * 0.8, r * 0.8);
        break;
      case "Circle":
        circle(g, x, y, r * 0.45);
        g.stroke();
        break;
      case "Cross":
        g.beginPath();
        g.moveTo(x - r * 0.4, y - r * 0.4);
        g.lineTo(x + r * 0.4, y + r * 0.4);
        g.moveTo(x + r * 0.4, y - r * 0.4);
        g.lineTo(x - r * 0.4, y + r * 0.4);
        g.stroke();
        break;
      case "Label":
        if (!scene.stones[p]) {
          // label over an empty point: the grid under it is covered with the wood texture
          g.save();
          circle(g, x, y, r * 0.7);
          g.clip();
          drawTexture(g, images["board.png"], x - r, y - r, r * 2, r * 2, ox, oy);
          g.restore();
        }
        textMid(g, m.mark.text, x, y, r * 1.1, r * 1.6, color);
        break;
    }
  }
}

/** BoardRenderer.drawName (BoardRenderer.java:802-934): both name stones (the side to move drawn on
 * top), the black name in black, the white name in white with a 2 px black outline. Names longer
 * than 0.43 of the board are cut by the Java step. */
function drawName(g: CanvasRenderingContext2D, scene: BoardScene, ox: number, oy: number, geo: BoardGeom) {
  const { size, margin, stoneRadius: r, square, n } = geo;
  g.font = `${i(Math.min(28, (margin * 53) / 100))}px ${GRAPH_FONT}`;
  const fm = javaMetrics(g);
  const width = (s: string) => Math.round(g.measureText(s).width);
  const limit = square * n * 0.43;
  const cut = (s: string) => {
    while (s && width(s) > limit) s = s.slice(0, s.length - Math.max(1, i((width(s) - square * i(n / 2)) / i(width(s) / s.length))));
    return s;
  };
  const black = cut(scene.black);
  const white = cut(scene.white);
  const d = i((r * 5) / 4);
  const top = oy - margin + r + size;
  const discs: [string, number][] = [["#000", i((r * 9) / 10)], ["#fff", i((r * 3) / 10)]];
  if (scene.toPlay === "Black") discs.reverse();
  for (const [color, dx] of discs) {
    g.fillStyle = color;
    circle(g, ox + i(size / 2) - dx + d / 2, top + d / 2, d / 2);
    g.fill();
  }
  const baseY = oy - margin + size + i((r * 13) / 8) + i((fm.ascent + fm.descent + 2) / 4);
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  if (black) {
    g.fillStyle = "#000";
    g.fillText(black, ox + i(size / 2) - width(black) - i((margin * 4) / 5), baseY);
  }
  if (white) {
    const x = ox + i(size / 2) + i((margin * 4) / 5) + 1;
    g.strokeStyle = "#000";
    g.lineWidth = 2;
    g.lineJoin = "miter";
    g.strokeText(white, x, baseY);
    g.fillStyle = "#fff";
    g.fillText(white, x, baseY);
  }
}

// ------------------------------------------------------------------ sub board

/** What the sub board shows (SubBoardRenderer). */
export interface SubBoardScene {
  cols: number;
  rows: number;
  stones: (Stone | null)[];
  toPlay: Stone;
  mode: SubBoardMode;
  /** Analysis the variation comes from: the current node's, or the snapshot taken when the
   *  pointer came to rest on the sub board (no-refresh-on-sub). */
  analysis: NodeAnalysisView | null;
  /** The current node's analysis: the heat maps never freeze (SubBoardRenderer.java:1235). */
  heat: NodeAnalysisView | null;
  /** The sub board's own candidate index (bestmovesNum). */
  candidate: number;
  /** Wheel-stepped length; null shows the whole variation (SHOW_NORMAL_BOARD). */
  length: number | null;
  /** The pointer rests on the sub board with no-refresh-on-sub (isMouseOver). */
  over: boolean;
  candidatesShown: boolean;
  engine: boolean;
  branchLimit: number;
  /** 变化图中去掉死子: the variation's captured stones are taken off. */
  removeDead: boolean;
  ownership: number[] | null;
  pureEstimate: PureEstimate;
}

export interface SubBoardReport {
  mode: SubBoardMode;
  icon: "play" | "pause" | null;
  /** The drawn candidate index label (bestmovesNum + 1). */
  label: number | null;
  /** The candidate whose variation is drawn. */
  target: Point | null;
  branchLength: number | null;
  numbers: { x: number; y: number; text: string; red: boolean }[];
  heat: { x: number; y: number; size: number }[];
  frozen: boolean;
  ownership: boolean;
}

/** SubBoardRenderer.draw (SubBoardRenderer.java:194-230): flat stones, no coordinates; 变化图 shows
 *  the variation of its own candidate index with the play/pause mark, 仅棋盘棋子 the stones only,
 *  the heat maps the first-instinct priors or the searched visits of the current analysis. */
export function paintSubBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, s: SubBoardScene): SubBoardReport {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  const cols = s.cols;
  const X = (x: number) => ox + geo.margin + sq * x;
  const Y = (y: number) => oy + geo.margin + sq * y;
  const px = (p: number) => X(p % cols);
  const py = (p: number) => Y(i(p / cols));
  const report: SubBoardReport = { mode: s.mode, icon: null, label: null, target: null, branchLength: null, numbers: [], heat: [], frozen: s.over, ownership: !!s.ownership };
  g.save();
  g.beginPath();
  g.rect(ox, oy, geo.size, geo.size);
  g.clip();
  drawTexture(g, images["board.png"], ox, oy, geo.size, geo.size);
  grid(g, geo, ox, oy, 1.5, 1);
  g.fillStyle = "#000";
  for (const [a, b] of geo.cols === 19 ? hoshi(geo.cols, geo.rows) : []) {
    circle(g, X(a) + 0.5, Y(b) + 0.5, Math.max(1.5, sq * 0.12));
    g.fill();
  }
  const flatStone = (p: number, stone: Stone) => {
    g.fillStyle = stone === "Black" ? "#000" : "#fff";
    fillCircle(g, px(p), py(p), r);
    if (stone === "White") {
      g.strokeStyle = "#000";
      strokeCircle(g, px(p), py(p), r, 1);
    }
  };
  // drawPlay (SubBoardRenderer.java:292-345): paused while the pointer rests here.
  if (s.mode === "Variation") {
    const m = geo.margin;
    g.fillStyle = "#000";
    if (s.over) {
      g.fillRect(ox + geo.size - i((r * 7) / 2), oy + geo.size - i((m * 9) / 10), i((m * 3) / 10), i((m * 8) / 10));
      g.fillRect(ox + geo.size + i((m * 5) / 10) - i((r * 7) / 2), oy + geo.size - i((m * 9) / 10), i((m * 3) / 10), i((m * 8) / 10));
    } else {
      g.beginPath();
      g.moveTo(ox + geo.size - i((r * 7) / 2), oy + geo.size - 1);
      g.lineTo(ox + geo.size - i((r * 7) / 2), oy + geo.size - m + 1);
      g.lineTo(ox + geo.size - i((r * 5) / 2), oy + geo.size - i(m / 2));
      g.closePath();
      g.fill();
    }
    report.icon = s.over ? "pause" : "play";
  }
  const below = s.pureEstimate === "BigBelow";
  if (s.ownership && below) drawOwnership(g, s.ownership, s.stones, cols, s.pureEstimate, X, Y, sq, false);
  s.stones.forEach((stone, p) => {
    if (stone) flatStone(p, stone);
  });
  if (s.mode === "Policy" || s.mode === "Visits") drawSubHeat(g, s, X, Y, sq, report);
  // drawBranch (SubBoardRenderer.java:857-1020); maybeCleanBranch clears it while the side to
  // move's candidates are hidden.
  let branch: { preview: Preview; candidate: CandidateView } | null = null;
  if (s.mode === "Variation" && s.candidatesShown && s.engine) {
    const candidates = s.analysis?.candidates ?? [];
    const index = candidates.length ? Math.min(s.candidate, candidates.length - 1) : s.candidate;
    const c = candidates[index];
    if (c) {
      const full = s.branchLimit > 0 ? Math.min(s.branchLimit, c.pv.length) : c.pv.length;
      const preview = playVariation(s.stones, s.toPlay, cols, c, s.length ?? full, s.removeDead);
      branch = { preview, candidate: c };
      for (let p = 0; p < preview.stones.length; p++) {
        const captured = preview.captured.get(p);
        const stone = preview.stones[p];
        if (captured && !stone) {
          woodDisc(g, px(p), py(p), r + 1, 1, ox, oy);
          if (s.stones[p]) {
            const x = p % cols, y = i(p / cols);
            g.strokeStyle = "#000";
            g.lineWidth = 1;
            g.beginPath();
            g.moveTo(px(p) - (x === 0 ? 0 : r + 1), py(p) + 0.5);
            g.lineTo(px(p) + (x === cols - 1 ? 0 : r + 1), py(p) + 0.5);
            g.moveTo(px(p) + 0.5, py(p) - (y === 0 ? 0 : r + 1));
            g.lineTo(px(p) + 0.5, py(p) + (y === s.rows - 1 ? 0 : r + 1));
            g.stroke();
          }
          g.fillStyle = captured === "Black" ? "rgba(0,0,0,0.431)" : "rgba(255,255,255,0.471)";
          fillCircle(g, px(p), py(p), r);
        } else if (stone) flatStone(p, stone);
      }
      report.target = c.point;
      report.branchLength = s.length;
    }
    // The rank of the shown variation, also while there is none (publishUnavailableBranchNumber).
    g.fillStyle = "#000";
    g.font = `bold ${i((r * 3) / 2)}px ${GRAPH_FONT}`;
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
    g.fillText(String(index + 1), ox + geo.size - i((r * 9) / 5), oy + geo.size - i(r / 5));
    report.label = index + 1;
  }
  if (s.ownership && !below) drawOwnership(g, s.ownership, s.stones, cols, s.pureEstimate, X, Y, sq, false);
  if (branch) drawSubNumbers(g, geo, ox, oy, branch, px, py, s, report);
  g.restore();
  return report;
}

/** SubBoardRenderer.drawMoveNumbers (SubBoardRenderer.java:1118-1225). */
function drawSubNumbers(g: CanvasRenderingContext2D, geo: BoardGeom, ox: number, oy: number, branch: { preview: Preview; candidate: CandidateView }, px: (p: number) => number, py: (p: number) => number, s: SubBoardScene, report: SubBoardReport) {
  const r = geo.stoneRadius;
  const { preview, candidate } = branch;
  if (preview.pass) {
    const black = preview.toPlay === "Black";
    g.fillStyle = black ? "rgba(255,255,255,0.588)" : "rgba(0,0,0,0.588)";
    g.beginPath();
    g.ellipse(ox + i(geo.size / 2), oy + i(geo.size / 2), 4 * r, 4 * r, 0, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = black ? "#000" : "#fff";
    jString(g, t("BoardRenderer.pass"), ox + i(geo.size / 2), oy + i(geo.size / 2), UI_FONT, r * 4, r * 6);
  }
  const first = candidate.point ? candidate.point.y * s.cols + candidate.point.x : -1;
  for (const [p, n] of preview.numbers) {
    const red = s.length !== null ? p === preview.last : p === first;
    g.fillStyle = red ? "#f00" : preview.stones[p] === "Black" ? "#fff" : "#000";
    jString(g, String(n), px(p), py(p), UI_FONT, r * 1.8, r * 1.7);
    report.numbers.push({ x: p % s.cols, y: i(p / s.cols), text: String(n), red });
  }
}

/** SubBoardRenderer.drawLeelazSuggestions + drawHeat (SubBoardRenderer.java:1230-1328, 1447-1468):
 *  heatmap.png on every candidate, 2.7 squares wide at the largest prior (第一感) or the most
 *  visits (计算后); no text. */
function drawSubHeat(g: CanvasRenderingContext2D, s: SubBoardScene, X: (x: number) => number, Y: (y: number) => number, sq: number, report: SubBoardReport) {
  const candidates = s.heat?.candidates ?? [];
  const img = images["heatmap.png"];
  if (!candidates.length || !img?.naturalWidth) return;
  const maxVisits = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const maxPrior = Math.max(0, ...candidates.map((c) => c.prior));
  const seen = new Set<string>();
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  for (const c of candidates) {
    if (!c.point) continue;
    const key = `${c.point.x},${c.point.y}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const length = s.mode === "Visits"
      ? i((maxVisits > 0 ? c.allocation_visits / maxVisits : 0) * 2.7 * sq)
      : i(maxPrior > 0 ? (c.prior * 2.7 * sq) / maxPrior : 0);
    if (length <= 0) continue;
    g.drawImage(img, X(c.point.x) - i(length / 2), Y(c.point.y) - i(length / 2), length, length);
    report.heat.push({ x: c.point.x, y: c.point.y, size: length });
  }
}

/** LizzieFrame.drawStringMid (LizzieFrame.java:12392-12429), aboveOrBelow 0: the 12 px ui font is
 * scaled to span maxW, capped at maxH and at frameFontSize + 6, centred on x. */
function stringMid(g: CanvasRenderingContext2D, s: string, x: number, y: number, maxH: number, maxW: number) {
  g.font = `12px ${GRAPH_FONT}`;
  let size = Math.min(maxH, Math.round((12 * maxW) / g.measureText(s).width));
  if (Math.round(size) > 18) size = 18;
  g.font = `${size}px ${GRAPH_FONT}`;
  const fm = javaMetrics(g);
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y + i((fm.ascent - fm.descent) / 2));
}

export interface CapturedScene {
  toPlay: Stone;
  blackCaptures: number;
  whiteCaptures: number;
  moveNumber: number;
  komi: number;
  analysing: boolean;
  rules: string | null;
}

/** Java String.valueOf(double) for komi values: whole numbers keep ".0". */
export function javaDouble(v: number): string {
  return Number.isInteger(v) ? v.toFixed(1) : String(v);
}

/** LizzieFrame.drawCaptured (LizzieFrame.java:9098-9375): captures, rules caption, komi and
 * the live pondering dot. Stored analysis is intentionally independent of this indicator. */
export function paintCaptured(g: CanvasRenderingContext2D, r: Rect, s: CapturedScene) {
  const { w: width, h: height } = r;
  g.fillStyle = "rgba(0,0,0,0.51)";
  g.fillRect(0, 0, width, height);
  g.strokeStyle = "rgba(0,0,0,0.51)";
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(i(width / 2) - 1 + 0.5, 3);
  g.lineTo(i(width / 2) - 1 + 0.5, height - 1);
  g.stroke();
  const blacksTurn = s.toPlay === "Black";
  const diam0 = i(Math.min(i(width / 2), height) / 3);
  const small = i(diam0 / 2);
  const big = i((small * 3) / 2);
  const diam = i((diam0 * 3) / 2);
  const bd = i(((blacksTurn ? big : small) * 3) / 2);
  const wd = i(((blacksTurn ? small : big) * 3) / 2);
  const oval = (color: string, cx: number, d: number) => {
    g.fillStyle = color;
    circle(g, cx - i(d / 2) + d / 2, i((diam - d) / 2) + d / 2, d / 2);
    g.fill();
  };
  oval("#000", i(width / 4), bd);
  oval("#fff", i((width * 3) / 4), wd);
  const statusDiam = i(height / 4) < 10 ? i(height / 4) : 10;
  g.fillStyle = s.analysing ? "#00ff00" : "#ff0000";
  circle(g, i(width / 2) - 1 - i(statusDiam / 2) + statusDiam / 2, i((height * 7) / 26) + i((diam - statusDiam) / 2) + statusDiam / 2, statusDiam / 2);
  g.fill();
  g.fillStyle = "#fff";
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  stringMid(g, `${t("LizzieFrame.captures")}${s.blackCaptures}`, i(width / 4), i((height * 28) / 32), i(height / 6), i((width * 3) / 10));
  stringMid(g, `${t("LizzieFrame.captures")}${s.whiteCaptures}`, i((width * 3) / 4), i((height * 28) / 32), i(height / 6), i((width * 3) / 10));
  g.font = `${i(Math.max(11, height * 0.18))}px ${GRAPH_FONT}`;
  const komi = javaDouble(s.komi);
  g.fillText(komi, i(width / 2) - 1 - i(Math.round(g.measureText(komi).width) / 2), i((height * 7) / 8));
  const caption = s.rules ?? `${s.moveNumber}`;
  stringMid(g, caption, i(width / 2) - 1, i((height * 3) / 10), i(height / 5), i(width / 2));
}

/** LizzieFrame.drawMoveStatistics (LizzieFrame.java:8702-9025): black-perspective bar and
 * last-move deltas; preserve the unanalysed fallback when both payloads are absent. */
export function paintStat(g: CanvasRenderingContext2D, r: Rect, current: HistoryPoint | null, previous: HistoryPoint | null, moveNumber: number, stdev: number | null, leadWithKomi: boolean) {
  const { w, h } = r;
  g.fillStyle = "rgba(0,0,0,0.51)";
  g.fillRect(0, 0, w, h);
  g.strokeStyle = "rgba(0,0,0,0.51)";
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(1, 1.5);
  g.lineTo(w - 1, 1.5);
  g.stroke();
  const width = w - 4;
  const height = h - 4;
  let text = ` ${t("LizzieFrame.display.lastMove")}(#  ): -0.0%`;
  const data = current && current.visits > 0 ? current : previous && previous.visits > 0 ? previous : null;
  if (data) {
    const black = data.black.winrate;
    const barY = i(h * 0.32), barH = Math.max(4, i(h * 0.3));
    const split = i(width * black / 100);
    g.fillStyle = "#000";
    g.fillRect(2, barY, split, barH);
    g.fillStyle = "#fff";
    g.fillRect(2 + split, barY, width - split, barH);
    textMid(g, `${black.toFixed(1)}%`, w * 0.1, h * 0.17, Math.min(17, h * 0.24), w * 0.3, "#fff");
    textMid(g, `${(100 - black).toFixed(1)}%`, w * 0.9, h * 0.17, Math.min(17, h * 0.24), w * 0.3, "#fff");
    textMid(g, String(moveNumber), w / 2, h * 0.17, Math.min(17, h * 0.24), w * 0.25, "#fff");
    text = "";
    if (data.black.score_lead !== null) {
      // drawMoveStatistics: Black's lead, the board difference under 盘面 (core `black`).
      const label = leadWithKomi ? t("LizzieFrame.scoreLeadWithKomi") : t("LizzieFrame.scoreLeadJustScore");
      text = `${label}${data.black.score_lead.toFixed(1)}${stdev === null ? "" : ` ±${stdev.toFixed(1)}`} `;
    }
    if (current && previous && current.visits > 0 && previous.visits > 0) {
      const gain = 100 - previous.winrate - current.winrate;
      const before = previous.black.winrate;
      const beforeX = 2 + width * before / 100, nowX = 2 + split;
      if (Math.abs(nowX - beforeX) > 5) {
        g.strokeStyle = "#888";
        g.lineWidth = 1;
        g.beginPath(); g.moveTo(beforeX, barY); g.lineTo(beforeX, barY + barH); g.stroke();
        g.fillStyle = gain >= 0 ? "#00ff00" : "#ff0000";
        const direction = Math.sign(nowX - beforeX);
        g.beginPath(); g.moveTo(beforeX, barY + barH / 2 - 2); g.lineTo(nowX - direction * 5, barY + barH / 2 - 2);
        g.lineTo(nowX - direction * 5, barY + 1); g.lineTo(nowX, barY + barH / 2);
        g.lineTo(nowX - direction * 5, barY + barH - 1); g.lineTo(nowX - direction * 5, barY + barH / 2 + 2);
        g.lineTo(beforeX, barY + barH / 2 + 2); g.closePath(); g.fill();
      }
      text += `${t("LizzieFrame.display.lastMove")}(#  ): ${gain > 0 ? "+" : ""}${gain.toFixed(1)}%`;
      if (previous.score_lead !== null && current.score_lead !== null) {
        const scoreGain = -previous.score_lead - current.score_lead;
        text += ` ${scoreGain > 0 ? "+" : ""}${scoreGain.toFixed(1)}${t("LizzieFrame.pts")}`;
      }
    }
  }
  // LizzieFrame.drawString: fit width * 20 / 21 at 12 px, capped at height / 4 and frame font + 4
  g.font = `12px ${GRAPH_FONT}`;
  const fit = Math.round((12 * i((width * 20) / 21)) / g.measureText(text).width);
  g.font = `${Math.min(i(height / 4), fit, 16)}px ${GRAPH_FONT}`;
  const fm = javaMetrics(g);
  g.fillStyle = "#fff";
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(text, 2, 2 + i((height * 17) / 20) + i((fm.ascent - fm.descent) / 2));
}

export interface GraphScene {
  history: HistoryPoint[];
  current: number;
  line: Set<number>;
  mainLine: Set<number>;
  maxScore: number;
}

/** WinrateGraph.draw default single-engine branch, WinrateGraph.java:664-1060,1151-1470. */
export function paintGraph(g: CanvasRenderingContext2D, r: Rect, lineNodes: number, scene: GraphScene): number {
  const W0 = r.w;
  const H0 = r.h;
  g.fillStyle = "rgba(22,25,29,0.647)";
  g.fillRect(0, 0, W0, H0);
  const posx = 0;
  const posy = 2;
  let width = W0 - 6;
  const height = H0 - 4;
  const seg = (x1: number, y1: number, x2: number, y2: number) => {
    g.beginPath();
    g.moveTo(x1 + 0.5, y1 + 0.5);
    g.lineTo(x2 + 0.5, y2 + 0.5);
    g.stroke();
  };
  g.strokeStyle = "rgba(255,255,255,0.282)";
  g.lineWidth = 1;
  g.setLineDash([4, 4]);
  for (const k of [1, 3]) seg(posx, posy + height - i((height * ((k * 100) / 4)) / 100), posx + width, posy + height - i((height * ((k * 100) / 4)) / 100));
  const baselineY = posy + i(height / 2);
  const baseline = (bw: number) => {
    g.setLineDash([]);
    g.strokeStyle = "rgba(236,232,224,0.659)";
    g.lineWidth = 1.5;
    seg(posx, baselineY, posx + bw, baselineY);
  };
  const { slots: numMoves, width: plotWidth } = graphGeom(r.w, r.h, lineNodes);
  width = plotWidth;
  baseline(width);
  const x = (p: HistoryPoint) => i((Math.max(0, p.move_number - 1) * width) / numMoves);
  // Black's numbers from the core (`black`; the lead with the komi under 盘面).
  const wy = (p: HistoryPoint) => posy + height - i(p.black.winrate * height / 100);
  const sy = (p: HistoryPoint) => baselineY - i(p.black.score_lead! * height / (2 * scene.maxScore));
  const eligible = scene.history.filter((p) => p.visits > 0 && Number.isFinite(p.winrate));
  if (eligible.length) {
    g.fillStyle = "rgba(0,0,0,0.55)";
    g.fillRect(0, baselineY - 7, 28, 14);
    textMid(g, "50%", 14, baselineY, 10, 26, "#ddd");
  }
  const fillSegment = (x1: number, y1: number, x2: number, y2: number) => {
    const polygon = (a: number, b: number, c: number, d: number, above: boolean) => {
      g.fillStyle = above ? "rgba(100,180,255,0.204)" : "rgba(232,225,210,0.165)";
      g.beginPath(); g.moveTo(a, baselineY); g.lineTo(a, b); g.lineTo(c, d); g.lineTo(c, baselineY); g.closePath(); g.fill();
    };
    if ((y1 - baselineY) * (y2 - baselineY) < 0) {
      const cross = x1 + (x2 - x1) * (baselineY - y1) / (y2 - y1);
      polygon(x1, y1, cross, baselineY, y1 < baselineY);
      polygon(cross, baselineY, x2, y2, y2 < baselineY);
    } else polygon(x1, y1, x2, y2, y1 < baselineY || y2 < baselineY);
  };
  const drawLine = (points: HistoryPoint[], dashed: boolean) => {
    points.sort((a, b) => a.move_number - b.move_number);
    for (let k = 1; k < points.length; k++) {
      const a = points[k - 1], b = points[k], gap = b.move_number - a.move_number;
      if (gap <= 0 || gap >= 25) continue;
      if (gap === 1) fillSegment(x(a), wy(a), x(b), wy(b));
      g.setLineDash(dashed ? [4, 4] : []);
      g.strokeStyle = gap > 1 ? "#fff" : "rgb(100,180,255)";
      g.lineWidth = 1.7;
      seg(x(a), wy(a), x(b), wy(b));
    }
    const scores = points.filter((p) => p.black.score_lead !== null);
    for (let k = 1; k < scores.length; k++) {
      const a = scores[k - 1], b = scores[k];
      if (b.move_number - a.move_number >= 25) continue;
      g.setLineDash(dashed ? [4, 4] : []);
      g.strokeStyle = "#f0f";
      g.lineWidth = 1;
      seg(x(a), sy(a), x(b), sy(b));
    }
  };
  if (scene.history.some((p) => !scene.line.has(p.node))) {
    const trunk = eligible.filter((p) => scene.mainLine.has(p.node)).sort((a, b) => a.move_number - b.move_number);
    const fork = trunk.findIndex((p) => !scene.line.has(p.node));
    if (fork >= 0) drawLine(trunk.slice(Math.max(0, fork - 1)), true);
  }
  drawLine(eligible.filter((p) => scene.line.has(p.node)), false);
  g.setLineDash([]);
  const current = eligible.find((p) => p.node === scene.current);
  if (current) {
    const marker = (y: number, color: string, radius: number) => {
      g.fillStyle = "rgba(255,255,255,0.18)"; circle(g, x(current), y, radius + 3); g.fill();
      g.fillStyle = color; circle(g, x(current), y, radius); g.fill();
      g.strokeStyle = "#fff"; g.lineWidth = 1; g.stroke();
    };
    const score = current.black.score_lead;
    if (score !== null) marker(sy(current), "#ffd700", Math.abs(sy(current) - wy(current)) <= 12 ? 7 : 5);
    marker(wy(current), "rgb(255,60,60)", 5);
    textMid(g, current.black.winrate.toFixed(1), x(current), wy(current) - 12, 13, 70, "#fff");
    if (score !== null) textMid(g, `${score >= 0 ? "B" : "W"}+${Math.abs(score).toFixed(1)}`, x(current), sy(current) - 9, 12, 80, "#ff0");
  }
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.font = `11px ${GRAPH_FONT}`;
  if (width >= 150) {
    g.fillStyle = "rgb(200,200,200)";
    const step = numMoves <= 63 ? 10 : numMoves <= 125 ? 20 : numMoves < 205 ? 30 : 40;
    for (let k = 1; k <= i(numMoves / step); k++)
      if (numMoves - k * step > 3) g.fillText(`${k * step}`, posx + i(((k * step - 1) * width) / numMoves) - 3, posy + height - 1);
  }
  // drawLineLegend; the box outline keeps the grid's dashed stroke, as in Java
  if (W0 >= 240) {
    const fm = javaMetrics(g);
    const labels: [string, string][] = [
      [t("WinrateGraph.legendWinrate"), "rgb(100,180,255)"],
      [t("WinrateGraph.legendScoreLead"), "rgb(255,0,255)"],
    ];
    const widths = labels.map(([s]) => Math.round(g.measureText(s).width));
    const boxW = widths.reduce((sum, sw) => sum + 16 + 5 + sw + 12, 0) - 12 + 16;
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
    labels.forEach(([s, color], k) => {
      g.strokeStyle = color;
      g.lineWidth = 2;
      g.beginPath();
      g.moveTo(cx, lineY);
      g.lineTo(cx + 16, lineY);
      g.stroke();
      cx += 21;
      g.fillStyle = "rgb(210,210,210)";
      g.fillText(s, cx, textY);
      cx += widths[k] + 12;
    });
  }
  return eligible.length;
}

/** Pondering status lines at ponderingY (LizzieFrame.drawPonderingState). */
export function paintStatus(g: CanvasRenderingContext2D, r: Rect, lines: string[]) {
  const lh = Math.max(14, i(r.h / 3));
  g.font = `${i(lh * 0.78)}px ${FONT}`;
  g.textBaseline = "middle";
  g.textAlign = "left";
  lines.forEach((s, k) => {
    const tw = g.measureText(s).width;
    g.fillStyle = "rgba(0,0,0,0.6)";
    g.fillRect(0, k * lh, tw + 4, lh - 1);
    g.fillStyle = "#fff";
    g.fillText(s, 1, k * lh + lh / 2);
  });
}

/** LizzieFrame.createVarTreeImage + VariationTree.draw/drawTree (VariationTree.java:44-402): the
 * whole tree laid out from the root at (20, 20) with Java's lane allocation, shown through the scroll
 * pane that renderVarTree positions on the current node (LizzieFrame.java:13942-13975) unless the
 * user scrolled it (`scroll`). Nodes off the current line are dimmed; commented nodes get the blue
 * ring; 显示分支面板上的手数 (`numbers`) writes each node's core display number beside it
 * (VariationTree.java:159-176, 285-304). Returns node centres (panel coordinates) for click
 * hit-testing and the viewport. */
export function paintTree(
  g: CanvasRenderingContext2D,
  r: Rect,
  nodes: Map<number, TreeNodeView>,
  root: number,
  current: number,
  line: Set<number>,
  scroll: { top: number; left: number } | null,
  numbers: boolean,
): { hits: Map<string, number>; viewport: TreeViewport; numbers: number } {
  const hits = new Map<string, number>();
  const { w, h } = r;
  g.fillStyle = "rgba(0,0,0,0.51)";
  g.fillRect(0, 0, w, h);
  const node = (n: number) => nodes.get(n)!;
  const [YS, XS, DOT, CENTER, RING] = [20, 20, 11, 5, 15];
  const kids = (n: number) => node(n).children;
  const moveNo = (n: number) => {
    let d = 0;
    for (let p = node(n).parent; p !== null; p = node(p).parent) d++;
    return d;
  };
  const ops: ((g: CanvasRenderingContext2D) => void)[] = [];
  let maxY = h;
  let maxX = w;
  let curY = 0;
  let curX = 0;
  // LizzieFrame varTreeMaxX / varTreeMaxY: 60 px beyond the farthest node
  const grow = (x: number, y: number) => {
    maxX = Math.max(maxX, x + 60);
    maxY = Math.max(maxY, y + 60);
  };
  const segment = (x1: number, y1: number, x2: number, y2: number, color: string) =>
    ops.push((g) => {
      g.strokeStyle = color;
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(x1 + 0.5, y1 + 0.5);
      g.lineTo(x2 + 0.5, y2 + 0.5);
      g.stroke();
    });
  let drawnNumbers = 0;
  /** Font(uiFontName, PLAIN, 9) in white, left of a trunk node or above a branch's first node. */
  const label = (n: number, x: (k: number) => number, y: number) => {
    const k = node(n).number;
    if (!numbers || k === 0) return;
    drawnNumbers++;
    ops.push((g) => {
      g.font = `9px ${GRAPH_FONT}`;
      g.fillStyle = "#fff";
      g.textAlign = "left";
      g.textBaseline = "alphabetic";
      g.fillText(`${k}`, x(k), y);
    });
  };
  const dot = (n: number, x: number, y: number) => {
    const dim = !line.has(n);
    const commented = node(n).has_comment;
    if (n === current) [curX, curY] = [x, y];
    hits.set(`${x + 5},${y + 5}`, n);
    ops.push((g) => {
      const disc = (color: string, d: number) => {
        g.fillStyle = color;
        circle(g, x + 5.5, y + 5.5, d / 2);
        g.fill();
      };
      if (commented) disc("rgb(0,0,255)", RING);
      disc("#fff", DOT);
      if (dim) disc("rgba(0,0,0,0.153)", commented ? RING : DOT);
      if (n === current) disc("#000", CENTER);
    });
    grow(x, y);
  };
  const lanes: number[] = [];
  const drawTree = (posx: number, posy: number, startLane: number, start: number, variation: number) => {
    let depth = 1;
    for (let n = start; kids(n).length; n = kids(n)[0]) depth++;
    const mn = moveNo(start);
    let lane = startLane;
    while (lane < lanes.length && lanes[lane] <= mn + depth) lanes[lane++] = mn - 1;
    if (lane >= lanes.length) lanes.push(0);
    if (variation > 1) lanes[lane - 1] = mn - 1;
    lanes[lane] = mn;
    const x = posx + lane * XS;
    const color = line.has(start) ? "#fff" : "rgb(103,103,103)";
    if (lane > 0) {
      if (lane - startLane > 0 || variation > 1) {
        segment(x + 5, posy + 5, x + 5 - XS, posy + 5 - YS, color);
        segment(posx + (startLane - variation) * XS + 11, posy - YS + 5, x + 5 - XS, posy + 5 - YS, color);
      } else segment(x + 5, posy + 5, x + 10 - XS, posy + 10 - YS, color);
    }
    if (node(start).parent !== null) {
      dot(start, x, posy);
      label(start, (k) => (k < 10 ? x + 3 : k >= 100 ? x - 3 : x), posy - 5);
    } else {
      // root: square, red centre while it is the displayed node
      ops.push((g) => {
        g.fillStyle = color;
        g.fillRect(x, posy, DOT, DOT);
        g.strokeStyle = "#000";
        g.lineWidth = 1;
        g.strokeRect(x - 0.5, posy - 0.5, DOT + 1, DOT + 1);
        if (start === current) {
          g.fillStyle = "rgb(255,0,0)";
          g.fillRect(x + 2, posy + 2, DOT - 4, DOT - 4);
        }
      });
      hits.set(`${x + 5},${posy + 5}`, start);
      if (start === current) [curX, curY] = [x, posy];
      grow(x, posy);
    }
    let n = start;
    while (kids(n).length) {
      posy += YS;
      n = kids(n)[0];
      dot(n, x, posy);
      segment(x + 5, posy - 1, x + 5, posy - YS + 11, line.has(n) ? "#fff" : "rgb(103,103,103)");
      label(n, (k) => (k < 10 ? x - 7 : k >= 100 ? x - 13 : x - 10), posy - 1);
    }
    while (n !== start) {
      n = node(n).parent!;
      kids(n).slice(1).forEach((c, k) => drawTree(posx, posy, lane + k + 1, c, k + 1));
      posy -= YS;
    }
  };
  drawTree(20, 20, 0, root, 0);
  // JScrollPane with AS_NEEDED bars of DemoScrollBarUI width 9; the horizontal bar is off when the
  // pane is under 100 px high (LizzieFrame.java:13976-13983).
  const hbar = maxX > w && h >= 100;
  const viewH = h - (hbar ? SCROLLBAR : 0);
  const vbar = maxY > viewH;
  const viewW = w - (vbar ? SCROLLBAR : 0);
  const clampTop = (v: number) => Math.max(0, Math.min(i(v), maxY - viewH));
  const clampLeft = (v: number) => Math.max(0, Math.min(i(v), maxX - viewW));
  // renderVarTree: the current node centred in each direction; the root keeps the origin.
  const atRoot = node(current).parent === null;
  const top = clampTop(scroll ? scroll.top : curY <= viewH / 2 || atRoot ? 0 : curY - viewH / 2);
  const left = clampLeft(scroll ? scroll.left : curX <= viewW / 2 || atRoot ? 0 : curX - viewW / 2);
  g.save();
  g.beginPath();
  g.rect(0, 0, viewW, viewH);
  g.clip();
  g.translate(-left, -top);
  ops.forEach((op) => op(g));
  g.restore();
  const visible = new Map<string, number>();
  for (const [k, id] of hits) {
    const [x, y] = k.split(",").map(Number);
    if (y - top >= 0 && y - top < viewH && x - left >= 0 && x - left < viewW) visible.set(`${x - left},${y - top}`, id);
  }
  const vertical = vbar ? scrollbar(viewH, maxY, top) : null;
  const horizontal = hbar ? scrollbar(viewW, maxX, left) : null;
  // DemoScrollBarUI: arrow buttons, black track, thumb (230,230,250) at 0.7
  const arrows = (a: string, b: string, x1: number, y1: number, x2: number, y2: number, horiz: boolean) => {
    g.fillStyle = "rgb(238,238,238)";
    g.fillRect(x1, y1, horiz ? ARROW : SCROLLBAR, horiz ? SCROLLBAR : ARROW);
    g.fillRect(x2, y2, horiz ? ARROW : SCROLLBAR, horiz ? SCROLLBAR : ARROW);
    if (images[a]?.naturalWidth) g.drawImage(images[a], x1 + (horiz ? 1 : 0), y1 + (horiz ? 0 : 1));
    if (images[b]?.naturalWidth) g.drawImage(images[b], x2 + (horiz ? 2 : 0), y2 + (horiz ? 0 : 2));
  };
  if (vertical) {
    const sx = w - SCROLLBAR;
    arrows("smallUp.png", "smallDown.png", sx, 0, sx, viewH - ARROW, false);
    g.fillStyle = "#000";
    g.fillRect(sx, ARROW, SCROLLBAR, vertical.track);
    g.fillStyle = "rgba(230,230,250,0.7)";
    g.fillRect(sx, ARROW + vertical.thumbStart, SCROLLBAR, vertical.thumb);
  }
  if (horizontal) {
    const sy = h - SCROLLBAR;
    arrows("smallLeft.png", "smallRight.png", 0, sy, viewW - ARROW, sy, true);
    g.fillStyle = "#000";
    g.fillRect(ARROW, sy, horizontal.track, SCROLLBAR);
    g.fillStyle = "rgba(230,230,250,0.7)";
    g.fillRect(ARROW + horizontal.thumbStart, sy, horizontal.thumb, SCROLLBAR);
  }
  return { hits: visible, viewport: { top, left, maxX, maxY, viewW, viewH, vertical, horizontal }, numbers: drawnNumbers };
}

/** Scroll bar width and arrow button length of DemoScrollBarUI. */
export const SCROLLBAR = 9;
export const ARROW = 11;

/** Track length between the arrows, thumb length and thumb offset for a bar of `length`. */
export interface ScrollbarGeom {
  track: number;
  thumb: number;
  thumbStart: number;
}

function scrollbar(length: number, content: number, offset: number): ScrollbarGeom {
  const track = Math.max(0, length - 2 * ARROW);
  const thumb = Math.min(track, Math.max(8, i((track * length) / content)));
  const thumbStart = i(((track - thumb) * offset) / Math.max(1, content - length));
  return { track, thumb, thumbStart };
}

/** The visible part of the variation tree and its scroll bars, for scrolling and hit tests. */
export interface TreeViewport {
  top: number;
  left: number;
  maxX: number;
  maxY: number;
  viewW: number;
  viewH: number;
  vertical: ScrollbarGeom | null;
  horizontal: ScrollbarGeom | null;
}
