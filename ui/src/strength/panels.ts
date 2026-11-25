// The panels of the 棋力评估 dashboard and its detail dialog (LizzieFrame.java nested classes
// 18335-20597): each shows what its Java paintComponent shows. Figures, names and bars are
// document content (strength.css); the two move charts paint on a canvas from the tokens (paint.ts)
// with Java's hit targets, hover states and tooltips.
import type { MoveRank, StrengthReport, StrengthSample, StrengthSideReport, StrengthSummary } from "../contract/generated";
import { t } from "../i18n";
import {
  aiLikelihood, averageScoreLossText, confidenceText, hasSamples, highestDanLevel, isGoodMove, javaFixed, javaFormat, MOVE_RANKS,
  moveRankName, percentText, rankScaleRows, rankScaleSelectedIndex, rankScalePlainText, rankScaleTooltip, sideRankText, strengthLabel,
  weightedScoreLossText,
} from "./format";
import { ff, fillRound, line, metrics, painted, palette, setFont, stone, strokeRound, text, width, type G, type Painted } from "./paint";

const i = Math.trunc;
const clamp = (v: number, min: number, max: number) => Math.max(min, Math.min(max, v));
const sideName = (black: boolean) => (black ? t("Menu.Black") : t("Menu.White"));

export function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", content = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (content) e.textContent = content;
  return e;
}

/** The side's flat stone of the stone pair (黑白棋子对). */
export function stoneMark(black: boolean): HTMLElement {
  const s = el("span", `ps-stone ${black ? "black" : "white"}`);
  s.setAttribute("aria-hidden", "true");
  return s;
}

/** A section's heading: the side's stone and its title. */
export function sideHeading(black: boolean, title: string): HTMLElement {
  const head = el("div", "ps-side-head");
  head.append(stoneMark(black), el("h3", "", title));
  return head;
}

/** A track with its fill at `fraction`; `kind` names the fill's colour (strength.css). */
function meter(fraction: number, kind: string): HTMLElement {
  const track = el("span", "ps-meter");
  const fill = el("span", kind);
  fill.style.width = `${Math.round(clamp(fraction, 0, 1) * 1000) / 10}%`;
  track.append(fill);
  return track;
}

/** The texts an assessment section shows (acceptance probe): rank, the confidence value and the
 *  five metric values in order. */
export interface CardTexts {
  rank: string;
  confidence: string;
  metrics: string[];
}

/** PlayerStrengthAssessmentCard: the side, its rank and 可信度, five metric rows with their bars
 *  and the 段位标尺 scale. The tooltip and accessible texts are Java's. */
export function assessmentSection(black: boolean, side: StrengthSideReport, humanSl: boolean): { element: HTMLElement; drawn: CardTexts } {
  const report = side.summary;
  const drawn: CardTexts = { rank: sideRankText(report), confidence: confidenceText(report.confidence), metrics: [] };
  const section = el("section", "ps-side");
  section.title = rankScaleTooltip(side);
  section.setAttribute("aria-label", `${sideName(black)} ${t("PlayerStrengthEstimate.title")}`);
  section.setAttribute("aria-description", rankScalePlainText(side));

  const figure = el("div", "ps-figure");
  figure.append(el("span", "ps-headline", drawn.rank), el("span", "ps-label", `${t("PlayerStrengthEstimate.confidence")}: ${drawn.confidence}`));

  const rows = el("div", "ps-metrics");
  const moves = t("PlayerStrengthEstimate.moves");
  const metric = (label: string, value: string, fraction: number, kind: string) => {
    // drawMetricRow: an empty HumanSL 40 side shows "-" and empty bars except for 手数.
    if (humanSl && !hasSamples(report) && label !== moves) {
      value = "-";
      fraction = 0;
    }
    drawn.metrics.push(value);
    const row = el("div", "ps-metric");
    row.append(el("span", "ps-label", label), el("span", "ps-value", value), meter(fraction, kind));
    rows.append(row);
  };
  metric(moves, String(report.sample_count), clamp(report.sample_count / 240, 0, 1), "ps-fill-rate");
  metric(t("PlayerStrengthEstimate.firstChoice"), percentText(report.first_choice_rate), report.first_choice_rate, "ps-fill-rate");
  metric(t("PlayerStrengthEstimate.goodMoveRate"), percentText(report.move_rank_good_move_rate), report.move_rank_good_move_rate, "ps-fill-rate");
  metric(t("PlayerStrengthEstimate.avgScoreLoss"), averageScoreLossText(report), 1 - clamp(report.average_score_equivalent_loss / 8, 0, 1), "ps-fill-loss");
  metric(t("PlayerStrengthEstimate.weightedScoreLoss"), weightedScoreLossText(report), 1 - clamp(report.weighted_score_loss / 8, 0, 1), "ps-fill-loss");

  // The scale with the side's band marked.
  const scale = el("div", "ps-scale");
  scale.append(el("div", "ps-label", t("PlayerStrengthEstimate.rank.scale")));
  const selected = humanSl && !hasSamples(report) ? -1 : rankScaleSelectedIndex(report.rank_value);
  rankScaleRows().forEach(([key, name], k) => {
    const row = el("div", k === selected ? "ps-scale-row selected" : "ps-scale-row");
    row.append(el("span", "ps-scale-dot"), el("span", "ps-scale-key", key), el("span", "", name));
    scale.append(row);
  });

  const body = el("div", "ps-side-body");
  body.append(rows, scale);
  section.append(sideHeading(black, sideName(black) + t("PlayerStrengthEstimate.performanceSuffix")), figure, body);
  return { element: section, drawn };
}

