// Canvas painters reproducing the fixed Java drawing (wimi321/lizzieyzy-next cb53acc).
// Analysis presentation consumes node-persisted projections, independent of engine pondering.
import type { CandidateView, HistoryPoint, LastAction, Mark, NodeAnalysisView, Point, Stone, TreeNodeView } from "../contract/generated";
import { t } from "../i18n";
import type { BoardGeom, Rect } from "./layout";
import { graphGeom } from "./layout";

const COLS = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
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

/** What the boards show for the current node. */
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
  /** Names are drawn on the board once a record was loaded (Java setPlayers). */
  showNames: boolean;
  analysis: NodeAnalysisView | null;
  hovered: number | null;
  suggestionLimit: number;
  branchLimit: number;
  subCandidate: number;
  subLength: number | null;
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

/** BoardRenderer.drawShadow + theme stone image (shadow-size 85). */
function drawStone(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, black: boolean) {
  g.save();
  g.globalAlpha = 0.85;
  const sh = g.createRadialGradient(cx + r * 0.12, cy + r * 0.12, r * 0.6, cx + r * 0.12, cy + r * 0.12, r * 1.18);
  sh.addColorStop(0, "rgba(0,0,0,0.55)");
  sh.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = sh;
  circle(g, cx + r * 0.12, cy + r * 0.12, r * 1.18);
  g.fill();
  g.globalAlpha = 1;
  const img = images[black ? "black0.png" : "white0.png"];
  if (img?.naturalWidth) g.drawImage(img, cx - r, cy - r, r * 2 + 1, r * 2 + 1);
  else {
    g.fillStyle = black ? "#111" : "#f4f4f4";
    circle(g, cx, cy, r);
    g.fill();
  }
  g.restore();
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

interface Preview {
  stones: (Stone | null)[];
  numbers: Map<number, number>;
  captured: Map<number, Stone>;
  last: number | null;
}

/** VariationPreviewGenerator over the core's `Branch` result: each move's stone and the stones
 *  it captures come from the view; nothing is decided here and the record is never touched. */
function variation(scene: BoardScene, candidate: CandidateView, length: number): Preview {
  const stones = scene.stones.slice();
  const numbers = new Map<number, number>();
  const captured = new Map<number, Stone>();
  let color = scene.toPlay;
  let last: number | null = null;
  for (let k = 0; k < Math.min(length, candidate.pv.length); k++) {
    const pt = candidate.pv[k];
    last = null;
    if (pt) {
      const p = pt.y * scene.cols + pt.x;
      stones[p] = color;
      captured.delete(p);
      numbers.set(p, k + 1);
      for (const q of candidate.pv_captures[k] ?? []) {
        const index = q.y * scene.cols + q.x;
        const stone = stones[index];
        if (stone) captured.set(index, stone);
        stones[index] = null;
        numbers.delete(index);
      }
      last = p;
    }
    color = color === "Black" ? "White" : "Black";
  }
  return { stones, numbers, captured, last };
}

function drawPreview(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, r: number, suppressFirst: boolean) {
  for (const [p, stone] of preview.captured) {
    if (preview.stones[p]) continue;
    g.fillStyle = stone === "Black" ? "rgba(0,0,0,0.235)" : "rgba(255,255,255,0.333)";
    circle(g, px(p), py(p), r);
    g.fill();
  }
  for (const [p, n] of preview.numbers) {
    if (suppressFirst && n === 1) continue;
    textMid(g, String(n), px(p), py(p), r * 1.45, r * 1.9, preview.stones[p] === "Black" ? "#fff" : "#000");
  }
  if (preview.last !== null && preview.numbers.has(preview.last)) {
    const x = px(preview.last), y = py(preview.last);
    g.fillStyle = "#f00";
    g.beginPath();
    g.moveTo(x - r, y - r);
    g.lineTo(x - r * 0.25, y - r);
    g.lineTo(x - r, y - r * 0.25);
    g.closePath();
    g.fill();
  }
}

interface ShownCandidate {
  c: CandidateView;
  k: number;
  x: number;
  y: number;
  ratio: number;
  needed: boolean;
  showStats: boolean;
}

/** Candidates BoardRenderer.drawLeelazSuggestions (BoardRenderer.java:2437-2632) shows, worst first. */
function shownCandidates(scene: BoardScene, px: (p: number) => number, py: (p: number) => number, showingBranch: boolean): ShownCandidate[] {
  const candidates = scene.analysis?.candidates ?? [];
  const max = Math.max(1, ...candidates.map((c) => c.allocation_visits));
  const shown: ShownCandidate[] = [];
  for (let k = candidates.length - 1; k >= 0; k--) {
    const c = candidates[k], pt = c.point;
    const hovered = scene.hovered === c.order;
    const outOfOrder = scene.suggestionLimit > 0 && c.order + 1 > scene.suggestionLimit;
    if (!pt || c.visits <= 0 || pt.x >= scene.cols || pt.y >= scene.rows || pt.x < 0 || pt.y < 0) continue;
    if (showingBranch && !hovered) continue;
    // show-nosugg-circle=false: Java's unimportant background is empty.
    if (outOfOrder && !hovered && k !== 0) continue;
    const p = pt.y * scene.cols + pt.x;
    const ratio = Math.min(1, c.allocation_visits / max);
    const needed = scene.nextMainMove?.x === pt.x && scene.nextMainMove.y === pt.y;
    shown.push({ c, k, x: px(p), y: py(p), ratio, needed, showStats: ratio > 0 || hovered || k === 0 || needed });
  }
  return shown;
}

/** BoardRenderer.java:2587-2662: default HSB colours and allocation alpha; a candidate on a next
 * move keeps only the dimmed blue best ring (isOnNext), the hovered branch gets the red ring. */
function drawCandidateDiscs(g: CanvasRenderingContext2D, r: number, shown: ShownCandidate[], showingBranch: boolean, next: Point[], ox: number, oy: number) {
  for (const { c, k, x, y, ratio, needed, showStats } of shown) {
    if (showingBranch) {
      g.strokeStyle = "#f00";
      g.lineWidth = (r + 1) / 11;
      circle(g, x, y, r + 1);
      g.stroke();
      continue;
    }
    // Java repaints the wood beneath the colour, so the grid does not show through suggestions.
    // Very low allocation retains 20% of the original surface (SRC_ATOP on an opaque board).
    const onNext = next.some((n) => n.x === c.point!.x && n.y === c.point!.y);
    if (showStats) {
      g.save();
      // Java fillOval uses a 2r+1 diameter, unlike its outline.
      circle(g, x + 0.5, y + 0.5, r + 1.5);
      g.clip();
      g.globalAlpha = ratio < 0.05 && !needed ? 0.8 : 1;
      drawTexture(g, images["board.png"], x - r - 1, y - r - 1, 2 * (r + 1) + 1, 2 * (r + 1) + 1, ox, oy);
      g.restore();
    }
    const alphaRatio = Math.max(0, Math.log(ratio) / 5 + 1);
    const hue = k === 0 ? 180 : 120 * Math.sqrt(ratio);
    // HSB brightness .85 corresponds to HSL lightness .425 at full saturation.
    g.fillStyle = `hsla(${hue},100%,42.5%,${Math.trunc(32 + 208 * alphaRatio) / 255})`;
    circle(g, x + 0.5, y + 0.5, r + 1.5);
    g.fill();
    if (k === 0) {
      g.strokeStyle = onNext ? "rgba(0,0,255,0.667)" : "#00f";
      g.lineWidth = (r + 2) / 15;
      circle(g, x, y, r + 2);
      g.stroke();
    } else if (!onNext) {
      g.strokeStyle = `rgba(0,0,0,${(48 + 48 * alphaRatio) / 255})`;
      g.lineWidth = (r + 1) / 26.5;
      circle(g, x, y, r + 1);
      g.stroke();
    }
  }
}

/** BoardRenderer.drawStringFor3row: Java's width-limited font and alphabetic baseline.
 * The two-row visits line uses drawString(aboveOrBelow=0), centred by FontMetrics. */
function suggestionText(g: CanvasRenderingContext2D, s: string, x: number, y: number, maxH: number, maxW: number, color: string, centred = false, family = `"Lizzie Winrate", ${FONT}`) {
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.font = `100px ${family}`;
  const size = Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(s).width))));
  g.font = `${size}px ${family}`;
  if (centred) {
    const fm = javaMetrics(g);
    y += i((fm.ascent - fm.descent) / 2);
  }
  g.fillStyle = color;
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y);
}

