// 引擎参数 (LizzieFrame.setLzSaiEngine, LizzieFrame.java:13109-13121) and its KataGo window
// (SetKataEngines.java); the Leela Zero / Sai window is engine/leelaOptions.ts. The toolbar's
// 激进度 / 广度 groups are in chrome/toolbar.ts.
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run, state } from "../app";
import type { KataEngineParamsEdit, KataParam, KataParamEdit, SearchThreadsCommand, SearchThreadsStatus, ThreadRestoreSource } from "../contract/generated";
import { confirm, formPanel, message } from "../dialogs";
import { format, t } from "../i18n";
import { messageText } from "../errorText";
import { matchActive } from "../match/state";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { button, checkbox, footer, groupNote, input, primaryButton, row, switchBox } from "./controls";
import { openLeelaEngines, paramCaption } from "./leelaOptions";

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

/** The open 引擎参数 window's lock (SetKataEngines.refreshOpenDialogs after a game transition). */
let refreshOpenLock: (() => void) | null = null;
registerViewHandler(() => refreshOpenLock?.());

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
  const reason = detail === null ? "" : messageText(detail).replace(/[\x00-\x1f\x7f]/g, " ").trim();
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

/** The window's 修改 and value of `param` (SetKataEngines.loadCurrentParameter): the value the
 *  engine reported, shown as the toolbar has it when the toolbar's 修改 and value give that value
 *  (matchesEffectiveValue); the saved ones before the engine reported it, or while an engine
 *  match holds the parameter. */
function current(param: KataParam, enabled: boolean, value: number | null): { enabled: boolean; text: string } {
  const reported = matchActive() ? null : (state.view?.engine.kata_params[param === "Pda" ? "pda" : "wrn"] ?? null);
  if (reported === null) return { enabled, text: value === null ? "" : javaDouble(value) };
  const toolbar = kataToolbarState[param]?.();
  if (toolbar && (toolbar.enabled ? parseJavaDouble(toolbar.text) === reported : reported === 0)) return toolbar;
  return { enabled: reported !== 0, text: javaDouble(reported) };
}

/** The fields' red background: Double.parseDouble fails or the value is outside Java's range. */
export function kataValueInvalid(param: KataParam, text: string): boolean {
  const v = parseJavaDouble(text);
  return v === null || (param === "Pda" ? v < -3 || v > 3 : v < 0 || v > 2);
}

/** setLzSaiEngine: no loaded engine → the hint; KataGo → SetKataEngines; Leela Zero / Sai →
 *  SetLeelaEngines. Java opens SetLeelaEngines for every other engine too, whose options would
 *  not apply; those get Java's not-Leela/Sai notice instead. A KataGo an engine match is using
 *  still opens the window, locked (its preferences can be saved). */
export async function openEngineParameters(): Promise<void> {
  const foreground = state.view?.engine.foreground;
  if (!foreground?.ready && !(foreground?.katago && matchActive())) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
  if (foreground.katago) { await openKataEngines(); return; }
  if (foreground.leela_sai) { await openLeelaEngines(); return; }
  await message(t("Message.title"), t("LizzieFrame.setParamsWarning"));
}

/** SetKataEngines.Hint2 (LinkLabel), a note under the group: its red part is drawn as a warning
 *  and its 这里 link opens the KataGo parameter help; the link's own colour is the link style's. */
function hint2(): HTMLElement {
  const el = document.createElement("p");
  el.className = "ds-note kata-params-hint";
  const source = t("SetKataEngines.Hint2").replace(/^<html>|<\/html>$/g, "");
  const parsed = new DOMParser().parseFromString(source, "text/html").body;
  for (const font of parsed.querySelectorAll("font")) {
    const span = document.createElement("span");
    if (font.getAttribute("color") === "red") span.className = "kata-params-warn";
    span.append(...font.childNodes);
    font.replaceWith(span);
  }
  for (const link of parsed.querySelectorAll("a")) {
    const open = document.createElement("button");
    open.type = "button";
    open.className = "ds-link";
    open.title = link.getAttribute("href") ?? "";
    open.append(...link.childNodes);
    open.onclick = async () => {
      try { await invoke("open_katago_parameter_help"); } catch { /* Java ignores a browser that cannot open. */ }
    };
    link.replaceWith(open);
  }
  el.append(...parsed.childNodes);
  return el;
}

/** One SetKataEngines row: 修改, the value, 自动加载 and 在工具栏中控制; `options` is the line that
 *  holds the two check boxes. */
interface Row { el: HTMLElement; options: HTMLElement; edit: HTMLInputElement; value: HTMLInputElement; autoLoad: HTMLInputElement; inMenu: HTMLInputElement }
/** SetKataEngines.draft: a row's 修改 and value, and its 自动加载 and value, as text. */
interface Draft { value: string; auto: string }
function draft(r: Row): Draft {
  return { value: `${r.edit.checked} ${r.value.value}`, auto: `${r.autoLoad.checked} ${r.value.value}` };
}

