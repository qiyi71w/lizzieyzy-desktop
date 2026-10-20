import { t } from "../i18n";

/** Explicit keys keep the Java bundle extractor aware of server-reported outcomes. */
export function tuningText(key: string): string {
  const messages: Record<string, string> = {
    "AutoSetup.benchmarkFailed": t("AutoSetup.benchmarkFailed"),
    "AutoSetup.benchmarkCancelled": t("AutoSetup.benchmarkCancelled"),
    "AutoSetup.benchmarkDone": t("AutoSetup.benchmarkDone"),
    "EngineThreadPolicy.benchmarkBusy": t("EngineThreadPolicy.benchmarkBusy"),
    "EngineThreadPolicy.targetDeleted": t("EngineThreadPolicy.targetDeleted"),
    "EngineThreadPolicy.invalidRecommendation": t("EngineThreadPolicy.invalidRecommendation"),
    "EngineThreadPolicy.environmentChanged": t("EngineThreadPolicy.environmentChanged"),
    "EngineThreadPolicy.environmentUnknown": t("EngineThreadPolicy.environmentUnknown"),
    "EngineThreadPolicy.invalidBenchmarkCommand": t("EngineThreadPolicy.invalidBenchmarkCommand"),
    "EngineThreadPolicy.benchmarkInputUnavailable": t("EngineThreadPolicy.benchmarkInputUnavailable"),
    "EngineThreadPolicy.selectTarget": t("EngineThreadPolicy.selectTarget"),
    "EngineThreadPolicy.unknownBenchmarkTarget": t("EngineThreadPolicy.unknownBenchmarkTarget"),
    "MeasuredTuning.saved": t("MeasuredTuning.saved"),
    "MeasuredTuning.rejected": t("MeasuredTuning.rejected"),
    "AutoSetup.experimentalBenchmarkUnavailable": t("AutoSetup.experimentalBenchmarkUnavailable"),
  };
  return messages[key] ?? t(key);
}
export function plainJava(text: string): string {
  const document = new DOMParser().parseFromString(text.replace(/<br\s*\/?\s*>/gi, "\n"), "text/html");
  return document.body.textContent ?? text;
}
