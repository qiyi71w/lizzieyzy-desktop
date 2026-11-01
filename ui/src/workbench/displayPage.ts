// 综合设置 > 棋盘与显示 > 启动时加载 (ConfigDialog2.createDisplaySection, ConfigDialog2.java:3313-
// 3356): the rows in Java's order, edited as a draft and committed together with 保存设置
// (ConfigDialog2.java:6667-6668, 6734-6750); 取消 discards them. Java's 显示快速启动 row is not
// shown: this product has no 快速启动 links for it to control.
import { SETTINGS_PAGES } from "../engine/settings";
import { registerSettingTargets } from "../engine/settingsTargets";
import { t } from "../i18n";
import { panelDraft } from "./panelDraft";

type Field = "alwaysOnTop" | "showStatus" | "showSubBoard" | "showWinrateInTitle";

/** The setting targets of the rows shown (ConfigSettingTargets.java:56-60), in Java's order. */
const TARGETS: Record<Field, string> = {
  alwaysOnTop: "config.display.always-on-top",
  showStatus: "config.display.status",
  showSubBoard: "config.display.sub-board",
  showWinrateInTitle: "config.display.title-winrate",
};

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
  for (const [field, title, sub] of rows) draft.row(host, field, TARGETS[field], title, sub);
  // The dialog commits the shared panel draft once with 保存设置 (panelDraft.ts).
  return { save: async () => true };
};
registerSettingTargets("display", Object.values(TARGETS));
