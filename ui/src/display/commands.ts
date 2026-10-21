// Board display entries of the command catalog: 显示 > 坐标 / 手数 / 落子评价标记 / 推荐点 / 下一手 /
// 小棋盘设置 / 自动播放 / 主界面设置 > 棋盘下方显示黑白名字 / KataGo设置 > 形势判断 > 纯网络形势,
// 分析 > 纯网络 / 策略网络, the top toolbar's 选点 黑 / 白 and 落子评价 button, the bottom toolbar's
// 手数 / 坐标 / 纯网络 / 自动播放 and the display shortcuts.
import { state } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import type { DisplaySettings, MoveRankMode, SubBoardMode } from "../contract/generated";
import { message } from "../dialogs";
import { t } from "../i18n";
import { openAutoPlayDialog } from "./autoplay";
import { inputDialog, openCandidatesDelayDialog, openInfoOrderDialog, openMoveNumberDialog, openReplayIntervalDialog, parseJavaInt } from "./dialogs";
import { boardUi, setCandidates, togglePolicy, toggleRawNet } from "./state";

const display = (): DisplaySettings | undefined => state.view?.settings.display;

type Flag = { [K in keyof DisplaySettings]: DisplaySettings[K] extends boolean ? K : never }[keyof DisplaySettings];

/** A check box item flipping one saved flag. */
function flag(key: Flag): CommandEntry {
  return {
    run: () => setDisplay({ [key]: !display()?.[key] } as Partial<DisplaySettings>),
    checked: () => display()?.[key] ?? false,
  };
}

/** Config.toggleCoordinates (C, 坐标). */
export function toggleCoordinates(): void {
  setDisplay({ coordinates: !display()?.coordinates });
}

/** Config.toggleShowMoveNumber (M, 手数): 全部 → 最近1手 → 不显示 → 全部 (onlyLastMoveNumber is
 *  forced to 1 first; no engine game is played here, so the genmove branch never applies). */
export function toggleMoveNumbers(): void {
  const n = display()?.moveNumbers ?? 0;
  setDisplay({ moveNumbers: n === -1 ? 1 : n === 0 ? -1 : 0 });
}

/** Config.toggleShowMoveAllInBranch (Ctrl+M, 分支内总是显示所有手数). */
export function toggleAllNumbersInBranch(): void {
  setDisplay({ allNumbersInBranch: !display()?.allNumbersInBranch });
}

/** Config.toggleShowMoveRankMark (Alt+M, 落子评价): 不显示 → 最后一手 → 最后多手 (when more than one)
 *  → 所有 → 不显示; showing the last moves hides the move numbers. */
export function toggleMoveRank(): void {
  const s = display();
  if (!s) return;
  let rank = s.moveRank;
  if (rank < 0) rank = 1;
  else if (rank === 1) rank = s.moveRankCustom > 1 ? s.moveRankCustom : 0;
  else if (s.moveRankCustom > 1 && rank === s.moveRankCustom) rank = 0;
  else if (rank === 0) rank = -1;
  setDisplay(rank > 0 ? { moveRank: rank, moveNumbers: 0 } : { moveRank: rank });
}

/** Config.toggleShowNextMoves (J): 详细 → 简易 → 不显示 → 详细. */
export function toggleNextMoves(): void {
  const s = display();
  if (!s) return;
  if (s.nextMoves && s.nextMoveInfo) setDisplay({ nextMoveInfo: false });
  else if (s.nextMoves) setDisplay({ nextMoves: false });
  else setDisplay({ nextMoves: true, nextMoveInfo: true });
}

/** Config.toggleShowSuggestionVariations (Shift+F, Alt+V). */
export function toggleSuggestionVariations(): void {
  setDisplay({ suggestionVariations: !display()?.suggestionVariations });
}

/** 落子评价标记 状态: the move numbers hide the marks, so 不显示 is checked while they show. */
const rankShown = (rank: (s: DisplaySettings) => boolean): (() => boolean) => () => {
  const s = display();
  return !!s && s.moveNumbers === 0 && rank(s);
};

/** Menu.setBtnRankMark: lit while no move numbers show and the marks are on. */
export function rankMarkLit(): boolean {
  const s = display();
  return !!s && s.moveNumbers === 0 && s.moveRank >= 0;
}

/** rankCustomMove's text field (DocumentListener): every edit saves max(1, value), keeping the
 *  previous value while the text is not a number. */
export function setRankCustomText(text: string): void {
  const s = display();
  if (!s) return;
  typedRankCustom = Math.max(1, parseJavaInt(text.trim()) ?? typedRankCustom ?? s.moveRankCustom);
  setDisplay({ moveRankCustom: typedRankCustom });
}

