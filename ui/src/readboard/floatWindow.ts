// 棋盘同步工具's float board window (FloatBoard.java): an undecorated, transparent, always-on-top
// window over the client's board. It paints the overlay the main window sends (FloatBoardRenderer)
// in physical pixels on FloatBoardRenderer's grid, and the eight icon buttons at its bottom right
// (setButton). Pointer, wheel and key input goes to the main window, which owns the hover, the
// preview depth and every command. The 位置 button's state (showPosBtn) is the window's own and is
// not persisted, like Java's.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Point } from "../contract/generated";
import { onAppearanceChange, syncAppearance } from "../appearance/state";
import { setLanguage } from "../i18n";
import type { BoardGeom } from "../workbench/layout";
import { loadImages, paintBoard } from "../workbench/paint";
import { FLOAT_DATA_EVENT, FLOAT_INPUT_EVENT, type FloatButton, type FloatInput, type FloatPayload } from "./floatModel";

const STYLE = `
@font-face { font-family: "Lizzie Winrate"; src: url("/fonts/OpenSans-Semibold.ttf") format("truetype"); }
html, body { margin: 0; height: 100%; overflow: hidden; background: transparent; user-select: none; }
#board { position: absolute; left: 0; top: 0; }
.fb { position: absolute; width: 19px; height: 19px; padding: 0; border: 1px solid rgba(128,128,128,0.8);
  background: transparent center no-repeat; z-index: 2; }
.fb[hidden] { display: none; }
`;

/** FloatBoardRenderer.setupSizeParameters (FloatBoardRenderer.java:208-245) for the window's
 *  physical size: the board inside the `pad` edge, columns and rows spaced on their own
 *  (round(W0/N)), the half-step margin int(W0/(2N)), the grid centred in the leftover pixels. The
 *  wood of the edit mode covers the whole inner rectangle (FloatBoard.drawBackground). */
function floatGeom(width: number, height: number, pad: number, cols: number, rows: number): BoardGeom {
  const w0 = width - 2 * pad;
  const h0 = height - 2 * pad;
  const square = Math.round(w0 / cols);
  const squareY = Math.round(h0 / rows);
  return {
    x: pad,
    y: pad,
    size: w0,
    sizeY: h0,
    margin: Math.trunc((w0 - square * cols) / 2) + Math.trunc(w0 / (2 * cols)),
    marginY: Math.trunc((h0 - squareY * rows) / 2) + Math.trunc(h0 / (2 * rows)),
    square,
    squareY,
    stoneRadius: Math.max(1, Math.trunc(Math.max(square, squareY) / 2) - 1),
    n: Math.max(cols, rows),
    cols,
    rows,
  };
}

