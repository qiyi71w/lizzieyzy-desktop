//! Engines that end unexpectedly start again (自动检测引擎是否存活, Java
//! `EngineManager.checkEngineAlive` / `Leelaz.AutomaticRestartAttempt`, and the engine-game
//! participant recovery).
//!
//! A foreground engine whose process or remote command ended after it was ready is started again
//! with the same profile, as the foreground engine. Nothing of the open game is captured for it:
//! the restarted engine follows the seat's wish like any newly seated one, so it is brought to
//! the position, komi, session rules and board size current when it is synced (a position the
//! user moved to meanwhile included), and it analyses only while the user has not paused. Komi
//! and board size stay those of the game; the profile's apply only to a new engine choice.
//!
//! Java only restarts an engine it may check (`canCheckAlive`): a restart clears that until the
//! new process produced an accepted analysis line or a legal game move, so an engine that keeps
//! crashing is restarted once, not in a loop. The death is detected from the process / connection
//! end the runtime reports rather than by polling.
//!
//! A participant of an engine match that ends retires the match (unchanged); once the match is
//! retired the participant starts again. Its failures to start are logged without the ordinary
//! diagnostic, and it is not tried again.

use std::collections::HashSet;

use super::Workbench;
use crate::engine::runtime::{EngineFailure, InstanceId};
use crate::sanitize::sanitize;

#[derive(Default)]
pub(super) struct RestartFlow {
    /// Instances an automatic restart started that have produced neither an accepted analysis
    /// line nor a legal game move yet (Java `canCheckAlive == false`): ending again before that
    /// does not restart them.
    unproven: HashSet<InstanceId>,
    /// The foreground engine (profile id) admitted for a restart once the events of its end are
    /// handled (a game retired, a lease ended).
    due: Option<String>,
    /// Replacements of match participants until they are ready: a failure to start is logged, not
    /// shown (Java's deferred participant recovery marks the engine not loaded, silently).
    quiet: HashSet<InstanceId>,
}

impl Workbench {
    /// An engine ended unexpectedly. Decides whether it starts again and returns whether the
    /// ordinary failure diagnostic is shown for it.
    pub(super) fn restart_on_failure(&mut self, f: &EngineFailure) -> bool {
        let proven = !self.restart.unproven.remove(&f.instance);
        if self.restart.quiet.remove(&f.instance) && f.during_startup {
            self.log(true, format!("engine restart [{}] failed: {}", f.engine_name, sanitize(&f.message)));
            return false;
        }
        // `checkEngineAlive`: the foreground engine only, once it was ready, not while a holder
        // owned its state, while 起始局面设置模式 is on, or during an engine match (whose
        // participants are restarted after it).
        let admitted = self.config.engine.analysis.auto_check_engine_alive
            && f.foreground
            && !f.during_startup
            && !f.held
            && proven
            && !self.setup_mode()
            && !self.match_active();
        if admitted {
            self.restart.due = Some(f.profile_id.clone());
        }
        true
    }

    /// Carries out a restart admitted while handling the events of the engine's end.
    pub(super) fn restart_due(&mut self) {
        if let Some(profile) = self.restart.due.take() {
            self.restart_failed_foreground(&profile);
        }
    }

    /// 重启已失败的前台引擎: starts the engine of `profile_id` again as the foreground engine, synced
    /// to the current position, komi, session rules and board size, analysing unless paused. The
    /// seat must be empty; a launch failure is reported like any engine start. Returns whether an
    /// engine took the seat.
    pub(super) fn restart_failed_foreground(&mut self, profile_id: &str) -> bool {
        if self.engines.foreground().is_some() || self.setup_mode() {
            return false;
        }
        let Some(profile) = self.config.engine.engines.iter().find(|p| p.id == profile_id).cloned() else {
            return false;
        };
        let instance = match self.engines.start(&profile, self.bootstrap(&profile)) {
            Ok(i) => i,
            Err(f) => {
                self.engines.foreground_launch_failed(&profile);
                self.engine_failed(*f);
                return false;
            }
        };
        self.log(false, format!("engine restart [{}] {}", profile.name, sanitize(&profile.command)));
        self.restart.unproven.insert(instance);
        let keep = self.config.engine.analysis.fast_engine_change;
        self.engines.set_foreground(Some(instance), keep);
        self.sync_engine();
        self.drain_engine_events();
        true
    }

    /// A match participant that ended during the match starts again after it; the one that held
    /// the foreground seat takes it back when nothing else did meanwhile. Not tried again.
    pub(super) fn restart_participant(&mut self, profile_id: &str, foreground: bool) {
        if self.engines.instance_of(profile_id).is_some() {
            return;
        }
        let Some(profile) = self.config.engine.engines.iter().find(|p| p.id == profile_id).cloned() else {
            return;
        };
        let seat = foreground && self.engines.foreground().is_none();
        match self.engines.start(&profile, self.bootstrap(&profile)) {
            Ok(instance) => {
                self.log(false, format!("engine restart [{}] {}", profile.name, sanitize(&profile.command)));
                self.restart.quiet.insert(instance);
                if seat {
                    let keep = self.config.engine.analysis.fast_engine_change;
                    self.engines.set_foreground(Some(instance), keep);
                    self.sync_engine();
                }
            }
            Err(f) => {
                if seat {
                    self.engines.foreground_launch_failed(&profile);
                }
                self.log(true, format!("engine restart [{}] failed: {}", profile.name, sanitize(&f.message)));
            }
        }
    }

    /// The instance started up: a participant's replacement reports its failures again.
    pub(super) fn restart_ready(&mut self, instance: InstanceId) {
        self.restart.quiet.remove(&instance);
    }

    /// The instance produced an accepted analysis line or a legal game move: an automatic restart
    /// is admitted for it again.
    pub(super) fn restart_proven(&mut self, instance: InstanceId) {
        self.restart.unproven.remove(&instance);
    }

    /// The record took a game engine's move: the engine board holds it, and the engine proved
    /// itself (`Leelaz.genmove` → `canCheckAlive = true`).
    pub(super) fn engine_move_committed(&mut self, game: u64, request: u64, stamp: u64) {
        if let Some(instance) = self.engines.game_instance(game) {
            self.restart_proven(instance);
        }
        self.engines.game_move_committed(game, request, stamp);
    }
}
