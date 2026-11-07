// 界面 > 面板 / 工具栏 and their Java shortcuts (Menu.java:1162-1435, 4407-4591; Input.java:599-
// 646, 708-716, 767-775, 844-850), the in-frame separators (InFrameLeftoverDragHandles), the X
// controls overlay (LizzieFrame.drawControls / stopShowingControl) and the window's 总在最前.
// The core owns the committed flags and sizes (TogglePanel, SetPanelSettings, SetPanelSizes,
// NudgeBoardPosition, RestoreDefaultPanelSizes, SetBottomToolbar); a separator drag is a UI draft
// until the pointer is released.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { BottomToolbar, PanelSettings, PanelSizes, WorkbenchPanel, WorkbenchView } from "../contract/generated";
import { ask } from "../dialogs";
import { commentEditing } from "../editing/comment";
import { openAnalysisSettings } from "../engine/settings";
import { product, t } from "../i18n";
import type { Handles, LayoutRequest, Rect } from "./layout";
import { GRAPH_FONT, prepare } from "./paint";
import { listShown, presentation, statusShown } from "./presentation";

const $ = (id: string) => document.getElementById(id) as HTMLElement;

function panels(): PanelSettings | undefined {
  return state.view?.settings.panels;
}

function toggle(panel: WorkbenchPanel): void {
  void run({ type: "TogglePanel", panel });
}

/** The sizes shown: a separator drag's draft while the pointer is held, else the committed ones. */
let draft: PanelSizes | null = null;

/** What the in-frame layout is computed from (LizzieFrame.java:7199-7228), with the open
 *  SetCustomMode dialog's draft when there is one. */
export function layoutRequest(v: WorkbenchView): LayoutRequest {
  const shown = presentation(v);
  const p = shown.panels;
  const committed = draft ?? v.settings.panel_sizes;
  // The dialog's 主棋盘位置 slider sets the proportion (setBoardPositionProportion); a dragged fine
  // share moves to that position's own share, proportion / 8 (leftoverShareAfterAssignedProportion).
  const moved = shown.boardPositionProportion;
  const sizes = moved === committed.boardPositionProportion
    ? committed
    : { ...committed, boardPositionProportion: moved, leftoverLeftShare: committed.leftoverLeftShare === null ? null : moved / 8 };
  return {
    cols: v.document.size.width,
    rows: v.document.size.height,
    showComment: p.showComment,
    showSubBoard: p.showSubBoard,
    showVariationGraph: p.showVariationGraph,
    showListPane: listShown(shown.mode, p),
    showStatus: statusShown(shown.mode, p),
    showCaptured: p.showCaptured,
    showWinrateGraph: p.showWinrateGraph,
    largeSubBoard: p.largeSubBoard,
    largeWinrateGraph: p.largeWinrateGraph,
    boardPositionProportion: sizes.boardPositionProportion,
    leftoverLeftShare: sizes.leftoverLeftShare,
    commentHeightShare: sizes.commentHeightShare,
    variationGraphShare: sizes.variationGraphShare,
  };
}

// ------------------------------------------------------------------ toolbars

/** 界面 > 工具栏 > 顶部工具栏 (Menu.java:4409-4455): 分开 shows the double-menu row, 合并 and 不显示
 *  hide it (the combined row's controls are not added to the menu bar in the fixed build). */
function setTopToolbar(showTopToolbar: boolean, showDoubleMenu: boolean): Promise<unknown> {
  const p = panels();
  if (!p) return Promise.resolve();
  return run({ type: "SetPanelSettings", settings: { ...p, showTopToolbar, showDoubleMenu } });
}

function topSeparated(p: PanelSettings | undefined): boolean {
  return !!p && p.showTopToolbar && p.showDoubleMenu;
}

function setBottomToolbar(mode: BottomToolbar): Promise<unknown> {
  return run({ type: "SetBottomToolbar", mode });
}

/** 使用详细工具栏 (Menu.java:4561-4591): turning it on asks first; turning it off does not. */
async function toggleDetailedToolbar(): Promise<void> {
  if (state.view?.settings.bottom_toolbar === "Detailed") {
    await setBottomToolbar("Simple");
    return;
  }
  const body = document.createElement("p");
  body.textContent = "详细工具栏已过时,其中的内容可以在顶部/底部工具栏中找到,从详细工具栏发起的功能也已过时,例如自动分析不能分析所有分支,确定启用详细工具栏?";
  if (await ask("确定使用详细工具栏?", body, product("yes"), product("no"))) await setBottomToolbar("Detailed");
}

