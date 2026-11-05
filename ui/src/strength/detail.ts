// 棋力评估 - 详细数据 (showPlayerStrengthDetailDialog 17645-17668): on the 吻合度 tab the dark match
// chart (buildPlayerStrengthMatchPanel), otherwise the two dark metric cards with the review note
// (buildPlayerStrengthAssessmentPanel / buildPlayerStrengthAssessmentDetailCard).
import type { StrengthReport, StrengthSummary } from "../contract/generated";
import { t } from "../i18n";
import {
  averageScoreLossText, confidenceText, difficultyText, displayRank, humanSlCoverageText, matchRateText, precisePercentText, sideRankText,
  weightedScoreLossText,
} from "./format";
import { matchChart, noteStrip, paintDetailRoot } from "./panels";
import { painted } from "./paint";
import { openWindow } from "./window";

function el(tag: "div" | "span", cls: string, text = ""): HTMLElement {
  const e = document.createElement(tag);
  e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** PlayerStrengthDetailCard with its title and the GridLayout(0, 2, 10, 9) of metrics. */
function detailCard(black: boolean, s: StrengthSummary, humanSl: boolean): HTMLElement {
  const card = el("div", `ps-detail-card ${black ? "black" : "white"}`);
  card.append(el("div", "ps-detail-card-title", black ? t("Menu.Black") : t("Menu.White")));
  const metrics = el("div", "ps-detail-metrics");
  const rows: [string, string][] = [
    [t("PlayerStrengthEstimate.match.strength"), displayRank(s.strength_band)],
    [t("PlayerStrengthEstimate.rank.scale"), sideRankText(s)],
    [t("PlayerStrengthEstimate.confidence"), confidenceText(s.confidence)],
    [t("PlayerStrengthEstimate.moves"), String(s.sample_count)],
    [t("PlayerStrengthEstimate.firstChoice"), precisePercentText(s.first_choice_rate)],
    [t("PlayerStrengthEstimate.goodMoveRate"), precisePercentText(s.move_rank_good_move_rate)],
    [t("PlayerStrengthEstimate.weightedScoreLoss"), weightedScoreLossText(s)],
    [t("PlayerStrengthEstimate.match.avgScoreLoss"), averageScoreLossText(s)],
    [t("PlayerStrengthEstimate.mistakeRate"), precisePercentText(s.mistake_rate)],
    [t("PlayerStrengthEstimate.match.overallMatch"), matchRateText(s)],
  ];
  if (humanSl) rows.push([t("HumanSL40.difficulty"), difficultyText(s)], [t("HumanSL40.coverage"), humanSlCoverageText(s)]);
  for (const [label, value] of rows) metrics.append(el("span", "ps-metric-label", label), el("span", "ps-metric-value", value || "-"));
  card.append(metrics);
  return card;
}

/** The open 详细数据 dialog (acceptance probe): its kind and window. */
export let openDetail: { kind: "assessment" | "match"; box: HTMLElement } | null = null;

export function openDetailDialog(report: StrengthReport, matchDetail: boolean): void {
  const title = `${t("PlayerStrengthEstimate.title")} - ${t("PlayerStrengthEstimate.detailData")}`;
  const win = openWindow(title, `ps-detail-dialog${report.model.human_sl ? " humansl" : ""}`, () => {
    if (openDetail?.box === win.box) openDetail = null;
  });
  openDetail = { kind: matchDetail ? "match" : "assessment", box: win.box };
  const root = el("div", `ps-detail-root ${matchDetail ? "match" : "assessment"}`);
  root.append(painted("ps-root-bg", paintDetailRoot).element);
  if (matchDetail) {
    const chart = el("div", "ps-fill");
    chart.append(matchChart(report).element);
    root.append(chart);
  } else {
    const cards = el("div", "ps-two ps-gap-14 ps-detail-cards");
    cards.append(detailCard(true, report.black.summary, report.model.human_sl), detailCard(false, report.white.summary, report.model.human_sl));
    const note = el("div", "ps-detail-note-box");
    note.append(noteStrip(report.model.human_sl ? t("HumanSL40.reviewNote") : t("PlayerStrengthEstimate.reviewOnlyNote"), true).element);
    root.append(cards, note);
  }
  win.body.append(root);
}
