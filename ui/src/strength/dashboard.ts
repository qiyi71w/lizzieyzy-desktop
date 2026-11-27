// 棋力评估 (LizzieFrame.showPlayerStrengthEstimate; Java StrengthDialog and StrengthDashboard): the
// modal dashboard with its toolbar (测评 / 吻合度 / 发挥水准 / 分阶段 tabs and their subtitle, the
// model picker ending with 导入模型…, 详细数据), the four dashboards or, while they cannot be
// computed, what to do first (StrengthEmptyState: analyse the game, a game too short to estimate,
// the human-style model's one-time calculation), and the HumanSL strip. The core computes the
// report, its context (stage, analysed moves) and its phases; a model change, an import, a HumanSL
// run, a started analysis or the 700 ms refresh (StrengthDialog.refresh) shows a fresh report.
import { registerViewHandler, run, state } from "../app";
import { chooseStrengthModel } from "../contract/client";
import type { StrengthContext, StrengthModelView, StrengthPhases, StrengthReport, StrengthView } from "../contract/generated";
import { icon } from "../design/icons";
import { message } from "../dialogs";
import { errorText } from "../errorText";
import { format, product, registerProduct, t } from "../i18n";
import { startFlash } from "../quick/flash";
import { size } from "../setup/setupText";
import { openDetail, openDetailDialog } from "./detail";
import { hasSamples, humanSlText, javaFormat, modelBadge, modelDescription, modelDisplayName, modelTitle } from "./format";
import { prepareHumanSl, progressReport } from "./humanSl";
import { assessmentSection, el, legend, matchSummary, moveHitMap, noteStrip, performancePanel, type CardTexts } from "./panels";
import { phasesPanel, type PhaseActions, type PhasesView } from "./phases";
import { rulerDrawn } from "./scale";
import { openWindow, ownerFocus, type StrengthWindow } from "./window";
import type { Reason } from "../commands";
import { locateControl } from "../search/highlight";
import { phasesLocateReason } from "../search/locateRules";
import "./strength.css";

// Java's note on the human-style weights names their size; this one takes it from the resource
// catalogue (PlayerStrengthEstimate.empty.humanStyle.download with the size as {0}). Java's
// analysis page says the game is not analysed even when part of it is; this one says how much.
registerProduct({
  ZhCn: {
    strengthHumanStyleDownload: "第一次使用需要下载约 {0} 的人类风格权重，下载前会先征求你的同意。",
    strengthPartlyAnalyzed: "这盘棋只分析了 {0} / {1} 手",
  },
  ZhTw: {
    strengthHumanStyleDownload: "第一次使用需要下載約 {0} 的人類風格權重，下載前會先徵求你的同意。",
    strengthPartlyAnalyzed: "這盤棋只分析了 {0} / {1} 手",
  },
  EnUs: {
    strengthHumanStyleDownload: "The first time, it downloads the human-style weights (about {0}). You will be asked before the download starts.",
    strengthPartlyAnalyzed: "Only {0} of {1} moves of this game are analyzed",
  },
  JaJp: {
    strengthHumanStyleDownload: "初回は人間スタイルの重み（約 {0}）をダウンロードします。ダウンロードの前に確認します。",
    strengthPartlyAnalyzed: "この対局は {1} 手中 {0} 手しか解析されていません",
  },
  KoKr: {
    strengthHumanStyleDownload: "처음 사용할 때 사람 스타일 가중치(약 {0})를 내려받습니다. 내려받기 전에 먼저 묻습니다.",
    strengthPartlyAnalyzed: "이 대국은 {1}수 중 {0}수만 분석되었습니다",
  },
  ThTh: {
    strengthHumanStyleDownload: "ครั้งแรกจะดาวน์โหลดน้ำหนักสไตล์มนุษย์ (ประมาณ {0}) และจะถามก่อนเริ่มดาวน์โหลด",
    strengthPartlyAnalyzed: "เกมนี้วิเคราะห์แล้วเพียง {0} จาก {1} ตา",
  },
});

type Tab = "assessment" | "match" | "performance" | "phases";

/** StrengthDialog.REBUILD_TICKS_WHILE_ANALYZING: shown figures follow a running analysis every
 *  third refresh. */
const REBUILD_TICKS_WHILE_ANALYZING = 3;

/** An empty page that stays while its stage does, updating its progress in place
 *  (StrengthEmptyState.showProgress). */
interface Page {
  /** The stage it shows (with the human-style download it mentions). */
  stage: string;
  update(context: StrengthContext): void;
}

/** The human-style page's parts that follow the HumanSL preparation status. */
interface HumanParts {
  run: HTMLButtonElement;
  status: HTMLElement;
  bar: Bar;
}

interface Dashboard {
  win: StrengthWindow;
  content: HTMLElement;
  /** The strength view's stamp of the shown report. */
  stamp: number;
  /** StrengthDialog.wasAnalyzing and quietTicks. */
  analyzing: boolean;
  quiet: number;
  /** The newest report request; older answers are dropped. */
  request: number;
  /** A report request is under way. */
  loading: boolean;
  comboOpen: boolean;
  /** A phase divider is held: the refresh waits for it. */
  holding: boolean;
  /** The selected tab, kept when a new report is shown. */
  tab: Tab;
  /** Shows a tab of the figures; null while the dashboard has none (its tabs are disabled). */
  select: ((tab: Tab) => void) | null;
  shown: { report: StrengthReport; context: StrengthContext; phases: StrengthPhases } | null;
  phases: PhasesView | null;
  page: Page | null;
  human: HumanParts | null;
  /** The HumanSL status lines (the strip's) and 分析棋局时自动准备 boxes (the strip's and the
   *  human-style page's). */
  statuses: HTMLElement[];
  backgrounds: HTMLInputElement[];
  /** What the built dashboard shows (acceptance probe). */
  probe: Probe | null;
}

