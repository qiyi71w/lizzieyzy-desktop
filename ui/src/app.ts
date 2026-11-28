// UI controller: holds the latest projection from Rust and sends commands. Commands are sent one
// at a time in the order the user issued them (like Swing's event queue), so a quick Down/Up pair
// can never be applied in the opposite order.
import { dispatch, nativeDialog } from "./contract/client";
import type { CoachLevelView, Command, CommandResult, CommentLabels, ExportKind, GameInfo, Navigation, StoneSound, TreeNodeView, WorkbenchView } from "./contract/generated";
import { ask, confirm, message, timedMessage } from "./dialogs";
import { product, t } from "./i18n";
import { tuningText } from "./engine/tuningText";
import { contributionText } from "./contribution/text";
import { errorText } from "./errorText";
import { players } from "./workbench/wording";
import "./ogskifu/text";

export const APP_TITLE = "LizzieYzy Tauri";

export const state = {
  view: null as WorkbenchView | null,
  nodes: new Map<number, TreeNodeView>(),
  root: 0,
  /** Nodes on the current line: root to current, then first children to the line end. */
  line: new Set<number>(),
  /** Node count of the main line (first children from the root). */
  mainLineNodes: 1,
  /** Last stone sound requested and how many were requested (reported to acceptance automation). */
  lastSound: null as StoneSound | null,
  soundRequests: 0,
  /** Every HumanSL opponent, weakest first: the catalogue of the last full view. */
  humanSlLevels: [] as readonly CoachLevelView[],
};

const listeners: (() => void)[] = [];
export function subscribe(listener: () => void): void {
  listeners.push(listener);
}

const viewHandlers = new Set<(view: WorkbenchView) => void>();
/** Feature-owned one-shot notices and presentation state stay outside the controller. Returns what
 *  ends the subscription; a handler that ends its own (or another's) while a view is handed out is
 *  not called again, and the rest still are. */
export function registerViewHandler(handler: (view: WorkbenchView) => void): () => void {
  viewHandlers.add(handler);
  return () => void viewHandlers.delete(handler);
}

/** Applies a published view; stale ones (lower `seq`) are ignored. */
export function apply(view: WorkbenchView): void {
  if (state.view && view.seq <= state.view.seq) return;
  if (view.tree) {
    state.nodes = new Map(view.tree.nodes.map((n) => [n.id, n]));
    state.root = view.tree.nodes.find((n) => n.parent === null)?.id ?? 0;
    let count = 1;
    for (let n = state.root; state.nodes.get(n)?.children.length; n = state.nodes.get(n)!.children[0]) count++;
    state.mainLineNodes = count;
  }
  if (view.human_sl_levels) state.humanSlLevels = view.human_sl_levels;
  const line = new Set(view.position.path);
  for (let n: number | undefined = view.position.node; n !== undefined && n !== view.position.line_end; ) {
    n = state.nodes.get(n)?.children[0];
    if (n !== undefined) line.add(n);
  }
  line.add(view.position.line_end);
  state.line = line;
  state.view = view;
  const outcome = view.open_outcome;
  if (outcome?.type === "Failed") {
    // Files: failedToOpenFile (LizzieFrame.java:17160-17165) under the message title, where Java's
    // option pane says "Error" in every language; pasted records: the Message dialog with
    // KifuLoad.failed + reason (4764-4777, Message.java:25).
    if (outcome.message_key === "KifuLoad.failed") message(t("Message.title"), `${t("KifuLoad.failed")}${outcome.detail}`, "error");
    // openFile during a game (LizzieFrame.java:5134-5138): Utils.showMsg(openFileFailed.inGame).
    else if (outcome.message_key === "LizzieFrame.openFileFailed.inGame") message(t("Message.title"), t("LizzieFrame.openFileFailed.inGame"));
    // An OGS link from 打开在线链接 whose game was not downloaded (OgsKifuImport.tryOpen →
    // failKifuLoad); OGS refusing requests for now has its own message.
    else if (outcome.message_key === "OgsKifuImport.failed") message(t("Message.title"), t("OgsKifuImport.failed"), "error");
    else if (outcome.message_key === "ogsRecordRateLimited") message(t("Message.title"), product("ogsRecordRateLimited"), "error");
    else message(t("Message.title"), t("LizzieFrame.prompt.failedToOpenFile"), "error");
  }
  // SgfSaveCoordinator.showFailure: Utils.showMsg(saveFileFailed); a success shows nothing.
  if (view.save_outcome?.type === "Failed") message(t("Message.title"), t("LizzieFrame.saveFileFailed"), "error");
  view.sounds.forEach(playSound);
  for (const handler of viewHandlers) handler(view);
  listeners.forEach((l) => l());
}

