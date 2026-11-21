// 独立棋局控制面板 window (Controller.java): eight large buttons in a 4 x 2 grid filling the window.
// Each press goes to the main window, which owns the record and runs the command. The browsing
// buttons show the shared step icons, keep Java's arrow text as their unseen text and are named
// (accessible name and tooltip) by what they do; 返回主分支 and 涂鸦 show their icon above their name.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { icon, type IconName } from "../design/icons";
import { product, registerProduct, setLanguage, t } from "../i18n";
import { CONTROL_DATA_EVENT, CONTROL_INPUT_EVENT, type ControlButton, type ControlInput, type ControlPayload } from "./model";
import "../design/window";
import "./control.css";

registerProduct({
  ZhCn: { controlFirstMove: "跳到第一手", controlLastMove: "跳到最后一手", controlBackFive: "后退 5 手", controlForwardFive: "前进 5 手" },
  ZhTw: { controlFirstMove: "跳到第一手", controlLastMove: "跳到最後一手", controlBackFive: "後退 5 手", controlForwardFive: "前進 5 手" },
  EnUs: { controlFirstMove: "Go to first move", controlLastMove: "Go to last move", controlBackFive: "Back 5 moves", controlForwardFive: "Forward 5 moves" },
  JaJp: { controlFirstMove: "初手へ移動", controlLastMove: "最終手へ移動", controlBackFive: "5手戻る", controlForwardFive: "5手進む" },
  KoKr: { controlFirstMove: "첫 수로 이동", controlLastMove: "마지막 수로 이동", controlBackFive: "5수 뒤로", controlForwardFive: "5수 앞으로" },
  ThTh: { controlFirstMove: "ไปตาแรก", controlLastMove: "ไปตาสุดท้าย", controlBackFive: "ย้อนกลับ 5 ตา", controlForwardFive: "ไปข้างหน้า 5 ตา" },
});

/** Java's buttons and texts in GridLayout order, with their icon, and for an icon-only button its
 *  name (null: the drawn text names it). Texts are read at each render, after the language is set;
 *  literal t() calls keep the keys in the Java string extraction. */
const BUTTONS: [ControlButton, IconName, () => string, (() => string) | null][] = [
  ["first", "step-first", () => "|<", () => product("controlFirstMove")],
  ["last", "step-last", () => ">|", () => product("controlLastMove")],
  ["back5", "step-back-ten", () => "<<", () => product("controlBackFive")],
  ["forward5", "step-forward-ten", () => ">>", () => product("controlForwardFive")],
  ["back1", "step-back", () => "<", () => t("Accessibility.previousMove")],
  ["forward1", "step-forward", () => ">", () => t("Accessibility.nextMove")],
  ["backToMain", "back-to-main", () => t("Controller.btnBackToMain"), null],
  ["painting", "paint-brush", () => t("Controller.btnDrawPainting"), null],
];

export function bootControlPanel(): void {
  const send = (input: ControlInput) => void emitTo("main", CONTROL_INPUT_EVENT, input);
  const grid = document.createElement("div");
  grid.id = "cp-grid";
  const buttons = BUTTONS.map(([key, glyph, text, name]) => {
    const b = document.createElement("button");
    b.dataset.key = `control.${key}`;
    const label = document.createElement("span");
    label.className = name ? "cp-unseen" : "cp-label";
    b.append(icon(glyph), label);
    b.onclick = () => send({ type: "button", button: key });
    grid.append(b);
    return { b, label, text, name };
  });
  document.body.append(grid);

  let lastReport = "";
  const report = () => {
    const report = {
      width: window.innerWidth,
      height: window.innerHeight,
      buttons: buttons.map(({ b }) => {
        const r = b.getBoundingClientRect();
        return {
          key: b.dataset.key!,
          text: b.textContent ?? "",
          fontSize: Math.round(parseFloat(getComputedStyle(b).fontSize)),
          x: Math.round(r.left + r.width / 2),
          y: Math.round(r.top + r.height / 2),
          width: Math.round(r.width),
          height: Math.round(r.height),
        };
      }),
    };
    const text = JSON.stringify(report);
    if (text === lastReport) return;
    lastReport = text;
    send({ type: "report", report });
  };
  new ResizeObserver(report).observe(grid);

  const render = (p: ControlPayload) => {
    setLanguage(p.language);
    for (const { b, label, text, name } of buttons) {
      label.textContent = text();
      if (name) { b.setAttribute("aria-label", name()); b.title = name(); }
    }
    report();
  };

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  const self = getCurrentWebviewWindow();
  void self.listen<ControlPayload>(CONTROL_DATA_EVENT, (e) => render(e.payload)).then(() => send({ type: "ready" }));
}
