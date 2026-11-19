// The committed appearance every painter draws with: the core's appearance and theme values plus
// their decoded images. A change swaps images only after all of them decoded, and only if no newer
// change arrived meanwhile (Menu.refreshCustomBackgrounds / Config.readThemeVaule clear every board
// and wallpaper cache, Menu.java:7075-7122, Config.java:2287-2323). The frame font size is the
// value this session started with (LizzieFrame.java:20764-20767: applied after a restart).
import type { AppearanceSettings, ThemeImages, ThemeSettings, WorkbenchView } from "../contract/generated";
import { appearanceImages } from "../contract/client";
import { setThemeMode } from "../design/theme";
import { cssColor, sameRgba, THEME_DEFAULT } from "./model";

export interface LoadedImages {
  board?: HTMLImageElement;
  background?: HTMLImageElement;
  black?: HTMLImageElement;
  white?: HTMLImageElement;
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
const DEFAULT_URLS: ThemeImages = { board: "/assets/board.png", background: "/assets/background.jpg", black: "/assets/black0.png", white: "/assets/white0.png", customBoard: false, customBackground: false };

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

/** Decodes the images `settings` and `theme` select; the core's URLs or, when it cannot answer,
 *  the bundled defaults. Used for the committed appearance and for the 棋盘预览 draft. */
export async function loadThemeImages(settings: AppearanceSettings, theme: ThemeSettings): Promise<{ images: LoadedImages; customBoard: boolean }> {
  const urls = await appearanceImages(settings, theme).catch(() => DEFAULT_URLS);
  const themeUrls = settings.customBoardImage ? await appearanceImages({ ...settings, customBoardImage: "" }, theme).catch(() => urls) : urls;
  const [board, background, black, white, themeBoard] = await Promise.all([urls.board, urls.background, urls.black, urls.white, themeUrls.board].map(loadImage));
  return { images: { board, background, black, white, themeBoard }, customBoard: !!board && urls.customBoard };
}

/** The colour mode, and what the theme chooses for the comment pane: its colours and text size
 *  (LizzieFrame.java:11292-11304, 11358-11359) and the 评论 / 问题手 underline, the theme's accent
 *  colour at alpha 220 (SidebarHeaderPanel.java:226-238). A value still at the theme default is
 *  left to the style sheet, which draws it for the current colour mode. */
function applyStyles(settings: AppearanceSettings, theme: ThemeSettings): void {
  setThemeMode(settings.mode);
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
 *  (isFrameFontSmall / isFrameFontMiddle, Config.java:3638-3646). */
let metrics = { frameFontSize: 12, menuHeight: 20, menuIconSize: 16 };

export function frameMetrics(): { frameFontSize: number; menuHeight: number; menuIconSize: number } {
  return metrics;
}

/** Start-up: the first appearance is in place before the first paint, and the frame font size of
 *  this session is fixed (Config.java:1871 reads frame-font-size once; a change applies after a
 *  restart, LizzieFrame.java:20764-20767). */
export async function startAppearance(first: WorkbenchView): Promise<void> {
  const size = first.settings.active_frame_font_size;
  const small = size === 12;
  const middle = size > 12 && size <= 16;
  metrics = { frameFontSize: size, menuHeight: small ? 20 : middle ? 25 : 30, menuIconSize: small ? 16 : middle ? 20 : 24 };
  const root = document.documentElement.style;
  root.setProperty("--frame-font-size", `${size}px`);
  root.setProperty("--menu-icon-size", `${metrics.menuIconSize}px`);
  await syncAppearance(first.settings.appearance, first.settings.theme, first.settings.theme_files_revision);
}
