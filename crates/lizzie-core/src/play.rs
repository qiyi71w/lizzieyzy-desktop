//! Rules of 人机对弈 that need no engine or document: the human's countdown
//! (`LizzieFrame.countDownForHuman`), the engine's clock that feeds `time_left`
//! (`EngineCountDown`), the engine time commands (`LizzieFrame.sendAiTime`) and the 自动落子
//! decision (`Leelaz.notifyAutoPlay`, `randomBestmove`).

use crate::settings::{AiTimeMode, AiTimeSettings, AutoPlaySettings, KataTimeType, RandomStartSettings};

/// The human's clock: 保留时间 (minutes), then 读秒 periods. Advanced one second at a time while
/// it is the human's turn, like Java's one-second timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HumanClock {
    pub minutes: i32,
    pub seconds: i32,
    /// Remaining 读秒 periods.
    pub byo_times: i32,
    /// Seconds left in the current period; -1 without 读秒 (or once a period is used up).
    pub byo_seconds: i32,
    /// The configured period length (-1 for 无).
    period: i32,
}

/// What one second of the human's clock did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockTick {
    Running,
    /// `Utils.playByoyomi(n)`: the countdown voice for the last ten seconds of a period.
    Voice(u8),
    /// Time and every period used up: the human loses on time.
    Expired,
}

impl HumanClock {
    /// `countDownForHuman(minutes, seconds, times)`; -1 minutes or seconds mean 无.
    pub fn new(minutes: i32, seconds: i32, times: i32) -> HumanClock {
        HumanClock {
            minutes,
            seconds: 0,
            byo_times: times,
            byo_seconds: if seconds > 0 { seconds } else { -1 },
            period: seconds,
        }
    }

    pub fn tick(&mut self) -> ClockTick {
        if self.seconds > 0 {
            self.seconds -= 1;
        } else if self.minutes > 0 {
            self.minutes -= 1;
            self.seconds = 59;
        } else if self.byo_seconds >= 0 {
            let voice = (1..=10).contains(&self.byo_seconds).then(|| (self.byo_seconds - 1) as u8);
            self.byo_seconds -= 1;
            if let Some(n) = voice {
                return ClockTick::Voice(n);
            }
        } else if self.byo_times > 1 {
            self.byo_times -= 1;
            self.byo_seconds = self.period;
        } else {
            return ClockTick::Expired;
        }
        ClockTick::Running
    }

    /// `tryToResetByoTime`: a move starts the period again.
    pub fn moved(&mut self) {
        self.byo_seconds = self.period;
    }
}

/// `EngineCountDown.TimeType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EngineTime {
    /// `time_settings main byo stones` (Canadian), whole seconds.
    Canadian { byo: i64, stones: i64 },
    KataNone,
    KataAbsolute,
    KataCanadian { byo: i64, stones: i64 },
    KataByoyomi { byo: i64 },
    KataFischer { increment: i64 },
    KataFischerCapped { increment: i64, limit: i64, per_move: i64 },
}

/// The engine's remaining time as the client tracks it, told to the engine as `time_left` before
/// each of its moves. KataGo times are kept in centiseconds (Java counts down 0.01 s steps).
#[derive(Clone, Debug, PartialEq)]
pub struct EngineClock {
    kind: EngineTime,
    color: &'static str,
    main: i64,
    byo: i64,
    stones: i64,
    periods: i64,
    /// Sub-second remainder of Canadian time (`tempTimes`).
    pending: i64,
}

fn centis(text: &str) -> Option<i64> {
    let v: f64 = text.parse().ok()?;
    v.is_finite().then(|| (v * 100.0).round() as i64)
}

