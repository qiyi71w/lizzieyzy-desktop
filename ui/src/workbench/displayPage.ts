// 综合设置 > 棋盘与显示 > 启动时加载 (ConfigDialog2.createDisplaySection, ConfigDialog2.java:3313-
// 3356): the rows in Java's order, edited as a draft and committed together with 保存设置
// (ConfigDialog2.java:6667-6668, 6734-6750); 取消 discards them. 显示快速启动 is the 快速启动
// setting, saved by this page; the other rows are panel settings. This product adds 语言 last,
// the setting of the 语言 menu and 初始化设置, which applies from the next start.
import { run, state } from "../app";
import type { Language } from "../contract/generated";
import { checkbox, field } from "../engine/controls";
import { SETTINGS_PAGES } from "../engine/settings";
import { registerSettingTargets } from "../engine/settingsTargets";
import { product, registerProduct, t } from "../i18n";
import { LANGUAGES } from "../initSettings";
import { panelDraft } from "./panelDraft";

registerProduct({
  ZhCn: { settingsLanguageSub: "重新启动后生效" },
  ZhTw: { settingsLanguageSub: "重新開啟後生效" },
  EnUs: { settingsLanguageSub: "Takes effect after a restart" },
  JaJp: { settingsLanguageSub: "再起動後に反映されます" },
  KoKr: { settingsLanguageSub: "다시 시작하면 적용됩니다" },
  ThTh: { settingsLanguageSub: "มีผลหลังเปิดโปรแกรมใหม่" },
});

type Field = "alwaysOnTop" | "showStatus" | "showSubBoard" | "showWinrateInTitle";

/** The setting targets of the rows shown (ConfigSettingTargets.java:56-60), in Java's order. */
const TARGETS: Record<Field, string> = {
  alwaysOnTop: "config.display.always-on-top",
  showStatus: "config.display.status",
  showSubBoard: "config.display.sub-board",
  showWinrateInTitle: "config.display.title-winrate",
};
const QUICK_LINKS = "config.display.quick-links";
const LANGUAGE = "config.display.language";

SETTINGS_PAGES.display = (host) => {
  const heading = document.createElement("h3");
  heading.textContent = t("ConfigDialog2.modern.display.title");
  const subtitle = document.createElement("p");
  subtitle.textContent = t("ConfigDialog2.modern.display.subtitle");
  host.append(heading, subtitle);
  const rows: [Field, string, string][] = [
    ["alwaysOnTop", t("ConfigDialog2.modern.display.alwaysOnTop"), t("ConfigDialog2.modern.display.alwaysOnTopSub")],
    ["showStatus", t("ConfigDialog2.modern.display.status"), t("ConfigDialog2.modern.display.statusSub")],
    ["showSubBoard", t("ConfigDialog2.modern.display.subBoard"), t("ConfigDialog2.modern.display.subBoardSub")],
    ["showWinrateInTitle", t("ConfigDialog2.modern.display.titleWr"), t("ConfigDialog2.modern.display.titleWrSub")],
  ];
  const draft = panelDraft();
  const [first, ...rest] = rows;
  draft.row(host, first[0], TARGETS[first[0]], first[1], first[2]);
  // 显示快速启动 (chkShowQuickLinks), the second row; 保存设置 writes ui.show-quick-links.
  const row = document.createElement("div");
  row.className = "settings-row";
  row.dataset.settingTarget = QUICK_LINKS;
  const quickLinks = checkbox(t("ConfigDialog2.modern.display.quickLinks"), state.view?.settings.quick_links.show ?? false);
  quickLinks.input.dataset.key = QUICK_LINKS;
  quickLinks.label.title = t("ConfigDialog2.modern.display.quickLinksSub");
  const hint = document.createElement("p");
  hint.textContent = t("ConfigDialog2.modern.display.quickLinksSub");
  row.append(quickLinks.label, hint);
  host.append(row);
  for (const [key, title, sub] of rest) draft.row(host, key, TARGETS[key], title, sub);
  const language = document.createElement("select");
  language.dataset.key = "settings.language";
  language.title = product("settingsLanguageSub");
  for (const [value, text] of LANGUAGES) language.append(new Option(text(), value));
  language.value = state.view?.settings.language ?? "System";
  field(host, t("FirstUseSettings.lblLanguage"), language);
  (host.lastElementChild as HTMLElement).dataset.settingTarget = LANGUAGE;
  const languageHint = document.createElement("p");
  languageHint.textContent = product("settingsLanguageSub");
  host.append(languageHint);
  // The dialog commits the shared panel draft once with 保存设置 (panelDraft.ts).
  return {
    save: async () => {
      const now = state.view?.settings;
      if (!now) return true;
      if (now.quick_links.show !== quickLinks.input.checked) {
        const result = await run({ type: "SetQuickLinks", settings: { ...now.quick_links, show: quickLinks.input.checked } });
        if (!result || result.type === "Refused") return false;
      }
      if (language.value !== now.language) {
        const result = await run({ type: "SetLanguage", language: language.value as Language });
        if (!result || result.type === "Refused") return false;
      }
      return true;
    },
  };
};
registerSettingTargets("display", [...Object.values(TARGETS), QUICK_LINKS, LANGUAGE]);
