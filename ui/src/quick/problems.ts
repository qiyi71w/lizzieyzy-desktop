// 评论 / 问题手 panel (SidebarPanel, SidebarHeaderPanel classic layout, BlunderListPanel). The core
// owns the list, its order and the persisted panel state; this module draws the header, the
// toolbar and the rows, and sends the page, side, sort and jump commands.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { ProblemListSettings, ProblemMoveView, ProblemSort, WorkbenchView } from "../contract/generated";
import { format, t } from "../i18n";
import "./quick.css";

const $ = <T extends HTMLElement>(id: string) => document.getElementById(id) as T;

/** The row chosen in the list (JList selection), by node; a new record clears it. */
let selected: number | null = null;
let selectedDocument = -1;
/** Re-sorting scrolls to the selection, else to the top (BlunderListPanel sort listener). */
let resorted = false;
let drawn = "";
let shownRows: ProblemMoveView[] = [];

/** One change to the panel state; the rest is sent as it is. */
function setProblems(change: Partial<ProblemListSettings>): void {
  const settings = state.view?.settings.problems;
  if (settings) void run({ type: "SetProblemList", settings: { ...settings, ...change } });
}

/** jumpToProblemMove: the position before the move, with the move marked on the board. */
function jump(row: ProblemMoveView): void {
  selected = row.node;
  void run({ type: "JumpToProblemMove", node: row.node });
}

/** SidebarHeaderPanel.progressLabelFor: hidden without moves to evaluate. */
function progressLabel(v: WorkbenchView): string {
  const p = v.problems;
  if (p.total <= 0) return "";
  const analyzed = Math.max(0, Math.min(p.analyzed, p.total));
  if (p.running) return format(t("SidebarHeader.progress.running"), analyzed, p.total);
  if (analyzed >= p.total) return t("SidebarHeader.progress.complete");
  return format(t("SidebarHeader.progress.partial"), analyzed, p.total);
}

/** BlunderListPanel.emptyStateText, in Java's order. */
function emptyText(v: WorkbenchView): string {
  if (v.problems.running) return t("BlunderListPanel.organizing");
  if (v.problems.analyzed === 0) return t("BlunderListPanel.unanalyzed");
  return t("BlunderListPanel.filteredEmpty");
}

/** NumberFormat with one fraction digit and no grouping. */
function decimal(value: number): string {
  return value.toFixed(1);
}

/** Board.convertCoordinatesToName. */
function coordinate(row: ProblemMoveView, width: number, height: number): string {
  if (width > 25 || height > 25) return `(${row.point.x},${row.point.y})`;
  return `${"ABCDEFGHJKLMNOPQRSTUVWXYZ"[row.point.x]}${height - row.point.y}`;
}

function moveLabel(row: ProblemMoveView, v: WorkbenchView): string {
  const side = row.mover === "Black" ? t("SidebarHeader.black") : t("SidebarHeader.white");
  return format(t("BlunderListPanel.move"), row.move_number, side, coordinate(row, v.document.size.width, v.document.size.height));
}

function detailLabel(row: ProblemMoveView): string {
  const searches = format(t("BlunderListPanel.searches"), row.playouts);
  return row.score_loss === null ? searches : `${format(t("BlunderListPanel.score"), decimal(row.score_loss))}  \u00b7  ${searches}`;
}

function span(className: string, text: string): HTMLSpanElement {
  const el = document.createElement("span");
  el.className = className;
  el.textContent = text;
  return el;
}

/** ProblemRow: ordinal, move title, detail, loss and its caption (stacked on a narrow panel). */
function rowElement(row: ProblemMoveView, index: number, v: WorkbenchView): HTMLElement {
  const el = document.createElement("div");
  el.className = "problem-row";
  el.dataset.key = "problems.row";
  el.dataset.node = String(row.node);
  el.classList.toggle("current", row.current);
  el.classList.toggle("selected", row.node === selected);
  const title = moveLabel(row, v);
  const detail = detailLabel(row);
  const loss = `${decimal(row.winrate_loss)}%`;
  el.title = format(t("BlunderListPanel.row"), index + 1, title, decimal(row.winrate_loss), detail);
  const lossEl = span("problem-loss", loss);
  lossEl.classList.toggle("severe", row.tier >= 5);
  el.append(
    span("problem-ordinal", String(index + 1)),
    span("problem-title", title),
    span("problem-detail", detail),
    lossEl,
    span("problem-caption", t("BlunderListPanel.loss")),
    span("problem-caption-stacked", format(t("BlunderListPanel.lossValue"), loss)),
  );
  return el;
}

