// 闪电分析 (LizzieFrame.flashAnalyzeGame / flashAnalyzePart, FlashAnalyzeMenu, AnalysisPartGame,
// WaitForAnalysis): the four entries, the move-range dialog, the progress window and the notices
// a run ends with; the progress window also follows a running 批量精析. The core owns the runs;
// this module holds only the window and its 隐藏界面.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { dispatch, isApiError } from "../contract/client";
import type { AnalysisEngineSettings, Command, FlashOutcome, FlashTarget, WorkbenchView } from "../contract/generated";
import { confirm, formPanel, message, setOwnerDisabled } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { button, footer, groupNote, primaryButton, row } from "../engine/controls";
import { toggleConsole } from "../engine/console";
import { product, t } from "../i18n";
import type { MenuNode } from "../chrome/javaMenus";
import { openAnalysisEngineSettings } from "./analysisSettings";
import "./quick.css";

const COMMAND_MISSING = "LizzieFrame.analysisCommandMissing";

/** flashAnalyzeGame's start: a refusal is explained like Java (Utils.showMsg); a missing command
 *  offers 精析设置 (in its BATCH context for 批量精析), whose 确定 runs `retry`
 *  (resumeFlashAnalysisAfterSettings). Resolves true once the analysis started. */
export async function requestFlash(command: Command, retry: (kept: AnalysisEngineSettings) => Promise<void>, batch = false): Promise<boolean> {
  let result;
  try {
    result = await dispatch(command);
  } catch (e) {
    await message(t("Message.title"), isApiError(e) ? e.message : String(e), "error");
    return false;
  }
  if (result.type !== "Refused") return true;
  if (result.message_key === COMMAND_MISSING) {
    if (await confirm(t("LizzieFrame.analysisCommandMissingTitle"), t("LizzieFrame.analysisCommandMissing"))) {
      await openAnalysisEngineSettings({ resume: retry, batch });
    }
    return false;
  }
  // AnalysisSettings.reuseStatus.* (showFlashAnalysisReuseUnavailable) and the conflicts.
  await message(t("Message.title"), t(result.message_key));
  return false;
}

export async function startFlash(target: FlashTarget): Promise<void> {
  await requestFlash({ type: "StartFlashAnalysis", target }, () => startFlash(target));
}

/** Utils.parseTextToInt with the dialog's own rule that an empty field means "from the first /
 *  to the last move" (-1); other text keeps the previous value as in Java. */
function rangeValue(text: string, previous: number): number {
  const trimmed = text.trim();
  if (trimmed === "") return -1;
  return parseJavaInt(trimmed) ?? previous;
}

/** 局部精析 (AnalysisPartGame): the start and end move rows and the tip, then 开始分析 in the
 *  footer with 取消. */
async function openPartGame(): Promise<void> {
  const range = state.view?.settings.flash_range ?? { startMove: -1, endMove: -1 };
  const panel = document.createElement("div");
  panel.className = "flash-part";
  const field = (value: number, key: string, name: string) => {
    const input = document.createElement("input");
    input.type = "text";
    input.inputMode = "numeric";
    input.value = value > 0 ? String(value) : "";
    input.dataset.key = `flashPart.${key}`;
    row(panel, name, "", input);
    return input;
  };
  const start = field(range.startMove, "start", t("AnalysisPartGame.startMove"));
  const end = field(range.endMove, "end", t("AnalysisPartGame.endMove"));
  groupNote(panel, t("AnalysisPartGame.lblNotice"));
  const begin = primaryButton(t("AnalysisPartGame.btnStart"), () => {});
  begin.dataset.key = "flashPart.run";
  const dialog = formPanel(t("AnalysisPartGame.title"), panel);
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "flashPart.cancel";
  footer(panel, { primary: begin, cancel });
  begin.onclick = async () => {
    const target: FlashTarget = {
      kind: "MoveRange",
      start_move: rangeValue(start.value, range.startMove),
      end_move: rangeValue(end.value, range.endMove),
    };
    await dialog.close();
    await startFlash(target);
  };
  for (const input of [start, end]) {
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter") {
        e.preventDefault();
        begin.click();
      }
    });
  }
  await dialog.closed;
}

Object.assign(COMMANDS, {
  "whole-game-lightning-overview": { run: () => void startFlash({ kind: "WholeGame" }) },
  "partial-lightning-analysis": { run: () => void openPartGame() },
  "all-branches-lightning-analysis": { run: () => void startFlash({ kind: "AllBranches" }) },
} satisfies Record<string, CommandEntry>);

/** FlashAnalyzeMenu: the toolbar button's popup. */
export const FLASH_MENU: MenuNode[] = [
  { t: "item", key: "Menu.flashAnalyzeAllGame", id: "whole-game-lightning-overview" },
  { t: "item", key: "Menu.flashAnalyzePartGame", id: "partial-lightning-analysis" },
  { t: "item", key: "Menu.flashAnalyzeAllBranches", id: "all-branches-lightning-analysis" },
  { t: "item", key: "Menu.flashAnalyzeSettings", id: "lightning-analysis-settings" },
];

// ------------------------------------------------------------------ WaitForAnalysis

/** What the window shows: a flash analysis run, or the record a 批量精析 analyses (Java opens
 *  one window per record). `key` names either; 取消分析 sends `cancel`. */
