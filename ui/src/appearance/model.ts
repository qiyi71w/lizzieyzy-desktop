// Presentation rules of the Java theme values (wimi321/lizzieyzy-next cb53acc): colour arrays,
// derived colours, font roles, the 中式经典 board style and the 错误节点 rules. Pure functions only;
// the active committed values live in ./state.
import type { Language } from "../contract/generated";
import type { Skin } from "../design/theme";

export const DEFAULT_THEME = "default";
/** The bundled theme with Java's own default values; while it is the committed theme the interface
 *  takes Java's colours and wallpaper (design/theme.ts skin "java"). */
export const JAVA_CLASSIC_THEME = "java-classic";
/** The bundled theme with Java's default values under Java's Morandi palette (ui.use-morandi-colors);
 *  while it is the committed theme the interface takes those colours (skin "soft"). */
export const JAVA_SOFT_THEME = "java-soft";

/** The skin a theme selects: the bundled Java themes theirs, every other theme the program's. */
export function themeSkin(theme: string): Skin {
  return theme === JAVA_CLASSIC_THEME ? "java" : theme === JAVA_SOFT_THEME ? "soft" : "modern";
}

/** ConfigDialog2.getFontItemName (ConfigDialog2.java:6374-6380): combo index 0 and 1. */
export const LIZZIE_DEFAULT_FONT = "Lizzie Default";
export const SYSTEM_DEFAULT_FONT = "Dialog.plain";

/** Theme.DEFAULT_BLUNDER_THRESHOLDS / DEFAULT_BLUNDER_COLORS (Theme.java:36-45): 重置 of the
 *  错误节点 rules (Config.resetBlunderColor, Config.java:1491-1500). */
export const DEFAULT_BLUNDER_THRESHOLDS = [-24, -12, -6, -3, -1, 3, 100];
export const DEFAULT_BLUNDER_COLORS = [[155, 25, 150], [208, 16, 19], [200, 140, 50], [180, 180, 0], [140, 202, 34], [0, 220, 0], [0, 210, 210]];

const i = Math.trunc;

