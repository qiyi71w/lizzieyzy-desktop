// SetKataRules.java:98-145; KataRulesEditor.java:59-111, 199-233 (shared with SetAnalysisRules).
import { run, state } from "../app";
import { formPanel, message } from "../dialogs";
import { format, t } from "../i18n";
import { button, footer, groupNote, primaryButton, row, segmented, switchRow } from "./controls";

export type Rules = Record<string, unknown>;
const PRESETS: Record<string, Rules> = {
  chinese: { ko: "SIMPLE", scoring: "AREA", tax: "NONE", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "N" },
  japanese: { ko: "SIMPLE", scoring: "TERRITORY", tax: "SEKI", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "stone-scoring": { ko: "SIMPLE", scoring: "AREA", tax: "ALL", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "tromp-taylor": { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: false, whiteHandicapBonus: "0" },
  goe: { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
};

/** KataRulesEditor: one group, 典型规则 and then one row per rule, each a segmented control,
 *  added to `body`. 典型规则 are Java's buttons, one per preset on a line under the row's name:
 *  each fills the rules it sets and keeps no choice of its own. `changed` runs after every change;
 *  the getter returns the edited rules (unselected fields absent). */
export function rulesEditor(body: HTMLElement, initial: Rules, changed: (rules: Rules) => void = () => {}): () => Rules {
  let rules: Rules = { ...initial };
  const controls = new Map<string, HTMLInputElement[]>();
  const refresh = () => {
    for (const [name, inputs] of controls) {
      for (const control of inputs) {
        control.checked = String(rules[name] ?? (control.dataset.boolean ? false : "")) === control.value;
        control.disabled = rules.scoring === "TERRITORY" && (name === "hasButton" || name === "friendlyPassOk");
      }
    }
    changed(rules);
  };
  // Java's captions end in a colon; a row's name does not.
  const caption = (text: string) => text.replace(/[:：]\s*$/, "");
  const presets = document.createElement("span");
  presets.className = "engine-rules-presets";
  presets.setAttribute("role", "group");
  for (const [key, text] of [
    ["chinese", t("SetKataRules.btnChnRule")],
    ["japanese", t("SetKataRules.btnJpnRule")],
    ["stone-scoring", t("SetKataRules.btnChnOldRule")],
    ["tromp-taylor", t("SetKataRules.btnTTRule")],
    ["goe", t("SetKataRules.btnGoeRule")],
  ]) {
    const preset = button(text, () => { rules = { ...rules, ...PRESETS[key] }; refresh(); });
    preset.dataset.key = `rules.preset.${key}`;
    presets.append(preset);
  }
  const presetRow = row(body, caption(t("SetKataRules.lblClassicRules")), "");
  presets.setAttribute("aria-labelledby", (presetRow.querySelector(".ds-row-title")!.id ||= "engine-rules-presets-name"));
  presetRow.append(presets);
  const options: [string, string, [string, string][], boolean?][] = [
    ["scoring", t("SetKataRules.lblScoringRule"), [["AREA", t("SetKataRules.rdoArea")], ["TERRITORY", t("SetKataRules.rdoTerritory")]]],
    ["ko", t("SetKataRules.lblKoRule"), [["SIMPLE", t("SetKataRules.rdoSimpleKo")], ["POSITIONAL", t("SetKataRules.rdoPositionKo")], ["SITUATIONAL", t("SetKataRules.rdoSituationalKo")]]],
    ["suicide", t("SetKataRules.lblMultiStoneSuicide"), [["true", t("SetKataRules.rdoSuicide")], ["false", t("SetKataRules.rdoNoSuicide")]], true],
    ["tax", t("SetKataRules.lblTaxRule"), [["NONE", t("SetKataRules.rdoNoTax")], ["SEKI", t("SetKataRules.rdoSeKiTax")], ["ALL", t("SetKataRules.rdoAllTax")]]],
    ["whiteHandicapBonus", t("SetKataRules.lblWhiteHandicapBonus"), [["0", t("SetKataRules.rdoNoHandicapKomi")], ["N", t("SetKataRules.rdoHandicapKomiN")], ["N-1", t("SetKataRules.rdoHandicapKomiN1")]]],
    ["hasButton", t("SetKataRules.lblHasButton"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
    ["friendlyPassOk", t("SetKataRules.lblFriendlyPassOk"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
  ];
  for (const [name, title, choices, boolean] of options) {
    const rule = segmented(`rule-${name}`, choices, "");
    for (const control of rule.inputs) {
      if (boolean) control.dataset.boolean = "true";
      control.onchange = () => { rules[name] = boolean ? control.value === "true" : control.value; refresh(); };
    }
    controls.set(name, rule.inputs);
    row(body, caption(title), "", rule.el);
  }
  refresh();
  return () => ({ ...rules });
}

/** SetAnalysisRules (SetAnalysisRules.java:19-82): the 精析 rules, tromp-taylor when none are set.
 *  Resolves to the edited rules JSON on 确定, null on 取消. */
export async function editAnalysisRules(current: string): Promise<string | null> {
  const body = document.createElement("div"); body.className = "engine-rules";
  let initial: Rules = PRESETS["tromp-taylor"];
  try { const parsed: unknown = current ? JSON.parse(current) : null; if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) initial = { ...parsed }; } catch { /* Unreadable saved rules: start from tromp-taylor. */ }
  const rules = rulesEditor(body, initial);
  let result: string | null = null;
  const dialog = formPanel(t("SetAnalysisRules.title"), body);
  footer(body, {
    primary: primaryButton(t("SetKataRules.btnApply"), () => { result = JSON.stringify(rules()); void dialog.close(); }),
    cancel: button(t("SetKataRules.btnCancel"), () => dialog.close()),
  });
  await dialog.closed;
  return result;
}

/** SetKataRules.composeMatchRules (SetKataRules.java:63-72): the editor alone, for 本局规则's
 *  自定义; nothing is sent to an engine. Resolves to the composed rules JSON, null on 取消. */
export async function composeMatchRules(initialJson: string): Promise<string | null> {
  const body = document.createElement("div"); body.className = "engine-rules";
  let initial: Rules = {};
  try { const parsed: unknown = JSON.parse(initialJson || "{}"); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) initial = { ...parsed }; } catch { /* Unreadable prefill: leave choices unset. */ }
  const rules = rulesEditor(body, initial);
  let result: string | null = null;
  const dialog = formPanel(t("SetKataRules.title"), body);
  footer(body, {
    primary: primaryButton(t("SetKataRules.btnApply"), () => {
      const r = rules();
      result = JSON.stringify({ ...r, suicide: r.suicide ?? false, hasButton: r.hasButton ?? false, friendlyPassOk: r.friendlyPassOk ?? false });
      void dialog.close();
    }),
    cancel: button(t("SetKataRules.btnCancel"), () => dialog.close()),
  });
  await dialog.closed;
  return result;
}

export async function openRules(): Promise<void> {
  const view = state.view;
  if (!view?.engine.foreground?.ready) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
  if (!view.engine.foreground.katago) { await message(t("Message.title"), t("SetKataRules.notKataGoHint")); return; }
  const engine = view.engine.foreground.index;
  const body = document.createElement("div"); body.className = "engine-rules";
  let initial: Rules = {};
  try { const parsed: unknown = JSON.parse(view.engine.rules ?? "{}"); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) initial = { ...parsed }; } catch { /* No confirmed rules response: leave choices unset, like Java. */ }
  // The 贴目 convention note follows the group; the editor fills it while building its rows.
  const hint = document.createElement("p"); hint.className = "ds-note";
  const rulesNow = rulesEditor(body, initial, (rules) => {
    const komi = state.view?.komi ?? view.komi;
    const conventional = rules.scoring === "TERRITORY" ? 6.5 : rules.scoring === "AREA" ? (rules.hasButton ? 7 : 7.5) : null;
    hint.textContent = conventional !== null && conventional !== komi && view.document.info.handicap === 0
      ? format(t("SetKataRules.komiConventionHint"), komi, conventional) : "";
  });
  // 自动应用 ends the rules' group; the notes follow it.
  const auto = switchRow(body, t("SetKataRules.chkbxAutoLoadRules"), "", view.settings.engine.auto_load_kata_rules);
  body.append(hint);
  groupNote(body, view.engine.rules ? "" : t("SetKataRules.status.unconfirmed"));
  const dialog = formPanel(t("SetKataRules.title"), body);
  const apply = primaryButton(t("SetKataRules.btnApply"), async () => {
    if (!state.view?.engine.foreground?.ready || state.view.engine.foreground.index !== engine) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
    const rules = rulesNow();
    const requested = { ...rules, suicide: rules.suicide ?? false, hasButton: rules.hasButton ?? false, friendlyPassOk: rules.friendlyPassOk ?? false };
    const result = await run({ type: "SetKataRules", rules: JSON.stringify(requested), auto_load: auto.input.checked });
    if (result && result.type !== "Refused") await dialog.close();
  });
  footer(body, { primary: apply, cancel: button(t("SetKataRules.btnCancel"), () => dialog.close()) });
  await dialog.closed;
}
