// macOS: the menus in the system menu bar at the top of the screen. Java keeps its menus inside the
// window on every system (Lizzie.java:1108-1114); on macOS this product shows them where the system
// shows every program's, built from the same catalog and model as the window's menu bar
// (menuModel.ts) and kept current as the view and the page change. A system menu has no danger ink,
// tooltips, or text field or button inside an item: 落子评价's count is asked for in a dialog, a
// button is an item of its own and an engine's state mark joins its name. Java's items register no
// key equivalents, so the page keeps their keys. F10 and function search open the window's menu
// panel (menubar.ts).
import { CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu, type CheckMenuItemOptions, type IconMenuItem, type MenuItemOptions, type PredefinedMenuItemOptions, type SubmenuOptions } from "@tauri-apps/api/menu";
import { subscribe } from "../app";
import { COMMANDS } from "../commands";
import type { EngineState } from "../contract/generated";
import { product, registerProduct } from "../i18n";
import { JAVA_MENUS } from "./javaMenus";
import { entries, topEnabled, topShown, topTitle, type Entry } from "./menuModel";
import { closeMenus } from "./menubar";

/** The application menu's and 编辑's system items, named as macOS names them in each language. */
registerProduct({
  ZhCn: { macAbout: "关于 LizzieYzy Tauri", macServices: "服务", macHide: "隐藏 LizzieYzy Tauri", macHideOthers: "隐藏其他", macShowAll: "全部显示", macQuit: "退出 LizzieYzy Tauri", macUndo: "撤销", macRedo: "重做", macCut: "剪切", macCopy: "拷贝", macPaste: "粘贴", macSelectAll: "全选" },
  ZhTw: { macAbout: "關於 LizzieYzy Tauri", macServices: "服務", macHide: "隱藏 LizzieYzy Tauri", macHideOthers: "隱藏其他", macShowAll: "顯示全部", macQuit: "結束 LizzieYzy Tauri", macUndo: "還原", macRedo: "重做", macCut: "剪下", macCopy: "拷貝", macPaste: "貼上", macSelectAll: "全選" },
  EnUs: { macAbout: "About LizzieYzy Tauri", macServices: "Services", macHide: "Hide LizzieYzy Tauri", macHideOthers: "Hide Others", macShowAll: "Show All", macQuit: "Quit LizzieYzy Tauri", macUndo: "Undo", macRedo: "Redo", macCut: "Cut", macCopy: "Copy", macPaste: "Paste", macSelectAll: "Select All" },
  JaJp: { macAbout: "LizzieYzy Tauri について", macServices: "サービス", macHide: "LizzieYzy Tauri を非表示", macHideOthers: "ほかを非表示", macShowAll: "すべてを表示", macQuit: "LizzieYzy Tauri を終了", macUndo: "取り消す", macRedo: "やり直す", macCut: "カット", macCopy: "コピー", macPaste: "ペースト", macSelectAll: "すべてを選択" },
  KoKr: { macAbout: "LizzieYzy Tauri에 관하여", macServices: "서비스", macHide: "LizzieYzy Tauri 가리기", macHideOthers: "기타 가리기", macShowAll: "모두 보기", macQuit: "LizzieYzy Tauri 종료", macUndo: "실행 취소", macRedo: "실행 복귀", macCut: "오려두기", macCopy: "복사하기", macPaste: "붙여넣기", macSelectAll: "전체 선택" },
  ThTh: { macAbout: "เกี่ยวกับ LizzieYzy Tauri", macServices: "บริการ", macHide: "ซ่อน LizzieYzy Tauri", macHideOthers: "ซ่อนรายการอื่น", macShowAll: "แสดงทั้งหมด", macQuit: "ออกจาก LizzieYzy Tauri", macUndo: "เลิกทำ", macRedo: "ทำซ้ำ", macCut: "ตัด", macCopy: "คัดลอก", macPaste: "วาง", macSelectAll: "เลือกทั้งหมด" },
});

/** An item of the system menu bar as the model has it. */
interface Spec {
  /** Unique within its menu: menuModel's uid; a separator's is the uid of the item before it. */
  uid: string;
  kind: "sep" | "item" | "check" | "submenu";
  text: string;
  checked: boolean;
  enabled: boolean;
  run?: () => void;
  items?: Spec[];
}

/** Java's engine state marks (Menu.java:7407-7429), as text after the name. */
const ENGINE_MARK: Record<EngineState, string> = { Ready: "●", Loading: "◐" };

/** A menu level's items for the system menu bar, its submenus' with them. */
function specs(list: Entry[]): Spec[] {
  return list.map((entry, k): Spec => {
    if (entry.t === "sep") {
      const before = list[k - 1];
      return { uid: before && before.t !== "sep" ? before.uid : "", kind: "sep", text: "", checked: false, enabled: true };
    }
    if (entry.t === "submenu") {
      return { uid: entry.uid, kind: "submenu", text: entry.text, checked: false, enabled: entry.enabled, items: entry.enabled ? specs(entries(entry.children ?? [], entry.uid, false)) : [] };
    }
    // An engine's state mark joins its name: a system menu has no column for it.
    const mark = entry.engine?.state ? ENGINE_MARK[entry.engine.state] : "○";
    const text = entry.engine && !entry.checked ? `${entry.text} ${mark}` : entry.text;
    return { uid: entry.uid, kind: entry.t === "item" ? "item" : "check", text, checked: entry.checked, enabled: entry.enabled, run: entry.command?.run };
  });
}