/** PlayerStrengthMatchSummaryCard: the 吻合度 ring with its figure and four small figures. */
export function matchSummary(black: boolean, report: StrengthSummary): HTMLElement {
  const section = el("section", "ps-side ps-match-summary");
  const ring = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  ring.setAttribute("class", "ps-ring");
  ring.setAttribute("viewBox", "0 0 44 44");
  ring.setAttribute("aria-hidden", "true");
  // drawRing: a round-capped arc from 12 o'clock, clockwise, over its track.
  const value = clamp(report.match_rate, 0, 1);
  ring.innerHTML = `<circle class="ps-ring-track" cx="22" cy="22" r="18"/>${value > 0 ? `<circle class="ps-ring-value" cx="22" cy="22" r="18" pathLength="100" stroke-dasharray="${value * 100} 100" transform="rotate(-90 22 22)"/>` : ""}`;
  const figure = el("div", "ps-figure");
  figure.append(el("span", "ps-headline", percentText(report.match_rate)), el("span", "ps-label", t("PlayerStrengthEstimate.match.overallMatch")));
  const lead = el("div", "ps-ring-row");
  lead.append(ring, figure);
  const stats = el("div", "ps-stats");
  for (const [label, shown] of [
    [t("PlayerStrengthEstimate.firstChoice"), percentText(report.first_choice_rate)],
    [t("PlayerStrengthEstimate.goodMoveRate"), percentText(report.move_rank_good_move_rate)],
    [t("PlayerStrengthEstimate.moves"), String(report.sample_count)],
    [t("PlayerStrengthEstimate.mistakeRate"), percentText(report.mistake_rate)],
  ] as [string, string][]) {
    const stat = el("div", "ps-stat");
    stat.append(el("span", "ps-label", label), el("span", "ps-value", shown));
    stats.append(stat);
  }
  const body = el("div", "ps-match-body");
  body.append(lead, stats);
  section.append(sideHeading(black, sideName(black) + t("PlayerStrengthEstimate.matchSuffix")), body);
  return section;
}