/** Java sound files per `Utils.playVoiceFile` (assets/sound). */
const SOUND_FILES: Record<StoneSound, string> = {
  Stone: "/assets/sound/Stone.wav",
  DeadStone: "/assets/sound/deadStone.wav",
  DeadStoneMore: "/assets/sound/deadStoneMore.wav",
};
let soundsPlaying = 0;

/** Like Java's `msemaphoretryroom`, at most two stone sounds overlap; a step while two are
 *  playing stays silent. Playback failures are ignored (sound is optional). */
function playSound(sound: StoneSound) {
  state.lastSound = sound;
  state.soundRequests++;
  if (soundsPlaying >= 2) return;
  soundsPlaying++;
  const audio = new Audio(SOUND_FILES[sound]);
  const done = () => soundsPlaying--;
  audio.addEventListener("ended", done, { once: true });
  audio.play().catch(done);
}

let queue: Promise<unknown> = Promise.resolve();

/** Refusals Java says in a 3 s message rather than a dialog: a desktop move during Web 试下
 *  (LizzieFrame.showTrialBlockedHint) and a delete during an AI 陪练 game
 *  (HumanSlGameController.showDeleteBlockedHint). */
const TIMED_REFUSALS: Record<string, true> = {
  "LizzieFrame.webTrialDesktopBlocked": true,
  "HumanSlGame.deleteBlocked": true,
};

/** Sends a command after the previously issued ones. Errors are shown, not thrown. */
export function run(command: Command): Promise<CommandResult | null> {
  const next = queue.then(async () => {
    try {
      const result = await dispatch(command);
      if (result.type === "Refused") {
        const messages: Record<string, string> = {
          "SetKataRules.notKataGoHint": t("SetKataRules.notKataGoHint"),
          "LizzieFrame.setParamNoEngineHint": t("LizzieFrame.setParamNoEngineHint"),
          "LizzieFrame.setParamsWarning": t("LizzieFrame.setParamsWarning"),
          "product.consoleBoardCommand": product("consoleBoardCommand"),
          "product.importStale": product("importStale"),
          "product.importNeedsReplace": product("importNeedsReplace"),
          "product.quickLinkFailed": product("quickLinkFailed"),
          "LizzieFrame.setupEditRequiresRootOnly": t("LizzieFrame.setupEditRequiresRootOnly"),
          "LizzieFrame.setupModeRequiresRootOnly": t("LizzieFrame.setupModeRequiresRootOnly"),
          "LizzieFrame.setupModeEngineNotReady": t("LizzieFrame.setupModeEngineNotReady"),
          "SpinAndMirror.noneSquareError": t("SpinAndMirror.noneSquareError"),
          // 人机对弈 refusals (the game's gate and the 新对局 dialogs).
          "LizzieFrame.waitEngineLoadingHint": t("LizzieFrame.waitEngineLoadingHint"),
          "LizzieFrame.openFileFailed.inGame": t("LizzieFrame.openFileFailed.inGame"),
          "NewAnaGameDialog.wrongAiMoveSettings": t("NewAnaGameDialog.wrongAiMoveSettings"),
          "NewAnaGameDialog.noEngineHint": t("NewAnaGameDialog.noEngineHint"),
          "AnalysisSettings.reuseStatus.play_mode": t("AnalysisSettings.reuseStatus.play_mode"),
          "DesktopTimeControl.websocketAdvancedUnsupported": t("DesktopTimeControl.websocketAdvancedUnsupported"),
          // AI 陪练 refusals (a game on, the model missing).
          "LizzieFrame.engineGameStopFirstHint": t("LizzieFrame.engineGameStopFirstHint"),
          "HumanSlGame.missingModelDownloadPrompt": t("HumanSlGame.missingModelDownloadPrompt"),
          // Engine-taking modes while a viewer's Web 试下 holds the engine.
          "AnalysisSettings.reuseStatus.application_exclusive_mode": t("AnalysisSettings.reuseStatus.application_exclusive_mode"),
          // 激进度 / 广度 refusals: the engine is busy with something else.
          "Fox.conflict": t("Fox.conflict"),
          "AnalysisSettings.reuseStatus.engine_game": t("AnalysisSettings.reuseStatus.engine_game"),
          "AnalysisSettings.reuseStatus.human_sl_game": t("AnalysisSettings.reuseStatus.human_sl_game"),
          "AnalysisSettings.reuseStatus.engine_state_unrestored": t("AnalysisSettings.reuseStatus.engine_state_unrestored"),
          "AnalysisSettings.reuseStatus.readboard_gma": t("AnalysisSettings.reuseStatus.readboard_gma"),
          "AnalysisSettings.reuseStatus.missing_capability": t("AnalysisSettings.reuseStatus.missing_capability"),
          "AnalysisSettings.reuseStatus.existing_lease": t("AnalysisSettings.reuseStatus.existing_lease"),
          // 规则面板 确定 while 整盘分析 starts or runs.
          "WholeGameAnalysis.conflict.analysis": t("WholeGameAnalysis.conflict.analysis"),
          // 断开智子引擎 while a 智子 engine is starting, switching or held by a task.
          "RemoteCompute.zhiziSession.blocked": t("RemoteCompute.zhiziSession.blocked"),
          // 对局参数: 继续 before a revision was confirmed, and a revision the core refused.
          "EngineGameParameters.resumeBlocked": t("EngineGameParameters.resumeBlocked"),
          "EngineGameParameters.notPlaying": t("EngineGameParameters.notPlaying"),
          "EngineGameParameters.invalidWrn": t("EngineGameParameters.invalidWrn"),
          "EngineGameParameters.invalidPda": t("EngineGameParameters.invalidPda"),
          "EngineGameParameters.rejected": t("EngineGameParameters.rejected"),
        };
        // 跑谱贡献 refusals: Java keys and the product's unsupported-package text.
        if (TIMED_REFUSALS[result.message_key]) timedMessage(t(result.message_key), 3);
        else void message(t("Message.title"), messages[result.message_key] ?? contributionText(result.message_key), "warning");
      }
      return result;
    } catch (e) {
      // Tuning refusals carry a Java message key (`EngineThreadPolicy.*`), not display text.
      const text = command.type === "Tuning" ? errorText(e, tuningText) : errorText(e);
      // Only the command kind: arguments may hold credentials (ikatago scan, proxy settings).
      const op = "op" in command ? `.${"type" in command.op ? command.op.type : command.op.op}` : "";
      console.error("command failed", `${command.type}${op}`, e);
      await message(t("Message.title"), text, "error");
      return null;
    }
  });
  queue = next;
  return next;
}