/** Shows a toolbar so one of its controls can be located, as Menu.locateTopFunction
 *  (Menu.java:6979-7014: the top toolbar becomes 分开) and BottomToolbar.locateFunction
 *  (BottomToolbar.java:632-643: a hidden bottom toolbar takes its main-bar height) do. Resolves
 *  once the toolbar is laid out; the destination's value and action are untouched. */
export async function revealToolbar(kind: "top" | "bottom"): Promise<void> {
  if (kind === "top") {
    if (!topSeparated(panels())) await setTopToolbar(true, true);
  } else if (state.view?.settings.bottom_toolbar === "Hidden") await setBottomToolbar("Simple");
  const frame = Promise.withResolvers<void>();
  requestAnimationFrame(() => frame.resolve());
  await frame.promise;
}

let appliedOnTop: boolean | null = null;
registerViewHandler((v) => {
  const p = v.settings.panels;
  // LizzieFrame.toggleAlwaysOntop / ConfigDialog2 保存设置: the main window follows the setting.
  if (p.alwaysOnTop !== appliedOnTop) {
    appliedOnTop = p.alwaysOnTop;
    void getCurrentWindow().setAlwaysOnTop(p.alwaysOnTop);
  }
  const top = $("toolbar");
  const display = topSeparated(p) ? "" : "none";
  if (top.style.display !== display) top.style.display = display;
});

// ------------------------------------------------------------------ menus

function flag(panel: WorkbenchPanel, checked: (p: PanelSettings, v: WorkbenchView) => boolean): CommandEntry {
  return { run: () => toggle(panel), checked: () => (state.view ? checked(state.view.settings.panels, state.view) : false) };
}

Object.assign(COMMANDS, {
  // 界面 > 面板, checked as the menu listener sets them (Menu.java:2785-2799).
  "menu.subboard": flag("SubBoard", (p) => p.showSubBoard),
  "menu.winrateGraph": flag("WinrateGraph", (p) => p.showWinrateGraph),
  "menu.commitPane": {
    run: () => toggle("Comment"),
    // isCommentPanelAutoHiddenByMode: disabled and unchecked in 双引擎模式.
    enabled: () => !state.view?.settings.comment_panel_auto_hidden,
    checked: () => !!state.view && !state.view.settings.comment_panel_auto_hidden && state.view.settings.panels.showComment,
  },
  "menu.variationPane": flag("VariationGraph", (p) => p.showVariationGraph),
  "menu.listPane": flag("ListPane", (p, v) => listShown(v.settings.extra_mode, p)),
  "menu.informationPane": flag("Captured", (p) => p.showCaptured),
  // 经典模式 turns the status off for the session only (Config.showStatus, Menu.java:2797).
  "menu.statusPanel": flag("Status", (p) => p.showStatus && !p.classicMode),
  "menu.restoreDefaultPanelSizes": { run: () => void run({ type: "RestoreDefaultPanelSizes" }) },
  // 界面 > 放大小棋盘 / 放大胜率图 / 总在最前 (Menu.java:966-986, 1027-1036, 2768-2782).
  "menu.largeSubBoard": flag("LargeSubBoard", (p) => p.showSubBoard && p.largeSubBoard),
  "menu.largeWinrateGraph": flag("LargeWinrateGraph", (p) => p.showWinrateGraph && p.largeWinrateGraph),
  "menu.alwaysOnTop": flag("AlwaysOnTop", (p) => p.alwaysOnTop),
  // 界面 > 主棋盘位置 (Menu.java:555-581).
  "Menu.leftMove": { run: () => void run({ type: "NudgeBoardPosition", delta: -1 }) },
  "Menu.rightMove": { run: () => void run({ type: "NudgeBoardPosition", delta: 1 }) },
  // 工具栏 > 顶部工具栏, marked as its menu listener does (Menu.java:4476-4494).
  "menu.topToolBar.separated": { run: () => void setTopToolbar(true, true), checked: () => topSeparated(panels()) },
  "menu.topToolBar.combined": { run: () => void setTopToolbar(true, false), checked: () => !!panels()?.showTopToolbar && !panels()?.showDoubleMenu },
  // 不显示 keeps (or restores) the double-menu flag (Menu.java:4440-4453).
  "menu.topToolBar.invisible": { run: () => void setTopToolbar(false, true), checked: () => !!panels() && !panels()!.showTopToolbar },
  // 工具栏 > 底部工具栏 (Menu.java:4532-4591, syncBottomToolbarMenuMarks 10628-10654).
  "menu.bottomToolBar.visible": { run: () => void setBottomToolbar("Simple"), checked: () => state.view?.settings.bottom_toolbar === "Simple" },
  "menu.bottomToolBar.invisible": { run: () => void setBottomToolbar("Hidden"), checked: () => state.view?.settings.bottom_toolbar === "Hidden" },
  "toolbar.detailed": { run: () => void toggleDetailedToolbar(), checked: () => state.view?.settings.bottom_toolbar === "Detailed" },
} satisfies Record<string, CommandEntry>);

