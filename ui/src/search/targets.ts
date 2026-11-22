// Where a function-search result leads and whether it can be used now (Java cb53acc
// FunctionSearchController.activate / unavailableReason / dependencyReason and
// LizzieFrame.functionEntryUnavailableReason). Resolving is read-only: it never builds a page,
// opens a menu or runs a command. Activating follows the catalog target: an ACTION runs its catalog
// command, a WINDOW clicks its menu item or toolbar control (the Java openers run their owner
// method), and SETTING / NAVIGATION menus and toolbar controls are located, never clicked.
import { state } from "../app";
import { livePopupAnchor, locateHiddenButton, locateLivePopupItem } from "../chrome/bottombar";
import { locateMenuItem, menuItemCommand, menuItemState } from "../chrome/menubar";
import { COMMANDS } from "../commands";
import type { FunctionMatch } from "../contract/generated";
import { dialogOpen, message } from "../dialogs";
import { openSettingsAt, settingTargetAvailable } from "../engine/settingsTargets";
import { product, registerProduct, t } from "../i18n";
import { resourceView } from "../setup/dialog";
import { markupToolsShown, setEditing } from "../editing/state";
import { revealToolbar } from "../workbench/panels";
import { matchPlaying } from "../match/state";
import { aiCoachBlockedByGame } from "../humansl/state";
import { renderToolbar, revealOverflowedControl } from "../chrome/toolbar";
import type { KataParam } from "../contract/generated";
import { kataGoPda, kataRevealed } from "../engine/kataParams";
import { revealTopGroup, TOP_GROUP_OF } from "../chrome/topGroups";
import { kataGoCustom } from "../engine/customPda";
import { clearHighlight, highlight } from "./highlight";

/** The image settings' dependency: only the plain-colour choices turn an image path off (any
 *  theme, 默认 included, edits its images). */
registerProduct({
  ZhCn: { fsThemeImageDependency: "背景、棋盘或棋子选用纯色时，对应的图片路径不可用。" },
  ZhTw: { fsThemeImageDependency: "背景、棋盤或棋子選用純色時，對應的圖片路徑無法使用。" },
  EnUs: { fsThemeImageDependency: "An image path is unavailable while its background, board or stones use a plain color." },
  JaJp: { fsThemeImageDependency: "背景・碁盤・石に単色を選んでいる間は、対応する画像のパスを使えません。" },
  KoKr: { fsThemeImageDependency: "배경, 바둑판, 바둑돌에 단색을 고른 동안에는 해당 이미지 경로를 쓸 수 없습니다." },
  ThTh: { fsThemeImageDependency: "เมื่อพื้นหลัง กระดาน หรือหมากใช้สีพื้น จะใช้ที่อยู่ไฟล์รูปภาพของส่วนนั้นไม่ได้" },
});

export type DestinationKind = "action" | "menu" | "toolbar" | "setting" | "window" | "context";

/** A Java resource key, or with `product` this product's own message key. */
export interface Reason {
  key: string;
  product?: boolean;
}

export interface Resolution {
  /** The destination this result uses (Java's activation branch), null when it has none here. */
  kind: DestinationKind | null;
  /** The product has no real control, row or command for the destination. */
  missing: boolean;
  reason: Reason | null;
  dependency: Reason | null;
}

const UNAVAILABLE: Reason = { key: "unavailable", product: true };
const CONTEXT: Reason = { key: "FunctionSearch.unavailable.context" };
const TARGET_MISSING: Reason = { key: "FunctionSearch.unavailable.targetMissing" };
const TOOLBAR: Reason = { key: "FunctionSearch.unavailable.toolbar" };
const NO_ENGINE: Reason = { key: "LizzieFrame.setParamNoEngineHint" };
const NOT_KATAGO: Reason = { key: "SetKataRules.notKataGoHint" };
const ANALYSIS_CONFLICT: Reason = { key: "WholeGameAnalysis.conflict.analysis" };
const ENGINE_GAME: Reason = { key: "AnalysisSettings.reuseStatus.engine_game" };

export function reasonText(reason: Reason): string {
  return reason.product ? product(reason.key) : t(reason.key);
}

/** Report form: the Java key, or `product.<key>`. */
export function reasonKey(reason: Reason | null): string | null {
  return reason ? (reason.product ? `product.${reason.key}` : reason.key) : null;
}

