// 棋力评估 (LizzieFrame.showPlayerStrengthEstimate 17271-17300, buildPlayerStrengthDashboardPanel
// 17417-17566): the modal dashboard with its header (mark, title, subtitle, 测评 / 吻合度 / 发挥水准
// tabs, model combo, 导入, 详细数据), the three dashboards and the HumanSL strip. The core computes
// the report; a model change, an import, a HumanSL run or a changed HumanSL 40 stamp rebuilds the
// dashboard from a fresh report like Java's dispose-and-reopen and 700 ms refresh.
import { registerViewHandler, run, state } from "../app";
import { chooseStrengthModel } from "../contract/client";
import type { StrengthModelView, StrengthReport, StrengthView } from "../contract/generated";
import { message } from "../dialogs";
import { format, t } from "../i18n";
import { openDetail, openDetailDialog } from "./detail";
import { humanSlText, javaFormat, modelDisplayName } from "./format";
import { prepareHumanSl, progressReport } from "./humanSl";
import { DASH, drawRound, ff, fillRound, fittedText, infoBadge, painted, rgb, setFont, text as drawText, width, type Painted } from "./paint";
import { assessmentCard, headerMark, matchSummaryCard, moveHitMap, noteStrip, paintDashboardRoot, performancePanel, type CardTexts } from "./panels";
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

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text) e.textContent = text;
  return e;
}

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

/** PlayerStrengthDetailButton: the compact 详细数据 button with the "!" badge, or 导入 with the gold
 *  plus mark; painted with its rollover and focus states. */
function detailButton(text: string, compact: boolean, onClick: () => void): HTMLButtonElement {
  const button = el("button", "ps-detail-button");
  button.type = "button";
  button.setAttribute("aria-label", text);
  let rollover = false;
  let focused = false;
  const face: Painted = painted("ps-detail-face", (g, w, h) => {
    fillRound(g, 0, 0, w - 1, h - 1, 22, rollover ? rgb(238, 228, 207, 238) : rgb(242, 235, 222, 210));
    drawRound(g, 0, 0, w - 1, h - 1, 22, rollover ? rgb(183, 129, 45, 160) : rgb(207, 194, 170, 130));
    if (focused) drawRound(g, 2, 2, w - 5, h - 5, 18, rgb(120, 82, 26), 2);
    if (compact) infoBadge(g, 12, 9, 22, 24, 0.95);
    else {
      const mid = Math.trunc(h / 2);
      fillRound(g, 13, mid - 7, 18, 14, 8, rgb(183, 129, 45, 170));
      const stroke = rgb(255, 253, 247, 220);
      g.strokeStyle = stroke;
      g.lineWidth = 1;
      g.beginPath();
      g.moveTo(20.5, mid - 3 + 0.5);
      g.lineTo(24.5, mid - 3 + 0.5);
      g.moveTo(22.5, mid - 5 + 0.5);
      g.lineTo(22.5, mid - 1 + 0.5);
      g.stroke();
    }
    fittedText(g, text, compact ? 42 : 40, Math.trunc(h / 2) + 5, w - (compact ? 52 : 48), DASH.ACCENT_DARK, true, ff() + 1);
  });
  button.append(face.element);
  // setPreferredSize(max(112 | 104, icon and padding + the text width), 42).
  const probe = document.createElement("canvas").getContext("2d")!;
  setFont(probe, true, ff() + 1);
  button.style.width = `${Math.max(compact ? 112 : 104, (compact ? 66 : 62) + width(probe, text))}px`;
  const look = (r: boolean, f: boolean) => {
    rollover = r;
    focused = f;
    face.repaint();
  };
  button.addEventListener("mouseenter", () => look(true, focused));
  button.addEventListener("mouseleave", () => look(false, focused));
  button.addEventListener("focus", () => look(rollover, true));
  button.addEventListener("blur", () => look(rollover, false));
  button.onclick = onClick;
  return button;
}

/** PlayerStrengthModelCombo: 棋力模型 over the selected name with the arrow, and its popup list
 *  (PlayerStrengthModelComboRenderer rows). */
