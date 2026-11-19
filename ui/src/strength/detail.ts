// 棋力评估 - 详细数据 (showPlayerStrengthDetailDialog 17645-17668): on the 吻合度 tab the match
// chart (buildPlayerStrengthMatchPanel), otherwise the two sides' figures with the review note
// (buildPlayerStrengthAssessmentPanel / buildPlayerStrengthAssessmentDetailCard). The sides are
// sections split by a hairline on the dialog's ground.
import type { StrengthReport, StrengthSummary } from "../contract/generated";
import { t } from "../i18n";
import {
  averageScoreLossText, confidenceText, difficultyText, displayRank, humanSlCoverageText, matchRateText, precisePercentText, sideRankText,
  weightedScoreLossText,
} from "./format";
import { el, legend, matchChart, noteStrip, sideHeading } from "./panels";
import { openWindow } from "./window";

/** PlayerStrengthDetailCard: the side's title and its metrics in two columns of label and value. */
function detailCard(black: boolean, s: StrengthSummary, humanSl: boolean): HTMLElement {
  const card = el("section", `ps-side ps-detail-card ${black ? "black" : "white"}`);
  const metrics = el("dl", "ps-detail-metrics");
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
  for (const [label, value] of rows) {
    const row = el("div", "ps-detail-metric");
    row.append(el("dt", "ps-metric-label", label), el("dd", "ps-metric-value", value || "-"));
    metrics.append(row);
  }
  card.append(sideHeading(black, black ? t("Menu.Black") : t("Menu.White")), metrics);
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
  if (matchDetail) {
    const head = el("div", "ps-section-head");
    head.append(legend());
    const chart = el("div", "ps-fill");
    chart.append(matchChart(report).element);
    root.append(head, chart);
  } else {
    const cards = el("div", "ps-two ps-detail-cards");
    cards.append(detailCard(true, report.black.summary, report.model.human_sl), detailCard(false, report.white.summary, report.model.human_sl));
    root.append(cards, noteStrip(report.model.human_sl ? t("HumanSL40.reviewNote") : t("PlayerStrengthEstimate.reviewOnlyNote")));
  }
  win.body.append(root);
}
