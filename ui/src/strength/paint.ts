// Java2D drawing of the dashboard's painted Swing components on a canvas: the shared
// playerStrength* paint helpers (LizzieFrame.java:18120-18314) and Graphics2D's integer geometry
// (fillRoundRect arcs are diameters, draw* outlines cover width + 1 pixels, drawString takes the
// baseline). A painted panel repaints at the device pixel ratio whenever its size changes.
import { frameMetrics } from "../appearance/state";
import { GRAPH_FONT } from "../workbench/paint";

/** java.awt.Color(r, g, b, a) as a CSS colour. */
export function rgb(r: number, g: number, b: number, a = 255): string {
  return a === 255 ? `rgb(${r},${g},${b})` : `rgba(${r},${g},${b},${a / 255})`;
}

/** Config.frameFontSize of this session. */
export function ff(): number {
  return frameMetrics().frameFontSize;
}

/** PlayerStrengthDashboardRoot colours. */
export const DASH = {
  BACKGROUND: rgb(248, 243, 233),
  PAPER: rgb(255, 252, 245),
  CARD: rgb(255, 253, 247, 235),
  TEXT: rgb(47, 35, 22),
  MUTED_TEXT: rgb(130, 119, 104),
  ACCENT: rgb(63, 119, 83),
  ACCENT_DARK: rgb(75, 51, 22),
  GOLD: rgb(183, 129, 45),
  LINE: rgb(219, 207, 188),
};

/** PlayerStrengthDetailPalette. */
export const DETAIL = {
  BACKGROUND_TOP: rgb(24, 30, 40),
  BACKGROUND_BOTTOM: rgb(13, 17, 24),
  CARD: rgb(30, 39, 53),
  CARD_SOFT: rgb(39, 49, 65),
  CARD_BORDER: rgb(86, 101, 125),
  TEXT: rgb(248, 250, 252),
  WARM_TEXT: rgb(255, 244, 218),
  MUTED_TEXT: rgb(203, 213, 225),
  BLACK_ACCENT: [255, 116, 122] as const,
  BLACK_ACCENT_SOFT: rgb(255, 151, 154),
  WHITE_ACCENT: [118, 180, 255] as const,
  WHITE_ACCENT_SOFT: rgb(153, 204, 255),
  GOLD: rgb(245, 198, 105),
  GREEN: rgb(88, 211, 153),
  TRACK: rgb(48, 58, 76),
  GRID: rgb(116, 132, 158),
};

export type G = CanvasRenderingContext2D;

const IMAGE_FILES = ["player_strength_bg.png", "player_strength_black_stone.png", "player_strength_white_stone.png"];
const images = new Map<string, HTMLImageElement>();
/** Canvases painted before every image arrived; dropped once none is still loading. */
const imageWaiters = new Set<() => void>();
const imagesSettled = () => [...images.values()].every((img) => img.complete);

for (const file of IMAGE_FILES) {
  const img = new Image();
  img.onload = img.onerror = () => {
    imageWaiters.forEach((w) => w());
    if (imagesSettled()) imageWaiters.clear();
  };
  img.src = `/assets/ui/${file}`;
  images.set(file, img);
}

/** playerStrengthImage: /assets/ui/<file>, decoded once; panels repaint when one arrives. */
function image(file: string): HTMLImageElement | null {
  const img = images.get(file);
  return img?.complete && img.naturalWidth > 0 ? img : null;
}

