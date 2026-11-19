// 自由涂鸦 (DrawPainting.java, opened by LizzieFrame.drawPainting): a nearly transparent layer over
// the workbench between the toolbars. Dragging with the left button draws 3 px lines in the chosen
// pen; the buttons at its top right choose 蓝 / 绿 / 红 (the chosen one lit), take back the last line,
// clear every line and close it, as ESC does. The lines live in this layer only: the record, the
// unsaved mark and every other display stay as they are. Closing saves the pen it had
// (saveLastColor, `last-painting-color`), which the next layer opens with.
import { run, state } from "../app";
import { COMMANDS } from "../commands";
import type { PaintingColor } from "../contract/generated";
import { icon, type IconName } from "../design/icons";
import "./painting.css";

/** Color.BLUE / GREEN / RED (DrawPainting.setColor). */
const PEN: Record<PaintingColor, string> = { Blue: "rgb(0,0,255)", Green: "rgb(0,255,0)", Red: "rgb(255,0,0)" };
const PENS: [PaintingColor, IconName][] = [["Blue", "pen-blue"], ["Green", "pen-green"], ["Red", "pen-red"]];

interface Stroke {
  color: PaintingColor;
  points: [number, number][];
}

interface Layer {
  root: HTMLElement;
  canvas: HTMLCanvasElement;
  color: PaintingColor;
  /** drawlist: the finished lines, oldest first. */
  strokes: Stroke[];
  /** list: the line being dragged. */
  current: Stroke | null;
  refresh: () => void;
}

let layer: Layer | null = null;

export function paintingOpen(): boolean {
  return layer !== null;
}

/** Keys while the layer is open: it holds the focus as Java's dialog does, so only ESC (closing it)
 *  does anything. True when the key belongs to the layer. */
export function paintingKey(e: KeyboardEvent): boolean {
  if (!layer) return false;
  if (e.key === "Escape") closePainting();
  return true;
}

function segment(g: CanvasRenderingContext2D, s: Stroke, i: number): void {
  const [x0, y0] = s.points[i - 1];
  const [x1, y1] = s.points[i];
  g.strokeStyle = PEN[s.color];
  g.beginPath();
  g.moveTo(x1, y1);
  g.lineTo(x0, y0);
  g.stroke();
}

function pen(g: CanvasRenderingContext2D): void {
  // BasicStroke(3): square caps, mitre joins; every segment is its own drawLine.
  g.lineWidth = 3;
  g.lineCap = "square";
}

/** DrawPainting.draw: every finished line, then the one being dragged. */
function redraw(l: Layer): void {
  const g = l.canvas.getContext("2d")!;
  const ratio = window.devicePixelRatio || 1;
  g.setTransform(ratio, 0, 0, ratio, 0, 0);
  g.clearRect(0, 0, l.canvas.width, l.canvas.height);
  pen(g);
  for (const s of l.current ? [...l.strokes, l.current] : l.strokes) {
    for (let i = 1; i < s.points.length; i++) segment(g, s, i);
  }
}

function fit(l: Layer): void {
  const ratio = window.devicePixelRatio || 1;
  const w = Math.round(l.root.clientWidth * ratio);
  const h = Math.round(l.root.clientHeight * ratio);
  if (l.canvas.width !== w || l.canvas.height !== h) {
    l.canvas.width = w;
    l.canvas.height = h;
  }
  redraw(l);
}

function button(name: IconName, key: string, action: () => void, tip = ""): HTMLElement {
  const b = document.createElement("span");
  b.className = "paint-btn";
  b.dataset.key = key;
  if (tip) b.title = tip;
  b.append(icon(name));
  b.addEventListener("click", action);
  return b;
}

/** LizzieFrame.drawPainting: one layer at a time here; another press of the button leaves it. */
export function openPainting(): void {
  const main = document.getElementById("main");
  if (layer || !main || !state.view) return;
  const root = document.createElement("div");
  root.id = "painting";
  root.tabIndex = -1;
  const canvas = document.createElement("canvas");
  const bar = document.createElement("div");
  bar.className = "paint-buttons";
  const l: Layer = { root, canvas, color: state.view.settings.edit.paintingColor, strokes: [], current: null, refresh: () => {} };
  const pens = PENS.map(([color, name]) => {
    const b = button(name, `painting.${color}`, () => {
      l.color = color;
      l.refresh();
    });
    b.setAttribute("role", "button");
    return { b, color };
  });
  // setColorButton: the chosen pen is lit.
  l.refresh = () => {
    for (const p of pens) {
      const lit = p.color === l.color;
      p.b.classList.toggle("on", lit);
      p.b.setAttribute("aria-pressed", String(lit));
    }
  };
  bar.append(
    ...pens.map((p) => p.b),
    button("undo", "painting.undo", () => {
      if (l.strokes.pop()) redraw(l);
    }),
    button("clear-all", "painting.clear", () => {
      l.strokes = [];
      l.current = null;
      redraw(l);
    }),
    button("close", "painting.close", closePainting, "ESC"),
  );
  // The lines are drawn over the buttons too (DrawPainting.paint draws the image last).
  root.append(bar, canvas);
  l.refresh();

  root.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || (e.target as HTMLElement).closest(".paint-btn")) return;
    root.setPointerCapture(e.pointerId);
    l.current = { color: l.color, points: [] };
  });
  // mouseDragged adds a point; the press itself does not.
  root.addEventListener("pointermove", (e) => {
    const s = l.current;
    if (!s || !(e.buttons & 1)) return;
    const r = root.getBoundingClientRect();
    s.points.push([e.clientX - r.left, e.clientY - r.top]);
    if (s.points.length > 1) {
      const g = canvas.getContext("2d")!;
      const ratio = window.devicePixelRatio || 1;
      g.setTransform(ratio, 0, 0, ratio, 0, 0);
      pen(g);
      segment(g, s, s.points.length - 1);
    }
  });
  // mouseReleased (left): a line with any point joins the list.
  const release = (e: PointerEvent) => {
    if (e.button !== 0 || !l.current) return;
    if (l.current.points.length > 0) l.strokes.push(l.current);
    l.current = null;
  };
  root.addEventListener("pointerup", release);
  root.addEventListener("pointercancel", release);
  // The layer covers the workbench: no press, wheel or popup reaches the boards and panels.
  for (const type of ["mousedown", "mouseup", "click", "dblclick", "contextmenu", "wheel"]) {
    root.addEventListener(type, (e) => {
      e.stopPropagation();
      if (type !== "click") e.preventDefault();
    }, type === "wheel" ? { passive: false } : undefined);
  }

  main.append(root);
  layer = l;
  new ResizeObserver(() => {
    if (layer === l) fit(l);
  }).observe(root);
  fit(l);
  root.focus();
}

/** ESC or the close button: saveLastColor, then the layer and its lines go. */
export function closePainting(): void {
  const l = layer;
  if (!l) return;
  layer = null;
  l.root.remove();
  const edit = state.view?.settings.edit;
  if (edit && edit.paintingColor !== l.color) void run({ type: "SetEditSettings", settings: { ...edit, paintingColor: l.color } });
}

/** Acceptance: the open layer's pen and lines (point counts), in client coordinates. */
export function paintingReport() {
  if (!layer) return null;
  const r = layer.root.getBoundingClientRect();
  return {
    color: layer.color,
    rect: { x: r.left, y: r.top, w: r.width, h: r.height },
    strokes: layer.strokes.map((s) => ({ color: s.color, points: s.points.length })),
    dragging: layer.current?.points.length ?? null,
    focused: document.activeElement === layer.root,
  };
}

COMMANDS["toolbar.painting"] = { run: openPainting };