/** The count last typed into the item, which its click uses even before the saved setting returns. */
let typedRankCustom: number | null = null;

/** rankCustomMove click: hiddenMoveNumber, then the typed count is the marks' range. */
function showRankCustom(): void {
  const custom = typedRankCustom ?? display()?.moveRankCustom ?? 3;
  typedRankCustom = null;
  setDisplay({ moveNumbers: 0, moveRankCustom: custom, moveRank: custom });
}

/** 显示最后多手: (setCustomMoves, Menu.java:9365-9397): JOptionPane input; the new count is also
 *  the marks' range. A non-integer shows Menu.inputIntegerHint. */
async function chooseRankCustom(): Promise<void> {
  const s = display();
  if (!s) return;
  const text = await inputDialog(t("Menu.rankMenu.setCustomMoves.message"), String(s.moveRankCustom));
  if (text === null) return;
  const n = parseJavaInt(text);
  if (n === null) {
    await message(t("Message.title"), t("Menu.inputIntegerHint"));
    return;
  }
  const custom = Math.max(1, n);
  setDisplay({ moveRankCustom: custom, moveRank: custom });
}

/** 下一手提示需要计算量 (Menu.java:690-723): any Java int, negative ones too (Integer.parseInt). */
async function chooseNextMoveMinPlayouts(): Promise<void> {
  const s = display();
  if (!s) return;
  const text = await inputDialog(t("Menu.minPlayoutsForNextMoveMessage"), String(s.nextMoveMinPlayouts));
  if (text === null) return;
  const n = parseJavaInt(text);
  if (n === null) await message(t("Message.title"), t("Menu.inputIntegerHint"));
  else setDisplay({ nextMoveMinPlayouts: n });
}

function moveNumbers(n: number): CommandEntry {
  return { run: () => setDisplay({ moveNumbers: n }), checked: () => display()?.moveNumbers === n };
}

function rankMode(mode: MoveRankMode): CommandEntry {
  return { run: () => setDisplay({ moveRankMode: mode }), checked: () => display()?.moveRankMode === mode };
}

function subBoard(mode: SubBoardMode): CommandEntry {
  return { run: () => setDisplay({ subBoard: mode }), checked: () => display()?.subBoard === mode };
}

function pureEstimate(mode: DisplaySettings["pureEstimate"]): CommandEntry {
  return { run: () => setDisplay({ pureEstimate: mode }), checked: () => display()?.pureEstimate === mode };
}

function nextMoves(shown: boolean, info: boolean): CommandEntry {
  return {
    run: () => setDisplay({ nextMoves: shown, nextMoveInfo: info }),
    checked: () => {
      const s = display();
      return !!s && s.nextMoves === shown && (!shown || s.nextMoveInfo === info);
    },
  };
}

/** 选点 黑 / 白 (chkShowBlack / chkShowWhite): one colour's candidates. */
export function setCandidateSide(black: boolean, shown: boolean): void {
  setCandidates(black ? shown : boardUi.showBlack, black ? boardUi.showWhite : shown);
}

const PRESET_MOVE_NUMBERS = [0, 1, 5, 10, -1];

