// 独立棋局控制面板 entries (视图 > 面板 > 独立棋局控制面板, the top toolbar's control button;
// LizzieFrame.openController) and its buttons. The shell owns the window, its bounds and whether
// it reopens at start; this module toggles it, follows whether it is shown for the menu check and
// runs the browsing command of each button the window reports (Controller.java:35-124).
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { navigate, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { language, t } from "../i18n";
import { CONTROL_CLOSED_EVENT, CONTROL_DATA_EVENT, CONTROL_INPUT_EVENT, type ControlButton, type ControlInput, type ControlReport } from "./model";

/** src-tauri control_panel::LABEL. */
const LABEL = "control-panel";

let shown = false;
let report: ControlReport | null = null;

function toggle(): void {
  void invoke<boolean>("toggle_control_panel", { title: t("Controller.title") }).then((s) => (shown = s));
}

/** LizzieFrame.undo / redo return at once in a game against the engine (isPlayingAgainstLeelaz,
 *  isAnaPlayingAgainstLeelaz); the other buttons browse as their menu items do. */
function step(op: "Back" | "Forward", count: number): void {
  if (!state.view?.game) navigate({ op, count });
}

const ACTIONS: Record<ControlButton, () => void> = {
  first: () => COMMANDS["menu.jumpToFirst"].run(),
  last: () => COMMANDS["menu.jumpToLast"].run(),
  back5: () => step("Back", 5),
  forward5: () => step("Forward", 5),
  back1: () => step("Back", 1),
  forward1: () => step("Forward", 1),
  backToMain: () => COMMANDS["menu.backToMainBranch"].run(),
  // drawPainting's layer is a dialog that takes the focus; here it is in the main window.
  painting: () => {
    void getCurrentWindow().setFocus();
    COMMANDS["toolbar.painting"].run();
  },
};

function input(i: ControlInput): void {
  if (i.type === "ready") void emitTo(LABEL, CONTROL_DATA_EVENT, { language: language() });
  else if (i.type === "button") ACTIONS[i.button]();
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