// ------------------------------------------------------------------ keys

/** Panel keys of Input.keyPressed in Java's modifier precedence; Ctrl includes Command where Java
 *  uses controlIsPressed. Other modifier combinations of these keys belong to other features. */
export function handlePanelKey(e: KeyboardEvent): boolean {
  const ctrl = e.ctrlKey || e.metaKey;
  switch (e.code) {
    case "KeyZ":
      // VK_Z tests isControlDown (Input.java:709): Command+Z stays the temporary board.
      if (e.ctrlKey) toggle("AlwaysOnTop");
      else if (e.altKey) toggle("SubBoard");
      else return false;
      return true;
    case "KeyW":
      // VK_W: Ctrl+W 大胜率图, Alt+W 胜率图; W alone does nothing.
      if (ctrl) toggle("LargeWinrateGraph");
      else if (e.altKey) toggle("WinrateGraph");
      return true;
    case "KeyG":
      if (e.altKey) toggle("ListPane");
      else if (e.shiftKey) toggle("VariationGraph");
      else return false;
      return true;
    case "KeyT":
      // Ctrl+T (comment node colour) comes first.
      if (ctrl || !e.altKey) return false;
      toggle("Comment");
      return true;
    case "KeyF":
      // Shift+F is the variation display; plain F the candidates.
      if (e.shiftKey || !ctrl) return false;
      toggle("LargeSubBoard");
      return true;
    case "BracketLeft":
    case "BracketRight":
      void run({ type: "NudgeBoardPosition", delta: e.code === "BracketLeft" ? -1 : 1 });
      return true;
    case "KeyX":
      // Shift+X 综合设置 at its first page (openConfigDialog2(0)); Alt+X is the engine list.
      if (e.shiftKey) void openAnalysisSettings("display");
      else if (e.altKey) return false;
      else showControls();
      return true;
  }
  return false;
}

// ------------------------------------------------------------------ X: controls overlay

/** LizzieFrame.commands (LizzieFrame.java:141-196), "key|action". */
function commandLines(): string[] {
  return [
    t("LizzieFrame.commands.keySpace"),
    t("LizzieFrame.commands.keyN"),
    t("LizzieFrame.commands.keyEnter"),
    t("LizzieFrame.commands.mouseWheelScroll"),
    t("LizzieFrame.commands.keyComma"),
    t("LizzieFrame.commands.rightClick"),
    t("LizzieFrame.commands.keyA"),
    t("LizzieFrame.commands.keyG"),
    t("LizzieFrame.commands.keyR"),
    t("LizzieFrame.commands.mousePointSub"),
    t("LizzieFrame.commands.keyY"),
    t("LizzieFrame.commands.keyU"),
    t("LizzieFrame.commands.keyI"),
    t("LizzieFrame.commands.keySlash"),
    t("LizzieFrame.commands.keyB"),
    t("LizzieFrame.commands.keyV"),
    t("LizzieFrame.commands.keyF"),
    t("LizzieFrame.commands.keyZ"),
    t("LizzieFrame.commands.keyShiftF"),
    t("LizzieFrame.commands.keyHandY"),
    t("LizzieFrame.commands.key123456789"),
    t("LizzieFrame.commands.keyUpDownArrow"),
    t("LizzieFrame.commands.keyC"),
    t("LizzieFrame.commands.keyP"),
    t("LizzieFrame.commands.keyM"),
    t("LizzieFrame.commands.keyAltC"),
    t("LizzieFrame.commands.keyAltV"),
    t("LizzieFrame.commands.keyJ"),
    t("LizzieFrame.commands.keyW"),
    t("LizzieFrame.commands.keyCtrlW"),
    t("LizzieFrame.commands.keyShiftG"),
    t("LizzieFrame.commands.keyAltZ"),
    t("LizzieFrame.commands.keyBracket"),
    t("LizzieFrame.commands.keyCtrlT"),
    t("LizzieFrame.commands.keyHome"),
    t("LizzieFrame.commands.keyEnd"),
    t("LizzieFrame.commands.keyControl"),
    t("LizzieFrame.commands.keyDelete"),
    t("LizzieFrame.commands.keyE"),
  ];
}

