// 棋力评估 - 详细数据 (showPlayerStrengthDetailDialog 17645-17668): on the 吻合度 tab the match
// chart (buildPlayerStrengthMatchPanel), on 分阶段 every figure of every phase (Java
// StrengthPhaseTable), otherwise the two sides' figures with the review note
// (buildPlayerStrengthAssessmentPanel / buildPlayerStrengthAssessmentDetailCard). The sides are
// sections split by a hairline on the dialog's ground.
import type { StrengthPhases, StrengthReport, StrengthSummary } from "../contract/generated";
import { product, t } from "../i18n";
import {
  averageScoreLossText, confidenceText, difficultyText, displayRank, humanSlCoverageText, matchRateText, precisePercentText, rankText,
  sideRankText, weightedScoreLossText,
} from "./format";
import { el, explain, legend, matchChart, noteStrip, sideHeading, stoneMark } from "./panels";
import { phaseName, phaseRange } from "./phases";
import { openWindow } from "./window";

/** PlayerStrengthDetailCard: the side's title and its metrics in two columns of label and value,
 *  each row explained by its help (StrengthDetailTable; the HumanSL figures' help is this
 *  product's, Java has none). */
function detailCard(black: boolean, s: StrengthSummary, humanSl: boolean): HTMLElement {
  const card = el("section", `ps-side ps-detail-card ${black ? "black" : "white"}`);
  const metrics = el("dl", "ps-detail-metrics");
  const rows: [string, string, string, string][] = [
    [t("PlayerStrengthEstimate.match.strength"), displayRank(s.strength_band), t("PlayerStrengthEstimate.help.band"), "band"],
    [t("PlayerStrengthEstimate.rank.scale"), sideRankText(s), t("PlayerStrengthEstimate.help.rank"), "rank"],
    [t("PlayerStrengthEstimate.confidence"), confidenceText(s.confidence), t("PlayerStrengthEstimate.help.confidence"), "confidence"],
    [t("PlayerStrengthEstimate.moves"), String(s.sample_count), t("PlayerStrengthEstimate.help.moves"), "moves"],
    [t("PlayerStrengthEstimate.firstChoice"), precisePercentText(s.first_choice_rate), t("PlayerStrengthEstimate.help.firstChoice"), "firstChoice"],
    [t("PlayerStrengthEstimate.goodMoveRate"), precisePercentText(s.move_rank_good_move_rate), t("PlayerStrengthEstimate.help.goodMoveRate"), "goodMoveRate"],
    [t("PlayerStrengthEstimate.weightedScoreLoss"), weightedScoreLossText(s), t("PlayerStrengthEstimate.help.weightedScoreLoss"), "weightedScoreLoss"],
    [t("PlayerStrengthEstimate.match.avgScoreLoss"), averageScoreLossText(s), t("PlayerStrengthEstimate.help.avgScoreLoss"), "avgScoreLoss"],
    [t("PlayerStrengthEstimate.mistakeRate"), precisePercentText(s.mistake_rate), t("PlayerStrengthEstimate.help.mistakeRate"), "mistakeRate"],
    [t("PlayerStrengthEstimate.match.overallMatch"), matchRateText(s), t("PlayerStrengthEstimate.help.overallMatch"), "overallMatch"],
  ];
  if (humanSl) {
    rows.push(
      [t("HumanSL40.difficulty"), difficultyText(s), product("strengthHumanSlDifficultyHelp"), "difficulty"],
      [t("HumanSL40.coverage"), humanSlCoverageText(s), product("strengthHumanSlCoverageHelp"), "coverage"],
    );
  }
  for (const [label, value, help, name] of rows) {
    const row = el("div", "ps-detail-metric");
    explain(row, help, `detail.${black ? "black" : "white"}.${name}`);
    row.append(el("dt", "ps-metric-label", label), el("dd", "ps-metric-value", value || "-"));
    metrics.append(row);
  }
  card.append(sideHeading(black, black ? t("Menu.Black") : t("Menu.White")), metrics);
  return card;
}

/** StrengthPhaseTable: a row per figure with Black's and White's value in each phase, each label
 *  explained by its help. Wide, the phases stand side by side under their names; in a narrow
 *  window each phase's rows follow its name (strength.css places the cells). */
