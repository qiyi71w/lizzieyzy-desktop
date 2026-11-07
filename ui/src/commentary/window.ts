// AI 解说 window (TeacherDialog / TeacherDialogView): mode toggles and 设置, the context bar with the
// scope or the 从/到 range and 开始解说 / 停止, the 棋理解说 / 分析依据 tabs, the status strip and the
// follow-up composer. The core owns the dialog state and every enablement flag; this window keeps
// only interaction state (mode, range fields, tab, the typed question) and the settings drafts.
import { emitTo } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { Command, CommentaryView, Mode } from "../contract/generated";
import { format, isChinese, setLanguage, t } from "../i18n";
import { icon } from "./icons";
import { markdownToHtml } from "./markdown";
import {
  COMMENTARY_DATA_EVENT,
  COMMENTARY_FOCUS_EVENT,
  COMMENTARY_INPUT_EVENT,
  type CommentaryInput,
  type CommentaryPayload,
  type CommentaryReport,
} from "./model";
import { send } from "./send";
import { openSettings, type SettingsDialog } from "./settings";
import { failureText, statusText, teacherText } from "./text";
import "./commentary.css";

/** TeacherDialogView.StatusTone. */
type Tone = "neutral" | "running" | "success" | "warning" | "error";

function el<K extends keyof HTMLElementTagNameMap>(tag: K, className = "", text = ""): HTMLElementTagNameMap[K] {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text) e.textContent = text;
  return e;
}

function button(text: string, className: string, key: string): HTMLButtonElement {
  const b = el("button", `tc-btn ${className}`.trim(), text);
  b.type = "button";
  b.dataset.key = key;
  return b;
}

