// Editing interaction state (UI-owned, not saved) and the edit command with its Java prompts.
// The document itself changes only in Rust through the `Edit` command; menus, toolbars, keys and
// board input all go through `edit` / the prompt helpers here.
import { run, state } from "../app";
import type { CommandResult, EditCommand, EditTarget, MarkTool, Stone } from "../contract/generated";
import { ask, confirm } from "../dialogs";
import { t } from "../i18n";

/** LizzieFrame.blackorwhite: 交替落子 (0), 添加黑子 (1), 添加白子 (2). */
export type Placement = "alternate" | Stone;

/** LizzieFrame.setupTool. */
export type SetupTool = "black" | "white" | "erase";

export const editing = {
  placement: "alternate" as Placement,
  /** Input.insert: 插入黑子 / 插入白子 mode, or none. */
  insert: null as Stone | null,
  /** LizzieFrame.isMarkuping + markupType: the selected markup tool, or none. */
  markup: null as MarkTool | null,
  setupTool: "black" as SetupTool,
};

const changeListeners: (() => void)[] = [];

/** Toolbars and menus that show the interaction state repaint through this. */
export function onEditingChange(listener: () => void): void {
  changeListeners.push(listener);
}

export function setEditing(patch: Partial<typeof editing>): void {
  Object.assign(editing, patch);
  changeListeners.forEach((l) => l());
}

export function edit(op: EditCommand): Promise<CommandResult | null> {
  return run({ type: "Edit", op });
}

/** Asks only when Rust needs a confirmation, then confirms exactly the document, revision and
 *  node it was asked for; an edit in between turns the answer into a no-op. */
async function editWithPrompt(op: (confirmed: EditTarget | null) => EditCommand, prompt: () => Promise<boolean>): Promise<void> {
  const r = await edit(op(null));
  if (r?.type === "NeedsEditConfirmation" && (await prompt())) await edit(op(r.target));
}

/** 删除一手 / 删除分支: Board.deleteMove asks with an OK/Cancel dialog when moves follow. */
export function deleteWithPrompt(branch: boolean): Promise<void> {
  return editWithPrompt(
    (confirmed) => (branch ? { op: "DeleteBranch", confirmed } : { op: "DeleteMove", confirmed }),
    () => confirm(t("LizzieFrame.delete"), t("LizzieFrame.deleteMoves")),
  );
}

/** LizzieFrame.newEmptyBoard: "新的棋盘?" with "不再提示", Cancel as the default option. The check
 *  box takes effect when toggled, whichever button closes the dialog. */
export function newBoardWithPrompt(): Promise<void> {
  return editWithPrompt(
    (confirmed) => ({ op: "NewBoard", confirmed }),
    () => {
      const body = document.createElement("div");
      body.className = "new-board-prompt";
      const label = document.createElement("div");
      label.textContent = t("LizzieFrame.confirmNewBoard");
      const box = document.createElement("label");
      const check = document.createElement("input");
      check.type = "checkbox";
      check.dataset.key = "noNoticeAgain";
      check.onchange = () => {
        const settings = state.view?.settings.edit;
        if (settings) void run({ type: "SetEditSettings", settings: { ...settings, showNewBoardHint: !check.checked } });
      };
      box.append(check, document.createTextNode(t("LizzieFrame.noNoticeAgain")));
      body.append(label, box);
      return ask(t("LizzieFrame.confirmNewBoardTitle"), body, t("LizzieFrame.confirm"), t("LizzieFrame.cancel"), true);
    },
  );
}

/** LizzieFrame.confirmStartingPositionConversion (YES_NO, warning). */
const confirmConversion = () =>
  confirm(t("LizzieFrame.convertStartingPositionConfirmTitle"), t("LizzieFrame.convertStartingPositionConfirmMessage"), "warning");

/** 起始局面设置模式 on/off (toggleSetupMode); entering with moves asks before converting. */
export function setSetupMode(enabled: boolean): Promise<void> {
  return editWithPrompt((confirmed) => ({ op: "SetSetupMode", enabled, confirmed }), confirmConversion);
}

/** 将当前盘面设为起始局面. */
export function convertToStartingPosition(): Promise<void> {
  return editWithPrompt((confirmed) => ({ op: "ConvertToStartingPosition", confirmed }), confirmConversion);
}
