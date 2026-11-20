// StartAnaDialog (the automatic analysis and the two batch variants), SetDiffAnalyze and
// AutoAnalyzeMenu. Drafts only; Rust owns the run. The dialogs are grouped rows
// (engine/controls.ts): untitled groups of fields and switches, the buttons in the footer.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { AutoAnalysisSettings, WorkbenchView } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { actions, button, primaryButton, row, switchBox, switchRow } from "../engine/controls";
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

/** The next group of rows in `body`: the groups have no titles, a gap parts them. */
function group(body: HTMLElement): HTMLDivElement {
  const el = document.createElement("div"); el.className = "auto-analysis-group";
  body.append(el);
  return el;
}
function number(value: number, key: string, decimal = false): HTMLInputElement {
  const el = document.createElement("input"); el.type = "text"; el.inputMode = decimal ? "decimal" : "numeric";
  el.value = value >= 0 ? String(value) : ""; el.dataset.key = el.id = key;
  return el;
}
/** A switch row; its switch carries `key`. */
function toggle(host: HTMLElement, text: string, value: boolean, key: string): HTMLInputElement {
  const { input } = switchRow(host, text, "", value);
  input.dataset.key = input.id = key;
  return input;
}
const integer = (el: HTMLInputElement) => parseJavaInt(el.value.replaceAll(" ", "")) ?? -1;

/** Which side, the two fluctuation thresholds, then the three work limits; Java's optional
 *  condition fields use -1 for empty. */
