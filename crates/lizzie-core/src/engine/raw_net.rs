//! Pure network (`Leelaz.toggleHeatmap`), distinct from search ownership / KataGo estimate.
//! Payloads are published only after a complete, command-owned GTP response.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::record::{BoardSize, NodeId, Stone};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum RawNetMode {
    #[default]
    Off,
    Policy,
    Ownership,
}

#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct RawNetView {
    pub mode: RawNetMode,
    pub node: NodeId,
    /// Row-major probabilities (Java renders `(int)(policy * 1000)`).
    pub policy: Option<Vec<f64>>,
    /// White's win probability, independent of the player to move.
    pub white_winrate: Option<f64>,
    pub white_lead: Option<f64>,
    pub symmetry: Option<u8>,
    /// Black-positive ownership; exposed only in Ownership mode.
    pub ownership: Option<Vec<f64>>,
}

impl RawNetView {
    pub(crate) fn pending(mode: RawNetMode, node: NodeId) -> Self {
        Self { mode, node, policy: None, white_winrate: None, white_lead: None, symmetry: None, ownership: None }
    }
}

/// KataGo v1.18.2 `docs/GTP_Extensions.md`, `kata-raw-nn`: keys can be permuted and
/// whitespace is arbitrary. Illegal policy points may be NAN; like Java they draw as zero.
/// Unknown fields are skipped, without interpreting their numeric payload as another key.
pub(crate) fn parse_kata(text: &str, size: BoardSize, mode: RawNetMode, node: NodeId) -> Option<RawNetView> {
    let mut view = RawNetView::pending(mode, node);
    let count = usize::from(size.width) * usize::from(size.height);
    let mut tokens = text.split_whitespace();
    while let Some(key) = tokens.next() {
        match key {
            "symmetry" => view.symmetry = tokens.next()?.parse::<u8>().ok().filter(|n| *n < 8),
            "whiteWin" => view.white_winrate = tokens.next()?.parse::<f64>().ok().filter(|n| n.is_finite() && (0.0..=1.0).contains(n)),
            "whiteLead" => view.white_lead = tokens.next()?.parse::<f64>().ok().filter(|n| n.is_finite()),
            "policy" | "whiteOwnership" => {
                let mut values = Vec::with_capacity(count);
                for _ in 0..count {
                    let n = tokens.next()?.parse::<f64>().ok()?;
                    if key == "policy" {
                        values.push(if n.is_finite() { n.max(0.0) } else { 0.0 });
                    } else {
                        if !n.is_finite() { return None; }
                        // kata-raw-nn whiteOwnership is White-positive (GTP_Extensions.md);
                        // exposed Black-positive. Java converted it to side-to-move-positive
                        // for its estimate renderer.
                        values.push(-n);
                    }
                }
                if key == "policy" {
                    view.policy = Some(values);
                } else if mode == RawNetMode::Ownership {
                    view.ownership = Some(values);
                }
            }
            _ => {}
        }
    }
    (view.policy.is_some() && view.white_winrate.is_some() && view.white_lead.is_some()
        && view.symmetry.is_some() && (mode != RawNetMode::Ownership || view.ownership.is_some())).then_some(view)
}

/// Leela Zero `Network::show_heatmap`: integer thousandths in board rows, then
/// `winrate: <probability>` for the side to move. Keep the shared view in White's viewpoint.
pub(crate) fn parse_leela(text: &str, size: BoardSize, node: NodeId, to_play: Stone) -> Option<RawNetView> {
    let mut view = RawNetView::pending(RawNetMode::Policy, node);
    let count = usize::from(size.width) * usize::from(size.height);
    let mut policy = Vec::with_capacity(count);
    for line in text.lines() {
        if let Some((_, value)) = line.split_once("winrate:") {
            let n = value.split_whitespace().next()?.parse::<f64>().ok()?;
            if n.is_finite() && (0.0..=1.0).contains(&n) {
                view.white_winrate = Some(if to_play == Stone::White { n } else { 1.0 - n });
            }
        } else {
            let row: Option<Vec<u32>> = line.split_whitespace().map(|s| s.parse().ok()).collect();
            if let Some(row) = row.filter(|r| r.len() == usize::from(size.width)) {
                policy.extend(row.into_iter().map(|n| f64::from(n) / 1000.0));
            }
        }
    }
    if policy.len() != count { return None; }
    view.policy = Some(policy);
    Some(view)
}
