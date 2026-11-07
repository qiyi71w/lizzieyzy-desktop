// The painted panels of the 棋力评估 dashboard and its detail dialog (LizzieFrame.java nested
// classes 18335-20597): each paints exactly what its Java paintComponent paints at the size the
// layout gives it. Hover states and tooltips follow the Java mouse listeners.
import type { StrengthReport, StrengthSample, StrengthSideReport, StrengthSummary } from "../contract/generated";
import { rankColor } from "../appearance/model";
import { state } from "../app";
import { t } from "../i18n";
import {
  aiLikelihood, averageScoreLossText, confidenceText, hasSamples, highestDanLevel, isGoodMove, javaFixed, javaFormat, MOVE_RANKS,
  moveRankName, percentText, rankScaleRows, rankScaleSelectedIndex, rankScalePlainText, rankScaleTooltip, sideRankText, strengthLabel,
  weightedScoreLossText,
} from "./format";
import {
  bar, brighter, components, DASH, DETAIL, drawRound, fillOval, fillRect, fillRound, ff, fittedText, generatedStone, gradient,
  imageCover, infoBadge, line, metrics, painted, rgb, roundedCard, setFont, stone, text, width, type G, type Painted,
} from "./paint";

const i = Math.trunc;
const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));
const sideName = (black: boolean) => (black ? t("Menu.Black") : t("Menu.White"));

/** PlayerStrengthDashboardRoot.paintComponent: the paper picture (or gradient) and its frame. */
export function paintDashboardRoot(g: G, w: number, h: number): void {
  if (imageCover(g, "player_strength_bg.png", 0, 0, w, h, 1)) fillRect(g, 0, 0, w, h, rgb(255, 252, 245, 104));
  else fillRect(g, 0, 0, w, h, gradient(g, 0, 0, DASH.PAPER, 0, h, DASH.BACKGROUND));
  fillRound(g, 18, 18, Math.max(0, w - 36), Math.max(0, h - 36), 28, rgb(255, 255, 255, 46));
}

/** PlayerStrengthDetailRoot.paintComponent. */
export function paintDetailRoot(g: G, w: number, h: number): void {
  g.fillStyle = gradient(g, 0, 0, DETAIL.BACKGROUND_TOP, 0, Math.max(1, h), DETAIL.BACKGROUND_BOTTOM);
  g.fillRect(0, 0, w, h);
  fillRound(g, 10, 10, Math.max(0, w - 20), Math.max(0, h - 20), 28, rgb(255, 255, 255, 14));
}

/** PlayerStrengthHeaderMark (62 x 50). */
export function headerMark(): Painted {
  return painted("ps-header-mark", (g, w, h) => {
    const blackY = Math.max(6, i((h - 30) / 2));
    fillRound(g, 4, 5, Math.max(42, w - 10), Math.max(34, h - 10), 18, rgb(190, 137, 55, 60));
    stone(g, true, 8, blackY, 30);
    stone(g, false, 30, blackY + 6, 24);
  });
}

/** The texts an assessment card last painted (acceptance probe): rank, the confidence value and
 *  the five metric values in drawing order. */
export interface CardTexts {
  rank: string;
  confidence: string;
  metrics: string[];
}

/** PlayerStrengthAssessmentCard: stone, 黑棋表现 / rank / 可信度, five metric rows and the 评分规则
 *  scale. The tooltip and accessible texts are Java's. */
