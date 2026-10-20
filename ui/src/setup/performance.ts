// KataGoAutoSetupDialog.java:1728-1835,3237-3517; Java benchmark report and actions.
import { registerViewHandler, run, state } from "../app";
import type { TuningView, WorkbenchView } from "../contract/generated";
import { confirm, formPanel, message } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button } from "../engine/controls";
import { plainJava, tuningText } from "../engine/tuningText";
import type { SetupCard, SetupContext } from "./card";
import "./performance.css";

let latest: TuningView | null = null;
const listeners = new Set<() => void>();
export const tuningView = (): TuningView | null => latest;
export function onTuningChange(listener: () => void): () => void { listeners.add(listener); return () => { listeners.delete(listener); }; }
const seenOutcomes = new Set<string>();
let startupNotice: HTMLElement | null = null;
registerViewHandler((view) => {
  // WorkbenchView is a delta: unrelated publications must not clear the last tuning state.
  if (view.tuning) {
    latest = view.tuning;
    const outcome = latest.outcome;
    if (outcome) {
      const identity = `${outcome.taskId}:${outcome.engineId}:${outcome.key}:${outcome.detail}`;
      if (!seenOutcomes.has(identity)) {
        seenOutcomes.add(identity);
        void message(t("AutoSetup.benchmarkTitle"), [tuningText(outcome.key), outcome.detail].filter(Boolean).join("\n"), outcome.success ? "info" : "warning");
      }
    }
    renderStartupNotice();
  }
  listeners.forEach((listener) => listener());
});

function renderStartupNotice(): void {
  const task = latest?.startupNotice;
  if (!task) { startupNotice?.remove(); startupNotice = null; return; }
  if (!startupNotice) { startupNotice = document.createElement("aside"); startupNotice.className = "engine-limit-notice tuning-startup-notice"; document.body.append(startupNotice); }
  startupNotice.replaceChildren(); startupNotice.setAttribute("aria-live", "polite");
  const heading = document.createElement("strong"); heading.textContent = t("AutoSetup.benchmarking");
  const description = document.createElement("p"); description.textContent = t("EngineThreadPolicy.automaticBenchmark");
  const profile = latest?.profiles.find((entry) => entry.engineId === task.engineId);
  const target = document.createElement("p"); target.textContent = [profile?.name, profile?.engine, profile?.config, profile?.model].filter(Boolean).join("\n");
  const progress = document.createElement("progress"); progress.max = 1000; progress.value = task.permille;
  const status = document.createElement("p"); status.textContent = task.statusLines.join("\n");
  startupNotice.append(heading, description, target, progress, status);
  const stop = button(t("AutoSetup.stopBenchmark"), () => { void run({ type: "Tuning", op: { type: "StopBenchmark" } }); }); stop.disabled = task.cancelling; stop.dataset.key = "tuning.startup.stop";
  const dismiss = button(t("AutoSetup.close"), () => { void run({ type: "Tuning", op: { type: "DismissStartupNotice", task_id: task.id } }); }); dismiss.dataset.key = "tuning.startup.dismiss";
  actions(startupNotice, stop, dismiss);
}

