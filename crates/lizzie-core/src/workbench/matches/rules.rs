//! 本局规则 (Java `MatchRulesSnapshot`, `MatchRulesAdmission`, `MatchRulesTexts`): what each
//! participant reported about the game's target rules, whether the game may start, and the one
//! text the rules caption, 本局规则详情 and the saved record's 本局规则 block are made from.
use super::*;
use crate::engine::rules::KataRules;

/// `EngineRulesResult.Status`, as far as a match observes it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RulesStatus {
    #[default]
    Pending,
    Confirmed,
    Unconfirmed,
    SetFailed,
    QueryFailed,
}

/// `EngineRulesResult.Reason`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum RulesReason {
    #[default]
    None,
    QueryUnsupported,
    QueryRejected,
    QueryTimeout,
    InvalidReadback,
    SetRejected,
    SetTimeout,
}

impl RulesReason {
    fn key(self) -> Option<&'static str> {
        Some(match self {
            RulesReason::None => return None,
            RulesReason::QueryUnsupported => "MatchRules.reason.QUERY_UNSUPPORTED",
            RulesReason::QueryRejected => "MatchRules.reason.QUERY_REJECTED",
            RulesReason::QueryTimeout => "MatchRules.reason.QUERY_TIMEOUT",
            RulesReason::InvalidReadback => "MatchRules.reason.INVALID_READBACK",
            RulesReason::SetRejected => "MatchRules.reason.SET_REJECTED",
            RulesReason::SetTimeout => "MatchRules.reason.SET_TIMEOUT",
        })
    }
}

/// One participant's rules for the game (`MatchRulesPrepareState.Side`): its commands, the rules
/// it had before the overlay, the last rules it reported and the result of the current operation.
/// A failed operation keeps the earlier report as `observed`; it is not a current success.
#[derive(Clone, Debug, Default)]
pub(super) struct SideRules {
    pub(super) can_set: bool,
    pub(super) can_query: bool,
    /// The game's own `kata-get-rules` answer, before the overlay.
    pub(super) original: Option<KataRules>,
    /// `Side.modifiedOrUncertain`: the game's rules were sent to the engine.
    pub(super) modified: bool,
    pub(super) observed: Option<KataRules>,
    pub(super) status: RulesStatus,
    pub(super) reason: RulesReason,
}

/// What giving a participant back does about its rules (`EngineManager.restoreMatchRulesSide`).
pub(super) enum RulesHandback<'a> {
    /// The game's rules were never sent: nothing to undo.
    Untouched,
    /// Set these rules again and read them back.
    Restore(&'a KataRules),
    /// The game's rules were sent, but the rules before them are unknown or incomplete: the
    /// engine cannot be put back and is left unrestored.
    Unknown,
}

impl SideRules {
    /// A side about to be prepared: an engine without `kata-get-rules` cannot confirm any rules
    /// (and is not given them, as in Java); `last_known` is what the engine reported before;
    /// `modified` whether the game's rules were sent to it.
    pub(super) fn prepare(can_query: bool, can_set: bool, modified: bool, last_known: Option<KataRules>) -> Self {
        let mut side = SideRules {
            can_set,
            can_query,
            modified,
            observed: last_known,
            ..SideRules::default()
        };
        if !can_query {
            side.status = RulesStatus::Unconfirmed;
            side.reason = RulesReason::QueryUnsupported;
        }
        side
    }

    /// The operation ended without a result. Only a pending side changes: a settled one was not
    /// asked anything further in Java.
    pub(super) fn fail(&mut self, status: RulesStatus, reason: RulesReason) {
        if self.status == RulesStatus::Pending {
            self.status = status;
            self.reason = reason;
        }
    }

    /// The engine answered `kata-get-rules` with `rules`. `original` marks the answer before the
    /// overlay (the caller keeps it as `original`); with a setter the side stays pending until
    /// the overlay is read back.
    pub(super) fn report(&mut self, rules: KataRules, original: bool) {
        if self.status != RulesStatus::Pending {
            return;
        }
        let complete = rules.has_required_fields();
        self.observed = Some(rules);
        if !original || !self.can_set || !complete {
            self.status = RulesStatus::Confirmed;
        }
    }

    /// Whether the side settled, or answered the game's rules query, during the preparation.
    pub(super) fn answered(&self) -> bool {
        self.status != RulesStatus::Pending || self.original.is_some()
    }

