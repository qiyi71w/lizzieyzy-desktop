// AI 陪练 setup (NewHumanSlGameDialog.java) as grouped rows: the subtitle, then 训练方式 / AI 对手 /
// 执棋 / 用时 / 从当前局面开始 with the note on what the rank means, the 让子 / 贴目 group behind
// 更多设置, the summary row and the download and preparation panel; the footer holds 更多设置, 开始训练
// and 取消. The dialog holds the draft only: the core owns the session and saves the settings once a
// game started; the model download is a resource job.
import { registerViewHandler, run, state } from "../app";
import type { HumanSlNotice, HumanSlStartupStage, HumanSlTrainingSettings, ResourceOutcome } from "../contract/generated";
import { icon } from "../design/icons";
import { stepper } from "../design/stepper";
import { ask, closeBox, openModalLayer } from "../dialogs";
import { footer, groupNote, groupTitle, row, segmented, switchRow } from "../engine/controls";
import { format, product, t } from "../i18n";
import { messageText } from "../errorText";
import { openAutoSetup, resourceCommand, resourceView } from "../setup/dialog";
import { javaFormat, resourceText } from "../setup/setupText";
import { humanSlSession, humanSlUi, humanSlUiChanged } from "./state";
import "./humansl.css";

/** HumanSlTrainingConfig move times by 用时 index. */
const TIMES = [10, 30, 60, 24 * 60 * 60];
const COLORS = ["Random", "Black", "White"] as const;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function keyed<T extends HTMLElement>(e: T, name: string): T {
  e.dataset.key = `humansl.dialog.${name}`;
  return e;
}

function button(name: string, text: string, onClick: () => void): HTMLButtonElement {
  const b = keyed(el("button", "", text), name);
  b.type = "button";
  b.onclick = onClick;
  return b;
}

function select(name: string, options: string[]): HTMLSelectElement {
  const s = keyed(el("select"), name);
  for (const text of options) s.append(el("option", "", text));
  return s;
}

/** Two to four exclusive choices as a segmented control, valued by their index. */
function choice(name: string, texts: string[], index: number) {
  const s = segmented(`humansl-${name}`, texts.map((text, k): [string, string] => [String(k), text]), String(index));
  keyed(s.el, name);
  return s;
}

/** updateEngineStartupStage's texts. */
function stageText(stage: HumanSlStartupStage | null): string {
  switch (stage) {
    case "LoadingModels": return t("HumanSlTraining.preparing.loading");
    case "OptimizingGpu": return t("HumanSlTraining.preparing.optimizing");
    case "CacheReady": return t("HumanSlTraining.preparing.cacheReady");
    case "Ready": return t("HumanSlTraining.preparing.ready");
    default: return t("HumanSlTraining.preparing");
  }
}

/** formatMegabytes. */
function megabytes(bytes: number): string {
  return `${(Math.max(0, bytes) / 1024 / 1024).toFixed(1)} MB`;
}

/** Utils.parseTextToDouble: the field's number, else the fallback. */
function parseKomi(text: string, fallback: number): number {
  const s = text.trim();
  if (!/^-?(\d+\.?\d*|\.\d+)$/.test(s)) return fallback;
  const n = Number(s);
  return Number.isFinite(n) ? n : fallback;
}

/** StartFailed reasons; literal t() calls keep the keys in the Java string extraction. */
function failureReason(key: string): string {
  const texts: Record<string, string> = {
    "HumanSlGame.error.noEngine": t("HumanSlGame.error.noEngine"),
    "HumanSlGame.error.tensorRtCompanionMissing": t("HumanSlGame.error.tensorRtCompanionMissing"),
    "HumanSlGame.error.liveUrlSgfActive": t("HumanSlGame.error.liveUrlSgfActive"),
    "HumanSlTraining.preparing.failedDetail": t("HumanSlTraining.preparing.failedDetail"),
    "AnalysisEngineCommandHelper.noEngineSelected": t("AnalysisEngineCommandHelper.noEngineSelected"),
  };
  return texts[key] ?? t(key);
}

