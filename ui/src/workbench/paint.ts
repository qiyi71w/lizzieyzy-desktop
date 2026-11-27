// Canvas painters reproducing the fixed Java drawing (wimi321/lizzieyzy-next cb53acc).
// Analysis presentation consumes node-persisted projections, independent of engine pondering.
import type {
  BoardOverlayView,
  CandidateView,
  CoachMarks,
  CoordinateStyle,
  DisplaySettings,
  EstimateStyle,
  FocusView,
  LastAction,
  Mark,
  MoveRank,
  NodeAnalysisView,
  Point,
  RawNetView,
  RegionView,
  ScoreView,
  Stone,
  SubBoardMode,
  ThemeSettings,
  TrackingSettings,
  TreeNodeView,
  WinrateReading,
} from "../contract/generated";
import { format, language, t } from "../i18n";
import { appearance, frameMetrics, onAppearanceChange } from "../appearance/state";
import { softStoneShadow, stonePicture } from "../appearance/stones";
import { chineseClassicLabel, cssColor, customFont, drawChineseClassicMarkers, javaHue, localeFonts, opaque, reverseColor, sameRgba, THEME_DEFAULT, themeColor, themedFont } from "../appearance/model";
import { stagePalette, stageWallpaper, type StagePalette } from "../design/stage";
import { CARD_RADIUS, type BoardGeom, type Rect } from "./layout";
import { komiText, lead as leadOf, previousMove } from "./wording";
import { RecentCache } from "./recentCache";

const COLS = "ABCDEFGHJKLMNOPQRSTUVWXYZ";
const COLS_WITH_I = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

/** The 坐标格式 labels (BoardRenderer.java:1038-1099 with Board.asName, Board.java:444-467):
 *  columns are letters without I, letters with I (含I / 野狐), or numbers (数字); wider boards than
 *  the alphabet number them. Rows count up from the bottom unless numbered from the top or 野狐. */
export function coordinateLabels(style: CoordinateStyle, cols: number, rows: number): { cols: string[]; rows: string[] } {
  const numeric = style === "NumbersFromTop" || style === "NumbersFromBottom";
  const alphabet = style === "WithI" || style === "Fox" ? COLS_WITH_I : COLS;
  const fromTop = style === "NumbersFromTop" || style === "Fox";
  return {
    cols: Array.from({ length: cols }, (_, k) => (numeric || cols > alphabet.length ? `${k + 1}` : alphabet[k])),
    rows: Array.from({ length: rows }, (_, k) => `${rows <= 25 && !fromTop ? rows - k : k + 1}`),
  };
}

/** CoordinateLabels (Java): the coordinates around the main board at `scale` times their usual
 *  size, bold or plain. */
export interface CoordinateLook {
  scale: number;
  bold: boolean;
}

/** CoordinateLabels.of(theme): a theme's 棋盘坐标 (its size one of 100, 140 and 200 percent). */
export function coordinateLook(theme: ThemeSettings): CoordinateLook {
  return { scale: theme.coordinateSize / 100, bold: theme.coordinateBold };
}

export interface NameWinrateTexts {
  black: string;
  white: string;
}

/** NameWinrates.black / white: Black's win rate to one decimal and White's making 100 with it as
 *  shown, "--" for both without analysis. The reading is the statistics bar's own
 *  (StatisticsView.reading). */
export function nameWinrates(reading: WinrateReading | null): NameWinrateTexts {
  if (!reading) return { black: "--", white: "--" };
  // Java Math.round: half up.
  const tenths = Math.floor(reading.black * 10 + 0.5);
  const percent = (t: number) => `${Math.trunc(t / 10)}.${t % 10}%`;
  return { black: percent(tenths), white: percent(1000 - tenths) };
}

/** A text's ink box (canvas coordinates): from its left end and the font's ascent above the
 *  baseline to its right end and the descent below. */
export interface TextBox {
  x: number;
  y: number;
  w: number;
  h: number;
}

/** What drawName drew: the names as cut (fitName), the win rates beside them while shown, the
 *  click area (BoardNamePlate.hitBox: from the start of Black's text to the end of White's, the
 *  stones between them included, the font's height) and each text's box. */
export interface NamesReport {
  black: string;
  white: string;
  winrates: NameWinrateTexts | null;
  box: TextBox;
  blackBox: TextBox | null;
  whiteBox: TextBox | null;
  blackRateBox: TextBox | null;
  whiteRateBox: TextBox | null;
}

/** Board.maybeConvertNormalCoordsToOther of the engine coordinate: 含I letters for 含I and
 *  野狐, rows from the top for 野狐; numeric styles and boards over 25 keep the plain name. */
export function frameCoordinate(p: Point | null, style: CoordinateStyle, width: number, height: number): string {
  if (!p) return "pass";
  if (width > 25 || height > 25) return `(${p.x},${p.y})`;
  if (style === "WithI") return `${"ABCDEFGHIJKLMNOPQRSTUVWXYZ"[p.x]}${height - p.y}`;
  if (style === "Fox") return `${"ABCDEFGHIJKLMNOPQRSTUVWXYZ"[p.x]}${p.y + 1}`;
  return `${"ABCDEFGHJKLMNOPQRSTUVWXYZ"[p.x]}${height - p.y}`;
}

const FONT = '"DejaVu Sans", "Noto Sans CJK SC", sans-serif';
/** LizzieFrame.playoutsFont = Font("Dialog.plain", 12) (Utils.java:95): Arial on Windows, Lucida
 * Grande on macOS, DejaVu Sans on common Linux. */
const DIALOG_FONT = navigator.userAgent.includes("Windows")
  ? "Arial"
  : navigator.userAgent.includes("Mac OS")
    ? '"Lucida Grande"'
    : '"DejaVu Sans"';
/** Config.sysDefaultFontName = Dialog.plain; CJK falls back like the JDK composite font. */
export const GRAPH_FONT = `${DIALOG_FONT}, "Noto Sans CJK SC", "Microsoft YaHei", sans-serif`;
const i = Math.trunc;

/** LizzieFrame.uiFont (ui-font-name), playoutsFont (font-name, 计算量及其他字体) and winrateFont
 *  (winrate-font-name): Font(Config.sysDefaultFontName, PLAIN, 12) and the bundled Lizzie font
 *  unless the theme names a font (Utils.loadFonts, Utils.java:92-114; Config.readThemeVaule
 *  2231-2240). Startup resolves "Lizzie Default" for the UI font to Dialog (Theme.uiFontName's
 *  Chinese 微软雅黑 swap needs Lizzie.config, which is not yet set while Config loads). */
let UI_FONT = GRAPH_FONT;
let PLAYOUTS_FONT = GRAPH_FONT;
const LIZZIE_WINRATE_FONT = `"Lizzie Winrate", ${FONT}`;
let WINRATE_FONT = LIZZIE_WINRATE_FONT;
/** A configured winrate font is created BOLD (Utils.java:111-114). */
let winrateBold = false;
/** Font weight prefix of a family as Java created it. */
const weightOf = (family: string, bold = false) => (bold || (winrateBold && family === WINRATE_FONT) ? "bold " : "");
/** Lizzie.config.uiFontName and Lizzie.config.fontName as CSS stacks, for the windows that
 *  create their own fonts from them (MoveListFrame.java:320-321, 3453, 5473, 5680, 5860). */
export function themeFonts(): { ui: string; info: string } {
  return { ui: UI_FONT, info: PLAYOUTS_FONT };
}
/** LizzieFrame.winrateFont's face (winrate-font-name, else the bundled Lizzie font), for the
 *  windows that draw win rates themselves (黑白胜率大字). */
export function winrateFace(): string {
  return WINRATE_FONT;
}
/** Whether the theme names its ui-font-name / font-name (not Lizzie Default). */
let uiNamed = false;
let infoNamed = false;
/** The face of the panels' interface text (提子, 胜率条, 胜率图, 状态, 变化树, 双引擎状态): the theme's
 *  ui-font-name (`ui`) or font-name (`info`) when it names one, else the design system's
 *  `--ds-font`. The board's own texts keep the Java fonts above. */
export function panelFont(role: "ui" | "info" | "plain" = "plain"): string {
  if (role === "ui" && uiNamed) return UI_FONT;
  if (role === "info" && infoNamed) return PLAYOUTS_FONT;
  return stagePalette().font || GRAPH_FONT;
}

/** The committed theme as the painters use it (Config.readThemeVaule, Config.java:2241-2285): the
 *  values the Java default theme loads until the first appearance arrives. */
const look = {
  showShadow: true,
  shadowSize: 85,
  pureStone: false,
  pureBoard: null as string | null,
  pureBackground: null as string | null,
  indicator: 1,
  bestHue: 0.5,
  bestColor: "rgb(255,0,0)",
  chinese: false,
  customBoard: false,
  showCommentNode: true,
  board: undefined as HTMLImageElement | undefined,
  themeBoard: undefined as HTMLImageElement | undefined,
  background: undefined as HTMLImageElement | undefined,
  black: undefined as HTMLImageElement | undefined,
  white: undefined as HTMLImageElement | undefined,
  blackVariants: [] as HTMLImageElement[],
  whiteVariants: [] as HTMLImageElement[],
  /** 立体棋子 (AppearanceSettings.enhancedStones). */
  enhanced: false,
  /** glass-effect-level (Config.java:1809, default 1) and the theme's background-filter. */
  glassLevel: 1,
  backgroundFilter: 20,
  /** glass-blur-radius, liquid-blur-radius and the glass panel colours (Theme.java:738-767). */
  glassBlur: 10,
  liquidBlur: 15,
  glassOverlay: "rgba(24,24,26,0.4)",
  glassBorder: "rgba(255,255,255,0.102)",
  glassHighlight: "rgba(255,255,255,0.302)",
};
onAppearanceChange(() => {
  const a = appearance();
  if (!a) return;
  const th = a.theme;
  // LocaleFontSupport.resolveConfiguredFontName: the locale's candidates follow the Dialog fonts,
  // ahead of the generic family that would end the fallback.
  const dialog = GRAPH_FONT.replace(/, sans-serif$/, `${localeFonts(language())}, sans-serif`);
  UI_FONT = themedFont(th.uiFontName, dialog, dialog);
  PLAYOUTS_FONT = themedFont(th.fontName, dialog, dialog);
  winrateBold = customFont(th.winrateFontName);
  uiNamed = customFont(th.uiFontName);
  infoNamed = customFont(th.fontName);
  // A configured winrate font is a BOLD Font of its own: its stack ends with the bundled font so it
  // never equals the plain UI / info stack of the same family.
  WINRATE_FONT = winrateBold ? `${themedFont(th.winrateFontName, LIZZIE_WINRATE_FONT, dialog)}, "Lizzie Winrate"` : LIZZIE_WINRATE_FONT;
  Object.assign(look, {
    showShadow: th.showStoneShadow,
    shadowSize: th.shadowSize,
    pureStone: th.usePureStone,
    pureBoard: th.usePureBoard ? cssColor(opaque(th.pureBoardColor)) : null,
    pureBackground: th.usePureBackground ? cssColor(opaque(th.pureBackgroundColor)) : null,
    indicator: th.stoneIndicatorType,
    bestHue: javaHue(th.bestMoveColor),
    bestColor: cssColor(reverseColor(th.bestMoveColor)),
    chinese: a.settings.boardStyle === "ChineseClassic",
    customBoard: a.customBoard,
    showCommentNode: th.showCommentNodeColor,
    glassLevel: a.settings.glassEffectLevel,
    enhanced: a.settings.enhancedStones,
    backgroundFilter: th.backgroundFilter,
    glassBlur: th.glassBlurRadius,
    liquidBlur: th.liquidBlurRadius,
    glassOverlay: cssColor(th.glassPanelOverlayColor),
    glassBorder: cssColor(th.glassPanelBorderColor),
    glassHighlight: cssColor(th.glassPanelHighlightColor),
    ...a.images,
  });
});

/** The hovered candidate's variation as the main board shows it (BoardRenderer.drawBranch); `order`
 *  is -1 for the 下一手 详细 point's variation (BoardRenderer.mouseOveredMove). */
export interface PreviewVariation {
  order: number;
  pv: (Point | null)[];
  pv_captures: Point[][];
  /** kata-analyze `pvVisits`, one per `pv` move; empty when the engine sends none. */
  pv_visits: number[];
  /** Moves shown: limit-branch-length, or the branch replay's displayed length. */
  length: number;
}

/** What the main board shows for the current node. */
export interface BoardScene {
  cols: number;
  rows: number;
  stones: (Stone | null)[];
  lastAction: LastAction | null;
  toPlay: Stone;
  marks: Mark[];
  nextMoves: Point[];
  /** Actual first child only; a pass/setup first child has no point even if later branches do. */
  nextMainMove: Point | null;
  nextColor: Stone | null;
  black: string;
  white: string;
  /** 棋盘下方显示黑白名字 with a player name to show (BoardRenderer.isEmptyName). */
  names: boolean;
  /** Both sides' live win rates beside the names (NameWinrates) while they are shown, as drawn:
   *  Black's and White's, or "--" for both without analysis. */
  nameWinrates: NameWinrateTexts | null;
  /** 棋盘坐标 of the committed theme (CoordinateLabels). */
  labels: CoordinateLook;
  snapshot: boolean;
  mainTrunk: boolean;
  /** 试下 runs (LizzieFrame.isTrying): the core numbers only its own stones. */
  trying: boolean;
  analysis: NodeAnalysisView | null;
  overlay: BoardOverlayView;
  display: DisplaySettings;
  suggestionLimit: number;
  /** Points ("x,y") of the candidate kept beyond 最大候选点数 (MoveData.lastTimeUnlimited). */
  unlimited: Set<string>;
  /** 显示候选点 for the side to move (F, holding Z, 延迟显示候选点). */
  candidatesShown: boolean;
  /** 显示策略网络 (T). */
  policy: boolean;
  /** 纯网络 (H) is on. */
  rawNetOn: boolean;
  /** Its result for this position; null until it arrives or for another position. */
  rawNet: RawNetView | null;
  katago: boolean;
  engine: boolean;
  /** The hovered candidate (`order`), while hovering candidates applies. */
  hovered: number | null;
  /** The hovered candidate's variation, null when no preview shows. */
  preview: PreviewVariation | null;
  /** Ownership squares: 纯网络's or Kata形势's (main.ts / analysis/info.ts decide which). */
  ownership: OwnershipLayer | null;
  /** 终局点目 in progress: its marks replace the analysis overlays (BoardRenderer.java:328-426). */
  score: ScoreView | null;
  /** The problem move a 问题手 jump marked (LizzieFrame.clickbadmove), while its parent shows. */
  mark: Point | null;
  /** 强制计算区域 (main board only). */
  region: RegionView | null;
  /** The region selection being dragged (main board only). */
  selecting: { allow: boolean; from: Point; to: Point } | null;
  /** 选点重点研究 of this position (main board only) and its outline settings. */
  focus: FocusView | null;
  tracking: TrackingSettings;
  /** 优先分析此点 points of this position (LizzieFrame.priorityMoveCoords, main board only). */
  priority: Point[];
  /** 显示落子矩形: the empty point under the pointer and the side to play (drawmoveblock). */
  moveRect: { point: Point; black: boolean } | null;
  /** 点击棋子回看 shows (BoardRenderer.java:313-326): its 落子评价 marks, or only the last move's
   *  mark (drawLastMoveMark). The scene then holds the reviewed position. */
  review: "ranks" | "last" | null;
  /** AI 陪练's hint and 看 AI 推荐 marks (main board only; BoardRenderer.coachMarks). */
  coach: CoachMarks | null;
}

/** What the main board drew, for the acceptance probe. */
export interface BoardReport {
  coordinates: boolean;
  /** Below the board: player names, the pure network line, or nothing. */
  bottomLine: "names" | "rawNet" | null;
  /** The player names as drawn (fitName), the win rates beside them and where each was drawn,
   *  while they show. */
  names: NamesReport | null;
  rawNetInfo: string | null;
  moveNumbers: { x: number; y: number; text: string; red: boolean }[];
  lastMoveMarker: Point | null;
  ranks: { x: number; y: number; rank: MoveRank; last: boolean }[];
  rankCorner: boolean;
  candidateMode: "stats" | "policy" | "rawNet" | "hidden";
  candidates: { x: number; y: number; texts: string[]; badge: string | null }[];
  rawNetPoints: number;
  ownership: string | null;
  nextMove: { x: number; y: number; texts: string[]; fill: string } | null;
  preview: { order: number; length: number; numbers: number } | null;
  /** Coordinate labels drawn along the top and the left edge (坐标格式). */
  coordinateLabels: { cols: string[]; rows: string[] } | null;
  /** Every coordinate label drawn: its text, font, font size (px) and ink box. */
  coordinateText: { text: string; font: string; size: number; box: TextBox }[];
  /** The best candidate's ring: blue (show-blue-ring) or the faint black ring. */
  bestRing: "blue" | "plain" | null;
  /** Grey unimportant-candidate circles drawn (show-nosugg-circle, rank 20 and beyond). */
  unimportant: number;
  /** Rank badges drawn in the white style (white-suggestion-order-white while White plays). */
  whiteOrderBadges: boolean;
  /** PV visits labels drawn on the hovered variation. */
  pvVisits: { x: number; y: number; text: string }[];
  /** Kata形势 squares drawn: the style and whose ownership. */
  estimate: { style: EstimateStyle; source: OwnershipLayer["source"] } | null;
  /** 终局点目 marks drawn: red dots (neutral / 不计入) and owned squares. */
  score: { neutral: number; black: number; white: number } | null;
  /** The marked problem move's magenta ring (drawFocusStone). */
  mark: Point | null;
  /** The region squares drawn (drawAllSelectedRectByCoords) and the dragged rectangle. */
  region: { allow: boolean; points: number } | null;
  selecting: { allow: boolean; from: Point; to: Point } | null;
  /** Tracked-point markers: the quality outline colour (null when hidden), the progress ratio
   *  while still searched, and the pending-cancel arc. */
  tracked: { x: number; y: number; outline: string | null; progress: number | null; cancelling: boolean }[];
  /** The 落子矩形 drawn. */
  moveRect: Point | null;
  /** The click review drawn: its position's node marks ("ranks" / "last"). */
  review: BoardScene["review"];
  /** The coach marks drawn: the hint's area and the marked point. */
  coach: CoachMarks | null;
}

export const images: Record<string, HTMLImageElement> = {};
export function loadImages(names: string[]): Promise<void> {
  return Promise.all(
    names.map(
      (n) =>
        new Promise<void>((resolve) => {
          const img = new Image();
          img.onload = () => resolve();
          img.onerror = () => resolve();
          img.src = `/assets/${n}`;
          images[n] = img;
        }),
    ),
  ).then(() => undefined);
}

export function prepare(canvas: HTMLCanvasElement, r: Rect): CanvasRenderingContext2D {
  canvas.style.left = `${r.x}px`;
  canvas.style.top = `${r.y}px`;
  canvas.style.width = `${r.w}px`;
  canvas.style.height = `${r.h}px`;
  const dpr = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(r.w * dpr));
  const h = Math.max(1, Math.round(r.h * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, r.w, r.h);
  return g;
}

function circle(g: CanvasRenderingContext2D, x: number, y: number, r: number) {
  g.beginPath();
  g.arc(x, y, r, 0, Math.PI * 2);
}

/** Each context's pattern of the texture it tiles last (a context tiles one texture: its board's or
 *  its wallpaper), so that a texture replaced is not kept. */
const patterns = new WeakMap<CanvasRenderingContext2D, { img: HTMLImageElement; pattern: CanvasPattern }>();

/** Tile `img` over the rect with the texture anchored at (ax, ay) (TexturePaint semantics). */
function drawTexture(g: CanvasRenderingContext2D, img: HTMLImageElement | undefined, x: number, y: number, w: number, h: number, ax = x, ay = y) {
  if (!img?.naturalWidth) {
    g.fillStyle = "rgb(217,152,77)";
    g.fillRect(x, y, w, h);
    return;
  }
  let tiled = patterns.get(g);
  if (tiled?.img !== img) patterns.set(g, (tiled = { img, pattern: g.createPattern(img, "repeat")! }));
  tiled.pattern.setTransform(new DOMMatrix([1, 0, 0, 1, ax, ay]));
  g.fillStyle = tiled.pattern;
  g.fillRect(x, y, w, h);
}

/** The goban being painted: its origin and size anchor the texture of the board and of every disc
 *  cut from it (BoardRenderer `paint`), and `image` / `custom` say what covers it. */
const surface = { x: 0, y: 0, size: 0, image: undefined as HTMLImageElement | undefined, custom: false };

/** The board paint over a rect (BoardRenderer.drawWoodenBoard, BoardRenderer.java:3620-3651): the
 *  pure-board colour; a custom board image scaled to cover the board and centred
 *  (drawCoverTextureImage, 4020-4038); else the theme texture tiled from the board origin. */
function boardFill(g: CanvasRenderingContext2D, x: number, y: number, w: number, h: number) {
  const img = surface.image;
  if (look.pureBoard) {
    g.fillStyle = look.pureBoard;
    g.fillRect(x, y, w, h);
  } else if (surface.custom && img?.naturalWidth) {
    const scale = Math.max(surface.size / img.naturalWidth, surface.size / img.naturalHeight);
    const pattern = g.createPattern(img, "repeat")!;
    pattern.setTransform(new DOMMatrix([scale, 0, 0, scale, surface.x + (surface.size - img.naturalWidth * scale) / 2, surface.y + (surface.size - img.naturalHeight * scale) / 2]));
    g.fillStyle = pattern;
    g.fillRect(x, y, w, h);
  } else drawTexture(g, img, x, y, w, h, surface.x, surface.y);
}

/** jhlabs GaussianFilter(radius) on an opaque region: makeKernel (sigma = radius / 3, taps beyond
 * the radius zeroed, normalised), then two convolveAndTranspose passes with CLAMP_EDGES, each
 * rounding to (int)(v + 0.5). Canvas `filter` is not used: WebKitGTK ignores it. The kernel works
 * in device pixels. */
interface Kernel {
  rad: number;
  reach: number;
  weights: number[];
}

function blurKernel(radius: number): Kernel {
  const rad = radius * (window.devicePixelRatio || 1);
  const reach = Math.ceil(rad);
  const sigma = rad / 3;
  const kernel = Array.from({ length: reach * 2 + 1 }, (_, k) => {
    const d = (k - reach) ** 2;
    return d > rad * rad ? 0 : Math.exp(-d / (2 * sigma * sigma)) / Math.sqrt(2 * Math.PI * sigma);
  });
  const total = kernel.reduce((a, b) => a + b, 0);
  return { rad, reach, weights: kernel.map((v) => v / total) };
}

/** A blurred region of the stage in device pixels: its rectangle, its kernel and its pixels. */
interface Frosted {
  x: number;
  y: number;
  w: number;
  h: number;
  k: Kernel;
  img: ImageData;
}

/** The regions a static background blurred. */
export type FrostRegions = Frosted[];

/** Rows [from, to) of column `X` of the region [x, x + w) x [y, y + h) blurred as blurRegion blurs it
 *  (the same sums in the same order), written as column `X - x` of `img` (the region's picture): the
 *  horizontal pass, clamped to the region's columns, over the rows the vertical one reads (into
 *  `across`, h * 3 values), then the vertical pass, clamped to its rows. `src` holds the unblurred
 *  pixels with its top-left pixel at the origin. */
function frostRows(src: ImageData, x: number, y: number, w: number, h: number, k: Kernel, X: number, from: number, to: number, img: ImageData, across: Uint8ClampedArray) {
  const { reach, weights } = k;
  const px = src.data;
  const lo = Math.max(0, from - reach);
  const hi = Math.min(h, to + reach);
  for (let l = lo; l < hi; l++) {
    let r0 = 0;
    let g0 = 0;
    let b0 = 0;
    for (let s = -reach; s <= reach; s++) {
      const wt = weights[s + reach];
      if (!wt) continue;
      const o = ((y + l) * src.width + Math.min(x + w - 1, Math.max(x, X + s))) * 4;
      r0 += px[o] * wt;
      g0 += px[o + 1] * wt;
      b0 += px[o + 2] * wt;
    }
    across[l * 3] = i(r0 + 0.5);
    across[l * 3 + 1] = i(g0 + 0.5);
    across[l * 3 + 2] = i(b0 + 0.5);
  }
  const d = img.data;
  for (let a = from; a < to; a++) {
    let r0 = 0;
    let g0 = 0;
    let b0 = 0;
    for (let s = -reach; s <= reach; s++) {
      const wt = weights[s + reach];
      if (!wt) continue;
      const o = Math.min(h - 1, Math.max(0, a + s)) * 3;
      r0 += across[o] * wt;
      g0 += across[o + 1] * wt;
      b0 += across[o + 2] * wt;
    }
    const o = (a * w + X - x) * 4;
    d[o] = i(r0 + 0.5);
    d[o + 1] = i(g0 + 0.5);
    d[o + 2] = i(b0 + 0.5);
    d[o + 3] = 255;
  }
}

