// 设置 > 初始化设置 (Menu.java:5369-5378, FirstUseSettings(false), FirstUseSettings.java:33-766): a
// modal, fixed-size form over the usual display and analysis options, filled from the current
// settings. 加载默认设置 changes only the form; 确定 saves everything and closes; Escape or the
// close box saves nothing. Java's 界面样式 row (Swing look and feel) is not part of this product, so
// the rows below it move up by one row and no restart hint follows. The language is saved like the
// 语言 menu and, as in Java, applies from the next start.
import { run, state } from "./app";
import { frameMetrics } from "./appearance/state";
import { COMMANDS } from "./commands";
import type { AnalysisSettings, DisplaySettings, Language } from "./contract/generated";
import { formPanel, message } from "./dialogs";
import { absolute, button, labelEl, openInfoOrderDialog, parseJavaInt, place } from "./display/dialogs";
import { isChinese, product, t } from "./i18n";

/** Java languageOptions with its combo texts. */
const LANGUAGES: [Language, () => string][] = [
  ["System", () => t("menu.language.default")],
  ["ZhCn", () => "简体中文"],
  ["ZhTw", () => "繁體中文"],
  ["EnUs", () => "English"],
  ["JaJp", () => "日本語"],
  ["KoKr", () => "한국어"],
  ["ThTh", () => "ไทย"],
];

/** Java's own dialog bottom (the 界面样式 row at 366 removed). */
const ROW_SHIFT = 40;
/** maxAnalyzeTimeMillis <= 0 is saved as 9999 minutes. */
const UNLIMITED_SECONDS = 9999 * 60;

/** Long.parseLong; the product keeps no negative limits (Java treats them as no limit or as an
 *  immediate stop, like 0). */
function parseJavaLong(text: string): number | null {
  if (!/^[+-]?\d+$/.test(text)) return null;
  const n = BigInt(text);
  if (n < -(2n ** 63n) || n > 2n ** 63n - 1n) return null;
  return n < 0n ? 0 : Number(n);
}