impl EngineClock {
    /// `setEngineCountDown(line)` + `initialize(engine_black)`; `None` when the command is not one
    /// the clock understands (`EngineManager.parseAdvcanceTimeSettingsFailed`).
    pub fn parse(line: &str, engine_black: bool) -> Option<EngineClock> {
        let line = line.trim().to_lowercase();
        let p: Vec<&str> = line.split_whitespace().collect();
        let int = |s: &str| s.parse::<i64>().ok();
        // (kind, main, byo, stones, periods)
        let (kind, main, byo, stones, periods) = match p.as_slice() {
            ["time_settings", m, b, s] => {
                let (byo, stones) = (int(b)?, int(s)?);
                (EngineTime::Canadian { byo, stones }, int(m)?, byo, stones, 0)
            }
            ["kata-time_settings", "none"] => (EngineTime::KataNone, 0, 0, 0, 0),
            ["kata-time_settings", "absolute", m] => (EngineTime::KataAbsolute, centis(m)?, 0, 0, 0),
            ["kata-time_settings", "canadian", m, b, s] => {
                let (byo, stones) = (centis(b)?, int(s)?);
                (EngineTime::KataCanadian { byo, stones }, centis(m)?, byo, stones, 0)
            }
            ["kata-time_settings", "byoyomi", m, b, n] => {
                let byo = centis(b)?;
                (EngineTime::KataByoyomi { byo }, centis(m)?, byo, 0, int(n)?)
            }
            ["kata-time_settings", "fischer", m, i] => (EngineTime::KataFischer { increment: centis(i)? }, centis(m)?, 0, 0, 0),
            ["kata-time_settings", "fischer-capped", m, i, l, x] => {
                let (main, limit) = (centis(m)?, centis(l)?);
                if limit > 0 && limit < main {
                    return None;
                }
                (EngineTime::KataFischerCapped { increment: centis(i)?, limit, per_move: centis(x)? }, main, 0, 0, 0)
            }
            _ => return None,
        };
        Some(EngineClock { kind, color: if engine_black { "B" } else { "W" }, main, byo, stones, periods, pending: 0 })
    }

    /// `countDownCentiseconds`, `n` times: the engine thought for `n` centiseconds.
    pub fn elapse(&mut self, n: u64) {
        for _ in 0..n {
            match self.kind {
                EngineTime::KataNone => return,
                EngineTime::Canadian { .. } => {
                    self.pending += 1;
                    if self.pending >= 100 {
                        self.pending = 0;
                        if self.main > 0 {
                            self.main -= 1;
                        } else if self.byo > 0 {
                            self.byo -= 1;
                        }
                    }
                }
                EngineTime::KataCanadian { .. } => {
                    if self.main > 0 {
                        self.main -= 1;
                    } else if self.byo > 0 {
                        self.byo -= 1;
                    }
                }
                EngineTime::KataByoyomi { byo } => {
                    if self.main > 0 {
                        self.main -= 1;
                    } else if self.byo > 0 {
                        self.byo -= 1;
                    } else if self.periods > 0 {
                        self.periods -= 1;
                        self.byo = byo;
                    }
                }
                EngineTime::KataFischer { .. } | EngineTime::KataFischerCapped { .. } | EngineTime::KataAbsolute => {
                    self.main = (self.main - 1).max(0);
                }
            }
        }
    }

    /// `claimTimeLeftCommand`: advances to the next move boundary and gives the `time_left` to send
    /// before the engine's move (none for `kata-time_settings none`).
    pub fn claim(&mut self) -> Option<String> {
        let seconds = |c: i64| format!("{:.2}", c.max(0) as f64 / 100.0);
        let (time, moves) = match self.kind {
            EngineTime::KataNone => return None,
            EngineTime::Canadian { byo, stones } => {
                if self.main <= 0 {
                    self.stones -= 1;
                    if self.stones <= 0 {
                        self.stones = stones;
                        self.byo = byo;
                    }
                    (self.byo.max(0).to_string(), self.stones)
                } else {
                    (self.main.to_string(), 0)
                }
            }
            EngineTime::KataCanadian { byo, stones } => {
                if self.main <= 0 {
                    self.stones -= 1;
                    if self.stones <= 0 {
                        self.stones = stones;
                        self.byo = byo;
                    }
                    (seconds(self.byo), self.stones)
                } else {
                    (seconds(self.main), 0)
                }
            }
            EngineTime::KataByoyomi { byo } => {
                if self.main <= 0 {
                    self.byo = byo;
                    (seconds(self.byo), self.periods)
                } else {
                    (seconds(self.main), 0)
                }
            }
            EngineTime::KataFischer { increment } => {
                self.main += increment;
                (seconds(self.main), 0)
            }
            EngineTime::KataFischerCapped { increment, limit, per_move } => {
                self.main += increment;
                if limit > 0 {
                    self.main = self.main.min(limit);
                }
                let this_move = if per_move > 0 { per_move.min(self.main) } else { self.main };
                (seconds(this_move), 0)
            }
            EngineTime::KataAbsolute => (seconds(self.main), 0),
        };
        Some(format!("time_left {} {time} {moves}", self.color))
    }
}

