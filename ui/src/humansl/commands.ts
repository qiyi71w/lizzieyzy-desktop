// AI 陪练 entries of the command catalog (Menu.java 对局 > 新建对局 > AI 陪练 2992-3003, the menu
// strip's aiCoachButton 5699-5732 and FunctionCatalog game.ai-coach), the strip button's state
// (Menu.updateAiCoachState 11186-11231) and the session's one-shot messages. Rust owns the session;
// this module sends HumanSl commands and shows its view.
import { registerViewHandler, state, subscribe } from "../app";
import { COMMANDS, type CommandEntry, type Reason } from "../commands";
import { icon } from "../design/icons";
import { message, timedMessage } from "../dialogs";
import { registerProduct, t } from "../i18n";
import { mountTrainingBar, refreshTrainingBar } from "./bar";
import { dialogNotice, humanSlDialogOpen, openHumanSlDialog, startFailedBody } from "./dialog";
import { aiCoachBlockedByGame, humanSlGameActive, humanSlUi, onHumanSlUiChange, registerHumanSlContinuation } from "./state";

registerProduct({
  ZhCn: { humanSlModeDuringGame: "AI 陪练对局正在进行。这盘结束后，才能在设置中更改训练方式。" },
  ZhTw: { humanSlModeDuringGame: "AI 陪練對局正在進行。這盤結束後，才能在設定中更改訓練方式。" },
  EnUs: { humanSlModeDuringGame: "An AI Coach game is in progress. The training mode can be changed in the settings once this game ends." },
  JaJp: { humanSlModeDuringGame: "AI コーチの対局中です。この対局が終わると、設定でトレーニング方式を変更できます。" },
  KoKr: { humanSlModeDuringGame: "AI 코치 대국이 진행 중입니다. 이 대국이 끝나면 설정에서 훈련 방식을 바꿀 수 있습니다." },
  ThTh: { humanSlModeDuringGame: "กำลังเล่นเกมกับโค้ช AI อยู่ เปลี่ยนรูปแบบการฝึกในการตั้งค่าได้เมื่อจบเกมนี้" },
});

/** LizzieFrame.startHumanSlGameDialog(Reserved): a game on shows its bar; an 引擎对战 or 人机对弈
 *  game refuses with engineGameStopFirstHint; otherwise the setup dialog opens. */
function startDialog(): void {
  if (humanSlGameActive()) {
    humanSlUi.focusBar?.();
    return;
  }
  if (aiCoachBlockedByGame()) {
    void message(t("Message.title"), t("LizzieFrame.engineGameStopFirstHint"));
    return;
  }
  openHumanSlDialog();
}

/** Function search's 训练方式 (AI 陪练's setup dialog at its 训练方式 choice): not during a game of
 *  its own, nor while a game or an engine match keeps AI 陪练 from starting. */
function trainingModeUnavailable(): Reason | null {
  if (humanSlGameActive()) return { key: "humanSlModeDuringGame", product: true };
  return aiCoachBlockedByGame() ? { key: "LizzieFrame.engineGameStopFirstHint" } : null;
}

/** LizzieFrame.handleAiCoachToolbarAction: the bar of a game on, the open dialog, else a new one. */
function aiCoachAction(): void {
  if (humanSlGameActive()) humanSlUi.focusBar?.();
  else if (humanSlDialogOpen()) humanSlUi.focusDialog?.();
  else startDialog();
}

Object.assign(COMMANDS, {
  "menu.newHumanSlGame": { run: startDialog },
  "game.ai-coach": { run: aiCoachAction },
  "humansl.training-mode": { run: () => openHumanSlDialog(true), disabledReason: trainingModeUnavailable },
} satisfies Record<string, CommandEntry>);

// 再来一盘 after the session's exit: the setup again, which opens with the last game's choices.
registerHumanSlContinuation("HumanSlSetup", startDialog);

/** The toolbar's AI 陪练 button: 准备中… while the model downloads or the companion starts,
 *  陪练中 (selected) during the game and its exit, else AI 陪练. */
export function buildAiCoachButton(): HTMLElement {
  const pill = document.createElement("span");
  pill.className = "tb-pill coach";
  pill.dataset.key = "toolbar.aiCoach";
  pill.title = t("Menu.aiCoach.tip");
  pill.setAttribute("role", "button");
  pill.setAttribute("aria-description", t("Menu.aiCoach.tip"));
  const label = document.createElement("span");
  pill.append(icon("spark"), label);
  pill.addEventListener("mousedown", (e) => e.preventDefault());
  pill.onclick = aiCoachAction;
  const update = () => {
    const session = state.view?.human_sl;
    const playing = humanSlGameActive();
    // refreshAiFeatureLabels (Menu.java:11171-11181): a window narrower than 900 px shows the short
    // idle label; the session labels are kept.
    const idle = window.innerWidth < 900 ? t("Menu.aiCoach.short") : t("Menu.aiCoach");
    const text = playing ? t("Menu.aiCoach.playing") : humanSlUi.downloading || session ? t("Menu.aiCoach.preparing") : idle;
    if (label.textContent !== text) label.textContent = text;
    pill.classList.toggle("on", playing);
    pill.setAttribute("aria-pressed", String(playing));
  };
  subscribe(update);
  onHumanSlUiChange(update);
  window.addEventListener("resize", update);
  update();
  return pill;
}

mountTrainingBar();

registerViewHandler((view) => {
  for (const n of view.human_sl_notices) {
    if (dialogNotice(n)) continue;
    switch (n.type) {
      case "AiPassed":
        timedMessage(t("HumanSlGame.aiPassed"), 3);
        break;
      case "AiMoveFailed":
        timedMessage(t("HumanSlGame.aiMoveFailed"), 4);
        break;
      case "AiResigned":
        timedMessage(t("HumanSlGame.aiResigned"), 3);
        break;
      case "BothPassed":
        timedMessage(t("HumanSlGame.bothPassed"), 3);
        break;
      case "BackToLiveGame":
        timedMessage(t("HumanSlGame.backToLiveGame"), 3);
        break;
      case "AnalysisHidden":
        timedMessage(t("HumanSlGame.analysisHidden"), 3);
        break;
      case "GameMovesDeleted":
        timedMessage(t("HumanSlGame.gameMovesDeleted"), 4);
        break;
      case "StartFailed":
        void message(t("Message.title"), startFailedBody(n), "warning");
        break;
      case "ExitRecoveryFailed":
        void message(t("Message.title"), t("HumanSlGame.error.exitRecoveryFailed"), "error");
        break;
      case "Ended":
        break;
    }
  }
  // showHumanSlTrainingBar: the bar takes the focus when it appears.
  if (refreshTrainingBar() && humanSlGameActive(view)) humanSlUi.focusBar?.();
});
