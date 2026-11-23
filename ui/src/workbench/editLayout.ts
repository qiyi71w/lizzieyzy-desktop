// 布局编辑模式 (GLOSSARY.md): 显示 > 面板 > 编辑面板布局… opens an edit bar above
// the workbench and puts a named grip on every shown panel; the panels' content takes no pointer
// meanwhile. Dragging a grip moves the panel to any place in either 停靠栏, dragging a divider
// changes two heights and double-clicking it shares them evenly; all of it changes only this
// session's draft. 完成 commits the draft with one SetPanelArrangement (none when it equals the
// committed arrangement), 取消 drops it, 恢复 Java 布局 makes it "no arrangement". A layout mode or
// an enlarged view taking over, a modal dialog, or a window too small for the draft ends the
// session unsaved, with a timed notice. Geometry and fit are arrangement.ts's (arrangedLayout),
// at the workbench height the edit bar leaves; the session is a UI draft and never reaches the core.
// By keyboard: the first grip takes the focus, Tab goes round the grips and the bar, Alt+arrows on a
// grip move its panel (a live region says where to, or why not), Enter there is 完成, Esc is 取消.
import { registerViewHandler, run, state } from "../app";
import { COMMANDS } from "../commands";
import type { ArrangeablePanel, ArrangedPanel, ExtraMode, PanelArrangement } from "../contract/generated";
import { icon } from "../design/icons";
import { os } from "../design/theme";
import { dialogOpen, timedMessage } from "../dialogs";
import { product, registerProduct } from "../i18n";
import { arrangedLayout, equalizeDivider, moveDivider, type ArrangementFallback } from "./arrangement";
import { CARD_INSET, cardLayout, inFrame, SUB_CLEAR, workbenchLayout, type LayoutRequest, type PanelId, type Panels, type Rect } from "./layout";
import { commitArrangement, layoutRequest, placeHandles, type DividerOwner } from "./panels";
import { presentation } from "./presentation";
import "./editLayout.css";