    /// What its handback has to do about the engine's rules.
    pub(super) fn handback(&self) -> RulesHandback<'_> {
        match &self.original {
            _ if !self.modified => RulesHandback::Untouched,
            Some(original) if original.has_required_fields() => RulesHandback::Restore(original),
            _ => RulesHandback::Unknown,
        }
    }

    /// `MatchRulesAdmission.hardFailureReason`, as the failure message key
    /// (`MatchRulesTexts.failureMessage`).
    fn hard_failure(&self) -> Option<&'static str> {
        match self.status {
            RulesStatus::SetFailed => Some("MatchRules.failed.setFailed"),
            RulesStatus::QueryFailed => Some("MatchRules.failed.queryFailed"),
            RulesStatus::Confirmed
                if self.can_query && self.observed.as_ref().is_some_and(|r| !r.has_required_fields()) =>
            {
                Some("MatchRules.failed.incomplete")
            }
            _ => None,
        }
    }

    /// `SideResult.unsupportedConfirmation`: the engine cannot confirm the rules; the game needs
    /// the user's consent.
    pub(super) fn unsupported_confirmation(&self) -> bool {
        !self.can_query && (self.status == RulesStatus::Unconfirmed || self.reason == RulesReason::QueryUnsupported)
    }

    fn matches(&self, target: &KataRules) -> bool {
        self.observed.as_ref() == Some(target)
    }

    /// `MatchRulesTexts.statusLabel`.
    fn status_key(&self) -> &'static str {
        if self.status == RulesStatus::Confirmed && self.reason == RulesReason::None {
            return "MatchRules.status.CONFIRMED";
        }
        if self.status == RulesStatus::Unconfirmed || self.reason == RulesReason::QueryUnsupported {
            return "MatchRules.unconfirmed";
        }
        if let Some(key) = self.reason.key() {
            return key;
        }
        match self.status {
            RulesStatus::Pending => "MatchRules.status.PENDING",
            RulesStatus::SetFailed => "MatchRules.status.SET_FAILED",
            RulesStatus::QueryFailed => "MatchRules.status.QUERY_FAILED",
            RulesStatus::Confirmed | RulesStatus::Unconfirmed => "MatchRules.unconfirmed",
        }
    }

    /// `MatchRulesTexts.reasonOrStatus`.
    fn reason_or_status_key(&self) -> &'static str {
        self.reason.key().unwrap_or_else(|| self.status_key())
    }
}

/// `MatchRulesAdmission.Decision`'s outcome once both sides settled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Admission {
    Confirmed,
    /// Some engine cannot confirm the rules: the game starts only with the user's consent.
    Unverified,
    /// The game cannot start; the failure message key.
    Rejected(&'static str),
}

/// `MatchRulesAdmission.decide`.
pub(super) fn admission(target: Option<&KataRules>, black: &SideRules, white: &SideRules) -> Admission {
    if let Some(key) = black.hard_failure().or_else(|| white.hard_failure()) {
        return Admission::Rejected(key);
    }
    let Some(target) = target else {
        return Admission::Rejected("MatchRules.failed.mismatch");
    };
    if !black.unsupported_confirmation() && !white.unsupported_confirmation() {
        return if black.matches(target) && white.matches(target) {
            Admission::Confirmed
        } else {
            Admission::Rejected("MatchRules.failed.mismatch")
        };
    }
    if [black, white]
        .iter()
        .any(|side| side.can_query && !side.matches(target))
    {
        return Admission::Rejected("MatchRules.failed.mismatch");
    }
    Admission::Unverified
}

/// `MatchRulesSnapshot.Phase`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RulesPhase {
    Preparing,
    Failed,
    Playing,
    Completed,
}

/// `MatchRulesSnapshot`: the game's rules as frozen at one moment; later engine replies never
/// change a published snapshot.
#[derive(Clone, Debug)]
pub(super) struct RulesSnapshot {
    pub(super) phase: RulesPhase,
    target: KataRules,
    /// Black, then White: the engine's label (`MatchRulesTexts.identityLabel`, its command's
    /// credentials masked) and its rules.
    sides: [(String, SideRules); 2],
    confirmed: bool,
}

impl RulesSnapshot {
    /// The game's snapshot from its participants. A preparing game shows idle sides, as Java
    /// publishes nothing between the start and the decision.
    pub(super) fn of(run: &Run, phase: RulesPhase, confirmed: bool) -> Option<RulesSnapshot> {
        let target = KataRules::parse(&run.settings.rules)?;
        let side = |stone| {
            let index = side_index(run.first_is_black, stone);
            let identity = if index == 0 {
                &run.settings.first
            } else {
                &run.settings.second
            };
            let commands = crate::sanitize::sanitize(&identity.commands);
            let label = if identity.name.is_empty() {
                commands
            } else if commands.is_empty() {
                identity.name.clone()
            } else {
                format!("{} ({commands})", identity.name)
            };
            let rules = if phase == RulesPhase::Preparing {
                SideRules::default()
            } else {
                run.sides[index].rules.clone()
            };
            (label, rules)
        };
        Some(RulesSnapshot {
            phase,
            target,
            sides: [side(Stone::Black), side(Stone::White)],
            confirmed,
        })
    }
}

impl Workbench {
    /// `MatchRuleOption.displayName`, else 自定义: the rules decide the name, whichever entry the
    /// dialog showed.
    pub(super) fn match_rule_name(&self, rules: &KataRules) -> String {
        self.match_text(rules.match_name_key())
    }

