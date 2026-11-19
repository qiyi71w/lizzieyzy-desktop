// 存档与读档 (Java LizzieFrame.showTempGamePanel, addTempGameOne, addTempGameNew, showBigBoardImage):
// a panel over the workbench between the toolbars and the bottom toolbar: a strip with the options,
// 全部删除 and 关闭, then a grid of equal slot cells that fills the panel's width, each a picture with
// its name, time and actions, the last one adding a slot. Every change comes back as a new
// `recovery` projection and the panel is rebuilt from it, like Java rebuilding the panel after each
// action.
import { registerViewHandler, run, saveFileName, state } from "../app";
import { recoveryThumbnail } from "../contract/client";
import type { RecoveryCommand, SlotRef, SlotView } from "../contract/generated";
import { chooseSave, confirm } from "../dialogs";
import { product, t } from "../i18n";
import { slotThumbnail } from "./capture";
import "./recovery.css";

// The zoomed picture's size and the rest before it shows.
const ZOOM = 600;
const ZOOM_DELAY = 800;

let root: HTMLElement | null = null;
let builtFor = "";
/** Names typed but not yet used, kept across rebuilds caused by other changes (the periodic
 *  automatic save); any panel action clears them, as Java rebuilds every field. */
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

function refresh(): void {
  const v = state.view;
  if (!root || !v) return;
  const r = v.recovery;
  if (pictures.revision !== r.revision) void loadPictures(r.revision, [...(r.auto ? [r.auto] : []), ...r.slots]);
  const key = `${r.revision}:${r.settings.autoSaveOnExit}:${r.settings.resumePreviousGame}:${r.settings.zoomThumbnails}`;
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

  const scroll = document.createElement("div");
  scroll.className = "rc-scroll";
  const grid = document.createElement("div");
  grid.className = "rc-grid";
  scroll.append(grid);

  const cards = [...(recovery.auto ? [recovery.auto] : []), ...recovery.slots];
  for (const slot of cards) grid.append(slotCell(slot));
  const last = cards.at(-1);
  grid.append(newCell(!last || last.slot.type === "Auto" ? 1 : last.slot.index + 1));
  grid.addEventListener("mousemove", pointerMoved);

  const previousScroll = host.querySelector(".rc-scroll")?.scrollTop ?? 0;
  host.replaceChildren(topBar(), scroll);
  scroll.scrollTop = previousScroll;
  if (focused) host.querySelector<HTMLElement>(`[data-key="${focused}"]`)?.focus();
}

/** tempGamePanelTop: the three options, 全部删除 and 关闭 at the strip's right end. */
function topBar(): HTMLElement {
  const settings = state.view!.recovery.settings;
  const bar = document.createElement("div");
  bar.className = "rc-top";
  const boxes: HTMLInputElement[] = [];
  const check = (text: string, key: string, checked: boolean) => {
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
    bar.append(el);
  };
  check(t("LizzieFrame.saveAndLoad.chkZoomImage"), "recovery.chk.zoom", settings.zoomThumbnails);
  check(t("LizzieFrame.saveAndLoad.chkAutoSaveOnExit"), "recovery.chk.autosave", settings.autoSaveOnExit);
  check(t("LizzieFrame.saveAndLoad.chkAutoResume"), "recovery.chk.resume", settings.resumePreviousGame);
  const deleteAll = button(t("LizzieFrame.saveAndLoad.btnDeleteAll"), "recovery.deleteAll", async () => {
    if (await confirm(t("LizzieFrame.warning"), t("LizzieFrame.saveAndLoad.deleteAllWarining"))) await send({ type: "DeleteAllSlots" });
  });
  deleteAll.classList.add("danger");
  const actions = document.createElement("span");
  actions.className = "rc-actions";
  actions.append(deleteAll, button(t("LizzieFrame.saveAndLoad.close"), "recovery.close", closeRecovery));
  bar.append(actions);
  return bar;
}

function row(...children: HTMLElement[]): HTMLElement {
  const el = document.createElement("div");
  el.className = "rc-row";
  el.append(...children);
  return el;
}

