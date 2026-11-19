// 棋力评估 (LizzieFrame.showPlayerStrengthEstimate 17271-17300, buildPlayerStrengthDashboardPanel
// 17417-17566): the modal dashboard with its toolbar (测评 / 吻合度 / 发挥水准 tabs and their
// subtitle, model combo, 导入, 详细数据), the three dashboards and the HumanSL strip. The core
// computes the report; a model change, an import, a HumanSL run or a changed HumanSL 40 stamp
// rebuilds the dashboard from a fresh report like Java's dispose-and-reopen and 700 ms refresh.
import { registerViewHandler, run, state } from "../app";
import { chooseStrengthModel } from "../contract/client";
import type { StrengthModelView, StrengthReport, StrengthView } from "../contract/generated";
import { icon } from "../design/icons";
import { message } from "../dialogs";
import { format, t } from "../i18n";
import { openDetail, openDetailDialog } from "./detail";
import { humanSlText, javaFormat, modelDisplayName } from "./format";
import { prepareHumanSl, progressReport } from "./humanSl";
import { assessmentSection, el, legend, matchSummary, moveHitMap, noteStrip, performancePanel, type CardTexts } from "./panels";
import { openWindow, ownerFocus, type StrengthWindow } from "./window";
import "./strength.css";

type Tab = "assessment" | "match" | "performance";

interface Dashboard {
  win: StrengthWindow;
  content: HTMLElement;
  /** The strength view's stamp of the shown report (installHumanSlDashboardRefresh's previous). */
  stamp: number;
  /** The newest report request; older answers are dropped. */
  request: number;
  comboOpen: boolean;
  statusLabel: HTMLElement | null;
  background: HTMLInputElement | null;
  /** What the built dashboard shows (acceptance probe). */
  probe: Probe | null;
}

