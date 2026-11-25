// Command catalog: the Java entries this client implements, keyed by the Java action id
// (Menu.java ACTION_PROPERTY / FunctionCatalog) or, for items without one, by their resource key.
// Menus, toolbars and shortcuts look entries up here; a Java entry with no catalog entry is shown
// disabled and marked as not yet available. Later slices register their own entries.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { navigate, pasteRecord, run, saveRecord, saveRecordAs, state } from "./app";
import type { DisplaySettings, Language } from "./contract/generated";
import type { IconName } from "./design/icons";
import { confirm, message } from "./dialogs";
import { openDiagnostics } from "./diagnostics";
import { openJavaImport } from "./javaImport";
import { t } from "./i18n";
import { openMoreEngines } from "./engine/catalog";
import { consoleVisible, toggleConsole } from "./engine/console";
import { openRules } from "./engine/rules";
import { openEngineParameters } from "./engine/kataParams";
import { openAnalysisSettings } from "./engine/settings";
import { openAutoSetup } from "./setup/dialog";
import { openRemoteCompute } from "./cloud/center";
import "./engine/events";
import { openContributionSettings } from "./contribution/settings";
import "./contribution/window";

export interface CommandEntry {
  run: () => void;
  checked?: () => boolean;
  /** A Java-faithful disabled state (not "unimplemented"). */
  enabled?: () => boolean;
  /** Why the item cannot be used now, as this product's message key (`product(key)`), or null when
   *  it can. A reason disables the item; the window's menus show it as the item's tooltip and
   *  function search gives the same text. The system menu bar only greys the item. */
  disabledReason?: () => string | null;
  /** Java items hidden by a menu-open listener (setVisible(false)) while this is false. */
  visible?: () => boolean;
  /** Java items whose text a menu-open listener sets (启动/停止 Web 旁观, Menu.java:4301-4309). */
  label?: () => string;
  /** A button inside the menu item, beside its label (自定义布局's 设置, Menu.java:1985-2050).
   *  With `icon` it shows the icon, and `label` is its name and tooltip. `item` names it where a
   *  menu holds no buttons and it is an item of its own (the macOS menu bar); `label` by default. */
  button?: { label: () => string; run: () => void; icon?: IconName; item?: () => string };
}

/** Settings > 语言 (Menu.java:7025-7034): saved now, applied after a restart. */
function language(lang: Language): CommandEntry {
  return {
    run: async () => {
      const r = await run({ type: "SetLanguage", language: lang });
      if (r) message("LizzieYzy Tauri", t("Lizzie.hint.restart"));
    },
    checked: () => state.view?.settings.language === lang,
  };
}

/** One change to `settings.display`, the rest as it is (SetDisplaySettings). */
export function setDisplay(change: Partial<DisplaySettings>): void {
  const settings = state.view?.settings.display;
  if (settings) void run({ type: "SetDisplaySettings", settings: { ...settings, ...change } });
}

