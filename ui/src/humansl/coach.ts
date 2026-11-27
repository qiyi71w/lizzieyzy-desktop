// AI 陪练's coach section (Java CoachSummaryView, CoachHistoryView, CoachPanelState): the player's
// win chance with its bar and curve, in 边下边提醒 the count of each rating and the latest rated
// moves, after the game the three worst mistakes, which end the session at the position before
// them. Unlike Java it is no side panel: it takes the 胜率 panel's rectangle, as a card of its own
// where the training mode hides that panel and as the panel's contents in 边下边分析; 实战后复盘
// shows a note there instead. Where the workbench has no room for more than the section's title,
// or after the game for every one of the worst mistakes, the training bar offers the section in a
// popover over the board. Every figure is the core's.
import { run } from "../app";
import type { HumanSlView, MoveFeedback, WorkbenchView } from "../contract/generated";
import { icon } from "../design/icons";
import { format, t } from "../i18n";
import type { Rect } from "../workbench/layout";
import { coachLevel, humanSlGameActive, humanSlSession, levelFox } from "./state";
import { VERDICTS, VERDICT_KEYS, curveGeometry, isMistake, recentRatings, signedPercent } from "./summary";
import "./humansl.css";

const SVG = "http://www.w3.org/2000/svg";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

function svg(tag: string, attrs: Record<string, string | number>): SVGElement {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  return e;
}

/** Enter and Space press the focused button inside `root`, once per press, as a button takes
 *  them; they never reach the workbench's keys (Space would toggle the analysis there and Enter
 *  is refused). */
