// 综合设置 board display rows (ConfigDialog2 modern cards, createDisplaySection 2955-3356):
// 引擎与分析 > 候选点外观与过滤 / 变化图与 PV 访问, 对局与操作 > 显示坐标 / 小棋盘不跟随刷新 and
// 鼠标交互与坐标格式 > 坐标格式. Like Java they apply with 保存设置; the other rows of those cards
// belong to other tickets and are not shown yet.
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
function check(host: HTMLElement, key: string, sub: string, value: boolean): HTMLInputElement {
  const row = checkbox(t(key), value);
  row.input.dataset.key = key;
  row.label.title = t(sub);
  const hint = document.createElement("p");
  hint.textContent = t(sub);
  host.append(row.label, hint);
  return row.input;
}

/** A combo box row; `items` pairs each value with its resource key. */
function combo<T extends string | number>(host: HTMLElement, key: string, sub: string, items: [T, string][], value: T): () => T {
  const select = document.createElement("select");
  select.dataset.key = key;
  select.title = t(sub);
  items.forEach(([, text], k) => {
    const option = document.createElement("option");
    option.value = String(k);
    option.textContent = t(text);
    select.append(option);
  });
  select.value = String(Math.max(0, items.findIndex(([v]) => v === value)));
  field(host, t(key), select);
  return () => items[Number(select.value)][0];
}

/** JFormattedTextField with DigitOnlyFilter(`pattern` removes the rest) and a unit after it. */
function text(host: HTMLElement, key: string, sub: string, value: string, unit: string, pattern: RegExp): HTMLInputElement {
  const control = input(value);
  control.dataset.key = key;
  control.title = t(sub);
  control.addEventListener("input", () => {
    const kept = control.value.replace(pattern, "");
    if (kept !== control.value) control.value = kept;
  });
  const wrap = document.createElement("span");
  wrap.className = "engine-unit";
  wrap.append(control, ` ${t(unit)}`);
  field(host, t(key), wrap);
  return control;
}

/** Utils.txtFieldDoubleValue / txtFieldIntValue: an unparsable field reads as 0. */
function fieldNumber(control: HTMLInputElement, integer: boolean): number {
  const v = integer ? Number.parseInt(control.value, 10) : Number.parseFloat(control.value);
  return Number.isFinite(v) ? v : 0;
}

/** Adds the rows to the 引擎与分析 (`engine`) and 对局与操作 (`play`) pages; `save` sends the
 *  edited display settings with 保存设置 and is false when the core refused them. */
export function buildDisplaySettings(engine: HTMLElement, play: HTMLElement): { save: () => Promise<boolean> } {
  const s = state.view!.settings.display;

  // 候选点外观与过滤 (ConfigDialog2.java:797-814, 970-1027, 6731-6805)
  card(engine, t("ConfigDialog2.modern.candidates.title"), t("ConfigDialog2.modern.candidates.subtitle"));
  const blueRing = check(engine, "ConfigDialog2.modern.candidates.blueRing", "ConfigDialog2.modern.candidates.blueRingSub", s.blueRing);
  const colorRatio = combo(engine, "ConfigDialog2.modern.candidates.colorRatio", "ConfigDialog2.modern.candidates.colorRatioSub", [
    [1, "LizzieConfig.SuggestionMoveColorConcentration1"],
    [2, "LizzieConfig.SuggestionMoveColorConcentration2"],
    [3, "LizzieConfig.SuggestionMoveColorConcentration3"],
  ], s.suggestionColorRatio);
  // The combo of white-suggestion-white x white-suggestion-order-white.
  const whiteStyle = combo(engine, "ConfigDialog2.modern.candidates.whiteStyle", "ConfigDialog2.modern.candidates.whiteStyleSub", [
    [0, "LizzieConfig.chkShowWhiteSuggWhite1"],
    [1, "LizzieConfig.chkShowWhiteSuggWhite2"],
    [2, "LizzieConfig.chkShowWhiteSuggWhite3"],
    [3, "LizzieConfig.chkShowWhiteSuggWhite4"],
  ], (s.whiteSuggestionWhite ? 1 : 0) + (s.whiteSuggestionOrderWhite ? 2 : 0));
  const noSuggCircle = check(engine, "ConfigDialog2.modern.candidates.noSuggCircle", "ConfigDialog2.modern.candidates.noSuggCircleSub", s.noSuggCircle);
  const minRatio = text(engine, "ConfigDialog2.modern.candidates.minPlayoutRatio", "ConfigDialog2.modern.candidates.minPlayoutRatioSub", javaDouble(s.minPlayoutRatio * 100), "ConfigDialog2.modern.unit.percent", /[^0-9.]+/g);

  // 变化图与 PV 访问 (ConfigDialog2.java:1606-1630, 1945-1951, 6565-6583)
  card(engine, t("ConfigDialog2.modern.pv.title"), t("ConfigDialog2.modern.pv.subtitle"));
  const pvMode = combo<PvVisitsMode>(engine, "ConfigDialog2.modern.pv.mode", "ConfigDialog2.modern.pv.modeSub", [
    ["Off", "FirstUseSettings.rdoNoPvVistits"],
    ["LastMove", "FirstUseSettings.rdoLastPvVistits"],
    ["AllMoves", "FirstUseSettings.rdoEveryPvVistits"],
  ], s.pvVisits);
  const pvLimit = text(engine, "ConfigDialog2.modern.pv.limit", "ConfigDialog2.modern.pv.limitSub", String(s.pvVisitsLimit), "ConfigDialog2.modern.unit.visits", /[^0-9]+/g);
  const removeDead = check(engine, "ConfigDialog2.modern.pv.removeDead", "ConfigDialog2.modern.pv.removeDeadSub", s.removeDeadInVariation);

  // 对局与操作 (ConfigDialog2.java:3152-3229): the T10 rows of the card.
  card(play, t("ConfigDialog2.modern.play.title"), t("ConfigDialog2.modern.play.subtitle"));
  const coordinates = check(play, "ConfigDialog2.modern.play.coordinates", "ConfigDialog2.modern.play.coordinatesSub", s.coordinates);
  const freezeSub = check(play, "ConfigDialog2.modern.play.freezeSubBoard", "ConfigDialog2.modern.play.freezeSubBoardSub", s.noRefreshOnSub);
  // 鼠标交互与坐标格式 > 坐标格式 (ConfigDialog2.java:1721-1734, 6520-6531)
  card(play, t("ConfigDialog2.modern.interaction.title"), t("ConfigDialog2.modern.interaction.subtitle"));
  const coords = combo<CoordinateStyle>(play, "ConfigDialog2.modern.interaction.specialCoords", "ConfigDialog2.modern.interaction.specialCoordsSub", [
    ["Normal", "ConfigDialog2.SpecialCoordsNormal"],
    ["WithI", "ConfigDialog2.SpecialCoordsWithI"],
    ["Fox", "ConfigDialog2.SpecialCoordsFox"],
    ["NumbersFromTop", "ConfigDialog2.SpecialCoordsNumberFromTop"],
    ["NumbersFromBottom", "ConfigDialog2.SpecialCoordsNumberFromBottom"],
  ], s.coordinateStyle);

  return {
    save: async () => {
      const current = state.view?.settings.display;
      if (!current) return false;
      const white = whiteStyle();
      const next: DisplaySettings = {
        ...current,
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
