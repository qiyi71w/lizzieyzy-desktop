// Board display keys of Input.keyPressed / keyReleased (Input.java:466-500, 560-574, 642-669,
// 708-716, 760-765, 851-854), in Java's modifier precedence. Combinations bound to other features
// (Ctrl+F, Ctrl/Alt/Shift+T, Alt/Shift/Ctrl+C, Ctrl/Alt+Z, Ctrl+V) are left to the caller.
import { openAutoPlayDialog } from "./autoplay";
import { replayBranch } from "./replay";
import { startTemporaryBoard, stopTemporaryBoard, togglePolicy, toggleRawNet, toggleCandidates } from "./state";
import {
  toggleAllNumbersInBranch,
  toggleCoordinates,
  toggleMoveNumbers,
  toggleMoveRank,
  toggleNextMoves,
  toggleSuggestionVariations,
} from "./commands";

/** Runs the display action of a workbench key press; returns whether the key was handled. */
export function handleDisplayKey(e: KeyboardEvent): boolean {
  const ctrl = e.ctrlKey || e.metaKey; // controlIsPressed
  const plain = !ctrl && !e.altKey && !e.shiftKey;
  switch (e.code) {
    case "KeyM":
      if (ctrl) toggleAllNumbersInBranch();
      else if (e.altKey) toggleMoveRank();
      else toggleMoveNumbers();
      return true;
    case "KeyJ":
      toggleNextMoves();
      return true;
    case "KeyF":
      if (e.shiftKey) toggleSuggestionVariations();
      else if (ctrl) return false;
      else toggleCandidates();
      return true;
    case "KeyH":
      toggleRawNet();
      return true;
    case "KeyV":
      if (ctrl || !e.altKey) return false;
      toggleSuggestionVariations();
      return true;
    case "KeyT":
      if (!plain) return false;
      togglePolicy();
      return true;
    case "KeyC":
      if (!plain) return false;
      toggleCoordinates();
      return true;
    case "KeyZ":
      if (e.ctrlKey || e.altKey) return false;
      startTemporaryBoard();
      return true;
    case "KeyR":
      replayBranch();
      return true;
    case "KeyA":
      // Ctrl+A only; Alt / Shift / plain A belong to other features.
      if (!ctrl) return false;
      openAutoPlayDialog();
      return true;
  }
  return false;
}

// keyReleased VK_Z: whatever the modifiers, holding Z ends (a no-op unless startTemporaryBoard ran).
document.addEventListener("keyup", (e) => {
  if (e.code === "KeyZ") stopTemporaryBoard();
});
