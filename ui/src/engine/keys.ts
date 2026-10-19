// Input.java:336, 438-440, 606-608, 753-755, 784-834.
import { run } from "../app";
import { openMoreEngines } from "./catalog";
import { toggleConsole } from "./console";
import { openRules } from "./rules";

export function handleEngineKey(event: KeyboardEvent): boolean {
  if (event.metaKey || event.repeat) return false;
  const key = event.key.toLowerCase();
  if (!event.altKey && !event.ctrlKey && !event.shiftKey) {
    if (event.code === "Space" || key === " ") { void run({ type: "ToggleAnalysis" }); return true; }
    if (key === "e") { void toggleConsole(); return true; }
  }
  if (event.altKey && !event.ctrlKey && !event.shiftKey && key === "x") { void openMoreEngines(); return true; }
  if (event.shiftKey && !event.altKey && !event.ctrlKey && key === "d") { void openRules(); return true; }
  if (event.ctrlKey && !event.altKey && !event.shiftKey && /^[0-9]$/.test(key)) {
    void run({ type: "SwitchEngine", index: key === "0" ? 9 : Number(key) - 1 });
    return true;
  }
  return false;
}