/** The device rectangle of a stage rectangle. */
function deviceRect(r: Rect): [number, number, number, number] {
  const dpr = window.devicePixelRatio || 1;
  return [Math.round(r.x * dpr), Math.round(r.y * dpr), Math.round(r.w * dpr), Math.round(r.h * dpr)];
}

/** Blurs region `r` of what `g` holds; the result is kept in `frost`. */
function blurRegion(g: CanvasRenderingContext2D, r: Rect, radius: number, frost: FrostRegions) {
  const k = blurKernel(radius);
  const { reach, weights } = k;
  const [x, y, w, h] = deviceRect(r);
  const img = g.getImageData(x, y, w, h);
  const px = img.data;
  const tmp = new Uint8ClampedArray(px.length);
  const pass = (src: Uint8ClampedArray, dst: Uint8ClampedArray, horizontal: boolean) => {
    const [n, lines] = horizontal ? [w, h] : [h, w];
    for (let l = 0; l < lines; l++)
      for (let a = 0; a < n; a++) {
        let r0 = 0;
        let g0 = 0;
        let b0 = 0;
        for (let s = -reach; s <= reach; s++) {
          const wt = weights[s + reach];
          if (!wt) continue;
          const p = Math.min(n - 1, Math.max(0, a + s));
          const o = (horizontal ? l * w + p : p * w + l) * 4;
          r0 += src[o] * wt;
          g0 += src[o + 1] * wt;
          b0 += src[o + 2] * wt;
        }
        const o = (horizontal ? l * w + a : a * w + l) * 4;
        dst[o] = i(r0 + 0.5);
        dst[o + 1] = i(g0 + 0.5);
        dst[o + 2] = i(b0 + 0.5);
        dst[o + 3] = 255;
      }
  };
  pass(px, tmp, true);
  pass(tmp, px, false);
  g.putImageData(img, x, y);
  frost.push({ x, y, w, h, k, img });
}

/** GlassEffectRenderer.drawLiquidEffects (GlassEffectRenderer.java:226-266) at the moment `at`
 *  (ms) the background is painted: the moving highlight (6 s cycle) and the top / bottom inner
 *  shadows. */
function liquidEffects(g: CanvasRenderingContext2D, r: Rect, at: number) {
  const phase = (at % 6000) / 6000;
  const cx = r.x + r.w * (0.5 + 0.3 * Math.sin(phase * Math.PI * 2));
  const cy = r.y + r.h * (0.5 + 0.3 * Math.cos(phase * Math.PI * 2 * 0.7));
  const highlight = g.createRadialGradient(cx, cy, 0, cx, cy, Math.max(r.w, r.h) * 0.5);
  highlight.addColorStop(0, "rgba(255,255,255,0.196)");
  highlight.addColorStop(0.5, "rgba(255,255,255,0.059)");
  highlight.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = highlight;
  g.beginPath();
  g.roundRect(r.x, r.y, r.w, r.h, CARD_RADIUS);
  g.fill();
  const top = g.createLinearGradient(0, r.y, 0, r.y + 15);
  top.addColorStop(0, "rgba(0,0,0,0.157)");
  top.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = top;
  g.beginPath();
  g.roundRect(r.x + 2, r.y + 2, r.w - 4, 15, 8);
  g.fill();
  const bottom = g.createLinearGradient(0, r.y + r.h - 10, 0, r.y + r.h);
  bottom.addColorStop(0, "rgba(0,0,0,0)");
  bottom.addColorStop(1, "rgba(0,0,0,0.098)");
  g.fillStyle = bottom;
  g.beginPath();
  g.roundRect(r.x + 2, r.y + r.h - 10, r.w - 4, 10, 8);
  g.fill();
}

/** The workbench's cards (DESIGN.md 工作台卡片, layout.cardLayout) and the 1 px dividers between
 *  the sections of a card. */
export interface StageCards {
  cards: Rect[];
  dividers: Rect[];
}

/** The card shadow's blur, downward offset and inward spread (DESIGN.md Elevation: a soft
 *  downward shadow, no halo). */
const CARD_SHADOW = { blur: 22, offset: 8, spread: 10 };

/** One card: on the program's own stage its soft downward shadow, outside the card only (a CSS
 *  box-shadow does not show through its box), then the translucent ground and the half-pixel edge. */
function drawCard(g: CanvasRenderingContext2D, r: Rect, p: StagePalette, w: number, h: number) {
  if (p.builtin) {
    const dpr = window.devicePixelRatio || 1;
    // The casting shape is drawn off the canvas; its shadow is offset back under the card.
    const away = w + h + 100;
    g.save();
    g.beginPath();
    g.rect(0, 0, w, h);
    g.roundRect(r.x, r.y, r.w, r.h, CARD_RADIUS);
    g.clip("evenodd");
    g.shadowColor = p.cardShadow;
    g.shadowBlur = CARD_SHADOW.blur * dpr;
    g.shadowOffsetX = away * dpr;
    g.shadowOffsetY = CARD_SHADOW.offset * dpr;
    g.fillStyle = "#000";
    g.beginPath();
    const s = CARD_SHADOW.spread;
    g.roundRect(r.x + s - away, r.y + s, Math.max(1, r.w - 2 * s), Math.max(1, r.h - 2 * s), CARD_RADIUS);
    g.fill();
    g.restore();
  }
  g.fillStyle = p.panel;
  g.beginPath();
  g.roundRect(r.x, r.y, r.w, r.h, CARD_RADIUS);
  g.fill();
  g.strokeStyle = p.cardEdge;
  g.lineWidth = 1;
  g.stroke();
}

/** LizzieFrame.createBackground (wallpaper tiled from the panel origin), then the cards and the
 * dividers between their sections. The program's own stage tiles its wallpaper for the colour
 * mode and leaves it sharp under the cards. The pure background colour replaces the wallpaper and
 * Java's containers (LizzieFrame.java:8197-8208). Over a wallpaper the user chose, the glass
 * setting treats the ground of each card as drawContainer did Java's containers
 * (LizzieFrame.java:8221-8245): with glass-effect-level above 0, GlassEffectRenderer.drawGlassPanel
 * FROSTED (glass-blur-radius 10) or, from level 2, LIQUID (liquid-blur-radius 15 plus its
 * highlight and inner shadows) with the theme's overlay, border and highlight; at level 0 only
 * the region blurred by the theme's background-filter.
 * Painted once per size and layout into a detached canvas and returned as a PNG blob: a live
 * full-window canvas layer would be recomposited every frame by WebKitGTK. `at` is the moment of
 * the liquid highlight; the regions blurred are kept in `frost`. */
export function paintBackground(w: number, h: number, stage: StageCards, at: number, frost: FrostRegions): Promise<Blob | null> {
  const canvas = document.createElement("canvas");
  const g = prepare(canvas, { x: 0, y: 0, w, h });
  paintWallpaper(g, w, h);
  paintCards(g, w, h, stage, at, (r, radius) => blurRegion(g, r, radius, frost));
  const { promise, resolve } = Promise.withResolvers<Blob | null>();
  canvas.toBlob(resolve);
  return promise;
}

function paintWallpaper(g: CanvasRenderingContext2D, w: number, h: number) {
  const p = stagePalette();
  if (look.pureBackground && !p.builtin) {
    g.fillStyle = look.pureBackground;
    g.fillRect(0, 0, w, h);
  } else drawTexture(g, p.builtin ? stageWallpaper() : look.background, 0, 0, w, h, 0, 0);
}

/** The stage over its wallpaper: `blur` frosts a card's region of the wallpaper. */
function paintCards(g: CanvasRenderingContext2D, w: number, h: number, stage: StageCards, at: number, blur: (r: Rect, radius: number) => void) {
  const p = stagePalette();
  // The glass setting acts only on a stage the user chose: the program's own stage never frosts
  // its cards (DESIGN.md: 卡片不模糊背后的壁纸), whatever glass-effect-level says.
  const glass = p.builtin || look.pureBackground ? [] : stage.cards;
  const glassy = look.glassLevel > 0;
  const liquid = look.glassLevel >= 2;
  for (const r of glass) {
    if (r.w <= 0 || r.h <= 0 || r.x < 0 || r.y < 0 || r.x + r.w > w || r.y + r.h > h) continue;
    if (!glassy) {
      // GaussianFilter(0) leaves the region as it is.
      if (look.backgroundFilter > 0) blur(r, look.backgroundFilter);
      continue;
    }
    const radius = liquid ? look.liquidBlur : look.glassBlur;
    if (radius > 0) blur(r, radius);
    g.fillStyle = look.glassOverlay;
    g.beginPath();
    g.roundRect(r.x, r.y, r.w, r.h, CARD_RADIUS);
    g.fill();
    g.strokeStyle = look.glassBorder;
    g.lineWidth = 1;
    g.beginPath();
    g.roundRect(r.x + 0.5, r.y + 0.5, r.w, r.h, CARD_RADIUS);
    g.stroke();
    const light = g.createLinearGradient(0, r.y, 0, r.y + r.h * 0.3);
    light.addColorStop(0, look.glassHighlight);
    light.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = light;
    g.beginPath();
    g.roundRect(r.x + 1, r.y + 1, r.w - 2, i(r.h / 3), CARD_RADIUS);
    g.fill();
    if (liquid) liquidEffects(g, r, at);
  }
  for (const r of stage.cards) drawCard(g, r, p, w, h);
  g.fillStyle = p.line;
  for (const d of stage.dividers) g.fillRect(d.x, d.y, d.w, d.h);
}

/** The stage painted on a live canvas instead of being encoded, while the static background shows
 *  another stage (a separator held, a window resized, a panel shown or hidden): the wallpaper and
 *  the cards painted as paintBackground paints them, the liquid highlight at the moment of the
 *  background shown, and the frosted regions. The wallpaper is tiled from the origin, so its pixels
 *  hold whatever the workbench's size, and a frosted pixel depends only on the wallpaper and on how
 *  far it is from its region's sides: one whose distance to each side is at least `reach` in two
 *  regions, or the same side in both, is blurred alike in both. A region is copied from the frosted
 *  region it overlaps most (the static background's, or the last live paint's) wherever that
 *  holds, so only the bands near a moved side are computed each frame. */
export interface LiveStage {
  at: number;
  /** The frosted regions of the static background shown at the start, and of the last live paint. */
  shown: Frosted[];
  last: Frosted[];
  /** The wallpaper's pixels, read once a frosted pixel is computed, over every region computed so
   *  far rounded up to 512 device pixels, so that a window resized reads them again only every 512
   *  pixels it grows. */
  src: ImageData | null;
}

/** A live stage of the appearance whose static background shown was painted at `at` and blurred
 *  `frost` (empty when that background is for another appearance). */
export function beginLiveStage(at: number, frost: FrostRegions): LiveStage {
  return { at, shown: frost, last: [], src: null };
}

/** The wallpaper's pixels over at least `w` x `h` device pixels, rounded up to 512. */
function wallpaperPixels(w: number, h: number): ImageData {
  const dpr = window.devicePixelRatio || 1;
  const canvas = document.createElement("canvas");
  const ww = Math.ceil(w / 512) * 512;
  const wh = Math.ceil(h / 512) * 512;
  const g = prepare(canvas, { x: 0, y: 0, w: ww / dpr, h: wh / dpr });
  paintWallpaper(g, ww / dpr, wh / dpr);
  return g.getImageData(0, 0, canvas.width, canvas.height);
}

/** The span [lo, hi) of offsets into a region's side of length `n` at `at` blurred alike in the
 *  region of the same side at `fromAt` with length `fromN`. */
function alikeSpan(at: number, n: number, fromAt: number, fromN: number, reach: number): [number, number] {
  const lo = fromAt === at ? 0 : Math.max(reach, fromAt + reach - at);
  const hi = fromAt + fromN === at + n ? n : Math.min(n - reach, fromAt + fromN - reach - at);
  return [lo, hi];
}

/** The region [x, x + w) x [y, y + h) of `live`'s wallpaper frosted with `k`. */
function frostLive(live: LiveStage, x: number, y: number, w: number, h: number, k: Kernel): Frosted {
  const r = k.reach;
  let from: Frosted | null = null;
  let most = 0;
  for (const frosted of [live.last, live.shown])
    for (const f of frosted) {
      if (f.k.rad !== k.rad) continue;
      if (f.x === x && f.y === y && f.w === w && f.h === h) return f;
      const area = Math.max(0, Math.min(x + w, f.x + f.w) - Math.max(x, f.x)) * Math.max(0, Math.min(y + h, f.y + f.h) - Math.max(y, f.y));
      if (area > most) {
        most = area;
        from = f;
      }
    }
  const img = new ImageData(w, h);
  const [top, bottom] = from ? alikeSpan(y, h, from.y, from.h, r) : [0, 0];
  const [left, right] = from ? alikeSpan(x, w, from.x, from.w, r) : [0, 0];
  let across: Uint8ClampedArray | null = null;
  for (let a = 0; a < w; a++) {
    const X = x + a;
    const alike = from !== null && top < bottom && a >= left && a < right;
    if (alike) {
      const d = img.data;
      const s = from!.img.data;
      for (let l = top; l < bottom; l++) {
        const o = (l * w + a) * 4;
        const p = ((y + l - from!.y) * from!.w + X - from!.x) * 4;
        d[o] = s[p];
        d[o + 1] = s[p + 1];
        d[o + 2] = s[p + 2];
        d[o + 3] = 255;
      }
      if (top === 0 && bottom === h) continue;
    }
    const had = live.src;
    const src = had && had.width >= x + w && had.height >= y + h ? had : (live.src = wallpaperPixels(Math.max(had?.width ?? 0, x + w), Math.max(had?.height ?? 0, y + h)));
    across ??= new Uint8ClampedArray(h * 3);
    if (!alike) frostRows(src, x, y, w, h, k, X, 0, h, img, across);
    else {
      if (top > 0) frostRows(src, x, y, w, h, k, X, 0, top, img, across);
      if (bottom < h) frostRows(src, x, y, w, h, k, X, bottom, h, img, across);
    }
  }
  return { x, y, w, h, k, img };
}

/** The stage of `live` with the cards `stage` into `g` (prepared for the `w` x `h` workbench). */
export function paintLiveStage(g: CanvasRenderingContext2D, live: LiveStage, w: number, h: number, stage: StageCards): void {
  paintWallpaper(g, w, h);
  const painted: Frosted[] = [];
  paintCards(g, w, h, stage, live.at, (r, radius) => {
    const [x, y, rw, rh] = deviceRect(r);
    const f = frostLive(live, x, y, rw, rh, blurKernel(radius));
    painted.push(f);
    g.putImageData(f.img, x, y);
  });
  live.last = painted;
}

/** BoardRenderer.drawShadow with show-stone-shadow and shadow-size (BoardRenderer.java:3796-3860):
 *  the cached shadow grows with cachedR = stoneRadius * shadow-size / 100; at the default 85 it is
 *  the shape measured on the Java capture. 立体棋子 draws its own shadow instead. */
function stoneShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  if (!look.showShadow) return;
  if (look.enhanced) {
    softStoneShadow(g, cx, cy, r, look.shadowSize);
    return;
  }
  const k = look.shadowSize / 85;
  const off = r * 0.12 * k;
  const outer = r + r * 0.18 * k;
  g.save();
  g.globalAlpha = 0.85;
  const sh = g.createRadialGradient(cx + off, cy + off, r * 0.6, cx + off, cy + off, outer);
  sh.addColorStop(0, "rgba(0,0,0,0.55)");
  sh.addColorStop(1, "rgba(0,0,0,0)");
  g.fillStyle = sh;
  circle(g, cx + off, cy + off, outer);
  g.fill();
  g.restore();
}

/** 立体棋子's shadows of one board canvas, painted once into a transparent layer of the canvas's size
 *  and kept while `key` (the stones' points, the board geometry, the canvas size and transform and
 *  the shadow size) stays: analysis repaints then composite one image instead of two gradients per
 *  stone. Source-over is associative, so the layer composited onto the board equals the shadows
 *  painted onto it one by one. */
const shadowLayers = new WeakMap<HTMLCanvasElement | OffscreenCanvas, { key: string; layer: HTMLCanvasElement }>();

/** WebView2 paints the board canvases in software. A layer on the GPU would be read back from it at
 *  every repaint of the board (about ten a second while an engine analyses), so there the layer is
 *  kept in memory like the board it is composited onto. */
const SHADOW_LAYER: CanvasRenderingContext2DSettings = navigator.userAgent.includes("Windows") ? { willReadFrequently: true } : {};

function enhancedShadows(g: CanvasRenderingContext2D, points: number[], geometry: string, px: (p: number) => number, py: (p: number) => number, r: number) {
  const canvas = g.canvas;
  const t = g.getTransform();
  const key = `${canvas.width}x${canvas.height}|${t.a},${t.b},${t.c},${t.d},${t.e},${t.f}|${geometry}|${r}|${look.shadowSize}|${points.join(",")}`;
  let cached = shadowLayers.get(canvas);
  if (cached?.key !== key) {
    const layer = cached?.layer ?? document.createElement("canvas");
    if (layer.width !== canvas.width || layer.height !== canvas.height) {
      layer.width = canvas.width;
      layer.height = canvas.height;
    }
    const lg = layer.getContext("2d", SHADOW_LAYER)!;
    lg.setTransform(1, 0, 0, 1, 0, 0);
    lg.clearRect(0, 0, layer.width, layer.height);
    lg.setTransform(t);
    for (const p of points) softStoneShadow(lg, px(p), py(p), r, look.shadowSize);
    cached = { key, layer };
    shadowLayers.set(canvas, cached);
  }
  g.save();
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.drawImage(cached.layer, 0, 0);
  g.restore();
}

/** BoardRenderer.drawShadow2 (BoardRenderer.java:3757-3789) under policy and raw-net circles: a
 *  faint ring just outside the circle. Java never sets `cachedStoneCenter2`, so the cached image's
 *  gradient and fill sit at its corner and only the lower-right quarter of the ring is drawn. */
function ghostShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  const size = Math.max(1, i(i((r * 70) / 100) * 0.2));
  const outer = r + size;
  const width = 2 * outer + size;
  const ring = g.createRadialGradient(cx, cy, 0, cx, cy, outer);
  ring.addColorStop(Math.max(0, r / outer - 0.0001), "rgba(0,0,0,0)");
  ring.addColorStop(r / outer, "rgba(50,50,50,0.157)");
  ring.addColorStop(1, "rgba(0,0,0,0)");
  g.save();
  g.beginPath();
  g.rect(cx, cy, width, width);
  g.clip();
  g.fillStyle = ring;
  fillCircle(g, cx, cy, outer);
  g.restore();
}

/** BoardRenderer.drawStone with the theme stone images, or drawStoneSimple with use-pure-stone:
 *  a black or white disc, the white one outlined max(r / 16, 1) (BoardRenderer.java:3901-3916).
 *  At point (x, y), 立体棋子 shows the picture it picks there among the stone's own and its others;
 *  `shadow` false leaves the shadow to a pass before all stones. */
function drawStone(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, black: boolean, x: number, y: number, shadow = true) {
  if (shadow) stoneShadow(g, cx, cy, r);
  const own = black ? look.black : look.white;
  const others = black ? look.blackVariants : look.whiteVariants;
  const pick = look.enhanced && others.length > 0 ? stonePicture(others.length + 1, x, y) : 0;
  const img = pick === 0 ? own : others[pick - 1];
  if (look.pureStone) {
    g.fillStyle = black ? "#000" : "#fff";
    fillCircle(g, cx, cy, r);
    if (!black) {
      g.strokeStyle = "#000";
      strokeCircle(g, cx, cy, r, Math.max(r / 16, 1));
    }
  } else if (img?.naturalWidth) g.drawImage(img, cx - r, cy - r, r * 2 + 1, r * 2 + 1);
  else {
    g.fillStyle = black ? "#111" : "#f4f4f4";
    circle(g, cx, cy, r);
    g.fill();
  }
}

function textMid(g: CanvasRenderingContext2D, s: string, x: number, y: number, size: number, maxW: number, color: string, family = FONT) {
  let px = size;
  g.font = `${px}px ${family}`;
  while (px > 4 && g.measureText(s).width > maxW) {
    px -= 0.5;
    g.font = `${px}px ${family}`;
  }
  g.fillStyle = color;
  g.textAlign = "center";
  g.textBaseline = "middle";
  g.fillText(s, x, y);
}

/** Java FontMetrics ints (FontDesignMetrics rounds each value as (int)(0.95 + v)). */
function javaMetrics(g: CanvasRenderingContext2D) {
  const m = g.measureText("0");
  return { ascent: i(0.95 + m.fontBoundingBoxAscent), descent: i(0.95 + m.fontBoundingBoxDescent) };
}

function grid(g: CanvasRenderingContext2D, geo: BoardGeom, ox: number, oy: number, border: number, normal: number) {
  const sq = geo.square;
  const sqY = geo.squareY ?? sq;
  const mY = geo.marginY ?? geo.margin;
  g.strokeStyle = "#000";
  for (let k = 0; k < geo.cols; k++) {
    g.lineWidth = k === 0 || k === geo.cols - 1 ? border : normal;
    const a = geo.margin + sq * k;
    g.beginPath();
    g.moveTo(ox + a + 0.5, oy + mY);
    g.lineTo(ox + a + 0.5, oy + mY + sqY * (geo.rows - 1));
    g.stroke();
  }
  for (let k = 0; k < geo.rows; k++) {
    g.lineWidth = k === 0 || k === geo.rows - 1 ? border : normal;
    const a = mY + sqY * k;
    g.beginPath();
    g.moveTo(ox + geo.margin, oy + a + 0.5);
    g.lineTo(ox + geo.margin + sq * (geo.cols - 1), oy + a + 0.5);
    g.stroke();
  }
}

/** Star points of the standard sizes (BoardStylePainter). */
function hoshi(cols: number, rows: number): [number, number][] {
  if (cols !== rows) return [];
  const n = cols;
  const pts = n === 19 ? [3, 9, 15] : n === 13 ? [3, 6, 9] : n === 9 ? [2, 4, 6] : [];
  const out: [number, number][] = [];
  for (const a of pts) for (const b of pts) if (!(n === 9 && (a === 4) !== (b === 4))) out.push([a, b]);
  return out;
}

/** Utils.getPlayoutsString (Utils.java:992-1002), including Java's trailing decimal. */
export function playouts(n: number): string {
  if (n >= 10_000_000) return `${(Math.round(n / 100_000) / 10).toFixed(1)}m`;
  if (n >= 9950) return `${Math.round(n / 1000)}k`;
  if (n >= 1000) return `${(Math.round(n / 100) / 10).toFixed(1)}k`;
  return String(n);
}

// ------------------------------------------------------------------ Java drawing primitives

/** fillCircle: fillOval(x - r, y - r, 2r + 1, 2r + 1). */
function fillCircle(g: CanvasRenderingContext2D, x: number, y: number, radius: number) {
  g.beginPath();
  g.arc(x + 0.5, y + 0.5, radius + 0.5, 0, Math.PI * 2);
  g.fill();
}

/** drawCircle(g, x, y, radius, f): a BasicStroke(radius / f) oval. */
function strokeCircle(g: CanvasRenderingContext2D, x: number, y: number, radius: number, width: number) {
  g.lineWidth = width;
  circle(g, x, y, radius);
  g.stroke();
}

/** drawPolygonSmall: the red corner triangle at the upper left of a stone. */
function smallTriangle(g: CanvasRenderingContext2D, x: number, y: number, r: number) {
  const a = i((r * 16) / 15), b = i((r * 4) / 11);
  g.beginPath();
  g.moveTo(x - a, y - a);
  g.lineTo(x - a, y - b);
  g.lineTo(x - b, y - a);
  g.closePath();
  g.fill();
}

/** java.awt.Color.HSBtoRGB with an alpha (0-255). */
function hsb(hue: number, saturation: number, brightness: number, alpha = 255): string {
  const h = (hue - Math.floor(hue)) * 6;
  const f = h - Math.floor(h);
  const p = brightness * (1 - saturation);
  const q = brightness * (1 - saturation * f);
  const t = brightness * (1 - saturation * (1 - f));
  const [r, g, b] = [[brightness, t, p], [q, brightness, p], [p, brightness, t], [p, q, brightness], [t, p, brightness], [brightness, p, q]][i(h)] ?? [brightness, t, p];
  return `rgba(${i(r * 255 + 0.5)},${i(g * 255 + 0.5)},${i(b * 255 + 0.5)},${alpha / 255})`;
}

/** drawLeelazSuggestions hues: RGBtoHSB(2, 0, 0), RGBtoHSB(0, 255, 0); the best candidate takes
 *  Config.bestMoveColor, the hue of best-move-color (look.bestHue, cyan 0.5 by default). */
