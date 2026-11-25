//! The opponents of AI 陪练, weakest first: the rows of the community table that lines KataGo's
//! human-style levels up with Fox / Tygem ranks, each with the HumanSL settings it was measured
//! with (Java `training/CoachLevel`, `HumanSlPlaySettings`). Up to 1 kyu the AI plays the raw
//! policy of a pre-AlphaZero profile; from 1 dan it searches, and each 强 half step sits between
//! two dan levels.
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;

/// One opponent. Saved by name; Java's `KYU_10` … `DAN_11` names read as aliases.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum CoachLevel {
    #[serde(alias = "KYU_10")]
    Kyu10,
    #[serde(alias = "KYU_9")]
    Kyu9,
    #[serde(alias = "KYU_8")]
    Kyu8,
    #[serde(alias = "KYU_7")]
    Kyu7,
    #[serde(alias = "KYU_6")]
    Kyu6,
    #[serde(alias = "KYU_5")]
    Kyu5,
    #[serde(alias = "KYU_4")]
    Kyu4,
    #[serde(alias = "KYU_3")]
    Kyu3,
    #[serde(alias = "KYU_2")]
    Kyu2,
    #[serde(alias = "KYU_1")]
    Kyu1,
    #[serde(alias = "DAN_1")]
    Dan1,
    #[serde(alias = "STRONG_DAN_1")]
    StrongDan1,
    #[serde(alias = "DAN_2")]
    Dan2,
    #[serde(alias = "STRONG_DAN_2")]
    StrongDan2,
    #[default]
    #[serde(alias = "DAN_3")]
    Dan3,
    #[serde(alias = "STRONG_DAN_3")]
    StrongDan3,
    #[serde(alias = "DAN_4")]
    Dan4,
    #[serde(alias = "STRONG_DAN_4")]
    StrongDan4,
    #[serde(alias = "DAN_5")]
    Dan5,
    #[serde(alias = "STRONG_DAN_5")]
    StrongDan5,
    #[serde(alias = "DAN_6")]
    Dan6,
    #[serde(alias = "STRONG_DAN_6")]
    StrongDan6,
    #[serde(alias = "DAN_7")]
    Dan7,
    #[serde(alias = "STRONG_DAN_7")]
    StrongDan7,
    #[serde(alias = "DAN_8")]
    Dan8,
    #[serde(alias = "STRONG_DAN_8")]
    StrongDan8,
    #[serde(alias = "DAN_9")]
    Dan9,
    #[serde(alias = "STRONG_DAN_9")]
    StrongDan9,
    #[serde(alias = "DAN_10")]
    Dan10,
    #[serde(alias = "STRONG_DAN_10")]
    StrongDan10,
    #[serde(alias = "DAN_11")]
    Dan11,
}

/// A plain level, or the 强 half step above a dan level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
pub enum CoachLevelKind {
    Kyu,
    Dan,
    StrongDan,
}

/// How KataGo picks the AI's moves at one level, after the `gtp_human*_example.cfg` configs the
/// levels were tuned with. A policy level (`searches` false) plays the raw HumanSL policy of its
/// profile and keeps the other values of its config for reference; a search level searches and
/// shifts the human policy towards the moves the search likes (PiKL): the lower the lambda, the
/// more KataGo and the less the human model decides.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlaySettings {
    /// The HumanSL profile, such as `preaz_11k` or `rank_3k`.
    pub profile: &'static str,
    pub searches: bool,
    /// KataGo's visits per move.
    pub max_visits: u32,
    /// `humanSLChosenMovePiklLambda`: utility a move may lose before its human odds are cut.
    pub pikl_lambda: f64,
    /// `humanSLRootExploreProbWeightless`: share of root visits spent on human moves.
    pub root_explore_weightless: f64,
    /// `humanSLCpuctPermanent`: how far human moves are explored whatever they lose.
    pub cpuct_permanent: f64,
    /// `chosenMoveTemperatureOnlyBelowProb`: the temperature only damps moves rarer than this.
    pub temperature_only_below_prob: f64,
    pub static_score_utility: f64,
    pub dynamic_score_utility: f64,
    /// How many times the coach's ten seconds per move the level may search.
    pub move_time_factor: u32,
}

/// The coach's search budget per move; [`PlaySettings::move_time_factor`] multiplies it.
const MOVE_SECONDS: u64 = 10;

