// 引擎对战 entries of the command catalog (Menu.java 对局 menu 2979-2990 and 3097-3159, the double
// menu's 新对局 popup 9109-9135 and Input.java Alt+E), and the match's one-shot messages and 胜率图
// request.
// Rust owns the match; this module sends Match commands and shows its view.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { isApiError, writeMatchWinrate } from "../contract/client";
import type { MatchCommand, MatchNotice, MatchOutcomeView, MatchScoreView, MatchView, MatchWinrateImageRequest, UnrestoredReason } from "../contract/generated";
import { ask, message } from "../dialogs";
import { format, product, registerProduct, t } from "../i18n";
import { messageNoModal, openEngineGameDialog, openManual, reviseBatchLimit } from "./dialogs";
import { parameterFailures, parametersPanelOpen } from "./parameters";
import { openingLocateReason } from "../search/locateRules";
import { matchActive, matchUi, sideName } from "./state";

function match(op: MatchCommand) {
  return run({ type: "Match", op });
}

/** EngineGameDesktop.stop / togglePause: nothing without a match. */
function stopMatch(): void {
  if (matchActive()) void match({ type: "Stop" });
}

function toggleMatchPause(): void {
  if (matchActive()) void match({ type: "Pause" });
}

registerProduct({
  ZhCn: {
    matchOpeningDuringGame: "人机对局正在进行，打开“引擎对局”会结束这盘棋。请先结束对局，再查看开局参数。",
    matchOpeningDuringCoach: "AI 陪练正在进行，打开“引擎对局”会结束陪练。请先结束陪练，再查看开局参数。",
    matchOpeningNeedsAnalysis: "开局广度和开局激进度只用于分析模式的引擎对局。在“引擎对局”的“更多设置”中选择“分析模式”后，这两项才会出现。",
  },
  ZhTw: {
    matchOpeningDuringGame: "人機對局正在進行，開啟「引擎對局」會結束這盤棋。請先結束對局，再查看開局參數。",
    matchOpeningDuringCoach: "AI 陪練正在進行，開啟「引擎對局」會結束陪練。請先結束陪練，再查看開局參數。",
    matchOpeningNeedsAnalysis: "開局廣度和開局激進度只用於分析模式的引擎對局。在「引擎對局」的「更多設定」中選擇「分析模式」後，這兩項才會出現。",
  },
  EnUs: {
    matchOpeningDuringGame: "A game against the engine is in progress, and opening the Engine game dialog would end it. End the game first, then look at the opening parameters.",
    matchOpeningDuringCoach: "AI Coach is in progress, and opening the Engine game dialog would end it. End AI Coach first, then look at the opening parameters.",
    matchOpeningNeedsAnalysis: "Opening breadth and opening aggressiveness apply only to analysis-mode engine games. Choose “AnalyzeMode” under “MoreSettings” in the Engine game dialog to show them.",
  },
  JaJp: {
    matchOpeningDuringGame: "エンジンとの対局中です。「エンジン同士の対局設定」を開くとこの対局は終了します。対局を終えてから開始時のパラメータを確認してください。",
    matchOpeningDuringCoach: "AI コーチの最中です。「エンジン同士の対局設定」を開くと AI コーチは終了します。AI コーチを終えてから開始時のパラメータを確認してください。",
    matchOpeningNeedsAnalysis: "開始時の幅と開始時の積極性は、分析モードのエンジン対局でのみ使われます。「エンジン同士の対局設定」の「詳細設定」で「分析モード」を選ぶと表示されます。",
  },
  KoKr: {
    matchOpeningDuringGame: "엔진과의 대국이 진행 중이며, [신규대국(AI끼리)]를 열면 이 대국이 끝납니다. 대국을 먼저 끝낸 뒤 시작 매개변수를 확인하세요.",
    matchOpeningDuringCoach: "AI 코치가 진행 중이며, [신규대국(AI끼리)]를 열면 AI 코치가 끝납니다. AI 코치를 먼저 끝낸 뒤 시작 매개변수를 확인하세요.",
    matchOpeningNeedsAnalysis: "시작 폭과 시작 공격성은 분석 모드 엔진 대국에서만 쓰입니다. [신규대국(AI끼리)]의 [추가설정]에서 [분석 모드]를 고르면 나타납니다.",
  },
  ThTh: {
    matchOpeningDuringGame: "กำลังเล่นเกมกับเอนจินอยู่ การเปิดหน้าต่าง “เกมเครื่องยนต์” จะจบเกมนี้ โปรดจบเกมก่อน แล้วจึงดูพารามิเตอร์เริ่มต้น",
    matchOpeningDuringCoach: "กำลังใช้โค้ช AI อยู่ การเปิดหน้าต่าง “เกมเครื่องยนต์” จะจบการฝึกกับโค้ช AI โปรดจบการฝึกก่อน แล้วจึงดูพารามิเตอร์เริ่มต้น",
    matchOpeningNeedsAnalysis: "ความกว้างเริ่มต้นและความก้าวร้าวเริ่มต้นใช้กับเกมเอนจินในโหมดวิเคราะห์เท่านั้น เลือก “โหมดวิเคราะห์” ใน “การตั้งค่าเพิ่มเติม” ของหน้าต่าง “เกมเครื่องยนต์” แล้วจึงจะแสดง",
  },
});

