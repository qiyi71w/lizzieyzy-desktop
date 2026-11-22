// The window's menus (Menu.java), drawn from the menu model (menuModel.ts): the menu bar on Windows
// and Linux, the toolbar popups everywhere, and on macOS, whose menus are the system menu bar's
// (nativeMenu.ts), the menu panel F10 and function search open. Items run catalog commands;
// entries without one are disabled and marked as not yet available.
import { state, subscribe } from "../app";
import { setRankCustomText } from "../display/commands";
import { product, registerProduct, t } from "../i18n";
import { JAVA_MENUS, type MenuNode, type TopMenu } from "./javaMenus";
import { os } from "../design/theme";
import { icon } from "../design/icons";
import { highlight } from "../search/highlight";
import { paintingOpen } from "../display/painting";
import { contextMenuOpen } from "../editing/contextMenu";
import { onAppearanceChange } from "../appearance/state";
import { activateFirst, bindMenuRows, menuKey, OPEN_SUB, reveal, setActiveRow, subOpeners } from "./menuKeys";
import { entries, functionPath, menuItemState, topEnabled, topShown, topTitle, type Row } from "./menuModel";

/** 更多菜单: the button at the menu bar's end that holds the menus a narrow window has no room for. */
registerProduct({
  ZhCn: { menuOverflow: "更多菜单" },
  ZhTw: { menuOverflow: "更多選單" },
  EnUs: { menuOverflow: "More menus" },
  JaJp: { menuOverflow: "その他のメニュー" },
  KoKr: { menuOverflow: "메뉴 더 보기" },
  ThTh: { menuOverflow: "เมนูเพิ่มเติม" },
});

let openMenu: HTMLElement | null = null;
/** The element focused before the menus opened; it gets the focus back when they close. */
let focusBefore: Element | null = null;

/** Whether the latest input was a key rather than a pointer press: a toolbar popup opened by a key
 *  starts with its first item active, one opened by the mouse with none. */
let keyboardInput = false;
document.addEventListener("keydown", () => (keyboardInput = true), true);
document.addEventListener("pointerdown", () => (keyboardInput = false), true);

/** Closes the open menu or toolbar popup. Focus left in it (or lost with it) returns to where it
 *  was before the menus opened, unless another menu takes over (`handover`). */
