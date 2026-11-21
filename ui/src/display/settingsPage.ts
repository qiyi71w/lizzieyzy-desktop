// 综合设置 board display rows (ConfigDialog2 modern cards, createDisplaySection 2955-3356):
// 引擎与分析 > 分析与胜率曲线 (display toggles) / 候选点外观与过滤 / 变化图与 PV 访问,
// 对局与操作 > 显示坐标 / 小棋盘不跟随刷新 and 鼠标交互与坐标格式 > 坐标格式. Like Java they apply
// with 保存设置. Each row carries its ConfigSettingTargets id as `data-setting-target`; rows of
// these cards owned by other settings are passed in by the caller and placed in Java order.
// This product also places here the values Java edited in their own small dialogs (设置最大宽度,
// 设置初始目数差最大值, 设置低计算量阈值, 设置变化图回放间隔, 延迟显示; their menu items open these rows,
// which carry the menu items' catalog ids), two settings Java offered only in 初始化设置 and the
// menus (目数显示, 鼠标悬停显示变化图时不刷新) and the button that opens 自定义信息顺序.
import { run, state } from "../app";
import type { CoordinateStyle, DisplaySettings, PvVisitsMode } from "../contract/generated";
import { product, registerProduct, t } from "../i18n";
import { groupTitle, input, row, switchBox, switchRow } from "../engine/controls";
import { javaDouble, parseJavaDouble } from "../workbench/paint";
import { panelDraft } from "../workbench/panelDraft";
import { decimalOnly, fieldValue, openInfoOrderDialog, parseJavaInt } from "./dialogs";
import { delayCandidates, setCandidates } from "./state";
import { clearTreeTooWide } from "./variationTree";

