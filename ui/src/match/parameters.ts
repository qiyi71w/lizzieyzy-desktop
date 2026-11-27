// 对局参数 (Java EngineGameParametersDialog, EngineGameParameterControls): the breadth and
// aggressiveness of the two participant positions of the playing analysis-mode engine game. The
// panel holds only drafts; the core's parameter session writes the values and reports what the
// engines confirmed. Its entries are 棋局 > 对局参数 and the top toolbar's game controls.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS, reasonText, type Reason } from "../commands";
import type { MatchParameterFailure, MatchParameterValue, MatchView, WorkbenchView } from "../contract/generated";
import { icon } from "../design/icons";
import { formPanel, message } from "../dialogs";
import { button, footer, primaryButton, row } from "../engine/controls";
import { format, product, t } from "../i18n";
import { parseJavaDouble } from "../workbench/paint";
import { requestState, type ParameterRequest } from "./request";
import "./match.css";

const NOT_PLAYING: Reason = { key: "EngineGameParameters.notPlaying" };

/** The game the panel edits: a playing or paused analysis-mode game (EngineGameParameterSession
 *  reviseFor's NOT_PLAYING otherwise). */
export function parametersGame(v: WorkbenchView | null = state.view): MatchView | null {
  const m = v?.engine_match;
  return m && m.kind === "Analysis" && (m.phase === "Playing" || m.phase === "Paused") && m.parameters ? m : null;
}

export function parametersUnavailable(): Reason | null {
  return parametersGame() ? null : NOT_PLAYING;
}

/** One participant's parameter failure as Java words it: "引擎 1: <reason>", joined by "; "
 *  (EngineGameParticipantCommands, EngineGameParameterSession); a failure of the game as a whole
 *  has no participant. */
export function parameterFailures(failures: MatchParameterFailure[]): string {
  return failures
    .map(({ first, reason }) => {
      let why: string;
      switch (reason.type) {
        case "Unsupported":
          why = `${reason.parameter === "Wrn" ? t("EngineGameParameters.wrn") : t("EngineGameParameters.pda")} (${t("EngineGameParameters.unsupported")})`;
          break;
        case "Timeout":
          why = t("EngineGameCommand.timeout");
          break;
        case "Rejected":
          why = format(t("EngineGameCommand.rejected"), reason.command, reason.response);
          break;
        case "NotSent":
          why = format(t("EngineGameCommand.notSent"), reason.command);
          break;
        case "Mismatch":
          why = format(t("EngineGameParameters.readbackMismatch"), reason.actual, reason.expected);
          break;
        case "ProcessChanged":
          why = t("EngineGameParameters.processChanged");
          break;
        case "Interrupted":
          why = t("EngineGameParameters.updateInterrupted");
          break;
        case "GameEnded":
          why = t("EngineGameParameters.transactionLost");
          break;
        case "TurnTimeout":
          why = t("EngineGameParameters.turnTimeout");
          break;
      }
      if (first === null) return why;
      return `${first ? t("EngineGameParameters.position1") : t("EngineGameParameters.position2")}: ${why}`;
    })
    .join("; ");
}

/** A value as people type it (EngineGameParametersDialog.formatValue): 0, 0.25, -1.5. */
const plain = (value: number) => String(value + 0);

/** EngineGameOpeningParameters.parseWrn / parsePda: a complete number in range, else null. */
function parse(pda: boolean, text: string): number | null {
  const v = parseJavaDouble(text);
  if (v === null) return null;
  return (pda ? v >= -3 && v <= 3 : v >= 0 && v <= 2) ? v : null;
}

/** EngineGameParametersDialog.ParameterCell: the field, the confirmed value under it and the value
 *  being applied. `dirty`: the user edited the field since the panel last loaded it. */
interface Cell {
  pda: boolean;
  el: HTMLElement;
  field: HTMLInputElement;
  confirmed: HTMLElement;
  pending: HTMLElement;
  dirty: boolean;
}

