// KataGoAutoSetupDialog: a single non-modal setup window; Rust owns acquisition and profiles.
import { registerViewHandler, run, state } from "../app";
import type { Asset, ExperimentalBackend, InstalledPackage, ResourceCommand, ResourceEngineChoice, ResourcePlanView, ResourceResult, ResourcesView, TuningView } from "../contract/generated";
import { ask, message } from "../dialogs";
import { product, t } from "../i18n";
import { tuningText } from "../engine/tuningText";
import type { SetupCard, SetupCardKey, SetupContext } from "./card";
import { overviewCard } from "./overview";
import { weightsCard } from "./weights";
import { accelerationCard } from "./acceleration";
import { performanceCard } from "./performance";
import { action, actions, javaFormat, node, resourceText, row, size } from "./setupText";
import "./setup.css";

let resources: ResourcesView | null = null;
let tuning: TuningView | null = null;
let refreshWindow: (() => void) | null = null;
let windowInstance: { focus(card: SetupCardKey, ctx: SetupContext): void } | null = null;
let footerStatus = "";
const seenOutcomes = new Set<string>();
export function resourceView(): ResourcesView | null { return resources ?? state.view?.resources ?? null; }
export function setupBusy(): boolean { return !!resourceView()?.task || !!tuning?.task || !!resourceView()?.detecting; }
export function setupStatus(text: string): void { footerStatus = text; refreshWindow?.(); }
/** Rust task phases; the downloaded item is a catalog/asset identifier and stays as is. */
function phaseText(phase: string): string {
  switch (phase) {
    case "detecting": return product("setupPhaseDetecting");
    case "downloading": return product("setupPhaseDownloading");
    case "verifying": return product("setupPhaseVerifying");
    case "installing": return product("setupPhaseInstalling");
    case "cancelling": return t("AutoSetup.cancelling");
    default: return phase;
  }
}

registerViewHandler((view) => {
  if (view.resources) resources = view.resources;
  if (view.tuning) tuning = view.tuning;
  const outcome = view.resources?.lastOutcome;
  if (outcome) {
    const id = `resource:${outcome.task}:${outcome.status}`;
    if (!seenOutcomes.has(id)) {
      seenOutcomes.add(id);
      const text = resourceText(outcome.messageKey);
      footerStatus = text.includes("%s") ? javaFormat(text, outcome.detail) : `${text}${outcome.detail ? `\n${outcome.detail}` : ""}`;
      if (outcome.status === "failed") void message(t("AutoSetup.failed"), footerStatus, "error");
    }
  }
  const benchmark = view.tuning?.outcome;
  if (benchmark) {
    const id = `tuning:${benchmark.taskId}:${benchmark.key}`;
    if (!seenOutcomes.has(id)) { seenOutcomes.add(id); footerStatus = `${tuningText(benchmark.key)}${benchmark.detail ? `\n${benchmark.detail}` : ""}`; }
  }
  refreshWindow?.();
});

/** Also handles immediate refusals and profile previews; background completions arrive in the view. */
export async function resourceCommand(op: ResourceCommand): Promise<ResourceResult | null> {
  const result = await run({ type: "Resource", op });
  if (!result) { setupStatus(t("AutoSetup.failed")); return null; }
  if (result.type === "Refused") { setupStatus(resourceText(result.message_key)); return null; }
  if (result.type !== "Resource") return null;
  const r = result.result;
  if (r.type === "Refused") {
    setupStatus(resourceText(r.message_key));
    await message(t("AutoSetup.failed"), resourceText(r.message_key), "warning");
  } else if (r.type === "Plan") await confirmSetup(r.plan);
  else if (r.type === "Verified") {
    const keys = { ACTIVE: "AutoSetup.validationRunning", VALID: "AutoSetup.validationPassed", TIMED_OUT: "AutoSetup.validationTimedOut", WRONG_ARCHITECTURE: "AutoSetup.validationWrongArchitecture", MISSING_DEPENDENCY: "AutoSetup.validationMissingDependency", START_FAILED: "AutoSetup.validationStartFailed" };
    setupStatus(`${resourceText(keys[r.validation.status])}${r.validation.detail ? `\n${r.validation.detail}` : ""}`);
    if (r.validation.status !== "VALID" && r.validation.status !== "ACTIVE") await message(t("AutoSetup.engineValidation"), footerStatus, "error");
  }
  if (r.type === "Done" && op.type === "ApplySetup") setupStatus(t("AutoSetup.setupDone"));
  return r;
}

