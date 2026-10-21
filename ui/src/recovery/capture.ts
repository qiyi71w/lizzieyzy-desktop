// Pictures of the workbench as shown (Java LizzieFrame.saveImage, savePicToClipboard and
// saveMainBoardToImageOri): the wallpaper with its frosted containers, then the panels Java paints
// into its frame image (board, captures, statistics, win-rate graph, status, sub board), cropped to a
// region in panel pixels and written opaque (TYPE_INT_RGB). The variation tree, the comment and the
// candidate table are Swing components in Java, outside that image.
import { run, state } from "../app";
import type { CommandResult } from "../contract/generated";
import { confirm, message } from "../dialogs";
import { t } from "../i18n";
import { boardGeom, type Panels, type Rect } from "../workbench/layout";

export interface CaptureSurface {
  panels: Panels;
  /** #main size in CSS pixels; the background image covers it. */
  width: number;
  height: number;
  /** Object URL of the background image, null until it is painted. */
  background: string | null;
}

let surface: () => CaptureSurface | null = () => null;
let repaint: () => void = () => {};

/** The workbench supplies its layout and a synchronous repaint, so a picture never shows a frame
 *  older than the view. */
export function bindCaptureSurface(get: () => CaptureSurface | null, render: () => void): void {
  surface = get;
  repaint = render;
}

const LAYERS = ["board", "captured", "stat", "graph", "status", "sub"];

/** Base64 PNG of `pick`'s region, or null when the workbench is not laid out yet. */
async function capture(pick: (s: CaptureSurface) => Rect | null): Promise<string | null> {
  try {
    repaint();
    const s = surface();
    const r = s && pick(s);
    if (!s || !r || r.w <= 0 || r.h <= 0) return null;
    const canvas = document.createElement("canvas");
    canvas.width = r.w;
    canvas.height = r.h;
    const g = canvas.getContext("2d")!;
    g.fillStyle = "#000";
    g.fillRect(0, 0, r.w, r.h);
    if (s.background) {
      const image = new Image();
      image.src = s.background;
      await image.decode();
      g.drawImage(image, -r.x, -r.y, s.width, s.height);
    }
    for (const id of LAYERS) {
      const layer = document.getElementById(id) as HTMLCanvasElement | null;
      if (!layer) continue;
      const [x, y, w, h] = [layer.style.left, layer.style.top, layer.style.width, layer.style.height].map(parseFloat);
      if ([x, y, w, h].every(Number.isFinite) && w > 0 && h > 0) g.drawImage(layer, x - r.x, y - r.y, w, h);
    }
    const url = canvas.toDataURL("image/png");
    return url.slice(url.indexOf(",") + 1);
  } catch (e) {
    console.error("picture capture failed", e);
    return null;
  }
}

/** boardX, boardY, maxSize: the board square with coordinates and the player names. */
export function captureMainBoard(): Promise<string | null> {
  return capture((s) => s.panels.board);
}

/** subBoardRenderer x, y, boardWidth, boardHeight. */
function subBoardRect(s: CaptureSurface): Rect | null {
  const v = state.view;
  if (!v || s.panels.sub.w <= 0 || s.panels.sub.h <= 0) return null;
  const geo = boardGeom(s.panels.sub, v.document.size.width, v.document.size.height, 0.035);
  return { x: geo.x, y: geo.y, w: geo.size, h: geo.size };
}

/** Java `config.showSubBoard`: this layout always shows it once the window gives it room. */
function subBoardShown(): boolean {
  const s = surface();
  return !!s && subBoardRect(s) !== null;
}

/** "保存胜率图": statx, staty, grw * 1.03, grh + stath. */
function winrateRect(s: CaptureSurface): Rect {
  const { stat, graph } = s.panels;
  return { x: stat.x, y: stat.y, w: Math.trunc(graph.w * 1.03), h: graph.h + stat.h };
}

/** The prompts and messages of a picture write (LizzieFrame.saveImage). */
async function followPicture(r: CommandResult | null, png: string): Promise<void> {
  if (r?.type === "NeedsImageOverwriteConfirmation") {
    if (await confirm(t("LizzieFrame.warning"), t("LizzieFrame.fileExists"))) await followPicture(await run({ type: "ExportImageTo", path: r.path, png }), png);
  } else if (r?.type === "ExportFailed") {
    const fileName = r.path.split(/[\\/]/).pop() ?? r.path;
    await message(t("LizzieFrame.lizzieError"), `${t("LizzieFrame.saveImageErrorHint1")} "${fileName}"\n(${t("LizzieFrame.saveImageErrorHint2")})`, "error");
  }
}

async function savePicture(png: string | null): Promise<void> {
  if (png) await followPicture(await run({ type: "ExportImage", png }), png);
}

async function copyPicture(png: string | null): Promise<void> {
  if (png) await run({ type: "CopyImage", png });
}

/** 保存主棋盘截图 (Alt+S). */
export async function saveMainBoardPicture(): Promise<void> {
  await savePicture(await captureMainBoard());
}

/** 保存小棋盘截图 (Shift+S); without a sub board Utils.showMsg(saveSubBoardHint). */
export async function saveSubBoardPicture(): Promise<void> {
  if (!subBoardShown()) {
    await message(t("Message.title"), t("LizzieFrame.saveSubBoardHint"));
    return;
  }
  await savePicture(await capture(subBoardRect));
}

/** 保存胜率图 (Shift+Alt+S). */
export async function saveWinratePicture(): Promise<void> {
  await savePicture(await capture(winrateRect));
}

/** 复制主棋盘到剪贴板 (Shift+C). */
export async function copyMainBoardPicture(): Promise<void> {
  await copyPicture(await captureMainBoard());
}

/** 复制小棋盘到剪贴板 (Alt+C); without a sub board nothing happens (LizzieFrame.copySubBoard). */
export async function copySubBoardPicture(): Promise<void> {
  if (subBoardShown()) await copyPicture(await capture(subBoardRect));
}
