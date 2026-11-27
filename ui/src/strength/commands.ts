// 棋力评估 entries of the command catalog: 分析 > 棋力评估 (Menu.java:3501-3509, FunctionCatalog
// menu.playerStrengthEstimate) and the top toolbar's 棋力评估 button (Menu.java:8386-8404), both
// LizzieFrame.showPlayerStrengthEstimate, and function search's 分阶段, the dashboard at its
// 分阶段 page.
import { COMMANDS, type CommandEntry } from "../commands";
import { locateStrengthPhases, openStrengthDashboard } from "./dashboard";

Object.assign(COMMANDS, {
  "menu.playerStrengthEstimate": { run: () => void openStrengthDashboard() },
  "strength.phase-bounds": { run: () => void locateStrengthPhases(), locate: locateStrengthPhases },
} satisfies Record<string, CommandEntry>);