function select(node: number | null): void {
  selected = node;
  for (const el of $("problemList").children) (el as HTMLElement).classList.toggle("selected", (el as HTMLElement).dataset.node === String(node));
  $("problemList").querySelector<HTMLElement>(".problem-row.selected")?.scrollIntoView({ block: "nearest" });
}

/** Header keys (SidebarHeaderPanel.installKeyboardActions): Left / Right pick the page, Enter
 *  toggles it, B / W pick the side while problems show. */
function headerKey(e: KeyboardEvent): void {
  const s = state.view?.settings.problems;
  if (!s || e.ctrlKey || e.metaKey || e.altKey) return;
  const key = e.key.length === 1 ? e.key.toUpperCase() : e.key;
  if (key === "ArrowLeft") setProblems({ showing: false });
  else if (key === "ArrowRight") setProblems({ showing: true });
  else if (key === "Enter") setProblems({ showing: !s.showing });
  else if (key === "B" && s.showing) setProblems({ side: "Black" });
  else if (key === "W" && s.showing) setProblems({ side: "White" });
  else return;
  e.preventDefault();
  e.stopPropagation();
}

/** List keys: Up / Down move the selection, Enter / Space open the selected move. */
function listKey(e: KeyboardEvent): void {
  if (e.ctrlKey || e.metaKey || e.altKey || !shownRows.length) return;
  const at = shownRows.findIndex((r) => r.node === selected);
  if (e.key === "ArrowUp") select(shownRows[Math.max(0, at - 1)].node);
  else if (e.key === "ArrowDown") select(shownRows[Math.min(shownRows.length - 1, at + 1)].node);
  else if (e.key === "Enter" || e.key === " ") {
    const row = shownRows[at];
    if (row) jump(row);
  } else return;
  e.preventDefault();
  e.stopPropagation();
}

export function bindProblemPanel(): void {
  const tabs = $("commentTabs");
  $("commentsTab").textContent = t("SidebarHeader.comments");
  $("problemsTab").textContent = t("SidebarHeader.problems");
  $("blackSide").textContent = t("SidebarHeader.black");
  $("whiteSide").textContent = t("SidebarHeader.white");
  tabs.setAttribute("aria-label", t("SidebarHeader.accessibleName"));
  tabs.setAttribute("aria-description", t("SidebarHeader.accessibleDescription"));
  $("commentsTab").addEventListener("click", () => setProblems({ showing: false }));
  $("problemsTab").addEventListener("click", () => setProblems({ showing: true }));
  $("blackSide").addEventListener("click", () => setProblems({ side: "Black" }));
  $("whiteSide").addEventListener("click", () => setProblems({ side: "White" }));
  tabs.addEventListener("keydown", headerKey);
  const sort = $<HTMLSelectElement>("problemSort");
  sort.title = t("BlunderListPanel.sort");
  sort.setAttribute("aria-label", t("BlunderListPanel.sort"));
  // ProblemListSort (ProblemListSort.java:5-33).
  const sorts: [ProblemSort, string][] = [
    ["LossDesc", t("BlunderListPanel.sort.loss")],
    ["MoveAsc", t("BlunderListPanel.sort.earliest")],
    ["MoveDesc", t("BlunderListPanel.sort.latest")],
  ];
  for (const [value, text] of sorts) {
    const option = document.createElement("option");
    option.value = value;
    option.textContent = text;
    sort.append(option);
  }
  sort.addEventListener("change", () => {
    resorted = true;
    setProblems({ sort: sort.value as ProblemSort });
  });
  const list = $("problemList");
  list.setAttribute("aria-label", t("SidebarHeader.problems"));
  list.addEventListener("keydown", listKey);
  // A left press and release on the same row navigates (BlunderListPanel mouseReleased); rows are
  // found by node because a view may redraw them in between.
  let pressed: string | undefined;
  const nodeAt = (e: MouseEvent) => (e.target as Element).closest<HTMLElement>(".problem-row")?.dataset.node;
  list.addEventListener("mousedown", (e) => { pressed = e.button === 0 ? nodeAt(e) : undefined; });
  list.addEventListener("mouseup", (e) => {
    const node = nodeAt(e);
    const row = e.button === 0 && node !== undefined && node === pressed ? shownRows.find((r) => String(r.node) === node) : undefined;
    pressed = undefined;
    if (row) jump(row);
  });
  // Focus selects the first visible row without navigating.
  list.addEventListener("focus", () => {
    if (selected !== null && shownRows.some((r) => r.node === selected)) return;
    const top = list.getBoundingClientRect().top;
    const first = [...list.children].find((el) => el.getBoundingClientRect().bottom > top) as HTMLElement | undefined;
    if (first) select(Number(first.dataset.node));
  });
}

