// 棋盘同步工具's float board (FloatBoard.java) in the main window: shows, moves and hides the
// window to follow WorkbenchView.readboard.float_board, sends it the scene to paint and applies its
// input. The core owns the window's existence, geometry, edit mode and hidden suggestions; this
// module owns the float board's own hover (FloatBoardRenderer is separate from the main board's
// renderer), its preview depth and the 位置 buttons' offset (extraX / extraY, not persisted).
// Like Java's JDialog, the window is created once and then only hidden and shown again; it is
// destroyed with the application.
//
// It follows every published view rather than the main window's paint loop, so the float board
// keeps up while the main window is minimised or covered by the client it overlays.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { navigate, registerViewHandler, run, state } from "../app";
import { toggleKataEstimate } from "../analysis/infoCommands";
import type { FloatBoardView, FloatSceneView, Point, WorkbenchView } from "../contract/generated";
import { dialogOpen } from "../dialogs";
import { bindFloatReplayHost } from "../display/autoplay";
import type { ReplayTarget } from "../display/replay";
import { candidatesShownFor, boardUi, onBoardUiChange, togglePolicy, toggleRawNet } from "../display/state";
import { edit } from "../editing/state";
import { language } from "../i18n";
import type { BoardScene, PreviewVariation } from "../workbench/paint";
import { FLOAT_DATA_EVENT, FLOAT_INPUT_EVENT, FLOAT_LABEL, type FloatInput, type FloatPayload } from "./floatModel";

/** Builds the main board's scene for a hovered candidate and its variation (main.ts); outside the
 *  edit mode `at` is the main line's end the overlay draws instead of the current position. */
export type SceneSource = (v: WorkbenchView, hovered: number | null, preview: PreviewVariation | null, at: FloatSceneView | null) => BoardScene;

type Placement = { x: number; y: number; width: number; height: number };
/** What show_float_board placed: the edge round the helper's rectangle and the window's physical
 *  size, and whether it had to build the window (its page is not listening yet). */
type Placed = { pad: number; width: number; height: number; created: boolean };
type Variation = { key: string; pv: (Point | null)[]; pv_captures: Point[][]; pv_visits: number[] };

let source: SceneSource | null = null;
const win = {
  /** The placement last asked of src-tauri (null: hidden), and what it reported back. */
  requested: null as string | null,
  placed: null as Omit<Placed, "created"> | null,
  /** The window's page listens (its "ready" arrived), and how many "ready"s arrived so far. */
  ready: false,
  readies: 0,
  lastSent: "",
  /** extraX / extraY (the arrow buttons), cleared when the helper's size changes (setPos). */
  extraX: 0,
  extraY: 0,
  size: "",
  /** The last float_hide_request answered with FloatBoardHidden. */
  confirmed: null as number | null,
};
/** Window operations run one after another, in the order the views asked for them. */
let ops: Promise<unknown> = Promise.resolve();

/** FloatBoard.mouseOverCoordinate, the selected candidate's point (isMouseOver), the displayed
 *  branch length (null: SHOW_NORMAL_BOARD, the whole variation), a middle-click cancel, the
 *  variation kept while 鼠标悬停显示变化图时,变化图不刷新 is on and a count of the variations
 *  selected, which tells the auto-replay a newly selected one (replayTarget). */
const cursor = {
  over: null as string | null,
  hover: null as string | null,
  length: null as number | null,
  cancelled: false,
  frozen: null as Variation | null,
  node: "",
  selections: 0,
};

/** The one key format of an intersection, compared between hover, candidates and the pointer. */
const pointKey = (p: Point) => `${p.x},${p.y}`;

/** clearMoved: no candidate selected, the normal board. */
function clearMoved(): void {
  cursor.hover = null;
  cursor.length = null;
  cursor.cancelled = false;
  cursor.frozen = null;
}

function placement(f: FloatBoardView): Placement {
  const size = `${f.width}x${f.height}`;
  if (size !== win.size) {
    win.size = size;
    win.extraX = 0;
    win.extraY = 0;
  }
  return { x: f.x + win.extraX, y: f.y + win.extraY, width: f.width, height: f.height };
}

