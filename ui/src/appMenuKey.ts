/** ⌘Q (Quit), ⌘H (Hide) and ⌥⌘H (Hide Others): the macOS application menu takes these before
 *  Java's Input.keyPressed sees them, so no window of this page claims them and the system menu
 *  bar (chrome/nativeMenu.ts, src-tauri `install_macos_quit` until it is in) acts. Elsewhere Meta
 *  is the system's key, never Java's Ctrl. */
export function appMenuKey(e: KeyboardEvent): boolean {
  return e.metaKey && (e.code === "KeyQ" || e.code === "KeyH");
}
