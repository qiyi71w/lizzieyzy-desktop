// ConfigDialog2.java:6701-6711, 6851-6854 and the modern 棋谱加载 / 引擎与分析 / 对局与操作 / 高级 sections.
import { run, state } from "../app";
import type { AnalysisSettings, Command, ProxyMode } from "../contract/generated";
import { confirm, formPanel, message } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button, checkbox, field, input } from "./controls";
import { resourceCommand, resourceView } from "../setup/dialog";
import { buildDisplaySettings } from "../display/settingsPage";

export async function openAnalysisSettings(): Promise<void> {
  if (!state.view) return;
  const settings = { ...state.view.settings.engine.analysis };
  const network = { ...(resourceView()?.network ?? { proxyMode: "Direct" as const, proxyHost: "127.0.0.1", proxyPort: 7897 }) };
  const startupBenchmark = state.view.settings.engine.startup_benchmark;
  const body = document.createElement("div"); body.className = "engine-settings";
  const nav = document.createElement("nav");
  const form = document.createElement("div"); form.className = "engine-settings-form";
  const advanced = document.createElement("div"); advanced.className = "engine-settings-form"; advanced.hidden = true;
  const play = document.createElement("div"); play.className = "engine-settings-form"; play.hidden = true;
  const kifu = document.createElement("div"); kifu.className = "engine-settings-form"; kifu.hidden = true;
  const theme = document.createElement("div"); theme.className = "engine-settings-form"; theme.hidden = true;
  const pages = { engine: form, advanced, play, kifu, theme } as Record<string, HTMLElement>;
  const showPage = (key: string) => { for (const [id, page] of Object.entries(pages)) page.hidden = id !== key; };
  for (const [key, text] of [["display", t("ConfigDialog2.modern.nav.display")], ["kifu", t("ConfigDialog2.modern.nav.kifu")], ["engine", t("ConfigDialog2.modern.nav.engine")], ["play", t("ConfigDialog2.modern.nav.play")], ["theme", t("ConfigDialog2.modern.nav.theme")], ["advanced", t("ConfigDialog2.modern.nav.advanced")], ["about", t("ConfigDialog2.modern.nav.about")]]) {
    const item = document.createElement("button"); item.textContent = text; item.dataset.key = `settings.nav.${key}`;
    item.disabled = !(key in pages);
    if (item.disabled) { item.title = product("unavailable"); item.className = "unavailable"; }
    else item.onclick = () => showPage(key);
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
  // 引擎与分析 > 选点重点研究累计计算量目标 (ConfigDialog2.java:3041-3047, after 候选点最高值红色高亮).
  const tracking = { ...state.view.settings.tracking };
  const trackingVisits = input(tracking.maxVisits, "number"); trackingVisits.dataset.key = "settings.tracking.maxVisits"; trackingVisits.min = "0"; trackingVisits.step = "1"; trackingVisits.required = true;
  trackingVisits.title = t("ConfigDialog2.modern.analysis.trackingVisitsSub");
  field(form, t("ConfigDialog2.modern.analysis.trackingVisits"), trackingVisits); inputs.push(trackingVisits);
  // 主题 > 关注点质量外框 (ConfigDialog2.java:3923-3943).
  const outlineTitle = document.createElement("h3"); outlineTitle.textContent = t("ConfigDialog2.modern.trackingAppearance.title");
  const outlineSubtitle = document.createElement("p"); outlineSubtitle.textContent = t("ConfigDialog2.modern.trackingAppearance.subtitle"); theme.append(outlineTitle, outlineSubtitle);
  const outline = checkbox(t("ConfigDialog2.modern.trackingAppearance.outline"), tracking.showOutline); outline.input.dataset.key = "settings.tracking.showOutline"; outline.label.title = t("ConfigDialog2.modern.trackingAppearance.outlineSub"); theme.append(outline.label);
  const opacity = input(tracking.outlineOpacity, "range"); opacity.dataset.key = "settings.tracking.outlineOpacity"; opacity.min = "0"; opacity.max = "100"; opacity.step = "1";
  opacity.title = t("ConfigDialog2.modern.trackingAppearance.outlineOpacitySub");
  const opacityValue = document.createElement("span"); opacityValue.textContent = `${opacity.value}%`;
  opacity.oninput = () => { opacityValue.textContent = `${opacity.value}%`; };
  const opacityRow = document.createElement("span"); opacityRow.className = "engine-actions"; opacityRow.append(opacity, opacityValue);
  field(theme, t("ConfigDialog2.modern.trackingAppearance.outlineOpacity"), opacityRow);
  const time = checkbox(t("ConfigDialog2.modern.analysis.limitTime"), settings.limitTime); time.input.dataset.key = "settings.analysis.limitTime";
  const visits = checkbox(t("ConfigDialog2.modern.analysis.limitVisits"), settings.limitPlayout); visits.input.dataset.key = "settings.analysis.limitPlayout";
  const fast = checkbox(t("ConfigDialog2.modern.advanced.fastSwitch"), settings.fastEngineChange); fast.input.dataset.key = "settings.analysis.fastEngineChange";
  form.append(time.label, visits.label, fast.label);
  const displayRows = buildDisplaySettings(form, play);
  const heading = document.createElement("h3"); heading.textContent = t("ConfigDialog2.modern.advanced.title");
  const subtitle = document.createElement("p"); subtitle.textContent = t("ConfigDialog2.modern.advanced.subtitle"); advanced.append(heading, subtitle);
  const benchmark = checkbox(t("ConfigDialog2.modern.advanced.firstBenchmark"), startupBenchmark); benchmark.input.dataset.key = "settings.startupBenchmark"; benchmark.label.title = t("ConfigDialog2.modern.advanced.firstBenchmarkSub"); advanced.append(benchmark.label);
  const benchmarkHint = document.createElement("p"); benchmarkHint.textContent = t("ConfigDialog2.modern.advanced.firstBenchmarkSub"); advanced.append(benchmarkHint);
  // ConfigDialog2 高级与性能 (3271-3276): turning 五子棋无提子规则 on asks first; Cancel unchecks it.
  const edit = state.view.settings.edit;
  const noCapture = checkbox(t("ConfigDialog2.modern.advanced.noCapture"), edit.noCapture); noCapture.input.dataset.key = "settings.noCapture"; noCapture.label.title = t("ConfigDialog2.modern.advanced.noCaptureSub"); advanced.append(noCapture.label);
  noCapture.input.onchange = async () => {
    if (noCapture.input.checked && !(await confirm(t("ConfigDialog2.chkNoCapture.title"), t("ConfigDialog2.chkNoCapture.content"), "info"))) noCapture.input.checked = false;
  };
  const noCaptureHint = document.createElement("p"); noCaptureHint.textContent = t("ConfigDialog2.modern.advanced.noCaptureSub"); advanced.append(noCaptureHint);
  // 棋谱加载 > 打开 SGF 后行为 (ConfigDialog2.java:2956-2987): applied with 保存设置 (6641-6644).
  const sgfTitle = document.createElement("h3"); sgfTitle.textContent = t("ConfigDialog2.modern.kifu.title");
  const sgfSubtitle = document.createElement("p"); sgfSubtitle.textContent = t("ConfigDialog2.modern.kifu.subtitle"); kifu.append(sgfTitle, sgfSubtitle);
  const kifuRow = (field: string, title: string, sub: string, value: boolean) => {
    const row = checkbox(title, value); row.input.dataset.key = `settings.kifu.${field}`; row.label.title = sub;
    const hint = document.createElement("p"); hint.textContent = sub; kifu.append(row.label, hint);
    return row.input;
  };
  const loading = state.view.settings;
  const autoAnalyze = kifuRow("auto_quick_analyze_on_load", t("ConfigDialog2.modern.kifu.autoAnalyze"), t("ConfigDialog2.modern.kifu.autoAnalyzeSub"), loading.auto_quick_analyze_on_load);
  const jumpLast = kifuRow("load_sgf_last", t("ConfigDialog2.modern.kifu.jumpLast"), t("ConfigDialog2.modern.kifu.jumpLastSub"), loading.load_sgf_last);
  const readKomi = kifuRow("read_komi", t("ConfigDialog2.modern.kifu.readKomi"), t("ConfigDialog2.modern.kifu.readKomiSub"), loading.read_komi);
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
  body.append(form, kifu, play, theme, advanced);
  const dialog = formPanel(t("Menu.comprehensiveSettings"), body);
  const save = button(t("SetKataRules.btnApply"), async () => {
    const invalid = inputs.find((control) => !control.checkValidity());
    if (invalid) { showPage("engine"); invalid.reportValidity(); return; }
    const proxyMode: ProxyMode = mode.value === "System" ? "System" : mode.value === "Manual" ? "Manual" : "Direct";
    const validPort = /^\d+$/.test(port.value) && Number(port.value) >= 1 && Number(port.value) <= 65535;
    if (proxyMode === "Manual" && (!host.value.trim() || !validPort)) {
      showPage("advanced");
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
    if (noCapture.input.checked !== edit.noCapture) {
      const result = await run({ type: "SetEditSettings", settings: { ...edit, noCapture: noCapture.input.checked } }); if (!result || result.type === "Refused") return;
    }
    const now = state.view?.settings ?? loading;
    const kifuChanges: [boolean, Command][] = [
      [autoAnalyze.checked !== now.auto_quick_analyze_on_load, { type: "SetAutoQuickAnalyze", enabled: autoAnalyze.checked }],
      [jumpLast.checked !== now.load_sgf_last, { type: "SetLoadSgfLast", enabled: jumpLast.checked }],
      [readKomi.checked !== now.read_komi, { type: "SetReadKomi", enabled: readKomi.checked }],
    ];
    for (const [changed, command] of kifuChanges) {
      if (!changed) continue;
      const result = await run(command); if (!result || result.type === "Refused") return;
    }
    if (!(await displayRows.save())) return;
    const trackingNow = { maxVisits: Number(trackingVisits.value), showOutline: outline.input.checked, outlineOpacity: Number(opacity.value) };
    if (JSON.stringify(trackingNow) !== JSON.stringify(tracking)) {
      const result = await run({ type: "SetTrackingSettings", settings: trackingNow }); if (!result || result.type === "Refused") return;
    }
    const result = await run({ type: "SetAnalysisSettings", settings });
    if (result && result.type !== "Refused") await dialog.close();
  }); save.dataset.key = "settings.apply";
  const cancel = button(t("SetKataRules.btnCancel"), () => dialog.close()); cancel.dataset.key = "settings.cancel";
  actions(body, save, cancel);
  await dialog.closed;
}
