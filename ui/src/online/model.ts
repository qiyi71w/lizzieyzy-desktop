// The relays between the main window and the 弈客 windows. The main window owns the only workbench
// subscription: it sends 弈客直播中心 (label `yike-live`, yikelive.html) its part of every view and
// turns both windows' input into commands and window calls. The 弈客 web window's toolbar is the
// local webview `yike-bar` (yikebrowser.html) above the remote 弈客 page.
import type { Language, LiveCategory, LiveListView, OnlineStatusView } from "../contract/generated";

export const LIVE_LABEL = "yike-live";
/** main → 弈客直播中心: LivePayload. */
export const LIVE_DATA_EVENT = "yike-live-data";
/** 弈客直播中心 → main: LiveInput. */
export const LIVE_INPUT_EVENT = "yike-live-input";

export const BAR_LABEL = "yike-bar";
/** main → toolbar: BarPayload (the answer to its `ready`). */
export const BAR_DATA_EVENT = "yike-bar-data";
/** toolbar → main: BarInput. */
export const BAR_INPUT_EVENT = "yike-bar-input";
/** platform → toolbar: `{ url }` whenever the page address changes (CefDisplayHandler.onAddressChange). */
export const BAR_ADDRESS_EVENT = "yike-browser-address";

/** YikeApiClient.YIKE_LIVE_URL / YIKE_GAME_URL. */
export const YIKE_LIVE_URL = "https://home.yikeweiqi.com/#/live";
export const YIKE_GAME_URL = "https://home.yikeweiqi.com/#/game";

export interface LivePayload {
  language: Language;
  live: LiveListView;
  /** The follow's status (updateYikeLiveSyncStatus); shown when the window's active room is its
   *  url or the window follows nothing (YikeLiveDialog.updateSyncStatus). */
  status: OnlineStatusView | null;
}

/** What the acceptance probe reports for 弈客直播中心, as the window drew it. */
export interface LiveReport {
  category: LiveCategory;
  filter: string;
  /** Refresh / 更多 / 同步 / 打开网页 and the category are disabled while loading. */
  loading: boolean;
  status: string;
  /** Visible rows in view order (after sorting and filtering), each the eight cells. */
  rows: string[][];
  /** View index of the selected row. */
  selected: number | null;
  sort: { column: number; ascending: boolean } | null;
  activeUrl: string;
  /** Controls with their client-area centres, so automation clicks the real controls. */
  targets: { key: string; text: string; enabled: boolean; x: number; y: number }[];
}

export type LiveInput =
  | { type: "ready" }
  /** 刷新 / 分类 (more false) and 更多 (loadGames). */
  | { type: "load"; category: LiveCategory; more: boolean }
  /** 同步 / double-click / Enter on a row with a room (syncSelectedGame → syncOnline). */
  | { type: "follow"; url: string }
  /** 打开网页 (openYikeLiveWeb). */
  | { type: "web" }
  | { type: "report"; report: LiveReport };

export interface BarPayload {
  language: Language;
}

export type BarInput =
  | { type: "ready" }
  /** 后退 (browser_.goBack when it can). */
  | { type: "back" }
  /** 加载 or Enter in the address field: load `url` and follow page addresses from now on. */
  | { type: "load"; url: string; enter: boolean }
  /** 停止同步 (stopYikeBrowserSync(true)). */
  | { type: "stop" }
  | { type: "report"; address: string };
