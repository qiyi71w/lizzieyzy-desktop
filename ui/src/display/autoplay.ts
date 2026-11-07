// 设置自动播放 (AutoPlay.java, 显示 > 自动播放 / Ctrl+A / the bottom toolbar's 自动播放) and the three
// players it controls:
// - 变化图 (LizzieFrame.autoReplayBranch, LizzieFrame.java:11538-11615): while auto-replay-branch is
//   on, a newly hovered variation starts at its first move (or whole, for 先显示整个变化图) and grows
//   one move every replay-branch-interval-seconds; analysis keeps running.
// - 副棋盘 (BottomToolbar.autoPlaySub, BottomToolbar.java:4759-4814): the sub board's variation
//   grows one move every N milliseconds; placing a move starts it again at 1.
// - 主棋盘 (BottomToolbar.autoPlayMain, BottomToolbar.java:4707-4754): every N seconds one move
//   forward, or the engine's best move (落子最佳选点; 无下一手时,落子最佳选点 at the end of the line,
//   stopping after two passes). Navigating meanwhile only delays the next step.
// The run states are session state like Java's toolbar check boxes; the intervals and options are
// saved display settings.
import { registerViewHandler, run, state } from "../app";
import { setDisplay } from "../commands";
import { formPanel } from "../dialogs";
import { t } from "../i18n";
import { edit } from "../editing/state";
import { javaDouble } from "../workbench/paint";
import { absolute, button, place } from "./dialogs";
import type { ReplayTarget } from "./replay";

interface AutoplayHost {
  /** The main board's hovered variation, as BoardRenderer.replayTarget. */
  target(): ReplayTarget | null;
  repaint(): void;
}

let host: AutoplayHost | null = null;

export function bindAutoplayHost(h: AutoplayHost): void {
  host = h;
}

// ------------------------------------------------------------------ 变化图

/** The auto-replayed variation: its key, displayed length (null: whole) and next step time. */
const branch = { key: null as string | null, length: null as number | null, steps: 0, nextAt: 0 };
let branchTimer: number | undefined;

/** The displayed length of the hovered variation `key` (`steps` moves long) while auto-replay is
 *  on, else null. A new key restarts it (LizzieFrame.java:10131-10135, 11566-11583). */
export function autoReplayLength(key: string, steps: number): number | null {
  const d = state.view?.settings.display;
  if (!d?.autoReplayBranch) return null;
  if (branchTimer === undefined) branchTimer = window.setInterval(branchTick, 20);
  if (branch.key !== key) {
    branch.key = key;
    branch.length = d.entireVariationFirst ? null : 1;
    const delay = d.entireVariationFirst ? d.entireVariationFirstSeconds : d.replayIntervalSeconds * 0.3;
    branch.nextAt = performance.now() + delay * 1000;
  }
  branch.steps = steps;
  return branch.length;
}

/** The 20 ms Swing timer (LizzieFrame.java:11553-11612). */
function branchTick(): void {
  const d = state.view?.settings.display;
  if (!d?.autoReplayBranch) {
    window.clearInterval(branchTimer);
    branchTimer = undefined;
    branch.key = null;
    return;
  }
  // The target lookup itself restarts a changed variation (autoReplayLength).
  const target = host?.target() ?? null;
  if (!target) {
    branch.key = null;
    return;
  }
  const now = performance.now();
  if (now < branch.nextAt) return;
  const length = branch.length === null || branch.length === 1 ? 2 : Math.min(Math.max(0, branch.length + 1), branch.steps + 1);
  branch.nextAt = now + d.replayIntervalSeconds * 1000;
  if (length === branch.length) return;
  branch.length = length;
  host?.repaint();
}

/** Starts the timer when auto-replay is already on at start-up (Lizzie.java:488). */
registerViewHandler((view) => {
  if (view.settings.display.autoReplayBranch && branchTimer === undefined) branchTimer = window.setInterval(branchTick, 20);
});

/** The toolbar's txtAutoMain (seconds) / txtAutoSub (milliseconds): read by the players at every
 *  step, set by 确定 and filled from the saved intervals at start-up. */
const intervals = { main: -1, sub: -1, loaded: false };

// ------------------------------------------------------------------ 副棋盘

const sub = { running: false, timer: undefined as number | undefined, count: 1, wheeled: false };

/** The sub board's displayed length while its autoplay runs and the wheel has not taken over. */
export function subAutoplayLength(): number | null {
  return sub.running && !sub.wheeled ? sub.count : null;
}

/** Turning the wheel over the sub board takes the length over until the next move is placed. */
export function subWheeled(): void {
  if (sub.running) sub.wheeled = true;
}

