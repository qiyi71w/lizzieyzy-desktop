// 终局点目: the ScoreResult dialog (ScoreResult.java) beside the board while scoring. The core
// owns the marks, the count and the result; this window shows them in Java's layout and words.
// It is non-modal: the board stays usable for marking dead stones and 不计入 points.
import { run, state, subscribe } from "../app";
import { setDisplay } from "../commands";
import type { Point, ScoreView } from "../contract/generated";
import { message } from "../dialogs";
import { t } from "../i18n";
import { javaDouble } from "../workbench/paint";

let box: HTMLElement | null = null;
let lines: { hint: HTMLElement; rule: HTMLElement; black: HTMLElement; white: HTMLElement; result: HTMLElement } | null = null;
let toggleRules: HTMLButtonElement | null = null;
/** setVisible state: shown when scoring starts and after each mark, hidden by 确定. */
let visible = false;
/** The window has its place until it is hidden (packAndPosition runs only while not showing). */
let placed = false;
let wasScoring = false;
let boardRect: () => DOMRect | null = () => null;

/** ScoreResult.setScore's lines in Java's formats: the counts, komi as String.valueOf(double),
 *  the totals as doubles and the margin with one decimal. */
export function scoreLines(s: ScoreView): { rule: string; black: string; white: string; result: string; blackWins: boolean } {
  const blackWins = s.black_total >= s.white_total;
  const komi = javaDouble(s.komi);
  const black = s.territory
    ? `${t("ScoreResult.lblBlackScore")}${s.black_points}-${s.white_captures}=${javaDouble(s.black_total)}`
    : `${t("ScoreResult.lblBlackScore")}${s.black_alive}+${s.black_points}=${javaDouble(s.black_total)}`;
  const white = s.territory
    ? `${t("ScoreResult.lblWhiteScore")}${s.white_points}-${s.black_captures}+${komi}=${javaDouble(s.white_total)}`
    : `${t("ScoreResult.lblWhiteScore")}${s.white_alive}+${s.white_points}+${komi}=${javaDouble(s.white_total)}`;
  const result = `${blackWins ? t("ScoreResult.blackWin") : t("ScoreResult.whiteWin")}${Math.abs(s.black_total - s.white_total).toFixed(1)}${t("ScoreResult.points")}`;
  const rule = s.territory ? t("ScoreResult.lblUseTerritoryScoring") : t("ScoreResult.lblUseAreaScoring");
  return { rule, black, white, result, blackWins };
}

function line(text = ""): HTMLElement {
  const el = document.createElement("div");
  el.textContent = text;
  // ScoreTextLine: 4 px horizontal and 9 px vertical padding, frame font.
  el.style.cssText = "padding:9px 4px;white-space:nowrap;font-size:12px;color:#000";
  return el;
}

function button(text: string, onClick: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  b.textContent = text;
  b.tabIndex = -1; // setFocusable(false)
  b.onclick = onClick;
  return b;
}

function build(): HTMLElement {
  const section = document.createElement("section");
  section.className = "modal-box score-result";
  section.setAttribute("role", "dialog");
  section.style.cssText = "position:fixed;z-index:25;min-width:0;max-width:none;background:rgb(210,210,210);padding:0 0 16px";
  const title = document.createElement("div");
  title.className = "modal-title";
  title.style.cssText = "display:flex;align-items:center;justify-content:space-between;gap:12px;cursor:default;user-select:none";
  const caption = document.createElement("span");
  caption.textContent = t("ScoreResult.title");
  // The window's close box: setVisible(false) and endFinalScore.
  const close = button("×", leave);
  close.style.cssText = "border:none;background:none;font-size:14px;line-height:1;padding:0 2px";
  title.append(caption, close);
  dragBy(title, section);
  const body = document.createElement("div");
  body.className = "modal-body";
  body.style.cssText = "padding:28px 22px 0;white-space:normal";
  const hint = line(t("ScoreResult.hint"));
  hint.style.color = "rgb(30,100,200)";
  hint.style.marginBottom = "8px";
  lines = { hint, rule: line(), black: line(), white: line(), result: line() };
  lines.rule.style.marginBottom = "4px";
  lines.black.style.marginBottom = "4px";
  lines.white.style.marginBottom = "8px";
  body.append(hint, lines.rule, lines.black, lines.white, lines.result);
  const buttons = document.createElement("div");
  buttons.className = "modal-buttons";
  buttons.style.cssText = "justify-content:center;gap:5px;padding:13px 22px 0";
  toggleRules = button("", () => setDisplay({ territoryScoring: !state.view?.settings.display.territoryScoring }));
  buttons.append(
    toggleRules,
    // setVisible(false) + startFinalScore: the marks start over and the window comes back.
    button(t("ScoreResult.btnRecalculate"), () => {
      placed = false;
      visible = true;
      void run({ type: "Score", op: { type: "Recalculate" } });
      refresh();
    }),
    button(t("ScoreResult.confirmResult"), confirmResult),
    button(t("ScoreResult.btnClose"), leave),
  );
  section.append(title, body, buttons);
  return section;
}

