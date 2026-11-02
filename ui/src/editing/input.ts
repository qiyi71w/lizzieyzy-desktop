// Board mouse and record-editing keys (Input.java mousePressed / mouseReleased / mouseDragged /
// mouseMoved and keyPressed, LizzieFrame onClicked / onClickedRight / openRightClickMenu /
// DraggedReleased). Every document change goes through the Edit command or a catalog entry.
import { navigate, state, unavailable } from "../app";
import { COMMANDS } from "../commands";
import type { Point, Stone } from "../contract/generated";
import { t } from "../i18n";
import { commitComment } from "./comment";
import { contextMenuShowing, openContextMenu, type ContextEntry } from "./contextMenu";
import { deleteWithPrompt, edit, editing } from "./state";

/** What the workbench knows about the board panel. */
export interface BoardHost {
  /** The intersection under a client point, or null off the grid. */
  pointAt(clientX: number, clientY: number): Point | null;
  /** Client-space centre and stone radius of an intersection. */
  stoneAt(point: Point): { x: number; y: number; r: number } | null;
  /** Moves of the suggestion variation previewed on the main board (BoardRenderer
   *  hasSelectedVariation), passes skipped, or null when none is shown. */
  previewMoves(): Point[] | null;
  /** A suggestion clicked in the candidate table is cleared by a right click
   *  (openRightClickMenu `clickOrder != -1`); true when there was one. */
  clearTableSelection(): boolean;
}

function command(id: string): void {
  const entry = COMMANDS[id];
  if (entry) entry.run();
  else unavailable();
}

function stoneOn(point: Point): Stone | null {
  const v = state.view;
  if (!v) return null;
  return v.position.stones[point.y * v.document.size.width + point.x] ?? null;
}

// ------------------------------------------------------------------ moving a stone

/** A stone being moved: pressed with 启用拖动棋子 (Input.Draggedmode, follows the drag and ends
 *  on release) or picked from 移动 (Input.tempDrag, follows the pointer and ends on the next
 *  press). `at` is the document, revision and node it was taken from: once any of them changes
 *  (another document, an edit, navigation) the release moves nothing. */
let moving: { from: Point; color: Stone; picked: boolean; at: string } | null = null;
let ghost: HTMLImageElement | null = null;
let host: BoardHost;

/** DraggedDragged: the moved stone is drawn on the intersection under the pointer. */
function showGhost(clientX: number, clientY: number): void {
  if (!moving) return;
  const point = host.pointAt(clientX, clientY);
  const at = point && host.stoneAt(point);
  if (!at) return;
  if (!ghost) {
    ghost = document.createElement("img");
    ghost.className = "move-stone-ghost";
    ghost.alt = "";
    document.body.append(ghost);
  }
  const src = `/assets/${moving.color === "Black" ? "black0.png" : "white0.png"}`;
  if (ghost.getAttribute("src") !== src) ghost.src = src;
  ghost.style.left = `${at.x - at.r}px`;
  ghost.style.top = `${at.y - at.r}px`;
  ghost.style.width = ghost.style.height = `${at.r * 2 + 1}px`;
}

/** DraggedReleased (LizzieFrame.java:11710): onto another empty intersection the stone moves;
 *  the same point, an occupied one or off the board leaves the record unchanged. */
function finishMove(point: Point | null): void {
  const m = moving;
  moving = null;
  ghost?.remove();
  ghost = null;
  if (!m || !point || m.at !== positionKey() || (point.x === m.from.x && point.y === m.from.y) || stoneOn(point)) return;
  void edit({ op: "EditStone", point: m.from, change: { kind: "MoveTo", point } });
}

function positionKey(): string {
  const v = state.view;
  return v ? `${v.document.id}:${v.document.revision}:${v.position.node}` : "";
}

function startPick(point: Point): void {
  const color = stoneOn(point);
  if (color) moving = { from: point, color, picked: true, at: positionKey() };
}

// ------------------------------------------------------------------ context menus

/** RightClickMenu (empty intersection), Java order with its visibility outside engine games;
 *  the analysis-region items are not migrated yet and stay disabled. */
