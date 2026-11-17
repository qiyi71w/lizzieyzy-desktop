//! Contract types of 在线棋谱 / 弈客直播 (see `../contract.rs`).

use lizzie_core::api::*;
use ts_rs::Config;

pub fn decls(cfg: &Config) -> Vec<String> {
    crate::decls![cfg; OnlineCommand, LiveCategory, OnlineText, LiveGameView, LiveListView, OnlineFollowView, OnlineStatusView, ProxyWarningView, OnlineView]
}