/** The chart legend at the top right: the stone pair named. */
export function legend(): HTMLElement {
  const host = el("div", "ps-legend");
  for (const black of [true, false]) {
    const item = el("span", "ps-legend-item");
    item.append(stoneMark(black), sideName(black));
    host.append(item);
  }
  return host;
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

/** A move's bar in the side's stone colour: the black stone solid, the white one light with its
 *  edge, so each reads on the lane track in either colour mode. */
function stoneBar(g: G, black: boolean, x: number, y: number, w: number, h: number): void {
  const p = palette();
  g.fillStyle = black ? p.stoneBlack : p.stoneWhite;
  g.fillRect(x, y, w, h);
  g.strokeStyle = black ? p.stoneBlackEdge : p.stoneWhiteEdge;
  g.lineWidth = 1;
  g.strokeRect(x + 0.5, y + 0.5, w - 1, h - 1);
}

/** A hover box near the pointer: the raised ground with its control line, a title in 600 and
 *  an optional detail line in --ds-text-3. */
function hoverBox(g: G, w: number, h: number, at: { x: number; y: number }, title: string, detail: string | null): void {
  const p = palette();
  setFont(g, true, ff().size);
  const titleMetrics = metrics(g);
  let boxWidth = width(g, title);
  setFont(g, false, ff().s);
  const detailMetrics = metrics(g);
  if (detail !== null) boxWidth = Math.max(boxWidth, width(g, detail));
  boxWidth += 20;
  const boxHeight = titleMetrics.height + (detail === null ? 0 : detailMetrics.height + 2) + 12;
  const x = Math.min(Math.max(8, at.x + 14), Math.max(8, w - boxWidth - 8));
  let y = at.y - boxHeight - 12;
  if (y < 4) y = Math.min(h - boxHeight - 4, at.y + 16);
  fillRound(g, x, y, boxWidth, boxHeight, 6, p.raised);
  strokeRound(g, x, y, boxWidth, boxHeight, 6, p.lineStrong);
  text(g, title, x + 10, y + 6 + titleMetrics.ascent, p.text, true, ff().size);
  if (detail !== null) text(g, detail, x + 10, y + 8 + titleMetrics.height + detailMetrics.ascent, p.text3, false, ff().s);
}

/** Move numbers under a chart: a hairline, ticks and Label-size figures in --ds-text-3; `roomy`
 *  keeps a tick's figure clear of the last move's, which is added when the steps miss it. */
function axis(g: G, x: number, y: number, w: number, max: number, step: number, roomy: (tickX: number, endX: number) => boolean): void {
  const p = palette();
  line(g, x, y, x + w, y, p.lineStrong);
  setFont(g, false, ff().s);
  g.fillStyle = p.text3;
  const endX = moveToX(max, x, w, max);
  const baseline = y + 6 + metrics(g).ascent;
  let lastLabelX = Number.MIN_SAFE_INTEGER;
  for (let move = 1; move <= max; move += step) {
    const tickX = moveToX(move, x, w, max);
    if (move !== 1 && move !== max && !roomy(tickX, endX)) continue;
    line(g, tickX, y, tickX, y + 3, p.lineStrong);
    const label = String(move);
    g.fillText(label, tickX - i(width(g, label) / 2), baseline);
    lastLabelX = tickX;
  }
  const endLabel = String(max);
  if (max > 1 && lastLabelX !== endX && endX - lastLabelX >= 42) {
    line(g, endX, y, endX, y + 3, p.lineStrong);
    g.fillText(endLabel, endX - i(width(g, endLabel) / 2), baseline);
  }
}

type Lane = "first" | "good";
interface HitPoint { sample: StrengthSample; black: boolean; lane: Lane; x: number; bounds: { x: number; y: number; w: number; h: number }; tooltip: string }

/** PlayerStrengthMoveHitMapPanel: 选点命中图. Per side a section with the 一选 lane above the 好手
 *  lane, the sides split by a dashed midline, a hairline grid at the move ticks, the side's
 *  figures at the right and move numbers below. Hovering a bar outlines it, shows a box and the
 *  tooltip. */
export function moveHitMap(report: StrengthReport): Painted {
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
  const figures = (s: StrengthSummary): [string, string][] => [
    [t("PlayerStrengthEstimate.match.firstChoice"), percentText(s.first_choice_rate)],
    [t("PlayerStrengthEstimate.match.goodMove"), percentText(s.move_rank_good_move_rate)],
  ];

  const panel = painted("ps-hit-map", (g, w, h) => {
    hitPoints = [];
    const p = palette();
    if (report.black.samples.length === 0 && report.white.samples.length === 0) {
      text(g, t("PlayerStrengthEstimate.match.noMoveSamples"), 0, 24, p.text3, false, ff().size);
      return;
    }
    const firstLabel = t("PlayerStrengthEstimate.match.firstChoice");
    const goodLabel = t("PlayerStrengthEstimate.match.goodMove");
    setFont(g, false, ff().s);
    const labelWidth = Math.max(width(g, firstLabel), width(g, goodLabel)) + 10;
    setFont(g, false, ff().size);
    const figureWidth = Math.max(...[report.black.summary, report.white.summary].flatMap((s) => figures(s).map(([a, b]) => width(g, `${a} ${b}`))));
    const statsWidth = 14 + 8 + figureWidth + 4;
    const chartX = labelWidth;
    const chartWidth = Math.max(120, w - chartX - statsWidth - 20);
    const axisHeight = 22;
    const sectionHeight = Math.max(40, i((h - axisHeight) / 2));
    const max = maxMove(report);
    const step = dashboardTickStep(max, chartWidth);
    // The grid at the move ticks over both sections, the dashed midline between them.
    for (let move = 1; move <= max; move += step) {
      const x = moveToX(move, chartX, chartWidth, max);
      line(g, x, 0, x, sectionHeight * 2, p.grid);
    }
    line(g, chartX, sectionHeight, chartX + chartWidth, sectionHeight, p.baseline, true);
    section(g, report.black, chartX, 0, chartWidth, sectionHeight, max, true, labelWidth);
    section(g, report.white, chartX, sectionHeight, chartWidth, sectionHeight, max, false, labelWidth);
    const statsX = chartX + chartWidth + 20;
    stats(g, report.black.summary, statsX, 0, sectionHeight, true);
    stats(g, report.white.summary, statsX, sectionHeight, sectionHeight, false);
    axis(g, chartX, sectionHeight * 2, chartWidth, max, step, (tickX, endX) => endX - tickX >= 42);
    if (hovered && mouse) {
      const sample = hovered.sample;
      hoverBox(g, w, h, mouse, `${sideName(hovered.black)} ${javaFormat(t("PlayerStrengthEstimate.match.tooltipMove"), sample.move_number)}`,
        `${laneLabel(sample, hovered.lane)}  ${t("PlayerStrengthEstimate.match.tooltipWinrateLoss")} ${javaFixed(sample.winrate_loss, 1)}%`);
    }
  });

  function section(g: G, side: StrengthSideReport, x: number, top: number, w: number, h: number, max: number, black: boolean, labelWidth: number): void {
    const p = palette();
    const laneHeight = clamp(i(h / 7), 7, 10);
    const firstY = top + i(h * 0.32) - i(laneHeight / 2);
    const goodY = top + i(h * 0.68) - i(laneHeight / 2);
    g.fillStyle = p.well;
    g.fillRect(x, firstY, w, laneHeight);
    g.fillRect(x, goodY, w, laneHeight);
    setFont(g, false, ff().s);
    g.fillStyle = p.text3;
    const ascent = metrics(g).ascent;
    const center = (y: number) => y + i((laneHeight + ascent) / 2) - 1;
    const firstLabel = t("PlayerStrengthEstimate.match.firstChoice");
    const goodLabel = t("PlayerStrengthEstimate.match.goodMove");
    g.fillText(firstLabel, labelWidth - 10 - width(g, firstLabel), center(firstY));
    g.fillText(goodLabel, labelWidth - 10 - width(g, goodLabel), center(goodY));
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
        stoneBar(g, black, blockX, laneY, Math.max(3, blockWidth), laneHeight);
        if (isHovered(point)) {
          g.save();
          g.lineWidth = 1.3;
          g.strokeStyle = p.text;
          g.beginPath();
          g.roundRect(blockX - 2.5, laneY - 2.5, Math.max(3, blockWidth) + 5, laneHeight + 5, 3);
          g.stroke();
          g.restore();
        }
      }
    }
  }

  function stats(g: G, s: StrengthSummary, x: number, top: number, h: number, black: boolean): void {
    const p = palette();
    setFont(g, false, ff().size);
    const m = metrics(g);
    const firstBaseline = top + i(h / 2) - 2;
    stone(g, black, x, firstBaseline - m.ascent + i((m.ascent - 14) / 2), 14);
    figures(s).forEach(([label, value], k) => {
      const y = firstBaseline + k * (m.height + 2);
      text(g, `${label} `, x + 22, y, p.text3, false, ff().size);
      const labelWidth = width(g, `${label} `);
      text(g, value, x + 22 + labelWidth, y, p.text, true, ff().size);
    });
  }

  /** findHitPoint: the nearest centre among the hit bounds containing the point. */
  function find(at: { x: number; y: number } | null): HitPoint | null {
    if (!at) return null;
    let best: HitPoint | null = null;
    let bestDistance = Infinity;
    for (const hp of hitPoints) {
      const b = hp.bounds;
      if (at.x < b.x || at.y < b.y || at.x >= b.x + b.w || at.y >= b.y + b.h) continue;
      const d = (at.x - (b.x + b.w / 2)) ** 2 + (at.y - (b.y + b.h / 2)) ** 2;
      if (d < bestDistance) {
        bestDistance = d;
        best = hp;
      }
    }
    return best;
  }

  const update = (at: { x: number; y: number } | null) => {
    const next = find(at);
    if (hovered === next && mouse?.x === at?.x && mouse?.y === at?.y) return;
    hovered = next;
    mouse = at;
    panel.element.style.cursor = hovered ? "pointer" : "";
    setTooltip(panel.element, hovered?.tooltip ?? null);
    panel.repaint();
  };
  panel.element.addEventListener("mousemove", (e) => update({ x: e.offsetX, y: e.offsetY }));
  panel.element.addEventListener("mouseleave", () => update(null));
  return panel;
}

