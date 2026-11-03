// 强制计算区域 (the toolbar's selectAllow / selectAvoid / clearSelect buttons and their limit
// popups, Menu.java:7646-8077; Alt+drag and Alt+click on the board, Input.java:21-24, 78-81,
// 155-158, LizzieFrame.java:11833-11928) and the board popup's region and 选点重点研究 items
// (RightClickMenu.java:124-204, 484-589). The core owns the region and the tracked points; the
// selection mode and the rectangle being dragged are window state, like Java's.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { FocusCapability, Point, RegionCommand, RegionSettings, TrackingCommand } from "../contract/generated";
import { parseJavaInt } from "../display/dialogs";
import type { ContextEntry } from "../editing/contextMenu";
import { product, registerProduct, t } from "../i18n";
import type { MenuNode } from "../chrome/javaMenus";
import { registerItemField } from "../chrome/menubar";

registerProduct({
  ZhCn: {
    trackPointUnsupportedTooltip: "当前 KataGo 不支持选点重点研究（分析指令的 focus 选项），需要换用支持它的 KataGo 版本。普通分析仍可使用。",
    trackPointProbeFailed: "选点重点研究不可用（引擎检测失败）",
    trackPointProbeFailedTooltip: "检测当前 KataGo 是否支持选点重点研究时出错。重启引擎会重新检测；普通分析仍可使用。",
  },
  ZhTw: {
    trackPointUnsupportedTooltip: "目前的 KataGo 不支援選點重點研究（分析指令的 focus 選項），需要換用支援它的 KataGo 版本。一般分析仍可使用。",
    trackPointProbeFailed: "選點重點研究無法使用（引擎檢測失敗）",
    trackPointProbeFailedTooltip: "檢測目前的 KataGo 是否支援選點重點研究時出錯。重新啟動引擎會重新檢測；一般分析仍可使用。",
  },
  EnUs: {
    trackPointUnsupportedTooltip: "This KataGo does not support point evaluation (the focus option of its analysis command). Use a KataGo version that supports it. Normal analysis still works.",
    trackPointProbeFailed: "Point evaluation unavailable (engine check failed)",
    trackPointProbeFailedTooltip: "Checking whether this KataGo supports point evaluation failed. Restarting the engine checks again; normal analysis still works.",
  },
});

// ------------------------------------------------------------------ window state

/** Input.selectMode: the next press on the board starts a selection (a toolbar button is lit). */
let selectMode = false;
/** LizzieFrame.selectForceAllow: what a selection chooses. */
let selectAllow = true;
/** The selection being dragged (drawSelectedRect), as the points it covers; null when it covers
 *  none or no drag is on. */
let dragging: { allow: boolean; from: Point; to: Point } | null = null;

const listeners: (() => void)[] = [];
export function onRegionUiChange(listener: () => void): void {
  listeners.push(listener);
}
function changed(): void {
  listeners.forEach((l) => l());
}

export function region(op: RegionCommand): void {
  void run({ type: "Region", op });
}

function tracking(op: TrackingCommand): void {
  void run({ type: "Tracking", op });
}

/** The selection drawn while dragging. */
export function regionDrag(): { allow: boolean; from: Point; to: Point } | null {
  return dragging;
}

/** Whether a press on the board selects a region (Input.selectMode or Alt). */
export function selecting(alt: boolean): boolean {
  return selectMode || alt;
}

/** selectPressed: Alt chooses the allowed region and keeps it. */
export function selectionPressed(alt: boolean): void {
  if (alt) {
    selectAllow = true;
    region({ type: "Keep" });
  }
  dragging = null;
}

/** selectDragged: the covered points, or none. */
export function selectionDragged(range: { from: Point; to: Point } | null): void {
  dragging = range && { allow: selectAllow, ...range };
  changed();
}

/** selectReleased: a selection covering points becomes the region and ends the selection mode;
 *  one covering none changes nothing. */
export function selectionReleased(range: { from: Point; to: Point } | null): void {
  dragging = null;
  if (range) {
    region({ type: "Select", allow: selectAllow, ...range });
    selectMode = false;
  }
  changed();
}

