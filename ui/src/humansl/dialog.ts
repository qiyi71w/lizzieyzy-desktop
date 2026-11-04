// AI 陪练 setup (NewHumanSlGameDialog.java): header with 从当前局面开始 and 更多设置, the card with
// 训练方式 / AI 对手 / 执棋 / 用时 / 开始训练, the summary row, the 让子 / 贴目 fold, the download and
// preparation panel, and the footer hint with 取消. The dialog holds the draft only: the core owns
// the session and saves the settings once a game started; the model download is a resource job.
import { registerViewHandler, run, state } from "../app";
import type { HumanSlNotice, HumanSlStartupStage, HumanSlTrainingSettings, ResourceOutcome } from "../contract/generated";
import { openModalLayer } from "../dialogs";
import { format, t } from "../i18n";
import { openAutoSetup, resourceCommand, resourceView } from "../setup/dialog";
import { javaFormat, resourceText } from "../setup/setupText";
import { humanSlSession, humanSlUi, humanSlUiChanged } from "./state";
import "./humansl.css";

const ICON = "/assets/ui/ai-coach/coach-target.png";
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

function button(name: string, text: string, style: "primary" | "secondary", onClick: () => void): HTMLButtonElement {
  const b = keyed(el("button", `hs-button ${style}`, text), name);
  b.type = "button";
  b.onclick = onClick;
  return b;
}

function select(name: string, options: string[]): HTMLSelectElement {
  const s = keyed(el("select", "hs-choice"), name);
  for (const text of options) s.append(el("option", "", text));
  return s;
}