/** Installed and current: the receipt names this catalog asset, carries its archive digest and is usable.
 *  A same-id package with another digest is a previous package and is never used for this asset. */
export function currentPackage(r: ResourcesView, assetId: string, backend?: ExperimentalBackend): InstalledPackage | undefined {
  const asset = r.packages.find((a) => a.id === assetId); if (!asset) return undefined;
  const digest = asset.sha256.toLowerCase();
  return r.inventory.packages.find((p) => p.receipt.assetId === asset.id && p.receipt.archiveSha256.toLowerCase() === digest && p.usable && (backend === undefined || p.receipt.experimentalBackend === backend));
}
const experimentalBackends: readonly ExperimentalBackend[] = ["direct-ml", "open-vino-gpu", "open-vino-npu", "rocm-gfx103x", "rocm-gfx110x", "rocm-gfx1151", "rocm-gfx120x"];
/** Experimental backend variants an asset provides, derived from its backend and ROCm GPU family. */
export function assetBackends(asset: Asset): ExperimentalBackend[] {
  const names: string[] = asset.backend === "directml" ? ["direct-ml"] : asset.backend === "openvino" ? ["open-vino-gpu", "open-vino-npu"] : asset.backend === "rocm" && asset.gpuFamily ? [`rocm-${asset.gpuFamily.toLowerCase()}`] : [];
  return experimentalBackends.filter((b) => names.includes(b));
}
export function packageTarget(installed: InstalledPackage): ResourceEngineChoice | null {
  const r = resourceView(); const config = r?.inventory.gtpConfig;
  if (!config) return null;
  return { assetId: installed.receipt.assetId, executable: installed.executable, config, backend: installed.receipt.experimentalBackend, updateProfileId: null };
}
export function setupVerified(target: ResourceEngineChoice | null): boolean {
  const r = resourceView(); if (!r || !target) return false;
  const matches = r.verificationExecutable === target.executable;
  if (matches && r.cudaProbe?.passed === false) return false;
  const foreground = state.view?.engine.foreground;
  if (r.currentSetup.executable === target.executable && r.currentSetup.discoverySource === "foreground" && foreground?.ready && foreground.katago) return true;
  return matches && (r.verification?.status === "VALID" || r.verification?.status === "ACTIVE");
}
export function currentTarget(): ResourceEngineChoice | null {
  const r = resourceView(); if (!r) return null;
  const s = r.currentSetup;
  if (!s.executable || !s.config) return r.chosenEngine ? { ...r.chosenEngine, updateProfileId: null } : null;
  const installed = r.inventory.packages.find((p) => p.executable === s.executable);
  return { assetId: installed?.receipt.assetId ?? "local", executable: s.executable, config: s.config, backend: installed?.receipt.experimentalBackend ?? null, updateProfileId: null };
}

