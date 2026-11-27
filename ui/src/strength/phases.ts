// 分阶段 (Java StrengthPhaseView, StrengthPhaseTimeline, StrengthPhaseCard): the game as a bar
// split into 布局 / 中盘 / 官子 whose two dividers can be dragged, or moved with the arrow keys once
// focused; a card per phase with each side's rank, its best and weakest phase and who played the
// phase better; what is worth a closer look in review and why phase estimates vary more. The core
// estimates every phase (StrengthPhases); this view holds the dividers' draft while they move,
// asks the core for the phases at each step and keeps the boundaries once a divider is let go.
import type { PhaseSide, PhaseVerdict, StrengthPhase, StrengthPhases } from "../contract/generated";
import { format, t } from "../i18n";
import { confidenceText, rankText } from "./format";
import { el, explain, stoneMark } from "./panels";
import { phaseSaves } from "./phaseSaves";
import { ff, line, metrics, painted, palette, setFont, width, type G } from "./paint";
import { rankRuler, rulerDrawn, type RulerDrawn } from "./scale";

/** Black and White as the phase cards name them (StrengthDashboard's blackName / whiteName). */
const sideName = (black: boolean) => (black ? t("SidebarHeader.black") : t("SidebarHeader.white"));

/** StrengthPhases.NAMES; literal t() calls keep the keys in the Java string extraction. */
export function phaseName(index: number): string {
  return [t("PlayerStrengthEstimate.phase.opening"), t("PlayerStrengthEstimate.phase.middle"), t("PlayerStrengthEstimate.phase.endgame")][index] ?? "";
}

/** "第 61–150 手", or 未下到 for a phase the game never reached. */
export function phaseRange(phase: StrengthPhase): string {
  return phase.last >= phase.first ? format(t("PlayerStrengthEstimate.phase.range"), phase.first, phase.last) : t("PlayerStrengthEstimate.phase.unplayed");
}

/** StrengthPhaseCard.rankText: the side's rank, or a dash when it has too few moves. */
export function phaseRank(side: PhaseSide): string {
  return side.ranked ? rankText(side.summary.rank_value) : "\u2014";
}

/** StrengthPhaseCard.verdict: who played the phase better, or why there is no rank. */
function verdictText(verdict: PhaseVerdict): string {
  switch (verdict.type) {
    case "NotReached": return t("PlayerStrengthEstimate.phase.notReached");
    case "HumanStyleLimit": return format(t("PlayerStrengthEstimate.phase.humanStyleLimit"), verdict.moves);
    case "TooFew": return t("PlayerStrengthEstimate.phase.tooFew");
    case "Even": return t("PlayerStrengthEstimate.phase.even");
    case "Slightly": return format(t("PlayerStrengthEstimate.phase.slightly"), sideName(verdict.black));
    case "Clearly": return format(t("PlayerStrengthEstimate.phase.clearly"), sideName(verdict.black));
  }
}

/** StrengthPhaseView.review: the phase each side may want to look at again, or null. */
function reviewText(phases: StrengthPhases): string | null {
  const review = phases.review;
  const item = (black: boolean, phase: number) => format(t("PlayerStrengthEstimate.phase.reviewItem"), sideName(black), phaseName(phase));
  switch (review.type) {
    case "None": return null;
    case "Steady": return t("PlayerStrengthEstimate.phase.steady");
    case "One": return format(t("PlayerStrengthEstimate.phase.review"), item(review.black, review.phase));
    case "Both": return format(t("PlayerStrengthEstimate.phase.reviewBoth"), item(true, review.black), item(false, review.white));
    case "Same": return format(t("PlayerStrengthEstimate.phase.reviewSame"), phaseName(review.phase));
  }
}

/** What one phase card shows (acceptance probe). */
interface CardShown {
  name: string;
  range: string;
  verdict: string;
  played: boolean;
  black: { rank: string; badge: string; confidence: string };
  white: { rank: string; badge: string; confidence: string };
  ruler: RulerDrawn | null;
}

/** StrengthPhaseCard: the phase's dot, name and moves; a row per side with its stone, name, rank,
 *  best / weakest badge and confidence; both ranks on one ruler; who played it better. */
