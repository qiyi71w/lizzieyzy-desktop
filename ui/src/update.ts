// 帮助 > 检查更新 (Menu.java:5274-5277): the 检查更新 page (CheckUpdateDialog) and the 发现新版本 window
// (WindowsUpdateDialog), both modeless over the workbench. Rust owns the settings, the check, the
// package preparation and the installation; these windows show `view.update` and send its commands.
// Opening the page uses no network. The automatic options save at once and apply from the next start.
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run, state } from "./app";
import { COMMANDS } from "./commands";
import type { UpdateDownloadView, UpdateOfferView, UpdateResultView, UpdateSettings, UpdateView } from "./contract/generated";
import { message } from "./dialogs";
import { product, registerProduct, t } from "./i18n";
import "./update.css";

registerProduct({
  ZhCn: {
    updateSigningNotConfigured: "此版本没有更新签名公钥，无法检查更新。",
    updateUnsupportedForm: "当前安装形式不支持程序内更新。",
    updateErrorSources: "无法下载更新文件，请检查网络后重试。",
    updateErrorInstall: "无法安装更新，当前程序未改变。请重试，或打开 Release 页面手动下载。",
    updateIntroInPlace: "这次会先下载并校验更新文件，然后替换程序文件并重新打开。",
    updateReadyIntroInPlace: "更新文件已下载并校验。点击“立即更新”后，将替换程序文件并重新打开。",
    updatePreserved: "用户数据、棋谱、设置、引擎和权重都会保留。",
    updateInstalling: "正在安装更新，程序将关闭并重新打开…",
  },
  ZhTw: {
    updateSigningNotConfigured: "此版本沒有更新簽名公鑰，無法檢查更新。",
    updateUnsupportedForm: "目前的安裝形式不支援程式內更新。",
    updateErrorSources: "無法下載更新檔案，請檢查網路後重試。",
    updateErrorInstall: "無法安裝更新，目前的程式未改變。請重試，或開啟 Release 頁面手動下載。",
    updateIntroInPlace: "這次會先下載並校驗更新檔案，然後替換程式檔案並重新開啟。",
    updateReadyIntroInPlace: "更新檔案已下載並校驗。點擊「立即更新」後，將替換程式檔案並重新開啟。",
    updatePreserved: "使用者資料、棋譜、設定、引擎和權重都會保留。",
    updateInstalling: "正在安裝更新，程式將關閉並重新開啟…",
  },
  EnUs: {
    updateSigningNotConfigured: "This build has no update signing key, so it cannot check for updates.",
    updateUnsupportedForm: "This kind of installation cannot update from within the program.",
    updateErrorSources: "The update could not be downloaded. Check your network and retry.",
    updateErrorInstall: "The update could not be installed; the program is unchanged. Retry, or download it from the Release page.",
    updateIntroInPlace: "The update is downloaded and verified first; the program files are then replaced and the program restarts.",
    updateReadyIntroInPlace: "The update is downloaded and verified. Update now replaces the program files and restarts the program.",
    updatePreserved: "User data, game records, settings, engines and weights are kept.",
    updateInstalling: "Installing the update; the program closes and restarts…",
  },
});

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

function button(text: string, key: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", "", text);
  b.type = "button";
  b.dataset.key = key;
  b.onclick = onClick;
  return b;
}

/** WindowsUpdateDialog.formatBytes. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  const units = ["KB", "MB", "GB"];
  let unit = -1;
  do {
    value /= 1024;
    unit++;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value.toFixed(1)} ${units[unit]}`;
}

const downloading = (d: UpdateDownloadView) => d.phase === "Downloading" || d.phase === "Paused";

/** UpdateText.userFacingError. */
function errorText(d: UpdateDownloadView): string | null {
  switch (d.error) {
    case "Integrity": return t("WindowsUpdate.error.integrity");
    case "Sources": return product("updateErrorSources");
    case "Install": return product("updateErrorInstall");
    case "Generic": return t("WindowsUpdate.error.generic");
    default: return null;
  }
}

/** UpdateDownloadText.status. */
function downloadStatus(u: UpdateView): string {
  if (u.installing) return product("updateInstalling");
  const d = u.download;
  let text = "";
  switch (d.phase) {
    case "Cancelled": text = t("WindowsUpdate.status.cancelled"); break;
    case "Failed": text = t("WindowsUpdate.status.failed"); break;
    case "Ready": text = t("WindowsUpdate.ready"); break;
    case "Paused": text = t("WindowsUpdate.status.paused"); break;
    case "Downloading":
      text = !d.started
        ? t("WindowsUpdate.status.preparing")
        : `${d.verifying ? t("WindowsUpdate.status.verifying") : t("WindowsUpdate.downloading")}\n${formatBytes(d.bytes)} / ${formatBytes(d.total)}`;
      break;
    case "None": break;
  }
  const error = errorText(d);
  return error ? `${text}\n${error}` : text;
}

