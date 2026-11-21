// Remote compute (远程算力) presentation helpers: the account seam, Java's display formatting and
// the plan/link rules of RemoteComputeConfig that the dialog applies to its unsaved drafts.
// Commands go straight to `dispatch`, not through the workbench queue: an account request may wait
// on the network and must not hold back board commands or the cancel request that ends it.
// Nothing here logs a command, reply or link: they can carry passwords, codes or link secrets.
import { dispatch, isApiError } from "../contract/client";
import type { CloudCatalog, CloudCommand, CloudFailure, CloudReply, CloudStartFailure, Command, CommandResult, Weight } from "../contract/generated";
import { format, product, registerProduct, t } from "../i18n";

// Why the 智子 engine did not start (ZhiziGtpTransport's startup messages, which Java wrote in
// Chinese only).
registerProduct({
  ZhCn: {
    cloudStartNotReady: "智子云算力 60 秒内没有准备好，请稍后重试。",
    cloudStartDisconnected: "智子云算力在准备好之前断开了连接，请稍后重试。",
    cloudStartRefused: "智子云算力拒绝了连接，请检查账号和套餐。",
    cloudStartRefusedDetail: "智子云算力拒绝了连接：{0}",
  },
  ZhTw: {
    cloudStartNotReady: "智子雲算力 60 秒內沒有準備好，請稍後重試。",
    cloudStartDisconnected: "智子雲算力在準備好之前中斷了連線，請稍後重試。",
    cloudStartRefused: "智子雲算力拒絕了連線，請檢查帳號和方案。",
    cloudStartRefusedDetail: "智子雲算力拒絕了連線：{0}",
  },
  EnUs: {
    cloudStartNotReady: "Zhizi Cloud was not ready within 60 seconds. Try again later.",
    cloudStartDisconnected: "Zhizi Cloud closed the connection before it was ready. Try again later.",
    cloudStartRefused: "Zhizi Cloud refused the connection. Check the account and plan.",
    cloudStartRefusedDetail: "Zhizi Cloud refused the connection: {0}",
  },
  JaJp: {
    cloudStartNotReady: "Zhizi Cloud は 60 秒以内に準備できませんでした。しばらくしてから再試行してください。",
    cloudStartDisconnected: "Zhizi Cloud は準備が整う前に接続を切断しました。しばらくしてから再試行してください。",
    cloudStartRefused: "Zhizi Cloud が接続を拒否しました。アカウントとプランを確認してください。",
    cloudStartRefusedDetail: "Zhizi Cloud が接続を拒否しました：{0}",
  },
  KoKr: {
    cloudStartNotReady: "Zhizi Cloud가 60초 안에 준비되지 않았습니다. 잠시 후 다시 시도하세요.",
    cloudStartDisconnected: "Zhizi Cloud가 준비되기 전에 연결을 끊었습니다. 잠시 후 다시 시도하세요.",
    cloudStartRefused: "Zhizi Cloud가 연결을 거부했습니다. 계정과 요금제를 확인하세요.",
    cloudStartRefusedDetail: "Zhizi Cloud가 연결을 거부했습니다: {0}",
  },
  ThTh: {
    cloudStartNotReady: "Zhizi Cloud ไม่พร้อมภายใน 60 วินาที กรุณาลองใหม่ภายหลัง",
    cloudStartDisconnected: "Zhizi Cloud ตัดการเชื่อมต่อก่อนที่จะพร้อม กรุณาลองใหม่ภายหลัง",
    cloudStartRefused: "Zhizi Cloud ปฏิเสธการเชื่อมต่อ กรุณาตรวจสอบบัญชีและแพ็กเกจ",
    cloudStartRefusedDetail: "Zhizi Cloud ปฏิเสธการเชื่อมต่อ: {0}",
  },
});

