// Record-editing entries of the command catalog (Menu.java Game/Edit menus, the top toolbar's
// edit and markup buttons, the bottom toolbar and FunctionCatalog ids). Menus, toolbars, shortcuts
// and context menus run them by Java id.
import { run, state } from "../app";
import { COMMANDS, type CommandEntry } from "../commands";
import type { EditSettings, MarkTool, Stone, Transform } from "../contract/generated";
import { message } from "../dialogs";
import { t } from "../i18n";
import { openBoardSize, openGameInfo } from "./dialogs";
import { convertToStartingPosition, deleteWithPrompt, edit, editing, newBoardWithPrompt, setEditing, setSetupMode, type Placement, type SetupTool } from "./state";

/** The 交替落子 toolbar icon (blackwhite) is lit only once 交替落子 was chosen explicitly: from the
 *  menu (Menu.java:3792-3796) or the button itself (7606-7617). Choosing black/white again from
 *  their own button returns to alternate play with nothing lit (7563-7567). */
export const placementLit = { alternate: false };

/** Every placement choice also ends insert mode (Input.insert = 0). */
export function choosePlacement(placement: Placement, alternateLit: boolean): void {
  placementLit.alternate = alternateLit;
  setEditing({ placement, insert: null });
}

/** One setting changed, the rest of `settings.edit` as it is. */
function setEdit<K extends keyof EditSettings>(key: K, value: EditSettings[K]): void {
  const settings = state.view?.settings.edit;
  if (settings) void run({ type: "SetEditSettings", settings: { ...settings, [key]: value } });
}

function editToggle(key: "allowDoubleClick" | "allowDrag" | "showNewBoardHint"): CommandEntry {
  return {
    run: () => setEdit(key, !state.view?.settings.edit[key]),
    checked: () => state.view?.settings.edit[key] ?? false,
  };
}

/** LizzieFrame.selectSetupTool: choosing a tool also enters setup mode. */
function setupTool(tool: SetupTool): CommandEntry {
  return {
    run: () => {
      setEditing({ setupTool: tool });
      if (!state.view?.editing.setup_mode) void setSetupMode(true);
    },
    checked: () => editing.setupTool === tool,
  };
}

/** 插入黑子 / 插入白子 check boxes: a toggle (Menu.java:3837-3857). */
function insert(color: Stone): CommandEntry {
  return {
    run: () => setEditing({ insert: editing.insert === color ? null : color }),
    checked: () => editing.insert === color,
  };
}

/** Markup buttons: choosing the selected tool again turns markup off (Menu.java:8521-8623). */
function markup(tool: MarkTool): CommandEntry {
  return {
    run: () => setEditing({ markup: editing.markup === tool ? null : tool }),
    checked: () => editing.markup === tool,
  };
}

function transform(transform: Transform): CommandEntry {
  return { run: () => void edit({ op: "Transform", transform }) };
}

/** Menu.java:3325-3338: nothing continued shows continueLadderFail with
 *  Board.MINIMUM_LADDER_LENGTH_FOR_AUTO_CONTINUATION (5). */
async function continueLadder(): Promise<void> {
  const r = await edit({ op: "ContinueLadder" });
  if (r?.type === "Unchanged") void message(t("Message.title"), t("Menu.game.continueLadderFail").replace("%d", "5"));
}

const setAsMain: CommandEntry = { run: () => void edit({ op: "SetAsMain" }) };

