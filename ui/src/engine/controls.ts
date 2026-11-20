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

/** A row of a group: its name with `sub` under it, `controls` at the end. A row with one field
 *  is a label, so its name focuses the field. Consecutive rows form one group. */
export function row(host: ParentNode, title: string | Node, sub: string, ...controls: (Node | string)[]): HTMLElement {
  const single = controls.length === 1 && controls[0] instanceof HTMLElement && controls[0].matches("input:not([type=checkbox], [type=radio]), select, textarea");
  const r = document.createElement(single ? "label" : "div");
  r.className = "ds-row";
  const end = document.createElement("span");
  end.className = "ds-row-control";
  end.append(...controls);
  r.append(rowText(title, sub), end);
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
 *  wrap among themselves, so the switch stays at the row's end. */
export function switchRow(host: ParentNode, title: string, sub: string, checked: boolean, ...extra: (Node | string)[]): { row: HTMLLabelElement; input: HTMLInputElement } {
  const box = switchBox(checked);
  const r = document.createElement("label");
  r.className = "ds-row";
  const end = document.createElement("span");
  end.className = "ds-row-control";
  if (extra.length) {
    const fields = document.createElement("span");
    fields.className = "ds-row-fields";
    fields.append(...extra);
    end.append(fields);
  }
  end.append(box);
  r.append(rowText(title, sub), end);
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