let controls: HTMLCanvasElement | null = null;
let repaint: () => void = () => {};

/** Whether X is held: the variation tree, the list and the comment are hidden meanwhile. */
export function controlsShowing(): boolean {
  return controls !== null && controls.style.display !== "none";
}

/** drawControls (LizzieFrame.java:8616-8689): the key list centred over the workbench. */
function showControls(): void {
  if (controlsShowing()) return;
  const main = $("main");
  if (!controls) {
    controls = document.createElement("canvas");
    controls.id = "controls";
    main.append(controls);
  }
  const W = main.clientWidth;
  const H = main.clientHeight;
  controls.style.display = "";
  const g = prepare(controls, { x: 0, y: 0, w: W, h: H });
  const lines = commandLines();
  const i = Math.trunc;
  const fontSize = i(H / 1.2 / lines.length);
  g.font = `${fontSize}px ${GRAPH_FONT}`;
  const maxWidth = Math.max(0, ...lines.map((line) => g.measureText(line).width));
  const lineHeight = i(fontSize * 1.22);
  const boxWidth = Math.min(i(maxWidth * 1.4), W);
  const boxHeight = Math.min(lines.length * lineHeight, H);
  const x = Math.min(i(W / 2) - i(boxWidth / 2), W);
  const y = Math.min(i(H / 2) - i(boxHeight / 2), H);
  g.fillStyle = "rgba(0, 0, 0, 0.51)";
  g.fillRect(x, y, boxWidth, boxHeight);
  const lineX = i(x + boxWidth * 0.3);
  g.strokeStyle = "rgba(0, 0, 0, 0.235)";
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(lineX + 0.5, y + 2);
  g.lineTo(lineX + 0.5, y + boxHeight - 2);
  g.stroke();
  g.fillStyle = "#fff";
  g.textBaseline = "alphabetic";
  let offset = y;
  for (const line of lines) {
    const [key, action = ""] = line.split("|");
    g.fillText(key, lineX - g.measureText(key).width - 4, fontSize + offset);
    g.fillText(action, lineX + 4, fontSize + offset);
    offset += lineHeight;
  }
  repaint();
}

/** stopShowingControl (LizzieFrame.java:15185-15198): releasing X, or leaving the window. */
function hideControls(): void {
  if (!controlsShowing()) return;
  controls!.style.display = "none";
  repaint();
}

document.addEventListener("keyup", (e) => {
  if (e.code === "KeyX") hideControls();
});
window.addEventListener("blur", hideControls);

// ------------------------------------------------------------------ separators

type Seam = "left" | "right" | "commentTop" | "variationList";
const SEAMS: Seam[] = ["left", "right", "commentTop", "variationList"];
const handleEls = new Map<Seam, HTMLElement>();
/** The press: its seam, the geometry it was made on (the zones stay put while dragging, as
 *  update() returns early) and the committed sizes the draft starts from. */
let drag: { seam: Seam; at: Handles; base: PanelSizes; grab: number; pointer: number; el: HTMLElement } | null = null;

const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

