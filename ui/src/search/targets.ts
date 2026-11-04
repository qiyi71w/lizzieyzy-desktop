// Where a function-search result leads and whether it can be used now (Java cb53acc
// FunctionSearchController.activate / unavailableReason / dependencyReason and
// LizzieFrame.functionEntryUnavailableReason). Resolving is read-only: it never builds a page,
// opens a menu or runs a command. Only an ACTION destination runs its catalog command; settings,
// menus, toolbar controls and windows other than the Java openers are located, never clicked.
import { state } from "../app";
import { locateMenuItem, menuItemState } from "../chrome/menubar";
import { COMMANDS } from "../commands";
import type { FunctionMatch } from "../contract/generated";
import { dialogOpen, message } from "../dialogs";
import { openSettingsAt, settingTargetAvailable } from "../engine/settingsTargets";
import { isChinese, product, t } from "../i18n";
import { resourceView } from "../setup/dialog";
import { markupToolsShown, setEditing } from "../editing/state";
import { revealToolbar } from "../workbench/panels";
import { clearHighlight, highlight } from "./highlight";

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
  "share.private-history": true, // openPrivateKifuSearch
  "share.public-history": true, // openPublicKifuSearch
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
  "RightClickMenu.addSuggestionAsBranch": true,
  "RightClickMenu.previousMove": true,
  "RightClickMenu2.findStone": true,
  "RightClickMenu2.moveStone": true,
  "RightClickMenu2.switchone": true,
  "RightClickMenu2.deleteone": true,
};

/** The located control of a toolbar destination (`data-function` = Java action id). */
function toolbarControl(id: string): HTMLElement | null {
  return document.querySelector<HTMLElement>(`#toolbar [data-function="${CSS.escape(id)}"], #bottombar [data-function="${CSS.escape(id)}"]`);
}

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
      return !control || control.classList.contains("unavailable") || control.matches(":disabled");
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

/** LizzieFrame.functionEntryUnavailableReason for the states this product has. Java's engine
 *  game, Web Board, ReadBoard and read-mode checks belong to entries whose destinations do not
 *  exist here yet; they resolve to `missing` before reaching this. */
function entryReason(id: string): Reason | null {
  const view = state.view;
  if (!view) return { key: "FunctionSearch.unavailable.application" };
  if (id.startsWith("legacy.") && !isChinese()) return TOOLBAR;
  const engine = view.engine.foreground;
  switch (id) {
    // The double menu's game controls and 终止人机对局 (LizzieFrame.java:13178-13187).
    case "toolbar.game-pause":
    case "toolbar.game-stop":
    case "toolbar.game-resign":
    case "game.stop-human":
      return view.game ? null : { key: "FunctionSearch.unavailable.game" };
    case "engine.pda":
    case "toolbar.wrn":
      if (!engine) return NO_ENGINE;
      return engine.katago ? null : NOT_KATAGO;
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
    case "sync.board":
      return navigator.userAgent.includes("Windows") ? null : { key: "FunctionSearch.unavailable.windows" };
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
      return view.engine.full_trace ? null : { key: "FunctionSearch.unavailable.fullTrace" };
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
    case "toolbar.detailed":
      return isChinese() ? null : CONTEXT;
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
      return { key: "FunctionSearch.dependency.themeImage" };
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
  if (kind === "toolbar" && !revealsMarkup(m.id) && groupHidden(toolbarControl(m.id)!)) return TOOLBAR;
  const reason = entryReason(m.id);
  if (reason) return reason;
  if (kind === "menu" && menuItemState(m.id) === "hidden") return CONTEXT;
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

/** Menu.locateTopFunction (Menu.java:6997-7001): locating a markup button shows the markup tools
 *  for this session (Config.isShowingMarkupTools, not saved); btnMarkup itself does not. */
function revealsMarkup(id: string): boolean {
  return id.startsWith("toolbar.markup-") && id !== "toolbar.markup-tools";
}

/** BottomToolbar.locateFunction / Menu.locateTopFunction: shows the toolbar (and the markup tools
 *  for a markup button), then scrolls to and focuses the control with the gold line. The control
 *  is not clicked and no saved setting changes. */
async function locateToolbarControl(id: string): Promise<Reason | null> {
  const control = toolbarControl(id);
  if (!control) return TOOLBAR;
  if (revealsMarkup(id) && !markupToolsShown()) setEditing({ markupRevealed: true });
  await revealToolbar(control.closest("#bottombar") ? "bottom" : "top");
  if (!control.isConnected || control.getClientRects().length === 0) return TOOLBAR;
  control.scrollIntoView({ block: "nearest", inline: "nearest" });
  if (!control.hasAttribute("tabindex")) control.tabIndex = -1;
  control.focus({ preventScroll: true });
  highlight(control);
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
      return locateMenuItem(m.id) ? null : TARGET_MISSING;
    case "toolbar":
      return locateToolbarControl(m.id);
    case "window":
    case "action":
      COMMANDS[m.id].run();
      return null;
    default:
      return TARGET_MISSING;
  }
}
