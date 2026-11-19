// KataGo 激进参数PDA设置 (SetKataPDA.java), opened from the "..." beside the toolbar's PDA, and the
// board popup's 优先分析此点 / 清除优先分析 (RightClickMenu.java:148-156, 318-336). The core owns the
// saved choice, the engine's read-back and the priority points; this window holds its drafts.
import { run, state, subscribe } from "../app";
import type { CustomPdaMode, Point } from "../contract/generated";
import { formPanel, message } from "../dialogs";
import type { ContextEntry } from "../editing/contextMenu";
import { isChinese, t } from "../i18n";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { actions, button } from "./controls";

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

/** A JFontCheckBox with its text; `key` names it for automation. */
function check(text: string, key: string, checked: boolean): { label: HTMLLabelElement; box: HTMLInputElement } {
  const box = el("input");
  box.type = "checkbox";
  box.checked = checked;
  box.dataset.key = key;
  const label = el("label", "custom-pda-check");
  label.append(box, text);
  return { label, box };
}

function valueField(text: string, key: string, name: string): HTMLInputElement {
  const field = el("input", "custom-pda-value");
  field.value = text;
  field.dataset.key = key;
  field.setAttribute("aria-label", name);
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

  const body = el("div", "kata-params custom-pda");
  const current = el("div", "custom-pda-current");
  const pda = el("span", "custom-pda-current-value", pdaText());
  pda.dataset.key = "customPda.current";
  current.append(el("span", "", t("SetKataPDA.lblCurPDA")), pda);

  // ButtonGroup of three JCheckBoxes: one is always selected, a click on it keeps it.
  const dynamic = check(t("SetKataPDA.chkDymPda"), "customPda.dynamic", mode === "Dynamic");
  const fixed = check(t("SetKataPDA.chkStaticPda"), "customPda.fixed", mode === "Fixed");
  const disabled = check(t("SetKataPDA.chkNoPDA"), "customPda.disabled", mode === "Disabled");
  const cap = valueField(javaDouble(capValue), "customPda.cap", t("SetKataPDA.lblDymRatio"));
  const value = valueField(javaDouble(fixedValue), "customPda.value", t("SetKataPDA.chkStaticPda"));
  const choices: [HTMLInputElement, CustomPdaMode][] = [[dynamic.box, "Dynamic"], [fixed.box, "Fixed"], [disabled.box, "Disabled"]];
  const enableFields = () => {
    cap.disabled = !dynamic.box.checked;
    value.disabled = !fixed.box.checked;
  };
  for (const [box] of choices) {
    box.onchange = () => {
      for (const [other] of choices) other.checked = other === box;
      enableFields();
    };
  }
  enableFields();
  const modes = el("div", "custom-pda-modes");
  const ratio = el("label", "custom-pda-ratio");
  ratio.append(t("SetKataPDA.lblDymRatio"), cap);
  modes.append(dynamic.label, ratio, fixed.label, value, disabled.label);

  const auto = check(t("SetKataPDA.chkAutoPDA"), "customPda.autoApply", saved.autoApply);
  body.append(current, modes, el("div", "", t("SetKataPDA.lblTip")), auto.label);
  body.append(el("div", "custom-pda-warning", t("SetKataPDA.lblpdakatagopdapda")));
  // Two lines Java writes in Chinese only, without resources (SetKataPDA.java:280-283, 300-303).
  if (isChinese()) {
    body.append(
      el("div", "custom-pda-warning", "被让子需要消极(PDA为负,如默认为白视角,则黑方需设置PDA为正才是消极)"),
      el("div", "custom-pda-warning", "一般默认为白方视角,也可修改配置文件改为黑方视角,或无视角(轮谁下是谁的视角)"),
    );
  }

  const dialog = formPanel(t("SetKataPDA.title"), body);
  currentPda = pda;
  void dialog.closed.then(() => {
    if (currentPda === pda) currentPda = null;
  });
  const apply = button(t("SetKataPDA.btnApply"), async () => {
    const chosen = choices.find(([box]) => box.checked)?.[1] ?? "Disabled";
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
    const result = await run({ type: "SaveCustomPda", settings: { mode: chosen, dynamicCap, fixedValue: fixedPda, autoApply: auto.box.checked } });
    if (!result || result.type === "Refused") return;
    await dialog.close();
  });
  apply.dataset.key = "customPda.apply";
  const cancel = button(t("SetKataPDA.btnCancel"), () => void dialog.close());
  cancel.dataset.key = "customPda.cancel";
  actions(body, apply, cancel);
  await dialog.closed;
}