/** addTempGameOne: the picture (a click loads the slot, as does 读档), the name with 重命名, then
 *  the time with 读档 / 存档 / 删除. */
function slotCell(s: SlotView): HTMLElement {
  const auto = s.slot.type === "Auto";
  const index = s.slot.type === "Manual" ? s.slot.index : 1;
  const key = slotKey(s.slot);
  // The picture scaled to the cell; without a picture it is empty but still loads.
  const picture = document.createElement("div");
  picture.className = "rc-picture";
  picture.dataset.key = `${key}.image`;
  const url = pictures.urls.get(key);
  if (url) picture.style.backgroundImage = `url(${url})`;
  picture.onclick = () => load(s.slot);
  const name = nameField(`${key}.name`, auto ? t("LizzieFrame.saveAndLoad.exitRecord") : s.name, !auto);
  const time = label(s.time);
  time.className = "rc-time";
  const actions = document.createElement("span");
  actions.className = "rc-actions";
  const del = button(t("LizzieFrame.saveAndLoad.del"), `${key}.del`, () => void send({ type: "DeleteSlot", slot: s.slot }));
  del.classList.add("danger");
  actions.append(
    button(t("LizzieFrame.saveAndLoad.load"), `${key}.load`, () => load(s.slot)),
    button(t("LizzieFrame.saveAndLoad.save"), `${key}.save`, async () => {
      if (s.slot.type !== "Manual") return;
      // JOptionPane YES_NO_OPTION: 存档已存在,是否覆盖?
      if (!(await chooseSave(t("LizzieFrame.warning"), t("LizzieFrame.recordExists"), product("yes"), product("no")))) return;
      await send({ type: "OverwriteSlot", index: s.slot.index, name: saveFileName(), thumbnail: await slotThumbnail() });
    }, !auto),
    del,
  );
  const cell = document.createElement("div");
  cell.className = "rc-slot";
  cell.append(
    picture,
    row(
      label(auto ? t("LizzieFrame.saveAndLoad.autoRec") : `${t("LizzieFrame.saveAndLoad.rec")}${index}`),
      name,
      button(t("LizzieFrame.saveAndLoad.reName"), `${key}.rename`, () => {
        if (s.slot.type === "Manual") void send({ type: "RenameSlot", index: s.slot.index, name: name.value });
      }, !auto),
    ),
    row(time, actions),
  );
  return cell;
}

/** addTempGameNew: the picture-sized button and the small one both add a slot after the last. */
function newCell(index: number): HTMLElement {
  const name = nameField("recovery.new.name", "");
  const create = async () => {
    await send({ type: "NewSlot", name: name.value.length > 0 ? name.value : saveFileName(), thumbnail: await slotThumbnail() });
  };
  const big = button(t("LizzieFrame.saveAndLoad.newRecord"), "recovery.new.image", create);
  big.classList.add("rc-new");
  const small = button(t("LizzieFrame.saveAndLoad.newRecord"), "recovery.new", create);
  small.classList.add("primary");
  const cell = document.createElement("div");
  cell.className = "rc-slot";
  cell.append(big, row(label(`${t("LizzieFrame.saveAndLoad.rec")}${index}`), name, small));
  return cell;
}

/** tempGamePanelLis: with 鼠标悬停放大缩略棋盘 on, resting 800 ms on a picture shows it at 600 x 600 at
 *  the pointer; moving onto another picture or off the pictures hides it. */
function pointerMoved(e: MouseEvent): void {
  if (!state.view?.recovery.settings.zoomThumbnails) return;
  const x = e.clientX;
  const y = e.clientY;
  if (hover && hover.x === x && hover.y === y) return;
  const picture = (e.target as Element).closest<HTMLElement>(".rc-picture");
  if (!picture) {
    if (hover) clearTimeout(hover.timer);
    hover = null;
    hideZoom();
    return;
  }
  const key = picture.dataset.key!.replace(/\.image$/, "");
  if (zoom && zoom.key !== key) hideZoom();
  if (hover) clearTimeout(hover.timer);
  const timer = zoom ? 0 : window.setTimeout(() => showZoom(key, x, y), ZOOM_DELAY);
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
