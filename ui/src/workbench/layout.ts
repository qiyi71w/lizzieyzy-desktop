// Ports of the fixed Java geometry (cb53acc):
//  - InFrameLayout.layout with every panel flag and the persisted shares (InFrameLayout.java:262-472)
//  - the 放大小棋盘 / 放大胜率图 landscape layout of LizzieFrame.paintMainPanel (LizzieFrame.java:7255-7692)
//  - InFrameLeftoverDragHandles' hot zones (InFrameLeftoverDragHandles.java:61-162)
//  - BoardRenderer.calculatePixelMargins (BoardRenderer.java:3678-3746) and re-centering (:743)
//  - SubBoardRenderer.calculatePixelMargins (SubBoardRenderer.java:1353-1422)

export interface Rect {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** The in-frame panels of the Normal mode workbench. */
export type PanelId = "captured" | "stat" | "graph" | "comment" | "status" | "tree" | "table" | "sub";

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
  /** Which panels Java draws for this request. */
  shown: Record<PanelId, boolean>;
  /** The separator drag zones; only the locked in-frame layout has them. */
  handles: Handles | null;
}

/** What the layout depends on (LizzieFrame.java:7164-7228). */
export interface LayoutRequest {
  cols: number;
  rows: number;
  showComment: boolean;
  showSubBoard: boolean;
  showVariationGraph: boolean;
  /** Config.showListPane(): the preference while the mode and 大胜率图 allow the list. */
  showListPane: boolean;
  showStatus: boolean;
  showCaptured: boolean;
  showWinrateGraph: boolean;
  /** Config.largeSubBoard and largeWinrateGraph, the stored flags. */
  largeSubBoard: boolean;
  largeWinrateGraph: boolean;
  boardPositionProportion: number;
  leftoverLeftShare: number | null;
  commentHeightShare: number | null;
  variationGraphShare: number | null;
}

/** The drag zones and the values a drag maps the pointer through (InFrameLeftoverDragHandles.update). */
export interface Handles {
  left: Rect | null;
  right: Rect | null;
  commentTop: Rect | null;
  variationList: Rect | null;
  leftoverWidth: number;
  boardX: number;
  boardWidth: number;
  commentSplitBottom: number;
  commentSplitH: number;
  commentSeamY: number;
  variationContainerH: number;
  variationTop: number;
  variationSeamY: number;
}

const i = Math.trunc;
const NONE: Rect = { x: 0, y: 0, w: 0, h: 0 };
const MIN_COMMENT_HEIGHT = 68;
const MIN_CANDIDATE_TABLE_HEIGHT = 36;

/** The Normal mode workbench: the locked in-frame layout unless 放大小棋盘 or 放大胜率图 shows
 *  (useLockedInFrameLayout, LizzieFrame.java:7176-7180); the drag zones exist in landscape only. */
export function workbenchLayout(width: number, height: number, r: LayoutRequest): Panels {
  const largeSub = r.showSubBoard && r.largeSubBoard;
  const largeWinrate = r.showWinrateGraph && r.largeWinrateGraph;
  if (largeSub || largeWinrate) return enlargedLayout(width, height, r, largeSub, largeWinrate);
  return inFrameLayout(width, height, r, width >= height);
}

