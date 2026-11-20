// 编辑棋局信息 (GameInfoDialog.java) and 设置棋盘大小 (SetBoardSize.java). Both send one Edit command
// and close; the record itself changes only in Rust.
import { state } from "../app";
import { icon } from "../design/icons";
import { formPanel } from "../dialogs";
import { actions, button, primaryButton, row } from "../engine/controls";
import { product, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import { edit } from "./state";
import "./chrome.css";

/** java.math.BigDecimal(String) syntax, used by KomiDocument to accept an insertion. */
const BIG_DECIMAL = /^[+-]?(\d+(\.\d*)?|\.\d+)([eE][+-]?\d+)?$/;

/** KomiDocument(true).insertString (KomiDocument.java:16-40): the result must still be a number
 *  (a lone "-" is allowed while typing); after an existing decimal point only one digit may follow
 *  and it must be 5. Removal is never filtered. */
function komiFilter(field: HTMLInputElement): void {
  field.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const start = field.selectionStart ?? field.value.length;
    const end = field.selectionEnd ?? start;
    const old = field.value.slice(0, start) + field.value.slice(end);
    const next = old.slice(0, start) + inserted + old.slice(start);
    let accept = next.trim() === "-" || BIG_DECIMAL.test(next);
    const decimal = old.lastIndexOf(".");
    if (accept && decimal > -1 && decimal < start) {
      if (next.length - (decimal + 1) > 1) accept = false;
      else accept = /^[+-]?\d+$/.test(inserted) && Number.parseInt(inserted, 10) === 5;
    }
    if (!accept) e.preventDefault();
  });
}

/** Double.parseDouble: surrounding blanks allowed; anything else that is not a number fails. */
function parseDouble(text: string): number | null {
  const s = text.trim();
  if (!BIG_DECIMAL.test(s)) return null;
  const n = Number(s);
  return Number.isFinite(n) ? n : null;
}

/** LizzieFrame.editGameInfo (LizzieFrame.java:3866-3879) → GameInfoDialog: one group of rows,
 *  黑 / 白 beside their stones, 贴目 / 让子, each field at its row's end, and 确定. Names and komi
 *  are editable, the handicap is read-only. Apply sends names and komi (GameInfoDialog.apply
 *  259-306); a komi that does not parse leaves the dialog open, as Java's NumberFormatException
 *  does, with the field marked invalid and "无效的数字" under 贴目 until the field is edited. */
export function openGameInfo(): void {
  const info = state.view?.document.info;
  if (!info) return;
  const body = document.createElement("div");
  body.className = "modal-body game-info";
  const field = (id: string, text: string, value: string, name: Node | string = text) => {
    const control = document.createElement("input");
    control.type = "text";
    control.value = value;
    control.id = `gameInfo-${id}`;
    control.setAttribute("aria-label", text);
    row(body, name, "", control);
    return control;
  };
  // The players' names beside their stones.
  const player = (stone: "stone-black" | "stone-white", text: string) => {
    const name = document.createDocumentFragment();
    name.append(icon(stone), text);
    return name;
  };
  const black = field("black", t("GameInfoDialog.black"), info.black, player("stone-black", t("GameInfoDialog.black")));
  const white = field("white", t("GameInfoDialog.white"), info.white, player("stone-white", t("GameInfoDialog.white")));
  const komi = field("komi", t("GameInfoDialog.komi"), javaDouble(info.komi));
  komi.inputMode = "decimal";
  komiFilter(komi);
  const komiError = document.createElement("span");
  komiError.className = "ds-row-sub game-info-error";
  komiError.id = "gameInfo-komi-error";
  komiError.hidden = true;
  komi.closest(".ds-row")!.firstElementChild!.append(komiError);
  komi.addEventListener("input", () => {
    komi.classList.remove("invalid");
    komi.removeAttribute("aria-invalid");
    komi.removeAttribute("aria-describedby");
    komiError.hidden = true;
  });
  // FORMAT_HANDICAP: DecimalFormat("0") limited to one integer digit.
  const handicap = field("handicap", t("GameInfoDialog.handicap"), String(Math.trunc(info.handicap) % 10));
  handicap.inputMode = "numeric";
  handicap.readOnly = true;
  const dialog = formPanel(t("GameInfoDialog.title"), body);
  const ok = primaryButton(t("GameInfoDialog.okButton"), async () => {
    const value = parseDouble(komi.value);
    if (value === null) {
      komi.classList.add("invalid");
      komi.setAttribute("aria-invalid", "true");
      komi.setAttribute("aria-describedby", komiError.id);
      komiError.textContent = t("LizzieChangeMove.txtMoveNumber.error");
      komiError.hidden = false;
      komi.focus();
      return;
    }
    await dialog.close();
    await edit({ op: "SetGameInfo", black: black.value, white: white.value, komi: value });
  });
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "gameInfo.cancel";
  actions(body, ok, cancel);
}