/** Java RemoteComputeConfig *_ZHIZI_ARGS, in the dialog's 连接方式 order. */
export const VIP_ARGS = "--platform all --engine-type go --gpu-type vip-share --kata-name katago-TENSORRT --kata-weight 10b512t";
export const PRESETS: { label: () => string; args: string }[] = [
  { label: () => t("RemoteCompute.plan.vip"), args: VIP_ARGS },
  { label: () => t("RemoteCompute.plan.1x"), args: "--platform all --engine-type go --gpu-type 1x --kata-name katago-TENSORRT --kata-weight 10b512t" },
  { label: () => t("RemoteCompute.plan.3x"), args: "--platform all --engine-type go --gpu-type 3x --kata-name katago-TENSORRT --kata-weight 10b512t" },
  { label: () => t("RemoteCompute.plan.6x"), args: "--platform all --engine-type go --gpu-type 6x --kata-name katago-TENSORRT --kata-weight 10b512t" },
  { label: () => t("RemoteCompute.plan.12x"), args: "--platform all --engine-type go --gpu-type 12x --kata-name katago-TENSORRT --kata-weight 10b512t" },
  { label: () => t("RemoteCompute.plan.24x"), args: "--platform all --engine-type go --gpu-type 24x --kata-name katago-TENSORRT --kata-weight 10b512t" },
  { label: () => t("RemoteCompute.plan.1xCuda"), args: "--platform all --engine-type go --gpu-type 1x --kata-name katago-CUDA --kata-weight 10b512t" },
];
/** ZhiziEngineCatalog.DEFAULT_WEIGHT. */
export const DEFAULT_WEIGHT = "10b512t";
/** ZhiziApiClient.BALANCE_TOP_UP_PRESETS_FEN; the account adapter accepts only these. */
export const TOP_UP_PRESETS_FEN = [1000, 3000, 5000, 10000];

export type AccountOutcome = { reply: CloudReply; failure?: undefined } | { reply?: undefined; failure: CloudFailure };

function failure(key: string): CloudFailure {
  return { key, status: 0, retryAfterSeconds: 0, retryable: false, uncertain: false, cancelled: false, authenticationInvalidated: false };
}

/** One group of a window's account requests (Java: a dialog's own SwingWorkers). Its id scopes
 *  `CloudCancelRequest`, so closing one window never ends another window's requests or an
 *  engine start. `cancelOnClose` is false for work Java lets finish after its dialog closes
 *  (RemoteComputeDialog.runBackground: sign-in, codes, reset); only its reply is dropped then. */
export interface RequestOwner {
  readonly id: number;
  readonly cancelOnClose: boolean;
  closed: boolean;
  pending: number;
}

// Safe integers unique across reloads of the page: milliseconds × 1000 plus a counter.
let lastId = 0;

export function requestOwner(cancelOnClose = true): RequestOwner {
  lastId = Math.max(lastId + 1, Date.now() * 1000);
  return { id: lastId, cancelOnClose, closed: false, pending: 0 };
}

/** One account operation of `owner`; null once the owner has closed (its reply is dropped).
 *  Failures arrive as sanitized keys; a transport error becomes a generic one. */
export async function account(op: CloudCommand, owner: RequestOwner): Promise<AccountOutcome | null> {
  if (owner.closed) return null;
  owner.pending++;
  let outcome: AccountOutcome;
  try {
    const result = await dispatch({ type: "CloudAccount", op, owner: owner.id });
    if (result.type === "Cloud") outcome = { reply: result.reply };
    else if (result.type === "CloudError") outcome = { failure: result.failure };
    else outcome = { failure: failure("invalid_response") };
  } catch {
    outcome = { failure: failure("unknown_error") };
  } finally {
    owner.pending--;
  }
  return owner.closed ? null : outcome;
}

/** A non-account cloud command; an ApiError is returned instead of thrown. */
export async function cloudCommand(command: Command): Promise<CommandResult | { type: "Failed"; message: string }> {
  try {
    return await dispatch(command);
  } catch (e) {
    return { type: "Failed", message: isApiError(e) ? e.message : "" };
  }
}

/** Closes `owner`: its late replies are dropped, and its requests still in flight are cancelled
 *  (scoped to this owner only) unless Java lets them finish. */
export function closeOwner(owner: RequestOwner): void {
  if (owner.closed) return;
  owner.closed = true;
  if (owner.pending > 0 && owner.cancelOnClose) void dispatch({ type: "CloudCancelRequest", owner: owner.id }).catch(() => undefined);
}

/** The Java message an ApiError names by key (QR image unreadable, no local engine); raw detail is
 *  never shown. */
export function keyedDetail(message: string): string {
  switch (message) {
    case "RemoteCompute.error.invalidImage": return t("RemoteCompute.error.invalidImage");
    case "RemoteCompute.error.localMissing": return t("RemoteCompute.error.localMissing");
    default: return "";
  }
}

function optionValue(args: string, option: string, fallback: string): string {
  const parts = args.trim().split(/\s+/);
  for (let i = 0; i < parts.length - 1; i++) if (parts[i] === option) return parts[i + 1].trim();
  return fallback;
}

