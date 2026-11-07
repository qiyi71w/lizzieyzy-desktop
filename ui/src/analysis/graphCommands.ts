// 显示 > 胜率图设置 entries of the command catalog (Menu.java:1532-1659, check states 2748-2767):
// 显示胜率/目数曲线 胜率 / 目数 / 全部, 单曲线模式使用面积填充, 显示恶手损失条,
// 显示鼠标悬停信息(仅限黑视角) and 初始最大目数差. Each choice saves through SetDisplaySettings.
import { state } from "../app";
import { COMMANDS, setDisplay, type CommandEntry } from "../commands";
import { ask, message } from "../dialogs";
import { product, t } from "../i18n";
import { javaFixed1 } from "./graph";

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

/** Double.parseDouble after its whitespace trim: a signed decimal with an optional exponent and
 *  float / double suffix. NaN and Infinity are refused: they would leave the score scale undefined
 *  and cannot be saved. */
function parseJavaDouble(text: string): number | null {
  const s = text.trim();
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[fFdD]?$/.test(s)) return null;
  const v = Number(s.replace(/[fFdD]$/, ""));
  return Number.isFinite(v) ? v : null;
}

/** 初始最大目数差 (Menu.java:1626-1659): JOptionPane input with the current value; the absolute
 *  value is saved, other text shows Menu.inputDoubleTip. */
async function chooseInitialMaxScoreLead(): Promise<void> {
  const s = display();
  if (!s) return;
  const text = `${t("Menu.initialMaxScoreLead.message")}${javaFixed1(Math.abs(s.initialMaxScoreLead))}`;
  const body = document.createElement("div");
  body.className = "display-input";
  const label = document.createElement("div");
  label.textContent = text;
  const field = document.createElement("input");
  field.type = "text";
  field.setAttribute("aria-label", text);
  body.append(label, field);
  queueMicrotask(() => field.focus());
  if (!(await ask(t("Menu.initialMaxScoreLead.title"), body, product("ok"), product("cancel")))) return;
  const value = parseJavaDouble(field.value);
  if (value === null) await message(t("Message.title"), t("Menu.inputDoubleTip"));
  else setDisplay({ initialMaxScoreLead: Math.abs(value) });
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
  "menu.initialMaxScoreLead": { run: chooseInitialMaxScoreLead },
} satisfies Record<string, CommandEntry>);
