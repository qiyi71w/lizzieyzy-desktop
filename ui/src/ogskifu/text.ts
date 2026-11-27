// The OGS messages without a Java counterpart: OGS refusing requests for now (HTTP 429), and the
// empty table's first-use line. Java shows one message for every failure.
import { registerProduct } from "../i18n";

registerProduct({
  ZhCn: {
    ogsIdleText: "输入 OGS 用户名或玩家 ID 后按“{0}”。",
    ogsRateLimited: "OGS 暂时限制了请求次数，未能查询对局。请稍后再试。",
    ogsRecordRateLimited: "OGS 暂时限制了请求次数，未能下载这局棋谱。请稍后再试。",
  },
  ZhTw: {
    ogsIdleText: "輸入 OGS 使用者名稱或玩家 ID 後按「{0}」。",
    ogsRateLimited: "OGS 暫時限制了請求次數，未能查詢對局。請稍後再試。",
    ogsRecordRateLimited: "OGS 暫時限制了請求次數，未能下載這局棋譜。請稍後再試。",
  },
  EnUs: {
    ogsIdleText: "Enter an OGS username or player ID and press {0}.",
    ogsRateLimited: "OGS is limiting requests right now, so the games could not be listed. Try again later.",
    ogsRecordRateLimited: "OGS is limiting requests right now, so the game could not be downloaded. Try again later.",
  },
  JaJp: {
    ogsIdleText: "OGS のユーザー名またはプレイヤー ID を入力して「{0}」を押してください。",
    ogsRateLimited: "OGS が一時的にリクエストを制限しているため、対局を取得できませんでした。しばらくしてからもう一度お試しください。",
    ogsRecordRateLimited: "OGS が一時的にリクエストを制限しているため、棋譜をダウンロードできませんでした。しばらくしてからもう一度お試しください。",
  },
  KoKr: {
    ogsIdleText: "OGS 사용자 이름 또는 플레이어 ID를 입력하고 [{0}]을 누르세요.",
    ogsRateLimited: "OGS가 일시적으로 요청을 제한하고 있어 대국을 가져오지 못했습니다. 잠시 후 다시 시도하세요.",
    ogsRecordRateLimited: "OGS가 일시적으로 요청을 제한하고 있어 기보를 다운로드하지 못했습니다. 잠시 후 다시 시도하세요.",
  },
  ThTh: {
    ogsIdleText: "ป้อนชื่อผู้ใช้ OGS หรือรหัสผู้เล่น แล้วกด “{0}”",
    ogsRateLimited: "OGS จำกัดจำนวนคำขอชั่วคราว จึงดึงรายการเกมไม่ได้ โปรดลองอีกครั้งภายหลัง",
    ogsRecordRateLimited: "OGS จำกัดจำนวนคำขอชั่วคราว จึงดาวน์โหลดบันทึกเกมไม่ได้ โปรดลองอีกครั้งภายหลัง",
  },
});
