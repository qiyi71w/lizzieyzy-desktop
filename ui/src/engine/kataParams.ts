// 引擎参数 (LizzieFrame.setLzSaiEngine, LizzieFrame.java:13109-13121) and its KataGo window
// (SetKataEngines.java); the Leela Zero / Sai window is engine/leelaOptions.ts. The toolbar's
// 激进度 / 广度 groups are in chrome/toolbar.ts.
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run, state } from "../app";
import type { KataEngineParamsEdit, KataParam, KataParamEdit, SearchThreadsCommand, SearchThreadsStatus, ThreadRestoreSource } from "../contract/generated";
import { confirm, formPanel, message } from "../dialogs";
import { format, t } from "../i18n";
import { matchPlaying } from "../match/state";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { actions, button } from "./controls";
import { openLeelaEngines } from "./leelaOptions";

/** 搜索线程数 operations the window waits for, by ticket. A newer operation supersedes an older
 *  one, whose waiter gets `null`. */
const threadWaiters = new Map<number, (status: SearchThreadsStatus | null) => void>();
registerViewHandler((view) => {
  const op = view.engine.search_threads_operation;
  if (!op) return;
  for (const [ticket, resolve] of threadWaiters) {
    if (ticket < op.ticket) { threadWaiters.delete(ticket); resolve(null); }
  }
  const resolve = threadWaiters.get(op.ticket);
  if (resolve && op.status.type !== "Running") { threadWaiters.delete(op.ticket); resolve(op.status); }
});

/** How 搜索线程数 operation `ticket` ended (or asks to go on), `null` once another replaced it. */
function threadSettled(ticket: number): Promise<SearchThreadsStatus | null> {
  const op = state.view?.engine.search_threads_operation;
  if (op && op.ticket > ticket) return Promise.resolve(null);
  if (op?.ticket === ticket && op.status.type !== "Running") return Promise.resolve(op.status);
  const { promise, resolve } = Promise.withResolvers<SearchThreadsStatus | null>();
  threadWaiters.set(ticket, resolve);
  return promise;
}

/** SetKataEngines.threadSource{CFG,BENCHMARK,COMMAND}. */
const THREAD_SOURCES: Record<ThreadRestoreSource, () => string> = {
  CFG: () => t("SetKataEngines.threadSourceCFG"),
  BENCHMARK: () => t("SetKataEngines.threadSourceBENCHMARK"),
  COMMAND: () => t("SetKataEngines.threadSourceCOMMAND"),
};

/** The row's messages the core names by key. */
const THREAD_MESSAGES: Record<string, () => string> = {
  "SetKataEngines.threadStale": () => t("SetKataEngines.threadStale"),
  "SetKataEngines.threadInvalid": () => t("SetKataEngines.threadInvalid"),
  "SetKataEngines.threadFailed": () => t("SetKataEngines.threadFailed"),
  "SetKataEngines.threadReadFailed": () => t("SetKataEngines.threadReadFailed"),
  "SetKataEngines.threadRemoteRestoreUnsupported": () => t("SetKataEngines.threadRemoteRestoreUnsupported"),
  "SetKataEngines.threadRestoreFailed": () => t("SetKataEngines.threadRestoreFailed"),
};

/** showThreadError: the message, then the reason with control characters blanked, at most 240
 *  characters. */
