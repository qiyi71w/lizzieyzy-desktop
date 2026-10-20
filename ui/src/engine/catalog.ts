// MoreEngines.java:151-169, 621-877: form above the catalog, immediate catalog operations.
import { run, state } from "../app";
import type { EngineProfile, EngineStartup } from "../contract/generated";
import { chooseSave, confirm, formPanel } from "../dialogs";
import { product, t } from "../i18n";
import { actions, button, checkbox, field, input } from "./controls";

let opened = false;
export async function openMoreEngines(): Promise<void> {
  if (opened || !state.view) return;
  opened = true;
  const resume = state.view.engine.analysing;
  if (resume) await run({ type: "ToggleAnalysis" });
  try {
    let engines = state.view.settings.engine.engines.map((entry) => ({ ...entry }));
    let startup = state.view.settings.engine.startup;
    let selected = engines.length ? 0 : -1;
    const body = document.createElement("div");
    body.className = "engine-catalog";
    const form = document.createElement("div");
    form.className = "engine-form";
    const name = input("");
    const command = document.createElement("textarea");
    command.rows = 5;
    const initial = input("");
    initial.placeholder = t("MoreEngines.initialCommandHint");
    const width = input(19, "number"); width.min = "2"; width.max = "52";
    const height = input(19, "number"); height.min = "2"; height.max = "52";
    const komi = input(7.5, "number"); komi.step = "0.5";
    const preload = checkbox(t("MoreEngines.lblpreload"), false);
    const defaultEngine = checkbox(t("MoreEngines.lbldefault"), false);
    field(form, t("MoreEngines.lblName"), name);
    field(form, t("MoreEngines.lblCommand"), command);
    field(form, t("MoreEngines.lblInitialCommand"), initial);
    actions(form, preload.label, defaultEngine.label);
    const size = document.createElement("div"); size.className = "engine-size";
    field(size, t("MoreEngines.lblWidth"), width);
    field(size, t("MoreEngines.lblHeight"), height);
    field(size, t("MoreEngines.lblKomi"), komi);
    form.append(size);
    actions(form, button(t("EngineThreadPolicy.cfg"), () => undefined, false), button(t("EngineThreadPolicy.benchmark"), () => undefined, false), button(t("EngineThreadPolicy.startBenchmark"), () => undefined, false), button(t("MoreEngines.scan"), () => undefined, false), button(t("MoreEngines.gtpConfig"), () => undefined, false));
    const unavailable = document.createElement("small"); unavailable.className = "engine-unavailable-note";
    unavailable.textContent = product("unavailable"); form.append(unavailable);
    body.append(form);
    const tableWrap = document.createElement("div"); tableWrap.className = "engine-table-wrap";
    const table = document.createElement("table"); table.className = "engine-table";
    tableWrap.append(table);
    // Fields this form does not edit (resource ownership, thread policy, GTP configuration) stay.
    const read = (): EngineProfile | null => selected < 0 ? null : ({
      ...engines[selected], name: name.value, command: command.value, initialCommand: initial.value,
      preload: preload.input.checked, isDefault: defaultEngine.input.checked,
      width: Number(width.value), height: Number(height.value), komi: Number(komi.value),
    });
    const persist = async (): Promise<boolean> => {
      const result = await run({ type: "SaveEngineSettings", engines, startup });
      if (!result || result.type === "Refused") return false;
      // IDs for new entries are assigned by Rust; use the acknowledged published catalog.
      const saved = state.view?.settings.engine.engines;
      if (saved?.length === engines.length) engines = saved.map((entry) => ({ ...entry }));
      return true;
    };
    const save = async (): Promise<boolean> => {
      const previous = engines.map((entry) => ({ ...entry }));
      const draft = read();
      if (draft) {
        if (![name, width, height, komi].every((el) => el.reportValidity())) return false;
        engines[selected] = draft;
        if (draft.isDefault) engines.forEach((entry, index) => { entry.isDefault = index === selected; });
      }
      const saved = await persist();
      if (!saved) engines = previous;
      return saved;
    };
    const dirty = async (): Promise<boolean> => {
      const draft = read();
      if (!draft || JSON.stringify(draft) === JSON.stringify(engines[selected])) return true;
      if (await chooseSave(t("MoreEngines.title"), t("MoreEngines.saveHint3"), t("MoreEngines.saveHint"), t("MoreEngines.saveHint2"))) return save();
      return true;
    };
    const load = () => {
      const entry = engines[selected];
      for (const control of [name, command, initial, width, height, komi, preload.input, defaultEngine.input]) control.disabled = !entry;
      name.value = entry?.name ?? ""; command.value = entry?.command ?? ""; initial.value = entry?.initialCommand ?? "";
      width.value = String(entry?.width ?? 19); height.value = String(entry?.height ?? 19); komi.value = String(entry?.komi ?? 7.5);
      preload.input.checked = entry?.preload ?? false; defaultEngine.input.checked = entry?.isDefault ?? false;
      table.replaceChildren();
      const header = table.createTHead().insertRow();
      for (const text of [t("MoreEngines.column0"), t("MoreEngines.column1"), t("MoreEngines.column2"), t("MoreEngines.column3"), t("MoreEngines.column4"), t("MoreEngines.column5"), t("MoreEngines.column6"), t("MoreEngines.column7")]) {
        const th = document.createElement("th"); th.textContent = text; header.append(th);
      }
      const rows = table.createTBody();
      engines.forEach((engine, index) => {
        const row = rows.insertRow(); row.tabIndex = 0; row.classList.toggle("selected", index === selected);
        for (const value of [index + 1, engine.name, engine.command, engine.preload ? "✓" : "", engine.width, engine.height, engine.komi, engine.isDefault ? "✓" : ""]) row.insertCell().textContent = String(value);
        row.onclick = async () => { if (index !== selected && await dirty()) { selected = index; load(); } };
        row.onkeydown = (event) => { if (event.key === "Enter") row.click(); };
      });
    };
    const reorder = async (target: number) => {
      if (selected < 0 || !(await dirty())) return;
      const next = Math.max(0, Math.min(engines.length - 1, target));
      engines.splice(next, 0, engines.splice(selected, 1)[0]); selected = next;
      await persist(); load();
    };
    actions(body,
      button(t("MoreEngines.add"), async () => {
        if (!(await dirty())) return;
        engines.unshift({ id: "", name: t("MoreEngines.newEngine"), command: "", initialCommand: "", preload: false, isDefault: engines.length === 0, width: 19, height: 19, komi: 7.5 });
        selected = 0; await persist(); load();
      }),
      button(t("MoreEngines.delete"), async () => {
        if (selected < 0 || !(await confirm(t("MoreEngines.deleteHint6"), t("MoreEngines.deleteHint5")))) return;
        engines.splice(selected, 1); selected = Math.min(selected, engines.length - 1); await persist(); load();
      }),
      button(t("MoreEngines.moveFirst"), () => reorder(0)), button(t("MoreEngines.moveLast"), () => reorder(engines.length - 1)),
      button(t("MoreEngines.moveUp"), () => reorder(selected - 1)), button(t("MoreEngines.moveDown"), () => reorder(selected + 1)),
      button(t("MoreEngines.moveUp5"), () => reorder(selected - 5)), button(t("MoreEngines.moveDown5"), () => reorder(selected + 5)),
      button(t("MoreEngines.save"), async () => { if (await save()) load(); }));
    body.append(tableWrap);
    const radios = document.createElement("div"); radios.className = "engine-actions"; radios.append(t("ChooseMoreEngine.lblchooseStart"));
    for (const [value, text] of [["Default", t("MoreEngines.lblrdoDefault")], ["Last", t("ChooseMoreEngine.lblrdoLast")], ["Empty", t("ChooseMoreEngine.lblrdoNone")], ["Manual", t("ChooseMoreEngine.lblrdoMannul")]] as const) {
      const radio = input(value, "radio"); radio.name = "engine-startup"; radio.checked = startup === value;
      radio.onchange = async () => { startup = value satisfies EngineStartup; await persist(); };
      const label = document.createElement("label"); label.append(radio, text); radios.append(label);
    }
    body.append(radios);
    const dialog = formPanel(t("MoreEngines.title"), body, dirty);
    actions(body, button(t("MoreEngines.exit"), () => dialog.close()));
    load();
    await dialog.closed;
  } finally {
    opened = false;
    if (resume && state.view?.engine.foreground && state.view.engine.paused) await run({ type: "ToggleAnalysis" });
  }
}

export async function chooseEngine(manual = false): Promise<void> {
  const engines = state.view?.engine.engines ?? [];
  const body = document.createElement("div"); body.className = "engine-chooser";
  const select = document.createElement("select"); select.size = Math.min(16, Math.max(3, engines.length));
  engines.forEach((engine, index) => { const option = new Option(`[${index + 1}] ${engine.name}    ${engine.command}`, String(index)); select.add(option); });
  body.append(select);
  const dialog = formPanel(manual ? t("loadEngine.title") : t("ChooseMoreEngine.title"), body);
  const load = async () => { if (select.selectedIndex < 0) return; await run({ type: "SwitchEngine", index: Number(select.value) }); await dialog.close(); };
  select.ondblclick = load;
  actions(body, button(manual ? t("loadEngine.ok") : t("ChooseMoreEngine.ok"), load), button(t("loadEngine.noEngine"), () => dialog.close()));
  await dialog.closed;
}
