use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Web 旁观 (Menu.java:4263-4325, BottomToolbar.java:1237-1273).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(tag = "type")]
pub enum WebBoardCommand {
    /// 启动 Web 旁观: the page and board stream on this machine's network address.
    Start,
    /// 停止 Web 旁观: viewers are disconnected and a running 试下 ends.
    Stop,
    /// 强制结束试下 (`exitWebTrialFromToolbar`).
    ForceExitTrial,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, TS)]
pub struct WebBoardView {
    /// The access address while Web 旁观 runs (`WebBoardManager.getAccessUrl`).
    pub url: Option<String>,
    /// A viewer's 试下 drives the board (`LizzieFrame.isTrialActive`).
    pub trial_active: bool,
}
