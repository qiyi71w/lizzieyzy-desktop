// AI 陪练's words and figures (ui/src/humansl/summary.ts) on fixed inputs: how a game ended for the
// player, the summary's figures in Java's order (Java `CoachPanelState.summary`), the signed
// change of a rated move (Java `CoachPanelState.signedPercent`), the latest rated moves and the
// curve's geometry.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import type { HumanSlFeedbackView, MoveFeedback } from "../src/contract/generated.ts";
import { curveGeometry, outcome, recentRatings, signedPercent, summaryStats } from "../src/humansl/summary.ts";

test("a resignation or a counted win is won or lost for the player, a drawn count is a draw, an uncounted game only over", () => {
  assert.equal(outcome({ type: "Resigned", winner: "White" }, true), "Lost");
  assert.equal(outcome({ type: "Resigned", winner: "White" }, false), "Won");
  assert.equal(outcome({ type: "Counted", verdict: { type: "Win", winner: "Black", margin: 6.5 } }, true), "Won");
  // The rules give a drawn count to the first passer: a win or a loss, not a draw.
  assert.equal(outcome({ type: "Counted", verdict: { type: "DrawToFirstPasser", winner: "Black" } }, false), "Lost");
  assert.equal(outcome({ type: "Counted", verdict: { type: "Draw" } }, true), "Draw");
  assert.equal(outcome({ type: "Uncounted" }, true), "Over");
});

test("the figures are the moves, in 边下边提醒 the key mistakes and the hints, then the take-backs", () => {
  const feedback = { key_mistakes: 2, hints: 3 } as HumanSlFeedbackView;
  assert.deepEqual(summaryStats({ moves: 120, take_backs: 1, feedback }), [
    { key: "CoachPanel.stat.moves", value: 120 },
    { key: "CoachPanel.stat.mistakes", value: 2 },
    { key: "CoachPanel.stat.hints", value: 3 },
    { key: "CoachPanel.stat.takeBacks", value: 1 },
  ]);
  assert.deepEqual(summaryStats({ moves: 40, take_backs: 0, feedback: null }), [
    { key: "CoachPanel.stat.moves", value: 40 },
    { key: "CoachPanel.stat.takeBacks", value: 0 },
  ]);
});

test("a change is whole percent with its sign, and one that rounds to nothing is +0%", () => {
  assert.equal(signedPercent(-0.12), "\u221212%");
  assert.equal(signedPercent(0.031), "+3%");
  assert.equal(signedPercent(-0.004), "+0%");
  assert.equal(signedPercent(0), "+0%");
});

test("the latest rated moves come newest first, at most the rows asked for", () => {
  const rated = (n: number) => ({ move_number: n }) as MoveFeedback;
  const ratings = [1, 3, 5, 7, 9, 11, 13, 15, 17, 19].map(rated);
  assert.deepEqual(recentRatings(ratings).map((m) => m.move_number), [19, 17, 15, 13, 11, 9, 7, 5]);
  assert.deepEqual(recentRatings(ratings.slice(0, 2), 8).map((m) => m.move_number), [3, 1]);
  assert.deepEqual(recentRatings([]), []);
});

test("the curve spans the box: 0 % at the bottom, 100 % at the top, the last point at the right edge", () => {
  const g = curveGeometry([50, 100, 0, 25], 300, 100);
  assert.equal(g.line, "M0,50L100,0L200,100L300,75");
  assert.equal(g.area, "M0,50L0,50L100,0L200,100L300,75L300,50Z");
  assert.deepEqual([g.y75, g.y50, g.y25], [25, 50, 75]);
  assert.deepEqual(g.last, { x: 300, y: 75 });
  // One value: a point at the right edge and no line; none: the point on 50 %.
  const one = curveGeometry([62], 200, 80);
  assert.equal(one.line, "");
  assert.deepEqual(one.last, { x: 200, y: 30.4 });
  assert.deepEqual(curveGeometry([], 200, 80).last, { x: 200, y: 40 });
});