interface Probe {
  report: StrengthReport;
  cards: { black: CardTexts; white: CardTexts } | null;
  notes: Record<Tab, string>;
}

let dashboard: Dashboard | null = null;

/** humanSlPreparationStatus: the strip's wording of the preparation status. */
function preparationStatus(view: StrengthView): string {
  let key = view.status.key;
  if (key === "HumanSL40.needEngine") key = "HumanSL40.backgroundNeedEngine";
  else if (key === "HumanSL40.failed" || key === "HumanSL40.invalidModel" || key === "HumanSL40.invalidResponse") key = "HumanSL40.backgroundFailed";
  return format(humanSlText(key), view.status.completed, view.status.total);
}

/** StrengthDialog.tone: the preparation is under way. */
function preparing(view: StrengthView): boolean {
  return view.status.key === "HumanSL40.preparing" || view.status.key === "HumanSL40.progress";
}

/** The menu and the toolbar button open the dashboard for the active model. */
export async function openStrengthDashboard(): Promise<void> {
  if (dashboard) dashboard.win.box.focus();
  else await showDashboard();
}

/** Function search's 分阶段: the dashboard at that page, on its first divider. Before the game can
 *  be estimated the dashboard stays at the page that says what to do first, and that is the
 *  reason the divider was not reached. */
export async function locateStrengthPhases(): Promise<Reason | null> {
  const d = dashboard ?? (await showDashboard());
  if (!d?.shown) return { key: "FunctionSearch.unavailable.targetMissing" };
  const reason = phasesLocateReason(d.shown.context.stage);
  if (reason) {
    d.win.box.focus();
    return reason;
  }
  d.select?.("phases");
  const divider = d.content.querySelector<HTMLElement>('[data-strength="phase.divider.0"]');
  if (!divider) return { key: "FunctionSearch.unavailable.targetMissing" };
  locateControl(divider, divider.closest<HTMLElement>(".ps-phase-timeline") ?? divider);
  return null;
}

/** Opens the dashboard with a fresh report; the open one if another request opened it meanwhile,
 *  or null when the report failed. */
async function showDashboard(): Promise<Dashboard | null> {
  const result = await run({ type: "Strength", op: { type: "Report" } });
  if (dashboard) return dashboard;
  if (result?.type !== "Strength") return null;
  const content = el("div", "ps-root");
  // StrengthDialog's 700 ms timer, stopped when the window closes.
  const timer = setInterval(() => checkStamp(d), 700);
  const d: Dashboard = {
    win: openWindow(t("PlayerStrengthEstimate.title"), "ps-dashboard", () => {
      clearInterval(timer);
      if (dashboard === d) dashboard = null;
    }),
    content,
    stamp: state.view?.strength.stamp ?? 0,
    analyzing: state.view?.strength.analyzing ?? false,
    quiet: 0,
    request: 0,
    loading: false,
    comboOpen: false,
    holding: false,
    tab: "assessment",
    select: null,
    shown: null,
    page: null,
    human: null,
    statuses: [],
    backgrounds: [],
    probe: null,
    phases: null,
  };
  dashboard = d;
  d.win.body.append(content);
  build(d, result.report, result.context, result.phases);
  return d;
}

/** Re-requests the report and shows it (StrengthDialog.showEstimate). */
async function refresh(d: Dashboard): Promise<void> {
  const request = ++d.request;
  d.stamp = state.view?.strength.stamp ?? d.stamp;
  d.loading = true;
  const result = await run({ type: "Strength", op: { type: "Report" } });
  if (request === d.request) d.loading = false;
  if (dashboard !== d || request !== d.request || result?.type !== "Strength") return;
  build(d, result.report, result.context, result.phases);
}

/** StrengthDialog.analyzeGame: 整盘速览 of the main window, the same operation and progress as the
 *  analysis menu's, then the estimate with its progress. */
async function analyzeGame(d: Dashboard): Promise<void> {
  await startFlash({ kind: "WholeGame" });
  if (dashboard !== d) return;
  d.analyzing = state.view?.strength.analyzing ?? d.analyzing;
  await refresh(d);
}

/** StrengthDialog.prepareNow: 开始计算, then the estimate (the weights may have arrived). */
async function prepareNow(d: Dashboard): Promise<void> {
  await prepareHumanSl();
  if (dashboard === d) await refresh(d);
}

/** A dashboard button. */
function toolButton(text: string, onClick: () => void, cls = ""): HTMLButtonElement {
  const button = el("button", cls, text);
  button.type = "button";
  button.onclick = onClick;
  return button;
}

/** Names a control for the acceptance probe. */
function tagged<T extends HTMLElement>(e: T, key: string): T {
  e.dataset.strength = key;
  return e;
}

/** Names a text for the acceptance probe. */
function text<T extends HTMLElement>(e: T, key: string): T {
  e.dataset.strengthText = key;
  return e;
}

/** A small rounded label: 推荐 in the accent, 实验 in the caution colour. */
function badge(model: StrengthModelView, key: string): HTMLElement | null {
  const b = modelBadge(model);
  return b && text(el("span", b.recommended ? "ps-badge recommended" : "ps-badge", b.text), key);
}

/** StrengthModelPicker: 棋力模型 before a field with the chosen model's everyday name (and its
 *  实验 badge for human style), opening a menu where each model has its name, badge, technical
 *  name and one sentence, the current one checked, then 导入模型…. */