/** Each step shows `count` moves, waits the interval and adds one; a non-positive interval
 *  unchecks it (BottomToolbar.java:4771-4807). */
function startSub(): void {
  if (sub.running || !(intervals.sub > 0)) return;
  sub.running = true;
  host?.repaint();
  const step = () => {
    if (!sub.running) return;
    if (!(intervals.sub > 0)) {
      stopSub();
      return;
    }
    sub.timer = window.setTimeout(() => {
      sub.count++;
      host?.repaint();
      step();
    }, intervals.sub);
  };
  step();
}

/** Unchecking shows the whole variation again (setDisplayedBranchLength(-2)). */
function stopSub(): void {
  window.clearTimeout(sub.timer);
  sub.running = false;
  sub.timer = undefined;
  host?.repaint();
}

// ------------------------------------------------------------------ 主棋盘

/** `node`: curNode, the position the next step starts from; `own`: our step is under way. */
const main = { running: false, timer: undefined as number | undefined, node: -1, own: false };

function stopMain(): void {
  window.clearTimeout(main.timer);
  main.running = false;
  main.timer = undefined;
}

/** isConsecutivePass: the current node and its parent are both passes. */
function consecutivePass(): boolean {
  const node = state.view ? state.nodes.get(state.view.position.node) : undefined;
  const parent = node?.parent == null ? undefined : state.nodes.get(node.parent);
  return !!node?.pass && !!parent?.pass;
}

async function mainTick(): Promise<void> {
  const v = state.view;
  if (!main.running || !v) return;
  if (!(intervals.main > 0)) {
    stopMain();
    return;
  }
  main.own = false;
  // A position the user went to meanwhile is the new starting point (curNode).
  if (v.position.node === main.node) {
    const d = v.settings.display;
    const atEnd = !state.nodes.get(v.position.node)?.children.length;
    if (d.directlyWithBestMove) {
      main.own = true;
      await edit({ op: "PlayBestMove" });
    } else if (!atEnd) {
      main.own = true;
      await run({ type: "Navigate", nav: { op: "Forward", count: 1 } });
    } else if (d.continueWithBestMove) {
      if (consecutivePass()) {
        stopMain();
        return;
      }
      main.own = true;
      await edit({ op: "PlayBestMove" });
    }
  } else main.node = v.position.node;
  if (main.running) main.timer = window.setTimeout(() => void mainTick(), intervals.main * 1000);
}

function startMain(): void {
  if (main.running) return;
  main.running = true;
  main.node = state.view?.position.node ?? -1;
  void mainTick();
}

let lastNode = -1;
registerViewHandler((view) => {
  if (!intervals.loaded) {
    intervals.loaded = true;
    intervals.main = view.settings.display.autoplayMainSeconds;
    intervals.sub = view.settings.display.autoplaySubMillis;
  }
  const node = view.position.node;
  if (node !== lastNode) {
    // Our own step moves the starting point along.
    if (main.own) {
      main.node = node;
      main.own = false;
    }
    // Board.clearAfterMove (Board.java:3666-3669, after every move or navigation): the sub
    // board's autoplay starts again at one move, the wheel's hold released.
    sub.count = 1;
    sub.wheeled = false;
    lastNode = node;
  }
});

/** What the acceptance probe reports. */
export function autoplayReport() {
  return {
    main: main.running,
    sub: sub.running ? { length: sub.count, wheeled: sub.wheeled } : null,
    branch: state.view?.settings.display.autoReplayBranch ? { key: branch.key, length: branch.length } : null,
  };
}

/** The detailed bottom toolbar's 自动播放(大)(秒) box (chkAutoMain, BottomToolbar.java:1849-1856)
 *  and its txtAutoMain / txtAutoSub fields, which the players read at every step. */
export const toolbarAutoplay = {
  mainRunning: (): boolean => main.running,
  setMain(on: boolean): void {
    if (on) startMain();
    else stopMain();
  },
  interval: (kind: "main" | "sub"): number => intervals[kind],
  setInterval(kind: "main" | "sub", value: number): void {
    intervals[kind] = value;
  },
};

// ------------------------------------------------------------------ 设置自动播放

function choice(panel: HTMLElement, text: string, checked: boolean, x: number, y: number, w: number, h: number): HTMLInputElement {
  const label = place(panel, document.createElement("label"), x, y, w, h);
  label.className = "display-choice";
  const input = document.createElement("input");
  input.type = "checkbox";
  input.checked = checked;
  label.append(input, text);
  return input;
}

function textField(panel: HTMLElement, value: string, x: number, y: number, w: number, h: number): HTMLInputElement {
  const field = place(panel, document.createElement("input"), x, y, w, h);
  field.type = "text";
  field.value = value;
  return field;
}

