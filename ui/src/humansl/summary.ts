// What AI 陪练 shows of a game in words and figures, from the core's session view alone (Java
// CoachPanelState, CoachSummaryView): how the game ended for the player, the summary's figures,
// the change of the win chance a rated move made, the latest rated moves and the win-chance
// curve's geometry. No DOM and no interface language: callers pass the texts in.
import type { HumanSlResult, HumanSlView, MoveFeedback, Stone, Verdict } from "../contract/generated";

/** CoachPanel.verdictText. */
export const VERDICT_KEYS: Record<Verdict, string> = {
  Brilliant: "CoachPanel.verdict.brilliant",
  Good: "CoachPanel.verdict.good",
  Slow: "CoachPanel.verdict.slow",
  Problem: "CoachPanel.verdict.problem",
  Blunder: "CoachPanel.verdict.blunder",
};

/** The ratings in the order the core counts them (`HumanSlFeedbackView.counts`). */
export const VERDICTS: readonly Verdict[] = ["Brilliant", "Good", "Slow", "Problem", "Blunder"];

/** 缓手, 问题手 and 恶手 (MoveVerdict.isMistake). */
export function isMistake(v: Verdict): boolean {
  return v === "Slow" || v === "Problem" || v === "Blunder";
}

/** How the game ended for the player (CoachPanelState.summary): a resignation or a count with a
 *  winner, also a drawn count the rules gave to the first passer, is won or lost; a drawn count is
 *  a draw; a game KataGo did not count is only over. */
export type Outcome = "Won" | "Lost" | "Draw" | "Over";

export function outcome(r: HumanSlResult, humanIsBlack: boolean): Outcome {
  let winner: Stone;
  switch (r.type) {
    case "Uncounted":
      return "Over";
    case "Resigned":
      winner = r.winner;
      break;
    case "Counted":
      if (r.verdict.type === "Draw") return "Draw";
      winner = r.verdict.winner;
      break;
  }
  return (winner === "Black") === humanIsBlack ? "Won" : "Lost";
}

/** The headline's key for `o` (CoachPanel.youWon / youLost, HumanSlGame.draw, CoachPanel.gameOver). */
export const OUTCOME_KEYS: Record<Outcome, string> = {
  Won: "CoachPanel.youWon",
  Lost: "CoachPanel.youLost",
  Draw: "HumanSlGame.draw",
  Over: "CoachPanel.gameOver",
};

/** The summary's figures with their sentence keys, in Java's order: the moves, in 边下边提醒 the key
 *  mistakes and the hints, then the take-backs. */
export function summaryStats(s: Pick<HumanSlView, "moves" | "take_backs" | "feedback">): { key: string; value: number }[] {
  const stats = [{ key: "CoachPanel.stat.moves", value: s.moves }];
  if (s.feedback) {
    stats.push({ key: "CoachPanel.stat.mistakes", value: s.feedback.key_mistakes }, { key: "CoachPanel.stat.hints", value: s.feedback.hints });
  }
  stats.push({ key: "CoachPanel.stat.takeBacks", value: s.take_backs });
  return stats;
}

/** A change of the win chance (0 to 1) in whole percent with its sign, "−12%" or "+3%"; a change
 *  that rounds to nothing is "+0%" (CoachPanelState.signedPercent). */
export function signedPercent(change: number): string {
  const points = Math.round(Math.abs(change) * 100);
  return change < 0 && points > 0 ? `\u2212${points}%` : `+${points}%`;
}

/** The latest rated moves, newest first (CoachPanelState.RECENT_ROWS). */
export function recentRatings(ratings: readonly MoveFeedback[], rows = 8): MoveFeedback[] {
  return ratings.slice(-rows).reverse();
}

/** The win-chance curve at its real size `w` x `h`: 0 % at the bottom, 100 % at the top, the
 *  values spread evenly from the left edge to the right one (a single value at the right edge);
 *  the line, the area between it and 50 %, and the last point. */
export interface CurveGeometry {
  line: string;
  area: string;
  /** The 25 %, 50 % and 75 % heights. */
  y25: number;
  y50: number;
  y75: number;
  last: { x: number; y: number };
}

export function curveGeometry(values: readonly number[], w: number, h: number): CurveGeometry {
  const n = values.length;
  const x = (i: number) => (n < 2 ? w : (w * i) / (n - 1));
  const y = (v: number) => h * (1 - Math.min(100, Math.max(0, v)) / 100);
  const r = (v: number) => Math.round(v * 10) / 10;
  const points = values.map((v, i) => `${r(x(i))},${r(y(v))}`);
  const mid = r(y(50));
  return {
    line: n > 1 ? `M${points.join("L")}` : "",
    area: n > 1 ? `M${r(x(0))},${mid}L${points.join("L")}L${r(x(n - 1))},${mid}Z` : "",
    y25: r(y(25)),
    y50: mid,
    y75: r(y(75)),
    last: { x: r(x(n - 1)), y: r(y(n ? values[n - 1] : 50)) },
  };
}