/** FunctionSearchController.activate's explicit openers (WINDOW-like destinations Java opens with
 *  its own owner method instead of locating a control). Each runs this product's equivalent
 *  catalog command; one without that command is unavailable. */
const OPENERS: Record<string, true> = {
  "weights.download": true, // openKataGoWeightDownload
  "engine.acceleration": true, // openKataGoAcceleration
  "game.komi": true, // editGameKomi
  "engine.rules": true, // setRulesAtEditor
  "game.ai-coach": true, // handleAiCoachToolbarAction
};

/** Board context-menu items (editing/input.ts pointMenu / stoneMenu, analysis/region.ts
 *  regionMenuEntries) that act in this product, by the Java menu key the catalog path ends with.
 *  The others are shown disabled there. */
const CONTEXT_ITEMS: Record<string, true> = {
  "RightClickMenu.cleanedittemp": true,
  "RightClickMenu.addblack": true,
  "RightClickMenu.addwhite": true,
  "RightClickMenu.allow": true,
  "RightClickMenu.allow2": true,
  "RightClickMenu.allow3": true,
  "RightClickMenu.avoid": true,
  "RightClickMenu.avoid2": true,
  "RightClickMenu.cancelavoid": true,
  "RightClickMenu.trackPoint": true,
  "RightClickMenu.untrackPoint": true,
  "RightClickMenu.clearAllTracked": true,
  "RightClickMenu.priority": true,
  "RightClickMenu.clearPriority": true,
  "RightClickMenu.addSuggestionAsBranch": true,
  "RightClickMenu.previousMove": true,
  "RightClickMenu2.findStone": true,
  "RightClickMenu2.moveStone": true,
  "RightClickMenu2.switchone": true,
  "RightClickMenu2.deleteone": true,
  "RightClickMenu2.review": true,
};

/** The located control of a toolbar destination (`data-function` = Java action id), on a bar or
 *  in the top toolbar's 更多, or the 同步 button holding a popup item. `engine.pda` is the 激进度
 *  check box, except with a KataGoPda engine, whose 激进度 group is hidden: then its "..."
 *  (Menu.topFunctionTarget → more2). */
function toolbarControl(id: string): HTMLElement | null {
  const only = id === "engine.pda" ? (kataGoPda() ? ".pda-more" : ":not(.pda-more)") : "";
  const selector = `[data-function="${CSS.escape(id)}"]${only}`;
  return document.querySelector<HTMLElement>(`#toolbar ${selector}, #toolbarOverflow ${selector}, #bottombar ${selector}`) ?? livePopupAnchor(id);
}

/** The 激进度 / 广度 groups (Menu.locateTopFunction sets showPDAInMenu / showWRNInMenu) and the
 *  自定义内容 groups of chrome/topGroups.ts: locating shows the group for this run, so a group its
 *  setting hides is not a reason; the engine-state reasons are
 *  LizzieFrame.functionEntryUnavailableReason's. */
const KATA_GROUPS: Record<string, KataParam> = { "engine.pda": "Pda", "toolbar.wrn": "Wrn" };

function kindOf(m: FunctionMatch): DestinationKind | null {
  if (m.target === "Context") return "context";
  if (m.section !== null) return "setting";
  if (OPENERS[m.id]) return "window";
  if (m.target === "Action") return "action";
  if (menuItemState(m.id) !== null) return "menu";
  if (m.path[0] === "FunctionSearch.path.topToolbar" || m.path[0] === "FunctionSearch.path.bottomToolbar") return "toolbar";
  return null;
}

function missing(m: FunctionMatch, kind: DestinationKind | null): boolean {
  switch (kind) {
    case null:
      return true;
    case "context":
      return !CONTEXT_ITEMS[m.path[m.path.length - 1] ?? ""];
    case "setting":
      return !settingTargetAvailable(m.id);
    case "window":
    case "action":
      return COMMANDS[m.id] === undefined;
    case "menu":
      return menuItemState(m.id) === "missing";
    case "toolbar": {
      const control = toolbarControl(m.id);
      return !control || control.classList.contains("unavailable");
    }
  }
}

const WHOLE_GAME_ACTIVE = ["Preparing", "Baseline", "Deep", "Pausing", "Paused"];

/** A whole-game deep analysis session exists (quick/wholeGame.ts `active`). */
function wholeGameActive(): boolean {
  return WHOLE_GAME_ACTIVE.includes(state.view?.whole_game.state ?? "Idle");
}

