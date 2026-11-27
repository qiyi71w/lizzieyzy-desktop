// Saving the 分阶段 boundaries while saves overlap (ui/src/strength/phaseSaves.ts): the core answers
// the saves in the order they were sent; the newest input settles the view, on what is saved.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import { phaseSaves } from "../src/strength/phaseSaves.ts";

interface Phases {
  bounds: [number, number];
}

/** A save the test answers: `ok` with the phases at the sent boundaries, `fail` with null. */
function core() {
  const sent: { bounds: [number, number]; ok(): void; fail(): void }[] = [];
  const save = (bounds: [number, number]) => {
    const { promise, resolve } = Promise.withResolvers<Phases | null>();
    sent.push({ bounds, ok: () => resolve({ bounds }), fail: () => resolve(null) });
    return promise;
  };
  return { sent, save };
}

const DEFAULT: Phases = { bounds: [60, 150] };

test("→ then ←: the first save is written, the second fails: the view settles on the first", async () => {
  const c = core();
  const saves = phaseSaves(DEFAULT, c.save);
  const first = saves.keep([61, 150]);
  const second = saves.keep([60, 150]);
  assert.equal(saves.pending, 2);
  c.sent[0].ok();
  assert.equal(await first, null);
  c.sent[1].fail();
  assert.deepEqual(await second, { bounds: [61, 150] });
  assert.deepEqual(saves.saved, { bounds: [61, 150] });
  assert.equal(saves.pending, 0);
});

test("→ then ←, both saves written: the view settles on the second", async () => {
  const c = core();
  const saves = phaseSaves(DEFAULT, c.save);
  const first = saves.keep([61, 150]);
  const second = saves.keep([60, 150]);
  c.sent[0].ok();
  c.sent[1].ok();
  assert.equal(await first, null);
  assert.deepEqual(await second, { bounds: [60, 150] });
  assert.deepEqual(saves.saved, { bounds: [60, 150] });
});

test("both saves fail: the view returns to the boundaries saved before", async () => {
  const c = core();
  const saves = phaseSaves(DEFAULT, c.save);
  const first = saves.keep([61, 150]);
  const second = saves.keep([60, 150]);
  c.sent[0].fail();
  c.sent[1].fail();
  assert.equal(await first, null);
  assert.equal(await second, DEFAULT);
  assert.equal(saves.saved, DEFAULT);
});

test("every keep is sent, a copy of the boundaries, and counts as started at once", async () => {
  const c = core();
  const saves = phaseSaves(DEFAULT, c.save);
  const bounds: [number, number] = [70, 150];
  const kept = saves.keep(bounds);
  bounds[0] = 80;
  assert.equal(saves.started, 1);
  assert.deepEqual(c.sent.map((s) => s.bounds), [[70, 150]]);
  c.sent[0].ok();
  assert.deepEqual(await kept, { bounds: [70, 150] });
  assert.equal(saves.started, 1);
});
