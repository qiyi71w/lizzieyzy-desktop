// 综合设置 > 主题外观 (ConfigDialog2.rebuildThemeTabLikeDesign, ConfigDialog2.java:3728-3965, with
// the controls of loadThemeTab 4369-5232): the theme's values are a draft read from the chosen
// theme (readThemeValues / readDefaultTheme 6120-6372) and sent with 保存设置 (writeThemeValues /
// writeDefaultTheme 6191-6471, saveConfig 6863-6867); 取消 discards them. 添加 / 删除 manage the
// theme list at once like Java, without touching the committed appearance. 棋盘预览 paints the
// draft on its own canvas (pnlBoardPreview 4980-5197). 选点评估关注外框 (3923-3943) edits the
// 选点重点研究 settings, not the theme, and is saved on its own.
import { run, state } from "../app";
import { appearanceCatalog, chooseAppearanceImage } from "../contract/client";
import type { AppearanceCatalog, ThemeSettings } from "../contract/generated";
import { ask, confirm, message } from "../dialogs";
import { SETTINGS_PAGES, type SettingsDialog } from "../engine/settings";
import { registerSettingTargets } from "../engine/settingsTargets";
import { product, t } from "../i18n";
import { cssColor, DEFAULT_BLUNDER_COLORS, DEFAULT_BLUNDER_THRESHOLDS, DEFAULT_THEME, LIZZIE_DEFAULT_FONT, opaque, SYSTEM_DEFAULT_FONT } from "./model";
import { loadThemeImages, type LoadedImages } from "./state";

/** ConfigSettingTargets THEME rows this page has (ConfigSettingTargets.java:61-86), in row order. */
const TARGETS = [
  "config.theme.current",
  "config.theme.winrate-width",
  "config.theme.blunder-width",
  "config.theme.shadow",
  "config.theme.info-font",
  "config.theme.ui-font",
  "config.theme.winrate-font",
  "config.theme.background-image",
  "config.theme.board-image",
  "config.theme.black-stone-image",
  "config.theme.white-stone-image",
  "config.theme.background-blur",
  "config.theme.winrate-color",
  "config.theme.missing-color",
  "config.theme.blunder-color",
  "config.theme.score-color",
  "config.theme.comment-background",
  "config.theme.comment-text",
  "config.theme.best-move-color",
  "config.theme.comment-font-size",
  "config.theme.stone-indicator",
  "config.theme.comment-node-color",
  "config.tracking.outline",
  "config.tracking.outline-opacity",
  "config.theme.blunder-rules",
  "config.theme.score-blunders",
] as const;
type Target = (typeof TARGETS)[number];

/** readDefaultTheme's fixed image paths: the default theme's images are not editable (6313-6324). */
const DEFAULT_PATHS = { background: "/assets/background.jpg", board: "/assets/board.png", black: "/assets/black0.png", white: "/assets/white0.png" };

const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
};

/** createDesignSettingsCard: a titled card. Texts arrive translated (t with literal keys). */
function card(host: HTMLElement, title: string, subtitle: string): HTMLElement {
  const box = el("section", "appearance-card");
  box.append(el("h3", undefined, title), el("p", "appearance-card-sub", subtitle));
  host.append(box);
  return box;
}

/** createDesignRow: title and sub line on the left, the controls on the right; rows with a
 *  ConfigSettingTargets id carry it for 功能搜索's row location. */
function row(box: HTMLElement, target: Target | null, title: string, subtitle: string, ...controls: HTMLElement[]): HTMLElement {
  const line = el("div", "appearance-row");
  if (target) line.dataset.settingTarget = target;
  const text = el("div", "appearance-row-text");
  text.append(el("div", "appearance-row-title", title), el("div", "appearance-row-sub", subtitle));
  const slot = el("div", "appearance-row-controls");
  slot.append(...controls);
  line.append(text, slot);
  box.append(line);
  return line;
}

function check(key: string, label: string): HTMLInputElement {
  const input = el("input");
  input.type = "checkbox";
  input.dataset.key = key;
  input.setAttribute("aria-label", label);
  return input;
}

function labelled(input: HTMLInputElement, label: string): HTMLLabelElement {
  const wrap = el("label", "appearance-check");
  wrap.append(input, label);
  return wrap;
}

/** JSpinner(SpinnerNumberModel(value, min, max, step)): a typed value outside the model is
 *  rejected and the last valid one returns. */
function spinner(key: string, min: number, max: number, step: number, label: string): HTMLInputElement {
  const input = el("input");
  input.type = "number";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.dataset.key = key;
  input.title = label;
  let last = input.value;
  input.addEventListener("focus", () => (last = input.value));
  input.addEventListener("change", () => {
    const v = Number(input.value);
    if (input.value === "" || !Number.isFinite(v) || v < min || v > max) input.value = last;
    else last = input.value;
  });
  return input;
}

/** JFormattedTextField with DigitOnlyFilter; Utils.txtFieldIntValue reads a blank as 0. */
function digits(key: string): HTMLInputElement {
  const input = el("input", "appearance-number");
  input.type = "text";
  input.inputMode = "numeric";
  input.dataset.key = key;
  input.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const kept = inserted.replace(/\D+/g, "");
    if (kept === inserted) return;
    e.preventDefault();
    if (kept) input.setRangeText(kept, input.selectionStart ?? input.value.length, input.selectionEnd ?? input.value.length, "end");
  });
  return input;
}
const intValue = (input: HTMLInputElement) => {
  const s = input.value.trim();
  return s === "" || s.length >= 10 ? 0 : Number.parseInt(s, 10);
};