const RED_HUE = 0;
const GREEN_HUE = 1 / 3;
/** BoardRenderer.maxAlpha and drawLeelazSuggestions' minAlpha / alphaFactor. */
const MAX_ALPHA = 240;
const MIN_ALPHA = 32;
function suggestionAlpha(percent: number): number {
  return i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * Math.max(0, Math.log(percent) / 5 + 1));
}

/** BoardRenderer.drawString (BoardRenderer.java:4253-4289): the 100-point font scaled to `maxW`,
 *  capped at `maxH`, centred on x; aboveOrBelow 0 centres on y, 1 puts the baseline on y. */
function jString(g: CanvasRenderingContext2D, s: string, x: number, y: number, family: string, maxH: number, maxW: number, aboveOrBelow = 0, bold = false) {
  const weight = weightOf(family, bold);
  g.font = `${weight}100px ${family}`;
  const size = Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(s).width))));
  g.font = `${weight}${Math.max(1, size)}px ${family}`;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const fm = javaMetrics(g);
  const height = fm.ascent - fm.descent;
  const offset = aboveOrBelow === -1 ? i(height / 2) : aboveOrBelow === 1 ? -i(height / 2) : 0;
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y + i(height / 2) + offset);
}

/** drawCoordinate's sizing (jString's) by face, weight, limits and label: the board repaints with
 *  every analysis update, and a label is measured again only when one of them changes. */
const coordinateFits = new Map<string, { font: string; size: number; width: number; ascent: number; descent: number }>();

/** BoardRenderer.drawCoordinate: a coordinate label in the UI font, bold or plain, sized as
 *  jString sizes it within `maxH` / `maxW` (CoordinateLabels.maxHeight / maxWidth) and centred on
 *  (x, y). Returns what it drew, for the report. */
function drawCoordinate(g: CanvasRenderingContext2D, text: string, x: number, y: number, maxH: number, maxW: number, bold: boolean): BoardReport["coordinateText"][number] {
  const key = `${UI_FONT}\n${bold}\n${maxH}\n${maxW}\n${text}`;
  let fit = coordinateFits.get(key);
  if (!fit) {
    const weight = weightOf(UI_FONT, bold);
    g.font = `${weight}100px ${UI_FONT}`;
    const size = Math.max(1, Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(text).width)))));
    const font = `${weight}${size}px ${UI_FONT}`;
    g.font = font;
    const fm = javaMetrics(g);
    fit = { font, size, width: Math.round(g.measureText(text).width), ascent: fm.ascent, descent: fm.descent };
    if (coordinateFits.size >= 1024) coordinateFits.clear();
    coordinateFits.set(key, fit);
  }
  g.font = fit.font;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const left = x - i(fit.width / 2);
  const baseline = y + i((fit.ascent - fit.descent) / 2);
  g.fillText(text, left, baseline);
  return { text, font: fit.font, size: fit.size, box: { x: left, y: baseline - fit.ascent, w: fit.width, h: fit.ascent + fit.descent } };
}

/** The board paint under a disc (drawLeelazSuggestions / drawCircleBackground: setPaint(paint)). */
function woodDisc(g: CanvasRenderingContext2D, x: number, y: number, radius: number, alpha: number) {
  g.save();
  g.beginPath();
  g.arc(x + 0.5, y + 0.5, radius + 0.5, 0, Math.PI * 2);
  g.clip();
  g.globalAlpha = alpha;
  boardFill(g, x - radius - 1, y - radius - 1, 2 * radius + 3, 2 * radius + 3);
  g.restore();
}

/** Java String.valueOf(round(v * 10) / 10.0). */
function round1(v: number): string {
  return javaDouble(Math.round(v * 10) / 10 + 0);
}

/** Utils.convertScoreToString with 目差显示为与最佳选点之差: the difference to the best score. */
function scoreDiff(diff: number): string {
  const rounded = Math.round(Math.abs(diff) * 10) / 10;
  return diff > 0 ? `+${javaDouble(rounded)}` : diff < 0 ? `-${javaDouble(rounded)}` : javaDouble(Math.round(diff * 10) / 10 + 0);
}

/** MoveRankDefinition boardMarkRadiusFactor; the colours are the stage palette's `moveRank`. */
const RANK_RADIUS: Record<MoveRank, number> = { Best: 0.1, Good: 0.1, Normal: 0.1225, Inaccuracy: 0.145, Mistake: 0.1675, Blunder: 0.19 };

/** drawAllSelectedRectByCoords and the dragged drawSelectedRect: a square of the square width
 *  centred on each point, the dragged rectangle over the points it covers, allowed and avoided in
 *  the stage palette's region colours (BoardRenderer.java:1172-1177, 1201-1212). */
function drawRegion(g: CanvasRenderingContext2D, s: BoardScene, X: (x: number) => number, Y: (y: number) => number, sq: number, report: BoardReport): void {
  const half = i(sq / 2);
  const palette = stagePalette();
  if (s.region?.points.length) {
    g.fillStyle = s.region.allow ? palette.regionAllow : palette.regionAvoid;
    for (const p of s.region.points) g.fillRect(X(p.x) - half, Y(p.y) - half, sq, sq);
    report.region = { allow: s.region.allow, points: s.region.points.length };
  }
  const d = s.selecting;
  if (d) {
    g.fillStyle = d.allow ? palette.regionAllow : palette.regionAvoid;
    g.fillRect(X(d.from.x) - half, Y(d.from.y) - half, sq * (d.to.x - d.from.x + 1), sq * (d.to.y - d.from.y + 1));
    report.selecting = d;
  }
}

/** drawTrackedPointMarkers (BoardRenderer.java:2916-3017): on each empty tracked point a dashed
 *  quality ring in the 落子评价 colour of its candidate against the best one (grey without both),
 *  then, while it is still searched, a blue progress ring filling clockwise from the top towards
 *  the target visits, or the orange arc while a cancel is pending. */
function drawTracking(g: CanvasRenderingContext2D, s: BoardScene, X: (x: number) => number, Y: (y: number) => number, sq: number, r: number, report: BoardReport): void {
  const focus = s.focus;
  if (!focus?.points.length) return;
  const quality = Math.max(r + 2, Math.round(sq * 0.48));
  const progress = quality + Math.max(3, Math.round(sq * 0.075));
  const qualityWidth = Math.max(2.2, sq * 0.055);
  const progressWidth = Math.max(2.4, sq * 0.065);
  g.save();
  g.lineCap = "round";
  g.lineJoin = "round";
  for (const p of focus.points) {
    const { x, y } = p.point;
    if (x >= s.cols || y >= s.rows || s.stones[y * s.cols + x]) continue;
    const cx = X(x), cy = Y(y);
    const marker: BoardReport["tracked"][number] = { x, y, outline: null, progress: null, cancelling: false };
    if (s.tracking.showOutline) {
      marker.outline = p.rank ? stagePalette().moveRank[p.rank] : stagePalette().moveUnranked;
      g.globalAlpha = s.tracking.outlineOpacity / 100;
      g.setLineDash([Math.max(4.5, sq * 0.14), Math.max(3, sq * 0.09)]);
      g.lineWidth = qualityWidth;
      g.strokeStyle = marker.outline;
      circle(g, cx, cy, quality);
      g.stroke();
    }
    g.globalAlpha = 1;
    g.setLineDash([]);
    g.lineWidth = progressWidth;
    if (p.active) {
      const ratio = focus.target_visits <= 0 ? 0 : Math.max(0, Math.min(1, p.visits / focus.target_visits));
      marker.progress = ratio;
      g.strokeStyle = "rgba(25,118,210,0.4118)";
      circle(g, cx, cy, progress);
      g.stroke();
      const degrees = Math.round(360 * ratio);
      if (degrees > 0) {
        g.strokeStyle = "rgb(66,165,245)";
        g.beginPath();
        g.arc(cx, cy, progress, -Math.PI / 2, -Math.PI / 2 + (degrees * Math.PI) / 180);
        g.stroke();
      }
    } else if (focus.cancellation_pending) {
      marker.cancelling = true;
      g.strokeStyle = "rgb(245,166,35)";
      g.beginPath();
      g.arc(cx, cy, progress, (-145 * Math.PI) / 180, (-35 * Math.PI) / 180);
      g.stroke();
    }
    report.tracked.push(marker);
  }
  g.restore();
}

// ------------------------------------------------------------------ variations

interface Preview {
  stones: (Stone | null)[];
  numbers: Map<number, number>;
  /** Branch.pvVisitsList: the `pvVisits` of the move last placed on each point. */
  visits: Map<number, number>;
  captured: Map<number, Stone>;
  last: number | null;
  /** The last shown move is a pass. */
  pass: boolean;
  /** Side to play after the shown moves. */
  toPlay: Stone;
}

/** VariationPreviewGenerator / SubBoardRenderer Branch over the core's `Branch` result: each
 *  move's stone and the stones it captures come from the view; with 变化图中去掉死子 off
 *  (`removeDead` false) the captured stones stay on the board (`withSimulation`). */
function playVariation(stones: (Stone | null)[], toPlay: Stone, cols: number, v: { pv: (Point | null)[]; pv_captures: Point[][]; pv_visits?: number[] }, length: number, removeDead: boolean): Preview {
  const out = stones.slice();
  const numbers = new Map<number, number>();
  const visits = new Map<number, number>();
  const captured = new Map<number, Stone>();
  let color = toPlay;
  let last: number | null = null;
  let pass = false;
  for (let k = 0; k < Math.min(length, v.pv.length); k++) {
    const pt = v.pv[k];
    last = null;
    pass = !pt;
    if (pt) {
      const p = pt.y * cols + pt.x;
      out[p] = color;
      captured.delete(p);
      numbers.set(p, k + 1);
      const n = v.pv_visits?.[k];
      if (n !== undefined) visits.set(p, n);
      for (const q of removeDead ? v.pv_captures[k] ?? [] : []) {
        const index = q.y * cols + q.x;
        const stone = out[index];
        if (stone) captured.set(index, stone);
        out[index] = null;
        numbers.delete(index);
      }
      last = p;
    }
    color = color === "Black" ? "White" : "Black";
  }
  return { stones: out, numbers, visits, captured, last, pass, toPlay: color };
}

/** BoardRenderer.drawPass (BoardRenderer.java:504-523): translucent disc at the centre. */
function drawPassDisc(g: CanvasRenderingContext2D, ox: number, oy: number, geo: BoardGeom, blackToPlay: boolean) {
  const r = geo.stoneRadius;
  g.fillStyle = blackToPlay ? "rgba(255,255,255,0.314)" : "rgba(0,0,0,0.314)";
  g.beginPath();
  g.ellipse(ox + i(geo.size / 2), oy + i(geo.size / 2), i((r * 5) / 2), i((r * 5) / 2), 0, 0, Math.PI * 2);
  g.fill();
  g.fillStyle = blackToPlay ? "rgba(0,0,0,0.784)" : "rgba(255,255,255,0.784)";
  jString(g, t("BoardRenderer.pass"), ox + i(geo.size / 2), oy + i(geo.size / 2), UI_FONT, r * 3, i((r * 9) / 2));
}

/** VariationPreviewGenerator.drawPvVisits (VariationPreviewGenerator.java:384-411): the orange
 *  strip above the number with the move's visits. */
function drawPvVisits(g: CanvasRenderingContext2D, x: number, y: number, visits: number, last: boolean, sq: number) {
  const left = last ? 0.43 : visits >= 1000 ? 0.3 : 0.25;
  const width = last ? (visits >= 1000 ? 0.93 : 0.9) : visits >= 1000 ? 0.8 : 0.72;
  g.fillStyle = "#ffc800"; // java.awt.Color.ORANGE
  g.fillRect(i(x - sq * left), i(y - sq * 0.5), i(sq * width), Math.round(sq * 0.33));
  g.fillStyle = "#000";
  jString(g, playouts(visits), i(x + sq * 0.1), i(y - sq * 0.2), UI_FONT, sq * 0.33, sq * 0.8, 1);
}

/** drawPolygonSmallPv (VariationPreviewGenerator.java:466-480), called with the square width. */
function smallPvTriangle(g: CanvasRenderingContext2D, x: number, y: number, sq: number) {
  g.beginPath();
  g.moveTo(x - i((sq * 9) / 20), i(y - sq * 0.5));
  g.lineTo(x - i((sq * 9) / 20), i(y - sq * 0.18));
  g.lineTo(x - i((sq * 17) / 40), i(y - sq * 0.18));
  g.lineTo(x - i((sq * 11) / 40), i(y - sq * 0.5));
  g.closePath();
  g.fill();
}

/** VariationPreviewGenerator.drawAnnotations (VariationPreviewGenerator.java:316-369): branch
 *  numbers 1..n, the last one red with the corner triangle; the hovered point keeps its candidate
 *  information unless a later move was played there. 变化图显示计算量 adds each move's visits above
 *  its number (the last move only, or every move) when above pv-visits-limit. Returns
 *  hideMouseOverInfo. */
function drawPreviewNumbers(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, geo: BoardGeom, hover: number, cols: number, d: DisplaySettings, report: BoardReport): boolean {
  const r = geo.stoneRadius, sq = geo.square;
  const visitsLast = d.pvVisits !== "Off", visitsAll = d.pvVisits === "AllMoves";
  let hide = false;
  for (const [p, n] of preview.numbers) {
    if (p === hover) {
      if (n > 1) hide = true;
      else continue;
    }
    const x = px(p), y = py(p);
    const last = p === preview.last;
    const visits = preview.visits.get(p) ?? 0;
    const showVisits = (last ? visitsLast : visitsAll) && visits > d.pvVisitsLimit;
    if (showVisits) {
      drawPvVisits(g, x, y, visits, last, sq);
      report.pvVisits.push({ x: p % cols, y: i(p / cols), text: playouts(visits) });
    }
    if (last) {
      g.fillStyle = "#f00";
      if (showVisits) smallPvTriangle(g, x, y, sq);
      else smallTriangle(g, x, y, r);
    } else g.fillStyle = preview.stones[p] === "Black" ? "#fff" : "#000";
    const wide = !last && n >= 100;
    if (showVisits) jString(g, String(n), x, i(y + sq * 0.12), UI_FONT, r * 1.3, i(r * (wide ? 1.85 : 1.4)));
    else jString(g, String(n), x, y, UI_FONT, r * 1.4, i(r * (n >= 100 ? 1.85 : 1.4)));
    report.moveNumbers.push({ x: p % cols, y: i(p / cols), text: String(n), red: last });
  }
  return hide;
}

function drawPreviewStones(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, r: number) {
  for (const [p, stone] of preview.captured) {
    if (preview.stones[p]) continue;
    g.fillStyle = stone === "Black" ? "rgba(0,0,0,0.235)" : "rgba(255,255,255,0.333)";
    circle(g, px(p), py(p), r);
    g.fill();
  }
}

/** drawFloatCapturedPoint (VariationPreviewGenerator.java:192-215, 287-313): on 棋盘同步工具's float
 *  board a stone the variation captures is covered by the board texture (radius stoneRadius + 1,
 *  the theme board tiled from the board's origin) and, unless it is the hovered point, crossed by
 *  1px black lines that stop at the board's edge; 野狐 (boardType 0) moves them 1px right and down.
 *  No stone follows there (Stone.needDrawBlack / needDrawWhite). */
function drawFloatCapturedPoints(g: CanvasRenderingContext2D, preview: Preview, px: (p: number) => number, py: (p: number) => number, r: number, cols: number, rows: number, hover: number, boardType: number) {
  const radius = r + 1;
  const offset = boardType === 0 ? 1 : 0;
  for (const p of preview.captured.keys()) {
    if (preview.stones[p]) continue;
    const cx = px(p), cy = py(p);
    g.save();
    g.beginPath();
    g.arc(cx + 0.5, cy + 0.5, radius + 0.5, 0, Math.PI * 2);
    g.clip();
    drawTexture(g, surface.image, cx - radius - 1, cy - radius - 1, 2 * radius + 3, 2 * radius + 3, surface.x, surface.y);
    g.restore();
    if (p === hover) continue;
    const x = p % cols, y = i(p / cols);
    // drawLine covers both end pixels.
    g.strokeStyle = "#000";
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(cx - (x === 0 ? 0 : radius), cy + offset + 0.5);
    g.lineTo(cx + (x === cols - 1 ? 0 : radius) + 1, cy + offset + 0.5);
    g.moveTo(cx + offset + 0.5, cy - (y === 0 ? 0 : radius));
    g.lineTo(cx + offset + 0.5, cy + (y === rows - 1 ? 0 : radius) + 1);
    g.stroke();
  }
}

// ------------------------------------------------------------------ ownership (纯网络 H, Kata形势)

/** Black-positive ownership with the style it is painted in: 纯网络's (`rawNet`), the position's
 *  Kata形势 (`estimate`, the current node's or its parent's) or a candidate's movesOwnership. */
export interface OwnershipLayer {
  ownership: number[];
  style: EstimateStyle;
  source: "rawNet" | "estimate" | "candidate";
}

/** BoardRenderer.drawScore (BoardRenderer.java:4765-4796): a red dot on neutral and 不计入 points,
 *  a square of the owner's colour on every other point, stones included. */
function drawScoreMarks(g: CanvasRenderingContext2D, score: ScoreView, X: (x: number) => number, Y: (y: number) => number, sq: number, cols: number): { neutral: number; black: number; white: number } {
  const counts = { neutral: 0, black: 0, white: 0 };
  const dot = i((sq * 4) / 10), box = i((sq * 6) / 10);
  score.points.forEach((p, k) => {
    const x = X(k % cols), y = Y(i(k / cols));
    if (p.owner === null || p.excluded) {
      g.fillStyle = "#f00";
      circle(g, x - i((sq * 2) / 10) + dot / 2, y - i((sq * 2) / 10) + dot / 2, dot / 2);
      g.fill();
      counts.neutral++;
    } else {
      g.fillStyle = p.owner === "Black" ? "#000" : "#fff";
      g.fillRect(x - i((sq * 3) / 10), y - i((sq * 3) / 10), box, box);
      counts[p.owner === "Black" ? "black" : "white"]++;
    }
  });
  return counts;
}

/** BoardRenderer.convertLength. */
function convertLength(v: number): number {
  const a = Math.abs(v);
  return a > 0.2 ? (a * 6) / 10 : 0;
}

/** BoardRenderer / SubBoardRenderer drawKataEstimateBySize and drawKataEstimateByTransparent
 *  (BoardRenderer.java:1262-1380, SubBoardRenderer.java:594-706) for a Black-positive ownership.
 *  Java hands them side-to-move-positive values with the side to move; both say Black owns a point
 *  exactly when the Black-positive value is positive. "Small" uses the small squares Java drew
 *  when the KataGo estimate's own small option was set (an objective fix: the pure-network choice
 *  decides); the sub board has no small squares in Java. */
function drawOwnership(g: CanvasRenderingContext2D, own: number[], stones: (Stone | null)[], cols: number, style: EstimateStyle, X: (x: number) => number, Y: (y: number) => number, sq: number, main: boolean) {
  const big = style === "BigBelow";
  for (let k = 0; k < own.length; k++) {
    const o = own[k];
    if (!o) continue;
    const black = o > 0;
    const x = X(k % cols), y = Y(i(k / cols));
    if (style === "BySize") {
      const length = i(convertLength(o) * sq);
      g.fillStyle = black ? "rgba(0,0,0,0.706)" : "rgba(255,255,255,0.706)";
      if (length > 0) g.fillRect(x - i(length / 2), y - i(length / 2), length, length);
      continue;
    }
    if (!big && style === "TransparentNotOnLive" && stones[k] === (black ? "Black" : "White")) continue;
    const alpha = Math.abs(i(o * (big ? (black ? 105 : 165) : 255)));
    if (black && !big && stones[k] === "Black") {
      const v = 127 - i((alpha - 1) / 2);
      g.fillStyle = `rgb(${v},${v},${v})`;
    } else g.fillStyle = black ? `rgba(0,0,0,${alpha / 255})` : `rgba(255,255,255,${alpha / 255})`;
    if (big) g.fillRect(x - i((sq * 5) / 10), y - i((sq * 5) / 10), sq, sq);
    else if (main && style === "Small") {
      const length = Math.round(sq / (black ? 8 : 6));
      g.fillRect(x - length, y - length, length * 2, length * 2);
    } else g.fillRect(x - i(sq / 4), y - i(sq / 4), i(sq / 2), i(sq / 2));
  }
}

// ------------------------------------------------------------------ main board

/** drawStringForOrder (BoardRenderer.java:4291-4333): the rank badge at the upper right, black on
 *  orange; with white-suggestion-order-white while White plays, white on dark gold. */
function orderBadge(g: CanvasRenderingContext2D, x: number, y: number, order: number, sq: number, white: boolean) {
  const bx = Math.round(x + sq * 0.43) + (order === 0 ? 1 : 0);
  const by = Math.round(y - sq * 0.358) - (order === 0 ? 1 : 0);
  const label = String(order + 1);
  const weight = weightOf(WINRATE_FONT);
  g.font = `${weight}100px ${WINRATE_FONT}`;
  const size = Math.min(sq * 0.36, Math.round((100 * sq * 0.39) / Math.max(1, Math.round(g.measureText(label).width))));
  g.font = `${weight}${size}px ${WINRATE_FONT}`;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const fm = javaMetrics(g);
  const height = fm.ascent - fm.descent;
  const width = Math.round(g.measureText(label).width);
  const left = bx - i(width / 2);
  g.fillStyle = white ? "rgb(155,118,36)" : "#ffc800"; // java.awt.Color.ORANGE
  g.fillRect(left, by - height, width, height + Math.max(1, i(height / 12)));
  g.fillStyle = white ? "#fff" : "#000";
  g.fillText(label, left, by);
}

/** drawOrder's colour choice: the white badge while White plays (blackToPlay false). */
function whiteBadges(s: BoardScene): boolean {
  return s.display.whiteSuggestionOrderWhite && s.toPlay === "White";
}

/** BoardRenderer.drawStringFor3row: Java's width-limited font, baseline on y. */
function suggestionText(g: CanvasRenderingContext2D, s: string, x: number, y: number, maxH: number, maxW: number, family: string) {
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const weight = weightOf(family);
  g.font = `${weight}100px ${family}`;
  const size = Math.min(maxH, Math.round((100 * maxW) / Math.max(1, Math.round(g.measureText(s).width))));
  g.font = `${weight}${size}px ${family}`;
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y);
}

/** drawSuggestionText / drawTwoRowSuggestionField (BoardRenderer.java:2747-2913): the shown fields
 *  (win rate 0, playouts 1, score 2) in the rows of 自定义信息顺序. */
function drawSuggestionText(g: CanvasRenderingContext2D, geo: BoardGeom, x: number, y: number, texts: string[], colors: string[], fields: number[], defaultOrder: boolean, roundedWinrate: number, visits: number) {
  const r = geo.stoneRadius, sq = geo.square;
  const font = (field: number) => (field === 1 ? PLAYOUTS_FONT : WINRATE_FONT);
  if (fields.length === 3) {
    fields.forEach((field, row) => {
      g.fillStyle = colors[field];
      if (row === 0) {
        const narrow = defaultOrder && field === 0 && roundedWinrate < 10;
        suggestionText(g, texts[field], x, y - Math.round(sq * (narrow ? 0.127 : 0.125)), sq * (narrow ? 0.36 : 0.35), sq * 0.67, font(field));
      } else if (row === 1) {
        const defaultPlayouts = defaultOrder && field === 1;
        suggestionText(g, texts[field], x, y + Math.round(sq * 0.18), sq * (defaultPlayouts ? 0.34 : 0.32), defaultPlayouts && visits < 1000 ? r * 1.3 : r * 1.8, font(field));
      } else {
        suggestionText(g, texts[field], x, y + Math.round(sq * 0.435), ((sq * (geo.n - 1) + 1) * 0.273) / (geo.n - 1), r * 1.6, font(field));
      }
    });
  } else if (fields.length === 2) {
    for (const [field, other, first] of [[fields[0], fields[1], true], [fields[1], fields[0], false]] as const) {
      g.fillStyle = colors[field];
      if (!first)
        jString(g, texts[field], x, y + (field === 2 ? i((r * 4) / 9) : i((r * 15) / 35)), font(field), field === 2 ? r * 0.75 : r * 0.77, field === 2 ? r * 1.6 : r * 1.8);
      else if (field === 0)
        jString(g, texts[field], x, y - i(sq / (roundedWinrate < 10 ? 15 : 16)), font(field), r, sq * (roundedWinrate < 10 ? 0.57 : 0.735), 1);
      else if (field === 1) {
        const above = other === 0;
        jString(g, texts[field], x, y - (above ? i(sq / 15) : i(r / 15)), font(field), r * (above ? 0.77 : 0.82), r * (above ? 1.8 : 1.73), 1);
      } else jString(g, texts[field], x, y - i(sq / 16), font(field), r * 0.88, sq * 0.735, 1);
    }
  } else if (fields.length === 1) {
    const field = fields[0];
    g.fillStyle = colors[field];
    jString(g, texts[field], x, y, font(field), field === 0 ? sq * 0.46 : r, field === 2 ? r * 1.7 : r * 1.9);
  }
}

