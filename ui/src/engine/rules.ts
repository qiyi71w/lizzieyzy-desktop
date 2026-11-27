// The KataGo rules dialogs: the 规则面板 (SetKataRules.java:98-145, its rule sets those of
// RulesQuickMenu.java and RulesQuickSwitch.java), and the rules editor of 精析规则 and 本局规则's
// 自定义 (KataRulesEditor.java:59-111, 199-233, shared with SetAnalysisRules).
import { registerViewHandler, run, state } from "../app";
import { ruleSetOf } from "../contract/client";
import { KATA_RULE_PRESETS, RULE_SETS, type WorkbenchView } from "../contract/generated";
import { stepper } from "../design/stepper";
import { formPanel, message } from "../dialogs";
import { product, registerProduct, t } from "../i18n";
import { javaDouble } from "../workbench/paint";
import { button, footer, groupNote, groupTitle, input, primaryButton, row, segmented, switchRow } from "./controls";

export type Rules = Record<string, unknown>;

registerProduct({
  ZhCn: {
    rulesItemized: "逐项设置",
    rulesAutoApplyNote: "选中典型规则时，新对局的贴目也用它的贴目。",
  },
  ZhTw: {
    rulesItemized: "逐項設定",
    rulesAutoApplyNote: "選中典型規則時，新對局的貼目也用它的貼目。",
  },
  EnUs: {
    rulesItemized: "Individual rules",
    rulesAutoApplyNote: "When a classic rule set is selected, new games also use its komi.",
  },
  JaJp: {
    rulesItemized: "個別設定",
    rulesAutoApplyNote: "ルール選択でルールを選ぶと、新しい対局のコミもそのルールのコミになります。",
  },
  KoKr: {
    rulesItemized: "항목별 설정",
    rulesAutoApplyNote: "전통 규칙을 선택하면 새 대국의 덤도 그 규칙의 덤을 따릅니다.",
  },
  ThTh: {
    rulesItemized: "ตั้งค่าทีละข้อ",
    rulesAutoApplyNote: "เมื่อเลือกกฎคลาสสิก เกมใหม่จะใช้โคมิของกฎนั้นด้วย",
  },
});

// Java's captions end in a colon; a row's name does not.
const caption = (text: string) => text.replace(/[:：]\s*$/, "");

/** The seven rules, one row each with its choices as a segmented control, added to `body`.
 *  `changed` runs after every change, also the first fill; `rules()` is the edited rules
 *  (unselected fields absent), `fill(preset)` sets the fields `preset` has. */
function ruleRows(body: HTMLElement, initial: Rules, changed: (rules: Rules) => void): { rules: () => Rules; fill: (preset: Readonly<Rules>) => void } {
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
      control.dataset.key = `rules.${name}.${control.value}`;
      control.onchange = () => { rules[name] = boolean ? control.value === "true" : control.value; refresh(); };
    }
    controls.set(name, rule.inputs);
    row(body, caption(title), "", rule.el);
  }
  refresh();
  return { rules: () => ({ ...rules }), fill: (preset) => { rules = { ...rules, ...preset }; refresh(); } };
}

/** KataRulesEditor: one group, 典型规则 and then the rules' rows. 典型规则 are Java's buttons, one per
 *  preset on a line under the row's name: each fills the rules it sets and keeps no choice of its
 *  own. Returns the edited rules (unselected fields absent). */
function rulesEditor(body: HTMLElement, initial: Rules): () => Rules {
  const presets = document.createElement("span");
  presets.className = "engine-rules-presets";
  presets.setAttribute("role", "group");
  const presetRow = row(body, caption(t("SetKataRules.lblClassicRules")), "");
  presets.setAttribute("aria-labelledby", (presetRow.querySelector(".ds-row-title")!.id ||= "engine-rules-presets-name"));
  presetRow.append(presets);
  const editor = ruleRows(body, initial, () => {});
  for (const [key, text] of [
    ["chinese", t("SetKataRules.btnChnRule")],
    ["japanese", t("SetKataRules.btnJpnRule")],
    ["stone-scoring", t("SetKataRules.btnChnOldRule")],
    ["tromp-taylor", t("SetKataRules.btnTTRule")],
    ["goe", t("SetKataRules.btnGoeRule")],
  ]) {
    const preset = button(text, () => editor.fill(KATA_RULE_PRESETS[key]));
    preset.dataset.key = `rules.preset.${key}`;
    presets.append(preset);
  }
  return editor.rules;
}