function threadError(key: string, detail: string | null = null): Promise<void> {
  let text = THREAD_MESSAGES[key]?.() ?? t(key);
  const reason = detail?.replace(/[\x00-\x1f\x7f]/g, " ").trim();
  if (reason) text += `\n${reason.slice(0, 240)}`;
  return message(t("SetKataEngines.title"), text, "warning");
}

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

  // 搜索线程数（本次运行） (SetKataEngines.java:491-528, 754-762, 836-888): the numSearchThreads
  // of the KataGo process this window was opened for; another process makes every step stale.
  const threadsNow = view.engine.search_threads;
  const process = threadsNow?.process ?? null;
  const remote = threadsNow?.remote ?? false;
  const threadTooltip = remote ? t("SetKataEngines.threadRemoteTooltip") : t("SetKataEngines.threadLocalTooltip");
  /** The process's override state; another process's is unknown (overrideState of a stale row). */
  const threadState = () => {
    const v = state.view?.engine.search_threads;
    return v && v.process === process ? v.state : "Unknown";
  };
  const threads = document.createElement("span");
  threads.textContent = t("SetKataEngines.lblNumSearchThreads");
  threads.title = threadTooltip;
  const threadOn = threadState() === "On";
  const threadCheck = check(threadOn);
  threadCheck.dataset.key = "kataParams.threadsEdit";
  const threadValue = document.createElement("input");
  threadValue.dataset.key = "kataParams.threads";
  threadValue.disabled = !threadOn;
  threadValue.value = threadsNow?.confirmed?.toString() ?? "";
  threadValue.title = t("SetKataEngines.threadUnknown");
  const threadStatus = document.createElement("span");
  threadStatus.className = "kata-params-thread-status";
  threadStatus.dataset.key = "kataParams.threadsStatus";
  threadStatus.textContent = process === null ? ""
    : threadsNow?.confirmed == null ? t("SetKataEngines.threadReading") : t("SetKataEngines.threadRefreshing");
  let threadEdited = false;
  threadValue.addEventListener("input", () => { threadEdited = true; });
  const threadTooltips = () => {
    const title = process !== null && threadState() === "Unknown" ? t("SetKataEngines.threadProvenanceUnknown") : threadTooltip;
    threadCheck.title = threadValue.title = title;
  };
  threadTooltips();
  // A remote engine's override cannot be unchecked: its source is the server's.
  threadCheck.onchange = () => {
    if (remote && !threadCheck.checked && threadState() === "On") {
      threadCheck.checked = true;
      void threadError("SetKataEngines.threadRemoteRestoreUnsupported");
    }
    threadValue.disabled = !threadCheck.checked;
  };
  const keepThreadOverride = () => {
    if (threadState() === "On") { threadCheck.checked = true; threadValue.disabled = false; }
  };
  grid.append(threads, threadCheck, threadValue, threadStatus);

  const hint = document.createElement("div");
  hint.textContent = t("SetKataEngines.lblHint");
  body.append(grid, hint, hint2());

  // A 搜索线程数 step in progress keeps the window open and its buttons disabled.
  let busy = false;
  let open = true;
  const dialog = formPanel(t("SetKataEngines.title"), body, async () => !busy);
  void dialog.closed.then(() => { open = false; });
  const rows = (): KataEngineParamsEdit => {
    const [pdaEdit, wrnEdit]: KataParamEdit[] = [pda, wrn].map((r) => ({ enabled: r.edit.checked, value: parseJavaDouble(r.value.value), auto_load: r.autoLoad.checked }));
    return { pda: pdaEngine ? null : pdaEdit, wrn: wrnEdit, show_pda: pda.inMenu.checked, show_wrn: wrn.inMenu.checked };
  };
  // applyExistingSettings after the core saved the rows: the toolbar groups take them.
  const finish = async () => {
    if (!pdaEngine) kataToolbarUse.Pda?.(pda.edit.checked, pda.value.value);
    kataToolbarUse.Wrn?.(wrn.edit.checked, wrn.value.value);
    await dialog.close();
  };
  /** One step of the row; resolves to how it ended, `null` when it did not run. */
  const threadStep = async (op: SearchThreadsCommand) => {
    busy = apply.disabled = cancel.disabled = true;
    try {
      const result = await run({ type: "SearchThreads", op });
      return result?.type === "SearchThreads" ? { ticket: result.ticket, status: await threadSettled(result.ticket) } : null;
    } finally {
      busy = apply.disabled = cancel.disabled = false;
    }
  };
  // confirmSettings: the core decides between saving, setting and restoring.
  const apply = button(t("SetKataEngines.btnApply"), async () => {
    if (busy) return;
    if (process === null) {
      // No KataGo process: only the other rows (Java runtimeThreads == null).
      if (threadCheck.checked) { await threadError("SetKataEngines.threadStale"); return; }
      const result = await run({ type: "SaveKataEngineParams", params: rows() });
      if (result && result.type !== "Refused") await finish();
      return;
    }
    const step = await threadStep({ type: "Confirm", process, enabled: threadCheck.checked, text: threadValue.value, edited: threadEdited, params: rows() });
    let status = step?.status ?? null;
    if (step && status?.type === "ConfirmRestore") {
      const recommended = status.recommended === null ? t("SetKataEngines.threadSourceUnknown") : String(status.recommended);
      const warning = format(t("SetKataEngines.threadRestoreConfirm"), THREAD_SOURCES[status.source](), recommended);
      if (!(await confirm(t("SetKataEngines.title"), warning))) {
        threadCheck.checked = true;
        threadValue.disabled = false;
        return;
      }
      status = (await threadStep({ type: "Restore", ticket: step.ticket }))?.status ?? null;
      if (status?.type === "Failed") threadTooltips();
    }
    if (!status || !open) return;
    if (status.type === "Done") { await finish(); return; }
    if (status.type !== "Failed") return;
    if (status.message_key === "SetKataEngines.threadRemoteRestoreUnsupported" || status.message_key === "SetKataEngines.threadRestoreFailed") keepThreadOverride();
    await threadError(status.message_key, status.detail);
    if (status.message_key === "SetKataEngines.threadInvalid") threadValue.focus();
  });
  apply.dataset.key = "kataParams.apply";
  const cancel = button(t("SetKataEngines.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "kataParams.cancel";
  actions(body, apply, cancel);

  // Opening reads the process's value; a value the user typed meanwhile stays.
  if (process !== null) {
    const read = async () => {
      const result = await run({ type: "SearchThreads", op: { type: "Read", process } });
      const status = result?.type === "SearchThreads" ? await threadSettled(result.ticket) : null;
      if (!status || !open) return;
      if (status.type === "Failed" && status.message_key === "SetKataEngines.threadStale") {
        if (!threadEdited) threadValue.value = "";
        threadStatus.textContent = t("SetKataEngines.threadReadFailed");
        threadStatus.title = t("SetKataEngines.threadStale");
      } else if (status.type === "Done") {
        threadStatus.textContent = "";
        threadStatus.title = "";
        if (status.threads !== null && !threadEdited) threadValue.value = String(status.threads);
      } else {
        threadStatus.textContent = t("SetKataEngines.threadReadFailed");
        threadStatus.title = t("SetKataEngines.threadReadFailedHint");
      }
      threadTooltips();
    };
    void read();
  }
  await dialog.closed;
}
