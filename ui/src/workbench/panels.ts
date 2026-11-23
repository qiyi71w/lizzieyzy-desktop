// 界面 > 面板 / 工具栏 and their Java shortcuts (Menu.java:1162-1435, 4407-4591; Input.java:599-
// 646, 708-716, 767-775, 844-850), the in-frame separators (InFrameLeftoverDragHandles) and a
// panel arrangement's dividers, the X controls overlay (LizzieFrame.drawControls /
// stopShowingControl) and the window's 总在最前. The core owns the committed flags, sizes and
// arrangement (TogglePanel, SetPanelSettings, SetPanelSizes, NudgeBoardPosition,
// RestoreDefaultPanelSizes, SetPanelArrangement, SetBottomToolbar); a separator drag is a UI
// draft until the pointer is released.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { ArrangeablePanel, BottomToolbar, PanelArrangement, PanelSettings, PanelSizes, WorkbenchPanel, WorkbenchView } from "../contract/generated";
import { commentEditing } from "../editing/comment";
import { openAnalysisSettings } from "../engine/settings";
import { t } from "../i18n";
import type { ArrangementDivider } from "./arrangement";
import type { Handles, LayoutRequest, Rect } from "./layout";
import { GRAPH_FONT, prepare } from "./paint";
import { listShown, presentation, statusShown } from "./presentation";
import { humanSlDisplay } from "../humansl/state";

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
    // An AI 陪练 game in 实战后复盘 hides the win-rate graph for the game only
    // (HumanSlGameController.applyTrainingModeVisualState: showWinrateGraph = false).
    showWinrateGraph: p.showWinrateGraph && humanSlDisplay(v) !== "Suppress",
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
 *  hide it (the combined row's controls are not added to the menu bar in the fixed build). The
 *  user's choice also ends a reveal by function search, at once: a choice equal to the saved one
 *  publishes no view. A choice that could not be saved leaves the toolbar as saved. */
async function setTopToolbar(visible: boolean, separated: boolean): Promise<void> {
  topRevealed = false;
  const p = panels();
  if (!p) return;
  const settings = { ...p, showTopToolbar: visible, showDoubleMenu: separated };
  showTopToolbar(settings);
  if (await run({ type: "SetPanelSettings", settings }) === null) {
    const saved = panels();
    if (saved) showTopToolbar(saved);
  }
}

/** btnDoubleMenu (Menu.java:7542-7549): flips 分开 / 合并, the same setting as the menu's. The row
 *  it sits in is shown, also when function search showed a toolbar saved 不显示 (locateTopFunction
 *  sets the running showTopToolBar), so the toolbar stays on. */
export function toggleDoubleMenu(): void {
  const p = panels();
  if (p) void setTopToolbar(true, !topSeparated(p));
}

/** Function search showed the top toolbar 分开 for this run (Menu.locateTopFunction changes the
 *  running settings only); the saved choice stays as it is. */
let topRevealed = false;

function topSeparated(p: PanelSettings | undefined): boolean {
  return topRevealed || (!!p && p.showTopToolbar && p.showDoubleMenu);
}

function showTopToolbar(p: PanelSettings): void {
  const top = $("toolbar");
  const display = topSeparated(p) ? "" : "none";
  if (top.style.display !== display) top.style.display = display;
}

function setBottomToolbar(mode: BottomToolbar): Promise<unknown> {
  return run({ type: "SetBottomToolbar", mode });
}

/** Shows a toolbar so one of its controls can be located, as Menu.locateTopFunction
 *  (Menu.java:6979-7014: the top toolbar becomes 分开) and BottomToolbar.locateFunction
 *  (BottomToolbar.java:632-643: a hidden bottom toolbar takes its main-bar height) do. Resolves
 *  once the toolbar is laid out; the destination's value and action are untouched. */
export async function revealToolbar(kind: "top" | "bottom"): Promise<void> {
  const p = panels();
  if (kind === "top") {
    if (p && !topSeparated(p)) {
      topRevealed = true;
      showTopToolbar(p);
    }
  } else if (state.view?.settings.bottom_toolbar === "Hidden") await setBottomToolbar("Simple");
  // The bar is laid out when the view with the new mode arrives, which may follow the command's
  // reply; give it up to about a second.
  for (let n = 0; n < 60; n++) {
    const frame = Promise.withResolvers<void>();
    requestAnimationFrame(() => frame.resolve());
    await frame.promise;
    if (kind === "top" || state.view?.settings.bottom_toolbar === "Simple") break;
  }
}

let appliedOnTop: boolean | null = null;
registerViewHandler((v) => {
  const p = v.settings.panels;
  // LizzieFrame.toggleAlwaysOntop / ConfigDialog2 保存设置: the main window follows the setting.
  if (p.alwaysOnTop !== appliedOnTop) {
    appliedOnTop = p.alwaysOnTop;
    void getCurrentWindow().setAlwaysOnTop(p.alwaysOnTop);
  }
  showTopToolbar(p);
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
  "menu.topToolBar.combined": { run: () => void setTopToolbar(true, false), checked: () => !topRevealed && !!panels()?.showTopToolbar && !panels()?.showDoubleMenu },
  // 不显示 keeps (or restores) the double-menu flag (Menu.java:4440-4453).
  "menu.topToolBar.invisible": { run: () => void setTopToolbar(false, true), checked: () => !topRevealed && !!panels() && !panels()!.showTopToolbar },
  // 工具栏 > 底部工具栏 (Menu.java:4532-4591, syncBottomToolbarMenuMarks 10628-10654).
  "menu.bottomToolBar.visible": { run: () => void setBottomToolbar("Simple"), checked: () => state.view?.settings.bottom_toolbar === "Simple" },
  "menu.bottomToolBar.invisible": { run: () => void setBottomToolbar("Hidden"), checked: () => state.view?.settings.bottom_toolbar === "Hidden" },
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

// The four Java zones (InFrameLeftoverDragHandles) and, while a panel arrangement applies, the
// dividers between neighbouring panels of a column (arrangement.ts) share one drag: a press
// captures the pointer, moving changes only a UI draft, releasing hands the draft over once, and
// Escape, a lost pointer or leaving the window cancel it.

type Seam = "left" | "right" | "commentTop" | "variationList";
const SEAMS: Seam[] = ["left", "right", "commentTop", "variationList"];
/** The zones' elements by `data-name`. */
const handleEls = new Map<string, HTMLElement>();

interface Drag {
  el: HTMLElement;
  pointer: number;
  divider: boolean;
  /** Shows the draft for the pointer (workbench coordinates). */
  move(p: { x: number; y: number }): void;
  release(): Promise<void>;
  cancel(): void;
}
let drag: Drag | null = null;

const clamp01 = (n: number) => Math.max(0, Math.min(1, n));

function pointerIn(e: PointerEvent): { x: number; y: number } {
  const r = $("main").getBoundingClientRect();
  return { x: e.clientX - r.left, y: e.clientY - r.top };
}

async function finishDrag(commit: boolean): Promise<void> {
  if (!drag) return;
  const d = drag;
  drag = null;
  if (d.el.hasPointerCapture(d.pointer)) d.el.releasePointerCapture(d.pointer);
  if (commit) await d.release();
  else d.cancel();
  repaint();
}

function cancelDrag(): void {
  void finishDrag(false);
}

/** A zone element; `press` starts its drag or returns null to ignore the press. */
function createHandle(name: string, kind: "edge" | "seam" | "divider", press: (e: PointerEvent) => Omit<Drag, "el" | "pointer"> | null): HTMLElement {
  const el = document.createElement("div");
  el.className = `panel-handle ${kind}`;
  el.dataset.name = name;
  el.addEventListener("pointerdown", (e) => {
    if (e.button !== 0 || drag || !state.view) return;
    const d = press(e);
    if (!d) return;
    e.preventDefault();
    el.setPointerCapture(e.pointerId);
    drag = { ...d, el, pointer: e.pointerId };
  });
  el.addEventListener("pointermove", (e) => {
    if (drag?.el !== el || drag.pointer !== e.pointerId) return;
    drag.move(pointerIn(e));
    repaint();
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

/** Puts the zone `name` at `r`, creating it on first use; hides it when `r` is null. */
function placeZone(name: string, r: Rect | null, create: () => HTMLElement): void {
  let el = handleEls.get(name);
  if (!el) {
    if (!r) return;
    el = create();
    handleEls.set(name, el);
    $("main").append(el);
  }
  el.style.display = r ? "" : "none";
  if (r) Object.assign(el.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
}

// ---- the Java zones

/** The geometry the zones were placed for; a press keeps it (the zones stay put while dragging,
 *  as update() returns early). */
let lastHandles: Handles | null = null;

/** mouseDragged: applyLeftoverShare / applyCommentHeightShare / applyVariationGraphShare
 *  (LizzieFrame.java:7969-7998), shown at once and not yet saved; mouseReleased commits the
 *  drag's sizes (commitLeftoverShare and its siblings). A saved drag stays drawn until the
 *  published view carries it (see the view handler below). */
function pressSeam(seam: Seam, e: PointerEvent): Omit<Drag, "el" | "pointer"> | null {
  const at = lastHandles;
  if (!at) return null;
  const p = pointerIn(e);
  let grab: number;
  if (seam === "left" || seam === "right") {
    if (at.leftoverWidth <= 0) return null;
    grab = seam === "left" ? p.x - at.boardX : p.x - (at.boardX + at.boardWidth);
  } else {
    if (seam === "commentTop" ? at.commentSplitH <= 0 : at.variationContainerH <= 0) return null;
    grab = p.y - (seam === "commentTop" ? at.commentSeamY : at.variationSeamY);
  }
  const base = { ...state.view!.settings.panel_sizes };
  return {
    divider: false,
    move(p) {
      if (seam === "left" || seam === "right") {
        const nextBoardX = seam === "left" ? p.x - grab : p.x - grab - at.boardWidth;
        const share = clamp01(nextBoardX / at.leftoverWidth);
        draft = { ...base, leftoverLeftShare: share, boardPositionProportion: Math.max(0, Math.min(8, Math.round(share * 8))) };
      } else {
        const seamY = p.y - grab;
        if (seam === "commentTop") draft = { ...base, commentHeightShare: clamp01((at.commentSplitBottom - seamY) / at.commentSplitH) };
        else draft = { ...base, variationGraphShare: clamp01((seamY - at.variationTop) / at.variationContainerH) };
      }
    },
    async release() {
      const sizes = draft;
      if (sizes) {
        const result = await run({ type: "SetPanelSizes", sizes });
        if (result && result.type !== "Refused" && draft === sizes && JSON.stringify(state.view?.settings.panel_sizes) !== JSON.stringify(sizes)) return;
      }
      if (!drag) draft = null;
    },
    cancel() {
      draft = null;
    },
  };
}

registerViewHandler((v) => {
  if (!drag && draft && JSON.stringify(v.settings.panel_sizes) === JSON.stringify(draft)) draft = null;
});

// ---- the arrangement's dividers

/** Who a divider drag works for: the arrangement a divider moved to `y` gives (from the one
 *  drawn, at the size drawn), or null when that is no change (the layout drawn stays), and what
 *  releasing does with it, resolving true when it was committed so the draft stays drawn until a
 *  published view carries it. Normal use commits (`commitArrangement`); an edit session keeps the
 *  result in its own draft. */
export interface DividerOwner {
  move(upper: ArrangeablePanel, y: number): PanelArrangement | null;
  release(next: PanelArrangement): Promise<boolean>;
}

/** A divider drag's arrangement while the pointer is held (or until the view carries it). */
let arrangementDraft: PanelArrangement | null = null;

export function dividerDraft(): PanelArrangement | null {
  return arrangementDraft;
}

const sameArrangement = (a: PanelArrangement | null | undefined, b: PanelArrangement | null) => JSON.stringify(a ?? null) === JSON.stringify(b);

/** Releasing a divider outside an edit session: one SetPanelArrangement. */
export async function commitArrangement(next: PanelArrangement): Promise<boolean> {
  const result = await run({ type: "SetPanelArrangement", arrangement: next });
  return !!result && result.type !== "Refused";
}

let lastDividers: { dividers: ArrangementDivider[]; owner: DividerOwner } | null = null;

function pressDivider(name: string, e: PointerEvent): Omit<Drag, "el" | "pointer"> | null {
  const placed = lastDividers;
  const divider = placed?.dividers.find((d) => d.name === name);
  if (!placed || !divider) return null;
  const { owner } = placed;
  const grab = pointerIn(e).y - divider.y;
  return {
    divider: true,
    move(p) {
      arrangementDraft = owner.move(divider.upper, p.y - grab);
    },
    async release() {
      const next = arrangementDraft;
      if (next && (await owner.release(next)) && arrangementDraft === next && !sameArrangement(state.view?.settings.panel_arrangement, next)) return;
      if (!drag) arrangementDraft = null;
    },
    cancel() {
      arrangementDraft = null;
    },
  };
}

registerViewHandler((v) => {
  if (!drag && arrangementDraft && sameArrangement(v.settings.panel_arrangement, arrangementDraft)) arrangementDraft = null;
});

/** Places the zones for the layout just drawn: the Java zones `h` (none outside the locked
 *  layout) and, while an arrangement applies, its `dividers` worked by `owner`. While a drag is
 *  held the zones stay put; a divider drag ends uncommitted once the arrangement no longer
 *  applies (another mode, or a window it does not fit). */
export function placeHandles(h: Handles | null, dividers: ArrangementDivider[] | null, owner: DividerOwner | null): void {
  if (drag) {
    if (drag.divider && !(dividers && owner)) cancelDrag();
    return;
  }
  lastHandles = h;
  lastDividers = dividers && owner ? { dividers, owner } : null;
  for (const seam of SEAMS) {
    const name = `panel-handle-${seam}`;
    placeZone(name, h ? h[seam] : null, () => createHandle(name, seam === "left" || seam === "right" ? "edge" : "seam", (e) => pressSeam(seam, e)));
  }
  const current = new Set(lastDividers?.dividers.map((d) => d.name));
  for (const name of handleEls.keys()) if (name.startsWith("panel-divider-") && !current.has(name)) placeZone(name, null, () => handleEls.get(name)!);
  for (const d of lastDividers?.dividers ?? []) placeZone(d.name, d.zone, () => createHandle(d.name, "divider", (e) => pressDivider(d.name, e)));
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
