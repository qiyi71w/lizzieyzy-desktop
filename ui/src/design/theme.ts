// The document state the design tokens select on: operating system, colour mode, skin and type
// sizes. Importing this module sets data-os, data-theme, data-skin and the default type sizes
// before the first paint. The workbench window sets the mode and the skin from the appearance
// setting; the program's other windows follow them through the storage they share, at once and
// when they open later.
import { invoke } from "@tauri-apps/api/core";

/** 外观模式: follow the system, or always light or dark; light by default. */
export type ThemeMode = "System" | "Light" | "Dark";
export type ResolvedTheme = "light" | "dark";
export type Os = "windows" | "mac" | "linux";

/** The token values the interface draws with: the program's own, or Java's (the bundled Java 经典
 *  theme). The same components either way; only colours and the workbench wallpaper differ. */
export type Skin = "modern" | "java";

/** Where the workbench window leaves the mode and the skin for the other windows and for the next
 *  start. */
const STORE = "lizzieyzy.themeMode";
const SKIN_STORE = "lizzieyzy.skin";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
const listeners: (() => void)[] = [];
let mode = storedMode();
let currentSkin = storedSkin();

function storedMode(): ThemeMode {
  const stored = localStorage.getItem(STORE);
  return stored === "System" || stored === "Dark" ? stored : "Light";
}

function storedSkin(): Skin {
  return localStorage.getItem(SKIN_STORE) === "java" ? "java" : "modern";
}

export const os: Os = navigator.userAgent.includes("Windows") ? "windows" : navigator.userAgent.includes("Mac OS") ? "mac" : "linux";

/** Whether a dialog's cancel button comes before its affirmative one (DESIGN.md Buttons): after it
 *  on Windows, before it on macOS, and on Linux as the desktop's GTK dialogs order them (GNOME's
 *  cancel-first unless the desktop turns on GTK's alternative order). */
export let cancelFirst = os !== "windows";
if (os === "linux") void invoke<boolean>("gtk_cancel_first").then((first) => (cancelFirst = first));

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
  if (root.dataset.theme === next && root.dataset.skin === currentSkin) return;
  root.dataset.theme = next;
  root.dataset.skin = currentSkin;
  for (const listener of listeners) listener();
}

export function setThemeMode(next: ThemeMode): void {
  mode = next;
  if (storedMode() !== next) localStorage.setItem(STORE, next);
  apply();
}

export function skin(): Skin {
  return currentSkin;
}

export function setSkin(next: Skin): void {
  currentSkin = next;
  if (storedSkin() !== next) localStorage.setItem(SKIN_STORE, next);
  apply();
}

/** Called after data-theme or data-skin changed; canvas painters re-read their token colours here. */
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
// Another window of the program changed the mode or the skin.
window.addEventListener("storage", (e) => {
  if (e.key === STORE) mode = storedMode();
  else if (e.key === SKIN_STORE) currentSkin = storedSkin();
  else return;
  apply();
});
apply();
