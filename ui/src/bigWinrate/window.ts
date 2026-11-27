// 黑白胜率大字 window (Java BigWinrateWindow / BigWinratePanel): Black's and White's win rates side by
// side in large figures, on a black and a white card (简洁) or as seven-segment digits (数码管). It
// draws what the main window sends and nothing else: no timer of its own, and a picture only when
// the figures, the look, the window's size or the colours change. Its right-click menu (also the
// menu key or Shift+F10) picks the look and 窗口置顶; the main window runs the choice.
import { emitTo, listen } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { CheckMenuItem, Menu, PredefinedMenuItem } from "@tauri-apps/api/menu";
import { LogicalPosition } from "@tauri-apps/api/dpi";
import { activateFirst, bindMenuRows, menuKey } from "../chrome/menuKeys";
import type { BigWinrateStyle } from "../contract/generated";
import { icon } from "../design/icons";
import { applyTypeSizes, onThemeChange } from "../design/theme";
import { setLanguage, t } from "../i18n";
import {
  BIG_WINRATE_DATA_EVENT,
  BIG_WINRATE_INPUT_EVENT,
  type BigWinrateCardReport,
  type BigWinrateInput,
  type BigWinrateMenuReport,
  type BigWinratePayload,
  type Box,
} from "./model";
import "../design/window";
import "../design/stage.css";
import "./bigWinrate.css";

/** The lit segments of each digit (BigWinratePanel.SEGMENTS): a to f go round from the top
 *  clockwise, so b and c are the right side, d the bottom, e and f the left; g is the middle bar. */
const SEGMENTS = ["abcdef", "bc", "abged", "abgcd", "fgbc", "afgcd", "afgecd", "abc", "abcdefg", "abcdfg"];
/** Java's alphas: the side names, a dimmed figure (简洁 / 数码管) and an unlit segment. */
const LABEL_ALPHA = 170 / 255;
const DIM_PLAIN = 150 / 255;
const DIM_LED = 170 / 255;
const UNLIT = 22 / 255;

const send = (input: BigWinrateInput) => void emitTo("main", BIG_WINRATE_INPUT_EVENT, input);

interface Colors {
  ground: string;
  line: string;
  card: [string, string];
  edge: string;
  ink: [string, string];
  label: [string, string];
  radius: number;
  sizeS: number;
}

/** The colours of a look, from the design tokens: 简洁 the stones' (棋子) colours on the window's
 *  panel ground, 数码管 the dark set's display ground and text (the GTP console's dark surface). */
function colors(surface: HTMLElement, led: boolean): Colors {
  const css = getComputedStyle(surface);
  const v = (name: string) => css.getPropertyValue(name).trim();
  const sizeS = parseFloat(v("--ds-size-s")) || 12;
  if (led) {
    return {
      ground: v("--ds-panel"),
      line: v("--ds-line-strong"),
      card: [v("--ds-field"), v("--ds-field")],
      edge: v("--ds-line-strong"),
      ink: [v("--ds-text"), v("--ds-text")],
      label: [v("--ds-text-2"), v("--ds-text-2")],
      radius: parseFloat(v("--ds-radius-l")) || 10,
      sizeS,
    };
  }
  return {
    ground: v("--ds-panel"),
    line: v("--ds-line-strong"),
    card: [v("--ws-stone-black"), v("--ws-stone-white")],
    edge: v("--ws-stone-white-edge"),
    ink: [v("--ws-on-black"), v("--ws-on-white")],
    label: [v("--ws-on-black"), v("--ws-on-white")],
    radius: 0,
    sizeS,
  };
}

const round = (b: Box): Box => ({ x: Math.round(b.x), y: Math.round(b.y), w: Math.round(b.w), h: Math.round(b.h) });

/** One segment of a seven-segment digit in the cell at x, y, as a six-sided bar
 *  (BigWinratePanel.segment), its points rounded as java.awt.Polygon keeps them. */
