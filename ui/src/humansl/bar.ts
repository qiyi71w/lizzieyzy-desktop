// The in-game training bar (HumanSlTrainingBar.java): 对手 and the opponent, 我方 / 对手 clocks
// around the turn status with the thin thinking line below, and 停一手 / 认输 / 重试 AI /
// 结束并复盘, as capsules on the bar ground like the bottom bar. While KataGo counts the game the
// status says so, and after the game it states the result (CoachPanelState, resultText). It sits
// between the workbench and the bottom toolbar (LizzieFrame.layoutMainContent) while a game is on;
// when the window is too narrow for one line, the capsules go on to a second.
import { run } from "../app";
import type { HumanSlCommand, HumanSlResult, HumanSlView } from "../contract/generated";
import { format, t } from "../i18n";
import { coachLevel, humanSlGameActive, humanSlSession, humanSlUi, levelFox } from "./state";
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

/** HumanSlGameController.resultText: how the game ended, in words. */
function resultText(r: HumanSlResult): string {
  switch (r.type) {
    case "Uncounted":
      return t("HumanSlGame.result.uncounted");
    case "Resigned":
      return r.winner === "Black" ? t("HumanSlGame.result.blackByResignation") : t("HumanSlGame.result.whiteByResignation");
    case "Counted": {
      const v = r.verdict;
      if (v.type === "Draw") return t("HumanSlGame.draw");
      if (v.type === "DrawToFirstPasser") return v.winner === "Black" ? t("HumanSlGame.result.blackWinsDraw") : t("HumanSlGame.result.whiteWinsDraw");
      return format(v.winner === "Black" ? t("HumanSlGame.result.blackByPoints") : t("HumanSlGame.result.whiteByPoints"), v.margin);
    }
  }
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
  const caption = el("div", "hs-bar-caption");
  const opponent = el("div", "hs-bar-opponent");
  opponent.dataset.key = "humansl.bar.opponent";
  identity.append(caption, opponent);

  const statusPanel = el("div", "hs-bar-status hs-cap");
  const humanClock = el("span", "hs-bar-clock");
  humanClock.dataset.key = "humansl.bar.humanClock";
  const turn = el("span", "hs-bar-turn");
  turn.dataset.key = "humansl.bar.turn";
  const aiClock = el("span", "hs-bar-clock");
  aiClock.dataset.key = "humansl.bar.aiClock";
  const thinking = el("div", "hs-bar-thinking");
  thinking.dataset.key = "humansl.bar.thinking";
  thinking.append(el("div", "hs-bar-thinking-run"));
  statusPanel.append(humanClock, turn, aiClock, thinking);

  const actions = el("div", "hs-bar-actions hs-cap");
  const make = (name: string, style: string, onClick: () => void) => {
    const b = el("button", `hs-seg ${style}`);
    b.type = "button";
    b.dataset.key = `humansl.bar.${name}`;
    b.onclick = onClick;
    actions.append(b);
    return b;
  };
  // The labels are set when the bar is painted, in the interface language of then. The bar paints
  // with every view and every 500 ms; replacing a label's text node between the press and the
  // release of its button cancels that click, so a label changes only with its text.
  const label = (b: HTMLButtonElement, text: string) => {
    if (b.textContent !== text) b.textContent = text;
  };
  const pass = make("pass", "", () => send({ type: "Pass" }));
  const resign = make("resign", "", () => send({ type: "Resign" }));
  const retry = make("retryAi", "", () => send({ type: "RetryAi" }));
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
    caption.textContent = t("HumanSlTraining.bar.opponent");
    // HumanSlGameController.opponentLabel: the opponent's Fox / Tygem rank.
    const level = coachLevel(s.level);
    opponent.textContent = level ? levelFox(level) : "";
    const key = `${s.id}:${s.human_ms}:${s.ai_ms}:${s.clock}`;
    if (key !== base.key) base = { key, at: performance.now() };
    const live = performance.now() - base.at;
    humanClock.textContent = `${t("HumanSlTraining.bar.you")}  ${clock(s.human_ms + (s.clock === "Human" ? live : 0))}`;
    aiClock.textContent = `${t("HumanSlTraining.bar.ai")}  ${clock(s.ai_ms + (s.clock === "Ai" ? live : 0))}`;
    const stopping = s.phase === "Stopping";
    const cleanup = s.phase === "CleanupPending";
    const over = s.result !== null;
    turn.textContent = cleanup ? t("HumanSlTraining.bar.cleanupRetry")
      : stopping ? t("HumanSlTraining.bar.stopping")
      : s.result ? `${t("CoachPanel.status.gameOver")} · ${resultText(s.result)}`
      : s.counting ? t("CoachPanel.status.counting")
      : s.ai_failed ? t("HumanSlTraining.bar.aiFailed")
      : s.ai_thinking ? t("HumanSlTraining.bar.aiThinking")
      : t("HumanSlTraining.bar.yourTurn");
    thinking.classList.toggle("on", s.ai_thinking || s.counting || stopping);
    label(pass, t("HumanSlGame.btn.pass"));
    label(resign, t("HumanSlGame.btn.resign"));
    label(retry, t("HumanSlTraining.retryAi"));
    // After the game its buttons are done (CoachPanel: the grid hides); 结束并复盘 stays.
    pass.hidden = resign.hidden = over;
    retry.hidden = !(s.phase === "Playing" && s.ai_failed);
    pass.disabled = !(s.phase === "Playing" && s.human_turn);
    // canResign: also while the AI thinks, not while KataGo counts.
    resign.disabled = !(s.phase === "Playing" && !s.counting && !over);
    label(finish, cleanup ? t("HumanSlTraining.retryCleanup") : stopping ? t("HumanSlTraining.stopping") : t("HumanSlTraining.finishReview"));
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

