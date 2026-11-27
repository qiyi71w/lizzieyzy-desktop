// Import of LizzieYzy Next (Java) settings: choose the Java settings folder, preview what is taken
// over, left out or not usable, confirm credentials and replacements, then read the report.
// Java has no counterpart; each step is grouped rows (engine/controls.ts) with the tables and
// lists as full-width content, and a footer: 预览 or 导入 with 取消 (跳过 for the first-use offer),
// then 关闭 under the report. The core reads and writes; this dialog only holds the user's choices
// until "导入".
import { run, state } from "./app";
import type {
  EngineProblem,
  ImportEngineView,
  ImportFailure,
  ImportItemView,
  ImportPreviewView,
  ImportReportView,
  ImportSlotView,
  ImportStatus,
  ImportTarget,
} from "./contract/generated";
import { formPanel, message } from "./dialogs";
import { product, t } from "./i18n";
import { button, checkbox, footer, groupTitle, input, primaryButton, row, switchRow } from "./engine/controls";
import "./javaImport.css";

let opened = false;

/** Help > 从 LizzieYzy Next 导入设置, and the first-use offer (`firstUse`). */
export async function openJavaImport(firstUse = false): Promise<void> {
  if (opened || !state.view) return;
  opened = true;
  const body = document.createElement("div");
  body.className = "java-import";
  const dialog = formPanel(product("importTitle"), body);
  let declined = firstUse;
  try {
    showSource(body, firstUse, dialog.close, () => (declined = false));
    await dialog.closed;
    // Closing the first-use offer without importing declines it.
    if (declined && state.view?.java_import.offer) await run({ type: "DismissJavaImportOffer" });
  } finally {
    opened = false;
  }
}

function text(tag: string, content: string, className = ""): HTMLElement {
  const el = document.createElement(tag);
  el.textContent = content;
  if (className) el.className = className;
  return el;
}

function failureText(f: ImportFailure): string {
  switch (f.type) {
    case "NoConfig": return product("importNoConfig", f.path);
    case "Invalid": return product("importInvalid", f.path, f.detail);
    case "Unreadable": return product("importUnreadable", f.path, f.detail);
  }
}

let names = 0;

/** A row whose `wide` part (a folder path, the folders found) takes the whole line under its name
 *  and `controls`; a path field is named by the row. */
function wideRow(host: HTMLElement, title: string, wide: HTMLElement, ...controls: HTMLElement[]): HTMLElement {
  const r = row(host, title, "", ...controls);
  r.classList.add("java-import-wide");
  const name = r.querySelector<HTMLElement>(".ds-row-title")!;
  name.id ||= `java-import-name-${++names}`;
  if (wide instanceof HTMLInputElement) wide.setAttribute("aria-labelledby", name.id);
  r.append(wide);
  return r;
}

/** A switch row for a choice the import makes only when asked, keyed `key`. */
function choice(host: HTMLElement, label: string, key: string): HTMLInputElement {
  const box = switchRow(host, label, "", false).input;
  box.dataset.key = key;
  return box;
}

/** Step 1: the Java settings folder. */
function showSource(body: HTMLElement, firstUse: boolean, close: () => Promise<void>, started: () => void, folder = "", program: string | null = null): void {
  body.replaceChildren();
  body.append(text("p", firstUse ? product("importFirstUse") : product("importIntro")));
  const path = input(folder || (state.view?.java_import.candidates[0] ?? ""));
  path.className = "java-import-path";
  path.dataset.key = "import-folder";
  wideRow(body, product("importFolder"), path, button(product("browse"), async () => {
    const r = await run({ type: "ChooseJavaFolder", program: false });
    if (r?.type === "FolderChosen") path.value = r.path;
  }));
  const candidates = state.view?.java_import.candidates ?? [];
  if (candidates.length) {
    const found = document.createElement("div");
    found.className = "java-import-found";
    candidates.forEach((c, k) => {
      const b = button(c, () => { path.value = c; });
      b.dataset.key = `import-candidate-${k}`;
      found.append(b);
    });
    wideRow(body, product("importFound").replace(/\s*[:：]\s*$/, ""), found);
  }
  const error = text("p", "", "java-import-error");
  error.setAttribute("role", "alert");
  body.append(error);
  const preview = primaryButton(product("importPreview"), async () => {
    error.textContent = "";
    if (!path.value.trim()) return;
    const r = await run({ type: "PreviewJavaImport", config_dir: path.value.trim(), program_dir: program });
    if (r?.type === "JavaImportFailed") error.textContent = failureText(r.failure);
    else if (r?.type === "JavaImportPreview") showPreview(body, r.preview, firstUse, close, started, path.value.trim());
  });
  preview.dataset.key = "import-preview";
  footer(body, { primary: preview, cancel: button(firstUse ? product("importSkip") : product("cancel"), close) });
  path.focus();
}

