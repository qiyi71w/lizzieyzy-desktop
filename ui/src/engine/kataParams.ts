// 引擎参数 (LizzieFrame.setLzSaiEngine, LizzieFrame.java:13109-13121) and its KataGo window
// (SetKataEngines.java); the Leela Zero / Sai window is engine/leelaOptions.ts. The toolbar's
// 激进度 / 广度 groups are in chrome/toolbar.ts.
import { invoke } from "@tauri-apps/api/core";
import { run, state } from "../app";
import type { KataParam, KataParamEdit } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { product, t } from "../i18n";
import { matchPlaying } from "../match/state";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { actions, button } from "./controls";
import { openLeelaEngines } from "./leelaOptions";

/** Leelaz.isKataGoPda: a KataGoPda build sets its PDA in its own window, not here. */
export function kataGoPda(): boolean {
  return state.view?.engine.foreground?.gtp_name.startsWith("KataGoPda") ?? false;
}

/** Function search shows a hidden group for this run only (Menu.locateTopFunction); the saved
 *  show-*-in-menu choice stays as it is. */
export const kataRevealed: Record<KataParam, boolean> = { Pda: false, Wrn: false };

/** Menu.setUseGfPda / setUseWrn: the toolbar group takes the window's 修改 and value. */
export const kataToolbarUse: Partial<Record<KataParam, (enabled: boolean, text: string) => void>> = {};

/** The toolbar group's 修改 and value now: Java's chkKataEngine* / txtKataEngine*, which the
 *  engine's read-back (LizzieFrame.setPdaAndWrn) and every toolbar edit keep up to date. */
export const kataToolbarState: Partial<Record<KataParam, () => { enabled: boolean; text: string }>> = {};

/** The window's 修改 and value of `param`: the toolbar's once the engine reported the parameter
 *  (or the toolbar changed it), else the saved ones. */
function current(param: KataParam, enabled: boolean, value: number | null): { enabled: boolean; text: string } {
  const reported = state.view?.engine.kata_params[param === "Pda" ? "pda" : "wrn"] ?? null;
  const toolbar = reported === null ? undefined : kataToolbarState[param]?.();
  return toolbar ?? { enabled, text: value === null ? "" : javaDouble(value) };
}

/** The fields' red background: Double.parseDouble fails or the value is outside Java's range. */
export function kataValueInvalid(param: KataParam, text: string): boolean {
  const v = parseJavaDouble(text);
  return v === null || (param === "Pda" ? v < -3 || v > 3 : v < 0 || v > 2);
}

/** setLzSaiEngine: no loaded engine → the hint; KataGo → SetKataEngines; Leela Zero / Sai →
 *  SetLeelaEngines. Java opens SetLeelaEngines for every other engine too, whose options would
 *  not apply; those get Java's not-Leela/Sai notice instead. */
export async function openEngineParameters(): Promise<void> {
  const foreground = state.view?.engine.foreground;
  if (!foreground?.ready) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
  if (foreground.katago) { await openKataEngines(); return; }
  if (foreground.leela_sai) { await openLeelaEngines(); return; }
  await message(t("Message.title"), t("LizzieFrame.setParamsWarning"));
}

/** The red / blue parts and the 这里 link of SetKataEngines.Hint2 (LinkLabel). */
function hint2(): HTMLElement {
  const el = document.createElement("div");
  el.className = "kata-params-hint";
  const source = t("SetKataEngines.Hint2").replace(/^<html>|<\/html>$/g, "");
  const parsed = new DOMParser().parseFromString(source, "text/html").body;
  for (const font of parsed.querySelectorAll("font")) {
    const span = document.createElement("span");
    span.style.color = font.getAttribute("color") ?? "";
    span.append(...font.childNodes);
    font.replaceWith(span);
  }
  for (const link of parsed.querySelectorAll("a")) {
    const span = document.createElement("span");
    span.className = "kata-params-link";
    span.title = link.getAttribute("href") ?? "";
    span.append(...link.childNodes);
    span.onclick = async () => {
      try { await invoke("open_katago_parameter_help"); } catch { /* Java ignores a browser that cannot open. */ }
    };
    link.replaceWith(span);
  }
  el.append(...parsed.childNodes);
  return el;
}

function check(checked: boolean): HTMLInputElement {
  const c = document.createElement("input");
  c.type = "checkbox";
  c.checked = checked;
  return c;
}

/** One SetKataEngines row: 修改, the value, 自动加载 and 在工具栏中控制. */
interface Row { edit: HTMLInputElement; value: HTMLInputElement; autoLoad: HTMLInputElement; inMenu: HTMLInputElement }