/** UpdateCheckFeedback.warning: one 测试-channel candidate could not be verified. */
function warning(r: UpdateResultView): string {
  if (r.stable_candidate_failed === r.beta_candidate_failed) return "";
  return r.stable_candidate_failed ? t("WindowsUpdate.partialCheck.stable") : t("WindowsUpdate.partialCheck.beta");
}

/** UpdateCheckFeedback.message. */
function feedback(r: UpdateResultView): string {
  const beta = r.channel === "Beta";
  switch (r.outcome) {
    case "UnavailableBuild": return t("WindowsUpdate.devBuild");
    case "SigningNotConfigured": return product("updateSigningNotConfigured");
    case "UnsupportedForm": return product("updateUnsupportedForm");
    case "NoUpdate": return beta ? t("WindowsUpdate.noUpdate.beta") : t("WindowsUpdate.noUpdate.stable");
    case "NoPackage": return t("WindowsUpdate.noPackage");
    case "Failed":
      if (r.failure === "Fetch") return beta ? t("WindowsUpdate.fetchFailed.beta") : t("WindowsUpdate.fetchFailed.stable");
      return t("WindowsUpdate.checkFailed");
    case "Offer": return t("WindowsUpdate.available");
  }
}

/** A modeless window over the workbench with its title bar close button; the newest-clicked one
 *  is on top. */
function frame(title: string, cls: string, key: string, onClose: () => void): { box: HTMLElement; close: () => void } {
  const box = el("section", `modal-box update-window ${cls}`);
  box.dataset.key = key;
  box.dataset.freeWindow = "";
  box.tabIndex = -1;
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "false");
  box.setAttribute("aria-label", title);
  const head = el("div", "modal-title update-title", title);
  const x = button("×", `${key}.close`, () => close());
  x.className = "update-close";
  x.setAttribute("aria-label", product("close"));
  head.append(x);
  box.append(head);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    box.remove();
    onClose();
  };
  box.addEventListener("keydown", (e) => e.stopPropagation());
  box.addEventListener("pointerdown", () => {
    if (box.nextElementSibling) document.body.append(box);
  });
  document.body.append(box);
  return { box, close };
}

// ------------------------------------------------------------------ 检查更新 page

let page: { box: HTMLElement; close: () => void; show: (u: UpdateView) => void } | null = null;

function saveSettings(change: Partial<UpdateSettings>): void {
  const settings = state.view?.update.settings;
  if (settings) void run({ type: "Update", op: { type: "Settings", settings: { ...settings, ...change } } });
}

function radio(name: string, text: string, key: string, onPick: () => void): { label: HTMLLabelElement; input: HTMLInputElement } {
  const input = el("input");
  input.type = "radio";
  input.name = name;
  input.onchange = () => { if (input.checked) onPick(); };
  const label = el("label", "update-choice");
  label.dataset.key = key;
  label.append(input, text);
  return { label, input };
}

