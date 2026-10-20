// GtpEngineConfigDialog.java:139-191,255-315,341-402; schema owns available settings.
import { run, state } from "../app";
import type { GtpConfiguration } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { t } from "../i18n";
import { actions, button, field, input } from "./controls";

const object = (value: unknown): Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value) ? value as Record<string, unknown> : {};
const fieldLabel = (name: string): string => {
  const labels: Record<string, string> = {
  mode: t("GtpEngineConfig.field.mode"), rankPreset: t("GtpEngineConfig.field.rankPreset"),
  maxTimeSeconds: t("GtpEngineConfig.field.maxTimeSeconds"), maxSimulations: t("GtpEngineConfig.field.maxSimulations"),
  pnLevel: t("GtpEngineConfig.field.pnLevel"), pnWeight: t("GtpEngineConfig.field.pnWeight"), vnMixRate: t("GtpEngineConfig.field.vnMixRate"),
  threads: t("GtpEngineConfig.field.threads"), finalScoreRule: t("GtpEngineConfig.field.finalScoreRule"), resignThreshold: t("GtpEngineConfig.field.resignThreshold"),
  };
  return labels[name] ?? name;
};
const enumLabel = (name: string, value: string): string => {
  const labels: Record<string, string> = {
    "mode.rank": t("GtpEngineConfig.value.mode.rank"), "mode.fixed-time": t("GtpEngineConfig.value.mode.fixed-time"), "mode.advanced": t("GtpEngineConfig.value.mode.advanced"),
    "finalScoreRule.japanese": t("GtpEngineConfig.value.finalScoreRule.japanese"), "finalScoreRule.territory": t("GtpEngineConfig.value.finalScoreRule.territory"),
    "finalScoreRule.chinese": t("GtpEngineConfig.value.finalScoreRule.chinese"), "finalScoreRule.area": t("GtpEngineConfig.value.finalScoreRule.area"),
  };
  if (labels[`${name}.${value}`]) return labels[`${name}.${value}`];
  if (name === "rankPreset" && /^\d+[kd]$/.test(value)) return (value.endsWith("k") ? t("GtpEngineConfig.rank.kyu") : t("GtpEngineConfig.rank.dan")).replace("%s", value.slice(0, -1));
  return value;
};
interface SchemaField { name: string; type: string; group: string; defaultValue: unknown; minimum?: number; maximum?: number; enumValues: string[]; activeWhen: string }
function accepts(f: SchemaField, value: unknown): boolean {
  if (f.type === "string") return typeof value === "string" && (!f.enumValues.length || f.enumValues.includes(value));
  return typeof value === "number" && Number.isFinite(value) && (f.type !== "integer" || Number.isInteger(value)) && (f.minimum === undefined || value >= f.minimum) && (f.maximum === undefined || value <= f.maximum);
}