export function keepButtonKeys(root: HTMLElement): void {
  root.addEventListener("keydown", (e) => {
    const button = e.target;
    if (!(button instanceof HTMLButtonElement) || (e.key !== "Enter" && e.key !== " ") || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
    e.stopPropagation();
    e.preventDefault();
    if (!e.repeat && !button.disabled) button.click();
  });
}

/** The section's place: the 胜率 panel's rectangle, `own` when the training mode hides that panel
 *  and the section is a card of its own there, not the shown panel's contents. */
export interface CoachSlot {
  rect: Rect;
  own: boolean;
}

/** What the section takes of the workbench: a card of its own to draw (the section or 实战后复盘's
 *  note when it fits), and whether it stands in for the 胜率 panel's contents. */
export interface CoachPlacement {
  card: Rect | null;
  replacesGraph: boolean;
}

/** The modes with a section: 实战后复盘 shows no win chance (CoachPanelState.winChance). */
function hasSection(s: HumanSlView): boolean {
  return s.mode !== "PostGameReview";
}

/** A move as the rows name it, 停一手 for a pass. */
function moveName(vertex: string): string {
  return vertex === "pass" ? t("HumanSlGame.btn.pass") : vertex;
}

/** "提 3": a sentence with its figure in bold, the translators' whole sentence. */
export function boldFigure(template: string, value: string | number): DocumentFragment {
  const f = document.createDocumentFragment();
  const [before, ...after] = template.split("{0}");
  if (before) f.append(before);
  if (after.length) {
    f.append(el("b", "", String(value)));
    if (after.join("{0}")) f.append(after.join("{0}"));
  }
  return f;
}

let clipIds = 0;

/** The curve as the 胜率 panel draws it, at the box's real size: the 25 % and 75 % hairlines, the
 *  dashed 50 % line, the areas above and below 50 %, the line and the ringed last point; with
 *  `labels` the 75 / 50 / 25 % ticks are named at the left, as the panel does. */
function drawCurve(box: HTMLElement, values: readonly number[], labels: boolean): void {
  const w = box.clientWidth;
  const h = box.clientHeight;
  box.replaceChildren();
  box.dataset.points = String(values.length);
  if (w <= 0 || h <= 0) return;
  const g = curveGeometry(values, w, h);
  const root = svg("svg", { width: w, height: h, viewBox: `0 0 ${w} ${h}`, "aria-hidden": "true" });
  const id = `hs-curve-${++clipIds}`;
  const defs = svg("defs", {});
  const above = svg("clipPath", { id: `${id}-above` });
  above.append(svg("rect", { x: 0, y: 0, width: w, height: g.y50 }));
  const below = svg("clipPath", { id: `${id}-below` });
  below.append(svg("rect", { x: 0, y: g.y50, width: w, height: Math.max(0, h - g.y50) }));
  defs.append(above, below);
  root.append(defs);
  for (const y of [g.y25, g.y75]) root.append(svg("line", { x1: 0, x2: w, y1: y, y2: y, class: "hs-curve-grid" }));
  root.append(svg("line", { x1: 0, x2: w, y1: g.y50, y2: g.y50, class: "hs-curve-base" }));
  if (labels) {
    for (const [v, y] of [[75, g.y75], [50, g.y50], [25, g.y25]]) {
      const text = svg("text", { x: 2, y: y - 3, class: "hs-curve-label" });
      text.textContent = `${v}%`;
      root.append(text);
    }
  }
  if (g.area) {
    root.append(
      svg("path", { d: g.area, class: "hs-curve-above", "clip-path": `url(#${id}-above)` }),
      svg("path", { d: g.area, class: "hs-curve-below", "clip-path": `url(#${id}-below)` }),
      svg("path", { d: g.line, class: "hs-curve-line" }),
    );
  }
  if (values.length) {
    root.append(
      svg("circle", { cx: g.last.x, cy: g.last.y, r: 4.5, class: "hs-curve-ring" }),
      svg("circle", { cx: g.last.x, cy: g.last.y, r: 3, class: "hs-curve-point" }),
    );
  }
  box.append(root);
}

/** One row of the history: the move number, the move, its rating and the change it made. */
function row(m: MoveFeedback, pick: boolean, latest: boolean): HTMLElement {
  const r = pick ? el("button", "hs-row") : el("div", "hs-row");
  if (latest) r.classList.add("latest");
  if (isMistake(m.verdict)) r.dataset.mistake = "";
  const tag = el("span", "hs-tag", t(VERDICT_KEYS[m.verdict]));
  tag.dataset.verdict = m.verdict;
  r.append(
    el("span", "hs-row-n", `#${m.move_number}`),
    el("span", "hs-row-move", moveName(m.vertex)),
    tag,
    el("span", "hs-row-change", signedPercent(m.win_after - m.win_before)),
  );
  if (r instanceof HTMLButtonElement) {
    r.type = "button";
    r.dataset.key = "humansl.coach.mistake";
    r.title = format(t("CoachPanel.moveAt"), m.move_number, moveName(m.vertex));
    // CoachPanel.reviewFrom: the session ends, then the board shows the position before the move.
    r.onclick = () => void run({ type: "HumanSl", op: { type: "ReviewFrom", move_number: m.move_number } });
  }
  return r;
}

/** The section for `s`: the head, the bar and the curve (drawn once the section has its size),
 *  then in 边下边提醒 the count of each rating and the latest rated moves, or after the game the
 *  worst mistakes. */
function buildSection(s: HumanSlView): HTMLElement {
  const card = el("section", "hs-card");
  card.dataset.mode = s.mode;
  if (s.mode === "LiveAnalysis") card.classList.add("fill");
  const values = s.win_chances ?? [];
  const last = values.length ? values[values.length - 1] : null;
  const level = coachLevel(s.level);
  const head = el("div", "hs-wc-head");
  head.append(
    el("span", "hs-wc-title", t("CoachPanel.winChance")),
    el("span", "hs-wc-caption", s.mode === "LiveAnalysis" ? t("CoachPanel.winChanceLive") : format(t("CoachPanel.winChanceRank"), level ? levelFox(level) : "")),
    el("span", "hs-wc-value", last === null ? "—" : `${Math.round(last)}%`),
  );
  // The player's share against the AI's in their stones' colours, the centre ticked; while the AI
  // holds its reply to a 恶手, the chance before it is marked.
  const bar = el("div", "hs-wc-bar");
  bar.dataset.player = s.human_is_black ? "Black" : "White";
  if (last === null) bar.dataset.empty = "";
  else {
    const fill = el("div", "hs-wc-fill");
    fill.style.width = `${Math.min(100, Math.max(0, last))}%`;
    bar.append(fill);
  }
  bar.append(el("div", "hs-wc-tick"));
  const decision = s.feedback?.decision;
  if (decision) {
    const mark = el("div", "hs-wc-mark");
    mark.style.left = `${decision.rating.win_before * 100}%`;
    bar.append(mark);
  }
  const curve = el("div", "hs-wc-curve");
  card.append(head, bar, curve);
  const f = s.feedback;
  if (!f) return card;
  const tally = el("div", "hs-tally");
  VERDICTS.forEach((v, i) => {
    const cell = el("div");
    cell.dataset.verdict = v;
    const n = el("span", "hs-tally-n", String(f.counts[i]));
    if (!f.counts[i]) n.dataset.zero = "";
    cell.append(n, el("span", "hs-tally-l", t(VERDICT_KEYS[v])));
    tally.append(cell);
  });
  card.append(tally);
  const over = s.result !== null;
  const moves = over ? f.worst : recentRatings(f.ratings);
  if (moves.length) {
    const hist = el("div", "hs-hist");
    hist.append(el("div", "hs-hist-head", t(over ? "CoachPanel.keyMoves" : "CoachPanel.recentMoves")));
    moves.forEach((m, i) => hist.append(row(m, over, !over && i === 0)));
    card.append(hist);
  }
  return card;
}

/** As many history rows as fit (CoachHistoryView.rowsThatFit); with less room the history, the
 *  count, the curve and the bar go in that order. Whether the curve stayed. */
function fit(card: HTMLElement): boolean {
  const overflows = () => card.scrollHeight > card.clientHeight + 1;
  const rows = [...card.querySelectorAll(".hs-row")];
  while (overflows() && rows.length) rows.pop()!.remove();
  for (const selector of [".hs-hist", ".hs-tally", ".hs-wc-curve", ".hs-wc-bar"]) {
    const part = card.querySelector(selector);
    if (part && overflows()) part.remove();
  }
  return !!card.querySelector(".hs-wc-curve");
}

function drawCurves(card: HTMLElement, s: HumanSlView): void {
  const box = card.querySelector<HTMLElement>(".hs-wc-curve");
  if (box) drawCurve(box, s.win_chances ?? [], card.classList.contains("fill"));
}

/** What the section shows, so it is built again only when that changes. */
function sectionSignature(s: HumanSlView): string {
  const f = s.feedback;
  return JSON.stringify([s.mode, s.level, s.human_is_black, s.result !== null, s.win_chances, f && [f.counts, f.worst, recentRatings(f.ratings), f.decision?.rating.win_before]]);
}

// ---------------------------------------------------------------- on the workbench

let host: HTMLElement | null = null;
let note: HTMLElement | null = null;
let placed: { sig: string; placement: CoachPlacement } = { sig: "", placement: { card: null, replacesGraph: false } };
let inPopover = false;
const popoverListeners: (() => void)[] = [];

function place(e: HTMLElement, r: Rect): void {
  e.style.left = `${r.x}px`;
  e.style.top = `${r.y}px`;
  e.style.width = `${r.w}px`;
  e.style.height = `${r.h}px`;
}

function setInPopover(on: boolean): void {
  if (inPopover === on) return;
  inPopover = on;
  if (!on) closeCoachPopover(false);
  popoverListeners.forEach((l) => l());
}

/** The bar offers the section in a popover: the game's mode has one and the workbench has no room
 *  for it, or after the game no room for every one of the worst mistakes. */
export function coachInPopover(): boolean {
  return inPopover;
}

export function onCoachPopoverChange(listener: () => void): void {
  popoverListeners.push(listener);
}

/** Puts the section, or 实战后复盘's note, into `slot` of `main` for the view (none without a game
 *  or a slot). `look` changes with anything that changes the text's size (the font, the language,
 *  the appearance). Built and fitted again only when the slot, the look or the figures change. */
export function placeCoach(main: HTMLElement, v: WorkbenchView, slot: CoachSlot | null, look: string): CoachPlacement {
  const s = humanSlGameActive(v) ? humanSlSession(v) : null;
  if (!host) {
    host = el("div", "hs-card-host");
    host.dataset.name = "humanSlCoachCard";
    keepButtonKeys(host);
    note = el("div", "hs-gap-note");
    note.dataset.name = "humanSlGapNote";
    main.append(host, note);
  }
  const sig = JSON.stringify([s && sectionSignature(s), s?.mode, slot, look]);
  if (sig !== placed.sig) placed = { sig, placement: layOut(s, slot) };
  refreshPopover(s);
  return placed.placement;
}

function layOut(s: HumanSlView | null, slot: CoachSlot | null): CoachPlacement {
  host!.hidden = true;
  note!.hidden = true;
  if (!s) {
    setInPopover(false);
    return { card: null, replacesGraph: false };
  }
  if (!hasSection(s)) {
    setInPopover(false);
    if (!slot) return { card: null, replacesGraph: false };
    // 实战后复盘: the hidden panel's place says why it is empty, when the sentence fits there.
    note!.textContent = t("HumanSlGame.analysisHidden");
    place(note!, slot.rect);
    note!.hidden = false;
    const fits = note!.scrollHeight <= note!.clientHeight + 1 && note!.scrollWidth <= note!.clientWidth + 1;
    note!.hidden = !fits;
    return { card: fits ? slot.rect : null, replacesGraph: false };
  }
  if (!slot) {
    setInPopover(true);
    return { card: null, replacesGraph: false };
  }
  const card = buildSection(s);
  host!.replaceChildren(card);
  host!.classList.toggle("own", slot.own);
  place(host!, slot.rect);
  host!.hidden = false;
  const fits = fit(card);
  host!.hidden = !fits;
  if (fits) drawCurves(card, s);
  // After the game each of the worst mistakes leads into the review: where the card has room for
  // fewer, the popover has them all. With room for none, the list's heading ("click one") is not
  // drawn; its line stays, so nothing else on the card moves.
  const mistakes = s.result !== null ? s.feedback?.worst.length ?? 0 : 0;
  const kept = card.querySelectorAll("button.hs-row").length;
  const hist = card.querySelector<HTMLElement>(".hs-hist");
  if (mistakes && !kept && hist) hist.style.visibility = "hidden";
  setInPopover(!fits || kept < mistakes);
  // Without room the 胜率 panel keeps its own contents and the bar offers the section.
  return { card: fits && slot.own ? slot.rect : null, replacesGraph: fits && !slot.own };
}

/** What the acceptance probe reads of the section: where it is, what it shows and what it left
 *  out for room, and the popover. */
export function coachReport() {
  const section = (root: Element | null) => {
    const card = root?.querySelector<HTMLElement>(".hs-card");
    if (!card) return null;
    const box = card.getBoundingClientRect();
    const curve = card.querySelector<HTMLElement>(".hs-wc-curve");
    const c = curve?.getBoundingClientRect();
    return {
      box: { x: Math.round(box.x), y: Math.round(box.y), w: Math.round(box.width), h: Math.round(box.height) },
      mode: card.dataset.mode,
      title: card.querySelector(".hs-wc-title")?.textContent ?? "",
      caption: card.querySelector(".hs-wc-caption")?.textContent ?? "",
      value: card.querySelector(".hs-wc-value")?.textContent ?? "",
      bar: !!card.querySelector(".hs-wc-bar"),
      curve: c ? { w: Math.round(c.width), h: Math.round(c.height), points: Number(curve!.dataset.points ?? 0), labels: card.querySelectorAll(".hs-curve-label").length } : null,
      counts: [...card.querySelectorAll(".hs-tally-n")].map((n) => Number(n.textContent)),
      heading: (() => {
        const head = card.querySelector(".hs-hist-head");
        return head && getComputedStyle(head).visibility === "visible" ? head.textContent : null;
      })(),
      rows: [...card.querySelectorAll(".hs-row")].map((r) => r.textContent ?? ""),
      overflows: card.scrollHeight > card.clientHeight + 1,
    };
  };
  return {
    card: host && !host.hidden ? section(host) : null,
    note: note && !note.hidden ? { text: note.textContent ?? "", box: rectOf(note) } : null,
    inPopover,
    popover: pop ? { box: rectOf(pop), scrolls: pop.scrollHeight > pop.clientHeight + 1, label: pop.getAttribute("aria-label"), section: section(pop), players: pop.querySelector(".hs-pop-players")?.textContent ?? "" } : null,
  };
}

function rectOf(e: Element) {
  const r = e.getBoundingClientRect();
  return { x: Math.round(r.x), y: Math.round(r.y), w: Math.round(r.width), h: Math.round(r.height) };
}

// ---------------------------------------------------------------- the popover

let pop: HTMLElement | null = null;
let trigger: HTMLElement | null = null;
let popSig = "";

/** Opens the section over the board above `from`, the bar's button, or closes it. */
export function toggleCoachPopover(from: HTMLElement): void {
  if (pop) {
    closeCoachPopover(true);
    return;
  }
  const s = humanSlSession();
  if (!s || !hasSection(s)) return;
  trigger = from;
  pop = el("div", "hs-pop");
  pop.id = "humanslCoachPopover";
  pop.dataset.name = "humanSlCoachPopover";
  pop.tabIndex = -1;
  pop.setAttribute("role", "dialog");
  keepButtonKeys(pop);
  pop.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeCoachPopover(true);
    }
  });
  // Focus moving out of it (Tab past its last row, a click elsewhere) closes it.
  pop.addEventListener("focusout", (e) => {
    const to = e.relatedTarget as Node | null;
    if (to && !pop?.contains(to) && to !== trigger) closeCoachPopover(false);
  });
  document.addEventListener("pointerdown", outside, true);
  window.addEventListener("resize", position);
  document.body.append(pop);
  from.setAttribute("aria-expanded", "true");
  from.setAttribute("aria-controls", pop.id);
  popSig = "";
  refreshPopover(s);
  pop.focus();
}

