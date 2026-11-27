// The menus as they stand now: Menu.java's items with their texts, check marks and enabled states,
// from the catalog (javaMenus.ts), the command catalog (commands.ts) and the view. The window's
// menu bar and toolbar popups (menubar.ts), the macOS system menu bar (nativeMenu.ts) and function
// search read it; it builds nothing.
import { run, state } from "../app";
import { COMMANDS, reasonText, type CommandEntry } from "../commands";
import type { DisplaySettings, EngineState } from "../contract/generated";
import { DARK_READY } from "../design/theme";
import { chooseEngine } from "../engine/catalog";
import { isChinese, product, t } from "../i18n";
import { JAVA_MENUS, type MenuNode, type TopMenu } from "./javaMenus";

/** A catalog node other than a separator. */
export type CatalogNode = Exclude<MenuNode, { t: "sep" }>;

/** One item of a menu as it shows now. */
export interface Row {
  t: "item" | "check" | "radio" | "submenu";
  /** The catalog node it stands for; the items of a dynamic list share theirs. */
  node: CatalogNode;
  /** Unique within its menu: the menu's path and the item's id or key. */
  uid: string;
  /** The automation key (`data-key`): the node's key, a quick link's own; none on an engine. */
  key?: string;
  text: string;
  checked: boolean;
  enabled: boolean;
  /** No catalog command: disabled and marked as not yet available. */
  unavailable: boolean;
  /** Destroys record content or the user's data (DESIGN.md 危险操作). */
  danger: boolean;
  /** The item's tooltip. */
  title?: string;
  /** An engine of the engine menu: its index and its state (running or not), whose mark (Java's
   *  ● ◐ ○) shows while it is not the checked one. */
  engine?: { index: number; state: EngineState | null };
  /** What choosing the item runs; absent on a submenu and on an unavailable item. */
  command?: CommandEntry;
  /** A submenu's items. */
  children?: MenuNode[];
}

export type Entry = { t: "sep" } | Row;

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

/** File > 最近打开: newest first as "1: name", full path as tooltip (Menu.java:7192-7208). Entries
 *  open without changing the recent list; a missing file shows the normal open failure. */
function recentItems(): MenuNode[] {
  return (state.view?.recent_files ?? []).map((path, k) => ({ t: "item", key: `${k + 1}: ${fileName(path)}`, id: `${RECENT}${path}` }));
}

/** Items that destroy record content or the user's data, in the danger ink like the bottom bar's
 *  清空棋盘 / 删除 (DESIGN.md 危险操作): 编辑's 清空棋盘, 删除一手, 删除分支, 摆棋's 清空全部设置子,
 *  更多's 清空棋盘 / 删除, and 帮助's 清除所有个人数据. */
const DANGER = new Set(["menu.clearBoard", "menu.delete", "menu.deleteBranch", "menu.setupClearAll", "bottombar.more.clearButton", "bottombar.more.deleteMove", "menu.clearUserData"]);

/** 落子评价's count where a menu holds no text field: the Windows item, which asks for it in a
 *  dialog (setCustomMoves, Menu.java:9365-9397). */
const RANK_DIALOG: CatalogNode = { t: "check", key: "Menu.rankMenu.setCustomMoves", suffix: "{display.moveRankCustom}" };

/** A plain item of a dynamic list. */
function listed(node: CatalogNode, uid: string, text: string, command: CommandEntry, more: Partial<Row> = {}): Row {
  return { t: "item", node, uid, text, checked: false, enabled: true, unavailable: false, danger: false, command, ...more };
}

/** The engine menu's engines (Menu.java:7407-7429), the twentieth slot being the overflow chooser;
 *  engine.select-secondary is the same list for 双引擎模式's secondary slot (Menu.java:7453-7485). */
