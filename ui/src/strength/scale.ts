// 段位标尺 (Java StrengthScale): kyu, amateur dan, professional, top professional and AI on one
// unbroken track with labelled ticks; where one tier meets the next is a longer tick. The dan ranks
// get the most room because most players are there. Painted on a canvas like the dashboard's other
// charts; what it drew is kept for the acceptance probe.
import { format, t } from "../i18n";
import { ff, fillRound, line, metrics, painted, palette, setFont, stone, width, type G } from "./paint";

/** The rank values where each tier starts and ends; 1 is 1 dan, below it are the kyu ranks. */
const TIERS: readonly [number, number][] = [[-18, 1], [1, 10], [10, 11], [11, 12], [12, 13]];
/** Each tier's share of the ruler. */
const WEIGHTS = [0.22, 0.4, 0.126, 0.127, 0.127];

/** A tick: its rank value, label (or null), how much the label matters (0 most) and whether one tier
 *  ends there. Labels between two whole ranks name a tier and have no tick. */
interface Tick {
  value: number;
  label: string | null;
  priority: number;
  boundary: boolean;
}

/** StrengthScale.ticks, from the weakest rank to AI. */
function ticks(): Tick[] {
  const kyu = t("PlayerStrengthEstimate.scale.kyu");
  const dan = t("PlayerStrengthEstimate.scale.dan");
  const list: Tick[] = [
    { value: -13, label: format(kyu, 15), priority: 5, boundary: false },
    { value: -8, label: format(kyu, 10), priority: 3, boundary: false },
    { value: -3, label: format(kyu, 5), priority: 4, boundary: false },
  ];
  for (let rank = 1; rank <= 9; rank++) {
    const priority = rank === 1 ? 0 : rank === 5 || rank === 9 ? 1 : rank % 2 === 1 ? 2 : -1;
    list.push({ value: rank, label: priority < 0 ? null : format(dan, rank), priority, boundary: rank === 1 });
  }
  for (let boundary = 10; boundary <= 12; boundary++) list.push({ value: boundary, label: null, priority: -1, boundary: true });
  list.push(
    { value: 10.5, label: t("PlayerStrengthEstimate.scale.pro"), priority: 1, boundary: false },
    { value: 11.5, label: t("PlayerStrengthEstimate.scale.topPro"), priority: 2, boundary: false },
    { value: 12.5, label: t("PlayerStrengthEstimate.scale.ai"), priority: 0, boundary: false },
  );
  return list;
}

/** StrengthScale.x over a ruler 1 wide: where the rank `value` sits, from 0 to 1. */
export function scaleFraction(value: number): number {
  const clamped = Math.max(TIERS[0][0], Math.min(TIERS[TIERS.length - 1][1], value));
  let x = 0;
  for (let i = 0; i < TIERS.length; i++) {
    const [low, high] = TIERS[i];
    if (clamped < high || i === TIERS.length - 1) return x + WEIGHTS[i] * Math.max(0, Math.min(1, (clamped - low) / (high - low)));
    x += WEIGHTS[i];
  }
  return x;
}

/** A point on the ruler: a black or white stone, or the accent knob when `black` is null; a
 *  null rank draws nothing. */
export interface Marker {
  rank: number | null;
  black: boolean | null;
}

/** What a painted ruler shows (acceptance probe), in client pixels of the canvas. */
export interface RulerDrawn {
  /** Every labelled tick with the line its label went on (0 or 1), or -1 when it found no room. */
  labels: { text: string; value: number; x: number; left: number; right: number; shown: boolean; row: number }[];
  /** The tier boundaries' long ticks. */
  boundaries: number[];
  markers: { rank: number; black: boolean | null; x: number }[];
  width: number;
}

const drawnRulers = new WeakMap<HTMLCanvasElement, RulerDrawn>();

/** The probe's view of a painted ruler. */
export function rulerDrawn(canvas: HTMLCanvasElement): RulerDrawn | null {
  return drawnRulers.get(canvas) ?? null;
}

const MARKER = 12;
const TRACK = 6;

/** The ruler's height with `rows` lines of labels at this session's type size
 *  (StrengthScale.height). */
function rulerHeight(rows: number): number {
  const g = document.createElement("canvas").getContext("2d")!;
  setFont(g, false, ff().s);
  return 1 + MARKER + 7 + rows * metrics(g).height;
}

/** StrengthScale.paint: the ruler with `markers`; `fill` fills the track in the accent up to the
 *  first marker. */
