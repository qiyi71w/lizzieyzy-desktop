// 帮助 > 检查更新 (Menu.java:5274-5277): the 检查更新 page (CheckUpdateDialog) as rows of 综合设置 > 关于,
// with the offer a check found and its download in place below them, and the 发现新版本 window
// (WindowsUpdateDialog) for an offer found while those rows are not in view. Rust owns the
// settings, the check, the package preparation and the installation; this module shows
// `view.update` and sends its commands. Opening the rows uses no network. The options save at
// once, apart from 综合设置's 确定 / 取消, and the automatic ones apply from the next start.
import { invoke } from "@tauri-apps/api/core";
import { registerViewHandler, run, state } from "./app";
import { COMMANDS } from "./commands";
import type { UpdateDownloadView, UpdateError, UpdateOfferView, UpdateResultView, UpdateSettings, UpdateView } from "./contract/generated";
import { message } from "./dialogs";
import { product, registerProduct, t } from "./i18n";
import { icon } from "./design/icons";
import { openSettingsAt, registerSettingTargets } from "./engine/settingsTargets";
import "./update.css";

registerProduct({
  ZhCn: {
    updateSigningNotConfigured: "此版本没有更新签名公钥，无法检查更新。",
    updateUnsupportedForm: "当前安装形式不支持程序内更新。",
    updateErrorSources: "无法下载更新文件，请检查网络后重试。",
    updateErrorInstall: "无法安装更新，当前程序未改变。请重试，或打开 Release 页面手动下载。",
    updateIntroInPlace: "这次会先下载并校验更新文件，然后替换程序文件并重新打开。",
    updateReadyIntroInPlace: "更新文件已下载并校验。点击“立即更新”后，将替换程序文件并重新打开。",
    updatePreserved: "用户数据、棋谱、设置、引擎和权重都会保留。",
    updateInstalling: "正在安装更新，程序将关闭并重新打开…",
    updateErrorPermission: "程序所在文件夹无法写入，未安装更新，当前程序未改变。请把程序文件夹移到可写入的位置后重试，或打开 Release 页面手动下载。",
    updateErrorInUse: "有程序文件正在使用，无法替换。已恢复原文件，当前程序未改变。请关闭正在使用这些文件的程序（例如另一个已打开的本程序或棋盘同步工具）后重试。",
    updateErrorInterrupted: "上次更新被中断，已恢复原文件，当前程序未改变。请重新检查更新。",
    updateErrorUnrecovered: "上次更新被中断，部分原文件暂未恢复。请关闭其他使用程序文件夹的程序后重新启动本程序；仍有问题请从 Release 页面重新下载程序。",
    updateIntroDeb: "这次会先下载并校验安装包，然后交给系统软件包管理（apt）安装：系统会请你授权，安装完成后程序重新打开。",
    updateReadyIntroDeb: "安装包已下载并校验。点击“立即更新”后，系统会请你授权，由系统软件包管理（apt）安装，完成后程序重新打开。",
    updateInstallingDeb: "正在等待系统授权并安装更新，完成后程序会重新打开…",
    updateErrorDismissed: "已取消系统授权，未安装更新，当前程序未改变。",
    updateErrorUnauthorized: "未获得系统授权，未安装更新，当前程序未改变。可以重试，或在终端用下面的命令安装已校验的安装包：",
    updateErrorPackageManager: "系统软件包管理没有安装这个更新，当前程序未改变。",
    updateErrorPackageUnfinished: "更新没有装完，本程序的软件包处于未完成状态。请在终端运行 sudo dpkg --configure -a 完成安装；如果仍未解决，请重新安装这个安装包（可从 Release 页面下载）。",
  },
  ZhTw: {
    updateSigningNotConfigured: "此版本沒有更新簽名公鑰，無法檢查更新。",
    updateUnsupportedForm: "目前的安裝形式不支援程式內更新。",
    updateErrorSources: "無法下載更新檔案，請檢查網路後重試。",
    updateErrorInstall: "無法安裝更新，目前的程式未改變。請重試，或開啟 Release 頁面手動下載。",
    updateIntroInPlace: "這次會先下載並校驗更新檔案，然後替換程式檔案並重新開啟。",
    updateReadyIntroInPlace: "更新檔案已下載並校驗。點擊「立即更新」後，將替換程式檔案並重新開啟。",
    updatePreserved: "使用者資料、棋譜、設定、引擎和權重都會保留。",
    updateInstalling: "正在安裝更新，程式將關閉並重新開啟…",
    updateErrorPermission: "程式所在資料夾無法寫入，未安裝更新，目前的程式未改變。請將程式資料夾移到可寫入的位置後重試，或開啟 Release 頁面手動下載。",
    updateErrorInUse: "有程式檔案正在使用，無法替換。已還原原檔案，目前的程式未改變。請關閉正在使用這些檔案的程式（例如另一個已開啟的本程式或棋盤同步工具）後重試。",
    updateErrorInterrupted: "上次更新被中斷，已還原原檔案，目前的程式未改變。請重新檢查更新。",
    updateErrorUnrecovered: "上次更新被中斷，部分原檔案暫未還原。請關閉其他使用程式資料夾的程式後重新啟動本程式；仍有問題請從 Release 頁面重新下載程式。",
    updateIntroDeb: "這次會先下載並校驗安裝套件，然後交給系統套件管理（apt）安裝：系統會請你授權，安裝完成後程式重新開啟。",
    updateReadyIntroDeb: "安裝套件已下載並校驗。點擊「立即更新」後，系統會請你授權，由系統套件管理（apt）安裝，完成後程式重新開啟。",
    updateInstallingDeb: "正在等待系統授權並安裝更新，完成後程式會重新開啟…",
    updateErrorDismissed: "已取消系統授權，未安裝更新，目前的程式未改變。",
    updateErrorUnauthorized: "未取得系統授權，未安裝更新，目前的程式未改變。可以重試，或在終端機用下面的指令安裝已校驗的安裝套件：",
    updateErrorPackageManager: "系統套件管理沒有安裝這個更新，目前的程式未改變。",
    updateErrorPackageUnfinished: "更新沒有裝完，本程式的套件處於未完成狀態。請在終端機執行 sudo dpkg --configure -a 完成安裝；如果仍未解決，請重新安裝這個安裝套件（可從 Release 頁面下載）。",
  },
  EnUs: {
    updateSigningNotConfigured: "This build has no update signing key, so it cannot check for updates.",
    updateUnsupportedForm: "This kind of installation cannot update from within the program.",
    updateErrorSources: "The update could not be downloaded. Check your network and retry.",
    updateErrorInstall: "The update could not be installed; the program is unchanged. Retry, or download it from the Release page.",
    updateIntroInPlace: "The update is downloaded and verified first; the program files are then replaced and the program restarts.",
    updateReadyIntroInPlace: "The update is downloaded and verified. Update now replaces the program files and restarts the program.",
    updatePreserved: "User data, game records, settings, engines and weights are kept.",
    updateInstalling: "Installing the update; the program closes and restarts…",
    updateErrorPermission: "The program's folder cannot be written, so the update was not installed and the program is unchanged. Move the folder to a writable location and retry, or download the update from the Release page.",
    updateErrorInUse: "A program file is in use and could not be replaced. The original files were restored; the program is unchanged. Close whatever uses them (another copy of this program or the board synchronization tool, for example) and retry.",
    updateErrorInterrupted: "The last update was interrupted. The original files were restored; the program is unchanged. Check for updates to try again.",
    updateErrorUnrecovered: "The last update was interrupted and some original files could not be restored yet. Close other programs that use the program's folder and start this program again; if the problem remains, download the program again from the Release page.",
    updateIntroDeb: "The update is downloaded and verified first, then handed to the system package manager (apt): the system asks you to authorize it, and the program restarts once it is installed.",
    updateReadyIntroDeb: "The update is downloaded and verified. After Update now, the system asks you to authorize it, the system package manager (apt) installs it, and the program restarts.",
    updateInstallingDeb: "Waiting for system authorization and installing the update; the program restarts when it is done…",
    updateErrorDismissed: "System authorization was cancelled. The update was not installed; the program is unchanged.",
    updateErrorUnauthorized: "The system did not authorize the installation. The update was not installed; the program is unchanged. Retry, or install the verified package from a terminal with this command:",
    updateErrorPackageManager: "The system package manager did not install this update; the program is unchanged.",
    updateErrorPackageUnfinished: "The update was not fully installed, and the program's package is left unfinished. Run sudo dpkg --configure -a in a terminal to finish it; if that does not help, install the package again (it can be downloaded from the Release page).",
  },
  JaJp: {
    updateSigningNotConfigured: "このビルドには更新署名用の公開鍵がないため、更新を確認できません。",
    updateUnsupportedForm: "このインストール形式では、プログラム内から更新できません。",
    updateErrorSources: "更新ファイルをダウンロードできませんでした。ネットワークを確認して再試行してください。",
    updateErrorInstall: "更新をインストールできませんでした。プログラムは変更されていません。再試行するか、Release ページを開いて手動でダウンロードしてください。",
    updateIntroInPlace: "更新ファイルをダウンロードして検証したあと、プログラムファイルを置き換えて再起動します。",
    updateReadyIntroInPlace: "更新ファイルはダウンロード・検証済みです。「今すぐアップデートしてください」を押すと、プログラムファイルを置き換えて再起動します。",
    updatePreserved: "ユーザーデータ、棋譜、設定、エンジン、ウェイトはすべて保持されます。",
    updateInstalling: "更新をインストールしています。プログラムを終了して再起動します…",
    updateErrorPermission: "プログラムのフォルダーに書き込めないため、更新はインストールされず、プログラムは変更されていません。書き込み可能な場所にフォルダーを移動して再試行するか、Release ページを開いて手動でダウンロードしてください。",
    updateErrorInUse: "使用中のプログラムファイルがあり、置き換えられませんでした。元のファイルを復元したため、プログラムは変更されていません。これらのファイルを使用しているプログラム（別に起動している本プログラムや盤面同期ツールなど）を閉じてから再試行してください。",
    updateErrorInterrupted: "前回の更新が中断されました。元のファイルを復元したため、プログラムは変更されていません。もう一度更新を確認してください。",
    updateErrorUnrecovered: "前回の更新が中断され、一部の元のファイルをまだ復元できていません。プログラムのフォルダーを使用している他のプログラムを閉じてから、本プログラムを再起動してください。解決しない場合は Release ページからプログラムを再ダウンロードしてください。",
    updateIntroDeb: "インストールパッケージをダウンロードして検証したあと、システムのパッケージ管理（apt）でインストールします。システムから認証を求められ、インストール後にプログラムが再起動します。",
    updateReadyIntroDeb: "インストールパッケージはダウンロード・検証済みです。「今すぐアップデートしてください」を押すとシステムから認証を求められ、システムのパッケージ管理（apt）でインストールしたあと、プログラムが再起動します。",
    updateInstallingDeb: "システムの認証を待って更新をインストールしています。完了するとプログラムが再起動します…",
    updateErrorDismissed: "システムの認証がキャンセルされました。更新はインストールされず、プログラムは変更されていません。",
    updateErrorUnauthorized: "システムの認証を得られませんでした。更新はインストールされず、プログラムは変更されていません。再試行するか、ターミナルで次のコマンドを実行して検証済みのパッケージをインストールしてください：",
    updateErrorPackageManager: "システムのパッケージ管理はこの更新をインストールしませんでした。プログラムは変更されていません。",
    updateErrorPackageUnfinished: "更新のインストールが完了せず、本プログラムのパッケージが未完了の状態です。ターミナルで sudo dpkg --configure -a を実行してインストールを完了してください。解決しない場合は、このインストールパッケージを再インストールしてください（Release ページからダウンロードできます）。",
  },
  KoKr: {
    updateSigningNotConfigured: "이 버전에는 업데이트 서명 공개 키가 없어 업데이트를 확인할 수 없습니다.",
    updateUnsupportedForm: "현재 설치 형태에서는 프로그램 안에서 업데이트할 수 없습니다.",
    updateErrorSources: "업데이트 파일을 다운로드하지 못했습니다. 네트워크를 확인한 뒤 다시 시도하세요.",
    updateErrorInstall: "업데이트를 설치하지 못했습니다. 현재 프로그램은 변경되지 않았습니다. 다시 시도하거나 Release 페이지에서 직접 다운로드하세요.",
    updateIntroInPlace: "먼저 업데이트 파일을 다운로드해 검증한 뒤, 프로그램 파일을 교체하고 다시 시작합니다.",
    updateReadyIntroInPlace: "업데이트 파일 다운로드와 검증이 끝났습니다. “지금 업데이트”를 누르면 프로그램 파일을 교체하고 다시 시작합니다.",
    updatePreserved: "사용자 데이터, 기보, 설정, 엔진, 가중치는 모두 유지됩니다.",
    updateInstalling: "업데이트를 설치하는 중입니다. 프로그램이 닫힌 뒤 다시 시작됩니다…",
    updateErrorPermission: "프로그램 폴더에 쓸 수 없어 업데이트를 설치하지 않았습니다. 현재 프로그램은 변경되지 않았습니다. 프로그램 폴더를 쓰기 가능한 위치로 옮긴 뒤 다시 시도하거나 Release 페이지에서 직접 다운로드하세요.",
    updateErrorInUse: "사용 중인 프로그램 파일이 있어 교체할 수 없습니다. 원래 파일을 복구했으며 현재 프로그램은 변경되지 않았습니다. 이 파일을 사용 중인 프로그램(예: 이미 열려 있는 이 프로그램의 다른 창이나 바둑판 동기화 도구)을 닫은 뒤 다시 시도하세요.",
    updateErrorInterrupted: "지난 업데이트가 중단되어 원래 파일을 복구했습니다. 현재 프로그램은 변경되지 않았습니다. 업데이트를 다시 확인하세요.",
    updateErrorUnrecovered: "지난 업데이트가 중단되어 일부 원래 파일을 아직 복구하지 못했습니다. 프로그램 폴더를 사용 중인 다른 프로그램을 닫고 이 프로그램을 다시 시작하세요. 문제가 계속되면 Release 페이지에서 프로그램을 다시 다운로드하세요.",
    updateIntroDeb: "먼저 설치 패키지를 다운로드해 검증한 뒤 시스템 패키지 관리자(apt)로 설치합니다. 시스템에서 승인을 요청하며, 설치가 끝나면 프로그램이 다시 시작됩니다.",
    updateReadyIntroDeb: "설치 패키지 다운로드와 검증이 끝났습니다. “지금 업데이트”를 누르면 시스템에서 승인을 요청하고 시스템 패키지 관리자(apt)가 설치한 뒤 프로그램이 다시 시작됩니다.",
    updateInstallingDeb: "시스템 승인을 기다리며 업데이트를 설치하는 중입니다. 완료되면 프로그램이 다시 시작됩니다…",
    updateErrorDismissed: "시스템 승인이 취소되어 업데이트를 설치하지 않았습니다. 현재 프로그램은 변경되지 않았습니다.",
    updateErrorUnauthorized: "시스템 승인을 받지 못해 업데이트를 설치하지 않았습니다. 현재 프로그램은 변경되지 않았습니다. 다시 시도하거나 터미널에서 아래 명령으로 검증된 설치 패키지를 설치하세요:",
    updateErrorPackageManager: "시스템 패키지 관리자가 이 업데이트를 설치하지 않았습니다. 현재 프로그램은 변경되지 않았습니다.",
    updateErrorPackageUnfinished: "업데이트가 끝까지 설치되지 않아 이 프로그램의 패키지가 미완료 상태입니다. 터미널에서 sudo dpkg --configure -a 를 실행해 설치를 마치세요. 그래도 해결되지 않으면 이 설치 패키지를 다시 설치하세요(Release 페이지에서 다운로드할 수 있습니다).",
  },
  ThTh: {
    updateSigningNotConfigured: "รุ่นนี้ไม่มีกุญแจสาธารณะสำหรับลายเซ็นอัปเดต จึงตรวจสอบการอัปเดตไม่ได้",
    updateUnsupportedForm: "รูปแบบการติดตั้งนี้ไม่รองรับการอัปเดตจากภายในโปรแกรม",
    updateErrorSources: "ดาวน์โหลดไฟล์อัปเดตไม่ได้ โปรดตรวจสอบเครือข่ายแล้วลองอีกครั้ง",
    updateErrorInstall: "ติดตั้งการอัปเดตไม่ได้ โปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง โปรดลองอีกครั้ง หรือเปิดหน้า Release เพื่อดาวน์โหลดเอง",
    updateIntroInPlace: "ระบบจะดาวน์โหลดและตรวจสอบไฟล์อัปเดตก่อน จากนั้นแทนที่ไฟล์โปรแกรมและเปิดโปรแกรมใหม่",
    updateReadyIntroInPlace: "ดาวน์โหลดและตรวจสอบไฟล์อัปเดตแล้ว เมื่อคลิก “อัปเดตตอนนี้” ระบบจะแทนที่ไฟล์โปรแกรมและเปิดโปรแกรมใหม่",
    updatePreserved: "ข้อมูลผู้ใช้ บันทึกหมาก การตั้งค่า เอนจิน และโมเดลจะยังอยู่ครบ",
    updateInstalling: "กำลังติดตั้งการอัปเดต โปรแกรมจะปิดและเปิดใหม่…",
    updateErrorPermission: "เขียนไฟล์ลงในโฟลเดอร์ของโปรแกรมไม่ได้ จึงไม่ได้ติดตั้งการอัปเดต และโปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง โปรดย้ายโฟลเดอร์โปรแกรมไปยังตำแหน่งที่เขียนได้แล้วลองอีกครั้ง หรือเปิดหน้า Release เพื่อดาวน์โหลดเอง",
    updateErrorInUse: "มีไฟล์โปรแกรมที่กำลังถูกใช้งานอยู่ จึงแทนที่ไม่ได้ กู้คืนไฟล์เดิมแล้ว และโปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง โปรดปิดโปรแกรมที่ใช้ไฟล์เหล่านี้ (เช่น โปรแกรมนี้อีกหน้าต่างหนึ่ง หรือเครื่องมือซิงโครไนซ์บอร์ด) แล้วลองอีกครั้ง",
    updateErrorInterrupted: "การอัปเดตครั้งก่อนถูกขัดจังหวะ กู้คืนไฟล์เดิมแล้ว และโปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง โปรดตรวจสอบการอัปเดตอีกครั้ง",
    updateErrorUnrecovered: "การอัปเดตครั้งก่อนถูกขัดจังหวะ และยังกู้คืนไฟล์เดิมบางส่วนไม่ได้ โปรดปิดโปรแกรมอื่นที่ใช้โฟลเดอร์ของโปรแกรมนี้ แล้วเปิดโปรแกรมนี้ใหม่ หากยังมีปัญหา โปรดดาวน์โหลดโปรแกรมใหม่จากหน้า Release",
    updateIntroDeb: "ระบบจะดาวน์โหลดและตรวจสอบแพ็กเกจติดตั้งก่อน จากนั้นส่งให้ตัวจัดการแพ็กเกจของระบบ (apt) ติดตั้ง ระบบจะขอให้คุณอนุญาต และโปรแกรมจะเปิดใหม่เมื่อติดตั้งเสร็จ",
    updateReadyIntroDeb: "ดาวน์โหลดและตรวจสอบแพ็กเกจติดตั้งแล้ว เมื่อคลิก “อัปเดตตอนนี้” ระบบจะขอให้คุณอนุญาต แล้วตัวจัดการแพ็กเกจของระบบ (apt) จะติดตั้ง และโปรแกรมจะเปิดใหม่เมื่อเสร็จ",
    updateInstallingDeb: "กำลังรอการอนุญาตจากระบบและติดตั้งการอัปเดต โปรแกรมจะเปิดใหม่เมื่อเสร็จ…",
    updateErrorDismissed: "ยกเลิกการอนุญาตจากระบบแล้ว จึงไม่ได้ติดตั้งการอัปเดต และโปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง",
    updateErrorUnauthorized: "ไม่ได้รับอนุญาตจากระบบ จึงไม่ได้ติดตั้งการอัปเดต และโปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง ลองอีกครั้งได้ หรือติดตั้งแพ็กเกจที่ตรวจสอบแล้วในเทอร์มินัลด้วยคำสั่งด้านล่าง:",
    updateErrorPackageManager: "ตัวจัดการแพ็กเกจของระบบไม่ได้ติดตั้งการอัปเดตนี้ โปรแกรมปัจจุบันไม่มีการเปลี่ยนแปลง",
    updateErrorPackageUnfinished: "ติดตั้งการอัปเดตไม่ครบ แพ็กเกจของโปรแกรมนี้อยู่ในสถานะติดตั้งไม่เสร็จ โปรดรัน sudo dpkg --configure -a ในเทอร์มินัลเพื่อติดตั้งให้เสร็จ หากยังไม่หาย โปรดติดตั้งแพ็กเกจนี้ใหม่ (ดาวน์โหลดได้จากหน้า Release)",
  },
});