    /// `MatchRulesTexts.namedFields`.
    fn match_rules_named_fields(&self, rules: &KataRules) -> String {
        let name = self.match_rule_name(rules);
        let fields = rules.field_summary();
        if fields.is_empty() {
            name
        } else {
            format!("{name} ({fields})")
        }
    }

    fn match_rules_color(&self, index: usize) -> String {
        self.match_text(if index == 0 {
            "MatchRules.black"
        } else {
            "MatchRules.white"
        })
    }

    /// `MatchRulesSnapshot.mainSummary`: the rules caption while the snapshot can be inspected.
    pub(super) fn match_rules_summary(&self, snapshot: &RulesSnapshot) -> String {
        if snapshot.phase == RulesPhase::Preparing {
            return self.match_text("MatchRules.checking");
        }
        if snapshot.confirmed {
            return self.match_rule_name(&snapshot.target);
        }
        let side = |index: usize| {
            let rules = &snapshot.sides[index].1;
            let summary = match &rules.observed {
                Some(observed)
                    if rules.status == RulesStatus::Unconfirmed || rules.reason == RulesReason::QueryUnsupported =>
                {
                    format!(
                        "{} ({})",
                        self.match_rule_name(observed),
                        self.match_text("MatchRules.unconfirmed")
                    )
                }
                Some(observed) => self.match_rule_name(observed),
                None => self.match_text(rules.status_key()),
            };
            format!("{}: {summary}", self.match_rules_color(index))
        };
        format!("{} / {}", side(0), side(1))
    }

    /// `MatchRulesTexts.details`: the 本局规则详情 text, and the body of the saved record's block.
    pub(super) fn match_rules_details(&self, snapshot: &RulesSnapshot) -> String {
        let text = |key| self.match_text(key);
        let mut lines = vec![
            text("MatchRules.details.target").replace("{0}", &self.match_rule_name(&snapshot.target)),
            text(match snapshot.phase {
                RulesPhase::Preparing => "MatchRules.checking",
                RulesPhase::Failed => "MatchRules.failed",
                _ if snapshot.confirmed => "MatchRules.status.CONFIRMED",
                _ => "MatchRules.unconfirmed",
            }),
        ];
        for (index, (label, rules)) in snapshot.sides.iter().enumerate() {
            lines.push(
                text("MatchRules.details.side")
                    .replace("{0}", &self.match_rules_color(index))
                    .replace("{1}", label),
            );
            lines.push(text("MatchRules.details.status").replace("{0}", &text(rules.status_key())));
            match &rules.observed {
                Some(observed) if rules.status == RulesStatus::Confirmed => {
                    lines.push(
                        text("MatchRules.details.actual").replace("{0}", &self.match_rules_named_fields(observed)),
                    );
                }
                observed => {
                    lines.push(
                        text("MatchRules.details.unavailable").replace("{0}", &text(rules.reason_or_status_key())),
                    );
                    // A failed operation's earlier report; in a failed preparation, a side still
                    // pending had its operation cut off, so its report is not current either.
                    let failed = match rules.status {
                        RulesStatus::SetFailed | RulesStatus::QueryFailed => true,
                        RulesStatus::Pending => snapshot.phase == RulesPhase::Failed,
                        _ => false,
                    };
                    if let Some(observed) = observed.as_ref().filter(|_| failed) {
                        lines.push(
                            text("MatchRules.details.lastObserved")
                                .replace("{0}", &self.match_rules_named_fields(observed)),
                        );
                    }
                }
            }
        }
        lines.join("\n")
    }

    /// `EngineGamePresentation.inspectableMatchRules`: the open document's own record first (the
    /// game played on it), else the running match's, else a start that is still preparing or
    /// failed.
    pub(super) fn inspectable_match_rules(&self) -> Option<&RulesSnapshot> {
        if let Some((_, record)) = self
            .matches
            .rules_record
            .as_ref()
            .filter(|(doc, _)| *doc == self.doc.id)
        {
            return Some(record);
        }
        let live = self.matches.rules.as_ref()?;
        (self.matches.run.is_some() || matches!(live.phase, RulesPhase::Preparing | RulesPhase::Failed)).then_some(live)
    }

    /// `LizzieFrame.inspectMatchRules`: the frozen text 本局规则详情 shows; no engine is asked.
    pub fn inspect_match_rules(&self) -> CommandResult {
        match self.inspectable_match_rules() {
            Some(snapshot) => CommandResult::MatchRulesDetails {
                text: self.match_rules_details(snapshot),
            },
            None => CommandResult::Unchanged,
        }
    }

    /// The caption the captured panel shows instead of the engine's rules.
    pub(in crate::workbench) fn match_rules_caption(&self) -> Option<String> {
        self.inspectable_match_rules()
            .map(|snapshot| self.match_rules_summary(snapshot))
    }
}
