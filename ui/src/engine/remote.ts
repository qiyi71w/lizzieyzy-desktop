// Remote (SSH) engines: the connection fields of 更多引擎 (MoreEngines.java:285-413) and
// 远程引擎设置 (RemoteEngineSettings.java), and the texts of remote failures. Passwords never
// return to the page: the field shows whether one is kept, and a typed one replaces it.
import { run } from "../app";
import type { RemoteFailure, SshPasswordState, SshSettings } from "../contract/generated";
import { formPanel } from "../dialogs";
import { product, registerProduct, t } from "../i18n";
import { actions, button, input } from "./controls";

registerProduct({
  ZhCn: {
    sshPasswordStored: "已保存在系统凭据库；留空保持不变",
    sshPasswordSession: "本次运行已记住；留空保持不变",
    sshPasswordMissing: "{0} 没有已保存的密码。输入密码并保存后再连接。",
    sshHostKeyChanged: "{0} 提供的主机密钥与之前信任的不同，连接已中止，未发送登录信息。\n之前信任：{1}\n本次提供：{2}\n只有确认服务器确实更换了密钥时，才信任新密钥。",
    sshTrustAndRetry: "信任新密钥并重试",
    sshTrust: "信任新密钥",
    sshEndedStatus: "远程引擎进程已在服务器上结束（退出码 {0}）。",
    sshEndedSignal: "远程引擎进程已在服务器上被信号 {0} 结束。",
    sshEnded: "远程引擎进程已在服务器上结束。",
    sshLost: "与服务器的连接已断开，无法确认远程引擎进程是否仍在运行（{0}）。",
  },
  ZhTw: {
    sshPasswordStored: "已儲存在系統憑據庫；留空保持不變",
    sshPasswordSession: "本次執行已記住；留空保持不變",
    sshPasswordMissing: "{0} 沒有已儲存的密碼。輸入密碼並儲存後再連線。",
    sshHostKeyChanged: "{0} 提供的主機金鑰與之前信任的不同，連線已中止，未傳送登入資訊。\n之前信任：{1}\n本次提供：{2}\n只有確認伺服器確實更換了金鑰時，才信任新金鑰。",
    sshTrustAndRetry: "信任新金鑰並重試",
    sshTrust: "信任新金鑰",
    sshEndedStatus: "遠端引擎行程已在伺服器上結束（結束代碼 {0}）。",
    sshEndedSignal: "遠端引擎行程已在伺服器上被訊號 {0} 結束。",
    sshEnded: "遠端引擎行程已在伺服器上結束。",
    sshLost: "與伺服器的連線已中斷，無法確認遠端引擎行程是否仍在執行（{0}）。",
  },
  EnUs: {
    sshPasswordStored: "Kept in the system credential store; leave empty to keep it",
    sshPasswordSession: "Kept until you quit; leave empty to keep it",
    sshPasswordMissing: "No password is saved for {0}. Enter the password, save, then connect again.",
    sshHostKeyChanged: "{0} offered a host key other than the trusted one. The connection was stopped before any login was sent.\nTrusted: {1}\nOffered: {2}\nTrust the new key only if you know the server's key was changed.",
    sshTrustAndRetry: "Trust new key and retry",
    sshTrust: "Trust new key",
    sshEndedStatus: "The remote engine process ended on the server (exit code {0}).",
    sshEndedSignal: "The remote engine process on the server was ended by signal {0}.",
    sshEnded: "The remote engine process ended on the server.",
    sshLost: "The connection to the server closed; whether the remote engine process still runs is unknown ({0}).",
  },
});

/** The failure text: Java's SSHController wording for login and connection failures. */
export function remoteFailureText(r: RemoteFailure): string {
  switch (r.type) {
    case "Authentication": return t("SSHController.connectFailed");
    case "Connect": return `${t("SSHController.engineFailed")}: ${r.detail || t("Leelaz.engineStartNoExceptionMessage")}`;
    case "PasswordMissing": return product("sshPasswordMissing", r.account);
    case "HostKeyChanged": return product("sshHostKeyChanged", `${r.host}:${r.port}`, r.trusted, r.offered);
    case "Ended":
      if (r.status !== null) return product("sshEndedStatus", r.status);
      if (r.signal !== null) return product("sshEndedSignal", r.signal);
      return product("sshEnded");
    case "Lost": return product("sshLost", r.detail);
  }
}

