// 补算 HumanSL (HumanSlStrengthDialog.prepare): the game, the engine, the model download prompt,
// then the modal "HumanSL 40" progress dialog (500 x 165) with its status, bar and 取消. The model
// download is the resource job AI 陪练 uses; the preparation itself is the core's manual task.
import { registerViewHandler, run, state } from "../app";
import { dispatch, isApiError } from "../contract/client";
import type { StrengthManualOutcome } from "../contract/generated";
import { chooseSave, message } from "../dialogs";
import { format, product, t } from "../i18n";
import { resourceCommand, resourceView } from "../setup/dialog";
import { humanSlText } from "./format";
import { openWindow } from "./window";

const TITLE = "HumanSL 40";

interface Progress {
  update(): void;
  report(): ProgressReport;
}

/** Acceptance probe of the progress dialog: its status text, the bar (value in percent, null
 *  while indeterminate) and 取消's client-area centre. */
export interface ProgressReport {
  status: string;
  determinate: boolean;
  value: number | null;
  cancel: { x: number; y: number } | null;
}

let current: Progress | null = null;

export function progressReport(): ProgressReport | null {
  return current?.report() ?? null;
}

/** errorText: a HumanSL40.* key as its text, any other error after HumanSL40.failed. */
function failureText(outcome: Extract<StrengthManualOutcome, { type: "Failed" }>): string {
  return outcome.detail !== null ? `${t("HumanSL40.failed")}\n${outcome.detail}` : humanSlText(outcome.message_key);
}

/** Sends RunHumanSl (`resume`: after the model download it needed, for the game it was asked
 *  for); a refusal is shown as prepare shows it. Resolves whether the run started, or "download"
 *  when only the HumanSL model is missing. */
async function startRun(resume: boolean): Promise<boolean | "download"> {
  let result;
  try {
    result = await dispatch({ type: "Strength", op: { type: "RunHumanSl", resume } });
  } catch (e) {
    await message("Error", isApiError(e) ? e.message : String(e), "error");
    return false;
  }
  if (result.type !== "Refused") return true;
  const key = result.message_key;
  if (key === "HumanSL40.downloadRequired") return "download";
  // needEngine: showMessageDialog; the task's own failures (the model check, the game replaced
  // during the download) are errors; the game's snapshot throws before the dialog (warning).
  const failure = key === "HumanSL40.invalidModel" || key === "HumanSL40.gameChanged";
  if (key === "HumanSL40.needEngine") await message(t("Message.title"), t(key));
  else await message(TITLE, humanSlText(key), failure ? "error" : "warning");
  return false;
}

/** Resolves once the dialog (or the prompt that ended the run) is gone. */
export async function prepareHumanSl(): Promise<void> {
  const previousRun = state.view?.strength.manual?.run ?? 0;
  const started = await startRun(false);
  if (started === true) return progressDialog(false, previousRun);
  // JOptionPane.YES_NO_OPTION titled "HumanSL 40".
  if (started === "download" && (await chooseSave(TITLE, t("HumanSL40.downloadPrompt"), product("yes"), product("no")))) {
    await progressDialog(true, previousRun);
  }
}

/** `previousRun`: the manual run before this dialog's; its outcome (still in the view until the
 *  new run is admitted) is not this dialog's result. Without `download` the run has started. */