/** BoardRenderer.java:2648-2913: order badges and the default-order info rows (three with a score
 * lead, otherwise winrate over playouts). */
function drawCandidateLabels(g: CanvasRenderingContext2D, geo: BoardGeom, shown: ShownCandidate[], scene: BoardScene, showingBranch: boolean) {
  const r = geo.stoneRadius, sq = geo.square;
  for (const { c, x, y, showStats } of shown) {
    if (c.order < 9 && showStats) {
      const bx = Math.round(x + sq * 0.43) + (c.order === 0 ? 1 : 0);
      const by = Math.round(y - sq * 0.358) - (c.order === 0 ? 1 : 0);
      const label = String(c.order + 1);
      const family = `"Lizzie Winrate", ${FONT}`;
      g.font = `100px ${family}`;
      const size = Math.min(sq * 0.36, Math.round(100 * sq * 0.39 / Math.max(1, Math.round(g.measureText(label).width))));
      g.font = `${size}px ${family}`;
      g.textBaseline = "alphabetic";
      const fm = javaMetrics(g);
      const height = fm.ascent - fm.descent;
      const width = Math.round(g.measureText(label).width);
      const left = bx - i(width / 2);
      g.fillStyle = "#ffc800"; // java.awt.Color.ORANGE
      g.fillRect(left, by - height, width, height + Math.max(1, i(height / 12)));
      g.fillStyle = "#000";
      g.textAlign = "left";
      g.fillText(label, left, by);
    }
    if (!showStats) continue;
    const color = showingBranch && scene.toPlay === "Black" ? "#fff" : "#000";
    const narrow = Math.round(c.winrate * 10) / 10 < 10;
    if (c.score_lead !== null) {
      suggestionText(g, c.winrate.toFixed(1), x, y - Math.round(sq * (narrow ? 0.127 : 0.125)), sq * (narrow ? 0.36 : 0.35), sq * 0.67, color);
      suggestionText(g, playouts(c.visits), x, y + Math.round(sq * 0.18), sq * 0.34, r * (c.visits < 1000 ? 1.3 : 1.8), color, false, GRAPH_FONT);
      suggestionText(g, c.score_lead.toFixed(1), x, y + Math.round(sq * 0.435), sq * 0.273, r * 1.6, color);
    } else {
      suggestionText(g, c.winrate.toFixed(1), x, y - i(sq / (narrow ? 15 : 16)), r, sq * (narrow ? 0.57 : 0.735), color);
      suggestionText(g, playouts(c.visits), x, y + i((r * 15) / 35), r * 0.77, r * 1.8, color, true, GRAPH_FONT);
    }
  }
}

