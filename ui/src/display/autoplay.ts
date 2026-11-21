// 设置自动播放 (AutoPlay.java, 显示 > 自动播放 / Ctrl+A / the bottom toolbar's 自动播放) and the three
// players it controls:
// - 变化图 (LizzieFrame.autoReplayBranch, LizzieFrame.java:11538-11615): while auto-replay-branch is
//   on, a newly hovered variation starts at its first move (or whole, for 先显示整个变化图) and grows
//   one move every replay-branch-interval-seconds, on the main board and on 棋盘同步工具's float
//   board alike; analysis keeps running.
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
import { button, footer, primaryButton, row, switchBox, switchRow } from "../engine/controls";
import type { DisplaySettings } from "../contract/generated";
import { product, registerProduct, t } from "../i18n";
import { edit } from "../editing/state";
import { javaDouble } from "../workbench/paint";
import type { ReplayTarget } from "./replay";
import "./dialogs.css";

registerProduct({
  ZhCn: { autoplayAppliesNow: "立即生效，取消不会撤回" },
  ZhTw: { autoplayAppliesNow: "立即生效，取消不會撤回" },
  EnUs: { autoplayAppliesNow: "Applies at once; Cancel does not undo it" },
  JaJp: { autoplayAppliesNow: "すぐに反映され、キャンセルでは元に戻りません" },
  KoKr: { autoplayAppliesNow: "바로 적용되며 취소해도 되돌려지지 않습니다" },
  ThTh: { autoplayAppliesNow: "มีผลทันที การยกเลิกจะไม่ย้อนกลับ" },
});

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

/** The main board's auto-replayed variation: its key, displayed length (null: whole) and moves. */
const branch = { key: null as string | null, length: null as number | null, steps: 0 };
let branchTimer: number | undefined;

/** 棋盘同步工具's float board (FloatBoardRenderer.replayTarget / getDisplayedBranchLength /
 *  setReplayLength, FloatBoardRenderer.java:2528-2548): its own variation and displayed length,
 *  which its wheel also changes, stepped by the same timer (LizzieFrame.java:11568-11571,
 *  11596-11607). */
interface FloatReplayHost {
  /** The shown variation while the window is visible; a new key restarts the wait (the wheel
   *  changes it too: setDisplayedBranchLength → stopReplay). */
  target(): ReplayTarget | null;
  length(): number | null;
  /** Sets the length and repaints the window. */
  setLength(length: number): void;
}

let floatHost: FloatReplayHost | null = null;

/** The one wait both boards step on (`mainTarget`, `floatingTarget`, `nextStepAt`,
 *  LizzieFrame.java:11566-11583): either target changing restarts it. */
const clock = { main: null as string | null, float: null as string | null, nextAt: 0 };

export function bindFloatReplayHost(h: FloatReplayHost): void {
  floatHost = h;
}

/** The wait before a new variation's first step, in ms (LizzieFrame.java:11578-11582). */
function firstStepDelay(d: DisplaySettings): number {
  return (d.entireVariationFirst ? d.entireVariationFirstSeconds : d.replayIntervalSeconds * 0.3) * 1000;
}

/** The next displayed length (LizzieFrame.java:11588-11605): the whole variation or one move
 *  become two, then one more up to the variation's end. */
function nextLength(length: number | null, steps: number): number {
  return length === null || length === 1 ? 2 : Math.min(Math.max(0, length + 1), steps + 1);
}

/** The displayed length of the hovered variation `key` (`steps` moves long) while auto-replay is
 *  on, else null. A new key restarts it (LizzieFrame.java:10131-10135, 11566-11583). */
export function autoReplayLength(key: string, steps: number): number | null {
  const d = state.view?.settings.display;
  if (!d?.autoReplayBranch) return null;
  if (branchTimer === undefined) branchTimer = window.setInterval(branchTick, 20);
  if (branch.key !== key) {
    branch.key = key;
    branch.length = d.entireVariationFirst ? null : 1;
  }
  branch.steps = steps;
  return branch.length;
}

/** The displayed length of variation `key` while auto-replay shows it (null: the whole
 *  variation), `undefined` when it does not. Java keeps one displayedBranchLength that the replay
 *  timer and doBranch both step (LizzieFrame.java:11585-11594, 12036-12073). */
export function autoReplayed(key: string): number | null | undefined {
  return state.view?.settings.display.autoReplayBranch && branch.key === key ? branch.length : undefined;
}

/** Sets the auto-replayed variation `key`'s displayed length, which the replay grows on from.
 *  Returns false when auto-replay does not show `key`. */
export function setAutoReplayed(key: string, length: number): boolean {
  if (autoReplayed(key) === undefined) return false;
  branch.length = length;
  return true;
}

/** The 20 ms Swing timer (LizzieFrame.java:11553-11612): one wait for the main board's and the
 *  float board's variations; when it is over both step and the next wait is the interval. */
