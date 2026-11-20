// The rows and controls of a form dialog (dialogs.ts formPanel); design/dialog.css draws them.
// Dialogs are grouped lists (DESIGN.md Dialogs): a group title, then rows on one raised surface,
// each row its name (and a description under it) at the start and its controls at the end.
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

export function field(host: HTMLElement, key: string, control: HTMLElement): void {
  const label = document.createElement("label");
  label.className = "engine-field";
  label.append(key, control);
  host.append(label);
}

export function actions(host: HTMLElement, ...items: (HTMLElement | string)[]): void {
  const row = document.createElement("div");
  row.className = "engine-actions";
  row.append(...items);
  host.append(row);
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

/** The name side of a row: the name, and the description under it when there is one. */
function rowText(title: string | Node, sub: string): HTMLSpanElement {
  const text = document.createElement("span");
  text.className = "ds-row-text";
  const name = document.createElement("span");
  name.className = "ds-row-title";
  name.append(title);
  text.append(name);
  if (sub) {
    const p = document.createElement("span");
    p.className = "ds-row-sub";
    p.textContent = sub;
    text.append(p);
  }
  return text;
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
 *  group and each field take the row's name `name`, a field followed by the unit slots beside it
 *  ("限时 [10] 分钟"), so several fields in one row stay apart. A field named by its own label or
 *  `aria-label` keeps that name; a radio button is named by its segment. */
function nameControls(name: HTMLElement, slots: Node[]): void {
  name.id ||= `ds-row-name-${++ids}`;
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
      for (const near of [slots[at - 1], slots[at + 1]]) {
        if (near instanceof HTMLElement && near.classList.contains("ds-row-unit")) names.push((near.id ||= `ds-row-unit-${++ids}`));
      }
      el.setAttribute("aria-labelledby", names.join(" "));
    }
  });
}

/** A row of a group: its name with `sub` under it, `controls` at the end. A row with one bare field
 *  is a label, so its name focuses the field; otherwise its controls are named as nameControls
 *  says. Consecutive rows form one group. */
export function row(host: ParentNode, title: string | Node, sub: string, ...controls: (Node | string)[]): HTMLElement {
  const single = controls.length === 1 && controls[0] instanceof HTMLElement && controls[0].matches("input:not([type=checkbox], [type=radio]), select, textarea");
  const r = document.createElement(single ? "label" : "div");
  r.className = "ds-row";
  const end = document.createElement("span");
  end.className = "ds-row-control";
  const slots = slotted(controls);
  end.append(...slots);
  const text = rowText(title, sub);
  if (!single) nameControls(text.firstElementChild as HTMLElement, slots);
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
 *  wrap among themselves, so the switch stays at the row's end. The row is the switch's label;
 *  the extra fields are named as nameControls says. */
export function switchRow(host: ParentNode, title: string, sub: string, checked: boolean, ...extra: (Node | string)[]): { row: HTMLLabelElement; input: HTMLInputElement } {
  const box = switchBox(checked);
  const r = document.createElement("label");
  r.className = "ds-row";
  // Named explicitly: a label without `for` would label the first extra field instead.
  box.id = `ds-switch-${++ids}`;
  r.htmlFor = box.id;
  const text = rowText(title, sub);
  const end = document.createElement("span");
  end.className = "ds-row-control";
  if (extra.length) {
    const fields = document.createElement("span");
    fields.className = "ds-row-fields";
    const slots = slotted(extra);
    fields.append(...slots);
    nameControls(text.firstElementChild as HTMLElement, slots);
    end.append(fields);
  }
  end.append(box);
  r.append(text, end);
  host.append(r);
  return { row: r, input: box };
}

/** Two to four exclusive choices as one segmented control over radio buttons named `name`;
 *  `inputs` are the radios in `choices` order. */
export function segmented(name: string, choices: [value: string, text: string][], value: string): { el: HTMLSpanElement; inputs: HTMLInputElement[]; value: () => string } {
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
  return { el, inputs, value: () => inputs.find((r) => r.checked)?.value ?? value };
}

/** A note after a group: what applies to the rows above it as a whole. */
export function groupNote(host: ParentNode, text: string): HTMLParagraphElement {
  const p = document.createElement("p");
  p.className = "ds-note";
  p.textContent = text;
  host.append(p);
  return p;
}
