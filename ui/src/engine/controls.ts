// The rows and controls of a form dialog (dialogs.ts formPanel); design/dialog.css draws them.
// Dialogs are grouped lists (DESIGN.md Dialogs): a group title, then rows on one raised surface,
// each row its name (and a description under it) at the start and its controls at the end.
import { cancelFirst } from "../design/theme";
import { product } from "../i18n";
import "../design/dialog.css";
import "./engine.css";

export function button(key: string, action: () => void, enabled = true): HTMLButtonElement {
  const el = document.createElement("button");
  el.textContent = key;
  el.disabled = !enabled;
  if (!enabled) { el.title = product("unavailable"); el.classList.add("unavailable"); }
  el.onclick = action;
  return el;
}

/** The dialog's main action: its one filled accent button. */
export function primaryButton(key: string, action: () => void): HTMLButtonElement {
  const el = button(key, action);
  el.classList.add("primary");
  return el;
}

export function input(value: string | number, type = "text"): HTMLInputElement {
  const el = document.createElement("input");
  el.type = type;
  el.value = String(value);
  return el;
}

export function checkbox(key: string, checked: boolean): { label: HTMLLabelElement; input: HTMLInputElement } {
  const el = input("", "checkbox");
  el.checked = checked;
  const label = document.createElement("label");
  label.append(el, key);
  return { label, input: el };
}

/** A row of actions inside a dialog's content (a card's buttons, a console's prompt); a dialog's
 *  own buttons are its footer. */
export function actions(host: HTMLElement, ...items: (HTMLElement | string)[]): void {
  const row = document.createElement("div");
  row.className = "engine-actions";
  row.append(...items);
  host.append(row);
}

/** The buttons that end a dialog, by role (DESIGN.md Buttons). */
export interface FooterButtons {
  /** At the footer's start, apart from the buttons that end the dialog: a check box, 自动生成,
   *  更多设置, 恢复默认. */
  start?: (HTMLElement | string)[];
  /** The dialog's main action (primaryButton), the one Enter would choose. */
  primary?: HTMLElement;
  /** Further buttons that end the dialog another way (不保存, 中断). */
  others?: HTMLElement[];
  /** 取消 when the dialog holds a draft that 确定 commits; 关闭 when its changes already apply or
   *  it only shows something. */
  cancel?: HTMLElement;
}

/** The dialog's footer, its last child (it stays in view while the content scrolls): `start` at
 *  its start, the other buttons at its end in the platform's order — primary, others, cancel where
 *  the affirmative button comes first (Windows), others, cancel, primary where cancel comes first
 *  (macOS, GNOME). */
export function footer(host: HTMLElement, buttons: FooterButtons): HTMLElement {
  const row = document.createElement("div");
  row.className = "engine-actions ds-footer";
  const start = document.createElement("span");
  start.className = "ds-footer-start";
  start.append(...(buttons.start ?? []));
  const end = document.createElement("span");
  end.className = "ds-footer-end";
  const { primary, cancel, others = [] } = buttons;
  const ordered = cancelFirst ? [...others, cancel, primary] : [primary, ...others, cancel];
  end.append(...ordered.filter((b): b is HTMLElement => !!b));
  row.append(start, end);
  host.append(row);
  return row;
}

/** A group's title over the rows that follow it, with an optional note under the title. */
export function groupTitle(host: ParentNode, title: string, note = ""): HTMLHeadingElement {
  const h = document.createElement("h3");
  h.className = "ds-group-title";
  h.textContent = title;
  host.append(h);
  if (note) {
    const p = document.createElement("p");
    p.className = "ds-group-note";
    p.textContent = note;
    host.append(p);
  }
  return h;
}

/** The name side of a row: `text` holding the `name`, and the `description` under it when there
 *  is one. */
function rowText(title: string | Node, sub: string): { text: HTMLSpanElement; name: HTMLSpanElement; description: HTMLSpanElement | null } {
  const text = document.createElement("span");
  text.className = "ds-row-text";
  const name = document.createElement("span");
  name.className = "ds-row-title";
  name.append(title);
  text.append(name);
  if (!sub) return { text, name, description: null };
  const description = document.createElement("span");
  description.className = "ds-row-sub";
  description.textContent = sub;
  text.append(description);
  return { text, name, description };
}

/** Controls for a row's end: a string (a unit, or the text before a field) becomes a unit slot,
 *  so fields whose units differ still end on one line. */
function slotted(controls: (Node | string)[]): Node[] {
  return controls.map((c) => {
    if (typeof c !== "string") return c;
    const unit = document.createElement("span");
    unit.className = "ds-row-unit";
    unit.textContent = c;
    return unit;
  });
}

/** Element ids for the names that rows give their controls. */
let ids = 0;

/** Gives each control among `slots` that has no name of its own one from the row: a segmented
 *  group and each field take the row's name `name`, a field also the unit slot before it and the
 *  one after it unless another field follows that ("限时 [10] 分钟", "最小手数 [a] 连续 [b] 手"), so
 *  several fields in one row stay apart. A field named by its own label or `aria-label` keeps that
 *  name; a radio button is named by its segment. */