/** WholeGameAnalysisPlan.countMainlineMoves(...) == 0: no move on the main line. */
function noMainlineMove(): boolean {
  let node = state.nodes.get(state.root);
  while (node) {
    if (node.kind === "Black" || node.kind === "White") return false;
    node = node.children[0] === undefined ? undefined : state.nodes.get(node.children[0]);
  }
  return true;
}

/** LizzieFrame.functionEntryUnavailableReason for the states this product has. Java's Web Board,
 *  ReadBoard and read-mode checks belong to entries whose destinations do not exist here yet; they
 *  resolve to `missing` before reaching this. */
function entryReason(id: string): Reason | null {
  const view = state.view;
  if (!view) return { key: "FunctionSearch.unavailable.application" };
  const engine = view.engine.foreground;
  switch (id) {
    // The double menu's game controls and 终止人机对局 (LizzieFrame.java:13178-13187).
    case "toolbar.game-pause":
    case "toolbar.game-stop":
      return view.game || view.engine_match ? null : { key: "FunctionSearch.unavailable.game" };
    case "toolbar.game-resign":
    case "game.stop-human":
      return view.game ? null : { key: "FunctionSearch.unavailable.game" };
    // 引擎对战 (LizzieFrame.java:13272-13275): menu items stay enabled like Java's.
    case "menu.pauseEngineGame":
    case "menu.changeEngineGameNumbers":
    case "menu.breakEngineGame":
      return matchPlaying(view) ? null : CONTEXT;
    case "board.try-play":
    case "board.delete-without-prompt":
      return view.engine_match?.phase === "Starting" || matchPlaying(view) ? ENGINE_GAME : null;
    // AI 陪练 (LizzieFrame.java:13170-13171, aiCoachBlockedByGame).
    case "game.ai-coach":
      return aiCoachBlockedByGame(view) ? ENGINE_GAME : null;
    case "engine.pda":
    case "toolbar.wrn":
      if (view.engine_match?.phase === "Starting" || matchPlaying(view)) return ENGINE_GAME;
      if (!engine) return NO_ENGINE;
      return engine.katago ? null : NOT_KATAGO;
    // RightClickMenu.java:124-156: shown only outside games with a KataGoCustom / KataGoPda engine.
    case "board.priority-point":
    case "board.clear-priority":
      return kataGoCustom() && !view.game ? null : CONTEXT;
    case "toolbar.force-allow":
    case "toolbar.force-avoid":
    case "toolbar.force-clear":
    case "toolbar.force-allow-options":
    case "toolbar.force-avoid-options":
      if (!engine) return NO_ENGINE;
      return id.includes("avoid") && engine.katago ? CONTEXT : null;
    case "engine.rules":
      // rulesUnavailableReason (LizzieFrame.java:13144-13151)
      if (wholeGameActive()) return ANALYSIS_CONFLICT;
      if (!engine) return NO_ENGINE;
      return engine.katago ? null : NOT_KATAGO;
    case "engine.parameters":
    case "menu.showHeatmap":
      return engine ? null : NO_ENGINE;
    // LizzieFrame.java:13216-13218: Windows first, then the helper next to the program.
    case "sync.board":
      if (!navigator.userAgent.includes("Windows")) return { key: "FunctionSearch.unavailable.windows" };
      return view.readboard.available ? null : { key: "FunctionSearch.unavailable.readBoard" };
    // LizzieFrame.java:13172-13177, 13240-13241.
    case "sync.exit-web-trial":
      return view.web_board.url && view.web_board.trial_active ? null : { key: "FunctionSearch.unavailable.webBoard" };
    case "menu.webBoardCopyUrl":
      return view.web_board.url ? null : { key: "FunctionSearch.unavailable.webBoard" };
    case "game.komi":
    case "lightning-analysis-settings":
    case "whole-game-lightning-overview":
    case "all-branches-lightning-analysis":
    case "partial-lightning-analysis":
      return wholeGameActive() ? ANALYSIS_CONFLICT : null;
    case "whole-game-deep-analysis":
      if (wholeGameActive()) return null;
      return noMainlineMove() ? { key: "WholeGameAnalysis.noGame" } : null;
    case "engine.select-secondary":
      return view.settings.extra_mode === "DoubleEngine" ? null : { key: "FunctionSearch.unavailable.secondary" };
    case "menu.stopFullTrace":
      return view.full_trace.length ? null : { key: "FunctionSearch.unavailable.fullTrace" };
    case "menu.undoDelete":
      return view.editing.can_undo ? null : { key: "FunctionSearch.unavailable.undoEdit" };
    case "menu.redoDelete":
      return view.editing.can_redo ? null : { key: "FunctionSearch.unavailable.redoEdit" };
    case "menu.commitPane":
      // Config.isCommentPanelAutoHiddenByMode: Four_Sub or Double_Engine.
      return view.settings.extra_mode === "DoubleEngine" ? CONTEXT : null;
    case "menu.moveNumberAlwaysFromOne":
    case "menu.showAllMoveNumberInBranch":
      return view.settings.display.moveNumbers === -1 ? { key: "FunctionSearch.unavailable.allMoveNumbers" } : null;
    default:
      return null;
  }
}

