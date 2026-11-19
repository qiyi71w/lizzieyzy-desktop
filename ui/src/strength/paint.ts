// Canvas drawing of the 棋力评估 charts (选点命中图, the 详细数据 match chart): the design tokens as
// colours, the interface font at the session's sizes, and a painted panel that repaints at the
// device pixel ratio whenever its size or the colour mode changes.
import { frameMetrics } from "../appearance/state";
import { onThemeChange, token } from "../design/theme";
import "../design/stage.css";

/** Config.frameFontSize of this session: --ds-size. */
export function ff(): number {
  return frameMetrics().frameFontSize;
}

/** The chart colours, read from the tokens (design/tokens.css, design/stage.css and the --ps-*
 *  block of strength.css) once per colour mode. */
export interface Palette {
  text: string;
  text2: string;
  text3: string;
  line: string;
  lineStrong: string;
  well: string;
  raised: string;
  grid: string;
  baseline: string;
  stoneBlack: string;
  stoneBlackEdge: string;
  stoneWhite: string;
  stoneWhiteEdge: string;
  rankWindow: string;
  ai: string;
}

let cached: Palette | null = null;

export function palette(): Palette {
  cached ??= {
    text: token("--ds-text"),
    text2: token("--ds-text-2"),
    text3: token("--ds-text-3"),
    line: token("--ds-line"),
    lineStrong: token("--ds-line-strong"),
    well: token("--ds-well"),
    raised: token("--ds-raised"),
    grid: token("--ws-graph-grid"),
    baseline: token("--ws-graph-baseline"),
    stoneBlack: token("--ws-stone-black"),
    stoneBlackEdge: token("--ws-stone-black-edge"),
    stoneWhite: token("--ws-stone-white"),
    stoneWhiteEdge: token("--ws-stone-white-edge"),
    rankWindow: token("--ps-rank-window"),
    ai: token("--ps-ai"),
  };
  return cached;
}

export type G = CanvasRenderingContext2D;

let font = "";

/** The interface font: 600 for figures and titles, 400 otherwise. */
export function setFont(g: G, bold: boolean, size: number): void {
  font ||= token("--ds-font") || "sans-serif";
  g.font = `${bold ? 600 : 400} ${size}px ${font}`;
}

/** FontMetrics ascent / height of the current font. */
export function metrics(g: G): { ascent: number; height: number } {
  const m = g.measureText("Hg");
  const ascent = Math.ceil(m.fontBoundingBoxAscent);
  return { ascent, height: ascent + Math.ceil(m.fontBoundingBoxDescent) };
}

export function width(g: G, text: string): number {
  return Math.round(g.measureText(text).width);
}

/** A rounded box `r` px at the corners. */
export function fillRound(g: G, x: number, y: number, w: number, h: number, r: number, fill: string): void {
  if (w <= 0 || h <= 0) return;
  g.beginPath();
  g.roundRect(x, y, w, h, Math.min(r, w / 2, h / 2));
  g.fillStyle = fill;
  g.fill();
}

/** The outline of a rounded box, on the pixel grid. */
export function strokeRound(g: G, x: number, y: number, w: number, h: number, r: number, color: string, stroke = 1): void {
  if (w < 0 || h < 0) return;
  g.beginPath();
  g.roundRect(x + 0.5, y + 0.5, w - 1, h - 1, Math.min(r, w / 2, h / 2));
  g.strokeStyle = color;
  g.lineWidth = stroke;
  g.stroke();
}

/** A hairline from (x1, y1) to (x2, y2); `dash` draws it dashed. */
export function line(g: G, x1: number, y1: number, x2: number, y2: number, color: string, dash = false): void {
  g.save();
  if (dash) g.setLineDash([3, 3]);
  g.beginPath();
  g.moveTo(Math.round(x1) + 0.5, Math.round(y1) + 0.5);
  g.lineTo(Math.round(x2) + 0.5, Math.round(y2) + 0.5);
  g.strokeStyle = color;
  g.lineWidth = 1;
  g.stroke();
  g.restore();
}

export function text(g: G, s: string, x: number, y: number, color: string, bold: boolean, size: number): void {
  setFont(g, bold, size);
  g.fillStyle = color;
  g.fillText(s, x, y);
}

/** A flat stone of the stone pair (黑白棋子对): black solid, white light with its edge. */
export function stone(g: G, black: boolean, x: number, y: number, size: number): void {
  const p = palette();
  const r = size / 2;
  g.beginPath();
  g.arc(x + r, y + r, r - 0.5, 0, Math.PI * 2);
  g.fillStyle = black ? p.stoneBlack : p.stoneWhite;
  g.fill();
  g.strokeStyle = black ? p.stoneBlackEdge : p.stoneWhiteEdge;
  g.lineWidth = 1;
  g.stroke();
}

/** A painted JComponent: a canvas filling its box, repainted on resize and on request. */
export interface Painted {
  readonly element: HTMLCanvasElement;
  readonly width: number;
  readonly height: number;
  repaint(): void;
}

/** The painted panels on screen, repainted with the new colours when the colour mode changes. */
const shown = new Set<() => void>();
onThemeChange(() => {
  cached = null;
  for (const repaint of shown) repaint();
});

export function painted(cls: string, paint: (g: G, w: number, h: number) => void): Painted {
  const canvas = document.createElement("canvas");
  canvas.className = `ps-canvas ${cls}`;
  let w = 0;
  let h = 0;
  const repaint = () => {
    if (!canvas.isConnected) {
      shown.delete(repaint);
      return;
    }
    const dpr = window.devicePixelRatio || 1;
    const pw = Math.round(w * dpr);
    const ph = Math.round(h * dpr);
    if (canvas.width !== pw) canvas.width = pw;
    if (canvas.height !== ph) canvas.height = ph;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    g.textBaseline = "alphabetic";
    if (w > 0 && h > 0) paint(g, w, h);
  };
  new ResizeObserver(() => {
    w = Math.round(canvas.clientWidth);
    h = Math.round(canvas.clientHeight);
    if (canvas.isConnected) shown.add(repaint);
    repaint();
  }).observe(canvas);
  return {
    element: canvas,
    get width() { return w; },
    get height() { return h; },
    repaint,
  };
}