registerProduct({
  ZhCn: { updateSectionSub: "更新选项改动后立即保存，不受“{0}”“{1}”影响。" },
  ZhTw: { updateSectionSub: "更新選項變更後立即儲存，不受「{0}」「{1}」影響。" },
  EnUs: { updateSectionSub: "Update options are saved as soon as you change them; {0} and {1} do not affect them." },
  JaJp: { updateSectionSub: "更新の設定は変更した時点で保存され、「{0}」「{1}」の影響を受けません。" },
  KoKr: { updateSectionSub: "업데이트 옵션은 바꾸는 즉시 저장되며 '{0}', '{1}'과 관계없습니다." },
  ThTh: { updateSectionSub: "ตัวเลือกการอัปเดตจะบันทึกทันทีที่เปลี่ยน ไม่ขึ้นกับปุ่ม “{0}” และ “{1}”" },
});

function el<K extends keyof HTMLElementTagNameMap>(tag: K, cls = "", text = ""): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (cls) node.className = cls;
  if (text) node.textContent = text;
  return node;
}

function button(text: string, key: string, onClick: () => void): HTMLButtonElement {
  const b = el("button", "", text);
  b.type = "button";
  b.dataset.key = key;
  b.onclick = onClick;
  return b;
}

/** WindowsUpdateDialog.formatBytes. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  let value = bytes;
  const units = ["KB", "MB", "GB"];
  let unit = -1;
  do {
    value /= 1024;
    unit++;
  } while (value >= 1024 && unit < units.length - 1);
  return `${value.toFixed(1)} ${units[unit]}`;
}

const downloading = (d: UpdateDownloadView) => d.phase === "Downloading" || d.phase === "Paused";

/** UpdateText.userFacingError, with what the system itself said. */
function errorText(d: UpdateDownloadView): string | null {
  if (d.error === null) return null;
  const text = failureText(d.error);
  return d.detail ? `${text}\n${d.detail}` : text;
}

