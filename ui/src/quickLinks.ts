// 快速启动设置 (OtherPrograms.java): a group of rows for the chosen program above a 30-row table of
// the programs. A row is chosen for editing (rows past the list add a program) by a click, or by
// ↑ / ↓ / Home / End in the table, which is one Tab stop; until then the rows and 保存 are off and
// the group's title says to choose a row. 保存 writes it, 删除 empties its command, and 上移 / 下移 /
// 置顶 reorder at once. Every change is saved and the 快速启动 menu follows it, so the footer only
// has 关闭.
import { run, state, subscribe } from "./app";
import { COMMANDS } from "./commands";
import type { QuickLinkProgram } from "./contract/generated";
import { formPanel } from "./dialogs";
import { button, footer, groupTitle, input, primaryButton, row } from "./engine/controls";
import { product, t } from "./i18n";
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
    // The chosen program: its title (or the prompt to choose one), 名称, then 命令行 with 自动生成
    // at its end and the command on the line under its name.
    const heading = groupTitle(body, "");
    heading.classList.add("quick-links-heading");
    heading.id = "quick-links-heading";
    const name = input("");
    name.className = "quick-links-name";
    name.dataset.key = "quick-links.name";
    name.id = "quick-links-name";
    row(body, t("MoreEngines.lblName"), "", name);
    const command = document.createElement("textarea");
    command.dataset.key = "quick-links.command";
    command.id = "quick-links-command";
    const scan = button(t("MoreEngines.scan"), async () => {
      const r = await run({ type: "ChooseQuickLinkProgram", title: t("OtherPrograms.selectProgram"), filter_name: t("OtherPrograms.program") });
      if (r?.type !== "QuickLinkProgramChosen") return;
      command.value = r.program.command;
      name.value = r.program.name;
    });
    scan.dataset.key = "quick-links.scan";
    const commandRow = row(body, t("MoreEngines.lblCommand"), "", scan);
    commandRow.classList.add("quick-links-command");
    const commandName = commandRow.querySelector<HTMLElement>(".ds-row-title")!;
    commandName.id = "quick-links-command-name";
    command.setAttribute("aria-labelledby", commandName.id);
    commandRow.append(command);

    const table = document.createElement("table");
    table.className = "engine-table quick-links-table";
    const header = table.createTHead().insertRow();
    for (const key of ["MoreEngines.column0", "MoreEngines.column1", "MoreEngines.column2"]) {
      const th = document.createElement("th");
      th.textContent = t(key);
      header.append(th);
    }
    const rows = table.createTBody();
    rows.addEventListener("keydown", (event) => {
      // ↓ from no chosen row chooses the first.
      const targets: Record<string, number> = { ArrowUp: Math.max(0, current - 1), ArrowDown: Math.min(ROWS - 1, current + 1), Home: 0, End: ROWS - 1 };
      const target = targets[event.key] as number | undefined;
      if (target !== undefined) {
        event.preventDefault();
        if (target !== current) choose(target, true);
      } else if ((event.key === "Enter" || event.key === " ") && event.target instanceof HTMLTableRowElement) {
        event.preventDefault();
        choose(event.target.sectionRowIndex, true);
      }
    });
    const tableWrap = document.createElement("div");
    tableWrap.className = "engine-table-wrap quick-links-table-wrap";
    tableWrap.append(table);

    const editors: (HTMLInputElement | HTMLTextAreaElement | HTMLButtonElement)[] = [];
    /** A row chosen (handleTableClick) or none (the initial state, 取消 and 删除); `focus` puts the
     *  focus on the chosen row, as a click or a key in the table does. */
    const choose = (row: number, focus = false) => {
      current = row;
      const chosen = row >= 0;
      if (chosen) {
        name.value = programs[row]?.name ?? "";
        command.value = programs[row]?.command ?? "";
        heading.textContent = `${t("OtherPrograms.editEngine")}${row + 1}`;
      } else {
        name.value = command.value = "";
        heading.textContent = t("OtherPrograms.engineName");
      }
      heading.classList.toggle("prompt", !chosen);
      for (const el of editors) el.disabled = !chosen;
      render();
      if (focus && chosen) rows.rows[row].focus();
    };
    /** The table body is one Tab stop: the chosen row, or the first while none is. */
    const render = () => {
      rows.replaceChildren();
      for (let index = 0; index < ROWS; index++) {
        const program = programs[index];
        const row = rows.insertRow();
        row.dataset.key = `quick-links.row.${index}`;
        row.classList.toggle("selected", index === current);
        if (index === current) row.setAttribute("aria-current", "true");
        row.tabIndex = index === Math.max(0, current) ? 0 : -1;
        for (const value of [String(index + 1), program?.name ?? "", program?.command ?? ""]) row.insertCell().textContent = value;
        row.title = program?.command ?? "";
        row.onclick = () => choose(index, true);
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
    const saveButton = primaryButton(t("MoreEngines.save"), () => void save());
    const cancel = button(t("MoreEngines.cancel"), () => choose(-1));
    const remove = button(t("MoreEngines.delete"), async () => {
      command.value = "";
      await save();
      choose(-1);
    });
    remove.classList.add("danger");
    // Every change is already saved: the footer only closes.
    const exit = button(product("close"), () => void dialog.close());
    for (const [el, key] of [[moveUp, "moveUp"], [moveDown, "moveDown"], [moveFirst, "moveFirst"], [saveButton, "save"], [cancel, "cancel"], [remove, "delete"], [exit, "exit"]] as const) {
      el.dataset.key = `quick-links.${key}`;
    }
    // 保存 with no row chosen would save nothing: it is off, and its reason is the heading.
    saveButton.setAttribute("aria-describedby", heading.id);
    editors.push(name, command, scan, moveUp, moveDown, moveFirst, saveButton, cancel, remove);
    // The order buttons at the start, the chosen row's edits at the end, then the table.
    const actions = document.createElement("div");
    actions.className = "quick-links-actions";
    const order = document.createElement("span");
    order.append(moveUp, moveDown, moveFirst);
    const edit = document.createElement("span");
    edit.append(saveButton, cancel, remove);
    actions.append(order, edit);
    body.append(actions, tableWrap);
    footer(body, { cancel: exit });
    choose(-1);
    const dialog = formPanel(t("OtherPrograms.title"), body);
    // The table is where a row is chosen: it takes the focus, not 关闭.
    rows.rows[0].focus();
    await dialog.closed;
  } finally {
    opened = false;
  }
}

// 快速启动 > 设置 (LizzieFrame.openProgramDialog).
COMMANDS["Menu.editProgram"] = { run: () => void openQuickLinks() };
