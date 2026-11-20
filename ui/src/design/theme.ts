// The document state the design tokens select on: operating system, colour mode and type sizes.
// Importing this module sets data-os, data-theme and the default type sizes before the first
// paint. The workbench window sets the mode from the appearance setting; the program's other
// windows follow it through the storage they share, at once and when they open later.

/** 外观模式: follow the system, or always light or dark; light by default. */
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
  return stored === "System" || stored === "Dark" ? stored : "Light";
}

export const os: Os = navigator.userAgent.includes("Windows") ? "windows" : navigator.userAgent.includes("Mac OS") ? "mac" : "linux";

/** The dark set is unfinished: until it is, every window shows the light set whatever the saved
 *  mode, and the 外观模式 menu stays hidden. */
export const DARK_READY = false;

/** The theme the tokens resolve to now. */
export function resolvedTheme(): ResolvedTheme {
  return DARK_READY && (mode === "Dark" || (mode === "System" && systemDark.matches)) ? "dark" : "light";
}

/** The interface's type steps in px (DESIGN.md Typography): caption, body, title, headline. */
export interface TypeSizes {
  s: number;
  size: number;
  l: number;
  xl: number;
}

/** The type steps at 界面字体大小 `frame`: captions at that size under the platform's own body
 *  text, which at the default 12 is 13 px on macOS and 14 px on Windows and Linux (Fluent's body). */
export function typeSizes(frame: number): TypeSizes {
  const size = frame + (os === "mac" ? 1 : 2);
  return { s: frame, size, l: size + 1, xl: size + 3 };
}

/** Sets the stylesheets' --ds-size steps for 界面字体大小 `frame`. */
export function applyTypeSizes(frame: number): void {
  const sizes = typeSizes(frame);
  const root = document.documentElement.style;
  root.setProperty("--ds-size-s", `${sizes.s}px`);
  root.setProperty("--ds-size", `${sizes.size}px`);
  root.setProperty("--ds-size-l", `${sizes.l}px`);
  root.setProperty("--ds-size-xl", `${sizes.xl}px`);
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
// The default 界面字体大小 until a window learns the session's.
applyTypeSizes(12);
systemDark.addEventListener("change", apply);
// Another window of the program changed the mode.
window.addEventListener("storage", (e) => {
  if (e.key !== STORE) return;
  mode = storedMode();
  apply();
});
apply();
