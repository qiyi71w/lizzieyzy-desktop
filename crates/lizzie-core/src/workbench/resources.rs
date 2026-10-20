//! Application flow for engine resources (KataGo auto setup).

use super::Workbench;
use crate::api::ResourcesView;
use crate::settings::Storage;

pub(super) struct ResourceFlow {
    /// Increases whenever the resources view changes.
    revision: u64,
    sent: Option<u64>,
}

impl ResourceFlow {
    pub(super) fn new(_storage: &Storage) -> ResourceFlow {
        ResourceFlow {
            revision: 1,
            sent: None,
        }
    }
}

impl Workbench {
    pub(super) fn resources_view(&self) -> ResourcesView {
        ResourcesView::default()
    }

    /// The resources view when it changed since the last view, or for a full snapshot.
    pub(super) fn take_resources_view(&mut self, full: bool) -> Option<ResourcesView> {
        if !full && self.resources.sent == Some(self.resources.revision) {
            return None;
        }
        self.resources.sent = Some(self.resources.revision);
        Some(self.resources_view())
    }
}
