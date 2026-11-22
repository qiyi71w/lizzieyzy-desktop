// The document state the design tokens select on: operating system, colour mode, skin, the
// theme's 界面颜色 and type sizes. Importing this module sets data-os, data-theme, data-skin, the
// interface colours and the default type sizes before the first paint. The workbench window sets
// the mode, the skin and the colours from the appearance setting; the program's other windows
// follow them through the storage they share, at once and when they open later.
import { invoke } from "@tauri-apps/api/core";
import { paletteTokens, SKIN_COLORS, skinColorsOf, type InterfaceColors, type SkinColors } from "./palette";

/** 外观模式: follow the system, or always light or dark; light by default. */
export type ThemeMode = "System" | "Light" | "Dark";
export type ResolvedTheme = "light" | "dark";
export type Os = "windows" | "mac" | "linux";

/** The token values the interface draws with: the program's own, Java's (the bundled Java 经典
 *  theme) or Java's Morandi palette (the bundled Java 柔和 theme). The same components either way;
 *  only colours and the workbench wallpaper differ. */
export type Skin = "modern" | "java" | "soft";

/** Where the workbench window leaves the mode, the skin and the interface colours for the other
 *  windows and for the next start. */
const STORE = "lizzieyzy.themeMode";
const SKIN_STORE = "lizzieyzy.skin";
const COLORS_STORE = "lizzieyzy.interfaceColors";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
const listeners: (() => void)[] = [];
let mode = storedMode();
let currentSkin = storedSkin();
let colors = storedColors();
/** The root style properties the interface colours set last. */
let colored: string[] = [];

function storedMode(): ThemeMode {
  const stored = localStorage.getItem(STORE);
  return stored === "System" || stored === "Dark" ? stored : "Light";
}

function storedSkin(): Skin {
  const stored = localStorage.getItem(SKIN_STORE);
  return stored === "java" || stored === "soft" ? stored : "modern";
}

function storedColors(): InterfaceColors {
  let parsed: unknown;
  try {
    parsed = JSON.parse(localStorage.getItem(COLORS_STORE) ?? "null");
  } catch {
    return SKIN_COLORS;
  }
  if (typeof parsed !== "object" || parsed === null) return SKIN_COLORS;
  const stored: object = parsed;
  const color = (key: string): number[] => {
    const value: unknown = Reflect.get(stored, key);
    return Array.isArray(value) && value.length === 3 && value.every((v) => Number.isInteger(v) && v >= 0 && v <= 255) ? value : [];
  };
  return { chrome: color("chrome"), control: color("control"), accent: color("accent"), text: color("text") };
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

/** Hidden elements that take one skin's token values, whatever is set on the root. */
const probes: Partial<Record<Skin, HTMLElement>> = {};

/** The skin's own colours (a Java skin's or the program's, the latter in the current colour mode),
 *  without any interface colour a theme set. */
export function skinColors(which: Skin): SkinColors {
  let probe = probes[which];
  if (!probe?.isConnected) {
    probe = document.createElement("div");
    probe.hidden = true;
    document.documentElement.append(probe);
    probes[which] = probe;
  }
  if (which === "modern") probe.dataset.theme = resolvedTheme();
  else probe.dataset.skin = which;
  return skinColorsOf(getComputedStyle(probe));
}

/** Sets on the root the tokens the theme's interface colours change for the current skin. */
function applyColors(): void {
  const root = document.documentElement.style;
  for (const name of colored) root.removeProperty(name);
  colored = [];
  if (![colors.chrome, colors.control, colors.accent, colors.text].some((c) => c.length > 0)) return;
  const tokens = paletteTokens(colors, skinColors(currentSkin));
  colored = Object.keys(tokens);
  for (const name of colored) root.setProperty(name, tokens[name]);
}

function changed(): void {
  for (const listener of listeners) listener();
}

function apply(): void {
  const next = resolvedTheme();
  const root = document.documentElement;
  if (root.dataset.theme === next && root.dataset.skin === currentSkin) return;
  root.dataset.theme = next;
  root.dataset.skin = currentSkin;
  applyColors();
  changed();
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

/** The theme's 界面颜色 (design/palette.ts); an empty colour is the skin's own. */
export function setInterfaceColors(next: InterfaceColors): void {
  const stored = JSON.stringify(next);
  if (stored === JSON.stringify(colors)) return;
  colors = next;
  if (localStorage.getItem(COLORS_STORE) !== stored) localStorage.setItem(COLORS_STORE, stored);
  applyColors();
  changed();
}

/** Called after data-theme, data-skin or the interface colours changed; canvas painters re-read
 *  their token colours here. */
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
// Another window of the program changed the mode, the skin or the interface colours.
window.addEventListener("storage", (e) => {
  if (e.key === STORE) mode = storedMode();
  else if (e.key === SKIN_STORE) currentSkin = storedSkin();
  else if (e.key === COLORS_STORE) {
    colors = storedColors();
    applyColors();
    changed();
    return;
  } else return;
  apply();
});
apply();
