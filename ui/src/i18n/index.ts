// UI text. Java's own wording comes from its DisplayStrings bundles (javaStrings.json, regenerated
// by tools/extract_java_strings.py); the few product messages without a Java counterpart are in
// PRODUCT below. The language is fixed for a session, like Java (changing it needs a restart).
import data from "./javaStrings.json";
import type { Language } from "../contract/generated";

type Table = Record<string, string>;
const JAVA = data.strings as Record<string, Table>;

/** The LizzieYzy Next settings import (no Java counterpart). Targets name this product's settings. */
const IMPORT_TEXT: Record<string, Table> = {
  ZhCn: {
    javaImport: "从 LizzieYzy Next 导入设置…",
    importTitle: "从 LizzieYzy Next 导入设置",
    importFirstUse: "这是第一次使用 LizzieYzy Tauri。可以从 LizzieYzy Next 导入复盘、引擎和界面设置；LizzieYzy Next 的文件只会被读取，不会被修改。",
    importIntro: "选择 LizzieYzy Next 保存设置的文件夹（其中有 config.txt）。导入前会先列出将迁入、不兼容和未迁入的项目；LizzieYzy Next 的文件不会被修改。",
    importFolder: "LizzieYzy Next 设置文件夹",
    importFound: "已找到：",
    browse: "浏览…",
    importPreview: "预览",
    importSkip: "不导入",
    importBack: "返回",
    importDo: "导入",
    importNoConfig: "该文件夹中没有 config.txt：{0}",
    importInvalid: "config.txt 不是有效的设置文件（{1}）：{0}",
    importUnreadable: "无法读取 config.txt（{1}）：{0}",
    importSource: "来源：{0}",
    importCounts: "将迁入 {0} 项，不兼容 {1} 项，未迁入 {2} 项",
    importReportCounts: "已迁入 {0} 项，不兼容 {1} 项，未迁入 {2} 项",
    importEngines: "引擎",
    importEngineName: "名称",
    importEngineCommand: "命令",
    importEngineStatus: "状态",
    importEngineProblems: "问题",
    importCopyCredentials: "复制凭据",
    importUnmigratedFields: "未迁入字段：{0}",
    importIncompatibleFields: "不适用字段（LizzieYzy Next 对其自带引擎包的管理）：{0}",
    importProgramFolder: "LizzieYzy Next 程序文件夹",
    importRepreview: "重新预览",
    importReplaces: "以下设置已在 LizzieYzy Tauri 中修改过，导入会替换它们：{0}",
    importReplaceConfirm: "替换这些设置",
    importDone: "导入完成",
    importFixLater: "有问题的引擎可以在“{0}”中修改。",
    importStale: "这个预览已经失效，请重新预览。",
    importNeedsReplace: "导入会替换已修改的设置，请先勾选“替换这些设置”。",
    importListImported: "迁入的设置",
    importListIncompatible: "不兼容的设置",
    importListNotMigrated: "未迁入的设置",
    statusImported: "迁入",
    statusIncompatible: "不兼容",
    statusNotMigrated: "未迁入",
    reasonNotYetAvailable: "所属功能尚未在 LizzieYzy Tauri 中提供",
    reasonCredential: "含账号或凭据，未读取",
    reasonInvalidValue: "值无效",
    reasonWindowCoordinates: "Java 窗口位置按 Java 的缩放和布局记录，不适用于本窗口",
    reasonRemoteEngine: "远程（SSH）引擎尚未提供",
    reasonCredentialNotConfirmed: "命令含凭据，未确认复制",
    reasonUnreadable: "文件无法读取",
    problemProgramNotFound: "找不到程序：{0}",
    problemFileNotFound: "找不到文件：{0} {1}",
    problemEmptyCommand: "命令为空",
    problemInvalidValue: "{0} 无效，使用默认值",
    targetReadKomi: "自动加载棋谱中的贴目",
    targetLoadSgfLast: "打开棋谱后跳到最后一手",
    targetReplaceFileHint: "显示覆盖文件提示",
    targetLanguage: "界面语言",
    targetPlaySound: "落子音效",
    targetRecentFiles: "最近打开的棋谱",
    targetLastFolder: "上次使用的文件夹",
    targetEngines: "引擎列表和默认引擎",
    targetEngineStartup: "启动时加载的引擎",
    targetLastEngine: "上次使用的引擎",
    targetAnalysisInterval: "分析刷新间隔",
    targetLimitTime: "限制分析时间",
    targetMaxAnalyzeTime: "最长分析时间（秒）",
    targetLimitPlayout: "限制计算量",
    targetLimitPlayouts: "最大计算量",
    targetLimitMaxSuggestion: "最多显示的候选数",
    targetLimitBranchLength: "变化图最大长度",
    targetFastEngineChange: "快速切换引擎",
    targetPonderLimitTips: "达到分析限制时提示",
    targetKataRules: "KataGo 规则",
    targetAutoLoadKataRules: "自动加载 KataGo 规则",
    targetStartupBenchmark: "启动时智能测速",
    targetNetworkProxy: "网络代理",
  },
  ZhTw: {
    javaImport: "從 LizzieYzy Next 匯入設定…",
    importTitle: "從 LizzieYzy Next 匯入設定",
    importFirstUse: "這是第一次使用 LizzieYzy Tauri。可以從 LizzieYzy Next 匯入復盤、引擎和介面設定；LizzieYzy Next 的檔案只會被讀取，不會被修改。",
    importIntro: "選擇 LizzieYzy Next 儲存設定的資料夾（其中有 config.txt）。匯入前會先列出將遷入、不相容和未遷入的項目；LizzieYzy Next 的檔案不會被修改。",
    importFolder: "LizzieYzy Next 設定資料夾",
    importFound: "已找到：",
    browse: "瀏覽…",
    importPreview: "預覽",
    importSkip: "不匯入",
    importBack: "返回",
    importDo: "匯入",
    importNoConfig: "該資料夾中沒有 config.txt：{0}",
    importInvalid: "config.txt 不是有效的設定檔（{1}）：{0}",
    importUnreadable: "無法讀取 config.txt（{1}）：{0}",
    importSource: "來源：{0}",
    importCounts: "將遷入 {0} 項，不相容 {1} 項，未遷入 {2} 項",
    importReportCounts: "已遷入 {0} 項，不相容 {1} 項，未遷入 {2} 項",
    importEngines: "引擎",
    importEngineName: "名稱",
    importEngineCommand: "命令",
    importEngineStatus: "狀態",
    importEngineProblems: "問題",
    importCopyCredentials: "複製憑據",
    importUnmigratedFields: "未遷入欄位：{0}",
    importIncompatibleFields: "不適用欄位（LizzieYzy Next 對其內建引擎包的管理）：{0}",
    importProgramFolder: "LizzieYzy Next 程式資料夾",
    importRepreview: "重新預覽",
    importReplaces: "以下設定已在 LizzieYzy Tauri 中修改過，匯入會取代它們：{0}",
    importReplaceConfirm: "取代這些設定",
    importDone: "匯入完成",
    importFixLater: "有問題的引擎可以在「{0}」中修改。",
    importStale: "這個預覽已經失效，請重新預覽。",
    importNeedsReplace: "匯入會取代已修改的設定，請先勾選「取代這些設定」。",
    importListImported: "遷入的設定",
    importListIncompatible: "不相容的設定",
    importListNotMigrated: "未遷入的設定",
    statusImported: "遷入",
    statusIncompatible: "不相容",
    statusNotMigrated: "未遷入",
    reasonNotYetAvailable: "所屬功能尚未在 LizzieYzy Tauri 中提供",
    reasonCredential: "含帳號或憑據，未讀取",
    reasonInvalidValue: "值無效",
    reasonWindowCoordinates: "Java 視窗位置依 Java 的縮放和佈局記錄，不適用於本視窗",
    reasonRemoteEngine: "遠端（SSH）引擎尚未提供",
    reasonCredentialNotConfirmed: "命令含憑據，未確認複製",
    reasonUnreadable: "檔案無法讀取",
    problemProgramNotFound: "找不到程式：{0}",
    problemFileNotFound: "找不到檔案：{0} {1}",
    problemEmptyCommand: "命令為空",
    problemInvalidValue: "{0} 無效，使用預設值",
    targetReadKomi: "自動載入棋譜中的貼目",
    targetLoadSgfLast: "開啟棋譜後跳到最後一手",
    targetReplaceFileHint: "顯示覆蓋檔案提示",
    targetLanguage: "介面語言",
    targetPlaySound: "落子音效",
    targetRecentFiles: "最近開啟的棋譜",
    targetLastFolder: "上次使用的資料夾",
    targetEngines: "引擎清單和預設引擎",
    targetEngineStartup: "啟動時載入的引擎",
    targetLastEngine: "上次使用的引擎",
    targetAnalysisInterval: "分析更新間隔",
    targetLimitTime: "限制分析時間",
    targetMaxAnalyzeTime: "最長分析時間（秒）",
    targetLimitPlayout: "限制計算量",
    targetLimitPlayouts: "最大計算量",
    targetLimitMaxSuggestion: "最多顯示的候選數",
    targetLimitBranchLength: "變化圖最大長度",
    targetFastEngineChange: "快速切換引擎",
    targetPonderLimitTips: "達到分析限制時提示",
    targetKataRules: "KataGo 規則",
    targetAutoLoadKataRules: "自動載入 KataGo 規則",
    targetStartupBenchmark: "啟動時智慧測速",
    targetNetworkProxy: "網路代理",
  },
  EnUs: {
    javaImport: "Import settings from LizzieYzy Next…",
    importTitle: "Import settings from LizzieYzy Next",
    importFirstUse: "This is the first start of LizzieYzy Tauri. You can import review, engine and interface settings from LizzieYzy Next; its files are only read, never changed.",
    importIntro: "Choose the folder where LizzieYzy Next keeps its settings (it contains config.txt). Before anything is imported you see what is taken over, what cannot be used and what is left out; LizzieYzy Next's files are not changed.",
    importFolder: "LizzieYzy Next settings folder",
    importFound: "Found:",
    browse: "Browse…",
    importPreview: "Preview",
    importSkip: "Don't import",
    importBack: "Back",
    importDo: "Import",
    importNoConfig: "There is no config.txt in this folder: {0}",
    importInvalid: "config.txt is not a valid settings file ({1}): {0}",
    importUnreadable: "config.txt cannot be read ({1}): {0}",
    importSource: "Source: {0}",
    importCounts: "{0} to import, {1} not usable, {2} left out",
    importReportCounts: "{0} imported, {1} not usable, {2} left out",
    importEngines: "Engines",
    importEngineName: "Name",
    importEngineCommand: "Command",
    importEngineStatus: "Status",
    importEngineProblems: "Problems",
    importCopyCredentials: "Copy credentials",
    importUnmigratedFields: "Fields left out: {0}",
    importIncompatibleFields: "Fields that do not apply (LizzieYzy Next's management of its own engine package): {0}",
    importProgramFolder: "LizzieYzy Next program folder",
    importRepreview: "Preview again",
    importReplaces: "These settings were changed in LizzieYzy Tauri and the import replaces them: {0}",
    importReplaceConfirm: "Replace these settings",
    importDone: "Import finished",
    importFixLater: "Engines with problems can be changed in \"{0}\".",
    importStale: "This preview is no longer valid. Preview again.",
    importNeedsReplace: "The import replaces changed settings. Tick \"Replace these settings\" first.",
    importListImported: "Imported settings",
    importListIncompatible: "Settings that cannot be used",
    importListNotMigrated: "Settings left out",
    statusImported: "imported",
    statusIncompatible: "not usable",
    statusNotMigrated: "left out",
    reasonNotYetAvailable: "its feature is not in LizzieYzy Tauri yet",
    reasonCredential: "holds an account or credentials; not read",
    reasonInvalidValue: "invalid value",
    reasonWindowCoordinates: "Java window positions follow Java's scaling and layout, not this window's",
    reasonRemoteEngine: "remote (SSH) engines are not available yet",
    reasonCredentialNotConfirmed: "the command holds credentials; copying was not confirmed",
    reasonUnreadable: "the file cannot be read",
    problemProgramNotFound: "program not found: {0}",
    problemFileNotFound: "file not found: {0} {1}",
    problemEmptyCommand: "empty command",
    problemInvalidValue: "{0} is invalid; the default is used",
    targetReadKomi: "Load komi from records",
    targetLoadSgfLast: "Go to the last move after opening",
    targetReplaceFileHint: "Show the replace-file prompt",
    targetLanguage: "Interface language",
    targetPlaySound: "Stone sounds",
    targetRecentFiles: "Recent records",
    targetLastFolder: "Last folder",
    targetEngines: "Engine list and default engine",
    targetEngineStartup: "Engine loaded at start",
    targetLastEngine: "Engine used last",
    targetAnalysisInterval: "Analysis update interval",
    targetLimitTime: "Limit analysis time",
    targetMaxAnalyzeTime: "Maximum analysis time (s)",
    targetLimitPlayout: "Limit playouts",
    targetLimitPlayouts: "Maximum playouts",
    targetLimitMaxSuggestion: "Maximum candidates shown",
    targetLimitBranchLength: "Maximum variation length",
    targetFastEngineChange: "Fast engine change",
    targetPonderLimitTips: "Notice when an analysis limit is reached",
    targetKataRules: "KataGo rules",
    targetAutoLoadKataRules: "Load KataGo rules automatically",
    targetStartupBenchmark: "Benchmark at start-up",
    targetNetworkProxy: "Network proxy",
  },
};