interface ShownCandidate {
  c: CandidateView;
  k: number;
  x: number;
  y: number;
  ratio: number;
  fraction: number;
  hovered: boolean;
  needShow: boolean;
  skipNumbers: boolean;
  /** The unimportant layer already drew its circle (hasDrawBackground). */
  background: boolean;
}

/** drawLeelazSuggestions' hue fraction: the allocation share raised to 1 / suggestion-color-ratio
 *  (BoardRenderer.java:2532-2536, 3127-3131). */
function hueFraction(ratio: number, s: BoardScene): number {
  return Math.pow(ratio, 1 / Math.max(1, s.display.suggestionColorRatio));
}

/** Rank beyond 最大候选点数 (`outOfOrder`), unless kept there (`lastTimeUnlimited`). */
function outOfOrder(c: CandidateView, s: BoardScene): boolean {
  return s.suggestionLimit > 0 && c.order + 1 > s.suggestionLimit && !(c.point && s.unlimited.has(`${c.point.x},${c.point.y}`));
}

const samePoint = (a: Point | null | undefined, b: Point | null | undefined): boolean => !!a && !!b && a.x === b.x && a.y === b.y;

/** The candidate drawn as hovered: the hovered rank, or the candidate on the 下一手 详细 point
 *  while its variation shows (the selection's coordinate, BoardRenderer.java:2507-2513). */
function isHovered(c: CandidateView, s: BoardScene): boolean {
  return s.hovered === c.order || (s.preview?.order === -1 && samePoint(c.point, s.overlay.next_move?.point));
}

/** drawLeelazSuggestionsUnimportant (BoardRenderer.java:3057-3172): candidates of rank 20 and
 *  beyond, and with show-nosugg-circle those beyond 最大候选点数, get a plain disc and ring without
 *  text, except the actual next move and a 优先分析此点 point (3108-3119). Returns the points
 *  covered (hasDrawBackground); draws them only when `draw` (no branch shows). */
function drawUnimportant(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, draw: boolean, report: BoardReport): Set<number> {
  const covered = new Set<number>();
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const r = geo.stoneRadius;
  for (let k = candidates.length - 1; k >= 0; k--) {
    const c = candidates[k], pt = c.point;
    if (!pt || c.visits <= 0 || pt.x >= s.cols || pt.y >= s.rows || pt.x < 0 || pt.y < 0) continue;
    const beyond = outOfOrder(c, s);
    if (!beyond && c.order < 20) continue;
    if (samePoint(pt, s.nextMainMove) || s.priority.some((p) => samePoint(p, pt))) continue;
    if (!s.display.noSuggCircle && beyond) continue;
    covered.add(pt.y * s.cols + pt.x);
    if (!draw) continue;
    const ratio = max > 0 ? Math.min(1, c.allocation_visits / max) : 0;
    const alphaRatio = Math.max(0, Math.log(ratio) / 5 + 1);
    g.fillStyle = hsb(k === 0 ? look.bestHue : RED_HUE + (GREEN_HUE - RED_HUE) * hueFraction(ratio, s), 1, 0.85, i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * alphaRatio));
    fillCircle(g, X(pt.x), Y(pt.y), r + 1);
    g.strokeStyle = `rgba(0,0,0,${i(48 + 48 * alphaRatio) / 255})`;
    strokeCircle(g, X(pt.x), Y(pt.y), r + 1, (r + 1) / 26.5);
    report.unimportant++;
  }
  return covered;
}

/** Candidates drawLeelazSuggestions (BoardRenderer.java:2437-2632) shows, worst first; the move
 *  played and analysed further than its candidate (`ignored`) is returned apart. Without
 *  show-nosugg-circle the candidates beyond 最大候选点数 are not kept at all, except the one
 *  lastTimeUnlimited keeps (BoardData.tryToLimitMoves, BoardData.java:586-635) and a 优先分析此点
 *  point (619-628); with it, those under an unimportant circle show nothing more unless hovered
 *  (2523-2524). The actual next move and a 优先分析此点 point (needShow) and the hovered one keep
 *  their numbers (2547-2553, 2571-2572). */
function shownCandidates(s: BoardScene, X: (x: number) => number, Y: (y: number) => number, ignored: Point | null, covered: Set<number>): { shown: ShownCandidate[]; ignoredCandidate: CandidateView | null } {
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(1, ...candidates.map((c) => c.allocation_visits));
  const shown: ShownCandidate[] = [];
  let ignoredCandidate: CandidateView | null = null;
  for (let k = candidates.length - 1; k >= 0; k--) {
    const c = candidates[k], pt = c.point;
    if (!pt || c.visits <= 0 || pt.x >= s.cols || pt.y >= s.rows || pt.x < 0 || pt.y < 0) continue;
    const priority = s.priority.some((p) => samePoint(p, pt));
    const beyond = outOfOrder(c, s) && !priority;
    if (beyond && !s.display.noSuggCircle) continue;
    if (samePoint(ignored, pt)) {
      ignoredCandidate = c;
      continue;
    }
    const hovered = isHovered(c, s);
    const background = covered.has(pt.y * s.cols + pt.x);
    if (beyond && !hovered && background) continue;
    const ratio = Math.min(1, c.allocation_visits / max);
    const needShow = samePoint(s.nextMainMove, pt) || priority;
    const lack = ratio <= s.display.minPlayoutRatio;
    shown.push({
      c,
      k,
      x: X(pt.x),
      y: Y(pt.y),
      ratio,
      fraction: k === 0 ? 0 : hueFraction(ratio, s),
      hovered,
      needShow,
      skipNumbers: (beyond || lack) && !hovered && k !== 0 && !needShow,
      background,
    });
  }
  return { shown, ignoredCandidate };
}

/** BoardRenderer.java:2573-2629: the wood under the colour, the HSB disc with allocation alpha and
 *  its ring: the best candidate's blue ring (show-blue-ring; dimmed on a next move) or else the
 *  faint black ring the others get, which a next move's dashed ring replaces (isOnNext). A point
 *  under an unimportant circle is not painted again unless hovered. Nothing while a branch shows. */
function drawCandidateDiscs(g: CanvasRenderingContext2D, geo: BoardGeom, shown: ShownCandidate[], next: Point[], s: BoardScene, report: BoardReport) {
  const r = geo.stoneRadius;
  for (const { c, k, x, y, ratio, fraction, hovered, needShow, skipNumbers, background } of shown) {
    if (background && !hovered) continue;
    if (!skipNumbers) woodDisc(g, x, y, r + 1, ratio < 0.05 && !hovered && !needShow ? 0.8 : 1);
    const onNext = next.some((n) => n.x === c.point!.x && n.y === c.point!.y);
    const alphaRatio = Math.max(0, Math.log(ratio) / 5 + 1);
    g.fillStyle = hsb(k === 0 ? look.bestHue : RED_HUE + (GREEN_HUE - RED_HUE) * fraction, 1, 0.85, i(MIN_ALPHA + (MAX_ALPHA - MIN_ALPHA) * alphaRatio));
    fillCircle(g, x, y, r + 1);
    if (k === 0 && s.display.blueRing) {
      g.strokeStyle = onNext ? "rgba(0,0,255,0.667)" : "#00f";
      strokeCircle(g, x, y, r + 2, (r + 2) / 15);
      report.bestRing = "blue";
    } else if (!onNext) {
      g.strokeStyle = `rgba(0,0,0,${i(48 + 48 * alphaRatio) / 255})`;
      strokeCircle(g, x, y, r + 1, (r + 1) / 26.5);
      if (k === 0) report.bestRing = "plain";
    } else if (k === 0) report.bestRing = "plain";
  }
}

/** BoardRenderer.java:2555-2569, 2630-2712: badges and information rows. While the 下一手 详细
 *  variation shows, the candidate on that point keeps only its badge (isMouseOverNextBlunder). */
function drawCandidateLabels(g: CanvasRenderingContext2D, geo: BoardGeom, shown: ShownCandidate[], s: BoardScene, branch: boolean, hideHoverInfo: boolean, hoverStoneBlack: boolean, report: BoardReport) {
  const d = s.display, sq = geo.square, r = geo.stoneRadius;
  const candidates = s.analysis?.candidates ?? [];
  const maxAlloc = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const maxWinrate = Math.max(0, ...candidates.map((c) => c.winrate));
  let maxScore = -300;
  if (d.suggestionScore) for (const c of candidates) if (c.score_lead !== null && c.score_lead > maxScore) maxScore = c.score_lead;
  const defaultOrder = d.suggestionInfoOrder[0] === 1 && d.suggestionInfoOrder[1] === 2 && d.suggestionInfoOrder[2] === 3;
  const white = whiteBadges(s);
  const nextHover = s.preview?.order === -1;
  for (const { c, k, x, y, fraction, hovered, skipNumbers } of shown) {
    if ((!branch || hovered) && d.suggestionOrder && c.order === 0) orderBadge(g, x, y, 0, sq, white);
    if (skipNumbers || (branch && !hovered)) continue;
    if (d.suggestionOrder && c.order < 9 && c.order > 0) orderBadge(g, x, y, c.order, sq, white);
    if (nextHover) continue;
    const badge = d.suggestionOrder && c.order < 9 ? String(c.order + 1) : null;
    if (hovered && branch) {
      g.strokeStyle = "#f00";
      strokeCircle(g, x, y, r + 1, (r + 1) / 11);
      if (hideHoverInfo) continue;
    }
    let normal = d.whiteSuggestionWhite && s.toPlay === "White" ? "#fff" : "#000";
    if (branch) normal = hoverStoneBlack ? "#fff" : "#000";
    // Config.bestColor, the reverse of best-move-color (BoardRenderer.java:2686-2689).
    const maxColor = k === 0 ? look.bestColor : fraction > 0.375 ? "#f00" : "rgb(100,255,235)";
    const canMax = d.suggestionMaxRed && !(hovered && branch);
    // 总是显示黑胜率 / 目差显示 are applied by the core (`shown`).
    const roundedWinrate = Math.round(c.shown.winrate * 10) / 10;
    const texts = [
      roundedWinrate.toFixed(1),
      playouts(c.visits),
      c.score_lead === null ? "" : d.scoreAsDiff ? scoreDiff(c.score_lead - maxScore) : round1(c.shown.score_lead ?? c.score_lead),
    ];
    const colors = [
      canMax && c.winrate === maxWinrate ? maxColor : normal,
      canMax && c.allocation_visits === maxAlloc ? maxColor : normal,
      canMax && c.score_lead === maxScore ? maxColor : normal,
    ];
    const fields: number[] = [];
    if (d.suggestionWinrate) fields.push(0);
    if (d.suggestionPlayouts) fields.push(1);
    if (d.suggestionScore && c.score_lead !== null) fields.push(2);
    if (!defaultOrder) fields.sort((a, b) => d.suggestionInfoOrder[a] - d.suggestionInfoOrder[b]);
    drawSuggestionText(g, geo, x, y, texts, colors, fields, defaultOrder, roundedWinrate, c.visits);
    report.candidates.push({ x: c.point!.x, y: c.point!.y, texts: fields.map((f) => texts[f]), badge });
  }
}

/** 显示策略网络 (BoardRenderer.java:2362-2421): every candidate's prior, best cyan, the others red to
 *  green by their share of the largest prior. */
function drawPolicy(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const r = geo.stoneRadius;
  const candidates = s.analysis?.candidates ?? [];
  const max = Math.max(0, ...candidates.map((c) => c.prior));
  if (max <= 0) return;
  for (const c of candidates) {
    if (!c.point) continue;
    const x = X(c.point.x), y = Y(c.point.y);
    const percent = c.prior / max;
    ghostShadow(g, x, y, r);
    g.fillStyle = hsb(c.prior === max ? look.bestHue : RED_HUE + (GREEN_HUE - RED_HUE) * percent, 1, 0.85, suggestionAlpha(percent));
    fillCircle(g, x, y, r);
    const text = c.prior.toFixed(1);
    g.fillStyle = "#fff";
    jString(g, text, x, y, WINRATE_FONT, r, r * 1.9);
    report.candidates.push({ x: c.point.x, y: c.point.y, texts: [text], badge: null });
  }
}

/** 纯网络 (BoardRenderer.java:2232-2360): heatcount = (int)(policy * 1000) on every empty point with
 *  a positive count, best cyan, others red to green; in the ownership state only the numbers,
 *  coloured by their share. */
function drawRawNetPolicy(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const policy = s.rawNet?.policy;
  if (!policy) return;
  const r = geo.stoneRadius;
  const owner = s.katago && s.rawNet?.mode === "Ownership";
  const heat = policy.map((p) => i(p * 1000));
  const max = Math.max(0, ...heat);
  for (let k = 0; k < heat.length; k++) {
    if (heat[k] <= 0 || s.stones[k]) continue;
    const x = X(k % s.cols), y = Y(i(k / s.cols));
    const percent = heat[k] / max;
    const best = heat[k] === max;
    if (!owner) {
      ghostShadow(g, x, y, r);
      g.fillStyle = hsb(best ? look.bestHue : RED_HUE + (GREEN_HUE - RED_HUE) * percent, 1, 0.85, suggestionAlpha(percent));
      fillCircle(g, x, y, r);
    }
    const text = (heat[k] / 10).toFixed(1);
    g.fillStyle = !owner ? "#fff" : best ? "rgb(255,0,0)" : percent >= 0.3 ? "rgb(0,120,255)" : "rgb(0,235,0)";
    if (owner && percent < 0.3) jString(g, text, x, y, WINRATE_FONT, i((r * 4) / 5), r * 1.55);
    else jString(g, text, x, y, WINRATE_FONT, r, r * 1.9);
    report.rawNetPoints++;
    report.candidates.push({ x: k % s.cols, y: i(k / s.cols), texts: [text], badge: null });
  }
}

/** The 下一手 详细 numbers centred on (x, y) in the current fill (drawNextMoveBlunder,
 *  BoardRenderer.java:3487-3618; drawNextBlunderFirstMove 3404-3485). Returns the drawn texts. */
function nextMoveTexts(g: CanvasRenderingContext2D, s: BoardScene, sq: number, x: number, y: number): string[] {
  const info = s.overlay.next_move!;
  const d = s.display;
  const wr = info.winrate.toFixed(1);
  if (info.score_lead !== null) {
    // The mover's own score: the best candidate's plus the difference to it.
    const mover = (info.score_diff ?? 0) + (s.analysis?.candidates[0]?.score_lead ?? 0);
    if (d.suggestionWinrate && d.suggestionScore) {
      const sc = d.scoreAsDiff && info.score_diff !== null ? scoreDiff(info.score_diff) : round1(info.score_lead);
      const winrateFirst = d.suggestionInfoOrder[0] <= d.suggestionInfoOrder[2];
      const [top, bottom] = winrateFirst ? [wr, sc] : [sc, wr];
      const below = winrateFirst && mover > -10 ? i((sq * 11) / 30) : i(sq * 0.34);
      jString(g, top, x, y, WINRATE_FONT, sq * 0.39, sq * 0.815, 1);
      jString(g, bottom, x, y + below, WINRATE_FONT, sq * 0.3, sq * 0.65, 1);
      return [top, bottom];
    }
    if (d.suggestionScore) {
      const sc = mover.toFixed(1);
      jString(g, sc, x, y, WINRATE_FONT, sq * 0.4, sq * 0.835);
      return [sc];
    }
  }
  if (!d.suggestionWinrate) return [];
  jString(g, wr, x, y, WINRATE_FONT, sq * 0.4, sq * 0.835);
  return [wr];
}

/** 下一手 详细 (BoardRenderer.java:3264-3321, 3487-3618): the move's result inside its ring. */
function drawNextMoveInfo(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const info = s.overlay.next_move!;
  const r = geo.stoneRadius;
  const x = X(info.point.x), y = Y(info.point.y);
  const fill = info.verdict === "Bad" ? "rgba(220,80,30,0.333)" : info.verdict === "Questionable" ? "rgb(220,150,30)" : "rgb(0,220,0)";
  if (info.verdict === "Bad") woodDisc(g, x, y, r + 1, 0.8);
  g.fillStyle = fill;
  fillCircle(g, x, y, r + 1);
  g.strokeStyle = s.nextColor === "White" ? "#fff" : "#000";
  strokeCircle(g, x, y, r + 2, Math.max(r / 7, 2));
  g.fillStyle = info.verdict === "Good" ? "#000" : "#fff";
  const texts = nextMoveTexts(g, s, geo.square, x, y);
  report.nextMove = { x: info.point.x, y: info.point.y, texts, fill };
}

/** drawNextBlunderFirstMove (BoardRenderer.java:3404-3485): while the 下一手 详细 variation shows,
 *  the move itself as a stone of the side to play with its numbers in the opposite colour; `shadow`
 *  false when its shadow is already drawn. */
function drawNextFirstMove(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport, shadow: boolean) {
  const info = s.overlay.next_move!;
  const x = X(info.point.x), y = Y(info.point.y);
  const black = s.toPlay === "Black";
  drawStone(g, x, y, geo.stoneRadius, black, info.point.x, info.point.y, shadow);
  g.fillStyle = black ? "#fff" : "#000";
  const texts = nextMoveTexts(g, s, geo.square, x, y);
  report.nextMove = { x: info.point.x, y: info.point.y, texts, fill: black ? "#000" : "#fff" };
}

/** BoardRenderer.drawNextMoveOutlinesOnTop + drawDashedCircle: radius r + 2, first ring
 * max(r/5.5, 3) wide, others max(r/11, 2), dash {2.2w, 1.7w}, round caps, colour of the first next
 * move. */
function drawNextMoveOutlines(g: CanvasRenderingContext2D, next: Point[], color: Stone, X: (x: number) => number, Y: (y: number) => number, r: number) {
  g.strokeStyle = color === "Black" ? "#000" : "#fff";
  g.lineCap = "round";
  next.forEach((pt, k) => {
    const w = Math.max(k === 0 ? Math.max(r / 5.5, 3) : Math.max(r / 11, 2), 1);
    g.lineWidth = w;
    g.setLineDash([w * 2.2, w * 1.7]);
    circle(g, X(pt.x), Y(pt.y), r + 2);
    g.stroke();
  });
  g.setLineDash([]);
  g.lineCap = "butt";
}

/** BoardRenderer.drawRawWinrate (BoardRenderer.java:747-790): the pure network's line under the
 *  board. Before the first answer for this position nothing stale is shown. */
function rawNetLine(s: BoardScene): string {
  const net = s.rawNet;
  if (net && net.whiteWinrate !== null) {
    if (s.katago)
      return `${t("BoardRenderer.pureNetWhiteWinrate")}${(net.whiteWinrate * 100).toFixed(1)} ${t("BoardRenderer.whiteScore")}${(net.whiteLead ?? 0).toFixed(1)} ${t("BoardRenderer.symmetry")}${net.symmetry ?? 0}`;
    // Leela Zero's `heatmap` win rate is the side to move's.
    const own = s.toPlay === "White" ? net.whiteWinrate : 1 - net.whiteWinrate;
    return `${t("BoardRenderer.pureNetWinrate")}${(own * 100).toFixed(1)}`;
  }
  return t("BoardRenderer.noPureNetWinrate");
}

/** What 棋盘同步工具's float board (FloatBoardRenderer) draws apart from the main board. */
export interface FloatLayers {
  /** FloatBoard.boardType (0 野狐, 1 弈城, 2 新浪, others): places the captured-point cross. */
  boardType: number;
  /** Lizzie.board.isSetupMode(): only the goban and the stones (FloatBoardRenderer.java:133-139). */
  setup: boolean;
  /** FloatBoard.editMode: outside it a variation shows only its own new stones over the target's
   *  board (VariationPreviewGenerator.drawStones, VariationPreviewGenerator.java:192-200). */
  edit: boolean;
}

/** `board` / `stones`: the goban (wood, grid, star points, coordinates) and the resting stones;
 *  `float`: 棋盘同步工具's float board, which leaves both out outside its edit mode
 *  (FloatBoardRenderer.draw, renderImages). */
export interface BoardLayers {
  board: boolean;
  stones: boolean;
  float?: FloatLayers;
}

/** Main board: BoardRenderer.draw (BoardRenderer.java:277-430): goban and coordinates, the line
 *  under the board, stones, ownership, 落子评价 / move numbers / last-move mark, markup, 下一手,
 *  candidates (statistics, 策略网络 or 纯网络) and the next-move rings, less what `layers` leaves
 *  out. */
