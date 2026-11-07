//! Grounded commentary knowledge matching from Java cb53acc's MatchEngine.
//!
//! Only the two classpath catalogues reached by TeacherEvidence are embedded;
//! the separate joseki recognizer, SGF dictionaries and training UI are not on
//! this path. Query construction belongs to the commentary evidence module.
mod data;
mod engine;
mod features;
mod geometry;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, LazyLock, Mutex};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KnowledgeQuery {
    pub board_size: u32,
    pub move_number: u32,
    pub played_move: String,
    pub text: String,
    pub recent_moves: Vec<KnowledgeMove>,
    pub candidate_moves: Vec<String>,
    pub principal_variation: Vec<String>,
    pub board_snapshot: Vec<SnapshotStone>,
    pub loss_score: Option<f64>,
}
impl Default for KnowledgeQuery {
    fn default() -> Self {
        Self {
            board_size: 19,
            move_number: 0,
            played_move: String::new(),
            text: String::new(),
            recent_moves: Vec::new(),
            candidate_moves: Vec::new(),
            principal_variation: Vec::new(),
            board_snapshot: Vec::new(),
            loss_score: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct KnowledgeMove {
    pub col: i32,
    pub row: i32,
    pub gtp: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SnapshotStone {
    pub point: String,
    pub color: char,
}

static LIBRARY: LazyLock<Mutex<Option<Arc<data::Library>>>> = LazyLock::new(|| Mutex::new(None));
fn library() -> Option<Arc<data::Library>> {
    let mut cache = LIBRARY.lock().unwrap_or_else(|poison| poison.into_inner());
    if cache.is_none() {
        *cache = data::Library::load().ok().map(Arc::new);
    }
    cache.clone()
}

/// Return at most two grounded, titled matches, in Java prompt order.
/// Missing/invalid catalogue data yields an empty string, as TeacherEvidence does.
/// Parsing is process-wide and lazy; matching runs without holding the cache lock.
pub fn match_text(query: &KnowledgeQuery) -> String {
    library().map_or_else(String::new, |library| engine::match_text(&library, query))
}

/// Discard parsed knowledge; the next request lazily loads the embedded catalogues.
/// In-flight requests retain their immutable snapshot. Java's reload only cleared
/// the separate joseki-recognizer caches, not commentary's training/pattern holders;
/// clearing these immutable embedded catalogues has the same observable output.
pub fn reload() {
    *LIBRARY.lock().unwrap_or_else(|poison| poison.into_inner()) = None;
}

#[cfg(test)]
mod tests;
