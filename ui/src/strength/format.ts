// Texts of the 棋力评估 dashboard: PlayerStrengthRankFormatter, the SideReport.*Text() helpers
// (PlayerStrengthEstimator.java:1181-1244) and LizzieFrame's playerStrength* text helpers
// (LizzieFrame.java:17852-17973, 18935-18994). The report carries Java's numbers; everything here
// only formats them.
import type { MoveRank, StrengthConfidence, StrengthModelView, StrengthSideReport, StrengthSummary } from "../contract/generated";
import { isChinese, t } from "../i18n";

/** String.format(Locale.US, "%.Nf", v): HALF_UP on the shortest decimal form, the sign kept. */
export function javaFixed(v: number, digits: number): string {
  if (Number.isNaN(v)) return "NaN";
  if (!Number.isFinite(v)) return v > 0 ? "Infinity" : "-Infinity";
  const negative = v < 0 || Object.is(v, -0);
  let s = Math.abs(v).toString();
  if (s.includes("e")) s = Math.abs(v).toFixed(Math.min(100, digits + 25));
  const [whole, fraction = ""] = s.split(".");
  let body: string;
  if (fraction.length <= digits) {
    body = digits > 0 ? `${whole}.${fraction.padEnd(digits, "0")}` : whole;
  } else {
    let n = BigInt(whole + fraction.slice(0, digits));
    if (fraction[digits] >= "5") n += 1n;
    const all = n.toString().padStart(digits + 1, "0");
    body = digits > 0 ? `${all.slice(0, -digits)}.${all.slice(-digits)}` : all;
  }
  return negative ? `-${body}` : body;
}

/** java.util.Formatter for the patterns of these strings: %s, %d, %n$s, %n$d and %%. */
export function javaFormat(pattern: string, ...args: (string | number)[]): string {
  let next = 0;
  return pattern.replace(/%(?:(\d+)\$)?([sd%])/g, (_, index: string | undefined, conversion: string) => {
    if (conversion === "%") return "%";
    const arg = index ? args[Number(index) - 1] : args[next++];
    if (arg === undefined) return "";
    return conversion === "d" && typeof arg === "number" ? String(Math.trunc(arg)) : String(arg);
  });
}

/** playerStrengthPercentText / SideReport.percentText: "%.0f%%". */
export function percentText(value: number): string {
  return `${javaFixed(value * 100, 0)}%`;
}

/** playerStrengthPrecisePercentText: "%.1f%%". */
export function precisePercentText(value: number): string {
  return `${javaFixed(value * 100, 1)}%`;
}

export function hasSamples(s: StrengthSummary | null | undefined): boolean {
  return !!s && s.sample_count > 0;
}

/** SideReport.averageScoreLossText. */
export function averageScoreLossText(s: StrengthSummary): string {
  return s.score_sample_count <= 0 ? "-" : javaFixed(s.average_score_loss, 1);
}

/** SideReport.weightedScoreLossText. */
export function weightedScoreLossText(s: StrengthSummary): string {
  return javaFixed(s.weighted_score_loss, 1);
}

/** SideReport.matchRateText. */
export function matchRateText(s: StrengthSummary): string {
  return percentText(s.match_rate);
}

/** SideReport.difficultyText. */
export function difficultyText(s: StrengthSummary): string {
  return s.average_difficulty === null ? "-" : javaFixed(s.average_difficulty, 0);
}

/** SideReport.humanSlCoverageText. */
export function humanSlCoverageText(s: StrengthSummary): string {
  return s.human_sl_coverage === null ? "-" : `${javaFixed(100 * s.human_sl_coverage, 1)}%`;
}

/** PlayerStrengthRankFormatter.format. */
export function rankText(rankValue: number | null): string {
  if (rankValue === null || !Number.isFinite(rankValue)) return "-";
  const value = Math.max(-18, Math.min(12, rankValue));
  if (value >= 12) return `${javaFixed(value, 1)} ${t("PlayerStrengthEstimate.rank.scale.ai")}`;
  if (value >= 11) return `${javaFixed(value, 1)} ${t("PlayerStrengthEstimate.rank.scale.topPro")}`;
  if (value >= 10) return `${javaFixed(value, 1)} ${t("PlayerStrengthEstimate.rank.scale.pro")}`;
  if (value >= 1) return javaFormat(t("PlayerStrengthEstimate.rank.danSingle"), javaFixed(value, 1));
  // Go ranks have no 0 dan: the point immediately below 1 dan is 1 kyu.
  return javaFormat(t("PlayerStrengthEstimate.rank.kyuSingle"), javaFixed(Math.max(1, 2 - value), 1));
}

