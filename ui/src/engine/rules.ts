// SetKataRules.java:98-145; KataRulesEditor.java:59-111, 199-233 (shared with SetAnalysisRules).
import { run, state } from "../app";
import { formPanel, message } from "../dialogs";
import { format, t } from "../i18n";
import { actions, button, checkbox } from "./controls";

export type Rules = Record<string, unknown>;
const PRESETS: Record<string, Rules> = {
  chinese: { ko: "SIMPLE", scoring: "AREA", tax: "NONE", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "N" },
  japanese: { ko: "SIMPLE", scoring: "TERRITORY", tax: "SEKI", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "stone-scoring": { ko: "SIMPLE", scoring: "AREA", tax: "ALL", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "tromp-taylor": { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: false, whiteHandicapBonus: "0" },
  goe: { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
};

/** KataRulesEditor: the classic presets and one radio row per rule, added to `body`. `changed`
 *  runs after every choice; the getter returns the edited rules (unselected fields absent). */
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
  const preset = (name: string) => () => { rules = { ...rules, ...PRESETS[name] }; refresh(); };
  actions(body, t("SetKataRules.lblClassicRules"),
    button(t("SetKataRules.btnChnRule"), preset("chinese")),
    button(t("SetKataRules.btnJpnRule"), preset("japanese")),
    button(t("SetKataRules.btnChnOldRule"), preset("stone-scoring")),
    button(t("SetKataRules.btnTTRule"), preset("tromp-taylor")),
    button(t("SetKataRules.btnGoeRule"), preset("goe")));
  const options: [string, string, [string, string][], boolean?][] = [
    ["scoring", t("SetKataRules.lblScoringRule"), [["AREA", t("SetKataRules.rdoArea")], ["TERRITORY", t("SetKataRules.rdoTerritory")]]],
    ["ko", t("SetKataRules.lblKoRule"), [["SIMPLE", t("SetKataRules.rdoSimpleKo")], ["POSITIONAL", t("SetKataRules.rdoPositionKo")], ["SITUATIONAL", t("SetKataRules.rdoSituationalKo")]]],
    ["suicide", t("SetKataRules.lblMultiStoneSuicide"), [["true", t("SetKataRules.rdoSuicide")], ["false", t("SetKataRules.rdoNoSuicide")]], true],
    ["tax", t("SetKataRules.lblTaxRule"), [["NONE", t("SetKataRules.rdoNoTax")], ["SEKI", t("SetKataRules.rdoSeKiTax")], ["ALL", t("SetKataRules.rdoAllTax")]]],
    ["whiteHandicapBonus", t("SetKataRules.lblWhiteHandicapBonus"), [["0", t("SetKataRules.rdoNoHandicapKomi")], ["N", t("SetKataRules.rdoHandicapKomiN")], ["N-1", t("SetKataRules.rdoHandicapKomiN1")]]],
    ["hasButton", t("SetKataRules.lblHasButton"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
    ["friendlyPassOk", t("SetKataRules.lblFriendlyPassOk"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
  ];
  const rows = document.createElement("div");
  for (const [name, caption, choices, boolean] of options) {
    const row = document.createElement("div"); row.className = "engine-rule-row";
    const title = document.createElement("span"); title.textContent = caption; row.append(title);
    const inputs: HTMLInputElement[] = [];
    for (const [value, text] of choices) {
      const control = document.createElement("input"); control.type = "radio"; control.name = `rule-${name}`; control.value = value;
      if (boolean) control.dataset.boolean = "true";
      control.onchange = () => { rules[name] = boolean ? value === "true" : value; refresh(); };
      const label = document.createElement("label"); label.append(control, text); row.append(label); inputs.push(control);
    }
    controls.set(name, inputs); rows.append(row);
  }
  body.append(rows);
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
  actions(body,
    button(t("SetKataRules.btnApply"), () => { result = JSON.stringify(rules()); void dialog.close(); }),
    button(t("SetKataRules.btnCancel"), () => dialog.close()));
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
  const hint = document.createElement("div"); hint.className = "engine-hint";
  const rulesNow = rulesEditor(body, initial, (rules) => {
    const komi = state.view?.komi ?? view.komi;
    const conventional = rules.scoring === "TERRITORY" ? 6.5 : rules.scoring === "AREA" ? (rules.hasButton ? 7 : 7.5) : null;
    hint.textContent = conventional !== null && conventional !== komi && view.document.info.handicap === 0
      ? format(t("SetKataRules.komiConventionHint"), komi, conventional) : "";
  });
  const auto = checkbox(t("SetKataRules.chkbxAutoLoadRules"), view.settings.engine.auto_load_kata_rules);
  body.append(auto.label, hint);
  const status = document.createElement("div"); status.textContent = view.engine.rules ? "" : t("SetKataRules.status.unconfirmed"); body.append(status);
  const dialog = formPanel(t("SetKataRules.title"), body);
  actions(body, button(t("SetKataRules.btnApply"), async () => {
    if (!state.view?.engine.foreground?.ready || state.view.engine.foreground.index !== engine) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
    const rules = rulesNow();
    const requested = { ...rules, suicide: rules.suicide ?? false, hasButton: rules.hasButton ?? false, friendlyPassOk: rules.friendlyPassOk ?? false };
    const result = await run({ type: "SetKataRules", rules: JSON.stringify(requested), auto_load: auto.input.checked });
    if (result && result.type !== "Refused") await dialog.close();
  }), button(t("SetKataRules.btnCancel"), () => dialog.close()));
  await dialog.closed;
}
