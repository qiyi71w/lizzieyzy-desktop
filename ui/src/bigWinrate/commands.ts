// 黑白胜率大字 in the main window (Java LizzieFrame.toggleBigWinrateWindow, BigWinrateWindow's timer):
// 显示 > 黑白胜率大字 and function search open and close the window. While it is open this module
// sends it the figures of the core's win-rate reading (the one the statistics bar shows) with the
// style, 窗口置顶 and fonts: only when something it shows changed, and figures at most once every
// 250 ms (Java's REFRESH_MILLIS). Closing the window stops all of it. The window's right-click menu
// choices run here as the core's operation; the shell owns the window and its bounds.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { WorkbenchView } from "../contract/generated";
import { frameMetrics, onAppearanceChange } from "../appearance/state";
import { language, t } from "../i18n";
import { panelFont, winrateFace } from "../workbench/paint";
import {
  BIG_WINRATE_CLOSED_EVENT,
  BIG_WINRATE_DATA_EVENT,
  BIG_WINRATE_INPUT_EVENT,
  bigWinrateFigures,
  type BigWinrateInput,
  type BigWinratePayload,
  type BigWinrateReport,
} from "./model";

/** src-tauri big_winrate::LABEL. */
const LABEL = "big-winrate";
/** Java BigWinrateWindow.REFRESH_MILLIS. */
const REFRESH_MS = 250;

let shown = false;
let toggling = false;
/** The page of the window being opened listens (its `ready`): it may say so before the shell's
 *  receipt that the window is open arrives. */
let pageReady = false;
/** Moves on with every close, so a receipt that arrives after its window already closed opens
 *  nothing. */
let closes = 0;
/** Ends the subscriptions to the views and the applied appearance, which last while the window
 *  is open. */
let unsubscribe: (() => void) | undefined;
/** The payload the window last got, as sent ("" when it has none yet), and the payload itself. */
let lastSent = "";
let last: BigWinratePayload | null = null;
let sentAt = Number.NEGATIVE_INFINITY;
/** A figure change waiting for the 250 ms since the last send. */
let timer: number | undefined;
/** Payloads sent while the window was open, for the acceptance probe. */
let sends = 0;
/** The 窗口置顶 last given to the window. */
let onTopApplied: boolean | null = null;
let report: BigWinrateReport | null = null;

function payload(v: WorkbenchView): BigWinratePayload {
  const s = v.settings.big_winrate;
  return {
    figures: bigWinrateFigures(v.statistics.reading),
    style: s.style,
    onTop: s.onTop,
    language: language(),
    figureFont: winrateFace(),
    labelFont: panelFont("ui"),
    frameFontSize: frameMetrics().frameFontSize,
  };
}

function send(p: BigWinratePayload, text: string): void {
  window.clearTimeout(timer);
  timer = undefined;
  lastSent = text;
  last = p;
  sentAt = performance.now();
  sends++;
  void emitTo(LABEL, BIG_WINRATE_DATA_EVENT, p).catch(() => {});
}

/** The page listens: it gets what to draw at once (Java refreshes as it is shown). */
function sendCurrent(): void {
  if (!state.view) return;
  const p = payload(state.view);
  send(p, JSON.stringify(p));
}

/** The 250 ms passed: the window gets the figures as they are now. */
function flush(): void {
  timer = undefined;
  const v = state.view;
  if (!shown || !v) return;
  const p = payload(v);
  const text = JSON.stringify(p);
  if (text !== lastSent) send(p, text);
}

/** Feeds the open window for a view as it arrives, whether or not the main window is drawn
 *  (minimised, Java's timer still runs), and for an applied appearance. It is subscribed to the
 *  views only while the window is open; nothing happens while what it shows is unchanged. */
function relay(v: WorkbenchView): void {
  const p = payload(v);
  if (onTopApplied !== p.onTop) {
    onTopApplied = p.onTop;
    // The shell switches it (src-tauri on_top): on Windows it also restacks the window.
    void invoke("set_window_on_top", { label: LABEL, onTop: p.onTop }).catch((e: unknown) => console.error("黑白胜率大字 窗口置顶 failed", e));
  }
  const text = JSON.stringify(p);
  if (text === lastSent) {
    window.clearTimeout(timer);
    timer = undefined;
    return;
  }
  // A change of look, language or fonts goes at once; figures wait out the 250 ms.
  const settings = !last || last.style !== p.style || last.onTop !== p.onTop || last.language !== p.language || last.figureFont !== p.figureFont || last.labelFont !== p.labelFont || last.frameFontSize !== p.frameFontSize;
  const wait = sentAt + REFRESH_MS - performance.now();
  if (settings || wait <= 0) send(p, text);
  else if (timer === undefined) timer = window.setTimeout(flush, wait);
}

/** The window closed: nothing more is sent, timed or relayed. */
function stop(): void {
  shown = false;
  pageReady = false;
  closes++;
  unsubscribe?.();
  unsubscribe = undefined;
  lastSent = "";
  last = null;
  onTopApplied = null;
  report = null;
  window.clearTimeout(timer);
  timer = undefined;
}

function toggle(): void {
  if (toggling) return;
  toggling = true;
  const closesBefore = closes;
  // This toggle opens the window: only its new page's ready counts.
  if (!shown) pageReady = false;
  void invoke<boolean>("toggle_big_winrate", { title: t("BigWinrateWindow.title") })
    .then((open) => {
      if (!open) return stop();
      // Its close box closed it before this receipt came: it is gone already.
      if (closes !== closesBefore) return;
      shown = true;
      lastSent = "";
      last = null;
      // The shell built it with the saved 窗口置顶.
      onTopApplied = state.view?.settings.big_winrate.onTop ?? null;
      if (!unsubscribe) {
        const views = registerViewHandler(relay);
        // A theme's fonts resolve after the view that changed them: the window redraws with them
        // without waiting for another view.
        const looks = onAppearanceChange(() => state.view && relay(state.view));
        unsubscribe = () => {
          views();
          looks();
        };
      }
      if (pageReady) sendCurrent();
    })
    .catch((e: unknown) => console.error("黑白胜率大字 failed", e))
    .finally(() => (toggling = false));
}

function input(i: BigWinrateInput): void {
  const s = state.view?.settings.big_winrate;
  switch (i.type) {
    case "ready":
      // Before the shell's receipt that the window is open, the receipt sends it.
      pageReady = true;
      if (shown) sendCurrent();
      break;
    case "style":
      if (s) void run({ type: "SetBigWinrate", style: i.style, on_top: s.onTop });
      break;
    case "onTop":
      if (s) void run({ type: "SetBigWinrate", style: s.style, on_top: i.onTop });
      break;
    case "report":
      if (shown) report = i.report;
      break;
  }
}

/** Listens to the window and the shell. */
export async function startBigWinrate(): Promise<void> {
  await listen<BigWinrateInput>(BIG_WINRATE_INPUT_EVENT, (e) => input(e.payload));
  await listen(BIG_WINRATE_CLOSED_EVENT, stop);
}

/** What the acceptance probe reports: whether the window is open, what it was last sent, how many
 *  payloads went to it, whether a send is waiting, and what it drew. */
export function bigWinrateReport() {
  return { shown, sends, pending: timer !== undefined, sent: last, window: report };
}

Object.assign(COMMANDS, {
  "menu.bigWinrateWindow": { run: toggle },
} satisfies Record<string, CommandEntry>);