function setText(el: HTMLElement, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

function cell(pda: boolean, label: string, key: string): Cell {
  const el = document.createElement("span");
  el.className = "match-param-cell";
  const field = document.createElement("input");
  field.type = "text";
  field.inputMode = "decimal";
  field.dataset.key = key;
  field.setAttribute("aria-label", label);
  const confirmed = document.createElement("span");
  confirmed.className = "match-param-state";
  confirmed.dataset.key = `${key}.confirmed`;
  const pending = document.createElement("span");
  pending.className = "match-param-state pending";
  pending.dataset.key = `${key}.pending`;
  field.setAttribute("aria-describedby", `${(confirmed.id = `${key}-confirmed`)} ${(pending.id = `${key}-pending`)}`);
  el.append(field, confirmed, pending);
  const c: Cell = { pda, el, field, confirmed, pending, dirty: false };
  field.addEventListener("input", () => {
    c.dirty = true;
    field.classList.toggle("invalid", parse(pda, field.value) === null);
  });
  return c;
}

/** Loads `text` without marking the field edited. */
function load(c: Cell, text: string): void {
  if (c.field.value !== text) c.field.value = text;
  c.field.classList.remove("invalid");
}

function discard(c: Cell): void {
  c.dirty = false;
  load(c, "");
}

/** ParameterCell.show: `value` null without a playing game; `closing` keeps the submitted text.
 *  The value being applied shows also while the panel waits for it (Java hides it then). */
function show(c: Cell, value: MatchParameterValue | null, closing: boolean): void {
  const name = `${c.pda ? t("EngineGameParameters.pda") : t("EngineGameParameters.wrn")}: `;
  if (!value || !value.supported) {
    setText(c.confirmed, value ? name + t("EngineGameParameters.unsupported") : "");
    setText(c.pending, "");
    c.field.disabled = true;
    discard(c);
    return;
  }
  setText(c.confirmed, name + (value.confirmed === null ? t("EngineGameParameters.unknown") : plain(value.confirmed)));
  setText(c.pending, value.target === null ? "" : format(t("EngineGameParameters.pending"), plain(value.target)));
  c.field.disabled = closing;
  if (!c.dirty && !closing) load(c, value.confirmed === null ? "" : plain(value.confirmed));
}

/** The open panel's refresh, null while it is closed. */
let refreshOpen: (() => void) | null = null;

/** Whether the panel is showing, so its inline failure text replaces the popup. */
export function parametersPanelOpen(): boolean {
  return refreshOpen !== null;
}

registerViewHandler(() => refreshOpen?.());

/** EngineGameParametersDialog.open: the one panel, or the open one refreshed; a closed panel's
 *  drafts are gone. */
export async function openMatchParameters(): Promise<void> {
  if (refreshOpen) {
    refreshOpen();
    return;
  }
  const body = document.createElement("div");
  body.className = "match-params";
  const sides = document.createElement("div");
  sides.className = "match-sides";
  body.append(sides);
  // 黑方设置 / 白方设置 columns under their stones; every control says whose side it sets.
  const sideNames = [t("NewEngineGameDialog.lblB"), t("NewEngineGameDialog.lblW")];
  const head = document.createElement("div");
  head.className = "match-params-head";
  (["stone-black", "stone-white"] as const).forEach((stone, k) => {
    const h = document.createElement("span");
    h.append(icon(stone), sideNames[k]);
    head.append(h);
  });
  sides.append(head);
  const engines = [0, 1].map((k) => {
    const e = document.createElement("span");
    e.className = "match-param-engine";
    e.dataset.key = `matchParams.engine${k === 0 ? "Black" : "White"}`;
    return e;
  });
  row(sides, t("NewEngineGameDialog.lblengine"), "", ...engines);
  const wrnName = t("EngineGameParameters.wrn");
  const pdaName = t("EngineGameParameters.pda");
  // By column: [black, white]; the positions they hold follow the current game's colours.
  const wrn = [0, 1].map((k) => cell(false, `${sideNames[k]} · ${wrnName}`, `matchParams.wrn${k === 0 ? "Black" : "White"}`));
  const pda = [0, 1].map((k) => cell(true, `${sideNames[k]} · ${pdaName}`, `matchParams.pda${k === 0 ? "Black" : "White"}`));
  row(sides, wrnName, "", ...wrn.map((c) => c.el));
  row(sides, pdaName, "", ...pda.map((c) => c.el));
  const status = document.createElement("p");
  status.className = "ds-note match-params-status";
  status.setAttribute("role", "status");
  status.dataset.key = "matchParams.status";
  body.append(status);

  /** The game the drafts were typed for, or null without a playing game. */
  let draftGame: number | null = null;
  /** The game the panel last showed playing, and why its values were not confirmed when it ended
   *  before the engines answered. */
  let shownGame: number | null = null;
  let endedReason = "";
  /** 确定 was pressed: "sent" until the core answered, then the request it took (close once both
   *  participants confirmed, stay open on a failure). */
  let closing: false | "sent" | ParameterRequest = false;

  const dialog = formPanel(t("EngineGameParameters.title"), body);
  const apply = primaryButton(t("EngineGameParameters.ok"), () => void submit());
  apply.dataset.key = "matchParams.ok";
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "matchParams.cancel";
  footer(body, { primary: apply, cancel });

  const refresh = () => {
    const v = state.view;
    const m = parametersGame(v);
    const params = m?.parameters ?? null;
    const game = m ? m.game : null;
    if (game !== draftGame) {
      // Drafts never carry over to another game.
      for (const c of [...wrn, ...pda]) discard(c);
      draftGame = game;
    }
    if (m) {
      shownGame = m.game;
      endedReason = "";
    } else {
      // A game that ended before the engines answered keeps its reasons with its parameters.
      const ended = v?.engine_match;
      const failures = ended && ended.game === shownGame ? (ended.parameters?.failures ?? []) : [];
      if (failures.length) endedReason = parameterFailures(failures);
    }
    if (typeof closing === "object") {
      const now = requestState(closing, v?.seq ?? 0, m);
      if (now === "confirmed") {
        closing = false;
        void dialog.close();
        return;
      }
      if (now === "open") closing = false;
    }
    // The black column holds 引擎 1 (position 0) when it plays Black.
    const black = m && !m.first_is_black ? 1 : 0;
    const positions = params ? [params.first, params.second] : null;
    [0, 1].forEach((k) => {
      // Column k shows the position playing its colour.
      const position = m ? (k === 0 ? black : 1 - black) : 0;
      const name = !m ? "" : position === 0 ? m.first_name : m.second_name;
      setText(engines[k], !m ? "" : `${position === 0 ? t("EngineGameParameters.position1") : t("EngineGameParameters.position2")} · ${name}`);
      show(wrn[k], positions ? positions[position].wrn : null, closing !== false);
      show(pda[k], positions ? positions[position].pda : null, closing !== false);
    });
    const disabled = !m || closing !== false;
    if (apply.disabled !== disabled) apply.disabled = disabled;
    const cancelText = closing ? product("close") : product("cancel");
    setText(cancel, cancelText);
    let text = "";
    if (!m) text = endedReason ? `${t("EngineGameParameters.notPlaying")}\n${endedReason}` : t("EngineGameParameters.notPlaying");
    else if (params && params.failures.length > 0) text = format(t("EngineGameParameters.failed"), parameterFailures(params.failures));
    setText(status, text);
    status.hidden = text === "";
    // A control that turned disabled drops the focus to the page, where Escape and Tab no longer
    // reach the panel: 取消 / 关闭 takes it.
    if (document.activeElement === document.body || (document.activeElement as HTMLInputElement | null)?.disabled) cancel.focus();
  };

  /** EngineGameParametersDialog.apply: only the edited fields, each a complete valid number. */
  const submit = async () => {
    if (closing) {
      refresh();
      return;
    }
    const m = parametersGame();
    if (!m || m.game !== draftGame) {
      refresh();
      await message(t("EngineGameParameters.title"), t("EngineGameParameters.notPlaying"));
      return;
    }
    const invalid = (cells: Cell[]) => cells.some((c) => c.dirty && parse(c.pda, c.field.value) === null);
    if (invalid(wrn)) {
      await message(t("EngineGameParameters.title"), t("EngineGameParameters.invalidWrn"), "warning");
      return;
    }
    if (invalid(pda)) {
      await message(t("EngineGameParameters.title"), t("EngineGameParameters.invalidPda"), "warning");
      return;
    }
    const black = m.first_is_black ? 0 : 1;
    // By position: [引擎 1, 引擎 2].
    const byPosition = (cells: Cell[]) => {
      const [first, second] = black === 0 ? cells : [cells[1], cells[0]];
      return [first, second].map((c) => (c.dirty ? parse(c.pda, c.field.value) : null));
    };
    const [firstWrn, secondWrn] = byPosition(wrn);
    const [firstPda, secondPda] = byPosition(pda);
    if ([firstWrn, secondWrn, firstPda, secondPda].every((v) => v === null)) {
      await dialog.close();
      return;
    }
    // The fields lock at once; the views after the core's answer show the values being applied.
    closing = "sent";
    refresh();
    const result = await run({
      type: "Match",
      op: { type: "ReviseParameters", game: m.game, first_wrn: firstWrn, second_wrn: secondWrn, first_pda: firstPda, second_pda: secondPda },
    });
    if (refreshOpen !== refresh) return;
    if (result?.type === "Unchanged") {
      // The engines already hold every value typed: nothing to wait for.
      closing = false;
      await dialog.close();
      return;
    }
    if (result?.type === "ParametersRevised") {
      for (const c of [...wrn, ...pda]) if (c.dirty && parse(c.pda, c.field.value) !== null) c.dirty = false;
      closing = { game: m.game, after: result.after };
    } else {
      closing = false;
    }
    refresh();
  };

  refreshOpen = refresh;
  void dialog.closed.then(() => {
    if (refreshOpen === refresh) refreshOpen = null;
  });
  refresh();
  // The first field that takes a value has the focus.
  [...wrn, ...pda].find((c) => !c.field.disabled)?.field.focus();
  await dialog.closed;
}

Object.assign(COMMANDS, {
  "menu.engineGameParameters": { run: () => void openMatchParameters(), disabledReason: parametersUnavailable },
});

/** The toolbar's 对局参数 button (EngineGameParameterControls.refreshEntry) as the view says: shown
 *  during an analysis-mode batch (between its games too), usable while a game is on, otherwise
 *  saying why. Written only when it changes. */
export function refreshParametersButton(b: HTMLElement): void {
  const match = state.view?.engine_match;
  const display = match && match.kind !== "Genmove" ? "" : "none";
  if (b.style.display !== display) b.style.display = display;
  const reason = parametersUnavailable();
  const off = reason !== null;
  if (b.classList.contains("disabled") !== off) b.classList.toggle("disabled", off);
  const aria = String(off);
  if (b.getAttribute("aria-disabled") !== aria) b.setAttribute("aria-disabled", aria);
  const title = reason ? reasonText(reason) : t("EngineGameParameters.title");
  if (b.title !== title) b.title = title;
}
