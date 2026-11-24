// 综合设置 > 主题外观 (ConfigDialog2.rebuildThemeTabLikeDesign, ConfigDialog2.java:3728-3965, with
// the controls of loadThemeTab 4369-5232): the theme's values are a draft read from the chosen
// theme (readThemeValues / readDefaultTheme 6120-6372) and sent with 保存设置 (writeThemeValues /
// writeDefaultTheme 6191-6471, saveConfig 6863-6867); 取消 discards them. 添加 / 删除 manage the
// theme list at once like Java, without touching the committed appearance. 棋盘预览 paints the
// draft on its own canvas (pnlBoardPreview 4980-5197). 选点评估关注外框 (3923-3943) edits the
// 选点重点研究 settings, not the theme, and is saved on its own. Beyond Java: the theme's 界面颜色,
// and 棋盘 / 棋子 chosen from thumbnails of the pictures the program ships, for every theme
// including 默认 (Java fixes the default theme's images, 6313-6324).
import { run, state } from "../app";
import { appearanceCatalog, appearanceImages, chooseAppearanceImage } from "../contract/client";
import type { AppearanceCatalog, AppearanceSettings, ShippedBoard, ShippedStones, ThemeSettings } from "../contract/generated";
import { ask, confirm, message } from "../dialogs";
import { groupTitle, row as groupRow } from "../engine/controls";
import { SETTINGS_PAGES, type SettingsDialog } from "../engine/settings";
import { registerSettingTargets } from "../engine/settingsTargets";
import { isChinese, product, registerProduct, t } from "../i18n";
import { cssColor, DEFAULT_BLUNDER_COLORS, DEFAULT_BLUNDER_THRESHOLDS, DEFAULT_THEME, JAVA_CLASSIC_THEME, JAVA_SOFT_THEME, LIZZIE_DEFAULT_FONT, opaque, SYSTEM_DEFAULT_FONT, themeSkin } from "./model";
import { loadVariants, type LoadedImages } from "./state";
import { softStoneShadow, stonePicture } from "./stones";
import { icon } from "../design/icons";
import type { InterfaceColors } from "../design/palette";
import { stepper } from "../design/stepper";
import { programStage, stageReady, stageWallpaper } from "../design/stage";
import { skinColors } from "../design/theme";

/** ConfigSettingTargets THEME rows this page has (ConfigSettingTargets.java:61-86), in row order,
 *  with 设置 > 界面字体大小's row (its catalog id), which this product keeps here instead of Java's
 *  SetFrameFontSize dialog. */
const TARGETS = [
  "config.theme.current",
  "config.theme.winrate-width",
  "config.theme.blunder-width",
  "config.theme.shadow",
  "config.theme.info-font",
  "config.theme.ui-font",
  "menu.frameFontSize",
  "config.theme.winrate-font",
  "config.theme.background-image",
  "config.theme.board-image",
  "config.theme.black-stone-image",
  "config.theme.white-stone-image",
  "config.theme.background-blur",
  "config.theme.winrate-color",
  "config.theme.missing-color",
  "config.theme.blunder-color",
  "config.theme.score-color",
  "config.theme.comment-background",
  "config.theme.comment-text",
  "config.theme.best-move-color",
  "config.theme.comment-font-size",
  "config.theme.stone-indicator",
  "config.theme.comment-node-color",
  "config.tracking.outline",
  "config.tracking.outline-opacity",
  "config.theme.blunder-rules",
  "config.theme.score-blunders",
] as const;
type Target = (typeof TARGETS)[number];

registerProduct({
  ZhCn: { settingsFrameFont: "界面字体大小", settingsFrameFontSub: "菜单、面板和对话框的文字大小，可设 12–20，重新启动后生效" },
  ZhTw: { settingsFrameFont: "介面字型大小", settingsFrameFontSub: "選單、面板和對話框的文字大小，可設 12–20，重新開啟後生效" },
  EnUs: { settingsFrameFont: "Interface font size", settingsFrameFontSub: "Text size of menus, panels and dialogs, 12–20; takes effect after a restart" },
  JaJp: { settingsFrameFont: "画面の文字サイズ", settingsFrameFontSub: "メニュー、パネル、ダイアログの文字サイズ（12〜20）。再起動後に反映されます" },
  KoKr: { settingsFrameFont: "화면 글꼴 크기", settingsFrameFontSub: "메뉴, 패널, 대화 상자의 글자 크기(12–20). 다시 시작하면 적용됩니다" },
  ThTh: { settingsFrameFont: "ขนาดตัวอักษรของหน้าจอ", settingsFrameFontSub: "ขนาดตัวอักษรของเมนู แผง และกล่องโต้ตอบ (12–20) มีผลหลังเปิดโปรแกรมใหม่" },
});

/** Accessible names of fields that share a row: a colour, its alpha, the shadow's switch. */
registerProduct({
  ZhCn: { themeFieldColor: "{0}：颜色", themeFieldOpacity: "{0}：不透明度", themeFieldShowShadow: "显示棋子阴影" },
  ZhTw: { themeFieldColor: "{0}：顏色", themeFieldOpacity: "{0}：不透明度", themeFieldShowShadow: "顯示棋子陰影" },
  EnUs: { themeFieldColor: "{0}: color", themeFieldOpacity: "{0}: opacity", themeFieldShowShadow: "Show stone shadows" },
  JaJp: { themeFieldColor: "{0}：色", themeFieldOpacity: "{0}：不透明度", themeFieldShowShadow: "石の影を表示" },
  KoKr: { themeFieldColor: "{0}: 색상", themeFieldOpacity: "{0}: 불투명도", themeFieldShowShadow: "돌 그림자 표시" },
  ThTh: { themeFieldColor: "{0}: สี", themeFieldOpacity: "{0}: ความทึบ", themeFieldShowShadow: "แสดงเงาหิน" },
});

/** The visible names of 胜率曲线宽度's two fields: the 胜率图's win rate line and its score line. */
registerProduct({
  ZhCn: { themeWidthWinrate: "胜率", themeWidthScore: "目数" },
  ZhTw: { themeWidthWinrate: "勝率", themeWidthScore: "目數" },
  EnUs: { themeWidthWinrate: "Winrate", themeWidthScore: "Score" },
  JaJp: { themeWidthWinrate: "勝率", themeWidthScore: "目差" },
  KoKr: { themeWidthWinrate: "승률", themeWidthScore: "스코어" },
  ThTh: { themeWidthWinrate: "อัตราชนะ", themeWidthScore: "คะแนน" },
});

/** 棋子阴影's row: the size is a percentage of the stone radius (paintPreview, BoardRenderer), so
 *  the row says so and its field carries the unit. */
registerProduct({
  ZhCn: { themeShadowSub: "开启后按棋子半径的百分比设置阴影大小" },
  ZhTw: { themeShadowSub: "開啟後按棋子半徑的百分比設定陰影大小" },
  EnUs: { themeShadowSub: "Turn on the stone shadow; its size is a percentage of the stone radius" },
  JaJp: { themeShadowSub: "石の影を表示し、大きさを石の半径に対する割合で設定します" },
  KoKr: { themeShadowSub: "돌 그림자를 켜고 크기를 돌 반지름에 대한 비율로 설정합니다" },
  ThTh: { themeShadowSub: "เปิดเงาหิน และกำหนดขนาดเป็นเปอร์เซ็นต์ของรัศมีหิน" },
});

/** 新增 and 删除 reopen the settings (Java's setVisible(false) + openConfigDialog2), dropping the
 *  other pages' unsaved edits: the row and the delete confirmation say so before it happens. */
registerProduct({
  ZhCn: { themeManageSub: "{0}。新增或删除会立即执行，其他设置页未保存的修改会被放弃", themeDeleteDiscards: "删除会立即执行，其他设置页未保存的修改会被放弃。" },
  ZhTw: { themeManageSub: "{0}。新增或刪除會立即執行，其他設定頁未儲存的修改會被捨棄", themeDeleteDiscards: "刪除會立即執行，其他設定頁未儲存的修改會被捨棄。" },
  EnUs: { themeManageSub: "{0}. New and Delete take effect at once and discard unsaved changes on the other pages", themeDeleteDiscards: "Deleting takes effect at once and discards unsaved changes on the other settings pages." },
  JaJp: { themeManageSub: "{0}。新規と削除はすぐに実行され、他のページの未保存の変更は破棄されます", themeDeleteDiscards: "削除はすぐに実行され、他の設定ページの未保存の変更は破棄されます。" },
  KoKr: { themeManageSub: "{0}. 신규생성과 삭제는 바로 실행되며 다른 페이지의 저장하지 않은 변경은 버려집니다", themeDeleteDiscards: "삭제는 바로 실행되며 다른 설정 페이지의 저장하지 않은 변경은 버려집니다." },
  ThTh: { themeManageSub: "{0} การสร้างใหม่และการลบมีผลทันที และจะทิ้งการเปลี่ยนแปลงที่ยังไม่บันทึกในหน้าอื่น", themeDeleteDiscards: "การลบมีผลทันที และจะทิ้งการเปลี่ยนแปลงที่ยังไม่บันทึกในหน้าตั้งค่าอื่น" },
});

/** The names of the bundled themes the list translates: the Java ones, stored as JAVA_CLASSIC_THEME
 *  and JAVA_SOFT_THEME, and this product's textured ones (tools/textured_themes.py). */