/** A parameter's row: its name with Java's range hint under it and 自动加载 / 在工具栏中控制 on a
 *  line under the hint; the value and 修改 (a switch) at the end, so every row of the group ends
 *  on the same line. */
function paramRow(host: HTMLElement, param: KataParam, label: string, enabled: boolean, text: string, autoLoad: boolean, inMenu: boolean): Row {
  const [name, hint] = paramCaption(label);
  const auto = checkbox(t("SetKataEngines.lblAutoLoad"), autoLoad);
  const menu = checkbox(t("SetKataEngines.lblShowInMenu"), inMenu);
  const edit = switchBox(enabled), value = input(text);
  value.dataset.key = `kataParams.${param.toLowerCase()}`;
  edit.dataset.key = `kataParams.${param.toLowerCase()}Edit`;
  auto.input.dataset.key = `kataParams.${param.toLowerCase()}AutoLoad`;
  menu.input.dataset.key = `kataParams.${param.toLowerCase()}InMenu`;
  value.inputMode = "decimal";
  value.setAttribute("aria-label", name);
  edit.setAttribute("aria-label", name);
  const el = row(host, name, hint, value, edit);
  const options = document.createElement("span");
  options.className = "engine-param-options";
  options.append(auto.label, menu.label);
  el.firstElementChild!.append(options);
  const r: Row = { el, options, edit, value, autoLoad: auto.input, inMenu: menu.input };
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
  return r;
}

