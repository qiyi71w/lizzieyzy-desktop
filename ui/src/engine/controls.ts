// The rows and controls of a form dialog (dialogs.ts formPanel); design/dialog.css draws them.
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
