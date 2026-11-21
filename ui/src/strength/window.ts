// A modal JDialog of the 棋力评估 feature on the shared dialog frame: the title with its close
// box, Esc closing it (AccessibilitySupport.installEscapeAction) and the keyboard kept inside
// the dialog.
import { closeBox, openModalLayer } from "../dialogs";

export interface StrengthWindow {
  box: HTMLDivElement;
  body: HTMLDivElement;
  close(): void;
}

/** `onClose` runs once, for the close button, Esc and `close()` alike. */
export function openWindow(title: string, cls: string, onClose: () => void): StrengthWindow {
  const previous = document.activeElement as HTMLElement | null;
  const overlay = document.createElement("div");
  overlay.className = "modal";
  const box = document.createElement("div");
  box.className = `modal-box ps-window ${cls}`;
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", title);
  box.tabIndex = -1;
  const head = document.createElement("div");
  head.className = "modal-title";
  const caption = document.createElement("span");
  caption.textContent = title;
  head.append(caption, closeBox(() => close()));
  const body = document.createElement("div");
  body.className = "ps-window-body";
  box.append(head, body);
  overlay.append(box);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    closeLayer();
    // The control that opened this dialog may have gone with a rebuilt dashboard: the owner
    // dialog below (the newest remaining modal) takes the focus then, like a Swing owner window.
    if (previous?.isConnected) previous.focus();
    else ownerFocus();
    onClose();
  };
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      close();
    } else if (e.key === "Tab") {
      const items = [...box.querySelectorAll<HTMLElement>("button:not(:disabled):not([tabindex='-1']), input:not(:disabled), [tabindex='0']")]
        .filter((el) => el.getClientRects().length > 0);
      const at = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length]?.focus();
    }
  });
  const closeLayer = openModalLayer(overlay);
  box.focus();
  return { box, body, close };
}

/** Focuses the newest open modal dialog, so its Esc binding has the keyboard. */
export function ownerFocus(): void {
  const boxes = document.querySelectorAll<HTMLElement>("#dialog .modal-box");
  boxes[boxes.length - 1]?.focus();
}
