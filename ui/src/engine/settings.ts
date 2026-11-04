// ConfigDialog2.java:6701-6711, 6851-6854 and the modern 棋谱加载 / 引擎与分析 / 对局与操作 / 高级 sections
// (createDisplaySection 2955-3356). Every setting row carries its ConfigSettingTargets id as
// `data-setting-target`; edits stay a draft until 保存设置.
import { run, state } from "../app";
import type { AnalysisSettings, Command, ProxyMode } from "../contract/generated";
import { confirm, formPanel, message } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button, checkbox, field, input } from "./controls";
import { resourceCommand, resourceView } from "../setup/dialog";
import { buildDisplaySettings } from "../display/settingsPage";
import { beginPanelDraft } from "../workbench/panelDraft";

/** 综合设置's navigation sections in Java order (ConfigDialog2 `SettingSection` plus 关于). */
export type SettingsSection = "display" | "kifu" | "engine" | "play" | "theme" | "advanced" | "about";

/** A section page built by the module that owns its settings (显示, 主题). `save` applies the
 *  page's edits with 保存设置 and is false when they were invalid or the core refused them. */
export interface SettingsPage {
  save: () => Promise<boolean>;
}

/** What a page may ask of its dialog: Java's `setVisible(false); openConfigDialog2(tab)` after
 *  saveConfig (`saveAndReopen`, false when the save did not go through) or without it (`reopen`,
 *  every unsaved edit dropped). */
export interface SettingsDialog {
  saveAndReopen: (section: SettingsSection) => Promise<boolean>;
  reopen: (section: SettingsSection) => Promise<void>;
}

/** Section pages registered by their owners; a section without a page stays unavailable. */
export const SETTINGS_PAGES: Partial<Record<SettingsSection, (host: HTMLElement, dialog: SettingsDialog) => SettingsPage>> = {};

/** A row's description line, shown below it as in Java's design rows. */
function hint(text: string): HTMLParagraphElement {
  const p = document.createElement("p"); p.textContent = text; return p;
}

/** A check box row with its sub line as the tooltip and the hint below. */
function toggleRow(host: ParentNode, target: string, key: string, title: string, sub: string, value: boolean): HTMLInputElement {
  const row = checkbox(title, value); row.input.dataset.key = key; row.label.dataset.settingTarget = target; row.label.title = sub;
  host.append(row.label, hint(sub));
  return row.input;
}

function heading(host: ParentNode, title: string, subtitle: string): void {
  const h = document.createElement("h3"); h.textContent = title;
  const p = document.createElement("p"); p.textContent = subtitle; host.append(h, p);
}

/** `onOpened` runs once the dialog and its 保存设置 / 取消 controls are in place, before the
 *  dialog closes; it may locate rows but must not change the draft. */
