// Field-wise equality of the analysis parts of consecutive views, so panels repaint only when
// what they show changed. Every field of the contract types needs a comparator: a new field fails
// the type check until it is compared here.
import type { CandidateView, HistoryPoint, NodeAnalysisView, Point } from "../contract/generated";

type Fields<T> = { [K in keyof T]-?: (a: T[K], b: T[K]) => boolean };

const same = (a: unknown, b: unknown) => a === b;

function sameBy<T>(fields: Fields<T>, a: T, b: T): boolean {
  for (const k in fields) if (!fields[k](a[k], b[k])) return false;
  return true;
}

function sameList<T>(item: (a: T, b: T) => boolean) {
  return (a: readonly T[], b: readonly T[]) => {
    if (a === b) return true;
    if (a.length !== b.length) return false;
    for (let k = 0; k < a.length; k++) if (!item(a[k], b[k])) return false;
    return true;
  };
}

const samePoint = (a: Point, b: Point) => a.x === b.x && a.y === b.y;
const sameMove = (a: Point | null, b: Point | null) => a === b || (a !== null && b !== null && samePoint(a, b));

const CANDIDATE: Fields<CandidateView> = {
  point: sameMove,
  visits: same,
  allocation_visits: same,
  winrate: same,
  score_lead: same,
  score_stdev: same,
  prior: same,
  lcb: same,
  order: same,
  pv: sameList(sameMove),
  pv_captures: sameList(sameList(samePoint)),
};
const sameCandidates = sameList((a: CandidateView, b: CandidateView) => sameBy(CANDIDATE, a, b));

const ANALYSIS: Fields<NodeAnalysisView> = {
  node: same,
  engine: same,
  to_play: same,
  visits: same,
  root_visits: same,
  winrate: same,
  score_lead: same,
  score_stdev: same,
  komi: same,
  candidates: sameCandidates,
};

const HISTORY_POINT: Fields<HistoryPoint> = {
  node: same,
  move_number: same,
  to_play: same,
  visits: same,
  winrate: same,
  score_lead: same,
};

export function sameAnalysis(a: NodeAnalysisView | null, b: NodeAnalysisView | null): boolean {
  return a === b || (a !== null && b !== null && sameBy(ANALYSIS, a, b));
}

export const sameHistory = sameList((a: HistoryPoint, b: HistoryPoint) => sameBy(HISTORY_POINT, a, b));
