// The document state the design tokens select on: operating system and colour mode. Importing
// this module sets data-os and data-theme before the first paint. The workbench window sets the
// mode from the appearance setting; the program's other windows follow it through the storage
// they share, at once and when they open later.

/** 外观模式: follow the system, or always light or dark. */
export type ThemeMode = "System" | "Light" | "Dark";
export type ResolvedTheme = "light" | "dark";
export type Os = "windows" | "mac" | "linux";

/** Where the workbench window leaves the mode for the other windows and for the next start. */
const STORE = "lizzieyzy.themeMode";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
const listeners: (() => void)[] = [];
let mode = storedMode();

function storedMode(): ThemeMode {
  const stored = localStorage.getItem(STORE);
  return stored === "Light" || stored === "Dark" ? stored : "System";
}

export const os: Os = navigator.userAgent.includes("Windows") ? "windows" : navigator.userAgent.includes("Mac OS") ? "mac" : "linux";

/** The theme the tokens resolve to now. */
export function resolvedTheme(): ResolvedTheme {
  return mode === "Dark" || (mode === "System" && systemDark.matches) ? "dark" : "light";
}

function apply(): void {
  const next = resolvedTheme();
  const root = document.documentElement;
  if (root.dataset.theme === next) return;
  root.dataset.theme = next;
  for (const listener of listeners) listener();
}

export function themeMode(): ThemeMode {
  return mode;
}

export function setThemeMode(next: ThemeMode): void {
  mode = next;
  if (storedMode() !== next) localStorage.setItem(STORE, next);
  apply();
}

/** Called after data-theme changed; canvas painters re-read their token colours here. */
export function onThemeChange(listener: () => void): void {
  listeners.push(listener);
}

/** A token's current value, e.g. token("--ds-text"). */
export function token(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

document.documentElement.dataset.os = os;
systemDark.addEventListener("change", apply);
// Another window of the program changed the mode.
window.addEventListener("storage", (e) => {
  if (e.key !== STORE) return;
  mode = storedMode();
  apply();
});
apply();