/** CSS colour of a theme colour array; a fourth entry is the Java alpha (0-255). */
export function cssColor(c: readonly number[]): string {
  if (c.length > 3 && c[3] !== 255) return `rgba(${c[0]},${c[1]},${c[2]},${+(c[3] / 255).toFixed(3)})`;
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

/** Utils.getNoneAlphaColor: pure board / background colours drop the alpha (Theme.java:104-110). */
export function opaque(c: readonly number[]): number[] {
  return [c[0], c[1], c[2]];
}

export function sameColor(a: readonly number[], b: readonly number[]): boolean {
  return a.length === b.length && a.every((v, k) => v === b[k]);
}

/** java.awt.Color.RGBtoHSB(...)[0]: Config.bestMoveColor (Config.java:2259-2261). */
export function javaHue(c: readonly number[]): number {
  const [r, g, b] = c;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  if (max === 0 || max === min) return 0;
  const range = max - min;
  const rc = Math.fround((max - r) / range);
  const gc = Math.fround((max - g) / range);
  const bc = Math.fround((max - b) / range);
  let hue = r === max ? bc - gc : g === max ? 2 + rc - bc : 4 + gc - rc;
  hue = Math.fround(hue / 6);
  return hue < 0 ? hue + 1 : hue;
}

/** Config.reverseColor (Config.java:3522-3532): Config.bestColor, the best candidate's max text. */
export function reverseColor(c: readonly number[]): number[] {
  return [255 - c[0], 255 - c[1], 255 - c[2]];
}

/** A configured font family ahead of the role's default stack. "Lizzie Default" (or empty) keeps the
 *  role's default (LocaleFontSupport.isDefaultSelection, Utils.loadFonts 105-114); "Dialog.plain" is
 *  the 系统默认 item, the platform Dialog font. The browser's per-glyph fallback through the stack
 *  takes the place of LocaleFontSupport's candidate probing (LocaleFontSupport.java:79-120). */
export function themedFont(name: string, roleDefault: string, dialog: string): string {
  const value = name.trim();
  if (!value || value === LIZZIE_DEFAULT_FONT || value === "Lizzie默认") return roleDefault;
  if (value === SYSTEM_DEFAULT_FONT || value === "Dialog") return dialog;
  return `"${value.replace(/["\\]/g, "")}", ${dialog}`;
}

/** Utils.loadFonts: a configured winrate font is created BOLD (Utils.java:111-114). */
export function customFont(name: string): boolean {
  const value = name.trim();
  return !!value && value !== LIZZIE_DEFAULT_FONT && value !== "Lizzie默认";
}

/** LocaleFontSupport candidate lists (LocaleFontSupport.java:21-64) per UI language, as a stack
 *  tail: the browser's per-glyph fallback through it does what canDisplayUpTo probing selects. */
const LOCALE_FONTS: Partial<Record<Language, string[]>> = {
  ZhCn: ["Microsoft YaHei UI", "Microsoft YaHei", "PingFang SC", "Noto Sans CJK SC", "Source Han Sans SC", "SimHei", "Arial Unicode MS"],
  ZhTw: ["Microsoft JhengHei UI", "Microsoft JhengHei", "PingFang TC", "Noto Sans CJK TC", "Source Han Sans TC", "MingLiU", "Arial Unicode MS"],
  JaJp: ["Yu Gothic UI", "Yu Gothic", "Meiryo UI", "Meiryo", "Hiragino Sans", "Noto Sans CJK JP", "Arial Unicode MS"],
  KoKr: ["Malgun Gothic", "Apple SD Gothic Neo", "Noto Sans CJK KR", "NanumGothic", "Arial Unicode MS"],
  ThTh: ["Leelawadee UI", "Tahoma", "Noto Sans Thai", "Noto Sans Thai Looped", "Thonburi", "Arial Unicode MS", "Noto Sans", "DejaVu Sans"],
};
export function localeFonts(language: Language): string {
  return (LOCALE_FONTS[language] ?? []).map((f) => `, "${f}"`).join("");
}

// ------------------------------------------------------------------ 中式经典 (BoardStylePainter.java)

const CHINESE_NUMERALS = ["一", "二", "三", "四", "五", "六", "七", "八", "九", "十", "十一", "十二", "十三", "十四", "十五", "十六", "十七", "十八", "十九"];

/** chineseClassicCoordinateLabel: the line number counted from the nearer edge. */
export function chineseClassicLabel(index: number, size: number): string {
  if (index < 0 || index >= size || size <= 0) return "";
  const line = Math.min(index + 1, size - index);
  return line >= 1 && line <= CHINESE_NUMERALS.length ? CHINESE_NUMERALS[line - 1] : String(line);
}

/** chineseClassicMarkerPoints: four corner points and the centre of square boards of 5 or more. */
export function chineseClassicMarkers(cols: number, rows: number): [number, number][] {
  if (cols !== rows || cols < 5) return [];
  const edge = cols >= 13 ? 3 : cols >= 7 ? 2 : 1;
  const opposite = cols - 1 - edge;
  const center = i(cols / 2);
  if (edge <= 0 || edge >= center || opposite <= center) return [];
  return [[edge, edge], [opposite, edge], [center, center], [edge, opposite], [opposite, opposite]];
}

/** drawChineseClassicMarkers + drawCornerMarker: open corner brackets around each marker point. */
export function drawChineseClassicMarkers(g: CanvasRenderingContext2D, cols: number, rows: number, square: number, X: (x: number) => number, Y: (y: number) => number): void {
  const points = chineseClassicMarkers(cols, rows);
  if (!points.length) return;
  const grid = Math.max(1, square);
  const gap = Math.max(2, grid * 0.11);
  const arm = Math.max(gap + 2, grid * 0.3);
  g.save();
  g.strokeStyle = "rgb(20,16,10)";
  g.lineWidth = Math.max(1.1, Math.min(3, grid * 0.045));
  g.lineCap = "square";
  g.lineJoin = "miter";
  g.beginPath();
  for (const [px, py] of points) {
    const cx = X(px), cy = Y(py);
    const [l, r, t, b] = [cx - arm, cx + arm, cy - arm, cy + arm];
    const [il, ir, it, ib] = [cx - gap, cx + gap, cy - gap, cy + gap];
    const seg = (x1: number, y1: number, x2: number, y2: number) => {
      g.moveTo(x1, y1);
      g.lineTo(x2, y2);
    };
    seg(l, it, il, it);
    seg(il, t, il, it);
    seg(ir, it, r, it);
    seg(ir, t, ir, it);
    seg(l, ib, il, ib);
    seg(il, ib, il, b);
    seg(ir, ib, r, ib);
    seg(ir, ib, ir, b);
  }
  g.stroke();
  g.restore();
}

// ------------------------------------------------------------------ panel colours

const blend = (a: number, b: number, t: number) => Math.round(a + (b - a) * t);

/** WinrateGraph.resolveGraphBackgroundColor (WinrateGraph.java:312-323) with
 *  Config.commentBackgroundColor; WinrateGraph.blend rounds the linear mix. */
export function graphBackground(panel: readonly number[]): number[] {
  const lift = [58, 66, 76];
  const alpha = Math.max(118, Math.min(178, (panel[3] ?? 255) - 35));
  return [blend(panel[0], lift[0], 0.38), blend(panel[1], lift[1], 0.38), blend(panel[2], lift[2], 0.38), alpha];
}

const linear = (v: number) => {
  const c = v / 255;
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
};

/** WinrateGraph.resolveBelowBaselineFillColor (WinrateGraph.java:396-399). */
export function belowBaselineFill(background: readonly number[]): number[] {
  const luminance = 0.2126 * linear(background[0]) + 0.7152 * linear(background[1]) + 0.0722 * linear(background[2]);
  return luminance < 0.5 ? [232, 225, 210, 42] : [50, 54, 60, 42];
}

// ------------------------------------------------------------------ theme defaults

/** The default theme's values of the colours a user can change (ThemeSettings::default in
 *  crates/lizzie-core/src/appearance.rs). While a colour holds its default the interface draws
 *  with its own token for the current colour mode. */
export const THEME_DEFAULT = {
  winrateLine: [100, 180, 255],
  winrateMiss: [0, 0, 178],
  scoreLine: [255, 0, 255],
  blunderBar: [255, 204, 255, 170],
  commentBackground: [0, 0, 0, 200],
  commentFont: [255, 255, 255],
  commentNode: [0, 0, 255],
  glassAccent: [96, 165, 250],
  /** glass-corner-radius: the arc of Java's controls. */
  glassCornerRadius: 12,
  /** The 错误节点 colours, worst to best. */
  blunderNodes: [[155, 25, 150], [208, 16, 19], [200, 140, 50], [180, 180, 0], [140, 202, 34], [0, 220, 0], [0, 210, 210]],
} as const;

/** Equal colours, a missing alpha being opaque. */
export function sameRgba(a: readonly number[], b: readonly number[]): boolean {
  return [0, 1, 2, 3].every((k) => (a[k] ?? 255) === (b[k] ?? 255));
}

/** A theme colour the user can change: `tokenColor` while it holds the theme default
 *  (`themeDefault`), else the user's colour as it is. */
export function themeColor(value: readonly number[] | undefined, themeDefault: readonly number[], tokenColor: string): string {
  if (!value || sameRgba(value, themeDefault)) return tokenColor;
  return cssColor(value);
}