function modelPicker(d: Dashboard, models: StrengthModelView[], selected: StrengthModelView): HTMLElement {
  const host = el("div", "ps-combo");
  const caption = el("span", "ps-label", t("PlayerStrengthEstimate.model"));
  const anchor = el("div", "ps-combo-anchor");
  const button = tagged(el("button", "ps-combo-button"), "model.combo");
  button.type = "button";
  button.setAttribute("aria-label", t("PlayerStrengthEstimate.model"));
  button.setAttribute("aria-description", modelTitle(selected));
  button.setAttribute("aria-haspopup", "menu");
  button.setAttribute("aria-expanded", "false");
  const value = el("span", "ps-combo-value");
  value.append(text(el("span", "", modelTitle(selected)), "model.value"));
  if (selected.human_sl) value.append(badge(selected, "model.valueBadge")!);
  button.append(value, icon("chevron-down"));
  const list = el("div", "ps-combo-list");
  list.setAttribute("role", "menu");
  list.hidden = true;
  const items: HTMLElement[] = models.map((model) => {
    const current = model.id === selected.id;
    const row = tagged(el("div", current ? "ps-model-row current" : "ps-model-row"), `model.option.${model.id}`);
    row.setAttribute("role", "menuitemradio");
    row.setAttribute("aria-checked", String(current));
    row.setAttribute("aria-description", modelDescription(model));
    const check = icon("check");
    check.classList.add("ps-model-check");
    const head = el("div", "ps-model-head");
    head.append(text(el("span", "ps-model-title", modelTitle(model)), `model.${model.id}.title`));
    const b = badge(model, `model.${model.id}.badge`);
    if (b) head.append(b);
    const technical = modelDisplayName(model);
    if (technical !== modelTitle(model)) head.append(text(el("span", "ps-model-technical", technical), `model.${model.id}.technical`));
    const body = el("div", "ps-model-text");
    body.append(head, text(el("span", "ps-model-description", modelDescription(model)), `model.${model.id}.description`));
    row.append(check, body);
    return row;
  });
  const importItem = tagged(el("div", "ps-model-row ps-model-import"), "model.import");
  importItem.setAttribute("role", "menuitem");
  importItem.title = t("PlayerStrengthEstimate.importModel.tooltip");
  importItem.append(icon("open"), text(el("span", "", t("PlayerStrengthEstimate.model.import")), "model.import"));
  items.push(importItem);
  const separator = el("div", "ps-combo-separator");
  separator.setAttribute("role", "separator");
  list.append(...items.slice(0, -1), separator, importItem);
  anchor.append(button, list);
  host.append(caption, anchor);

  const currentIndex = Math.max(0, models.findIndex((m) => m.id === selected.id));
  let active = currentIndex;
  items.forEach((item, k) => {
    item.addEventListener("mouseenter", () => mark(k));
    item.addEventListener("mousedown", (e) => e.preventDefault());
    item.onclick = () => choose(k);
  });

  function mark(k: number): void {
    active = k;
    items.forEach((item, j) => item.classList.toggle("active", j === k));
  }
  /** The active item scrolled into the list's view (keyboard moves; the pointer is already on it). */
  function reveal(): void {
    const item = items[active];
    if (item.offsetTop < list.scrollTop) list.scrollTop = item.offsetTop;
    else if (item.offsetTop + item.offsetHeight > list.scrollTop + list.clientHeight) {
      list.scrollTop = item.offsetTop + item.offsetHeight - list.clientHeight;
    }
  }
  // The list's own scrolling leaves it where it is; the scrolling around it moves it along.
  const follow = (e: Event) => {
    if (e.target !== list) place();
  };
  function setOpen(open: boolean): void {
    if (d.comboOpen === open) return;
    d.comboOpen = open;
    list.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
    if (open) {
      mark(currentIndex);
      place();
      reveal();
      // Capturing: also the scroll of the part between the tabs and the strip in a short window.
      d.win.body.addEventListener("scroll", follow, { capture: true, passive: true });
      window.addEventListener("resize", place);
    } else {
      d.win.body.removeEventListener("scroll", follow, { capture: true });
      window.removeEventListener("resize", place);
    }
    // A refresh skipped while the menu was open runs now.
    if (!open && dashboard === d) checkStamp(d);
  }
  /** Like Java's popup menu the list may pass the dialog's edges: under the field and
   *  right-aligned with it (StrengthModelPicker.menuLocation), above it when there is more room
   *  there, always inside the window and scrolling when taller than the room. */
  function place(): void {
    if (!list.isConnected) {
      window.removeEventListener("resize", place);
      return;
    }
    const field = button.getBoundingClientRect();
    const gap = 4;
    const margin = 8;
    list.style.maxHeight = "";
    const height = list.offsetHeight;
    const below = innerHeight - field.bottom - gap - margin;
    const above = field.top - gap - margin;
    const down = height <= below || below >= above;
    const room = Math.min(480, down ? below : above);
    list.style.maxHeight = `${room}px`;
    list.style.top = `${down ? field.bottom + gap : field.top - gap - Math.min(height, room)}px`;
    const width = list.offsetWidth;
    list.style.left = `${Math.max(margin, Math.min(field.right - width, innerWidth - margin - width))}px`;
  }
  function choose(k: number): void {
    setOpen(false);
    const model = models[k];
    if (model) chooseModel(d, model);
    else void importModel(d);
  }
  button.onclick = () => setOpen(!d.comboOpen);
  button.addEventListener("blur", () => setOpen(false));
  button.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!d.comboOpen) setOpen(true);
      else {
        mark(Math.max(0, Math.min(items.length - 1, active + (e.key === "ArrowDown" ? 1 : -1))));
        reveal();
      }
    } else if ((e.key === "Enter" || e.key === " ") && d.comboOpen) {
      e.preventDefault();
      choose(active);
    } else if (e.key === "Escape" && d.comboOpen) {
      e.preventDefault();
      e.stopPropagation();
      setOpen(false);
    }
  });
  return host;
}

