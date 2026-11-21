// KataGo auto setup dialog (KataGoAutoSetupDialog): its sidebar cards share this seam.
import type { WorkbenchView } from "../contract/generated";

/** Sidebar entries in Java order (CARD_OVERVIEW, CARD_WEIGHTS, CARD_BENCHMARK, CARD_ACCELERATION). */
export type SetupCardKey = "overview" | "weights" | "performance" | "acceleration";

/** What opened the dialog: a card to show and, for the performance card, the engine profile to
 *  optimize (MoreEngines "性能测试" passes the selected entry). */
export interface SetupContext {
  engineId?: string;
}

/** One card's content. `update` runs for every published view while the dialog is open. */
export interface SetupCard {
  element: HTMLElement;
  update(view: WorkbenchView): void;
  /** Called when the dialog closes. */
  dispose?(): void;
}

/** A step's state: ready, missing (or not done yet), failed, or a plain fact. */
export type SetupTone = "ready" | "missing" | "failed" | "neutral";

/** Draws `el` as a 状态标签 in the tone of its state. */
export function tagged<T extends HTMLElement>(el: T, tone: SetupTone): T {
  el.className = `setup-tag ${tone}`;
  return el;
}