export function bootCommentary(): void {
  const self = getCurrentWebviewWindow();
  const emit = (input: CommentaryInput) => void emitTo("main", COMMENTARY_INPUT_EVENT, input);
  type Op = Extract<Command, { type: "Commentary" }>["op"];
  const commentary = (op: Op) => send({ type: "Commentary", op });

  let v: CommentaryView | null = null;
  let mode: Mode = "Next";
  let tab: "lesson" | "evidence" = "lesson";
  /** A message this window shows in place of the core status until the core status changes
   *  (an invalid range, a refused request, a browser failure). */
  let local: { text: string; tone: Tone; under: string } | null = null;
  let rangeBound = -1;
  let shownOutput = "";
  let settings: SettingsDialog | null = null;

  const root = el("div");
  root.id = "tc-root";

  // ------------------------------------------------------------- header: modes and 设置
  const header = el("div", "tc-header");
  const modes = el("div", "tc-modes");
  modes.setAttribute("role", "radiogroup");
  const modeButtons = (
    [
      ["Next", "next", "Teacher.mode.next", "Teacher.action.next", "Teacher.action.next.description"],
      ["Range", "range", "Teacher.mode.range", "Teacher.action.range", "Teacher.action.range.description"],
      ["Whole", "whole", "Teacher.mode.whole", "Teacher.action.whole", "Teacher.action.whole.description"],
    ] as const
  ).map(([value, glyph, label, name, description]) => {
    const b = el("button", "tc-mode");
    b.type = "button";
    b.dataset.key = `commentary.mode.${glyph}`;
    b.setAttribute("role", "radio");
    b.title = t(name);
    b.setAttribute("aria-label", t(name));
    b.setAttribute("aria-description", t(description));
    b.append(icon(glyph), el("span", "", t(label)));
    b.onclick = () => {
      // Selecting a mode never starts a request.
      mode = value;
      render();
    };
    modes.append(b);
    return { value, b };
  });
  const settingsButton = button(t("Teacher.action.settings"), "", "commentary.settings");
  settingsButton.prepend(icon("settings"));
  settingsButton.querySelector("svg")?.classList.add("icon");
  settingsButton.title = t("Teacher.action.settings.description");
  settingsButton.onclick = () => void showSettings();
  header.append(modes, settingsButton);

  // ------------------------------------------------------------- reader
  const reader = el("div", "tc-reader");
  const context = el("div", "tc-context");
  const scope = el("div", "tc-scope");
  scope.dataset.key = "commentary.scope";
  const range = el("div", "tc-range");
  const rangeField = (id: string, key: string, label: string) => {
    const input = el("input");
    input.type = "number";
    input.id = id;
    input.step = "1";
    input.min = "1";
    input.dataset.key = key;
    input.setAttribute("aria-label", label);
    return input;
  };
  const fromField = rangeField("tc-from", "commentary.range.from", t("Teacher.range.from"));
  const toField = rangeField("tc-to", "commentary.range.to", t("Teacher.range.to"));
  const fromLabel = el("label", "", t("Teacher.range.from"));
  const toLabel = el("label", "", t("Teacher.range.to"));
  fromLabel.htmlFor = fromField.id;
  toLabel.htmlFor = toField.id;
  range.append(fromLabel, fromField, toLabel, toField);
  const actions = el("div", "tc-actions");
  const start = button(t("Teacher.action.start"), "primary", "commentary.start");
  const stop = button(t("Teacher.action.stop"), "danger", "commentary.stop");
  stop.title = t("Teacher.action.stop.description");
  actions.append(start, stop);
  context.append(range, scope, actions);

  const tabs = el("div", "tc-tabs");
  tabs.setAttribute("role", "tablist");
  const tabButton = (value: "lesson" | "evidence", key: string) => {
    const b = el("button", "tc-tab", t(key));
    b.type = "button";
    b.setAttribute("role", "tab");
    b.dataset.key = `commentary.tab.${value}`;
    b.onclick = () => {
      tab = value;
      render();
    };
    tabs.append(b);
    return b;
  };
  const lessonTab = tabButton("lesson", "Teacher.learning.lesson");
  const evidenceTab = tabButton("evidence", "Teacher.learning.evidence");

  const pages = el("div", "tc-pages");
  const lessonPage = el("div", "tc-page");
  const empty = el("div", "tc-empty");
  const emptyTitle = el("div", "tc-empty-title");
  const emptyDetail = el("div", "tc-empty-detail");
  emptyDetail.dataset.key = "commentary.emptyDetail";
  empty.append(icon("commentary"), emptyTitle, emptyDetail);
  const output = el("div", "tc-output");
  output.setAttribute("aria-label", t("Teacher.output"));
  lessonPage.append(empty, output);
  const evidencePage = el("div", "tc-page");
  const evidence = el("pre", "tc-evidence");
  evidence.setAttribute("aria-label", t("Teacher.learning.evidence"));
  evidencePage.append(evidence);
  pages.append(lessonPage, evidencePage);

  const statusArea = el("div", "tc-status-area");
  const progress = el("div", "tc-progress");
  progress.setAttribute("role", "progressbar");
  progress.setAttribute("aria-label", t("Teacher.progress.accessible"));
  const statusRow = el("div", "tc-status-row");
  const statusLeft = el("div", "tc-status-left");
  const dot = el("span", "tc-dot");
  const status = el("div", "tc-status");
  status.dataset.key = "commentary.status";
  status.setAttribute("role", "status");
  status.setAttribute("aria-label", t("Teacher.status.accessible"));
  statusLeft.append(dot, status);
  const account = el("div", "tc-account");
  const model = el("div", "tc-model");
  model.dataset.key = "commentary.model";
  model.setAttribute("aria-label", t("Teacher.model.accessible"));
  const usage = button(t("Teacher.chatgpt.usage"), "", "commentary.usage");
  account.append(model, usage);
  statusRow.append(statusLeft, account);
  statusArea.append(progress, statusRow);
  reader.append(context, tabs, pages, statusArea);

  // ------------------------------------------------------------- composer
  const composer = el("div", "tc-composer");
  const prompt = el("div", "tc-prompt");
  const followLabel = el("label", "", t("Teacher.followUp"));
  const followUp = el("input", "tc-input");
  followUp.id = "tc-follow";
  followUp.type = "text";
  followUp.placeholder = t("Teacher.followUp.placeholder");
  followUp.dataset.key = "commentary.followUp";
  followUp.setAttribute("aria-description", t("Teacher.followUp.placeholder"));
  followLabel.htmlFor = followUp.id;
  const ask = button(t("Teacher.action.ask"), "primary", "commentary.ask");
  ask.title = t("Teacher.action.ask.description");
  prompt.append(followLabel, followUp, ask);
  const footer = el("div", "tc-footer");
  const writeLabel = el("label", "tc-check");
  const writeBox = el("input");
  writeBox.type = "checkbox";
  writeBox.dataset.key = "commentary.writeToSgf";
  writeLabel.append(writeBox, document.createTextNode(t("Teacher.writeToSgf")));
  footer.append(writeLabel);
  for (const [suffix, key] of [["simpler", "Teacher.learning.simpler"], ["reply", "Teacher.learning.reply"]] as const) {
    const shortcut = button(t(key), "", `commentary.${suffix}`);
    // The shortcuts only fill the question and focus it; they never send it.
    shortcut.onclick = () => {
      followUp.value = shortcut.textContent ?? "";
      followUp.focus();
      report();
    };
    footer.append(shortcut);
  }
  composer.append(prompt, footer);
  root.append(header, reader, composer);
  document.body.append(root);

  // ------------------------------------------------------------- actions
  const showLocal = (text: string, tone: Tone) => {
    local = { text, tone, under: JSON.stringify(v?.status ?? null) };
    render();
  };

  /** JSpinner commit: a whole number within 1..max(1, last move), else null. */
  const rangeValue = (input: HTMLInputElement): number | null => {
    const max = Math.max(1, v?.main_line_last_move ?? 1);
    const n = Number(input.value.trim());
    return input.value.trim() !== "" && Number.isInteger(n) && n >= 1 && n <= max ? n : null;
  };

  start.onclick = async () => {
    if (!v || !v.start_enabled) return;
    // 连接 AI: an unusable or unready connection opens the settings instead.
    if (!v.settings_usable || !v.connection_ready) {
      await showSettings();
      return;
    }
    let first = 1;
    let last = Math.max(1, v.main_line_last_move);
    if (mode === "Range") {
      const a = rangeValue(fromField);
      const b = rangeValue(toField);
      if (a === null || b === null) {
        showLocal(t("Teacher.status.invalidRange"), "warning");
        return;
      }
      // Reversed endpoints are swapped by the core.
      first = a;
      last = b;
    }
    const r = await commentary({ type: "Start", mode, first, last });
    if (!r.ok) {
      if (r.message === "Teacher.empty.connect") await showSettings();
      else showLocal(failureText(r.message), "warning");
    }
  };
  stop.onclick = () => {
    if (v?.stop_enabled) void commentary({ type: "Stop" });
  };
  const askFollowUp = async () => {
    if (!v || !v.controls_enabled) return;
    const question = followUp.value.trim();
    if (!question) return;
    const r = await commentary({ type: "Ask", question });
    // TeacherDialog.askFollowUp clears the field only when the request actually started.
    if (r.ok) {
      if (r.result.type === "Done" && followUp.value.trim() === question) followUp.value = "";
      report();
    } else if (r.message === "Teacher.empty.connect") await showSettings();
    else showLocal(failureText(r.message), "warning");
  };
  ask.onclick = () => void askFollowUp();
  followUp.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      void askFollowUp();
    }
  });
  followUp.addEventListener("input", () => report());
  // The choice stays changeable while a request runs; the value at completion counts.
  writeBox.onchange = () => void commentary({ type: "SetWriteToSgf", enabled: writeBox.checked });
  usage.onclick = async () => {
    const r = await send({ type: "CommentaryConnection", op: { type: "OpenUsage" } });
    if (!r.ok) showLocal(r.message.startsWith("Teacher.") ? failureText(r.message) : t("Teacher.chatgpt.error.browser"), "warning");
  };

  async function showSettings(): Promise<void> {
    if (!v || !v.settings_enabled || settings) return;
    // 设置 first stops a running request (TeacherDialog settingsButton).
    if (v.running) await commentary({ type: "Stop" });
    settings = openSettings({
      view: () => v,
      onClose: () => {
        settings = null;
        render();
        focusPrimary();
      },
      changed: () => report(),
    });
    render();
  }

  /** focusPrimaryControl: 开始解说 when it can be used. */
  const focusPrimary = () => {
    if (!settings && !start.disabled) start.focus();
  };

  // ------------------------------------------------------------- render
  function render(): void {
    if (!v) return;
    const c = v;
    document.documentElement.lang = isChinese() ? "zh" : "en";
    for (const m of modeButtons) {
      const on = m.value === mode;
      m.b.classList.toggle("selected", on);
      m.b.setAttribute("aria-checked", String(on));
      m.b.disabled = !c.controls_enabled;
    }
    settingsButton.disabled = !c.settings_enabled;

    // Context bar: the range fields in 区间, else the scope of the shown commentary.
    const isRange = mode === "Range";
    range.hidden = !isRange;
    scope.hidden = isRange;
    const scopeText = c.scope.key ? teacherText(c.scope) : c.current_move < 0 ? t("Teacher.position.none") : format(t("Teacher.position.move"), c.current_move);
    scope.textContent = scopeText;
    scope.title = scopeText;
    const max = Math.max(1, c.main_line_last_move);
    if (rangeBound !== max) {
      // The spinners are rebuilt with the board: 1 to the main line's last move.
      rangeBound = max;
      fromField.value = "1";
      toField.value = String(max);
    }
    fromField.max = toField.max = String(max);
    fromField.disabled = toField.disabled = !c.controls_enabled;
    start.disabled = !c.start_enabled;
    start.textContent = !c.running && (!c.settings_usable || !c.connection_ready) ? t("Teacher.action.connect") : t("Teacher.action.start");
    start.setAttribute("aria-description", start.textContent);
    stop.disabled = !c.stop_enabled;

    lessonTab.classList.toggle("selected", tab === "lesson");
    evidenceTab.classList.toggle("selected", tab === "evidence");
    lessonTab.setAttribute("aria-selected", String(tab === "lesson"));
    evidenceTab.setAttribute("aria-selected", String(tab === "evidence"));
    lessonPage.hidden = tab !== "lesson";
    evidencePage.hidden = tab !== "evidence";

    // The core status, or this window's own message until the core status changes.
    if (local && local.under !== JSON.stringify(c.status)) local = null;
    const changedTarget = c.target_changed && c.status.key === "Teacher.status.completed";
    const shownStatus = local?.text ?? statusText(c.status, c.target_changed);
    const tone: Tone = local?.tone ?? (changedTarget ? "warning" : (c.status.tone.toLowerCase() as Tone));
    status.textContent = shownStatus || " ";
    status.title = shownStatus;
    dot.className = `tc-dot ${tone}`;
    progress.hidden = !c.running;
    const modelText = teacherText(c.model_status);
    model.textContent = modelText || " ";
    model.title = modelText;
    usage.hidden = !c.chatgpt_usage_visible;

    // 棋理解说: the empty / loading card, or the rendered Markdown.
    const out = c.output_state === "Output";
    empty.hidden = out;
    output.hidden = !out;
    if (out) {
      if (shownOutput !== c.output) {
        shownOutput = c.output;
        output.innerHTML = markdownToHtml(c.output);
        lessonPage.scrollTop = 0;
      }
    } else {
      shownOutput = "";
      output.replaceChildren();
      emptyTitle.textContent = c.output_state === "Loading" ? t("Teacher.loading.title") : t("Teacher.empty.title");
      // setStatus: a neutral status shows the guide, others their own text; a connection that
      // is not ready asks to connect first (refreshSettingsStatus).
      emptyDetail.textContent =
        c.output_state === "Loading"
          ? shownStatus
          : c.settings_loaded && !c.connection_ready
            ? t("Teacher.empty.connect")
            : tone === "neutral"
              ? t("Teacher.empty.guide")
              : shownStatus;
    }
    if (evidence.textContent !== c.evidence) {
      evidence.textContent = c.evidence;
      evidencePage.scrollTop = 0;
    }

    followUp.disabled = ask.disabled = !c.controls_enabled;
    writeBox.checked = c.write_to_sgf;
    settings?.refresh();
    report();
  }

  // ------------------------------------------------------------- acceptance report
  let lastReport = "";
  function report(): void {
    if (!v) return;
    const targets = [...root.querySelectorAll<HTMLElement>("[data-key]"), ...(settings?.targets() ?? [])]
      .filter((e) => e.getClientRects().length > 0)
      .map((e) => {
        const r = e.getBoundingClientRect();
        const disabled = (e instanceof HTMLButtonElement || e instanceof HTMLInputElement || e instanceof HTMLSelectElement) && e.disabled;
        // A secret field stays secret when 显示密钥 shows it as text.
        const secret = (e instanceof HTMLInputElement && e.type === "password") || "secret" in e.dataset;
        const text = e instanceof HTMLInputElement ? (secret ? "" : e.type === "checkbox" ? String(e.checked) : e.value) : e instanceof HTMLSelectElement ? e.value : (e.textContent ?? "");
        return { key: e.dataset.key ?? "", text, x: Math.round(r.left + r.width / 2), y: Math.round(r.top + r.height / 2), enabled: !disabled };
      });
    const r: CommentaryReport = {
      mode,
      scope: mode === "Range" ? "" : (scope.textContent ?? ""),
      status: status.textContent ?? "",
      tone: dot.className.replace("tc-dot ", ""),
      modelStatus: model.textContent ?? "",
      running: v.running,
      startLabel: start.textContent ?? "",
      startEnabled: !start.disabled,
      stopEnabled: !stop.disabled,
      settingsEnabled: !settingsButton.disabled,
      controlsEnabled: v.controls_enabled,
      range: mode === "Range" ? { from: Number(fromField.value), to: Number(toField.value), min: 1, max: Number(toField.max) } : null,
      tab,
      outputState: v.output_state,
      emptyTitle: empty.hidden ? "" : (emptyTitle.textContent ?? ""),
      emptyDetail: empty.hidden ? "" : (emptyDetail.textContent ?? ""),
      output: output.hidden ? "" : output.innerText,
      evidence: evidence.textContent ?? "",
      followUp: followUp.value,
      writeToSgf: writeBox.checked,
      usageVisible: !usage.hidden,
      settings: settings?.report() ?? null,
      targets,
    };
    const text = JSON.stringify(r);
    if (text === lastReport) return;
    lastReport = text;
    emit({ type: "report", report: r });
  }

  // ------------------------------------------------------------- keys and events
  document.addEventListener("keydown", (e) => {
    // Escape disposes the dialog; the settings dialog takes its own Escape first.
    if (e.key === "Escape" && !settings && !e.defaultPrevented) {
      e.preventDefault();
      void self.close();
    }
  });
  document.addEventListener("contextmenu", (e) => e.preventDefault());
  window.addEventListener("resize", () => report());
  for (const field of [fromField, toField]) field.addEventListener("change", () => report());

  let documentId = -1;
  void self
    .listen<CommentaryPayload>(COMMENTARY_DATA_EVENT, (e) => {
      setLanguage(e.payload.language);
      const first = !v;
      // TeacherDialog's game guard: another record drops the unsent question (followUp.setText("")).
      if (!first && e.payload.document !== documentId) followUp.value = "";
      documentId = e.payload.document;
      v = e.payload.view;
      if (first) mode = v.mode;
      // A failed or stopped follow-up gives its question back when the field is empty.
      if (v.restore_question && !followUp.value.trim()) followUp.value = v.restore_question;
      render();
      if (first) focusPrimary();
    })
    .then(() => {
      emit({ type: "ready" });
      // TeacherDialog's constructor: refresh from the board and load the settings.
      void commentary({ type: "Open" });
    });
  void self.listen(COMMENTARY_FOCUS_EVENT, () => {
    void self.setFocus();
    focusPrimary();
  });
}