/** StrengthDialog.chooseModel: the same model again only reruns HumanSL's report. */
function chooseModel(d: Dashboard, model: StrengthModelView): void {
  if (model.id === state.view?.strength.active && !model.human_sl) return;
  void (async () => {
    await run({ type: "Strength", op: { type: "SelectModel", id: model.id } });
    if (dashboard === d) await refresh(d);
  })();
}

/** 导入模型… (StrengthDialog.importModel): the file chooser, then the imported model is active. */
async function importModel(d: Dashboard): Promise<void> {
  const title = t("PlayerStrengthEstimate.importModel.title");
  const path = await chooseStrengthModel(title).catch((e: unknown) => {
    void message(t("Message.title"), errorText(e), "error");
    return null;
  });
  if (!path) return;
  const result = await run({ type: "Strength", op: { type: "ImportModel", path } });
  if (result?.type === "StrengthImportFailed") {
    await message(title, javaFormat(t("PlayerStrengthEstimate.importModel.error"), result.detail), "error");
  } else if (result?.type === "Done" && dashboard === d) await refresh(d);
}

function tabButton(label: string): HTMLButtonElement {
  const b = el("button", "ps-tab", label);
  b.type = "button";
  b.setAttribute("role", "tab");
  return b;
}

/** The tab row's width at the window's: one row (StrengthTabs) that, when it does not fit,
 *  scrolls across with a fade on each side that hides more (strength.css .more-start /
 *  .more-end). Answers the way to scroll a tab whole into view, clear of the fades. */
function scrollingTabs(row: HTMLElement): (tab: HTMLElement) => void {
  const FADE = 24;
  const edges = () => {
    const start = row.scrollLeft > 0.5;
    const end = row.scrollLeft + row.clientWidth < row.scrollWidth - 0.5;
    if (row.classList.contains("more-start") !== start) row.classList.toggle("more-start", start);
    if (row.classList.contains("more-end") !== end) row.classList.toggle("more-end", end);
  };
  row.addEventListener("scroll", edges, { passive: true });
  return (tab) => {
    const view = row.getBoundingClientRect();
    const r = tab.getBoundingClientRect();
    const left = view.left + row.clientLeft;
    const right = left + row.clientWidth;
    // At either end the row stops scrolling, and that side has no fade.
    const by = r.left < left + FADE ? r.left - (left + FADE) : r.right > right - FADE ? r.right - (right - FADE) : 0;
    if (Math.abs(by) > 0.5) row.scrollLeft += by;
    edges();
  };
}

/** The element's nearest ancestor with an overflow across when that ancestor scrolls across only
 *  (overflow-x auto or scroll, overflow-y hidden or clip: the tab row); null otherwise, as for a
 *  panel that scrolls both ways or a box that clips. */
function acrossScroller(e: HTMLElement, box: HTMLElement): HTMLElement | null {
  for (let a = e.parentElement; a && a !== box; a = a.parentElement) {
    const s = getComputedStyle(a);
    if (s.overflowX === "visible") continue;
    const across = s.overflowX === "auto" || s.overflowX === "scroll";
    return across && (s.overflowY === "hidden" || s.overflowY === "clip") ? a : null;
  }
  return null;
}

/** A side with no analysed move has no measurement: its figures and bars would show defaults
 *  (0%, full loss bars), so the section keeps its heading and shows the empty state instead. */
function emptySide(section: HTMLElement): HTMLElement {
  section.replaceChildren(section.firstElementChild!, el("p", "ps-empty", t("PlayerStrengthEstimate.match.noMoveSamples")));
  return section;
}

/** The assessment tab with what its cards and note show. */
interface Assessment {
  panel: HTMLElement;
  cards: { black: CardTexts; white: CardTexts };
  note: string;
}

/** StrengthDashboard.assessmentView: the two sides and the note. */
function assessmentDashboard(report: StrengthReport): Assessment {
  const humanSl = report.model.human_sl;
  const panel = el("div", "ps-dash ps-dash-assessment");
  const sides = el("div", "ps-two");
  const black = assessmentSection(true, report.black, humanSl);
  const white = assessmentSection(false, report.white, humanSl);
  sides.append(
    hasSamples(report.black.summary) ? black.element : emptySide(black.element),
    hasSamples(report.white.summary) ? white.element : emptySide(white.element),
  );
  const note = humanSl ? t("HumanSL40.reviewNote") : t("PlayerStrengthEstimate.reviewOnlyNote");
  panel.append(sides, noteStrip(note));
  return { panel, cards: { black: black.drawn, white: white.drawn }, note };
}

/** buildPlayerStrengthMatchDashboard: the two sides' match figures, 选点命中图 with its legend,
 *  the note. */
function matchDashboard(report: StrengthReport): HTMLElement {
  const panel = el("div", "ps-dash ps-dash-match");
  const top = el("div", "ps-two ps-match-top");
  const black = matchSummary(true, report.black.summary);
  const white = matchSummary(false, report.white.summary);
  top.append(hasSamples(report.black.summary) ? black : emptySide(black), hasSamples(report.white.summary) ? white : emptySide(white));
  const head = el("div", "ps-section-head");
  const titles = el("div", "ps-section-title");
  titles.append(el("h3", "", t("PlayerStrengthEstimate.match.moveMap")), el("p", "ps-label", t("PlayerStrengthEstimate.match.moveMapHint")));
  head.append(titles, legend());
  const map = el("div", "ps-fill ps-hit-map-box");
  map.append(moveHitMap(report).element);
  const chart = el("section", "ps-chart");
  chart.append(head, map);
  panel.append(top, chart, noteStrip(t("PlayerStrengthEstimate.match.reviewNote")));
  return panel;
}