/** The JDialog's title bar moves it. */
function dragBy(handle: HTMLElement, target: HTMLElement) {
  handle.addEventListener("mousedown", (ev) => {
    if (ev.button !== 0 || (ev.target as HTMLElement).closest("button")) return;
    ev.preventDefault();
    const dx = ev.clientX - target.offsetLeft, dy = ev.clientY - target.offsetTop;
    const move = (e: MouseEvent) => {
      target.style.left = `${e.clientX - dx}px`;
      target.style.top = `${e.clientY - dy}px`;
    };
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  });
}

/** 关闭 and the close box: hide and leave scoring (endFinalScore resumes the analysis). */
function leave() {
  visible = false;
  refresh();
  if (state.view?.score) void run({ type: "Score", op: { type: "Toggle" } });
}

/** 确定: the result goes into the game info (RE, written by the core as SGF B+3.5), Java's tip
 *  names it in the dialog's words, and the window hides while scoring goes on. */
async function confirmResult() {
  const score = state.view?.score;
  if (!score) return;
  const text = scoreLines(score).result;
  visible = false;
  refresh();
  const result = await run({ type: "Score", op: { type: "Confirm" } });
  if (result && result.type !== "Refused") await message(t("Message.title"), `${t("ScoreResult.addResultTip")}${text}`);
}

/** packAndPosition (ScoreResult.java:97-134) inside the window: right of the board, else left of
 *  it, above, below, or at the right edge; vertically a third above the board's middle. */
function position(section: HTMLElement) {
  const board = boardRect();
  if (!board) return;
  const w = section.offsetWidth, h = section.offsetHeight;
  const W = window.innerWidth, H = window.innerHeight;
  const beside = Math.max(0, Math.min(board.top + board.height / 2 - (h * 2) / 3, H - h));
  let x: number, y: number;
  if (board.right + w + 5 <= W) [x, y] = [board.right + 5, beside];
  else if (board.left - w >= 0) [x, y] = [board.left - w, beside];
  else if (board.top - h > 0) [x, y] = [board.left + board.width / 2 - w / 2, board.top - h];
  else if (board.bottom + h <= H) [x, y] = [board.left + board.width / 2 - w / 2, board.bottom];
  else [x, y] = [W - w, beside];
  section.style.left = `${Math.round(x)}px`;
  section.style.top = `${Math.round(y)}px`;
}

function refresh() {
  const score = state.view?.score ?? null;
  if (!score || !visible) {
    // Detached while hidden, so only a showing window counts as a dialog.
    box?.remove();
    placed = false;
    return;
  }
  box ??= build();
  const s = scoreLines(score);
  const l = lines!;
  l.rule.textContent = s.rule;
  l.black.textContent = s.black;
  l.white.textContent = s.white;
  l.result.textContent = s.result;
  // Black's win on black, White's on the field background (AppleStyleSupport.validFieldBackground).
  l.result.style.background = s.blackWins ? "#000" : "#fff";
  l.result.style.color = s.blackWins ? "#fff" : "#000";
  toggleRules!.textContent = score.territory ? t("ScoreResult.btnToggleRulesArea") : t("ScoreResult.btnToggleRulesTerritory");
  if (!box.isConnected) document.body.append(box);
  if (!placed) {
    position(box);
    placed = true;
  }
}

/** A left click on the board while scoring (leftClickInScoreMode → toggleDeadStoneOrEmptyPoint →
 *  showGroupResult): the mark changes and the window shows again. */
export function scorePoint(point: Point): void {
  visible = true;
  void run({ type: "Score", op: { type: "Point", point } }).then(refresh);
}

/** 终局点目 (Ctrl+Q, menu, bottom bar): LizzieFrame.toggleScoreMode. */
export function toggleScore(): void {
  void run({ type: "Score", op: { type: "Toggle" } });
}

/** Follows the view: scoring starts with the window shown and ends with it hidden. */
export function bindScoreDialog(board: () => DOMRect | null): void {
  boardRect = board;
  subscribe(() => {
    const scoring = !!state.view?.score;
    if (scoring && !wasScoring) visible = true;
    if (!scoring) visible = false;
    wasScoring = scoring;
    refresh();
  });
}

/** The dialog as shown, for the acceptance probe. */
export function scoreDialogReport() {
  const shown = !!box?.isConnected && !!lines;
  return {
    visible: shown,
    lines: shown ? [lines!.hint, lines!.rule, lines!.black, lines!.white, lines!.result].map((el) => el.textContent ?? "") : [],
    toggleRules: shown ? toggleRules!.textContent : null,
  };
}
