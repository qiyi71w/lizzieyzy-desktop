// The workbench stage: the wallpaper under the board and the palette the canvas panels draw with.
// Without a theme wallpaper, a custom background image or a pure background colour the stage is
// the program's own: a linen cloth in the colour mode's ground colour under translucent panels of
// that mode. A wallpaper or colour the user chose stays as it is, under Java's translucent black
// panels drawn with the dark theme's colours.
import { appearance, onAppearanceChange } from "../appearance/state";
import { onThemeChange, resolvedTheme } from "./theme";
import "./stage.css";

/** The bundled wallpaper the core resolves to without a theme or custom background image. */
const DEFAULT_WALLPAPER = "/assets/background.jpg";

/** The program's own wallpapers, 512 px tiles made by tools/stage_wallpapers.py. */
const WALLPAPERS = { light: new Image(), dark: new Image() };
WALLPAPERS.light.src = "/assets/stage-light.jpg";
WALLPAPERS.dark.src = "/assets/stage-dark.jpg";

/** Settles once both wallpapers can be painted (or cannot be read). */
export const stageReady: Promise<unknown> = Promise.allSettled([WALLPAPERS.light.decode(), WALLPAPERS.dark.decode()]);

/** True while the committed appearance names no wallpaper of its own: the stage is then the
 *  program's wallpaper for the current colour mode and the panels take that mode's tokens. */
export function builtinStage(): boolean {
  const a = appearance();
  if (!a) return true;
  if (a.theme.usePureBackground) return false;
  const src = a.images.background?.src;
  return !!src && new URL(src, location.href).pathname === DEFAULT_WALLPAPER;
}

/** The program's wallpaper for the current colour mode. */
export function stageWallpaper(): HTMLImageElement {
  return WALLPAPERS[resolvedTheme()];
}

/** The colours and the interface face the canvas panels draw with. */
export interface StagePalette {
  builtin: boolean;
  /** A card's translucent ground, the hairline between its sections, the card as it reads over
   *  the wallpaper (grounds that hide what runs under them), its edge and its shadow's colour. */
  panel: string;
  line: string;
  panelSolid: string;
  cardEdge: string;
  cardShadow: string;
  lineStrong: string;
  text: string;
  text2: string;
  text3: string;
  accent: string;
  /** The design system's interface font stack (`--ds-font`). */
  font: string;
  winrateLine: string;
  winrateMiss: string;
  scoreLine: string;
  blunderBar: string;
  commentNode: string;
  graphGrid: string;
  graphBaseline: string;
  graphGuide: string;
  graphHover: string;
  graphScoreText: string;
  graphSnapshot: string;
  graphScoreMarker: string;
  graphFillBelow: string;
  stoneBlack: string;
  stoneBlackEdge: string;
  stoneWhite: string;
  stoneWhiteEdge: string;
  onBlack: string;
  onWhite: string;
  barGrid: string;
  gain: string;
  loss: string;
  live: string;
  idle: string;
  treeNode: string;
  treeEmpty: string;
  treeLine: string;
  treeLineOff: string;
  treeDim: string;
  /** The seven 错误节点 levels, worst to best. */
  treeNodes: string[];
  scrollThumb: string;
}

const NAMES: Record<Exclude<keyof StagePalette, "builtin" | "treeNodes">, string> = {
  panel: "--ws-panel",
  line: "--ws-panel-line",
  panelSolid: "--ws-panel-solid",
  cardEdge: "--ws-card-edge",
  cardShadow: "--ws-card-shadow",
  lineStrong: "--ds-line-strong",
  text: "--ds-text",
  text2: "--ds-text-2",
  text3: "--ds-text-3",
  accent: "--ds-accent",
  font: "--ds-font",
  winrateLine: "--ws-winrate-line",
  winrateMiss: "--ws-winrate-miss",
  scoreLine: "--ws-score-line",
  blunderBar: "--ws-blunder-bar",
  commentNode: "--ws-comment-node",
  graphGrid: "--ws-graph-grid",
  graphBaseline: "--ws-graph-baseline",
  graphGuide: "--ws-graph-guide",
  graphHover: "--ws-graph-hover",
  graphScoreText: "--ws-graph-score-text",
  graphSnapshot: "--ws-graph-snapshot",
  graphScoreMarker: "--ws-graph-score-marker",
  graphFillBelow: "--ws-graph-fill-below",
  stoneBlack: "--ws-stone-black",
  stoneBlackEdge: "--ws-stone-black-edge",
  stoneWhite: "--ws-stone-white",
  stoneWhiteEdge: "--ws-stone-white-edge",
  onBlack: "--ws-on-black",
  onWhite: "--ws-on-white",
  barGrid: "--ws-bar-grid",
  gain: "--ws-gain",
  loss: "--ws-loss",
  live: "--ws-live",
  idle: "--ws-idle",
  treeNode: "--ws-tree-node",
  treeEmpty: "--ws-tree-empty",
  treeLine: "--ws-tree-line",
  treeLineOff: "--ws-tree-line-off",
  treeDim: "--ws-tree-dim",
  scrollThumb: "--ws-scroll-thumb",
};

