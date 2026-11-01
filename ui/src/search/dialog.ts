// FunctionSearchDialog (Java cb53acc): a stationary centred search bar; the results unfold below it
// with their real paths and the selected one's details. The field's own editing and the input
// method keep their keys. It is a modal layer of the dialog host (dialogs.ts openModalLayer): the
// rest of the window is inert while it is open, and a dialog opened over it takes the input.
import { state } from "../app";
import { isApiError, searchFunctions } from "../contract/client";
import type { FunctionMatch, FunctionTarget } from "../contract/generated";
import { openModalLayer } from "../dialogs";
import { t } from "../i18n";
import { reasonKey, reasonText, resolveFunction, type Resolution } from "./targets";
import "./search.css";

/** FunctionCatalog.categoryKeys: the category box's order after 全部功能. */
const CATEGORIES = [
  "FunctionSearch.category.file",
  "FunctionSearch.category.analysis",
  "FunctionSearch.category.game",
  "FunctionSearch.category.engine",
  "FunctionSearch.category.sync",
  "FunctionSearch.category.view",
];

/** A typed query shows at most this many results; browsing shows them all. */
const QUERY_CAP = 50;

/** FunctionSearch.isEmptyQuery: nothing but (Unicode) white space after NFKC. */
export function isEmptyQuery(query: string): boolean {
  return /^[\s\u0085\u001c-\u001f]*$/u.test(query.normalize("NFKC"));
}

/** SearchInputSession: one physical Enter, kept apart from the input method's commit. Enter acts
 *  on its release, and only when its press was not composing and did not commit text. */
class InputSession {
  composing = false;
  private committed = false;
  private committedAt = 0;
  private armed = false;

  /** A key event the input method is processing (WebKit and Chromium report 229). */
  imeKey(e: KeyboardEvent): boolean {
    return this.composing || e.isComposing || e.keyCode === 229;
  }

  start(): void {
    this.composing = true;
    this.armed = false;
  }

  end(at: number): void {
    this.composing = false;
    this.committed = true;
    this.committedAt = at;
    this.armed = false;
  }

  press(e: KeyboardEvent): void {
    // A later physical press follows a completed Space/mouse commit; the Enter that caused a
    // commit is a composing key event (isComposing / 229) and stays unarmed.
    if (this.committed && e.timeStamp > this.committedAt) this.committed = false;
    this.armed = !this.imeKey(e) && !this.committed && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
  }

  release(): boolean {
    const activate = this.armed && !this.composing && !this.committed;
    this.armed = false;
    this.committed = false;
    return activate;
  }
}

function element<K extends keyof HTMLElementTagNameMap>(tag: K, className: string, text?: string): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

function typeText(target: FunctionTarget): string {
  switch (target) {
    case "Setting":
      return t("FunctionSearch.type.setting");
    case "Action":
      return t("FunctionSearch.type.action");
    case "Window":
      return t("FunctionSearch.type.window");
    default:
      return t("FunctionSearch.type.navigation");
  }
}

export interface SearchDialogReport {
  query: string;
  expanded: boolean;
  category: string | null;
  count: string;
  /** Result ids in the order shown. */
  rows: string[];
  selected: string | null;
  activateEnabled: boolean;
  composing: boolean;
  focus: "input" | "results" | "category" | "activate" | "cancel" | "browse" | null;
  geometry: { x: number; y: number; width: number; height: number };
}

export interface SearchDialog {
  /** The modal layer, for checks that leave the search itself out (dialogOpen(except)). */
  readonly layer: HTMLElement;
  /** The chosen result once the dialog closed, null when it was cancelled. */
  readonly done: Promise<FunctionMatch | null>;
  contains(target: EventTarget | null): boolean;
  focus(): void;
  report(): SearchDialogReport;
}

