// StartAnaDialog (the automatic analysis and the two batch variants), SetDiffAnalyze and
// AutoAnalyzeMenu. Drafts only; Rust owns the run.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { AutoAnalysisSettings, WorkbenchView } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { actions, button } from "../engine/controls";
import { t } from "../i18n";
import type { MenuNode } from "../chrome/javaMenus";
import { openAnalysisEngineSettings } from "./analysisSettings";
import { requestFlash } from "./flash";
import "./autoAnalysis.css";

export const AUTO_ANALYSIS_MENU: MenuNode[] = [
  { t: "item", key: "Menu.wholeGameDeepAnalysis", id: "whole-game-deep-analysis" },
  { t: "item", key: "Menu.autoAnalyze", id: "auto-analysis" },
  { t: "sep" },
  { t: "item", key: "Menu.batchAnalyze", id: "batch-analysis" },
  { t: "item", key: "Menu.batchAnalysisMode", id: "batch-deep-analysis" },
  { t: "sep" },
  { t: "item", key: "Menu.stopAutoAnalyze", id: "stop-analysis" },
  { t: "item", key: "Menu.batchAnalyzeTable", id: "batch-progress" },
];

function grid(): { body: HTMLDivElement; rows: HTMLDivElement } {
  const body = document.createElement("div"); body.className = "auto-analysis-dialog";
  const rows = document.createElement("div"); rows.className = "auto-analysis-grid";
  body.append(rows);
  return { body, rows };
}
function row(host: HTMLElement, text: string, control: HTMLElement): void {
  const label = document.createElement("label"); label.textContent = text;
  if (control instanceof HTMLInputElement) { control.id = control.dataset.key!; label.htmlFor = control.id; }
  host.append(label, control);
}
function number(value: number, key: string, decimal = false): HTMLInputElement {
  const el = document.createElement("input"); el.type = "text"; el.inputMode = decimal ? "decimal" : "numeric";
  el.value = value >= 0 ? String(value) : ""; el.dataset.key = key;
  return el;
}
function check(value: boolean, key: string): HTMLInputElement {
  const el = document.createElement("input"); el.type = "checkbox"; el.checked = value; el.dataset.key = key;
  return el;
}
const integer = (el: HTMLInputElement) => parseJavaInt(el.value.replaceAll(" ", "")) ?? -1;

/** Seven equal-height two-column rows; Java's optional condition fields use -1 for empty. */
async function openDiff(draft: AutoAnalysisSettings): Promise<void> {
  const { body, rows } = grid();
  const black = check(draft.diffBlack, "autoDiff.black"), white = check(draft.diffWhite, "autoDiff.white");
  row(rows, t("StartAnaDialog.analyzeBlack"), black); row(rows, t("StartAnaDialog.analyzeWhite"), white);
  const win = check(draft.diffUseWin, "autoDiff.useWin"), score = check(draft.diffUseScore, "autoDiff.useScore");
  const winValue = number(draft.diffWinThreshold, "autoDiff.win", true), scoreValue = number(draft.diffScoreThreshold, "autoDiff.score", true);
  for (const [text, box, value] of [[t("SetDiffAnalyze.lblWinThreshold"), win, winValue], [t("SetDiffAnalyze.lblScoreThreshold"), score, scoreValue]] as const) {
    const group = document.createElement("div"); group.className = "auto-analysis-inline"; group.append(box, value);
    value.disabled = !box.checked; box.onchange = () => { value.disabled = !box.checked; }; row(rows, text, group);
  }
  const time = number(draft.diffTime, "autoDiff.time"), visits = number(draft.diffVisits, "autoDiff.visits"), first = number(draft.diffFirstVisits, "autoDiff.firstVisits");
  row(rows, t("StartAnaDialog.timePerMove"), time); row(rows, t("StartAnaDialog.totalVisitsPerMove"), visits); row(rows, t("StartAnaDialog.firstVisitsPerMove"), first);
  const dialog = formPanel(t("SetDiffAnalyze.title"), body);
  const apply = button(t("SetKataEngines.btnApply"), async () => {
    if (!black.checked && !white.checked) { await message(t("Message.title"), t("SetDiffAnalyze.noBlackWhite")); return; }
    if (!win.checked && !score.checked) { await message(t("Message.title"), t("SetDiffAnalyze.noWinScore")); return; }
    if (integer(time) <= 0 && integer(visits) <= 0 && integer(first) <= 0) { await message(t("Message.title"), t("SetDiffAnalyze.noCondition")); return; }
    const w = Number(winValue.value.trim()), s = Number(scoreValue.value.trim());
    if (!Number.isFinite(w) || !Number.isFinite(s)) return;
    const next: AutoAnalysisSettings = { ...draft, diffBlack: black.checked, diffWhite: white.checked, diffUseWin: win.checked, diffUseScore: score.checked, diffWinThreshold: w, diffScoreThreshold: s, diffTime: integer(time), diffVisits: integer(visits), diffFirstVisits: integer(first) };
    const result = await run({ type: "SaveAutoAnalysisSettings", settings: next });
    if (!result || result.type === "Refused") return;
    Object.assign(draft, next); await dialog.close();
  });
  apply.dataset.key = "autoDiff.apply";
  const cancel = button(t("SetKataEngines.btnCancel"), () => void dialog.close()); cancel.dataset.key = "autoDiff.cancel";
  actions(body, apply, cancel);
  await dialog.closed;
}

