#!/usr/bin/env python3
"""Regenerate ui/src/i18n/javaStrings.json from the fixed Java resource bundles.

The UI shows Java's own wording. This script collects every resource key the UI uses (`t("Key")`
calls, `"key"` entries in ui/src/chrome/javaMenus.ts, and any other string literal in ui/src that
names a bundle key, as the tables that pass keys to t() through a helper do) and copies those
entries from the Java DisplayStrings bundles, per locale, falling back to the base bundle like
Java's ResourceBundle. A key the UI still uses that the fixed commit deleted keeps its entry at
PREVIOUS_SHA, the commit the table was taken from before, until the feature using it follows the
fixed commit; the script lists these keys. CORRECTIONS replaces the few Java entries that are
wrong, per locale, and holds the entries whose newer wording describes behaviour this product
does not have yet.

Usage: tools/extract_java_strings.py <java-checkout>
The Java checkout must be wimi321/lizzieyzy-next at 6a5c8d1524f8bdfd9163162a38dcf64c23402a1b
(tag next-2026-11-19.1), with cb53acc55eda42c6c671b5f668cd3b597456bf07 in its history.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXED_SHA = "6a5c8d1524f8bdfd9163162a38dcf64c23402a1b"
PREVIOUS_SHA = "cb53acc55eda42c6c671b5f668cd3b597456bf07"
L10N = "src/main/resources/l10n"
LOCALES = {
    "ZhCn": "DisplayStrings_zh_CN.properties",
    "EnUs": "DisplayStrings_en_US.properties",
    "ZhTw": "DisplayStrings_zh_TW.properties",
    "JaJp": "DisplayStrings_ja_JP.properties",
    "KoKr": "DisplayStrings_ko.properties",
    "ThTh": "DisplayStrings_th_TH.properties",
}

# Java entries this product corrects. 目数显示's items read as the opposite score in English,
# Korean and Thai (Menu.leadWithKomi shows KataGo's lead after komi, Menu.leadJustScore the board
# difference); they follow the Japanese and the Chinese, with the search aliases naming them. The
# Japanese update button was a request, the Yike sync failure was slang in three locales, the
# proxy subtitle named Java, and the Thai score lead labels said "lead" the metal (ตะกั่ว). The
# English and Thai settings title said "Shif+X". Some entries escape a character twice (\\: \\=
# \\! \\" \\uXXXX and a leading "\\ "), which Java reads as a backslash before the character and
# shows; they carry the once-escaped text, the leading one as the space the English entry starts with.
# AI 陪练's 认输 button read as resigning from an office in Japanese and Korean (辞任, 사임); it
# takes the words of the game's own resign button (Menu.resignBtn: 投了, 기권), as Thai already does.
# AI 解说's address note adds that a system or manual 网络代理 receives HTTP requests and the key
# unencrypted, which Java does not say.
#
# The entries marked "held" keep PREVIOUS_SHA's wording where the fixed commit rewrote it for
# behaviour this product does not have yet; each goes when its feature follows the fixed commit.
# 评估此点 keeps naming the reason itself when it is unavailable (Java now keeps the item's name
# and adds the reason in brackets) and its tooltip keeps the conditions this product checks.
CORRECTIONS: dict[str, dict[str, str]] = {
    "ZhCn": {
        "ConfigDialog2.modern.proxy.subtitle": "控制程序自身网络请求使用的代理策略",
        "SetLeelaEngines.lblResign": "放弃胜利率(%,default -1,范围-1到30):",
        "Teacher.settings.apiHint": "填写服务地址和密钥，再选择解说模型。HTTP 不加密，仅用于本机服务或可信局域网。网络代理为手动配置，或系统设置让该地址走代理时，代理能读到 HTTP 请求内容和密钥。",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "选点重点研究不可用（需要本地 KataGo 支持）",
        "RightClickMenu.trackPoint.unavailableTooltip": "需要稳定局面，以及当前前台本地 KataGo 已支持选点重点研究",
        "RightClickMenu.trackPoint.upgradeRequired": "选点评估需要升级引擎",
    },
    "ZhTw": {
        "ConfigDialog2.modern.proxy.subtitle": "選擇來自應用程式的網路請求如何使用代理",
        "SetLeelaEngines.lblResign": "放棄勝利率(%,default -1,範圍-1到30):",
        "Teacher.settings.apiHint": "填寫服務網址與金鑰，再選擇解說模型。HTTP 不加密，僅用於本機服務或可信區域網路。網路代理為手動配置，或系統設定讓該網址經過代理時，代理能讀到 HTTP 請求內容和金鑰。",
        "YikeLiveDialog.syncFailed": "弈客同步失敗",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "選點重點研究不可用（需要本地 KataGo 支援）",
        "RightClickMenu.trackPoint.unavailableTooltip": "需要穩定局面，以及目前前台本地 KataGo 已支援選點重點研究",
        "RightClickMenu.trackPoint.upgradeRequired": "選點評估需要升級引擎",
    },
    "EnUs": {
        "ConfigDialog2.modern.proxy.subtitle": "Choose how network requests from the app use a proxy",
        "FunctionSearch.aliases.menu.kataScolreLead": "Show score lead|Including komi|Excluding komi",
        "FunctionSearch.aliases.settings.general": "comprehensive settings|application preferences|Config (Shift+X)",
        "Menu.comprehensiveSettings": "Config (Shift+X)",
        "Menu.leadJustScore": "Excluding komi",
        "Menu.leadWithKomi": "Including komi",
        "Teacher.settings.apiHint": "Enter the service address and key, then choose a commentary model. HTTP is unencrypted; use it only for local services or trusted private networks. If Network proxy is set to a manual proxy, or to system settings that send this address through a proxy, the proxy can read HTTP requests and the key.",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "Move focus unavailable (local KataGo capability required)",
        "RightClickMenu.trackPoint.unavailableTooltip": "Requires a stable position and the current foreground local KataGo with move-focus capability",
        "RightClickMenu.trackPoint.upgradeRequired": "Point evaluation requires an engine upgrade",
    },
    "JaJp": {
        "ConfigDialog2.modern.proxy.subtitle": "アプリからのネットワーク リクエストでプロキシを使用する方法を選択します",
        "HumanSlGame.btn.resign": "投了",
        "LizzieChangeMove.txtMoveNumber.error": "無効な番号です!",
        "LizzieFrame.saveFileFailed": "保存に失敗しました!",
        "SetLeelaEngines.lblResign": "winRate を辞任 (%,default -1、範囲 -1 ～ 30):",
        "Teacher.settings.apiHint": "サービス URL とキーを入力し、解説モデルを選んでください。HTTP は暗号化されないため、ローカルサービスまたは信頼できるプライベートネットワークでのみ使用してください。ネットワークプロキシが手動構成の場合、またはシステム設定でこのアドレスにプロキシが使われる場合、プロキシは HTTP のリクエストとキーを読み取れます。",
        "WindowsUpdate.btnUpdate": "今すぐ更新",
        "YikeLiveDialog.syncFailed": "弈客の同期に失敗しました",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "move focus は利用不可（ローカル KataGo の対応が必要）",
        "RightClickMenu.trackPoint.unavailableTooltip": "安定した局面と、move focus 対応の現在のフォアグラウンドローカル KataGo が必要です",
        "RightClickMenu.trackPoint.upgradeRequired": "選点評価にはエンジンの更新が必要です",
    },
    "KoKr": {
        "AnalysisFrame.concentration": "집중:",
        "AnalysisFrame.maxVisits": "최대 방문수:",
        "Byoyomi.emptyTimeHint": "시간과 뵤요미는 동시에 없을 수 없습니다!",
        "ConfigDialog2.modern.proxy.subtitle": "앱의 네트워크 요청이 프록시를 사용하는 방법을 선택하세요.",
        "EngineGameInfo.backgroundPonder": " 배경고려:",
        "EngineGameInfo.finishedByDoublePass": " 더블 패스 게임",
        "EngineGameInfo.finishedByMoves": " 이동 횟수 없음",
        "EngineGameInfo.finishedWin": " 승리",
        "EngineGameInfo.firstVisits": " BestMove 방문수:",
        "EngineGameInfo.gameScore": " 점수:",
        "EngineGameInfo.gameWinrate": " 승률:",
        "EngineGameInfo.sgfStartBlackWin": " 검은색 승리",
        "EngineGameInfo.sgfStartWhiteWin": " 흰색 승리",
        "EngineGameInfo.totalVisits": " 총 방문수:",
        "FoxKifuDownload.points": " 포인트",
        "FoxKifuDownload.stones": " 돌",
        "FunctionSearch.aliases.menu.kataScolreLead": "우세 집수 표시|점수 리드 보기|덤 포함|덤 제외",
        "HumanSlGame.btn.resign": "기권",
        "Leelaz.autoAnalyzeComplete": "자동 분석 완료,SGF 저장 위치:",
        "Leelaz.autoAnalyzeStoppedSaved": "자동 분석을 중지했습니다. 현재 SGF 저장 위치:",
        "LizzieChangeMove.txtMoveNumber.error": "잘못된 번호!",
        "LizzieFrame.display.space": " 단축키=Space",
        "LizzieFrame.ifReplaceFile": "파일 바꾸기 \"",
        "LizzieFrame.prompt.showControlsHint": "x = 보기 컨트롤을 누르세요.",
        "LizzieFrame.saveFileFailed": "저장 실패!",
        "LizzieFrame.visits": "방문수:",
        "Menu.leadJustScore": "덤 제외",
        "Menu.leadWithKomi": "덤 포함",
        "Movelistframe.AIscore": " 정확성",
        "Movelistframe.allWinrate": " 전체(승률",
        "Movelistframe.allmoves": " 모든 동작",
        "Movelistframe.analyzed": " 분석됨",
        "Movelistframe.avgDifference": " AvgLoss 승리/점수",
        "Movelistframe.movedAndAverage": " 이동 평균",
        "Movelistframe.movesAnd>": " 이동,>",
        "Movelistframe.movesAndEnd": " 이동) End(<",
        "Movelistframe.movesAndMiddle": " 이동) 중간(<",
        "Movelistframe.movesAndRightBrackets": " 이동)",
        "Movelistframe.scoreAndStdev": " 점수 표준편차",
        "Movelistframe.stdev": " 표준편차",
        "NewAnaGameDialog.noEngineHint": "엔진을 선택해주세요!",
        "ScoreResult.points": " 포인트",
        "SetKataPDA.lblCurPDA": "현재 PDA:",
        "SetKataPDA.lblTip": "팁: 동적 비율이 변경된 후 PDA는 핸디캡에 의해 재설정됩니다.",
        "SetKataRules.lblHasButton": "HasButton:",
        "SetKataRules.lblMultiStoneSuicide": "MultiStoneSuicide:",
        "SetKataRules.lblTaxRule": "세금 규칙:",
        "SetKataRules.lblWhiteHandicapBonus": "화이트핸디캡보너스:",
        "SetLeelaEngines.lblLagbuffer": "GTP 지연 버퍼(기본값 1, 범위 0~3000):",
        "SetLeelaEngines.lblRam": "최대 RAM 제한(MiB, 기본값 2048, 범위 128~131072):",
        "SetLeelaEngines.lblResign": "winRate 사임(%,default -1, 범위 -1 ~ 30):",
        "SetMatchAiPara.lblCalculation": "공식:",
        "Teacher.settings.apiHint": "서비스 주소와 키를 입력한 뒤 해설 모델을 선택하세요. HTTP는 암호화되지 않으므로 로컬 서비스나 신뢰할 수 있는 사설망에서만 사용하세요. 네트워크 프록시를 수동으로 구성했거나 시스템 설정이 이 주소에 프록시를 사용하면, 프록시가 HTTP 요청과 키를 읽을 수 있습니다.",
        "TencentKifuDownload.points": " 포인트",
        "TencentKifuDownload.stones": " 돌",
        "YikeLiveDialog.syncFailed": "Yike 동기화 실패",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "move focus 사용 불가 (로컬 KataGo 지원 필요)",
        "RightClickMenu.trackPoint.unavailableTooltip": "안정된 국면과 move focus를 지원하는 현재 포그라운드 로컬 KataGo가 필요합니다",
        "RightClickMenu.trackPoint.upgradeRequired": "착점 평가에는 엔진 업데이트가 필요합니다",
    },
    "ThTh": {
        "AnalysisFrame.concentration": "ความเข้มข้น:",
        "AnalysisFrame.maxVisits": "การเข้าชมสูงสุด:",
        "AnalysisFrame.totalVisits": "จำนวนการเข้าชมทั้งหมด:",
        "BoardRenderer.pureNetWhiteWinrate": "RawNet: WhiteWinRate",
        "BoardRenderer.pureNetWinrate": "อัตราการชนะของ RawNet:",
        "Byoyomi.emptyTimeHint": "เวลาและเบียโยมิไม่สามารถเป็นได้ในเวลาเดียวกัน!",
        "ChooseMoreEngine.lblchooseStart": "โหลดอัตโนมัติ:",
        "ChooseMoreEngine.title": "เลือกเครื่องยนต์(ดับเบิ้ลคลิก=สวิตช์)",
        "ConfigDialog2.modern.proxy.subtitle": "เลือกวิธีที่คำขอเครือข่ายจากแอปใช้พร็อกซี",
        "ContributeView.lblGameInfos.games": " เกม",
        "EngineGameInfo.backgroundPonder": " พื้นหลังไตร่ตรอง:",
        "EngineGameInfo.continueGame": " เล่นเกมต่อ:",
        "EngineGameInfo.exchange": " แลกเปลี่ยน สีดำ/ขาว:",
        "EngineGameInfo.finishedByDoublePass": " เกมผ่านสองครั้ง",
        "EngineGameInfo.finishedByMoves": " หมดการเคลื่อนไหว",
        "EngineGameInfo.finishedWin": " ชนะ",
        "EngineGameInfo.firstVisits": " BestMove การเข้าชม:",
        "EngineGameInfo.gameScore": " คะแนน:",
        "EngineGameInfo.gameWinrate": " อัตราการชนะ:",
        "EngineGameInfo.komi": " โคมิ:",
        "EngineGameInfo.maxMoves": " การเคลื่อนไหวสูงสุด:",
        "EngineGameInfo.randomPlay1": " สุ่มเล่น: ก่อน",
        "EngineGameInfo.randomPlay2": " การเคลื่อนไหว อัตราการชนะไม่ต่ำกว่าการเคลื่อนไหวที่ดีที่สุด",
        "EngineGameInfo.resignThreshold3": " ย้ายชนะอัตราด้านล่าง",
        "EngineGameInfo.result.totalVisits": " การเข้าชมทั้งหมด:",
        "EngineGameInfo.sgfStartBlackWin": " สีดำชนะ",
        "EngineGameInfo.sgfStartWhiteWin": " ไวท์ ชนะ",
        "EngineGameInfo.totalGames": " เกมแม็กซ์:",
        "EngineGameInfo.totalTime": " เวลาทั้งหมด:",
        "EngineGameInfo.totalVisits": " การเข้าชมทั้งหมด:",
        "EnginePkConfig.chkRandomMove": "สุ่ม:อันดับแรก",
        "EnginePkConfig.chkSatartNum": "StartNum(ค่าเริ่มต้น:1)",
        "EnginePkConfig.lblnameSetting": "ชื่อโฟลเดอร์เกมเป็นกลุ่ม (ครั้งเดียว):",
        "EnginePkConfig.lblresignSettingBlack": "เกณฑ์การลาออกของคนผิวดำ:",
        "EnginePkConfig.lblresignSettingWhite": "เกณฑ์การลาออกสีขาว:",
        "EnginePkConfig.textAreaHint": "เคล็ดลับ:\r\nหากไม่เคลื่อนไหวหรือผ่านทั้งสองอย่าง เกมจะถูกบันทึกไว้ แต่จะไม่ถูกบันทึกเป็นคะแนน\r\nใน GenmoveMode เอ็นจิ้นจะใช้คำสั่ง genmove โปรดเพิ่ม --noponder(leela) ปิดการใช้งานไตร่ตรอง (katago)\r\nใน AnalyzeMode เครื่องยนต์จะใช้คำสั่ง lz-analyze หรือ kata-analyze และวางตำแหน่งที่ดีที่สุด",
        "FirstUseSettings.lblHint": "คำแนะนำ:ใช้ล้อเลื่อนของเมาส์เพื่อควบคุมเกมและรูปแบบไปข้างหน้า/ข้างหลัง",
        "FirstUseSettings.lblNotice": "Noitce:0 หมายถึงไม่จำกัด คุณสามารถเปลี่ยนการตั้งค่าเหล่านี้ได้ในการตั้งค่า Menu-Settings-Initialize",
        "FoxKifuDownload.points": " แต้ม",
        "FoxKifuDownload.stones": " หมาก",
        "FunctionSearch.aliases.menu.kataScolreLead": "แสดงคะแนนนำ|รวมโคมิ|ไม่รวมโคมิ",
        "FunctionSearch.aliases.menu.newHumanSlGame": "โค้ช AI|โค้ช AI",
        "FunctionSearch.aliases.menu.setupBlackToPlay": "ฝ่ายดำเดิน|ดำเดิน",
        "FunctionSearch.aliases.menu.setupClearAll": "ล้างหมากจัดวางทั้งหมด|ล้างหมากตั้งค่าทั้งหมด",
        "FunctionSearch.aliases.menu.setupModeToggle": "โหมดจัดวางหมาก|โหมดตั้งค่าตำแหน่งเริ่มต้น",
        "FunctionSearch.aliases.menu.setupWhiteToPlay": "ฝ่ายขาวเดิน|ขาวเดิน",
        "FunctionSearch.aliases.settings.general": "การตั้งค่าทั้งหมด|การตั้งค่าแอปพลิเคชัน|กำหนดค่า (Shift+X)",
        "Leelaz.autoAnalyzeComplete": "วิเคราะห์อัตโนมัติเสร็จสมบูรณ์ SGF บันทึกใน:",
        "Leelaz.autoAnalyzeStoppedSaved": "หยุดการวิเคราะห์อัตโนมัติแล้ว บันทึก SGF ปัจจุบันไว้ที่:",
        "Leelaz.resign": "ลาออก!",
        "LizzieChangeMove.txtMoveNumber.error": "หมายเลขไม่ถูกต้อง!",
        "LizzieFrame.AIscore": "ความแม่นยำ:",
        "LizzieFrame.captures": "จับกิน:",
        "LizzieFrame.display.space": " ปุ่มลัด=ช่องว่าง",
        "LizzieFrame.ifReplaceFile": "แทนที่ไฟล์ \"",
        "LizzieFrame.mainEngine": "เครื่องยนต์หลัก:",
        "LizzieFrame.prompt.showControlsHint": "กด x = ควบคุมมุมมองค้างไว้",
        "LizzieFrame.replaceFileNotice": "เคล็ดลับ: หากคุณไม่ต้องการแทนที่ไฟล์ ให้ใช้ \"Menu-File-Save as\"",
        "LizzieFrame.saveFileFailed": "บันทึกล้มเหลว!",
        "LizzieFrame.scoreLeadJustScore": "คะแนนนำ:",
        "LizzieFrame.scoreLeadWithKomi": "คะแนนนำ:",
        "LizzieFrame.subEngine": "เครื่องยนต์ย่อย:",
        "LizzieFrame.visits": "การเข้าชม:",
        "LizzieFrame.winrate": "ชนะอัตรา:",
        "Menu.comprehensiveSettings": "กำหนดค่า (Shift+X)",
        "Menu.leadJustScore": "ไม่รวมโคมิ",
        "Menu.leadWithKomi": "รวมโคมิ",
        "MoreEngines.lblName": "ชื่อ:",
        "Movelistframe.AIscore": " ความแม่นยำ",
        "Movelistframe.allWinrate": " ทั้งหมด(winRate",
        "Movelistframe.allmoves": " ทุกการเคลื่อนไหว",
        "Movelistframe.analyzed": " วิเคราะห์แล้ว",
        "Movelistframe.avgDifference": " AvgLoss ชนะ/คะแนน",
        "Movelistframe.movedAndAverage": " เคลื่อนที่โดยเฉลี่ย",
        "Movelistframe.movesAnd>": " ย้าย,>",
        "Movelistframe.movesAndEnd": " ย้าย) สิ้นสุด (<",
        "Movelistframe.movesAndMiddle": " ย้าย) กลาง (<",
        "Movelistframe.movesAndRightBrackets": " ย้าย)",
        "Movelistframe.scoreAndStdev": " คะแนน stdev",
        "Movelistframe.stdev": " stdev",
        "MovenumberDialog.lblChangeTo": "แสดงครั้งสุดท้าย:",
        "NewAnaGameDialog.noEngineHint": "กรุณาเลือกเครื่องยนต์!",
        # Java's Thai bundle names both 差手 and 恶手 ความผิดพลาด; 恶手 reads ตาพลาด, the word Java's
        # coach panel uses for it, and 差手 keeps Java's word.
        "PlayerStrengthEstimate.moveRank.blunder": "ตาพลาด",
        "SGFParse.leadJustScore": "คะแนนนำ:",
        "SGFParse.leadWithKomi": "คะแนนนำ:",
        "SGFParse.seconds": " วินาที",
        "ScoreResult.points": " คะแนน",
        "SetAiTimes.lblAdvTime": "เวลาล่วงหน้า:",
        "SetAiTimes.lblFirstPo": "การเข้าชมครั้งแรก(ไม่บังคับ):",
        "SetAiTimes.lblGenmoveTime": "เวลา:",
        "SetAiTimes.lblKataTime": "KataGo เวลา:",
        "SetAiTimes.lblPo": "การเข้าชม(ไม่บังคับ):",
        "SetAiTimes.lblTime": "เวลา:",
        "SetKataEngines.Hint2": "<html>การแก้ไขอาจ <font color=\"red\">ส่งผลต่อความแข็งแกร่ง</font> คลิก <b><a href=\"https://github.com/lightvector/KataGo/blob/20d34784703c5b4000643d3ccc43bb37d418f3b5/cpp/configs/gtp_example.cfg#L76\"><font color=\"blue\">ที่นี่</font></a></b> เพื่อรับข้อมูลเพิ่มเติม</html>",
        "SetKataEngines.lblHint": "เคล็ดลับ: ต้องใช้ KataGo v1.4 หรือใหม่กว่า",
        "SetKataEngines.lblPDA": "playoutการได้เปรียบสองเท่า (ค่าเริ่มต้น 0.0, ช่วง -3.0 ถึง 3.0):",
        "SetKataEngines.lblWRN": "การวิเคราะห์WideRootNoise (ค่าเริ่มต้น 0.04, ช่วง 0.0 ถึง 2.0):",
        "SetKataPDA.lblCurPDA": "พีดีเอปัจจุบัน:",
        "SetKataPDA.lblTip": "เคล็ดลับ: PDA จะถูกรีเซ็ตโดยแต้มต่อหลังจากการเปลี่ยนแปลงอัตราส่วนไดนามิก",
        "SetKataPDA.lblpdakatagopdapda": "เคล็ดลับ: PDA เป็นพารามิเตอร์ที่ต้องการเล่นแบบรุกหรือตั้งรับ",
        "SetKataRules.lblClassicRules": "กฎคลาสสิก:",
        "SetKataRules.lblHasButton": "มีปุ่ม:",
        "SetKataRules.lblKoRule": "โครูลส์:",
        "SetKataRules.lblMultiStoneSuicide": "มัลติสโตนฆ่าตัวตาย:",
        "SetKataRules.lblScoringRule": "การให้คะแนน:",
        "SetKataRules.lblTaxRule": "กฎภาษี:",
        "SetKataRules.lblWhiteHandicapBonus": "ไวท์แฮนดิแคปโบนัส:",
        "SetLeelaEngines.lblHint": "เคล็ดลับ: ต้องใช้เครื่องยนต์ Leela/Sai",
        "SetLeelaEngines.lblLagbuffer": "GTP lagbuffer (ค่าเริ่มต้น 1 ช่วง 0 ถึง 3000):",
        "SetLeelaEngines.lblRam": "ขีดจำกัด RAM สูงสุด (MiB ค่าเริ่มต้น 2048 ช่วง 128 ถึง 131072):",
        "SetLeelaEngines.lblResign": "ลาออก winRate (%,default -1,ช่วง -1 ถึง 30):",
        "SetLeelaEngines.lblVisits": "ขีดจำกัดการเข้าชมสูงสุด (ช่วง 0 ถึง 1000000000,0 หมายถึงไม่จำกัด):",
        "SetMatchAiPara.lblCalculation": "สูตร:",
        "Teacher.settings.apiHint": "กรอกที่อยู่บริการและคีย์ แล้วเลือกโมเดลอธิบาย HTTP ไม่เข้ารหัส จึงควรใช้กับบริการในเครื่องหรือเครือข่ายส่วนตัวที่เชื่อถือได้เท่านั้น หากพร็อกซีเครือข่ายกำหนดค่าด้วยตนเอง หรือการตั้งค่าระบบให้ที่อยู่นี้ผ่านพร็อกซี พร็อกซีจะอ่านคำขอ HTTP และคีย์ได้",
        "TencentKifuDownload.points": " คะแนน",
        "TencentKifuDownload.stones": " หิน",
        "menu.setFrameSizeRestart": "ตั้งค่าสำเร็จ โปรดรีสตาร์ท Lizzie!",
        # Held (see above).
        "RightClickMenu.trackPoint.requiresCapability": "ใช้ move focus ไม่ได้ (ต้องใช้ KataGo ภายในเครื่องที่รองรับ)",
        "RightClickMenu.trackPoint.unavailableTooltip": "ต้องมีตำแหน่งที่เสถียรและ KataGo ภายในเครื่องที่อยู่เบื้องหน้าและรองรับ move focus",
        "RightClickMenu.trackPoint.upgradeRequired": "การประเมินจุดต้องอัปเกรดเอนจิน",
    },
}


def parse_properties(text: str) -> dict[str, str]:
    """java.util.Properties text format: continuation lines, escapes, \\uXXXX."""
    lines = text.replace("\r\n", "\n").split("\n")
    logical: list[str] = []
    buf = ""
    for line in lines:
        if buf:
            line = buf + line.lstrip()
            buf = ""
        trailing = len(line) - len(line.rstrip("\\"))
        if trailing % 2 == 1:
            buf = line[:-1]
            continue
        logical.append(line)
    out: dict[str, str] = {}
    for line in logical:
        s = line.lstrip()
        if not s or s[0] in "#!":
            continue
        m = re.match(r"((?:[^=:\s\\]|\\.)+)\s*[=:]?\s*(.*)$", s)
        if not m:
            continue
        out[unescape(m.group(1))] = unescape(m.group(2))
    return out


def unescape(s: str) -> str:
    def rep(m: re.Match) -> str:
        c = m.group(1)
        if c.startswith("u"):
            return chr(int(c[1:], 16))
        return {"n": "\n", "t": "\t", "r": "\r", "f": "\f"}.get(c, c)

    return re.sub(r"\\(u[0-9a-fA-F]{4}|.)", rep, s)


def used_keys() -> tuple[set[str], set[str]]:
    """Keys passed to t("...") literally, plus `key: "..."` fields of the UI's data tables
    (menus, toolbars), which reach t() through the table, keys the Rust core writes into records
    through `java_text("...")` and the function catalogue's keys; then the UI's other dotted
    string literals, wanted where they name a bundle key."""
    keys: set[str] = set()
    literals: set[str] = set()
    for ts in (ROOT / "ui/src").rglob("*.ts"):
        text = ts.read_text(encoding="utf-8")
        keys.update(re.findall(r"\bt\(\s*\"([^\"]+)\"", text))
        keys.update(re.findall(r"\"?\bkey\"?:\s*\"([^\"]+)\"", text))
        literals.update(re.findall(r"\"([A-Za-z][\w-]*(?:\.[\w-]+)+)\"", text))
    for rs in (ROOT / "crates").rglob("*.rs"):
        keys.update(re.findall(r"\bjava_text\(\s*[^,()]+,\s*\"([^\"]+)\"", rs.read_text(encoding="utf-8")))
    catalog = json.loads((ROOT / "crates/lizzie-core/src/function_search/catalog.json").read_text(encoding="utf-8"))
    keys.update(catalog["categories"])
    for entry in catalog["entries"]:
        keys.update(entry[field] for field in ("title", "description", "category", "aliases", "weakAliases") if entry[field])
        keys.update(entry["path"])
    return keys, literals


def main() -> None:
    java = Path(sys.argv[1])
    sha = subprocess.run(["git", "-C", str(java), "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip()
    if sha != FIXED_SHA:
        raise SystemExit(f"{java} is at {sha or '?'}, expected {FIXED_SHA}")

    def fixed(file: str) -> dict[str, str]:
        return parse_properties((java / L10N / file).read_text(encoding="utf-8"))

    def previous(file: str) -> dict[str, str]:
        shown = subprocess.run(["git", "-C", str(java), "show", f"{PREVIOUS_SHA}:{L10N}/{file}"],
                               capture_output=True, encoding="utf-8")
        if shown.returncode != 0:
            raise SystemExit(f"{java} cannot show {PREVIOUS_SHA}:{L10N}/{file}: {shown.stderr.strip()}")
        return parse_properties(shown.stdout)

    base = fixed("DisplayStrings.properties")
    keys, literals = used_keys()
    # A function's aliases and description are wanted only while the catalogue lists the function.
    per_function = ("FunctionSearch.aliases.", "FunctionSearch.description.")
    keys.update(key for key in base if key.startswith("FunctionSearch.") and not key.startswith(per_function))
    # Match summaries and notices select Java keys dynamically by side/outcome; AI 解说 statuses,
    # failures and ChatGPT messages arrive from the core and the adapter as Teacher.* keys; Fox
    # direct-connection statuses and failures arrive from the core as Fox.* keys.
    keys.update(key for key in base if key.startswith(("EngineGameInfo.", "EnginePkConfig.",
        "NewEngineGameDialog.", "Mannul.", "Teacher.", "Fox.")))
    keys.add("GtpConsolePane.isEngineGame")
    bundles = {locale: fixed(file) for locale, file in LOCALES.items()}
    for bundle in bundles.values():
        keys.update(key for key in bundle if key.startswith("FunctionSearch.") and not key.startswith(per_function))
    keys.update(key for key in literals if key in base or any(key in bundle for bundle in bundles.values()))
    previous_base = previous("DisplayStrings.properties")
    previous_bundles = {locale: previous(file) for locale, file in LOCALES.items()}
    result: dict[str, dict[str, str]] = {}
    missing: set[str] = set()
    held: set[str] = set()
    for locale, bundle in bundles.items():
        table = {}
        for k in sorted(keys):
            if k in bundle:
                table[k] = bundle[k]
            elif k in base:
                table[k] = base[k]
            elif k in previous_bundles[locale] or k in previous_base:
                table[k] = previous_bundles[locale].get(k, previous_base.get(k))
                held.add(k)
            else:
                missing.add(k)
        table.update(CORRECTIONS.get(locale, {}))
        result[locale] = table
    out = ROOT / "ui/src/i18n/javaStrings.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    source = f"wimi321/lizzieyzy-next {FIXED_SHA} {L10N}; keys deleted there from {PREVIOUS_SHA}"
    out.write_text(json.dumps({"source": source, "strings": result},
                              ensure_ascii=False, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(f"{len(keys)} keys, {len(held)} deleted at {FIXED_SHA[:8]} and taken from {PREVIOUS_SHA[:8]}:")
    for k in sorted(held):
        print("  ", k)
    # Keys that are literal Java text (language names, Chinese-only items) are expected here.
    print(f"{len(missing)} without a bundle entry (shown literally):")
    for k in sorted(missing):
        print("  ", k)


if __name__ == "__main__":
    main()