export async function openAnalysisSettings(section: SettingsSection = "engine", onOpened?: (body: HTMLElement) => void): Promise<void> {
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
  const pages = { engine: form, advanced, play, kifu } as Record<string, HTMLElement>;
  const panels = beginPanelDraft();
  const registered: SettingsPage[] = [];
  const owner: SettingsDialog = {
    saveAndReopen: async (target) => {
      if (!(await apply())) return false;
      void openAnalysisSettings(target);
      return true;
    },
    reopen: async (target) => {
      await dialog.close();
      void openAnalysisSettings(target);
    },
  };
  for (const [key, build] of Object.entries(SETTINGS_PAGES)) {
    const page = document.createElement("div"); page.className = "engine-settings-form"; page.hidden = true;
    pages[key] = page; registered.push(build(page, owner));
  }
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
  // Numeric analysis fields (addInputRow / addToggleInputRow): checked with 保存设置. A limit's
  // switch shares the row with its value, so locating the row reaches both.
  const inputs: HTMLInputElement[] = [];
  const inputRow = (host: ParentNode, target: string, title: string, sub: string, unit: string, control: HTMLInputElement, toggle?: HTMLInputElement) => {
    const row = document.createElement(toggle ? "div" : "label"); row.className = "engine-field"; row.dataset.settingTarget = target;
    const name = document.createElement("span"); name.textContent = title;
    const value = document.createElement("span"); value.className = "engine-unit";
    if (toggle) { toggle.title = sub; toggle.setAttribute("aria-label", title); value.append(toggle); }
    value.append(control, ` ${unit}`); row.append(name, value); host.append(row, hint(sub)); inputs.push(control);
  };
  const numberRow = (host: ParentNode, target: string, key: keyof AnalysisSettings, title: string, sub: string, unit: string, toggle?: HTMLInputElement) => {
    const control = input(settings[key] as number, "number"); control.dataset.key = `settings.analysis.${key}`; control.min = "0"; control.step = "1"; control.required = true;
    control.title = sub; control.setAttribute("aria-label", title);
    control.oninput = () => { Object.assign(settings, { [key]: Number(control.value) }); };
    inputRow(host, target, title, sub, unit, control, toggle);
  };
  // 分析与胜率曲线 after its display toggles (ConfigDialog2.java:3041-3061), then 实况分析限制 (3063-3086).
  const analysisRows = document.createDocumentFragment();
  // 选点评估计算量 (3041-3047): a 选点重点研究 setting, saved with SetTrackingSettings.
  const trackingVisits = input(state.view.settings.tracking.maxVisits, "number"); trackingVisits.dataset.key = "settings.tracking.maxVisits"; trackingVisits.min = "0"; trackingVisits.step = "1"; trackingVisits.required = true;
  trackingVisits.title = t("ConfigDialog2.modern.analysis.trackingVisitsSub"); trackingVisits.setAttribute("aria-label", t("ConfigDialog2.modern.analysis.trackingVisits"));
  inputRow(analysisRows, "config.analysis.tracking-visits", t("ConfigDialog2.modern.analysis.trackingVisits"), t("ConfigDialog2.modern.analysis.trackingVisitsSub"), t("ConfigDialog2.modern.unit.visits"), trackingVisits);
  numberRow(analysisRows, "config.analysis.suggestion-limit", "limitMaxSuggestion", t("ConfigDialog2.modern.analysis.suggestionLimit"), t("ConfigDialog2.modern.analysis.suggestionLimitSub"), t("ConfigDialog2.modern.unit.items"));
  numberRow(analysisRows, "config.analysis.variation-limit", "limitBranchLength", t("ConfigDialog2.modern.analysis.variationLimit"), t("ConfigDialog2.modern.analysis.variationLimitSub"), t("ConfigDialog2.modern.unit.moves"));
  heading(analysisRows, t("ConfigDialog2.modern.analysis.liveLimits"), t("ConfigDialog2.modern.analysis.liveLimitsSub"));
  const time = input("", "checkbox"); time.checked = settings.limitTime; time.dataset.key = "settings.analysis.limitTime";
  numberRow(analysisRows, "config.analysis.limit-time", "maxAnalyzeTimeSeconds", t("ConfigDialog2.modern.analysis.limitTime"), t("ConfigDialog2.modern.analysis.limitTimeSub"), t("ConfigDialog2.modern.unit.seconds"), time);
  const visits = input("", "checkbox"); visits.checked = settings.limitPlayout; visits.dataset.key = "settings.analysis.limitPlayout";
  numberRow(analysisRows, "config.analysis.limit-visits", "limitPlayouts", t("ConfigDialog2.modern.analysis.limitVisits"), t("ConfigDialog2.modern.analysis.limitVisitsSub"), t("ConfigDialog2.modern.unit.visits"), visits);
  // 对局与操作 (3162-3191) and 鼠标交互与坐标格式 > 右键行为 (3215-3220, 1430-1496, 6650-6657, 6819-6826).
  // The 点击复盘 / 落子矩形 rows are not part of this product's settings yet.
  const edit = state.view.settings.edit;
  const problems = state.view.settings.problems;
  const playRows = document.createDocumentFragment();
  const doubleClick = toggleRow(playRows, "config.play.double-click", "settings.edit.allowDoubleClick", t("ConfigDialog2.modern.play.doubleClick"), t("ConfigDialog2.modern.play.doubleClickSub"), edit.allowDoubleClick);
  const drag = toggleRow(playRows, "config.play.drag-stone", "settings.edit.allowDrag", t("ConfigDialog2.modern.play.drag"), t("ConfigDialog2.modern.play.dragSub"), edit.allowDrag);
  // chkShowComment is disabled while the layout mode hides the panel (ConfigDialog2.java:4270-4283).
  panels.row(playRows, "showComment", "config.play.comment-panel", t("ConfigDialog2.modern.play.commentPanel"), t("ConfigDialog2.modern.play.commentPanelSub"), state.view.settings.comment_panel_auto_hidden);
  // Checked hides the control bar (chkHideCommentControlPane -> hide-blunder-table-control-pane).
  const hideControls = toggleRow(playRows, "config.play.hide-panel-controls", "settings.problems.hideControlPane", t("ConfigDialog2.modern.play.hidePanelControls"), t("ConfigDialog2.modern.play.hidePanelControlsSub"), problems.hideControlPane);
  const interactionRows = document.createElement("div");
  const rightClick = document.createElement("select"); rightClick.dataset.key = "settings.edit.showRightMenu"; rightClick.title = t("ConfigDialog2.modern.interaction.rightClickSub");
  for (const [value, text] of [["menu", t("LizzieConfig.rdoRightClickMenu")], ["back", t("LizzieConfig.rdoRightClickBack")]]) {
    const option = document.createElement("option"); option.value = value; option.textContent = text; rightClick.append(option);
  }
  rightClick.value = edit.showRightMenu ? "menu" : "back";
  field(interactionRows, t("ConfigDialog2.modern.interaction.rightClick"), rightClick);
  (interactionRows.lastElementChild as HTMLElement).dataset.settingTarget = "config.interaction.right-click";
  const interaction = document.createDocumentFragment(); interaction.append(...interactionRows.childNodes, hint(rightClick.title));
  const displayRows = buildDisplaySettings(form, play, { analysis: analysisRows, play: playRows, interaction });
  // 高级与性能 (3238-3278): Java order 对局时后台计算, 快速切换, 首次启动智能测速, 五子棋无提子规则, 网络代理.
  heading(advanced, t("ConfigDialog2.modern.advanced.title"), t("ConfigDialog2.modern.advanced.subtitle"));
  // chkPonder (leelaz.play-ponder, ConfigDialog2.java:1898-1901, 6674): the 修改AI用时 setting.
  const ponder = toggleRow(advanced, "config.advanced.ponder", "settings.game.playPonder", t("ConfigDialog2.modern.advanced.ponder"), t("ConfigDialog2.modern.advanced.ponderSub"), state.view.settings.game.playPonder);
  const fast = toggleRow(advanced, "config.advanced.fast-switch", "settings.analysis.fastEngineChange", t("ConfigDialog2.modern.advanced.fastSwitch"), t("ConfigDialog2.modern.advanced.fastSwitchSub"), settings.fastEngineChange);
  const benchmark = toggleRow(advanced, "config.advanced.startup-benchmark", "settings.startupBenchmark", t("ConfigDialog2.modern.advanced.firstBenchmark"), t("ConfigDialog2.modern.advanced.firstBenchmarkSub"), startupBenchmark);
  // ConfigDialog2 高级与性能 (3271-3276): turning 五子棋无提子规则 on asks first; Cancel unchecks it.
  const noCapture = toggleRow(advanced, "config.advanced.no-capture", "settings.noCapture", t("ConfigDialog2.modern.advanced.noCapture"), t("ConfigDialog2.modern.advanced.noCaptureSub"), edit.noCapture);
  noCapture.onchange = async () => {
    if (noCapture.checked && !(await confirm(t("ConfigDialog2.chkNoCapture.title"), t("ConfigDialog2.chkNoCapture.content"), "info"))) noCapture.checked = false;
  };
  // 棋谱加载 > 打开 SGF 后行为 (ConfigDialog2.java:2956-2987): applied with 保存设置 (6641-6644).
  heading(kifu, t("ConfigDialog2.modern.kifu.title"), t("ConfigDialog2.modern.kifu.subtitle"));
  const kifuRow = (target: string, field: string, title: string, sub: string, value: boolean) => toggleRow(kifu, target, `settings.kifu.${field}`, title, sub, value);
  const loading = state.view.settings;
  const autoAnalyze = kifuRow("config.kifu.auto-analyze", "auto_quick_analyze_on_load", t("ConfigDialog2.modern.kifu.autoAnalyze"), t("ConfigDialog2.modern.kifu.autoAnalyzeSub"), loading.auto_quick_analyze_on_load);
  const jumpLast = kifuRow("config.kifu.jump-last", "load_sgf_last", t("ConfigDialog2.modern.kifu.jumpLast"), t("ConfigDialog2.modern.kifu.jumpLastSub"), loading.load_sgf_last);
  const readKomi = kifuRow("config.kifu.read-komi", "read_komi", t("ConfigDialog2.modern.kifu.readKomi"), t("ConfigDialog2.modern.kifu.readKomiSub"), loading.read_komi);
  // 网络代理 (addNetworkProxyRows 3503-3534): rows of the 高级与性能 card; the address row has no description.
  const mode = document.createElement("select"); mode.dataset.key = "settings.proxy.mode";
  for (const [value, text] of [["Direct", t("ConfigDialog2.modern.proxy.direct")], ["System", t("ConfigDialog2.modern.proxy.system")], ["Manual", t("ConfigDialog2.modern.proxy.manualMode")]] as const) {
    const option = document.createElement("option"); option.value = value; option.textContent = text; mode.append(option);
  }
  mode.value = network.proxyMode; field(advanced, t("ConfigDialog2.modern.proxy.title"), mode);
  (advanced.lastElementChild as HTMLElement).dataset.settingTarget = "config.proxy.mode";
  advanced.append(hint(t("ConfigDialog2.modern.proxy.subtitle")));
  const restart = document.createElement("p"); restart.textContent = t("ConfigDialog2.modern.proxy.restartHint"); advanced.append(restart);
  const host = input(network.proxyHost || "127.0.0.1"); host.dataset.key = "settings.proxy.host";
  const port = input(network.proxyPort || 7897, "number"); port.dataset.key = "settings.proxy.port"; port.min = "1"; port.max = "65535"; port.step = "1";
  const address = document.createElement("div"); address.className = "engine-actions";
  field(address, t("ConfigDialog2.modern.proxy.host"), host); field(address, t("ConfigDialog2.modern.proxy.port"), port);
  const manualRow = document.createElement("div"); manualRow.className = "engine-field"; manualRow.dataset.settingTarget = "config.proxy.manual-address";
  const manualTitle = document.createElement("span"); manualTitle.textContent = t("ConfigDialog2.modern.proxy.manual"); manualRow.append(manualTitle, address); advanced.append(manualRow);
  const toggleManual = () => { host.disabled = port.disabled = mode.value !== "Manual"; }; mode.onchange = toggleManual; toggleManual();
  // 日志与分析刷新 (3279-3310): only 本地分析刷新间隔 is a setting of this product.
  heading(advanced, t("ConfigDialog2.modern.engineHealth.title"), t("ConfigDialog2.modern.engineHealth.subtitle"));
  numberRow(advanced, "config.engine.update-interval", "intervalCentisec", t("ConfigDialog2.modern.engineHealth.interval"), t("ConfigDialog2.modern.engineHealth.intervalSub"), t("LizzieConfig.title.centisecond"));
  body.append(...Object.values(pages));
  showPage(section in pages ? section : "engine");
  const dialog = formPanel(t("Menu.comprehensiveSettings"), body);
  /** 保存设置 (saveConfig): true once everything was applied and the dialog closed. */
  const apply = async (): Promise<boolean> => {
    const invalid = inputs.find((control) => !control.checkValidity());
    if (invalid) { showPage(Object.keys(pages).find((id) => pages[id].contains(invalid)) ?? "engine"); invalid.reportValidity(); return false; }
    const proxyMode: ProxyMode = mode.value === "System" ? "System" : mode.value === "Manual" ? "Manual" : "Direct";
    const validPort = /^\d+$/.test(port.value) && Number(port.value) >= 1 && Number(port.value) <= 65535;
    if (proxyMode === "Manual" && (!host.value.trim() || !validPort)) {
      showPage("advanced");
      await message(t("ConfigDialog2.modern.proxy.title"), !host.value.trim() ? t("ConfigDialog2.modern.proxy.hostRequired") : t("ConfigDialog2.modern.proxy.invalidPort"), "warning");
      (!host.value.trim() ? host : port).focus(); return false;
    }
    settings.limitTime = time.checked; settings.limitPlayout = visits.checked; settings.fastEngineChange = fast.checked;
    const proxy = { proxyMode, proxyHost: host.value.trim(), proxyPort: validPort ? Number(port.value) : 7897 };
    if (JSON.stringify(proxy) !== JSON.stringify(network)) {
      const result = await resourceCommand({ type: "SetNetworkProxy", settings: proxy }); if (!result || result.type === "Refused") return false;
    }
    if (benchmark.checked !== startupBenchmark) {
      const result = await run({ type: "Tuning", op: { type: "SetStartupBenchmark", enabled: benchmark.checked } }); if (!result || result.type === "Refused") return false;
    }
    const nowGame = state.view?.settings ?? loading;
    if (ponder.checked !== nowGame.game.playPonder) {
      const result = await run({ type: "Game", op: { type: "SetAiTime", time: nowGame.game.aiTime, ponder: ponder.checked, autoplay: nowGame.autoplay.settings } });
      if (!result || result.type === "Refused") return false;
    }
    // The draft's rows over the currently committed settings; untouched fields keep their values.
    const committed = state.view?.settings ?? loading;
    const nextEdit = { ...committed.edit, noCapture: noCapture.checked, allowDoubleClick: doubleClick.checked, allowDrag: drag.checked, showRightMenu: rightClick.value === "menu" };
    if (JSON.stringify(nextEdit) !== JSON.stringify(committed.edit)) {
      const result = await run({ type: "SetEditSettings", settings: nextEdit }); if (!result || result.type === "Refused") return false;
    }
    const nowProblems = state.view?.settings.problems ?? problems;
    if (hideControls.checked !== nowProblems.hideControlPane) {
      const result = await run({ type: "SetProblemList", settings: { ...nowProblems, hideControlPane: hideControls.checked } }); if (!result || result.type === "Refused") return false;
    }
    const now = state.view?.settings ?? loading;
    const kifuChanges: [boolean, Command][] = [
      [autoAnalyze.checked !== now.auto_quick_analyze_on_load, { type: "SetAutoQuickAnalyze", enabled: autoAnalyze.checked }],
      [jumpLast.checked !== now.load_sgf_last, { type: "SetLoadSgfLast", enabled: jumpLast.checked }],
      [readKomi.checked !== now.read_komi, { type: "SetReadKomi", enabled: readKomi.checked }],
    ];
    for (const [changed, command] of kifuChanges) {
      if (!changed) continue;
      const result = await run(command); if (!result || result.type === "Refused") return false;
    }
    if (!(await displayRows.save())) return false;
    for (const page of registered) if (!(await page.save())) return false;
    if (!(await panels.save())) return false;
    // Over the committed tracking settings: the 主题 page may have just saved its outline rows.
    const nowTracking = state.view?.settings.tracking ?? loading.tracking;
    if (Number(trackingVisits.value) !== nowTracking.maxVisits) {
      const result = await run({ type: "SetTrackingSettings", settings: { ...nowTracking, maxVisits: Number(trackingVisits.value) } }); if (!result || result.type === "Refused") return false;
    }
    const result = await run({ type: "SetAnalysisSettings", settings });
    if (!result || result.type === "Refused") return false;
    await dialog.close();
    return true;
  };
  const save = button(t("SetKataRules.btnApply"), () => void apply()); save.dataset.key = "settings.apply";
  const cancel = button(t("SetKataRules.btnCancel"), () => dialog.close()); cancel.dataset.key = "settings.cancel";
  actions(body, save, cancel);
  onOpened?.(body);
  await dialog.closed;
}