/** playerStrengthRankValueText(SideReport) / playerStrengthEstimateText. */
export function sideRankText(s: StrengthSummary): string {
  return hasSamples(s) ? rankText(s.rank_value) : "-";
}

/** playerStrengthRankRangeText. */
function rankRangeText(raw: string, dan: boolean, chinese: boolean): string {
  const value = raw.trim();
  if (!value) return "-";
  if (!chinese) return dan ? `${value} dan` : `${value} kyu`;
  const parts = value.split("-");
  if (parts.length === 2) {
    return javaFormat(dan ? t("PlayerStrengthEstimate.rank.danRange") : t("PlayerStrengthEstimate.rank.kyuRange"), parts[0], parts[1]);
  }
  return javaFormat(dan ? t("PlayerStrengthEstimate.rank.danSingle") : t("PlayerStrengthEstimate.rank.kyuSingle"), value);
}

/** playerStrengthDisplayRank: the STRENGTH_BANDS literal as Java shows it. */
export function displayRank(strengthBand: string): string {
  const band = strengthBand.trim();
  if (!band || band === "-") return "-";
  const chinese = isChinese();
  if (band.toLowerCase() === "beginner") return chinese ? t("PlayerStrengthEstimate.rank.beginner") : "Beginner";
  if (band.includes("AI")) return chinese ? t("PlayerStrengthEstimate.rank.ai") : band;
  if (band.includes("一线职业")) return chinese ? t("PlayerStrengthEstimate.rank.topPro") : band;
  if (band.includes("职业")) return chinese ? t("PlayerStrengthEstimate.rank.pro") : band;
  const k = band.indexOf("k");
  if (k > 0) return rankRangeText(band.substring(0, k), false, chinese);
  const d = band.indexOf("d");
  if (d > 0) return rankRangeText(band.substring(0, d), true, chinese);
  return band;
}

/** PlayerStrengthMatchChart.strengthLabel. */
export function strengthLabel(s: StrengthSummary): string {
  return hasSamples(s) ? displayRank(s.strength_band) : "-";
}

/** PlayerStrengthMatchChart.highestDanLevel: the number right before the band's first 'd'. */
export function highestDanLevel(strengthBand: string): number {
  const band = strengthBand.trim();
  const d = band.indexOf("d");
  if (d <= 0) return -1;
  let start = d - 1;
  while (start >= 0 && band[start] >= "0" && band[start] <= "9") start--;
  if (start === d - 1) return -1;
  return Number.parseInt(band.substring(start + 1, d), 10);
}

/** playerStrengthConfidenceText. */
export function confidenceText(confidence: StrengthConfidence): string {
  switch (confidence) {
    case "High": return t("PlayerStrengthEstimate.confidence.high");
    case "Medium": return t("PlayerStrengthEstimate.confidence.medium");
    default: return t("PlayerStrengthEstimate.confidence.low");
  }
}

