// 显示 > 工具栏 > 顶部工具栏 > 自定义内容 (Menu.customTopToolBar, Menu.java:4639-4847) and 自动换行
// (Menu.java:4457-4467): the top toolbar's groups and wrapping. Each check item writes its choice
// at once (no 确定); the groups themselves are built in chrome/toolbar.ts.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { TopToolbarSettings } from "../contract/generated";
import { renderToolbar } from "./toolbar";

export type TopGroup = Exclude<keyof TopToolbarSettings, "autoWrap">;

/** Function search shows a hidden group for this run only (Menu.locateTopFunction sets the
 *  running Config field); the saved choice stays as it is. */
const revealed: Partial<Record<TopGroup, boolean>> = {};

/** The group function search locates each Java top-toolbar action id in (Menu.locateTopFunction,
 *  Menu.java:6981-7002: the other ids there are the basic buttons'). 激进度 / 广度 are the
 *  engine's groups (search/targets.ts KATA_GROUPS). */
export const TOP_GROUP_OF: Record<string, TopGroup> = {
  "toolbar.change-turn": "showBasicButtons",
  "toolbar.markup-tools": "showBasicButtons",
  "toolbar.markup-letter": "showBasicButtons",
  "toolbar.markup-number": "showBasicButtons",
  "toolbar.markup-cross": "showBasicButtons",
  "toolbar.markup-square": "showBasicButtons",
  "toolbar.markup-triangle": "showBasicButtons",
  "toolbar.markup-eraser": "showBasicButtons",
  "toolbar.markup-clear": "showBasicButtons",
  "toolbar.painting": "showBasicButtons",
  "toolbar.force-allow": "showForce",
  "toolbar.force-avoid": "showForce",
  "toolbar.force-clear": "showForce",
  "toolbar.force-allow-options": "showForce",
  "toolbar.force-avoid-options": "showForce",
  "toolbar.game-pause": "showGameControls",
  "toolbar.game-stop": "showGameControls",
  "toolbar.game-resign": "showGameControls",
  "toolbar.time-limit": "showTimeLimit",
  "toolbar.visit-limit": "showVisitLimit",
  "toolbar.analyze-sides": "showAnalyzeSides",
  "toolbar.candidate-sides": "showCandidateSides",
  "toolbar.candidate-info": "showCandidateInfo",
};

export function topGroupShown(group: TopGroup): boolean {
  return revealed[group] === true || state.view?.settings.top_toolbar[group] === true;
}

/** Shows `group` for this run; returns whether it was hidden. */
export function revealTopGroup(group: TopGroup): boolean {
  if (topGroupShown(group)) return false;
  revealed[group] = true;
  return true;
}

/** The check item flips what is shown now and saves it, ending a reveal. Unticking a revealed
 *  group saves no change, so the toolbar is redrawn here rather than by a published view. */
function toggle(group: TopGroup): void {
  const saved = state.view?.settings.top_toolbar;
  if (!saved) return;
  const shown = topGroupShown(group);
  revealed[group] = false;
  renderToolbar();
  void run({ type: "SetTopToolbarSettings", settings: { ...saved, [group]: !shown } });
}

function item(group: TopGroup): CommandEntry {
  return { run: () => toggle(group), checked: () => topGroupShown(group) };
}

/** 自动换行 (Config.autoWrapToolBar): off, the row stays one line and what does not fit is cut off. */
export function topToolbarWraps(): boolean {
  return state.view?.settings.top_toolbar.autoWrap !== false;
}

function toggleAutoWrap(): void {
  const saved = state.view?.settings.top_toolbar;
  if (saved) void run({ type: "SetTopToolbarSettings", settings: { ...saved, autoWrap: !saved.autoWrap } });
}

Object.assign(COMMANDS, {
  "menu.autoWrap": { run: toggleAutoWrap, checked: topToolbarWraps },
  "menu.showDoubleMenuBtn": item("showDoubleMenuButton"),
  "menu.showBasicBtn": item("showBasicButtons"),
  "menu.playMoveTools": item("showEditBar"),
  "menu.allowAvoidTools": item("showForce"),
  "menu.showGameController": item("showGameControls"),
  "menu.showTimeLimitController": item("showTimeLimit"),
  "menu.showPlayoutLimitController": item("showVisitLimit"),
  "menu.showAnalyzeController": item("showAnalyzeSides"),
  "menu.showCandidateController": item("showCandidateSides"),
  "menu.showCandidateInfoController": item("showCandidateInfo"),
  "menu.showRuleMenu": item("showRules"),
  "menu.showParamMenu": item("showParameters"),
  "menu.showGobanMenu": item("showBoardSize"),
  "menu.showSaveLoadMenu": item("showSaveLoad"),
} satisfies Record<string, CommandEntry>);