/** StrengthEmptyState's slim bar of a known amount of work; hidden while there is none. */
interface Bar {
  element: HTMLElement;
  set(done: number, total: number): void;
}

function progressBar(): Bar {
  const element = el("div", "ps-progress-bar ps-empty-progress");
  element.setAttribute("role", "progressbar");
  element.setAttribute("aria-valuemin", "0");
  const fill = el("div", "ps-progress-fill");
  element.append(fill);
  return {
    element,
    set(done, total) {
      element.hidden = total <= 0;
      const value = total > 0 ? Math.min(100, Math.max(0, (done * 100) / total)) : 0;
      fill.style.width = `${value}%`;
      element.setAttribute("aria-valuemax", String(total));
      element.setAttribute("aria-valuenow", String(done));
    },
  };
}

/** StrengthEmptyState's column: the stone pair, the title and the explanation. */
function emptyPage(stage: string, title: string, body: string): { element: HTMLElement; column: HTMLElement; heading: HTMLElement } {
  const element = el("div", "ps-dash ps-empty-page");
  element.dataset.stage = stage;
  const column = el("div", "ps-empty-column");
  const stones = icon("stone-alternate");
  stones.classList.add("ps-empty-stones");
  const heading = text(el("h3", "ps-empty-title", title), "page.title");
  column.append(stones, heading, text(el("p", "ps-empty-body", body), "page.body"));
  element.append(column);
  return { element, column, heading };
}

/** "正在分析整盘棋：已分析 42 / 326 手…" while an analysis runs, else "". */
function runningText(context: StrengthContext): string {
  return context.analyzing
    ? format(t("PlayerStrengthEstimate.empty.analysis.running"), context.analyzed, Math.max(context.total, context.analyzed))
    : "";
}

/** The analysis page's title: how much of the game is analysed once part of it is and no analysis
 *  runs (a running one shows its progress below), else that it is not analysed yet. */
function analysisTitle(context: StrengthContext): string {
  return !context.analyzing && context.analyzed > 0
    ? product("strengthPartlyAnalyzed", context.analyzed, Math.max(context.total, context.analyzed))
    : t("PlayerStrengthEstimate.empty.analysis.title");
}

/** StrengthEmptyState(ANALYSIS): this game is not analysed yet, or only part of it, with 分析整盘棋,
 *  its progress while an analysis runs (the button then waits), or that the game is too short,
 *  without the button. */
function analysisPage(d: Dashboard, context: StrengthContext): { element: HTMLElement; page: Page } {
  const short = context.stage.type === "TooShort";
  const { element, column, heading } = short
    ? emptyPage("short", t("PlayerStrengthEstimate.empty.short.title"), t("PlayerStrengthEstimate.empty.short.body"))
    : emptyPage("analysis", analysisTitle(context), t("PlayerStrengthEstimate.empty.analysis.body"));
  if (short) return { element, page: { stage: "TooShort", update() {} } };
  const action = tagged(toolButton(t("PlayerStrengthEstimate.empty.analysis.action"), () => void analyzeGame(d), "primary ps-empty-action"), "analyze");
  const status = text(el("p", "ps-empty-status"), "page.status");
  status.setAttribute("role", "status");
  const bar = progressBar();
  column.append(action, status, bar.element);
  const update = (c: StrengthContext) => {
    const title = analysisTitle(c);
    if (heading.textContent !== title) heading.textContent = title;
    if (action.disabled !== c.analyzing) action.disabled = c.analyzing;
    const running = runningText(c);
    status.hidden = !running;
    if (status.textContent !== running) status.textContent = running;
    bar.set(c.analyzing ? c.analyzed : 0, c.analyzing ? Math.max(c.total, c.analyzed) : 0);
  };
  update(context);
  return { element, page: { stage: "NeedsAnalysis", update } };
}

/** A 分析棋局时自动准备 box; a failed save shows its message and the box returns to the saved
 *  choice. */
function backgroundToggle(d: Dashboard, key: string): HTMLLabelElement {
  const toggle = el("label", "ps-check");
  const box = tagged(el("input"), key);
  box.type = "checkbox";
  box.checked = state.view?.strength.background ?? false;
  box.onchange = async () => {
    const result = await run({ type: "Strength", op: { type: "SetBackground", enabled: box.checked } });
    if (result?.type !== "Done") box.checked = state.view?.strength.background ?? false;
  };
  d.backgrounds.push(box);
  toggle.append(box, t("HumanSL40.backgroundToggle"));
  return toggle;
}

/** StrengthEmptyState(HUMAN_STYLE): what the one-time calculation does, 开始计算 with the
 *  preparation's status and progress, then 分析棋局时自动准备 with what it costs and, while the
 *  weights are missing, their download. */
function humanStylePage(d: Dashboard, context: StrengthContext): { element: HTMLElement; page: Page } {
  const { element, column } = emptyPage("humanStyle", t("PlayerStrengthEstimate.empty.humanStyle.title"), t("PlayerStrengthEstimate.empty.humanStyle.body"));
  const start = tagged(toolButton(t("HumanSL40.manualRun"), () => void prepareNow(d), "primary ps-empty-action"), "run.page");
  const status = text(el("p", "ps-empty-status"), "page.status");
  status.setAttribute("role", "status");
  const bar = progressBar();
  d.human = { run: start, status, bar };
  const divider = el("div", "ps-empty-divider");
  const toggle = backgroundToggle(d, "background.page");
  toggle.classList.add("ps-empty-toggle");
  const cost = text(el("p", "ps-empty-note", t("HumanSL40.backgroundToggle.description")), "page.cost");
  column.append(start, status, bar.element, divider, toggle, cost);
  const download = context.human_style_download;
  if (download !== null) {
    column.append(text(el("p", "ps-empty-note ps-empty-download", product("strengthHumanStyleDownload", size(download))), "page.download"));
  }
  if (state.view) showHumanStatus(d, state.view.strength);
  return { element, page: { stage: `NeedsHumanStyle:${download ?? ""}`, update() {} } };
}

