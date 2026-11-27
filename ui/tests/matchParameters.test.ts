// What the 对局参数 panel does after 确定 with each view (ui/src/match/request.ts). The expected
// outcomes are Java EngineGameParametersDialog.refresh while closeWhenConfirmed: a failure that is
// no longer pending reopens the panel, every value confirmed closes it, no playing game reopens
// it; here they hold also when the first view after the request is already the outcome.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import type { MatchParameterFailure, MatchParameterValue, MatchView } from "../src/contract/generated.ts";
import { requestState } from "../src/match/request.ts";

const value = (confirmed: number | null, target: number | null = null): MatchParameterValue => ({ supported: true, confirmed, target });

/** Game 3 with 引擎 2's breadth as `wrn`, every other value confirmed at 0. */
function game(wrn: MatchParameterValue, pending = false, failures: MatchParameterFailure[] = [], number = 3): MatchView {
  const parameters = { pending, failures, first: { wrn: value(0), pda: value(0) }, second: { wrn, pda: value(0) } };
  return { game: number, parameters } as unknown as MatchView;
}

const request = { game: 3, after: 10 };
const notSent: MatchParameterFailure = { first: false, reason: { type: "NotSent", command: "kata-set-param analysisWideRootNoise 0.5" } };
const mismatch: MatchParameterFailure = { first: false, reason: { type: "Mismatch", actual: "0.25", expected: "0.5" } };

test("a view built before the core took the values decides nothing, though it shows them settled", () => {
  assert.equal(requestState(request, 9, game(value(0))), "waiting");
  assert.equal(requestState(request, 10, game(value(0.5))), "waiting");
  assert.equal(requestState(request, 10, game(value(null, 0.5), false, [notSent])), "waiting");
  assert.equal(requestState(request, 10, null), "waiting");
});

test("the first view after the request may already be its outcome: confirmed closes the panel", () => {
  assert.equal(requestState(request, 11, game(value(0.5))), "confirmed");
});

test("a write that could not be sent, or a read-back of another value, gives the panel back", () => {
  assert.equal(requestState(request, 11, game(value(null, 0.5), false, [notSent])), "open");
  assert.equal(requestState(request, 11, game(value(null, 0.5), false, [mismatch])), "open");
});

test("values still being written keep the panel waiting, also a retry over a recorded failure", () => {
  assert.equal(requestState(request, 11, game(value(null, 0.5), true)), "waiting");
  assert.equal(requestState(request, 11, game(value(null, 0.5), true, [mismatch])), "waiting");
});

test("without the request's game playing the panel takes another try", () => {
  assert.equal(requestState(request, 11, null), "open");
  assert.equal(requestState(request, 11, game(value(0.5), false, [], 4)), "open");
});
