// 独立棋局控制面板 entries (视图 > 面板 > 独立棋局控制面板, the top toolbar's control button;
// LizzieFrame.openController) and its buttons. The shell owns the window, its bounds and whether
// it reopens at start; this module toggles it, follows whether it is shown for the menu check and
// runs the action of each button the window reports (Controller.java:35-124).
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { redo, undo } from "../analysis/table";
import { COMMANDS, type CommandEntry } from "../commands";
import { dialogOpen } from "../dialogs";
import { language, t } from "../i18n";
import { CONTROL_CLOSED_EVENT, CONTROL_DATA_EVENT, CONTROL_INPUT_EVENT, type ControlButton, type ControlInput, type ControlReport } from "./model";

/** src-tauri control_panel::LABEL. */
const LABEL = "control-panel";

let shown = false;
let report: ControlReport | null = null;

function toggle(): void {
  void invoke<boolean>("toggle_control_panel", { title: t("Controller.title") }).then((s) => (shown = s));
}

const ACTIONS: Record<ControlButton, () => void> = {
  first: () => COMMANDS["menu.jumpToFirst"].run(),
  last: () => COMMANDS["menu.jumpToLast"].run(),
  // LizzieFrame.undo / redo (Controller.java:57-99): a shown variation steps first.
  back5: () => undo(5),
  forward5: () => redo(5),
  back1: () => undo(1),
  forward1: () => redo(1),
  backToMain: () => COMMANDS["menu.backToMainBranch"].run(),
  // drawPainting's layer is a dialog that takes the focus; here it is in the main window.
  painting: () => {
    void getCurrentWindow().setFocus();
    COMMANDS["toolbar.painting"].run();
  },
};

function input(i: ControlInput): void {
  if (i.type === "ready") void emitTo(LABEL, CONTROL_DATA_EVENT, { language: language() });
  // A Java modal dialog (ConfigDialog2, FirstUseSettings, JOptionPane) blocks the Controller
  // window too: its buttons do nothing until the dialog closes.
  else if (i.type === "button") { if (!dialogOpen()) ACTIONS[i.button](); }
  else report = i.report;
}

/** Listens to the window and the shell; the panel shown at the last exit opens again. */
export async function startControlPanel(): Promise<void> {
  await listen<ControlInput>(CONTROL_INPUT_EVENT, (e) => input(e.payload));
  await listen(CONTROL_CLOSED_EVENT, () => (shown = false));
  shown = await invoke<boolean>("reopen_control_panel", { title: t("Controller.title") });
}

/** What the acceptance probe reports: whether the panel is shown and its buttons as drawn. */
export function controlPanelReport() {
  return { shown, window: report };
}

Object.assign(COMMANDS, {
  "menu.ctrlPanel": { run: toggle, checked: () => shown },
} satisfies Record<string, CommandEntry>);