registerProduct({
  ZhCn: {
    editLayout: "编辑面板布局",
    editHint: "拖动面板标题换位置，拖动面板之间的分隔线调高度。",
    editRestoreJava: "恢复 Java 布局",
    editDone: "完成",
    editShowPanel: "显示{0}",
    editGripRole: "可移动的面板",
    editGripHelp: "用 Alt+上下方向键在这一栏里移动这个面板，用 Alt+左右方向键把它移到另一栏。按 Enter 完成，按 Esc 取消。",
    editGripHelpMac: "用 Option+上下方向键在这一栏里移动这个面板，用 Option+左右方向键把它移到另一栏。按 Enter 完成，按 Esc 取消。",
    editMovedLeft: "{0}现在在左栏第 {1} 个，共 {2} 个。",
    editMovedRight: "{0}现在在右栏第 {1} 个，共 {2} 个。",
    editAtTop: "{0}已经在这一栏最上面。",
    editAtBottom: "{0}已经在这一栏最下面。",
    editAlreadyLeft: "{0}已经在左栏。",
    editAlreadyRight: "{0}已经在右栏。",
    editNoFitLeft: "左栏放不下{0}，面板没有移动。放大窗口后再试。",
    editNoFitRight: "右栏放不下{0}，面板没有移动。放大窗口后再试。",
    editNoFitHere: "{0}放到这个位置后放不下，面板没有移动。放大窗口后再试。",
    "panel.Captured": "信息面板",
    "panel.Winrate": "胜率图",
    "panel.Comment": "评论/问题手面板",
    "panel.SubBoard": "小棋盘",
    "panel.VariationGraph": "分支面板",
    "panel.CandidateList": "选点列表面板",
    editUnavailableMode: "切换到默认、经典或精简模式后才能编辑面板布局。",
    editUnavailableEnlarged: "关闭放大小棋盘和放大胜率图后才能编辑面板布局。",
    editUnavailablePortrait: "窗口是竖向的，棋盘两侧放不下面板。把窗口调宽后再试。",
    editUnavailableFit: "窗口太小，放不下面板布局。放大窗口后再试。",
    editExitMode: "布局模式已切换，面板布局的改动没有保存。",
    editExitEnlarged: "已打开放大小棋盘或放大胜率图，面板布局的改动没有保存。",
    editExitDialog: "打开对话框时结束了面板布局编辑，改动没有保存。",
    editExitFit: "窗口放不下面板布局了，面板布局的改动没有保存。",
  },
  ZhTw: {
    editLayout: "編輯面板佈局",
    editHint: "拖動面板標題換位置，拖動面板之間的分隔線調高度。",
    editRestoreJava: "恢復 Java 佈局",
    editDone: "完成",
    editShowPanel: "顯示{0}",
    editGripRole: "可移動的面板",
    editGripHelp: "用 Alt+上下方向鍵在這一欄裡移動這個面板，用 Alt+左右方向鍵把它移到另一欄。按 Enter 完成，按 Esc 取消。",
    editGripHelpMac: "用 Option+上下方向鍵在這一欄裡移動這個面板，用 Option+左右方向鍵把它移到另一欄。按 Enter 完成，按 Esc 取消。",
    editMovedLeft: "{0}現在在左欄第 {1} 個，共 {2} 個。",
    editMovedRight: "{0}現在在右欄第 {1} 個，共 {2} 個。",
    editAtTop: "{0}已經在這一欄最上面。",
    editAtBottom: "{0}已經在這一欄最下面。",
    editAlreadyLeft: "{0}已經在左欄。",
    editAlreadyRight: "{0}已經在右欄。",
    editNoFitLeft: "左欄放不下{0}，面板沒有移動。放大視窗後再試。",
    editNoFitRight: "右欄放不下{0}，面板沒有移動。放大視窗後再試。",
    editNoFitHere: "{0}放到這個位置後放不下，面板沒有移動。放大視窗後再試。",
    "panel.Captured": "資訊面板",
    "panel.Winrate": "勝率圖",
    "panel.Comment": "評論/問題手面板",
    "panel.SubBoard": "小棋盤",
    "panel.VariationGraph": "分支面板",
    "panel.CandidateList": "選點列表面板",
    editUnavailableMode: "切換到預設、經典或精簡模式後才能編輯面板佈局。",
    editUnavailableEnlarged: "關閉放大小棋盤和放大勝率圖後才能編輯面板佈局。",
    editUnavailablePortrait: "視窗是直向的，棋盤兩側放不下面板。把視窗調寬後再試。",
    editUnavailableFit: "視窗太小，放不下面板佈局。放大視窗後再試。",
    editExitMode: "佈局模式已切換，面板佈局的變更沒有儲存。",
    editExitEnlarged: "已開啟放大小棋盤或放大勝率圖，面板佈局的變更沒有儲存。",
    editExitDialog: "開啟對話框時結束了面板佈局編輯，變更沒有儲存。",
    editExitFit: "視窗放不下面板佈局了，面板佈局的變更沒有儲存。",
  },
  EnUs: {
    editLayout: "Edit panel layout",
    editHint: "Drag a panel's title to move it. Drag the line between two panels to change their heights.",
    editRestoreJava: "Restore Java layout",
    editDone: "Done",
    editShowPanel: "Show {0}",
    editGripRole: "movable panel",
    editGripHelp: "Press Alt+Up or Alt+Down to move this panel within its column, and Alt+Left or Alt+Right to move it to the other column. Press Enter to finish, or Esc to cancel.",
    editGripHelpMac: "Press Option+Up or Option+Down to move this panel within its column, and Option+Left or Option+Right to move it to the other column. Press Enter to finish, or Esc to cancel.",
    editMovedLeft: "{0} is now number {1} of {2} in the left column.",
    editMovedRight: "{0} is now number {1} of {2} in the right column.",
    editAtTop: "{0} is already at the top of its column.",
    editAtBottom: "{0} is already at the bottom of its column.",
    editAlreadyLeft: "{0} is already in the left column.",
    editAlreadyRight: "{0} is already in the right column.",
    editNoFitLeft: "The left column has no room for {0} at this window size, so the panel did not move. Make the window larger and try again.",
    editNoFitRight: "The right column has no room for {0} at this window size, so the panel did not move. Make the window larger and try again.",
    editNoFitHere: "{0} does not fit in that position at this window size, so the panel did not move. Make the window larger and try again.",
    "panel.Captured": "Information panel",
    "panel.Winrate": "WinRate graph",
    "panel.Comment": "Comment/blunder panel",
    "panel.SubBoard": "Sub board",
    "panel.VariationGraph": "Variation panel",
    "panel.CandidateList": "Candidates list panel",
    editUnavailableMode: "Switch to Default, Classic or Simple mode to edit the panel layout.",
    editUnavailableEnlarged: "Turn off Large sub board and Large winRate graph to edit the panel layout.",
    editUnavailablePortrait: "The window is taller than it is wide, so there is no room for panels beside the board. Make the window wider and try again.",
    editUnavailableFit: "The window is too small for the panel layout. Make the window larger and try again.",
    editExitMode: "The layout mode changed, so the panel layout changes were not saved.",
    editExitEnlarged: "Large sub board or Large winRate graph was turned on, so the panel layout changes were not saved.",
    editExitDialog: "A dialog opened, so panel layout editing ended without saving the changes.",
    editExitFit: "The window no longer fits the panel layout, so the panel layout changes were not saved.",
  },
  JaJp: {
    editLayout: "パネルのレイアウトを編集",
    editHint: "パネルのタイトルをドラッグすると移動し、パネル間の境界線をドラッグすると高さを変えられます。",
    editRestoreJava: "Java のレイアウトに戻す",
    editDone: "完了",
    editShowPanel: "{0}を表示",
    editGripRole: "移動できるパネル",
    editGripHelp: "Alt+上下の矢印キーでこのパネルを列の中で移動し、Alt+左右の矢印キーでもう一方の列へ移動します。Enter で完了、Esc でキャンセルします。",
    editGripHelpMac: "Option+上下の矢印キーでこのパネルを列の中で移動し、Option+左右の矢印キーでもう一方の列へ移動します。Enter で完了、Esc でキャンセルします。",
    editMovedLeft: "{0}は左の列の {1} 番目になりました（全 {2} 個）。",
    editMovedRight: "{0}は右の列の {1} 番目になりました（全 {2} 個）。",
    editAtTop: "{0}はすでに列のいちばん上にあります。",
    editAtBottom: "{0}はすでに列のいちばん下にあります。",
    editAlreadyLeft: "{0}はすでに左の列にあります。",
    editAlreadyRight: "{0}はすでに右の列にあります。",
    editNoFitLeft: "左の列に{0}を入れる余裕がないため、パネルは移動しませんでした。ウィンドウを大きくしてからもう一度お試しください。",
    editNoFitRight: "右の列に{0}を入れる余裕がないため、パネルは移動しませんでした。ウィンドウを大きくしてからもう一度お試しください。",
    editNoFitHere: "{0}はその位置に収まらないため、パネルは移動しませんでした。ウィンドウを大きくしてからもう一度お試しください。",
    "panel.Captured": "情報パネル",
    "panel.Winrate": "勝率グラフ",
    "panel.Comment": "コメント/問題手パネル",
    "panel.SubBoard": "サブ碁盤",
    "panel.VariationGraph": "バリエーションパネル",
    "panel.CandidateList": "候補手リストパネル",
    editUnavailableMode: "パネルのレイアウトは、デフォルト・クラシック・シンプルモードに切り替えると編集できます。",
    editUnavailableEnlarged: "大きいサブ碁盤と大きい勝率グラフをオフにすると、パネルのレイアウトを編集できます。",
    editUnavailablePortrait: "ウィンドウが縦長のため、碁盤の横にパネルを置けません。ウィンドウを横に広げてからもう一度お試しください。",
    editUnavailableFit: "ウィンドウが小さすぎてパネルのレイアウトが収まりません。ウィンドウを大きくしてからもう一度お試しください。",
    editExitMode: "レイアウトモードが切り替わったため、パネルのレイアウトの変更は保存されませんでした。",
    editExitEnlarged: "大きいサブ碁盤または大きい勝率グラフがオンになったため、パネルのレイアウトの変更は保存されませんでした。",
    editExitDialog: "ダイアログが開いたため、パネルのレイアウトの編集を終了しました。変更は保存されていません。",
    editExitFit: "ウィンドウにパネルのレイアウトが収まらなくなったため、変更は保存されませんでした。",
  },
  KoKr: {
    editLayout: "패널 레이아웃 편집",
    editHint: "패널 제목을 끌어 위치를 바꾸고, 패널 사이의 구분선을 끌어 높이를 조정합니다.",
    editRestoreJava: "Java 레이아웃으로 복원",
    editDone: "완료",
    editShowPanel: "{0} 표시",
    editGripRole: "이동할 수 있는 패널",
    editGripHelp: "Alt+위/아래 화살표로 이 패널을 같은 열 안에서 옮기고, Alt+왼쪽/오른쪽 화살표로 다른 열로 옮깁니다. Enter를 누르면 완료하고 Esc를 누르면 취소합니다.",
    editGripHelpMac: "Option+위/아래 화살표로 이 패널을 같은 열 안에서 옮기고, Option+왼쪽/오른쪽 화살표로 다른 열로 옮깁니다. Enter를 누르면 완료하고 Esc를 누르면 취소합니다.",
    editMovedLeft: "{0} 위치는 이제 왼쪽 열의 {2}개 중 {1}번째입니다.",
    editMovedRight: "{0} 위치는 이제 오른쪽 열의 {2}개 중 {1}번째입니다.",
    editAtTop: "{0} 위치가 이미 이 열의 맨 위입니다.",
    editAtBottom: "{0} 위치가 이미 이 열의 맨 아래입니다.",
    editAlreadyLeft: "{0} 위치가 이미 왼쪽 열입니다.",
    editAlreadyRight: "{0} 위치가 이미 오른쪽 열입니다.",
    editNoFitLeft: "왼쪽 열에 공간이 부족해 {0}을(를) 옮기지 않았습니다. 창을 키운 뒤 다시 시도하세요.",
    editNoFitRight: "오른쪽 열에 공간이 부족해 {0}을(를) 옮기지 않았습니다. 창을 키운 뒤 다시 시도하세요.",
    editNoFitHere: "그 위치에는 {0}이(가) 들어가지 않아 옮기지 않았습니다. 창을 키운 뒤 다시 시도하세요.",
    "panel.Captured": "정보 영역",
    "panel.Winrate": "승률 그래프",
    "panel.Comment": "주석/문제수 패널",
    "panel.SubBoard": "보조바둑판",
    "panel.VariationGraph": "변화도 영역",
    "panel.CandidateList": "후보수 목록창",
    editUnavailableMode: "기본, 클래식 또는 단순 모드로 전환해야 패널 레이아웃을 편집할 수 있습니다.",
    editUnavailableEnlarged: "보조바둑판 크게와 승률그래프 크게를 꺼야 패널 레이아웃을 편집할 수 있습니다.",
    editUnavailablePortrait: "창이 세로로 길어 바둑판 옆에 패널을 둘 수 없습니다. 창을 넓힌 뒤 다시 시도하세요.",
    editUnavailableFit: "창이 너무 작아 패널 레이아웃이 들어가지 않습니다. 창을 키운 뒤 다시 시도하세요.",
    editExitMode: "레이아웃 모드가 바뀌어 패널 레이아웃 변경 사항을 저장하지 않았습니다.",
    editExitEnlarged: "보조바둑판 크게 또는 승률그래프 크게가 켜져 패널 레이아웃 변경 사항을 저장하지 않았습니다.",
    editExitDialog: "대화 상자가 열려 패널 레이아웃 편집을 끝냈습니다. 변경 사항은 저장하지 않았습니다.",
    editExitFit: "창에 패널 레이아웃이 더 이상 들어가지 않아 변경 사항을 저장하지 않았습니다.",
  },
  ThTh: {
    editLayout: "แก้ไขเค้าโครงแผง",
    editHint: "ลากชื่อแผงเพื่อย้ายตำแหน่ง ลากเส้นแบ่งระหว่างแผงเพื่อปรับความสูง",
    editRestoreJava: "คืนค่าเค้าโครง Java",
    editDone: "เสร็จสิ้น",
    editShowPanel: "แสดง{0}",
    editGripRole: "แผงที่ย้ายได้",
    editGripHelp: "กด Alt+ลูกศรขึ้นหรือลงเพื่อย้ายแผงนี้ภายในคอลัมน์ กด Alt+ลูกศรซ้ายหรือขวาเพื่อย้ายไปอีกคอลัมน์ กด Enter เพื่อเสร็จสิ้น หรือกด Esc เพื่อยกเลิก",
    editGripHelpMac: "กด Option+ลูกศรขึ้นหรือลงเพื่อย้ายแผงนี้ภายในคอลัมน์ กด Option+ลูกศรซ้ายหรือขวาเพื่อย้ายไปอีกคอลัมน์ กด Enter เพื่อเสร็จสิ้น หรือกด Esc เพื่อยกเลิก",
    editMovedLeft: "ตอนนี้{0}อยู่ลำดับที่ {1} จาก {2} ในคอลัมน์ซ้าย",
    editMovedRight: "ตอนนี้{0}อยู่ลำดับที่ {1} จาก {2} ในคอลัมน์ขวา",
    editAtTop: "{0}อยู่บนสุดของคอลัมน์นี้แล้ว",
    editAtBottom: "{0}อยู่ล่างสุดของคอลัมน์นี้แล้ว",
    editAlreadyLeft: "{0}อยู่ในคอลัมน์ซ้ายแล้ว",
    editAlreadyRight: "{0}อยู่ในคอลัมน์ขวาแล้ว",
    editNoFitLeft: "คอลัมน์ซ้ายไม่มีที่พอสำหรับ{0} จึงไม่ได้ย้ายแผง ขยายหน้าต่างแล้วลองอีกครั้ง",
    editNoFitRight: "คอลัมน์ขวาไม่มีที่พอสำหรับ{0} จึงไม่ได้ย้ายแผง ขยายหน้าต่างแล้วลองอีกครั้ง",
    editNoFitHere: "{0}ไม่พอดีกับตำแหน่งนั้น จึงไม่ได้ย้ายแผง ขยายหน้าต่างแล้วลองอีกครั้ง",
    "panel.Captured": "แผงข้อมูล",
    "panel.Winrate": "กราฟ WinRate",
    "panel.Comment": "แผงความคิดเห็น/ข้อผิดพลาด",
    "panel.SubBoard": "บอร์ดย่อย",
    "panel.VariationGraph": "แผงรูปแบบ",
    "panel.CandidateList": "แผงรายชื่อผู้สมัคร",
    editUnavailableMode: "สลับไปที่โหมดเริ่มต้น คลาสสิก หรือธรรมดาก่อน จึงจะแก้ไขเค้าโครงแผงได้",
    editUnavailableEnlarged: "ปิดบอร์ดย่อยขนาดใหญ่และกราฟ winRate ขนาดใหญ่ก่อน จึงจะแก้ไขเค้าโครงแผงได้",
    editUnavailablePortrait: "หน้าต่างสูงกว่ากว้าง จึงไม่มีที่วางแผงข้างกระดาน ขยายหน้าต่างให้กว้างขึ้นแล้วลองอีกครั้ง",
    editUnavailableFit: "หน้าต่างเล็กเกินไปสำหรับเค้าโครงแผง ขยายหน้าต่างแล้วลองอีกครั้ง",
    editExitMode: "โหมดเค้าโครงเปลี่ยนไปแล้ว การเปลี่ยนแปลงเค้าโครงแผงจึงไม่ได้บันทึก",
    editExitEnlarged: "เปิดบอร์ดย่อยขนาดใหญ่หรือกราฟ winRate ขนาดใหญ่แล้ว การเปลี่ยนแปลงเค้าโครงแผงจึงไม่ได้บันทึก",
    editExitDialog: "มีกล่องโต้ตอบเปิดขึ้น จึงจบการแก้ไขเค้าโครงแผงโดยไม่บันทึกการเปลี่ยนแปลง",
    editExitFit: "หน้าต่างไม่พอสำหรับเค้าโครงแผงแล้ว การเปลี่ยนแปลงเค้าโครงแผงจึงไม่ได้บันทึก",
  },
});

