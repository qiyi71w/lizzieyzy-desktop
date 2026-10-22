// StartAnaDialog(false), SetDiffAnalyze and AutoAnalyzeMenu. Drafts only; Rust owns the run.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { AutoAnalysisSettings, WorkbenchView } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { actions, button } from "../engine/controls";
import { t } from "../i18n";
import type { MenuNode } from "../chrome/javaMenus";
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
function row(host: HTMLElement, key: string, control: HTMLElement): void {
  const label = document.createElement("label"); label.textContent = t(key);
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
  row(rows, "StartAnaDialog.analyzeBlack", black); row(rows, "StartAnaDialog.analyzeWhite", white);
  const win = check(draft.diffUseWin, "autoDiff.useWin"), score = check(draft.diffUseScore, "autoDiff.useScore");
  const winValue = number(draft.diffWinThreshold, "autoDiff.win", true), scoreValue = number(draft.diffScoreThreshold, "autoDiff.score", true);
  for (const [key, box, value] of [["SetDiffAnalyze.lblWinThreshold", win, winValue], ["SetDiffAnalyze.lblScoreThreshold", score, scoreValue]] as const) {
    const group = document.createElement("div"); group.className = "auto-analysis-inline"; group.append(box, value);
    value.disabled = !box.checked; box.onchange = () => { value.disabled = !box.checked; }; row(rows, key, group);
  }
  const time = number(draft.diffTime, "autoDiff.time"), visits = number(draft.diffVisits, "autoDiff.visits"), first = number(draft.diffFirstVisits, "autoDiff.firstVisits");
  row(rows, "StartAnaDialog.timePerMove", time); row(rows, "StartAnaDialog.totalVisitsPerMove", visits); row(rows, "StartAnaDialog.firstVisitsPerMove", first);
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

export async function openAutoAnalysis(): Promise<void> {
  const saved = state.view?.settings.auto_analysis;
  if (!saved) return;
  const draft = { ...saved };
  const { body, rows } = grid();
  const start = number(draft.startMove, "autoAnalysis.start"), end = number(draft.endMove, "autoAnalysis.end");
  const time = number(draft.timeSeconds, "autoAnalysis.time"), visits = number(draft.visits, "autoAnalysis.visits"), first = number(draft.firstVisits, "autoAnalysis.firstVisits");
  row(rows, "StartAnaDialog.startMove", start); row(rows, "StartAnaDialog.endMove", end);
  row(rows, "StartAnaDialog.timePerMove", time); row(rows, "StartAnaDialog.totalVisitsPerMove", visits); row(rows, "StartAnaDialog.firstVisitsPerMove", first);
  const black = check(draft.black, "autoAnalysis.black"), white = check(draft.white, "autoAnalysis.white"), branches = check(draft.allBranches, "autoAnalysis.allBranches"), diff = check(draft.diffEnable, "autoAnalysis.diff");
  row(rows, "StartAnaDialog.analyzeBlack", black); row(rows, "StartAnaDialog.analyzeWhite", white); row(rows, "StartAnaDialog.analyzeAllBranch", branches);
  const conditions = button(t("StartAnaDialog.btnSetDiff"), () => void openDiff(draft)); conditions.dataset.key = "autoAnalysis.conditions";
  conditions.disabled = !diff.checked; diff.onchange = () => { conditions.disabled = !diff.checked; draft.diffEnable = diff.checked; };
  const group = document.createElement("div"); group.className = "auto-analysis-inline"; group.append(diff, conditions); row(rows, "StartAnaDialog.lblDiffAnalyze", group);
  const pause = check(draft.exitByPause, "autoAnalysis.exitByPause"), save = check(draft.autoSave, "autoAnalysis.autoSave");
  row(rows, "StartAnaDialog.chkExitByPause", pause); row(rows, "StartAnaDialog.autoSaveKifu", save);
  const dialog = formPanel(t("StartAnaDialog.title"), body);
  const begin = button(t("StartAnaDialog.startAnalyze"), async () => {
    const settings: AutoAnalysisSettings = { ...draft, startMove: integer(start), endMove: integer(end), timeSeconds: integer(time), visits: integer(visits), firstVisits: integer(first), timeEnabled: integer(time) > 0, visitsEnabled: integer(visits) > 0, firstVisitsEnabled: integer(first) > 0, black: black.checked, white: white.checked, allBranches: branches.checked, diffEnable: diff.checked, exitByPause: pause.checked, autoSave: save.checked };
    const result = await run({ type: "StartAutoAnalysis", settings });
    if (result && result.type !== "Refused") await dialog.close();
  });
  begin.dataset.key = "autoAnalysis.begin";
  const stop = button(t("StartAnaDialog.stopAnalyze"), async () => { await dialog.close(); await run({ type: "StopAutoAnalysis" }); }); stop.dataset.key = "autoAnalysis.stop";
  actions(body, begin, stop);
  await dialog.closed;
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