export function rankRuler(markers: Marker[], fill: boolean): HTMLCanvasElement {
  const labelled = ticks();
  const ruler = painted("ps-ruler", (g, w) => {
    const p = palette();
    // One pixel above for the stones, which are a little larger than the knob.
    g.translate(0, 1);
    const trackY = (MARKER - TRACK) / 2;
    fillRound(g, 0, trackY, w, TRACK, TRACK / 2, p.well);
    const first = markers[0]?.rank;
    if (fill && first !== null && first !== undefined && Number.isFinite(first)) {
      fillRound(g, 0, trackY, Math.max(TRACK, scaleFraction(first) * w), TRACK, TRACK / 2, p.accent);
    }
    const drawn: RulerDrawn = { labels: [], boundaries: [], markers: [], width: w };
    const rows = paintTicks(g, w, trackY + TRACK, labelled, drawn);
    paintMarkers(g, w, markers, drawn);
    drawnRulers.set(ruler.element, drawn);
    // A second line of labels makes the ruler taller; its box change paints it again.
    const height = `${rulerHeight(rows)}px`;
    if (ruler.element.style.height !== height) ruler.element.style.height = height;
  });
  ruler.element.style.height = `${rulerHeight(1)}px`;
  return ruler.element;
}

/** StrengthScale.paintTicks: the ticks, then the labels by importance; a label that would touch a
 *  placed one goes on a second line, and gives way only when it touches one there too. Answers
 *  how many lines of labels were drawn. */
function paintTicks(g: G, w: number, trackBottom: number, labelled: Tick[], drawn: RulerDrawn): number {
  const p = palette();
  setFont(g, false, ff().s);
  const m = metrics(g);
  const tickTop = trackBottom + 2;
  const baseline = MARKER + 7 + m.ascent;
  const placed: { left: number; right: number }[][] = [[], []];
  for (const tick of [...labelled].sort((a, b) => a.priority - b.priority)) {
    const x = scaleFraction(tick.value) * w;
    if (tick.value % 1 === 0) {
      g.save();
      g.globalAlpha = tick.boundary ? 200 / 255 : 150 / 255;
      line(g, x, tickTop, x, tickTop + (tick.boundary ? 5 : 3), p.text3);
      g.restore();
      if (tick.boundary) drawn.boundaries.push(x);
    }
    if (tick.label === null) continue;
    const labelWidth = width(g, tick.label);
    const left = Math.max(0, Math.min(w - labelWidth, Math.round(x - labelWidth / 2)));
    const box = { left: left - 3, right: left + labelWidth + 3 };
    const row = placed.findIndex((line) => line.every((o) => box.right <= o.left || box.left >= o.right));
    drawn.labels.push({ text: tick.label, value: tick.value, x, left, right: left + labelWidth, shown: row >= 0, row });
    if (row < 0) continue;
    placed[row].push(box);
    g.fillStyle = p.text3;
    g.fillText(tick.label, left, baseline + row * m.height);
  }
  drawn.labels.sort((a, b) => a.value - b.value);
  drawn.boundaries.sort((a, b) => a - b);
  return placed[1].length > 0 ? 2 : 1;
}

function paintMarkers(g: G, w: number, markers: Marker[], drawn: RulerDrawn): void {
  const p = palette();
  const centres = markers.map((m) => (m.rank !== null && Number.isFinite(m.rank) ? scaleFraction(m.rank) * w : NaN));
  // Two stones that would hide each other sit side by side about their middle.
  if (centres.length === 2 && centres.every(Number.isFinite)) {
    const apart = MARKER * 0.85;
    if (Math.abs(centres[0] - centres[1]) < apart) {
      const middle = (centres[0] + centres[1]) / 2;
      const firstLeft = centres[0] <= centres[1];
      centres[0] = middle + (firstLeft ? -apart / 2 : apart / 2);
      centres[1] = middle + (firstLeft ? apart / 2 : -apart / 2);
    }
  }
  markers.forEach((marker, i) => {
    if (!Number.isFinite(centres[i])) return;
    const left = Math.max(1, Math.min(w - MARKER - 1, centres[i] - MARKER / 2));
    drawn.markers.push({ rank: marker.rank!, black: marker.black, x: left + MARKER / 2 });
    if (marker.black === null) {
      g.beginPath();
      g.arc(left + MARKER / 2, MARKER / 2, MARKER / 2, 0, Math.PI * 2);
      g.fillStyle = p.raised;
      g.fill();
      g.beginPath();
      g.arc(left + MARKER / 2, MARKER / 2, MARKER / 2 - 2, 0, Math.PI * 2);
      g.strokeStyle = p.accent;
      g.lineWidth = 2;
      g.stroke();
    } else {
      stone(g, marker.black, left - 1, -1, MARKER + 2);
    }
  });
}
