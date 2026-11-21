// 批量分析进度表 window (AnalysisTable.java): five buttons in the window's button bar over a
// six-column table whose first row is the record being analysed. It draws what the main window
// relays and reports every click; the main window decides the command.
import "../design/window";
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { BatchMove } from "../contract/generated";
import { product, registerProduct, setLanguage, t } from "../i18n";
import { BATCH_DATA_EVENT, BATCH_INPUT_EVENT, type BatchButton, type BatchInput, type BatchPayload, type BatchReport } from "./model";
import "./batch.css";

registerProduct({
  ZhCn: { batchTableEmpty: "队列里没有棋谱。点“{0}”加入要分析的棋谱。" },
  ZhTw: { batchTableEmpty: "佇列裡沒有棋譜。點「{0}」加入要分析的棋譜。" },
  EnUs: { batchTableEmpty: "No records are queued. Use “{0}” to add records to analyse." },
  JaJp: { batchTableEmpty: "キューに棋譜がありません。「{0}」で分析する棋譜を追加します。" },
  KoKr: { batchTableEmpty: "대기 중인 기보가 없습니다. '{0}' 버튼으로 분석할 기보를 추가하세요." },
  ThTh: { batchTableEmpty: "ไม่มีไฟล์ในคิว ใช้ “{0}” เพื่อเพิ่มไฟล์ที่จะวิเคราะห์" },
});

/** The top buttons: key, text and look (开始/结束 is the window's main action, 清空 empties the
 *  queue). Literal t() calls keep the keys in the Java string extraction. */
const BUTTONS: [BatchButton, () => string, "" | "primary" | "danger"][] = [
  ["stopGo", () => t("AnalysisTable.stopGo"), ""],
  ["stopStart", () => t("AnalysisTable.stopStart"), "primary"],
  ["stopStartAnalysisMode", () => t("AnalysisTable.stopStartAnalysisMode"), ""],
  ["addFile", () => t("AnalysisTable.addFile"), ""],
  ["clearAllFiles", () => t("AnalysisTable.clearAllFiles"), "danger"],
];

/** The row buttons' columns: text (also the header), action and the rows (index, count) where
 *  Java's renderer disables them (MyButtonFirst / Up / Down / Delete; 下移 stops at the last row). */
const ROW_ACTIONS: [() => string, BatchMove | "Remove", (row: number, count: number) => boolean][] = [
  [() => t("AnalysisTable.prior"), "Prior", (row) => row > 1],
  [() => t("AnalysisTable.up"), "Up", (row) => row > 1],
  [() => t("AnalysisTable.down"), "Down", (row, count) => row > 0 && row < count - 1],
  [() => t("AnalysisTable.delete"), "Remove", (row) => row > 0],
];

export function bootBatchTable(): void {
  const send = (input: BatchInput) => void emitTo("main", BATCH_INPUT_EVENT, input);

  const bar = document.createElement("div");
  bar.id = "bt-buttons";
  const buttons = new Map<BatchButton, HTMLButtonElement>();
  for (const [key, , look] of BUTTONS) {
    const b = document.createElement("button");
    b.dataset.key = `batchTable.${key}`;
    if (look) b.className = look;
    b.onclick = () => send({ type: "button", button: key });
    buttons.set(key, b);
    bar.append(b);
  }
  const scroll = document.createElement("div");
  scroll.id = "bt-scroll";
  const table = document.createElement("table");
  const columns = document.createElement("colgroup");
  // The number, the file name taking the rest, then the four row actions sized to their text.
  for (const cls of ["bt-col-number", "bt-col-name", "bt-col-action", "bt-col-action", "bt-col-action", "bt-col-action"]) {
    const col = document.createElement("col");
    col.className = cls;
    columns.append(col);
  }
  const head = document.createElement("thead");
  const body = document.createElement("tbody");
  table.append(columns, head, body);
  scroll.append(table);
  // 增加棋谱 is the way to fill an empty queue; the hint says so under the header.
  const empty = document.createElement("div");
  empty.className = "bt-empty";
  scroll.append(empty);
  document.body.append(bar, scroll);

  let lastReport = "";
  const report = (rows: string[][]) => {
    const targets = [...document.querySelectorAll<HTMLElement>("[data-key]")].map((el) => {
      const r = el.getBoundingClientRect();
      return {
        key: el.dataset.key!,
        text: el.textContent ?? "",
        enabled: !(el instanceof HTMLButtonElement && el.disabled),
        x: Math.round(r.left + r.width / 2),
        y: Math.round(r.top + r.height / 2),
      };
    });
    const r: BatchReport = { rows, targets };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    send({ type: "report", report: r });
  };

  const render = (p: BatchPayload) => {
    setLanguage(p.language);
    // resetAnalysisMode: 暂停/继续 and 开始/结束 wait while a 批量精析 runs.
    const flash = p.running?.mode === "Flash";
    for (const [key, text] of BUTTONS) {
      const b = buttons.get(key)!;
      b.textContent = text();
      b.disabled = flash && (key === "stopGo" || key === "stopStart");
    }
    const header = document.createElement("tr");
    for (const text of [t("AnalysisTable.column1"), t("AnalysisTable.column2"), ...ROW_ACTIONS.map(([label]) => label())]) {
      const th = document.createElement("th");
      th.textContent = text;
      header.append(th);
    }
    head.replaceChildren(header);
    const rows: string[][] = [];
    body.replaceChildren(
      ...p.items.map((item, row) => {
        const tr = document.createElement("tr");
        const number = document.createElement("td");
        number.textContent = row === 0 ? t("AnalysisTable.current") : String(row + 1);
        const name = document.createElement("td");
        name.textContent = item.name;
        // The record is analysed in the background, not on the main board: its progress shows here.
        const r = p.running;
        if (row === 0 && r && r.item === item.id && !r.loading) {
          const progress = document.createElement("span");
          progress.className = "bt-progress";
          progress.textContent = `${r.done}/${r.total}`;
          name.append(progress);
        }
        tr.append(number, name);
        for (const [label, action, enabled] of ROW_ACTIONS) {
          const td = document.createElement("td");
          td.className = action === "Remove" ? "bt-action bt-remove" : "bt-action";
          const b = document.createElement("button");
          b.textContent = label();
          b.dataset.key = `batchTable.${action}.${row}`;
          b.disabled = !enabled(row, p.items.length);
          b.onclick = () => send({ type: "row", item: item.id, action });
          td.append(b);
          tr.append(td);
        }
        rows.push([number.textContent ?? "", name.textContent ?? ""]);
        return tr;
      }),
    );
    empty.hidden = p.items.length > 0;
    empty.textContent = product("batchTableEmpty", t("AnalysisTable.addFile"));
    report(rows);
  };

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  const self = getCurrentWebviewWindow();
  void self.listen<BatchPayload>(BATCH_DATA_EVENT, (e) => render(e.payload)).then(() => send({ type: "ready" }));
}
