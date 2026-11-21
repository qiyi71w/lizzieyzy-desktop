// A non-modal in-window JDialog of the remote compute feature: the title row ending in the close
// box, Esc closing it (AccessibilitySupport.installEscapeToClose), the workbench left usable.
import { closeBox } from "../dialogs";

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

export function button(text: string, cls: string, key: string, onClick: () => void): HTMLButtonElement {
  const node = el("button", cls, text);
  node.type = "button";
  node.dataset.key = key;
  node.onclick = onClick;
  return node;
}

export interface CloudWindow {
  frame: HTMLElement;
  body: HTMLElement;
  close(): void;
}

/** `onClose` runs once, for the close button, Esc and `close()` alike. */
export function openCloudWindow(title: string, cls: string, key: string, onClose: () => void): CloudWindow {
  const previous = document.activeElement as HTMLElement | null;
  const frame = el("section", `cloud-window ${cls}`);
  frame.dataset.key = key;
  frame.tabIndex = -1;
  frame.setAttribute("role", "dialog");
  frame.setAttribute("aria-modal", "false");
  frame.setAttribute("aria-label", title);
  const head = el("div", "modal-title cloud-window-title");
  const x = closeBox(() => close());
  x.dataset.key = `${key}.close`;
  head.append(el("h2", "", title), x);
  const body = el("div", "cloud-window-body");
  frame.append(head, body);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    frame.remove();
    if (previous?.isConnected) previous.focus();
    onClose();
  };
  frame.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); close(); }
  });
  frame.addEventListener("pointerdown", (e) => {
    e.stopPropagation();
    // The newest-clicked window is on top, like an activated Swing window.
    if (frame.nextElementSibling) document.body.append(frame);
  });
  document.body.append(frame);
  frame.focus();
  return { frame, body, close };
}
