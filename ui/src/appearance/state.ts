// The committed appearance every painter draws with: the core's appearance and theme values plus
// their decoded images. A change swaps images only after all of them decoded, and only if no newer
// change arrived meanwhile (Menu.refreshCustomBackgrounds / Config.readThemeVaule clear every board
// and wallpaper cache, Menu.java:7075-7122, Config.java:2287-2323). The frame font size is the
// value this session started with (LizzieFrame.java:20764-20767: applied after a restart).
import type { AppearanceSettings, ThemeImages, ThemeSettings, WorkbenchView } from "../contract/generated";
import { appearanceImages } from "../contract/client";
import { applyTypeSizes, setInterfaceColors, setSkin, setThemeMode, typeSizes, type TypeSizes } from "../design/theme";
import { cssColor, sameRgba, THEME_DEFAULT, themeSkin } from "./model";

export interface LoadedImages {
  board?: HTMLImageElement;
  background?: HTMLImageElement;
  black?: HTMLImageElement;
  white?: HTMLImageElement;
  /** The stones' other pictures (ThemeImages.blackVariants / whiteVariants) that decoded. */
  blackVariants?: HTMLImageElement[];
  whiteVariants?: HTMLImageElement[];
  /** Theme.defaultBoard: the theme's own board without custom-board-image (SubBoardRenderer.java:1338-1341). */
  themeBoard?: HTMLImageElement;
}

export interface ActiveAppearance {
  settings: AppearanceSettings;
  theme: ThemeSettings;
  images: LoadedImages;
  /** Theme.isUsingCustomBoardImage: a readable custom-board-image is what the board shows. */
  customBoard: boolean;
}

/** The bundled default images (Theme.java:146-204 fallbacks). */
const DEFAULT_URLS: ThemeImages = { board: "/assets/board.png", background: "/assets/background.jpg", black: "/assets/black0.png", white: "/assets/white0.png", customBoard: false, customBackground: false, blackVariants: [], whiteVariants: [] };

let current: ActiveAppearance | null = null;
let revision = 0;
let requested = "";
let generation = 0;
const listeners: (() => void)[] = [];

/** The committed appearance; null only before the first view arrived. */
export function appearance(): ActiveAppearance | null {
  return current;
}

/** Moves on with every applied appearance: painters include it in their repaint signatures. */
export function appearanceRevision(): number {
  return revision;
}

export function onAppearanceChange(listener: () => void): void {
  listeners.push(listener);
}

function loadImage(url: string): Promise<HTMLImageElement | undefined> {
  const img = new Image();
  img.src = url;
  return img.decode().then(() => img, () => undefined);
}

/** Decodes the images `settings` and `theme` select for the committed appearance; the core's URLs
 *  or, when it cannot answer, the bundled defaults. */
async function loadThemeImages(settings: AppearanceSettings, theme: ThemeSettings): Promise<{ images: LoadedImages; customBoard: boolean }> {
  const urls = await appearanceImages(settings, theme).catch(() => DEFAULT_URLS);
  const themeUrls = settings.customBoardImage ? await appearanceImages({ ...settings, customBoardImage: "" }, theme).catch(() => urls) : urls;
  const [board, background, black, white, themeBoard] = await Promise.all([urls.board, urls.background, urls.black, urls.white, themeUrls.board].map(loadImage));
  const [blackVariants, whiteVariants] = await Promise.all([urls.blackVariants, urls.whiteVariants].map(loadVariants));
  return { images: { board, background, black, white, blackVariants, whiteVariants, themeBoard }, customBoard: !!board && urls.customBoard };
}

/** The pictures among `urls` that decode, in order. */
export async function loadVariants(urls: readonly string[]): Promise<HTMLImageElement[]> {
  return (await Promise.all(urls.map(loadImage))).filter((img) => img !== undefined);
}

/** The colour mode, the skin (the Java themes take Java's colours) and the theme's 界面颜色, and
 *  what the theme chooses for the comment pane: its colours and text size (LizzieFrame.java:11292-11304,
 *  11358-11359), the 评论 / 问题手 underline, the theme's accent colour at alpha 220
 *  (SidebarHeaderPanel.java:226-238), and the controls' corners: glass-corner-radius is the arc,
 *  at least 8, Java rounds its buttons, fields, lists and popup menus with
 *  (AppleStyleSupport.controlCornerRadius, RoundedLineBorder), a corner radius of half the arc. A
 *  value still at the theme default is left to the style sheet, which draws it for the current
 *  colour mode and skin. */