export function paintBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, s: BoardScene, layers: BoardLayers = { board: true, stones: true }): BoardReport {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  const cols = s.cols;
  const d = s.display;
  const sizeY = geo.sizeY ?? geo.size;
  const marginY = geo.marginY ?? geo.margin;
  const squareY = geo.squareY ?? sq;
  const X = (x: number) => ox + geo.margin + sq * x;
  const Y = (y: number) => oy + marginY + squareY * y;
  const px = (p: number) => X(p % cols);
  const py = (p: number) => Y(i(p / cols));
  const report: BoardReport = {
    coordinates: d.coordinates,
    bottomLine: null,
    names: null,
    rawNetInfo: null,
    moveNumbers: [],
    lastMoveMarker: null,
    ranks: [],
    rankCorner: false,
    candidateMode: "hidden",
    candidates: [],
    rawNetPoints: 0,
    ownership: null,
    nextMove: null,
    preview: null,
    coordinateLabels: null,
    coordinateText: [],
    bestRing: null,
    unimportant: 0,
    whiteOrderBadges: false,
    pvVisits: [],
    estimate: null,
    score: null,
    mark: null,
    region: null,
    selecting: null,
    tracked: [],
    moveRect: null,
    review: null,
    coach: null,
  };
  // drawWoodenBoard (BoardRenderer.java:3620-3651): a custom board image gets a (0,0,0,50) wash.
  Object.assign(surface, { x: ox, y: oy, size: geo.size, image: look.board, custom: look.customBoard });
  // The pure network's line replaces the names (BoardRenderer.java:303-311).
  const rawLine = s.rawNetOn || (s.policy && s.engine && !s.katago);
  if (layers.board) {
    boardFill(g, ox, oy, geo.size, sizeY);
    if (look.customBoard && !look.pureBoard) {
      g.fillStyle = "rgba(0,0,0,0.196)";
      g.fillRect(ox, oy, geo.size, sizeY);
    }
    const avail = geo.size - 2 * geo.margin;
    grid(g, geo, ox, oy, Math.max(geo.size > 560 ? 2 : 1, avail / 481), Math.max(1, avail / 750));
    // drawStarPoints (BoardRenderer.java:1119-1143): 中式经典 corner markers, else the star points
    // of radius (int)(0.015*size)/2.
    if (look.chinese) drawChineseClassicMarkers(g, geo.cols, geo.rows, sq, (x) => X(x) + 0.5, (y) => Y(y) + 0.5);
    else {
      const star = i(i(0.015 * geo.size) / 2);
      g.fillStyle = "#000";
      for (const [a, b] of hoshi(geo.cols, geo.rows)) {
        circle(g, X(a) + 0.5, Y(b) + 0.5, star + 1);
        g.fill();
      }
    }
    // Coordinates (BoardRenderer.java:1038-1100) in the 坐标格式, or 中式经典's line numbers from the
    // nearer edge (BoardStylePainter.chineseClassicCoordinateLabel); the bottom row gives way to the
    // names and the line. 棋盘坐标 scales their limits and may make them bold (drawCoordinate).
    if (d.coordinates) {
      g.fillStyle = "#000";
      const at = i((geo.margin * 4) / 10);
      const atY = i((marginY * 4) / 10);
      const bottom = !s.names && !rawLine;
      const labels = look.chinese
        ? { cols: Array.from({ length: geo.cols }, (_, k) => chineseClassicLabel(k, geo.cols)), rows: Array.from({ length: geo.rows }, (_, k) => chineseClassicLabel(k, geo.rows)) }
        : coordinateLabels(d.coordinateStyle, geo.cols, geo.rows);
      const maxH = i((r * 4) / 5) * s.labels.scale;
      const maxW = r * s.labels.scale;
      const label = (text: string, x: number, y: number) => report.coordinateText.push(drawCoordinate(g, text, x, y, maxH, maxW, s.labels.bold));
      labels.cols.forEach((text, k) => {
        label(text, X(k), oy + atY);
        if (bottom) label(text, X(k), oy + sizeY - atY);
      });
      labels.rows.forEach((text, k) => {
        label(text, ox + at, Y(k));
        label(text, ox + geo.size - at, Y(k));
      });
      report.coordinateLabels = labels;
    }
  }
  if (rawLine) {
    const text = rawNetLine(s);
    g.fillStyle = "#000";
    // Font(Lizzie.config.fontName, PLAIN, 16) (BoardRenderer.java:786).
    jString(g, text, ox + i(geo.size / 2), oy - i((marginY * 13) / 40) + sizeY, PLAYOUTS_FONT, i(marginY / 2), geo.size, 0, true);
    report.bottomLine = "rawNet";
    report.rawNetInfo = text;
  } else if (s.names) {
    report.names = drawName(g, s, ox, oy, geo);
    report.bottomLine = "names";
  }
  const layer = s.ownership;
  // shouldShowCountBlockBelow: the big squares go under the stones.
  const below = layer?.style === "BigBelow";
  if (layer?.source === "rawNet") report.ownership = layer.style;
  else if (layer) report.estimate = { style: layer.style, source: layer.source };
  if (layer && below) drawOwnership(g, layer.ownership, s.stones, cols, layer.style, X, Y, sq, true);
  const preview = s.preview ? playVariation(s.stones, s.toPlay, cols, s.preview, s.preview.length, d.removeDeadInVariation) : null;
  const stones = preview?.stones ?? s.stones;
  /** The points whose 立体棋子 shadow the stone pass already drew under every stone. */
  const shadowed = new Set<number>();
  if (layers.stones) {
    const onlyNew = layers.float && !layers.float.edit && !layers.float.setup ? preview?.numbers ?? null : null;
    for (let p = 0; p < stones.length; p++) if (stones[p] && (!onlyNew || onlyNew.has(p))) shadowed.add(p);
    // 立体棋子's soft shadows reach past the stone: all of them go under every stone. An empty board
    // has none, so it neither builds nor composites the layer.
    if (look.enhanced && look.showShadow && shadowed.size) enhancedShadows(g, [...shadowed], `${ox},${oy},${geo.margin},${marginY},${sq},${squareY},${cols}`, px, py, r);
    for (const p of shadowed) drawStone(g, px(p), py(p), r, stones[p] === "Black", p % cols, i(p / cols), !look.enhanced);
  }
  // drawmoveblock (BoardRenderer.java:1390-1408), composited after the stones and any shown
  // variation (renderImages 1880): a square 46% of the grid wide in the colour to play, on a point
  // empty in the current position.
  if (s.moveRect && !s.review && !s.stones[s.moveRect.point.y * cols + s.moveRect.point.x]) {
    const half = i((sq * 23) / 100);
    g.fillStyle = s.moveRect.black ? "#000" : "#fff";
    g.fillRect(X(s.moveRect.point.x) - half, Y(s.moveRect.point.y) - half, i((sq * 46) / 100), i((sq * 46) / 100));
    report.moveRect = s.moveRect.point;
  }
  if (layers.float?.setup) return report;
  if (layer && !below) drawOwnership(g, layer.ownership, s.stones, cols, layer.style, X, Y, sq, true);
  // renderImages draws the selection images after Kata形势 (BoardRenderer.java:1884-1887).
  drawRegion(g, s, X, Y, sq, report);
  // renderImages ends with the score image; the move numbers still follow it.
  if (s.score) report.score = drawScoreMarks(g, s.score, X, Y, sq, cols);
  const nextHover = s.preview?.order === -1 && s.overlay.next_move ? s.overlay.next_move.point : null;
  const hoverPoint = nextHover ?? (s.preview ? s.analysis?.candidates.find((c) => c.order === s.preview!.order)?.point ?? null : null);
  const hoverIndex = hoverPoint ? hoverPoint.y * cols + hoverPoint.x : -1;
  // A review draws its variation's captured stones itself (drawBranchForMouseOnStone).
  if (preview && !s.review) {
    if (layers.float) drawFloatCapturedPoints(g, preview, px, py, r, cols, s.rows, hoverIndex, layers.float.boardType);
    else drawPreviewStones(g, preview, px, py, r);
  }
  if (s.review) {
    drawReviewOverlay(g, s, geo, X, Y, preview, report);
    return report;
  }
  const hideHoverInfo = layers.float ? drawFloatMoveOverlay(g, s, geo, X, Y, preview, hoverIndex, report) : drawMoveOverlay(g, s, geo, ox, oy, X, Y, preview, hoverIndex, report);
  if (preview) report.preview = { order: s.preview!.order, length: s.preview!.length, numbers: preview.numbers.size };
  // While scoring no markup, next moves or candidates are drawn (BoardRenderer.java:363-364).
  if (s.score) return report;
  drawMarks(g, s, px, py, r);
  const branch = !!preview;
  const showNext = d.nextMoves && !branch && !!s.nextColor;
  // 下一手 详细 only beside the statistics (BoardRenderer.java:3214-3221).
  const nextInfo = showNext && s.candidatesShown && !s.policy && !s.rawNetOn ? s.overlay.next_move : null;
  if (nextInfo) drawNextMoveInfo(g, s, geo, X, Y, report);
  const next = showNext ? s.nextMoves : [];
  if (s.candidatesShown && s.rawNetOn) {
    report.candidateMode = "rawNet";
    if (!branch) drawRawNetPolicy(g, s, geo, X, Y, report);
  } else if (s.candidatesShown && s.policy) {
    report.candidateMode = "policy";
    if (!branch) drawPolicy(g, s, geo, X, Y, report);
  } else if (s.candidatesShown) {
    report.candidateMode = "stats";
    report.whiteOrderBadges = whiteBadges(s);
    const covered = drawUnimportant(g, s, geo, X, Y, !branch, report);
    const { shown, ignoredCandidate } = shownCandidates(s, X, Y, nextInfo?.hides_candidate ? nextInfo.point : null, covered);
    if (!branch) drawCandidateDiscs(g, geo, shown, next, s, report);
    // Java strokes the hints over finished candidates; with the rows on their Java baselines the
    // dashes would cross the score row, so the hints go between the discs and the labels.
    if (next.length) drawNextMoveOutlines(g, next, s.nextColor!, X, Y, r);
    // The move played and analysed further than its candidate keeps only its badge (2464-2494).
    if (ignoredCandidate?.point && d.suggestionOrder && ignoredCandidate.order < 9) orderBadge(g, X(ignoredCandidate.point.x), Y(ignoredCandidate.point.y), ignoredCandidate.order, sq, whiteBadges(s));
    drawCandidateLabels(g, geo, shown, s, branch, hideHoverInfo, hoverIndex >= 0 && preview?.stones[hoverIndex] === "Black", report);
  } else if (next.length) drawNextMoveOutlines(g, next, s.nextColor!, X, Y, r);
  // The variation starts with that move, so with 立体棋子 its shadow is already under the stones.
  if (nextHover) drawNextFirstMove(g, s, geo, X, Y, report, !look.enhanced || !shadowed.has(nextHover.y * cols + nextHover.x));
  // drawTrackingOverlay (BoardRenderer.java:414, 2721-2734): main board, no branch shown.
  if (!branch) drawTracking(g, s, X, Y, sq, r, report);
  // drawFocusStone (BoardRenderer.java:421-422, 1410-1416): a magenta ring of 7/6 the stone
  // radius, stroke radius / 11.5, while no branch shows.
  if (s.mark && !branch) {
    const ring = i((r * 7) / 6);
    g.strokeStyle = "#f0f";
    g.lineWidth = ring / 11.5;
    circle(g, X(s.mark.x), Y(s.mark.y), ring);
    g.stroke();
    report.mark = s.mark;
  }
  if (s.coach && (s.coach.area || s.coach.point)) {
    drawCoachMarks(g, s.coach, X, Y, sq, squareY);
    report.coach = s.coach;
  }
  return report;
}

/** CoachBoardMarks.paint, over the stones: the hint's area as a soft rounded square in the coach
 *  teal from the first third's line to the last's, and a point as a teal ring with a glow. */
function drawCoachMarks(g: CanvasRenderingContext2D, m: CoachMarks, X: (x: number) => number, Y: (y: number) => number, sq: number, sqY: number): void {
  g.save();
  if (m.area) {
    const [x0, y0, x1, y1] = m.area;
    const left = X(x0) - i(sq / 2);
    const top = Y(y0) - i(sqY / 2);
    const w = (x1 - x0 + 1) * sq;
    const h = (y1 - y0 + 1) * sqY;
    const arc = i(sq / 2) / 2;
    g.beginPath();
    g.roundRect(left, top, w, h, arc);
    g.fillStyle = "rgba(61,190,170,0.180)";
    g.fill();
    g.strokeStyle = "rgba(61,190,170,0.824)";
    g.lineWidth = Math.max(2, sq / 14);
    g.stroke();
  }
  if (m.point) {
    const radius = i((sq * 46) / 100);
    circle(g, X(m.point.x), Y(m.point.y), radius);
    g.fillStyle = "rgba(61,190,170,0.275)";
    g.fill();
    g.strokeStyle = "#3dbeaa";
    g.lineWidth = Math.max(2.5, sq / 9);
    g.stroke();
  }
  g.restore();
}

/** The review's marks over its position (BoardRenderer.java:313-326, 1465-1559): the variation's
 *  stones a later move captures are covered by the board and shaded, crossed when they were
 *  stones of the position (drawCapturedStone); every variation move keeps its number, the last
 *  one played on its point, in the contrasting colour (none red); then drawMoveRankMark or
 *  drawLastMoveMark, whose stone-indicator-type 0 is the red triangle, 1 the ring and 2 the dot;
 *  3 draws nothing. */
function drawReviewOverlay(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, preview: Preview | null, report: BoardReport): void {
  const r = geo.stoneRadius, cols = s.cols, ring = Math.round(geo.square * 0.22);
  report.review = s.review;
  if (preview && s.preview) {
    // Branch.moveNumberList: a captured move keeps its number.
    const numbers = new Map<number, number>();
    s.preview.pv.slice(0, s.preview.length).forEach((pt, k) => { if (pt) numbers.set(pt.y * cols + pt.x, k + 1); });
    for (const [p, stone] of preview.captured) {
      if (preview.stones[p]) continue;
      const x = X(p % cols), y = Y(i(p / cols)), played = numbers.has(p);
      g.save();
      g.beginPath();
      g.arc(x + 0.5, y + 0.5, r + 1.5, 0, Math.PI * 2);
      g.clip();
      drawTexture(g, surface.image, x - r - 2, y - r - 2, 2 * r + 5, 2 * r + 5, surface.x, surface.y);
      g.restore();
      if (!played) {
        const px = p % cols, py = i(p / cols);
        g.strokeStyle = "#000";
        g.lineWidth = 1;
        g.beginPath();
        g.moveTo(x - (px === 0 ? 0 : r + 1), y + 0.5);
        g.lineTo(x + (px === cols - 1 ? 0 : r + 1) + 1, y + 0.5);
        g.moveTo(x + 0.5, y - (py === 0 ? 0 : r + 1));
        g.lineTo(x + 0.5, y + (py === s.rows - 1 ? 0 : r + 1) + 1);
        g.stroke();
      }
      g.fillStyle = stone === "Black" ? `rgba(0,0,0,${(played ? 80 : 60) / 255})` : `rgba(255,255,255,${(played ? 110 : 85) / 255})`;
      circle(g, x, y, r);
      g.fill();
    }
    for (const [p, n] of numbers) {
      const stone = preview.stones[p] ?? preview.captured.get(p);
      if (!stone) continue;
      const x = X(p % cols), y = Y(i(p / cols));
      g.fillStyle = stone === "Black" ? "#fff" : "#000";
      jString(g, String(n), x, y, UI_FONT, r * 1.4, i(r * (n >= 100 ? 1.85 : 1.4)));
      report.moveNumbers.push({ x: p % cols, y: i(p / cols), text: String(n), red: false });
    }
    report.preview = { order: s.preview.order, length: s.preview.length, numbers: numbers.size };
  }
  if (s.review === "ranks") {
    drawRankMarks(g, s, geo, X, Y, preview, ring / 5, report);
    return;
  }
  const last = s.lastAction;
  if (!last?.point || s.snapshot || look.indicator === 3) return;
  const x = X(last.point.x), y = Y(last.point.y);
  const opposite = last.color === "White" ? "#000" : "#fff";
  if (look.indicator === 0) {
    g.fillStyle = "#f00";
    indicatorTriangle(g, x, y, r, true);
  } else if (look.indicator === 2) {
    g.fillStyle = opposite;
    fillCircle(g, x, y, i(r * 0.325));
  } else {
    g.strokeStyle = opposite;
    strokeCircle(g, x, y, ring, ring / 5);
  }
  report.lastMoveMarker = last.point;
}

/** drawPolygon / drawPolygonCircle (BoardRenderer.java:4184-4198): the stone-indicator-type 0
 *  triangle with Java's integer vertices, filled or stroked. */
function indicatorTriangle(g: CanvasRenderingContext2D, x: number, y: number, r: number, fill: boolean) {
  g.beginPath();
  g.moveTo(x, y - i((10 * r) / 22));
  g.lineTo(x - i(r / 2), y + i((8 * r) / 22));
  g.lineTo(x + i(r / 2), y + i((8 * r) / 22));
  g.closePath();
  if (fill) g.fill();
  else g.stroke();
}

/** drawMoveRankMark's marks (BoardRenderer.java:539-658, FloatBoardRenderer.java:926-1042): the
 *  rank discs of the moves a shown variation leaves visible, then the last move's outline in the
 *  opposite colour: with stone-indicator-type 0 the triangle stroked `triangleWidth` wide, else the
 *  ring of round(squareWidth * 0.22) stroked a fifth of it and the red corner. */
function drawRankMarks(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, preview: Preview | null, triangleWidth: number, report: BoardReport): void {
  const r = geo.stoneRadius, cols = s.cols;
  const last = s.lastAction;
  const ring = Math.round(geo.square * 0.22);
  const hiddenByBranch = (p: number) => !!preview && (!preview.stones[p] || preview.numbers.has(p));
  for (const m of s.overlay.ranks) {
    if (hiddenByBranch(m.point.y * cols + m.point.x)) continue;
    g.fillStyle = stagePalette().moveRank[m.rank];
    // drawMoveRankMarkCircle (BoardRenderer.java:644-657): the last move's mark is the triangle
    // with stone-indicator-type 0.
    if (m.last && look.indicator === 0) indicatorTriangle(g, X(m.point.x), Y(m.point.y), r, true);
    else fillCircle(g, X(m.point.x), Y(m.point.y), m.last ? ring : Math.round(geo.square * RANK_RADIUS[m.rank]));
    report.ranks.push({ x: m.point.x, y: m.point.y, rank: m.rank, last: m.last });
  }
  if (last?.point && !s.snapshot && !hiddenByBranch(last.point.y * cols + last.point.x)) {
    g.strokeStyle = last.color === "White" ? "#000" : "#fff";
    if (look.indicator === 0) {
      g.lineWidth = triangleWidth;
      indicatorTriangle(g, X(last.point.x), Y(last.point.y), r, false);
    } else {
      strokeCircle(g, X(last.point.x), Y(last.point.y), ring, ring / 5);
      if (s.overlay.rank_corner) {
        g.fillStyle = "#f00";
        smallTriangle(g, X(last.point.x), Y(last.point.y), r);
        report.rankCorner = true;
      }
    }
    report.lastMoveMarker = last.point;
  }
}

/** The float board's move overlay (FloatBoardRenderer.java:152-157): 落子评价 only while move
 *  numbers are off, 原棋盘不显示评价标记 (disable-move-rank-in-origin) is off and 落子评价 is not
 *  无, its last-move triangle outlined 1px (drawPolygonCircle under the default stroke, 977-990);
 *  then, or else, only the shown variation's numbers (drawMoveNumbers, 1057-1062). The float board
 *  has no move numbers, last-move indicator or pass disc of its own (VariationPreviewGenerator
 *  .java:318). Returns hideMouseOverInfo. */
function drawFloatMoveOverlay(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, preview: Preview | null, hoverIndex: number, report: BoardReport): boolean {
  const d = s.display, cols = s.cols;
  if (d.moveNumbers === 0 && !d.disableMoveRankInOrigin && d.moveRank >= 0) drawRankMarks(g, s, geo, X, Y, preview, 1, report);
  return preview ? drawPreviewNumbers(g, preview, (p) => X(p % cols), (p) => Y(i(p / cols)), geo, hoverIndex, cols, d, report) : false;
}

/** moveOverlayOrder (BoardRenderer.java:210-231, 337-361): 落子评价 while move numbers are off,
 *  else drawMoveNumbers; a shown variation brings its own numbers. Returns hideMouseOverInfo. */
function drawMoveOverlay(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, ox: number, oy: number, X: (x: number) => number, Y: (y: number) => number, preview: Preview | null, hoverIndex: number, report: BoardReport): boolean {
  const d = s.display, r = geo.stoneRadius, sq = geo.square, cols = s.cols;
  const last = s.lastAction;
  if (preview) {
    if (preview.pass) drawPassDisc(g, ox, oy, geo, preview.toPlay === "Black");
  } else if (last && !last.point && !s.snapshot) drawPassDisc(g, ox, oy, geo, s.toPlay === "Black");
  const ring = Math.round(sq * 0.22);
  const drawNumbers = () => {
    for (const label of s.overlay.move_numbers) {
      const x = X(label.point.x), y = Y(label.point.y);
      if (label.last) { g.fillStyle = "#f00"; smallTriangle(g, x, y, r); }
      else g.fillStyle = s.stones[label.point.y * cols + label.point.x] === "Black" ? "#fff" : "#000";
      jString(g, String(label.number), x, y, UI_FONT, r * 1.4, i(r * (label.number >= 100 ? 1.85 : 1.4)));
      report.moveNumbers.push({ x: label.point.x, y: label.point.y, text: String(label.number), red: label.last });
    }
  };
  if (s.overlay.ranks_after_numbers && !preview) drawNumbers();
  // drawMoveRankMark (BoardRenderer.java:529-658); the type-0 triangle outline is stroked a fifth
  // of the ring (590-595).
  if (s.overlay.ranks_after_numbers || (d.moveNumbers === 0 && d.moveRank >= 0)) {
    drawRankMarks(g, s, geo, X, Y, preview, ring / 5, report);
    if (!preview) return false;
  }
  if (preview) return drawPreviewNumbers(g, preview, (p) => X(p % cols), (p) => Y(i(p / cols)), geo, hoverIndex, cols, d, report);
  if (s.snapshot) return false;
  // drawMoveNumbers (BoardRenderer.java:1898-2101); the core chose the numbers. While 试下 runs
  // its stones are numbered whatever 手数 shows (1916-1918, 1974, 2066-2077).
  if (d.moveNumbers === 0 && !(d.allNumbersInBranch && !s.mainTrunk) && !s.trying) {
    // stone-indicator-type (BoardRenderer.java:1932-1953): 0 a red triangle, 1 a ring and 2 a dot
    // of (int)(r * 0.325) in the opposite colour, 3 nothing.
    if (last?.point && look.indicator !== 3) {
      const [x, y] = [X(last.point.x), Y(last.point.y)];
      const opposite = last.color === "White" ? "#000" : "#fff";
      if (look.indicator === 0) {
        g.fillStyle = "#f00";
        indicatorTriangle(g, x, y, r, true);
      } else if (look.indicator === 2) {
        g.fillStyle = opposite;
        fillCircle(g, x, y, i(r * 0.325));
      } else {
        g.strokeStyle = opposite;
        strokeCircle(g, x, y, ring, ring / 5);
      }
      report.lastMoveMarker = last.point;
    }
    return false;
  }
  drawNumbers();
  // A last move without a 试下 number (the record's own) keeps the small red triangle (1993-2005).
  if (s.trying && last?.point && !s.overlay.move_numbers.some((label) => label.last)) {
    g.fillStyle = "#f00";
    smallTriangle(g, X(last.point.x), Y(last.point.y), r);
    report.lastMoveMarker = last.point;
  }
  return false;
}

/** BoardRenderer markup: TR / SQ / CR / MA / LB, drawn in the contrasting colour of the point. */
function drawMarks(g: CanvasRenderingContext2D, scene: BoardScene, px: (p: number) => number, py: (p: number) => number, r: number) {
  const cols = scene.cols;
  g.lineWidth = Math.max(1.5, r / 10);
  for (const m of scene.marks) {
    const p = m.point.y * cols + m.point.x;
    const color = scene.stones[p] === "Black" ? "#fff" : "#000";
    const [x, y] = [px(p), py(p)];
    g.strokeStyle = color;
    switch (m.mark.kind) {
      case "Triangle":
        g.beginPath();
        g.moveTo(x, y - r * 0.55);
        g.lineTo(x - r * 0.5, y + r * 0.35);
        g.lineTo(x + r * 0.5, y + r * 0.35);
        g.closePath();
        g.stroke();
        break;
      case "Square":
        g.strokeRect(x - r * 0.4, y - r * 0.4, r * 0.8, r * 0.8);
        break;
      case "Circle":
        circle(g, x, y, r * 0.45);
        g.stroke();
        break;
      case "Cross":
        g.beginPath();
        g.moveTo(x - r * 0.4, y - r * 0.4);
        g.lineTo(x + r * 0.4, y + r * 0.4);
        g.moveTo(x + r * 0.4, y - r * 0.4);
        g.lineTo(x - r * 0.4, y + r * 0.4);
        g.stroke();
        break;
      case "Label":
        if (!scene.stones[p]) {
          // label over an empty point: the grid under it is covered with the board paint
          g.save();
          circle(g, x, y, r * 0.7);
          g.clip();
          boardFill(g, x - r, y - r, r * 2, r * 2);
          g.restore();
        }
        textMid(g, m.mark.text, x, y, r * 1.1, r * 1.6, color);
        break;
    }
  }
}

/** NameWinrates.WIDEST: the widest win rate as drawn beside a name, two spaces before it. */
const WIDEST_WINRATE = "  100.0%";

interface TextMeasure {
  width: number;
  ascent: number;
  descent: number;
}

/** Measures by font and text: the names, labels, hints and the side to move drawn on every
 *  repaint are measured once, and again only when their text or font changes (a new language,
 *  face or size is a new key). Texts that change with every analysis update (the win rates beside
 *  the names) take the room of the texts used longest ago, never of the ones each repaint uses. */
const textMeasures = new RecentCache<string, TextMeasure>(256);
function measure(g: CanvasRenderingContext2D, text: string): TextMeasure {
  const key = `${g.font}\n${text}`;
  let m = textMeasures.get(key);
  if (m === undefined) {
    const metrics = g.measureText(text);
    m = { width: metrics.width, ascent: metrics.fontBoundingBoxAscent, descent: metrics.fontBoundingBoxDescent };
    textMeasures.set(key, m);
  }
  return m;
}

function textWidth(g: CanvasRenderingContext2D, text: string): number {
  return Math.round(measure(g, text).width);
}

/** BoardRenderer.drawName (BoardRenderer.java:796-862): both name stones (the side to move drawn on
 *  top), the black name left of them and the white name right of them, both in the board's ink;
 *  the stones beside them tell the sides apart. A name wider than 0.43 of the grid is cut with
 *  an ellipsis (fitName). While the win rates show (NameWinrates) each sits next to the stones,
 *  Black's ending where Black's name ended and White's starting where White's started, and the
 *  names give up the room of the widest one (WIDEST), so a changing win rate never moves them. */
function drawName(g: CanvasRenderingContext2D, scene: BoardScene, ox: number, oy: number, geo: BoardGeom): NamesReport {
  const { size, margin, stoneRadius: r, square, n } = geo;
  g.font = `${i(Math.min(28, (margin * 53) / 100))}px ${GRAPH_FONT}`;
  const fm = javaMetrics(g);
  const rates = scene.nameWinrates;
  const slot = rates ? textWidth(g, WIDEST_WINRATE) : 0;
  const limit = i(square * n * 0.43) - slot;
  const black = fitName(g, scene.black, limit);
  const white = fitName(g, scene.white, limit);
  const d = i((r * 5) / 4);
  const top = oy - margin + r + size;
  const discs: [string, number][] = [["#000", i((r * 9) / 10)], ["#fff", i((r * 3) / 10)]];
  if (scene.toPlay === "Black") discs.reverse();
  for (const [color, dx] of discs) {
    g.fillStyle = color;
    circle(g, ox + i(size / 2) - dx + d / 2, top + d / 2, d / 2);
    g.fill();
  }
  const baseY = oy - margin + size + i((r * 13) / 8) + i((fm.ascent + fm.descent + 2) / 4);
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillStyle = "#000";
  // Black's text ends here, White's starts here.
  const blackEnd = ox + i(size / 2) - i((margin * 4) / 5);
  const whiteStart = ox + i(size / 2) + i((margin * 4) / 5) + 1;
  const box = (x: number, text: string): TextBox => ({ x, y: baseY - fm.ascent, w: textWidth(g, text), h: fm.ascent + fm.descent });
  const blackBox = black ? box(blackEnd - slot - textWidth(g, black), black) : null;
  const whiteBox = white ? box(whiteStart + slot, white) : null;
  const blackRateBox = rates ? box(blackEnd - textWidth(g, rates.black), rates.black) : null;
  const whiteRateBox = rates ? box(whiteStart, rates.white) : null;
  for (const [text, at] of [[black, blackBox], [white, whiteBox], [rates?.black, blackRateBox], [rates?.white, whiteRateBox]] as const) {
    if (text && at) g.fillText(text, at.x, baseY);
  }
  const left = Math.min(blackBox?.x ?? blackEnd, blackRateBox?.x ?? blackEnd);
  const right = Math.max(whiteBox ? whiteBox.x + whiteBox.w : whiteStart, whiteRateBox ? whiteRateBox.x + whiteRateBox.w : whiteStart);
  return { black, white, winrates: rates, box: { x: left, y: baseY - fm.ascent, w: right - left, h: fm.ascent + fm.descent }, blackBox, whiteBox, blackRateBox, whiteRateBox };
}

