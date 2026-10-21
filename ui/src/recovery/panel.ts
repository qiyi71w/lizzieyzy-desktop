// 存档与读档 (Java LizzieFrame.showTempGamePanel, addTempGameOne, addTempGameNew, showBigBoardImage):
// a grey panel over the workbench between the toolbars and the bottom toolbar, laid out with
// Java's absolute geometry: a 20 px bar, then 300 px pictures in 310 x 345 cells. Every change
// comes back as a new `recovery` projection and the panel is rebuilt from it, like Java rebuilding
// the panel after each action.
import { registerViewHandler, run, saveFileName, state } from "../app";
import { recoveryThumbnail } from "../contract/client";
import type { RecoveryCommand, SlotRef, SlotView } from "../contract/generated";
import { chooseSave, confirm } from "../dialogs";
import { product, t } from "../i18n";
import { captureMainBoard } from "./capture";
import "./recovery.css";

const CELL_W = 310;
const CELL_H = 345;
const PICTURE = 300;
const ZOOM = 600;
const ZOOM_DELAY = 800;

let root: HTMLElement | null = null;
let builtFor = "";
/** Names typed but not yet used, kept across rebuilds caused by other changes (the periodic
 *  automatic save, a resize); any panel action clears them, as Java rebuilds every field. */
const drafts = new Map<string, string>();
const pictures = { revision: -1, urls: new Map<string, string>() };
let zoom: { el: HTMLElement; key: string } | null = null;
let hover: { key: string; x: number; y: number; timer: number } | null = null;

const slotKey = (slot: SlotRef) => (slot.type === "Auto" ? "recovery.auto" : `recovery.slot.${slot.index}`);

export function recoveryOpen(): boolean {
  return root !== null;
}

/** What the acceptance probe reports besides the panel's controls. */
export function recoveryReport() {
  return { open: root !== null, zoom: zoom?.key ?? null };
}

/** File > 存档与读档 and the toolbar's 存档 button; while open it is shown again from scratch. */
export function openRecovery(): void {
  if (!state.view) return;
  if (!root) {
    root = document.createElement("div");
    root.id = "recovery";
    document.getElementById("main")!.append(root);
  }
  drafts.clear();
  builtFor = "";
  refresh();
}

/** hideTempGamePanel: the workbench panels show again and the board takes the keyboard. */
export function closeRecovery(): void {
  hideZoom();
  if (hover) clearTimeout(hover.timer);
  hover = null;
  root?.remove();
  root = null;
  drafts.clear();
  (document.activeElement as HTMLElement | null)?.blur?.();
}

registerViewHandler(() => refresh());
window.addEventListener("resize", () => refresh());

function refresh(): void {
  const v = state.view;
  if (!root || !v) return;
  const r = v.recovery;
  if (pictures.revision !== r.revision) void loadPictures(r.revision, [...(r.auto ? [r.auto] : []), ...r.slots]);
  const key = `${r.revision}:${r.settings.autoSaveOnExit}:${r.settings.resumePreviousGame}:${r.settings.zoomThumbnails}:${root.clientWidth}:${root.clientHeight}`;
  if (key === builtFor) return;
  builtFor = key;
  build();
}

/** Pictures of the current revision as object URLs; older ones are released. */
async function loadPictures(revision: number, slots: SlotView[]): Promise<void> {
  for (const url of pictures.urls.values()) URL.revokeObjectURL(url);
  pictures.urls.clear();
  pictures.revision = revision;
  await Promise.all(slots.filter((s) => s.has_thumbnail).map(async (s) => {
    try {
      const bytes = await recoveryThumbnail(s.slot);
      if (pictures.revision !== revision || !bytes.byteLength) return;
      const key = slotKey(s.slot);
      const url = URL.createObjectURL(new Blob([bytes]));
      pictures.urls.set(key, url);
      const picture = root?.querySelector<HTMLElement>(`[data-key="${key}.image"]`);
      if (picture) picture.style.backgroundImage = `url(${url})`;
    } catch (e) {
      console.error("slot picture unavailable", e);
    }
  }));
}

function at(el: HTMLElement, x: number, y: number, w: number, h: number): HTMLElement {
  el.style.left = `${x}px`;
  el.style.top = `${y}px`;
  el.style.width = `${w}px`;
  el.style.height = `${h}px`;
  return el;
}

function label(text: string): HTMLElement {
  const el = document.createElement("span");
  el.className = "rc-label";
  el.textContent = text;
  return el;
}

function button(text: string, key: string, action: () => void, enabled = true): HTMLButtonElement {
  const el = document.createElement("button");
  el.className = "rc-button";
  el.textContent = text;
  el.dataset.key = key;
  el.disabled = !enabled;
  el.onclick = () => {
    hideZoom();
    action();
  };
  return el;
}

