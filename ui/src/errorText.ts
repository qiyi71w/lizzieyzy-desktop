// The text a failed command shows. An ApiError's message is one of:
// - display text (shown as it is);
// - an interface text key (`Fox.conflict`, `product.settingsSaveFailed`);
// - a key, a line break, then detail: `"<key>\n<detail>"`. The detail (the operating system's
//   reason for a settings-file write, see `SETTINGS_SAVE_FAILED` in the core's `workbench.rs`)
//   is shown on its own line under the translated text.
// The core also passes such a message on as the detail of a view outcome (a benchmark result, a
// 搜索线程 save, a HumanSL start); those go through `messageText`.
// Every key is written out so the Java string extractor keeps it.
import { isApiError } from "./contract/client";
import { product, registerProduct, t } from "./i18n";

registerProduct({
  ZhCn: {
    settingsSaveFailed: "设置未能保存。请解决下面列出的问题（例如确认设置文件夹可以写入），然后重试。",
    settingsFileKept: "本次运行不会保存设置：程序启动时未能使用设置文件，为保留您原有的设置，该文件保持不变。请解决下面列出的问题，然后重新启动程序。",
    windowStateSaveFailed: "窗口位置和面板大小未能保存。请解决下面列出的问题（例如确认设置文件夹可以写入）；下次更改时会再次保存。",
    windowStateResetFailed: "未能重置界面位置。请解决下面列出的问题（例如确认设置文件夹可以写入），然后重试。",
  },
  ZhTw: {
    settingsSaveFailed: "設定未能儲存。請解決下面列出的問題（例如確認設定資料夾可以寫入），然後重試。",
    settingsFileKept: "本次執行不會儲存設定：程式啟動時未能使用設定檔，為保留您原有的設定，該檔案保持不變。請解決下面列出的問題，然後重新啟動程式。",
    windowStateSaveFailed: "視窗位置和面板大小未能儲存。請解決下面列出的問題（例如確認設定資料夾可以寫入）；下次變更時會再次儲存。",
    windowStateResetFailed: "未能重置介面位置。請解決下面列出的問題（例如確認設定資料夾可以寫入），然後重試。",
  },
  EnUs: {
    settingsSaveFailed: "Settings could not be saved. Fix the problem below (for example, make sure the settings folder can be written to), then try again.",
    settingsFileKept: "Settings are not saved in this session: the settings file could not be used when the program started, so it is left unchanged to keep your settings. Fix the problem below, then restart the program.",
    windowStateSaveFailed: "Window positions and panel sizes could not be saved. Fix the problem below (for example, make sure the settings folder can be written to); they are saved again the next time they change.",
    windowStateResetFailed: "The frame location could not be reset. Fix the problem below (for example, make sure the settings folder can be written to), then try again.",
  },
  JaJp: {
    settingsSaveFailed: "設定を保存できませんでした。下記の問題を解決してから（例: 設定フォルダーに書き込めるか確認）、もう一度お試しください。",
    settingsFileKept: "今回の実行中は設定を保存しません。起動時に設定ファイルを使用できなかったため、元の設定を守るためにファイルはそのまま残しています。下記の問題を解決してから、プログラムを再起動してください。",
    windowStateSaveFailed: "ウィンドウの位置とパネルのサイズを保存できませんでした。下記の問題を解決してください（例: 設定フォルダーに書き込めるか確認）。次に変更したときに改めて保存します。",
    windowStateResetFailed: "フレームの位置をリセットできませんでした。下記の問題を解決してから（例: 設定フォルダーに書き込めるか確認）、もう一度お試しください。",
  },
  KoKr: {
    settingsSaveFailed: "설정을 저장하지 못했습니다. 아래 문제를 해결한 뒤(예: 설정 폴더에 쓸 수 있는지 확인) 다시 시도하세요.",
    settingsFileKept: "이번 실행에서는 설정을 저장하지 않습니다. 프로그램 시작 시 설정 파일을 사용할 수 없어 기존 설정을 보존하도록 파일을 그대로 두었습니다. 아래 문제를 해결한 뒤 프로그램을 다시 시작하세요.",
    windowStateSaveFailed: "창 위치와 패널 크기를 저장하지 못했습니다. 아래 문제를 해결하세요(예: 설정 폴더에 쓸 수 있는지 확인). 다음에 변경할 때 다시 저장합니다.",
    windowStateResetFailed: "창 위치를 초기화하지 못했습니다. 아래 문제를 해결한 뒤(예: 설정 폴더에 쓸 수 있는지 확인) 다시 시도하세요.",
  },
  ThTh: {
    settingsSaveFailed: "บันทึกการตั้งค่าไม่ได้ โปรดแก้ไขปัญหาด้านล่าง (เช่น ตรวจสอบว่าโฟลเดอร์การตั้งค่าเขียนได้) แล้วลองอีกครั้ง",
    settingsFileKept: "รอบการทำงานนี้จะไม่บันทึกการตั้งค่า เนื่องจากใช้ไฟล์การตั้งค่าไม่ได้ตอนเริ่มโปรแกรม จึงคงไฟล์ไว้ตามเดิมเพื่อรักษาการตั้งค่าของคุณ โปรดแก้ไขปัญหาด้านล่าง แล้วเริ่มโปรแกรมใหม่",
    windowStateSaveFailed: "บันทึกตำแหน่งหน้าต่างและขนาดแผงไม่ได้ โปรดแก้ไขปัญหาด้านล่าง (เช่น ตรวจสอบว่าโฟลเดอร์การตั้งค่าเขียนได้) ระบบจะบันทึกอีกครั้งเมื่อมีการเปลี่ยนแปลงครั้งถัดไป",
    windowStateResetFailed: "รีเซ็ตตำแหน่งเฟรมไม่ได้ โปรดแก้ไขปัญหาด้านล่าง (เช่น ตรวจสอบว่าโฟลเดอร์การตั้งค่าเขียนได้) แล้วลองอีกครั้ง",
  },
});