export function assessmentCard(black: boolean, side: StrengthSideReport, humanSl: boolean): Painted & { drawn: CardTexts } {
  const report = side.summary;
  const drawn: CardTexts = { rank: "", confidence: "", metrics: [] };
  const card = painted("ps-assessment-card", (g, width, height) => {
    roundedCard(g, 0, 0, width, height);
    generatedStone(g, black, 30, 38, 72, 1);
    const scaleWidth = Math.max(136, Math.min(180, i(width / 3)));
    const scaleX = Math.max(286, width - scaleWidth - 28);
    const contentRight = Math.max(260, scaleX - 24);
    drawn.rank = sideRankText(report);
    drawn.confidence = confidenceText(report.confidence);
    drawn.metrics = [];
    fittedText(g, sideName(black) + t("PlayerStrengthEstimate.performanceSuffix"), 118, 58, contentRight - 118, DASH.TEXT, true, ff() + 6);
    fittedText(g, drawn.rank, 118, 122, contentRight - 118, black ? DASH.ACCENT_DARK : DASH.GOLD, true, ff() + 15);
    fittedText(g, `${t("PlayerStrengthEstimate.confidence")}: ${drawn.confidence}`, 120, 154, contentRight - 120, DASH.MUTED_TEXT, false, ff() + 1);
    drawRankScalePanel(g, scaleX, 68, scaleWidth, Math.max(178, height - 116));
    const rowY = 178;
    const moves = t("PlayerStrengthEstimate.moves");
    const metric = (y: number, label: string, value: string, fraction: number, color: string) => {
      // drawMetricRow: an empty HumanSL 40 side shows "-" and empty bars except for 手数.
      if (humanSl && !hasSamples(report) && label !== moves) {
        value = "-";
        fraction = 0;
      }
      const valueY = y + 14;
      drawn.metrics.push(value);
      line(g, 34, y - 18, contentRight, y - 18, DASH.LINE);
      fittedText(g, label, 36, y - 2, Math.max(120, contentRight - 36), DASH.TEXT, false, ff());
      fittedText(g, value, 36, valueY, 58, DASH.TEXT, true, ff() + 1);
      bar(g, 102, valueY - 8, Math.max(64, contentRight - 102), fraction, color);
    };
    metric(rowY, moves, String(report.sample_count), clamp(report.sample_count / 240, 0, 1), DASH.ACCENT);
    metric(rowY + 34, t("PlayerStrengthEstimate.firstChoice"), percentText(report.first_choice_rate), report.first_choice_rate, DASH.ACCENT);
    metric(rowY + 68, t("PlayerStrengthEstimate.goodMoveRate"), percentText(report.move_rank_good_move_rate), report.move_rank_good_move_rate, DASH.ACCENT);
    metric(rowY + 102, t("PlayerStrengthEstimate.avgScoreLoss"), averageScoreLossText(report), 1 - clamp(report.average_score_equivalent_loss / 8, 0, 1), DASH.GOLD);
    metric(rowY + 136, t("PlayerStrengthEstimate.weightedScoreLoss"), weightedScoreLossText(report), 1 - clamp(report.weighted_score_loss / 8, 0, 1), DASH.GOLD);
  });

  function drawRankScalePanel(g: G, x: number, y: number, w: number, h: number): void {
    const rows = rankScaleRows();
    const selected = humanSl && !hasSamples(report) ? -1 : rankScaleSelectedIndex(report.rank_value);
    const panelHeight = Math.max(154, h);
    fittedText(g, t("PlayerStrengthEstimate.rank.scale"), x, y - 13, w, DASH.MUTED_TEXT, false, Math.max(10, ff() - 1));
    fillRound(g, x, y, w, panelHeight, 16, rgb(252, 249, 241, 176));
    drawRound(g, x, y, w, panelHeight, 16, rgb(220, 209, 192));
    const rowHeight = Math.max(34, i((panelHeight - 12) / rows.length));
    rows.forEach(([scale, name], k) => {
      const rowY = y + 6 + k * rowHeight;
      const active = k === selected;
      if (active) fillRound(g, x + 6, rowY, w - 12, rowHeight - 4, 12, rgb(235, 222, 199, 235));
      fillOval(g, x + 13, rowY + i(rowHeight / 2) - 4, 8, 8, active ? DASH.ACCENT_DARK : DASH.MUTED_TEXT);
      const size = Math.max(10, ff() - 1);
      fittedText(g, scale, x + 28, rowY + i(rowHeight / 2) + 5, 34, active ? DASH.ACCENT_DARK : DASH.MUTED_TEXT, true, size);
      fittedText(g, name, x + 66, rowY + i(rowHeight / 2) + 5, w - 74, active ? DASH.ACCENT_DARK : DASH.TEXT, false, size);
    });
  }

  card.element.title = rankScaleTooltip(side);
  card.element.setAttribute("role", "img");
  card.element.setAttribute("aria-label", `${sideName(black)} ${t("PlayerStrengthEstimate.title")}`);
  card.element.setAttribute("aria-description", rankScalePlainText(side));
  return Object.assign(card, { drawn });
}

/** PlayerStrengthMatchSummaryCard: the 吻合度 ring and four small figures. */
export function matchSummaryCard(black: boolean, report: StrengthSummary): Painted {
  return painted("ps-match-card", (g, width, height) => {
    roundedCard(g, 0, 0, width, height);
    generatedStone(g, black, 22, 20, 44, 1);
    text(g, sideName(black) + t("PlayerStrengthEstimate.matchSuffix"), 74, 50, DASH.TEXT, true, ff() + 5);
    // drawRing: a 10 px round-capped arc from 12 o'clock, clockwise.
    const cx = 36 + 34;
    const cy = 72 + 34;
    g.lineWidth = 10;
    g.lineCap = "round";
    g.beginPath();
    g.arc(cx, cy, 34, 0, Math.PI * 2);
    g.strokeStyle = rgb(221, 215, 202);
    g.stroke();
    const value = clamp(report.match_rate, 0, 1);
    if (value > 0) {
      g.beginPath();
      g.arc(cx, cy, 34, -Math.PI / 2, -Math.PI / 2 + Math.PI * 2 * value);
      g.strokeStyle = black ? DASH.ACCENT : DASH.GOLD;
      g.stroke();
    }
    g.lineCap = "butt";
    text(g, percentText(report.match_rate), 118, 109, black ? DASH.ACCENT : DASH.GOLD, true, ff() + 14);
    text(g, t("PlayerStrengthEstimate.match.overallMatch"), 120, 134, DASH.MUTED_TEXT, false, ff());
    const metricX = Math.max(238, width - 250);
    const columnGap = Math.max(92, Math.min(118, i((width - metricX - 34) / 2)));
    const statWidth = Math.max(72, columnGap - 12);
    const tiny = (x: number, y: number, label: string, value: string) => {
      fittedText(g, label, x, y - 22, statWidth, DASH.MUTED_TEXT, false, ff());
      fittedText(g, value, x, y + 6, statWidth, DASH.TEXT, true, ff() + 6);
    };
    tiny(metricX, 62, t("PlayerStrengthEstimate.firstChoice"), percentText(report.first_choice_rate));
    tiny(metricX + columnGap, 62, t("PlayerStrengthEstimate.goodMoveRate"), percentText(report.move_rank_good_move_rate));
    tiny(metricX, 122, t("PlayerStrengthEstimate.moves"), String(report.sample_count));
    tiny(metricX + columnGap, 122, t("PlayerStrengthEstimate.mistakeRate"), percentText(report.mistake_rate));
  });
}

/** playerStrengthMaxMove. */
function maxMove(report: StrengthReport): number {
  return report.overall.samples.reduce((m, s) => Math.max(m, s.move_number), 1);
}

/** playerStrengthMoveToX. */
function moveToX(move: number, x: number, w: number, max: number): number {
  if (max <= 1) return x;
  return x + Math.round(clamp((move - 1) / (max - 1), 0, 1) * Math.max(0, w - 1));
}