/** The user chooses a managed target (or addition) before Rust captures the exact proposed change. */
export async function planSetup(target: ResourceEngineChoice, weight?: string): Promise<void> {
  const r = resourceView(); if (!r || setupBusy()) return;
  const body = node("div", "", "setup-plan"); body.append(node("p", product("setupSelect")));
  const weights = node("select"); weights.dataset.key = "setup.plan.weight";
  const candidates = new Set(r.inventory.weights.map((w) => w.path));
  if (r.currentSetup.weight) candidates.add(r.currentSetup.weight);
  if (r.chosenWeight) candidates.add(r.chosenWeight);
  if (weight) candidates.add(weight);
  for (const path of candidates) { const o = node("option", path); o.value = path; weights.append(o); }
  if (!candidates.size) { await message(t("AutoSetup.title"), t("AutoSetup.needWeight"), "warning"); return; }
  weights.value = weight ?? r.chosenWeight ?? r.currentSetup.weight ?? [...candidates][0];
  const profile = node("select"); profile.dataset.key = "setup.plan.profile";
  const add = node("option", product("setupNewProfile")); add.value = ""; profile.append(add);
  for (const p of state.view?.settings.engine.engines ?? []) {
    if (!p.managed || p.managed.command !== p.command) continue;
    const option = node("option", `${product("setupUpdateProfile")}: ${p.name}`); option.value = p.id; profile.append(option);
  }
  const weightLabel = node("label", t("AutoSetup.localWeight")); weightLabel.append(weights);
  const profileLabel = node("label", product("setupTargetProfile")); profileLabel.append(profile);
  body.append(weightLabel, profileLabel);
  if (await ask(product("setupPlanTitle"), body, t("AutoSetup.autoSetup"), product("cancel"), true)) {
    await resourceCommand({ type: "PlanSetup", engine: { ...target, updateProfileId: profile.value || null }, weight: weights.value });
  }
}
async function confirmSetup(plan: ResourcePlanView): Promise<void> {
  const body = node("div", "", "setup-plan");
  body.append(node("strong", product(plan.replacesProfileId ? "setupUpdateProfile" : "setupNewProfile")));
  row(body, product("setupName"), plan.name); row(body, product("setupCommand"), plan.command);
  const makeDefault = node("input"); makeDefault.type = "checkbox"; makeDefault.checked = plan.isDefault; makeDefault.dataset.key = "setup.plan.default";
  const switchNow = node("input"); switchNow.type = "checkbox"; switchNow.checked = false; switchNow.dataset.key = "setup.plan.switch";
  const defaultLabel = node("label"); defaultLabel.append(makeDefault, product("setupMakeDefault"));
  const switchLabel = node("label"); switchLabel.append(switchNow, product("setupSwitchNow")); body.append(defaultLabel, switchLabel);
  if (await ask(product("setupPlanTitle"), body, t("AutoSetup.autoSetup"), product("cancel"), true)) {
    await resourceCommand({ type: "ApplySetup", plan: plan.id, make_default: makeDefault.checked, switch_now: switchNow.checked });
  }
}
export async function downloadPackage(asset: Asset, backend: ResourceEngineChoice["backend"] = null): Promise<void> {
  if (setupBusy()) return;
  const text = asset.releaseTier === "experimental"
    ? t("AutoSetup.experimentalBackendConfirm").replace("%s", asset.id).replace("%s", size(asset.sizeBytes))
    : product("setupDownloadConfirm", asset.id, size(asset.sizeBytes));
  const body = node("p", text);
  if (await ask(t("AutoSetup.title"), body, t("AutoSetup.downloadShort"), product("cancel"), true)) {
    await resourceCommand({ type: "Download", item: { type: "Package", asset: asset.id, confirmed: true, backend } });
  }
}