/** FunctionSearchController.dependencyReason. */
function dependencyReason(id: string): Reason | null {
  switch (id) {
    case "config.proxy.manual-address":
      return (resourceView()?.network.proxyMode ?? "Direct") === "Manual" ? null : { key: "FunctionSearch.dependency.manualProxy" };
    case "config.play.comment-panel":
      return state.view?.settings.extra_mode === "DoubleEngine" ? { key: "FunctionSearch.dependency.commentPanel" } : null;
    case "config.theme.background-image":
    case "config.theme.board-image":
    case "config.theme.black-stone-image":
    case "config.theme.white-stone-image":
      return { key: "fsThemeImageDependency", product: true };
    default:
      return null;
  }
}

/** A toolbar control inside a tool group its own setting switched off. The whole toolbar being
 *  hidden does not count (locating shows it), nor the markup buttons, which locating shows too. */
function groupHidden(control: HTMLElement): boolean {
  for (let el = control.parentElement; el && el.id !== "toolbar" && el.id !== "bottombar"; el = el.parentElement) {
    if (getComputedStyle(el).display === "none") return true;
  }
  return false;
}

function unavailableReason(m: FunctionMatch, kind: DestinationKind | null, absent: boolean, search: HTMLElement | null): Reason | null {
  if (!state.view) return { key: "FunctionSearch.unavailable.application" };
  if (dialogOpen(search ?? undefined)) return { key: "FunctionSearch.unavailable.modal" };
  if (absent) return UNAVAILABLE;
  if (kind === "context") return state.view.settings.edit.showRightMenu ? null : CONTEXT;
  if (kind === "toolbar" && !revealsMarkup(m.id) && !KATA_GROUPS[m.id] && !TOP_GROUP_OF[m.id] && groupHidden(toolbarControl(m.id)!)) return TOOLBAR;
  const reason = entryReason(m.id);
  if (reason) return reason;
  // A real toolbar control disabled for now (广度 while the engine is not free for it).
  if (kind === "toolbar" && toolbarControl(m.id)?.matches(":disabled")) return CONTEXT;
  if (kind === "menu" && menuItemState(m.id) === "hidden") return CONTEXT;
  // The engine menu, disabled while an engine game runs (its only disabled state on the bar).
  if (kind === "menu" && menuItemState(m.id) === "disabled") return ENGINE_GAME;
  // A registered command its owner reports disabled or hidden, whatever the destination.
  const entry = COMMANDS[m.id];
  if (entry && ((entry.visible && !entry.visible()) || (entry.enabled && !entry.enabled()))) return CONTEXT;
  return null;
}

/** `search` is the open search's modal layer, which does not count as another modal dialog. */
export function resolveFunction(m: FunctionMatch, search: HTMLElement | null = null): Resolution {
  const kind = kindOf(m);
  const absent = missing(m, kind);
  return { kind, missing: absent, reason: unavailableReason(m, kind, absent, search), dependency: dependencyReason(m.id) };
}

/** The board as the focus owner (Java owner.mainPanel). */
export function focusBoard(): void {
  const board = document.getElementById("board");
  if (!board) return;
  if (!board.hasAttribute("tabindex")) board.tabIndex = -1;
  board.focus({ preventScroll: true });
}

/** FunctionSearchController.navigateBoard: explains where the board context item is and returns to
 *  the board; nothing on the board changes. */
