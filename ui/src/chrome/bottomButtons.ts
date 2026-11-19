// 显示 > 工具栏 > 底部工具栏 > 自定义内容 (Menu.customToolbarItem, Menu.java:4896-5156). Each check
// writes its choice at once (no 确定); the buttons themselves are laid out in chrome/bottombar.ts.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { BottomButtonSettings } from "../contract/generated";

export type BottomButton = keyof BottomButtonSettings;

/** Config.badMoves's default (Config.java:1936), from the toolbar flags the program started with;
 *  decided once, at the first view. */
let badMovesDefault: boolean | null = null;

export function bottomButtonShown(button: BottomButton): boolean {
  const s = state.view?.settings;
  if (!s) return false;
  if (button !== "badMoves") return s.bottom_buttons[button];
  badMovesDefault ??= !(s.panels.showTopToolbar && s.panels.showDoubleMenu);
  return s.bottom_buttons.badMoves ?? badMovesDefault;
}

/** Each check flips what is shown now and saves it (Menu.java:4904-5156). */
function toggle(button: BottomButton): void {
  const saved = state.view?.settings.bottom_buttons;
  if (saved) void run({ type: "SetBottomButtons", settings: { ...saved, [button]: !bottomButtonShown(button) } });
}

function item(button: BottomButton): CommandEntry {
  return { run: () => toggle(button), checked: () => bottomButtonShown(button) };
}

Object.assign(COMMANDS, {
  "menu.customToolbar.liveButton": item("live"),
  "menu.customToolbar.kataEstimate": item("kataEstimate"),
  "menu.customToolbar.batchOpen": item("batchOpen"),
  "menu.customToolbar.openfile": item("openFile"),
  "menu.customToolbar.savefile": item("saveFile"),
  "menu.customToolbar.finalScore": item("finalScore"),
  "menu.customToolbar.heatMap": item("heatMap"),
  "menu.customToolbar.badMoves": item("badMoves"),
  "menu.customToolbar.analyzeList": item("analyzeList"),
  "menu.customToolbar.refresh": item("refresh"),
  "menu.customToolbar.analyse": item("analyse"),
  "menu.customToolbar.tryPlay": item("tryPlay"),
  "menu.customToolbar.setMain": item("setMain"),
  "menu.customToolbar.backMain": item("backMain"),
  "menu.customToolbar.clearButton": item("clearBoard"),
  "menu.customToolbar.deleteMove": item("deleteMove"),
  "menu.customToolbar.moveRank": item("moveRank"),
  "menu.customToolbar.move": item("moveNumbers"),
  "menu.customToolbar.coords": item("coords"),
  "menu.customToolbar.autoPlay": item("autoPlay"),
} satisfies Record<string, CommandEntry>);
