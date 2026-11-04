// 引擎对战 entries of the command catalog (Menu.java 对局 menu 2979-2990 and 3097-3159, the double
// menu's 新对局 popup 9109-9135, Input.java Alt+E, the detailed toolbar's 引擎对战 panel and
// FunctionCatalog legacy.engine-game-pause), and the match's one-shot messages and 胜率图 request.
// Rust owns the match; this module sends Match commands and shows its view.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { isApiError, writeMatchWinrate } from "../contract/client";
import type { MatchCommand, MatchNotice, MatchOutcomeView, MatchScoreView, MatchView, MatchWinrateImageRequest, UnrestoredReason } from "../contract/generated";
import { ask, message } from "../dialogs";
import { format, product, t } from "../i18n";
import { matchRuleOptions, messageNoModal, openEngineGameDialog, openManual, reviseBatchLimit } from "./dialogs";
import { matchActive, matchUi, sideName } from "./state";

function match(op: MatchCommand) {
  return run({ type: "Match", op });
}

/** EngineGameDesktop.stop / togglePause: nothing without a match. */
export function stopMatch(): void {
  if (matchActive()) void match({ type: "Stop" });
}

export function toggleMatchPause(): void {
  if (matchActive()) void match({ type: "Pause" });
}

/** BottomToolbar.startEngineGame (4198-4226): corrupt saved 本局规则 is reported and, like rules
 *  never chosen, opens the 引擎对局 dialog instead of starting. */
export async function startFromToolbar(): Promise<void> {
  const m = state.view?.settings.matches;
  if (!m) return;
  if (m.rules_corrupt) {
    await message(t("Message.title"), t("NewEngineGameDialog.matchRulesCorrupt"));
    await openEngineGameDialog();
    return;
  }
  if (m.settings.rules.trim() === "") {
    await openEngineGameDialog();
    return;
  }
  await match({ type: "StartToolbar" });
}

Object.assign(COMMANDS, {
  "menu.newEngineGame": { run: () => void openEngineGameDialog() },
  "menu.breakEngineGame": { run: stopMatch },
  "menu.pauseEngineGame": { run: toggleMatchPause },
  "menu.changeEngineGameNumbers": { run: () => void reviseBatchLimit() },
  "menu.intervention": { run: openManual },
  // FunctionCatalog ACTION: the toolbar's 暂停/继续.
  "legacy.engine-game-pause": { run: toggleMatchPause },
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
  };
  let k = 0;
  return format(texts[n.message_key] ?? t(n.message_key), ...n.arguments).replace(/%s/g, () => n.arguments[k++] ?? "");
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
  const target = m.rules_target_choice === null
    ? t("NewEngineGameDialog.matchRules.custom")
    : matchRuleOptions().find(([name]) => name === m.rules_target_choice)?.[1] ?? t("NewEngineGameDialog.matchRules.custom");
  const body = document.createElement("div");
  body.className = "match-consent";
  body.textContent = format(t("MatchRules.consentMessage"), engines.join("\n"), target);
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
  for (const n of view.match_notices) void message(t("Message.title"), noticeText(n), "warning");
  const image = view.match_winrate_image;
  if (image && image.game !== requested) {
    requested = image.game;
    void saveWinrate(image);
  } else if (!image) requested = null;
});
