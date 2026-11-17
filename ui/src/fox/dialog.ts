// 野狐 AI 直连 (FoxLiveSession.java:188-301, 394-403, 516-525, 576-641, 978-1009): one modeless
// window, reopened as it was left. Rust owns the listener, the game, the engine and the transcript;
// this window holds only the port and limit drafts and shows the core's Fox view. Closing it does
// not stop the session.
import { apply, registerViewHandler, run, saveRecordAs, state } from "../app";
import { COMMANDS } from "../commands";
import { isApiError, snapshot } from "../contract/client";
import type { FoxCommand, FoxReplaceChoice, FoxStartOutcome, FoxView } from "../contract/generated";
import { message, options } from "../dialogs";
import { format, product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import "./fox.css";

/** FoxLiveSession.ATTENTION_STATUS: statuses that stop automation or need the user's hand. */
const ATTENTION: Record<string, true> = Object.fromEntries([
  "Fox.portBusy", "Fox.invalidPort", "Fox.invalidLimits", "Fox.rulesUnavailable",
  "Fox.engineUnavailable", "Fox.engineError", "Fox.unrestored", "Fox.connectionError",
  "Fox.protocolError", "Fox.disconnected", "Fox.incomplete", "Fox.clockRisk",
  "Fox.confirmationFailed", "Fox.officialAction", "Fox.conflict", "Fox.pdaFailed",
  "Fox.noCheckpoint", "Fox.recoveryMismatch", "Fox.recoveryUnconfirmed",
  "Fox.recoveryEngineChanged",
].map((key) => [key, true]));

function fox(op: FoxCommand) {
  return run({ type: "Fox", op });
}

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** The Fox session holds the foreground engine (FoxLiveSession.ownsEngine): toolbar PDA edits go
 *  to it and 广度 is unavailable. */
export function foxOwnsEngine(): boolean {
  return state.view?.fox.owns_board ?? false;
}

/** FoxLiveSession.submitPda: while the session holds the engine every toolbar PDA edit is its
 *  intent, including drafts it refuses. An unparsable or out-of-range draft is consumed silently;
 *  readiness, dynamic-config and failure statuses come back as the view's PDA status. Resolves to
 *  the current accepted Fox view once the intent was handled (the snapshot read afterwards may be
 *  older than the published view and then does not count), or null when nothing was sent or the
 *  view could not be read. */
export async function submitFoxPda(draft: string): Promise<FoxView | null> {
  const value = parseJavaDouble(draft);
  if (value === null || value < -3 || value > 3) return null;
  if (await fox({ type: "SetPda", value }) === null) return null;
  try {
    const view = await snapshot();
    apply(view);
    return state.view?.fox ?? null;
  } catch (e) {
    await message("Error", isApiError(e) ? e.message : String(e), "error");
    return null;
  }
}

/** Double.parseDouble: surrounding blanks allowed, a signed decimal with an optional exponent and
 *  float / double suffix; NaN and Infinity are refused (Java's limits reject them too). */
function parseJavaDouble(text: string): number | null {
  const s = text.trim();
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[fFdD]?$/.test(s)) return null;
  const v = Number(s.replace(/[fFdD]$/, ""));
  return Number.isFinite(v) ? v : null;
}

/** FoxEngineSession.Limits.parse + its constructor: 0 means unlimited; otherwise a positive time
 *  up to 1e20 and a positive visit count up to 2^50 (Long.parseLong of the trimmed text). */
function parseLimits(time: string, visits: string): { max_time: number; max_visits: number } | null {
  const seconds = parseJavaDouble(time);
  const count = visits.trim();
  if (seconds === null || seconds < 0 || seconds > 1e20 || !/^[+-]?\d+$/.test(count)) return null;
  const n = Number(count);
  if (n < 0 || n > 1125899906842624) return null;
  return { max_time: seconds, max_visits: n };
}

/** The port spinner's formatter: digits only, 1-65535. */
function parsePort(text: string): number | null {
  if (!/^[0-9]+$/.test(text)) return null;
  const n = Number(text);
  return n >= 1 && n <= 65535 ? n : null;
}

/** MessageFormat of an int argument: grouped digits. */
function javaInt(n: number): string {
  return Math.round(n).toLocaleString("en-US");
}

