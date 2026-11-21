// The follow's status texts (OnlineText): a Java resource key with its arguments, composed the way
// the Java code concatenates them, Java's hard-coded browser texts verbatim, or this product's own
// message for what Java has no text for.
import type { OnlineText } from "../contract/generated";
import { product, registerProduct, t } from "../i18n";

// 对局室 (#/unite/<room>) follows through the page's own requests: without the 弈客 page webview
// there is nothing to poll.
registerProduct({
  ZhCn: { "Online.uniteNeedsPage": "对局室需要在弈客网页窗口中打开后才能同步。" },
  ZhTw: { "Online.uniteNeedsPage": "對局室需要在弈客網頁視窗中開啟後才能同步。" },
  EnUs: { "Online.uniteNeedsPage": "Open the game room in the Yike web window to sync it." },
  JaJp: { "Online.uniteNeedsPage": "対局ルームを同期するには、Yike の Web ウィンドウで開いてください。" },
  KoKr: { "Online.uniteNeedsPage": "대국방은 Yike 웹 창에서 연 뒤에 동기화할 수 있습니다." },
  ThTh: { "Online.uniteNeedsPage": "ต้องเปิดห้องเกมในหน้าต่างเว็บ Yike ก่อนจึงจะซิงค์ได้" },
});

/** Literal t() calls keep the keys the core sends in the Java string extraction. */
function javaText(key: string): string {
  switch (key) {
    case "YikeLiveDialog.loading": return t("YikeLiveDialog.loading");
    case "YikeLiveDialog.loaded": return t("YikeLiveDialog.loaded");
    case "YikeLiveDialog.loadFailed": return t("YikeLiveDialog.loadFailed");
    case "YikeLiveDialog.noSelection": return t("YikeLiveDialog.noSelection");
    case "YikeLiveDialog.syncNoSgf": return t("YikeLiveDialog.syncNoSgf");
    case "YikeLiveDialog.syncing": return t("YikeLiveDialog.syncing");
    case "YikeLiveDialog.syncedWatching": return t("YikeLiveDialog.syncedWatching");
    case "YikeLiveDialog.syncedFinished": return t("YikeLiveDialog.syncedFinished");
    case "YikeLiveDialog.syncFailed": return t("YikeLiveDialog.syncFailed");
    case "YikeLiveDialog.curveCompleting": return t("YikeLiveDialog.curveCompleting");
    case "YikeLiveDialog.curveWaiting": return t("YikeLiveDialog.curveWaiting");
    case "YikeLiveDialog.curveUpdated": return t("YikeLiveDialog.curveUpdated");
    case "YikeLiveDialog.curveUpToDate": return t("YikeLiveDialog.curveUpToDate");
    case "YikeLiveDialog.curveFailed": return t("YikeLiveDialog.curveFailed");
    case "OnlineDialog.lblError.text": return t("OnlineDialog.lblError.text");
    default: return t(key);
  }
}

/** String.format's %s / %d and MessageFormat's {n}, in argument order. */
function substitute(template: string, args: string[]): { text: string; used: boolean } {
  let next = 0;
  let used = false;
  const text = template
    .replace(/%[sd]/g, () => { used = true; return args[next++] ?? ""; })
    .replace(/\{(\d+)\}/g, (_, n: string) => { used = true; return args[Number(n)] ?? ""; });
  return { text, used };
}

export function onlineText(text: OnlineText): string {
  const { key, args } = text;
  switch (key) {
    // Java's hard-coded Chinese browser statuses, composed by the core, in every language.
    case "Online.text":
      return args[0] ?? "";
    case "Online.uniteNeedsPage":
      return product("Online.uniteNeedsPage");
    // loaded + " " + N + " " + loadedSuffix (YikeLiveDialog.java:241-247).
    case "YikeLiveDialog.loaded":
      return `${javaText(key)} ${args[0] ?? ""} ${t("YikeLiveDialog.loadedSuffix")}`;
    // syncNoSgf + " " + game name (YikeLiveDialog.java:292-295); the core's own one has no name.
    case "YikeLiveDialog.syncNoSgf":
      return args.length ? `${javaText(key)} ${args[0]}` : javaText(key);
  }
  const template = javaText(key);
  const { text: filled, used } = substitute(template, args);
  // The rest append ": " + detail (loadFailed, syncing, syncFailed, curveFailed).
  return used || args.length === 0 ? filled : `${template}: ${args.join(": ")}`;
}
