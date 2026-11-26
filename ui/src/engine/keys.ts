// Input.java:336, 438-440, 606-608, 751-755, 784-834.
import { run } from "../app";
import { inspectMatchRules } from "../match/rulesDetails";
import { openMoreEngines } from "./catalog";
import { toggleConsole } from "./console";
import { openEngineParameters } from "./kataParams";
import { openRules } from "./rules";

/** LizzieFrame.inspectMatchRulesOrSetRules: 本局规则详情 when the caption shows 本局规则, else the
 *  rules dialog. */
async function inspectMatchRulesOrSetRules(): Promise<void> {
  if (!(await inspectMatchRules())) await openRules();
}

export function handleEngineKey(event: KeyboardEvent): boolean {
  if (event.metaKey || event.repeat) return false;
  const key = event.key.toLowerCase();
  if (!event.altKey && !event.ctrlKey && !event.shiftKey) {
    if (event.code === "Space" || key === " ") { void run({ type: "ToggleAnalysis" }); return true; }
    if (key === "e") { void toggleConsole(); return true; }
  }
  if (event.altKey && !event.ctrlKey && !event.shiftKey && key === "x") { void openMoreEngines(); return true; }
  // VK_D with Alt, whatever else is held (Input.java:750-752).
  if (event.altKey && key === "d") { void openEngineParameters(); return true; }
  if (event.shiftKey && !event.altKey && !event.ctrlKey && key === "d") { void inspectMatchRulesOrSetRules(); return true; }
  if (event.ctrlKey && !event.altKey && !event.shiftKey && /^[0-9]$/.test(key)) {
    void run({ type: "SwitchEngine", index: key === "0" ? 9 : Number(key) - 1 });
    return true;
  }
  return false;
}
