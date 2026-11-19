// Icons the shared table (design/icons.ts) does not have yet, drawn in its grammar: 16-unit line
// drawings in currentColor with 1.5 strokes and round caps and joins (icons.css). The AI 解说
// window, the independent board windows and the float board use them. Static markup; nothing from
// data enters these strings.
import "../design/icons.css";

const SVG = "http://www.w3.org/2000/svg";

const ICONS = {
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
  /** Position: arrows in four directions. */
  move: '<path d="M8 2v12M2 8h12M6.25 3.75 8 2l1.75 1.75M6.25 12.25 8 14l1.75-1.75M3.75 6.25 2 8l1.75 1.75M12.25 6.25 14 8l-1.75 1.75"/>',
} as const;

export type LocalIconName = keyof typeof ICONS;

/** A fresh `<svg class="icon">`, built like design/icons.ts `icon`: hidden from assistive
 *  technology (its host carries the name) and tagged `data-icon`. */
export function localIcon(name: LocalIconName): SVGSVGElement {
  const svg = document.createElementNS(SVG, "svg");
  svg.setAttribute("class", "icon");
  svg.setAttribute("viewBox", "0 0 16 16");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("focusable", "false");
  svg.dataset.icon = name;
  svg.innerHTML = ICONS[name];
  return svg;
}
