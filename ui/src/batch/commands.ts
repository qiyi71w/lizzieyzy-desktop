// 批量分析 / 批量精析 / 批量分析进度表 (Menu.java:3414-3456, AutoAnalyzeMenu, Input VK_O with
// Ctrl, AnalysisTable): the entries, the settings dialog the queue asks for, the batch's end
// notices and the relay to the 进度表 window. The core owns the queue and the run
// (WorkbenchView.batch); this module only follows it and turns the window's buttons into
// commands.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { BatchCommand, Command, WorkbenchView } from "../contract/generated";
import { message } from "../dialogs";
import { language, t } from "../i18n";
import { openAutoAnalysis, openFlashBatchSetup } from "../quick/autoAnalysis";
import { BATCH_DATA_EVENT, BATCH_INPUT_EVENT, BATCH_LABEL, type BatchButton, type BatchInput, type BatchPayload, type BatchReport } from "./model";

/** A command from the 进度表 window: its refusal is shown in the main window, which comes to
 *  the front for it like the Java message over LizzieFrame. */
async function send(command: Command): Promise<void> {
  const result = await run(command);
  if (result?.type === "Refused") void getCurrentWindow().setFocus();
}

/** openAnalysisTable: opened or brought to the front. AnalysisTable's JFrame is not always on top. */
function openTable(): void {
  void invoke("open_batch_table", { title: t("AnalysisTable.title"), onTop: false });
}

Object.assign(COMMANDS, {
  // openFileWithAna(isAnalysisMode): the chooser; a chosen queue asks for its settings.
  "batch-analysis": { run: () => void setup({ type: "Choose", mode: "Ordinary" }) },
  "batch-deep-analysis": { run: () => void setup({ type: "Choose", mode: "Flash" }) },
  "batch-progress": { run: openTable },
} satisfies Record<string, CommandEntry>);

/** Ctrl+O (Input.java:548-550, before Alt / Shift): 批量分析. Plain O stays 打开棋谱. */
export function handleBatchKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyO" || !(e.ctrlKey || e.metaKey)) return false;
  COMMANDS["batch-analysis"].run();
  return true;
}

// ------------------------------------------------------------------ settings dialog

/** The dialog shown for `batch.setup`; after it closed, the setup the core still reports is the
 *  one just answered until the core clears it or another Choose / Setup is accepted. */
let dialogShown = false;
let answered = false;

function syncSetup(v: WorkbenchView): void {
  const mode = v.batch.setup;
  if (mode === null) {
    answered = false;
    return;
  }
  if (dialogShown || answered) return;
  dialogShown = true;
  void getCurrentWindow().setFocus();
  // StartAnaDialog's batch variants: the dialog sends StartOrdinary / StartFlash / CancelSetup.
  void (mode === "Ordinary" ? openAutoAnalysis(true) : openFlashBatchSetup()).finally(() => {
    dialogShown = false;
    answered = true;
  });
}

/** Choose / Setup: an accepted one asks for a new setup even if the previous answer's view has
 *  not arrived yet. */
async function setup(op: BatchCommand): Promise<void> {
  const result = await run({ type: "Batch", op });
  if (result?.type === "Refused") void getCurrentWindow().setFocus();
  if (result?.type !== "Done") return;
  answered = false;
  if (state.view) syncSetup(state.view);
}

// ------------------------------------------------------------------ notices

/** Literal t() calls keep the core's notice and failure keys in the Java string extraction. */
function noticeText(key: string): string {
  const text: Record<string, string> = {
    "Leelaz.batchAutoAnalyzeComplete": t("Leelaz.batchAutoAnalyzeComplete"),
    "Leelaz.batchAutoAnalyzeStopped": t("Leelaz.batchAutoAnalyzeStopped"),
    "LizzieFrame.prompt.failedToOpenFile": t("LizzieFrame.prompt.failedToOpenFile"),
    "LizzieFrame.saveFileFailed": t("LizzieFrame.saveFileFailed"),
    "AnalysisEngine.requestDispatchFailed": t("AnalysisEngine.requestDispatchFailed"),
    "AnalysisEngine.foregroundRestoreFailed": t("AnalysisEngine.foregroundRestoreFailed"),
    "AutoAnalyze.engineUnavailable": t("AutoAnalyze.engineUnavailable"),
  };
  return text[key] ?? t(key);
}

let lastNotice = -1;

/** Utils.showMsg with the batch's end, then one line per record that could not be read or saved. */
function showNotice(v: WorkbenchView): void {
  const n = v.batch.notice;
  if (!n || n.id === lastNotice) return;
  lastNotice = n.id;
  const failures = n.failures.map((f) => `${noticeText(f.message_key)}: ${f.path}`);
  // A batch that ended on an unreadable record names it in its own line only.
  const lines = n.failures.some((f) => f.message_key === n.message_key) ? failures : [noticeText(n.message_key), ...failures];
  void message(t("Message.title"), lines.join("\n"), n.failures.length > 0 ? "warning" : "info");
}

// ------------------------------------------------------------------ 进度表 window

let lastSent = "";
/** `open_table` of the previous view; the first snapshot never opens the window. */
let openCount: number | null = null;
let report: BatchReport | null = null;

function relay(v: WorkbenchView): void {
  if (openCount !== null && v.batch.open_table > openCount) openTable();
  openCount = v.batch.open_table;
  const payload: BatchPayload = {
    language: language(),
    items: v.batch.items,
    running: v.batch.running,
    autoAnalysis: v.auto_analysis.active || v.auto_analysis.starting,
  };
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(BATCH_LABEL, BATCH_DATA_EVENT, payload);
}

/** The top buttons (AnalysisTable.java:56-157). */
function click(button: BatchButton): void {
  const v = state.view;
  if (!v) return;
  const flash = v.batch.running?.mode === "Flash";
  const batch = (op: BatchCommand) => send({ type: "Batch", op });
  switch (button) {
    case "stopGo":
      // togglePonder; disabled while a 批量精析 runs (resetAnalysisMode).
      if (!flash) void send({ type: "ToggleAnalysis" });
      return;
    case "stopStart":
      if (flash) return;
      if (v.auto_analysis.active || v.auto_analysis.starting) void send({ type: "StopAutoAnalysis" });
      else void setup({ type: "Setup", mode: "Ordinary" });
      return;
    case "stopStartAnalysisMode":
      if (flash) void batch({ type: "CancelFlash" });
      else void setup({ type: "Setup", mode: "Flash" });
      return;
    case "addFile":
      void batch({ type: "AddFiles" });
      return;
    case "clearAllFiles":
      void batch({ type: "Clear" });
      return;
  }
}

function input(i: BatchInput): void {
  switch (i.type) {
    case "ready":
      lastSent = "";
      if (state.view) relay(state.view);
      return;
    case "button":
      click(i.button);
      return;
    case "row":
      void send({ type: "Batch", op: i.action === "Remove" ? { type: "Remove", item: i.item } : { type: "Move", item: i.item, to: i.action } });
      return;
    case "report":
      report = i.report;
      return;
  }
}

registerViewHandler((v) => {
  syncSetup(v);
  showNotice(v);
  relay(v);
});

/** Listens to the 进度表 window. */
export async function startBatchTable(): Promise<void> {
  await listen<BatchInput>(BATCH_INPUT_EVENT, (e) => input(e.payload));
}

/** What the acceptance probe reports: the 进度表's rows and controls as the window drew them,
 *  and whether a batch settings dialog is shown. */
export function batchReport() {
  return { setupDialog: dialogShown, table: report };
}