const text = (tag: string, value: string, className = ""): HTMLElement => { const node = document.createElement(tag); node.textContent = value; node.className = className; return node; };
function metric(title: string, description: string, technicalName: string) {
  const element = document.createElement("section"); element.className = "benchmark-metric";
  const value = text("strong", "—", "benchmark-value"); const unit = text("span", "", "benchmark-unit");
  const technical = text("div", "", "benchmark-technical");
  element.append(text("h3", title), value, unit, technical, text("p", description));
  return { element, update(speed: number) {
    const available = Number.isFinite(speed) && speed > 0;
    value.textContent = available ? speed.toLocaleString(undefined, { maximumFractionDigits: 0 }) : "—";
    unit.textContent = available ? t("AutoSetup.benchmarkPerSecond") : "";
    technical.textContent = available ? technicalName : t("AutoSetup.benchmarkMetricUnavailable");
    technical.classList.toggle("missing", !available);
  } };
}
export function performanceCard(ctx: SetupContext): SetupCard {
  if (!latest && state.view?.tuning) latest = state.view.tuning;
  const element = document.createElement("section"); element.className = "setup-performance";
  element.append(text("h2", t("AutoSetup.benchmarkTitle")), text("p", t("AutoSetup.benchmarkSubtitle"), "benchmark-subtitle"));
  const report = document.createElement("div"); report.className = "benchmark-report";
  const notice = document.createElement("div"); notice.className = "benchmark-model-notice";
  notice.append(text("strong", t("B11SpeedNotice.title")), text("p", t("B11SpeedNotice.description")));
  const status = text("div", "", "benchmark-status"); status.setAttribute("aria-live", "polite");
  const metrics = document.createElement("div"); metrics.className = "benchmark-report-body";
  const nn = metric(t("AutoSetup.benchmarkNnSpeed"), t("AutoSetup.benchmarkNnSpeedDescription"), "nnEvals/s");
  const visits = metric(t("AutoSetup.benchmarkSearchSpeed"), t("AutoSetup.benchmarkSearchSpeedDescription"), "visits/s");
  const metadata = document.createElement("dl"); metadata.className = "benchmark-metadata";
  const values = [t("MoreEngines.column1"), t("AutoSetup.localEngine"), t("AutoSetup.gtpConfig"), t("AutoSetup.benchmarkRecommended"), t("AutoSetup.benchmarkCurrentModel"), t("AutoSetup.benchmarkBackend"), t("AutoSetup.benchmarkTime")].map((label) => { const value = text("dd", "—"); metadata.append(text("dt", label), value); return value; });
  metrics.append(nn.element, visits.element, metadata);
  const details = text("p", "", "benchmark-policy"); report.append(notice, status, metrics, details);
  element.append(report, text("p", `ⓘ  ${t("AutoSetup.benchmarkSpeedExplanation")}`, "benchmark-explanation"), text("p", `ⓘ  ${t("AutoSetup.benchmarkComparisonNote")}`, "benchmark-comparison"));
  const taskArea = document.createElement("div"); taskArea.className = "benchmark-task";
  const progress = document.createElement("progress"); progress.max = 1000;
  const lines = text("pre", "");
  const stop = button(t("AutoSetup.stopBenchmark"), () => { void run({ type: "Tuning", op: { type: "StopBenchmark" } }); }); stop.dataset.key = "performance.stop";
  taskArea.append(progress, lines, stop); element.append(taskArea);
  let targetId = "";
  let pending = false;
  let disposed = false;
  const execute = async (action: () => Promise<unknown>) => { if (pending) return; pending = true; update(state.view); try { await action(); } finally { pending = false; update(state.view); } };
  const start = async (experimental: boolean) => {
    if (experimental && !(await confirm(t("AutoSetup.experimentalBenchmarkConfirmTitle"), plainJava(t("AutoSetup.experimentalBenchmarkConfirmMessage"))))) return;
    await execute(() => run({ type: "Tuning", op: { type: "StartBenchmark", engine_id: targetId, experimental } }));
  };
  const experimental = button(t("AutoSetup.experimentalPerformance"), () => { void start(true); }); experimental.dataset.key = "performance.experimental";
  const optimize = button(t("AutoSetup.optimizePerformance"), () => { void start(false); }); optimize.className = "primary"; optimize.dataset.key = "performance.optimize";
  const importReport = button(t("MeasuredTuning.import"), () => { void execute(async () => {
    const engineId = targetId;
    const result = await run({ type: "Tuning", op: { type: "ImportMeasured", engine_id: engineId, path: null } });
    if (disposed || result?.type !== "Tuning" || result.result.type !== "MeasuredReview") return;
    const review = result.result;
    // MeasuredTuningDialog.java:60-83: show the target, scene, parameter change and paired gain.
    // The core has validated the report. Derive only its display metric, never application authority.
    const runs: unknown[] = Array.isArray(review.report.runs) ? review.report.runs : [];
    const speeds: number[] = [];
    for (let index = 0; index + 1 < runs.length; index += 2) {
      const first = runs[index], second = runs[index + 1];
      if (typeof first !== "object" || first === null || typeof second !== "object" || second === null) continue;
      if (!("profile" in first) || !("seconds" in first) || !("seconds" in second)) continue;
      if (typeof first.seconds !== "number" || typeof second.seconds !== "number" || first.seconds <= 0 || second.seconds <= 0) continue;
      const speed = first.profile === "baseline" ? first.seconds / second.seconds : second.seconds / first.seconds;
      if (Number.isFinite(speed)) speeds.push(speed);
    }
    if (!speeds.length) { await message(t("MeasuredTuning.title"), t("MeasuredTuning.rejected"), "warning"); return; }
    speeds.sort((a, b) => a - b);
    const gain = ((speeds[Math.floor(speeds.length / 2)] - 1) * 100).toLocaleString(undefined, { minimumFractionDigits: 1, maximumFractionDigits: 1 });
    const profile = latest?.profiles.find((entry) => entry.engineId === review.engine_id);
    const identity = `${profile?.name ?? ""} [${review.engine_id}]`;
    const content = document.createElement("div"); content.className = "measured-review";
    const summary = text("div", [
      t("EngineThreadPolicy.target").replace("%s", identity),
      review.report.scene === "live" ? t("MeasuredTuning.live") : t("MeasuredTuning.wholeGame"),
      `${JSON.stringify(review.report.baselineParameters)} → ${JSON.stringify(review.report.candidateParameters)}`,
      t("MeasuredTuning.gain").replace("%.1f", gain).replace("%%", "%"),
      "", t("MeasuredTuning.confirm"),
    ].join("\n"), "measured-review-summary");
    summary.tabIndex = 0; summary.setAttribute("aria-label", t("MeasuredTuning.title")); content.append(summary);
    let accepted = false;
    const prompt = formPanel(t("MeasuredTuning.title"), content);
    const accept = button(product("ok"), () => { accepted = true; void prompt.close(); }); accept.dataset.key = "performance.measured.confirm";
    const cancel = button(product("cancel"), () => prompt.close()); cancel.dataset.key = "performance.measured.cancel";
    actions(content, accept, cancel);
    await prompt.closed;
    if (!accepted || disposed) return;
    const applied = await run({ type: "Tuning", op: { type: "ConfirmMeasured", engine_id: review.engine_id, review_id: review.review_id } });
    if (!disposed && applied?.type === "Tuning" && applied.result.type === "Done") await message(t("MeasuredTuning.title"), t("MeasuredTuning.saved"));
  }); }); importReport.dataset.key = "performance.import";
  const restore = button(t("MeasuredTuning.restore"), async () => {
    if (await confirm(t("MeasuredTuning.title"), t("MeasuredTuning.restoreConfirm"))) await execute(() => run({ type: "Tuning", op: { type: "RestoreMeasured", engine_id: targetId } }));
  }); restore.dataset.key = "performance.restore";
  const controls = document.createElement("div"); controls.className = "benchmark-actions";
  actions(controls, experimental, optimize); actions(controls, importReport, restore); element.append(controls);
  function update(view: WorkbenchView | null): void {
    if (view?.tuning) latest = view.tuning;
    const foreground = view?.engine.foreground;
    targetId = ctx.engineId ?? (foreground ? view?.settings.engine.engines[foreground.index]?.id : undefined) ?? "";
    const profile = latest?.profiles.find((entry) => entry.engineId === targetId);
    const policy = profile?.policy;
    const threads = policy?.["katago-benchmark-threads"] ?? 0;
    const nnSpeed = policy?.["katago-benchmark-nn-evals-per-second"] ?? 0;
    const visitsSpeed = policy?.["katago-benchmark-visits-per-second"] ?? 0;
    nn.update(nnSpeed); visits.update(visitsSpeed);
    const measuredAt = policy?.["katago-benchmark-updated-at"] ?? 0;
    const entries = [profile?.name ?? t("EngineThreadPolicy.selectTarget"), profile?.engine, profile?.config, threads > 0 ? String(threads) : "—", profile?.model || t("AutoSetup.notFound"), policy?.["katago-benchmark-backend"] || t("AutoSetup.benchmarkCurrentBackend"), measuredAt > 0 ? new Date(measuredAt).toLocaleString() : "—"];
    entries.forEach((entry, index) => { values[index].textContent = entry || "—"; values[index].title = entry || ""; });
    notice.hidden = !/b11/i.test(profile?.model ?? "");
    const task = latest?.task;
    status.textContent = task?.engineId === targetId ? t("AutoSetup.benchmarking") : !profile?.eligible ? t("AutoSetup.benchmarkUnavailableShort") : profile.environmentStatus ? t("B11SpeedNotice.historicalStatus") : threads <= 0 ? t("AutoSetup.benchmarkNotRun") : nnSpeed > 0 && visitsSpeed > 0 ? t("AutoSetup.benchmarkDone") : t("AutoSetup.benchmarkLegacyResult");
    const command = view?.settings.engine.engines.find((entry) => entry.id === targetId)?.command ?? "";
    details.textContent = [policy?.source === "BENCHMARK" ? t("EngineThreadPolicy.benchmark") : t("EngineThreadPolicy.cfg"), profile?.environmentStatus ? tuningText(profile.environmentStatus) : "", profile?.environmentStatus && threads > 0 ? t("B11SpeedNotice.historicalResult") : "", /(?:numSearchThreads\s*=\s*[1-9]|--?threads\s+[1-9])/i.test(command) ? t("EngineThreadPolicy.explicitOverride") : "", !profile?.eligible ? t("AutoSetup.benchmarkUnavailable") : ""].filter(Boolean).join("\n");
    taskArea.hidden = !task;
    progress.value = task?.permille ?? 0; lines.textContent = task?.statusLines.join("\n") ?? ""; stop.disabled = task?.cancelling ?? true;
    const busy = pending || !!task;
    optimize.disabled = !profile?.eligible || !profile.engine || !profile.config || !profile.model || busy;
    optimize.textContent = threads > 0 ? t("AutoSetup.optimizePerformanceAgain") : t("AutoSetup.optimizePerformance");
    experimental.hidden = !latest?.appleSilicon; experimental.disabled = optimize.disabled;
    importReport.disabled = !profile || busy;
    const scenes = policy?.["katago-measured-scenes-v1"];
    restore.disabled = !profile || busy || !scenes || (typeof scenes === "object" && Object.keys(scenes).length === 0);
  }
  const unsubscribe = onTuningChange(() => update(state.view));
  update(state.view);
  void run({ type: "Tuning", op: { type: "Refresh" } });
  return { element, update, dispose: () => { disposed = true; unsubscribe(); } };
}
