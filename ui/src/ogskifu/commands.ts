// OGS 棋谱 entries (同步 > OGS棋谱 and the toolbar's OGS button; Java `Menu.openKifu(true)`),
// function search's 打开棋谱后此窗口 and the relay to its window. The core owns the search and its
// list; the window shows them.
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen } from "@tauri-apps/api/event";
import { registerViewHandler, run, state } from "../app";
import { frameMetrics } from "../appearance/state";
import { COMMANDS, type CommandEntry } from "../commands";
import { afterGetRequest } from "../foxkifu/afterGet";
import type { WorkbenchView } from "../contract/generated";
import { language, registerProduct, t } from "../i18n";
import { ogsAfterGetReason } from "../search/locateRules";
import {
  OGS_KIFU_DATA_EVENT,
  OGS_KIFU_INPUT_EVENT,
  OGS_KIFU_LABEL,
  type OgsKifuInput,
  type OgsKifuPayload,
  type OgsKifuReport,
} from "./model";
import "./text";

let lastSent = "";
let report: OgsKifuReport | null = null;
let opening = false;

/** openKifu(true): the window comes up (or forward) and browsing may preload the analysis engine.
 *  The JFrame follows the main window's 总在最前. */
export async function openOgsKifu(): Promise<void> {
  if (opening) return;
  opening = true;
  try {
    await run({ type: "OgsKifu", op: { type: "Open" } });
    await invoke("open_ogs_kifu", {
      title: t("Menu.ogsKifu"),
      onTop: state.view?.settings.panels.alwaysOnTop ?? false,
    });
  } finally {
    opening = false;
  }
}

/** Function search's 打开棋谱后此窗口: the window comes up with that choice located, unchanged. */
const afterGet = afterGetRequest(openOgsKifu, () => {
  if (state.view) relay(state.view);
});

function relay(v: WorkbenchView): void {
  const payload: OgsKifuPayload = { language: language(), frameFontSize: frameMetrics().frameFontSize, locate: afterGet.request, view: v.ogs_kifu };
  const text = JSON.stringify(payload);
  if (text === lastSent) return;
  lastSent = text;
  void emitTo(OGS_KIFU_LABEL, OGS_KIFU_DATA_EVENT, payload);
}

registerViewHandler(relay);

/** Listens to the window: a (re)loaded page asks for the current state. */
export async function startOgsKifu(): Promise<void> {
  await listen<OgsKifuInput>(OGS_KIFU_INPUT_EVENT, (e) => {
    if (e.payload.type === "ready") {
      lastSent = "";
      if (state.view) relay(state.view);
    } else if (e.payload.type === "located") {
      afterGet.located(e.payload.request);
    } else report = e.payload.report;
  });
}

/** What the acceptance probe reports: what the window last showed. */
export function ogsKifuReport() {
  return report;
}

registerProduct({
  ZhCn: { ogsAfterGetBusy: "OGS棋谱正在搜索或下载棋谱，完成后才能设置“打开棋谱后此窗口”。" },
  ZhTw: { ogsAfterGetBusy: "OGS棋譜正在搜尋或下載棋譜，完成後才能設定「開啟棋譜後此視窗」。" },
  EnUs: { ogsAfterGetBusy: "OGS game records is searching or downloading a game. “After open game this frame” can be set once it finishes." },
  JaJp: { ogsAfterGetBusy: "OGS棋譜で検索またはダウンロードの最中です。終わってから「棋譜を開いた後このウィンドウを」を設定してください。" },
  KoKr: { ogsAfterGetBusy: "OGS 기보에서 검색 또는 다운로드가 진행 중입니다. 끝난 뒤 [기보 열기 후 이 윈도우를] 항목을 설정하세요." },
  ThTh: { ogsAfterGetBusy: "บันทึกเกม OGS กำลังค้นหาหรือดาวน์โหลดอยู่ เมื่อเสร็จแล้วจึงตั้งค่า “หลังจากเปิดบันทึกเกม ให้หน้าต่างนี้” ได้" },
});

Object.assign(COMMANDS, {
  "menu.ogsKifu": { run: () => void openOgsKifu() },
  // The window holds 打开棋谱后此窗口 while it searches or downloads: offered once that is over.
  "ogs.after-get": { run: () => void afterGet.locate(), locate: () => afterGet.locate(), disabledReason: () => ogsAfterGetReason(state.view) },
} satisfies Record<string, CommandEntry>);