function count(items: ImportItemView[], status: ImportStatus): number {
  return items.filter((i) => i.status === status).length;
}

// Targets that are Java menu items take Java's own wording.
const JAVA_TARGET_LABELS: Partial<Record<ImportTarget, () => string>> = {
  PureNetwork: () => t("Menu.showHeatmap"),
  AppendWinrateToComment: () => t("Menu.appendWinrateToComment"),
  LiveAlwaysGotoLast: () => t("Menu.alwaysGoToLastMove"),
  LiveOpenHtml: () => t("Menu.EnableEnterYikeGame"),
  WinrateGraph: () => t("Menu.winrateGraphSettings"),
  KataEstimate: () => t("Menu.kataEstimate"),
  KataPda: () => t("Menu.chkPDA.toolTopText"),
  KataWrn: () => t("Menu.chkWRN.toolTopText"),
  // 设置KataGo引擎高级参数's 自动加载 and 在工具栏中控制 columns (SetKataEngines.java).
  KataParamAutoLoad: () => t("SetKataEngines.title"),
  KataParamToolbar: () => t("SetKataEngines.title"),
  LeelaOptions: () => t("SetLeelaEngines.title"),
  // KataGo 激进参数PDA设置 (SetKataPDA.java): mode, ratio / value and 自动加载.
  CustomPda: () => t("SetKataPDA.title"),
  Scoring: () => t("BottomToolbar.finalScore"),
  SuggestionsFrame: () => t("Menu.SuggestionList"),
  ExtraMode: () => t("Menu.layoutMode"),
  AutoAnalysis: () => t("StartAnaDialog.title"),
  HumanEngineGame: () => t("Menu.newGame"),
  GameAiTime: () => t("SetAiTimes.title"),
  GameRandomStart: () => t("SetAnaGameRandomStart.title"),
  AutoPlay: () => t("AutoPlay.title"),
  WholeGameVisits: () => t("WholeGameAnalysis.visits.label"),
  Hawkeye: () => t("Menu.hawkEye"),
  // Menu.java:1163 界面 > 面板, 1427 恢复默认面板尺寸, 4509 底部工具栏.
  WorkbenchPanels: () => t("Menu.panel"),
  PanelSizes: () => t("Menu.restoreDefaultPanelSizes"),
  BottomToolbar: () => t("Menu.bottomToolBar"),
  Appearance: () => t("Menu.appearanceTheme"),
  // AI 陪练's last-started settings (HumanSlTraining.title) and the HumanSL model (AutoSetup.humanSlModel).
  HumanSlTraining: () => t("HumanSlTraining.title"),
  HumanSlModel: () => t("AutoSetup.humanSlModel"),
  // AI 解说's teacher.properties (TeacherSettingsDialog title).
  Commentary: () => t("Teacher.settings.title"),
  // 综合设置 > 高级 > 高级与性能 and 日志与分析刷新 (ConfigDialog2.java:3253-3264, 3284-3295).
  LizzieCache: () => t("ConfigDialog2.modern.advanced.cache"),
  StopAtEmptyBoard: () => t("ConfigDialog2.modern.advanced.stopEmpty"),
  AlwaysGtp: () => t("ConfigDialog2.modern.engineHealth.alwaysGtp"),
  AutoCheckEngineAlive: () => t("ConfigDialog2.modern.engineHealth.checkAlive"),
  // 设置 > 同步时不播放落子声 and 同步 > 棋盘同步工具选项 (Menu.java:5547-5556, 4352-4392).
  NoSoundInSync: () => t("Menu.notPlaySoundInSync"),
  ReadBoardStatSync: () => t("Menu.alwaysKeepBoardStatSync"),
  ReadBoardGetFocus: () => t("Menu.readBoardGetFocus"),
  // The 不再提示 of LizzieFrame.showReadBoardWebSocketPonderingNotice.
  ReadBoardPonderingNotice: () => t("ReadBoard.websocketPonderingNotice.title"),
  Diagnostics: () => t("DiagnosticsDialog.title"),
  QuickLinks: () => t("Menu.quickLinks"),
  // 同步 > 野狐棋谱's account, 近期搜索 and 打开棋谱后此窗口.
  FoxKifu: () => t("Menu.foxKifu"),
  // 同步 > 腾讯棋谱's last query, 近期搜索 and 打开棋谱后此窗口.
  TencentKifu: () => t("Menu.tencentKifu"),
  // 引擎 > 远程算力 and 同步 > Web 旁观 (Menu.java remote compute and web board menus).
  Cloud: () => t("Menu.remoteCompute"),
  WebBoard: () => t("Menu.webBoard"),
  // 显示 menus and dialogs these settings belong to (Menu.java, ConfigDialog2, EnginePkConfig).
  Coordinates: () => t("Menu.coordsMenu"),
  NamesOnBoard: () => t("Menu.showNameInBoard"),
  MoveNumbers: () => t("Menu.moveMenu"),
  VariationTree: () => t("Menu.variationPaneSettings"),
  MoveRank: () => t("Menu.moveRankMenu"),
  Suggestions: () => t("Menu.Suggestions"),
  NextMoves: () => t("Menu.nextMoveHint"),
  SubBoard: () => t("Menu.subBoardSettings"),
  AutoQuickAnalysis: () => t("ConfigDialog2.modern.kifu.autoAnalyze"),
  AnalysisEngine: () => t("Menu.flashAnalyzeSettings"),
  QuickModel: () => t("AutoSetup.quickAnalysisModel"),
  StrengthBackground: () => t("HumanSL40.backgroundToggle"),
  StrengthPhaseBounds: () => `${t("PlayerStrengthEstimate.title")} · ${t("PlayerStrengthEstimate.phase.title")}`,
  StrengthModels: () => t("PlayerStrengthEstimate.model"),
  Tsumego: () => t("TsumeGoFrame.title"),
  CaptureTsumego: () => t("CaptureTsumeGoFrame.title"),
  TopToolbar: () => t("Menu.topToolBar"),
  // Menu.java:4897 底部工具栏 > 自定义内容.
  BottomButtons: () => t("Menu.customToolbar"),
  ProblemList: () => t("Menu.commitPane"),
  FlashRange: () => t("Menu.flashAnalyzePartGame"),
  EngineMatch: () => t("EnginePkConfig.title"),
  Contribution: () => t("Menu.contributeMenu"),
  // 综合设置 > 对局与操作 > 显示落子矩形 / 启用点击复盘 (ConfigDialog2.java:3168-3214).
  MoveRect: () => t("ConfigDialog2.modern.interaction.moveRect"),
  ClickReview: () => t("ConfigDialog2.modern.play.clickReview"),
  // The top toolbar's 自由涂鸦 button (Menu.java:8639-8643), whose layer opens with this pen.
  PaintingColor: () => t("Menu.drawPainting.toolTipText"),
  // 显示 > 黑白胜率大字's style, 窗口置顶 and bounds (BigWinrateSettings).
  BigWinrate: () => t("Menu.bigWinrateWindow"),
};