const BUNDLED_THEME_NAMES = {
  ZhCn: { themeJavaClassic: "Java 经典", themeJavaSoft: "Java 柔和", themeKaya: "日式榧木", themeWalnut: "胡桃木", themeYunzi: "云子" },
  ZhTw: { themeJavaClassic: "Java 經典", themeJavaSoft: "Java 柔和", themeKaya: "日式榧木", themeWalnut: "胡桃木", themeYunzi: "雲子" },
  EnUs: { themeJavaClassic: "Java Classic", themeJavaSoft: "Java Soft", themeKaya: "Kaya", themeWalnut: "Walnut", themeYunzi: "Yunzi" },
  JaJp: { themeJavaClassic: "Java クラシック", themeJavaSoft: "Java ソフト", themeKaya: "榧", themeWalnut: "ウォールナット", themeYunzi: "雲子" },
  KoKr: { themeJavaClassic: "Java 클래식", themeJavaSoft: "Java 소프트", themeKaya: "비자나무", themeWalnut: "호두나무", themeYunzi: "운자" },
  ThTh: { themeJavaClassic: "Java คลาสสิก", themeJavaSoft: "Java ซอฟต์", themeKaya: "ไม้คายะ", themeWalnut: "ไม้วอลนัต", themeYunzi: "หมากหยุนจื่อ" },
};
registerProduct(BUNDLED_THEME_NAMES);
const BUNDLED_THEME_KEYS: Record<string, string> = {
  [JAVA_CLASSIC_THEME]: "themeJavaClassic",
  [JAVA_SOFT_THEME]: "themeJavaSoft",
  kaya: "themeKaya",
  walnut: "themeWalnut",
  yunzi: "themeYunzi",
};

/** 立体棋子 (AppearanceSettings.enhancedStones): the row in 图片与材质. */
registerProduct({
  ZhCn: { themeDepth: "立体棋子", themeDepthSub: "对所有主题生效：阴影改为贴盘的接触阴影加柔和投影，主题带多张棋子图时各点纹路不同。关闭后按 Java 版画法" },
  ZhTw: { themeDepth: "立體棋子", themeDepthSub: "對所有主題生效：陰影改為貼盤的接觸陰影加柔和投影，主題帶多張棋子圖時各點紋路不同。關閉後按 Java 版畫法" },
  EnUs: { themeDepth: "3D stones", themeDepthSub: "For every theme: stones get a contact shadow and a soft cast shadow, and themes with several stone images vary them from point to point. Off draws stones as the Java version does" },
  JaJp: { themeDepth: "立体的な碁石", themeDepthSub: "すべてのテーマに適用：影を盤に接する影と柔らかな落ち影にし、碁石の画像が複数あるテーマでは点ごとに模様が変わります。オフにすると Java 版と同じ描き方です" },
  KoKr: { themeDepth: "입체 바둑돌", themeDepthSub: "모든 테마에 적용: 그림자를 판에 닿는 그림자와 부드러운 그림자로 그리고, 바둑돌 이미지가 여러 장인 테마는 자리마다 무늬가 달라집니다. 끄면 Java 판과 같게 그립니다" },
  ThTh: { themeDepth: "หมากสามมิติ", themeDepthSub: "ใช้กับทุกธีม: เงาเป็นเงาสัมผัสกระดานและเงาตกกระทบแบบนุ่ม และธีมที่มีภาพหมากหลายภาพจะแสดงลายต่างกันในแต่ละจุด ปิดแล้วจะวาดแบบเวอร์ชัน Java" },
});

/** 界面颜色: the card, its four rows and each row's reset. */
registerProduct({
  ZhCn: {
    themeUiTitle: "界面颜色",
    themeUiSub: "条栏、按钮、强调色和文字的颜色，随主题保存。未修改的颜色沿用主题配色，工作台面板和棋盘不受影响。",
    themeUiChrome: "条栏底色",
    themeUiChromeSub: "标题栏、菜单栏、工具栏、底栏和各窗口顶部的按钮条",
    themeUiControl: "按钮底色",
    themeUiControlSub: "按钮、输入框、菜单和条栏里的按钮组；对话框底色取在条栏和按钮之间",
    themeUiAccent: "强调色",
    themeUiAccentSub: "主按钮、选中项、开关和焦点框",
    themeUiText: "文字颜色",
    themeUiTextSub: "界面文字和图标；分隔线和悬停效果也随它变化",
    themeUiReset: "恢复默认",
    themeUiResetFor: "{0}：恢复默认",
  },
  ZhTw: {
    themeUiTitle: "介面顏色",
    themeUiSub: "條欄、按鈕、強調色和文字的顏色，隨主題儲存。未修改的顏色沿用主題配色，工作台面板和棋盤不受影響。",
    themeUiChrome: "條欄底色",
    themeUiChromeSub: "標題列、選單列、工具列、底欄和各視窗頂部的按鈕列",
    themeUiControl: "按鈕底色",
    themeUiControlSub: "按鈕、輸入框、選單和條欄裡的按鈕組；對話框底色取在條欄和按鈕之間",
    themeUiAccent: "強調色",
    themeUiAccentSub: "主按鈕、選取項目、開關和焦點框",
    themeUiText: "文字顏色",
    themeUiTextSub: "介面文字和圖示；分隔線和懸停效果也隨它變化",
    themeUiReset: "恢復預設",
    themeUiResetFor: "{0}：恢復預設",
  },
  EnUs: {
    themeUiTitle: "Interface colors",
    themeUiSub: "Colors of the bars, buttons, accent and text, saved with the theme. Colors you leave unchanged keep the theme's own; the workbench panels and the board stay as they are.",
    themeUiChrome: "Bar background",
    themeUiChromeSub: "Title bar, menu bar, toolbar, bottom bar and the button strip at the top of each window",
    themeUiControl: "Button background",
    themeUiControlSub: "Buttons, fields, menus and the button groups in the bars; dialogs take a shade between the bars and the buttons",
    themeUiAccent: "Accent",
    themeUiAccentSub: "Main buttons, selected items, switches and the focus ring",
    themeUiText: "Text color",
    themeUiTextSub: "Interface text and icons; separators and hover shading follow it",
    themeUiReset: "Reset",
    themeUiResetFor: "{0}: reset",
  },
  JaJp: {
    themeUiTitle: "画面の色",
    themeUiSub: "バー、ボタン、強調色、文字の色です。テーマと一緒に保存されます。変更していない色はテーマの配色のままで、作業画面のパネルと碁盤は変わりません。",
    themeUiChrome: "バーの背景色",
    themeUiChromeSub: "タイトルバー、メニューバー、ツールバー、下部バー、各ウィンドウ上部のボタン列",
    themeUiControl: "ボタンの背景色",
    themeUiControlSub: "ボタン、入力欄、メニュー、バー内のボタン群。ダイアログの背景はバーとボタンの中間の色になります",
    themeUiAccent: "強調色",
    themeUiAccentSub: "メインボタン、選択項目、スイッチ、フォーカス枠",
    themeUiText: "文字色",
    themeUiTextSub: "画面の文字とアイコン。区切り線やポインターを重ねたときの色もこれに合わせて変わります",
    themeUiReset: "既定に戻す",
    themeUiResetFor: "{0}：既定に戻す",
  },
  KoKr: {
    themeUiTitle: "화면 색상",
    themeUiSub: "바, 버튼, 강조색, 글자의 색상이며 테마와 함께 저장됩니다. 바꾸지 않은 색상은 테마 색을 그대로 쓰고, 작업 화면의 패널과 바둑판은 바뀌지 않습니다.",
    themeUiChrome: "바 배경색",
    themeUiChromeSub: "제목 표시줄, 메뉴 표시줄, 도구 모음, 하단 바와 각 창 위쪽의 버튼 줄",
    themeUiControl: "버튼 배경색",
    themeUiControlSub: "버튼, 입력란, 메뉴와 바 안의 버튼 묶음. 대화 상자 배경은 바와 버튼의 중간 색이 됩니다",
    themeUiAccent: "강조색",
    themeUiAccentSub: "기본 버튼, 선택 항목, 스위치, 포커스 테두리",
    themeUiText: "글자색",
    themeUiTextSub: "화면의 글자와 아이콘. 구분선과 마우스를 올렸을 때의 색도 이에 맞춰 바뀝니다",
    themeUiReset: "기본값으로",
    themeUiResetFor: "{0}: 기본값으로",
  },
  ThTh: {
    themeUiTitle: "สีของหน้าจอ",
    themeUiSub: "สีของแถบ ปุ่ม สีเน้น และตัวอักษร บันทึกไปกับธีม สีที่ไม่ได้เปลี่ยนจะใช้สีของธีม แผงในพื้นที่ทำงานและกระดานจะไม่เปลี่ยน",
    themeUiChrome: "สีพื้นแถบ",
    themeUiChromeSub: "แถบชื่อเรื่อง แถบเมนู แถบเครื่องมือ แถบล่าง และแถบปุ่มด้านบนของแต่ละหน้าต่าง",
    themeUiControl: "สีพื้นปุ่ม",
    themeUiControlSub: "ปุ่ม ช่องป้อนข้อมูล เมนู และกลุ่มปุ่มในแถบ พื้นกล่องโต้ตอบจะเป็นสีระหว่างแถบกับปุ่ม",
    themeUiAccent: "สีเน้น",
    themeUiAccentSub: "ปุ่มหลัก รายการที่เลือก สวิตช์ และกรอบโฟกัส",
    themeUiText: "สีตัวอักษร",
    themeUiTextSub: "ตัวอักษรและไอคอน เส้นแบ่งและสีเมื่อชี้เมาส์จะเปลี่ยนตามด้วย",
    themeUiReset: "คืนค่าเริ่มต้น",
    themeUiResetFor: "{0}: คืนค่าเริ่มต้น",
  },
});

/** 棋盘 and 棋子 chosen from thumbnails: the rows' descriptions and the two tiles beside the
 *  pictures the program ships. */