export function closeMenus(handover = false): void {
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

/** Shows `list` as a popup below the box `r` or above it, drawn and closed like the menu bar's
 *  menus; like Swing's, it moves left rather than leave the window. One taller than the room on its
 *  side scrolls inside, as the menu bar's dropdowns do. */
function showPopup(r: { left: number; top: number; bottom: number }, list: HTMLElement, above: boolean, keyboard: boolean): void {
  replaceMenu();
  const holder = document.createElement("div");
  holder.className = "popup-anchor open";
  holder.style.left = `${r.left}px`;
  holder.style.top = `${r.bottom}px`;
  list.style.maxHeight = `${above ? r.top - EDGE : window.innerHeight - EDGE - r.bottom}px`;
  holder.append(list);
  document.body.append(holder);
  const box = list.getBoundingClientRect();
  if (box.right > window.innerWidth) holder.style.left = `${Math.max(0, r.left - (box.right - window.innerWidth))}px`;
  if (above) holder.style.top = `${r.top - box.height}px`;
  openMenu = holder;
  if (keyboard) activateFirst(list);
}

/** A JPopupMenu shown below `anchor` (a toolbar button), or above it. */
export function openPopupMenu(anchor: HTMLElement, items: MenuNode[], above = false): void {
  showPopup(anchor.getBoundingClientRect(), dropdown(items), above, keyboardInput);
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

/** The catalog node each shown row stands for, so a function search can find it again. */
const rowNodes = new WeakMap<Element, MenuNode>();

/** The gap a dropdown keeps from the window's edges. */
const EDGE = 4;

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

/** One dropdown level, rebuilt each time it opens so checked/enabled states are current. */
function dropdown(items: MenuNode[]): HTMLElement {
  const list = document.createElement("div");
  list.className = "dropdown";
  // An open submenu stays beside its row while this level scrolls.
  list.addEventListener("scroll", () => {
    const sub = list.querySelector<HTMLElement>(OPEN_SUB);
    if (sub) placeSub(sub);
  });
  for (const entry of entries(items)) {
    if (entry.t !== "sep") {
      list.append(menuRow(entry, list));
      continue;
    }
    const sep = document.createElement("div");
    sep.className = "sep";
    list.append(sep);
  }
  bindMenuRows(list);
  return list;
}

/** The row of `entry` in the dropdown `list`: its check mark (an engine's state mark while not the
 *  checked one), its text, a text field or a button its item holds, and a submenu row's submenu. */
function menuRow(entry: Row, list: HTMLElement): HTMLElement {
  const row = document.createElement("div");
  row.className = "mi";
  if (entry.key !== undefined) row.dataset.key = entry.key;
  if (entry.engine) row.dataset.engineIndex = String(entry.engine.index);
  rowNodes.set(row, entry.node);
  if (entry.danger) row.classList.add("danger");
  const mark = document.createElement("span");
  mark.className = "mark";
  const text = document.createElement("span");
  text.className = "label";
  text.textContent = entry.text;
  row.append(mark, text);
  if (entry.title !== undefined) row.title = entry.title;
  if (entry.node.key === "Menu.rankMenu.lblCustomMove") row.append(rankCustomField());
  const itemField = entry.node.id ? itemFields.get(entry.node.id) : undefined;
  if (itemField) row.append(itemField());
  if (entry.t === "submenu") row.classList.add("sub");
  else if (entry.t !== "item") {
    mark.textContent = entry.checked ? "✓" : (entry.engine?.mark ?? "");
    row.classList.add("checkable");
    row.setAttribute("role", entry.t === "check" ? "menuitemcheckbox" : "menuitemradio");
    row.setAttribute("aria-checked", String(entry.checked));
  }
  if (!entry.enabled) row.classList.add("disabled");
  if (entry.unavailable) row.classList.add("unavailable");
  if (entry.t === "submenu") {
    // A disabled row opens nothing, though moving onto it still closes its siblings' submenus.
    const open = () => expand(row, list, row.classList.contains("disabled") ? [] : (entry.children ?? []));
    subOpeners.set(row, open);
    row.addEventListener("mouseenter", open);
    return row;
  }
  const command = entry.command;
  if (command && entry.enabled) {
    row.addEventListener("click", (e) => {
      e.stopPropagation();
      closeMenus();
      command.run();
    });
  }
  const inner = command?.button;
  if (inner) {
    const b = document.createElement("button");
    b.className = "mi-button";
    b.dataset.key = `${entry.node.id ?? entry.node.key}.set`;
    // An icon button (the "?" InfoMenuItem) is named and titled by its label.
    if (inner.icon) {
      b.classList.add("mi-icon-button");
      b.append(icon(inner.icon));
      b.title = inner.label();
      b.setAttribute("aria-label", b.title);
    } else b.textContent = inner.label();
    b.addEventListener("click", (e) => {
      e.stopPropagation();
      closeMenus();
      inner.run();
    });
    row.append(b);
  }
  return row;
}

/** The built top-level menus and how each opens (`keyboard`: with its first item active). */
const tops = new Map<TopMenu, { top: HTMLElement; open: (keyboard?: boolean) => void }>();

/** The window bar holding the menus. */
let barHost: HTMLElement | null = null;

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
function overflowedMenus(): TopMenu[] {
  return [...tops].filter(([menu, { top }]) => menu !== OVERFLOW && top.style.display !== "none" && top.classList.contains("overflowed")).map(([menu]) => menu);
}

/** A dropdown of whole menus, a submenu row each, named as the menu's title (an engine menu's is
 *  its engine); a disabled menu's row is disabled. 更多菜单's, and macOS's menu panel. */
function menusDropdown(menus: TopMenu[]): HTMLElement {
  const list = dropdown(menus.map((menu): MenuNode => ({ t: "submenu", key: menu.key, items: menu.items })));
  [...list.children].forEach((row, k) => {
    const name = topTitle(menus[k]);
    const text = row.querySelector(".label");
    if (text) text.textContent = name;
    row.setAttribute("aria-label", name);
    if (topEnabled(menus[k])) return;
    row.classList.add("disabled");
    row.setAttribute("aria-disabled", "true");
  });
  return list;
}

/** macOS: the menus are the system menu bar's (nativeMenu.ts), which a page cannot open. F10 and
 *  function search open them here instead: a panel under the window bar's mark with a row per menu,
 *  each opening like that menu (DESIGN.md 窗口栏). Returns the panel's list. */
function openMenuPanel(host: HTMLElement, keyboard: boolean): HTMLElement {
  const box = host.getBoundingClientRect();
  const mark = host.querySelector(".app-mark")?.getBoundingClientRect();
  const list = menusDropdown(JAVA_MENUS.filter(topShown));
  showPopup({ left: mark?.left ?? box.left, top: box.top, bottom: box.bottom }, list, false, keyboard);
  return list;
}

/** The menus of the window bar `host`: on Windows and Linux the menu bar; on macOS, whose menus are
 *  the system menu bar's, the menu panel. Either way the keys and presses of the open menu or
 *  toolbar popup. */
export function buildMenubar(host: HTMLElement): void {
  barHost = host;
  if (os !== "mac") buildStrip(host);
  // In the capture phase: a press on the window bar's free space starts moving the window before
  // it reaches the document's other listeners, and must still close an open menu.
  document.addEventListener("mousedown", (e) => {
    if (openMenu && !openMenu.contains(e.target as Node)) closeMenus();
  }, true);
  // Swing's menu keys: F10 opens the first enabled menu on the bar with its first item active (on
  // macOS the menu panel), or closes the open one; in an open menu or toolbar popup the keys of
  // menuKey work, Left / Right on a menu's outer level going to the neighbouring enabled menu. Not
  // over a modal dialog (the bar is inert then), 自由涂鸦 or the board's right-click menu.
  document.addEventListener("keydown", (e) => {
    const f10 = e.key === "F10" && !e.shiftKey && !e.ctrlKey && !e.altKey && !e.metaKey;
    const menu = openMenu;
    if (menu) {
      const root = menu.querySelector<HTMLElement>(":scope > .dropdown");
      if (f10) closeMenus();
      else if (!root || !menuKey(e, root, closeMenus, menu.classList.contains("popup-anchor") ? undefined : stepTop)) return;
    } else {
      if (!f10 || host.closest("[inert]") || paintingOpen() || contextMenuOpen()) return;
      if (os === "mac") openMenuPanel(host, true);
      else {
        const first = [...tops.values()].find(({ top }) => onBar(top) && enabledTop(top));
        if (!first) return;
        first.open(true);
      }
    }
    e.preventDefault();
    e.stopPropagation();
  }, true);
}

/** Windows and Linux: the menu bar, a title per menu after the mark, and 更多菜单 for the menus a
 *  narrow window has no room for. */
function buildStrip(host: HTMLElement): void {
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
    if (!toggled && !topShown(menu)) continue;
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
        const shown = topShown(menu);
        top.style.display = shown ? "" : "none";
        if (!shown && openMenu === top) closeMenus();
      };
      subscribe(update); update();
    }
    if (menu.key === "Menu.noEngine") {
      const secondary = menu.when === "doubleEngine";
      let matchShown = false;
      const update = () => {
        if (secondary) {
          const shown = topShown(menu);
          top.style.display = shown ? "" : "none";
          if (!shown && openMenu === top) closeMenus();
        }
        // Disabled while an 引擎对战 batch runs; open when the batch starts, on the bar or inside
        // 更多菜单, it closes.
        const match = !topEnabled(menu);
        top.classList.toggle("match-off", match);
        top.setAttribute("aria-disabled", String(match));
        if (match && !matchShown && (openMenu === top || (openMenu?.dataset.key === OVERFLOW.key && top.classList.contains("overflowed")))) closeMenus();
        matchShown = match;
        const text = topTitle(menu);
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
    showDropdown(more, menusDropdown(overflowedMenus()), keyboard);
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
}

/** Whether a menu is open (workbench shortcuts are inactive then, like a Swing popup). */
export function menuOpen(): boolean {
  return openMenu !== null;
}

/** Menu.locateFunction: opens the real menu path and selects the item (focus and the gold line)
 *  without invoking it or changing its setting. Enter then acts on it like the selected Swing item. */
export function locateMenuItem(id: string): boolean {
  const path = functionPath(id);
  if (!path || menuItemState(id) !== "ok") return false;
  const opener = tops.get(path.menu);
  let list: HTMLElement | null;
  if (os === "mac" || opener?.top.classList.contains("overflowed")) {
    // A menu 更多菜单 holds opens as that menu's submenu there; on macOS the menu panel holds them
    // all.
    let outer: HTMLElement | null = null;
    if (os === "mac") {
      if (barHost) outer = openMenuPanel(barHost, false);
    } else {
      const more = tops.get(OVERFLOW);
      more?.open();
      outer = more?.top.querySelector<HTMLElement>(":scope > .dropdown") ?? null;
    }
    const held = outer && [...outer.children].find((child): child is HTMLElement => {
      const node = rowNodes.get(child);
      return node !== undefined && node.t !== "sep" && node.items === path.menu.items;
    });
    if (!outer || !held) return false;
    setActiveRow(outer, held, false);
    reveal(outer, held);
    list = expand(held, outer, path.menu.items);
  } else if (opener) {
    opener.open();
    list = opener.top.querySelector<HTMLElement>(":scope > .dropdown");
  } else return false;
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