export function navigate(nav: Navigation): void {
  run({ type: "Navigate", nav });
}

/** "从剪贴板粘贴棋谱": confirm before replacing a record that has content (LizzieFrame.pasteSgf). */
export async function pasteRecord(): Promise<void> {
  const r = await run({ type: "PasteRecord", confirmed: false });
  if (r?.type !== "NeedsConfirmation") return;
  if (await confirm(t("LizzieFrame.pasteSgfReplaceTitle"), t("LizzieFrame.pasteSgfReplaceMessage"))) {
    await run({ type: "PasteRecord", confirmed: true });
  }
}

/** GameInfo.getSaveFileName: "<black>_Vs_<white>", or GameInfo.untitled when both are empty. */
export function saveFileName(): string {
  const info = state.view?.document.info;
  return !info || (info.black === "" && info.white === "") ? t("GameInfo.untitled") : `${info.black}_Vs_${info.white}`;
}

/** The chooser's default name (SgfSaveCoordinator.chooseAndSave): the save file name, then
 *  "_yyyyMMddHHmmss". */
function suggestedSaveName(): string {
  const name = saveFileName();
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  const stamp = `${d.getFullYear()}${p(d.getMonth() + 1)}${p(d.getDate())}${p(d.getHours())}${p(d.getMinutes())}${p(d.getSeconds())}`;
  return name === "" ? stamp : `${name}_${stamp}`;
}

