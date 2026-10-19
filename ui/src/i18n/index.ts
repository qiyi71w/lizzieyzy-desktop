// UI text. Java's own wording comes from its DisplayStrings bundles (javaStrings.json, regenerated
// by tools/extract_java_strings.py); the few product messages without a Java counterpart are in
// PRODUCT below. The language is fixed for a session, like Java (changing it needs a restart).
import data from "./javaStrings.json";
import type { Language } from "../contract/generated";

type Table = Record<string, string>;
const JAVA = data.strings as Record<string, Table>;

/** Messages that exist only in this product (capability not migrated yet) and the JDK option-pane
 *  button labels Java takes from the platform. Other languages fall back to English. */
const PRODUCT: Record<string, Table> = {
  ZhCn: { unavailable: "此功能尚未在 LizzieYzy Tauri 中提供", diagnosticsLogs: "app.log：{0}，crash.log：{1}", none: "无", ok: "确定", cancel: "取消", close: "关闭" },
  ZhTw: { unavailable: "此功能尚未在 LizzieYzy Tauri 中提供", diagnosticsLogs: "app.log：{0}，crash.log：{1}", none: "無", ok: "確定", cancel: "取消", close: "關閉" },
  EnUs: { unavailable: "Not available in LizzieYzy Tauri yet", diagnosticsLogs: "app.log: {0}, crash.log: {1}", none: "none", ok: "OK", cancel: "Cancel", close: "Close" },
};

let lang: Exclude<Language, "System"> = "ZhCn";

export function setLanguage(l: Language): void {
  lang = l === "System" ? "ZhCn" : l;
}

export function language(): Language {
  return lang;
}

/** Java `Lizzie.isChinese` (bundle value "yes"). */
export function isChinese(): boolean {
  return t("Lizzie.isChinese") === "yes";
}

/** Java resource string; unknown keys are shown literally (Java uses literal text for some items). */
export function t(key: string): string {
  return JAVA[lang]?.[key] ?? JAVA.ZhCn?.[key] ?? key;
}

/** MessageFormat-style `{0}` substitution. */
export function format(text: string, ...args: (string | number)[]): string {
  return text.replace(/\{(\d+)\}/g, (_, k) => `${args[Number(k)] ?? ""}`);
}

export function product(key: string, ...args: (string | number)[]): string {
  const table = PRODUCT[lang] ?? PRODUCT.EnUs;
  return format(table[key] ?? PRODUCT.EnUs[key] ?? key, ...args);
}