/** Input.mouseClicked with Alt: an existing region is cleared like the clear button. */
export function selectionClicked(): void {
  if (state.view?.region.points.length) clearSelect();
}

// ------------------------------------------------------------------ toolbar

/** selectAllow / selectAvoid: the first press lights the button and starts the selection mode,
 *  the other button switches the kind, the same button again ends it. Either keeps the region. */
function chooseSelect(allow: boolean): void {
  if (!selectMode) {
    selectMode = true;
    selectAllow = allow;
  } else if (selectAllow !== allow) {
    selectAllow = allow;
  } else {
    selectMode = false;
  }
  region({ type: "Keep" });
  changed();
}

/** clearSelect: ends the selection mode and clears the region. */
function clearSelect(): void {
  selectMode = false;
  dragging = null;
  region({ type: "Clear" });
  changed();
}

/** Whether a selection button shows its lit icon. */
export function selectLit(allow: boolean): boolean {
  return selectMode && selectAllow === allow;
}

function regionSettings(): RegionSettings | null {
  return state.view?.settings.region ?? null;
}

function setRegion(change: Partial<RegionSettings>): void {
  const settings = regionSettings();
  if (settings) void run({ type: "SetRegionSettings", settings: { ...settings, ...change } });
}

/** The custom count last typed into a popup, which a click on its item uses at once. */
const typed: { allow: number | null; avoid: number | null } = { allow: null, avoid: null };

function moves(allow: boolean): number {
  const s = regionSettings();
  return (allow ? s?.allowMoves : s?.avoidMoves) ?? 999;
}

function customMoves(allow: boolean): number {
  const s = regionSettings();
  return (allow ? typed.allow ?? s?.allowCustomMoves : typed.avoid ?? s?.avoidCustomMoves) ?? 5;
}

function limitEntries(allow: boolean): Record<string, CommandEntry> {
  const side = allow ? "allow" : "avoid";
  const set = (n: number) => setRegion(allow ? { allowMoves: n } : { avoidMoves: n });
  return {
    [`toolbar.force-${side}-options.all`]: { run: () => set(999), checked: () => moves(allow) === 999 },
    [`toolbar.force-${side}-options.one`]: { run: () => set(1), checked: () => moves(allow) === 1 },
    [`toolbar.force-${side}-options.custom`]: {
      run: () => {
        const n = customMoves(allow);
        typed[side] = null;
        setRegion(allow ? { allowMoves: n, allowCustomMoves: n } : { avoidMoves: n, avoidCustomMoves: n });
      },
      checked: () => moves(allow) !== 999 && moves(allow) !== 1,
    },
  };
}

/** selectAllowPopup / selectAvoidPopup (限制整个搜索过程 / 仅限制第一手 / 自定义限制手数). */
export function limitPopup(allow: boolean): MenuNode[] {
  const side = allow ? "allow" : "avoid";
  return [
    { t: "check", key: "Menu.selectLimitAllMoves", id: `toolbar.force-${side}-options.all` },
    { t: "check", key: "Menu.selectLimitOneMove", id: `toolbar.force-${side}-options.one` },
    { t: "check", key: "Menu.selectLimitCustomMoves", id: `toolbar.force-${side}-options.custom` },
  ];
}

/** txtLimitLengthAllow / txtLimitLengthAvoid (IntDocument): each edit is the custom count
 *  (parseTextToInt keeps the previous one for anything but an integer); while the custom item is
 *  the chosen limit it is also the limit (popupMenuWillBecomeInvisible). */
function customField(allow: boolean): HTMLInputElement {
  const side = allow ? "allow" : "avoid";
  const field = document.createElement("input");
  field.type = "text";
  field.className = "mi-field";
  field.value = String(customMoves(allow));
  field.setAttribute("aria-label", t("Menu.selectLimitCustomMoves"));
  field.addEventListener("beforeinput", (e) => {
    if (e.inputType.startsWith("insert") && /\D/.test(e.data ?? "")) e.preventDefault();
  });
  field.addEventListener("input", () => {
    const n = parseJavaInt(field.value.trim()) ?? customMoves(allow);
    typed[side] = n;
    const custom = moves(allow) !== 999 && moves(allow) !== 1;
    setRegion(allow ? { allowCustomMoves: n, ...(custom ? { allowMoves: n } : {}) } : { avoidCustomMoves: n, ...(custom ? { avoidMoves: n } : {}) });
  });
  field.addEventListener("click", (e) => e.stopPropagation());
  return field;
}
registerItemField("toolbar.force-allow-options.custom", () => customField(true));
registerItemField("toolbar.force-avoid-options.custom", () => customField(false));