function failureText(error: UpdateError): string {
  switch (error) {
    case "Integrity": return t("WindowsUpdate.error.integrity");
    case "Sources": return product("updateErrorSources");
    case "Generic": return t("WindowsUpdate.error.generic");
    default: return handoverText(error);
  }
}

/** An installation that did not happen: the program is unchanged unless said otherwise. */
function handoverText(error: UpdateError): string {
  switch (error) {
    case "Permission": return product("updateErrorPermission");
    case "InUse": return product("updateErrorInUse");
    case "Interrupted": return product("updateErrorInterrupted");
    case "Unrecovered": return product("updateErrorUnrecovered");
    case "AuthorizationDismissed": return product("updateErrorDismissed");
    case "Unauthorized": return product("updateErrorUnauthorized");
    case "PackageManager": return product("updateErrorPackageManager");
    case "PackageUnfinished": return product("updateErrorPackageUnfinished");
    default: return product("updateErrorInstall");
  }
}

/** UpdateDownloadText.status. */
function downloadStatus(u: UpdateView): string {
  if (u.installing) return product(u.result?.offer?.form === "Deb" ? "updateInstallingDeb" : "updateInstalling");
  const d = u.download;
  let text = "";
  switch (d.phase) {
    case "Cancelled": text = t("WindowsUpdate.status.cancelled"); break;
    case "Failed": text = t("WindowsUpdate.status.failed"); break;
    case "Ready": text = t("WindowsUpdate.ready"); break;
    case "Paused": text = t("WindowsUpdate.status.paused"); break;
    case "Downloading":
      text = !d.started
        ? t("WindowsUpdate.status.preparing")
        : `${d.verifying ? t("WindowsUpdate.status.verifying") : t("WindowsUpdate.downloading")}\n${formatBytes(d.bytes)} / ${formatBytes(d.total)}`;
      break;
    case "None": break;
  }
  const error = errorText(d);
  return error ? `${text}\n${error}` : text;
}

