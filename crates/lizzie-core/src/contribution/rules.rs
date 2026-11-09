//! `Utils.getRuleString(JSONObject)` (`Utils.java:435-488`): the rules of a watched game as the
//! contribution window and the root comment show them, in the session language.

use serde_json::Value;

/// `getRuleString`: one labelled line per rule field present in the JSON, separated by `\r\n`.
/// `text` resolves a Java text key in the session language.
pub fn rules_text(rules: &Value, text: &dyn Fn(&str) -> String) -> String {
    let mut out = String::new();
    if let Some(scoring) = rules.get("scoring") {
        out.push_str(&text("ContributeView.rules.scoring"));
        out.push_str(&text(if scoring.as_str() == Some("AREA") {
            "ContributeView.rules.scoring.area"
        } else {
            "ContributeView.rules.scoring.territory"
        }));
    }
    if let Some(ko) = rules.get("ko") {
        out.push_str("\r\n");
        out.push_str(&text("ContributeView.rules.ko"));
        let key = match ko.as_str() {
            Some("POSITIONAL") => "ContributeView.rules.ko.positional",
            Some("SITUATIONAL") => "ContributeView.rules.ko.situational",
            Some("SIMPLE") => "ContributeView.rules.ko.simple",
            _ => "ContributeView.rules.ko",
        };
        out.push_str(&text(key));
    }
    if let Some(suicide) = rules.get("suicide") {
        out.push_str("\r\n");
        out.push_str(&text("ContributeView.rules.suicide"));
        out.push_str(&text(if suicide.as_bool() == Some(true) {
            "ContributeView.rules.yes"
        } else {
            "ContributeView.rules.no"
        }));
    }
    if let Some(tax) = rules.get("tax") {
        out.push_str("\r\n");
        out.push_str(&text("ContributeView.rules.tax"));
        let key = match tax.as_str() {
            Some("NONE") => "ContributeView.rules.tax.none",
            Some("ALL") => "ContributeView.rules.tax.all",
            Some("SEKI") => "ContributeView.rules.tax.seki",
            _ => "ContributeView.rules.tax",
        };
        out.push_str(&text(key));
    }
    if let Some(bonus) = rules.get("whiteHandicapBonus") {
        out.push_str("\r\n");
        out.push_str(&text("ContributeView.rules.whiteHandicapBonus"));
        let key = match bonus.as_str() {
            Some("0") => "ContributeView.rules.whiteHandicapBonus.0",
            Some("N") => "ContributeView.rules.whiteHandicapBonus.N",
            Some("N-1") => "ContributeView.rules.whiteHandicapBonus.N-1",
            _ => "ContributeView.rules.whiteHandicapBonus",
        };
        out.push_str(&text(key));
    }
    if let Some(button) = rules.get("hasButton") {
        out.push_str("\r\n");
        out.push_str(&text("ContributeView.rules.button"));
        out.push_str(&text(if button.as_bool() == Some(true) {
            "ContributeView.rules.yes"
        } else {
            "ContributeView.rules.no"
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn every_present_field_is_labelled_in_order() {
        let rules = json!({
            "scoring": "AREA",
            "ko": "POSITIONAL",
            "suicide": false,
            "tax": "NONE",
            "whiteHandicapBonus": "N",
            "hasButton": true,
            "komi": 7.5
        });
        let text = rules_text(&rules, &|key| key.to_string());
        assert_eq!(
            text,
            "ContributeView.rules.scoringContributeView.rules.scoring.area\r\nContributeView.rules.koContributeView.rules.ko.positional\r\nContributeView.rules.suicideContributeView.rules.no\r\nContributeView.rules.taxContributeView.rules.tax.none\r\nContributeView.rules.whiteHandicapBonusContributeView.rules.whiteHandicapBonus.N\r\nContributeView.rules.buttonContributeView.rules.yes"
        );
    }

    #[test]
    fn an_empty_rule_object_says_nothing() {
        assert_eq!(rules_text(&json!({"komi": 7.5}), &|key| key.to_string()), "");
    }
}
