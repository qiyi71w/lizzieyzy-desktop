// 点击棋子回看 (Board.setPressStoneInfo / clearPressStoneInfo, Board.java:7306-7387) and 右键 回看
// (RightClickMenu2.java:159-166): display state only. A press on a stone, held 50 ms, shows the
// position before the latest move played there (the core's ClickReview answer) with its best
// variation growing one move every 分支回放间隔; moving to another point at least 300 ms after the
// press, leaving the board or another position on the board ends it. The record, the current node
// and the analysis are never touched.
import { registerViewHandler, run, state } from "../app";
import type { ClickReviewView, Point, WorkbenchView } from "../contract/generated";
import type { BoardScene } from "../workbench/paint";

interface Review {
  point: Point;
  /** The position on the board when it was pressed (document, revision and node). */
  at: string;
  startedAt: number;
  /** The answer, once looked up; null while pending or when no move was played there. */
  view: ClickReviewView | null;
  /** Board.reviewLength: the variation moves shown. */
  length: number;
}

let review: Review | null = null;
let timer: number | undefined;
let repaint: () => void = () => {};

export function bindClickReview(schedule: () => void): void {
  repaint = schedule;
}

function positionKey(v: WorkbenchView): string {
  return `${v.document.id}:${v.document.revision}:${v.position.node}`;
}

function stop(): void {
  window.clearTimeout(timer);
  window.clearInterval(timer);
  timer = undefined;
  const shown = review?.view !== null && review?.view !== undefined;
  review = null;
  if (shown) repaint();
}

/** setPressStoneInfo: a left press needs 启用点击棋子回看, 右键 回看 does not. */
export function pressStone(point: Point, fromRightClick: boolean): void {
  const v = state.view;
  if (!v || (!fromRightClick && !v.settings.edit.clickReview)) return;
  stop();
  const r: Review = { point, at: positionKey(v), startedAt: performance.now(), view: null, length: 1 };
  review = r;
  timer = window.setTimeout(() => {
    timer = undefined;
    void run({ type: "ClickReview", point }).then((result) => {
      const now = state.view;
      if (review !== r || !now || positionKey(now) !== r.at || result?.type !== "ClickReview" || !result.review) {
        if (review === r) review = null;
        return;
      }
      r.view = result.review;
      repaint();
      // startReviewThread: (int) (replayBranchIntervalSeconds * 1000) between steps.
      const best = result.review.analysis?.candidates[0];
      const steps = best?.pv.length ?? 0;
      if (steps > 1) {
        timer = window.setInterval(() => {
          if (review !== r) return;
          r.length++;
          if (r.length >= steps) {
            window.clearInterval(timer);
            timer = undefined;
          }
          repaint();
        }, Math.trunc(now.settings.display.replayIntervalSeconds * 1000));
      }
    });
  }, 50);
}

/** clearPressStoneInfo: the pointer at `point` (null: off the board) ends the review once it
 *  is on another point and 300 ms have passed since the press. */
export function pointerAt(point: Point | null): void {
  if (!review) return;
  if (point && point.x === review.point.x && point.y === review.point.y) return;
  if (performance.now() - review.startedAt < 300) return;
  stop();
}

// Another document, an edit or navigation ends it (the history operations' clearPressStoneInfo).
registerViewHandler((view) => {
  if (review && positionKey(view) !== review.at) stop();
});

/** The board while a review shows (BoardRenderer.java:313-326): the reviewed position, its best
 *  variation up to the review length, then its 落子评价 marks or only its last move's mark;
 *  nothing else of the current position. Null while no review shows. */
export function reviewScene(scene: BoardScene): BoardScene | null {
  const r = review?.view;
  if (!r) return null;
  const best = r.analysis?.candidates[0];
  return {
    ...scene,
    stones: r.stones,
    lastAction: r.last_action,
    toPlay: r.to_play,
    snapshot: r.snapshot,
    marks: [],
    nextMoves: [],
    nextMainMove: null,
    nextColor: null,
    analysis: r.analysis,
    overlay: { ...scene.overlay, move_numbers: [], ranks: r.ranks ?? [], ranks_after_numbers: false, next_move: null },
    candidatesShown: false,
    policy: false,
    rawNetOn: false,
    rawNet: null,
    hovered: null,
    // Branch.Input with recordVisits false: no visits on the review's variation.
    preview: best ? { order: best.order, pv: best.pv, pv_captures: best.pv_captures, pv_visits: [], length: Math.min(review!.length, best.pv.length) } : null,
    ownership: null,
    score: null,
    mark: null,
    region: null,
    selecting: null,
    focus: null,
    priority: [],
    moveRect: null,
    review: r.ranks ? "ranks" : "last",
  };
}

/** What the acceptance probe reports. */
export function clickReviewReport() {
  return review ? { point: review.point, node: review.view?.node ?? null, length: review.length } : null;
}
