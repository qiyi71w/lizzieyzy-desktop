// 设置Leela(Sai)引擎高级参数 (SetLeelaEngines.java): the Leela Zero / Sai branch of 引擎参数
// (LizzieFrame.setLzSaiEngine). A row per parameter: its name with Java's range hint under it and
// 自动加载 under the hint; the value as typed (not checked, like Java) and 修改 at the end.
import { run, state } from "../app";
import type { LeelaOptionSetting, LeelaOptionSettings } from "../contract/generated";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { button, checkbox, footer, groupNote, input, primaryButton, row, switchBox } from "./controls";

type RowKey = keyof LeelaOptionSettings;

/** One row's controls. */
interface Row { edit: HTMLInputElement; value: HTMLInputElement; autoLoad: HTMLInputElement }

/** A Java parameter label as a row's name and description: the text before the first bracket,
 *  and the bracketed range hints joined ("最大计算量(Visits)(最小0 …):" → "最大计算量",
 *  "Visits · 最小0 …"). Shared with SetKataEngines. */
export function paramCaption(label: string): [name: string, hint: string] {
  const at = label.search(/[(（]/);
  if (at < 0) return [label.replace(/\\?[:：]\s*$/, "").trim(), ""];
  const hints = [...label.slice(at).matchAll(/[(（]([^)）]*)[)）]/g)].map((match) => match[1].trim());
  return [label.slice(0, at).trim(), hints.join(" · ")];
}

/** SetLeelaEngines.java:155-184: 修改 makes the value editable and enables 自动加载, which keeps
 *  its tick while disabled. */
function paramRow(host: HTMLElement, key: RowKey, label: string, saved: LeelaOptionSetting): Row {
  const [name, hint] = paramCaption(label);
  const autoLoad = checkbox(t("SetLeelaEngines.lblAutoLoad"), saved.autoLoad);
  const r: Row = { edit: switchBox(saved.enabled), value: input(saved.value), autoLoad: autoLoad.input };
  r.edit.dataset.key = `leelaOptions.${key}Edit`;
  r.value.dataset.key = `leelaOptions.${key}`;
  r.autoLoad.dataset.key = `leelaOptions.${key}AutoLoad`;
  r.value.inputMode = "decimal";
  r.edit.setAttribute("aria-label", name);
  r.value.setAttribute("aria-label", name);
  const editChanged = () => {
    r.value.disabled = !r.edit.checked;
    r.autoLoad.disabled = !r.edit.checked;
  };
  r.edit.onchange = editChanged;
  editChanged();
  const el = row(host, name, hint, r.value, r.edit);
  const options = document.createElement("span");
  options.className = "engine-param-options";
  options.append(autoLoad.label);
  el.firstElementChild!.append(options);
  return r;
}

export async function openLeelaEngines(): Promise<void> {
  const saved = state.view?.settings.engine.leela_options;
  if (!saved) return;
  const body = document.createElement("div");
  body.className = "engine-params";
  const rows: [RowKey, Row][] = [
    ["memory", paramRow(body, "memory", t("SetLeelaEngines.lblRam"), saved.memory)],
    ["visits", paramRow(body, "visits", t("SetLeelaEngines.lblVisits"), saved.visits)],
    ["lagbuffer", paramRow(body, "lagbuffer", t("SetLeelaEngines.lblLagbuffer"), saved.lagbuffer)],
    ["resign", paramRow(body, "resign", t("SetLeelaEngines.lblResign"), saved.resign)],
  ];
  groupNote(body, t("SetLeelaEngines.lblHint"));

  const dialog = formPanel(t("SetLeelaEngines.title"), body);
  const apply = primaryButton(t("SetLeelaEngines.btnApply"), async () => {
    const options = Object.fromEntries(
      rows.map(([key, r]) => [key, { enabled: r.edit.checked, autoLoad: r.autoLoad.checked, value: r.value.value }]),
    ) as LeelaOptionSettings;
    const result = await run({ type: "SaveLeelaEngineOptions", options });
    if (!result || result.type === "Refused") return;
    await dialog.close();
  });
  apply.dataset.key = "leelaOptions.apply";
  const cancel = button(t("SetLeelaEngines.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "leelaOptions.cancel";
  footer(body, { primary: apply, cancel });
  await dialog.closed;
}
