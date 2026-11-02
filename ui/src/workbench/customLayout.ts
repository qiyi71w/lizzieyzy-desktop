// 显示 > 自定义布局1/2 (Alt+8 / Alt+9, Menu.java:1974-2128) and their 设置 buttons, and 面板 >
// 可视化面板设置 (Menu.visualizedPanelSettings): LizzieFrame.switchToCustomMode applies a saved
// snapshot (Config.loadCustomLayout, done by the core and src-tauri, which also places the
// windows); SetCustomMode (SetCustomMode.java) edits the layout live and 确定 saves it.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { run, state } from "../app";
import type { WindowPlacement } from "../contract/generated";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { holdIndependent } from "./independent";
import { listShown, presentation, setPresentationDraft, statusShown, type Presentation } from "./presentation";

/** The main window's outer bounds in physical pixels, as Config.saveCustomMode records the frame
 *  (`main-frame-position`, `main-frame-maxsize`). */
async function framePlacement(): Promise<WindowPlacement> {
  const w = getCurrentWindow();
  const [pos, size, maximized] = await Promise.all([w.outerPosition(), w.outerSize(), w.isMaximized()]);
  return { x: pos.x, y: pos.y, width: size.width, height: size.height, maximized };
}

/** switchToCustomMode (LizzieFrame.java:21068-21072): an empty slot does nothing. */
export function applyCustomLayout(slot: 1 | 2): void {
  void holdIndependent(() => run({ type: "ApplyCustomLayout", slot }));
}

/** Config.loadCustomLayout over the shown presentation: the nine panel flags, the board position,
 *  浮动棋盘模式 exactly when the snapshot had the floating main board, and the floating windows it
 *  had opened (existing ones stay open). */
function loadSnapshot(base: Presentation, slot: 1 | 2): Presentation {
  const layout = state.view?.settings.custom_layouts[slot - 1];
  if (!layout) return base;
  const s = layout.panels;
  return {
    mode: s.showingIndependentMain ? "FloatBoard" : "Normal",
    boardPositionProportion: layout.board_position_proportion,
    panels: {
      ...base.panels,
      showSubBoard: s.showSubBoard,
      showWinrateGraph: s.showWinrateGraph,
      showComment: s.showComment,
      showStatus: s.showStatus,
      showVariationGraph: s.showVariationGraph,
      showCaptured: s.showCaptured,
      showListPane: s.showListPane,
      largeSubBoard: s.largeSubBoard,
      largeWinrateGraph: s.largeWinrateGraph,
      showingIndependentMain: base.panels.showingIndependentMain || s.showingIndependentMain,
      showingIndependentSub: base.panels.showingIndependentSub || s.showingIndependentSub,
    },
  };
}

let openDialog = false;

