// The main window's wording (ui/src/workbench/wording.ts) on fixed inputs. The expected texts are
// what the current Java writes for the same values: MoveStatusText.lead and previousMove,
// CandidateTableText, LizzieFrame.setPlayers / setResult with MainWindowTitle.join, and
// InfoPanelText.komi.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import { changeFromBest, komiText, lead, lossLevel, players, previousMove, withChange } from "../src/workbench/wording.ts";

test("the lead names the side ahead, and a lead that rounds to 0.0 is even", () => {
  assert.deepEqual(lead(7.3), { side: "Black", points: "7.3" });
  assert.deepEqual(lead(-7.3), { side: "White", points: "7.3" });
  assert.deepEqual(lead(-0.26), { side: "White", points: "0.3" });
  assert.deepEqual(lead(0.06), { side: "Black", points: "0.1" });
  assert.equal(lead(0.04), "even");
  assert.equal(lead(-0.04), "even");
  assert.equal(lead(0), "even");
});

test("the previous move writes its rank, then its changes without a colon, and 0.0 without a sign", () => {
  const zh = { name: "上一手", pts: "目" };
  const at = (rank: number | null, win: number, score: number | null) => previousMove(rank, win, score, zh);
  // Down (Java MoveStatusTextTest: "上一手(#3) -0.3% -1.9目").
  assert.deepEqual(at(3, -0.34, -1.88), {
    text: "上一手(#3) -0.3% -1.9目",
    name: "上一手(#3)",
    winrate: { value: "-0.3%", loss: true },
    score: { value: "-1.9目", loss: true },
  });
  // Up, not a candidate.
  assert.equal(at(null, 2.46, 1.04).text, "上一手 +2.5% +1.0目");
  assert.equal(at(null, 2.46, 1.04).winrate.loss, false);
  // Both changes round to 0.0: no sign and no loss (Java: "上一手(#1) 0.0% 0.0目").
  const even = at(1, -0.01, 0.02);
  assert.equal(even.text, "上一手(#1) 0.0% 0.0目");
  assert.equal(even.winrate.loss || even.score?.loss, false);
  assert.equal(at(2, 0.04, -0.04).text, "上一手(#2) 0.0% 0.0目");
  // Without a score change only the win rate (Java: "上一手 +1.2%").
  assert.deepEqual(at(null, 1.2, null), { text: "上一手 +1.2%", name: "上一手", winrate: { value: "+1.2%", loss: false }, score: null });
  // Rank 0 is no candidate; the unit follows the bundle without a space, as Java appends it.
  assert.equal(previousMove(0, -5, 3.25, { name: "Prev. move", pts: "pts" }).text, "Prev. move -5.0% +3.3pts");
});

test("the actual move's cell writes the value, then its change from the best after an arrow", () => {
  // Down, up and unchanged.
  assert.equal(withChange(99.2, changeFromBest(45.3, 45.4)), "99.2 ↓0.1");
  assert.equal(withChange(50.4, changeFromBest(50.4, 55.0)), "50.4 ↓4.6");
  assert.equal(withChange(12.0, changeFromBest(3.5, 1.25)), "12.0 ↑2.3");
  assert.equal(withChange(60.0, changeFromBest(60.0, 60.0)), "60.0");
  // A change that rounds to 0.0 is left out.
  assert.equal(withChange(60.0, changeFromBest(60.0, 60.04)), "60.0");
  // Without both values only the value.
  assert.equal(withChange(-3.5, changeFromBest(null, 2)), "-3.5");
});

/** The cells and the parsing before this change: "↓-4.6(50.4)", the renderers reading the number
 *  between the arrow and "(" back. */
function oldCell(value: number | null, best: number | null, shown: number): string {
  if (value === null || best === null) return shown.toFixed(1);
  const d = value - best;
  return `${d > 0 ? "↑" : "↓"}${d.toFixed(1)}(${shown.toFixed(1)})`;
}
function oldParsed(text: string): number {
  return text.includes("(") ? Number.parseFloat(text.substring(1, text.indexOf("("))) : 0;
}

test("each actual row keeps the loss level it had before the wording changed", () => {
  const win = [-1, -3, -6, -12, -24];
  const score = [-0.5, -1.5, -3, -6, -12];
  // [actual win rate, best win rate, actual score, best score]: the side to move's values.
  const rows: [number, number, number | null, number | null][] = [
    [55.0, 55.0, 3.2, 3.2],
    [54.96, 55.0, 3.18, 3.2],
    [54.04, 55.0, 3.2, 3.2],
    [53.95, 55.0, 3.2, 3.2],
    [54.0, 55.0, 2.75, 3.2],
    [54.0, 55.0, 1.65, 3.2],
    [52.0, 55.0, 1.7, 3.2],
    [49.05, 55.0, 0.2, 3.2],
    [43.0, 55.0, -2.8, 3.2],
    [31.04, 55.0, -8.8, 3.2],
    [10.0, 55.0, null, null],
    [56.3, 55.0, 4.0, 3.2],
    [-0.0, 0.04, -0.04, 0.0],
  ];
  const levels = rows.map(([w, bw, s, bs]) => {
    const before = lossLevel(oldParsed(oldCell(w, bw, w)), oldParsed(oldCell(s, bs, s ?? 0)), win, score);
    const after = lossLevel(changeFromBest(w, bw), changeFromBest(s, bs), win, score);
    assert.equal(after, before, `row ${w}/${bw}, ${s}/${bs}`);
    return after;
  });
  // Every level appears; the change is read at the tenth the cell writes, so -0.96 counts as -1.0
  // and only the rows that match the best stay at the best level.
  assert.deepEqual(levels, [5, 5, 4, 4, 4, 3, 3, 2, 1, 0, 0, 5, 5]);
});

test("the title writes both players in brackets with the result after them", () => {
  const zh = { black: "黑", white: "白", result: "结果:" };
  assert.equal(players("柯洁", "申真谞", "", zh), "[黑] 柯洁 vs [白] 申真谞");
  assert.equal(players("柯洁", "", "", zh), "[黑] 柯洁 vs [白]");
  assert.equal(players("", "申真谞", "", zh), "[黑]  vs [白] 申真谞");
  assert.equal(players("", "", "", zh), "[黑]  vs [白]");
  assert.equal(players("柯洁", "申真谞", "B+R", zh), "[黑] 柯洁 vs [白] 申真谞 (结果:B+R)");
  assert.equal(players("Ke Jie", "Shin Jinseo", "W+0.5", { black: "B", white: "W", result: "Result:" }), "[B] Ke Jie vs [W] Shin Jinseo (Result:W+0.5)");
});

test("the game panel's komi drops trailing zeros", () => {
  assert.equal(komiText(7.0), "7");
  assert.equal(komiText(7.5), "7.5");
  assert.equal(komiText(6.25), "6.25");
  assert.equal(komiText(0), "0");
  assert.equal(komiText(-0), "0");
  assert.equal(komiText(-6.5), "-6.5");
});

test("the game panel's komi is plain digits however small or large, as Java's toPlainString", () => {
  assert.equal(komiText(0.0000001), "0.0000001");
  assert.equal(komiText(0.00000015), "0.00000015");
  assert.equal(komiText(-0.000000025), "-0.000000025");
  assert.equal(komiText(0.000001), "0.000001");
  assert.equal(komiText(1e21), "1000000000000000000000");
  assert.equal(komiText(1.5e22), "15000000000000000000000");
});
