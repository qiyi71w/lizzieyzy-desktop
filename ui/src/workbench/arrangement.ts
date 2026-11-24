// The panel 排列 (GLOSSARY.md) as geometry: the Java in-frame board and column widths
// (layout.inFrame) with each 停靠栏's shown panels stacked by weight, or the one reason it cannot
// apply. Pure functions of their arguments, so a draft or a hypothetical arrangement and any
// workbench size are laid out the same way as the committed one.
import type { ArrangeablePanel, ArrangedPanel, ExtraMode, PanelArrangement } from "../contract/generated";
import { CARD_INSET, edgeZone, inFrame, type InFrame, type LayoutRequest, type Panels, type Rect } from "./layout";

/** Why an arrangement shows as the Java layout: the layout mode is not 默认 / 经典 / 精简, 放大小棋盘
 *  or 放大胜率图 is on, the window is portrait (no width beside the board), or a shown panel would
 *  get less than its minimum height or no width (its column has none, the board pushed against
 *  that side). */
export type ArrangementFallback = "mode" | "enlarged" | "portrait" | "fit";

/** The least height of each panel (CSS px of its slot before the card inset, not scaled with the
 *  interface font size). Comment and list are Java's MIN_COMMENT_HEIGHT and
 *  MIN_CANDIDATE_TABLE_HEIGHT; the others were measured where their content stays recognisable.
 *  Winrate's is the graph below the statistics strip, which keeps Java's height (a tenth of the
 *  board); the sub board's is its section, whose height never exceeds the column width. */
export const MIN_HEIGHT: Readonly<Record<ArrangeablePanel, number>> = {
  Captured: 40,
  Winrate: 60,
  Comment: 68,
  SubBoard: 110,
  VariationGraph: 48,
  CandidateList: 36,
};

/** A draggable line between two neighbouring shown panels of a column. `y` is the lower panel's
 *  top; `zone` is the pointer's hot area along the card's hairline, inside the card. */
export interface ArrangementDivider {
  /** Stable `data-name`. */
  name: string;
  column: "left" | "right";
  upper: ArrangeablePanel;
  lower: ArrangeablePanel;
  y: number;
  zone: Rect;
}

export type ArrangedOutcome = { panels: Panels; fallback: null } | { panels: null; fallback: ArrangementFallback };

const NONE: Rect = { x: 0, y: 0, w: 0, h: 0 };
const ZONE = 6;

function panelShown(r: LayoutRequest, panel: ArrangeablePanel): boolean {
  switch (panel) {
    case "Captured": return r.showCaptured;
    case "Winrate": return r.showWinrateGraph;
    case "Comment": return r.showComment;
    case "SubBoard": return r.showSubBoard;
    case "VariationGraph": return r.showVariationGraph;
    case "CandidateList": return r.showListPane;
  }
}

/** A column's slot on the workbench and its shown panels, top to bottom. */
interface Column {
  side: "left" | "right";
  rect: Rect;
  shown: ArrangedPanel[];
}

interface Frame {
  frame: InFrame;
  columns: [Column, Column];
  locked: boolean;
}

/** The in-frame geometry an arrangement can use, or why it cannot: 精简模式 lays the board out
 *  without the dragged shares (useLockedInFrameLayout), as workbenchLayout does. */
function frameFor(width: number, height: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement): Frame | ArrangementFallback {
  if (mode !== "Normal" && mode !== "Min") return "mode";
  if ((r.showSubBoard && r.largeSubBoard) || (r.showWinrateGraph && r.largeWinrateGraph)) return "enlarged";
  const locked = mode === "Normal";
  const frame = inFrame(width, height, locked ? r : { ...r, leftoverLeftShare: null, commentHeightShare: null, variationGraphShare: null });
  if (frame.leftoverWidth <= 0) return "portrait";
  const column = (side: "left" | "right", rect: Rect, panels: ArrangedPanel[]): Column => ({ side, rect, shown: panels.filter((p) => panelShown(r, p.panel)) });
  const columns: [Column, Column] = [column("left", frame.left, a.left), column("right", frame.right, a.right)];
  if (columns.some((c) => c.shown.length > 0 && c.rect.w <= 0)) return "fit";
  return { frame, locked, columns };
}

/** The shown panels' heights, unrounded: shares of the column by weight, except that the sub
 *  board is never taller than the column is wide; what it does not take goes to the others by
 *  weight, and alone in a column it keeps only its square. */
function heights(shown: readonly ArrangedPanel[], colW: number, colH: number): number[] {
  const sub = shown.findIndex((p) => p.panel === "SubBoard");
  const side = Math.max(0, Math.min(colW, colH));
  if (sub >= 0 && shown.length === 1) return [side];
  const total = shown.reduce((sum, p) => sum + p.weight, 0);
  if (sub >= 0 && (colH * shown[sub].weight) / total > side) {
    const rest = total - shown[sub].weight;
    return shown.map((p, k) => (k === sub ? side : ((colH - side) * p.weight) / rest));
  }
  return shown.map((p) => (colH * p.weight) / total);
}