/** The menus after the application menu: the catalog's shown menus, in Java's order. */
function menus(): Spec[] {
  return JAVA_MENUS.filter(topShown).map((menu): Spec => {
    const at = menu.when ? `${menu.key}[${menu.when}]` : menu.key;
    return { uid: at, kind: "submenu", text: topTitle(menu), checked: false, enabled: topEnabled(menu), items: specs(entries(menu.items, at, false)) };
  });
}

/** Runs a menu action as the window's own menus do: the menu or toolbar popup open in the window
 *  closes first, as a press elsewhere closes it, and nothing runs while a modal dialog, or a window
 *  that disabled the workbench, holds the window bar inert. */
function act(run: (() => void) | undefined): void {
  closeMenus();
  if (run && !document.getElementById("menubar")?.closest("[inert]")) run();
}

/** Menu text as written: the system menus take `&` for a mnemonic mark, `&&` for the character. */
const menuText = (text: string) => text.replaceAll("&", "&&");

type Handle = Submenu | MenuItem | PredefinedMenuItem | CheckMenuItem | IconMenuItem;

/** An item the system menu bar shows: what was last set on it, the page's hold on it and, for a
 *  submenu, its items. */
interface Shown {
  spec: Spec;
  handle: Handle | null;
  items: Shown[];
}

/** The menu bar once installed: its menu and the model's menus in it. */
let bar: { menu: Menu; items: Shown[] } | null = null;
/** Calls into the menus so far (acceptance probe): updates should take few. */
let calls = 0;

function counted<T>(call: Promise<T>): Promise<T> {
  calls++;
  return call;
}

function track(spec: Spec): Shown {
  return { spec, handle: null, items: (spec.items ?? []).map(track) };
}

function options(item: Shown): MenuItemOptions | CheckMenuItemOptions | SubmenuOptions | PredefinedMenuItemOptions {
  const { spec } = item;
  const text = menuText(spec.text);
  switch (spec.kind) {
    case "sep":
      return { item: "Separator" };
    case "submenu": {
      const items: (MenuItemOptions | CheckMenuItemOptions | SubmenuOptions | PredefinedMenuItemOptions)[] = item.items.map(options);
      // 编辑 ends with the system's text editing items: the text fields' ⌘Z, ⌘C, ⌘V and the rest
      // are theirs.
      if (spec.uid === "Menu.edit") {
        items.push(
          { item: "Separator" },
          { item: "Undo", text: product("macUndo") },
          { item: "Redo", text: product("macRedo") },
          { item: "Separator" },
          { item: "Cut", text: product("macCut") },
          { item: "Copy", text: product("macCopy") },
          { item: "Paste", text: product("macPaste") },
          { item: "SelectAll", text: product("macSelectAll") },
        );
      }
      return { text, enabled: spec.enabled, items };
    }
    case "check":
      return { text, enabled: spec.enabled, checked: spec.checked, action: () => choose(item) };
    case "item":
      return { text, enabled: spec.enabled, action: () => choose(item) };
  }
}

/** A click on an item. The system has toggled a check item's mark already: the record follows it,
 *  and the next update puts the command's state back. */
function choose(item: Shown): void {
  if (item.spec.kind === "check") item.spec = { ...item.spec, checked: !item.spec.checked };
  schedule();
  act(item.spec.run);
}

/** Lets go of the page's hold on a removed item and on everything in it. */
async function release(item: Shown): Promise<void> {
  await Promise.all(item.items.map(release));
  if (item.handle) await counted(item.handle.close());
}

/** Takes hold of the items just made in the submenu `owner` from options, and of the items in
 *  them; lets go of the others (编辑's text editing). */
async function attach(owner: Submenu, shown: Shown[]): Promise<void> {
  const handles = await counted(owner.items());
  await Promise.all(handles.map((handle, k) => {
    const item = shown[k];
    if (!item) return counted(handle.close());
    item.handle = handle;
    return handle instanceof Submenu && item.items.length > 0 ? attach(handle, item.items) : undefined;
  }));
}

/** Makes the item of `spec`, with its submenu's items, and takes hold of them. */
async function make(spec: Spec): Promise<{ item: Shown; handle: Handle }> {
  const item = track(spec);
  const made = options(item);
  const handle = await counted("item" in made ? PredefinedMenuItem.new(made) : "items" in made ? Submenu.new(made) : "checked" in made ? CheckMenuItem.new(made) : MenuItem.new(made));
  item.handle = handle;
  if (handle instanceof Submenu && item.items.length > 0) await attach(handle, item.items);
  return { item, handle };
}

