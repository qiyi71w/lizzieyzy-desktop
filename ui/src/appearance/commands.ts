// 显示 > 外观主题, 设置 > 主题 and 设置 > 界面字体大小 (Menu.java:596-629, 738-796, 5402-5411,
// 5455-5503, 5601-5621). Each change is one complete SetAppearanceSettings or SetFrameFontSize; the
// view that follows repaints with the new appearance.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { chooseAppearanceImage } from "../contract/client";
import type { AppearanceSettings, BoardStyle } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import { actions, button } from "../engine/controls";
import { openAnalysisSettings } from "../engine/settings";
import { isChinese, t } from "../i18n";
import { syncAppearance } from "./state";
import "./page";
import "./appearance.css";

registerViewHandler((view) => void syncAppearance(view));

/** One change to the appearance settings, the theme values as they are. */
async function setAppearance(change: Partial<AppearanceSettings>): Promise<boolean> {
  const v = state.view;
  if (!v) return false;
  const result = await run({ type: "SetAppearanceSettings", settings: { ...v.settings.appearance, ...change }, theme: v.settings.theme });
  return !!result && result.type !== "Refused";
}

/** 棋盘风格 > 日式 / 中式经典 (Menu.java:604-621, Config.setBoardStyle 2717-2727). */
function boardStyle(style: BoardStyle): CommandEntry {
  return {
    run: () => {
      if (state.view?.settings.appearance.boardStyle !== style) void setAppearance({ boardStyle: style });
    },
    checked: () => state.view?.settings.appearance.boardStyle === style,
  };
}

/** Utils.showMsg / showMsgNoModal: the Message dialog (Message.title). */
const notice = (text: string) => message(t("Message.title"), text);

/** LizzieFrame.setFrameFontSize / SetFrameFontSize.applyChange: saved for the next start; outside
 *  Chinese a size over 12 first warns that text may not fit (LizzieFrame.java:20751-20768,
 *  SetFrameFontSize.java:109-115). */
async function setFrameSize(size: number, restartText: () => string): Promise<void> {
  const result = await run({ type: "SetFrameFontSize", size });
  if (!result || result.type === "Refused") return;
  if (!isChinese() && size > 12) await notice(t("menu.setFrameSizeAlart"));
  await notice(restartText());
}

const configuredFrameSize = () => state.view?.settings.appearance.frameFontSize ?? 12;

function frameSize(size: number): CommandEntry {
  return { run: () => void setFrameSize(size, () => t("menu.setFrameSizeRestart")), checked: () => configuredFrameSize() === size };
}

/** SetFrameFontSize dialog (SetFrameFontSize.java:31-146): digits only, 12-20 accepted; a value
 *  outside turns the field red with the LizzieChangeMove.txtMoveNumber.error tip. */
async function openFrameFontSize(): Promise<void> {
  const body = document.createElement("div");
  body.className = "appearance-frame-size";
  const label = document.createElement("label");
  label.textContent = t("SetFrameFontSize.lblSetSize");
  const field = document.createElement("input");
  field.type = "text";
  field.inputMode = "numeric";
  field.dataset.key = "SetFrameFontSize.size";
  field.value = String(configuredFrameSize());
  field.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const digits = inserted.replace(/\D+/g, "");
    if (digits === inserted) return;
    e.preventDefault();
    if (digits) field.setRangeText(digits, field.selectionStart ?? field.value.length, field.selectionEnd ?? field.value.length, "end");
  });
  label.append(field);
  body.append(label);
  const dialog = formPanel(t("SetFrameFontSize.title"), body);
  const ok = button(t("SetFrameFontSize.okButton"), async () => {
    // txtFieldValue: blank or ten or more characters read as 0.
    const text = field.value.trim();
    const size = text === "" || text.length >= 10 ? 0 : Number.parseInt(text, 10);
    if (size < 12 || size > 20) {
      field.classList.add("invalid");
      field.title = t("LizzieChangeMove.txtMoveNumber.error");
      return;
    }
    await dialog.close();
    await setFrameSize(size, () => t("SetFrameFontSize.successHint"));
  });
  ok.className = "primary";
  ok.dataset.key = "SetFrameFontSize.okButton";
  body.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      ok.click();
    }
  });
  actions(body, ok);
  queueMicrotask(() => field.select());
  await dialog.closed;
}

Object.assign(COMMANDS, {
  // 设置 > 主题 opens 综合设置 at 主题外观 (Menu.java:5406-5411, openConfigDialog2(1)).
  "menu.theme": { run: () => void openAnalysisSettings("theme") },
  "Menu.boardStyleJapanese": boardStyle("Japanese"),
  "Menu.boardStyleChineseClassic": boardStyle("ChineseClassic"),
  // Apple 风格界面 (Menu.java:743-757): turning it on also turns the Morandi colours on.
  "menu.toggleAppleStyle": {
    run: async () => {
      const v = state.view?.settings.appearance;
      if (!v) return;
      const apple = !v.appleStyle;
      // Utils.showMsgNoModal(Menu.restartRequiredForIcons) (Menu.java:756).
      if (await setAppearance({ appleStyle: apple, morandiColors: apple || v.morandiColors })) void notice(t("Menu.restartRequiredForIcons"));
    },
    checked: () => state.view?.settings.appearance.appleStyle ?? false,
  },
  // 经典配色 (Menu.java:760-780): checked while the Morandi colours are off; choosing it turns
  // Apple style off.
  "menu.toggleClassicColors": {
    run: async () => {
      const v = state.view?.settings.appearance;
      if (!v) return;
      const classic = v.morandiColors;
      if (await setAppearance({ morandiColors: !classic, appleStyle: classic ? false : v.appleStyle })) void notice(t("Menu.restartRequiredForIcons"));
    },
    checked: () => !(state.view?.settings.appearance.morandiColors ?? false),
  },
  // 设置自定义棋盘背景 / 清除自定义棋盘背景 (Menu.java:785-794, 7049-7073).
  "menu.setCustomBoard": {
    run: async () => {
      const path = await chooseAppearanceImage();
      if (path?.trim()) await setAppearance({ customBoardImage: path.trim() });
    },
  },
  "menu.clearCustomBoard": { run: () => void setAppearance({ customBoardImage: "" }) },
  "menu.frameFontSizeSmall": frameSize(12),
  "menu.frameFontSizeMiddle": frameSize(16),
  "menu.frameFontSizeBig": frameSize(20),
  "menu.frameFontSizeOther": {
    run: () => void openFrameFontSize(),
    checked: () => ![12, 16, 20].includes(configuredFrameSize()),
  },
} satisfies Record<string, CommandEntry>);