function targetLabel(target: ImportTarget): string {
  const java = JAVA_TARGET_LABELS[target];
  return java ? java() : product(`target${target}`);
}

function itemLine(i: ImportItemView): string {
  const parts = [i.key];
  if (i.target) parts.push(`→ ${targetLabel(i.target)}`);
  if (i.value !== null && !i.credential) parts.push(`= ${i.value.replaceAll("\n", ", ")}`);
  if (i.reason) parts.push(`（${product(`reason${i.reason}`)}）`);
  if (i.detail) parts.push(i.detail);
  return parts.join(" ");
}

function itemLists(host: HTMLElement, items: ImportItemView[], report: boolean): void {
  for (const status of ["Imported", "Incompatible", "NotMigrated"] as const) {
    const list = items.filter((i) => i.status === status);
    if (!list.length) continue;
    const details = document.createElement("details");
    details.dataset.key = `import-list-${status}`;
    details.open = status === "Incompatible";
    details.append(text("summary", `${product(report && status === "Imported" ? "importListImported" : `importList${status}`)} (${list.length})`));
    const ul = document.createElement("ul");
    for (const i of list) ul.append(text("li", itemLine(i), i.credential ? "credential" : ""));
    details.append(ul);
    host.append(details);
  }
}

/** A Java 存档与读档 slot: which one, its name, time and move, and what happens to it. */
function slotLine(s: ImportSlotView): string {
  const parts = [s.key, s.name || (s.auto ? t("LizzieFrame.saveAndLoad.exitRecord") : ""), s.time, product("importMoveNumber", s.move_number), product(`status${s.status}`)];
  if (s.reason) parts.push(`（${product(`reason${s.reason}`)}）`);
  if (s.detail) parts.push(s.detail);
  return parts.filter((p) => p !== "").join(" ");
}

