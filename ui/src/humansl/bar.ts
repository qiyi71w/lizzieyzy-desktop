// The in-game training bar (HumanSlTrainingBar.java): 对手 and the opponent, 我方 / 对手 clocks
// around the turn status with the thin thinking line below, and 提示 / 悔棋 / 停一手 / 认输 /
// 重试 AI / 结束并复盘, as capsules on the bar ground like the bottom bar (CoachPanel's 提示 / 悔棋 /
// 停一手 / 认输 order). While KataGo counts the game the status says so. A further capsule shows the
// player's last move (CoachPanel's move card), in 边下边提醒 with its rating or the turn's hint, and
// after a 恶手 offers 重下这手 / 看 AI 推荐 / 不重下，继续对局 (the decision rows); the marks go on
// the board. After the game the status states the outcome and the result, a line gives the
// summary's figures, and 再来一盘 comes before 结束并复盘 (CoachPanelState.summary). Where the
// workbench has no room for the coach section (after the game, for every one of its mistakes),
// the last move (after the game the figures) opens it over the board. It sits between the
// workbench and the bottom toolbar (LizzieFrame.layoutMainContent) while a game is on; when the
// window is too narrow for one line, the capsules go on to a second.
import { run } from "../app";
import type { HumanSlCommand, HumanSlFeedbackView, HumanSlLastMove, HumanSlResult, HumanSlView, Verdict } from "../contract/generated";
import { format, t } from "../i18n";
import { boldFigure, chevron, coachInPopover, keepButtonKeys, onCoachPopoverChange, toggleCoachPopover } from "./coach";
import { coachLevel, humanSlGameActive, humanSlSession, humanSlUi, levelFox } from "./state";
import { OUTCOME_KEYS, VERDICT_KEYS, outcome, summaryStats } from "./summary";
import "./humansl.css";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** Writes `text` only when it differs: the bar repaints every 500 ms, and replacing a button's
 *  text between its press and release would lose the click. */
function setText(e: HTMLElement, text: string): void {
  if (e.textContent !== text) e.textContent = text;
}

function send(op: HumanSlCommand): void {
  void run({ type: "HumanSl", op });
}

