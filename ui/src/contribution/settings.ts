// KataGo跑谱贡献设置 (ContributeSettings.java) and 选择已配置引擎 (SavedEnginePathChooser.java). The
// dialog holds a draft of `settings.contribution`; 保存设置 / 开始跑谱贡献! send it whole. The training
// password never comes back to the page: the field shows where one is kept, and only an edited
// field is sent (empty forgets it).
import { run, state } from "../app";
import type { ContributionCommand, ContributionQualification, ContributionSettings, SavedEngineCandidate, SshPasswordState, SshSettings } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { product, registerProduct, t } from "../i18n";
import { button, footer, groupTitle, input, primaryButton, row, switchRow } from "../engine/controls";
import { editRemoteEngine, stagePassword } from "../engine/remote";
import "./contribution.css";

function contribution(op: ContributionCommand) {
  return run({ type: "Contribution", op });
}

registerProduct({
  ZhCn: { contributionGroupEngine: "引擎", contributionGroupAccount: "账号", contributionGroupGames: "对局" },
  ZhTw: { contributionGroupEngine: "引擎", contributionGroupAccount: "帳號", contributionGroupGames: "對局" },
  EnUs: { contributionGroupEngine: "Engine", contributionGroupAccount: "Account", contributionGroupGames: "Games" },
  JaJp: { contributionGroupEngine: "エンジン", contributionGroupAccount: "アカウント", contributionGroupGames: "対局" },
  KoKr: { contributionGroupEngine: "엔진", contributionGroupAccount: "계정", contributionGroupGames: "대국" },
  ThTh: { contributionGroupEngine: "เอนจิน", contributionGroupAccount: "บัญชี", contributionGroupGames: "เกม" },
});

function passwordHint(s: SshPasswordState): string {
  switch (s) {
    case "Stored": return product("contributionPasswordStored");
    case "Session": return product("contributionPasswordSession");
    case "None": return "";
  }
}

function qualificationText(q: ContributionQualification): string {
  if (q.type === "Unknown") return product("contributionUnknown");
  return q.status === "unsupported"
    ? product("contributionUnsupported", q.version, q.backend)
    : product("contributionUnverified", q.version, q.backend);
}

/** IntDocument: digits only. */
function integerOnly(field: HTMLInputElement): void {
  field.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /\D/.test(e.data ?? "")) e.preventDefault();
  });
  field.addEventListener("input", () => {
    const digits = field.value.replace(/\D/g, "");
    if (digits !== field.value) field.value = digits;
  });
}

let open = false;

/** Menu 跑谱贡献 > KataGo训练设置 (LizzieFrame.openContributeSettings) and a missing user name or
 *  engine path at start. */
export async function openContributionSettings(): Promise<void> {
  const saved = state.view?.settings.contribution;
  if (!saved || open) return;
  open = true;
  try { await settingsDialog(saved); } finally { open = false; }
}