/** The selected candidate and its variation (FloatBoardRenderer.prepareSelection / drawBranch),
 *  shortened to limit-branch-length or the wheel's length. The candidates are those of the node
 *  the overlay draws (getCurOrMainEnd). `selection` is the selected variation the wheel and the
 *  arrow keys step (hasSelectedVariation) even while it is not shown: one move without
 *  auto-replay, or a middle-click cancel. `target` is the shown variation as the auto-replay
 *  steps it. */
function hovered(v: WorkbenchView, f: FloatBoardView): { order: number | null; preview: PreviewVariation | null; selection: PreviewVariation | null; target: ReplayTarget | null } {
  const at = f.scene;
  const node = `${v.document.id}:${at ? at.node : v.position.node}`;
  if (node !== cursor.node) {
    cursor.node = node;
    cursor.length = null;
    cursor.frozen = null;
    cursor.cancelled = false;
  }
  const candidates = (at ? at.analysis : v.analysis)?.candidates ?? [];
  const d = v.settings.display;
  // FloatBoard.isMouseOver plus FloatBoardRenderer's preview conditions: the side to move's
  // candidates and 鼠标悬停显示变化图 on, no pure network, policy, scoring or setup mode.
  const allowed = candidatesShownFor(at ? at.to_play : v.position.to_play) && d.suggestionVariations && v.rawNet === null && !boardUi.policy && !v.score && !v.editing.setup_mode;
  const c = allowed && cursor.hover !== null ? candidates.find((c) => c.point && pointKey(c.point) === cursor.hover) : undefined;
  if (!c?.point) {
    cursor.frozen = null;
    return { order: null, preview: null, selection: null, target: null };
  }
  const key = `${node}:${pointKey(c.point)}`;
  // A newly selected variation (!notChangedMouseOverMove, FloatBoardRenderer.java:748-758) starts
  // at one move when its candidate has too few playouts or lies beyond 最大候选点数, or while
  // auto-replay runs, unless auto-replay shows the whole variation first.
  if (cursor.frozen?.key !== key) {
    cursor.selections++;
    const ratio = c.allocation_visits / Math.max(1, ...candidates.map((x) => x.allocation_visits));
    const maxSuggestion = v.settings.engine.analysis.limitMaxSuggestion;
    const minor = ratio <= d.minPlayoutRatio || (maxSuggestion > 0 && c.order + 1 > maxSuggestion && !c.kept);
    if (!(d.autoReplayBranch && d.entireVariationFirst) && (cursor.length === null || cursor.length < 2) && (minor || d.autoReplayBranch)) cursor.length = 1;
  }
  const frozen = !d.noRefreshOnMouse || cursor.frozen?.key !== key ? { key, pv: c.pv, pv_captures: c.pv_captures, pv_visits: c.pv_visits } : cursor.frozen;
  cursor.frozen = frozen;
  const limit = v.settings.engine.analysis.limitBranchLength;
  const steps = limit > 0 ? Math.min(limit, frozen.pv.length) : frozen.pv.length;
  const selection = { order: c.order, pv: frozen.pv, pv_captures: frozen.pv_captures, pv_visits: frozen.pv_visits, length: cursor.length ?? steps };
  // drawBranch (FloatBoardRenderer.java:790-799): one move without auto-replay, or a cancelled
  // preview, shows no variation.
  if (cursor.cancelled || (cursor.length === 1 && !d.autoReplayBranch)) return { order: c.order, preview: null, selection, target: null };
  return { order: c.order, preview: selection, selection, target: { key: `${key}:${cursor.selections}`, steps } };
}

/** FloatBoard.doBranch: the target window gets the focus back (Menu.readBoardGetFocus, decided by
 *  the core), then forward from the whole variation shows two moves and one more per notch within
 *  it; back from the whole variation shows all but its last move, then one less down to one. A
 *  notch that sets a length restarts the auto-replay wait and ends a middle-click cancel
 *  (setDisplayedBranchLength → stopReplay, previewCancelled = false,
 *  FloatBoardRenderer.java:2515-2524); one at either end sets none (FloatBoard.java:501-510). */
