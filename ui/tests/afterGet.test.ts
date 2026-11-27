// Function search's 打开棋谱后此窗口: the main window's side of one request to a 棋谱 window.
// A request ends once: when the window reports this request located, when a newer request takes
// its place, or at the deadline, whatever the window's opening is doing by then.
// Run with: npm test

import assert from "node:assert/strict";
import { test } from "node:test";
// afterGet.ts reads `window` only while a request runs, so the clock below is in place by then.
import { afterGetRequest } from "../src/foxkifu/afterGet.ts";

const TARGET_MISSING = { key: "FunctionSearch.unavailable.targetMissing" };
const DEADLINE_MS = 10_000;

/** A clock the tests move by hand, standing in for the window's timers. */
const clock = {
  now: 0,
  nextId: 1,
  timers: [] as { id: number; due: number; fn: () => void }[],
  setTimeout(fn: () => void, ms: number): number {
    const id = clock.nextId++;
    clock.timers.push({ id, due: clock.now + ms, fn });
    return id;
  },
  clearTimeout(id: number): void {
    clock.timers = clock.timers.filter((t) => t.id !== id);
  },
  reset(): void {
    clock.now = 0;
    clock.timers = [];
  },
  advance(ms: number): void {
    const end = clock.now + ms;
    for (;;) {
      const due = clock.timers.filter((t) => t.due <= end).sort((a, b) => a.due - b.due)[0];
      if (!due) break;
      clock.timers = clock.timers.filter((t) => t !== due);
      clock.now = due.due;
      due.fn();
    }
    clock.now = end;
  },
};
Object.assign(globalThis, { window: clock });

/** Lets pending promise callbacks run. */
async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await Promise.resolve();
}

/** A request object whose window opens at once, counting relays. */
function setup(open: () => Promise<void> = async () => {}) {
  clock.reset();
  const relays: (number | null)[] = [];
  const r = afterGetRequest(open, () => relays.push(r.request));
  return { r, relays };
}

/** Follows a locate() call without awaiting it. */
function track(p: Promise<unknown>) {
  const s: { settled: boolean; value?: unknown; error?: unknown } = { settled: false };
  p.then(
    (value) => Object.assign(s, { settled: true, value }),
    (error) => Object.assign(s, { settled: true, error }),
  );
  return s;
}

test("the window's report for the current request locates it, and the window then hears the request ended", async () => {
  const { r, relays } = setup();
  const a = track(r.locate());
  const id = r.request;
  assert.equal(typeof id, "number");
  await flush();
  assert.deepEqual(relays, [id], "once open, the window is sent the request");
  r.located(id!);
  await flush();
  assert.deepEqual(a, { settled: true, value: null });
  assert.equal(r.request, null);
  assert.deepEqual(relays, [id, null], "one relay when the request ends");
  assert.equal(clock.timers.length, 0, "no deadline left");
});

test("each request has its own number", () => {
  const { r } = setup();
  void r.locate();
  const first = r.request;
  void r.locate();
  assert.notEqual(r.request, first);
});

test("a newer request ends the older one as not reached, and the older one's late report does not count for the newer", async () => {
  const { r, relays } = setup();
  const a = track(r.locate());
  const aId = r.request!;
  await flush();
  const b = track(r.locate());
  const bId = r.request!;
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING });
  const relaysBefore = relays.length;
  r.located(aId);
  await flush();
  assert.equal(b.settled, false, "the older request's report leaves the newer one waiting");
  assert.equal(r.request, bId);
  assert.equal(clock.timers.length, 1, "the newer request keeps its deadline");
  assert.equal(relays.length, relaysBefore, "nothing sent for a report that does not count");
  r.located(bId);
  await flush();
  assert.deepEqual(b, { settled: true, value: null });
});

test("a report from a request that ran out of time does not count for the next one", async () => {
  const { r } = setup();
  const a = track(r.locate());
  const aId = r.request!;
  await flush();
  clock.advance(DEADLINE_MS + 1);
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING });
  const b = track(r.locate());
  await flush();
  r.located(aId);
  await flush();
  assert.equal(b.settled, false);
  assert.equal(clock.timers.length, 1);
});

test("a report after the request ended changes nothing", async () => {
  const { r, relays } = setup();
  const a = track(r.locate());
  const aId = r.request!;
  await flush();
  clock.advance(DEADLINE_MS + 1);
  await flush();
  const relaysBefore = relays.length;
  r.located(aId);
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING });
  assert.equal(relays.length, relaysBefore);
});

test("the deadline ends the request even while the window has not opened", async () => {
  const opening = Promise.withResolvers<void>();
  const { r, relays } = setup(() => opening.promise);
  const a = track(r.locate());
  await flush();
  clock.advance(DEADLINE_MS + 1);
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING }, "not reached at the deadline");
  assert.equal(r.request, null);
  const relaysBefore = relays.length;
  opening.resolve();
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING }, "opening later changes nothing");
  assert.equal(relays.length, relaysBefore, "and asks the window for nothing");
});

test("the window failing to open before the deadline reaches the caller", async () => {
  const opening = Promise.withResolvers<void>();
  const { r, relays } = setup(() => opening.promise);
  const a = track(r.locate());
  await flush();
  const failure = new Error("window failed");
  opening.reject(failure);
  await flush();
  assert.deepEqual(a, { settled: true, error: failure });
  assert.equal(r.request, null);
  assert.equal(clock.timers.length, 0);
  assert.deepEqual(relays, [null], "the request's end is sent once");
});

test("the window failing to open after the deadline changes nothing", async () => {
  const opening = Promise.withResolvers<void>();
  const { r, relays } = setup(() => opening.promise);
  const a = track(r.locate());
  await flush();
  clock.advance(DEADLINE_MS + 1);
  await flush();
  const relaysBefore = relays.length;
  opening.reject(new Error("window failed"));
  await flush();
  assert.deepEqual(a, { settled: true, value: TARGET_MISSING });
  assert.equal(relays.length, relaysBefore);
});
