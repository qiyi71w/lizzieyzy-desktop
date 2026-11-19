// Help > 诊断与日志 (DiagnosticsDialog): 诊断记录 and its modules, 完整日志 and its scopes (applied
// with 应用), application/crash logs, and the ReadBoard helper's own log writers.
import { registerViewHandler, run, state } from "./app";
import { logsStatus } from "./contract/client";
import type { DiagnosticModule, HelperLoggingView, LoggingSettings, Persistence, Presentation, TraceScope } from "./contract/generated";
import { confirm, panel } from "./dialogs";
import { format, product, t } from "./i18n";

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
  // Two columns: the LizzieYzy switches on the left, ReadBoard and the logs on the right.
  const body = document.createElement("div");
  body.className = "diagnostics";
  const hostColumn = document.createElement("div");
  const sideColumn = document.createElement("div");
  hostColumn.className = sideColumn.className = "diag-column";
  body.append(hostColumn, sideColumn);
  let group: HTMLElement = hostColumn;
  const section = (title: string, column: HTMLElement) => {
    group = document.createElement("section");
    group.className = "diag-group";
    const h = document.createElement("h3");
    h.textContent = title;
    group.append(h);
    column.append(group);
  };
  const row = (...children: (HTMLElement | string)[]) => {
    const r = document.createElement("div");
    r.className = "diag-row";
    r.append(...children);
    group.append(r);
    return r;
  };
  const button = (label: string, onClick: () => void, enabled = true) => {
    const b = document.createElement("button");
    b.textContent = label;
    b.disabled = !enabled;
    if (!enabled) b.title = product("unavailable");
    b.onclick = onClick;
    return b;
  };

  // LizzieYzy pane (DiagnosticsDialog.java:288-352).
  section(t("DiagnosticsDialog.hostPane"), hostColumn);
  /** checkRow (DiagnosticsDialog.java): the box and its label; a state shown for the switch
   *  follows at the row's end. */
  const check = (label: string, key: string, indent: boolean) => {
    const box = document.createElement("input");
    box.type = "checkbox";
    box.dataset.key = key;
    const l = document.createElement("label");
    l.className = "diag-check";
    l.append(box, label);
    const r = row(l);
    if (indent) r.classList.add("diag-indent");
    return { box, row: r };
  };
  const enabled = check(t("DiagnosticsDialog.diagnosticsEnabled"), "diagnostics.enabled", false);
  const modules = moduleRows().map(([m, label]) => [m, check(label, `diagnostics.module.${m}`, true).box] as const);
  const fullLogs = check(t("DiagnosticsDialog.fullTrace"), "diagnostics.fullTrace", false);
  const traceState = document.createElement("span");
  traceState.className = "diag-feedback";
  fullLogs.row.append(traceState);
  const scopes = scopeRows().map(([s, label]) => [s, check(label, `diagnostics.scope.${s}`, true).box] as const);
  const scopeNote = document.createElement("div");
  scopeNote.className = "diag-feedback diag-note";
  const migration = document.createElement("div");
  migration.className = "diag-feedback diag-note";
  migration.textContent = t("DiagnosticsDialog.gtpMigrationNote");
  group.append(scopeNote, migration);
  // The status area under the buttons (statusArea): 应用 and export results.
  const feedback = document.createElement("span");
  feedback.className = "diag-feedback";
  // A 完整日志 start or stop shows once the view after it arrives.
  let refreshPending = false;

  const active = () => state.view?.full_trace ?? [];
  /** setModulesEnabled (DiagnosticsDialog.java:1097-1102). */
  const modulesEnabled = () => { for (const [, box] of modules) box.disabled = !enabled.box.checked; };
  /** refreshScopeControls (DiagnosticsDialog.java:1007-1027): a running session's scopes are
   *  shown and locked while 完整日志 stays checked. */
  const refreshScopes = () => {
    const running = active();
    const locked = running.length > 0 && fullLogs.box.checked;
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
    enabled.box.checked = s.diagnosticsEnabled;
    for (const [m, box] of modules) box.checked = s.diagnosticModules.includes(m);
    const running = active();
    fullLogs.box.checked = running.length > 0;
    const shown = running.length ? running : s.preferredFullTraceScopes;
    for (const [sc, box] of scopes) box.checked = shown.includes(sc);
    refreshScopes();
    modulesEnabled();
  };
  enabled.box.onchange = modulesEnabled;
  fullLogs.box.onchange = refreshScopes;
  /** selectedScopes (DiagnosticsDialog.java:980-995): none checked means all. */
  const selectedScopes = (): TraceScope[] => {
    const chosen = scopes.filter(([, box]) => box.checked).map(([s]) => s);
    return chosen.length ? chosen : scopes.map(([s]) => s);
  };
  /** applyCurrentPlan (DiagnosticsDialog.java:418-475). */
  const apply = async () => {
    const saved = state.view?.settings.logging;
    if (!saved) return;
    const continues = active().length > 0 && fullLogs.box.checked;
    const next: LoggingSettings = {
      diagnosticsEnabled: enabled.box.checked,
      diagnosticModules: modules.filter(([, box]) => box.checked).map(([m]) => m),
      preferredFullTraceScopes: continues ? saved.preferredFullTraceScopes : selectedScopes(),
    };
    if ((await run({ type: "SetLogging", settings: next })) === null) {
      refreshHost();
      feedback.textContent = t("DiagnosticsDialog.applyFailed");
      return;
    }
    feedback.textContent = t("DiagnosticsDialog.applied");
    if (fullLogs.box.checked) {
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
  section(t("DiagnosticsDialog.readBoardPane"), sideColumn);
  const value = (label: string) => {
    const name = document.createElement("span");
    name.className = "diag-name";
    name.textContent = `${label}: `;
    const span = document.createElement("span");
    row(name, span).classList.add("diag-value");
    return span;
  };
  const capability = value(t("DiagnosticsDialog.helperCapability"));
  const persistence = value(t("DiagnosticsDialog.helperPersistence"));
  const drops = value(t("DiagnosticsDialog.helperDropCount"));
  const session = value(t("DiagnosticsDialog.helperProcessSession"));
  const helperRow = (label: string) => {
    const observed = document.createElement("span");
    observed.className = "diag-feedback";
    const box = document.createElement("input");
    box.type = "checkbox";
    const l = document.createElement("label");
    l.className = "diag-check";
    l.append(box, label);
    row(l, observed);
    return { observed, box };
  };
  const helperDiagnostics = helperRow(t("DiagnosticsDialog.helperDiagnostics"));
  const helperTrace = helperRow(t("DiagnosticsDialog.helperFullTrace"));
  const helperCapture = helperRow(t("DiagnosticsDialog.helperCapture"));
  const summary = document.createElement("span");
  row(summary).classList.add("diag-summary");
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

  section(t("DiagnosticsDialog.health"), sideColumn);
  row(product("diagnosticsLogs", bytes(status.app_log_bytes), bytes(status.crash_log_bytes)));
  const path = document.createElement("code");
  path.textContent = status.directory;
  const folder = document.createElement("span");
  folder.className = "diag-name";
  folder.textContent = `${t("DiagnosticsDialog.logsFolder")}: `;
  row(folder, path).classList.add("diag-value");
  row(
    button(t("DiagnosticsDialog.openLogs"), () => run({ type: "OpenLogFolder" })),
    button(t("DiagnosticsDialog.apply"), () => void apply()),
    button(t("DiagnosticsDialog.exportDefault"), async () => {
      feedback.textContent = t("DiagnosticsDialog.exporting");
      const r = await run({ type: "ExportDiagnostics" });
      feedback.textContent = r?.type === "Exported" ? `${t("DiagnosticsDialog.exportSuccess")}${r.path}` : r ? "" : t("DiagnosticsDialog.exportFailure");
    }),
  ).classList.add("diag-actions");
  row(feedback).classList.add("diag-status");
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
