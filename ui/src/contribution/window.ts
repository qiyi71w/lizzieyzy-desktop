// KataGo分布式训练 window (ContributeView.java): a non-modal window over the workbench that follows
// `view.contribution` — it opens for a new session id and goes away when the session is removed —
// and the one-time 跑谱贡献 notices. Every control sends a core command; the window keeps only the
// console text and the typed numbers.
import { invoke } from "@tauri-apps/api/core";
import { navigate, registerViewHandler, run, state } from "../app";
import { engineConsole, onConsole } from "../contract/client";
import type { ConsoleLineView, ContributionCommand, ContributionNotice, ContributionView, ContributionWatchSettings } from "../contract/generated";
import { icon, type IconName } from "../design/icons";
import { message, panel } from "../dialogs";
import { product, t } from "../i18n";
import { button, checkbox, input } from "../engine/controls";
import { toggleConsole } from "../engine/console";
import { contributionFailure } from "../engine/events";
import { openContributionSettings } from "./settings";
import { contributionText, outOfDateText } from "./text";
import "./contribution.css";

function contribution(op: ContributionCommand) {
  return run({ type: "Contribution", op });
}

/** One saved window choice changed, the others as they are (uiConfig.put per control). */
function setWatch(change: Partial<ContributionWatchSettings>): void {
  const watch = state.view?.settings.contribution.watch;
  if (watch) void contribution({ type: "SetWatchSettings", settings: { ...watch, ...change } });
}

/** IntDocument: digits only. */
function integerOnly(field: HTMLInputElement): HTMLInputElement {
  field.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /\D/.test(e.data ?? "")) e.preventDefault();
  });
  field.addEventListener("keydown", (e) => e.stopPropagation());
  return field;
}

function row(className: string, ...items: (HTMLElement | string)[]): HTMLDivElement {
  const el = document.createElement("div");
  el.className = className;
  el.append(...items);
  return el;
}

function keyed<T extends HTMLElement>(el: T, key: string): T {
  el.dataset.key = `contribution.view.${key}`;
  return el;
}

/** A step button: the bottom bar's icon, named by the Java glyph it replaces. */
function stepButton(glyph: string, name: IconName, key: string, run: () => void): HTMLButtonElement {
  const el = keyed(button("", run), key);
  el.classList.add("contribution-step");
  el.setAttribute("aria-label", glyph);
  el.title = glyph;
  el.append(icon(name));
  return el;
}

/** The parts of the open window that follow the view. */
interface OpenWindow {
  id: number;
  engine: string;
  root: HTMLElement;
  console: HTMLElement;
  consoleChars: number;
  tip: HTMLElement;
  consoleToggle: HTMLButtonElement;
  alwaysTop: HTMLInputElement;
  info: HTMLElement;
  alwaysLastMove: HTMLInputElement;
  autoPlay: HTMLInputElement;
  autoPlayNextGame: HTMLInputElement;
  skipNon19: HTMLInputElement;
  type: HTMLElement;
  komi: HTMLElement;
  result: HTMLElement;
  resultToggle: HTMLButtonElement;
  rules: HTMLElement;
  rulesToggle: HTMLButtonElement;
  pause: HTMLButtonElement;
  slowQuit: HTMLButtonElement;
  forceQuit: HTMLButtonElement;
  closeView: HTMLButtonElement;
}

let win: OpenWindow | null = null;

function setText(el: HTMLElement, text: string): void {
  if (el.textContent !== text) el.textContent = text;
}