function inFrameLayout(width: number, height: number, r: LayoutRequest, handles: boolean): Panels {
  const maxBound = Math.max(width, height);
  const maxSize = Math.max(Math.min(width, height), Math.max(r.cols, r.rows) + 5);
  const panelMargin = i(maxSize * 0.02);
  const leftoverWidth = width - maxSize;
  let boardX = i(leftoverWidth / 8) * r.boardPositionProportion;
  if (r.leftoverLeftShare !== null) {
    const minBoardX = panelMargin;
    const maxBoardX = width - maxSize - panelMargin;
    const raw = Math.round(leftoverWidth * r.leftoverLeftShare);
    if (minBoardX <= maxBoardX) boardX = Math.max(minBoardX, Math.min(maxBoardX, raw));
  }
  const boardY = i((height - maxSize) / 2);
  const capx = 0;
  const capy = 0;
  const capw = boardX - panelMargin;
  let caph = boardY + i(maxSize / 8);
  const statx = capx;
  let staty = capy + caph;
  const statw = capw;
  let stath = i(maxSize / 10);
  let gry = staty + stath;
  let grh = i(maxSize / 3);
  const vx = boardX + maxSize + panelMargin;
  const vy = capy;
  const vw = width - vx;
  const vh = height - vy;
  let ponderingY = Math.max(0, height);
  if (r.showStatus) ponderingY = ponderingY - i(maxSize * 0.023) - i(maxBound * 0.04);

  let subBoardY = gry + grh;
  const subBoardWidth = statw;
  const subBoardHeight = ponderingY - subBoardY;
  let subBoardLength = Math.min(subBoardWidth, subBoardHeight);
  let subBoardX = statx + i((statw - subBoardLength) / 2);

  let treeh = vh;
  let cx = vx;
  let cy = vy;
  let cw = vw;
  let ch = vh;
  const variations = r.showVariationGraph || r.showListPane;
  if (r.showComment) {
    // The comment takes the left slot; the sub board moves to the right column.
    if (variations) {
      treeh = i(vh / 2);
      cy = vy + treeh;
      ch = treeh;
    }
    const [tempx, tempy, tempw, temph] = [cx, cy, cw, ch];
    cx = subBoardWidth > subBoardHeight ? subBoardX - i((subBoardWidth - subBoardHeight) / 2) : subBoardX;
    cy = subBoardY;
    cw = subBoardWidth;
    ch = subBoardHeight;
    subBoardX = tempx;
    subBoardY = tempy;
    subBoardLength = Math.min(tempw, temph);
  }
  if (r.showSubBoard && r.showComment) {
    treeh = treeh + i(vh / 2) - subBoardLength;
    subBoardY = variations ? subBoardY + i(vh / 2) - subBoardLength : subBoardY + vh - subBoardLength;
    subBoardY = Math.max(subBoardY, vy);
  }
  if (variations && r.showSubBoard !== r.showComment) treeh = vh;

  const treeContainerH = treeh;
  let table = NONE;
  if (r.showListPane) {
    if (r.showVariationGraph) {
      const containerH = treeh;
      const maxVar = containerH - MIN_CANDIDATE_TABLE_HEIGHT;
      if (r.variationGraphShare !== null && maxVar >= 0) {
        treeh = Math.max(0, Math.min(maxVar, Math.round(containerH * r.variationGraphShare)));
        table = { x: vx, y: vy + treeh, w: vw, h: containerH - treeh };
      } else {
        // tree and candidate table each take half (LizzieFrame.java:7613-7616)
        treeh = i(containerH / 2);
        table = { x: vx, y: vy + treeh, w: vw, h: treeh };
      }
    } else table = { x: vx, y: vy, w: vw, h: treeh };
  }

  if (r.showComment && r.commentHeightShare !== null && ch > 0) {
    const splitBottom = cy + ch;
    const splitH = splitBottom - capy;
    if (MIN_COMMENT_HEIGHT <= splitH) {
      ch = Math.max(MIN_COMMENT_HEIGHT, Math.min(splitH, Math.round(splitH * r.commentHeightShare)));
      cy = splitBottom - ch;
      const aboveH = cy - capy;
      caph = Math.min(caph, Math.max(0, aboveH));
      stath = Math.min(stath, Math.max(0, aboveH - caph));
      grh = Math.max(0, aboveH - caph - stath);
      staty = capy + caph;
      gry = staty + stath;
    }
  }

  const board = { x: boardX, y: boardY, w: maxSize, h: maxSize };
  const comment = r.showComment ? { x: cx, y: cy, w: cw, h: ch } : NONE;
  const tree = r.showVariationGraph ? { x: vx, y: vy, w: vw, h: treeh } : NONE;
  const captured = { x: capx, y: capy, w: capw, h: caph };
  const glass: Rect[] = [];
  if (r.showWinrateGraph) glass.push({ x: statx, y: staty, w: statw, h: stath + grh });
  if (variations) glass.push({ x: vx, y: vy, w: vw, h: treeContainerH });
  if (r.showComment) glass.push(comment);
  if (r.showCaptured) glass.push(captured);
  return {
    board,
    captured,
    stat: { x: statx, y: staty, w: statw, h: stath },
    graph: { x: statx, y: gry, w: statw, h: grh },
    comment,
    ponderingY,
    tree,
    table,
    sub: r.showSubBoard ? { x: subBoardX, y: subBoardY, w: subBoardLength, h: subBoardLength } : NONE,
    glass,
    shown: {
      captured: r.showCaptured,
      stat: r.showWinrateGraph,
      graph: r.showWinrateGraph,
      comment: r.showComment,
      status: r.showStatus,
      tree: r.showVariationGraph,
      table: r.showListPane && table.w >= 10 && table.h >= 5,
      sub: r.showSubBoard,
    },
    handles: handles
      ? dragHandles(r, board, { x: capx, y: capy, w: capw, h: height - capy }, { x: vx, y: vy, w: vw, h: vh }, comment, tree, table, captured, leftoverWidth)
      : null,
  };
}

