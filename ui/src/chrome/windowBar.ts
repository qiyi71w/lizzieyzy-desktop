// The window bar as the window's title bar on every platform: the LizzieYzy mark, the title in
// the bar's free space and the drag region; on Windows and Linux also the window buttons, and on
// Linux the resize edges (Windows resizes a frameless window itself). macOS draws its own buttons
// over the bar's left end.
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

/** The workbench title in the window bar's two parts and the system's one line: `game` names the
 *  game (players, result, file), `detail` the analysis in parts, the most wanted first (values,
 *  engine with speed, notices), `detailTip` names the values for the pointer; `text` is the system
 *  title (task bar, window list). */
export interface WindowTitle {
  text: string;
  game: string;
  detail: string[];
  detailTip?: string;
}

/** Clearance between the title and the menus or window buttons beside it. */
const TITLE_PAD = 20;
/** Space between the game and the analysis when they come close. */
const TITLE_GAP = 24;
const DETAIL_SEPARATOR = " · ";
/** The system title is rewritten at most this often: an analysing engine changes it ten times a
 *  second, which task bars and window lists then repaint. */
const SYSTEM_TITLE_INTERVAL_MS = 1000;
/** A press this close to the window's edge resizes it; along an edge, this close to its end
 *  resizes from the corner. */
const EDGE = 5;
const CORNER = 16;

let bar: HTMLElement | null = null;
let title: HTMLElement | null = null;
const titleGame = document.createElement("span");
const titleDetail = document.createElement("span");
const buttons = new Map<WindowButton, HTMLButtonElement>();
const titleRowListeners: (() => void)[] = [];

/** Calls `listener` when the bar gains or loses its title row, which changes the bar's height
 *  and so the room left to the workbench below it. */
export function onTitleRowChange(listener: () => void): void {
  titleRowListeners.push(listener);
}

/** The title is one line beside the menus where it fits. A game too wide for the bar's free space
 *  there takes a row of its own under the menus instead: the window's whole width, centred, broken
 *  between words onto a second line when even that is too narrow (inside a name only when the name
 *  alone is wider than the row). A game longer than two lines ends the second in "…". That row
 *  leaves the analysis out (the system title keeps it); its tooltip gives the whole title when the
 *  row leaves out the analysis or shortens the game. */
function layoutTitle(): void {
  if (!bar || !title) return;
  const before = bar.hasAttribute("data-title-row");
  bar.removeAttribute("data-title-row");
  titleGame.textContent = wanted.game;
  placeTitle();
  if (titleGame.scrollWidth > titleGame.clientWidth) {
    bar.setAttribute("data-title-row", "");
    title.style.paddingLeft = "";
    titleDetail.hidden = true;
    if (breaksAfterHyphen()) titleGame.textContent = wanted.game.replace(/-(?=\S)/g, "-\u2060");
    title.title = wanted.detail.length || clamped() ? wanted.text : "";
  }
  if (before !== bar.hasAttribute("data-title-row")) titleRowListeners.forEach((listener) => listener());
}

/** Whether the title row breaks a line right after a hyphen inside a part. Lines should break
 *  only at the spaces, but Chromium (and so WebView2) also breaks after a hyphen
 *  ("Wolfeschlegel-" / "Steinhause"); the row then writes a word joiner after each such hyphen,
 *  which keeps a part the row holds on one line while a part wider than the row still breaks
 *  inside itself (overflow-wrap). The joiner is left out otherwise, as it can move the glyphs
 *  around it by a fraction of a pixel. */
function breaksAfterHyphen(): boolean {
  const text = titleGame.firstChild;
  const s = text?.textContent ?? "";
  if (!text) return false;
  const range = document.createRange();
  const top = (i: number): number => {
    range.setStart(text, i);
    range.setEnd(text, i + 1);
    return range.getBoundingClientRect().top;
  };
  for (let i = s.indexOf("-"); i >= 0 && i + 1 < s.length; i = s.indexOf("-", i + 1)) {
    if (s[i + 1] !== " " && top(i + 1) > top(i) + 1) return true;
  }
  return false;
}

/** Whether the title row holds more of the game than its two lines show. */
function clamped(): boolean {
  return (bar?.hasAttribute("data-title-row") ?? false) && titleGame.scrollHeight > titleGame.clientHeight;
}

/** The title beside the menus: the game sits on the window's centre line, or as close to it as the
 *  menus and the analysis allow (moving continuously, so a changing analysis never makes it jump);
 *  the analysis stands at the bar's far end, quieter, with as many of its leading parts as fit
 *  beside the whole game, none when not even the first does. A shortened title has the whole of it
 *  as its tooltip. */
function placeTitle(): void {
  if (!bar || !title) return;
  title.style.paddingLeft = "";
  titleDetail.hidden = false;
  const frame = bar.getBoundingClientRect();
  const box = title.getBoundingClientRect();
  const room = box.width - TITLE_PAD * 2;
  // scrollWidth rounds the text's width; one pixel more keeps a text that fits from ending in "…".
  const game = titleGame.scrollWidth + 1;
  let parts = wanted.detail.length;
  let detail = 0;
  for (; parts > 0; parts--) {
    titleDetail.textContent = wanted.detail.slice(0, parts).join(DETAIL_SEPARATOR);
    detail = titleDetail.scrollWidth + 1;
    if (game + TITLE_GAP + detail <= room) break;
  }
  titleDetail.hidden = parts === 0;
  titleDetail.title = wanted.detailTip ?? "";
  const start = box.left + TITLE_PAD;
  const end = box.right - TITLE_PAD - (parts ? detail + TITLE_GAP : 0);
  const centre = (frame.left + frame.right) / 2;
  const left = Math.max(start, Math.min(end - game, centre - game / 2));
  title.style.paddingLeft = `${Math.floor(left - box.left)}px`;
  const shortened = titleGame.scrollWidth > titleGame.clientWidth || parts < wanted.detail.length;
  title.title = shortened ? wanted.text : "";
}

