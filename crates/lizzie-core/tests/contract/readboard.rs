//! Contract types of 棋盘同步工具 (see `../contract.rs`).

use lizzie_core::api::*;
use lizzie_core::readboard::logging::{LoggingStatus, Persistence, Presentation};
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; ReadBoardCommand, PonderingNoticeDecision, ReadBoardView, FloatBoardView, FloatSceneView, ReadBoardNotice, HelperLoggingView, LoggingStatus, Persistence, Presentation]
}