/** InFrameLeftoverDragHandles.update / place / placeHorizontal at zoom 1, in panel coordinates. */
function dragHandles(r: LayoutRequest, board: Rect, leftColumn: Rect, rightColumn: Rect, comment: Rect, tree: Rect, table: Rect, captured: Rect, leftoverWidth: number): Handles {
  const leftOccupied = r.showCaptured || r.showWinrateGraph || r.showComment || r.showSubBoard;
  const rightOccupied = r.showVariationGraph || r.showListPane || (r.showSubBoard && r.showComment);
  const edge = (present: boolean, leftEdge: boolean, column: Rect): Rect | null => {
    if (!present || leftoverWidth <= 0) return null;
    let x: number;
    let w: number;
    if (leftEdge) {
      const hot = Math.max(6, board.x - (column.x + column.w) + 4);
      x = Math.max(column.x, board.x - hot);
      w = board.x - x;
    } else {
      const boardRight = board.x + board.w;
      x = boardRight;
      w = Math.min(Math.max(6, column.x - boardRight + 4), column.x + column.w - x);
    }
    return w < 1 || board.h < 1 ? null : { x, y: board.y, w, h: board.h };
  };
  const seam = (dividerY: number | null, along: Rect): Rect | null => {
    if (dividerY === null || along.w < 1) return null;
    let y = dividerY - 2;
    let h = 6;
    if (y < 0) {
      h += y;
      y = 0;
    }
    return h < 1 ? null : { x: along.x, y, w: along.w, h };
  };
  return {
    left: edge(leftOccupied, true, leftColumn),
    right: edge(rightOccupied, false, rightColumn),
    commentTop: seam(comment.h > 0 ? comment.y : null, comment),
    variationList: seam(r.showVariationGraph && r.showListPane ? table.y : null, table),
    leftoverWidth,
    boardX: board.x,
    boardWidth: board.w,
    commentSplitBottom: comment.y + comment.h,
    commentSplitH: comment.h > 0 ? comment.y + comment.h - captured.y : 0,
    commentSeamY: comment.y,
    variationContainerH: tree.h + table.h,
    variationTop: tree.h > 0 ? tree.y : table.y,
    variationSeamY: table.y,
  };
}

/** 放大小棋盘 / 放大胜率图 in landscape (LizzieFrame.java:7255-7300, 7313-7385, 7414-7449,
 *  7491-7692): the board moves to the right edge and the panels share the space on its left.
 *  The persisted shares belong to the locked layout only. */