function progressDialog(download: boolean, previousRun: number): Promise<void> {
  const done = Promise.withResolvers<void>();
  /** download: the model job; starting: RunHumanSl sent; running: the manual task's status. */
  let phase: "download" | "starting" | "running" | "ended" = download ? "download" : "running";
  let downloadTask: number | null = null;
  /** Read through a call: closures (the window's close) change `phase` while a command awaits. */
  const ended = () => phase === "ended";

  const status = document.createElement("div");
  status.className = "ps-progress-status";
  status.setAttribute("role", "status");
  status.textContent = t("HumanSL40.preparing");
  const bar = document.createElement("div");
  bar.className = "ps-progress-bar";
  bar.setAttribute("role", "progressbar");
  const fill = document.createElement("div");
  fill.className = "ps-progress-fill";
  bar.append(fill);
  const cancel = document.createElement("button");
  cancel.type = "button";
  cancel.className = "ps-native-button";
  cancel.textContent = t("HumanSL40.cancel");
  const actions = document.createElement("div");
  actions.className = "ps-progress-actions";
  actions.append(cancel);

  let barValue: number | null = 0;
  const setBar = (value: number | null, max = 100) => {
    barValue = value === null ? null : max > 0 ? Math.min(100, (value * 100) / max) : 0;
    bar.classList.toggle("indeterminate", barValue === null);
    fill.style.width = barValue === null ? "" : `${barValue}%`;
  };
  setBar(0);

  /** windowClosed: the download is cancelled, a running task too. */
  const win = openWindow(TITLE, "ps-progress-dialog", () => {
    const was = phase;
    phase = "ended";
    current = null;
    if (downloadTask !== null) void resourceCommand({ type: "CancelTask", task: downloadTask });
    else if (was === "running" || was === "starting") void run({ type: "Strength", op: { type: "CancelHumanSl" } });
    done.resolve();
  });
  win.body.append(status, bar, actions);
  cancel.onclick = () => win.close();
  cancel.focus();

  /** The task ended by itself: nothing left to cancel. */
  const finish = () => {
    phase = "ended";
    downloadTask = null;
    win.close();
  };

  async function startManual(): Promise<void> {
    phase = "starting";
    previousRun = state.view?.strength.manual?.run ?? 0;
    const started = await startRun(true);
    if (ended()) return;
    if (started !== true) {
      finish();
      return;
    }
    phase = "running";
    progress.update();
  }

  async function settle(outcome: StrengthManualOutcome): Promise<void> {
    phase = "ended";
    if (outcome.type === "Failed") await message(TITLE, failureText(outcome), "error");
    finish();
  }

  const progress: Progress = {
    update() {
      const view = state.view;
      if (!view) return;
      if (phase === "download") {
        const outcome = view.resources?.lastOutcome;
        if (downloadTask !== null && outcome?.task === downloadTask) {
          downloadTask = null;
          // A failed download is reported by the resource job's own error dialog.
          if (outcome.status === "done") void startManual();
          else finish();
          return;
        }
        const task = resourceView()?.task;
        if (task && task.id === downloadTask) {
          const bytes = Number(task.bytes);
          const total = task.total === null ? 0 : Number(task.total);
          status.textContent = t("HumanSL40.downloading") + (total > 0 ? ` ${Math.floor((bytes * 100) / total)}%` : "");
          setBar(null);
        }
      } else if (phase === "running") {
        const s = view.strength.status;
        status.textContent = format(humanSlText(s.key), s.completed, s.total);
        setBar(s.completed, s.total);
        const manual = view.strength.manual;
        if (manual && manual.run !== previousRun && !manual.running && manual.outcome) void settle(manual.outcome);
      }
    },
    report() {
      const r = cancel.getBoundingClientRect();
      return {
        status: status.textContent ?? "",
        determinate: barValue !== null,
        value: barValue,
        cancel: r.width > 0 ? { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) } : null,
      };
    },
  };
  current = progress;

  if (download) {
    void (async () => {
      const result = await resourceCommand({ type: "Download", item: { type: "HumanSlModel" } });
      if (ended()) {
        // Closed before the job's receipt: DownloadSession.cancel covers a job not yet seen.
        if (result?.type === "Started") void resourceCommand({ type: "CancelTask", task: result.task });
        return;
      }
      if (result?.type === "Started") downloadTask = result.task;
      else if (result?.type === "Done") void startManual();
      else finish();
    })();
  } else progress.update();
  return done.promise;
}

registerViewHandler(() => current?.update());
