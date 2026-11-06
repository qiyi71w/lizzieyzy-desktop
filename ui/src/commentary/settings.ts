// AI 解说设置 (TeacherSettingsDialog + ChatGptSettingsPanel): a modal dialog over the commentary
// window with the 连接服务 and 讲解偏好 pages. Every field is a draft owned here until 保存设置 /
// 保存偏好; 取消 and Escape discard them. Account sign-in, selection and sign-out act at once, as in
// Java. The access key exists only in its field and in the command that saves or lists with it.
import { invoke } from "@tauri-apps/api/core";
import type { Command, CommentaryAccount, CommentaryModel, CommentaryView, Preferences, Provider } from "../contract/generated";
import { format, product, t } from "../i18n";
import { icon } from "./icons";
import type { SettingsReport } from "./model";
import { send } from "./send";
import { failureText, keyOr } from "./text";

export interface SettingsDialog {
  refresh(): void;
  report(): SettingsReport;
  targets(): HTMLElement[];
}

interface Host {
  view(): CommentaryView | null;
  onClose(): void;
  /** Something the acceptance report shows changed without a new view. */
  changed(): void;
}

type ConnectionOp = Extract<Command, { type: "CommentaryConnection" }>["op"];
const connection = (op: ConnectionOp) => send({ type: "CommentaryConnection", op });

/** TeacherSettingsStyle.text: the redesigned dialog's wording. */
const design = (key: string) => t(`Teacher.settings.design.${key}`);

/** RankChoice: 18 级 … 1 级, then 业余 1 段 … 9 段. */
const RANKS: { mode: "k" | "d"; num: number }[] = [
  ...Array.from({ length: 18 }, (_, k) => ({ mode: "k" as const, num: 18 - k })),
  ...Array.from({ length: 9 }, (_, k) => ({ mode: "d" as const, num: k + 1 })),
];