function enlargedLayout(width: number, height: number, r: LayoutRequest, largeSub: boolean, largeWinrate: boolean): Panels {
  const maxBound = Math.max(width, height);
  const maxSize = Math.max(Math.min(width, height), Math.max(r.cols, r.rows) + 5);
  const boardY = i((height - maxSize) / 2);
  const panelMargin = i(maxSize * 0.02);
  let boardX = i((width - maxSize) / 8) * r.boardPositionProportion;
  const capx = 0;
  let capy = 0;
  let capw = boardX - panelMargin;
  let caph = boardY + i(maxSize / 8);
  const statx = capx;
  let staty = capy + caph;
  let statw = capw;
  let stath = i(maxSize / 10);
  const grx = statx;
  let gry = staty + stath;
  let grw = statw;
  let grh = i(maxSize / 3);
  let vx = boardX + maxSize + panelMargin;
  const vy = capy;
  let vw = width - vx;
  let vh = height - vy;
  let ponderingY = height;
  if (r.showStatus) ponderingY = ponderingY - i(maxSize * 0.023) - i(maxBound * 0.04);
  let subBoardY = gry + grh;
  let subBoardWidth = grw;
  let subBoardHeight = ponderingY - subBoardY;
  let subBoardLength = Math.min(subBoardWidth, subBoardHeight);
  let subBoardX = statx + i((statw - subBoardLength) / 2);
  const noVariation = !r.showVariationGraph && !r.showListPane;
  const noComment = !r.showComment;
  let isSmallCap = false;
  if (largeSub) {
    boardX = width - maxSize - panelMargin;
    const spaceW = boardX - panelMargin;
    const panelW = i(spaceW / 2);
    let panelH = i((height * 2) / 7);
    capw = noVariation && noComment ? spaceW : panelW;
    caph = i(panelH * 0.2);
    staty = capy + caph;
    statw = capw;
    stath = i(panelH * 0.33);
    gry = staty + stath;
    grw = spaceW;
    grh = panelH - caph - stath;
    vx = statx + statw;
    vw = panelW;
    vh = stath + caph;
    subBoardY = gry + grh;
    subBoardWidth = spaceW;
    subBoardHeight = ponderingY - subBoardY;
    subBoardLength = Math.min(subBoardWidth, subBoardHeight);
    if (subBoardHeight > subBoardWidth) {
      subBoardY = subBoardY + subBoardHeight - subBoardWidth;
      panelH = i((height * 2) / 7) + (subBoardHeight - subBoardWidth);
      caph = i(panelH * 0.2);
      staty = capy + caph;
      stath = i(panelH * 0.33);
      gry = staty + stath;
      grh = panelH - caph - stath;
      vh = stath + caph;
    }
    subBoardX = statx + i((spaceW - subBoardLength) / 2);
    isSmallCap = true;
  } else if (largeWinrate) {
    boardX = width - maxSize - panelMargin;
    const spaceW = boardX - panelMargin;
    const panelW = i(spaceW / 2);
    const panelH = i(height / 4);
    capy = panelH + 1;
    capw = spaceW;
    caph = i((ponderingY - panelH) * 0.15);
    staty = capy + caph;
    statw = capw;
    stath = caph;
    gry = staty + stath;
    grw = statw;
    grh = ponderingY - gry;
    vx = panelW;
    vw = panelW;
    vh = panelH;
    subBoardY = 0;
    subBoardWidth = panelW;
    subBoardHeight = panelH;
    subBoardLength = Math.min(subBoardWidth, subBoardHeight);
    subBoardX = statx + i((vw - subBoardLength) / 2);
  }
  const conth = stath + grh;
  let treeh = vh;
  let cx = vx;
  let cy = vy;
  let cw = vw;
  let ch = vh;
  if (r.showComment) {
    if (r.showVariationGraph || r.showListPane) {
      treeh = i(vh / 2);
      cy = vy + treeh;
      ch = treeh;
    }
    if (!largeSub) {
      const [tempx, tempy, tempw, temph] = [cx, cy, cw, ch];
      cx = subBoardWidth > subBoardHeight ? subBoardX - i((subBoardWidth - subBoardHeight) / 2) : subBoardX;
      cy = subBoardY;
      cw = subBoardWidth;
      ch = subBoardHeight;
      subBoardX = tempx;
      subBoardY = tempy;
      subBoardLength = Math.min(tempw, temph);
    }
  }
  // Background containers, in paint order (7491-7529).
  const plainGraph = largeSub && noComment && noVariation && !r.showCaptured;
  const glass: Rect[] = [];
  if (r.showWinrateGraph) {
    if (plainGraph) glass.push({ x: statx, y: 0, w: grw, h: conth + caph });
    else glass.push({ x: statx, y: staty, w: isSmallCap ? grw : statw, h: conth });
  }
  if (r.showVariationGraph || r.showListPane) {
    if (!r.showSubBoard && r.showComment) treeh = vh;
    glass.push({ x: vx, y: vy, w: vw, h: treeh });
  }
  if (r.showComment) glass.push({ x: cx, y: cy, w: cw, h: ch });
  const captured = largeSub && r.showSubBoard && !r.showWinrateGraph ? { x: capx, y: capy, w: capw, h: treeh } : { x: capx, y: capy, w: capw, h: caph };
  if (r.showCaptured) glass.push(captured);
  // The variation tree and the list (7607-7625); 大胜率图 never shows the list (showListPane()).
  const isLargeSubboard = largeSub && !r.largeWinrateGraph;
  let table: Rect | null = null;
  let tree = NONE;
  if (r.showVariationGraph || r.showListPane) {
    if (!r.showSubBoard && r.showComment) treeh = vh;
    if (r.showListPane && !isLargeSubboard) {
      if (r.showVariationGraph) {
        treeh = i(treeh / 2);
        table = { x: vx, y: vy + treeh, w: vw, h: treeh };
      } else table = { x: vx, y: vy, w: vw, h: treeh };
    }
    if (r.showVariationGraph) tree = r.showCaptured ? { x: vx, y: vy, w: vw, h: treeh } : { x: vx - vw, y: vy, w: vw * 2, h: treeh };
  }
  if (r.showComment && largeSub) {
    if (!r.showVariationGraph && r.showListPane) cy = ch;
    if (!r.showWinrateGraph) {
      cx = cx - cw;
      cw = cw * 2;
    }
  }
  let stat = NONE;
  let graph = NONE;
  if (r.showWinrateGraph) {
    stat = { x: statx, y: plainGraph ? staty - caph : staty, w: statw, h: stath };
    if (r.showListPane && isLargeSubboard) {
      if (!r.showVariationGraph) {
        table = noComment ? { x: vx, y: vy, w: vw, h: vh } : { x: grx + i(grw / 2), y: 0, w: i(grw / 2), h: ch };
      } else {
        table = { x: grx + i(grw / 2), y: gry, w: i(grw / 2), h: grh };
        grw = i(grw / 2);
      }
    }
    if (plainGraph) {
      gry -= caph;
      grh += caph;
    }
    graph = { x: grx, y: gry, w: grw, h: grh };
  } else if (isLargeSubboard && r.showListPane) table = { x: grx, y: gry, w: grw, h: grh };
  return {
    board: { x: boardX, y: boardY, w: maxSize, h: maxSize },
    captured,
    stat,
    graph,
    comment: r.showComment ? { x: cx, y: cy, w: cw, h: ch } : NONE,
    ponderingY,
    tree,
    table: table ?? NONE,
    sub: r.showSubBoard ? { x: subBoardX, y: subBoardY, w: subBoardLength, h: subBoardLength } : NONE,
    glass,
    shown: {
      captured: r.showCaptured,
      stat: r.showWinrateGraph,
      graph: r.showWinrateGraph,
      comment: r.showComment,
      status: r.showStatus,
      tree: r.showVariationGraph,
      table: table !== null,
      sub: r.showSubBoard,
    },
    handles: null,
  };
}

