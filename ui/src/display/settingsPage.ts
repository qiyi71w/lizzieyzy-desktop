// 综合设置 board display rows (ConfigDialog2 modern cards, createDisplaySection 2955-3356):
// 引擎与分析 > 分析与胜率曲线 (display toggles) / 候选点外观与过滤 / 变化图与 PV 访问,
// 对局与操作 > 显示坐标 / 小棋盘不跟随刷新 and 鼠标交互与坐标格式 > 坐标格式. Like Java they apply
// with 保存设置. Each row carries its ConfigSettingTargets id as `data-setting-target`; rows of
// these cards owned by other settings are passed in by the caller and placed in Java order.
import { run, state } from "../app";
import type { CoordinateStyle, DisplaySettings, PvVisitsMode } from "../contract/generated";
import { t } from "../i18n";
import { checkbox, field, input } from "../engine/controls";
import { javaDouble } from "../workbench/paint";

function card(host: HTMLElement, title: string, subtitle: string): void {
  const heading = document.createElement("h3");
  heading.textContent = title;
  const sub = document.createElement("p");
  sub.textContent = subtitle;
  host.append(heading, sub);
}

/** A check box row with its sub line as the tooltip and the hint below. */
function check(host: HTMLElement, target: string, key: keyof DisplaySettings, title: string, sub: string, value: boolean): HTMLInputElement {
  const row = checkbox(title, value);
  row.input.dataset.key = `settings.display.${key}`;
  row.label.dataset.settingTarget = target;
  row.label.title = sub;
  const hint = document.createElement("p");
  hint.textContent = sub;
  host.append(row.label, hint);
  return row.input;
}

/** The row `field` just appended to `host`, tagged with its setting target, then its description. */
function tagLastRow(host: HTMLElement, target: string, sub: string): void {
  (host.lastElementChild as HTMLElement).dataset.settingTarget = target;
  const hint = document.createElement("p");
  hint.textContent = sub;
  host.append(hint);
}

/** A combo box row with its sub line as the tooltip and the hint below; `items` pairs each value with its text. */
function combo<T extends string | number>(host: HTMLElement, target: string, key: string, title: string, sub: string, items: [T, string][], value: T): () => T {
  const select = document.createElement("select");
  select.dataset.key = `settings.display.${key}`;
  select.title = sub;
  items.forEach(([, text], k) => {
    const option = document.createElement("option");
    option.value = String(k);
    option.textContent = text;
    select.append(option);
  });
  select.value = String(Math.max(0, items.findIndex(([v]) => v === value)));
  field(host, title, select);
  tagLastRow(host, target, sub);
  return () => items[Number(select.value)][0];
}

/** JFormattedTextField with DigitOnlyFilter(`pattern` removes the rest) and a unit after it. */
function text(host: HTMLElement, target: string, key: keyof DisplaySettings, title: string, sub: string, value: string, unit: string, pattern: RegExp): HTMLInputElement {
  const control = input(value);
  control.dataset.key = `settings.display.${key}`;
  control.title = sub;
  control.addEventListener("input", () => {
    const kept = control.value.replace(pattern, "");
    if (kept !== control.value) control.value = kept;
  });
  const wrap = document.createElement("span");
  wrap.className = "engine-unit";
  wrap.append(control, ` ${unit}`);
  field(host, title, wrap);
  tagLastRow(host, target, sub);
  return control;
}

/** Utils.txtFieldDoubleValue / txtFieldIntValue: an unparsable field reads as 0. */
function fieldNumber(control: HTMLInputElement, integer: boolean): number {
  const v = integer ? Number.parseInt(control.value, 10) : Number.parseFloat(control.value);
  return Number.isFinite(v) ? v : 0;
}

/** Rows of the shared cards that other settings own, inserted where Java places them. */
export interface SharedCardRows {
  /** After the 分析与胜率曲线 display toggles: the analysis limits and 实况分析限制. */
  analysis: Node;
  /** First rows of 对局与操作, before 显示坐标. */
  play: Node;
  /** First rows of 鼠标交互与坐标格式, before 坐标格式. */
  interaction: Node;
}

/** Adds the rows to the 引擎与分析 (`engine`) and 对局与操作 (`play`) pages; `save` sends the
 *  edited display settings with 保存设置 and is false when the core refused them. */
