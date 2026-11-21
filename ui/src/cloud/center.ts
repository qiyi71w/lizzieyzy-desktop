// 远程算力 (Java RemoteComputeDialog): a non-modal window with the 智子云算力 and 自建算力 pages,
// each a sign-in/link column beside a 一键启用 column, and the current-engine/status footer.
// Rust owns the account session, credentials, settings and the engine; this window holds only
// unsaved drafts (fields, plan/model choice, link) and presentation state. A reply that arrives
// after the window closed, or after a newer request of the same kind, is dropped.
import { registerViewHandler, state } from "../app";
import type { CloudCatalog, CloudCommand, Command, CloudFailure, CloudReply, CloudSettings, CloudState, EngineFailureView, EngineProfile, Overview, WorkbenchView } from "../contract/generated";
import { message } from "../dialogs";
import { format, t } from "../i18n";
import { openAccountDetails } from "./details";
import { openPayment } from "./payment";
import {
  account as accountRequest, closeOwner, cloudCommand, cloudStartText, confirmedWeight, customDisplayName, DEFAULT_WEIGHT, duration, failureText, kataWeight,
  keyedDetail, localTime, money, normalizeLink, PRESETS, resolveWeight, samePlan, selectableWeight, validLink, weightLabel, weightName,
  requestOwner, withKataWeight, zhiziDisplayName, type AccountOutcome,
} from "./service";
import { icon } from "../design/icons";
import { button, el, openCloudWindow } from "./window";
import "./cloud.css";

type Page = "zhizi" | "custom";
type Provider = "zhizi" | "custom" | "local";
/** RemoteComputeConfig.ZHIZI_CATALOG_REFRESH_INTERVAL_MILLIS. */
const CATALOG_REFRESH_MS = 6 * 60 * 60 * 1000;

interface Center {
  focus(): void;
  onView(view: WorkbenchView): void;
}

let instance: Center | null = null;
registerViewHandler((view) => instance?.onView(view));

/** Menu 设置 > 远程算力, the bottom toolbar 远程算力 button and the setup window's 远程算力 entry. */
export function openRemoteCompute(): void {
  if (instance) { instance.focus(); return; }
  const view = state.view;
  if (!view) return;
  instance = createCenter(view);
}

function profileOf(view: WorkbenchView): EngineProfile | null {
  const fg = view.engine.foreground;
  return fg ? view.settings.engine.engines[fg.index] ?? null : null;
}

/** The foreground engine's provider (RemoteComputeDialog.activeProviderForStatus). */
function foregroundProvider(view: WorkbenchView): Provider | null {
  if (!view.engine.foreground) return null;
  const cloud = profileOf(view)?.cloud;
  return cloud?.provider === "Zhizi" ? "zhizi" : cloud?.provider === "Custom" ? "custom" : "local";
}

/** The launch an engine switch asked for, as the core stores it on the seated profile. */
type Launch = { provider: "zhizi"; args: string } | { provider: "custom"; url: string } | { provider: "local" };

function launches(profile: EngineProfile, launch: Launch): boolean {
  const cloud = profile.cloud;
  if (launch.provider === "zhizi") return cloud?.provider === "Zhizi" && cloud.args === launch.args;
  if (launch.provider === "custom") return cloud?.provider === "Custom" && cloud.url === launch.url;
  return !cloud;
}

/** switchPrimaryEngine's verdict on one view: `true` once the seated profile `profile` is ready
 *  with the requested launch, `false` once the switch ended otherwise — the seat's engine failed,
 *  the seat was emptied (关闭所有引擎 during the start) or holds another engine, or another
 *  engine (or a newer launch of the same profile) is ready in it — and `null` while the requested
 *  engine is still starting (monitorEngineSwitch: a token change or the FAILED phase ends it).
 *  Only views built after the switch committed (`seq > after`) count: a failure shown before it
 *  never decides, and the final view decides alone, whatever intermediate views were coalesced
 *  away. */
function switchOutcome(view: WorkbenchView, profile: string, after: number, launch: Launch): boolean | null {
  if (view.seq <= after) return null;
  if (view.engine.down) return false;
  const seated = profileOf(view);
  if (!seated || seated.id !== profile) return false;
  if (!view.engine.foreground?.ready) return null;
  return launches(seated, launch);
}

/** A label and its control (fieldRow). */
function fieldRow(label: string, control: HTMLElement, input: HTMLInputElement): HTMLElement {
  const row = el("div", "rc-field");
  const caption = el("label", "rc-field-label", label);
  input.id = `rc-${input.dataset.key}`;
  caption.htmlFor = input.id;
  row.append(caption, control);
  return row;
}

function textInput(key: string, placeholder: string, type: "text" | "password" = "text"): HTMLInputElement {
  const input = el("input", "rc-input");
  input.type = type;
  input.placeholder = placeholder;
  input.dataset.key = key;
  input.autocomplete = "off";
  input.spellcheck = false;
  return input;
}

/** MemoryCheckBox: a check box with its tooltip. */
function memoryChip(label: string, key: string, tip: string): { chip: HTMLLabelElement; input: HTMLInputElement } {
  const chip = el("label", "rc-check");
  const input = el("input");
  input.type = "checkbox";
  input.dataset.key = key;
  chip.title = tip;
  chip.append(input, label);
  return { chip, input };
}

