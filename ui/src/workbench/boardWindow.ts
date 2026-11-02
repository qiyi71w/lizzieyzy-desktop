// 浮动主棋盘 / 浮动小棋盘 windows (IndependentMainBoard.java / IndependentSubBoard.java): an
// undecorated square window painting the scene the main window relays, with the close, always-on-
// top and lock buttons shown while the pointer is inside (19 px icons at the top left). Unlocked,
// a drag moves the window and a drag within 6-7 px of an edge resizes it, kept square. Pointer and
// key input goes to the main window, which owns the record and every interaction state.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import type { IndependentBoard, Point } from "../contract/generated";
import { setLanguage } from "../i18n";
import { INDEPENDENT_DATA_EVENT, INDEPENDENT_INPUT_EVENT, INDEPENDENT_LABELS, type IndependentInput, type IndependentPayload, type KeyInit } from "./independentModel";
import { boardGeom, mainMarginRatio, SUB_MARGIN_RATIO, type BoardGeom } from "./layout";
import { loadImages, paintBoard, paintSubBoard, prepare } from "./paint";
import { onAppearanceChange, syncAppearance } from "../appearance/state";

const STYLE = `
@font-face { font-family: "Lizzie Winrate"; src: url("/fonts/OpenSans-Semibold.ttf") format("truetype"); }
html, body { margin: 0; height: 100%; overflow: hidden; background: #000; user-select: none; }
#board { position: absolute; left: 0; top: 0; }
.ib { position: absolute; left: 0; width: 19px; height: 19px; padding: 0; border: 1px solid #888;
  background: #eee center / 15px 15px no-repeat; display: none; z-index: 2; }
body.over .ib { display: block; }
`;

/** Java's resize zones: within 6 px of the top or left edge, 7 px of the bottom or right edge. */
type Edge = "top" | "down" | "left" | "right" | null;