/** SetCustomMode(index, isSetCustom): `slot` null is 可视化面板设置, which saves the panels only. */
export function openSetCustomMode(slot: 1 | 2 | null): void {
  const v = state.view;
  if (!v || openDialog) return;
  openDialog = true;
  // The dialog stops pondering while it is open and resumes it afterwards (oriPondering).
  const pondering = v.engine.analysing;
  if (pondering) void run({ type: "ToggleAnalysis" });
  const committed = presentation(v);
  const d: Presentation = slot === null ? { ...committed, panels: { ...committed.panels } } : loadSnapshot(committed, slot);
  const p = d.panels;
  const show = () => setPresentationDraft({ ...d, panels: { ...p } });
  show();

  const grid = document.createElement("div");
  grid.className = "custom-mode-grid";
  const row = (key: string, control: HTMLInputElement, tooltip?: string) => {
    const label = document.createElement("label");
    label.textContent = t(key);
    if (tooltip) label.title = tooltip;
    label.htmlFor = control.id = `custom-mode-${key}`;
    grid.append(label, control);
    return control;
  };
  const box = (key: string, checked: boolean, change: (on: boolean) => void, tooltip?: string) => {
    const c = document.createElement("input");
    c.type = "checkbox";
    c.checked = checked;
    if (tooltip) c.title = tooltip;
    c.addEventListener("change", () => {
      change(c.checked);
      show();
    });
    return row(key, c, tooltip);
  };
  const autoHidden = () => d.mode === "FourSub" || d.mode === "DoubleEngine";
  const commentTip = () => (autoHidden() ? t("LizzieConfig.title.showComment.autoHidden.tooltip") : t("SetCustomMode.commentPanel.tooltip"));

  const subBoard = box("SetCustomMode.subBoard", p.showSubBoard, (on) => {
    p.showSubBoard = on;
    if (!on) bigSubBoard.checked = false;
  });
  const winrate = box("SetCustomMode.winratePanel", p.showWinrateGraph, (on) => {
    p.showWinrateGraph = on;
    if (!on) {
      bigWinrate.checked = false;
      if (!bigSubBoard.checked) slider.disabled = false;
    }
  });
  // setShowComment applies the mode policy; syncCommentPanelControl (SetCustomMode.java:436-445).
  const comment = box("SetCustomMode.commentPanel", p.showComment, (on) => {
    p.showComment = on && !autoHidden();
    syncComment();
  }, commentTip());
  const syncComment = () => {
    comment.checked = p.showComment;
    comment.disabled = autoHidden();
    comment.title = commentTip();
  };
  syncComment();
  box("SetCustomMode.variationPanel", p.showVariationGraph, (on) => {
    p.showVariationGraph = on;
  });
  // Config.toggleShowListPane: showing the list leaves 大胜率图 (and 双引擎模式 for 默认模式).
  const list = box("SetCustomMode.listPanel", p.showListPane, (on) => {
    if (listShown(d.mode, p) !== on) {
      p.showListPane = on;
      if (on && d.mode !== "FloatBoard" && d.mode !== "Thinking" && d.mode !== "FourSub") {
        if (d.mode === "DoubleEngine") {
          // LizzieFrame.defaultMode.
          d.mode = "Normal";
          Object.assign(p, { classicMode: false, showSubBoard: true, showWinrateGraph: true, showComment: true, showCaptured: true, showVariationGraph: true, largeSubBoard: false });
        }
        p.largeWinrateGraph = false;
      }
    }
    if (listShown(d.mode, p)) {
      bigWinrate.checked = false;
      if (!bigSubBoard.checked) slider.disabled = false;
    }
  });
  box("SetCustomMode.informationPanel", p.showCaptured, (on) => {
    p.showCaptured = on;
  });
  // Config.showStatus is false in 经典模式 (it is not the saved preference).
  box("SetCustomMode.statusPanel", statusShown("Normal", p), (on) => {
    p.showStatus = on;
    if (on) p.classicMode = false;
  });
  const bigSubBoard = box("SetCustomMode.bigSubBoard", p.showSubBoard && p.largeSubBoard, (on) => {
    p.largeSubBoard = on;
    if (on) {
      p.showSubBoard = true;
      bigWinrate.checked = false;
      subBoard.checked = true;
      slider.disabled = true;
    } else if (!bigWinrate.checked) slider.disabled = false;
  });
  const bigWinrate = box("SetCustomMode.bigWinrate", p.showWinrateGraph && p.largeWinrateGraph, (on) => {
    p.largeWinrateGraph = on;
    if (on) {
      p.largeSubBoard = false;
      p.showWinrateGraph = true;
      bigSubBoard.checked = false;
      list.checked = false;
      p.showListPane = false;
      winrate.checked = true;
      slider.disabled = true;
    } else slider.disabled = false;
  });
  // JSlider 0..8 with snapping ticks.
  const slider = document.createElement("input");
  slider.type = "range";
  slider.min = "0";
  slider.max = "8";
  slider.step = "1";
  slider.value = String(d.boardPositionProportion);
  slider.addEventListener("input", () => {
    d.boardPositionProportion = Number(slider.value);
    show();
  });
  row("SetCustomMode.sldBoardPositionProportion", slider);
  // EngineManager.publishExtraMode: the mode changes without the mode's panel defaults.
  const floatMain = box("SetCustomMode.floatMainBoard", d.mode === "FloatBoard", (on) => {
    d.mode = on ? "FloatBoard" : "Normal";
    p.showingIndependentMain = on;
    for (const c of [bigWinrate, bigSubBoard]) {
      if (on) c.checked = false;
      c.disabled = on;
    }
    syncComment();
  });
  const floatSub = box("SetCustomMode.floatSubBoard", p.showingIndependentSub, (on) => {
    p.showingIndependentSub = on;
    p.showSubBoard = !on;
    subBoard.checked = !on;
    subBoard.disabled = on;
  });
  if (floatMain.checked) {
    for (const c of [bigWinrate, bigSubBoard]) {
      c.checked = false;
      c.disabled = true;
    }
  }
  if (floatSub.checked) {
    subBoard.checked = false;
    subBoard.disabled = true;
  }
  if (listShown(d.mode, p)) bigWinrate.checked = false;
  if (p.showWinrateGraph && p.largeWinrateGraph) list.checked = false;
  if ((p.showSubBoard && p.largeSubBoard) || (p.showWinrateGraph && p.largeWinrateGraph)) slider.disabled = true;

  const buttons = document.createElement("div");
  buttons.className = "modal-buttons";
  const ok = document.createElement("button");
  ok.className = "primary";
  ok.textContent = t("SetKataEngines.btnApply");
  const cancel = document.createElement("button");
  cancel.textContent = t("SetKataEngines.btnCancel");
  buttons.append(ok, cancel);
  const body = document.createElement("div");
  body.append(grid, buttons);

  let confirmed = false;
  const finish = async () => {
    if (confirmed) await commit(d, slot);
    setPresentationDraft(null);
    openDialog = false;
    if (pondering) void run({ type: "ToggleAnalysis" });
    return true;
  };
  const dialog = formPanel(slot === null ? t("VisualizedPanelSettings.title") : `${t("SetCustomMode.title")}${slot}`, body, finish);
  ok.onclick = () => {
    confirmed = true;
    void dialog.close();
  };
  // 取消, Escape and the close box put the saved layout back (restoreTempConfig).
  cancel.onclick = () => void dialog.close();
}

/** 确定: Config.savePanelConfig, then Config.saveCustomMode for a slot. The mode is set first
 *  (its entry defaults are then replaced by the dialog's flags, as Java changes only the mode). */
async function commit(d: Presentation, slot: 1 | 2 | null): Promise<void> {
  await holdIndependent(async () => {
    const v = state.view;
    if (!v) return;
    if (d.mode !== v.settings.extra_mode) await run({ type: "SetExtraMode", mode: d.mode });
    await run({ type: "SetPanelSettings", settings: d.panels });
    const sizes = state.view?.settings.panel_sizes ?? v.settings.panel_sizes;
    if (d.boardPositionProportion !== sizes.boardPositionProportion)
      await run({ type: "SetPanelSizes", sizes: { ...sizes, boardPositionProportion: d.boardPositionProportion, leftoverLeftShare: null } });
    if (slot !== null) await run({ type: "SaveCustomLayout", slot, window: await framePlacement() });
  });
}
