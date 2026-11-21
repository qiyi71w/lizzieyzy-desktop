// 显示 > 面板 > 分支面板设置 (Menu.java:1211-1293, states 2601-2612): whether the variation tree
// scrolls or is laid out whole in the panel (LizzieFrame.shouldShowSimpleVariation, 13664-13667).
// Board.hasBigBranch is display state: the scrolled tree of the open document reached 设置最大宽度.
// It holds for that document until another one opens or a maximum width is saved in 综合设置.
import type { DisplaySettings } from "../contract/generated";

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

/** 设置最大宽度 saved (Menu.java:1270, now the 综合设置 row): the too-wide mark clears and the tree
 *  is laid out again, even when the width is the same. */
export function clearTreeTooWide(): void {
  tooWide = null;
  cleared();
}