/** Messages that exist only in this product (capability not migrated yet) and the JDK option-pane
 *  button labels Java takes from the platform. Other languages fall back to English. */
const PRODUCT: Record<string, Table> = {
  ZhCn: { ...IMPORT_TEXT.ZhCn, unavailable: "此功能尚未在 LizzieYzy Tauri 中提供", diagnosticsLogs: "app.log：{0}，crash.log：{1}", none: "无", ok: "确定", cancel: "取消", close: "关闭", yes: "是", no: "否", consoleBoardCommand: "棋谱管理当前局面，不能通过 GTP 控制台改变棋盘。请使用棋盘操作。" },
  ZhTw: { ...IMPORT_TEXT.ZhTw, unavailable: "此功能尚未在 LizzieYzy Tauri 中提供", diagnosticsLogs: "app.log：{0}，crash.log：{1}", none: "無", ok: "確定", cancel: "取消", close: "關閉", yes: "是", no: "否", consoleBoardCommand: "棋譜管理目前局面，不能透過 GTP 控制台改變棋盤。請使用棋盤操作。" },
  EnUs: { ...IMPORT_TEXT.EnUs, unavailable: "Not available in LizzieYzy Tauri yet", diagnosticsLogs: "app.log: {0}, crash.log: {1}", none: "none", ok: "OK", cancel: "Cancel", close: "Close", yes: "Yes", no: "No", consoleBoardCommand: "The game record owns the position. Use board controls instead of a board-changing GTP console command." },
};

