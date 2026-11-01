// The only bridge to the Rust side. Every UI entry (menus, toolbars, shortcuts, mouse) sends a
// `Command` through `dispatch`; state arrives only as `WorkbenchView`s.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ApiError, AppearanceSettings, Command, CommandResult, ConsoleLineView, LogsStatus, SlotRef, ThemeSettings, WorkbenchView } from "./generated";
import type { AppearanceCatalog, ThemeImages } from "../appearance/model";

export function dispatch(command: Command): Promise<CommandResult> {
  return invoke<CommandResult>("dispatch_command", { command });
}

export function snapshot(): Promise<WorkbenchView> {
  return invoke<WorkbenchView>("workbench_snapshot");
}

export function logsStatus(): Promise<LogsStatus> {
  return invoke<LogsStatus>("logs_directory");
}

export async function onView(handler: (view: WorkbenchView) => void): Promise<UnlistenFn> {
  const channel = new Channel<WorkbenchView>();
  channel.onmessage = handler;
  const generation = await invoke<number>("subscribe_workbench", { channel });
  return () => {
    channel.onmessage = () => {};
    void invoke("unsubscribe_workbench", { generation });
  };
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

/** The picture of a 存档与读档 slot as stored (empty when it has none). */
export function recoveryThumbnail(slot: SlotRef): Promise<ArrayBuffer> {
  return invoke<ArrayBuffer>("recovery_thumbnail", { slot });
}

/** The themes (default first) and the system font families. Read-only. */
export function appearanceCatalog(): Promise<AppearanceCatalog> {
  return invoke<AppearanceCatalog>("appearance_catalog");
}

/** Loadable URLs of the images these values select (custom overrides first). Read-only. */
export function appearanceImages(settings: AppearanceSettings, theme: ThemeSettings): Promise<ThemeImages> {
  return invoke<ThemeImages>("appearance_images", { settings, theme });
}

/** The system chooser for one image file (JFileChooser FILES_ONLY); null when cancelled. */
export function chooseAppearanceImage(): Promise<string | null> {
  return invoke<string | null>("appearance_choose_image");
}

/** The window is closing with 自动存档 on: answer with `Recovery { op: ExitAutoSave }`. */
export function onExitAutoSaveRequest(handler: () => void): Promise<UnlistenFn> {
  return listen("exit-autosave-request", () => handler());
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}
