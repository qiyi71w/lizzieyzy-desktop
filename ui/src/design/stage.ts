// The workbench stage: the wallpaper under the board and the palette the canvas panels draw with.
// Without a theme wallpaper, a custom background image or a pure background colour the stage is
// the program's own: a linen cloth in the colour mode's ground colour under translucent panels of
// that mode. A wallpaper or colour the user chose stays as it is, under Java's translucent black
// panels drawn with the dark theme's colours. The Java skins (Java 经典 and Java 柔和) are Java's own
// stage: its grey metal wallpaper (the theme's background, Java's default image) under dark glass, in
// Java's colours.
import { appearance, onAppearanceChange } from "../appearance/state";
import { onThemeChange, resolvedTheme, skin } from "./theme";
import "./stage.css";

/** The bundled wallpaper the core resolves to without a theme or custom background image. */
const DEFAULT_WALLPAPER = "/assets/background.jpg";

/** The program's own wallpapers, 512 px tiles made by tools/stage_wallpapers.py. */
const WALLPAPERS = { light: new Image(), dark: new Image() };
WALLPAPERS.light.src = "/assets/stage-light.jpg";
WALLPAPERS.dark.src = "/assets/stage-dark.jpg";

/** Settles once both wallpapers can be painted (or cannot be read). */
export const stageReady: Promise<unknown> = Promise.allSettled([WALLPAPERS.light.decode(), WALLPAPERS.dark.decode()]);

/** True when a background choice leaves the stage to the program: the program's skin, no pure
 *  colour, and the bundled wallpaper the core resolves to without a theme or custom image (`src`,
 *  that background's URL). */
export function programStage(javaSkin: boolean, pureBackground: boolean, src: string | undefined): boolean {
  return !javaSkin && !pureBackground && !!src && new URL(src, location.href).pathname === DEFAULT_WALLPAPER;
}

/** True while the committed appearance leaves the stage to the program: the stage is then the
 *  program's wallpaper for the current colour mode and the panels take that mode's tokens. */
export function builtinStage(): boolean {
  const a = appearance();
  if (!a) return skin() === "modern";
  return programStage(skin() !== "modern", a.theme.usePureBackground, a.images.background?.src);
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
  /** The white engine's win-rate curve in the engine-game graph while the theme's miss colour
   *  holds its default (Java draws it in the miss colour, which here is a quiet bridge grey). */
  engineWhiteLine: string;
  scoreLine: string;
  blunderBar: string;
  commentNode: string;
  /** 胜率图 (stage.css): grid, midline, guides, Label text, readout figures and chip ground,
   *  snapshot marks, the two current markers with their ring and the area fill on either side of
   *  the midline. */
  graphGrid: string;
  graphBaseline: string;
  graphGuide: string;
  graphLabel: string;
  graphText: string;
  graphScoreText: string;
  graphChip: string;
  graphSnapshot: string;
  graphMoveMarker: string;
  graphScoreMarker: string;
  graphMarkerRing: string;
  graphFillAbove: string;
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

let cached: StagePalette | null = null;
let probe: HTMLElement | null = null;
let revision = 0;
const listeners: (() => void)[] = [];

/** The palette for the current stage and colour mode. */
export function stagePalette(): StagePalette {
  if (cached) return cached;
  const builtin = builtinStage();
  // A hidden element fixed to the theme the workbench takes (applyStage): the colour mode's on the
  // program's stage, else the dark theme's whatever the mode (a Java skin's under that skin, the
  // wallpaper cards' over a wallpaper the user chose).
  if (!probe?.isConnected) {
    probe = document.createElement("div");
    probe.id = "stage-probe";
    probe.hidden = true;
    document.documentElement.append(probe);
  }
  probe.dataset.theme = builtin ? resolvedTheme() : "dark";
  const style = getComputedStyle(probe);
  const token = (name: string) => style.getPropertyValue(name).trim();
  cached = {
    builtin,
    panel: token("--ws-panel"),
    line: token("--ws-panel-line"),
    panelSolid: token("--ws-panel-solid"),
    cardEdge: token("--ws-card-edge"),
    cardShadow: token("--ws-card-shadow"),
    lineStrong: token("--ds-line-strong"),
    text: token("--ds-text"),
    text2: token("--ds-text-2"),
    text3: token("--ds-text-3"),
    accent: token("--ds-accent"),
    font: token("--ds-font"),
    winrateLine: token("--ws-winrate-line"),
    winrateMiss: token("--ws-winrate-miss"),
    engineWhiteLine: token("--ws-engine-white-line"),
    scoreLine: token("--ws-score-line"),
    blunderBar: token("--ws-blunder-bar"),
    commentNode: token("--ws-comment-node"),
    graphGrid: token("--ws-graph-grid"),
    graphBaseline: token("--ws-graph-baseline"),
    graphGuide: token("--ws-graph-guide"),
    graphLabel: token("--ws-graph-label"),
    graphText: token("--ws-graph-text"),
    graphScoreText: token("--ws-graph-score-text"),
    graphChip: token("--ws-graph-chip"),
    graphSnapshot: token("--ws-graph-snapshot"),
    graphMoveMarker: token("--ws-graph-move-marker"),
    graphScoreMarker: token("--ws-graph-score-marker"),
    graphMarkerRing: token("--ws-graph-marker-ring"),
    graphFillAbove: token("--ws-graph-fill-above"),
    graphFillBelow: token("--ws-graph-fill-below"),
    stoneBlack: token("--ws-stone-black"),
    stoneBlackEdge: token("--ws-stone-black-edge"),
    stoneWhite: token("--ws-stone-white"),
    stoneWhiteEdge: token("--ws-stone-white-edge"),
    onBlack: token("--ws-on-black"),
    onWhite: token("--ws-on-white"),
    barGrid: token("--ws-bar-grid"),
    gain: token("--ws-gain"),
    loss: token("--ws-loss"),
    live: token("--ws-live"),
    idle: token("--ws-idle"),
    treeNode: token("--ws-tree-node"),
    treeEmpty: token("--ws-tree-empty"),
    treeLine: token("--ws-tree-line"),
    treeLineOff: token("--ws-tree-line-off"),
    treeDim: token("--ws-tree-dim"),
    treeNodes: [1, 2, 3, 4, 5, 6, 7].map((k) => token(`--ws-node-${k}`)),
    scrollThumb: token("--ws-scroll-thumb"),
  };
  return cached;
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

/** The stage as the style sheets see it. The workbench is fixed to the theme its stage takes, so
 *  the interface colours a theme sets (design/palette.ts) stay off its panes: the colour mode's
 *  on the program's stage; over a wallpaper the user chose and under the Java skins the dark
 *  theme's in either colour mode, like the canvas panels beside them. */
function applyStage(): void {
  const builtin = builtinStage();
  document.documentElement.dataset.stage = builtin ? "builtin" : skin() !== "modern" ? "java" : "wallpaper";
  document.getElementById("main")?.setAttribute("data-theme", builtin ? resolvedTheme() : "dark");
}

function changed(): void {
  cached = null;
  revision++;
  applyStage();
  for (const listener of listeners) listener();
}

onAppearanceChange(changed);
onThemeChange(changed);
applyStage();