/** BoardRenderer.drawNextMoveOutlinesOnTop + drawDashedCircle: radius r + 2, first ring
 * max(r/5.5, 3) wide, others max(r/11, 2), dash {2.2w, 1.7w}, round caps, colour of the first next
 * move. */
function drawNextMoveOutlines(g: CanvasRenderingContext2D, next: Point[], color: Stone, px: (p: number) => number, py: (p: number) => number, r: number, cols: number) {
  g.strokeStyle = color === "Black" ? "#000" : "#fff";
  g.lineCap = "round";
  next.forEach((pt, k) => {
    const w = Math.max(k === 0 ? Math.max(r / 5.5, 3) : Math.max(r / 11, 2), 1);
    g.lineWidth = w;
    g.setLineDash([w * 2.2, w * 1.7]);
    const p = pt.y * cols + pt.x;
    circle(g, px(p), py(p), r + 2);
    g.stroke();
  });
  g.setLineDash([]);
  g.lineCap = "butt";
}

/** Main board: BoardRenderer.drawWoodenBoard / drawGrid / drawStones / drawLastMoveMark /
 *  drawPass / drawMarkup / drawNextMoveOutlinesOnTop / drawName. */
export function paintBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, scene: BoardScene): number {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  const cols = scene.cols;
  const px = (p: number) => ox + geo.margin + sq * (p % cols);
  const py = (p: number) => oy + geo.margin + sq * i(p / cols);
  drawTexture(g, images["board.png"], ox, oy, geo.size, geo.size);
  const avail = geo.size - 2 * geo.margin;
  grid(g, geo, ox, oy, Math.max(geo.size > 560 ? 2 : 1, avail / 481), Math.max(1, avail / 750));
  // star points radius (int)(0.015*size)/2
  const star = i(i(0.015 * geo.size) / 2);
  g.fillStyle = "#000";
  for (const [a, b] of hoshi(geo.cols, geo.rows)) {
    circle(g, ox + geo.margin + sq * a + 0.5, oy + geo.margin + sq * b + 0.5, star + 1);
    g.fill();
  }
  // coordinates (BoardRenderer.java:1044-1080); the bottom row gives way to the player names
  const cs = i((r * 4) / 5) + 2;
  const names = scene.showNames && (scene.black || scene.white);
  for (let k = 0; k < geo.cols; k++) {
    const a = geo.margin + sq * k;
    const label = COLS[k] ?? `${k + 1}`;
    textMid(g, label, ox + a, oy + geo.margin * 0.4, cs, sq, "#111");
    if (!names) textMid(g, label, ox + a, oy + geo.size - geo.margin * 0.4, cs, sq, "#111");
  }
  for (let k = 0; k < geo.rows; k++) {
    const a = geo.margin + sq * k;
    textMid(g, `${geo.rows - k}`, ox + geo.margin * 0.4, oy + a, cs, geo.margin * 0.8, "#111");
    textMid(g, `${geo.rows - k}`, ox + geo.size - geo.margin * 0.4, oy + a, cs, geo.margin * 0.8, "#111");
  }
  const candidate = scene.analysis?.candidates.find((c) => c.order === scene.hovered);
  const preview = candidate ? variation(scene, candidate, scene.branchLimit > 0 ? scene.branchLimit : candidate.pv.length) : null;
  const stones = preview?.stones ?? scene.stones;
  for (let p = 0; p < stones.length; p++) {
    const s = stones[p];
    if (s) drawStone(g, px(p), py(p), r, s === "Black");
  }
  const last = scene.lastAction;
  if (last?.point) {
    // drawLastMoveMark: ring in the opposite colour
    const p = last.point.y * cols + last.point.x;
    g.strokeStyle = last.color === "Black" ? "#fff" : "#000";
    g.lineWidth = Math.max(1.5, r / 9);
    circle(g, px(p), py(p), r * 0.45);
    g.stroke();
  } else if (last) {
    // BoardRenderer.drawPass (BoardRenderer.java:504-523): translucent disc at the centre
    const black = scene.toPlay === "Black";
    g.fillStyle = black ? "rgba(255,255,255,0.31)" : "rgba(0,0,0,0.31)";
    circle(g, ox + geo.size / 2, oy + geo.size / 2, (r * 5) / 2);
    g.fill();
    textMid(g, t("BoardRenderer.pass"), ox + geo.size / 2, oy + geo.size / 2, r * 3, (r * 9) / 2, black ? "rgba(0,0,0,0.78)" : "rgba(255,255,255,0.78)");
  }
  drawMarks(g, scene, px, py, r, ox, oy);
  if (preview) drawPreview(g, preview, px, py, r, true);
  const showingBranch = !!preview;
  const shown = shownCandidates(scene, px, py, showingBranch);
  // Java hides the next-move hints while a branch is shown (BoardRenderer.java:415-416).
  const next = scene.nextColor && !showingBranch ? scene.nextMoves : [];
  drawCandidateDiscs(g, r, shown, showingBranch, next, ox, oy);
  // Java strokes the hints over finished candidates; with the rows on their Java baselines the
  // dashes would cross the score row, so the hints go between the discs and the labels.
  if (scene.nextColor && next.length) drawNextMoveOutlines(g, next, scene.nextColor, px, py, r, cols);
  drawCandidateLabels(g, geo, shown, scene, showingBranch);
  if (names) drawName(g, scene, ox, oy, geo);
  return shown.length;
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

/** SubBoardRenderer.java:857-988: current position and independently selected PV, flat stones,
 * white stones outlined, no coordinates. Empty analysis leaves the position unchanged. */
export function paintSubBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, scene: BoardScene) {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  drawTexture(g, images["board.png"], ox, oy, geo.size, geo.size);
  grid(g, geo, ox, oy, 1.5, 1);
  g.fillStyle = "#000";
  for (const [a, b] of geo.cols === 19 ? hoshi(geo.cols, geo.rows) : []) {
    circle(g, ox + geo.margin + sq * a + 0.5, oy + geo.margin + sq * b + 0.5, Math.max(1.5, sq * 0.12));
    g.fill();
  }
  const cols = scene.cols;
  const px = (p: number) => ox + geo.margin + sq * (p % cols);
  const py = (p: number) => oy + geo.margin + sq * i(p / cols);
  g.lineWidth = 1;
  g.strokeStyle = "#000";
  const candidate = scene.analysis?.candidates[scene.subCandidate];
  const limit = scene.branchLimit > 0 ? scene.branchLimit : (candidate?.pv.length ?? 0);
  const preview = candidate ? variation(scene, candidate, scene.subLength ?? limit) : null;
  (preview?.stones ?? scene.stones).forEach((s, p) => {
    if (!s) return;
    g.fillStyle = s === "Black" ? "#000" : "#fff";
    circle(g, px(p) + 0.5, py(p) + 0.5, r + 0.5);
    g.fill();
    if (s === "White") {
      circle(g, px(p) + 0.5, py(p) + 0.5, r);
      g.stroke();
    }
  });
  if (preview) drawPreview(g, preview, px, py, r, false);
  // SubBoardRenderer.java:984: rank of the independently selected variation.
  g.font = `bold ${i((r * 3) / 2)}px ${FONT}`;
  g.fillStyle = "#000";
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(`▶ ${scene.subCandidate + 1}`, ox + geo.size - (r * 9) / 5 - i(r * 1.6), oy + geo.size - r / 5);
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
export function paintStat(g: CanvasRenderingContext2D, r: Rect, current: HistoryPoint | null, previous: HistoryPoint | null, moveNumber: number, stdev: number | null) {
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
    const black = data.to_play === "Black" ? data.winrate : 100 - data.winrate;
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
    if (data.score_lead !== null) {
      const score = data.to_play === "Black" ? data.score_lead : -data.score_lead;
      text = `${t("LizzieFrame.scoreLeadJustScore")}${score.toFixed(1)}${stdev === null ? "" : ` ±${stdev.toFixed(1)}`} `;
    }
    if (current && previous && current.visits > 0 && previous.visits > 0) {
      const gain = 100 - previous.winrate - current.winrate;
      const before = previous.to_play === "Black" ? previous.winrate : 100 - previous.winrate;
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
  const wy = (p: HistoryPoint) => posy + height - i((p.to_play === "Black" ? p.winrate : 100 - p.winrate) * height / 100);
  const sy = (p: HistoryPoint) => baselineY - i((p.to_play === "Black" ? p.score_lead! : -p.score_lead!) * height / (2 * scene.maxScore));
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
    const scores = points.filter((p) => p.score_lead !== null);
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
    if (current.score_lead !== null) marker(sy(current), "#ffd700", Math.abs(sy(current) - wy(current)) <= 12 ? 7 : 5);
    marker(wy(current), "rgb(255,60,60)", 5);
    textMid(g, (current.to_play === "Black" ? current.winrate : 100 - current.winrate).toFixed(1), x(current), wy(current) - 12, 13, 70, "#fff");
    if (current.score_lead !== null) {
      const score = current.to_play === "Black" ? current.score_lead : -current.score_lead;
      textMid(g, `${score >= 0 ? "B" : "W"}+${Math.abs(score).toFixed(1)}`, x(current), sy(current) - 9, 12, 80, "#ff0");
    }
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
 * ring. Returns node centres (panel coordinates) for click hit-testing and the viewport. */
export function paintTree(
  g: CanvasRenderingContext2D,
  r: Rect,
  nodes: Map<number, TreeNodeView>,
  root: number,
  current: number,
  line: Set<number>,
  scroll: { top: number; left: number } | null,
): { hits: Map<string, number>; viewport: TreeViewport } {
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
  // numbering along a variation: 1 at its first node, +1 along first children
  const branchNo = (n: number) => {
    let k = 1;
    for (let m = n, p = node(m).parent; p !== null && kids(p)[0] === m; m = p, p = node(m).parent) k++;
    return k;
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
  const label = (s: string, x: number, y: number) =>
    ops.push((g) => {
      g.font = `9px ${GRAPH_FONT}`;
      g.fillStyle = "#fff";
      g.textAlign = "left";
      g.textBaseline = "alphabetic";
      g.fillText(s, x, y);
    });
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
      const k = branchNo(start);
      label(`${k}`, k < 10 ? x + 3 : k >= 100 ? x - 3 : x, posy - 5);
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
      const k = lane === 0 ? moveNo(n) : branchNo(n);
      label(`${k}`, k < 10 ? x - 7 : k >= 100 ? x - 13 : x - 10, posy - 1);
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
  return { hits: visible, viewport: { top, left, maxX, maxY, viewW, viewH, vertical, horizontal } };
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