Object.assign(COMMANDS, {
  // File
  "files.new": { run: () => void newBoardWithPrompt() },
  // Game (Menu.java:3162-3339)
  "menu.playBestMove": { run: () => void edit({ op: "PlayBestMove" }) },
  "menu.playPassMove": { run: () => void edit({ op: "Pass" }) },
  "menu.setupModeToggle": {
    run: () => void setSetupMode(!state.view?.editing.setup_mode),
    checked: () => state.view?.editing.setup_mode ?? false,
  },
  "menu.setupToolBlack": setupTool("black"),
  "menu.setupToolWhite": setupTool("white"),
  "menu.setupToolErase": setupTool("erase"),
  "menu.setupClearAll": { run: () => void edit({ op: "SetupClearAll" }) },
  "menu.setupBlackToPlay": { run: () => void edit({ op: "SetupSideToPlay", color: "Black" }) },
  "menu.setupWhiteToPlay": { run: () => void edit({ op: "SetupSideToPlay", color: "White" }) },
  "menu.convertCurrentPosition": { run: () => void convertToStartingPosition() },
  "menu.continueLadder": { run: () => void continueLadder() },
  // Edit (Menu.java:3757-4081)
  "menu.addBlack": { run: () => choosePlacement("Black", false) },
  "menu.addWhite": { run: () => choosePlacement("White", false) },
  "menu.alternatelyMoves": { run: () => choosePlacement("alternate", true) },
  "menu.allowDoubleClick": editToggle("allowDoubleClick"),
  "menu.allowDrag": editToggle("allowDrag"),
  "menu.insertBlack": insert("Black"),
  "menu.insertWhite": insert("White"),
  "menu.clearBoard": { run: () => void edit({ op: "ClearBoard" }) },
  "menu.setAsMain": setAsMain,
  "menu.delete": { run: () => void deleteWithPrompt(false) },
  "menu.deleteBranch": { run: () => void deleteWithPrompt(true) },
  // Hidden while there is nothing to restore (menuSelected, Menu.java:4077-4080).
  "menu.undoDelete": { run: () => void edit({ op: "UndoEdit" }), visible: () => state.view?.editing.can_undo ?? false },
  "menu.redoDelete": { run: () => void edit({ op: "RedoEdit" }), visible: () => state.view?.editing.can_redo ?? false },
  "game.komi": { run: openGameInfo },
  "menu.setBoard": { run: openBoardSize },
  "menu.exchange": transform("ExchangeColors"),
  "menu.spinRight": transform("RotateRight"),
  "menu.spinLeft": transform("RotateLeft"),
  "menu.mirrorVertical": transform("MirrorVertical"),
  "menu.mirrorHorizon": transform("MirrorHorizontal"),
  // View > 主界面设置
  "menu.showNewBoardConfirmDialog": editToggle("showNewBoardHint"),
  // Top toolbar (Menu.java:8473-8637)
  "toolbar.change-turn": { run: () => void edit({ op: "ChangeTurn" }) },
  "toolbar.markup-tools": {
    // setMarkupType(false, 0), then show/hide the markup buttons (show-markup-tools).
    run: () => {
      setEditing({ markup: null });
      setEdit("showMarkupTools", !state.view?.settings.edit.showMarkupTools);
    },
    checked: () => state.view?.settings.edit.showMarkupTools ?? false,
  },
  "toolbar.markup-letter": markup("Letter"),
  "toolbar.markup-number": markup("Number"),
  "toolbar.markup-circle": markup("Circle"),
  "toolbar.markup-cross": markup("Cross"),
  "toolbar.markup-square": markup("Square"),
  "toolbar.markup-triangle": markup("Triangle"),
  "toolbar.markup-eraser": markup("Eraser"),
  "toolbar.markup-clear": { run: () => void edit({ op: "ClearMarks" }) },
  // Bottom toolbar (BottomToolbar.java:1307, 1321)
  "toolbar.try-play": { run: () => void edit({ op: "TryPlay" }), checked: () => state.view?.editing.trying ?? false },
  "board.delete-without-prompt": { run: () => void edit({ op: "DeleteMoveNoHint" }) },
  // RightClickMenu.cleanedittemp: shown while either edit cache holds a record.
  "board.clear-edit-cache": {
    run: () => void edit({ op: "ClearEditCache" }),
    visible: () => (state.view?.editing.can_undo || state.view?.editing.can_redo) ?? false,
  },
} satisfies Record<string, CommandEntry>);