function createCenter(initial: WorkbenchView): Center {
  const title = t("RemoteCompute.title");
  let alive = true;
  const win = openCloudWindow(title, "remote-compute", "remote-compute", () => {
    alive = false;
    instance = null;
    clearInterval(cooldownTimer);
    closeOwner(owner);
    closeOwner(writes);
    monitor = null;
    // Field secrets end with the window.
    password.value = newPassword.value = confirmPassword.value = code.value = "";
  });

  // ---- presentation state -------------------------------------------------------------------
  let saved: CloudSettings = initial.settings.cloud;
  let lastViewCloud = JSON.stringify(saved);
  let account: CloudState | null = null;
  let page: Page = saved.provider === "custom" ? "custom" : "zhizi";
  let codeMode = false, resetMode = false, busy = false, passwordShown = false;
  let overview: Overview | null = null, overviewLoading = false, catalogRefreshing = false;
  let cooldown = 0, cooldownTimer: number | undefined;
  let statusText = t("RemoteCompute.chooseHint"), statusOk: boolean | null = null, dotOk = true;
  /** Java dispose cancels the overview and catalog workers; runBackground sign-in work and the
   *  account choices (logout, remember) finish after the window closes, their replies dropped. */
  const owner = requestOwner();
  const writes = requestOwner(false);
  let overviewSeq = 0, catalogSeq = 0, operationSeq = 0;
  let weightOptions: { name: string; confirmed: boolean }[] = [];
  /** The engine switch in progress (switchPrimaryEngine's monitor), armed before its command is
   *  sent; `receipt` is the seat and view boundary the committed command reported, `seen` each
   *  profile's latest foreground engine failure the views showed meanwhile (a failure is in one
   *  view only, which may come before the receipt). */
  let monitor: {
    launch: Launch; success: string; failure: string; receipt: { profile: string; after: number } | null;
    seen: Map<string, { seq: number; failure: EngineFailureView }>;
  } | null = null;

  const request = (op: CloudCommand): Promise<AccountOutcome | null> => accountRequest(op, owner);
  const write = (op: CloudCommand): Promise<AccountOutcome | null> => accountRequest(op, writes);
  const applyReply = (reply: CloudReply) => { saved = reply.settings; account = reply.state; };
  const authenticated = () => !!account?.authenticated;
  const status = (text: string, ok: boolean) => { statusText = text; statusOk = ok; dotOk = ok; };

  // ---- header: the two pages as text segments -------------------------------------------------
  const header = el("div", "rc-header");
  const tabs = el("div", "rc-tabs");
  const zhiziTab = button(t("RemoteCompute.zhizi"), "rc-tab", "remote.tab.zhizi", () => { page = "zhizi"; update(); });
  const customTab = button(t("RemoteCompute.custom"), "rc-tab", "remote.tab.custom", () => { page = "custom"; update(); });
  tabs.append(zhiziTab, customTab);
  header.append(tabs);

  // ---- 智子云算力: sign-in ----------------------------------------------------------------------
  const loginCard = el("div", "rc-section");
  loginCard.append(el("h3", "rc-section-title", t("RemoteCompute.zhizi")), el("p", "rc-section-subtitle", t("RemoteCompute.zhiziDescription")));
  const loginForm = el("div", "rc-login");
  const segments = el("div", "rc-tabs rc-segments");
  const passwordModeButton = button(t("RemoteCompute.passwordLogin"), "rc-segment", "remote.mode.password", () => { resetMode = false; codeMode = false; update(); });
  const codeModeButton = button(t("RemoteCompute.codeLogin"), "rc-segment", "remote.mode.code", () => { resetMode = false; codeMode = true; update(); });
  segments.append(passwordModeButton, codeModeButton);
  const identifier = textInput("remote.account", t("RemoteCompute.accountPlaceholder"));
  identifier.value = saved.identifier;
  const password = textInput("remote.password", t("RemoteCompute.passwordPlaceholder"), "password");
  const eye = el("button", "rc-eye");
  eye.type = "button";
  eye.dataset.key = "remote.showPassword";
  eye.append(icon("hawkeye"));
  eye.onclick = () => { passwordShown = !passwordShown; update(); };
  const passwordShell = el("div", "rc-password-shell");
  password.className = "rc-password-input";
  passwordShell.append(password, eye);
  const passwordRow = fieldRow(t("RemoteCompute.password"), passwordShell, password);
  const newPassword = textInput("remote.newPassword", t("RemoteCompute.newPasswordPlaceholder"), "password");
  const newPasswordRow = fieldRow(t("RemoteCompute.newPassword"), newPassword, newPassword);
  const confirmPassword = textInput("remote.confirmPassword", t("RemoteCompute.confirmPasswordPlaceholder"), "password");
  const confirmPasswordRow = fieldRow(t("RemoteCompute.confirmPassword"), confirmPassword, confirmPassword);
  const code = textInput("remote.code", t("RemoteCompute.codePlaceholder"));
  const sendCodeButton = button(t("RemoteCompute.sendCode"), "", "remote.sendCode", () => void sendCode());
  const codeControl = el("div", "rc-code");
  codeControl.append(code, sendCodeButton);
  const codeRow = fieldRow(t("RemoteCompute.code"), codeControl, code);
  const fastLoginHint = el("p", "rc-hint", t("RemoteCompute.fastLoginCreatesAccount"));
  const rememberToken = memoryChip(t("RemoteCompute.rememberLogin"), "remote.rememberLogin", t("RemoteCompute.rememberLoginTip"));
  const rememberPassword = memoryChip(t("RemoteCompute.rememberPassword"), "remote.rememberPassword", t("RemoteCompute.rememberPasswordTip"));
  rememberToken.input.checked = saved.rememberToken;
  rememberPassword.input.checked = saved.rememberPassword;
  rememberToken.input.onchange = rememberPassword.input.onchange = () => update();
  rememberPassword.input.addEventListener("change", () => { if (!rememberPassword.input.checked) void forgetSavedPassword(); });
  const forgotButton = button(t("RemoteCompute.forgotPassword"), "rc-link", "remote.forgotPassword", () => {
    resetMode = !resetMode; codeMode = false;
    code.value = newPassword.value = confirmPassword.value = "";
    update();
  });
  const options = el("div", "rc-options");
  const chips = el("div", "rc-chips");
  chips.append(rememberToken.chip, rememberPassword.chip);
  options.append(chips, forgotButton);
  const loginButton = button(t("RemoteCompute.login"), "rc-login-button", "remote.login", () => {
    if (resetMode) void resetPassword(); else if (codeMode) void loginWithCode(); else void loginWithPassword();
  });
  loginForm.append(segments, fieldRow(t("RemoteCompute.account"), identifier, identifier), passwordRow, newPasswordRow, confirmPasswordRow, codeRow, fastLoginHint, options, loginButton);

  const signedIn = el("div", "rc-signed-in");
  const accountText = el("p", "rc-account-text");
  const membershipLabel = el("div", "rc-account-value"), balanceLabel = el("div", "rc-balance");
  const activityLabel = el("div", "rc-account-small"), updatedLabel = el("div", "rc-account-updated");
  // The account's refresh as its icon; update() names it 刷新 or 刷新中 in its tooltip.
  const refreshAccountButton = button("", "rc-icon-button", "remote.account.refresh", () => void refreshOverview(true));
  refreshAccountButton.append(icon("refresh"));
  const usageButton = button(t("RemoteCompute.account.usageDetails"), "rc-link", "remote.account.usage", () => showDetails(0));
  const creditButton = button(t("RemoteCompute.account.creditDetails"), "rc-link", "remote.account.credit", () => showDetails(1));
  const accountActions = el("div", "rc-account-actions");
  accountActions.append(refreshAccountButton, usageButton, creditButton);
  const paymentButton = button(t("RemoteCompute.payment.open"), "", "remote.payment", () => showPayment());
  paymentButton.title = t("RemoteCompute.payment.openDescription");
  const logoutButton = button(t("RemoteCompute.changeAccount"), "", "remote.logout", () => void logout());
  const accountButtons = el("div", "rc-actions");
  accountButtons.append(paymentButton, logoutButton);
  signedIn.append(el("div", "rc-signed-in-title", t("RemoteCompute.loggedIn")), accountText, membershipLabel, balanceLabel, activityLabel, updatedLabel, accountActions, accountButtons);
  loginCard.append(loginForm, signedIn);

  // ---- 智子云算力: 一键启用 ---------------------------------------------------------------------
  const zhiziAction = el("div", "rc-section");
  const plan = el("div", "rc-plan");
  const presetSelect = el("select");
  presetSelect.id = "rc-preset";
  presetSelect.dataset.key = "remote.preset";
  presetSelect.title = t("RemoteCompute.connectionModeDescription");
  PRESETS.forEach((preset, i) => presetSelect.append(new Option(preset.label(), String(i))));
  const weightSelect = el("select");
  weightSelect.id = "rc-weight";
  weightSelect.dataset.key = "remote.weight";
  // The list's refresh, an icon beside it; its tooltip names the list.
  const refreshWeightsButton = button("", "rc-icon-button", "remote.refreshWeights", () => void refreshCatalog(true));
  refreshWeightsButton.append(icon("refresh"));
  refreshWeightsButton.title = t("RemoteCompute.refreshWeightsTip");
  refreshWeightsButton.setAttribute("aria-label", t("RemoteCompute.refreshWeights"));
  const officialButton = button(t("RemoteCompute.weightUseOfficial"), "rc-link rc-official", "remote.officialWeight", () => {
    if (!weightOptions.some((w) => w.name.toLowerCase() === DEFAULT_WEIGHT)) return;
    weightSelect.value = weightOptions.find((w) => w.name.toLowerCase() === DEFAULT_WEIGHT)!.name;
    status(t("RemoteCompute.status.weightOfficialSelected"), true);
    update();
  });
  officialButton.title = t("RemoteCompute.weightUseOfficialDescription");
  const presetLabel = el("label", "", t("RemoteCompute.connectionMode"));
  presetLabel.htmlFor = presetSelect.id;
  const weightLabelNode = el("label", "", t("RemoteCompute.weightModel"));
  weightLabelNode.htmlFor = weightSelect.id;
  weightSelect.setAttribute("aria-description", t("RemoteCompute.weightModelDescription"));
  const weightRow = el("div", "rc-weight-row");
  weightRow.append(weightSelect, refreshWeightsButton);
  plan.append(presetLabel, presetSelect, weightLabelNode, weightRow, officialButton);
  presetSelect.onchange = weightSelect.onchange = () => update();
  const useZhiziButton = button(t("RemoteCompute.enableZhizi"), "", "remote.useZhizi", () => void useZhizi());
  const localFromZhizi = button(t("RemoteCompute.backToLocal"), "", "remote.local.zhizi", () => void useLocal());
  const zhiziButtons = el("div", "rc-actions");
  zhiziButtons.append(useZhiziButton, localFromZhizi);
  const website = el("div", "rc-note");
  const websiteCopy = el("div", "rc-note-copy");
  websiteCopy.append(el("strong", "", t("RemoteCompute.websiteTitle")), el("span", "rc-website-url", "www.zhizigo.cn"));
  const websiteButton = button(t("RemoteCompute.openWebsite"), "", "remote.website", () => void openLink(false));
  website.append(websiteCopy, websiteButton);
  zhiziAction.append(el("h3", "rc-section-title", t("RemoteCompute.oneClickEnable")), plan, zhiziButtons, website);
  const zhiziPage = el("div", "rc-page");
  zhiziPage.append(loginCard, zhiziAction);

  // ---- 自建算力 -------------------------------------------------------------------------------
  const customCard = el("div", "rc-section");
  const link = textInput("remote.link", t("RemoteCompute.linkPlaceholder"));
  link.value = saved.customUrl;
  link.oninput = () => update();
  const importRow = el("div", "rc-import-row");
  const importQrButton = button(t("RemoteCompute.importQr"), "", "remote.importQr", () => void importQr());
  importRow.append(importQrButton, el("span", "rc-import-hint", t("RemoteCompute.qrHint")));
  const info = el("div", "rc-note");
  const infoText = el("div", "rc-note-copy");
  infoText.append(el("strong", "", t("RemoteCompute.katrainCompatible")), el("span", "", t("RemoteCompute.katrainDescription")));
  info.append(infoText);
  customCard.append(el("h3", "rc-section-title", t("RemoteCompute.custom")), el("p", "rc-section-subtitle", t("RemoteCompute.customDescription")),
    fieldRow(t("RemoteCompute.link"), link, link), importRow, info);
  const customAction = el("div", "rc-section");
  const useCustomButton = button(t("RemoteCompute.enableCustom"), "primary", "remote.useCustom", () => void useCustom());
  const localFromCustom = button(t("RemoteCompute.backToLocal"), "", "remote.local.custom", () => void useLocal());
  const customButtons = el("div", "rc-actions");
  customButtons.append(useCustomButton, localFromCustom);
  const deploy = el("div", "rc-note");
  const deployCopy = el("div", "rc-note-copy");
  deployCopy.append(el("strong", "", t("RemoteCompute.noLinkTitle")), el("span", "", t("RemoteCompute.noLinkDescription")));
  const deployButton = button(t("RemoteCompute.deployGuide"), "", "remote.deployGuide", () => void openLink(true));
  deployButton.title = t("RemoteCompute.deployGuideDescription");
  deploy.append(deployCopy, deployButton);
  customAction.append(el("h3", "rc-section-title", t("RemoteCompute.oneClickEnable")), customButtons, deploy);
  const customPage = el("div", "rc-page");
  customPage.append(customCard, customAction);

  // ---- footer ------------------------------------------------------------------------------
  const footer = el("div", "rc-footer");
  const dot = el("span", "rc-dot");
  dot.setAttribute("role", "img");
  dot.setAttribute("aria-label", t("RemoteCompute.connectionIndicator"));
  const currentLabel = el("span", "rc-current");
  currentLabel.setAttribute("aria-label", t("RemoteCompute.currentStatus"));
  const statusLabel = el("span", "rc-status");
  statusLabel.setAttribute("role", "status");
  statusLabel.setAttribute("aria-live", "polite");
  statusLabel.setAttribute("aria-label", t("RemoteCompute.connectionStatus"));
  footer.append(dot, currentLabel, statusLabel);

  const root = el("div", "rc-root");
  root.append(header, zhiziPage, customPage, footer);
  win.body.append(root);

  // ---- drafts derived from the controls ------------------------------------------------------
  const selectedWeight = () => weightSelect.value || DEFAULT_WEIGHT;
  const currentArgs = () => withKataWeight(PRESETS[Number(presetSelect.value)]?.args ?? PRESETS[0].args, selectedWeight());
  const selectedUnconfirmed = () => weightOptions.find((w) => w.name === weightSelect.value)?.confirmed === false;

  /** populateWeightOptions: confirmed entries plus the remembered one, preferred selected. */
  function populateWeights(catalog: CloudCatalog, preferredWeight: string) {
    const requested = selectableWeight(preferredWeight) ? preferredWeight.trim() : catalog.defaultWeight;
    const preferred = resolveWeight(catalog, requested);
    weightSelect.replaceChildren();
    weightOptions = [];
    for (const w of catalog.weights) {
      const isPreferred = w.name.toLowerCase() === preferred.toLowerCase();
      if (!selectableWeight(w.name) || (!confirmedWeight(w) && !isPreferred)) continue;
      const label = weightLabel(w, w.name === catalog.defaultWeight);
      const option = new Option(label, w.name);
      option.title = w.description ? `${label} - ${w.description}` : label;
      weightSelect.append(option);
      weightOptions.push({ name: w.name, confirmed: confirmedWeight(w) });
    }
    weightSelect.value = weightOptions.find((w) => w.name === preferred)?.name ?? weightOptions[0]?.name ?? "";
  }

  function selectPreset(args: string) {
    const index = PRESETS.findIndex((p) => samePlan(p.args, args));
    presetSelect.value = String(Math.max(0, index));
  }

  // ---- the projection of the window ----------------------------------------------------------
  function zhiziActionState(view: WorkbenchView): { text: string; tip: string; enabled: boolean } {
    const usingZhizi = saved.provider === "zhizi";
    const sameArgs = currentArgs() === saved.args;
    const current = foregroundProvider(view) === "zhizi";
    const loaded = current && !!view.engine.foreground?.ready;
    const failed = current && view.engine.down;
    if (!authenticated()) return { text: t("RemoteCompute.action.loginToEnable"), tip: t("RemoteCompute.action.loginToEnableTip"), enabled: false };
    if (usingZhizi && sameArgs && loaded) return { text: t("RemoteCompute.action.zhiziEnabled"), tip: t("RemoteCompute.action.alreadyEnabledTip"), enabled: false };
    if (usingZhizi && sameArgs && current && !failed) return { text: t("RemoteCompute.action.connectingZhizi"), tip: t("RemoteCompute.action.connectingTip"), enabled: false };
    if (usingZhizi && sameArgs && failed) return { text: t("RemoteCompute.action.reconnectZhizi"), tip: t("RemoteCompute.action.reconnectTip"), enabled: !busy };
    if (usingZhizi) return { text: t("RemoteCompute.action.changeZhizi"), tip: t("RemoteCompute.action.changeZhiziTip"), enabled: !busy };
    return { text: t("RemoteCompute.enableZhizi"), tip: t("RemoteCompute.action.enableZhiziTip"), enabled: !busy };
  }

  function customActionState(): { text: string; tip: string; enabled: boolean } {
    const value = normalizeLink(link.value);
    const valid = validLink(value);
    const usingCustom = saved.provider === "custom";
    if (!value) return { text: t("RemoteCompute.action.enterLink"), tip: t("RemoteCompute.action.enterLinkTip"), enabled: false };
    if (!valid) return { text: usingCustom ? t("RemoteCompute.action.validLinkToChange") : t("RemoteCompute.action.validLinkToEnable"), tip: t("RemoteCompute.error.linkScheme"), enabled: false };
    if (usingCustom && value === normalizeLink(saved.customUrl)) return { text: t("RemoteCompute.action.customEnabled"), tip: t("RemoteCompute.action.alreadyEnabledTip"), enabled: false };
    if (usingCustom) return { text: t("RemoteCompute.action.changeCustom"), tip: t("RemoteCompute.action.changeCustomTip"), enabled: !busy };
    return { text: t("RemoteCompute.enableCustom"), tip: t("RemoteCompute.action.enableCustomTip"), enabled: !busy };
  }

  function showOverview() {
    if (!overview) {
      membershipLabel.textContent = t("RemoteCompute.account.loading");
      membershipLabel.className = "rc-account-value";
      balanceLabel.textContent = t("RemoteCompute.account.balancePending");
      activityLabel.textContent = updatedLabel.textContent = "\u00a0";
      return;
    }
    const { account: profile, balance, recentUsage, loadedAt } = overview;
    let membership = profile.membership ? t("RemoteCompute.account.vipActive") : t("RemoteCompute.account.vipInactive");
    if (profile.membership && profile.membershipExpiresAt) membership += ` · ${format(t("RemoteCompute.account.expires"), localTime(profile.membershipExpiresAt, "date"))}`;
    membershipLabel.textContent = membership;
    membershipLabel.className = `rc-account-value ${profile.membership ? "rc-good" : "rc-muted"}`;
    balanceLabel.textContent = format(t("RemoteCompute.account.balanceSummary"), money(balance.remainingBalanceYuan), money(balance.yesterdayConsumptionYuan));
    const latest = recentUsage.items[0];
    const recent = latest ? format(t("RemoteCompute.account.recentUsage"), duration(latest.durationSeconds), money(latest.totalCostYuan)) : t("RemoteCompute.account.noRecentUsage");
    activityLabel.textContent = format(t("RemoteCompute.account.activity"), balance.currentConnections, recent);
    updatedLabel.textContent = format(t("RemoteCompute.account.updatedAt"), localTime(loadedAt, "time"));
  }

  function update() {
    // A control this update disables or hides would drop the focus to the page, where Esc no
    // longer reaches the window; Swing moves it on within the window, so here it goes to the
    // window itself.
    const focused = document.activeElement;
    const view = state.view ?? initial;
    const signedInNow = authenticated();
    zhiziTab.classList.toggle("selected", page === "zhizi");
    customTab.classList.toggle("selected", page === "custom");
    zhiziTab.setAttribute("aria-pressed", String(page === "zhizi"));
    customTab.setAttribute("aria-pressed", String(page === "custom"));
    zhiziPage.hidden = page !== "zhizi";
    customPage.hidden = page !== "custom";

    loginForm.hidden = signedInNow;
    signedIn.hidden = !signedInNow;
    const passwordMode = !codeMode && !resetMode;
    passwordModeButton.classList.toggle("selected", passwordMode);
    codeModeButton.classList.toggle("selected", codeMode && !resetMode);
    passwordRow.hidden = !passwordMode;
    newPasswordRow.hidden = confirmPasswordRow.hidden = !resetMode;
    codeRow.hidden = !(codeMode || resetMode);
    fastLoginHint.hidden = !(codeMode && !resetMode);
    rememberToken.chip.hidden = resetMode;
    rememberPassword.chip.hidden = !passwordMode;
    // 智子云算力's one filled action: 登录 until signed in, then 启用.
    loginButton.classList.toggle("primary", !signedInNow);
    useZhiziButton.classList.toggle("primary", signedInNow);
    forgotButton.textContent = resetMode ? t("RemoteCompute.backToLogin") : t("RemoteCompute.forgotPassword");
    loginButton.textContent = resetMode ? t("RemoteCompute.resetPasswordAndLogin") : t("RemoteCompute.login");
    // A remembered password stays in the system store; the field shows that one is there.
    password.placeholder = account?.hasPassword && account.passwordStoredSecurely ? "••••••••" : t("RemoteCompute.passwordPlaceholder");
    for (const input of [password, newPassword, confirmPassword]) input.type = passwordShown ? "text" : "password";
    eye.classList.toggle("selected", passwordShown);
    eye.setAttribute("aria-pressed", String(passwordShown));
    const eyeText = passwordShown ? t("RemoteCompute.hidePassword") : t("RemoteCompute.showPassword");
    eye.title = eyeText;
    eye.setAttribute("aria-label", eyeText);
    if (signedInNow) {
      const who = saved.identifier.trim() ? format(t("RemoteCompute.accountValue"), saved.identifier) : t("RemoteCompute.loggedIn");
      accountText.textContent = who + (account?.passwordStoredSecurely ? t("RemoteCompute.passwordStored") : t("RemoteCompute.passwordNotStored"));
    }
    showOverview();

    for (const input of [identifier, password, newPassword, confirmPassword, code, link]) input.disabled = busy;
    rememberToken.input.disabled = busy;
    rememberPassword.input.disabled = busy || codeMode;
    eye.disabled = busy || codeMode;
    presetSelect.disabled = busy;
    passwordModeButton.disabled = codeModeButton.disabled = forgotButton.disabled = busy;
    loginButton.disabled = busy;
    websiteButton.disabled = busy;
    logoutButton.disabled = busy;
    localFromZhizi.disabled = localFromCustom.disabled = importQrButton.disabled = busy;
    if (cooldown > 0) {
      sendCodeButton.textContent = format(t("RemoteCompute.sendCodeCountdown"), cooldown);
      sendCodeButton.disabled = true;
    } else {
      sendCodeButton.textContent = t("RemoteCompute.sendCode");
      sendCodeButton.disabled = busy || !(codeMode || resetMode);
    }
    const accountIdle = !busy && !overviewLoading && signedInNow;
    const refreshAccountText = overviewLoading ? t("RemoteCompute.account.refreshing") : t("RemoteCompute.account.refresh");
    refreshAccountButton.title = refreshAccountText;
    refreshAccountButton.setAttribute("aria-label", refreshAccountText);
    refreshAccountButton.disabled = usageButton.disabled = creditButton.disabled = paymentButton.disabled = !accountIdle;

    weightSelect.disabled = busy || catalogRefreshing || weightOptions.length === 0;
    refreshWeightsButton.disabled = busy || catalogRefreshing;
    const selectedWeightOption = weightSelect.selectedOptions[0];
    weightSelect.title = selectedWeightOption?.title ?? "";
    officialButton.hidden = !selectedUnconfirmed();
    officialButton.disabled = busy;

    const zhizi = zhiziActionState(view);
    useZhiziButton.textContent = zhizi.text; useZhiziButton.title = zhizi.tip; useZhiziButton.disabled = !zhizi.enabled;
    const custom = customActionState();
    useCustomButton.textContent = custom.text; useCustomButton.title = custom.tip; useCustomButton.disabled = !custom.enabled;

    const active = foregroundProvider(view) ?? (saved.provider as Provider);
    if (active === "zhizi") {
      currentLabel.textContent = t("RemoteCompute.current.zhizi");
      currentLabel.title = format(t("RemoteCompute.currentValue"), zhiziDisplayName(saved.args));
    } else if (active === "custom") {
      currentLabel.textContent = t("RemoteCompute.current.custom");
      currentLabel.title = format(t("RemoteCompute.currentValue"), customDisplayName(saved.customUrl));
    } else {
      currentLabel.textContent = currentLabel.title = t("RemoteCompute.current.local");
    }
    statusLabel.textContent = statusText;
    statusLabel.title = statusText;
    statusLabel.className = `rc-status ${statusOk === null ? "" : statusOk ? "rc-ok" : "rc-error"}`;
    dot.classList.toggle("rc-dot-error", !dotOk);
    if (focused instanceof HTMLElement && focused !== win.frame && win.frame.contains(focused) && (focused.matches(":disabled") || focused.closest("[hidden]"))) {
      win.frame.focus();
    }
  }

  /** updateCurrentStatus: the footer dot returns to green with the engine status. */
  const currentStatus = () => { dotOk = true; update(); };

  // ---- account ------------------------------------------------------------------------------
  /** A failed account request: a lost sign-in returns to the form (invalidateZhiziToken). */
  async function lostSignIn(f: CloudFailure) {
    if (!f.authenticationInvalidated) return;
    overview = null;
    const outcome = await request({ type: "State" });
    if (outcome?.reply) applyReply(outcome.reply);
    else if (account) account = { ...account, authenticated: false };
  }

  /** runBackground: busy, a status, and on failure the status plus a warning dialog. */
  async function background(op: CloudCommand, pending: string, sendCode = false): Promise<CloudReply | null> {
    const seq = ++operationSeq;
    busy = true;
    status(pending, true);
    update();
    const outcome = await write(op);
    if (!outcome || seq !== operationSeq) return null;
    busy = false;
    if (outcome.reply) { applyReply(outcome.reply); return outcome.reply; }
    const f = outcome.failure;
    if (f.cancelled) { statusText = t("RemoteCompute.chooseHint"); statusOk = null; currentStatus(); return null; }
    if (sendCode && (f.retryAfterSeconds > 0 || f.key === "fast_login_too_frequent")) startCooldown(f.retryAfterSeconds > 0 ? f.retryAfterSeconds : 60);
    await lostSignIn(f);
    if (!alive) return null;
    const text = failureText(f, currentArgs(), sendCode);
    status(text, false);
    currentStatus();
    dotOk = false;
    update();
    void message(t("RemoteCompute.connectionFailedTitle"), text, "warning");
    return null;
  }

  function startCooldown(seconds: number) {
    cooldown = Math.max(1, Math.min(600, Math.trunc(seconds)));
    clearInterval(cooldownTimer);
    cooldownTimer = window.setInterval(() => {
      cooldown = Math.max(0, cooldown - 1);
      if (cooldown === 0) { clearInterval(cooldownTimer); cooldownTimer = undefined; }
      update();
    }, 1000);
    update();
  }

  /** onZhiziLoggedIn / onZhiziPasswordLoggedIn. */
  function loggedIn(requestedToken: boolean, requestedPassword: boolean) {
    rememberToken.input.checked = saved.rememberToken && !!account?.tokenStoredSecurely;
    rememberPassword.input.checked = saved.rememberPassword && !!account?.passwordStoredSecurely;
    password.value = code.value = "";
    identifier.value = saved.identifier;
    const sessionOnly = (requestedToken && !account?.tokenStoredSecurely) || (requestedPassword && !account?.passwordStoredSecurely);
    status(sessionOnly ? t("RemoteCompute.status.loginSessionOnly") : t("RemoteCompute.status.loginSuccess"), !sessionOnly);
    dotOk = !sessionOnly;
    update();
    overview = null;
    void refreshOverview(true);
    void refreshCatalog(false);
  }

  async function loginWithPassword() {
    const id = identifier.value.trim(), secret = password.value.trim();
    // Java fills the field with a remembered password; here the system store supplies it.
    const storedPassword = !!account?.hasPassword && !!account.passwordStoredSecurely && id.toLowerCase() === saved.identifier.trim().toLowerCase();
    if (!id || (!secret && !storedPassword)) { status(t("RemoteCompute.error.accountPasswordRequired"), false); update(); return; }
    const token = rememberToken.input.checked, remember = rememberPassword.input.checked;
    password.value = "";
    const reply = await background({ type: "PasswordLogin", identifier: id, password: secret, remember_token: token, remember_password: remember }, t("RemoteCompute.status.loggingIn"));
    if (reply) loggedIn(token, remember);
  }

  async function loginWithCode() {
    const id = identifier.value.trim(), secret = code.value.trim();
    if (!id || !secret) { status(t("RemoteCompute.error.accountCodeRequired"), false); update(); return; }
    const token = rememberToken.input.checked;
    code.value = "";
    const reply = await background({ type: "CodeLogin", identifier: id, code: secret, remember_token: token }, t("RemoteCompute.status.codeLogin"));
    if (reply) loggedIn(token, false);
  }

  async function resetPassword() {
    const id = identifier.value.trim(), secret = code.value.trim();
    const next = newPassword.value, again = confirmPassword.value;
    if (!id || !secret) { status(t("RemoteCompute.error.accountCodeRequired"), false); update(); return; }
    if ([...next].length < 8) { status(t("RemoteCompute.error.passwordTooShort"), false); update(); return; }
    if (next !== again) { status(t("RemoteCompute.error.passwordMismatch"), false); update(); return; }
    const token = rememberToken.input.checked, remember = rememberPassword.input.checked;
    code.value = newPassword.value = confirmPassword.value = "";
    const reply = await background({ type: "ResetPassword", identifier: id, code: secret, password: next, remember_token: token, remember_password: remember }, t("RemoteCompute.status.resettingPassword"));
    if (!reply) return;
    loggedIn(token, remember);
    resetMode = false;
    status(t("RemoteCompute.status.passwordReset"), true);
    update();
  }

  async function sendCode() {
    const id = identifier.value.trim();
    if (!id) { status(t("RemoteCompute.error.accountRequired"), false); update(); return; }
    const reply = await background({ type: "SendCode", identifier: id, purpose: resetMode ? "reset_password" : "fast_login" }, t("RemoteCompute.status.sendingCode"), true);
    if (reply?.result.type !== "CodeSent") return;
    startCooldown(reply.result.retry_after_seconds);
    status(t("RemoteCompute.status.codeSent"), true);
    update();
  }

  /** clearSavedPasswordPreference: unticking 记住密码 forgets a stored password at once. */
  async function forgetSavedPassword() {
    if (!saved.rememberPassword && !account?.hasPassword) return;
    const outcome = await write({ type: "Remember", token: saved.rememberToken, password: false });
    if (outcome?.reply) applyReply(outcome.reply);
    if (alive) update();
  }

  async function logout() {
    overviewSeq++; catalogSeq++; operationSeq++;
    overviewLoading = catalogRefreshing = false;
    busy = true;
    update();
    const outcome = await write({ type: "Logout" });
    if (!outcome) return;
    busy = false;
    if (outcome.failure) {
      if (!outcome.failure.cancelled) status(failureText(outcome.failure, currentArgs()), false);
      update();
      return;
    }
    applyReply(outcome.reply);
    overview = null;
    password.value = newPassword.value = confirmPassword.value = "";
    resetMode = false;
    passwordShown = false;
    rememberPassword.input.checked = false;
    rememberToken.input.checked = saved.rememberToken;
    const warning = outcome.reply.state.storageWarning;
    status(warning ? t("RemoteCompute.status.credentialCleanupFailed") : t("RemoteCompute.status.loggedOut"), !warning);
    update();
  }

  async function refreshOverview(force: boolean) {
    if (overviewLoading) return;
    if (!authenticated()) { overview = null; update(); return; }
    const seq = ++overviewSeq;
    overviewLoading = true;
    update();
    const outcome = await request({ type: "Overview", force });
    if (!outcome || seq !== overviewSeq) return;
    overviewLoading = false;
    if (outcome.reply) {
      applyReply(outcome.reply);
      if (outcome.reply.result.type === "Overview") overview = outcome.reply.result.overview;
      if (force) status(t("RemoteCompute.account.updated"), true);
    } else if (!outcome.failure.cancelled) {
      await lostSignIn(outcome.failure);
      if (!alive) return;
      status(failureText(outcome.failure, currentArgs()), false);
    }
    update();
  }

  async function refreshCatalog(userInitiated: boolean) {
    if (!authenticated()) {
      if (userInitiated) { status(t("RemoteCompute.error.loginBeforeRefresh"), false); update(); }
      return;
    }
    if (catalogRefreshing) return;
    const preferred = selectedWeight();
    const seq = ++catalogSeq;
    catalogRefreshing = true;
    status(t("RemoteCompute.status.refreshingWeights"), true);
    update();
    const outcome = await request({ type: "RefreshCatalog", args: currentArgs() });
    if (!outcome || seq !== catalogSeq) return;
    catalogRefreshing = false;
    if (outcome.reply) {
      applyReply(outcome.reply);
      if (outcome.reply.result.type === "Catalog") {
        const catalog = outcome.reply.result.catalog;
        const retired = !catalog.weights.some((w) => w.name.toLowerCase() === preferred.toLowerCase());
        const resolved = resolveWeight(catalog, preferred);
        populateWeights(catalog, resolved);
        status(retired
          ? format(t("RemoteCompute.status.weightsRefreshedRetired"), catalog.weights.length, preferred, weightName(resolved))
          : format(t("RemoteCompute.status.weightsRefreshed"), catalog.weights.length), true);
      }
    } else if (!outcome.failure.cancelled) {
      await lostSignIn(outcome.failure);
      if (!alive) return;
      if (userInitiated) status(t("RemoteCompute.status.weightsRefreshFailed"), false);
      else if (!outcome.failure.authenticationInvalidated) dotOk = true;
    }
    update();
  }

  function showDetails(tab: 0 | 1) {
    if (!authenticated()) { status(t("RemoteCompute.error.loginFirst"), false); update(); return; }
    openAccountDetails(tab, (f) => { if (alive) void lostSignIn(f).then(() => alive && update()); });
  }

  function showPayment() {
    if (!authenticated()) { status(t("RemoteCompute.error.loginFirst"), false); update(); return; }
    void openPayment(() => {
      if (!alive) return;
      overview = null;
      void refreshOverview(true);
      status(t("RemoteCompute.payment.accountRefreshed"), true);
      update();
    }).then(async () => {
      if (!alive) return;
      const outcome = await request({ type: "State" });
      if (outcome?.reply) applyReply(outcome.reply);
      if (alive && !authenticated()) { overview = null; currentStatus(); }
    });
  }

  // ---- links and QR ------------------------------------------------------------------------
  /** openExternalPage: the browser, else the link copied with a notice. */
  async function openLink(custom: boolean) {
    const url = custom ? "https://github.com/wimi321/katago-remote-one-click" : "http://www.zhizigo.cn/";
    const result = await cloudCommand({ type: "OpenCloudLink", custom });
    if (!alive) return;
    if (result.type === "Done") {
      status(custom ? t("RemoteCompute.status.deployGuideOpened") : t("RemoteCompute.status.websiteOpened"), true);
      update();
      return;
    }
    try { await navigator.clipboard.writeText(url); } catch { /* the notice still names the link */ }
    if (!alive) return;
    status(custom ? t("RemoteCompute.status.deployGuideCopied") : t("RemoteCompute.status.websiteCopied"), false);
    update();
    void message(custom ? t("RemoteCompute.deployGuideCopiedTitle") : t("RemoteCompute.websiteCopiedTitle"),
      format(custom ? t("RemoteCompute.deployGuideCopiedMessage") : t("RemoteCompute.websiteCopiedMessage"), url));
  }

  async function importQr() {
    const result = await cloudCommand({ type: "CloudImportQr" });
    if (!alive || result.type === "Unchanged") return;
    if (result.type === "CloudUrl") {
      link.value = result.url;
      status(t("RemoteCompute.status.qrRead"), true);
      update();
      return;
    }
    const failure = t("RemoteCompute.error.qrNotRecognized");
    status(failure, false);
    update();
    const detail = result.type === "Failed" ? keyedDetail(result.message) : "";
    void message(t("RemoteCompute.qrFailedTitle"), detail ? `${failure}\n\n${detail}` : failure, "warning");
  }

  // ---- engine switches ---------------------------------------------------------------------
  /** switchPrimaryEngine: the command, then the views after its commit decide success or failure. */
  async function switchEngine(command: Command, launch: Launch, starting: string, success: string, failure: string) {
    const pending: NonNullable<typeof monitor> = { launch, success, failure, receipt: null, seen: new Map() };
    monitor = pending;
    busy = true;
    status(starting, true);
    update();
    const result = await cloudCommand(command);
    if (!alive || monitor !== pending) return;
    if (result.type === "CloudSwitch") {
      pending.receipt = { profile: result.profile, after: result.after };
      // The outcome may already be in the latest view, published before this reply arrived.
      watch(state.view ?? initial);
      return;
    }
    const detail = result.type === "Refused" ? t(result.message_key) : result.type === "Failed" ? keyedDetail(result.message) : "";
    finishSwitch(false, detail && launch.provider === "local" && result.type === "Failed" ? detail : detail ? `${failure} ${detail}` : failure);
  }

  function watch(view: WorkbenchView) {
    if (!monitor) return;
    for (const failure of view.engine_failures) if (!failure.secondary) monitor.seen.set(failure.profile_id, { seq: view.seq, failure });
    const receipt = monitor.receipt;
    if (!receipt) return;
    const ok = switchOutcome(view, receipt.profile, receipt.after, monitor.launch);
    if (ok === null) return;
    // A 智子 engine that never became ready says why (ZhiziGtpTransport, friendlyZhiziErrorMessage);
    // only the requested foreground profile's own failure after the commit speaks for this switch.
    const seen = monitor.seen.get(receipt.profile);
    const reason = !ok && seen && seen.seq > receipt.after ? seen.failure.cloud : undefined;
    const launch = monitor.launch;
    finishSwitch(ok, ok ? monitor.success : reason && launch.provider === "zhizi" ? cloudStartText(reason, launch.args) : monitor.failure);
  }

  function finishSwitch(ok: boolean, text: string) {
    monitor = null;
    busy = false;
    status(text, ok);
    update();
    // The committed provider and launch settings come back with the account state.
    void request({ type: "State" }).then((outcome) => {
      if (!outcome?.reply) return;
      applyReply(outcome.reply);
      if (alive) update();
    });
  }

  async function useZhizi() {
    if (!authenticated()) { status(t("RemoteCompute.error.loginFirst"), false); page = "zhizi"; update(); return; }
    const args = currentArgs();
    const failure = selectedUnconfirmed() ? t("RemoteCompute.error.unconfirmedWeightRejected") : t("RemoteCompute.error.zhiziRestartFailed");
    // The explicit 启用 action is the authorization to start the billed cloud engine.
    await switchEngine({ type: "CloudActivate", settings: { ...saved, provider: "zhizi", args }, confirmed: true }, { provider: "zhizi", args },
      t("RemoteCompute.status.startingZhizi"), format(t("RemoteCompute.status.zhiziConnected"), zhiziDisplayName(args)), failure);
  }

  async function useCustom() {
    const value = normalizeLink(link.value);
    if (!value) { status(t("RemoteCompute.error.linkRequired"), false); update(); return; }
    if (!validLink(value)) {
      status(t("RemoteCompute.error.linkScheme"), false);
      update();
      void message(t("RemoteCompute.invalidLinkTitle"), t("RemoteCompute.invalidLinkMessage"), "warning");
      return;
    }
    link.value = value;
    page = "custom";
    // The core stores the link trimmed, without line breaks.
    await switchEngine({ type: "CloudActivate", settings: { ...saved, provider: "custom", customUrl: value }, confirmed: true },
      { provider: "custom", url: value.trim().replace(/[\r\n]/g, "") },
      t("RemoteCompute.status.startingCustom"), format(t("RemoteCompute.status.customEnabled"), customDisplayName(value)), t("RemoteCompute.error.engineSwitchFailed"));
  }

  async function useLocal() {
    await switchEngine({ type: "CloudLocal" }, { provider: "local" }, t("RemoteCompute.status.startingLocal"), t("RemoteCompute.status.localEnabled"), t("RemoteCompute.error.engineSwitchFailed"));
  }

  // ---- loadState ---------------------------------------------------------------------------
  populateWeights(saved.catalog, kataWeight(saved.args));
  selectPreset(saved.args);
  update();
  void (async () => {
    const outcome = await request({ type: "State" });
    if (!outcome) return;
    if (outcome.failure) { update(); return; }
    applyReply(outcome.reply);
    identifier.value = saved.identifier;
    rememberToken.input.checked = saved.rememberToken;
    rememberPassword.input.checked = saved.rememberPassword;
    if (outcome.reply.state.storageWarning) status(t("RemoteCompute.status.credentialReadFailed"), false);
    update();
    if (authenticated()) {
      void refreshOverview(false);
      const catalog = saved.catalog;
      const authoritative = catalog.weights.length > 0 && catalog.weights.every((w) => w.source === "SERVER_CAPABILITIES");
      if (!authoritative || saved.catalogUpdatedAt <= 0 || Date.now() - saved.catalogUpdatedAt >= CATALOG_REFRESH_MS) void refreshCatalog(false);
    }
  })();

  return {
    focus: () => win.frame.focus(),
    onView(view) {
      const cloud = JSON.stringify(view.settings.cloud);
      if (cloud !== lastViewCloud) { lastViewCloud = cloud; saved = view.settings.cloud; }
      watch(view);
      update();
    },
  };
}