/** StartAnaDialog(false). `batch`: the batch queue's variant (Lizzie.frame.isBatchAna): the
 *  start label names the batch, there is no 自动保存棋谱 row, 开始分析 starts the batch and
 *  closing the dialog or 终止分析 gives the queue's setup up (cancelBatchSettings). */
export async function openAutoAnalysis(batch = false): Promise<void> {
  const saved = state.view?.settings.auto_analysis;
  if (!saved) return;
  const draft = { ...saved };
  const { body, rows } = grid();
  const start = number(draft.startMove, "autoAnalysis.start"), end = number(draft.endMove, "autoAnalysis.end");
  const time = number(draft.timeSeconds, "autoAnalysis.time"), visits = number(draft.visits, "autoAnalysis.visits"), first = number(draft.firstVisits, "autoAnalysis.firstVisits");
  row(rows, batch ? t("StartAnaDialog.startMoveBatch") : t("StartAnaDialog.startMove"), start); row(rows, t("StartAnaDialog.endMove"), end);
  row(rows, t("StartAnaDialog.timePerMove"), time); row(rows, t("StartAnaDialog.totalVisitsPerMove"), visits); row(rows, t("StartAnaDialog.firstVisitsPerMove"), first);
  const black = check(draft.black, "autoAnalysis.black"), white = check(draft.white, "autoAnalysis.white"), branches = check(draft.allBranches, "autoAnalysis.allBranches"), diff = check(draft.diffEnable, "autoAnalysis.diff");
  row(rows, t("StartAnaDialog.analyzeBlack"), black); row(rows, t("StartAnaDialog.analyzeWhite"), white); row(rows, t("StartAnaDialog.analyzeAllBranch"), branches);
  const conditions = button(t("StartAnaDialog.btnSetDiff"), () => void openDiff(draft)); conditions.dataset.key = "autoAnalysis.conditions";
  conditions.disabled = !diff.checked; diff.onchange = () => { conditions.disabled = !diff.checked; draft.diffEnable = diff.checked; };
  const group = document.createElement("div"); group.className = "auto-analysis-inline"; group.append(diff, conditions); row(rows, t("StartAnaDialog.lblDiffAnalyze"), group);
  const pause = check(draft.exitByPause, "autoAnalysis.exitByPause"), save = check(draft.autoSave, "autoAnalysis.autoSave");
  row(rows, t("StartAnaDialog.chkExitByPause"), pause);
  if (!batch) row(rows, t("StartAnaDialog.autoSaveKifu"), save);
  const dialog = formPanel(t("StartAnaDialog.title"), body);
  let started = false;
  const begin = button(t("StartAnaDialog.startAnalyze"), async () => {
    const settings: AutoAnalysisSettings = { ...draft, startMove: integer(start), endMove: integer(end), timeSeconds: integer(time), visits: integer(visits), firstVisits: integer(first), timeEnabled: integer(time) > 0, visitsEnabled: integer(visits) > 0, firstVisitsEnabled: integer(first) > 0, black: black.checked, white: white.checked, allBranches: branches.checked, diffEnable: diff.checked, exitByPause: pause.checked, autoSave: save.checked };
    const result = await run(batch ? { type: "Batch", op: { type: "StartOrdinary", settings } } : { type: "StartAutoAnalysis", settings });
    if (!result || result.type === "Refused") return;
    started = true;
    await dialog.close();
  });
  begin.dataset.key = "autoAnalysis.begin";
  const stop = button(t("StartAnaDialog.stopAnalyze"), async () => { await dialog.close(); if (!batch) await run({ type: "StopAutoAnalysis" }); }); stop.dataset.key = "autoAnalysis.stop";
  actions(body, begin, stop);
  await dialog.closed;
  if (batch && !started) await run({ type: "Batch", op: { type: "CancelSetup" } });
}

