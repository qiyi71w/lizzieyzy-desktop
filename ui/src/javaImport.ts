// Import of LizzieYzy Next (Java) settings: choose the Java settings folder, preview what is taken
// over, left out or not usable, confirm credentials and replacements, then read the report.
// Java has no counterpart; the dialog follows the engine dialogs' Swing form density. The core
// reads and writes; this dialog only holds the user's choices until "导入".
import { run, state } from "./app";
import type {
  EngineProblem,
  ImportEngineView,
  ImportFailure,
  ImportItemView,
  ImportPreviewView,
  ImportReportView,
  ImportStatus,
} from "./contract/generated";
import { formPanel, message } from "./dialogs";
import { product, t } from "./i18n";
import { actions, button, checkbox, input } from "./engine/controls";
import "./javaImport.css";

let opened = false;

/** Help > 从 LizzieYzy Next 导入设置, and the first-use offer (`firstUse`). */
export async function openJavaImport(firstUse = false): Promise<void> {
  if (opened || !state.view) return;
  opened = true;
  const body = document.createElement("div");
  body.className = "modal-body java-import";
  const dialog = formPanel(product("importTitle"), body);
  let declined = firstUse;
  try {
    showSource(body, firstUse, dialog.close, () => (declined = false));
    await dialog.closed;
    // Closing the first-use offer without importing declines it.
    if (declined && state.view?.java_import.offer) await run({ type: "DismissJavaImportOffer" });
  } finally {
    opened = false;
  }
}

function text(tag: string, content: string, className = ""): HTMLElement {
  const el = document.createElement(tag);
  el.textContent = content;
  if (className) el.className = className;
  return el;
}

function failureText(f: ImportFailure): string {
  switch (f.type) {
    case "NoConfig": return product("importNoConfig", f.path);
    case "Invalid": return product("importInvalid", f.path, f.detail);
    case "Unreadable": return product("importUnreadable", f.path, f.detail);
  }
}

/** Step 1: the Java settings folder. */
function showSource(body: HTMLElement, firstUse: boolean, close: () => Promise<void>, started: () => void, folder = "", program: string | null = null): void {
  body.replaceChildren();
  body.append(text("p", firstUse ? product("importFirstUse") : product("importIntro")));
  const path = input(folder || (state.view?.java_import.candidates[0] ?? ""));
  path.className = "java-import-path";
  path.dataset.key = "import-folder";
  const row = document.createElement("div");
  row.className = "java-import-row";
  row.append(text("span", product("importFolder")), path, button(product("browse"), async () => {
    const r = await run({ type: "ChooseJavaFolder", program: false });
    if (r?.type === "FolderChosen") path.value = r.path;
  }));
  body.append(row);
  const candidates = state.view?.java_import.candidates ?? [];
  if (candidates.length) {
    const found = document.createElement("div");
    found.className = "java-import-found";
    found.append(text("span", product("importFound")));
    candidates.forEach((c, k) => {
      const b = button(c, () => { path.value = c; });
      b.dataset.key = `import-candidate-${k}`;
      found.append(b);
    });
    body.append(found);
  }
  const error = text("p", "", "java-import-error");
  body.append(error);
  const preview = button(product("importPreview"), async () => {
    error.textContent = "";
    if (!path.value.trim()) return;
    const r = await run({ type: "PreviewJavaImport", config_dir: path.value.trim(), program_dir: program });
    if (r?.type === "JavaImportFailed") error.textContent = failureText(r.failure);
    else if (r?.type === "JavaImportPreview") showPreview(body, r.preview, firstUse, close, started, path.value.trim());
  });
  preview.dataset.key = "import-preview";
  actions(body, preview, button(firstUse ? product("importSkip") : product("cancel"), close));
  path.focus();
}

function count(items: ImportItemView[], status: ImportStatus): number {
  return items.filter((i) => i.status === status).length;
}

function itemLine(i: ImportItemView): string {
  const parts = [i.key];
  if (i.target) parts.push(`→ ${product(`target${i.target}`)}`);
  if (i.value !== null && !i.credential) parts.push(`= ${i.value.replaceAll("\n", ", ")}`);
  if (i.reason) parts.push(`（${product(`reason${i.reason}`)}）`);
  if (i.detail) parts.push(i.detail);
  return parts.join(" ");
}

function itemLists(host: HTMLElement, items: ImportItemView[], report: boolean): void {
  for (const status of ["Imported", "Incompatible", "NotMigrated"] as const) {
    const list = items.filter((i) => i.status === status);
    if (!list.length) continue;
    const details = document.createElement("details");
    details.dataset.key = `import-list-${status}`;
    details.open = status === "Incompatible";
    details.append(text("summary", `${product(report && status === "Imported" ? "importListImported" : `importList${status}`)} (${list.length})`));
    const ul = document.createElement("ul");
    for (const i of list) ul.append(text("li", itemLine(i), i.credential ? "credential" : ""));
    details.append(ul);
    host.append(details);
  }
}

function problemText(p: EngineProblem): string {
  switch (p.type) {
    case "ProgramNotFound": return product("problemProgramNotFound", p.path);
    case "FileNotFound": return product("problemFileNotFound", p.option, p.path);
    case "EmptyCommand": return product("problemEmptyCommand");
    case "InvalidValue": return product("problemInvalidValue", p.field);
  }
}

