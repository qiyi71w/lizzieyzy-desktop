// 野狐棋谱 entries (同步 > 野狐棋谱, the toolbar's 野狐棋谱 button and the bottom 同步 popup;
// `openFoxReq`) and the relay to its window. The core owns the search and its list; the window
// shows them.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { WorkbenchView } from "../contract/generated";
import { language, t } from "../i18n";
import { FOX_KIFU_DATA_EVENT, FOX_KIFU_INPUT_EVENT, FOX_KIFU_LABEL, type FoxKifuInput, type FoxKifuPayload, type FoxKifuReport } from "./model";

let lastSent = "";
let report: FoxKifuReport | null = null;
let opening = false;

/** openFoxReq: the window comes up (or forward) and browsing may preload the analysis engine.
 *  The JFrame follows the main window's 总在最前. */
export async function openFoxKifu(): Promise<void> {
  if (opening) return;
  opening = true;
  try {
    await run({ type: "FoxKifu", op: { type: "Open" } });
    await invoke("open_fox_kifu", {
      title: t("FoxKifuDownload.title"),
      onTop: state.view?.settings.panels.alwaysOnTop ?? false,
    });
  } finally {
    opening = false;
  }
}

function relay(v: WorkbenchView): void {
  const payload: FoxKifuPayload = { language: language(), view: v.fox_kifu };
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(FOX_KIFU_LABEL, FOX_KIFU_DATA_EVENT, payload);
}

registerViewHandler(relay);

/** Listens to the window: a (re)loaded page asks for the current state. */
export async function startFoxKifu(): Promise<void> {
  await listen<FoxKifuInput>(FOX_KIFU_INPUT_EVENT, (e) => {
    if (e.payload.type === "ready") {
      lastSent = "";
      if (state.view) relay(state.view);
    } else report = e.payload.report;
  });
}

/** What the acceptance probe reports: what the window last showed. */
export function foxKifuReport() {
  return report;
}

Object.assign(COMMANDS, {
  "menu.foxKifu": { run: () => void openFoxKifu() },
} satisfies Record<string, CommandEntry>);