export function buildDisplaySettings(engine: HTMLElement, play: HTMLElement, shared: SharedCardRows): { save: () => Promise<boolean> } {
  const s = state.view!.settings.display;

  // 分析与胜率曲线 (ConfigDialog2.java:2994-3062, 6534-6553, 6739, 6780, 6829); the 胜率曲线 and
  // 分支面板 visibility rows belong to the panel layout.
  card(engine, t("ConfigDialog2.modern.analysis.title"), t("ConfigDialog2.modern.analysis.subtitle"));
  const blackWinrate = check(engine, "settings.black-winrate", "winrateAlwaysBlack", t("Menu.alwaysShowBlackWinrate"), t("FunctionSearch.description.blackWinrate"), s.winrateAlwaysBlack);
  const blunderBar = check(engine, "config.analysis.blunder-bar", "blunderBar", t("ConfigDialog2.modern.analysis.blunderBar"), t("ConfigDialog2.modern.analysis.blunderBarSub"), s.blunderBar);
  const hover = check(engine, "config.analysis.hover", "mouseOverWinrateGraph", t("ConfigDialog2.modern.analysis.hover"), t("ConfigDialog2.modern.analysis.hoverSub"), s.mouseOverWinrateGraph);
  const graphFill = check(engine, "config.analysis.graph-fill", "winrateGraphFill", t("ConfigDialog2.modern.analysis.graphFill"), t("ConfigDialog2.modern.analysis.graphFillSub"), s.winrateGraphFill);
  const maxRed = check(engine, "config.analysis.max-red", "suggestionMaxRed", t("ConfigDialog2.modern.analysis.maxRed"), t("ConfigDialog2.modern.analysis.maxRedSub"), s.suggestionMaxRed);
  engine.append(shared.analysis);

  // 候选点外观与过滤 (ConfigDialog2.java:797-814, 970-1027, 6731-6805)
  card(engine, t("ConfigDialog2.modern.candidates.title"), t("ConfigDialog2.modern.candidates.subtitle"));
  const blueRing = check(engine, "config.candidates.blue-ring", "blueRing", t("ConfigDialog2.modern.candidates.blueRing"), t("ConfigDialog2.modern.candidates.blueRingSub"), s.blueRing);
  const colorRatio = combo(engine, "config.candidates.color-ratio", "suggestionColorRatio", t("ConfigDialog2.modern.candidates.colorRatio"), t("ConfigDialog2.modern.candidates.colorRatioSub"), [
    [1, t("LizzieConfig.SuggestionMoveColorConcentration1")],
    [2, t("LizzieConfig.SuggestionMoveColorConcentration2")],
    [3, t("LizzieConfig.SuggestionMoveColorConcentration3")],
  ], s.suggestionColorRatio);
  // The combo of white-suggestion-white x white-suggestion-order-white.
  const whiteStyle = combo(engine, "config.candidates.white-style", "whiteSuggestion", t("ConfigDialog2.modern.candidates.whiteStyle"), t("ConfigDialog2.modern.candidates.whiteStyleSub"), [
    [0, t("LizzieConfig.chkShowWhiteSuggWhite1")],
    [1, t("LizzieConfig.chkShowWhiteSuggWhite2")],
    [2, t("LizzieConfig.chkShowWhiteSuggWhite3")],
    [3, t("LizzieConfig.chkShowWhiteSuggWhite4")],
  ], (s.whiteSuggestionWhite ? 1 : 0) + (s.whiteSuggestionOrderWhite ? 2 : 0));
  const noSuggCircle = check(engine, "config.candidates.no-suggestion-circle", "noSuggCircle", t("ConfigDialog2.modern.candidates.noSuggCircle"), t("ConfigDialog2.modern.candidates.noSuggCircleSub"), s.noSuggCircle);
  const minRatio = text(engine, "config.candidates.minimum-playout-ratio", "minPlayoutRatio", t("ConfigDialog2.modern.candidates.minPlayoutRatio"), t("ConfigDialog2.modern.candidates.minPlayoutRatioSub"), javaDouble(s.minPlayoutRatio * 100), t("ConfigDialog2.modern.unit.percent"), /[^0-9.]+/g);

  // 变化图与 PV 访问 (ConfigDialog2.java:1606-1630, 1945-1951, 6565-6583)
  card(engine, t("ConfigDialog2.modern.pv.title"), t("ConfigDialog2.modern.pv.subtitle"));
  const pvMode = combo<PvVisitsMode>(engine, "config.pv.mode", "pvVisits", t("ConfigDialog2.modern.pv.mode"), t("ConfigDialog2.modern.pv.modeSub"), [
    ["Off", t("FirstUseSettings.rdoNoPvVistits")],
    ["LastMove", t("FirstUseSettings.rdoLastPvVistits")],
    ["AllMoves", t("FirstUseSettings.rdoEveryPvVistits")],
  ], s.pvVisits);
  const pvLimit = text(engine, "config.pv.visit-limit", "pvVisitsLimit", t("ConfigDialog2.modern.pv.limit"), t("ConfigDialog2.modern.pv.limitSub"), String(s.pvVisitsLimit), t("ConfigDialog2.modern.unit.visits"), /[^0-9]+/g);
  const removeDead = check(engine, "config.pv.remove-dead", "removeDeadInVariation", t("ConfigDialog2.modern.pv.removeDead"), t("ConfigDialog2.modern.pv.removeDeadSub"), s.removeDeadInVariation);

  // 对局与操作 (ConfigDialog2.java:3152-3204)
  card(play, t("ConfigDialog2.modern.play.title"), t("ConfigDialog2.modern.play.subtitle"));
  play.append(shared.play);
  const coordinates = check(play, "config.play.coordinates", "coordinates", t("ConfigDialog2.modern.play.coordinates"), t("ConfigDialog2.modern.play.coordinatesSub"), s.coordinates);
  const freezeSub = check(play, "config.play.freeze-sub-board", "noRefreshOnSub", t("ConfigDialog2.modern.play.freezeSubBoard"), t("ConfigDialog2.modern.play.freezeSubBoardSub"), s.noRefreshOnSub);
  // 鼠标交互与坐标格式 (ConfigDialog2.java:3205-3228); 坐标格式 1721-1734, 6520-6531
  card(play, t("ConfigDialog2.modern.interaction.title"), t("ConfigDialog2.modern.interaction.subtitle"));
  play.append(shared.interaction);
  const coords = combo<CoordinateStyle>(play, "config.interaction.coordinates", "coordinateStyle", t("ConfigDialog2.modern.interaction.specialCoords"), t("ConfigDialog2.modern.interaction.specialCoordsSub"), [
    ["Normal", t("ConfigDialog2.SpecialCoordsNormal")],
    ["WithI", t("ConfigDialog2.SpecialCoordsWithI")],
    ["Fox", t("ConfigDialog2.SpecialCoordsFox")],
    ["NumbersFromTop", t("ConfigDialog2.SpecialCoordsNumberFromTop")],
    ["NumbersFromBottom", t("ConfigDialog2.SpecialCoordsNumberFromBottom")],
  ], s.coordinateStyle);

  return {
    save: async () => {
      const current = state.view?.settings.display;
      if (!current) return false;
      const white = whiteStyle();
      const next: DisplaySettings = {
        ...current,
        winrateAlwaysBlack: blackWinrate.checked,
        blunderBar: blunderBar.checked,
        mouseOverWinrateGraph: hover.checked,
        winrateGraphFill: graphFill.checked,
        suggestionMaxRed: maxRed.checked,
        blueRing: blueRing.checked,
        suggestionColorRatio: colorRatio(),
        whiteSuggestionWhite: (white & 1) !== 0,
        whiteSuggestionOrderWhite: (white & 2) !== 0,
        noSuggCircle: noSuggCircle.checked,
        minPlayoutRatio: fieldNumber(minRatio, false) / 100,
        pvVisits: pvMode(),
        pvVisitsLimit: fieldNumber(pvLimit, true),
        removeDeadInVariation: removeDead.checked,
        coordinates: coordinates.checked,
        noRefreshOnSub: freezeSub.checked,
        coordinateStyle: coords(),
      };
      if (JSON.stringify(next) === JSON.stringify(current)) return true;
      const result = await run({ type: "SetDisplaySettings", settings: next });
      return !!result && result.type !== "Refused";
    },
  };
}