registerProduct({
  ZhCn: {
    themeBoardPickSub: "选一种自带棋盘或纯色，或用其他图片；路径也可以直接填写",
    themeStones: "棋子",
    themeStonesSub: "选一套自带棋子或纯色；用自己的图片时在下面两行分别指定",
    themeStonePathSub: "图片路径，可直接填写，或点“...”选择文件",
    themePickPure: "纯色",
    themePickOther: "其他图片…",
  },
  ZhTw: {
    themeBoardPickSub: "選一種內建棋盤或純色，或用其他圖片；路徑也可以直接填寫",
    themeStones: "棋子",
    themeStonesSub: "選一套內建棋子或純色；用自己的圖片時在下面兩行分別指定",
    themeStonePathSub: "圖片路徑，可直接填寫，或點「...」選擇檔案",
    themePickPure: "純色",
    themePickOther: "其他圖片…",
  },
  EnUs: {
    themeBoardPickSub: "Pick a built-in board or a plain color, or use another image; you can also type its path",
    themeStones: "Stones",
    themeStonesSub: "Pick a built-in set or plain stones; for your own images, set them in the two rows below",
    themeStonePathSub: "Image path; type it, or click \"...\" to choose a file",
    themePickPure: "Plain",
    themePickOther: "Other image…",
  },
  JaJp: {
    themeBoardPickSub: "内蔵の碁盤か単色を選ぶか、ほかの画像を使います。パスを直接入力することもできます",
    themeStones: "碁石",
    themeStonesSub: "内蔵の碁石か単色を選びます。自分の画像を使うときは下の 2 行で指定します",
    themeStonePathSub: "画像のパス。直接入力するか、「...」でファイルを選びます",
    themePickPure: "単色",
    themePickOther: "ほかの画像…",
  },
  KoKr: {
    themeBoardPickSub: "내장 바둑판이나 단색을 고르거나 다른 이미지를 씁니다. 경로를 직접 입력할 수도 있습니다",
    themeStones: "바둑돌",
    themeStonesSub: "내장 바둑돌이나 단색을 고릅니다. 직접 고른 이미지는 아래 두 줄에서 지정합니다",
    themeStonePathSub: "이미지 경로. 직접 입력하거나 \"...\"을 눌러 파일을 고릅니다",
    themePickPure: "단색",
    themePickOther: "다른 이미지…",
  },
  ThTh: {
    themeBoardPickSub: "เลือกกระดานในตัวหรือสีพื้น หรือใช้รูปภาพอื่น จะพิมพ์ที่อยู่ไฟล์เองก็ได้",
    themeStones: "หมาก",
    themeStonesSub: "เลือกชุดหมากในตัวหรือสีพื้น ถ้าใช้รูปภาพของคุณเอง ให้กำหนดในสองแถวด้านล่าง",
    themeStonePathSub: "ที่อยู่ไฟล์รูปภาพ พิมพ์เอง หรือคลิก \"...\" เพื่อเลือกไฟล์",
    themePickPure: "สีพื้น",
    themePickOther: "รูปภาพอื่น…",
  },
});

/** A theme as the list names it: 默认 and the bundled themes above translated, every other theme as
 *  stored. */
function themeName(name: string): string {
  if (name === DEFAULT_THEME) return t("LizzieConfig.title.defaultTheme");
  const key = BUNDLED_THEME_KEYS[name];
  return key ? product(key) : name;
}

/** The program's own default images: what the core answers when a theme names no image or one
 *  that does not read (Theme.java:146-204 fallbacks). */
const DEFAULT_PATHS = { background: "/assets/background.jpg", board: "/assets/board.png", black: "/assets/black0.png", white: "/assets/white0.png" };

const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string): HTMLElementTagNameMap[K] => {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
};

/** createDesignSettingsCard: a titled group. Texts arrive translated (t with literal keys). */
function card(host: HTMLElement, title: string, subtitle: string): HTMLElement {
  const box = el("section", "appearance-card");
  groupTitle(box, title, subtitle);
  host.append(box);
  return box;
}

/** createDesignRow: a grouped row; rows with a ConfigSettingTargets id carry it for 功能搜索's row
 *  location. Rows holding a path, the preview or the rules table put them under their name
 *  (appearance.css). */
function row(box: HTMLElement, target: Target | null, title: string, subtitle: string, ...controls: (HTMLElement | string)[]): HTMLElement {
  const line = groupRow(box, title, subtitle, ...controls);
  if (target) line.dataset.settingTarget = target;
  return line;
}

/** A check box drawn as a switch. */
function check(key: string, label: string): HTMLInputElement {
  const input = el("input", "ds-switch");
  input.type = "checkbox";
  input.dataset.key = key;
  input.setAttribute("aria-label", label);
  return input;
}

/** A switch after its text, or a radio before it (the radio is drawn by its segmented group). */
function labelled(input: HTMLInputElement, label: string): HTMLLabelElement {
  const wrap = el("label", "appearance-check");
  if (input.type === "checkbox") wrap.append(label, input);
  else wrap.append(input, label);
  return wrap;
}

/** JSpinner(SpinnerNumberModel(value, min, max, step)): a typed value outside the model is
 *  rejected and the last valid one returns. */
function spinner(key: string, min: number, max: number, step: number, label: string): HTMLInputElement {
  const input = el("input");
  input.type = "number";
  input.min = String(min);
  input.max = String(max);
  input.step = String(step);
  input.dataset.key = key;
  input.title = label;
  let last = input.value;
  input.addEventListener("focus", () => (last = input.value));
  input.addEventListener("change", () => {
    const v = Number(input.value);
    if (input.value === "" || !Number.isFinite(v) || v < min || v > max) input.value = last;
    else last = input.value;
  });
  return input;
}

/** JFormattedTextField with DigitOnlyFilter; Utils.txtFieldIntValue reads a blank as 0. */
function digits(key: string): HTMLInputElement {
  const input = el("input", "appearance-number");
  input.type = "text";
  input.inputMode = "numeric";
  input.dataset.key = key;
  input.addEventListener("beforeinput", (e) => {
    if (!e.inputType.startsWith("insert")) return;
    const inserted = e.data ?? e.dataTransfer?.getData("text/plain") ?? "";
    const kept = inserted.replace(/\D+/g, "");
    if (kept === inserted) return;
    e.preventDefault();
    if (kept) input.setRangeText(kept, input.selectionStart ?? input.value.length, input.selectionEnd ?? input.value.length, "end");
  });
  return input;
}
const intValue = (input: HTMLInputElement) => {
  const s = input.value.trim();
  return s === "" || s.length >= 10 ? 0 : Number.parseInt(s, 10);
};

/** ColorLabel + 选择 (createColorControl, 4033-4053): the swatch opens the chooser; with alpha
 *  (ColorLabel useAplha) the alpha is edited beside it and written when not 255 (Theme.color2Array).
 *  `name` names the swatch; the alpha is "`subject`：不透明度". */
interface ColorControl {
  root: HTMLElement;
  swatch: HTMLInputElement;
  get(): number[];
  set(c: readonly number[]): void;
  name(swatchName: string, subject?: string): void;
}
function colorControl(key: string, alpha: boolean, chooseButton: boolean, name: string, subject = name): ColorControl {
  const root = el("span", "appearance-color");
  const swatch = el("input");
  swatch.type = "color";
  swatch.dataset.key = key;
  root.append(swatch);
  const opacity = el("input", "appearance-number");
  if (alpha) {
    opacity.type = "number";
    opacity.min = "0";
    opacity.max = "255";
    opacity.step = "1";
    opacity.dataset.key = `${key}.alpha`;
    opacity.addEventListener("change", () => {
      const v = Math.round(Number(opacity.value));
      opacity.value = String(Number.isFinite(v) ? Math.max(0, Math.min(255, v)) : 255);
    });
    root.append(stepper(opacity));
  }
  if (chooseButton) {
    const choose = el("button", "appearance-choose", t("ConfigDialog2.modern.chooseColor"));
    choose.type = "button";
    choose.onclick = () => swatch.click();
    root.append(choose);
  }
  const hex = (v: number) => v.toString(16).padStart(2, "0");
  const control: ColorControl = {
    root,
    swatch,
    get() {
      const v = swatch.value;
      const rgb = [1, 3, 5].map((k) => Number.parseInt(v.slice(k, k + 2), 16));
      const a = alpha ? Number(opacity.value) : 255;
      return a !== 255 ? [...rgb, a] : rgb;
    },
    set(c) {
      swatch.value = `#${hex(c[0])}${hex(c[1])}${hex(c[2])}`;
      opacity.value = String(c[3] ?? 255);
    },
    name(swatchName, about = swatchName) {
      swatch.setAttribute("aria-label", swatchName);
      if (alpha) opacity.setAttribute("aria-label", product("themeFieldOpacity", about));
    },
  };
  control.name(name, subject);
  return control;
}

/** A font combo box: Lizzie默认 (stored "Lizzie Default"), 系统默认 (stored "Dialog.plain"), then
 *  the system families (fontList, 4513-4521; getFontItemName 6374-6380). Each shows in its font. */
interface FontCombo {
  select: HTMLSelectElement;
  setFonts(fonts: string[]): void;
  get(): string;
  set(name: string): void;
}
function fontCombo(key: string): FontCombo {
  const select = el("select");
  select.dataset.key = key;
  let fonts: string[] = [];
  let loaded = false;
  // setFontValue (6114-6118): an unknown name selects index 0. "Dialog.plain" selects 系统默认,
  // which Java's combo could not find again (its items are display names).
  const pick = (name: string) => {
    select.value = [...select.options].some((o) => o.value === name) ? name : LIZZIE_DEFAULT_FONT;
    select.style.fontFamily = select.selectedIndex > 1 ? `"${select.value}"` : "";
  };
  const fill = (selected: string) => {
    select.replaceChildren();
    const add = (value: string, text: string) => {
      const option = el("option", undefined, text);
      option.value = value;
      if (value !== LIZZIE_DEFAULT_FONT && value !== SYSTEM_DEFAULT_FONT) option.style.fontFamily = `"${value}"`;
      select.append(option);
    };
    add(LIZZIE_DEFAULT_FONT, t("FontList.lizzieDefault"));
    add(SYSTEM_DEFAULT_FONT, t("FontList.systemDefault"));
    // Java fills the whole list before selecting (4513-4526): until the system list arrives the
    // requested font is kept as its own item instead of falling back to index 0.
    for (const font of loaded ? fonts : [selected]) if (font !== LIZZIE_DEFAULT_FONT && font !== SYSTEM_DEFAULT_FONT) add(font, font);
    pick(selected);
  };
  const combo: FontCombo = {
    select,
    setFonts(next) {
      const selected = combo.get();
      fonts = next;
      loaded = true;
      fill(selected);
    },
    get: () => select.value || LIZZIE_DEFAULT_FONT,
    set: (name) => (loaded ? pick(name) : fill(name)),
  };
  select.addEventListener("change", () => pick(select.value));
  fill(LIZZIE_DEFAULT_FONT);
  return combo;
}

