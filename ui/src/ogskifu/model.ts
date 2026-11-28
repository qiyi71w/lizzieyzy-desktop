// The relay between the main window and the OGS 棋谱 window (label `ogs-kifu`, ogskifu.html). The
// main window owns the only workbench subscription and sends the window its part of every view;
// the window sends its commands itself (dispatch) and reports what it shows.
import type { Language, OgsKifuRow, OgsKifuView } from "../contract/generated";

export const OGS_KIFU_LABEL = "ogs-kifu";
/** main → window: OgsKifuPayload. */
export const OGS_KIFU_DATA_EVENT = "ogs-kifu-data";
/** window → main: OgsKifuInput. */
export const OGS_KIFU_INPUT_EVENT = "ogs-kifu-input";

export interface OgsKifuPayload {
  language: Language;
  /** Config.frameFontSize of this session. */
  frameFontSize: number;
  /** Function search's request for 打开棋谱后此窗口, by number, until the window says it located it. */
  locate: number | null;
  /** The view's part for the window, with the page's rows as last received. */
  view: OgsKifuWindowView;
}

export type OgsKifuWindowView = Omit<OgsKifuView, "rows"> & { rows: OgsKifuRow[] };

/** The window's part of the published views, which carry the page's rows only when they changed.
 *  Until one has carried them the rows are unknown, not empty: a reloaded main window first
 *  applies views built before its full snapshot. */
export class RelayedRows {
  private rows: OgsKifuRow[] | null = null;

  /** `view` with the rows as last received; null while they are unknown (nothing to relay). */
  windowView(view: OgsKifuView): OgsKifuWindowView | null {
    if (view.rows) this.rows = view.rows;
    return this.rows && { ...view, rows: this.rows };
  }
}

/** What the acceptance probe reports for the window: its visible texts and controls. */
export interface OgsKifuReport {
  query: string;
  afterGet: number;
  player: string;
  recent: string[];
  header: string[];
  /** Shown rows, cell texts in column order (the 打开 column omitted). */
  rows: { cells: string[]; id: number }[];
  /** The selected game, if any. */
  selected: number | null;
  /** The window's KifuProbe: the element with keyboard focus (its data-key, id or tag), the
   *  located control, the type size, and the elements whose text is cut off or that reach past
   *  the window's edges. */
  focus: string;
  located: string | null;
  fontSize: string;
  clipped: string[];
  page: string;
  overlay: string | null;
  status: string;
  empty: { title: string; text: string } | null;
  busy: boolean;
  /** How many reports the window has sent, this one included (it sends none before a probe). */
  sent: number;
  targets: { key: string; text: string; x: number; y: number; enabled: boolean }[];
}

export type OgsKifuInput = { type: "ready" } | { type: "located"; request: number } | { type: "report"; report: OgsKifuReport };