/** The engine table; with `choose`, rows with credentials get the "复制凭据" box. */
function engineTable(host: HTMLElement, engines: ImportEngineView[], choose: Map<number, HTMLInputElement> | null): void {
  if (!engines.length) return;
  host.append(text("div", product("importEngines"), "java-import-section"));
  const wrap = document.createElement("div");
  wrap.className = "engine-table-wrap java-import-engines";
  const table = document.createElement("table");
  table.className = "engine-table";
  const head = document.createElement("tr");
  for (const h of ["#", product("importEngineName"), product("importEngineCommand"), product("importEngineStatus"), product("importEngineProblems")]) head.append(text("th", h));
  table.append(head);
  for (const e of engines) {
    const tr = document.createElement("tr");
    tr.append(text("td", String(e.index + 1) + (e.is_default ? " *" : "")), text("td", e.name), text("td", e.command));
    const status = document.createElement("td");
    status.textContent = product(`status${e.status}`) + (e.reason ? `（${product(`reason${e.reason}`)}）` : "");
    if (choose && e.credential && e.status === "Imported") {
      const box = checkbox(product("importCopyCredentials"), false);
      box.input.dataset.key = `import-credential-${e.index}`;
      choose.set(e.index, box.input);
      status.append(document.createElement("br"), box.label);
    }
    const problems = [...e.problems.map(problemText)];
    if (e.unmigrated_fields.length) problems.push(product("importUnmigratedFields", e.unmigrated_fields.join(", ")));
    const p = text("td", problems.join("；"), e.problems.length ? "java-import-problem" : "");
    for (const cell of [tr.children[2] as HTMLElement, p]) cell.title = cell.textContent ?? "";
    tr.append(status, p);
    table.append(tr);
  }
  wrap.append(table);
  host.append(wrap);
}

/** Step 2: what would be imported. */
function showPreview(body: HTMLElement, p: ImportPreviewView, firstUse: boolean, close: () => Promise<void>, started: () => void, folder: string): void {
  body.replaceChildren();
  body.append(text("p", product("importSource", p.config_file)));
  body.append(text("p", product("importCounts", count(p.items, "Imported"), count(p.items, "Incompatible"), count(p.items, "NotMigrated")), "java-import-counts"));
  const credentials = new Map<number, HTMLInputElement>();
  engineTable(body, p.engines, credentials);
  if (p.engines.some((e) => e.problems.some((x) => x.type === "ProgramNotFound" || x.type === "FileNotFound")) || p.program_dir) {
    const row = document.createElement("div");
    row.className = "java-import-row";
    const program = input(p.program_dir ?? "");
    program.className = "java-import-path";
    program.dataset.key = "import-program";
    row.append(text("span", product("importProgramFolder")), program, button(product("browse"), async () => {
      const r = await run({ type: "ChooseJavaFolder", program: true });
      if (r?.type === "FolderChosen") program.value = r.path;
    }), button(product("importRepreview"), async () => {
      const r = await run({ type: "PreviewJavaImport", config_dir: folder, program_dir: program.value.trim() || null });
      if (r?.type === "JavaImportPreview") showPreview(body, r.preview, firstUse, close, started, folder);
      else if (r?.type === "JavaImportFailed") void message(product("importTitle"), failureText(r.failure), "warning");
    }));
    body.append(row);
  }
  let replace: HTMLInputElement | null = null;
  if (p.replaces.length) {
    const box = document.createElement("div");
    box.className = "java-import-replaces";
    box.append(text("p", product("importReplaces", p.replaces.map((x) => product(`target${x}`)).join("、"))));
    const confirm = checkbox(product("importReplaceConfirm"), false);
    confirm.input.dataset.key = "import-replace";
    replace = confirm.input;
    box.append(confirm.label);
    body.append(box);
  }
  itemLists(body, p.items, false);
  const apply = button(product("importDo"), async () => {
    const copy = [...credentials].filter(([, box]) => box.checked).map(([index]) => index);
    const r = await run({ type: "ApplyJavaImport", preview: p.id, copy_credentials: copy, replace: replace?.checked ?? false });
    if (r?.type === "JavaImported") {
      started();
      showReport(body, r.report, close);
    }
  });
  apply.dataset.key = "import-apply";
  apply.className = "primary";
  const updateApply = () => { apply.disabled = !!replace && !replace.checked; };
  replace?.addEventListener("change", updateApply);
  updateApply();
  actions(body, apply, button(product("importBack"), () => showSource(body, firstUse, close, started, folder, p.program_dir)), button(firstUse ? product("importSkip") : product("cancel"), close));
}

/** Step 3: what was imported. */
function showReport(body: HTMLElement, r: ImportReportView, close: () => Promise<void>): void {
  body.replaceChildren();
  body.append(text("p", product("importDone"), "java-import-section"));
  body.append(text("p", product("importSource", r.config_file)));
  body.append(text("p", product("importReportCounts", count(r.items, "Imported"), count(r.items, "Incompatible"), count(r.items, "NotMigrated")), "java-import-counts"));
  engineTable(body, r.engines, null);
  if (r.engines.some((e) => e.problems.length)) body.append(text("p", product("importFixLater", t("Menu.engineConfig"))));
  if (r.restart_required) body.append(text("p", t("Lizzie.hint.restart"), "java-import-restart"));
  itemLists(body, r.items, true);
  const done = button(product("close"), close);
  done.dataset.key = "import-close";
  done.className = "primary";
  actions(body, done);
  done.focus();
}
