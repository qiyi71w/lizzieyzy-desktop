use super::*;
use crate::humansl::{choose, passing};
use serde_json::json;

/// The readiness check may take as long as the engine's first GPU compilation.
const READY_SECONDS: u64 = 180;
/// How long KataGo may count a finished game (`COUNT_TIMEOUT`).
const COUNT_SECONDS: u64 = 20;
/// The AI's reply that gives the game up (Java `RESIGN`).
const RESIGN: &str = "resign";

impl Workbench {
    /// Seed the session's random source for reproducible controlled-engine scenarios.
    pub fn set_human_sl_random_seed(&mut self, seed: u64) {
        self.human_sl.random = seed.max(1);
    }
    pub(super) fn human_sl_random(&mut self) -> f64 {
        if self.human_sl.random == 0 {
            self.human_sl.random = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos() as u64
                | 1;
        }
        let mut x = self.human_sl.random;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.human_sl.random = x;
        (x >> 11) as f64 / ((1_u64 << 53) as f64)
    }
    pub(super) fn human_sl_schedule(&mut self, now: Instant) {
        let Some(s) = self.human_sl.session.as_ref() else {
            return;
        };
        if s.phase != HumanSlPhase::Playing
            || s.request.is_some()
            || s.result.is_some()
            || self.doc.record.node(s.live).to_play == s.human
        {
            return;
        }
        let first = Self::human_sl_first_phase(&s.settings);
        self.human_sl_request(first, now);
    }
    /// The level's first question: a search level searches, a policy level probes its policy.
    fn human_sl_first_phase(settings: &HumanSlTrainingSettings) -> Search {
        if settings.level.play().searches {
            Search::Play
        } else {
            Search::Probe
        }
    }
    fn human_sl_request_id(s: &Session, phase: &Search) -> String {
        let kind = match phase {
            Search::Ready => "ready",
            Search::Probe => "policy",
            Search::PassCheck => "pass",
            Search::Play | Search::Pacing => "play",
            Search::Count => "count",
        };
        format!("humansl-{kind}-{}-{}", s.id, s.generation)
    }
    /// Asks the companion `phase`'s first question about the game's position, within its budget:
    /// the readiness check's, the count's, or the level's per move.
    pub(super) fn human_sl_request(&mut self, phase: Search, now: Instant) {
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        s.generation += 1;
        s.failed = false;
        let budget = match phase {
            Search::Ready => Duration::from_secs(READY_SECONDS),
            Search::Count => Duration::from_secs(COUNT_SECONDS),
            _ => s.settings.level.play().move_budget(),
        };
        s.request = Some(Request {
            id: Self::human_sl_request_id(s, &phase),
            generation: s.generation,
            node: s.live,
            stamp: self.doc.record.node(s.live).stamp,
            started: now,
            deadline: now + budget,
            attempt: 0,
            phase,
            candidates: Vec::new(),
            human_pass: false,
            top_share: None,
            evaluation: None,
            selected: None,
            pace: now,
            button: false,
        });
        self.human_sl_send(now);
    }
    fn human_sl_ensure_worker(&mut self) -> Option<InstanceId> {
        let s = self.human_sl.session.as_ref()?;
        if let Some(worker) = s.worker.filter(|&worker| self.engines.worker_alive(worker)) {
            return Some(worker);
        }
        let worker = self.engines.start_worker(&s.command, "HumanSL").ok()?;
        self.engines.report_worker_diagnostics(worker);
        self.human_sl.session.as_mut()?.worker = Some(worker);
        Some(worker)
    }
    fn human_sl_send(&mut self, now: Instant) {
        let Some(worker) = self.human_sl_ensure_worker() else {
            self.human_sl_request_failed(now);
            return;
        };
        let rules = self.quick_rules();
        let button = match &rules {
            Value::String(name) => KataRules::parse(name),
            rules => KataRules::parse(&rules.to_string()),
        }
        .is_some_and(|r| r.has_button());
        let Some(s) = self.human_sl.session.as_ref() else {
            return;
        };
        let Some(r) = s.request.as_ref() else { return };
        let at = protocol::Position {
            record: &self.doc.record,
            node: r.node,
            komi: self.komi,
            rules,
            seconds: r.deadline.saturating_duration_since(now).as_secs_f64(),
        };
        let play = s.settings.level.play();
        let line = match r.phase {
            Search::Ready => protocol::policy_query(&r.id, at, play.profile, 1),
            Search::Probe => protocol::policy_query(&r.id, at, play.profile, protocol::ROOT_SYMMETRIES),
            Search::PassCheck => protocol::pass_check_query(&r.id, at),
            Search::Play => protocol::play_query(&r.id, at, play),
            Search::Count => protocol::count_query(&r.id, at),
            Search::Pacing => return,
        };
        if let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.request.as_mut()) {
            r.button = button;
        }
        if !self.engines.worker_send(worker, &line) {
            self.human_sl_request_failed(now);
        }
    }
    /// Whether the request still asks about the game's position as it stands: the same session
    /// generation, document, position and position meaning (its stamp). What the board shows
    /// does not matter.
    fn human_sl_request_current(&self) -> bool {
        self.human_sl.session.as_ref().is_some_and(|s| {
            s.request.as_ref().is_some_and(|r| {
                r.generation == s.generation
                    && s.doc == self.doc.id
                    && r.node == s.live
                    && self.doc.record.contains(r.node)
                    && self.doc.record.node(r.node).stamp == r.stamp
            })
        })
    }
    fn human_sl_request_failed(&mut self, now: Instant) {
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        if s.phase == HumanSlPhase::Preparing {
            self.human_sl_start_failed("HumanSL readiness request failed".into());
            return;
        }
        if s.request.is_none() {
            return;
        }
        if let Some(worker) = s.worker.take() {
            self.engines.terminate(worker);
        }
        // A count that fails or times out is not tried again: the game is not counted.
        if s.request.as_ref().is_some_and(|r| r.phase == Search::Count) {
            self.human_sl_counted(None, now);
            return;
        }
        let first = Self::human_sl_first_phase(&s.settings);
        let budget = s.settings.level.play().move_budget();
        if s.request.as_ref().is_some_and(|r| r.attempt == 0) {
            s.generation += 1;
            let id = Self::human_sl_request_id(s, &first);
            let r = s.request.as_mut().unwrap();
            r.attempt = 1;
            r.generation = s.generation;
            r.phase = first;
            r.id = id;
            r.candidates.clear();
            r.human_pass = false;
            r.top_share = None;
            r.evaluation = None;
            r.deadline = now + budget;
            self.human_sl_send(now);
        } else {
            s.request = None;
            s.failed = true;
            s.ai_ms = s.ai_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
            s.turn = now;
            self.human_sl.notices.push(HumanSlNotice::AiMoveFailed);
        }
    }
    /// A policy level's probe answered: its legal moves and, when the AI may pass, KataGo's own
    /// pass check next.
    fn human_sl_probed(&mut self, response: &Value, now: Instant) {
        let Some(node) = self
            .human_sl
            .session
            .as_ref()
            .and_then(|s| s.request.as_ref())
            .map(|r| r.node)
        else {
            return;
        };
        if protocol::human_policy(response).is_none() {
            self.human_sl_request_failed(now);
            return;
        }
        let candidates = protocol::policy(response, &self.doc.record, node, false);
        let may_pass = passing::allowed(&self.doc.record, node);
        let human_pass =
            may_pass && passing::human_would_pass(&protocol::policy(response, &self.doc.record, node, true));
        let s = self.human_sl.session.as_mut().unwrap();
        let id = Self::human_sl_request_id(s, &Search::PassCheck);
        let r = s.request.as_mut().unwrap();
        r.top_share = choose::top_share(&candidates);
        r.candidates = candidates;
        r.human_pass = human_pass;
        if may_pass {
            r.phase = Search::PassCheck;
            r.id = id;
            self.human_sl_send(now);
        } else {
            self.human_sl_sample_policy(now);
        }
    }
    /// A policy level's pass check answered: the AI passes when KataGo's search says so; where
    /// players of the level would mostly pass, it plays KataGo's move instead, what is left of
    /// their policy being noise; otherwise it samples the policy.
    fn human_sl_pass_checked(&mut self, response: &Value, now: Instant) {
        let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.request.as_mut()) else {
            return;
        };
        r.evaluation = protocol::root_evaluation(response);
        let (node, human_pass) = (r.node, r.human_pass);
        if passing::ends(response, &self.doc.record, node) {
            self.human_sl_chosen(Some("pass".into()), now);
        } else if let Some(vertex) = passing::search_move(response).filter(|_| human_pass) {
            let vertex = vertex.to_string();
            self.human_sl_chosen(Some(vertex), now);
        } else {
            self.human_sl_sample_policy(now);
        }
    }
    /// A policy level's move, sampled from its probe's human policy.
    fn human_sl_sample_policy(&mut self, now: Instant) {
        let random = self.human_sl_random();
        let Some(r) = self.human_sl.session.as_ref().and_then(|s| s.request.as_ref()) else {
            return;
        };
        let number = self.doc.record.node(r.node).move_number;
        let size = self.doc.record.size;
        let selected = choose::policy_move(&r.candidates, number, size.width, size.height, random).map(str::to_string);
        self.human_sl_chosen(selected, now);
    }
    /// A search level's search answered: the move genmove would have chosen from it.
    fn human_sl_searched(&mut self, response: &Value, now: Instant) {
        let Some(s) = self.human_sl.session.as_ref() else {
            return;
        };
        let Some(r) = s.request.as_ref() else { return };
        let (node, play) = (r.node, s.settings.level.play());
        if protocol::human_policy(response).is_none() {
            self.human_sl_request_failed(now);
            return;
        }
        let human = protocol::policy(response, &self.doc.record, node, false);
        let top_share = choose::top_share(&human);
        let may_pass = passing::allowed(&self.doc.record, node);
        let selected = if may_pass && passing::ends(response, &self.doc.record, node) {
            Some("pass".to_string())
        } else {
            let human = if may_pass {
                protocol::policy(response, &self.doc.record, node, true)
            } else {
                human
            };
            let infos = response["moveInfos"].as_array().map(Vec::as_slice).unwrap_or_default();
            let number = self.doc.record.node(node).move_number;
            let size = self.doc.record.size;
            let random = self.human_sl_random();
            choose::searched_move(infos, &human, play, number, (size.width, size.height), may_pass, random)
        };
        if let Some(r) = self.human_sl.session.as_mut().and_then(|s| s.request.as_mut()) {
            r.top_share = top_share;
            r.evaluation = protocol::root_evaluation(response);
        }
        self.human_sl_chosen(selected, now);
    }
    /// The move is chosen; it is played once the AI has "thought" for as long as a player of the
    /// level would over it, at the chosen pace, counted from the start of its turn. When KataGo
    /// has seen the AI lost for long enough, it resigns instead (`resignIfHopeless`).
    fn human_sl_chosen(&mut self, mut selected: Option<String>, now: Instant) {
        if selected.is_none() {
            self.human_sl_request_failed(now);
            return;
        }
        let random = self.human_sl_random();
        let area = self.doc.record.size.area();
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        let pace = s.settings.pace;
        let Some(r) = s.request.as_mut() else { return };
        let number = self.doc.record.node(r.node).move_number;
        if let Some((winrate, lead)) = r.evaluation {
            if s.resignation.should_resign(winrate, lead, number, area) {
                selected = Some(RESIGN.into());
            }
        }
        r.pace = r.started + humansl::think_time(r.top_share, number, area, random, pace);
        r.selected = selected;
        r.phase = Search::Pacing;
        if now >= r.pace {
            self.human_sl_apply_ai(now);
        }
    }
    /// The chosen reply is played on the game's position, which the board shows again if the
    /// player was browsing (Java `applyAiMoveIfCurrent`). The AI's resignation ends the game;
    /// its pass after the player's ends it too, and KataGo counts it (`applyAiMove`).
    fn human_sl_apply_ai(&mut self, now: Instant) {
        if !self.human_sl_request_current() {
            self.human_sl_lost_game();
            return;
        }
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        let live = s.live;
        let Some(r) = s.request.take() else { return };
        let Some(vertex) = r.selected else { return };
        s.ai_ms = s.ai_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
        s.turn = now;
        let human = s.human;
        self.browse_to(live);
        if vertex == RESIGN {
            self.human_sl.notices.push(HumanSlNotice::AiResigned);
            self.human_sl_end_game(HumanSlResult::Resigned { winner: human }, now);
            return;
        }
        let point = gtp::parse_vertex(self.doc.record.size, &vertex).ok().flatten();
        if point.is_some() && self.human_sl_commit(point) {
            return;
        }
        // A stale or illegal reply passes too, but only the AI's own pass may end the game.
        self.human_sl_commit(None);
        if vertex.trim().eq_ignore_ascii_case("pass") && self.human_sl_both_passed() {
            self.human_sl_count(now);
        } else {
            self.human_sl.notices.push(HumanSlNotice::AiPassed);
        }
    }
    fn human_sl_worker_line(&mut self, text: &str, now: Instant) {
        let Ok(response) = serde_json::from_str::<Value>(text) else {
            let line = text.to_ascii_lowercase();
            if let Some(s) = self.human_sl.session.as_mut() {
                if s.phase == HumanSlPhase::Preparing {
                    s.diagnostics.push_back(text.into());
                    while s.diagnostics.len() > 10 {
                        s.diagnostics.pop_front();
                    }
                    if line.contains("started, ready")
                        || line.contains("timing cache")
                            && (line.contains("saved new") || line.contains("using existing"))
                    {
                        s.stage = HumanSlStartupStage::CacheReady;
                    } else if line.contains("initializing (may take a long time)")
                        || line.contains("creating new timing cache")
                        || line.contains("building network")
                    {
                        s.stage = HumanSlStartupStage::OptimizingGpu;
                    } else if line.contains("analysis engine starting")
                        || line.contains("after dedups")
                        || line.contains("loaded model")
                    {
                        s.stage = HumanSlStartupStage::LoadingModels;
                    }
                }
            }
            return;
        };
        let Some(s) = self.human_sl.session.as_ref() else {
            return;
        };
        let Some(r) = s.request.as_ref() else { return };
        if response["id"].as_str() != Some(r.id.as_str()) || response["isDuringSearch"].as_bool() == Some(true) {
            return;
        }
        if !self.human_sl_request_current() {
            self.human_sl_lost_game();
            return;
        }
        if let Some(error) = response.get("error") {
            if r.phase == Search::Ready {
                self.human_sl_start_failed(error.as_str().map_or_else(|| error.to_string(), str::to_string));
            } else {
                self.human_sl_request_failed(now);
            }
            return;
        }
        match r.phase {
            Search::Ready if protocol::human_policy(&response).is_none() => {
                self.human_sl_start_failed("HumanSL model did not return humanPolicy".into())
            }
            Search::Ready => self.human_sl_prepared(now),
            Search::Probe => self.human_sl_probed(&response, now),
            Search::PassCheck => self.human_sl_pass_checked(&response, now),
            Search::Play => self.human_sl_searched(&response, now),
            Search::Count => self.human_sl_counted(Some(&response), now),
            Search::Pacing => {}
        }
    }
    pub(in crate::workbench) fn human_sl_deadline(&self) -> Option<Instant> {
        let s = self.human_sl.session.as_ref()?;
        if matches!(s.phase, HumanSlPhase::Stopping | HumanSlPhase::CleanupPending) {
            return None;
        }
        let request = s
            .request
            .as_ref()
            .map(|r| if r.phase == Search::Pacing { r.pace } else { r.deadline });
        Some(request.map_or(s.tick, |d| d.min(s.tick)))
    }
    pub(in crate::workbench) fn human_sl_tick(&mut self, now: Instant) -> bool {
        let Some(s) = self.human_sl.session.as_mut() else {
            return false;
        };
        if matches!(s.phase, HumanSlPhase::Stopping | HumanSlPhase::CleanupPending) {
            return false;
        }
        let changed = now >= s.tick;
        if changed {
            s.tick = now + Duration::from_secs(1);
        }
        let Some(r) = s.request.as_ref() else { return changed };
        if r.phase == Search::Pacing {
            if now >= r.pace {
                self.human_sl_apply_ai(now);
                return true;
            }
            return changed;
        }
        if now >= r.deadline {
            if let Some(worker) = s.worker {
                let _ = self.engines.worker_send(
                    worker,
                    &json!({"id":format!("cancel-{}",r.id),"action":"terminate","terminateId":r.id}).to_string(),
                );
            }
            if r.phase == Search::Ready {
                self.human_sl_start_failed(format!("HumanSL readiness timed out after {READY_SECONDS} seconds"));
            } else {
                self.human_sl_request_failed(now);
            }
            return true;
        }
        changed
    }
    pub(in crate::workbench) fn human_sl_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        let Some(s) = self.human_sl.session.as_ref() else {
            return Some(event);
        };
        let worker = s.worker;
        let restore = s.restore;
        match event {
            RuntimeEvent::WorkerLine { instance, line } if Some(instance) == worker => {
                self.human_sl_worker_line(&line, Instant::now());
                None
            }
            RuntimeEvent::WorkerExited {
                instance,
                message,
                output,
                ..
            } if Some(instance) == worker => {
                if s.phase == HumanSlPhase::Preparing {
                    self.human_sl_start_failed(format!(
                        "{message}\n{}",
                        output.iter().rev().take(10).cloned().collect::<Vec<_>>().join("\n")
                    ));
                } else if s.phase == HumanSlPhase::Playing {
                    let s = self.human_sl.session.as_mut().unwrap();
                    s.worker = None;
                    if s.request.as_ref().is_some_and(|r| r.phase != Search::Pacing) {
                        self.human_sl_request_failed(Instant::now());
                    }
                }
                None
            }
            RuntimeEvent::GameReleased { game, failure } if Some(game) == restore => {
                if failure.is_some() {
                    self.human_sl_cleanup_failed();
                } else {
                    self.human_sl_ended();
                }
                None
            }
            other => Some(other),
        }
    }
}
