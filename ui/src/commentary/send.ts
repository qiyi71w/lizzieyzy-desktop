// Commands of the AI 解说 window go straight to the adapter (the window needs their results:
// model lists, save outcomes). Arguments may hold the typed API key: nothing here logs them.
import { dispatch, isApiError } from "../contract/client";
import type { Command, CommandResult } from "../contract/generated";

export type Outcome = { ok: true; result: CommandResult } | { ok: false; message: string };

/** A refusal's Java key or an error's message (a Teacher.* key or safe English detail). */
export async function send(command: Command): Promise<Outcome> {
  try {
    const result = await dispatch(command);
    if (result.type === "Refused") return { ok: false, message: result.message_key };
    return { ok: true, result };
  } catch (e) {
    return { ok: false, message: isApiError(e) ? e.message : "" };
  }
}
