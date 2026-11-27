// When function search's entries for settings inside a dialog or a child window can reach their
// control (the rules alone, without the DOM, so they can be tested on their own).
import type { Reason } from "../commands";
import type { StrengthStage, WorkbenchView } from "../contract/generated";

/** 开局广度 / 开局激进度, in the 引擎对局 dialog: refused like the dialog while a game of the batch
 *  plays; not while a 人机对弈 game or an AI 陪练 session is on, which opening the dialog ends;
 *  and the rows show only for an analysis-mode match. */
export function openingLocateReason(v: WorkbenchView | null): Reason | null {
  if (!v) return null;
  if (v.engine_match?.phase === "Playing" || v.engine_match?.phase === "Paused") return { key: "LizzieFrame.engineGameStopFirstHint" };
  if (v.game) return { key: "matchOpeningDuringGame", product: true };
  if (v.human_sl) return { key: "matchOpeningDuringCoach", product: true };
  return v.settings.matches.session.kind === "Genmove" ? { key: "matchOpeningNeedsAnalysis", product: true } : null;
}

/** 分阶段 in 棋力评估 at `stage`: its page and dividers exist only once the game can be estimated;
 *  before that the dashboard opens at the page that says what to do first, which is the reason. */
export function phasesLocateReason(stage: StrengthStage): Reason | null {
  switch (stage.type) {
    case "Ready":
      return null;
    case "TooShort":
      return { key: "PlayerStrengthEstimate.empty.short.title", shown: true };
    case "NeedsAnalysis":
      return { key: "PlayerStrengthEstimate.empty.analysis.title", shown: true };
    case "NeedsHumanStyle":
      return { key: "PlayerStrengthEstimate.empty.humanStyle.title", shown: true };
  }
}

/** 打开棋谱后此窗口 in a 棋谱 window: request `locate` (a number, or null for none) runs `act` once
 *  per request, once the choice can take the focus (`ready`), and says when it did; until then each
 *  new payload of the request tries again. A payload without a request lets the next one locate
 *  again whatever its number. */
export function pendingLocate(ready: () => boolean, act: (request: number) => void): (locate: number | null) => boolean {
  let done: number | null = null;
  return (locate) => {
    if (locate === null) {
      done = null;
      return false;
    }
    if (locate === done || !ready()) return false;
    done = locate;
    act(locate);
    return true;
  };
}

/** OGS棋谱's 打开棋谱后此窗口: the window holds the choice while it searches or downloads
 *  (OgsKifuBrowser.updateControls), so function search offers it only once that is over. */
export function ogsAfterGetReason(v: WorkbenchView | null): Reason | null {
  return v?.ogs_kifu.busy ? { key: "ogsAfterGetBusy", product: true } : null;
}