/** The start failure as the dialog's inline error (showInlineError). */
export function startFailedText(n: Extract<HumanSlNotice, { type: "StartFailed" }>): string {
  // The detail's last lines may be a core error message (a settings write that failed).
  const detail = n.detail?.split("\n").map((line) => messageText(line)).join("\n");
  return javaFormat(t("HumanSlGame.error.startFailed"), detail ?? failureReason(n.reason));
}

/** failing: a start failed and the companion is still being stopped; cleanup: that stop could not
 *  restore the foreground engine and waits for 重试清理 (postCleanupRecoveryPending). */
type Phase = "idle" | "downloading" | "starting" | "preparing" | "failing" | "cleanup";

interface Dialog {
  update(): void;
  notice(n: HumanSlNotice): boolean;
  outcome(o: ResourceOutcome): void;
  focus(): void;
}

let current: Dialog | null = null;

export function humanSlDialogOpen(): boolean {
  return current !== null;
}

/** startHumanSlGameDialogReserved's dialog; an open one comes to the front instead. */
export function openHumanSlDialog(): void {
  if (current) {
    current.focus();
    return;
  }
  const view = state.view;
  if (!view) return;
  const saved = view.settings.human_sl;
  const previous = document.activeElement as HTMLElement | null;

  let phase: Phase = "idle";
  let downloadTask: number | null = null;
  /** pauseDownload (true) or cancelDownload (false) requested the stop. */
  let pausing = false;
  let paused = false;
  let statusText = "";
  let statusError = false;
  let downloadSeen = false;
  let preparingBase = { secs: 0, at: performance.now() };
  let elapsedTimer: number | undefined;
  /** A TensorRT start failure offers 一键设置's repair on the start button (pendingTensorRtRepairContext). */
  let repair = false;
  /** The start failure's inline error, shown again once a pending cleanup succeeded. */
  let failureText = "";

  // ---------------------------------------------------------------- controls
  const overlay = el("div", "modal");
  const box = el("div", "modal-box humansl-dialog");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", t("HumanSlTraining.title"));
  box.tabIndex = -1;
  const root = el("div", "hs-setup");
  // restoreLastStartedSettings: the draft starts from the last started game.
  const s = saved.training;

  let more = saved.advanced_visible;
  const moreButton = button("more", "", () => {
    more = !more;
    render();
    if (more) advanced.scrollIntoView({ block: "nearest" });
  });
  moreButton.classList.add("hs-more");

  const mode = choice("mode", [t("HumanSlTraining.mode.review"), t("HumanSlTraining.mode.live")], s.mode === "LiveAnalysis" ? 1 : 0);
  let rankPreset = s.opponent === "Rank";
  let dan = s.danRank;
  const opponent = choice("opponent", [t("HumanSlTraining.opponent.rank"), t("HumanSlTraining.opponent.pro")], rankPreset ? 0 : 1);
  keyed(opponent.inputs[0], "opponentRank");
  keyed(opponent.inputs[1], "opponentPro");
  const kind = choice("rankKind", [t("HumanSlTraining.rank.kyu"), t("HumanSlTraining.rank.dan")], dan ? 1 : 0);
  kind.el.setAttribute("aria-label", `${t("HumanSlTraining.rank.kyu")} / ${t("HumanSlTraining.rank.dan")}`);
  keyed(kind.inputs[0], "kyu");
  keyed(kind.inputs[1], "dan");
  const rankInput = keyed(el("input", "hs-rank"), "rank");
  rankInput.type = "number";
  rankInput.step = "1";
  rankInput.min = "1";
  rankInput.setAttribute("aria-label", t("HumanSlTraining.opponent.rank"));
  let rank = s.rank;
  const rankPanel = el("span", "hs-rank-panel");
  rankPanel.append(kind.el, stepper(rankInput));
  const proTexts = [t("HumanSlTraining.pro.modern"), t("HumanSlTraining.pro.online9d")];
  const proStyle = choice("proStyle", proTexts, s.opponent === "Online9d" ? 1 : 0);
  proStyle.el.setAttribute("aria-label", t("HumanSlTraining.opponent.pro"));
  const color = choice("color", [t("HumanSlTraining.color.random"), t("HumanSlTraining.color.black"), t("HumanSlTraining.color.white")], Math.max(0, COLORS.indexOf(s.playerColor)));
  const timeTexts = [t("HumanSlTraining.time.10"), t("HumanSlTraining.time.30"), t("HumanSlTraining.time.60"), t("HumanSlTraining.time.unlimited")];
  const time = choice("time", timeTexts, Math.max(0, TIMES.indexOf(s.moveTimeSeconds)));
  const startIcon = icon("spark");
  const startLabel = el("span");
  const startButton = button("start", "", () => void primaryAction());
  startButton.classList.add("hs-start");
  startButton.append(startIcon, startLabel);

  const modelStatus = keyed(el("span", "hs-model-status"), "modelStatus");
  const status = keyed(el("span", "hs-status"), "status");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  const summary = el("div", "hs-summary");
  summary.append(modelStatus, status);

  const handicap = select("handicap", Array.from({ length: 10 }, (_, k) => String(k)));
  handicap.selectedIndex = s.handicap;
  const komi = keyed(el("input", "hs-komi"), "komi");
  komi.type = "text";
  komi.inputMode = "decimal";
  komi.value = String(s.komi);
  // KomiDocument(true): a signed decimal.
  komi.addEventListener("input", () => {
    const kept = komi.value.replace(/[^\d.-]/g, "");
    if (kept !== komi.value) komi.value = kept;
  });

  const progress = keyed(el("div", "hs-progress"), "progress");
  const progressFill = el("div", "hs-progress-fill");
  progress.append(progressFill);
  const progressText = el("span", "hs-progress-text");
  const pauseButton = button("pause", t("HumanSlTraining.download.pause"), () => void stopDownload(true));
  const cancelDownloadButton = button("downloadCancel", t("HumanSlTraining.download.cancel"), () => void stopDownload(false));
  const downloadPanel = el("div", "hs-download");
  downloadPanel.append(progress, progressText, pauseButton, cancelDownloadButton);

  // The training's rows with the note on what the rank means; AI 对手's description is the rank
  // range, shown while 按段位 is chosen.
  const form = el("div");
  row(form, t("HumanSlTraining.mode"), "", mode.el);
  const opponentRow = row(form, t("HumanSlTraining.opponent"), t("HumanSlTraining.rank.range"), opponent.el, rankPanel, proStyle.el);
  const rankRange = opponentRow.querySelector<HTMLElement>(".ds-row-sub")!;
  row(form, t("HumanSlTraining.color"), "", color.el);
  row(form, t("HumanSlTraining.time"), "", time.el);
  const fromCurrent = keyed(switchRow(form, t("HumanSlTraining.fromCurrent"), "", false).input, "fromCurrent");
  groupNote(form, t("HumanSlTraining.footer"));
  const advanced = el("div");
  groupTitle(advanced, t("HumanSlTraining.more"));
  row(advanced, t("HumanSlTraining.handicap"), "", handicap);
  row(advanced, t("HumanSlTraining.komi"), "", komi);
  groupNote(advanced, t("HumanSlTraining.moreHint"));

  root.append(el("p", "hs-subtitle", t("HumanSlTraining.subtitle")), form, advanced, summary, downloadPanel);
  const caption = el("span");
  caption.append(icon("spark"), t("HumanSlTraining.title"));
  const title = el("div", "modal-title hs-title");
  title.append(caption, keyed(closeBox(() => void closeDialog()), "closeBox"));
  box.append(title, root);
  footer(box, { start: [moreButton], primary: startButton, cancel: button("close", t("HumanSlTraining.close"), () => void closeDialog()) });
  overlay.append(box);

  function maxRank(): number {
    return dan ? 9 : 20;
  }

  /** updateRankModel: the value is kept within the new range. */
  function setDan(value: boolean): void {
    dan = value;
    rank = Math.min(rank, maxRank());
    edited();
  }

  function readRank(): void {
    const n = Number.parseInt(rankInput.value, 10);
    if (Number.isFinite(n)) rank = Math.min(Math.max(1, n), maxRank());
  }

  rankInput.addEventListener("change", () => { readRank(); edited(); });
  for (const r of [...mode.inputs, ...proStyle.inputs, ...time.inputs, ...color.inputs]) r.addEventListener("change", edited);
  for (const r of opponent.inputs) r.addEventListener("change", () => { rankPreset = opponent.value() === "0"; edited(); });
  for (const r of kind.inputs) r.addEventListener("change", () => setDan(kind.value() === "1"));
  // updateKomiForHandicap.
  handicap.addEventListener("change", () => {
    komi.value = handicap.selectedIndex >= 2 ? "0" : "7.5";
    edited();
  });

  /** A form change shows the summary again (updateTrainingSummary resets an inline error). */
  function edited(): void {
    if (phase === "idle") {
      statusText = "";
      statusError = false;
    }
    render();
  }

  /** selectedConfig. */
  function settings(): HumanSlTrainingSettings {
    readRank();
    const h = handicap.selectedIndex;
    return {
      mode: mode.value() === "1" ? "LiveAnalysis" : "PostGameReview",
      opponent: rankPreset ? "Rank" : proStyle.value() === "1" ? "Online9d" : "ModernPro",
      danRank: dan,
      rank,
      playerColor: COLORS[Number(color.value())] ?? "Random",
      moveTimeSeconds: TIMES[Number(time.value())] ?? 10,
      handicap: h,
      komi: parseKomi(komi.value, h >= 2 ? 0 : 7.5),
    };
  }

  function canStartFromCurrent(): boolean {
    return (state.view?.position.move_number ?? 0) > 0;
  }

  function modelInstalled(): boolean {
    return state.view?.settings.human_sl.model.status === "Installed";
  }

  /** updateTrainingSummary. */
  function summaryText(): string {
    const opponentText = rankPreset
      ? format(dan ? t("HumanSlTraining.rank.danValue") : t("HumanSlTraining.rank.kyuValue"), rank)
      : proTexts[Number(proStyle.value())] ?? "";
    const timeText = timeTexts[Number(time.value())] ?? "";
    return format(mode.value() === "1" ? t("HumanSlTraining.summary.live") : t("HumanSlTraining.summary.review"), opponentText, timeText);
  }

  function elapsedSeconds(): number {
    return preparingBase.secs + Math.floor((performance.now() - preparingBase.at) / 1000);
  }

  function setPhase(next: Phase): void {
    phase = next;
    humanSlUi.dialogOwnsSession = next === "starting" || next === "preparing" || next === "failing" || next === "cleanup";
    const downloading = next === "downloading";
    if (humanSlUi.downloading !== downloading) {
      humanSlUi.downloading = downloading;
      humanSlUiChanged();
    }
    const preparing = next === "starting" || next === "preparing";
    if (preparing && elapsedTimer === undefined) elapsedTimer = window.setInterval(render, 1000);
    if (!preparing && elapsedTimer !== undefined) {
      window.clearInterval(elapsedTimer);
      elapsedTimer = undefined;
    }
  }

  function render(): void {
    const enabled = phase === "idle";
    const preparing = phase === "starting" || phase === "preparing";
    // setFormEnabled.
    const choices = [...mode.inputs, ...opponent.inputs, ...kind.inputs, ...proStyle.inputs, ...color.inputs, ...time.inputs];
    for (const control of [...choices, rankInput, moreButton, handicap, komi]) control.disabled = !enabled;
    fromCurrent.disabled = !enabled || !canStartFromCurrent();
    if (!canStartFromCurrent()) fromCurrent.checked = false;
    moreButton.textContent = more ? t("HumanSlTraining.less") : t("HumanSlTraining.more");
    moreButton.setAttribute("aria-expanded", String(more));
    // showOpponentCard: the chosen opponent kind shows its own controls.
    opponent.inputs[rankPreset ? 0 : 1].checked = true;
    kind.inputs[dan ? 1 : 0].checked = true;
    rankPanel.hidden = rankRange.hidden = !rankPreset;
    proStyle.el.hidden = rankPreset;
    rankInput.max = String(maxRank());
    if (rankInput.value !== String(rank)) rankInput.value = String(rank);
    advanced.hidden = !more;
    const installed = modelInstalled();
    modelStatus.textContent = installed ? t("HumanSlTraining.model.ready") : t("HumanSlTraining.model.missing");
    // The start button: 重试清理 (styleDanger) while cleanup is pending, the TensorRT repair after
    // such a failure (presentPreparationFailure), else 开始训练 / 下载并开始 / 继续下载并开始.
    const cleanup = phase === "cleanup";
    startButton.disabled = cleanup ? humanSlSession()?.phase !== "CleanupPending" : !enabled;
    startButton.classList.toggle("danger", cleanup);
    startButton.classList.toggle("primary", !cleanup);
    startIcon.toggleAttribute("hidden", cleanup || repair);
    startLabel.textContent = cleanup ? t("HumanSlTraining.retryCleanup")
      : repair ? t("EngineFailedMessage.openTensorRtRepair")
      : paused ? t("HumanSlTraining.download.resume")
      : installed ? t("HumanSlTraining.start") : t("HumanSlTraining.downloadAndStart");
    if (repair && !cleanup) {
      startButton.setAttribute("aria-label", t("EngineFailedMessage.openTensorRtRepairAccessibleName"));
      startButton.setAttribute("aria-description", t("EngineFailedMessage.openTensorRtRepairAccessibleDescription"));
    } else {
      startButton.removeAttribute("aria-label");
      startButton.removeAttribute("aria-description");
    }
    // The download panel serves the download (with 暂停 / 取消) and the startup (bar only).
    downloadPanel.hidden = !(phase === "downloading" || preparing);
    pauseButton.hidden = cancelDownloadButton.hidden = phase !== "downloading";
    let text = statusText;
    if (preparing) {
      const session = humanSlSession();
      text = stageText(phase === "preparing" ? session?.stage ?? null : null);
      progress.classList.add("indeterminate");
      progressFill.style.width = "";
      progressText.textContent = format(t("HumanSlTraining.preparing.elapsed"), elapsedSeconds());
    } else if (phase === "downloading") {
      const task = resourceView()?.task;
      if (task && task.id === downloadTask) {
        downloadSeen = true;
        text = t("HumanSlTraining.download.running");
        // updateDownloadProgress.
        const bytes = Number(task.bytes);
        const total = task.total === null ? 0 : Number(task.total);
        progress.classList.toggle("indeterminate", total <= 0);
        progressFill.style.width = total > 0 ? `${Math.min(100, (bytes * 100) / total)}%` : "";
        progressText.textContent = total > 0 ? `${Math.floor((bytes * 100) / total)}%  ${megabytes(bytes)} / ${megabytes(total)}` : megabytes(bytes);
        pauseButton.disabled = cancelDownloadButton.disabled = !task.cancellable;
      } else if (!downloadSeen) {
        text = t("HumanSlTraining.download.starting");
        progress.classList.remove("indeterminate");
        progressFill.style.width = "0%";
        progressText.textContent = "0%";
      }
    } else if (!text) text = summaryText();
    status.textContent = text;
    status.classList.toggle("error", statusError && (phase === "idle" || phase === "failing" || phase === "cleanup"));
    // Java's ESC binding is on the dialog's root pane: a control disabled while it had the focus
    // (开始训练 during a start) must not leave the keyboard outside the modal dialog.
    if (box.isConnected && !box.contains(document.activeElement)) box.focus();
  }

  // ---------------------------------------------------------------- actions
  /** onPrimaryAction: a missing model is downloaded first. */
  async function primaryAction(): Promise<void> {
    // 重试清理: the session's exit runs again.
    if (phase === "cleanup") {
      if (humanSlSession()?.phase === "CleanupPending") await run({ type: "HumanSl", op: { type: "Finish" } });
      return;
    }
    if (phase !== "idle") return;
    // openPendingTensorRtRepair: 一键设置 at its TensorRT section. Its window is not modal, so this
    // dialog closes first instead of staying above it.
    if (repair) {
      repair = false;
      dispose();
      await openAutoSetup("acceleration");
      return;
    }
    if (!modelInstalled()) {
      await startDownload();
      return;
    }
    await startGame();
  }

  /** startConfiguredGame: the core prepares the companion and reports the stages. A running batch
   *  analysis is stopped first once the user agrees (`stopBatch`). */
  async function startGame(stopBatch = false): Promise<void> {
    statusText = "";
    statusError = false;
    repair = false;
    failureText = "";
    preparingBase = { secs: 0, at: performance.now() };
    setPhase("starting");
    render();
    const result = await run({
      type: "HumanSl",
      op: {
        type: "Start",
        settings: settings(),
        from_current: fromCurrent.checked && canStartFromCurrent(),
        advanced_visible: more,
        stop_batch: stopBatch,
      },
    });
    if (current !== dialog) return;
    if (result?.type === "NeedsBatchStop") {
      const body = document.createElement("div");
      body.textContent = product("humanSlStopBatch");
      const stop = await ask(t("Menu.aiCoach"), body, product("humanSlStopBatchOk"), product("cancel"), true);
      if (current !== dialog) return;
      if (stop) return startGame(true);
      setPhase("idle");
      render();
      return;
    }
    if (!result || result.type === "Refused") {
      setPhase("idle");
      render();
    }
  }

  /** startModelDownload. */
  async function startDownload(): Promise<void> {
    paused = false;
    pausing = false;
    downloadSeen = false;
    statusText = "";
    statusError = false;
    downloadTask = null;
    setPhase("downloading");
    render();
    const result = await resourceCommand({ type: "Download", item: { type: "HumanSlModel" } });
    if (current !== dialog) return;
    if (result?.type === "Started") {
      downloadTask = result.task;
      render();
    } else if (result?.type === "Done") {
      setPhase("idle");
      await startGame();
    } else {
      setPhase("idle");
      render();
    }
  }

  /** pauseDownload / cancelDownload: both cancel the job; a pause offers 继续下载并开始. */
  async function stopDownload(pause: boolean): Promise<void> {
    if (phase !== "downloading" || downloadTask === null) return;
    pausing = pause;
    await resourceCommand({ type: "CancelTask", task: downloadTask });
  }

  /** handleDownloadStopped. */
  function downloadStopped(o: ResourceOutcome): void {
    downloadTask = null;
    setPhase("idle");
    if (o.status === "cancelled" && pausing) {
      paused = true;
      statusText = t("HumanSlTraining.download.paused");
    } else if (o.status === "cancelled") {
      statusText = t("HumanSlTraining.download.cancelled");
    } else {
      const reason = resourceText(o.messageKey);
      statusText = format(t("HumanSlTraining.download.failed"), o.detail ? `${reason} ${o.detail}` : reason);
    }
    statusError = false;
    pausing = false;
    render();
  }

  function dispose(): void {
    setPhase("idle");
    closeLayer();
    current = null;
    humanSlUi.focusDialog = null;
    previous?.focus?.();
  }

  /** closeDialog: the download is cancelled, a startup stops (the core restores the foreground).
   *  While a failed startup is being cleaned up the dialog stays; with cleanup pending, closing
   *  retries it (retryRunnerCleanup / retryPostCleanupRecovery). */
  async function closeDialog(): Promise<void> {
    if (phase === "failing") return;
    if (phase === "cleanup") {
      if (humanSlSession()?.phase === "CleanupPending") await run({ type: "HumanSl", op: { type: "Finish" } });
      return;
    }
    const task = downloadTask;
    const preparing = phase === "starting" || phase === "preparing";
    dispose();
    if (task !== null) await resourceCommand({ type: "CancelTask", task });
    if (preparing || humanSlSession()?.phase === "Preparing") await run({ type: "HumanSl", op: { type: "CancelPreparation" } });
  }

  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      void closeDialog();
    } else if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement) && !(e.target instanceof HTMLSelectElement)) {
      // getRootPane().setDefaultButton(startButton).
      e.preventDefault();
      if (e.target === rankInput) readRank();
      if (!startButton.disabled) void primaryAction();
    }
  });

  const dialog: Dialog = {
    update() {
      const session = humanSlSession();
      if (phase === "starting" || phase === "preparing") {
        if (session?.phase === "Preparing") {
          if (phase !== "preparing" || session.preparing_secs !== preparingBase.secs) preparingBase = { secs: session.preparing_secs, at: performance.now() };
          phase = "preparing";
        } else if (session?.phase === "Playing") {
          // completeRunnerPreparation: only a game that started hides the dialog.
          dispose();
          return;
        } else if (session?.phase === "CleanupPending") setPhase("cleanup");
      } else if (phase === "failing") {
        if (!session) setPhase("idle");
        else if (session.phase === "CleanupPending") setPhase("cleanup");
      } else if (phase === "cleanup" && !session) setPhase("idle");
      render();
    },
    notice(n) {
      const ours = phase === "starting" || phase === "preparing" || phase === "failing" || phase === "cleanup";
      if (!ours) return false;
      if (n.type === "StartFailed") {
        // presentPreparationFailure: the inline error; the form stays disabled until the
        // companion is stopped (Ended), then comes back with the user's draft.
        statusText = failureText = startFailedText(n);
        statusError = true;
        repair = n.reason === "HumanSlGame.error.tensorRtCompanionMissing";
        setPhase(humanSlSession() ? "failing" : "idle");
        render();
        return true;
      }
      if (n.type === "ExitRecoveryFailed") {
        // retainPostCleanupRecovery: 重试清理 in place of the start button, inline error.
        statusText = t("AnalysisEngine.foregroundRestoreFailed");
        statusError = true;
        repair = false;
        setPhase("cleanup");
        render();
        return true;
      }
      if (n.type === "Ended") {
        // A cleanup that finished shows the start failure again (completeFailedRunnerPreparation).
        if (phase === "cleanup" && failureText) statusText = failureText;
        setPhase("idle");
        render();
        return true;
      }
      return false;
    },
    outcome(o) {
      if (phase !== "downloading" || o.task !== downloadTask) return;
      if (o.status === "done") {
        // The core remembered the model path; the game starts with the current settings.
        downloadTask = null;
        setPhase("idle");
        void startGame();
      } else downloadStopped(o);
    },
    focus() {
      box.focus();
    },
  };

  current = dialog;
  humanSlUi.focusDialog = dialog.focus;
  const closeLayer = openModalLayer(overlay);
  // A session already preparing (the dialog opened again) shows its startup.
  if (humanSlSession()?.phase === "Preparing") setPhase("preparing");
  render();
  startButton.focus();
}

/** The notices of the current view the dialog took (its startup's outcome), so the session's
 *  messages do not show them again. */
let taken = new Set<HumanSlNotice>();

registerViewHandler((view) => {
  taken = new Set();
  const dialog = current;
  if (!dialog) return;
  const outcome = view.resources?.lastOutcome;
  if (outcome) dialog.outcome(outcome);
  // The startup's outcome first, then the session state it leaves.
  for (const n of view.human_sl_notices) if (current?.notice(n)) taken.add(n);
  current?.update();
});

/** Whether the dialog took a session notice of the current view. */
export function dialogNotice(n: HumanSlNotice): boolean {
  return taken.has(n);
}
