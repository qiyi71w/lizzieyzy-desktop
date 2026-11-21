// MoreEngines.java:151-169, 621-877: the selected entry's fields above the catalog, immediate
// catalog operations.
import { run, state } from "../app";
import type { EngineProfile, EngineStartup } from "../contract/generated";
import { chooseSave, confirm, formPanel } from "../dialogs";
import { product, t } from "../i18n";
import { openAutoSetup } from "../setup/dialog";
import { onTuningChange, tuningView } from "../setup/performance";
import { configureGtp } from "./gtpConfig";
import { scanEngine } from "./scan";
import { tuningText } from "./tuningText";
import { stepper } from "../design/stepper";
import { actions, button, footer, input, primaryButton, row, switchBox, switchRow } from "./controls";
import { aboutRemoteButton, RemoteFields } from "./remote";

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
    // Java's labels end in a colon; a row's name does not.
    const caption = (text: string) => text.replace(/\\?[:：]\s*$/, "");
    const radioGroup = (...labels: HTMLLabelElement[]) => {
      const el = document.createElement("span"); el.className = "ds-seg"; el.setAttribute("role", "radiogroup"); el.append(...labels); return el;
    };
    // The selected entry in two columns of groups: what runs (名称, 命令行, 初始命令, its threads)
    // at the start; the board, loading and the SSH connection at the end.
    const columns = document.createElement("div"); columns.className = "engine-catalog-columns";
    const start = document.createElement("div"), end = document.createElement("div");
    const program = document.createElement("div"), threads = document.createElement("div"), board = document.createElement("div"), ssh = document.createElement("div");
    start.append(program, threads); end.append(board, ssh); columns.append(start, end);
    const name = input("");
    const command = document.createElement("textarea");
    command.rows = 3;
    let scanBusy = false, gtpBusy = false;
    const initial = input("");
    initial.placeholder = t("MoreEngines.initialCommandHint");
    const width = input(19, "number"); width.min = "2"; width.max = "52";
    const height = input(19, "number"); height.min = "2"; height.max = "52";
    const komi = input(7.5, "number"); komi.step = "0.5";
    const scan = button(t("MoreEngines.scan"), async () => {
      const entryId = engines[selected]?.id; scanBusy = true; updatePolicy();
      try { const scanned = await scanEngine(); if (scanned !== null && engines[selected]?.id === entryId) { command.value = scanned; updatePolicy(); } }
      finally { scanBusy = false; updatePolicy(); }
    }); scan.dataset.key = "engine.scan";
    const gtp = button(t("MoreEngines.gtpConfig"), async () => {
      const entry = engines[selected]; if (!entry) return;
      gtpBusy = true; updatePolicy(); gtp.textContent = t("GtpEngineConfig.detecting");
      try { await configureGtp(entry.id); }
      finally { gtpBusy = false; gtp.textContent = t("MoreEngines.gtpConfig"); updatePolicy(); }
    }); gtp.dataset.key = "engine.gtpConfig";
    row(program, caption(t("MoreEngines.lblName")), "", name);
    // 命令行 and 初始命令 take a line of their own under the row's name, named after it.
    command.setAttribute("aria-label", caption(t("MoreEngines.lblCommand")));
    initial.setAttribute("aria-label", caption(t("MoreEngines.lblInitialCommand")));
    row(program, caption(t("MoreEngines.lblCommand")), "", scan, gtp).append(command);
    row(program, caption(t("MoreEngines.lblInitialCommand")), "").append(initial);
    row(board, t("MoreEngines.lblWidth"), "", stepper(width));
    row(board, t("MoreEngines.lblHeight"), "", stepper(height));
    row(board, t("MoreEngines.lblKomi"), "", stepper(komi));
    const preload = switchRow(board, t("MoreEngines.lblpreload"), "", false);
    const defaultEngine = switchRow(board, t("MoreEngines.lbldefault"), "", false);
    // 远程引擎 (MoreEngines.java:479-493): the command runs on the SSH host as written.
    const remoteEngine = switchBox(false);
    remoteEngine.dataset.key = "engine.remoteEngine";
    remoteEngine.setAttribute("aria-label", t("MoreEngines.chkRemoteEngine"));
    row(ssh, t("MoreEngines.chkRemoteEngine"), "", aboutRemoteButton(), remoteEngine);
    const remote = new RemoteFields({
      host: "IP", port: t("MoreEngines.lblPort"), user: t("MoreEngines.rdoUserName"),
      password: t("MoreEngines.lblPassword"), key: t("MoreEngines.rdoKeygen"), browse: t("MoreEngines.scanKeygen"),
    }, "engine.remote");
    remote.port.inputMode = "numeric";
    row(ssh, remote.labels.host, "", remote.host);
    row(ssh, remote.labels.port, "", remote.port);
    row(ssh, remote.labels.user, "", remote.user);
    // 密码 / 密钥 is the row's choice; the field or 浏览 it enables stands at the end.
    row(ssh, radioGroup(remote.passwordLabel, remote.keyLabel), "", remote.password, remote.browse);
    remoteEngine.onchange = () => { remote.setEnabled(remoteEngine.checked); updatePolicy(); };
    const cfg = input("CFG", "radio"), benchmark = input("BENCHMARK", "radio");
    cfg.name = benchmark.name = "engine-thread-source";
    cfg.dataset.key = "engine.thread.cfg"; benchmark.dataset.key = "engine.thread.benchmark";
    const cfgLabel = document.createElement("label"); cfgLabel.append(cfg, t("EngineThreadPolicy.cfg"));
    const benchmarkLabel = document.createElement("label"); const benchmarkText = document.createTextNode(""); benchmarkLabel.append(benchmark, benchmarkText);
    const benchmarkButton = button(t("EngineThreadPolicy.startBenchmark"), async () => {
      const entry = engines[selected]; if (!entry) return;
      const draft = read();
      if (JSON.stringify(draft) !== JSON.stringify(entry)) {
        if (!(await chooseSave(t("EngineThreadPolicy.settings"), t("EngineThreadPolicy.benchmarkSavePrompt"), t("EngineThreadPolicy.saveAndBenchmark"), t("MoreEngines.cancel"))) || !(await save())) return;
      }
      await dialog.close();
      await openAutoSetup("performance", { engineId: entry.id });
    }); benchmarkButton.dataset.key = "engine.performance";
    // 按引擎设置线程: CFG or the benchmark's recommendation; what the choice means under the name.
    const threadSource = radioGroup(cfgLabel, benchmarkLabel);
    const policyRow = row(threads, t("EngineThreadPolicy.settings"), "", threadSource, benchmarkButton);
    const policyStatus = document.createElement("span"); policyStatus.className = "ds-row-sub engine-thread-status"; policyRow.firstElementChild!.append(policyStatus);
    const selectSource = async (source: "CFG" | "BENCHMARK") => {
      const entry = engines[selected]; if (!entry) return;
      await run({ type: "Tuning", op: { type: "SetThreadSource", engine_id: entry.id, source } });
      updatePolicy();
    };
    cfg.onchange = () => { if (cfg.checked) void selectSource("CFG"); };
    benchmark.onchange = () => { if (benchmark.checked) void selectSource("BENCHMARK"); };
    const updatePolicy = () => {
      const entry = engines[selected];
      const profile = tuningView()?.profiles.find((item) => item.engineId === entry?.id);
      const policy = profile?.policy;
      const recommendation = policy?.["katago-benchmark-threads"] ?? 0;
      const draftLocal = !!entry && /^(?:"[^"\n]*katago[^"\n]*"|'[^'\n]*katago[^'\n]*'|\S*katago\S*)(?:\s|$)/i.test(command.value.trim()) && !command.value.includes("://");
      const local = draftLocal;
      const remote = remoteEngine.checked || /(?:^|\s)ssh\s|:\/\//i.test(command.value);
      policyRow.hidden = !entry || (!draftLocal && !remote && !!command.value.trim() && !/katago/i.test(command.value));
      threadSource.hidden = benchmarkButton.hidden = remote;
      cfg.checked = !local || policy?.source !== "BENCHMARK"; benchmark.checked = local && policy?.source === "BENCHMARK";
      cfg.disabled = !local;
      benchmark.disabled = !local || (recommendation <= 0 && policy?.source !== "BENCHMARK");
      benchmarkText.textContent = recommendation > 0 ? t("EngineThreadPolicy.benchmarkWithThreads").replace("%d", String(recommendation)) : t("EngineThreadPolicy.benchmarkNotDetected");
      benchmarkButton.textContent = recommendation > 0 ? t("EngineThreadPolicy.rerunBenchmark") : t("EngineThreadPolicy.startBenchmark");
      benchmarkButton.disabled = !entry || !draftLocal;
      policyStatus.textContent = remote ? t("EngineThreadPolicy.remoteManaged") : draftLocal ? [policy?.source === "BENCHMARK" && recommendation <= 0 ? t("EngineThreadPolicy.invalidRecommendation") : policy?.source === "BENCHMARK" ? t("EngineThreadPolicy.benchmarkDescription") : t("EngineThreadPolicy.cfgDescription"), profile?.environmentStatus ? tuningText(profile.environmentStatus) : "", /(?:numSearchThreads\s*=\s*[1-9]|--?threads\s+[1-9])/i.test(command.value) ? t("EngineThreadPolicy.explicitOverride") : ""].filter(Boolean).join(" · ") : "";
      scan.disabled = !entry || scanBusy; gtp.disabled = !entry || gtpBusy;
    };
    command.dataset.key = "engine.command"; command.addEventListener("input", updatePolicy);
    body.append(columns);
    const tableWrap = document.createElement("div"); tableWrap.className = "engine-table-wrap";
    const table = document.createElement("table"); table.className = "engine-table";
    tableWrap.append(table);
    // Fields this form does not edit (resource ownership, thread policy, GTP configuration) stay.
    const read = (): EngineProfile | null => selected < 0 ? null : ({
      ...engines[selected], name: name.value, command: command.value, initialCommand: initial.value,
      preload: preload.input.checked, isDefault: defaultEngine.input.checked,
      width: Number(width.value), height: Number(height.value), komi: Number(komi.value),
      ssh: remoteEngine.checked ? remote.read() : undefined,
    });
    const persist = async (): Promise<boolean> => {
      const result = await run({ type: "SaveEngineSettings", engines, startup });
      if (!result || result.type === "Refused") return false;
      // IDs for new entries are assigned by Rust; use the acknowledged published catalog.
      const saved = state.view?.settings.engine.engines;
      if (saved?.length === engines.length) engines = saved.map((entry) => ({ ...entry }));
      return true;
    };
    // One save at a time, with the form inert until it ends (Java saves on the event thread):
    // a staged password belongs to the save that staged it, and no newer draft is typed while
    // the save's own reload is pending.
    let saving = false;
    const save = async (): Promise<boolean> => {
      if (saving) return false;
      saving = true;
      try {
        const previous = engines.map((entry) => ({ ...entry }));
        const draft = read();
        if (draft) {
          if (![name, width, height, komi].every((el) => el.reportValidity())) return false;
          engines[selected] = draft;
          if (draft.isDefault) engines.forEach((entry, index) => { entry.isDefault = index === selected; });
        }
        // A typed password is staged first, as an engine this save starts looks it up when
        // connecting; a refused or failed save puts the previous one back.
        body.inert = true;
        const settle = draft?.ssh ? await remote.stagePassword() : null;
        const saved = (!draft?.ssh || settle !== null) && await persist();
        await settle?.(saved);
        if (!saved) engines = previous;
        return saved;
      } finally {
        saving = false;
        body.inert = false;
      }
    };
    const dirty = async (): Promise<boolean> => {
      const draft = read();
      if (!draft || (JSON.stringify(draft) === JSON.stringify(engines[selected]) && remote.typedPassword() === null)) return true;
      if (await chooseSave(t("MoreEngines.title"), t("MoreEngines.saveHint3"), t("MoreEngines.saveHint"), t("MoreEngines.saveHint2"))) return save();
      return true;
    };
    const load = () => {
      const entry = engines[selected];
      for (const control of [name, command, initial, width, height, komi, preload.input, defaultEngine.input, remoteEngine]) control.disabled = !entry;
      name.value = entry?.name ?? ""; command.value = entry?.command ?? ""; initial.value = entry?.initialCommand ?? "";
      width.value = String(entry?.width ?? 19); height.value = String(entry?.height ?? 19); komi.value = String(entry?.komi ?? 7.5);
      preload.input.checked = entry?.preload ?? false; defaultEngine.input.checked = entry?.isDefault ?? false;
      remoteEngine.checked = !!entry?.ssh; remote.set(entry?.ssh); remote.setEnabled(!!entry?.ssh);
      updatePolicy();
      table.replaceChildren();
      const header = table.createTHead().insertRow();
      for (const text of [t("MoreEngines.column0"), t("MoreEngines.column1"), t("MoreEngines.column2"), t("MoreEngines.column3"), t("MoreEngines.column4"), t("MoreEngines.column5"), t("MoreEngines.column6"), t("MoreEngines.column7"), t("MoreEngines.column8")]) {
        const th = document.createElement("th"); th.textContent = text; header.append(th);
      }
      const rows = table.createTBody();
      engines.forEach((engine, index) => {
        const row = rows.insertRow(); row.tabIndex = 0; row.classList.toggle("selected", index === selected);
        for (const value of [index + 1, engine.name, engine.command, engine.preload ? "✓" : "", engine.width, engine.height, engine.komi, engine.isDefault ? "✓" : "", engine.ssh ? "✓" : ""]) row.insertCell().textContent = String(value);
        row.onclick = async () => { if (index !== selected && await dirty()) { selected = index; load(); } };
        row.onkeydown = (event) => { if (event.key === "Enter") row.click(); };
      });
    };
    // A catalog edit the core refuses (an active engine game) leaves the table on the catalog the
    // core keeps, as MoreEngines returns on a failed save.
    const edit = async (change: () => void) => {
      const previous = engines, previousSelected = selected;
      engines = engines.map((entry) => ({ ...entry }));
      change();
      if (!(await persist())) { engines = previous; selected = previousSelected; }
      load();
    };
    const reorder = async (target: number) => {
      if (selected < 0 || !(await dirty())) return;
      const next = Math.max(0, Math.min(engines.length - 1, target));
      await edit(() => { engines.splice(next, 0, engines.splice(selected, 1)[0]); selected = next; });
    };
    // 删除 removes the entry from the catalog: the danger button, after its confirmation.
    const remove = button(t("MoreEngines.delete"), async () => {
      if (selected < 0 || !(await confirm(t("MoreEngines.deleteHint6"), t("MoreEngines.deleteHint5")))) return;
      await edit(() => { engines.splice(selected, 1); selected = Math.min(selected, engines.length - 1); });
    });
    remove.classList.add("danger");
    actions(body,
      button(t("MoreEngines.add"), async () => {
        if (!(await dirty())) return;
        await edit(() => {
          engines.unshift({ id: "", name: t("MoreEngines.newEngine"), command: "", initialCommand: "", preload: false, isDefault: engines.length === 0, width: 19, height: 19, komi: 7.5 });
          selected = 0;
        });
      }),
      remove,
      button(t("MoreEngines.moveFirst"), () => reorder(0)), button(t("MoreEngines.moveLast"), () => reorder(engines.length - 1)),
      button(t("MoreEngines.moveUp"), () => reorder(selected - 1)), button(t("MoreEngines.moveDown"), () => reorder(selected + 1)),
      button(t("MoreEngines.moveUp5"), () => reorder(selected - 5)), button(t("MoreEngines.moveDown5"), () => reorder(selected + 5)),
      primaryButton(t("MoreEngines.save"), async () => { if (await save()) load(); }));
    body.append(tableWrap);
    const startupRadios: HTMLInputElement[] = [];
    const startupLabels = ([["Default", t("MoreEngines.lblrdoDefault")], ["Last", t("ChooseMoreEngine.lblrdoLast")], ["Empty", t("ChooseMoreEngine.lblrdoNone")], ["Manual", t("ChooseMoreEngine.lblrdoMannul")]] as const).map(([value, text]) => {
      const radio = input(value, "radio"); radio.name = "engine-startup"; radio.checked = startup === value;
      radio.onchange = async () => {
        const previous = startup;
        startup = value satisfies EngineStartup;
        if (await persist()) return;
        startup = previous;
        for (const other of startupRadios) other.checked = other.value === previous;
      };
      startupRadios.push(radio);
      const label = document.createElement("label"); label.append(radio, text); return label;
    });
    row(body, caption(t("ChooseMoreEngine.lblchooseStart")), "", radioGroup(...startupLabels));
    const dialog = formPanel(t("MoreEngines.title"), body, dirty);
    // Catalog operations apply at once and the entry's fields ask before they are left, so the
    // window only closes.
    const close = button(product("close"), () => void dialog.close());
    close.dataset.key = "engine.close";
    footer(body, { cancel: close });
    load();
    const unsubscribe = onTuningChange(() => {
      const saved = state.view?.settings.engine.engines;
      engines = engines.map((entry) => {
        const updated = saved?.find((item) => item.id === entry.id);
        return updated ? { ...entry, threadPolicy: updated.threadPolicy, gtpConfiguration: updated.gtpConfiguration } : entry;
      });
      updatePolicy();
    });
    void run({ type: "Tuning", op: { type: "Refresh" } });
    await dialog.closed;
    unsubscribe();
  } finally {
    opened = false;
    if (resume && state.view?.engine.foreground && state.view.engine.paused) await run({ type: "ToggleAnalysis" });
  }
}

