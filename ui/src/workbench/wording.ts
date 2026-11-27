// How the main window writes five of its figures, following the current Java (next-2026-11-19.1):
// the lead under the win-rate bar and the previous move's change beside it (MoveStatusText.lead,
// MoveStatusText.previousMove), the actual move's cells in both candidate lists with the loss
// level they are coloured by (CandidateTableText, CandidateTableStyle), the players in the window
// title (LizzieFrame.setPlayers / setResult, MainWindowTitle.join) and the komi of the game panel
// (InfoPanelText.komi). Plain functions of their inputs: the callers pass the interface words, so
// this module imports nothing and its tests run without a window.

/** One decimal as the tables and panels write it. */
const tenth = (v: number) => v.toFixed(1);

/** Who leads by how much: the side and the unsigned points, or "even" when the lead rounds to
 *  0.0 (MoveStatusText.lead). `blackLead` is Black's lead, negative when White leads. */
export function lead(blackLead: number): { side: "Black" | "White"; points: string } | "even" {
  const points = tenth(Math.abs(blackLead));
  if (points === "0.0") return "even";
  return { side: blackLead > 0 ? "Black" : "White", points };
}

/** A change at one decimal: "+1.2" above zero, "-0.3" below it, and no sign when it rounds to
 *  0.0 (MoveStatusText.signed). */
function signed(change: number): string {
  const digits = tenth(Math.abs(change));
  return `${digits === "0.0" ? "" : change > 0 ? "+" : "-"}${digits}`;
}

/** The words of the previous move: LizzieFrame.status.previousMove and LizzieFrame.pts. */
export interface PreviousMoveWords {
  name: string;
  pts: string;
}

/** The previous move's part of the line under the bar, whole in `text` and in the pieces the
 *  window draws: its name with the rank, the win-rate change and the score change, each a `loss`
 *  when it is below zero and does not round to 0.0. */
export interface PreviousMove {
  text: string;
  name: string;
  winrate: { value: string; loss: boolean };
  score: { value: string; loss: boolean } | null;
}

/** What the previous move changed for the side that played it, "上一手(#3) -0.3% -1.9目"
 *  (MoveStatusText.previousMove): the name, its candidate `rank` in brackets when it was one, the
 *  win-rate change in percentage points, then the score change in points when known. */
export function previousMove(rank: number | null, winrateChange: number, scoreChange: number | null, words: PreviousMoveWords): PreviousMove {
  const name = rank ? `${words.name}(#${rank})` : words.name;
  const part = (change: number, unit: string) => ({ value: `${signed(change)}${unit}`, loss: change < 0 && tenth(-change) !== "0.0" });
  const winrate = part(winrateChange, "%");
  const score = scoreChange === null ? null : part(scoreChange, words.pts);
  return { text: [name, winrate.value, score?.value].filter(Boolean).join(" "), name, winrate, score };
}

/** How far the actual move is from the best candidate (`value - best`, the side to move's
 *  values), rounded to the tenth its cell writes: the value the row is coloured by
 *  (CandidateTableText.differenceIn of the cell). 0 when either value is unknown. */
export function changeFromBest(value: number | null, best: number | null): number {
  return value === null || best === null ? 0 : Number(tenth(value - best)) + 0;
}

/** The actual move's cell (CandidateTableText.valueWithDifference): the value as the table shows
 *  it, then its `change` from the best candidate after an arrow ("99.2 ↓0.1"); a change that
 *  rounds to 0.0 is left out. */
export function withChange(shown: number, change: number): string {
  const gap = tenth(Math.abs(change));
  return gap === "0.0" ? tenth(shown) : `${tenth(shown)} ${change > 0 ? "↑" : "↓"}${gap}`;
}

/** The actual move's loss level (CandidateTableStyle.playedMoveBackground): 0 the worst to 5 the
 *  best, by the win-rate and score changes against the five configured thresholds each. */
export function lossLevel(change: number, scoreChange: number, winThresholds: readonly number[], scoreThresholds: readonly number[]): number {
  for (let k = 4; k >= 0; k--) if (change <= winThresholds[k] || scoreChange <= scoreThresholds[k]) return 4 - k;
  return 5;
}

/** The words of the title's players: Menu.Black, Menu.White and LizzieFrame.result. */
export interface PlayerWords {
  black: string;
  white: string;
  result: string;
}

/** The players and the result in the title, "[黑] X vs [白] Y (结果:B+R)" (setPlayers, setResult,
 *  joined by MainWindowTitle.join). A missing name leaves its place empty, as Java's format does;
 *  the ends are trimmed. */
export function players(black: string, white: string, result: string, words: PlayerWords): string {
  const names = `[${words.black}] ${black} vs [${words.white}] ${white}`.trim();
  return result ? `${names} (${words.result}${result})` : names;
}

/** The game panel's komi without trailing zeros and always in plain digits: 7, 7.5, 6.25, 0,
 *  0.0000001 (Java `InfoPanelText.komi`, `BigDecimal…stripTrailingZeros().toPlainString()`).
 *  JavaScript writes very small and very large numbers with an exponent, which is spelled out. */
export function komiText(komi: number): string {
  const text = String(komi + 0);
  const exponent = /^(-?)(\d)(?:\.(\d+))?e([+-]\d+)$/.exec(text);
  if (!exponent) return text;
  const [, sign, first, rest = "", power] = exponent;
  const digits = first + rest;
  const point = 1 + Number(power);
  return point <= 0 ? `${sign}0.${"0".repeat(-point)}${digits}` : `${sign}${digits.padEnd(point, "0")}`;
}
