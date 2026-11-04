// Analysis information beside the candidates, as the Java workbench shows it: which ownership the
// boards draw (纯网络 / Kata形势, BoardRenderer.drawEstimate, SubBoardRenderer.drawEstimate), the
// statistics panel's inputs (LizzieFrame.drawMoveStatistics) and the window title
// (LizzieFrame.updateTitle with its visits-per-second sampler). Values come from the core
// projections only; nothing is flipped by side or shifted by komi here.
import { APP_TITLE, state, windowTitle } from "../app";
import type { CandidateView, HistoryPoint, WorkbenchView } from "../contract/generated";
import { t } from "../i18n";
import { playouts, type OwnershipLayer, type StatScene } from "../workbench/paint";
import { sameOwnership } from "../workbench/viewEquality";

/** The ownership the main board draws: 纯网络's while it is on (shouldShowHeatmapFor suppresses
 *  Kata形势), else Kata形势 on the main board, the hovered candidate's own with 按选点显示评估
 *  (BoardRenderer.java:476-502, 1653-1656). Nothing while scoring. */
export function mainOwnership(v: WorkbenchView, hovered: number | null): OwnershipLayer | null {
  if (v.score) return null;
  if (v.rawNet) return rawNetOwnership(v);
  const k = v.kata_estimate;
  if (!k.state.shown || !k.state.onMain) return null;
  const candidate = hovered !== null && hovered >= 0 ? v.analysis?.candidates.find((c) => c.order === hovered) : undefined;
  return estimateLayer(v, candidate);
}

/** The ownership 双引擎模式's right board draws (boardRenderer2: leelaz2's heatcount and
 *  estimateArray2, BoardRenderer.java:753, 1602-1630) under the main board's rules: the secondary
 *  engine's 纯网络 for this position, else its Kata形势 on the main board, the hovered candidate's
 *  own with 按选点显示评估. Nothing while scoring. */
export function secondaryOwnership(v: WorkbenchView, hovered: number | null): OwnershipLayer | null {
  const s = v.secondary;
  if (v.score || !s) return null;
  if (v.rawNet) {
    const net = s.rawNet;
    const own = net && net.node === v.position.node && net.mode === "Ownership" && v.engine.secondary?.katago ? net.ownership : null;
    return own ? { ownership: own, style: v.settings.display.pureEstimate, source: "rawNet" } : null;
  }
  const k = v.kata_estimate;
  if (!k.state.shown || !k.state.onMain) return null;
  const style = v.settings.display.kataEstimateStyle;
  const candidate = hovered !== null && hovered >= 0 ? s.analysis?.candidates.find((c) => c.order === hovered) : undefined;
  if (v.settings.display.useMovesOwnership && candidate?.ownership) return { ownership: candidate.ownership, style, source: "candidate" };
  return s.ownership ? { ownership: s.ownership, style, source: "estimate" } : null;
}

/** The ownership the sub board draws (SubBoardRenderer.java:264-290, 880-901): 纯网络's, else
 *  Kata形势 on the sub board, the shown variation's candidate's own with 按选点显示评估. */
export function subOwnership(v: WorkbenchView, candidate: CandidateView | undefined): OwnershipLayer | null {
  if (v.score) return null;
  if (v.rawNet) return rawNetOwnership(v);
  const k = v.kata_estimate;
  if (!k.state.shown || !k.state.onSub) return null;
  return estimateLayer(v, candidate);
}

function rawNetOwnership(v: WorkbenchView): OwnershipLayer | null {
  const net = v.rawNet;
  // A result for another position is never shown.
  const own = net && net.node === v.position.node && net.mode === "Ownership" && v.engine.foreground?.katago ? net.ownership : null;
  return own ? { ownership: own, style: v.settings.display.pureEstimate, source: "rawNet" } : null;
}

function estimateLayer(v: WorkbenchView, candidate: CandidateView | undefined): OwnershipLayer | null {
  const style = v.settings.display.kataEstimateStyle;
  if (v.settings.display.useMovesOwnership && candidate?.ownership) return { ownership: candidate.ownership, style, source: "candidate" };
  const own = v.kata_estimate.ownership;
  return own ? { ownership: own, style, source: "estimate" } : null;
}

/** Repaint signatures of the boards' ownership: a revision bumped when the squares' values change.
 *  `raw` is 思考模式's raw board, `quadN` 四方图模式's sub boards. */
type LayerBoard = "main" | "sub" | "right" | "raw" | `quad${number}`;
const layerSeen: Partial<Record<LayerBoard, { own: number[] | null; revision: number }>> = {};
export function layerSignature(board: LayerBoard, layer: OwnershipLayer | null): string {
  const seen = (layerSeen[board] ??= { own: null, revision: 0 });
  const own = layer?.ownership ?? null;
  if (!sameOwnership(seen.own, own)) {
    seen.own = own;
    seen.revision++;
  }
  return layer ? `${layer.style}:${layer.source}:${seen.revision}` : "";
}