/** ChooseMoreEngine (and loadEngine at a manual start): picks a catalog entry for the foreground
 *  engine, or with `secondary` for the 双引擎模式 secondary slot (ChooseMoreEngine.createDialog(2)). */
export async function chooseEngine(manual = false, secondary = false): Promise<void> {
  const engines = state.view?.engine.engines ?? [];
  const body = document.createElement("div"); body.className = "engine-chooser";
  const select = document.createElement("select"); select.size = Math.min(16, Math.max(3, engines.length));
  engines.forEach((engine, index) => { const option = new Option(`[${index + 1}] ${engine.name}    ${engine.command}`, String(index)); select.add(option); });
  body.append(select);
  const dialog = formPanel(manual ? t("loadEngine.title") : t("ChooseMoreEngine.title"), body);
  const load = async () => {
    if (select.selectedIndex < 0) return;
    const index = Number(select.value);
    await run(secondary ? { type: "SwitchSecondaryEngine", index } : { type: "SwitchEngine", index });
    await dialog.close();
  };
  select.ondblclick = load;
  footer(body, { primary: primaryButton(manual ? t("loadEngine.ok") : t("ChooseMoreEngine.ok"), load), cancel: button(t("loadEngine.noEngine"), () => dialog.close()) });
  await dialog.closed;
}