function doBranch(selection: PreviewVariation, down: boolean): void {
  void run({ type: "ReadBoard", op: { type: "ReturnFocus" } });
  const limit = state.view?.settings.engine.analysis.limitBranchLength ?? 0;
  const steps = limit > 0 ? Math.min(limit, selection.pv.length) : selection.pv.length;
  const length = cursor.length;
  if (down) {
    if (length === null || length === 1) cursor.length = 2;
    else if (steps > length) cursor.length = length + 1;
  } else if (length === null) cursor.length = steps - 1;
  else if (length > 1) cursor.length = length - 1;
  if (cursor.length !== length) {
    cursor.selections++;
    cursor.cancelled = false;
  }
}

function send(v: WorkbenchView, f: FloatBoardView): void {
  if (!source || !win.placed || !win.ready) return;
  const { order, preview } = hovered(v, f);
  const base = source(v, order, preview, f.scene);
  const setup = v.editing.setup_mode;
  // FloatBoardRenderer draws no markup, next-move hints, names, scoring, region or focus marks;
  // in the setup mode nothing beyond the stones.
  const payload: FloatPayload = {
    language: language(),
    appearance: v.settings.appearance,
    theme: v.settings.theme,
    themeFilesRevision: v.settings.theme_files_revision,
    ...win.placed,
    scene: {
      ...base,
      unlimited: [...base.unlimited],
      marks: [],
      nextMoves: [],
      nextMainMove: null,
      nextColor: null,
      names: false,
      score: null,
      mark: null,
      region: null,
      selecting: null,
      focus: null,
      ...(setup ? { ownership: null, rawNetOn: false, policy: false } : {}),
    },
    edit: f.edit_mode,
    hidden: f.suggestions_hidden,
    editButton: f.edit_button,
    pondering: v.engine.analysing,
    boardType: f.board_type,
    setup,
  };
  const text = JSON.stringify(payload);
  if (text === win.lastSent) return;
  win.lastSent = text;
  void emitTo(FLOAT_LABEL, FLOAT_DATA_EVENT, payload);
}

/** Shows, moves or hides the window for the current view, then sends it the scene. */
function sync(): void {
  const v = state.view;
  if (!v) return;
  const f = v.readboard.float_board;
  const want = f ? placement(f) : null;
  const key = want ? JSON.stringify(want) : null;
  const moved = key !== win.requested;
  if (moved) {
    win.requested = key;
    ops = ops.then(async () => {
      try {
        if (want) {
          const readies = win.readies;
          const { created, ...placed } = await invoke<Placed>("show_float_board", want);
          win.placed = placed;
          // A new page announces itself once it listens; until then nothing is sent.
          if (created && win.readies === readies) win.ready = false;
          win.lastSent = "";
        } else if (win.placed) {
          // setVisible(false) clears the hover and the preview.
          cursor.over = null;
          clearMoved();
          await invoke("hide_float_board");
          // Off the screen only once the hide succeeded (a failed hide keeps it placed).
          win.placed = null;
        }
      } catch (e) {
        console.error("placing the float board failed", e);
      }
      sync();
    });
  }
  // A `place` waits until the window hidden for it is off the screen (Java writes it after
  // runOnEdtAndWait hid the window): answered once, after the hide queued above has finished,
  // and only if the window is really off the screen. Without the answer the move fails at its
  // confirmation timeout and nothing is clicked.
  const request = v.readboard.float_hide_request;
  if (request !== null && request !== win.confirmed) {
    win.confirmed = request;
    ops = ops.then(async () => {
      if (win.placed === null) await run({ type: "ReadBoard", op: { type: "FloatBoardHidden", request } });
    });
  }
  if (!moved && f) send(v, f);
}

