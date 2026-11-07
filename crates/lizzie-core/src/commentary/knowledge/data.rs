use serde::Deserialize;

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Catalog {
    pub joseki_lines: Vec<JosekiLine>,
    pub life_death_problems: Vec<Problem>,
    pub tesuji_problems: Vec<Problem>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct JosekiLine {
    pub title: String,
    pub family: String,
    pub phase: Vec<String>,
    pub tags: Vec<String>,
    pub normalized_features: Vec<String>,
    pub relative_sequence: Vec<String>,
    pub decision_rules: Vec<String>,
    pub common_mistakes: Vec<String>,
    pub training_focus: Vec<String>,
    pub branches: Vec<Variation>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Variation {
    pub name: String,
    pub when_to_choose: String,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Problem {
    pub title: String,
    pub region: String,
    pub difficulty: String,
    pub objective: String,
    pub tags: Vec<String>,
    pub initial_stones: Vec<Stone>,
    pub correct_moves: Vec<TrainingMove>,
    pub failure_moves: Vec<TrainingMove>,
    pub teaching: ProblemTeaching,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct ProblemTeaching {
    pub recognition: Option<String>,
    pub tesuji_idea: Option<String>,
    pub explanation: Option<String>,
    pub memory_cue: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum TrainingMove {
    Point(String),
    Detail {
        #[serde(default)]
        r#move: Option<String>,
        #[serde(default)]
        gtp: Option<String>,
        #[serde(default)]
        why: Option<String>,
    },
}

impl TrainingMove {
    pub fn point(&self) -> Option<&str> {
        match self {
            Self::Point(point) => Some(point),
            Self::Detail { r#move, gtp, .. } => r#move.as_deref().or(gtp.as_deref()),
        }
    }
    pub fn failure_point(&self) -> Option<&str> {
        match self {
            Self::Point(point) => Some(point),
            Self::Detail { r#move, .. } => r#move.as_deref(),
        }
    }
    pub fn why(&self) -> &str {
        match self {
            Self::Detail { why: Some(why), .. } => why,
            _ => "次序错误",
        }
    }
}

#[derive(Deserialize)]
pub(super) struct Stone {
    #[serde(default)]
    pub point: Option<String>,
    #[serde(default)]
    pub gtp: Option<String>,
    #[serde(default = "black")]
    pub color: String,
}
fn black() -> String {
    "B".into()
}
impl Stone {
    pub fn point(&self) -> Option<&str> {
        self.point.as_deref().or(self.gtp.as_deref())
    }
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Pattern {
    pub title: String,
    pub category: String,
    pub pattern_type: String,
    pub phase: Vec<String>,
    pub levels: Vec<String>,
    pub regions: Vec<String>,
    pub tags: Vec<String>,
    pub aliases: Vec<String>,
    pub board_signals: Vec<String>,
    pub triggers: Triggers,
    pub variations: Vec<Variation>,
    pub teaching: PatternTeaching,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Triggers {
    pub move_features: Vec<String>,
    pub candidate_features: Vec<String>,
    pub pv_features: Vec<String>,
    pub min_move_number: Option<u32>,
    pub max_move_number: Option<u32>,
    pub min_loss_score: Option<f64>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct PatternTeaching {
    pub recognition: String,
    pub correct_idea: String,
    pub memory_cue: String,
    pub common_mistake: String,
    pub drill: String,
}

pub(super) struct Library {
    pub catalog: Catalog,
    pub patterns: Vec<Pattern>,
}
impl Library {
    pub fn load() -> Result<Self, serde_json::Error> {
        let mut catalog: Catalog =
            serde_json::from_str(include_str!("../../../resources/knowledge/training-catalog.json"))?;
        // Java's loader excludes problems with no usable initial stones.
        for problems in [&mut catalog.life_death_problems, &mut catalog.tesuji_problems] {
            for problem in problems.iter_mut() {
                problem.initial_stones.retain(|stone| stone.point().is_some());
                problem.correct_moves.retain(|mv| mv.point().is_some());
                problem.failure_moves.retain(|mv| mv.failure_point().is_some());
            }
            problems.retain(|problem| !problem.initial_stones.is_empty());
        }
        Ok(Self {
            catalog,
            patterns: serde_json::from_str(include_str!("../../../resources/knowledge/pattern-cards.json"))?,
        })
    }
}
