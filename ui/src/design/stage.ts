// The workbench stage: the ground under the board and the palette the canvas panels draw with.
// The bundled default wallpaper gives way to the flat stage of the current colour mode; a theme's
// own wallpaper, a custom background image or a pure background colour keeps the wallpaper and
// its translucent dark panels, drawn with the dark theme's colours.
import { appearance, onAppearanceChange } from "../appearance/state";
import { onThemeChange } from "./theme";
import "./stage.css";

/** The bundled wallpaper the core resolves to without a theme or custom background image. */
const DEFAULT_WALLPAPER = "/assets/background.jpg";

/** True while the committed appearance shows the bundled default wallpaper: the workbench ground
 *  is then the flat `--ds-stage` and the panels take the current mode's tokens. */
export function flatStage(): boolean {
  const a = appearance();
  if (!a) return true;
  if (a.theme.usePureBackground) return false;
  const src = a.images.background?.src;
  return !!src && new URL(src, location.href).pathname === DEFAULT_WALLPAPER;
}

/** The colours and the interface face the canvas panels draw with. */
export interface StagePalette {
  flat: boolean;
  /** Panel ground and edge (the edge 1px, square). */
  panel: string;
  line: string;
  lineStrong: string;
  sunken: string;
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
  stoneBlack: string;
  stoneBlackEdge: string;
  stoneWhite: string;
  onBlack: string;
  onWhite: string;
  barGrid: string;
  gain: string;
  loss: string;
  live: string;
  idle: string;
  treeNode: string;
  treeLine: string;
  treeLineOff: string;
  treeDim: string;
  scrollThumb: string;
}

const NAMES: Record<Exclude<keyof StagePalette, "flat">, string> = {
  panel: "--ds-panel",
  line: "--ds-line",
  lineStrong: "--ds-line-strong",
  sunken: "--ds-sunken",
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
  stoneBlack: "--ws-stone-black",
  stoneBlackEdge: "--ws-stone-black-edge",
  stoneWhite: "--ws-stone-white",
  onBlack: "--ws-on-black",
  onWhite: "--ws-on-white",
  barGrid: "--ws-bar-grid",
  gain: "--ws-gain",
  loss: "--ws-loss",
  live: "--ws-live",
  idle: "--ws-idle",
  treeNode: "--ws-tree-node",
  treeLine: "--ws-tree-line",
  treeLineOff: "--ws-tree-line-off",
  treeDim: "--ws-tree-dim",
  scrollThumb: "--ws-scroll-thumb",
};

/** The wallpaper's panels: translucent black under light text, as the wallpaper always had. */
const WALLPAPER_PANEL = "rgba(0,0,0,0.51)";

let cached: StagePalette | null = null;
let darkProbe: HTMLElement | null = null;
let revision = 0;
const listeners: (() => void)[] = [];

/** The palette for the current stage and colour mode. */
export function stagePalette(): StagePalette {
  if (cached) return cached;
  const flat = flatStage();
  let source: Element = document.documentElement;
  if (!flat) {
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
  const palette = { flat } as StagePalette;
  for (const [key, name] of Object.entries(NAMES)) (palette as unknown as Record<string, string>)[key] = style.getPropertyValue(name).trim();
  if (!flat) {
    palette.panel = WALLPAPER_PANEL;
    palette.line = WALLPAPER_PANEL;
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

/** The stage as the style sheets see it. Over a wallpaper the workbench's panes take the dark
 *  theme's tokens in either colour mode, like the canvas panels beside them. */
function applyStage(): void {
  const flat = flatStage();
  document.documentElement.dataset.stage = flat ? "flat" : "wallpaper";
  const main = document.getElementById("main");
  if (flat) main?.removeAttribute("data-theme");
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

onThemeChange(changed);
onAppearanceChange(changed);
applyStage();