function nameField(key: string, value: string, enabled = true): HTMLInputElement {
  const el = document.createElement("input");
  el.className = "rc-name";
  el.dataset.key = key;
  el.value = enabled ? drafts.get(key) ?? value : value;
  el.disabled = !enabled;
  el.oninput = () => drafts.set(key, el.value);
  return el;
}

async function send(op: RecoveryCommand): Promise<void> {
  drafts.clear();
  await run({ type: "Recovery", op });
}

/** Load button or a click on the picture: open the slot's record, then hide the panel. */
function load(slot: SlotRef): void {
  void send({ type: "LoadSlot", slot });
  closeRecovery();
}

function build(): void {
  const v = state.view!;
  const host = root!;
  const recovery = v.recovery;
  const focused = (document.activeElement as HTMLElement | null)?.dataset?.key;
  hideZoom();
  const width = Math.max(0, host.clientWidth - 12);
  const columns = Math.max(1, Math.floor(width / CELL_W));

  const scroll = document.createElement("div");
  scroll.className = "rc-scroll";
  const content = document.createElement("div");
  content.className = "rc-content";
  scroll.append(content);

  const cards = [...(recovery.auto ? [recovery.auto] : []), ...recovery.slots];
  cards.forEach((slot, i) => addSlot(content, slot, (i % columns) * CELL_W, Math.floor(i / columns) * CELL_H + 20));
  const last = cards.at(-1);
  const newIndex = !last || last.slot.type === "Auto" ? 1 : last.slot.index + 1;
  const newX = (cards.length % columns) * CELL_W;
  const newY = Math.floor(cards.length / columns) * CELL_H + 20;
  addNew(content, newIndex, newX, newY);
  content.style.width = `${width}px`;
  content.style.height = `${Math.max(host.clientHeight - 5, newY + CELL_H)}px`;
  content.addEventListener("mousemove", (e) => pointerMoved(e, content, cards, columns));

  const previousScroll = host.querySelector(".rc-scroll")?.scrollTop ?? 0;
  host.replaceChildren(scroll, topBar(width));
  scroll.scrollTop = previousScroll;
  if (focused) host.querySelector<HTMLElement>(`[data-key="${focused}"]`)?.focus();
}

/** tempGamePanelTop: check boxes, 全部删除 and 关闭 placed from the right edge. */
function topBar(width: number): HTMLElement {
  const settings = state.view!.recovery.settings;
  const bar = document.createElement("div");
  bar.className = "rc-top";
  const boxes: HTMLInputElement[] = [];
  const check = (text: string, key: string, checked: boolean, x: number, w: number) => {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.checked = checked;
    box.dataset.key = key;
    box.onchange = () => {
      const [zoomBox, autoSave, resume] = boxes;
      void run({ type: "Recovery", op: { type: "SetSettings", settings: { autoSaveOnExit: autoSave.checked, resumePreviousGame: resume.checked, zoomThumbnails: zoomBox.checked } } });
    };
    boxes.push(box);
    const el = document.createElement("label");
    el.className = "rc-check";
    el.append(box, text);
    bar.append(at(el, x, 0, w, 20));
  };
  const start = Math.max(0, width - 445);
  check(t("LizzieFrame.saveAndLoad.chkZoomImage"), "recovery.chk.zoom", settings.zoomThumbnails, start, 150);
  check(t("LizzieFrame.saveAndLoad.chkAutoSaveOnExit"), "recovery.chk.autosave", settings.autoSaveOnExit, start + 150, 80);
  check(t("LizzieFrame.saveAndLoad.chkAutoResume"), "recovery.chk.resume", settings.resumePreviousGame, start + 230, 110);
  bar.append(
    at(button(t("LizzieFrame.saveAndLoad.btnDeleteAll"), "recovery.deleteAll", async () => {
      if (await confirm(t("LizzieFrame.warning"), t("LizzieFrame.saveAndLoad.deleteAllWarining"))) await send({ type: "DeleteAllSlots" });
    }), start + 343, 0, 60, 19),
    at(button(t("LizzieFrame.saveAndLoad.close"), "recovery.close", closeRecovery), Math.min(start + 403, width - 40), 0, 40, 19),
  );
  return bar;
}

