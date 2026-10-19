// Help > 诊断与日志 (DiagnosticsDialog): engine GTP full trace plus application/crash logs.
// ReadBoard/network recording and trace scopes remain visibly unavailable.
import { run, state } from "./app";
import { logsStatus } from "./contract/client";
import { panel } from "./dialogs";
import { product, t } from "./i18n";

function bytes(n: number | null): string {
  return n === null ? product("none") : `${(n / 1024).toFixed(1)} KB`;
}

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
  await panel(t("DiagnosticsDialog.title"), body);
}
