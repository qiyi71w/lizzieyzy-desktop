// UI controller: holds the latest projection from Rust and sends commands. Commands are sent one
// at a time in the order the user issued them (like Swing's event queue), so a quick Down/Up pair
// can never be applied in the opposite order.
import { dispatch, isApiError } from "./contract/client";
import type { Command, CommandResult, Navigation, StoneSound, TreeNodeView, WorkbenchView } from "./contract/generated";
import { ask, confirm, message } from "./dialogs";
import { product, t } from "./i18n";

export const APP_TITLE = "LizzieYzy Tauri";

export const state = {
  view: null as WorkbenchView | null,
  nodes: new Map<number, TreeNodeView>(),
  root: 0,
  /** Nodes on the current line: root to current, then first children to the line end. */
  line: new Set<number>(),
  /** Node count of the main line (first children from the root). */
  mainLineNodes: 1,
  /** Last stone sound requested (reported to acceptance automation). */
  lastSound: null as StoneSound | null,
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
        };
        const text = messages[result.message_key] ?? t(result.message_key);
        void message(t("Message.title"), text, "warning");
      }
      return result;
    } catch (e) {
      const text = isApiError(e) ? e.message : String(e);
      // Only the command kind: arguments may hold credentials (ikatago scan, proxy settings).
      const op = "op" in command ? `.${command.op.type}` : "";
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

/** The chooser's default name (SgfSaveCoordinator.chooseAndSave): GameInfo.getSaveFileName, i.e.
 *  "<black>_Vs_<white>" or GameInfo.untitled, then "_yyyyMMddHHmmss". */
function suggestedSaveName(): string {
  const info = state.view?.document.info;
  const name = !info || (info.black === "" && info.white === "") ? t("GameInfo.untitled") : `${info.black}_Vs_${info.white}`;
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

/** A Java entry whose capability has not been migrated yet. */
export function unavailable(): void {
  message(APP_TITLE, product("unavailable"));
}

/** Java LizzieFrame.updateTitle without an engine: DEFAULT_TITLE + " " + players + result + file. */
export function windowTitle(version: string): string {
  const v = state.view;
  let title = `${APP_TITLE} ${version} `;
  if (!v) return title;
  if (v.document.format !== null) {
    const info = v.document.info;
    title += `- [${t("Menu.Black")}]${info.black} vs[${t("Menu.White")}]${info.white}`;
    if (info.result) title += `(${t("LizzieFrame.result")}${info.result})`;
  }
  if (v.document.file_name) title += ` - ${v.document.file_name}`;
  return title;
}
