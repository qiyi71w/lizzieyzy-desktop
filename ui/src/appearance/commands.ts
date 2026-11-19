// 显示 > 外观主题, 设置 > 主题 and 设置 > 界面字体大小 (Menu.java:596-629, 738-796, 5402-5411,
// 5455-5503, 5601-5621) and this product's 外观模式. Each change is one complete
// SetAppearanceSettings or SetFrameFontSize; the view that follows repaints with the new appearance.
// 界面字体大小 > 其他 opens the size's 综合设置 row instead of Java's SetFrameFontSize dialog.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import { chooseAppearanceImage } from "../contract/client";
import type { AppearanceMode, AppearanceSettings, BoardStyle } from "../contract/generated";
import { message } from "../dialogs";
import { openAnalysisSettings } from "../engine/settings";
import { openSettingsAt } from "../engine/settingsTargets";
import { isChinese, t } from "../i18n";
import { syncAppearance } from "./state";
import "./page";
import "./appearance.css";

registerViewHandler((view) => void syncAppearance(view.settings.appearance, view.settings.theme, view.settings.theme_files_revision));

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

/** 外观模式 > 跟随系统 / 浅色 / 深色: applies at once. */
function appearanceMode(mode: AppearanceMode): CommandEntry {
  return {
    run: () => {
      if (state.view?.settings.appearance.mode !== mode) void setAppearance({ mode });
    },
    checked: () => state.view?.settings.appearance.mode === mode,
  };
}

/** Utils.showMsg / showMsgNoModal: the Message dialog (Message.title). */
const notice = (text: string) => message(t("Message.title"), text);

/** LizzieFrame.setFrameFontSize: saved for the next start; outside Chinese a size over 12 first
 *  warns that text may not fit (LizzieFrame.java:20751-20768). */
async function setFrameSize(size: number): Promise<void> {
  const result = await run({ type: "SetFrameFontSize", size });
  if (!result || result.type === "Refused") return;
  if (!isChinese() && size > 12) await notice(t("menu.setFrameSizeAlart"));
  await notice(t("menu.setFrameSizeRestart"));
}

const configuredFrameSize = () => state.view?.settings.appearance.frameFontSize ?? 12;

function frameSize(size: number): CommandEntry {
  return { run: () => void setFrameSize(size), checked: () => configuredFrameSize() === size };
}

Object.assign(COMMANDS, {
  // 设置 > 主题 opens 综合设置 at 主题外观 (Menu.java:5406-5411, openConfigDialog2(1)).
  "menu.theme": { run: () => void openAnalysisSettings("theme") },
  "Menu.boardStyleJapanese": boardStyle("Japanese"),
  "Menu.boardStyleChineseClassic": boardStyle("ChineseClassic"),
  "menu.appearanceModeSystem": appearanceMode("System"),
  "menu.appearanceModeLight": appearanceMode("Light"),
  "menu.appearanceModeDark": appearanceMode("Dark"),
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
    run: () => void openSettingsAt("menu.frameFontSize"),
    checked: () => ![12, 16, 20].includes(configuredFrameSize()),
  },
} satisfies Record<string, CommandEntry>);