registerProduct({
  ZhCn: {
    settingsTreeWidth: "分支面板最大宽度",
    settingsTreeWidthSub: "分支面板用滚动条时，变化树宽过此值就改为整体显示；卡顿时调低",
    settingsScoreScale: "目数曲线初始范围",
    settingsScoreScaleSub: "胜率图上目数曲线纵轴的初始上下限，目数差更大时自动放大",
    settingsNextMoveVisits: "下一手提示低计算量阈值",
    settingsNextMoveVisitsSub: "“圈+胜率/目数”提示中，计算量低于此值的选点算作计算量低",
    settingsReplayInterval: "变化图回放间隔",
    settingsReplayIntervalSub: "按 R 回放选点变化图、自动播放变化图时每一手的间隔",
    settingsScoreDisplaySub: "选点、状态栏等处的目数差都按此显示",
    settingsFreezeVariationSub: "鼠标停在选点上时，变化图停在当时的计算结果，移开再移回才更新",
    unitPixels: "像素",
    unitMilliseconds: "毫秒",
    unitPoints: "目",
  },
  ZhTw: {
    settingsTreeWidth: "分支面板最大寬度",
    settingsTreeWidthSub: "分支面板用捲軸時，變化樹寬過此值就改為整體顯示；卡頓時調低",
    settingsScoreScale: "目數曲線初始範圍",
    settingsScoreScaleSub: "勝率圖上目數曲線縱軸的初始上下限，目數差更大時自動放大",
    settingsNextMoveVisits: "下一手提示低計算量閾值",
    settingsNextMoveVisitsSub: "「圈+勝率/目數」提示中，計算量低於此值的選點算作計算量低",
    settingsReplayInterval: "變化圖回放間隔",
    settingsReplayIntervalSub: "按 R 回放選點變化圖、自動播放變化圖時每一手的間隔",
    settingsScoreDisplaySub: "選點、狀態列等處的目數差都按此顯示",
    settingsFreezeVariationSub: "滑鼠停在選點上時，變化圖停在當時的計算結果，移開再移回才更新",
    unitPixels: "像素",
    unitMilliseconds: "毫秒",
    unitPoints: "目",
  },
  EnUs: {
    settingsTreeWidth: "Variation tree max width",
    settingsTreeWidthSub: "With the scroll bar on, a tree wider than this is shown whole instead; lower it if the tree lags",
    settingsScoreScale: "Initial score-lead range",
    settingsScoreScaleSub: "The score-lead line's starting vertical range on the win-rate graph; it widens when a lead is larger",
    settingsNextMoveVisits: "Next-move hint low-visits threshold",
    settingsNextMoveVisitsSub: "In the circle with win rate / score hint, a candidate below these visits counts as low visits",
    settingsReplayInterval: "Variation replay interval",
    settingsReplayIntervalSub: "Time between moves when R replays a candidate's variation or autoplay replays a variation",
    settingsScoreDisplaySub: "Score leads on candidates, the status bar and elsewhere follow this",
    settingsFreezeVariationSub: "While the pointer rests on a candidate, its variation keeps the result from that moment until the pointer leaves and returns",
    unitPixels: "px",
    unitMilliseconds: "ms",
    unitPoints: "points",
  },
  JaJp: {
    settingsTreeWidth: "分岐ツリーの最大幅",
    settingsTreeWidthSub: "スクロールバー使用時、この幅を超えるツリーは全体表示に切り替わります。重い場合は下げてください",
    settingsScoreScale: "目差曲線の初期範囲",
    settingsScoreScaleSub: "勝率グラフの目差曲線の縦軸の初期範囲。目差がこれより大きいと自動で広がります",
    settingsNextMoveVisits: "次の着手ヒントの低訪問数しきい値",
    settingsNextMoveVisitsSub: "差異表示のヒントで、訪問数がこの値未満の候補手を低い訪問数として扱います",
    settingsReplayInterval: "変化図の再生間隔",
    settingsReplayIntervalSub: "R で候補手の変化図を再生するときや、自動再生で変化図を再生するときの一手ごとの間隔",
    settingsScoreDisplaySub: "候補手やステータスバーなどの目差がすべてこの表示になります",
    settingsFreezeVariationSub: "候補手にマウスを置いている間は、その時点の結果のまま変化図を固定し、離して戻すと更新します",
    unitPixels: "ピクセル",
    unitMilliseconds: "ミリ秒",
    unitPoints: "目",
  },
  KoKr: {
    settingsTreeWidth: "분기 트리 최대 너비",
    settingsTreeWidthSub: "스크롤바를 쓸 때 이 너비를 넘는 트리는 전체 보기로 바뀝니다. 느려지면 낮추세요",
    settingsScoreScale: "집 차이 곡선 초기 범위",
    settingsScoreScaleSub: "승률 그래프에서 집 차이 곡선 세로축의 초기 범위이며, 집 차이가 더 크면 자동으로 넓어집니다",
    settingsNextMoveVisits: "다음수 표시 낮은 비짓 기준",
    settingsNextMoveVisitsSub: "승률/점수 차이 원 표시에서 비짓이 이 값보다 적은 후보수를 낮은 비짓으로 봅니다",
    settingsReplayInterval: "변화도 재생 간격",
    settingsReplayIntervalSub: "R로 후보수 변화도를 재생하거나 자동 재생으로 변화도를 재생할 때 수 사이 간격",
    settingsScoreDisplaySub: "후보수, 상태 표시줄 등의 집 차이가 모두 이 방식으로 표시됩니다",
    settingsFreezeVariationSub: "후보수에 마우스를 올려 둔 동안 그 순간의 결과로 변화도를 고정하고, 마우스를 뗐다가 다시 올리면 갱신합니다",
    unitPixels: "픽셀",
    unitMilliseconds: "밀리초",
    unitPoints: "집",
  },
  ThTh: {
    settingsTreeWidth: "ความกว้างสูงสุดของแผนผังแยก",
    settingsTreeWidthSub: "เมื่อใช้แถบเลื่อน แผนผังที่กว้างเกินค่านี้จะแสดงทั้งหมดแทน ลดค่าลงหากรู้สึกช้า",
    settingsScoreScale: "ช่วงเริ่มต้นของเส้นแต้มนำ",
    settingsScoreScaleSub: "ช่วงแกนตั้งเริ่มต้นของเส้นแต้มนำบนกราฟอัตราชนะ จะขยายเองเมื่อแต้มนำมากกว่านี้",
    settingsNextMoveVisits: "เกณฑ์การเข้าชมต่ำของคำใบ้ตาถัดไป",
    settingsNextMoveVisitsSub: "ในคำใบ้วงกลมพร้อมอัตราชนะ ตัวเลือกที่เข้าชมน้อยกว่าค่านี้ถือว่าเข้าชมต่ำ",
    settingsReplayInterval: "ช่วงเวลาเล่นรูปแบบซ้ำ",
    settingsReplayIntervalSub: "ระยะห่างระหว่างแต่ละตาเมื่อกด R เล่นรูปแบบของตัวเลือก หรือเมื่อเล่นรูปแบบอัตโนมัติ",
    settingsScoreDisplaySub: "แต้มนำบนตัวเลือก แถบสถานะ และส่วนอื่น จะแสดงตามนี้ทั้งหมด",
    settingsFreezeVariationSub: "ขณะวางเมาส์บนตัวเลือก รูปแบบจะคงผลการคำนวณ ณ ตอนนั้นไว้ จนกว่าจะเลื่อนออกแล้วกลับมาใหม่",
    unitPixels: "พิกเซล",
    unitMilliseconds: "มิลลิวินาที",
    unitPoints: "แต้ม",
  },
});

