// Java KataGoAutoSetupDialog.createAccelerationSection + KataGoAccelerationLayout.
import { state } from "../app";
import type { ExperimentalBackend, ResourceCommand, ResourceOutcome, ResourcesView } from "../contract/generated";
import { confirm } from "../dialogs";
import { product, t } from "../i18n";
import type { SetupCard, SetupContext } from "./card";
import { packageTarget, planSetup, resourceCommand, resourceView, setupStatus } from "./dialog";
import { action, actions, javaFormat, node, row, section, size } from "./setupText";

export function accelerationCard(ctx: SetupContext): SetupCard {
  const element = node("div", "", "setup-acceleration");
  let last: ResourcesView | null = null, selected: ExperimentalBackend = "direct-ml", disposed = false;
  let pending: { task: ResourceOutcome["task"]; next: () => Promise<unknown> } | null = null;
  const complete = () => {
    const outcome = resourceView()?.lastOutcome;
    if (!pending || !outcome || outcome.task !== pending.task) return;
    const next = pending.next; pending = null;
    if (outcome.status === "done" && !disposed) queueMicrotask(() => { if (!disposed) void next(); });
  };
  const afterTask = async (op: ResourceCommand, next: () => Promise<unknown>) => {
    const result = await resourceCommand(op);
    if (disposed) return;
    if (result?.type === "Started") { pending = { task: result.task, next }; complete(); }
    else if (result?.type === "Verified" && (result.validation.status === "ACTIVE" || result.validation.status === "VALID")) await next();
  };
  const activate = async (asset: string, command: "EnableTensorRt" | "SwitchBackToCuda") => {
    const r = resourceView(), installed = r?.inventory.packages.find((p) => p.receipt.assetId === asset && p.usable);
    const target = installed && packageTarget(installed); if (!target || !r?.currentSetup.weight) return;
    await afterTask({ type: "Verify", target, weight: r.currentSetup.weight }, () => resourceCommand({ type: command }));
  };
  const render = () => {
    const r = resourceView(); if (!r) return; last = r; element.replaceChildren();
    const card = section(t("AutoSetup.accelerationTitle"), t("AutoSetup.accelerationSubtitle"));
    const windows = r.machine?.os === "windows", cuda = r.acceleration.find((a) => a.backend === "cuda"), trt = r.acceleration.find((a) => a.backend === "tensorrt");
    const companion = r.inventory.packages.find((p) => p.receipt.assetId === "windows-nvidia" && p.usable), engine = r.inventory.packages.find((p) => p.receipt.assetId === "windows-tensorrt" && p.usable);
    const notApplicable = t("AutoSetup.tensorRtNotApplicable");
    if (ctx.engineId) {
      const target = state.view?.settings.engine.engines.find((p) => p.id === ctx.engineId);
      const directed = node("p", target ? javaFormat(t("AutoSetup.tensorRtDirectedTarget"), target.name) : t("AutoSetup.tensorRtTargetStale"), "setup-chip"); directed.setAttribute("aria-label", t("AutoSetup.tensorRtDirectedTargetAccessibleName")); directed.title = t("AutoSetup.tensorRtDirectedTargetAccessibleDescription"); card.append(directed);
    }
    row(card, t("AutoSetup.nvidiaRuntime"), !windows ? t("AutoSetup.nvidiaRuntimeNotApplicable") : cuda?.missing.length ? `${t("AutoSetup.nvidiaRuntimeMissing")} ${cuda.missing.join(", ")}` : cuda ? t("AutoSetup.nvidiaRuntimeReady") : product("setupUnknown"), true);
    const gpu = row(card, t("AutoSetup.nvidiaGpu"), r.detecting ? t("AutoSetup.gpuDetecting") : r.machine?.nvidiaGpus.length ? r.machine.nvidiaGpus.map((g) => `${g.name} · ${g.driver ?? product("setupUnknown")} · ${g.memoryMib == null ? product("setupUnknown") : size(Number(g.memoryMib) * 1024 ** 2)}${g.compute ? ` · CC ${g.compute.major}.${g.compute.minor}` : ""}`).join("\n") : t("AutoSetup.gpuDetectNotFound"), true);
    gpu.setAttribute("aria-label", t("AutoSetup.tensorRtGpuAdviceAccessibleName")); gpu.title = t("AutoSetup.tensorRtGpuAdviceAccessibleDescription");
    const runtime = row(card, t("AutoSetup.tensorRtRuntimeStatus"), !windows ? notApplicable : trt?.missing.length ? `${t("AutoSetup.tensorRtRuntimeNotReady")} · ${trt.missing.join(", ")}` : trt ? t("AutoSetup.tensorRtRuntimeReady") : product("setupUnknown"), true);
    runtime.setAttribute("aria-label", t("AutoSetup.tensorRtRuntimeAccessibleName")); runtime.title = t("AutoSetup.tensorRtRuntimeAccessibleDescription");
    row(card, t("AutoSetup.tensorRtCompanionStatus"), !windows ? notApplicable : companion ? t("AutoSetup.tensorRtCompanionReady") : t("AutoSetup.tensorRtCompanionMissing"), true);
    row(card, t("AutoSetup.tensorRtEngineStatus"), !windows ? notApplicable : engine ? t("AutoSetup.tensorRtEngineReady") : t("AutoSetup.tensorRtEngineMissing"), true);
    const missingActivation: string[] = [];
    if (!r.machine?.nvidiaGpus.length) missingActivation.push(t("AutoSetup.tensorRtMissingNvidiaGpu"));
    if (!trt || trt.missing.length) missingActivation.push(t("AutoSetup.tensorRtMissingRuntime"));
    if (!companion) missingActivation.push(t("AutoSetup.tensorRtMissingCompanion"));
    if (!engine) missingActivation.push(t("AutoSetup.tensorRtMissingEngine"));
    if (!r.currentSetup.weight) missingActivation.push(t("AutoSetup.tensorRtMissingWeight"));
    if (!r.inventory.gtpConfig) missingActivation.push(t("AutoSetup.tensorRtMissingGtpConfig"));
    const ready = !!windows && !missingActivation.length;
    const active = !!engine && r.currentSetup.executable === engine.executable;
    row(card, t("AutoSetup.tensorRtActivationStatus"), !windows ? notApplicable : active ? t("AutoSetup.tensorRtProfileActive") : ready ? t("AutoSetup.tensorRtProfileInactive") : `${t("AutoSetup.tensorRtProfileNotReady")} · ${missingActivation.join(", ")}`, true);
    const backendAssets: Record<ExperimentalBackend, string> = { "direct-ml": "windows-directml", "open-vino-gpu": "windows-openvino", "open-vino-npu": "windows-openvino", "rocm-gfx103x": "windows-rocm-gfx103x", "rocm-gfx110x": "windows-rocm-gfx110x", "rocm-gfx1151": "windows-rocm-gfx1151", "rocm-gfx120x": "windows-rocm-gfx120x" };
    const labels: Record<ExperimentalBackend, string> = { "direct-ml": t("AutoSetup.experimentalDirectMl"), "open-vino-gpu": t("AutoSetup.experimentalOpenVinoGpu"), "open-vino-npu": t("AutoSetup.experimentalOpenVinoNpu"), "rocm-gfx103x": t("AutoSetup.experimentalRocmGfx103x"), "rocm-gfx110x": t("AutoSetup.experimentalRocmGfx110x"), "rocm-gfx1151": t("AutoSetup.experimentalRocmGfx1151"), "rocm-gfx120x": t("AutoSetup.experimentalRocmGfx120x") };
    const experimentalPackage = r.inventory.packages.find((p) => p.receipt.assetId === backendAssets[selected] && p.receipt.experimentalBackend === selected && p.usable);
    row(card, t("AutoSetup.experimentalBackendStatus"), `${labels[selected]} · ${experimentalPackage ? r.currentSetup.executable === experimentalPackage.executable ? t("AutoSetup.experimentalBackendActive") : t("AutoSetup.experimentalBackendInstalled") : t("AutoSetup.experimentalBackendNotInstalled")}`, true);
    const repair = action("setup.acceleration.repair", t("AutoSetup.repairTensorRt"), async () => {
      if (await confirm(t("AutoSetup.tensorRtRepairConfirmTitle"), javaFormat(t("AutoSetup.tensorRtRepairConfirmMessage"), "3.7 GB", t("AutoSetup.tensorRtGpuHint")))) await resourceCommand({ type: "RepairTensorRt", confirmed: true });
    }, !!windows); repair.title = t("AutoSetup.repairTensorRtTooltip"); repair.setAttribute("aria-label", t("AutoSetup.repairTensorRtAccessibleName")); repair.setAttribute("aria-description", t("AutoSetup.repairTensorRtAccessibleDescription"));
    const enable = action("setup.acceleration.enable", t("AutoSetup.enableTensorRt"), () => activate("windows-tensorrt", "EnableTensorRt"), ready && !active); enable.title = t("AutoSetup.enableTensorRtTooltip"); enable.setAttribute("aria-label", t("AutoSetup.enableTensorRtAccessibleName")); enable.setAttribute("aria-description", t("AutoSetup.enableTensorRtAccessibleDescription"));
    card.append(node("h4", t("AutoSetup.installTensorRt")), node("p", t("AutoSetup.accelerationTensorRtHint"), "setup-hint")); actions(card, repair, enable);
    card.append(node("h4", t("AutoSetup.experimentalBackends")), node("p", t("AutoSetup.experimentalBackendHint"), "setup-hint"));
    const selector = node("select"); selector.dataset.key = "setup.acceleration.backend";
    for (const backend of Object.keys(labels) as ExperimentalBackend[]) { const option = node("option", labels[backend]); option.value = backend; selector.append(option); }
    selector.value = selected; selector.onchange = () => { const backend = (Object.keys(labels) as ExperimentalBackend[]).find((b) => b === selector.value); if (backend) selected = backend; render(); };
    card.append(selector);
    actions(card, action("setup.acceleration.installExperimental", experimentalPackage ? t("AutoSetup.enableExperimentalBackend") : t("AutoSetup.installExperimentalBackend"), async () => {
      const asset = r.packages.find((a) => a.id === backendAssets[selected]); if (!asset) return;
      const backend = selected;
      const use = async () => {
        const current = resourceView(), installed = current?.inventory.packages.find((p) => p.receipt.assetId === asset.id && p.receipt.experimentalBackend === backend && p.usable);
        const target = installed && packageTarget(installed); if (!target) return;
        await afterTask({ type: "Verify", target, weight: current?.currentSetup.weight ?? current?.chosenWeight ?? current?.inventory.weights[0]?.path ?? null }, () => planSetup(target));
      };
      if (experimentalPackage) await use();
      else if (await confirm(t("AutoSetup.experimentalBackends"), javaFormat(t("AutoSetup.experimentalBackendConfirm"), labels[backend], size(asset.sizeBytes)))) await afterTask({ type: "Download", item: { type: "Package", asset: asset.id, backend, confirmed: true } }, use);
    }, !!windows && !!r.inventory.gtpConfig && !!(r.currentSetup.weight ?? r.chosenWeight ?? r.inventory.weights[0]?.path) && (!experimentalPackage || r.currentSetup.executable !== experimentalPackage.executable)));
    if (!windows) card.append(node("p", t("AutoSetup.experimentalBackendWindowsOnly"), "setup-hint"));
    card.append(node("h4", t("AutoSetup.maintenanceActions")));
    actions(card,
      action("setup.acceleration.checkNvidia", t("AutoSetup.installNvidiaRuntime"), async () => { if (await confirm(t("AutoSetup.installNvidiaRuntime"), product("setupNvidiaConfirm"))) await resourceCommand({ type: "Download", item: { type: "NvidiaRuntime", backend: "cuda", confirmed: true } }); }, !!windows && !!cuda?.missing.length),
      action("setup.acceleration.cuda", t("AutoSetup.switchBackCuda"), () => activate("windows-nvidia", "SwitchBackToCuda"), !!windows && !!companion && !!r.currentSetup.weight && !!r.inventory.gtpConfig && r.currentSetup.executable !== companion.executable),
      action("setup.acceleration.cleanCache", t("AutoSetup.cleanTensorRtCache"), async () => {
        const result = await resourceCommand({ type: "CleanNvidiaCache", confirmed: false }); if (result?.type !== "CacheSize") return;
        if (!Number(result.bytes)) { setupStatus(t("AutoSetup.cleanTensorRtCacheEmpty")); return; }
        if (await confirm(t("AutoSetup.cleanTensorRtCache"), javaFormat(t("AutoSetup.cleanTensorRtCacheConfirm"), size(result.bytes)))) await resourceCommand({ type: "CleanNvidiaCache", confirmed: true });
      }, !!windows && Number(trt?.cacheBytes ?? 0) > 0));
    element.append(card);
  };
  return { element, update() { complete(); if (resourceView() !== last) render(); }, dispose() { disposed = true; pending = null; } };
}