function modelCombo(d: Dashboard, models: StrengthModelView[], selected: StrengthModelView, onSelect: (m: StrengthModelView) => void): HTMLElement {
  const host = el("div", "ps-combo");
  const button = el("button", "ps-combo-button");
  button.type = "button";
  button.title = t("PlayerStrengthEstimate.model.tooltip");
  button.setAttribute("aria-label", t("PlayerStrengthEstimate.model"));
  button.setAttribute("aria-haspopup", "listbox");
  button.setAttribute("aria-expanded", "false");
  let rollover = false;
  button.dataset.strength = "model.combo";
  let focused = false;
  const face = painted("ps-combo-face", (g, w, h) => {
    const open = d.comboOpen;
    fillRound(g, 1, 2, w - 2, h - 2, 23, rgb(120, 82, 35, rollover ? 28 : 16));
    fillRound(g, 0, 0, w - 1, h - 2, 23, open || rollover ? rgb(248, 239, 221, 242) : rgb(255, 252, 245, 226));
    drawRound(g, 0, 0, w - 1, h - 2, 23, open || rollover ? rgb(183, 129, 45, 185) : rgb(207, 194, 170, 150));
    drawText(g, t("PlayerStrengthEstimate.model"), 16, 15, DASH.MUTED_TEXT, false, Math.max(10, ff() - 1));
    fittedText(g, modelDisplayName(selected), 16, 32, Math.max(40, w - 56), DASH.TEXT, true, ff() + 1);
    const ax = w - 28;
    const ay = Math.trunc(h / 2) + 2;
    g.fillStyle = DASH.ACCENT_DARK;
    g.beginPath();
    g.moveTo(ax - 5, ay - 3);
    g.lineTo(ax + 5, ay - 3);
    g.lineTo(ax, ay + 4);
    g.closePath();
    g.fill();
    // The focused combo shows the look-and-feel focus rectangle inside its face.
    if (focused) drawRound(g, 3, 3, w - 7, h - 8, 18, rgb(120, 82, 26), 1);
  });
  button.append(face.element);
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
  list.style.maxHeight = `calc(${Math.min(8, Math.max(3, models.length))} * var(--ps-combo-row))`;
  host.append(button, list);

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
    face.repaint();
    // A refresh skipped while the popup was open runs now.
    if (!open && dashboard === d) checkStamp(d);
  }
  function choose(k: number): void {
    setOpen(false);
    const model = models[k];
    if (model) onSelect(model);
  }
  button.onclick = () => setOpen(!d.comboOpen);
  button.addEventListener("mouseenter", () => { rollover = true; face.repaint(); });
  button.addEventListener("mouseleave", () => { rollover = false; face.repaint(); });
  button.addEventListener("focus", () => { focused = true; face.repaint(); });
  button.addEventListener("blur", () => { focused = false; setOpen(false); face.repaint(); });
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

/** A dashboard card filling the card area; the painted panel fills it. */
function filled(p: Painted, cls: string): HTMLElement {
  const box = el("div", `ps-fill ${cls}`);
  box.append(p.element);
  return box;
}

/** buildPlayerStrengthAssessmentDashboard, with the cards' painted texts and the note. */
function assessmentDashboard(report: StrengthReport): { panel: HTMLElement; cards: { black: CardTexts; white: CardTexts }; note: string } {
  const humanSl = report.model.human_sl;
  const panel = el("div", "ps-dash ps-dash-assessment");
  const cards = el("div", "ps-two ps-gap-20");
  const black = assessmentCard(true, report.black, humanSl);
  const white = assessmentCard(false, report.white, humanSl);
  cards.append(filled(black, "ps-card"), filled(white, "ps-card"));
  const note = humanSl
    ? report.enough_data ? t("HumanSL40.reviewNote") : t("HumanSL40.unavailable")
    : report.enough_data ? t("PlayerStrengthEstimate.reviewOnlyNote") : t("PlayerStrengthEstimate.needMoreData");
  panel.append(cards, filled(noteStrip(note, false), "ps-note-strip"));
  return { panel, cards: { black: black.drawn, white: white.drawn }, note };
}