export function bootBoardWindow(): void {
  const style = document.createElement("style");
  style.textContent = STYLE;
  document.head.append(style);
  const self = getCurrentWebviewWindow();
  const board: IndependentBoard = self.label === INDEPENDENT_LABELS.Sub ? "Sub" : "Main";
  const send = (input: Record<string, unknown> & { type: string }) => void emitTo("main", INDEPENDENT_INPUT_EVENT, { board, ...input } as IndependentInput);

  const canvas = document.createElement("canvas");
  canvas.id = "board";
  document.body.append(canvas);
  const button = (top: number, type: "close" | "top" | "lock") => {
    const b = document.createElement("button");
    b.className = "ib";
    b.style.top = `${top}px`;
    b.tabIndex = -1;
    b.addEventListener("mousedown", (e) => e.stopPropagation());
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      send({ type });
    });
    document.body.append(b);
    return b;
  };
  // btnClose (0, 0), topUntop (0, 19), lockUnlock (0, 38).
  button(0, "close").style.backgroundImage = "url(/assets/close.png)";
  const top = button(19, "top");
  const lock = button(38, "lock");

  let data: IndependentPayload | null = null;
  let geo: BoardGeom | null = null;
  const size = () => ({ w: document.documentElement.clientWidth, h: document.documentElement.clientHeight });

  const render = () => {
    if (!data) return;
    setLanguage(data.language);
    const { w, h } = size();
    const r = { x: 0, y: 0, w, h };
    top.style.backgroundImage = `url(/assets/${data.onTop ? "btm" : "top"}.png)`;
    lock.style.backgroundImage = `url(/assets/${data.locked ? "Locked" : "Unlocked"}.png)`;
    if (data.board === "Main") {
      geo = boardGeom(r, data.scene.cols, data.scene.rows, mainMarginRatio(data.bigMargin));
      paintBoard(prepare(canvas, r), geo, r, { ...data.scene, unlimited: new Set(data.scene.unlimited) });
    } else {
      geo = boardGeom(r, data.scene.cols, data.scene.rows, SUB_MARGIN_RATIO);
      paintSubBoard(prepare(canvas, r), geo, r, data.scene);
    }
  };

  /** The intersection under a window point (BoardRenderer.convertScreenToCoordinates). */
  const pointAt = (x: number, y: number): Point | null => {
    if (!geo || !data || data.board !== "Main") return null;
    const cx = Math.floor((x - geo.x - geo.margin + geo.square / 2) / geo.square);
    const cy = Math.floor((y - geo.y - geo.margin + geo.square / 2) / geo.square);
    return cx >= 0 && cy >= 0 && cx < data.scene.cols && cy < data.scene.rows ? { x: cx, y: cy } : null;
  };

  // ---------------------------------------------------------------- moving and resizing
  let edge: Edge = null;
  /** The press moving or resizing the window: screen point and the window's logical bounds. */
  let grab: { sx: number; sy: number; x: number; y: number; w: number; h: number; edge: Edge } | null = null;
  const edgeAt = (x: number, y: number): Edge => {
    const { w, h } = size();
    if (y <= 6) return "top";
    if (Math.abs(y - h) <= 7) return "down";
    if (x <= 6) return "left";
    if (Math.abs(x - w) <= 7) return "right";
    return null;
  };
  const startGrab = async (e: MouseEvent) => {
    const scale = await self.scaleFactor();
    const pos = (await self.outerPosition()).toLogical(scale);
    const { w, h } = size();
    grab = { sx: e.screenX, sy: e.screenY, x: pos.x, y: pos.y, w, h, edge };
  };
  /** mouseDragged (IndependentMainBoard.java:418-441): an edge keeps the window square. */
  const dragTo = (e: MouseEvent) => {
    if (!grab) return;
    const dx = e.screenX - grab.sx;
    const dy = e.screenY - grab.sy;
    const g = grab;
    let side: number;
    switch (g.edge) {
      case "top":
        side = Math.max(1, g.h - dy);
        void self.setSize(new LogicalSize(side, side));
        void self.setPosition(new LogicalPosition(g.x, g.y + g.h - side));
        break;
      case "down":
        side = Math.max(1, g.h + dy);
        void self.setSize(new LogicalSize(side, side));
        break;
      case "left":
        side = Math.max(1, g.w - dx);
        void self.setSize(new LogicalSize(side, side));
        void self.setPosition(new LogicalPosition(g.x + g.w - side, g.y));
        break;
      case "right":
        side = Math.max(1, g.w + dx);
        void self.setSize(new LogicalSize(side, side));
        break;
      default:
        void self.setPosition(new LogicalPosition(g.x + dx, g.y + dy));
    }
  };

  // ---------------------------------------------------------------- pointer
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  document.documentElement.addEventListener("mouseenter", () => {
    document.body.classList.add("over");
    send({ type: "enter" });
  });
  document.documentElement.addEventListener("mouseleave", () => {
    document.body.classList.remove("over");
    send({ type: "leave" });
  });
  canvas.addEventListener("mousedown", (e) => {
    e.preventDefault();
    const point = pointAt(e.offsetX, e.offsetY);
    send({ type: "press", point, button: e.button, detail: e.detail, alt: e.altKey, sx: e.screenX, sy: e.screenY });
    if (e.button !== 0 || !data || data.locked) return;
    // A press on a stone with 启用拖动棋子 drags the stone (Draggedmode), not the window.
    if (data.board === "Main" && point && data.allowDrag && data.scene.stones[point.y * data.scene.cols + point.x]) return;
    void startGrab(e);
  });
  // The window keeps the drag while the pointer leaves it.
  canvas.addEventListener("pointerdown", (e) => {
    if (e.button === 0) canvas.setPointerCapture(e.pointerId);
  });
  window.addEventListener("mousemove", (e) => {
    if (grab && e.buttons & 1) {
      dragTo(e);
      return;
    }
    if (!data) return;
    if (!data.locked) {
      edge = edgeAt(e.clientX, e.clientY);
      canvas.style.cursor = edge === "top" || edge === "down" ? "ns-resize" : edge ? "ew-resize" : "default";
    } else {
      edge = null;
      canvas.style.cursor = "default";
    }
    if (data.board === "Main" && geo) {
      send({ type: "hover", fx: (e.clientX - geo.x - geo.margin) / geo.square, fy: (e.clientY - geo.y - geo.margin) / geo.square, reach: (geo.stoneRadius + 1) / geo.square });
    }
  });
  window.addEventListener("mouseup", (e) => {
    grab = null;
    send({ type: "release", point: pointAt(e.clientX, e.clientY), button: e.button });
  });
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (e.deltaY !== 0) send({ type: "wheel", down: e.deltaY > 0 });
  }, { passive: false });

  // ---------------------------------------------------------------- keys (InputIndependentMainBoard)
  const forward = (kind: "keydown" | "keyup") => (e: KeyboardEvent) => {
    e.preventDefault();
    const init: KeyInit = { key: e.key, code: e.code, altKey: e.altKey, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, metaKey: e.metaKey, repeat: e.repeat };
    send({ type: "key", kind, init });
  };
  document.addEventListener("keydown", forward("keydown"));
  document.addEventListener("keyup", forward("keyup"));

  // The theme images decode before they reach the painters: their arrival repaints.
  onAppearanceChange(render);
  void self.listen<IndependentPayload>(INDEPENDENT_DATA_EVENT, (e) => {
    data = e.payload;
    void syncAppearance(data.appearance, data.theme);
    render();
  }).then(async () => {
    await Promise.all([
      loadImages(["board.png", "heatmap.png"]),
      document.fonts.load('12px "Lizzie Winrate"'),
    ]);
    render();
    send({ type: "ready" });
  });
  window.addEventListener("resize", render);
}
