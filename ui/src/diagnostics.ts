// Help > 诊断与日志 (DiagnosticsDialog): engine GTP full trace plus application/crash logs, and the
// ReadBoard helper's own log writers. The host's ReadBoard/network recording and trace scopes
// remain visibly unavailable.
import { registerViewHandler, run, state } from "./app";
import { logsStatus } from "./contract/client";
import type { HelperLoggingView, Persistence, Presentation } from "./contract/generated";
import { confirm, panel } from "./dialogs";
import { format, product, t } from "./i18n";

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
  const body = document.createElement("div");
  body.className = "diagnostics";
  const section = (title: string) => {
    const h = document.createElement("div");
    h.className = "diag-section";
    h.textContent = title;
    body.append(h);
  };
  const row = (...children: (HTMLElement | string)[]) => {
    const r = document.createElement("div");
    r.className = "diag-row";
    r.append(...children);
    body.append(r);
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

  section(t("DiagnosticsDialog.hostPane"));
  const modules = row(`${t("DiagnosticsDialog.diagnosticsEnabled")}:`);
  const moduleNames = [
    t("DiagnosticsDialog.module.engine"),
    t("DiagnosticsDialog.module.gtpSummary"),
    t("DiagnosticsDialog.module.readboardYike"),
    t("DiagnosticsDialog.module.networkRemote"),
  ];
  for (const name of moduleNames) {
    const label = document.createElement("label");
    label.className = "unavailable";
    label.title = product("unavailable");
    const box = document.createElement("input");
    box.type = "checkbox";
    box.disabled = true;
    label.append(box, name);
    modules.append(label);
  }
  const traceState = document.createElement("span");
  const trace = document.createElement("input"); trace.type = "checkbox";
  const traceLabel = document.createElement("label"); traceLabel.append(trace, t("DiagnosticsDialog.fullTrace"));
  const scope = document.createElement("input"); scope.type = "checkbox"; scope.checked = true; scope.disabled = true;
  const scopeLabel = document.createElement("label"); scopeLabel.append(scope, t("DiagnosticsDialog.scope.engineGtp"));
  row(traceLabel, scopeLabel, traceState);
  for (const text of [t("DiagnosticsDialog.scope.readboardYike"), t("DiagnosticsDialog.scope.networkWebsocket")]) {
    const label = document.createElement("label"); label.className = "unavailable"; label.title = product("unavailable");
    const check = document.createElement("input"); check.type = "checkbox"; check.disabled = true;
    label.append(check, text); row(label);
  }
  const updateTrace = () => {
    trace.checked = state.view?.engine.full_trace ?? false;
    traceState.textContent = trace.checked ? t("DiagnosticsDialog.stopFullTrace") : t("DiagnosticsDialog.traceOff");
  };
  updateTrace();
  trace.onchange = async () => { await run({ type: "SetFullTrace", enabled: trace.checked }); updateTrace(); };

  // ReadBoard (DiagnosticsDialog.java:354-388): the helper's capability, persistence, dropped
  // events and process session, then the desired writers beside what the helper confirmed, then
  // the capture session. Without a helper the values are Java's detached snapshot; a toggle there
  // is refused and the boxes go back (requestSet false, refreshHelperFromSnapshot).
  section(t("DiagnosticsDialog.readBoardPane"));
  const value = (label: string) => {
    const span = document.createElement("span");
    row(`${label}: `, span);
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
    row(label, observed, box);
    return { observed, box };
  };
  const helperDiagnostics = helperRow(t("DiagnosticsDialog.helperDiagnostics"));
  const helperTrace = helperRow(t("DiagnosticsDialog.helperFullTrace"));
  const helperCapture = helperRow(t("DiagnosticsDialog.helperCapture"));
  const summary = document.createElement("span");
  row(summary);
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

  section(t("DiagnosticsDialog.health"));
  row(product("diagnosticsLogs", bytes(status.app_log_bytes), bytes(status.crash_log_bytes)));
  const path = document.createElement("code");
  path.textContent = status.directory;
  row(`${t("DiagnosticsDialog.logsFolder")}: `, path);
  const feedback = document.createElement("span");
  feedback.className = "diag-feedback";
  row(
    button(t("DiagnosticsDialog.openLogs"), () => run({ type: "OpenLogFolder" })),
    button(t("DiagnosticsDialog.exportDefault"), async () => {
      feedback.textContent = t("DiagnosticsDialog.exporting");
      const r = await run({ type: "ExportDiagnostics" });
      feedback.textContent = r?.type === "Exported" ? `${t("DiagnosticsDialog.exportSuccess")}${r.path}` : r ? "" : t("DiagnosticsDialog.exportFailure");
    }),
    feedback,
  );
  refreshReadBoard = updateReadBoard;
  try {
    await panel(t("DiagnosticsDialog.title"), body);
  } finally {
    if (refreshReadBoard === updateReadBoard) refreshReadBoard = null;
  }
}