export const COMMANDS: Record<string, CommandEntry> = {
  "analysis.toggle": { run: () => run({ type: "ToggleAnalysis" }) },
  // 分析 > 清除Lizzie缓存 / 清除分析信息 (Menu.java:3533-3583); Shift+A is the 全局 cache item.
  "menu.clearAllLizzieCache": { run: () => run({ type: "ClearAnalysisCache", scope: "All" }) },
  "menu.clearThisLizzieCache": { run: () => run({ type: "ClearAnalysisCache", scope: "Current" }) },
  "menu.clearAllLizzieBestmoves": { run: () => run({ type: "ClearAnalysisInfo", scope: "All" }) },
  "menu.clearThisLizzieBestmoves": { run: () => run({ type: "ClearAnalysisInfo", scope: "Current" }) },
  "engine.configuration": { run: openMoreEngines },
  "engine.setup": { run: () => openAutoSetup("overview") },
  "weights.download": { run: () => openAutoSetup("weights") },
  "engine.acceleration": { run: () => openAutoSetup("acceleration") },
  "engine.rules": { run: openRules },
  // 设置 > 引擎参数(Alt+D) and the toolbar's 参数 (Menu.java:5355-5363, 6136-6143).
  "engine.parameters": { run: () => void openEngineParameters() },
  // 设置 > 远程算力 (Menu.java:5334-5342).
  "engine.remote": { run: openRemoteCompute },
  // 综合设置 opens at its first page, 棋盘与显示 (Menu.java:5395-5399, ConfigDialog2.java:4168-4171).
  "settings.general": { run: () => void openAnalysisSettings("display") },
  "menu.gtpPanel": { run: toggleConsole, checked: consoleVisible },
  // An engine that ended and was not started again is still the current engine (Java `reStartEngine`).
  "menu.restartCurrentEngine": { run: () => run({ type: "RestartEngine" }), enabled: () => !!state.view?.engine.foreground || !!state.view?.engine.down },
  "menu.shutdownCurrentEngine": { run: () => run({ type: "ShutdownEngines", scope: "Current" }), enabled: () => !!state.view?.engine.foreground },
  // 双引擎模式's engine menu (Menu.java:5857-5876): reStartEngine2 / killThisEngines2.
  "menu.restartCurrentEngine2": { run: () => run({ type: "RestartSecondaryEngine" }), enabled: () => !!state.view?.engine.secondary },
  "menu.shutdownCurrentEngine2": { run: () => run({ type: "ShutdownSecondaryEngine" }), enabled: () => !!state.view?.engine.secondary },
  "menu.shutdownOtherEngine": { run: () => run({ type: "ShutdownEngines", scope: "Others" }) },
  "menu.shutdownAllEngine": { run: () => run({ type: "ShutdownEngines", scope: "All" }) },
  "files.open": { run: () => run({ type: "ChooseAndOpenRecord" }) },
  "files.save": { run: saveRecord },
  "files.save-as": { run: saveRecordAs },
  "menu.showReplaceFileConfirmDialog": {
    run: () => run({ type: "SetReplaceFileHint", enabled: !state.view?.settings.replace_file_hint }),
    checked: () => state.view?.settings.replace_file_hint ?? true,
  },
  "menu.pasteSgf": { run: pasteRecord },
  "menu.loadKomi": {
    run: () => run({ type: "SetReadKomi", enabled: !state.view?.settings.read_komi }),
    checked: () => state.view?.settings.read_komi ?? true,
  },
  "menu.playSound": {
    run: () => run({ type: "SetPlaySound", enabled: !state.view?.settings.play_sound }),
    checked: () => state.view?.settings.play_sound ?? true,
  },
  // 显示 > 推荐点 > 总是显示黑胜率 (Menu.java:1487-1500) and 显示 > KataGo设置 > 目差显示 (2130-2158):
  // "目差" turns show-katago-score-lead-with-komi off, "盘面" on. The core converts every number.
  "settings.black-winrate": {
    run: () => setDisplay({ winrateAlwaysBlack: !state.view?.settings.display.winrateAlwaysBlack }),
    checked: () => state.view?.settings.display.winrateAlwaysBlack ?? false,
  },
  "Menu.leadWithKomi": {
    run: () => setDisplay({ scoreLeadWithKomi: false }),
    checked: () => !(state.view?.settings.display.scoreLeadWithKomi ?? false),
  },
  "Menu.leadJustScore": {
    run: () => setDisplay({ scoreLeadWithKomi: true }),
    checked: () => state.view?.settings.display.scoreLeadWithKomi ?? false,
  },
  // System.exit(0): no window state is written.
  "menu.forceExit": { run: () => getCurrentWindow().destroy() },
  "menu.exit": { run: () => getCurrentWindow().close() },
  "menu.deletePersistFile": {
    run: async () => {
      if (await run({ type: "ResetUiPosition" })) message("LizzieYzy Tauri", t("Config.deletePersistFile"));
    },
  },
  "menu.backToMainBranch": { run: () => navigate({ op: "MainTrunk" }) },
  "menu.jumpToFirst": { run: () => navigate({ op: "First" }) },
  "menu.jumpToLast": { run: () => navigate({ op: "Last" }) },
  "menu.jumpToLeft": { run: () => navigate({ op: "PreviousBranch" }) },
  "menu.jumpToRight": { run: () => navigate({ op: "NextBranch" }) },
  "menu.diagnosticsAndLogs": { run: openDiagnostics },
  "menu.stopFullTrace": { run: () => run({ type: "SetFullTrace", enabled: false }), enabled: () => (state.view?.full_trace.length ?? 0) > 0 },
  "menu.clearUserData": {
    run: async () => {
      const ok = await confirm(t("Menu.clearAllPersonalData.confirmTitle"), t("Menu.clearAllPersonalData.confirmMessage"));
      if (!ok) return;
      if (await run({ type: "ClearPersonalData" })) {
        message(t("Menu.clearAllPersonalData.doneTitle"), t("Menu.clearAllPersonalData.doneMessage"));
      }
    },
  },
  "product.javaImport": { run: () => openJavaImport() },
  // 设置 > 显示跑谱贡献菜单 and the 跑谱贡献 menu (Menu.java:5558-5575, 5636-5680).
  "menu.showContribute": {
    run: () => run({ type: "Contribution", op: { type: "SetShowMenu", enabled: !(state.view?.settings.contribution.showMenu ?? true) } }),
    checked: () => state.view?.settings.contribution.showMenu ?? true,
  },
  "menu.kataGoDistributedTraining": { run: () => run({ type: "Contribution", op: { type: "Start" } }) },
  "menu.kataGoTrainingSettings": { run: () => void openContributionSettings() },
  "menu.kataGoOfficialWebsite": { run: () => run({ type: "Contribution", op: { type: "OpenLink", link: "OfficialWebsite" } }) },
  "menu.language.default": language("System"),
  简体中文: language("ZhCn"),
  繁體中文: language("ZhTw"),
  English: language("EnUs"),
  日本語: language("JaJp"),
  한국어: language("KoKr"),
  ไทย: language("ThTh"),
};