function openWindow(view: ContributionView, watch: ContributionWatchSettings): void {
  closeWindow();
  const root = document.createElement("section");
  root.className = "contribution-window";
  root.setAttribute("role", "dialog");
  root.setAttribute("aria-label", t("ContributeView.title"));
  // A JFrame of its own: usable while a window disables the main one.
  root.dataset.freeWindow = "";
  root.dataset.key = "contribution.view";

  // DO_NOTHING_ON_CLOSE: the core closes an ended session and refuses a running one (closeTip).
  const header = row("contribution-header", t("ContributeView.title"), keyed(button(product("close"), () => void contribution({ type: "CloseView" })), "close"));

  const consoleText = document.createElement("pre");
  // The engine's output on the dark console surface whatever the mode, like the GTP console.
  consoleText.className = "contribution-console"; consoleText.tabIndex = 0; consoleText.dataset.theme = "dark";
  consoleText.setAttribute("aria-label", t("ContributeView.btnFullConsole"));
  const tip = row("contribution-tip");
  const consoleToggle = keyed(button("", () => setWatch({ hideConsole: !state.view?.settings.contribution.watch.hideConsole })), "console");
  const fullConsole = keyed(button(t("ContributeView.btnFullConsole"), () => void toggleConsole()), "fullConsole");
  const alwaysTop = checkbox(t("ContributeView.chkAlwaysTop"), watch.alwaysTop);
  keyed(alwaysTop.input, "alwaysTop").onchange = () => setWatch({ alwaysTop: alwaysTop.input.checked });

  const info = document.createElement("span");
  const saveAll = keyed(button(t("ContributeView.btnSaveGameRecords"), () => void contribution({ type: "SaveAllGames" })), "saveAll");

  // setWatchGame takes the 0-based index; the view counts from 1.
  const watching = () => (state.view?.contribution?.watching ?? 1) - 1;
  const gameIndex = integerOnly(input("")); gameIndex.className = "contribution-number"; keyed(gameIndex, "gameIndex");
  gameIndex.setAttribute("aria-label", t("ContributeView.lblGoto"));
  const gotoGame = () => {
    const n = Number.parseInt(gameIndex.value, 10);
    if (Number.isNaN(n)) return;
    void contribution({ type: "WatchGame", index: n - 1 });
    gameIndex.value = "";
  };
  gameIndex.addEventListener("keydown", (e) => { if (e.key === "Enter") gotoGame(); });

  const moveNumber = integerOnly(input("")); moveNumber.className = "contribution-number"; keyed(moveNumber, "moveNumber");
  moveNumber.setAttribute("aria-label", t("ContributeView.btnGotoMove"));
  const gotoMove = () => {
    const n = Number.parseInt(moveNumber.value, 10);
    if (Number.isNaN(n)) return;
    void run({ type: "GotoMoveNumber", number: n });
    moveNumber.value = "";
  };
  moveNumber.addEventListener("keydown", (e) => { if (e.key === "Enter") gotoMove(); });
  const alwaysLastMove = checkbox(t("ContributeView.chkAlwaysLastMove"), watch.alwaysLastMove);
  keyed(alwaysLastMove.input, "alwaysLastMove").onchange = () => setWatch({ alwaysLastMove: alwaysLastMove.input.checked });

  const autoPlay = checkbox(t("ContributeView.chkAutoPlay"), watch.autoPlay);
  keyed(autoPlay.input, "autoPlay").onchange = () => setWatch({ autoPlay: autoPlay.input.checked });
  // String.valueOf(double): a whole number keeps its ".0".
  const seconds = watch.autoPlayInterval;
  const interval = input(Number.isInteger(seconds) ? seconds.toFixed(1) : String(seconds)); interval.className = "contribution-number"; keyed(interval, "interval");
  interval.setAttribute("aria-label", t("ContributeView.lblAutoPlayInterval"));
  interval.addEventListener("keydown", (e) => e.stopPropagation());
  // DoubleDocument; only a positive time is kept.
  interval.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /[^\d.]/.test(e.data ?? "")) e.preventDefault();
  });
  interval.addEventListener("input", () => {
    const seconds = Number(interval.value);
    if (interval.value.trim() && Number.isFinite(seconds) && seconds > 0) setWatch({ autoPlayInterval: seconds });
  });
  const autoPlayNextGame = checkbox(t("ContributeView.chkAutoPlayNextGame"), watch.autoPlayNextGame);
  keyed(autoPlayNextGame.input, "autoPlayNextGame").onchange = () => setWatch({ autoPlayNextGame: autoPlayNextGame.input.checked });
  const skipNon19 = checkbox(t("ContributeView.chkIgnoreNone19"), watch.skipNon19);
  keyed(skipNon19.input, "skipNon19").onchange = () => setWatch({ skipNon19: skipNon19.input.checked });

  const type = document.createElement("span");
  const komi = document.createElement("span");
  const result = document.createElement("span");
  const resultToggle = keyed(button("", () => void contribution({ type: "ToggleResultHidden" })), "result");
  const rules = document.createElement("pre"); rules.className = "contribution-rules";
  const rulesToggle = keyed(button("", () => setWatch({ hideRules: !state.view?.settings.contribution.watch.hideRules })), "rules");
  rulesToggle.classList.add("contribution-wide");

  const pause = keyed(button(t("ContributeView.btnPause"), () => void contribution({ type: "PauseResume" })), "pause");
  const slowQuit = keyed(button(t("ContributeView.btnSlowShutdown"), () => void contribution({ type: "SlowQuit" })), "slowQuit");
  slowQuit.title = t("ContributeView.btnSlowShutdownTip");
  const forceQuit = keyed(button(t("ContributeView.btnForceShutdown"), () => void contribution({ type: "ForceQuit" })), "forceQuit");
  forceQuit.title = t("ContributeView.btnForceShutdownTip"); forceQuit.classList.add("danger");
  const closeView = keyed(button(t("ContributeView.btnCloseView"), () => void contribution({ type: "CloseView" })), "closeView");

  root.append(
    header,
    row("contribution-console-pane", consoleText, tip),
    row("contribution-console-buttons", consoleToggle, fullConsole, alwaysTop.label),
    row("contribution-row", info, saveAll),
    row("contribution-row",
      stepButton("|<", "step-first", "firstGame", () => void contribution({ type: "WatchGame", index: 0 })),
      keyed(button(t("ContributeView.btnPrevious"), () => void contribution({ type: "WatchGame", index: watching() - 1 })), "previousGame"),
      keyed(button(t("ContributeView.btnNext"), () => void contribution({ type: "WatchGame", index: watching() + 1 })), "nextGame"),
      // getGameMaxIndex; an index past the end selects the last game.
      stepButton(">|", "step-last", "lastGame", () => void contribution({ type: "WatchGame", index: Math.max(0, (state.view?.contribution?.total ?? 1) - 1) })),
      t("ContributeView.lblGoto"), gameIndex, keyed(button(t("ContributeView.btnGotoGame"), gotoGame), "gotoGame")),
    row("contribution-row",
      stepButton("|<", "step-first", "firstMove", () => navigate({ op: "First" })),
      stepButton("<<", "step-back-ten", "back10", () => navigate({ op: "Back", count: 10 })),
      stepButton("<", "step-back", "back", () => navigate({ op: "Back", count: 1 })),
      stepButton(">", "step-forward", "forward", () => navigate({ op: "Forward", count: 1 })),
      stepButton(">>", "step-forward-ten", "forward10", () => navigate({ op: "Forward", count: 10 })),
      stepButton(">|", "step-last", "lastMove", () => navigate({ op: "Last" })),
      moveNumber, keyed(button(t("ContributeView.btnGotoMove"), gotoMove), "gotoMove"), alwaysLastMove.label),
    row("contribution-row", autoPlay.label, t("ContributeView.lblAutoPlayInterval"), interval, autoPlayNextGame.label, skipNon19.label),
    row("contribution-row contribution-game", type, komi, result, resultToggle),
    rules, rulesToggle,
    row("contribution-buttons", pause, slowQuit, forceQuit, closeView),
  );
  document.body.append(root);
  win = {
    id: view.id, engine: view.console_engine, root, console: consoleText, consoleChars: 0, tip, consoleToggle, alwaysTop: alwaysTop.input, info,
    alwaysLastMove: alwaysLastMove.input, autoPlay: autoPlay.input, autoPlayNextGame: autoPlayNextGame.input, skipNon19: skipNon19.input,
    type, komi, result, resultToggle, rules, rulesToggle, pause, slowQuit, forceQuit, closeView,
  };
  void startConsole();
}

