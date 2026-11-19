// 综合设置 > 关于 (ConfigDialog2.createUnifiedAboutHero, ConfigDialog2.java:2639-2688): the product
// name, its version, where it comes from and the 项目主页 / 发布下载 / 问题反馈 buttons, opened from
// 帮助 > 关于 (Menu.java:5263-5271, openConfigDialog2(2)), then the 检查更新 rows (update.ts) that
// 帮助 > 检查更新 opens. Java's feature cards and QQ group are not shown, and the buttons open this
// product's repository.
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { COMMANDS } from "./commands";
import { message } from "./dialogs";
import { openAnalysisSettings, SETTINGS_PAGES } from "./engine/settings";
import { button } from "./engine/controls";
import { product, registerProduct, t } from "./i18n";
import { buildUpdateSection } from "./update";
import "./about.css";

registerProduct({
  ZhCn: { aboutVersion: "版本 {0}", aboutOrigin: "由 LizzieYzy Next（Java）迁移而来。" },
  ZhTw: { aboutVersion: "版本 {0}", aboutOrigin: "由 LizzieYzy Next（Java）遷移而來。" },
  EnUs: { aboutVersion: "Version {0}", aboutOrigin: "Migrated from LizzieYzy Next (Java)." },
  JaJp: { aboutVersion: "バージョン {0}", aboutOrigin: "LizzieYzy Next（Java）から移行しました。" },
  KoKr: { aboutVersion: "버전 {0}", aboutOrigin: "LizzieYzy Next(Java)에서 이전했습니다." },
  ThTh: { aboutVersion: "เวอร์ชัน {0}", aboutOrigin: "ย้ายมาจาก LizzieYzy Next (Java)" },
});

SETTINGS_PAGES.about = (host) => {
  const hero = document.createElement("div"); hero.className = "about-hero";
  const title = document.createElement("h2"); title.textContent = "LizzieYzy Tauri";
  const version = document.createElement("p"); version.className = "about-version"; version.dataset.key = "about.version";
  void getVersion().then((v) => { version.textContent = product("aboutVersion", v); });
  const origin = document.createElement("p"); origin.className = "about-origin"; origin.textContent = product("aboutOrigin");
  const links = document.createElement("div"); links.className = "about-links";
  for (const [page, label] of [["home", t("ConfigDialog2.modern.about.homepage")], ["releases", t("ConfigDialog2.modern.about.releases")], ["issues", t("ConfigDialog2.modern.about.issues")]] as const) {
    const link = button(label, async () => {
      // The command's only failure: no system browser could be started.
      try { await invoke("open_project_page", { page }); } catch { await message(t("Message.title"), t("AutoSetup.openBrowserUnsupported"), "error"); }
    });
    link.dataset.key = `about.${page}`;
    links.append(link);
  }
  hero.append(title, version, origin, links);
  host.append(hero);
  buildUpdateSection(host);
  return { save: async () => true };
};

Object.assign(COMMANDS, {
  // 帮助 > 关于: 综合设置 at its 关于 page.
  "menu.about": { run: () => void openAnalysisSettings("about") },
});