function engineRows(node: CatalogNode, uid: (name: string) => string): Row[] {
  const secondary = node.id === "engine.select-secondary";
  const current = secondary ? state.view?.engine.secondary : state.view?.engine.foreground;
  const engines = state.view?.engine.engines ?? [];
  const rows = engines.slice(0, engines.length > 20 ? 19 : 20).map((engine, index) => listed(node, uid(`engine.${index}`), `[${index + 1}] ${engine.name}`, {
    run: () => void run(secondary ? { type: "SwitchSecondaryEngine", index } : { type: "SwitchEngine", index }),
  }, {
    t: "radio",
    checked: current?.index === index,
    title: engine.command,
    engine: { index, state: engine.state },
  }));
  if (engines.length > 20) rows.push(listed(node, uid("engine.more"), t("Menu.moreEngines"), { run: () => void chooseEngine(false, secondary) }));
  return rows;
}

/** Menu.refreshQuickLinkItems: one item per saved program, named as saved; choosing one starts it
 *  (FastLink.startProgram). */
function quickLinkRows(node: CatalogNode, uid: (name: string) => string): Row[] {
  return (state.view?.settings.quick_links.programs ?? []).map((program, index) => {
    const key = `quickLinks.program.${index}`;
    return listed(node, uid(key), program.name, { run: () => void run({ type: "LaunchQuickLink", index }) }, { key, title: program.command });
  });
}

/** A catalog item or submenu as it shows now; null while its command hides it (a menu-open
 *  listener's setVisible(false)). */
function catalogRow(node: CatalogNode, uid: string): Row | null {
  const tip = tooltip(node.key);
  const base = { node, uid, key: node.key, text: label(node), checked: false, danger: node.id !== undefined && DANGER.has(node.id) };
  if (node.t === "submenu" || node.key === "Menu.openRecent") {
    const children = node.key === "Menu.openRecent" ? recentItems() : (node.items ?? []);
    // A submenu with nothing in the catalog leads to nothing this client has; 最近打开 may be empty.
    const empty = children.length === 0 && node.key !== "Menu.openRecent";
    return { ...base, t: "submenu", children, enabled: !empty, unavailable: empty, title: empty ? product("unavailable") : tip };
  }
  const recent = node.id?.startsWith(RECENT) ? node.id.slice(RECENT.length) : null;
  const command: CommandEntry | undefined = recent
    ? { run: () => run({ type: "OpenRecord", path: recent, source: "Recent" }) }
    : (COMMANDS[node.id ?? ""] ?? COMMANDS[node.key]);
  if (command?.visible && !command.visible()) return null;
  const kind = node.t === "check" || node.t === "radio" ? node.t : "item";
  const reason = command?.disabledReason?.() ?? null;
  return {
    ...base,
    t: kind,
    text: command?.label?.() ?? base.text,
    checked: kind !== "item" && !!command?.checked?.(),
    enabled: command !== undefined && reason === null && (!command.enabled || command.enabled()),
    unavailable: command === undefined,
    title: command === undefined ? product("unavailable") : reason !== null ? reasonText(reason) : (recent ?? tip),
    command,
  };
}

/** The items of one menu level as they show now: hidden items left out, separators only between
 *  items. `at` is the level's path, which makes the uids. `controls`: whether the menu can hold a
 *  text field or a button inside an item (the window's menus); where it cannot (the system menu
 *  bar), 落子评价's count is asked for in a dialog and a button is an item of its own after its
 *  item. */
export function entries(items: MenuNode[], at = "", controls = true): Entry[] {
  const out: Entry[] = [];
  const taken = new Set<string>();
  const uid = (name: string): string => {
    let id = `${at}/${name}`;
    for (let k = 2; taken.has(id); k++) id = `${at}/${name}#${k}`;
    taken.add(id);
    return id;
  };
  for (const item of items) {
    if (item.t === "sep") {
      if (out.length > 0 && out[out.length - 1].t !== "sep") out.push(item);
      continue;
    }
    if (!visible(item.when)) continue;
    const node = !controls && item.key === "Menu.rankMenu.lblCustomMove" ? RANK_DIALOG : item;
    if (node.t === "dynamic" && node.key === "engine.list") out.push(...engineRows(node, uid));
    else if (node.t === "dynamic" && node.key === "quickLinks.programs") out.push(...quickLinkRows(node, uid));
    else if (node.t !== "dynamic" || node.key === "Menu.openRecent") {
      const row = catalogRow(node, uid(node.id ?? node.key));
      if (!row) continue;
      out.push(row);
      const button = row.command?.button;
      if (button && !controls) {
        const key = `${node.id ?? node.key}.set`;
        out.push(listed(node, uid(key), `${button.item?.() ?? button.label()}…`, { run: button.run }, { key }));
      }
    }
  }
  if (out[out.length - 1]?.t === "sep") out.pop();
  return out;
}