/// The engine time commands of a Genmove game (`LizzieFrame.sendAiTime`), the command the engine's
/// clock follows (`installPlayingAgainstHumanCountDown`), and whether KataGo time was set for an
/// engine that is not KataGo (`LizzieFrame.sendTimes.kataGoTimeMismatch`).
pub struct AiTimeCommands {
    pub commands: Vec<String>,
    pub clock: Option<String>,
    pub kata_mismatch: bool,
}

pub fn ai_time_commands(s: &AiTimeSettings, katago: bool) -> AiTimeCommands {
    let mut out = AiTimeCommands { commands: Vec::new(), clock: None, kata_mismatch: false };
    match s.mode {
        AiTimeMode::EngineOwned => {}
        AiTimeMode::Advanced => {
            out.commands.push(s.advanced.clone());
            out.clock = Some(s.advanced.clone());
        }
        AiTimeMode::KataGo => {
            let main = s.kata_main_minutes * 60;
            let command = match s.kata_type {
                KataTimeType::Byoyomi => {
                    format!("kata-time_settings byoyomi {main} {} {}", s.kata_byoyomi_seconds, s.kata_byoyomi_times)
                }
                KataTimeType::Fischer => format!("kata-time_settings fischer {main} {}", s.kata_fischer_increment),
                KataTimeType::Absolute => format!("kata-time_settings absolute {main}"),
            };
            out.commands.push(command.clone());
            out.clock = Some(command);
            out.kata_mismatch = !katago;
        }
        AiTimeMode::Fixed => {
            let seconds = s.fixed_seconds;
            if katago {
                out.commands.push("kata-time_settings none".to_string());
                out.commands.push(if seconds == 0 {
                    "kata-set-param maxTime 1e20".to_string()
                } else {
                    format!("kata-set-param maxTime {seconds}")
                });
            } else {
                let command = format!("time_settings 0 {seconds} 1");
                out.commands.push(command.clone());
                if seconds > 0 {
                    out.clock = Some(command);
                }
            }
        }
    }
    out
}

/// One candidate as the 自动落子 decision sees it (`MoveData`): win rate in percent for the side
/// to move.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Choice<P> {
    pub point: P,
    pub winrate: f64,
    pub visits: u64,
}

/// `notifyAutoPlay`'s `playNow`: a condition is set and met by the current search.
pub fn autoplay_due(a: &AutoPlaySettings, first_visits: u64, total_visits: u64, elapsed_millis: u128) -> bool {
    (a.first_playouts_enabled && a.first_playouts > 0 && first_visits >= a.first_playouts as u64)
        || (a.playouts_enabled && a.playouts > 0 && total_visits >= a.playouts as u64)
        || (a.time_enabled && a.time_seconds > 0 && elapsed_millis >= a.time_seconds as u128 * 1000)
}

/// AI随机开局 (`randomBestmove` on the 自动落子 path): a uniformly random entry of a list holding
/// the best candidate once and every candidate within `winrate_diff` of it once, plus once more
/// when it has at least `playouts_diff` percent of the most searched candidate's visits.
/// `pick(n)` chooses an index below `n`. Java compares with the most searched candidate only
/// when the unrelated `check-random-visits` option is on (otherwise it divides by zero and
/// doubles every candidate); here the comparison always uses it.
pub fn random_choice<P: Copy>(candidates: &[Choice<P>], r: &RandomStartSettings, pick: impl FnOnce(usize) -> usize) -> Option<P> {
    let best = candidates.first()?;
    let most = candidates.iter().map(|c| c.visits).max().unwrap_or(0);
    let mut entries = vec![best.point];
    for c in candidates[1..].iter().filter(|c| c.winrate >= best.winrate - r.winrate_diff) {
        if r.playouts_diff > 0.0 && most > 0 && c.visits as f64 / most as f64 >= r.playouts_diff / 100.0 {
            entries.push(c.point);
        }
        entries.push(c.point);
    }
    Some(entries[pick(entries.len()).min(entries.len() - 1)])
}