/** SetAnalysisRules (SetAnalysisRules.java:19-82): the 精析 rules, tromp-taylor when none are set.
 *  Resolves to the edited rules JSON on 确定, null on 取消. */
export async function editAnalysisRules(current: string): Promise<string | null> {
  const body = document.createElement("div"); body.className = "engine-rules";
  let initial: Rules = { ...KATA_RULE_PRESETS["tromp-taylor"] };
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

/** The 规则面板's list row of the rules that are no rule set. */
const CUSTOM = "custom";

/** The 规则面板's watch over the rules it applied, while it waits for KataGo to take them. */
let watchSessionRules: ((view: WorkbenchView) => void) | null = null;
registerViewHandler((view) => watchSessionRules?.(view));

/** Whether rules JSON `json` holds exactly the fields and values of `rules`. */
function sameRules(json: string, rules: Rules): boolean {
  const parsed = JSON.parse(json) as Rules;
  const keys = Object.keys(parsed);
  return keys.length === Object.keys(rules).length && keys.every((k) => parsed[k] === rules[k]);
}

/** The 规则面板 while it is open, for the acceptance report. */
let openPanel: HTMLElement | null = null;

/** 规则面板 (SetKataRules, Shift+D and 引擎规则 in the menu). Top down: 典型规则, the rule sets
 *  (RULE_SETS) and 自定义 as one list; 贴目 and 启动时自动应用; the seven rules under 逐项设置.
 *  Picking a rule set fills the rules and the komi and turns 启动时自动应用 on (RulesQuickSwitch);
 *  自定义 changes nothing. After any change the list shows the rule set the rules and komi are, as
 *  the core recognises it, else 自定义. 确定 applies it all as one command and closes once KataGo
 *  confirmed the rules, staying with the failure otherwise; 取消 discards the draft. */
export async function openRules(): Promise<void> {
  const view = state.view;
  if (!view?.engine.foreground?.ready) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
  if (!view.engine.foreground.katago) { await message(t("Message.title"), t("SetKataRules.notKataGoHint")); return; }
  const engine = view.engine.foreground.index;
  let initial: Rules = {};
  try { const parsed: unknown = JSON.parse(view.engine.rules ?? "{}"); if (parsed && typeof parsed === "object" && !Array.isArray(parsed)) initial = { ...parsed }; } catch { /* No confirmed rules response: leave choices unset, like Java. */ }
  const body = document.createElement("div"); body.className = "engine-rules rules-panel";

  const listName = groupTitle(body, caption(t("SetKataRules.lblClassicRules")));
  listName.id = "rules-sets-name";
  const list = document.createElement("div");
  list.className = "rules-sets";
  list.setAttribute("role", "radiogroup");
  list.setAttribute("aria-labelledby", listName.id);
  const radios = [...RULE_SETS.map((s) => [s.id, t(s.labelKey)]), [CUSTOM, t("NewEngineGameDialog.matchRules.custom")]].map(([id, text]) => {
    const radio = input(id, "radio");
    radio.name = "rule-set";
    radio.dataset.key = `rules.set.${id}`;
    const choice = document.createElement("label");
    choice.className = "ds-row rules-set";
    const name = document.createElement("span");
    name.className = "ds-row-title";
    name.textContent = text;
    choice.append(radio, name);
    list.append(choice);
    return radio;
  });
  const select = (id: string | null) => { for (const r of radios) r.checked = r.value === (id ?? CUSTOM); };
  body.append(list);
  // The focus scrolls only the radio into view; the row it rings comes in whole.
  list.addEventListener("focusin", (e) => (e.target as HTMLElement).closest(".rules-set")?.scrollIntoView({ block: "nearest" }));

  // 贴目 starts at the game's; a handicap game keeps it (RulesQuickMenu.handicapKomi).
  const handicap = view.document.info.handicap > 0;
  const komi = input(javaDouble(view.komi), "number");
  komi.step = "0.5";
  komi.disabled = handicap;
  komi.dataset.key = "rules.komi";
  row(body, t("NewGameDialog.Komi"), handicap ? t("RulesQuickMenu.handicapKomi") : "", stepper(komi));
  const auto = switchRow(body, t("SetKataRules.chkbxAutoLoadRules"), product("rulesAutoApplyNote"), view.settings.engine.auto_load_kata_rules);
  auto.input.dataset.key = "rules.autoLoad";

  groupTitle(body, product("rulesItemized"));
  // The row picked last tells a draw rule from the rule set it counts like; until one is picked,
  // the rule set chosen in the settings does. A later answer supersedes an earlier one.
  let picked: string | null = null;
  // 自定义 was picked last: 确定 applies no rule set, whatever rule set the rules resemble.
  let customPicked = false;
  let asked = 0;
  let listening = false;
  // A handicap game keeps its komi in the field; the rule set picked there is still told by its
  // own komi, which becomes that of new games with 启动时自动应用 (RulesQuickSwitch).
  const komiValue = () => {
    const set = handicap ? RULE_SETS.find((s) => s.id === picked) : undefined;
    return set ? set.komi : komi.value.trim() === "" ? NaN : Number(komi.value);
  };
  const recognise = async (rules: Rules) => {
    customPicked = false;
    const ticket = ++asked;
    const k = komiValue();
    const id = Number.isFinite(k) ? await ruleSetOf(JSON.stringify(rules), k, picked) : null;
    if (ticket === asked) select(id);
  };
  const editor = ruleRows(body, initial, (rules) => { if (listening) void recognise(rules); });
  komi.addEventListener("input", () => { komi.classList.remove("invalid"); void recognise(editor.rules()); });
  for (const radio of radios) {
    radio.onchange = () => {
      asked++;
      const set = RULE_SETS.find((s) => s.id === radio.value);
      customPicked = !set;
      if (!set) return;
      picked = set.id;
      if (!handicap) komi.value = javaDouble(set.komi);
      komi.classList.remove("invalid");
      auto.input.checked = true;
      editor.fill(set.rules);
    };
  }
  const status = groupNote(body, view.engine.rules ? "" : t("SetKataRules.status.unconfirmed"));
  status.setAttribute("role", "status");
  select(view.engine.rules ? await ruleSetOf(view.engine.rules, view.komi, null) : null);
  listening = true;

  const dialog = formPanel(t("SetKataRules.title"), body);
  const apply = primaryButton(t("SetKataRules.btnApply"), async () => {
    if (!state.view?.engine.foreground?.ready || state.view.engine.foreground.index !== engine) { await message(t("Message.title"), t("LizzieFrame.setParamNoEngineHint")); return; }
    const k = komiValue();
    if (!Number.isFinite(k)) { komi.classList.add("invalid"); komi.focus(); return; }
    const rules = editor.rules();
    const requested = { ...rules, suicide: rules.suicide ?? false, hasButton: rules.hasButton ?? false, friendlyPassOk: rules.friendlyPassOk ?? false };
    asked++;
    const ruleSet = customPicked ? null : await ruleSetOf(JSON.stringify(requested), k, picked);
    select(ruleSet);
    status.textContent = "";
    const result = await run({ type: "SetKataRules", rules: JSON.stringify(requested), komi: k, rule_set: ruleSet, auto_load: auto.input.checked });
    // Pressed, 确定 applies; a panel closed meanwhile is not touched again.
    if (openPanel !== body || result?.type !== "RulesApplied") return;
    // SetKataRules.watchEngineRulesStatus: closed once the engine that took the rules reads them
    // back. Only the views after the core took them tell; that engine going away fails them,
    // and an engine started after it answers for itself, not for this 确定.
    const { after, instance } = result;
    apply.disabled = true;
    const failed = () => {
      watchSessionRules = null;
      status.textContent = t("SetKataRules.status.setFailed");
      apply.disabled = false;
    };
    watchSessionRules = (v) => {
      if (v.seq <= after) return;
      if (v.engine.foreground?.instance !== instance) {
        failed();
        return;
      }
      const applied = v.engine.session_rules;
      if (!applied || !sameRules(applied.rules, requested)) return;
      if (applied.status === "Confirmed") {
        watchSessionRules = null;
        void dialog.close();
      } else if (applied.status === "Failed") {
        failed();
      } else {
        status.textContent = t("SetKataRules.status.pending");
      }
    };
    if (state.view) watchSessionRules(state.view);
  });
  footer(body, { primary: apply, cancel: button(t("SetKataRules.btnCancel"), () => dialog.close()) });
  openPanel = body;
  await dialog.closed;
  openPanel = null;
  watchSessionRules = null;
}

/** The open 规则面板 as drawn, for the acceptance probe: the chosen row, each row's id and text,
 *  the komi (whether it can be changed and its row's text), 启动时自动应用, the status line, the
 *  parts that run past their row or past the panel, whether 启动时自动应用's switch stands beside
 *  its name rather than under its note, and the focused control's ring (its width, and whether
 *  the ring's box lies wholly in the scrolled body). Null while closed. */
export function rulesPanelReport() {
  if (!openPanel) return null;
  const box = openPanel.closest(".modal-box")!.getBoundingClientRect();
  const past = [...openPanel.querySelectorAll<HTMLElement>(".ds-row > *, .ds-seg > label, .ds-row-control > *")].filter((el) => {
    const r = el.getBoundingClientRect();
    const host = el.closest(".ds-row")!.getBoundingClientRect();
    return r.width > 0 && (r.right > host.right + 0.5 || r.left < host.left - 0.5 || r.right > box.right + 0.5);
  }).map((el) => el.textContent ?? el.className);
  const autoRow = openPanel.querySelector<HTMLInputElement>("[data-key='rules.autoLoad']")!;
  const text = autoRow.closest(".ds-row")!.querySelector(".ds-row-text")!.getBoundingClientRect();
  const komi = openPanel.querySelector<HTMLInputElement>("[data-key='rules.komi']")!;
  const active = document.activeElement instanceof HTMLElement && openPanel.contains(document.activeElement) ? document.activeElement : null;
  // A row, segment or stepper draws the ring of the control inside it.
  const ringed = active?.closest<HTMLElement>(".rules-set, .ds-seg > label, .ds-stepper") ?? active;
  const ring = ringed && getComputedStyle(ringed);
  // The body less the footer that stays over its foot; the footer's own buttons are in it.
  const body = openPanel.getBoundingClientRect();
  const foot = openPanel.querySelector(".ds-footer")!;
  const view = { left: body.left, right: body.right, top: body.top, bottom: foot.contains(active) ? body.bottom : Math.min(body.bottom, foot.getBoundingClientRect().top) };
  const rr = ringed?.getBoundingClientRect();
  const out = ring ? Math.max(0, parseFloat(ring.outlineOffset) + parseFloat(ring.outlineWidth)) : 0;
  return {
    selected: openPanel.querySelector<HTMLInputElement>("input[name='rule-set']:checked")?.value ?? null,
    sets: [...openPanel.querySelectorAll<HTMLInputElement>("input[name='rule-set']")].map((r) => [r.value, r.closest("label")!.textContent ?? ""]),
    komi: komi.value,
    komiDisabled: komi.disabled,
    komiRow: komi.closest(".ds-row")!.querySelector(".ds-row-text")!.textContent ?? "",
    autoLoad: autoRow.checked,
    focus: active && ring && rr ? {
      key: active.dataset.key ?? active.tagName.toLowerCase(),
      visible: active.matches(":focus-visible"),
      ring: ring.outlineStyle === "none" ? 0 : parseFloat(ring.outlineWidth),
      inside: rr.left - out >= view.left - 0.5 && rr.right + out <= view.right + 0.5 && rr.top - out >= view.top - 0.5 && rr.bottom + out <= view.bottom + 0.5,
      box: [rr.left, rr.top, rr.right, rr.bottom].map(Math.round),
      view: [view.left, view.top, view.right, view.bottom].map(Math.round),
    } : null,
    status: openPanel.querySelector("[role='status']")?.textContent ?? "",
    past,
    switchBesideName: autoRow.getBoundingClientRect().top < text.bottom,
    scrolls: openPanel.scrollHeight > openPanel.clientHeight + 1,
  };
}
