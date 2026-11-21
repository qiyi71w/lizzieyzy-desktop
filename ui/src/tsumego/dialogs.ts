// 死活题 (TsumeGoFrame.java), 抓取死活题's instructions (CaptureTsumeGoFrame.java) and its 参数设置
// (CaptureTsumeGoSettings.java), and LizzieFrame.startCaptureTsumeGo. Rust owns the saved choices,
// the frame and the captured problem; these windows hold only their drafts.
import { invoke } from "@tauri-apps/api/core";
import { run, state } from "../app";
import { errorText } from "../errorText";
import type { CaptureSettings, TsumegoAttacker, TsumegoFrameSettings, TsumegoKoThreat, TsumegoToPlay } from "../contract/generated";
import { closeBox, formPanel, message } from "../dialogs";
import { parseJavaInt } from "../display/dialogs";
import { choosePlacement } from "../editing/commands";
import { edit } from "../editing/state";
import { button, footer, primaryButton, row, segmented } from "../engine/controls";
import { plainJava } from "../engine/tuningText";
import { product, registerProduct, t } from "../i18n";
import { messageNoModal } from "../match/dialogs";
import "./tsumego.css";

registerProduct({
  ZhCn: { tsumegoCaptureStartFailed: "未能开始抓取死活题。" },
  ZhTw: { tsumegoCaptureStartFailed: "未能開始抓取死活題。" },
  EnUs: { tsumegoCaptureStartFailed: "Could not start capturing a tsumego." },
  JaJp: { tsumegoCaptureStartFailed: "詰碁のキャプチャーを開始できませんでした。" },
  KoKr: { tsumegoCaptureStartFailed: "사활 화면 캡처를 시작하지 못했습니다." },
  ThTh: { tsumegoCaptureStartFailed: "เริ่มจับสึเมโกะไม่ได้" },
});

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

/** IntDocument: an insertion is taken only when the whole text then parses as an int
 *  (Integer.parseInt); removals are always taken. Typing over a selection removes it first. */
function intDocument(field: HTMLInputElement): void {
  let kept = field.value;
  field.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert") || e.inputType === "insertCompositionText") return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const start = field.selectionStart ?? field.value.length;
    const end = field.selectionEnd ?? start;
    const next = field.value.slice(0, start) + inserted + field.value.slice(end);
    if (parseJavaInt(next) !== null) return;
    e.preventDefault();
    if (end > start) {
      field.setRangeText("", start, end, "end");
      kept = field.value;
    }
  });
  // The input method's text cannot be refused before it lands: take it back afterwards.
  field.addEventListener("input", (e) => {
    const insert = (e as InputEvent).inputType?.startsWith("insert") ?? false;
    if (insert && parseJavaInt(field.value) === null) field.value = kept;
    else kept = field.value;
  });
}

function intField(value: number, key: string): HTMLInputElement {
  const field = el("input");
  field.type = "text";
  field.inputMode = "numeric";
  field.value = String(value);
  field.dataset.key = key;
  intDocument(field);
  return field;
}

/** A Swing button that is not focusable (setFocusable(false)): a press keeps the focus where it is. */
function unfocusable(b: HTMLButtonElement): HTMLButtonElement {
  b.tabIndex = -1;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  return b;
}

/** A window of its own (JDialog / JFrame) inside the workbench: non-modal, above the workbench and
 *  below modal dialogs, the dialog frame with the close box in its title; Escape in the window is
 *  the close box. `body` follows the title and scrolls under it; its footer is the window's button
 *  row. */
function windowBox(title: string, cls: string, close: () => void): { box: HTMLDivElement; body: HTMLDivElement } {
  const box = el("div", `modal-box engine-dialog tsumego-window ${cls}`);
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-label", title);
  // Usable while WaitForAnalysis disables the main window, like the Java window.
  box.dataset.freeWindow = "";
  const head = el("div", "modal-title tsumego-title", title);
  const x = unfocusable(closeBox(close));
  x.dataset.key = `${cls}.close`;
  head.append(x);
  const body = el("div", "tsumego-body");
  box.append(head, body);
  box.addEventListener("keydown", (e) => {
    if (e.key !== "Escape" || e.isComposing) return;
    e.preventDefault();
    e.stopPropagation();
    close();
  });
  return { box, body };
}

// ------------------------------------------------------------------ 死活题

let tsumegoWindow: HTMLElement | null = null;

function closeTsumego(): void {
  tsumegoWindow?.remove();
  tsumegoWindow = null;
}

/** A ButtonGroup of non-focusable radio buttons as one segmented control; `name` prefixes each
 *  choice's automation key, which its segment carries. */
