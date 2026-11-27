// 浮动主棋盘 / 浮动小棋盘 windows (IndependentMainBoard.java / IndependentSubBoard.java): an
// undecorated square window painting the scene the main window relays, edged by a --ds-line-strong
// hairline, with the close, always-on-top and lock buttons shown while the pointer is inside or one
// of them has the keyboard focus (a column of 19 px icon buttons at the top left, in the Tab order,
// named by their tooltips; the two toggles read as pressed while on).
// Unlocked, a drag moves the window and a drag within 6-7 px of an edge resizes it, kept square.
// Pointer and key input goes to the main window, which owns the record and every interaction state.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { LogicalPosition, LogicalSize } from "@tauri-apps/api/window";
import type { IndependentBoard, Point, SlotThumbnail } from "../contract/generated";
import { appMenuKey } from "../appMenuKey";
import { product, registerProduct, setLanguage } from "../i18n";
import { INDEPENDENT_DATA_EVENT, INDEPENDENT_INPUT_EVENT, INDEPENDENT_LABELS, INDEPENDENT_PICTURE_EVENT, type IndependentInput, type IndependentPayload, type KeyInit } from "./independentModel";
import { boardGeom, selectRange, SUB_MARGIN_RATIO, type BoardGeom } from "./layout";
import { loadImages, paintBoard, paintSubBoard, prepare, type NamesReport } from "./paint";
import { onAppearanceChange, syncAppearance } from "../appearance/state";
import { icon } from "../design/icons";
import { token } from "../design/theme";
import "../design/window";

const STYLE = `
@font-face { font-family: "Lizzie Winrate"; src: url("/fonts/OpenSans-Semibold.ttf") format("truetype"); }
html, body { overflow: hidden; background: var(--ds-panel); }
body::after { content: ""; position: fixed; inset: 0; z-index: 3; border: 1px solid var(--ds-line-strong); pointer-events: none; }
#board { position: absolute; left: 0; top: 0; }
.ib { --menu-icon-size: 13px; position: absolute; left: 0; z-index: 2; width: 19px; height: 19px; display: inline-flex;
  align-items: center; justify-content: center; padding: 0; border-radius: 0; color: var(--ds-text-2); opacity: 0; pointer-events: none; }
.ib:hover { color: var(--ds-text); }
.ib:focus-visible { outline-offset: -3px; }
.ib[aria-pressed="true"] { background-image: linear-gradient(var(--ds-accent-soft), var(--ds-accent-soft)); color: var(--ds-accent-text); }
body.over .ib, body:has(.ib:focus-visible) .ib { opacity: 1; pointer-events: auto; }
`;

