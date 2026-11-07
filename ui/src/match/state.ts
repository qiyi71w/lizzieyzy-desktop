// 引擎对战 presentation state the core does not own: the toolbar's 引擎对战 check box (chkenginePk,
// never saved), the batch title LizzieFrame keeps between games (enginePkTitile) and the match
// values of the last projection, which the one-shot outcome needs after the match view is gone.
import { state } from "../app";
import type { MatchPhase, MatchSessionSettings, MatchSettings, MatchView, WorkbenchView } from "../contract/generated";

const listeners: (() => void)[] = [];

export const matchUi = {
  /** chkenginePk: the 黑/白 engine boxes are enabled only while it is checked. */
  engineMatchChecked: false,
  /** LizzieFrame.hasEnginePkTitile / enginePkTitile: null when off; "" or the batch score title. */
  pkTitle: null as string | null,
  /** The match of the latest view that had one. */
  last: null as MatchView | null,
};

export function onMatchUiChange(listener: () => void): void {
  listeners.push(listener);
}

export function setEngineMatchChecked(checked: boolean): void {
  matchUi.engineMatchChecked = checked;
  listeners.forEach((l) => l());
}

/** EngineGameSnapshot.playing(): a game of the batch is on (running or paused). */
export function matchPlaying(v: WorkbenchView | null = state.view): boolean {
  const phase: MatchPhase | undefined = v?.engine_match?.phase;
  return phase === "Playing" || phase === "Paused";
}

/** EngineGameDesktop.batchActive(): a batch exists (any phase until it is idle). */
export function matchActive(v: WorkbenchView | null = state.view): boolean {
  return !!v?.engine_match;
}

/** The side's engine name this game: the first engine plays Black unless the colours changed. */
export function sideName(m: MatchView, black: boolean): string {
  return m.first_is_black === black ? m.first_name : m.second_name;
}

/** EnginePkIdentity.persistSide: the chosen catalog entry's command and name. */
export function engineIdentity(index: number): MatchSettings["first"] {
  const e = state.view?.engine.engines[index];
  return { commands: e?.command ?? "", name: e?.name ?? "" };
}

/** Match SetValues with one change over the saved values (Java's immediate saves and toolbar edits). */
export function matchValues(change: { settings?: Partial<MatchSettings>; session?: Partial<MatchSessionSettings>; disable_wrn?: boolean }) {
  const m = state.view!.settings.matches;
  return {
    type: "Match" as const,
    op: {
      type: "SetValues" as const,
      settings: { ...m.settings, ...change.settings },
      session: { ...m.session, ...change.session },
      disable_wrn: change.disable_wrn ?? m.disable_wrn,
    },
  };
}
