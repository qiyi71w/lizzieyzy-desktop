// 显示 > 布局模式 (Menu.java:1839-1910), 浮动棋盘模式 (1912-1972), 面板 > 浮动主棋盘 / 浮动小棋盘
// (1367-1401), 自定义布局 (1974-2128) and their Alt shortcuts (Input.java:717-731, 784-829): the
// core switches the mode and its panels (SetExtraMode, SetClassicMode, ToggleIndependentBoard,
// SetPanelSettings for Java's raw flag changes); this module sets the window width each entry
// sets (LizzieFrame.java:3284-3535) and the 双引擎 hint after opening a record with secondary
// analysis (SGFParser.java:127-140).
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { ExtraMode, PanelSettings } from "../contract/generated";
import { confirm } from "../dialogs";
import { t } from "../i18n";
import { applyCustomLayout, openSetCustomMode } from "./customLayout";
import { holdIndependent } from "./independent";

type Fit = ExtraMode | "Classic";

/** Width per mode as a percentage of the outer height less the bottom toolbar (toolbarHeight). */
const PERCENT: Partial<Record<Fit, number>> = { Normal: 165, Classic: 145, FourSub: 162, Thinking: 166, FloatBoard: 65 };

/** The bounds each mode entry gives a window that is not maximized: it keeps its position; 双引擎
 *  is (height - toolbar - 65) * 2 wide; 简洁模式 is a square of the shorter side (less the toolbar)
 *  0.94 wide (minMode). setSize is the client area, so the frame's borders are taken off. */
async function fitWindow(fit: Fit): Promise<void> {
  const window = getCurrentWindow();
  if (await window.isMaximized()) return;
  const scale = await window.scaleFactor();
  const outer = (await window.outerSize()).toLogical(scale);
  const inner = (await window.innerSize()).toLogical(scale);
  const bar = document.getElementById("bottombar")?.offsetHeight ?? 0;
  const height = Math.trunc(outer.height);
  const dw = outer.width - inner.width;
  if (fit === "Min") {
    const least = Math.min(Math.trunc(outer.width), height - bar);
    await window.setSize(new LogicalSize(Math.trunc(least * 0.94) - dw, least + bar - (outer.height - inner.height)));
    return;
  }
  const width = fit === "DoubleEngine" ? (height - bar - 65) * 2 : Math.trunc(((height - bar) * PERCENT[fit]!) / 100);
  await window.setSize(new LogicalSize(width - dw, inner.height));
}

/** A user's layout choice: the window takes the mode's width even when the mode was already
 *  chosen, as Java's mode entries always set the bounds. */
export async function setExtraMode(mode: ExtraMode): Promise<void> {
  const result = await holdIndependent(() => run({ type: "SetExtraMode", mode }));
  if (result?.type === "Done" || result?.type === "Unchanged") await fitWindow(mode);
}

function panels(): PanelSettings | undefined {
  return state.view?.settings.panels;
}

/** LizzieFrame.classicMode. */
async function classicMode(): Promise<void> {
  const result = await run({ type: "SetClassicMode" });
  if (result?.type === "Done" || result?.type === "Unchanged") await fitWindow("Classic");
}

/** 浮动棋盘模式 > 浮动主棋盘: independentBoardMode(true) (SetExtraMode(FloatBoard)'s panels with the
 *  in-frame sub board kept), then the floating sub board closes. */
async function floatMainOnly(): Promise<void> {
  const p = panels();
  if (!p) return;
  await holdIndependent(async () => {
    await run({ type: "SetExtraMode", mode: "FloatBoard" });
    await run({
      type: "SetPanelSettings",
      settings: { ...p, showWinrateGraph: true, showComment: true, showCaptured: true, showVariationGraph: true, showListPane: true, showSubBoard: true, showingIndependentMain: true, showingIndependentSub: false },
    });
  });
  await fitWindow("FloatBoard");
}

/** 浮动棋盘模式 > 浮动小棋盘: the in-frame sub board toggles, the floating sub board opens and the
 *  floating main board closes; the mode is unchanged. */
function floatSubOnly(): void {
  const p = panels();
  if (p) void run({ type: "SetPanelSettings", settings: { ...p, showSubBoard: !p.showSubBoard, showingIndependentSub: true, showingIndependentMain: false } });
}

/** 浮动棋盘模式 > 额外浮动棋盘: defaultMode, then both floating boards open. */
async function extraFloatBoards(): Promise<void> {
  const p = panels();
  if (!p) return;
  await holdIndependent(async () => {
    await run({ type: "SetExtraMode", mode: "Normal" });
    await run({
      type: "SetPanelSettings",
      settings: { ...p, classicMode: false, showSubBoard: true, showListPane: true, showWinrateGraph: true, showComment: true, showCaptured: true, showVariationGraph: true, largeSubBoard: false, largeWinrateGraph: false, showingIndependentMain: true, showingIndependentSub: true },
    });
  });
  await fitWindow("Normal");
}

