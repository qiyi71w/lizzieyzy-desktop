// Java menu bar (Menu.java): every top-level menu and item in Java order. Items run catalog
// commands; entries without one are disabled and marked as not yet available.
import { run, state, subscribe, unavailable } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { isChinese, product, t } from "../i18n";
import { JAVA_MENUS, type MenuNode } from "./javaMenus";
import { chooseEngine } from "../engine/catalog";

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
    case "contribute":
      return true;
    default:
      return false;
  }
}

function label(node: { key: string; suffix?: string }): string {
  const suffix = node.suffix?.replace("{sp}", isChinese() ? "" : " ") ?? "";
  return t(node.key) + suffix;
}

let openMenu: HTMLElement | null = null;

function closeMenus() {
  openMenu?.classList.remove("open");
  openMenu?.querySelector(".dropdown")?.remove();
  openMenu = null;
}

function fileName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

/** Recent entries carry their path in the id. */
const RECENT = "recent:";

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
      const engines = state.view?.engine.engines ?? [];
      // Menu.java:7407-7429: the twentieth slot is the overflow chooser.
      engines.slice(0, engines.length > 20 ? 19 : 20).forEach((engine, index) => {
        const row = document.createElement("div"); row.className = "mi checkable";
        row.dataset.engineIndex = String(index); row.title = engine.command;
        const mark = document.createElement("span"); mark.className = "mark";
        mark.textContent = state.view?.engine.foreground?.index === index ? "✓" : engine.state === "Ready" ? "●" : engine.state === "Loading" ? "◐" : "○";
        const text = document.createElement("span"); text.className = "label"; text.textContent = `[${index + 1}] ${engine.name}`;
        row.append(mark, text);
        row.onclick = (event) => { event.stopPropagation(); closeMenus(); void run({ type: "SwitchEngine", index }); };
        list.append(row);
      });
      if (engines.length > 20) {
        const more = document.createElement("div"); more.className = "mi"; more.textContent = t("Menu.moreEngines");
        more.onclick = (event) => { event.stopPropagation(); closeMenus(); void chooseEngine(); }; list.append(more);
      }
      continue;
    }
    if (node.t === "dynamic" && node.key !== "Menu.openRecent") continue;
    const row = document.createElement("div");
    row.className = "mi";
    row.dataset.key = node.key;
    const text = document.createElement("span");
    text.className = "label";
    text.textContent = label(node);
    const mark = document.createElement("span");
    mark.className = "mark";
    row.append(mark, text);
    if (node.t === "submenu" || node.key === "Menu.openRecent") {
      row.classList.add("sub");
      const children = node.key === "Menu.openRecent" ? recentItems() : (node.items ?? []);
      if (children.length === 0 && node.key !== "Menu.openRecent") {
        row.classList.add("disabled", "unavailable");
        row.title = product("unavailable");
      }
      row.addEventListener("mouseenter", () => {
        list.querySelectorAll(":scope > .mi > .dropdown").forEach((d) => d.remove());
        const sub = dropdown(children);
        if (sub.childElementCount === 0) sub.classList.add("empty");
        row.append(sub);
      });
    } else {
      const recent = node.id?.startsWith(RECENT) ? node.id.slice(RECENT.length) : null;
      if (recent) row.title = recent;
      const entry: CommandEntry | undefined = recent
        ? { run: () => run({ type: "OpenRecord", path: recent, source: "Recent" }) }
        : (COMMANDS[node.id ?? ""] ?? COMMANDS[node.key]);
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

export function buildMenubar(host: HTMLElement): void {
  const measure = document.createElement("canvas").getContext("2d")!;
  for (const menu of JAVA_MENUS) {
    if (!visible(menu.when)) continue;
    if (menu.key === "Menu.aiCoach" || menu.key === "Menu.aiCommentary") {
      // AI 陪练 / AI 解说 are buttons on the menu bar (Menu.java:5730-5732)
      const pill = document.createElement("span");
      pill.className = `pill ${menu.key === "Menu.aiCoach" ? "coach" : "commentary"} unavailable`;
      pill.textContent = menu.key === "Menu.aiCoach" ? `◎  ${t(menu.key)}` : t(menu.key);
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
      const update = () => {
        const engine = state.view?.engine.foreground;
        const text = engine ? `[${engine.index + 1}]: ${engine.name}` : t("Menu.noEngine");
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