function withoutOption(args: string, option: string): string {
  const parts = args.trim() ? args.trim().split(/\s+/) : [];
  const result: string[] = [];
  for (let i = 0; i < parts.length; i++) {
    if (parts[i] === option) { i++; continue; }
    result.push(parts[i]);
  }
  return result.join(" ");
}

/** ZhiziEngineCatalog.isSelectableWeight. */
export function selectableWeight(name: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9._+-]{0,63}$/.test(name.trim());
}

export function kataWeight(args: string): string {
  return optionValue(args, "--kata-weight", DEFAULT_WEIGHT);
}

/** RemoteComputeConfig.withKataWeight. */
export function withKataWeight(args: string, weight: string): string {
  const value = selectableWeight(weight) ? weight.trim() : DEFAULT_WEIGHT;
  const parts = (args.trim() ? args : VIP_ARGS).trim().split(/\s+/);
  const result: string[] = [];
  let replaced = false;
  for (let i = 0; i < parts.length; i++) {
    result.push(parts[i]);
    if (parts[i] === "--kata-weight") {
      if (i + 1 < parts.length) i++;
      result.push(value);
      replaced = true;
    }
  }
  if (!replaced) result.push("--kata-weight", value);
  return result.join(" ");
}

export function samePlan(a: string, b: string): boolean {
  return withoutOption(a, "--kata-weight") === withoutOption(b, "--kata-weight");
}

/** RemoteComputeConfig.displayNameForZhiziWeight. */
export function weightName(weight: string): string {
  const name = weight.trim();
  switch (name.toLowerCase()) {
    case "28bnbt": return "28B NBT";
    case "18bnbt": return "18B NBT";
    case "fdx": return "FDX · 40B NBT";
    case "10b384t": return t("RemoteCompute.weightName.10b384t");
    case "10b512t": return t("RemoteCompute.weightName.10b512t");
    case "11b768t": return t("RemoteCompute.weightName.11b768t");
  }
  if (/^\d+b$/i.test(name)) return name.toUpperCase();
  return name || "28B NBT";
}

/** RemoteComputeConfig.displayNameForZhiziArgs: 智子云算力 <plan> · <model> · <backend>. */
export function zhiziDisplayName(args: string): string {
  const backend = args.includes("katago-CUDA") ? "CUDA" : "TensorRT";
  const gpu = optionValue(args, "--gpu-type", "vip-share");
  let plan: string;
  switch (gpu) {
    case "vip-share": plan = t("RemoteCompute.plan.vipShort"); break;
    case "1x": plan = backend === "CUDA" ? t("RemoteCompute.plan.1xCuda") : t("RemoteCompute.plan.1x"); break;
    case "3x": plan = t("RemoteCompute.plan.3x"); break;
    case "6x": plan = t("RemoteCompute.plan.6x"); break;
    case "12x": plan = t("RemoteCompute.plan.12x"); break;
    case "24x": plan = t("RemoteCompute.plan.24x"); break;
    default: plan = gpu.trim() ? `On-demand ${gpu}` : "On-demand";
  }
  return `${t("RemoteCompute.zhizi")} ${plan} · ${weightName(kataWeight(args))} · ${backend}`;
}

export function confirmedWeight(weight: Weight): boolean {
  return weight.source === "OFFICIAL_DOCUMENTED" || weight.source === "BUILT_IN_CURRENT" || weight.source === "SERVER_CAPABILITIES";
}

/** ZhiziEngineCatalog.resolveSupportedWeight. */
export function resolveWeight(catalog: CloudCatalog, requested: string): string {
  return catalog.weights.find((w) => w.name.toLowerCase() === requested.trim().toLowerCase())?.name ?? catalog.defaultWeight;
}

/** The 智子模型 entry text (RemoteComputeDialog.WeightItem.displayLabel). */
export function weightLabel(weight: Weight, isDefault: boolean): string {
  const readable = weightName(weight.name);
  let label = readable.toLowerCase() === weight.name.toLowerCase() ? readable : `${readable} (${weight.name})`;
  const detail = weightDetail(weight);
  if (detail) label += ` · ${detail}`;
  if (isDefault) label += ` · ${t("RemoteCompute.weightDefault")}`;
  switch (weight.source) {
    case "OFFICIAL_DOCUMENTED": label += ` · ${t("RemoteCompute.weightSource.official")}`; break;
    case "BUILT_IN_CURRENT": label += ` · ${t("RemoteCompute.weightSource.builtin")}`; break;
    case "SERVER_CAPABILITIES": label += ` · ${t("RemoteCompute.weightSource.server")}`; break;
    case "USER_PRESERVED": label += ` · ${t("RemoteCompute.weightSource.preserved")}`; break;
    default: label += ` · ${t("RemoteCompute.weightSource.legacy")}`;
  }
  return label;
}

