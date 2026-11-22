// GetEngineLine.java:57-82,111-148. The shell owns native file choosers.
import { run } from "../app";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { button, footer, input, primaryButton, row, segmented } from "./controls";

export async function scanEngine(): Promise<string | null> {
  const body = document.createElement("div");
  body.className = "engine-scan";
  const prompt = document.createElement("p"); prompt.textContent = t("MoreEngines.chooseTypeTitle"); body.append(prompt);
  let type: string | null = null;
  const dialog = formPanel(t("MoreEngines.chooseType"), body);
  // The engine types are the dialog's options, each ending it, in its footer.
  const choices = [["KataGo", "KataGo"], ["LeelaZero", "LeelaZero"], ["ikatago", t("MoreEngines.ikatago")], ["other", t("MoreEngines.other")]].map(([value, label]) => {
    const control = button(label, () => { type = value; void dialog.close(); });
    control.dataset.key = `scan.type.${value}`;
    return control;
  });
  footer(body, { others: choices });
  await dialog.closed;
  if (type === null) return null;
  let result;
  if (type === "ikatago") {
    // Java's captions end in a colon; a row's name does not.
    const caption = (text: string) => text.replace(/[:：]\s*$/, "");
    const credentials = document.createElement("div"); credentials.className = "engine-scan-credentials";
    const platform = segmented("scan-ikatago-platform", [["colab", "Colab"], ["all", t("MoreEngines.otherPlatform")]], "colab");
    platform.el.dataset.key = "scan.ikatago.platform";
    platform.inputs.forEach((radio) => (radio.dataset.key = `scan.ikatago.platform.${radio.value}`));
    const user = input(""); user.autocomplete = "off"; user.dataset.key = "scan.ikatago.username";
    const password = input("", "password"); password.autocomplete = "off"; password.dataset.key = "scan.ikatago.password";
    row(credentials, caption(t("MoreEngines.choosePlatformTitle")), "", platform.el);
    row(credentials, caption(t("MoreEngines.ikatagoUserName")), "", user);
    row(credentials, caption(t("MoreEngines.ikatagoPassWord")), "", password);
    let accepted = false;
    const prompt = formPanel(t("MoreEngines.choosePlatform"), credentials);
    const accept = primaryButton(t("MoreEngines.scan"), () => { accepted = true; void prompt.close(); }); accept.dataset.key = "scan.ikatago.accept";
    footer(credentials, { primary: accept, cancel: button(t("MoreEngines.cancel"), () => prompt.close()) });
    await prompt.closed;
    if (!accepted) return null;
    result = await run({ type: "Tuning", op: { type: "ScanIkatago", platform: platform.value(), user_name: user.value, password: password.value } });
    password.value = "";
  } else {
    result = await run({ type: "Tuning", op: { type: "Scan", engine_type: type, analysis: false } });
  }
  return result?.type === "Tuning" && result.result.type === "Scanned" ? result.result.command : null;
}
