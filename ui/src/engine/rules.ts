// SetKataRules.java:98-145; KataRulesEditor.java:59-111, 199-233.
import { run, state } from "../app";
import { formPanel, message } from "../dialogs";
import { format, t } from "../i18n";
import { actions, button, checkbox } from "./controls";

type Rules = Record<string, unknown>;
const PRESETS: Record<string, Rules> = {
  chinese: { ko: "SIMPLE", scoring: "AREA", tax: "NONE", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "N" },
  japanese: { ko: "SIMPLE", scoring: "TERRITORY", tax: "SEKI", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "stone-scoring": { ko: "SIMPLE", scoring: "AREA", tax: "ALL", suicide: false, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
  "tromp-taylor": { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: false, whiteHandicapBonus: "0" },
  goe: { ko: "POSITIONAL", scoring: "AREA", tax: "NONE", suicide: true, hasButton: false, friendlyPassOk: true, whiteHandicapBonus: "0" },
};

export async function openRules(): Promise<void> {
  const view = state.view;
  if (!view?.engine.foreground?.ready) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
  if (!view.engine.foreground.katago) { await message(t("Message.title"), t("SetKataRules.notKataGoHint")); return; }
  const engine = view.engine.foreground.index;
  const body = document.createElement("div"); body.className = "engine-rules";
  let rules: Rules = {};
  try { const parsed: unknown = JSON.parse(view.engine.rules ?? "{}"); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) rules = { ...parsed }; } catch { /* No confirmed rules response: leave choices unset, like Java. */ }
  const rows = document.createElement("div");
  const controls = new Map<string, HTMLInputElement[]>();
  const hint = document.createElement("div"); hint.className = "engine-hint";
  const refresh = () => {
    for (const [name, inputs] of controls) {
      for (const control of inputs) {
        control.checked = String(rules[name] ?? (control.dataset.boolean ? false : "")) === control.value;
        control.disabled = rules.scoring === "TERRITORY" && (name === "hasButton" || name === "friendlyPassOk");
      }
    }
    const komi = state.view?.komi ?? view.komi;
    const conventional = rules.scoring === "TERRITORY" ? 6.5 : rules.scoring === "AREA" ? (rules.hasButton ? 7 : 7.5) : null;
    hint.textContent = conventional !== null && conventional !== komi && view.document.info.handicap === 0
      ? format(t("SetKataRules.komiConventionHint"), komi, conventional) : "";
  };
  actions(body, t("SetKataRules.lblClassicRules"),
    button(t("SetKataRules.btnChnRule"), () => { rules = { ...rules, ...PRESETS.chinese }; refresh(); }),
    button(t("SetKataRules.btnJpnRule"), () => { rules = { ...rules, ...PRESETS.japanese }; refresh(); }),
    button(t("SetKataRules.btnChnOldRule"), () => { rules = { ...rules, ...PRESETS["stone-scoring"] }; refresh(); }),
    button(t("SetKataRules.btnTTRule"), () => { rules = { ...rules, ...PRESETS["tromp-taylor"] }; refresh(); }),
    button(t("SetKataRules.btnGoeRule"), () => { rules = { ...rules, ...PRESETS.goe }; refresh(); }));
  const options: [string, string, [string, string][], boolean?][] = [
    ["scoring", t("SetKataRules.lblScoringRule"), [["AREA", t("SetKataRules.rdoArea")], ["TERRITORY", t("SetKataRules.rdoTerritory")]]],
    ["ko", t("SetKataRules.lblKoRule"), [["SIMPLE", t("SetKataRules.rdoSimpleKo")], ["POSITIONAL", t("SetKataRules.rdoPositionKo")], ["SITUATIONAL", t("SetKataRules.rdoSituationalKo")]]],
    ["suicide", t("SetKataRules.lblMultiStoneSuicide"), [["true", t("SetKataRules.rdoSuicide")], ["false", t("SetKataRules.rdoNoSuicide")]], true],
    ["tax", t("SetKataRules.lblTaxRule"), [["NONE", t("SetKataRules.rdoNoTax")], ["SEKI", t("SetKataRules.rdoSeKiTax")], ["ALL", t("SetKataRules.rdoAllTax")]]],
    ["whiteHandicapBonus", t("SetKataRules.lblWhiteHandicapBonus"), [["0", t("SetKataRules.rdoNoHandicapKomi")], ["N", t("SetKataRules.rdoHandicapKomiN")], ["N-1", t("SetKataRules.rdoHandicapKomiN1")]]],
    ["hasButton", t("SetKataRules.lblHasButton"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
    ["friendlyPassOk", t("SetKataRules.lblFriendlyPassOk"), [["true", t("SetKataRules.rdoButtonGo")], ["false", t("SetKataRules.rdoNoButtonGo")]], true],
  ];
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
  const auto = checkbox(t("SetKataRules.chkbxAutoLoadRules"), view.settings.engine.auto_load_kata_rules);
  body.append(auto.label, hint);
  const status = document.createElement("div"); status.textContent = view.engine.rules ? "" : t("SetKataRules.status.unconfirmed"); body.append(status);
  const dialog = formPanel(t("SetKataRules.title"), body);
  actions(body, button(t("SetKataRules.btnApply"), async () => {
    if (!state.view?.engine.foreground?.ready || state.view.engine.foreground.index !== engine) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
    const requested = { ...rules, suicide: rules.suicide ?? false, hasButton: rules.hasButton ?? false, friendlyPassOk: rules.friendlyPassOk ?? false };
    const result = await run({ type: "SetKataRules", rules: JSON.stringify(requested), auto_load: auto.input.checked });
    if (result && result.type !== "Refused") await dialog.close();
  }), button(t("SetKataRules.btnCancel"), () => dialog.close()));
  refresh();
  await dialog.closed;
}