export function openSearchDialog(): SearchDialog {
  const { promise, resolve } = Promise.withResolvers<FunctionMatch | null>();
  const session = new InputSession();
  const availability = new Map<string, Resolution>();
  let closed = false;
  let expanded = false;
  let generation = 0;
  let cached: { query: string; matches: FunctionMatch[] } | null = null;
  let model: FunctionMatch[] = [];
  let selected = -1;
  /** Why the last search failed, shown in place of the results until a search succeeds. */
  let failure: string | null = null;

  const overlay = element("div", "fs-overlay");
  const surface = element("div", "fs-surface");
  surface.setAttribute("role", "dialog");
  surface.setAttribute("aria-modal", "true");
  surface.setAttribute("aria-label", t("FunctionSearch.title"));

  const top = element("div", "fs-top");
  const icon = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  icon.setAttribute("class", "fs-icon");
  icon.setAttribute("viewBox", "0 0 16 16");
  icon.setAttribute("aria-hidden", "true");
  icon.innerHTML = '<circle cx="5.5" cy="5.5" r="4.5"/><line x1="9" y1="9" x2="14" y2="14"/>';
  const input = element("input", "fs-input");
  input.type = "text";
  input.spellcheck = false;
  input.autocomplete = "off";
  input.placeholder = t("FunctionSearch.hint");
  input.title = t("FunctionSearch.hint");
  input.setAttribute("aria-label", t("FunctionSearch.title"));
  input.dataset.key = "function.search.input";
  const cancel = element("button", "fs-button", t("FunctionSearch.cancel"));
  cancel.dataset.key = "function.search.cancel";
  top.append(icon, input, cancel);

  const lower = element("div", "fs-lower");
  const browse = element("button", "fs-button fs-browse", t("FunctionSearch.browse"));
  browse.dataset.key = "function.search.browse";
  const expansion = element("div", "fs-expansion");
  const categoryLine = element("div", "fs-category-line");
  const categories = element("select", "fs-categories");
  categories.setAttribute("aria-label", t("FunctionSearch.browse"));
  categories.dataset.key = "function.search.category";
  categories.append(new Option(t("FunctionSearch.all"), ""));
  for (const key of CATEGORIES) categories.append(new Option(t(key), key));
  const count = element("span", "fs-count");
  categoryLine.append(categories, count);
  const results = element("div", "fs-results");
  results.tabIndex = 0;
  results.setAttribute("role", "listbox");
  results.setAttribute("aria-label", t("FunctionSearch.browse"));
  results.dataset.key = "function.search.results";
  const footer = element("div", "fs-footer");
  const activate = element("button", "fs-button primary", t("FunctionSearch.activate"));
  activate.dataset.key = "function.search.activate";
  footer.append(element("span", "fs-keyboard", t("FunctionSearch.keyboard")), activate);
  expansion.append(categoryLine, results, footer);
  lower.append(browse, expansion);
  surface.append(top, lower);
  overlay.append(surface);

  // APPLICATION_MODAL: everything else in the window is inert until the search closes.
  const closeLayer = openModalLayer(overlay);

  // Centred over the window; the bar stays where it opened and the results unfold downward
  // (FunctionSearchDialog.java:163-195, the window standing for the owner and its work area).
  const width = Math.max(240, Math.min(720, window.innerWidth - 48));
  const x = Math.max(16, Math.min((window.innerWidth - width) / 2, window.innerWidth - width - 16));
  const fontSize = Number.parseFloat(getComputedStyle(surface).fontSize) || 12;
  const anchorY = Math.max(16, Math.min((window.innerHeight - top.offsetHeight) / 2, window.innerHeight - Math.max(220, fontSize * 10)));
  const availableBottom = window.innerHeight - 16;
  surface.style.left = `${Math.round(x)}px`;
  surface.style.top = `${Math.round(anchorY)}px`;
  surface.style.width = `${width}px`;

  function layout(): void {
    browse.hidden = expanded;
    expansion.hidden = !expanded;
    surface.style.height = expanded ? `${Math.max(180, availableBottom - anchorY)}px` : "";
  }

  function current(): FunctionMatch | null {
    return model[selected] ?? null;
  }

  function row(index: number): HTMLElement {
    const m = model[index];
    const resolution = availability.get(m.id)!;
    const isSelected = index === selected;
    const r = element("div", "fs-row");
    r.id = `fs-row-${index}`;
    r.dataset.index = String(index);
    r.dataset.function = m.id;
    r.setAttribute("role", "option");
    r.setAttribute("aria-selected", String(isSelected));
    r.classList.toggle("selected", isSelected);
    r.classList.toggle("unavailable", resolution.reason !== null);
    const title = t(m.title);
    const path = m.path.map((key) => t(key)).join(" › ");
    const heading = element("div", "fs-heading");
    heading.append(element("span", "fs-title", title), element("span", "fs-path", `  ${path}`));
    r.append(heading);
    if (isSelected) {
      const lines = [typeText(m.target) + (m.shortcut ? ` · ${m.shortcut}` : ""), t(m.description)];
      if (resolution.dependency) lines.push(reasonText(resolution.dependency));
      if (resolution.reason) lines.push(reasonText(resolution.reason));
      r.append(element("div", "fs-detail", lines.join("\n")));
    }
    r.setAttribute("aria-label", `${title} ${path}`);
    r.setAttribute("aria-description", resolution.reason ? reasonText(resolution.reason) : t(m.description) + (resolution.dependency ? ` ${reasonText(resolution.dependency)}` : ""));
    return r;
  }

  function renderRows(): void {
    if (failure !== null) {
      // The search itself failed: say so instead of "no matching features"; the next query retries.
      const alert = element("div", "fs-empty fs-failure", failure);
      alert.setAttribute("role", "alert");
      results.replaceChildren(alert);
      results.setAttribute("aria-description", failure);
      results.removeAttribute("aria-activedescendant");
    } else if (model.length === 0) {
      results.replaceChildren(element("div", "fs-empty", t("FunctionSearch.empty")));
      results.setAttribute("aria-description", t("FunctionSearch.empty"));
      results.removeAttribute("aria-activedescendant");
    } else {
      const scroll = results.scrollTop;
      results.replaceChildren(...model.map((_, index) => row(index)));
      results.scrollTop = scroll;
      results.setAttribute("aria-description", count.textContent ?? "");
      if (selected >= 0) results.setAttribute("aria-activedescendant", `fs-row-${selected}`);
    }
    refreshActivate();
  }

  function refreshActivate(): void {
    const m = current();
    activate.disabled = !m || availability.get(m.id)?.reason != null;
  }

  function select(index: number): void {
    if (index < 0 || index >= model.length) return;
    const previous = selected;
    selected = index;
    const m = model[index];
    availability.set(m.id, resolveFunction(m, overlay));
    for (const i of new Set([previous, index])) {
      const old = results.children[i];
      if (i >= 0 && old) old.replaceWith(row(i));
    }
    results.setAttribute("aria-activedescendant", `fs-row-${index}`);
    results.children[index]?.scrollIntoView({ block: "nearest" });
    refreshActivate();
  }

  /** refreshResults: the chosen category of the query's (or the whole catalog's) results, in the
   *  order the search returned them. The count is of all of them; a typed query lists 50. */
  async function refresh(): Promise<void> {
    const query = input.value;
    const browsing = isEmptyQuery(query);
    const mine = ++generation;
    if (browsing && !expanded) {
      model = [];
      selected = -1;
      failure = null;
      availability.clear();
      renderRows();
      return;
    }
    let matches = cached?.query === query ? cached.matches : null;
    if (!matches) {
      try {
        matches = await searchFunctions(query, state.view?.settings.active_language ?? "ZhCn");
      } catch (e) {
        if (mine !== generation || closed) return;
        console.error("function search failed", e);
        model = [];
        selected = -1;
        failure = isApiError(e) ? e.message : String(e);
        availability.clear();
        count.textContent = "";
        renderRows();
        return;
      }
      if (mine !== generation || closed) return;
      cached = { query, matches };
    }
    failure = null;
    const category = categories.value || null;
    const previous = current()?.id;
    const shown: FunctionMatch[] = [];
    let total = 0;
    for (const m of matches) {
      if (category && m.category !== category) continue;
      total++;
      if (browsing || shown.length < QUERY_CAP) shown.push(m);
    }
    model = shown;
    availability.clear();
    for (const m of model) availability.set(m.id, resolveFunction(m, overlay));
    selected = previous === undefined ? -1 : model.findIndex((m) => m.id === previous);
    if (selected < 0 && model.length > 0) selected = 0;
    count.textContent = t("FunctionSearch.results").replace("%d", String(total));
    renderRows();
    results.children[selected]?.scrollIntoView({ block: "nearest" });
  }

  /** stateTimer: availability follows the application while the search is open. */
  const timer = window.setInterval(() => {
    let changed = false;
    for (const m of model) {
      const next = resolveFunction(m, overlay);
      const old = availability.get(m.id);
      if (reasonKey(next.reason) !== reasonKey(old?.reason ?? null) || reasonKey(next.dependency) !== reasonKey(old?.dependency ?? null)) {
        availability.set(m.id, next);
        changed = true;
      }
    }
    if (changed) renderRows();
  }, 250);

  function close(result: FunctionMatch | null): void {
    if (closed) return;
    closed = true;
    clearInterval(timer);
    closeLayer();
    resolve(result);
  }

  /** activateSelection: an unavailable result is re-checked and stays open. */
  function activateSelection(): void {
    if (closed || session.composing) return;
    const m = current();
    if (!m) return;
    const resolution = resolveFunction(m, overlay);
    availability.set(m.id, resolution);
    if (resolution.reason) {
      select(selected);
      return;
    }
    close(m);
  }

  async function step(direction: number): Promise<void> {
    if (!expanded) {
      expanded = true;
      layout();
    }
    if (model.length === 0) await refresh();
    if (closed || model.length === 0) return;
    select(Math.max(0, Math.min(model.length - 1, selected + direction)));
  }

  /** handleKey of the field and the list. */
  function keyDown(e: KeyboardEvent): void {
    if (e.key === "Enter") {
      const composing = session.imeKey(e);
      session.press(e);
      // While composing, the candidate window keeps Enter.
      if (!composing) e.preventDefault();
      return;
    }
    if (session.imeKey(e) || e.ctrlKey || e.altKey || e.shiftKey || e.metaKey) return;
    if (e.key === "ArrowUp" || e.key === "ArrowDown") {
      e.preventDefault();
      void step(e.key === "ArrowDown" ? 1 : -1);
    }
  }

  function keyUp(e: KeyboardEvent): void {
    if (e.key === "Enter" && session.release()) activateSelection();
  }

  for (const target of [input, results]) {
    target.addEventListener("keydown", keyDown);
    target.addEventListener("keyup", keyUp);
  }
  input.addEventListener("compositionstart", () => session.start());
  input.addEventListener("compositionend", (e) => session.end(e.timeStamp));
  input.addEventListener("input", () => {
    if (closed) return;
    expanded = !isEmptyQuery(input.value);
    layout();
    void refresh();
  });
  categories.addEventListener("change", () => void refresh());
  cancel.addEventListener("click", () => close(null));
  activate.addEventListener("click", activateSelection);
  browse.addEventListener("click", () => {
    expanded = true;
    layout();
    void refresh();
  });
  results.addEventListener("click", (e) => {
    const r = (e.target as Element).closest<HTMLElement>(".fs-row");
    if (r) select(Number(r.dataset.index));
  });
  results.addEventListener("dblclick", (e) => {
    const r = (e.target as Element).closest<HTMLElement>(".fs-row");
    if (!r) return;
    select(Number(r.dataset.index));
    activateSelection();
  });
  surface.addEventListener("keydown", (e) => {
    // The root pane's Escape binding: closes unless the input method is composing.
    if (e.key === "Escape" && !session.imeKey(e) && !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey) {
      e.preventDefault();
      close(null);
      return;
    }
    if (e.key === "Tab") {
      const items = [...surface.querySelectorAll<HTMLElement>("input, select, button:not(:disabled), .fs-results")].filter((el) => el.getClientRects().length > 0);
      const at = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length]?.focus();
    }
  });
  // Keys stay inside the modal search, as in a Swing modal dialog.
  for (const type of ["keydown", "keyup", "keypress"]) overlay.addEventListener(type, (e) => e.stopPropagation());
  overlay.addEventListener("contextmenu", (e) => e.preventDefault());

  layout();
  void refresh();
  input.focus();

  return {
    done: promise,
    layer: overlay,
    contains: (target) => target instanceof Node && overlay.contains(target),
    focus: () => input.focus(),
    report: () => {
      const active = document.activeElement;
      const focus = active === input ? "input" : active === results ? "results" : active === categories ? "category" : active === activate ? "activate" : active === cancel ? "cancel" : active === browse ? "browse" : null;
      const r = surface.getBoundingClientRect();
      return {
        query: input.value,
        expanded,
        category: categories.value || null,
        count: expanded ? count.textContent ?? "" : "",
        rows: model.map((m) => m.id),
        selected: current()?.id ?? null,
        activateEnabled: !activate.disabled,
        composing: session.composing,
        focus,
        geometry: { x: Math.round(r.left), y: Math.round(r.top), width: Math.round(r.width), height: Math.round(r.height) },
      };
    },
  };
}