/** ColorLabel + 选择 (createColorControl, 4033-4053): the swatch opens the chooser; with alpha
 *  (ColorLabel useAplha) the alpha is edited beside it and written when not 255 (Theme.color2Array). */
interface ColorControl {
  root: HTMLElement;
  get(): number[];
  set(c: readonly number[]): void;
}
function colorControl(key: string, alpha: boolean, chooseButton: boolean): ColorControl {
  const root = el("span", "appearance-color");
  const swatch = el("input");
  swatch.type = "color";
  swatch.dataset.key = key;
  root.append(swatch);
  const opacity = el("input", "appearance-number");
  if (alpha) {
    opacity.type = "number";
    opacity.min = "0";
    opacity.max = "255";
    opacity.step = "1";
    opacity.dataset.key = `${key}.alpha`;
    opacity.addEventListener("change", () => {
      const v = Math.round(Number(opacity.value));
      opacity.value = String(Number.isFinite(v) ? Math.max(0, Math.min(255, v)) : 255);
    });
    root.append(opacity);
  }
  if (chooseButton) {
    const choose = el("button", "appearance-choose", t("ConfigDialog2.modern.chooseColor"));
    choose.type = "button";
    choose.onclick = () => swatch.click();
    root.append(choose);
  }
  const hex = (v: number) => v.toString(16).padStart(2, "0");
  return {
    root,
    get() {
      const v = swatch.value;
      const rgb = [1, 3, 5].map((k) => Number.parseInt(v.slice(k, k + 2), 16));
      const a = alpha ? Number(opacity.value) : 255;
      return a !== 255 ? [...rgb, a] : rgb;
    },
    set(c) {
      swatch.value = `#${hex(c[0])}${hex(c[1])}${hex(c[2])}`;
      opacity.value = String(c[3] ?? 255);
    },
  };
}

/** A font combo box: Lizzie默认 (stored "Lizzie Default"), 系统默认 (stored "Dialog.plain"), then
 *  the system families (fontList, 4513-4521; getFontItemName 6374-6380). Each shows in its font. */
interface FontCombo {
  select: HTMLSelectElement;
  setFonts(fonts: string[]): void;
  get(): string;
  set(name: string): void;
}
function fontCombo(key: string): FontCombo {
  const select = el("select");
  select.dataset.key = key;
  let fonts: string[] = [];
  let loaded = false;
  // setFontValue (6114-6118): an unknown name selects index 0. "Dialog.plain" selects 系统默认,
  // which Java's combo could not find again (its items are display names).
  const pick = (name: string) => {
    select.value = [...select.options].some((o) => o.value === name) ? name : LIZZIE_DEFAULT_FONT;
    select.style.fontFamily = select.selectedIndex > 1 ? `"${select.value}"` : "";
  };
  const fill = (selected: string) => {
    select.replaceChildren();
    const add = (value: string, text: string) => {
      const option = el("option", undefined, text);
      option.value = value;
      if (value !== LIZZIE_DEFAULT_FONT && value !== SYSTEM_DEFAULT_FONT) option.style.fontFamily = `"${value}"`;
      select.append(option);
    };
    add(LIZZIE_DEFAULT_FONT, t("FontList.lizzieDefault"));
    add(SYSTEM_DEFAULT_FONT, t("FontList.systemDefault"));
    // Java fills the whole list before selecting (4513-4526): until the system list arrives the
    // requested font is kept as its own item instead of falling back to index 0.
    for (const font of loaded ? fonts : [selected]) if (font !== LIZZIE_DEFAULT_FONT && font !== SYSTEM_DEFAULT_FONT) add(font, font);
    pick(selected);
  };
  const combo: FontCombo = {
    select,
    setFonts(next) {
      const selected = combo.get();
      fonts = next;
      loaded = true;
      fill(selected);
    },
    get: () => select.value || LIZZIE_DEFAULT_FONT,
    set: (name) => (loaded ? pick(name) : fill(name)),
  };
  select.addEventListener("change", () => pick(select.value));
  fill(LIZZIE_DEFAULT_FONT);
  return combo;
}

/** An image path field with its "..." chooser (addAssetRow, 4117-4143; getImagePath 5283-5302). */
function pathControl(key: string, onChange: () => void): { root: HTMLElement; input: HTMLInputElement; browse: HTMLButtonElement } {
  const root = el("span", "appearance-path");
  const input = el("input");
  input.type = "text";
  input.dataset.key = key;
  input.addEventListener("change", onChange);
  const browse = el("button", undefined, "...");
  browse.type = "button";
  browse.dataset.key = `${key}.browse`;
  browse.onclick = async () => {
    const path = await chooseAppearanceImage();
    if (path) {
      input.value = path;
      onChange();
    }
  };
  root.append(input, browse);
  return { root, input, browse };
}

// ------------------------------------------------------------------ 棋盘预览

const PREVIEW_W = 220;
const PREVIEW_H = 180;