function radios<T extends string>(name: string, value: T, choices: [T, string][]): { el: HTMLElement; get: () => T } {
  const seg = segmented(name, choices, value);
  seg.inputs.forEach((radio, k) => {
    radio.tabIndex = -1;
    radio.parentElement!.dataset.key = `${name}.${choices[k][0]}`;
  });
  return { el: seg.el, get: seg.value };
}

/** LizzieFrame.openTsumego: an open 死活题 window is replaced by a new one with the saved choices. */
export function openTsumego(): void {
  closeTsumego();
  const saved = state.view?.settings.tsumego.frame;
  if (!saved) return;
  const { box, body } = windowBox(t("TsumeGoFrame.title"), "tsumego-frame", closeTsumego);
  box.dataset.name = "tsumego.frame";
  // 先手方, 进攻方, 添加劫材 and 墙面间距 as one group of rows.
  const toPlay = radios<TsumegoToPlay>("tsumego.toPlay", saved.toPlay, [
    ["Keep", t("TsumeGoFrame.rdoKeepOn")],
    ["Black", t("TsumeGoFrame.rdoBlackToPlay")],
    ["White", t("TsumeGoFrame.rdoWhiteToPlay")],
  ]);
  const attacker = radios<TsumegoAttacker>("tsumego.attacker", saved.attacker, [
    ["Auto", t("TsumeGoFrame.rdoAutoDetect")],
    ["Black", t("TsumeGoFrame.rdoBlackAttack")],
    ["White", t("TsumeGoFrame.rdoWhiteAttack")],
  ]);
  const koThreat = radios<TsumegoKoThreat>("tsumego.ko", saved.koThreat, [
    ["Both", t("TsumeGoFrame.rdoKoBoth")],
    ["Attacker", t("TsumeGoFrame.rdoKoAttacker")],
    ["Defender", t("TsumeGoFrame.rdoKoDefender")],
    ["None", t("TsumeGoFrame.rdoKoNone")],
  ]);
  const wall = intField(saved.wallDistance, "tsumego.wall");
  row(body, t("TsumeGoFrame.lblToPlay"), "", toPlay.el);
  row(body, t("TsumeGoFrame.lblAttacker"), "", attacker.el);
  row(body, t("TsumeGoFrame.lblKoThreat"), "", koThreat.el);
  row(body, t("TsumeGoFrame.lblWallDistance"), "", wall);

  const capture = unfocusable(button(t("TsumeGoFrame.btnCapture"), () => {
    openCaptureFrame();
    closeTsumego();
  }));
  capture.dataset.key = "tsumego.capture";
  const confirm = primaryButton(t("TsumeGoFrame.btnConfirm"), () => {
    const settings: TsumegoFrameSettings = {
      toPlay: toPlay.get(),
      attacker: attacker.get(),
      koThreat: koThreat.get(),
      // Integer.parseInt failing keeps the saved distance, as does a value not above 0 (core).
      wallDistance: parseJavaInt(wall.value) ?? 0,
    };
    closeTsumego();
    void frame(settings);
  });
  confirm.dataset.key = "tsumego.confirm";
  const cancel = button(t("TsumeGoFrame.btnCancel"), closeTsumego);
  cancel.dataset.key = "tsumego.cancel";
  // 屏幕抓取 ends this window by opening 抓取死活题.
  footer(body, { primary: confirm, others: [capture], cancel });

  // The window takes its own keys, like a focused Swing dialog; Shift+T starts the capture.
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.code === "KeyT" && e.shiftKey && e.target !== wall) {
      e.preventDefault();
      closeTsumego();
      void startCapture();
    }
  });
  document.body.append(box);
  tsumegoWindow = box;
  confirm.focus();
}

/** 确定: build the frame, end insert mode (Menu.clearInsert), then one message per side whose ko
 *  threats found no room (Tsumego.buildCoverWall). */
async function frame(settings: TsumegoFrameSettings): Promise<void> {
  const result = await edit({ op: "FrameTsumego", settings });
  if (!result || result.type === "Refused") return;
  choosePlacement("alternate", false);
  if (result.type !== "TsumegoFramed") return;
  for (const color of result.no_room) {
    const side = color === "Black" ? t("Tsumego.noRoomForKoThreat.black") : t("Tsumego.noRoomForKoThreat.white");
    await message(t("Message.title"), `${t("Tsumego.noRoomForKoThreat1")} [${side}] ${t("Tsumego.noRoomForKoThreat2")}`);
  }
}

// ------------------------------------------------------------------ 抓取死活题

let captureWindow: HTMLElement | null = null;

function closeCaptureFrame(): void {
  captureWindow?.remove();
  captureWindow = null;
}

/** LizzieFrame.openCaptureTsumego: the instructions window at (20, 20), always above the
 *  workbench; an open one is replaced. */