const STANDARD = [19, 15, 13, 9];

/** SetBoardSize.txtFieldValue: blank or too long reads as 0; otherwise Integer.parseInt. */
function sizeValue(text: string): number | null {
  const s = text.trim();
  if (s === "" || s.length >= String(2 ** 31 - 1).length) return 0;
  return /^[+-]?\d+$/.test(s) ? Number.parseInt(s, 10) : null;
}

/** 设置棋盘大小 (SetBoardSize.java): 19x19 / 15x15 / 13x13 / 9x9 / 其他 as one segmented control,
 *  then 其他's width x height fields, enabled only for 其他. The current size is preselected; a
 *  non-standard current size fills the 其他 fields, otherwise they show the saved other size. 确定
 *  (the default button) checks 其他 (checkMove: both at least 2, else both fields turn red with
 *  "无效的数字") and sends the size (applyChange 255-265). */
export function openBoardSize(): void {
  const view = state.view;
  if (!view) return;
  const { width: w, height: h } = view.document.size;
  const current = w === h && STANDARD.includes(w) ? w : null;
  const body = document.createElement("div");
  body.className = "modal-body board-size";
  const presets = document.createElement("div");
  presets.className = "ds-row board-size-presets";
  const seg = document.createElement("span");
  seg.className = "ds-seg";
  seg.setAttribute("role", "radiogroup");
  seg.setAttribute("aria-label", t("SetBoardSize.title"));
  presets.append(seg);
  body.append(presets);
  const radios: HTMLInputElement[] = [];
  const radio = (text: string, checked: boolean) => {
    const label = document.createElement("label");
    const control = document.createElement("input");
    control.type = "radio";
    control.name = "boardSize";
    control.checked = checked;
    control.tabIndex = -1;
    label.append(control, text);
    seg.append(label);
    radios.push(control);
    return control;
  };
  const sizes: [number, string][] = [[19, t("SetBoardSize.rdo19")], [15, t("SetBoardSize.rdo15")], [13, t("SetBoardSize.rdo13")], [9, t("SetBoardSize.rdo9")]];
  const choices = sizes.map(([n, text]) => [n, radio(text, current === n)] as const);
  const other = radio(t("SetBoardSize.rdoOther"), current === null);
  const field = (value: number, name: string) => {
    const control = document.createElement("input");
    control.type = "text";
    control.inputMode = "numeric";
    control.value = String(value);
    control.setAttribute("aria-label", name);
    return control;
  };
  // 其他's two fields share a row: each is named after its side (宽 / 高).
  const width = field(current === null ? w : view.settings.edit.otherSizeWidth, t("MoreEngines.lblWidth"));
  const height = field(current === null ? h : view.settings.edit.otherSizeHeight, t("MoreEngines.lblHeight"));
  row(body, t("SetBoardSize.rdoOther"), "", width, "x", height);
  const sync = () => {
    width.disabled = height.disabled = !other.checked;
  };
  radios.forEach((r) => (r.onchange = sync));
  sync();
  const dialog = formPanel(t("SetBoardSize.title"), body);
  const ok = primaryButton(t("SetBoardSize.okButton"), () => void apply());
  ok.tabIndex = -1;
  const cancel = button(product("cancel"), () => void dialog.close());
  cancel.dataset.key = "boardSize.cancel";
  actions(body, ok, cancel);
  const apply = async () => {
    let size: [number, number];
    if (other.checked) {
      const x = sizeValue(width.value);
      const y = sizeValue(height.value);
      if (x === null || y === null || x < 2 || y < 2) {
        for (const f of [width, height]) {
          f.classList.add("invalid");
          f.title = t("LizzieChangeMove.txtMoveNumber.error");
        }
        return;
      }
      size = [x, y];
    } else {
      const n = choices.find(([, r]) => r.checked)![0];
      size = [n, n];
    }
    await dialog.close();
    await edit({ op: "SetBoardSize", width: size[0], height: size[1] });
  };
  // getRootPane().setDefaultButton(okButton): Enter anywhere in the dialog but on 取消 applies.
  body.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target !== cancel) {
      e.preventDefault();
      void apply();
    }
  });
}