/** The six move ranks' class names, each selecting its colour (stage.css --ws-strength-rank-*). */
const RANK_CLASS: Record<MoveRank, string> = {
  Best: "best",
  Good: "good",
  Normal: "normal",
  Inaccuracy: "inaccuracy",
  Mistake: "mistake",
  Blunder: "blunder",
};

/** PlayerStrengthPerformanceRankPanel: 发挥水准分布, the six move-rank rows per side. */
export function performancePanel(report: StrengthReport): HTMLElement {
  const panel = el("div", "ps-dash ps-dash-performance");
  const head = el("div", "ps-section-head");
  const titles = el("div", "ps-section-title");
  titles.append(el("h3", "", t("PlayerStrengthEstimate.performance.title")), el("p", "ps-label", t("PlayerStrengthEstimate.performance.hint")));
  head.append(titles);
  panel.append(head);
  if (!hasSamples(report.black.summary) && !hasSamples(report.white.summary)) {
    panel.append(el("p", "ps-empty", t("PlayerStrengthEstimate.performance.noMoveSamples")));
    return panel;
  }
  const sides = el("div", "ps-two");
  for (const black of [true, false]) {
    const s = black ? report.black : report.white;
    const section = el("section", "ps-side");
    const heading = sideHeading(black, sideName(black));
    heading.append(el("span", "ps-tag", javaFormat(t("PlayerStrengthEstimate.performance.sideMoves"), s.summary.sample_count)));
    const table = el("div", "ps-ranks");
    const header = el("div", "ps-rank-row ps-rank-head");
    header.append(el("span"), el("span", "ps-label ps-num", t("PlayerStrengthEstimate.performance.count")), el("span"), el("span", "ps-label ps-num", t("PlayerStrengthEstimate.performance.percent")));
    table.append(header);
    // performanceRankCounts: a first choice counts as 最佳, any other 最佳 as 好手.
    const counts = MOVE_RANKS.map(() => 0);
    for (const sample of s.samples) {
      const rank = sample.first_choice ? "Best" : sample.move_rank === "Best" ? "Good" : sample.move_rank;
      counts[MOVE_RANKS.indexOf(rank)]++;
    }
    const sampleCount = Math.max(0, s.summary.sample_count);
    MOVE_RANKS.forEach((rank, k) => {
      const fraction = sampleCount <= 0 ? 0 : counts[k] / sampleCount;
      const row = el("div", `ps-rank-row rank-${RANK_CLASS[rank]}`);
      const name = el("span", "ps-rank-name");
      name.append(el("span", "ps-rank-dot"), moveRankName(rank));
      row.append(name, el("span", "ps-value ps-num", String(counts[k])), meter(fraction, "ps-fill-rank"), el("span", "ps-num", `${javaFixed(fraction * 100, 1)}%`));
      table.append(row);
    });
    section.append(heading, table);
    sides.append(section);
  }
  panel.append(sides);
  return panel;
}