function slotList(host: HTMLElement, slots: ImportSlotView[]): void {
  if (!slots.length) return;
  const details = document.createElement("details");
  details.dataset.key = "import-list-recovery";
  details.open = true;
  details.append(text("summary", `${product("importListRecovery")} (${slots.length})`));
  const ul = document.createElement("ul");
  for (const s of slots) ul.append(text("li", slotLine(s)));
  details.append(ul);
  host.append(details);
}

function problemText(p: EngineProblem): string {
  switch (p.type) {
    case "ProgramNotFound": return product("problemProgramNotFound", p.path);
    case "FileNotFound": return product("problemFileNotFound", p.option, p.path);
    case "EmptyCommand": return product("problemEmptyCommand");
    case "InvalidValue": return product("problemInvalidValue", p.field);
  }
}

/** The engine table; with `choose`, rows with credentials get the "复制凭据" box. */
function engineTable(host: HTMLElement, engines: ImportEngineView[], choose: Map<number, HTMLInputElement> | null): void {
  if (!engines.length) return;
  groupTitle(host, product("importEngines"));
  const wrap = document.createElement("div");
  wrap.className = "engine-table-wrap java-import-engines";
  const table = document.createElement("table");
  table.className = "engine-table";
  const head = document.createElement("tr");
  for (const h of ["#", product("importEngineName"), product("importEngineCommand"), product("importEngineStatus"), product("importEngineProblems")]) head.append(text("th", h));
  table.append(head);
  for (const e of engines) {
    const tr = document.createElement("tr");
    tr.append(text("td", String(e.index + 1) + (e.is_default ? " *" : "")), text("td", e.name), text("td", e.command));
    const status = document.createElement("td");
    status.textContent = product(`status${e.status}`) + (e.reason ? `（${product(`reason${e.reason}`)}）` : "");
    if (choose && e.credential && e.status === "Imported") {
      const box = checkbox(product("importCopyCredentials"), false);
      box.input.dataset.key = `import-credential-${e.index}`;
      choose.set(e.index, box.input);
      status.append(document.createElement("br"), box.label);
    }
    const problems = [...e.problems.map(problemText)];
    if (e.unmigrated_fields.length) problems.push(product("importUnmigratedFields", e.unmigrated_fields.join(", ")));
    if (e.incompatible_fields.length) problems.push(product("importIncompatibleFields", e.incompatible_fields.join(", ")));
    const p = text("td", problems.join("；"), e.problems.length ? "java-import-problem" : "");
    for (const cell of [tr.children[2] as HTMLElement, p]) cell.title = cell.textContent ?? "";
    tr.append(status, p);
    table.append(tr);
  }
  wrap.append(table);
  host.append(wrap);
}

