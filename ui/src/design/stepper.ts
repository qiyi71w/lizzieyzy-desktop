// The number field with up / down arrows beside it, one control in every window: the toolbar's
// 贴目, 激进度, 广度 and limits, and the dialogs' number fields (base.css draws it).
import { icon } from "./icons";

interface StepperOptions {
  /** Writes a stepped number into the field; plain `String` by default. */
  format?: (value: number) => string;
  /** The tooltips of the up and the down arrow. */
  titles?: [string, string];
}

/** Puts `input` in a stepper and returns the stepper, which takes the field's place. The arrows,
 *  ↑ / ↓ and, while the field has the focus, the wheel change the field's number by its `step`
 *  attribute (1 without one) within its `min` and `max`, then fire `input` and `change` as typing
 *  and leaving the field do, so the field's own listeners apply the value. A field that is empty,
 *  holds no number, or is disabled or read-only is not stepped. */
export function stepper(input: HTMLInputElement, options: StepperOptions = {}): HTMLElement {
  const { format = String, titles } = options;
  const box = document.createElement("span");
  box.className = "ds-stepper";
  const arrows = document.createElement("span");
  arrows.className = "ds-stepper-arrows";
  arrows.setAttribute("aria-hidden", "true");
  const move = (direction: 1 | -1) => {
    if (input.disabled || input.readOnly || input.value.trim() === "") return;
    const current = Number(input.value);
    if (!Number.isFinite(current)) return;
    const step = Number(input.getAttribute("step")) || 1;
    const min = input.getAttribute("min") ? Number(input.getAttribute("min")) : -Infinity;
    const max = input.getAttribute("max") ? Number(input.getAttribute("max")) : Infinity;
    // Rounded to a millionth, so tenths do not gather binary noise ("0.30000000000000004").
    const next = Math.min(max, Math.max(min, Math.round((current + direction * step) * 1e6) / 1e6));
    if (next === current) return;
    input.value = format(next);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  };
  for (const direction of [1, -1] as const) {
    const arrow = document.createElement("span");
    arrow.append(icon(direction > 0 ? "chevron-up" : "chevron-down"));
    if (titles) arrow.title = titles[direction > 0 ? 0 : 1];
    // The focus stays where it was, as with the field's own arrows.
    arrow.addEventListener("mousedown", (event) => event.preventDefault());
    arrow.addEventListener("click", () => move(direction));
    arrows.append(arrow);
  }
  input.addEventListener("keydown", (event) => {
    if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
    event.preventDefault();
    move(event.key === "ArrowUp" ? 1 : -1);
  });
  input.addEventListener(
    "wheel",
    (event) => {
      if (document.activeElement !== input || event.deltaY === 0) return;
      event.preventDefault();
      move(event.deltaY < 0 ? 1 : -1);
    },
    { passive: false },
  );
  box.append(input, arrows);
  return box;
}
