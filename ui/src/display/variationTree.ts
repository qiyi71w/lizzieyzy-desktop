// 显示 > 面板 > 分支面板设置 (Menu.java:1211-1293, states 2601-2612): whether the variation tree
// scrolls or is laid out whole in the panel (LizzieFrame.shouldShowSimpleVariation, 13664-13667).
// Board.hasBigBranch is display state: the scrolled tree of the open document reached 设置最大宽度.
// It holds for that document until another one opens or the maximum width is confirmed again.
import { state } from "../app";
import { setDisplay } from "../commands";
import type { DisplaySettings } from "../contract/generated";
import { ask } from "../dialogs";
import { t } from "../i18n";
import { digitsOnly, parseJavaInt } from "./dialogs";

/** The document whose scrolled tree reached the maximum width (Board.setBigBranch), and that
 *  width: confirming another one clears the mark (Menu.java:1270). */
let tooWide: { document: number; width: number } | null = null;
/** Repaints the tree when the mark clears; the setting may not have changed. */
let cleared = () => {};

export function onTreeTooWideCleared(listener: () => void): void {
  cleared = listener;
}

/** Board.setBigBranch for the open document. */
export function markTreeTooWide(document: number, width: number): void {
  tooWide = { document, width };
}

/** shouldShowSimpleVariation: always without the scroll pane; with it, once the tree was too wide
 *  unless 超过宽度依然使用滚动条 is on. */
export function treeFit(display: DisplaySettings, document: number): boolean {
  const big = tooWide?.document === document && tooWide.width === display.maxTreeWidth;
  return !display.scrollVariation || (!display.ignoreOutOfWidth && big);
}

/** 设置最大宽度 (Menu.java:1223-1277): the hint, 最大宽度(默认10000): and an IntDocument field;
 *  确定 saves the typed int (parseTextToInt keeps the old one otherwise), clears the too-wide mark
 *  and lays the tree out again. */
export async function chooseMaxTreeWidth(): Promise<void> {
  const s = state.view?.settings.display;
  if (!s) return;
  const body = document.createElement("div");
  body.className = "display-input";
  const hint = document.createElement("div");
  hint.textContent = t("Menu.maxTreeWidthHint");
  const label = document.createElement("div");
  label.textContent = t("Menu.lblMaxTreeWidth");
  label.style.marginTop = "10px";
  const field = document.createElement("input");
  field.type = "text";
  field.value = String(s.maxTreeWidth);
  field.setAttribute("aria-label", t("Menu.lblMaxTreeWidth"));
  field.dataset.key = "menu.maxTreeWidth.field";
  digitsOnly(field);
  body.append(hint, label, field);
  queueMicrotask(() => field.focus());
  if (!(await ask(t("Menu.setMaxTreeWidthTitle"), body, t("LizzieFrame.confirm"), t("LizzieFrame.cancel")))) return;
  tooWide = null;
  setDisplay({ maxTreeWidth: parseJavaInt(field.value.trim()) ?? s.maxTreeWidth });
  cleared();
}
