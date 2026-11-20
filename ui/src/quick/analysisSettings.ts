// 分析 > 精析设置 (AnalysisSettings.java, frame 592x481): its controls as a flowing form in
// 综合设置's grammar (quick.css). The dialog edits a draft of `settings.analysis_engine`; 确定
// saves it, Escape discards it.
import { invoke } from "@tauri-apps/api/core";
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { dispatch, isApiError } from "../contract/client";
import type { AnalysisEngineSettings, LeaseAvailabilityView } from "../contract/generated";
import { ask, formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { actions } from "../engine/controls";
import { editAnalysisRules } from "../engine/rules";
import { editRemoteEngine, stagePassword } from "../engine/remote";
import { format, product, t } from "../i18n";
import "./quick.css";

/** AnalysisEngineCommandHelper / AnalysisSettings refusal texts; literal t() calls keep the keys in
 *  the Java string extraction (run() shows SaveAnalysisEngineSettings' commandRequired itself). */
function commandHelperText(key: string): string {
  const texts: Record<string, string> = {
    "AnalysisEngineCommandHelper.noEngineSelected": t("AnalysisEngineCommandHelper.noEngineSelected"),
    "AnalysisEngineCommandHelper.noDefaultEngine": t("AnalysisEngineCommandHelper.noDefaultEngine"),
    "AnalysisEngineCommandHelper.emptyCommand": t("AnalysisEngineCommandHelper.emptyCommand"),
    "AnalysisEngineCommandHelper.missingGtp": t("AnalysisEngineCommandHelper.missingGtp"),
    "AnalysisEngineCommandHelper.missingConfig": t("AnalysisEngineCommandHelper.missingConfig"),
    "AnalysisEngineCommandHelper.generateConfigFailed": t("AnalysisEngineCommandHelper.generateConfigFailed"),
    "AnalysisEngineCommandHelper.remoteUnsupported": t("AnalysisEngineCommandHelper.remoteUnsupported"),
    "AnalysisSettings.commandRequired": t("AnalysisSettings.commandRequired"),
  };
  // generateConfigFailed's {0} (the IO error) is not part of the refusal.
  return format(texts[key] ?? t(key));
}

/** `AnalysisSettings.reuseStatus.<availability>` (Leelaz.ExclusiveGtpLeaseAvailability). */
function availabilityText(lease: LeaseAvailabilityView): string {
  switch (lease) {
    case "Available": return t("AnalysisSettings.reuseStatus.available");
    case "NoForegroundEngine": return t("AnalysisSettings.reuseStatus.no_foreground_engine");
    case "EngineNotReady": return t("AnalysisSettings.reuseStatus.engine_not_ready");
    case "NotKatago": return t("AnalysisSettings.reuseStatus.not_katago");
    case "MissingCapability": return t("AnalysisSettings.reuseStatus.missing_capability");
    case "ExistingLease": return t("AnalysisSettings.reuseStatus.existing_lease");
    case "EngineStateUnrestored": return t("AnalysisSettings.reuseStatus.engine_state_unrestored");
    case "PlayMode": return t("AnalysisSettings.reuseStatus.play_mode");
    case "ApplicationExclusive": return t("AnalysisSettings.reuseStatus.application_exclusive_mode");
    case "HumanSlGame": return t("AnalysisSettings.reuseStatus.human_sl_game");
    case "ReadBoardGma": return t("AnalysisSettings.reuseStatus.readboard_gma");
  }
}

/** reuseTargetText (AnalysisSettings.java:505-527). */
function reuseTargetText(): string {
  const engine = state.view?.engine;
  if (!engine?.foreground) return t("AnalysisSettings.reuseStatus.noEngine");
  const name = engine.foreground.name.trim() || t("AnalysisSettings.reuseStatus.currentEngine");
  return `${t("AnalysisSettings.reuseStatus.target")} ${name} - ${availabilityText(engine.lease)}`;
}

type Generated = { command: string; generatedConfig: string | null } | { refused: string };

/** GenerateAnalysisCommand: the analysis command from an engine profile (null: the current one).
 *  The display resolution falls back silently (resolveAnalysisEngineCommandForDisplay); the
 *  buttons show a refusal like Utils.showMsg. */
async function generate(index: number | null, quiet: boolean): Promise<Generated | null> {
  try {
    const result = await dispatch({ type: "GenerateAnalysisCommand", index });
    if (result.type === "AnalysisCommand") return { command: result.command, generatedConfig: result.generated_config };
    if (result.type === "Refused") {
      if (!quiet) await message(t("Message.title"), commandHelperText(result.message_key), "warning");
      return { refused: result.message_key };
    }
    return null;
  } catch (e) {
    if (!quiet) await message(t("Message.title"), isApiError(e) ? e.message : String(e), "error");
    return null;
  }
}

function text(content: string, className = ""): HTMLSpanElement {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = content;
  return el;
}

/** A labelled field of the form: the label column, then the control column. */
function field(host: HTMLElement, label: HTMLElement, control: HTMLElement): void {
  const row = document.createElement("div");
  row.className = "engine-field";
  row.append(label, control);
  host.append(row);
}

function choice(type: "checkbox" | "radio", label: string, key: string, checked: boolean): { label: HTMLLabelElement; input: HTMLInputElement } {
  const input = document.createElement("input");
  input.type = type;
  input.checked = checked;
  input.dataset.key = `analysisSettings.${key}`;
  if (type === "radio") input.name = "analysis-rules";
  const el = document.createElement("label");
  el.className = "quick-check";
  el.append(input, label);
  return { label: el, input };
}

function jbutton(label: string, key: string, action: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.textContent = label;
  b.dataset.key = `analysisSettings.${key}`;
  b.onclick = action;
  return b;
}

/** 选择引擎 (chooseSavedEngineCommand, java:529-563): a JOptionPane input dialog over the saved
 *  engines; resolves to the chosen index, or null. */
async function chooseEngine(): Promise<number | null> {
  const engines = state.view?.settings.engine.engines ?? [];
  if (!engines.length) {
    await message(t("Message.title"), t("AnalysisSettings.noSavedEngines"));
    return null;
  }
  const body = document.createElement("div");
  body.className = "display-input";
  const caption = document.createElement("div");
  caption.textContent = t("NewAnaGameDialog.chooseEngine");
  const select = document.createElement("select");
  select.dataset.key = "analysisSettings.engineChoice";
  engines.forEach((engine, k) => {
    const option = document.createElement("option");
    option.value = String(k);
    option.textContent = engine.name.trim() || `Engine ${k + 1}`;
    select.append(option);
  });
  body.append(caption, select);
  queueMicrotask(() => select.focus());
  return (await ask(t("NewAnaGameDialog.chooseEngine"), body, product("ok"), product("cancel"))) ? Number(select.value) : null;
}

/** The lblHint2 HTML: the engine's name stressed and the link to KataGo's analysis engine
 *  documentation, as a hint line. */
function hintLabel(): HTMLElement {
  const el = text("", "engine-hint analysis-settings-hint");
  const source = t("AnalysisSettings.lblHint2").replace(/^<html>|<\/html>$/g, "");
  const parsed = new DOMParser().parseFromString(source, "text/html").body;
  // The link's own <font> is its text; any other names the analysis engine.
  for (const font of parsed.querySelectorAll("font")) {
    const span = document.createElement("span");
    if (!font.closest("a")) span.className = "analysis-settings-em";
    span.append(...font.childNodes);
    font.replaceWith(span);
  }
  for (const bold of parsed.querySelectorAll("b")) bold.replaceWith(...bold.childNodes);
  for (const link of parsed.querySelectorAll("a")) {
    const span = document.createElement("span");
    span.className = "analysis-settings-link";
    span.title = link.getAttribute("href") ?? "";
    span.append(...link.childNodes);
    // LinkLabel's Desktop.browse (java:628-637); the shell opens the same href every bundle has.
    span.onclick = () => void invoke("open_analysis_engine_docs").catch(() => { /* Java ignores a browser that cannot open. */ });
    link.replaceWith(span);
  }
  el.append(...parsed.childNodes);
  return el;
}

/** How the dialog was opened from a flash analysis (AnalysisSettings(isDuringAnalyze, fromError,
 *  context)). */
export interface AnalysisSettingsOptions {
  /** After a failed analysis process: 确定并重新计算 is shown (isDuringAnalyze). */
  redo?: boolean;
  /** The flash analysis to start after 确定 with the settings it kept
   *  (promptForMissingFlashAnalysisCommand → resumeFlashAnalysisAfterSettings); closing the
   *  dialog drops it. */
  resume?: (kept: AnalysisEngineSettings) => Promise<void>;
  /** Context.BATCH: the 单步计算量 field shows and saves 批量精析's `batchVisits` instead of
   *  `maxVisits`. Without it the dialog follows Java's default: BATCH while a 批量精析 runs
   *  (`isBatchAnalysisMode`). */
  batch?: boolean;
}

/** Resolves to the settings 确定 kept, or null when the dialog was closed without saving. */
export async function openAnalysisEngineSettings(options: AnalysisSettingsOptions = {}): Promise<AnalysisEngineSettings | null> {
  const fromError = options.redo === true || options.resume !== undefined;
  const view = state.view;
  if (!view) return null;
  const batch = options.batch ?? view.batch.running?.mode === "Flash";
  const saved: AnalysisEngineSettings = { ...view.settings.analysis_engine };
  // resolveAnalysisEngineCommandForDisplay: a command that follows the managed engine is
  // generated again from the current engine; otherwise (or when that fails) the saved one shows.
  let shown = saved.command;
  let generatedConfig: string | null = null;
  if (!saved.commandCustomized) {
    const result = await generate(null, true);
    if (result && "command" in result) {
      shown = result.command;
      generatedConfig = result.generatedConfig;
    }
  }
  let rules = saved.specificRules;
  let edited = false;
  const panel = document.createElement("div");
  panel.className = "analysis-settings";
  const form = document.createElement("div");
  form.className = "quick-form";
  panel.append(form);
  const toolbar = document.createElement("span");
  toolbar.className = "analysis-settings-row";
  field(form, text(t("AnalysisSettings.lblEngineCmd")), toolbar);
  const remoteRow = document.createElement("div");
  remoteRow.className = "quick-check-with";
  const command = document.createElement("textarea");
  command.className = "analysis-settings-command";
  command.dataset.key = "analysisSettings.command";
  command.spellcheck = false;
  command.value = shown;
  command.setAttribute("aria-label", t("AnalysisSettings.lblEngineCmd"));
  // A missing analysis.cfg the generation wrote: said under the command, with its path (the
  // dialog's text is selectable), and replaced at the next generation.
  const generatedStatus = text("", "engine-hint analysis-settings-generated");
  generatedStatus.setAttribute("role", "status");
  generatedStatus.dataset.key = "analysisSettings.generatedConfig";
  const showGeneratedConfig = (path: string | null) => {
    generatedStatus.textContent = path === null ? "" : format(t("AnalysisEngineCommandHelper.generatedConfig"), path);
    generatedStatus.hidden = path === null;
  };
  showGeneratedConfig(generatedConfig);
  const take = (result: Generated | null) => {
    if (!result || !("command" in result)) return;
    command.value = result.command;
    edited = true;
    showGeneratedConfig(result.generatedConfig);
  };
  // 自动生成 (GetEngineLine(dialog, true, true, false, false)): the KataGo executable, model and
  // config choosers build an `analysis … -quit-without-waiting` line.
  const generateButton = jbutton(t("AnalysisSettings.btnGenerate"), "generate", async () => {
    const result = await run({ type: "Tuning", op: { type: "Scan", engine_type: "KataGo", analysis: true } });
    if (result?.type === "Tuning" && result.result.type === "Scanned") {
      command.value = result.result.command;
      edited = true;
    }
  });
  const engineButton = jbutton(t("NewAnaGameDialog.chooseEngine"), "chooseEngine", async () => {
    const index = await chooseEngine();
    if (index !== null) take(await generate(index, false));
  });
  toolbar.append(generateButton, engineButton);
  // 远程引擎 / 设置 (java:354-377): 设置 edits pending data that 确定 keeps with the rest.
  const remote = choice("checkbox", t("MoreEngines.chkRemoteEngine"), "remoteEngine", saved.useSsh);
  let ssh = saved.ssh;
  let sshPassword: string | null = null;
  const remoteSettings = jbutton(t("Menu.settings"), "remoteSettings", async () => {
    const next = await editRemoteEngine(ssh, sshPassword);
    if (next) { ssh = next.ssh; sshPassword = next.password; }
  });
  remoteRow.append(remote.label, remoteSettings);
  form.append(remoteRow, command, generatedStatus, text(t("AnalysisSettings.example"), "engine-hint analysis-settings-example"), hintLabel(), document.createElement("hr"));
  const visits = document.createElement("input");
  visits.type = "text";
  visits.className = "quick-number";
  // Context.BATCH (java:303-306): the same label edits `batch-analysis-playouts`.
  visits.value = String(batch ? saved.batchVisits : saved.maxVisits);
  visits.dataset.key = batch ? "analysisSettings.batchVisits" : "analysisSettings.maxVisits";
  visits.setAttribute("aria-label", t("AnalysisSettings.lblMaxVisits"));
  field(form, text(t("AnalysisSettings.lblMaxVisits")), visits);
  const current = choice("radio", t("AnalysisSettings.rdoUseCurrentRules"), "useCurrentRules", saved.useCurrentRules);
  const specific = choice("radio", t("AnalysisSettings.rdoUseSpecificRules"), "useSpecificRules", !saved.useCurrentRules);
  const specificRow = document.createElement("span");
  specificRow.className = "analysis-settings-row";
  specificRow.append(specific.label, jbutton(t("AnalysisSettings.btnSetRules"), "setRules", async () => {
    const next = await editAnalysisRules(rules);
    if (next !== null) rules = next;
  }));
  const ruleChoices = document.createElement("div");
  ruleChoices.className = "analysis-settings-rules";
  ruleChoices.setAttribute("role", "radiogroup");
  ruleChoices.setAttribute("aria-label", t("AnalysisSettings.lblRules"));
  ruleChoices.append(current.label, specificRow);
  field(form, text(t("AnalysisSettings.lblRules")), ruleChoices);
  form.append(document.createElement("hr"));
  const override = choice("checkbox", t("AnalysisSettings.chkAlwaysOverride"), "alwaysOverride", saved.alwaysOverride);
  const preload = choice("checkbox", t("AnalysisSettings.chkPreLoad"), "preload", saved.preload);
  const autoQuit = choice("checkbox", t("AnalysisSettings.chkAutoExit"), "autoQuit", saved.autoQuit);
  const reuse = choice("checkbox", t("AnalysisSettings.chkReuseCurrentEngine"), "reuseCurrentEngine", saved.reuseCurrentEngine);
  const target = text("", "engine-hint analysis-settings-target");
  target.dataset.key = "analysisSettings.reuseTarget";
  form.append(override.label, preload.label, autoQuit.label, reuse.label, target);
  // updateReuseControls (java:493-503): reusing the foreground engine needs no command or process.
  const updateReuse = () => {
    const own = !reuse.input.checked;
    command.disabled = !own;
    generateButton.disabled = !own;
    engineButton.disabled = !own;
    remote.input.disabled = !own;
    remoteSettings.disabled = !own || !remote.input.checked;
    preload.input.disabled = !own;
    autoQuit.input.disabled = !own;
    target.textContent = reuseTargetText();
  };
  reuse.input.onchange = updateReuse;
  remote.input.onchange = updateReuse;
  updateReuse();
  const redo = jbutton(t("AnalysisSettings.btnConfirmAndRedo"), "confirmAndRedo", () => {});
  const confirm = jbutton(t("AnalysisSettings.btnConfirm"), "confirm", () => {});
  confirm.classList.add("primary");
  redo.hidden = options.redo !== true;
  actions(panel, redo, confirm);
  const dialog = formPanel(t("AnalysisSettings.title"), panel);
  let saving = false;
  let kept: AnalysisEngineSettings | null = null;
  /** saveConfig; `then` runs once the settings are kept and the dialog is closed. */
  const save = async (then: () => Promise<unknown>) => {
    if (saving) return;
    saving = true;
    try {
      const typed = parseJavaInt(visits.value.trim());
      // saveConfig (java:660-668): BATCH keeps the typed number as it is; otherwise at least 1.
      const settings: AnalysisEngineSettings = {
        ...saved,
        useSsh: remote.input.checked,
        ssh,
        command: command.value.trim(),
        maxVisits: batch ? saved.maxVisits : Math.max(1, typed ?? saved.maxVisits),
        batchVisits: batch ? (typed ?? saved.batchVisits) : saved.batchVisits,
        useCurrentRules: current.input.checked,
        specificRules: rules,
        alwaysOverride: override.input.checked,
        preload: preload.input.checked,
        autoQuit: autoQuit.input.checked,
        reuseCurrentEngine: reuse.input.checked,
      };
      // A refusal (AnalysisSettings.commandRequired) is shown by run and keeps the dialog open.
      // A typed password is staged first, as an analysis this save starts looks it up when
      // connecting; a refused or failed save puts the previous one back.
      const settle = await stagePassword(ssh, sshPassword);
      if (!settle) return;
      const result = await run({ type: "SaveAnalysisEngineSettings", settings, shown_command: shown, edited });
      const keep = result !== null && result.type !== "Refused";
      await settle(keep);
      if (!keep) return;
      sshPassword = null;
      kept = settings;
      await dialog.close();
      await then();
    } finally {
      saving = false;
    }
  };
  // 确定: from an error the running flash analysis ends (destroyAnalysisEngine) and a pending one
  // starts with the new settings.
  confirm.onclick = () => save(async () => {
    if (!fromError) return;
    await run({ type: "CancelFlashAnalysis" });
    if (kept) await options.resume?.(kept);
  });
  // 确定并重新计算: the last flash analysis runs again with the new settings.
  redo.onclick = () => save(() => run({ type: "RedoFlashAnalysis" }));
  await dialog.closed;
  return kept;
}

Object.assign(COMMANDS, {
  "lightning-analysis-settings": { run: () => void openAnalysisEngineSettings() },
} satisfies Record<string, CommandEntry>);
