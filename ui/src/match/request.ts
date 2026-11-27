// Where a 对局参数 request stands (Java EngineGameParametersDialog.refresh after 确定): the core
// answers with the last view it built before it took the values, so only a later view shows what
// came of them, however the answer and the views reach the panel. No DOM: the panel acts on it.
import type { MatchView } from "../contract/generated";

/** Values the core took for game `game`; the views with `seq > after` show what came of them. */
export interface ParameterRequest {
  game: number;
  after: number;
}

/** `waiting`: the view comes from before the request, or the values are still being written.
 *  `confirmed`: both participants confirmed every value; the panel closes. `open`: the request
 *  failed (its reasons are shown, the game is paused) or its game is no longer playing; the panel
 *  takes another 确定. */
export type RequestState = "waiting" | "confirmed" | "open";

/** `game`: the playing analysis game the view shows, null without one. */
export function requestState(request: ParameterRequest, seq: number, game: MatchView | null): RequestState {
  if (seq <= request.after) return "waiting";
  const p = game?.game === request.game ? game.parameters : null;
  if (!p) return "open";
  if (p.pending) return "waiting";
  if (p.failures.length > 0) return "open";
  return [p.first, p.second].every((x) => x.wrn.target === null && x.pda.target === null) ? "confirmed" : "waiting";
}