/** NewHumanSlGameDialog.field: a bold caption 6 px above its control. */
function field(label: string, control: HTMLElement): HTMLElement {
  const f = el("div", "hs-field");
  f.append(el("div", "hs-label", label), control);
  return f;
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
  return javaFormat(t("HumanSlGame.error.startFailed"), n.detail ?? failureReason(n.reason));
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

  const header = el("div", "hs-header");
  const icon = el("img", "hs-header-icon");
  icon.src = ICON;
  icon.alt = "";
  const copy = el("div", "hs-copy");
  copy.append(el("div", "hs-title", t("HumanSlTraining.title")), el("div", "hs-subtitle", t("HumanSlTraining.subtitle")));
  const headerActions = el("div", "hs-header-actions");
  const fromCurrent = keyed(el("input"), "fromCurrent");
  fromCurrent.type = "checkbox";
  const fromCurrentLabel = el("label", "hs-check");
  fromCurrentLabel.append(fromCurrent, t("HumanSlTraining.fromCurrent"));
  let more = saved.advanced_visible;
  const moreButton = button("more", "", "secondary", () => {
    more = !more;
    render();
  });
  headerActions.append(fromCurrentLabel, moreButton);
  header.append(icon, copy, headerActions);

  const mode = select("mode", [t("HumanSlTraining.mode.review"), t("HumanSlTraining.mode.live")]);
  let rankPreset = saved.training.opponent === "Rank";
  let dan = saved.training.danRank;
  const rankTab = button("opponentRank", t("HumanSlTraining.opponent.rank"), "secondary", () => { rankPreset = true; edited(); });
  const proTab = button("opponentPro", t("HumanSlTraining.opponent.pro"), "secondary", () => { rankPreset = false; edited(); });
  const tabs = el("div", "hs-segments");
  tabs.append(rankTab, proTab);
  const kyuTab = button("kyu", t("HumanSlTraining.rank.kyu"), "secondary", () => setDan(false));
  const danTab = button("dan", t("HumanSlTraining.rank.dan"), "secondary", () => setDan(true));
  const kind = el("div", "hs-segments");
  kind.append(kyuTab, danTab);
  const rankInput = keyed(el("input", "hs-rank"), "rank");
  rankInput.type = "number";
  rankInput.step = "1";
  rankInput.min = "1";
  let rank = saved.training.rank;
  const rankPanel = el("div", "hs-rank-panel");
  const rankRow = el("div", "hs-rank-row");
  rankRow.append(kind, rankInput);
  rankPanel.append(rankRow, el("div", "hs-hint", t("HumanSlTraining.rank.range")));
  const proStyle = select("proStyle", [t("HumanSlTraining.pro.modern"), t("HumanSlTraining.pro.online9d")]);
  const proPanel = el("div", "hs-pro-panel");
  proPanel.append(proStyle);
  const opponent = el("div", "hs-opponent");
  opponent.append(tabs, rankPanel, proPanel);
  const color = select("color", [t("HumanSlTraining.color.random"), t("HumanSlTraining.color.black"), t("HumanSlTraining.color.white")]);
  const time = select("time", [t("HumanSlTraining.time.10"), t("HumanSlTraining.time.30"), t("HumanSlTraining.time.60"), t("HumanSlTraining.time.unlimited")]);
  const startIcon = el("img", "hs-start-icon");
  startIcon.src = ICON;
  startIcon.alt = "";
  const startLabel = el("span");
  const startButton = button("start", "", "primary", () => void primaryAction());
  startButton.classList.add("hs-start");
  startButton.append(startIcon, startLabel);

  const modelStatus = keyed(el("span", "hs-model-status"), "modelStatus");
  const status = keyed(el("span", "hs-status"), "status");
  status.setAttribute("role", "status");
  status.setAttribute("aria-live", "polite");
  const summary = el("div", "hs-summary");
  summary.append(modelStatus, status);

  const handicap = select("handicap", Array.from({ length: 10 }, (_, k) => String(k)));
  const komi = keyed(el("input", "hs-komi"), "komi");
  komi.type = "text";
  // KomiDocument(true): a signed decimal.
  komi.addEventListener("input", () => {
    const kept = komi.value.replace(/[^\d.-]/g, "");
    if (kept !== komi.value) komi.value = kept;
  });
  const advanced = el("div", "hs-advanced");
  advanced.append(field(t("HumanSlTraining.handicap"), handicap), field(t("HumanSlTraining.komi"), komi), el("div", "hs-hint hs-more-hint", t("HumanSlTraining.moreHint")));

  const progress = keyed(el("div", "hs-progress"), "progress");
  const progressFill = el("div", "hs-progress-fill");
  const progressText = el("span", "hs-progress-text");
  progress.append(progressFill, progressText);
  const pauseButton = button("pause", t("HumanSlTraining.download.pause"), "secondary", () => void stopDownload(true));
  const cancelDownloadButton = button("downloadCancel", t("HumanSlTraining.download.cancel"), "secondary", () => void stopDownload(false));
  const downloadPanel = el("div", "hs-download");
  downloadPanel.append(progress, pauseButton, cancelDownloadButton);

  const card = el("div", "hs-card");
  card.append(
    field(t("HumanSlTraining.mode"), mode),
    field(t("HumanSlTraining.opponent"), opponent),
    field(t("HumanSlTraining.color"), color),
    field(t("HumanSlTraining.time"), time),
    startButton,
    summary,
    advanced,
    downloadPanel,
  );

  const footer = el("div", "hs-footer");
  footer.append(el("div", "hs-hint", t("HumanSlTraining.footer")), button("close", t("HumanSlTraining.close"), "secondary", () => void closeDialog()));

  root.append(header, card, footer);
  box.append(el("div", "modal-title", t("HumanSlTraining.title")), root);
  overlay.append(box);

  // restoreLastStartedSettings.
  const s = saved.training;
  mode.selectedIndex = s.mode === "LiveAnalysis" ? 1 : 0;
  proStyle.selectedIndex = s.opponent === "Online9d" ? 1 : 0;
  color.selectedIndex = Math.max(0, COLORS.indexOf(s.playerColor));
  time.selectedIndex = Math.max(0, TIMES.indexOf(s.moveTimeSeconds));
  handicap.selectedIndex = s.handicap;
  komi.value = String(s.komi);

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
  for (const control of [mode, proStyle, time, color]) control.addEventListener("change", edited);
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
      mode: mode.selectedIndex === 1 ? "LiveAnalysis" : "PostGameReview",
      opponent: rankPreset ? "Rank" : proStyle.selectedIndex === 1 ? "Online9d" : "ModernPro",
      danRank: dan,
      rank,
      playerColor: COLORS[color.selectedIndex] ?? "Random",
      moveTimeSeconds: TIMES[time.selectedIndex] ?? 10,
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
      : proStyle.options[proStyle.selectedIndex]?.text ?? "";
    const timeText = time.options[time.selectedIndex]?.text ?? "";
    return format(mode.selectedIndex === 1 ? t("HumanSlTraining.summary.live") : t("HumanSlTraining.summary.review"), opponentText, timeText);
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
    for (const control of [mode, rankTab, proTab, kyuTab, danTab, rankInput, proStyle, color, time, moreButton, handicap, komi]) control.disabled = !enabled;
    fromCurrent.disabled = !enabled || !canStartFromCurrent();
    if (!canStartFromCurrent()) fromCurrent.checked = false;
    fromCurrentLabel.classList.toggle("disabled", fromCurrent.disabled);
    moreButton.textContent = more ? t("HumanSlTraining.less") : t("HumanSlTraining.more");
    moreButton.classList.toggle("selected", more);
    // showOpponentCard / refreshSegmentStyle: the selected segment is a primary button.
    for (const [b, on] of [[rankTab, rankPreset], [proTab, !rankPreset], [kyuTab, !dan], [danTab, dan]] as const) {
      b.classList.toggle("primary", on);
      b.classList.toggle("secondary", !on);
    }
    rankPanel.hidden = !rankPreset;
    proPanel.hidden = rankPreset;
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
    startIcon.hidden = cleanup || repair;
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
    // packForContent / fitToScreen: 920 x 410, 500 with the fold, a download or the startup;
    // within the window less a 40 px margin, at least 860 x 390.
    const target = more || phase !== "idle" ? 500 : 410;
    box.style.height = `${Math.min(target, window.innerHeight - 40)}px`;
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

  /** startConfiguredGame: the core prepares the companion and reports the stages. */
  async function startGame(): Promise<void> {
    statusText = "";
    statusError = false;
    repair = false;
    failureText = "";
    preparingBase = { secs: 0, at: performance.now() };
    setPhase("starting");
    render();
    const result = await run({
      type: "HumanSl",
      op: { type: "Start", settings: settings(), from_current: fromCurrent.checked && canStartFromCurrent(), advanced_visible: more },
    });
    if (current !== dialog) return;
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