export type Side = "left" | "right";

/** The six 可排列面板 in the order the Java layout lists them. */
const PANELS: readonly ArrangeablePanel[] = ["Captured", "Winrate", "Comment", "SubBoard", "VariationGraph", "CandidateList"];

/** The layout request's flag of each panel. */
const FLAG = {
  Captured: "showCaptured",
  Winrate: "showWinrateGraph",
  Comment: "showComment",
  SubBoard: "showSubBoard",
  VariationGraph: "showVariationGraph",
  CandidateList: "showListPane",
} as const satisfies Record<ArrangeablePanel, keyof LayoutRequest>;

/** The 显示 > 面板 item that shows each panel. */
const MENU: Record<ArrangeablePanel, string> = {
  Captured: "menu.informationPane",
  Winrate: "menu.winrateGraph",
  Comment: "menu.commitPane",
  SubBoard: "menu.subboard",
  VariationGraph: "menu.variationPane",
  CandidateList: "menu.listPane",
};

/** The workbench panel that tells whether each one is drawn (Panels.shown). */
const DRAWN: Record<ArrangeablePanel, PanelId> = {
  Captured: "captured",
  Winrate: "graph",
  Comment: "comment",
  SubBoard: "sub",
  VariationGraph: "tree",
  CandidateList: "table",
};

