// 显示 > 胜率图设置 entries of the command catalog (Menu.java:1532-1659, check states 2748-2767):
// 显示胜率/目数曲线 胜率 / 目数 / 全部, 单曲线模式使用面积填充, 显示恶手损失条,
// 显示鼠标悬停信息(仅限黑视角) and 初始最大目数差. Each choice saves through SetDisplaySettings;
// 初始最大目数差 opens its 综合设置 row.
import { state } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import { openSettingsAt } from "../engine/settingsTargets";

const display = () => state.view?.settings.display;

/** One 显示胜率/目数曲线 choice; checked when both flags match it. */
function lines(winrate: boolean, score: boolean): CommandEntry {
  return {
    run: () => setDisplay({ winrateLine: winrate, scoreLeadLine: score }),
    checked: () => {
      const s = display();
      return !!s && s.winrateLine === winrate && s.scoreLeadLine === score;
    },
  };
}

Object.assign(COMMANDS, {
  "Menu.showWinRateOrScoreLeadLine.winrate": lines(true, false),
  "Menu.showWinRateOrScoreLeadLine.scoreLead": lines(false, true),
  "Menu.showWinRateOrScoreLeadLine.both": lines(true, true),
  "menu.showWinrateGraphFill": { run: () => setDisplay({ winrateGraphFill: !display()?.winrateGraphFill }), checked: () => display()?.winrateGraphFill ?? true },
  "menu.showBlunderBar": { run: () => setDisplay({ blunderBar: !display()?.blunderBar }), checked: () => display()?.blunderBar ?? false },
  // Turning it off clears the hovered node (Lizzie.frame.clearMouseOverWinrateGraph); main.ts drops
  // the hover while the setting is off.
  "menu.showMouseOverWinrateGraph": {
    run: () => setDisplay({ mouseOverWinrateGraph: !display()?.mouseOverWinrateGraph }),
    checked: () => display()?.mouseOverWinrateGraph ?? true,
  },
  "menu.initialMaxScoreLead": { run: () => void openSettingsAt("menu.initialMaxScoreLead") },
} satisfies Record<string, CommandEntry>);
