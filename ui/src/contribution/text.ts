// 跑谱贡献 texts: the Java keys the core sends (tips, notices, refusals) and this product's own
// wording (engine qualification, the credential-store state of the training password).
import { product, registerProduct, t } from "../i18n";

registerProduct({
  ZhCn: {
    contributionResourceUnsupported: "所选引擎包不支持跑谱贡献，未开始。请选择支持分布式训练的官方 KataGo。",
    contributionUnsupported: "KataGo {0}（{1}）不支持跑谱贡献，无法开始。请选择支持分布式训练的官方 KataGo。",
    contributionUnverified: "KataGo {0}（{1}）为官方分布式训练构建；训练服务器是否接受此版本，本应用尚未验证。",
    contributionUnknown: "未确认此引擎能否跑谱贡献；开始后由引擎输出和训练服务器判定。",
    contributionPasswordStored: "已保存在系统凭据库；不改动则保持不变",
    contributionPasswordSession: "本次运行已记住；不改动则保持不变",
  },
  ZhTw: {
    contributionResourceUnsupported: "所選引擎套件不支援跑譜貢獻，未開始。請選擇支援分散式訓練的官方 KataGo。",
    contributionUnsupported: "KataGo {0}（{1}）不支援跑譜貢獻，無法開始。請選擇支援分散式訓練的官方 KataGo。",
    contributionUnverified: "KataGo {0}（{1}）為官方分散式訓練建置；訓練伺服器是否接受此版本，本應用程式尚未驗證。",
    contributionUnknown: "未確認此引擎能否跑譜貢獻；開始後由引擎輸出和訓練伺服器判定。",
    contributionPasswordStored: "已儲存在系統憑據庫；不改動則保持不變",
    contributionPasswordSession: "本次執行已記住；不改動則保持不變",
  },
  EnUs: {
    contributionResourceUnsupported: "The selected engine package cannot contribute, so nothing was started. Choose an official KataGo built for distributed training.",
    contributionUnsupported: "KataGo {0} ({1}) cannot contribute and will not start. Choose an official KataGo built for distributed training.",
    contributionUnverified: "KataGo {0} ({1}) is an official distributed-training build; whether the training server accepts this version is not yet verified in this app.",
    contributionUnknown: "Whether this engine can contribute is unconfirmed; the engine output and the training server decide after the start.",
    contributionPasswordStored: "Kept in the system credential store; unchanged unless you edit it",
    contributionPasswordSession: "Kept until you quit; unchanged unless you edit it",
  },
});

/** The text of a key the core sends for 跑谱贡献 (tip line, message notices, refusals). Every Java
 *  key is written out so the string extractor keeps it. */
export function contributionText(key: string): string {
  const texts: Record<string, string> = {
    "ContributeView.lblTip": t("ContributeView.lblTip"),
    "ContributeView.closeTip": t("ContributeView.closeTip"),
    "Contribute.slowQuit.repeatedly": t("Contribute.slowQuit.repeatedly"),
    "Contribute.pauseResumeTooFrequently": t("Contribute.pauseResumeTooFrequently"),
    "Contribute.tips.signalToStop": t("Contribute.tips.signalToStop"),
    "Contribute.tips.exitedAfterSignal": t("Contribute.tips.exitedAfterSignal"),
    "Contribute.tips.startingNewGame": t("Contribute.tips.startingNewGame"),
    "Contribute.tips.predownload": t("Contribute.tips.predownload"),
    "Contribute.tips.download": t("Contribute.tips.download"),
    "Contribute.tips.invalidUserNamePassword": t("Contribute.tips.invalidUserNamePassword"),
    "Contribute.tips.noResponseFromServer": t("Contribute.tips.noResponseFromServer"),
    "Contribute.tips.errorConnectingToServer": t("Contribute.tips.errorConnectingToServer"),
    "Contribute.tips.uploadError": t("Contribute.tips.uploadError"),
    "Contribute.tips.initialQuery": t("Contribute.tips.initialQuery"),
    "Contribute.tips.couldNotParseServerUrl": t("Contribute.tips.couldNotParseServerUrl"),
    "Contribute.tips.didYouVerifyYourEmailAddress": t("Contribute.tips.didYouVerifyYourEmailAddress"),
    "Contribute.tips.modelFileWasIncompletelyDownloaded": t("Contribute.tips.modelFileWasIncompletelyDownloaded"),
    "Contribute.tips.useOfficialEngineSupportDistributedTraining": t("Contribute.tips.useOfficialEngineSupportDistributedTraining"),
    "Contribute.tips.buildRejected": t("Contribute.tips.buildRejected"),
    "Contribute.tips.noGameToSave": t("Contribute.tips.noGameToSave"),
    "Contribute.tips.contributingAndStartAnotherLizzieYzy": t("Contribute.tips.contributingAndStartAnotherLizzieYzy"),
    "Contribute.tips.alreadyTraining": t("Contribute.tips.alreadyTraining"),
    "Contribute.tips.noUserName": t("Contribute.tips.noUserName"),
    "Contribute.tips.noEnginePath": t("Contribute.tips.noEnginePath"),
    "Contribute.tips.noCAcerts": t("Contribute.tips.noCAcerts"),
    "Contribute.tips.PauseHaveSent": t("Contribute.tips.PauseHaveSent"),
    "Contribute.tips.ResumeHaveSent": t("Contribute.tips.ResumeHaveSent"),
    "Contribute.tips.blockedByKataGoTuning": t("Contribute.tips.blockedByKataGoTuning"),
    "contribution.resourceUnsupported": product("contributionResourceUnsupported"),
  };
  return texts[key] ?? t(key);
}

/** `Contribute.version.outOfDate (version), outOfDate2` (ContributeEngine.java:505-509) as plain
 *  text, and the label of the KataGo releases link the Java HTML carries. */
export function outOfDateText(version: string): { text: string; link: string } {
  const html = `${t("Contribute.version.outOfDate")} (${version}), ${t("Contribute.version.outOfDate2")}`;
  const anchor = /<a\b[^>]*>(.*?)<\/a>/is.exec(html);
  const plain = (anchor ? html.replace(anchor[0], "") : html)
    .replace(/<br\s*\/?>/gi, "\n")
    .replace(/<[^>]*>/g, "")
    .trim();
  return { text: plain, link: anchor?.[1].replace(/<[^>]*>/g, "").trim() || "https://github.com/lightvector/KataGo/releases" };
}