/** addTempGameOne. */
function addSlot(content: HTMLElement, s: SlotView, x: number, y: number): void {
  const auto = s.slot.type === "Auto";
  const index = s.slot.type === "Manual" ? s.slot.index : 1;
  const key = slotKey(s.slot);
  // A JLabel with the picture zoomed to 300 x 300; without a picture it is empty but still loads.
  const picture = document.createElement("div");
  picture.className = "rc-picture";
  picture.dataset.key = `${key}.image`;
  const url = pictures.urls.get(key);
  if (url) picture.style.backgroundImage = `url(${url})`;
  picture.onclick = () => load(s.slot);
  const name = nameField(`${key}.name`, auto ? t("LizzieFrame.saveAndLoad.exitRecord") : s.name, !auto);
  content.append(
    at(picture, x, y, PICTURE, PICTURE),
    at(label(auto ? t("LizzieFrame.saveAndLoad.autoRec") : `${t("LizzieFrame.saveAndLoad.rec")}${index}`), x + 5, y + 300, 65, 20),
    at(name, x + (auto ? 70 : 45), y + 300, auto ? 169 : 194, 20),
    at(button(t("LizzieFrame.saveAndLoad.reName"), `${key}.rename`, () => {
      if (s.slot.type === "Manual") void send({ type: "RenameSlot", index: s.slot.index, name: name.value });
    }, !auto), x + 240, y + 300, 60, 20),
    at(label(s.time), x + 5, y + 320, 200, 20),
    at(button(t("LizzieFrame.saveAndLoad.load"), `${key}.load`, () => load(s.slot)), x + 150, y + 320, 50, 20),
    at(button(t("LizzieFrame.saveAndLoad.save"), `${key}.save`, async () => {
      if (s.slot.type !== "Manual") return;
      // JOptionPane YES_NO_OPTION: 存档已存在,是否覆盖?
      if (!(await chooseSave(t("LizzieFrame.warning"), t("LizzieFrame.recordExists"), product("yes"), product("no")))) return;
      const thumbnail = await captureMainBoard();
      if (thumbnail) await send({ type: "OverwriteSlot", index: s.slot.index, name: saveFileName(), thumbnail });
    }, !auto), x + 200, y + 320, 50, 20),
    at(button(t("LizzieFrame.saveAndLoad.del"), `${key}.del`, () => void send({ type: "DeleteSlot", slot: s.slot })), x + 250, y + 320, 50, 20),
  );
}

/** addTempGameNew: the picture-sized button and the small one both add a slot after the last. */
function addNew(content: HTMLElement, index: number, x: number, y: number): void {
  const name = nameField("recovery.new.name", "");
  const create = async () => {
    const thumbnail = await captureMainBoard();
    if (thumbnail) await send({ type: "NewSlot", name: name.value.length > 0 ? name.value : saveFileName(), thumbnail });
  };
  const big = button(t("LizzieFrame.saveAndLoad.newRecord"), "recovery.new.image", create);
  big.classList.add("rc-new");
  content.append(
    at(big, x, y, PICTURE, PICTURE),
    at(label(`${t("LizzieFrame.saveAndLoad.rec")}${index}`), x + 5, y + 300, 65, 20),
    at(name, x + 45, y + 300, 174, 20),
    at(button(t("LizzieFrame.saveAndLoad.newRecord"), "recovery.new", create), x + 220, y + 300, 80, 20),
  );
}

/** tempGamePanelLis: with 鼠标悬停放大缩略棋盘 on, resting 800 ms on a picture shows it at 600 x 600 at
 *  the pointer; moving onto another picture or off the pictures hides it. */
function pointerMoved(e: MouseEvent, content: HTMLElement, cards: SlotView[], columns: number): void {
  if (!state.view?.recovery.settings.zoomThumbnails) return;
  const box = content.getBoundingClientRect();
  const x = e.clientX - box.left;
  const y = e.clientY - box.top;
  if (hover && hover.x === x && hover.y === y) return;
  const i = cards.findIndex((_, k) => {
    const cx = (k % columns) * CELL_W, cy = Math.floor(k / columns) * CELL_H + 20;
    return cx < x && x < cx + PICTURE && cy < y && y < cy + PICTURE;
  });
  if (i < 0) {
    if (hover) clearTimeout(hover.timer);
    hover = null;
    hideZoom();
    return;
  }
  const key = slotKey(cards[i].slot);
  if (zoom && zoom.key !== key) hideZoom();
  if (hover) clearTimeout(hover.timer);
  const timer = zoom ? 0 : window.setTimeout(() => showZoom(key, e.clientX, e.clientY), ZOOM_DELAY);
  hover = { key, x, y, timer };
}

/** showBigBoardImage: the slot's picture in a 600 x 600 popup with its corner at the pointer, kept
 *  on screen like JPopupMenu.show. */
function showZoom(key: string, clientX: number, clientY: number): void {
  const url = pictures.urls.get(key);
  if (!root || !url) return;
  hideZoom();
  const el = document.createElement("div");
  el.className = "rc-zoom";
  el.dataset.key = "recovery.zoom";
  const image = document.createElement("img");
  image.src = url;
  image.alt = "";
  image.draggable = false;
  el.append(image);
  el.style.left = `${Math.max(0, Math.min(clientX, window.innerWidth - ZOOM))}px`;
  el.style.top = `${Math.max(0, Math.min(clientY, window.innerHeight - ZOOM))}px`;
  root.append(el);
  zoom = { el, key };
}

function hideZoom(): void {
  zoom?.el.remove();
  zoom = null;
}
