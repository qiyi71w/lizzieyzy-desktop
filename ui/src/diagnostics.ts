// Help > 诊断与日志 (DiagnosticsDialog): 诊断记录 and its modules, 完整日志 and its scopes (applied
// with 应用), application/crash logs, and the ReadBoard helper's own log writers. The dialog is
// grouped rows (engine/controls.ts) in one column.
import { registerViewHandler, run, state } from "./app";
import { logsStatus } from "./contract/client";
import type { DiagnosticModule, HelperLoggingView, LoggingSettings, Persistence, Presentation, TraceScope } from "./contract/generated";
import { confirm, panel } from "./dialogs";
import { actions, button, groupNote, groupTitle, row, switchRow } from "./engine/controls";
import { format, product, t } from "./i18n";
import "./diagnostics.css";

// Labels resolve in the active language when the dialog opens.
const moduleRows = (): [DiagnosticModule, string][] => [
  ["engine", t("DiagnosticsDialog.module.engine")],
  ["gtp-summary", t("DiagnosticsDialog.module.gtpSummary")],
  ["readboard-yike", t("DiagnosticsDialog.module.readboardYike")],
  ["network-remote", t("DiagnosticsDialog.module.networkRemote")],
];
const scopeRows = (): [TraceScope, string][] => [
  ["engine-gtp", t("DiagnosticsDialog.scope.engineGtp")],
  ["readboard-yike", t("DiagnosticsDialog.scope.readboardYike")],
  ["network-websocket", t("DiagnosticsDialog.scope.networkWebsocket")],
];

/** DiagnosticsDialog.scopeLabels (DiagnosticsDialog.java:1132-1150). */
function scopeLabels(chosen: TraceScope[]): string {
  const labels = scopeRows().filter(([s]) => chosen.includes(s)).map(([, label]) => label);
  return labels.length ? labels.join(", ") : t("DiagnosticsDialog.noScopes");
}

function bytes(n: number | null): string {
  return n === null ? product("none") : `${(n / 1024).toFixed(1)} KB`;
}

/** DiagnosticsDialog.presentationText (DiagnosticsDialog.java:771-792). */
function presentationText(p: Presentation): string {
  switch (p) {
    case "On":
      return t("DiagnosticsDialog.presentation.on");
    case "OnStorageDegraded":
      return t("DiagnosticsDialog.presentation.onDegraded");
    case "OnStorageUnavailable":
      return t("DiagnosticsDialog.presentation.onUnavailable");
    case "NotApplied":
      return t("DiagnosticsDialog.presentation.notApplied");
    case "LegacyUnconfirmed":
      return t("DiagnosticsDialog.presentation.legacy");
    case "Off":
      return t("DiagnosticsDialog.presentation.off");
    case "Unknown":
      return t("DiagnosticsDialog.presentation.unknown");
  }
}

/** DiagnosticsDialog.persistenceText (DiagnosticsDialog.java:1326-1340). */
function persistenceText(p: Persistence | null): string {
  switch (p) {
    case null:
      return t("DiagnosticsDialog.capability.none");
    case "Healthy":
      return t("DiagnosticsDialog.persistence.healthy");
    case "Degraded":
      return t("DiagnosticsDialog.persistence.degraded");
    case "Unavailable":
      return t("DiagnosticsDialog.persistence.unavailable");
  }
}

/** DiagnosticsDialog.formatCaptureSummary (DiagnosticsDialog.java:1311-1324). The observed capture
 *  toggle is read from the core's `processSession=… desired=… observed=<on|off|unknown> …`. */
function captureSummary(l: HelperLoggingView | null): string {
  if (!l?.process_session_id) return t("DiagnosticsDialog.captureSummary.none");
  const observed = /\bobserved=(\w+)/.exec(l.capture_summary)?.[1];
  const toggle = observed === "on" ? t("DiagnosticsDialog.presentation.on") : observed === "off" ? t("DiagnosticsDialog.presentation.off") : t("DiagnosticsDialog.presentation.unknown");
  const desired = l.desired_capture ? t("DiagnosticsDialog.presentation.on") : t("DiagnosticsDialog.presentation.off");
  return format(t("DiagnosticsDialog.captureSummaryDetail"), l.process_session_id, desired, toggle, persistenceText(l.persistence));
}

/** Refreshes the open dialog's ReadBoard section from the newest view. */
let refreshReadBoard: (() => void) | null = null;
registerViewHandler(() => refreshReadBoard?.());

