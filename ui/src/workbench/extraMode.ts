// 显示 > 布局模式 (Menu.java:1899-1910): 默认模式 (Alt+1, LizzieFrame.defaultMode) and 双引擎模式
// (Alt+6, Config.toggleExtraMode(2) → LizzieFrame.extraMode), the window width each mode change
// sets, and the 双引擎 hint after opening a record with secondary analysis (SGFParser.java:127-140).
// The mode itself is a core setting (SetExtraMode); only the window placement is done here.
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { ExtraMode } from "../contract/generated";
import { confirm } from "../dialogs";
import { t } from "../i18n";

/** LizzieFrame.extraMode (LizzieFrame.java:3333-3338) and defaultMode (3526-3531): a window that
 *  is not maximized keeps its position and height and takes the mode's width, from the outer
 *  height less the bottom toolbar (toolbarHeight). */
async function fitWindow(mode: ExtraMode): Promise<void> {
  const window = getCurrentWindow();
  if (await window.isMaximized()) return;
  const scale = await window.scaleFactor();
  const outer = (await window.outerSize()).toLogical(scale);
  const inner = (await window.innerSize()).toLogical(scale);
  const bar = document.getElementById("bottombar")?.offsetHeight ?? 0;
  const height = Math.trunc(outer.height);
  const width = mode === "DoubleEngine" ? (height - bar - 65) * 2 : Math.trunc(((height - bar) * 165) / 100);
  // setSize is the client area: the frame's borders keep their width.
  await window.setSize(new LogicalSize(width - (outer.width - inner.width), inner.height));
}

/** A user's layout choice: the core switches the mode; the window takes the mode's width even
 *  when the mode was already chosen, as Java's defaultMode / extraMode always set the bounds. */
export async function setExtraMode(mode: ExtraMode): Promise<void> {
  const result = await run({ type: "SetExtraMode", mode });
  if (result?.type === "Done" || result?.type === "Unchanged") await fitWindow(mode);
}

/** Alt+1 / Alt+6 (Input.java:784-813): Alt with any other modifier. */
export function handleLayoutKey(e: KeyboardEvent): boolean {
  if (!e.altKey || e.metaKey) return false;
  const id = e.code === "Digit1" ? "Menu.defaultView" : e.code === "Digit6" ? "Menu.extraMode2" : null;
  if (!id) return false;
  COMMANDS[id].run();
  return true;
}

// SGFParser.loadFile / loadFromString: JOptionPane OK_CANCEL; OK switches to 双引擎模式.
registerViewHandler((view) => {
  const outcome = view.open_outcome;
  if (outcome?.type !== "Adopted" || !outcome.double_engine_hint) return;
  void confirm(t("SGFParse.doubleEngineHintTitle"), t("SGFParse.doubleEngineHint")).then((ok) => {
    if (ok) void setExtraMode("DoubleEngine");
  });
});

Object.assign(COMMANDS, {
  "Menu.defaultView": { run: () => void setExtraMode("Normal") },
  "Menu.extraMode2": { run: () => void setExtraMode("DoubleEngine"), checked: () => state.view?.settings.extra_mode === "DoubleEngine" },
} satisfies Record<string, CommandEntry>);