/** Whether a menu is on the bar now: AI 陪练 and AI 解说 are toolbar buttons (toolbar.ts), the
 *  conditional menus follow their settings (setVisible). */
export function topShown(menu: TopMenu): boolean {
  return menu.key !== "Menu.aiCoach" && menu.key !== "Menu.aiCommentary" && visible(menu.when);
}

/** The engine menu takes no press while an 引擎对战 batch runs (Menu.engineMenu.setEnabled(false),
 *  SwingEngineGameChrome.java:49-53). */
export function topEnabled(menu: TopMenu): boolean {
  return !(menu.key === "Menu.noEngine" && menu.when !== "doubleEngine" && state.view?.engine_match);
}

/** A menu's title. The engine menus' (Menu.java:10419, EngineManager.reStartEngine2): "[n]: name" of
 *  the foreground engine, or of the secondary engine on 双引擎模式's menu, else Menu.noEngine; the
 *  foreground one reads 对战中 while an 引擎对战 batch runs (SwingEngineGameChrome; the strip shows no
 *  menu icons, so Playing2 / ready2 are not visible there, java-ref-ui j08a, j09a) and 跑谱贡献中
 *  while the contribution runs (ContributeEngine.java:175, 716). */
export function topTitle(menu: TopMenu): string {
  if (menu.key !== "Menu.noEngine") return t(menu.key);
  const secondary = menu.when === "doubleEngine";
  if (!secondary && state.view?.engine_match) return t("EngineManager.engineGamePlaying");
  if (!secondary && state.view?.contribution?.active) return t("Contribute.engineMenu.contributing");
  const engine = secondary ? state.view?.engine.secondary : state.view?.engine.foreground;
  return engine ? `[${engine.index + 1}]: ${engine.name}` : t("Menu.noEngine");
}

/** The secondary engine menu's title while it shows (acceptance probe), else null. */
export function secondaryMenuTitle(): string | null {
  const menu = JAVA_MENUS.find((m) => m.when === "doubleEngine");
  return menu && topShown(menu) ? topTitle(menu) : null;
}

/** Menu.functionPath: the first item with this Java action id, depth first in menu order, with the
 *  submenus leading to it. */
export function functionPath(id: string): { menu: TopMenu; nodes: MenuNode[] } | null {
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
 *  available in this client, "hidden" when it is not shown now, "disabled" when its menu is
 *  disabled (the engine menu while an engine game runs), else "ok". */
export function menuItemState(id: string): "ok" | "hidden" | "missing" | "disabled" | null {
  const path = functionPath(id);
  if (!path) return null;
  const leaf = path.nodes[path.nodes.length - 1];
  if (leaf.t === "sep") return null;
  if (leaf.t === "dynamic" && leaf.key !== "engine.list" && leaf.key !== "Menu.openRecent" && leaf.key !== "quickLinks.programs") return "missing";
  if (leaf.t === "submenu" && !implemented(leaf.items ?? [])) return "missing";
  const entry = leaf.t === "item" || leaf.t === "check" || leaf.t === "radio" ? (COMMANDS[leaf.id ?? ""] ?? COMMANDS[leaf.key]) : null;
  if (entry === undefined) return "missing";
  const shown = topShown(path.menu) && path.nodes.every((node) => node.t === "sep" || visible(node.when));
  if (!shown || (entry?.visible && !entry.visible())) return "hidden";
  if (leaf.t === "dynamic" && leaf.key === "engine.list" && (state.view?.engine.engines.length ?? 0) === 0) return "hidden";
  // Its menu takes no press now, so search may not open it either.
  if (!topEnabled(path.menu)) return "disabled";
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
