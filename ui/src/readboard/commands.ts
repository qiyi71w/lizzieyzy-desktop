// 棋盘同步工具 (ReadBoard) in the workbench: 同步 > 棋盘同步工具(Alt+O) and its settings, the helper
// update confirmation and the helper's one-shot notices. The core owns the helper, its settings
// and the update; this module only asks and shows.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { ReadBoardNotice } from "../contract/generated";
import { chooseSave, message } from "../dialogs";
import { product, t } from "../i18n";

/** LizzieFrame.openBoardSync (LizzieFrame.java:2741-2751): Windows only; without the helper the
 *  core answers with FunctionSearch.unavailable.readBoard. A running helper is stopped and
 *  started again. */
function openBoardSync(): void {
  if (navigator.userAgent.includes("Windows")) void run({ type: "ReadBoard", op: { type: "Open" } });
}

Object.assign(COMMANDS, {
  // Menu.readBoard (Menu.java:4246-4259), shown on Windows only (javaMenus "when": "windows").
  "sync.board": { run: openBoardSync },
  // 同步 > 棋盘同步工具选项 (Menu.java:4353-4392): checks read when the menu opens.
  "menu.alwaysKeepBoardStatSync": {
    run: () => void run({ type: "ReadBoard", op: { type: "SetAlwaysSyncBoardStat", enabled: !state.view?.readboard.always_sync_board_stat } }),
    checked: () => state.view?.readboard.always_sync_board_stat ?? true,
  },
  "menu.readBoardGetFocus": {
    run: () => void run({ type: "ReadBoard", op: { type: "SetGetFocus", enabled: !state.view?.readboard.get_focus } }),
    checked: () => state.view?.readboard.get_focus ?? true,
  },
  // 设置 > 同步时不播放落子声 (Menu.java:5547-5556).
  "menu.notPlaySoundInSync": {
    run: () => void run({ type: "ReadBoard", op: { type: "SetNoSoundInSync", enabled: !state.view?.readboard.no_sound_in_sync } }),
    checked: () => state.view?.readboard.no_sound_in_sync ?? true,
  },
} satisfies Record<string, CommandEntry>);

/** Input.java VK_O: Alt+O opens the tool (Ctrl+O, checked first, is not this). Off Windows Java
 *  only writes to stderr, so the key does nothing there. */
export function handleReadBoardKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyO" || !e.altKey || e.ctrlKey || e.metaKey) return false;
  openBoardSync();
  return true;
}

/** Java's String.format with %s, in order. */
function fill(text: string, args: string[]): string {
  let k = 0;
  return text.replace(/%s/g, () => args[k++] ?? "");
}

/** The Message dialog (Utils.showMsg / SMessage) for a notice. Literal t() calls keep the keys in
 *  the Java string extraction. */
function noticeText(n: ReadBoardNotice): string {
  // showReadBoardGmaUnsupportedOnce: a literal in Java, not a resource.
  if (n.type === "GmaUnsupported") return "该模式仅支持 KataGo";
  const texts: Record<string, string> = {
    // restartReadBoardAfterHostedUpdate (LizzieFrame.java:2921-2940).
    "ReadBoard.updateInstallSucceeded": t("ReadBoard.updateInstallSucceeded"),
    "ReadBoard.updateInstallRestartFailed": t("ReadBoard.updateInstallRestartFailed"),
    "ReadBoard.updateInstallFailed": t("ReadBoard.updateInstallFailed"),
    "ReadBoard.updateInstallNoLongerActive": t("ReadBoard.updateInstallNoLongerActive"),
    // The helper failed to start, or is older than 220430 (ReadBoard.java:616-617, 866-867).
    "ReadBoard.loadFailed": t("ReadBoard.loadFailed"),
    "ReadBoard.versionCheckFaied": t("ReadBoard.versionCheckFaied"),
    // No helper next to the program (FunctionSearch's reason, shown for the menu and Alt+O).
    "FunctionSearch.unavailable.readBoard": t("FunctionSearch.unavailable.readBoard"),
    // showForegroundEngineLeaseConflict (ReadBoard.java:1242-1250).
    "AnalysisSettings.reuseStatus.existing_lease": t("AnalysisSettings.reuseStatus.existing_lease"),
  };
  return fill(texts[n.key] ?? t(n.key), n.args);
}

/** The offer the confirmation is showing or was answered for, so a republished view does not ask
 *  again; a later offer (after this one cleared) asks anew. */
let asked: string | null = null;

registerViewHandler((view) => {
  for (const n of view.readboard.notices) void message(t("Message.title"), noticeText(n));
  const offer = view.readboard.update_offer;
  if (offer === null) {
    asked = null;
    return;
  }
  if (offer === asked) return;
  asked = offer;
  // confirmHostedReadBoardUpdate (LizzieFrame.java:2864-2897): JOptionPane YES_NO, warning; any
  // answer but YES (closing included) declines.
  void chooseSave(t("ReadBoard.updateInstallConfirmTitle"), fill(t("ReadBoard.updateInstallConfirmMessage"), [offer]), product("yes"), product("no")).then((accept) =>
    run({ type: "ReadBoard", op: { type: "AnswerUpdate", accept } }),
  );
});