/** Step 2: what would be imported. */
function showPreview(body: HTMLElement, p: ImportPreviewView, firstUse: boolean, close: () => Promise<void>, started: () => void, folder: string): void {
  body.replaceChildren();
  body.append(text("p", product("importSource", p.config_file)));
  body.append(text("p", product("importCounts", count(p.items, "Imported"), count(p.items, "Incompatible"), count(p.items, "NotMigrated")), "java-import-counts"));
  const credentials = new Map<number, HTMLInputElement>();
  engineTable(body, p.engines, credentials);
  // The credentials Java remembered: each comes over only when its switch is on.
  const choices = document.createElement("div");
  choices.className = "java-import-choices";
  body.append(choices);
  // The remembered AI 解说 API key (Java's secure-credentials).
  const commentaryKey = p.commentary_credential
    ? choice(choices, product("importCopyCommentaryKey", p.commentary_credential.base_url || p.commentary_credential.source), "import-commentary-key")
    : null;
  // The 精析设置 remote engine's password.
  const analysisSsh = p.analysis_ssh_credential ? choice(choices, product("importCopyAnalysisSshPassword", p.analysis_ssh_credential), "import-analysis-ssh-password") : null;
  // The Zhizi account's remembered sign-in token and password, each on its own.
  const cloudToken = p.cloud_credentials?.token ? choice(choices, product("importCopyCloudToken", p.cloud_credentials.identifier, t("RemoteCompute.zhizi")), "import-cloud-token") : null;
  const cloudPassword = p.cloud_credentials?.password ? choice(choices, product("importCopyCloudPassword", p.cloud_credentials.identifier, t("RemoteCompute.zhizi")), "import-cloud-password") : null;
  // 跑谱贡献's account, password, custom-command passwords and 远程SSH password.
  const contribution = p.contribution_credential ? choice(choices, product("importCopyContribution", t("Menu.contributeMenu")), "import-contribution-credentials") : null;
  // 快速启动 commands carrying passwords or tokens: the program list comes over only when chosen.
  const quickLinks = p.quick_link_credential ? choice(choices, product("importCopyQuickLinks", t("Menu.quickLinks")), "import-quick-link-credentials") : null;
  if (p.engines.some((e) => e.problems.some((x) => x.type === "ProgramNotFound" || x.type === "FileNotFound")) || p.program_dir) {
    const program = input(p.program_dir ?? "");
    program.className = "java-import-path";
    program.dataset.key = "import-program";
    wideRow(body, product("importProgramFolder"), program, button(product("browse"), async () => {
      const r = await run({ type: "ChooseJavaFolder", program: true });
      if (r?.type === "FolderChosen") program.value = r.path;
    }), button(product("importRepreview"), async () => {
      const r = await run({ type: "PreviewJavaImport", config_dir: folder, program_dir: program.value.trim() || null });
      if (r?.type === "JavaImportPreview") showPreview(body, r.preview, firstUse, close, started, folder);
      else if (r?.type === "JavaImportFailed") void message(product("importTitle"), failureText(r.failure), "warning");
    }));
  }
  let replace: HTMLInputElement | null = null;
  if (p.replaces.length) {
    const box = document.createElement("div");
    box.className = "java-import-replaces";
    // The replaced themes are named: each is a whole folder of this product's own.
    const label = (target: ImportTarget) => target === "Themes" ? `${targetLabel(target)}（${p.replaced_themes.join("、")}）` : targetLabel(target);
    box.append(text("p", product("importReplaces", p.replaces.map(label).join("、"))));
    const confirm = checkbox(product("importReplaceConfirm"), false);
    confirm.input.dataset.key = "import-replace";
    replace = confirm.input;
    box.append(confirm.label);
    body.append(box);
  }
  itemLists(body, p.items, false);
  slotList(body, p.recovery);
  // The Java slots come over only when chosen explicitly.
  const recovery = p.recovery.some((s) => s.status === "Imported") ? choice(body, product("javaImportRecovery"), "import-recovery") : null;
  const apply = primaryButton(product("importDo"), async () => {
    const copy = [...credentials].filter(([, box]) => box.checked).map(([index]) => index);
    const r = await run({
      type: "ApplyJavaImport",
      preview: p.id,
      copy_credentials: copy,
      copy_commentary_key: commentaryKey?.checked ?? false,
      copy_analysis_ssh_password: analysisSsh?.checked ?? false,
      copy_cloud_token: cloudToken?.checked ?? false,
      copy_cloud_password: cloudPassword?.checked ?? false,
      copy_contribution_credentials: contribution?.checked ?? false,
      copy_quick_link_credentials: quickLinks?.checked ?? false,
      replace: replace?.checked ?? false,
      recovery: recovery?.checked ? { auto_slot_name: t("LizzieFrame.saveAndLoad.exitRecord") } : null,
    });
    if (r?.type === "JavaImported") {
      started();
      showReport(body, r.report, close);
    }
  });
  apply.dataset.key = "import-apply";
  const updateApply = () => { apply.disabled = !!replace && !replace.checked; };
  replace?.addEventListener("change", updateApply);
  updateApply();
  // 返回 goes back a step without ending the dialog: it stands at the footer's start.
  const back = button(product("importBack"), () => showSource(body, firstUse, close, started, folder, p.program_dir));
  footer(body, { start: [back], primary: apply, cancel: button(firstUse ? product("importSkip") : product("cancel"), close) });
}

/** Step 3: what was imported. */
function showReport(body: HTMLElement, r: ImportReportView, close: () => Promise<void>): void {
  body.replaceChildren();
  body.append(text("p", product("importDone"), "java-import-section"));
  body.append(text("p", product("importSource", r.config_file)));
  body.append(text("p", product("importReportCounts", count(r.items, "Imported"), count(r.items, "Incompatible"), count(r.items, "NotMigrated")), "java-import-counts"));
  engineTable(body, r.engines, null);
  if (r.engines.some((e) => e.problems.length)) body.append(text("p", product("importFixLater", t("Menu.engineConfig"))));
  if (r.restart_required) body.append(text("p", t("Lizzie.hint.restart"), "java-import-restart"));
  itemLists(body, r.items, true);
  slotList(body, r.recovery);
  // The import is done: only 关闭 is left.
  const done = button(product("close"), close);
  done.dataset.key = "import-close";
  footer(body, { cancel: done });
  done.focus();
}