interface Probe {
  report: StrengthReport;
  tab: () => Tab;
  cards: { black: CardTexts; white: CardTexts };
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

/** The menu, the toolbar button and function search open the dashboard for the active model. */
export async function openStrengthDashboard(): Promise<void> {
  if (dashboard) {
    dashboard.win.box.focus();
    return;
  }
  const result = await run({ type: "Strength", op: { type: "Report" } });
  if (result?.type !== "Strength" || dashboard) return;
  const content = el("div", "ps-root");
  // installHumanSlDashboardRefresh's 700 ms timer, stopped when the window closes.
  const timer = setInterval(() => checkStamp(d), 700);
  const d: Dashboard = {
    win: openWindow(t("PlayerStrengthEstimate.title"), "ps-dashboard", () => {
      clearInterval(timer);
      if (dashboard === d) dashboard = null;
    }),
    content,
    stamp: state.view?.strength.stamp ?? 0,
    request: 0,
    comboOpen: false,
    statusLabel: null,
    background: null,
    probe: null,
  };
  dashboard = d;
  d.win.body.append(content);
  build(d, result.report);
}

/** Re-requests the report and rebuilds the dashboard (Java disposes and reopens it). */
async function refresh(d: Dashboard): Promise<void> {
  const request = ++d.request;
  d.stamp = state.view?.strength.stamp ?? d.stamp;
  const result = await run({ type: "Strength", op: { type: "Report" } });
  if (dashboard !== d || request !== d.request || result?.type !== "Strength") return;
  build(d, result.report);
}

/** A toolbar button of the dashboard (导入, 详细数据). */
function toolButton(text: string, onClick: () => void): HTMLButtonElement {
  const button = el("button", "", text);
  button.type = "button";
  button.onclick = onClick;
  return button;
}

/** PlayerStrengthModelCombo: 棋力模型 before a button with the selected name and the arrow, and
 *  its popup list (PlayerStrengthModelComboRenderer rows). */
function modelCombo(d: Dashboard, models: StrengthModelView[], selected: StrengthModelView, onSelect: (m: StrengthModelView) => void): HTMLElement {
  const host = el("div", "ps-combo");
  const caption = el("span", "ps-label", t("PlayerStrengthEstimate.model"));
  const anchor = el("div", "ps-combo-anchor");
  const button = el("button", "ps-combo-button");
  button.type = "button";
  button.title = t("PlayerStrengthEstimate.model.tooltip");
  button.setAttribute("aria-label", t("PlayerStrengthEstimate.model"));
  button.setAttribute("aria-haspopup", "listbox");
  button.setAttribute("aria-expanded", "false");
  button.dataset.strength = "model.combo";
  button.append(el("span", "ps-combo-value", modelDisplayName(selected)), icon("chevron-down"));
  const list = el("div", "ps-combo-list");
  list.setAttribute("role", "listbox");
  list.hidden = true;
  let active = Math.max(0, models.findIndex((m) => m.id === selected.id));
  const rows = models.map((model, k) => {
    const row = el("div", "ps-combo-row", modelDisplayName(model));
    row.dataset.strength = `model.option.${model.id}`;
    row.setAttribute("role", "option");
    row.addEventListener("mouseenter", () => mark(k));
    row.addEventListener("mousedown", (e) => e.preventDefault());
    row.onclick = () => choose(k);
    return row;
  });
  list.append(...rows);
  // setMaximumRowCount(min(8, max(3, model count))).
  list.style.maxHeight = `calc(${Math.min(8, Math.max(3, models.length))} * var(--ps-combo-row) + 8px)`;
  anchor.append(button, list);
  host.append(caption, anchor);

  function mark(k: number): void {
    active = k;
    rows.forEach((r, j) => {
      r.classList.toggle("selected", j === k);
      r.setAttribute("aria-selected", String(j === k));
    });
  }
  function setOpen(open: boolean): void {
    if (d.comboOpen === open) return;
    d.comboOpen = open;
    list.hidden = !open;
    button.setAttribute("aria-expanded", String(open));
    if (open) mark(Math.max(0, models.findIndex((m) => m.id === selected.id)));
    // A refresh skipped while the popup was open runs now.
    if (!open && dashboard === d) checkStamp(d);
  }
  function choose(k: number): void {
    setOpen(false);
    const model = models[k];
    if (model) onSelect(model);
  }
  button.onclick = () => setOpen(!d.comboOpen);
  button.addEventListener("blur", () => setOpen(false));
  button.addEventListener("keydown", (e) => {
    if (e.key === "ArrowDown" || e.key === "ArrowUp") {
      e.preventDefault();
      if (!d.comboOpen) setOpen(true);
      else mark(Math.max(0, Math.min(models.length - 1, active + (e.key === "ArrowDown" ? 1 : -1))));
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

function tabButton(text: string): HTMLButtonElement {
  const b = el("button", "ps-tab", text);
  b.type = "button";
  b.setAttribute("role", "tab");
  return b;
}

/** buildPlayerStrengthAssessmentDashboard: the two sides and the note. */
function assessmentDashboard(report: StrengthReport): { panel: HTMLElement; cards: { black: CardTexts; white: CardTexts }; note: string } {
  const humanSl = report.model.human_sl;
  const panel = el("div", "ps-dash ps-dash-assessment");
  const sides = el("div", "ps-two");
  const black = assessmentSection(true, report.black, humanSl);
  const white = assessmentSection(false, report.white, humanSl);
  sides.append(black.element, white.element);
  const note = humanSl
    ? report.enough_data ? t("HumanSL40.reviewNote") : t("HumanSL40.unavailable")
    : report.enough_data ? t("PlayerStrengthEstimate.reviewOnlyNote") : t("PlayerStrengthEstimate.needMoreData");
  panel.append(sides, noteStrip(note));
  return { panel, cards: { black: black.drawn, white: white.drawn }, note };
}

/** Names a control for the acceptance probe. */
function tagged<T extends HTMLElement>(e: T, key: string): T {
  e.dataset.strength = key;
  return e;
}

/** buildPlayerStrengthMatchDashboard: the two sides' match figures, 选点命中图 with its legend,
 *  the note. */
function matchDashboard(report: StrengthReport): HTMLElement {
  const panel = el("div", "ps-dash ps-dash-match");
  const top = el("div", "ps-two ps-match-top");
  top.append(matchSummary(true, report.black.summary), matchSummary(false, report.white.summary));
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

/** Builds the panel for `report` into the open dialog (buildPlayerStrengthDashboardPanel): the
 *  toolbar, the shown dashboard and the HumanSL strip, divided by hairlines. */
function build(d: Dashboard, report: StrengthReport): void {
  const view = state.view?.strength;
  const root = d.content;
  root.replaceChildren();

  const assessment = assessmentDashboard(report);
  const cards: Record<Tab, HTMLElement> = {
    assessment: assessment.panel,
    match: matchDashboard(report),
    performance: performancePanel(report),
  };
  const cardHost = el("div", "ps-cards");
  cardHost.append(cards.assessment, cards.match, cards.performance);

  const subtitle = el("p", "ps-subtitle");
  let current: Tab = "assessment";
  const subtitles: Record<Tab, () => string> = {
    assessment: () => t("PlayerStrengthEstimate.subtitle.assessment"),
    match: () => t("PlayerStrengthEstimate.subtitle.match"),
    performance: () => t("PlayerStrengthEstimate.subtitle.performance"),
  };
  const tabs: Record<Tab, HTMLButtonElement> = {
    assessment: tabButton(t("PlayerStrengthEstimate.tab.assessment")),
    match: tabButton(t("PlayerStrengthEstimate.tab.match")),
    performance: tabButton(t("PlayerStrengthEstimate.tab.performance")),
  };
  const show = (tab: Tab) => {
    current = tab;
    subtitle.textContent = subtitles[tab]();
    for (const key of Object.keys(cards) as Tab[]) {
      cards[key].hidden = key !== tab;
      tabs[key].classList.toggle("selected", key === tab);
      tabs[key].setAttribute("aria-selected", String(key === tab));
    }
  };
  for (const key of Object.keys(tabs) as Tab[]) {
    tabs[key].onclick = () => show(key);
    tabs[key].dataset.strength = `tab.${key}`;
  }
  const tabRow = el("div", "ps-tabs");
  tabRow.setAttribute("role", "tablist");
  tabRow.append(tabs.assessment, tabs.match, tabs.performance);

  const actions = el("div", "ps-actions");
  const models = view?.models.length ? view.models : [report.model];
  const detail = toolButton(t("PlayerStrengthEstimate.detailData"), () => openDetailDialog(report, current === "match"));
  detail.classList.add("primary");
  actions.append(
    modelCombo(d, models, report.model, (model) => {
      if (model.id !== state.view?.strength.active || model.human_sl) {
        void (async () => {
          await run({ type: "Strength", op: { type: "SelectModel", id: model.id } });
          if (dashboard === d) await refresh(d);
        })();
      }
    }),
    tagged(importButton(d), "import"),
    tagged(detail, "detail"),
  );
  const header = el("div", "ps-header");
  header.append(tabRow, subtitle, actions);

  // The HumanSL strip: 后台 HumanSL, 补算 HumanSL for HumanSL 40, the preparation status.
  const strip = el("div", "ps-strip");
  const toggle = el("label", "ps-check");
  const box = el("input");
  box.type = "checkbox";
  box.checked = view?.background ?? false;
  box.dataset.strength = "background";
  box.onchange = async () => {
    const result = await run({ type: "Strength", op: { type: "SetBackground", enabled: box.checked } });
    // A failed save shows its message and the box returns to the saved choice.
    if (result?.type !== "Done") box.checked = state.view?.strength.background ?? false;
  };
  toggle.append(box, t("HumanSL40.backgroundToggle"));
  strip.append(toggle);
  if (report.model.human_sl) {
    const manual = toolButton(t("HumanSL40.manualRun"), async () => {
      await prepareHumanSl();
      if (dashboard === d) await refresh(d);
    });
    manual.dataset.strength = "run";
    strip.append(manual);
  }
  const status = el("span", "ps-strip-status", view ? preparationStatus(view) : "");
  status.setAttribute("role", "status");
  strip.append(status);
  d.statusLabel = status;
  d.background = box;

  root.append(header, cardHost, strip);
  show("assessment");
  // A rebuild removes the control that had the focus (the combo, 导入, 补算 HumanSL): the focus
  // would fall to the body, outside every dialog's Esc binding. Java's reopened dialog is focused.
  if (!document.activeElement?.closest("#dialog .modal-box")) ownerFocus();
  d.probe = {
    report,
    tab: () => current,
    cards: assessment.cards,
    notes: { assessment: assessment.note, match: t("PlayerStrengthEstimate.match.reviewNote"), performance: "" },
  };
}

/** createPlayerStrengthImportModelButton. */
function importButton(d: Dashboard): HTMLButtonElement {
  const title = t("PlayerStrengthEstimate.importModel.title");
  const button = toolButton(t("PlayerStrengthEstimate.importModel"), async () => {
    const path = await chooseStrengthModel(title).catch((e: unknown) => {
      void message(t("Message.title"), String(e), "error");
      return null;
    });
    if (!path) return;
    const result = await run({ type: "Strength", op: { type: "ImportModel", path } });
    if (result?.type === "StrengthImportFailed") {
      await message(title, javaFormat(t("PlayerStrengthEstimate.importModel.error"), result.detail), "error");
    } else if (result?.type === "Done" && dashboard === d) await refresh(d);
  });
  button.title = t("PlayerStrengthEstimate.importModel.tooltip");
  return button;
}

/** installHumanSlDashboardRefresh: with HumanSL 40 active a changed stamp rebuilds the dashboard,
 *  unless the model popup is open. */
function checkStamp(d: Dashboard): void {
  const view = state.view?.strength;
  if (!view || d.comboOpen) return;
  const active = view.models.find((m) => m.id === view.active);
  if (!active?.human_sl || view.stamp === d.stamp) return;
  void refresh(d);
}

registerViewHandler((view) => {
  const d = dashboard;
  if (!d) return;
  if (d.statusLabel) d.statusLabel.textContent = preparationStatus(view.strength);
  if (d.background && d.background.checked !== view.strength.background) d.background.checked = view.strength.background;
});

/** Client-area centre of a shown control, else null. */
function centre(e: Element | null | undefined): { x: number; y: number } | null {
  const r = e?.getBoundingClientRect();
  return r && r.width > 0 && r.height > 0 ? { x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2) } : null;
}

/** Acceptance probe (read-only): what the 棋力评估 dialogs show and where their controls are.
 *  null while neither the dashboard nor the HumanSL 40 progress dialog is open. */
export function strengthReport() {
  const d = dashboard;
  const progress = progressReport();
  if (!d && !progress) return null;
  const probe = d?.probe ?? null;
  const tab = probe?.tab() ?? "assessment";
  const controls: { key: string; x: number; y: number }[] = [];
  const add = (key: string, e: Element | null | undefined) => {
    const c = centre(e);
    if (c) controls.push({ key, ...c });
  };
  if (d) {
    for (const e of d.win.box.querySelectorAll<HTMLElement>("[data-strength]")) add(e.dataset.strength!, e);
    add("dashboard.close", d.win.box.querySelector(".ps-window-close"));
  }
  if (openDetail) add("detail.close", openDetail.box.querySelector(".ps-window-close"));
  if (progress?.cancel) controls.push({ key: "progress.cancel", ...progress.cancel });
  return {
    open: !!d,
    tab,
    model: probe?.report.model.id ?? "",
    ranks: { black: probe?.cards.black.rank ?? "", white: probe?.cards.white.rank ?? "" },
    confidence: { black: probe?.cards.black.confidence ?? "", white: probe?.cards.white.confidence ?? "" },
    metrics: { black: probe?.cards.black.metrics ?? [], white: probe?.cards.white.metrics ?? [] },
    note: probe?.notes[tab] ?? "",
    status: d?.statusLabel?.textContent ?? "",
    background: d?.background?.checked ?? false,
    comboOpen: d?.comboOpen ?? false,
    detailOpen: openDetail?.kind ?? null,
    progress: progress && { status: progress.status, determinate: progress.determinate, value: progress.value },
    controls,
  };
}