function openPage(): void {
  if (page) {
    document.body.append(page.box);
    page.box.focus();
    return;
  }
  const { box, close } = frame(t("WindowsUpdate.page.title"), "update-page", "update.page", () => { page = null; });
  const body = el("div", "update-body");
  const versionLabel = el("div", "update-muted", t("WindowsUpdate.currentVersion"));
  const versionValue = el("div", "update-version");
  versionValue.dataset.key = "update.version";

  const stable = radio("update-channel", t("WindowsUpdate.channel.stable"), "update.channel.stable", () => saveSettings({ channel: "Stable" }));
  const beta = radio("update-channel", t("WindowsUpdate.channel.beta"), "update.channel.beta", () => saveSettings({ channel: "Beta" }));
  const official = radio("update-source", t("WindowsUpdate.source.official"), "update.source.official", () => saveSettings({ source: "Official" }));
  const github = radio("update-source", t("WindowsUpdate.source.github"), "update.source.github", () => saveSettings({ source: "Github" }));
  const channelRow = el("div", "update-row");
  channelRow.append(stable.label, beta.label);
  const sourceRow = el("div", "update-row");
  sourceRow.append(official.label, github.label);

  const check = (key: string, onChange: (on: boolean) => void) => {
    const input = el("input");
    input.type = "checkbox";
    input.onchange = () => onChange(input.checked);
    const text = el("span");
    const label = el("label", "update-check");
    label.dataset.key = key;
    label.append(input, text);
    return { label, input, text };
  };
  const autoCheck = check("update.autoCheck", (on) => saveSettings(on ? { autoCheck: true } : { autoCheck: false, autoDownload: false }));
  const autoDownload = check("update.autoDownload", (on) => saveSettings({ autoDownload: on }));
  const result = el("div", "update-result");
  result.dataset.key = "update.result";
  result.setAttribute("aria-live", "polite");

  body.append(
    versionLabel, versionValue,
    el("div", "update-label", t("WindowsUpdate.page.channel")), channelRow,
    el("div", "update-label", t("WindowsUpdate.page.source")), sourceRow,
    autoCheck.label, autoDownload.label, result,
  );

  const footer = el("div", "modal-buttons update-buttons");
  const cancel = button(t("WindowsUpdate.btnCancel"), "update.cancel", () => void run({ type: "Update", op: { type: "Cancel" } }));
  const pause = button("", "update.pause", () => void run({ type: "Update", op: { type: "TogglePause" } }));
  const retry = button(t("WindowsUpdate.btnResume"), "update.retry", () => void run({ type: "Update", op: { type: "Retry" } }));
  const viewOffer = button(t("WindowsUpdate.viewOffer"), "update.viewOffer", () => openOffer(true));
  const later = button(t("WindowsUpdate.later"), "update.later", () => close());
  const checkNow = button(t("WindowsUpdate.btnCheck"), "update.check", () => void run({ type: "Update", op: { type: "Check" } }));
  checkNow.classList.add("primary");
  footer.append(cancel, pause, retry, viewOffer, later, checkNow);
  box.append(body, footer);
  // The page's default button (getRootPane().setDefaultButton(checkButton)).
  box.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement) && !checkNow.disabled) {
      e.preventDefault();
      checkNow.click();
    }
  });

  const show = (u: UpdateView) => {
    versionValue.textContent = u.current_version || "-";
    const s = u.settings;
    stable.input.checked = s.channel === "Stable";
    beta.input.checked = s.channel === "Beta";
    // 测试 always reads GitHub (refreshSourceState).
    const isStable = s.channel === "Stable";
    official.input.disabled = github.input.disabled = !isStable;
    official.label.classList.toggle("disabled", !isStable);
    github.label.classList.toggle("disabled", !isStable);
    official.input.checked = isStable && s.source === "Official";
    github.input.checked = !isStable || s.source === "Github";
    autoCheck.input.checked = s.autoCheck;
    autoDownload.input.checked = s.autoDownload;
    autoDownload.input.disabled = !s.autoCheck;
    autoDownload.label.classList.toggle("disabled", !s.autoCheck);
    autoCheck.text.textContent = s.autoCheck && !u.auto_check_active ? t("WindowsUpdate.autoCheckPending") : t("WindowsUpdate.autoCheck");
    autoDownload.text.textContent = s.autoDownload && !u.auto_download_active ? t("WindowsUpdate.autoDownloadPending") : t("WindowsUpdate.autoDownload");

    const d = u.download;
    const offer = u.result?.offer ?? null;
    checkNow.disabled = u.busy;
    viewOffer.hidden = !offer || downloading(d) || d.phase === "Cancelled" || d.phase === "Failed";
    viewOffer.disabled = u.busy;
    cancel.hidden = pause.hidden = !downloading(d);
    pause.textContent = d.phase === "Paused" ? t("WindowsUpdate.btnResume") : t("WindowsUpdate.btnPause");
    retry.hidden = !offer || (d.phase !== "Cancelled" && d.phase !== "Failed");
    retry.disabled = u.busy;
    if (d.phase !== "None" || u.installing) {
      result.textContent = downloadStatus(u);
    } else if (u.checking) {
      result.textContent = t("WindowsUpdate.checking");
    } else if (!u.result) {
      result.textContent = "";
    } else {
      const extra = warning(u.result);
      result.textContent = extra ? `${feedback(u.result)}\n${extra}` : feedback(u.result);
    }
  };
  page = { box, close, show };
  if (state.view) show(state.view.update);
  box.focus();
}

// ------------------------------------------------------------------ 发现新版本

let offerWindow: { id: number; close: () => void; show: (u: UpdateView) => void } | null = null;

function intro(offer: UpdateOfferView, ready: boolean): string {
  // The NSIS installer closes the program, installs and reopens it (Java's own wording); the app
  // bundle and AppImage are replaced in place before the program restarts.
  if (offer.form === "Nsis") return ready ? t("WindowsUpdate.summary.readyIntro") : t("WindowsUpdate.summary.intro");
  return ready ? product("updateReadyIntroInPlace") : product("updateIntroInPlace");
}