async function settingsDialog(saved: ContributionSettings): Promise<void> {
  const tip = {
    engine: t("ContributeSettings.engineTip"),
    // gamesTip2 (unfinished games are lost at the end) is added off Windows only.
    games: t("ContributeSettings.gamesTip") + (navigator.userAgent.includes("Windows") ? "" : t("ContributeSettings.gamesTip2")),
    config: t("ContributeSettings.configTip"),
    ownership: t("ContributeSettings.ownerShipTip"),
    autoSave: t("ContributeSettings.autoSaveTip"),
    command: t("ContributeSettings.customCommandTip"),
    noRating: t("ContributeSettings.noRatingMatchesTip"),
    savedEngine: t("ContributeSettings.savedEngineTip"),
  };
  let ssh: SshSettings = saved.ssh;

  const enginePath = input(saved.enginePath); enginePath.title = tip.engine; enginePath.dataset.key = "contribution.enginePath";
  enginePath.setAttribute("aria-label", t("ContributeSettings.lblEngine"));
  const configPath = input(saved.configPath); configPath.title = tip.config; configPath.dataset.key = "contribution.configPath";
  configPath.setAttribute("aria-label", t("ContributeSettings.lblConfig"));
  const command = input(saved.command); command.title = tip.command; command.dataset.key = "contribution.command";
  command.setAttribute("aria-label", t("ContributeSettings.chkUseCommand"));
  const userName = input(saved.userName); userName.dataset.key = "contribution.userName";
  userName.setAttribute("aria-label", t("ContributeSettings.lblUserName"));
  const password = input("", "password"); password.autocomplete = "off"; password.dataset.key = "contribution.password";
  password.setAttribute("aria-label", t("ContributeSettings.lblPassword"));
  const games = input(saved.batchGames); games.inputMode = "numeric"; games.title = tip.games; games.dataset.key = "contribution.batchGames"; integerOnly(games);
  games.setAttribute("aria-label", t("ContributeSettings.lblSimultaneousGames"));

  // GetEngineLine.relativizePath: a chosen file goes into the command line quoted, so a path with
  // spaces stays one argument (kept absolute: the contribution runs in its own folder).
  const quoted = (path: string) => `"${path}"`;
  const browseEngine = button(t("ContributeSettings.btnScan"), async () => {
    const r = await contribution({ type: "ChooseFile", kind: "Engine", title: t("LizzieConfig.prompt.selectEngine") });
    if (r?.type === "FileChosen") { enginePath.value = quoted(r.path); void qualify(); }
  });
  browseEngine.title = tip.engine; browseEngine.dataset.key = "contribution.browseEngine";
  const savedEngine = button(t("ContributeSettings.btnSavedEngine"), async () => {
    const r = await contribution({ type: "SavedEngines" });
    if (r?.type !== "ContributionSavedEngines") return;
    if (r.candidates.length === 0) { await message(t("Message.title"), t("ContributeSettings.savedEngine.empty")); return; }
    const chosen = await chooseSavedEngine(r.candidates);
    if (chosen?.field_text) { enginePath.value = chosen.field_text; void qualify(); }
  });
  savedEngine.title = tip.savedEngine; savedEngine.setAttribute("aria-description", tip.savedEngine); savedEngine.dataset.key = "contribution.savedEngine";
  const browseConfig = button(t("ContributeSettings.btnScan"), async () => {
    const r = await contribution({ type: "ChooseFile", kind: "Config", title: t("MoreEngines.chooseConfig") });
    if (r?.type === "FileChosen") configPath.value = quoted(r.path);
  });
  browseConfig.title = tip.config; browseConfig.dataset.key = "contribution.browseConfig";
  // 远程引擎设置's 确定 keeps the connection at once and marks it used (RemoteEngineSettings.java:192-207).
  const remoteSettings = button(t("ContributeSettings.btnRemoteSetting"), async () => {
    const edited = await editRemoteEngine(ssh, null);
    if (!edited) return;
    const current = state.view?.settings.contribution ?? saved;
    const settle = await stagePassword(edited.ssh, edited.password);
    if (!settle) return;
    const r = await contribution({ type: "SaveSettings", settings: { ...current, ssh: edited.ssh, remote: true } });
    // Only the password changed: the settings answer Unchanged, and the new password stays.
    const keep = r?.type === "Done" || r?.type === "Unchanged";
    await settle(keep);
    if (keep) ssh = edited.ssh;
  });
  remoteSettings.dataset.key = "contribution.remoteSettings";
  const signUp = button(t("ContributeSettings.btnSignUp"), () => void contribution({ type: "OpenLink", link: "SignUp" }));
  signUp.dataset.key = "contribution.signUp";

  const qualification = document.createElement("span"); qualification.className = "ds-row-sub contribution-qualification"; qualification.setAttribute("aria-live", "polite");
  let qualifying = 0;
  const qualify = async () => {
    const ticket = ++qualifying;
    // A custom command names no engine file this client could know (§1.5).
    if (useCommand.input.checked) { qualification.textContent = product("contributionUnknown"); return; }
    const path = enginePath.value.trim();
    if (!path) { qualification.textContent = ""; return; }
    const r = await contribution({ type: "Qualify", engine_path: path });
    if (ticket === qualifying && r?.type === "ContributionQualification") qualification.textContent = qualificationText(r.qualification);
  };
  let typing: number | undefined;
  enginePath.addEventListener("input", () => { window.clearTimeout(typing); typing = window.setTimeout(() => void qualify(), 300); });
  let passwordEdited = false;
  password.addEventListener("input", () => { passwordEdited = true; });
  let checking = 0;
  const refreshPassword = async () => {
    const ticket = ++checking;
    const user = userName.value.trim();
    if (!user) { password.placeholder = ""; return; }
    const r = await contribution({ type: "PasswordState", user });
    if (ticket === checking && r?.type === "ContributionPassword") password.placeholder = passwordHint(r.state);
  };
  userName.addEventListener("change", () => void refreshPassword());

  // Grouped rows. The paths and the command take a line of their own under their row's name; the
  // engine's qualification is said under the engine row's name.
  const body = document.createElement("div"); body.className = "contribution-settings-body";
  groupTitle(body, product("contributionGroupEngine"));
  const engineRow = row(body, t("ContributeSettings.lblEngine"), tip.engine, enginePath, browseEngine, savedEngine);
  engineRow.classList.add("contribution-path");
  engineRow.querySelector(".ds-row-text")!.append(qualification);
  row(body, t("ContributeSettings.lblConfig"), tip.config, configPath, browseConfig).classList.add("contribution-path");
  const useCommand = switchRow(body, t("ContributeSettings.chkUseCommand"), tip.command, saved.useCommand);
  useCommand.row.classList.add("contribution-path");
  useCommand.row.append(command);
  const remote = switchRow(body, t("ContributeSettings.chkRemote"), "", saved.remote, remoteSettings);
  const ignore = switchRow(body, t("ContributeSettings.chkIgnoreSettings"), "", saved.usePureCommand);
  groupTitle(body, product("contributionGroupAccount"));
  row(body, t("ContributeSettings.lblUserName"), "", userName, signUp);
  row(body, t("ContributeSettings.lblPassword"), "", password);
  groupTitle(body, product("contributionGroupGames"));
  row(body, t("ContributeSettings.lblSimultaneousGames"), tip.games, games);
  const ownership = switchRow(body, t("ContributeSettings.lblShowOwnerShip"), tip.ownership, saved.showEstimate).input;
  const noRating = switchRow(body, t("ContributeSettings.lblChkNoRatingMatches"), tip.noRating, saved.disableRatingMatches).input;
  const autoSave = switchRow(body, t("ContributeSettings.lblAutoSave"), tip.autoSave, saved.autoSave).input;
  useCommand.input.dataset.key = "contribution.useCommand"; remote.input.dataset.key = "contribution.remote"; ignore.input.dataset.key = "contribution.usePureCommand";
  ownership.dataset.key = "contribution.showEstimate"; noRating.dataset.key = "contribution.disableRatingMatches"; autoSave.dataset.key = "contribution.autoSave";

  // setAferSets: the rest of the form does not apply to a command followed as it is.
  const afterSets = (ignored: boolean) => { for (const el of [userName, password, games, ownership, noRating]) el.disabled = ignored; };
  const commandMode = () => {
    const on = useCommand.input.checked;
    remote.input.disabled = !on;
    remoteSettings.disabled = !remote.input.checked;
    command.disabled = !on;
    ignore.input.disabled = !on;
    for (const el of [enginePath, configPath, browseEngine, savedEngine, browseConfig]) el.disabled = on;
    afterSets(on && ignore.input.checked);
  };
  useCommand.input.onchange = () => { commandMode(); void qualify(); };
  remote.input.onchange = () => { remoteSettings.disabled = !remote.input.checked; };
  ignore.input.onchange = () => afterSets(ignore.input.checked);
  autoSave.onchange = () => { if (autoSave.checked) void message(t("Message.title"), tip.autoSave); };

  const draft = (): ContributionSettings => ({
    ...(state.view?.settings.contribution ?? saved),
    enginePath: enginePath.value, configPath: configPath.value, userName: userName.value, batchGames: games.value,
    showEstimate: ownership.checked, useCommand: useCommand.input.checked, usePureCommand: ignore.input.checked,
    command: command.value, autoSave: autoSave.checked, disableRatingMatches: noRating.checked,
    remote: remote.input.checked, ssh,
  });
  /** saveConfig, then the password when its field was edited. */
  const save = async (): Promise<boolean> => {
    const settings = draft();
    const r = await contribution({ type: "SaveSettings", settings });
    // Saving what is already saved answers Unchanged; both go on (saveConfig always succeeds).
    if (r?.type !== "Done" && r?.type !== "Unchanged") return false;
    if (passwordEdited) {
      const p = await contribution({ type: "SetPassword", user: settings.userName, password: password.value });
      if (p?.type !== "ContributionPassword") return false;
      passwordEdited = false;
      password.value = "";
      password.placeholder = passwordHint(p.state);
    }
    return true;
  };

  const dialog = formPanel(t("ContributeSettings.title"), body);
  const saveButton = button(t("ContributeSettings.btnSave"), () => void save());
  saveButton.dataset.key = "contribution.save";
  const start = primaryButton(t("ContributeSettings.btnStart"), async () => {
    if (!(await save())) return;
    await dialog.close();
    await contribution({ type: "Start" });
  });
  start.dataset.key = "contribution.start";
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "contribution.cancel";
  footer(body, { start: [saveButton], primary: start, cancel });

  commandMode();
  void qualify();
  void refreshPassword();
  (enginePath.disabled ? command : enginePath).focus();
  await dialog.closed;
  clearTimeout(typing);
}

