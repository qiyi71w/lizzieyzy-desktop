// WholeGameAnalysisDialog: modeless, never disables the board. Only the draft/visibility lives
// here; Rust owns the session, progress, plan and engine handback.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS } from "../commands";
import { dispatch } from "../contract/client";
import type { WholeGameAnalysisView, WholeGameState } from "../contract/generated";
import { message } from "../dialogs";
import { boardUi, togglePolicy } from "../display/state";
import { format, t } from "../i18n";
import "./wholeGame.css";

let win: HTMLElement | null = null;
let preset: HTMLSelectElement, custom: HTMLInputElement, hint: HTMLElement;
let phase: HTMLElement, progress: HTMLProgressElement, percent: HTMLElement, counts: HTMLElement;
let mode: HTMLElement, remaining: HTMLElement, start: HTMLButtonElement, pause: HTMLButtonElement, stop: HTMLButtonElement;
let draft = true;
let completedRun = 0;
let saveNoticeRun = 0;
let estimateGeneration = 0;
const active = (s: WholeGameState) => ["Preparing", "Baseline", "Deep", "Pausing", "Paused"].includes(s);
const terminal = (s: WholeGameState) => ["Complete", "Cancelled", "Failed"].includes(s);

export function formatWholeGameDuration(ms: number): string {
  const seconds = Math.max(0, Math.ceil(ms / 1000));
  const pad = (n: number) => String(n).padStart(2, "0");
  const hours = Math.floor(seconds / 3600);
  return hours > 0 ? `${hours}:${pad(Math.floor(seconds / 60) % 60)}:${pad(seconds % 60)}` : `${pad(Math.floor(seconds / 60))}:${pad(seconds % 60)}`;
}
function keyText(key: string): string {
  const texts: Record<string, string> = {
    "WholeGameAnalysis.ready": t("WholeGameAnalysis.ready"),
    "WholeGameAnalysis.preparing": t("WholeGameAnalysis.preparing"),
    "WholeGameAnalysis.baseline": t("WholeGameAnalysis.baseline"),
    "WholeGameAnalysis.deep": t("WholeGameAnalysis.deep"),
    "WholeGameAnalysis.pausing": t("WholeGameAnalysis.pausing"),
    "WholeGameAnalysis.paused": t("WholeGameAnalysis.paused"),
    "WholeGameAnalysis.complete": t("WholeGameAnalysis.complete"),
    "WholeGameAnalysis.cancelled": t("WholeGameAnalysis.cancelled"),
    "WholeGameAnalysis.error.engine": t("WholeGameAnalysis.error.engine"),
    "WholeGameAnalysis.error.request": t("WholeGameAnalysis.error.request"),
    "WholeGameAnalysis.error.gameChanged": t("WholeGameAnalysis.error.gameChanged"),
    "WholeGameAnalysis.mode.local": t("WholeGameAnalysis.mode.local"),
    "WholeGameAnalysis.mode.localShared": t("WholeGameAnalysis.mode.localShared"),
    "WholeGameAnalysis.noGame": t("WholeGameAnalysis.noGame"),
    "WholeGameAnalysis.conflict": t("WholeGameAnalysis.conflict"),
    "WholeGameAnalysis.conflict.analysis": t("WholeGameAnalysis.conflict.analysis"),
    "WholeGameAnalysis.visits.invalid": t("WholeGameAnalysis.visits.invalid"),
  };
  return texts[key] ?? t(key);
}
function notice(text: string, ms: number): void {
  const el = document.createElement("div"); el.className = "whole-game-notice"; el.role = "status"; el.textContent = text;
  document.body.append(el); window.setTimeout(() => el.remove(), ms);
}
function visits(): number { return preset.value === "custom" ? Number(custom.value) : Number(preset.value); }
function valid(): boolean { const n = visits(); return Number.isInteger(n) && n >= 500 && n <= 1_000_000; }
async function updateHint(): Promise<void> {
  const generation = ++estimateGeneration;
  custom.hidden = preset.value !== "custom";
  hint.classList.toggle("invalid", !valid());
  hint.textContent = valid() ? t("WholeGameAnalysis.visits.hint") : t("WholeGameAnalysis.visits.invalid");
  if (!valid() || !draft) return;
  const result = await dispatch({ type: "EstimateWholeGameAnalysis", visits: visits() });
  if (generation !== estimateGeneration || !draft) return;
  if (result.type === "WholeGameEstimate" && result.millis !== null) hint.textContent = format(t("WholeGameAnalysis.visits.estimate"), formatWholeGameDuration(result.millis));
}
function hide(): void { if (win) win.hidden = true; }
function build(): HTMLElement {
  const box = document.createElement("section"); box.className = "whole-game-window"; box.hidden = true;
  box.role = "dialog"; box.setAttribute("aria-label", t("WholeGameAnalysis.title")); box.dataset.freeWindow = "";
  const close = document.createElement("button"); close.className = "whole-game-close"; close.textContent = "×";
  close.dataset.key = "wholeGame.hide"; close.setAttribute("aria-label", t("WholeGameAnalysis.hide")); close.onclick = hide;
  const header = document.createElement("header");
  const icon = document.createElement("span"); icon.className = "whole-game-icon"; icon.textContent = "●";
  const text = document.createElement("div"), title = document.createElement("h2"), description = document.createElement("p");
  title.textContent = t("WholeGameAnalysis.title"); description.textContent = t("WholeGameAnalysis.description"); text.append(title, description); header.append(icon, text);
  const card = document.createElement("div"); card.className = "whole-game-card";
  const controls = document.createElement("div"); controls.className = "whole-game-visits";
  const label = document.createElement("label"); label.textContent = t("WholeGameAnalysis.visits.label"); label.htmlFor = "wholeGamePreset";
  preset = document.createElement("select"); preset.id = "wholeGamePreset"; preset.dataset.key = "wholeGame.visitsPreset";
  const options: [string,string][] = [["500", t("WholeGameAnalysis.visits.quick")], ["1000", t("WholeGameAnalysis.visits.standard")], ["3000", t("WholeGameAnalysis.visits.deep")], ["10000", t("WholeGameAnalysis.visits.professional")], ["custom", t("WholeGameAnalysis.visits.custom")]];
  for (const [value,text] of options) { const o = document.createElement("option"); o.value = value; o.textContent = text; preset.append(o); }
  custom = document.createElement("input"); custom.type = "number"; custom.min = "500"; custom.max = "1000000"; custom.step = "100";
  custom.dataset.key = "wholeGame.customVisits"; custom.setAttribute("aria-label", t("WholeGameAnalysis.visits.customValue"));
  hint = document.createElement("p"); hint.dataset.key = "wholeGame.visitsHint";
  controls.append(label, preset, custom, hint); preset.onchange = () => void updateHint(); custom.oninput = () => void updateHint();
  phase = document.createElement("h3"); phase.dataset.key = "wholeGame.phase";
  const bar = document.createElement("div"); bar.className = "whole-game-bar";
  progress = document.createElement("progress"); progress.max = 100; progress.dataset.key = "wholeGame.progress";
  percent = document.createElement("span"); bar.append(progress, percent);
  counts = document.createElement("p"); counts.dataset.key = "wholeGame.counts";
  const metadata = document.createElement("div"); metadata.className = "whole-game-metadata";
  mode = document.createElement("span"); remaining = document.createElement("span"); mode.dataset.key = "wholeGame.mode"; remaining.dataset.key = "wholeGame.remaining";
  metadata.append(mode, remaining); card.append(controls, phase, bar, counts, metadata);
  const actions = document.createElement("footer");
  const button = (key: string) => { const b = document.createElement("button"); b.dataset.key = `wholeGame.${key}`; actions.append(b); return b; };
  start = button("start"); pause = button("pause"); stop = button("stop");
  start.textContent = t("WholeGameAnalysis.start");
  start.onclick = async () => {
    if (!valid()) { await updateHint(); custom.focus(); return; }
    draft = false;
    const result = await run({ type: "StartWholeGameAnalysis", visits: visits() });
    if (!result || result.type === "Refused" || result.type === "Unchanged") { draft = true; render(state.view?.whole_game); return; }
    // ResultView.activate resets only transient modes once; later user toggles remain effective.
    if (boardUi.policy) togglePolicy();
    if (state.view?.rawNet) await run({ type: "StopRawNet" });
  };
  pause.onclick = () => void run({ type: state.view?.whole_game.state === "Paused" ? "ResumeWholeGameAnalysis" : "PauseWholeGameAnalysis" });
  stop.onclick = () => { const s = state.view?.whole_game.state; if (s && terminal(s)) hide(); else void run({ type: "StopWholeGameAnalysis" }); };
  box.onkeydown = (e) => {
    e.stopPropagation();
    if (e.key === "Escape") { e.preventDefault(); hide(); }
    if (e.key === "Enter" && !(e.target instanceof HTMLButtonElement)) {
      e.preventDefault(); (start.disabled ? pause.disabled ? stop.disabled ? null : stop : pause : start)?.click();
    }
  };
  box.append(close, header, card, actions); document.body.append(box); return box;
}
function render(v?: WholeGameAnalysisView): void {
  if (!win || !v) return;
  const s = draft ? "Idle" : v.state;
  const idle = s === "Idle", done = terminal(s), running = ["Preparing", "Baseline", "Deep"].includes(s);
  preset.disabled = !idle; custom.disabled = !idle || preset.value !== "custom";
  start.disabled = !idle; pause.disabled = !(running || s === "Paused"); stop.disabled = idle;
  pause.textContent = s === "Paused" ? t("WholeGameAnalysis.resume") : t("WholeGameAnalysis.pause");
  stop.textContent = done ? t("WholeGameAnalysis.close") : t("WholeGameAnalysis.cancel");
  start.classList.toggle("primary", idle); pause.classList.toggle("primary", s === "Paused"); stop.classList.toggle("primary", done);
  phase.textContent = idle ? t("WholeGameAnalysis.ready") : keyText(v.detail_key);
  progress.value = idle ? 0 : v.percent; percent.textContent = `${progress.value}%`;
  counts.textContent = format(t("WholeGameAnalysis.progress"), idle ? "0" : v.completed.toLocaleString("en-US"), idle ? "0" : v.total.toLocaleString("en-US"), idle ? "0" : v.target_visits.toLocaleString("en-US"));
  mode.textContent = idle || s === "Preparing" || !v.mode_key ? "" : keyText(v.mode_key);
  remaining.textContent = running ? v.remaining_ms === null ? t("WholeGameAnalysis.remaining.calculating") : format(t("WholeGameAnalysis.remaining"), formatWholeGameDuration(v.remaining_ms)) : "";
}
export async function openWholeGameAnalysis(): Promise<void> {
  const view = state.view; if (!view) return;
  const existing = active(view.whole_game.state);
  if (!existing) {
    let n = state.nodes.get(state.root), hasGame = false;
    while (n) { if (n.kind === "Black" || n.kind === "White") { hasGame = true; break; } n = n.children[0] === undefined ? undefined : state.nodes.get(n.children[0]); }
    if (!hasGame) { await message(t("Message.title"), t("WholeGameAnalysis.noGame")); return; }
  }
  win ??= build();
  draft = !existing;
  const n = existing ? view.whole_game.visits : view.settings.whole_game.deepVisits;
  custom.value = String(n); preset.value = [500,1000,3000,10000].includes(n) ? String(n) : "custom";
  win.hidden = false; render(view.whole_game); if (draft) { void updateHint(); start.focus(); }
}
COMMANDS["whole-game-deep-analysis"] = { run: () => void openWholeGameAnalysis() };
registerViewHandler((view) => {
  const v = view.whole_game;
  if (active(v.state)) draft = false;
  if (v.save_failed && v.run !== saveNoticeRun) { saveNoticeRun = v.run; notice(t("WholeGameAnalysis.visits.saveFailed"), 4000); }
  if (v.state === "Complete" && v.run !== completedRun) { completedRun = v.run; hide(); notice(t("WholeGameAnalysis.resultsShown"), 5000); }
  render(v);
});