/** HumanSlTrainingBar.format: MM:SS. */
function clock(ms: number): string {
  const seconds = Math.max(0, Math.floor(ms / 1000));
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

/** CoachPanel.areaName: the nine areas row by row from the top left (CoachBoardMarks.areaOf). */
const AREA_KEYS = [
  "CoachPanel.area.topLeft", "CoachPanel.area.top", "CoachPanel.area.topRight",
  "CoachPanel.area.left", "CoachPanel.area.center", "CoachPanel.area.right",
  "CoachPanel.area.bottomLeft", "CoachPanel.area.bottom", "CoachPanel.area.bottomRight",
];

/** The coach capsule's line: in 边下边提醒 the turn's hint once asked for; otherwise the player's
 *  last move (停一手 for a pass, HumanSlGameController.lastHumanMoveText), in 边下边提醒 with its
 *  rating. */
function coachNote(last: HumanSlLastMove | null, f: HumanSlFeedbackView | null): { text: string; verdict: Verdict | "" } {
  const hint = f?.hint;
  if (hint && !f.decision) {
    if (hint.step === "Waiting") return { text: t("CoachPanel.hint.waiting"), verdict: "" };
    if (hint.step === "Area" && hint.area !== null) {
      return { text: format(t("CoachPanel.hint.area"), t(AREA_KEYS[hint.area])), verdict: "" };
    }
    return { text: format(t("CoachPanel.hint.point"), hint.vertex ?? ""), verdict: "" };
  }
  if (!last) return { text: "", verdict: "" };
  const head = `${t("CoachPanel.lastMove")} ${last.vertex === "pass" ? t("HumanSlGame.btn.pass") : last.vertex}`;
  if (!f) return { text: head, verdict: "" };
  if (last.verdict) return { text: `${head} · ${t(VERDICT_KEYS[last.verdict])}`, verdict: last.verdict };
  return { text: `${head} · ${t(f.judging ? "CoachPanel.judging" : "CoachPanel.notRated")}`, verdict: "" };
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
  keepButtonKeys(host);

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
  // After the game: the outcome, then the result.
  const outcomeText = el("span", "hs-sum-head");
  const result = el("span", "hs-sum-result");
  const aiClock = el("span", "hs-bar-clock");
  aiClock.dataset.key = "humansl.bar.aiClock";
  const thinking = el("div", "hs-bar-thinking");
  thinking.dataset.key = "humansl.bar.thinking";
  thinking.append(el("div", "hs-bar-thinking-run"));
  statusPanel.append(humanClock, turn, aiClock, thinking);
  // CoachPanelState.summary: the moves, in 边下边提醒 the key mistakes and the hints, the take-backs.
  const stats = el("span", "hs-bar-stats");
  stats.dataset.key = "humansl.bar.stats";
  let statsShown = "";

  // The game's actions, and 结束并复盘 apart from them (CoachPanel: the button grid, the finish
  // button below it): on a narrow window the two capsules go on to separate lines.
  const actions = el("div", "hs-bar-actions hs-cap");
  const ending = el("div", "hs-bar-actions hs-cap");
  const make = (parent: HTMLElement, name: string, style: string, onClick: () => void) => {
    const b = el("button", `hs-seg ${style}`);
    b.type = "button";
    b.dataset.key = `humansl.bar.${name}`;
    b.onclick = onClick;
    parent.append(b);
    return b;
  };
  // The labels are set when the bar is painted, in the interface language of then.
  const hint = make(actions, "hint", "", () => send({ type: "Hint" }));
  const takeBack = make(actions, "takeBack", "", () => send({ type: "TakeBack" }));
  const pass = make(actions, "pass", "", () => send({ type: "Pass" }));
  const resign = make(actions, "resign", "", () => send({ type: "Resign" }));
  const retry = make(actions, "retryAi", "", () => send({ type: "RetryAi" }));
  // 再来一盘 (CoachPanel's PLAY_AGAIN): the session ends, then the setup opens again with the last
  // game's choices (abortAndThen).
  const playAgain = make(ending, "playAgain", "", () => send({ type: "AbortThen", continuation: "HumanSlSetup" }));
  // 重试清理 runs the exit again (Finish); a stop in progress is never clicked (disabled).
  const finish = make(ending, "finish", "danger", () => {
    const phase = humanSlSession()?.phase;
    send({ type: phase === "Stopping" ? "Abort" : "Finish" });
  });

  // The last move (its rating or the turn's hint in 边下边提醒), and the three choices after a 恶手.
  // Where the coach section has no room on the workbench, the last move (after the game the
  // figures) is a button that opens the section over the board.
  const coach = el("div", "hs-bar-coach hs-cap");
  coach.dataset.name = "humanSlCoach";
  const details = el("button", "hs-seg hs-details");
  details.type = "button";
  details.dataset.key = "humansl.bar.details";
  details.setAttribute("aria-haspopup", "dialog");
  details.setAttribute("aria-expanded", "false");
  details.append(chevron());
  details.onclick = () => toggleCoachPopover(details);
  details.hidden = true;
  const note = el("span", "hs-bar-note");
  note.dataset.key = "humansl.bar.note";
  coach.append(details, note);
  const retryMove = make(coach, "retryMove", "", () => send({ type: "Decide", choice: "Retry" }));
  const showBest = make(coach, "showBest", "", () => send({ type: "Decide", choice: "ShowBest" }));
  const playOn = make(coach, "playOn", "", () => send({ type: "Decide", choice: "PlayOn" }));

  host.append(identity, statusPanel, stats, coach, actions, ending);
  document.getElementById("bottombar")!.before(host);
  bar = host;

  /** The clocks' bases: the totals of the last view and when they arrived. */
  let base = { key: "", at: performance.now() };
  let ticker: number | undefined;

  const paint = (s: HumanSlView) => {
    setText(caption, t("HumanSlTraining.bar.opponent"));
    // HumanSlGameController.opponentLabel: the opponent's Fox / Tygem rank.
    const level = coachLevel(s.level);
    setText(opponent, level ? levelFox(level) : "");
    const key = `${s.id}:${s.human_ms}:${s.ai_ms}:${s.clock}`;
    if (key !== base.key) base = { key, at: performance.now() };
    const live = performance.now() - base.at;
    setText(humanClock, `${t("HumanSlTraining.bar.you")}  ${clock(s.human_ms + (s.clock === "Human" ? live : 0))}`);
    setText(aiClock, `${t("HumanSlTraining.bar.ai")}  ${clock(s.ai_ms + (s.clock === "Ai" ? live : 0))}`);
    const stopping = s.phase === "Stopping";
    const cleanup = s.phase === "CleanupPending";
    const over = s.result !== null;
    if (s.result && !stopping && !cleanup) {
      // CoachPanelState.summary: 你赢了 / 你输了 / 和棋 / 对局结束, then the result.
      const o = outcome(s.result, s.human_is_black);
      if (turn.firstElementChild !== outcomeText) turn.replaceChildren(outcomeText, " · ", result);
      setText(outcomeText, t(OUTCOME_KEYS[o]));
      if (outcomeText.dataset.outcome !== o) outcomeText.dataset.outcome = o;
      setText(result, resultText(s.result));
    } else {
      setText(turn, cleanup ? t("HumanSlTraining.bar.cleanupRetry")
        : stopping ? t("HumanSlTraining.bar.stopping")
        : s.counting ? t("CoachPanel.status.counting")
        : s.ai_failed ? t("HumanSlTraining.bar.aiFailed")
        : s.feedback?.decision ? t("CoachPanel.status.decide")
        : s.ai_thinking ? t("HumanSlTraining.bar.aiThinking")
        : t("HumanSlTraining.bar.yourTurn"));
    }
    thinking.classList.toggle("on", s.ai_thinking || s.counting || stopping);
    const f = s.feedback;
    setText(hint, t(f?.hint?.step === "Area" ? "CoachPanel.hint.more" : "CoachPanel.hint"));
    hint.hidden = over || !f;
    hint.disabled = !(s.phase === "Playing" && f?.can_hint && f.hint?.step !== "Point");
    setText(takeBack, t("HumanSlTraining.takeBack"));
    setText(pass, t("HumanSlGame.btn.pass"));
    setText(resign, t("HumanSlGame.btn.resign"));
    setText(retry, t("HumanSlTraining.retryAi"));
    // After the game its buttons are done (CoachPanel: the grid hides); 结束并复盘 stays.
    takeBack.hidden = pass.hidden = resign.hidden = over;
    retry.hidden = !(s.phase === "Playing" && s.ai_failed);
    actions.hidden = over && retry.hidden;
    // canTakeBack: a move of the player's to withdraw, also while the AI thinks.
    takeBack.disabled = !(s.phase === "Playing" && s.can_take_back);
    pass.disabled = !(s.phase === "Playing" && s.human_turn);
    // canResign: also while the AI thinks, not while KataGo counts.
    resign.disabled = !(s.phase === "Playing" && !s.counting && !over);
    setText(finish, cleanup ? t("HumanSlTraining.retryCleanup") : stopping ? t("HumanSlTraining.stopping") : t("HumanSlTraining.finishReview"));
    finish.disabled = stopping;
    setText(playAgain, t("CoachPanel.playAgain"));
    playAgain.hidden = !(over && s.phase === "Playing");
    // The figures: built again only when one changes.
    const figures = over ? summaryStats(s) : [];
    const shown = JSON.stringify(figures);
    if (shown !== statsShown) {
      statsShown = shown;
      stats.replaceChildren();
      figures.forEach(({ key, value }, i) => {
        if (i) stats.append(el("span", "hs-sep", "·"));
        stats.append(boldFigure(t(key), value));
      });
    }
    const decision = f?.decision ?? null;
    const n = coachNote(s.last_move, f ?? null);
    setText(note, n.text);
    if (note.dataset.verdict !== n.verdict) note.dataset.verdict = n.verdict;
    note.hidden = over || !n.text;
    setText(retryMove, t("CoachPanel.retry"));
    setText(showBest, t("CoachPanel.showBest"));
    setText(playOn, t("CoachPanel.continue"));
    // CoachPanelState.prompt: 看 AI 推荐 goes once KataGo's point is marked.
    retryMove.hidden = playOn.hidden = !decision;
    showBest.hidden = !decision || decision.best_shown;
    // The button carries the last move during the game and the figures after it; the other stays
    // in its place.
    const carried = over ? stats : note;
    const opens = coachInPopover() && !!carried.textContent;
    details.hidden = !opens;
    const noteInside = opens && carried === note;
    const statsInside = opens && carried === stats;
    if (noteInside !== (note.parentElement === details)) noteInside ? details.prepend(note) : details.after(note);
    if (statsInside !== (stats.parentElement === details)) statsInside ? details.prepend(stats) : statusPanel.after(stats);
    stats.hidden = !over;
    coach.hidden = over ? !opens : !n.text && !decision;
  };
  onCoachPopoverChange(() => refresh?.());

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