// FoxRulesText.java: a built-in preset name, else each field as the rules dialog words it.
type Preset = [name: string, key: string, ko: string, scoring: string, tax: string, suicide: boolean, hasButton: boolean, whiteHandicapBonus: string, friendlyPassOk: boolean];
/** One entry per semantically distinct KataGo preset (KataGoRules.officialPresets fields). */
const PRESETS: Preset[] = [
  ["chinese", "Fox.rulesPreset.chinese", "SIMPLE", "AREA", "NONE", false, false, "N", true],
  ["chinese-ogs", "Fox.rulesPreset.chineseOgs", "POSITIONAL", "AREA", "NONE", false, false, "N", true],
  ["japanese", "Fox.rulesPreset.japaneseKorean", "SIMPLE", "TERRITORY", "SEKI", false, false, "0", true],
  ["stone-scoring", "Fox.rulesPreset.stoneScoring", "SIMPLE", "AREA", "ALL", false, false, "0", true],
  ["tromp-taylor", "Fox.rulesPreset.trompTaylor", "POSITIONAL", "AREA", "NONE", true, false, "0", false],
  ["aga", "Fox.rulesPreset.agaBga", "SITUATIONAL", "AREA", "NONE", false, false, "N-1", true],
  ["new-zealand", "Fox.rulesPreset.newZealand", "SITUATIONAL", "AREA", "NONE", true, false, "0", true],
  ["aga-button", "Fox.rulesPreset.agaButton", "SITUATIONAL", "AREA", "NONE", false, true, "N-1", true],
  ["goe", "LizzieFrame.currentRules.goe", "POSITIONAL", "AREA", "NONE", true, false, "0", true],
  ["ancient-territory", "Fox.rulesPreset.ancientTerritory", "SIMPLE", "TERRITORY", "ALL", false, false, "0", false],
];

/** The preset's text; literal t() calls keep the keys in the Java string extraction. */
function presetText(key: string): string {
  switch (key) {
    case "Fox.rulesPreset.chinese": return t("Fox.rulesPreset.chinese");
    case "Fox.rulesPreset.chineseOgs": return t("Fox.rulesPreset.chineseOgs");
    case "Fox.rulesPreset.japaneseKorean": return t("Fox.rulesPreset.japaneseKorean");
    case "Fox.rulesPreset.stoneScoring": return t("Fox.rulesPreset.stoneScoring");
    case "Fox.rulesPreset.trompTaylor": return t("Fox.rulesPreset.trompTaylor");
    case "Fox.rulesPreset.agaBga": return t("Fox.rulesPreset.agaBga");
    case "Fox.rulesPreset.newZealand": return t("Fox.rulesPreset.newZealand");
    case "Fox.rulesPreset.agaButton": return t("Fox.rulesPreset.agaButton");
    case "LizzieFrame.currentRules.goe": return t("LizzieFrame.currentRules.goe");
    default: return t("Fox.rulesPreset.ancientTerritory");
  }
}

/** The fields in the rules dialog's order, with its label and value wording. */
function ruleFields(): [string, string, Record<string, string>][] {
  return [
    ["scoring", t("SetKataRules.lblScoringRule"), { AREA: t("SetKataRules.rdoArea"), TERRITORY: t("SetKataRules.rdoTerritory") }],
    ["ko", t("SetKataRules.lblKoRule"), { SIMPLE: t("SetKataRules.rdoSimpleKo"), POSITIONAL: t("SetKataRules.rdoPositionKo"), SITUATIONAL: t("SetKataRules.rdoSituationalKo") }],
    ["suicide", t("SetKataRules.lblMultiStoneSuicide"), { true: t("SetKataRules.rdoSuicide"), false: t("SetKataRules.rdoNoSuicide") }],
    ["tax", t("SetKataRules.lblTaxRule"), { NONE: t("SetKataRules.rdoNoTax"), SEKI: t("SetKataRules.rdoSeKiTax"), ALL: t("SetKataRules.rdoAllTax") }],
    ["whiteHandicapBonus", t("SetKataRules.lblWhiteHandicapBonus"), { 0: t("SetKataRules.rdoNoHandicapKomi"), N: t("SetKataRules.rdoHandicapKomiN"), "N-1": t("SetKataRules.rdoHandicapKomiN1") }],
    ["hasButton", t("SetKataRules.lblHasButton"), { true: t("SetKataRules.rdoButtonGo"), false: t("SetKataRules.rdoNoButtonGo") }],
    ["friendlyPassOk", t("SetKataRules.lblFriendlyPassOk"), { true: t("SetKataRules.rdoButtonGo"), false: t("SetKataRules.rdoNoButtonGo") }],
  ];
}

