// 腾讯棋谱 entries (同步 > 腾讯棋谱 and the toolbar's 腾讯棋谱 button; `openTencentKifu`) and the
// relay to its window. The core owns the search and its list; the window shows them.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { WorkbenchView } from "../contract/generated";
import { language, t } from "../i18n";
import {
  TENCENT_KIFU_DATA_EVENT,
  TENCENT_KIFU_INPUT_EVENT,
  TENCENT_KIFU_LABEL,
  type TencentKifuInput,
  type TencentKifuPayload,
  type TencentKifuReport,
} from "./model";

let lastSent = "";
let report: TencentKifuReport | null = null;
let opening = false;

/** openTencentKifu: the window comes up (or forward) and browsing may preload the analysis
 *  engine. The JFrame follows the main window's 总在最前. */
export async function openTencentKifu(): Promise<void> {
  if (opening) return;
  opening = true;
  try {
    await run({ type: "TencentKifu", op: { type: "Open" } });
    await invoke("open_tencent_kifu", {
      title: t("TencentKifuDownload.title"),
      onTop: state.view?.settings.panels.alwaysOnTop ?? false,
    });
  } finally {
    opening = false;
  }
}

function relay(v: WorkbenchView): void {
  const payload: TencentKifuPayload = { language: language(), view: v.tencent_kifu };
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(TENCENT_KIFU_LABEL, TENCENT_KIFU_DATA_EVENT, payload);
}

registerViewHandler(relay);

/** Listens to the window: a (re)loaded page asks for the current state. */
export async function startTencentKifu(): Promise<void> {
  await listen<TencentKifuInput>(TENCENT_KIFU_INPUT_EVENT, (e) => {
    if (e.payload.type === "ready") {
      lastSent = "";
      if (state.view) relay(state.view);
    } else report = e.payload.report;
  });
}

/** What the acceptance probe reports: what the window last showed. */
export function tencentKifuReport() {
  return report;
}

Object.assign(COMMANDS, {
  "menu.tencentKifu": { run: () => void openTencentKifu() },
} satisfies Record<string, CommandEntry>);