/** The menu item's reason when edit mode cannot start, one per fallback. */
const UNAVAILABLE: Record<ArrangementFallback, string> = {
  mode: "editUnavailableMode",
  enlarged: "editUnavailableEnlarged",
  portrait: "editUnavailablePortrait",
  fit: "editUnavailableFit",
};

type ExitReason = "mode" | "enlarged" | "dialog" | "fit";
const EXIT: Record<ExitReason, string> = { mode: "editExitMode", enlarged: "editExitEnlarged", dialog: "editExitDialog", fit: "editExitFit" };
/** How long the notice of an automatic exit shows. */
const EXIT_NOTICE_SECONDS = 5;

export function panelName(panel: ArrangeablePanel): string {
  return product(`panel.${panel}`);
}

const union = (a: Rect, b: Rect): Rect => {
  if (a.w <= 0 || a.h <= 0) return b;
  if (b.w <= 0 || b.h <= 0) return a;
  const x = Math.min(a.x, b.x);
  const y = Math.min(a.y, b.y);
  return { x, y, w: Math.max(a.x + a.w, b.x + b.w) - x, h: Math.max(a.y + a.h, b.y + b.h) - y };
};

/** A panel's rectangle in a layout (the sub board's as the layout gives it). */
function slot(p: Panels, panel: ArrangeablePanel): Rect {
  switch (panel) {
    case "Captured": return p.captured;
    case "Winrate": return union(p.stat, p.graph);
    case "Comment": return p.comment;
    case "SubBoard": return p.sub;
    case "VariationGraph": return p.tree;
    case "CandidateList": return p.table;
  }
}

const same = (a: PanelArrangement | null | undefined, b: PanelArrangement | null | undefined) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null);

// ------------------------------------------------------------------ the arrangement from Java

/** The arrangement the Java layout shows at this size: each panel's 停靠栏 and order where it is,
 *  a hidden one where Java would put it once shown, and each weight its height in pixels, the sub
 *  board's being its section (down to the next panel below it, or its column's foot). With these
 *  weights arrangedLayout puts every panel where the Java layout has it, as long as the Java
 *  layout leaves no gap in a column (a hidden 信息面板 or 胜率图 keeps its room in Java, and a sub
 *  board narrower than its section leaves the rest of the section empty). */
export function initialArrangement(width: number, height: number, r: LayoutRequest, mode: ExtraMode): PanelArrangement {
  const locked = mode === "Normal";
  const now = workbenchLayout(width, height, r, locked);
  const placed = PANELS.map((panel, order) => {
    const p = r[FLAG[panel]] ? now : workbenchLayout(width, height, { ...r, [FLAG[panel]]: true }, locked);
    const rect = slot(p, panel);
    const left = rect.x < p.board.x;
    let h = rect.h;
    if (panel === "SubBoard") {
      const below = PANELS.filter((q) => q !== panel && p.shown[DRAWN[q]])
        .map((q) => slot(p, q))
        .filter((q) => q.h > 0 && q.x < p.board.x === left && q.y >= rect.y + rect.h)
        .map((q) => q.y);
      h = Math.min(left ? p.ponderingY : height, ...below) - rect.y;
    }
    return { side: left ? "left" : "right", y: rect.y, order, entry: { panel, weight: Math.max(1, h) } };
  });
  const column = (side: Side): ArrangedPanel[] =>
    placed.filter((p) => p.side === side).sort((a, b) => a.y - b.y || a.order - b.order).map((p) => p.entry);
  return { left: column("left"), right: column("right") };
}

// ------------------------------------------------------------------ the session

interface Session {
  /** The committed arrangement when the session began. */
  base: PanelArrangement | null;
  /** The arrangement edited; null shows the Java layout (恢复 Java 布局). */
  draft: PanelArrangement | null;
  /** Nothing changed yet. Without a committed arrangement the draft is then the Java layout's
   *  arrangement at the current size (initialArrangement), so it follows a resize or a panel
   *  shown from the edit bar, and 完成 commits nothing. */
  pristine: boolean;
}
let session: Session | null = null;
/** 完成's arrangement, laid out until a published view carries it. */
let sent: { value: PanelArrangement | null } | null = null;

let repaint: () => void = () => {};
/** The grip the keyboard goes to once the overlay is placed next: the first one on entering, a
 *  panel's own once the edit bar's tag chosen by keyboard has shown it. */
let focusNext: ArrangeablePanel | "first" | null = null;

registerViewHandler((v) => {
  if (sent && same(v.settings.panel_arrangement, sent.value)) sent = null;
});

/** Whether 布局编辑模式 is on. */
export function editing(): boolean {
  return session !== null;
}

/** What the last layout in edit mode was made of: the size, the request, the arrangement laid out
 *  (null: the Java layout) and the drawn panels, cards and columns. */
interface Frame {
  W: number;
  H: number;
  request: LayoutRequest;
  mode: ExtraMode;
  arrangement: PanelArrangement | null;
  panels: Panels;
  cards: Rect[];
  columns: Record<Side, Rect>;
  /** The Java layout's panel rectangles at this size (as drawn, in their cards). */
  java: Record<"captured" | "stat" | "graph" | "comment" | "tree" | "table" | "sub", Rect>;
}
let frame: Frame | null = null;

/** Why edit mode cannot start now (null when it can): the arrangement, or the one the Java layout
 *  gives, does not apply or does not fit the workbench the edit bar would leave. */
export function editUnavailable(): ArrangementFallback | null {
  const v = state.view;
  const main = document.getElementById("main");
  if (!v || !main || session) return null;
  const r = layoutRequest(v);
  const W = main.clientWidth;
  const H = main.clientHeight - barHeight(r);
  const committed = sent ? sent.value : v.settings.panel_arrangement;
  const mode = presentation(v).mode;
  return arrangedLayout(W, H, r, mode, committed ?? initialArrangement(W, H, r, mode)).fallback;
}

/** 显示 > 面板 > 编辑面板布局…: opens the session with the committed arrangement as its draft. */
export function enterEdit(): void {
  const v = state.view;
  if (!v || session || editUnavailable()) return;
  const committed = sent ? sent.value : v.settings.panel_arrangement;
  session = { base: committed, draft: committed, pristine: true };
  focusNext = "first";
  syncTags(layoutRequest(v));
  editBar().hidden = false;
  repaint();
}

