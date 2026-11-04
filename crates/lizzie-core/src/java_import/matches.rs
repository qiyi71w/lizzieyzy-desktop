use super::*;

impl Reader<'_> {
    pub(super) fn matches(&mut self, ui: &Map<String, Value>, handled: &mut Handled) {
        const STRINGS: &[(&str, &str)] = &[
            ("engine-pk-black-commands", "first.commands"),
            ("engine-pk-black-name", "first.name"),
            ("engine-pk-white-commands", "second.commands"),
            ("engine-pk-white-name", "second.name"),
            ("engine-game-match-rules", "rules"),
            ("advance-black-time-txt", "firstTimeCommand"),
            ("advance-white-time-txt", "secondTimeCommand"),
        ];
        for (key, field) in STRINGS {
            if let Some(raw) = ui.get(*key) {
                handled.push(("ui", key));
                let value = (!raw.is_null()).then(|| {
                    let text = shown(raw);
                    if key.ends_with("-commands") {
                        Value::from(resolve_command(&text, &self.source.roots(), &mut Vec::new()))
                    } else {
                        Value::from(text)
                    }
                });
                self.match_value(format!("ui.{key}"), field, value, raw, false);
            }
        }
        for (key, field, kind) in [
            ("new-engine-game-komi", "komi", DisplayKind::Num),
            ("new-engine-game-handicap", "handicap", DisplayKind::Int(0, 9)),
            ("engine-pk-ponder", "ponder", DisplayKind::Bool),
            ("engine-sgf-start", "sgfStart", DisplayKind::Bool),
            ("engine-sgf-random", "sgfRandom", DisplayKind::Bool),
            ("pk-advance-time-settings", "advancedTime", DisplayKind::Bool),
            (
                "first-engine-min-move",
                "firstResign.minMove",
                DisplayKind::Int(0, i32::MAX as i64),
            ),
            (
                "second-engine-min-move",
                "secondResign.minMove",
                DisplayKind::Int(0, i32::MAX as i64),
            ),
            (
                "first-engine-resign-move-counts",
                "firstResign.consecutiveMoves",
                DisplayKind::Int(0, i32::MAX as i64),
            ),
            (
                "second-engine-resign-move-counts",
                "secondResign.consecutiveMoves",
                DisplayKind::Int(0, i32::MAX as i64),
            ),
            ("first-engine-resign-winrate", "firstResign.winrate", DisplayKind::Num),
            ("second-engine-resign-winrate", "secondResign.winrate", DisplayKind::Num),
            ("check-random-visits", "checkRandomVisits", DisplayKind::Bool),
            ("percents-random-visits", "randomVisitsPercent", DisplayKind::Num),
            (
                "show-previous-bestmoves-in-enginegame",
                "showPreviousBestmoves",
                DisplayKind::Bool,
            ),
            (
                "show-previous-bestmoves-only-first-move",
                "previousBestmoveOnly",
                DisplayKind::Bool,
            ),
        ] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            let value = match kind {
                DisplayKind::Bool => as_bool(raw).map(Value::from),
                DisplayKind::Int(min, max) => as_i64(raw).filter(|n| (min..=max).contains(n)).map(Value::from),
                _ => as_f64(raw).map(Value::from),
            };
            self.match_value(format!("ui.{key}"), field, value, raw, false);
        }
        for (key, field) in [
            ("pk-black-time-mode", "firstTimeMode"),
            ("pk-white-time-mode", "secondTimeMode"),
        ] {
            let raw = ui.get(key);
            if raw.is_some() {
                handled.push(("ui", key));
            }
            let selected = raw.and_then(Value::as_str).map(|s| s.trim().to_ascii_uppercase());
            let mode = match selected.as_deref() {
                Some("ENGINE_OWNED") => "EngineOwned",
                Some("RAW_ADVANCED") => "RawAdvanced",
                Some("FIXED") => "Fixed",
                _ => {
                    if ui.get("pk-advance-time-settings").and_then(as_bool).unwrap_or(false) {
                        "RawAdvanced"
                    } else {
                        "Fixed"
                    }
                }
            };
            if raw.is_some() || ui.contains_key("pk-advance-time-settings") {
                self.match_value(
                    format!("ui.{key}"),
                    field,
                    Some(Value::from(mode)),
                    raw.unwrap_or(&Value::Null),
                    false,
                );
            }
        }
    }
    fn match_value(&mut self, key: String, field: &'static str, value: Option<Value>, raw: &Value, toolbar: bool) {
        if let Some(value) = value {
            self.imported(key, ImportTarget::EngineMatch, shown(&value));
            let value = DisplayValue {
                target: ImportTarget::EngineMatch,
                field,
                value,
            };
            if toolbar {
                self.mapped.match_toolbar.push(value);
            } else {
                self.mapped.matches.push(value);
            }
        } else {
            self.invalid(key, ImportTarget::EngineMatch, raw);
        }
    }
    pub(super) fn match_toolbar(&mut self, values: &[Value], name: &str) {
        for (index, field) in [
            (17, "firstMoveVisits"),
            (18, "secondMoveVisits"),
            (19, "firstSeconds"),
            (20, "firstVisits"),
            (21, "secondVisits"),
            (22, "batchLimit"),
            (23, "batchEnabled"),
            (24, "continuePosition"),
            (25, "firstMoveVisitsEnabled"),
            (26, "visitsEnabled"),
            (27, "timeEnabled"),
            (30, "autoSave"),
            (31, "kind"),
            (35, "exchangeColors"),
            (36, "maxMoves"),
            (37, "maxMovesEnabled"),
            (38, "secondSeconds"),
            (44, "randomMovesEnabled"),
            (45, "randomMoves"),
            (46, "randomWinrateDiff"),
            (49, "saveWinrate"),
        ] {
            let raw = &values[index];
            let value = match index {
                30 | 35 | 37 | 44 | 49 => as_bool(raw).map(Value::from),
                31 => as_bool(raw).map(|v| Value::from(if v { "Genmove" } else { "Analysis" })),
                46 => as_f64(raw).map(Value::from),
                23..=26 => as_i64(raw).map(|v| Value::from(v > 0)),
                27 => as_i64(raw).map(|_| Value::from(true)),
                19 | 38 => as_i64(raw).map(|v| Value::from(if v > 0 { v } else { -1 })),
                _ => as_i64(raw).and_then(|v| u32::try_from(v.max(0)).ok()).map(Value::from),
            };
            self.match_value(format!("{name}[{index}]"), field, value, raw, true);
        }
    }
}
