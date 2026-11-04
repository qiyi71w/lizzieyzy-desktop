// Java menu bar (Menu.java): every top-level menu and item in Java order. Items run catalog
// commands; entries without one are disabled and marked as not yet available.
import { run, state, subscribe, unavailable } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { DisplaySettings } from "../contract/generated";
import { setRankCustomText } from "../display/commands";
import { isChinese, product, t } from "../i18n";
import { JAVA_MENUS, type MenuNode, type TopMenu } from "./javaMenus";
import { chooseEngine } from "../engine/catalog";
import { highlight } from "../search/highlight";
import { buildAiCoachButton } from "../humansl/commands";

/** Java defaults for conditional menus (Config.java: show-quick-links false, show-Contribute true;
 *  the secondary engine menu needs double-engine mode; ReadBoard is Windows only). */
function visible(when: string | undefined): boolean {
  switch (when) {
    case undefined:
      return true;
    case "chinese":
      return isChinese();
    case "windows":
      return navigator.userAgent.includes("Windows");
    // Config.useJavaLooks defaults to !OS.isWindows() (Config.java:1647).
    case "javaLooks":
      return !navigator.userAgent.includes("Windows");
    case "contribute":
      return true;
    // Menu.setEngineMenuone2status: the secondary engine menu shows in 双引擎模式 only.
    case "doubleEngine":
      return state.view?.settings.extra_mode === "DoubleEngine";
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

function closeMenus() {
  openMenu?.classList.remove("open");
  openMenu?.querySelector(".dropdown")?.remove();
  if (openMenu?.classList.contains("popup-anchor")) openMenu.remove();
  openMenu = null;
}

/** A JPopupMenu shown below `anchor` (a toolbar button), drawn and closed like the menu bar's. */
export function openPopupMenu(anchor: HTMLElement, items: MenuNode[], above = false): void {
  closeMenus();
  const r = anchor.getBoundingClientRect();
  const holder = document.createElement("div");
  holder.className = "popup-anchor open";
  holder.style.left = `${r.left}px`;
  holder.style.top = `${r.bottom}px`;
  holder.append(dropdown(items));
  document.body.append(holder);
  if (above) holder.style.top = `${r.top - holder.firstElementChild!.getBoundingClientRect().height}px`;
  openMenu = holder;
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

/** Shows a submenu's dropdown beside its row, closing the row's open siblings first. */
function expand(row: HTMLElement, list: HTMLElement, children: MenuNode[]): HTMLElement {
  list.querySelectorAll(":scope > .mi > .dropdown").forEach((d) => d.remove());
  const sub = dropdown(children);
  if (sub.childElementCount === 0) sub.classList.add("empty");
  row.append(sub);
  return sub;
}

/** One dropdown level, rebuilt each time it opens so checked/enabled states are current. */
function dropdown(items: MenuNode[]): HTMLElement {
  const list = document.createElement("div");
  list.className = "dropdown";
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
    if (node.t === "dynamic" && node.key !== "Menu.openRecent") continue;
    const row = document.createElement("div");
    row.className = "mi";
    row.dataset.key = node.key;
    rowNodes.set(row, node);
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
      row.addEventListener("mouseenter", () => expand(row, list, children));
    } else {
      const recent = node.id?.startsWith(RECENT) ? node.id.slice(RECENT.length) : null;
      if (recent) row.title = recent;
      const entry: CommandEntry | undefined = recent
        ? { run: () => run({ type: "OpenRecord", path: recent, source: "Recent" }) }
        : (COMMANDS[node.id ?? ""] ?? COMMANDS[node.key]);
      if (entry?.visible && !entry.visible()) continue;
      if (node.t === "check" || node.t === "radio") {
        mark.textContent = entry?.checked?.() ? "✓" : "";
        row.classList.add("checkable");
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
  return list;
}

/** File > 最近打开: newest first as "1: name", full path as tooltip (Menu.java:7192-7208). Entries
 *  open without changing the recent list; a missing file shows the normal open failure. */
function recentItems(): MenuNode[] {
  return (state.view?.recent_files ?? []).map((path, k) => ({ t: "item", key: `${k + 1}: ${fileName(path)}`, id: `${RECENT}${path}` }));
}

/** The 双引擎模式 secondary engine menu's title element. */
let secondaryMenu: HTMLElement | null = null;

/** The built top-level menus and how each opens. */
const tops = new Map<TopMenu, { top: HTMLElement; open: () => void }>();

export function buildMenubar(host: HTMLElement): void {
  const measure = document.createElement("canvas").getContext("2d")!;
  for (const menu of JAVA_MENUS) {
    // The secondary engine menu follows the layout mode; it is built hidden.
    if (menu.when !== "doubleEngine" && !visible(menu.when)) continue;
    if (menu.key === "Menu.aiCoach") {
      // AI 陪练 is a button on the menu bar (Menu.java:5699-5732), its label following the session.
      host.append(buildAiCoachButton());
      continue;
    }
    if (menu.key === "Menu.aiCommentary") {
      // AI 解说 is a button on the menu bar (Menu.java:5713-5732)
      const pill = document.createElement("span");
      pill.className = "pill commentary unavailable";
      pill.textContent = t(menu.key);
      pill.title = product("unavailable");
      pill.onclick = unavailable;
      host.append(pill);
      continue;
    }
    const top = document.createElement("span");
    top.className = "m";
    top.textContent = t(menu.key);
    top.dataset.key = menu.key;
    host.append(top);
    // MenuButton: text width plus EmptyBorder(5, 10, 5, 10) (WindowMenuStrip.java:295)
    measure.font = getComputedStyle(top).font;
    top.style.width = `${Math.ceil(measure.measureText(top.textContent).width) + 20}px`;
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
        // menu icons, so Playing2 / ready2 are not visible there; java-ref-ui j08a, j09a).
        const match = secondary ? null : state.view?.engine_match;
        top.classList.toggle("match-off", !!match);
        const text = match ? t("EngineManager.engineGamePlaying") : engine ? `[${engine.index + 1}]: ${engine.name}` : t("Menu.noEngine");
        if (top.firstChild?.textContent === text) return;
        if (top.firstChild?.nodeType === Node.TEXT_NODE) top.firstChild.textContent = text;
        else top.prepend(document.createTextNode(text));
        measure.font = getComputedStyle(top).font;
        top.style.width = `${Math.ceil(measure.measureText(text).width) + 20}px`;
      };
      subscribe(update); update();
    }
    const open = () => {
      closeMenus();
      openMenu = top;
      top.classList.add("open");
      top.append(dropdown(menu.items));
    };
    tops.set(menu, { top, open });
    top.addEventListener("mousedown", (e) => {
      e.preventDefault();
      if (openMenu === top && !(e.target as HTMLElement).closest(".dropdown")) closeMenus();
      else if (openMenu !== top) open();
    });
    top.addEventListener("mouseenter", () => {
      if (openMenu && openMenu !== top) open();
    });
  }
  document.addEventListener("mousedown", (e) => {
    if (openMenu && !openMenu.contains(e.target as Node)) closeMenus();
  });
  document.addEventListener("keydown", (e) => {
    if (openMenu && e.key === "Escape") {
      closeMenus();
      e.stopPropagation();
    }
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
  if (leaf.t === "dynamic" && leaf.key !== "engine.list" && leaf.key !== "Menu.openRecent") return "missing";
  if (leaf.t === "submenu" && !implemented(leaf.items ?? [])) return "missing";
  const entry = leaf.t === "item" || leaf.t === "check" || leaf.t === "radio" ? (COMMANDS[leaf.id ?? ""] ?? COMMANDS[leaf.key]) : null;
  if (entry === undefined) return "missing";
  const shown = tops.has(path.menu) && visible(path.menu.when) && path.nodes.every((node) => node.t === "sep" || visible(node.when));
  if (!shown || (entry?.visible && !entry.visible())) return "hidden";
  if (leaf.t === "dynamic" && leaf.key === "engine.list" && (state.view?.engine.engines.length ?? 0) === 0) return "hidden";
  return "ok";
}

/** Menu.locateFunction: opens the real menu path and selects the item (focus and the gold line)
 *  without invoking it or changing its setting. Enter then acts on it like the selected Swing item. */
export function locateMenuItem(id: string): boolean {
  const path = functionPath(id);
  const opener = path && tops.get(path.menu);
  if (!path || !opener || menuItemState(id) !== "ok") return false;
  opener.open();
  let list = opener.top.querySelector<HTMLElement>(":scope > .dropdown");
  let row: HTMLElement | undefined;
  for (const [depth, node] of path.nodes.entries()) {
    if (!list) return false;
    const rows = [...list.children].filter((child): child is HTMLElement => rowNodes.get(child) === node);
    if (node.t === "dynamic" && node.key === "engine.list") {
      // The selected engine's row, else the first one.
      const current = node.id === "engine.select-secondary" ? state.view?.engine.secondary : state.view?.engine.foreground;
      row = rows.find((r) => r.dataset.engineIndex === String(current?.index)) ?? rows[0];
    } else row = rows[0];
    if (!row) return false;
    if (depth < path.nodes.length - 1) list = expand(row, list, node.t === "sep" ? [] : (node.items ?? []));
  }
  if (!row) return false;
  const located = row;
  const leaf = path.nodes[path.nodes.length - 1];
  located.tabIndex = -1;
  located.classList.add("located-item");
  located.addEventListener("keydown", (e) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    e.preventDefault();
    if (leaf.t === "submenu" && list) expand(located, list, leaf.items ?? []);
    else located.click();
  });
  located.scrollIntoView({ block: "nearest", inline: "nearest" });
  located.focus({ preventScroll: true });
  highlight(located);
  return true;
}