/** 面板 > 浮动主棋盘(Alt+Q) (toggleOnlyIndependMainBoard, LizzieFrame.java:3415-3421, 3435-3440):
 *  in 浮动棋盘模式 back to the raw Normal mode with the floating board toggled; otherwise the raw
 *  浮动棋盘模式 with the floating board shown and the other panels as they are. */
async function toggleOnlyFloatMain(): Promise<void> {
  const v = state.view;
  if (!v) return;
  const p = v.settings.panels;
  await holdIndependent(async () => {
    if (v.settings.extra_mode === "FloatBoard") {
      // ToggleIndependentBoard leaves 浮动棋盘模式 for Normal with the window kept open.
      if (p.showingIndependentMain) {
        await run({ type: "ToggleIndependentBoard", board: "Main" });
        const after = state.view?.settings.panels ?? p;
        await run({ type: "SetPanelSettings", settings: { ...after, showingIndependentMain: false } });
      } else {
        await run({ type: "ToggleIndependentBoard", board: "Main" });
        await run({ type: "ToggleIndependentBoard", board: "Main" });
      }
    } else {
      await run({ type: "SetExtraMode", mode: "FloatBoard" });
      await run({ type: "SetPanelSettings", settings: { ...p, showingIndependentMain: true } });
    }
  });
}

/** Alt+Q (Input.java:721-722): LizzieFrame.toggleIndependentMainBoard, the window alone. */
function toggleFloatMainWindow(): void {
  const p = panels();
  if (p) void run({ type: "SetPanelSettings", settings: { ...p, showingIndependentMain: !p.showingIndependentMain } });
}

const mode = () => state.view?.settings.extra_mode;

Object.assign(COMMANDS, {
  "Menu.defaultView": { run: () => void setExtraMode("Normal") },
  "Menu.classicView": { run: () => void classicMode() },
  "Menu.minView": { run: () => void setExtraMode("Min") },
  "Menu.extraMode3": { run: () => void setExtraMode("Thinking"), checked: () => mode() === "Thinking" },
  "Menu.extraMode1": { run: () => void setExtraMode("FourSub"), checked: () => mode() === "FourSub" },
  "Menu.extraMode2": { run: () => void setExtraMode("DoubleEngine"), checked: () => mode() === "DoubleEngine" },
  "Menu.indepBothBoard": { run: () => void setExtraMode("FloatBoard") },
  "Menu.indepMainBoard": { run: () => void floatMainOnly() },
  "Menu.indepSubBoard": { run: floatSubOnly },
  "Menu.indepExtraBoard": { run: () => void extraFloatBoards() },
  // Checked as the menu listener marks them (Menu.java:2811-2824).
  "menu.independentMainBoard": { run: () => void toggleOnlyFloatMain(), checked: () => !!panels()?.showingIndependentMain && mode() === "FloatBoard" },
  "menu.independentMainBoard2": {
    run: () => void holdIndependent(() => run({ type: "ToggleIndependentBoard", board: "Main" })),
    checked: () => !!panels()?.showingIndependentMain && mode() !== "FloatBoard",
  },
  "menu.independentSubBoard": { run: () => void run({ type: "ToggleIndependentBoard", board: "Sub" }), checked: () => !!panels()?.showingIndependentSub },
  "menu.custom-layout-1": { run: () => applyCustomLayout(1), button: { label: () => t("Menu.customLayoutSet"), run: () => openSetCustomMode(1) } },
  "menu.custom-layout-2": { run: () => applyCustomLayout(2), button: { label: () => t("Menu.customLayoutSet"), run: () => openSetCustomMode(2) } },
  "menu.visualizedPanelSettings": { run: () => openSetCustomMode(null) },
} satisfies Record<string, CommandEntry>);

/** Alt+1..9, Alt+Q and Alt+A (Input.java:717-731, 784-829). Ctrl+9 switches engines and Ctrl+Q /
 *  Ctrl+A belong to other features; Alt+A does nothing while syncing a board (not in this product). */
export function handleLayoutKey(e: KeyboardEvent): boolean {
  if (!e.altKey || e.metaKey) return false;
  const ctrl = e.ctrlKey;
  const ids: Record<string, string> = {
    Digit1: "Menu.defaultView",
    Digit2: "Menu.classicView",
    Digit3: "Menu.minView",
    Digit4: "Menu.extraMode3",
    Digit5: "Menu.extraMode1",
    Digit6: "Menu.extraMode2",
    Digit7: "Menu.indepBothBoard",
    Digit8: "menu.custom-layout-1",
  };
  if (e.code === "Digit9" && !ctrl) applyCustomLayout(2);
  else if (e.code === "KeyQ" && !ctrl) toggleFloatMainWindow();
  else if (e.code === "KeyA" && !ctrl) void run({ type: "ToggleIndependentBoard", board: "Sub" });
  else if (ids[e.code]) COMMANDS[ids[e.code]].run();
  else return false;
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