function row(grid: HTMLElement, param: KataParam, label: string, enabled: boolean, text: string, autoLoad: boolean, inMenu: boolean): Row {
  const r: Row = { edit: check(enabled), value: document.createElement("input"), autoLoad: check(autoLoad), inMenu: check(inMenu) };
  r.value.value = text;
  r.value.dataset.key = `kataParams.${param.toLowerCase()}`;
  r.edit.dataset.key = `kataParams.${param.toLowerCase()}Edit`;
  r.autoLoad.dataset.key = `kataParams.${param.toLowerCase()}AutoLoad`;
  r.inMenu.dataset.key = `kataParams.${param.toLowerCase()}InMenu`;
  const name = document.createElement("span");
  name.textContent = label;
  const paint = () => r.value.classList.toggle("invalid", kataValueInvalid(param, r.value.value));
  r.value.addEventListener("input", paint);
  paint();
  // 修改 off clears and disables 自动加载; a checked 自动加载 turns 修改 on when opening.
  const editChanged = () => {
    r.value.disabled = !r.edit.checked;
    r.autoLoad.disabled = !r.edit.checked;
    if (!r.edit.checked) r.autoLoad.checked = false;
  };
  r.edit.onchange = editChanged;
  if (autoLoad) r.edit.checked = true;
  editChanged();
  grid.append(name, r.edit, r.value, r.autoLoad, r.inMenu);
  return r;
}

async function openKataEngines(): Promise<void> {
  const view = state.view;
  if (!view) return;
  const s = view.settings.engine;
  const p = s.kata_params;
  const pdaEngine = kataGoPda();
  const body = document.createElement("div");
  body.className = "kata-params";
  const grid = document.createElement("div");
  grid.className = "kata-params-grid";
  const head = (text = "") => { const h = document.createElement("span"); h.className = "kata-params-head"; h.textContent = text; return h; };
  grid.append(head(), head(), head(), head(t("SetKataEngines.lblAutoLoad")), head(t("SetKataEngines.lblShowInMenu")));

  const pdaNow = current("Pda", s.kata_pda_enabled, s.kata_pda);
  const pda = row(grid, "Pda", t("SetKataEngines.lblPDA"), pdaNow.enabled, pdaNow.text, p.autoLoadPda, p.showPda);
  if (pdaEngine) {
    // SetKataEngines.java:591-605: a KataGoPda engine's PDA is set from the toolbar's own panel.
    const note = document.createElement("span");
    note.className = "kata-params-pda-engine";
    note.textContent = t("SetKataEngines.lblPdaEngineUseMenu");
    pda.edit.replaceWith(note);
    for (const control of [pda.value, pda.autoLoad, pda.inMenu]) control.remove();
  }
  const wrnNow = current("Wrn", p.wrnEnabled, p.wrn);
  const wrn = row(grid, "Wrn", t("SetKataEngines.lblWRN"), wrnNow.enabled, wrnNow.text, p.autoLoadWrn, p.showWrn);
  // SetKataEngines.java:586-589: 广度 cannot be changed while the engine plays a genmove game.
  if (view.game?.kind === "Genmove" || (matchPlaying(view) && view.engine_match?.kind === "Genmove")) wrn.value.disabled = wrn.edit.disabled = true;

  // 搜索线程数 (SetKataEngines.java:491-528): this run's numSearchThreads is not migrated yet.
  const threads = document.createElement("span");
  threads.textContent = t("SetKataEngines.lblNumSearchThreads");
  threads.title = t("SetKataEngines.threadLocalTooltip");
  const threadCheck = check(false);
  const threadValue = document.createElement("input");
  for (const control of [threadCheck, threadValue]) {
    control.disabled = true;
    control.classList.add("unavailable");
    control.title = product("unavailable");
  }
  grid.append(threads, threadCheck, threadValue, head(), head());

  const hint = document.createElement("div");
  hint.textContent = t("SetKataEngines.lblHint");
  body.append(grid, hint, hint2());

  const dialog = formPanel(t("SetKataEngines.title"), body);
  const apply = button(t("SetKataEngines.btnApply"), async () => {
    const [pdaEdit, wrnEdit]: KataParamEdit[] = [pda, wrn].map((r) => ({ enabled: r.edit.checked, value: parseJavaDouble(r.value.value), auto_load: r.autoLoad.checked }));
    const result = await run({
      type: "SaveKataEngineParams",
      params: { pda: pdaEngine ? null : pdaEdit, wrn: wrnEdit, show_pda: pda.inMenu.checked, show_wrn: wrn.inMenu.checked },
    });
    if (!result || result.type === "Refused") return;
    if (!pdaEngine) kataToolbarUse.Pda?.(pda.edit.checked, pda.value.value);
    kataToolbarUse.Wrn?.(wrn.edit.checked, wrn.value.value);
    await dialog.close();
  });
  apply.dataset.key = "kataParams.apply";
  const cancel = button(t("SetKataEngines.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "kataParams.cancel";
  actions(body, apply, cancel);
  await dialog.closed;
}