registerProduct({
  ZhCn: {
    settingsDelayCandidates: "延迟显示选点",
    settingsDelayCandidatesSub: "每到一个新局面先隐藏选点，过这么多秒再显示；0 或负数则一直隐藏，按 F 显示",
    settingsInfoOrderSub: "选点圆圈里显示胜率、计算量、目数中的哪几项，以及从上到下的顺序",
  },
  ZhTw: {
    settingsDelayCandidates: "延遲顯示選點",
    settingsDelayCandidatesSub: "每到一個新局面先隱藏選點，過這麼多秒再顯示；0 或負數則一直隱藏，按 F 顯示",
    settingsInfoOrderSub: "選點圓圈裡顯示勝率、計算量、目數中的哪幾項，以及由上到下的順序",
  },
  EnUs: {
    settingsDelayCandidates: "Delay candidates",
    settingsDelayCandidatesSub: "On each new position, candidates stay hidden for this many seconds; 0 or less keeps them hidden until you press F",
    settingsInfoOrderSub: "Which of win rate, visits and score lead a candidate circle shows, and their order from top to bottom",
  },
  JaJp: {
    settingsDelayCandidates: "候補手の表示を遅らせる",
    settingsDelayCandidatesSub: "新しい局面ごとに候補手をこの秒数だけ隠します。0 以下なら F を押すまで隠したままです",
    settingsInfoOrderSub: "候補手の円に勝率・訪問数・目差のどれを表示するかと、上からの順序",
  },
  KoKr: {
    settingsDelayCandidates: "후보수 표시 지연",
    settingsDelayCandidatesSub: "새 국면마다 후보수를 이 시간(초) 동안 숨깁니다. 0 이하이면 F를 누를 때까지 숨깁니다",
    settingsInfoOrderSub: "후보수 원에 승률, 방문 수, 집 차이 중 무엇을 어떤 순서로 표시할지 정합니다",
  },
  ThTh: {
    settingsDelayCandidates: "หน่วงการแสดงตัวเลือก",
    settingsDelayCandidatesSub: "ทุกตำแหน่งใหม่จะซ่อนตัวเลือกไว้ตามจำนวนวินาทีนี้ ถ้าเป็น 0 หรือติดลบจะซ่อนไว้จนกว่าจะกด F",
    settingsInfoOrderSub: "เลือกว่าวงกลมตัวเลือกจะแสดงอัตราชนะ จำนวนการเข้าชม หรือแต้มนำ และเรียงจากบนลงล่างอย่างไร",
  },
});

function card(host: HTMLElement, title: string, subtitle: string): void {
  groupTitle(host, title, subtitle);
}

/** A switch row with its description under the name. */
function check(host: HTMLElement, target: string, key: keyof DisplaySettings, title: string, sub: string, value: boolean): HTMLInputElement {
  const r = switchRow(host, title, sub, value);
  r.input.dataset.key = `settings.display.${key}`;
  r.row.dataset.settingTarget = target;
  return r.input;
}

/** A combo box row with its description under the name; `items` pairs each value with its text. */
function combo<T extends string | number>(host: HTMLElement, target: string, key: string, title: string, sub: string, items: [T, string][], value: T): () => T {
  const select = document.createElement("select");
  select.dataset.key = `settings.display.${key}`;
  select.title = sub;
  items.forEach(([, text], k) => {
    const option = document.createElement("option");
    option.value = String(k);
    option.textContent = text;
    select.append(option);
  });
  select.value = String(Math.max(0, items.findIndex(([v]) => v === value)));
  row(host, title, sub, select).dataset.settingTarget = target;
  return () => items[Number(select.value)][0];
}