// ------------------------------------------------------------------ board popup

function same(a: Point, b: Point): boolean {
  return a.x === b.x && a.y === b.y;
}

/** The label and tooltip of 评估此点 when it cannot be chosen (trackingUnavailableKey); a failed
 *  probe has its own wording (Java had no such state). */
function unavailableTracking(capability: FocusCapability): { label: string; title: string } {
  switch (capability) {
    case "Unsupported":
      return { label: t("RightClickMenu.trackPoint.upgradeRequired"), title: product("trackPointUnsupportedTooltip") };
    case "Failed":
      return { label: product("trackPointProbeFailed"), title: product("trackPointProbeFailedTooltip") };
    default:
      return { label: t("RightClickMenu.trackPoint.requiresCapability"), title: t("RightClickMenu.trackPoint.unavailableTooltip") };
  }
}

/** RightClickMenu's region and tracking items for an empty intersection, in Java order and with
 *  its visibility outside engine games. */
export function regionMenuEntries(point: Point): ContextEntry[] {
  const v = state.view!;
  const r = v.region;
  const allowed = r.allow ? r.points : [];
  const focus = v.focus;
  const tracked = focus.points.some((p) => same(p.point, point));
  const entries: ContextEntry[] = [
    { key: "RightClickMenu.allow", label: t("RightClickMenu.allow"), icon: "setpoint.png", run: () => region({ type: "AllowPoint", point }) },
  ];
  if (allowed.length) entries.push({ key: "RightClickMenu.allow2", label: t("RightClickMenu.allow2"), icon: "addpoint.png", run: () => region({ type: "AddAllowPoint", point }) });
  if (allowed.length > 1 && allowed.some((p) => same(p, point)))
    entries.push({ key: "RightClickMenu.allow3", label: t("RightClickMenu.allow3"), icon: "clear.png", run: () => region({ type: "RemoveAllowPoint", point }) });
  entries.push(
    { key: "RightClickMenu.avoid", label: t("RightClickMenu.avoid"), icon: "forbidpoint.png", run: () => region({ type: "AvoidPoint", point }) },
    { key: "RightClickMenu.cancelavoid", label: t("RightClickMenu.cancelavoid"), icon: "recycle.png", run: () => region({ type: "ClearForcing" }) },
    { key: "RightClickMenu.avoid2", label: t("RightClickMenu.avoid2"), checked: r.keep_forcing, run: () => region({ type: "ToggleKeepForcing" }) },
  );
  if (!tracked) {
    const off = focus.available ? null : unavailableTracking(focus.capability);
    entries.push({
      key: "RightClickMenu.trackPoint",
      label: off?.label ?? t("RightClickMenu.trackPoint"),
      title: off?.title ?? t("RightClickMenu.trackPoint.tooltip"),
      icon: "trackingpoint.png",
      disabled: !!off,
      run: () => tracking({ type: "Track", point }),
    });
  } else {
    entries.push({ key: "RightClickMenu.untrackPoint", label: t("RightClickMenu.untrackPoint"), icon: "clear.png", run: () => tracking({ type: "Untrack", point }) });
  }
  if (focus.points.length)
    entries.push({ key: "RightClickMenu.clearAllTracked", label: t("RightClickMenu.clearAllTracked"), icon: "recycle.png", run: () => tracking({ type: "Clear" }) });
  return entries;
}

Object.assign(COMMANDS, {
  "toolbar.force-allow": { run: () => chooseSelect(true), checked: () => selectLit(true) },
  "toolbar.force-avoid": { run: () => chooseSelect(false), checked: () => selectLit(false) },
  "toolbar.force-clear": { run: clearSelect },
  ...limitEntries(true),
  ...limitEntries(false),
} satisfies Record<string, CommandEntry>);