function input(e: FloatInput): void {
  const v = state.view;
  const f = v?.readboard.float_board;
  if (e.type === "ready") {
    win.ready = true;
    win.readies++;
    win.lastSent = "";
    return;
  }
  if (!v || !f) return;
  if (e.type === "leave") {
    // mouseExited
    cursor.over = null;
    clearMoved();
    return;
  }
  // The main window's modal dialogs block the float board like the workbench.
  if (dialogOpen()) return;
  switch (e.type) {
    case "move": {
      // mouseMoved: a new intersection selects the candidate there, if any, else clears it.
      const key = e.point ? pointKey(e.point) : null;
      if (key === cursor.over) return;
      cursor.over = key;
      clearMoved();
      if (key && (f.scene ? f.scene.analysis : v.analysis)?.candidates.some((c) => c.point && pointKey(c.point) === key)) cursor.hover = key;
      return;
    }
    case "press":
      // mousePressed: the middle button cancels the preview; the others clear the hover, and the
      // left one places a move where clicks place them (onClicked). The right one does nothing more.
      if (e.button === 1) {
        cursor.cancelled = true;
        return;
      }
      cursor.over = null;
      clearMoved();
      if (e.button === 0 && e.point && f.clicks_play) void edit({ op: "Play", point: e.point, color: null });
      return;
    case "wheel": {
      const { selection } = hovered(v, f);
      if (selection) doBranch(selection, e.down);
      else if (f.edit_mode) navigate(e.down ? { op: "Forward", count: 1 } : { op: "Back", count: 1 });
      return;
    }
    case "key":
      key(v, f, e.code);
      return;
    case "button":
      switch (e.button) {
        // btnStopGo: Lizzie.leelaz.togglePonder.
        case "stopGo":
          void run({ type: "ToggleAnalysis" });
          return;
        case "hideShow":
          toggleHide(f);
          return;
        case "edit":
          toggleEdit(f);
          return;
        case "left":
          win.extraX--;
          return;
        case "right":
          win.extraX++;
          return;
        case "up":
          win.extraY--;
          return;
        case "down":
          win.extraY++;
          return;
      }
  }
}

/** changeEetEditMode: the hover clears; the core leaves for the main line's end on exit. */
function toggleEdit(f: FloatBoardView): void {
  cursor.over = null;
  clearMoved();
  void run({ type: "ReadBoard", op: { type: "SetFloatEditMode", enabled: !f.edit_mode } });
}

/** toggleHide (+/- and F): the hover clears; showing again returns to the main line's end. */
function toggleHide(f: FloatBoardView): void {
  cursor.over = null;
  clearMoved();
  void run({ type: "ReadBoard", op: { type: "SetFloatSuggestionsHidden", hidden: !f.suggestions_hidden } });
}

/** FloatBoard's key listener (FloatBoard.java:283-313). */
function key(v: WorkbenchView, f: FloatBoardView, code: string): void {
  switch (code) {
    case "KeyV":
      toggleEdit(f);
      return;
    case "Period":
      if (v.settings.display.kataEstimateShortcut) toggleKataEstimate();
      return;
    case "KeyH":
      toggleRawNet();
      return;
    case "KeyT":
      togglePolicy();
      return;
    case "ArrowUp":
    case "ArrowDown": {
      const { selection } = hovered(v, f);
      if (selection) doBranch(selection, code === "ArrowDown");
      return;
    }
    case "KeyG":
      // tryToRefreshVariation: the selected candidate's variation is taken again, whole.
      if (hovered(v, f).preview) {
        cursor.length = null;
        cursor.frozen = null;
      }
      return;
    case "KeyF":
      toggleHide(f);
      return;
    case "Space":
      // togglePonderMannul, the workbench's Space.
      void run({ type: "ToggleAnalysis" });
      return;
  }
}

/** Starts following the views and the window's input; `scene` builds the main board's scene. */
export async function startFloatBoard(scene: SceneSource): Promise<void> {
  source = scene;
  // The float board's own replay target and length (FloatBoardRenderer.replayTarget,
  // getDisplayedBranchLength, setReplayLength), stepped by the main window's timer while shown.
  bindFloatReplayHost({
    target: () => {
      const v = state.view;
      const f = v?.readboard.float_board;
      return v && f && win.placed ? hovered(v, f).target : null;
    },
    length: () => cursor.length,
    setLength: (length) => {
      cursor.length = length;
      sync();
    },
  });
  registerViewHandler(() => sync());
  onBoardUiChange(sync);
  await listen<FloatInput>(FLOAT_INPUT_EVENT, (e) => {
    input(e.payload);
    sync();
  });
  sync();
}

/** What the acceptance probe reports: whether the window is placed and where. */
export function floatBoardReport() {
  return { open: win.placed !== null, requested: win.requested, extra: { x: win.extraX, y: win.extraY } };
}
