// Canvas painters reproducing the fixed Java drawing (wimi321/lizzieyzy-next cb53acc).
// Analysis presentation consumes node-persisted projections, independent of engine pondering.
import type {
  BoardOverlayView,
  CandidateView,
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
  TrackingSettings,
  TreeNodeView,
} from "../contract/generated";
import { language, t } from "../i18n";
import { appearance, onAppearanceChange } from "../appearance/state";
import { chineseClassicLabel, cssColor, customFont, drawChineseClassicMarkers, javaHue, localeFonts, opaque, rankColor, reverseColor, THEME_DEFAULT, themeColor, themedFont } from "../appearance/model";
import { stagePalette, stageWallpaper, type StagePalette } from "../design/stage";
import type { BoardGeom, Rect } from "./layout";

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
}

/** What the main board drew, for the acceptance probe. */
export interface BoardReport {
  coordinates: boolean;
  /** Below the board: player names, the pure network line, or nothing. */
  bottomLine: "names" | "rawNet" | null;
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

const patterns = new WeakMap<CanvasRenderingContext2D, Map<HTMLImageElement, CanvasPattern>>();

/** Tile `img` over the rect with the texture anchored at (ax, ay) (TexturePaint semantics). */
function drawTexture(g: CanvasRenderingContext2D, img: HTMLImageElement | undefined, x: number, y: number, w: number, h: number, ax = x, ay = y) {
  if (!img?.naturalWidth) {
    g.fillStyle = "rgb(217,152,77)";
    g.fillRect(x, y, w, h);
    return;
  }
  let byImage = patterns.get(g);
  if (!byImage) patterns.set(g, (byImage = new Map()));
  let pattern = byImage.get(img);
  if (!pattern) byImage.set(img, (pattern = g.createPattern(img, "repeat")!));
  pattern.setTransform(new DOMMatrix([1, 0, 0, 1, ax, ay]));
  g.fillStyle = pattern;
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
 * rounding to (int)(v + 0.5). Canvas `filter` is not used: WebKitGTK ignores it. */
function blurRegion(g: CanvasRenderingContext2D, r: Rect, radius: number) {
  const dpr = window.devicePixelRatio || 1;
  const rad = radius * dpr;
  const [x, y, w, h] = [Math.round(r.x * dpr), Math.round(r.y * dpr), Math.round(r.w * dpr), Math.round(r.h * dpr)];
  const reach = Math.ceil(rad);
  const sigma = rad / 3;
  const kernel = Array.from({ length: reach * 2 + 1 }, (_, k) => {
    const d = (k - reach) ** 2;
    return d > rad * rad ? 0 : Math.exp(-d / (2 * sigma * sigma)) / Math.sqrt(2 * Math.PI * sigma);
  });
  const total = kernel.reduce((a, b) => a + b, 0);
  const weights = kernel.map((v) => v / total);
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
}

/** GlassEffectRenderer.drawLiquidEffects (GlassEffectRenderer.java:226-266) at the moment the
 *  background is painted: the moving highlight (6 s cycle) and the top / bottom inner shadows. */
function liquidEffects(g: CanvasRenderingContext2D, r: Rect) {
  const phase = (Date.now() % 6000) / 6000;
  const cx = r.x + r.w * (0.5 + 0.3 * Math.sin(phase * Math.PI * 2));
  const cy = r.y + r.h * (0.5 + 0.3 * Math.cos(phase * Math.PI * 2 * 0.7));
  const highlight = g.createRadialGradient(cx, cy, 0, cx, cy, Math.max(r.w, r.h) * 0.5);
  highlight.addColorStop(0, "rgba(255,255,255,0.196)");
  highlight.addColorStop(0.5, "rgba(255,255,255,0.059)");
  highlight.addColorStop(1, "rgba(255,255,255,0)");
  g.fillStyle = highlight;
  g.beginPath();
  g.roundRect(r.x, r.y, r.w, r.h, 8);
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

/** LizzieFrame.createBackground (wallpaper tiled from the panel origin) plus drawContainer for each
 * panel (LizzieFrame.java:8221-8245): with glass-effect-level above 0,
 * GlassEffectRenderer.drawGlassPanel (corner 16) FROSTED (glass-blur-radius 10) or, from level 2,
 * LIQUID (liquid-blur-radius 15 plus its highlight and inner shadows), overlay (24,24,26,102),
 * border (255,255,255,26), highlight (255,255,255,77); at level 0 only the region blurred by the
 * theme's background-filter. The pure background colour replaces the wallpaper and the containers
 * (LizzieFrame.java:8197-8208).
 * The program's own stage tiles its wallpaper for the colour mode instead and leaves it sharp
 * under the panels: the material shows through them.
 * Painted once per size into a detached canvas and returned as a PNG blob: a live full-window canvas
 * layer would be recomposited every frame by WebKitGTK. */
export function paintBackground(w: number, h: number, glass: Rect[]): Promise<Blob | null> {
  if (stagePalette().flat) return Promise.resolve(null);
  const canvas = document.createElement("canvas");
  const g = prepare(canvas, { x: 0, y: 0, w, h });
  if (look.pureBackground && !builtin) {
    g.fillStyle = look.pureBackground;
    g.fillRect(0, 0, w, h);
    glass = [];
  } else drawTexture(g, builtin ? stageWallpaper() : look.background, 0, 0, w, h, 0, 0);
  const glassy = look.glassLevel > 0;
  const liquid = look.glassLevel >= 2;
  for (const r of glass) {
    if (r.w <= 0 || r.h <= 0 || r.x < 0 || r.y < 0 || r.x + r.w > w || r.y + r.h > h) continue;
    if (!glassy) {
      if (builtin) continue;
      // GaussianFilter(0) leaves the region as it is.
      if (look.backgroundFilter > 0) blurRegion(g, r, look.backgroundFilter);
      continue;
    }
    // drawGlassPanel: the theme's glass values (Theme.java:738-767); the panel corner is the 16
    // drawContainer passes, not glass-corner-radius.
    const radius = liquid ? look.liquidBlur : look.glassBlur;
    if (radius > 0) blurRegion(g, r, radius);
    g.fillStyle = look.glassOverlay;
    g.beginPath();
    g.roundRect(r.x, r.y, r.w, r.h, 8);
    g.fill();
    g.strokeStyle = look.glassBorder;
    g.lineWidth = 1;
    g.beginPath();
    g.roundRect(r.x + 0.5, r.y + 0.5, r.w, r.h, 8);
    g.stroke();
    const light = g.createLinearGradient(0, r.y, 0, r.y + r.h * 0.3);
    light.addColorStop(0, look.glassHighlight);
    light.addColorStop(1, "rgba(255,255,255,0)");
    g.fillStyle = light;
    g.beginPath();
    g.roundRect(r.x + 1, r.y + 1, r.w - 2, i(r.h / 3), 8);
    g.fill();
    if (liquid) liquidEffects(g, r);
  }
  const { promise, resolve } = Promise.withResolvers<Blob | null>();
  canvas.toBlob(resolve);
  return promise;
}

/** BoardRenderer.drawShadow with show-stone-shadow and shadow-size (BoardRenderer.java:3796-3860):
 *  the cached shadow grows with cachedR = stoneRadius * shadow-size / 100; at the default 85 it is
 *  the shape measured on the Java capture. */
function stoneShadow(g: CanvasRenderingContext2D, cx: number, cy: number, r: number) {
  if (!look.showShadow) return;
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
 *  a black or white disc, the white one outlined max(r / 16, 1) (BoardRenderer.java:3901-3916). */
function drawStone(g: CanvasRenderingContext2D, cx: number, cy: number, r: number, black: boolean) {
  stoneShadow(g, cx, cy, r);
  const img = black ? look.black : look.white;
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

/** MoveRankDefinition boardMarkRadiusFactor; the colours are rankColor's. */
const RANK_RADIUS: Record<MoveRank, number> = { Best: 0.1, Good: 0.1, Normal: 0.1225, Inaccuracy: 0.145, Mistake: 0.1675, Blunder: 0.19 };

/** Selection squares: allowed blue, avoided red, alpha 45 (BoardRenderer.java:1172-1177,
 *  1201-1212). */
const REGION_COLOR = { allow: "rgba(0,0,120,0.1765)", avoid: "rgba(120,0,0,0.1765)" };

/** drawAllSelectedRectByCoords and the dragged drawSelectedRect: a square of the square width
 *  centred on each point, the dragged rectangle over the points it covers. */
function drawRegion(g: CanvasRenderingContext2D, s: BoardScene, X: (x: number) => number, Y: (y: number) => number, sq: number, report: BoardReport): void {
  const half = i(sq / 2);
  if (s.region?.points.length) {
    g.fillStyle = s.region.allow ? REGION_COLOR.allow : REGION_COLOR.avoid;
    for (const p of s.region.points) g.fillRect(X(p.x) - half, Y(p.y) - half, sq, sq);
    report.region = { allow: s.region.allow, points: s.region.points.length };
  }
  const d = s.selecting;
  if (d) {
    g.fillStyle = d.allow ? REGION_COLOR.allow : REGION_COLOR.avoid;
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
      marker.outline = p.rank ? rankColor(p.rank) : "rgb(112,118,124)";
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
 *  the move itself as a stone of the side to play with its numbers in the opposite colour. */
function drawNextFirstMove(g: CanvasRenderingContext2D, s: BoardScene, geo: BoardGeom, X: (x: number) => number, Y: (y: number) => number, report: BoardReport) {
  const info = s.overlay.next_move!;
  const x = X(info.point.x), y = Y(info.point.y);
  const black = s.toPlay === "Black";
  drawStone(g, x, y, geo.stoneRadius, black);
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
    // names and the line.
    if (d.coordinates) {
      g.fillStyle = "#000";
      const at = i((geo.margin * 4) / 10);
      const atY = i((marginY * 4) / 10);
      const bottom = !s.names && !rawLine;
      const labels = look.chinese
        ? { cols: Array.from({ length: geo.cols }, (_, k) => chineseClassicLabel(k, geo.cols)), rows: Array.from({ length: geo.rows }, (_, k) => chineseClassicLabel(k, geo.rows)) }
        : coordinateLabels(d.coordinateStyle, geo.cols, geo.rows);
      labels.cols.forEach((label, k) => {
        jString(g, label, X(k), oy + atY, UI_FONT, i((r * 4) / 5), r);
        if (bottom) jString(g, label, X(k), oy + sizeY - atY, UI_FONT, i((r * 4) / 5), r);
      });
      labels.rows.forEach((label, k) => {
        jString(g, label, ox + at, Y(k), UI_FONT, i((r * 4) / 5), r);
        jString(g, label, ox + geo.size - at, Y(k), UI_FONT, i((r * 4) / 5), r);
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
    drawName(g, s, ox, oy, geo);
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
  if (layers.stones) {
    const onlyNew = layers.float && !layers.float.edit && !layers.float.setup ? preview?.numbers ?? null : null;
    for (let p = 0; p < stones.length; p++) {
      const stone = stones[p];
      if (stone && (!onlyNew || onlyNew.has(p))) drawStone(g, px(p), py(p), r, stone === "Black");
    }
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
  if (nextHover) drawNextFirstMove(g, s, geo, X, Y, report);
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
  return report;
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
    g.fillStyle = rankColor(m.rank);
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

/** BoardRenderer.drawName (BoardRenderer.java:802-934): both name stones (the side to move drawn on
 * top), the black name in black, the white name in white with a 2 px black outline. Names longer
 * than 0.43 of the board are cut by the Java step. */
function drawName(g: CanvasRenderingContext2D, scene: BoardScene, ox: number, oy: number, geo: BoardGeom) {
  const { size, margin, stoneRadius: r, square, n } = geo;
  g.font = `${i(Math.min(28, (margin * 53) / 100))}px ${GRAPH_FONT}`;
  const fm = javaMetrics(g);
  const width = (s: string) => Math.round(g.measureText(s).width);
  const limit = square * n * 0.43;
  const cut = (s: string) => {
    while (s && width(s) > limit) s = s.slice(0, s.length - Math.max(1, i((width(s) - square * i(n / 2)) / i(width(s) / s.length))));
    return s;
  };
  const black = cut(scene.black);
  const white = cut(scene.white);
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
  if (black) {
    g.fillStyle = "#000";
    g.fillText(black, ox + i(size / 2) - width(black) - i((margin * 4) / 5), baseY);
  }
  if (white) {
    const x = ox + i(size / 2) + i((margin * 4) / 5) + 1;
    g.strokeStyle = "#000";
    g.lineWidth = 2;
    g.lineJoin = "miter";
    g.strokeText(white, x, baseY);
    g.fillStyle = "#fff";
    g.fillText(white, x, baseY);
  }
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

/** LizzieFrame.drawStringMid (LizzieFrame.java:12392-12429), aboveOrBelow 0: the 12 px font is
 * scaled to span maxW, capped at maxH and at frameFontSize + 6, centred on x. */
function stringMid(g: CanvasRenderingContext2D, s: string, x: number, y: number, maxH: number, maxW: number, family = panelFont()) {
  g.font = `12px ${family}`;
  let size = Math.min(maxH, Math.round((12 * maxW) / g.measureText(s).width));
  if (Math.round(size) > 18) size = 18;
  g.font = `${size}px ${family}`;
  const fm = javaMetrics(g);
  g.fillText(s, x - i(Math.round(g.measureText(s).width) / 2), y + i((fm.ascent - fm.descent) / 2));
}

/** A panel's ground: the palette's panel colour over the whole rect. */
function panelGround(g: CanvasRenderingContext2D, w: number, h: number, p: StagePalette) {
  g.fillStyle = p.panel;
  g.fillRect(0, 0, w, h);
}

/** On the flat stage a panel is a flat region with a 1px square edge just inside its rect. */
function panelEdge(g: CanvasRenderingContext2D, w: number, h: number, p: StagePalette) {
  if (!p.flat || w < 2 || h < 2) return;
  g.strokeStyle = p.line;
  g.lineWidth = 1;
  g.setLineDash([]);
  g.strokeRect(0.5, 0.5, w - 1, h - 1);
}

/** The widest digit's advance in the current font: canvas text has no font-variant-numeric, so
 *  tabular figures are laid out by hand. */
function digitAdvance(g: CanvasRenderingContext2D): number {
  let widest = 0;
  for (const d of "0123456789") widest = Math.max(widest, g.measureText(d).width);
  return widest;
}

/** Width of `s` with tabular figures (fillTabular). */
function tabularWidth(g: CanvasRenderingContext2D, s: string): number {
  const digit = digitAdvance(g);
  let width = 0;
  for (const c of s) width += c >= "0" && c <= "9" ? digit : g.measureText(c).width;
  return Math.round(width);
}

/** fillText with tabular figures: every digit centred in the widest digit's advance. */
function fillTabular(g: CanvasRenderingContext2D, s: string, x: number, y: number) {
  const digit = digitAdvance(g);
  for (const c of s) {
    const own = g.measureText(c).width;
    if (c >= "0" && c <= "9") {
      g.fillText(c, x + (digit - own) / 2, y);
      x += digit;
    } else {
      g.fillText(c, x, y);
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

/** drawCaptured's AI总评分 text (LizzieFrame.AIscore + "%.1f"). */
export function aiScoreText(score: number): string {
  return `${t("LizzieFrame.AIscore")}${score.toFixed(1)}`;
}

/** Java String.valueOf(double) for komi values: whole numbers keep ".0". */
export function javaDouble(v: number): string {
  return Number.isInteger(v) ? v.toFixed(1) : String(v);
}

/** Double.parseDouble: surrounding blanks allowed, a signed decimal with an optional exponent and
 *  float / double suffix; NaN and Infinity are refused (Java's limits reject them too). */
export function parseJavaDouble(text: string): number | null {
  const s = text.trim();
  if (!/^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?[fFdD]?$/.test(s)) return null;
  const v = Number(s.replace(/[fFdD]$/, ""));
  return Number.isFinite(v) ? v : null;
}

/** LizzieFrame.drawCaptured (LizzieFrame.java:9098-9375): captures, rules caption, komi and
 * the live pondering dot. Stored analysis is intentionally independent of this indicator. On the
 * flat stage the stones are flat discs with a 1px edge (white always; black where the panel is
 * dark), the capture counts and AI总评分 primary text and komi and the caption secondary. */
export function paintCaptured(g: CanvasRenderingContext2D, r: Rect, s: CapturedScene) {
  const p = stagePalette();
  const { w: width, h: height } = r;
  panelGround(g, width, height, p);
  g.strokeStyle = p.line;
  g.lineWidth = 1;
  g.beginPath();
  g.moveTo(i(width / 2) - 1 + 0.5, 3);
  g.lineTo(i(width / 2) - 1 + 0.5, height - 1);
  g.stroke();
  panelEdge(g, width, height, p);
  const blacksTurn = s.toPlay === "Black";
  const diam0 = i(Math.min(i(width / 2), height) / 3);
  const small = i(diam0 / 2);
  const big = i((small * 3) / 2);
  const diam = i((diam0 * 3) / 2);
  const bd = i(((blacksTurn ? big : small) * 3) / 2);
  const wd = i(((blacksTurn ? small : big) * 3) / 2);
  const oval = (color: string, edge: string | null, cx: number, d: number) => {
    const x = cx - i(d / 2) + d / 2, y = i((diam - d) / 2) + d / 2;
    g.fillStyle = color;
    circle(g, x, y, d / 2);
    g.fill();
    if (!edge || d < 3) return;
    g.strokeStyle = edge;
    g.lineWidth = 1;
    circle(g, x, y, d / 2 - 0.5);
    g.stroke();
  };
  oval(p.stoneBlack, p.flat && p.stoneBlackEdge !== p.stoneBlack ? p.stoneBlackEdge : null, i(width / 4), bd);
  oval(p.stoneWhite, p.flat ? p.lineStrong : null, i((width * 3) / 4), wd);
  const statusDiam = i(height / 4) < 10 ? i(height / 4) : 10;
  g.fillStyle = s.analysing ? p.live : p.idle;
  circle(g, i(width / 2) - 1 - i(statusDiam / 2) + statusDiam / 2, i((height * 7) / 26) + i((diam - statusDiam) / 2) + statusDiam / 2, statusDiam / 2);
  g.fill();
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  const font = panelFont();
  if (s.small) {
    // The small strip has no capture counts; AI总评分 sits lower and larger, komi and the
    // move / rules caption use min(w * 0.4, h * 0.85) * 0.2 capped at frameFontSize + 6.
    g.fillStyle = p.text;
    if (s.aiScoreBlack !== null) stringMid(g, aiScoreText(s.aiScoreBlack), i(width / 4), i((height * 5) / 7), i((height * 2) / 5), i((width * 4) / 10));
    if (s.aiScoreWhite !== null) stringMid(g, aiScoreText(s.aiScoreWhite), i((width * 3) / 4), i((height * 5) / 7), i((height * 2) / 5), i((width * 4) / 10));
    g.fillStyle = p.text2;
    g.font = `${i(Math.min(18, Math.max(Math.min(width * 0.4, height * 0.85) * 0.2, 11)))}px ${font}`;
    const komi = javaDouble(s.komi);
    g.fillText(komi, i(width / 2) - 1 - i(Math.round(g.measureText(komi).width) / 2), i((height * 15) / 16));
    const caption = s.rules ?? `${s.moveNumber}`;
    g.fillText(caption, i(width / 2) - 1 - i(Math.round(g.measureText(caption).width) / 2), i((height * 5) / 16));
    return;
  }
  g.fillStyle = p.text;
  stringMid(g, `${t("LizzieFrame.captures")}${s.blackCaptures}`, i(width / 4), i((height * 28) / 32), i(height / 6), i((width * 3) / 10));
  stringMid(g, `${t("LizzieFrame.captures")}${s.whiteCaptures}`, i((width * 3) / 4), i((height * 28) / 32), i(height / 6), i((width * 3) / 10));
  if (s.aiScoreBlack !== null) stringMid(g, aiScoreText(s.aiScoreBlack), i(width / 4), i((height * 19) / 32), i(height / 5), i((width * 4) / 10));
  if (s.aiScoreWhite !== null) stringMid(g, aiScoreText(s.aiScoreWhite), i((width * 3) / 4), i((height * 19) / 32), i(height / 5), i((width * 4) / 10));
  g.fillStyle = p.text2;
  g.font = `${i(Math.max(11, height * 0.18))}px ${font}`;
  const komi = javaDouble(s.komi);
  g.fillText(komi, i(width / 2) - 1 - i(Math.round(g.measureText(komi).width) / 2), i((height * 7) / 8));
  const caption = s.rules ?? `${s.moveNumber}`;
  stringMid(g, caption, i(width / 2) - 1, i((height * 3) / 10), i(height / 5), i(width / 2));
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
}

/** Java's sign rule: "+" only above zero (a zero change reads "-0.0"). */
function javaSigned(v: number): string {
  return `${v > 0 ? "+" : "-"}${Math.abs(v).toFixed(1)}`;
}

/** LizzieFrame.drawMoveStatistics (LizzieFrame.java:8702-9025) outside play mode: the score and
 *  last-move line, the black/white bar with its 10% dashed grid, the change marker and the
 *  percentages. Unknown values are left out instead of being drawn as zero. On the flat stage the
 *  bar has a 1px edge and the percentages tabular figures. */
export function paintStat(g: CanvasRenderingContext2D, r: Rect, s: StatScene): StatReport {
  const { w, h } = r;
  const report: StatReport = { text: "", bar: null, change: null, moveNumber: false };
  if (w < 10 || h < 5) return report;
  const p = stagePalette();
  panelGround(g, w, h, p);
  if (p.flat) panelEdge(g, w, h, p);
  else {
    g.strokeStyle = p.line;
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(1, 1.5);
    g.lineTo(w - 1, 1.5);
    g.stroke();
  }
  const x0 = 2, y0 = 2;
  const width = w - 4;
  const height = h - 4;
  let text = "";
  if (s.kata && s.lead !== null) text += `${s.leadWithKomi ? t("LizzieFrame.scoreLeadWithKomi") : t("LizzieFrame.scoreLeadJustScore")}${s.lead.toFixed(1)}`;
  text += ` ${t("LizzieFrame.display.lastMove")}${s.matched ? `(#${s.matched})` : "(#  )"}`;
  if (s.change !== null) {
    text += `: ${javaSigned(s.change)}%`;
    if (s.kata && s.scoreChange !== null) text += ` ${javaSigned(s.scoreChange)}${t("LizzieFrame.pts")}`;
  }
  report.text = text;
  // LizzieFrame.drawString: fit width * 20 / 21 at 12 px, capped at height / 4 and frame font + 4
  const font = panelFont();
  g.font = `12px ${font}`;
  const fit = Math.round((12 * i((width * 20) / 21)) / g.measureText(text).width);
  g.font = `${Math.min(i(height / 4), fit, 16)}px ${font}`;
  let fm = javaMetrics(g);
  g.fillStyle = p.text;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(text, x0, y0 + i((height * 17) / 20) + i((fm.ascent - fm.descent) / 2));
  const textWidth = (str: string) => Math.round(g.measureText(str).width);
  const moveNumber = String(s.moveNumber);
  if (s.black === null) {
    if (s.moveNumberDown) {
      g.font = `${i(Math.min(width * 0.63, height) * 0.24)}px ${panelFont("ui")}`;
      fm = javaMetrics(g);
      const swM = textWidth(moveNumber);
      if (width > 2 * swM) {
        g.fillText(moveNumber, x0 + i((width - swM) / 2), y0 + i(height / 6) + i((fm.ascent - fm.descent) / 2));
        report.moveNumber = true;
      }
    }
    return report;
  }
  const maxW = width;
  const barW = i((s.black * maxW) / 100);
  const barWW = i(((100 - s.black) * maxW) / 100);
  const barY = y0 + i(height / 3), barH = i(height / 3);
  const bx = x0, wx = bx + barW;
  g.fillStyle = p.stoneWhite;
  g.fillRect(wx, barY, barWW, barH);
  g.fillStyle = p.stoneBlack;
  g.fillRect(bx, barY, barW, barH);
  if (s.change !== null && s.before !== null) {
    const lastX = bx + i((s.before * maxW) / 100);
    const diffX = Math.min(wx, lastX), diffW = Math.abs(wx - lastX);
    if (diffW > 0) {
      const big = barH > 30, k = big ? 5 : 3, tip = big ? 7 : 5;
      const gain = s.change >= 0;
      report.change = gain ? "gain" : "loss";
      const color = gain ? p.gain : p.loss;
      g.strokeStyle = color;
      g.fillStyle = color;
      g.lineWidth = big ? 2 : 1;
      const o = big ? 0 : 0.5;
      const line = (xa: number, xb: number) => {
        g.beginPath();
        g.moveTo(xa, barY + o);
        g.lineTo(xb + 1, barY + o);
        g.stroke();
      };
      const triangle = (xs: number, xTip: number, yTip: number) => {
        g.beginPath();
        g.moveTo(xs, barY + 1 - k);
        g.lineTo(xs, barY + 1 + k);
        g.lineTo(xTip, yTip);
        g.closePath();
        g.fill();
      };
      // The arrow points from the win rate before the move to the one now (rightTri).
      if (wx > lastX) {
        if (diffW > 3) line(diffX, diffX + diffW - 3);
        triangle(Math.max(diffX, diffX + diffW - tip), diffX + diffW, barY);
      } else {
        const end = diffX + diffW - 1;
        if (diffW > 3) line(diffX + 2, end);
        triangle(Math.min(end + 1, diffX + tip), diffX, barY + 1);
      }
      if (diffW > tip) {
        g.strokeStyle = p.barGrid;
        g.beginPath();
        g.moveTo(lastX + o, barY);
        g.lineTo(lastX + o, barY + barH);
        g.stroke();
      }
    }
  }
  g.font = `${i(Math.min(maxW * 0.63, height) * 0.24)}px ${panelFont("ui")}`;
  fm = javaMetrics(g);
  const fontHeight = fm.ascent - fm.descent;
  const textY = y0 + barH - i((barH - fontHeight) / 2);
  const winB = `${s.black.toFixed(1)}%`, winW = `${(100 - s.black).toFixed(1)}%`;
  report.bar = { black: winB, white: winW };
  // The percentages sit above the bar, on the panel.
  const figures = (str: string) => (p.flat ? tabularWidth(g, str) : textWidth(str));
  const draw = (str: string, x: number) => (p.flat ? fillTabular(g, str, x, textY) : g.fillText(str, x, textY));
  g.fillStyle = p.text;
  draw(winB, bx + 4);
  const swW = figures(winW);
  draw(winW, bx + maxW - swW - 4);
  if (s.moveNumberDown) {
    const swM = figures(moveNumber);
    if (maxW > 2 * swM + figures(winB) + swW) {
      draw(moveNumber, bx + i((maxW - swM) / 2));
      report.moveNumber = true;
    }
  }
  // winRateGridLines = 9 dashed grey lines at the tenths (maxBarwidth / 10 in int arithmetic).
  g.strokeStyle = p.barGrid;
  g.lineWidth = 1;
  g.setLineDash([4, 4]);
  g.beginPath();
  for (let k = 1; k <= 9; k++) {
    const x = bx + k * i(maxW / 10) + 0.5;
    g.moveTo(x, barY);
    g.lineTo(x, barY + barH);
  }
  g.stroke();
  g.setLineDash([]);
  if (p.flat) {
    g.strokeStyle = p.lineStrong;
    g.strokeRect(bx + 0.5, barY + 0.5, barW + barWW - 1, barH - 1);
  }
  return report;
}

/** drawMoveStatistics in play mode (LizzieFrame.java:8705-8736): the panel strip and, while the
 *  human's clock runs, its text centred in it (drawString fitted to the strip, at most frame
 *  font + 4). */
export function paintPlayStat(g: CanvasRenderingContext2D, r: Rect, clock: string | null): void {
  const { w, h } = r;
  if (w < 10 || h < 5) return;
  const p = stagePalette();
  panelGround(g, w, h, p);
  if (p.flat) panelEdge(g, w, h, p);
  else {
    g.strokeStyle = p.line;
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(1, 1.5);
    g.lineTo(w - 1, 1.5);
    g.stroke();
  }
  if (!clock) return;
  const font = panelFont("ui");
  g.font = `12px ${font}`;
  g.font = `${Math.min(h, (12 * w) / g.measureText(clock).width, 16)}px ${font}`;
  const fm = javaMetrics(g);
  g.fillStyle = p.text;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(clock, i((w - Math.round(g.measureText(clock).width)) / 2), i(h / 2) + i((fm.ascent - fm.descent) / 2));
}

/** Line height, size, left inset and bottom inset of the status lines (13 px text on 20 px lines). */
const STATUS = { line: 20, size: 13, minSize: 11, left: 12, bottom: 8 };

/** Pondering status lines at ponderingY (LizzieFrame.drawPonderingState), a block resting on the
 *  status area's bottom: the engine line in primary text at weight 600, the others secondary.
 *  Without room for full lines they share the height, the text never under 11 px. On the
 *  wallpaper the block sits on a translucent black ground in white text. The face is the theme's
 *  font-name when it names one (LizzieFrame.createStatusFont, LizzieFrame.java:8432-8448). */
export function paintStatus(g: CanvasRenderingContext2D, r: Rect, lines: string[]) {
  if (!lines.length) return;
  const p = stagePalette();
  const room = (r.h - STATUS.bottom) / lines.length;
  const lh = Math.max(STATUS.minSize + 2, Math.min(STATUS.line, Math.floor(room)));
  const size = Math.max(STATUS.minSize, Math.min(STATUS.size, Math.round((lh * STATUS.size) / STATUS.line)));
  const top = r.h - STATUS.bottom - lines.length * lh;
  const family = panelFont("info");
  const fonts = lines.map((_, k) => `${k === 0 ? "600 " : ""}${size}px ${family}`);
  g.textBaseline = "middle";
  g.textAlign = "left";
  if (!p.flat) {
    let widest = 0;
    lines.forEach((s, k) => {
      g.font = fonts[k];
      widest = Math.max(widest, g.measureText(s).width);
    });
    g.fillStyle = "rgba(0,0,0,0.6)";
    g.fillRect(STATUS.left - 6, top - 4, Math.ceil(widest) + 12, lines.length * lh + 8);
  }
  lines.forEach((s, k) => {
    g.font = fonts[k];
    g.fillStyle = !p.flat ? "#fff" : k === 0 ? p.text : p.text2;
    g.fillText(s, STATUS.left, top + k * lh + lh / 2);
  });
}

/** LizzieFrame.drawPonderingStateForExtraMode2 (LizzieFrame.java:8324-8354) with fitStatusFont /
 *  truncateStatusText (8405-8430, 8553-8577): one status box of 双引擎模式 at (x, y) of a panel
 *  `panelW` x `panelH`. The font (plain, `size`) shrinks to at most 10 to fit the width left on its
 *  board's side (`split`, where the right board starts), then the text is cut with "..."; the box
 *  is the panel ground with a 1px outline, the text primary and centred. Returns the text as drawn
 *  and its width (0: nothing). Java split at half the panel, so a window wider than the two boards
 *  cut the secondary engine's box (starting at the right board) down to a few characters. */
export function paintDualStrip(g: CanvasRenderingContext2D, text: string, x: number, y: number, size: number, split: number, panelW: number, panelH: number): { text: string; width: number } {
  const rightEdge = x < split ? split : panelW;
  const maxWidth = Math.max(1, rightEdge - x - 4);
  // fitStatusFont over createStatusFont (LizzieFrame.java:8405-8448): the theme's font-name first.
  const family = panelFont("info");
  const font = (px: number) => `${px}px ${family}`;
  const measure = (s: string) => Math.round(g.measureText(s).width);
  let px = size;
  g.font = font(px);
  const measured = measure(text);
  if (text && measured > maxWidth) {
    const least = Math.min(size, 10);
    px = Math.max(least, Math.min(size, Math.floor((size * maxWidth) / measured)));
    g.font = font(px);
    while (px > least && measure(text) > maxWidth) g.font = font(--px);
  }
  let shown = text;
  if (measure(shown) > maxWidth) {
    const dots = measure("...");
    if (dots > maxWidth) shown = "";
    else {
      let low = 0;
      let high = text.length;
      while (low < high) {
        const middle = i((low + high + 1) / 2);
        if (measure(text.substring(0, middle)) + dots <= maxWidth) low = middle;
        else high = middle - 1;
      }
      const prefix = text.substring(0, low).trim();
      shown = prefix ? `${prefix}...` : "...";
    }
  }
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
  g.fillRect(drawX, drawY, width, height);
  g.strokeStyle = p.line;
  g.lineWidth = 1;
  g.strokeRect(drawX + 0.5, drawY + 0.5, width, height);
  g.fillStyle = p.text;
  g.textAlign = "left";
  g.textBaseline = "alphabetic";
  g.fillText(shown, drawX + (width - stringWidth) / 2, drawY + i((height - fontHeight) / 2) + ascent);
  return { text: shown, width: stringWidth };
}

/** One drawing of VariationTree.drawTree (scroll pane) or VariationTreeBig.drawTree (whole tree
 *  in the panel): the deferred drawing, the centre of every node walked (for click hit-testing,
 *  Java's calc pass), the current node's place and lane and the extent of what was drawn. */
interface TreeWalk {
  ops: ((g: CanvasRenderingContext2D) => void)[];
  hits: Map<string, number>;
  curX: number;
  curY: number;
  curLane: number;
  maxLane: number;
  maxX: number;
  maxY: number;
  numbers: number;
}

const [TREE_YS, TREE_XS, TREE_DOT, TREE_CENTER, TREE_RING] = [20, 20, 11, 5, 15];

/** VariationTree.drawTree / VariationTreeBig.drawTree from `start` at (`posx`, `posy`), Java's lane
 *  allocation. Nodes left of `minX` or above the top are not drawn. `fit` is the whole-tree view:
 *  the first node also obeys that, the main line stops above `maxY`, and the current node takes
 *  VariationTreeBig's red (or reversed 错误节点 colour) centre instead of the black one. */
function walkTree(
  nodes: Map<number, TreeNodeView>,
  start0: number,
  current: number,
  line: Set<number>,
  numbers: boolean,
  nodeColors: Map<number, number[]>,
  posx: number,
  posy0: number,
  minX: number,
  maxposy: number,
  fit: boolean,
  w: number,
  h: number,
): TreeWalk {
  const [YS, XS, DOT, CENTER, RING] = [TREE_YS, TREE_XS, TREE_DOT, TREE_CENTER, TREE_RING];
  const p = stagePalette();
  const commentRing = themeColor(appearance()?.theme.commentNodeColor, THEME_DEFAULT.commentNode, p.commentNode);
  const lineColor = (n: number) => (line.has(n) ? p.treeLine : p.treeLineOff);
  const labelFont = `9px ${panelFont("ui")}`;
  const node = (n: number) => nodes.get(n)!;
  const kids = (n: number) => node(n).children;
  const moveNo = (n: number) => {
    let d = 0;
    for (let p = node(n).parent; p !== null; p = node(p).parent) d++;
    return d;
  };
  const walk: TreeWalk = { ops: [], hits: new Map(), curX: 0, curY: 0, curLane: 0, maxLane: 0, maxX: w, maxY: h, numbers: 0 };
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
  /** Font(uiFontName, PLAIN, 9) in secondary text, left of a trunk node or above a branch's first
   *  node. */
  const label = (n: number, x: (k: number) => number, y: number) => {
    const k = node(n).number;
    if (!numbers || k === 0) return;
    walk.numbers++;
    ops.push((g) => {
      g.font = labelFont;
      g.fillStyle = p.text2;
      g.textAlign = "left";
      g.textBaseline = "alphabetic";
      g.fillText(`${k}`, x(k), y);
    });
  };
  const dot = (n: number, x: number, y: number) => {
    const dim = !line.has(n);
    const commented = look.showCommentNode && node(n).has_comment;
    const projected = nodeColors.get(n);
    const fill = projected ? cssColor(projected) : p.treeNode;
    ops.push((g) => {
      const disc = (color: string, d: number) => {
        g.fillStyle = color;
        circle(g, x + 5.5, y + 5.5, d / 2);
        g.fill();
      };
      if (commented) disc(commentRing, RING);
      disc(fill, DOT);
      if (dim) disc(p.treeDim, commented ? RING : DOT);
      if (n !== current) return;
      if (p.flat) {
        // The current node: an accent ring around the node, its colour left readable.
        g.strokeStyle = p.accent;
        g.lineWidth = 2;
        circle(g, x + 5.5, y + 5.5, RING / 2 + 1.5);
        g.stroke();
      } else if (fit) {
        // VariationTreeBig: the reversed 错误节点 colour, red on a white node, one pixel larger
        disc(projected ? cssColor(reverseColor(projected)) : "rgb(255,0,0)", CENTER + 2);
      } else disc("#000", CENTER);
    });
    if (!fit) grow(x, y);
  };
  const visit = (n: number, x: number, y: number, lane: number) => {
    hits.set(`${x + 5},${y + 5}`, n);
    if (n === current) [walk.curX, walk.curY, walk.curLane] = [x, y, lane];
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
        label(start, (k) => (k < 10 ? x + 3 : k >= 100 ? x - 3 : x), posy - 5);
      } else {
        // root: square, a red (flat: accent) centre while it is the displayed node
        ops.push((g) => {
          g.fillStyle = line.has(start) ? p.treeNode : p.treeLineOff;
          g.fillRect(x, posy, DOT, DOT);
          g.strokeStyle = p.flat ? p.lineStrong : "#000";
          g.lineWidth = 1;
          g.strokeRect(x - 0.5, posy - 0.5, DOT + 1, DOT + 1);
          if (start === current) {
            g.fillStyle = p.flat ? p.accent : "rgb(255,0,0)";
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
      label(n, (k) => (k < 10 ? x - 7 : k >= 100 ? x - 13 : x - 10), posy - 1);
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
 * whole tree laid out from the root at (20, 20) with Java's lane allocation, shown through the scroll
 * pane that renderVarTree positions on the current node (LizzieFrame.java:13942-13975) unless the
 * user scrolled it (`scroll`). Nodes off the current line are dimmed; with show-comment-node-color
 * commented nodes get the comment-node-color ring, and every node is filled with the 错误节点
 * colour the core projects for it (`nodeColors`, LizzieFrame.getBlunderNodeColor 11465-11536 with
 * Board.updateIsBest; VariationTree.java:136-158), the palette's plain node colour without one;
 * the flat stage marks the current node with an accent ring; 显示分支面板上的手数 (`numbers`)
 * writes each node's core display number beside it (VariationTree.java:159-176, 285-304).
 * 设置最大宽度 (`maxWidth`, setTreeMaxLimit 13894-13908): a layout that wide is cut off there and
 * reported `tooWide` (Board.setBigBranch). Returns node centres (panel coordinates) for click
 * hit-testing and the viewport. */
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
): { hits: Map<string, number>; viewport: TreeViewport; numbers: number; tooWide: boolean } {
  const { w, h } = r;
  const p = stagePalette();
  panelGround(g, w, h, p);
  let walk = walkTree(nodes, root, current, line, numbers, nodeColors, 20, 20, 0, Infinity, false, w, h);
  const tooWide = walk.maxX >= maxWidth;
  const maxX = tooWide ? maxWidth : walk.maxX;
  // VariationTree.draw: with the current lane taken as 0 (getCurLane from the root), the tree
  // moves left only when the cut width is under 51 px.
  const shift = Math.max(0, TREE_XS + 20 + TREE_DOT - maxX);
  if (shift > 0) walk = walkTree(nodes, root, current, line, numbers, nodeColors, 20 - shift, 20, 0, Infinity, false, w, h);
  const { hits, ops, curX, curY, maxY } = walk;
  // JScrollPane with AS_NEEDED bars of DemoScrollBarUI width 9; the horizontal bar is off when the
  // pane is under 100 px high (LizzieFrame.java:13976-13983).
  const hbar = maxX > w && h >= 100;
  const viewH = h - (hbar ? SCROLLBAR : 0);
  const vbar = maxY > viewH;
  const viewW = w - (vbar ? SCROLLBAR : 0);
  const clampTop = (v: number) => Math.max(0, Math.min(i(v), maxY - viewH));
  const clampLeft = (v: number) => Math.max(0, Math.min(i(v), maxX - viewW));
  // renderVarTree: the current node centred in each direction; the root keeps the origin.
  const atRoot = nodes.get(current)!.parent === null;
  const top = clampTop(scroll ? scroll.top : curY <= viewH / 2 || atRoot ? 0 : curY - viewH / 2);
  const left = clampLeft(scroll ? scroll.left : curX <= viewW / 2 || atRoot ? 0 : curX - viewW / 2);
  // The cut image is maxX wide: nothing beyond it shows.
  const shownW = Math.min(viewW, maxX - left);
  g.save();
  g.beginPath();
  g.rect(0, 0, shownW, viewH);
  g.clip();
  g.translate(-left, -top);
  ops.forEach((op) => op(g));
  g.restore();
  const visible = new Map<string, number>();
  for (const [k, id] of hits) {
    const [x, y] = k.split(",").map(Number);
    if (y - top >= 0 && y - top < viewH && x - left >= 0 && x - left < shownW) visible.set(`${x - left},${y - top}`, id);
  }
  const vertical = vbar ? scrollbar(viewH, maxY, top) : null;
  const horizontal = hbar ? scrollbar(viewW, maxX, left) : null;
  // DemoScrollBarUI: arrow buttons, black track, thumb (230,230,250) at 0.7; on the flat stage a
  // sunken track and buttons with drawn arrows and a neutral thumb.
  const button = (x: number, y: number, horiz: boolean, forward: boolean, image: string) => {
    const [bw, bh] = horiz ? [ARROW, SCROLLBAR] : [SCROLLBAR, ARROW];
    g.fillStyle = p.flat ? p.sunken : "rgb(238,238,238)";
    g.fillRect(x, y, bw, bh);
    if (!p.flat) {
      if (images[image]?.naturalWidth) g.drawImage(images[image], x + (horiz ? (forward ? 2 : 1) : 0), y + (horiz ? 0 : forward ? 2 : 1));
      return;
    }
    const cx = x + bw / 2, cy = y + bh / 2, s = 2.5, d = forward ? 1 : -1;
    g.fillStyle = p.text2;
    g.beginPath();
    if (horiz) {
      g.moveTo(cx + d * s, cy);
      g.lineTo(cx - d * s, cy - s);
      g.lineTo(cx - d * s, cy + s);
    } else {
      g.moveTo(cx, cy + d * s);
      g.lineTo(cx - s, cy - d * s);
      g.lineTo(cx + s, cy - d * s);
    }
    g.closePath();
    g.fill();
  };
  const track = p.flat ? p.sunken : "#000";
  const thumb = p.flat ? p.scrollThumb : "rgba(230,230,250,0.7)";
  if (vertical) {
    const sx = w - SCROLLBAR;
    button(sx, 0, false, false, "smallUp.png");
    button(sx, viewH - ARROW, false, true, "smallDown.png");
    g.fillStyle = track;
    g.fillRect(sx, ARROW, SCROLLBAR, vertical.track);
    g.fillStyle = thumb;
    g.fillRect(sx, ARROW + vertical.thumbStart, SCROLLBAR, vertical.thumb);
  }
  if (horizontal) {
    const sy = h - SCROLLBAR;
    button(0, sy, true, false, "smallLeft.png");
    button(viewW - ARROW, sy, true, true, "smallRight.png");
    g.fillStyle = track;
    g.fillRect(ARROW, sy, horizontal.track, SCROLLBAR);
    g.fillStyle = thumb;
    g.fillRect(ARROW + horizontal.thumbStart, sy, horizontal.thumb, SCROLLBAR);
  }
  panelEdge(g, w, h, p);
  return { hits: visible, viewport: { top, left, maxX, maxY, viewW, viewH, vertical, horizontal }, numbers: walk.numbers, tooWide };
}

/** LizzieFrame.createVarTreeImage's simple view (shouldShowSimpleVariation) drawn by
 *  VariationTreeBig.draw (VariationTreeBig.java:320-367): no scroll bars; the current node sits
 *  in the middle row, the drawing starts at the ancestor near the top of the panel and stops at its
 *  bottom, and it moves left so that the current lane (and as much of the lanes to its right as
 *  fits) shows. Nodes are drawn as in the scroll pane; returns their centres for clicks. */
export function paintTreeFit(
  g: CanvasRenderingContext2D,
  r: Rect,
  nodes: Map<number, TreeNodeView>,
  current: number,
  line: Set<number>,
  numbers: boolean,
  nodeColors: Map<number, number[]>,
): { hits: Map<string, number>; viewport: TreeViewport; numbers: number } {
  const { w, h } = r;
  const p = stagePalette();
  panelGround(g, w, h, p);
  const viewport: TreeViewport = { top: 0, left: 0, maxX: w, maxY: h, viewW: w, viewH: h, vertical: null, horizontal: null };
  if (w <= 0 || h <= 0) return { hits: new Map(), viewport, numbers: 0 };
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
  const { curLane: lane, maxLane } = walkTree(nodes, start, current, line, numbers, nodeColors, 0, posy, 0, h, true, w, h);
  const x0 = 20;
  let startx = x0;
  const over = (lane + 1) * TREE_XS + x0 + TREE_DOT - w;
  if (over > 0) startx -= over;
  if (maxLane * TREE_XS + x0 + TREE_DOT > w) {
    const half = i((w - x0 - TREE_DOT) / 2);
    if ((maxLane - lane) * TREE_XS < half) startx -= (maxLane - lane) * TREE_XS;
    else if (lane * TREE_XS + x0 + TREE_DOT > w) startx -= half;
  }
  const walk = walkTree(nodes, start, current, line, numbers, nodeColors, startx, posy, 0, h, true, w, h);
  g.save();
  g.beginPath();
  g.rect(0, 0, w, h);
  g.clip();
  walk.ops.forEach((op) => op(g));
  g.restore();
  panelEdge(g, w, h, p);
  const hits = new Map<string, number>();
  for (const [k, id] of walk.hits) {
    const [x, y] = k.split(",").map(Number);
    if (x >= 0 && x < w && y >= 0 && y < h) hits.set(k, id);
  }
  return { hits, viewport, numbers: walk.numbers };
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
