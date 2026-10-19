// In-window modal dialogs in the role of Java JOptionPane message / confirm dialogs. While one is
// open, workbench shortcuts are inactive (focus stays inside the dialog) like a Swing modal dialog.
import { product } from "./i18n";

const host = () => document.getElementById("dialog")!;
let open = 0;

export function dialogOpen(): boolean {
  return open > 0;
}

interface Button {
  label: string;
  value: boolean;
  primary?: boolean;
}

function show(title: string, body: HTMLElement | string, buttons: Button[], kind: "info" | "warning" | "error"): Promise<boolean> {
  const { promise, resolve } = Promise.withResolvers<boolean>();
  const previous = document.activeElement as HTMLElement | null;
  const overlay = document.createElement("div");
  overlay.className = "modal";
  const box = document.createElement("div");
  box.className = `modal-box ${kind}`;
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
  const close = (value: boolean) => {
    overlay.remove();
    open--;
    host().hidden = open === 0;
    previous?.focus?.();
    resolve(value);
  };
  for (const b of buttons) {
    const el = document.createElement("button");
    el.textContent = b.label;
    if (b.primary) el.className = "primary";
    el.onclick = () => close(b.value);
    row.append(el);
  }
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") close(false);
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) close(buttons.find((b) => b.primary)?.value ?? true);
  });
  box.append(head, content, row);
  overlay.append(box);
  host().append(overlay);
  host().hidden = false;
  open++;
  (row.querySelector("button.primary") as HTMLElement | null)?.focus();
  return promise;
}

/** JOptionPane.showMessageDialog. */
export function message(title: string, text: string, kind: "info" | "warning" | "error" = "info"): Promise<void> {
  return show(title, text, [{ label: product("ok"), value: true, primary: true }], kind).then(() => undefined);
}

/** JOptionPane.showConfirmDialog with OK_CANCEL_OPTION. */
export function confirm(title: string, text: string, kind: "info" | "warning" = "warning"): Promise<boolean> {
  return show(title, text, [
    { label: product("ok"), value: true, primary: true },
    { label: product("cancel"), value: false },
  ], kind);
}

/** A dialog with custom content and a close button (diagnostics). */
export function panel(title: string, body: HTMLElement): Promise<void> {
  return show(title, body, [{ label: product("close"), value: true, primary: true }], "info").then(() => undefined);
}