function weightDetail(weight: Weight): string {
  const name = weight.name.trim();
  if (name.toLowerCase() === "fdx") return t("RemoteCompute.weightHint.fdx");
  if (name.toLowerCase() === "20b") return t("RemoteCompute.weightHint.20b");
  const description = weight.description.replace(/\s+/g, " ").trim();
  const lower = description.toLowerCase(), lowerName = name.toLowerCase();
  if (!description || lower === lowerName || lower === `weight for ${lowerName}` || lower === `${lowerName} weight` || description.toLowerCase() === `${lowerName}权重`) return "";
  if (lower.startsWith(lowerName)) {
    const rest = description.slice(name.length).replace(/^[\s,，:：()（）-]+/, "");
    return rest === "权重" ? "" : rest;
  }
  return description;
}

/** RemoteComputeConfig.normalizeCustomWebSocketUrl. */
export function normalizeLink(value: string): string {
  let text = [...value.trim()].map((c) => {
    const code = c.charCodeAt(0);
    if (code === 0x3000) return " ";
    return code >= 0xff01 && code <= 0xff5e ? String.fromCharCode(code - 0xfee0) : c;
  }).join("");
  if ((text.startsWith("\"") && text.endsWith("\"")) || (text.startsWith("'") && text.endsWith("'"))) text = text.slice(1, -1).trim();
  if (text.startsWith("完善://")) text = `ws://${text.slice("完善://".length)}`;
  const lower = text.toLowerCase();
  const ws = lower.indexOf("ws://"), wss = lower.indexOf("wss://");
  const at = ws < 0 ? wss : wss < 0 ? ws : Math.min(ws, wss);
  if (at > 0) text = text.slice(at);
  if (at >= 0) {
    const end = text.search(/[\s"'<>]/);
    if (end >= 0) text = text.slice(0, end);
  }
  return text.trim();
}

const LINK = /^(wss?):\/\/(?:[^@/?#]*@)?(\[[^\]]*\]|[^:/?#]*)(?::(\d*))?/i;

/** RemoteComputeConfig.isCustomWebSocketUrl: ws/wss with a host. */
export function validLink(value: string): boolean {
  const host = LINK.exec(normalizeLink(value))?.[2] ?? "";
  return host.replace(/^\[|\]$/g, "").trim() !== "";
}

/** RemoteComputeConfig.displayNameForCustomWebSocketUrl: 自建算力 · host[:port], never the path. */
export function customDisplayName(url: string): string {
  const custom = t("RemoteCompute.custom");
  const match = LINK.exec(normalizeLink(url));
  if (!match || !match[2].replace(/^\[|\]$/g, "").trim()) return custom;
  return `${custom} · ${match[2]}${match[3] && Number(match[3]) > 0 ? `:${Number(match[3])}` : ""}`;
}

/** RemoteComputeConfig.friendlyZhiziErrorMessage for a sanitized failure key. */
export function failureText(f: CloudFailure, args: string, sendCode = false): string {
  return withVipHint(keyText(f.key, sendCode), args);
}

/** Why the 智子 engine did not start, with the plan hint like any 智子 failure. Without `args` (a
 *  failure dialog that does not know the plan) there is no hint. */
export function cloudStartText(f: CloudStartFailure, args: string): string {
  switch (f.type) {
    case "Token": return withVipHint(keyText(f.key), args);
    case "Unreachable": return t("RemoteCompute.error.network");
    case "Refused": return withVipHint(f.detail ? product("cloudStartRefusedDetail", f.detail) : product("cloudStartRefused"), args);
    case "Disconnected": return product("cloudStartDisconnected");
    case "NotReady": return product("cloudStartNotReady");
  }
}

function keyText(key: string, sendCode = false): string {
  let text: string;
  switch (key) {
    case "invalid_phone": case "invalid_email": text = t("RemoteCompute.error.invalidAccount"); break;
    case "invalid_account": text = t("RemoteCompute.error.accountRequired"); break;
    case "password_required": text = t("RemoteCompute.error.accountPasswordRequired"); break;
    case "verification_code_required": text = t("RemoteCompute.error.accountCodeRequired"); break;
    case "login_required": text = t("RemoteCompute.error.loginFirst"); break;
    case "invalid_password": case "invalid_credentials": text = t("RemoteCompute.error.invalidCredentials"); break;
    case "invalid_verification_code": text = t("RemoteCompute.error.invalidVerificationCode"); break;
    case "fast_login_too_frequent": text = t("RemoteCompute.error.codeTooFrequent"); break;
    case "password_too_short": text = t("RemoteCompute.error.passwordTooShort"); break;
    case "duplicate_phone": case "duplicate_email": text = t("RemoteCompute.error.accountAlreadyExists"); break;
    case "not_found": text = t("RemoteCompute.error.accountNotFound"); break;
    case "unauthorized": text = t("RemoteCompute.error.sessionExpired"); break;
    case "send_code_error": text = t("RemoteCompute.error.codeSendTemporary"); break;
    case "failed_to_insert_socket_io_token": text = t("RemoteCompute.error.workerUnavailable"); break;
    case "failed_aggerate": case "count_usage_error": case "list_usages_error": case "count_credits_error": case "list_credits_error":
      text = t("RemoteCompute.error.billingUnavailable"); break;
    case "invalid_pay_type": case "invalid_amount": case "invalid_top_up_amount": case "invalid_product": case "missing_product_name":
    case "invalid_product_name": case "failed_create_prepay_id": case "failed_insert_order": case "order_not_found":
      text = t("RemoteCompute.error.paymentUnavailable"); break;
    case "network_error": text = t("RemoteCompute.error.network"); break;
    case "invalid_response": text = t("RemoteCompute.error.invalidResponse"); break;
    default: text = sendCode ? t("RemoteCompute.error.codeSendTemporary") : t("RemoteCompute.error.genericConnection");
  }
  return text;
}

/** VIP 包月 access problems get the plan hint (RemoteComputeConfig.looksLikeVipAccessProblem). */
function withVipHint(text: string, args: string): string {
  const lower = text.toLowerCase();
  const vipProblem = ["worker", "vip", "quota", "permission", "forbidden", "unauthorized", "403"].some((w) => lower.includes(w))
    || ["无权限", "未开通", "额度", "余额"].some((w) => text.includes(w));
  if (optionValue(args, "--gpu-type", "vip-share") === "vip-share" && args && vipProblem) return `${text}\n\n${t("RemoteCompute.error.vipHint")}`;
  return text;
}

/** Java money(): ¥ and two decimals, HALF_UP, from an exact decimal string. */
export function money(value: string): string {
  const match = /^(-?)(\d+)(?:\.(\d+))?$/.exec(value.trim());
  if (!match) return "¥0.00";
  const [, sign, whole, fraction = ""] = match;
  let cents = BigInt(whole + (fraction + "00").slice(0, 2));
  if (Number(fraction[2] ?? "0") >= 5) cents += 1n;
  const text = cents.toString().padStart(3, "0");
  return `¥${sign && cents > 0n ? "-" : ""}${text.slice(0, -2)}.${text.slice(-2)}`;
}

export function fen(amount: number): string {
  const cents = Math.max(0, Math.trunc(amount));
  return `¥${Math.floor(cents / 100)}.${String(cents % 100).padStart(2, "0")}`;
}

export function duration(seconds: number): string {
  const safe = Math.max(0, seconds);
  const hours = Math.floor(safe / 3600), minutes = Math.floor((safe % 3600) / 60);
  return hours > 0 ? format(t("RemoteCompute.account.hoursMinutes"), hours, minutes) : format(t("RemoteCompute.account.minutes"), Math.max(1, minutes));
}

const two = (n: number) => String(n).padStart(2, "0");

/** yyyy-MM-dd, yyyy-MM-dd HH:mm or HH:mm in the system time zone, like Java's formatters. */
export function localTime(value: string | number | null | undefined, pattern: "date" | "dateTime" | "time"): string {
  if (value === null || value === undefined || value === "") return "-";
  const d = new Date(value);
  if (Number.isNaN(d.getTime())) return "-";
  const date = `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`, time = `${two(d.getHours())}:${two(d.getMinutes())}`;
  return pattern === "date" ? date : pattern === "time" ? time : `${date} ${time}`;
}

export function months(count: number): string {
  return format(t("RemoteCompute.payment.months"), count);
}
