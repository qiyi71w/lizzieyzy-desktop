// Java KataGoAutoSetupDialog.createOverviewSection, with the approved separate-delivery inventory.
import { invoke } from "@tauri-apps/api/core";
import type { Asset, CapabilityStatus, ExperimentalBackend, MissingDependency, RecommendationReason, ResourcesView, RuntimeRequirement } from "../contract/generated";
import { ask, confirm, message } from "../dialogs";
import { product, t } from "../i18n";
import type { SetupCard, SetupContext } from "./card";
import { assetBackends, currentPackage, downloadPackage, packageTarget, planSetup, resourceCommand, resourceView, setupStatus, setupVerified } from "./dialog";
import { action, actions, node, notice, resourceText, row, section, size } from "./setupText";

export function overviewCard(_ctx: SetupContext): SetupCard {
  const element = node("div", "", "setup-overview");
  let last: ResourcesView | null = null, lastEngine = "", detailsOpen = false;
  return { element, update(view) {
    const r = resourceView(), foreground = view.engine.foreground;
    const engineState = `${foreground?.index}:${foreground?.ready}:${foreground?.version}`;
    if (!r || r === last && engineState === lastEngine) return; last = r; lastEngine = engineState;
    element.replaceChildren(notice());
    const summary = section(t("AutoSetup.overviewTitle"), t("AutoSetup.overviewSubtitle")), s = r.currentSetup;
    row(summary, t("AutoSetup.localWeightModel"), s.modelName ?? t("AutoSetup.notFound"));
    const activePackage = r.inventory.packages.find((p) => p.executable === s.executable);
    const activeAsset = r.packages.find((a) => a.id === activePackage?.receipt.assetId);
    const packageNames: Record<string, string> = { cuda: t("AutoSetup.packageNvidia"), tensorrt: t("AutoSetup.packageTensorRt"), opencl: t("AutoSetup.packageOpenCl"), eigen: t("AutoSetup.packageCpu") };
    row(summary, t("AutoSetup.packageType"), activeAsset ? packageNames[activeAsset.backend] ?? t("AutoSetup.packageWithKataGo") : s.executable ? t("AutoSetup.packageExternal") : t("AutoSetup.packageWithoutEngine"));
    const keys = { ACTIVE: "AutoSetup.validationRunning", VALID: "AutoSetup.validationPassed", TIMED_OUT: "AutoSetup.validationTimedOut", WRONG_ARCHITECTURE: "AutoSetup.validationWrongArchitecture", MISSING_DEPENDENCY: "AutoSetup.validationMissingDependency", START_FAILED: "AutoSetup.validationStartFailed" };
    const checkedExecutable = s.executable ?? r.chosenEngine?.executable;
    const checked = r.verificationExecutable === checkedExecutable ? r.verification : null;
    const validation = row(summary, t("AutoSetup.engineValidation"), foreground?.ready && s.discoverySource === "foreground" ? t("AutoSetup.validationRunning") : checked ? resourceText(keys[checked.status]) : t("AutoSetup.validationNotAvailable"));
    if (checked) validation.title = checked.detail;
    const details = node("div", "", "setup-technical"); details.hidden = !detailsOpen;
    const sourceNames: Record<string, string> = { foreground: t("AutoSetup.sourceCurrentEngine"), default: t("AutoSetup.sourceDefaultEngine"), local: t("AutoSetup.sourceManualSelection"), downloaded: t("AutoSetup.sourceRememberedSetup"), imported: t("AutoSetup.sourceManualSelection") };
    const paths = [[t("AutoSetup.localEngine"), s.executable], [t("AutoSetup.localWeight"), s.weight], [t("AutoSetup.gtpConfig"), s.config], [t("AutoSetup.analysisConfig"), s.analysisConfig], [t("AutoSetup.discoverySource"), sourceNames[s.discoverySource] ?? t("AutoSetup.sourceNone")]];
    for (const [label, value] of paths) row(details, label ?? "", value ?? t("AutoSetup.notFound"));
    const disclose = action("setup.technical", t("AutoSetup.technicalDetails"), () => { detailsOpen = !detailsOpen; details.hidden = !detailsOpen; disclose.setAttribute("aria-expanded", String(detailsOpen)); }); disclose.setAttribute("aria-expanded", String(detailsOpen));
    actions(summary, disclose, action("setup.copyPaths", t("AutoSetup.copyPaths"), async () => {
      try { await navigator.clipboard.writeText(paths.map(([label, value]) => `${label}: ${value ?? ""}`).join("\n")); setupStatus(product("setupCopied")); }
      catch { setupStatus(product("setupCopyFailed")); }
    }));
    summary.append(details);
    actions(summary,
      action("setup.chooseLocal", t("AutoSetup.chooseExistingKataGo"), () => resourceCommand({ type: "ChooseLocalEngine", choice: { executable: null, config: null, weight: null } })),
      action("setup.repairAnalysis", t("AutoSetup.repairAnalysisConfig"), () => resourceCommand({ type: "RepairAnalysisConfig" }), !!r.inventory.gtpConfig),
      action("setup.openFolder", t("AutoSetup.openAppFolder"), () => resourceCommand({ type: "OpenResourceFolder" })),
      action("setup.fullDownloads", t("AutoSetup.viewFullDownloads"), async () => { try { await invoke("open_katago_releases"); } catch (error) { await message(t("AutoSetup.title"), String(error), "error"); } }),
      action("setup.refresh", t("AutoSetup.refresh"), () => resourceCommand({ type: "Inspect", refresh: true })));
    if (r.chosenEngine) {
      const chosen = section(t("AutoSetup.sourceManualSelection"));
      row(chosen, t("AutoSetup.localEngine"), r.chosenEngine.executable); row(chosen, t("AutoSetup.localWeight"), r.chosenWeight ?? t("AutoSetup.notFound"));
      actions(chosen, action("setup.useLocal", product("setupUse"), () => planSetup(r.chosenEngine!, r.chosenWeight ?? undefined), setupVerified(r.chosenEngine))); summary.append(chosen);
    }
    element.append(summary);
    const machine = section(product("setupComputer"), product("setupSeparate")), m = r.machine;
    row(machine, product("setupOsArch"), m ? `${m.os} / ${m.arch}${m.rosetta ? " (Rosetta)" : ""}` : product("setupUnknown"));
    row(machine, product("setupCpu"), m ? `${m.cpuName ?? product("setupUnknown")}${m.logicalCpus ? ` · ${m.logicalCpus}` : ""}` : product("setupUnknown"));
    row(machine, product("setupMemory"), size(m?.totalMemoryBytes));
    row(machine, product("setupGpu"), m?.nvidiaGpus.length ? m.nvidiaGpus.map((g) => `${g.name} · ${product("setupVram")}: ${g.memoryMib == null ? product("setupUnknown") : size(Number(g.memoryMib) * 1024 ** 2)} · ${product("setupDriver")}: ${g.driver ?? product("setupUnknown")}`).join("\n") : m?.apple ? `${m.apple.chip ?? product("setupUnknown")} · Metal · ${product("setupVram")}: ${product("setupUnknown")}` : product("setupUnknown"));
    element.append(machine);
    const reasonText: Record<RecommendationReason, string> = {
      nvidiaDetected: product("setupReasonNvidia"), nvidiaNotDetected: product("setupReasonNoNvidia"), driverSupported: product("setupReasonDriver"), driverProbeRequired: product("setupReasonProbe"), driverUnknown: product("setupReasonDriverUnknown"), driverTooOld: product("setupReasonOld"),
      openclAvailable: product("setupReasonOpencl"), openclUnavailable: product("setupReasonNoOpencl"), cpuFallback: product("setupReasonCpu"), nativeMetal: product("setupReasonMetal"), rosettaUseArm64: product("setupReasonRosetta"), tensorRtAllowed: product("setupReasonTrt"), tensorRtNotRecommended: product("setupReasonNoTrt"), computeUnknown: product("setupReasonCompute"), experimentalManualOnly: product("setupReasonExperimental"), dependenciesMissing: product("setupReasonMissing"),
      cpuOptimized: product("setupReasonCpuOptimized"), cpuFeaturesUnknown: product("setupReasonCpuFeaturesUnknown"), fuseUnknown: product("setupReasonFuseUnknown"), libraryVersionUnknown: product("setupReasonLibraryVersionUnknown"),
    };
    const missingText = (d: MissingDependency): string => {
      switch (d.kind) {
        case "nvidiaGpu": return product("setupDependencyGpu");
        case "minimumDriver": return product("setupDependencyDriver", d.detail);
        case "linuxLibrary": return d.detail;
        case "loaderCacheUnknown": return product("setupDependencyLoader");
        case "windowsNvidiaRuntime": return product("setupDependencyNvidia");
        case "tensorRtRuntime": return product("setupDependencyTrt");
        case "openclIcd": return product("setupDependencyOpencl");
        case "directX12": return product("setupDependencyDx");
        case "openVinoDevice": return product("setupDependencyIntel");
        case "rocmFamily": return product("setupDependencyRocm", d.detail);
        case "cpuFeature": return product("setupDependencyCpu", d.detail);
        case "appImageFuse": return product("setupDependencyFuse");
      }
    };
    const runtime: Record<RuntimeRequirement, string> = {
      none: product("none"), "linux-cuda-cudnn": "NVIDIA CUDA / cuDNN", "nvidia-runtime-download": product("setupDependencyNvidia"), "tensorrt-runtime": product("setupDependencyTrt"), "opencl-icd": product("setupDependencyOpencl"), metal: "Metal", directx12: "DirectX 12", openvino: "OpenVINO", "rocm-family": "ROCm",
    };
    const runtimeText = (asset: Asset): string => {
      const extras = [
        asset.cuda ? `CUDA ${asset.cuda}` : "", asset.cudnn ? `cuDNN ${asset.cudnn}` : "", asset.tensorrt ? `TensorRT ${asset.tensorrt}` : "",
        asset.minimumDriver ? product("setupDependencyDriver", asset.minimumDriver) : "", asset.gpuFamily,
        asset.platform.startsWith("linux-") && asset.externalLibraries.length ? `${product("setupLinuxLibraries")}: ${asset.externalLibraries.join(", ")}` : "",
        asset.appImage ? product("setupAppImage") : "",
        asset.cpuFeatures.length ? `${product("setupCpuFeatures")}: ${asset.cpuFeatures.map((f) => f.toUpperCase()).join(", ")}` : "",
      ].filter(Boolean);
      return [...(asset.runtimeRequirement === "none" && extras.length ? [] : [runtime[asset.runtimeRequirement]]), ...extras].join(" · ");
    };
    const focusText: Record<CapabilityStatus, string> = { unsupported: product("setupFocusUnsupported"), unverified: product("setupFocusUnverified") };
    const contributionText: Record<CapabilityStatus, string> = { unsupported: product("setupContributionUnsupported"), unverified: product("setupContributionUnverified") };
    const recommended = r.recommendation?.packages.find((p) => p.recommended);
    const recommendation = section(product("setupRecommended"));
    if (recommended) recommendation.append(node("strong", recommended.assetId), node("p", recommended.reasons.map((key) => reasonText[key]).join(" · ")));
    else recommendation.append(node("p", r.recommendation?.packages.length ? product("setupNoEligible") : product("setupUnknown")));
    if (recommended?.missingDependencies.length) row(recommendation, product("setupMissing"), recommended.missingDependencies.map(missingText).join(" · "));
    element.append(recommendation);
    const list = section(product("setupPackages"));
    actions(list, action("setup.importPackage", product("setupImportPackage"), async () => {
      if (r.machine?.os !== "windows") { await resourceCommand({ type: "ImportPackage", path: null, backend: null, confirmed: false }); return; }
      const variants: { backend: ExperimentalBackend | null; label: string }[] = [
        { backend: null, label: product("setupStandardPackage") },
        { backend: "direct-ml", label: t("AutoSetup.experimentalDirectMl") },
        { backend: "open-vino-gpu", label: t("AutoSetup.experimentalOpenVinoGpu") },
        { backend: "open-vino-npu", label: t("AutoSetup.experimentalOpenVinoNpu") },
        { backend: "rocm-gfx103x", label: t("AutoSetup.experimentalRocmGfx103x") },
        { backend: "rocm-gfx110x", label: t("AutoSetup.experimentalRocmGfx110x") },
        { backend: "rocm-gfx1151", label: t("AutoSetup.experimentalRocmGfx1151") },
        { backend: "rocm-gfx120x", label: t("AutoSetup.experimentalRocmGfx120x") },
      ];
      const body = node("div", "", "setup-plan"), selector = node("select"); selector.dataset.key = "setup.importPackage.backend";
      for (const v of variants) { const option = node("option", v.label); option.value = v.backend ?? ""; selector.append(option); }
      body.append(selector);
      if (!(await ask(product("setupImportPackage"), body, product("setupImportPackage"), product("cancel"), true))) return;
      const variant = variants.find((v) => (v.backend ?? "") === selector.value); if (!variant) return;
      if (variant.backend && !(await confirm(t("AutoSetup.experimentalBackends"), product("setupImportExperimental", variant.label)))) return;
      await resourceCommand({ type: "ImportPackage", path: null, backend: variant.backend, confirmed: !!variant.backend });
    }));
    for (const [tier, title] of [["stable", product("setupStable")], ["optional", product("setupOptional")], ["experimental", product("setupExperimental")]] as const) {
      const assets = r.packages.filter((a) => a.releaseTier === tier && a.platform === r.recommendation?.platform);
      if (!assets.length) continue;
      list.append(node("h4", title));
      for (const asset of assets) {
        const item = node("article", "", "setup-package");
        const installed = currentPackage(r, asset.id);
        const previous = !installed && r.inventory.packages.some((p) => p.receipt.assetId === asset.id && p.receipt.archiveSha256.toLowerCase() !== asset.sha256.toLowerCase());
        const target = installed && packageTarget(installed);
        const option = r.recommendation?.packages.find((p) => p.assetId === asset.id);
        item.append(node("strong", `${asset.id}${option?.recommended ? ` · ${t("AutoSetup.recommendedBadge")}` : ""}`), node("span", `${size(asset.sizeBytes)} · ${installed ? product("setupInstalled") : previous ? product("setupInstalledPrevious") : product("setupNotInstalled")}`, "setup-package-state"));
        const origin = asset.origin === "official-release" ? product("setupOriginOfficial") : product("setupOriginProject", asset.sourceTag);
        row(item, product("setupSource"), `${origin} · KataGo ${asset.version} · ${asset.sourceTag} ${asset.sourceCommit.slice(0, 12)}\n${r.packageUrls[asset.id] ?? ""}`);
        row(item, product("setupLicense"), `${asset.licenseSummary} · ${asset.licenseTexts.join(", ")}`);
        row(item, product("setupRuntime"), runtimeText(asset));
        item.append(node("p", product("setupCapabilities", focusText[asset.focus], contributionText[asset.contribution]), "setup-hint"));
        if (option) {
          item.append(node("p", option.reasons.map((key) => reasonText[key]).join(" · "), "setup-hint"));
          if (option.missingDependencies.length) row(item, product("setupMissing"), option.missingDependencies.map(missingText).join(" · "));
        }
        const backend = assetBackends(asset)[0] ?? null;
        actions(item,
          action(`setup.package.${asset.id}.download`, t("AutoSetup.downloadShort"), () => downloadPackage(asset, backend)),
          action(`setup.package.${asset.id}.verify`, product("setupVerify"), () => target ? resourceCommand({ type: "Verify", target, weight: r.currentSetup.weight ?? r.chosenWeight ?? r.inventory.weights[0]?.path ?? null }) : undefined, !!target),
          action(`setup.package.${asset.id}.use`, product("setupUse"), () => target ? planSetup(target) : undefined, !!target && setupVerified(target) && (!!r.currentSetup.weight || !!r.chosenWeight || r.inventory.weights.length > 0)));
        list.append(item);
      }
    }
    element.append(list);
  } };
}