/** JFormattedTextField with DigitOnlyFilter (`pattern` removes the rest; null keeps every
 *  character) and a unit after it. */
function text(host: HTMLElement, target: string, key: keyof DisplaySettings, title: string, sub: string, value: string, unit: string, pattern: RegExp | null): HTMLInputElement {
  const control = input(value);
  control.dataset.key = `settings.display.${key}`;
  control.inputMode = "numeric";
  control.title = sub;
  control.setAttribute("aria-label", title);
  if (pattern) {
    control.addEventListener("input", () => {
      const kept = control.value.replace(pattern, "");
      if (kept !== control.value) control.value = kept;
    });
  }
  row(host, title, sub, control, unit).dataset.settingTarget = target;
  return control;
}

/** Utils.txtFieldDoubleValue / txtFieldIntValue: an unparsable field reads as 0. */
function fieldNumber(control: HTMLInputElement, integer: boolean): number {
  const v = integer ? Number.parseInt(control.value, 10) : Number.parseFloat(control.value);
  return Number.isFinite(v) ? v : 0;
}

/** The rows taken over from Java's small dialogs: a field that does not read as a Java int keeps
 *  the saved value, as those dialogs did (parseTextToInt, or their error message). */
function intOr(control: HTMLInputElement, saved: number): number {
  return parseJavaInt(control.value.trim()) ?? saved;
}

/** Rows of the shared cards that other settings own, inserted where Java places them. */
export interface SharedCardRows {
  /** After the 分析与胜率曲线 display toggles: the analysis limits and 实况分析限制. */
  analysis: Node;
  /** First rows of 对局与操作, before 显示坐标. */
  play: Node;
  /** First rows of 鼠标交互与坐标格式, before 坐标格式. */
  interaction: Node;
}

/** Adds the rows to the 引擎与分析 (`engine`) and 对局与操作 (`play`) pages; `save` sends the
 *  edited display settings with 保存设置 and is false when the core refused them. `fields` are
 *  the rows whose validity 保存设置 checks before anything saves. */
