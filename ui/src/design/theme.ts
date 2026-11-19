// The document state the design tokens select on: operating system, language and colour mode.
// Importing this module sets data-os and data-theme before the first paint; the appearance
// setting then chooses the mode.

/** 外观模式: follow the system, or always light or dark. */
export type ThemeMode = "System" | "Light" | "Dark";
export type ResolvedTheme = "light" | "dark";
export type Os = "windows" | "mac" | "linux";

const systemDark = window.matchMedia("(prefers-color-scheme: dark)");
const listeners: (() => void)[] = [];
let mode: ThemeMode = "System";

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

const LANG: Record<string, string> = { ZhCn: "zh-CN", ZhTw: "zh-TW", EnUs: "en", JaJp: "ja", KoKr: "ko", ThTh: "th" };

/** The interface language as the document language, so the system font picks that language's
 *  glyphs (Chinese, Japanese and Korean share code points). */
export function setDocumentLanguage(language: string): void {
  document.documentElement.lang = LANG[language] ?? "zh-CN";
}

document.documentElement.dataset.os = os;
systemDark.addEventListener("change", apply);
apply();
