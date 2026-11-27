// The toolbar's analysis changes between sending them and seeing them in the projection
// (toolbar.ts). Kept apart from the toolbar so it runs without a page.
import type { CommandResult } from "../contract/generated";

/** Whether the core took a command: a refusal (`Refused`, shown as a message) and a failure
 *  (null, shown as an error) leave every setting as it was. */
export function taken(result: CommandResult | null): boolean {
  return result !== null && result.type !== "Refused";
}

/** Changes sent but not yet shown by the projection. A save replaces every setting, so a later
 *  change builds on these, not on a projection that does not have them yet. They go once no save
 *  is pending and a projection shows them all, or as soon as the core does not take a save (it
 *  keeps what it had, and a later save must not carry the change it did not take). */
export class PendingChanges<T extends object> {
  private changes: Partial<T> = {};
  private saves = 0;

  /** The settings to send for `change`: the projection with the pending changes and `change` over
   *  it. `change` is pending from here on. */
  send(projection: T, change: Partial<T>): T {
    this.changes = { ...this.changes, ...change };
    this.saves++;
    return { ...projection, ...this.changes };
  }

  /** The projection as the toolbar shows it, the pending changes over it. */
  over(projection: T): T {
    return { ...projection, ...this.changes };
  }

  /** A save answered: a save the core did not take drops every pending change. */
  answered(result: CommandResult | null, projection: T | undefined): void {
    this.saves--;
    if (!taken(result)) this.changes = {};
    this.settle(projection);
  }

  /** Drops the changes once every save has answered and `projection` has caught up with them. A
   *  projection is checked as it arrives and again when the last save answers, as either may come
   *  first. */
  settle(projection: T | undefined): void {
    if (this.saves > 0 || !projection) return;
    const keys = Object.keys(this.changes) as (keyof T)[];
    if (keys.every((k) => projection[k] === this.changes[k])) this.changes = {};
  }
}