registerProduct({
  ZhCn: { boardAlwaysOnTop: "置顶", boardLock: "锁定窗口" },
  ZhTw: { boardAlwaysOnTop: "置頂", boardLock: "鎖定視窗" },
  EnUs: { boardAlwaysOnTop: "Always on top", boardLock: "Lock window" },
  JaJp: { boardAlwaysOnTop: "常に前面表示", boardLock: "ウィンドウを固定" },
  KoKr: { boardAlwaysOnTop: "항상 맨 앞에", boardLock: "창 고정" },
  ThTh: { boardAlwaysOnTop: "อยู่ด้านบนเสมอ", boardLock: "ล็อกหน้าต่าง" },
});

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
    b.type = "button";
    b.className = "ib";
    b.style.top = `${top}px`;
    // A press leaves the focus on the board, so its keys keep going to the board after a click.
    b.addEventListener("mousedown", (e) => {
      e.stopPropagation();
      e.preventDefault();
    });
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      send({ type });
    });
    document.body.append(b);
    return b;
  };
  // btnClose (0, 0), topUntop (0, 19), lockUnlock (0, 38).
  const close = button(0, "close");
  close.append(icon("close"));
  const top = button(19, "top");
  top.append(icon("pin"));
  const lock = button(38, "lock");
  lock.append(icon("lock"));
  const names: [HTMLButtonElement, string][] = [[close, "close"], [top, "boardAlwaysOnTop"], [lock, "boardLock"]];

  let data: IndependentPayload | null = null;
  let geo: BoardGeom | null = null;
  /** The names as last drawn (IndependentMainBoard's boardRenderer.isOnPlayerName) and what was
   *  last reported of them, null before the first report: a new window always reports its first
   *  drawing, even no names, so the main window drops what a closed one reported. */
  let drawnNames: NamesReport | null = null;
  let namesSent: string | null = null;
  const size = () => ({ w: document.documentElement.clientWidth, h: document.documentElement.clientHeight });

  const render = () => {
    if (!data) return;
    setLanguage(data.language);
    for (const [b, key] of names) {
      const text = product(key);
      b.title = text;
      b.setAttribute("aria-label", text);
    }
    const { w, h } = size();
    const r = { x: 0, y: 0, w, h };
    top.setAttribute("aria-pressed", String(data.onTop));
    lock.setAttribute("aria-pressed", String(data.locked));
    if (data.board === "Main") {
      geo = boardGeom(r, data.scene.cols, data.scene.rows, data.marginRatio);
      drawnNames = paintBoard(prepare(canvas, r), geo, r, { ...data.scene, unlimited: new Set(data.scene.unlimited) }).names;
      const text = JSON.stringify(drawnNames);
      if (text !== namesSent) {
        namesSent = text;
        send({ type: "names", names: drawnNames });
      }
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
  /** The press of a region selection (Alt, or a lit region button): it drags the selection, not
   *  the window (IndependentMainBoard.mousePressed / mouseDragged / mouseReleased). */
  let selection: { x: number; y: number; button: number } | null = null;
  const range = (e: MouseEvent) => (geo && selection ? selectRange(geo, selection.x, selection.y, e.clientX, e.clientY) : null);
  canvas.addEventListener("mousedown", (e) => {
    e.preventDefault();
    const point = pointAt(e.offsetX, e.offsetY);
    const box = drawnNames?.box;
    const onNames = !!box && e.offsetX >= box.x && e.offsetX < box.x + box.w && e.offsetY >= box.y && e.offsetY < box.y + box.h;
    send({ type: "press", point, button: e.button, detail: e.detail, alt: e.altKey, sx: e.screenX, sy: e.screenY, onNames });
    if (data?.board === "Main" && e.button !== 1 && (e.altKey || data.selectMode)) {
      selection = { x: e.clientX, y: e.clientY, button: e.button };
      return;
    }
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
    if (selection) {
      send({ type: "select", range: range(e), end: false, click: false });
      return;
    }
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
      send({ type: "hover", fx: (e.clientX - geo.x - geo.margin) / geo.square, fy: (e.clientY - geo.y - geo.margin) / geo.square, pressed: e.buttons !== 0 });
    }
  });
  window.addEventListener("mouseup", (e) => {
    if (selection) {
      if (e.button !== selection.button) return;
      send({ type: "select", range: range(e), end: true, click: e.altKey && e.clientX === selection.x && e.clientY === selection.y });
      selection = null;
      return;
    }
    grab = null;
    send({ type: "release", point: pointAt(e.clientX, e.clientY), button: e.button });
  });
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (e.deltaY !== 0) send({ type: "wheel", down: e.deltaY > 0 });
  }, { passive: false });

  // ---------------------------------------------------------------- keys (InputIndependentMainBoard)
  const forward = (kind: "keydown" | "keyup") => (e: KeyboardEvent) => {
    if (appMenuKey(e)) return;
    // A focused button keeps Tab, Enter and Space: they move between the buttons or press one.
    // Shift+Tab may arrive as another key (X11's ISO_Left_Tab) on the Tab key's code.
    const tab = e.key === "Tab" || e.code === "Tab";
    if (e.target instanceof HTMLButtonElement && (tab || e.key === "Enter" || e.key === " ")) return;
    // Tab from the board goes to the main window as every key does, and also reaches the buttons.
    if (!tab) e.preventDefault();
    const init: KeyInit = { key: e.key, code: e.code, altKey: e.altKey, ctrlKey: e.ctrlKey, shiftKey: e.shiftKey, metaKey: e.metaKey, repeat: e.repeat };
    send({ type: "key", kind, init });
  };
  document.addEventListener("keydown", forward("keydown"));
  document.addEventListener("keyup", forward("keyup"));

  // The theme images decode before they reach the painters: their arrival repaints.
  onAppearanceChange(render);
  void self.listen<IndependentPayload>(INDEPENDENT_DATA_EVENT, (e) => {
    data = e.payload;
    void syncAppearance(data.appearance, data.theme, data.themeFilesRevision);
    render();
  }).then(async () => {
    await Promise.all([
      loadImages(["board.png", "heatmap.png"]),
      document.fonts.load('12px "Lizzie Winrate"'),
    ]);
    render();
    send({ type: "ready" });
  });
  // getIndependMainBoardToClipboard / getIndependSubBoardToClipboard: the window's area as
  // painted, at its size, opaque (TYPE_INT_RGB) on the window's ground, with the position of the
  // scene it shows (a newer scene may have replaced the one the main window had when it asked).
  void self.listen<{ id: number }>(INDEPENDENT_PICTURE_EVENT, (e) => {
    let picture: SlotThumbnail | null = null;
    const { w, h } = size();
    if (data && w > 0 && h > 0) {
      render();
      const out = document.createElement("canvas");
      out.width = w;
      out.height = h;
      const g = out.getContext("2d")!;
      g.fillStyle = token("--ds-panel");
      g.fillRect(0, 0, w, h);
      g.drawImage(canvas, 0, 0, w, h);
      const url = out.toDataURL("image/png");
      picture = { png: url.slice(url.indexOf(",") + 1), ...data.shows };
    }
    send({ type: "picture", id: e.payload.id, picture });
  });
  window.addEventListener("resize", render);
}
