// AI 陪练's KataGo rank table (CoachRankTable.java), opened from 等级对照表 in the setup dialog:
// every opponent, strongest first, by its KataGo level, its Fox / Tygem rank and its real-world
// grade, equal grades merged into one cell. The table body is one Tab stop: ↑/↓ and Home/End move
// the pick, Enter or Space (or a click on a row) chooses it and closes the table; Escape, the close
// box and 关闭 close it without a choice.
import type { CoachLevel, CoachLevelView } from "../contract/generated";
import { closeBox, openModalLayer } from "../dialogs";
import { footer } from "../engine/controls";
import { t } from "../i18n";
import { levelGrade, levelKataGo } from "./state";

/** Rows left above the picked one when the table opens (Sheet.highlightBounds). */
const ROWS_ABOVE = 3;

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** CoachRankTable.Sheet.sameGrade. */
function sameGrade(a: CoachLevelView, b: CoachLevelView): boolean {
  return a.grade === b.grade && a.grade_number === b.grade_number;
}

/** Opens the table over the setup dialog with `selected` picked out. `onPick` gets the level the
 *  player chooses once the table has closed; `onClose` runs whenever it closes. Returns a function
 *  that closes it without a choice. */
export function openRankTable(
  levels: readonly CoachLevelView[],
  selected: CoachLevel,
  onPick: (level: CoachLevel) => void,
  onClose: () => void,
): () => void {
  const rows = [...levels].reverse();
  let highlight = rows.findIndex((l) => l.level === selected);

  const title = t("HumanSlTraining.rankTable.title");
  const overlay = el("div", "modal");
  const box = el("div", "modal-box hs-rank-dialog");
  box.dataset.key = "humansl.rankTable";
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", title);
  box.tabIndex = -1;
  const heading = el("div", "modal-title");
  heading.append(el("span", "", title), closeBox(() => close(null)));

  const note = el("p", "hs-rank-note", t("HumanSlTraining.rankTable.note"));
  note.id = "hs-rank-note";
  const scroll = el("div", "hs-rank-scroll");
  const table = el("table", "hs-rank-table");
  table.dataset.key = "humansl.rankTable.table";
  table.tabIndex = 0;
  table.setAttribute("role", "grid");
  table.setAttribute("aria-label", title);
  table.setAttribute("aria-describedby", note.id);
  const head = el("thead");
  const headRow = el("tr");
  for (const key of ["HumanSlTraining.rankTable.kataGo", "HumanSlTraining.rankTable.fox", "HumanSlTraining.rankTable.real"]) {
    headRow.append(el("th", "", t(key)));
  }
  // The header cells stick to the top of the body (the row itself scrolls away).
  headRow.cells[0].dataset.key = "humansl.rankTable.header";
  head.append(headRow);
  const body = el("tbody");
  const trs: HTMLTableRowElement[] = [];
  /** The merged grade cell of each row. */
  const gradeOf: HTMLTableCellElement[] = [];
  rows.forEach((level, i) => {
    const tr = el("tr");
    tr.id = `hs-rank-${level.level}`;
    tr.dataset.key = `humansl.rankTable.row.${level.level}`;
    tr.append(el("td", "", levelKataGo(level)), el("td", "", level.fox));
    if (i === 0 || !sameGrade(rows[i - 1], level)) {
      let span = 1;
      while (i + span < rows.length && sameGrade(rows[i + span], level)) span++;
      const grade = el("td", "grade", levelGrade(level));
      grade.rowSpan = span;
      grade.classList.toggle("last", i + span === rows.length);
      tr.append(grade);
      gradeOf.push(grade);
    } else gradeOf.push(gradeOf[i - 1]);
    trs.push(tr);
    body.append(tr);
  });
  table.append(head, body);
  scroll.append(table);
  // The note and the table; in a short window they scroll together (humansl.css), else the table
  // scrolls under the note.
  const middle = el("div", "hs-rank-body");
  middle.append(note, scroll);
  box.append(heading, middle);
  const closeButton = el("button", "", t("HumanSlTraining.rankTable.close"));
  closeButton.type = "button";
  closeButton.dataset.key = "humansl.rankTable.close";
  closeButton.onclick = () => close(null);
  footer(box, { cancel: closeButton });
  overlay.append(box);

  /** The picked row and its grade cell in the selected style; the grid's active row follows. */
  function paint(): void {
    trs.forEach((tr, i) => {
      const on = i === highlight;
      tr.classList.toggle("selected", on);
      tr.setAttribute("aria-selected", String(on));
    });
    for (const cell of new Set(gradeOf)) cell.classList.toggle("selected", highlight >= 0 && gradeOf[highlight] === cell);
    if (highlight >= 0) table.setAttribute("aria-activedescendant", trs[highlight].id);
    else table.removeAttribute("aria-activedescendant");
  }

  /** The element that scrolls the rows: the table's own body, or in a short window the note and
   *  the table together. */
  function scroller(): HTMLElement {
    return getComputedStyle(scroll).overflowY === "visible" ? middle : scroll;
  }

  /** Scrolls just enough that `tr` shows whole below the sticky header; with `above`, so that it
   *  sits that many rows below the header (as many as leave it in view). */
  function reveal(tr: HTMLTableRowElement, above = 0): void {
    const view = scroller();
    const frame = view.getBoundingClientRect();
    const r = tr.getBoundingClientRect();
    const top = frame.top + head.getBoundingClientRect().height;
    if (r.top < top) view.scrollTop -= top - r.top;
    else if (r.bottom > frame.bottom) view.scrollTop += r.bottom - frame.bottom;
    if (above > 0) {
      // As many of `above` as leave the row itself in view.
      const fit = Math.floor((frame.bottom - top) / r.height) - 1;
      view.scrollTop += tr.getBoundingClientRect().top - top - Math.max(0, Math.min(above, fit)) * r.height;
    }
  }

  function move(to: number): void {
    const next = Math.max(0, Math.min(rows.length - 1, to));
    if (next === highlight) return;
    highlight = next;
    paint();
    reveal(trs[next]);
  }

  let closed = false;
  function close(pick: CoachLevel | null): void {
    if (closed) return;
    closed = true;
    closeLayer();
    onClose();
    if (pick) onPick(pick);
  }

  table.addEventListener("keydown", (e) => {
    const at = highlight < 0 ? 0 : highlight;
    if (e.key === "ArrowDown") move(highlight < 0 ? 0 : at + 1);
    else if (e.key === "ArrowUp") move(highlight < 0 ? 0 : at - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(rows.length - 1);
    else if (e.key === "Enter" || e.key === " ") {
      if (highlight >= 0) close(rows[highlight].level);
    } else return;
    e.preventDefault();
  });
  // A click picks the row under the pointer; on a merged grade cell, the row at that height.
  body.addEventListener("click", (e) => {
    const at = trs.findIndex((tr) => {
      const r = tr.getBoundingClientRect();
      return e.clientY >= r.top && e.clientY < r.bottom;
    });
    if (at >= 0) close(rows[at].level);
  });
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Escape") {
      e.preventDefault();
      close(null);
    }
  });

  const closeLayer = openModalLayer(overlay);
  paint();
  // The picked row near the top, a few rows above it (Sheet.highlightBounds).
  if (highlight >= 0) reveal(trs[highlight], ROWS_ABOVE);
  table.focus({ preventScroll: true });
  return () => close(null);
}
