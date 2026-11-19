// 设置 > 初始化设置 (Menu.java:5369-5378, FirstUseSettings(false), FirstUseSettings.java:33-766): a
// modal form over the usual display and analysis options, filled from the current settings.
// 加载默认设置 changes only the form; 确定 saves everything and closes; Escape or the close box
// saves nothing. Java's 界面样式 row (Swing look and feel) is not part of this product and no
// restart hint follows. The language is saved like the 语言 menu and, as in Java, applies from the
// next start.
import { run, state } from "./app";
import { COMMANDS } from "./commands";
import type { AnalysisSettings, DisplaySettings, Language } from "./contract/generated";
import { icon } from "./design/icons";
import { formPanel, message } from "./dialogs";
import { openInfoOrderDialog, parseJavaInt } from "./display/dialogs";
import { product, t } from "./i18n";

/** Java languageOptions with its combo texts (also 综合设置's 语言 row). */
export const LANGUAGES: [Language, () => string][] = [
  ["System", () => t("menu.language.default")],
  ["ZhCn", () => "简体中文"],
  ["ZhTw", () => "繁體中文"],
  ["EnUs", () => "English"],
  ["JaJp", () => "日本語"],
  ["KoKr", () => "한국어"],
  ["ThTh", () => "ไทย"],
];

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
  // One grid: the row names, then up to three aligned columns of choices (a, b, c); a row's last
  // control may run on to the row's end.
  const form = document.createElement("div");
  form.className = "init-form";
  type Column = "a" | "b" | "c" | "end" | "b-end" | "a-end";
  const place = <T extends HTMLElement>(el: T, column: Column): T => {
    el.classList.add(`init-${column}`);
    form.append(el);
    return el;
  };
  const help = (key: string, title: string, text: string) => {
    const b = document.createElement("button");
    b.type = "button";
    b.className = "init-help";
    b.dataset.key = key;
    b.title = title;
    b.setAttribute("aria-label", title);
    b.append(icon("help"));
    b.onclick = () => void message(title, text);
    return b;
  };
  const name = (text: string, helpButton?: HTMLButtonElement) => {
    const cell = document.createElement("div");
    cell.className = "init-name";
    cell.append(text);
    if (helpButton) cell.append(helpButton);
    form.append(cell);
  };
  /** A radio or check box with its text, placed in `column` unless it goes into a unit. */
  const choice = (type: "radio" | "checkbox", text: string, key: string, column?: Column, group = "") => {
    const wrap = document.createElement("label");
    const input = document.createElement("input");
    input.type = type;
    if (group) input.name = group;
    input.dataset.key = key;
    wrap.append(input, text);
    if (column) place(wrap, column);
    return { wrap, input };
  };
  const pair = (group: string, first: [string, string], second: [string, string]) => [
    choice("radio", first[0], `init.${first[1]}`, "a", group).input,
    choice("radio", second[0], `init.${second[1]}`, "b-end", group).input,
  ];
  const field = (key: string) => {
    const input = document.createElement("input");
    input.type = "text";
    input.dataset.key = key;
    return input;
  };

  name(t("FirstUseSettings.lblMouseOverSuggestion"));
  const [suggestionRefresh, suggestionFrozen] = pair("initSuggestion",
    [t("FirstUseSettings.rdoMouseOverSuggestionRefresh"), "suggestionRefresh"],
    [t("FirstUseSettings.rdoMouseOverSuggestionNoRefresh"), "suggestionNoRefresh"]);
  name(t("FirstUseSettings.lblMouseOverSubboard"));
  const [subRefresh, subFrozen] = pair("initSub",
    [t("FirstUseSettings.rdoMouseOverSubboardRefresh"), "subRefresh"],
    [t("FirstUseSettings.rdoMouseOverSubboardNoRefresh"), "subNoRefresh"]);
  name(t("FirstUseSettings.lblShowScore"));
  const [withKomi, justScore] = pair("initScore",
    [t("FirstUseSettings.rdoScoreOnBoardWithKomi"), "scoreWithKomi"],
    [t("FirstUseSettings.rdoScoreOnBoardJustScore"), "scoreJustScore"]);
  name(t("FirstUseSettings.lblWinratePerspective"));
  const [alwaysBlack, alternately] = pair("initPerspective",
    [t("FirstUseSettings.rdoAlwaysBlack"), "alwaysBlack"],
    [t("FirstUseSettings.rdoAlternately"), "alternately"]);
  name(t("FirstUseSettings.lblLizzieCache"), help("init.cacheHelp", t("FirstUseSettings.lizzieCacheDiscribeTitle"), t("FirstUseSettings.lizzieCacheDiscribe")));
  const [cacheOn, cacheOff] = pair("initCache",
    [t("FirstUseSettings.rdoLizzieCacheEnable"), "cacheEnable"],
    [t("FirstUseSettings.rdoLizzieCacheDisable"), "cacheDisable"]);

  name(t("FirstUseSettings.lblSuggestionInfo"));
  const showWinrate = choice("checkbox", t("FirstUseSettings.chkWinrate"), "init.showWinrate", "a").input;
  const showPlayouts = choice("checkbox", t("FirstUseSettings.chkVisits"), "init.showPlayouts", "b").input;
  const showScore = choice("checkbox", t("FirstUseSettings.chkScoreLead"), "init.showScore", "c").input;
  const order = document.createElement("button");
  order.type = "button";
  order.textContent = t("FirstUseSettings.btnCustomOrder");
  order.dataset.key = "init.customOrder";
  place(order, "end");
  // SuggestionInfoOrderSettings saves its own boxes and then refreshes these (setChkSuggestionInfo).
  order.onclick = () => openInfoOrderDialog((shows) => {
    showWinrate.checked = shows.suggestionWinrate;
    showPlayouts.checked = shows.suggestionPlayouts;
    showScore.checked = shows.suggestionScore;
  });

  name(t("FirstUseSettings.lblLimitSuggestion"));
  const limitSuggestion = place(field("init.limitSuggestion"), "a");
  limitSuggestion.setAttribute("aria-label", t("FirstUseSettings.lblLimitSuggestion"));
  const variationName = document.createElement("span");
  variationName.textContent = t("FirstUseSettings.lblLimitVariation");
  place(variationName, "b");
  const limitVariation = place(field("init.limitVariation"), "c");
  limitVariation.setAttribute("aria-label", t("FirstUseSettings.lblLimitVariation"));

  name(t("FirstUseSettings.lblMaxAnalyzeTime"), help("init.maxTimeHelp", t("FirstUseSettings.maxAnalyzeTimeDiscribeTitile"), t("FirstUseSettings.maxAnalyzeTimeDiscribe")));
  const limit = (column: "a" | "b-end", text: string, key: string, fieldKey: string) => {
    const unit = document.createElement("span");
    unit.className = "init-unit";
    const box = choice("checkbox", text, key);
    const input = field(fieldKey);
    input.setAttribute("aria-label", text);
    unit.append(box.wrap, input);
    place(unit, column);
    return [box.input, input];
  };
  const [limitTime, maxTime] = limit("a", t("FirstUseSettings.chkLimitTime"), "init.limitTime", "init.maxTime");
  const [limitPlayout, maxPlayouts] = limit("b-end", t("FirstUseSettings.chkLimitVisits"), "init.limitPlayout", "init.maxPlayouts");
  limitTime.addEventListener("change", () => (maxTime.disabled = !limitTime.checked));
  limitPlayout.addEventListener("change", () => (maxPlayouts.disabled = !limitPlayout.checked));

  name(t("FirstUseSettings.lblLanguage"));
  const languageChoice = place(document.createElement("select"), "a-end");
  languageChoice.dataset.key = "init.language";
  languageChoice.setAttribute("aria-label", t("FirstUseSettings.lblLanguage"));
  for (const [value, text] of LANGUAGES) languageChoice.append(new Option(text(), value));
  languageChoice.value = v.settings.language;

  const notes = document.createElement("div");
  notes.className = "init-notes";
  for (const text of [t("FirstUseSettings.lblNotice"), t("FirstUseSettings.lblHint")]) {
    const p = document.createElement("p");
    p.className = "engine-hint";
    p.textContent = text;
    notes.append(p);
  }
  form.append(notes);

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
  maxTime.value = String(analysis.maxAnalyzeTimeSeconds);
  // Whether the time field was given any input (typing, deleting, pasting) or 加载默认设置: only
  // then is its text a limit the user chose, even when it reads as before.
  let timeEdited = false;
  maxTime.addEventListener("input", () => (timeEdited = true));
  maxPlayouts.value = String(analysis.limitPlayouts);
  limitTime.checked = analysis.limitTime;
  limitPlayout.checked = analysis.limitPlayout;
  maxTime.disabled = !analysis.limitTime;
  maxPlayouts.disabled = !analysis.limitPlayout;

  const defaults = document.createElement("button");
  defaults.type = "button";
  defaults.textContent = t("FirstUseSettings.defaultSettings");
  defaults.dataset.key = "init.defaults";
  defaults.onclick = () => {
    showPlayouts.checked = showScore.checked = showWinrate.checked = true;
    cacheOn.checked = subFrozen.checked = suggestionFrozen.checked = justScore.checked = alternately.checked = true;
    limitSuggestion.value = "10";
    limitVariation.value = "0";
    limitTime.checked = true;
    maxTime.disabled = false;
    maxTime.value = "600";
    timeEdited = true;
    limitPlayout.checked = false;
    maxPlayouts.disabled = true;
    maxPlayouts.value = "";
  };
  const apply = document.createElement("button");
  apply.type = "button";
  apply.className = "primary";
  apply.textContent = t("FirstUseSettings.btnApply");
  apply.dataset.key = "init.apply";
  const actions = document.createElement("div");
  actions.className = "engine-actions";
  actions.append(defaults, apply);
  const body = document.createElement("div");
  body.append(form, actions);

  const dialog = formPanel(t("FirstUseSettings.title"), body);
  const box = body.parentElement!;
  box.classList.add("init-settings");
  const head = box.querySelector(".modal-title")!;
  const close = document.createElement("button");
  close.type = "button";
  close.className = "modal-close";
  close.append(icon("close"));
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
    // The time limit as saved stays when its field was given no input: a saved 0 (kept by the
    // toolbar and 综合设置) is not an analysis change. A limit of 0 or less entered in the field,
    // even the same 0 typed again, means 9999 minutes (FirstUseSettings.java:656-658).
    const savedSeconds = now.settings.engine.analysis.maxAnalyzeTimeSeconds;
    const seconds = time === null || !timeEdited ? savedSeconds : time <= 0 ? UNLIMITED_SECONDS : time;
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
