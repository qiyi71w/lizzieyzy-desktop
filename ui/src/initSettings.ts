// 设置 > 初始化设置 (Menu.java:5369-5378, FirstUseSettings(false), FirstUseSettings.java:33-766): a
// modal form of grouped rows (engine/controls.ts) over the usual display and analysis options,
// filled from the current settings. 加载默认设置 (at the footer's start) changes only the form; 确定
// saves everything and closes; 取消, Escape or the close box saves nothing. Java's 界面样式 row
// (Swing look and feel) is not part of this product and no restart hint follows. The language is
// saved like the 语言 menu and, as in Java, applies from the next start.
import { run, state } from "./app";
import { COMMANDS } from "./commands";
import type { AnalysisSettings, DisplaySettings, Language } from "./contract/generated";
import { icon } from "./design/icons";
import { formPanel, message } from "./dialogs";
import { openInfoOrderDialog, parseJavaInt } from "./display/dialogs";
import { button, footer, groupNote, groupTitle, primaryButton, row, segmented, switchRow } from "./engine/controls";
import { product, t } from "./i18n";
import "./display/dialogs.css";

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
  const body = document.createElement("div");
  body.className = "init-settings";
  /** The next group of rows; a gap parts the groups. */
  const group = () => {
    const el = document.createElement("div");
    el.className = "init-group";
    body.append(el);
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
  /** A row of two exclusive choices keyed `init.<key>`: a segmented control at the row's end, or
   *  for long texts (`long`) radios one under the other on the line under the row's name.
   *  `before` stands at the row's end, ahead of a segmented control. */
  const pair = (host: HTMLElement, title: string, name: string, first: [string, string], second: [string, string], long = false, ...before: HTMLElement[]) => {
    let inputs: HTMLInputElement[];
    if (long) {
      const r = row(host, title, "", ...before);
      r.classList.add("init-wide-row");
      const rowName = r.querySelector<HTMLElement>(".ds-row-title")!;
      rowName.id = `init-name-${name}`;
      const choices = document.createElement("div");
      choices.className = "init-radios";
      choices.setAttribute("role", "radiogroup");
      choices.setAttribute("aria-labelledby", rowName.id);
      inputs = [first, second].map(([text]) => {
        const input = document.createElement("input");
        input.type = "radio";
        input.name = name;
        const label = document.createElement("label");
        label.append(input, text);
        choices.append(label);
        return input;
      });
      r.append(choices);
    } else {
      const seg = segmented(name, [[first[1], first[0]], [second[1], second[0]]], "");
      inputs = seg.inputs;
      row(host, title, "", ...before, seg.el);
    }
    inputs[0].dataset.key = `init.${first[1]}`;
    inputs[1].dataset.key = `init.${second[1]}`;
    return inputs;
  };
  const field = (key: string) => {
    const input = document.createElement("input");
    input.type = "text";
    input.inputMode = "numeric";
    input.dataset.key = key;
    return input;
  };

  // How the board shows the analysis.
  const shows = group();
  const [suggestionRefresh, suggestionFrozen] = pair(shows, t("FirstUseSettings.lblMouseOverSuggestion"), "initSuggestion",
    [t("FirstUseSettings.rdoMouseOverSuggestionRefresh"), "suggestionRefresh"],
    [t("FirstUseSettings.rdoMouseOverSuggestionNoRefresh"), "suggestionNoRefresh"], true);
  const [subRefresh, subFrozen] = pair(shows, t("FirstUseSettings.lblMouseOverSubboard"), "initSub",
    [t("FirstUseSettings.rdoMouseOverSubboardRefresh"), "subRefresh"],
    [t("FirstUseSettings.rdoMouseOverSubboardNoRefresh"), "subNoRefresh"], true);
  const [withKomi, justScore] = pair(shows, t("FirstUseSettings.lblShowScore"), "initScore",
    [t("FirstUseSettings.rdoScoreOnBoardWithKomi"), "scoreWithKomi"],
    [t("FirstUseSettings.rdoScoreOnBoardJustScore"), "scoreJustScore"]);
  const [alwaysBlack, alternately] = pair(shows, t("FirstUseSettings.lblWinratePerspective"), "initPerspective",
    [t("FirstUseSettings.rdoAlwaysBlack"), "alwaysBlack"],
    [t("FirstUseSettings.rdoAlternately"), "alternately"]);
  const [cacheOn, cacheOff] = pair(shows, t("FirstUseSettings.lblLizzieCache"), "initCache",
    [t("FirstUseSettings.rdoLizzieCacheEnable"), "cacheEnable"],
    [t("FirstUseSettings.rdoLizzieCacheDisable"), "cacheDisable"], false,
    help("init.cacheHelp", t("FirstUseSettings.lizzieCacheDiscribeTitle"), t("FirstUseSettings.lizzieCacheDiscribe")));

  // 选点信息: a switch per item, then the order in its own dialog.
  const info = group();
  groupTitle(info, t("FirstUseSettings.lblSuggestionInfo"));
  const infoSwitch = (text: string, key: string) => {
    const input = switchRow(info, text, "", false).input;
    input.dataset.key = key;
    return input;
  };
  const showWinrate = infoSwitch(t("FirstUseSettings.chkWinrate"), "init.showWinrate");
  const showPlayouts = infoSwitch(t("FirstUseSettings.chkVisits"), "init.showPlayouts");
  const showScore = infoSwitch(t("FirstUseSettings.chkScoreLead"), "init.showScore");
  const order = button(t("FirstUseSettings.btnCustomOrder"), () => {});
  order.type = "button";
  order.dataset.key = "init.customOrder";
  // SuggestionInfoOrderSettings saves its own switches and then refreshes these (setChkSuggestionInfo).
  order.onclick = () => openInfoOrderDialog((shows) => {
    showWinrate.checked = shows.suggestionWinrate;
    showPlayouts.checked = shows.suggestionPlayouts;
    showScore.checked = shows.suggestionScore;
  });
  row(info, product("infoOrderLines"), "", order);

  // The limits: the candidates and variation, then the analysis of one move.
  const limits = group();
  const limitSuggestion = field("init.limitSuggestion");
  row(limits, t("FirstUseSettings.lblLimitSuggestion"), "", limitSuggestion);
  const limitVariation = field("init.limitVariation");
  row(limits, t("FirstUseSettings.lblLimitVariation"), "", limitVariation);
  const perMove = group();
  groupTitle(perMove, t("FirstUseSettings.lblMaxAnalyzeTime")).append(help("init.maxTimeHelp", t("FirstUseSettings.maxAnalyzeTimeDiscribeTitile"), t("FirstUseSettings.maxAnalyzeTimeDiscribe")));
  /** A limit's switch with the field it enables before it. */
  const limit = (text: string, key: string, fieldKey: string) => {
    const input = field(fieldKey);
    const box = switchRow(perMove, text, "", false, input).input;
    box.dataset.key = key;
    return [box, input];
  };
  const [limitTime, maxTime] = limit(t("FirstUseSettings.chkLimitTime"), "init.limitTime", "init.maxTime");
  const [limitPlayout, maxPlayouts] = limit(t("FirstUseSettings.chkLimitVisits"), "init.limitPlayout", "init.maxPlayouts");
  limitTime.addEventListener("change", () => (maxTime.disabled = !limitTime.checked));
  limitPlayout.addEventListener("change", () => (maxPlayouts.disabled = !limitPlayout.checked));

  const languages = group();
  const languageChoice = document.createElement("select");
  languageChoice.dataset.key = "init.language";
  for (const [value, text] of LANGUAGES) languageChoice.append(new Option(text(), value));
  languageChoice.value = v.settings.language;
  row(languages, t("FirstUseSettings.lblLanguage"), "", languageChoice);

  for (const text of [t("FirstUseSettings.lblNotice"), t("FirstUseSettings.lblHint")]) groupNote(body, text);

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

  const defaults = button(t("FirstUseSettings.defaultSettings"), () => {
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
  });
  defaults.type = "button";
  defaults.dataset.key = "init.defaults";
  const apply = primaryButton(t("FirstUseSettings.btnApply"), () => {});
  apply.type = "button";
  apply.dataset.key = "init.apply";

  const dialog = formPanel(t("FirstUseSettings.title"), body);
  dialog.closeButton.dataset.key = "init.close";
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "init.cancel";
  footer(body, { start: [defaults], primary: apply, cancel });

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