function outside(e: PointerEvent): void {
  const target = e.target as Node;
  if (pop && !pop.contains(target) && !trigger?.contains(target)) closeCoachPopover(false);
}

/** Closes the popover; with `refocus` the focus goes back to the bar's button. */
export function closeCoachPopover(refocus: boolean): void {
  if (!pop) return;
  document.removeEventListener("pointerdown", outside, true);
  window.removeEventListener("resize", position);
  pop.remove();
  pop = null;
  trigger?.setAttribute("aria-expanded", "false");
  if (refocus) trigger?.focus();
  trigger = null;
}

/** The popover follows the view: the section and the players' captures and komi (the 提子 panel
 *  has no room either), built again when they change, above its button. */
function refreshPopover(s: HumanSlView | null): void {
  if (!pop) return;
  if (!s || !hasSection(s) || !inPopover || !trigger?.isConnected) {
    closeCoachPopover(false);
    return;
  }
  const sig = JSON.stringify([sectionSignature(s), s.black_captures, s.white_captures, s.komi]);
  if (sig !== popSig) {
    popSig = sig;
    pop.setAttribute("aria-label", t("HumanSlTraining.coachDetails"));
    const card = buildSection(s);
    card.prepend(playersLine(s));
    pop.replaceChildren(card);
  }
  position();
}

