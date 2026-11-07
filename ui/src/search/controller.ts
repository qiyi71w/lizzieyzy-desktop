// FunctionSearchController (Java cb53acc): the search entries (toolbar button, Ctrl+K / Cmd+K),
// shortcut isolation, focus restore after a cancelled search, and the final activation admission.
import { state } from "../app";
import { COMMANDS } from "../commands";
import { searchFunctions } from "../contract/client";
import { dialogOpen, message, ownerIsDisabled } from "../dialogs";
import { t } from "../i18n";
import { openSearchDialog, type SearchDialog } from "./dialog";
import { activateFunction, focusBoard, reasonKey, reasonText, resolveFunction, type DestinationKind } from "./targets";

let dialog: SearchDialog | null = null;
let openQueued = false;
/** Keys pressed into the search (and the K that opened it): their releases, and their auto-repeat
 *  once it closed, never reach the board or the control opened afterwards. */
const swallowedKeys = new Set<string>();
let restoreGeneration = 0;
let restoreTimer = 0;
let lastActivation: { id: string; kind: DestinationKind | null; reason: string | null } | null = null;
/** The workbench's refusal for the focus owner or key target (bindFunctionSearch). */
let workbenchRefuses: (target: EventTarget | null) => boolean = () => false;

/** A modal dialog other than the search is showing (hasOtherModal). */
function otherModal(): boolean {
  if (dialogOpen(dialog?.layer) || ownerIsDisabled()) return true;
  return [...document.querySelectorAll<HTMLElement>("[aria-modal='true']")].some((el) => !dialog?.contains(el) && el.getClientRects().length > 0);
}

/** Whether a search entry must not open the search now: the focus owner (or key target) is a text
 *  control, or a menu, context menu, progress window or other modal dialog is showing. The toolbar
 *  button asks on its press, before that press closes an open menu. */
export function functionSearchRefused(target: EventTarget | null = document.activeElement): boolean {
  return workbenchRefuses(target) || otherModal();
}

function cancelFocusRestore(): void {
  restoreGeneration++;
  clearInterval(restoreTimer);
  restoreTimer = 0;
}

function focusable(el: HTMLElement): boolean {
  return el.isConnected && el.getClientRects().length > 0 && !el.closest("[inert]") && !(el as HTMLButtonElement).disabled
    && (el.tabIndex >= 0 || el.hasAttribute("tabindex") || el.isContentEditable);
}

/** restoreFocus: the control that had the focus, else the board, retried for 2 s while nothing
 *  else has the focus; a control the user chose in the meantime keeps it. */
function restoreFocus(previous: HTMLElement | null, generation: number): void {
  if (generation !== restoreGeneration) return;
  let attempts = 0;
  const tick = () => {
    if (generation !== restoreGeneration) return;
    const target = previous && focusable(previous) ? previous : document.getElementById("board");
    if (++attempts > 40 || dialog || otherModal() || !target || document.activeElement === target) {
      cancelFocusRestore();
      return;
    }
    if (!document.hasFocus()) return;
    const current = document.activeElement;
    if (current && current !== document.body) {
      cancelFocusRestore();
      return;
    }
    if (target.id === "board") focusBoard();
    else target.focus({ preventScroll: true });
  };
  tick();
  if (generation === restoreGeneration) restoreTimer = window.setInterval(tick, 50);
}

/** open(): the search, then the chosen destination once the search closed. */
export async function openFunctionSearch(): Promise<void> {
  cancelFocusRestore();
  const generation = restoreGeneration;
  if (!state.view) return;
  if (dialog) {
    dialog.focus();
    return;
  }
  if (functionSearchRefused()) return;
  const active = document.activeElement;
  const previous = active instanceof HTMLElement && active !== document.body ? active : null;
  const current = openSearchDialog();
  dialog = current;
  const selected = await current.done;
  if (dialog === current) dialog = null;
  if (selected) {
    const reason = await activateFunction(selected);
    lastActivation = { id: selected.id, kind: resolveFunction(selected).kind, reason: reasonKey(reason) };
    if (reason) await message(t("FunctionSearch.title"), reasonText(reason));
  } else restoreFocus(previous, generation);
}

/** getMenuShortcutKeyMaskEx with nothing else held: Cmd on macOS, Ctrl elsewhere. */
function shortcut(e: KeyboardEvent): boolean {
  const mac = navigator.userAgent.includes("Mac OS");
  return !e.altKey && !e.shiftKey && (mac ? e.metaKey && !e.ctrlKey : e.ctrlKey && !e.metaKey) && (e.code === "KeyK" || e.key.toLowerCase() === "k");
}

/** dispatchKeyEvent, ahead of every workbench key handler. `refuse` is the workbench's own
 *  refusal for a key target or focus owner: a text control (native editing and the input method
 *  keep the key), an open menu, context menu or progress window. Every entry uses it. */
export function bindFunctionSearch(refuse: (target: EventTarget | null) => boolean): void {
  workbenchRefuses = refuse;
  window.addEventListener("keydown", (e) => {
    if (!dialog) cancelFocusRestore();
    if (dialog) {
      if (dialog.contains(e.target)) swallowedKeys.add(e.code);
      return;
    }
    if (e.repeat && swallowedKeys.has(e.code)) {
      e.preventDefault();
      e.stopImmediatePropagation();
      return;
    }
    if (!shortcut(e) || functionSearchRefused(e.target)) return;
    swallowedKeys.add(e.code);
    e.preventDefault();
    e.stopImmediatePropagation();
    if (openQueued) return;
    openQueued = true;
    setTimeout(() => {
      if (!openQueued) return;
      openQueued = false;
      void openFunctionSearch();
    });
  }, true);
  window.addEventListener("keyup", (e) => {
    if (!swallowedKeys.delete(e.code) || dialog) return;
    e.preventDefault();
    e.stopImmediatePropagation();
  }, true);
  // A press elsewhere is the user choosing the focus.
  window.addEventListener("pointerdown", () => {
    if (!dialog) cancelFocusRestore();
  }, true);
  window.addEventListener("blur", () => swallowedKeys.clear());
}

/** Acceptance probe: the open search, the last activation, and every catalog entry's destination
 *  as resolved now (read-only). `registered` marks ids the command catalog implements, so a check
 *  can require each to reach a real destination. */
export async function functionSearchReport() {
  const entries = await searchFunctions("", state.view?.settings.active_language ?? "ZhCn");
  return {
    open: dialog?.report() ?? null,
    lastActivation,
    entries: entries.map((m) => {
      const r = resolveFunction(m, dialog?.layer ?? null);
      return {
        id: m.id,
        target: m.target,
        kind: r.kind,
        missing: r.missing,
        reason: reasonKey(r.reason),
        dependency: reasonKey(r.dependency),
        registered: COMMANDS[m.id] !== undefined,
      };
    }),
  };
}
