// Ports of the fixed Java geometry (cb53acc):
//  - InFrameLayout.layout, Normal mode with default panels (InFrameLayout.java:262-472)
//  - BoardRenderer.calculatePixelMargins (BoardRenderer.java:3678-3746) and re-centering (:743)
//  - SubBoardRenderer.calculatePixelMargins (SubBoardRenderer.java:1353-1422)

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface Panels {
  board: Rect;
  captured: Rect;
  stat: Rect;
  graph: Rect;
  comment: Rect;
  ponderingY: number;
  tree: Rect;
  table: Rect;
  sub: Rect;
  /** Frosted background containers (LizzieFrame.java:7491-7529), in Java's paint order. */
  glass: Rect[];
}

const i = Math.trunc;

/** Default request: showComment, showSubBoard, showListPane, showVariationGraph, showStatus. */
export function inFrameLayout(width: number, height: number): Panels {
  const maxBound = Math.max(width, height);
  let maxSize = Math.min(width, height);
  maxSize = Math.max(maxSize, 19 + 5);
  const panelMargin = i(maxSize * 0.02);
  const leftoverWidth = width - maxSize;
  const boardX = i(leftoverWidth / 8) * 4;
  const boardY = i((height - maxSize) / 2);
  const capx = 0;
  const capy = 0;
  const capw = boardX - panelMargin;
  const caph = boardY + i(maxSize / 8);
  const statx = capx;
  const staty = capy + caph;
  const statw = capw;
  const stath = i(maxSize / 10);
  const gry = staty + stath;
  const grh = i(maxSize / 3);
  const vx = boardX + maxSize + panelMargin;
  const vy = capy;
  const vw = width - vx;
  const vh = height - vy;
  const ponderingY = height - i(maxSize * 0.023) - i(maxBound * 0.04);

  let subBoardY = gry + grh;
  const subBoardWidth = statw;
  const subBoardHeight = ponderingY - subBoardY;
  let subBoardLength = Math.min(subBoardWidth, subBoardHeight);
  let subBoardX = statx + i((statw - subBoardLength) / 2);

  let treeh = i(vh / 2);
  const cx = subBoardWidth > subBoardHeight ? subBoardX - i((subBoardWidth - subBoardHeight) / 2) : subBoardX;
  const cy = subBoardY;
  const cw = subBoardWidth;
  const ch = subBoardHeight;
  // comment takes the left slot; sub board moves to the right column
  subBoardX = vx;
  subBoardY = vy + treeh;
  subBoardLength = Math.min(vw, treeh);
  treeh = treeh + i(vh / 2) - subBoardLength;
  subBoardY = Math.max(subBoardY + i(vh / 2) - subBoardLength, vy);
  const treeContainer = { x: vx, y: vy, w: vw, h: treeh };
  // tree and candidate table each take treeh / 2 (LizzieFrame.java:7613-7616)
  treeh = i(treeh / 2);
  return {
    board: { x: boardX, y: boardY, w: maxSize, h: maxSize },
    captured: { x: capx, y: capy, w: capw, h: caph },
    stat: { x: statx, y: staty, w: statw, h: stath },
    graph: { x: statx, y: gry, w: statw, h: grh },
    comment: { x: cx, y: cy, w: cw, h: ch },
    ponderingY,
    tree: { x: vx, y: vy, w: vw, h: treeh },
    table: { x: vx, y: vy + treeh, w: vw, h: treeh },
    sub: { x: subBoardX, y: subBoardY, w: subBoardLength, h: subBoardLength },
    glass: [
      { x: statx, y: staty, w: statw, h: stath + grh },
      treeContainer,
      { x: cx, y: cy, w: cw, h: ch },
      { x: capx, y: capy, w: capw, h: caph },
    ],
  };
}

export interface BoardGeom {
  /** Wood rectangle relative to the panel the board was laid out in. */
  x: number;
  y: number;
  size: number;
  margin: number;
  square: number;
  stoneRadius: number;
  /** Larger board dimension; the grid spacing is computed from it. */
  n: number;
  cols: number;
  rows: number;
}

function pixelMargins(length: number, n: number, ratio: number): { size: number; margin: number; avail: number } {
  const marginRatio = (ratio / n) * 19.0;
  let size = length + 1;
  let margin: number;
  let avail: number;
  do {
    size--;
    margin = i(marginRatio * size);
    avail = size - 2 * margin;
  } while ((avail - 1) % (n - 1) !== 0);
  return { size, margin, avail };
}

/** Board geometry; `ratio` is the margin ratio (main board with coordinates 0.055, sub board 0.035).
 *  Rectangular boards use the spacing of their larger dimension. */
export function boardGeom(r: Rect, cols: number, rows: number, ratio = 0.055): BoardGeom {
  const n = Math.max(cols, rows);
  const { size, margin, avail } = pixelMargins(Math.min(r.w, r.h), n, ratio);
  const square = i((avail - 1) / (n - 1));
  const stoneRadius = square < 4 ? 1 : i(square / 2) - 1;
  return { x: r.x + i((r.w - size) / 2), y: r.y + i((r.h - size) / 2), size, margin, square, stoneRadius, n, cols, rows };
}

/** WinrateGraph.java:585-678,2755-2761. Shared by painting and move-column navigation. */
export function graphGeom(width: number, height: number, lineNodes: number) {
  return { width: i((width - 6) * 0.98), height: height - 4, slots: Math.max(50, lineNodes - 2) };
}