function end(): void {
  session = null;
  focusNext = null;
  if (live) live.textContent = "";
  frame = null;
  cancelPanelDrag();
  editBar().hidden = true;
  hideOverlay();
  repaint();
}

/** 完成: commits the draft with one SetPanelArrangement, none when it equals the committed one. */
export async function finishEdit(): Promise<void> {
  const s = session;
  if (!s) return;
  const next = s.draft;
  const committed = state.view?.settings.panel_arrangement ?? null;
  end();
  if (same(next, committed)) return;
  sent = { value: next };
  const result = await run({ type: "SetPanelArrangement", arrangement: next });
  if (!result || result.type === "Refused") sent = null;
  repaint();
}

/** 取消: drops every change of the session. */
export function cancelEdit(): void {
  if (session) end();
}

/** 恢复 Java 布局: the draft becomes "no arrangement", previewed at once; 完成 then commits that. */
export function restoreJavaLayout(): void {
  if (!session) return;
  session.draft = null;
  session.pristine = false;
  repaint();
}

function exitEdit(reason: ExitReason): void {
  if (!session) return;
  end();
  timedMessage(product(EXIT[reason]), EXIT_NOTICE_SECONDS);
}

/** The arrangement `next` becomes the draft. */
function setDraft(next: PanelArrangement): void {
  if (!session) return;
  session.draft = next;
  session.pristine = false;
  repaint();
}

/** The arrangement edit moves start from: the draft, or for the Java layout the arrangement it
 *  gives at the size laid out. */
function editBase(f: Frame): PanelArrangement {
  return f.arrangement ?? initialArrangement(f.W, f.H, f.request, f.mode);
}

/** `a` with `panel` moved into `side` before its `k`-th shown panel (other than `panel`; `k` past
 *  the last one puts it after that one), keeping its weight; null when that is where it already is
 *  or the result does not fit this size. Callable without a pointer. */
function moved(f: Frame, a: PanelArrangement, panel: ArrangeablePanel, side: Side, k: number): PanelArrangement | null {
  const entry = [...a.left, ...a.right].find((e) => e.panel === panel);
  if (!entry) return null;
  const shown = (e: ArrangedPanel) => e.panel !== panel && f.request[FLAG[e.panel]];
  const list = a[side].filter((e) => e.panel !== panel);
  const others = list.filter(shown);
  const at = k < others.length ? list.indexOf(others[Math.max(0, k)]) : others.length ? list.indexOf(others[others.length - 1]) + 1 : list.length;
  const column = [...list.slice(0, at), entry, ...list.slice(at)];
  const next = {
    left: side === "left" ? column : a.left.filter((e) => e.panel !== panel),
    right: side === "right" ? column : a.right.filter((e) => e.panel !== panel),
  };
  if (same(next, a)) return null;
  return arrangedLayout(f.W, f.H, f.request, f.mode, next).panels ? next : null;
}

/** Moves `panel` in the draft to `side`, before its `k`-th shown panel other than itself; false
 *  (nothing changes) outside edit mode, where it already is, or where the draft would not fit. */
export function movePanel(panel: ArrangeablePanel, side: Side, k: number): boolean {
  const f = frame;
  if (!session || !f) return false;
  const next = moved(f, editBase(f), panel, side, k);
  if (next) setDraft(next);
  return next !== null;
}

// ------------------------------------------------------------------ layout hooks

/** The arrangement the workbench lays out: in edit mode the draft (the Java layout's arrangement
 *  while nothing changed without a committed one; null for the Java layout), else the committed one
 *  or 完成's until the view carries it. Ends the session first when its layout mode or an enlarged
 *  view takes over or the draft no longer fits; `exited` then asks for another layout. A size
 *  change in edit mode cancels a held drag. */
export function editArrangement(v: { settings: { panel_arrangement: PanelArrangement | null } }, W: number, H: number, r: LayoutRequest, mode: ExtraMode): { arrangement: PanelArrangement | null; exited: boolean } {
  const committed = sent ? sent.value : v.settings.panel_arrangement;
  const s = session;
  if (!s) return { arrangement: committed, exited: false };
  const draft = s.pristine && !s.base ? initialArrangement(W, H, r, mode) : s.draft;
  const fallback = arrangedLayout(W, H, r, mode, draft ?? initialArrangement(W, H, r, mode)).fallback;
  const reason: ExitReason | null = fallback === "mode" || fallback === "enlarged" ? fallback : fallback === "portrait" || (fallback === "fit" && draft) ? "fit" : null;
  if (reason) {
    exitEdit(reason);
    return { arrangement: committed, exited: true };
  }
  if (frame && (frame.W !== W || frame.H !== H)) {
    cancelPanelDrag();
    placeHandles(null, null, null);
  }
  return { arrangement: draft, exited: false };
}

/** Who a divider drag of the arrangement `a` laid out at this size works for: in edit mode the
 *  release becomes the draft, otherwise it is committed (one SetPanelArrangement). */
export function dividerOwner(W: number, H: number, r: LayoutRequest, mode: ExtraMode, a: PanelArrangement): DividerOwner {
  return {
    move: (upper, y) => moveDivider(W, H, r, mode, a, upper, y),
    release: session
      ? async (next) => {
          setDraft(next);
          return false;
        }
      : commitArrangement,
  };
}

/** A divider's two panels share their heights as evenly as they fit (double-click in edit mode). */
function equalize(name: string): void {
  const f = frame;
  const a = f?.arrangement;
  if (!session || !f || !a) return;
  const divider = f.panels.arranged?.dividers.find((d) => d.name === name);
  if (!divider) return;
  const next = equalizeDivider(f.W, f.H, f.request, f.mode, a, divider.upper);
  if (!same(next, a)) setDraft(next);
}

// ------------------------------------------------------------------ the edit bar

let bar: HTMLElement | null = null;
let tags: HTMLElement | null = null;
let tagsFor = "";

function pill(name: string, text: string, run: () => void): HTMLButtonElement {
  const b = document.createElement("button");
  b.type = "button";
  // The name is a class as well, so the focus report tells the controls apart.
  b.className = `tb-pill ${name}`;
  b.dataset.name = name;
  b.textContent = text;
  // As in the toolbars, a click does not take the keyboard from the workbench.
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("click", run);
  return b;
}

function capsule(...items: HTMLElement[]): HTMLElement {
  const cap = document.createElement("span");
  cap.className = "tb-cap";
  cap.append(...items);
  return cap;
}

/** The row above the workbench: what to do, a tag per hidden panel, then 恢复 Java 布局, 取消 and
 *  完成. Built hidden on first use. */
