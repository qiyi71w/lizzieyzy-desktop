// The parts of the window bar that belong to the window frame: the title in the bar's free space,
// the drag region and, on Linux, the window buttons and the resize edges. macOS draws its own
// buttons over the bar's left end; Windows keeps the system title bar, so there the bar only
// carries the menus.
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { os } from "../design/theme";
import { product, registerProduct } from "../i18n";

registerProduct({
  ZhCn: { windowMinimize: "最小化", windowMaximize: "最大化", windowRestore: "还原", windowClose: "关闭" },
  ZhTw: { windowMinimize: "最小化", windowMaximize: "最大化", windowRestore: "還原", windowClose: "關閉" },
  EnUs: { windowMinimize: "Minimize", windowMaximize: "Maximize", windowRestore: "Restore", windowClose: "Close" },
  JaJp: { windowMinimize: "最小化", windowMaximize: "最大化", windowRestore: "元に戻す", windowClose: "閉じる" },
  KoKr: { windowMinimize: "최소화", windowMaximize: "최대화", windowRestore: "이전 크기로", windowClose: "닫기" },
  ThTh: { windowMinimize: "ย่อ", windowMaximize: "ขยาย", windowRestore: "คืนขนาด", windowClose: "ปิด" },
});

type WindowButton = "minimize" | "maximize" | "close";
/** The edge or corner a resize starts from, as the window API names it. */
type ResizeDirection = "East" | "North" | "NorthEast" | "NorthWest" | "South" | "SouthEast" | "SouthWest" | "West";
type Glyph = WindowButton | "restore";

const ICON: Record<Glyph, string> = {
  minimize: '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 5h7"/></svg>',
  maximize: '<svg viewBox="0 0 10 10" aria-hidden="true"><rect x="1.5" y="1.5" width="7" height="7"/></svg>',
  restore: '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M3.5 3.5V1.5h5v5h-2M1.5 3.5h5v5h-5z"/></svg>',
  close: '<svg viewBox="0 0 10 10" aria-hidden="true"><path d="M1.5 1.5l7 7M8.5 1.5l-7 7"/></svg>',
};
const LABEL: Record<Glyph, string> = { minimize: "windowMinimize", maximize: "windowMaximize", restore: "windowRestore", close: "windowClose" };

/** Text padding of #windowTitle on each side (styles.css). */
const TITLE_PAD = 12;
/** The system title is rewritten at most this often: an analysing engine changes it ten times a
 *  second, which task bars and window lists then repaint. */
const SYSTEM_TITLE_INTERVAL_MS = 1000;
/** A press this close to the window's edge resizes it; along an edge, this close to its end
 *  resizes from the corner. */
const EDGE = 5;
const CORNER = 16;

let bar: HTMLElement | null = null;
let title: HTMLElement | null = null;
const buttons = new Map<WindowButton, HTMLButtonElement>();
const measure = document.createElement("canvas").getContext("2d")!;

/** Centres the title in the window when it fits between its neighbours, else in its own space. */
function layoutTitle(): void {
  if (!bar || !title) return;
  title.style.paddingLeft = title.style.paddingRight = "";
  const frame = bar.getBoundingClientRect();
  const box = title.getBoundingClientRect();
  measure.font = getComputedStyle(title).font;
  const text = measure.measureText(title.textContent ?? "").width;
  // Padding added on one side moves the text's centre by half of it.
  const shift = Math.round(frame.left + frame.right - box.left - box.right);
  if (!shift || text + Math.abs(shift) > box.width - TITLE_PAD * 2) return;
  if (shift > 0) title.style.paddingLeft = `${TITLE_PAD + shift}px`;
  else title.style.paddingRight = `${TITLE_PAD - shift}px`;
}

let systemTitle = "";
let wantedTitle = "";
let systemTimer = 0;

function writeSystemTitle(): void {
  systemTitle = wantedTitle;
  document.title = systemTitle;
  void getCurrentWindow().setTitle(systemTitle);
  systemTimer = window.setTimeout(() => {
    systemTimer = 0;
    if (wantedTitle !== systemTitle) writeSystemTitle();
  }, SYSTEM_TITLE_INTERVAL_MS);
}

/** The workbench title: in the window bar at once; in the system's title (task bar, window list,
 *  the Windows title bar) at once when that was at rest, then at most once a second. */
export function setWindowTitle(text: string): void {
  if (title && title.textContent !== text) {
    title.textContent = text;
    layoutTitle();
  }
  wantedTitle = text;
  if (!systemTimer && wantedTitle !== systemTitle) writeSystemTitle();
}

/** The window's state the bar follows: a maximized window has no edge and its maximize button
 *  restores; a full-screen macOS window has no window buttons over the bar. */