/// `notifyAnaResign`: from `start_move` on, a first candidate below `percent` is one more losing
/// decision; any other resets the count. Returns the new count.
pub fn resign_count(count: u32, move_number: u32, best_winrate: Option<f64>, start_move: u32, percent: f64) -> u32 {
    if move_number < start_move {
        return count;
    }
    match best_winrate {
        Some(w) if w < percent => count + 1,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_clock_runs_main_time_then_periods_then_expires() {
        // No main time, 3-second periods, two of them.
        let mut c = HumanClock::new(-1, 3, 2);
        assert_eq!(c.tick(), ClockTick::Voice(2));
        assert_eq!(c.tick(), ClockTick::Voice(1));
        assert_eq!(c.tick(), ClockTick::Voice(0));
        assert_eq!(c.tick(), ClockTick::Running); // the period's last second
        assert_eq!(c.tick(), ClockTick::Running); // the next period starts
        assert_eq!((c.byo_times, c.byo_seconds), (1, 3));
        c.moved();
        assert_eq!(c.byo_seconds, 3);
        for _ in 0..4 {
            c.tick();
        }
        assert_eq!(c.tick(), ClockTick::Expired);
    }

    #[test]
    fn human_clock_main_time_counts_minutes_down() {
        let mut c = HumanClock::new(1, -1, 1);
        c.tick();
        assert_eq!((c.minutes, c.seconds), (0, 59));
        for _ in 0..59 {
            c.tick();
        }
        assert_eq!((c.minutes, c.seconds), (0, 0));
        assert_eq!(c.tick(), ClockTick::Expired);
    }

    #[test]
    fn engine_clock_time_left_follows_java_counting() {
        let mut c = EngineClock::parse("kata-time_settings byoyomi 1 5 2", true).unwrap();
        assert_eq!(c.claim().as_deref(), Some("time_left B 1.00 0"));
        c.elapse(150); // main time used up, half a second into the period
        assert_eq!(c.claim().as_deref(), Some("time_left B 5.00 2"));
        let mut f = EngineClock::parse("kata-time_settings fischer 10 5", false).unwrap();
        f.elapse(300);
        assert_eq!(f.claim().as_deref(), Some("time_left W 12.00 0"));
        let mut t = EngineClock::parse("time_settings 0 2 1", false).unwrap();
        assert_eq!(t.claim().as_deref(), Some("time_left W 2 1"));
        assert!(EngineClock::parse("kgs-time_settings byoyomi 1 2 3", true).is_none());
        assert!(EngineClock::parse("kata-time_settings none", true).unwrap().claim().is_none());
    }

    #[test]
    fn fixed_time_commands_differ_for_katago() {
        let s = AiTimeSettings::default();
        assert_eq!(ai_time_commands(&s, true).commands, ["kata-time_settings none", "kata-set-param maxTime 2"]);
        let other = ai_time_commands(&s, false);
        assert_eq!(other.commands, ["time_settings 0 2 1"]);
        assert_eq!(other.clock.as_deref(), Some("time_settings 0 2 1"));
        let kata = AiTimeSettings { mode: AiTimeMode::KataGo, ..s };
        let k = ai_time_commands(&kata, false);
        assert_eq!(k.commands, ["kata-time_settings byoyomi 600 5 3"]);
        assert!(k.kata_mismatch);
    }

    #[test]
    fn random_choice_weights_close_candidates_by_their_visits() {
        let c = |p: u8, winrate: f64, visits: u64| Choice { point: p, winrate, visits };
        let r = RandomStartSettings { enabled: true, moves: 20, winrate_diff: 1.0, playouts_diff: 10.0 };
        let candidates = [c(0, 50.0, 1000), c(1, 49.5, 200), c(2, 49.5, 50), c(3, 45.0, 900)];
        let mut entries = Vec::new();
        for i in 0..5 {
            entries.push(random_choice(&candidates, &r, |n| {
                assert_eq!(n, 4, "best once, 1 twice (enough visits), 2 once, 3 too far behind");
                i
            }));
        }
        assert_eq!(entries, [Some(0), Some(1), Some(1), Some(2), Some(2)]);
    }

    #[test]
    fn resign_needs_consecutive_losing_decisions_after_the_start_move() {
        assert_eq!(resign_count(0, 100, Some(1.0), 150, 5.0), 0);
        assert_eq!(resign_count(0, 150, Some(1.0), 150, 5.0), 1);
        assert_eq!(resign_count(1, 151, Some(30.0), 150, 5.0), 0);
    }
}