/** Each shown panel's top and height in whole pixels (the rounded running total of `heights`) and
 *  its unrounded height, which the minimum is checked against. */
function stack(c: Column): { y: number; h: number; exact: number }[] {
  let sum = 0;
  return heights(c.shown, c.rect.w, c.rect.h).map((exact) => {
    const y = c.rect.y + Math.round(sum);
    sum += exact;
    return { y, h: c.rect.y + Math.round(sum) - y, exact };
  });
}

function minHeight(panel: ArrangeablePanel, statH: number): number {
  return panel === "Winrate" ? statH + MIN_HEIGHT.Winrate : MIN_HEIGHT[panel];
}

/** The workbench with `a` applied at this size, or the reason it shows the Java layout. */
export function arrangedLayout(width: number, height: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement): ArrangedOutcome {
  const f = frameFor(width, height, r, mode, a);
  if (typeof f === "string") return { panels: null, fallback: f };
  const { frame, columns } = f;
  const rects = new Map<ArrangeablePanel, Rect>();
  const dividers: ArrangementDivider[] = [];
  for (const c of columns) {
    const placed = stack(c);
    for (const [k, p] of c.shown.entries()) {
      if (placed[k].exact < minHeight(p.panel, frame.statH) - 1e-6) return { panels: null, fallback: "fit" };
      rects.set(p.panel, { x: c.rect.x, y: placed[k].y, w: c.rect.w, h: placed[k].h });
      // A column too narrow for the zone inside its card has no divider (nothing to press).
      if (k === 0 || c.rect.w - 2 * CARD_INSET < 1) continue;
      const upper = c.shown[k - 1].panel;
      const y = placed[k].y;
      dividers.push({
        name: `panel-divider-${upper}-${p.panel}`,
        column: c.side,
        upper,
        lower: p.panel,
        y,
        zone: { x: c.rect.x + CARD_INSET, y: y - ZONE / 2, w: c.rect.w - 2 * CARD_INSET, h: ZONE },
      });
    }
  }
  const rect = (panel: ArrangeablePanel) => rects.get(panel) ?? NONE;
  const winrate = rect("Winrate");
  const statH = Math.min(frame.statH, winrate.h);
  const { board, leftoverWidth } = frame;
  // The board's side zones stay; the drag maps of the two Java seams the dividers replace are unused.
  const handles = f.locked
    ? {
        left: edgeZone(columns[0].shown.length > 0, true, { ...frame.left, h: height }, board, leftoverWidth),
        right: edgeZone(columns[1].shown.length > 0, false, frame.right, board, leftoverWidth),
        commentTop: null,
        variationList: null,
        leftoverWidth,
        boardX: board.x,
        boardWidth: board.w,
        commentSplitBottom: 0,
        commentSplitH: 0,
        commentSeamY: 0,
        variationContainerH: 0,
        variationTop: 0,
        variationSeamY: 0,
      }
    : null;
  return {
    fallback: null,
    panels: {
      board,
      captured: rect("Captured"),
      stat: rects.has("Winrate") ? { ...winrate, h: statH } : NONE,
      graph: rects.has("Winrate") ? { ...winrate, y: winrate.y + statH, h: winrate.h - statH } : NONE,
      comment: rect("Comment"),
      ponderingY: frame.ponderingY,
      tree: rect("VariationGraph"),
      table: rect("CandidateList"),
      sub: rect("SubBoard"),
      // The status lines stay where Java has them, under the left column and as wide as it.
      status: { x: frame.left.x, y: frame.ponderingY, w: Math.max(0, frame.left.w), h: height - frame.ponderingY },
      glass: [],
      shown: {
        captured: r.showCaptured,
        stat: r.showWinrateGraph,
        graph: r.showWinrateGraph,
        comment: r.showComment,
        status: r.showStatus,
        tree: r.showVariationGraph,
        table: r.showListPane,
        sub: r.showSubBoard,
      },
      handles,
      arranged: { dividers },
    },
  };
}

/** The least t in (0, 1) for which `pred` holds, `pred` being false then true along t; 1 when it
 *  never does. */
function first(pred: (t: number) => boolean): number {
  let [lo, hi] = [0, 1];
  for (let n = 0; n < 48; n++) {
    const mid = (lo + hi) / 2;
    if (pred(mid)) hi = mid;
    else lo = mid;
  }
  return hi;
}

/** How close to 0 and 1 a share is probed, and the least height change that is not rounding. */
const EDGE = 1e-9;

interface DividerShares {
  /** The index of the upper panel in its column's shown panels. */
  k: number;
  /** Upper's share of the pair's weight in `a`. */
  now: number;
  /** The column's heights and the divider's y with upper's share t of the pair's weight. */
  at(t: number): { heights: number[]; divider: number };
  /** The shares the divider can take: every shown panel of the column at least its minimum height
   *  (with what the sub board does not take given to the others), and the sub board given no
   *  more weight than brings it to the column width or than it has now. Null when there is none. */
  range: [number, number] | null;
  /** `a` with upper's share t of the pair's weight. */
  apply(t: number): PanelArrangement;
}