let lang: Exclude<Language, "System"> = "ZhCn";

export function setLanguage(l: Language): void {
  lang = l === "System" ? "ZhCn" : l;
}

export function language(): Language {
  return lang;
}

/** Java `Lizzie.isChinese` (bundle value "yes"). */
export function isChinese(): boolean {
  return t("Lizzie.isChinese") === "yes";
}

/** Java resource string; unknown keys are shown literally (Java uses literal text for some items). */
export function t(key: string): string {
  return JAVA[lang]?.[key] ?? JAVA.ZhCn?.[key] ?? key;
}

/** MessageFormat-style `{0}` substitution. */
export function format(text: string, ...args: (string | number)[]): string {
  return text.replace(/\{(\d+)\}/g, (_, k) => `${args[Number(k)] ?? ""}`);
}

/** Product-only messages of a feature module, kept next to that feature; same language rules as
 *  PRODUCT. Later registrations do not replace existing keys. */
export function registerProduct(tables: Record<string, Table>): void {
  for (const [language, table] of Object.entries(tables)) {
    PRODUCT[language] = { ...table, ...PRODUCT[language] };
  }
}

export function product(key: string, ...args: (string | number)[]): string {
  const table = PRODUCT[lang] ?? PRODUCT.EnUs;
  return format(table[key] ?? PRODUCT.EnUs[key] ?? key, ...args);
}
