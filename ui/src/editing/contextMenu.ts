// Board right-click popup (RightClickMenu / RightClickMenu2 are JPopupMenus over the main panel).
// Drawn and operated like the menu bar dropdowns (Up / Down / Home / End, Enter / Space, Escape);
// closes on a click outside, the wheel, Escape or window changes, the focus going back where it
// was. Board input stays blocked while it is open and for Java's 200 ms after it closes
// (`isShowingRightMenu` is reset by a 200 ms timer in popupMenuWillBecomeInvisible).
import { bindMenuRows, menuKey } from "../chrome/menuKeys";
import { icon, type IconName } from "../design/icons";
import { product } from "../i18n";
import "./editing.css";

export interface ContextItem {
  /** Java resource key (reported to automation as the item id). */
  key: string;
  label: string;
  /** Icon shown in the check column like the Swing menu item icon. */
  icon?: IconName;
  /** JCheckBoxMenuItem state. */
  checked?: boolean;
  /** setToolTipText. */
  title?: string;
  /** setEnabled(false): shown greyed, not chosen. */
  disabled?: boolean;
  /** Absent: a Java entry whose capability is not migrated yet (shown disabled). */
  run?: () => void;
}

export type ContextEntry = ContextItem | "sep";

/** Items that destroy record content, in the danger ink (DESIGN.md 危险操作): RightClickMenu2's
 *  删除 takes the stone off the record's position. */
const DANGER = new Set(["RightClickMenu2.deleteone"]);

let menu: HTMLElement | null = null;
let shown: string[] | null = null;
let closedAt = -Infinity;
/** The element focused before the popup opened; it gets the focus back when the popup closes. */
let focusBefore: Element | null = null;

export function closeContextMenu(): void {
  if (!menu) return;
  const focus = document.activeElement;
  const lost = !focus || focus === document.body || menu.contains(focus);
  menu.remove();
  menu = null;
  shown = null;
  closedAt = performance.now();
  const before = focusBefore;
  focusBefore = null;
  if (lost && before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
}

/** Opens the popup with its top-left corner at the client point, kept inside the window. */
export function openContextMenu(clientX: number, clientY: number, entries: ContextEntry[]): void {
  const before = menu ? focusBefore : document.activeElement;
  closeContextMenu();
  focusBefore = before;
  const list = document.createElement("div");
  list.className = "dropdown context-menu";
  const keys: string[] = [];
  for (const entry of entries) {
    if (entry === "sep") {
      if (list.lastElementChild && !list.lastElementChild.classList.contains("sep")) {
        const sep = document.createElement("div");
        sep.className = "sep";
        list.append(sep);
      }
      continue;
    }
    const row = document.createElement("div");
    row.className = "mi";
    row.dataset.key = entry.key;
    if (DANGER.has(entry.key)) row.classList.add("danger");
    const mark = document.createElement("span");
    mark.className = "mark";
    if (entry.icon) {
      mark.append(icon(entry.icon));
    } else if (entry.checked !== undefined) {
      // A JCheckBoxMenuItem: a checked one's check in the check column.
      row.classList.add("checkable");
      if (entry.checked) mark.append(icon("check"));
      row.setAttribute("role", "menuitemcheckbox");
      row.setAttribute("aria-checked", String(entry.checked));
    }
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = entry.label;
    row.append(mark, text);
    const action = entry.run;
    if (entry.title) row.title = entry.title;
    if (action && entry.disabled) {
      row.classList.add("disabled");
    } else if (action) {
      row.addEventListener("click", (e) => {
        e.stopPropagation();
        closeContextMenu();
        action();
      });
    } else {
      row.classList.add("disabled", "unavailable");
      row.title = product("unavailable");
    }
    list.append(row);
    keys.push(entry.key);
  }
  if (list.lastElementChild?.classList.contains("sep")) list.lastElementChild.remove();
  bindMenuRows(list);
  list.addEventListener("contextmenu", (e) => e.preventDefault());
  document.body.append(list);
  const r = list.getBoundingClientRect();
  list.style.left = `${Math.max(0, Math.min(clientX, window.innerWidth - r.width))}px`;
  list.style.top = `${Math.max(0, Math.min(clientY, window.innerHeight - r.height))}px`;
  menu = list;
  shown = keys;
}

/** The popup is open: workbench shortcuts are inactive, like under a Swing popup. */
export function contextMenuOpen(): boolean {
  return menu !== null;
}

/** LizzieFrame.isShowingRightMenu: open, or closed less than 200 ms ago. */
export function contextMenuShowing(): boolean {
  return menu !== null || performance.now() - closedAt < 200;
}

/** Item keys of the open popup, for the acceptance report. */
export function contextMenuItems(): string[] | null {
  return shown;
}

document.addEventListener(
  "mousedown",
  (e) => {
    if (menu && !menu.contains(e.target as Node)) closeContextMenu();
  },
  true,
);
document.addEventListener(
  "wheel",
  (e) => {
    if (menu && !menu.contains(e.target as Node)) closeContextMenu();
  },
  { capture: true, passive: true },
);
document.addEventListener(
  "keydown",
  (e) => {
    if (menu && menuKey(e, menu, closeContextMenu)) {
      e.stopPropagation();
      e.preventDefault();
    }
  },
  true,
);
window.addEventListener("blur", closeContextMenu);
window.addEventListener("resize", closeContextMenu);