function nameControls(name: HTMLElement, slots: Node[]): void {
  name.id ||= `ds-row-name-${++ids}`;
  const unit = (near: Node | undefined): near is HTMLElement => near instanceof HTMLElement && near.classList.contains("ds-row-unit");
  slots.forEach((slot, at) => {
    if (!(slot instanceof Element)) return;
    for (const el of [slot, ...slot.querySelectorAll("*")]) {
      if (el.hasAttribute("aria-label") || el.hasAttribute("aria-labelledby")) continue;
      if (el.getAttribute("role") === "radiogroup") {
        el.setAttribute("aria-labelledby", name.id);
        continue;
      }
      if (!el.matches("input:not([type=hidden]), select, textarea") || el.closest("[role=radiogroup]")) continue;
      const label = el.closest("label");
      if (label && (label.htmlFor ? label.htmlFor === el.id : label.querySelector("input, select, textarea, button, meter, output, progress") === el)) continue;
      const names = [name.id];
      const before = slots[at - 1];
      const after = slots[at + 1];
      const next = slots[at + 2];
      const fieldNext = next instanceof Element && (next.matches("input, select, textarea") || !!next.querySelector("input, select, textarea"));
      if (unit(before)) names.push((before.id ||= `ds-row-unit-${++ids}`));
      if (unit(after) && !fieldNext) names.push((after.id ||= `ds-row-unit-${++ids}`));
      el.setAttribute("aria-labelledby", names.join(" "));
    }
  });
}

/** Names `control` by the row's `name` and describes it by the row's `description`, so its name
 *  leaves out the description and any field the row's label also holds. A control named by its
 *  own `aria-label` keeps that name. */
function nameByRow(control: HTMLElement, name: HTMLElement, description: HTMLElement | null): void {
  name.id ||= `ds-row-name-${++ids}`;
  if (!control.hasAttribute("aria-label") && !control.hasAttribute("aria-labelledby")) control.setAttribute("aria-labelledby", name.id);
  if (!description) return;
  description.id ||= `ds-row-sub-${++ids}`;
  control.setAttribute("aria-describedby", description.id);
}

/** A row of a group: its name with `sub` under it, `controls` at the end. A row with one bare field
 *  is a label, so its name focuses the field, and names it as nameByRow says; otherwise its
 *  controls are named as nameControls says. Consecutive rows form one group. */
export function row(host: ParentNode, title: string | Node, sub: string, ...controls: (Node | string)[]): HTMLElement {
  const only = controls.length === 1 ? controls[0] : null;
  const single = only instanceof HTMLElement && only.matches("input:not([type=checkbox], [type=radio]), select, textarea") ? only : null;
  const r = document.createElement(single ? "label" : "div");
  r.className = "ds-row";
  const end = document.createElement("span");
  end.className = "ds-row-control";
  const slots = slotted(controls);
  end.append(...slots);
  const { text, name, description } = rowText(title, sub);
  if (single) nameByRow(single, name, description);
  else nameControls(name, slots);
  r.append(text, end);
  host.append(r);
  return r;
}

/** A check box drawn as a sliding switch. */
export function switchBox(checked: boolean): HTMLInputElement {
  const el = input("", "checkbox");
  el.className = "ds-switch";
  el.checked = checked;
  return el;
}

/** A row whose whole surface toggles its switch; `extra` controls stand before the switch and
 *  wrap among themselves, so the switch stays at the row's end. The row is the switch's label for
 *  clicks; the switch is named as nameByRow says (not by the fields inside the row), the extra
 *  fields as nameControls says. */
export function switchRow(host: ParentNode, title: string, sub: string, checked: boolean, ...extra: (Node | string)[]): { row: HTMLLabelElement; input: HTMLInputElement } {
  const box = switchBox(checked);
  const r = document.createElement("label");
  r.className = "ds-row";
  // Clicked through `for`: a label without it would label the first extra field instead.
  box.id = `ds-switch-${++ids}`;
  r.htmlFor = box.id;
  const { text, name, description } = rowText(title, sub);
  nameByRow(box, name, description);
  const end = document.createElement("span");
  end.className = "ds-row-control";
  if (extra.length) {
    const fields = document.createElement("span");
    fields.className = "ds-row-fields";
    const slots = slotted(extra);
    fields.append(...slots);
    nameControls(name, slots);
    end.append(fields);
  }
  end.append(box);
  r.append(text, end);
  host.append(r);
  return { row: r, input: box };
}

/** Two to four exclusive choices as one segmented control over radio buttons named `name`;
 *  `inputs` are the radios in `choices` order, `value()` the checked choice's value (`value`
 *  while none is checked). */
export function segmented<T extends string>(name: string, choices: [value: T, text: string][], value: T): { el: HTMLSpanElement; inputs: HTMLInputElement[]; value: () => T } {
  const el = document.createElement("span");
  el.className = "ds-seg";
  el.setAttribute("role", "radiogroup");
  const inputs = choices.map(([v, text]) => {
    const radio = input(v, "radio");
    radio.name = name;
    radio.checked = v === value;
    const label = document.createElement("label");
    label.append(radio, text);
    el.append(label);
    return radio;
  });
  return {
    el,
    inputs,
    value: () => {
      const k = inputs.findIndex((r) => r.checked);
      return k < 0 ? value : choices[k][0];
    },
  };
}

/** A note after a group: what applies to the rows above it as a whole. */
export function groupNote(host: ParentNode, text: string): HTMLParagraphElement {
  const p = document.createElement("p");
  p.className = "ds-note";
  p.textContent = text;
  host.append(p);
  return p;
}
