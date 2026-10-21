// ConfigDialog2.java:6701-6711, 6851-6854 and modern engine/analysis section.
import { run, state } from "../app";
import type { AnalysisSettings, ProxyMode } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button, checkbox, field, input } from "./controls";
import { resourceCommand, resourceView } from "../setup/dialog";

export async function openAnalysisSettings(): Promise<void> {
  if (!state.view) return;
  const settings = { ...state.view.settings.engine.analysis };
  const network = { ...(resourceView()?.network ?? { proxyMode: "Direct" as const, proxyHost: "127.0.0.1", proxyPort: 7897 }) };
  const startupBenchmark = state.view.settings.engine.startup_benchmark;
  const body = document.createElement("div"); body.className = "engine-settings";
  const nav = document.createElement("nav");
  const form = document.createElement("div"); form.className = "engine-settings-form";
  const advanced = document.createElement("div"); advanced.className = "engine-settings-form"; advanced.hidden = true;
  for (const [key, text] of [["display", t("ConfigDialog2.modern.nav.display")], ["kifu", t("ConfigDialog2.modern.nav.kifu")], ["engine", t("ConfigDialog2.modern.nav.engine")], ["play", t("ConfigDialog2.modern.nav.play")], ["theme", t("ConfigDialog2.modern.nav.theme")], ["advanced", t("ConfigDialog2.modern.nav.advanced")], ["about", t("ConfigDialog2.modern.nav.about")]]) {
    const item = document.createElement("button"); item.textContent = text; item.dataset.key = `settings.nav.${key}`;
    item.disabled = key !== "engine" && key !== "advanced";
    if (item.disabled) { item.title = product("unavailable"); item.className = "unavailable"; }
    else item.onclick = () => { form.hidden = key !== "engine"; advanced.hidden = key !== "advanced"; };
    nav.append(item);
  }
  body.append(nav);
  const unavailable = document.createElement("small"); unavailable.className = "engine-unavailable-note";
  unavailable.textContent = product("unavailable"); nav.append(unavailable);
  const fields: [keyof AnalysisSettings, string, number][] = [
    ["intervalCentisec", t("ConfigDialog2.modern.engineHealth.interval"), settings.intervalCentisec],
    ["maxAnalyzeTimeSeconds", t("ConfigDialog2.modern.analysis.limitTime"), settings.maxAnalyzeTimeSeconds],
    ["limitPlayouts", t("ConfigDialog2.modern.analysis.limitVisits"), settings.limitPlayouts],
    ["limitMaxSuggestion", t("ConfigDialog2.modern.analysis.suggestionLimit"), settings.limitMaxSuggestion],
    ["limitBranchLength", t("ConfigDialog2.modern.analysis.variationLimit"), settings.limitBranchLength],
  ];
  const inputs: HTMLInputElement[] = [];
  for (const [key, label, value] of fields) {
    const control = input(value, "number"); control.dataset.key = `settings.analysis.${key}`; control.min = "0"; control.step = "1"; control.required = true;
    control.oninput = () => { Object.assign(settings, { [key]: Number(control.value) }); };
    field(form, label, control); inputs.push(control);
  }
  const time = checkbox(t("ConfigDialog2.modern.analysis.limitTime"), settings.limitTime); time.input.dataset.key = "settings.analysis.limitTime";
  const visits = checkbox(t("ConfigDialog2.modern.analysis.limitVisits"), settings.limitPlayout); visits.input.dataset.key = "settings.analysis.limitPlayout";
  const fast = checkbox(t("ConfigDialog2.modern.advanced.fastSwitch"), settings.fastEngineChange); fast.input.dataset.key = "settings.analysis.fastEngineChange";
  form.append(time.label, visits.label, fast.label);
  const heading = document.createElement("h3"); heading.textContent = t("ConfigDialog2.modern.advanced.title");
  const subtitle = document.createElement("p"); subtitle.textContent = t("ConfigDialog2.modern.advanced.subtitle"); advanced.append(heading, subtitle);
  const benchmark = checkbox(t("ConfigDialog2.modern.advanced.firstBenchmark"), startupBenchmark); benchmark.input.dataset.key = "settings.startupBenchmark"; benchmark.label.title = t("ConfigDialog2.modern.advanced.firstBenchmarkSub"); advanced.append(benchmark.label);
  const benchmarkHint = document.createElement("p"); benchmarkHint.textContent = t("ConfigDialog2.modern.advanced.firstBenchmarkSub"); advanced.append(benchmarkHint);
  const proxyTitle = document.createElement("h3"); proxyTitle.textContent = t("ConfigDialog2.modern.proxy.title");
  const proxySubtitle = document.createElement("p"); proxySubtitle.textContent = t("ConfigDialog2.modern.proxy.subtitle"); advanced.append(proxyTitle, proxySubtitle);
  const mode = document.createElement("select"); mode.dataset.key = "settings.proxy.mode";
  for (const [value, text] of [["Direct", t("ConfigDialog2.modern.proxy.direct")], ["System", t("ConfigDialog2.modern.proxy.system")], ["Manual", t("ConfigDialog2.modern.proxy.manualMode")]] as const) {
    const option = document.createElement("option"); option.value = value; option.textContent = text; mode.append(option);
  }
  mode.value = network.proxyMode; field(advanced, t("ConfigDialog2.modern.proxy.title"), mode);
  const restart = document.createElement("p"); restart.textContent = t("ConfigDialog2.modern.proxy.restartHint"); advanced.append(restart);
  const host = input(network.proxyHost || "127.0.0.1"); host.dataset.key = "settings.proxy.host";
  const port = input(network.proxyPort || 7897, "number"); port.dataset.key = "settings.proxy.port"; port.min = "1"; port.max = "65535"; port.step = "1";
  const address = document.createElement("div"); address.className = "engine-actions";
  field(address, t("ConfigDialog2.modern.proxy.host"), host); field(address, t("ConfigDialog2.modern.proxy.port"), port);
  const manualRow = document.createElement("div"); manualRow.className = "engine-field";
  const manualTitle = document.createElement("span"); manualTitle.textContent = t("ConfigDialog2.modern.proxy.manual"); manualRow.append(manualTitle, address); advanced.append(manualRow);
  const toggleManual = () => { host.disabled = port.disabled = mode.value !== "Manual"; }; mode.onchange = toggleManual; toggleManual();
  body.append(form, advanced);
  const dialog = formPanel(t("Menu.comprehensiveSettings"), body);
  const save = button(t("SetKataRules.btnApply"), async () => {
    const invalid = inputs.find((control) => !control.checkValidity());
    if (invalid) { form.hidden = false; advanced.hidden = true; invalid.reportValidity(); return; }
    const proxyMode: ProxyMode = mode.value === "System" ? "System" : mode.value === "Manual" ? "Manual" : "Direct";
    const validPort = /^\d+$/.test(port.value) && Number(port.value) >= 1 && Number(port.value) <= 65535;
    if (proxyMode === "Manual" && (!host.value.trim() || !validPort)) {
      advanced.hidden = false; form.hidden = true;
      await message(t("ConfigDialog2.modern.proxy.title"), !host.value.trim() ? t("ConfigDialog2.modern.proxy.hostRequired") : t("ConfigDialog2.modern.proxy.invalidPort"), "warning");
      (!host.value.trim() ? host : port).focus(); return;
    }
    settings.limitTime = time.input.checked; settings.limitPlayout = visits.input.checked; settings.fastEngineChange = fast.input.checked;
    const proxy = { proxyMode, proxyHost: host.value.trim(), proxyPort: validPort ? Number(port.value) : 7897 };
    if (JSON.stringify(proxy) !== JSON.stringify(network)) {
      const result = await resourceCommand({ type: "SetNetworkProxy", settings: proxy }); if (!result || result.type === "Refused") return;
    }
    if (benchmark.input.checked !== startupBenchmark) {
      const result = await run({ type: "Tuning", op: { type: "SetStartupBenchmark", enabled: benchmark.input.checked } }); if (!result || result.type === "Refused") return;
    }
    const result = await run({ type: "SetAnalysisSettings", settings });
    if (result && result.type !== "Refused") await dialog.close();
  }); save.dataset.key = "settings.apply";
  const cancel = button(t("SetKataRules.btnCancel"), () => dialog.close()); cancel.dataset.key = "settings.cancel";
  actions(body, save, cancel);
  await dialog.closed;
}
