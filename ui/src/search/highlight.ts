// The gold line Java draws round a located control for 2.4 s (FunctionSearchController.highlight,
// Color(185, 156, 93); ConfigDialog2 SETTING_HIGHLIGHT_MILLIS). One control at a time.
import "./search.css";

let current: { element: HTMLElement; timer: number } | null = null;

export function highlight(element: HTMLElement): void {
  clearHighlight();
  element.classList.add("located");
  current = { element, timer: window.setTimeout(clearHighlight, 2400) };
}

export function clearHighlight(): void {
  if (!current) return;
  clearTimeout(current.timer);
  current.element.classList.remove("located");
  current = null;
}
