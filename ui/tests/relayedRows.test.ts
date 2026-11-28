// The OGS 棋谱 window's rows relayed from published views, which carry them only after the list
// changed: a reloaded main window applies views built before its full snapshot first.
import assert from "node:assert/strict";
import { test } from "node:test";

import type { OgsKifuRow, OgsKifuView } from "../src/contract/generated.ts";
import { RelayedRows } from "../src/ogskifu/model.ts";

const row = (id: number): OgsKifuRow => ({ id, ended: "2026-01-01T12:00:00Z", black: "黒", white: `w${id}`, result: "B+R", size: "19×19" });

const view = (rows: OgsKifuRow[] | null, shown = 1): OgsKifuView => ({
  after_get: "Minimize",
  recent: ["alice"],
  label: { type: "Player", name: "Alice", id: 12 },
  rows,
  listed: true,
  page: 1,
  pages: 3,
  shown,
  busy: false,
  status: null,
  opened: 0,
});

test("a view without rows before any carried them relays nothing, not an empty page", () => {
  const relayed = new RelayedRows();
  assert.equal(relayed.windowView(view(null)), null);
  assert.deepEqual(relayed.windowView(view([row(101), row(102)]))?.rows.map((r) => r.id), [101, 102]);
});

test("later views without rows keep the last ones; an emptied list applies at once", () => {
  const relayed = new RelayedRows();
  relayed.windowView(view([row(101), row(102)]));
  assert.deepEqual(relayed.windowView(view(null))?.rows.map((r) => r.id), [101, 102]);
  assert.deepEqual(relayed.windowView(view([], 2))?.rows, []);
  assert.deepEqual(relayed.windowView(view(null, 2))?.rows, []);
});