/** PlayerStrengthNoteStrip / PlayerStrengthDetailNoteStrip: the note as a warning 提示条. */
export function noteStrip(note: string): HTMLElement {
  const strip = el("p", "ps-note", note);
  strip.setAttribute("role", "note");
  return strip;
}

/** PlayerStrengthMatchChart (detail dialog): per side the 一选 and 好手 lanes, the 段位 windows of
 *  7d+ segments with their hover outline and tooltip, the 疑似AI intervals and the side's figures
 *  at the right; the sides split by a dashed midline, move numbers below. */
export function matchChart(report: StrengthReport): Painted {
  let hoverText: string | null = null;
  let hoverPoint: { x: number; y: number } | null = null;

  /** The rows of a side's section: lanes, the rank row and the AI row. */
  const rowsOf = (top: number, sectionHeight: number) => ({
    firstY: top + 12,
    goodY: top + 26,
    rankY: top + 42,
    aiY: top + Math.max(64, sectionHeight - 26),
  });
  /** The lane captions' column, measured at each paint (the hover test uses the last one). */
  let captionWidth = 64;
  const geometry = (w: number, h: number) => {
    const chartX = captionWidth;
    const statsWidth = 170;
    const sectionHeight = Math.max(92, i((h - 24) / 2));
    return { chartX, chartWidth: Math.max(120, w - chartX - statsWidth), max: maxMove(report), sectionHeight, statsX: w - statsWidth + 20 };
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
  const near = (p: { x: number; y: number }, b: { x: number; y: number; w: number; h: number }) =>
    p.x >= b.x - 6 && p.y >= b.y - 10 && p.x < b.x + b.w + 6 && p.y < b.y + b.h + 10;

  /** rankTooltipAt: the black section's windows first. */
  const tooltipAt = (p: { x: number; y: number }, w: number, h: number): string | null => {
    const { chartX, chartWidth, max, sectionHeight } = geometry(w, h);
    for (const [side, black, sideTop] of [[report.black, true, 0], [report.white, false, sectionHeight]] as const) {
      for (const segment of shown(side)) {
        // Rectangle.grow(6, 10).contains.
        if (!near(p, windowBounds(segment.first_move, segment.last_move, chartX, rowsOf(sideTop, sectionHeight).rankY, chartWidth, max))) continue;
        return javaFormat(t("PlayerStrengthEstimate.match.rankTooltip"), sideName(black), segment.first_move, segment.last_move, strengthLabel(segment.summary));
      }
    }
    return null;
  };

  const chart = painted("ps-match-chart", (g, w, h) => {
    const p = palette();
    setFont(g, false, ff().s);
    captionWidth = Math.max(width(g, t("PlayerStrengthEstimate.match.firstChoice")), width(g, t("PlayerStrengthEstimate.match.goodMove"))) + 10;
    const { chartX, chartWidth, max, sectionHeight, statsX } = geometry(w, h);
    const step = tickStep(max, chartWidth);
    for (let move = 1; move <= max; move += step) {
      const x = moveToX(move, chartX, chartWidth, max);
      line(g, x, 0, x, sectionHeight * 2, p.grid);
    }
    line(g, chartX, sectionHeight, chartX + chartWidth, sectionHeight, p.baseline, true);
    side(g, report.black, true, chartX, 0, chartWidth, sectionHeight, max, statsX);
    side(g, report.white, false, chartX, sectionHeight, chartWidth, sectionHeight, max, statsX);
    axis(g, chartX, sectionHeight * 2, chartWidth, max, step, () => true);
    if (hoverText && hoverPoint) hoverBox(g, w, h, hoverPoint, hoverText, null);
  });

  function centered(g: G, s: string, x1: number, x2: number, y: number): void {
    g.fillText(s, x1 + Math.max(0, i((x2 - x1 - width(g, s)) / 2)), y);
  }

  function stat(g: G, x: number, y: number, label: string, value: string): void {
    const p = palette();
    text(g, `${label} `, x, y, p.text3, false, ff().size);
    const labelWidth = width(g, `${label} `);
    text(g, value, x + labelWidth, y, p.text, true, ff().size);
  }

  function side(g: G, s: StrengthSideReport, black: boolean, chartX: number, top: number, chartWidth: number, sectionHeight: number, max: number, statsX: number): void {
    const p = palette();
    const { firstY, goodY, rankY, aiY } = rowsOf(top, sectionHeight);
    const laneHeight = 7;
    const blockWidth = Math.max(3, Math.min(8, i(chartWidth / Math.max(max, 1))));
    stone(g, black, statsX, firstY - 4, 14);
    text(g, sideName(black), statsX + 22, firstY + 7, p.text, true, ff().size);
    g.fillStyle = p.well;
    g.fillRect(chartX, firstY, chartWidth, laneHeight);
    g.fillRect(chartX, goodY, chartWidth, laneHeight);
    for (const sample of bySortedMove(s.samples)) {
      const x = moveToX(sample.move_number, chartX, chartWidth, max);
      if (sample.first_choice) stoneBar(g, black, x, firstY, blockWidth, laneHeight);
      if (isGoodMove(sample.move_rank)) stoneBar(g, black, x, goodY, blockWidth, laneHeight);
    }
    // drawRankWindows.
    for (const segment of shown(s)) {
      const b = windowBounds(segment.first_move, segment.last_move, chartX, rankY, chartWidth, max);
      fillRound(g, b.x, b.y, b.w, b.h, 3, p.rankWindow);
      if (hoverPoint && near(hoverPoint, b)) {
        g.save();
        g.lineWidth = 1.3;
        g.strokeStyle = p.text;
        g.beginPath();
        g.roundRect(b.x - 1.5, b.y - 1.5, b.w + 3, b.h + 3, 4);
        g.stroke();
        g.restore();
      }
      if (b.w >= 12) {
        g.save();
        g.beginPath();
        g.rect(b.x + 2, b.y, Math.max(1, b.w - 4), b.h);
        g.clip();
        setFont(g, true, ff().s);
        g.fillStyle = p.text;
        centered(g, strengthLabel(segment.summary), b.x, b.x + b.w, rankY + 11);
        g.restore();
      }
    }
    // drawAiIntervals.
    let suspect = false;
    g.strokeStyle = p.ai;
    g.lineWidth = 1;
    g.strokeRect(chartX + 0.5, aiY + 0.5, chartWidth, 15);
    for (const segment of s.segments) {
      if (segment.summary.sample_count < s.minimum_segment_samples || aiLikelihood(segment.summary) < 0.7) continue;
      suspect = true;
      const x1 = moveToX(segment.first_move, chartX, chartWidth, max);
      const x2 = moveToX(segment.last_move + 1, chartX, chartWidth, max);
      g.fillStyle = p.ai;
      g.fillRect(x1 + 1, aiY + 1, Math.max(4, x2 - x1 - 2), 14);
    }
    if (!suspect) {
      setFont(g, false, ff().s);
      g.fillStyle = p.text3;
      centered(g, t("PlayerStrengthEstimate.match.noAiInterval"), chartX, chartX + chartWidth, aiY + 12);
    }
    stat(g, statsX, goodY + laneHeight + 12, t("PlayerStrengthEstimate.match.firstChoice"), `${javaFixed(s.summary.first_choice_rate * 100, 1)}%`);
    stat(g, statsX, goodY + laneHeight + 30, t("PlayerStrengthEstimate.match.goodMove"), `${javaFixed(s.summary.move_rank_good_move_rate * 100, 1)}%`);
    stat(g, statsX, aiY + 12, t("PlayerStrengthEstimate.match.aiLike"), `${javaFixed(aiLikelihood(s.summary) * 100, 1)}%`);
    // The lane captions at the left, Label size.
    setFont(g, false, ff().s);
    g.fillStyle = p.text3;
    const firstLabel = t("PlayerStrengthEstimate.match.firstChoice");
    const goodLabel = t("PlayerStrengthEstimate.match.goodMove");
    g.fillText(firstLabel, chartX - 10 - width(g, firstLabel), firstY + laneHeight);
    g.fillText(goodLabel, chartX - 10 - width(g, goodLabel), goodY + laneHeight);
  }

  const update = (at: { x: number; y: number } | null) => {
    const next = at ? tooltipAt(at, chart.width, chart.height) : null;
    if (hoverText === next && hoverPoint?.x === at?.x && hoverPoint?.y === at?.y) return;
    hoverText = next;
    hoverPoint = at;
    chart.element.style.cursor = next ? "pointer" : "";
    setTooltip(chart.element, next);
    chart.repaint();
  };
  chart.element.addEventListener("mousemove", (e) => update({ x: e.offsetX, y: e.offsetY }));
  chart.element.addEventListener("mouseleave", () => update(null));
  return chart;
}