/** Double.parseDouble(text.replace(" ", "")), or NaN. */
function parseJavaDouble(text: string): number {
  const s = text.replaceAll(" ", "");
  return /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[dDfF]?$/.test(s) ? Number.parseFloat(s) : Number.NaN;
}

/** A saved interval as the toolbar field shows it: empty while none was entered. */
function intervalText(v: number): string {
  return v === -1 ? "" : String(v);
}

/** 设置自动播放 (AutoPlay.java, frame 330x232, not resizable). Only 确定 applies: the intervals and
 *  options are saved, the main and sub board players start or stop with their check boxes and
 *  the branch replay follows its setting. */
export function openAutoPlayDialog(): void {
  const s = state.view?.settings.display;
  if (!s) return;
  const panel = absolute(314, 193);
  const mainCheck = choice(panel, t("AutoPlay.chkAutoPlayMainbord"), main.running, 6, 6, 168, 24);
  const mainField = textField(panel, intervalText(s.autoplayMainSeconds), 180, 6, 66, 24);
  // AutoPlay.java:119-135 filled this box from continue-with-best-move; it shows its own setting.
  const directly = choice(panel, t("AutoPlay.chkDirectlyWithBestMove"), s.directlyWithBestMove, 17, 33, 294, 23);
  const continueBest = choice(panel, t("AutoPlay.chkContinueWithBestMove"), s.continueWithBestMove, 17, 60, 294, 23);
  const subCheck = choice(panel, t("AutoPlay.chkAutoPlaySubbord"), sub.running, 6, 87, 168, 24);
  const subField = textField(panel, intervalText(s.autoplaySubMillis), 180, 87, 66, 24);
  const branchCheck = choice(panel, t("AutoPlay.chkAutoPlayBranch"), s.autoReplayBranch, 6, 114, 168, 24);
  const branchField = textField(panel, String(Math.trunc(s.replayIntervalSeconds * 1000)), 180, 114, 66, 24);
  const entireCheck = choice(panel, t("AutoPlay.chbDisplayEntireVariationFirst"), s.entireVariationFirst, 17, 141, 230, 23);
  const entireField = textField(panel, javaDouble(s.entireVariationFirstSeconds), 268, 141, 43, 24);
  for (const [field, key] of [[mainField, "AutoPlay.chkAutoPlayMainbord"], [subField, "AutoPlay.chkAutoPlaySubbord"], [branchField, "AutoPlay.chkAutoPlayBranch"], [entireField, "AutoPlay.chbDisplayEntireVariationFirst"]] as const)
    field.setAttribute("aria-label", t(key));
  const ok = place(panel, button(t("AutoPlay.okButton")), 110, 171, 93, 25);
  const enable = () => {
    directly.disabled = continueBest.disabled = !mainCheck.checked;
    entireCheck.disabled = entireField.disabled = !branchCheck.checked;
  };
  mainCheck.onchange = branchCheck.onchange = enable;
  // Like their actionPerformed handlers (AutoPlay.java:119-146), the two best-move boxes apply at
  // once, a running main-board autoplay included.
  directly.onchange = () => setDisplay({ directlyWithBestMove: directly.checked });
  continueBest.onchange = () => setDisplay({ continueWithBestMove: continueBest.checked });
  enable();
  const dialog = formPanel(t("AutoPlay.title"), panel);
  ok.onclick = async () => {
    const mainSeconds = parseJavaDouble(mainField.value);
    const subMillis = parseJavaDouble(subField.value);
    const entire = parseJavaDouble(entireField.value);
    const branchMillis = parseJavaDouble(branchField.value);
    const current = state.view?.settings.display ?? s;
    const main = mainSeconds > 0 ? mainSeconds : -1;
    const subInterval = subMillis > 0 ? Math.trunc(subMillis) : -1;
    void dialog.close();
    const result = await run({
      type: "SetDisplaySettings",
      settings: {
        ...current,
        autoplayMainSeconds: main,
        autoplaySubMillis: subInterval,
        directlyWithBestMove: directly.checked,
        continueWithBestMove: continueBest.checked,
        entireVariationFirst: entireCheck.checked,
        entireVariationFirstSeconds: Number.isNaN(entire) ? current.entireVariationFirstSeconds : entire,
        replayIntervalSeconds: Number.isNaN(branchMillis) ? current.replayIntervalSeconds : branchMillis / 1000,
        autoReplayBranch: branchCheck.checked,
      },
    });
    // A refused or unsaved setting (already reported) changes nothing, the players included.
    if (!result || result.type === "Refused") return;
    intervals.main = main;
    intervals.sub = subInterval;
    if (mainCheck.checked) startMain();
    else stopMain();
    if (subCheck.checked) startSub();
    else stopSub();
  };
}