function applyStyles(settings: AppearanceSettings, theme: ThemeSettings): void {
  setThemeMode(settings.mode);
  setSkin(themeSkin(settings.theme));
  setInterfaceColors({ chrome: theme.uiChromeColor, control: theme.uiControlColor, accent: theme.uiAccentColor, text: theme.uiTextColor });
  const root = document.documentElement.style;
  const own = (name: string, value: readonly number[], themeDefault: readonly number[], css = cssColor(value)) => {
    if (sameRgba(value, themeDefault)) root.removeProperty(name);
    else root.setProperty(name, css);
  };
  own("--comment-background", theme.commentBackgroundColor, THEME_DEFAULT.commentBackground);
  own("--comment-color", theme.commentFontColor, THEME_DEFAULT.commentFont);
  const [r, g, b] = theme.glassAccentColor;
  own("--theme-accent-underline", [r, g, b], THEME_DEFAULT.glassAccent, `rgba(${r}, ${g}, ${b}, ${220 / 255})`);
  if (theme.commentFontSize > 0) root.setProperty("--comment-font-size", `${theme.commentFontSize}px`);
  else root.removeProperty("--comment-font-size");
  const radius = Math.max(8, theme.glassCornerRadius) / 2;
  const radii: Record<string, number> = { "--ds-radius": radius, "--ds-radius-cap": radius, "--ds-radius-in": Math.max(2, radius - 2), "--ds-radius-menu": radius + 4 };
  for (const [name, value] of Object.entries(radii)) {
    if (theme.glassCornerRadius === THEME_DEFAULT.glassCornerRadius) root.removeProperty(name);
    else root.setProperty(name, `${value}px`);
  }
}

/** Applies a published appearance (the workbench's view, or the one a floating board window is
 *  sent). Unchanged values do nothing; a newer change wins over a slower earlier one, so images of
 *  a superseded change never reach the painters. */
export async function syncAppearance(settings: AppearanceSettings, theme: ThemeSettings, filesRevision: number): Promise<void> {
  // The revision changes when theme folders were rewritten in place under the same values.
  const key = JSON.stringify([settings, theme, filesRevision]);
  if (key === requested) return;
  requested = key;
  const mine = ++generation;
  const loaded = await loadThemeImages(settings, theme);
  if (mine !== generation) return;
  current = { settings, theme, ...loaded };
  revision++;
  applyStyles(settings, theme);
  for (const listener of listeners) listener();
}

/** This session's Config.frameFontSize and what Config.java:1871-1873 derives from it once:
 *  menuHeight 20 / 25 / 30 and menuIconSize 16 / 20 / 24 for 12, 13-16 and larger sizes
 *  (isFrameFontSmall / isFrameFontMiddle, Config.java:3638-3646); `type` is the interface's type
 *  steps at that size (design/theme.ts typeSizes). */
let metrics = { frameFontSize: 12, menuHeight: 20, menuIconSize: 16, type: typeSizes(12) };

export function frameMetrics(): { frameFontSize: number; menuHeight: number; menuIconSize: number; type: TypeSizes } {
  return metrics;
}

/** Start-up: the first appearance is in place before the first paint, and the frame font size of
 *  this session is fixed (Config.java:1871 reads frame-font-size once; a change applies after a
 *  restart, LizzieFrame.java:20764-20767). */
export async function startAppearance(first: WorkbenchView): Promise<void> {
  const size = first.settings.active_frame_font_size;
  const small = size === 12;
  const middle = size > 12 && size <= 16;
  metrics = { frameFontSize: size, menuHeight: small ? 20 : middle ? 25 : 30, menuIconSize: small ? 16 : middle ? 20 : 24, type: typeSizes(size) };
  applyTypeSizes(size);
  document.documentElement.style.setProperty("--menu-icon-size", `${metrics.menuIconSize}px`);
  await syncAppearance(first.settings.appearance, first.settings.theme, first.settings.theme_files_revision);
}