const ELLIPSIS = "…";
/** Grapheme boundaries: a cut never splits a letter from its marks, a surrogate pair or an emoji
 *  sequence. */
const GRAPHEMES = new Intl.Segmenter(undefined, { granularity: "grapheme" });
/** fitName's results by font, width and name: the board repaints with every analysis update, and
 *  a name is measured again only when one of them changes. */
const fittedNames = new Map<string, string>();

/** BoardNamePlate.fit (Java): `name` as it is when it fits `maxWidth` px in the current font, else
 *  shortened one character at a time from its end until it fits with "…" after it (at least one
 *  character stays), the cut text without trailing blanks. */
export function fitName(g: CanvasRenderingContext2D, name: string, maxWidth: number): string {
  const key = `${g.font}\n${maxWidth}\n${name}`;
  let shown = fittedNames.get(key);
  if (shown !== undefined) return shown;
  const width = (s: string) => Math.round(g.measureText(s).width);
  if (width(name) <= maxWidth) shown = name;
  else {
    const ends = Array.from(GRAPHEMES.segment(name), (s) => s.index + s.segment.length);
    let k = ends.length - 1;
    while (k > 0 && width(name.slice(0, ends[k]) + ELLIPSIS) > maxWidth) k--;
    shown = name.slice(0, ends[k]).trim() + ELLIPSIS;
  }
  if (fittedNames.size >= 64) fittedNames.clear();
  fittedNames.set(key, shown);
  return shown;
}

/** What paintEmptyBoardHint drew, in the board canvas's coordinates. */
export interface EmptyHintReport {
  text: string;
  lines: string[];
  size: number;
  box: Rect;
}

const WORDS = new Intl.Segmenter(undefined, { granularity: "word" });
/** The hint's lines for a text, face and room: laid out again only when one of them changes. */
let hintLayout: { key: string; lines: string[]; size: number } | null = null;

/** `text` broken between words into lines no wider than `room` px in the current font: blanks
 *  at a break are dropped and punctuation stays with the word before it. Null when one word alone
 *  is wider. */
function wrapWords(g: CanvasRenderingContext2D, text: string, room: number): string[] | null {
  const lines: string[] = [];
  let line = "";
  for (const { segment, isWordLike } of WORDS.segment(text)) {
    const blank = !segment.trim();
    if (!line) {
      if (!blank) line = segment;
      continue;
    }
    // Punctuation and blanks never start a line.
    if (!isWordLike || g.measureText(line + segment).width <= room) {
      line += segment;
      continue;
    }
    lines.push(line.trimEnd());
    line = segment;
  }
  if (line.trim()) lines.push(line.trimEnd());
  return lines.every((l) => g.measureText(l).width <= room) ? lines : null;
}

/** EmptyBoardHint.paint (Java) on the main board, painted over what paintBoard drew: `text` centred
 *  on the board at Java's max(13, board width / 48) px in the interface face and the board's ink,
 *  on a patch of the board's own paint so the grid does not cross it (as a label mark covers the
 *  grid) instead of Java's dark pill. The patch stays inside the grid, clear of the coordinates:
 *  a longer text breaks between words into at most three lines, shrinking to fit, never under
 *  9 px. Nothing on a board under 200 px (Java). */
export function paintEmptyBoardHint(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, text: string): EmptyHintReport | null {
  if (geo.size < 200) return null;
  const family = panelFont("ui");
  const grid = geo.size - 2 * geo.margin;
  const key = `${family}\n${grid}\n${geo.size}\n${text}`;
  if (hintLayout?.key !== key) {
    let size = Math.max(13, geo.size / 48);
    let lines: string[] | null = null;
    for (; size >= 9; size--) {
      g.font = `${size}px ${family}`;
      lines = wrapWords(g, text, grid - 2 * Math.round(Math.round(size * 1.4) * 0.75));
      if (lines && lines.length <= 3) break;
    }
    size = Math.max(9, size);
    hintLayout = { key, lines: lines ?? [text], size };
  }
  const { lines, size } = hintLayout;
  g.font = `${size}px ${family}`;
  const lineH = Math.round(size * 1.4);
  const padX = Math.round(lineH * 0.75);
  const w = Math.min(grid, Math.ceil(Math.max(...lines.map((l) => measure(g, l).width))) + 2 * padX);
  const h = lines.length * lineH + Math.round(lineH / 2);
  const sizeY = geo.sizeY ?? geo.size;
  const x = Math.round(geo.x - origin.x + (geo.size - w) / 2);
  const y = Math.round(geo.y - origin.y + (sizeY - h) / 2);
  g.save();
  g.beginPath();
  g.roundRect(x, y, w, h, Math.round(lineH / 4));
  g.clip();
  boardFill(g, x, y, w, h);
  g.restore();
  g.fillStyle = "#000";
  g.textAlign = "center";
  g.textBaseline = "middle";
  lines.forEach((l, k) => g.fillText(l, x + w / 2, y + Math.round(lineH / 4) + k * lineH + lineH / 2));
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  return { text, lines, size, box: { x, y, w, h } };
}

// ------------------------------------------------------------------ sub board

/** What the sub board shows (SubBoardRenderer). */
export interface SubBoardScene {
  cols: number;
  rows: number;
  stones: (Stone | null)[];
  toPlay: Stone;
  mode: SubBoardMode;
  /** Analysis the variation comes from: the current node's, or the snapshot taken when the
   *  pointer came to rest on the sub board (no-refresh-on-sub). */
  analysis: NodeAnalysisView | null;
  /** The current node's analysis: the heat maps never freeze (SubBoardRenderer.java:1235). */
  heat: NodeAnalysisView | null;
  /** The sub board's own candidate index (bestmovesNum). */
  candidate: number;
  /** Wheel-stepped length; null shows the whole variation (SHOW_NORMAL_BOARD). */
  length: number | null;
  /** The pointer rests on the sub board with no-refresh-on-sub (isMouseOver). */
  over: boolean;
  candidatesShown: boolean;
  engine: boolean;
  branchLimit: number;
  /** 变化图中去掉死子: the variation's captured stones are taken off. */
  removeDead: boolean;
  /** 纯网络's or Kata形势's squares (the shown variation's movesOwnership with 按选点显示评估). */
  ownership: OwnershipLayer | null;
  /** 四方图模式's renderer order (SubBoardRenderer.setOrder): with fewer candidates than order + 1
   *  the board shows no variation (getBestMove returns empty). 0 for the ordinary sub board. */
  order?: number;
}

export interface SubBoardReport {
  mode: SubBoardMode;
  icon: "play" | "pause" | null;
  /** The drawn candidate index label (bestmovesNum + 1). */
  label: number | null;
  /** The candidate whose variation is drawn. */
  target: Point | null;
  branchLength: number | null;
  numbers: { x: number; y: number; text: string; red: boolean }[];
  heat: { x: number; y: number; size: number }[];
  frozen: boolean;
  /** Whose ownership squares were drawn, if any. */
  ownership: OwnershipLayer["source"] | null;
}

/** SubBoardRenderer.draw (SubBoardRenderer.java:194-230): flat stones, no coordinates; 变化图 shows
 *  the variation of its own candidate index with the play/pause mark, 仅棋盘棋子 the stones only,
 *  the heat maps the first-instinct priors or the searched visits of the current analysis. */
export function paintSubBoard(g: CanvasRenderingContext2D, geo: BoardGeom, origin: { x: number; y: number }, s: SubBoardScene): SubBoardReport {
  const ox = geo.x - origin.x;
  const oy = geo.y - origin.y;
  const sq = geo.square;
  const r = geo.stoneRadius;
  const cols = s.cols;
  const X = (x: number) => ox + geo.margin + sq * x;
  const Y = (y: number) => oy + geo.margin + sq * y;
  const px = (p: number) => X(p % cols);
  const py = (p: number) => Y(i(p / cols));
  // renderImages (SubBoardRenderer.java:1089-1115): squares over the stones only where the
  // variation layer is drawn, so 仅棋盘棋子 keeps just the big squares under the stones. Java's
  // one image serves Kata形势 and 纯网络 alike, so the rule holds for both.
  const layer = s.ownership;
  const below = layer?.style === "BigBelow";
  const above = !!layer && !below && s.mode !== "Raw";
  const report: SubBoardReport = { mode: s.mode, icon: null, label: null, target: null, branchLength: null, numbers: [], heat: [], frozen: s.over, ownership: layer && (below || above) ? layer.source : null };
  g.save();
  g.beginPath();
  g.rect(ox, oy, geo.size, geo.size);
  g.clip();
  // SubBoardRenderer.drawWoodenBoard (SubBoardRenderer.java:1330-1345): the pure-board colour or
  // Theme.defaultBoard, the theme's board without the custom image; drawStarPoints (527-550) puts
  // the 中式经典 markers in place of the star points.
  Object.assign(surface, { x: ox, y: oy, size: geo.size, image: look.themeBoard, custom: false });
  boardFill(g, ox, oy, geo.size, geo.size);
  grid(g, geo, ox, oy, 1.5, 1);
  if (look.chinese) drawChineseClassicMarkers(g, geo.cols, geo.rows, sq, (x) => X(x) + 0.5, (y) => Y(y) + 0.5);
  g.fillStyle = "#000";
  for (const [a, b] of geo.cols === 19 && !look.chinese ? hoshi(geo.cols, geo.rows) : []) {
    circle(g, X(a) + 0.5, Y(b) + 0.5, Math.max(1.5, sq * 0.12));
    g.fill();
  }
  const flatStone = (p: number, stone: Stone) => {
    g.fillStyle = stone === "Black" ? "#000" : "#fff";
    fillCircle(g, px(p), py(p), r);
    if (stone === "White") {
      g.strokeStyle = "#000";
      strokeCircle(g, px(p), py(p), r, 1);
    }
  };
  // drawPlay (SubBoardRenderer.java:292-345): paused while the pointer rests here.
  if (s.mode === "Variation") {
    const m = geo.margin;
    g.fillStyle = "#000";
    if (s.over) {
      g.fillRect(ox + geo.size - i((r * 7) / 2), oy + geo.size - i((m * 9) / 10), i((m * 3) / 10), i((m * 8) / 10));
      g.fillRect(ox + geo.size + i((m * 5) / 10) - i((r * 7) / 2), oy + geo.size - i((m * 9) / 10), i((m * 3) / 10), i((m * 8) / 10));
    } else {
      g.beginPath();
      g.moveTo(ox + geo.size - i((r * 7) / 2), oy + geo.size - 1);
      g.lineTo(ox + geo.size - i((r * 7) / 2), oy + geo.size - m + 1);
      g.lineTo(ox + geo.size - i((r * 5) / 2), oy + geo.size - i(m / 2));
      g.closePath();
      g.fill();
    }
    report.icon = s.over ? "pause" : "play";
  }
  if (layer && below) drawOwnership(g, layer.ownership, s.stones, cols, layer.style, X, Y, sq, false);
  s.stones.forEach((stone, p) => {
    if (stone) flatStone(p, stone);
  });
  if (s.mode === "Policy" || s.mode === "Visits") drawSubHeat(g, s, X, Y, sq, report);
  // drawBranch (SubBoardRenderer.java:857-1020); maybeCleanBranch clears it while the side to
  // move's candidates are hidden.
  let branch: { preview: Preview; candidate: CandidateView } | null = null;
  if (s.mode === "Variation" && s.candidatesShown && s.engine) {
    const candidates = s.analysis?.candidates ?? [];
    const enough = candidates.length >= (s.order ?? 0) + 1;
    const index = enough ? Math.min(s.candidate, candidates.length - 1) : s.candidate;
    const c = enough ? candidates[index] : undefined;
    if (c) {
      const full = s.branchLimit > 0 ? Math.min(s.branchLimit, c.pv.length) : c.pv.length;
      const preview = playVariation(s.stones, s.toPlay, cols, c, s.length ?? full, s.removeDead);
      branch = { preview, candidate: c };
      for (let p = 0; p < preview.stones.length; p++) {
        const captured = preview.captured.get(p);
        const stone = preview.stones[p];
        if (captured && !stone) {
          woodDisc(g, px(p), py(p), r + 1, 1);
          if (s.stones[p]) {
            const x = p % cols, y = i(p / cols);
            g.strokeStyle = "#000";
            g.lineWidth = 1;
            g.beginPath();
            g.moveTo(px(p) - (x === 0 ? 0 : r + 1), py(p) + 0.5);
            g.lineTo(px(p) + (x === cols - 1 ? 0 : r + 1), py(p) + 0.5);
            g.moveTo(px(p) + 0.5, py(p) - (y === 0 ? 0 : r + 1));
            g.lineTo(px(p) + 0.5, py(p) + (y === s.rows - 1 ? 0 : r + 1));
            g.stroke();
          }
          g.fillStyle = captured === "Black" ? "rgba(0,0,0,0.431)" : "rgba(255,255,255,0.471)";
          fillCircle(g, px(p), py(p), r);
        } else if (stone) flatStone(p, stone);
      }
      report.target = c.point;
      report.branchLength = s.length;
    }
    // The rank of the shown variation, also while there is none (publishUnavailableBranchNumber).
    g.fillStyle = "#000";
    g.font = `bold ${i((r * 3) / 2)}px ${GRAPH_FONT}`;
    g.textAlign = "left";
    g.textBaseline = "alphabetic";
    g.fillText(String(index + 1), ox + geo.size - i((r * 9) / 5), oy + geo.size - i(r / 5));
    report.label = index + 1;
  }
  if (layer && above) drawOwnership(g, layer.ownership, s.stones, cols, layer.style, X, Y, sq, false);
  if (branch) drawSubNumbers(g, geo, ox, oy, branch, px, py, s, report);
  g.restore();
  return report;
}