export async function openDiagnostics(): Promise<void> {
  const status = await logsStatus();
  // One column of groups: LizzieYzy, ReadBoard, then the logs' health with the actions.
  const body = document.createElement("div");
  body.className = "diagnostics";
  /** The next block of rows in `host`: a gap parts it from the block before. */
  const block = (host: HTMLElement) => {
    const el = document.createElement("div");
    el.className = "diag-block";
    host.append(el);
    return el;
  };

  // LizzieYzy pane (DiagnosticsDialog.java:288-352): each switch with its categories indented
  // under it, then the scope notes.
  groupTitle(body, t("DiagnosticsDialog.hostPane"));
  /** checkRow (DiagnosticsDialog.java): the switch and its label; a state shown for the switch
   *  stands before it at the row's end. */
  const check = (host: HTMLElement, label: string, key: string, sub: boolean, ...state: HTMLElement[]) => {
    const { row: r, input } = switchRow(host, label, "", false, ...state);
    input.dataset.key = key;
    if (sub) r.classList.add("diag-sub");
    return input;
  };
  const diagnostics = block(body);
  const enabled = check(diagnostics, t("DiagnosticsDialog.diagnosticsEnabled"), "diagnostics.enabled", false);
  const modules = moduleRows().map(([m, label]) => [m, check(diagnostics, label, `diagnostics.module.${m}`, true)] as const);
  const trace = block(body);
  const traceState = document.createElement("span");
  const fullLogs = check(trace, t("DiagnosticsDialog.fullTrace"), "diagnostics.fullTrace", false, traceState);
  const scopes = scopeRows().map(([s, label]) => [s, check(trace, label, `diagnostics.scope.${s}`, true)] as const);
  const scopeNote = groupNote(body, "");
  groupNote(body, t("DiagnosticsDialog.gtpMigrationNote"));
  // A 完整日志 start or stop shows once the view after it arrives.
  let refreshPending = false;

  const active = () => state.view?.full_trace ?? [];
  /** setModulesEnabled (DiagnosticsDialog.java:1097-1102). */
  const modulesEnabled = () => { for (const [, box] of modules) box.disabled = !enabled.checked; };
  /** refreshScopeControls (DiagnosticsDialog.java:1007-1027): a running session's scopes are
   *  shown and locked while 完整日志 stays checked. */
  const refreshScopes = () => {
    const running = active();
    const locked = running.length > 0 && fullLogs.checked;
    if (locked) for (const [s, box] of scopes) box.checked = running.includes(s);
    for (const [, box] of scopes) box.disabled = locked;
    scopeNote.textContent = running.length
      ? `${t("DiagnosticsDialog.activeScopes")}: ${scopeLabels(running)}\n${t("DiagnosticsDialog.stopToChangeScopes")}`
      : t("DiagnosticsDialog.scopesNextSession");
    traceState.textContent = running.length ? t("DiagnosticsDialog.stopFullTrace") : t("DiagnosticsDialog.traceOff");
  };
  /** refreshFromRuntime (DiagnosticsDialog.java:646-659). */
  const refreshHost = () => {
    const s = state.view?.settings.logging;
    if (!s) return;
    enabled.checked = s.diagnosticsEnabled;
    for (const [m, box] of modules) box.checked = s.diagnosticModules.includes(m);
    const running = active();
    fullLogs.checked = running.length > 0;
    const shown = running.length ? running : s.preferredFullTraceScopes;
    for (const [sc, box] of scopes) box.checked = shown.includes(sc);
    refreshScopes();
    modulesEnabled();
  };
  enabled.onchange = modulesEnabled;
  fullLogs.onchange = refreshScopes;
  /** selectedScopes (DiagnosticsDialog.java:980-995): none checked means all. */
  const selectedScopes = (): TraceScope[] => {
    const chosen = scopes.filter(([, box]) => box.checked).map(([s]) => s);
    return chosen.length ? chosen : scopes.map(([s]) => s);
  };
  /** applyCurrentPlan (DiagnosticsDialog.java:418-475). */
  const apply = async () => {
    const saved = state.view?.settings.logging;
    if (!saved) return;
    const continues = active().length > 0 && fullLogs.checked;
    const next: LoggingSettings = {
      diagnosticsEnabled: enabled.checked,
      diagnosticModules: modules.filter(([, box]) => box.checked).map(([m]) => m),
      preferredFullTraceScopes: continues ? saved.preferredFullTraceScopes : selectedScopes(),
    };
    if ((await run({ type: "SetLogging", settings: next })) === null) {
      refreshHost();
      feedback.textContent = t("DiagnosticsDialog.applyFailed");
      return;
    }
    feedback.textContent = t("DiagnosticsDialog.applied");
    if (fullLogs.checked) {
      if (!active().length) {
        const message = `${t("DiagnosticsDialog.confirmMessage")}\n${scopeLabels(selectedScopes())}`;
        if (await confirm(t("DiagnosticsDialog.confirmTitle"), message)) {
          if ((await run({ type: "SetFullTrace", enabled: true, scopes: selectedScopes() })) === null) {
            feedback.textContent = t("DiagnosticsDialog.applyFailed");
          }
        }
        refreshPending = true;
        refreshHost();
      } else {
        feedback.textContent = t("DiagnosticsDialog.activeScopesUnchanged");
      }
    } else if (active().length) {
      await run({ type: "SetFullTrace", enabled: false });
      refreshPending = true;
      refreshHost();
    }
  };
  refreshHost();

  // ReadBoard (DiagnosticsDialog.java:354-388): the helper's capability, persistence, dropped
  // events and process session, then the desired writers beside what the helper confirmed, then
  // the capture session. Without a helper the values are Java's detached snapshot; a toggle there
  // is refused and the boxes go back (requestSet false, refreshHelperFromSnapshot).
  groupTitle(body, t("DiagnosticsDialog.readBoardPane"));
  const values = block(body);
  const value = (label: string) => {
    const span = document.createElement("span");
    span.className = "diag-value";
    row(values, label, "", span);
    return span;
  };
  const capability = value(t("DiagnosticsDialog.helperCapability"));
  const persistence = value(t("DiagnosticsDialog.helperPersistence"));
  const drops = value(t("DiagnosticsDialog.helperDropCount"));
  const session = value(t("DiagnosticsDialog.helperProcessSession"));
  // Each writer's switch is the desired state; what the helper confirmed stands before it.
  const writers = block(body);
  const helperRow = (label: string) => {
    const observed = document.createElement("span");
    return { observed, box: switchRow(writers, label, "", false, observed).input };
  };
  const helperDiagnostics = helperRow(t("DiagnosticsDialog.helperDiagnostics"));
  const helperTrace = helperRow(t("DiagnosticsDialog.helperFullTrace"));
  const helperCapture = helperRow(t("DiagnosticsDialog.helperCapture"));
  const summary = groupNote(body, "");
  const updateReadBoard = () => {
    const l = state.view?.readboard.logging ?? null;
    capability.textContent = l?.status === "LegacyUnconfirmed" || l?.diagnostics === "LegacyUnconfirmed"
      ? t("DiagnosticsDialog.presentation.legacy")
      : l?.drop_count != null ? t("DiagnosticsDialog.capability.ready") : t("DiagnosticsDialog.capability.none");
    persistence.textContent = persistenceText(l?.persistence ?? null);
    drops.textContent = l?.drop_count == null ? "—" : String(l.drop_count);
    session.textContent = l?.process_session_id || t("DiagnosticsDialog.capability.none");
    helperDiagnostics.box.checked = l?.desired_diagnostics ?? false;
    helperTrace.box.checked = l?.desired_trace ?? false;
    helperCapture.box.checked = l?.desired_capture ?? false;
    helperDiagnostics.observed.textContent = presentationText(l?.diagnostics ?? "Unknown");
    helperTrace.observed.textContent = presentationText(l?.trace ?? "Unknown");
    helperCapture.observed.textContent = presentationText(l?.capture ?? "Unknown");
    summary.textContent = `${t("DiagnosticsDialog.captureSummary")}: ${captureSummary(l)}`;
  };
  /** requestHelperToggle (DiagnosticsDialog.java:705-721): all three wishes go together; turning
   *  capture on asks first (OK_CANCEL, warning). */
  const requestHelper = async (capture: boolean) => {
    if (capture && helperCapture.box.checked && !state.view?.readboard.logging?.desired_capture) {
      if (!(await confirm(t("DiagnosticsDialog.captureConfirmTitle"), t("DiagnosticsDialog.captureConfirmMessage")))) {
        helperCapture.box.checked = false;
        return;
      }
    }
    await run({ type: "ReadBoard", op: { type: "SetHelperLogging", diagnostics: helperDiagnostics.box.checked, capture: helperCapture.box.checked, trace: helperTrace.box.checked } });
    updateReadBoard();
  };
  helperDiagnostics.box.onchange = () => void requestHelper(false);
  helperTrace.box.onchange = () => void requestHelper(false);
  helperCapture.box.onchange = () => void requestHelper(true);
  updateReadBoard();

  // 日志健康状态: the log sizes, the folder on a line of its own under its name, the actions and
  // the status area under them.
  groupTitle(body, t("DiagnosticsDialog.health"));
  const health = block(body);
  row(health, product("diagnosticsLogs", bytes(status.app_log_bytes), bytes(status.crash_log_bytes)), "");
  const path = document.createElement("code");
  path.className = "diag-path";
  path.textContent = status.directory;
  row(health, t("DiagnosticsDialog.logsFolder"), "").firstElementChild!.append(path);
  actions(
    body,
    button(t("DiagnosticsDialog.openLogs"), () => run({ type: "OpenLogFolder" })),
    button(t("DiagnosticsDialog.apply"), () => void apply()),
    button(t("DiagnosticsDialog.exportDefault"), async () => {
      feedback.textContent = t("DiagnosticsDialog.exporting");
      const r = await run({ type: "ExportDiagnostics" });
      feedback.textContent = r?.type === "Exported" ? `${t("DiagnosticsDialog.exportSuccess")}${r.path}` : r ? "" : t("DiagnosticsDialog.exportFailure");
    }),
  );
  // The status area under the buttons (statusArea): 应用 and export results.
  const feedback = groupNote(body, "");
  const refresh = () => {
    updateReadBoard();
    if (refreshPending) {
      refreshPending = false;
      refreshHost();
    } else {
      refreshScopes();
    }
  };
  refreshReadBoard = refresh;
  try {
    await panel(t("DiagnosticsDialog.title"), body);
  } finally {
    if (refreshReadBoard === refresh) refreshReadBoard = null;
  }
}
