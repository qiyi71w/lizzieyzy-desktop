// Keeping phase boundaries (StrengthPhaseView's onCommit) while saves may overlap: the phases view's
// dividers send a save on every step, and the newest input decides what the view settles on. The
// core answers saves in the order they were sent (ui/src/app.ts run()), so the last save that
// succeeded is what is saved, even when a newer one then fails.

/** The saves of one phases view. */
export interface PhaseSaves<T> {
  /** The phases at the saved boundaries. */
  readonly saved: T;
  /** Saves under way. */
  readonly pending: number;
  /** Saves started so far: an answer asked for before the latest one is stale. */
  readonly started: number;
  /** Saves `bounds`. Resolves to the phases to show while this is still the newest save (the saved
   *  ones, also when this save failed), null once a newer one has started. */
  keep(bounds: [number, number]): Promise<T | null>;
}

export function phaseSaves<T>(initial: T, save: (bounds: [number, number]) => Promise<T | null>): PhaseSaves<T> {
  let saved = initial;
  let pending = 0;
  let started = 0;
  return {
    get saved() {
      return saved;
    },
    get pending() {
      return pending;
    },
    get started() {
      return started;
    },
    async keep(bounds) {
      const at = ++started;
      pending++;
      try {
        const result = await save([...bounds]);
        if (result) saved = result;
        return at === started ? saved : null;
      } finally {
        pending--;
      }
    },
  };
}