/** FoxRulesText.describe of the core's rules JSON (`kata-set-rules` argument). */
export function describeRules(json: string): string {
  let parsed: unknown;
  try { parsed = JSON.parse(json); } catch { return json; }
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) return json;
  const fields = new Map(Object.entries(parsed as Record<string, unknown>).filter(([, v]) => v !== null && v !== undefined));
  for (const [, key, ko, scoring, tax, suicide, hasButton, bonus, friendly] of PRESETS) {
    const preset: Record<string, unknown> = { ko, scoring, tax, suicide, hasButton, whiteHandicapBonus: bonus, friendlyPassOk: friendly };
    const names = Object.keys(preset);
    if (fields.size === names.length && names.every((n) => fields.has(n) && String(fields.get(n)) === String(preset[n]))) return presetText(key);
  }
  // Dialog labels carry their own trailing colon; the item format supplies one per locale.
  const label = (text: string) => text.replace(/\\?[:：]\s*$/, "");
  const item = (name: string, value: string) => format(t("Fox.rulesItem"), name, value);
  const items: string[] = [];
  for (const [name, text, values] of ruleFields()) {
    if (!fields.has(name)) continue;
    const value = String(fields.get(name));
    fields.delete(name);
    items.push(item(label(text), values[value] ?? value));
  }
  // Fields the dialog has no wording for keep their KataGo name and value, sorted by name.
  for (const name of [...fields.keys()].sort()) items.push(item(name, String(fields.get(name))));
  return items.join(t("Fox.rulesSeparator"));
}

let win: HTMLElement | null = null;
let statusLine: HTMLElement, statusText: HTMLElement, clockRow: HTMLElement, pdaRow: HTMLElement;
let browsingRow: HTMLElement, browsingText: HTMLElement, returnLive: HTMLButtonElement;
let portField: HTMLInputElement, timeField: HTMLInputElement, visitsField: HTMLInputElement;
let rulesText: HTMLElement, start: HTMLButtonElement, stop: HTMLButtonElement, transcript: HTMLTextAreaElement;
/** The engine's limits last written into the fields; a field still showing it is not a draft. */
let shownTime = "";
let shownVisits = "";
/** A Start attempt is between its first request and its outcome. */
let starting = false;
/** A status this window set for its own drafts (invalid port / limits); shown until the core's
 *  status moves on from `base`. */
let local: { key: string; base: string } | null = null;

function row(...items: HTMLElement[]): HTMLElement {
  const r = el("div", "fox-row");
  r.append(...items);
  return r;
}

function formRow(form: HTMLElement, key: string, field: HTMLInputElement): void {
  const label = el("label", "fox-label", key);
  label.htmlFor = field.id;
  form.append(label, field);
}