/** SubBoardRenderer.drawMoveNumbers (SubBoardRenderer.java:1118-1225). */
function drawSubNumbers(g: CanvasRenderingContext2D, geo: BoardGeom, ox: number, oy: number, branch: { preview: Preview; candidate: CandidateView }, px: (p: number) => number, py: (p: number) => number, s: SubBoardScene, report: SubBoardReport) {
  const r = geo.stoneRadius;
  const { preview, candidate } = branch;
  if (preview.pass) {
    const black = preview.toPlay === "Black";
    g.fillStyle = black ? "rgba(255,255,255,0.588)" : "rgba(0,0,0,0.588)";
    g.beginPath();
    g.ellipse(ox + i(geo.size / 2), oy + i(geo.size / 2), 4 * r, 4 * r, 0, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = black ? "#000" : "#fff";
    jString(g, t("BoardRenderer.pass"), ox + i(geo.size / 2), oy + i(geo.size / 2), UI_FONT, r * 4, r * 6);
  }
  const first = candidate.point ? candidate.point.y * s.cols + candidate.point.x : -1;
  for (const [p, n] of preview.numbers) {
    const red = s.length !== null ? p === preview.last : p === first;
    g.fillStyle = red ? "#f00" : preview.stones[p] === "Black" ? "#fff" : "#000";
    jString(g, String(n), px(p), py(p), UI_FONT, r * 1.8, r * 1.7);
    report.numbers.push({ x: p % s.cols, y: i(p / s.cols), text: String(n), red });
  }
}

/** SubBoardRenderer.drawLeelazSuggestions + drawHeat (SubBoardRenderer.java:1230-1328, 1447-1468):
 *  heatmap.png on every candidate, 2.7 squares wide at the largest prior (第一感) or the most
 *  visits (计算后); no text. */
function drawSubHeat(g: CanvasRenderingContext2D, s: SubBoardScene, X: (x: number) => number, Y: (y: number) => number, sq: number, report: SubBoardReport) {
  const candidates = s.heat?.candidates ?? [];
  const img = images["heatmap.png"];
  if (!candidates.length || !img?.naturalWidth) return;
  const maxVisits = Math.max(0, ...candidates.map((c) => c.allocation_visits));
  const maxPrior = Math.max(0, ...candidates.map((c) => c.prior));
  const seen = new Set<string>();
  g.imageSmoothingEnabled = true;
  g.imageSmoothingQuality = "high";
  for (const c of candidates) {
    if (!c.point) continue;
    const key = `${c.point.x},${c.point.y}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const length = s.mode === "Visits"
      ? i((maxVisits > 0 ? c.allocation_visits / maxVisits : 0) * 2.7 * sq)
      : i(maxPrior > 0 ? (c.prior * 2.7 * sq) / maxPrior : 0);
    if (length <= 0) continue;
    g.drawImage(img, X(c.point.x) - i(length / 2), Y(c.point.y) - i(length / 2), length, length);
    report.heat.push({ x: c.point.x, y: c.point.y, size: length });
  }
}

/** The panels' type scale, the interface's steps (DESIGN.md Typography): Label (caption), Body,
 *  Title (the figures beside a name), Headline. */
function typeScale(): { label: number; body: number; large: number; headline: number } {
  const type = frameMetrics().type;
  return { label: type.s, body: type.size, large: type.l, headline: type.xl };
}

/** Side padding of a card section's content: 14 px, 12 where the section is narrow. */
function sidePad(w: number): number {
  return w >= 240 ? 14 : 12;
}

/** A number with its name (数字带说明规则): the name at Label size in text3, then the figure at
 *  weight 600 with tabular figures in `color` (text without one), then its unit like the name. */
interface Figure {
  name: string;
  value: string;
  unit?: string;
  color?: string;
}

/** Name to figure, and figure to the next name. */
const FIGURE_GAP = 4;
const FIGURES_GAP = 10;

/** A resource label used as a figure's name: without its trailing colon. */
function nameOf(label: string): string {
  return label.replace(/[:：]\s*$/, "").trim();
}

/** Width of `figures` side by side, names at `label` px and figures at `size` px. */
function figuresWidth(g: CanvasRenderingContext2D, figures: Figure[], label: number, size: number, family: string): number {
  let width = 0;
  figures.forEach((f, k) => {
    if (k > 0) width += FIGURES_GAP;
    g.font = `${label}px ${family}`;
    if (f.name) width += measure(g, f.name).width + (f.value ? FIGURE_GAP : 0);
    if (f.unit) width += FIGURE_GAP + measure(g, f.unit).width;
    g.font = `600 ${size}px ${family}`;
    width += tabularWidth(g, f.value);
  });
  return width;
}

/** Draws `figures` side by side from `x` on the baseline `y`. */
function drawFigures(g: CanvasRenderingContext2D, figures: Figure[], x: number, y: number, label: number, size: number, family: string, p: StagePalette) {
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  figures.forEach((f, k) => {
    if (k > 0) x += FIGURES_GAP;
    g.font = `${label}px ${family}`;
    if (f.name) {
      g.fillStyle = p.text3;
      g.fillText(f.name, x, y);
      x += measure(g, f.name).width + (f.value ? FIGURE_GAP : 0);
    }
    g.font = `600 ${size}px ${family}`;
    g.fillStyle = f.color ?? p.text;
    fillTabular(g, f.value, x, y);
    x += tabularWidth(g, f.value);
    if (f.unit) {
      g.font = `${label}px ${family}`;
      g.fillStyle = p.text3;
      g.fillText(f.unit, x + FIGURE_GAP, y);
      x += FIGURE_GAP + measure(g, f.unit).width;
    }
  });
}

/** `text` cut with "..." to fit `maxWidth` in the current font (truncateStatusText's cut): the
 *  longest prefix that fits with the dots, or nothing when the dots alone do not fit. */
function elide(g: CanvasRenderingContext2D, text: string, maxWidth: number): string {
  const measure = (s: string) => Math.round(g.measureText(s).width);
  if (measure(text) <= maxWidth) return text;
  const dots = measure("...");
  if (dots > maxWidth) return "";
  let low = 0;
  let high = text.length;
  while (low < high) {
    const middle = i((low + high + 1) / 2);
    if (measure(text.substring(0, middle)) + dots <= maxWidth) low = middle;
    else high = middle - 1;
  }
  const prefix = text.substring(0, low).trim();
  return prefix ? `${prefix}...` : "...";
}

/** The engine dot (live or idle) of `d` px centred on (x, y). */
function engineDot(g: CanvasRenderingContext2D, x: number, y: number, d: number, live: boolean, p: StagePalette) {
  g.fillStyle = live ? p.live : p.idle;
  circle(g, x, y, d / 2);
  g.fill();
}

/** The game header's sizes at one scale. */
interface HeaderSizes {
  stone: number;
  label: number;
  body: number;
  large: number;
  gap: number;
  dot: number;
}

/** The widest digit's advance in the current font: canvas text has no font-variant-numeric, so
 *  tabular figures are laid out by hand. */
function digitAdvance(g: CanvasRenderingContext2D): number {
  let widest = 0;
  for (const d of "0123456789") widest = Math.max(widest, g.measureText(d).width);
  return widest;
}

/** A string's parts for tabular figures: each digit alone (group 1), and every run between
 *  digits whole, so it is measured and shaped as one text (combining marks stay on their letters). */
const TABULAR_PARTS = /([0-9])|[^0-9]+/g;

/** Width of `s` with tabular figures (fillTabular). */
export function tabularWidth(g: CanvasRenderingContext2D, s: string): number {
  const digit = digitAdvance(g);
  let width = 0;
  for (const [part, figure] of s.matchAll(TABULAR_PARTS)) width += figure ? digit : g.measureText(part).width;
  return Math.round(width);
}

/** fillText with tabular figures: every digit centred in the widest digit's advance, the text
 *  between digits drawn as one run. */
export function fillTabular(g: CanvasRenderingContext2D, s: string, x: number, y: number) {
  const digit = digitAdvance(g);
  for (const [part, figure] of s.matchAll(TABULAR_PARTS)) {
    const own = g.measureText(part).width;
    if (figure) {
      g.fillText(part, x + (digit - own) / 2, y);
      x += digit;
    } else {
      g.fillText(part, x, y);
      x += own;
    }
  }
}

export interface CapturedScene {
  toPlay: Stone;
  blackCaptures: number;
  whiteCaptures: number;
  moveNumber: number;
  komi: number;
  analysing: boolean;
  rules: string | null;
  /** AI总评分 of Black / White (core `statistics`), null when none of their moves is evaluated. */
  aiScoreBlack: number | null;
  aiScoreWhite: number | null;
  /** isSmallCap: 四方图 / 思考模式's captured strip (LizzieFrame.java:9189-9373). */
  small?: boolean;
}

/** What paintCaptured drew, in panel coordinates: the rules caption (null when the panel shows
 *  the move number or has no room for the caption), the side to move (null in the small strip or
 *  without room), the engine dot and the boxes beside and below them. */
export interface CapturedReport {
  width: number;
  height: number;
  caption: string | null;
  captionBox: Rect | null;
  turn: string | null;
  turnBox: Rect | null;
  /** The engine dot: its centre, diameter and row (1 with the caption, 2 with the komi). */
  dot: { x: number; y: number; d: number; row: 1 | 2 };
  /** Black's and White's stone with the 提子 beside it, and the 贴目 row (null with one row). */
  black: Rect;
  white: Rect;
  komi: Rect | null;
  /** The 贴目 figure as written (komiText), also when its row is left out. */
  komiText: string;
}

/** drawCaptured's AI总评分 text (LizzieFrame.AIscore + "%.1f"). */
export function aiScoreText(score: number): string {
  return `${t("LizzieFrame.AIscore")}${score.toFixed(1)}`;
}

/** Java String.valueOf(double) for komi values: whole numbers keep ".0". */
export function javaDouble(v: number): string {
  return Number.isInteger(v) ? v.toFixed(1) : String(v);
}

/** Double.parseDouble: surrounding blanks allowed, a signed decimal with an optional exponent or a
 *  hex significand with its binary exponent (0x1.8p3), then an optional float / double suffix;
 *  NaN and Infinity are refused (Java's limits reject them too). */
export function parseJavaDouble(text: string): number | null {
  const s = text.trim().replace(/[fFdD]$/, "");
  const hex = /^([+-]?)0[xX]([0-9a-fA-F]*)\.?([0-9a-fA-F]*)[pP]([+-]?\d+)$/.exec(s);
  let v: number;
  if (hex) {
    const [, sign, whole, fraction, exponent] = hex;
    if (!whole && !fraction) return null;
    const significand = parseInt(whole + fraction, 16);
    v = (sign === "-" ? -1 : 1) * (significand && significand * 2 ** (Number(exponent) - 4 * fraction.length));
  } else if (/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$/.test(s)) v = Number(s);
  else return null;
  return Number.isFinite(v) ? v : null;
}

/** The smallest text the panels draw (DESIGN.md: nothing below Label; 9 px at the smallest
 *  interface size a short section allows). */
const MIN_TEXT = 9;

/** LizzieFrame.drawCaptured (LizzieFrame.java:9098-9375) as the card's game header: Black on the
 *  left and White on the right, each a flat stone (the side to move full size, the other smaller,
 *  as Java draws it) with its 提子 beside it and its AI总评分 below; in the middle the engine dot
 *  with the rules caption (else the move number) and the komi under it. The side to move (Java
 *  `InfoPanelText.turn`, not in the small strip) follows the dot: the two go together to the
 *  komi row, at the komi's left, where they fit whole beside Black's AI总评分; the caption and the
 *  komi keep their place and size either way (Java sets the dot and the side to move on a row of
 *  their own under the caption). The small strip of 四方图 / 思考模式 has no capture counts. The
 *  dot shows pondering; stored analysis is independent of it.
 *  Text shrinks to fit, never under MIN_TEXT; a section too short or narrow for that drops rows
 *  instead: first the second row (吻合度, 贴目; the small strip moves 吻合度 beside its stones
 *  while that fits), then the caption (the dot stays), then the 提子 names (each count stays
 *  beside its stone). The side to move never makes the other text smaller, moves or drops it.
 *  Returns what was drawn; its captionBox is the rules caption's text box as drawn (Java's
 *  matchRulesCaptionHitBox), null when no caption is drawn. */
export function paintCaptured(g: CanvasRenderingContext2D, r: Rect, s: CapturedScene): CapturedReport | null {
  const { w, h } = r;
  if (w < 10 || h < 5) return null;
  const p = stagePalette();
  const family = panelFont();
  const type = typeScale();
  const pad = sidePad(w);
  const room = w - 2 * pad;
  const side = (captures: number, score: number | null) => ({
    top: s.small ? [] : [{ name: nameOf(t("LizzieFrame.captures")), value: String(captures) }],
    bottom: score === null ? [] : [{ name: nameOf(t("LizzieFrame.AIscore")), value: score.toFixed(1) }],
  });
  const black = side(s.blackCaptures, s.aiScoreBlack);
  const white = side(s.whiteCaptures, s.aiScoreWhite);
  const komi: Figure[] = [{ name: nameOf(t("Menu.komi")), value: komiText(s.komi) }];
  const moveNumber: Figure[] = [{ name: nameOf(t("BottomToolbar.move")), value: String(s.moveNumber) }];
  const STONE = 20;
  const sizes = (k: number): HeaderSizes => ({
    stone: STONE * k,
    label: Math.max(MIN_TEXT, type.label * k),
    body: Math.max(MIN_TEXT, type.body * k),
    large: Math.max(MIN_TEXT, type.large * k),
    gap: 8 * k,
    dot: Math.max(3, 6 * k),
  });
  const captionWidth = (z: HeaderSizes) => {
    if (!s.rules) return figuresWidth(g, moveNumber, z.label, z.body, family);
    g.font = `${z.body}px ${family}`;
    return g.measureText(s.rules).width;
  };
  // The layouts from the fullest: both rows, then one row with and without the caption, then
  // the counts without their names.
  const tops: [Figure[], Figure[]] = [black.top, white.top];
  const bare: [Figure[], Figure[]] = [black.top.map((f) => ({ ...f, name: "" })), white.top.map((f) => ({ ...f, name: "" }))];
  const plans = [
    { twoRows: true, caption: true, top: tops },
    ...(s.small
      ? [
          { twoRows: false, caption: true, top: [black.bottom, white.bottom] as [Figure[], Figure[]] },
          { twoRows: false, caption: true, top: tops },
          { twoRows: false, caption: false, top: tops },
        ]
      : [
          { twoRows: false, caption: true, top: tops },
          { twoRows: false, caption: false, top: tops },
          { twoRows: false, caption: false, top: bare },
        ]),
  ];
  /** The space between the two sides and the middle, which does not scale with the text. */
  const FIXED = 2 * 12;
  const layout = (plan: (typeof plans)[number]) => {
    // Stone row, 6 px, figure row (or the stone row alone); the scale follows the height.
    let k = Math.max(0.3, Math.min(1, plan.twoRows ? (h - 8) / (STONE + 6 + type.large * 1.35) : (h - 4) / STONE));
    const needed = (z: HeaderSizes) => {
      const sideW = (top: Figure[], bottom: Figure[]) =>
        Math.max(z.stone + (top.length ? z.gap + figuresWidth(g, top, z.label, z.large, family) : 0), plan.twoRows ? figuresWidth(g, bottom, z.label, z.large, family) : 0);
      const middle = Math.max(z.dot + (plan.caption ? 5 * k + captionWidth(z) : 0), plan.twoRows ? figuresWidth(g, komi, z.label, z.body, family) : 0);
      return 2 * Math.max(sideW(plan.top[0], black.bottom), sideW(plan.top[1], white.bottom)) + middle + FIXED;
    };
    // Only the scaled part shrinks; the text's own rounding may leave a pixel over, so a little
    // more is taken.
    for (let n = 0; n < 4 && needed(sizes(k)) > room; n++) k = Math.max(0.3, k * ((room - FIXED) / (needed(sizes(k)) - FIXED)) - 0.005);
    const z = sizes(k);
    const tall = plan.twoRows ? z.stone + 6 * k + Math.round(z.large * 1.35) : Math.max(z.stone, Math.round(z.large * 1.2));
    return { k, z, fits: needed(z) <= room && tall <= h - 2 };
  };
  let plan = plans[plans.length - 1];
  let { k, z } = layout(plan);
  for (const candidate of plans) {
    const laid = layout(candidate);
    if (!laid.fits) continue;
    [plan, k, z] = [candidate, laid.k, laid.z];
    break;
  }
  const lineB = Math.round(z.large * 1.35);
  // The rows share the section's spare height with its edges, up to 14 px between them, so they
  // stand apart instead of huddling in the middle.
  const gapB = Math.min(14 * k, Math.max(6 * k, (h - z.stone - lineB) / 3));
  const rowA = plan.twoRows ? Math.max(0, (h - (z.stone + gapB + lineB)) / 2) + z.stone / 2 : h / 2;
  const baseA = Math.round(rowA + z.large * 0.36);
  const baseB = Math.round(rowA + z.stone / 2 + gapB + lineB / 2 + z.large * 0.36);
  const stone = (cx: number, toPlay: boolean, fill: string, edge: string) => {
    const d = toPlay ? z.stone : z.stone * 0.75;
    g.fillStyle = fill;
    circle(g, cx, rowA, d / 2);
    g.fill();
    g.strokeStyle = edge;
    g.lineWidth = 1;
    circle(g, cx, rowA, d / 2 - 0.5);
    g.stroke();
  };
  stone(pad + z.stone / 2, s.toPlay === "Black", p.stoneBlack, p.stoneBlackEdge);
  stone(w - pad - z.stone / 2, s.toPlay === "White", p.stoneWhite, p.stoneWhiteEdge);
  drawFigures(g, plan.top[0], pad + z.stone + z.gap, baseA, z.label, z.large, family, p);
  drawFigures(g, plan.top[1], w - pad - z.stone - z.gap - figuresWidth(g, plan.top[1], z.label, z.large, family), baseA, z.label, z.large, family, p);
  // Each stone with the 提子 beside it.
  const [blackW, whiteW] = plan.top.map((top) => z.stone + (top.length ? z.gap + figuresWidth(g, top, z.label, z.large, family) : 0));
  const komiW = figuresWidth(g, komi, z.label, z.body, family);
  const komiX = i(w / 2 - komiW / 2);
  // The middle as the dot and the caption lay it out on the first row.
  const start = i(w / 2 - (z.dot + (plan.caption ? 5 * k + captionWidth(z) : 0)) / 2);
  // The side to move with the dot on the komi row, left of the komi, clear of Black's AI总评分.
  const turn = s.small || !plan.twoRows ? null : t(s.toPlay === "Black" ? "LizzieFrame.info.blackToPlay" : "LizzieFrame.info.whiteToPlay");
  g.font = `${z.body}px ${family}`;
  const turnText = turn ? measure(g, turn).width : 0;
  const scoreW = figuresWidth(g, black.bottom, z.label, z.large, family);
  const groupX = komiX - 8 * k - (z.dot + 5 * k + turnText);
  const shownTurn = turn !== null && groupX >= pad + (scoreW ? scoreW + 12 : 0);
  const komiMid = baseB - z.body * 0.36;
  const report: CapturedReport = {
    width: w,
    height: h,
    caption: null,
    captionBox: null,
    turn: null,
    turnBox: null,
    dot: shownTurn ? { x: groupX + z.dot / 2, y: komiMid, d: z.dot, row: 2 } : { x: start + z.dot / 2, y: rowA, d: z.dot, row: 1 },
    black: { x: pad, y: rowA - z.stone / 2, w: blackW, h: z.stone },
    white: { x: w - pad - whiteW, y: rowA - z.stone / 2, w: whiteW, h: z.stone },
    komi: plan.twoRows ? { x: komiX, y: baseB - z.body, w: komiW, h: z.body * 1.2 } : null,
    komiText: komi[0].value,
  };
  if (plan.twoRows) {
    drawFigures(g, black.bottom, pad, baseB, z.label, z.large, family, p);
    drawFigures(g, white.bottom, w - pad - figuresWidth(g, white.bottom, z.label, z.large, family), baseB, z.label, z.large, family, p);
    drawFigures(g, komi, komiX, baseB, z.label, z.body, family, p);
  }
  engineDot(g, report.dot.x, report.dot.y, z.dot, s.analysing, p);
  const box = (text: string, left: number, base: number) => {
    const m = measure(g, text);
    const ascent = m.ascent || z.body * 0.8;
    const descent = m.descent || z.body * 0.25;
    return { x: left, y: base - ascent, w: m.width, h: ascent + descent };
  };
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  if (shownTurn && turn) {
    const x = groupX + z.dot + 5 * k;
    g.font = `${z.body}px ${family}`;
    g.fillStyle = p.text2;
    g.fillText(turn, x, baseB);
    report.turn = turn;
    report.turnBox = box(turn, x, baseB);
  }
  if (!plan.caption) return report;
  const captionBase = Math.round(rowA + z.body * 0.36);
  const x = start + z.dot + 5 * k;
  if (!s.rules) {
    drawFigures(g, moveNumber, x, captionBase, z.label, z.body, family, p);
    return report;
  }
  g.font = `${z.body}px ${family}`;
  g.fillStyle = p.text2;
  g.fillText(s.rules, x, captionBase);
  report.caption = s.rules;
  report.captionBox = box(s.rules, x, captionBase);
  return report;
}

/** What the statistics panel shows, from core projections only. */
export interface StatScene {
  /** Black's win rate on the bar: the current node's, else the last analysed one's before it
   *  (parent, then grandparent); null when none is analysed. */
  black: number | null;
  /** Black's win rate at the position the change is measured from, for the change marker. */
  before: number | null;
  /** The mover's win-rate / score-lead change for the last move (core `last_move`), null unknown. */
  change: number | null;
  scoreChange: number | null;
  /** Black's lead from the same node as `black` (the board difference under 盘面). */
  lead: number | null;
  leadWithKomi: boolean;
  /** isKataStyle: a KataGo engine or KataGo data. */
  kata: boolean;
  /** 1-based candidate rank of the last move (lastMoveMatchCandidteNo). */
  matched: number | null;
  moveNumber: number;
  /** shouldDrawMoveNumberDown: the captured panel shows the rules instead of the move number. */
  moveNumberDown: boolean;
}

export interface StatReport {
  text: string;
  bar: { black: string; white: string } | null;
  change: "gain" | "loss" | null;
  moveNumber: boolean;
  /** The row under the bar (the lead and the last move) is drawn: it fits whole. Otherwise what it
   *  lacked: the section's height (whatever the text) or the width for its text. */
  note: boolean;
  noteLacks: "height" | "width" | null;
}

/** LizzieFrame.drawMoveStatistics (LizzieFrame.java:8702-9025) outside play mode, as a card
 *  section: both win rates as headline figures at the two ends, each after its side's name (Java
 *  "黑 99.2%"), with the move number between them
 *  (when it moved here from the captured panel and fits), the black/white bar with its centre
 *  tick and, under it, the last move's change from the win rate before it (gain or loss colour,
 *  pointing to the win rate now); then the score lead at the left and the last move's rank and
 *  changes at the right, a loss in the loss colour. Unknown values are left out instead of being
 *  drawn as zero. `report.text` is Java's line of the same values. */
export function paintStat(g: CanvasRenderingContext2D, r: Rect, s: StatScene): StatReport {
  const { w, h } = r;
  const report: StatReport = { text: "", bar: null, change: null, moveNumber: false, note: false, noteLacks: null };
  if (w < 10 || h < 5) return report;
  const p = stagePalette();
  // The lead with komi keeps its label and signed figure; Black's lead on the board is worded by
  // who leads (MoveStatusText.lead): "白领先 7.3 目", "均势".
  let lead: Figure[] = [];
  let leadText = "";
  if (s.kata && s.lead !== null && s.leadWithKomi) {
    lead = [{ name: nameOf(t("LizzieFrame.scoreLeadWithKomi")), value: s.lead.toFixed(1) }];
    leadText = `${t("LizzieFrame.scoreLeadWithKomi")}${s.lead.toFixed(1)}`;
  } else if (s.kata && s.lead !== null) {
    const who = leadOf(s.lead);
    if (who === "even") {
      leadText = t("LizzieFrame.status.even");
      lead = [{ name: "", value: leadText }];
    } else {
      const words = t(who.side === "Black" ? "LizzieFrame.status.blackLeads" : "LizzieFrame.status.whiteLeads");
      const [before, after = ""] = words.split("{0}");
      leadText = format(words, who.points);
      lead = [{ name: before.trim(), value: who.points, unit: after.trim() }];
    }
  }
  // The previous move's change only when the core has both positions analysed (Java
  // `LizzieFrame.drawMoveStatistics`: `validLastWinrate && validWinrate`), worded by
  // MoveStatusText.previousMove: its rank only when it was a candidate, the score when known.
  const prev = s.change === null ? null : previousMove(s.matched, s.change, s.kata ? s.scoreChange : null, { name: t("LizzieFrame.status.previousMove"), pts: t("LizzieFrame.pts") });
  report.text = [leadText, prev?.text].filter(Boolean).join(" ");

  const family = panelFont();
  const ui = panelFont("ui");
  const type = typeScale();
  const pad = sidePad(w);
  const room = w - 2 * pad;
  const last: Figure[] = [];
  if (prev) {
    last.push({ name: prev.name, value: prev.winrate.value, ...(prev.winrate.loss ? { color: p.loss } : {}) });
    if (prev.score) last.push({ name: "", value: prev.score.value, ...(prev.score.loss ? { color: p.loss } : {}) });
  }
  const moveNumber: Figure[] = [{ name: nameOf(t("BottomToolbar.move")), value: String(s.moveNumber) }];
  // Headline row, 6 px, the bar, 8 px (the change marker), the note row; the scale follows the
  // height, text never under MIN_TEXT. Without room for the note row at that size (height, or
  // width for the lead and the last move side by side) the row is left out, and the percentages
  // and the bar take the height with room for the change marker under the bar.
  const rows = (note: boolean) => {
    const k = Math.max(0.3, Math.min(1, note ? (h - 6) / (type.headline * 1.25 + 6 + 10 + 8 + type.body * 1.4) : (h - 4) / (type.headline * 1.25 + 6 + 10 + 6)));
    const headline = Math.max(MIN_TEXT, type.headline * k);
    const label = Math.max(MIN_TEXT, type.label * k);
    const body = Math.max(MIN_TEXT, type.body * k);
    const barH = Math.max(4, Math.round(10 * k));
    const rowH1 = Math.round(headline * 1.25);
    const rowH3 = note ? Math.round(body * 1.4) : 0;
    const total = rowH1 + 6 * k + barH + (note ? 8 * k + rowH3 : 6);
    return { k, headline, label, body, barH, rowH1, rowH3, total };
  };
  let z = rows(true);
  const notesW = figuresWidth(g, lead, z.label, z.body, family) + figuresWidth(g, last, z.label, z.body, family) + (lead.length ? 12 : 0);
  const note = z.total <= h - 2 && notesW <= room;
  if (!note) {
    report.noteLacks = z.total > h - 2 ? "height" : "width";
    z = rows(false);
  }
  const { k, headline, label, body, barH, rowH1, rowH3, total } = z;
  // Spare height goes to the two gaps around the bar as well as the edges, up to 6 px each.
  const extra = Math.min(6 * k, Math.max(0, (h - total - 4) / 4));
  const top = Math.max(0, (h - total - 2 * extra) / 2);
  const base1 = Math.round(top + rowH1 / 2 + headline * 0.36);
  const barY = Math.round(top + rowH1 + 6 * k + extra);
  const base3 = Math.round(barY + barH + 8 * k + extra + rowH3 / 2 + body * 0.36);

  // The note row: the lead at the left, the last move at the right.
  if (note) {
    report.note = true;
    drawFigures(g, lead, pad, base3, label, body, family, p);
    drawFigures(g, last, w - pad - figuresWidth(g, last, label, body, family), base3, label, body, family, p);
  }

  const moveW = figuresWidth(g, moveNumber, label, body, family);
  const centreMove = (ends: number) => {
    if (!s.moveNumberDown || room < ends + moveW + 24) return;
    drawFigures(g, moveNumber, i(w / 2 - moveW / 2), base1, label, body, family, p);
    report.moveNumber = true;
  };
  if (s.black === null) {
    centreMove(0);
    return report;
  }
  const winB = `${s.black.toFixed(1)}%`, winW = `${(100 - s.black).toFixed(1)}%`;
  // Each percentage after its side's name (Java `Menu.Black` / `Menu.White`), the name a Label;
  // a section too narrow for both names keeps the percentages alone.
  let ends: [Figure[], Figure[]] = [[{ name: t("Menu.Black"), value: winB }], [{ name: t("Menu.White"), value: winW }]];
  let [winBWidth, winWWidth] = ends.map((f) => figuresWidth(g, f, label, headline, ui));
  if (winBWidth + winWWidth + 12 > room) {
    ends = [[{ name: "", value: winB }], [{ name: "", value: winW }]];
    [winBWidth, winWWidth] = ends.map((f) => figuresWidth(g, f, label, headline, ui));
  }
  const [barBlack, barWhite] = ends.map(([f]) => (f.name ? `${f.name} ${f.value}` : f.value));
  report.bar = { black: barBlack, white: barWhite };
  drawFigures(g, ends[0], pad, base1, label, headline, ui, p);
  drawFigures(g, ends[1], w - pad - winWWidth, base1, label, headline, ui, p);
  centreMove(winBWidth + winWWidth);

  // The bar: White's track with its edge inside, Black's share over it, the centre tick.
  const radius = barH / 2;
  const barW = i((s.black * room) / 100);
  g.fillStyle = p.stoneWhite;
  g.beginPath();
  g.roundRect(pad, barY, room, barH, radius);
  g.fill();
  g.strokeStyle = p.stoneWhiteEdge;
  g.lineWidth = 1;
  g.beginPath();
  g.roundRect(pad + 0.5, barY + 0.5, room - 1, barH - 1, radius - 0.5);
  g.stroke();
  g.save();
  g.beginPath();
  g.roundRect(pad, barY, room, barH, radius);
  g.clip();
  g.fillStyle = p.stoneBlack;
  g.fillRect(pad, barY, barW, barH);
  g.restore();
  g.fillStyle = p.barGrid;
  g.fillRect(pad + i(room / 2), barY - 3, 1, barH + 6);

  // The change: a 2 px line under the bar from the win rate before the move to the one now,
  // ending in an arrowhead.
  if (s.change !== null && s.before !== null) {
    const nowX = pad + barW;
    const lastX = pad + i((s.before * room) / 100);
    if (nowX !== lastX) {
      const gain = s.change >= 0;
      report.change = gain ? "gain" : "loss";
      const y = barY + barH + 3;
      const d = nowX > lastX ? 1 : -1;
      const tip = Math.min(4, Math.abs(nowX - lastX));
      g.fillStyle = gain ? p.gain : p.loss;
      g.fillRect(Math.min(lastX, nowX - d * tip), y - 1, Math.abs(nowX - lastX) - tip, 2);
      g.beginPath();
      g.moveTo(nowX, y);
      g.lineTo(nowX - d * tip, y - 2.5);
      g.lineTo(nowX - d * tip, y + 2.5);
      g.closePath();
      g.fill();
    }
  }
  return report;
}

/** drawMoveStatistics in play mode (LizzieFrame.java:8705-8736): while the human's clock runs,
 *  its text centred in the section, fitted to the width, at most the headline size. */
export function paintPlayStat(g: CanvasRenderingContext2D, r: Rect, clock: string | null): void {
  const { w, h } = r;
  if (w < 10 || h < 5 || !clock) return;
  const p = stagePalette();
  const font = panelFont("ui");
  g.font = `600 12px ${font}`;
  const size = Math.min(h, (12 * (w - 2 * sidePad(w))) / tabularWidth(g, clock), typeScale().headline);
  g.font = `600 ${size}px ${font}`;
  g.fillStyle = p.text;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  fillTabular(g, clock, i((w - tabularWidth(g, clock)) / 2), Math.round(h / 2 + size * 0.36));
}

/** The status lines as paintStatus drew them, each cut with "..." where it did not fit; `rows`
 *  holds only the hint rows the height had room for. */
export interface StatusReport {
  title: string;
  state: string | null;
  rows: string[][];
}

/** Pondering status lines (LizzieFrame.drawPonderingState) as a card section, centred in the
 *  status area: the engine line at Title weight, the other lines at Label size in text3. With
 *  `live` known (the engine's own status, not a loading or recovery message), the second line is
 *  the analysis state (Java `StatusOverlay.state`), drawn right of the engine name after the live /
 *  idle dot, and the Space hint that follows it joins the next hint, on one line where both fit.
 *  `speed`, the analysing engine's search speed, follows the engine name at Label size in text3
 *  when it fits whole. Without room for full lines they share the height at the largest scale
 *  that fits, the hints pairing up again where the smaller text lets them, the text never under
 *  9 px; each line is cut with "..." to the width, and a hint row the height cannot hold whole is
 *  left out. The face is the theme's font-name when it names one (LizzieFrame.createStatusFont,
 *  LizzieFrame.java:8432-8448). */
export function paintStatus(g: CanvasRenderingContext2D, r: Rect, lines: string[], live: boolean | null, speed: string): StatusReport {
  const report: StatusReport = { title: "", state: null, rows: [] };
  if (!lines.length) return report;
  const { w, h } = r;
  const p = stagePalette();
  const family = panelFont("info");
  const type = typeScale();
  const pad = sidePad(w);
  const room = w - 2 * pad;
  const [title, ...others] = lines;
  const state = live !== null && others.length ? others[0] : null;
  const notes = state === null ? others : others.slice(1);
  const rowsAt = (label: number) => {
    g.font = `${label}px ${family}`;
    return notes.length >= 2 && g.measureText(notes[0]).width + 16 + g.measureText(notes[1]).width <= room
      ? [[notes[0], notes[1]], ...notes.slice(2).map((s) => [s])]
      : notes.map((s) => [s]);
  };
  const fit = (rows: string[][]) => Math.min(1, (h - 8) / (type.body * 1.5 + rows.length * type.label * 1.45));
  let rows = rowsAt(type.label);
  let k = fit(rows);
  // Smaller text may pair the hotkey and the hint, which leaves a larger scale.
  for (let scale = 1; k < 1 && scale > k; scale -= 0.05) {
    const paired = rowsAt(Math.max(9, type.label * scale));
    if (fit(paired) >= scale) {
      [rows, k] = [paired, scale];
      break;
    }
  }
  const body = Math.max(9, type.body * k);
  const label = Math.max(9, type.label * k);
  const lh1 = Math.round(body * 1.5);
  const lh2 = Math.round(label * 1.45);
  let y = Math.max(2, (h - lh1 - rows.length * lh2) / 2);
  g.textAlign = "left";
  g.textBaseline = "middle";
  let stateW = 0;
  if (state !== null && live !== null) {
    g.font = `${body}px ${family}`;
    const shown = elide(g, state, room * 0.6);
    report.state = shown;
    const x = w - pad - g.measureText(shown).width;
    g.fillStyle = p.text2;
    g.fillText(shown, x, y + lh1 / 2);
    engineDot(g, x - 8, y + lh1 / 2, 6, live, p);
    stateW = w - pad - x + 11 + 12;
  }
  g.font = `600 ${body}px ${family}`;
  g.fillStyle = p.text;
  const name = elide(g, title, room - stateW);
  report.title = name;
  g.fillText(name, pad, y + lh1 / 2);
  if (speed) {
    const x = pad + g.measureText(name).width + 8;
    g.font = `${label}px ${family}`;
    if (x + tabularWidth(g, speed) <= w - pad - stateW) {
      g.fillStyle = p.text3;
      fillTabular(g, speed, x, y + lh1 / 2);
    }
  }
  y += lh1;
  g.font = `${label}px ${family}`;
  g.fillStyle = p.text3;
  for (const row of rows) {
    if (y + lh2 > h) break;
    if (row.length === 2) {
      g.fillText(row[0], pad, y + lh2 / 2);
      g.fillText(row[1], w - pad - g.measureText(row[1]).width, y + lh2 / 2);
      report.rows.push(row);
    } else {
      const shown = elide(g, row[0], room);
      g.fillText(shown, pad, y + lh2 / 2);
      report.rows.push([shown]);
    }
    y += lh2;
  }
  return report;
}

/** LizzieFrame.drawPonderingStateForExtraMode2 (LizzieFrame.java:8324-8354) with fitStatusFont /
 *  truncateStatusText (8405-8430, 8553-8577): one status box of 双引擎模式 at (x, y) of a panel
 *  `panelW` x `panelH`. The font is the panels' Body size (Java's 18 px box reached over the
 *  players' names under a board that fills the panel's height); it shrinks to at most 10 to fit
 *  the width left on its board's side (`split`, where the right board starts), then the text is
 *  cut with "..."; the box is a small rounded patch of the card ground, the text primary and
 *  centred. Returns the text as drawn and its width (0: nothing). Java split at half the panel, so
 *  a window wider than the two boards cut the secondary engine's box (starting at the right board)
 *  down to a few characters. */
export function paintDualStrip(g: CanvasRenderingContext2D, text: string, x: number, y: number, split: number, panelW: number, panelH: number): { text: string; width: number } {
  const rightEdge = x < split ? split : panelW;
  const maxWidth = Math.max(1, rightEdge - x - 4);
  // fitStatusFont over createStatusFont (LizzieFrame.java:8405-8448): the theme's font-name first.
  const family = panelFont("info");
  const font = (px: number) => `${px}px ${family}`;
  const measure = (s: string) => Math.round(g.measureText(s).width);
  const size = typeScale().body;
  let px = size;
  g.font = font(px);
  const measured = measure(text);
  if (text && measured > maxWidth) {
    const least = Math.min(size, 10);
    px = Math.max(least, Math.min(size, Math.floor((size * maxWidth) / measured)));
    g.font = font(px);
    while (px > least && measure(text) > maxWidth) g.font = font(--px);
  }
  const shown = elide(g, text, maxWidth);
  const stringWidth = measure(shown);
  if (stringWidth <= 0) return { text: "", width: 0 };
  const { ascent, descent } = javaMetrics(g);
  const fontHeight = ascent + descent;
  const width = Math.min(maxWidth, Math.max(stringWidth, 1));
  const height = Math.max(i(fontHeight * 1.2), 1);
  const drawX = Math.max(0, Math.min(x, panelW - width));
  const drawY = Math.max(0, Math.min(y, Math.max(0, panelH - height - 4)));
  const p = stagePalette();
  g.fillStyle = p.panel;
  g.beginPath();
  g.roundRect(drawX - 4, drawY, width + 8, height, 4);
  g.fill();
  g.fillStyle = p.text;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(shown, drawX + (width - stringWidth) / 2, drawY + i((height - fontHeight) / 2) + ascent);
  return { text: shown, width: stringWidth };
}

/** One drawing of VariationTree.drawTree (scroll pane) or VariationTreeBig.drawTree (whole tree
 *  in the panel): the deferred drawing, the centre of every node walked (for click hit-testing,
 *  Java's calc pass), the current node's place and lane, the extent of what was drawn and, by row
 *  top, the node whose move number the row shows. */
interface TreeWalk {
  ops: ((g: CanvasRenderingContext2D) => void)[];
  hits: Map<string, number>;
  curX: number;
  curY: number;
  curLane: number;
  maxLane: number;
  maxX: number;
  maxY: number;
  rows: Map<number, number>;
}

/** Row and lane spacing, node size and the first lane's least left edge (Java's 20, 20, 11 and
 *  the scroll pane's 20 px plus the room the current node's ring needs). */
const [TREE_YS, TREE_XS, TREE_DOT, TREE_X0] = [20, 20, 11, 24];
/** From a row number's right edge to the first lane's left edge: 2 px, the column's 1 px edge
 *  and the 3 px the current node's ring reaches left of its node. */
const TREE_NUMBER_GAP = 6;

/** A row number drawn, in panel coordinates (its ink box) for the acceptance report. */
export interface TreeRowNumber {
  node: number;
  text: string;
  x: number;
  y: number;
  w: number;
  h: number;
  current: boolean;
}

/** Where the lanes start and where they show from: with 显示分支面板上的手数 the first lane starts
 *  right of the tree's widest move number (in the current row's 600 weight) and of the column's
 *  4 px left padding, at `TREE_X0` at least, and the lanes show from `gutter` (where the first
 *  lane's current ring ends); left of it is the column, which they pass under when they scroll or
 *  move left. Without the numbers there is no column and the lanes show from the panel's edge. */
export interface TreeMargin {
  x0: number;
  gutter: number;
}

function treeMargin(g: CanvasRenderingContext2D, nodes: Map<number, TreeNodeView>, numbers: boolean): TreeMargin {
  if (!numbers) return { x0: TREE_X0, gutter: 0 };
  let most = 0;
  for (const n of nodes.values()) if (n.kind === "Black" || n.kind === "White") most = Math.max(most, n.move_number);
  g.font = `600 ${typeScale().label}px ${panelFont("ui")}`;
  const x0 = Math.max(TREE_X0, 4 + Math.ceil(tabularWidth(g, String(most))) + TREE_NUMBER_GAP);
  return { x0, gutter: x0 - 3 };
}

/** The move numbers of the rows in the column at the panel's left (a row is one move deeper than
 *  the row above, so all its nodes are the same move; this replaces Java's number beside every
 *  node, which ran into the next lane's nodes). Each row shows its chosen node's move number,
 *  right-aligned in tabular figures at the Label size in the secondary text colour, the current
 *  node's row in the text colour at 600; a row whose chosen node is the root or places stones
 *  shows none. `top` is how far the drawing is scrolled; rows outside `viewH` are left out.
 *  While the lanes are moved under the column (`under`) a hairline at its edge sets them off. */
function paintRowNumbers(g: CanvasRenderingContext2D, walk: TreeWalk, nodes: Map<number, TreeNodeView>, margin: TreeMargin, top: number, viewH: number, under: boolean): TreeRowNumber[] {
  const p = stagePalette();
  const size = typeScale().label;
  const right = margin.x0 - TREE_NUMBER_GAP;
  const drawn: TreeRowNumber[] = [];
  if (under) {
    g.fillStyle = p.line;
    g.fillRect(margin.gutter - 1, 0, 1, viewH);
  }
  g.save();
  g.beginPath();
  g.rect(0, 0, margin.gutter - 1, viewH);
  g.clip();
  g.textBaseline = "alphabetic";
  for (const [y, n] of walk.rows) {
    const node = nodes.get(n)!;
    const row = y - top;
    if ((node.kind !== "Black" && node.kind !== "White") || row + TREE_DOT < 0 || row > viewH) continue;
    const current = y === walk.curY;
    const text = String(node.move_number);
    g.font = `${current ? "600 " : ""}${size}px ${panelFont("ui")}`;
    g.fillStyle = current ? p.text : p.text2;
    const w = tabularWidth(g, text);
    const base = row + 5.5 + size * 0.36;
    const ink = g.measureText(text);
    fillTabular(g, text, right - w, base);
    drawn.push({ node: n, text, x: right - w, y: base - ink.actualBoundingBoxAscent, w, h: ink.actualBoundingBoxAscent + ink.actualBoundingBoxDescent, current });
  }
  g.restore();
  return drawn;
}

/** VariationTree.drawTree / VariationTreeBig.drawTree from `start` at (`posx`, `posy`), Java's lane
 *  allocation. Nodes left of `minX` or above the top are not drawn. `fit` is the whole-tree view:
 *  the first node also obeys that and the main line stops above `maxY`. Every node is the same
 *  size. A node without an 错误节点 colour (it or the move before it not evaluated) is hollow; a
 *  commented node carries a small dot of the comment-node colour at its upper right; only the
 *  current node has a ring, in the interface's text colour (the root square an accent centre). */
function walkTree(
  nodes: Map<number, TreeNodeView>,
  start0: number,
  current: number,
  line: Set<number>,
  nodeColors: Map<number, number[]>,
  posx: number,
  posy0: number,
  minX: number,
  maxposy: number,
  fit: boolean,
  w: number,
  h: number,
): TreeWalk {
  const [YS, XS, DOT] = [TREE_YS, TREE_XS, TREE_DOT];
  const p = stagePalette();
  const commentDot = themeColor(appearance()?.theme.commentNodeColor, THEME_DEFAULT.commentNode, p.commentNode);
  // While the theme keeps its 错误节点 colours, each level takes the stage's colour for it.
  const themeNodes = appearance()?.theme.blunderNodeColors;
  const defaults = THEME_DEFAULT.blunderNodes;
  const ownNodes = !!themeNodes && (themeNodes.length !== defaults.length || themeNodes.some((c, k) => !sameRgba(c, defaults[k])));
  const nodeFill = (c: number[]) => {
    const level = ownNodes ? -1 : defaults.findIndex((d) => sameRgba(c, d));
    return level >= 0 ? p.treeNodes[level] : cssColor(c);
  };
  const lineColor = (n: number) => (line.has(n) ? p.treeLine : p.treeLineOff);
  const node = (n: number) => nodes.get(n)!;
  const kids = (n: number) => node(n).children;
  const moveNo = (n: number) => {
    let d = 0;
    for (let p = node(n).parent; p !== null; p = node(p).parent) d++;
    return d;
  };
  const walk: TreeWalk = { ops: [], hits: new Map(), curX: 0, curY: 0, curLane: 0, maxLane: 0, maxX: w, maxY: h, rows: new Map() };
  const { ops, hits } = walk;
  const shown = (x: number, y: number) => x > minX && y > 0;
  // LizzieFrame varTreeMaxX / varTreeMaxY: 60 px beyond the farthest node drawn
  const grow = (x: number, y: number) => {
    walk.maxX = Math.max(walk.maxX, x + 60);
    walk.maxY = Math.max(walk.maxY, y + 60);
  };
  const segment = (x1: number, y1: number, x2: number, y2: number, color: string) =>
    ops.push((g) => {
      g.strokeStyle = color;
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(x1 + 0.5, y1 + 0.5);
      g.lineTo(x2 + 0.5, y2 + 0.5);
      g.stroke();
    });
  const dot = (n: number, x: number, y: number) => {
    const dim = !line.has(n);
    const commented = look.showCommentNode && node(n).has_comment;
    const projected = nodeColors.get(n);
    const fill = projected ? nodeFill(projected) : null;
    ops.push((g) => {
      const cx = x + 5.5, cy = y + 5.5;
      if (fill) {
        g.fillStyle = fill;
        circle(g, cx, cy, DOT / 2);
        g.fill();
      } else {
        circle(g, cx, cy, DOT / 2 - 0.75);
        g.fillStyle = p.treeEmpty;
        g.fill();
        g.strokeStyle = p.treeLine;
        g.lineWidth = 1.5;
        g.stroke();
      }
      if (dim) {
        g.fillStyle = p.treeDim;
        circle(g, cx, cy, DOT / 2);
        g.fill();
      }
      if (n === current) {
        g.strokeStyle = p.text;
        g.lineWidth = 1.5;
        circle(g, cx, cy, DOT / 2 + 2.25);
        g.stroke();
      }
      if (commented) {
        // The comment dot, set off from the node (and the current node's ring) by a rim of the
        // panel's colour.
        g.globalAlpha = dim ? 0.5 : 1;
        circle(g, cx + 4, cy - 4, 2.5);
        g.fillStyle = commentDot;
        g.fill();
        g.strokeStyle = p.panelSolid;
        g.lineWidth = 1.25;
        g.stroke();
        g.globalAlpha = 1;
      }
    });
    if (!fit) grow(x, y);
  };
  // A row's number comes from its node on the current line, else from its leftmost node.
  const picked = new Map<number, { lane: number; on: boolean }>();
  const visit = (n: number, x: number, y: number, lane: number) => {
    hits.set(`${x + 5},${y + 5}`, n);
    if (n === current) [walk.curX, walk.curY, walk.curLane] = [x, y, lane];
    const on = line.has(n);
    const was = picked.get(y);
    if (!was || (on && !was.on) || (on === was.on && lane < was.lane)) {
      picked.set(y, { lane, on });
      walk.rows.set(y, n);
    }
  };
  const lanes: number[] = [];
  const drawTree = (posy: number, startLane: number, start: number, variation: number) => {
    let depth = 1;
    for (let n = start; kids(n).length; n = kids(n)[0]) depth++;
    const mn = moveNo(start);
    let lane = startLane;
    while (lane < lanes.length && lanes[lane] <= mn + depth) lanes[lane++] = mn - 1;
    if (lane >= lanes.length) lanes.push(0);
    if (variation > 1) lanes[lane - 1] = mn - 1;
    lanes[lane] = mn;
    walk.maxLane = Math.max(walk.maxLane, lane);
    const x = posx + lane * XS;
    const color = lineColor(start);
    visit(start, x, posy, lane);
    if (lane > 0) {
      if (lane - startLane > 0 || variation > 1) {
        segment(x + 5, posy + 5, x + 5 - XS, posy + 5 - YS, color);
        segment(posx + (startLane - variation) * XS + 11, posy - YS + 5, x + 5 - XS, posy + 5 - YS, color);
      } else segment(x + 5, posy + 5, x + 10 - XS, posy + 10 - YS, color);
    }
    if (!fit || shown(x, posy)) {
      if (node(start).parent !== null) {
        dot(start, x, posy);
      } else {
        // root: square, an accent centre while it is the displayed node
        ops.push((g) => {
          g.fillStyle = line.has(start) ? p.treeNode : p.treeLineOff;
          g.fillRect(x, posy, DOT, DOT);
          g.strokeStyle = p.lineStrong;
          g.lineWidth = 1;
          g.strokeRect(x - 0.5, posy - 0.5, DOT + 1, DOT + 1);
          if (start === current) {
            g.fillStyle = p.accent;
            g.fillRect(x + 2, posy + 2, DOT - 4, DOT - 4);
          }
        });
        if (!fit) grow(x, posy);
      }
    }
    let n = start;
    while (kids(n).length && (!fit || posy + YS < maxposy)) {
      posy += YS;
      n = kids(n)[0];
      visit(n, x, posy, lane);
      if (!shown(x, posy)) continue;
      dot(n, x, posy);
      segment(x + 5, posy - 1, x + 5, posy - YS + 11, lineColor(n));
    }
    while (n !== start) {
      n = node(n).parent!;
      kids(n).slice(1).forEach((c, k) => drawTree(posy, lane + k + 1, c, k + 1));
      posy -= YS;
    }
  };
  drawTree(posy0, 0, start0, 0);
  return walk;
}

/** LizzieFrame.createVarTreeImage + VariationTree.draw/drawTree (VariationTree.java:44-402): the
 * whole tree laid out from the root at (x0, 20) with Java's lane allocation, shown through the
 * scroll pane that renderVarTree positions on the current node (LizzieFrame.java:13942-13975)
 * unless the user scrolled it (`scroll`). Nodes off the current line are dimmed; with
 * show-comment-node-color commented nodes carry a dot of the comment-node colour, and every node
 * is filled with the 错误节点 colour the core projects for it (`nodeColors`,
 * LizzieFrame.getBlunderNodeColor 11465-11536 with Board.updateIsBest; VariationTree.java:136-158),
 * hollow without one; the current node carries an ink ring; 显示分支面板上的手数 (`numbers`)
 * writes each row's move number in the column at the panel's left (paintRowNumbers), which stays
 * there as the tree scrolls sideways. 设置最大宽度 (`maxWidth`, setTreeMaxLimit 13894-13908): a
 * layout that wide is cut off there and reported `tooWide` (Board.setBigBranch). Returns node
 * centres (panel coordinates) for click hit-testing, nodes under the number column left out, the
 * viewport and the row numbers drawn. */
export function paintTree(
  g: CanvasRenderingContext2D,
  r: Rect,
  nodes: Map<number, TreeNodeView>,
  root: number,
  current: number,
  line: Set<number>,
  scroll: { top: number; left: number } | null,
  numbers: boolean,
  nodeColors: Map<number, number[]>,
  maxWidth: number,
): { hits: Map<string, number>; viewport: TreeViewport; margin: TreeMargin; rowNumbers: TreeRowNumber[]; tooWide: boolean } {
  const { w, h } = r;
  const p = stagePalette();
  const margin = treeMargin(g, nodes, numbers);
  let walk = walkTree(nodes, root, current, line, nodeColors, margin.x0, 20, 0, Infinity, false, w, h);
  const tooWide = walk.maxX >= maxWidth;
  const maxX = tooWide ? maxWidth : walk.maxX;
  // VariationTree.draw: with the current lane taken as 0 (getCurLane from the root), the tree
  // moves left only when the cut width is under the first lane's end.
  const shift = Math.max(0, TREE_XS + margin.x0 + TREE_DOT - maxX);
  if (shift > 0) walk = walkTree(nodes, root, current, line, nodeColors, margin.x0 - shift, 20, 0, Infinity, false, w, h);
  const { hits, ops, curX, curY, maxY } = walk;
  // JScrollPane with AS_NEEDED bars of DemoScrollBarUI width 9; the horizontal bar is off when the
  // pane is under 100 px high (LizzieFrame.java:13976-13983).
  const hbar = maxX > w && h >= 100;
  const viewH = h - (hbar ? SCROLLBAR : 0);
  const vbar = maxY > viewH;
  const viewW = w - (vbar ? SCROLLBAR : 0);
  const clampTop = (v: number) => Math.max(0, Math.min(i(v), maxY - viewH));
  // A tree no wider than the panel (the image is the panel's width, every node 60 px short of its
  // edge) never scrolls sideways: Java's centring would move it by the vertical bar's width with no
  // horizontal bar to move it back, which here would put the first lane under the number column.
  const clampLeft = (v: number) => (maxX <= w ? 0 : Math.max(0, Math.min(i(v), maxX - viewW)));
  // renderVarTree: the current node centred in each direction; the root keeps the origin.
  const atRoot = nodes.get(current)!.parent === null;
  const top = clampTop(scroll ? scroll.top : curY <= viewH / 2 || atRoot ? 0 : curY - viewH / 2);
  const left = clampLeft(scroll ? scroll.left : curX <= viewW / 2 || atRoot ? 0 : curX - viewW / 2);
  // The cut image is maxX wide: nothing beyond it shows.
  const shownW = Math.min(viewW, maxX - left);
  // The lanes pass under the number column.
  g.save();
  g.beginPath();
  g.rect(margin.gutter, 0, shownW - margin.gutter, viewH);
  g.clip();
  g.translate(-left, -top);
  ops.forEach((op) => op(g));
  g.restore();
  const rowNumbers = numbers ? paintRowNumbers(g, walk, nodes, margin, top, viewH, shift + left > 0) : [];
  const visible = new Map<string, number>();
  for (const [k, id] of hits) {
    const [x, y] = k.split(",").map(Number);
    if (y - top >= 0 && y - top < viewH && x - left >= margin.gutter && x - left < shownW) visible.set(`${x - left},${y - top}`, id);
  }
  const vertical = vbar ? scrollbar(viewH, maxY, top) : null;
  const horizontal = hbar ? scrollbar(viewW, maxX, left) : null;
  // DemoScrollBarUI's geometry (arrow zones at both ends, the track between them) drawn as a thin
  // rounded thumb only; the arrow zones still step when pressed.
  g.fillStyle = p.scrollThumb;
  if (vertical) {
    g.beginPath();
    g.roundRect(w - SCROLLBAR + 2, ARROW + vertical.thumbStart, SCROLLBAR - 4, vertical.thumb, (SCROLLBAR - 4) / 2);
    g.fill();
  }
  if (horizontal) {
    g.beginPath();
    g.roundRect(ARROW + horizontal.thumbStart, h - SCROLLBAR + 2, horizontal.thumb, SCROLLBAR - 4, (SCROLLBAR - 4) / 2);
    g.fill();
  }
  return { hits: visible, viewport: { top, left, maxX, maxY, viewW, viewH, vertical, horizontal }, margin, rowNumbers, tooWide };
}

/** LizzieFrame.createVarTreeImage's simple view (shouldShowSimpleVariation) drawn by
 *  VariationTreeBig.draw (VariationTreeBig.java:320-367): no scroll bars; the current node sits
 *  in the middle row, the drawing starts at the ancestor near the top of the panel and stops at its
 *  bottom, and it moves left so that the current lane (and as much of the lanes to its right as
 *  fits) shows. Nodes are drawn as in the scroll pane, the row numbers in the same column at the
 *  panel's left; returns the node centres for clicks. */
export function paintTreeFit(
  g: CanvasRenderingContext2D,
  r: Rect,
  nodes: Map<number, TreeNodeView>,
  current: number,
  line: Set<number>,
  numbers: boolean,
  nodeColors: Map<number, number[]>,
): { hits: Map<string, number>; viewport: TreeViewport; margin: TreeMargin; rowNumbers: TreeRowNumber[] } {
  const { w, h } = r;
  const viewport: TreeViewport = { top: 0, left: 0, maxX: w, maxY: h, viewW: w, viewH: h, vertical: null, horizontal: null };
  const margin = treeMargin(g, nodes, numbers);
  if (w <= 0 || h <= 0) return { hits: new Map(), viewport, margin, rowNumbers: [] };
  const node = (n: number) => nodes.get(n)!;
  const depth = (n: number) => {
    let d = 0;
    for (let p = node(n).parent; p !== null; p = node(p).parent) d++;
    return d;
  };
  // BoardHistoryNode.findTop: the highest branching point the current line leaves the first
  // children at; the node itself on the main line.
  let top = current;
  for (let s = current, p = node(s).parent; p !== null; s = p, p = node(s).parent) {
    if (node(p).children[0] !== s) top = p;
  }
  let posy = i(h / 2) - TREE_YS * (depth(current) - depth(top));
  let start = top;
  while (posy > -TREE_YS && node(start).parent !== null) {
    start = node(start).parent!;
    posy -= TREE_YS;
  }
  // The dummy drawing finds the current lane and the widest lane.
  const { curLane: lane, maxLane } = walkTree(nodes, start, current, line, nodeColors, 0, posy, 0, h, true, w, h);
  const x0 = margin.x0;
  let startx = x0;
  const over = (lane + 1) * TREE_XS + x0 + TREE_DOT - w;
  if (over > 0) startx -= over;
  if (maxLane * TREE_XS + x0 + TREE_DOT > w) {
    const half = i((w - x0 - TREE_DOT) / 2);
    if ((maxLane - lane) * TREE_XS < half) startx -= (maxLane - lane) * TREE_XS;
    else if (lane * TREE_XS + x0 + TREE_DOT > w) startx -= half;
  }
  const walk = walkTree(nodes, start, current, line, nodeColors, startx, posy, 0, h, true, w, h);
  g.save();
  g.beginPath();
  g.rect(margin.gutter, 0, w - margin.gutter, h);
  g.clip();
  walk.ops.forEach((op) => op(g));
  g.restore();
  const rowNumbers = numbers ? paintRowNumbers(g, walk, nodes, margin, 0, h, startx < x0) : [];
  const hits = new Map<string, number>();
  for (const [k, id] of walk.hits) {
    const [x, y] = k.split(",").map(Number);
    if (x >= margin.gutter && x < w && y >= 0 && y < h) hits.set(k, id);
  }
  return { hits, viewport, margin, rowNumbers };
}

/** Scroll bar width and arrow button length of DemoScrollBarUI. */
export const SCROLLBAR = 9;
export const ARROW = 11;

/** Track length between the arrows, thumb length and thumb offset for a bar of `length`. */
export interface ScrollbarGeom {
  track: number;
  thumb: number;
  thumbStart: number;
}

function scrollbar(length: number, content: number, offset: number): ScrollbarGeom {
  const track = Math.max(0, length - 2 * ARROW);
  const thumb = Math.min(track, Math.max(8, i((track * length) / content)));
  const thumbStart = i(((track - thumb) * offset) / Math.max(1, content - length));
  return { track, thumb, thumbStart };
}

/** The visible part of the variation tree and its scroll bars, for scrolling and hit tests. */
export interface TreeViewport {
  top: number;
  left: number;
  maxX: number;
  maxY: number;
  viewW: number;
  viewH: number;
  vertical: ScrollbarGeom | null;
  horizontal: ScrollbarGeom | null;
}
