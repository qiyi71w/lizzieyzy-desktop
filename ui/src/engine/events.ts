import { registerViewHandler, run, state } from "../app";
import type { EngineFailureView, RulesPromptView } from "../contract/generated";
import { chooseSave, formPanel, message, panel } from "../dialogs";
import { format, product, t } from "../i18n";
import { chooseEngine, openMoreEngines } from "./catalog";
import { actions, button, checkbox } from "./controls";
import { remoteFailureText } from "./remote";
import { openAutoSetup } from "../setup/dialog";

/** EngineFailedMessage: the failure text, the launch command and 详细信息; the details window
 *  holds the engine output with 复制错误 / 导出诊断包 (EngineFailedMessage.java:261-468). A remote
 *  engine's failure takes SSHController's wording; a configured engine whose login or connection
 *  failed opens 更多引擎 afterwards (SSHController.java:38-54). */
async function failureDialog(failure: EngineFailureView): Promise<void> {
  const body = document.createElement("div"); body.className = "engine-failure";
  const text = failure.remote ? remoteFailureText(failure.remote) : failure.message;
  const summary = document.createElement("pre"); summary.textContent = `${failure.engine_name}: ${text}`;
  const label = document.createElement("div"); label.textContent = t("EngineFailedMessage.engineCmd");
  const command = document.createElement("pre"); command.textContent = failure.command;
  body.append(summary, label, command);
  const error = `${failure.engine_name}: ${text}\n${t("EngineFailedMessage.engineCmd")} ${failure.command}\n${failure.output.join("\n")}`;
  const dialog = formPanel(t("Leelaz.engineFailed"), body);
  const details = () => {
    const box = document.createElement("div"); box.className = "engine-failure";
    const pre = document.createElement("pre"); pre.className = "engine-failure-details"; pre.textContent = failure.output.join("\n");
    const status = document.createElement("span");
    box.append(pre);
    actions(box,
      button(t("EngineFailedMessage.copyError"), async () => {
        try { await navigator.clipboard.writeText(error); status.textContent = t("EngineFailedMessage.copied"); } catch { status.textContent = t("EngineFailedMessage.copyFailed"); }
      }),
      button(t("EngineFailedMessage.exportDiagnostics"), () => void run({ type: "ExportDiagnostics" })));
    box.append(status);
    void panel(t("EngineFailedMessage.details"), box);
  };
  if (failure.repair) {
    const repair = button(t("EngineFailedMessage.openTensorRtRepair"), async () => { await dialog.close(); await openAutoSetup("acceleration", { engineId: failure.repair!.profileId }); });
    repair.dataset.key = "engine.failure.repair";
    repair.setAttribute("aria-label", t("EngineFailedMessage.openTensorRtRepairAccessibleName"));
    repair.setAttribute("aria-description", t("EngineFailedMessage.openTensorRtRepairAccessibleDescription"));
    repair.title = t("EngineFailedMessage.openTensorRtRepairAccessibleDescription");
    actions(body, repair);
  }
  const remote = failure.remote;
  const index = state.view?.settings.engine.engines.findIndex((e) => e.id === failure.profile_id) ?? -1;
  const restart = () => run(state.view?.engine.secondary?.index === index ? { type: "SwitchSecondaryEngine", index } : { type: "SwitchEngine", index });
  if (remote?.type === "HostKeyChanged") {
    // The analysis engine connects again at its next analysis; a configured engine restarts.
    // Only the key shown here is trusted: a key offered since has its own refusal.
    const trust = button(index >= 0 ? product("sshTrustAndRetry") : product("sshTrust"), async () => {
      const r = await run({ type: "SshCredential", op: { type: "TrustHostKey", host: remote.host, port: remote.port, offered: remote.offered } });
      await dialog.close();
      if (r?.type === "Done" && index >= 0) await restart();
    });
    trust.dataset.key = "engine.failure.trustHostKey";
    actions(body, trust);
  }
  actions(body, button(t("EngineFailedMessage.details"), details), button(product("close"), () => dialog.close()));
  await dialog.closed;
  if (index >= 0 && (remote?.type === "Authentication" || remote?.type === "Connect" || remote?.type === "PasswordMissing")) await openMoreEngines();
}

let notice: HTMLElement | null = null;
function limitNotice(): void {
  if (notice) return;
  const box = document.createElement("section"); notice = box;
  box.className = "engine-limit-notice"; box.setAttribute("role", "dialog"); box.setAttribute("aria-label", t("leelaz.stopByLimitTitle"));
  const title = document.createElement("strong"); title.textContent = t("leelaz.stopByLimitTitle");
  const text = document.createElement("p"); text.textContent = `${t("leelaz.stopByLimit")}\n${t("leelaz.stopByLimit2")}`;
  const suppress = checkbox(t("LizzieFrame.noNoticeAgain"), false);
  suppress.input.onchange = () => { if (suppress.input.checked) void run({ type: "SetPonderLimitTips", enabled: false }); };
  box.append(title, text, suppress.label);
  actions(box, button(product("close"), () => { box.remove(); notice = null; }));
  document.body.append(box);
}

/** LizzieFrame.offerContinueAfterRulesFailure: YES analyses with the engine's current rules, NO
 *  pauses analysis (JOptionPane YES_NO, warning). */
async function rulesPrompt(prompt: RulesPromptView): Promise<void> {
  const text = format(t("KifuRules.failurePrompt"), prompt.target, prompt.observed ?? t("KifuRules.actualUnconfirmed"), prompt.failure);
  const accept = await chooseSave(t("KifuRules.failureTitle"), text, product("yes"), product("no"));
  await run({ type: "AnswerRulesPrompt", prompt: prompt.prompt, accept });
}

let startupSeen = false;
let failures: Promise<void> = Promise.resolve();
registerViewHandler((view) => {
  if (view.engine_failure) {
    const failure = view.engine_failure;
    failures = failures.then(() => failureDialog(failure));
  }
  if (view.engine_notice === "StoppedByLimit") limitNotice();
  // EngineManager.showSetupModeEngineUnavailable: Utils.showMsg.
  if (view.engine_notice === "SetupModeActive") void message(t("Message.title"), t("EngineManager.setupModeActive"));
  // EngineManager.rejectSameEngineSelection: the foreground and the secondary engine must differ.
  if (view.engine_notice === "SameEngine") void message(t("Message.title"), t("EngineManager.sameEngineHint"));
  if (view.rules_prompt) {
    const prompt = view.rules_prompt;
    failures = failures.then(() => rulesPrompt(prompt));
  }
  if (!startupSeen) {
    startupSeen = true;
    if (view.settings.engine.startup === "Manual" && view.engine.engines.length > 0 && !view.engine.foreground) void chooseEngine(true);
  }
});
