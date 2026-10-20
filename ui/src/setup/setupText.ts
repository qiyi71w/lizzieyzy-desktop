import { format, product, registerProduct, t } from "../i18n";

registerProduct({
  EnUs: {
    setupPlanStale: "The setup preview is no longer current. Review the setup again before applying it.", setupTaskStale: "That resource task is no longer active.", setupInvalid: "This engine, weight, or configuration cannot be used together. Verify the engine and check its supporting files.", setupUnmanaged: "This profile is not managed by the resource store and will not be replaced. Add a new profile instead.", setupImportMismatch: "This archive does not match a trusted engine package for this platform. Check the package and its SHA-256 checksum.", setupDownloadFailed: "The requested resource is not in the trusted catalog.",
    setupStandardPackage: "Stable or optional package", setupImportExperimental: "Import {0}? This is an experimental backend. Confirm that the selected hardware family matches this computer. Only the trusted catalog archive will be accepted; no profile will change.",
    setupImportPackageDone: "Engine package imported. Verify it before applying a setup.",
    setupComputer: "This computer", setupCpu: "CPU", setupMemory: "Memory", setupUnknown: "unknown",
    setupRecommended: "Recommended engine package", setupSeparate: "Engine resources are delivered separately from this client. Detection does not download, replace a configuration, or select the fastest engine.",
    setupPackages: "Packages for this platform", setupStable: "Stable", setupOptional: "Optional", setupExperimental: "Experimental",
    setupSource: "Source", setupLicense: "License", setupRuntime: "Runtime", setupAnalysis: "Analysis supported · Contribution: not provided by these packages",
    setupImportPackage: "Import package…", setupVerify: "Verify", setupUse: "Use this setup…", setupMissing: "Missing dependencies", setupInstalled: "Installed", setupNotInstalled: "Not installed",
    setupNewProfile: "Add a new managed profile", setupUpdateProfile: "Update managed profile", setupPlanTitle: "Confirm engine setup", setupName: "Name", setupCommand: "Command",
    setupMakeDefault: "Set as default", setupSwitchNow: "Switch to this engine now", setupSelect: "Choose the weight and profile for this setup. Existing unmanaged profiles are never changed.",
    setupDownloadConfirm: "Download {0} ({1})? This installs resources only; engine profiles are not changed.", setupNvidiaConfirm: "NVIDIA runtime downloads may require about 1.8 GB for CUDA and up to 3.7 GB with TensorRT. Drivers are not installed. Continue?",
    setupReasonNvidia: "NVIDIA GPU detected", setupReasonNoNvidia: "No NVIDIA GPU detected", setupReasonDriver: "Driver supports CUDA", setupReasonProbe: "Driver requires an explicit inference probe", setupReasonDriverUnknown: "Driver compatibility unknown", setupReasonOld: "Driver is too old",
    setupReasonOpencl: "OpenCL ICD available", setupReasonNoOpencl: "OpenCL ICD unavailable", setupReasonCpu: "CPU fallback; no GPU runtime required", setupReasonMetal: "Native Metal backend", setupReasonRosetta: "Rosetta detected; use the arm64 Metal package", setupReasonTrt: "Optional TensorRT supported by this GPU", setupReasonNoTrt: "TensorRT is not recommended for this GPU", setupReasonCompute: "Compute capability unknown", setupReasonExperimental: "Manual experimental selection only", setupReasonMissing: "Runtime dependencies are missing",
    setupDependencyGpu: "NVIDIA GPU", setupDependencyDriver: "NVIDIA driver ≥ {0}", setupDependencyLoader: "System library loader information unknown", setupDependencyNvidia: "Windows NVIDIA runtime download", setupDependencyTrt: "TensorRT runtime", setupDependencyOpencl: "OpenCL ICD", setupDependencyDx: "DirectX 12 GPU", setupDependencyIntel: "Matching Intel GPU/NPU", setupDependencyRocm: "Matching AMD GPU: {0}", setupCopied: "Paths copied", setupCopyFailed: "Could not copy paths", setupRemote: "Remote Compute",
  },
  ZhCn: {
    setupPlanStale: "配置预览已失效，请重新检查后再应用。", setupTaskStale: "该资源任务已结束。", setupInvalid: "此引擎、权重或配置无法配合使用，请验证引擎并检查所需文件。", setupUnmanaged: "此配置不由资源库托管，不会被替换，请新增配置。", setupImportMismatch: "此压缩包与本平台的可信引擎包不匹配，请检查引擎包及 SHA-256 校验值。", setupDownloadFailed: "请求的资源不在可信目录中。",
    setupStandardPackage: "稳定或可选引擎包", setupImportExperimental: "导入 {0}？这是实验性后端，请确认所选硬件系列与本机匹配。仅接受可信目录中的压缩包，不会改变引擎配置。",
    setupImportPackageDone: "引擎包已导入，请先验证再应用配置。",
    setupComputer: "这台电脑", setupCpu: "处理器", setupMemory: "内存", setupUnknown: "未知", setupRecommended: "推荐引擎包", setupSeparate: "本客户端不捆绑引擎资源。检测不会下载、替换配置，也不代表已找到最快引擎。", setupPackages: "适用于此平台的引擎包", setupStable: "稳定", setupOptional: "可选", setupExperimental: "实验性", setupSource: "来源", setupLicense: "许可", setupRuntime: "运行依赖", setupAnalysis: "支持分析 · 棋谱贡献：这些引擎包不提供", setupImportPackage: "导入引擎包…", setupVerify: "验证", setupUse: "使用此配置…", setupMissing: "缺少依赖", setupInstalled: "已安装", setupNotInstalled: "未安装", setupNewProfile: "新增托管引擎配置", setupUpdateProfile: "更新托管引擎配置", setupPlanTitle: "确认引擎配置", setupName: "名称", setupCommand: "命令", setupMakeDefault: "设为默认", setupSwitchNow: "立即切换引擎", setupSelect: "选择权重及配置。现有非托管配置不会被修改。", setupDownloadConfirm: "下载 {0}（{1}）？仅安装资源，不改变引擎配置。", setupNvidiaConfirm: "CUDA 运行库约需下载 1.8 GB，加上 TensorRT 最多约 3.7 GB。不会安装驱动。是否继续？", setupCopied: "路径已复制", setupCopyFailed: "无法复制路径", setupRemote: "远程算力",
  },
  ZhTw: {
    setupPlanStale: "設定預覽已失效，請重新檢查後再套用。", setupTaskStale: "該資源工作已結束。", setupInvalid: "此引擎、權重或設定無法配合使用，請驗證引擎並檢查所需檔案。", setupUnmanaged: "此設定不由資源庫託管，不會被取代，請新增設定。", setupImportMismatch: "此壓縮套件與本平台的可信引擎套件不符，請檢查套件及 SHA-256 校驗值。", setupDownloadFailed: "要求的資源不在可信目錄中。",
    setupStandardPackage: "穩定或可選引擎套件", setupImportExperimental: "匯入 {0}？這是實驗性後端，請確認所選硬體系列與本機相符。僅接受可信目錄中的壓縮套件，不會改變引擎設定。", 
    setupImportPackageDone: "引擎套件已匯入，請先驗證再套用設定。",
    setupComputer: "這台電腦", setupCpu: "處理器", setupMemory: "記憶體", setupUnknown: "未知", setupRecommended: "建議引擎套件", setupSeparate: "本客戶端不隨附引擎資源。偵測不會下載、取代設定，也不代表已找到最快引擎。", setupPackages: "適用此平台的引擎套件", setupStable: "穩定", setupOptional: "可選", setupExperimental: "實驗性", setupSource: "來源", setupLicense: "授權", setupRuntime: "執行依賴", setupAnalysis: "支援分析 · 棋譜貢獻：這些套件不提供", setupImportPackage: "匯入引擎套件…", setupVerify: "驗證", setupUse: "使用此設定…", setupMissing: "缺少依賴", setupInstalled: "已安裝", setupNotInstalled: "未安裝", setupNewProfile: "新增託管引擎設定", setupUpdateProfile: "更新託管引擎設定", setupPlanTitle: "確認引擎設定", setupName: "名稱", setupCommand: "命令", setupMakeDefault: "設為預設", setupSwitchNow: "立即切換引擎", setupSelect: "選擇權重及設定。現有非託管設定不會被修改。", setupDownloadConfirm: "下載 {0}（{1}）？僅安裝資源，不改變引擎設定。", setupNvidiaConfirm: "CUDA 執行庫約需下載 1.8 GB，加上 TensorRT 最多約 3.7 GB。不會安裝驅動程式。是否繼續？", setupCopied: "路徑已複製", setupCopyFailed: "無法複製路徑", setupRemote: "遠端算力",
  },
});

