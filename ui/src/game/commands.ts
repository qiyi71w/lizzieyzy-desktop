// 人机对弈 entries of the command catalog (Menu.java 棋局 menu 2952-3195, the double menu's game
// buttons 9094-9222, Input.java N / Enter / Alt+,), the 读秒 text and voices, and the game's
// one-shot messages. Rust owns the game; this module sends Game commands and shows its view.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { GameClockView, GameCommand, GameKind, GameNotice, Stone } from "../contract/generated";
import { message } from "../dialogs";
import { setCandidates } from "../display/state";
import { plainJava } from "../engine/tuningText";
import { t } from "../i18n";
import { openNewAnaGame, openNewGame, openSetAiTime } from "./dialogs";

function game(op: GameCommand) {
  return run({ type: "Game", op });
}

/** Lizzie.leelaz.noAnalyze: the current engine cannot analyse (no engine counts as able). */
function noAnalyze(): boolean {
  const fg = state.view?.engine.foreground;
  return !!fg && !fg.can_analyse;
}

/** startAnalyzeGameDialogReserved: without analysis the Genmove dialog opens instead. */
function openAnalysisGame(): void {
  void (noAnalyze() ? openNewGame() : openNewAnaGame());
}

function cont(kind: GameKind, human: Stone): CommandEntry {
  return { run: () => void game({ type: "Continue", kind, human }) };
}

const inGame = () => !!state.view?.game;

Object.assign(COMMANDS, {
  "menu.newGenmoveGame": { run: () => void openNewGame() },
  // InfoMenuItem: the "?" beside it shows LizzieFrame.showAnalyzeGenmoveInfo.
  "menu.newAnalyzeModeGame": {
    run: openAnalysisGame,
    button: {
      label: () => "?",
      run: () => void message(t("LizzieFrame.aboutAnalyzeGenmoveInfoTitle"), plainJava(t("LizzieFrame.aboutAnalyzeGenmoveInfo"))),
    },
  },
  // 续弈[AI执黑] is the human playing White (continueAiPlaying(.., playerIsB = false, ..)).
  "Menu.continueGenmoveGameAsWhite": cont("Genmove", "White"),
  "Menu.continueGenmoveGameAsBlack": cont("Genmove", "Black"),
  "Menu.continueAnalyzeGameAsWhite": cont("Analysis", "White"),
  "Menu.continueAnalyzeGameAsBlack": cont("Analysis", "Black"),
  "menu.setAiTime": { run: () => void openSetAiTime() },
  // 终止人机对局(空格): togglePonderMannul, which outside a game toggles the analysis.
  "game.stop-human": { run: () => void run({ type: "ToggleAnalysis" }) },
  "toolbar.game-pause": { run: () => void game({ type: "Pause" }), enabled: inGame },
  "toolbar.game-stop": { run: () => void game({ type: "End" }), visible: inGame },
  // doubleMenuResign: no confirmation.
  "toolbar.game-resign": { run: () => void game({ type: "Resign" }), visible: inGame },
} satisfies Record<string, CommandEntry>);

/** Input.keyPressed VK_N (Alt with analysis: the 分析模式 dialog), VK_ENTER (续弈, Alt for 分析模式)
 *  and Alt+VK_COMMA (LizzieFrame.genmove). True when the key was handled. */
export function handleGameKey(e: KeyboardEvent): boolean {
  if (e.metaKey) return false;
  if (e.code === "KeyN") {
    if (e.altKey && !noAnalyze()) void openNewAnaGame();
    else void openNewGame();
    return true;
  }
  if (e.key === "Enter") {
    // A focused button takes its own Enter.
    if (e.target instanceof HTMLButtonElement || e.defaultPrevented) return false;
    void game({ type: "Continue", kind: e.altKey ? "Analysis" : "Genmove", human: null });
    return true;
  }
  if (e.code === "Comma" && e.altKey) {
    void game({ type: "EngineMove" });
    return true;
  }
  return false;
}

/** drawContDownForHuman's text: 时间m:s (while main time is left) and 读秒s(times). */
export function clockText(c: GameClockView): string {
  const main = c.minutes > 0 || c.seconds > 0 ? `${t("Byoyomi.time")}${c.minutes}:${c.seconds} ` : "";
  const byo = c.byo_seconds >= 0 ? ` ${t("Byoyomi.byoyomi")}${c.byo_seconds}(${c.byo_times})` : "";
  return main + byo;
}

/** Utils.showMsg text: the prefix (the engine name for Leelaz.resign), then the message. */
function noticeText(n: GameNotice): string {
  const text: Record<string, string> = {
    "LizzieFrame.startContinueGame": t("LizzieFrame.startContinueGame"),
    "LizzieFrame.passInGameTip": t("LizzieFrame.passInGameTip"),
    "LizzieFrame.sendTimes.kataGoTimeMismatch": t("LizzieFrame.sendTimes.kataGoTimeMismatch"),
    "EngineManager.parseAdvcanceTimeSettingsFailed": t("EngineManager.parseAdvcanceTimeSettingsFailed"),
    "Leelaz.blackWinAiResign": t("Leelaz.blackWinAiResign"),
    "Leelaz.whiteWinAiResign": t("Leelaz.whiteWinAiResign"),
    "Leelaz.resign": t("Leelaz.resign"),
    "Byoyomi.timeOutBlack": t("Byoyomi.timeOutBlack"),
    "Byoyomi.timeOutWhite": t("Byoyomi.timeOutWhite"),
  };
  const body = text[n.message_key] ?? t(n.message_key);
  return n.prefix ? `${n.prefix} ${body}` : body;
}

/** The game whose candidate boxes were set: the dialog's 显示黑/白 when it starts
 *  (toolbar.setChkShowBlack/White), both shown again once it is over (stopAiPlayingAndPolicy). */
let candidatesFor: number | null = null;

registerViewHandler((view) => {
  for (const n of view.game_notices) void message(t("Message.title"), noticeText(n));
  // Utils.playByoyomi: assets/sound/<n>.wav; playback failures are ignored.
  for (const n of view.byoyomi_voices) void new Audio(`/assets/sound/${n}.wav`).play().catch(() => undefined);
  const g = view.game;
  if (g && g.id !== candidatesFor) {
    candidatesFor = g.id;
    setCandidates(g.show_black, g.show_white);
  } else if (!g && candidatesFor !== null) {
    candidatesFor = null;
    setCandidates(true, true);
  }
});
