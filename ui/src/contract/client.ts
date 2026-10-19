// The only bridge to the Rust side. Every UI entry (menus, toolbars, shortcuts, mouse) sends a
// `Command` through `dispatch`; state arrives only as `WorkbenchView`s.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ApiError, Command, CommandResult, ConsoleLineView, LogsStatus, WorkbenchView } from "./generated";

export function dispatch(command: Command): Promise<CommandResult> {
  return invoke<CommandResult>("dispatch_command", { command });
}

export function snapshot(): Promise<WorkbenchView> {
  return invoke<WorkbenchView>("workbench_snapshot");
}

export function logsStatus(): Promise<LogsStatus> {
  return invoke<LogsStatus>("logs_directory");
}

export function onView(handler: (view: WorkbenchView) => void): Promise<UnlistenFn> {
  return listen<WorkbenchView>("workbench-view", (e) => handler(e.payload));
}

export function engineConsole(since: number): Promise<ConsoleLineView[]> {
  return invoke<ConsoleLineView[]>("engine_console", { since });
}

export function onConsole(handler: (lines: ConsoleLineView[]) => void): Promise<UnlistenFn> {
  return listen<ConsoleLineView[]>("engine-console", (event) => handler(event.payload));
}

/** A Java entry whose capability is not migrated yet was used outside the UI (e.g. multi-file drop). */
export function onUnavailable(handler: (capability: string) => void): Promise<UnlistenFn> {
  return listen<string>("capability-unavailable", (e) => handler(e.payload));
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}