function branchTick(): void {
  const d = state.view?.settings.display;
  if (!d?.autoReplayBranch) {
    window.clearInterval(branchTimer);
    branchTimer = undefined;
    branch.key = null;
    clock.main = null;
    clock.float = null;
    return;
  }
  // The main target lookup itself restarts a changed variation's length (autoReplayLength).
  const main = host?.target() ?? null;
  const float = floatHost?.target() ?? null;
  if (!main) branch.key = null;
  const now = performance.now();
  if ((main?.key ?? null) !== clock.main || (float?.key ?? null) !== clock.float) {
    clock.main = main?.key ?? null;
    clock.float = float?.key ?? null;
    clock.nextAt = now + firstStepDelay(d);
  }
  if (now < clock.nextAt) return;
  if (main) {
    const length = nextLength(branch.length, branch.steps);
    if (length !== branch.length) {
      branch.length = length;
      host?.repaint();
    }
  }
  if (float && floatHost) {
    const length = floatHost.length();
    const next = nextLength(length, float.steps);
    if (next !== length) floatHost.setLength(next);
  }
  clock.nextAt = now + d.replayIntervalSeconds * 1000;
}

/** Starts the timer when auto-replay is already on at start-up (Lizzie.java:488). */
registerViewHandler((view) => {
  if (view.settings.display.autoReplayBranch && branchTimer === undefined) branchTimer = window.setInterval(branchTick, 20);
});

/** The main (seconds) and sub board (milliseconds) autoplay intervals: read by the players at
 *  every step, filled from the saved intervals at start-up and set by 自动播放's 确定 once the core
 *  saved them. */
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

// ------------------------------------------------------------------ 设置自动播放

/** The next group of rows in `body`: the groups have no titles, a gap parts them. */
function group(body: HTMLElement): HTMLDivElement {
  const el = document.createElement("div");
  el.className = "autoplay-group";
  body.append(el);
  return el;
}

/** A row with an interval field, then the switch that runs the player or option it times. */
function timed(host: HTMLElement, text: string, checked: boolean, value: string): { box: HTMLInputElement; field: HTMLInputElement } {
  const box = switchBox(checked);
  const field = document.createElement("input");
  field.type = "text";
  field.inputMode = "decimal";
  field.value = value;
  box.setAttribute("aria-label", text);
  field.setAttribute("aria-label", text);
  row(host, text, "", field, box);
  return { box, field };
}

/** Double.parseDouble(text.replace(" ", "")), or NaN. */
function parseJavaDouble(text: string): number {
  const s = text.replaceAll(" ", "");
  return /^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[dDfF]?$/.test(s) ? Number.parseFloat(s) : Number.NaN;
}

/** A saved interval as its field shows it: empty while none was entered. */
function intervalText(v: number): string {
  return v === -1 ? "" : String(v);
}

/** 设置自动播放 (AutoPlay.java): a group per player, its interval field and switch, the main
 *  board's best-move options and 先显示整个变化图 under their player. The two best-move options
 *  apply at once and say so under their names; 确定 applies the rest: the intervals and options
 *  are saved, the main and sub board players start or stop with their switches and the branch
 *  replay follows its setting; 取消 leaves them. */
export function openAutoPlayDialog(): void {
  const s = state.view?.settings.display;
  if (!s) return;
  const body = document.createElement("div");
  body.className = "autoplay-dialog";
  const mainGroup = group(body);
  const { box: mainCheck, field: mainField } = timed(mainGroup, t("AutoPlay.chkAutoPlayMainbord"), main.running, intervalText(s.autoplayMainSeconds));
  // AutoPlay.java:119-135 filled this box from continue-with-best-move; it shows its own setting.
  const directly = switchRow(mainGroup, t("AutoPlay.chkDirectlyWithBestMove"), product("autoplayAppliesNow"), s.directlyWithBestMove).input;
  const continueBest = switchRow(mainGroup, t("AutoPlay.chkContinueWithBestMove"), product("autoplayAppliesNow"), s.continueWithBestMove).input;
  const { box: subCheck, field: subField } = timed(group(body), t("AutoPlay.chkAutoPlaySubbord"), sub.running, intervalText(s.autoplaySubMillis));
  const branchGroup = group(body);
  const { box: branchCheck, field: branchField } = timed(branchGroup, t("AutoPlay.chkAutoPlayBranch"), s.autoReplayBranch, String(Math.trunc(s.replayIntervalSeconds * 1000)));
  const { box: entireCheck, field: entireField } = timed(branchGroup, t("AutoPlay.chbDisplayEntireVariationFirst"), s.entireVariationFirst, javaDouble(s.entireVariationFirstSeconds));
  const ok = primaryButton(t("AutoPlay.okButton"), () => {});
  ok.dataset.key = "autoplay.ok";
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
  const dialog = formPanel(t("AutoPlay.title"), body);
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "autoplay.cancel";
  footer(body, { primary: ok, cancel });
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
