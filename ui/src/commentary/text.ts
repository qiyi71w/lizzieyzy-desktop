// Wording of the AI 解说 feature: the core sends Java `Teacher.*` keys with MessageFormat arguments
// (TeacherStrings); this module turns them into text. The product messages are the notice for a
// commentary that was not written because its move changed, and the causes of a reply that cannot
// be shown or written (`product.commentaryEmpty`, `product.commentaryTooLarge` from the core).
import type { CommentaryStatus, CommentaryText } from "../contract/generated";
import { format, product, registerProduct, t } from "../i18n";

registerProduct({
  ZhCn: {
    commentaryTargetChanged: "解说已完成，但讲解的这一手已被修改或删除，因此未加入棋谱评论。",
    commentaryEmpty: "AI 服务没有返回内容。请重试。",
    commentaryTooLarge: "解说超出棋谱评论的长度上限，未加入棋谱评论。",
  },
  ZhTw: {
    commentaryTargetChanged: "解說已完成，但講解的這一手已被修改或刪除，因此未加入棋譜評論。",
    commentaryEmpty: "AI 服務沒有傳回內容。請重試。",
    commentaryTooLarge: "解說超出棋譜評論的長度上限，未加入棋譜評論。",
  },
  EnUs: {
    commentaryTargetChanged: "Commentary completed, but the explained move was edited or deleted, so it was not added to the game comments.",
    commentaryEmpty: "The AI service returned no text. Try again.",
    commentaryTooLarge: "The commentary is longer than a game comment can hold, so it was not added to the game comments.",
  },
  JaJp: {
    commentaryTargetChanged: "解説は完了しましたが、解説対象の手が変更または削除されたため、棋譜のコメントには追加しませんでした。",
    commentaryEmpty: "AI サービスから内容が返されませんでした。もう一度お試しください。",
    commentaryTooLarge: "解説が棋譜コメントの長さの上限を超えたため、棋譜のコメントには追加しませんでした。",
  },
  KoKr: {
    commentaryTargetChanged: "해설은 완료되었지만 설명한 수가 수정되거나 삭제되어 기보 코멘트에 추가하지 않았습니다.",
    commentaryEmpty: "AI 서비스가 내용을 반환하지 않았습니다. 다시 시도하세요.",
    commentaryTooLarge: "해설이 기보 코멘트의 길이 한도를 넘어 기보 코멘트에 추가하지 않았습니다.",
  },
  ThTh: {
    commentaryTargetChanged: "คำอธิบายเสร็จแล้ว แต่ตาเดินที่อธิบายถูกแก้ไขหรือลบไปแล้ว จึงไม่ได้เพิ่มลงในความคิดเห็นของบันทึกหมาก",
    commentaryEmpty: "บริการ AI ไม่ได้ส่งข้อความกลับมา โปรดลองอีกครั้ง",
    commentaryTooLarge: "คำอธิบายยาวเกินกว่าที่ความคิดเห็นของบันทึกหมากรองรับได้ จึงไม่ได้เพิ่มลงในความคิดเห็น",
  },
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

/** The status line: a failure's cause is a Java key when the core classified it, or one of this
 *  product's `product.*` causes. */
export function statusText(status: CommentaryStatus, targetChanged: boolean): string {
  if (targetChanged && status.key === "Teacher.status.completed") return product("commentaryTargetChanged");
  if (!status.detail_key) return teacherText(status);
  const detail = status.detail_key.startsWith("product.") ? product(status.detail_key.slice("product.".length)) : t(status.detail_key);
  return format(t(status.key), detail, ...status.args.slice(1));
}

/** A command failure's message: a Teacher.* key, or safe English detail without one. */
export function failureText(message: string): string {
  return message.startsWith("Teacher.") ? keyOr(message, t("Teacher.error.generic")) : message || t("Teacher.error.generic");
}