export async function configureGtp(engineId: string): Promise<void> {
  const result = await run({ type: "Tuning", op: { type: "InspectGtpConfiguration", engine_id: engineId } });
  if (result?.type !== "Tuning" || result.result.type !== "GtpSchema") return;
  if (!result.result.schema) { await message(t("GtpEngineConfig.title"), t("GtpEngineConfig.unsupported")); return; }
  const schema = result.result.schema;
  const fields: SchemaField[] = (Array.isArray(schema.fields) ? schema.fields : []).map((raw: unknown) => {
    const f = object(raw);
    return { name: String(f.name), type: String(f.type), group: String(f.group ?? "advanced"), defaultValue: f.defaultValue,
      minimum: typeof f.minimum === "number" ? f.minimum : undefined, maximum: typeof f.maximum === "number" ? f.maximum : undefined,
      enumValues: Array.isArray(f.enumValues) ? f.enumValues.filter((v): v is string => typeof v === "string") : [], activeWhen: String(f.activeWhen ?? "") };
  });
  const saved = state.view?.settings.engine.engines.find((entry) => entry.id === engineId)?.gtpConfiguration?.profile ?? {};
  const selected = object(object(schema.state).selected), effective = object(object(schema.state).effective);
  const body = document.createElement("div"); body.className = "gtp-config";
  const subtitle = document.createElement("p"); subtitle.textContent = t("GtpEngineConfig.subtitle"); body.append(subtitle);
  const basic = document.createElement("section"), advanced = document.createElement("section"); advanced.hidden = true;
  const heading = document.createElement("h3"); heading.textContent = t("GtpEngineConfig.basic"); body.append(heading, basic);
  const editors = new Map<string, HTMLInputElement | HTMLSelectElement>();
  const values = (): Record<string, unknown> => Object.fromEntries(fields.map((f) => { const el = editors.get(f.name)!; return [f.name, f.type === "string" ? el.value : el.value.trim() ? Number(el.value) : NaN]; }));
  const update = () => {
    const current = values();
    fields.forEach((f) => {
      const at = f.activeWhen.indexOf("=");
      const enabled = at <= 0 || at === f.activeWhen.length - 1 || f.activeWhen.slice(at + 1).split("|").some((v) => v.trim().toLowerCase() === String(current[f.activeWhen.slice(0, at).trim()] ?? "").toLowerCase());
      editors.get(f.name)!.disabled = !enabled;
      editors.get(f.name)!.closest("label")?.classList.toggle("inactive", !enabled);
    });
  };
  for (const f of fields) {
    let editor: HTMLInputElement | HTMLSelectElement;
    if (f.type === "string" && f.enumValues.length) {
      editor = document.createElement("select"); f.enumValues.forEach((v) => (editor as HTMLSelectElement).add(new Option(enumLabel(f.name, v), v)));
    } else {
      editor = input("", f.type === "string" ? "text" : "number");
      if (f.minimum !== undefined) editor.min = String(f.minimum);
      if (f.maximum !== undefined) editor.max = String(f.maximum);
      editor.step = f.type === "integer" ? "1" : f.maximum !== undefined && f.maximum <= 1 ? "0.01" : "0.5";
    }
    const initial = accepts(f, saved[f.name]) ? saved[f.name] : accepts(f, selected[f.name]) ? selected[f.name] : f.defaultValue;
    editor.value = String(initial); editor.dataset.key = `gtp.field.${f.name}`; editor.required = f.type !== "string";
    editor.oninput = update; editor.onchange = update;
    editors.set(f.name, editor); field(f.group.toLowerCase() === "basic" ? basic : advanced, fieldLabel(f.name), editor);
  }
  const toggle = button(t("GtpEngineConfig.showAdvanced"), () => { advanced.hidden = !advanced.hidden; toggle.textContent = advanced.hidden ? t("GtpEngineConfig.showAdvanced") : t("GtpEngineConfig.hideAdvanced"); }); toggle.dataset.key = "gtp.advanced";
  body.append(toggle, advanced);
  const hint = document.createElement("p"); hint.textContent = t("GtpEngineConfig.nextSearch"); body.append(hint);
  const different = fields.find((f) => selected[f.name] !== undefined && effective[f.name] !== undefined && selected[f.name] !== effective[f.name]);
  if (different) {
    const note = document.createElement("p"); let index = 0;
    note.textContent = t("GtpEngineConfig.effectiveDifference").replace(/%s/g, () => [fieldLabel(different.name), enumLabel(different.name, String(selected[different.name])), enumLabel(different.name, String(effective[different.name]))][index++]); body.append(note);
  }
  const dialog = formPanel(t("GtpEngineConfig.title"), body);
  const defaults = button(t("GtpEngineConfig.restoreDefaults"), () => { fields.forEach((f) => { editors.get(f.name)!.value = String(f.defaultValue); }); update(); }); defaults.dataset.key = "gtp.defaults";
  const cancel = button(t("GtpEngineConfig.cancel"), () => dialog.close()); cancel.dataset.key = "gtp.cancel";
  const save = button(t("GtpEngineConfig.save"), async () => {
    const profile = values(); const invalid = fields.find((f) => !accepts(f, profile[f.name]));
    if (invalid) { await message(t("GtpEngineConfig.invalidValue"), `${t("GtpEngineConfig.invalidField")} ${fieldLabel(invalid.name)}`, "warning"); return; }
    save.disabled = true;
    const configuration: GtpConfiguration = { protocol: String(schema.protocol), profile };
    try {
      const applied = await run({ type: "Tuning", op: { type: "SaveGtpConfiguration", engine_id: engineId, configuration } });
      if (applied?.type === "Tuning" && applied.result.type === "Done") await dialog.close();
    } finally { save.disabled = false; }
  }); save.dataset.key = "gtp.save";
  actions(body, defaults, cancel, save); update(); await dialog.closed;
}
