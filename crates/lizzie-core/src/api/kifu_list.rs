//! What the 野狐棋谱 and 腾讯棋谱 windows share: their page buttons and their message boxes.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// |< < > >| over a game list (25 games a page).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
pub enum KifuPage {
    First,
    Previous,
    Next,
    Last,
}

/// A message box of a kifu window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
pub struct KifuNotice {
    pub id: u64,
    /// The Java message key; a `getKifuFailed` key is followed by `detail`.
    pub message_key: String,
    pub detail: String,
}
