// 显示 > 工具栏 > 顶部工具栏 > 自定义内容 (Menu.customTopToolBar, Menu.java:4639-4846): the top
// toolbar's optional groups. Each check item writes its choice at once (no 确定); the groups
// themselves are built in chrome/toolbar.ts.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { TopToolbarSettings } from "../contract/generated";
import { renderToolbar } from "./toolbar";

export type TopGroup = keyof TopToolbarSettings;

/** Function search shows a hidden group for this run only (Menu.locateTopFunction sets the
 *  running Config field); the saved choice stays as it is. */
const revealed: Partial<Record<TopGroup, boolean>> = {};

/** The group function search locates each Java top-toolbar action id in. */
export const TOP_GROUP_OF: Record<string, TopGroup> = {
  "toolbar.time-limit": "showTimeLimit",
  "toolbar.visit-limit": "showVisitLimit",
  "toolbar.analyze-sides": "showAnalyzeSides",
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

Object.assign(COMMANDS, {
  "menu.showTimeLimitController": item("showTimeLimit"),
  "menu.showPlayoutLimitController": item("showVisitLimit"),
  "menu.showAnalyzeController": item("showAnalyzeSides"),
  "menu.showCandidateInfoController": item("showCandidateInfo"),
} satisfies Record<string, CommandEntry>);