/** WindowsUpdateDialog.openRelease: the failure names the system's reason. */
async function openRelease(): Promise<void> {
  try {
    await invoke("open_update_release");
  } catch (e) {
    const reason = e && typeof e === "object" && "message" in e ? String(e.message) : String(e);
    await message(t("Message.title"), `${t("WindowsUpdate.status.openReleaseFailed")}: ${reason}`, "error");
  }
}

/** Shows the current offer (`openOffer`): `focus` is false for an automatic check's reminder
 *  (setAutoRequestFocus(false)). Viewing grants nothing; only 立即更新 prepares and installs. */
function openOffer(focus: boolean): void {
  const u = state.view?.update;
  const offer = u?.result?.offer;
  if (!u || !offer) return;
  page?.close();
  if (offerWindow?.id === offer.id) {
    if (focus) offerWindow.show(u);
    return;
  }
  offerWindow?.close();
  const { box, close } = frame(t("WindowsUpdate.title"), "update-offer", "update.offer", () => { offerWindow = null; });
  const body = el("div", "update-body");
  const heading = el("div", "update-offer-heading", `LizzieYzy Tauri ${offer.version}`);
  const current = el("div", "", `${t("WindowsUpdate.currentVersion")}: ${offer.current_version}`);
  const extra = el("div", "update-warning");
  const summary = el("div", "update-summary");
  summary.dataset.key = "update.summary";
  const bar = el("div", "update-progress");
  bar.dataset.key = "update.progress";
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-label", t("WindowsUpdate.accessibility.progress"));
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  const fill = el("div", "update-progress-fill");
  const percent = el("span", "update-progress-text");
  bar.append(fill, percent);
  const status = el("div", "update-status");
  status.dataset.key = "update.status";
  status.setAttribute("aria-label", t("WindowsUpdate.accessibility.status"));
  status.setAttribute("aria-live", "polite");
  body.append(heading, current, extra, summary, bar, status);

  const footer = el("div", "modal-buttons update-buttons");
  const release = button(t("WindowsUpdate.btnRelease"), "update.release", () => void openRelease());
  const closeButton = button(t("WindowsUpdate.btnLater"), "update.offerClose", () => close());
  const cancel = button(t("WindowsUpdate.btnCancel"), "update.offerCancel", () => void run({ type: "Update", op: { type: "Cancel" } }));
  const pause = button("", "update.offerPause", () => void run({ type: "Update", op: { type: "TogglePause" } }));
  const update = button(t("WindowsUpdate.btnUpdate"), "update.updateNow", () => void run({ type: "Update", op: { type: "Update", offer: offer.id } }));
  update.classList.add("primary");
  footer.append(release, closeButton, cancel, pause, update);
  box.append(body, footer);

  const show = (v: UpdateView) => {
    const shown = v.result?.offer;
    // A newer check replaced this offer: its window goes away (`!current → dispose`).
    if (!shown || shown.id !== offer.id) {
      close();
      return;
    }
    const d = v.download;
    const ready = d.phase === "Ready";
    extra.textContent = v.result ? warning(v.result) : "";
    extra.hidden = !extra.textContent;
    summary.textContent = `${intro(offer, ready)}\n\n${t("WindowsUpdate.summary.package")}: ${offer.asset_name}\n${t("WindowsUpdate.summary.totalDownload")}: ${formatBytes(offer.size)}\n\n${product("updatePreserved")}`;
    update.disabled = v.busy;
    cancel.disabled = pause.disabled = !downloading(d);
    pause.textContent = d.phase === "Paused" ? t("WindowsUpdate.btnResume") : t("WindowsUpdate.btnPause");
    bar.hidden = d.phase === "None";
    const value = d.total > 0 ? Math.min(1000, Math.floor((d.bytes * 1000) / d.total)) : 0;
    fill.style.width = `${value / 10}%`;
    percent.textContent = `${Math.floor(value / 10)}%`;
    bar.setAttribute("aria-valuenow", String(Math.floor(value / 10)));
    const text = downloadStatus(v);
    if (text) status.textContent = text;
  };
  offerWindow = { id: offer.id, close, show };
  show(u);
  if (focus) box.focus();
}

let reminded = 0;
registerViewHandler((view) => {
  const u = view.update;
  page?.show(u);
  offerWindow?.show(u);
  // An offer the core asks to show: a manual check's, or once an automatic one's.
  if (u.remind > reminded) {
    reminded = u.remind;
    openOffer(u.origin !== "Automatic");
  }
});

Object.assign(COMMANDS, {
  // 帮助 > 检查更新 (WindowsUpdateController.openCheckUpdatePage): one page, brought to front.
  "menu.checkUpdate": { run: () => openPage() },
});