impl PlaySettings {
    /// A level that plays the raw policy of `profile`, as `gtp_human5k_example.cfg` does.
    const fn policy(profile: &'static str) -> Self {
        Self {
            profile,
            searches: false,
            max_visits: 40,
            pikl_lambda: 100_000_000.0,
            root_explore_weightless: 0.0,
            cpuct_permanent: 1000.0,
            temperature_only_below_prob: 0.01,
            static_score_utility: 0.3,
            dynamic_score_utility: 0.0,
            move_time_factor: 1,
        }
    }
    /// A search level with the values of `gtp_human9d_search_example.cfg`'s family.
    const fn search(profile: &'static str) -> Self {
        Self {
            profile,
            searches: true,
            max_visits: 60,
            pikl_lambda: 0.75,
            root_explore_weightless: 0.8,
            cpuct_permanent: 5.0,
            temperature_only_below_prob: 1.0,
            static_score_utility: 0.5,
            dynamic_score_utility: 0.5,
            move_time_factor: 1,
        }
    }
    /// The 强 configs: a `rank_` profile one step up, explored less, with human odds cut later.
    const fn half_step(profile: &'static str) -> Self {
        Self::search(profile)
            .pikl(1.25)
            .root_weightless(0.1)
            .only_below_prob(0.01)
    }
    const fn visits(self, max_visits: u32) -> Self {
        Self { max_visits, ..self }
    }
    const fn pikl(self, pikl_lambda: f64) -> Self {
        Self { pikl_lambda, ..self }
    }
    const fn root_weightless(self, root_explore_weightless: f64) -> Self {
        Self {
            root_explore_weightless,
            ..self
        }
    }
    const fn permanent_exploration(self, cpuct_permanent: f64) -> Self {
        Self {
            cpuct_permanent,
            ..self
        }
    }
    const fn only_below_prob(self, temperature_only_below_prob: f64) -> Self {
        Self {
            temperature_only_below_prob,
            ..self
        }
    }
    const fn score_utility(self, value: f64) -> Self {
        Self {
            static_score_utility: value,
            dynamic_score_utility: value,
            ..self
        }
    }
    const fn move_time(self, move_time_factor: u32) -> Self {
        Self {
            move_time_factor,
            ..self
        }
    }

    /// `chosenMoveTemperatureEarly`: the move temperature at the start of the game.
    pub fn temperature_early(&self) -> f64 {
        if self.searches {
            0.70
        } else {
            0.85
        }
    }
    /// `chosenMoveTemperature`: the move temperature later on.
    pub fn temperature(&self) -> f64 {
        if self.searches {
            0.40
        } else {
            0.70
        }
    }
    /// `chosenMoveTemperatureHalflife`: moves until the temperature is halfway there.
    pub fn temperature_halflife(&self) -> f64 {
        if self.searches {
            50.0
        } else {
            80.0
        }
    }
    /// `humanSLChosenMoveIgnorePass`: passing keeps the share KataGo's search gives it.
    pub fn ignore_pass(&self) -> bool {
        self.searches
    }
    /// The search budget of one move: ten seconds, twice that for the 200- and 400-visit levels
    /// so that slower machines still reach their visits. No setting changes it.
    pub fn move_budget(&self) -> Duration {
        Duration::from_secs(MOVE_SECONDS * u64::from(self.move_time_factor))
    }
}

struct Row {
    kind: CoachLevelKind,
    rank: u8,
    fox: &'static str,
    play: PlaySettings,
}

const fn row(kind: CoachLevelKind, rank: u8, fox: &'static str, play: PlaySettings) -> Row {
    Row { kind, rank, fox, play }
}

use CoachLevelKind::{Dan, Kyu, StrongDan};
use PlaySettings as P;