/** 双引擎模式's main panel: the board size and the two boards' rectangles. */
export interface DualPanels {
  size: number;
  left: Rect;
  right: Rect;
}

/** 双引擎模式 (LizzieFrame.java:6640-6661): two square boards side by side from the panel's top
 *  left, the primary engine's on the left; nothing else but the status strips at y = size. */
export function doubleEngineLayout(width: number, height: number, cols: number, rows: number): DualPanels {
  const size = Math.max(Math.min(i(width / 2), height - 20), Math.max(cols, rows) + 5);
  return { size, left: { x: 0, y: 0, w: size, h: size }, right: { x: size, y: 0, w: size, h: size } };
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

/** BoardRenderer.isBigMargin (BoardRenderer.java:3653-3665): coordinates, names under the board, or
 *  the pure network's line take the 0.055 margin, otherwise 0.03 (calculatePixelMargins). */
export function mainMarginRatio(bigMargin: boolean): number {
  return bigMargin ? 0.055 : 0.03;
}

/** The sub board's margin (SubBoardRenderer.calculatePixelMargins). */
export const SUB_MARGIN_RATIO = 0.035;

/** Board geometry for a margin ratio (main board `mainMarginRatio`, sub board `SUB_MARGIN_RATIO`).
 *  Rectangular boards use the spacing of their larger dimension. */
export function boardGeom(r: Rect, cols: number, rows: number, ratio: number): BoardGeom {
  const n = Math.max(cols, rows);
  const { size, margin, avail } = pixelMargins(Math.min(r.w, r.h), n, ratio);
  const square = i((avail - 1) / (n - 1));
  const stoneRadius = square < 4 ? 1 : i(square / 2) - 1;
  return { x: r.x + i((r.w - size) / 2), y: r.y + i((r.h - size) / 2), size, margin, square, stoneRadius, n, cols, rows };
}

/** WinrateGraph.java:585-678,2755-2761: the plot width and height inside the graph panel and its
 *  move slots for `moves` (the larger of the main trunk's and the current line's last move number
 *  minus one), at least 50. Shared by painting and graph hit testing. */
export function graphGeom(width: number, height: number, moves: number) {
  return { width: i((width - 6) * 0.98), height: height - 4, slots: Math.max(50, moves) };
}
