// KataGo 激进参数PDA设置 (SetKataPDA.java), opened from the "..." beside the toolbar's PDA, and the
// board popup's 优先分析此点 / 清除优先分析 (RightClickMenu.java:148-156, 318-336). The core owns the
// saved choice, the engine's read-back and the priority points; this window holds its drafts.
import { run, state, subscribe } from "../app";
import type { CustomPdaMode, Point } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import type { ContextEntry } from "../editing/contextMenu";
import { isChinese, product, registerProduct, t } from "../i18n";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { button, footer, groupNote, primaryButton, row, segmented, switchRow } from "./controls";
import { paramCaption } from "./leelaOptions";

/** The name of the row of the three PDA choices, which Java shows without a caption. */
registerProduct({
  ZhCn: { customPdaModeGroup: "PDA 方式" },
  ZhTw: { customPdaModeGroup: "PDA 方式" },
  EnUs: { customPdaModeGroup: "PDA mode" },
  JaJp: { customPdaModeGroup: "PDA の方式" },
  KoKr: { customPdaModeGroup: "PDA 방식" },
  ThTh: { customPdaModeGroup: "โหมด PDA" },
});

/** Leelaz.isKatagoCustom: a `KataGoCustom` build, or a `KataGoPda` one (Leelaz.java:6246-6253). */
export function kataGoCustom(): boolean {
  const name = state.view?.engine.foreground?.gtp_name ?? "";
  return name.startsWith("KataGoCustom") || name.startsWith("KataGoPda");
}

/** 优先分析此点 and 清除优先分析 with the separator after them, for an empty intersection while a
 *  KataGoCustom / KataGoPda engine is in front; nothing otherwise. */
export function priorityMenuEntries(point: Point): ContextEntry[] {
  if (!kataGoCustom()) return [];
  return [
    { key: "RightClickMenu.priority", label: t("RightClickMenu.priority"), icon: "priority-point", run: () => void run({ type: "SetPriorityPoint", point }) },
    { key: "RightClickMenu.clearPriority", label: t("RightClickMenu.clearPriority"), icon: "clear-all", run: () => void run({ type: "ClearPriorityPoints" }) },
    "sep",
  ];
}

/** curPDA of the open window: the engine's PDA as it reports it (String.format("%.3f")). */
let currentPda: HTMLElement | null = null;

function pdaText(): string {
  return (state.view?.engine.custom_pda.pda ?? 0).toFixed(3);
}

subscribe(() => {
  if (currentPda) currentPda.textContent = pdaText();
});

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

/** A typed number of the window; the row it stands in names it. */
function valueField(text: string, key: string): HTMLInputElement {
  const field = el("input");
  field.value = text;
  field.inputMode = "decimal";
  field.dataset.key = key;
  return field;
}

/** Menu.more2: opens the window whatever the foreground KataGo is; only a `KataGoPda` engine gets
 *  its commands (the core decides). The choice and values start from the engine's read-back once it
 *  reported them, else from the saved settings (SetKataPDA.java:285-298). */
export async function openCustomPda(): Promise<void> {
  const view = state.view;
  if (!view) return;
  const saved = view.settings.engine.custom_pda;
  const live = view.engine.custom_pda;
  const reported = live.pda !== null || live.cap !== null;
  const mode: CustomPdaMode = reported ? live.mode : saved.mode;
  const capValue = live.cap ?? saved.dynamicCap ?? 0;
  const fixedValue = live.pda ?? saved.fixedValue ?? 0;
  // Java's captions end in a colon; a row's name does not.
  const caption = (text: string) => text.replace(/\\?[:：]\s*$/, "");

  // One group: 当前PDA, the choice, the value each choice uses, 自动加载; the warnings under it.
  const body = el("div", "custom-pda-dialog");
  const pda = el("span", "custom-pda-current", pdaText());
  pda.dataset.key = "customPda.current";
  row(body, caption(t("SetKataPDA.lblCurPDA")), "", pda);

  // ButtonGroup of three JCheckBoxes, one always selected: a segmented control of the three.
  const modes = segmented<CustomPdaMode>("custom-pda-mode", [["Dynamic", t("SetKataPDA.chkDymPda")], ["Fixed", t("SetKataPDA.chkStaticPda")], ["Disabled", t("SetKataPDA.chkNoPDA")]], mode);
  const [dynamic, fixed] = modes.inputs;
  modes.inputs.forEach((radio, k) => (radio.dataset.key = `customPda.${["dynamic", "fixed", "disabled"][k]}`));
  row(body, product("customPdaModeGroup"), "", modes.el);
  const cap = valueField(javaDouble(capValue), "customPda.cap");
  const value = valueField(javaDouble(fixedValue), "customPda.value");
  // 动态系数 with Java's tip on it, then the fixed value; the one the choice does not use is off.
  row(body, caption(t("SetKataPDA.lblDymRatio")), t("SetKataPDA.lblTip"), cap);
  row(body, t("SetKataPDA.chkStaticPda"), "", value);
  const enableFields = () => {
    cap.disabled = !dynamic.checked;
    value.disabled = !fixed.checked;
  };
  for (const radio of modes.inputs) radio.onchange = enableFields;
  enableFields();

  const [autoName, autoSub] = paramCaption(t("SetKataPDA.chkAutoPDA"));
  const auto = switchRow(body, autoName, autoSub, saved.autoApply);
  auto.input.dataset.key = "customPda.autoApply";
  // The warnings, one line each; two of them Java writes in Chinese only, without resources
  // (SetKataPDA.java:280-283, 300-303).
  const notes = [t("SetKataPDA.lblpdakatagopdapda")];
  if (isChinese()) {
    notes.push(
      "被让子需要消极(PDA为负,如默认为白视角,则黑方需设置PDA为正才是消极)",
      "一般默认为白方视角,也可修改配置文件改为黑方视角,或无视角(轮谁下是谁的视角)",
    );
  }
  groupNote(body, notes.join("\n"));

  const dialog = formPanel(t("SetKataPDA.title"), body);
  currentPda = pda;
  void dialog.closed.then(() => {
    if (currentPda === pda) currentPda = null;
  });
  const apply = primaryButton(t("SetKataPDA.btnApply"), async () => {
    const chosen = modes.value();
    const capText = cap.value.trim();
    const valueText = value.value.trim();
    const dynamicCap = parseJavaDouble(capText);
    const fixedPda = parseJavaDouble(valueText);
    // An empty field is kept as no value and sends nothing; text that is not a number, or a
    // dynamic ratio of 0, is refused like Java's 输入参数有误.
    const wrong = chosen === "Dynamic" ? capText !== "" && (dynamicCap === null || dynamicCap === 0) : chosen === "Fixed" && valueText !== "" && fixedPda === null;
    if (wrong) {
      await message(t("Message.title"), t("SetKataPDA.wrongParameter"));
      return;
    }
    const result = await run({ type: "SaveCustomPda", settings: { mode: chosen, dynamicCap, fixedValue: fixedPda, autoApply: auto.input.checked } });
    if (!result || result.type === "Refused") return;
    await dialog.close();
  });
  apply.dataset.key = "customPda.apply";
  const cancel = button(t("SetKataPDA.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "customPda.cancel";
  footer(body, { primary: apply, cancel });
  await dialog.closed;
}
