// Line glyphs of TeacherDialogStyle (ModeGlyph, settings, commentary icon) and the lucide-style
// icons TeacherSettingsStyle.icon loads (link, sliders-horizontal, browser-globe, lock-keyhole,
// book-open). Static markup; nothing from data enters these strings.
const svg = (body: string, viewBox = "0 0 24 24") =>
  `<svg viewBox="${viewBox}" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const ICONS = {
  next: svg(`<circle cx="12" cy="12" r="9.5"/><path d="M10 8.5v7l5.5-3.5z"/>`),
  range: svg(`<rect x="3.5" y="5" width="17" height="15.5" rx="2"/><path d="M3.5 10h17M8 3v4M16 3v4"/>`),
  whole: svg(`<path d="M6 20v-6M12 20V9M18 20V4"/>`, "0 0 24 24").replace('stroke-width="2"', 'stroke-width="3.2"'),
  settings: svg(
    `<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09a1.65 1.65 0 0 0-1.08-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09a1.65 1.65 0 0 0 1.51-1.08 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>`,
  ),
  /** TeacherDialogStyle.commentaryIcon: a speech card with its tail and two text lines. */
  commentary: svg(`<rect x="5" y="5" width="42" height="31" rx="7"/><path d="M31 36v10l-9-10M14 16h24M14 25h16"/>`, "0 0 52 52").replace('stroke-width="2"', 'stroke-width="3"'),
  link: svg(`<path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/>`),
  sliders: svg(`<path d="M21 4h-7M10 4H3M21 12h-9M8 12H3M21 20h-5M12 20H3M14 2v4M8 10v4M16 18v4"/>`),
  browser: svg(`<rect x="2" y="2" width="82" height="62" rx="5"/><path d="M2 16h82"/><circle cx="10" cy="9" r="1.5"/><circle cx="16" cy="9" r="1.5"/><circle cx="22" cy="9" r="1.5"/><circle cx="43" cy="40" r="14"/><path d="M29 40h28M43 26a21 21 0 0 1 0 28M43 26a21 21 0 0 0 0 28"/>`, "0 0 86 66"),
  lock: svg(`<rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/><circle cx="12" cy="16" r="1"/><path d="M12 17v2"/>`),
  book: svg(`<path d="M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z"/><path d="M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z"/>`),
};

/** An element holding one of the static glyphs. */
export function icon(name: keyof typeof ICONS): HTMLElement {
  const holder = document.createElement("span");
  holder.style.display = "contents";
  holder.innerHTML = ICONS[name];
  return holder;
}
