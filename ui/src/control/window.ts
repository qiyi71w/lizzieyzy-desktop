// 独立棋局控制面板 window (Controller.java): eight large buttons in a 4 x 2 grid filling the window.
// Each press goes to the main window, which owns the record and runs the command. The browsing
// buttons show the shared step icons and keep Java's arrow text as their unseen text; 返回主分支
// and 涂鸦 show their icon above their name.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { icon, type IconName } from "../design/icons";
import { setLanguage, t } from "../i18n";
import { CONTROL_DATA_EVENT, CONTROL_INPUT_EVENT, type ControlButton, type ControlInput, type ControlPayload } from "./model";
import "../design/window";
import "./control.css";

/** Java's buttons and texts in GridLayout order, with their icon and whether the text is drawn.
 *  Literal t() calls keep the keys in the Java string extraction. */
const BUTTONS: [ControlButton, IconName, () => string, boolean][] = [
  ["first", "step-first", () => "|<", false],
  ["last", "step-last", () => ">|", false],
  ["back5", "step-back-ten", () => "<<", false],
  ["forward5", "step-forward-ten", () => ">>", false],
  ["back1", "step-back", () => "<", false],
  ["forward1", "step-forward", () => ">", false],
  ["backToMain", "back-to-main", () => t("Controller.btnBackToMain"), true],
  ["painting", "paint-brush", () => t("Controller.btnDrawPainting"), true],
];

export function bootControlPanel(): void {
  const send = (input: ControlInput) => void emitTo("main", CONTROL_INPUT_EVENT, input);
  const grid = document.createElement("div");
  grid.id = "cp-grid";
  const buttons = BUTTONS.map(([key, glyph, text, shown]) => {
    const b = document.createElement("button");
    b.dataset.key = `control.${key}`;
    const label = document.createElement("span");
    label.className = shown ? "cp-label" : "cp-unseen";
    b.append(icon(glyph), label);
    b.onclick = () => send({ type: "button", button: key });
    grid.append(b);
    return { b, label, text };
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
    for (const { label, text } of buttons) label.textContent = text();
    report();
  };

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  const self = getCurrentWebviewWindow();
  void self.listen<ControlPayload>(CONTROL_DATA_EVENT, (e) => render(e.payload)).then(() => send({ type: "ready" }));
}