/** An image path field with its "..." chooser (addAssetRow, 4117-4143; getImagePath 5283-5302);
 *  `choose` opens the chooser and tells whether a file was chosen. */
interface PathControl {
  root: HTMLElement;
  input: HTMLInputElement;
  browse: HTMLButtonElement;
  choose(): Promise<boolean>;
}
function pathControl(key: string, onChange: () => void): PathControl {
  const root = el("span", "appearance-path");
  const input = el("input");
  input.type = "text";
  input.dataset.key = key;
  input.addEventListener("change", onChange);
  const browse = el("button", undefined, "...");
  browse.type = "button";
  browse.dataset.key = `${key}.browse`;
  const choose = async () => {
    const path = await chooseAppearanceImage();
    if (!path) return false;
    input.value = path;
    onChange();
    return true;
  };
  browse.onclick = () => void choose();
  root.append(input, browse);
  return { root, input, browse, choose };
}

/** A 界面颜色 row's colour: the swatch shows the theme's colour, or the skin's own while the theme
 *  leaves it unset (empty); 恢复默认 unsets it. */
interface InterfaceColorControl {
  root: HTMLElement;
  get(): readonly number[];
  set(value: readonly number[], skin: readonly number[]): void;
}
function interfaceColor(key: string, name: string): InterfaceColorControl {
  const color = colorControl(key, false, false, name);
  const reset = el("button", undefined, product("themeUiReset"));
  reset.type = "button";
  reset.dataset.key = `${key}.reset`;
  reset.setAttribute("aria-label", product("themeUiResetFor", name));
  const root = el("span", "appearance-pair");
  root.append(color.root, reset);
  let value: readonly number[] = [];
  let skin: readonly number[] = [];
  const show = () => {
    color.set(value.length === 3 ? value : skin);
    reset.disabled = value.length !== 3;
  };
  color.swatch.addEventListener("input", () => {
    value = color.get();
    reset.disabled = false;
  });
  reset.onclick = () => {
    value = [];
    show();
    // The button it was on is unavailable now.
    color.swatch.focus();
  };
  return {
    root,
    get: () => value,
    set(next, base) {
      value = next;
      skin = base;
      show();
    },
  };
}

/** 棋盘 or 棋子 chosen by thumbnail (DESIGN.md 图片选择): a radio group of the pictures the program
 *  ships and 纯色, then 其他图片…, a button that opens the file chooser. The page keeps the values;
 *  `show` marks the choice they make. */
interface Tiles {
  root: HTMLElement;
  /** The 纯色 tile's picture. */
  pure: HTMLElement;
  /** The 其他图片… tile's picture. */
  other: HTMLElement;
  setShipped(list: { id: string; name: string; picture: HTMLElement }[]): void;
  /** Marks `id`: "pure", "other" or a shipped tile's id. */
  show(id: string): void;
}
function tiles(name: string, keyBase: string, pureKey: string, onPick: (id: string) => void): Tiles {
  const root = el("span", "appearance-tiles");
  const group = el("span", "appearance-tile-group");
  group.setAttribute("role", "radiogroup");
  group.setAttribute("aria-label", name);
  const radioName = `appearance-${keyBase.replace(/\W/g, "-")}-${Math.random().toString(36).slice(2)}`;
  const tile = (id: string, text: string, key: string, picture: HTMLElement) => {
    const label = el("label", "appearance-tile");
    const input = el("input");
    input.type = "radio";
    input.name = radioName;
    input.value = id;
    input.dataset.key = key;
    input.addEventListener("change", () => {
      if (input.checked) onPick(id);
    });
    const frame = el("span", "appearance-tile-picture");
    frame.append(picture);
    label.append(input, frame, el("span", "appearance-tile-name", text));
    return label;
  };
  const pure = el("span", "appearance-tile-fill");
  const pureTile = tile("pure", product("themePickPure"), pureKey, pure);
  const other = el("span", "appearance-tile-picture");
  const otherTile = el("button", "appearance-tile");
  otherTile.type = "button";
  otherTile.dataset.key = `${keyBase}.other`;
  otherTile.append(other, el("span", "appearance-tile-name", product("themePickOther")));
  otherTile.onclick = () => onPick("other");
  group.append(pureTile);
  root.append(group, otherTile);
  let shipped: HTMLElement[] = [];
  return {
    root,
    pure,
    other,
    setShipped(list) {
      for (const old of shipped) old.remove();
      shipped = list.map((s) => tile(s.id, s.name, `${keyBase}.${s.id}`, s.picture));
      pureTile.before(...shipped);
    },
    show(id) {
      for (const input of group.querySelectorAll("input")) input.checked = input.value === id;
      otherTile.classList.toggle("chosen", id === "other");
      if (id === "other") otherTile.setAttribute("aria-current", "true");
      else otherTile.removeAttribute("aria-current");
    },
  };
}

/** A tile's picture: one image (a board), or two side by side (a black and a white stone). */
function tilePicture(...urls: string[]): HTMLElement {
  const box = el("span", urls.length > 1 ? "appearance-tile-stones" : "appearance-tile-image");
  for (const url of urls) {
    const img = el("img");
    img.src = url;
    img.alt = "";
    img.draggable = false;
    box.append(img);
  }
  return box;
}

// ------------------------------------------------------------------ 棋盘预览

const PREVIEW_W = 220;
const PREVIEW_H = 180;

/** pnlBoardPreview.paintComponent (ConfigDialog2.java:4980-5196) for the draft: the background and
 *  the board from (30, 30), both tiled from the origin or in their pure colours, 19 lines every 30 px
 *  from (60, 60), a black stone at (120, 150) and a white one at (90, 120) with radius 14. With
 *  立体棋子 (`enhanced`) the shadows are the board's soft ones, both under the stones. */
function paintPreview(canvas: HTMLCanvasElement, v: ThemeSettings, images: LoadedImages, enhanced: boolean): void {
  const dpr = window.devicePixelRatio || 1;
  canvas.width = PREVIEW_W * dpr;
  canvas.height = PREVIEW_H * dpr;
  const g = canvas.getContext("2d")!;
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.clearRect(0, 0, PREVIEW_W, PREVIEW_H);
  const tiled = (img: HTMLImageElement, x: number) => {
    g.fillStyle = g.createPattern(img, "repeat")!;
    g.fillRect(x, x, Math.max(img.naturalWidth, PREVIEW_W), Math.max(img.naturalHeight, PREVIEW_H));
  };
  const pureBackground = cssColor(opaque(v.pureBackgroundColor));
  if (!images.background || v.usePureBackground) {
    g.fillStyle = pureBackground;
    g.fillRect(0, 0, PREVIEW_W, PREVIEW_H);
  } else tiled(images.background, 0);
  const pureBoard = cssColor(opaque(v.pureBoardColor));
  if (!images.board || v.usePureBoard) {
    g.fillStyle = pureBoard;
    g.fillRect(30, 30, PREVIEW_W, PREVIEW_H);
  } else tiled(images.board, 30);
  const [x, y, square] = [60, 60, 30];
  const radius = square / 2 - 1;
  const size = radius * 2 + 1;
  const r = (radius * v.shadowSize) / 100;
  const shadow = Math.trunc(r * 0.2) || 1;
  const farther = Math.trunc(r * 0.17) || 1;
  g.strokeStyle = "#000";
  g.lineWidth = 1;
  g.beginPath();
  for (let k = 0; k < 19; k++) {
    g.moveTo(x, y + square * k + 0.5);
    g.lineTo(PREVIEW_H, y + square * k + 0.5);
    g.moveTo(x + square * k + 0.5, y);
    g.lineTo(x + square * k + 0.5, PREVIEW_W);
  }
  g.stroke();
  const oval = (cx: number, cy: number, rr: number) => {
    g.beginPath();
    g.ellipse(cx + 0.5, cy + 0.5, rr + 0.5, rr + 0.5, 0, 0, Math.PI * 2);
    g.fill();
  };
  // The two stones stand on points (2, 3) and (1, 2) of the grid; 立体棋子 picks their pictures there
  // as the board does.
  const stones = [[x + square * 2, y + square * 3, 2, 3], [x + square, y + square * 2, 1, 2]];
  if (enhanced && v.showStoneShadow) for (const [sx, sy] of stones) softStoneShadow(g, sx + 0.5, sy + 0.5, radius, v.shadowSize);
  const picture = (own: HTMLImageElement | undefined, others: HTMLImageElement[] | undefined, [, , px, py]: number[]) => {
    if (!enhanced || !others?.length) return own;
    const pick = stonePicture(others.length + 1, px, py);
    return pick === 0 ? own : others[pick - 1];
  };
  const stone = (sx: number, sy: number, black: boolean, img: HTMLImageElement | undefined) => {
    if (v.showStoneShadow && !enhanced) {
      const top = g.createRadialGradient(sx, sy, 0, sx, sy, radius + shadow);
      top.addColorStop(0.3, "rgba(50,50,50,0.588)");
      top.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = top;
      oval(sx, sy, radius + shadow);
      const lower = g.createRadialGradient(sx + shadow, sy + shadow, 0, sx + shadow, sy + shadow, radius + farther);
      lower.addColorStop(0.6, "rgba(0,0,0,0.549)");
      lower.addColorStop(1, "rgba(0,0,0,0)");
      g.fillStyle = lower;
      oval(sx + shadow, sy + shadow, radius + farther);
    }
    if (v.usePureStone || !img) {
      // drawStoneSimple (5264-5273).
      g.fillStyle = black ? "#000" : "#fff";
      oval(sx, sy, radius);
      if (!black) {
        g.strokeStyle = "#000";
        g.lineWidth = Math.max(radius / 16, 1);
        g.beginPath();
        g.arc(sx, sy, radius, 0, Math.PI * 2);
        g.stroke();
      }
    } else g.drawImage(img, sx - radius, sy - radius, size, size);
  };
  stone(stones[0][0], stones[0][1], true, picture(images.black, images.blackVariants, stones[0]));
  stone(stones[1][0], stones[1][1], false, picture(images.white, images.whiteVariants, stones[1]));
}