/** The cards over a wallpaper the user chose: translucent black under light text, and the
 *  near-opaque black that hides what runs under a ground there (stage.css). */
const WALLPAPER_PANEL = "rgba(0,0,0,0.51)";
const WALLPAPER_SOLID = "rgba(0,0,0,0.78)";

let cached: StagePalette | null = null;
let darkProbe: HTMLElement | null = null;
let revision = 0;
const listeners: (() => void)[] = [];

/** The palette for the current stage and colour mode. */
export function stagePalette(): StagePalette {
  if (cached) return cached;
  const builtin = builtinStage();
  let source: Element = document.documentElement;
  if (!builtin) {
    // The dark theme's values whatever the mode: a hidden element fixed to the dark theme.
    if (!darkProbe?.isConnected) {
      darkProbe = document.createElement("div");
      darkProbe.dataset.theme = "dark";
      darkProbe.hidden = true;
      document.documentElement.append(darkProbe);
    }
    source = darkProbe;
  }
  const style = getComputedStyle(source);
  const palette = { builtin } as StagePalette;
  for (const [key, name] of Object.entries(NAMES)) (palette as unknown as Record<string, string>)[key] = style.getPropertyValue(name).trim();
  palette.treeNodes = [1, 2, 3, 4, 5, 6, 7].map((k) => style.getPropertyValue(`--ws-node-${k}`).trim());
  if (!builtin) {
    palette.panel = WALLPAPER_PANEL;
    palette.panelSolid = WALLPAPER_SOLID;
  }
  cached = palette;
  return palette;
}

/** Moves on whenever the palette or the stage changes: painters include it in their repaint
 *  signatures, as they do the appearance revision. */
export function stageRevision(): number {
  return revision;
}

/** Called after the palette changed (colour mode, or the stage with the appearance). */
export function onStageChange(listener: () => void): void {
  listeners.push(listener);
}

/** The stage as the style sheets see it. Over a wallpaper the user chose, the workbench's panes
 *  take the dark theme's tokens in either colour mode, like the canvas panels beside them. */
function applyStage(): void {
  const builtin = builtinStage();
  document.documentElement.dataset.stage = builtin ? "builtin" : "wallpaper";
  const main = document.getElementById("main");
  if (builtin) main?.removeAttribute("data-theme");
  else main?.setAttribute("data-theme", "dark");
}

function changed(): void {
  cached = null;
  revision++;
  applyStage();
  for (const listener of listeners) listener();
}

const colourProbe = document.createElement("canvas").getContext("2d")!;

/** [r, g, b, a(0-255)] of a CSS colour in any notation: the canvas reports what it parsed as
 *  `#rrggbb` or `rgba(r, g, b, a)`. The build shortens the tokens' own notation (`#fff`). */
export function rgbaOf(css: string): number[] {
  colourProbe.fillStyle = "#000";
  colourProbe.fillStyle = css;
  const parsed = colourProbe.fillStyle;
  if (parsed.startsWith("#")) {
    const n = Number.parseInt(parsed.slice(1), 16);
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255, 255];
  }
  const [r = 0, g = 0, b = 0, a = 1] = parsed.slice(parsed.indexOf("(") + 1, -1).split(",").map(Number);
  return [r, g, b, Math.round(a * 255)];
}

onAppearanceChange(changed);
onThemeChange(changed);
applyStage();