function pointerIn(e: PointerEvent): { x: number; y: number } {
  const r = $("main").getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

/** mouseDragged: applyLeftoverShare / applyCommentHeightShare / applyVariationGraphShare
 *  (LizzieFrame.java:7969-7998), shown at once and not yet saved. */
function dragTo(e: PointerEvent): void {
  if (!drag) return;
  const { seam, at, base, grab } = drag;
  const p = pointerIn(e);
  if (seam === "left" || seam === "right") {
    const nextBoardX = seam === "left" ? p.x - grab : p.x - grab - at.boardWidth;
    const share = clamp01(nextBoardX / at.leftoverWidth);
    draft = { ...base, leftoverLeftShare: share, boardPositionProportion: Math.max(0, Math.min(8, Math.round(share * 8))) };
  } else {
    const seamY = p.y - grab;
    if (seam === "commentTop") draft = { ...base, commentHeightShare: clamp01((at.commentSplitBottom - seamY) / at.commentSplitH) };
    else draft = { ...base, variationGraphShare: clamp01((seamY - at.variationTop) / at.variationContainerH) };
  }
  repaint();
}

/** mouseReleased commits the drag's sizes (commitLeftoverShare and its siblings); a cancelled
 *  drag (Escape, a lost pointer, leaving the window) keeps the committed ones. A saved drag stays
 *  drawn until the published view carries it (see the view handler below). */
async function finishDrag(commit: boolean): Promise<void> {
  if (!drag) return;
  const { el, pointer } = drag;
  drag = null;
  if (el.hasPointerCapture(pointer)) el.releasePointerCapture(pointer);
  const sizes = draft;
  if (commit && sizes) {
    const result = await run({ type: "SetPanelSizes", sizes });
    if (result && result.type !== "Refused" && draft === sizes && JSON.stringify(state.view?.settings.panel_sizes) !== JSON.stringify(sizes)) return;
  }
  if (!drag) draft = null;
  repaint();
}

registerViewHandler((v) => {
  if (!drag && draft && JSON.stringify(v.settings.panel_sizes) === JSON.stringify(draft)) draft = null;
});

function cancelDrag(): void {
  void finishDrag(false);
}

function createHandle(seam: Seam): HTMLElement {
  const el = document.createElement("div");
  el.className = `panel-handle ${seam === "left" || seam === "right" ? "edge" : "seam"}`;
  el.dataset.name = `panel-handle-${seam}`;
  el.addEventListener("pointerdown", (e) => {
    const at = lastHandles;
    if (e.button !== 0 || !at || drag) return;
    const p = pointerIn(e);
    let grab: number;
    if (seam === "left" || seam === "right") {
      if (at.leftoverWidth <= 0) return;
      grab = seam === "left" ? p.x - at.boardX : p.x - (at.boardX + at.boardWidth);
    } else {
      if (seam === "commentTop" ? at.commentSplitH <= 0 : at.variationContainerH <= 0) return;
      grab = p.y - (seam === "commentTop" ? at.commentSeamY : at.variationSeamY);
    }
    const v = state.view;
    if (!v) return;
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    drag = { seam, at, base: { ...v.settings.panel_sizes }, grab, pointer: e.pointerId, el };
  });
  el.addEventListener("pointermove", (e) => {
    if (drag?.el === el && drag.pointer === e.pointerId) dragTo(e);
  });
  el.addEventListener("pointerup", (e) => {
    if (drag?.el === el && drag.pointer === e.pointerId) void finishDrag(true);
  });
  el.addEventListener("pointercancel", cancelDrag);
  el.addEventListener("lostpointercapture", () => {
    if (drag?.el === el) cancelDrag();
  });
  return el;
}

document.addEventListener("keydown", (e) => {
  if (drag && e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    cancelDrag();
  }
}, true);
window.addEventListener("blur", cancelDrag);

let lastHandles: Handles | null = null;

/** Places the drag zones for the layout just drawn; none outside the locked layout. */
export function placeHandles(h: Handles | null): void {
  if (drag) return;
  lastHandles = h;
  const main = $("main");
  for (const seam of SEAMS) {
    let el = handleEls.get(seam);
    const r: Rect | null = h ? h[seam] : null;
    if (!el) {
      if (!r) continue;
      el = createHandle(seam);
      handleEls.set(seam, el);
      main.append(el);
    }
    el.style.display = r ? "" : "none";
    if (r) Object.assign(el.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
  }
}

// ------------------------------------------------------------------ focus

/** LizzieFrame.onMouseMoved and the tree / list mouseMoved listeners (LizzieFrame.java:887-898,
 *  10033-10035): moving over the board, the variation tree or the list gives the workbench the
 *  keyboard back, except while a comment is being edited. */
function refocus(): void {
  if (commentEditing()) return;
  const active = document.activeElement;
  if (active instanceof HTMLElement && active !== document.body && !active.closest(".modal")) active.blur();
}

/** Connects the workbench: `schedule` repaints it. */
export function bindPanels(schedule: () => void): void {
  repaint = schedule;
  for (const id of ["board", "board2", "tree", "table"]) $(id).addEventListener("mousemove", refocus);
}