function build(): HTMLElement {
  const box = el("div", "modal-box fox-window");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", t("Fox.title"));
  box.dataset.name = "fox.dialog";
  box.tabIndex = -1;
  // An owned JDialog stays usable while WaitForAnalysis disables the main window.
  box.dataset.freeWindow = "";
  const head = el("div", "modal-title fox-title", t("Fox.title"));
  const x = el("button", "fox-close", "×");
  x.setAttribute("aria-label", product("close"));
  x.dataset.key = "fox.close";
  x.onclick = () => { box.hidden = true; };
  head.append(x);

  // Status card: status with its dot, then the clock, PDA and browsing rows when they have text.
  statusText = el("span", "fox-status-text");
  statusLine = row(el("span", "fox-dot"), statusText);
  statusLine.classList.add("fox-status");
  statusLine.dataset.key = "fox.status";
  statusLine.setAttribute("role", "status");
  clockRow = row();
  clockRow.classList.add("fox-detail", "fox-clock");
  clockRow.dataset.key = "fox.clock";
  pdaRow = row();
  pdaRow.classList.add("fox-detail", "fox-muted");
  pdaRow.dataset.key = "fox.pda-status";
  browsingText = el("span", "fox-browsing-text");
  returnLive = el("button", "fox-button", t("Fox.returnLive"));
  returnLive.dataset.key = "fox.return-live";
  returnLive.onclick = () => void fox({ type: "ReturnLive" });
  browsingRow = row(browsingText, returnLive);
  browsingRow.classList.add("fox-detail", "fox-browsing");
  browsingRow.dataset.key = "fox.browsing";
  const card = el("div", "fox-card");
  card.append(statusLine, clockRow, pdaRow, browsingRow);

  // Port, limits, the unlimited note, the inherited rules and the 广度 note.
  const form = el("div", "fox-form");
  portField = el("input", "fox-port");
  portField.type = "number";
  portField.min = "1";
  portField.max = "65535";
  portField.step = "1";
  portField.value = "7000";
  portField.id = "fox-port";
  portField.dataset.key = "fox.port";
  timeField = el("input", "fox-time");
  timeField.id = "fox-max-time";
  timeField.dataset.key = "fox.max-time";
  visitsField = el("input", "fox-visits");
  visitsField.id = "fox-max-visits";
  visitsField.dataset.key = "fox.max-visits";
  formRow(form, t("Fox.port"), portField);
  formRow(form, t("Fox.maxTime"), timeField);
  formRow(form, t("Fox.maxVisits"), visitsField);
  form.append(el("span"), el("span", "fox-muted fox-note", t("Fox.limitUnlimited")));
  const rulesLabel = el("div", "fox-rules-label", t("Fox.rules"));
  rulesText = el("div", "fox-rules");
  rulesText.dataset.key = "fox.rules";
  const breadth = el("div", "fox-muted fox-breadth", t("Fox.breadthUnavailable"));

  start = el("button", "fox-button", t("Fox.start"));
  start.dataset.key = "fox.start";
  start.onclick = () => void startSession();
  stop = el("button", "fox-button", t("Fox.stop"));
  stop.dataset.key = "fox.stop";
  stop.onclick = () => void fox({ type: "Stop" });
  const actions = el("div", "fox-actions");
  actions.append(start, stop);

  const communication = el("div", "fox-muted fox-communication", t("Fox.communication"));
  transcript = el("textarea", "fox-transcript");
  transcript.readOnly = true;
  transcript.wrap = "off";
  transcript.spellcheck = false;
  transcript.dataset.key = "fox.transcript";
  transcript.setAttribute("aria-label", t("Fox.communication"));

  const content = el("div", "fox-content");
  content.append(card, form, rulesLabel, rulesText, breadth, actions, communication, transcript);
  box.append(head, content);
  // The window's own keys stay out of the workbench shortcuts (the JDialog has the focus); like
  // the Java dialog it closes only from its title bar.
  box.addEventListener("keydown", (e) => e.stopPropagation());
  document.body.append(box);
  return box;
}

/** The status and its dot: attention red, live green while listening, else muted. */
function renderStatus(v: FoxView): void {
  if (local && v.status_key !== local.base) local = null;
  const key = local?.key ?? (v.status_key || "Fox.waiting");
  const text = t(key);
  if (statusText.textContent !== text) statusText.textContent = text;
  statusLine.dataset.tone = ATTENTION[key] ? "attention" : v.listening ? "live" : "idle";
  statusLine.dataset.status = key;
}

/** A secondary row takes no space until it carries text (DetailLabel). */
function detail(row: HTMLElement, text: string): void {
  if (row.textContent !== text) row.textContent = text;
  row.hidden = text === "";
}

