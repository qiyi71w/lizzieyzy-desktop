// 本局规则详情 (MatchRulesDetailsDialog.java): the frozen text of the rules the rules caption shows,
// read-only and selectable, closed with 关闭 or Escape. The core decides whether there is any.
import { run } from "../app";
import { panel } from "../dialogs";
import { t } from "../i18n";
import "./match.css";

/** LizzieFrame.inspectMatchRules: shows the details and returns true when the caption shows 本局规则;
 *  without them nothing happens and it returns false. */
export async function inspectMatchRules(): Promise<boolean> {
  const result = await run({ type: "InspectMatchRules" });
  if (result?.type !== "MatchRulesDetails") return false;
  const text = document.createElement("textarea");
  text.className = "match-rules-details";
  text.readOnly = true;
  text.spellcheck = false;
  text.textContent = result.text;
  text.setAttribute("aria-label", t("MatchRules.details.title"));
  text.dataset.key = "matchRules.details";
  await panel(t("MatchRules.details.title"), text, t("MatchRules.details.close"));
  return true;
}