async function navigateBoard(m: FunctionMatch): Promise<null> {
  const board = t("FunctionSearch.path.board");
  const path = m.path.map((key) => (key === "FunctionSearch.path.board" ? board : t(key))).join(" → ");
  const args = [board, path];
  await message(t(m.title), t("FunctionSearch.navigate.board").replace(/%s/g, () => args.shift() ?? ""));
  focusBoard();
  return null;
}

/** Menu.locateTopFunction (Menu.java:6997-7001): locating a markup button or 自由涂鸦 shows the
 *  markup tools for this session (Config.isShowingMarkupTools, not saved); btnMarkup itself does
 *  not. */
function revealsMarkup(id: string): boolean {
  return (id.startsWith("toolbar.markup-") && id !== "toolbar.markup-tools") || id === "toolbar.painting";
}

/** BottomToolbar.locateFunction / Menu.locateTopFunction: shows the toolbar (and the markup tools
 *  for a markup button, and 更多's panel for a control it holds), then scrolls to and focuses the
 *  control with the gold line. A bottom button not on the bar is located in 更多. The control is
 *  not clicked and no saved setting changes. */
async function locateToolbarControl(id: string): Promise<Reason | null> {
  const control = toolbarControl(id);
  if (!control) return TOOLBAR;
  const kata = KATA_GROUPS[id];
  const top = TOP_GROUP_OF[id];
  if (kata && !kataRevealed[kata]) {
    kataRevealed[kata] = true;
    renderToolbar();
  } else if (top && revealTopGroup(top)) renderToolbar();
  if (revealsMarkup(id) && !markupToolsShown()) setEditing({ markupRevealed: true });
  const bottom = !!control.closest("#bottombar");
  await revealToolbar(bottom ? "bottom" : "top");
  // A 同步 popup item: the popup opens over the bar whether or not 同步 itself is on it.
  if (control.dataset.function !== id) return locateLivePopupItem(id) ? null : TARGET_MISSING;
  if (bottom && control.isConnected && control.getClientRects().length === 0 && locateHiddenButton(id)) return null;
  if (!bottom) revealOverflowedControl(control);
  if (!control.isConnected || control.getClientRects().length === 0) return TOOLBAR;
  control.scrollIntoView({ block: "nearest", inline: "nearest" });
  if (!control.hasAttribute("tabindex")) control.tabIndex = -1;
  clickOnSpace(control);
  control.focus({ preventScroll: true });
  highlight(control);
  return null;
}

const spaceClickable = new WeakSet<HTMLElement>();

/** The located control is a focused JButton (Menu.java:7012-7013): Space clicks it there rather
 *  than reaching the workbench's Space (分析). Kept, like Java's setFocusable(true). */
function clickOnSpace(control: HTMLElement): void {
  if (spaceClickable.has(control)) return;
  spaceClickable.add(control);
  control.addEventListener("keydown", (e) => {
    if (e.code !== "Space" || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    e.preventDefault();
    e.stopPropagation();
    if (!e.repeat) control.click();
  });
}

/** FunctionSearchController.activate's doClick for a WINDOW destination (Java 225-236): the menu
 *  item, else the toolbar control, acts as clicked; a menu that is a submenu opens like a located
 *  one. Nothing is shown or located first. */
function clickMenuItem(id: string): Reason | null {
  const entry = menuItemCommand(id);
  if (!entry) return locateMenuItem(id) ? null : TARGET_MISSING;
  if (entry.enabled && !entry.enabled()) return CONTEXT;
  entry.run();
  return null;
}

function clickToolbarControl(id: string): Reason | null {
  const control = toolbarControl(id);
  if (!control || control.dataset.function !== id) return TARGET_MISSING;
  control.click();
  return null;
}

/** Runs the destination after the search closed; a reason means nothing happened. */
export async function activateFunction(m: FunctionMatch): Promise<Reason | null> {
  clearHighlight();
  const { kind, reason } = resolveFunction(m);
  if (reason) return reason;
  switch (kind) {
    case "context":
      return navigateBoard(m);
    case "setting":
      return (await openSettingsAt(m.id)) ? null : TARGET_MISSING;
    case "menu":
      if (m.target === "Window") return clickMenuItem(m.id);
      return locateMenuItem(m.id) ? null : TARGET_MISSING;
    case "toolbar":
      return m.target === "Window" ? clickToolbarControl(m.id) : locateToolbarControl(m.id);
    case "window":
    case "action":
      COMMANDS[m.id].run();
      return null;
    default:
      return TARGET_MISSING;
  }
}