function editBar(): HTMLElement {
  if (bar) return bar;
  bar = document.createElement("div");
  bar.className = "edit-bar";
  bar.dataset.name = "edit-bar";
  bar.setAttribute("role", "group");
  bar.setAttribute("aria-label", product("editLayout"));
  bar.hidden = true;
  const hint = document.createElement("span");
  hint.className = "edit-bar-hint";
  hint.dataset.name = "edit-bar-hint";
  hint.textContent = product("editHint");
  tags = capsule();
  tags.classList.add("edit-bar-tags");
  const actions = document.createElement("span");
  actions.className = "edit-bar-actions";
  actions.append(
    capsule(pill("edit-bar-restore-java", product("editRestoreJava"), restoreJavaLayout)),
    capsule(pill("edit-bar-cancel", product("cancel"), cancelEdit), pill("edit-bar-done", product("editDone"), () => void finishEdit())),
  );
  // The grips' description, and the live region that says where a keyboard move put a panel or
  // why it did not move.
  const help = document.createElement("span");
  help.id = GRIP_HELP_ID;
  help.hidden = true;
  help.textContent = product(os === "mac" ? "editGripHelpMac" : "editGripHelp");
  live = document.createElement("div");
  live.className = "edit-bar-live";
  live.dataset.name = "edit-bar-live";
  live.setAttribute("role", "status");
  live.setAttribute("aria-live", "polite");
  bar.append(hint, tags, actions, help, live);
  // Its buttons keep their own Enter and Space, ahead of the workbench's shortcuts.
  bar.addEventListener("keydown", (e) => {
    const el = e.target as HTMLElement;
    if (!(el instanceof HTMLButtonElement) || e.altKey || e.ctrlKey || e.metaKey || e.shiftKey || (e.key !== "Enter" && e.key !== " ")) return;
    e.stopPropagation();
    e.preventDefault();
    el.click();
  });
  document.getElementById("main")!.before(bar);
  return bar;
}

/** One tag per hidden panel: choosing it is choosing the panel in 显示 > 面板. */
function syncTags(r: LayoutRequest): void {
  editBar();
  const hidden = PANELS.filter((p) => !r[FLAG[p]]);
  const key = hidden.join(",");
  if (key === tagsFor) return;
  tagsFor = key;
  tags!.replaceChildren(...hidden.map((panel) => {
    const b = pill(`edit-bar-show-${panel}`, "", () => {
      if (document.activeElement === b) focusNext = panel;
      COMMANDS[MENU[panel]]?.run();
    });
    const name = document.createElement("span");
    name.textContent = panelName(panel);
    b.append(icon("plus"), name);
    b.setAttribute("aria-label", product("editShowPanel", panelName(panel)));
    b.title = b.getAttribute("aria-label")!;
    return b;
  }));
  tags!.hidden = hidden.length === 0;
}

/** The edit bar's height at the window's width while it is hidden (the room edit mode would take
 *  from the workbench); 0 while it shows, being already outside the workbench. */
function barHeight(r: LayoutRequest): number {
  const b = editBar();
  if (!b.hidden) return 0;
  syncTags(r);
  b.classList.add("measuring");
  b.hidden = false;
  const h = b.offsetHeight;
  b.hidden = true;
  b.classList.remove("measuring");
  return h;
}

// ------------------------------------------------------------------ the overlay

const $ = (id: string) => document.getElementById(id) as HTMLElement;

let layer: HTMLElement | null = null;
const outlines = new Map<ArrangeablePanel, HTMLElement>();
const grips = new Map<ArrangeablePanel, HTMLButtonElement>();
const drops = new Map<Side, HTMLElement>();
let insertLine: HTMLElement | null = null;
let ghost: HTMLElement | null = null;

function place(el: HTMLElement, r: Rect): void {
  Object.assign(el.style, { left: `${r.x}px`, top: `${r.y}px`, width: `${r.w}px`, height: `${r.h}px` });
}

function show(el: HTMLElement | null | undefined, shown: boolean): void {
  if (!el) return;
  const display = shown ? "" : "none";
  if (el.style.display !== display) el.style.display = display;
}

function overlayElement(cls: string, name: string): HTMLElement {
  const el = document.createElement("div");
  el.className = cls;
  el.dataset.name = name;
  $("main").append(el);
  return el;
}

/** The pane over the workbench that takes the pointer from the board and the panels: no move is
 *  played, the variation tree does not scroll, no list row or comment text is picked. */
function editLayer(): HTMLElement {
  if (layer) return layer;
  layer = overlayElement("edit-layer", "panel-edit-layer");
  for (const type of ["mousedown", "contextmenu", "dblclick"]) layer.addEventListener(type, (e) => e.preventDefault());
  layer.addEventListener("wheel", (e) => {
    e.preventDefault();
    e.stopPropagation();
  }, { passive: false });
  return layer;
}

function grip(panel: ArrangeablePanel): HTMLButtonElement {
  let b = grips.get(panel);
  if (b) return b;
  b = document.createElement("button");
  b.type = "button";
  b.className = "panel-grip";
  b.dataset.name = `panel-grip-${panel}`;
  b.setAttribute("aria-roledescription", product("editGripRole"));
  b.setAttribute("aria-describedby", GRIP_HELP_ID);
  const name = document.createElement("span");
  name.textContent = panelName(panel);
  b.append(icon("move"), name);
  b.title = name.textContent;
  b.addEventListener("mousedown", (e) => e.preventDefault());
  b.addEventListener("pointerdown", (e) => pressGrip(panel, b!, e));
  b.addEventListener("pointermove", (e) => moveGrip(e));
  b.addEventListener("pointerup", (e) => {
    if (panelDrag?.pointer === e.pointerId) dropPanel();
  });
  b.addEventListener("pointercancel", cancelPanelDrag);
  b.addEventListener("lostpointercapture", () => {
    if (panelDrag?.grip === b) cancelPanelDrag();
  });
  grips.set(panel, b);
  $("main").append(b);
  return b;
}

function hideOverlay(): void {
  show(layer, false);
  for (const el of [...outlines.values(), ...grips.values(), ...drops.values()]) show(el, false);
  show(insertLine, false);
  show(ghost, false);
}

/** Each shown panel's rectangle as drawn (the sub board's section in its card), left column first,
 *  each top to bottom: the grips' order. */
function shownPanels(P: Panels, cards: Rect[]): { panel: ArrangeablePanel; side: Side; rect: Rect }[] {
  const out = PANELS.flatMap((panel) => {
    let rect = slot(P, panel);
    if (!P.shown[DRAWN[panel]] || rect.w <= 0 || rect.h <= 0) return [];
    if (panel === "SubBoard") {
      const cx = rect.x + rect.w / 2;
      const cy = rect.y + rect.h / 2;
      const card = cards.find((c) => cx >= c.x && cx <= c.x + c.w && cy >= c.y && cy <= c.y + c.h);
      if (card) {
        const top = Math.max(card.y, rect.y - SUB_CLEAR);
        rect = { x: card.x, y: top, w: card.w, h: Math.min(card.y + card.h, rect.y + rect.h + SUB_CLEAR) - top };
      }
    }
    return [{ panel, side: (rect.x < P.board.x ? "left" : "right") as Side, rect }];
  });
  return out.sort((a, b) => (a.side === b.side ? a.rect.y - b.rect.y : a.side === "left" ? -1 : 1));
}

/** Puts the edit overlay over the layout just drawn: the pointer pane, each shown panel's outline
 *  and grip, a dashed drop zone on an empty 停靠栏, and the edit bar's tags. Outside edit mode
 *  nothing shows. A held panel drag ends when the layout under it changed. */
