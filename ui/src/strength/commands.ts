// 棋力评估 entry of the command catalog: 分析 > 棋力评估 (Menu.java:3501-3509, FunctionCatalog
// menu.playerStrengthEstimate) and the top toolbar's 棋力评估 button (Menu.java:8386-8404), both
// LizzieFrame.showPlayerStrengthEstimate.
import { COMMANDS, type CommandEntry } from "../commands";
import { openStrengthDashboard } from "./dashboard";

Object.assign(COMMANDS, {
  "menu.playerStrengthEstimate": { run: () => void openStrengthDashboard() },
} satisfies Record<string, CommandEntry>);
