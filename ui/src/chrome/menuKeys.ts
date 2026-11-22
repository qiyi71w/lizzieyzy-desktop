// Swing's menu keys and the shared active row of a dropdown (DESIGN.md Menus): the menu bar's
// menus, the toolbar popups and every right-click menu use these, in the main window and in the
// tool windows alike. A leaf module: it imports nothing, so any window can bind its menus with it.

/** A submenu dropdown that is showing. */
export const OPEN_SUB = ":scope > .mi > .dropdown:not(.empty)";

/** Opens a submenu row's dropdown, as hovering the row does. */
export const subOpeners = new WeakMap<HTMLElement, () => HTMLElement>();

/** The rows the arrow keys stop on: a dropdown's enabled items, in order. */
function enabledRows(list: HTMLElement): HTMLElement[] {
  return [...list.children].filter((child): child is HTMLElement => child.classList.contains("mi") && !child.classList.contains("disabled"));
}

function activeRow(list: HTMLElement): HTMLElement | null {
  return list.querySelector<HTMLElement>(":scope > .mi.active");
}

/** Makes `row` its dropdown's active row (Swing's armed item), the one highlight the pointer and
 *  the keys share; with `focus` the row takes the focus too, so a screen reader announces it. */
export function setActiveRow(list: HTMLElement, row: HTMLElement, focus: boolean): void {
  const old = activeRow(list);
  if (old !== row) old?.classList.remove("active");
  row.classList.add("active");
  if (focus) row.focus({ preventScroll: true });
}

/** Scrolls a dropdown that scrolls inside just enough to show `row` whole; the page stays put. */
export function reveal(list: HTMLElement, row: HTMLElement): void {
  if (row.offsetTop < list.scrollTop) list.scrollTop = row.offsetTop - 4;
  else if (row.offsetTop + row.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = row.offsetTop + row.offsetHeight - list.clientHeight + 4;
}

export function activateFirst(list: HTMLElement): void {
  const first = enabledRows(list)[0];
  if (!first) return;
  setActiveRow(list, first, true);
  reveal(list, first);
}

/** Roles, focusability and the shared active row of one dropdown: the menu bar's menus, the
 *  toolbar popups, the board's right-click menu and 超级鹰眼's. A row the pointer moves onto
 *  becomes active (and takes the focus when a row has it); it stays active on leaving while its
 *  submenu shows. */
export function bindMenuRows(list: HTMLElement): void {
  list.setAttribute("role", "menu");
  for (const child of list.children) {
    const row = child as HTMLElement;
    if (row.classList.contains("sep")) row.setAttribute("role", "separator");
    if (!row.classList.contains("mi")) continue;
    row.tabIndex = -1;
    if (!row.hasAttribute("role")) row.setAttribute("role", "menuitem");
    if (row.classList.contains("disabled")) row.setAttribute("aria-disabled", "true");
    // A submenu row holds its dropdown, which must not join the row's name.
    if (row.classList.contains("sub")) {
      row.setAttribute("aria-haspopup", "menu");
      row.setAttribute("aria-label", row.querySelector(".label")?.textContent ?? "");
    }
    row.querySelector(":scope > .mark")?.setAttribute("aria-hidden", "true");
    row.addEventListener("mouseenter", () => {
      if (!row.classList.contains("disabled")) setActiveRow(list, row, document.activeElement?.classList.contains("mi") ?? false);
    });
    row.addEventListener("mouseleave", () => {
      if (!row.querySelector(":scope > .dropdown:not(.empty)")) row.classList.remove("active");
    });
  }
}

/** Swing's menu keys in the open dropdown `root` and its submenus, acting on the innermost level
 *  with an active row: Up / Down (wrapping; the first item when none is active) and Home / End move
 *  among enabled items; Right opens the active row's submenu with its first item active; Left and
 *  Escape close the innermost submenu back to its row; Enter / Space run the active row as a click
 *  does; Tab moves between a row and a field or button inside it, which keeps its other keys. On
 *  the outer level Left / Right go to the neighbouring menu (`across`) and Escape runs `close`.
 *  True when the key was used. */
export function menuKey(e: KeyboardEvent, root: HTMLElement, close: () => void, across?: (step: 1 | -1) => void): boolean {
  if (e.ctrlKey || e.altKey || e.metaKey) return false;
  const inner = e.target instanceof HTMLElement && root.contains(e.target) && e.target.matches("input, button") ? e.target : null;
  if (inner && e.key !== "Escape" && e.key !== "Tab" && e.key !== "ArrowUp" && e.key !== "ArrowDown") return false;
  const path = [root];
  for (let sub = root.querySelector<HTMLElement>(OPEN_SUB); sub; sub = sub.querySelector<HTMLElement>(OPEN_SUB)) path.push(sub);
  let depth = path.length - 1;
  while (depth > 0 && !activeRow(path[depth])) depth--;
  let row = activeRow(path[depth]);
  if (inner) {
    row = inner.closest<HTMLElement>(".mi");
    depth = row?.parentElement ? path.indexOf(row.parentElement) : -1;
    if (!row || depth < 0) return false;
  }
  const list = path[depth];
  const rows = enabledRows(list);
  // Closes the submenu at `level`, making the row that opened it active again.
  const back = (level: number) => {
    const owner = path[level].parentElement as HTMLElement;
    path[level].remove();
    setActiveRow(path[level - 1], owner, true);
    reveal(path[level - 1], owner);
  };
  const move = (next: HTMLElement | undefined) => {
    list.querySelectorAll(":scope > .mi > .dropdown").forEach((d) => d.remove());
    if (!next) return;
    setActiveRow(list, next, true);
    reveal(list, next);
  };
  const openSub = (owner: HTMLElement) => {
    const open = subOpeners.get(owner);
    if (open) activateFirst(open());
  };
  switch (e.key) {
    case "ArrowDown":
    case "ArrowUp": {
      const at = row ? rows.indexOf(row) : -1;
      move(at < 0 ? rows[0] : rows[(at + (e.key === "ArrowDown" ? 1 : -1) + rows.length) % rows.length]);
      break;
    }
    case "Home":
      move(rows[0]);
      break;
    case "End":
      move(rows[rows.length - 1]);
      break;
    case "ArrowRight":
      if (row && subOpeners.has(row)) openSub(row);
      else across?.(1);
      break;
    case "ArrowLeft":
      if (depth > 0) back(depth);
      else across?.(-1);
      break;
    case "Escape":
      if (inner && row) setActiveRow(list, row, true);
      else if (path.length > 1) back(path.length - 1);
      else close();
      break;
    case "Enter":
    case " ":
      if (row && !e.repeat) {
        if (subOpeners.has(row)) openSub(row);
        else row.click();
      }
      break;
    case "Tab": {
      if (!row) break;
      const stops = [row, ...row.querySelectorAll<HTMLElement>(":scope > input, :scope > button")];
      const at = stops.indexOf(document.activeElement as HTMLElement);
      stops[(at + (e.shiftKey ? -1 : 1) + stops.length) % stops.length].focus({ preventScroll: true });
      break;
    }
    default:
      return false;
  }
  return true;
}