export function node<K extends keyof HTMLElementTagNameMap>(tag: K, text = "", className = ""): HTMLElementTagNameMap[K] {
  const el = document.createElement(tag); el.textContent = text; el.className = className; return el;
}
export function action(key: string, text: string, fn: () => void | Promise<unknown>, enabled = true): HTMLButtonElement {
  const el = node("button", text); el.dataset.key = key; el.disabled = !enabled; el.onclick = () => { void fn(); }; return el;
}
export function future(key: string, text: string): HTMLButtonElement {
  const el = action(key, text, () => {}, false); el.className = "unavailable"; el.title = product("unavailable"); return el;
}
export function section(title: string, subtitle = ""): HTMLElement {
  const el = node("section", "", "setup-section"); el.append(node("h3", title)); if (subtitle) el.append(node("p", subtitle, "setup-hint")); return el;
}
export function notice(): HTMLElement {
  const el = node("section", "", "setup-b11"); el.append(node("strong", t("AutoSetup.b11DefaultNoticeTitle")), node("p", t("AutoSetup.b11DefaultNoticeBody"))); return el;
}
export function row(host: HTMLElement, label: string, value: string, chip = false): HTMLElement {
  const el = node("div", "", "setup-info-row"); const content = node("span", value || product("setupUnknown"), chip ? "setup-chip" : "setup-info-value"); content.title = value; el.append(node("span", label), content); host.append(el); return content;
}
export function actions(host: HTMLElement, ...controls: HTMLElement[]): void { const el = node("div", "", "setup-actions"); el.append(...controls); host.append(el); }
export function size(bytes: number | bigint | null | undefined): string {
  if (bytes == null) return product("setupUnknown"); const n = Number(bytes); return n >= 1024 ** 3 ? `${(n / 1024 ** 3).toFixed(2)} GB` : `${(n / 1024 ** 2).toFixed(1)} MB`;
}
export function javaFormat(text: string, ...args: string[]): string { let i = 0; return format(text, ...args).replace(/%s/g, () => args[i++] ?? ""); }

