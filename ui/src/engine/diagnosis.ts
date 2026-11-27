import type { StartupDiagnosis } from "../contract/generated";
import { format, product, registerProduct, t } from "../i18n";

// Windows error 206 while an engine loads a DLL: what to do, with the names of 设置引擎
// (MoreEngines.title) and its 命令行 (MoreEngines.lblCommand). Java's text sends the user to a
// portable package, which does not hold this product's engines.
registerProduct({
  ZhCn: {
    enginePathTooLong: "Windows 报告路径过长（错误 206），引擎未能启动。请把引擎所在的整个文件夹（连同其中的 DLL）移到路径更短的位置，例如 C:\\katago，然后在“设置引擎”中把命令行改为新路径。不要单独移动其中的 DLL。",
  },
  ZhTw: {
    enginePathTooLong: "Windows 回報路徑過長（錯誤 206），引擎未能啟動。請把引擎所在的整個資料夾（連同其中的 DLL）移到路徑更短的位置，例如 C:\\katago，然後在「設定引擎」中把命令列改為新路徑。不要單獨移動其中的 DLL。",
  },
  EnUs: {
    enginePathTooLong: "Windows reported a path that is too long (error 206), so the engine could not start. Move the engine's whole folder, with the DLLs in it, to a shorter path such as C:\\katago, then change its command in Engine Management to the new path. Do not move the DLLs on their own.",
  },
  JaJp: {
    enginePathTooLong: "Windows がパスが長すぎると報告したため（エラー 206）、エンジンを起動できませんでした。エンジンのあるフォルダーを中の DLL ごと C:\\katago などの短いパスに移動し、「エンジン管理」でコマンドを新しいパスに変更してください。DLL だけを移動しないでください。",
  },
  KoKr: {
    enginePathTooLong: "Windows에서 경로가 너무 길다고 보고하여(오류 206) 엔진을 시작하지 못했습니다. 엔진이 있는 폴더 전체를 안의 DLL과 함께 C:\\katago처럼 경로가 짧은 곳으로 옮긴 뒤 '인공지능 엔진 설정'에서 명령어를 새 경로로 바꾸세요. DLL만 따로 옮기지 마세요.",
  },
  ThTh: {
    enginePathTooLong: "Windows รายงานว่าเส้นทางยาวเกินไป (ข้อผิดพลาด 206) จึงเริ่มเอนจินไม่ได้ ให้ย้ายโฟลเดอร์ของเอนจินทั้งโฟลเดอร์พร้อมไฟล์ DLL ข้างในไปยังเส้นทางที่สั้นกว่า เช่น C:\\katago แล้วแก้คำสั่งใน “การจัดการเครื่องยนต์” เป็นเส้นทางใหม่ อย่าย้ายเฉพาะไฟล์ DLL",
  },
});

/** What to do about an engine start that failed for a known reason: Windows error 206 or a
 *  packaged engine too new for this macOS (MacOsEngineCompatibility, KataGoSetup.macos.minimumVersion). */
export function diagnosisText(diagnosis: StartupDiagnosis): string {
  return diagnosis.type === "PathTooLong"
    ? product("enginePathTooLong")
    : format(t("KataGoSetup.macos.minimumVersion"), diagnosis.minimum, diagnosis.actual);
}