/** UpdateCheckFeedback.warning: one 测试-channel candidate could not be verified. */
function warning(r: UpdateResultView): string {
  if (r.stable_candidate_failed === r.beta_candidate_failed) return "";
  return r.stable_candidate_failed ? t("WindowsUpdate.partialCheck.stable") : t("WindowsUpdate.partialCheck.beta");
}

/** UpdateCheckFeedback.message. */
function feedback(r: UpdateResultView): string {
  const beta = r.channel === "Beta";
  switch (r.outcome) {
    case "UnavailableBuild": return t("WindowsUpdate.devBuild");
    case "SigningNotConfigured": return product("updateSigningNotConfigured");
    case "UnsupportedForm": return product("updateUnsupportedForm");
    case "NoUpdate": return beta ? t("WindowsUpdate.noUpdate.beta") : t("WindowsUpdate.noUpdate.stable");
    case "NoPackage": return t("WindowsUpdate.noPackage");
    case "Failed":
      if (r.failure === "Fetch") return beta ? t("WindowsUpdate.fetchFailed.beta") : t("WindowsUpdate.fetchFailed.stable");
      return t("WindowsUpdate.checkFailed");
    case "Offer": return t("WindowsUpdate.available");
  }
}

/** A modeless window over the workbench with its title bar close button; the newest-clicked one
 *  is on top. */
