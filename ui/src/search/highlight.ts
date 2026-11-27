// The gold line Java draws round a located control for 2.4 s (FunctionSearchController.highlight,
// Color(185, 156, 93); ConfigDialog2 SETTING_HIGHLIGHT_MILLIS). One control at a time.
import "./search.css";

let current: { element: HTMLElement; timer: number; ended: (() => void) | undefined } | null = null;

/** `ended` runs once the line is gone again (its time is up or another control took it). */
export function highlight(element: HTMLElement, ended?: () => void): void {
  clearHighlight();
  element.classList.add("located");
  current = { element, timer: window.setTimeout(clearHighlight, 2400), ended };
}

export function clearHighlight(): void {
  if (!current) return;
  const { element, timer, ended } = current;
  clearTimeout(timer);
  element.classList.remove("located");
  current = null;
  ended?.();
}

/** A setting function search reaches inside a dialog or a window: `control` comes into view with
 *  the focus, the gold line round `around` (the control with its name). Nothing is changed. */
export function locateControl(control: HTMLElement, around: HTMLElement = control, ended?: () => void): void {
  around.scrollIntoView({ block: "nearest", inline: "nearest" });
  control.focus({ preventScroll: true });
  highlight(around, ended);
}