Object.assign(COMMANDS, {
  "menu.coordsMenu": { run: toggleCoordinates, checked: () => display()?.coordinates ?? false },
  // 手数 (Menu.java:799-964, checks 2830-2896)
  "Menu.noMoveNum": moveNumbers(0),
  "Menu.lastOneMoveNum": moveNumbers(1),
  "Menu.lastFiveMoveNum": moveNumbers(5),
  "Menu.lastTenMoveNum": moveNumbers(10),
  "Menu.allMoveNum": moveNumbers(-1),
  "Menu.anyMoveNum": { run: openMoveNumberDialog, checked: () => !PRESET_MOVE_NUMBERS.includes(display()?.moveNumbers ?? 0) },
  "menu.moveNumberAlwaysFromOne": {
    // also clears 分支内总是显示所有手数
    run: () => setDisplay({ moveNumbersFromOne: !display()?.moveNumbersFromOne, allNumbersInBranch: false }),
    checked: () => display()?.moveNumbersFromOne ?? false,
    enabled: () => display()?.moveNumbers !== -1,
  },
  "menu.moveNumberInBranchFromOne": { run: () => setDisplay({ branchNumbersFromOne: true }), checked: () => display()?.branchNumbersFromOne ?? true },
  "Menu.moveNumberInBracnhFromOneContinue": { run: () => setDisplay({ branchNumbersFromOne: false }), checked: () => !(display()?.branchNumbersFromOne ?? true) },
  "menu.showAllMoveNumberInBranch": {
    run: toggleAllNumbersInBranch,
    checked: () => !!display()?.allNumbersInBranch || display()?.moveNumbers === -1,
    enabled: () => display()?.moveNumbers !== -1,
  },
  "menu.showMoveNumberOnVariationPane": flag("treeMoveNumbers"),
  // 落子评价标记 (addRankMarkMenu, Menu.java:9233-9562)
  "Menu.rankMenu.rankNoneMove": {
    run: () => setDisplay({ moveRank: -1 }),
    checked: () => {
      const s = display();
      return !!s && (s.moveNumbers !== 0 || s.moveRank === -1);
    },
  },
  "Menu.rankMenu.rankLastMove": { run: () => setDisplay({ moveNumbers: 0, moveRank: 1 }), checked: rankShown((s) => s.moveRank === 1) },
  "Menu.rankMenu.lblCustomMove": {
    run: showRankCustom,
    checked: rankShown((s) => s.moveRank > 1 && s.moveRank === s.moveRankCustom),
  },
  "Menu.rankMenu.setCustomMoves": { run: chooseRankCustom, checked: rankShown((s) => s.moveRank > 1 && s.moveRank === s.moveRankCustom) },
  "Menu.rankMenu.rankAllMove": { run: () => setDisplay({ moveNumbers: 0, moveRank: 0 }), checked: rankShown((s) => s.moveRank === 0) },
  "Menu.rankMenu.evaluationMode.auto": rankMode("Auto"),
  "Menu.rankMenu.evaluationMode.score": rankMode("Score"),
  "Menu.rankMenu.evaluationMode.winrate": rankMode("Winrate"),
  "Menu.rankMenu.evaluationMode.combined": rankMode("Combined"),
  // 推荐点 (Menu.java:1076-1673, checks 2630-2747)
  "menu.suggestion1": flag("suggestionWinrate"),
  "menu.suggestion2": flag("suggestionPlayouts"),
  "menu.suggestion3": flag("suggestionScore"),
  "menu.showScoreAsDiff": flag("scoreAsDiff"),
  "menu.customInfoOrdr": { run: openInfoOrderDialog },
  "menu.setCandidatesDelay": { run: openCandidatesDelayDialog, checked: () => display()?.delayCandidates ?? false },
  "menu.showSuggestionOrder": flag("suggestionOrder"),
  "menu.showMaxValueReverse": flag("suggestionMaxRed"),
  "menu.showWhiteSuggestWhite": flag("whiteSuggestionWhite"),
  "menu.showVariationOnMouse": { run: toggleSuggestionVariations, checked: () => display()?.suggestionVariations ?? true },
  "menu.noRefreshOnMouse": flag("noRefreshOnMouse"),
  "menu.setReplayInterval": { run: openReplayIntervalDialog },
  // 下一手 (Menu.java:654-723, checks 2613-2626)
  "Menu.nextMoveHintNone": nextMoves(false, false),
  "Menu.nextMoveHintSimple": nextMoves(true, false),
  "Menu.nextMoveHintInformation": nextMoves(true, true),
  "menu.minPlayoutsForNextMove": { run: chooseNextMoveMinPlayouts },
  // 自动播放(Ctrl+A) (Menu.java:643-652) and the bottom toolbar's 自动播放 (BottomToolbar.java:1296-1302)
  "menu.makeAutoPlay": { run: openAutoPlayDialog },
  // 主界面设置 > 棋盘下方显示黑白名字 (show-name-in-board)
  "menu.showNameInBoard": flag("namesOnBoard"),
  // 小棋盘设置 (Menu.java:1684-1827, checks 2912-2935)
  "Menu.showHeat": subBoard("Policy"),
  "Menu.showHeatAfterCalc": subBoard("Visits"),
  "Menu.notShowHeat": { run: () => setDisplay({ subBoard: "Variation" }), checked: () => display()?.subBoard === "Variation" || display()?.subBoard === "Raw" },
  "menu.subBoardShowVar": subBoard("Variation"),
  "Menu.subBoardShowRaw": subBoard("Raw"),
  // KataGo设置 > 形势判断 > 纯网络形势 (Menu.java:2474-2569, checks 2719-2741)
  "pureEstimate.Small": pureEstimate("Small"),
  "pureEstimate.Transparent": pureEstimate("Transparent"),
  "pureEstimate.TransparentNotOnLive": pureEstimate("TransparentNotOnLive"),
  "pureEstimate.BigBelow": pureEstimate("BigBelow"),
  "pureEstimate.BySize": pureEstimate("BySize"),
  // 分析 > 纯网络 (H) / 策略网络 (T) (Menu.java:3512-3530, check 3589)
  "menu.showHeatmap": { run: toggleRawNet },
  "menu.showPolicy": { run: togglePolicy, checked: () => boardUi.policy },
} satisfies Record<string, CommandEntry>);