/** SavedEnginePathChooser.choose: the confirmed available entry, or null when cancelled. */
async function chooseSavedEngine(candidates: SavedEngineCandidate[]): Promise<SavedEngineCandidate | null> {
  const kinds = {
    Local: t("ContributeSettings.savedEngine.typeLocal"),
    Remote: t("ContributeSettings.savedEngine.typeRemote"),
    Indirect: t("ContributeSettings.savedEngine.typeIndirect"),
    Unknown: t("ContributeSettings.savedEngine.typeUnknown"),
  };
  const reasons = {
    EmptyCommand: t("ContributeSettings.savedEngine.reasonEmpty"),
    UnparseableCommand: t("ContributeSettings.savedEngine.reasonUnparseable"),
    MissingFile: t("ContributeSettings.savedEngine.reasonMissing"),
    RemoteTransport: t("ContributeSettings.savedEngine.reasonRemote"),
    IndirectLauncher: t("ContributeSettings.savedEngine.reasonIndirect"),
  };
  const available = (c: SavedEngineCandidate) => !!c.executable && !!c.field_text;
  const reason = (c: SavedEngineCandidate) => reasons[c.reason ?? "IndirectLauncher"];

  const body = document.createElement("div"); body.className = "contribution-saved";
  const hint = document.createElement("p"); hint.textContent = t("ContributeSettings.savedEngineTip");
  const wrap = document.createElement("div"); wrap.className = "engine-table-wrap contribution-saved-table";
  const table = document.createElement("table"); table.className = "engine-table";
  const head = table.createTHead().insertRow();
  for (const key of [t("ContributeSettings.savedEngine.columnIndex"), t("ContributeSettings.savedEngine.columnName"), t("ContributeSettings.savedEngine.columnType"), t("ContributeSettings.savedEngine.columnPath")]) {
    const th = document.createElement("th"); th.textContent = key; head.append(th);
  }
  const rows = table.createTBody();
  const detail = document.createElement("textarea"); detail.readOnly = true; detail.rows = 3; detail.className = "contribution-saved-detail";
  detail.setAttribute("aria-label", t("ContributeSettings.savedEngine.columnPath"));
  let selected = 0;
  let chosen: SavedEngineCandidate | null = null;
  const fill = primaryButton(t("ContributeSettings.savedEngine.fill"), () => confirmChoice());
  fill.dataset.key = "contribution.savedEngine.fill";
  const dialog = formPanel(t("ContributeSettings.savedEngine.title"), body);
  const confirmChoice = () => {
    const c = candidates[selected];
    if (!c || !available(c)) return;
    chosen = c;
    void dialog.close();
  };
  const select = (k: number) => {
    selected = k;
    [...rows.rows].forEach((row, i) => row.classList.toggle("selected", i === k));
    const c = candidates[k];
    fill.disabled = !available(c);
    detail.value = available(c)
      ? `${t("ContributeSettings.savedEngine.detailPath")}\n${c.executable}`
      : `${t("ContributeSettings.savedEngine.detailUnavailable")}\n${reason(c)}`;
  };
  candidates.forEach((c, k) => {
    const row = rows.insertRow();
    const path = available(c) ? c.executable! : reason(c);
    for (const text of [String(c.index), c.name || t("ContributeSettings.savedEngine.unnamed"), kinds[c.kind], path]) {
      const td = row.insertCell(); td.textContent = text; td.title = text;
    }
    if (!available(c)) row.classList.add("contribution-unavailable");
    row.onclick = () => select(k);
    row.ondblclick = () => { select(k); confirmChoice(); };
  });
  table.tabIndex = 0;
  table.setAttribute("aria-label", t("ContributeSettings.savedEngine.title"));
  table.onkeydown = (e) => {
    if (e.key === "ArrowDown" && selected < candidates.length - 1) { e.preventDefault(); select(selected + 1); }
    if (e.key === "ArrowUp" && selected > 0) { e.preventDefault(); select(selected - 1); }
    if (e.key === "Enter") { e.preventDefault(); confirmChoice(); }
  };
  wrap.append(table);
  body.append(hint, wrap, detail);
  const cancel = button(t("ContributeSettings.savedEngine.cancel"), () => void dialog.close());
  footer(body, { primary: fill, cancel });
  select(0);
  table.focus();
  await dialog.closed;
  return chosen;
}