/** pnlBoardPreview.paintComponent (ConfigDialog2.java:4980-5196) for the draft: the background and
 *  the board from (30, 30), both tiled from the origin or in their pure colours, 19 lines every 30 px
 *  from (60, 60), a black stone at (120, 150) and a white one at (90, 120) with radius 14. */
function paintPreview(canvas: HTMLCanvasElement, v: ThemeSettings, images: LoadedImages): void {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = PREVIEW_W * dpr;
  canvas.height = PREVIEW_H * dpr;
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, PREVIEW_W, PREVIEW_H);
  const tiled = (img: HTMLImageElement, x: number) => {
    g.fillStyle = g.createPattern(img, "repeat")!;
    g.fillRect(x, x, Math.max(img.naturalWidth, PREVIEW_W), Math.max(img.naturalHeight, PREVIEW_H));
  };
  const pureBackground = cssColor(opaque(v.pureBackgroundColor));
  if (!images.background || v.usePureBackground) {
    g.fillStyle = pureBackground;
    g.fillRect(0, 0, PREVIEW_W, PREVIEW_H);
  } else tiled(images.background, 0);
  const pureBoard = cssColor(opaque(v.pureBoardColor));
  if (!images.board || v.usePureBoard) {
    g.fillStyle = pureBoard;
    g.fillRect(30, 30, PREVIEW_W, PREVIEW_H);
  } else tiled(images.board, 30);
  const [x, y, square] = [60, 60, 30];
  const radius = square / 2 - 1;
  const size = radius * 2 + 1;
  const r = (radius * v.shadowSize) / 100;
  const shadow = Math.trunc(r * 0.2) || 1;
  const farther = Math.trunc(r * 0.17) || 1;
  g.strokeStyle = "#000";
  g.lineWidth = 1;
  g.beginPath();
  for (let k = 0; k < 19; k++) {
    g.moveTo(x, y + square * k + 0.5);
    g.lineTo(PREVIEW_H, y + square * k + 0.5);
    g.moveTo(x + square * k + 0.5, y);
    g.lineTo(x + square * k + 0.5, PREVIEW_W);
  }
  g.stroke();
  const oval = (cx: number, cy: number, rr: number) => {
    g.beginPath();
    g.ellipse(cx + 0.5, cy + 0.5, rr + 0.5, rr + 0.5, 0, 0, Math.PI * 2);
    g.fill();
  };
  const stone = (sx: number, sy: number, black: boolean, img: HTMLImageElement | undefined) => {
    if (v.showStoneShadow) {
      const top = g.createRadialGradient(sx, sy, 0, sx, sy, radius + shadow);
      top.addColorStop(0.3, "rgba(50,50,50,0.588)");
      top.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = top;
      oval(sx, sy, radius + shadow);
      const lower = g.createRadialGradient(sx + shadow, sy + shadow, 0, sx + shadow, sy + shadow, radius + farther);
      lower.addColorStop(0.6, "rgba(0,0,0,0.549)");
      lower.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = lower;
      oval(sx + shadow, sy + shadow, radius + farther);
    }
    if (v.usePureStone || !img) {
      // drawStoneSimple (5264-5273).
      g.fillStyle = black ? "#000" : "#fff";
      oval(sx, sy, radius);
      if (!black) {
        g.strokeStyle = "#000";
        g.lineWidth = Math.max(radius / 16, 1);
        g.beginPath();
        g.arc(sx, sy, radius, 0, Math.PI * 2);
        g.stroke();
      }
    } else g.drawImage(img, sx - radius, sy - radius, size, size);
  };
  stone(x + square * 2, y + square * 3, true, images.black);
  stone(x + square, y + square * 2, false, images.white);
}

// ------------------------------------------------------------------ page

/** JOptionPane.showInputDialog(dialog, null, ConfigDialog2.inputThemeNameTitle, INFORMATION_MESSAGE)
 *  (ConfigDialog2.java:4456-4461): the title alone, no message above the field. */
async function themeNameDialog(): Promise<string | null> {
  const body = document.createElement("div");
  body.className = "display-input";
  const field = document.createElement("input");
  field.type = "text";
  field.setAttribute("aria-label", t("ConfigDialog2.inputThemeNameTitle"));
  body.append(field);
  queueMicrotask(() => field.focus());
  return (await ask(t("ConfigDialog2.inputThemeNameTitle"), body, product("ok"), product("cancel"))) ? field.value : null;
}

