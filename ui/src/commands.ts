// Command catalog: the Java entries this client implements, keyed by the Java action id
// (Menu.java ACTION_PROPERTY / FunctionCatalog) or, for items without one, by their resource key.
// Menus, toolbars and shortcuts look entries up here; a Java entry with no catalog entry is shown
// disabled and marked as not yet available. Later slices register their own entries.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { navigate, pasteRecord, run, saveRecord, saveRecordAs, state } from "./app";
import type { DisplaySettings, Language } from "./contract/generated";
import { confirm, message } from "./dialogs";
import { openDiagnostics } from "./diagnostics";
import { openJavaImport } from "./javaImport";
import { t } from "./i18n";
import { openMoreEngines } from "./engine/catalog";
import { consoleVisible, toggleConsole } from "./engine/console";
import { openRules } from "./engine/rules";
import { openAnalysisSettings } from "./engine/settings";
import { openAutoSetup } from "./setup/dialog";
import "./engine/events";

export interface CommandEntry {
  run: () => void;
  checked?: () => boolean;
  /** A Java-faithful disabled state (not "unimplemented"). */
  enabled?: () => boolean;
  /** Java items hidden by a menu-open listener (setVisible(false)) while this is false. */
  visible?: () => boolean;
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

function setDisplay(change: Partial<DisplaySettings>): void {
  const settings = state.view?.settings.display;
  if (settings) void run({ type: "SetDisplaySettings", settings: { ...settings, ...change } });
}

export const COMMANDS: Record<string, CommandEntry> = {
  "analysis.toggle": { run: () => run({ type: "ToggleAnalysis" }) },
  "engine.configuration": { run: openMoreEngines },
  "engine.setup": { run: () => openAutoSetup("overview") },
  "weights.download": { run: () => openAutoSetup("weights") },
  "engine.acceleration": { run: () => openAutoSetup("acceleration") },
  "engine.rules": { run: openRules },
  "settings.general": { run: openAnalysisSettings },
  "menu.gtpPanel": { run: toggleConsole, checked: consoleVisible },
  "menu.restartCurrentEngine": { run: () => run({ type: "RestartEngine" }), enabled: () => !!state.view?.engine.foreground },
  "menu.shutdownCurrentEngine": { run: () => run({ type: "ShutdownEngines", scope: "Current" }), enabled: () => !!state.view?.engine.foreground },
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
  "menu.stopFullTrace": { run: () => run({ type: "SetFullTrace", enabled: false }), enabled: () => state.view?.engine.full_trace ?? false },
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
  "menu.language.default": language("System"),
  简体中文: language("ZhCn"),
  繁體中文: language("ZhTw"),
  English: language("EnUs"),
  日本語: language("JaJp"),
  한국어: language("KoKr"),
  ไทย: language("ThTh"),
};
