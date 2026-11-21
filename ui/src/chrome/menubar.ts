// Java menu bar (Menu.java): every top-level menu and item in Java order. Items run catalog
// commands; entries without one are disabled and marked as not yet available.
import { run, state, subscribe } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { DisplaySettings } from "../contract/generated";
import { setRankCustomText } from "../display/commands";
import { isChinese, product, registerProduct, t } from "../i18n";
import { JAVA_MENUS, type MenuNode, type TopMenu } from "./javaMenus";
import { chooseEngine } from "../engine/catalog";
import { DARK_READY } from "../design/theme";
import { icon } from "../design/icons";
import { highlight } from "../search/highlight";
import { paintingOpen } from "../display/painting";
import { contextMenuOpen } from "../editing/contextMenu";
import { onAppearanceChange } from "../appearance/state";

/** 更多菜单: the button at the menu bar's end that holds the menus a narrow window has no room for. */
registerProduct({
  ZhCn: { menuOverflow: "更多菜单" },
  ZhTw: { menuOverflow: "更多選單" },
  EnUs: { menuOverflow: "More menus" },
  JaJp: { menuOverflow: "その他のメニュー" },
  KoKr: { menuOverflow: "메뉴 더 보기" },
  ThTh: { menuOverflow: "เมนูเพิ่มเติม" },
});

/** Java conditional menus: 快速启动 follows 显示快速启动 (Config.java: show-quick-links, false by
 *  default); show-Contribute is settings.contribution.showMenu, true by default; the secondary
 *  engine menu needs double-engine mode; ReadBoard is Windows only. This product's 外观模式 waits
 *  for the dark set (design/theme.ts DARK_READY). */
function visible(when: string | undefined): boolean {
  switch (when) {
    case undefined:
      return true;
    case "windows":
      return navigator.userAgent.includes("Windows");
    // Config.useJavaLooks defaults to !OS.isWindows() (Config.java:1647).
    case "javaLooks":
      return !navigator.userAgent.includes("Windows");
    case "contribute":
      return state.view?.settings.contribution.showMenu ?? true;
    case "quickLinks":
      return state.view?.settings.quick_links.show ?? false;
    // Menu.setEngineMenuone2status: the secondary engine menu shows in 双引擎模式 only.
    case "doubleEngine":
      return state.view?.settings.extra_mode === "DoubleEngine";
    case "darkMode":
      return DARK_READY;
    default:
      return false;
  }
}

/** Java resource text; `product.*` entries are this product's own items. A `{display.name}` suffix
 *  shows the current display setting, like Java labels rebuilt with the value. */
function label(node: { key: string; suffix?: string }): string {
  const suffix = node.suffix
    ?.replace("{sp}", isChinese() ? "" : " ")
    .replace(/\{display\.(\w+)\}/g, (_, name: string) => String(state.view?.settings.display[name as keyof DisplaySettings] ?? "")) ?? "";
  return (node.key.startsWith("product.") ? product(node.key.slice("product.".length)) : t(node.key)) + suffix;
}

let openMenu: HTMLElement | null = null;
/** The element focused before the menus opened; it gets the focus back when they close. */
let focusBefore: Element | null = null;

/** Whether the latest input was a key rather than a pointer press: a toolbar popup opened by a key
 *  starts with its first item active, one opened by the mouse with none. */
let keyboardInput = false;
document.addEventListener("keydown", () => (keyboardInput = true), true);
document.addEventListener("pointerdown", () => (keyboardInput = false), true);

/** Closes the open menu. Focus left in it (or lost with it) returns to where it was before the
 *  menus opened, unless another menu takes over (`handover`). */