function phaseCard(phase: StrengthPhase, index: number): { element: HTMLElement; shown: () => CardShown } {
  const key = `phase.${index}`;
  const played = phase.last >= phase.first;
  const card = el("section", `ps-phase-card${played ? "" : " unplayed"}`);
  card.dataset.phase = String(index);
  const head = el("div", "ps-phase-card-head");
  const dot = el("span", `ps-phase-dot tint-${index}`);
  dot.setAttribute("aria-hidden", "true");
  head.append(dot, named(el("h3", "", phaseName(index)), `${key}.name`), named(el("span", "ps-label ps-phase-range", phaseRange(phase)), `${key}.range`));
  const body = el("div", "ps-phase-card-body");
  const shownSide = (black: boolean) => {
    const side = black ? phase.black : phase.white;
    const sideKey = `${key}.${black ? "black" : "white"}`;
    const row = el("div", "ps-phase-side");
    row.append(
      stoneMark(black),
      named(el("span", "ps-label ps-phase-name", sideName(black)), `${sideKey}.name`),
      named(el("span", side.ranked ? "ps-value ps-phase-rank" : "ps-phase-rank none", phaseRank(side)), `${sideKey}.rank`),
    );
    const badge = side.badge === "Best" ? t("PlayerStrengthEstimate.phase.best") : side.badge === "Weakest" ? t("PlayerStrengthEstimate.phase.weakest") : "";
    if (badge) row.append(named(el("span", `ps-phase-badge${side.badge === "Best" ? " best" : ""}`, badge), `${sideKey}.badge`));
    const confidence = side.ranked ? confidenceText(side.summary.confidence) : "";
    if (confidence) {
      const chip = explain(el("span", `ps-phase-confidence ${side.summary.confidence.toLowerCase()}`), t("PlayerStrengthEstimate.help.confidence"), `${sideKey}.confidence`);
      named(chip, `${sideKey}.confidence`);
      const mark = el("span", "ps-confidence-dot");
      mark.setAttribute("aria-hidden", "true");
      chip.append(mark, el("span", "ps-confidence-label", t("PlayerStrengthEstimate.confidence")), el("span", "", confidence));
      row.append(chip);
    }
    body.append(row);
    return { rank: phaseRank(side), badge, confidence };
  };
  const black = shownSide(true);
  const white = shownSide(false);
  const ruler = rankRuler(
    [
      { rank: phase.black.ranked ? phase.black.summary.rank_value : null, black: true },
      { rank: phase.white.ranked ? phase.white.summary.rank_value : null, black: false },
    ],
    false,
  );
  const rulerBox = explain(el("div", "ps-phase-ruler"), t("PlayerStrengthEstimate.help.rank"), `${key}.rank`);
  rulerBox.append(ruler);
  const verdict = el("p", "ps-phase-verdict");
  const v = phase.verdict;
  const decided = v.type === "Slightly" || v.type === "Clearly";
  if (decided) verdict.append(stoneMark(v.black));
  verdict.classList.toggle("decided", decided);
  verdict.append(named(el("span", "", verdictText(v)), `${key}.verdict`));
  body.append(rulerBox, verdict);
  card.append(head, body);
  card.setAttribute("aria-label", phaseName(index));
  card.setAttribute(
    "aria-description",
    `${phaseRange(phase)}; ${sideName(true)} ${black.rank}; ${sideName(false)} ${white.rank}; ${verdictText(v)}`,
  );
  return {
    element: card,
    shown: () => ({ name: phaseName(index), range: phaseRange(phase), verdict: verdictText(v), played, black, white, ruler: rulerDrawn(ruler) }),
  };
}

/** Names a text for the acceptance probe (its words and whether it is cut off). */
function named<T extends HTMLElement>(e: T, key: string): T {
  e.dataset.strengthText = key;
  return e;
}

/** How the view reaches the core: the phases at a draft's boundaries (nothing kept), and keeping
 *  boundaries (their phases come back). Null: no answer for this dialog. */
export interface PhaseActions {
  preview(bounds: [number, number]): Promise<StrengthPhases | null>;
  commit(bounds: [number, number]): Promise<StrengthPhases | null>;
  /** A divider is held: the dialog's refresh waits. */
  holding(on: boolean): void;
}

export interface PhasesView {
  element: HTMLElement;
  probe(): unknown;
}

/** The bar's geometry (StrengthPhaseTimeline): 1 px in from the sides, 34 px tall under a 5 px
 *  margin. */
const BAR_LEFT = 1;
const BAR_TOP = 5;
const BAR_HEIGHT = 34;

