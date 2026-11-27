// The toolbar's pending analysis changes (ui/src/chrome/analysisDraft.ts): what each save sends
// and when the changes go.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import { PendingChanges, taken } from "../src/chrome/analysisDraft.ts";

type Limits = { limitMaxSuggestion: number; maxAnalyzeTimeSeconds: number };
const start: Limits = { limitMaxSuggestion: 10, maxAnalyzeTimeSeconds: 600 };
const done = { type: "Done" } as const;
const refused = { type: "Refused", message_key: "Fox.conflict" } as const;

test("only a refusal and a failure count as not taken", () => {
  assert.equal(taken(done), true);
  assert.equal(taken({ type: "Unchanged" }), true);
  assert.equal(taken(refused), false);
  assert.equal(taken(null), false);
});

test("a refused change is dropped: the fields show the projection and a later save does not carry it", () => {
  const draft = new PendingChanges<Limits>();
  assert.deepEqual(draft.send(start, { limitMaxSuggestion: 3 }), { limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 600 });
  draft.answered(refused, start);
  assert.deepEqual(draft.over(start), start);
  assert.deepEqual(draft.send(start, { maxAnalyzeTimeSeconds: 9 }), { limitMaxSuggestion: 10, maxAnalyzeTimeSeconds: 9 });
});

test("a failed save (null) is dropped the same way", () => {
  const draft = new PendingChanges<Limits>();
  draft.send(start, { limitMaxSuggestion: 3 });
  draft.answered(null, start);
  assert.deepEqual(draft.send(start, { maxAnalyzeTimeSeconds: 9 }), { limitMaxSuggestion: 10, maxAnalyzeTimeSeconds: 9 });
});

test("changes the core took build on each other until the projection shows them", () => {
  const draft = new PendingChanges<Limits>();
  draft.send(start, { limitMaxSuggestion: 3 });
  // The second change is sent before the first is projected: it keeps the first.
  assert.deepEqual(draft.send(start, { maxAnalyzeTimeSeconds: 9 }), { limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 9 });
  draft.answered(done, start);
  draft.answered(done, start);
  // Both answered, the projection still old: the fields keep showing the changes.
  assert.deepEqual(draft.over(start), { limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 9 });
  draft.settle({ limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 9 });
  // Caught up: the changes are gone, so a later projection is shown as it is.
  assert.deepEqual(draft.over(start), start);
});

test("a projection while a save is pending keeps the changes", () => {
  const draft = new PendingChanges<Limits>();
  draft.send(start, { limitMaxSuggestion: 3 });
  draft.settle({ limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 600 });
  assert.deepEqual(draft.send(start, { maxAnalyzeTimeSeconds: 9 }), { limitMaxSuggestion: 3, maxAnalyzeTimeSeconds: 9 });
});