/** StrengthDialog.showStatus on the human-style page: the status line, 开始计算 waiting while
 *  the preparation works, and its progress when the number of moves is known. */
function showHumanStatus(d: Dashboard, view: StrengthView): void {
  const parts = d.human;
  if (!parts) return;
  const shown = preparationStatus(view);
  const working = preparing(view);
  parts.status.hidden = !shown.trim();
  if (parts.status.textContent !== shown) parts.status.textContent = shown;
  if (parts.run.disabled !== working) parts.run.disabled = working;
  parts.bar.set(working ? view.status.completed : 0, working ? view.status.total : 0);
}

/** StrengthBanner over the figures: a running analysis and its progress, or that only part of the
 *  game is analysed, with 分析整盘棋 for the rest. */
function analysisBanner(d: Dashboard, context: StrengthContext): HTMLElement | null {
  const partial = context.stage.type === "Ready" && context.stage.partial;
  const shown = context.analyzing
    ? runningText(context)
    : partial
      ? format(t("PlayerStrengthEstimate.partial"), context.analyzed, Math.max(context.total, context.analyzed))
      : "";
  if (!shown) return null;
  const banner = el("div", "ps-note ps-banner");
  const note = text(el("p", "", shown), "banner");
  note.setAttribute("role", "status");
  banner.append(note);
  if (partial) {
    banner.append(tagged(toolButton(t("PlayerStrengthEstimate.empty.analysis.action"), () => void analyzeGame(d)), "analyzeRest"));
  }
  return banner;
}

/** The HumanSL strip: 分析棋局时自动准备 (its tooltip says what it costs), 开始计算 for HumanSL 40,
 *  the preparation status. */
function humanSlStrip(d: Dashboard, report: StrengthReport): HTMLElement {
  const strip = el("div", "ps-strip");
  const toggle = backgroundToggle(d, "background");
  toggle.title = t("HumanSL40.backgroundToggle.description");
  strip.append(toggle);
  if (report.model.human_sl) strip.append(tagged(toolButton(t("HumanSL40.manualRun"), () => void prepareNow(d)), "run"));
  const view = state.view?.strength;
  const status = el("span", "ps-strip-status", view ? preparationStatus(view) : "");
  status.setAttribute("role", "status");
  strip.append(status);
  d.statuses.push(status);
  return strip;
}

/** StrengthDashboard.showReport: `report` in the open dialog. An empty page whose stage stays
 *  only updates its progress; otherwise the toolbar, the figures or the empty page, and the strip
 *  are built again, keeping the selected tab. */