function closeWindow(): void {
  win?.root.remove();
  win = null;
}

/** The acceptance probe's view of the window: where it is, what it says and the console as drawn. */
export function contributionReport() {
  if (!win) return null;
  const r = win.root.getBoundingClientRect();
  const shown = (el: HTMLElement) => !el.hidden && el.getBoundingClientRect().width > 0;
  const spans = [...win.console.children] as HTMLElement[];
  const out = spans.filter((s) => s.className === "contribution-out");
  const err = spans.filter((s) => s.className === "contribution-err");
  return {
    id: win.id,
    rect: { x: Math.round(r.left), y: Math.round(r.top), w: Math.round(r.width), h: Math.round(r.height) },
    zIndex: getComputedStyle(win.root).zIndex,
    tip: win.tip.textContent,
    info: win.info.textContent,
    game: { type: win.type.textContent, komi: win.komi.textContent, result: win.result.textContent, rules: win.rules.hidden ? null : win.rules.textContent },
    pause: { text: win.pause.textContent, disabled: win.pause.disabled },
    buttons: { slowQuit: shown(win.slowQuit), forceQuit: shown(win.forceQuit), closeView: shown(win.closeView) },
    console: {
      hidden: win.console.hidden,
      chars: win.consoleChars,
      out: out.length,
      err: err.length,
      outColor: out[0] ? getComputedStyle(out[0]).color : null,
      errColor: err[0] ? getComputedStyle(err[0]).color : null,
      tail: (win.console.textContent ?? "").slice(-600),
    },
  };
}

