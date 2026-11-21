// 打开在线链接 (OnlineDialog.java:262-420): an application-modal dialog with one group of rows, the
// 链接 field (the clipboard's text on every open, all selected when focused) under its prompt and
// 刷新 seconds (digits only, 1 by default); a rejected link marks the field and says 无效的链接 under
// the prompt. The footer: 确定 (default) follows the link, 中断 stops the follow, 取消 hides. One
// dialog for the session: its fields and error keep their state between opens like the Java
// instance. The core decides what a link is and reports a rejected one (`online.dialog_error`),
// which shows the dialog again with the error (error(true), 474-481).
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run } from "../app";
import type { OnlineCommand, WorkbenchView } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { digitsOnly, fieldValue } from "../display/dialogs";
import { button, footer, primaryButton, row } from "../engine/controls";
import { t } from "../i18n";
import "./online.css";

const online = (op: OnlineCommand) => run({ type: "Online", op });

/** The Java instance's fields. */
let url = "";
let refresh = "1";
let errorShown = false;
/** The open dialog's error label, while it is shown. */
let shownError: HTMLElement | null = null;
/** The follow was started from this dialog (OnlineDialog.fromBrowser false): a rejected link
 *  shows the dialog again. 弈客直播中心 and the web window start theirs through applyChangeWeb
 *  (fromBrowser true), whose errors leave the hidden dialog alone. */
let fromDialog = false;

export function followedElsewhere(): void {
  fromDialog = false;
}

/** Shows or hides 无效的链接 under the link's prompt, with the link field marked invalid. */
function markError(error: HTMLElement, field: HTMLInputElement): void {
  error.hidden = !errorShown;
  field.classList.toggle("invalid", errorShown);
  if (errorShown) field.setAttribute("aria-invalid", "true");
  else field.removeAttribute("aria-invalid");
}

function show(): void {
  if (shownError) {
    markError(shownError, shownError.closest(".ds-row")!.querySelector("input")!);
    return;
  }
  // 链接 (its prompt and the error under its name, the field across the row) and 刷新, then the
  // footer.
  const body = document.createElement("div");
  body.className = "modal-body online-form";
  const urlField = document.createElement("input");
  urlField.type = "text";
  urlField.value = url;
  urlField.spellcheck = false;
  urlField.dataset.key = "OnlineDialog.url";
  urlField.addEventListener("focus", () => urlField.select());
  const refreshField = document.createElement("input");
  refreshField.type = "text";
  refreshField.inputMode = "numeric";
  refreshField.value = refresh;
  refreshField.dataset.key = "OnlineDialog.refresh";
  digitsOnly(refreshField);
  const urlRow = row(body, t("OnlineDialog.title.url"), t("OnlineDialog.lblPrompt1.text"), urlField);
  urlRow.classList.add("online-url");
  const error = document.createElement("span");
  error.className = "ds-row-sub online-error";
  error.textContent = t("OnlineDialog.lblError.text");
  error.dataset.key = "OnlineDialog.error";
  error.setAttribute("role", "alert");
  urlRow.firstElementChild!.append(error);
  markError(error, urlField);
  // A rejected link's mark and reason go as soon as the link is edited.
  urlField.addEventListener("input", () => {
    if (!errorShown) return;
    errorShown = false;
    markError(error, urlField);
  });
  row(body, t("OnlineDialog.title.refresh"), "", refreshField, t("OnlineDialog.title.refreshTime"));
  const ok = primaryButton(t("OnlineDialog.button.ok"), () => apply());
  const interrupt = button(t("OnlineDialog.button.interrupt"), () => {
    // stopSync (OnlineDialog.java:4460-4504): the follow stops, the link field empties and the
    // dialog hides.
    urlField.value = "";
    void dialog.close();
    void online({ type: "Stop" });
  });
  const cancel = button(t("OnlineDialog.button.cancel"), () => void dialog.close());
  ok.dataset.key = "OnlineDialog.ok";
  interrupt.dataset.key = "OnlineDialog.interrupt";
  cancel.dataset.key = "OnlineDialog.cancel";
  footer(body, { primary: ok, others: [interrupt], cancel });

  const dialog = formPanel(t("OnlineDialog.title.config"), body);
  shownError = error;
  const keep = () => {
    url = urlField.value;
    refresh = refreshField.value;
  };
  void dialog.closed.then(() => {
    keep();
    shownError = null;
  });

  /** applyChange: the core reads the link; a rejected one comes back as dialog_error. */
  const apply = () => {
    keep();
    errorShown = false;
    fromDialog = true;
    void dialog.close();
    // Utils.txtFieldValue; the core reads 0 (blank) as 10 seconds.
    void online({ type: "Open", url: urlField.value, refresh_seconds: fieldValue(refreshField) });
  };
  // getRootPane().setDefaultButton(okButton): Enter anywhere applies.
  body.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      apply();
    }
  });
}

/** LizzieFrame.openOnlineDialog: the follow stops, the clipboard's text fills the link field and
 *  the dialog shows. Text-less clipboard contents read as "" (paste, OnlineDialog.java:402-419). */
export async function openOnlineDialog(): Promise<void> {
  if (shownError) return;
  void online({ type: "Stop" });
  url = await invoke<string>("read_clipboard_text").catch(() => "");
  show();
}

/** dialog_error / proxy_warning of the previous view; the first view only sets them. */
let lastError: number | null = null;
let lastProxy: number | null = null;

registerViewHandler((v: WorkbenchView) => {
  const o = v.online;
  if (lastError !== null && o.dialog_error !== lastError && fromDialog) {
    errorShown = true;
    show();
  }
  lastError = o.dialog_error;
  // showNetworkProxyConfigWarning (OnlineDialog.java:483-492): JOptionPane WARNING with Java's
  // hard-coded title.
  const proxy = o.proxy_warning;
  if (proxy && lastProxy !== null && proxy.seq !== lastProxy) void message("网络代理设置", proxy.message, "warning");
  lastProxy = proxy?.seq ?? (lastProxy ?? 0);
});

/** What the acceptance probe reports: whether the dialog is shown and its error label. */
export function onlineDialogReport() {
  return { shown: shownError !== null, error: shownError ? !shownError.hidden : errorShown, url, refresh };
}
