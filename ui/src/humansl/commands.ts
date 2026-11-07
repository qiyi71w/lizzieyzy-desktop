// AI 陪练 entries of the command catalog (Menu.java 对局 > 新建对局 > AI 陪练 2992-3003, the menu
// strip's aiCoachButton 5699-5732 and FunctionCatalog game.ai-coach), the strip button's state
// (Menu.updateAiCoachState 11186-11231) and the session's one-shot messages. Rust owns the session;
// this module sends HumanSl commands and shows its view.
import { registerViewHandler, state, subscribe } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { message } from "../dialogs";
import { t } from "../i18n";
import { mountTrainingBar, refreshTrainingBar, timedMessage } from "./bar";
import { dialogNotice, humanSlDialogOpen, openHumanSlDialog, startFailedText } from "./dialog";
import { aiCoachBlockedByGame, humanSlGameActive, humanSlUi, onHumanSlUiChange } from "./state";

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

/** LizzieFrame.handleAiCoachToolbarAction: the bar of a game on, the open dialog, else a new one. */
function aiCoachAction(): void {
  if (humanSlGameActive()) humanSlUi.focusBar?.();
  else if (humanSlDialogOpen()) humanSlUi.focusDialog?.();
  else startDialog();
}

Object.assign(COMMANDS, {
  "menu.newHumanSlGame": { run: startDialog },
  "game.ai-coach": { run: aiCoachAction },
} satisfies Record<string, CommandEntry>);

/** The menu strip's AI 陪练 button: 准备中… while the model downloads or the companion starts,
 *  陪练中 (selected) during the game and its exit, else AI 陪练. */
export function buildAiCoachButton(): HTMLElement {
  const pill = document.createElement("span");
  pill.className = "pill coach";
  pill.dataset.key = "toolbar.aiCoach";
  pill.title = t("Menu.aiCoach.tip");
  pill.setAttribute("role", "button");
  pill.setAttribute("aria-description", t("Menu.aiCoach.tip"));
  const icon = document.createElement("img");
  icon.className = "coach-icon";
  icon.src = "/assets/ui/ai-coach/coach-target.png";
  icon.alt = "";
  const label = document.createElement("span");
  pill.append(icon, label);
  pill.addEventListener("mousedown", (e) => e.preventDefault());
  pill.onclick = aiCoachAction;
  const strip = document.getElementById("menubar");
  const update = () => {
    const session = state.view?.human_sl;
    const playing = humanSlGameActive();
    // refreshAiFeatureLabels (Menu.java:11171-11181): a strip narrower than 900 px shows the short
    // idle label; the session labels are kept.
    const width = strip?.clientWidth ?? 0;
    const idle = width > 0 && width < 900 ? t("Menu.aiCoach.short") : t("Menu.aiCoach");
    const text = playing ? t("Menu.aiCoach.playing") : humanSlUi.downloading || session ? t("Menu.aiCoach.preparing") : idle;
    if (label.textContent !== text) label.textContent = text;
    pill.classList.toggle("selected", playing);
    pill.setAttribute("aria-pressed", String(playing));
  };
  subscribe(update);
  onHumanSlUiChange(update);
  // The strip's componentResized listener re-evaluates the label.
  if (strip) new ResizeObserver(update).observe(strip);
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
      case "StartFailed":
        void message(t("Message.title"), startFailedText(n), "warning");
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