function render(): void {
  const v = state.view?.fox;
  if (!win || !v) return;
  renderStatus(v);
  const c = v.clock;
  detail(clockRow, c ? format(t("Fox.clock"), javaInt(c.main_seconds), javaInt(c.byo_seconds), javaInt(c.periods), c.remaining_seconds.toFixed(1)) : "");
  const pda = v.pda_status_key === "Fox.pdaState"
    ? (v.desired_pda !== null && v.confirmed_pda !== null ? format(t("Fox.pdaState"), javaDouble(v.desired_pda), javaDouble(v.confirmed_pda)) : "")
    : v.pda_status_key ? t(v.pda_status_key) : "";
  detail(pdaRow, pda);
  const browsing = v.browsing ? t(v.browsing_status_key) : "";
  if (browsingText.textContent !== browsing) browsingText.textContent = browsing;
  browsingRow.hidden = !v.browsing;
  returnLive.disabled = !v.browsing || v.restoring;

  // The engine's current limits replace a field only while it still shows the previous ones, so an
  // edited draft survives a reopen (queryLimits replaceTime / replaceVisits); 0 is unlimited.
  if (v.max_time !== null) {
    const text = v.max_time === 0 ? "0" : javaDouble(v.max_time);
    if (timeField.value === shownTime) timeField.value = text;
    shownTime = text;
  }
  if (v.max_visits !== null) {
    const text = String(v.max_visits);
    if (visitsField.value === shownVisits) visitsField.value = text;
    shownVisits = text;
  }
  // refreshLimitControls: limits editable only before Start with confirmed engine limits.
  const editable = !v.listening && !v.restoring && !v.preparing && !starting && v.max_time !== null && v.max_visits !== null;
  timeField.disabled = !editable;
  visitsField.disabled = !editable;
  portField.disabled = v.listening || starting;

  const rules = v.rules !== null ? describeRules(v.rules) : v.preparing ? t("SetKataRules.status.pending") : t("Fox.rulesUnavailable");
  if (rulesText.textContent !== rules) rulesText.textContent = rules;

  start.disabled = !v.can_start || starting;
  stop.disabled = !v.can_stop;

  const lines = v.transcript.join("\n");
  if (transcript.value !== lines) {
    transcript.value = lines;
    transcript.scrollTop = transcript.scrollHeight;
  }
}

/** One Start request and the outcome the core recorded for it. The outcome stays in the view until
 *  the next Start, so it is read from a snapshot taken after this request was handled. */
async function startStep(op: FoxCommand): Promise<FoxStartOutcome | null> {
  const r = await fox(op);
  if (r === null || r.type === "Refused") return null;
  try {
    const view = await snapshot();
    apply(view);
    return view.fox.outcome;
  } catch (e) {
    await message("Error", isApiError(e) ? e.message : String(e), "error");
    return null;
  }
}

/** FoxLiveSession.start: drafts checked first, then the account warning, then save / replace /
 *  cancel for a record with content (cancel the default). 先保存 saves through 另存为 and ends this
 *  attempt without listening; the user starts again afterwards. */
async function startSession(): Promise<void> {
  const v = state.view;
  if (!v?.fox.can_start || starting) return;
  local = null;
  const limits = parseLimits(timeField.value, visitsField.value);
  if (!limits) { local = { key: "Fox.invalidLimits", base: v.fox.status_key }; render(); return; }
  const port = parsePort(portField.value);
  if (port === null) { local = { key: "Fox.invalidPort", base: v.fox.status_key }; render(); return; }
  starting = true;
  render();
  try {
    let warning = false;
    let replace: FoxReplaceChoice = "Ask";
    let document = v.document.id;
    let revision = v.document.revision;
    for (;;) {
      const outcome = await startStep({ type: "Start", port, ...limits, warning_confirmed: warning, replace, document, revision });
      if (outcome?.type === "NeedsWarning" && !warning) {
        // JOptionPane OK_CANCEL_OPTION, OK the default.
        const ok = await options(t("Fox.title"), t("Fox.confirm"), [[product("ok"), "fox.confirm.ok"], [product("cancel"), "fox.confirm.cancel"]], 0);
        if (ok !== 0) return;
        warning = true;
      } else if (outcome?.type === "NeedsReplacement" && replace === "Ask") {
        const choice = await options(t("Fox.title"), t("Fox.replaceNotice"), [[t("Fox.save"), "Fox.save"], [t("Fox.replace"), "Fox.replace"], [t("Fox.cancel"), "Fox.cancel"]], 2);
        replace = choice === 0 ? "Save" : choice === 1 ? "Replace" : "Cancel";
        document = outcome.document;
        revision = outcome.revision;
      } else {
        if (outcome?.type === "SaveRequested") await saveRecordAs();
        return;
      }
    }
  } finally {
    starting = false;
    render();
  }
}

/** FoxLiveSession.showDialog: show the window as it was left and read the current engine's rules
 *  and limits again. */
export function openFox(): void {
  win ??= build();
  win.hidden = false;
  render();
  win.focus();
  void fox({ type: "Prepare" });
}

COMMANDS["Fox.title"] = { run: openFox };

registerViewHandler(() => render());
