// 死活题 and 抓取死活题 entries of the command catalog (Menu.java 分析 menu, the 死活 toolbar button
// and its 自定义工具栏 check item, FunctionCatalog game.tsumego / menu.captureTsumeGo), their keys
// (Input.java VK_E / VK_T with Shift) and the capture overlay's results.
import { listen } from "@tauri-apps/api/event";
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { CapturedProblem } from "../contract/generated";
import { edit } from "../editing/state";
import { openCaptureFrame, openTsumego, startCapture } from "./dialogs";

Object.assign(COMMANDS, {
  "game.tsumego": { run: openTsumego },
  "menu.captureTsumeGo": { run: openCaptureFrame },
  // 自定义工具栏 > 死活 (show-tsume-go-menu).
  "menu.showTsumeGoMenu": {
    run: () => void run({ type: "SetTsumegoButtonShown", shown: !(state.view?.settings.tsumego.showButton ?? true) }),
    checked: () => state.view?.settings.tsumego.showButton ?? true,
  },
} satisfies Record<string, CommandEntry>);

/** Shift+E opens 死活题, Shift+T starts the capture; Ctrl and Alt with E / T belong to other
 *  features first (Input.java:330-337, 642-651). */
export function handleTsumegoKey(e: KeyboardEvent): boolean {
  if (!e.shiftKey || e.ctrlKey || e.metaKey || e.altKey) return false;
  if (e.code === "KeyE") openTsumego();
  else if (e.code === "KeyT") void startCapture();
  else return false;
  return true;
}

// CaptureTsumeGo.processStoneData: the stones are placed, then 死活题 opens to frame them. A
// cancelled capture shows the instructions again.
void listen<CapturedProblem | null>("tsumego-capture-done", async (e) => {
  if (!e.payload) return;
  const result = await edit({ op: "LoadCapturedProblem", problem: e.payload });
  if (result?.type === "Done") openTsumego();
});
void listen("tsumego-capture-cancelled", () => openCaptureFrame());
