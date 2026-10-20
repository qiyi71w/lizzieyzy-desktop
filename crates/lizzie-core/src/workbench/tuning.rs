//! Application flow for engine performance: thread policy, benchmark, measured tuning and GTP
//! configuration.

use super::Workbench;
use crate::api::TuningView;

#[derive(Default)]
pub(super) struct TuningFlow {
    /// Increases whenever the tuning view changes.
    revision: u64,
    sent: Option<u64>,
}

impl Workbench {
    pub(super) fn tuning_view(&self) -> TuningView {
        TuningView::default()
    }

    /// The tuning view when it changed since the last view, or for a full snapshot.
    pub(super) fn take_tuning_view(&mut self, full: bool) -> Option<TuningView> {
        if !full && self.tuning.sent == Some(self.tuning.revision) {
            return None;
        }
        self.tuning.sent = Some(self.tuning.revision);
        Some(self.tuning_view())
    }
}
