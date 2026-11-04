use super::*;
use serde_json::json;

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
            || self.doc.record.node(self.cursor).to_play == s.human
        {
            return;
        }
        self.human_sl_request(Search::Policy, now);
    }
    pub(super) fn human_sl_request(&mut self, phase: Search, now: Instant) {
        let pace = Duration::from_millis(800 + (self.human_sl_random() * 3200.0) as u64);
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        s.generation += 1;
        s.failed = false;
        let seconds = if phase == Search::Ready {
            180
        } else {
            s.settings.move_time_seconds.max(2)
        };
        let id = format!(
            "humansl-{}-{}-{}",
            if phase == Search::Ready { "ready" } else { "policy" },
            s.id,
            s.generation
        );
        s.request = Some(Request {
            id,
            generation: s.generation,
            doc: self.doc.id,
            node: self.cursor,
            stamp: self.doc.record.node(self.cursor).stamp,
            position_context: self.human_sl.position_context,
            sent: now,
            deadline: now + Duration::from_secs(u64::from(seconds)),
            pace: now + pace,
            attempt: 0,
            phase,
            candidates: Vec::new(),
            allowed: Vec::new(),
            visits: 1,
            rounds: 0,
            evidence: None,
            selected: None,
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
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        let Some(r) = s.request.as_mut() else { return };
        r.sent = now;
        let line = protocol::query(
            &r.id,
            &self.doc.record,
            r.node,
            &s.settings,
            self.komi,
            rules,
            protocol::QueryBudget {
                visits: r.visits,
                allowed: &r.allowed,
                seconds: r.deadline.saturating_duration_since(now).as_secs_f64(),
                readiness: r.phase == Search::Ready,
            },
        );
        if !self.engines.worker_send(worker, &line) {
            self.human_sl_request_failed(now);
        }
    }
    fn human_sl_request_current(&self) -> bool {
        self.human_sl.session.as_ref().is_some_and(|s| {
            s.request.as_ref().is_some_and(|r| {
                r.generation == s.generation
                    && r.doc == self.doc.id
                    && r.position_context == self.human_sl.position_context
                    && r.node == self.cursor
                    && self.doc.record.node(self.cursor).stamp == r.stamp
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
        let Some(r) = s.request.as_mut() else { return };
        if r.evidence.is_some() {
            self.human_sl_choose(now);
            return;
        }
        if let Some(worker) = s.worker.take() {
            self.engines.terminate(worker);
        }
        if r.attempt == 0 {
            r.attempt = 1;
            s.generation += 1;
            r.generation = s.generation;
            r.id = format!("humansl-policy-{}-{}", s.id, s.generation);
            r.phase = Search::Policy;
            r.visits = 1;
            r.allowed.clear();
            r.candidates.clear();
            r.rounds = 0;
            r.deadline = now + Duration::from_secs(u64::from(s.settings.move_time_seconds.max(2)));
            self.human_sl_send(now);
        } else {
            s.request = None;
            s.failed = true;
            s.ai_ms = s.ai_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
            s.turn = now;
            self.human_sl.notices.push(HumanSlNotice::AiMoveFailed);
        }
    }
    fn human_sl_choose(&mut self, now: Instant) {
        let random = self.human_sl_random();
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        let Some(r) = s.request.as_mut() else { return };
        let Some(evidence) = r.evidence.as_ref() else {
            self.human_sl_request_failed(now);
            return;
        };
        let infos = evidence["moveInfos"].as_array().map(Vec::as_slice).unwrap_or_default();
        let number = self.doc.record.node(r.node).move_number;
        let selected = if number >= 200 && protocol::top(evidence).is_some_and(|v| v.eq_ignore_ascii_case("pass")) {
            Some("pass".into())
        } else {
            humansl::select(&r.candidates, infos, number, &protocol::profile(&s.settings), random)
                .filter(|v| number >= 200 || !v.eq_ignore_ascii_case("pass"))
        };
        if selected.is_none() {
            r.evidence = None;
            self.human_sl_request_failed(now);
            return;
        }
        r.selected = selected;
        r.phase = Search::Pacing;
        if now >= r.pace {
            self.human_sl_apply_ai(now);
        }
    }
    fn human_sl_apply_ai(&mut self, now: Instant) {
        if !self.human_sl_request_current() {
            self.human_sl_exit(true);
            return;
        }
        let Some(s) = self.human_sl.session.as_mut() else {
            return;
        };
        let Some(r) = s.request.take() else { return };
        let Some(vertex) = r.selected else { return };
        s.ai_ms = s.ai_ms.saturating_add(now.duration_since(s.turn).as_millis() as u64);
        s.turn = now;
        let point = gtp::parse_vertex(self.doc.record.size, &vertex).ok().flatten();
        let passed = point.is_none() || !self.human_sl_commit(point);
        if passed {
            self.human_sl_commit(None);
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
            self.human_sl_exit(true);
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
        if r.phase == Search::Ready {
            if response
                .get("humanPolicy")
                .or_else(|| response.get("rootInfo").and_then(|v| v.get("humanPolicy")))
                .is_none()
            {
                self.human_sl_start_failed("HumanSL model did not return humanPolicy".into());
            } else {
                self.human_sl_prepared(now);
            }
            return;
        }
        let phase = &r.phase;
        if *phase == Search::Policy {
            let number = self.doc.record.node(r.node).move_number;
            let candidates = protocol::policy(&response, &self.doc.record, r.node, false);
            let mut allowed: Vec<_> =
                humansl::candidate_pool(&protocol::policy(&response, &self.doc.record, r.node, number >= 200))
                    .into_iter()
                    .map(|m| m.vertex)
                    .collect();
            for m in humansl::candidate_pool(&candidates) {
                if !allowed.contains(&m.vertex) {
                    allowed.push(m.vertex);
                }
            }
            if let Some(top) = protocol::top(&response).filter(|v| number >= 200 || !v.eq_ignore_ascii_case("pass")) {
                if !allowed.iter().any(|v| v == top) {
                    allowed.push(top.into());
                }
            }
            // Include the ordinary policy argmax as an additional tactical verification candidate.
            if let Some(policy) = response["policy"].as_array() {
                if let Some((i, _)) = policy
                    .iter()
                    .enumerate()
                    .filter_map(|(i, v)| v.as_f64().map(|p| (i, p)))
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                {
                    if i < self.doc.record.size.area() {
                        let size = self.doc.record.size;
                        let vertex = gtp::vertex(
                            size,
                            Point {
                                x: (i % usize::from(size.width)) as u16,
                                y: (i / usize::from(size.width)) as u16,
                            },
                        );
                        if !allowed.contains(&vertex) {
                            allowed.push(vertex);
                        }
                    }
                }
            }
            if allowed.is_empty() {
                self.human_sl_request_failed(now);
                return;
            }
            let s = self.human_sl.session.as_mut().unwrap();
            let r = s.request.as_mut().unwrap();
            r.candidates = candidates;
            r.allowed = allowed;
            r.visits = protocol::visits(&s.settings);
            r.phase = Search::Verify;
            r.id = format!("humansl-verify-{}-{}", s.id, s.generation);
            self.human_sl_send(now);
            return;
        }
        if *phase != Search::Verify {
            return;
        }
        let s = self.human_sl.session.as_mut().unwrap();
        let r = s.request.as_mut().unwrap();
        let infos = response["moveInfos"].as_array().map(Vec::as_slice).unwrap_or_default();
        if infos.is_empty() {
            self.human_sl_request_failed(now);
            return;
        }
        let visits = |value: &Value| {
            value["moveInfos"].as_array().map_or(0, |moves| {
                moves.iter().filter_map(|m| m["visits"].as_u64()).sum::<u64>()
            })
        };
        if r.evidence.as_ref().is_some_and(|old| visits(&response) < visits(old)) {
            self.human_sl_choose(now);
            return;
        }
        let profile = protocol::profile(&s.settings);
        let deepen = protocol::elite(&profile) || humansl::needs_deep_verification(&r.candidates, infos, &profile);
        r.evidence = Some(response);
        if self.doc.record.node(r.node).move_number >= 200
            && r.evidence
                .as_ref()
                .and_then(protocol::top)
                .is_some_and(|v| v.eq_ignore_ascii_case("pass"))
        {
            self.human_sl_choose(now);
            return;
        }
        let next = humansl::adaptive_visits(
            r.visits,
            now.duration_since(r.sent),
            r.deadline.saturating_duration_since(now),
        );
        if deepen && r.rounds < 4 && next > r.visits {
            r.rounds += 1;
            r.visits = next;
            r.id = format!("humansl-tactical-{}-{}-{}", s.id, s.generation, r.rounds);
            self.human_sl_send(now);
        } else {
            self.human_sl_choose(now);
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
        if r.phase == Search::Pacing && now >= r.pace {
            self.human_sl_apply_ai(now);
            return true;
        }
        if now >= r.deadline {
            if let Some(worker) = s.worker {
                let _ = self.engines.worker_send(
                    worker,
                    &json!({"id":format!("cancel-{}",r.id),"action":"terminate","terminateId":r.id}).to_string(),
                );
            }
            if r.phase == Search::Ready {
                self.human_sl_start_failed("HumanSL readiness timed out after 180 seconds".into());
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
            } if Some(instance) == worker => {
                if s.phase == HumanSlPhase::Preparing {
                    self.human_sl_start_failed(format!(
                        "{message}\n{}",
                        output.iter().rev().take(10).cloned().collect::<Vec<_>>().join("\n")
                    ));
                } else if s.phase == HumanSlPhase::Playing {
                    let s = self.human_sl.session.as_mut().unwrap();
                    s.worker = None;
                    if let Some(request) = s.request.as_mut().filter(|r| r.phase != Search::Pacing) {
                        request.evidence = None;
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