/** The draft's preview images. An image the theme names that does not read is left out, so the
 *  preview shows the pure colour or the plain stone instead (pnlBoardPreview 4994-5049, 5113-5115);
 *  the core answers the default image only when neither the custom override nor the theme's own
 *  file reads. An empty path, or the default image's own address, is the default image, as on the
 *  board, and where the workbench shows the program's own wallpaper for that default background
 *  (any theme but the Java ones) the preview shows it too. `variants` (立体棋子's other stone
 *  pictures) settles separately, so the preview need not wait for pictures it may not show. */
async function loadPreviewImages(settings: AppearanceSettings, draft: ThemeSettings): Promise<{ images: LoadedImages; variants: Promise<Pick<LoadedImages, "blackVariants" | "whiteVariants">> }> {
  const urls = await appearanceImages(settings, draft).catch(() => null);
  if (!urls) return { images: {}, variants: Promise.resolve({}) };
  const unread = (url: string, path: string, fallback: string) => path !== "" && path !== fallback && url === fallback;
  const decoded = (url: string, path: string, fallback: string): Promise<HTMLImageElement | undefined> => {
    if (unread(url, path, fallback)) return Promise.resolve(undefined);
    const img = new Image();
    img.src = url;
    return img.decode().then(() => img, () => undefined);
  };
  const variants = Promise.all([loadVariants(urls.blackVariants), loadVariants(urls.whiteVariants)])
    .then(([blackVariants, whiteVariants]) => ({ blackVariants, whiteVariants }));
  const linen = !unread(urls.background, draft.backgroundImage, DEFAULT_PATHS.background) && programStage(themeSkin(settings.theme) !== "modern", false, urls.background);
  const [background, board, black, white] = await Promise.all([
    linen ? stageReady.then(stageWallpaper) : decoded(urls.background, draft.backgroundImage, DEFAULT_PATHS.background),
    decoded(urls.board, draft.boardImage, DEFAULT_PATHS.board),
    decoded(urls.black, draft.blackStoneImage, DEFAULT_PATHS.black),
    decoded(urls.white, draft.whiteStoneImage, DEFAULT_PATHS.white),
  ]);
  return { images: { background, board, black, white }, variants };
}

// ------------------------------------------------------------------ page

/** JOptionPane.showInputDialog(dialog, null, ConfigDialog2.inputThemeNameTitle, INFORMATION_MESSAGE)
 *  (ConfigDialog2.java:4456-4461): the title alone, no message above the field. */
async function themeNameDialog(): Promise<string | null> {
  const body = document.createElement("div");
  body.className = "display-input";
  const field = document.createElement("input");
  field.type = "text";
  field.setAttribute("aria-label", t("ConfigDialog2.inputThemeNameTitle"));
  body.append(field);
  queueMicrotask(() => field.focus());
  return (await ask(t("ConfigDialog2.inputThemeNameTitle"), body, product("ok"), product("cancel"))) ? field.value : null;
}

