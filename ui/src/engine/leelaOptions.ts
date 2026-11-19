// 设置Leela(Sai)引擎高级参数 (SetLeelaEngines.java): the Leela Zero / Sai branch of 引擎参数
// (LizzieFrame.setLzSaiEngine). Each row: the label with Java's range hint, 修改, the value as
// typed (not checked, like Java) and 自动加载.
import { run, state } from "../app";
import type { LeelaOptionSetting, LeelaOptionSettings } from "../contract/generated";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { actions, button, primaryButton } from "./controls";

type RowKey = keyof LeelaOptionSettings;

/** One row's controls. */
interface Row { edit: HTMLInputElement; value: HTMLInputElement; autoLoad: HTMLInputElement }

function check(checked: boolean): HTMLInputElement {
  const c = document.createElement("input");
  c.type = "checkbox";
  c.checked = checked;
  return c;
}

/** SetLeelaEngines.java:155-184: 修改 makes the value editable and enables 自动加载, which keeps
 *  its tick while disabled. */
function row(grid: HTMLElement, key: RowKey, label: string, saved: LeelaOptionSetting): Row {
  const r: Row = { edit: check(saved.enabled), value: document.createElement("input"), autoLoad: check(saved.autoLoad) };
  r.value.value = saved.value;
  r.edit.dataset.key = `leelaOptions.${key}Edit`;
  r.value.dataset.key = `leelaOptions.${key}`;
  r.autoLoad.dataset.key = `leelaOptions.${key}AutoLoad`;
  const name = document.createElement("span");
  name.textContent = label;
  const editChanged = () => {
    r.value.disabled = !r.edit.checked;
    r.autoLoad.disabled = !r.edit.checked;
  };
  r.edit.onchange = editChanged;
  editChanged();
  grid.append(name, r.edit, r.value, r.autoLoad);
  return r;
}

export async function openLeelaEngines(): Promise<void> {
  const saved = state.view?.settings.engine.leela_options;
  if (!saved) return;
  const body = document.createElement("div");
  body.className = "kata-params";
  const grid = document.createElement("div");
  grid.className = "kata-params-grid leela-params-grid";
  const head = (text = "") => { const h = document.createElement("span"); h.className = "kata-params-head"; h.textContent = text; return h; };
  grid.append(head(), head(), head(), head(t("SetLeelaEngines.lblAutoLoad")));
  const rows: [RowKey, Row][] = [
    ["memory", row(grid, "memory", t("SetLeelaEngines.lblRam"), saved.memory)],
    ["visits", row(grid, "visits", t("SetLeelaEngines.lblVisits"), saved.visits)],
    ["lagbuffer", row(grid, "lagbuffer", t("SetLeelaEngines.lblLagbuffer"), saved.lagbuffer)],
    ["resign", row(grid, "resign", t("SetLeelaEngines.lblResign"), saved.resign)],
  ];
  const hint = document.createElement("div");
  hint.className = "engine-hint";
  hint.textContent = t("SetLeelaEngines.lblHint");
  body.append(grid, hint);

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
  actions(body, apply, cancel);
  await dialog.closed;
}
