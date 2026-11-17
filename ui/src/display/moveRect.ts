// 显示落子矩形 (LizzieFrame.shouldShowRect / onMouseMoved / onMouseExited, LizzieFrame.java:9950-9973,
// 10144-10182; IndependentMainBoard.java:584-606): a square on the intersection under the pointer
// where the side to play would put its stone. Each board tracks its own pointer; leaving the board
// removes the square. 双引擎模式's right board never keeps one (LizzieFrame.java:10160-10162).
import type { Point, WorkbenchView } from "../contract/generated";

export type RectBoard = "main" | "independent";

const pointers: Record<RectBoard, Point | null> = { main: null, independent: null };

/** The pointer is over `point` of `board` (null: off its grid). True when that changed. */
export function rectPointer(board: RectBoard, point: Point | null): boolean {
  const old = pointers[board];
  if (old === point || (old && point && old.x === point.x && old.y === point.y)) return false;
  pointers[board] = point;
  return true;
}

/** shouldShowRect: 始终; 仅对局时 in a game against the engine, once it is loaded, on the human's
 *  turn; never while scoring (isInScoreMode). */
function shown(v: WorkbenchView): boolean {
  if (v.score) return false;
  switch (v.settings.edit.moveRect) {
    case "Always":
      return true;
    case "OnPlay": {
      const g = v.game;
      return !!g && g.phase !== "Starting" && g.phase !== "Ending" && v.position.to_play === g.human;
    }
    case "Never":
      return false;
  }
}

/** The square `board` shows, for its scene. */
export function moveRect(v: WorkbenchView, board: RectBoard): { point: Point; black: boolean } | null {
  const point = pointers[board];
  return point && shown(v) ? { point, black: v.position.to_play === "Black" } : null;
}