export function openCaptureFrame(): void {
  closeCaptureFrame();
  const { box, body } = windowBox(t("CaptureTsumeGoFrame.title"), "capture-frame", closeCaptureFrame);
  box.dataset.key = "capture.frame";
  box.tabIndex = -1;
  const grid = el("div", "capture-grid");
  const steps = [t("CaptureTsumeGoFrame.lblStep1"), t("CaptureTsumeGoFrame.lblStep2"), t("CaptureTsumeGoFrame.lblStep3")];
  steps.forEach((step, i) => {
    // Swing HTML labels: their line breaks and spaces as text.
    const text = el("div", "capture-step", plainJava(step));
    const picture = el("img");
    picture.src = `/assets/capture${i + 1}.png`;
    picture.alt = "";
    grid.append(text, picture);
  });
  const settings = unfocusable(button(t("CaptureTsumeGoFrame.btnSettings"), () => void openCaptureSettings()));
  settings.dataset.key = "capture.settings";
  const start = unfocusable(primaryButton(t("CaptureTsumeGoFrame.btnStart"), () => void startCapture()));
  start.dataset.key = "capture.start";
  body.append(grid);
  // 参数设置 and the tip leave the window open; 开始抓取 hides it.
  footer(body, { start: [settings, el("span", "engine-hint", t("CaptureTsumeGoFrame.lblTip"))], primary: start });
  box.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.code === "KeyT" && e.shiftKey) {
      e.preventDefault();
      void startCapture();
    }
  });
  document.body.append(box);
  captureWindow = box;
  box.focus();
}

/** A Java label as a row name: without its trailing colon. */
const named = (text: string) => text.replace(/\s*[:：]\s*$/, "");

/** 参数设置 over the instructions window: five IntDocument fields as one group of rows; 确定 saves
 *  them all only when each parses (else Menu.inputIntegerHint) and closes either way. */
async function openCaptureSettings(): Promise<void> {
  const saved = state.view?.settings.tsumego.capture;
  if (!saved) return;
  const body = el("div", "capture-settings");
  const fields = {
    blackOffset: intField(saved.blackOffset, "capture.blackOffset"),
    whiteOffset: intField(saved.whiteOffset, "capture.whiteOffset"),
    blackPercent: intField(saved.blackPercent, "capture.blackPercent"),
    whitePercent: intField(saved.whitePercent, "capture.whitePercent"),
    grayOffset: intField(saved.grayOffset, "capture.grayOffset"),
  };
  row(body, named(t("CaptureTsumeGoSettings.lblBlackOffset")), "", fields.blackOffset);
  row(body, named(t("CaptureTsumeGoSettings.lblWhiteOffset")), "", fields.whiteOffset);
  row(body, named(t("CaptureTsumeGoSettings.lblBlackPercent")), "", fields.blackPercent);
  row(body, named(t("CaptureTsumeGoSettings.lblWhitePercent")), "", fields.whitePercent);
  row(body, named(t("CaptureTsumeGoSettings.lblGrayOffset")), "", fields.grayOffset);
  const dialog = formPanel(t("CaptureTsumeGoSettings.title"), body);
  const confirm = primaryButton(t("CaptureTsumeGoSettings.btnConfirm"), async () => {
    const values = Object.fromEntries(Object.entries(fields).map(([k, f]) => [k, parseJavaInt(f.value)]));
    if (Object.values(values).some((v) => v === null)) await message(t("Message.title"), t("Menu.inputIntegerHint"));
    else void run({ type: "SetCaptureSettings", settings: values as CaptureSettings });
    await dialog.close();
  });
  confirm.dataset.key = "capture.settingsConfirm";
  const cancel = button(t("CaptureTsumeGoSettings.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "capture.settingsCancel";
  footer(body, { primary: confirm, cancel });
  await dialog.closed;
}

const MAC_PERMISSION_HINT = "macOS 需要授予\"屏幕录制\"权限才能使用抓死活棋功能。\n请在 系统设置 → 隐私与安全性 → 屏幕录制 中允许本应用。";
let macHintShown = false;

/** LizzieFrame.startCaptureTsumeGo: the instructions window hides and the full-screen overlay opens
 *  over a picture of the screen (Rust minimizes the main window first and restores it after). */
export async function startCapture(): Promise<void> {
  closeCaptureFrame();
  if (document.documentElement.dataset.os === "mac" && !macHintShown) {
    macHintShown = true;
    messageNoModal(MAC_PERMISSION_HINT);
  }
  try {
    await invoke("start_tsumego_capture");
  } catch (e) {
    await message(t("Message.title"), `${product("tsumegoCaptureStartFailed")}\n${errorText(e)}`);
  }
}