export function buildDisplaySettings(engine: HTMLElement, play: HTMLElement, shared: SharedCardRows): { save: () => Promise<boolean>; fields: HTMLInputElement[] } {
  const s = state.view!.settings.display;

  // 分析与胜率曲线 (ConfigDialog2.java:2994-3062, 6534-6553, 6739, 6780, 6829); the 胜率曲线 and
  // 分支面板 visibility rows edit the dialog's panel draft (panelDraft.ts).
  card(engine, t("ConfigDialog2.modern.analysis.title"), t("ConfigDialog2.modern.analysis.subtitle"));
  panelDraft().row(engine, "showWinrateGraph", "config.analysis.winrate", t("ConfigDialog2.modern.analysis.winrate"), t("ConfigDialog2.modern.analysis.winrateSub"));
  const blackWinrate = check(engine, "settings.black-winrate", "winrateAlwaysBlack", t("Menu.alwaysShowBlackWinrate"), t("FunctionSearch.description.blackWinrate"), s.winrateAlwaysBlack);
  panelDraft().row(engine, "showVariationGraph", "config.analysis.variation", t("ConfigDialog2.modern.analysis.variation"), t("ConfigDialog2.modern.analysis.variationSub"));
  // 设置最大宽度 (Menu.java:1223-1277): Java enables its menu item only while the tree scrolls.
  const treeWidth = text(engine, "menu.maxTreeWidth", "maxTreeWidth", product("settingsTreeWidth"), product("settingsTreeWidthSub"), String(s.maxTreeWidth), product("unitPixels"), /[^0-9]+/g);
  treeWidth.disabled = !s.scrollVariation;
  const blunderBar = check(engine, "config.analysis.blunder-bar", "blunderBar", t("ConfigDialog2.modern.analysis.blunderBar"), t("ConfigDialog2.modern.analysis.blunderBarSub"), s.blunderBar);
  const hover = check(engine, "config.analysis.hover", "mouseOverWinrateGraph", t("ConfigDialog2.modern.analysis.hover"), t("ConfigDialog2.modern.analysis.hoverSub"), s.mouseOverWinrateGraph);
  const graphFill = check(engine, "config.analysis.graph-fill", "winrateGraphFill", t("ConfigDialog2.modern.analysis.graphFill"), t("ConfigDialog2.modern.analysis.graphFillSub"), s.winrateGraphFill);
  // 设置初始目数差最大值 (Menu.java:1626-1659): the field reads as Double.parseDouble and the
  // absolute value is saved; another text makes 保存设置 report Menu.inputDoubleTip on the field.
  const scoreScale = text(engine, "menu.initialMaxScoreLead", "initialMaxScoreLead", product("settingsScoreScale"), product("settingsScoreScaleSub"), javaDouble(Math.abs(s.initialMaxScoreLead)), product("unitPoints"), null);
  scoreScale.inputMode = "decimal";
  scoreScale.addEventListener("input", () => {
    scoreScale.setCustomValidity(parseJavaDouble(scoreScale.value) === null ? t("Menu.inputDoubleTip") : "");
    scoreScale.removeAttribute("aria-invalid");
  });
  scoreScale.addEventListener("invalid", () => scoreScale.setAttribute("aria-invalid", "true"));
  const maxRed = check(engine, "config.analysis.max-red", "suggestionMaxRed", t("ConfigDialog2.modern.analysis.maxRed"), t("ConfigDialog2.modern.analysis.maxRedSub"), s.suggestionMaxRed);
  engine.append(shared.analysis);

  // 候选点外观与过滤 (ConfigDialog2.java:797-814, 970-1027, 6731-6805)
  card(engine, t("ConfigDialog2.modern.candidates.title"), t("ConfigDialog2.modern.candidates.subtitle"));
  const blueRing = check(engine, "config.candidates.blue-ring", "blueRing", t("ConfigDialog2.modern.candidates.blueRing"), t("ConfigDialog2.modern.candidates.blueRingSub"), s.blueRing);
  const colorRatio = combo(engine, "config.candidates.color-ratio", "suggestionColorRatio", t("ConfigDialog2.modern.candidates.colorRatio"), t("ConfigDialog2.modern.candidates.colorRatioSub"), [
    [1, t("LizzieConfig.SuggestionMoveColorConcentration1")],
    [2, t("LizzieConfig.SuggestionMoveColorConcentration2")],
    [3, t("LizzieConfig.SuggestionMoveColorConcentration3")],
  ], s.suggestionColorRatio);
  // The combo of white-suggestion-white x white-suggestion-order-white.
  const whiteStyle = combo(engine, "config.candidates.white-style", "whiteSuggestion", t("ConfigDialog2.modern.candidates.whiteStyle"), t("ConfigDialog2.modern.candidates.whiteStyleSub"), [
    [0, t("LizzieConfig.chkShowWhiteSuggWhite1")],
    [1, t("LizzieConfig.chkShowWhiteSuggWhite2")],
    [2, t("LizzieConfig.chkShowWhiteSuggWhite3")],
    [3, t("LizzieConfig.chkShowWhiteSuggWhite4")],
  ], (s.whiteSuggestionWhite ? 1 : 0) + (s.whiteSuggestionOrderWhite ? 2 : 0));
  const noSuggCircle = check(engine, "config.candidates.no-suggestion-circle", "noSuggCircle", t("ConfigDialog2.modern.candidates.noSuggCircle"), t("ConfigDialog2.modern.candidates.noSuggCircleSub"), s.noSuggCircle);
  const minRatio = text(engine, "config.candidates.minimum-playout-ratio", "minPlayoutRatio", t("ConfigDialog2.modern.candidates.minPlayoutRatio"), t("ConfigDialog2.modern.candidates.minPlayoutRatioSub"), javaDouble(s.minPlayoutRatio * 100), t("ConfigDialog2.modern.unit.percent"), /[^0-9.]+/g);
  // 显示 > KataGo设置 > 目数显示, in the menu's order: 计算贴目后目数差 turns show-katago-score-lead-with-komi off.
  const scoreDisplay = combo<number>(engine, "config.candidates.score-lead", "scoreLeadWithKomi", t("Menu.kataScolreLead"), product("settingsScoreDisplaySub"), [
    [0, t("Menu.leadWithKomi")],
    [1, t("Menu.leadJustScore")],
  ], s.scoreLeadWithKomi ? 1 : 0);
  // 设置低计算量阈值 (Menu.java:690-723): any Java int, negative ones too.
  const nextMoveVisits = text(engine, "menu.minPlayoutsForNextMove", "nextMoveMinPlayouts", product("settingsNextMoveVisits"), product("settingsNextMoveVisitsSub"), String(s.nextMoveMinPlayouts), t("ConfigDialog2.modern.unit.visits"), /[^0-9-]+/g);
  // 推荐点 > 延迟显示 (SetDelayShowCandidates): the switch, then the seconds it enables, which end
  // on the line of the other fields.
  const delayOn = switchBox(s.delayCandidates);
  delayOn.dataset.key = "settings.display.delayCandidates";
  delayOn.setAttribute("aria-label", product("settingsDelayCandidates"));
  const delaySeconds = input(javaDouble(s.delayCandidatesSeconds));
  delaySeconds.dataset.key = "settings.display.delayCandidatesSeconds";
  delaySeconds.inputMode = "decimal";
  delaySeconds.setAttribute("aria-label", product("settingsDelayCandidates"));
  delaySeconds.title = product("settingsDelayCandidatesSub");
  delaySeconds.disabled = !s.delayCandidates;
  decimalOnly(delaySeconds);
  delayOn.onchange = () => (delaySeconds.disabled = !delayOn.checked);
  row(engine, product("settingsDelayCandidates"), product("settingsDelayCandidatesSub"), delayOn, delaySeconds, t("ConfigDialog2.modern.unit.seconds")).dataset.settingTarget = "menu.setCandidatesDelay";
  // 推荐点 > 自定义顺序 (SuggestionInfoOrderSettings): its own dialog, saved by its own 确定.
  const infoOrder = document.createElement("button");
  infoOrder.type = "button";
  infoOrder.textContent = t("FirstUseSettings.btnCustomOrder");
  infoOrder.dataset.key = "settings.display.infoOrder";
  infoOrder.onclick = () => openInfoOrderDialog();
  row(engine, t("FirstUseSettings.lblSuggestionInfo"), product("settingsInfoOrderSub"), infoOrder);

  // 变化图与 PV 访问 (ConfigDialog2.java:1606-1630, 1945-1951, 6565-6583)
  card(engine, t("ConfigDialog2.modern.pv.title"), t("ConfigDialog2.modern.pv.subtitle"));
  const pvMode = combo<PvVisitsMode>(engine, "config.pv.mode", "pvVisits", t("ConfigDialog2.modern.pv.mode"), t("ConfigDialog2.modern.pv.modeSub"), [
    ["Off", t("FirstUseSettings.rdoNoPvVistits")],
    ["LastMove", t("FirstUseSettings.rdoLastPvVistits")],
    ["AllMoves", t("FirstUseSettings.rdoEveryPvVistits")],
  ], s.pvVisits);
  const pvLimit = text(engine, "config.pv.visit-limit", "pvVisitsLimit", t("ConfigDialog2.modern.pv.limit"), t("ConfigDialog2.modern.pv.limitSub"), String(s.pvVisitsLimit), t("ConfigDialog2.modern.unit.visits"), /[^0-9]+/g);
  const removeDead = check(engine, "config.pv.remove-dead", "removeDeadInVariation", t("ConfigDialog2.modern.pv.removeDead"), t("ConfigDialog2.modern.pv.removeDeadSub"), s.removeDeadInVariation);
  // 设置变化图回放间隔 (SetReplayTime): milliseconds.
  const replay = text(engine, "menu.setReplayInterval", "replayIntervalSeconds", product("settingsReplayInterval"), product("settingsReplayIntervalSub"), String(Math.trunc(s.replayIntervalSeconds * 1000)), product("unitMilliseconds"), /[^0-9]+/g);
  let replayEdited = false;
  replay.addEventListener("input", () => (replayEdited = true));

  // 对局与操作 (ConfigDialog2.java:3152-3204)
  card(play, t("ConfigDialog2.modern.play.title"), t("ConfigDialog2.modern.play.subtitle"));
  play.append(shared.play);
  const coordinates = check(play, "config.play.coordinates", "coordinates", t("ConfigDialog2.modern.play.coordinates"), t("ConfigDialog2.modern.play.coordinatesSub"), s.coordinates);
  const freezeSub = check(play, "config.play.freeze-sub-board", "noRefreshOnSub", t("ConfigDialog2.modern.play.freezeSubBoard"), t("ConfigDialog2.modern.play.freezeSubBoardSub"), s.noRefreshOnSub);
  // 鼠标悬停显示变化图时,变化图不刷新 (the 推荐点 menu's check and 初始化设置's first row).
  const freezeVariation = check(play, "config.play.freeze-variation", "noRefreshOnMouse", t("Menu.noRefreshOnMouse"), product("settingsFreezeVariationSub"), s.noRefreshOnMouse);
  // 鼠标交互与坐标格式 (ConfigDialog2.java:3205-3228); 坐标格式 1721-1734, 6520-6531
  card(play, t("ConfigDialog2.modern.interaction.title"), t("ConfigDialog2.modern.interaction.subtitle"));
  play.append(shared.interaction);
  const coords = combo<CoordinateStyle>(play, "config.interaction.coordinates", "coordinateStyle", t("ConfigDialog2.modern.interaction.specialCoords"), t("ConfigDialog2.modern.interaction.specialCoordsSub"), [
    ["Normal", t("ConfigDialog2.SpecialCoordsNormal")],
    ["WithI", t("ConfigDialog2.SpecialCoordsWithI")],
    ["Fox", t("ConfigDialog2.SpecialCoordsFox")],
    ["NumbersFromTop", t("ConfigDialog2.SpecialCoordsNumberFromTop")],
    ["NumbersFromBottom", t("ConfigDialog2.SpecialCoordsNumberFromBottom")],
  ], s.coordinateStyle);

  return {
    fields: [scoreScale],
    save: async () => {
      const current = state.view?.settings.display;
      if (!current) return false;
      const white = whiteStyle();
      const scale = parseJavaDouble(scoreScale.value);
      const replayMillis = fieldValue(replay);
      // A seconds field that does not read as a number keeps the saved delay (SetDelayShowCandidates).
      const delay = Number(delaySeconds.value.trim());
      const next: DisplaySettings = {
        ...current,
        winrateAlwaysBlack: blackWinrate.checked,
        blunderBar: blunderBar.checked,
        mouseOverWinrateGraph: hover.checked,
        winrateGraphFill: graphFill.checked,
        suggestionMaxRed: maxRed.checked,
        maxTreeWidth: intOr(treeWidth, current.maxTreeWidth),
        initialMaxScoreLead: scale === null ? current.initialMaxScoreLead : Math.abs(scale),
        blueRing: blueRing.checked,
        suggestionColorRatio: colorRatio(),
        whiteSuggestionWhite: (white & 1) !== 0,
        whiteSuggestionOrderWhite: (white & 2) !== 0,
        noSuggCircle: noSuggCircle.checked,
        minPlayoutRatio: fieldNumber(minRatio, false) / 100,
        scoreLeadWithKomi: scoreDisplay() === 1,
        nextMoveMinPlayouts: intOr(nextMoveVisits, current.nextMoveMinPlayouts),
        delayCandidates: delayOn.checked,
        delayCandidatesSeconds: delaySeconds.value.trim() !== "" && Number.isFinite(delay) ? delay : current.delayCandidatesSeconds,
        pvVisits: pvMode(),
        pvVisitsLimit: fieldNumber(pvLimit, true),
        removeDeadInVariation: removeDead.checked,
        // The field shows whole milliseconds (blank reads as 0): one the user did not edit keeps the
        // saved seconds exactly.
        replayIntervalSeconds: replayEdited ? replayMillis / 1000 : current.replayIntervalSeconds,
        coordinates: coordinates.checked,
        noRefreshOnSub: freezeSub.checked,
        noRefreshOnMouse: freezeVariation.checked,
        coordinateStyle: coords(),
      };
      if (JSON.stringify(next) !== JSON.stringify(current)) {
        const result = await run({ type: "SetDisplaySettings", settings: next });
        if (!result || result.type === "Refused") return false;
      }
      // A changed delay applies at once, as 应用 did: candidates hide for it, or show when it is off.
      if (next.delayCandidates !== current.delayCandidates || next.delayCandidatesSeconds !== current.delayCandidatesSeconds) {
        if (next.delayCandidates) delayCandidates(next.delayCandidatesSeconds);
        else setCandidates(true, true);
      }
      // Every 保存设置, even with the same width, lets a tree that grew too wide scroll again
      // (Menu.java:1265-1272).
      clearTreeTooWide();
      return true;
    },
  };
}
