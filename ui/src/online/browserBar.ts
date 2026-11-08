// The 弈客 web window's toolbar (BrowserFrame.java:286-445): 后退, the address field, 加载 and 停止同步
// above the remote 弈客 page. The page lives in its own webview; this strip only reports the
// requests to the main window, which loads the page and starts or stops the follow.
import { emitTo, listen } from "@tauri-apps/api/event";
import { UI_FONT } from "../hawkeye/model";
import { isChinese, setLanguage, t } from "../i18n";
import { BAR_ADDRESS_EVENT, BAR_DATA_EVENT, BAR_INPUT_EVENT, type BarInput, type BarPayload } from "./model";
import "./bar.css";

export function bootYikeBar(): void {
  document.body.style.fontFamily = UI_FONT;
  const send = (input: BarInput) => void emitTo("main", BAR_INPUT_EVENT, input);

  const back = document.createElement("button");
  back.type = "button";
  back.id = "yb-back";
  back.dataset.key = "yikeBrowser.back";
  const icon = document.createElement("img");
  icon.src = "/assets/left.png";
  icon.alt = "";
  back.append(icon);
  // setFocusable(false): the address field keeps the focus.
  back.tabIndex = -1;
  back.onmousedown = (e) => e.preventDefault();
  back.onclick = () => send({ type: "back" });

  const address = document.createElement("input");
  address.type = "text";
  address.id = "yb-address";
  address.dataset.key = "yikeBrowser.address";
  address.spellcheck = false;
  // The page's start address until the first address change arrives.
  address.value = new URLSearchParams(location.search).get("url") ?? "";

  // makeLabelButton: self-drawn label buttons, outside the focus cycle.
  const labelButton = (key: string) => {
    const b = document.createElement("span");
    b.className = "yb-label";
    b.dataset.key = `yikeBrowser.${key}`;
    b.setAttribute("role", "button");
    return b;
  };
  const load = labelButton("load");
  const stop = labelButton("stop");
  // 加载: startYikeSync(addr, true) then loadURL(addr).
  load.onclick = () => send({ type: "load", url: address.value, enter: false });
  stop.onclick = () => send({ type: "stop" });
  // Enter: loadURL(addr) then startYikeSync(addr, true). Java's action listener loads the address
  // a second time on the same Enter; one load is enough.
  address.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      send({ type: "load", url: address.value, enter: true });
    }
  });
  address.addEventListener("input", () => send({ type: "report", address: address.value }));

  const labels = () => {
    document.documentElement.lang = isChinese() ? "zh" : "en";
    load.textContent = t("LizzieFrame.onLoad");
    stop.textContent = t("LizzieFrame.stopSync");
  };
  labels();
  document.body.append(back, address, load, stop);
  document.addEventListener("contextmenu", (e) => {
    if (e.target !== address) e.preventDefault();
  });

  void listen<BarPayload>(BAR_DATA_EVENT, (e) => {
    setLanguage(e.payload.language);
    labels();
  });
  // onAddressChange: the field shows the page's address, hash routes included.
  void listen<{ url: string }>(BAR_ADDRESS_EVENT, (e) => {
    address.value = e.payload.url;
    send({ type: "report", address: address.value });
  }).then(() => send({ type: "ready" }));
}