async function openKataEngines(): Promise<void> {
  const view = state.view;
  if (!view) return;
  const s = view.settings.engine;
  const p = s.kata_params;
  const pdaEngine = kataGoPda();
  const body = document.createElement("div");
  body.className = "engine-params";

  const pdaNow = current("Pda", s.kata_pda_enabled, s.kata_pda);
  const pda = paramRow(body, "Pda", t("SetKataEngines.lblPDA"), pdaNow.enabled, pdaNow.text, p.autoLoadPda, p.showPda);
  if (pdaEngine) {
    // SetKataEngines.java:591-605: a KataGoPda engine's PDA is set from the toolbar's own panel;
    // the row shows that instead of its controls and check boxes.
    const note = document.createElement("span");
    note.className = "kata-params-pda-engine";
    note.textContent = t("SetKataEngines.lblPdaEngineUseMenu");
    pda.el.lastElementChild!.replaceChildren(note);
    pda.options.remove();
  }
  const wrnNow = current("Wrn", p.wrnEnabled, p.wrn);
  const wrn = paramRow(body, "Wrn", t("SetKataEngines.lblWRN"), wrnNow.enabled, wrnNow.text, p.autoLoadWrn, p.showWrn);
  // SetKataEngines.refreshParameterEditors: until an engine match gave its parameters back
  // (EngineGameModule.ordinaryParametersHidden) both rows are locked, with
  // EngineGameParameterControls.explainAdvancedLock's words; a genmove game also holds 广度
  // (SetKataEngines.java:586-589). Java puts the words in each control's tooltip only, which a
  // disabled control does not show here, so they are also a line under the rows.
  const lockNote = groupNote(body, t("EngineGameParameters.useGamePanel"));
  lockNote.dataset.key = "kataParams.useGamePanel";
  const lock = () => {
    const v = state.view;
    const engineGame = matchActive(v);
    const wrnOwned = engineGame || v?.game?.kind === "Genmove";
    const tip = engineGame ? t("EngineGameParameters.useGamePanel") : "";
    for (const [r, owned] of [[pda, engineGame], [wrn, wrnOwned]] as const) {
      if (r.edit.disabled !== owned) r.edit.disabled = owned;
      const value = owned || !r.edit.checked;
      if (r.value.disabled !== value) r.value.disabled = value;
      for (const c of [r.edit, r.value]) if (c.title !== tip) c.title = tip;
      // A locked row cannot be corrected, so it is not marked invalid (an unsaved value is empty).
      const invalid = !owned && kataValueInvalid(r === pda ? "Pda" : "Wrn", r.value.value);
      if (r.value.classList.contains("invalid") !== invalid) r.value.classList.toggle("invalid", invalid);
    }
    if (lockNote.hidden === engineGame) lockNote.hidden = !engineGame;
  };
  lock();
  for (const r of [pda, wrn]) r.edit.addEventListener("change", lock);

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
  const [threadName, threadHint] = paramCaption(t("SetKataEngines.lblNumSearchThreads"));
  const threads = document.createElement("span");
  threads.textContent = threadName;
  threads.title = threadTooltip;
  const threadOn = threadState() === "On";
  const threadCheck = switchBox(threadOn);
  threadCheck.dataset.key = "kataParams.threadsEdit";
  threadCheck.setAttribute("aria-label", threadName);
  const threadValue = input("");
  threadValue.inputMode = "numeric";
  threadValue.setAttribute("aria-label", threadName);
  threadValue.dataset.key = "kataParams.threads";
  threadValue.disabled = !threadOn;
  threadValue.value = threadsNow?.confirmed?.toString() ?? "";
  threadValue.title = t("SetKataEngines.threadUnknown");
  // The read status is a line under the row's hint; it is gone while there is nothing to say.
  const threadStatus = document.createElement("span");
  threadStatus.className = "ds-row-sub kata-params-thread-status";
  threadStatus.setAttribute("role", "status");
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
  // The value and the override switch at the end of the row, like the parameters above.
  row(body, threads, threadHint, threadValue, threadCheck).firstElementChild!.append(threadStatus);
  groupNote(body, t("SetKataEngines.lblHint"));
  body.append(hint2());

  // A 搜索线程数 step in progress keeps the window open and its buttons disabled.
  let busy = false;
  let open = true;
  const dialog = formPanel(t("SetKataEngines.title"), body, async () => !busy);
  refreshOpenLock = lock;
  void dialog.closed.then(() => {
    open = false;
    refreshOpenLock = null;
  });
  // applyExistingSettings saves only what changed since the window loaded it (loadedPda,
  // loadedAutoPda, …), so an unchanged copy never overwrites newer engine or saved values; a
  // checked 广度 or 自动加载 whose text is not a number in 0..2 is not taken
  // (OrdinaryBreadth.validDraft). While an engine match holds the parameters only the
  // preferences are saved.
  const drafts = (): Record<KataParam, Draft> => ({ Pda: draft(pda), Wrn: draft(wrn) });
  const loaded = drafts();
  const edits = (param: KataParam, r: Row): KataParamEdit => {
    const now = draft(r);
    const valid = param === "Pda" || !kataValueInvalid("Wrn", r.value.value);
    return {
      enabled: r.edit.checked,
      value: parseJavaDouble(r.value.value),
      auto_load: r.autoLoad.checked,
      edited: !matchActive() && now.value !== loaded[param].value && (!r.edit.checked || valid),
      auto_load_edited: now.auto !== loaded[param].auto && (!r.autoLoad.checked || valid),
    };
  };
  const rows = (): KataEngineParamsEdit => ({
    pda: pdaEngine ? null : edits("Pda", pda),
    wrn: edits("Wrn", wrn),
    show_pda: pda.inMenu.checked,
    show_wrn: wrn.inMenu.checked,
  });
  // applyExistingSettings reads the rows once the 搜索线程数 step ended (SetKataEngines.java:653-660):
  // rows edited while it ran are saved again. A toolbar group takes a row only when the engine
  // got it (setUseGfPda / setUseWrn).
  const finish = async (committed: KataEngineParamsEdit, at: Record<KataParam, Draft>) => {
    for (const [param, edit] of [["Pda", committed.pda], ["Wrn", committed.wrn]] as const) {
      if (edit?.edited) loaded[param].value = at[param].value;
      if (edit?.auto_load_edited) loaded[param].auto = at[param].auto;
    }
    const now = rows();
    const shown = { pda: [pda.edit.checked, pda.value.value] as const, wrn: [wrn.edit.checked, wrn.value.value] as const };
    const again = [now.pda, now.wrn].some((e) => e?.edited || e?.auto_load_edited);
    if (again || now.show_pda !== committed.show_pda || now.show_wrn !== committed.show_wrn) {
      const result = await run({ type: "SaveKataEngineParams", params: now });
      if (!result || result.type === "Refused") return;
    }
    if (!pdaEngine && (committed.pda?.edited || now.pda?.edited)) kataToolbarUse.Pda?.(...shown.pda);
    if (committed.wrn.edited || now.wrn.edited) kataToolbarUse.Wrn?.(...shown.wrn);
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
  const apply = primaryButton(t("SetKataEngines.btnApply"), async () => {
    if (busy) return;
    const params = rows();
    const at = drafts();
    if (process === null) {
      // No KataGo process: only the other rows (Java runtimeThreads == null).
      if (threadCheck.checked) { await threadError("SetKataEngines.threadStale"); return; }
      const result = await run({ type: "SaveKataEngineParams", params });
      if (result && result.type !== "Refused") await finish(params, at);
      return;
    }
    const step = await threadStep({ type: "Confirm", process, enabled: threadCheck.checked, text: threadValue.value, edited: threadEdited, params });
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
    if (status.type === "Done") { await finish(params, at); return; }
    if (status.type !== "Failed") return;
    if (status.message_key === "SetKataEngines.threadRemoteRestoreUnsupported" || status.message_key === "SetKataEngines.threadRestoreFailed") keepThreadOverride();
    await threadError(status.message_key, status.detail);
    if (status.message_key === "SetKataEngines.threadInvalid") threadValue.focus();
  });
  apply.dataset.key = "kataParams.apply";
  const cancel = button(t("SetKataEngines.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "kataParams.cancel";
  footer(body, { primary: apply, cancel });

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