function closeMenus(handover = false): void {
  const menu = openMenu;
  if (!menu) return;
  const focus = document.activeElement;
  const lost = !focus || focus === document.body || menu.contains(focus);
  menu.classList.remove("open");
  menu.querySelector(".dropdown")?.remove();
  if (menu.classList.contains("popup-anchor")) menu.remove();
  else menu.setAttribute("aria-expanded", "false");
  openMenu = null;
  if (handover) return;
  const before = focusBefore;
  focusBefore = null;
  if (lost && before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
}

/** Closes the open menu for the one about to open, keeping the focus to return to. */
function replaceMenu(): void {
  if (!openMenu) focusBefore = document.activeElement;
  closeMenus(true);
}

/** A JPopupMenu shown below `anchor` (a toolbar button), drawn and closed like the menu bar's; like
 *  Swing's, it moves left rather than leave the window. */
export function openPopupMenu(anchor: HTMLElement, items: MenuNode[], above = false): void {
  replaceMenu();
  const r = anchor.getBoundingClientRect();
  const holder = document.createElement("div");
  holder.className = "popup-anchor open";
  holder.style.left = `${r.left}px`;
  holder.style.top = `${r.bottom}px`;
  const list = dropdown(items);
  holder.append(list);
  document.body.append(holder);
  const box = list.getBoundingClientRect();
  if (box.right > window.innerWidth) holder.style.left = `${Math.max(0, r.left - (box.right - window.innerWidth))}px`;
  if (above) holder.style.top = `${r.top - box.height}px`;
  openMenu = holder;
  if (keyboardInput) activateFirst(list);
}

/** rankCustomMove: the count is typed into the item itself (Menu.java:9234-9316). */
function rankCustomField(): HTMLInputElement {
  const field = document.createElement("input");
  field.type = "text";
  field.className = "mi-field";
  field.value = String(state.view?.settings.display.moveRankCustom ?? 3);
  field.setAttribute("aria-label", t("Menu.rankMenu.lblCustomMove"));
  field.addEventListener("beforeinput", (e) => {
    // IntDocument: digits only.
    if (e.inputType.startsWith("insert") && /\D/.test(e.data ?? "")) e.preventDefault();
  });
  field.addEventListener("input", () => setRankCustomText(field.value));
  field.addEventListener("click", (e) => e.stopPropagation());
  return field;
}

/** Text fields typed into a popup item (a JCheckBoxMenuItem holding a JTextField), by item id. */
const itemFields = new Map<string, () => HTMLElement>();
export function registerItemField(id: string, make: () => HTMLElement): void {
  itemFields.set(id, make);
}

/** Java setToolTipText on menu items (Menu.java:892-929, 9564-9569). */
function tooltip(key: string): string | undefined {
  switch (key) {
    case "Menu.moveNumberInBracnhFromOne":
    case "Menu.moveNumberInBracnhFromOneContinue":
      return t("Menu.moveNumberInBranchTips");
    case "Menu.rankMenu.evaluationMode.auto":
      return t("Menu.rankMenu.evaluationMode.auto.tooltip");
    case "Menu.rankMenu.evaluationMode.score":
      return t("Menu.rankMenu.evaluationMode.score.tooltip");
    case "Menu.rankMenu.evaluationMode.winrate":
      return t("Menu.rankMenu.evaluationMode.winrate.tooltip");
    case "Menu.rankMenu.evaluationMode.combined":
      return t("Menu.rankMenu.evaluationMode.combined.tooltip");
  }
  return undefined;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Recent entries carry their path in the id. */
const RECENT = "recent:";

/** The catalog node each shown row stands for, so a function search can find it again. */
const rowNodes = new WeakMap<Element, MenuNode>();

/** Items that destroy record content or the user's data, in the danger ink like the bottom bar's
 *  清空棋盘 / 删除 (DESIGN.md 危险操作): 编辑's 清空棋盘, 删除一手, 删除分支, 摆棋's 清空全部设置子,
 *  更多's 清空棋盘 / 删除, and 帮助's 清除所有个人数据. */
const DANGER = new Set(["menu.clearBoard", "menu.delete", "menu.deleteBranch", "menu.setupClearAll", "bottombar.more.clearButton", "bottombar.more.deleteMove", "menu.clearUserData"]);

/** The gap a dropdown keeps from the window's edges. */
const EDGE = 4;

/** A submenu dropdown that is showing. */
const OPEN_SUB = ":scope > .mi > .dropdown:not(.empty)";

/** Puts an open submenu beside its row: to the right, or to the left of its menu when the right
 *  edge would cut it, moved up as far as the bottom edge needs. Its own open submenu follows. */
function placeSub(sub: HTMLElement): void {
  const row = (sub.parentElement as HTMLElement).getBoundingClientRect();
  const width = sub.offsetWidth;
  const left = row.right + width > window.innerWidth - EDGE ? row.left - width : row.right;
  sub.style.left = `${Math.max(0, left)}px`;
  sub.style.top = `${Math.max(EDGE, Math.min(row.top - 4, window.innerHeight - EDGE - sub.offsetHeight))}px`;
  const inner = sub.querySelector<HTMLElement>(OPEN_SUB);
  if (inner) placeSub(inner);
}

/** Shows a submenu's dropdown beside its row, closing the row's open siblings first; one taller
 *  than the window scrolls inside. */
function expand(row: HTMLElement, list: HTMLElement, children: MenuNode[]): HTMLElement {
  list.querySelectorAll(":scope > .mi > .dropdown").forEach((d) => d.remove());
  const sub = dropdown(children);
  if (sub.childElementCount === 0) sub.classList.add("empty");
  sub.style.maxHeight = `${window.innerHeight - 2 * EDGE}px`;
  row.append(sub);
  if (sub.childElementCount > 0) placeSub(sub);
  return sub;
}

/** Opens a submenu row's dropdown, as hovering the row does. */
const subOpeners = new WeakMap<HTMLElement, () => HTMLElement>();

/** The rows the arrow keys stop on: a dropdown's enabled items, in order. */
function enabledRows(list: HTMLElement): HTMLElement[] {
  return [...list.children].filter((child): child is HTMLElement => child.classList.contains("mi") && !child.classList.contains("disabled"));
}

function activeRow(list: HTMLElement): HTMLElement | null {
  return list.querySelector<HTMLElement>(":scope > .mi.active");
}

/** Makes `row` its dropdown's active row (Swing's armed item), the one highlight the pointer and
 *  the keys share; with `focus` the row takes the focus too, so a screen reader announces it. */
function setActiveRow(list: HTMLElement, row: HTMLElement, focus: boolean): void {
  const old = activeRow(list);
  if (old !== row) old?.classList.remove("active");
  row.classList.add("active");
  if (focus) row.focus({ preventScroll: true });
}

/** Scrolls a dropdown that scrolls inside just enough to show `row` whole; the page stays put. */
function reveal(list: HTMLElement, row: HTMLElement): void {
  if (row.offsetTop < list.scrollTop) list.scrollTop = row.offsetTop - 4;
  else if (row.offsetTop + row.offsetHeight > list.scrollTop + list.clientHeight) list.scrollTop = row.offsetTop + row.offsetHeight - list.clientHeight + 4;
}

function activateFirst(list: HTMLElement): void {
  const first = enabledRows(list)[0];
  if (!first) return;
  setActiveRow(list, first, true);
  reveal(list, first);
}

/** Roles, focusability and the shared active row of one dropdown: the menu bar's menus, the
 *  toolbar popups and the board's right-click menu. A row the pointer moves onto becomes active
 *  (and takes the focus when a row has it); it stays active on leaving while its submenu shows. */
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

/** One dropdown level, rebuilt each time it opens so checked/enabled states are current. */
function dropdown(items: MenuNode[]): HTMLElement {
  const list = document.createElement("div");
  list.className = "dropdown";
  // An open submenu stays beside its row while this level scrolls.
  list.addEventListener("scroll", () => {
    const sub = list.querySelector<HTMLElement>(OPEN_SUB);
    if (sub) placeSub(sub);
  });
  for (const node of items) {
    if (node.t === "sep") {
      if (list.lastElementChild && !list.lastElementChild.classList.contains("sep")) {
        const sep = document.createElement("div");
        sep.className = "sep";
        list.append(sep);
      }
      continue;
    }
    if (!visible(node.when)) continue;
    if (node.t === "dynamic" && node.key === "engine.list") {
      // engine.select-secondary: the same list for 双引擎模式's secondary slot (Menu.java:7453-7485).
      const secondary = node.id === "engine.select-secondary";
      const current = secondary ? state.view?.engine.secondary : state.view?.engine.foreground;
      const engines = state.view?.engine.engines ?? [];
      // Menu.java:7407-7429: the twentieth slot is the overflow chooser.
      engines.slice(0, engines.length > 20 ? 19 : 20).forEach((engine, index) => {
        const row = document.createElement("div"); row.className = "mi checkable"; rowNodes.set(row, node);
        row.dataset.engineIndex = String(index); row.title = engine.command;
        row.setAttribute("role", "menuitemradio"); row.setAttribute("aria-checked", String(current?.index === index));
        const mark = document.createElement("span"); mark.className = "mark";
        mark.textContent = current?.index === index ? "✓" : engine.state === "Ready" ? "●" : engine.state === "Loading" ? "◐" : "○";
        const text = document.createElement("span"); text.className = "label"; text.textContent = `[${index + 1}] ${engine.name}`;
        row.append(mark, text);
        row.onclick = (event) => {
          event.stopPropagation(); closeMenus();
          void run(secondary ? { type: "SwitchSecondaryEngine", index } : { type: "SwitchEngine", index });
        };
        list.append(row);
      });
      if (engines.length > 20) {
        const more = document.createElement("div"); more.className = "mi"; more.textContent = t("Menu.moreEngines");
        more.onclick = (event) => { event.stopPropagation(); closeMenus(); void chooseEngine(false, secondary); }; list.append(more);
      }
      continue;
    }
    if (node.t === "dynamic" && node.key === "quickLinks.programs") {
      // Menu.refreshQuickLinkItems: one item per saved program, named as saved; choosing one starts
      // it (FastLink.startProgram).
      (state.view?.settings.quick_links.programs ?? []).forEach((program, index) => {
        const row = document.createElement("div"); row.className = "mi"; rowNodes.set(row, node);
        row.dataset.key = `quickLinks.program.${index}`; row.title = program.command;
        const mark = document.createElement("span"); mark.className = "mark";
        const text = document.createElement("span"); text.className = "label"; text.textContent = program.name;
        row.append(mark, text);
        row.onclick = (event) => { event.stopPropagation(); closeMenus(); void run({ type: "LaunchQuickLink", index }); };
        list.append(row);
      });
      continue;
    }
    if (node.t === "dynamic" && node.key !== "Menu.openRecent") continue;
    const row = document.createElement("div");
    row.className = "mi";
    row.dataset.key = node.key;
    rowNodes.set(row, node);
    if (node.id && DANGER.has(node.id)) row.classList.add("danger");
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label(node);
    const mark = document.createElement("span");
    mark.className = "mark";
    row.append(mark, text);
    const tip = tooltip(node.key);
    if (tip) row.title = tip;
    if (node.key === "Menu.rankMenu.lblCustomMove") row.append(rankCustomField());
    const itemField = node.id ? itemFields.get(node.id) : undefined;
    if (itemField) row.append(itemField());
    if (node.t === "submenu" || node.key === "Menu.openRecent") {
      row.classList.add("sub");
      const children = node.key === "Menu.openRecent" ? recentItems() : (node.items ?? []);
      if (children.length === 0 && node.key !== "Menu.openRecent") {
        row.classList.add("disabled", "unavailable");
        row.title = product("unavailable");
      }
      // A disabled row opens nothing, though moving onto it still closes its siblings' submenus.
      const open = () => expand(row, list, row.classList.contains("disabled") ? [] : children);
      subOpeners.set(row, open);
      row.addEventListener("mouseenter", open);
    } else {
      const recent = node.id?.startsWith(RECENT) ? node.id.slice(RECENT.length) : null;
      if (recent) row.title = recent;
      const entry: CommandEntry | undefined = recent
        ? { run: () => run({ type: "OpenRecord", path: recent, source: "Recent" }) }
        : (COMMANDS[node.id ?? ""] ?? COMMANDS[node.key]);
      if (entry?.visible && !entry.visible()) continue;
      if (entry?.label) text.textContent = entry.label();
      if (node.t === "check" || node.t === "radio") {
        const checked = !!entry?.checked?.();
        mark.textContent = checked ? "✓" : "";
        row.classList.add("checkable");
        row.setAttribute("role", node.t === "check" ? "menuitemcheckbox" : "menuitemradio");
        row.setAttribute("aria-checked", String(checked));
      }
      if (!entry) {
        row.classList.add("disabled", "unavailable");
        row.title = product("unavailable");
      } else if (entry.enabled && !entry.enabled()) {
        row.classList.add("disabled");
      } else {
        row.addEventListener("click", (e) => {
          e.stopPropagation();
          closeMenus();
          entry.run();
        });
      }
      if (entry?.button) {
        const inner = entry.button;
        const b = document.createElement("button");
        b.className = "mi-button";
        b.dataset.key = `${node.id ?? node.key}.set`;
        b.textContent = inner.label();
        b.addEventListener("click", (e) => {
          e.stopPropagation();
          closeMenus();
          inner.run();
        });
        row.append(b);
      }
    }
    list.append(row);
  }
  if (list.lastElementChild?.classList.contains("sep")) list.lastElementChild.remove();
  bindMenuRows(list);
  return list;
}

/** File > 最近打开: newest first as "1: name", full path as tooltip (Menu.java:7192-7208). Entries
 *  open without changing the recent list; a missing file shows the normal open failure. */
function recentItems(): MenuNode[] {
  return (state.view?.recent_files ?? []).map((path, k) => ({ t: "item", key: `${k + 1}: ${fileName(path)}`, id: `${RECENT}${path}` }));
}

/** The 双引擎模式 secondary engine menu's title element. */
let secondaryMenu: HTMLElement | null = null;

/** The built top-level menus and how each opens (`keyboard`: with its first item active). */
const tops = new Map<TopMenu, { top: HTMLElement; open: (keyboard?: boolean) => void }>();

/** Whether a menu title stands on the bar: shown now and not moved into 更多菜单. */
const onBar = (top: HTMLElement) => top.style.display !== "none" && !top.classList.contains("overflowed");

/** Whether a menu title can open: the engine menu is disabled while an 引擎对战 batch runs
 *  (Menu.engineMenu.setEnabled(false), SwingEngineGameChrome.java:49-53). */
const enabledTop = (top: HTMLElement) => top.getAttribute("aria-disabled") !== "true";

/** Left / Right on a menu's outer level: the neighbouring enabled menu on the bar, wrapping,
 *  opened as by a key. */
function stepTop(step: 1 | -1): void {
  const shown = [...tops.values()].filter(({ top }) => onBar(top) && enabledTop(top));
  const at = shown.findIndex(({ top }) => top === openMenu);
  shown[(at + step + shown.length) % shown.length]?.open(true);
}

/** 更多菜单's entry in `tops`: last on the bar, its items the menus moved into it. */
const OVERFLOW: TopMenu = { key: "menu.overflow", items: [] };

/** A menu title's presses: a press opens its menu or closes the open one; moving onto it while
 *  another menu is open opens it instead. */
function pressable(top: HTMLElement, open: () => void): void {
  top.addEventListener("mousedown", (e) => {
    const target = e.target as HTMLElement;
    // A press on a dropdown's own box (its scrollbar or padding) keeps its default, so the
    // scrollbar drags.
    if (!target.classList.contains("dropdown")) e.preventDefault();
    if (openMenu === top && !target.closest(".dropdown")) closeMenus();
    else if (openMenu !== top) open();
  });
  top.addEventListener("mouseenter", () => {
    if (openMenu && openMenu !== top) open();
  });
}

/** Opens `list` under the menu title `top`: down to the window's bottom edge, scrolling inside when
 *  longer, and moved left rather than cross the right edge. */
function showDropdown(top: HTMLElement, list: HTMLElement, keyboard: boolean): void {
  openMenu = top;
  top.classList.add("open");
  top.setAttribute("aria-expanded", "true");
  top.append(list);
  list.style.maxHeight = `${window.innerHeight - EDGE - list.getBoundingClientRect().top}px`;
  const box = list.getBoundingClientRect();
  const over = box.right - (window.innerWidth - EDGE);
  if (over > 0) list.style.left = `${-Math.min(over, box.left)}px`;
  if (keyboard) activateFirst(list);
}

/** The menus 更多菜单 holds now, in bar order. */
function overflowedMenus(): [TopMenu, HTMLElement][] {
  return [...tops].filter(([menu, { top }]) => menu !== OVERFLOW && top.style.display !== "none" && top.classList.contains("overflowed")).map(([menu, { top }]) => [menu, top]);
}

/** 更多菜单's dropdown: a submenu row per menu it holds, named as that menu's title (an engine
 *  menu's is its engine); a disabled menu's row is disabled. */
function overflowDropdown(): HTMLElement {
  const held = overflowedMenus();
  const list = dropdown(held.map(([menu]): MenuNode => ({ t: "submenu", key: menu.key, items: menu.items })));
  [...list.children].forEach((row, k) => {
    const name = held[k][1].getAttribute("aria-label") ?? "";
    const text = row.querySelector(".label");
    if (text) text.textContent = name;
    row.setAttribute("aria-label", name);
    if (enabledTop(held[k][1])) return;
    row.classList.add("disabled");
    row.setAttribute("aria-disabled", "true");
  });
  return list;
}

export function buildMenubar(host: HTMLElement): void {
  const measure = document.createElement("canvas").getContext("2d")!;
  // A menu title's width: its own text in its rendered font plus EmptyBorder(5, 10, 5, 10)
  // (WindowMenuStrip.java:295).
  const fitTitle = (top: HTMLElement) => {
    measure.font = getComputedStyle(top).font;
    top.style.width = `${Math.ceil(measure.measureText(top.getAttribute("aria-label") ?? "").width) + 20}px`;
  };
  // The menus as one menu bar for assistive technology; a press on the gaps between them still
  // moves the window like the rest of the bar.
  const strip = document.createElement("div");
  strip.className = "menu-strip";
  strip.setAttribute("role", "menubar");
  strip.toggleAttribute("data-tauri-drag-region", true);
  host.append(strip);
  for (const menu of JAVA_MENUS) {
    // The secondary engine menu follows the layout mode, the 跑谱贡献 menu 跑谱贡献 设置 >
    // 显示跑谱贡献菜单 and 快速启动 显示快速启动; they are built and shown or hidden as the view
    // changes (setVisible).
    const toggled = menu.when === "doubleEngine" || menu.when === "contribute" || menu.when === "quickLinks";
    if (!toggled && !visible(menu.when)) continue;
    // AI 陪练 and AI 解说 are buttons in the toolbar's lower row (toolbar.ts), not menus.
    if (menu.key === "Menu.aiCoach" || menu.key === "Menu.aiCommentary") continue;
    const top = document.createElement("span");
    top.className = "m";
    top.textContent = t(menu.key);
    top.dataset.key = menu.key;
    top.setAttribute("role", "menuitem");
    top.setAttribute("aria-haspopup", "menu");
    top.setAttribute("aria-expanded", "false");
    // The title's own text: the open dropdown inside it must not join its name.
    top.setAttribute("aria-label", top.textContent);
    strip.append(top);
    fitTitle(top);
    if (menu.when === "contribute" || menu.when === "quickLinks") {
      const update = () => {
        const shown = visible(menu.when);
        top.style.display = shown ? "" : "none";
        if (!shown && openMenu === top) closeMenus();
      };
      subscribe(update); update();
    }
    if (menu.key === "Menu.noEngine") {
      // The engine menus' titles (Menu.java:10419, EngineManager.reStartEngine2): "[n]: name" of the
      // foreground engine, or of the secondary engine on 双引擎模式's menu, else Menu.noEngine.
      const secondary = menu.when === "doubleEngine";
      if (secondary) secondaryMenu = top;
      const update = () => {
        if (secondary) {
          const shown = visible(menu.when);
          top.style.display = shown ? "" : "none";
          if (!shown && openMenu === top) closeMenus();
        }
        const engine = secondary ? state.view?.engine.secondary : state.view?.engine.foreground;
        // SwingEngineGameChrome: 对战中 and disabled while a batch runs (the menu strip shows no
        // menu icons, so Playing2 / ready2 are not visible there; java-ref-ui j08a, j09a). Open,
        // on the bar or inside 更多菜单, it closes.
        const match = secondary ? null : state.view?.engine_match;
        top.classList.toggle("match-off", !!match);
        top.setAttribute("aria-disabled", String(!!match));
        if (match && (openMenu === top || (openMenu?.dataset.key === OVERFLOW.key && top.classList.contains("overflowed")))) closeMenus();
        // 跑谱贡献中 while the contribution runs (ContributeEngine.java:175, 716).
        const contributing = !secondary && !!state.view?.contribution?.active;
        const text = match ? t("EngineManager.engineGamePlaying") : contributing ? t("Contribute.engineMenu.contributing") : engine ? `[${engine.index + 1}]: ${engine.name}` : t("Menu.noEngine");
        if (top.firstChild?.textContent === text) return;
        top.setAttribute("aria-label", text);
        if (top.firstChild?.nodeType === Node.TEXT_NODE) top.firstChild.textContent = text;
        else top.prepend(document.createTextNode(text));
        fitTitle(top);
      };
      subscribe(update); update();
    }
    const open = (keyboard = false) => {
      replaceMenu();
      showDropdown(top, dropdown(menu.items), keyboard);
    };
    tops.set(menu, { top, open });
    pressable(top, open);
  }
  // 更多菜单: in a window too narrow for every menu beside the mark, the title's padding and the
  // window buttons, the last menus move into it, so the window buttons always stay on the bar.
  const more = document.createElement("span");
  more.className = "m m-overflow";
  more.dataset.key = OVERFLOW.key;
  more.title = product("menuOverflow");
  more.setAttribute("role", "menuitem");
  more.setAttribute("aria-haspopup", "menu");
  more.setAttribute("aria-expanded", "false");
  more.setAttribute("aria-label", more.title);
  more.append(icon("more"));
  more.style.display = "none";
  strip.append(more);
  const openMore = (keyboard = false) => {
    replaceMenu();
    showDropdown(more, overflowDropdown(), keyboard);
  };
  tops.set(OVERFLOW, { top: more, open: openMore });
  pressable(more, openMore);
  const fit = () => {
    const bar = getComputedStyle(host);
    const gap = parseFloat(bar.columnGap) || 0;
    let room = host.clientWidth - parseFloat(bar.paddingLeft) - parseFloat(bar.paddingRight) - gap * (host.children.length - 1);
    for (const child of host.children) {
      if (child === strip || !(child instanceof HTMLElement)) continue;
      // The title gives up its text, not its padding.
      const own = getComputedStyle(child);
      room -= child.id === "windowTitle" ? parseFloat(own.paddingLeft) + parseFloat(own.paddingRight) : child.offsetWidth + parseFloat(own.marginLeft) + parseFloat(own.marginRight);
    }
    const stripGap = parseFloat(getComputedStyle(strip).columnGap) || 0;
    const menus = [...tops].filter(([menu, { top }]) => menu !== OVERFLOW && top.style.display !== "none").map(([, { top }]) => top);
    const width = (top: HTMLElement) => parseFloat(top.style.width) || top.offsetWidth;
    const all = menus.reduce((sum, top, k) => sum + width(top) + (k > 0 ? stripGap : 0), 0);
    // Every menu fits, or as many as leave room for 更多菜单 itself.
    const limit = all <= room ? Infinity : room - 32 - stripGap;
    let used = 0;
    let held = 0;
    menus.forEach((top, k) => {
      used += width(top) + (k > 0 ? stripGap : 0);
      const out = used > limit;
      if (out) held++;
      if (out && openMenu === top) closeMenus();
      top.classList.toggle("overflowed", out);
    });
    if (!held && openMenu === more) closeMenus();
    more.style.display = held ? "" : "none";
  };
  new ResizeObserver(fit).observe(host);
  // The window buttons join the bar after the menus; the menus' titles change with the view.
  new MutationObserver(fit).observe(host, { childList: true });
  subscribe(fit);
  // The session's frame font size and the theme's font apply when the appearance does (start-up,
  // before the window shows, and every theme change): the titles are measured again in it.
  onAppearanceChange(() => {
    for (const [menu, { top }] of tops) if (menu !== OVERFLOW) fitTitle(top);
    fit();
  });
  fit();
  // In the capture phase: a press on the window bar's free space starts moving the window before
  // it reaches the document's other listeners, and must still close an open menu.
  document.addEventListener("mousedown", (e) => {
    if (openMenu && !openMenu.contains(e.target as Node)) closeMenus();
  }, true);
  // Swing's menu keys: F10 opens the first enabled menu on the bar with its first item active, or
  // closes the open one; in an open menu or toolbar popup the keys of menuKey work, Left / Right on
  // a menu's outer level going to the neighbouring enabled menu. Not over a modal dialog (the bar
  // is inert then), 自由涂鸦 or the board's right-click menu.
  document.addEventListener("keydown", (e) => {
    const f10 = e.key === "F10" && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey;
    const menu = openMenu;
    if (menu) {
      const root = menu.querySelector<HTMLElement>(":scope > .dropdown");
      if (f10) closeMenus();
      else if (!root || !menuKey(e, root, closeMenus, menu.classList.contains("popup-anchor") ? undefined : stepTop)) return;
    } else {
      if (!f10 || host.closest("[inert]") || paintingOpen() || contextMenuOpen()) return;
      const first = [...tops.values()].find(({ top }) => onBar(top) && enabledTop(top));
      if (!first) return;
      first.open(true);
    }
    e.preventDefault();
    e.stopPropagation();
  }, true);
}

/** Whether a menu is open (workbench shortcuts are inactive then, like a Swing popup). */
export function menuOpen(): boolean {
  return openMenu !== null;
}

/** The secondary engine menu's title while it shows (acceptance probe), else null. */
export function secondaryMenuTitle(): string | null {
  return secondaryMenu && secondaryMenu.style.display !== "none" ? secondaryMenu.firstChild?.textContent ?? null : null;
}

/** Menu.functionPath: the first item with this Java action id, depth first in menu order, with the
 *  submenus leading to it. */
function functionPath(id: string): { menu: TopMenu; nodes: MenuNode[] } | null {
  const find = (items: MenuNode[]): MenuNode[] | null => {
    for (const node of items) {
      if (node.t === "sep") continue;
      if (node.id === id) return [node];
      const below = node.items && find(node.items);
      if (below) return [node, ...below];
    }
    return null;
  };
  for (const menu of JAVA_MENUS) {
    const nodes = find(menu.items);
    if (nodes) return { menu, nodes };
  }
  return null;
}

/** Whether a submenu leads to anything this client implements. */
function implemented(items: MenuNode[]): boolean {
  return items.some((node) => node.t !== "sep" && (node.t === "dynamic" || (node.t === "submenu" ? implemented(node.items ?? []) : (COMMANDS[node.id ?? ""] ?? COMMANDS[node.key]) !== undefined)));
}

/** A Java menu destination: null when no menu has the id, "missing" when its item is not
 *  available in this client, "hidden" when it is not shown now, else "ok". Read-only. */
export function menuItemState(id: string): "ok" | "hidden" | "missing" | null {
  const path = functionPath(id);
  if (!path) return null;
  const leaf = path.nodes[path.nodes.length - 1];
  if (leaf.t === "sep") return null;
  if (leaf.t === "dynamic" && leaf.key !== "engine.list" && leaf.key !== "Menu.openRecent" && leaf.key !== "quickLinks.programs") return "missing";
  if (leaf.t === "submenu" && !implemented(leaf.items ?? [])) return "missing";
  const entry = leaf.t === "item" || leaf.t === "check" || leaf.t === "radio" ? (COMMANDS[leaf.id ?? ""] ?? COMMANDS[leaf.key]) : null;
  if (entry === undefined) return "missing";
  const shown = tops.has(path.menu) && visible(path.menu.when) && path.nodes.every((node) => node.t === "sep" || visible(node.when));
  if (!shown || (entry?.visible && !entry.visible())) return "hidden";
  if (leaf.t === "dynamic" && leaf.key === "engine.list" && (state.view?.engine.engines.length ?? 0) === 0) return "hidden";
  return "ok";
}

/** What clicking the menu item runs (JMenuItem.doClick, the row's click): null for a submenu, a
 *  dynamic entry or an item this client lacks. */
export function menuItemCommand(id: string): CommandEntry | null {
  const nodes = functionPath(id)?.nodes;
  const leaf = nodes?.[nodes.length - 1];
  if (!leaf || (leaf.t !== "item" && leaf.t !== "check" && leaf.t !== "radio")) return null;
  return COMMANDS[leaf.id ?? ""] ?? COMMANDS[leaf.key] ?? null;
}

/** Menu.locateFunction: opens the real menu path and selects the item (focus and the gold line)
 *  without invoking it or changing its setting. Enter then acts on it like the selected Swing item. */
export function locateMenuItem(id: string): boolean {
  const path = functionPath(id);
  const opener = path && tops.get(path.menu);
  if (!path || !opener || menuItemState(id) !== "ok") return false;
  let list: HTMLElement | null;
  if (opener.top.classList.contains("overflowed")) {
    // A menu 更多菜单 holds opens as that menu's submenu there.
    const more = tops.get(OVERFLOW);
    more?.open();
    const outer = more?.top.querySelector<HTMLElement>(":scope > .dropdown") ?? null;
    const held = outer && [...outer.children].find((child): child is HTMLElement => {
      const node = rowNodes.get(child);
      return node !== undefined && node.t !== "sep" && node.items === path.menu.items;
    });
    if (!outer || !held) return false;
    setActiveRow(outer, held, false);
    reveal(outer, held);
    list = expand(held, outer, path.menu.items);
  } else {
    opener.open();
    list = opener.top.querySelector<HTMLElement>(":scope > .dropdown");
  }
  let row: HTMLElement | undefined;
  for (const [depth, node] of path.nodes.entries()) {
    if (!list) return false;
    const rows = [...list.children].filter((child): child is HTMLElement => rowNodes.get(child) === node);
    if (node.t === "dynamic" && node.key === "engine.list") {
      // The selected engine's row, else the first one.
      const current = node.id === "engine.select-secondary" ? state.view?.engine.secondary : state.view?.engine.foreground;
      row = rows.find((r) => r.dataset.engineIndex === String(current?.index)) ?? rows[0];
    } else if (node.t === "dynamic" && node.key === "quickLinks.programs") {
      // The 快速启动 menu itself (Menu.java:6843): its first item, 设置 when there is no program.
      row = rows[0] ?? [...list.children].find((child): child is HTMLElement => (child as HTMLElement).dataset.key === "Menu.editProgram");
    } else row = rows[0];
    if (!row) return false;
    if (depth < path.nodes.length - 1) {
      setActiveRow(list, row, false);
      reveal(list, row);
      list = expand(row, list, node.t === "sep" ? [] : (node.items ?? []));
    }
  }
  if (!row) return false;
  selectLocated(row);
  return true;
}

/** A toolbar popup's item located like a menu item (BottomToolbar.locateFunction's JMenuItem
 *  branch, BottomToolbar.java:645-649): the popup opens above `anchor` with the item selected. */
export function locatePopupItem(anchor: HTMLElement, items: MenuNode[], id: string): boolean {
  openPopupMenu(anchor, items, true);
  const row = [...(openMenu?.querySelectorAll<HTMLElement>(".mi") ?? [])].find((r) => {
    const node = rowNodes.get(r);
    return node !== undefined && node.t !== "sep" && node.id === id;
  });
  if (!row) return false;
  selectLocated(row);
  return true;
}

/** Focus and the gold line on a located row, which becomes its menu's active row: Enter or Space
 *  then acts on it like the selected Swing item. */
function selectLocated(row: HTMLElement): void {
  row.classList.add("located-item");
  setActiveRow(row.parentElement as HTMLElement, row, false);
  reveal(row.parentElement as HTMLElement, row);
  row.focus({ preventScroll: true });
  highlight(row);
}
