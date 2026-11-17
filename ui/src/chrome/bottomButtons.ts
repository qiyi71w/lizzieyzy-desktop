// 显示 > 工具栏 > 底部工具栏 > 自定义内容 (Menu.customToolbarItem, Menu.java:4896-5156) and the
// 详细工具栏(过时的)(仅中文) items 显示拓展/隐藏详细工具栏按钮 and 设置模块顺序 (4593-4615). Each check
// writes its choice at once (no 确定); the buttons themselves are laid out in chrome/bottombar.ts.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { BottomButtonSettings, DetailPanelOrder } from "../contract/generated";
import { ask } from "../dialogs";
import { isChinese } from "../i18n";

export type BottomButton = Exclude<keyof BottomButtonSettings, "detailButton">;

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

/** BottomToolbar.showDetail (BottomToolbar.java:76, restShowDetail 2631-2639). */
export function detailButtonShown(): boolean {
  return !!state.view?.settings.bottom_buttons.detailButton && isChinese();
}

/** Each check flips what is shown now and saves it (Menu.java:4904-5156). */
function toggle(button: BottomButton): void {
  const saved = state.view?.settings.bottom_buttons;
  if (saved) void run({ type: "SetBottomButtons", settings: { ...saved, [button]: !bottomButtonShown(button) } });
}

function item(button: BottomButton): CommandEntry {
  return { run: () => toggle(button), checked: () => bottomButtonShown(button) };
}

function toggleDetailButton(): void {
  const saved = state.view?.settings.bottom_buttons;
  if (saved) void run({ type: "SetBottomButtons", settings: { ...saved, detailButton: !saved.detailButton } });
}

const PANELS: [label: string, field: keyof DetailPanelOrder][] = [
  ["自动分析面板:", "autoAnalysis"],
  ["引擎对战面板:", "engineMatch"],
  ["自动落子面板:", "autoPlay"],
];

/** ToolbarPositionConfig (ToolbarPositionConfig.java:12-80): a place 1–3 for each panel; 确认
 *  applies and saves them, 取消 leaves them. */
async function openPanelOrder(): Promise<void> {
  const saved = state.view?.settings.detail_panel_order;
  if (!saved) return;
  const body = document.createElement("div");
  body.className = "panel-order";
  const boxes = PANELS.map(([text, field]) => {
    const row = document.createElement("label");
    const box = document.createElement("select");
    box.dataset.key = `panelOrder.${field}`;
    box.append(...["1", "2", "3"].map((n) => Object.assign(document.createElement("option"), { textContent: n })));
    box.selectedIndex = saved[field];
    row.append(text, box);
    body.append(row);
    return [field, box] as const;
  });
  if (!(await ask("显示顺序设置", body, "确认", "取消"))) return;
  const order = { ...saved };
  for (const [field, box] of boxes) order[field] = box.selectedIndex;
  void run({ type: "SetDetailPanelOrder", order });
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
  "menu.showDetailedToolbarMenu": { run: toggleDetailButton, checked: () => !!state.view?.settings.bottom_buttons.detailButton },
  "menu.detailedToolbarOrder": { run: () => void openPanelOrder() },
} satisfies Record<string, CommandEntry>);