async function syncFrame(): Promise<void> {
  const window = getCurrentWindow();
  const [maximized, fullscreen] = await Promise.all([window.isMaximized(), window.isFullscreen()]);
  const root = document.documentElement;
  root.toggleAttribute("data-maximized", maximized);
  root.toggleAttribute("data-fullscreen", fullscreen);
  for (const [name, button] of buttons) {
    const glyph: Glyph = name === "maximize" && maximized ? "restore" : name;
    if (button.dataset.glyph === glyph) continue;
    button.dataset.glyph = glyph;
    button.innerHTML = ICON[glyph];
    button.title = product(LABEL[glyph]);
    button.setAttribute("aria-label", button.title);
  }
}

/** The window buttons of one side of the bar, in the desktop's order. */
function windowButtons(names: WindowButton[], leading: boolean): HTMLElement {
  const window = getCurrentWindow();
  const group = document.createElement("div");
  group.className = leading ? "window-buttons leading" : "window-buttons";
  for (const name of names) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = name;
    button.tabIndex = -1;
    button.dataset.key = `window.${name}`;
    button.onclick = () => void (name === "minimize" ? window.minimize() : name === "maximize" ? window.toggleMaximize() : window.close());
    buttons.set(name, button);
    group.append(button);
  }
  return group;
}

/** A window without the system's frame is resized from its own edges: the pointer shows the
 *  direction there and a press hands the resize to the window manager. */
function bindResizeEdges(): void {
  const root = document.documentElement;
  // -1 within `reach` of the start of a side of length `size`, 1 within it of the end, else 0.
  const side = (at: number, size: number, reach: number) => (at < reach ? -1 : at >= size - reach ? 1 : 0);
  const edge = (e: MouseEvent): ResizeDirection | null => {
    if (root.hasAttribute("data-maximized") || root.hasAttribute("data-fullscreen")) return null;
    const onColumn = side(e.clientX, window.innerWidth, EDGE) !== 0;
    const onRow = side(e.clientY, window.innerHeight, EDGE) !== 0;
    if (!onColumn && !onRow) return null;
    const vertical = side(e.clientY, window.innerHeight, onColumn ? CORNER : EDGE);
    const horizontal = side(e.clientX, window.innerWidth, onRow ? CORNER : EDGE);
    return `${["North", "", "South"][vertical + 1]}${["West", "", "East"][horizontal + 1]}` as ResizeDirection;
  };
  const CURSOR: Record<ResizeDirection, string> = { North: "ns-resize", South: "ns-resize", West: "ew-resize", East: "ew-resize", NorthWest: "nwse-resize", SouthEast: "nwse-resize", NorthEast: "nesw-resize", SouthWest: "nesw-resize" };
  window.addEventListener("mousemove", (e) => {
    const direction = e.buttons ? null : edge(e);
    if (direction) root.style.setProperty("--window-edge-cursor", CURSOR[direction]);
    root.toggleAttribute("data-window-edge", !!direction);
  }, true);
  window.addEventListener("mouseout", (e) => {
    if (!e.relatedTarget) root.removeAttribute("data-window-edge");
  });
  window.addEventListener("mousedown", (e) => {
    const direction = e.button === 0 ? edge(e) : null;
    if (!direction) return;
    // The press belongs to the window's edge, not to what lies under it.
    e.preventDefault();
    e.stopImmediatePropagation();
    void getCurrentWindow().startResizeDragging(direction);
  }, true);
}

/** Adds the title and, where this window draws its own frame, the drag region, the window buttons
 *  and the resize edges to the bar that already holds the menus. */
export async function mountWindowBar(host: HTMLElement): Promise<void> {
  bar = host;
  title = document.createElement("div");
  title.id = "windowTitle";
  host.append(title);
  new ResizeObserver(layoutTitle).observe(title);
  if (os === "windows") return;
  // Pressing the bar's free space or the title moves the window; a double press maximizes it.
  host.toggleAttribute("data-tauri-drag-region", true);
  title.toggleAttribute("data-tauri-drag-region", true);
  if (os === "linux") {
    document.documentElement.dataset.frame = "own";
    // GTK's `gtk-decoration-layout`, e.g. "appmenu:minimize,maximize,close": the names before the
    // colon sit at the bar's start, the others at its end. Without any known button the usual
    // three sit at the end.
    const layout = await invoke<string>("window_button_layout");
    const [leading = [], trailing = []] = layout.split(":").map((side) => side.split(",").map((name) => name.trim()).filter((name): name is WindowButton => name === "minimize" || name === "maximize" || name === "close"));
    const known = leading.length + trailing.length > 0;
    if (leading.length) host.append(windowButtons(leading, true));
    if (trailing.length || !known) host.append(windowButtons(known ? trailing : ["minimize", "maximize", "close"], false));
    bindResizeEdges();
  }
  await syncFrame();
  await getCurrentWindow().onResized(() => void syncFrame());
}
