// Comment pane click-to-edit (LizzieFrame.setCommentEditable, java:11248-11275). A left click on
// the comment opens an edit pane with the node's comment; a press anywhere else in the window
// (processCommentMousePressed and the tree / table / bottom-bar handlers) commits it. The draft
// always belongs to the node the edit began on, even when the shown position changes meanwhile.
import { registerViewHandler, state } from "../app";
import { edit } from "./state";
import "./editing.css";

interface Session {
  document: number;
  node: number;
  original: string;
  area: HTMLTextAreaElement;
  view: HTMLElement;
}

let session: Session | null = null;

function begin(view: HTMLElement): void {
  const v = state.view;
  if (session || !v) return;
  // Java refuses while the engine is still loading (`!Lizzie.leelaz.isLoaded()`).
  const foreground = v.engine.foreground;
  if (foreground && v.engine.engines[foreground.index]?.state === "Loading") return;
  const original = v.position.comment;
  const area = document.createElement("textarea");
  area.className = "comment-edit";
  area.id = "commentEdit";
  area.spellcheck = false;
  // setCommentEditable(true): a non-empty comment gets a trailing newline; the caret ends there.
  area.value = original.length > 0 ? `${original}\n` : "";
  area.addEventListener("blur", () => {
    // Switching to another application keeps the pane open, as in Java.
    if (document.hasFocus()) commitComment();
  });
  view.style.display = "none";
  view.after(area);
  session = { document: v.document.id, node: v.position.node, original, area, view };
  area.focus();
  area.setSelectionRange(area.value.length, area.value.length);
}

/** setCommentEditable(false): closes the edit pane and stores the text (one trailing newline
 *  removed) on the node it was opened on. Does nothing when no edit is open. */
export function commitComment(): void {
  const s = session;
  if (!s) return;
  session = null;
  const text = s.area.value.endsWith("\n") ? s.area.value.slice(0, -1) : s.area.value;
  s.area.remove();
  s.view.style.display = "";
  // Node ids belong to their document; a draft of a document no longer shown has no target.
  if (text !== s.original && state.view?.document.id === s.document) {
    void edit({ op: "SetComment", node: s.node, comment: text });
  }
}

export function commentEditing(): boolean {
  return session !== null;
}

/** `view` is #commentText (the shown comment); the edit pane takes its place in the panel. */
export function bindCommentEditor(view: HTMLElement): void {
  view.addEventListener("click", (e) => {
    // mouseClicked: a press and release without dragging (a drag selects text instead).
    if (e.button === 0 && !window.getSelection()?.toString()) begin(view);
  });
  // Any press outside the edit pane commits first, before the pressed control acts (menus and
  // the board prevent the default focus change, so blur alone would miss them).
  document.addEventListener(
    "mousedown",
    (e) => {
      if (session && e.target !== session.area) commitComment();
    },
    true,
  );
  registerViewHandler((v) => {
    if (!session) return;
    if (v.document.id !== session.document) {
      // Opening another record replaces the document: commit can no longer reach the node.
      session.area.remove();
      session.view.style.display = "";
      session = null;
    } else if (v.position.node !== session.node) {
      commitComment();
    }
  });
}
