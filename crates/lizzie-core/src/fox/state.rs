const SECOND: u64 = 1_000_000_000;
const COLUMNS: &[u8] = b"ABCDEFGHJKLMNOPQRSTUVWXYZ";
const INVALID: &str = "Invalid Fox message parameters";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Placement {
    pub number: u32,
    pub x: u8,
    pub y: u8,
    pub color: char,
}
impl Placement {
    pub fn vertex(&self, size: u8) -> String {
        format!("{}{}", COLUMNS[self.x as usize] as char, size - self.y)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Rules {
    pub size: u8,
    pub main_seconds: u32,
    pub byo_seconds: u32,
    pub periods: u32,
    pub komi: f64,
    pub setup: Vec<Placement>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clock {
    pub main_seconds: u32,
    pub byo_seconds: u32,
    pub periods: u32,
    pub received_nanos: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Action {
    pub placement: Option<Placement>,
    pub color: char,
    pub after_placement: u32,
}
impl Action {
    pub fn is_pass(&self) -> bool { self.placement.is_none() }
    pub fn vertex(&self, size: u8) -> String {
        self.placement.as_ref().map_or_else(|| "pass".into(), |p| p.vertex(size))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmissionOutcome { Pending, Confirmed, Failed, Cancelled }
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Submission {
    pub connection: u64,
    pub game: u64,
    pub revision: u64,
    pub search: u64,
    pub action: Action,
    pub body: String,
    pub prepared_nanos: u64,
    pub deadline_nanos: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubmissionFact {
    pub submission: Submission,
    pub outcome: SubmissionOutcome,
    pub written_nanos: u64,
    pub resolved_nanos: u64,
    pub reason: String,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub connection: u64,
    pub game: u64,
    pub revision: u64,
    pub rules: Option<Rules>,
    pub clock: Option<Clock>,
    pub ai: char,
    pub to_play: Option<char>,
    pub placements: Vec<Placement>,
    pub actions: Vec<Action>,
    pub initialized: bool,
    pub complete: bool,
    pub active: bool,
    pub status_key: String,
    pub clock_refreshed: bool,
    pub submit_deadline_nanos: u64,
    pub pending: Option<Submission>,
    pub automation_blocked: bool,
    pub turn_started_nanos: u64,
    pub fox_deadline_nanos: u64,
}
impl Snapshot {
    pub fn searchable(&self) -> bool {
        self.initialized && self.complete && self.active && !self.automation_blocked
    }
    pub fn own_turn(&self) -> bool { self.to_play == Some(self.ai) }
}

/// Sole writer of protocol facts. Callers supply monotonic time, including lifecycle transitions.
#[derive(Debug)]
pub struct GameState {
    connection: u64,
    game: u64,
    revision: u64,
    rules: Option<Rules>,
    awaiting_rules: bool,
    clock: Option<Clock>,
    ai: char,
    to_play: Option<char>,
    placements: Vec<Placement>,
    actions: Vec<Action>,
    status_seen: bool,
    complete: bool,
    active: bool,
    disconnected_game: bool,
    status_key: String,
    turn_started_nanos: u64,
    submit_deadline_nanos: u64,
    direct_max_time: f64,
    refresh_started_nanos: u64,
    refresh_requested: bool,
    clock_refreshed: bool,
    automation_blocked: bool,
    automation_stop_reason: String,
    pending: Option<Submission>,
    written: bool,
    submissions: Vec<SubmissionFact>,
    checkpoint: Option<Snapshot>,
    recovery_failure: String,
    recovery_contradiction: bool,
    remaining_main_nanos: u64,
}
impl Default for GameState {
    fn default() -> Self {
        Self {
            connection: 0, game: 0, revision: 0, rules: None, awaiting_rules: true,
            clock: None, ai: '\0', to_play: None, placements: Vec::new(), actions: Vec::new(),
            status_seen: false, complete: false, active: false, disconnected_game: false,
            status_key: "Fox.waiting".into(), turn_started_nanos: 0, submit_deadline_nanos: 0,
            direct_max_time: 1e20, refresh_started_nanos: 0, refresh_requested: false,
            clock_refreshed: false, automation_blocked: false, automation_stop_reason: String::new(),
            pending: None, written: false, submissions: Vec::new(), checkpoint: None,
            recovery_failure: "Fox.noCheckpoint".into(), recovery_contradiction: false,
            remaining_main_nanos: u64::MAX,
        }
    }
}

impl GameState {
    pub fn set_direct_max_time(&mut self, seconds: f64) -> Result<(), String> {
        require(seconds.is_finite() && seconds > 0.0)?;
        self.direct_max_time = seconds;
        Ok(())
    }
    fn effective_deadline(&self) -> u64 {
        if self.to_play != Some(self.ai) { return self.submit_deadline_nanos; }
        self.submit_deadline_nanos.min(self.turn_started_nanos.saturating_add((self.direct_max_time * SECOND as f64) as u64))
    }
    fn initialized(&self) -> bool {
        self.rules.is_some() && self.status_seen && self.clock.is_some() && self.to_play.is_some()
    }
    fn searchable(&self) -> bool {
        self.initialized() && self.complete && self.active && !self.automation_blocked
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            connection: self.connection, game: self.game, revision: self.revision,
            rules: self.rules.clone(), clock: self.clock.clone(), ai: self.ai, to_play: self.to_play,
            placements: self.placements.clone(), actions: self.actions.clone(), initialized: self.initialized(),
            complete: self.complete, active: self.active,
            status_key: if self.active && self.automation_blocked { &self.automation_stop_reason } else { &self.status_key }.clone(),
            clock_refreshed: self.clock_refreshed, submit_deadline_nanos: self.effective_deadline(),
            pending: self.pending.clone(), automation_blocked: self.automation_blocked,
            turn_started_nanos: self.turn_started_nanos, fox_deadline_nanos: self.submit_deadline_nanos,
        }
    }
    pub fn connected(&mut self, now: u64) {
        self.connection += 1;
        self.reset(now);
        self.status_key = "Fox.syncing".into();
    }
    pub fn disconnected(&mut self, now: u64) {
        let unresolved = self.submissions.iter().any(|f| f.submission.game == self.game
            && !matches!(f.outcome, SubmissionOutcome::Confirmed | SubmissionOutcome::Cancelled));
        if self.searchable() && !unresolved {
            self.checkpoint = Some(self.snapshot());
            self.recovery_failure = "Fox.noCheckpoint".into();
            self.recovery_contradiction = false;
        } else if unresolved {
            self.checkpoint = None;
            self.recovery_failure = "Fox.recoveryUnconfirmed".into();
        } else if self.automation_blocked {
            self.revoke_recovery(&self.snapshot().status_key);
        }
        self.stop_automation("Fox.disconnected", now);
        self.disconnected_game |= self.active;
        self.revision += 1;
        self.active = false;
        self.status_key = "Fox.disconnected".into();
    }
    fn reset(&mut self, now: u64) {
        self.stop_automation("Fox.stopped", now);
        self.automation_blocked = false;
        self.refresh_requested = false;
        self.clock_refreshed = false;
        self.revision += 1;
        self.rules = None;
        self.awaiting_rules = true;
        self.clock = None;
        self.remaining_main_nanos = u64::MAX;
        self.ai = '\0';
        self.to_play = None;
        self.placements.clear();
        self.actions.clear();
        self.status_seen = false;
        self.complete = false;
        self.active = false;
    }
    pub fn reject(&mut self, now: u64) {
        self.stop_automation("Fox.protocolError", now);
        self.revoke_recovery("Fox.protocolError");
        self.complete = false;
        self.revision += 1;
        self.status_key = "Fox.protocolError".into();
    }
    pub fn stop_automation(&mut self, reason: &str, now: u64) {
        self.automation_blocked = true;
        self.automation_stop_reason = reason.into();
        self.status_key = reason.into();
        self.clock_refreshed = false;
        self.resolve_submission(SubmissionOutcome::Failed, reason, now);
    }
    fn resolve_submission(&mut self, outcome: SubmissionOutcome, reason: &str, now: u64) {
        if self.pending.take().is_some() {
            let fact = self.submissions.last_mut().expect("pending has submission fact");
            fact.outcome = outcome;
            fact.resolved_nanos = now;
            fact.reason = reason.into();
        }
    }
    fn confirm(&mut self, action: &Action, now: u64) {
        if let Some(pending) = &self.pending {
            if self.written && pending.action == *action {
                self.resolve_submission(SubmissionOutcome::Confirmed, "", now);
            } else {
                self.stop_automation("Fox.confirmationFailed", now);
                self.gap();
            }
        }
    }
    pub fn submissions(&self) -> &[SubmissionFact] { &self.submissions }
    pub fn latest_submission(&self) -> Option<&SubmissionFact> { self.submissions.last() }
    pub fn begin_clock_refresh(&mut self, now: u64) -> bool {
        if !self.searchable() || self.to_play != Some(self.ai) || self.refresh_requested { return false; }
        self.refresh_requested = true;
        self.refresh_started_nanos = now;
        true
    }
    pub fn prepare_submission(&mut self, expected: &Snapshot, search: u64, vertex: &str, now: u64) -> Result<Option<Submission>, String> {
        if expected.connection != self.connection || expected.game != self.game || expected.revision != self.revision
            || !self.searchable() || self.to_play != Some(self.ai) || !self.clock_refreshed
            || self.pending.is_some() || now >= self.effective_deadline() { return Ok(None); }
        let size = self.rules.as_ref().expect("initialized rules").size;
        let placement = if vertex.eq_ignore_ascii_case("pass") { None } else {
            let bytes = vertex.as_bytes();
            require((2..=3).contains(&bytes.len()) && bytes[1..].iter().all(u8::is_ascii_digit))?;
            let x = COLUMNS.iter().position(|c| *c == bytes[0]).ok_or(INVALID)?;
            let row: u8 = vertex[1..].parse().map_err(|_| INVALID)?;
            require(x < size as usize && row >= 1 && row <= size)?;
            Some(Placement { number: self.placements.len() as u32 + 1, x: x as u8, y: size - row, color: self.ai })
        };
        let action = Action { after_placement: placement.as_ref().map_or(self.placements.len() as u32, |p| p.number), placement, color: self.ai };
        let body = action.placement.as_ref().map_or_else(|| format!("AFSKIP,{}", self.ai), |p| format!("AFPLAY,{},{number}^{x}^{y}^{color}", self.ai, number=p.number, x=p.x, y=p.y, color=p.color));
        let submission = Submission { connection: self.connection, game: self.game, revision: self.revision,
            search, action, body, prepared_nanos: now, deadline_nanos: self.effective_deadline() };
        self.pending = Some(submission.clone());
        self.written = false;
        self.submissions.push(SubmissionFact { submission: submission.clone(), outcome: SubmissionOutcome::Pending,
            written_nanos: 0, resolved_nanos: 0, reason: String::new() });
        self.status_key = "Fox.pending".into();
        Ok(Some(submission))
    }
    /// Exact value identity replaces Java reference identity without allowing a forged successor.
    pub fn claim_submission(&mut self, expected: &Submission, now: u64) -> bool {
        if self.pending.as_ref() != Some(expected) || self.written || self.automation_blocked || !self.active || !self.complete
            || expected.connection != self.connection || expected.game != self.game || expected.revision != self.revision
            || self.to_play != Some(self.ai) || now >= expected.deadline_nanos.min(self.effective_deadline()) { return false; }
        self.written = true;
        self.submissions.last_mut().expect("pending fact").written_nanos = now;
        true
    }
    pub fn cancel_unwritten_for_pda(&mut self, now: u64) -> bool {
        if self.pending.is_none() { return true; }
        if self.written { return false; }
        self.resolve_submission(SubmissionOutcome::Cancelled, "Fox.pdaSuperseded", now);
        true
    }
    pub fn has_checkpoint(&self) -> bool { self.checkpoint.is_some() }
    pub fn revoke_recovery(&mut self, reason: &str) {
        self.checkpoint = None;
        self.recovery_contradiction = true;
        self.recovery_failure = reason.into();
    }
    pub fn recovery_status(&self) -> String {
        if !self.initialized() || !self.active { return "Fox.syncing".into(); }
        let Some(old) = &self.checkpoint else { return self.recovery_failure.clone(); };
        if self.recovery_contradiction || self.rules != old.rules || self.ai != old.ai { return "Fox.recoveryMismatch".into(); }
        if self.placements == old.placements {
            return if self.to_play == old.to_play { "Fox.restoring" } else { "Fox.recoveryMismatch" }.into();
        }
        let n = old.placements.len();
        if self.placements.len() != n + 1 || self.placements[..n] != old.placements
            || old.actions.last().is_none_or(|a| a.color != self.ai) || old.to_play == Some(self.ai)
            || self.placements[n].color != opposite(self.ai) || self.to_play != Some(self.ai) {
            return "Fox.recoveryMismatch".into();
        }
        "Fox.restoring".into()
    }
    pub fn recover_history(&mut self, now: u64) -> bool {
        if self.automation_blocked || self.recovery_status() != "Fox.restoring" { return false; }
        let old = self.checkpoint.as_ref().expect("restoring checkpoint");
        let extended = self.placements.len() > old.placements.len();
        if !extended && self.to_play == Some(self.ai) {
            self.turn_started_nanos = old.turn_started_nanos;
            self.submit_deadline_nanos = self.submit_deadline_nanos.min(old.fox_deadline_nanos);
        }
        if self.clock_nanos(self.clock.as_ref().expect("initialized clock")) <= SECOND
            || (self.to_play == Some(self.ai) && now >= self.effective_deadline()) {
            self.revoke_recovery("Fox.clockRisk");
            return false;
        }
        self.actions = self.checkpoint.take().expect("checkpoint").actions;
        if extended {
            let p = self.placements.last().expect("extended placement").clone();
            self.actions.push(Action { color: p.color, after_placement: p.number, placement: Some(p) });
        }
        self.complete = true;
        self.disconnected_game = false;
        self.revision += 1;
        self.turn_status();
        true
    }
    fn clock_nanos(&self, clock: &Clock) -> u64 {
        let seconds = if clock.main_seconds > 0 { clock.main_seconds } else if clock.periods > 0 { clock.byo_seconds } else { 0 };
        let nanos = u64::from(seconds) * SECOND;
        if clock.main_seconds > 0 { nanos.min(self.remaining_main_nanos) } else { nanos }
    }
    fn deadline(anchor: u64, duration: u64) -> u64 {
        // Unsigned monotonic zero cannot represent Java's negative expired deadline.
        anchor.saturating_add(duration).saturating_sub(SECOND)
    }
    fn end_turn(&mut self, now: u64) {
        if self.to_play == Some(self.ai) && self.clock.as_ref().is_some_and(|c| c.main_seconds > 0) {
            self.remaining_main_nanos = self.remaining_main_nanos.min(self.submit_deadline_nanos.saturating_add(SECOND).saturating_sub(now));
        }
    }
    fn start_turn(&mut self, now: u64) {
        self.turn_started_nanos = now;
        self.refresh_requested = false;
        self.clock_refreshed = false;
        self.submit_deadline_nanos = self.clock.as_ref().map_or(now, |c| Self::deadline(now, self.clock_nanos(c)));
    }
    fn turn_status(&mut self) {
        self.status_key = if self.pending.is_some() { "Fox.pending" } else if self.to_play == Some(self.ai) { "Fox.ourTurn" } else { "Fox.opponentTurn" }.into();
    }
    fn gap(&mut self) {
        self.complete = false;
        if self.checkpoint.is_some() { self.recovery_contradiction = true; }
        self.revision += 1;
        self.status_key = "Fox.incomplete".into();
    }
    fn terminal(&mut self, now: u64) {
        self.stop_automation("Fox.terminal", now);
        self.revision += 1;
        self.active = false;
        self.disconnected_game = false;
        self.awaiting_rules = true;
        self.revoke_recovery("Fox.noCheckpoint");
        self.status_key = "Fox.terminal".into();
    }

    /// Parses and validates the entire payload before applying any state transition.
    /// Malformed input returns an error unchanged; the owner calls `reject` to fail closed.
    pub fn receive(&mut self, body: &str, now: u64) -> Result<(), String> {
        let f: Vec<&str> = body.split(',').collect();
        match f[0] {
            "FARULE" => {
                require(f.len() >= 6)?;
                let size = integer(f[1], 2, 25)? as u8;
                let main_seconds = integer(f[2], 0, i32::MAX as i64)? as u32;
                let byo_seconds = integer(f[3], 0, i32::MAX as i64)? as u32;
                let periods = integer(f[4], 0, i32::MAX as i64)? as u32;
                let komi = integer(f[5], -100000, 100000)? as f64 / 100.0;
                let mut setup = Vec::with_capacity(f.len() - 6);
                let mut occupied = [false; 625];
                for text in &f[6..] {
                    let p = placement(text, size, true)?.expect("setup not sentinel");
                    let index = p.y as usize * size as usize + p.x as usize;
                    require(p.color == 'B' && !occupied[index])?;
                    occupied[index] = true;
                    setup.push(p);
                }
                let incoming = Rules { size, main_seconds, byo_seconds, periods, komi, setup };
                if !self.awaiting_rules {
                    if self.rules.as_ref() != Some(&incoming) {
                        self.stop_automation("Fox.protocolError", now);
                        self.revoke_recovery("Fox.protocolError");
                        self.revision += 1;
                    }
                    return Ok(());
                }
                self.reset(now);
                self.game += 1;
                self.rules = Some(incoming);
                self.awaiting_rules = false;
                self.status_key = "Fox.syncing".into();
            }
            "FASTATUS" => {
                if f.len() == 2 && f[1] == "0" { self.terminal(now); return Ok(()); }
                require(self.rules.is_some() && f.len() >= 3 && f[1] == "1")?;
                let ai = color(f[2])?;
                require(self.ai == '\0' || self.ai == ai)?;
                let size = self.rules.as_ref().expect("rules").size;
                let mut incoming = Vec::with_capacity(f.len() - 3);
                for text in &f[3..] {
                    let Some(p) = placement(text, size, false)? else { require(f.len() == 4)?; break; };
                    require(p.number as usize == incoming.len() + 1)?;
                    incoming.push(p);
                }
                if !self.status_seen { self.complete = incoming.is_empty() && !self.disconnected_game; }
                if self.status_seen && incoming != self.placements {
                    let confirmed = self.pending.as_ref().filter(|p| self.written && !p.action.is_pass()
                        && incoming.len() == self.placements.len() + 1
                        && incoming[..self.placements.len()] == self.placements
                        && incoming.last() == p.action.placement.as_ref()).map(|p| p.action.clone());
                    if let Some(action) = confirmed {
                        self.actions.push(action.clone());
                        self.confirm(&action, now);
                        self.end_turn(now);
                        self.to_play = Some(opposite(self.ai));
                        self.start_turn(now);
                    } else { self.gap(); }
                }
                if incoming != self.placements { self.revision += 1; }
                self.placements = incoming;
                self.ai = ai;
                self.status_seen = true;
                self.active = true;
                if !self.complete { self.status_key = "Fox.incomplete".into(); }
                else if self.to_play.is_some() && !self.automation_blocked { self.turn_status(); }
            }
            "FAMOVE" | "FASKIP" => {
                let pass = f[0] == "FASKIP";
                require(self.rules.is_some() && self.status_seen && self.active && f.len() == if pass { 3 } else { 4 })?;
                require(f[1] == "0" || f[1] == "1")?;
                require(color(f[2])? == self.ai)?;
                let next = if f[1] == "1" { self.ai } else { opposite(self.ai) };
                if pass {
                    if let Some(to_play) = self.to_play {
                        let action = Action { placement: None, color: opposite(next), after_placement: self.placements.len() as u32 };
                        if to_play == next && self.actions.last() == Some(&action) { return Ok(()); }
                        require(to_play != next)?;
                        self.actions.push(action.clone());
                        self.confirm(&action, now);
                        self.revision += 1;
                        if self.checkpoint.is_some() { self.recovery_contradiction = true; }
                    } else if self.complete { self.gap(); }
                } else {
                    let p = placement(f[3], self.rules.as_ref().expect("rules").size, false)?;
                    match p {
                        None => {
                            require(self.placements.is_empty())?;
                            if self.to_play.is_some_and(|c| c != next) { self.gap(); }
                            let opening = if self.rules.as_ref().expect("rules").setup.is_empty() { 'B' } else { 'W' };
                            if self.complete && self.actions.is_empty() && next != opening { self.gap(); }
                        }
                        Some(p) if self.placements.last() == Some(&p) => {
                            if self.to_play.is_some_and(|c| c != next) { self.gap(); }
                        }
                        Some(p) => {
                            require(p.number as usize == self.placements.len() + 1)?;
                            if self.to_play != Some(p.color) || next == p.color { self.gap(); }
                            let action = Action { color: p.color, after_placement: p.number, placement: Some(p.clone()) };
                            self.placements.push(p);
                            self.actions.push(action.clone());
                            self.confirm(&action, now);
                            self.revision += 1;
                            if self.checkpoint.is_some() { self.recovery_contradiction = true; }
                        }
                    }
                }
                let changed = self.to_play != Some(next);
                if changed { self.revision += 1; self.end_turn(now); }
                self.to_play = Some(next);
                if changed { self.start_turn(now); }
                if self.complete && !self.automation_blocked { self.turn_status(); }
            }
            "FATIMELEFT" => {
                require(f.len() == 4 && self.rules.is_some())?;
                let clock = Clock { main_seconds: integer(f[1], 0, i32::MAX as i64)? as u32,
                    byo_seconds: integer(f[2], 0, i32::MAX as i64)? as u32,
                    periods: integer(f[3], 0, i32::MAX as i64)? as u32, received_nanos: now };
                let rules = self.rules.as_ref().expect("rules");
                require(clock.byo_seconds <= rules.byo_seconds && clock.periods <= rules.periods && clock.main_seconds <= rules.main_seconds)?;
                require(self.clock.as_ref().is_none_or(|old| clock.periods <= old.periods && clock.main_seconds <= old.main_seconds))?;
                self.remaining_main_nanos = self.remaining_main_nanos.min(u64::from(clock.main_seconds) * SECOND);
                if self.to_play == Some(self.ai) {
                    let anchor = if self.refresh_requested && !self.clock_refreshed { self.refresh_started_nanos } else { now };
                    let deadline = Self::deadline(anchor, self.clock_nanos(&clock));
                    self.submit_deadline_nanos = if self.clock.is_none() { Self::deadline(self.turn_started_nanos, self.clock_nanos(&clock)) }
                        else { self.submit_deadline_nanos.min(deadline) };
                    if self.refresh_requested { self.clock_refreshed = true; }
                }
                self.clock = Some(clock);
            }
            "FARESULT" => { require(f.len() == 2 && !f[1].is_empty())?; self.terminal(now); }
            "FASCORE" => {
                require(f.len() == 1)?;
                self.stop_automation("Fox.officialAction", now);
                self.revision += 1;
                self.status_key = "Fox.officialAction".into();
            }
            _ => return Err("Unsupported Fox message".into()),
        }
        Ok(())
    }
}

fn require(condition: bool) -> Result<(), String> {
    if condition { Ok(()) } else { Err(INVALID.into()) }
}
fn opposite(color: char) -> char { if color == 'B' { 'W' } else { 'B' } }
fn color(value: &str) -> Result<char, String> {
    match value { "B" => Ok('B'), "W" => Ok('W'), _ => Err(INVALID.into()) }
}
fn integer(value: &str, min: i64, max: i64) -> Result<i64, String> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    require(!digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()))?;
    let number: i64 = value.parse().map_err(|_| INVALID)?;
    require(number >= min && number <= max)?;
    Ok(number)
}
fn placement(value: &str, size: u8, setup: bool) -> Result<Option<Placement>, String> {
    if !setup && value == "0^0^0^N" { return Ok(None); }
    let mut parts = value.split('^');
    let number = integer(parts.next().ok_or(INVALID)?, if setup { 0 } else { 1 }, i32::MAX as i64)? as u32;
    let x = integer(parts.next().ok_or(INVALID)?, 0, i64::from(size - 1))? as u8;
    let y = integer(parts.next().ok_or(INVALID)?, 0, i64::from(size - 1))? as u8;
    let color = color(parts.next().ok_or(INVALID)?)?;
    require(parts.next().is_none())?;
    Ok(Some(Placement { number, x, y, color }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receive(state: &mut GameState, messages: &[&str], now: u64) {
        for message in messages { state.receive(message, now).unwrap(); }
    }
    fn initialized() -> GameState {
        let mut state = GameState::default();
        state.connected(0);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,60,30,3", "FAMOVE,1,B,0^0^0^N"], 0);
        assert!(state.snapshot().searchable());
        state
    }
    fn refresh(state: &mut GameState, now: u64, clock: &str) {
        assert!(state.begin_clock_refresh(now));
        state.receive(&format!("FATIMELEFT,{clock}"), now).unwrap();
    }
    fn retained_pass() -> GameState {
        let mut state = initialized();
        state.receive("FAMOVE,0,B,1^16^3^B", 1).unwrap();
        state.receive("FAMOVE,1,B,2^3^15^W", 2).unwrap();
        state.receive("FASKIP,0,B", 3).unwrap();
        state.disconnected(4);
        assert!(state.has_checkpoint());
        state
    }
    fn reconnect(state: &mut GameState, rules: &str, status: &str, turn: &str) {
        state.connected(100 * SECOND);
        receive(state, &[rules, status, turn], 100 * SECOND);
        assert!(!state.recover_history(100 * SECOND));
        state.receive("FATIMELEFT,40,30,3", 101 * SECOND).unwrap();
    }

    #[test]
    fn initialization_setup_and_unnumbered_pass_have_distinct_identity() {
        let mut state = GameState::default();
        state.connected(0);
        receive(&mut state, &["FARULE,9,60,10,2,50,0^2^2^B,0^6^6^B", "FASTATUS,1,W", "FAMOVE,1,W,0^0^0^N"], 0);
        assert!(!state.snapshot().searchable());
        state.receive("FATIMELEFT,60,10,2", 0).unwrap();
        let snapshot = state.snapshot();
        assert!(snapshot.searchable() && snapshot.own_turn());
        assert!(snapshot.actions.is_empty() && snapshot.placements.is_empty());
        let rules = snapshot.rules.unwrap();
        assert_eq!(rules.komi, 0.5);
        assert_eq!(rules.setup.len(), 2);
        refresh(&mut state, 0, "60,10,2");
        let pending = state.prepare_submission(&state.snapshot(), 7, "J9", 1).unwrap().unwrap();
        assert_eq!(pending.body, "AFPLAY,W,1^8^0^W");
        assert_eq!(pending.action.vertex(9), "J9");
    }

    #[test]
    fn observed_pass_and_duplicate_events_do_not_consume_placement_numbers() {
        let mut state = initialized();
        receive(&mut state, &["FAMOVE,0,B,1^16^3^B", "FAMOVE,1,B,2^3^15^W", "FASKIP,0,B"], 1);
        let before = state.snapshot();
        receive(&mut state, &["FASKIP,0,B", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,0,B,2^3^15^W"], 2);
        assert_eq!(state.snapshot().revision, before.revision);
        assert_eq!(state.snapshot().actions, before.actions);
        assert_eq!(before.placements.len(), 2);
        assert_eq!(before.actions.len(), 3);
        assert!(before.actions[2].is_pass());
        assert_eq!(before.to_play, Some('W'));
        state.receive("FAMOVE,1,B,3^15^15^W", 3).unwrap();
        assert_eq!(state.snapshot().actions.len(), 4);
        assert!(state.snapshot().searchable());
    }

    #[test]
    fn prepare_write_and_official_confirmation_are_three_distinct_facts() {
        let mut state = initialized();
        refresh(&mut state, 2 * SECOND, "58,30,3");
        let expected = state.snapshot();
        let pending = state.prepare_submission(&expected, 7, "R16", 9 * SECOND).unwrap().unwrap();
        assert_eq!(pending.body, "AFPLAY,B,1^16^3^B");
        let mut forged = pending.clone();
        forged.search += 1;
        assert!(!state.claim_submission(&forged, 10 * SECOND));
        assert!(state.claim_submission(&pending.clone(), 10 * SECOND));
        assert!(!state.claim_submission(&pending, 11 * SECOND));
        assert!(!state.cancel_unwritten_for_pda(11 * SECOND));
        assert!(state.snapshot().actions.is_empty());
        state.receive("FASTATUS,1,B", 12 * SECOND).unwrap();
        assert!(state.snapshot().pending.is_some());
        state.receive("FAMOVE,0,B,1^16^3^B", 13 * SECOND).unwrap();
        assert_eq!(state.latest_submission().unwrap().outcome, SubmissionOutcome::Confirmed);
        assert_eq!(state.latest_submission().unwrap().written_nanos, 10 * SECOND);
        assert_eq!(state.latest_submission().unwrap().resolved_nanos, 13 * SECOND);
        assert_eq!(state.snapshot().actions.len(), 1);
        assert!(state.prepare_submission(&expected, 8, "D4", 14 * SECOND).unwrap().is_none());
    }

    #[test]
    fn pass_requires_skip_confirmation_and_full_status_can_only_confirm_placement() {
        let mut state = initialized();
        refresh(&mut state, 0, "60,30,3");
        let pass = state.prepare_submission(&state.snapshot(), 1, "PASS", 1).unwrap().unwrap();
        assert_eq!(pass.body, "AFSKIP,B");
        assert!(state.claim_submission(&pass, 2));
        state.receive("FASTATUS,1,B", 3).unwrap();
        assert!(state.snapshot().pending.is_some());
        receive(&mut state, &["FASKIP,0,B", "FASKIP,0,B", "FAMOVE,1,B,1^3^3^W"], 4);
        assert_eq!(state.snapshot().actions.len(), 2);
        assert_eq!(state.snapshot().placements.len(), 1);
        refresh(&mut state, 7, "59,30,3");
        let p = state.prepare_submission(&state.snapshot(), 2, "R16", 8).unwrap().unwrap();
        assert_eq!(p.body, "AFPLAY,B,2^16^3^B");
        assert!(state.claim_submission(&p, 9));
        receive(&mut state, &["FASTATUS,1,B,1^3^3^W,2^16^3^B", "FAMOVE,0,B,2^16^3^B"], 10);
        assert_eq!(state.latest_submission().unwrap().outcome, SubmissionOutcome::Confirmed);
        assert_eq!(state.snapshot().actions.len(), 3);
        assert!(state.snapshot().complete);
    }

    #[test]
    fn refresh_query_queue_and_repeated_initialization_never_extend_turn() {
        let mut state = initialized();
        state.set_direct_max_time(10.0).unwrap();
        refresh(&mut state, 3 * SECOND, "60,30,3");
        let before = state.snapshot();
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,60,30,3", "FAMOVE,1,B,0^0^0^N"], 8 * SECOND);
        let after = state.snapshot();
        assert_eq!(after.game, before.game);
        assert_eq!(after.revision, before.revision);
        assert_eq!(after.turn_started_nanos, 0);
        assert_eq!(after.submit_deadline_nanos, 10 * SECOND);
        assert_eq!(after.fox_deadline_nanos, 59 * SECOND);
        assert!(!state.begin_clock_refresh(8 * SECOND));
        let pending = state.prepare_submission(&after, 1, "D4", 9 * SECOND).unwrap().unwrap();
        assert!(!state.claim_submission(&pending, 10 * SECOND));
        state.stop_automation("Fox.clockRisk", 10 * SECOND);
        assert!(state.prepare_submission(&state.snapshot(), 2, "pass", 11 * SECOND).unwrap().is_none());
    }

    #[test]
    fn own_main_time_is_consumed_but_opponent_time_and_byo_periods_are_not_reset() {
        let mut state = initialized();
        refresh(&mut state, 0, "60,30,3");
        state.receive("FASKIP,0,B", 10 * SECOND).unwrap();
        state.receive("FAMOVE,1,B,1^3^3^W", 20 * SECOND).unwrap();
        refresh(&mut state, 21 * SECOND, "60,30,3");
        assert_eq!(state.snapshot().submit_deadline_nanos, 69 * SECOND);
        state.receive("FATIMELEFT,0,30,2", 30 * SECOND).unwrap();
        state.receive("FATIMELEFT,0,30,2", 40 * SECOND).unwrap();
        assert_eq!(state.snapshot().submit_deadline_nanos, 59 * SECOND);
        assert!(state.receive("FATIMELEFT,0,30,3", 41 * SECOND).is_err());
        state.receive("FASKIP,0,B", 42 * SECOND).unwrap();
        state.receive("FAMOVE,1,B,2^4^4^W", 50 * SECOND).unwrap();
        refresh(&mut state, 51 * SECOND, "0,30,2");
        assert_eq!(state.snapshot().submit_deadline_nanos, 79 * SECOND);
    }

    #[test]
    fn malformed_payloads_leave_every_public_fact_unchanged() {
        let mut state = initialized();
        for message in ["FARULE,19,60,30,3,650,0^3^3^B,0^3^3^B", "FASTATUS,1,B,1^3^3^B,3^4^4^W", "FASTATUS,1,B,0^0^0^N,1^3^3^B", "FAMOVE,0,B,2^3^15^B", "FASKIP,1,B", "FATIMELEFT,59,31,3", "FATIMELEFT,59,30,4", "FATIMELEFT,+1,30,3", "FARESULT,", "FASCORE,extra", "UNKNOWN"] {
            let before = state.snapshot();
            assert!(state.receive(message, 123).is_err(), "{message}");
            assert_eq!(state.snapshot(), before, "{message}");
        }
        state.reject(124);
        assert!(!state.snapshot().searchable());
        assert_eq!(state.snapshot().status_key, "Fox.protocolError");
    }

    #[test]
    fn cold_or_missing_history_is_observation_and_conflicting_rules_fail_closed() {
        let mut cold = GameState::default();
        cold.connected(0);
        receive(&mut cold, &["FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B", "FATIMELEFT,60,30,3", "FAMOVE,0,B,1^16^3^B"], 0);
        assert!(!cold.snapshot().searchable());
        assert!(cold.snapshot().actions.is_empty());
        let mut state = initialized();
        state.receive("FAMOVE,1,B,1^3^3^W", 1).unwrap();
        assert!(!state.snapshot().complete);
        let before = state.snapshot();
        state.receive("FARULE,19,60,30,3,750", 2).unwrap();
        assert_eq!(state.snapshot().rules, before.rules);
        assert_eq!(state.snapshot().actions, before.actions);
        assert_eq!(state.snapshot().game, before.game);
        assert_eq!(state.snapshot().status_key, "Fox.protocolError");
    }

    #[test]
    fn cancellation_supersedes_only_unwritten_permission_without_poisoning_checkpoint() {
        let mut state = initialized();
        refresh(&mut state, 0, "60,30,3");
        let first = state.prepare_submission(&state.snapshot(), 1, "D4", 1).unwrap().unwrap();
        assert!(state.cancel_unwritten_for_pda(2));
        let next = state.prepare_submission(&state.snapshot(), 2, "Q16", 3).unwrap().unwrap();
        assert_eq!(first.deadline_nanos, next.deadline_nanos);
        assert!(!state.claim_submission(&first, 4));
        assert!(state.claim_submission(&next, 4));
        assert!(!state.cancel_unwritten_for_pda(5));
        state.receive("FAMOVE,0,B,1^15^3^B", 6).unwrap();
        state.disconnected(7);
        assert!(state.has_checkpoint());
        assert_eq!(state.submissions()[0].outcome, SubmissionOutcome::Cancelled);
        assert_eq!(state.submissions()[1].outcome, SubmissionOutcome::Confirmed);
    }

    #[test]
    fn terminal_stop_disconnect_or_mismatched_echo_invalidate_old_engine_result() {
        for event in ["stop", "disconnect", "FASCORE", "FARESULT,W+R", "FAMOVE,0,B,1^3^15^B"] {
            let mut state = initialized();
            refresh(&mut state, 0, "60,30,3");
            let before = state.snapshot();
            let pending = state.prepare_submission(&before, 1, "D4", 1).unwrap().unwrap();
            match event {
                "stop" => state.stop_automation("Fox.stopped", 2),
                "disconnect" => state.disconnected(2),
                _ => state.receive(event, 2).unwrap(),
            }
            assert!(!state.claim_submission(&pending, 3), "{event}");
            assert!(state.prepare_submission(&before, 2, "Q16", 3).unwrap().is_none());
            assert_eq!(state.submissions()[0].outcome, SubmissionOutcome::Failed);
        }
    }

    #[test]
    fn confirmed_pass_recovers_only_after_complete_fresh_initialization() {
        for turn in ["FAMOVE,0,B,2^3^15^W", "FASKIP,0,B"] {
            let mut state = retained_pass();
            reconnect(&mut state, "FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W", turn);
            assert!(!state.snapshot().searchable());
            assert!(state.recover_history(101 * SECOND));
            let snapshot = state.snapshot();
            assert!(snapshot.searchable());
            assert_eq!(snapshot.actions.len(), 3);
            assert_eq!(snapshot.placements.len(), 2);
            assert!(snapshot.actions[2].is_pass());
            assert_eq!(snapshot.to_play, Some('W'));
            assert!(!state.recover_history(101 * SECOND + 1));
        }
    }

    #[test]
    fn recovery_accepts_exactly_one_proven_offline_opponent_placement() {
        let mut state = retained_pass();
        reconnect(&mut state, "FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W,3^15^15^W", "FAMOVE,1,B,3^15^15^W");
        assert!(state.recover_history(101 * SECOND));
        let snapshot = state.snapshot();
        assert_eq!(snapshot.actions.iter().map(|a| a.vertex(19)).collect::<Vec<_>>(), ["R16", "D4", "pass", "Q4"]);
        assert!(snapshot.own_turn());
        assert!(state.prepare_submission(&snapshot, 1, "Q16", 101 * SECOND).unwrap().is_none());
        refresh(&mut state, 102 * SECOND, "38,30,3");
        assert!(state.prepare_submission(&state.snapshot(), 2, "Q16", 103 * SECOND).unwrap().is_some());
    }

    #[test]
    fn recovery_rejects_rules_setup_color_placement_turn_and_unproven_extensions() {
        for (rules, status, turn) in [
            ("FARULE,19,60,30,3,750", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,0,B,2^3^15^W"),
            ("FARULE,19,60,30,3,650,0^3^3^B", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,0,B,2^3^15^W"),
            ("FARULE,19,60,30,3,650", "FASTATUS,1,W,1^16^3^B,2^3^15^W", "FAMOVE,1,W,2^3^15^W"),
            ("FARULE,19,60,30,3,650", "FASTATUS,1,B,1^15^3^B,2^3^15^W", "FAMOVE,0,B,2^3^15^W"),
            ("FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,1,B,2^3^15^W"),
            ("FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W,3^15^15^B", "FAMOVE,1,B,3^15^15^B"),
            ("FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W,3^15^15^W,4^3^3^B", "FAMOVE,1,B,4^3^3^B"),
        ] {
            let mut state = retained_pass();
            reconnect(&mut state, rules, status, turn);
            assert_eq!(state.recovery_status(), "Fox.recoveryMismatch");
            assert!(!state.recover_history(101 * SECOND));
            assert!(!state.snapshot().searchable());
        }
    }

    #[test]
    fn unchanged_own_turn_recovery_keeps_original_direct_budget_and_refresh_requirement() {
        let mut state = initialized();
        state.set_direct_max_time(10.0).unwrap();
        refresh(&mut state, 0, "60,30,3");
        let original = state.snapshot();
        state.disconnected(SECOND);
        state.connected(3 * SECOND);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FAMOVE,1,B,0^0^0^N"], 4 * SECOND);
        assert!(!state.recover_history(4 * SECOND));
        state.receive("FATIMELEFT,55,30,3", 5 * SECOND).unwrap();
        assert!(state.recover_history(6 * SECOND));
        assert_eq!(state.snapshot().turn_started_nanos, original.turn_started_nanos);
        assert_eq!(state.snapshot().submit_deadline_nanos, original.submit_deadline_nanos);
        refresh(&mut state, 7 * SECOND, "54,30,3");
        assert!(state.prepare_submission(&state.snapshot(), 2, "D4", 10 * SECOND).unwrap().is_none());
    }

    #[test]
    fn failed_submission_survives_reconnect_and_only_genuine_new_game_reopens_automation() {
        let mut state = initialized();
        refresh(&mut state, 0, "60,30,3");
        let pending = state.prepare_submission(&state.snapshot(), 1, "D4", 1).unwrap().unwrap();
        assert!(state.claim_submission(&pending, 2));
        state.stop_automation("Fox.confirmationFailed", 3);
        state.disconnected(4);
        state.connected(5);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,60,30,3", "FAMOVE,1,B,0^0^0^N"], 5);
        assert_eq!(state.recovery_status(), "Fox.recoveryUnconfirmed");
        assert!(!state.recover_history(6));
        assert!(!state.snapshot().searchable());
        receive(&mut state, &["FASTATUS,0", "FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,60,30,3", "FAMOVE,1,B,0^0^0^N"], 7);
        assert!(state.snapshot().searchable());
        assert_eq!(state.submissions()[0].outcome, SubmissionOutcome::Failed);
        assert_eq!(state.submissions()[0].written_nanos, 2);
    }

    #[test]
    fn stopped_incomplete_or_expired_recovery_cannot_acquire_a_checkpoint() {
        for event in ["FASCORE", "FARESULT,B+R", "FAMOVE,1,B,3^15^15^W", "engine", "connection", "expired"] {
            let mut state = retained_pass();
            reconnect(&mut state, "FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,0,B,2^3^15^W");
            match event {
                "engine" => state.revoke_recovery("Fox.recoveryEngineChanged"),
                "connection" => state.connected(102 * SECOND),
                "expired" => state.receive("FATIMELEFT,0,0,0", 102 * SECOND).unwrap(),
                _ => state.receive(event, 102 * SECOND).unwrap(),
            }
            assert!(!state.recover_history(102 * SECOND + 1), "{event}");
            assert!(!state.snapshot().searchable());
            if event == "expired" { assert_eq!(state.recovery_status(), "Fox.clockRisk"); }
        }
    }

    #[test]
    fn empty_placement_reconnect_restores_observed_pass_not_an_assumed_opening() {
        let mut state = initialized();
        state.receive("FASKIP,0,B", 1).unwrap();
        state.disconnected(2);
        state.connected(100 * SECOND);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B", "FATIMELEFT,40,30,3", "FAMOVE,0,B,0^0^0^N"], 100 * SECOND);
        assert!(state.recover_history(100 * SECOND));
        assert!(state.snapshot().placements.is_empty());
        assert_eq!(state.snapshot().actions.len(), 1);
        assert!(state.snapshot().actions[0].is_pass());
        assert_eq!(state.snapshot().to_play, Some('W'));
    }

    #[test]
    fn incomplete_connection_cannot_supply_clock_to_next_connection() {
        let mut state = initialized();
        state.receive("FAMOVE,0,B,1^16^3^B", 1).unwrap();
        state.disconnected(2);
        state.connected(3);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W"], 3);
        assert!(!state.recover_history(3));
        state.disconnected(4);
        state.connected(5);
        assert!(!state.snapshot().initialized);
        receive(&mut state, &["FARULE,19,60,30,3,650", "FASTATUS,1,B,1^16^3^B,2^3^15^W", "FAMOVE,1,B,2^3^15^W"], 5);
        assert!(!state.recover_history(5));
        state.receive("FATIMELEFT,50,30,3", 6).unwrap();
        assert!(state.recover_history(7));
        assert_eq!(state.snapshot().actions.iter().map(|a| a.vertex(19)).collect::<Vec<_>>(), ["R16", "D4"]);
        assert!(state.snapshot().own_turn());
    }

    #[test]
    fn official_action_retains_confirmed_history_but_cannot_become_recovery_permission() {
        let mut state = initialized();
        state.receive("FAMOVE,0,B,1^16^3^B", 1).unwrap();
        let confirmed = state.snapshot().actions;
        receive(&mut state, &["FASCORE", "FASTATUS,1,B,1^16^3^B", "FAMOVE,0,B,1^16^3^B", "FATIMELEFT,59,30,3"], 2);
        assert_eq!(state.snapshot().actions, confirmed);
        assert!(state.snapshot().complete);
        assert_eq!(state.snapshot().status_key, "Fox.officialAction");
        assert!(!state.snapshot().searchable());
        assert!(!state.begin_clock_refresh(3));
        state.disconnected(4);
        assert!(!state.has_checkpoint());
    }
}