/** playerStrengthAxisTickStep (the dashboard's hit map). */
function dashboardTickStep(max: number, w: number): number {
  const desired = Math.max(3, Math.min(7, i(w / 58)));
  const raw = Math.max(1, Math.ceil(max / desired));
  if (raw <= 5) return 5;
  if (raw <= 10) return 10;
  if (raw <= 20) return 20;
  if (raw <= 50) return 50;
  return i((raw + 49) / 50) * 50;
}

const bySortedMove = (samples: StrengthSample[]) => [...samples].sort((a, b) => a.move_number - b.move_number);

/** The two Swing tooltips as plain lines (the HTML's <br> breaks). */
function setTooltip(canvas: HTMLCanvasElement, tip: string | null): void {
  if (tip === null) canvas.removeAttribute("title");
  else if (canvas.title !== tip) canvas.title = tip;
}

type Lane = "first" | "good";
interface HitPoint { sample: StrengthSample; black: boolean; lane: Lane; x: number; bounds: { x: number; y: number; w: number; h: number }; tooltip: string }

/** PlayerStrengthMoveHitMapPanel: 选点命中图 with hover outline, painted hover box and tooltip. */
export function moveHitMap(report: StrengthReport): Painted {
  const MATCH_CHART_BACKGROUND = rgb(44, 51, 65);
  const MATCH_BLACK_PANEL = rgb(84, 68, 78);
  const MATCH_WHITE_PANEL = rgb(68, 82, 108);
  const MATCH_TRACK = rgb(58, 64, 78);
  const MATCH_GRID = rgb(255, 255, 255, 38);
  const MATCH_AXIS = rgb(120, 130, 146);
  const MATCH_TEXT = rgb(248, 248, 242);
  const MATCH_MUTED = rgb(214, 218, 225);
  const BLACK_HIT = [255, 86, 92];
  const BLACK_GOOD = [255, 122, 124];
  const WHITE_HIT = [96, 162, 255];
  const WHITE_GOOD = [126, 184, 255];
  let hitPoints: HitPoint[] = [];
  let hovered: HitPoint | null = null;
  let mouse: { x: number; y: number } | null = null;

  const laneLabel = (sample: StrengthSample, lane: Lane) =>
    sample.first_choice || lane === "first" ? t("PlayerStrengthEstimate.match.firstChoice") : t("PlayerStrengthEstimate.match.goodMove");
  const tooltip = (sample: StrengthSample, black: boolean, lane: Lane) => {
    // buildTooltipHtml: aiRank < 0 is never produced, so a move outside the candidates reads
    // "AI第1000选" like Java's ADDITIONAL_MOVE_ORDER 999.
    const aiChoice = sample.first_choice
      ? t("PlayerStrengthEstimate.match.firstChoice")
      : javaFormat(t("PlayerStrengthEstimate.match.tooltipAiRank"), sample.ai_rank + 1);
    const scoreLoss = sample.score_loss === null ? t("PlayerStrengthEstimate.match.tooltipUnknown") : javaFixed(sample.score_loss, 1);
    return [
      `${sideName(black)} ${javaFormat(t("PlayerStrengthEstimate.match.tooltipMove"), sample.move_number)}`,
      laneLabel(sample, lane),
      `${t("PlayerStrengthEstimate.match.tooltipAiChoice")}: ${aiChoice}`,
      `${t("PlayerStrengthEstimate.match.tooltipWinrateLoss")}: ${javaFixed(sample.winrate_loss, 1)}%`,
      `${t("PlayerStrengthEstimate.match.tooltipScoreLoss")}: ${scoreLoss}`,
      `${t("PlayerStrengthEstimate.match.tooltipComplexity")}: ${javaFixed(clamp(sample.complexity, 0, 1) * 100, 0)}`,
    ].join("\n");
  };
  const isHovered = (p: HitPoint) => !!hovered && hovered.sample === p.sample && hovered.black === p.black && hovered.lane === p.lane;

  const panel = painted("ps-hit-map", (g, w, h) => {
    hitPoints = [];
    roundedCard(g, 0, 0, w, h);
    text(g, t("PlayerStrengthEstimate.match.moveMap"), 30, 40, DASH.TEXT, true, ff() + 4);
    fittedText(g, t("PlayerStrengthEstimate.match.moveMapHint"), 30, 62, Math.max(120, w - 60), DASH.MUTED_TEXT, false, Math.max(10, ff() - 1));
    // drawLegend.
    const lx = Math.max(230, w - 160);
    setFont(g, false, Math.max(10, ff() - 1));
    fillRect(g, lx, 38 - 9, 14, 8, rgb(BLACK_HIT[0], BLACK_HIT[1], BLACK_HIT[2]));
    g.fillStyle = DASH.TEXT;
    g.fillText(t("Menu.Black"), lx + 19, 38);
    fillRect(g, lx + 66, 38 - 9, 14, 8, rgb(WHITE_HIT[0], WHITE_HIT[1], WHITE_HIT[2]));
    g.fillStyle = DASH.TEXT;
    g.fillText(t("Menu.White"), lx + 85, 38);

    const chartLeft = 30;
    const chartTop = 88;
    const chartTotalWidth = Math.max(220, w - 60);
    const statsWidth = Math.max(128, Math.min(156, i(chartTotalWidth / 4)));
    const chartX = chartLeft + 54;
    const chartWidth = Math.max(180, chartTotalWidth - 54 - statsWidth - 12);
    const sectionGap = 5;
    const sectionHeight = Math.max(58, i((h - chartTop - 34 - sectionGap) / 2));
    const max = maxMove(report);
    if (report.black.samples.length === 0 && report.white.samples.length === 0) {
      text(g, t("PlayerStrengthEstimate.match.noMoveSamples"), 32, 120, DASH.MUTED_TEXT, false, ff() + 2);
      return;
    }
    const blackTop = chartTop;
    const whiteTop = blackTop + sectionHeight + sectionGap;
    const panelWidth = chartWidth + statsWidth + 12;
    const statsX = chartX + chartWidth + 24;
    fillRound(g, chartLeft, blackTop - 8, chartTotalWidth, sectionHeight * 2 + sectionGap + 34, 18, MATCH_CHART_BACKGROUND);
    section(g, report.black, chartX, blackTop, chartWidth, panelWidth, sectionHeight, max, true);
    section(g, report.white, chartX, whiteTop, chartWidth, panelWidth, sectionHeight, max, false);
    axis(g, chartX, whiteTop + sectionHeight + 16, chartWidth, max);
    stats(g, report.black.summary, statsX, blackTop, sectionHeight, true);
    stats(g, report.white.summary, statsX, whiteTop, sectionHeight, false);
    hoverBox(g, w, h);
  });

  function axis(g: G, x: number, y: number, w: number, max: number): void {
    setFont(g, false, Math.max(10, ff() - 1));
    line(g, x, y - 10, x + w, y - 10, MATCH_AXIS);
    g.fillStyle = MATCH_AXIS;
    const step = dashboardTickStep(max, w);
    const endX = moveToX(max, x, w, max);
    let lastLabelX = Number.MIN_SAFE_INTEGER;
    for (let move = 1; move <= max; move += step) {
      const tickX = moveToX(move, x, w, max);
      if (move !== 1 && move !== max && endX - tickX < 42) continue;
      line(g, tickX, y - 14, tickX, y - 7, MATCH_AXIS);
      const label = String(move);
      g.fillStyle = MATCH_AXIS;
      g.fillText(label, tickX - i(width(g, label) / 2), y + 5);
      lastLabelX = tickX;
    }
    const endLabel = String(max);
    if (max > 1 && endX - lastLabelX >= 42) {
      line(g, endX, y - 14, endX, y - 7, MATCH_AXIS);
      g.fillStyle = MATCH_AXIS;
      g.fillText(endLabel, endX - i(width(g, endLabel) / 2), y + 5);
    }
  }

  function section(g: G, side: StrengthSideReport, x: number, top: number, w: number, panelWidth: number, h: number, max: number, black: boolean): void {
    fillRound(g, x - 12, top, panelWidth, h, 8, black ? MATCH_BLACK_PANEL : MATCH_WHITE_PANEL);
    // drawMoveGrid.
    const step = dashboardTickStep(max, w);
    for (let move = 1; move <= max; move += step) {
      const tickX = moveToX(move, x, w, max);
      line(g, tickX, top, tickX, top + h, MATCH_GRID);
    }
    line(g, x, top + h - 1, x + w, top + h - 1, rgb(255, 255, 255, 55));
    const laneHeight = Math.max(7, Math.min(9, i(h / 8)));
    const firstY = top + 18;
    const goodY = top + Math.max(38, i(h / 2) + 3);
    fillRect(g, x, firstY, w, laneHeight, MATCH_TRACK);
    fillRect(g, x, goodY, w, laneHeight, MATCH_TRACK);
    setFont(g, false, Math.max(10, ff() - 1));
    g.fillStyle = MATCH_MUTED;
    const firstLabel = t("PlayerStrengthEstimate.match.firstChoice");
    const goodLabel = t("PlayerStrengthEstimate.match.goodMove");
    g.fillText(firstLabel, x - 12 - width(g, firstLabel), firstY + laneHeight);
    g.fillText(goodLabel, x - 12 - width(g, goodLabel), goodY + laneHeight);
    if (side.samples.length === 0) return;
    const blockWidth = Math.max(3, Math.min(8, i(w / Math.max(1, max))));
    for (const sample of bySortedMove(side.samples)) {
      const good = isGoodMove(sample.move_rank);
      if (!sample.first_choice && !good) continue;
      const blockX = moveToX(sample.move_number, x, w, max);
      for (const lane of ["first", "good"] as const) {
        if (lane === "first" ? !sample.first_choice : !good) continue;
        const laneY = lane === "first" ? firstY : goodY;
        const point: HitPoint = {
          sample, black, lane, x: blockX,
          bounds: { x: blockX - 3, y: laneY - 7, w: Math.max(8, blockWidth + 6), h: laneHeight + 14 },
          tooltip: tooltip(sample, black, lane),
        };
        hitPoints.push(point);
        const hover = isHovered(point);
        const base = lane === "first" ? (black ? BLACK_HIT : WHITE_HIT) : black ? BLACK_GOOD : WHITE_GOOD;
        const [r, gg, b] = hover ? brighter(base) : base;
        fillRect(g, blockX, laneY, Math.max(3, blockWidth), laneHeight, rgb(r, gg, b));
        if (hover) drawRound(g, point.bounds.x, point.bounds.y, point.bounds.w, point.bounds.h, 8, rgb(255, 255, 255, 220));
      }
    }
  }

  function stats(g: G, report: StrengthSummary, x: number, top: number, h: number, black: boolean): void {
    const stoneSize = Math.max(20, Math.min(24, i(h / 3)));
    const stoneY = top + Math.max(6, i((h - stoneSize) / 2));
    const textX = x + stoneSize + 9;
    const firstBaseline = top + Math.max(27, i(h / 2) - 2);
    const goodBaseline = Math.min(top + h - 9, firstBaseline + 18);
    generatedStone(g, black, x, stoneY, stoneSize, 0.95);
    setFont(g, false, Math.max(10, ff()));
    g.fillStyle = MATCH_TEXT;
    g.fillText(`${t("PlayerStrengthEstimate.match.firstChoice")} ${percentText(report.first_choice_rate)}`, textX, firstBaseline);
    g.fillText(`${t("PlayerStrengthEstimate.match.goodMove")} ${percentText(report.move_rank_good_move_rate)}`, textX, goodBaseline);
  }

  function hoverBox(g: G, w: number, h: number): void {
    if (!hovered || !mouse) return;
    const sample = hovered.sample;
    const title = `${sideName(hovered.black)} ${javaFormat(t("PlayerStrengthEstimate.match.tooltipMove"), sample.move_number)}`;
    const detail = `${laneLabel(sample, hovered.lane)}  ${t("PlayerStrengthEstimate.match.tooltipWinrateLoss")} ${javaFixed(sample.winrate_loss, 1)}%`;
    setFont(g, true, Math.max(11, ff()));
    const titleMetrics = metrics(g);
    const titleWidth = width(g, title);
    setFont(g, false, Math.max(10, ff() - 1));
    const detailMetrics = metrics(g);
    const boxWidth = Math.max(titleWidth, width(g, detail)) + 28;
    const boxHeight = titleMetrics.height + detailMetrics.height + 18;
    const x = Math.min(Math.max(12, mouse.x + 16), Math.max(12, w - boxWidth - 12));
    let y = mouse.y - boxHeight - 14;
    if (y < 8) y = Math.min(h - boxHeight - 8, mouse.y + 18);
    fillRound(g, x, y, boxWidth, boxHeight, 14, rgb(41, 36, 27, 235));
    drawRound(g, x, y, boxWidth, boxHeight, 14, rgb(255, 245, 221, 180));
    text(g, title, x + 14, y + 14 + titleMetrics.ascent, rgb(255, 248, 235), true, Math.max(11, ff()));
    text(g, detail, x + 14, y + 15 + titleMetrics.height + detailMetrics.ascent, rgb(231, 217, 192), false, Math.max(10, ff() - 1));
  }

  /** findHitPoint: the nearest centre among the hit bounds containing the point. */
  function find(p: { x: number; y: number } | null): HitPoint | null {
    if (!p) return null;
    let best: HitPoint | null = null;
    let bestDistance = Infinity;
    for (const hp of hitPoints) {
      const b = hp.bounds;
      if (p.x < b.x || p.y < b.y || p.x >= b.x + b.w || p.y >= b.y + b.h) continue;
      const d = (p.x - (b.x + b.w / 2)) ** 2 + (p.y - (b.y + b.h / 2)) ** 2;
      if (d < bestDistance) {
        bestDistance = d;
        best = hp;
      }
    }
    return best;
  }

  const update = (p: { x: number; y: number } | null) => {
    const next = find(p);
    if (hovered === next && mouse?.x === p?.x && mouse?.y === p?.y) return;
    hovered = next;
    mouse = p;
    panel.element.style.cursor = hovered ? "pointer" : "";
    setTooltip(panel.element, hovered?.tooltip ?? null);
    panel.repaint();
  };
  panel.element.addEventListener("mousemove", (e) => update({ x: e.offsetX, y: e.offsetY }));
  panel.element.addEventListener("mouseleave", () => update(null));
  return panel;
}