interface Progress {
  key: string;
  loading: boolean;
  done: number;
  total: number;
  cancel: Command;
}

let win: HTMLElement | null = null;
let title: HTMLElement;
let progress: HTMLElement;
/** The progress the window shows, and the one its 隐藏界面 hid. */
let shown: Progress | null = null;
let hidden: string | null = null;
let closeTimer: number | undefined;

function hideWindow(): void {
  window.clearTimeout(closeTimer);
  closeTimer = undefined;
  shown = null;
  if (win) win.hidden = true;
  setOwnerDisabled(false);
}

/** WaitForAnalysis: a non-modal tool window over the disabled workbench with its progress, and a
 *  footer of 取消分析 (ends the run), 隐藏界面 (closes the window) and, at its start, GTP控制台. It
 *  has no close box: Java's title bar close does nothing here (DO_NOTHING_ON_CLOSE). */
function buildWindow(): HTMLElement {
  const box = document.createElement("div");
  box.className = "modal-box engine-dialog flash-wait";
  box.setAttribute("role", "dialog");
  box.dataset.freeWindow = "";
  title = document.createElement("div");
  title.className = "modal-title";
  const body = document.createElement("div");
  progress = document.createElement("p");
  progress.className = "flash-progress";
  progress.setAttribute("role", "status");
  body.append(progress);
  const keyed = (b: HTMLButtonElement, key: string) => {
    b.dataset.key = `flashWait.${key}`;
    return b;
  };
  const cancel = keyed(button(t("WaitForAnalysis.btnCancel"), () => {
    const command = shown?.cancel;
    hideWindow();
    if (command) void run(command);
  }), "cancel");
  const hide = keyed(button(t("WaitForAnalysis.btnHide"), () => {
    hidden = shown?.key ?? null;
    hideWindow();
  }), "hide");
  const gtp = keyed(button(t("WaitForAnalysis.btnGtpConsole"), () => void toggleConsole()), "console");
  footer(body, { start: [gtp], others: [cancel], cancel: hide });
  box.append(title, body);
  document.body.append(box);
  return box;
}

function showProgress(p: Progress): void {
  win ??= buildWindow();
  window.clearTimeout(closeTimer);
  closeTimer = undefined;
  shown = p;
  title.textContent = t("WaitForAnalysis.title");
  progress.textContent = p.loading ? t("WaitForAnalysis.lblAnalsisProgress") : `${t("WaitForAnalysis.progress")}${p.done}/${p.total}`;
  if (win.hidden) win.hidden = false;
  setOwnerDisabled(true);
}

const runKey = (run: number) => `run:${run}`;

/** setProgress(n, n): 分析完毕! for a second, then closed. A hidden window stays hidden. */
function completed(run: number): void {
  const key = runKey(run);
  if (shown?.key !== key || !win) return;
  title.textContent = t("AnalysisEngine.analyzeComplete");
  progress.textContent = `${t("AnalysisEngine.analyzeComplete")}!`;
  window.clearTimeout(closeTimer);
  closeTimer = window.setTimeout(() => {
    if (shown?.key === key) hideWindow();
  }, 1000);
}

/** Failure texts by key; literal t() calls keep the keys in the Java string extraction
 *  (reuseStatus.* are kept by 精析设置). */
function failureText(key: string): string {
  const texts: Record<string, string> = {
    "AnalysisEngine.requestDispatchFailed": t("AnalysisEngine.requestDispatchFailed"),
    "AnalysisEngine.foregroundRestoreFailed": t("AnalysisEngine.foregroundRestoreFailed"),
  };
  return texts[key] ?? t(key);
}

function outcome(o: FlashOutcome): void {
  switch (o.type) {
    case "Completed":
      completed(o.run);
      return;
    case "Failed":
      if (shown?.key === runKey(o.run)) hideWindow();
      // AnalysisEngine.requestDispatchFailed / foregroundRestoreFailed / reuseStatus.*.
      void message(t("Message.title"), failureText(o.message_key), "warning");
      return;
    case "EngineFailed":
      if (shown?.key === runKey(o.run)) hideWindow();
      // showErrMsg: the failure itself is shown as an engine failure; then AnalysisSettings(true, true).
      void openAnalysisEngineSettings({ redo: true });
      return;
  }
}

/** A flash analysis, else a running 批量精析 (Java's batch closes each record's window at its
 *  end and opens the next one's; 隐藏界面 lasts for that record). */
function render(view: WorkbenchView): void {
  if (view.flash_outcome) outcome(view.flash_outcome);
  const f = view.flash;
  const b = view.batch.running;
  const p: Progress | null = f
    ? { key: runKey(f.run), loading: f.loading, done: f.done, total: f.total, cancel: { type: "CancelFlashAnalysis" } }
    : b?.mode === "Flash"
      ? { key: `batch:${b.item}`, loading: b.loading, done: b.done, total: b.total, cancel: { type: "Batch", op: { type: "CancelFlash" } } }
      : null;
  // A later run of the same record (取消分析, then 进度表's 开始) shows again.
  if (!p) hidden = null;
  if (p && p.key !== hidden) showProgress(p);
  else if (shown !== null && closeTimer === undefined) hideWindow();
}

registerViewHandler(render);

/** The progress window disables the workbench (shortcuts included). */
export function flashWindowOpen(): boolean {
  return shown !== null;
}