function buildThemePage(host: HTMLElement, dialog: SettingsDialog): { save: () => Promise<boolean> } {
  const view = state.view!;
  const committed = { name: view.settings.appearance.theme, values: view.settings.theme };
  let catalog: AppearanceCatalog | null = null;
  let selected = committed.name;
  /** The values of the selected theme as read; fields the page does not show round-trip. */
  let base: ThemeSettings = structuredClone(committed.values);
  const redraw = () => schedulePreview();

  // 主题外观: 当前主题 and 棋盘预览.
  const profile = card(host, t("ConfigDialog2.modern.theme.title"), t("ConfigDialog2.modern.theme.subtitle"));
  const themes = el("select");
  themes.dataset.key = "settings.theme.current";
  const add = el("button", undefined, t("ConfigDialog2.addTheme"));
  add.type = "button";
  add.dataset.key = "ConfigDialog2.addTheme";
  const remove = el("button", "danger", t("ConfigDialog2.deleteTheme"));
  remove.type = "button";
  remove.dataset.key = "ConfigDialog2.deleteTheme";
  row(profile, "config.theme.current", t("ConfigDialog2.modern.theme.current"), product("themeManageSub", t("ConfigDialog2.modern.theme.currentSub")), themes, add, remove);
  const preview = el("canvas", "appearance-preview");
  preview.style.width = `${PREVIEW_W}px`;
  preview.style.height = `${PREVIEW_H}px`;
  preview.dataset.key = "settings.theme.preview";
  row(profile, null, t("ConfigDialog2.modern.theme.preview"), t("ConfigDialog2.modern.theme.previewSub"), preview);

  // 界面颜色 (this product's own): each swatch shows the theme's colour, or the skin's while unset.
  const interfaceCard = card(host, product("themeUiTitle"), product("themeUiSub"));
  const interfaceRow = (key: string, title: string, sub: string) => {
    const control = interfaceColor(`settings.theme.${key}`, title);
    row(interfaceCard, null, title, sub, control.root);
    return control;
  };
  const interfaceColors: Record<keyof InterfaceColors, InterfaceColorControl> = {
    chrome: interfaceRow("uiChromeColor", product("themeUiChrome"), product("themeUiChromeSub")),
    control: interfaceRow("uiControlColor", product("themeUiControl"), product("themeUiControlSub")),
    accent: interfaceRow("uiAccentColor", product("themeUiAccent"), product("themeUiAccentSub")),
    text: interfaceRow("uiTextColor", product("themeUiText"), product("themeUiTextSub")),
  };

  // 线条与字体.
  const lines = card(host, t("ConfigDialog2.modern.theme.lines"), t("ConfigDialog2.modern.theme.linesSub"));
  const winrateWidth = spinner("settings.theme.winrateStrokeWidth", 0.1, 10, 0.1, t("LizzieConfig.title.winrateStrokeWidth"));
  const scoreWidth = spinner("settings.theme.scoreLeadStrokeWidth", 0.1, 10, 0.1, t("LizzieConfig.title.scoreLeadStrokeWidth"));
  // Two widths in one row: each field has its line's name before it, the two kept together when
  // the row wraps, and that line's name as its accessible name.
  winrateWidth.setAttribute("aria-label", t("LizzieConfig.title.winrateStrokeWidth"));
  scoreWidth.setAttribute("aria-label", t("LizzieConfig.title.scoreLeadStrokeWidth"));
  const named = (name: string, field: HTMLInputElement) => {
    const pair = el("span", "appearance-pair", name);
    pair.append(stepper(field));
    return pair;
  };
  row(lines, "config.theme.winrate-width", t("ConfigDialog2.modern.theme.winrateWidth"), t("ConfigDialog2.modern.theme.winrateWidthSub"),
    named(product("themeWidthWinrate"), winrateWidth), named(product("themeWidthScore"), scoreWidth));
  const barWidth = spinner("settings.theme.minimumBlunderBarWidth", 1, 10, 1, t("LizzieConfig.title.minimumBlunderBarWidth"));
  row(lines, "config.theme.blunder-width", t("ConfigDialog2.modern.theme.blunderWidth"), t("ConfigDialog2.modern.theme.blunderWidthSub"), stepper(barWidth));
  const shadowLabel = t("LizzieConfig.title.shadowSize");
  // The size field takes the row's name; the switch says what it turns on.
  const showShadow = check("settings.theme.showStoneShadow", product("themeFieldShowShadow"));
  const shadowSize = spinner("settings.theme.shadowSize", 1, 150, 1, shadowLabel);
  showShadow.addEventListener("change", () => (shadowSize.disabled = !showShadow.checked));
  row(lines, "config.theme.shadow", t("ConfigDialog2.modern.theme.shadow"), product("themeShadowSub"), showShadow, stepper(shadowSize), "%");
  const infoFont = fontCombo("settings.theme.fontName");
  row(lines, "config.theme.info-font", t("ConfigDialog2.modern.theme.infoFont"), t("ConfigDialog2.modern.theme.infoFontSub"), infoFont.select);
  const uiFont = fontCombo("settings.theme.uiFontName");
  row(lines, "config.theme.ui-font", t("ConfigDialog2.modern.theme.uiFont"), t("ConfigDialog2.modern.theme.uiFontSub"), uiFont.select);
  // 设置 > 界面字体大小 (SetFrameFontSize.java:31-146): 12-20, saved for the next start.
  const savedFrameSize = view.settings.appearance.frameFontSize;
  const frameSize = spinner("settings.appearance.frameFontSize", 12, 20, 1, product("settingsFrameFont"));
  frameSize.value = String(savedFrameSize);
  row(lines, "menu.frameFontSize", product("settingsFrameFont"), product("settingsFrameFontSub"), stepper(frameSize));
  const winrateFont = fontCombo("settings.theme.winrateFontName");
  row(lines, "config.theme.winrate-font", t("ConfigDialog2.modern.theme.winrateFont"), t("ConfigDialog2.modern.theme.winrateFontSub"), winrateFont.select);

  // 图片与材质.
  const assets = card(host, t("ConfigDialog2.modern.theme.assets"), t("ConfigDialog2.modern.theme.assetsSub"));
  const assetHint = t("ConfigDialog2.modern.theme.assetHint");
  const pureBackgroundLabel = t("LizzieConfig.title.chkPureBackground");
  const pureBackground = check("settings.theme.usePureBackground", pureBackgroundLabel);
  const pureBackgroundColor = colorControl("settings.theme.pureBackgroundColor", false, false, product("themeFieldColor", pureBackgroundLabel));
  const backgroundPath = pathControl("settings.theme.backgroundImage", redraw);
  row(assets, "config.theme.background-image", t("ConfigDialog2.modern.theme.backgroundImage"), assetHint, labelled(pureBackground, pureBackgroundLabel), pureBackgroundColor.root, backgroundPath.root);
  // 棋盘 and 棋子: thumbnails of the pictures the program ships, 纯色 and 其他图片… over the paths,
  // which stay editable for typing a path.
  let pureBoard = false;
  let pureStone = false;
  let boards: ShippedBoard[] = [];
  let stoneSets: ShippedStones[] = [];
  const boardTitle = t("ConfigDialog2.modern.theme.boardImage");
  const pureBoardColor = colorControl("settings.theme.pureBoardColor", false, false, product("themeFieldColor", t("LizzieConfig.title.chkPureBoard")));
  // The paths' own edits and their "..." choices mark the tile they make.
  const boardPath = pathControl("settings.theme.boardImage", () => syncPictures());
  const boardTiles = tiles(boardTitle, "settings.theme.board", "settings.theme.usePureBoard", (id) => void pickBoard(id));
  row(assets, "config.theme.board-image", boardTitle, product("themeBoardPickSub"), boardTiles.root, pureBoardColor.root, boardPath.root);
  const stoneTiles = tiles(product("themeStones"), "settings.theme.stones", "settings.theme.usePureStone", (id) => void pickStones(id));
  stoneTiles.pure.append(el("span", "appearance-flat-stone black"), el("span", "appearance-flat-stone white"));
  row(assets, null, product("themeStones"), product("themeStonesSub"), stoneTiles.root);
  const blackPath = pathControl("settings.theme.blackStoneImage", () => syncPictures());
  row(assets, "config.theme.black-stone-image", t("ConfigDialog2.modern.theme.blackStoneImage"), product("themeStonePathSub"), blackPath.root);
  const whitePath = pathControl("settings.theme.whiteStoneImage", () => syncPictures());
  row(assets, "config.theme.white-stone-image", t("ConfigDialog2.modern.theme.whiteStoneImage"), product("themeStonePathSub"), whitePath.root);
  // 立体棋子 is the appearance's, not the theme's: it stays when another theme is chosen.
  const depth = check("settings.appearance.enhancedStones", product("themeDepth"));
  depth.checked = view.settings.appearance.enhancedStones;
  row(assets, null, product("themeDepth"), product("themeDepthSub"), depth);
  const backgroundFilter = digits("settings.theme.backgroundFilter");
  row(assets, "config.theme.background-blur", t("ConfigDialog2.modern.theme.blur"), t("ConfigDialog2.modern.theme.blurSub"), backgroundFilter);
  /** The pure toggles' listeners (4934-4976): the paths follow them, the 默认 theme's too (Java
   *  fixes its images); the filter follows 纯色背景. */
  const syncEnabled = () => {
    backgroundPath.input.disabled = backgroundPath.browse.disabled = pureBackground.checked;
    boardPath.input.disabled = boardPath.browse.disabled = pureBoard;
    for (const p of [blackPath, whitePath]) p.input.disabled = p.browse.disabled = pureStone;
    backgroundFilter.disabled = pureBackground.checked;
    shadowSize.disabled = !showShadow.checked;
  };
  pureBackground.addEventListener("change", syncEnabled);
  /** The selected theme's bundled pictures its folder replaces with the user's own of that name. */
  const replaced = () => catalog?.themes.find((e) => e.name === selected)?.replaced_images ?? [];
  /** The address an image value names, as the core reads it (appearance::images): a shipped
   *  address as it is, a relative path as the selected theme's own file (a bundled theme's own
   *  file is its shipped picture unless its folder replaces it, when it is no shipped picture: ""),
   *  an empty value as the default picture `fallback`. */
  const addressOf = (value: string, fallback: string) => {
    const path = value.trim().replaceAll("\\", "/");
    if (path === "") return fallback;
    if (path.startsWith("/")) return path;
    return replaced().includes(path) ? "" : `/themes/${selected}/${path}`;
  };
  /** The value choosing the shipped picture at `addresses` writes: the selected theme's own file by
   *  its name, so a bundled theme keeps its own value, or by its address where the theme's folder
   *  replaces that file; the 默认 theme's default picture as an empty value; else the picture's
   *  first address. */
  const valueFor = (addresses: readonly string[], fallback: string) => {
    const own = `/themes/${selected}/`;
    if (selected === DEFAULT_THEME && addresses[0] === fallback) return "";
    const mine = addresses.find((a) => a.startsWith(own));
    if (!mine) return addresses[0];
    return replaced().includes(mine.slice(own.length)) ? mine : mine.slice(own.length);
  };
  /** The tile the draft's values make: 纯色, a shipped picture, or 其他图片… for the user's own;
   *  none until the shipped pictures are known. */
  const boardChoice = () => {
    if (pureBoard) return "pure";
    if (boards.length === 0) return "";
    const at = addressOf(boardPath.input.value, DEFAULT_PATHS.board);
    return boards.find((b) => b.addresses.includes(at))?.theme ?? "other";
  };
  const stonesChoice = () => {
    if (pureStone) return "pure";
    if (stoneSets.length === 0) return "";
    const black = addressOf(blackPath.input.value, DEFAULT_PATHS.black);
    const white = addressOf(whitePath.input.value, DEFAULT_PATHS.white);
    return stoneSets.find((s) => s.pairs.some(([b, w]) => b === black && w === white))?.theme ?? "other";
  };
  /** 其他图片… shows the user's own picture while it is the choice. */
  const otherPictures = () => {
    const board = boardChoice() === "other" ? previewImages.board : undefined;
    boardTiles.other.replaceChildren(board ? tilePicture(board.src) : icon("open"));
    const stones = stonesChoice() === "other" ? [previewImages.black, previewImages.white].filter((img) => img !== undefined) : [];
    stoneTiles.other.replaceChildren(stones.length > 0 ? tilePicture(...stones.map((img) => img.src)) : icon("open"));
  };
  /** The tiles, the paths' availability and the preview after the values changed. */
  const syncPictures = () => {
    boardTiles.show(boardChoice());
    stoneTiles.show(stonesChoice());
    otherPictures();
    syncEnabled();
    redraw();
  };
  async function pickBoard(id: string) {
    if (id === "pure") pureBoard = true;
    else if (id === "other") {
      if (await boardPath.choose()) pureBoard = false;
    } else {
      const board = boards.find((b) => b.theme === id);
      if (board) {
        pureBoard = false;
        boardPath.input.value = valueFor(board.addresses, DEFAULT_PATHS.board);
      }
    }
    syncPictures();
  }
  async function pickStones(id: string) {
    if (id === "pure") pureStone = true;
    else if (id === "other") {
      if (await blackPath.choose()) pureStone = false;
    } else {
      const set = stoneSets.find((s) => s.theme === id);
      if (set) {
        const [black, white] = set.pairs.find(([b]) => b.startsWith(`/themes/${selected}/`)) ?? set.pairs[0];
        pureStone = false;
        blackPath.input.value = valueFor([black], DEFAULT_PATHS.black);
        whitePath.input.value = valueFor([white], DEFAULT_PATHS.white);
      }
    }
    syncPictures();
  }
  for (const p of [boardPath, blackPath, whitePath]) p.input.addEventListener("input", syncPictures);
  pureBoardColor.swatch.addEventListener("input", () => (boardTiles.pure.style.backgroundColor = pureBoardColor.swatch.value));

  // 颜色与标记.
  const colors = card(host, t("ConfigDialog2.modern.theme.colors"), t("ConfigDialog2.modern.theme.colorsSub"));
  const colorHint = t("ConfigDialog2.modern.colorRowHint");
  const colorRow = (target: Target, title: string, key: string) => {
    const control = colorControl(key, true, true, title);
    row(colors, target, title, colorHint, control.root);
    return control;
  };
  const winrateColor = colorRow("config.theme.winrate-color", t("ConfigDialog2.modern.theme.winrateColor"), "settings.theme.winrateLineColor");
  const missColor = colorRow("config.theme.missing-color", t("ConfigDialog2.modern.theme.missingColor"), "settings.theme.winrateMissLineColor");
  const blunderColor = colorRow("config.theme.blunder-color", t("ConfigDialog2.modern.theme.blunderColor"), "settings.theme.blunderBarColor");
  const scoreColor = colorRow("config.theme.score-color", t("ConfigDialog2.modern.theme.scoreColor"), "settings.theme.scoremeanLineColor");
  const commentBackground = colorRow("config.theme.comment-background", t("ConfigDialog2.modern.theme.commentBackground"), "settings.theme.commentBackgroundColor");
  const commentText = colorRow("config.theme.comment-text", t("ConfigDialog2.modern.theme.commentText"), "settings.theme.commentFontColor");
  const bestMove = colorRow("config.theme.best-move-color", t("ConfigDialog2.modern.theme.bestMove"), "settings.theme.bestMoveColor");
  const commentFontSize = digits("settings.theme.commentFontSize");
  row(colors, "config.theme.comment-font-size", t("ConfigDialog2.modern.theme.commentFontSize"), t("ConfigDialog2.modern.theme.commentFontSizeSub"), commentFontSize);
  // rdoStoneIndicator* in rowOf order; values per getStoneIndicatorType (6098-6106).
  const indicators: [number, string][] = [[1, t("ConfigDialog2.circle")], [0, t("ConfigDialog2.triangle")], [2, t("ConfigDialog2.solid")], [3, t("ConfigDialog2.empty")]];
  const indicatorGroup = `appearance-indicator-${Math.random().toString(36).slice(2)}`;
  const indicatorInputs = indicators.map(([value, text]) => {
    const input = el("input");
    input.type = "radio";
    input.name = indicatorGroup;
    input.value = String(value);
    input.dataset.key = `settings.theme.stoneIndicatorType.${value}`;
    return labelled(input, text);
  });
  const indicatorChoices = el("span", "ds-seg");
  indicatorChoices.setAttribute("role", "radiogroup");
  indicatorChoices.append(...indicatorInputs);
  row(colors, "config.theme.stone-indicator", t("ConfigDialog2.modern.theme.indicator"), t("ConfigDialog2.modern.theme.indicatorSub"), indicatorChoices);
  const commentNodeTitle = t("ConfigDialog2.modern.theme.commentNode");
  const showCommentNode = check("settings.theme.showCommentNodeColor", commentNodeTitle);
  const commentNodeColor = colorControl("settings.theme.commentNodeColor", true, false, product("themeFieldColor", commentNodeTitle), commentNodeTitle);
  row(colors, "config.theme.comment-node-color", commentNodeTitle, t("ConfigDialog2.modern.theme.commentNodeSub"), showCommentNode, commentNodeColor.root);

  // 选点评估关注外框.
  const tracking = view.settings.tracking;
  const trackingCard = card(host, t("ConfigDialog2.modern.trackingAppearance.title"), t("ConfigDialog2.modern.trackingAppearance.subtitle"));
  const outlineTitle = t("ConfigDialog2.modern.trackingAppearance.outline");
  const outline = check("settings.tracking.showOutline", outlineTitle);
  outline.checked = tracking.showOutline;
  row(trackingCard, "config.tracking.outline", outlineTitle, t("ConfigDialog2.modern.trackingAppearance.outlineSub"), outline);
  const opacity = el("input");
  opacity.type = "range";
  opacity.min = "0";
  opacity.max = "100";
  opacity.step = "1";
  opacity.value = String(tracking.outlineOpacity);
  opacity.dataset.key = "settings.tracking.outlineOpacity";
  opacity.setAttribute("aria-label", t("ConfigDialog2.modern.trackingAppearance.outlineOpacity"));
  const opacityValue = el("span", undefined, `${opacity.value}%`);
  opacity.oninput = () => (opacityValue.textContent = `${opacity.value}%`);
  row(trackingCard, "config.tracking.outline-opacity", t("ConfigDialog2.modern.trackingAppearance.outlineOpacity"), t("ConfigDialog2.modern.trackingAppearance.outlineOpacitySub"), opacity, opacityValue);

  // 错误节点.
  const blunders = card(host, t("ConfigDialog2.modern.theme.blunders"), t("ConfigDialog2.modern.theme.blundersSub"));
  const tableWrap = el("div", "appearance-blunder-table");
  const table = el("table", "appearance-rules");
  const head = el("tr");
  head.append(el("th", undefined, t("LizzieConfig.title.blunderThresholds")), el("th", undefined, t("LizzieConfig.title.blunderColor")));
  const thead = el("thead");
  thead.append(head);
  const tbody = el("tbody");
  table.append(thead, tbody);
  tableWrap.append(table);
  interface RuleRow { tr: HTMLTableRowElement; threshold: HTMLInputElement; color: ColorControl }
  let rules: RuleRow[] = [];
  let selectedRule = -1;
  const selectRule = (k: number) => {
    selectedRule = k;
    rules.forEach((rule, n) => rule.tr.classList.toggle("selected", n === k));
  };
  /** The fields of each rule carry its row number, so the rules' fields stay apart. */
  const numberRules = () => rules.forEach((rule, k) => {
    rule.threshold.setAttribute("aria-label", `${t("LizzieConfig.title.blunderThresholds")} ${k + 1}`);
    rule.color.name(`${t("LizzieConfig.title.blunderColor")} ${k + 1}`);
  });
  /** BlunderNodeTableModel.addRow: a threshold (empty for 添加) and its colour. */
  const addRule = (threshold: number | null, color: readonly number[]) => {
    const tr = el("tr");
    const input = el("input");
    input.type = "number";
    input.step = "any";
    input.dataset.key = "settings.theme.blunderWinrateThreshold";
    input.value = threshold === null ? "" : String(threshold);
    const control = colorControl("settings.theme.blunderNodeColor", true, false, t("LizzieConfig.title.blunderColor"));
    control.set(color);
    const a = el("td");
    a.append(stepper(input));
    const b = el("td");
    b.append(control.root);
    tr.append(a, b);
    const rule = { tr, threshold: input, color: control };
    tr.addEventListener("pointerdown", () => selectRule(rules.indexOf(rule)));
    tr.addEventListener("focusin", () => selectRule(rules.indexOf(rule)));
    rules.push(rule);
    tbody.append(tr);
    numberRules();
  };
  const setRules = (thresholds: readonly number[], colorsByRow: readonly (readonly number[])[]) => {
    tbody.replaceChildren();
    rules = [];
    selectedRule = -1;
    thresholds.forEach((v, k) => addRule(v, colorsByRow[k]));
  };
  const addRow = el("button", undefined, t("LizzieConfig.button.add"));
  addRow.type = "button";
  addRow.dataset.key = "LizzieConfig.button.add";
  addRow.onclick = () => addRule(null, [255, 255, 255]);
  const removeRow = el("button", undefined, t("LizzieConfig.button.remove"));
  removeRow.type = "button";
  removeRow.dataset.key = "LizzieConfig.button.remove";
  removeRow.onclick = () => {
    if (selectedRule < 0 || selectedRule >= rules.length) return;
    rules[selectedRule].tr.remove();
    rules.splice(selectedRule, 1);
    numberRules();
    selectRule(-1);
  };
  const resetRows = el("button", undefined, t("LizzieConfig.button.reset"));
  resetRows.type = "button";
  resetRows.dataset.key = "LizzieConfig.button.reset";
  // 重置 (ConfigDialog2.java:4826-4835): saveConfig commits the whole dialog, resetBlunderColor
  // (Config.java:1491-1508) then writes the default rules into the active theme and the dialog
  // reopens on 主题. Saving the draft with the default rules is that same end state.
  resetRows.onclick = async () => {
    setRules(DEFAULT_BLUNDER_THRESHOLDS, DEFAULT_BLUNDER_COLORS);
    await dialog.saveAndReopen("theme");
  };
  const ruleButtons = el("div", "appearance-rule-buttons");
  ruleButtons.append(addRow, removeRow, resetRows);
  row(blunders, "config.theme.blunder-rules", t("ConfigDialog2.modern.theme.blunderRules"), t("ConfigDialog2.modern.theme.blunderRulesSub"), tableWrap, ruleButtons);
  const useScoreDiff = check("settings.theme.useScorediffInVariationTree", t("LizzieConfig.chkUseScoreDiff"));
  // DoubleDocument: the field takes a decimal number.
  const scoreDiffPercent = el("input", "appearance-number");
  scoreDiffPercent.type = "text";
  scoreDiffPercent.inputMode = "decimal";
  scoreDiffPercent.dataset.key = "settings.theme.scorediffInVariationTreeFactor";
  scoreDiffPercent.title = t("LizzieConfig.lblUseScoreDiffPercent");
  scoreDiffPercent.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /[^\d.\-]/.test(e.data ?? "")) e.preventDefault();
  });
  useScoreDiff.addEventListener("change", () => (scoreDiffPercent.disabled = !useScoreDiff.checked));
  row(blunders, "config.theme.score-blunders", t("ConfigDialog2.modern.theme.scoreBlunders"), t("ConfigDialog2.modern.theme.scoreBlundersSub"), useScoreDiff, scoreDiffPercent);

  /** readThemeValues / readDefaultTheme: the controls show `v`. */
  const fill = (v: ThemeSettings) => {
    base = structuredClone(v);
    const skin = skinColors(themeSkin(selected));
    interfaceColors.chrome.set(v.uiChromeColor, skin.chrome);
    interfaceColors.control.set(v.uiControlColor, skin.control);
    interfaceColors.accent.set(v.uiAccentColor, skin.accent);
    interfaceColors.text.set(v.uiTextColor, skin.text);
    pureStone = v.usePureStone;
    pureBackground.checked = v.usePureBackground;
    pureBackgroundColor.set(opaque(v.pureBackgroundColor));
    pureBoard = v.usePureBoard;
    pureBoardColor.set(opaque(v.pureBoardColor));
    boardTiles.pure.style.backgroundColor = cssColor(opaque(v.pureBoardColor));
    showShadow.checked = v.showStoneShadow;
    winrateWidth.value = String(v.winrateStrokeWidth);
    scoreWidth.value = String(v.scoreLeadStrokeWidth);
    barWidth.value = String(v.minimumBlunderBarWidth);
    shadowSize.value = String(v.shadowSize);
    infoFont.set(v.fontName);
    uiFont.set(v.uiFontName);
    winrateFont.set(v.winrateFontName);
    backgroundPath.input.value = v.backgroundImage;
    boardPath.input.value = v.boardImage;
    blackPath.input.value = v.blackStoneImage;
    whitePath.input.value = v.whiteStoneImage;
    winrateColor.set(v.winrateLineColor);
    missColor.set(v.winrateMissLineColor);
    blunderColor.set(v.blunderBarColor);
    scoreColor.set(v.scoremeanLineColor);
    commentBackground.set(v.commentBackgroundColor);
    commentText.set(v.commentFontColor);
    // The best move swatch shows the colour at alpha 240 (6171-6173, 6350-6352).
    bestMove.set([...v.bestMoveColor.slice(0, 3), 240]);
    commentFontSize.value = String(v.commentFontSize);
    backgroundFilter.value = String(v.backgroundFilter);
    for (const label of indicatorInputs) {
      const input = label.querySelector("input")!;
      input.checked = Number(input.value) === v.stoneIndicatorType;
    }
    showCommentNode.checked = v.showCommentNodeColor;
    commentNodeColor.set(v.commentNodeColor);
    useScoreDiff.checked = v.useScorediffInVariationTree;
    scoreDiffPercent.value = String(v.scorediffInVariationTreeFactor * 100);
    scoreDiffPercent.disabled = !useScoreDiff.checked;
    setRules(v.blunderWinrateThresholds, v.blunderNodeColors);
    remove.disabled = selected === DEFAULT_THEME;
    syncPictures();
  };

  /** writeThemeValues / writeDefaultTheme: the draft as the controls show it. */
  const collect = (): ThemeSettings => {
    const number = (input: HTMLInputElement, fallback: number) => (input.value === "" || !Number.isFinite(Number(input.value)) ? fallback : Number(input.value));
    // finalizeEditedBlunderColors (6473-6479): the rows sorted by threshold, a blank one as 0.
    const rows = rules
      .map((rule) => ({ threshold: rule.threshold.value.trim() === "" || !Number.isFinite(Number(rule.threshold.value)) ? 0 : Number(rule.threshold.value), color: rule.color.get() }))
      .sort((a, b) => a.threshold - b.threshold);
    // Theme.parseBlunderNodeRuleSet (385-423) only accepts strictly increasing thresholds; Java saves
    // other tables and then reads the default rules back, which is what is saved here.
    const valid = rows.length > 0 && rows.every((r, k) => k === 0 || r.threshold > rows[k - 1].threshold);
    const checkedIndicator = indicatorInputs.map((l) => l.querySelector("input")!).find((input) => input.checked);
    // Utils.parseTextToDouble(txtPercentScoreDiff, previous * 100) / 100.
    const percent = Number.parseFloat(scoreDiffPercent.value);
    return {
      ...base,
      usePureStone: pureStone,
      usePureBoard: pureBoard,
      pureBoardColor: pureBoardColor.get(),
      usePureBackground: pureBackground.checked,
      pureBackgroundColor: pureBackgroundColor.get(),
      showStoneShadow: showShadow.checked,
      winrateStrokeWidth: number(winrateWidth, base.winrateStrokeWidth),
      scoreLeadStrokeWidth: number(scoreWidth, base.scoreLeadStrokeWidth),
      minimumBlunderBarWidth: number(barWidth, base.minimumBlunderBarWidth),
      shadowSize: number(shadowSize, base.shadowSize),
      fontName: infoFont.get(),
      uiFontName: uiFont.get(),
      winrateFontName: winrateFont.get(),
      backgroundImage: backgroundPath.input.value.trim(),
      boardImage: boardPath.input.value.trim(),
      blackStoneImage: blackPath.input.value.trim(),
      whiteStoneImage: whitePath.input.value.trim(),
      winrateLineColor: winrateColor.get(),
      winrateMissLineColor: missColor.get(),
      blunderBarColor: blunderColor.get(),
      scoremeanLineColor: scoreColor.get(),
      stoneIndicatorType: checkedIndicator ? Number(checkedIndicator.value) : base.stoneIndicatorType,
      showCommentNodeColor: showCommentNode.checked,
      commentNodeColor: commentNodeColor.get(),
      commentBackgroundColor: commentBackground.get(),
      commentFontColor: commentText.get(),
      bestMoveColor: bestMove.get(),
      commentFontSize: intValue(commentFontSize),
      backgroundFilter: intValue(backgroundFilter),
      blunderWinrateThresholds: valid ? rows.map((r) => r.threshold) : [...DEFAULT_BLUNDER_THRESHOLDS],
      blunderNodeColors: valid ? rows.map((r) => r.color) : DEFAULT_BLUNDER_COLORS.map((c) => [...c]),
      useScorediffInVariationTree: useScoreDiff.checked,
      scorediffInVariationTreeFactor: (Number.isFinite(percent) ? percent : base.scorediffInVariationTreeFactor * 100) / 100,
      uiChromeColor: [...interfaceColors.chrome.get()],
      uiControlColor: [...interfaceColors.control.get()],
      uiAccentColor: [...interfaceColors.accent.get()],
      uiTextColor: [...interfaceColors.text.get()],
    };
  };

  // 棋盘预览 repaints with the draft at once. Its images are decoded again 100 ms after the image
  // choices last changed, typing included (Java repaints on a 100 ms timer, 5220-5231), and only
  // the newest request paints.
  let previewKey = "";
  let previewImages: LoadedImages = {};
  let previewGeneration = 0;
  let previewPending = false;
  let previewTimer = 0;
  function schedulePreview() {
    if (previewPending) return;
    previewPending = true;
    requestAnimationFrame(() => {
      previewPending = false;
      if (!preview.isConnected && !host.isConnected) return;
      const draft = collect();
      // The default theme previews its own images without the custom overrides (readDefaultTheme),
      // a named theme its effective ones (Theme.backgroundPath / boardPath with the override).
      const settings = selected === DEFAULT_THEME
        ? { ...view.settings.appearance, theme: selected, customBoardImage: "", customBackgroundImage: "" }
        : { ...view.settings.appearance, theme: selected };
      const key = JSON.stringify([settings, draft.backgroundImage, draft.boardImage, draft.blackStoneImage, draft.whiteStoneImage]);
      if (key === previewKey) {
        paintPreview(preview, draft, previewImages, depth.checked);
        return;
      }
      // The first images load at once; until new ones arrive the preview keeps its last paint.
      const delay = previewKey === "" ? 0 : 100;
      previewKey = key;
      const mine = ++previewGeneration;
      window.clearTimeout(previewTimer);
      previewTimer = window.setTimeout(async () => {
        const { images, variants } = await loadPreviewImages(settings, draft);
        const stale = () => mine !== previewGeneration || (!preview.isConnected && !host.isConnected);
        if (stale()) return;
        previewImages = images;
        paintPreview(preview, collect(), previewImages, depth.checked);
        otherPictures();
        // 立体棋子's other stone pictures follow; until then each stone shows its first picture.
        const more = await variants;
        if (stale() || !(more.blackVariants?.length || more.whiteVariants?.length)) return;
        previewImages = { ...previewImages, ...more };
        paintPreview(preview, collect(), previewImages, depth.checked);
      }, delay);
    });
  }
  // Every control's edit repaints the preview with the draft.
  host.addEventListener("input", redraw);
  host.addEventListener("change", redraw);

  /** The theme combo box: 默认 first, then the catalog's themes (loadThemeTab 4372-4384). */
  const fillThemes = () => {
    themes.replaceChildren();
    const names = catalog?.themes.map((e) => e.name) ?? [DEFAULT_THEME, ...(committed.name !== DEFAULT_THEME ? [committed.name] : [])];
    for (const name of names) {
      const option = el("option", undefined, themeName(name));
      option.value = name;
      themes.append(option);
    }
    // A theme gone from the list (deleted) leaves the combo on its first item, 默认.
    if (!names.includes(selected)) {
      selected = DEFAULT_THEME;
      const values = entry(DEFAULT_THEME);
      if (values) fill(values);
    }
    themes.value = selected;
  };
  const entry = (name: string) => catalog?.themes.find((e) => e.name === name)?.values;
  themes.addEventListener("change", () => {
    selected = themes.value;
    const values = selected === committed.name ? committed.values : entry(selected);
    if (values) fill(values);
  });
  const reloadCatalog = async () => {
    try {
      catalog = await appearanceCatalog();
    } catch {
      return;
    }
    for (const combo of [infoFont, uiFont, winrateFont]) combo.setFonts(catalog.fonts);
    boards = catalog.boards;
    stoneSets = catalog.stones;
    boardTiles.setShipped(boards.map((b) => ({ id: b.theme, name: themeName(b.theme), picture: tilePicture(b.addresses[0]) })));
    stoneTiles.setShipped(stoneSets.map((s) => ({ id: s.theme, name: themeName(s.theme), picture: tilePicture(...s.pairs[0]) })));
    fillThemes();
    syncPictures();
  };

  // 添加 (4449-4477): a name not yet in the list, compared case-insensitively; then
  // setVisible(false) + openConfigDialog2(1) (4471-4472) drops every unsaved edit.
  add.onclick = async () => {
    const name = await themeNameDialog();
    if (name === null) return;
    // Java compares the list's names; the bundled themes are also taken under their stored names
    // and in every language, and stay taken after they are deleted.
    const names = [t("LizzieConfig.title.defaultTheme"), ...Object.keys(BUNDLED_THEME_KEYS), ...Object.values(BUNDLED_THEME_NAMES).flatMap((n) => Object.values(n)), ...(catalog?.themes.map((e) => e.name) ?? [])];
    if (names.some((n) => n.toLowerCase() === name.toLowerCase())) {
      await message(t("Message.title"), t("ConfigDialog2.duplicateThemeName"));
      return;
    }
    const result = await run({ type: "AddTheme", name });
    if (result && result.type !== "Refused") await dialog.reopen("theme");
  };
  // 删除 (4391-4435): asks first; the default theme cannot be deleted. Reopens like 添加
  // (4424-4425): the committed theme, or 默认 when it was the one deleted.
  remove.onclick = async () => {
    if (selected === DEFAULT_THEME) return;
    if (!(await confirm(t("LizzieFrame.warning"), `${t("ConfigDialog2.deleteThemeWarning")}'${themeName(selected)}' ?\n${product("themeDeleteDiscards")}`))) return;
    const result = await run({ type: "DeleteTheme", name: selected });
    if (result && result.type !== "Refused") await dialog.reopen("theme");
  };

  fillThemes();
  fill(committed.values);
  void reloadCatalog();

  return {
    save: async () => {
      const size = Number(frameSize.value);
      if (size !== (state.view?.settings.appearance.frameFontSize ?? savedFrameSize)) {
        const result = await run({ type: "SetFrameFontSize", size });
        if (!result || result.type === "Refused") return false;
        // LizzieFrame.java:20751-20768: outside Chinese a size over 12 may not fit every label.
        if (!isChinese() && size > 12) await message(t("Message.title"), t("menu.setFrameSizeAlart"));
      }
      const nowTracking = state.view?.settings.tracking ?? tracking;
      if (outline.checked !== nowTracking.showOutline || Number(opacity.value) !== nowTracking.outlineOpacity) {
        const result = await run({ type: "SetTrackingSettings", settings: { ...nowTracking, showOutline: outline.checked, outlineOpacity: Number(opacity.value) } });
        if (!result || result.type === "Refused") return false;
      }
      const values = collect();
      const settings = { ...view.settings.appearance, ...state.view?.settings.appearance, theme: selected, enhancedStones: depth.checked };
      const unchanged = selected === committed.name && depth.checked === (state.view?.settings.appearance ?? view.settings.appearance).enhancedStones;
      if (unchanged && JSON.stringify(values) === JSON.stringify(committed.values)) return true;
      const result = await run({ type: "SetAppearanceSettings", settings, theme: values });
      return !!result && result.type !== "Refused";
    },
  };
}

SETTINGS_PAGES.theme = buildThemePage;
// validateSettingRows(content, SettingSection.THEME): every row id is known to 功能搜索 from start-up,
// before the page is ever built (registration is metadata only).
registerSettingTargets("theme", TARGETS);