/** The 关于远程引擎 information (MoreEngines btnRemoteEngine). */
export function aboutRemoteButton(): HTMLButtonElement {
  const about = button("?", () => {
    const body = document.createElement("div");
    const text = document.createElement("p"); text.className = "engine-remote-about"; text.textContent = t("MoreEngines.aboutRemoteEngine");
    body.append(text);
    const dialog = formPanel(t("MoreEngines.aboutRemoteEngineTitle"), body);
    actions(body, button(product("ok"), () => dialog.close()));
  });
  about.dataset.key = "engine.remote.about";
  about.title = t("MoreEngines.aboutRemoteEngineTitle");
  return about;
}

/** Labels of one remote form: 更多引擎 and 远程引擎设置 use their own Java keys. */
export interface RemoteLabels { host: string; port: string; user: string; password: string; key: string; browse: string }

/** IP, 端口, 用户名, 密码 / 密钥 with 浏览. The 密码 / 密钥 pair always has at most one choice. */
export class RemoteFields {
  readonly host = input("");
  readonly port = input("");
  readonly user = input("");
  readonly usePassword: HTMLInputElement;
  readonly useKey: HTMLInputElement;
  readonly password = input("", "password");
  readonly browse: HTMLButtonElement;
  readonly passwordLabel = document.createElement("label");
  readonly keyLabel = document.createElement("label");
  private keyPath = "";
  private enabled = true;
  private checked = 0;

  constructor(readonly labels: RemoteLabels, kind: "radio" | "checkbox", readonly prefix: string) {
    this.usePassword = input("", kind);
    this.useKey = input("", kind);
    if (kind === "radio") this.usePassword.name = this.useKey.name = `${prefix}-auth`;
    this.passwordLabel.append(this.usePassword, labels.password);
    this.keyLabel.append(this.useKey, labels.key);
    this.password.autocomplete = "off";
    this.password.setAttribute("aria-label", labels.password);
    this.host.setAttribute("aria-label", labels.host);
    this.port.setAttribute("aria-label", labels.port);
    this.user.setAttribute("aria-label", labels.user);
    for (const [el, key] of [[this.host, "host"], [this.port, "port"], [this.user, "user"], [this.password, "password"], [this.usePassword, "usePassword"], [this.useKey, "useKey"]] as const) el.dataset.key = `${prefix}.${key}`;
    this.browse = button(labels.browse, async () => {
      const r = await run({ type: "SshCredential", op: { type: "ChooseKeyFile", title: t("MoreEngines.chooseKeygen") } });
      if (r?.type === "FileChosen") this.setKeyPath(r.path);
    });
    this.browse.dataset.key = `${prefix}.browse`;
    // RemoteEngineSettings: unchecking one checks the other; MoreEngines' radios only select.
    this.usePassword.onchange = () => { this.useKey.checked = kind === "checkbox" ? !this.usePassword.checked : false; this.update(); };
    this.useKey.onchange = () => { this.usePassword.checked = kind === "checkbox" ? !this.useKey.checked : false; this.update(); };
    for (const el of [this.host, this.port, this.user]) el.addEventListener("change", () => void this.refreshPassword());
  }

  private setKeyPath(path: string): void {
    this.keyPath = path;
    this.browse.title = this.keyLabel.title = path;
  }

  /** The fields of `ssh`, or empty ones (MoreEngines without a remote engine selects neither). */
  set(ssh: SshSettings | undefined, defaultChoice = false): void {
    this.host.value = ssh?.host ?? ""; this.port.value = ssh?.port ?? ""; this.user.value = ssh?.user ?? "";
    this.usePassword.checked = ssh ? !ssh.useKey : defaultChoice;
    this.useKey.checked = ssh?.useKey ?? false;
    this.setKeyPath(ssh?.keyPath ?? "");
    this.password.value = "";
    this.update();
    void this.refreshPassword();
  }

  read(): SshSettings {
    return { host: this.host.value, port: this.port.value, user: this.user.value, useKey: this.useKey.checked, keyPath: this.keyPath };
  }

