// The main window's side of function search's 打开棋谱后此窗口 for one 棋谱 window (野狐, 腾讯, OGS):
// the window opens, the relay asks it to locate the choice, and the activation counts as done only
// once the window says the choice has the focus for this request.
import type { Reason } from "../commands";

/** How long the window may take to come up and put the focus on the choice. */
const LOCATE_WAIT_MS = 10_000;
const NOT_REACHED: Reason = { key: "FunctionSearch.unavailable.targetMissing" };

export interface AfterGetRequest {
  /** The relay's `locate`: the number of the request waiting for the window, if any. */
  readonly request: number | null;
  /** Function search's activation: resolves null once the window located the choice for this
   *  request, and the reason once a newer request takes its place or the time is up, whether or
   *  not the window has come up; rejects if the window fails to open before then. */
  locate(): Promise<Reason | null>;
  /** The window's `located` for request `request`; one for any other request counts for nothing. */
  located(request: number): void;
}

interface Pending {
  id: number;
  timer: number;
  resolve: (reason: Reason | null) => void;
  reject: (error: unknown) => void;
}

/** `open` brings the window up; `relay` sends it the current payload. */
export function afterGetRequest(open: () => Promise<void>, relay: () => void): AfterGetRequest {
  let pending: Pending | null = null;
  let lastId = 0;
  /** Ends the waiting request and tells the window, which then stops locating. */
  const end = (): Pending | null => {
    const ended = pending;
    if (!ended) return null;
    pending = null;
    window.clearTimeout(ended.timer);
    relay();
    return ended;
  };
  return {
    get request() {
      return pending?.id ?? null;
    },
    locate() {
      end()?.resolve(NOT_REACHED);
      const { promise, resolve, reject } = Promise.withResolvers<Reason | null>();
      const id = ++lastId;
      pending = { id, resolve, reject, timer: window.setTimeout(() => end()?.resolve(NOT_REACHED), LOCATE_WAIT_MS) };
      // Once ended, the request asks the window for nothing more, whatever its opening does.
      open().then(
        () => {
          if (pending?.id === id) relay();
        },
        (e: unknown) => {
          if (pending?.id === id) end()!.reject(e);
        },
      );
      return promise;
    },
    located(request) {
      if (pending?.id === request) end()!.resolve(null);
    },
  };
}