function playersLine(s: HumanSlView): HTMLElement {
  const line = el("div", "hs-pop-players");
  const black = t("CoachPanel.black");
  const white = t("CoachPanel.white");
  const [mine, theirs] = s.human_is_black ? [s.black_captures, s.white_captures] : [s.white_captures, s.black_captures];
  const sep = () => el("span", "hs-sep", "·");
  const part = (label: string, figure: Node | string) => {
    const span = el("span");
    span.append(`${label} `, figure);
    return span;
  };
  const komi = Number.isInteger(s.komi) ? String(s.komi) : s.komi.toFixed(1);
  const bold = el("b", "", komi);
  line.append(
    part(`${t("CoachPanel.you")} · ${s.human_is_black ? black : white}`, boldFigure(t("CoachPanel.captures"), mine)),
    sep(),
    // Java `CoachPanelState.of` writes "AI" in every language.
    part(`AI · ${s.human_is_black ? white : black}`, boldFigure(t("CoachPanel.captures"), theirs)),
    sep(),
    part(t("HumanSlTraining.komi"), bold),
  );
  return line;
}

/** Above the button, its right edge with the button's where the window allows, as tall as the room
 *  above it; the contents scroll inside when taller. The curve is drawn at its size, with the
 *  panel's ticks in 边下边分析 as on the workbench. */
function position(): void {
  if (!pop || !trigger) return;
  const b = trigger.getBoundingClientRect();
  const width = Math.min(380, window.innerWidth - 16);
  pop.style.width = `${width}px`;
  pop.style.left = `${Math.max(8, Math.min(window.innerWidth - 8 - width, b.right - width))}px`;
  pop.style.bottom = `${window.innerHeight - b.top + 6}px`;
  pop.style.maxHeight = `${Math.max(0, b.top - 14)}px`;
  const box = pop.querySelector<HTMLElement>(".hs-wc-curve");
  const s = humanSlSession();
  if (box && s && box.dataset.size !== `${box.clientWidth}x${box.clientHeight}`) {
    drawCurve(box, s.win_chances ?? [], !!box.closest(".hs-card.fill"));
    box.dataset.size = `${box.clientWidth}x${box.clientHeight}`;
  }
}

/** The bar's button that opens the popover: the chevron after the text it carries. */
export function chevron(): Element {
  const c = icon("chevron-up");
  c.classList.add("hs-chev");
  return c;
}
