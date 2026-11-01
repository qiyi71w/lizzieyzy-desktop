// The committed appearance every painter draws with: the core's appearance and theme values plus
// their decoded images. A change swaps images only after all of them decoded, and only if no newer
// change arrived meanwhile (Menu.refreshCustomBackgrounds / Config.readThemeVaule clear every board
// and wallpaper cache, Menu.java:7075-7122, Config.java:2287-2323). The frame font size is the
// value this session started with (LizzieFrame.java:20764-20767: applied after a restart).
import type { AppearanceSettings, ThemeSettings, WorkbenchView } from "../contract/generated";
import { appearanceImages } from "../contract/client";
import { cssColor, type ThemeImages } from "./model";

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

/** Comment panel colours and size (LizzieFrame.java:11292-11304, 11358-11359; SidebarPanel.java:113-121)
 *  and the chrome palette: 苹果风格, else 莫兰迪, else classic (AppleStyleSupport.isAppleStyleEnabled /
 *  isClassicEnabled, AppleStyleSupport.java:66-72). */
function applyStyles(settings: AppearanceSettings, theme: ThemeSettings): void {
  const root = document.documentElement.style;
  root.setProperty("--comment-background", cssColor(theme.commentBackgroundColor));
  root.setProperty("--comment-color", cssColor(theme.commentFontColor));
  if (theme.commentFontSize > 0) root.setProperty("--comment-font-size", `${theme.commentFontSize}px`);
  else root.removeProperty("--comment-font-size");
  const chrome = settings.appleStyle ? "apple" : settings.morandiColors ? "morandi" : null;
  if (chrome) document.documentElement.dataset.chrome = chrome;
  else delete document.documentElement.dataset.chrome;
  // The glass values the chrome draws with: controlBorderColor, accentFillColor /
  // accentBorderColor and controlCornerRadius (AppleStyleSupport.java:688-717).
  const [r, g, b] = theme.glassAccentColor;
  root.setProperty("--glass-accent", `${r}, ${g}, ${b}`);
  root.setProperty("--glass-border", cssColor(theme.glassPanelBorderColor));
  root.setProperty("--control-radius", `${Math.max(8, theme.glassCornerRadius) / 2}px`);
  if (r === 96 && g === 165 && b === 250) root.removeProperty("--glass-accent-underline");
  else root.setProperty("--glass-accent-underline", `rgba(${r}, ${g}, ${b}, ${220 / 255})`);
}

/** Applies a published view's appearance. Unchanged values do nothing; a newer change wins over a
 *  slower earlier one, so images of a superseded change never reach the painters. */
export async function syncAppearance(view: WorkbenchView): Promise<void> {
  const { appearance: settings, theme } = view.settings;
  const key = JSON.stringify([settings, theme]);
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
  if (first.settings.appearance.appleStyle) installAppleIcons();
  await syncAppearance(first);
}

/** brightenIcon's recolouring as an SVG filter: every visible pixel becomes #E0E4EA. */
function installAppleIcons(): void {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("width", "0");
  svg.setAttribute("height", "0");
  svg.style.position = "absolute";
  svg.innerHTML = '<filter id="apple-icon" color-interpolation-filters="sRGB"><feFlood flood-color="#E0E4EA"/><feComposite in2="SourceAlpha" operator="in"/></filter>';
  document.body.append(svg);
  document.documentElement.dataset.appleIcons = "";
}