/** The divider below `upper` of `a` at this size as upper's share t of the two panels' weight
 *  (their sum stays), or null when the arrangement does not apply or `upper` has no divider below
 *  it. */
function dividerShares(width: number, height: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement, upper: ArrangeablePanel): DividerShares | null {
  const f = frameFor(width, height, r, mode, a);
  if (typeof f === "string") return null;
  const c = f.columns.find((col) => col.shown.some((p) => p.panel === upper));
  const k = c ? c.shown.findIndex((p) => p.panel === upper) : -1;
  if (!c || k < 0 || k + 1 >= c.shown.length) return null;
  const lower = c.shown[k + 1].panel;
  const sum = c.shown[k].weight + c.shown[k + 1].weight;
  const now = c.shown[k].weight / sum;
  const at = (t: number) => {
    const shown = c.shown.map((p, j) => (j === k ? { ...p, weight: sum * t } : j === k + 1 ? { ...p, weight: sum * (1 - t) } : p));
    const hs = heights(shown, c.rect.w, c.rect.h);
    return { heights: hs, divider: c.rect.y + hs.slice(0, k + 1).reduce((s, h) => s + h, 0) };
  };
  // Every height is monotone in t: upper's grows and lower's shrinks; the others stay, except
  // that when the sub board is one of the two and at the column width they share what it leaves,
  // which grows with t when it is the upper one and shrinks when it is the lower. So each
  // panel's minimum bounds t on one side, at the edge of a monotone predicate.
  let [lo, hi] = [0, 1];
  const [h0, h1] = [at(EDGE).heights, at(1 - EDGE).heights];
  for (const [j, p] of c.shown.entries()) {
    const min = minHeight(p.panel, f.frame.statH);
    if (h1[j] - h0[j] > EDGE) lo = Math.max(lo, first((t) => at(t).heights[j] >= min));
    else if (h0[j] - h1[j] > EDGE) hi = Math.min(hi, first((t) => at(t).heights[j] < min));
    else if (h0[j] < min) lo = 1;
  }
  // A sub board at the column width: more weight would only be given back to its neighbours.
  // Weight it already has beyond that stays within reach, so the current share is never cut off.
  const side = Math.max(0, Math.min(c.rect.w, c.rect.h));
  if (upper === "SubBoard") hi = Math.min(hi, Math.max(now, first((t) => at(t).heights[k] >= side)));
  if (lower === "SubBoard") lo = Math.max(lo, Math.min(now, first((t) => at(t).heights[k + 1] < side)));
  const apply = (t: number): PanelArrangement => {
    const weight = (p: ArrangedPanel): ArrangedPanel =>
      p.panel === upper ? { ...p, weight: sum * t } : p.panel === lower ? { ...p, weight: sum * (1 - t) } : p;
    return { left: a.left.map(weight), right: a.right.map(weight) };
  };
  return { k, now, at, range: lo < 1 && lo <= hi ? [lo, hi] : null, apply };
}

/** `a` with the divider below `upper` moved to `y` (workbench coordinates) at this size: only the
 *  two panels beside it change weight and their sum stays. The divider stops where any panel of
 *  its column would go below its minimum height and where the sub board would grow past the
 *  column width. Unchanged when the divider cannot go towards `y` at all, the arrangement does
 *  not apply or `upper` has no divider below it. */
export function moveDivider(width: number, height: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement, upper: ArrangeablePanel, y: number): PanelArrangement {
  const d = dividerShares(width, height, r, mode, a, upper);
  if (!d?.range) return a;
  const [lo, hi] = d.range;
  const t = Math.max(lo, Math.min(hi, first((s) => d.at(s).divider >= y)));
  // A share that leaves the divider where it is (a sub board at the column width keeps it there)
  // would only move the heights of the panels around it.
  return Math.abs(d.at(t).divider - d.at(d.now).divider) < EDGE ? a : d.apply(t);
}

/** `a` with the two panels beside the divider below `upper` as near the same height as that
 *  divider can bring them at this size (moveDivider's limits), so the result always fits and is
 *  never further from even than `a`: 布局编辑模式's double-click. */
export function equalizeDivider(width: number, height: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement, upper: ArrangeablePanel): PanelArrangement {
  const d = dividerShares(width, height, r, mode, a, upper);
  if (!d?.range) return a;
  const [lo, hi] = d.range;
  // Upper's height less lower's grows with t, so the share nearest to even is its zero clamped.
  const even = first((t) => {
    const hs = d.at(t).heights;
    return hs[d.k] >= hs[d.k + 1];
  });
  const t = Math.max(lo, Math.min(hi, even));
  return Math.abs(t - d.now) < EDGE ? a : d.apply(t);
}
