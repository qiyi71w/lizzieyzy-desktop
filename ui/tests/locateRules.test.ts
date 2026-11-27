// Function search's entries for settings inside a dialog or a child window: when they can reach
// their control, and what they say when they cannot.
// Run with: npm test

import assert from "node:assert/strict";
import { test } from "node:test";
import type { StrengthStage, WorkbenchView } from "../src/contract/generated.ts";
import { ogsAfterGetReason, openingLocateReason, pendingLocate, phasesLocateReason } from "../src/search/locateRules.ts";

/** A view with only what the rules read. */
function view(over: { kind?: "Analysis" | "Genmove"; game?: boolean; match?: "Playing" | "Paused" | null; humanSl?: string | null } = {}): WorkbenchView {
  return {
    settings: { matches: { session: { kind: over.kind ?? "Analysis" } } },
    game: over.game ? {} : null,
    engine_match: over.match ? { phase: over.match } : null,
    human_sl: over.humanSl ? { phase: over.humanSl } : null,
  } as unknown as WorkbenchView;
}

test("开局广度 / 开局激进度 can be located for an analysis-mode match with nothing else on", () => {
  assert.equal(openingLocateReason(view()), null);
});

test("开局广度 / 开局激进度 keep their earlier refusals", () => {
  assert.deepEqual(openingLocateReason(view({ match: "Playing" })), { key: "LizzieFrame.engineGameStopFirstHint" });
  assert.deepEqual(openingLocateReason(view({ match: "Paused" })), { key: "LizzieFrame.engineGameStopFirstHint" });
  assert.deepEqual(openingLocateReason(view({ game: true })), { key: "matchOpeningDuringGame", product: true });
  assert.deepEqual(openingLocateReason(view({ kind: "Genmove" })), { key: "matchOpeningNeedsAnalysis", product: true });
});

test("开局广度 / 开局激进度 are refused while an AI 陪练 session is on, which opening 引擎对局 would end", () => {
  for (const phase of ["Preparing", "Playing", "Review"]) {
    assert.deepEqual(openingLocateReason(view({ humanSl: phase })), { key: "matchOpeningDuringCoach", product: true }, phase);
  }
});

test("分阶段 is located only once the game can be estimated", () => {
  assert.equal(phasesLocateReason({ type: "Ready", partial: false }), null);
  assert.equal(phasesLocateReason({ type: "Ready", partial: true }), null);
});

test("分阶段 before the game can be estimated: not located, the dashboard's own page says why", () => {
  const cases: [StrengthStage, string][] = [
    [{ type: "TooShort" }, "PlayerStrengthEstimate.empty.short.title"],
    [{ type: "NeedsAnalysis" }, "PlayerStrengthEstimate.empty.analysis.title"],
    [{ type: "NeedsHumanStyle" }, "PlayerStrengthEstimate.empty.humanStyle.title"],
  ];
  for (const [stage, key] of cases) assert.deepEqual(phasesLocateReason(stage), { key, shown: true }, stage.type);
});

test("打开棋谱后此窗口 waits while its choice cannot take the focus, then locates the request once", () => {
  let ready = false;
  const acts: number[] = [];
  const locate = pendingLocate(() => ready, (request) => acts.push(request));
  assert.equal(locate(1), false, "not located while the choice is disabled");
  assert.deepEqual(acts, []);
  ready = true;
  assert.equal(locate(1), true, "located once the choice can take the focus");
  assert.deepEqual(acts, [1], "the act names the request it located");
  assert.equal(locate(1), false, "a repeated payload of the same request locates nothing more");
  assert.deepEqual(acts, [1]);
});

test("打开棋谱后此窗口 locates again for a newer request, and after a payload without one", () => {
  const acts: number[] = [];
  const locate = pendingLocate(() => true, (request) => acts.push(request));
  assert.equal(locate(null), false, "no request, nothing to locate");
  locate(1);
  assert.equal(locate(2), true, "a newer request locates again, even with no payload between");
  locate(null);
  assert.equal(locate(2), true, "after a payload without a request, the request locates again");
  assert.deepEqual(acts, [1, 2, 2]);
});

test("OGS 打开棋谱后此窗口 is unavailable while the window searches or downloads", () => {
  const ogs = (busy: boolean) => ({ ogs_kifu: { busy } }) as unknown as WorkbenchView;
  assert.deepEqual(ogsAfterGetReason(ogs(true)), { key: "ogsAfterGetBusy", product: true });
  assert.equal(ogsAfterGetReason(ogs(false)), null);
  assert.equal(ogsAfterGetReason(null), null);
});