  /** A password typed to replace the kept one. */
  typedPassword(): string | null {
    return !this.useKey.checked && this.password.value ? this.password.value : null;
  }

  setEnabled(on: boolean): void {
    this.enabled = on;
    this.update();
  }

  private update(): void {
    for (const el of [this.host, this.port, this.user, this.usePassword, this.useKey]) el.disabled = !this.enabled;
    this.password.disabled = !this.enabled || !this.usePassword.checked;
    this.browse.disabled = !this.enabled || !this.useKey.checked;
  }

  /** Shows whether a password is kept for the account the fields name. */
  async refreshPassword(): Promise<void> {
    const ticket = ++this.checked;
    const ssh = this.read();
    if (!ssh.host.trim() || !ssh.user.trim()) { this.password.placeholder = ""; return; }
    const r = await run({ type: "SshCredential", op: { type: "PasswordState", ssh } });
    if (ticket !== this.checked || r?.type !== "SshPassword") return;
    this.password.placeholder = passwordHint(r.state);
  }

  /** Stages the typed password for the saving of the settings that use it (`stagePassword`);
   *  a kept one empties the field and shows where it is kept. */
  async stagePassword(): Promise<Settle | null> {
    const settle = await stagePassword(this.read(), this.typedPassword());
    if (!settle) return null;
    return async (keep) => {
      await settle(keep);
      if (!keep) return;
      this.password.value = "";
      await this.refreshPassword();
    };
  }
}

function passwordHint(state: SshPasswordState): string {
  switch (state) {
    case "Stored": return product("sshPasswordStored");
    case "Session": return product("sshPasswordSession");
    case "None": return "";
  }
}

/** 远程引擎设置 for 精析设置 (RemoteEngineSettings(owner, initialData, consumer)): 确定 hands the
 *  fields back as pending data; nothing is kept until 精析设置 is saved. */
export async function editRemoteEngine(initial: SshSettings, password: string | null): Promise<{ ssh: SshSettings; password: string | null } | null> {
  const fields = new RemoteFields({
    host: t("RemoteEngineSettings.lblIP"), port: t("RemoteEngineSettings.lblProt"), user: t("RemoteEngineSettings.lblUserName"),
    password: t("RemoteEngineSettings.chkUsePassword"), key: t("RemoteEngineSettings.chkUseKeyGen"), browse: t("RemoteEngineSettings.scanKeyGen"),
  }, "checkbox", "remoteEngineSettings");
  fields.set(initial);
  if (password !== null) fields.password.value = password;
  const body = document.createElement("div");
  const grid = document.createElement("div"); grid.className = "engine-remote-grid";
  const label = (text: string) => { const el = document.createElement("span"); el.textContent = text; return el; };
  grid.append(label(fields.labels.host), fields.host, label(fields.labels.port), fields.port, label(fields.labels.user), fields.user,
    fields.passwordLabel, fields.password, fields.keyLabel, fields.browse);
  body.append(grid);
  let result: { ssh: SshSettings; password: string | null } | null = null;
  const dialog = formPanel(t("RemoteEngineSettings.title"), body);
  const ok = button(t("RemoteEngineSettings.okButton"), () => {
    result = { ssh: fields.read(), password: fields.typedPassword() };
    void dialog.close();
  });
  ok.dataset.key = "remoteEngineSettings.ok";
  actions(body, ok);
  await dialog.closed;
  return result;
}

/** Ends a staged password: `keep` once the settings were saved, else the previous comes back. */
export type Settle = (keep: boolean) => Promise<void>;

/** Keeps a typed password for `ssh` while the settings that use it are saved, so an engine that
 *  save starts finds it; with nothing to stage it settles as a no-op. Null when it could not be
 *  kept. */
export async function stagePassword(ssh: SshSettings, password: string | null): Promise<Settle | null> {
  if (password === null || ssh.useKey) return async () => {};
  const r = await run({ type: "SshCredential", op: { type: "StagePassword", ssh, password } });
  if (r?.type !== "SshPassword") return null;
  return async (keep) => { await run({ type: "SshCredential", op: { type: "SettlePassword", ssh, keep } }); };
}