function openInitSettings(): void {
  const v = state.view;
  if (!v) return;
  const display = v.settings.display;
  const analysis = v.settings.engine.analysis;
  const size = frameMetrics().frameFontSize;
  const small = size === 12;
  const middle = size > 12 && size <= 16;
  const sel = (s: number, m: number, b: number) => (small ? s : middle ? m : b);
  const chinese = isChinese();
  // Lizzie.setFrameSize(656 / 785 / 955, 524): a (w + 10) x 534 window, all content at scale 1;
  // here less the 界面样式 row.
  const width = sel(666, 795, 965);
  const panel = absolute(width, 524 - ROW_SHIFT);
  const col1 = sel(240, 310, 380);
  const col2 = sel(350, 420, 520);

  const label = (text: string, x: number, y: number, w: number, h = 22) => place(panel, labelEl(text), x, y, w, h);
  const choice = (type: "radio" | "checkbox", text: string, key: string, x: number, y: number, w: number, h: number, group = "") => {
    const wrap = place(panel, document.createElement("label"), x, y, w, h);
    wrap.className = "display-choice";
    const input = document.createElement("input");
    input.type = type;
    if (group) input.name = group;
    input.dataset.key = key;
    wrap.append(input, text);
    return input;
  };
  const pair = (group: string, y: number, first: [string, string, number], second: [string, string, number], h = 23) => [
    choice("radio", first[0], `init.${first[1]}`, col1, y, first[2], h, group),
    choice("radio", second[0], `init.${second[1]}`, col2, y, second[2], h, group),
  ];
  const field = (key: string, x: number, y: number) => {
    const input = place(panel, document.createElement("input"), x, y, 66, 24);
    input.type = "text";
    input.dataset.key = key;
    return input;
  };
  const help = (key: string, x: number, y: number, title: string, text: string) => {
    const b = place(panel, document.createElement("button"), x, y, 24, 23);
    b.type = "button";
    b.className = "init-help";
    b.dataset.key = key;
    b.title = title;
    const img = document.createElement("img");
    img.src = "/assets/settings.png";
    img.alt = "";
    b.append(img);
    b.onclick = () => void message(title, text);
  };

  label(t("FirstUseSettings.lblMouseOverSuggestion"), 10, 6, 334);
  const [suggestionRefresh, suggestionFrozen] = pair("initSuggestion", 6,
    [t("FirstUseSettings.rdoMouseOverSuggestionRefresh"), "suggestionRefresh", 108],
    [t("FirstUseSettings.rdoMouseOverSuggestionNoRefresh"), "suggestionNoRefresh", 457]);
  label(t("FirstUseSettings.lblMouseOverSubboard"), 10, 46, 344);
  const [subRefresh, subFrozen] = pair("initSub", 48,
    [t("FirstUseSettings.rdoMouseOverSubboardRefresh"), "subRefresh", 107],
    [t("FirstUseSettings.rdoMouseOverSubboardNoRefresh"), "subNoRefresh", 457]);
  label(t("FirstUseSettings.lblShowScore"), 10, 86, 337);
  const [withKomi, justScore] = pair("initScore", 88,
    [t("FirstUseSettings.rdoScoreOnBoardWithKomi"), "scoreWithKomi", sel(114, 114, 140)],
    [t("FirstUseSettings.rdoScoreOnBoardJustScore"), "scoreJustScore", 422]);
  label(t("FirstUseSettings.lblWinratePerspective"), 10, 126, 337);
  const [alwaysBlack, alternately] = pair("initPerspective", 128,
    [t("FirstUseSettings.rdoAlwaysBlack"), "alwaysBlack", sel(114, 114, 140)],
    [t("FirstUseSettings.rdoAlternately"), "alternately", 422]);
  label(t("FirstUseSettings.lblLizzieCache"), 10, 166, 285);
  const [cacheOn, cacheOff] = pair("initCache", 168,
    [t("FirstUseSettings.rdoLizzieCacheEnable"), "cacheEnable", 107],
    [t("FirstUseSettings.rdoLizzieCacheDisable"), "cacheDisable", 121]);
  help("init.cacheHelp", sel(120, 155, 187), 166, t("FirstUseSettings.lizzieCacheDiscribeTitle"), t("FirstUseSettings.lizzieCacheDiscribe"));

  label(t("FirstUseSettings.lblMaxAnalyzeTime"), 10, 286, 236);
  help("init.maxTimeHelp", sel(114, 148, 179), 286, t("FirstUseSettings.maxAnalyzeTimeDiscribeTitile"), t("FirstUseSettings.maxAnalyzeTimeDiscribe"));
  const limitTime = choice("checkbox", t("FirstUseSettings.chkLimitTime"), "init.limitTime", sel(160, 217, 277), 287, sel(70, 83, 99), 20);
  const maxTime = field("init.maxTime", col1, 286);
  const limitPlayout = choice("checkbox", t("FirstUseSettings.chkLimitVisits"), "init.limitPlayout", sel(335, 405, 505), 287,
    chinese ? sel(70, 80, 92) : sel(80, 92, 103), 20);
  const maxPlayouts = field("init.maxPlayouts", chinese ? sel(407, 491, 598) : sel(420, 507, 617), 286);
  limitTime.addEventListener("change", () => (maxTime.disabled = !limitTime.checked));
  limitPlayout.addEventListener("change", () => (maxPlayouts.disabled = !limitPlayout.checked));

  const noteWidth = Math.min(867, width - 10);
  label(t("FirstUseSettings.lblNotice"), 10, 406 - ROW_SHIFT, noteWidth);
  label(t("FirstUseSettings.lblHint"), 10, 431 - ROW_SHIFT, noteWidth).classList.add("init-hint");

  label(t("FirstUseSettings.lblSuggestionInfo"), 10, 206, 285);
  const showWinrate = choice("checkbox", t("FirstUseSettings.chkWinrate"), "init.showWinrate", col1, 208, 95, 23);
  const showPlayouts = choice("checkbox", t("FirstUseSettings.chkVisits"), "init.showPlayouts", col2, 208, 95, 23);
  const showScore = choice("checkbox", t("FirstUseSettings.chkScoreLead"), "init.showScore", sel(460, 530, 660), 208, 125, 23);
  const order = place(panel, button(t("FirstUseSettings.btnCustomOrder")), chinese ? sel(73, 85, 105) : sel(99, 130, 155), 205,
    chinese ? sel(101, 130, 160) : sel(131, 160, 200), 24);
  order.type = "button";
  order.className = "init-order";
  order.dataset.key = "init.customOrder";
  // SuggestionInfoOrderSettings saves its own boxes and then refreshes these (setChkSuggestionInfo).
  order.onclick = () => openInfoOrderDialog((shows) => {
    showWinrate.checked = shows.suggestionWinrate;
    showPlayouts.checked = shows.suggestionPlayouts;
    showScore.checked = shows.suggestionScore;
  });

  label(t("FirstUseSettings.lblLimitSuggestion"), 10, 246, 337);
  const limitSuggestion = field("init.limitSuggestion", col1, 246);
  label(t("FirstUseSettings.lblLimitVariation"), sel(355, 425, 525), 246, 236);
  const limitVariation = field("init.limitVariation", sel(525, 645, 785), 246);

  label(t("FirstUseSettings.lblLanguage"), 10, 326, 249);
  const languageChoice = place(panel, document.createElement("select"), col1, 326, small ? 300 : 360, 27);
  languageChoice.dataset.key = "init.language";
  languageChoice.setAttribute("aria-label", t("FirstUseSettings.lblLanguage"));
  for (const [value, text] of LANGUAGES) languageChoice.append(new Option(text(), value));
  languageChoice.value = v.settings.language;

  // The current settings (FirstUseSettings.java:722-763).
  (display.noRefreshOnMouse ? suggestionFrozen : suggestionRefresh).checked = true;
  (display.noRefreshOnSub ? subFrozen : subRefresh).checked = true;
  (display.scoreLeadWithKomi ? withKomi : justScore).checked = true;
  (display.winrateAlwaysBlack ? alwaysBlack : alternately).checked = true;
  (analysis.enableLizzieCache ? cacheOn : cacheOff).checked = true;
  showWinrate.checked = display.suggestionWinrate;
  showPlayouts.checked = display.suggestionPlayouts;
  showScore.checked = display.suggestionScore;
  limitVariation.value = String(analysis.limitBranchLength);
  limitSuggestion.value = String(analysis.limitMaxSuggestion);
  const shownTime = String(analysis.maxAnalyzeTimeSeconds);
  maxTime.value = shownTime;
  maxPlayouts.value = String(analysis.limitPlayouts);
  limitTime.checked = analysis.limitTime;
  limitPlayout.checked = analysis.limitPlayout;
  maxTime.disabled = !analysis.limitTime;
  maxPlayouts.disabled = !analysis.limitPlayout;

  const defaults = place(panel, button(t("FirstUseSettings.defaultSettings")), sel(209, 235, 260), 461 - ROW_SHIFT, sel(116, 130, 165), 25);
  defaults.type = "button";
  defaults.dataset.key = "init.defaults";
  defaults.onclick = () => {
    showPlayouts.checked = showScore.checked = showWinrate.checked = true;
    cacheOn.checked = subFrozen.checked = suggestionFrozen.checked = justScore.checked = alternately.checked = true;
    limitSuggestion.value = "10";
    limitVariation.value = "0";
    limitTime.checked = true;
    maxTime.disabled = false;
    maxTime.value = "600";
    limitPlayout.checked = false;
    maxPlayouts.disabled = true;
    maxPlayouts.value = "";
  };
  const apply = place(panel, button(t("FirstUseSettings.btnApply")), sel(332, 385, 440), 461 - ROW_SHIFT, sel(116, 130, 165), 25);
  apply.type = "button";
  apply.dataset.key = "init.apply";

  const dialog = formPanel(t("FirstUseSettings.title"), panel);
  const box = panel.parentElement!;
  box.classList.add("init-settings");
  const head = box.querySelector(".modal-title")!;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "init-close";
  close.textContent = "×";
  close.tabIndex = -1;
  close.dataset.key = "init.close";
  close.setAttribute("aria-label", product("close"));
  close.onclick = () => void dialog.close();
  head.append(close);

  apply.onclick = async () => {
    const now = state.view;
    if (!now) return;
    // A limit's field must read when its box is checked; otherwise a bad value keeps the setting.
    const variation = parseJavaInt(limitVariation.value);
    const suggestions = parseJavaInt(limitSuggestion.value);
    const time = parseJavaInt(maxTime.value);
    const playouts = parseJavaLong(maxPlayouts.value);
    if (variation === null || suggestions === null || (limitTime.checked && time === null) || (limitPlayout.checked && playouts === null)) {
      await message(t("Message.title"), t("FirstUseSettings.confirmHint2"));
      return;
    }
    // The time limit as saved stays when its field was not edited: a saved 0 (kept by the toolbar
    // and 综合设置) is not an analysis change. A typed limit of 0 or less means 9999 minutes.
    const savedSeconds = now.settings.engine.analysis.maxAnalyzeTimeSeconds;
    const seconds = time === null || maxTime.value === shownTime ? savedSeconds : time <= 0 ? UNLIMITED_SECONDS : time;
    const nextDisplay: DisplaySettings = {
      ...now.settings.display,
      suggestionWinrate: showWinrate.checked,
      suggestionPlayouts: showPlayouts.checked,
      suggestionScore: showScore.checked,
      noRefreshOnMouse: suggestionFrozen.checked,
      noRefreshOnSub: subFrozen.checked,
      scoreLeadWithKomi: withKomi.checked,
      winrateAlwaysBlack: alwaysBlack.checked,
    };
    const nextAnalysis: AnalysisSettings = {
      ...now.settings.engine.analysis,
      limitTime: limitTime.checked,
      limitPlayout: limitPlayout.checked,
      maxAnalyzeTimeSeconds: seconds,
      limitPlayouts: playouts ?? now.settings.engine.analysis.limitPlayouts,
      enableLizzieCache: cacheOn.checked,
      limitMaxSuggestion: Math.max(0, suggestions),
      limitBranchLength: Math.max(0, variation),
    };
    // FirstUseSettings saves everything at once: one command, admitted or refused as a whole. A
    // refusal (shown by `run`) or a failed save leaves the dialog open with this draft.
    const result = await run({ type: "SaveInitSettings", display: nextDisplay, analysis: nextAnalysis, language: languageChoice.value as Language });
    if (result?.type === "Done" || result?.type === "Unchanged") await dialog.close();
  };
}

Object.assign(COMMANDS, {
  "menu.initSettings": { run: openInitSettings },
});
