// Command catalog: the Java entries this client implements, keyed by the Java action id
// (Menu.java ACTION_PROPERTY / FunctionCatalog) or, for items without one, by their resource key.
// Menus, toolbars and shortcuts look entries up here; a Java entry with no catalog entry is shown
// disabled and marked as not yet available. Later slices register their own entries.
import { getCurrentWindow } from "@tauri-apps/api/window";
import { navigate, pasteRecord, run, state } from "./app";
import type { Language } from "./contract/generated";
import { confirm, message } from "./dialogs";
import { openDiagnostics } from "./diagnostics";
import { t } from "./i18n";
import { openMoreEngines } from "./engine/catalog";
import { consoleVisible, toggleConsole } from "./engine/console";
import { openRules } from "./engine/rules";
import { openAnalysisSettings } from "./engine/settings";
import "./engine/events";

export interface CommandEntry {
  run: () => void;
  checked?: () => boolean;
  /** A Java-faithful disabled state (not "unimplemented"). */
  enabled?: () => boolean;
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

export const COMMANDS: Record<string, CommandEntry> = {
  "analysis.toggle": { run: () => run({ type: "ToggleAnalysis" }) },
  "engine.configuration": { run: openMoreEngines },
  "engine.rules": { run: openRules },
  "settings.general": { run: openAnalysisSettings },
  "menu.gtpPanel": { run: toggleConsole, checked: consoleVisible },
  "menu.restartCurrentEngine": { run: () => run({ type: "RestartEngine" }), enabled: () => !!state.view?.engine.foreground },
  "menu.shutdownCurrentEngine": { run: () => run({ type: "ShutdownEngines", scope: "Current" }), enabled: () => !!state.view?.engine.foreground },
  "menu.shutdownOtherEngine": { run: () => run({ type: "ShutdownEngines", scope: "Others" }) },
  "menu.shutdownAllEngine": { run: () => run({ type: "ShutdownEngines", scope: "All" }) },
  "files.open": { run: () => run({ type: "ChooseAndOpenRecord" }) },
  "menu.pasteSgf": { run: pasteRecord },
  "menu.loadKomi": {
    run: () => run({ type: "SetReadKomi", enabled: !state.view?.settings.read_komi }),
    checked: () => state.view?.settings.read_komi ?? true,
  },
  "menu.playSound": {
    run: () => run({ type: "SetPlaySound", enabled: !state.view?.settings.play_sound }),
    checked: () => state.view?.settings.play_sound ?? true,
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
  "menu.language.default": language("System"),
  简体中文: language("ZhCn"),
  繁體中文: language("ZhTw"),
  English: language("EnUs"),
  日本語: language("JaJp"),
  한국어: language("KoKr"),
  ไทย: language("ThTh"),
};
