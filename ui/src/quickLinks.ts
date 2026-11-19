// 快速启动设置 (OtherPrograms.java): the form above a 30-row table of the programs. A row is chosen
// for editing (rows past the list add a program); 保存 writes it, 删除 empties its command, and
// 上移 / 下移 / 置顶 reorder at once. Every change is saved and the 快速启动 menu follows it.
import { run, state, subscribe } from "./app";
import { COMMANDS } from "./commands";
import type { QuickLinkProgram } from "./contract/generated";
import { formPanel } from "./dialogs";
import { button, input } from "./engine/controls";
import { t } from "./i18n";
import "./quickLinks.css";

/** OtherPrograms.getTableModel: the table always shows 30 rows. */
const ROWS = 30;

let opened = false;

function saved(): QuickLinkProgram[] {
  return (state.view?.settings.quick_links.programs ?? []).map((p) => ({ ...p }));
}

/** Resolved at each published view: a saved list arrives after its command's answer. */
const viewWaiters: (() => void)[] = [];
subscribe(() => viewWaiters.splice(0).forEach((resume) => resume()));

/** Saves `programs` as the list and gives the list the core saved (it trims commands and drops
 *  empty ones), or null when nothing was saved. */
async function persist(programs: QuickLinkProgram[]): Promise<QuickLinkProgram[] | null> {
  const show = state.view?.settings.quick_links.show ?? false;
  const before = JSON.stringify(state.view?.settings.quick_links.programs);
  const result = await run({ type: "SetQuickLinks", settings: { show, programs } });
  if (!result || result.type === "Refused") return null;
  // A changed list is published right after the answer; continue from it.
  if (result.type === "Done") {
    while (JSON.stringify(state.view?.settings.quick_links.programs) === before) {
      const published = Promise.withResolvers<void>();
      viewWaiters.push(published.resolve);
      await published.promise;
    }
  }
  return saved();
}

