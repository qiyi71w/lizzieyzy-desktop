// 回放变化图 (R): LizzieFrame.replayBranch / finishReplayBranch (LizzieFrame.java:11617-11674).
// The hovered candidate's variation is shown 2, 3, ... moves long, one step every
// replay-branch-interval-seconds, then whole again; moving to another candidate or position,
// leaving the board or the window losing the focus ends it.
import { run, state } from "../app";

/** The main board's hovered variation: `key` identifies its position and candidate point. */
export interface ReplayTarget {
  key: string;
  /** Moves of the variation (getReplayBranch). */
  steps: number;
}

interface ReplayHost {
  target(): ReplayTarget | null;
  repaint(): void;
}

let host: ReplayHost | null = null;
let timer: number | undefined;
let replay: { key: string; length: number | null } | null = null;
/** Analysis was paused for the replay (norefresh-onmouse-move off). */
let pausedAnalysis = false;

/** The main board supplies its hovered variation and repaints on each step. */
export function bindReplayHost(h: ReplayHost): void {
  host = h;
}

/** The displayed length for the hovered variation `key`, or null when no replay shortens it. */
export function replayLength(key: string): number | null {
  return replay && replay.key === key ? replay.length : null;
}

/** Not while auto-replay-branch runs its own replay (LizzieFrame.java:11628). */
export function replayBranch(): void {
  const view = state.view, h = host;
  if (timer !== undefined || !h || !view || view.settings.display.autoReplayBranch) return;
  const target = h.target();
  if (!target || target.steps <= 0) return;
  const { key, steps } = target;
  // Java pauses pondering unless the hovered variation is frozen anyway.
  pausedAnalysis = !view.settings.display.noRefreshOnMouse && view.engine.analysing;
  if (pausedAnalysis) void run({ type: "ToggleAnalysis" });
  replay = { key, length: null };
  let next = 2;
  const tick = () => {
    if (h.target()?.key !== key) {
      finishReplayBranch();
      return;
    }
    const complete = next > steps + 1;
    replay = { key, length: complete ? null : next++ };
    h.repaint();
    if (complete) finishReplayBranch();
  };
  timer = window.setInterval(tick, Math.max(1, Math.trunc(view.settings.display.replayIntervalSeconds * 1000)));
  tick();
}

export function finishReplayBranch(): void {
  if (timer === undefined) return;
  window.clearInterval(timer);
  timer = undefined;
  replay = null;
  if (pausedAnalysis && state.view && !state.view.engine.analysing) void run({ type: "ToggleAnalysis" });
  pausedAnalysis = false;
  host?.repaint();
}

/** Whether a replay is running (isReplayVariation). */
export function replaying(): boolean {
  return timer !== undefined;
}
