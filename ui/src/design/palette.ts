// 界面颜色: the four colours a theme may set for the interface (条栏底色, 按钮底色, 强调色, 文字颜色)
// and the token values that follow from them. A colour the theme leaves unset is the skin's own;
// tokens that depend on two of the colours (the dialogs' ground between the bars and the controls,
// the text steps over the bars) mix in the skin's value of the unset one. The workbench keeps its
// own tokens (stage.ts) and the danger fill stays red. No contrast is enforced: the colours are the
// user's choice.

/** The four colours as a theme stores them, [r, g, b] each; an empty one is the skin's own. */
export interface InterfaceColors {
  chrome: readonly number[];
  control: readonly number[];
  accent: readonly number[];
  text: readonly number[];
}

export const SKIN_COLORS: InterfaceColors = { chrome: [], control: [], accent: [], text: [] };

/** A skin's own values of the four colours, of the status hues and of the select arrow. */
export interface SkinColors extends InterfaceColors {
  danger: readonly number[];
  warning: readonly number[];
  success: readonly number[];
  chevron: string;
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

/** The skin's colours as `style`, the computed style of an element that takes them, holds them. */
export function skinColorsOf(style: CSSStyleDeclaration): SkinColors {
  const value = (name: string) => style.getPropertyValue(name).trim();
  const rgb = (name: string) => rgbaOf(value(name)).slice(0, 3);
  return {
    chrome: rgb("--ds-chrome"),
    control: rgb("--ds-raised"),
    accent: rgb("--ds-accent"),
    text: rgb("--ds-text"),
    danger: rgb("--ds-danger"),
    warning: rgb("--ds-warning-text"),
    success: rgb("--ds-success-text"),
    chevron: value("--ds-chevron"),
  };
}

const BLACK = [0, 0, 0];
const WHITE = [255, 255, 255];

/** `a` moved the fraction `t` of the way to `b`. */
function mix(a: readonly number[], b: readonly number[], t: number): number[] {
  return [0, 1, 2].map((k) => Math.round(a[k] + (b[k] - a[k]) * t));
}

function hex(c: readonly number[]): string {
  return `#${[0, 1, 2].map((k) => c[k].toString(16).padStart(2, "0")).join("")}`;
}

function alpha(c: readonly number[], a: number): string {
  return `rgba(${c[0]}, ${c[1]}, ${c[2]}, ${a})`;
}

/** Whether white reads better than black on `c` (WCAG relative luminance below 0.179). */
function dark(c: readonly number[]): boolean {
  const linear = (v: number) => (v / 255 <= 0.03928 ? v / 255 / 12.92 : ((v / 255 + 0.055) / 1.055) ** 2.4);
  return 0.2126 * linear(c[0]) + 0.7152 * linear(c[1]) + 0.0722 * linear(c[2]) < 0.179;
}

/** The token values for the colours a theme set (`own`), the others being the skin's (`skin`):
 *  only the tokens a set colour changes. `color-scheme` follows the controls, so the browser's own
 *  parts of a control (a select's list) match it. */
export function paletteTokens(own: InterfaceColors, skin: SkinColors): Record<string, string> {
  const isSet = (c: readonly number[]) => c.length === 3;
  const chrome = isSet(own.chrome) ? own.chrome : skin.chrome;
  const control = isSet(own.control) ? own.control : skin.control;
  const accent = isSet(own.accent) ? own.accent : skin.accent;
  const text = isSet(own.text) ? own.text : skin.text;
  const out: Record<string, string> = {};
  if (isSet(own.chrome)) out["--ds-chrome"] = hex(chrome);
  if (isSet(own.control)) {
    out["--ds-raised"] = hex(control);
    out["--ds-popup"] = hex(control);
    out["--ds-field"] = hex(control);
    out["color-scheme"] = dark(control) ? "dark" : "light";
  }
  if (isSet(own.chrome) || isSet(own.control)) out["--ds-panel"] = hex(mix(chrome, control, 0.5));
  if (isSet(own.chrome) || isSet(own.text)) {
    out["--ds-sunken"] = hex(mix(chrome, text, 0.06));
    const status: [string, readonly number[]][] = [["danger", skin.danger], ["warning", skin.warning], ["success", skin.success]];
    // Status text keeps its hue (danger stays red): darkened beside dark text, lightened beside
    // light text.
    const shade = dark(text) ? BLACK : WHITE;
    for (const [name, hue] of status) {
      out[`--ds-${name}-text`] = hex(mix(hue, shade, 0.3));
      out[`--ds-${name}-soft`] = hex(mix(chrome, hue, 0.16));
      out[`--ds-${name}-soft-line`] = hex(mix(chrome, hue, 0.4));
    }
  }
  if (isSet(own.text)) {
    const secondary = mix(text, chrome, 0.2);
    out["--ds-text"] = hex(text);
    out["--ds-text-2"] = hex(secondary);
    out["--ds-text-3"] = hex(mix(text, chrome, 0.3));
    out["--ds-text-off"] = hex(mix(text, chrome, 0.6));
    out["--ds-hover"] = alpha(text, 0.08);
    out["--ds-pressed"] = alpha(text, 0.15);
    out["--ds-well"] = alpha(text, 0.1);
    out["--ds-line"] = alpha(text, 0.16);
    out["--ds-line-strong"] = alpha(text, 0.34);
    out["--ds-shadow-cap"] = `0 0 0 0.5px ${alpha(text, 0.3)}, 0 1px 1.5px ${alpha(BLACK, 0.16)}`;
    out["--ds-shadow-pop"] = `0 0 0 1px ${alpha(text, 0.12)}, 0 6px 20px -4px ${alpha(BLACK, 0.3)}`;
    out["--ds-shadow-dialog"] = `0 0 0 1px ${alpha(text, 0.14)}, 0 18px 48px -8px ${alpha(BLACK, 0.4)}`;
    // The skin's arrow drawn in the secondary text colour.
    out["--ds-chevron"] = skin.chevron.replace(/stroke='%23[0-9a-fA-F]{6}'/, `stroke='%23${hex(secondary).slice(1)}'`);
  }
  if (isSet(own.accent)) {
    out["--ds-accent"] = hex(accent);
    out["--ds-accent-hover"] = hex(mix(accent, dark(chrome) ? WHITE : BLACK, 0.12));
    out["--ds-accent-pressed"] = hex(mix(accent, BLACK, 0.24));
    out["--ds-on-accent"] = hex(dark(accent) ? WHITE : BLACK);
    out["--ds-accent-soft"] = alpha(accent, 0.17);
    out["--ds-accent-soft-line"] = alpha(accent, 0.4);
    out["--ds-ring"] = alpha(accent, 0.6);
  }
  if (isSet(own.accent) || isSet(own.text)) out["--ds-accent-text"] = hex(mix(accent, text, 0.25));
  return out;
}
