//! One dialog/request owner. Frozen evidence stays bound across navigation; document switches
//! cancel it. Only the stamped, still-live target may receive the completed SGF block.
use super::Workbench;
use crate::api::*;
use crate::commentary::{self, codec, evidence, prompt, CommentaryFailure, CommentaryJob, JobConnection, Message};
use crate::record::{DocumentEdit, NodeId, ROOT};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
struct Request {
    id: u64,
    document: u64,
    node: NodeId,
    stamp: u64,
    cancel: Arc<AtomicBool>,
    model: String,
    question: String,
}
pub(super) struct CommentaryFlow {
    document: u64,
    open: bool,
    generation: u64,
    request: Option<Request>,
    jobs: Vec<CommentaryJob>,
    mode: CommentaryMode,
    scope: CommentaryText,
    output: String,
    last_completed_output: String,
    context: Vec<Message>,
    positions: Vec<evidence::Position>,
    target: Option<NodeId>,
    status: CommentaryStatus,
    facts: CommentaryConnectionFacts,
    write_to_sgf: bool,
    restore_question: Option<String>,
    target_changed: bool,
}
impl Default for CommentaryFlow {
    fn default() -> Self {
        Self {
            document: 0,
            open: false,
            generation: 0,
            request: None,
            jobs: Vec::new(),
            mode: CommentaryMode::Next,
            scope: CommentaryText::default(),
            output: String::new(),
            last_completed_output: String::new(),
            context: Vec::new(),
            positions: Vec::new(),
            target: None,
            status: CommentaryStatus::default(),
            facts: CommentaryConnectionFacts::default(),
            write_to_sgf: true,
            restore_question: None,
            target_changed: false,
        }
    }
}
impl CommentaryFlow {
    fn status(&mut self, key: &str, args: Vec<String>, tone: CommentaryTone) {
        self.status = CommentaryStatus {
            key: key.into(),
            args,
            tone,
            detail_key: String::new(),
        };
    }
    fn cancel(&mut self) {
        if let Some(r) = self.request.take() {
            r.cancel.store(true, Ordering::Release);
            if !r.question.is_empty() {
                self.restore_question = Some(r.question);
            }
            self.status("Teacher.status.cancelled", vec![], CommentaryTone::Warning);
        }
        self.generation += 1;
        self.jobs.clear();
    }
    fn clear(&mut self) {
        self.output.clear();
        self.last_completed_output.clear();
        self.context.clear();
        self.positions.clear();
        self.target = None;
        self.scope = CommentaryText::default();
        self.restore_question = None;
        self.target_changed = false;
    }
}
impl Workbench {
    pub(super) fn commentary_document_changed(&mut self) {
        if self.commentary.document == self.doc.id {
            return;
        }
        self.commentary.cancel();
        self.commentary.clear();
        self.commentary.document = self.doc.id;
        if self.commentary.open {
            self.refresh_commentary();
        }
    }
    fn refresh_commentary(&mut self) {
        if self.commentary.request.is_some() || !self.commentary.output.is_empty() {
            return;
        }
        let node = self.doc.record.node(self.cursor);
        self.commentary.scope = CommentaryText {
            key: "Teacher.position.commentary".into(),
            args: vec![node.move_number.to_string()],
        };
        // Java refreshFromBoard drops abandoned frozen evidence in both branches.
        self.commentary.context.clear();
        self.commentary.positions.clear();
        if let Some(saved) = codec::extract(&node.comment) {
            self.commentary.target = Some(self.cursor);
            self.commentary.output = saved.clone();
            self.commentary.last_completed_output = saved;
            self.commentary.scope = CommentaryText {
                key: "Teacher.position.commentary".into(),
                args: vec![node.move_number.to_string()],
            };
            self.commentary
                .status("Teacher.status.savedLoaded", vec![], CommentaryTone::Success);
        } else if let Some(p) = evidence::current(
            &self.doc.record,
            self.cursor,
            self.store.get(&self.doc.record, self.cursor),
        ) {
            self.commentary.status(
                "Teacher.status.ready",
                vec![p.move_number.to_string(), p.candidates.len().to_string()],
                CommentaryTone::Neutral,
            );
        } else {
            self.commentary
                .status("Teacher.status.needsAnalysis", vec![], CommentaryTone::Warning);
        }
    }
    pub fn set_commentary_facts(&mut self, facts: CommentaryConnectionFacts) {
        self.commentary.facts = facts;
    }
    pub fn set_commentary_connection(
        &mut self,
        provider: CommentaryProvider,
        base_url: String,
        model: String,
        remember: bool,
    ) -> Result<(), ApiError> {
        let (base_url, model) = if provider == CommentaryProvider::ApiKey {
            (
                commentary::settings::validate_base_url(&base_url).map_err(commentary_error)?,
                commentary::settings::validate_model(&model).map_err(commentary_error)?,
            )
        } else {
            (base_url, model)
        };
        self.commit_config(move |c| {
            c.commentary.provider = provider;
            c.commentary.base_url = base_url;
            c.commentary.model = model;
            c.commentary.remember_api_key = remember;
        })
    }
    pub fn take_commentary_jobs(&mut self) -> Vec<CommentaryJob> {
        std::mem::take(&mut self.commentary.jobs)
    }
    pub fn commentary(&mut self, op: CommentaryCommand) -> Result<CommandResult, ApiError> {
        self.commentary_document_changed();
        match op {
            CommentaryCommand::Open => {
                if !self.commentary.open {
                    self.commentary.clear();
                    self.commentary.write_to_sgf = true;
                    self.commentary.mode = CommentaryMode::Next;
                }
                self.commentary.open = true;
                self.refresh_commentary();
            }
            CommentaryCommand::Close => {
                self.commentary.cancel();
                self.commentary.open = false;
                self.commentary.clear();
            }
            CommentaryCommand::Stop => self.commentary.cancel(),
            CommentaryCommand::SetWriteToSgf { enabled } => self.commentary.write_to_sgf = enabled,
            CommentaryCommand::ReloadKnowledge => commentary::knowledge::reload(),
            CommentaryCommand::SavePreferences { preferences } => {
                let preferences = preferences.normalized();
                self.commit_config(move |c| c.commentary.preferences = preferences)?;
            }
            CommentaryCommand::Start { mode, first, last } => {
                if !self.commentary_admitted() {
                    return Ok(CommandResult::Unchanged);
                }
                if !self.commentary_ready() {
                    return Ok(CommandResult::Refused {
                        message_key: "Teacher.empty.connect".into(),
                    });
                }
                let rec = &self.doc.record;
                let (first, last) = (first.min(last).max(1), first.max(last).max(1));
                let target = if mode == CommentaryMode::Whole {
                    ROOT
                } else {
                    self.cursor
                };
                let range = match mode {
                    CommentaryMode::Next => evidence::Range {
                        positions: evidence::current(rec, self.cursor, self.store.get(rec, self.cursor))
                            .into_iter()
                            .collect(),
                        analyzed_positions: 1,
                        omitted_positions: 0,
                    },
                    CommentaryMode::Range => evidence::main_line(rec, first, last, |n| self.store.get(rec, n)),
                    CommentaryMode::Whole => evidence::whole_game(rec, |n| self.store.get(rec, n)),
                };
                if range.positions.is_empty() {
                    let key = if mode == CommentaryMode::Next {
                        "Teacher.status.needsAnalysis"
                    } else {
                        "Teacher.status.rangeNeedsAnalysis"
                    };
                    self.commentary.status(key, vec![], CommentaryTone::Warning);
                    return Ok(CommandResult::Unchanged);
                }
                let context = if mode == CommentaryMode::Next {
                    prompt::for_position(
                        &range.positions[0],
                        self.active_language,
                        &self.config.commentary.preferences,
                    )
                } else {
                    prompt::for_range(&range, mode, self.active_language, &self.config.commentary.preferences)
                };
                self.commentary.mode = mode;
                self.commentary.last_completed_output.clear();
                self.commentary.scope = match mode {
                    CommentaryMode::Next => CommentaryText {
                        key: "Teacher.position.commentary".into(),
                        args: vec![rec.node(target).move_number.to_string()],
                    },
                    CommentaryMode::Range => CommentaryText {
                        key: "Teacher.position.range".into(),
                        args: vec![first.to_string(), last.to_string()],
                    },
                    CommentaryMode::Whole => CommentaryText {
                        key: "Teacher.position.whole".into(),
                        args: vec![],
                    },
                };
                if mode == CommentaryMode::Next {
                    self.commentary
                        .status("Teacher.status.requesting", vec![], CommentaryTone::Running);
                } else {
                    self.commentary.status(
                        "Teacher.status.evidenceReady",
                        vec![
                            range.positions.len().to_string(),
                            range.analyzed_positions.to_string(),
                            range.omitted_positions.to_string(),
                        ],
                        CommentaryTone::Running,
                    );
                }
                self.begin_commentary(target, context.clone(), context, range.positions, String::new())?;
            }
            CommentaryCommand::Ask { question } => {
                let question = commentary::java_trim(&question).to_owned();
                if question.is_empty() || !self.commentary_admitted() {
                    return Ok(CommandResult::Unchanged);
                }
                if !self.commentary_ready() {
                    return Ok(CommandResult::Refused {
                        message_key: "Teacher.empty.connect".into(),
                    });
                }
                // Java TeacherDialog.askFollowUp: frozen evidence keeps the original request target;
                // only a rebuild without context falls back to the cursor.
                let (target, context, positions) = match self.commentary.target {
                    Some(target) if !self.commentary.context.is_empty() => (
                        target,
                        self.commentary.context.clone(),
                        self.commentary.positions.clone(),
                    ),
                    retained => {
                        let target = retained
                            .filter(|n| self.doc.record.contains(*n))
                            .filter(|_| !self.commentary.last_completed_output.is_empty())
                            .unwrap_or(self.cursor);
                        let Some(p) =
                            evidence::current(&self.doc.record, target, self.store.get(&self.doc.record, target))
                        else {
                            self.commentary
                                .status("Teacher.status.needsAnalysis", vec![], CommentaryTone::Warning);
                            return Ok(CommandResult::Unchanged);
                        };
                        (
                            target,
                            prompt::for_position(&p, self.active_language, &self.config.commentary.preferences),
                            vec![p],
                        )
                    }
                };
                let messages = prompt::for_follow_up(
                    &context,
                    &self.commentary.last_completed_output,
                    &question,
                    self.active_language,
                    &self.config.commentary.preferences,
                );
                self.commentary
                    .status("Teacher.status.requesting", vec![], CommentaryTone::Running);
                self.begin_commentary(target, messages, context, positions, question)?;
            }
        }
        Ok(CommandResult::Done)
    }
    fn commentary_admitted(&self) -> bool {
        self.commentary.open
            && self.commentary.request.is_none()
            && self.commentary.facts.loaded
            && self.commentary.facts.settings_usable
    }
    fn commentary_account(&self) -> Option<&CommentaryAccount> {
        let facts = &self.commentary.facts.chatgpt;
        facts.accounts.iter().find(|a| Some(&a.id) == facts.active.as_ref())
    }
    fn commentary_ready(&self) -> bool {
        let c = &self.config.commentary;
        let facts = &self.commentary.facts;
        if !facts.loaded || !facts.settings_usable {
            return false;
        }
        match c.provider {
            CommentaryProvider::Unselected => false,
            CommentaryProvider::ApiKey => {
                facts.api_key_present && !commentary::stream::blank(&c.model) && !commentary::stream::blank(&c.base_url)
            }
            CommentaryProvider::ChatGpt => self.commentary_account().is_some_and(|a| {
                a.signed_in && a.authorized && !commentary::stream::blank(&a.model) && !a.credentials_unavailable
            }),
        }
    }
    fn begin_commentary(
        &mut self,
        target: NodeId,
        messages: Vec<Message>,
        context: Vec<Message>,
        positions: Vec<evidence::Position>,
        question: String,
    ) -> Result<(), ApiError> {
        let c = &self.config.commentary;
        let (connection, model) = match c.provider {
            CommentaryProvider::ApiKey => {
                let base_url = commentary::settings::validate_base_url(&c.base_url).map_err(commentary_error)?;
                let model = commentary::settings::validate_model(&c.model).map_err(commentary_error)?;
                (
                    JobConnection::ApiKey {
                        base_url,
                        model: model.clone(),
                    },
                    model,
                )
            }
            CommentaryProvider::ChatGpt => {
                let a = self.commentary_account().expect("ready account");
                (
                    JobConnection::ChatGpt {
                        account: a.id.clone(),
                        model: a.model.clone(),
                        effort: a.effort.clone(),
                    },
                    a.model.clone(),
                )
            }
            CommentaryProvider::Unselected => return Ok(()),
        };
        self.commentary.generation += 1;
        let id = self.commentary.generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.commentary.request = Some(Request {
            id,
            document: self.doc.id,
            node: target,
            stamp: self.doc.record.node(target).stamp,
            cancel: cancel.clone(),
            model,
            question,
        });
        self.commentary.output.clear();
        self.commentary.context = context;
        self.commentary.positions = positions.clone();
        self.commentary.target = Some(target);
        self.commentary.target_changed = false;
        self.commentary.restore_question = None;
        self.commentary.jobs.push(CommentaryJob {
            id,
            messages,
            positions,
            connection,
            cancel,
        });
        Ok(())
    }
    /// Returns false for a stale/closed/cancelled outcome; navigation alone is not stale.
    pub fn commentary_finished(&mut self, id: u64, outcome: Result<String, CommentaryFailure>) -> bool {
        self.commentary_document_changed();
        if !self.commentary.open
            || !self
                .commentary
                .request
                .as_ref()
                .is_some_and(|r| r.id == id && r.document == self.doc.id && !r.cancel.load(Ordering::Acquire))
        {
            return false;
        }
        let request = self.commentary.request.take().expect("admitted request");
        let outcome = outcome.and_then(|s| {
            let s = commentary::java_trim(&s);
            if s.is_empty() {
                Err(CommentaryFailure::new("AI service returned an empty response."))
            } else {
                Ok(s.to_owned())
            }
        });
        match outcome {
            Ok(result) => {
                self.commentary.last_completed_output = result.clone();
                self.commentary.output = result.clone();
                let notes = commentary::verify::verify(&result, &self.commentary.positions);
                let notes: Vec<_> = notes.violations.iter().chain(&notes.warnings).take(4).collect();
                if !notes.is_empty() {
                    self.commentary.output.push_str("\n\n> ");
                    self.commentary.output.push_str(verifier_title(self.active_language));
                    self.commentary.output.push_str(": ");
                    self.commentary
                        .output
                        .push_str(&notes.into_iter().map(String::as_str).collect::<Vec<_>>().join("; "));
                }
                let mut saved = false;
                if self.commentary.write_to_sgf {
                    if self.doc.record.contains(request.node)
                        && self.doc.record.node(request.node).stamp == request.stamp
                    {
                        let old = &self.doc.record.node(request.node).comment;
                        match codec::upsert(
                            old,
                            &result,
                            &request.model,
                            &chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true),
                        ) {
                            Ok(comment) => {
                                saved = self
                                    .apply_edit(DocumentEdit::SetComment {
                                        node: request.node,
                                        comment,
                                    })
                                    .is_ok();
                            }
                            Err(e) => {
                                self.commentary
                                    .status("Teacher.status.failed", vec![e.message], CommentaryTone::Error);
                                if !request.question.is_empty() {
                                    self.commentary.restore_question = Some(request.question);
                                }
                                return true;
                            }
                        }
                    } else {
                        self.commentary.target_changed = true;
                    }
                }
                self.commentary.status(
                    if saved {
                        "Teacher.status.completedSaved"
                    } else {
                        "Teacher.status.completed"
                    },
                    vec![],
                    CommentaryTone::Success,
                );
            }
            Err(error) => {
                self.commentary.status(
                    if error.cancelled {
                        "Teacher.status.cancelled"
                    } else {
                        "Teacher.status.failed"
                    },
                    if error.cancelled { vec![] } else { vec![error.message] },
                    if error.cancelled {
                        CommentaryTone::Warning
                    } else {
                        CommentaryTone::Error
                    },
                );
                self.commentary.status.detail_key = error.message_key;
                if !request.question.is_empty() {
                    self.commentary.restore_question = Some(request.question);
                }
            }
        }
        true
    }
    pub(super) fn commentary_view(&self) -> CommentaryView {
        let f = &self.commentary;
        let running = f.request.is_some();
        let settings = &self.config.commentary;
        let mut last = ROOT;
        while let Some(n) = self.doc.record.node(last).children.first() {
            last = *n;
        }
        let model_status = if !f.facts.loaded {
            CommentaryText {
                key: "Teacher.status.loadingSettings".into(),
                args: vec![],
            }
        } else {
            match settings.provider {
                CommentaryProvider::Unselected => CommentaryText {
                    key: "Teacher.chatgpt.choose".into(),
                    args: vec![],
                },
                CommentaryProvider::ApiKey => CommentaryText {
                    key: if f.facts.api_key_present {
                        "Teacher.status.modelReady"
                    } else {
                        "Teacher.status.needsKey"
                    }
                    .into(),
                    args: if f.facts.api_key_present {
                        vec![settings.model.clone()]
                    } else {
                        vec![]
                    },
                },
                CommentaryProvider::ChatGpt => {
                    let a = self.commentary_account();
                    let connected = a.is_some_and(|a| a.signed_in && a.authorized);
                    CommentaryText {
                        key: if connected {
                            "Teacher.chatgpt.usingPlan"
                        } else {
                            "Teacher.chatgpt.notConnected"
                        }
                        .into(),
                        args: if connected {
                            a.map_or_else(Vec::new, |a| vec![a.model.clone()])
                        } else {
                            vec![]
                        },
                    }
                }
            }
        };
        CommentaryView {
            open: f.open,
            running,
            mode: f.mode,
            scope: f.scope.clone(),
            current_move: self.doc.record.node(self.cursor).move_number,
            main_line_last_move: self.doc.record.node(last).move_number.max(1),
            output: f.output.clone(),
            output_state: if !f.output.is_empty() {
                CommentaryOutputState::Output
            } else if running {
                CommentaryOutputState::Loading
            } else {
                CommentaryOutputState::Empty
            },
            evidence: f
                .positions
                .iter()
                .map(|p| prompt::format_position(p, self.active_language))
                .collect::<Vec<_>>()
                .join("\n\n"),
            status: f.status.clone(),
            model_status,
            connection_ready: self.commentary_ready(),
            settings_loaded: f.facts.loaded,
            settings_usable: f.facts.settings_usable,
            chatgpt_usage_visible: settings.provider == CommentaryProvider::ChatGpt
                && self.commentary_account().is_some_and(|a| a.signed_in && a.authorized),
            controls_enabled: f.facts.loaded && f.facts.settings_usable && !running,
            start_enabled: f.facts.loaded && !running,
            settings_enabled: f.facts.loaded && !running,
            stop_enabled: running,
            write_to_sgf: f.write_to_sgf,
            preferences: settings.preferences.clone(),
            connection: settings.clone(),
            facts: f.facts.clone(),
            restore_question: f.restore_question.clone(),
            target_changed: f.target_changed,
        }
    }
    pub(super) fn refresh_commentary_view(&mut self) {
        self.commentary_document_changed();
        if self.commentary.open
            && matches!(
                self.commentary.status.key.as_str(),
                "" | "Teacher.status.ready" | "Teacher.status.needsAnalysis"
            )
        {
            self.refresh_commentary();
        }
    }
    pub(super) fn take_commentary_restore(&mut self) -> Option<String> {
        self.commentary.restore_question.take()
    }
}
fn commentary_error(e: CommentaryFailure) -> ApiError {
    ApiError::new(ErrorKind::InvalidArgument, e.to_string())
}
fn verifier_title(language: crate::settings::Language) -> &'static str {
    prompt::localized(language, "verifyNote")
}