export async function openQuickLinks(): Promise<void> {
  if (opened || !state.view) return;
  opened = true;
  try {
    let programs = saved();
    let current = -1;
    const body = document.createElement("div");
    body.className = "quick-links";
    const heading = document.createElement("div");
    heading.className = "quick-links-heading";
    const nameLabel = document.createElement("label");
    nameLabel.textContent = t("MoreEngines.lblName");
    const name = input("");
    name.dataset.key = "quick-links.name";
    const commandLabel = document.createElement("label");
    commandLabel.textContent = t("MoreEngines.lblCommand");
    const command = document.createElement("textarea");
    command.dataset.key = "quick-links.command";
    nameLabel.htmlFor = name.id = "quick-links-name";
    commandLabel.htmlFor = command.id = "quick-links-command";
    const form = document.createElement("div");
    form.className = "quick-links-form";
    const labels = document.createElement("div");
    labels.className = "quick-links-command-label";
    const scan = button(t("MoreEngines.scan"), async () => {
      const r = await run({ type: "ChooseQuickLinkProgram", title: t("OtherPrograms.selectProgram"), filter_name: t("OtherPrograms.program") });
      if (r?.type !== "QuickLinkProgramChosen") return;
      command.value = r.program.command;
      name.value = r.program.name;
    });
    scan.dataset.key = "quick-links.scan";
    labels.append(commandLabel, scan);
    form.append(nameLabel, name, labels, command);

    const table = document.createElement("table");
    table.className = "engine-table quick-links-table";
    const header = table.createTHead().insertRow();
    for (const key of ["MoreEngines.column0", "MoreEngines.column1", "MoreEngines.column2"]) {
      const th = document.createElement("th");
      th.textContent = t(key);
      header.append(th);
    }
    const rows = table.createTBody();
    const tableWrap = document.createElement("div");
    tableWrap.className = "engine-table-wrap quick-links-table-wrap";
    tableWrap.append(table);

    const editors: (HTMLInputElement | HTMLTextAreaElement | HTMLButtonElement)[] = [];
    /** A row chosen (handleTableClick) or none (the initial state, 取消 and 删除). */
    const choose = (row: number, resetHeading = false) => {
      current = row;
      const chosen = row >= 0;
      if (chosen) {
        name.value = programs[row]?.name ?? "";
        command.value = programs[row]?.command ?? "";
        heading.textContent = `${t("OtherPrograms.editEngine")}${row + 1}`;
      } else {
        name.value = command.value = "";
        // Java names the cleared form after the command field (MoreEngines.lblCommand).
        if (resetHeading) heading.textContent = t("MoreEngines.lblCommand");
      }
      heading.classList.toggle("disabled", !chosen);
      for (const el of editors) el.disabled = !chosen;
      render();
    };
    const render = () => {
      rows.replaceChildren();
      for (let index = 0; index < ROWS; index++) {
        const program = programs[index];
        const row = rows.insertRow();
        row.dataset.key = `quick-links.row.${index}`;
        row.classList.toggle("selected", index === current);
        for (const value of [String(index + 1), program?.name ?? "", program?.command ?? ""]) row.insertCell().textContent = value;
        row.title = program?.command ?? "";
        row.onclick = () => choose(index);
      }
    };
    const save = async () => {
      if (current < 0) return;
      const next = programs.map((p) => ({ ...p }));
      const edited = { name: name.value, command: command.value };
      if (current >= next.length) next.push(edited);
      else next[current] = edited;
      const after = await commit(next);
      if (after) programs = after;
      render();
    };
    /** 上移 / 下移 / 置顶: Java moves only between existing programs. */
    const move = async (to: (from: number) => number) => {
      const from = current;
      if (from < 0 || from >= programs.length) return;
      const target = to(from);
      if (target < 0 || target >= programs.length || target === from) return;
      const next = programs.map((p) => ({ ...p }));
      next.splice(target, 0, next.splice(from, 1)[0]);
      const after = await commit(next);
      if (!after) return;
      programs = after;
      current = target;
      render();
    };
    /** One save at a time, with the dialog inert until its list is back (a second change made
     *  meanwhile would start from the list before it). */
    const commit = async (next: QuickLinkProgram[]): Promise<QuickLinkProgram[] | null> => {
      body.inert = true;
      try {
        return await persist(next);
      } finally {
        body.inert = false;
      }
    };
    const moveUp = button(t("MoreEngines.moveUp"), () => void move((i) => i - 1));
    const moveDown = button(t("MoreEngines.moveDown"), () => void move((i) => i + 1));
    const moveFirst = button(t("MoreEngines.moveFirst"), () => void move(() => 0));
    const saveButton = button(t("MoreEngines.save"), () => void save());
    const cancel = button(t("MoreEngines.cancel"), () => choose(-1, true));
    const remove = button(t("MoreEngines.delete"), async () => {
      command.value = "";
      await save();
      choose(-1, true);
    });
    const dialog = formPanel(t("OtherPrograms.title"), body);
    const exit = button(t("MoreEngines.exit"), () => void dialog.close());
    for (const [el, key] of [[moveUp, "moveUp"], [moveDown, "moveDown"], [moveFirst, "moveFirst"], [saveButton, "save"], [cancel, "cancel"], [remove, "delete"], [exit, "exit"]] as const) {
      el.dataset.key = `quick-links.${key}`;
    }
    editors.push(name, command, scan, moveUp, moveDown, moveFirst, cancel, remove);
    const actions = document.createElement("div");
    actions.className = "quick-links-actions";
    const order = document.createElement("span");
    order.append(moveUp, moveDown, moveFirst);
    const edit = document.createElement("span");
    edit.append(saveButton, cancel, remove, exit);
    actions.append(order, edit);
    heading.textContent = t("OtherPrograms.engineName");
    body.append(heading, form, actions, tableWrap);
    choose(-1);
    await dialog.closed;
  } finally {
    opened = false;
  }
}

// 快速启动 > 设置 (LizzieFrame.openProgramDialog).
COMMANDS["Menu.editProgram"] = { run: () => void openQuickLinks() };