/** StartAnaDialog(true): 批量精析's four rows (the start / end fields are the ones automatic
 *  analysis keeps), 设置 opens 精析设置 in its BATCH context and refreshes 每手总计算量 from
 *  it; closing the dialog or 终止分析 gives the queue's setup up. */
export async function openFlashBatchSetup(): Promise<void> {
  const view = state.view;
  if (!view) return;
  const range = view.settings.auto_analysis;
  const savedVisits = () => state.view?.settings.analysis_engine.batchVisits ?? view.settings.analysis_engine.batchVisits;
  const { body, rows } = grid();
  const start = number(range.startMove, "flashBatch.start"), end = number(range.endMove, "flashBatch.end");
  const visits = number(savedVisits(), "flashBatch.visits"), branches = check(false, "flashBatch.allBranches");
  row(rows, t("StartAnaDialog.startMoveBatch"), start); row(rows, t("StartAnaDialog.endMove"), end);
  row(rows, t("StartAnaDialog.totalVisitsPerMove"), visits); row(rows, t("StartAnaDialog.chkAllBranches"), branches);
  const dialog = formPanel(t("StartAnaDialog.title"), body);
  let started = false;
  /** apply(): Utils.parseTextToInt keeps the saved 每手总计算量 for text that is no number. */
  const begin = async (): Promise<void> => {
    const op = { type: "StartFlash", start_move: integer(start), end_move: integer(end), visits: parseJavaInt(visits.value.trim()) ?? savedVisits(), all_branches: branches.checked } as const;
    const ok = await requestFlash({ type: "Batch", op }, async (kept) => {
      visits.value = String(kept.batchVisits);
      await begin();
    }, true);
    if (!ok) return;
    started = true;
    await dialog.close();
  };
  const settings = button(t("WaitForAnalysis.btnSettings"), async () => {
    const kept = await openAnalysisEngineSettings({ batch: true });
    visits.value = String(kept?.batchVisits ?? savedVisits());
  });
  settings.dataset.key = "flashBatch.settings";
  const ok = button(t("StartAnaDialog.startAnalyze"), () => void begin()); ok.dataset.key = "flashBatch.begin";
  const stop = button(t("StartAnaDialog.stopAnalyze"), () => void dialog.close()); stop.dataset.key = "flashBatch.stop";
  actions(body, settings, ok, stop);
  await dialog.closed;
  if (!started) await run({ type: "Batch", op: { type: "CancelSetup" } });
}

Object.assign(COMMANDS, {
  "auto-analysis": { run: () => void openAutoAnalysis() },
  "stop-analysis": { run: () => void run({ type: "StopAutoAnalysis" }) },
} satisfies Record<string, CommandEntry>);