function frame(title: string, cls: string, key: string, onClose: () => void): { box: HTMLElement; close: () => void } {
  const box = el("section", `modal-box update-window ${cls}`);
  box.dataset.key = key;
  box.dataset.freeWindow = "";
  box.tabIndex = -1;
  box.setAttribute("role", "dialog");
  box.setAttribute("aria-modal", "false");
  box.setAttribute("aria-label", title);
  const head = el("div", "modal-title update-title", title);
  const x = button("", `${key}.close`, () => close());
  x.className = "modal-close";
  x.append(icon("close"));
  x.setAttribute("aria-label", product("close"));
  head.append(x);
  box.append(head);
  let closed = false;
  const close = () => {
    if (closed) return;
    closed = true;
    box.remove();
    onClose();
  };
  box.addEventListener("keydown", (e) => e.stopPropagation());
  box.addEventListener("pointerdown", () => {
    if (box.nextElementSibling) document.body.append(box);
  });
  document.body.append(box);
  return { box, close };
}

// ------------------------------------------------------------------ 综合设置 > 关于 > 检查更新

/** The rows of the open 综合设置; gone once that dialog closes. */
let section: { root: HTMLElement; show: (u: UpdateView) => void } | null = null;

/** Saves at once; a save that fails shows its error and the rows return to the saved settings
 *  (the control already shows the unsaved choice, and no new view follows a failure). */
