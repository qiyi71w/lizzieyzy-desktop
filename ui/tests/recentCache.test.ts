// The label measures' cache (ui/src/workbench/recentCache.ts, paint.ts measure): texts read on
// every repaint stay while texts that change with every update come and go.
// Run with `npm test` (Node 22.18 or later runs the TypeScript directly).
import assert from "node:assert/strict";
import { test } from "node:test";
import { RecentCache } from "../src/workbench/recentCache.ts";

/** A repaint as paint.ts does it: the fixed labels, then one new win rate per side, each measured
 *  only when the cache does not have it. Returns how often a fixed label was measured again. */
function repaints(cache: RecentCache<string, number>, count: number): number {
  const fixed = ["Black", "White", "上一手", "  100.0%"];
  let remeasured = 0;
  for (let n = 0; n < count; n++) {
    for (const text of fixed) {
      if (cache.get(text) === undefined) {
        if (n > 0) remeasured++;
        cache.set(text, text.length);
      }
    }
    for (const side of ["B", "W"]) {
      const rate = `${side} ${(n / 10).toFixed(1)}%`;
      if (cache.get(rate) === undefined) cache.set(rate, rate.length);
    }
  }
  return remeasured;
}

test("a thousand repaints with new win rates never measure the fixed labels again", () => {
  const cache = new RecentCache<string, number>(256);
  assert.equal(repaints(cache, 1000), 0);
  assert.equal(cache.size, 256);
});

test("the entry used longest ago makes room, a read entry counts as used", () => {
  const cache = new RecentCache<string, number>(2);
  cache.set("a", 1);
  cache.set("b", 2);
  assert.equal(cache.get("a"), 1);
  cache.set("c", 3);
  assert.equal(cache.get("b"), undefined);
  assert.equal(cache.get("a"), 1);
  assert.equal(cache.get("c"), 3);
});

test("setting a key again replaces it without dropping another", () => {
  const cache = new RecentCache<string, number>(2);
  cache.set("a", 1);
  cache.set("b", 2);
  cache.set("a", 3);
  assert.equal(cache.size, 2);
  assert.equal(cache.get("a"), 3);
  assert.equal(cache.get("b"), 2);
});
