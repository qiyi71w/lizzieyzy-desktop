// In-window modal dialogs in the role of Java JOptionPane message / confirm dialogs. While one is
// open the rest of the window is inert and Tab cycles inside the dialog, so neither the mouse nor
// the keyboard reaches the workbench, like a Swing modal dialog.
import { product, t } from "./i18n";
import { icon } from "./design/icons";
import { cancelFirst } from "./design/theme";
import "./design/dialog.css";

const host = () => document.getElementById("dialog")!;
let open = 0;

/** A modal dialog or layer is open. `except` leaves out one layer (openModalLayer), so a modal
 *  asking about the others does not count itself. */
export function dialogOpen(except?: HTMLElement): boolean {
  return open - (except && except.parentElement === host() ? 1 : 0) > 0;
}

let notices = 0;

/** Utils.showMsgNoModalForTime: the Message window, not modal, closed after `seconds`; each
 *  further one is offset so they stay readable. */
export function timedMessage(text: string, seconds: number): void {
  const box = document.createElement("div");
  box.className = "modal-box timed-notice";
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", t("Message.title"));
  box.dataset.freeWindow = "";
  box.dataset.name = "timed.notice";
  box.style.translate = `${(notices % 5) * 16}px ${(notices % 5) * 16}px`;
  notices++;
  const title = document.createElement("div");
  title.className = "modal-title";
  title.textContent = t("Message.title");
  const body = document.createElement("div");
  body.className = "modal-body";
  body.textContent = text;
  box.append(title, body);
  document.body.append(box);
  window.setTimeout(() => box.remove(), seconds * 1000);
}

/** A custom modal layer (the function search) under the same modal state as these dialogs: it
 *  joins the dialog host, only the newest layer or dialog takes input and everything else is
 *  inert. A dialog opened over it gets the input until it closes. Returns the layer's close. */
export function openModalLayer(layer: HTMLElement): () => void {
  host().append(layer);
  host().hidden = false;
  open++;
  setModal();
  let closed = false;
  return () => {
    if (closed) return;
    closed = true;
    layer.remove();
    open--;
    host().hidden = open === 0;
    setModal();
  };
}

/** A non-modal window that disables the main window while it is shown (WaitForAnalysis
 *  disableOwnerFrame): the workbench is inert, while windows marked `data-free-window` (the GTP
 *  console, that window itself) stay usable. Modal dialogs still block everything. */
let ownerDisabled = false;

export function setOwnerDisabled(disabled: boolean): void {
  ownerDisabled = disabled;
  setModal();
}

export function ownerIsDisabled(): boolean {
  return ownerDisabled;
}

interface Button<T = boolean> {
  label: string;
  value: T;
  primary?: boolean;
  /** Automation key (`data-key`). */
  key?: string;
}

/** A dialog's scrim and box. The box takes the focus a press on its own text gives, and a press on
 *  the scrim leaves the focus where it is, so Escape and Tab always reach the open dialog. */
function modalFrame(className: string): { overlay: HTMLDivElement; box: HTMLDivElement } {
  const overlay = document.createElement("div");
  overlay.className = "modal";
  overlay.addEventListener("mousedown", (e) => { if (e.target === overlay) e.preventDefault(); });
  const box = document.createElement("div");
  box.className = className;
  box.tabIndex = -1;
  overlay.append(box);
  return { overlay, box };
}

/** Tab and Shift+Tab move among `items`; from the box itself they go to the first or the last. */
function cycleFocus(items: HTMLElement[], back: boolean) {
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next = at < 0 ? items.at(back ? -1 : 0) : items[(at + (back ? -1 : 1) + items.length) % items.length];
  next?.focus();
}

/** `dismissed` is the answer of Escape (JOptionPane.CLOSED_OPTION's role); Enter outside a button
 *  chooses the primary button, else the first. `buttons` come in Java's order, the affirmative one
 *  first; where the platform puts cancel first (DESIGN.md Buttons) the primary button moves to the
 *  end with the one Escape answers before it. */
function show<T>(title: string, body: HTMLElement | string, buttons: Button<T>[], kind: "info" | "warning" | "error", dismissed: T): Promise<T> {
  const { promise, resolve } = Promise.withResolvers<T>();
  const previous = document.activeElement as HTMLElement | null;
  const { overlay, box } = modalFrame(`modal-box ${kind}`);
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", title);
  const head = document.createElement("div");
  head.className = "modal-title";
  head.textContent = title;
  const content = document.createElement("div");
  content.className = "modal-body";
  if (typeof body === "string") content.textContent = body;
  else content.append(body);
  const row = document.createElement("div");
  row.className = "modal-buttons";
  const close = (value: T) => {
    overlay.remove();
    open--;
    host().hidden = open === 0;
    setModal();
    previous?.focus?.();
    resolve(value);
  };
  const primary = buttons.find((b) => b.primary);
  const cancel = buttons.find((b) => !b.primary && b.value === dismissed);
  const ordered = cancelFirst ? [...buttons.filter((b) => b !== primary && b !== cancel), cancel, primary] : buttons;
  for (const b of ordered) {
    if (!b) continue;
    const el = document.createElement("button");
    el.textContent = b.label;
    if (b.primary) el.className = "primary";
    if (b.key) el.dataset.key = b.key;
    el.onclick = () => close(b.value);
    row.append(el);
  }
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") close(dismissed);
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) close((buttons.find((b) => b.primary) ?? buttons[0]).value);
    if (e.key === "Tab") {
      // Focus cycles through the dialog's own controls.
      e.preventDefault();
      cycleFocus([...box.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled)")], e.shiftKey);
    }
  });
  box.append(head, content, row);
  host().append(overlay);
  host().hidden = false;
  open++;
  setModal();
  (row.querySelector("button.primary") as HTMLElement | null)?.focus();
  return promise;
}