const icons: Record<SetupCardKey | "remote", string> = {
  overview: '<rect x="3" y="3" width="7" height="7"/><rect x="14" y="3" width="7" height="7"/><rect x="3" y="14" width="7" height="7"/><rect x="14" y="14" width="7" height="7"/>',
  weights: '<path d="M5 6h14a2 2 0 0 1 2 2v11H3V8a2 2 0 0 1 2-2Zm7-3v12m-5-5 5 5 5-5" fill="none" stroke="currentColor" stroke-width="2"/>',
  performance: '<g fill="none" stroke="currentColor" stroke-width="2"><ellipse cx="12" cy="9" rx="5" ry="7"/><path d="m9 15-5 7 6-2 2 3 3-8m-5-7 2 3 2-5"/></g>',
  acceleration: '<g fill="none" stroke="currentColor" stroke-width="2"><rect x="5" y="5" width="14" height="14"/><rect x="9" y="9" width="6" height="6"/><path d="M8 1v4m8-4v4M8 19v4m8-4v4M1 8h4m-4 8h4m14-8h4m-4 8h4"/></g>',
  remote: '<g fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9"/><ellipse cx="12" cy="12" rx="4" ry="9"/><path d="M3 12h18"/></g>',
};
export async function openAutoSetup(card: SetupCardKey = "overview", ctx: SetupContext = {}): Promise<void> {
  if (windowInstance) { windowInstance.focus(card, ctx); return; }
  const previous = document.activeElement as HTMLElement | null;
  const frame = node("section", "", "auto-setup-window"); frame.dataset.key = "auto-setup"; frame.tabIndex = -1;
  frame.setAttribute("role", "dialog"); frame.setAttribute("aria-modal", "false"); frame.setAttribute("aria-label", t("AutoSetup.title"));
  const heading = node("header", "", "setup-title"); heading.append(node("span", "", "setup-stones"), node("h2", t("AutoSetup.title")));
  const layout = node("div", "", "setup-layout"), nav = node("nav", "", "setup-nav"), content = node("fieldset", "", "setup-content");
  nav.setAttribute("aria-label", t("AutoSetup.sidebarTitle"));
  const navButtons = new Map<SetupCardKey, HTMLButtonElement>();
  let active = card, context = ctx;
  const cards = new Map<SetupCardKey, SetupCard>();
  const cardContexts = new Map<SetupCardKey, SetupContext>();
  const factory = (key: SetupCardKey): SetupCard => key === "overview" ? overviewCard(context) : key === "weights" ? weightsCard(context) : key === "acceleration" ? accelerationCard(context) : performanceCard(context);
  const show = (key: SetupCardKey, nextContext = context) => {
    if (cards.has(key) && (key === "performance" || key === "acceleration") && nextContext.engineId !== cardContexts.get(key)?.engineId) { cards.get(key)?.dispose?.(); cards.delete(key); }
    context = nextContext; active = key;
    let item = cards.get(key); if (!item) { item = factory(key); cards.set(key, item); cardContexts.set(key, { ...context }); }
    content.replaceChildren(item.element);
    navButtons.forEach((button, id) => { button.classList.toggle("selected", id === key); button.setAttribute("aria-current", String(id === key)); });
    update();
  };
  for (const [key, title] of [["overview", t("AutoSetup.navOverview")], ["weights", t("AutoSetup.navWeights")], ["performance", t("AutoSetup.navBenchmark")], ["acceleration", t("AutoSetup.navAcceleration")]] as const) {
    const button = action(`setup.nav.${key}`, title, () => show(key));
    const icon = node("span", "", "setup-nav-icon"); icon.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${icons[key]}</svg>`; button.prepend(icon); navButtons.set(key, button); nav.append(button);
  }
  const remote = action("setup.nav.remote", t("Menu.remoteCompute"), () => {}, false); remote.className = "unavailable"; remote.title = product("unavailable");
  const globe = node("span", "", "setup-nav-icon"); globe.innerHTML = `<svg viewBox="0 0 24 24" aria-hidden="true">${icons.remote}</svg>`; remote.prepend(globe); nav.append(remote);
  layout.append(nav, content);
  const footer = node("footer", "", "setup-footer");
  const progressTitle = node("strong", t("AutoSetup.progressTitle")), progress = node("progress"), progressStatus = node("span", "", "setup-progress-status"), status = node("span", "", "setup-footer-status"); status.setAttribute("role", "status"); status.setAttribute("aria-live", "polite");
  const progressBox = node("div", "", "setup-progress"); progressBox.append(progressTitle, progressStatus, progress);
  const stop = action("setup.stop", t("AutoSetup.stopDownload"), async () => {
    const task = resourceView()?.task;
    if (task?.cancellable) await resourceCommand({ type: "CancelTask", task: task.id });
    else if (tuning?.task) await run({ type: "Tuning", op: { type: "StopBenchmark" } });
  });
  const close = () => { for (const c of cards.values()) c.dispose?.(); frame.remove(); refreshWindow = null; windowInstance = null; previous?.focus(); };
  const closeButton = action("setup.close", t("AutoSetup.close"), close);
  const footerRow = node("div", "", "setup-footer-row"); footerRow.append(status); actions(footerRow, stop, closeButton); footer.append(progressBox, footerRow);
  frame.append(heading, layout, footer);
  function update() {
    const view = state.view; if (view) cards.get(active)?.update(view);
    const busy = setupBusy(); navButtons.forEach((b) => { b.disabled = busy; });
    // A disabled fieldset preserves each card's own prerequisite/future-ticket disabled states.
    content.disabled = busy;
    const task = resourceView()?.task, benchmark = tuning?.task;
    stop.textContent = benchmark ? t("AutoSetup.stopBenchmark") : t("AutoSetup.stopDownload");
    stop.disabled = task ? !task.cancellable : !benchmark || benchmark.cancelling;
    progressBox.hidden = !task && !benchmark;
    progressStatus.textContent = task ? [task.kind === "download" ? task.item : "", phaseText(task.phase), size(task.bytes) + (task.total ? ` / ${size(task.total)}` : "")].filter(Boolean).join(" · ") : benchmark?.statusLines.join("\n") ?? "";
    if (task?.total) { progress.max = Number(task.total); progress.value = Number(task.bytes); }
    else if (benchmark) { progress.max = 1000; progress.value = benchmark.permille; }
    else progress.removeAttribute("value");
    status.textContent = footerStatus;
  }
  frame.addEventListener("keydown", (event) => { event.stopPropagation(); if (event.key === "Escape") { event.preventDefault(); close(); } });
  frame.addEventListener("pointerdown", (event) => event.stopPropagation());
  refreshWindow = update;
  windowInstance = { focus(key, nextContext) { frame.focus(); if (!setupBusy()) show(key, nextContext); } };
  document.body.append(frame); show(card); frame.focus();
  await resourceCommand({ type: "Inspect", refresh: false });
}