let systemTitle = "";
let wanted: WindowTitle = { text: "", game: "", detail: [] };
let systemTimer = 0;

function writeSystemTitle(): void {
  systemTitle = wanted.text;
  document.title = systemTitle;
  void getCurrentWindow().setTitle(systemTitle);
  systemTimer = window.setTimeout(() => {
    systemTimer = 0;
    if (wanted.text !== systemTitle) writeSystemTitle();
  }, SYSTEM_TITLE_INTERVAL_MS);
}

/** The workbench title: in the window bar at once; in the system's title (task bar, window list)
 *  at once when that was at rest, then at most once a second. */
export function setWindowTitle(next: WindowTitle): void {
  if (next.text === wanted.text && next.game === wanted.game && next.detail.join("\n") === wanted.detail.join("\n")) return;
  wanted = next;
  layoutTitle();
  if (!systemTimer && wanted.text !== systemTitle) writeSystemTitle();
}

/** What the acceptance probe reports of the bar's title: the game, whether it is cut for want of
 *  room (one line ending in "…", or a title row past its two lines), whether it has the row under
 *  the menus and whether that row is past its two lines, and the tooltip giving the whole title. */
export function titleReport(): { game: string; cut: boolean; row: boolean; clamped: boolean; tooltip: string } {
  const rowClamped = clamped();
  return { game: wanted.game, cut: titleGame.scrollWidth > titleGame.clientWidth || rowClamped, row: bar?.hasAttribute("data-title-row") ?? false, clamped: rowClamped, tooltip: title?.title ?? "" };
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
 *  direction there and a press hands the resize to the window manager. While the pointer is in the
 *  edge band, a transparent layer over the whole window carries the direction's cursor, so the
 *  controls under the band show it too. The layer is a child of the root element, outside the body
 *  a modal dialog makes inert, so it stays over dialogs as well. */
function bindResizeEdges(): void {
  const root = document.documentElement;
  // -1 within `reach` of the start of a side of length `size`, 1 within it of the end, else 0.
  const side = (at: number, size: number, reach: number) => (at < reach ? -1 : at >= size - reach ? 1 : 0);
  // By vertical, then horizontal side: [-1, 0, 1] → [0, 1, 2].
  const DIRECTIONS: (ResizeDirection | null)[][] = [
    ["NorthWest", "North", "NorthEast"],
    ["West", null, "East"],
    ["SouthWest", "South", "SouthEast"],
  ];
  const edge = (e: MouseEvent): ResizeDirection | null => {
    if (root.hasAttribute("data-maximized") || root.hasAttribute("data-fullscreen")) return null;
    const onColumn = side(e.clientX, window.innerWidth, EDGE) !== 0;
    const onRow = side(e.clientY, window.innerHeight, EDGE) !== 0;
    const vertical = side(e.clientY, window.innerHeight, onColumn ? CORNER : EDGE);
    const horizontal = side(e.clientX, window.innerWidth, onRow ? CORNER : EDGE);
    return DIRECTIONS[vertical + 1][horizontal + 1];
  };
  const CURSOR: Record<ResizeDirection, string> = { North: "ns-resize", South: "ns-resize", West: "ew-resize", East: "ew-resize", NorthWest: "nwse-resize", SouthEast: "nwse-resize", NorthEast: "nesw-resize", SouthWest: "nesw-resize" };
  const layer = document.createElement("div");
  layer.className = "window-edge";
  layer.hidden = true;
  root.append(layer);
  window.addEventListener("mousemove", (e) => {
    const direction = e.buttons ? null : edge(e);
    if (direction) layer.style.cursor = CURSOR[direction];
    layer.hidden = !direction;
  }, true);
  window.addEventListener("mouseout", (e) => {
    if (!e.relatedTarget) layer.hidden = true;
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

/** The application's icon at the bar's start, the product's name as its tip. */
function appMark(): HTMLElement {
  const mark = document.createElement("span");
  mark.className = "app-mark";
  mark.title = "LizzieYzy Tauri";
  mark.setAttribute("aria-hidden", "true");
  mark.setAttribute("data-tauri-drag-region", "");
  const img = document.createElement("img");
  img.src = "/assets/app-icon.png";
  img.alt = "";
  img.draggable = false;
  mark.append(img);
  return mark;
}

/** Adds the mark, the title, the drag region and, where the system does not draw them, the
 *  window buttons and the resize edges to the bar that already holds the menus. */
export async function mountWindowBar(host: HTMLElement): Promise<void> {
  bar = host;
  host.prepend(appMark());
  title = document.createElement("div");
  title.id = "windowTitle";
  titleGame.className = "title-game";
  titleDetail.className = "title-detail";
  title.append(titleGame, titleDetail);
  host.append(title);
  new ResizeObserver(layoutTitle).observe(title);
  // Pressing the bar's free space or the title moves the window; a double press maximizes it.
  host.toggleAttribute("data-tauri-drag-region", true);
  title.toggleAttribute("data-tauri-drag-region", true);
  if (os === "windows") host.append(windowButtons(["minimize", "maximize", "close"], false));
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