/** playerStrengthModelDisplayName. */
export function modelDisplayName(model: StrengthModelView | null | undefined): string {
  if (!model) return "-";
  if (model.name.toLowerCase() === "xgboost20tun") return "XGBoost 20TUN";
  return model.name.trim() || "-";
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

/** playerStrengthAiLikelihood. */
export function aiLikelihood(s: StrengthSummary): number {
  if (!hasSamples(s)) return 0;
  const matchSignal = clamp((s.match_rate - 0.72) / 0.22, 0, 1);
  const firstChoiceSignal = clamp((s.first_choice_rate - 0.5) / 0.18, 0, 1);
  const lossSignal = clamp((3.2 - s.weighted_score_loss) / 3.2, 0, 1);
  const mistakeSignal = 1 - clamp(s.mistake_rate / 0.05, 0, 1);
  return clamp(0.35 * matchSignal + 0.3 * firstChoiceSignal + 0.25 * lossSignal + 0.1 * mistakeSignal, 0, 1);
}

/** playerStrengthRankScaleRows. */
export function rankScaleRows(): [string, string][] {
  return [
    ["12+", t("PlayerStrengthEstimate.rank.scale.ai")],
    ["11+", t("PlayerStrengthEstimate.rank.scale.topPro")],
    ["10+", t("PlayerStrengthEstimate.rank.scale.pro")],
    ["<10", t("PlayerStrengthEstimate.rank.scale.fox")],
  ];
}

/** playerStrengthRankScaleSelectedIndex. */
export function rankScaleSelectedIndex(rankValue: number | null): number {
  if (rankValue === null || !Number.isFinite(rankValue)) return 3;
  if (rankValue >= 12) return 0;
  if (rankValue >= 11) return 1;
  return rankValue >= 10 ? 2 : 3;
}

/** playerStrengthRankScaleTooltip, as the lines of its HTML. */
export function rankScaleTooltip(side: StrengthSideReport): string {
  const current = hasSamples(side.summary) ? rankText(side.summary.rank_value) : "-";
  return [`${t("PlayerStrengthEstimate.rank.scale")}: ${current}`, ...rankScaleRows().map(([a, b]) => `${a} ${b}`)].join("\n");
}

/** playerStrengthRankScalePlainText (the card's accessible description). */
export function rankScalePlainText(side: StrengthSideReport): string {
  const current = hasSamples(side.summary) ? rankText(side.summary.rank_value) : "-";
  return `${t("PlayerStrengthEstimate.rank.scale")}: ${current}${rankScaleRows().map(([a, b]) => `; ${a} ${b}`).join("")}`;
}

/** MoveRankDefinition.Rank in ordinal order with nameKey and isGoodMove. */
export const MOVE_RANKS: readonly MoveRank[] = ["Best", "Good", "Normal", "Inaccuracy", "Mistake", "Blunder"];

export function isGoodMove(rank: MoveRank): boolean {
  return rank === "Best" || rank === "Good";
}

/** Rank.nameKey; literal t() calls keep the keys in the Java string extraction. */
export function moveRankName(rank: MoveRank): string {
  switch (rank) {
    case "Best": return t("PlayerStrengthEstimate.moveRank.best");
    case "Good": return t("PlayerStrengthEstimate.moveRank.good");
    case "Normal": return t("PlayerStrengthEstimate.moveRank.normal");
    case "Inaccuracy": return t("PlayerStrengthEstimate.moveRank.inaccuracy");
    case "Mistake": return t("PlayerStrengthEstimate.moveRank.mistake");
    case "Blunder": return t("PlayerStrengthEstimate.moveRank.blunder");
  }
}

/** HumanSlStrengthPreparation status and thrown keys; literal t() calls keep them extracted. */
export function humanSlText(key: string): string {
  const texts: Record<string, string> = {
    "HumanSL40.backgroundIdle": t("HumanSL40.backgroundIdle"),
    "HumanSL40.backgroundOff": t("HumanSL40.backgroundOff"),
    "HumanSL40.backgroundPaused": t("HumanSL40.backgroundPaused"),
    "HumanSL40.backgroundReady": t("HumanSL40.backgroundReady"),
    "HumanSL40.backgroundNeedEngine": t("HumanSL40.backgroundNeedEngine"),
    "HumanSL40.backgroundFailed": t("HumanSL40.backgroundFailed"),
    "HumanSL40.preparing": t("HumanSL40.preparing"),
    "HumanSL40.progress": t("HumanSL40.progress"),
    "HumanSL40.cancelled": t("HumanSL40.cancelled"),
    "HumanSL40.failed": t("HumanSL40.failed"),
    "HumanSL40.downloadRequired": t("HumanSL40.downloadRequired"),
    "HumanSL40.needGame": t("HumanSL40.needGame"),
    "HumanSL40.needEngine": t("HumanSL40.needEngine"),
    "HumanSL40.needAnalysis": t("HumanSL40.needAnalysis"),
    "HumanSL40.boardSize": t("HumanSL40.boardSize"),
    "HumanSL40.setupUnsupported": t("HumanSL40.setupUnsupported"),
    "HumanSL40.invalidModel": t("HumanSL40.invalidModel"),
    "HumanSL40.invalidResponse": t("HumanSL40.invalidResponse"),
    "HumanSL40.gameChanged": t("HumanSL40.gameChanged"),
  };
  return texts[key] ?? t(key);
}
