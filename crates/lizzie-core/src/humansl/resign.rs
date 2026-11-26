//! When the AI gives up (Java `training/AiResignation.forPlay`), after KataGo's human-imitation
//! configs: a policy level fights on like `gtp_human5k_example.cfg`, 20 turns in a row below 0.5%
//! and at least 40 points behind; a search level gives up like `gtp_human9d_search_example.cfg`,
//! after 10 turns below 1% and 20 points behind. Both wait until 40% of the board's points have
//! been played.
use super::PlaySettings;

/// KataGo's human configs resign only once this share of the board was played.
const MIN_BOARD_SHARE: f64 = 0.4;

/// One game's count of the AI's hopeless turns against its level's thresholds.
#[derive(Clone, Debug, PartialEq)]
pub struct Resignation {
    max_winrate: f64,
    min_points_behind: f64,
    turns_needed: u32,
    hopeless_turns: u32,
}

impl Resignation {
    /// The thresholds of the level that plays `play`.
    pub fn for_play(play: &PlaySettings) -> Self {
        let (max_winrate, min_points_behind, turns_needed) = if play.searches {
            (0.01, 20.0, 10)
        } else {
            (0.005, 40.0, 20)
        };
        Self {
            max_winrate,
            min_points_behind,
            turns_needed,
            hopeless_turns: 0,
        }
    }

    /// Records KataGo's view of the AI's position on its turn, its winrate (0 to 1) and score lead,
    /// both the AI's, at move `move_number` of a board of `area` points, and says whether it
    /// resigns now. A turn without a usable evaluation leaves the count unchanged.
    pub fn should_resign(&mut self, winrate: f64, score_lead: f64, move_number: u32, area: usize) -> bool {
        if !winrate.is_finite() || !score_lead.is_finite() {
            return false;
        }
        let hopeless = f64::from(move_number) >= MIN_BOARD_SHARE * area as f64
            && winrate <= self.max_winrate
            && score_lead <= -self.min_points_behind;
        self.hopeless_turns = if hopeless { self.hopeless_turns + 1 } else { 0 };
        self.hopeless_turns >= self.turns_needed
    }
}
