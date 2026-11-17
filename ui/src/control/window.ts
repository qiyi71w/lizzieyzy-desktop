// 独立棋局控制面板 window (Controller.java): eight buttons in a 4 x 2 grid with 5 px gaps and a 5 px
// edge. Each press goes to the main window, which owns the record and runs the command. Like
// setBtnFont, every resize sets each button's font to fit its text: the smaller of 90% of the
// width per character and half the height.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { UI_FONT } from "../hawkeye/model";
import { isChinese, setLanguage, t } from "../i18n";
import { CONTROL_DATA_EVENT, CONTROL_INPUT_EVENT, type ControlButton, type ControlInput, type ControlPayload } from "./model";
import "./control.css";

/** Java's buttons and texts in GridLayout order. Literal t() calls keep the keys in the Java string
 *  extraction. */
const BUTTONS: [ControlButton, () => string][] = [
  ["first", () => "|<"],
  ["last", () => ">|"],
  ["back5", () => "<<"],
  ["forward5", () => ">>"],
  ["back1", () => "<"],
  ["forward1", () => ">"],
  ["backToMain", () => t("Controller.btnBackToMain")],
  ["painting", () => t("Controller.btnDrawPainting")],
];

export function bootControlPanel(): void {
  document.body.style.fontFamily = UI_FONT;
  const send = (input: ControlInput) => void emitTo("main", CONTROL_INPUT_EVENT, input);
  const grid = document.createElement("div");
  grid.id = "cp-grid";
  const buttons = BUTTONS.map(([key, text]) => {
    const b = document.createElement("button");
    b.dataset.key = `control.${key}`;
    b.onclick = () => send({ type: "button", button: key });
    grid.append(b);
    return { b, text };
  });
  document.body.append(grid);

  let lastReport = "";
  const fit = () => {
    for (const { b } of buttons) {
      const chars = Math.max(1, b.textContent?.length ?? 1);
      b.style.fontSize = `${Math.max(1, Math.min((b.clientWidth * 0.9) / chars, b.clientHeight * 0.5))}px`;
    }
    const report = {
      width: window.innerWidth,
      height: window.innerHeight,
      buttons: buttons.map(({ b }) => {
        const r = b.getBoundingClientRect();
        return {
          key: b.dataset.key!,
          text: b.textContent ?? "",
          fontSize: Math.round(parseFloat(b.style.fontSize)),
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
  new ResizeObserver(fit).observe(grid);

  const render = (p: ControlPayload) => {
    setLanguage(p.language);
    document.documentElement.lang = isChinese() ? "zh" : "en";
    for (const { b, text } of buttons) b.textContent = text();
    fit();
  };

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  const self = getCurrentWebviewWindow();
  void self.listen<ControlPayload>(CONTROL_DATA_EVENT, (e) => render(e.payload)).then(() => send({ type: "ready" }));
}