function render(view: ContributionView, watch: ContributionWatchSettings): void {
  if (!win) return;
  win.engine = view.console_engine;
  win.root.classList.toggle("contribution-top", watch.alwaysTop);
  win.console.hidden = watch.hideConsole;
  setText(win.consoleToggle, watch.hideConsole ? t("ContributeView.showConsole") : t("ContributeView.hideConsole"));
  setText(win.tip, contributionText(view.tip.key));
  // updateLblGameInfos: 正在观看<blue>i/N</blue>,已完成n 局.
  const count = `${view.watching}/${view.total}`;
  const rest = `,${t("ContributeView.lblGameInfos.complete")}${view.finished}${t("ContributeView.lblGameInfos.games")}`;
  if (win.info.textContent !== `${t("ContributeView.lblGameInfos.watching")}${count}${rest}`) {
    const blue = document.createElement("span"); blue.className = "contribution-count"; blue.textContent = count;
    win.info.replaceChildren(t("ContributeView.lblGameInfos.watching"), blue, rest);
  }
  win.alwaysTop.checked = watch.alwaysTop;
  win.alwaysLastMove.checked = watch.alwaysLastMove;
  win.autoPlay.checked = watch.autoPlay;
  win.autoPlayNextGame.checked = watch.autoPlayNextGame;
  win.skipNon19.checked = watch.skipNon19;
  win.skipNon19.disabled = !watch.autoPlayNextGame;

  const game = view.game;
  setText(win.type, t("ContributeView.lblGameType") + (game ? (game.rating ? t("contributeView.gameType.ratingGame") : t("contributeView.gameType.trainingGame")) : ""));
  setText(win.komi, t("ContributeView.lblKomi") + (game ? game.komi.toFixed(1) : ""));
  const result = view.result_hidden ? "---" : game?.result || t("ContributeView.lblGameResult.none");
  setText(win.result, t("ContributeView.lblGameResult") + result);
  setText(win.resultToggle, view.result_hidden ? t("ContributeView.show") : t("ContributeView.hide"));
  // Before the first game: the rule names without values (ContributeView.java:451-463).
  setText(win.rules, game?.rules_text ?? [
    t("ContributeView.rules.scoring"), t("ContributeView.rules.ko"), t("ContributeView.rules.suicide"),
    t("ContributeView.rules.tax"), t("ContributeView.rules.whiteHandicapBonus"), t("ContributeView.rules.button"),
  ].join("\r\n"));
  win.rules.hidden = watch.hideRules;
  setText(win.rulesToggle, watch.hideRules ? t("ContributeView.showRules") : t("ContributeView.hideRules"));

  setText(win.pause, view.paused ? t("ContributeView.btnResume") : t("ContributeView.btnPause"));
  // exitedAfterSignal: only 关闭此界面 is left, 暂停 stays disabled.
  const exited = view.phase === "Exited";
  win.pause.disabled = exited;
  win.slowQuit.hidden = exited;
  win.forceQuit.hidden = exited;
  win.closeView.hidden = !exited;
}