export function bootFloatWindow(): void {
  const style = document.createElement("style");
  style.textContent = STYLE;
  document.head.append(style);
  const self = getCurrentWebviewWindow();
  const send = (input: FloatInput) => void emitTo("main", FLOAT_INPUT_EVENT, input);

  const canvas = document.createElement("canvas");
  canvas.id = "board";
  document.body.append(canvas);

  /** Buttons are not focusable (setFocusable(false)): the keys stay with the window. */
  const button = (run: () => void) => {
    const b = document.createElement("button");
    b.className = "fb";
    b.tabIndex = -1;
    b.addEventListener("mousedown", (e) => {
      e.preventDefault();
      e.stopPropagation();
    });
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      run();
    });
    document.body.append(b);
    return b;
  };
  const command = (name: FloatButton) => button(() => send({ type: "button", button: name }));
  const icon = (b: HTMLButtonElement, name: string) => (b.style.backgroundImage = `url(/assets/${name}.png)`);
  const stopGo = command("stopGo");
  const hideShow = command("hideShow");
  const edit = command("edit");
  // noEdit.png / edit.png are scaled to 16 x 16 (FloatBoard.java:150-155).
  edit.style.backgroundSize = "16px 16px";
  const left = command("left");
  const right = command("right");
  const up = command("up");
  const down = command("down");
  icon(left, "leftFloat");
  icon(right, "rightFloat");
  icon(up, "upFloat");
  icon(down, "downFloat");
  let showPos = false;
  const pos = button(() => {
    showPos = !showPos;
    place();
  });
  icon(pos, "pos");

  let data: FloatPayload | null = null;
  let geo: BoardGeom | null = null;

  /** setButton (FloatBoard.java:769-842): offsets from the bottom-right corner, less the edge
   *  zoomIn(zoomIn(20)) (the physical edge in logical pixels). */
  function place() {
    const inset = data ? Math.round(data.pad / (window.devicePixelRatio || 1)) : 0;
    const w = document.documentElement.clientWidth;
    const h = document.documentElement.clientHeight;
    const at = (b: HTMLButtonElement, dx: number, dy: number) => {
      b.style.left = `${w - dx - inset}px`;
      b.style.top = `${h - dy - inset}px`;
    };
    for (const b of [left, right, up, down]) b.hidden = !showPos;
    if (showPos) {
      at(stopGo, 60, 18);
      at(hideShow, 80, 18);
      at(left, 38, 18);
      at(right, 19, 18);
      at(up, 19, 54);
      at(down, 19, 36);
      at(pos, 19, 72);
      at(edit, 100, 18);
    } else {
      at(pos, 19, 38);
      at(stopGo, 20, 18);
      at(hideShow, 40, 18);
      at(edit, 60, 18);
    }
  }

  /** paintMianPanel (FloatBoard.java:546-565): nothing on a window of 40 pixels or less; the
   *  renderer only while the suggestions show or in the edit mode. Outside the edit mode the board
   *  and the stones are the client's own, except the stones under a shown variation (renderImages);
   *  the setup mode draws the goban and the stones and nothing more (FloatBoardRenderer.java:133-139). */
  const render = () => {
    if (!data) return;
    setLanguage(data.language);
    icon(stopGo, data.pondering ? "tostop" : "toplay");
    icon(hideShow, data.hidden ? "plus" : "minus");
    icon(edit, data.edit ? "noEdit" : "edit");
    edit.hidden = !data.editButton;
    place();
    const { width, height, pad, scene } = data;
    const dpr = window.devicePixelRatio || 1;
    canvas.style.width = `${width / dpr}px`;
    canvas.style.height = `${height / dpr}px`;
    if (canvas.width !== width || canvas.height !== height) {
      canvas.width = Math.max(1, width);
      canvas.height = Math.max(1, height);
    }
    const g = canvas.getContext("2d")!;
    g.setTransform(1, 0, 0, 1, 0, 0);
    g.clearRect(0, 0, canvas.width, canvas.height);
    geo = width > 40 && height > 40 ? floatGeom(width, height, pad, scene.cols, scene.rows) : null;
    if (!geo || (data.hidden && !data.edit)) return;
    const float = { boardType: data.boardType, setup: data.setup };
    const layers = data.setup ? { board: true, stones: true, float } : { board: data.edit, stones: data.edit || scene.preview !== null, float };
    paintBoard(g, geo, { x: 0, y: 0 }, { ...scene, unlimited: new Set(scene.unlimited) }, layers);
  };

  /** convertScreenToCoordinates (FloatBoardRenderer.java:2372-2392) at a window point, the
   *  logical coordinates turned physical first (zoomOut). */
  const pointAt = (e: MouseEvent): Point | null => {
    if (!geo || !data) return null;
    const dpr = window.devicePixelRatio || 1;
    const sqY = geo.squareY ?? geo.square;
    const x = Math.floor((Math.round(e.clientX * dpr) - geo.x - geo.margin + Math.trunc(geo.square / 2)) / geo.square);
    const y = Math.floor((Math.round(e.clientY * dpr) - geo.y - (geo.marginY ?? geo.margin) + Math.trunc(sqY / 2)) / sqY);
    return x >= 0 && y >= 0 && x < data.scene.cols && y < data.scene.rows ? { x, y } : null;
  };

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  canvas.addEventListener("mousedown", (e) => {
    e.preventDefault();
    send({ type: "press", point: pointAt(e), button: e.button });
  });
  canvas.addEventListener("mousemove", (e) => send({ type: "move", point: pointAt(e) }));
  document.documentElement.addEventListener("mouseleave", () => send({ type: "leave" }));
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    if (e.deltaY !== 0) send({ type: "wheel", down: e.deltaY > 0 });
  }, { passive: false });
  document.addEventListener("keydown", (e) => {
    e.preventDefault();
    send({ type: "key", code: e.code });
  });

  // The theme images decode before they reach the painter: their arrival repaints.
  onAppearanceChange(render);
  void self.listen<FloatPayload>(FLOAT_DATA_EVENT, (e) => {
    data = e.payload;
    void syncAppearance(data.appearance, data.theme);
    render();
  }).then(async () => {
    await Promise.all([loadImages(["board.png", "heatmap.png"]), document.fonts.load('12px "Lizzie Winrate"')]);
    render();
    send({ type: "ready" });
  });
  window.addEventListener("resize", render);
}
