// The in-game training bar (HumanSlTrainingBar.java): 对手 and the opponent, 我方 / 对手 clocks
// around the turn status with the thin thinking bar below, and 停一手 / 重试 AI / 结束并复盘. It sits
// between the workbench and the bottom toolbar (LizzieFrame.layoutMainContent) while a game is on.
import { run } from "../app";
import type { HumanSlCommand, HumanSlView } from "../contract/generated";
import { t } from "../i18n";
import { humanSlGameActive, humanSlSession, humanSlUi, opponentLabel } from "./state";
import "./humansl.css";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function send(op: HumanSlCommand): void {
  void run({ type: "HumanSl", op });
}

/** HumanSlTrainingBar.format: MM:SS. */
function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

let bar: HTMLElement | null = null;
let refresh: (() => void) | null = null;

/** Builds the bar before the bottom toolbar; it stays hidden without a game. */
export function mountTrainingBar(): void {
  const host = el("div", "hs-bar");
  host.id = "humanslBar";
  host.dataset.name = "humanSlTrainingBar";
  host.hidden = true;

  const identity = el("div", "hs-bar-identity");
  const opponent = el("div", "hs-bar-opponent");
  opponent.dataset.key = "humansl.bar.opponent";
  identity.append(el("div", "hs-bar-caption", t("HumanSlTraining.bar.opponent")), opponent);

  const statusPanel = el("div", "hs-bar-status");
  const row = el("div", "hs-bar-row");
  const humanClock = el("span", "hs-bar-clock");
  humanClock.dataset.key = "humansl.bar.humanClock";
  const turn = el("span", "hs-bar-turn");
  turn.dataset.key = "humansl.bar.turn";
  const aiClock = el("span", "hs-bar-clock");
  aiClock.dataset.key = "humansl.bar.aiClock";
  row.append(humanClock, turn, aiClock);
  const thinking = el("div", "hs-bar-thinking");
  thinking.dataset.key = "humansl.bar.thinking";
  thinking.append(el("div", "hs-bar-thinking-run"));
  statusPanel.append(row, thinking);

  const actions = el("div", "hs-bar-actions");
  const make = (name: string, style: string, onClick: () => void) => {
    const b = el("button", `hs-button ${style}`);
    b.type = "button";
    b.dataset.key = `humansl.bar.${name}`;
    b.onclick = onClick;
    actions.append(b);
    return b;
  };
  const pass = make("pass", "secondary", () => send({ type: "Pass" }));
  pass.textContent = t("HumanSlGame.btn.pass");
  const retry = make("retryAi", "primary", () => send({ type: "RetryAi" }));
  retry.textContent = t("HumanSlTraining.retryAi");
  // 重试清理 runs the exit again (Finish); a stop in progress is never clicked (disabled).
  const finish = make("finish", "danger", () => {
    const phase = humanSlSession()?.phase;
    send({ type: phase === "Stopping" ? "Abort" : "Finish" });
  });

  host.append(identity, statusPanel, actions);
  document.getElementById("bottombar")!.before(host);
  bar = host;

  /** The clocks' bases: the totals of the last view and when they arrived. */
  let base = { key: "", at: performance.now() };
  let ticker: number | undefined;

  const paint = (s: HumanSlView) => {
    opponent.textContent = opponentLabel(s.opponent, s.dan_rank, s.rank);
    const key = `${s.id}:${s.human_ms}:${s.ai_ms}:${s.clock}`;
    if (key !== base.key) base = { key, at: performance.now() };
    const live = performance.now() - base.at;
    humanClock.textContent = `${t("HumanSlTraining.bar.you")}  ${clock(s.human_ms + (s.clock === "Human" ? live : 0))}`;
    aiClock.textContent = `${t("HumanSlTraining.bar.ai")}  ${clock(s.ai_ms + (s.clock === "Ai" ? live : 0))}`;
    const stopping = s.phase === "Stopping";
    const cleanup = s.phase === "CleanupPending";
    turn.textContent = cleanup ? t("HumanSlTraining.bar.cleanupRetry")
      : stopping ? t("HumanSlTraining.bar.stopping")
      : s.ai_failed ? t("HumanSlTraining.bar.aiFailed")
      : s.ai_thinking ? t("HumanSlTraining.bar.aiThinking")
      : t("HumanSlTraining.bar.yourTurn");
    thinking.classList.toggle("on", s.ai_thinking || stopping);
    retry.hidden = !(s.phase === "Playing" && s.ai_failed);
    pass.disabled = !(s.phase === "Playing" && s.human_turn);
    finish.textContent = cleanup ? t("HumanSlTraining.retryCleanup") : stopping ? t("HumanSlTraining.stopping") : t("HumanSlTraining.finishReview");
    finish.disabled = stopping;
  };

  refresh = () => {
    // Java attaches the bar only once a game started (showHumanSlTrainingBar after the handoff).
    const active = humanSlGameActive() && !humanSlUi.dialogOwnsSession;
    if (host.hidden === active) host.hidden = !active;
    const s = humanSlSession();
    if (!active || !s) {
      window.clearInterval(ticker);
      ticker = undefined;
      return;
    }
    paint(s);
    // refreshTimer: the clocks tick every 500 ms.
    ticker ??= window.setInterval(() => {
      const now = humanSlSession();
      if (now && humanSlGameActive()) paint(now);
    }, 500);
  };
  humanSlUi.focusBar = () => {
    if (!host.hidden) (finish.disabled ? host.querySelector<HTMLButtonElement>("button:not(:disabled)") : finish)?.focus();
  };
}

/** Shows, updates or hides the bar for the latest view; true when its visibility changed. */
export function refreshTrainingBar(): boolean {
  if (!bar) return false;
  const shown = !bar.hidden;
  refresh?.();
  return shown === bar.hidden;
}

let notices = 0;

/** Utils.showMsgNoModalForTime: the Message window, closed after `seconds`. */
export function timedMessage(text: string, seconds: number): void {
  const box = el("div", "modal-box hs-notice");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", t("Message.title"));
  box.dataset.freeWindow = "";
  box.dataset.name = "humansl.notice";
  box.style.translate = `${(notices % 5) * 16}px ${(notices % 5) * 16}px`;
  notices++;
  box.append(el("div", "modal-title", t("Message.title")), el("div", "modal-body", text));
  document.body.append(box);
  window.setTimeout(() => box.remove(), seconds * 1000);
}