/** drawMoveStatistics' inputs: the current node, else the last analysed position before it
 *  (previous, then the one before, LizzieFrame.java:8739-8770); the last move's change
 *  (core `last_move`, from the same earlier position, only while the current node is analysed)
 *  and its candidate rank. */
export function statScene(v: WorkbenchView, history: Map<number, HistoryPoint>, parentOf: (node: number) => number | undefined): StatScene {
  const analysed = (node: number | undefined) => {
    const p = node === undefined ? undefined : history.get(node);
    return p && p.visits > 0 ? p : undefined;
  };
  const node = v.position.node;
  const parent = parentOf(node);
  const current = analysed(node);
  const last = analysed(parent) ?? analysed(parent === undefined ? undefined : parentOf(parent));
  const shown = current ?? last;
  const change = v.statistics.last_move;
  const lead = shown?.black.score_lead ?? null;
  return {
    black: shown?.black.winrate ?? null,
    // blackLastWR: the position the change is measured from.
    before: change && last ? last.black.winrate : null,
    change: change?.winrate ?? null,
    scoreChange: change?.score ?? null,
    lead,
    leadWithKomi: v.settings.display.scoreLeadWithKomi,
    kata: !!v.engine.foreground?.katago || lead !== null,
    matched: v.statistics.moves.find((m) => m.node === node)?.matched ?? null,
    moveNumber: v.position.move_number,
    moveNumberDown: !!v.engine.rules_key,
  };
}

// ------------------------------------------------------------------ title

/** visitsString (LizzieFrame.java:1570-1620, getLastVisitsCount 2514-2572): once a second the
 *  current position's visits are sampled into four slots; the speed is measured against the
 *  oldest sample of the same position, and " - 搜索/秒" replaces it 5 s after the last measure. */
const samples: ({ position: string; visits: number } | null)[] = [null, null, null, null];
let sampleIndex = -1;
let speedText = "";
let speedAt = 0;

function sampleSpeed(): void {
  const v = state.view;
  if (!v?.engine.foreground || !v.engine.analysing) return;
  const position = `${v.document.id}:${v.position.node}`;
  const total = v.analysis?.visits ?? 0;
  let seconds = -1, before = 0;
  if (sampleIndex >= 0) {
    for (let age = 4; age >= 1; age--) {
      const sample = samples[(sampleIndex + 5 - age) % 4];
      if (sample?.position === position) {
        seconds = age;
        before = sample.visits;
        break;
      }
    }
  }
  const now = Date.now();
  if (seconds > 0) {
    const speed = Math.trunc((total - before) / seconds);
    if (speed >= 0) {
      speedText = ` ${speed} ${t("LizzieFrame.speedUnit")}`;
      speedAt = now;
    }
  } else if (now - speedAt > 5000) speedText = ` - ${t("LizzieFrame.speedUnit")}`;
  sampleIndex = (sampleIndex + 1) % 4;
  if (total > 0) samples[sampleIndex] = { position, visits: total };
}

/** Starts the once-a-second title update (showPlayouts). */
export function startTitleUpdates(repaint: () => void): void {
  setInterval(() => {
    sampleSpeed();
    repaint();
  }, 1000);
}

/** LizzieFrame.updateTitle (LizzieFrame.java:10942-11048) outside engine games: the app name, the
 *  [Black win rate, Black lead, visits] of the current position (else of the previous one), the
 *  engine with its speed (or " - 搜索/秒" while paused), the players, result and file. 试下 keeps
 *  LizzieFrame.tryTitle. */
export function workbenchTitle(v: WorkbenchView, version: string, history: Map<number, HistoryPoint>, parent: number | undefined): string {
  const base = windowTitle(version);
  if (v.editing.trying) return base;
  const stem = `${APP_TITLE} ${version}`;
  const rest = base.slice(stem.length + 1);
  const a = v.analysis;
  const previous = parent === undefined ? undefined : history.get(parent);
  // 窗口标题显示胜率 (show-title-wr, LizzieFrame.java:10966-11020) off drops the bracketed values; a
  // game in play mode drops them too, and a game without values shows "[ ---   ---   --- ] ".
  const shown = v.settings.panels.showWinrateInTitle && !v.game?.play_mode;
  const values = !shown ? null : a && a.visits > 0 ? { black: a.black, visits: a.visits } : previous && previous.visits > 0 ? previous : null;
  const lead = values?.black.score_lead ?? null;
  const prefix = values ? `[${values.black.winrate.toFixed(1)}${lead === null ? "" : ` ${lead.toFixed(1)}`} ${playouts(values.visits)}] ` : shown && v.game ? "[ ---   ---   --- ] " : "";
  const foreground = v.engine.foreground;
  if (!foreground) return `${stem}${prefix ? ` - ${prefix}` : ""} ${rest}`;
  const speed = v.engine.analysing ? `${speedText} ` : ` - ${t("LizzieFrame.speedUnit")} `;
  return `${stem} - ${prefix}${foreground.name}${speed}${rest}`;
}
