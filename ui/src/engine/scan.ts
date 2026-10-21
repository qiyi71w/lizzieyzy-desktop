// GetEngineLine.java:57-82,111-148. The shell owns native file choosers.
import { run } from "../app";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { actions, button, field, input } from "./controls";

export async function scanEngine(): Promise<string | null> {
  const body = document.createElement("div");
  body.className = "engine-scan";
  const prompt = document.createElement("p"); prompt.textContent = t("MoreEngines.chooseTypeTitle"); body.append(prompt);
  let type: string | null = null;
  const dialog = formPanel(t("MoreEngines.chooseType"), body);
  for (const [value, label] of [["KataGo", "KataGo"], ["LeelaZero", "LeelaZero"], ["ikatago", t("MoreEngines.ikatago")], ["other", t("MoreEngines.other")]]) {
    const control = button(label, () => { type = value; void dialog.close(); });
    control.dataset.key = `scan.type.${value}`;
    body.append(control);
  }
  await dialog.closed;
  if (type === null) return null;
  let result;
  if (type === "ikatago") {
    const credentials = document.createElement("div"); credentials.className = "engine-scan";
    const platform = document.createElement("select");
    platform.add(new Option("Colab", "colab")); platform.add(new Option(t("MoreEngines.otherPlatform"), "all"));
    platform.dataset.key = "scan.ikatago.platform";
    const user = input(""); user.autocomplete = "off"; user.dataset.key = "scan.ikatago.username";
    const password = input("", "password"); password.autocomplete = "off"; password.dataset.key = "scan.ikatago.password";
    field(credentials, t("MoreEngines.choosePlatformTitle"), platform);
    field(credentials, t("MoreEngines.ikatagoUserName"), user);
    field(credentials, t("MoreEngines.ikatagoPassWord"), password);
    let accepted = false;
    const prompt = formPanel(t("MoreEngines.choosePlatform"), credentials);
    const accept = button(t("MoreEngines.scan"), () => { accepted = true; void prompt.close(); }); accept.dataset.key = "scan.ikatago.accept";
    actions(credentials, accept, button(t("MoreEngines.cancel"), () => prompt.close()));
    await prompt.closed;
    if (!accepted) return null;
    result = await run({ type: "Tuning", op: { type: "ScanIkatago", platform: platform.value, user_name: user.value, password: password.value } });
    password.value = "";
  } else {
    result = await run({ type: "Tuning", op: { type: "Scan", engine_type: type } });
  }
  return result?.type === "Tuning" && result.result.type === "Scanned" ? result.result.command : null;
}