/** The prompts a save may need before it starts. */
async function followSave(r: CommandResult | null): Promise<void> {
  if (r?.type === "NeedsReplaceConfirmation") {
    // LizzieFrame.saveOriFile: the question, the notice and "不再提示"; Cancel is the default.
    const body = document.createElement("div");
    const line = (text: string) => {
      const el = document.createElement("div");
      el.textContent = text;
      body.append(el);
    };
    line(`${t("LizzieFrame.ifReplaceFile")}${r.file_name}" ?`);
    line(t("LizzieFrame.replaceFileNotice"));
    const label = document.createElement("label");
    const box = document.createElement("input");
    box.type = "checkbox";
    label.append(box, ` ${t("LizzieFrame.noNoticeAgain")}`);
    body.append(label);
    const ok = await ask(t("LizzieFrame.replaceFileTitle"), body, t("LizzieFrame.confirm"), t("LizzieFrame.cancel"), true);
    // The check box takes effect whether or not the file is replaced.
    if (box.checked) await run({ type: "SetReplaceFileHint", enabled: false });
    if (ok) await followSave(await run({ type: "SaveRecord", confirmed: r.document, suggested_name: suggestedSaveName() }));
  } else if (r?.type === "NeedsOverwriteConfirmation") {
    if (await confirm(t("LizzieFrame.warning"), t("LizzieFrame.prompt.sgfExists"))) await run({ type: "SaveRecordTo", path: r.path });
  }
}

/** "保存(Ctrl+S)" (LizzieFrame.saveOriFile). A record without a file of its own, or read from a
 *  GIB file, is saved through the chooser (Workbench::save_plan). */
export async function saveRecord(): Promise<void> {
  const path = state.view?.document.path;
  if (!path || path.toLowerCase().endsWith(".gib")) nativeDialog();
  await followSave(await run({ type: "SaveRecord", confirmed: null, suggested_name: suggestedSaveName() }));
}

/** "另存为(S)" (LizzieFrame.saveFile(false)). */
export async function saveRecordAs(): Promise<void> {
  await followSave(await run({ type: "ChooseAndSaveRecord", suggested_name: suggestedSaveName() }));
}

/** Words of the win-rate summaries SGFParser.formatComment writes into exported comments. */
function commentLabels(): CommentLabels {
  return {
    black: t("SGFParse.black"),
    white: t("SGFParse.white"),
    winrate: t("SGFParse.winrate"),
    lead: t("SGFParse.leadJustScore"),
    lead_with_komi: t("SGFParse.leadWithKomi"),
    stdev: t("SGFParse.stdev"),
    playouts: t("SGFParse.playouts"),
    komi: t("SGFParse.komi"),
    pda: t("SGFParse.pda"),
    wrn: t("SGFParse.wrn"),
  };
}

/** 更多保存 > 保存纯净棋谱 (Ctrl+Shift+S) / 保存纯净棋谱(带评论) / 保存纯净分支 (Ctrl+Alt+S): the save
 *  chooser like "另存为"; the end arrives as the view's save outcome. */
export async function exportRecord(kind: ExportKind): Promise<void> {
  const r = await run({ type: "ExportRecord", kind, suggested_name: suggestedSaveName(), labels: commentLabels() });
  if (r?.type === "NeedsExportOverwriteConfirmation" && await confirm(t("LizzieFrame.warning"), t("LizzieFrame.prompt.sgfExists"))) {
    await run({ type: "ExportRecordTo", kind: r.kind, path: r.path, labels: commentLabels() });
  }
}

/** "复制棋谱到剪贴板(Ctrl+C)" (LizzieFrame.copySgf). */
export function copyRecord(): void {
  void run({ type: "CopyRecord" });
}

/** A Java entry whose capability has not been migrated yet. */
export function unavailable(): void {
  message(APP_TITLE, product("unavailable"));
}

/** Java LizzieFrame.updateTitle without an engine: DEFAULT_TITLE, then the players with the result
 *  ("[黑] X vs [白] Y (结果:B+R)", titlePlayers) and the file.
 *  While 试下 is on the title is LizzieFrame.tryTitle only (tryPlay sets it; updateTitle returns). */
export function windowTitle(version: string): string {
  const v = state.view;
  if (v?.editing.trying) return t("LizzieFrame.tryTitle");
  let title = `${APP_TITLE} ${version} `;
  if (!v) return title;
  if (v.document.format !== null) title += `- ${titlePlayers(v.document.info)}`;
  // A record loaded from a 存档与读档 slot has no file of its own; Java shows the slot's file name.
  const fileName = v.document.file_name ?? v.document.origin_name;
  if (fileName) title += ` - ${fileName}`;
  return title;
}

/** The players and the result as the title writes them (LizzieFrame.setPlayers / setResult). */
export function titlePlayers(info: GameInfo): string {
  return players(info.black, info.white, info.result, { black: t("Menu.Black"), white: t("Menu.White"), result: t("LizzieFrame.result") });
}
