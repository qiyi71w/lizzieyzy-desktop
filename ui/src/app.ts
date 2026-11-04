// UI controller: holds the latest projection from Rust and sends commands. Commands are sent one
// at a time in the order the user issued them (like Swing's event queue), so a quick Down/Up pair
// can never be applied in the opposite order.
import { dispatch, isApiError } from "./contract/client";
import type { Command, CommandResult, CommentLabels, ExportKind, Navigation, StoneSound, TreeNodeView, WorkbenchView } from "./contract/generated";
import { ask, confirm, message } from "./dialogs";
import { product, t } from "./i18n";
import { tuningText } from "./engine/tuningText";

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
};

const listeners: (() => void)[] = [];
export function subscribe(listener: () => void): void {
  listeners.push(listener);
}

const viewHandlers: ((view: WorkbenchView) => void)[] = [];
/** Feature-owned one-shot notices and presentation state stay outside the controller. */
export function registerViewHandler(handler: (view: WorkbenchView) => void): void {
  viewHandlers.push(handler);
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
    // Files: JOptionPane "Error" with failedToOpenFile (LizzieFrame.java:17160-17165); pasted
    // records: the Message dialog with KifuLoad.failed + reason (4764-4777, Message.java:25).
    if (outcome.message_key === "KifuLoad.failed") message(t("Message.title"), `${t("KifuLoad.failed")}${outcome.detail}`, "error");
    // openFile during a game (LizzieFrame.java:5134-5138): Utils.showMsg(openFileFailed.inGame).
    else if (outcome.message_key === "LizzieFrame.openFileFailed.inGame") message(t("Message.title"), t("LizzieFrame.openFileFailed.inGame"));
    else message("Error", t("LizzieFrame.prompt.failedToOpenFile"), "error");
  }
  // SgfSaveCoordinator.showFailure: Utils.showMsg(saveFileFailed); a success shows nothing.
  if (view.save_outcome?.type === "Failed") message(t("Message.title"), t("LizzieFrame.saveFileFailed"), "error");
  view.sounds.forEach(playSound);
  viewHandlers.forEach((handler) => handler(view));
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

/** Sends a command after the previously issued ones. Errors are shown, not thrown. */
export function run(command: Command): Promise<CommandResult | null> {
  const next = queue.then(async () => {
    try {
      const result = await dispatch(command);
      if (result.type === "Refused") {
        const messages: Record<string, string> = {
          "SetKataRules.notKataGoHint": t("SetKataRules.notKataGoHint"),
          "LizzieFrame.setParamNoEngineHint": t("LizzieFrame.setParamNoEngineHint"),
          "product.consoleBoardCommand": product("consoleBoardCommand"),
          "product.importStale": product("importStale"),
          "product.importNeedsReplace": product("importNeedsReplace"),
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
        };
        const text = messages[result.message_key] ?? t(result.message_key);
        void message(t("Message.title"), text, "warning");
      }
      return result;
    } catch (e) {
      // Tuning refusals carry a Java message key (`EngineThreadPolicy.*`), not display text.
      const text = isApiError(e) ? (command.type === "Tuning" ? tuningText(e.message) : e.message) : String(e);
      // Only the command kind: arguments may hold credentials (ikatago scan, proxy settings).
      const op = "op" in command ? `.${"type" in command.op ? command.op.type : command.op.op}` : "";
      console.error("command failed", `${command.type}${op}`, e);
      await message("Error", text, "error");
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

/** "保存(Ctrl+S)" (LizzieFrame.saveOriFile). */
export async function saveRecord(): Promise<void> {
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

/** Java LizzieFrame.updateTitle without an engine: DEFAULT_TITLE + " " + players + result + file.
 *  While 试下 is on the title is LizzieFrame.tryTitle only (tryPlay sets it; updateTitle returns). */
export function windowTitle(version: string): string {
  const v = state.view;
  if (v?.editing.trying) return t("LizzieFrame.tryTitle");
  let title = `${APP_TITLE} ${version} `;
  if (!v) return title;
  if (v.document.format !== null) {
    const info = v.document.info;
    title += `- [${t("Menu.Black")}]${info.black} vs[${t("Menu.White")}]${info.white}`;
    if (info.result) title += `(${t("LizzieFrame.result")}${info.result})`;
  }
  // A record loaded from a 存档与读档 slot has no file of its own; Java shows the slot's file name.
  const fileName = v.document.file_name ?? v.document.origin_name;
  if (fileName) title += ` - ${fileName}`;
  return title;
}