export function placeEdit(P: Panels, cards: Rect[], W: number, H: number, r: LayoutRequest, mode: ExtraMode, arrangement: PanelArrangement | null): void {
  if (!session) return;
  const locked = mode === "Normal";
  const fi = inFrame(W, H, locked ? r : { ...r, leftoverLeftShare: null, commentHeightShare: null, variationGraphShare: null });
  const java = cardLayout(workbenchLayout(W, H, r, locked), H).panels;
  const next: Frame = {
    W,
    H,
    request: r,
    mode,
    arrangement,
    panels: P,
    cards,
    columns: { left: fi.left, right: fi.right },
    java: { captured: java.captured, stat: java.stat, graph: java.graph, comment: java.comment, tree: java.tree, table: java.table, sub: java.sub },
  };
  if (panelDrag && frame && JSON.stringify([frame.W, frame.H, frame.arrangement, frame.request]) !== JSON.stringify([W, H, arrangement, r])) cancelPanelDrag();
  frame = next;
  syncTags(r);
  show(editLayer(), true);
  const shown = shownPanels(P, cards);
  const visible = new Set(shown.map((s) => s.panel));
  for (const panel of PANELS) {
    if (visible.has(panel)) continue;
    show(outlines.get(panel), false);
    show(grips.get(panel), false);
  }
  const focused = document.activeElement;
  let previous: Element | null = null;
  // A grip taller than its panel (a narrow column, a large interface font) pushes the next one in
  // its column down rather than covering it.
  const below: Record<Side, number> = { left: 0, right: 0 };
  for (const { panel, side, rect } of shown) {
    let outline = outlines.get(panel);
    if (!outline) outlines.set(panel, (outline = overlayElement("panel-outline", `panel-outline-${panel}`)));
    place(outline, { x: rect.x + 3, y: rect.y + 3, w: rect.w - 6, h: rect.h - 6 });
    show(outline, true);
    const b = grip(panel);
    show(b, true);
    Object.assign(b.style, { left: `${rect.x + 6}px`, maxWidth: `${Math.max(0, rect.w - 12)}px` });
    const top = Math.max(rect.y + 6, below[side] + 2);
    b.style.top = `${top}px`;
    below[side] = top + b.offsetHeight;
    // Tab follows the grips left column first, each top to bottom; a held grip keeps its place.
    if (!panelDrag && previous && previous.nextElementSibling !== b) previous.after(b);
    previous = b;
  }
  // The keyboard stays on a grip moved in the DOM (a keyboard move takes it out and back in).
  const keep = focusNext === "first" ? shown[0]?.panel : focusNext ?? [...grips].find(([, g]) => g === focused)?.[0];
  if (keep && visible.has(keep)) {
    focusNext = null;
    const g = grips.get(keep)!;
    if (document.activeElement !== g) g.focus({ preventScroll: true });
  }
  for (const side of ["left", "right"] as const) {
    const empty = !shown.some((s) => s.side === side);
    let zone = drops.get(side);
    if (!zone && !empty) continue;
    if (!zone) drops.set(side, (zone = overlayElement("panel-drop", `panel-drop-${side}`)));
    const c = next.columns[side];
    place(zone, { x: c.x + CARD_INSET, y: c.y + CARD_INSET, w: c.w - 2 * CARD_INSET, h: c.h - 2 * CARD_INSET });
    show(zone, empty);
  }
}

// ------------------------------------------------------------------ dragging a panel

interface Target {
  arrangement: PanelArrangement;
  side: Side;
  /** The insertion line, or null for an empty column's drop zone. */
  line: Rect | null;
}

interface PanelDrag {
  panel: ArrangeablePanel;
  grip: HTMLElement;
  pointer: number;
  /** The pointer's offset in the panel's rectangle. */
  grab: { x: number; y: number };
  rect: Rect;
  target: Target | null;
}
let panelDrag: PanelDrag | null = null;

function pointerIn(e: PointerEvent): { x: number; y: number } {
  const m = $("main").getBoundingClientRect();
  return { x: e.clientX - m.left, y: e.clientY - m.top };
}

function pressGrip(panel: ArrangeablePanel, b: HTMLButtonElement, e: PointerEvent): void {
  const f = frame;
  if (!session || !f || panelDrag || e.button !== 0) return;
  const at = shownPanels(f.panels, f.cards).find((s) => s.panel === panel);
  if (!at) return;
  e.preventDefault();
  b.setPointerCapture(e.pointerId);
  const p = pointerIn(e);
  panelDrag = { panel, grip: b, pointer: e.pointerId, grab: { x: p.x - at.rect.x, y: p.y - at.rect.y }, rect: at.rect, target: null };
  b.classList.add("held");
}

/** Where dropping at `p` puts the panel: the column under the pointer, before the first shown
 *  panel whose middle is below it; null over the board, where it already is, or where it would
 *  not fit. */
function targetAt(f: Frame, panel: ArrangeablePanel, p: { x: number; y: number }): Target | null {
  const board = f.panels.board;
  const side: Side | null = p.x < board.x ? "left" : p.x >= board.x + board.w ? "right" : null;
  if (!side) return null;
  const others = shownPanels(f.panels, f.cards).filter((s) => s.side === side && s.panel !== panel);
  const k = others.filter((s) => s.rect.y + s.rect.h / 2 < p.y).length;
  const arrangement = moved(f, editBase(f), panel, side, k);
  if (!arrangement) return null;
  if (!others.length) return { arrangement, side, line: null };
  const first = others[0].rect;
  const y = k < others.length ? others[k].rect.y : others[k - 1].rect.y + others[k - 1].rect.h;
  return { arrangement, side, line: { x: first.x, y: y - 1, w: first.w, h: 2 } };
}

function moveGrip(e: PointerEvent): void {
  const d = panelDrag;
  const f = frame;
  if (!d || !f || d.pointer !== e.pointerId) return;
  const p = pointerIn(e);
  if (!ghost) {
    ghost = overlayElement("panel-ghost", "panel-ghost");
    ghost.append(icon("move"), document.createElement("span"));
  }
  ghost.lastElementChild!.textContent = panelName(d.panel);
  place(ghost, { x: p.x - d.grab.x, y: p.y - d.grab.y, w: d.rect.w, h: d.rect.h });
  show(ghost, true);
  d.target = targetAt(f, d.panel, p);
  if (!insertLine) insertLine = overlayElement("panel-insert", "panel-insert");
  if (d.target?.line) place(insertLine, d.target.line);
  show(insertLine, !!d.target?.line);
  for (const [side, zone] of drops) zone.classList.toggle("target", d.target?.side === side && !d.target.line);
}

/** Ends a held panel drag; the drag it ended, if any. */
function cancelPanelDrag(): PanelDrag | null {
  const d = panelDrag;
  if (!d) return null;
  panelDrag = null;
  d.grip.classList.remove("held");
  if (d.grip.hasPointerCapture(d.pointer)) d.grip.releasePointerCapture(d.pointer);
  show(ghost, false);
  show(insertLine, false);
  for (const zone of drops.values()) zone.classList.remove("target");
  return d;
}