async function openDiff(draft: AutoAnalysisSettings): Promise<void> {
  const body = document.createElement("div"); body.className = "auto-analysis-dialog";
  const sides = group(body);
  const black = toggle(sides, t("StartAnaDialog.analyzeBlack"), draft.diffBlack, "autoDiff.black"), white = toggle(sides, t("StartAnaDialog.analyzeWhite"), draft.diffWhite, "autoDiff.white");
  // Each threshold: its value, then the switch that uses it and enables the value.
  const thresholds = group(body);
  const win = switchBox(draft.diffUseWin), score = switchBox(draft.diffUseScore);
  win.dataset.key = "autoDiff.useWin"; score.dataset.key = "autoDiff.useScore";
  const winValue = number(draft.diffWinThreshold, "autoDiff.win", true), scoreValue = number(draft.diffScoreThreshold, "autoDiff.score", true);
  for (const [text, box, value] of [[t("SetDiffAnalyze.lblWinThreshold"), win, winValue], [t("SetDiffAnalyze.lblScoreThreshold"), score, scoreValue]] as const) {
    box.setAttribute("aria-label", text); value.setAttribute("aria-label", text);
    value.disabled = !box.checked; box.onchange = () => { value.disabled = !box.checked; };
    row(thresholds, text, "", value, box);
  }
  const limits = group(body);
  const time = number(draft.diffTime, "autoDiff.time"), visits = number(draft.diffVisits, "autoDiff.visits"), first = number(draft.diffFirstVisits, "autoDiff.firstVisits");
  row(limits, t("StartAnaDialog.timePerMove"), "", time); row(limits, t("StartAnaDialog.totalVisitsPerMove"), "", visits); row(limits, t("StartAnaDialog.firstVisitsPerMove"), "", first);
  const dialog = formPanel(t("SetDiffAnalyze.title"), body);
  const apply = primaryButton(t("SetKataEngines.btnApply"), async () => {
    if (!black.checked && !white.checked) { await message(t("Message.title"), t("SetDiffAnalyze.noBlackWhite")); return; }
    if (!win.checked && !score.checked) { await message(t("Message.title"), t("SetDiffAnalyze.noWinScore")); return; }
    if (integer(time) <= 0 && integer(visits) <= 0 && integer(first) <= 0) { await message(t("Message.title"), t("SetDiffAnalyze.noCondition")); return; }
    const w = Number(winValue.value.trim()), s = Number(scoreValue.value.trim());
    if (!Number.isFinite(w) || !Number.isFinite(s)) return;
    // SetDiffAnalyze.java:192-216 writes only the conditions; the parent's 差异分析 switch is
    // saved when 开始分析 starts the run (StartAnaDialog.java:392-393).
    const conditions = { diffBlack: black.checked, diffWhite: white.checked, diffUseWin: win.checked, diffUseScore: score.checked, diffWinThreshold: w, diffScoreThreshold: s, diffTime: integer(time), diffVisits: integer(visits), diffFirstVisits: integer(first) };
    const saved = state.view?.settings.auto_analysis ?? draft;
    const result = await run({ type: "SaveAutoAnalysisSettings", settings: { ...saved, ...conditions } });
    if (!result || result.type === "Refused") return;
    Object.assign(draft, conditions); await dialog.close();
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
  const body = document.createElement("div"); body.className = "auto-analysis-dialog";
  const moves = group(body);
  const start = number(draft.startMove, "autoAnalysis.start"), end = number(draft.endMove, "autoAnalysis.end");
  const time = number(draft.timeSeconds, "autoAnalysis.time"), visits = number(draft.visits, "autoAnalysis.visits"), first = number(draft.firstVisits, "autoAnalysis.firstVisits");
  row(moves, batch ? t("StartAnaDialog.startMoveBatch") : t("StartAnaDialog.startMove"), "", start); row(moves, t("StartAnaDialog.endMove"), "", end);
  const limits = group(body);
  row(limits, t("StartAnaDialog.timePerMove"), "", time); row(limits, t("StartAnaDialog.totalVisitsPerMove"), "", visits); row(limits, t("StartAnaDialog.firstVisitsPerMove"), "", first);
  const sides = group(body);
  const black = toggle(sides, t("StartAnaDialog.analyzeBlack"), draft.black, "autoAnalysis.black"), white = toggle(sides, t("StartAnaDialog.analyzeWhite"), draft.white, "autoAnalysis.white"), branches = toggle(sides, t("StartAnaDialog.analyzeAllBranch"), draft.allBranches, "autoAnalysis.allBranches");
  const options = group(body);
  // 波动过大时加强分析: 条件设置 before the switch that enables it.
  const diff = switchBox(draft.diffEnable); diff.dataset.key = diff.id = "autoAnalysis.diff"; diff.setAttribute("aria-label", t("StartAnaDialog.lblDiffAnalyze"));
  const conditions = button(t("StartAnaDialog.btnSetDiff"), () => void openDiff(draft)); conditions.dataset.key = "autoAnalysis.conditions";
  conditions.disabled = !diff.checked; diff.onchange = () => { conditions.disabled = !diff.checked; };
  row(options, t("StartAnaDialog.lblDiffAnalyze"), "", conditions, diff);
  const pause = toggle(options, t("StartAnaDialog.chkExitByPause"), draft.exitByPause, "autoAnalysis.exitByPause");
  const save = batch ? null : toggle(options, t("StartAnaDialog.autoSaveKifu"), draft.autoSave, "autoAnalysis.autoSave");
  const dialog = formPanel(t("StartAnaDialog.title"), body);
  let started = false;
  const begin = primaryButton(t("StartAnaDialog.startAnalyze"), async () => {
    const settings: AutoAnalysisSettings = { ...draft, startMove: integer(start), endMove: integer(end), timeSeconds: integer(time), visits: integer(visits), firstVisits: integer(first), timeEnabled: integer(time) > 0, visitsEnabled: integer(visits) > 0, firstVisitsEnabled: integer(first) > 0, black: black.checked, white: white.checked, allBranches: branches.checked, diffEnable: diff.checked, exitByPause: pause.checked, autoSave: save ? save.checked : draft.autoSave };
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
  const body = document.createElement("div"); body.className = "auto-analysis-dialog";
  const moves = group(body);
  const start = number(range.startMove, "flashBatch.start"), end = number(range.endMove, "flashBatch.end");
  row(moves, t("StartAnaDialog.startMoveBatch"), "", start); row(moves, t("StartAnaDialog.endMove"), "", end);
  const work = group(body);
  const visits = number(savedVisits(), "flashBatch.visits");
  row(work, t("StartAnaDialog.totalVisitsPerMove"), "", visits);
  const branches = toggle(work, t("StartAnaDialog.chkAllBranches"), false, "flashBatch.allBranches");
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
  const ok = primaryButton(t("StartAnaDialog.startAnalyze"), () => void begin()); ok.dataset.key = "flashBatch.begin";
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
/** A non-modal tool window over the workbench (no scrim): the message, then 确定 unless it
 *  closes by itself after three seconds. */
function nonmodal(text: string, timed: boolean): void {
  notice?.remove(); window.clearTimeout(noticeTimer);
  notice = document.createElement("div"); notice.className = "auto-analysis-notice modal-box"; notice.dataset.key = "autoAnalysis.notice";
  notice.setAttribute("role", "status");
  const content = document.createElement("p"); content.textContent = text; notice.append(content);
  if (!timed) { const close = primaryButton(t("SetKataEngines.btnApply"), () => { notice?.remove(); notice = null; }); close.dataset.key = "autoAnalysis.noticeClose"; actions(notice, close); }
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
    const body = document.createElement("div"); body.className = "auto-analysis-message";
    const content = document.createElement("p"); content.textContent = `${t("BottomToolbar.noticeBigDiff")}${a.diff_moves}(${t("BottomToolbar.needAnalyze")}${a.diff_nodes})${t("BottomToolbar.askStartReAnalyze")}`; body.append(content);
    const dialog = formPanel(t("BottomToolbar.askStartReAnalyzeTitle"), body);
    let answered = false;
    const answer = async (accept: boolean) => { answered = true; await run({ type: "AnswerAutoAnalysisDiff", accept }); await dialog.close(); };
    const yes = primaryButton(t("SetKataEngines.btnApply"), () => void answer(true)), no = button(t("SetKataEngines.btnCancel"), () => void answer(false));
    yes.dataset.key = "autoAnalysis.diffConfirm"; no.dataset.key = "autoAnalysis.diffCancel"; actions(body, yes, no);
    void dialog.closed.then(async () => { if (!answered) await run({ type: "AnswerAutoAnalysisDiff", accept: false }); askingDiff = false; });
  }
  if (a.pause_tip && !showingTip) {
    showingTip = true;
    const body = document.createElement("div"); body.className = "auto-analysis-message"; const content = document.createElement("p"); content.textContent = t("LizzieFrame.autoAnalyze.tip.content"); body.append(content);
    const dialog = formPanel(t("LizzieFrame.autoAnalyze.tip.title"), body);
    let dismissed = false;
    const dismiss = async (never_again: boolean) => { dismissed = true; await run({ type: "DismissAutoAnalysisTip", never_again }); await dialog.close(); };
    const never = button(t("LizzieFrame.autoAnalyze.notShowAgain"), () => void dismiss(true)), ok = primaryButton(t("LizzieFrame.confirm"), () => void dismiss(false));
    never.dataset.key = "autoAnalysis.tipNever"; ok.dataset.key = "autoAnalysis.tipOk"; actions(body, never, ok);
    void dialog.closed.then(async () => { if (!dismissed) await run({ type: "DismissAutoAnalysisTip", never_again: false }); showingTip = false; });
  }
}
registerViewHandler(render);