/** PlayerStrengthPerformanceRankPanel: 发挥水准分布, the six move-rank rows per side. */
export function performancePanel(report: StrengthReport): Painted {
  const MUTED = DASH.MUTED_TEXT;
  const COUNT_TEXT = rgb(73, 59, 41);
  const countX = (x: number, w: number) => x + Math.max(112, i((w * 38) / 100));
  const barX = (x: number, w: number) => x + Math.max(160, i((w * 56) / 100));
  const percentX = (x: number, w: number) => x + w - 58;

  return painted("ps-performance", (g, w, h) => {
    roundedCard(g, 0, 0, w, h);
    text(g, t("PlayerStrengthEstimate.performance.title"), 30, 42, DASH.TEXT, true, ff() + 5);
    fittedText(g, t("PlayerStrengthEstimate.performance.hint"), 30, 66, Math.max(120, w - 60), DASH.MUTED_TEXT, false, Math.max(10, ff() - 1));
    if (!hasSamples(report.black.summary) && !hasSamples(report.white.summary)) {
      fittedText(g, t("PlayerStrengthEstimate.performance.noMoveSamples"), 30, 132, Math.max(120, w - 60), MUTED, false, ff() + 1);
      return;
    }
    distributionCard(g, 26, 92, Math.max(280, w - 52), Math.max(214, h - 92 - 24));
  });

  function distributionCard(g: G, x: number, y: number, w: number, h: number): void {
    fillRound(g, x + 2, y + 4, w - 4, h - 2, 18, rgb(74, 51, 26, 18));
    fillRound(g, x, y, w, h, 18, rgb(255, 252, 244, 236));
    drawRound(g, x, y, w - 1, h - 1, 18, rgb(208, 183, 142, 120));
    const center = x + i(w / 2);
    g.save();
    g.setLineDash([5, 5]);
    g.lineCap = "round";
    g.lineWidth = 1.1;
    g.strokeStyle = rgb(219, 190, 143, 115);
    g.beginPath();
    g.moveTo(center + 0.5, y + 34);
    g.lineTo(center + 0.5, y + h - 28);
    g.stroke();
    g.restore();
    const gutter = Math.max(24, Math.min(42, i(w / 24)));
    const sideWidth = Math.max(240, i((w - gutter * 3) / 2));
    side(g, true, report.black, x + gutter, y + 34, sideWidth, h - 58);
    side(g, false, report.white, center + gutter, y + 34, sideWidth, h - 58);
  }

  function side(g: G, black: boolean, s: StrengthSideReport, x: number, y: number, w: number, h: number): void {
    const stoneSize = Math.max(28, Math.min(38, i(h / 8)));
    generatedStone(g, black, x, y - 6, stoneSize, 0.96);
    fittedText(g, sideName(black), x + stoneSize + 14, y + 18, Math.max(80, w - stoneSize - 118), DASH.TEXT, true, ff() + 7);
    // drawSamplePill.
    const px = x + w - 96;
    const py = y - 4;
    fillRound(g, px, py, 96, 28, 28, rgb(246, 238, 219, 220));
    drawRound(g, px, py, 95, 27, 28, rgb(205, 180, 139, 130));
    fittedText(g, javaFormat(t("PlayerStrengthEstimate.performance.sideMoves"), s.summary.sample_count), px + 10, py + 14 + i(ff() / 2) - 1, 76, COUNT_TEXT, true, Math.max(10, ff() - 1));
    const headerY = y + 58;
    fittedText(g, t("PlayerStrengthEstimate.performance.count"), countX(x, w) - 8, headerY, 58, MUTED, false, Math.max(10, ff() - 1));
    fittedText(g, t("PlayerStrengthEstimate.performance.percent"), percentX(x, w), headerY, 62, MUTED, false, Math.max(10, ff() - 1));
    // performanceRankCounts: a first choice counts as 最佳, any other 最佳 as 好手.
    const counts = MOVE_RANKS.map(() => 0);
    for (const sample of s.samples) {
      const rank = sample.first_choice ? "Best" : sample.move_rank === "Best" ? "Good" : sample.move_rank;
      counts[MOVE_RANKS.indexOf(rank)]++;
    }
    const sampleCount = Math.max(0, s.summary.sample_count);
    const rowsTop = headerY + 22;
    const rowHeight = Math.max(32, Math.min(52, i((h - 80) / MOVE_RANKS.length)));
    const morandi = state.view?.settings.appearance.morandiColors ?? false;
    MOVE_RANKS.forEach((rank, k) => {
      const fraction = sampleCount <= 0 ? 0 : counts[k] / sampleCount;
      row(g, moveRankName(rank), counts[k], fraction, components(rankColor(rank, morandi)), x, rowsTop + k * rowHeight, w, rowHeight);
    });
  }

  function row(g: G, label: string, count: number, fraction: number, color: number[], x: number, y: number, w: number, h: number): void {
    const [r, gg, b] = color;
    const solid = rgb(r, gg, b);
    const bullet = Math.max(10, Math.min(15, i(h / 3)));
    const centerY = y + i(h / 2);
    const labelX = x + bullet + 18;
    const cX = countX(x, w);
    const pX = percentX(x, w);
    const bX = barX(x, w);
    const barWidth = Math.max(34, pX - bX - 10);
    const baseline = centerY + i(ff() / 2) - 1;
    fillRound(g, x - 8, y + 3, w + 16, h - 6, 14, rgb(r, gg, b, 34));
    fillOval(g, x, centerY - i(bullet / 2), bullet, bullet, solid);
    const shine = Math.max(3, i(bullet / 3));
    fillOval(g, x + i(bullet / 5), centerY - i(bullet / 2) + i(bullet / 6), shine, shine, rgb(255, 255, 255, 110));
    fittedText(g, label, labelX, baseline, Math.max(42, cX - labelX - 14), DASH.TEXT, true, Math.max(11, ff() + 1));
    fittedText(g, String(count), cX, baseline, 44, COUNT_TEXT, true, Math.max(11, ff()));
    // drawProgressTrack.
    const ty = centerY - 6;
    fillRound(g, bX, ty, barWidth, 12, 12, rgb(226, 220, 206, 210));
    drawRound(g, bX, ty, barWidth - 1, 11, 12, rgb(200, 186, 160, 120));
    const fillWidth = fraction <= 0 ? 0 : Math.max(12, Math.round(barWidth * clamp(fraction, 0, 1)));
    if (fillWidth > 0) {
      const light = rgb(Math.min(255, r + 22), Math.min(255, gg + 28), Math.min(255, b + 22));
      fillRound(g, bX, ty, Math.min(barWidth, fillWidth), 12, 12, gradient(g, bX, ty, light, bX + fillWidth, ty + 12, solid));
    }
    fittedText(g, `${javaFixed(fraction * 100, 1)}%`, pX, baseline, 66, COUNT_TEXT, false, Math.max(10, ff()));
  }
}

