// The only bridge to the Rust side. Mutating UI entries send `Command` through `dispatch`;
// immutable function search is independent of the workbench and its lock; the 规则面板's rule set
// query only reads it.
import { Channel, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { ApiError, AppearanceCatalog, AppearanceSettings, Command, CommandResult, ConsoleLineView, FunctionMatch, Language, LogsStatus, SlotRef, ThemeImages, ThemeSettings, WorkbenchView } from "./generated";

/** Called just before a request that can show a native modal dialog (a file or folder chooser)
 *  goes to the Rust side, so what a modal dialog ends (布局编辑模式) ends before it shows. */
const nativeDialogListeners: (() => void)[] = [];

export function onNativeDialog(listener: () => void): void {
  nativeDialogListeners.push(listener);
}

/** A native modal dialog is about to show. */
export function nativeDialog(): void {
  for (const listener of nativeDialogListeners) listener();
}

/** Whether the adapter answers `command` through a native chooser (src-tauri: every
 *  `blocking_pick_*` / `blocking_save_file`): always, or when it names no path. 保存 chooses only
 *  for a record without a file of its own; its caller tells (app.ts saveRecord). */
function chooses(command: Command): boolean {
  switch (command.type) {
    case "ChooseAndOpenRecord":
    case "ChooseAndSaveRecord":
    case "ExportRecord":
    case "ExportImage":
    case "ExportTable":
    case "ExportDiagnostics":
    case "CloudImportQr":
    case "ChooseQuickLinkProgram":
    case "ChooseJavaFolder":
      return true;
    case "Batch":
      return command.op.type === "Choose" || command.op.type === "AddFiles";
    case "SshCredential":
      return command.op.type === "ChooseKeyFile";
    case "Contribution":
      return command.op.type === "ChooseFile";
    case "Tuning":
      return command.op.type === "Scan" || command.op.type === "ScanIkatago" || (command.op.type === "ImportMeasured" && command.op.path === null);
    case "Resource": {
      const op = command.op;
      if (op.type === "ChooseLocalEngine") return op.choice.executable === null || op.choice.config === null || op.choice.weight === null;
      return (op.type === "ImportPackage" || op.type === "ImportWeight" || op.type === "ImportHumanSlModel") && op.path === null;
    }
    default:
      return false;
  }
}

export function dispatch(command: Command): Promise<CommandResult> {
  if (chooses(command)) nativeDialog();
  return invoke<CommandResult>("dispatch_command", { command });
}

export function searchFunctions(query: string, language: Language): Promise<FunctionMatch[]> {
  return invoke<FunctionMatch[]>("search_functions", { query, language });
}

/** 规则面板: the rule set `rules` (KataGo rules JSON) and `komi` are, by id; `picked` is the rule
 *  set picked last in the panel. Null: no rule set (自定义). */
export function ruleSetOf(rules: string, komi: number, picked: string | null): Promise<string | null> {
  return invoke<string | null>("rule_set_of", { rules, komi, picked });
}

export function snapshot(): Promise<WorkbenchView> {
  return invoke<WorkbenchView>("workbench_snapshot");
}

export function logsStatus(): Promise<LogsStatus> {
  return invoke<LogsStatus>("logs_directory");
}

declare global {
  interface Window {
    /** WebView2's host bridge; absent in other webviews. */
    chrome?: { webview?: EventTarget };
  }
}

/** Views come over the channel, or as web messages carrying their JSON where the adapter posts
 *  them that way (WebView2: `ViewSink` in `src-tauri/src/lib.rs`). */
export async function onView(handler: (view: WorkbenchView) => void): Promise<UnlistenFn> {
  const channel = new Channel<WorkbenchView>();
  channel.onmessage = handler;
  const webview = window.chrome?.webview;
  // WebView2 dispatches a plain Event carrying `data`, not a MessageEvent.
  const message = (e: Event) => {
    if ("data" in e && typeof e.data === "string") handler(JSON.parse(e.data) as WorkbenchView);
  };
  webview?.addEventListener("message", message);
  const generation = await invoke<number>("subscribe_workbench", { channel });
  return () => {
    channel.onmessage = () => {};
    webview?.removeEventListener("message", message);
    void invoke("unsubscribe_workbench", { generation });
  };
}

export function engineConsole(since: number): Promise<ConsoleLineView[]> {
  return invoke<ConsoleLineView[]>("engine_console", { since });
}

/** 引擎对战 加载开局SGF: the adapter reads the files (an empty list opens the native multi-file SGF
 *  chooser) and replaces the match openings (Match ReplaceOpenings). */
export function openMatchOpenings(paths: string[]): Promise<CommandResult> {
  if (!paths.length) nativeDialog();
  return invoke<CommandResult>("open_match_openings", { paths });
}

/** The finished game's 胜率图 PNG for 自动保存胜率图, written to the path the core chose. */
export function writeMatchWinrate(game: number, bytes: number[]): Promise<CommandResult> {
  return invoke<CommandResult>("write_match_winrate", { game, bytes });
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
  nativeDialog();
  return invoke<string | null>("appearance_choose_image");
}

/** 棋力评估 导入模型: the system chooser titled `title` with the "XGBoost strength model (*.json,
 *  *.zip)" filter (JFileChooser.showOpenDialog); null when cancelled. */
export function chooseStrengthModel(title: string): Promise<string | null> {
  nativeDialog();
  return invoke<string | null>("strength_choose_model", { title });
}

/** The window is closing with 自动存档 on: answer with `Recovery { op: ExitAutoSave }`. */
export function onExitAutoSaveRequest(handler: () => void): Promise<UnlistenFn> {
  return listen("exit-autosave-request", () => handler());
}

export function isApiError(e: unknown): e is ApiError {
  return typeof e === "object" && e !== null && "kind" in e && "message" in e;
}
