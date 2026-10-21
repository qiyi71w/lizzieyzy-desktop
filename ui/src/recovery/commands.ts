// 文件 > 更多保存, 存档与读档 and the clipboard copies in the command catalog (Menu.java:312-447,
// the toolbar's 存档 button at 6155-6160), plus the automatic save of 存档与读档: every 60 seconds
// while 自动存档 is on (LizzieFrame.autosaveMaybe) and when the window closes (Lizzie.shutdown).
import { copyRecord, exportRecord, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { dispatch, onExitAutoSaveRequest } from "../contract/client";
import { captureMainBoard, copyMainBoardPicture, copySubBoardPicture, saveMainBoardPicture, saveSubBoardPicture, saveWinratePicture } from "./capture";
import { openRecovery } from "./panel";

Object.assign(COMMANDS, {
  "menu.saveRaw": { run: () => void exportRecord("Raw") },
  "menu.saveCommentRaw": { run: () => void exportRecord("RawWithComments") },
  "menu.saveBranchRaw": { run: () => void exportRecord("Branch") },
  "menu.saveMainBoardScreen": { run: () => void saveMainBoardPicture() },
  "menu.saveSubBoardScreen": { run: () => void saveSubBoardPicture() },
  "menu.saveWinrate": { run: () => void saveWinratePicture() },
  "menu.saveAndLoad": { run: openRecovery },
  "menu.copyBoardScreen": { run: () => void copyMainBoardPicture() },
  "menu.copySubBoardScreen": { run: () => void copySubBoardPicture() },
  "menu.copySgf": { run: copyRecord },
} satisfies Record<string, CommandEntry>);

const AUTO_SAVE_INTERVAL = 60_000;

/** Starts the periodic automatic save and answers the exit request. */
export async function startAutoSave(): Promise<void> {
  setInterval(async () => {
    if (!state.view?.recovery.settings.autoSaveOnExit) return;
    const thumbnail = await captureMainBoard();
    if (thumbnail) await run({ type: "Recovery", op: { type: "AutoSave", thumbnail } });
  }, AUTO_SAVE_INTERVAL);
  // Sent directly rather than after queued commands: the window is closing and the backend only
  // waits briefly for the answer.
  await onExitAutoSaveRequest(async () => {
    const thumbnail = await captureMainBoard();
    try {
      await dispatch({ type: "Recovery", op: { type: "ExitAutoSave", thumbnail } });
    } catch (e) {
      console.error("exit automatic save failed", e);
    }
  });
}
