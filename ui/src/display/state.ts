// Board display state that Java keeps for the session only (never saved): the 显示候选点 黑 / 白
// check boxes (`Config.showBlackCandidates` / `showWhiteCandidates`, F, holding Z), 显示策略网络
// (T, `LizzieFrame.isShowingPolicy`) and 延迟显示候选点, which hides the candidates after each move
// and shows them again after the set delay (`Board.java:3730-3745`).
import { registerViewHandler, run, state } from "../app";
import type { Stone } from "../contract/generated";
import { humanSlDisplay } from "../humansl/state";

export const boardUi = {
  showBlack: true,
  showWhite: true,
  /** 显示策略网络: candidates show their policy prior instead of their statistics. */
  policy: false,
};

/** The candidate check boxes as they were when Z was pressed (startTemporaryBoard). */
let temporary: { black: boolean; white: boolean } | null = null;
let delayTimer: number | undefined;

const listeners: (() => void)[] = [];
export function onBoardUiChange(listener: () => void): void {
  listeners.push(listener);
}
function changed(): void {
  listeners.forEach((l) => l());
}

export function setCandidates(black: boolean, white: boolean): void {
  if (boardUi.showBlack === black && boardUi.showWhite === white) return;
  boardUi.showBlack = black;
  boardUi.showWhite = white;
  changed();
}

/** Whether the candidates of `toPlay`'s side are shown. An AI 陪练 game overrides the check boxes
 *  without changing them (HumanSlGameController.applyTrainingModeVisualState): 实战后复盘 hides
 *  both colours' candidates, 边下边分析 shows both. */
export function candidatesShownFor(toPlay: Stone): boolean {
  const training = humanSlDisplay();
  if (training) return training === "Show";
  return toPlay === "Black" ? boardUi.showBlack : boardUi.showWhite;
}

/** F (LizzieFrame.toggleShowCandidates): both off while either is on, else both on. */
export function toggleCandidates(): void {
  if (boardUi.showBlack || boardUi.showWhite) setCandidates(false, false);
  else setCandidates(true, true);
}

/** Holding Z hides the candidates; releasing it restores each colour's check box. (Java restored
 *  the menu's black box from the white one, `stopTemporaryBoard`.) */
export function startTemporaryBoard(): void {
  if (temporary) return;
  temporary = { black: boardUi.showBlack, white: boardUi.showWhite };
  setCandidates(false, false);
}

export function stopTemporaryBoard(): void {
  if (!temporary) return;
  const { black, white } = temporary;
  temporary = null;
  setCandidates(black, white);
}

/** T / 分析 > 策略网络 (LizzieFrame.togglePolicy, LizzieFrame.java:11938-11963): a shown pure
 *  network (H) is turned off first; turning the policy on starts the analysis when it is paused and
 *  the position has no candidates yet (Leelaz.ponder). */
export function togglePolicy(): void {
  const view = state.view;
  if (view?.rawNet) void run({ type: "StopRawNet" });
  boardUi.policy = !boardUi.policy;
  if (boardUi.policy && view?.engine.foreground && view.engine.paused && !view.analysis?.candidates.length) {
    void run({ type: "ToggleAnalysis" });
  }
  changed();
}

/** H / 纯网络 (Leelaz.toggleHeatmap(false), Leelaz.java:24297-24342): without an engine it is T;
 *  otherwise the policy display ends and the foreground engine cycles its pure network. */
export function toggleRawNet(): void {
  if (!state.view?.engine.foreground) {
    togglePolicy();
    return;
  }
  if (boardUi.policy) {
    boardUi.policy = false;
    changed();
  }
  void run({ type: "ToggleRawNet" });
}

/** Board.handleCandidatesDelay: hideCandidates, then showCandidates after the delay (none while it
 *  is 0 or less: F shows them). */
export function delayCandidates(seconds: number): void {
  window.clearTimeout(delayTimer);
  setCandidates(false, false);
  if (seconds > 0) delayTimer = window.setTimeout(() => setCandidates(true, true), seconds * 1000);
}

let position = "";
registerViewHandler((view) => {
  const here = `${view.document.id}:${view.position.node}`;
  if (here === position) return;
  const first = position === "";
  position = here;
  const s = view.settings.display;
  if (!first && s.delayCandidates) delayCandidates(s.delayCandidatesSeconds);
});