// ---- console: this session's lines of the engine console stream (ContributeView.addLine /
// addErrorLine): stdout green with a leading space, stderr red, bounded like max_length.
const MAX_LENGTH = 15000;
let listening: Promise<unknown> | null = null;
let pending: ConsoleLineView[] | null = null;
let lastSeq = 0;

function appendLines(lines: ConsoleLineView[]): void {
  for (const line of lines) {
    if (line.seq <= lastSeq) continue;
    lastSeq = line.seq;
    if (!win || line.engine !== win.engine || line.kind === "Sent") continue;
    if (line.kind === "Output" && !line.text.trim()) continue;
    let text = line.kind === "Output" ? ` ${line.text}\n` : `${line.text}\n`;
    if (text.length > MAX_LENGTH / 10) text = `${text.slice(0, MAX_LENGTH / 10 - 5)}(...)\n`;
    const span = document.createElement("span");
    span.className = line.kind === "Output" ? "contribution-out" : "contribution-err";
    span.textContent = text;
    win.console.append(span);
    win.consoleChars += text.length;
  }
  if (!win) return;
  if (win.consoleChars > MAX_LENGTH) {
    let remove = win.consoleChars - MAX_LENGTH / 2;
    while (remove > 0 && win.console.firstChild) {
      const node = win.console.firstChild;
      const length = node.textContent?.length ?? 0;
      if (length <= remove) { node.remove(); remove -= length; win.consoleChars -= length; }
      else { node.textContent = node.textContent!.slice(remove); win.consoleChars -= remove; remove = 0; }
    }
  }
  win.console.scrollTop = win.console.scrollHeight;
}

/** Subscribes once; the first time the lines printed before the window opened are read too,
 *  merged by sequence so none is lost or shown twice. */
async function startConsole(): Promise<void> {
  if (listening) return;
  pending = [];
  try {
    listening = onConsole((lines) => { if (pending) pending.push(...lines); else appendLines(lines); });
    await listening;
    const history = await engineConsole(0);
    const buffered = pending ?? []; pending = null;
    appendLines([...history, ...buffered].sort((a, b) => a.seq - b.seq));
  } catch (error) {
    pending = null;
    await message(t("ContributeView.title"), String(error), "error");
  }
}

// ---- notices
async function outOfDate(version: string): Promise<void> {
  const { text, link } = outOfDateText(version);
  const body = document.createElement("div");
  const p = document.createElement("p"); p.className = "contribution-notice"; p.textContent = text;
  const open = button(link, async () => {
    try { await invoke("open_katago_releases"); } catch (error) { await message(t("Message.title"), String(error), "error"); }
  });
  open.className = "contribution-link";
  body.append(p, open);
  await panel(t("Message.title"), body);
}

async function notice(n: ContributionNotice): Promise<void> {
  switch (n.type) {
    case "Message": return message(t("Message.title"), contributionText(n.key));
    case "OutOfDate": return outOfDate(n.version);
    case "OpenSettings":
      await message(t("Message.title"), contributionText(n.key));
      return openContributionSettings();
    case "Saved": return message(t("Message.title"), `${t("EngineGameInfo.engineGameEndHintKifuPos")}${n.folder}`);
    // EngineFailedMessage with 重启引擎; it queues with the other engine failure windows.
    case "Failed": contributionFailure(n.failure); return;
  }
}

let notices: Promise<void> = Promise.resolve();
registerViewHandler((view) => {
  const session = view.contribution;
  if (!session) closeWindow();
  else {
    const watch = view.settings.contribution.watch;
    if (session.id !== win?.id) openWindow(session, watch);
    render(session, watch);
  }
  for (const n of view.contribution_notices) notices = notices.then(() => notice(n));
});
