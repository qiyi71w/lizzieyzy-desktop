// AI 陪练 presentation state the core does not own: whether the setup dialog is downloading the
// model (Java keeps the session PREPARING then, so the strip button reads 准备中…) and how the
// dialog is brought to the front. The session itself is the core's `human_sl` view.
import { registerViewHandler, run, state } from "../app";
import type { HumanSlAnalysisDisplay, HumanSlContinuation, HumanSlView, OpponentPreset, WorkbenchView } from "../contract/generated";
import { format, t } from "../i18n";
import { matchPlaying } from "../match/state";

const listeners: (() => void)[] = [];

export const humanSlUi = {
  /** The setup dialog's model download is running (startModelDownload: session PREPARING). */
  downloading: false,
  /** The setup dialog owns the session (starting, preparing, a failed start or its pending
   *  cleanup): no controller exists yet, so the training bar stays hidden. */
  dialogOwnsSession: false,
  /** Brings the open setup dialog to the front (humanSlSetupDialog.toFront), else null. */
  focusDialog: null as (() => void) | null,
  /** Focuses the training bar's primary button (HumanSlTrainingBar.requestPrimaryFocus). */
  focusBar: null as (() => void) | null,
};

export function onHumanSlUiChange(listener: () => void): void {
  listeners.push(listener);
}

export function humanSlUiChanged(): void {
  listeners.forEach((l) => l());
}

/** The core's session, from 开始训练 until the exit completed. */
export function humanSlSession(v: WorkbenchView | null = state.view): HumanSlView | null {
  return v?.human_sl ?? null;
}

/** LizzieFrame.hasActiveHumanSlGame: a controller exists (the game started and has not finished). */
export function humanSlGameActive(v: WorkbenchView | null = state.view): boolean {
  const s = humanSlSession(v);
  return !!s && s.phase !== "Preparing";
}

/** The training mode's analysis override (HumanSlGameController.applyTrainingModeVisualState)
 *  while a game is on; the committed display settings stay as they are. */
export function humanSlDisplay(v: WorkbenchView | null = state.view): HumanSlAnalysisDisplay | null {
  return humanSlGameActive(v) ? humanSlSession(v)!.analysis_display : null;
}

/** LizzieFrame.aiCoachBlockedByGame: an 引擎对战 game or a 人机对弈 game is playing. */
export function aiCoachBlockedByGame(v: WorkbenchView | null = state.view): boolean {
  return matchPlaying(v) || !!v?.game;
}

/** HumanSlGameController.opponentLabel: the rank as 3段 / 12级, or the pro style's name. */
export function opponentLabel(opponent: OpponentPreset, danRank: boolean, rank: number): string {
  if (opponent === "ModernPro") return t("HumanSlTraining.pro.modern");
  if (opponent === "Online9d") return t("HumanSlTraining.pro.online9d");
  return format(danRank ? t("HumanSlTraining.rank.danValue") : t("HumanSlTraining.rank.kyuValue"), rank);
}

/** The dialogs a session's exit may open afterwards, registered by their owners. */
const continuations = new Map<HumanSlContinuation, () => void>();

export function registerHumanSlContinuation(kind: HumanSlContinuation, open: () => void): void {
  continuations.set(kind, open);
}

// The core publishes the continuation once, when it should run (after a successful exit,
// possibly after 重试清理).
registerViewHandler((view) => {
  if (view.human_sl_continuation) continuations.get(view.human_sl_continuation)?.();
});

/** LizzieFrame.deferUntilHumanSlExit / HumanSlGameController.tryAbortAndThen: without a session
 *  the operation runs now (true). With one, the core stops it and keeps `continuation` in the
 *  exit's one slot (a later request is swallowed); the operation runs when the view publishes it,
 *  so the caller does nothing now (false). */
export async function endHumanSlFirst(continuation: HumanSlContinuation): Promise<boolean> {
  if (!humanSlSession()) return true;
  await run({ type: "HumanSl", op: { type: "AbortThen", continuation } });
  return false;
}