/** Sets an item's changed text, enabled state and check mark, one call each, and brings its
 *  submenu up to date. */
async function refresh(item: Shown, spec: Spec): Promise<void> {
  const handle = item.handle;
  if (handle && !(handle instanceof PredefinedMenuItem)) {
    if (spec.text !== item.spec.text) await counted(handle.setText(menuText(spec.text)));
    if (spec.enabled !== item.spec.enabled) await counted(handle.setEnabled(spec.enabled));
    if (spec.checked !== item.spec.checked && handle instanceof CheckMenuItem) await counted(handle.setChecked(spec.checked));
  }
  item.spec = spec;
  if (handle instanceof Submenu) await update(handle, 0, item.items, spec.items ?? []);
}

const key = (spec: Spec) => `${spec.kind}:${spec.uid}`;

/** Brings the model's items `shown` in `owner`, after its first `offset` items, to `next`: items
 *  the model no longer has leave, new ones are made in place, and the others are refreshed. Should
 *  the kept items have changed order, all of them are made again. */
async function update(owner: Menu | Submenu, offset: number, shown: Shown[], next: Spec[]): Promise<void> {
  const wanted = next.map(key);
  const kept = shown.map((item) => key(item.spec)).filter((k) => wanted.includes(k));
  const ordered = wanted.filter((k) => kept.includes(k)).join("|") === kept.join("|");
  for (let k = shown.length - 1; k >= 0; k--) {
    const item = shown[k];
    if (ordered && wanted.includes(key(item.spec))) continue;
    if (item.handle) await counted(owner.remove(item.handle));
    shown.splice(k, 1);
    await release(item);
  }
  for (const [k, spec] of next.entries()) {
    const item = shown[k];
    if (item && key(item.spec) === wanted[k]) {
      await refresh(item, spec);
      continue;
    }
    const made = await make(spec);
    await counted(owner.insert(made.handle, offset + k));
    shown.splice(k, 0, made.item);
  }
}

let timer: number | null = null;
let running = false;
let again = false;

async function sync(): Promise<void> {
  if (!bar) return;
  if (running) {
    again = true;
    return;
  }
  running = true;
  try {
    do {
      again = false;
      await update(bar.menu, 1, bar.items, menus());
    } while (again);
  } catch (e) {
    console.error("system menu bar update failed", e);
  } finally {
    running = false;
  }
}

/** Brings the menu bar up to date once the current event is handled. */
function schedule(): void {
  if (!bar || timer !== null) return;
  timer = window.setTimeout(() => {
    timer = null;
    void sync();
  });
}

/** Brings the installed menu bar up to date (nothing before it is installed): for page state an
 *  item shows that changes without a view or input, such as 编辑面板布局…'s availability after a
 *  resize (main.ts render). */
export function refreshNativeMenu(): void {
  schedule();
}

/** macOS: puts the menus in the system menu bar in place of the start-up one (src-tauri
 *  install_macos_quit) and keeps them current. The application menu comes first: 关于 (帮助 >
 *  关于's), the system's items, and 退出. */
export async function installNativeMenu(): Promise<void> {
  const menu = await Menu.new({
    items: [{
      text: "LizzieYzy Tauri",
      items: [
        { text: product("macAbout"), action: () => act(COMMANDS["menu.about"].run) },
        { item: "Separator" },
        { item: "Services", text: product("macServices") },
        { item: "Separator" },
        { item: "Hide", text: product("macHide") },
        { item: "HideOthers", text: product("macHideOthers") },
        { item: "ShowAll", text: product("macShowAll") },
        { item: "Separator" },
        // A plain item rather than the system's Quit, which ends the program without closing the
        // window: 退出 (menu.exit) closes it, so the program saves and ends as from its close button.
        { text: product("macQuit"), accelerator: "CmdOrCtrl+Q", action: () => act(COMMANDS["menu.exit"].run) },
      ],
    }],
  });
  const items: Shown[] = [];
  await update(menu, 1, items, menus());
  const before = await menu.setAsAppMenu();
  await before?.close();
  bar = { menu, items };
  subscribe(schedule);
  // Page state the items show besides the view (the GTP console's check and the like) changes
  // with input.
  document.addEventListener("click", schedule, true);
  document.addEventListener("keyup", schedule, true);
  window.addEventListener("focus", schedule);
  schedule();
}

type Reported = "-" | { text: string; kind: Spec["kind"]; checked: boolean; enabled: boolean; items?: Reported[] };

function report(list: Shown[]): Reported[] {
  return list.map(({ spec, items }) => spec.kind === "sep" ? "-" : { text: spec.text, kind: spec.kind, checked: spec.checked, enabled: spec.enabled, items: spec.kind === "submenu" ? report(items) : undefined });
}

/** The system menu bar's menus as the model has them now, on every system (acceptance probe). */
export function menuModelReport(): Reported[] {
  return report(menus().map(track));
}

/** What the system menu bar shows (macOS, once installed) and how many calls into it that took. */
export function nativeMenuReport(): { menus: Reported[]; calls: number } | null {
  return bar && { menus: report(bar.items), calls };
}