Object.assign(COMMANDS, {
  "menu.newEngineGame": { run: () => void openEngineGameDialog() },
  "match.opening-wrn": { run: () => void openEngineGameDialog("wrn"), disabledReason: () => openingLocateReason(state.view) },
  "match.opening-pda": { run: () => void openEngineGameDialog("pda"), disabledReason: () => openingLocateReason(state.view) },
  "menu.breakEngineGame": { run: stopMatch },
  "menu.pauseEngineGame": { run: toggleMatchPause },
  "menu.changeEngineGameNumbers": { run: () => void reviseBatchLimit() },
  "menu.intervention": { run: openManual },
} satisfies Record<string, CommandEntry>);

/** Input.keyPressed VK_E with Alt: LizzieFrame.startEngineGameDialog. True when handled. */
export function handleMatchKey(e: KeyboardEvent): boolean {
  if (e.code !== "KeyE" || !e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return false;
  void openEngineGameDialog();
  return true;
}

const firstWins = (s: MatchScoreView) => s.first_black_wins + s.first_white_wins;
const secondWins = (s: MatchScoreView) => s.second_black_wins + s.second_white_wins;

/** EngineManager.writeToFile's title (1962-1978): score, engines, the first engine's win rate and
 *  its two-sigma Wilson interval over the games played. */
function batchTitle(score: MatchScoreView, games: number, first: string, second: string): string {
  const a = firstWins(score), b = secondWins(score);
  const wr = a + b === 0 ? 0 : a / (a + b);
  const zxwr2 = (wr + 4 / (2 * games)) / (1 + 4 / games);
  const zxwrc2 = (2 * Math.sqrt((wr * (1 - wr)) / games + 4 / (2 * games * (2 * games)))) / (1 + 4 / games);
  return `${a}:${b} ${first} VS ${second}${t("EngineGameInfo.titleWinRate")}${(wr * 100).toFixed(1)}% 2σ ${(zxwr2 * 100).toFixed(2)}%±${(zxwrc2 * 100).toFixed(2)}%`;
}

/** The batch end message (EngineManager.java:2196-2211, 2297-2325). */
function batchEndText(o: MatchOutcomeView, m: MatchView): string {
  const s = o.score;
  let passAndMove = "";
  if (s.double_passes > 0) passAndMove = `${t("EngineGameInfo.doublePassGame")}${s.double_passes}`;
  if (s.max_moves > 0) passAndMove += `${passAndMove ? " " : ""}${t("EngineGameInfo.outOfMoveGame")}${s.max_moves}`;
  return `${t("EngineGameInfo.batchGameEndAndScore")}${m.first_name}   ${firstWins(s)}:${secondWins(s)}   ${m.second_name}${passAndMove ? " " : ""}${passAndMove},${t("EngineGameInfo.engineGameEndHintKifuPos")}${o.games_dir}`;
}

/** The single game end message (EngineManager.java:2326-2360). */
function gameEndText(o: MatchOutcomeView, m: MatchView): string {
  let text = t("EngineGameInfo.gameFinished");
  if (o.end === "MaxMoves") text += t("EngineGameInfo.finishedByMoves");
  else if (o.end === "DoublePass") text += t("EngineGameInfo.finishedByDoublePass");
  else if (o.winner === "White") text += `${t("GameInfoDialog.white")}(${sideName(m, false)})${t("EngineGameInfo.finishedWin")}`;
  else text += `${t("GameInfoDialog.black")}(${sideName(m, true)})${t("EngineGameInfo.finishedWin")}`;
  if (state.view?.settings.matches.session.autoSave) text += `,${t("EngineGameInfo.engineGameEndHintKifuPos")}${o.games_dir}`;
  return text;
}

/** An engine the retiring game could not put back as it was (LizzieFrame.foregroundUnrestored). */
function unrestoredText(reason: UnrestoredReason): string {
  const cause = reason === "Stop" ? t("LizzieFrame.foregroundUnrestored.reason.stop") : t("LizzieFrame.foregroundUnrestored.reason.restore");
  return `${format(t("LizzieFrame.display.foregroundUnrestored"), cause)}\n${format(t("LizzieFrame.display.foregroundUnrestoredAction"), t("Menu.restartCurrentEngine"))}`;
}

/** finishClaimedEngineGameStop: the title and the message of a game that ended. */
function outcome(o: MatchOutcomeView): void {
  const m = matchUi.last;
  matchUi.pkTitle = "";
  if (!m) return;
  if (o.end === "UserStopped") {
    // A manual stop reports only a batch that had passed its first game.
    if (o.batch && o.ordinal > 1) messageNoModal(batchEndText(o, m));
  } else if (o.end !== "Retired") {
    if (o.batch) matchUi.pkTitle = batchTitle(o.score, o.ordinal, m.first_name, m.second_name);
    if (o.batch_finished) messageNoModal(o.batch ? batchEndText(o, m) : gameEndText(o, m));
  }
  for (const reason of o.unrestored) void message(t("Message.title"), unrestoredText(reason), "warning");
}

/** Notice texts by key; literal t() calls keep the keys in the Java string extraction. */
function noticeText(n: MatchNotice): string {
  const texts: Record<string, string> = {
    "EngineManager.engineGameStartFailed": t("EngineManager.engineGameStartFailed"),
    "EngineManager.engineGameSameEngine": t("EngineManager.engineGameSameEngine"),
    "EngineManager.engineGameBlackSettingWrong": t("EngineManager.engineGameBlackSettingWrong"),
    "EngineManager.engineGameWhiteSettingWrong": t("EngineManager.engineGameWhiteSettingWrong"),
    "EngineManager.parseAdvcanceTimeSettingsFailed": t("EngineManager.parseAdvcanceTimeSettingsFailed"),
    "BottomToolbar.genmoveStopHint": t("BottomToolbar.genmoveStopHint"),
    "LizzieFrame.engineGameStopFirstHint": t("LizzieFrame.engineGameStopFirstHint"),
    "NewEngineGameDialog.matchRulesCorrupt": t("NewEngineGameDialog.matchRulesCorrupt"),
    "Leelaz.resign": t("Leelaz.resign"),
    // MatchRulesTexts.failureMessage.
    "MatchRules.failed": t("MatchRules.failed"),
    "MatchRules.failed.mismatch": t("MatchRules.failed.mismatch"),
    "MatchRules.failed.consentRefused": t("MatchRules.failed.consentRefused"),
    "MatchRules.failed.queryFailed": t("MatchRules.failed.queryFailed"),
    "MatchRules.failed.setFailed": t("MatchRules.failed.setFailed"),
    "MatchRules.failed.capabilityFailed": t("MatchRules.failed.capabilityFailed"),
    "MatchRules.failed.queryUnconfirmed": t("MatchRules.failed.queryUnconfirmed"),
    "MatchRules.failed.incomplete": t("MatchRules.failed.incomplete"),
    "MatchRules.failed.timeout": t("MatchRules.failed.timeout"),
    // The opening parameters were not confirmed (the game did not start), the batch's end
    // could not give an engine its values back, or a revision in 对局参数 was not confirmed (the
    // game is paused); {0} names each engine and reason.
    "product.matchOpeningFailed": product("matchOpeningFailed", "{0}"),
    "EngineGameParameters.restoreFailed": t("EngineGameParameters.restoreFailed"),
    "EngineGameParameters.failed": t("EngineGameParameters.failed"),
  };
  const args = n.parameter_failures.length ? [parameterFailures(n.parameter_failures)] : n.arguments;
  let k = 0;
  return format(texts[n.message_key] ?? t(n.message_key), ...args).replace(/%s/g, () => args[k++] ?? "");
}

/** MatchRulesTexts.statusLabel for an engine that cannot confirm the rules. */
function consentReason(key: string): string {
  const texts: Record<string, string> = {
    "MatchRules.unconfirmed": t("MatchRules.unconfirmed"),
    "MatchRules.reason.QUERY_REJECTED": t("MatchRules.reason.QUERY_REJECTED"),
    "MatchRules.reason.QUERY_TIMEOUT": t("MatchRules.reason.QUERY_TIMEOUT"),
    "MatchRules.reason.INVALID_READBACK": t("MatchRules.reason.INVALID_READBACK"),
    "MatchRules.reason.SET_REJECTED": t("MatchRules.reason.SET_REJECTED"),
    "MatchRules.reason.SET_TIMEOUT": t("MatchRules.reason.SET_TIMEOUT"),
    "MatchRules.reason.LIST_COMMANDS_TIMEOUT": t("MatchRules.reason.LIST_COMMANDS_TIMEOUT"),
    "MatchRules.reason.LIST_COMMANDS_FAILED": t("MatchRules.reason.LIST_COMMANDS_FAILED"),
    "MatchRules.reason.OCCUPIED": t("MatchRules.reason.OCCUPIED"),
    "MatchRules.reason.SEND_FAILED": t("MatchRules.reason.SEND_FAILED"),
  };
  return texts[key] ?? t(key);
}

/** SwingMatchRulesConsent / MatchRulesTexts.consentMessage: the engines that cannot confirm the
 *  rules, each "name (command) (colour): status", and the target rules; 是 lets the batch go on. */
async function askRulesConsent(m: MatchView): Promise<void> {
  const engines = m.rules_unverified.map((e) => {
    const identity = e.name && e.commands ? `${e.name} (${e.commands})` : e.name || e.commands;
    const colour = e.side === "Black" ? t("MatchRules.black") : e.side === "White" ? t("MatchRules.white") : t("MatchRules.unconfirmed");
    return format(t("MatchRules.consentEngine"), identity, colour, consentReason(e.reason_key));
  });
  const body = document.createElement("div");
  body.className = "match-consent";
  body.textContent = format(t("MatchRules.consentMessage"), engines.join("\n"), m.rules_target_name);
  const allow = await ask(t("MatchRules.consentTitle"), body, product("yes"), product("no"));
  await match({ type: "ConfirmRules", game: m.game, allow });
}

/** LizzieFrame.saveImage(statx, staty, grw * 1.03, grh + stath): the statistics strip and the win-
 *  rate graph as shown, from the strip's top left. */
async function winrateImage(): Promise<number[]> {
  const stat = document.getElementById("stat") as HTMLCanvasElement | null;
  const graph = document.getElementById("graph") as HTMLCanvasElement | null;
  const parts = [stat, graph].filter((c): c is HTMLCanvasElement => !!c && c.getClientRects().length > 0 && c.width > 0);
  if (!graph || !parts.includes(graph)) throw new Error("the win-rate graph is not shown");
  const g = graph.getBoundingClientRect();
  const origin = stat && parts.includes(stat) ? stat.getBoundingClientRect() : g;
  const scale = graph.width / g.width;
  const out = document.createElement("canvas");
  out.width = Math.round(g.width * 1.03 * scale);
  out.height = Math.round((g.bottom - origin.top) * scale);
  const ctx = out.getContext("2d")!;
  ctx.fillStyle = "#fff";
  ctx.fillRect(0, 0, out.width, out.height);
  for (const c of parts) {
    const r = c.getBoundingClientRect();
    ctx.drawImage(c, (r.left - origin.left) * scale, (r.top - origin.top) * scale, r.width * scale, r.height * scale);
  }
  const png = Promise.withResolvers<Blob | null>();
  out.toBlob(png.resolve, "image/png");
  const blob = await png.promise;
  if (!blob) throw new Error("PNG");
  return [...new Uint8Array(await blob.arrayBuffer())];
}

/** 自动保存胜率图: the adapter writes the PNG and reports success or its write failure to the core
 *  itself; only a picture that could not be made, or an adapter error before it reported, is
 *  reported here, so the core goes on to the next game. */
async function saveWinrate(request: MatchWinrateImageRequest): Promise<void> {
  let bytes: number[];
  try {
    bytes = await winrateImage();
  } catch (e) {
    void match({ type: "WinrateImageDone", game: request.game, error: e instanceof Error ? e.message : String(e) });
    return;
  }
  try {
    await writeMatchWinrate(request.game, bytes);
  } catch (e) {
    void match({ type: "WinrateImageDone", game: request.game, error: isApiError(e) ? e.message : String(e) });
  }
}

let requested: number | null = null;
/** The game whose rules consent was asked (once per game; the core reuses it in the batch). */
let consentAsked: number | null = null;

registerViewHandler((view) => {
  const m = view.engine_match;
  // prepareAcceptedEngineGameStart: every game starts without the batch title.
  if (m && m.game !== matchUi.last?.game) matchUi.pkTitle = null;
  if (m) matchUi.last = m;
  if (m?.rules_consent_required && m.game !== consentAsked) {
    consentAsked = m.game;
    void askRulesConsent(m);
  }
  if (view.match_outcome) outcome(view.match_outcome);
  for (const n of view.match_notices) {
    // SwingEngineGameChrome.showParameterFailureOnce: the open panel shows it inline instead.
    if (n.message_key === "EngineGameParameters.failed" && parametersPanelOpen()) continue;
    void message(t("Message.title"), noticeText(n), "warning");
  }
  const image = view.match_winrate_image;
  if (image && image.game !== requested) {
    requested = image.game;
    void saveWinrate(image);
  } else if (!image) requested = null;
});