function phaseTable(phases: StrengthPhases): HTMLElement {
  const rows: [string, string, boolean, (s: StrengthSummary) => string][] = [
    [t("PlayerStrengthEstimate.detail.rank"), t("PlayerStrengthEstimate.help.rank"), true, (s) => rankText(s.rank_value)],
    [t("PlayerStrengthEstimate.confidence"), t("PlayerStrengthEstimate.help.confidence"), true, (s) => confidenceText(s.confidence)],
    [t("PlayerStrengthEstimate.moves"), t("PlayerStrengthEstimate.help.moves"), false, (s) => String(s.sample_count)],
    [t("PlayerStrengthEstimate.firstChoice"), t("PlayerStrengthEstimate.help.firstChoice"), false, (s) => precisePercentText(s.first_choice_rate)],
    [t("PlayerStrengthEstimate.goodMoveRate"), t("PlayerStrengthEstimate.help.goodMoveRate"), false, (s) => precisePercentText(s.move_rank_good_move_rate)],
    [t("PlayerStrengthEstimate.weightedScoreLoss"), t("PlayerStrengthEstimate.help.weightedScoreLoss"), false, weightedScoreLossText],
    [t("PlayerStrengthEstimate.match.avgScoreLoss"), t("PlayerStrengthEstimate.help.avgScoreLoss"), false, averageScoreLossText],
    [t("PlayerStrengthEstimate.mistakeRate"), t("PlayerStrengthEstimate.help.mistakeRate"), false, (s) => precisePercentText(s.mistake_rate)],
  ];
  const helpTags = ["rank", "confidence", "moves", "firstChoice", "goodMoveRate", "weightedScoreLoss", "avgScoreLoss", "mistakeRate"];
  const table = el("div", "ps-phase-table");
  table.setAttribute("role", "table");
  // Each phase takes two rows of names and stones above its figure rows when narrow.
  const stride = rows.length + 2;
  const place = (cell: HTMLElement, wide: [number, number], narrow: [number, number]) => {
    cell.style.setProperty("--wide-col", String(wide[0]));
    cell.style.setProperty("--wide-row", String(wide[1]));
    cell.style.setProperty("--narrow-col", String(narrow[0]));
    cell.style.setProperty("--narrow-row", String(narrow[1]));
    table.append(cell);
    return cell;
  };
  phases.phases.forEach((phase, p) => {
    const head = el("div", `ps-pt-head${p > 0 ? " ps-pt-sep" : ""}`);
    const dot = el("span", `ps-phase-dot tint-${p}`);
    dot.setAttribute("aria-hidden", "true");
    const name = el("span", "ps-pt-name");
    name.append(dot, el("span", "", phaseName(p)));
    head.append(name, el("span", "ps-label", phaseRange(phase)));
    head.setAttribute("role", "columnheader");
    place(head, [2 + p * 2, 1], [1, 1 + p * stride]).classList.add("ps-pt-span");
    [true, false].forEach((black, s) => {
      const cell = el("div", `ps-pt-stone${s === 0 && p > 0 ? " ps-pt-sep" : ""}`);
      cell.append(stoneMark(black));
      cell.setAttribute("aria-label", black ? t("SidebarHeader.black") : t("SidebarHeader.white"));
      place(cell, [2 + p * 2 + s, 2], [2 + s, 2 + p * stride]);
    });
    rows.forEach(([label, help, needsRank, value], r) => {
      const labelCell = explain(el("div", `ps-pt-label${p > 0 ? " ps-pt-repeat" : ""}`, label), help, `phaseDetail.${p}.${helpTags[r]}`);
      labelCell.setAttribute("role", "rowheader");
      place(labelCell, [1, 3 + r], [1, 3 + p * stride + r]);
      for (const [s, side] of [phase.black, phase.white].entries()) {
        const shown = needsRank ? side.ranked : side.summary.sample_count > 0;
        const text = shown ? value(side.summary).trim() : "";
        const cell = el("div", `ps-pt-value${s === 0 && p > 0 ? " ps-pt-sep" : ""}`, text && text !== "-" ? text : "\u2014");
        cell.setAttribute("role", "cell");
        place(cell, [2 + p * 2 + s, 3 + r], [2 + s, 3 + p * stride + r]);
      }
    });
  });
  return table;
}

/** The open 详细数据 dialog (acceptance probe): its kind and window. */
export let openDetail: { kind: "assessment" | "match" | "phases"; box: HTMLElement } | null = null;

export function openDetailDialog(report: StrengthReport, kind: "assessment" | "match" | "phases", phases: StrengthPhases): void {
  const title = `${t("PlayerStrengthEstimate.title")} - ${t("PlayerStrengthEstimate.detailData")}`;
  const win = openWindow(title, `ps-detail-dialog${report.model.human_sl ? " humansl" : ""}`, () => {
    if (openDetail?.box === win.box) openDetail = null;
  });
  openDetail = { kind, box: win.box };
  const root = el("div", `ps-detail-root ${kind}`);
  const note = () => noteStrip(report.model.human_sl ? t("HumanSL40.reviewNote") : t("PlayerStrengthEstimate.reviewOnlyNote"));
  if (kind === "match") {
    const head = el("div", "ps-section-head");
    head.append(legend());
    const chart = el("div", "ps-fill");
    chart.append(matchChart(report).element);
    root.append(head, chart);
  } else if (kind === "phases") {
    // StrengthDialog.phaseDetailPanel.
    root.append(phaseTable(phases), note());
  } else {
    const cards = el("div", "ps-two ps-detail-cards");
    cards.append(detailCard(true, report.black.summary, report.model.human_sl), detailCard(false, report.white.summary, report.model.human_sl));
    root.append(cards, note());
  }
  win.body.append(root);
}
