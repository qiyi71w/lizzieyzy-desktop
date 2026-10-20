import { format, product, registerProduct, t } from "../i18n";

registerProduct({
  EnUs: {
    setupPlanStale: "The setup preview is no longer current. Review the setup again before applying it.", setupTaskStale: "That resource task is no longer active.", setupInvalid: "This engine, weight, or configuration cannot be used together. Verify the engine and check its supporting files.", setupUnmanaged: "This profile is not managed by the resource store and will not be replaced. Add a new profile instead.", setupImportMismatch: "This archive does not match a trusted engine package for this platform. Check the package and its SHA-256 checksum.", setupDownloadFailed: "The requested resource is not in the trusted catalog.",
    setupStandardPackage: "Stable or optional package", setupImportExperimental: "Import {0}? This is an experimental backend. Confirm that the selected hardware family matches this computer. Only the trusted catalog archive will be accepted; no profile will change.",
    setupImportPackageDone: "Engine package imported. Verify it before applying a setup.", setupTargetProfile: "Engine profile",
    setupPackageChecksumFailed: "The engine package did not match its pinned size or SHA-256 and was deleted. Download it again.",
    setupNoEligible: "No package for this platform has all of its dependencies on this computer. See the missing dependencies listed with each package below.",
    setupPhaseDetecting: "Detecting this computer…", setupPhaseDownloading: "Downloading", setupPhaseVerifying: "Verifying", setupPhaseInstalling: "Installing",
    setupOsArch: "OS / Architecture", setupGpu: "GPU", setupVram: "VRAM", setupDriver: "Driver", setupLinuxLibraries: "Linux system libraries",
    setupComputer: "This computer", setupCpu: "CPU", setupMemory: "Memory", setupUnknown: "unknown",
    setupRecommended: "Recommended engine package", setupSeparate: "Engine resources are delivered separately from this client. Detection does not download, replace a configuration, or select the fastest engine.",
    setupPackages: "Packages for this platform", setupStable: "Stable", setupOptional: "Optional", setupExperimental: "Experimental",
    setupSource: "Source", setupLicense: "License", setupRuntime: "Runtime",
    setupOriginOfficial: "KataGo official release", setupOriginProject: "Built by this project from the official {0} source", setupAppImage: "AppImage (needs FUSE)", setupCpuFeatures: "CPU features",
    setupCapabilities: "Analysis: supported · Selected-move focus: {0} · Contribution: {1}", setupFocusUnsupported: "not supported by this release", setupFocusUnverified: "not verified by this app",
    setupContributionUnverified: "built with contribution support upstream; not verified by this app (provided by the contribution feature)", setupContributionUnsupported: "not supported by this build",
    setupInstalledPrevious: "Installed (previous package)",
    setupReasonCpuOptimized: "CPU supports AVX2 and FMA; optimized CPU build", setupReasonCpuFeaturesUnknown: "CPU features (AVX2/FMA) unknown", setupReasonFuseUnknown: "FUSE availability unknown; the engine check reports a mount failure", setupReasonLibraryVersionUnknown: "System library versions unknown",
    setupDependencyCpu: "CPU feature: {0}", setupDependencyFuse: "FUSE for AppImage (/dev/fuse and fusermount)",
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
    setupImportPackageDone: "引擎包已导入，请先验证再应用配置。", setupTargetProfile: "引擎配置",
    setupPackageChecksumFailed: "引擎包未通过大小或 SHA-256 校验，损坏文件已删除，请重新下载。",
    setupNoEligible: "本机没有满足全部依赖的引擎包，请查看下方各引擎包列出的缺少依赖。",
    setupPhaseDetecting: "正在检测本机…", setupPhaseDownloading: "正在下载", setupPhaseVerifying: "正在验证", setupPhaseInstalling: "正在安装",
    setupOsArch: "系统 / 架构", setupGpu: "显卡", setupVram: "显存", setupDriver: "驱动", setupLinuxLibraries: "Linux 系统库",
    setupReasonNvidia: "检测到 NVIDIA 显卡", setupReasonNoNvidia: "未检测到 NVIDIA 显卡", setupReasonDriver: "驱动支持 CUDA", setupReasonProbe: "驱动需要先做一次推理验证", setupReasonDriverUnknown: "驱动兼容性未知", setupReasonOld: "驱动版本过旧",
    setupReasonOpencl: "已有 OpenCL 驱动（ICD）", setupReasonNoOpencl: "未找到 OpenCL 驱动（ICD）", setupReasonCpu: "CPU 通用方案，无需显卡运行库", setupReasonMetal: "原生 Metal 后端", setupReasonRosetta: "检测到 Rosetta 转译，请使用 arm64 Metal 引擎包", setupReasonTrt: "此显卡可选用 TensorRT", setupReasonNoTrt: "此显卡不推荐 TensorRT", setupReasonCompute: "算力版本未知", setupReasonExperimental: "仅可手动选择的实验性后端", setupReasonMissing: "缺少运行依赖",
    setupDependencyGpu: "NVIDIA 显卡", setupDependencyDriver: "NVIDIA 驱动 ≥ {0}", setupDependencyLoader: "无法读取系统库加载信息", setupDependencyNvidia: "需下载 Windows NVIDIA 运行库", setupDependencyTrt: "TensorRT 运行库", setupDependencyOpencl: "OpenCL 驱动（ICD）", setupDependencyDx: "支持 DirectX 12 的显卡", setupDependencyIntel: "匹配的 Intel 显卡/NPU", setupDependencyRocm: "匹配的 AMD 显卡：{0}",
    setupComputer: "这台电脑", setupCpu: "处理器", setupMemory: "内存", setupUnknown: "未知", setupRecommended: "推荐引擎包", setupSeparate: "本客户端不捆绑引擎资源。检测不会下载、替换配置，也不代表已找到最快引擎。", setupPackages: "适用于此平台的引擎包", setupStable: "稳定", setupOptional: "可选", setupExperimental: "实验性", setupSource: "来源", setupLicense: "许可", setupRuntime: "运行依赖", setupImportPackage: "导入引擎包…", setupVerify: "验证", setupUse: "使用此配置…", setupMissing: "缺少依赖", setupInstalled: "已安装", setupNotInstalled: "未安装", setupNewProfile: "新增托管引擎配置", setupUpdateProfile: "更新托管引擎配置", setupPlanTitle: "确认引擎配置", setupName: "名称", setupCommand: "命令", setupMakeDefault: "设为默认", setupSwitchNow: "立即切换引擎", setupSelect: "选择权重及配置。现有非托管配置不会被修改。", setupDownloadConfirm: "下载 {0}（{1}）？仅安装资源，不改变引擎配置。", setupNvidiaConfirm: "CUDA 运行库约需下载 1.8 GB，加上 TensorRT 最多约 3.7 GB。不会安装驱动。是否继续？", setupCopied: "路径已复制", setupCopyFailed: "无法复制路径", setupRemote: "远程算力",
    setupOriginOfficial: "KataGo 官方发布", setupOriginProject: "本项目依据官方 {0} 源码构建", setupAppImage: "AppImage（需要 FUSE）", setupCpuFeatures: "CPU 指令集",
    setupCapabilities: "分析：支持 · 选点焦点分析：{0} · 棋谱贡献：{1}", setupFocusUnsupported: "此版本不支持", setupFocusUnverified: "本应用未验证",
    setupContributionUnverified: "上游构建含贡献支持；本应用未验证（由贡献功能提供）", setupContributionUnsupported: "此构建不支持",
    setupInstalledPrevious: "已安装（旧版引擎包）",
    setupReasonCpuOptimized: "CPU 支持 AVX2 和 FMA，使用优化的 CPU 版本", setupReasonCpuFeaturesUnknown: "CPU 指令集（AVX2/FMA）未知", setupReasonFuseUnknown: "FUSE 可用性未知；引擎检查会报告挂载失败", setupReasonLibraryVersionUnknown: "系统库版本未知",
    setupDependencyCpu: "CPU 指令集：{0}", setupDependencyFuse: "AppImage 所需的 FUSE（/dev/fuse 和 fusermount）",
  },
  ZhTw: {
    setupPlanStale: "設定預覽已失效，請重新檢查後再套用。", setupTaskStale: "該資源工作已結束。", setupInvalid: "此引擎、權重或設定無法配合使用，請驗證引擎並檢查所需檔案。", setupUnmanaged: "此設定不由資源庫託管，不會被取代，請新增設定。", setupImportMismatch: "此壓縮套件與本平台的可信引擎套件不符，請檢查套件及 SHA-256 校驗值。", setupDownloadFailed: "要求的資源不在可信目錄中。",
    setupStandardPackage: "穩定或可選引擎套件", setupImportExperimental: "匯入 {0}？這是實驗性後端，請確認所選硬體系列與本機相符。僅接受可信目錄中的壓縮套件，不會改變引擎設定。", 
    setupImportPackageDone: "引擎套件已匯入，請先驗證再套用設定。", setupTargetProfile: "引擎設定",
    setupPackageChecksumFailed: "引擎套件未通過大小或 SHA-256 校驗，損壞檔案已刪除，請重新下載。",
    setupNoEligible: "本機沒有滿足全部依賴的引擎套件，請查看下方各引擎套件列出的缺少依賴。",
    setupPhaseDetecting: "正在偵測本機…", setupPhaseDownloading: "正在下載", setupPhaseVerifying: "正在驗證", setupPhaseInstalling: "正在安裝",
    setupOsArch: "系統 / 架構", setupGpu: "顯示卡", setupVram: "顯示記憶體", setupDriver: "驅動程式", setupLinuxLibraries: "Linux 系統程式庫",
    setupReasonNvidia: "偵測到 NVIDIA 顯示卡", setupReasonNoNvidia: "未偵測到 NVIDIA 顯示卡", setupReasonDriver: "驅動程式支援 CUDA", setupReasonProbe: "驅動程式需要先做一次推理驗證", setupReasonDriverUnknown: "驅動程式相容性未知", setupReasonOld: "驅動程式版本過舊",
    setupReasonOpencl: "已有 OpenCL 驅動（ICD）", setupReasonNoOpencl: "未找到 OpenCL 驅動（ICD）", setupReasonCpu: "CPU 通用方案，無需顯示卡執行庫", setupReasonMetal: "原生 Metal 後端", setupReasonRosetta: "偵測到 Rosetta 轉譯，請使用 arm64 Metal 引擎套件", setupReasonTrt: "此顯示卡可選用 TensorRT", setupReasonNoTrt: "此顯示卡不建議 TensorRT", setupReasonCompute: "算力版本未知", setupReasonExperimental: "僅可手動選擇的實驗性後端", setupReasonMissing: "缺少執行依賴",
    setupDependencyGpu: "NVIDIA 顯示卡", setupDependencyDriver: "NVIDIA 驅動程式 ≥ {0}", setupDependencyLoader: "無法讀取系統程式庫載入資訊", setupDependencyNvidia: "需下載 Windows NVIDIA 執行庫", setupDependencyTrt: "TensorRT 執行庫", setupDependencyOpencl: "OpenCL 驅動（ICD）", setupDependencyDx: "支援 DirectX 12 的顯示卡", setupDependencyIntel: "相符的 Intel 顯示卡/NPU", setupDependencyRocm: "相符的 AMD 顯示卡：{0}",
    setupComputer: "這台電腦", setupCpu: "處理器", setupMemory: "記憶體", setupUnknown: "未知", setupRecommended: "建議引擎套件", setupSeparate: "本客戶端不隨附引擎資源。偵測不會下載、取代設定，也不代表已找到最快引擎。", setupPackages: "適用此平台的引擎套件", setupStable: "穩定", setupOptional: "可選", setupExperimental: "實驗性", setupSource: "來源", setupLicense: "授權", setupRuntime: "執行依賴", setupImportPackage: "匯入引擎套件…", setupVerify: "驗證", setupUse: "使用此設定…", setupMissing: "缺少依賴", setupInstalled: "已安裝", setupNotInstalled: "未安裝", setupNewProfile: "新增託管引擎設定", setupUpdateProfile: "更新託管引擎設定", setupPlanTitle: "確認引擎設定", setupName: "名稱", setupCommand: "命令", setupMakeDefault: "設為預設", setupSwitchNow: "立即切換引擎", setupSelect: "選擇權重及設定。現有非託管設定不會被修改。", setupDownloadConfirm: "下載 {0}（{1}）？僅安裝資源，不改變引擎設定。", setupNvidiaConfirm: "CUDA 執行庫約需下載 1.8 GB，加上 TensorRT 最多約 3.7 GB。不會安裝驅動程式。是否繼續？", setupCopied: "路徑已複製", setupCopyFailed: "無法複製路徑", setupRemote: "遠端算力",
    setupOriginOfficial: "KataGo 官方發行", setupOriginProject: "本專案依據官方 {0} 原始碼建置", setupAppImage: "AppImage（需要 FUSE）", setupCpuFeatures: "CPU 指令集",
    setupCapabilities: "分析：支援 · 選點焦點分析：{0} · 棋譜貢獻：{1}", setupFocusUnsupported: "此版本不支援", setupFocusUnverified: "本應用程式未驗證",
    setupContributionUnverified: "上游建置含貢獻支援；本應用程式未驗證（由貢獻功能提供）", setupContributionUnsupported: "此建置不支援",
    setupInstalledPrevious: "已安裝（舊版引擎套件）",
    setupReasonCpuOptimized: "CPU 支援 AVX2 與 FMA，使用最佳化的 CPU 版本", setupReasonCpuFeaturesUnknown: "CPU 指令集（AVX2/FMA）未知", setupReasonFuseUnknown: "FUSE 可用性未知；引擎檢查會回報掛載失敗", setupReasonLibraryVersionUnknown: "系統程式庫版本未知",
    setupDependencyCpu: "CPU 指令集：{0}", setupDependencyFuse: "AppImage 所需的 FUSE（/dev/fuse 與 fusermount）",
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
    "AutoSetup.weightChecksumFailed": t("AutoSetup.weightChecksumFailed"), "AutoSetup.experimentalBackendChecksumFailed": t("AutoSetup.experimentalBackendChecksumFailed"), "AutoSetup.packageChecksumFailed": product("setupPackageChecksumFailed"),
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