function pointMenu(point: Point): ContextEntry[] {
  const v = state.view!;
  const preview = host.previewMoves();
  const entries: ContextEntry[] = [
    { key: "RightClickMenu.allow", label: t("RightClickMenu.allow"), icon: "setpoint.png" },
    { key: "RightClickMenu.avoid", label: t("RightClickMenu.avoid"), icon: "forbidpoint.png" },
    { key: "RightClickMenu.avoid2", label: t("RightClickMenu.avoid2"), checked: false },
    { key: "RightClickMenu.trackPoint", label: t("RightClickMenu.trackPoint"), icon: "trackingpoint.png" },
    "sep",
    { key: "RightClickMenu.previousMove", label: t("RightClickMenu.previousMove"), icon: "left.png", run: () => navigate({ op: "Back", count: 1 }) },
    { key: "RightClickMenu.findMove", label: t("RightClickMenu.findMove"), icon: "search.png", run: () => void edit({ op: "FindMove", point, any_branch: true }) },
    { key: "RightClickMenu.addblack", label: t("RightClickMenu.addblack"), icon: "smallblack1.png", run: () => void edit({ op: "Insert", point, color: "Black" }) },
    { key: "RightClickMenu.addwhite", label: t("RightClickMenu.addwhite"), icon: "smallwhite.png", run: () => void edit({ op: "Insert", point, color: "White" }) },
  ];
  if (v.editing.can_redo) entries.push({ key: "RightClickMenu.reedit", label: t("RightClickMenu.reedit"), icon: "right.png", run: () => void edit({ op: "RedoEdit" }) });
  if (v.editing.can_undo) entries.push({ key: "RightClickMenu.cleanupedit", label: t("RightClickMenu.cleanupedit"), icon: "left.png", run: () => void edit({ op: "UndoEdit" }) });
  if (v.editing.can_undo || v.editing.can_redo)
    entries.push({ key: "RightClickMenu.cleanedittemp", label: t("RightClickMenu.cleanedittemp"), icon: "recycle.png", run: () => void edit({ op: "ClearEditCache" }) });
  if (preview)
    entries.push({ key: "RightClickMenu.addSuggestionAsBranch", label: t("RightClickMenu.addSuggestionAsBranch"), run: () => void edit({ op: "AddVariationAsBranch", moves: preview }) });
  return entries;
}

/** RightClickMenu2 (a stone), Java order outside engine games. 回看 (click review) belongs to
 *  the display tickets and stays disabled. */
function stoneMenu(point: Point): ContextEntry[] {
  return [
    { key: "RightClickMenu2.moveStone", label: t("RightClickMenu2.moveStone"), run: () => startPick(point) },
    { key: "RightClickMenu2.switchone", label: t("RightClickMenu2.switchone"), run: () => void edit({ op: "EditStone", point, change: { kind: "Switch" } }) },
    { key: "RightClickMenu2.deleteone", label: t("RightClickMenu2.deleteone"), run: () => void edit({ op: "EditStone", point, change: { kind: "Delete" } }) },
    "sep",
    { key: "RightClickMenu2.review", label: t("RightClickMenu2.review") },
    "sep",
    { key: "RightClickMenu.previousMove", label: t("RightClickMenu.previousMove"), icon: "left.png", run: () => navigate({ op: "Back", count: 1 }) },
    { key: "RightClickMenu2.findStone", label: t("RightClickMenu2.findStone"), icon: "upSearch.png", run: () => void edit({ op: "FindMove", point, any_branch: true }) },
  ];
}

// ------------------------------------------------------------------ presses

/** LizzieFrame.handleSetupBoardClick (java:9500): the tool's colour, inverted by a right click;
 *  the eraser removes either colour. */
function setupClick(point: Point, right: boolean): void {
  const tool = editing.setupTool;
  const stone: Stone | null = tool === "erase" ? null : (tool === "black") !== right ? "Black" : "White";
  void edit({ op: "SetupStone", point, stone });
}

function leftPress(detail: number, point: Point | null): void {
  const v = state.view!;
  const settings = v.settings.edit;
  // Input.java:116: the second press of a double click finds the move instead of clicking.
  if (detail === 2 && !v.editing.trying && settings.allowDoubleClick) {
    if (point) void edit({ op: "FindMove", point, any_branch: false });
    return;
  }
  if (!point) return;
  // tryToMarkup, then Input.insert, then onClicked.
  if (editing.markup) {
    void edit({ op: "ToggleMark", point, tool: editing.markup });
    return;
  }
  const stone = stoneOn(point);
  if (editing.insert) {
    if (!stone) void edit({ op: "Insert", point, color: editing.insert });
    return;
  }
  if (stone) {
    // onClicked: a press on a stone starts a drag with 启用拖动棋子; Board.place ignores it.
    if (settings.allowDrag && !v.editing.trying) moving = { from: point, color: stone, picked: false, at: positionKey() };
    return;
  }
  void edit({ op: "Play", point, color: editing.placement === "alternate" ? null : editing.placement });
}

function rightPress(clientX: number, clientY: number, point: Point | null): void {
  const v = state.view!;
  // onClickedRight (java:9609): with 添加黑子 / 添加白子 a right click plays the other colour,
  // and a stone opens its menu whatever show-right-menu says.
  if (point && editing.placement !== "alternate") {
    if (stoneOn(point)) openContextMenu(clientX, clientY, stoneMenu(point));
    else void edit({ op: "Play", point, color: editing.placement === "Black" ? "White" : "Black" });
    return;
  }
  // openRightClickMenu (java:3079), else undoForRightClick.
  if (host.clearTableSelection()) return;
  if ((!v.settings.edit.showRightMenu && !host.previewMoves()) || !point) {
    navigate({ op: "Back", count: 1 });
    return;
  }
  openContextMenu(clientX, clientY, stoneOn(point) ? stoneMenu(point) : pointMenu(point));
}

/** Input.mousePressed / IndependentMainBoard.mousePressed: a press at `point` (null off the grid);
 *  a popup opens at the main-window client point (`clientX`, `clientY`). */