export function phasesPanel(initial: StrengthPhases, actions: PhaseActions): PhasesView {
  let phases = initial;
  /** The saves; their phases at the saved boundaries are shown again when keeping others fails. */
  const saves = phaseSaves(initial, (bounds) => actions.commit(bounds));
  let draft: [number, number] = [...initial.bounds];
  let frozenEnd = -1;
  let drag: number | null = null;
  let previewing = false;
  let bandLabels: string[] = [];
  let cards: { element: HTMLElement; shown: () => CardShown }[] = [];

  const panel = el("div", "ps-dash ps-dash-phases");

  // ---- the timeline card: title, hint, 恢复默认, the bar with its dividers
  const top = el("section", "ps-phase-timeline");
  const head = el("div", "ps-phase-head");
  const reset = el("button", "ps-phase-reset", t("PlayerStrengthEstimate.phase.reset"));
  reset.type = "button";
  reset.dataset.strength = "phase.reset";
  head.append(
    named(el("h3", "", t("PlayerStrengthEstimate.phase.title")), "phase.title"),
    named(el("span", "ps-label ps-phase-hint", t("PlayerStrengthEstimate.phase.hint")), "phase.hint"),
    reset,
  );
  const bar = el("div", "ps-timeline");
  const lastMove = () => phases.last_move;
  const min = () => phases.min_phase_moves;
  /** StrengthPhaseTimeline.axisEnd: the whole game, or further when it ended before the last
   *  divider; it holds still while a divider is dragged. */
  const axisEnd = () => (frozenEnd > 0 ? frozenEnd : Math.max(lastMove(), Math.max(phases.default_bounds[1], draft[1]) + min()));
  const canvas = painted("ps-timeline-bar", (g, w) => paintBar(g, w));
  const g0 = document.createElement("canvas").getContext("2d")!;
  setFont(g0, false, ff().s);
  canvas.element.style.height = `${BAR_TOP + BAR_HEIGHT + 5 + 6 + metrics(g0).height}px`;
  // The canvas's own size, read live: the dividers are placed when the bar's box changes, which may
  // be before the canvas has repainted at its new size.
  const barWidth = () => Math.max(1, Math.round(canvas.element.clientWidth) - 2);
  const xOf = (move: number) => BAR_LEFT + barWidth() * Math.max(0, Math.min(1, move / axisEnd()));
  const moveAt = (x: number) => Math.round(((x - BAR_LEFT) / barWidth()) * axisEnd());

  const dividers = [0, 1].map((index) => {
    const handle = el("div", "ps-divider");
    handle.tabIndex = 0;
    handle.setAttribute("role", "slider");
    handle.setAttribute("aria-orientation", "horizontal");
    handle.dataset.strength = `phase.divider.${index}`;
    handle.addEventListener("pointerdown", (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      frozenEnd = axisEnd();
      drag = index;
      handle.classList.add("dragging");
      handle.setPointerCapture(e.pointerId);
      handle.focus();
      actions.holding(true);
    });
    handle.addEventListener("pointermove", (e) => {
      if (drag !== index) return;
      moveDivider(index, moveAt(e.clientX - canvas.element.getBoundingClientRect().left));
    });
    const release = () => {
      if (drag !== index) return;
      drag = null;
      frozenEnd = -1;
      handle.classList.remove("dragging");
      layout();
      canvas.repaint();
      // Kept before the refresh held back by the drag asks for its report: that report then
      // already has these boundaries.
      void keep(draft);
      actions.holding(false);
    };
    handle.addEventListener("pointerup", release);
    handle.addEventListener("lostpointercapture", release);
    handle.addEventListener("keydown", (e) => {
      const step = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
      if (!step || e.altKey || e.ctrlKey || e.metaKey) return;
      e.preventDefault();
      if (moveDivider(index, draft[index] + step * (e.shiftKey ? 10 : 1))) void keep(draft);
    });
    return handle;
  });
  bar.append(canvas.element, ...dividers);
  top.append(head, bar);

  const cardRow = el("div", "ps-phase-cards");
  const notes = el("div", "ps-phase-notes");
  const review = el("p", "ps-phase-review");
  review.setAttribute("role", "note");
  const reviewText_ = named(el("span"), "phase.review");
  review.append(infoMark(), reviewText_);
  const caveat = named(el("p", "ps-phase-caveat", t("PlayerStrengthEstimate.phase.caveat")), "phase.caveat");
  notes.append(review, caveat);
  panel.append(top, cardRow, notes);

  reset.onclick = () => {
    draft = [...phases.default_bounds];
    layout();
    canvas.repaint();
    void keep(draft);
  };

  /** The latest move divider `index` can reach: the end of the axis less a phase, by the core's
   *  limit on boundaries. */
  const highest = (index: number) => (index === 0 ? draft[1] - min() : Math.max(draft[0] + min(), Math.min(phases.max_bound, axisEnd() - min())));

  /** StrengthPhaseTimeline.moveDivider: within its neighbours, every phase at least ten moves. */
  function moveDivider(index: number, move: number): boolean {
    const low = index === 0 ? min() : draft[0] + min();
    const target = Math.max(low, Math.min(highest(index), move));
    if (target === draft[index]) return false;
    draft = index === 0 ? [target, draft[1]] : [draft[0], target];
    layout();
    canvas.repaint();
    void preview();
    return true;
  }

  /** The phases at the draft while a divider moves; one request at a time, the newest draft next. */
  async function preview(): Promise<void> {
    if (previewing) return;
    previewing = true;
    try {
      while (true) {
        const asked: [number, number] = [...draft];
        const at = saves.started;
        const result = await actions.preview(asked);
        if (!result || at !== saves.started) return;
        if (asked[0] === draft[0] && asked[1] === draft[1]) {
          show(result, false);
          return;
        }
      }
    } finally {
      previewing = false;
    }
  }

  /** Keeps `bounds` (StrengthPhaseView's onCommit): saved once, then the phases at them. The last
   *  input wins: while an earlier keep is under way, even the boundaries it replaced are sent again.
   *  When saving fails (its message shown), the view returns to the saved boundaries. */
  async function keep(bounds: [number, number]): Promise<void> {
    const saved = saves.saved.bounds;
    const same = bounds[0] === saved[0] && bounds[1] === saved[1] && bounds[0] === phases.bounds[0] && bounds[1] === phases.bounds[1];
    if (same && saves.pending === 0) return;
    const shown = await saves.keep(bounds);
    if (!shown) return;
    if (drag === null) draft = [...shown.bounds];
    show(shown, true);
  }

  /** The dividers where the draft puts them, named for screen readers. */
  function layout(): void {
    const names = [0, 1, 2].map(phaseName);
    dividers.forEach((handle, index) => {
      handle.style.left = `${Math.round(xOf(draft[index])) - 8}px`;
      const label = format(t("PlayerStrengthEstimate.phase.divider"), names[index], names[index + 1], draft[index]);
      if (handle.getAttribute("aria-label") !== label) handle.setAttribute("aria-label", label);
      handle.setAttribute("aria-valuenow", String(draft[index]));
      handle.setAttribute("aria-valuemin", String(index === 0 ? min() : draft[0] + min()));
      handle.setAttribute("aria-valuemax", String(highest(index)));
    });
    const saved = saves.saved.bounds;
    const isDefault = saved[0] === phases.default_bounds[0] && saved[1] === phases.default_bounds[1];
    if (reset.hidden !== isDefault) {
      // 恢复默认 hides itself once used: the keyboard stays in the dialog, on the first divider.
      if (isDefault && document.activeElement === reset) dividers[0].focus();
      reset.hidden = isDefault;
    }
  }

  /** `next` in the cards and the notes; `kept`: the boundaries are now saved. */
  function show(next: StrengthPhases, kept: boolean): void {
    phases = next;
    if (kept) layout();
    cards = next.phases.map((phase, index) => phaseCard(phase, index));
    cardRow.replaceChildren(...cards.map((c) => c.element));
    const text = reviewText(next);
    review.hidden = text === null;
    if (reviewText_.textContent !== (text ?? "")) reviewText_.textContent = text ?? "";
    canvas.repaint();
  }

  /** StrengthPhaseTimeline.bandLabel: the phase's name and moves, or that the game ended first. */
  function bandLabel(index: number): string {
    const first = index === 0 ? 1 : draft[index - 1] + 1;
    const last = index === 2 ? lastMove() : Math.min(draft[index], lastMove());
    return `${phaseName(index)} \u00b7 ${last < first ? t("PlayerStrengthEstimate.phase.unplayed") : `${first}\u2013${last}`}`;
  }

  /** StrengthPhaseTimeline.paintComponent: the bar in three tinted bands with their labels, the
   *  part past the game's end veiled behind a dashed line, the move axis below. */
  function paintBar(g: G, w: number): void {
    const p = palette();
    const bw = barWidth();
    g.save();
    g.beginPath();
    g.roundRect(BAR_LEFT, BAR_TOP, bw, BAR_HEIGHT, 5);
    g.clip();
    g.fillStyle = p.well;
    g.fillRect(BAR_LEFT, BAR_TOP, bw, BAR_HEIGHT);
    const edges = [BAR_LEFT, xOf(draft[0]), xOf(draft[1]), BAR_LEFT + bw];
    setFont(g, true, ff().s);
    const m = metrics(g);
    const baseline = BAR_TOP + Math.trunc((BAR_HEIGHT - m.height) / 2) + m.ascent;
    bandLabels = [];
    [26, 52, 84].forEach((alpha, i) => {
      g.globalAlpha = alpha / 255;
      g.fillStyle = p.accent;
      g.fillRect(edges[i], BAR_TOP, edges[i + 1] - edges[i], BAR_HEIGHT);
      g.globalAlpha = 1;
      const room = Math.max(0, edges[i + 1] - edges[i] - 20);
      let label = bandLabel(i);
      if (width(g, label) > room) label = phaseName(i);
      if (width(g, label) <= room) {
        g.fillStyle = p.text;
        g.fillText(label, (edges[i] + edges[i + 1]) / 2 - width(g, label) / 2, baseline);
        bandLabels.push(label);
      } else bandLabels.push("");
    });
    const end = xOf(lastMove());
    const beyond = lastMove() < axisEnd();
    if (beyond) {
      g.globalAlpha = 150 / 255;
      g.fillStyle = p.panel;
      g.fillRect(end, BAR_TOP, BAR_LEFT + bw - end, BAR_HEIGHT);
    }
    g.restore();
    if (beyond) {
      g.save();
      g.globalAlpha = 110 / 255;
      line(g, end, BAR_TOP + 4, end, BAR_TOP + BAR_HEIGHT - 4, p.text3, true);
      g.restore();
    }
    paintAxis(g, w, BAR_TOP + BAR_HEIGHT + 5);
  }

  function paintAxis(g: G, w: number, top: number): void {
    const p = palette();
    setFont(g, false, ff().s);
    const end = axisEnd();
    const step = end <= 120 ? 20 : end <= 400 ? 50 : 100;
    const baseline = top + 6 + metrics(g).ascent;
    let previousRight = Number.MIN_SAFE_INTEGER;
    for (let move = step; move < end; move += step) {
      const x = xOf(move);
      g.save();
      g.globalAlpha = 150 / 255;
      line(g, x, top, x, top + 3, p.text3);
      g.restore();
      const label = String(move);
      const labelWidth = width(g, label);
      const labelLeft = Math.round(x - labelWidth / 2);
      if (labelLeft > previousRight + 6 && labelLeft + labelWidth <= w) {
        g.fillStyle = p.text3;
        g.fillText(label, labelLeft, baseline);
        previousRight = labelLeft + labelWidth;
      }
    }
  }

  new ResizeObserver(() => layout()).observe(bar);
  show(initial, true);

  return {
    element: panel,
    probe: () => {
      const r = canvas.element.getBoundingClientRect();
      return {
        bounds: saves.saved.bounds,
        draft,
        /** The boundaries the shown cards were estimated at (a preview's while a divider moves). */
        shown: phases.bounds,
        lastMove: lastMove(),
        axisEnd: axisEnd(),
        busy: previewing || saves.pending > 0 || drag !== null,
        bar: { x: r.left, y: r.top, width: r.width, height: r.height },
        dividers: dividers.map((h) => ({
          label: h.getAttribute("aria-label") ?? "",
          value: Number(h.getAttribute("aria-valuenow")),
          focused: document.activeElement === h,
        })),
        reset: !reset.hidden,
        bands: bandLabels,
        cards: cards.map((c) => c.shown()),
        review: review.hidden ? "" : reviewText_.textContent ?? "",
        caveat: caveat.textContent ?? "",
      };
    },
  };
}

/** StrengthNote's mark: an outlined circle with an "i". */
function infoMark(): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("class", "ps-info-mark");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("aria-hidden", "true");
  svg.innerHTML = '<circle cx="8" cy="8" r="7.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 7v4.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round"/><circle cx="8" cy="4.8" r="0.9" fill="currentColor"/>';
  return svg;
}