function build(d: Dashboard, report: StrengthReport, context: StrengthContext, phases: StrengthPhases): void {
  const previous = d.shown;
  d.shown = { report, context, phases };
  const stage = context.stage;
  const pageKey = stage.type === "NeedsHumanStyle" ? `NeedsHumanStyle:${context.human_style_download ?? ""}` : stage.type;
  if (d.page && previous?.report.model.id === report.model.id && d.page.stage === pageKey) {
    d.page.update(context);
    return;
  }
  const root = d.content;
  root.replaceChildren();
  d.page = null;
  d.human = null;
  d.statuses = [];
  d.backgrounds = [];
  d.phases = null;
  const ready = stage.type === "Ready";

  const cardHost = el("div", "ps-cards");
  let assessment: Assessment | null = null;
  let cards: Record<Tab, HTMLElement> | null = null;
  if (ready) {
    assessment = assessmentDashboard(report);
    d.phases = phasesPanel(phases, phaseActions(d));
    cards = {
      assessment: assessment.panel,
      match: matchDashboard(report),
      performance: performancePanel(report),
      phases: d.phases.element,
    };
    cardHost.append(cards.assessment, cards.match, cards.performance, cards.phases);
  } else {
    const shown = stage.type === "NeedsHumanStyle" ? humanStylePage(d, context) : analysisPage(d, context);
    d.page = shown.page;
    cardHost.append(shown.element);
  }

  const subtitle = el("p", "ps-subtitle");
  const subtitles: Record<Tab, () => string> = {
    assessment: () => t("PlayerStrengthEstimate.subtitle.assessment"),
    match: () => t("PlayerStrengthEstimate.subtitle.match"),
    performance: () => t("PlayerStrengthEstimate.subtitle.performance"),
    phases: () => t("PlayerStrengthEstimate.subtitle.phases"),
  };
  const tabs: Record<Tab, HTMLButtonElement> = {
    assessment: tabButton(t("PlayerStrengthEstimate.tab.assessment")),
    match: tabButton(t("PlayerStrengthEstimate.tab.match")),
    performance: tabButton(t("PlayerStrengthEstimate.tab.performance")),
    phases: tabButton(t("PlayerStrengthEstimate.tab.phases")),
  };
  const tabRow = el("div", "ps-tabs");
  tabRow.setAttribute("role", "tablist");
  // 分阶段 after the existing three (Java has it second).
  const order: Tab[] = ["assessment", "match", "performance", "phases"];
  tabRow.append(...order.map((key) => tabs[key]));
  const reveal = scrollingTabs(tabRow);
  const show = (tab: Tab) => {
    d.tab = tab;
    subtitle.textContent = subtitles[tab]();
    for (const key of order) {
      if (cards) cards[key].hidden = key !== tab;
      tabs[key].classList.toggle("selected", key === tab);
      tabs[key].setAttribute("aria-selected", String(key === tab));
    }
    reveal(tabs[tab]);
  };
  d.select = ready ? show : null;
  order.forEach((key, index) => {
    tabs[key].onclick = () => show(key);
    tabs[key].dataset.strength = `tab.${key}`;
    // The views wait for the figures.
    tabs[key].disabled = !ready;
    // StrengthTabs.bindArrow: ← / → choose the neighbouring tab, no wrapping round.
    tabs[key].addEventListener("keydown", (e) => {
      if (e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return;
      const next = order[index + (e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : NaN)];
      if (!next) return;
      e.preventDefault();
      tabs[next].focus();
      show(next);
    });
  });

  const actions = el("div", "ps-actions");
  const view = state.view?.strength;
  const models = view?.models.length ? view.models : [report.model];
  const detail = tagged(
    toolButton(
      t("PlayerStrengthEstimate.detailData"),
      () => openDetailDialog(report, d.tab === "match" ? "match" : d.tab === "phases" ? "phases" : "assessment", d.shown!.phases),
      "primary",
    ),
    "detail",
  );
  detail.disabled = !ready;
  actions.append(modelPicker(d, models, report.model), detail);
  const header = el("div", "ps-header");
  header.append(tabRow, subtitle, actions);

  // Header, banner and the tabs' panels; in a short window the part that scrolls (strength.css).
  const middle = el("div", "ps-middle");
  middle.append(header);
  const banner = ready ? analysisBanner(d, context) : null;
  if (banner) middle.append(banner);
  middle.append(cardHost);
  // The tab row's height keeps a control scrolled into view in a short window clear of it
  // (strength.css, .ps-middle's scroll-padding-top); the border box: the short window's padding
  // counts too. A new width may hide the shown tab: it scrolls back into view.
  const tabsHeight = new ResizeObserver(() => {
    if (!tabRow.isConnected) return tabsHeight.disconnect();
    const height = `${tabRow.offsetHeight}px`;
    if (middle.style.getPropertyValue("--ps-tabs-h") !== height) middle.style.setProperty("--ps-tabs-h", height);
    reveal(tabs[d.tab]);
  });
  tabsHeight.observe(tabRow, { box: "border-box" });
  const parts: HTMLElement[] = [middle];
  // The human-style page carries the strip's controls itself.
  if (stage.type !== "NeedsHumanStyle") parts.push(humanSlStrip(d, report));
  root.append(...parts);
  show(d.tab);
  // A rebuild removes the control that had the focus (the picker, 开始计算): the focus would
  // fall to the body, outside every dialog's Esc binding. Java's dialog keeps it.
  if (!document.activeElement?.closest("#dialog .modal-box")) ownerFocus();
  d.probe = {
    report,
    cards: assessment?.cards ?? null,
    notes: { assessment: assessment?.note ?? "", match: ready ? t("PlayerStrengthEstimate.match.reviewNote") : "", performance: "", phases: "" },
  };
}

/** The phases view's way to the core (StrengthDashboard.phasesView's onCommit): the phases at a
 *  dragged divider's draft, and the kept boundaries, which later reports and games use. */
function phaseActions(d: Dashboard): PhaseActions {
  return {
    async preview(bounds) {
      const result = await run({ type: "Strength", op: { type: "PreviewPhases", bounds } });
      return dashboard === d && result?.type === "StrengthPhases" ? result.phases : null;
    },
    async commit(bounds) {
      // A report asked for before these boundaries are kept has the old ones: its answer is dropped
      // and the report asked for again once they are kept.
      const stale = d.loading;
      if (stale) {
        d.request++;
        d.loading = false;
      }
      const result = await run({ type: "Strength", op: { type: "SetPhaseBounds", bounds } });
      if (dashboard !== d) return null;
      if (stale) void refresh(d);
      if (result?.type !== "StrengthPhases") return null;
      if (d.shown) d.shown.phases = result.phases;
      return result.phases;
    },
    holding(on) {
      d.holding = on;
      // A refresh skipped while the divider was held runs now.
      if (!on && dashboard === d) checkStamp(d);
    },
  };
}

/** StrengthDialog.refresh: a new estimate once the analysis or the HumanSL data behind it has
 *  changed: always while the human-style model is chosen, the figures are not ready or analysis
 *  runs, and as soon as analysis starts or ends; while analysis runs, shown figures every third
 *  tick. The model menu is left alone while it is open. */
function checkStamp(d: Dashboard): void {
  const view = state.view?.strength;
  if (!view || !d.shown || d.comboOpen || d.holding) return;
  const analyzing = view.analyzing;
  const switched = analyzing !== d.analyzing;
  d.analyzing = analyzing;
  const ready = d.shown.context.stage.type === "Ready";
  const humanSl = view.models.find((m) => m.id === view.active)?.human_sl ?? false;
  if (!analyzing && !switched && ready && !humanSl) return;
  if (view.stamp === d.stamp && !switched) return;
  if (analyzing && ready && !switched && ++d.quiet < REBUILD_TICKS_WHILE_ANALYZING) return;
  d.quiet = 0;
  void refresh(d);
}

registerViewHandler((view) => {
  const d = dashboard;
  if (!d) return;
  const status = preparationStatus(view.strength);
  for (const label of d.statuses) if (label.textContent !== status) label.textContent = status;
  for (const box of d.backgrounds) if (box.checked !== view.strength.background) box.checked = view.strength.background;
  showHumanStatus(d, view.strength);
});

/** Client-area centre of a shown control, else null; for a tab the tab row has partly scrolled
 *  out, of the part in view (where a click lands on it). */
function centre(e: Element | null | undefined): { x: number; y: number } | null {
  const r = e?.getBoundingClientRect();
  if (!r || r.width <= 0 || r.height <= 0) return null;
  const row = e instanceof HTMLElement && dashboard ? acrossScroller(e, dashboard.win.box) : null;
  let left = r.left;
  let right = r.right;
  if (row) {
    const view = row.getBoundingClientRect().left + row.clientLeft;
    left = Math.max(left, view);
    right = Math.min(right, view + row.clientWidth);
  }
  const x = right > left ? (left + right) / 2 : r.left + r.width / 2;
  return { x: Math.round(x), y: Math.round(r.top + r.height / 2) };
}

/** Acceptance probe (read-only): what the 棋力评估 dialogs show and where their controls are.
 *  null while neither the dashboard nor the HumanSL 40 progress dialog is open. */
export function strengthReport() {
  const d = dashboard;
  const progress = progressReport();
  if (!d && !progress) return null;
  const probe = d?.probe ?? null;
  const tab = d?.tab ?? "assessment";
  const controls: { key: string; x: number; y: number; disabled: boolean }[] = [];
  const add = (key: string, e: Element | null | undefined) => {
    const c = centre(e);
    if (c) controls.push({ key, ...c, disabled: e instanceof HTMLButtonElement || e instanceof HTMLInputElement ? e.disabled : false });
  };
  const texts: Record<string, string> = {};
  const helps: { key: string; text: string; x: number; y: number }[] = [];
  /** Shown texts and controls cut off: wider than their box, or past the dialog's sides. */
  const clipped: string[] = [];
  /** Shown texts and controls of a row that scrolls across only (the tab row), narrower than it and
   *  partly or wholly scrolled out of its view: scrolling the row shows them whole. */
  const scrolledOut: string[] = [];
  const boxes = [d?.win.box, openDetail?.box].filter((b): b is HTMLElement => !!b);
  for (const box of boxes) {
    const frame = box.getBoundingClientRect();
    for (const e of box.querySelectorAll<HTMLElement>("[data-strength-text], [data-strength]")) {
      const r = e.getBoundingClientRect();
      if (r.width === 0 && r.height === 0) continue;
      const key = e.dataset.strengthText ?? e.dataset.strength!;
      if (e.dataset.strengthText) texts[key] = e.textContent ?? "";
      // The model menu may pass the dialog's edges, not the window's.
      const bound = e.closest(".ps-combo-list") ? { left: 0, right: innerWidth } : frame;
      const cut = e.scrollWidth > e.clientWidth + 1 || r.left < bound.left - 0.5 || r.right > bound.right + 0.5;
      const row = acrossScroller(e, box);
      if (row) {
        const left = row.getBoundingClientRect().left + row.clientLeft;
        const hidden = r.left < left - 0.5 || r.right > left + row.clientWidth + 0.5;
        const whole = e.scrollWidth <= e.clientWidth + 1 && r.width <= row.clientWidth + 0.5;
        if (hidden || (cut && whole)) scrolledOut.push(key);
        if (cut && whole) continue;
      }
      if (cut) clipped.push(key);
    }
    for (const e of box.querySelectorAll<HTMLElement>("[data-help]")) {
      const c = centre(e);
      if (c) helps.push({ key: e.dataset.help!, text: e.title, ...c });
    }
  }
  if (d) {
    for (const e of d.win.box.querySelectorAll<HTMLElement>("[data-strength]")) add(e.dataset.strength!, e);
    add("dashboard.close", d.win.box.querySelector(".modal-close"));
  }
  if (openDetail) add("detail.close", openDetail.box.querySelector(".modal-close"));
  if (progress?.cancel) controls.push({ key: "progress.cancel", ...progress.cancel, disabled: false });
  const context = d?.shown?.context ?? null;
  return {
    open: !!d,
    tab,
    model: probe?.report.model.id ?? "",
    stage: context?.stage ?? null,
    analyzed: context?.analyzed ?? 0,
    total: context?.total ?? 0,
    analyzing: context?.analyzing ?? false,
    ranks: { black: probe?.cards?.black.rank ?? "", white: probe?.cards?.white.rank ?? "" },
    confidence: { black: probe?.cards?.black.confidence ?? "", white: probe?.cards?.white.confidence ?? "" },
    metrics: { black: probe?.cards?.black.metrics ?? [], white: probe?.cards?.white.metrics ?? [] },
    note: probe?.notes[tab] ?? "",
    phases: d?.phases?.probe() ?? null,
    // Every shown 段位标尺: on 测评 a side's, on 分阶段 a phase's, with what it drew.
    rulers: d
      ? [...d.win.box.querySelectorAll<HTMLCanvasElement>("canvas.ps-ruler")]
          .filter((c) => c.getClientRects().length > 0)
          .map((c) => {
            const r = c.getBoundingClientRect();
            return {
              side: c.closest<HTMLElement>("[data-side]")?.dataset.side ?? null,
              phase: c.closest<HTMLElement>("[data-phase]")?.dataset.phase ?? null,
              rect: { x: r.left, y: r.top, width: r.width, height: r.height },
              drawn: rulerDrawn(c),
            };
          })
      : [],
    status: d?.statuses[0]?.textContent ?? "",
    background: d?.backgrounds[0]?.checked ?? false,
    comboOpen: d?.comboOpen ?? false,
    detailOpen: openDetail?.kind ?? null,
    progress: progress && { status: progress.status, determinate: progress.determinate, value: progress.value },
    texts,
    helps,
    clipped,
    scrolledOut,
    // The tab row: whether it scrolls across (too narrow for one row) and which fades it shows.
    tabRow: (() => {
      const row = d?.win.box.querySelector<HTMLElement>(".ps-tabs");
      return row
        ? { scrolls: row.scrollWidth > row.clientWidth, fadeStart: row.classList.contains("more-start"), fadeEnd: row.classList.contains("more-end") }
        : null;
    })(),
    controls,
  };
}