async function saveSettings(change: Partial<UpdateSettings>): Promise<void> {
  const settings = state.view?.update.settings;
  if (!settings) return;
  const result = await run({ type: "Update", op: { type: "Settings", settings: { ...settings, ...change } } });
  if (result === null && state.view) section?.show(state.view.update);
}

function radio(name: string, text: string, key: string, onPick: () => void): { label: HTMLLabelElement; input: HTMLInputElement } {
  const input = el("input");
  input.type = "radio";
  input.name = name;
  input.dataset.key = `${key}.input`;
  input.onchange = () => { if (input.checked) onPick(); };
  const label = el("label", "update-choice");
  label.dataset.key = key;
  label.append(input, text);
  return { label, input };
}

/** CheckUpdateDialog's options, its result line and its buttons, then the offer the check found
 *  with WindowsUpdateDialog's account of it and its download, built into 综合设置's 关于 page. */
export function buildUpdateSection(host: HTMLElement): void {
  const heading = el("h3", "", t("WindowsUpdate.page.title"));
  const sub = el("p", "", product("updateSectionSub", t("SetKataRules.btnApply"), t("SetKataRules.btnCancel")));
  host.append(heading, sub);

  const stable = radio("update-channel", t("WindowsUpdate.channel.stable"), "update.channel.stable", () => saveSettings({ channel: "Stable" }));
  const beta = radio("update-channel", t("WindowsUpdate.channel.beta"), "update.channel.beta", () => saveSettings({ channel: "Beta" }));
  const official = radio("update-source", t("WindowsUpdate.source.official"), "update.source.official", () => saveSettings({ source: "Official" }));
  const github = radio("update-source", t("WindowsUpdate.source.github"), "update.source.github", () => saveSettings({ source: "Github" }));
  for (const [title, ...choices] of [[t("WindowsUpdate.page.channel"), stable, beta], [t("WindowsUpdate.page.source"), official, github]] as const) {
    const row = el("div", "engine-field");
    const unit = el("span", "engine-unit");
    unit.append(...choices.map((choice) => choice.label));
    row.append(el("span", "", title), unit);
    host.append(row);
  }

  const check = (key: string, onChange: (on: boolean) => void) => {
    const input = el("input");
    input.type = "checkbox";
    input.dataset.key = `${key}.input`;
    input.onchange = () => onChange(input.checked);
    const text = el("span");
    const label = el("label", "update-check");
    label.dataset.key = key;
    label.append(input, text);
    return { label, input, text };
  };
  const autoCheck = check("update.autoCheck", (on) => saveSettings(on ? { autoCheck: true } : { autoCheck: false, autoDownload: false }));
  const autoDownload = check("update.autoDownload", (on) => saveSettings({ autoDownload: on }));
  const result = el("div", "update-result");
  result.dataset.key = "update.result";
  result.setAttribute("aria-live", "polite");
  const details = el("div", "update-details");
  host.append(autoCheck.label, autoDownload.label, result, details);

  let offerId: number | null = null;
  let offerShow: ((v: UpdateView) => void) | null = null;
  const checkNow = button(t("WindowsUpdate.btnCheck"), "update.check", () => void run({ type: "Update", op: { type: "Check" } }));
  const updateNow = button(t("WindowsUpdate.btnUpdate"), "update.updateNow", () => {
    if (offerId !== null) void run({ type: "Update", op: { type: "Update", offer: offerId } });
  });
  const retry = button(t("WindowsUpdate.btnResume"), "update.retry", () => void run({ type: "Update", op: { type: "Retry" } }));
  const pause = button("", "update.offerPause", () => void run({ type: "Update", op: { type: "TogglePause" } }));
  const cancel = button(t("WindowsUpdate.btnCancel"), "update.offerCancel", () => void run({ type: "Update", op: { type: "Cancel" } }));
  const release = button(t("WindowsUpdate.btnRelease"), "update.release", () => void openRelease());
  const buttons = el("div", "engine-actions update-actions");
  buttons.dataset.settingTarget = "menu.checkUpdate";
  buttons.append(checkNow, updateNow, retry, pause, cancel, release);
  host.append(buttons);

  const show = (u: UpdateView) => {
    const s = u.settings;
    stable.input.checked = s.channel === "Stable";
    beta.input.checked = s.channel === "Beta";
    // 测试 always reads GitHub (refreshSourceState).
    const isStable = s.channel === "Stable";
    official.input.disabled = github.input.disabled = !isStable;
    official.label.classList.toggle("disabled", !isStable);
    github.label.classList.toggle("disabled", !isStable);
    official.input.checked = isStable && s.source === "Official";
    github.input.checked = !isStable || s.source === "Github";
    autoCheck.input.checked = s.autoCheck;
    autoDownload.input.checked = s.autoDownload;
    autoDownload.input.disabled = !s.autoCheck;
    autoDownload.label.classList.toggle("disabled", !s.autoCheck);
    autoCheck.text.textContent = s.autoCheck && !u.auto_check_active ? t("WindowsUpdate.autoCheckPending") : t("WindowsUpdate.autoCheck");
    autoDownload.text.textContent = s.autoDownload && !u.auto_download_active ? t("WindowsUpdate.autoDownloadPending") : t("WindowsUpdate.autoDownload");

    const d = u.download;
    const offer = u.result?.offer ?? null;
    if ((offer?.id ?? null) !== offerId) {
      offerId = offer?.id ?? null;
      offerShow = null;
      details.replaceChildren();
      if (offer) {
        const parts = offerDetails(offer);
        details.append(parts.heading, parts.summary, parts.bar);
        offerShow = parts.show;
      }
    }
    details.hidden = !offer;
    offerShow?.(u);
    checkNow.disabled = u.busy;
    updateNow.hidden = !offer || downloading(d) || d.phase === "Cancelled" || d.phase === "Failed";
    updateNow.disabled = u.busy;
    retry.hidden = !offer || (d.phase !== "Cancelled" && d.phase !== "Failed");
    retry.disabled = u.busy;
    pause.hidden = cancel.hidden = !downloading(d);
    pause.textContent = d.phase === "Paused" ? t("WindowsUpdate.btnResume") : t("WindowsUpdate.btnPause");
    release.hidden = !offer;
    // The status line is a 提示条 when it reports a failure or a partial check.
    let tone: "error" | "warning" | null = null;
    if (d.phase !== "None" || u.installing) {
      result.textContent = downloadStatus(u);
      if (d.phase === "Failed" || d.error !== null) tone = "error";
    } else if (u.checking) {
      result.textContent = t("WindowsUpdate.checking");
    } else if (!u.result) {
      result.textContent = "";
    } else {
      const extra = warning(u.result);
      result.textContent = extra ? `${feedback(u.result)}\n${extra}` : feedback(u.result);
      tone = u.result.outcome === "Failed" ? "error" : extra ? "warning" : null;
    }
    result.hidden = !result.textContent;
    result.classList.toggle("update-error", tone === "error");
    result.classList.toggle("update-warn", tone === "warning");
  };
  section = { root: host, show };
  if (state.view) show(state.view.update);
}

