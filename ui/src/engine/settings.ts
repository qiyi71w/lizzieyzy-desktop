// ConfigDialog2.java:6701-6711, 6851-6854 and modern engine/analysis section.
import { run, state } from "../app";
import type { AnalysisSettings } from "../contract/generated";
import { formPanel } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button, checkbox, field, input } from "./controls";

export async function openAnalysisSettings(): Promise<void> {
  if (!state.view) return;
  const settings = { ...state.view.settings.engine.analysis };
  const body = document.createElement("div"); body.className = "engine-settings";
  const nav = document.createElement("nav");
  for (const text of [t("ConfigDialog2.modern.nav.display"), t("ConfigDialog2.modern.nav.kifu"), t("ConfigDialog2.modern.nav.engine"), t("ConfigDialog2.modern.nav.play"), t("ConfigDialog2.modern.nav.theme"), t("ConfigDialog2.modern.nav.advanced"), t("ConfigDialog2.modern.nav.about")]) {
    const item = document.createElement("button"); item.textContent = text;
    item.disabled = text !== t("ConfigDialog2.modern.nav.engine");
    if (item.disabled) { item.title = product("unavailable"); item.className = "unavailable"; }
    nav.append(item);
  }
  body.append(nav);
  const unavailable = document.createElement("small"); unavailable.className = "engine-unavailable-note";
  unavailable.textContent = product("unavailable"); nav.append(unavailable);
  const form = document.createElement("div"); form.className = "engine-settings-form";
  const fields: [keyof AnalysisSettings, string, number][] = [
    ["intervalCentisec", t("ConfigDialog2.modern.engineHealth.interval"), settings.intervalCentisec],
    ["maxAnalyzeTimeSeconds", t("ConfigDialog2.modern.analysis.limitTime"), settings.maxAnalyzeTimeSeconds],
    ["limitPlayouts", t("ConfigDialog2.modern.analysis.limitVisits"), settings.limitPlayouts],
    ["limitMaxSuggestion", t("ConfigDialog2.modern.analysis.suggestionLimit"), settings.limitMaxSuggestion],
    ["limitBranchLength", t("ConfigDialog2.modern.analysis.variationLimit"), settings.limitBranchLength],
  ];
  const inputs: HTMLInputElement[] = [];
  for (const [key, label, value] of fields) {
    const control = input(value, "number"); control.min = "0"; control.step = "1"; control.required = true;
    control.oninput = () => { Object.assign(settings, { [key]: Number(control.value) }); };
    field(form, label, control); inputs.push(control);
  }
  const time = checkbox(t("ConfigDialog2.modern.analysis.limitTime"), settings.limitTime);
  const visits = checkbox(t("ConfigDialog2.modern.analysis.limitVisits"), settings.limitPlayout);
  const fast = checkbox(t("ConfigDialog2.modern.advanced.fastSwitch"), settings.fastEngineChange);
  form.append(time.label, visits.label, fast.label);
  body.append(form);
  const dialog = formPanel(t("Menu.comprehensiveSettings"), body);
  actions(form, button(t("SetKataRules.btnApply"), async () => {
    if (!inputs.every((control) => control.reportValidity())) return;
    settings.limitTime = time.input.checked; settings.limitPlayout = visits.input.checked; settings.fastEngineChange = fast.input.checked;
    const result = await run({ type: "SetAnalysisSettings", settings });
    if (result && result.type !== "Refused") await dialog.close();
  }), button(t("SetKataRules.btnCancel"), () => dialog.close()));
  await dialog.closed;
}
