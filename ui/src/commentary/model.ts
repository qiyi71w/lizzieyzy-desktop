// The relay between the main window and the AI 解说 window (label `commentary`, commentary.html).
// The main window owns the only workbench subscription; it sends the window its part of every view
// and keeps the window's last acceptance report. The window sends commands itself (dispatch).
import type { CommentaryView, Language } from "../contract/generated";

export const COMMENTARY_LABEL = "commentary";
/** main → window: CommentaryPayload. */
export const COMMENTARY_DATA_EVENT = "commentary-data";
/** main → window: the entry was used again while the window is open (TeacherDialog.show). */
export const COMMENTARY_FOCUS_EVENT = "commentary-focus";
/** window → main: CommentaryInput. */
export const COMMENTARY_INPUT_EVENT = "commentary-input";

export interface CommentaryPayload {
  language: Language;
  /** The open record; a new one resets the composer like TeacherDialog's game guard. */
  document: number;
  view: CommentaryView;
}

/** What the acceptance probe reports for the window (all visible state, no secrets). */
export interface CommentaryReport {
  mode: string;
  scope: string;
  status: string;
  tone: string;
  modelStatus: string;
  running: boolean;
  startLabel: string;
  startEnabled: boolean;
  stopEnabled: boolean;
  settingsEnabled: boolean;
  controlsEnabled: boolean;
  range: { from: number; to: number; min: number; max: number } | null;
  tab: "lesson" | "evidence";
  outputState: string;
  emptyTitle: string;
  emptyDetail: string;
  output: string;
  evidence: string;
  followUp: string;
  writeToSgf: boolean;
  usageVisible: boolean;
  settings: SettingsReport | null;
  targets: { key: string; text: string; x: number; y: number; enabled: boolean }[];
}

/** The settings dialog as shown; the access key field reports only whether it holds text. */
export interface SettingsReport {
  page: "connection" | "preferences";
  provider: string;
  status: string;
  baseUrl: string;
  keyFilled: boolean;
  keyShown: boolean;
  model: string;
  models: string[];
  remember: boolean;
  rememberEnabled: boolean;
  account: string;
  accounts: string[];
  chatModel: string;
  chatModels: string[];
  effort: string;
  efforts: string[];
  chatStatus: string;
  level: string;
  style: string;
  density: string;
  pace: string;
  variation: string;
  saveLabel: string;
  saveEnabled: boolean;
}

export type CommentaryInput = { type: "ready" } | { type: "report"; report: CommentaryReport };
