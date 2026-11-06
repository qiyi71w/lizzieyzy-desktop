// Wording of the AI 解说 feature: the core sends Java `Teacher.*` keys with MessageFormat arguments
// (TeacherStrings); this module turns them into text. The only product message is the notice for a
// commentary that was not written because its move changed.
import type { CommentaryStatus, CommentaryText } from "../contract/generated";
import { format, product, registerProduct, t } from "../i18n";

registerProduct({
  ZhCn: { commentaryTargetChanged: "解说已完成，但讲解的这一手已被修改或删除，因此未加入棋谱评论。" },
  ZhTw: { commentaryTargetChanged: "解說已完成，但講解的這一手已被修改或刪除，因此未加入棋譜評論。" },
  EnUs: { commentaryTargetChanged: "Commentary completed, but the explained move was edited or deleted, so it was not added to the game comments." },
});

/** A Java key, or the given text when the bundles have no such key (TeacherStrings fallback). */
export function keyOr(key: string, fallback = key): string {
  const text = t(key);
  return text === key ? fallback : text;
}

/** TeacherStrings.format. */
export function teacherText(text: CommentaryText): string {
  if (!text.key) return "";
  // Teacher.chatgpt.usingPlan has no placeholder; the core adds the model (D:3/Z:3 bundles).
  if (text.key === "Teacher.chatgpt.usingPlan") return text.args.length ? `${t(text.key)} · ${text.args[0]}` : t(text.key);
  return format(t(text.key), ...text.args);
}

/** The status line: a failure's cause is a Java key when the core classified it. */
export function statusText(status: CommentaryStatus, targetChanged: boolean): string {
  if (targetChanged && status.key === "Teacher.status.completed") return product("commentaryTargetChanged");
  if (status.detail_key) return format(t(status.key), t(status.detail_key), ...status.args.slice(1));
  return teacherText(status);
}

/** A command failure's message: a Teacher.* key, or safe English detail without one. */
export function failureText(message: string): string {
  return message.startsWith("Teacher.") ? keyOr(message, t("Teacher.error.generic")) : message || t("Teacher.error.generic");
}
