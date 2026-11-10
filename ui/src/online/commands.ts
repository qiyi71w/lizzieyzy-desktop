// 同步 menu's 弈客 entries and live options (Menu.java:4180-4352), 文件 > 打开在线链接(Q), the
// bottom toolbar's 弈客 popup (BottomToolbar.java:1182-1213), Shift+O / Q (Input.java:548-558,
// 717-724), and the relays to 弈客直播中心 and the 弈客 web window's toolbar. The core owns the
// follow, the live list and both options (WorkbenchView.online); this module follows it, opens the
// windows and turns their input into commands.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { OnlineCommand, WorkbenchView } from "../contract/generated";
import type { MenuNode } from "../chrome/javaMenus";
import { language, t } from "../i18n";
import { followedElsewhere, openOnlineDialog } from "./dialog";
import {
  BAR_DATA_EVENT,
  BAR_INPUT_EVENT,
  BAR_LABEL,
  LIVE_DATA_EVENT,
  LIVE_INPUT_EVENT,
  LIVE_LABEL,
  YIKE_GAME_URL,
  YIKE_LIVE_URL,
  type BarInput,
  type BarPayload,
  type LiveInput,
  type LivePayload,
  type LiveReport,
} from "./model";
import { onlineText } from "./text";

const online = (op: OnlineCommand) => run({ type: "Online", op });

// ------------------------------------------------------------------ 弈客 web window

/** BrowserFrame.baseTitle: the title the window was last opened with ("" before the first open). */
let baseTitle = "";
/** BrowserFrame.lastSyncStatus: the status the title shows. */
let lastBrowserStatus = "";
let browserAddress = "";

/** reportSyncStatus's text; a blank one is none (Utils.isBlank). */
function browserStatus(v: WorkbenchView | null): string {
  const text = v?.online.browser_status ? onlineText(v.online.browser_status) : "";
  return text.trim() ? text : "";
}

/** LizzieFrame.bowser(url, title, true): the web window opens (or is reused) at `url` titled
 *  `title` (openURL, BrowserFrame.java:653-669); a later status change extends the title. */
function openBrowser(url: string, title: string): void {
  baseTitle = title;
  lastBrowserStatus = browserStatus(state.view);
  void invoke("open_yike_browser", { url, title });
}

/** BrowserFrame.setSyncStatus (BrowserFrame.java:1072-1084): base title, then "  —  " + status. */
function browserTitle(v: WorkbenchView): void {
  const status = browserStatus(v);
  if (status === lastBrowserStatus) return;
  lastBrowserStatus = status;
  if (baseTitle) void invoke("yike_browser_set_title", { title: status ? `${baseTitle}  —  ${status}` : baseTitle });
}

function barInput(i: BarInput): void {
  switch (i.type) {
    case "ready": {
      const payload: BarPayload = { language: language() };
      void emitTo(BAR_LABEL, BAR_DATA_EVENT, payload);
      return;
    }
    case "back":
      void invoke("yike_browser_back");
      return;
    case "load":
      browserAddress = i.url;
      followedElsewhere();
      // 加载: startYikeSync then loadURL; Enter: loadURL then startYikeSync (BrowserFrame.java:289-308, 412-421).
      if (i.enter) {
        void invoke("yike_browser_navigate", { url: i.url });
        void online({ type: "BrowserLoad", url: i.url });
      } else {
        void online({ type: "BrowserLoad", url: i.url });
        void invoke("yike_browser_navigate", { url: i.url });
      }
      return;
    case "stop":
      void online({ type: "BrowserStopSync" });
      return;
    case "report":
      browserAddress = i.address;
      return;
  }
}

// ------------------------------------------------------------------ 弈客直播中心

/** The window was opened in this session (LizzieFrame.yikeLiveDialog != null). */
let liveOpened = false;
let liveReport: LiveReport | null = null;
let lastLive = "";
let lastReveal: number | null = null;

/** LizzieFrame.openYikeLiveDialog: shown and focused, then refreshIfEmpty. */
function openLive(): void {
  liveOpened = true;
  void invoke("open_yike_live", { title: t("YikeLiveDialog.title") });
  const live = state.view?.online.live;
  if (live && live.games.length === 0) void online({ type: "LoadLiveList", category: live.category, more: false });
}

