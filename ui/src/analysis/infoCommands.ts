// Analysis information entries of the command catalog: KataGo设置 > Kata形势显示 (Menu.java:2227-2597,
// checks 2664-2718), 棋局 > 终局点目 (Menu.java:3061-3070, check 3177) and their keys: `.` with
// 使用快捷键(大键盘点) and Ctrl+Q (Input.java:717-724, 743-748).
import { run, state } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import type { DisplaySettings, EstimateStyle, KataEstimateMode } from "../contract/generated";
import { toggleScore } from "./scoreDialog";

const display = (): DisplaySettings | undefined => state.view?.settings.display;

/** LizzieFrame.toggleShowKataEstimate (`.`, bottom bar Kata形势); the core ignores it without a
 *  KataGo engine. */
export function toggleKataEstimate(): void {
  void run({ type: "ToggleKataEstimate" });
}

/** The menu listener's checks: 关闭 while off; on, the boards it shows on, 隐藏 with neither. */
function shownMode(): KataEstimateMode {
  const s = state.view?.kata_estimate.state;
  if (!s?.shown) return "Close";
  if (s.onMain && s.onSub) return "Both";
  if (s.onMain) return "Main";
  return s.onSub ? "Sub" : "Hide";
}

function mode(m: KataEstimateMode): CommandEntry {
  return { run: () => void run({ type: "SetKataEstimate", mode: m }), checked: () => shownMode() === m };
}

/** A square style; the core turns Kata形势 on for a KataGo engine (Menu.java:2388-2389). */
function style(s: EstimateStyle): CommandEntry {
  return { run: () => setDisplay({ kataEstimateStyle: s }), checked: () => display()?.kataEstimateStyle === s };
}

function flag(key: "kataEstimateSaveStatus" | "useMovesOwnership" | "kataEstimateShortcut"): CommandEntry {
  return { run: () => setDisplay({ [key]: !display()?.[key] } as Partial<DisplaySettings>), checked: () => display()?.[key] ?? false };
}

Object.assign(COMMANDS, {
  "kataEstimate.Close": mode("Close"),
  "kataEstimate.Hide": mode("Hide"),
  "kataEstimate.Main": mode("Main"),
  "kataEstimate.Sub": mode("Sub"),
  "kataEstimate.Both": mode("Both"),
  "menu.kataEstimateSaveState": flag("kataEstimateSaveStatus"),
  "menu.kataEstimateByTransparentSmall": style("Small"),
  "kataEstimate.Transparent": style("Transparent"),
  "kataEstimate.TransparentNotOnLive": style("TransparentNotOnLive"),
  "kataEstimate.BigBelow": style("BigBelow"),
  "kataEstimate.BySize": style("BySize"),
  "menu.useMovesOwnership": flag("useMovesOwnership"),
  "menu.useKataEstimateShortcut": flag("kataEstimateShortcut"),
  "menu.scoreGame": { run: toggleScore, checked: () => !!state.view?.score },
} satisfies Record<string, CommandEntry>);

/** `.` (VK_PERIOD, the main keyboard's) and Ctrl+Q; returns whether the key was handled. Plain
 *  and Alt+Q belong to other features. */
export function handleInfoKey(e: KeyboardEvent): boolean {
  if (e.code === "Period") {
    if (display()?.kataEstimateShortcut) toggleKataEstimate();
    return true;
  }
  if (e.code === "KeyQ" && e.ctrlKey) {
    toggleScore();
    return true;
  }
  return false;
}