/** Literal calls keep runtime projection keys in the Java string extraction set. */
export function resourceText(key: string): string {
  const strings: Record<string, string> = {
    "AutoSetup.importPackageDone": product("setupImportPackageDone"), "AutoSetup.installNvidiaRuntimeDone": t("AutoSetup.installNvidiaRuntimeDone"), "AutoSetup.refresh": t("AutoSetup.refresh"), "AutoSetup.repairAnalysisConfig": t("AutoSetup.repairAnalysisConfig"),
    "AutoSetup.planStale": product("setupPlanStale"), "AutoSetup.taskStale": product("setupTaskStale"), "AutoSetup.setupInvalid": product("setupInvalid"), "AutoSetup.unmanagedProfile": product("setupUnmanaged"), "AutoSetup.importPackageMismatch": product("setupImportMismatch"), "AutoSetup.downloadFailed": product("setupDownloadFailed"),
    "EngineThreadPolicy.benchmarkBusy": t("EngineThreadPolicy.benchmarkBusy"), "AutoSetup.nvidiaRuntimeNotApplicable": t("AutoSetup.nvidiaRuntimeNotApplicable"), "AutoSetup.tensorRtEngineMissing": t("AutoSetup.tensorRtEngineMissing"), "AutoSetup.tensorRtMissingWeight": t("AutoSetup.tensorRtMissingWeight"),
    "AutoSetup.downloadCancelled": t("AutoSetup.downloadCancelled"), "AutoSetup.downloadDone": t("AutoSetup.downloadDone"), "AutoSetup.failed": t("AutoSetup.failed"),
    "AutoSetup.weightChecksumFailed": t("AutoSetup.weightChecksumFailed"), "AutoSetup.experimentalBackendChecksumFailed": t("AutoSetup.experimentalBackendChecksumFailed"),
    "AutoSetup.remoteRefreshFailedCached": t("AutoSetup.remoteRefreshFailedCached"), "AutoSetup.importWeightDone": t("AutoSetup.importWeightDone"), "AutoSetup.importWeightInvalid": t("AutoSetup.importWeightInvalid"),
    "AutoSetup.setupDone": t("AutoSetup.setupDone"), "AutoSetup.taskAlreadyRunning": t("AutoSetup.taskAlreadyRunning"), "AutoSetup.tensorRtRepairDone": t("AutoSetup.tensorRtRepairDone"),
    "AutoSetup.tensorRtNotApplicable": t("AutoSetup.tensorRtNotApplicable"), "AutoSetup.experimentalBackendWindowsOnly": t("AutoSetup.experimentalBackendWindowsOnly"), "AutoSetup.transformerRequires117": t("AutoSetup.transformerRequires117"),
    "AutoSetup.validationNotAvailable": t("AutoSetup.validationNotAvailable"), "AutoSetup.validationChecking": t("AutoSetup.validationChecking"), "AutoSetup.validationPassed": t("AutoSetup.validationPassed"), "AutoSetup.validationRunning": t("AutoSetup.validationRunning"),
    "AutoSetup.validationMissingDependency": t("AutoSetup.validationMissingDependency"), "AutoSetup.validationWrongArchitecture": t("AutoSetup.validationWrongArchitecture"), "AutoSetup.validationTimedOut": t("AutoSetup.validationTimedOut"), "AutoSetup.validationStartFailed": t("AutoSetup.validationStartFailed"),
    "AutoSetup.cleanTensorRtCacheDone": t("AutoSetup.cleanTensorRtCacheDone"), "AutoSetup.cleanTensorRtCacheEmpty": t("AutoSetup.cleanTensorRtCacheEmpty"), "AutoSetup.experimentalBackendDone": t("AutoSetup.experimentalBackendDone"),
    "AutoSetup.recommendationBalancedHint": t("AutoSetup.recommendationBalancedHint"), "AutoSetup.recommendationStrongestHint": t("AutoSetup.recommendationStrongestHint"), "AutoSetup.recommendationLightweightHint": t("AutoSetup.recommendationLightweightHint"),
  };
  return strings[key] ?? t(key);
}