/** Releasing a grip: the panel goes where the insertion line (or the drop zone) showed; elsewhere
 *  it stays where it was. */
function dropPanel(): void {
  const d = cancelPanelDrag();
  if (d?.target) setDraft(d.target.arrangement);
}

// Escape during a pointer drag cancels that drag only.
document.addEventListener("keydown", (e) => {
  if (panelDrag && e.key === "Escape") {
    e.preventDefault();
    e.stopPropagation();
    cancelPanelDrag();
  }
}, true);
window.addEventListener("blur", cancelPanelDrag);

// ------------------------------------------------------------------ the keyboard

const GRIP_HELP_ID = "edit-grip-help";
let live: HTMLElement | null = null;

/** Says `text` in the edit bar's live region; the same sentence twice in a row is said again. */
function announce(text: string): void {
  if (live) live.textContent = live.textContent === text ? `${text}\u00a0` : text;
}

type Step = "up" | "down" | Side;
const STEPS: Record<string, Step> = { ArrowUp: "up", ArrowDown: "down", ArrowLeft: "left", ArrowRight: "right" };

/** Alt+arrow on a grip: the panel one place up or down in its 停靠栏, or into the other one at the
 *  same place where there is one (else at its end); then the live region says where it is now, or
 *  why it did not move. */
function moveByKey(panel: ArrangeablePanel, step: Step): void {
  const f = frame;
  if (!f) return;
  const a = editBase(f);
  const isShown = (e: ArrangedPanel) => f.request[FLAG[e.panel]];
  const side: Side = a.left.some((e) => e.panel === panel) ? "left" : "right";
  const column = a[side].filter(isShown);
  const i = column.findIndex((e) => e.panel === panel);
  const name = panelName(panel);
  if (i < 0) return;
  let to: Side = side;
  let k: number;
  if (step === "up") {
    if (i === 0) return announce(product("editAtTop", name));
    k = i - 1;
  } else if (step === "down") {
    if (i === column.length - 1) return announce(product("editAtBottom", name));
    k = i + 1;
  } else {
    if (step === side) return announce(product(side === "left" ? "editAlreadyLeft" : "editAlreadyRight", name));
    to = step;
    k = Math.min(i, a[to].filter(isShown).length);
  }
  if (!movePanel(panel, to, k)) return announce(product(to === side ? "editNoFitHere" : to === "left" ? "editNoFitLeft" : "editNoFitRight", name));
  const now = session!.draft![to].filter(isShown);
  announce(product(to === "left" ? "editMovedLeft" : "editMovedRight", name, now.findIndex((e) => e.panel === panel) + 1, now.length));
}

/** Edit mode's keys, ahead of the workbench's (main.ts), so Alt+arrows on a grip do not also
 *  browse the record: on a grip Alt+arrows move its panel and Enter is 完成; Esc outside a pointer
 *  drag is 取消 (one in a drag cancels the drag only, above); Tab and Shift+Tab go round the grips
 *  and the edit bar's controls, whose buttons keep their own Enter and Space (editBar). Whether
 *  the key was taken. */
export function handleEditLayoutKey(e: KeyboardEvent): boolean {
  if (!session || panelDrag || e.ctrlKey || e.metaKey) return false;
  if (e.key === "Escape") {
    cancelEdit();
    return true;
  }
  const target = e.target as HTMLElement;
  // Shift+Tab can arrive as another key (X11's ISO_Left_Tab); its code is still Tab.
  if (e.key === "Tab" || e.code === "Tab") {
    // The stops: the shown grips (left column first, each top to bottom), then the bar's controls.
    const all = [...$("main").querySelectorAll<HTMLElement>(":scope > .panel-grip"), ...editBar().querySelectorAll<HTMLElement>("button")]
      .filter((el) => el.getClientRects().length > 0);
    const at = all.indexOf(target);
    if (e.altKey || at < 0) return false;
    all[(at + (e.shiftKey ? all.length - 1 : 1)) % all.length].focus();
    return true;
  }
  const panel = [...grips].find(([, g]) => g === target)?.[0];
  if (!panel || e.shiftKey) return false;
  if (e.key === "Enter" && !e.altKey) {
    void finishEdit();
    return true;
  }
  const step = e.altKey ? STEPS[e.key] : undefined;
  if (!step) return false;
  moveByKey(panel, step);
  return true;
}

// ------------------------------------------------------------------ wiring

Object.assign(COMMANDS, {
  "menu.editPanelLayout": {
    run: enterEdit,
    disabledReason: () => {
      const why = editUnavailable();
      return why && UNAVAILABLE[why];
    },
  },
});

/** Connects edit mode to the workbench: `schedule` lays it out again. A modal dialog (function
 *  search included) ends the session. */
export function bindEditLayout(schedule: () => void): void {
  repaint = schedule;
  const host = $("dialog");
  new MutationObserver(() => {
    if (session && dialogOpen()) exitEdit("dialog");
  }).observe(host, { childList: true, attributes: true, attributeFilter: ["hidden"] });
  $("main").addEventListener("dblclick", (e) => {
    const name = (e.target as HTMLElement).closest<HTMLElement>(".panel-handle.divider")?.dataset.name;
    if (session && name) equalize(name);
  });
}

/** Acceptance probe: whether edit mode is on and its draft, the Java layout's rectangles at this
 *  size, why it cannot start (outside it), and a held panel drag with where dropping would put it. */
export function editReport() {
  return {
    active: session !== null,
    pristine: session?.pristine ?? null,
    /** The arrangement laid out (null: the Java layout). */
    draft: session ? frame?.arrangement ?? null : null,
    java: session ? frame?.java ?? null : null,
    unavailable: session ? null : editUnavailable(),
    drag: panelDrag && { panel: panelDrag.panel, target: panelDrag.target && { side: panelDrag.target.side, line: panelDrag.target.line, arrangement: panelDrag.target.arrangement } },
    /** The edit bar while it shows: its height, and the controls or the sentence whose text does
     *  not fit their box or the window (none when nothing is cut). */
    bar: bar && !bar.hidden ? {
      height: bar.offsetHeight,
      clipped: [...bar.querySelectorAll<HTMLElement>("button, .edit-bar-hint")]
        .filter((el) => el.scrollWidth > el.clientWidth + 1 || el.scrollHeight > el.clientHeight + 1 || el.getBoundingClientRect().right > window.innerWidth)
        .map((el) => el.dataset.name),
    } : null,
    /** The control with the keyboard focus (its data-name, accessible name and description), and
     *  the live region's sentence. */
    focus: session ? (() => {
      const el = document.activeElement as HTMLElement | null;
      const described = el?.getAttribute("aria-describedby");
      return el && {
        name: el.dataset.name ?? null,
        label: el.getAttribute("aria-label") ?? el.textContent?.trim() ?? "",
        description: described ? document.getElementById(described)?.textContent ?? null : null,
      };
    })() : null,
    live: session ? live?.textContent?.trim() || null : null,
  };
}