// Explicit keys preserve core refusal/status strings in the shared Java string extraction.
function noticeText(key: string): string {
  const text: Record<string, string> = {
    "AutoAnalyze.engineUnavailable": t("AutoAnalyze.engineUnavailable"),
    "AutoAnalyze.conflict": t("AutoAnalyze.conflict"),
    "AutoAnalyze.releaseFailed": t("AutoAnalyze.releaseFailed"),
    "AutoAnalyze.preparingStatus": t("AutoAnalyze.preparingStatus"),
    "BottomToolbar.stopAutoAnaHint": t("BottomToolbar.stopAutoAnaHint"),
    "BottomToolbar.stopAutoAnaStoppedHint": t("BottomToolbar.stopAutoAnaStoppedHint"),
    "Leelaz.autoAnalyzeComplete": t("Leelaz.autoAnalyzeComplete"),
    "Leelaz.autoAnalyzeStoppedSaved": t("Leelaz.autoAnalyzeStoppedSaved"),
  };
  return text[key] ?? t(key);
}
let lastNotice = -1, askingDiff = false, showingTip = false;
let notice: HTMLElement | null = null;
let noticeTimer: number | undefined;
function nonmodal(text: string, timed: boolean): void {
  notice?.remove(); window.clearTimeout(noticeTimer);
  notice = document.createElement("div"); notice.className = "auto-analysis-notice modal-box"; notice.dataset.key = "autoAnalysis.notice";
  const content = document.createElement("div"); content.textContent = text; notice.append(content);
  if (!timed) { const close = button(t("SetKataEngines.btnApply"), () => { notice?.remove(); notice = null; }); close.dataset.key = "autoAnalysis.noticeClose"; notice.append(close); }
  document.body.append(notice);
  if (timed) noticeTimer = window.setTimeout(() => { notice?.remove(); notice = null; }, 3000);
}
function render(view: WorkbenchView): void {
  const a = view.auto_analysis;
  if (a.notice && a.notice.id !== lastNotice) {
    lastNotice = a.notice.id;
    const text = a.notice.message_key === "BottomToolbar.startReAnalyze"
      ? `${t("BottomToolbar.noticeBigDiff")}${a.diff_moves}(${t("BottomToolbar.needAnalyze")}${a.diff_nodes})${t("BottomToolbar.startReAnalyze")}`
      : noticeText(a.notice.message_key) + (a.notice.directory ?? "");
    nonmodal(text, a.notice.timed);
  }
  if (a.confirmation && !askingDiff) {
    askingDiff = true;
    const body = document.createElement("div"); body.className = "auto-analysis-dialog";
    const content = document.createElement("p"); content.textContent = `${t("BottomToolbar.noticeBigDiff")}${a.diff_moves}(${t("BottomToolbar.needAnalyze")}${a.diff_nodes})${t("BottomToolbar.askStartReAnalyze")}`; body.append(content);
    const dialog = formPanel(t("BottomToolbar.askStartReAnalyzeTitle"), body);
    let answered = false;
    const answer = async (accept: boolean) => { answered = true; await run({ type: "AnswerAutoAnalysisDiff", accept }); await dialog.close(); };
    const yes = button(t("SetKataEngines.btnApply"), () => void answer(true)), no = button(t("SetKataEngines.btnCancel"), () => void answer(false));
    yes.dataset.key = "autoAnalysis.diffConfirm"; no.dataset.key = "autoAnalysis.diffCancel"; actions(body, yes, no);
    void dialog.closed.then(async () => { if (!answered) await run({ type: "AnswerAutoAnalysisDiff", accept: false }); askingDiff = false; });
  }
  if (a.pause_tip && !showingTip) {
    showingTip = true;
    const body = document.createElement("div"); const content = document.createElement("p"); content.textContent = t("LizzieFrame.autoAnalyze.tip.content"); body.append(content);
    const dialog = formPanel(t("LizzieFrame.autoAnalyze.tip.title"), body);
    let dismissed = false;
    const dismiss = async (never_again: boolean) => { dismissed = true; await run({ type: "DismissAutoAnalysisTip", never_again }); await dialog.close(); };
    const never = button(t("LizzieFrame.autoAnalyze.notShowAgain"), () => void dismiss(true)), ok = button(t("LizzieFrame.confirm"), () => void dismiss(false));
    never.dataset.key = "autoAnalysis.tipNever"; ok.dataset.key = "autoAnalysis.tipOk"; actions(body, never, ok);
    void dialog.closed.then(async () => { if (!dismissed) await run({ type: "DismissAutoAnalysisTip", never_again: false }); showingTip = false; });
  }
}
registerViewHandler(render);