/** PlayerStrengthNoteStrip (dashboard, 64 px) and PlayerStrengthDetailNoteStrip (detail, 62 px). */
export function noteStrip(note: string, detail: boolean): Painted {
  return painted(detail ? "ps-detail-note" : "ps-note", (g, w, h) => {
    if (detail) {
      fillRound(g, 0, 5, w, h - 10, 18, rgb(34, 43, 58, 232));
      drawRound(g, 0, 5, w - 1, h - 11, 18, rgb(100, 116, 139, 180));
    } else {
      fillRound(g, 0, 4, w, h - 8, 18, rgb(248, 240, 224, 210));
      drawRound(g, 0, 4, w - 1, h - 9, 18, rgb(202, 184, 151));
    }
    infoBadge(g, 18, i(h / 2) - 14, 28, 28, 0.95);
    fittedText(g, note, 58, i(h / 2) + 6, Math.max(120, w - 78), detail ? DETAIL.MUTED_TEXT : DASH.ACCENT_DARK, false, ff() + 1);
  });
}

/** PlayerStrengthMatchChart (detail dialog): lanes, 段位 windows of 7d+ segments with their hover
 *  box and tooltip, the 疑似AI intervals and the side figures. */
export function matchChart(report: StrengthReport): Painted {
  const BLACK_PANEL = rgb(48, 38, 48);
  const WHITE_PANEL = rgb(34, 48, 68);
  const RANK_TEXT = rgb(12, 36, 25);
  let hoverText: string | null = null;
  let hoverPoint: { x: number; y: number } | null = null;

  const geometry = (w: number, h: number) => {
    const chartX = 10 + 48;
    const sectionHeight = Math.max(54, i((h - 8 - 18 - 3) / 2));
    return { chartX, chartWidth: Math.max(120, w - chartX - 140 - 10), max: maxMove(report), top: 8, sectionHeight };
  };
  /** axisTickStep of the detail chart: 1, 2, 5 or 10 times a power of ten. */
  const tickStep = (max: number, chartWidth: number) => {
    const rough = Math.max(1, Math.max(1, max - 1) / Math.max(3, i(chartWidth / 90)));
    let magnitude = 1;
    while (magnitude * 10 < rough) magnitude *= 10;
    for (const factor of [1, 2, 5, 10]) if (factor * magnitude >= rough) return factor * magnitude;
    return magnitude * 10;
  };
  const shown = (side: StrengthSideReport) =>
    side.segments.filter((s) => s.summary.sample_count >= side.minimum_segment_samples && highestDanLevel(s.summary.strength_band) >= 7);
  const windowBounds = (first: number, last: number, chartX: number, y: number, chartWidth: number, max: number) => {
    const x1 = moveToX(first, chartX, chartWidth, max);
    const x2 = moveToX(last + 1, chartX, chartWidth, max);
    return { x: x1 + 1, y, w: Math.max(20, x2 - x1 - 2), h: 15 };
  };

  /** rankTooltipAt: the black section's windows first. */
  const tooltipAt = (p: { x: number; y: number }, w: number, h: number): string | null => {
    const { chartX, chartWidth, max, top, sectionHeight } = geometry(w, h);
    for (const [side, black, sideTop] of [[report.black, true, top], [report.white, false, top + sectionHeight + 3]] as const) {
      for (const segment of shown(side)) {
        const b = windowBounds(segment.first_move, segment.last_move, chartX, sideTop + 25, chartWidth, max);
        // Rectangle.grow(6, 10).contains.
        if (p.x < b.x - 6 || p.y < b.y - 10 || p.x >= b.x + b.w + 6 || p.y >= b.y + b.h + 10) continue;
        return javaFormat(t("PlayerStrengthEstimate.match.rankTooltip"), sideName(black), segment.first_move, segment.last_move, strengthLabel(segment.summary));
      }
    }
    return null;
  };

  const chart = painted("ps-match-chart", (g, w, h) => {
    fillRound(g, 0, 0, w - 1, h - 1, 24, gradient(g, 0, 0, DETAIL.CARD_SOFT, 0, Math.max(1, h), DETAIL.CARD));
    drawRound(g, 0, 0, w - 1, h - 1, 24, DETAIL.CARD_BORDER);
    const { chartX, chartWidth, max, top, sectionHeight } = geometry(w, h);
    side(g, report.black, true, chartX, top, chartWidth, sectionHeight, max);
    side(g, report.white, false, chartX, top + sectionHeight + 3, chartWidth, sectionHeight, max);
    axis(g, chartX, top + sectionHeight * 2 + 3 + 14, chartWidth, max);
    hoverBox(g, w, h);
  });

  function centered(g: G, s: string, x1: number, x2: number, y: number): void {
    g.fillText(s, x1 + Math.max(0, i((x2 - x1 - width(g, s)) / 2)), y);
  }

  function stat(g: G, x: number, y: number, label: string, value: string): void {
    setFont(g, false, ff());
    g.fillStyle = DETAIL.MUTED_TEXT;
    g.fillText(`${label} `, x, y);
    const labelWidth = width(g, `${label} `);
    setFont(g, true, ff() + 1);
    g.fillStyle = DETAIL.TEXT;
    g.fillText(value, x + labelWidth, y);
  }

  function side(g: G, s: StrengthSideReport, black: boolean, chartX: number, top: number, chartWidth: number, sectionHeight: number, max: number): void {
    const statsX = chartX + chartWidth + 36;
    const firstY = top + 16;
    const goodY = top + 33;
    const rankY = top + 25;
    const aiY = top + sectionHeight - 15;
    const laneHeight = 7;
    const blockWidth = Math.max(3, Math.min(8, i(chartWidth / Math.max(max, 1))));
    fillRound(g, chartX - 12, top, chartWidth + 12 + 130, sectionHeight, 16, black ? BLACK_PANEL : WHITE_PANEL);
    drawRound(g, chartX - 12, top, chartWidth + 12 + 130, sectionHeight, 16, rgb(255, 255, 255, 28));
    // drawMoveGrid.
    const step = tickStep(max, chartWidth);
    for (let move = 1; move <= max; move += step) {
      const x = moveToX(move, chartX, chartWidth, max);
      line(g, x, top, x, top + sectionHeight, rgb(255, 255, 255, 35));
    }
    line(g, chartX, top + sectionHeight - 1, chartX + chartWidth, top + sectionHeight - 1, rgb(255, 255, 255, 60));
    // drawStoneMarker.
    const mx = chartX + chartWidth + 8;
    const my = firstY - 18;
    fillOval(g, mx, my, 22, 22, black ? rgb(20, 22, 26) : rgb(244, 244, 236));
    drawRound(g, mx, my, 22, 22, 22, black ? rgb(255, 255, 255, 95) : rgb(30, 34, 42, 170));
    fillOval(g, mx + 3, my + 3, 7, 7, black ? rgb(255, 255, 255, 45) : rgb(255, 255, 255, 180));
    fillRect(g, chartX, firstY, chartWidth, laneHeight, DETAIL.TRACK);
    fillRect(g, chartX, goodY, chartWidth, laneHeight, DETAIL.TRACK);
    for (const sample of bySortedMove(s.samples)) {
      const x = moveToX(sample.move_number, chartX, chartWidth, max);
      if (sample.first_choice) fillRect(g, x, firstY, blockWidth, laneHeight, black ? rgb(...DETAIL.BLACK_ACCENT) : rgb(...DETAIL.WHITE_ACCENT));
      if (isGoodMove(sample.move_rank)) fillRect(g, x, goodY, blockWidth, laneHeight, black ? DETAIL.BLACK_ACCENT_SOFT : DETAIL.WHITE_ACCENT_SOFT);
    }
    // drawRankWindows.
    for (const segment of shown(s)) {
      const b = windowBounds(segment.first_move, segment.last_move, chartX, rankY, chartWidth, max);
      fillRound(g, b.x, b.y, b.w, b.h, 4, DETAIL.GREEN);
      if (hoverPoint) {
        const p = hoverPoint;
        if (p.x >= b.x - 6 && p.y >= b.y - 10 && p.x < b.x + b.w + 6 && p.y < b.y + b.h + 10) drawRound(g, b.x - 1, b.y - 1, b.w + 1, b.h + 1, 5, rgb(255, 255, 255, 220));
      }
      if (b.w >= 12) {
        g.save();
        g.beginPath();
        g.rect(b.x + 2, b.y, Math.max(1, b.w - 4), b.h);
        g.clip();
        setFont(g, true, Math.max(10, ff()));
        g.fillStyle = RANK_TEXT;
        centered(g, strengthLabel(segment.summary), b.x, b.x + b.w, rankY + 12);
        g.restore();
      }
    }
    // drawAiIntervals.
    let suspect = false;
    g.strokeStyle = DETAIL.GOLD;
    g.lineWidth = 1;
    g.strokeRect(chartX + 0.5, aiY + 0.5, chartWidth, 15);
    for (const segment of s.segments) {
      if (segment.summary.sample_count < s.minimum_segment_samples || aiLikelihood(segment.summary) < 0.7) continue;
      suspect = true;
      const x1 = moveToX(segment.first_move, chartX, chartWidth, max);
      const x2 = moveToX(segment.last_move + 1, chartX, chartWidth, max);
      fillRect(g, x1 + 1, aiY + 1, Math.max(4, x2 - x1 - 2), 13, DETAIL.GOLD);
    }
    if (!suspect) {
      setFont(g, true, Math.max(10, ff()));
      g.fillStyle = DETAIL.TEXT;
      centered(g, t("PlayerStrengthEstimate.match.noAiInterval"), chartX, chartX + chartWidth, aiY + 12);
    }
    stat(g, statsX, firstY + laneHeight, t("PlayerStrengthEstimate.match.firstChoice"), `${javaFixed(s.summary.first_choice_rate * 100, 1)}%`);
    stat(g, statsX, goodY + laneHeight, t("PlayerStrengthEstimate.match.goodMove"), `${javaFixed(s.summary.move_rank_good_move_rate * 100, 1)}%`);
    stat(g, statsX, aiY + laneHeight + 2, t("PlayerStrengthEstimate.match.aiLike"), `${javaFixed(aiLikelihood(s.summary) * 100, 1)}%`);
    // The lane captions use the stats' last font (BOLD frameFontSize + 1 is restored to the
    // section font set before, PLAIN frameFontSize).
    setFont(g, false, ff());
    g.fillStyle = DETAIL.MUTED_TEXT;
    const firstLabel = t("PlayerStrengthEstimate.match.firstChoice");
    const goodLabel = t("PlayerStrengthEstimate.match.goodMove");
    g.fillText(firstLabel, chartX - 2 - width(g, firstLabel), firstY + laneHeight);
    g.fillText(goodLabel, chartX - 2 - width(g, goodLabel), goodY + laneHeight);
  }

  function axis(g: G, chartX: number, y: number, chartWidth: number, max: number): void {
    setFont(g, false, ff());
    line(g, chartX, y - 10, chartX + chartWidth, y - 10, DETAIL.GRID);
    const step = tickStep(max, chartWidth);
    for (let move = 1; move <= max; move += step) {
      const x = moveToX(move, chartX, chartWidth, max);
      line(g, x, y - 14, x, y - 8, DETAIL.GRID);
      g.fillStyle = DETAIL.GRID;
      g.fillText(String(move), x - i(width(g, String(move)) / 2), y + 5);
    }
    const endX = moveToX(max, chartX, chartWidth, max);
    if (max > 1 && (max - 1) % step !== 0) {
      g.fillStyle = DETAIL.GRID;
      g.fillText(String(max), endX - i(width(g, String(max)) / 2), y + 5);
    }
  }

  function hoverBox(g: G, w: number, h: number): void {
    if (!hoverText || !hoverPoint) return;
    setFont(g, true, Math.max(11, ff()));
    const m = metrics(g);
    const boxWidth = width(g, hoverText) + 18;
    const boxHeight = m.height + 10;
    const x = Math.min(Math.max(10, hoverPoint.x + 12), Math.max(10, w - boxWidth - 10));
    let y = hoverPoint.y - boxHeight - 10;
    if (y < 8) y = Math.min(h - boxHeight - 8, hoverPoint.y + 14);
    fillRound(g, x, y, boxWidth, boxHeight, 8, rgb(28, 34, 44, 238));
    drawRound(g, x, y, boxWidth, boxHeight, 8, rgb(255, 255, 255, 180));
    g.fillStyle = DETAIL.TEXT;
    g.fillText(hoverText, x + 9, y + 5 + m.ascent);
  }

  const update = (p: { x: number; y: number } | null) => {
    const next = p ? tooltipAt(p, chart.width, chart.height) : null;
    if (hoverText === next && hoverPoint?.x === p?.x && hoverPoint?.y === p?.y) return;
    hoverText = next;
    hoverPoint = p;
    chart.element.style.cursor = next ? "pointer" : "";
    setTooltip(chart.element, next);
    chart.repaint();
  };
  chart.element.addEventListener("mousemove", (e) => update({ x: e.offsetX, y: e.offsetY }));
  chart.element.addEventListener("mouseleave", () => update(null));
  return chart;
}