/** TeacherSettings.validateBaseUrl, reporting with the dialog's messages (validateAddressInput). */
function addressProblem(value: string): string | null {
  let candidate = value.trim();
  if (!candidate) return t("Teacher.settings.enterAddress");
  while (candidate.endsWith("/")) candidate = candidate.slice(0, -1);
  const invalid = t("Teacher.settings.invalidAddress");
  if (/[?#]/.test(candidate)) return invalid;
  let url: URL;
  try {
    url = new URL(candidate);
  } catch {
    return invalid;
  }
  const scheme = url.protocol.toLowerCase();
  if ((scheme !== "https:" && scheme !== "http:") || !url.hostname || url.username || url.password) return invalid;
  const host = url.hostname.toLowerCase().replace(/^\[|\]$/g, "");
  if (scheme === "http:" && !["localhost", "127.0.0.1", "::1", "0:0:0:0:0:0:0:1"].includes(host)) return invalid;
  return null;
}

/** TeacherSettings.validateModel. */
function modelProblem(value: string): string | null {
  const candidate = value.trim();
  if (!candidate) return t("Teacher.settings.enterModel");
  // Java's own (untranslated) message for an over-long name or one with control characters.
  if (candidate.length > 160 || /[\u0000-\u001f\u007f-\u009f]/.test(candidate)) return "Model name is invalid.";
  return null;
}

/** ChatGptSettingsPanel.Effort.toString: the bundle label plus the raw value when they differ. */
function effortLabel(value: string, modelDefault: string | null): string {
  if (!value) {
    if (!modelDefault) return t("Teacher.chatgpt.reasoning.defaultUnknown");
    return `${t("Teacher.chatgpt.reasoning.default")}: ${effortLabel(modelDefault, null)}`;
  }
  const label = keyOr(`Teacher.chatgpt.reasoning.${value}`, value);
  return label.toLowerCase() === value.toLowerCase() ? label : `${label} (${value})`;
}

/** Only known localized ChatGPT messages are shown; anything else may carry a URL. */
const chatError = (message: string) => (message.startsWith("Teacher.") && t(message) !== message ? t(message) : t("Teacher.chatgpt.error.network"));

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

function btn(text: string, className: string, key: string): HTMLButtonElement {
  const b = el("button", `tc-btn ${className}`.trim(), text);
  b.type = "button";
  b.dataset.key = key;
  return b;
}

function select(key: string, label: string): HTMLSelectElement {
  const s = el("select", "ts-field");
  s.dataset.key = key;
  s.setAttribute("aria-label", label);
  return s;
}

/** Replaces the options when their list changed; keeps the element (and its focus) otherwise. */
function setOptions(s: HTMLSelectElement, options: [string, string][]): void {
  const signature = JSON.stringify(options);
  if (s.dataset.options === signature) return;
  s.dataset.options = signature;
  s.replaceChildren(
    ...options.map(([value, text]) => {
      const o = el("option", "", text);
      o.value = value;
      return o;
    }),
  );
}

function privacyRow(): HTMLElement {
  const row = el("div", "ts-privacy");
  row.append(icon("lock"), el("span", "", design("privacy")));
  return row;
}

export function openSettings(host: Host): SettingsDialog {
  const previous = document.activeElement as HTMLElement | null;
  const windowRoot = document.getElementById("tc-root");
  windowRoot?.setAttribute("inert", "");

  const overlay = el("div", "ts-overlay");
  const box = el("div", "ts-box");
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "true");
  box.setAttribute("aria-label", t("Teacher.settings.title"));
  box.tabIndex = -1;
  overlay.append(box);

  // ------------------------------------------------------------- drafts
  let page: "connection" | "preferences" = "connection";
  let provider: Provider = "unselected";
  let initialized = false;
  let saving = false;
  let listing = false;
  let restoring = false;
  /** providerGeneration: a provider switch or closing drops the answers of earlier requests. */
  let generation = 0;
  let closed = false;
  let chatBusy = false;
  let chatMessage = "";
  const draftModels = new Map<string, string>();
  const draftEfforts = new Map<string, string>();
  const modelsRequested = new Set<string>();
  let apiModels: string[] = [];

  // ------------------------------------------------------------- header, rail, footer
  const head = el("div", "ts-head");
  head.append(el("div", "ts-title", t("Teacher.settings.title")), el("div", "ts-subtitle", design("subtitle")));
  const main = el("div", "ts-main");
  const rail = el("div", "ts-rail");
  const navButton = (value: "connection" | "preferences", glyph: "link" | "sliders", text: string) => {
    const b = el("button", "ts-nav");
    b.type = "button";
    b.dataset.key = `commentary.settings.page.${value}`;
    b.append(icon(glyph), el("span", "", text));
    b.onclick = () => {
      page = value;
      render();
    };
    rail.append(b);
    return b;
  };
  const connectionNav = navButton("connection", "link", design("connection"));
  const preferencesNav = navButton("preferences", "sliders", design("preferences"));
  const pages = el("div", "ts-pages");
  main.append(rail, pages);
  const foot = el("div", "ts-foot");
  const status = el("div", "ts-status");
  status.dataset.key = "commentary.settings.status";
  status.setAttribute("role", "status");
  status.setAttribute("aria-label", t("Teacher.settings.title"));
  const cancel = btn(t("Teacher.settings.cancel"), "", "commentary.settings.cancel");
  const save = btn(design("save"), "primary", "commentary.settings.save");
  foot.append(status, cancel, save);
  box.append(head, main, foot);

  // ------------------------------------------------------------- 连接服务
  const connectionPage = el("div", "ts-page");
  connectionPage.append(el("div", "ts-heading", design("connection")), el("div", "ts-note", design("connectionHint")));
  const providerTabs = el("div", "ts-providers");
  const providerTab = (value: Provider, text: string, key: string) => {
    const b = el("button", "ts-provider", text);
    b.type = "button";
    b.dataset.key = key;
    b.onclick = () => chooseProvider(value);
    providerTabs.append(b);
    return b;
  };
  const chatTab = providerTab("chatGpt", t("Teacher.chatgpt.tab"), "commentary.settings.provider.chatgpt");
  const apiTab = providerTab("apiKey", t("Teacher.settings.apiKey"), "commentary.settings.provider.apikey");
  connectionPage.append(providerTabs);

  // UNSELECTED: the setup welcome.
  const welcome = el("div", "ts-body");
  const welcomeIntro = el("div", "ts-welcome");
  const welcomeText = el("div");
  welcomeText.append(el("div", "ts-welcome-title", design("setupTitle")), el("div", "ts-note", t("Teacher.chatgpt.choose")));
  welcomeIntro.append(icon("browser"), welcomeText);
  welcome.append(welcomeIntro, privacyRow());

  // API Key
  const api = el("div", "ts-body");
  const form = el("div", "ts-form");
  const field = (key: string, type: string, placeholder = "") => {
    const input = el("input", "ts-field");
    input.type = type;
    input.dataset.key = key;
    input.placeholder = placeholder;
    input.spellcheck = false;
    input.autocomplete = "off";
    return input;
  };
  const formRow = (text: string, control: HTMLElement, target: HTMLInputElement) => {
    const label = el("label", "", text);
    target.id ||= `ts-${target.dataset.key?.replace(/\W/g, "-")}`;
    label.htmlFor = target.id;
    form.append(label, control);
  };
  const baseUrl = field("commentary.settings.baseUrl", "text", t("Teacher.settings.addressExample"));
  formRow(t("Teacher.settings.baseUrl"), baseUrl, baseUrl);
  const keyRow = el("div", "ts-row");
  const apiKey = field("commentary.settings.apiKey", "password");
  apiKey.dataset.secret = "";
  const showKeyLabel = el("label", "ts-check");
  const showKey = el("input");
  showKey.type = "checkbox";
  showKey.dataset.key = "commentary.settings.showKey";
  showKeyLabel.append(showKey, document.createTextNode(t("Teacher.settings.showKey")));
  showKey.onchange = () => {
    apiKey.type = showKey.checked ? "text" : "password";
  };
  keyRow.append(apiKey, showKeyLabel);
  formRow(t("Teacher.settings.keyLabel"), keyRow, apiKey);
  // The editable model combo: typed text or one of the listed models.
  const modelRow = el("div", "ts-row");
  const combo = el("div", "ts-combo");
  const modelInput = field("commentary.settings.model", "text", t("Teacher.settings.modelExample"));
  modelInput.setAttribute("role", "combobox");
  modelInput.setAttribute("aria-expanded", "false");
  const comboToggle = el("button", "ts-combo-toggle", "▾");
  comboToggle.type = "button";
  comboToggle.tabIndex = -1;
  comboToggle.dataset.key = "commentary.settings.modelList";
  const comboList = el("ul", "ts-combo-list");
  comboList.setAttribute("role", "listbox");
  comboList.hidden = true;
  combo.append(modelInput, comboToggle, comboList);
  const showModelList = (open: boolean) => {
    comboList.hidden = !open || apiModels.length === 0;
    modelInput.setAttribute("aria-expanded", String(!comboList.hidden));
    if (comboList.hidden) return;
    comboList.replaceChildren(
      ...apiModels.map((m) => {
        const li = el("li", m === modelInput.value.trim() ? "active" : "", m);
        li.setAttribute("role", "option");
        li.addEventListener("mousedown", (e) => {
          e.preventDefault();
          modelInput.value = m;
          showModelList(false);
          host.changed();
        });
        return li;
      }),
    );
  };
  comboToggle.onclick = () => {
    if (modelInput.disabled) return;
    showModelList(comboList.hidden);
    modelInput.focus();
  };
  modelInput.addEventListener("blur", () => showModelList(false));
  modelInput.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" && e.altKey) {
      e.preventDefault();
      showModelList(true);
    }
  });
  const getModels = btn(t("Teacher.settings.refreshModels"), "", "commentary.settings.getModels");
  modelRow.append(combo, getModels);
  formRow(t("Teacher.settings.model"), modelRow, modelInput);
  const rememberLabel = el("label", "ts-check");
  const remember = el("input");
  remember.type = "checkbox";
  remember.dataset.key = "commentary.settings.remember";
  rememberLabel.append(remember, document.createTextNode(t("Teacher.settings.rememberKey")));
  form.append(el("span"), rememberLabel);
  api.append(el("div", "ts-note", t("Teacher.settings.apiHint")), form, el("div", "ts-note", t("Teacher.settings.privacy")), privacyRow());

  // ChatGPT 登录
  const chat = el("div", "ts-body");
  const chatWelcome = el("div", "ts-welcome");
  const chatWelcomeText = el("div");
  chatWelcomeText.append(el("div", "ts-welcome-title", design("chatTitle")), el("div", "ts-welcome-hint", design("chatHint")));
  chatWelcome.append(icon("browser"), chatWelcomeText);
  const chatFields = el("div", "ts-body");
  const accountSelect = select("commentary.settings.account", t("Teacher.chatgpt.account"));
  accountSelect.id = "ts-account";
  const accountLabel = el("label", "ts-label", t("Teacher.chatgpt.account"));
  accountLabel.htmlFor = accountSelect.id;
  const modelFields = el("div", "ts-body");
  const chatGrid = el("div", "ts-chat-grid");
  const chatModel = select("commentary.settings.chatModel", t("Teacher.chatgpt.model"));
  chatModel.id = "ts-chat-model";
  const chatModelLabel = el("label", "", t("Teacher.chatgpt.model"));
  chatModelLabel.htmlFor = chatModel.id;
  const refresh = btn(t("Teacher.chatgpt.refresh"), "", "commentary.settings.refreshModels");
  const chatModelRow = el("div", "ts-row");
  chatModelRow.append(chatModel, refresh);
  const effort = select("commentary.settings.effort", t("Teacher.chatgpt.reasoning"));
  effort.id = "ts-effort";
  const effortLabelEl = el("label", "", t("Teacher.chatgpt.reasoning"));
  effortLabelEl.htmlFor = effort.id;
  chatGrid.append(chatModelLabel, chatModelRow, effortLabelEl, effort);
  const effortHint = el("div", "ts-note");
  modelFields.append(chatGrid, effortHint);
  chatFields.append(accountLabel, accountSelect, modelFields);
  const signInRow = el("div", "ts-actions");
  const connect = btn(t("Teacher.chatgpt.connect"), "primary ts-connect", "commentary.settings.connect");
  const cancelSignIn = btn(t("Teacher.chatgpt.cancel"), "", "commentary.settings.cancelSignIn");
  // ChatGptSettingsPanel.retry: shown while saved credentials cannot be read; reloads the account.
  const retry = btn(t("Teacher.chatgpt.retry"), "", "commentary.settings.retry");
  signInRow.append(connect, cancelSignIn, retry);
  const chatActions = el("div", "ts-actions");
  const addAccount = btn(t("Teacher.chatgpt.add"), "", "commentary.settings.addAccount");
  const signOut = btn(t("Teacher.chatgpt.logout"), "", "commentary.settings.signOut");
  const manageUsage = btn(t("Teacher.chatgpt.usage"), "", "commentary.settings.usage");
  chatActions.append(addAccount, signOut, manageUsage);
  const planNotice = el("div", "ts-note", design("planNotice").replace(/\\n/g, "\n"));
  const chatStatus = el("div", "ts-note");
  chatStatus.dataset.key = "commentary.settings.chatStatus";
  chat.append(chatWelcome, chatFields, signInRow, chatActions, planNotice, chatStatus, privacyRow());
  connectionPage.append(welcome, api, chat);

  // ------------------------------------------------------------- 讲解偏好
  const preferencesPage = el("div", "ts-page");
  preferencesPage.append(el("div", "ts-heading", design("preferences")), el("div", "ts-note", design("preferencesHint")));
  const preference = (section: string | null, title: string, hint: string, key: string, options: string[]) => {
    if (section) preferencesPage.append(el("div", "ts-section", section));
    const row = el("div", "ts-pref");
    const text = el("div", "ts-pref-text");
    const s = select(key, title);
    s.setAttribute("aria-description", hint);
    setOptions(s, options.map((o, k) => [String(k), o]));
    text.append(el("div", "ts-pref-title", title), el("div", "ts-note", hint));
    row.append(text, s);
    preferencesPage.append(row);
    return s;
  };
  const level = preference(design("personal"), design("level"), design("levelHint"), "commentary.settings.level",
    RANKS.map((r) => format(design(r.mode === "d" ? "rankDan" : "rankKyu"), r.num)));
  const style = preference(null, t("Teacher.settings.style"), design("styleHint"), "commentary.settings.style",
    ["balanced", "rigorous", "patient", "strict", "humorous"].map((k) => t(`Teacher.settings.style.${k}`)));
  const density = preference(design("details"), t("Teacher.settings.terminology"), design("termsHint"), "commentary.settings.density",
    ["low", "medium", "high"].map((k) => t(`Teacher.settings.density.${k}`)));
  const pace = preference(null, t("Teacher.settings.pace"), design("paceHint"), "commentary.settings.pace",
    ["brief", "standard", "detailed"].map((k) => t(`Teacher.settings.pace.${k}`)));
  const variation = preference(null, t("Teacher.settings.variation"), design("variationHint"), "commentary.settings.variation",
    ["few", "moderate", "many"].map((k) => t(`Teacher.settings.variation.${k}`)));
  const reload = el("button", "ts-link");
  reload.type = "button";
  reload.dataset.key = "commentary.settings.reloadKnowledge";
  reload.append(icon("book"), el("span", "", t("Teacher.settings.reloadKnowledge")));
  preferencesPage.append(reload);
  pages.append(connectionPage, preferencesPage);

  document.body.append(overlay);

  // ------------------------------------------------------------- state helpers
  const setStatus = (text: string) => {
    status.textContent = text || " ";
    host.changed();
  };
  const facts = () => host.view()?.facts ?? null;
  const activeAccount = (): CommentaryAccount | null => {
    const f = facts();
    return f?.chatgpt.accounts.find((a) => a.id === f.chatgpt.active) ?? null;
  };
  const chosenModel = (a: CommentaryAccount) => draftModels.get(a.id) ?? a.model;
  /** The selected ChatGPT model, when the server still lists it. */
  const selectedChatModel = (): CommentaryModel | null => {
    const a = activeAccount();
    return a ? (facts()?.chatgpt.models.find((m) => m.slug === chosenModel(a)) ?? null) : null;
  };
  const chosenEffort = (a: CommentaryAccount, slug: string) => draftEfforts.get(`${a.id}:${slug}`) ?? (slug === a.model ? (a.effort ?? "") : "");
  const preferences = (): Preferences => {
    const rank = RANKS[Number(level.value)] ?? { mode: "k", num: 5 };
    return {
      rankMode: rank.mode,
      rankNum: rank.num,
      styleIndex: Number(style.value),
      densityIndex: Number(density.value),
      paceIndex: Number(pace.value),
      variationIndex: Number(variation.value),
    };
  };

  /** Fills the access key with the key this session holds (Java's loaded password field), only
   *  while the field is empty and the address is still the saved one. */
  const restoreKey = async (openTab: boolean) => {
    const request = generation;
    const savedAddress = host.view()?.connection.baseUrl ?? "";
    restoring = true;
    render();
    if (openTab) {
      setStatus(t("Teacher.status.loadingSettings"));
      const r = await connection({ type: "OpenApiKeyTab" });
      if (closed || request !== generation) return;
      if (!r.ok) {
        restoring = false;
        setStatus(failureText(r.message));
        render();
        return;
      }
    }
    let key = "";
    try {
      key = await invoke<string>("commentary_api_key_draft");
    } catch {
      key = "";
    }
    if (closed || request !== generation) return;
    restoring = false;
    if (!apiKey.value && baseUrl.value === savedAddress) apiKey.value = key;
    if (openTab) setStatus("");
    render();
  };

  /** loadValues: the drafts start from the saved settings once they are loaded. */
  const initialize = () => {
    const v = host.view();
    if (!v || !v.settings_loaded || initialized) return;
    initialized = true;
    const c = v.connection;
    provider = c.provider;
    baseUrl.value = c.baseUrl;
    modelInput.value = c.model;
    apiModels = c.model.trim() ? [c.model] : [];
    // Without a credential store the key can only be session-only. Java left a saved choice checked
    // but disabled, so every save failed; this client shows the choice it can actually honour.
    remember.checked = c.rememberApiKey && v.facts.secure_storage.available;
    const p = v.preferences;
    const rank = RANKS.findIndex((r) => r.mode === (p.rankMode === "d" ? "d" : "k") && r.num === p.rankNum);
    level.value = String(rank < 0 ? 13 : rank);
    const clamp = (value: number, max: number) => String(Math.max(0, Math.min(max, value)));
    style.value = clamp(p.styleIndex, 4);
    density.value = clamp(p.densityIndex, 2);
    pace.value = clamp(p.paceIndex, 2);
    variation.value = clamp(p.variationIndex, 2);
    setStatus(v.settings_usable ? (provider === "unselected" ? t("Teacher.chatgpt.choose") : "") : t("Teacher.error.generic"));
    if (provider === "apiKey") void restoreKey(true);
    else void restoreKey(false);
  };

  // ------------------------------------------------------------- actions
  function chooseProvider(value: Provider): void {
    if (!initialized || saving || value === provider) return;
    generation++;
    // The other tab's operations lose ownership; their late completions are ignored, so their busy
    // state must not outlive the switch (TeacherSettingsDialog suspends the outgoing panel).
    listing = false;
    restoring = false;
    chatBusy = false;
    chatMessage = "";
    provider = value;
    setStatus("");
    render();
    if (value === "apiKey") void restoreKey(true);
  }

  getModels.onclick = async () => {
    const problem = addressProblem(baseUrl.value);
    if (problem) {
      setStatus(problem);
      baseUrl.focus();
      return;
    }
    if (!apiKey.value) {
      setStatus(t("Teacher.settings.enterKey"));
      return;
    }
    const request = generation;
    listing = true;
    setStatus(t("Teacher.settings.loadingModels"));
    render();
    const r = await connection({ type: "ListApiModels", base_url: baseUrl.value, api_key: apiKey.value });
    if (closed || request !== generation) return;
    listing = false;
    if (r.ok && r.result.type === "CommentaryModels") {
      // The typed or chosen model stays; a list never picks one by itself.
      apiModels = r.result.models;
      setStatus(format(t("Teacher.settings.modelsLoaded"), apiModels.length));
    } else setStatus(r.ok ? t("Teacher.error.generic") : failureText(r.message));
    render();
  };

  reload.onclick = async () => {
    const r = await send({ type: "Commentary", op: { type: "ReloadKnowledge" } });
    if (!closed) setStatus(r.ok ? t("Teacher.settings.knowledgeReloaded") : failureText(r.message));
  };

  /** ChatGptSettingsPanel.work: one account operation at a time; its failure shows in the panel. */
  const chatWork = async (op: ConnectionOp) => {
    const request = generation;
    chatBusy = true;
    chatMessage = "";
    render();
    const r = await connection(op);
    if (closed || request !== generation) return;
    chatBusy = false;
    if (!r.ok) chatMessage = chatError(r.message);
    render();
  };
  // ChatGptSettingsPanel: 使用 ChatGPT 继续 signs in the selected account again, 添加账号 a new one.
  connect.onclick = () => void chatWork({ type: "ChatGptSignIn", account: activeAccount()?.id ?? null });
  addAccount.onclick = () => void chatWork({ type: "ChatGptSignIn", account: null });
  cancelSignIn.onclick = () => void chatWork({ type: "ChatGptCancelSignIn" });
  retry.onclick = () => {
    const a = activeAccount();
    if (a) void chatWork({ type: "ChatGptSelectAccount", account: a.id });
  };
  signOut.onclick = () => {
    const a = activeAccount();
    if (a) void chatWork({ type: "ChatGptSignOut", account: a.id });
  };
  refresh.onclick = () => void chatWork({ type: "ChatGptRefreshModels" });
  manageUsage.onclick = () => void chatWork({ type: "OpenUsage" });
  accountSelect.onchange = () => {
    if (accountSelect.value) void chatWork({ type: "ChatGptSelectAccount", account: accountSelect.value });
  };
  chatModel.onchange = () => {
    const a = activeAccount();
    if (a && chatModel.value) draftModels.set(a.id, chatModel.value);
    render();
  };
  effort.onchange = () => {
    const a = activeAccount();
    const m = selectedChatModel();
    if (a && m) draftEfforts.set(`${a.id}:${m.slug}`, effort.value);
    host.changed();
  };

  const chatReady = () => {
    const a = activeAccount();
    return !chatBusy && !facts()?.chatgpt.signing_in && !!a && a.signed_in && a.authorized && !!selectedChatModel();
  };

  const finishSave = (message: string) => {
    saving = false;
    setStatus(message);
    render();
  };

  /** save(): preferences first, then the provider's connection, then the provider choice (not
   *  transactional, as in Java). 保存偏好 saves only the preferences and stays open. */
  save.onclick = async () => {
    if (saving || !initialized) return;
    const preferencesOnly = page === "preferences";
    if (!preferencesOnly && (provider === "unselected" || (provider === "chatGpt" && !chatReady()))) {
      setStatus(t("Teacher.chatgpt.choose"));
      return;
    }
    if (!preferencesOnly && provider === "apiKey") {
      const address = addressProblem(baseUrl.value);
      if (address) {
        setStatus(address);
        baseUrl.focus();
        return;
      }
      const invalidModel = modelProblem(modelInput.value);
      if (invalidModel) {
        setStatus(invalidModel);
        modelInput.focus();
        return;
      }
    }
    const a = activeAccount();
    const m = selectedChatModel();
    const chosen = { base_url: baseUrl.value, model: modelInput.value.trim(), api_key: apiKey.value, remember: remember.checked };
    const chatChoice = a && m ? { account: a.id, model: m.slug, effort: chosenEffort(a, m.slug) || null } : null;
    saving = true;
    setStatus(t("Teacher.settings.saving"));
    render();
    const prefs = await send({ type: "Commentary", op: { type: "SavePreferences", preferences: preferences() } });
    if (closed) return;
    if (!prefs.ok) return finishSave(failureText(prefs.message));
    if (preferencesOnly) return finishSave(design("preferencesSaved"));
    const saved =
      provider === "apiKey"
        ? await connection({ type: "SaveApiKeyConnection", ...chosen })
        : await connection({ type: "SaveChatGptConnection", account: chatChoice!.account, model: chatChoice!.model, effort: chatChoice!.effort });
    if (closed) return;
    if (!saved.ok) return finishSave(provider === "chatGpt" ? chatError(saved.message) : failureText(saved.message));
    const selected = await connection({ type: "SelectProvider", provider });
    if (closed) return;
    if (!selected.ok) return finishSave(failureText(selected.message));
    saving = false;
    close();
  };

  function close(): void {
    if (closed) return;
    closed = true;
    generation++;
    // Leaving the dialog ends a sign-in that is still waiting (ChatGptSettingsPanel.suspend).
    if (facts()?.chatgpt.signing_in) void connection({ type: "ChatGptCancelSignIn" });
    apiKey.value = "";
    overlay.remove();
    windowRoot?.removeAttribute("inert");
    if (previous?.isConnected) previous.focus();
    host.onClose();
  }
  cancel.onclick = () => {
    if (!saving) close();
  };

  box.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      if (!comboList.hidden) showModelList(false);
      else if (!saving) close();
    } else if (e.key === "Enter" && !e.isComposing && e.target instanceof HTMLInputElement && e.target.type !== "checkbox") {
      // getRootPane().setDefaultButton(saveButton)
      e.preventDefault();
      if (!save.disabled) save.click();
    } else if (e.key === "Tab") {
      const items = [...box.querySelectorAll<HTMLElement>("button:not(:disabled):not([tabindex='-1']), input:not(:disabled), select:not(:disabled)")].filter(
        (x) => x.getClientRects().length > 0,
      );
      const at = items.indexOf(document.activeElement as HTMLElement);
      e.preventDefault();
      items[(at + (e.shiftKey ? -1 : 1) + items.length) % items.length]?.focus();
    }
  });

  // ------------------------------------------------------------- render
  // ChatGptSettingsPanel.java:295-308: the first authorized use shows a modal welcome once per
  // account (titled 正在使用 ChatGPT 套餐); the adapter persists `welcomed` and reports it once.
  const welcomed = new Set<string>();
  function welcomeOnce(statusKey: string | undefined, account: string | undefined): void {
    if (statusKey !== "Teacher.chatgpt.welcome" || !account || welcomed.has(account)) return;
    welcomed.add(account);
    const layer = el("div", "ts-overlay ts-notice-layer");
    const card = el("div", "ts-notice");
    card.setAttribute("role", "alertdialog");
    card.setAttribute("aria-modal", "true");
    card.setAttribute("aria-label", t("Teacher.chatgpt.usingPlan"));
    const ok = el("button", "tc-btn primary", product("ok"));
    const close = () => {
      layer.remove();
      box.focus();
    };
    ok.onclick = close;
    layer.onkeydown = (e) => {
      if (e.key === "Escape" || e.key === "Enter") {
        e.preventDefault();
        e.stopPropagation();
        close();
      }
    };
    card.append(el("div", "ts-notice-title", t("Teacher.chatgpt.usingPlan")), el("div", "ts-notice-text", t("Teacher.chatgpt.welcome")), ok);
    layer.append(card);
    document.body.append(layer);
    ok.focus();
  }

  function renderChat(): void {
    const f = facts();
    const g = f?.chatgpt;
    const accounts = g?.accounts ?? [];
    const a = activeAccount();
    const signedIn = !!a?.signed_in;
    const authorized = signedIn && !!a?.authorized;
    const unavailable = !!a?.credentials_unavailable;
    const signingIn = !!g?.signing_in;
    const busy = chatBusy || signingIn || saving;
    welcomeOnce(g?.status_key, a?.id);
    chatWelcome.hidden = signedIn || accounts.length > 0;
    chatFields.hidden = accounts.length === 0;
    modelFields.hidden = !authorized;
    chatActions.hidden = !signedIn;
    const offerSignIn = !unavailable && !authorized;
    planNotice.hidden = !offerSignIn;
    connect.hidden = !offerSignIn;
    cancelSignIn.hidden = !signingIn;
    retry.hidden = !unavailable;
    signInRow.hidden = connect.hidden && cancelSignIn.hidden && retry.hidden;
    connect.disabled = busy || !offerSignIn;
    addAccount.disabled = busy;
    signOut.disabled = busy || !signedIn;
    manageUsage.disabled = busy;
    refresh.disabled = busy || !authorized;
    accountSelect.disabled = busy;
    cancelSignIn.disabled = false;
    retry.disabled = busy;

    setOptions(accountSelect, [...(a ? [] : [["", ""] as [string, string]]), ...accounts.map((x): [string, string] => [x.id, `${x.email} (${x.id.slice(0, 8)})`])]);
    accountSelect.value = a?.id ?? "";

    const models = g?.models ?? [];
    const chosen = a ? chosenModel(a) : "";
    const found = models.some((m) => m.slug === chosen);
    setOptions(chatModel, [...(found ? [] : [["", ""] as [string, string]]), ...models.map((m): [string, string] => [m.slug, m.display_name || m.slug])]);
    chatModel.value = found ? chosen : "";
    chatModel.disabled = busy || !authorized;
    const m = found ? (models.find((x) => x.slug === chosen) ?? null) : null;
    setOptions(effort, [["", effortLabel("", m?.default_effort ?? null)], ...(m?.efforts ?? []).map((x): [string, string] => [x, effortLabel(x, null)])]);
    effort.value = a && m && m.efforts.includes(chosenEffort(a, m.slug)) ? chosenEffort(a, m.slug) : "";
    const adjustable = !!m && m.efforts.length > 0;
    effort.disabled = busy || !authorized || !adjustable;
    effortHint.textContent = adjustable ? t("Teacher.chatgpt.reasoning.hint") : t("Teacher.chatgpt.reasoning.unavailable");
    effort.setAttribute("aria-description", effortHint.textContent);

    // Models load when an authorized account is shown (showAccount → refreshModels).
    if (provider === "chatGpt" && authorized && a && models.length === 0 && !busy && !modelsRequested.has(a.id)) {
      modelsRequested.add(a.id);
      void chatWork({ type: "ChatGptRefreshModels" });
    }

    // updateStatus, a failure of the last operation, or the adapter's own message.
    let text = "";
    if (busy && !saving) text = t("Teacher.chatgpt.working");
    else if (chatMessage) text = chatMessage;
    else if (g?.status_key) text = keyOr(g.status_key);
    else if (authorized && chosen && models.length > 0 && !found) text = t("Teacher.chatgpt.error.modelRemoved");
    else if (unavailable) text = t("Teacher.chatgpt.error.credentialsUnavailable");
    else if (!signedIn) text = accounts.length ? t("Teacher.chatgpt.notConnected") : "";
    else if (!a?.authorized) text = t("Teacher.chatgpt.error.permission");
    else if (a.session_only) text = t("Teacher.chatgpt.sessionOnly");
    else text = t("Teacher.chatgpt.usingPlan");
    chatStatus.textContent = text;
    chatStatus.hidden = !text;
  }

  function render(): void {
    if (closed) return;
    initialize();
    const v = host.view();
    const loaded = initialized;
    const locked = !loaded || saving;
    if (!loaded) status.textContent = t("Teacher.status.loadingSettings");
    connectionNav.classList.toggle("selected", page === "connection");
    preferencesNav.classList.toggle("selected", page === "preferences");
    connectionNav.disabled = preferencesNav.disabled = locked;
    connectionPage.hidden = page !== "connection";
    preferencesPage.hidden = page !== "preferences";
    chatTab.classList.toggle("selected", provider === "chatGpt");
    apiTab.classList.toggle("selected", provider === "apiKey");
    chatTab.disabled = apiTab.disabled = locked;
    welcome.hidden = provider !== "unselected";
    api.hidden = provider !== "apiKey";
    chat.hidden = provider !== "chatGpt";

    for (const input of [baseUrl, modelInput, showKey]) input.disabled = locked;
    apiKey.disabled = locked || restoring;
    getModels.disabled = locked || restoring || listing;
    const storage = v?.facts.secure_storage.available ?? false;
    remember.disabled = locked || !storage;
    rememberLabel.title = storage ? "" : t("Teacher.settings.storageUnavailable");
    for (const s of [level, style, density, pace, variation]) s.disabled = locked;
    reload.disabled = locked;
    renderChat();

    save.textContent = page === "preferences" ? design("savePreferences") : design("save");
    save.setAttribute("aria-description", save.textContent);
    save.disabled = locked || (page === "connection" && provider === "apiKey" && restoring);
    cancel.disabled = saving;
    host.changed();
  }

  render();
  box.focus();

  return {
    refresh: render,
    targets: () => [...box.querySelectorAll<HTMLElement>("[data-key]")],
    report: () => {
      const a = activeAccount();
      return {
        page,
        provider,
        status: status.textContent ?? "",
        baseUrl: baseUrl.value,
        keyFilled: apiKey.value.length > 0,
        keyShown: showKey.checked,
        model: modelInput.value,
        models: apiModels,
        remember: remember.checked,
        rememberEnabled: !remember.disabled,
        account: a ? `${a.email} (${a.id.slice(0, 8)})` : "",
        accounts: [...accountSelect.options].filter((o) => o.value).map((o) => o.text),
        chatModel: chatModel.value,
        chatModels: [...chatModel.options].filter((o) => o.value).map((o) => o.text),
        effort: effort.selectedOptions[0]?.text ?? "",
        efforts: [...effort.options].map((o) => o.text),
        chatStatus: chatStatus.hidden ? "" : (chatStatus.textContent ?? ""),
        level: level.selectedOptions[0]?.text ?? "",
        style: style.selectedOptions[0]?.text ?? "",
        density: density.selectedOptions[0]?.text ?? "",
        pace: pace.selectedOptions[0]?.text ?? "",
        variation: variation.selectedOptions[0]?.text ?? "",
        saveLabel: save.textContent ?? "",
        saveEnabled: !save.disabled,
      };
    },
  };
}
