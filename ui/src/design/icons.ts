// The main window's icon family: 16-unit line drawings in currentColor, 1.5 strokes with round
// caps and joins. Only stones, colour swatches and dots are filled. Semantic colours (allowed /
// avoided regions, stones, painting pens) come from the --icon-* variables in icons.css, so each
// theme draws them without filters.
import "./icons.css";

const SVG = "http://www.w3.org/2000/svg";

/** Each icon's inner markup in a `0 0 16 16` box; `ic-*` classes take their colour from icons.css. */
const ICONS = {
  "new-file":
    '<path d="M9 1.75H4.25a.75.75 0 0 0-.75.75v11c0 .41.34.75.75.75h7.5a.75.75 0 0 0 .75-.75V5.25z"/><path d="M9 1.75v3.5h3.5M8 7.5v4.5M5.75 9.75h4.5"/>',
  open: '<path d="M2 12.5V3.25c0-.41.34-.75.75-.75h3L7.25 4h5c.41 0 .75.34.75.75V6.5"/><path d="M2 12.5 4.06 7.06a.75.75 0 0 1 .7-.56h9.3c.35 0 .6.35.48.68l-1.9 4.84a.75.75 0 0 1-.7.48z"/>',
  save: '<path d="M2.5 3.25c0-.41.34-.75.75-.75h7.5l2.75 2.75v7.5c0 .41-.34.75-.75.75h-9.5a.75.75 0 0 1-.75-.75z"/><path d="M5 2.5v3h4.5v-3M5 13.5v-4h6v4"/>',
  hawkeye: '<path d="M1.5 8S3.75 3.5 8 3.5 14.5 8 14.5 8 12.25 12.5 8 12.5 1.5 8 1.5 8z"/><circle cx="8" cy="8" r="2"/>',
  "move-rank": '<circle cx="8" cy="8" r="6"/><circle class="ic-status" cx="8" cy="8" r="3"/>',
  "swap-turn": '<path d="M2.5 5h11M11 2.5 13.5 5 11 7.5M13.5 11h-11M5 8.5 2.5 11 5 13.5"/>',
  "set-main": '<path d="M8 2v8.5M4.75 7.25 8 10.5l3.25-3.25M3 13.5h10"/>',
  "back-to-main": '<path d="M6 3 2.5 6.5 6 10M2.5 6.5h7a4 4 0 0 1 4 4V13"/>',
  "control-panel":
    '<path d="M2 3.5h6.5M11.5 3.5H14M10 2v3M2 8h2M7 8h7M5.5 6.5v3M2 12.5h7.5M12.5 12.5H14M11 11v3"/>',
  markup: '<path d="M10.6 2.4a1.4 1.4 0 0 1 2 0l1 1a1.4 1.4 0 0 1 0 2L6.25 12.75 2.5 13.5l.75-3.75z"/><path d="m9.25 3.75 3 3"/>',
  "markup-letter": '<path d="M3.5 13.5 8 2.5l4.5 11M5.1 10h5.8"/>',
  "markup-number": '<path d="M5.5 4.75 8.5 2.5v11M5.5 13.5h6"/>',
  "markup-cross": '<path d="m3.5 3.5 9 9M12.5 3.5l-9 9"/>',
  "markup-square": '<rect x="3" y="3" width="10" height="10" rx="1"/>',
  "markup-triangle": '<path d="M8 2.5 14 13H2z"/>',
  eraser:
    '<path d="M8.6 2.4a1.4 1.4 0 0 1 2 0l3 3a1.4 1.4 0 0 1 0 2L8 13H5.25L2.4 10.15a1.4 1.4 0 0 1 0-2z"/><path d="m5.25 5.75 5 5M8 13h5.5"/>',
  clear: '<circle cx="8" cy="8" r="6"/><path d="m5.75 5.75 4.5 4.5M10.25 5.75l-4.5 4.5"/>',
  "clear-all":
    '<path d="M2.5 4.25h11M6 4.25V2.75c0-.41.34-.75.75-.75h2.5c.41 0 .75.34.75.75v1.5M4 4.25l.7 8.82c.03.4.36.68.75.68h5.1c.39 0 .72-.29.75-.68L12 4.25M6.75 6.75v4.5M9.25 6.75v4.5"/>',
  "paint-brush":
    '<path d="m13.75 2.25-6 6"/><path d="M7.75 8.25c-.9-.9-2.35-.9-3.25 0-.75.75-.6 1.85-.9 2.75-.25.75-.75 1.5-1.85 2 1.75.9 4.25.75 5.5-.5 1-1 1.05-3.7.5-4.25z"/>',
  "stone-black": '<circle class="ic-stone-black" cx="8" cy="8" r="6"/>',
  "stone-white": '<circle class="ic-stone-white" cx="8" cy="8" r="6"/>',
  "stone-alternate": '<circle class="ic-stone-black" cx="6" cy="6" r="4.5"/><circle class="ic-stone-white" cx="10" cy="10" r="4.5"/>',
  pass: '<path d="M4.75 8.5V5a1 1 0 0 1 2 0v2.5V3a1 1 0 0 1 2 0v4.5V3.75a1 1 0 0 1 2 0V8V5.5a1 1 0 0 1 2 0v4.25a4.25 4.25 0 0 1-4.25 4.25h-.6a4 4 0 0 1-3.1-1.47L2.6 9.85a1 1 0 0 1 1.45-1.37z"/>',
  flash: '<path d="M9.25 1.75 3.75 9H8l-1.25 5.25L12.25 7H8z"/>',
  "chevron-up": '<path d="M4 10 8 6l4 4"/>',
  "chevron-down": '<path d="m4 6 4 4 4-4"/>',
  play: '<path d="M5 3.25v9.5L12.75 8z"/>',
  pause: '<path d="M5.5 3.5v9M10.5 3.5v9"/>',
  "region-allow":
    '<path class="ic-allow" d="M2.5 5.5v-2a1 1 0 0 1 1-1h2M10.5 2.5h2a1 1 0 0 1 1 1v2M13.5 10.5v2a1 1 0 0 1-1 1h-2M5.5 13.5h-2a1 1 0 0 1-1-1v-2M8 5.75v4.5M5.75 8h4.5"/>',
  "region-avoid":
    '<path class="ic-avoid" d="M2.5 5.5v-2a1 1 0 0 1 1-1h2M10.5 2.5h2a1 1 0 0 1 1 1v2M13.5 10.5v2a1 1 0 0 1-1 1h-2M5.5 13.5h-2a1 1 0 0 1-1-1v-2M6.25 6.25l3.5 3.5M9.75 6.25l-3.5 3.5"/>',
  "point-only": '<circle class="ic-allow" cx="8" cy="8" r="6"/><circle class="ic-allow-fill" cx="8" cy="8" r="2.25"/>',
  "point-add": '<circle class="ic-allow" cx="8" cy="8" r="6"/><path class="ic-allow" d="M8 5.5v5M5.5 8h5"/>',
  "point-avoid": '<circle class="ic-avoid" cx="8" cy="8" r="6"/><path class="ic-avoid" d="m3.75 3.75 8.5 8.5"/>',
  "track-point":
    '<circle cx="8" cy="8" r="4.5"/><path d="M8 1.5v2.5M8 12v2.5M1.5 8H4M12 8h2.5"/><circle class="ic-fill" cx="8" cy="8" r="1.25"/>',
  "priority-point": '<path d="M8 13.5V3M4 7l4-4 4 4"/>',
  "go-to-move": '<circle cx="8" cy="8" r="6"/><path d="M8 4.75v6M5.5 8.25 8 10.75l2.5-2.5"/>',
  back: '<path d="M13 8H3M7 4 3 8l4 4"/>',
  forward: '<path d="M3 8h10M9 4l4 4-4 4"/>',
  "step-first": '<path d="M4 3.5v9M12 3.5 7.5 8l4.5 4.5"/>',
  "step-back-ten": '<path d="M8 3.5 3.5 8 8 12.5M12.5 3.5 8 8l4.5 4.5"/>',
  "step-back": '<path d="M10 3.5 5.5 8l4.5 4.5"/>',
  "step-forward": '<path d="m6 3.5 4.5 4.5L6 12.5"/>',
  "step-forward-ten": '<path d="m8 3.5 4.5 4.5L8 12.5M3.5 3.5 8 8l-4.5 4.5"/>',
  "step-last": '<path d="M12 3.5v9M4 3.5 8.5 8 4 12.5"/>',
  search: '<circle cx="7" cy="7" r="4.75"/><path d="m10.5 10.5 3.25 3.25"/>',
  undo: '<path d="M2.25 8A5.75 5.75 0 1 0 8 2.25 6.2 6.2 0 0 0 3.7 4L2.25 5.5M2.25 2.25V5.5H5.5"/>',
  refresh: '<path d="M13.75 8A5.75 5.75 0 1 1 8 2.25 6.2 6.2 0 0 1 12.3 4l1.45 1.5M13.75 2.25V5.5H10.5"/>',
  close: '<path d="m4 4 8 8M12 4l-8 8"/>',
  more: '<circle class="ic-fill" cx="3.75" cy="8" r="1.15"/><circle class="ic-fill" cx="8" cy="8" r="1.15"/><circle class="ic-fill" cx="12.25" cy="8" r="1.15"/>',
  spark: '<path d="M8 2.25 9.5 6.5l4.25 1.5L9.5 9.5 8 13.75 6.5 9.5 2.25 8 6.5 6.5z"/>',
  help: '<circle cx="8" cy="8" r="6.25"/><path d="M6.25 6.4a1.8 1.8 0 1 1 2.55 1.64c-.5.23-.8.62-.8 1.16v.3"/><circle class="ic-fill" cx="8" cy="11.4" r=".85"/>',
  /** 设置: an eight-toothed cog. */
  settings:
    '<path d="M6.8 3.4 6.9 1.85h2.2L9.2 3.4l1.2.5 1.17-1.03 1.56 1.56L12.1 5.6l.5 1.2 1.55.1v2.2l-1.55.1-.5 1.2 1.03 1.17-1.56 1.56-1.17-1.03-1.2.5-.1 1.55H6.9l-.1-1.55-1.2-.5-1.17 1.03-1.56-1.56L3.9 10.4l-.5-1.2-1.55-.1V6.9l1.55-.1.5-1.2-1.03-1.17L4.43 2.87 5.6 3.9z"/><circle cx="8" cy="8" r="2"/>',
  /** AI 解说: a speech card with two lines of text. */
  commentary:
    '<path d="M2.5 3.75c0-.69.56-1.25 1.25-1.25h8.5c.69 0 1.25.56 1.25 1.25v6c0 .69-.56 1.25-1.25 1.25H9l-3 2.5V11H3.75c-.69 0-1.25-.56-1.25-1.25z"/><path d="M5.25 5.75h5.5M5.25 8.25h3.5"/>',
  /** Kept on this device / locked in place. */
  lock: '<rect x="3" y="7" width="10" height="7" rx="1.25"/><path d="M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2M8 9.75v1.75"/>',
  /** Always on top. */
  pin: '<path d="M5.75 2.5h4.5M6.75 2.5v3.75L4.5 9.25h7L9.25 6.25V2.5M8 9.25v4.25"/>',
  plus: '<path d="M8 3.5v9M3.5 8h9"/>',
  minus: '<path d="M3.5 8h9"/>',
  /** A yes in a table cell (更多引擎's 预加载 / 默认 / SSH). */
  check: '<path d="m3.25 8.5 3 3 6.5-7"/>',
  /** An engine of the engine menu (Java's ● ◐ ○): ready, loading, not running. */
  "engine-ready": '<circle class="ic-fill" cx="8" cy="8" r="4.5"/>',
  "engine-loading": '<circle cx="8" cy="8" r="3.75"/><path class="ic-fill" d="M8 4.25a3.75 3.75 0 0 0 0 7.5z"/>',
  "engine-stopped": '<circle cx="8" cy="8" r="3.75"/>',
  /** Position: arrows in four directions. */
  move: '<path d="M8 2v12M2 8h12M6.25 3.75 8 2l1.75 1.75M6.25 12.25 8 14l1.75-1.75M3.75 6.25 2 8l1.75 1.75M12.25 6.25 14 8l-1.75 1.75"/>',
  "pen-blue": '<rect class="ic-pen ic-pen-blue" x="2.5" y="2.5" width="11" height="11" rx="2"/>',
  "pen-green": '<rect class="ic-pen ic-pen-green" x="2.5" y="2.5" width="11" height="11" rx="2"/>',
  "pen-red": '<rect class="ic-pen ic-pen-red" x="2.5" y="2.5" width="11" height="11" rx="2"/>',
  /** 综合设置's pages: 棋盘与显示, 棋谱加载, 引擎与分析, 对局与操作, 主题外观, 高级选项, 关于. */
  "nav-display": '<rect x="2" y="2.5" width="12" height="9" rx="1.5"/><path d="M6 14h4M8 11.5V14"/>',
  "nav-kifu": '<path d="M4 1.75h5.5L13 5.25V14H4z"/><path d="M9.5 1.75v3.5H13M6.5 8.5h4M6.5 11h4"/>',
  "nav-engine": '<rect x="4" y="4" width="8" height="8" rx="1.5"/><path d="M6.5 1.75V4M9.5 1.75V4M6.5 12v2.25M9.5 12v2.25M1.75 6.5H4M1.75 9.5H4M12 6.5h2.25M12 9.5h2.25"/>',
  "nav-play": '<circle cx="6" cy="8" r="3.75"/><circle cx="10.5" cy="8" r="3.75"/>',
  "nav-theme": '<path d="M8 1.75a6.25 6.25 0 1 0 0 12.5c1.05 0 1.3-.95.7-1.6-.6-.65-.2-1.9.9-1.9h1.65A3 3 0 0 0 14.25 7.8C14.25 4.5 11.45 1.75 8 1.75z"/><circle cx="5" cy="7" r=".9"/><circle cx="8" cy="4.75" r=".9"/><circle cx="11" cy="6.5" r=".9"/>',
  "nav-advanced": '<path d="M2.5 4h11M2.5 8h11M2.5 12h11"/><circle cx="6" cy="4" r="1.4"/><circle cx="10.5" cy="8" r="1.4"/><circle cx="5" cy="12" r="1.4"/>',
  "nav-about": '<circle cx="8" cy="8" r="6.25"/><path d="M8 7.25v4M8 4.9v.1"/>',
} as const;

export type IconName = keyof typeof ICONS;

/** A fresh `<svg class="icon">` for the named icon, hidden from assistive technology (its host
 *  carries the name or tooltip) and tagged `data-icon` for automation. */
export function icon(name: IconName): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  svg.dataset.icon = name;
  svg.innerHTML = ICONS[name];
  return svg;
}