function segment(s: string, x: number, y: number, width: number, height: number, t: number): [number, number][] {
  const gap = t * 0.22;
  const left = x + t / 2;
  const right = x + width - t / 2;
  const top = y + t / 2;
  const middle = y + height / 2;
  const bottom = y + height - t / 2;
  const h = t / 2;
  const horizontal = (from: number, to: number, at: number): [number, number][] =>
    [[from, at], [from + h, at - h], [to - h, at - h], [to, at], [to - h, at + h], [from + h, at + h]];
  const vertical = (at: number, from: number, to: number): [number, number][] =>
    [[at, from], [at + h, from + h], [at + h, to - h], [at, to], [at - h, to - h], [at - h, from + h]];
  const points =
    s === "a" ? horizontal(left + gap, right - gap, top)
    : s === "g" ? horizontal(left + gap, right - gap, middle)
    : s === "d" ? horizontal(left + gap, right - gap, bottom)
    : s === "f" ? vertical(left, top + gap, middle - gap)
    : s === "b" ? vertical(right, top + gap, middle - gap)
    : s === "e" ? vertical(left, middle + gap, bottom - gap)
    : vertical(right, middle + gap, bottom - gap);
  return points.map(([px, py]) => [Math.round(px), Math.round(py)]);
}

/** A digit (or "-", or a blank) in its cell, slanted like Java's: lit segments filled, the others
 *  faint. No halo: the design system draws no glow. */
function paintDigit(g: CanvasRenderingContext2D, c: string, x: number, y: number, width: number, height: number, lit: string, litAlpha: number): void {
  const on = c === "-" ? "g" : /[0-9]/.test(c) ? SEGMENTS[Number(c)] : "";
  const t = width * 0.19;
  // AffineTransform shear(-0.08, 0) after a translation of 0.08 * (y + height).
  const slant = (px: number, py: number): [number, number] => [px - 0.08 * py + 0.08 * (y + height), py];
  g.fillStyle = lit;
  for (const s of "abcdefg") {
    g.globalAlpha = on.includes(s) ? litAlpha : UNLIT;
    g.beginPath();
    for (const [i, [px, py]] of segment(s, x, y, width, height, t).entries()) {
      const [sx, sy] = slant(px, py);
      if (i === 0) g.moveTo(sx, sy);
      else g.lineTo(sx, sy);
    }
    g.closePath();
    g.fill();
  }
  g.globalAlpha = 1;
}

