// What the workbench lays out: the committed layout mode and panel flags, or while 自定义布局 >
// 设置 / 可视化面板设置 (SetCustomMode) is open, its uncommitted draft. Java's dialog writes the
// Config fields directly so the frame and the floating boards follow each checkbox at once, and
// 取消 puts the saved values back (SetCustomMode.restoreTempConfig); here the draft is a UI layer
// over the core projection and only 确定 commits it.
import type { ExtraMode, PanelSettings, WorkbenchView } from "../contract/generated";

export interface Presentation {
  mode: ExtraMode;
  panels: PanelSettings;
  /** The 主棋盘位置 slider (BoardPositionProportion, 0..8). */
  boardPositionProportion: number;
}

let draft: Presentation | null = null;
let changed: () => void = () => {};

/** The draft shown instead of the committed presentation, or null to show the view again. */
export function setPresentationDraft(next: Presentation | null): void {
  draft = next;
  changed();
}

export function presentationDraft(): Presentation | null {
  return draft;
}

/** Called when the draft changes, so the workbench repaints. */
export function onPresentationChange(repaint: () => void): void {
  changed = repaint;
}

export function presentation(v: WorkbenchView): Presentation {
  return draft ?? { mode: v.settings.extra_mode, panels: v.settings.panels, boardPositionProportion: v.settings.panel_sizes.boardPositionProportion };
}

/** Config.showListPane() (Config.java:2538-2546): the list is kept in 四方图 / 思考 / 浮动棋盘;
 *  in 默认 / 简洁 大胜率图 hides it without forgetting it; 双引擎模式 has none. */
export function listShown(mode: ExtraMode, p: PanelSettings): boolean {
  if (!p.showListPane) return false;
  if (mode === "FourSub" || mode === "Thinking" || mode === "FloatBoard") return true;
  return (mode === "Normal" || mode === "Min") && !p.largeWinrateGraph;
}

/** Whether the status lines are drawn: 经典模式 sets showStatus false for the session without
 *  writing the preference (LizzieFrame.classicMode, Config.java:1609), and 简洁模式 never draws
 *  them (`!isMinMode()`, LizzieFrame.java:7459-7532). */
export function statusShown(mode: ExtraMode, p: PanelSettings): boolean {
  return p.showStatus && !p.classicMode && mode !== "Min";
}