/** Redraws the header and the page shown when the panel state or the list changed. */
export function renderProblemPanel(v: WorkbenchView): void {
  const s = v.settings.problems;
  const p = v.problems;
  if (v.document.id !== selectedDocument) {
    selectedDocument = v.document.id;
    selected = null;
    $("problemList").scrollTop = 0;
  }
  const sig = JSON.stringify([s, p, selected, v.document.id, v.document.size]);
  if (sig === drawn) return;
  drawn = sig;
  $("commentsTab").classList.toggle("on", !s.showing);
  $("problemsTab").classList.toggle("on", s.showing);
  $("problemSides").hidden = !s.showing;
  $("blackSide").classList.toggle("on", s.side === "Black");
  $("whiteSide").classList.toggle("on", s.side === "White");
  const label = progressLabel(v);
  const progress = $("problemProgress");
  progress.hidden = label === "";
  progress.textContent = label;
  progress.title = label ? format(t("SidebarHeader.progress.tooltip"), label) : "";
  progress.classList.toggle("running", p.running);
  $("commentPage").hidden = s.showing;
  $("problemPage").hidden = !s.showing;
  const rows = s.side === "Black" ? p.black : p.white;
  shownRows = rows;
  $("problemCount").textContent = format(t("BlunderListPanel.count"), rows.length);
  $<HTMLSelectElement>("problemSort").value = s.sort;
  const list = $("problemList");
  const empty = $("problemEmpty");
  list.hidden = rows.length === 0;
  empty.hidden = rows.length > 0;
  empty.textContent = emptyText(v);
  if (selected !== null && !rows.some((r) => r.node === selected)) selected = null;
  const top = list.scrollTop;
  list.replaceChildren(...rows.map((row, k) => rowElement(row, k, v)));
  list.scrollTop = top;
  if (resorted) {
    resorted = false;
    const chosen = list.querySelector<HTMLElement>(".problem-row.selected");
    if (chosen) chosen.scrollIntoView({ block: "nearest" });
    else list.scrollTop = 0;
  }
}

/** What the panel shows, for the acceptance probe. */
export function problemsReport() {
  const s = state.view?.settings.problems;
  return {
    showing: s?.showing ?? false,
    side: s?.side ?? null,
    sort: s?.sort ?? null,
    hideControlPane: s?.hideControlPane ?? false,
    progress: $("problemProgress").hidden ? "" : $("problemProgress").textContent,
    count: $("problemCount").textContent,
    empty: $("problemEmpty").hidden ? null : $("problemEmpty").textContent,
    rows: shownRows.map((r) => ({ node: r.node, move: r.move_number, loss: r.winrate_loss, tier: r.tier, current: r.current, selected: r.node === selected })),
    mark: state.view?.problems.mark ?? null,
  };
}

// 显示评论/问题手面板顶部控制条 (Menu.java:1016-1025, check state 2783-2784): the flag only gates the
// legacy hover control pane, so the header itself stays.
Object.assign(COMMANDS, {
  "menu.showCommentConrolPane": {
    run: () => setProblems({ hideControlPane: !state.view?.settings.problems.hideControlPane }),
    checked: () => !(state.view?.settings.problems.hideControlPane ?? false),
  },
} satisfies Record<string, CommandEntry>);