export function bootBigWinrate(): void {
  const canvas = document.createElement("canvas");
  canvas.id = "bw-canvas";
  canvas.setAttribute("role", "img");
  canvas.tabIndex = 0;
  const menu = document.createElement("div");
  menu.className = "bw-menu";
  menu.hidden = true;
  document.body.append(canvas, menu);

  let data: BigWinratePayload | null = null;
  let paints = 0;
  let drawn = "";
  let cards: BigWinrateCardReport[] = [];
  let menuReport: BigWinrateMenuReport | null = null;
  let lastReport = "";
  let ariaLabel = "";
  /** Only builds with the acceptance channel send `acceptance-probe`: until one arrives the window
   *  neither measures its menu nor reports what it drew, so a normal build sends nothing per paint. */
  let probed = false;

  const report = () => {
    if (!data || !probed) return;
    const r = { width: window.innerWidth, height: window.innerHeight, style: data.style, paints, cards, menu: menuReport };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    send({ type: "report", report: r });
  };

  /** One side's card (BigWinratePanel.paintPlainCard / paintLedCard): the side's name at its top
   *  left, the figure in the room below it. */
  const paintCard = (g: CanvasRenderingContext2D, c: Colors, led: boolean, x: number, y: number, width: number, height: number, black: boolean): BigWinrateCardReport => {
    const p = data!;
    const side = black ? 0 : 1;
    g.fillStyle = c.card[side];
    g.beginPath();
    g.roundRect(x, y, width, height, Math.min(c.radius, width / 2, height / 2));
    g.fill();
    if (led || !black) {
      g.strokeStyle = c.edge;
      g.lineWidth = 1;
      g.beginPath();
      g.roundRect(x + 0.5, y + 0.5, width - 1, height - 1, Math.max(0, Math.min(c.radius, width / 2, height / 2) - 0.5));
      g.stroke();
    }
    // The name: Java's size (a tenth of the card, the Label step at least), smaller where the card
    // is too narrow for it, so it never runs past the card.
    const label = t(black ? "BigWinrateWindow.black" : "BigWinrateWindow.white");
    const inset = Math.max(6, Math.floor(height / 16));
    let size = Math.max(c.sizeS, height / 10);
    g.font = `400 ${size}px ${p.labelFont}`;
    const natural = g.measureText(label).width;
    if (natural > width - 2 * inset) {
      size = Math.max(1, (size * (width - 2 * inset)) / Math.max(1, natural));
      g.font = `400 ${size}px ${p.labelFont}`;
    }
    const lm = g.measureText(label);
    const ascent = lm.fontBoundingBoxAscent ?? lm.actualBoundingBoxAscent;
    const descent = lm.fontBoundingBoxDescent ?? lm.actualBoundingBoxDescent;
    g.fillStyle = c.label[side];
    g.globalAlpha = led ? 1 : LABEL_ALPHA;
    g.textBaseline = "alphabetic";
    g.fillText(label, x + inset, y + inset + ascent);
    g.globalAlpha = 1;
    const labelBox = { x: x + inset, y: y + inset, w: lm.width, h: ascent + descent };
    // The figure goes where Java puts it, unless that would run into the name: then it takes the
    // room between the name and the card's lower inset.
    const bandTop = labelBox.y + labelBox.h + Math.max(2, inset / 2);
    const bandBottom = y + height - inset;
    const band = Math.max(1, bandBottom - bandTop);
    const figure = black ? p.figures.black : p.figures.white;
    let box: Box;
    if (!led) {
      g.font = `600 100px ${p.figureFont}`;
      const width100 = Math.max(1, g.measureText(figure).width);
      const digit100 = Math.max(1, g.measureText("0").actualBoundingBoxAscent);
      let fontSize = Math.max(8, Math.min((100 * width * 0.8) / width100, (100 * height * 0.62) / digit100));
      let digit = (digit100 * fontSize) / 100;
      let baseline = y + (height + digit) / 2 + height / 30;
      if (baseline - digit < bandTop || baseline > bandBottom) {
        fontSize = Math.min(fontSize, (100 * band) / digit100);
        digit = (digit100 * fontSize) / 100;
        baseline = bandTop + (band + digit) / 2;
      }
      g.font = `600 ${fontSize}px ${p.figureFont}`;
      const w = g.measureText(figure).width;
      const left = x + (width - w) / 2;
      g.fillStyle = c.ink[side];
      g.globalAlpha = p.figures.dim ? DIM_PLAIN : 1;
      g.fillText(figure, left, baseline);
      g.globalAlpha = 1;
      const fm = g.measureText(figure);
      box = { x: left - fm.actualBoundingBoxLeft, y: baseline - fm.actualBoundingBoxAscent, w: fm.actualBoundingBoxLeft + fm.actualBoundingBoxRight, h: fm.actualBoundingBoxAscent + fm.actualBoundingBoxDescent };
    } else {
      // At least two cells; a short figure is right-aligned, as on a clock.
      const cells = Math.max(2, figure.length);
      let cellHeight = Math.min(height * 0.62, (width * 0.78) / (cells * 0.68));
      let top = y + (height - cellHeight) / 2 + height * 0.04;
      if (top < bandTop || top + cellHeight > bandBottom) {
        cellHeight = Math.min(cellHeight, band);
        top = bandTop + (band - cellHeight) / 2;
      }
      const cellWidth = cellHeight * 0.55;
      const spacing = cellWidth * 0.18;
      const total = cells * cellWidth + (cells - 1) * spacing;
      const left = x + (width - total) / 2 + cellHeight * 0.04;
      const offset = cells - figure.length;
      for (let cell = 0; cell < cells; cell++) {
        const ch = cell < offset ? " " : figure[cell - offset];
        paintDigit(g, ch, left + cell * (cellWidth + spacing), top, cellWidth, cellHeight, c.ink[side], p.figures.dim ? DIM_LED : 1);
      }
      // The slant leans the top right by 0.08 of the cell height.
      box = { x: left - cellWidth * 0.19 / 2, y: top, w: total + cellWidth * 0.19 + 0.08 * cellHeight, h: cellHeight };
    }
    return {
      side: black ? "black" : "white",
      card: round({ x, y, w: width, h: height }),
      label: { text: label, box: round(labelBox) },
      figure: { text: figure, dim: p.figures.dim, box: round(box) },
    };
  };

  /** BigWinratePanel.paintComponent: two cards of equal width side by side, Black's on the left. */
  const paint = (force = false) => {
    if (!data) return;
    const w = window.innerWidth;
    const h = window.innerHeight;
    const dpr = window.devicePixelRatio || 1;
    const led = data.style === "Led";
    const key = JSON.stringify([data.figures, data.style, data.figureFont, data.labelFont, data.language, w, h, dpr]);
    if (!force && key === drawn) return;
    drawn = key;
    // 数码管 is a dark display surface whatever the colour mode, as the GTP console is.
    if (led) canvas.dataset.theme = "dark";
    else delete canvas.dataset.theme;
    const c = colors(canvas, led);
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    canvas.style.width = `${w}px`;
    canvas.style.height = `${h}px`;
    const g = canvas.getContext("2d")!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const pad = Math.max(8, Math.floor(Math.min(w, h) / 18));
    const gap = Math.max(6, Math.floor(w / 30));
    const cardWidth = Math.max(1, Math.floor((w - 2 * pad - gap) / 2));
    const cardHeight = Math.max(1, h - 2 * pad);
    const rightX = pad + cardWidth + gap;
    g.fillStyle = c.ground;
    g.fillRect(0, 0, w, h);
    if (!led) {
      g.fillStyle = c.line;
      g.fillRect(pad + cardWidth + Math.floor(gap / 2), pad, 1, cardHeight);
    }
    cards = [paintCard(g, c, led, pad, pad, cardWidth, cardHeight, true), paintCard(g, c, led, rightX, pad, cardWidth, cardHeight, false)];
    paints++;
    const name = cards.map((card) => `${card.label.text} ${card.figure.text}${card.figure.text === "--" ? "" : "%"}`).join(" · ");
    if (name !== ariaLabel) {
      ariaLabel = name;
      canvas.setAttribute("aria-label", name);
    }
    report();
  };

  // ---- the right-click menu: 简洁, 数码管, then 窗口置顶 (BigWinrateWindow.menu)
  let focusBefore: Element | null = null;
  const item = (key: string, role: "menuitemradio" | "menuitemcheckbox", run: () => void) => {
    const row = document.createElement("div");
    row.className = "mi";
    row.dataset.key = key;
    row.setAttribute("role", role);
    const mark = document.createElement("span");
    mark.className = "mark";
    const label = document.createElement("span");
    label.className = "label";
    row.append(mark, label);
    row.addEventListener("click", () => {
      closeMenu();
      run();
    });
    return row;
  };
  const style = (s: BigWinrateStyle) => () => {
    if (data && data.style !== s) send({ type: "style", style: s });
  };
  const choices = {
    plain: style("Plain"),
    led: style("Led"),
    onTop: () => data && send({ type: "onTop", onTop: !data.onTop }),
  };
  const rows = {
    plain: item("bigWinrate.plain", "menuitemradio", choices.plain),
    led: item("bigWinrate.led", "menuitemradio", choices.led),
    onTop: item("bigWinrate.onTop", "menuitemcheckbox", choices.onTop),
  };
  const sep = document.createElement("div");
  sep.className = "sep";
  menu.append(rows.plain, rows.led, sep, rows.onTop);
  bindMenuRows(menu);

  /** The rows' texts and checks; text only where it changed. */
  const fillMenu = () => {
    if (!data) return;
    const states: [HTMLElement, string, boolean][] = [
      [rows.plain, t("BigWinrateWindow.plain"), data.style === "Plain"],
      [rows.led, t("BigWinrateWindow.led"), data.style === "Led"],
      [rows.onTop, t("BigWinrateWindow.onTop"), data.onTop],
    ];
    for (const [row, text, checked] of states) {
      const label = row.querySelector<HTMLElement>(".label")!;
      if (label.textContent !== text) label.textContent = text;
      if (row.getAttribute("aria-checked") !== String(checked)) {
        row.setAttribute("aria-checked", String(checked));
        row.querySelector(".mark")!.replaceChildren(...(checked ? [icon("check")] : []));
      }
    }
  };

  /** The same three choices as the system's menu, for a window too small to hold the page's own:
   *  it may reach past the window's edges, as Java's JPopupMenu does. Built once, its texts and
   *  checks set each time it opens. While it is open the system holds the window's events, so
   *  nothing else in the program answers until it closes. */
  let systemMenu: Promise<{ menu: Menu; plain: CheckMenuItem; led: CheckMenuItem; onTop: CheckMenuItem }> | null = null;
  const popupSystemMenu = async (x: number, y: number) => {
    systemMenu ??= (async () => {
      const [plain, led, onTop] = await Promise.all([
        CheckMenuItem.new({ text: "", action: choices.plain }),
        CheckMenuItem.new({ text: "", action: choices.led }),
        CheckMenuItem.new({ text: "", action: choices.onTop }),
      ]);
      const menu = await Menu.new({ items: [plain, led, await PredefinedMenuItem.new({ item: "Separator" }), onTop] });
      return { menu, plain, led, onTop };
    })();
    const m = await systemMenu;
    if (!data) return;
    await Promise.all([
      m.plain.setText(t("BigWinrateWindow.plain")),
      m.plain.setChecked(data.style === "Plain"),
      m.led.setText(t("BigWinrateWindow.led")),
      m.led.setChecked(data.style === "Led"),
      m.onTop.setText(t("BigWinrateWindow.onTop")),
      m.onTop.setChecked(data.onTop),
    ]);
    await m.menu.popup(new LogicalPosition(x, y));
  };

  const measureMenu = (): BigWinrateMenuReport | null => {
    if (menu.hidden || !probed) return null;
    const box = (r: DOMRect): Box => round({ x: r.left, y: r.top, w: r.width, h: r.height });
    return {
      box: box(menu.getBoundingClientRect()),
      items: [rows.plain, rows.led, rows.onTop].map((row) => {
        const label = row.querySelector<HTMLElement>(".label")!;
        return {
          key: row.dataset.key!,
          text: label.textContent ?? "",
          checked: row.getAttribute("aria-checked") === "true",
          box: box(row.getBoundingClientRect()),
          textWidth: label.scrollWidth,
          textBox: box(label.getBoundingClientRect()),
        };
      }),
    };
  };

  const closeMenu = () => {
    if (menu.hidden) return;
    const focus = document.activeElement;
    const lost = !focus || focus === document.body || menu.contains(focus);
    menu.hidden = true;
    menu.querySelectorAll(".mi.active").forEach((row) => row.classList.remove("active"));
    menuReport = null;
    const before = focusBefore;
    focusBefore = null;
    if (lost && before instanceof HTMLElement && before.isConnected) before.focus({ preventScroll: true });
    report();
  };
  const openMenu = (x: number, y: number, keyboard: boolean) => {
    if (!data) return;
    fillMenu();
    const wasHidden = menu.hidden;
    menu.hidden = false;
    if (menu.offsetWidth > window.innerWidth || menu.offsetHeight > window.innerHeight) {
      menu.hidden = wasHidden;
      closeMenu();
      void popupSystemMenu(x, y).catch((e: unknown) => console.error("黑白胜率大字 menu failed", e));
      return;
    }
    if (wasHidden) focusBefore = document.activeElement;
    menu.querySelectorAll(".mi.active").forEach((row) => row.classList.remove("active"));
    // Inside the window, beside the pointer.
    menu.style.left = `${Math.max(0, Math.min(x, window.innerWidth - menu.offsetWidth))}px`;
    menu.style.top = `${Math.max(0, Math.min(y, window.innerHeight - menu.offsetHeight))}px`;
    if (keyboard) activateFirst(menu);
    menuReport = measureMenu();
    report();
  };

  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    if (!menu.contains(e.target as Node)) openMenu(e.clientX, e.clientY, false);
  });
  document.addEventListener("mousedown", (e) => {
    if (!menu.hidden && !menu.contains(e.target as Node)) closeMenu();
  }, true);
  document.addEventListener("keydown", (e) => {
    if (!menu.hidden) {
      if (menuKey(e, menu, closeMenu)) {
        e.preventDefault();
        e.stopPropagation();
      }
      return;
    }
    if (!e.ctrlKey && !e.altKey && !e.metaKey && (e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey))) {
      e.preventDefault();
      openMenu(8, 8, true);
    }
  }, true);
  window.addEventListener("blur", closeMenu);
  window.addEventListener("resize", () => {
    closeMenu();
    paint();
  });
  onThemeChange(() => paint(true));
  void listen("acceptance-probe", () => {
    if (probed) return;
    probed = true;
    menuReport = measureMenu();
    report();
  });

  const self = getCurrentWebviewWindow();
  void self.listen<BigWinratePayload>(BIG_WINRATE_DATA_EVENT, (e) => {
    data = e.payload;
    setLanguage(data.language);
    applyTypeSizes(data.frameFontSize);
    if (!menu.hidden) {
      fillMenu();
      menuReport = measureMenu();
      report();
    }
    // The figure font may still be loading the first time: the picture waits for it.
    const font = `600 100px ${data.figureFont}`;
    if (document.fonts.check(font)) paint();
    else void document.fonts.load(font).finally(() => paint(true));
  }).then(() => send({ type: "ready" }));
}