/** The message of `e` for a dialog or status line (see `messageText`). */
export function errorText(e: unknown, other?: (message: string) => string): string {
  return isApiError(e) ? messageText(e.message, other) : String(e);
}

/** The text of an ApiError message. A message that is no known key goes through `other` (a
 *  feature's own key map, e.g. `tuningText`), which by default shows it as it is. */
export function messageText(message: string, other: (message: string) => string = (text) => text): string {
  const texts: Record<string, string> = {
    // Settings-file writes (with the cause as detail).
    "product.settingsSaveFailed": product("settingsSaveFailed"),
    "product.settingsFileKept": product("settingsFileKept"),
    "product.windowStateSaveFailed": product("windowStateSaveFailed"),
    "product.windowStateResetFailed": product("windowStateResetFailed"),
    // Engine and board commands while Fox direct connection, a whole-game analysis, Web 试下,
    // a ReadBoard GMA lease or 跑谱贡献 holds the engine.
    "Fox.conflict": t("Fox.conflict"),
    "WholeGameAnalysis.conflict.analysis": t("WholeGameAnalysis.conflict.analysis"),
    "AnalysisSettings.reuseStatus.application_exclusive_mode": t("AnalysisSettings.reuseStatus.application_exclusive_mode"),
    "AnalysisSettings.reuseStatus.existing_lease": t("AnalysisSettings.reuseStatus.existing_lease"),
    "Contribute.tips.contributingAndStartAnotherLizzieYzy": t("Contribute.tips.contributingAndStartAnotherLizzieYzy"),
    // 引擎对战 with saved match rules that no longer parse.
    "NewEngineGameDialog.matchRulesCorrupt": t("NewEngineGameDialog.matchRulesCorrupt"),
    // A web page the system cannot open (跑谱贡献 links, KataGo downloads).
    "AutoSetup.openBrowserUnsupported": t("AutoSetup.openBrowserUnsupported"),
  };
  const cut = message.indexOf("\n");
  const key = cut < 0 ? message : message.slice(0, cut);
  const text = texts[key];
  if (text === undefined) return other(message);
  return cut < 0 ? text : `${text}\n${message.slice(cut + 1)}`;
}