/** Names a control for the acceptance probe. */
function tagged<T extends HTMLElement>(e: T, key: string): T {
  e.dataset.strength = key;
  return e;
}

/** buildPlayerStrengthMatchDashboard. */
function matchDashboard(report: StrengthReport): HTMLElement {
  const panel = el("div", "ps-dash ps-dash-match");
  const top = el("div", "ps-two ps-gap-18 ps-match-top");
  top.append(filled(matchSummaryCard(true, report.black.summary), "ps-card"), filled(matchSummaryCard(false, report.white.summary), "ps-card"));
  panel.append(top, filled(moveHitMap(report), "ps-hit-map-box"), filled(noteStrip(t("PlayerStrengthEstimate.match.reviewNote"), false), "ps-note-strip"));
  return panel;
}

/** Builds the panel for `report` into the open dialog (buildPlayerStrengthDashboardPanel). */
function build(d: Dashboard, report: StrengthReport): void {
  const view = state.view?.strength;
  const root = d.content;
  root.replaceChildren();
  const background = painted("ps-root-bg", paintDashboardRoot);
  root.append(background.element);

  const assessment = assessmentDashboard(report);
  const cards: Record<Tab, HTMLElement> = {
    assessment: assessment.panel,
    match: matchDashboard(report),
    performance: filled(performancePanel(report), "ps-dash ps-dash-performance"),
  };
  const cardHost = el("div", "ps-cards");
  cardHost.append(cards.assessment, cards.match, cards.performance);

  const subtitle = el("div", "ps-subtitle");
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

  const titleRow = el("div", "ps-title-row");
  titleRow.append(headerMark().element, el("div", "ps-title", t("PlayerStrengthEstimate.title")));
  const tabRow = el("div", "ps-tabs");
  tabRow.setAttribute("role", "tablist");
  tabRow.append(tabs.assessment, tabs.match, tabs.performance);
  const titleBlock = el("div", "ps-title-block");
  titleBlock.append(titleRow, subtitle, tabRow);

  const actions = el("div", "ps-actions");
  const models = view?.models.length ? view.models : [report.model];
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
    tagged(detailButton(t("PlayerStrengthEstimate.detailData"), true, () => openDetailDialog(report, current === "match")), "detail"),
  );
  const header = el("div", "ps-header");
  header.append(titleBlock, actions);

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
    const manual = el("button", "ps-native-button", t("HumanSL40.manualRun"));
    manual.type = "button";
    manual.onclick = async () => {
      await prepareHumanSl();
      if (dashboard === d) await refresh(d);
    };
    manual.dataset.strength = "run";
    strip.append(manual);
  }
  const status = el("span", "ps-strip-status", view ? preparationStatus(view) : "");
  strip.append(status);
  d.statusLabel = status;
  d.background = box;

  root.append(header, cardHost, strip);
  show("assessment");
  alignSubtitle(titleRow, subtitle, tabRow);
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

/** BoxLayout(Y_AXIS) of the title block: the rows (alignment 0.5) are centred on one axis and the
 *  subtitle (LEFT_ALIGNMENT) starts on it, half the wider row's width from the left. */
function alignSubtitle(titleRow: HTMLElement, subtitle: HTMLElement, tabRow: HTMLElement): void {
  const preferred = (row: HTMLElement) => [...row.children].reduce((sum, c) => sum + (c as HTMLElement).offsetWidth, 0);
  // titleRow is a FlowLayout(LEFT, 12, 0): 12 px before, between and after its two parts.
  const axis = Math.max(preferred(titleRow) + 36, preferred(tabRow) + 24) / 2;
  subtitle.style.marginLeft = `${Math.trunc(axis)}px`;
}

/** createPlayerStrengthImportModelButton. */
function importButton(d: Dashboard): HTMLButtonElement {
  const title = t("PlayerStrengthEstimate.importModel.title");
  const button = detailButton(t("PlayerStrengthEstimate.importModel"), false, async () => {
    const path = await chooseStrengthModel(title).catch((e: unknown) => {
      void message("Error", String(e), "error");
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