/// In [`CoachLevel`] order.
const ROWS: [Row; 31] = [
    row(Kyu, 10, "17-18K", P::policy("preaz_20k")),
    row(Kyu, 9, "15-16K", P::policy("preaz_18k")),
    row(Kyu, 8, "13-14K", P::policy("preaz_16k")),
    row(Kyu, 7, "11-12K", P::policy("preaz_14k")),
    row(Kyu, 6, "9-10K", P::policy("preaz_12k")),
    row(Kyu, 5, "7-8K", P::policy("preaz_11k")),
    row(Kyu, 4, "5-6K", P::policy("preaz_10k")),
    row(Kyu, 3, "3-4K", P::policy("preaz_9k")),
    row(Kyu, 2, "2K", P::policy("preaz_8k")),
    row(Kyu, 1, "1K", P::policy("preaz_7k")),
    row(Dan, 1, "1D", P::search("rank_5k")),
    row(StrongDan, 1, "1-2D", P::half_step("rank_4k")),
    row(Dan, 2, "2D", P::search("rank_4k")),
    row(StrongDan, 2, "2-3D", P::half_step("rank_3k")),
    row(Dan, 3, "3D", P::search("rank_3k")),
    row(StrongDan, 3, "3-4D", P::half_step("rank_2k")),
    row(Dan, 4, "4D", P::search("rank_2k")),
    row(StrongDan, 4, "4-5D", P::half_step("rank_1k")),
    row(Dan, 5, "5D", P::search("rank_1k")),
    row(StrongDan, 5, "5-6D", P::half_step("rank_1d")),
    row(Dan, 6, "6D", P::search("rank_1d").pikl(1.25)),
    row(
        StrongDan,
        6,
        "6-7D",
        P::search("rank_2d").visits(80).pikl(1.0).root_weightless(0.2),
    ),
    row(
        Dan,
        7,
        "7D",
        P::search("rank_3d").visits(80).pikl(1.25).root_weightless(0.2),
    ),
    row(
        StrongDan,
        7,
        "7-8D",
        P::search("rank_3d").visits(80).pikl(1.0).root_weightless(0.5),
    ),
    row(
        Dan,
        8,
        "8D",
        P::search("rank_3d").visits(80).pikl(0.9).score_utility(1.0),
    ),
    row(
        StrongDan,
        8,
        "8-9D",
        P::search("rank_4d")
            .visits(100)
            .pikl(1.25)
            .root_weightless(0.2)
            .score_utility(1.0),
    ),
    row(
        Dan,
        9,
        "9D",
        P::search("rank_4d")
            .visits(100)
            .pikl(0.95)
            .root_weightless(0.9)
            .score_utility(1.0),
    ),
    row(
        StrongDan,
        9,
        "9D",
        P::search("rank_6d").visits(100).pikl(0.95).root_weightless(0.4),
    ),
    row(
        Dan,
        10,
        "9D",
        P::search("rank_7d")
            .visits(100)
            .pikl(0.8)
            .root_weightless(0.3)
            .permanent_exploration(2.0),
    ),
    row(
        StrongDan,
        10,
        "10D",
        P::search("rank_8d")
            .visits(200)
            .pikl(0.25)
            .root_weightless(0.9)
            .move_time(2),
    ),
    row(
        Dan,
        11,
        "10D",
        P::search("rank_9d")
            .visits(400)
            .pikl(0.01)
            .root_weightless(1.0)
            .move_time(2),
    ),
];

impl CoachLevel {
    /// Every level, weakest first.
    pub const ALL: [CoachLevel; 31] = {
        use CoachLevel::*;
        [
            Kyu10,
            Kyu9,
            Kyu8,
            Kyu7,
            Kyu6,
            Kyu5,
            Kyu4,
            Kyu3,
            Kyu2,
            Kyu1,
            Dan1,
            StrongDan1,
            Dan2,
            StrongDan2,
            Dan3,
            StrongDan3,
            Dan4,
            StrongDan4,
            Dan5,
            StrongDan5,
            Dan6,
            StrongDan6,
            Dan7,
            StrongDan7,
            Dan8,
            StrongDan8,
            Dan9,
            StrongDan9,
            Dan10,
            StrongDan10,
            Dan11,
        ]
    };

    fn row(self) -> &'static Row {
        &ROWS[self as usize]
    }
    pub fn kind(self) -> CoachLevelKind {
        self.row().kind
    }
    /// The number of the KataGo level: 3 for 3 dan and for 强3段.
    pub fn rank(self) -> u8 {
        self.row().rank
    }
    /// The Fox / Tygem rank, such as `3D`, `8-9D` or `17-18K`.
    pub fn fox(self) -> &'static str {
        self.row().fox
    }
    pub fn play(self) -> &'static PlaySettings {
        &self.row().play
    }
    /// The level an older setting named by its number: the plain level of that number, clamped to
    /// the table (kyu ranks weaker than 10 kyu become 10 kyu, which already plays the weakest
    /// HumanSL profile).
    pub fn for_rank(rank: i64, dan: bool) -> CoachLevel {
        let (kind, bounded) = if dan {
            (Dan, rank.clamp(1, 11))
        } else {
            (Kyu, rank.clamp(1, 10))
        };
        Self::ALL
            .into_iter()
            .find(|level| level.kind() == kind && i64::from(level.rank()) == bounded)
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_follow_the_level_order() {
        for (index, level) in CoachLevel::ALL.into_iter().enumerate() {
            assert_eq!(level as usize, index);
        }
    }
}
