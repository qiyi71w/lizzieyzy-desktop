// ConfigDialog2.locateSetting / applySettingNavigation / highlightSettingRow (Java cb53acc).
import { state } from "../app";
import { dialogOpen } from "../dialogs";
import { openAnalysisSettings, SETTINGS_PAGES, type SettingsSection } from "./settings";

// Only actual product rows belong here. A catalog entry alone never makes a setting available.
const targets = new Map<string, SettingsSection>([
  ...[
    "config.kifu.auto-analyze", "config.kifu.jump-last", "config.kifu.read-komi",
  ].map((id): [string, SettingsSection] => [id, "kifu"]),
  ...[
    "config.analysis.winrate", "settings.black-winrate", "config.analysis.variation", "config.analysis.blunder-bar", "config.analysis.hover",
    "config.analysis.graph-fill", "config.analysis.max-red", "config.analysis.suggestion-limit",
    "config.analysis.variation-limit", "config.analysis.limit-time", "config.analysis.limit-visits",
    "config.candidates.blue-ring", "config.candidates.color-ratio", "config.candidates.white-style",
    "config.candidates.no-suggestion-circle", "config.candidates.minimum-playout-ratio",
    "config.pv.mode", "config.pv.visit-limit", "config.pv.remove-dead",
  ].map((id): [string, SettingsSection] => [id, "engine"]),
  ...[
    "config.play.double-click", "config.play.drag-stone", "config.play.comment-panel", "config.play.hide-panel-controls",
    "config.play.coordinates", "config.play.freeze-sub-board", "config.interaction.right-click",
    "config.interaction.coordinates",
  ].map((id): [string, SettingsSection] => [id, "play"]),
  ...[
    "config.advanced.fast-switch", "config.advanced.startup-benchmark", "config.advanced.no-capture",
    "config.proxy.mode", "config.proxy.manual-address", "config.engine.update-interval",
  ].map((id): [string, SettingsSection] => [id, "advanced"]),
]);

/** Section owners register the Java IDs of the real data-setting-target rows they build.
 *  Registration is metadata only: availability must not build a page or edit a draft. */
export function registerSettingTargets(section: SettingsSection, ids: readonly string[]): void {
  for (const id of ids) targets.set(id, section);
}

/** Whether this product can build the target's real row, without opening or editing settings. */
export function settingTargetAvailable(target: string): boolean {
  if (!state.view) return false;
  const section = targets.get(target);
  return section !== undefined && (section === "engine" || section === "kifu" || section === "play"
    || section === "advanced" || SETTINGS_PAGES[section] !== undefined);
}

let clearHighlight: (() => void) | undefined;

/** Opens 综合设置 at the actual control, without changing it or applying the draft.
 *  Resolves when the location is shown, not when the user later closes settings. */
export function openSettingsAt(target: string): Promise<boolean> {
  if (!settingTargetAvailable(target) || dialogOpen()) return Promise.resolve(false);
  const section = targets.get(target)!;
  const { promise, resolve, reject } = Promise.withResolvers<boolean>();
  void openAnalysisSettings(section, (body) => {
    const row = [...body.querySelectorAll<HTMLElement>("[data-setting-target]")]
      .find((element) => element.dataset.settingTarget === target);
    if (!row) { resolve(false); return; }
    requestAnimationFrame(() => {
      if (!row.isConnected) { resolve(false); return; }
      clearHighlight?.();
      const outline = row.style.outline;
      const offset = row.style.outlineOffset;
      row.style.outline = "2px solid #B99C5D";
      row.style.outlineOffset = "2px";
      const clear = () => {
        clearTimeout(timer);
        row.style.outline = outline;
        row.style.outlineOffset = offset;
        if (clearHighlight === clear) clearHighlight = undefined;
      };
      const timer = window.setTimeout(clear, 2400);
      clearHighlight = clear;
      row.scrollIntoView({ block: "nearest", inline: "nearest" });
      const control = [...row.querySelectorAll<HTMLElement>(
        "input:not(:disabled):not([type='hidden']), select:not(:disabled), textarea:not(:disabled), button:not(:disabled), [tabindex]",
      )].find((element) => element.tabIndex >= 0 && !element.hidden && element.getClientRects().length > 0);
      if (control) control.focus({ preventScroll: true });
      else { row.tabIndex = -1; row.focus({ preventScroll: true }); }
      resolve(true);
    });
  }).catch(reject);
  return promise;
}