export function pressBoard(point: Point | null, button: number, detail: number, altKey: boolean, clientX: number, clientY: number): void {
  if (!state.view || (button !== 0 && button !== 1 && button !== 2)) return;
  // processCommentMousePressed: a press on the board closes the comment edit pane.
  commitComment();
  if (button === 1) return;
  if (state.view.editing.setup_mode) {
    if (point) setupClick(point, button === 2);
    return;
  }
  if (moving?.picked) {
    finishMove(point);
    return;
  }
  if (contextMenuShowing()) return;
  // Alt+press is the analysis region selection (selectPressed), not a board edit.
  if (altKey) return;
  if (button === 0) leftPress(detail, point);
  else rightPress(clientX, clientY, point);
}

/** mouseReleased: the middle button plays the previewed variation (playMiddleVariation); the left
 *  one ends a stone dragged with 启用拖动棋子 at `point` (DraggedReleased). */
export function releaseBoard(point: Point | null, button: number): void {
  if (button === 1) {
    if (contextMenuShowing()) return;
    const moves = host.previewMoves();
    if (moves) void edit({ op: "PlayVariation", moves });
    return;
  }
  if (button !== 0 || !moving || moving.picked) return;
  const v = state.view;
  if (v && !v.editing.trying && v.settings.edit.allowDrag) finishMove(point);
  else finishMove(null);
}

/** Binds the board panels: the main board and the second board (双引擎模式's right board, 思考模式's
 *  raw board), whose points `pointAt` resolves to the same intersections. */
export function bindBoardInput(boards: HTMLElement[], boardHost: BoardHost): void {
  host = boardHost;
  for (const board of boards) {
    board.addEventListener("contextmenu", (ev) => ev.preventDefault());
    board.addEventListener("mousedown", (ev) => {
      if (!state.view || (ev.button !== 0 && ev.button !== 1 && ev.button !== 2)) return;
      ev.preventDefault();
      pressBoard(ev.button === 1 ? null : host.pointAt(ev.clientX, ev.clientY), ev.button, ev.detail, ev.altKey, ev.clientX, ev.clientY);
    });
    board.addEventListener("mouseup", (ev) => {
      if (ev.button === 1) releaseBoard(null, 1);
    });
  }
  window.addEventListener("mousemove", (ev) => {
    if (!moving) return;
    if (moving.picked || ev.buttons & 1) showGhost(ev.clientX, ev.clientY);
  });
  window.addEventListener("mouseup", (ev) => {
    if (ev.button === 0) releaseBoard(host.pointAt(ev.clientX, ev.clientY), 0);
  });
}

// ------------------------------------------------------------------ keys (Input.keyPressed)

/** The record-editing keys of Input.keyPressed, in its precedence; browsing keys are left to
 *  the caller. Returns whether the key was handled. */
export function handleEditKey(e: KeyboardEvent): boolean {
  const ctrl = e.ctrlKey || e.metaKey; // controlIsPressed
  switch (e.key) {
    case "ArrowRight":
      if (e.shiftKey) {
        // Java also moves the branch down after 交换黑白 (a missing else); only the exchange runs.
        if (ctrl && e.altKey) command("menu.exchange");
        else void edit({ op: "MoveBranch", up: false });
        return true;
      }
      if (ctrl && e.altKey) {
        command("menu.spinRight");
        return true;
      }
      return false;
    case "ArrowLeft":
      if (e.shiftKey) {
        void edit({ op: "MoveBranch", up: true });
        return true;
      }
      if (ctrl && e.altKey) {
        command("menu.spinLeft");
        return true;
      }
      return false;
    case "ArrowUp":
      if (ctrl && e.altKey && !e.shiftKey) {
        command("menu.mirrorVertical");
        return true;
      }
      return false;
    case "ArrowDown":
      if (ctrl && e.altKey && !e.shiftKey) {
        command("menu.mirrorHorizon");
        return true;
      }
      return false;
    case "Home":
      if (!ctrl) return false;
      void edit({ op: "ClearBoard" });
      return true;
    case "Delete":
    case "Backspace":
      // Alt clears the analysis of the whole game (not a record edit).
      if (e.altKey) return false;
      void deleteWithPrompt(e.shiftKey);
      return true;
  }
  switch (e.code) {
    case "KeyL":
      void edit({ op: "SetAsMain" });
      return true;
    case "KeyP":
      command("menu.playPassMove");
      return true;
    case "Comma": {
      // Alt+, is genmove (engine play).
      if (e.altKey) return false;
      // playCurrentVariation, else playBestMove.
      const moves = host?.previewMoves();
      if (moves) void edit({ op: "PlayVariation", moves });
      else command("menu.playBestMove");
      return true;
    }
    case "KeyI":
      command(e.ctrlKey ? "menu.setBoard" : "game.komi");
      return true;
    case "KeyV":
      // Ctrl+V pastes and Alt+V toggles the variation display; plain V is 试下.
      if (ctrl || e.altKey) return false;
      command("toolbar.try-play");
      return true;
  }
  return false;
}