/** Only the newest dialog takes input; everything else in the window is inert. */
function setModal() {
  for (const el of document.body.children) {
    const free = (el as HTMLElement).hasAttribute("data-free-window");
    if (el !== host()) (el as HTMLElement).inert = open > 0 || (ownerDisabled && !free);
  }
  const overlays = [...host().children] as HTMLElement[];
  overlays.forEach((o, k) => (o.inert = k !== overlays.length - 1));
}

/** JOptionPane.showMessageDialog. */
export function message(title: string, text: string, kind: "info" | "warning" | "error" = "info"): Promise<void> {
  return show(title, text, [{ label: product("ok"), value: true, primary: true }], kind, false).then(() => undefined);
}

/** JOptionPane.showConfirmDialog with OK_CANCEL_OPTION. */
export function confirm(title: string, text: string, kind: "info" | "warning" = "warning"): Promise<boolean> {
  return show(title, text, [
    { label: product("ok"), value: true, primary: true },
    { label: product("cancel"), value: false },
  ], kind, false);
}

/** JOptionPane with custom content and two options. With `defaultCancel` the second option is
 *  Java's default option: it has the focus and Enter chooses it. */
export function ask(title: string, body: HTMLElement, ok: string, cancel: string, defaultCancel = false): Promise<boolean> {
  return show(title, body, [
    { label: ok, value: true, primary: !defaultCancel },
    { label: cancel, value: false, primary: defaultCancel },
  ], "warning", false);
}

/** JOptionPane.showOptionDialog: one button per option, `initial` the default option (it has the
 *  focus and Enter chooses it). Each option is its text and automation key. Resolves to the chosen
 *  index, or -1 when the dialog is dismissed. */
export function options(title: string, text: string, choices: [label: string, key: string][], initial: number, kind: "info" | "warning" = "warning"): Promise<number> {
  return show(title, text, choices.map(([label, key], value) => ({ label, key, value, primary: value === initial })), kind, -1);
}

/** A dialog with custom content and a close button (diagnostics). */
export function panel(title: string, body: HTMLElement): Promise<void> {
  return show(title, body, [{ label: product("close"), value: true, primary: true }], "info", false).then(() => undefined);
}

/** The close box that ends a dialog's title row and closes it as Escape does (DESIGN.md Dialogs).
 *  It is for the pointer: Escape is its key, so Tab passes over it. Automation key `dialog.close`
 *  unless the caller names its own. */
export function closeBox(close: () => void): HTMLButtonElement {
  const x = document.createElement("button");
  x.type = "button";
  x.className = "modal-close";
  x.tabIndex = -1;
  x.dataset.key = "dialog.close";
  x.setAttribute("aria-label", product("close"));
  x.title = product("close");
  x.append(icon("close"));
  x.onclick = () => close();
  return x;
}

/** Form dialogs own validation and close timing. Enter in a field the dialog does not handle
 *  itself presses the footer's main button (getRootPane().setDefaultButton), which validates as a
 *  click does; a search field, a button or a text area keeps its own Enter. The title row ends in
 *  the close box (`closeButton`). The title stays while the content scrolls, and the content's
 *  footer stays at its foot (design/dialog.css). */
export function formPanel(title: string, body: HTMLElement, beforeClose: () => Promise<boolean> = async () => true): { close: () => Promise<void>; closed: Promise<void>; closeButton: HTMLButtonElement } {
  const previous = document.activeElement as HTMLElement | null;
  const { overlay, box } = modalFrame("modal-box engine-dialog");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", title);
  const head = document.createElement("div");
  head.className = "modal-title";
  head.textContent = title;
  const done = Promise.withResolvers<void>();
  let closing = false;
  const close = async () => {
    if (closing) return;
    closing = true;
    try {
      if (!(await beforeClose())) return;
      overlay.remove();
      open--;
      host().hidden = open === 0;
      setModal();
      previous?.focus();
      done.resolve();
    } finally { closing = false; }
  };
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); void close(); }
    const field = e.target instanceof HTMLSelectElement
      || (e.target instanceof HTMLInputElement && !["search", "button", "submit", "reset", "file"].includes(e.target.type));
    if (e.key === "Enter" && field && !e.defaultPrevented && !e.isComposing) {
      const main = box.querySelector<HTMLButtonElement>(".ds-footer button.primary:not(:disabled)");
      if (main) { e.preventDefault(); main.click(); }
    }
    if (e.key === "Tab") {
      // Only rendered controls: 综合设置 keeps its other sections in the dialog, hidden. The close
      // box is for the pointer; Escape is its key.
      const items = [...box.querySelectorAll<HTMLElement>("button:not(:disabled), input:not(:disabled), textarea:not(:disabled), select:not(:disabled), [tabindex='0']")]
        .filter((el) => el !== x && el.getClientRects().length > 0);
      e.preventDefault();
      cycleFocus(items, e.shiftKey);
    }
  });
  const x = closeBox(() => void close());
  head.append(x);
  box.append(head, body);
  host().append(overlay);
  host().hidden = false;
  open++;
  setModal();
  (box.querySelector<HTMLElement>(".modal-title ~ * :is(input:not(:disabled), button:not(:disabled), select:not(:disabled))") ?? box).focus();
  return { close, closed: done.promise, closeButton: x };
}

export function chooseSave(title: string, text: string, save: string, discard: string): Promise<boolean> {
  return show(title, text, [{ label: save, value: true, primary: true }, { label: discard, value: false }], "warning", false);
}