registerSettingTargets("about", ["menu.checkUpdate"]);

// ------------------------------------------------------------------ 发现新版本

let offerWindow: { id: number; close: () => void; show: (u: UpdateView) => void } | null = null;

function intro(offer: UpdateOfferView, ready: boolean): string {
  // The NSIS installer and the portable zip's helper close the program, replace its files and
  // reopen it (Java's own wording); a deb is installed by the system package manager after the
  // system's authorization; the app bundle and AppImage are replaced in place before the program
  // restarts.
  if (offer.form === "Nsis" || offer.form === "PortableZip") {
    return ready ? t("WindowsUpdate.summary.readyIntro") : t("WindowsUpdate.summary.intro");
  }
  if (offer.form === "Deb") return product(ready ? "updateReadyIntroDeb" : "updateIntroDeb");
  return ready ? product("updateReadyIntroInPlace") : product("updateIntroInPlace");
}

/** WindowsUpdateDialog.openRelease: the failure names the system's reason. */
async function openRelease(): Promise<void> {
  try {
    await invoke("open_update_release");
  } catch (e) {
    const reason = e && typeof e === "object" && "message" in e ? String(e.message) : String(e);
    await message(t("Message.title"), `${t("WindowsUpdate.status.openReleaseFailed")}: ${reason}`, "error");
  }
}

/** WindowsUpdateDialog's account of one offer: its version, what 立即更新 does with it and the
 *  download's progress; `show` fills them from a view of this offer. */