function buildThemePage(host: HTMLElement, dialog: SettingsDialog): { save: () => Promise<boolean> } {
  const view = state.view!;
  const committed = { name: view.settings.appearance.theme, values: view.settings.theme };
  let catalog: AppearanceCatalog | null = null;
  let selected = committed.name;
  /** The values of the selected theme as read; image fields of the default theme round-trip. */
  let base: ThemeSettings = structuredClone(committed.values);
  const redraw = () => schedulePreview();

  // 主题外观: 当前主题 and 棋盘预览.
  const profile = card(host, t("ConfigDialog2.modern.theme.title"), t("ConfigDialog2.modern.theme.subtitle"));
  const themes = el("select");
  themes.dataset.key = "settings.theme.current";
  const add = el("button", undefined, t("ConfigDialog2.addTheme"));
  add.type = "button";
  add.dataset.key = "ConfigDialog2.addTheme";
  const remove = el("button", undefined, t("ConfigDialog2.deleteTheme"));
  remove.type = "button";
  remove.dataset.key = "ConfigDialog2.deleteTheme";
  row(profile, "config.theme.current", t("ConfigDialog2.modern.theme.current"), t("ConfigDialog2.modern.theme.currentSub"), themes, add, remove);
  const preview = el("canvas", "appearance-preview");
  preview.style.width = `${PREVIEW_W}px`;
  preview.style.height = `${PREVIEW_H}px`;
  preview.dataset.key = "settings.theme.preview";
  row(profile, null, t("ConfigDialog2.modern.theme.preview"), t("ConfigDialog2.modern.theme.previewSub"), preview);

  // 线条与字体.
  const lines = card(host, t("ConfigDialog2.modern.theme.lines"), t("ConfigDialog2.modern.theme.linesSub"));
  const winrateWidth = spinner("settings.theme.winrateStrokeWidth", 0.1, 10, 0.1, t("LizzieConfig.title.winrateStrokeWidth"));
  const scoreWidth = spinner("settings.theme.scoreLeadStrokeWidth", 0.1, 10, 0.1, t("LizzieConfig.title.scoreLeadStrokeWidth"));
  row(lines, "config.theme.winrate-width", t("ConfigDialog2.modern.theme.winrateWidth"), t("ConfigDialog2.modern.theme.winrateWidthSub"), winrateWidth, scoreWidth);
  const barWidth = spinner("settings.theme.minimumBlunderBarWidth", 1, 10, 1, t("LizzieConfig.title.minimumBlunderBarWidth"));
  row(lines, "config.theme.blunder-width", t("ConfigDialog2.modern.theme.blunderWidth"), t("ConfigDialog2.modern.theme.blunderWidthSub"), barWidth);
  const shadowLabel = t("LizzieConfig.title.shadowSize");
  const showShadow = check("settings.theme.showStoneShadow", shadowLabel);
  const shadowSize = spinner("settings.theme.shadowSize", 1, 150, 1, shadowLabel);
  showShadow.addEventListener("change", () => (shadowSize.disabled = !showShadow.checked));
  row(lines, "config.theme.shadow", t("ConfigDialog2.modern.theme.shadow"), t("ConfigDialog2.modern.theme.shadowSub"), labelled(showShadow, shadowLabel), shadowSize);
  const infoFont = fontCombo("settings.theme.fontName");
  row(lines, "config.theme.info-font", t("ConfigDialog2.modern.theme.infoFont"), t("ConfigDialog2.modern.theme.infoFontSub"), infoFont.select);
  const uiFont = fontCombo("settings.theme.uiFontName");
  row(lines, "config.theme.ui-font", t("ConfigDialog2.modern.theme.uiFont"), t("ConfigDialog2.modern.theme.uiFontSub"), uiFont.select);
  const winrateFont = fontCombo("settings.theme.winrateFontName");
  row(lines, "config.theme.winrate-font", t("ConfigDialog2.modern.theme.winrateFont"), t("ConfigDialog2.modern.theme.winrateFontSub"), winrateFont.select);

  // 图片与材质.
  const assets = card(host, t("ConfigDialog2.modern.theme.assets"), t("ConfigDialog2.modern.theme.assetsSub"));
  const assetHint = t("ConfigDialog2.modern.theme.assetHint");
  const pureBackgroundLabel = t("LizzieConfig.title.chkPureBackground");
  const pureBackground = check("settings.theme.usePureBackground", pureBackgroundLabel);
  const pureBackgroundColor = colorControl("settings.theme.pureBackgroundColor", false, false);
  const backgroundPath = pathControl("settings.theme.backgroundImage", redraw);
  row(assets, "config.theme.background-image", t("ConfigDialog2.modern.theme.backgroundImage"), assetHint, labelled(pureBackground, pureBackgroundLabel), pureBackgroundColor.root, backgroundPath.root);
  const pureBoardLabel = t("LizzieConfig.title.chkPureBoard");
  const pureBoard = check("settings.theme.usePureBoard", pureBoardLabel);
  const pureBoardColor = colorControl("settings.theme.pureBoardColor", false, false);
  const boardPath = pathControl("settings.theme.boardImage", redraw);
  row(assets, "config.theme.board-image", t("ConfigDialog2.modern.theme.boardImage"), assetHint, labelled(pureBoard, pureBoardLabel), pureBoardColor.root, boardPath.root);
  const blackPath = pathControl("settings.theme.blackStoneImage", redraw);
  row(assets, "config.theme.black-stone-image", t("ConfigDialog2.modern.theme.blackStoneImage"), assetHint, blackPath.root);
  const pureStoneLabel = t("LizzieConfig.title.chkPureStone");
  const pureStone = check("settings.theme.usePureStone", pureStoneLabel);
  const whitePath = pathControl("settings.theme.whiteStoneImage", redraw);
  row(assets, "config.theme.white-stone-image", t("ConfigDialog2.modern.theme.whiteStoneImage"), assetHint, labelled(pureStone, pureStoneLabel), whitePath.root);
  const backgroundFilter = digits("settings.theme.backgroundFilter");
  row(assets, "config.theme.background-blur", t("ConfigDialog2.modern.theme.blur"), t("ConfigDialog2.modern.theme.blurSub"), backgroundFilter);
  /** The pure toggles' listeners (4934-4976): a named theme's paths follow them; the filter
   *  follows 纯色背景 for every theme. */
  const syncEnabled = () => {
    const named = selected !== DEFAULT_THEME;
    backgroundPath.input.disabled = backgroundPath.browse.disabled = !named || pureBackground.checked;
    boardPath.input.disabled = boardPath.browse.disabled = !named || pureBoard.checked;
    for (const p of [blackPath, whitePath]) p.input.disabled = p.browse.disabled = !named || pureStone.checked;
    backgroundFilter.disabled = pureBackground.checked;
    shadowSize.disabled = !showShadow.checked;
  };
  for (const toggle of [pureBackground, pureBoard, pureStone]) toggle.addEventListener("change", syncEnabled);

  // 颜色与标记.
  const colors = card(host, t("ConfigDialog2.modern.theme.colors"), t("ConfigDialog2.modern.theme.colorsSub"));
  const colorHint = t("ConfigDialog2.modern.colorRowHint");
  const colorRow = (target: Target, title: string, key: string) => {
    const control = colorControl(key, true, true);
    row(colors, target, title, colorHint, control.root);
    return control;
  };
  const winrateColor = colorRow("config.theme.winrate-color", t("ConfigDialog2.modern.theme.winrateColor"), "settings.theme.winrateLineColor");
  const missColor = colorRow("config.theme.missing-color", t("ConfigDialog2.modern.theme.missingColor"), "settings.theme.winrateMissLineColor");
  const blunderColor = colorRow("config.theme.blunder-color", t("ConfigDialog2.modern.theme.blunderColor"), "settings.theme.blunderBarColor");
  const scoreColor = colorRow("config.theme.score-color", t("ConfigDialog2.modern.theme.scoreColor"), "settings.theme.scoremeanLineColor");
  const commentBackground = colorRow("config.theme.comment-background", t("ConfigDialog2.modern.theme.commentBackground"), "settings.theme.commentBackgroundColor");
  const commentText = colorRow("config.theme.comment-text", t("ConfigDialog2.modern.theme.commentText"), "settings.theme.commentFontColor");
  const bestMove = colorRow("config.theme.best-move-color", t("ConfigDialog2.modern.theme.bestMove"), "settings.theme.bestMoveColor");
  const commentFontSize = digits("settings.theme.commentFontSize");
  row(colors, "config.theme.comment-font-size", t("ConfigDialog2.modern.theme.commentFontSize"), t("ConfigDialog2.modern.theme.commentFontSizeSub"), commentFontSize);
  // rdoStoneIndicator* in rowOf order; values per getStoneIndicatorType (6098-6106).
  const indicators: [number, string][] = [[1, t("ConfigDialog2.circle")], [0, t("ConfigDialog2.triangle")], [2, t("ConfigDialog2.solid")], [3, t("ConfigDialog2.empty")]];
  const indicatorGroup = `appearance-indicator-${Math.random().toString(36).slice(2)}`;
  const indicatorInputs = indicators.map(([value, text]) => {
    const input = el("input");
    input.type = "radio";
    input.name = indicatorGroup;
    input.value = String(value);
    input.dataset.key = `settings.theme.stoneIndicatorType.${value}`;
    return labelled(input, text);
  });
  row(colors, "config.theme.stone-indicator", t("ConfigDialog2.modern.theme.indicator"), t("ConfigDialog2.modern.theme.indicatorSub"), ...indicatorInputs);
  const commentNodeTitle = t("ConfigDialog2.modern.theme.commentNode");
  const showCommentNode = check("settings.theme.showCommentNodeColor", commentNodeTitle);
  const commentNodeColor = colorControl("settings.theme.commentNodeColor", true, false);
  row(colors, "config.theme.comment-node-color", commentNodeTitle, t("ConfigDialog2.modern.theme.commentNodeSub"), showCommentNode, commentNodeColor.root);

  // 选点评估关注外框.
  const tracking = view.settings.tracking;
  const trackingCard = card(host, t("ConfigDialog2.modern.trackingAppearance.title"), t("ConfigDialog2.modern.trackingAppearance.subtitle"));
  const outlineTitle = t("ConfigDialog2.modern.trackingAppearance.outline");
  const outline = check("settings.tracking.showOutline", outlineTitle);
  outline.checked = tracking.showOutline;
  row(trackingCard, "config.tracking.outline", outlineTitle, t("ConfigDialog2.modern.trackingAppearance.outlineSub"), outline);
  const opacity = el("input");
  opacity.type = "range";
  opacity.min = "0";
  opacity.max = "100";
  opacity.step = "1";
  opacity.value = String(tracking.outlineOpacity);
  opacity.dataset.key = "settings.tracking.outlineOpacity";
  opacity.setAttribute("aria-label", t("ConfigDialog2.modern.trackingAppearance.outlineOpacity"));
  const opacityValue = el("span", undefined, `${opacity.value}%`);
  opacity.oninput = () => (opacityValue.textContent = `${opacity.value}%`);
  row(trackingCard, "config.tracking.outline-opacity", t("ConfigDialog2.modern.trackingAppearance.outlineOpacity"), t("ConfigDialog2.modern.trackingAppearance.outlineOpacitySub"), opacity, opacityValue);

  // 错误节点.
  const blunders = card(host, t("ConfigDialog2.modern.theme.blunders"), t("ConfigDialog2.modern.theme.blundersSub"));
  const tableWrap = el("div", "appearance-blunder-table");
  const table = el("table", "appearance-rules");
  const head = el("tr");
  head.append(el("th", undefined, t("LizzieConfig.title.blunderThresholds")), el("th", undefined, t("LizzieConfig.title.blunderColor")));
  const thead = el("thead");
  thead.append(head);
  const tbody = el("tbody");
  table.append(thead, tbody);
  tableWrap.append(table);
  interface RuleRow { tr: HTMLTableRowElement; threshold: HTMLInputElement; color: ColorControl }
  let rules: RuleRow[] = [];
  let selectedRule = -1;
  const selectRule = (k: number) => {
    selectedRule = k;
    rules.forEach((rule, n) => rule.tr.classList.toggle("selected", n === k));
  };
  /** BlunderNodeTableModel.addRow: a threshold (empty for 添加) and its colour. */
  const addRule = (threshold: number | null, color: readonly number[]) => {
    const tr = el("tr");
    const input = el("input");
    input.type = "number";
    input.step = "any";
    input.dataset.key = "settings.theme.blunderWinrateThreshold";
    input.value = threshold === null ? "" : String(threshold);
    const control = colorControl("settings.theme.blunderNodeColor", true, false);
    control.set(color);
    const a = el("td");
    a.append(input);
    const b = el("td");
    b.append(control.root);
    tr.append(a, b);
    const rule = { tr, threshold: input, color: control };
    tr.addEventListener("pointerdown", () => selectRule(rules.indexOf(rule)));
    tr.addEventListener("focusin", () => selectRule(rules.indexOf(rule)));
    rules.push(rule);
    tbody.append(tr);
  };
  const setRules = (thresholds: readonly number[], colorsByRow: readonly (readonly number[])[]) => {
    tbody.replaceChildren();
    rules = [];
    selectedRule = -1;
    thresholds.forEach((v, k) => addRule(v, colorsByRow[k]));
  };
  const addRow = el("button", undefined, t("LizzieConfig.button.add"));
  addRow.type = "button";
  addRow.dataset.key = "LizzieConfig.button.add";
  addRow.onclick = () => addRule(null, [255, 255, 255]);
  const removeRow = el("button", undefined, t("LizzieConfig.button.remove"));
  removeRow.type = "button";
  removeRow.dataset.key = "LizzieConfig.button.remove";
  removeRow.onclick = () => {
    if (selectedRule < 0 || selectedRule >= rules.length) return;
    rules[selectedRule].tr.remove();
    rules.splice(selectedRule, 1);
    selectRule(-1);
  };
  const resetRows = el("button", undefined, t("LizzieConfig.button.reset"));
  resetRows.type = "button";
  resetRows.dataset.key = "LizzieConfig.button.reset";
  // 重置 (ConfigDialog2.java:4826-4835): saveConfig commits the whole dialog, resetBlunderColor
  // (Config.java:1491-1508) then writes the default rules into the active theme and the dialog
  // reopens on 主题. Saving the draft with the default rules is that same end state.
  resetRows.onclick = async () => {
    setRules(DEFAULT_BLUNDER_THRESHOLDS, DEFAULT_BLUNDER_COLORS);
    await dialog.saveAndReopen("theme");
  };
  const ruleButtons = el("div", "appearance-rule-buttons");
  ruleButtons.append(addRow, removeRow, resetRows);
  row(blunders, "config.theme.blunder-rules", t("ConfigDialog2.modern.theme.blunderRules"), t("ConfigDialog2.modern.theme.blunderRulesSub"), tableWrap, ruleButtons);
  const useScoreDiff = check("settings.theme.useScorediffInVariationTree", t("LizzieConfig.chkUseScoreDiff"));
  // DoubleDocument: the field takes a decimal number.
  const scoreDiffPercent = el("input", "appearance-number");
  scoreDiffPercent.type = "text";
  scoreDiffPercent.inputMode = "decimal";
  scoreDiffPercent.dataset.key = "settings.theme.scorediffInVariationTreeFactor";
  scoreDiffPercent.title = t("LizzieConfig.lblUseScoreDiffPercent");
  scoreDiffPercent.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /[^\d.\-]/.test(e.data ?? "")) e.preventDefault();
  });
  useScoreDiff.addEventListener("change", () => (scoreDiffPercent.disabled = !useScoreDiff.checked));
  row(blunders, "config.theme.score-blunders", t("ConfigDialog2.modern.theme.scoreBlunders"), t("ConfigDialog2.modern.theme.scoreBlundersSub"), useScoreDiff, scoreDiffPercent);

  /** readThemeValues / readDefaultTheme: the controls show `v`. */
  const fill = (v: ThemeSettings) => {
    base = structuredClone(v);
    pureStone.checked = v.usePureStone;
    pureBackground.checked = v.usePureBackground;
    pureBackgroundColor.set(opaque(v.pureBackgroundColor));
    pureBoard.checked = v.usePureBoard;
    pureBoardColor.set(opaque(v.pureBoardColor));
    showShadow.checked = v.showStoneShadow;
    winrateWidth.value = String(v.winrateStrokeWidth);
    scoreWidth.value = String(v.scoreLeadStrokeWidth);
    barWidth.value = String(v.minimumBlunderBarWidth);
    shadowSize.value = String(v.shadowSize);
    infoFont.set(v.fontName);
    uiFont.set(v.uiFontName);
    winrateFont.set(v.winrateFontName);
    const named = selected !== DEFAULT_THEME;
    backgroundPath.input.value = named ? v.backgroundImage : DEFAULT_PATHS.background;
    boardPath.input.value = named ? v.boardImage : DEFAULT_PATHS.board;
    blackPath.input.value = named ? v.blackStoneImage : DEFAULT_PATHS.black;
    whitePath.input.value = named ? v.whiteStoneImage : DEFAULT_PATHS.white;
    winrateColor.set(v.winrateLineColor);
    missColor.set(v.winrateMissLineColor);
    blunderColor.set(v.blunderBarColor);
    scoreColor.set(v.scoremeanLineColor);
    commentBackground.set(v.commentBackgroundColor);
    commentText.set(v.commentFontColor);
    // The best move swatch shows the colour at alpha 240 (6171-6173, 6350-6352).
    bestMove.set([...v.bestMoveColor.slice(0, 3), 240]);
    commentFontSize.value = String(v.commentFontSize);
    backgroundFilter.value = String(v.backgroundFilter);
    for (const label of indicatorInputs) {
      const input = label.querySelector("input")!;
      input.checked = Number(input.value) === v.stoneIndicatorType;
    }
    showCommentNode.checked = v.showCommentNodeColor;
    commentNodeColor.set(v.commentNodeColor);
    useScoreDiff.checked = v.useScorediffInVariationTree;
    scoreDiffPercent.value = String(v.scorediffInVariationTreeFactor * 100);
    scoreDiffPercent.disabled = !useScoreDiff.checked;
    setRules(v.blunderWinrateThresholds, v.blunderNodeColors);
    remove.disabled = !named;
    syncEnabled();
    redraw();
  };

  /** writeThemeValues / writeDefaultTheme: the draft as the controls show it. */
  const collect = (): ThemeSettings => {
    const named = selected !== DEFAULT_THEME;
    const number = (input: HTMLInputElement, fallback: number) => (input.value === "" || !Number.isFinite(Number(input.value)) ? fallback : Number(input.value));
    // finalizeEditedBlunderColors (6473-6479): the rows sorted by threshold, a blank one as 0.
    const rows = rules
      .map((rule) => ({ threshold: rule.threshold.value.trim() === "" || !Number.isFinite(Number(rule.threshold.value)) ? 0 : Number(rule.threshold.value), color: rule.color.get() }))
      .sort((a, b) => a.threshold - b.threshold);
    // Theme.parseBlunderNodeRuleSet (385-423) only accepts strictly increasing thresholds; Java saves
    // other tables and then reads the default rules back, which is what is saved here.
    const valid = rows.length > 0 && rows.every((r, k) => k === 0 || r.threshold > rows[k - 1].threshold);
    const checkedIndicator = indicatorInputs.map((l) => l.querySelector("input")!).find((input) => input.checked);
    // Utils.parseTextToDouble(txtPercentScoreDiff, previous * 100) / 100.
    const percent = Number.parseFloat(scoreDiffPercent.value);
    return {
      ...base,
      usePureStone: pureStone.checked,
      usePureBoard: pureBoard.checked,
      pureBoardColor: pureBoardColor.get(),
      usePureBackground: pureBackground.checked,
      pureBackgroundColor: pureBackgroundColor.get(),
      showStoneShadow: showShadow.checked,
      winrateStrokeWidth: number(winrateWidth, base.winrateStrokeWidth),
      scoreLeadStrokeWidth: number(scoreWidth, base.scoreLeadStrokeWidth),
      minimumBlunderBarWidth: number(barWidth, base.minimumBlunderBarWidth),
      shadowSize: number(shadowSize, base.shadowSize),
      fontName: infoFont.get(),
      uiFontName: uiFont.get(),
      winrateFontName: winrateFont.get(),
      backgroundImage: named ? backgroundPath.input.value.trim() : base.backgroundImage,
      boardImage: named ? boardPath.input.value.trim() : base.boardImage,
      blackStoneImage: named ? blackPath.input.value.trim() : base.blackStoneImage,
      whiteStoneImage: named ? whitePath.input.value.trim() : base.whiteStoneImage,
      winrateLineColor: winrateColor.get(),
      winrateMissLineColor: missColor.get(),
      blunderBarColor: blunderColor.get(),
      scoremeanLineColor: scoreColor.get(),
      stoneIndicatorType: checkedIndicator ? Number(checkedIndicator.value) : base.stoneIndicatorType,
      showCommentNodeColor: showCommentNode.checked,
      commentNodeColor: commentNodeColor.get(),
      commentBackgroundColor: commentBackground.get(),
      commentFontColor: commentText.get(),
      bestMoveColor: bestMove.get(),
      commentFontSize: intValue(commentFontSize),
      backgroundFilter: intValue(backgroundFilter),
      blunderWinrateThresholds: valid ? rows.map((r) => r.threshold) : [...DEFAULT_BLUNDER_THRESHOLDS],
      blunderNodeColors: valid ? rows.map((r) => r.color) : DEFAULT_BLUNDER_COLORS.map((c) => [...c]),
      useScorediffInVariationTree: useScoreDiff.checked,
      scorediffInVariationTreeFactor: (Number.isFinite(percent) ? percent : base.scorediffInVariationTreeFactor * 100) / 100,
    };
  };

  // 棋盘预览 repaints with the draft (Java's 100 ms timer); images are decoded again only when the
  // image choices change, and only the newest request paints.
  let previewKey = "";
  let previewImages: LoadedImages = {};
  let previewGeneration = 0;
  let previewPending = false;
  function schedulePreview() {
    if (previewPending) return;
    previewPending = true;
    requestAnimationFrame(async () => {
      previewPending = false;
      if (!preview.isConnected && !host.isConnected) return;
      const draft = collect();
      // The default theme previews its own images (readDefaultTheme's fixed paths), a named
      // theme its effective ones (Theme.backgroundPath / boardPath with the custom override).
      const settings = selected === DEFAULT_THEME
        ? { ...view.settings.appearance, theme: selected, customBoardImage: "", customBackgroundImage: "" }
        : { ...view.settings.appearance, theme: selected };
      const key = JSON.stringify([settings, draft.backgroundImage, draft.boardImage, draft.blackStoneImage, draft.whiteStoneImage]);
      if (key !== previewKey) {
        previewKey = key;
        const mine = ++previewGeneration;
        const loaded = await loadThemeImages(settings, draft);
        if (mine !== previewGeneration) return;
        previewImages = loaded.images;
      }
      paintPreview(preview, collect(), previewImages);
    });
  }
  // Every control's edit repaints the preview with the draft.
  host.addEventListener("input", redraw);
  host.addEventListener("change", redraw);

  /** The theme combo box: 默认 first, then the catalog's themes (loadThemeTab 4372-4384). */
  const fillThemes = () => {
    themes.replaceChildren();
    const names = catalog?.themes.map((e) => e.name) ?? [DEFAULT_THEME, ...(committed.name !== DEFAULT_THEME ? [committed.name] : [])];
    for (const name of names) {
      const option = el("option", undefined, name === DEFAULT_THEME ? t("LizzieConfig.title.defaultTheme") : name);
      option.value = name;
      themes.append(option);
    }
    // A theme gone from the list (deleted) leaves the combo on its first item, 默认.
    if (!names.includes(selected)) {
      selected = DEFAULT_THEME;
      const values = entry(DEFAULT_THEME);
      if (values) fill(values);
    }
    themes.value = selected;
  };
  const entry = (name: string) => catalog?.themes.find((e) => e.name === name)?.values;
  themes.addEventListener("change", () => {
    selected = themes.value;
    const values = selected === committed.name ? committed.values : entry(selected);
    if (values) fill(values);
  });
  const reloadCatalog = async () => {
    try {
      catalog = await appearanceCatalog();
    } catch {
      return;
    }
    for (const combo of [infoFont, uiFont, winrateFont]) combo.setFonts(catalog.fonts);
    fillThemes();
  };

  // 添加 (4449-4477): a name not yet in the list, compared case-insensitively; then
  // setVisible(false) + openConfigDialog2(1) (4471-4472) drops every unsaved edit.
  add.onclick = async () => {
    const name = await themeNameDialog();
    if (name === null) return;
    const names = [t("LizzieConfig.title.defaultTheme"), ...(catalog?.themes.map((e) => e.name) ?? [])];
    if (names.some((n) => n.toLowerCase() === name.toLowerCase())) {
      await message(t("Message.title"), t("ConfigDialog2.duplicateThemeName"));
      return;
    }
    const result = await run({ type: "AddTheme", name });
    if (result && result.type !== "Refused") await dialog.reopen("theme");
  };
  // 删除 (4391-4435): asks first; the default theme cannot be deleted. Reopens like 添加
  // (4424-4425): the committed theme, or 默认 when it was the one deleted.
  remove.onclick = async () => {
    if (selected === DEFAULT_THEME) return;
    if (!(await confirm(t("LizzieFrame.warning"), `${t("ConfigDialog2.deleteThemeWarning")}'${selected}' ?`))) return;
    const result = await run({ type: "DeleteTheme", name: selected });
    if (result && result.type !== "Refused") await dialog.reopen("theme");
  };

  fillThemes();
  fill(committed.values);
  void reloadCatalog();

  return {
    save: async () => {
      const nowTracking = state.view?.settings.tracking ?? tracking;
      if (outline.checked !== nowTracking.showOutline || Number(opacity.value) !== nowTracking.outlineOpacity) {
        const result = await run({ type: "SetTrackingSettings", settings: { ...nowTracking, showOutline: outline.checked, outlineOpacity: Number(opacity.value) } });
        if (!result || result.type === "Refused") return false;
      }
      const values = collect();
      const settings = { ...view.settings.appearance, ...state.view?.settings.appearance, theme: selected };
      if (selected === committed.name && JSON.stringify(values) === JSON.stringify(committed.values)) return true;
      const result = await run({ type: "SetAppearanceSettings", settings, theme: values });
      return !!result && result.type !== "Refused";
    },
  };
}

SETTINGS_PAGES.theme = buildThemePage;
// validateSettingRows(content, SettingSection.THEME): every row id is known to 功能搜索 from start-up,
// before the page is ever built (registration is metadata only).
registerSettingTargets("theme", TARGETS);