export function setFont(g: G, bold: boolean, size: number): void {
  g.font = `${bold ? "bold " : ""}${size}px ${GRAPH_FONT}`;
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

/** fillRoundRect: arcs are the corner diameters. */
export function fillRound(g: G, x: number, y: number, w: number, h: number, arc: number, fill: string | CanvasGradient): void {
  if (w <= 0 || h <= 0) return;
  g.beginPath();
  g.roundRect(x, y, w, h, Math.min(arc / 2, w / 2, h / 2));
  g.fillStyle = fill;
  g.fill();
}

/** drawRoundRect: the outline of a (w + 1) x (h + 1) box, 1 px unless a stroke width is given. */
export function drawRound(g: G, x: number, y: number, w: number, h: number, arc: number, color: string, stroke = 1): void {
  if (w < 0 || h < 0) return;
  g.beginPath();
  g.roundRect(x + 0.5, y + 0.5, w, h, Math.min(arc / 2, w / 2, h / 2));
  g.strokeStyle = color;
  g.lineWidth = stroke;
  g.stroke();
}

export function fillRect(g: G, x: number, y: number, w: number, h: number, color: string | CanvasGradient): void {
  g.fillStyle = color;
  g.fillRect(x, y, w, h);
}

export function line(g: G, x1: number, y1: number, x2: number, y2: number, color: string): void {
  g.beginPath();
  g.moveTo(x1 + 0.5, y1 + 0.5);
  g.lineTo(x2 + 0.5, y2 + 0.5);
  g.strokeStyle = color;
  g.lineWidth = 1;
  g.stroke();
}

export function fillOval(g: G, x: number, y: number, w: number, h: number, fill: string | CanvasGradient): void {
  g.beginPath();
  g.ellipse(x + w / 2, y + h / 2, w / 2, h / 2, 0, 0, Math.PI * 2);
  g.fillStyle = fill;
  g.fill();
}

function drawOval(g: G, x: number, y: number, w: number, h: number, color: string, stroke = 1): void {
  g.beginPath();
  g.ellipse(x + w / 2 + 0.5, y + h / 2 + 0.5, w / 2, h / 2, 0, 0, Math.PI * 2);
  g.strokeStyle = color;
  g.lineWidth = stroke;
  g.stroke();
}

export function gradient(g: G, x1: number, y1: number, c1: string, x2: number, y2: number, c2: string): CanvasGradient {
  const paint = g.createLinearGradient(x1, y1, x2, y2);
  paint.addColorStop(0, c1);
  paint.addColorStop(1, c2);
  return paint;
}

/** playerStrengthDrawText. */
export function text(g: G, s: string, x: number, y: number, color: string, bold: boolean, size: number): void {
  setFont(g, bold, size);
  g.fillStyle = color;
  g.fillText(s, x, y);
}

/** playerStrengthDrawFittedText: the font shrinks to 10, then the text ends in "...". */
export function fittedText(g: G, s: string, x: number, y: number, maxWidth: number, color: string, bold: boolean, size: number): void {
  const safeWidth = Math.max(16, maxWidth);
  let fontSize = Math.max(10, size);
  setFont(g, bold, fontSize);
  while (fontSize > 10 && width(g, s) > safeWidth) setFont(g, bold, --fontSize);
  let shown = s;
  if (width(g, shown) > safeWidth) {
    let end = s.length;
    while (end > 0 && width(g, `${s.substring(0, end)}...`) > safeWidth) end--;
    shown = end <= 0 ? "..." : `${s.substring(0, end)}...`;
  }
  g.fillStyle = color;
  g.fillText(shown, x, y);
}

/** playerStrengthDrawRoundedCard. */
export function roundedCard(g: G, x: number, y: number, w: number, h: number): void {
  fillRound(g, x + 2, y + 4, w - 4, h - 2, 22, rgb(74, 51, 26, 22));
  fillRound(g, x, y, w, h, 22, DASH.CARD);
  drawRound(g, x, y, w - 1, h - 1, 22, rgb(155, 121, 74, 80));
}

function drawImage(g: G, img: HTMLImageElement, x: number, y: number, w: number, h: number, alpha: number): void {
  g.save();
  g.globalAlpha = Math.max(0, Math.min(1, alpha));
  g.imageSmoothingQuality = "high";
  g.drawImage(img, x, y, w, h);
  g.restore();
}

/** playerStrengthDrawImageCover; false without the image. */
export function imageCover(g: G, file: string, x: number, y: number, w: number, h: number, alpha: number): boolean {
  const img = image(file);
  if (!img || w <= 0 || h <= 0) return false;
  const scale = Math.max(w / img.naturalWidth, h / img.naturalHeight);
  const dw = Math.ceil(img.naturalWidth * scale);
  const dh = Math.ceil(img.naturalHeight * scale);
  drawImage(g, img, x + Math.trunc((w - dw) / 2), y + Math.trunc((h - dh) / 2), dw, dh, alpha);
  return true;
}

/** playerStrengthDrawStone (the painted fallback and the header mark's stones). */
export function stone(g: G, black: boolean, x: number, y: number, size: number): void {
  fillOval(g, x, y, size, size, gradient(g, x, y, black ? rgb(18, 19, 21) : rgb(255, 255, 248), x + size, y + size, black ? rgb(74, 75, 75) : rgb(214, 208, 196)));
  drawOval(g, x, y, size, size, black ? rgb(255, 255, 255, 70) : rgb(70, 58, 42, 110));
  const shine = Math.max(8, Math.trunc(size / 3));
  fillOval(g, x + Math.trunc(size / 5), y + Math.trunc(size / 6), shine, shine, rgb(255, 255, 255, black ? 55 : 170));
}

/** playerStrengthDrawGeneratedStone: the stone picture contained in the square, else painted. */
export function generatedStone(g: G, black: boolean, x: number, y: number, size: number, alpha: number): void {
  const img = image(black ? "player_strength_black_stone.png" : "player_strength_white_stone.png");
  if (!img) {
    stone(g, black, x, y, size);
    return;
  }
  const scale = Math.min(size / img.naturalWidth, size / img.naturalHeight);
  const dw = Math.round(img.naturalWidth * scale);
  const dh = Math.round(img.naturalHeight * scale);
  drawImage(g, img, x + Math.trunc((size - dw) / 2), y + Math.trunc((size - dh) / 2), dw, dh, alpha);
}

/** playerStrengthDrawInfoBadge: the gold "!" disc. */
export function infoBadge(g: G, x: number, y: number, w: number, h: number, alpha: number): void {
  const d = Math.max(14, Math.min(w, h) - 2);
  const dx = x + Math.trunc((w - d) / 2);
  const dy = y + Math.trunc((h - d) / 2);
  g.save();
  g.globalAlpha = alpha;
  fillOval(g, dx, dy, d, d, gradient(g, dx, dy, rgb(253, 242, 211), dx + d, dy + d, rgb(226, 168, 75)));
  drawOval(g, dx, dy, d - 1, d - 1, rgb(137, 86, 30, 150), 1.4);
  setFont(g, true, Math.max(12, Math.trunc(d / 2) + 2));
  const m = metrics(g);
  g.fillStyle = rgb(105, 67, 29);
  g.fillText("!", dx + Math.trunc((d - width(g, "!")) / 2), dy + Math.trunc((d - m.height) / 2) + m.ascent);
  g.restore();
}

/** playerStrengthDrawBar: an 8 px track and its fill. */
export function bar(g: G, x: number, y: number, w: number, fraction: number, fill: string): void {
  fillRound(g, x, y, w, 8, 8, rgb(223, 218, 205));
  fillRound(g, x, y, Math.round(w * Math.max(0, Math.min(1, fraction))), 8, 8, fill);
}

/** Color.brighter(). */
export function brighter([r, g, b]: readonly number[]): number[] {
  const i = 3;
  if (r === 0 && g === 0 && b === 0) return [i, i, i];
  const up = (c: number) => Math.min(Math.trunc((c > 0 && c < i ? i : c) / 0.7), 255);
  return [up(r), up(g), up(b)];
}

/** "rgb(r,g,b)" to its components. */
export function components(color: string): number[] {
  return (color.match(/\d+/g) ?? ["0", "0", "0"]).slice(0, 3).map(Number);
}

/** A painted JComponent: a canvas filling its box, repainted on resize and on request. */
export interface Painted {
  readonly element: HTMLCanvasElement;
  readonly width: number;
  readonly height: number;
  repaint(): void;
}

export function painted(cls: string, paint: (g: G, w: number, h: number) => void): Painted {
  const canvas = document.createElement("canvas");
  canvas.className = `ps-canvas ${cls}`;
  let w = 0;
  let h = 0;
  const repaint = () => {
    if (!canvas.isConnected) return;
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
    repaint();
  }).observe(canvas);
  if (!imagesSettled()) imageWaiters.add(repaint);
  return {
    element: canvas,
    get width() { return w; },
    get height() { return h; },
    repaint,
  };
}
