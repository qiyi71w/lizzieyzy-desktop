// 打开在线链接 (OnlineDialog.java:262-420): an application-modal 730x172 dialog with the prompt, the
// 链接 field (the clipboard's text on every open, all selected when focused), 刷新 seconds (digits
// only, 1 by default) and the hidden red 无效的链接; 确定 (default) follows the link, 中断 stops the
// follow, 取消 hides. One dialog for the session: its fields and error keep their state between
// opens like the Java instance. The core decides what a link is and reports a rejected one
// (`online.dialog_error`), which shows the dialog again with the error (error(true), 474-481).
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run } from "../app";
import type { OnlineCommand, WorkbenchView } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { button, digitsOnly, fieldValue } from "../display/dialogs";
import { actions } from "../engine/controls";
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

function span(text: string, cls = ""): HTMLSpanElement {
  const l = document.createElement("span");
  l.className = cls;
  l.textContent = text;
  return l;
}

function show(): void {
  if (shownError) {
    shownError.hidden = !errorShown;
    return;
  }
  // The prompt, then 链接 and 刷新 with their fields and the error, then 确定 / 中断 / 取消.
  const body = document.createElement("div");
  body.className = "online-form";
  const grid = document.createElement("div");
  grid.className = "online-grid";
  const urlField = document.createElement("input");
  urlField.type = "text";
  urlField.value = url;
  urlField.spellcheck = false;
  urlField.dataset.key = "OnlineDialog.url";
  urlField.setAttribute("aria-label", t("OnlineDialog.title.url"));
  urlField.addEventListener("focus", () => urlField.select());
  const refreshField = document.createElement("input");
  refreshField.type = "text";
  refreshField.value = refresh;
  refreshField.dataset.key = "OnlineDialog.refresh";
  refreshField.setAttribute("aria-label", t("OnlineDialog.title.refresh"));
  digitsOnly(refreshField);
  const error = span(t("OnlineDialog.lblError.text"), "online-error");
  error.dataset.key = "OnlineDialog.error";
  error.hidden = !errorShown;
  const refreshRow = document.createElement("div");
  refreshRow.className = "online-refresh";
  refreshRow.append(refreshField, span(t("OnlineDialog.title.refreshTime")), error);
  grid.append(
    span(t("OnlineDialog.lblPrompt1.text"), "online-prompt"),
    span(t("OnlineDialog.title.url")), urlField,
    span(t("OnlineDialog.title.refresh")), refreshRow,
  );
  body.append(grid);
  const ok = button(t("OnlineDialog.button.ok"));
  ok.className = "primary";
  const interrupt = button(t("OnlineDialog.button.interrupt"));
  const cancel = button(t("OnlineDialog.button.cancel"));
  ok.dataset.key = "OnlineDialog.ok";
  interrupt.dataset.key = "OnlineDialog.interrupt";
  cancel.dataset.key = "OnlineDialog.cancel";
  actions(body, ok, interrupt, cancel);

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
  ok.onclick = apply;
  // stopSync (OnlineDialog.java:4460-4504): the follow stops, the link field empties and the
  // dialog hides.
  interrupt.onclick = () => {
    urlField.value = "";
    void dialog.close();
    void online({ type: "Stop" });
  };
  cancel.onclick = () => void dialog.close();
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