function offerDetails(offer: UpdateOfferView): { heading: HTMLElement; summary: HTMLElement; bar: HTMLElement; show: (v: UpdateView) => void } {
  const heading = el("div", "update-offer-heading", `LizzieYzy Tauri ${offer.version}`);
  const summary = el("div", "update-summary");
  summary.dataset.key = "update.summary";
  const bar = el("div", "update-progress");
  bar.dataset.key = "update.progress";
  bar.setAttribute("role", "progressbar");
  bar.setAttribute("aria-label", t("WindowsUpdate.accessibility.progress"));
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", "100");
  const meter = el("progress");
  meter.max = 1000;
  meter.setAttribute("aria-hidden", "true");
  const percent = el("span", "update-progress-text");
  bar.append(meter, percent);
  const show = (v: UpdateView) => {
    const d = v.download;
    summary.textContent = `${intro(offer, d.phase === "Ready")}\n\n${t("WindowsUpdate.summary.package")}: ${offer.asset_name}\n${t("WindowsUpdate.summary.totalDownload")}: ${formatBytes(offer.size)}\n\n${product("updatePreserved")}`;
    bar.hidden = d.phase === "None";
    const value = d.total > 0 ? Math.min(1000, Math.floor((d.bytes * 1000) / d.total)) : 0;
    meter.value = value;
    percent.textContent = `${Math.floor(value / 10)}%`;
    bar.setAttribute("aria-valuenow", String(Math.floor(value / 10)));
  };
  return { heading, summary, bar, show };
}

/** Shows the current offer (`openOffer`): `focus` is false for an automatic check's reminder
 *  (setAutoRequestFocus(false)). Viewing grants nothing; only 立即更新 prepares and installs. */
function openOffer(focus: boolean): void {
  const u = state.view?.update;
  const offer = u?.result?.offer;
  if (!u || !offer) return;
  // 综合设置's 关于 page, when in view, shows the offer in place of this window.
  if (section?.root.isConnected && section.root.getClientRects().length > 0) return;
  if (offerWindow?.id === offer.id) {
    if (focus) offerWindow.show(u);
    return;
  }
  offerWindow?.close();
  const { box, close } = frame(t("WindowsUpdate.title"), "update-offer", "update.offer", () => { offerWindow = null; });
  const body = el("div", "update-body");
  const details = offerDetails(offer);
  const current = el("div", "", `${t("WindowsUpdate.currentVersion")}: ${offer.current_version}`);
  const extra = el("div", "update-warning");
  const status = el("div", "update-status");
  status.dataset.key = "update.status";
  status.setAttribute("aria-label", t("WindowsUpdate.accessibility.status"));
  status.setAttribute("aria-live", "polite");
  body.append(details.heading, current, extra, details.summary, details.bar, status);

  const footer = el("div", "modal-buttons update-buttons");
  const release = button(t("WindowsUpdate.btnRelease"), "update.release", () => void openRelease());
  const closeButton = button(t("WindowsUpdate.btnLater"), "update.offerClose", () => close());
  const cancel = button(t("WindowsUpdate.btnCancel"), "update.offerCancel", () => void run({ type: "Update", op: { type: "Cancel" } }));
  const pause = button("", "update.offerPause", () => void run({ type: "Update", op: { type: "TogglePause" } }));
  const update = button(t("WindowsUpdate.btnUpdate"), "update.updateNow", () => void run({ type: "Update", op: { type: "Update", offer: offer.id } }));
  update.classList.add("primary");
  footer.append(release, closeButton, cancel, pause, update);
  box.append(body, footer);

  const show = (v: UpdateView) => {
    const shown = v.result?.offer;
    // A newer check replaced this offer: its window goes away (`!current → dispose`).
    if (!shown || shown.id !== offer.id) {
      close();
      return;
    }
    const d = v.download;
    extra.textContent = v.result ? warning(v.result) : "";
    extra.hidden = !extra.textContent;
    details.show(v);
    update.disabled = v.busy;
    cancel.disabled = pause.disabled = !downloading(d);
    pause.textContent = d.phase === "Paused" ? t("WindowsUpdate.btnResume") : t("WindowsUpdate.btnPause");
    const text = downloadStatus(v);
    if (text) status.textContent = text;
    status.classList.toggle("update-error", d.phase === "Failed" || d.error !== null);
  };
  offerWindow = { id: offer.id, close, show };
  show(u);
  if (focus) box.focus();
}

let reminded = 0;
let handoverReported = false;
registerViewHandler((view) => {
  const u = view.update;
  if (section && !section.root.isConnected) section = null;
  section?.show(u);
  offerWindow?.show(u);
  // The previous run closed to install an update, which did not happen.
  if (u.handover_failed && !handoverReported) {
    handoverReported = true;
    void message(t("Message.title"), handoverText(u.handover_failed), "error");
  }
  // An offer the core asks to show: a manual check's, or once an automatic one's.
  if (u.remind > reminded) {
    reminded = u.remind;
    openOffer(u.origin !== "Automatic");
  }
});

Object.assign(COMMANDS, {
  // 帮助 > 检查更新 (WindowsUpdateController.openCheckUpdatePage): 综合设置 at the 检查更新 buttons.
  "menu.checkUpdate": { run: () => void openSettingsAt("menu.checkUpdate") },
});