function relayLive(v: WorkbenchView): void {
  const payload: LivePayload = { language: language(), live: v.online.live, status: v.online.status };
  const text = JSON.stringify(payload);
  if (text === lastLive) return;
  lastLive = text;
  void emitTo(LIVE_LABEL, LIVE_DATA_EVENT, payload);
}

function liveInput(i: LiveInput): void {
  switch (i.type) {
    case "ready":
      lastLive = "";
      if (state.view) relayLive(state.view);
      return;
    case "load":
      void online({ type: "LoadLiveList", category: i.category, more: i.more });
      return;
    case "follow":
      followedElsewhere();
      void online({ type: "FollowLive", url: i.url });
      return;
    case "web":
      COMMANDS["menu.yikeLiveWeb"].run();
      return;
    case "report":
      liveReport = i.report;
      return;
  }
}

// ------------------------------------------------------------------ entries

Object.assign(COMMANDS, {
  // Menu.openUrl (Menu.java:276-285).
  "files.url": { run: () => void openOnlineDialog() },
  "menu.yikeLive": { run: openLive },
  // openYikeLiveWeb: the live page titled 弈客直播.
  "menu.yikeLiveWeb": { run: () => openBrowser(YIKE_LIVE_URL, t("BottomToolbar.yikeLive")) },
  "menu.yikeRoom": { run: () => openBrowser(YIKE_GAME_URL, t("BottomToolbar.yikeRoom")) },
  // The check items show the saved options (the menu listener at Menu.java:4381-4390).
  "menu.EnableEnterYikeGame": {
    run: () => void online({ type: "SetOpenHtmlOnLive", enabled: !state.view?.online.open_html_on_live }),
    checked: () => state.view?.online.open_html_on_live ?? true,
  },
  "menu.alwaysGoToLastMove": {
    run: () => void online({ type: "SetAlwaysGotoLast", enabled: !state.view?.online.always_goto_last }),
    checked: () => state.view?.online.always_goto_last ?? false,
  },
} satisfies Record<string, CommandEntry>);

/** The bottom toolbar's 同步 popup (`yike`, BottomToolbar.java:1182-1273). Entries other features
 *  own resolve through the catalog like the menu bar's. */
export const LIVE_MENU: MenuNode[] = [
  { t: "item", key: "BottomToolbar.yikeLive", id: "menu.yikeLive" },
  { t: "item", key: "Menu.yikeLiveWeb", id: "menu.yikeLiveWeb" },
  { t: "item", key: "BottomToolbar.yikeRoom", id: "menu.yikeRoom" },
  { t: "item", key: "Menu.foxKifu", id: "menu.foxKifu" },
  { t: "item", key: "BottomToolbar.syncBoard", id: "sync.board", when: "windows" },
  { t: "item", key: "Menu.webBoardStart", id: "menu.webBoardToggle" },
  { t: "item", key: "Menu.webBoardForceExitTrial", id: "sync.exit-web-trial" },
];

/** Shift+O 弈客直播 (Input.java:553-554, InputIndependentMainBoard.java:331-332; Ctrl+O and Alt+O
 *  come first) and plain Q 打开在线链接 (Input.java:717-724; Ctrl+Q 点目 and Alt+Q 浮动主棋盘 are
 *  handled before this). */
export function handleOnlineKey(e: KeyboardEvent): boolean {
  if (e.ctrlKey || e.altKey || e.metaKey) return false;
  if (e.code === "KeyO" && e.shiftKey) COMMANDS["menu.yikeLive"].run();
  else if (e.code === "KeyQ") COMMANDS["files.url"].run();
  else return false;
  return true;
}

registerViewHandler((v) => {
  // revealYikeLiveSyncStatus: an opened 直播中心 comes to the front.
  if (lastReveal !== null && v.online.reveal !== lastReveal && liveOpened) void invoke("open_yike_live", { title: t("YikeLiveDialog.title") });
  lastReveal = v.online.reveal;
  relayLive(v);
  browserTitle(v);
});

/** Listens to 弈客直播中心 and the web window's toolbar. */
export async function startOnline(): Promise<void> {
  await listen<LiveInput>(LIVE_INPUT_EVENT, (e) => liveInput(e.payload));
  await listen<BarInput>(BAR_INPUT_EVENT, (e) => barInput(e.payload));
}

/** What the acceptance probe reports: 直播中心 as it drew itself, the web window's title and address. */
export function onlineReport() {
  return { live: liveOpened ? liveReport : null, browser: { baseTitle, status: lastBrowserStatus, address: browserAddress } };
}
