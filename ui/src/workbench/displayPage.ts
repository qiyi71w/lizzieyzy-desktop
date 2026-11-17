// 综合设置 > 棋盘与显示 > 启动时加载 (ConfigDialog2.createDisplaySection, ConfigDialog2.java:3313-
// 3356): the rows in Java's order, edited as a draft and committed together with 保存设置
// (ConfigDialog2.java:6667-6668, 6734-6750); 取消 discards them. 显示快速启动 is the 快速启动
// setting, saved by this page; the other rows are panel settings.
import { run, state } from "../app";
import { checkbox } from "../engine/controls";
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
const QUICK_LINKS = "config.display.quick-links";

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
  for (const [field, title, sub] of rest) draft.row(host, field, TARGETS[field], title, sub);
  // The dialog commits the shared panel draft once with 保存设置 (panelDraft.ts).
  return {
    save: async () => {
      const now = state.view?.settings.quick_links;
      if (!now || now.show === quickLinks.input.checked) return true;
      const result = await run({ type: "SetQuickLinks", settings: { ...now, show: quickLinks.input.checked } });
      return !!result && result.type !== "Refused";
    },
  };
};
registerSettingTargets("display", [...Object.values(TARGETS), QUICK_LINKS]);
